//! La fenêtre des réglages de Mouflon.
//!
//! L'app la décrit ([`Form`]) et donne ce qu'elle doit montrer
//! ([`FormValue`]) ; elle est mise en page ici, de haut en bas, comme sur
//! Mac : des sections, chacune un titre et un cadre où les lignes se
//! suivent, séparées d'un trait. Une case peut avoir un complément (remarque
//! et bouton) qui n'apparaît qu'au besoin : la fenêtre s'allonge alors vers
//! le bas, son haut ne bouge pas. Plus haute que l'écran, elle défile.
//!
//! Elle est faite des contrôles de Windows (cases, curseur, liste, boutons)
//! sur un [`Panel`], aux couleurs des réglages de Windows 11, en clair comme
//! en sombre. Fermée, elle est défaite : elle ne coûte rien tant qu'on ne
//! la rouvre pas.

use std::cell::{Cell, RefCell};
use std::mem::ManuallyDrop;
use std::path::Path;
use std::rc::Rc;

use windows_sys::Win32::Foundation::{HWND, LPARAM, LRESULT, RECT, WPARAM};
use windows_sys::Win32::UI::Controls::{TBM_GETTHUMBRECT, TBM_SETPOS};
use windows_sys::Win32::UI::WindowsAndMessaging::{
    BM_GETCHECK, BM_SETCHECK, BN_CLICKED, CB_GETCURSEL, CB_SETCURSEL, CBN_SELCHANGE,
    DefWindowProcW, GetWindowRect, SWP_NOACTIVATE, SWP_NOSIZE, SWP_NOZORDER, SendMessageW,
    SetWindowPos, WA_INACTIVE, WM_ACTIVATE, WM_COMMAND, WM_DESTROY, WM_HSCROLL, WM_NCDESTROY,
};

use super::host::emit;
use super::ui::{Handled, Panel, TBM_GETPOS, Tone};
use crate::{Form, FormRow, FormValue, UiEvent};

const CLASS: &str = "MouflonReglages";
/// Largeur du contenu (points) : celle-ci au moins, davantage si une case
/// est plus longue (selon la langue), jamais plus que l'autre.
const MIN_WIDTH: i32 = 480;
const MAX_WIDTH: i32 = 640;
/// Marges de la fenêtre.
const MARGIN: i32 = 20;
/// Entre deux sections, et entre le titre d'une section et son cadre.
const SECTION_GAP: i32 = 18;
const HEADER_GAP: i32 = 6;
/// Marges d'une ligne dans son cadre.
const PAD_X: i32 = 14;
const PAD_Y: i32 = 12;
/// Entre une case et son explication, son explication et son complément.
const HELP_GAP: i32 = 3;
const EXTRA_GAP: i32 = 8;
/// Entre un libellé et son curseur, une remarque et son bouton.
const SMALL_GAP: i32 = 6;
/// Entre un texte et le bouton à sa droite.
const BUTTON_GAP: i32 = 12;
/// Le dernier mouvement d'un curseur qu'on fait glisser (`TB_THUMBTRACK`) :
/// il n'est pas encore lâché.
const SLIDING: usize = 5;

/// Un texte posé dans la fenêtre, et ce qu'il dit (pour le mesurer).
struct Label {
    hwnd: HWND,
    text: String,
}

impl Label {
    fn new(panel: &Panel, text: &str, tone: Tone) -> Self {
        Self {
            hwnd: panel.label(text, tone),
            text: text.to_owned(),
        }
    }
}

/// Un bouton, son identifiant pour l'app et son libellé.
struct Button {
    hwnd: HWND,
    id: u32,
    text: String,
}

impl Button {
    fn new(panel: &Panel, button: &crate::FormButton) -> Self {
        Self {
            hwnd: panel.button(&button.label),
            id: button.id,
            text: button.label.clone(),
        }
    }
}

/// Un complément de case : montré ou caché.
struct Extra {
    note: Label,
    button: Button,
    shown: Cell<bool>,
}

enum Row {
    Check {
        id: u32,
        check: HWND,
        text: String,
        help: Option<Label>,
        extra: Option<Extra>,
    },
    Steps {
        id: u32,
        label: Label,
        slider: HWND,
        marks: Vec<Label>,
        help: Option<Label>,
        /// Le cran que l'app connaît.
        at: Cell<usize>,
    },
    Text {
        text: Label,
        button: Option<Button>,
    },
    Choice {
        id: u32,
        label: Label,
        list: HWND,
        choices: Vec<String>,
        help: Option<Label>,
    },
}

struct Section {
    header: Label,
    rows: Vec<Row>,
}

struct Built {
    /// Ce que l'app a décrit : une autre description refait la fenêtre.
    form: Form,
    panel: Rc<Panel>,
    sections: Vec<Section>,
}

thread_local! {
    /// La fenêtre, tant qu'elle est ouverte.
    // (Jamais défait : à la sortie du programme, Windows reprend ses
    // fenêtres lui-même. Les défaire à ce moment-là rappellerait leur
    // procédure pendant que le fil se défait.)
    static FORM: ManuallyDrop<RefCell<Option<Rc<Built>>>> =
        const { ManuallyDrop::new(RefCell::new(None)) };
    /// Où elle était la dernière fois (son coin en haut à gauche).
    static PLACE: Cell<Option<(i32, i32)>> = const { Cell::new(None) };
}

fn current() -> Option<Rc<Built>> {
    FORM.with(|slot| slot.try_borrow().ok().and_then(|slot| slot.clone()))
}

fn build_row(row: &FormRow, panel: &Panel) -> Row {
    let help = |text: &Option<String>| {
        text.as_ref()
            .map(|text| Label::new(panel, text, Tone::HELP))
    };
    match row {
        FormRow::Check {
            id,
            label,
            help: explained,
            extra,
        } => {
            let check = panel.check(label);
            let help = help(explained);
            let extra = extra.as_ref().map(|extra| {
                let note = Label::new(panel, &extra.note, Tone::HELP);
                let button = Button::new(panel, &extra.button);
                panel.set_shown(note.hwnd, false);
                panel.set_shown(button.hwnd, false);
                Extra {
                    note,
                    button,
                    shown: Cell::new(false),
                }
            });
            Row::Check {
                id: *id,
                check,
                text: label.clone(),
                help,
                extra,
            }
        }
        FormRow::Steps {
            id,
            label,
            steps,
            help: explained,
        } => {
            let label = Label::new(panel, label, Tone::TEXT);
            let slider = panel.slider(steps.len());
            let marks = steps
                .iter()
                .map(|step| Label::new(panel, step, Tone::HELP))
                .collect();
            Row::Steps {
                id: *id,
                label,
                slider,
                marks,
                help: help(explained),
                at: Cell::new(0),
            }
        }
        FormRow::Text { text, button } => Row::Text {
            text: Label::new(panel, text, Tone::HELP),
            button: button.as_ref().map(|button| Button::new(panel, button)),
        },
        FormRow::Choice {
            id,
            label,
            choices,
            help: explained,
        } => {
            let label = Label::new(panel, label, Tone::TEXT);
            let list = panel.combo(choices);
            Row::Choice {
                id: *id,
                label,
                list,
                choices: choices.clone(),
                help: help(explained),
            }
        }
    }
}

fn build(form: &Form) -> Option<Rc<Built>> {
    let panel = Panel::create(CLASS, &form.title, Some(form_proc))?;
    panel.dress();
    let sections = form
        .sections
        .iter()
        .map(|section| Section {
            header: Label::new(&panel, &section.title, Tone::HEADER),
            rows: section
                .rows
                .iter()
                .map(|row| build_row(row, &panel))
                .collect(),
        })
        .collect();
    Some(Rc::new(Built {
        form: form.clone(),
        panel,
        sections,
    }))
}

/// Abscisse du milieu du curseur à chacun de ses crans (dans le contrôle).
/// Le contrôle ne dit la place que du cran où il est : on les lui fait tous
/// prendre, puis il revient au sien.
fn tick_centers(slider: HWND, count: usize) -> Vec<i32> {
    let mut centers = Vec::with_capacity(count);
    // SAFETY: messages à un contrôle vivant de ce fil ; `rect` est à nous
    // et vit jusqu'au retour de chaque appel.
    unsafe {
        let now = SendMessageW(slider, TBM_GETPOS, 0, 0);
        for step in 0..count {
            let mut rect = RECT::default();
            SendMessageW(slider, TBM_SETPOS, 1, step as LPARAM);
            SendMessageW(slider, TBM_GETTHUMBRECT, 0, &raw mut rect as LPARAM);
            centers.push((rect.left + rect.right) / 2);
        }
        SendMessageW(slider, TBM_SETPOS, 1, now);
    }
    centers
}

/// Place une ligne dont le haut est à `top` ; renvoie sa hauteur.
fn layout_row(row: &Row, panel: &Panel, x: i32, top: i32, width: i32) -> i32 {
    let line = panel.fonts().line;
    // Une explication sous le reste, sur `width` à partir de `x`.
    let explained = |help: &Option<Label>, x: i32, y: &mut i32, width: i32| {
        if let Some(help) = help {
            let height = panel.wrapped_height(&help.text, false, width);
            *y += panel.px(HELP_GAP);
            panel.place(help.hwnd, x, *y, width, height);
            *y += height;
        }
    };
    match row {
        Row::Check {
            check,
            text,
            help,
            extra,
            ..
        } => {
            let (w, h) = panel.check_size(text, width);
            panel.place(*check, x, top, w, h);
            let indent = panel.check_indent();
            let (text_x, text_w) = (x + indent, width - indent);
            let mut y = top + h;
            explained(help, text_x, &mut y, text_w);
            if let Some(extra) = extra.as_ref().filter(|extra| extra.shown.get()) {
                let height = panel.wrapped_height(&extra.note.text, false, text_w);
                y += panel.px(EXTRA_GAP);
                panel.place(extra.note.hwnd, text_x, y, text_w, height);
                y += height + panel.px(SMALL_GAP);
                let (bw, bh) = panel.button_size(&extra.button.text);
                panel.place(extra.button.hwnd, text_x, y, bw.min(text_w), bh);
                y += bh;
            }
            y - top
        }
        Row::Steps {
            label,
            slider,
            marks,
            help,
            ..
        } => {
            panel.place(label.hwnd, x, top, width, line);
            let mut y = top + line + panel.px(SMALL_GAP);
            let height = panel.slider_height();
            panel.place(*slider, x, y, width, height);
            y += height + panel.px(2);
            // Le nom de chaque cran sous son repère, sans dépasser du
            // curseur.
            let centers = tick_centers(*slider, marks.len());
            let spread = match (centers.first(), centers.last()) {
                (Some(first), Some(last)) => last - first,
                _ => 0,
            };
            let even = |i: usize| width * i as i32 / (marks.len().max(2) - 1) as i32;
            for (i, mark) in marks.iter().enumerate() {
                // (Un contrôle qui ne dit pas ses repères : répartis.)
                let center = if spread > width / 2 {
                    centers[i]
                } else {
                    even(i)
                };
                let (w, _) = panel.text_size(&mark.text, false);
                let left = (x + center - w / 2).min(x + width - w).max(x);
                panel.place(mark.hwnd, left, y, w, line);
            }
            if !marks.is_empty() {
                y += line;
            }
            explained(help, x, &mut y, width);
            y - top
        }
        Row::Text { text, button } => {
            let (bw, bh) = button
                .as_ref()
                .map_or((0, 0), |button| panel.button_size(&button.text));
            let text_w = if button.is_some() {
                (width - bw - panel.px(BUTTON_GAP)).max(width / 3)
            } else {
                width
            };
            let h = panel.wrapped_height(&text.text, false, text_w);
            let height = h.max(bh);
            panel.place(text.hwnd, x, top + (height - h) / 2, text_w, h);
            if let Some(button) = button {
                let bw = bw.min(width - text_w);
                panel.place(button.hwnd, x + width - bw, top + (height - bh) / 2, bw, bh);
            }
            height
        }
        Row::Choice {
            label,
            list,
            choices,
            help,
            ..
        } => {
            let (cw, ch) = panel.combo_size(*list, choices);
            let cw = cw.min(width * 6 / 10);
            let height = line.max(ch);
            panel.place(
                label.hwnd,
                x,
                top + (height - line) / 2,
                (width - cw - panel.px(BUTTON_GAP)).max(0),
                line,
            );
            panel.place_combo(
                *list,
                x + width - cw,
                top + (height - ch) / 2,
                cw,
                choices.len(),
            );
            let mut y = top + height;
            explained(help, x, &mut y, width);
            y - top
        }
    }
}

/// Largeur du contenu : assez pour la plus longue case, ou le plus long
/// choix avec son libellé.
fn content_width(built: &Built) -> i32 {
    let panel = &built.panel;
    let widest = built
        .sections
        .iter()
        .flat_map(|section| &section.rows)
        .filter_map(|row| match row {
            Row::Check { text, .. } => {
                Some(panel.check_indent() + panel.text_size(text, false).0 + 2)
            }
            Row::Choice {
                label,
                list,
                choices,
                ..
            } => Some(
                panel.text_size(&label.text, false).0
                    + panel.px(BUTTON_GAP)
                    + panel.combo_size(*list, choices).0,
            ),
            _ => None,
        })
        .max()
        .unwrap_or(0);
    (widest + 2 * panel.px(MARGIN + PAD_X)).clamp(panel.px(MIN_WIDTH), panel.px(MAX_WIDTH))
}

/// Où couper `heights` (les hauteurs des sections, séparées de `gap`) en
/// deux colonnes aussi égales que possible : le rang de la première section
/// de la seconde colonne, et la hauteur de la plus haute des deux.
fn two_columns(heights: &[i32], gap: i32) -> (usize, i32) {
    let column =
        |sections: &[i32]| sections.iter().sum::<i32>() + gap * (sections.len() as i32 - 1).max(0);
    (1..heights.len())
        .map(|k| (k, column(&heights[..k]).max(column(&heights[k..]))))
        .min_by_key(|(_, tallest)| *tallest)
        .unwrap_or((heights.len(), column(heights)))
}

/// Met une section en place : son titre puis son cadre, dans la colonne qui
/// commence à `left`, son haut à `top`. Rend sa hauteur.
fn layout_section(
    section: &Section,
    panel: &Panel,
    (left, top, width): (i32, i32, i32),
    cards: &mut Vec<RECT>,
    lines: &mut Vec<RECT>,
) -> i32 {
    let margin = panel.px(MARGIN);
    let boxed = width - 2 * margin;
    let inner = boxed - 2 * panel.px(PAD_X);
    let x = left + margin + panel.px(PAD_X);
    let line = panel.fonts().line;
    let mut y = top;
    if section.header.text.is_empty() {
        panel.place(section.header.hwnd, left + margin, y, 0, 0);
    } else {
        panel.place(
            section.header.hwnd,
            left + margin + panel.px(2),
            y,
            boxed,
            line,
        );
        y += line + panel.px(HEADER_GAP);
    }
    let frame_top = y;
    for (i, row) in section.rows.iter().enumerate() {
        if i > 0 {
            lines.push(RECT {
                left: x,
                top: y,
                right: x + inner,
                bottom: y + 1,
            });
        }
        y += panel.px(PAD_Y);
        y += layout_row(row, panel, x, y, inner);
        y += panel.px(PAD_Y);
    }
    cards.push(RECT {
        left: left + margin,
        top: frame_top,
        right: left + margin + boxed,
        bottom: y,
    });
    y - top
}

/// Met tout en place et ajuste la fenêtre : les sections l'une sous
/// l'autre ; ou, quand l'écran n'est pas assez haut pour elles toutes mais
/// assez large, en deux colonnes (sur un portable, tout se voit ainsi sans
/// faire défiler).
fn layout(built: &Built) {
    let panel = &built.panel;
    let width = content_width(built);
    let margin = panel.px(MARGIN);
    let gap = panel.px(SECTION_GAP);
    // D'abord pour mesurer : la hauteur de chaque section.
    let (mut cards, mut lines) = (Vec::new(), Vec::new());
    let heights: Vec<i32> = built
        .sections
        .iter()
        .map(|section| layout_section(section, panel, (0, 0, width), &mut cards, &mut lines))
        .collect();
    let single = heights.iter().sum::<i32>() + gap * (heights.len() as i32 - 1).max(0);
    let (room_w, room_h) = panel.room();
    let wide = 2 * width - margin;
    let (second, tallest) = if single + 2 * margin > room_h && wide <= room_w {
        two_columns(&heights, gap)
    } else {
        (heights.len(), single)
    };
    // Puis pour de bon.
    cards.clear();
    lines.clear();
    let (mut left, mut y) = (0, margin);
    for (k, section) in built.sections.iter().enumerate() {
        if k == second {
            // La seconde colonne : une marge entre les deux, pas deux.
            (left, y) = (width - margin, margin);
        }
        y += layout_section(section, panel, (left, y, width), &mut cards, &mut lines) + gap;
    }
    panel.set_frames(cards, lines);
    let total = if second < heights.len() { wide } else { width };
    panel.fit(total, tallest + 2 * margin);
}

/// Montre ce que l'app demande ; `true` s'il faut refaire la mise en page
/// (un complément apparaît ou disparaît).
fn apply(built: &Built, values: &[FormValue]) -> bool {
    let mut moved = false;
    let rows = built.sections.iter().flat_map(|section| &section.rows);
    for row in rows {
        for value in values {
            // SAFETY (pour chaque message ci-dessous) : sans pointeur, à un
            // contrôle vivant de ce fil.
            match (row, *value) {
                (Row::Check { id, check, .. }, FormValue::Check(wanted, on)) if *id == wanted => {
                    // SAFETY: voir plus haut.
                    unsafe { SendMessageW(*check, BM_SETCHECK, usize::from(on), 0) };
                }
                (Row::Steps { id, slider, at, .. }, FormValue::Step(wanted, step))
                    if *id == wanted =>
                {
                    at.set(step);
                    // SAFETY: voir plus haut.
                    unsafe { SendMessageW(*slider, TBM_SETPOS, 1, step as LPARAM) };
                }
                (Row::Choice { id, list, .. }, FormValue::Step(wanted, index)) if *id == wanted => {
                    // SAFETY: voir plus haut.
                    unsafe { SendMessageW(*list, CB_SETCURSEL, index, 0) };
                }
                (
                    Row::Check {
                        extra: Some(extra), ..
                    },
                    FormValue::Shown(wanted, shown),
                ) if extra.button.id == wanted && extra.shown.get() != shown => {
                    extra.shown.set(shown);
                    built.panel.set_shown(extra.note.hwnd, shown);
                    built.panel.set_shown(extra.button.hwnd, shown);
                    moved = true;
                }
                _ => {}
            }
        }
    }
    moved
}

/// Ce que veut dire un geste sur un contrôle de la fenêtre : `code` est ce
/// que le contrôle annonce (un clic, un choix dans sa liste).
fn command(built: &Built, control: HWND, code: u32) -> Option<UiEvent> {
    let rows = built.sections.iter().flat_map(|section| &section.rows);
    for row in rows {
        match row {
            Row::Check {
                id, check, extra, ..
            } => {
                if *check == control && code == BN_CLICKED {
                    // SAFETY: message sans pointeur à un contrôle vivant.
                    let on = unsafe { SendMessageW(control, BM_GETCHECK, 0, 0) } == 1;
                    return Some(UiEvent::Check(*id, on));
                }
                if let Some(extra) = extra
                    && extra.button.hwnd == control
                    && code == BN_CLICKED
                {
                    return Some(UiEvent::Button(extra.button.id));
                }
            }
            Row::Text {
                button: Some(button),
                ..
            } if button.hwnd == control && code == BN_CLICKED => {
                return Some(UiEvent::Button(button.id));
            }
            Row::Choice { id, list, .. } if *list == control && code == CBN_SELCHANGE => {
                // SAFETY: message sans pointeur à un contrôle vivant.
                let index = unsafe { SendMessageW(control, CB_GETCURSEL, 0, 0) };
                return usize::try_from(index)
                    .ok()
                    .map(|index| UiEvent::Step(*id, index));
            }
            _ => {}
        }
    }
    None
}

/// Un curseur a bougé : le cran où il est, s'il vient d'en changer et qu'il
/// est lâché (une seule annonce : chaque cran redessine les animaux).
fn slid(built: &Built, control: HWND, code: usize) -> Option<UiEvent> {
    if code == SLIDING {
        return None;
    }
    built
        .sections
        .iter()
        .flat_map(|section| &section.rows)
        .find_map(|row| match row {
            Row::Steps { id, slider, at, .. } if *slider == control => {
                // SAFETY: message sans pointeur à un contrôle vivant.
                let step = unsafe { SendMessageW(control, TBM_GETPOS, 0, 0) }.max(0) as usize;
                (at.replace(step) != step).then_some(UiEvent::Step(*id, step))
            }
            _ => None,
        })
}

/// Ce que Windows dit à la fenêtre des réglages.
unsafe extern "system" fn form_proc(
    hwnd: HWND,
    message: u32,
    wparam: WPARAM,
    lparam: LPARAM,
) -> LRESULT {
    if message == WM_DESTROY {
        let mut now = RECT::default();
        // SAFETY: la fenêtre vit encore ; `now` est à nous.
        if unsafe { GetWindowRect(hwnd, &mut now) } != 0 {
            PLACE.set(Some((now.left, now.top)));
        }
    }
    if let Some(built) = current().filter(|built| built.panel.hwnd == hwnd) {
        match message {
            WM_COMMAND if lparam != 0 => {
                let code = ((wparam >> 16) & 0xFFFF) as u32;
                if let Some(event) = command(&built, lparam as HWND, code) {
                    emit(event);
                    return 0;
                }
            }
            WM_HSCROLL if lparam != 0 => {
                if let Some(event) = slid(&built, lparam as HWND, wparam & 0xFFFF) {
                    emit(event);
                }
                return 0;
            }
            // Au premier plan : ce qu'elle montre de l'état du système est
            // à relire.
            WM_ACTIVATE if (wparam & 0xFFFF) as u32 != WA_INACTIVE => emit(UiEvent::FormShown),
            WM_NCDESTROY => FORM.with(|slot| {
                if let Ok(mut slot) = slot.try_borrow_mut() {
                    *slot = None;
                }
            }),
            _ => {}
        }
        match built.panel.handle(message, wparam, lparam, &|_, _, _| {}) {
            Handled::Done(result) => return result,
            Handled::Relayout(place) => {
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

/// Ouvre la fenêtre des réglages devant les autres apps (elle est faite à
/// l'ouverture, et refaite si l'app la décrit autrement).
pub fn show_form(form: &Form, values: &[FormValue]) -> bool {
    let built = match current().filter(|built| built.form == *form) {
        Some(built) => built,
        None => {
            // L'ancienne, décrite autrement, est défaite avec ses contrôles.
            drop(FORM.with(|slot| slot.borrow_mut().take()));
            let Some(built) = build(form) else {
                return false;
            };
            FORM.with(|slot| *slot.borrow_mut() = Some(Rc::clone(&built)));
            apply(&built, values);
            layout(&built);
            built.panel.settle(PLACE.get());
            built
        }
    };
    if apply(&built, values) {
        layout(&built);
    }
    built.panel.show();
    true
}

/// Montre ces valeurs dans la fenêtre des réglages, si elle est ouverte :
/// l'app les donne à chaque changement, d'où qu'il vienne.
pub fn update_form(values: &[FormValue]) {
    if let Some(built) = current()
        && apply(&built, values)
    {
        layout(&built);
    }
}

/// Enregistre en PNG l'image de la fenêtre des réglages, barre de titre
/// comprise, en apparence claire ou sombre : de quoi vérifier sa mise en
/// page sur un vrai Windows (la CI). `false` si elle n'est pas ouverte.
pub fn snapshot_form(path: &Path, dark: bool) -> bool {
    current().is_some_and(|built| built.panel.snapshot(path, dark))
}

#[cfg(test)]
pub(super) mod tests {
    use windows_sys::Win32::UI::WindowsAndMessaging::{IsWindowVisible, WM_CLOSE};

    use super::super::ui::send;
    use super::*;
    use crate::{FormButton, FormExtra, FormSection};

    /// Une fenêtre des réglages comme l'app en décrit : un curseur, une
    /// liste, des cases (dont une à complément), un texte et son bouton.
    pub(in crate::windows) fn sample() -> Form {
        let text = |text: &str| text.to_owned();
        Form {
            title: text("Réglages de Mouflon"),
            sections: vec![
                FormSection {
                    title: text("Animaux"),
                    rows: vec![
                        FormRow::Steps {
                            id: 1,
                            label: text("Taille"),
                            steps: ["Petite", "Normale", "Grande", "Très grande"]
                                .map(text)
                                .to_vec(),
                            help: Some(text(
                                "Celle de tous les animaux. Un clic droit sur un animal ne change \
                                 que la sienne.",
                            )),
                        },
                        FormRow::Choice {
                            id: 2,
                            label: text("Humeur"),
                            choices: ["Au hasard", "Calmes", "Actifs", "Énervés"]
                                .map(text)
                                .to_vec(),
                            help: Some(text("Au hasard, chacun a la sienne.")),
                        },
                        FormRow::Check {
                            id: 3,
                            label: text("Sons & bêlements des animaux"),
                            help: None,
                            extra: None,
                        },
                    ],
                },
                FormSection {
                    title: text("Démarrage"),
                    rows: vec![
                        FormRow::Check {
                            id: 4,
                            label: text("Ouvrir Mouflon à l'ouverture de session"),
                            help: Some(text(
                                "Mouflon se lance tout seul quand tu ouvres ta session, et ses \
                                 animaux reviennent où ils étaient.",
                            )),
                            extra: Some(FormExtra {
                                note: text(
                                    "Windows a noté que tu avais coupé Mouflon au démarrage : \
                                     c'est dans ses réglages que cela se remet.",
                                ),
                                button: FormButton {
                                    id: 40,
                                    label: text("Ouvrir les réglages de Windows"),
                                },
                            }),
                        },
                        FormRow::Text {
                            text: text("Les animaux que tu ajoutes se rangent dans un dossier."),
                            button: Some(FormButton {
                                id: 50,
                                label: text("Ouvrir le dossier"),
                            }),
                        },
                    ],
                },
            ],
        }
    }

    /// Ce que l'app reçoit, sans les annonces de passage au premier plan.
    fn told(events: &Rc<RefCell<Vec<UiEvent>>>) -> Vec<UiEvent> {
        events
            .borrow_mut()
            .drain(..)
            .filter(|event| *event != UiEvent::FormShown)
            .collect()
    }

    /// Un contrôle annonce `code` à la fenêtre, comme après un geste de
    /// l'utilisateur.
    fn announce(built: &Built, control: HWND, code: u32) {
        send(
            built.panel.hwnd,
            WM_COMMAND,
            (code as usize) << 16,
            control as LPARAM,
        );
    }

    fn rows(built: &Built) -> Vec<&Row> {
        built
            .sections
            .iter()
            .flat_map(|section| &section.rows)
            .collect()
    }

    #[test]
    fn sections_split_into_two_columns_of_about_the_same_height() {
        // Cinq sections, comme les réglages : les deux premières d'un côté,
        // les trois autres de l'autre.
        assert_eq!(two_columns(&[330, 220, 180, 60, 80], 18), (2, 568));
        // Une grande section seule en face de toutes les autres.
        assert_eq!(two_columns(&[500, 100, 100], 10), (1, 500));
        assert_eq!(two_columns(&[100, 100, 500], 10), (2, 500));
        // Une seule section ne se coupe pas.
        assert_eq!(two_columns(&[300], 18), (1, 300));
        assert_eq!(two_columns(&[], 18), (0, 0));
    }

    #[test]
    fn a_thread_can_end_with_its_windows_still_open() {
        // Quitter Mouflon avec la fenêtre des réglages ouverte : le fil
        // principal se défait sans que rien ne rappelle la fenêtre (elle a
        // paniqué là, une fois : « Mouflon s'est arrêté » à chaque sortie).
        let ended = std::thread::spawn(|| {
            assert!(show_form(&sample(), &[FormValue::Check(3, true)]));
            assert!(current().is_some());
        })
        .join();
        assert!(ended.is_ok());
    }

    #[test]
    fn the_settings_window_shows_what_the_app_says_and_tells_what_the_user_does() {
        let events = Rc::new(RefCell::new(Vec::new()));
        {
            let events = Rc::clone(&events);
            super::super::host::set_ui_handler(move |event| events.borrow_mut().push(event));
        }
        let form = sample();
        let values = [
            FormValue::Step(1, 2),
            FormValue::Step(2, 1),
            FormValue::Check(3, true),
            // Une valeur pour une ligne que la fenêtre n'a pas : ignorée.
            FormValue::Check(99, true),
        ];
        assert!(show_form(&form, &values));
        let built = current().expect("la fenêtre est ouverte");
        assert!(built.panel.is_shown());
        let rows = rows(&built);
        let (Row::Steps { slider, .. }, Row::Choice { list, .. }) = (rows[0], rows[1]) else {
            panic!("un curseur puis une liste");
        };
        let (
            Row::Check { check: sounds, .. },
            Row::Check {
                check: login,
                extra,
                ..
            },
        ) = (rows[2], rows[3])
        else {
            panic!("deux cases");
        };
        let extra = extra.as_ref().expect("un complément");
        let Row::Text {
            button: Some(folder),
            ..
        } = rows[4]
        else {
            panic!("un texte et son bouton");
        };
        let read = |control: HWND, message: u32| send(control, message, 0, 0);
        // Elle montre ce que l'app a dit.
        assert_eq!(read(*slider, TBM_GETPOS), 2);
        assert_eq!(read(*list, CB_GETCURSEL), 1);
        assert_eq!(read(*sounds, BM_GETCHECK), 1);
        assert_eq!(read(*login, BM_GETCHECK), 0);
        // SAFETY: simple question sur un contrôle vivant.
        let visible = |control: HWND| unsafe { IsWindowVisible(control) } != 0;
        assert!(!visible(extra.button.hwnd), "le complément est caché");

        // L'utilisateur décoche une case, en coche une autre.
        send(*sounds, BM_SETCHECK, 0, 0);
        announce(&built, *sounds, BN_CLICKED);
        send(*login, BM_SETCHECK, 1, 0);
        announce(&built, *login, BN_CLICKED);
        assert_eq!(
            told(&events),
            [UiEvent::Check(3, false), UiEvent::Check(4, true)]
        );

        // L'app répond : un complément apparaît, la fenêtre s'allonge.
        let before = built.panel.content_height();
        update_form(&[FormValue::Check(4, true), FormValue::Shown(40, true)]);
        assert!(visible(extra.button.hwnd));
        assert!(built.panel.content_height() > before);
        announce(&built, extra.button.hwnd, BN_CLICKED);
        announce(&built, folder.hwnd, BN_CLICKED);
        assert_eq!(told(&events), [UiEvent::Button(40), UiEvent::Button(50)]);
        update_form(&[FormValue::Shown(40, false)]);
        assert!(!visible(extra.button.hwnd));
        assert_eq!(built.panel.content_height(), before);

        // Le curseur : rien tant qu'on le fait glisser, une seule annonce
        // quand on le lâche sur un autre cran.
        let slide = |to: isize, code: usize| {
            send(*slider, TBM_SETPOS, 1, to);
            send(built.panel.hwnd, WM_HSCROLL, code, *slider as LPARAM);
        };
        slide(3, SLIDING);
        assert_eq!(told(&events), []);
        slide(3, 8);
        slide(3, 8);
        assert_eq!(told(&events), [UiEvent::Step(1, 3)]);

        // La liste.
        send(*list, CB_SETCURSEL, 3, 0);
        announce(&built, *list, CBN_SELCHANGE);
        assert_eq!(told(&events), [UiEvent::Step(2, 3)]);

        // La même description ne refait pas la fenêtre ; une autre, si.
        let hwnd = built.panel.hwnd;
        assert!(show_form(&form, &[FormValue::Step(1, 0)]));
        assert_eq!(current().expect("toujours là").panel.hwnd, hwnd);
        assert_eq!(read(*slider, TBM_GETPOS), 0);

        // Fermée, elle est défaite ; l'app peut encore lui parler, sans
        // effet, et la rouvrir.
        send(hwnd, WM_CLOSE, 0, 0);
        assert!(current().is_none());
        drop(built);
        update_form(&values);
        assert!(!snapshot_form(Path::new("nulle-part.png"), false));
        assert!(show_form(&form, &values));
        let again = current().expect("rouverte");
        send(again.panel.hwnd, WM_CLOSE, 0, 0);
    }
}
