//! La galerie des animaux : une carte par espèce, avec son aperçu animé
//! (l'animal qui marche, dessiné par Mouflon), son nom, sa description et
//! un bouton pour en ajouter un à l'écran.
//!
//! Comme sur Mac, les cartes se rangent par trois, de haut en bas, famille
//! par famille (sous le nom de chacune, s'il y en a plusieurs), et la
//! galerie défile. Les aperçus arrivent après coup
//! ([`set_gallery_preview`]) : l'app les dessine sur un fil à part. Ils sont
//! gardés à la taille exacte où ils se montrent, et seulement tant que la
//! galerie est ouverte : fermée, elle est défaite et ne coûte plus rien.

use std::cell::{Cell, RefCell};
use std::ffi::c_void;
use std::mem::ManuallyDrop;
use std::path::Path;
use std::rc::Rc;

use windows_sys::Win32::Foundation::{HWND, LPARAM, LRESULT, RECT, WPARAM};
use windows_sys::Win32::Graphics::Gdi::{
    BI_RGB, BITMAPINFO, BITMAPINFOHEADER, DIB_RGB_COLORS, HDC, SetDIBitsToDevice,
};
use windows_sys::Win32::UI::WindowsAndMessaging::{
    BN_CLICKED, CB_GETCURSEL, CB_SETCURSEL, DefWindowProcW, GetWindowRect, IsIconic, KillTimer,
    SWP_NOACTIVATE, SWP_NOSIZE, SWP_NOZORDER, SendMessageW, SetTimer, SetWindowPos, WM_COMMAND,
    WM_DESTROY, WM_NCDESTROY, WM_TIMER,
};

use super::host::emit;
use super::ui::{Handled, Panel, Tone};
use crate::{GalleryCard, GalleryImage, UiEvent};

const CLASS: &str = "MouflonGalerie";
/// Largeur du contenu, marges, écart entre les cartes (points), colonnes.
const WIDTH: i32 = 660;
const MARGIN: i32 = 20;
const GAP: i32 = 14;
const COLUMNS: usize = 3;
/// Marge intérieure d'une carte, hauteur de son aperçu.
const PAD: i32 = 12;
const PREVIEW: i32 = 120;
/// Hauteur la plus grande de la galerie : au-delà, elle défile.
const MAX_HEIGHT: i32 = 640;
/// Avant le nom d'une famille (après la précédente), et après lui.
const FAMILY_GAP: i32 = 10;
const HEADER_GAP: i32 = 8;
/// Entre le bouton d'une carte et sa liste de choix.
const CHOICE_GAP: i32 = 6;
/// La minuterie des aperçus : dix images par seconde.
const TIMER: usize = 1;
const TIMER_MS: u32 = 100;

/// Les images de l'aperçu d'une carte, à la taille où elles se montrent :
/// BGRA prémultiplié, toutes de `width` sur `height` pixels, à poser à
/// (`left`, `top`) dans la place de l'aperçu.
#[derive(Default)]
struct Frames {
    width: i32,
    height: i32,
    left: i32,
    top: i32,
    images: Vec<Vec<u8>>,
}

struct Card {
    id: u32,
    name: (HWND, String),
    detail: (HWND, String),
    button: (HWND, String),
    /// La liste de ce qu'on peut ajouter (un bélier, une brebis…), si la
    /// carte en a une.
    choices: Option<(HWND, Vec<String>)>,
    /// La place de son aperçu, dans le contenu.
    image: Cell<RECT>,
    frames: RefCell<Frames>,
    shown: Cell<usize>,
}

struct Built {
    /// Ce que l'app a décrit : une autre description refait la galerie.
    title: String,
    given: Vec<GalleryCard>,
    panel: Rc<Panel>,
    cards: Vec<Card>,
    /// Le nom de chaque famille, quand il y en a plusieurs : le rang de sa
    /// première carte, et son titre.
    families: Vec<(usize, HWND)>,
    /// De quoi poser un aperçu sur le fond de sa carte avant de le montrer.
    scratch: RefCell<Vec<u8>>,
}

thread_local! {
    /// La galerie, tant qu'elle est ouverte. (Jamais défaite à la sortie du
    /// programme : voir la fenêtre des réglages.)
    static GALLERY: ManuallyDrop<RefCell<Option<Rc<Built>>>> =
        const { ManuallyDrop::new(RefCell::new(None)) };
    /// Où elle était la dernière fois (son coin en haut à gauche).
    static PLACE: Cell<Option<(i32, i32)>> = const { Cell::new(None) };
}

fn current() -> Option<Rc<Built>> {
    GALLERY.with(|slot| slot.try_borrow().ok().and_then(|slot| slot.clone()))
}

// ——— Les images ———

/// La part d'images RGBA (toutes de `width` sur `height`) où quelque chose
/// se voit dans l'une au moins : gauche, haut, droite, bas. `None` si elles
/// sont toutes transparentes.
fn visible_box(images: &[&[u8]], width: usize, height: usize) -> Option<[usize; 4]> {
    let (mut left, mut top, mut right, mut bottom) = (width, height, 0, 0);
    for image in images {
        for (y, row) in image.chunks_exact(width * 4).take(height).enumerate() {
            let seen = |x: &usize| row[x * 4 + 3] != 0;
            let Some(first) = (0..width).find(seen) else {
                continue;
            };
            let last = (0..width).rev().find(seen).unwrap_or(first);
            left = left.min(first);
            right = right.max(last + 1);
            top = top.min(y);
            bottom = bottom.max(y + 1);
        }
    }
    (right > left && bottom > top).then_some([left, top, right, bottom])
}

/// Pour chaque case d'arrivée (il y en a `out`, sur les cases de départ de
/// `from` à `to`) : la première case de départ qu'elle recouvre, et la part
/// de chacune dans sa moyenne.
fn spans(from: usize, to: usize, out: usize) -> Vec<(usize, Vec<f32>)> {
    let scale = (to - from) as f64 / out as f64;
    (0..out)
        .map(|o| {
            let start = from as f64 + o as f64 * scale;
            let end = start + scale;
            let first = (start.floor() as usize).min(to - 1);
            let last = (end.ceil() as usize).clamp(first + 1, to);
            let mut weights: Vec<f32> = (first..last)
                .map(|i| (end.min(i as f64 + 1.0) - start.max(i as f64)).max(0.0) as f32)
                .collect();
            let total: f32 = weights.iter().sum();
            if total > 0.0 {
                weights.iter_mut().for_each(|w| *w /= total);
            } else {
                weights = vec![1.0];
            }
            (first, weights)
        })
        .collect()
}

/// Met la part `from` (gauche, haut, droite, bas) d'une image RGBA
/// prémultipliée, large de `width`, à la taille `out_w` sur `out_h`, en
/// BGRA (l'ordre de Windows) : chaque pixel d'arrivée est la moyenne des
/// pixels de départ qu'il recouvre, chacun pour la surface recouverte.
fn resample(rgba: &[u8], width: usize, from: [usize; 4], out_w: usize, out_h: usize) -> Vec<u8> {
    let [x0, y0, x1, y1] = from;
    let (across, down) = (spans(x0, x1, out_w), spans(y0, y1, out_h));
    // D'abord chaque ligne en largeur, puis les lignes entre elles.
    let rows = y1 - y0;
    let mut wide = vec![0f32; rows * out_w * 4];
    for (r, line) in wide.chunks_exact_mut(out_w * 4).enumerate() {
        let source = &rgba[(y0 + r) * width * 4..];
        for (pixel, (first, weights)) in line.as_chunks_mut::<4>().0.iter_mut().zip(&across) {
            for (k, weight) in weights.iter().enumerate() {
                let at = (first + k) * 4;
                for (sum, value) in pixel.iter_mut().zip(&source[at..at + 4]) {
                    *sum += f32::from(*value) * weight;
                }
            }
        }
    }
    let mut out = vec![0u8; out_w * out_h * 4];
    for (line, (first, weights)) in out.chunks_exact_mut(out_w * 4).zip(&down) {
        for (x, pixel) in line.as_chunks_mut::<4>().0.iter_mut().enumerate() {
            let mut sums = [0f32; 4];
            for (k, weight) in weights.iter().enumerate() {
                let at = ((first - y0 + k) * out_w + x) * 4;
                for (sum, value) in sums.iter_mut().zip(&wide[at..at + 4]) {
                    *sum += value * weight;
                }
            }
            let byte = |sum: f32| sum.round().clamp(0.0, 255.0) as u8;
            pixel.copy_from_slice(&[byte(sums[2]), byte(sums[1]), byte(sums[0]), byte(sums[3])]);
        }
    }
    out
}

/// La taille d'une image de `width` sur `height` qui doit tenir dans une
/// place de `room_w` sur `room_h` : la sienne si elle y tient, sinon
/// réduite sans la déformer.
fn fitted(width: f64, height: f64, room_w: f64, room_h: f64) -> (f64, f64) {
    let scale = (room_w / width).min(room_h / height).min(1.0);
    (width * scale, height * scale)
}

/// Pose une image BGRA prémultipliée sur un fond uni (`ground`, noté comme
/// Windows note ses couleurs : rouge dans l'octet faible) : le résultat,
/// opaque, dans `out`.
fn over(image: &[u8], ground: u32, out: &mut Vec<u8>) {
    let ground = [(ground >> 16) & 0xFF, (ground >> 8) & 0xFF, ground & 0xFF];
    out.clear();
    out.reserve(image.len());
    for pixel in image.as_chunks::<4>().0 {
        let through = 255 - u32::from(pixel[3]);
        for (value, ground) in pixel.iter().zip(ground) {
            out.push((u32::from(*value) + (ground * through + 127) / 255).min(255) as u8);
        }
        out.push(255);
    }
}

/// Les images d'un aperçu, mises à la taille où elles se montrent sur cet
/// écran (`dpi`), dans une place de `room_w` sur `room_h` pixels.
fn prepare(given: &[GalleryImage], dpi: u32, room_w: i32, room_h: i32) -> Frames {
    let Some(first) = given.first() else {
        return Frames::default();
    };
    let (width, height) = (first.width as usize, first.height as usize);
    let usable: Vec<&[u8]> = given
        .iter()
        .filter(|image| image.width == first.width && image.height == first.height)
        .map(|image| image.rgba.as_slice())
        .filter(|rgba| rgba.len() >= width * height * 4)
        .collect();
    if width == 0 || height == 0 || usable.is_empty() || room_w <= 0 || room_h <= 0 {
        return Frames::default();
    }
    let Some(seen) = visible_box(&usable, width, height) else {
        return Frames::default();
    };
    // L'image entière, en pixels de cet écran, puis réduite à sa place.
    let natural = f64::from(dpi) / 96.0 / first.scale.max(0.1);
    let (shown_w, shown_h) = fitted(
        width as f64 * natural,
        height as f64 * natural,
        f64::from(room_w),
        f64::from(room_h),
    );
    let scale = shown_w / width as f64;
    // De l'image entière, on ne garde que la part où l'animal se voit.
    let out_w = (((seen[2] - seen[0]) as f64 * scale).round() as usize).max(1);
    let out_h = (((seen[3] - seen[1]) as f64 * scale).round() as usize).max(1);
    let left = (f64::from(room_w) - shown_w) / 2.0 + seen[0] as f64 * scale;
    let top = (f64::from(room_h) - shown_h) / 2.0 + seen[1] as f64 * scale;
    Frames {
        width: out_w as i32,
        height: out_h as i32,
        left: left.round() as i32,
        top: top.round() as i32,
        images: usable
            .iter()
            .map(|rgba| resample(rgba, width, seen, out_w, out_h))
            .collect(),
    }
}

// ——— La fenêtre ———

fn build(title: &str, cards: &[GalleryCard]) -> Option<Rc<Built>> {
    let panel = Panel::create(CLASS, title, Some(gallery_proc))?;
    panel.dress();
    panel.set_max_view(panel.px(MAX_HEIGHT));
    let several = cards
        .windows(2)
        .any(|pair| pair[0].family != pair[1].family);
    let mut families = Vec::new();
    let mut built = Vec::new();
    for (i, card) in cards.iter().enumerate() {
        if several && (i == 0 || cards[i - 1].family != card.family) {
            families.push((i, panel.label(&card.family, Tone::HEADER)));
        }
        let name = panel.label(&card.name, Tone::NAME);
        let detail = panel.label(&card.detail, Tone::HELP);
        let button = panel.button(&card.button);
        // À côté du bouton, ce qu'il ajoute : le bouton lit ce que la liste
        // montre, elle n'annonce rien elle-même.
        let choices = (!card.choices.is_empty()).then(|| {
            let list = panel.combo(&card.choices);
            // SAFETY: message sans pointeur à un contrôle vivant de ce fil.
            unsafe { SendMessageW(list, CB_SETCURSEL, 0, 0) };
            (list, card.choices.clone())
        });
        built.push(Card {
            id: card.id,
            name: (name, card.name.clone()),
            detail: (detail, card.detail.clone()),
            button: (button, card.button.clone()),
            choices,
            image: Cell::new(RECT::default()),
            frames: RefCell::default(),
            shown: Cell::new(0),
        });
    }
    Some(Rc::new(Built {
        title: title.to_owned(),
        given: cards.to_vec(),
        panel,
        cards: built,
        families,
        scratch: RefCell::default(),
    }))
}

/// Largeur d'une carte, et de ce qu'elle contient.
fn card_widths(panel: &Panel) -> (i32, i32) {
    let card = (panel.px(WIDTH) - 2 * panel.px(MARGIN) - (COLUMNS as i32 - 1) * panel.px(GAP))
        / COLUMNS as i32;
    (card, card - 2 * panel.px(PAD))
}

/// Met les cartes en place, par rangées de trois, famille par famille, et
/// ajuste la fenêtre.
fn layout(built: &Built) {
    let panel = &built.panel;
    let (card_width, inner) = card_widths(panel);
    let (margin, gap, pad) = (panel.px(MARGIN), panel.px(GAP), panel.px(PAD));
    let line = panel.fonts().line;
    let preview = panel.px(PREVIEW);
    let mut frames = Vec::new();
    let mut y = margin;
    // Les familles : du rang de leur première carte à celui de la suivante.
    let mut starts: Vec<usize> = built.families.iter().map(|(first, _)| *first).collect();
    if starts.first() != Some(&0) {
        starts.insert(0, 0);
    }
    for (f, &start) in starts.iter().enumerate() {
        let end = starts.get(f + 1).copied().unwrap_or(built.cards.len());
        if let Some((_, header)) = built.families.iter().find(|(first, _)| *first == start) {
            if f > 0 {
                y += panel.px(FAMILY_GAP);
            }
            panel.place(
                *header,
                margin + panel.px(2),
                y,
                panel.px(WIDTH) - 2 * margin,
                line,
            );
            y += line + panel.px(HEADER_GAP);
        }
        for row in built.cards[start..end].chunks(COLUMNS) {
            // La rangée prend la hauteur de sa plus haute carte.
            let details: Vec<i32> = row
                .iter()
                .map(|card| panel.wrapped_height(&card.detail.1, false, inner))
                .collect();
            let (_, button_h) = panel.button_size(&row[0].button.1);
            let tallest = details.iter().copied().max().unwrap_or(0);
            let height = pad
                + preview
                + panel.px(8)
                + line
                + panel.px(4)
                + tallest
                + panel.px(12)
                + button_h
                + pad;
            for (k, (card, detail)) in row.iter().zip(&details).enumerate() {
                let x = margin + k as i32 * (card_width + gap);
                frames.push(RECT {
                    left: x,
                    top: y,
                    right: x + card_width,
                    bottom: y + height,
                });
                let mut cy = y + pad;
                card.image.set(RECT {
                    left: x + pad,
                    top: cy,
                    right: x + pad + inner,
                    bottom: cy + preview,
                });
                cy += preview + panel.px(8);
                panel.place(card.name.0, x + pad, cy, inner, line);
                cy += line + panel.px(4);
                panel.place(card.detail.0, x + pad, cy, inner, *detail);
                // Le bouton en bas de la carte, aligné d'une carte à l'autre.
                let (button_w, _) = panel.button_size(&card.button.1);
                let button_w = button_w.min(inner);
                let bottom = y + height - pad;
                panel.place(
                    card.button.0,
                    x + pad,
                    bottom - button_h,
                    button_w,
                    button_h,
                );
                // Sa liste de choix prend le reste de la largeur.
                if let Some((list, choices)) = &card.choices {
                    let (_, list_h) = panel.combo_size(*list, choices);
                    let left = x + pad + button_w + panel.px(CHOICE_GAP);
                    panel.place_combo(
                        *list,
                        left,
                        bottom - (button_h + list_h) / 2,
                        (x + pad + inner - left).max(panel.px(40)),
                        choices.len(),
                    );
                }
            }
            y += height + gap;
        }
    }
    panel.set_frames(frames, Vec::new());
    panel.fit(panel.px(WIDTH), (y - gap + margin).max(2 * margin));
}

/// Peint les aperçus des cartes qui touchent `clip` (de la fenêtre).
fn draw_previews(built: &Built, hdc: HDC, clip: &RECT, scroll: i32) {
    let ground = built.panel.paint().colors.card;
    let mut scratch = built.scratch.borrow_mut();
    for card in &built.cards {
        let place = card.image.get();
        if place.bottom - scroll <= clip.top || place.top - scroll >= clip.bottom {
            continue;
        }
        let frames = card.frames.borrow();
        let Some(image) = frames.images.get(card.shown.get()) else {
            continue;
        };
        over(image, ground, &mut scratch);
        let info = BITMAPINFO {
            bmiHeader: BITMAPINFOHEADER {
                biSize: size_of::<BITMAPINFOHEADER>() as u32,
                biWidth: frames.width,
                // Négative : les lignes vont de haut en bas.
                biHeight: -frames.height,
                biPlanes: 1,
                biBitCount: 32,
                biCompression: BI_RGB,
                ..BITMAPINFOHEADER::default()
            },
            ..BITMAPINFO::default()
        };
        // SAFETY: `hdc` est le contexte de dessin de la fenêtre, valide le
        // temps de l'appel ; `scratch` porte `width` × `height` pixels de
        // quatre octets (faits par `over` d'une image de cette taille) et
        // vit jusqu'au retour, comme `info`.
        unsafe {
            SetDIBitsToDevice(
                hdc,
                place.left + frames.left,
                place.top - scroll + frames.top,
                frames.width as u32,
                frames.height as u32,
                0,
                0,
                0,
                frames.height as u32,
                scratch.as_ptr().cast::<c_void>(),
                &info,
                DIB_RGB_COLORS,
            );
        }
    }
}

/// La minuterie : les aperçus passent à l'image suivante.
fn tick(built: &Built) {
    // SAFETY: simple question sur notre fenêtre.
    if unsafe { IsIconic(built.panel.hwnd) } != 0 {
        return;
    }
    for card in &built.cards {
        let count = card.frames.borrow().images.len();
        if count > 1 {
            card.shown.set((card.shown.get() + 1) % count);
            built.panel.invalidate_part(&card.image.get());
        }
    }
}

/// Ce que Windows dit à la galerie.
unsafe extern "system" fn gallery_proc(
    hwnd: HWND,
    message: u32,
    wparam: WPARAM,
    lparam: LPARAM,
) -> LRESULT {
    if message == WM_DESTROY {
        let mut now = RECT::default();
        // SAFETY: la fenêtre vit encore ; `now` est à nous ; la minuterie
        // est la sienne.
        unsafe {
            KillTimer(hwnd, TIMER);
            if GetWindowRect(hwnd, &mut now) != 0 {
                PLACE.set(Some((now.left, now.top)));
            }
        }
    }
    if let Some(built) = current().filter(|built| built.panel.hwnd == hwnd) {
        match message {
            WM_COMMAND if lparam != 0 && ((wparam >> 16) & 0xFFFF) as u32 == BN_CLICKED => {
                let pressed = built
                    .cards
                    .iter()
                    .find(|card| card.button.0 == lparam as HWND);
                if let Some(card) = pressed {
                    let choice = card.choices.as_ref().map_or(0, |(list, _)| {
                        // SAFETY: message sans pointeur à un contrôle vivant.
                        unsafe { SendMessageW(*list, CB_GETCURSEL, 0, 0) }
                    });
                    emit(UiEvent::GalleryAdd(
                        card.id,
                        u8::try_from(choice).unwrap_or(0),
                    ));
                    return 0;
                }
            }
            WM_TIMER if wparam == TIMER => {
                tick(&built);
                return 0;
            }
            WM_NCDESTROY => GALLERY.with(|slot| {
                if let Ok(mut slot) = slot.try_borrow_mut() {
                    *slot = None;
                }
            }),
            _ => {}
        }
        let previews =
            |hdc: HDC, clip: &RECT, scroll: i32| draw_previews(&built, hdc, clip, scroll);
        match built.panel.handle(message, wparam, lparam, &previews) {
            Handled::Done(result) => return result,
            Handled::Relayout(place) => {
                // (Les aperçus, faits pour l'ancienne échelle, restent tels
                // quels jusqu'à la prochaine ouverture.)
                built.panel.set_max_view(built.panel.px(MAX_HEIGHT));
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

/// Montre la galerie (la refait si les cartes ont changé) au premier plan.
#[must_use]
pub fn show_gallery(title: &str, cards: &[GalleryCard]) -> bool {
    let known = current().filter(|built| built.title == title && built.given == cards);
    let built = match known {
        Some(built) => built,
        None => {
            // L'ancienne, décrite autrement, est défaite avec ses aperçus.
            drop(GALLERY.with(|slot| slot.borrow_mut().take()));
            let Some(built) = build(title, cards) else {
                return false;
            };
            GALLERY.with(|slot| *slot.borrow_mut() = Some(Rc::clone(&built)));
            layout(&built);
            built.panel.settle(PLACE.get());
            // SAFETY: notre fenêtre, sur ce fil ; la minuterie s'arrête avec
            // elle.
            unsafe { SetTimer(built.panel.hwnd, TIMER, TIMER_MS, None) };
            built
        }
    };
    built.panel.show();
    true
}

/// L'aperçu animé de la carte `id` : ses images, montrées en boucle tant
/// que la galerie est ouverte.
pub fn set_gallery_preview(id: u32, frames: &[GalleryImage]) {
    let Some(built) = current() else {
        return;
    };
    let Some(card) = built.cards.iter().find(|card| card.id == id) else {
        return;
    };
    let (_, inner) = card_widths(&built.panel);
    let prepared = prepare(frames, built.panel.dpi(), inner, built.panel.px(PREVIEW));
    *card.frames.borrow_mut() = prepared;
    card.shown.set(0);
    built.panel.invalidate_part(&card.image.get());
}

/// Enregistre en PNG l'image de la galerie entière, en apparence claire ou
/// sombre (la vitrine de la CI). `false` si elle n'est pas ouverte.
#[must_use]
pub fn snapshot_gallery(path: &Path, dark: bool) -> bool {
    current().is_some_and(|built| built.panel.snapshot(path, dark))
}

#[cfg(test)]
pub(super) mod tests {
    use windows_sys::Win32::UI::WindowsAndMessaging::WM_CLOSE;

    use super::super::ui::send;
    use super::*;

    /// Des cartes comme l'app en décrit : deux familles, des cartes avec
    /// et sans liste de choix.
    pub(in crate::windows) fn sample() -> Vec<GalleryCard> {
        let card = |id: u32, name: &str, family: &str, choices: &[&str]| GalleryCard {
            id,
            name: name.to_owned(),
            detail: format!(
                "{name}, une race rustique à la toison épaisse, vive et grégaire.\npar \
                 Alejandro San Martin Lamas"
            ),
            button: "Ajouter".to_owned(),
            choices: choices.iter().map(|choice| (*choice).to_owned()).collect(),
            family: family.to_owned(),
        };
        let sexes = ["Au hasard", "Un bélier", "Une brebis", "Un agneau"];
        vec![
            card(0, "Bleu du Maine", "France", &sexes),
            card(1, "Floucat", "France", &[]),
            card(2, "Solognote", "France", &sexes),
            card(3, "Mérinos d'Arles", "France", &sexes),
            card(7, "Jacob", "Îles Britanniques", &sexes),
            card(9, "Herdwick", "Îles Britanniques", &sexes),
        ]
    }

    /// Un aperçu de deux images : un disque qui change de couleur.
    pub(in crate::windows) fn preview(tint: u8) -> Vec<GalleryImage> {
        [tint, 255 - tint]
            .iter()
            .map(|&shade| GalleryImage {
                width: 160,
                height: 120,
                rgba: image(160, 120, |x, y| {
                    let (dx, dy) = (x as i32 - 80, y as i32 - 70);
                    if dx * dx + dy * dy < 40 * 40 {
                        [shade, 120, 255 - shade, 255]
                    } else {
                        [0, 0, 0, 0]
                    }
                }),
                scale: 2.0,
            })
            .collect()
    }

    #[test]
    fn the_gallery_shows_its_cards_and_adds_what_the_user_chooses() {
        use std::rc::Rc;

        let events = Rc::new(RefCell::new(Vec::new()));
        {
            let events = Rc::clone(&events);
            super::super::host::set_ui_handler(move |event| events.borrow_mut().push(event));
        }
        let cards = sample();
        assert!(show_gallery("Galerie des animaux", &cards));
        let built = current().expect("la galerie est ouverte");
        assert!(built.panel.is_shown());
        assert_eq!(built.cards.len(), cards.len());
        assert_eq!(built.families.len(), 2, "deux familles, deux titres");
        // Plus haute que ce qu'elle montre : elle défile.
        assert!(built.panel.content_height() > built.panel.px(MAX_HEIGHT));

        // Les aperçus arrivent, puis s'animent.
        set_gallery_preview(7, &preview(40));
        set_gallery_preview(42, &preview(40));
        let jacob = &built.cards[4];
        assert_eq!(jacob.frames.borrow().images.len(), 2);
        assert_eq!(jacob.shown.get(), 0);
        send(built.panel.hwnd, WM_TIMER, TIMER, 0);
        assert_eq!(jacob.shown.get(), 1);
        send(built.panel.hwnd, WM_TIMER, TIMER, 0);
        assert_eq!(jacob.shown.get(), 0);
        assert!(built.cards[0].frames.borrow().images.is_empty());

        // « Ajouter » dit quelle carte, et ce que sa liste montrait.
        let press = |card: &Card| {
            send(
                built.panel.hwnd,
                WM_COMMAND,
                (BN_CLICKED as usize) << 16,
                card.button.0 as LPARAM,
            );
        };
        let (list, _) = jacob.choices.as_ref().expect("une liste de choix");
        send(*list, CB_SETCURSEL, 2, 0);
        press(jacob);
        press(&built.cards[1]);
        assert_eq!(
            events.borrow_mut().drain(..).collect::<Vec<_>>(),
            [UiEvent::GalleryAdd(7, 2), UiEvent::GalleryAdd(1, 0)]
        );

        // Les mêmes cartes ne refont pas la galerie (ses aperçus restent).
        let hwnd = built.panel.hwnd;
        assert!(show_gallery("Galerie des animaux", &cards));
        assert_eq!(current().expect("toujours là").panel.hwnd, hwnd);
        assert_eq!(jacob.frames.borrow().images.len(), 2);

        // Fermée, elle est défaite, ses aperçus avec elle.
        send(hwnd, WM_CLOSE, 0, 0);
        assert!(current().is_none());
        set_gallery_preview(7, &preview(40));
        assert!(!snapshot_gallery(Path::new("nulle-part.png"), false));
    }

    /// Une image RGBA de `width` sur `height`, d'après la couleur de chaque
    /// pixel.
    fn image(width: usize, height: usize, color: impl Fn(usize, usize) -> [u8; 4]) -> Vec<u8> {
        (0..height)
            .flat_map(|y| (0..width).map(move |x| (x, y)))
            .flat_map(|(x, y)| color(x, y))
            .collect()
    }

    #[test]
    fn an_image_kept_at_its_size_only_changes_the_order_of_its_colours() {
        let source = image(3, 2, |x, y| [10 * x as u8, 20 * y as u8, 7, 200]);
        let same = resample(&source, 3, [0, 0, 3, 2], 3, 2);
        for (from, to) in source
            .as_chunks::<4>()
            .0
            .iter()
            .zip(same.as_chunks::<4>().0)
        {
            assert_eq!([from[2], from[1], from[0], from[3]], *to);
        }
    }

    #[test]
    fn an_image_halved_averages_its_pixels_four_by_four() {
        // Un damier : opaque et blanc, ou transparent.
        let source = image(4, 4, |x, y| {
            if (x + y) % 2 == 0 {
                [255, 255, 255, 255]
            } else {
                [0, 0, 0, 0]
            }
        });
        let half = resample(&source, 4, [0, 0, 4, 4], 2, 2);
        for pixel in half.as_chunks::<4>().0 {
            assert_eq!(*pixel, [128, 128, 128, 128]);
        }
        // À deux tiers : chaque pixel reste une couleur prémultipliée (pas
        // plus de couleur que d'opacité), et le tout garde sa moyenne.
        let smaller = resample(&source, 4, [0, 0, 4, 4], 3, 3);
        let mut total = 0u32;
        for pixel in smaller.as_chunks::<4>().0 {
            assert!(pixel[..3].iter().all(|value| *value <= pixel[3]));
            total += u32::from(pixel[3]);
        }
        assert!((total as f64 / 9.0 - 127.5).abs() < 2.0, "{total}");
    }

    #[test]
    fn only_the_part_where_the_animal_shows_is_kept() {
        let empty = image(8, 6, |_, _| [0, 0, 0, 0]);
        assert_eq!(visible_box(&[&empty], 8, 6), None);
        let first = image(8, 6, |x, y| {
            if (2..4).contains(&x) && y == 3 {
                [9, 9, 9, 9]
            } else {
                [0, 0, 0, 0]
            }
        });
        let second = image(8, 6, |x, y| {
            if x == 6 && (1..3).contains(&y) {
                [9, 9, 9, 9]
            } else {
                [0, 0, 0, 0]
            }
        });
        // La même part pour toutes les images : l'animal ne saute pas de
        // l'une à l'autre.
        assert_eq!(visible_box(&[&first, &second], 8, 6), Some([2, 1, 7, 4]));
        let cut = resample(&first, 8, [2, 1, 7, 4], 5, 3);
        assert_eq!(cut.len(), 5 * 3 * 4);
        assert_eq!(&cut[(2 * 5) * 4..][..4], [9, 9, 9, 9]);
        assert_eq!(&cut[(2 * 5 + 2) * 4..][..4], [0, 0, 0, 0]);
    }

    #[test]
    fn a_preview_is_shown_at_its_size_or_smaller_never_larger() {
        assert_eq!(fitted(100.0, 80.0, 173.0, 120.0), (100.0, 80.0));
        assert_eq!(fitted(346.0, 120.0, 173.0, 120.0), (173.0, 60.0));
        assert_eq!(fitted(100.0, 240.0, 173.0, 120.0), (50.0, 120.0));
    }

    #[test]
    fn a_preview_is_made_for_the_scale_of_its_screen() {
        // Dessiné à deux pixels par point, 40 points sur 30 ; l'animal en
        // occupe le milieu.
        let given = GalleryImage {
            width: 80,
            height: 60,
            rgba: image(80, 60, |x, y| {
                if (20..60).contains(&x) && (10..50).contains(&y) {
                    [50, 60, 70, 255]
                } else {
                    [0, 0, 0, 0]
                }
            }),
            scale: 2.0,
        };
        // Écran ordinaire : un pixel par point.
        let frames = prepare(std::slice::from_ref(&given), 96, 173, 120);
        assert_eq!((frames.width, frames.height), (20, 20));
        // Au milieu de sa place : (173 − 40) / 2 + 10, (120 − 30) / 2 + 5.
        assert_eq!((frames.left, frames.top), (77, 50));
        assert_eq!(&frames.images[0][..4], [70, 60, 50, 255]);
        // Écran à 200 % : ses pixels, tels quels.
        let frames = prepare(std::slice::from_ref(&given), 192, 346, 240);
        assert_eq!((frames.width, frames.height), (40, 40));
        // Sans image, ou toute transparente : rien.
        assert!(prepare(&[], 96, 173, 120).images.is_empty());
    }

    #[test]
    fn a_preview_is_laid_over_the_colour_of_its_card() {
        // Fond : rouge 10, vert 20, bleu 30 (comme Windows note une couleur).
        let ground = 10 | 20 << 8 | 30 << 16;
        let mut out = Vec::new();
        // Transparent, opaque, à moitié (BGRA prémultiplié).
        over(
            &[0, 0, 0, 0, 1, 2, 3, 255, 100, 100, 100, 128],
            ground,
            &mut out,
        );
        assert_eq!(&out[..4], [30, 20, 10, 255]);
        assert_eq!(&out[4..8], [1, 2, 3, 255]);
        assert_eq!(&out[8..], [115, 110, 105, 255]);
    }
}
