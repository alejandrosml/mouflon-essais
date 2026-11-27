//! La fenêtre « À propos » de Mouflon : son icône, son nom, sa version,
//! ses crédits (avec leurs liens) et sa mention de copyright.

use std::cell::{Cell, RefCell};
use std::mem::ManuallyDrop;
use std::rc::Rc;

use windows_sys::Win32::Foundation::{HWND, LPARAM, LRESULT, RECT, WPARAM};
use windows_sys::Win32::Graphics::Gdi::HDC;
use windows_sys::Win32::System::LibraryLoader::GetModuleHandleW;
use windows_sys::Win32::UI::WindowsAndMessaging::{
    BN_CLICKED, DI_NORMAL, DefWindowProcW, DestroyIcon, DrawIconEx, IDI_APPLICATION, IMAGE_ICON,
    LR_SHARED, LoadImageW, SWP_NOACTIVATE, SWP_NOSIZE, SWP_NOZORDER, SetWindowPos, WM_COMMAND,
    WM_NCDESTROY,
};

use super::ui::{Handled, Panel, Tone, open};
use crate::{About, AboutLine};

const CLASS: &str = "MouflonAPropos";
/// Largeur du contenu, marges, côté de l'icône (points).
const WIDTH: i32 = 380;
const MARGIN: i32 = 24;
const ICON: i32 = 64;
/// Sous l'icône, sous la version, entre deux lignes, avant le copyright.
const ICON_GAP: i32 = 10;
const VERSION_GAP: i32 = 14;
const LINE_GAP: i32 = 6;
const COPYRIGHT_GAP: i32 = 14;

/// Ce que montre une ligne.
enum Kind {
    Name,
    Text,
    Link(String),
}

struct Line {
    hwnd: HWND,
    text: String,
    kind: Kind,
    /// Écart au-dessus d'elle, en points.
    gap: i32,
}

/// L'icône du programme à une taille donnée (rendue à Windows à la fin, si
/// elle a été faite pour nous).
struct Icon {
    handle: isize,
    ours: bool,
}

impl Icon {
    fn new(side: i32) -> Self {
        // SAFETY: lecture d'une ressource du programme par son numéro (1 :
        // voir `build.rs`) à la taille voulue, ou à défaut de l'icône que
        // Windows donne à tout programme (partagée).
        unsafe {
            let instance = GetModuleHandleW(std::ptr::null());
            let first = std::ptr::without_provenance::<u16>(1);
            let own = LoadImageW(instance, first, IMAGE_ICON, side, side, 0);
            if own.is_null() {
                Self {
                    handle: LoadImageW(
                        std::ptr::null_mut(),
                        IDI_APPLICATION,
                        IMAGE_ICON,
                        side,
                        side,
                        LR_SHARED,
                    ) as isize,
                    ours: false,
                }
            } else {
                Self {
                    handle: own as isize,
                    ours: true,
                }
            }
        }
    }
}

impl Drop for Icon {
    fn drop(&mut self) {
        if self.ours {
            // SAFETY: l'icône a été faite pour nous et ne sert plus.
            unsafe { DestroyIcon(self.handle as _) };
        }
    }
}

struct Built {
    panel: Rc<Panel>,
    lines: Vec<Line>,
    icon: RefCell<Icon>,
    /// Où l'icône se dessine dans le contenu, et son côté (pixels).
    icon_at: Cell<(i32, i32, i32)>,
}

thread_local! {
    /// La fenêtre, tant qu'elle est ouverte. (Jamais défaite à la sortie du
    /// programme : voir la fenêtre des réglages.)
    static ABOUT: ManuallyDrop<RefCell<Option<Rc<Built>>>> =
        const { ManuallyDrop::new(RefCell::new(None)) };
}

fn current() -> Option<Rc<Built>> {
    ABOUT.with(|slot| slot.try_borrow().ok().and_then(|slot| slot.clone()))
}

fn build(about: &About<'_>) -> Option<Rc<Built>> {
    let panel = Panel::create(CLASS, about.name, Some(about_proc))?;
    panel.dress();
    let mut lines = Vec::new();
    let name = panel.centered(about.name, Tone::PLAIN);
    panel.set_title_font(name);
    lines.push(Line {
        hwnd: name,
        text: about.name.to_owned(),
        kind: Kind::Name,
        gap: ICON_GAP,
    });
    lines.push(Line {
        hwnd: panel.centered(about.version, Tone::FAINT),
        text: about.version.to_owned(),
        kind: Kind::Text,
        gap: 0,
    });
    for (i, line) in about.lines.iter().enumerate() {
        let gap = if i == 0 { VERSION_GAP } else { LINE_GAP };
        lines.push(match line {
            AboutLine::Text(text) => Line {
                hwnd: panel.centered(text, Tone::PLAIN),
                text: (*text).to_owned(),
                kind: Kind::Text,
                gap,
            },
            AboutLine::Link(shown, url) => Line {
                hwnd: panel.link(shown),
                text: (*shown).to_owned(),
                kind: Kind::Link((*url).to_owned()),
                gap,
            },
        });
    }
    lines.push(Line {
        hwnd: panel.centered(about.copyright, Tone::FAINT),
        text: about.copyright.to_owned(),
        kind: Kind::Text,
        gap: COPYRIGHT_GAP,
    });
    let icon = Icon::new(panel.px(ICON));
    Some(Rc::new(Built {
        panel,
        lines,
        icon: RefCell::new(icon),
        icon_at: Cell::new((0, 0, 0)),
    }))
}

/// Met tout en place, de haut en bas, centré, et ajuste la fenêtre.
fn layout(built: &Built) {
    let panel = &built.panel;
    let width = panel.px(WIDTH);
    let margin = panel.px(MARGIN);
    let inner = width - 2 * margin;
    let side = panel.px(ICON);
    let mut y = margin;
    built.icon_at.set(((width - side) / 2, y, side));
    y += side;
    for line in &built.lines {
        y += panel.px(line.gap);
        match &line.kind {
            Kind::Name => {
                let (_, h) = panel.title_size(&line.text);
                panel.place(line.hwnd, margin, y, inner, h);
                y += h;
            }
            Kind::Text => {
                let h = panel.wrapped_height(&line.text, false, inner);
                panel.place(line.hwnd, margin, y, inner, h);
                y += h;
            }
            Kind::Link(_) => {
                let (w, h) = panel.link_size(&line.text);
                let w = w.min(inner);
                panel.place(line.hwnd, (width - w) / 2, y, w, h);
                y += h;
            }
        }
    }
    panel.set_frames(Vec::new(), Vec::new());
    panel.fit(width, y + margin);
}

/// Ce que Windows dit à la fenêtre « À propos ».
unsafe extern "system" fn about_proc(
    hwnd: HWND,
    message: u32,
    wparam: WPARAM,
    lparam: LPARAM,
) -> LRESULT {
    if let Some(built) = current().filter(|built| built.panel.hwnd == hwnd) {
        match message {
            WM_COMMAND if lparam != 0 && ((wparam >> 16) & 0xFFFF) as u32 == BN_CLICKED => {
                let link = built.lines.iter().find_map(|line| match &line.kind {
                    Kind::Link(url) if line.hwnd == lparam as HWND => Some(url),
                    _ => None,
                });
                if let Some(url) = link {
                    open(url);
                    return 0;
                }
            }
            WM_NCDESTROY => ABOUT.with(|slot| {
                if let Ok(mut slot) = slot.try_borrow_mut() {
                    *slot = None;
                }
            }),
            _ => {}
        }
        let icon = |hdc: HDC, _: &RECT, scroll: i32| {
            let (x, y, side) = built.icon_at.get();
            // SAFETY: `hdc` est le contexte de dessin de la fenêtre, valide
            // le temps de l'appel ; l'icône vit autant que la fenêtre.
            unsafe {
                DrawIconEx(
                    hdc,
                    x,
                    y - scroll,
                    built.icon.borrow().handle as _,
                    side,
                    side,
                    0,
                    std::ptr::null_mut(),
                    DI_NORMAL,
                );
            }
        };
        match built.panel.handle(message, wparam, lparam, &icon) {
            Handled::Done(result) => return result,
            Handled::Relayout(place) => {
                *built.icon.borrow_mut() = Icon::new(built.panel.px(ICON));
                layout(&built);
                if let Some(place) = place {
                    // SAFETY: notre fenêtre, sur ce fil ; elle va où Windows
                    // la propose (elle change d'écran).
                    unsafe {
                        SetWindowPos(
                            hwnd,
                            std::ptr::null_mut(),
                            place.left,
                            place.top,
                            0,
                            0,
                            SWP_NOSIZE | SWP_NOZORDER | SWP_NOACTIVATE,
                        );
                    }
                }
            }
            Handled::No => {}
        }
    }
    // SAFETY: les arguments sont ceux que Windows vient de nous donner.
    unsafe { DefWindowProcW(hwnd, message, wparam, lparam) }
}

/// Ouvre la fenêtre « À propos », devant les autres apps (ou la ramène
/// devant, si elle est déjà ouverte).
pub fn show_about(about: &About<'_>) {
    let built = match current() {
        Some(built) => built,
        None => {
            let Some(built) = build(about) else {
                return;
            };
            ABOUT.with(|slot| *slot.borrow_mut() = Some(Rc::clone(&built)));
            layout(&built);
            built.panel.settle(None);
            built
        }
    };
    built.panel.show();
}

/// Enregistre en PNG l'image de la fenêtre « À propos », en apparence
/// claire ou sombre (la vitrine de la CI). `false` si elle n'est pas
/// ouverte.
#[must_use]
pub fn snapshot_about(path: &std::path::Path, dark: bool) -> bool {
    current().is_some_and(|built| built.panel.snapshot(path, dark))
}
