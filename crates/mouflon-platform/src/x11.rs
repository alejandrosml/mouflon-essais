//! Linux et les autres Unix, par X11 (aussi sous Wayland, à travers
//! XWayland : Mouflon n'y voit alors que les fenêtres des apps X11).
//!
//! X11 compte en pixels. Comme sous Windows, le repère commun a **une seule
//! échelle** pour tout le bureau : celle que dit `Xft.dpi` (96 points par
//! pouce : un point vaut un pixel), la même que suivent les autres apps.
//!
//! Tout passe par une connexion à part, en pur Rust (aucun code du système
//! n'est appelé d'ici : ce module n'a pas d'`unsafe`). Sans serveur X, chaque
//! fonction répond « rien » et l'app se contente de ce que dit winit.
//!
//! Les menus et les sons empruntent ce que le bureau a déjà : `zenity` ou
//! `kdialog` pour choisir dans une liste, `mpg123`, `ffplay`, `paplay`… pour
//! jouer un son. Rien de tout cela n'est exigé : sans eux, pas de menu, pas
//! de son.

use std::collections::HashMap;
use std::io::Write as _;
use std::path::PathBuf;
use std::process::{Command, Stdio};
use std::sync::mpsc::{Receiver, Sender, channel};
use std::sync::{Arc, Mutex, OnceLock};

use x11rb::connection::Connection as _;
use x11rb::protocol::randr::ConnectionExt as _;
use x11rb::protocol::screensaver::ConnectionExt as _;
use x11rb::protocol::xproto::{
    AtomEnum, ClientMessageEvent, ConnectionExt as _, EventMask, MapState, PropMode, Window,
    WindowClass,
};
use x11rb::rust_connection::RustConnection;
use x11rb::wrapper::ConnectionExt as _;

use super::plain::open_folder;
use super::{
    About, AboutLine, Form, FormRow, FormValue, LoginItem, OsRect, OsScreen, OsWindow, PopupItem,
    UiEvent, UserActivity,
};

x11rb::atom_manager! {
    Atoms: AtomsCookie {
        _NET_CLIENT_LIST,
        _NET_CLIENT_LIST_STACKING,
        _NET_WM_STRUT,
        _NET_WM_STRUT_PARTIAL,
        _NET_ACTIVE_WINDOW,
        _NET_CURRENT_DESKTOP,
        _NET_WORKAREA,
        _NET_WM_DESKTOP,
        _NET_WM_PID,
        _NET_WM_STATE,
        _NET_WM_STATE_HIDDEN,
        _NET_WM_STATE_ABOVE,
        _NET_WM_STATE_STICKY,
        _NET_WM_STATE_SKIP_TASKBAR,
        _NET_WM_STATE_SKIP_PAGER,
        _NET_WM_WINDOW_TYPE,
        _NET_WM_WINDOW_TYPE_DOCK,
        _NET_WM_WINDOW_TYPE_DESKTOP,
        _NET_FRAME_EXTENTS,
        _GTK_FRAME_EXTENTS,
    }
}

/// La connexion au serveur X, ce qu'on y lit sans cesse, et l'échelle.
struct X11 {
    conn: RustConnection,
    root: Window,
    atoms: Atoms,
    /// Pixels par point.
    scale: f64,
    /// Taille de l'écran X, en pixels.
    size: (u16, u16),
}

fn connect() -> Option<X11> {
    let (conn, screen) = x11rb::connect(None).ok()?;
    let root = conn.setup().roots.get(screen)?;
    let (root, size) = (root.root, (root.width_in_pixels, root.height_in_pixels));
    let atoms = Atoms::new(&conn).ok()?.reply().ok()?;
    // `Xft.dpi` : l'échelle que suivent les autres apps (96 : un pour un).
    let dpi = x11rb::resource_manager::new_from_default(&conn)
        .ok()
        .and_then(|db| db.get_string("Xft.dpi", "").map(str::to_owned))
        .and_then(|dpi| dpi.trim().parse::<f64>().ok())
        .filter(|dpi| dpi.is_finite() && *dpi > 0.0)
        .unwrap_or(96.0);
    Some(X11 {
        conn,
        root,
        atoms,
        scale: (dpi / 96.0).clamp(0.5, 8.0),
        size,
    })
}

/// Le serveur X, s'il y en a un (la connexion est faite une fois).
fn x11() -> Option<&'static X11> {
    static X: OnceLock<Option<X11>> = OnceLock::new();
    X.get_or_init(connect).as_ref()
}

impl X11 {
    /// Les entiers d'une propriété d'une fenêtre (vide si elle manque).
    fn cardinals(&self, window: Window, property: u32, max: u32) -> Vec<u32> {
        self.conn
            .get_property(false, window, property, AtomEnum::ANY, 0, max)
            .ok()
            .and_then(|cookie| cookie.reply().ok())
            .and_then(|reply| reply.value32().map(Iterator::collect))
            .unwrap_or_default()
    }

    /// Ce que les barres du bureau (tableaux de bord, docks) réservent aux
    /// bords de l'écran X : une par fenêtre qui en déclare. Les questions
    /// partent toutes ensemble, les réponses reviennent en une fois.
    fn struts(&self) -> Vec<Strut> {
        let clients = self.cardinals(self.root, self.atoms._NET_CLIENT_LIST, 4096);
        let ask = |property: u32, len: u32| -> Vec<_> {
            clients
                .iter()
                .map(|&window| {
                    self.conn
                        .get_property(false, window, property, AtomEnum::CARDINAL, 0, len)
                        .ok()
                })
                .collect()
        };
        let partial = ask(self.atoms._NET_WM_STRUT_PARTIAL, 12);
        let plain = ask(self.atoms._NET_WM_STRUT, 4);
        let values = |cookie: Option<x11rb::cookie::Cookie<'_, _, _>>| -> Vec<u32> {
            cookie
                .and_then(|cookie| cookie.reply().ok())
                .and_then(|reply: x11rb::protocol::xproto::GetPropertyReply| {
                    reply.value32().map(Iterator::collect)
                })
                .unwrap_or_default()
        };
        partial
            .into_iter()
            .zip(plain)
            .filter_map(|(partial, plain)| strut_of(&values(partial), &values(plain)))
            .collect()
    }

    /// La taille de l'écran X à présent, en pixels (elle change quand on
    /// branche un moniteur).
    fn root_size(&self) -> (f64, f64) {
        self.conn
            .get_geometry(self.root)
            .ok()
            .and_then(|cookie| cookie.reply().ok())
            .map_or(
                (f64::from(self.size.0), f64::from(self.size.1)),
                |geometry| (f64::from(geometry.width), f64::from(geometry.height)),
            )
    }

    fn points(&self, x: f64, y: f64, w: f64, h: f64) -> OsRect {
        OsRect {
            x: x / self.scale,
            y: y / self.scale,
            w: w / self.scale,
            h: h / self.scale,
        }
    }

    /// Le cadre visible d'une fenêtre, en pixels : sa place sur l'écran,
    /// avec la bordure que lui donne le gestionnaire de fenêtres, sans la
    /// marge invisible (les ombres) des fenêtres qui se décorent seules.
    fn frame(&self, window: Window) -> Option<(f64, f64, f64, f64)> {
        let geometry = self.conn.get_geometry(window).ok()?;
        let origin = self
            .conn
            .translate_coordinates(window, self.root, 0, 0)
            .ok()?;
        let (geometry, origin) = (geometry.reply().ok()?, origin.reply().ok()?);
        let (mut x, mut y) = (f64::from(origin.dst_x), f64::from(origin.dst_y));
        let (mut w, mut h) = (f64::from(geometry.width), f64::from(geometry.height));
        // Gauche, droite, haut, bas.
        if let [l, r, t, b] = self.cardinals(window, self.atoms._NET_FRAME_EXTENTS, 4)[..] {
            x -= f64::from(l);
            y -= f64::from(t);
            w += f64::from(l) + f64::from(r);
            h += f64::from(t) + f64::from(b);
        }
        if let [l, r, t, b] = self.cardinals(window, self.atoms._GTK_FRAME_EXTENTS, 4)[..] {
            x += f64::from(l);
            y += f64::from(t);
            w -= f64::from(l) + f64::from(r);
            h -= f64::from(t) + f64::from(b);
        }
        (w > 0.0 && h > 0.0).then_some((x, y, w, h))
    }
}

/// Ce qu'une barre du bureau réserve aux bords de l'écran X
/// (`_NET_WM_STRUT_PARTIAL`) : sa largeur à gauche, à droite, en haut, en
/// bas, puis, pour chacun de ces bords, d'où à où elle le longe. Ces mesures
/// partent des bords de tout l'écran X, pas de ceux d'un moniteur.
type Strut = [f64; 12];

/// La réserve d'une fenêtre, d'après sa propriété complète (12 nombres), ou
/// à défaut l'ancienne (4 nombres : elle longe alors tout le bord). Rien si
/// elle ne réserve rien.
fn strut_of(partial: &[u32], plain: &[u32]) -> Option<Strut> {
    let mut strut = [0.0; 12];
    if let Ok(partial) = <&[u32; 12]>::try_from(partial) {
        strut = partial.map(f64::from);
    } else if let Ok(plain) = <&[u32; 4]>::try_from(plain) {
        for (to, from) in strut.iter_mut().zip(plain) {
            *to = f64::from(*from);
        }
        for end in [5, 7, 9, 11] {
            strut[end] = f64::INFINITY;
        }
    } else {
        return None;
    }
    strut[..4].iter().any(|side| *side > 0.0).then_some(strut)
}

/// Un moniteur garde au moins ceci (pixels) en largeur et en hauteur : des
/// réserves qui lui en laisseraient moins ne sont pas crues.
const MONITOR_KEPT: f64 = 100.0;

/// Ce qui reste d'un moniteur (`x`, `y`, largeur, hauteur) une fois
/// retirées les barres posées sur lui, dans un écran X de taille `root`.
///
/// La zone de travail que publie le bureau (`_NET_WORKAREA`) est un seul
/// rectangle pour tous les moniteurs : une barre en bas du moniteur
/// principal y relève aussi le bas des autres, où il n'y a rien, et les
/// animaux y marcheraient en l'air. Chaque barre dit, elle, quel bord elle
/// longe et d'où à où : elle ne compte que pour le moniteur où son bord
/// intérieur se trouve.
fn monitor_work(
    (mx, my, mw, mh): (f64, f64, f64, f64),
    (root_w, root_h): (f64, f64),
    struts: &[Strut],
) -> (f64, f64, f64, f64) {
    let (mut left, mut top, mut right, mut bottom) = (mx, my, mx + mw, my + mh);
    // La barre longe-t-elle ce moniteur (de `from` à `to`, sur l'axe où il
    // va de `lo` à `hi`) ?
    let along = |from: f64, to: f64, lo: f64, hi: f64| from < hi && to >= lo;
    // Son bord intérieur tombe-t-il dans ce moniteur ?
    let inside = |edge: f64, lo: f64, hi: f64| edge > lo && edge < hi;
    for s in struts {
        let [l, r, t, b, ly0, ly1, ry0, ry1, tx0, tx1, bx0, bx1] = *s;
        if l > 0.0 && along(ly0, ly1, my, my + mh) && inside(l, mx, mx + mw) {
            left = left.max(l);
        }
        if r > 0.0 && along(ry0, ry1, my, my + mh) && inside(root_w - r, mx, mx + mw) {
            right = right.min(root_w - r);
        }
        if t > 0.0 && along(tx0, tx1, mx, mx + mw) && inside(t, my, my + mh) {
            top = top.max(t);
        }
        if b > 0.0 && along(bx0, bx1, mx, mx + mw) && inside(root_h - b, my, my + mh) {
            bottom = bottom.min(root_h - b);
        }
    }
    if right - left < MONITOR_KEPT || bottom - top < MONITOR_KEPT {
        return (mx, my, mw, mh);
    }
    (left, top, right - left, bottom - top)
}

/// Les écrans : chaque moniteur, sa zone de travail (sans les tableaux de
/// bord du bureau), et l'échelle du bureau. Vide sans serveur X.
#[must_use]
pub fn screens() -> Vec<OsScreen> {
    let Some(x) = x11() else {
        return Vec::new();
    };
    // Les moniteurs (le principal d'abord) ; à défaut, tout l'écran X.
    let mut monitors: Vec<(bool, f64, f64, f64, f64)> = x
        .conn
        .randr_get_monitors(x.root, true)
        .ok()
        .and_then(|cookie| cookie.reply().ok())
        .map(|reply| {
            reply
                .monitors
                .iter()
                .filter(|m| m.width > 0 && m.height > 0)
                .map(|m| {
                    (
                        m.primary,
                        f64::from(m.x),
                        f64::from(m.y),
                        f64::from(m.width),
                        f64::from(m.height),
                    )
                })
                .collect()
        })
        .unwrap_or_default();
    if monitors.is_empty() {
        monitors.push((true, 0.0, 0.0, f64::from(x.size.0), f64::from(x.size.1)));
    }
    monitors.sort_by_key(|m| !m.0);
    // La zone de travail du bureau courant : x, y, largeur, hauteur.
    let desktop = x
        .cardinals(x.root, x.atoms._NET_CURRENT_DESKTOP, 1)
        .first()
        .copied()
        .unwrap_or(0) as usize;
    let areas = x.cardinals(x.root, x.atoms._NET_WORKAREA, 4096);
    let areas = areas.as_chunks::<4>().0;
    let work = areas
        .get(desktop)
        .or_else(|| areas.first())
        .map(|&[x, y, w, h]| (f64::from(x), f64::from(y), f64::from(w), f64::from(h)));
    // Les barres du bureau, moniteur par moniteur, quand elles se
    // déclarent ; sinon, ce que la zone de travail laisse de chacun.
    let struts = x.struts();
    let root = x.root_size();
    monitors
        .into_iter()
        .map(|(_, mx, my, mw, mh)| {
            let (vx, vy, vw, vh) = if struts.is_empty() {
                work.map(|(wx, wy, ww, wh)| {
                    let (x0, y0) = (mx.max(wx), my.max(wy));
                    let (x1, y1) = ((mx + mw).min(wx + ww), (my + mh).min(wy + wh));
                    (x0, y0, x1 - x0, y1 - y0)
                })
                .filter(|(_, _, w, h)| *w > 100.0 && *h > 100.0)
                .unwrap_or((mx, my, mw, mh))
            } else {
                monitor_work((mx, my, mw, mh), root, &struts)
            };
            OsScreen {
                frame: x.points(mx, my, mw, mh),
                visible: x.points(vx, vy, vw, vh),
                scale: x.scale,
            }
        })
        .collect()
}

/// Les fenêtres du bureau, de la plus en avant à la plus en arrière.
#[must_use]
pub fn windows() -> Vec<OsWindow> {
    let Some(x) = x11() else {
        return Vec::new();
    };
    let own = std::process::id();
    let desktop = x
        .cardinals(x.root, x.atoms._NET_CURRENT_DESKTOP, 1)
        .first()
        .copied();
    // Ce que dit le gestionnaire de fenêtres (de l'arrière vers l'avant) ;
    // sans lui, les fenêtres filles de l'écran, dans leur ordre.
    let mut managed = true;
    let mut list = x.cardinals(x.root, x.atoms._NET_CLIENT_LIST_STACKING, 4096);
    if list.is_empty() {
        managed = false;
        list = x
            .conn
            .query_tree(x.root)
            .ok()
            .and_then(|cookie| cookie.reply().ok())
            .map(|reply| reply.children)
            .unwrap_or_default();
    }
    let mut out = Vec::new();
    for &window in list.iter().rev().take(256) {
        let Some(attributes) = x
            .conn
            .get_window_attributes(window)
            .ok()
            .and_then(|cookie| cookie.reply().ok())
        else {
            continue;
        };
        if attributes.map_state != MapState::VIEWABLE
            || attributes.class == WindowClass::INPUT_ONLY
            || (!managed && attributes.override_redirect)
        {
            continue;
        }
        let pid = x
            .cardinals(window, x.atoms._NET_WM_PID, 1)
            .first()
            .copied()
            .unwrap_or(0);
        // Les fenêtres de Mouflon (ses écrans transparents) ne comptent pas.
        if pid == own {
            continue;
        }
        // Réduite, ou sur un autre bureau (0xFFFFFFFF : sur tous).
        let states = x.cardinals(window, x.atoms._NET_WM_STATE, 32);
        if states.contains(&x.atoms._NET_WM_STATE_HIDDEN) {
            continue;
        }
        let on = x
            .cardinals(window, x.atoms._NET_WM_DESKTOP, 1)
            .first()
            .copied();
        if let (Some(on), Some(current)) = (on, desktop)
            && on != current
            && on != u32::MAX
        {
            continue;
        }
        // Le fond du bureau et ses tableaux de bord ne sont pas des fenêtres
        // où marcher (la zone de travail s'arrête déjà aux tableaux de bord).
        let kinds = x.cardinals(window, x.atoms._NET_WM_WINDOW_TYPE, 8);
        if kinds.contains(&x.atoms._NET_WM_WINDOW_TYPE_DOCK)
            || kinds.contains(&x.atoms._NET_WM_WINDOW_TYPE_DESKTOP)
        {
            continue;
        }
        let Some((fx, fy, fw, fh)) = x.frame(window) else {
            continue;
        };
        // Le nom de son programme (`WM_CLASS` : deux textes, le second est
        // le nom de l'app), pour le journal.
        let owner = x
            .conn
            .get_property(false, window, AtomEnum::WM_CLASS, AtomEnum::STRING, 0, 64)
            .ok()
            .and_then(|cookie| cookie.reply().ok())
            .map(|reply| {
                String::from_utf8_lossy(&reply.value)
                    .split('\0')
                    .rfind(|part| !part.is_empty())
                    .unwrap_or_default()
                    .to_owned()
            })
            .unwrap_or_default();
        out.push(OsWindow {
            id: u64::from(window),
            pid: pid as i32,
            frame: x.points(fx, fy, fw, fh),
            // Une fenêtre « toujours devant » flotte au-dessus des autres.
            layer: if states.contains(&x.atoms._NET_WM_STATE_ABOVE) {
                3
            } else {
                0
            },
            owner,
        });
    }
    out
}

/// Le processus de la fenêtre au premier plan.
#[must_use]
pub fn frontmost_pid() -> Option<i32> {
    let x = x11()?;
    let active = *x.cardinals(x.root, x.atoms._NET_ACTIVE_WINDOW, 1).first()?;
    if active == 0 {
        return None;
    }
    let pid = *x.cardinals(active, x.atoms._NET_WM_PID, 1).first()?;
    Some(pid as i32)
}

/// Le curseur, en points.
#[must_use]
pub fn cursor_position() -> Option<(f64, f64)> {
    let x = x11()?;
    let pointer = x.conn.query_pointer(x.root).ok()?.reply().ok()?;
    Some((
        f64::from(pointer.root_x) / x.scale,
        f64::from(pointer.root_y) / x.scale,
    ))
}

/// Depuis quand l'utilisateur n'a rien fait (l'économiseur d'écran de X le
/// sait). On ne distingue pas le clavier de la souris : on ne sait pas s'il
/// tape.
#[must_use]
pub fn user_activity() -> Option<UserActivity> {
    let x = x11()?;
    let info = x.conn.screensaver_query_info(x.root).ok()?.reply().ok()?;
    Some(UserActivity {
        since_key: f64::INFINITY,
        idle: f64::from(info.ms_since_user_input) / 1000.0,
    })
}

/// Une fenêtre d'écran de Mouflon vient d'être créée : elle reste devant
/// les autres, sur tous les bureaux, hors de la barre des tâches et du
/// sélecteur de fenêtres.
pub fn prepare_pet_window(window: &impl raw_window_handle::HasWindowHandle) {
    let Some(x) = x11() else {
        return;
    };
    let id = match window.window_handle().map(|handle| handle.as_raw()) {
        Ok(raw_window_handle::RawWindowHandle::Xlib(handle)) => handle.window as u32,
        Ok(raw_window_handle::RawWindowHandle::Xcb(handle)) => handle.window.get(),
        _ => return,
    };
    let states = [
        x.atoms._NET_WM_STATE_ABOVE,
        x.atoms._NET_WM_STATE_STICKY,
        x.atoms._NET_WM_STATE_SKIP_TASKBAR,
        x.atoms._NET_WM_STATE_SKIP_PAGER,
    ];
    // Avant qu'elle soit montrée, la propriété suffit ; après, c'est au
    // gestionnaire de fenêtres qu'on le demande (un message par paire
    // d'états, « 1 » : ajouter).
    let _ = x.conn.change_property32(
        PropMode::REPLACE,
        id,
        x.atoms._NET_WM_STATE,
        AtomEnum::ATOM,
        &states,
    );
    for pair in states.chunks(2) {
        let message = ClientMessageEvent::new(
            32,
            id,
            x.atoms._NET_WM_STATE,
            [1, pair[0], pair.get(1).copied().unwrap_or(0), 1, 0],
        );
        let _ = x.conn.send_event(
            false,
            x.root,
            EventMask::SUBSTRUCTURE_REDIRECT | EventMask::SUBSTRUCTURE_NOTIFY,
            message,
        );
    }
    let _ = x.conn.flush();
}

/// Pas de fenêtre à qui rattacher celles des écrans.
#[must_use]
pub fn overlay_owner() -> Option<isize> {
    None
}

/// Langues de l'utilisateur, d'après les variables d'environnement POSIX
/// (`LANGUAGE`, puis `LC_ALL`, `LC_MESSAGES` et `LANG`), en codes BCP 47.
#[must_use]
pub fn preferred_languages() -> Vec<String> {
    let mut tags: Vec<String> = std::env::var("LANGUAGE")
        .unwrap_or_default()
        .split(':')
        .filter(|tag| !tag.is_empty())
        .map(|tag| tag.replace('_', "-"))
        .collect();
    for var in ["LC_ALL", "LC_MESSAGES", "LANG"] {
        let value = std::env::var(var).unwrap_or_default();
        // « fr_FR.UTF-8 » ou « es_ES@euro » → « fr-FR », « es-ES ».
        let tag = value.split(['.', '@']).next().unwrap_or_default();
        if !tag.is_empty() && tag != "C" && tag != "POSIX" {
            tags.push(tag.replace('_', "-"));
        }
    }
    tags
}

/// Le programme `name` est-il installé (dans un dossier de `PATH`) ?
fn installed(name: &str) -> bool {
    std::env::var_os("PATH")
        .is_some_and(|path| std::env::split_paths(&path).any(|dir| dir.join(name).is_file()))
}

/// Qui reçoit ce que l'utilisateur fait dans les menus de Mouflon. Il est
/// appelé depuis le fil qui attend la réponse de la boîte de dialogue.
type Handler = Arc<dyn Fn(UiEvent) + Send + Sync>;

fn handler() -> &'static Mutex<Option<Handler>> {
    static HANDLER: Mutex<Option<Handler>> = Mutex::new(None);
    &HANDLER
}

pub fn set_ui_handler(handler_fn: impl Fn(UiEvent) + Send + Sync + 'static) {
    if let Ok(mut slot) = handler().lock() {
        *slot = Some(Arc::new(handler_fn));
    }
}

fn emit(event: UiEvent) {
    let handler = handler().lock().ok().and_then(|slot| slot.clone());
    if let Some(handler) = handler {
        handler(event);
    }
}

/// Une liste où choisir : chaque ligne, et ce qu'elle rend. Le bureau la
/// montre avec `zenity` (GNOME et les autres) ou `kdialog` (KDE) ; `None`
/// sans l'un ni l'autre, ou si l'on referme sans choisir.
fn choose(title: &str, lines: &[String]) -> Option<usize> {
    if lines.is_empty() {
        return None;
    }
    let output = if installed("zenity") {
        let mut command = Command::new("zenity");
        command.args([
            "--list",
            "--hide-header",
            "--title",
            title,
            "--text",
            "",
            "--column",
            "n",
            "--column",
            "",
            "--hide-column",
            "1",
            "--print-column",
            "1",
            "--width",
            "420",
            "--height",
            "560",
        ]);
        for (n, line) in lines.iter().enumerate() {
            command.arg(n.to_string()).arg(line);
        }
        command.stderr(Stdio::null()).output().ok()?
    } else if installed("kdialog") {
        let mut command = Command::new("kdialog");
        command.args(["--title", title, "--menu", title]);
        for (n, line) in lines.iter().enumerate() {
            command.arg(n.to_string()).arg(line);
        }
        command.stderr(Stdio::null()).output().ok()?
    } else {
        log::warn!("pas de menu ici : il faudrait `zenity` ou `kdialog`");
        return None;
    };
    String::from_utf8_lossy(&output.stdout)
        .trim()
        .split('|')
        .next()?
        .parse()
        .ok()
}

/// Les articles d'un menu mis à plat : leur ligne (« Taille › Grande »,
/// cochée d'un « ✓ ») et leur numéro.
fn flatten(items: &[PopupItem], prefix: &str, out: &mut Vec<(String, u32)>) {
    for item in items {
        match item {
            PopupItem::Item {
                id,
                label,
                checked,
                enabled: true,
            } => {
                let mark = if *checked { "✓ " } else { "" };
                out.push((format!("{mark}{prefix}{label}"), *id));
            }
            PopupItem::Submenu { label, items } => {
                flatten(items, &format!("{prefix}{label} › "), out);
            }
            _ => {}
        }
    }
}

/// Le menu d'un animal : une liste où choisir, sans faire attendre l'app
/// (la boîte de dialogue vit dans un autre programme).
pub fn pop_up_menu(items: &[PopupItem]) {
    let title = items
        .iter()
        .find_map(|item| match item {
            PopupItem::Header(name) => Some(name.clone()),
            _ => None,
        })
        .unwrap_or_else(|| "Mouflon".to_owned());
    let mut lines = Vec::new();
    flatten(items, "", &mut lines);
    let spawned = std::thread::Builder::new()
        .name("mouflon-menu".into())
        .spawn(move || {
            let labels: Vec<String> = lines.iter().map(|(label, _)| label.clone()).collect();
            if let Some((_, id)) = choose(&title, &labels).and_then(|at| lines.get(at)) {
                emit(UiEvent::Menu(*id));
            }
            emit(UiEvent::MenuClosed);
        });
    if spawned.is_err() {
        emit(UiEvent::MenuClosed);
    }
}

/// Ce que fait une ligne de la liste des réglages.
#[derive(Clone, Copy)]
enum Setting {
    Check(u32, bool),
    Step(u32, usize),
    Button(u32),
}

/// Les réglages, en liste : une ligne par case, une par cran de curseur ou
/// par choix, une par bouton. `false` sans `zenity` ni `kdialog` : l'app
/// ouvre alors le fichier des réglages.
pub fn show_form(form: &Form, values: &[FormValue]) -> bool {
    if !installed("zenity") && !installed("kdialog") {
        return false;
    }
    let checked = |id: u32| {
        values
            .iter()
            .any(|v| matches!(v, FormValue::Check(i, true) if *i == id))
    };
    let step = |id: u32| {
        values.iter().find_map(|v| match v {
            FormValue::Step(i, at) if *i == id => Some(*at),
            _ => None,
        })
    };
    let shown = |button: u32| {
        values
            .iter()
            .any(|v| matches!(v, FormValue::Shown(i, true) if *i == button))
    };
    let mut lines: Vec<(String, Setting)> = Vec::new();
    for section in &form.sections {
        for row in &section.rows {
            match row {
                FormRow::Check {
                    id, label, extra, ..
                } => {
                    let on = checked(*id);
                    let mark = if on { "✓ " } else { "☐ " };
                    lines.push((format!("{mark}{label}"), Setting::Check(*id, !on)));
                    if let Some(extra) = extra
                        && shown(extra.button.id)
                    {
                        lines.push((
                            format!("    {}", extra.button.label),
                            Setting::Button(extra.button.id),
                        ));
                    }
                }
                FormRow::Steps {
                    id,
                    label,
                    steps: choices,
                    ..
                }
                | FormRow::Choice {
                    id, label, choices, ..
                } => {
                    let current = step(*id);
                    for (at, choice) in choices.iter().enumerate() {
                        let mark = if current == Some(at) { "✓ " } else { "" };
                        lines.push((format!("{mark}{label} › {choice}"), Setting::Step(*id, at)));
                    }
                }
                FormRow::Text {
                    button: Some(button),
                    ..
                } => lines.push((button.label.clone(), Setting::Button(button.id))),
                FormRow::Text { button: None, .. } => {}
            }
        }
    }
    let title = form.title.clone();
    let spawned = std::thread::Builder::new()
        .name("mouflon-reglages".into())
        .spawn(move || {
            emit(UiEvent::FormShown);
            let labels: Vec<String> = lines.iter().map(|(label, _)| label.clone()).collect();
            match choose(&title, &labels).and_then(|at| lines.get(at)) {
                Some((_, Setting::Check(id, on))) => emit(UiEvent::Check(*id, *on)),
                Some((_, Setting::Step(id, at))) => emit(UiEvent::Step(*id, *at)),
                Some((_, Setting::Button(id))) => emit(UiEvent::Button(*id)),
                None => {}
            }
        });
    spawned.is_ok()
}

/// Rien à tenir à jour : la liste des réglages est refaite à chaque fois.
pub fn update_form(_values: &[FormValue]) {}

/// « À propos » : une boîte de message du bureau ; à défaut, la page du
/// projet dans le navigateur.
pub fn show_about(about: &About<'_>) {
    let mut text = format!("{} {}\n{}\n", about.name, about.version, about.copyright);
    let mut link = None;
    for line in about.lines {
        text.push('\n');
        match line {
            AboutLine::Text(line) => text.push_str(line),
            AboutLine::Link(label, url) => {
                link.get_or_insert(*url);
                text.push_str(label);
                text.push_str(" : ");
                text.push_str(url);
            }
        }
    }
    let shown = if installed("zenity") {
        Command::new("zenity")
            .args([
                "--info",
                "--no-markup",
                "--title",
                about.name,
                "--text",
                &text,
            ])
            .stderr(Stdio::null())
            .spawn()
            .is_ok()
    } else if installed("kdialog") {
        Command::new("kdialog")
            .args(["--title", about.name, "--msgbox", &text])
            .stderr(Stdio::null())
            .spawn()
            .is_ok()
    } else {
        false
    };
    if !shown && let Some(url) = link {
        open_folder(std::path::Path::new(url));
    }
}

/// Le fichier qui lance Mouflon à l'ouverture de session (la convention
/// des bureaux : `~/.config/autostart`).
fn autostart_file() -> Option<PathBuf> {
    let config = std::env::var_os("XDG_CONFIG_HOME")
        .map(PathBuf::from)
        .filter(|dir| dir.is_absolute())
        .or_else(|| Some(PathBuf::from(std::env::var_os("HOME")?).join(".config")))?;
    Some(config.join("autostart").join("mouflon.desktop"))
}

#[must_use]
pub fn login_item_available() -> bool {
    autostart_file().is_some() && std::env::current_exe().is_ok()
}

/// Mouflon est-il lancé à l'ouverture de session ?
#[must_use]
pub fn login_item() -> LoginItem {
    match autostart_file() {
        Some(file) if file.is_file() => LoginItem::On,
        Some(_) => LoginItem::Off,
        None => LoginItem::Unavailable,
    }
}

/// Lance (ou non) Mouflon à l'ouverture de session.
pub fn set_login_item(on: bool) -> LoginItem {
    let (Some(file), Ok(exe)) = (autostart_file(), std::env::current_exe()) else {
        return LoginItem::Unavailable;
    };
    if on {
        if let Some(dir) = file.parent() {
            let _ = std::fs::create_dir_all(dir);
        }
        let entry = format!(
            "[Desktop Entry]\nType=Application\nName=Mouflon\nExec=\"{}\"\nX-GNOME-Autostart-enabled=true\n",
            exe.display()
        );
        if let Err(e) = std::fs::write(&file, entry) {
            log::warn!("ouverture de session : {e}");
        }
    } else {
        let _ = std::fs::remove_file(&file);
    }
    login_item()
}

/// Pas de réglage du système à ouvrir : le fichier fait foi.
pub fn open_login_items_settings() {}

/// Sons joués en même temps, au plus.
const MAX_PLAYING: usize = 6;

/// Un son à jouer : le contenu d'un fichier audio et son volume (de 0 à 1).
struct Job {
    data: Arc<[u8]>,
    volume: f32,
}

/// Les lecteurs qu'un bureau a souvent déjà, et la façon de leur dire le
/// volume (de 0 à 1). `wav` : un son WAV, que `mpg123` ne lit pas.
fn player(file: &std::path::Path, volume: f32, wav: bool) -> Option<Command> {
    let volume = volume.clamp(0.0, 1.0);
    let mut command;
    if !wav && installed("mpg123") {
        command = Command::new("mpg123");
        command
            .arg("-q")
            .arg("-f")
            .arg(format!("{}", (volume * 32768.0) as u32));
    } else if installed("pw-play") {
        command = Command::new("pw-play");
        command.arg(format!("--volume={volume:.2}"));
    } else if installed("paplay") {
        command = Command::new("paplay");
        command.arg(format!("--volume={}", (volume * 65536.0) as u32));
    } else if installed("ffplay") {
        command = Command::new("ffplay");
        command
            .args(["-nodisp", "-autoexit", "-loglevel", "quiet", "-volume"])
            .arg(format!("{}", (volume * 100.0) as u32));
    } else {
        return None;
    }
    command
        .arg(file)
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null());
    Some(command)
}

/// Le fil des sons : chaque son est écrit une fois dans le dossier
/// temporaire, puis joué de là par un lecteur du bureau.
fn play_all(jobs: &Receiver<Job>) {
    let dir = std::env::temp_dir().join(format!("mouflon-sons-{}", std::process::id()));
    let mut files: HashMap<(usize, usize), PathBuf> = HashMap::new();
    let mut playing: Vec<std::process::Child> = Vec::new();
    let mut warned = false;
    while let Ok(job) = jobs.recv() {
        playing.retain_mut(|child| matches!(child.try_wait(), Ok(None)));
        if playing.len() >= MAX_PLAYING || job.volume <= 0.0 {
            continue;
        }
        let key = (job.data.as_ptr() as usize, job.data.len());
        let wav = job.data.starts_with(b"RIFF");
        let file = files.entry(key).or_insert_with(|| {
            let _ = std::fs::create_dir_all(&dir);
            let file = dir.join(format!("{}.{}", key.0, if wav { "wav" } else { "mp3" }));
            if let Ok(mut out) = std::fs::File::create(&file) {
                let _ = out.write_all(&job.data);
            }
            file
        });
        match player(file, job.volume, wav).map(|mut command| command.spawn()) {
            Some(Ok(child)) => playing.push(child),
            Some(Err(e)) => log::debug!("son : {e}"),
            None if !warned => {
                warned = true;
                log::warn!(
                    "pas de son ici : il faudrait `mpg123`, `ffplay`, `pw-play` ou `paplay`"
                );
            }
            None => {}
        }
    }
}

fn sounds() -> Option<&'static Mutex<Sender<Job>>> {
    static SOUNDS: OnceLock<Option<Mutex<Sender<Job>>>> = OnceLock::new();
    SOUNDS
        .get_or_init(|| {
            let (sender, receiver) = channel();
            std::thread::Builder::new()
                .name("mouflon-sons".into())
                .spawn(move || play_all(&receiver))
                .ok()
                .map(|_| Mutex::new(sender))
        })
        .as_ref()
}

/// Joue un son (le contenu d'un fichier MP3 ou WAV) au volume donné, sans
/// attendre.
pub fn play_sound(data: &Arc<[u8]>, volume: f32) {
    if let Some(sender) = sounds().and_then(|s| s.lock().ok()) {
        let _ = sender.send(Job {
            data: Arc::clone(data),
            volume,
        });
    }
}

/// Met le fil des sons en route.
pub fn warm_up_sounds(_sample: Option<&Arc<[u8]>>) {
    let _ = sounds();
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Une barre en bas d'un moniteur : `height` pixels au-dessus du bas
    /// `bottom` de ce moniteur, de `x0` à `x1`, dans un écran X haut de
    /// `root_h`.
    fn bottom_bar(root_h: f64, bottom: f64, height: f64, x0: f64, x1: f64) -> Strut {
        let mut strut = [0.0; 12];
        strut[3] = root_h - bottom + height;
        strut[10] = x0;
        strut[11] = x1;
        strut
    }

    #[test]
    fn a_bar_counts_only_on_the_monitor_it_sits_on() {
        // Un petit moniteur à gauche (800 de haut), un grand à droite
        // (1200) : la barre du petit ne relève pas le bas du grand.
        let (small, tall) = ((0.0, 0.0, 1000.0, 800.0), (1000.0, 0.0, 1600.0, 1200.0));
        let root = (2600.0, 1200.0);
        let bars = [bottom_bar(1200.0, 800.0, 40.0, 0.0, 999.0)];
        assert_eq!(monitor_work(small, root, &bars), (0.0, 0.0, 1000.0, 760.0));
        assert_eq!(monitor_work(tall, root, &bars), tall);
        // Une barre sur chacun, de hauteurs différentes.
        let bars = [
            bottom_bar(1200.0, 800.0, 40.0, 0.0, 999.0),
            bottom_bar(1200.0, 1200.0, 48.0, 1000.0, 2599.0),
        ];
        assert_eq!(monitor_work(small, root, &bars), (0.0, 0.0, 1000.0, 760.0));
        assert_eq!(
            monitor_work(tall, root, &bars),
            (1000.0, 0.0, 1600.0, 1152.0)
        );
    }

    #[test]
    fn stacked_monitors_each_keep_their_own_bars() {
        // Un moniteur au-dessus de l'autre ; la barre est en bas de celui
        // du haut (au milieu de l'écran X), une autre en haut de celui-ci.
        let (upper, lower) = ((0.0, 0.0, 1280.0, 1000.0), (0.0, 1000.0, 1280.0, 800.0));
        let root = (1280.0, 1800.0);
        let mut top_bar = [0.0; 12];
        top_bar[2] = 30.0;
        top_bar[9] = 1279.0;
        let bars = [bottom_bar(1800.0, 1000.0, 40.0, 0.0, 1279.0), top_bar];
        assert_eq!(monitor_work(upper, root, &bars), (0.0, 30.0, 1280.0, 930.0));
        assert_eq!(monitor_work(lower, root, &bars), lower);
    }

    #[test]
    fn a_bar_on_the_side_between_two_monitors_belongs_to_one() {
        let (left, right) = ((0.0, 0.0, 1000.0, 800.0), (1000.0, 0.0, 1000.0, 800.0));
        let root = (2000.0, 800.0);
        // Une barre verticale au bord gauche du moniteur de droite.
        let mut bar = [0.0; 12];
        bar[0] = 1048.0;
        bar[5] = 799.0;
        assert_eq!(monitor_work(left, root, &[bar]), left);
        assert_eq!(
            monitor_work(right, root, &[bar]),
            (1048.0, 0.0, 952.0, 800.0)
        );
        // Et au bord droit du moniteur de gauche.
        let mut bar = [0.0; 12];
        bar[1] = 1060.0;
        bar[7] = 799.0;
        assert_eq!(monitor_work(left, root, &[bar]), (0.0, 0.0, 940.0, 800.0));
        assert_eq!(monitor_work(right, root, &[bar]), right);
    }

    #[test]
    fn old_style_bars_run_along_the_whole_edge_and_absurd_ones_are_not_believed() {
        // L'ancienne propriété : quatre nombres, tout le long du bord.
        let old = strut_of(&[], &[0, 0, 0, 40]).expect("une barre en bas");
        let (small, tall) = ((0.0, 0.0, 1000.0, 800.0), (1000.0, 0.0, 1600.0, 1200.0));
        let root = (2600.0, 1200.0);
        assert_eq!(
            monitor_work(tall, root, &[old]),
            (1000.0, 0.0, 1600.0, 1160.0)
        );
        assert_eq!(monitor_work(small, root, &[old]), small);
        // La propriété complète l'emporte ; une fenêtre qui ne réserve rien
        // n'est pas une barre.
        let full = strut_of(&[0, 0, 0, 48, 0, 0, 0, 0, 0, 0, 1000, 2599], &[0, 0, 0, 40]);
        assert_eq!(
            full.map(|s| (s[3], s[10], s[11])),
            Some((48.0, 1000.0, 2599.0))
        );
        assert!(strut_of(&[0; 12], &[]).is_none());
        assert!(strut_of(&[], &[]).is_none());
        // Une réserve qui ne laisserait rien du moniteur.
        let mut absurd = [0.0; 12];
        absurd[3] = 1150.0;
        absurd[11] = 2599.0;
        assert_eq!(monitor_work(tall, root, &[absurd]), tall);
    }
}
