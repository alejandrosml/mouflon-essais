//! Windows : écrans, fenêtres des autres apps, curseur, par l'API Win32 ; et,
//! dans ses modules, tout ce que Mouflon demande d'autre au système :
//!
//! - [`scene`] : les calques de la scène (DirectComposition), que Windows
//!   compose lui-même ;
//! - [`clock`] : l'horloge de l'écran, pour le fil de l'affichage ;
//! - [`host`] : la fenêtre cachée du fil principal, qui reçoit ce que le
//!   système dit à Mouflon (raccourcis, touche Échap, second lancement) ;
//! - [`menu`], [`form`], [`gallery`], [`about`] : les menus surgissants, la
//!   fenêtre des réglages, la galerie des animaux, « À propos » ;
//! - [`keys`], [`login`], [`sound`] : raccourcis clavier, ouverture de
//!   session, sons.
//!
//! Le repère commun est en points, origine en haut à gauche de l'écran
//! principal. Windows, lui, compte en pixels, et chaque écran peut avoir son
//! échelle. Mouflon prend **une seule échelle pour tous les écrans**, celle de
//! l'écran principal : un point vaut ce nombre de pixels partout, et le
//! bureau reste d'un seul tenant (un mouton passe d'un écran à l'autre sans
//! changer de taille en pixels).

mod about;
mod clock;
mod form;
mod gallery;
mod host;
mod keys;
mod login;
mod menu;
mod scene;
mod sound;
mod ui;

use std::cell::RefCell;
use std::collections::HashMap;
use std::ffi::c_void;
use std::sync::{Mutex, OnceLock};
use std::time::Duration;

use windows_sys::Win32::Foundation::{CloseHandle, HWND, LPARAM, MAX_PATH, POINT, RECT};
use windows_sys::Win32::Globalization::GetUserDefaultLocaleName;
use windows_sys::Win32::Graphics::Dwm::{
    DWMWA_CLOAKED, DWMWA_EXTENDED_FRAME_BOUNDS, DwmGetWindowAttribute,
};
use windows_sys::Win32::Graphics::Gdi::{
    EnumDisplayMonitors, GetMonitorInfoW, HDC, HMONITOR, MONITORINFO,
};
use windows_sys::Win32::System::SystemInformation::GetTickCount;
use windows_sys::Win32::System::Threading::{
    GetCurrentThread, OpenProcess, PROCESS_QUERY_LIMITED_INFORMATION, QueryFullProcessImageNameW,
    SetThreadPriority, THREAD_PRIORITY_ABOVE_NORMAL, THREAD_PRIORITY_BELOW_NORMAL,
};
use windows_sys::Win32::UI::HiDpi::{
    DPI_AWARENESS_CONTEXT_PER_MONITOR_AWARE_V2, GetDpiForMonitor, MDT_EFFECTIVE_DPI,
    SetProcessDpiAwarenessContext,
};
use windows_sys::Win32::UI::Input::KeyboardAndMouse::{GetLastInputInfo, LASTINPUTINFO};
use windows_sys::Win32::UI::WindowsAndMessaging::{
    EnumWindows, GWL_EXSTYLE, GetClassNameW, GetCursorPos, GetForegroundWindow, GetWindowLongPtrW,
    GetWindowRect, GetWindowThreadProcessId, HWND_TOPMOST, IsIconic, IsWindowVisible, LWA_ALPHA,
    MONITORINFOF_PRIMARY, SWP_NOACTIVATE, SWP_NOMOVE, SWP_NOOWNERZORDER, SWP_NOSIZE, SWP_NOZORDER,
    SetLayeredWindowAttributes, SetWindowLongPtrW, SetWindowPos, WS_EX_LAYERED, WS_EX_NOACTIVATE,
    WS_EX_TOOLWINDOW, WS_EX_TOPMOST, WS_EX_TRANSPARENT,
};

use super::{OsRect, OsScreen, OsWindow, UserActivity};

pub use about::{show_about, snapshot_about};
pub use clock::ThreadClock;
pub use form::{show_form, snapshot_form, update_form};
pub use gallery::{set_gallery_preview, show_gallery, snapshot_gallery};
pub use host::{already_running, overlay_owner, set_ui_handler};
pub use keys::{listen_escape, set_shortcuts};
pub use login::{login_item, login_item_available, open_login_items_settings, set_login_item};
pub use menu::pop_up_menu;
pub use scene::{
    SceneImage, SceneLayer, attach_scene, detach_scene, scene_generation, scene_lost,
    scene_problem, scene_transaction,
};
pub use sound::{play_sound, warm_up_sounds};
pub use ui::message;

/// Un texte pour Windows : UTF-16, terminé par zéro.
fn wide(text: &str) -> Vec<u16> {
    text.encode_utf16().chain(std::iter::once(0)).collect()
}

/// Le texte UTF-16 d'un tampon, jusqu'à son zéro.
fn from_wide(buffer: &[u16]) -> String {
    let end = buffer.iter().position(|&c| c == 0).unwrap_or(buffer.len());
    String::from_utf16_lossy(&buffer[..end])
}

/// Le processus voit les vrais pixels de chaque écran (sinon Windows lui
/// donnerait des tailles arrangées). winit le demande aussi en démarrant ;
/// le premier qui le dit l'emporte, le second est ignoré.
fn see_real_pixels() {
    static ONCE: OnceLock<()> = OnceLock::new();
    ONCE.get_or_init(|| {
        // SAFETY: l'appel ne prend qu'une constante ; il échoue sans effet si
        // le choix est déjà fait pour ce processus.
        unsafe {
            SetProcessDpiAwarenessContext(DPI_AWARENESS_CONTEXT_PER_MONITOR_AWARE_V2);
        }
    });
}

/// Un rectangle en pixels : gauche, haut, droite, bas.
type Pixels = [i32; 4];

fn pixels(rect: RECT) -> Pixels {
    [rect.left, rect.top, rect.right, rect.bottom]
}

/// Un écran tel que Windows le donne : en pixels, avec son échelle.
#[derive(Clone, Copy, Debug, PartialEq)]
struct Monitor {
    frame: Pixels,
    work: Pixels,
    scale: f64,
    primary: bool,
}

/// Les écrans, en pixels (le principal d'abord).
fn monitors() -> Vec<Monitor> {
    see_real_pixels();
    /// Appelée par Windows pour chaque écran : `data` est le `Vec` à remplir.
    unsafe extern "system" fn each(
        monitor: HMONITOR,
        _: HDC,
        _: *mut RECT,
        data: LPARAM,
    ) -> windows_sys::core::BOOL {
        // SAFETY: `data` est le pointeur vers le `Vec<Monitor>` de
        // `monitors`, vivant et seul à y accéder pendant l'énumération.
        let found = unsafe { &mut *(data as *mut Vec<Monitor>) };
        let mut info = MONITORINFO {
            cbSize: size_of::<MONITORINFO>() as u32,
            rcMonitor: RECT::default(),
            rcWork: RECT::default(),
            dwFlags: 0,
        };
        let (mut dpi_x, mut dpi_y) = (96u32, 96u32);
        // SAFETY: `monitor` vient de Windows pour la durée de l'appel ;
        // `info` porte sa taille, les deux entiers sont à nous.
        let known = unsafe {
            let known = GetMonitorInfoW(monitor, &mut info) != 0;
            if GetDpiForMonitor(monitor, MDT_EFFECTIVE_DPI, &mut dpi_x, &mut dpi_y) != 0 {
                dpi_x = 96;
            }
            known
        };
        if known {
            found.push(Monitor {
                frame: pixels(info.rcMonitor),
                work: pixels(info.rcWork),
                scale: (f64::from(dpi_x) / 96.0).clamp(0.5, 8.0),
                primary: info.dwFlags & MONITORINFOF_PRIMARY != 0,
            });
        }
        1
    }
    let mut found: Vec<Monitor> = Vec::new();
    // SAFETY: `each` reçoit le pointeur vers `found`, qui vit jusqu'au retour
    // de l'appel (l'énumération est synchrone).
    unsafe {
        EnumDisplayMonitors(
            std::ptr::null_mut(),
            std::ptr::null(),
            Some(each),
            &raw mut found as LPARAM,
        );
    }
    found.sort_by_key(|m| !m.primary);
    note_monitors(&found);
    found
}

/// Les écrans au journal, à chaque fois qu'ils changent : en pixels, comme
/// Windows les range, chacun avec sa mise à l'échelle. Sur une machine
/// qu'on n'a pas sous la main, c'est ce qui dit comment les écrans sont
/// disposés (et s'ils n'ont pas tous la même échelle, ce qui a longtemps
/// coupé les moutons : voir [`place_pet_window`]).
fn note_monitors(monitors: &[Monitor]) {
    static NOTED: Mutex<Vec<Monitor>> = Mutex::new(Vec::new());
    let Ok(mut noted) = NOTED.lock() else {
        return;
    };
    if *noted == monitors {
        return;
    }
    noted.clear();
    noted.extend_from_slice(monitors);
    log::info!("écrans de Windows : {}", describe_monitors(monitors));
}

/// « 2 — 1920 × 1080 en 0, 0 (principal, 125 %, zone de travail
/// 1920 × 1020 en 0, 0) ; 1280 × 1024 en -1280, 56 (100 %, …) ».
fn describe_monitors(monitors: &[Monitor]) -> String {
    let rect = |[left, top, right, bottom]: Pixels| {
        format!("{} × {} en {left}, {top}", right - left, bottom - top)
    };
    let each: Vec<String> = monitors
        .iter()
        .map(|m| {
            format!(
                "{} ({}{:.0} %, zone de travail {})",
                rect(m.frame),
                if m.primary { "principal, " } else { "" },
                m.scale * 100.0,
                rect(m.work)
            )
        })
        .collect();
    format!("{} — {}", monitors.len(), each.join(" ; "))
}

/// Pixels par point : l'échelle de l'écran principal, pour tout le bureau.
fn world_scale() -> f64 {
    monitors().first().map_or(1.0, |m| m.scale)
}

fn to_points([left, top, right, bottom]: Pixels, scale: f64) -> OsRect {
    OsRect {
        x: f64::from(left) / scale,
        y: f64::from(top) / scale,
        w: f64::from(right - left) / scale,
        h: f64::from(bottom - top) / scale,
    }
}

/// Les écrans : cadre complet, zone de travail (sans la barre des tâches),
/// et l'échelle du bureau (la même pour tous, voir le module).
#[must_use]
pub fn screens() -> Vec<OsScreen> {
    let monitors = monitors();
    let scale = monitors.first().map_or(1.0, |m| m.scale);
    monitors
        .iter()
        .map(|m| OsScreen {
            frame: to_points(m.frame, scale),
            visible: to_points(m.work, scale),
            scale,
        })
        .collect()
}

/// Le curseur, en points.
#[must_use]
pub fn cursor_position() -> Option<(f64, f64)> {
    let mut at = POINT { x: 0, y: 0 };
    // SAFETY: `at` est à nous ; l'appel n'écrit que dedans.
    if unsafe { GetCursorPos(&mut at) } == 0 {
        return None;
    }
    let scale = cached_scale();
    Some((f64::from(at.x) / scale, f64::from(at.y) / scale))
}

/// L'échelle du bureau, relue de temps en temps (elle ne change que quand
/// l'utilisateur change ses réglages d'affichage) : le curseur est demandé
/// à chaque image.
fn cached_scale() -> f64 {
    static CACHE: Mutex<Option<(std::time::Instant, f64)>> = Mutex::new(None);
    let Ok(mut cache) = CACHE.lock() else {
        return world_scale();
    };
    match *cache {
        Some((at, scale)) if at.elapsed() < Duration::from_secs(2) => scale,
        _ => {
            let scale = world_scale();
            *cache = Some((std::time::Instant::now(), scale));
            scale
        }
    }
}

/// Identifiant du processus d'une fenêtre.
fn pid_of(window: HWND) -> u32 {
    let mut pid = 0u32;
    // SAFETY: `window` vient de Windows ; s'il n'existe plus, l'appel rend 0.
    unsafe {
        GetWindowThreadProcessId(window, &mut pid);
    }
    pid
}

/// Le nom du programme d'un processus (« notepad.exe »), gardé en mémoire :
/// le relevé des fenêtres revient plusieurs fois par seconde.
fn process_name(pid: u32) -> String {
    thread_local! {
        static NAMES: RefCell<HashMap<u32, String>> = RefCell::new(HashMap::new());
    }
    NAMES.with(|names| {
        let mut names = names.borrow_mut();
        if names.len() > 512 {
            names.clear();
        }
        names
            .entry(pid)
            .or_insert_with(|| {
                let mut buffer = [0u16; MAX_PATH as usize];
                let mut len = buffer.len() as u32;
                // SAFETY: le processus est ouvert pour une simple question,
                // sa poignée est refermée avant de sortir ; `buffer` et `len`
                // sont à nous et décrivent la même taille.
                let known = unsafe {
                    let process = OpenProcess(PROCESS_QUERY_LIMITED_INFORMATION, 0, pid);
                    if process.is_null() {
                        false
                    } else {
                        let known =
                            QueryFullProcessImageNameW(process, 0, buffer.as_mut_ptr(), &mut len)
                                != 0;
                        CloseHandle(process);
                        known
                    }
                };
                if !known {
                    return String::new();
                }
                let path = String::from_utf16_lossy(&buffer[..(len as usize).min(buffer.len())]);
                path.rsplit(['\\', '/'])
                    .next()
                    .unwrap_or_default()
                    .to_owned()
            })
            .clone()
    })
}

/// Une fenêtre telle que le relevé la voit : sa classe, ses styles étendus.
/// Est-ce une fenêtre où marcher (ou qui cache ce qui est derrière elle) ?
fn is_real_window(class: &str, ex_style: u32) -> bool {
    // Le bureau lui-même et la barre des tâches ne sont pas des fenêtres
    // où marcher (la zone de travail s'arrête déjà à la barre).
    if matches!(
        class,
        "Progman" | "WorkerW" | "Shell_TrayWnd" | "Shell_SecondaryTrayWnd"
    ) {
        return false;
    }
    // Les petites fenêtres d'outils sans barre de titre (bulles, voyants).
    if ex_style & WS_EX_TOOLWINDOW != 0 && ex_style & WS_EX_TOPMOST == 0 {
        return false;
    }
    // Une fenêtre qui laisse passer la souris est un calque posé sur
    // l'écran (un autre compagnon de bureau, l'incrustation d'un jeu ou
    // d'une carte graphique) : ni sol, ni obstacle.
    if ex_style & WS_EX_LAYERED != 0 && ex_style & WS_EX_TRANSPARENT != 0 {
        return false;
    }
    true
}

/// Les fenêtres du bureau, de la plus en avant à la plus en arrière.
#[must_use]
pub fn windows() -> Vec<OsWindow> {
    /// Appelée par Windows pour chaque fenêtre : `data` est le `Vec` à remplir.
    unsafe extern "system" fn each(window: HWND, data: LPARAM) -> windows_sys::core::BOOL {
        // SAFETY: `data` est le pointeur vers le `Vec` de `windows`, vivant
        // et seul à y accéder pendant l'énumération.
        let found = unsafe { &mut *(data as *mut Vec<(HWND, RECT, bool)>) };
        let mut cloaked = 0u32;
        let mut frame = RECT::default();
        let mut class = [0u16; 64];
        // SAFETY: `window` vient de Windows pour la durée de l'appel ; chaque
        // tampon est à nous, donné avec sa taille.
        let (bounded, style) = unsafe {
            // Cachée ou réduite : rien d'autre à lui demander.
            if IsWindowVisible(window) == 0 || IsIconic(window) != 0 {
                return 1;
            }
            DwmGetWindowAttribute(
                window,
                DWMWA_CLOAKED as u32,
                (&raw mut cloaked).cast::<c_void>(),
                size_of::<u32>() as u32,
            );
            let bounded = DwmGetWindowAttribute(
                window,
                DWMWA_EXTENDED_FRAME_BOUNDS as u32,
                (&raw mut frame).cast::<c_void>(),
                size_of::<RECT>() as u32,
            ) == 0;
            GetClassNameW(window, class.as_mut_ptr(), class.len() as i32);
            (bounded, GetWindowLongPtrW(window, GWL_EXSTYLE) as u32)
        };
        // Sur un autre bureau (« voilée »), ou sans taille.
        if !bounded || cloaked != 0 || frame.right <= frame.left || frame.bottom <= frame.top {
            return 1;
        }
        if is_real_window(&from_wide(&class), style) {
            found.push((window, frame, style & WS_EX_TOPMOST != 0));
        }
        1
    }
    let mut found: Vec<(HWND, RECT, bool)> = Vec::new();
    // SAFETY: `each` reçoit le pointeur vers `found`, qui vit jusqu'au retour
    // de l'appel (l'énumération est synchrone).
    unsafe {
        EnumWindows(Some(each), &raw mut found as LPARAM);
    }
    let scale = cached_scale();
    let own = std::process::id();
    found
        .into_iter()
        .filter_map(|(window, frame, topmost)| {
            let pid = pid_of(window);
            // Les fenêtres de Mouflon (ses écrans transparents, ses réglages)
            // ne comptent pas.
            (pid != own && pid != 0).then(|| OsWindow {
                id: window as usize as u64,
                pid: pid as i32,
                frame: to_points(pixels(frame), scale),
                // Une fenêtre « toujours devant » flotte au-dessus des autres.
                layer: if topmost { 3 } else { 0 },
                owner: process_name(pid),
            })
        })
        .collect()
}

/// Le processus de la fenêtre au premier plan.
#[must_use]
pub fn frontmost_pid() -> Option<i32> {
    // SAFETY: sans argument ; rend une poignée, ou rien.
    let window = unsafe { GetForegroundWindow() };
    if window.is_null() {
        return None;
    }
    match pid_of(window) {
        0 => None,
        pid => Some(pid as i32),
    }
}

/// Depuis quand l'utilisateur n'a rien fait. Windows ne distingue pas le
/// clavier de la souris : on ne sait pas s'il tape.
#[must_use]
pub fn user_activity() -> Option<UserActivity> {
    let mut last = LASTINPUTINFO {
        cbSize: size_of::<LASTINPUTINFO>() as u32,
        dwTime: 0,
    };
    // SAFETY: `last` est à nous et porte sa taille ; l'horloge ne prend rien.
    let now = unsafe {
        if GetLastInputInfo(&mut last) == 0 {
            return None;
        }
        GetTickCount()
    };
    Some(UserActivity {
        since_key: f64::INFINITY,
        idle: f64::from(now.wrapping_sub(last.dwTime)) / 1000.0,
    })
}

/// La poignée Win32 d'une fenêtre de winit.
fn hwnd_of(window: &impl raw_window_handle::HasWindowHandle) -> Option<HWND> {
    match window.window_handle().ok()?.as_raw() {
        raw_window_handle::RawWindowHandle::Win32(handle) => Some(handle.hwnd.get() as HWND),
        _ => None,
    }
}

/// Les styles d'une fenêtre d'écran de Mouflon : elle ne prend jamais le
/// clavier (un clic sur un mouton ne retire pas la main à l'app où l'on
/// travaillait), reste hors de la barre des tâches et d'Alt+Tab, et laisse
/// passer la souris tant que `hittest` est faux. C'est toujours une fenêtre
/// « en couches » : seul le style qui laisse passer la souris change quand
/// le curseur arrive sur un mouton ou le quitte, rien ne clignote.
fn pet_window_style(hwnd: HWND, hittest: bool) {
    // SAFETY: `hwnd` est une fenêtre vivante de ce processus (winit la
    // tient) ; on ne fait que régler ses styles étendus.
    let layered_before = unsafe {
        let style = GetWindowLongPtrW(hwnd, GWL_EXSTYLE) as u32;
        let mut wanted = style | WS_EX_NOACTIVATE | WS_EX_TOOLWINDOW | WS_EX_LAYERED;
        if hittest {
            wanted &= !WS_EX_TRANSPARENT;
        } else {
            wanted |= WS_EX_TRANSPARENT;
        }
        if wanted != style {
            SetWindowLongPtrW(hwnd, GWL_EXSTYLE, wanted as isize);
        }
        style & WS_EX_LAYERED != 0
    };
    if !layered_before {
        show_layered(hwnd);
    }
}

/// Windows ne montre une fenêtre « en couches » qu'une fois son opacité
/// dite : entière (ce qui se voit d'elle, ses calques, a sa propre
/// transparence).
fn show_layered(hwnd: HWND) {
    // SAFETY: `hwnd` est une fenêtre vivante de ce processus.
    unsafe {
        SetLayeredWindowAttributes(hwnd, 0, 255, LWA_ALPHA);
    }
}

/// Une fenêtre d'écran de Mouflon vient d'être créée : elle prend ses styles
/// (voir [`set_pet_window_hittest`]) et laisse passer la souris. (Son image
/// vient des calques de la scène, transparente là où rien n'est dessiné.)
pub fn prepare_pet_window(window: &impl raw_window_handle::HasWindowHandle) {
    if let Some(hwnd) = hwnd_of(window) {
        pet_window_style(hwnd, false);
        // (winit a pu la mettre « en couches » avant nous, sans rien dire
        // de son opacité.)
        show_layered(hwnd);
    }
}

/// Une fenêtre d'écran prend la souris (le curseur est sur un mouton, ou
/// l'on en tient un), ou la laisse passer aux apps en dessous.
pub fn set_pet_window_hittest(window: &impl raw_window_handle::HasWindowHandle, on: bool) {
    if let Some(hwnd) = hwnd_of(window) {
        pet_window_style(hwnd, on);
    }
}

/// Pose une fenêtre d'écran exactement sur son écran : coin en (`x`, `y`),
/// `width` × `height` pixels (`[x, y, width, height]`). `Some(true)` s'il a
/// fallu la déplacer, `Some(false)` si elle y était déjà, `None` si Windows
/// ne la laisse pas là.
///
/// winit crée ses fenêtres sur l'écran principal, puis les déplace. Quand
/// l'écran d'arrivée n'a pas la même mise à l'échelle, Windows le dit à la
/// fenêtre (`WM_DPICHANGED`) et winit la redimensionne aussitôt, pour lui
/// garder sa taille « en points » : une fenêtre de 1280 × 1024 pixels créée
/// sur un portable réglé à 125 % puis posée sur un écran à 100 % n'en
/// couvrait plus que 1024 × 819, dans un coin. Les moutons étaient coupés au
/// bord de cette fenêtre, et invisibles au-delà (photos du 10 novembre
/// 2026, trois écrans). Une fois la fenêtre sur son écran, la reposer ne
/// change plus son échelle : la seconde fois, elle garde sa taille.
pub fn place_pet_window(
    window: &impl raw_window_handle::HasWindowHandle,
    [x, y, width, height]: [i32; 4],
) -> Option<bool> {
    let hwnd = hwnd_of(window)?;
    let mut moved = false;
    for _ in 0..4 {
        let mut at = RECT::default();
        // SAFETY: `hwnd` est une fenêtre vivante de ce processus ; `at` est
        // à nous. Lecture de sa place, puis nouvelle place (ni rang parmi
        // les fenêtres, ni activation).
        let placed = unsafe {
            GetWindowRect(hwnd, &mut at) != 0
                && at.left == x
                && at.top == y
                && at.right - at.left == width
                && at.bottom - at.top == height
        };
        if placed {
            return Some(moved);
        }
        moved = true;
        // SAFETY: voir ci-dessus.
        unsafe {
            SetWindowPos(
                hwnd,
                std::ptr::null_mut(),
                x,
                y,
                width,
                height,
                SWP_NOACTIVATE | SWP_NOZORDER | SWP_NOOWNERZORDER,
            );
        }
    }
    None
}

/// Vérifie de temps en temps qu'une fenêtre d'écran a toujours ses styles
/// (une fenêtre qui prendrait la souris sans raison rendrait le bureau
/// inutilisable) et reste devant les autres fenêtres « toujours devant ».
/// `true` s'il a fallu la corriger.
pub fn check_pet_window(window: &impl raw_window_handle::HasWindowHandle, hittest: bool) -> bool {
    let Some(hwnd) = hwnd_of(window) else {
        return false;
    };
    // SAFETY: `hwnd` est une fenêtre vivante de ce processus ; lecture de
    // ses styles, puis rang parmi les fenêtres (ni taille, ni place, ni
    // activation).
    let style = unsafe { GetWindowLongPtrW(hwnd, GWL_EXSTYLE) as u32 };
    let needed = WS_EX_NOACTIVATE | WS_EX_TOOLWINDOW | WS_EX_LAYERED;
    let passes = style & WS_EX_TRANSPARENT != 0;
    let wrong = style & needed != needed || passes == hittest;
    if wrong {
        pet_window_style(hwnd, hittest);
    }
    if style & WS_EX_TOPMOST == 0 {
        // SAFETY: voir ci-dessus.
        unsafe {
            SetWindowPos(
                hwnd,
                HWND_TOPMOST,
                0,
                0,
                0,
                0,
                SWP_NOMOVE | SWP_NOSIZE | SWP_NOACTIVATE,
            );
        }
    }
    wrong
}

/// La langue de l'utilisateur (« fr-FR »).
#[must_use]
pub fn preferred_languages() -> Vec<String> {
    let mut buffer = [0u16; 85];
    // SAFETY: `buffer` est à nous, donné avec sa taille (celle que Windows
    // demande pour un nom de langue).
    let len = unsafe { GetUserDefaultLocaleName(buffer.as_mut_ptr(), buffer.len() as i32) };
    if len <= 1 {
        return Vec::new();
    }
    vec![from_wide(&buffer)]
}

/// Le fil courant passe devant (celui qui fait les images) ou derrière (un
/// travail de fond) les fils ordinaires.
pub fn set_thread_priority(raised: bool) {
    let priority = if raised {
        THREAD_PRIORITY_ABOVE_NORMAL
    } else {
        THREAD_PRIORITY_BELOW_NORMAL
    };
    // SAFETY: la pseudo-poignée du fil courant est toujours valide ; l'appel
    // ne change que sa priorité.
    let done = unsafe { SetThreadPriority(GetCurrentThread(), priority) };
    if done == 0 {
        log::debug!("priorité du fil inchangée");
    }
}

/// Ouvre un dossier dans l'Explorateur, ou une adresse dans le navigateur.
pub fn open_folder(path: &std::path::Path) -> bool {
    ui::open(&path.to_string_lossy())
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Pour l'œil : les trois fenêtres de Mouflon, avec leurs compléments et
    /// des aperçus, ouvertes quelques secondes (`MOUFLON_REGARD`, 5 sans
    /// rien ; `MOUFLON_REGARD_IMAGES` : un dossier où enregistrer leurs
    /// images, en clair et en sombre).
    ///
    /// ```sh
    /// cargo test -p mouflon-platform -- --ignored --nocapture the_windows_stay_open_for_a_look
    /// ```
    #[test]
    #[ignore = "ouvre des fenêtres et attend : pour regarder"]
    fn the_windows_stay_open_for_a_look() {
        use windows_sys::Win32::UI::WindowsAndMessaging::{
            DispatchMessageW, MSG, PM_REMOVE, PeekMessageW, TranslateMessage,
        };

        let form = form::tests::sample();
        let values = [
            crate::FormValue::Step(1, 1),
            crate::FormValue::Step(2, 0),
            crate::FormValue::Check(3, true),
            crate::FormValue::Check(4, true),
            crate::FormValue::Shown(40, true),
        ];
        let cards = gallery::tests::sample();
        assert!(show_gallery("Galerie des animaux", &cards));
        for (k, card) in cards.iter().enumerate() {
            set_gallery_preview(card.id, &gallery::tests::preview(40 * k as u8));
        }
        show_about(&crate::About {
            name: "Mouflon",
            version: "1.0.1",
            copyright: "© 2026 Alejandro San Martin Lamas",
            lines: &[
                crate::AboutLine::Text(
                    "Un gestionnaire de compagnons de bureau, léger et beau : des moutons \
                     marchent sur tes fenêtres.",
                ),
                crate::AboutLine::Link(
                    "alejandrosml.github.io/Mouflon",
                    "https://alejandrosml.github.io/Mouflon/",
                ),
                crate::AboutLine::Text("Sons : domaine public et CC0."),
                crate::AboutLine::Text("Licence MIT ou Apache 2.0."),
            ],
        });
        assert!(show_form(&form, &values));
        if let Some(dir) = std::env::var_os("MOUFLON_REGARD_IMAGES") {
            let dir = std::path::PathBuf::from(dir);
            let _ = std::fs::create_dir_all(&dir);
            for (name, dark) in [("clair", false), ("sombre", true)] {
                assert!(snapshot_form(
                    &dir.join(format!("reglages-{name}.png")),
                    dark
                ));
                assert!(snapshot_gallery(
                    &dir.join(format!("galerie-{name}.png")),
                    dark
                ));
            }
        }
        let seconds = std::env::var("MOUFLON_REGARD")
            .ok()
            .and_then(|seconds| seconds.parse::<f64>().ok())
            .unwrap_or(5.0);
        let until = std::time::Instant::now() + Duration::from_secs_f64(seconds);
        while std::time::Instant::now() < until {
            let mut message = MSG::default();
            // SAFETY: la boucle de messages ordinaire de ce fil ; `message`
            // est à nous.
            unsafe {
                while PeekMessageW(&mut message, std::ptr::null_mut(), 0, 0, PM_REMOVE) != 0 {
                    TranslateMessage(&message);
                    DispatchMessageW(&message);
                }
            }
            std::thread::sleep(Duration::from_millis(5));
        }
    }

    #[test]
    fn the_desktop_tool_windows_and_click_through_overlays_are_not_windows_to_walk_on() {
        assert!(is_real_window("Notepad", 0));
        assert!(is_real_window("Chrome_WidgetWin_1", WS_EX_LAYERED));
        for shell in [
            "Progman",
            "WorkerW",
            "Shell_TrayWnd",
            "Shell_SecondaryTrayWnd",
        ] {
            assert!(!is_real_window(shell, 0), "{shell}");
        }
        assert!(!is_real_window("Bulle", WS_EX_TOOLWINDOW));
        // Une palette d'outils « toujours devant » reste un rebord.
        assert!(is_real_window("Palette", WS_EX_TOOLWINDOW | WS_EX_TOPMOST));
        // L'incrustation d'une carte graphique, un autre compagnon de bureau.
        assert!(!is_real_window(
            "Incrustation",
            WS_EX_LAYERED | WS_EX_TRANSPARENT | WS_EX_TOPMOST
        ));
    }

    #[test]
    fn the_log_tells_how_the_screens_are_laid_out_and_scaled() {
        let monitors = [
            Monitor {
                frame: [0, 0, 1920, 1080],
                work: [0, 0, 1920, 1020],
                scale: 1.25,
                primary: true,
            },
            Monitor {
                frame: [-1280, 56, 0, 1080],
                work: [-1280, 56, 0, 1032],
                scale: 1.0,
                primary: false,
            },
        ];
        assert_eq!(
            describe_monitors(&monitors),
            "2 — 1920 × 1080 en 0, 0 (principal, 125 %, zone de travail 1920 × 1020 en 0, 0) ; \
             1280 × 1024 en -1280, 56 (100 %, zone de travail 1280 × 976 en -1280, 56)"
        );
    }

    #[test]
    fn texts_go_to_windows_and_back() {
        let text = wide("Réglages de Mouflon");
        assert_eq!(text.last(), Some(&0));
        assert_eq!(from_wide(&text), "Réglages de Mouflon");
        assert_eq!(from_wide(&[]), "");
    }
}
