//! Les fenêtres des écrans, sur le fil principal : une fenêtre transparente
//! par écran, créée une fois et jamais déplacée, où la scène se pose (voir
//! [`crate::scene`]). Seul le fil principal touche aux fenêtres : il les
//! fait, leur pose la racine des calques de la scène, et décide si elles
//! prennent la souris — ce que l'affichage lui dit à chaque changement
//! (au-dessus d'un animal, oui ; ailleurs, la souris passe aux apps en dessous).

use std::sync::Arc;

use mouflon_core::{Rect, Screen, Vec2};
use mouflon_platform as platform;
use winit::dpi::{LogicalPosition, LogicalSize, PhysicalPosition, PhysicalSize};
use winit::event_loop::ActiveEventLoop;
use winit::monitor::MonitorHandle;
use winit::window::{Theme, Window, WindowId, WindowLevel};

use crate::scene::{Overlay, ScreenInfo, Surface};

/// La fenêtre d'un écran.
pub struct ScreenWindow {
    pub info: ScreenInfo,
    pub window: Arc<Window>,
    surface: Surface,
    /// Prend-elle la souris en ce moment ?
    hittest: bool,
    /// Là où elle doit être, au pixel près (coin, largeur, hauteur), quand
    /// les écrans viennent du système en pixels (voir `ScreenWindows::exact`).
    pixels: Option<[i32; 4]>,
    /// Combien de fois sa taille a été redemandée (sous Windows : 1 quand
    /// il a fallu dire qu'elle ne tient pas à sa place).
    resized: std::cell::Cell<u8>,
}

/// On redemande sa taille à une fenêtre d'écran tant de fois, pas plus.
#[cfg_attr(target_os = "windows", allow(dead_code))]
const RESIZE_TRIES: u8 = 3;

/// Le rectangle `rect` (points) en pixels, à `scale` pixels par point :
/// coin, largeur, hauteur. Les bords sont arrondis un à un, pour que deux
/// écrans voisins se touchent encore au pixel près.
#[must_use]
pub fn pixel_rect(rect: Rect, scale: f64) -> [i32; 4] {
    let px = |v: f32| (f64::from(v) * scale).round() as i32;
    let (left, top) = (px(rect.x), px(rect.y));
    [
        left,
        top,
        (px(rect.x + rect.w) - left).max(1),
        (px(rect.y + rect.h) - top).max(1),
    ]
}

/// Quatre écrans au plus.
pub const MAX_SCREENS: usize = 4;

/// Pour le journal : « 2 — 1536 × 864 en 0, 0 (sol à 816) ; … », en points.
#[must_use]
pub fn describe(screens: &[Screen]) -> String {
    let each: Vec<String> = screens
        .iter()
        .map(|s| {
            format!(
                "{:.0} × {:.0} en {:.0}, {:.0} (sol à {:.0})",
                s.frame.w,
                s.frame.h,
                s.frame.x,
                s.frame.y,
                s.usable.bottom()
            )
        })
        .collect();
    format!("{} — {}", screens.len(), each.join(" ; "))
}

/// Essais : `MOUFLON_ECRANS="0,0,640,800;640,100,640,600,40"` fait comme si
/// le bureau avait ces écrans-là (en points : coin, largeur, hauteur, puis
/// au besoin la hauteur d'une barre des tâches en bas), découpés dans le
/// vrai. Chacun reçoit sa fenêtre et son sol : de quoi essayer plusieurs
/// écrans, de toutes les dispositions, sur une machine qui n'en a qu'un.
#[must_use]
pub fn asked(scale: f32) -> Option<Vec<Screen>> {
    let screens = parse(&std::env::var("MOUFLON_ECRANS").ok()?, scale);
    (!screens.is_empty()).then_some(screens)
}

/// Les écrans décrits par `text` (voir [`asked`]) ; ceux qui sont mal
/// écrits ou sans surface sont laissés de côté.
#[must_use]
pub fn parse(text: &str, scale: f32) -> Vec<Screen> {
    text.split(';')
        .filter_map(|one| {
            let n: Vec<f32> = one
                .split(',')
                .map(|v| v.trim().parse::<f32>().ok().filter(|v| v.is_finite()))
                .collect::<Option<_>>()?;
            let (&[x, y, w, h], bar) = match n.as_slice() {
                [rect @ .., bar] if rect.len() == 4 => (<&[f32; 4]>::try_from(rect).ok()?, *bar),
                rect => (<&[f32; 4]>::try_from(rect).ok()?, 0.0),
            };
            (w >= 1.0 && h >= 1.0).then(|| Screen {
                frame: Rect::new(x, y, w, h),
                usable: Rect::new(x, y, w, (h - bar.clamp(0.0, h * 0.5)).max(1.0)),
                scale,
            })
        })
        .collect()
}

/// Les écrans où vivent les animaux, parmi ceux du système (le principal
/// d'abord) : sans ceux qui n'ont pas de surface, ni ceux qui en répètent
/// un autre (écrans en miroir, image dans l'image) ; le principal seul si
/// `only_main` ; [`MAX_SCREENS`] au plus, le principal et les plus proches
/// de lui.
#[must_use]
pub fn chosen(screens: &[Screen], only_main: bool) -> Vec<Screen> {
    let inside = |a: &Rect, b: &Rect| {
        a.left() >= b.left() - 1.0
            && a.right() <= b.right() + 1.0
            && a.top() >= b.top() - 1.0
            && a.bottom() <= b.bottom() + 1.0
    };
    let mut kept: Vec<Screen> = Vec::new();
    for screen in screens {
        let f = screen.frame;
        let real = f.x.is_finite() && f.y.is_finite() && f.w >= 1.0 && f.h >= 1.0;
        if real && !kept.iter().any(|k| inside(&f, &k.frame)) {
            kept.push(*screen);
        }
    }
    if only_main {
        kept.truncate(1);
    }
    if kept.len() > MAX_SCREENS {
        let main = kept[0].frame;
        // L'écart entre deux écrans (0 s'ils se touchent), puis la distance
        // entre leurs centres.
        let away = |s: &Screen| {
            let f = s.frame;
            let dx = (f.left() - main.right())
                .max(main.left() - f.right())
                .max(0.0);
            let dy = (f.top() - main.bottom())
                .max(main.top() - f.bottom())
                .max(0.0);
            (dx.hypot(dy), (f.center() - main.center()).length())
        };
        kept[1..].sort_by(|a, b| {
            let (a, b) = (away(a), away(b));
            a.0.total_cmp(&b.0).then(a.1.total_cmp(&b.1))
        });
        kept.truncate(MAX_SCREENS);
    }
    kept
}

/// Les fenêtres de tous les écrans.
pub struct ScreenWindows {
    windows: Vec<ScreenWindow>,
    /// Calques Core Animation quand le système en a (sinon le GPU).
    layers_wanted: bool,
    /// Rien que des calques : l'affichage tourne sur son propre fil, où le
    /// GPU n'irait pas ; un écran sans calques resterait vide.
    layers_only: bool,
    /// Les écrans viennent du système en pixels, à une seule échelle pour
    /// tout le bureau (Windows, X11) : les fenêtres se posent au pixel
    /// près, et dessinent à cette échelle.
    exact: bool,
    next_key: u64,
}

impl Default for ScreenWindows {
    fn default() -> Self {
        Self::new()
    }
}

impl ScreenWindows {
    #[must_use]
    pub fn new() -> Self {
        Self {
            windows: Vec::new(),
            // `MOUFLON_GPU=1` : le GPU même là où le système a des calques
            // (pour comparer).
            layers_wanted: !gpu_asked(),
            // Sous Windows, des calques ou rien : une fenêtre d'écran que le
            // GPU ne saurait pas rendre transparente cacherait tout le bureau.
            layers_only: cfg!(target_os = "windows") && !gpu_asked(),
            exact: false,
            next_key: 1,
        }
    }

    /// Toutes les fenêtres montrent la scène en calques (et il y en a).
    #[must_use]
    pub fn all_layers(&self) -> bool {
        !self.windows.is_empty()
            && self
                .windows
                .iter()
                .all(|w| matches!(w.surface, Surface::Layers(_)))
    }

    /// Désormais, rien que des calques (voir `layers_only`).
    pub fn set_layers_only(&mut self, on: bool) {
        self.layers_only = on;
    }

    /// Aucun écran n'a pu recevoir sa fenêtre.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.windows.is_empty()
    }

    /// Le compositeur du système a perdu son appareil graphique (voir
    /// `platform::scene_lost`) : la scène est reposée dans chaque fenêtre,
    /// qui change de clé (l'affichage refait alors tous ses calques). `false`
    /// si l'une d'elles n'a pas pu la recevoir : à réessayer plus tard.
    pub fn reattach(&mut self) -> bool {
        let mut all = true;
        for w in &mut self.windows {
            if !matches!(w.surface, Surface::Layers(_)) {
                continue;
            }
            match platform::attach_scene(&*w.window) {
                Some(root) => {
                    w.surface = Surface::Layers(root);
                    w.info.key = self.next_key;
                    self.next_key += 1;
                }
                None => all = false,
            }
        }
        all
    }

    /// Vérifie de temps en temps que chaque fenêtre d'écran a toujours ses
    /// styles (Windows : une fenêtre qui prendrait la souris sans raison
    /// rendrait le bureau inutilisable) et couvre toujours son écran, au
    /// pixel près (un écran dont on change la mise à l'échelle fait
    /// redimensionner sa fenêtre : les animaux y seraient coupés).
    pub fn check(&self) {
        for w in &self.windows {
            #[cfg(target_os = "windows")]
            if platform::check_pet_window(&*w.window, w.hittest) {
                log::warn!("fenêtre de l'écran : styles remis en place");
            }
            if w.place() {
                log::warn!(
                    "fenêtre de l'écran {:.0} × {:.0} en {:.0}, {:.0} : remise à sa place",
                    w.info.rect.w,
                    w.info.rect.h,
                    w.info.rect.x,
                    w.info.rect.y
                );
            }
        }
    }

    /// Une fenêtre par écran. Quand les écrans changent, celles des écrans
    /// restés tels quels sont gardées (leurs animaux ne clignotent pas),
    /// les autres refaites. `true` si quelque chose a changé. `exact` :
    /// voir le champ du même nom.
    pub fn sync(&mut self, event_loop: &ActiveEventLoop, screens: &[Screen], exact: bool) -> bool {
        self.exact = exact;
        let fits = |w: &ScreenWindow, s: &Screen| {
            w.info.rect == s.frame
                && w.pixels == exact.then(|| pixel_rect(s.frame, f64::from(s.scale)))
        };
        let same = self.windows.len() == screens.len()
            && screens
                .iter()
                .all(|s| self.windows.iter().any(|w| fits(w, s)));
        if same {
            return false;
        }
        let mut old = std::mem::take(&mut self.windows);
        for screen in screens {
            if let Some(i) = old.iter().position(|w| fits(w, screen)) {
                self.windows.push(old.swap_remove(i));
                continue;
            }
            match self.open(event_loop, screen.frame, f64::from(screen.scale)) {
                Ok(window) => self.windows.push(window),
                Err(e) => log::error!("fenêtre de l'écran impossible : {e}"),
            }
        }
        for gone in old {
            platform::keep_pet_window(&*gone.window);
        }
        true
    }

    /// La fenêtre de l'écran `rect` (points), dont un point vaut `scale`
    /// pixels d'après le système.
    fn open(
        &mut self,
        event_loop: &ActiveEventLoop,
        rect: Rect,
        scale: f64,
    ) -> Result<ScreenWindow, String> {
        let attrs = Window::default_attributes()
            .with_title("Mouflon")
            .with_decorations(false)
            .with_transparent(true)
            .with_resizable(false)
            .with_window_level(WindowLevel::AlwaysOnTop)
            .with_inner_size(LogicalSize::new(f64::from(rect.w), f64::from(rect.h)))
            .with_position(LogicalPosition::new(f64::from(rect.x), f64::from(rect.y)))
            .with_active(false);
        #[cfg(target_os = "macos")]
        let attrs = {
            use winit::platform::macos::WindowAttributesExtMacOS;
            attrs.with_has_shadow(false).with_accepts_first_mouse(true)
        };
        // Sous Windows et X11, le repère commun a une seule échelle pour
        // tous les écrans (voir `mouflon_platform`) : la fenêtre se pose au
        // pixel près, sans passer par l'échelle que winit prête à son écran.
        let pixels = self.exact.then(|| pixel_rect(rect, scale));
        let attrs = match pixels {
            Some([x, y, width, height]) => attrs
                .with_inner_size(PhysicalSize::new(width as u32, height as u32))
                .with_position(PhysicalPosition::new(x, y)),
            None => attrs,
        };
        // Sous Windows, elle appartient à une fenêtre cachée (ni barre des
        // tâches, ni Alt+Tab), et n'a pas d'image à elle : seule compte la
        // surface transparente où le GPU dessine.
        #[cfg(target_os = "windows")]
        let attrs = {
            use winit::platform::windows::WindowAttributesExtWindows;
            let attrs = attrs
                .with_skip_taskbar(true)
                .with_no_redirection_bitmap(true);
            match platform::overlay_owner() {
                Some(owner) => attrs.with_owner_window(owner),
                None => attrs,
            }
        };
        // Sous X11, une fenêtre d'outil : sans bordure ni place dans la
        // barre des tâches (le reste se dit après, voir `prepare_pet_window`).
        #[cfg(all(unix, not(target_os = "macos")))]
        let attrs = {
            use winit::platform::x11::{WindowAttributesExtX11, WindowType};
            attrs.with_x11_window_type(vec![WindowType::Utility])
        };
        let window = Arc::new(event_loop.create_window(attrs).map_err(|e| e.to_string())?);
        // Elle laisse passer la souris, sauf au-dessus d'un animal.
        let _ = window.set_cursor_hittest(false);
        platform::prepare_pet_window(&*window);
        let layers = self
            .layers_wanted
            .then(|| platform::attach_scene(&*window))
            .flatten();
        let surface = match layers {
            Some(root) => Surface::Layers(root),
            None if self.layers_only => {
                platform::keep_pet_window(&*window);
                return Err(format!("calques {} indisponibles", platform::LAYERS));
            }
            None => Surface::Gpu(Arc::clone(&window)),
        };
        let key = self.next_key;
        self.next_key += 1;
        let opened = ScreenWindow {
            info: ScreenInfo {
                key,
                rect,
                scale: if self.exact && scale > 0.0 {
                    scale
                } else {
                    window.scale_factor()
                },
                hz: window_hz(&window),
            },
            window,
            surface,
            hittest: false,
            pixels,
            resized: std::cell::Cell::new(0),
        };
        // winit a pu la poser ailleurs, ou à une autre taille (un écran
        // dont la mise à l'échelle n'est pas celle de l'écran principal).
        if opened.place() {
            log::info!(
                "fenêtre de l'écran {:.0} × {:.0} en {:.0}, {:.0} : posée au pixel près",
                rect.w,
                rect.h,
                rect.x,
                rect.y
            );
        }
        Ok(opened)
    }

    /// Ce que la scène doit savoir des fenêtres.
    #[must_use]
    pub fn overlays(&self) -> Vec<Overlay> {
        self.windows
            .iter()
            .map(|w| Overlay {
                info: w.info,
                surface: w.surface.clone(),
            })
            .collect()
    }

    /// La fenêtre d'écran `id`, s'il en est une.
    #[must_use]
    pub fn get(&self, id: WindowId) -> Option<&ScreenWindow> {
        self.windows.iter().find(|w| w.window.id() == id)
    }

    /// La fenêtre `key` prend la souris (au-dessus d'un animal) ou la
    /// laisse passer.
    pub fn set_hittest(&mut self, key: u64, on: bool) {
        if let Some(w) = self.windows.iter_mut().find(|w| w.info.key == key)
            && w.hittest != on
        {
            // Sous Windows, seul le style qui laisse passer la souris change
            // (la fenêtre reste « en couches » : rien ne clignote) ; winit,
            // lui, croit toujours qu'elle la laisse passer, et ne la rendra
            // jamais opaque à la souris de lui-même.
            #[cfg(target_os = "windows")]
            platform::set_pet_window_hittest(&*w.window, on);
            #[cfg(not(target_os = "windows"))]
            {
                let _ = w.window.set_cursor_hittest(on);
                platform::prepare_pet_window(&*w.window);
            }
            w.hittest = on;
        }
    }

    /// La taille que doit garder la fenêtre d'écran `id`, en pixels, quand
    /// le système annonce que son échelle change (winit, sinon, la
    /// redimensionnerait pour lui garder sa taille « en points »).
    #[must_use]
    pub fn kept_size(&self, id: WindowId) -> Option<PhysicalSize<u32>> {
        let [_, _, width, height] = self.get(id)?.pixels?;
        Some(PhysicalSize::new(width as u32, height as u32))
    }

    /// L'échelle de la fenêtre `id` a changé : `true` si c'est celle d'un écran.
    pub fn scale_changed(&mut self, id: WindowId, scale: f64) -> bool {
        // L'échelle est alors celle du repère commun, pas celle que winit
        // prête à l'écran de la fenêtre.
        if self.exact {
            return false;
        }
        match self.windows.iter_mut().find(|w| w.window.id() == id) {
            Some(w) if w.info.scale != scale => {
                w.info.scale = scale;
                true
            }
            _ => false,
        }
    }

    /// La fréquence de chaque écran, relue : `true` si l'une a changé.
    pub fn refresh_hz(&mut self) -> bool {
        let mut changed = false;
        for w in &mut self.windows {
            let hz = window_hz(&w.window);
            if (hz - w.info.hz).abs() > 0.5 {
                w.info.hz = hz;
                changed = true;
            }
        }
        changed
    }

    /// Le thème du système, tel que le voient les fenêtres.
    #[must_use]
    pub fn dark(&self) -> Option<bool> {
        let theme = self.windows.first()?.window.theme()?;
        Some(theme == Theme::Dark)
    }

    /// La position d'un événement de la fenêtre `id`, dans le repère commun.
    #[must_use]
    pub fn to_global(&self, id: WindowId, position: PhysicalPosition<f64>) -> Option<Vec2> {
        let w = self.get(id)?;
        let local = position.to_logical::<f64>(w.info.scale);
        Some(Vec2::new(
            w.info.rect.x + local.x as f32,
            w.info.rect.y + local.y as f32,
        ))
    }

    /// Avant de quitter : les fenêtres ne seront jamais libérées (voir
    /// `platform::keep_pet_window`).
    pub fn shutdown(&self) {
        for w in &self.windows {
            platform::keep_pet_window(&*w.window);
        }
    }
}

impl ScreenWindow {
    /// La remet exactement sur son écran si elle n'y est plus (voir
    /// `pixels`) : `true` s'il a fallu la bouger.
    fn place(&self) -> bool {
        let Some(pixels) = self.pixels else {
            return false;
        };
        #[cfg(target_os = "windows")]
        {
            match platform::place_pet_window(&*self.window, pixels) {
                Some(moved) => {
                    self.resized.set(0);
                    moved
                }
                None => {
                    // (Dit une fois : la vérification revient sans cesse.)
                    if self.resized.replace(1) == 0 {
                        log::error!("fenêtre de l'écran : Windows ne la laisse pas à sa place");
                    }
                    false
                }
            }
        }
        #[cfg(not(target_os = "windows"))]
        {
            // Sa taille seulement : sa place, c'est le gestionnaire de
            // fenêtres qui en décide, et il ne sert à rien de la lui
            // disputer. S'il tient à une autre taille, on n'insiste pas.
            let [_, _, width, height] = pixels;
            let size = self.window.inner_size();
            if size.width == width as u32 && size.height == height as u32 {
                return false;
            }
            if self.resized.get() >= RESIZE_TRIES {
                return false;
            }
            self.resized.set(self.resized.get() + 1);
            let _ = self
                .window
                .request_inner_size(PhysicalSize::new(width as u32, height as u32));
            true
        }
    }
}

/// `MOUFLON_GPU=1` : la scène par le GPU, même là où le système a des calques.
#[must_use]
pub fn gpu_asked() -> bool {
    std::env::var("MOUFLON_GPU").is_ok_and(|v| !v.is_empty() && v != "0")
}

/// Fréquence de l'écran d'une fenêtre, en images par seconde : celle que
/// donne macOS, sinon celle de winit, sinon 60.
fn window_hz(window: &Window) -> f64 {
    platform::window_max_fps(window)
        .or_else(|| {
            window
                .current_monitor()
                .as_ref()
                .and_then(MonitorHandle::refresh_rate_millihertz)
                .map(|mhz| f64::from(mhz) / 1000.0)
        })
        .map_or(60.0, |hz| hz.clamp(30.0, 240.0))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn screen(x: f32, y: f32, w: f32, h: f32) -> Screen {
        Screen {
            frame: Rect::new(x, y, w, h),
            usable: Rect::new(x, y, w, h - 40.0),
            scale: 1.0,
        }
    }

    #[test]
    fn at_most_four_screens_the_main_one_and_those_nearest_to_it() {
        let main = screen(0.0, 0.0, 1920.0, 1080.0);
        let six = [
            main,
            screen(5760.0, 0.0, 1920.0, 1080.0),
            screen(1920.0, 0.0, 1920.0, 1080.0),
            screen(-1920.0, 0.0, 1920.0, 1080.0),
            screen(3840.0, 0.0, 1920.0, 1080.0),
            screen(0.0, -1080.0, 1920.0, 1080.0),
        ];
        let kept = chosen(&six, false);
        assert_eq!(kept.len(), MAX_SCREENS);
        assert_eq!(kept[0], main, "le principal d'abord");
        // Ses trois voisins ; ni celui d'après, ni le plus loin.
        for near in [six[2], six[3], six[5]] {
            assert!(kept.contains(&near), "{:?}", near.frame);
        }
        // Quatre ou moins : tous, dans l'ordre du système.
        assert_eq!(chosen(&six[..3], false), six[..3]);
        assert_eq!(chosen(&six[..1], false), six[..1]);
        assert!(chosen(&[], false).is_empty());
    }

    #[test]
    fn only_the_main_screen_when_asked() {
        let two = [
            screen(0.0, 0.0, 1920.0, 1080.0),
            screen(1920.0, 0.0, 1920.0, 1080.0),
        ];
        assert_eq!(chosen(&two, true), two[..1]);
    }

    #[test]
    fn mirrored_empty_and_nested_screens_are_left_out() {
        let main = screen(0.0, 0.0, 1920.0, 1080.0);
        let other = screen(1920.0, 0.0, 1280.0, 1024.0);
        let screens = [
            main,
            // En miroir (au demi-point près), vide, image dans l'image, pas
            // un nombre.
            screen(0.4, 0.0, 1919.6, 1080.0),
            screen(4000.0, 0.0, 0.0, 1080.0),
            screen(100.0, 100.0, 640.0, 480.0),
            screen(f32::NAN, 0.0, 800.0, 600.0),
            other,
        ];
        assert_eq!(chosen(&screens, false), [main, other]);
    }

    #[test]
    fn neighbouring_screens_still_touch_once_in_pixels() {
        // Deux écrans voisins à 125 % : 1536 points font 1920 pixels.
        assert_eq!(
            pixel_rect(Rect::new(0.0, 0.0, 1536.0, 864.0), 1.25),
            [0, 0, 1920, 1080]
        );
        assert_eq!(
            pixel_rect(Rect::new(-1024.0, 44.8, 1024.0, 819.2), 1.25),
            [-1280, 56, 1280, 1024]
        );
        // Des bords qui tombent entre deux pixels : arrondis un à un, les
        // deux écrans se touchent encore.
        let left = pixel_rect(Rect::new(0.0, 0.0, 1092.8, 614.4), 1.25);
        let right = pixel_rect(Rect::new(1092.8, 0.0, 1024.3, 614.4), 1.25);
        assert_eq!(left[0] + left[2], right[0]);
        assert_eq!(pixel_rect(Rect::new(0.0, 0.0, 0.1, 0.1), 1.0), [0, 0, 1, 1]);
    }

    #[test]
    fn simulated_screens_are_read_from_their_text() {
        let screens = parse("0,0,640,800; 640,100,640,600,40 ;;rien;1,2,3", 2.0);
        assert_eq!(screens.len(), 2);
        assert_eq!(screens[0].frame, Rect::new(0.0, 0.0, 640.0, 800.0));
        assert_eq!(screens[0].usable, screens[0].frame);
        assert_eq!(screens[1].frame, Rect::new(640.0, 100.0, 640.0, 600.0));
        assert_eq!(screens[1].usable, Rect::new(640.0, 100.0, 640.0, 560.0));
        assert_eq!(screens[1].scale, 2.0);
        // Sans surface, ou une barre plus haute que l'écran.
        assert!(parse("0,0,0,800", 1.0).is_empty());
        assert_eq!(
            parse("0,0,100,100,900", 1.0)[0].usable,
            Rect::new(0.0, 0.0, 100.0, 50.0)
        );
        assert_eq!(
            describe(&screens),
            "2 — 640 × 800 en 0, 0 (sol à 800) ; 640 × 600 en 640, 100 (sol à 660)"
        );
    }
}
