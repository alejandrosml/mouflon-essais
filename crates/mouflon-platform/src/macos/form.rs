//! La fenêtre des réglages de Mouflon.
//!
//! L'app la décrit ([`Form`]) et donne ce qu'elle doit montrer
//! ([`FormValue`]) ; elle est mise en page ici, de haut en bas : des
//! sections, chacune un titre et un cadre arrondi où les lignes se suivent,
//! séparées d'un trait. Une case peut avoir un complément (remarque et
//! bouton) qui n'apparaît qu'au besoin : la fenêtre s'allonge alors vers le
//! bas, son haut ne bouge pas.

use std::cell::{Cell, RefCell};
use std::path::Path;

use objc2::rc::Retained;
use objc2::runtime::{AnyObject, NSObject, ProtocolObject};
use objc2::{ClassType, MainThreadMarker, MainThreadOnly, define_class, msg_send, sel};
use objc2_app_kit::{
    NSAppearance, NSAppearanceCustomization, NSAppearanceNameAqua, NSAppearanceNameDarkAqua,
    NSApplication, NSBackingStoreType, NSBitmapImageFileType, NSBox, NSBoxType, NSButton, NSColor,
    NSControlStateValueOff, NSControlStateValueOn, NSFont, NSMenu, NSMenuItem, NSPopUpButton,
    NSResponder, NSSlider, NSTextField, NSTitlePosition, NSView, NSWindow, NSWindowStyleMask,
};
use objc2_foundation::{NSDictionary, NSPoint, NSRect, NSSize, NSString, ns_string};

use super::actions::{Actions, actions};
use super::ui::activate;
use crate::{Form, FormButton, FormRow, FormValue};

/// Largeur du contenu (points) : celle-ci au moins, davantage si une case
/// est plus longue (selon la langue), jamais plus que l'autre.
const MIN_WIDTH: f64 = 480.0;
const MAX_WIDTH: f64 = 640.0;
/// Marges de la fenêtre.
const MARGIN: f64 = 20.0;
/// Entre deux sections, et entre le titre d'une section et son cadre.
const SECTION_GAP: f64 = 18.0;
const HEADER_GAP: f64 = 6.0;
/// Marges d'une ligne dans son cadre.
const PAD_X: f64 = 14.0;
const PAD_Y: f64 = 10.0;
/// Retrait de l'explication sous une case, s'il ne peut pas être mesuré :
/// elle s'aligne sur le libellé de la case.
const INDENT: f64 = 20.0;
/// Entre une case et son explication, son explication et son complément.
const HELP_GAP: f64 = 2.0;
const EXTRA_GAP: f64 = 8.0;
/// Entre un libellé et son curseur, une remarque et son bouton.
const SMALL_GAP: f64 = 6.0;
/// Entre un texte et le bouton à sa droite.
const BUTTON_GAP: f64 = 12.0;
/// Arrondi des cadres.
pub(super) const CORNER: f64 = 10.0;

define_class!(
    // SAFETY:
    // - NSView n'impose rien à ses sous-classes ici : seule `isFlipped` est
    //   remplacée.
    // - `FlippedView` n'implémente pas `Drop`.
    #[unsafe(super(NSView, NSResponder, NSObject))]
    #[thread_kind = MainThreadOnly]
    #[name = "MouflonFormView"]
    pub(super) struct FlippedView;

    impl FlippedView {
        // SAFETY: même signature que `-[NSView isFlipped]`.
        #[unsafe(method(isFlipped))]
        fn is_flipped(&self) -> bool {
            // Repère de haut en bas : la mise en page part du haut.
            true
        }
    }
);

impl FlippedView {
    pub(super) fn new(mtm: MainThreadMarker) -> Retained<Self> {
        let this = Self::alloc(mtm).set_ivars(());
        let frame = NSRect::new(NSPoint::new(0.0, 0.0), NSSize::new(MIN_WIDTH, 100.0));
        // SAFETY: `initWithFrame:` est l'initialiseur désigné de NSView.
        unsafe { msg_send![super(this), initWithFrame: frame] }
    }
}

/// Un complément de case : montré ou caché.
struct Extra {
    id: u32,
    note: Retained<NSTextField>,
    button: Retained<NSButton>,
    shown: bool,
}

enum RowViews {
    Check {
        id: u32,
        check: Retained<NSButton>,
        help: Option<Retained<NSTextField>>,
        extra: Option<Extra>,
    },
    Steps {
        id: u32,
        label: Retained<NSTextField>,
        slider: Retained<NSSlider>,
        marks: Vec<Retained<NSTextField>>,
        help: Option<Retained<NSTextField>>,
    },
    Text {
        text: Retained<NSTextField>,
        button: Option<Retained<NSButton>>,
    },
    Choice {
        id: u32,
        label: Retained<NSTextField>,
        popup: Retained<NSPopUpButton>,
        help: Option<Retained<NSTextField>>,
    },
}

struct SectionViews {
    header: Retained<NSTextField>,
    frame: Retained<NSBox>,
    /// Traits entre les lignes (un de moins que de lignes).
    lines: Vec<Retained<NSBox>>,
    rows: Vec<RowViews>,
}

struct Built {
    /// Ce que l'app a décrit : une autre description refait la fenêtre.
    form: Form,
    window: Retained<NSWindow>,
    root: Retained<FlippedView>,
    width: f64,
    sections: Vec<SectionViews>,
}

thread_local! {
    /// La fenêtre, gardée fermée entre deux ouvertures.
    static FORM: RefCell<Option<Built>> = const { RefCell::new(None) };
}

pub(super) fn rect(x: f64, y: f64, w: f64, h: f64) -> NSRect {
    NSRect::new(NSPoint::new(x, y), NSSize::new(w, h))
}

/// Place un contrôle d'après son contour visible (sans les marges que
/// certains, comme les boutons, gardent autour de leur dessin).
pub(super) fn place(view: &NSView, x: f64, y: f64, w: f64, h: f64) {
    let insets = view.alignmentRectInsets();
    // Repère de haut en bas : la marge du haut se retranche de y.
    view.setFrame(rect(
        x - insets.left,
        y - insets.top,
        w + insets.left + insets.right,
        h + insets.top + insets.bottom,
    ));
}

pub(super) fn label(text: &str, mtm: MainThreadMarker) -> Retained<NSTextField> {
    NSTextField::labelWithString(&NSString::from_str(text), mtm)
}

/// Texte sur plusieurs lignes, en petit et en gris s'il explique.
pub(super) fn wrapping(text: &str, small: bool, mtm: MainThreadMarker) -> Retained<NSTextField> {
    let field = NSTextField::wrappingLabelWithString(&NSString::from_str(text), mtm);
    field.setSelectable(false);
    if small {
        field.setFont(Some(&NSFont::systemFontOfSize(
            NSFont::smallSystemFontSize(),
        )));
    }
    field.setTextColor(Some(&NSColor::secondaryLabelColor()));
    field
}

/// Hauteur d'un texte mis en page sur cette largeur : celle qu'il prendra
/// vraiment (le même calcul qu'AppKit pour le dessiner, coupures du
/// japonais et du chinois comprises).
pub(super) fn wrapped_height(field: &NSTextField, width: f64) -> f64 {
    field.setPreferredMaxLayoutWidth(width);
    field.intrinsicContentSize().height.max(0.0).ceil()
}

fn push_button(
    button: &FormButton,
    target: &AnyObject,
    mtm: MainThreadMarker,
) -> Retained<NSButton> {
    // SAFETY: `press:` est une action d'`Actions` (la cible), qui vit
    // jusqu'à la fin du processus.
    let view = unsafe {
        NSButton::buttonWithTitle_target_action(
            &NSString::from_str(&button.label),
            Some(target),
            Some(sel!(press:)),
            mtm,
        )
    };
    view.setTag(button.id as isize);
    view
}

fn build_row(row: &FormRow, root: &NSView, target: &AnyObject, mtm: MainThreadMarker) -> RowViews {
    match row {
        FormRow::Check {
            id,
            label: text,
            help,
            extra,
        } => {
            // SAFETY: `check:` est une action d'`Actions` (la cible).
            let check = unsafe {
                NSButton::checkboxWithTitle_target_action(
                    &NSString::from_str(text),
                    Some(target),
                    Some(sel!(check:)),
                    mtm,
                )
            };
            check.setTag(*id as isize);
            root.addSubview(&check);
            let help = help.as_ref().map(|text| {
                let field = wrapping(text, true, mtm);
                root.addSubview(&field);
                field
            });
            let extra = extra.as_ref().map(|extra| {
                let note = wrapping(&extra.note, true, mtm);
                let button = push_button(&extra.button, target, mtm);
                note.setHidden(true);
                button.setHidden(true);
                root.addSubview(&note);
                root.addSubview(&button);
                Extra {
                    id: extra.button.id,
                    note,
                    button,
                    shown: false,
                }
            });
            RowViews::Check {
                id: *id,
                check,
                help,
                extra,
            }
        }
        FormRow::Steps {
            id,
            label: text,
            steps,
            help,
        } => {
            let title = label(text, mtm);
            root.addSubview(&title);
            let last = steps.len().saturating_sub(1).max(1) as f64;
            // SAFETY: `step:` est une action d'`Actions` (la cible).
            let slider = unsafe {
                NSSlider::sliderWithValue_minValue_maxValue_target_action(
                    0.0,
                    0.0,
                    last,
                    Some(target),
                    Some(sel!(step:)),
                    mtm,
                )
            };
            slider.setTag(*id as isize);
            slider.setNumberOfTickMarks(steps.len() as isize);
            slider.setAllowsTickMarkValuesOnly(true);
            // Une seule action, quand on lâche le curseur : chaque cran
            // redessine les animaux.
            slider.setContinuous(false);
            root.addSubview(&slider);
            let small = NSFont::systemFontOfSize(NSFont::smallSystemFontSize());
            let marks = steps
                .iter()
                .map(|step| {
                    let mark = label(step, mtm);
                    mark.setFont(Some(&small));
                    mark.setTextColor(Some(&NSColor::secondaryLabelColor()));
                    root.addSubview(&mark);
                    mark
                })
                .collect();
            let help = help.as_ref().map(|text| {
                let field = wrapping(text, true, mtm);
                root.addSubview(&field);
                field
            });
            RowViews::Steps {
                id: *id,
                label: title,
                slider,
                marks,
                help,
            }
        }
        FormRow::Text { text, button } => {
            let text = wrapping(text, false, mtm);
            root.addSubview(&text);
            let button = button.as_ref().map(|button| {
                let view = push_button(button, target, mtm);
                root.addSubview(&view);
                view
            });
            RowViews::Text { text, button }
        }
        FormRow::Choice {
            id,
            label: text,
            choices,
            help,
        } => {
            let title = label(text, mtm);
            root.addSubview(&title);
            let popup = NSPopUpButton::initWithFrame_pullsDown(
                NSPopUpButton::alloc(mtm),
                rect(0.0, 0.0, 160.0, 24.0),
                false,
            );
            for choice in choices {
                popup.addItemWithTitle(&NSString::from_str(choice));
            }
            popup.setTag(*id as isize);
            // SAFETY: `pick:` est une action d'`Actions` (la cible), qui vit
            // jusqu'à la fin du processus.
            unsafe {
                popup.setTarget(Some(target));
                popup.setAction(Some(sel!(pick:)));
            }
            root.addSubview(&popup);
            let help = help.as_ref().map(|text| {
                let field = wrapping(text, true, mtm);
                root.addSubview(&field);
                field
            });
            RowViews::Choice {
                id: *id,
                label: title,
                popup,
                help,
            }
        }
    }
}

fn build(form: &Form, mtm: MainThreadMarker) -> Built {
    let actions: Retained<Actions> = actions(mtm);
    let target: &AnyObject = actions.as_ref();
    let root = FlippedView::new(mtm);
    let mut sections = Vec::new();
    for section in &form.sections {
        let header = label(&section.title, mtm);
        header.setFont(Some(
            &NSFont::boldSystemFontOfSize(NSFont::systemFontSize()),
        ));
        root.addSubview(&header);
        // Le cadre d'abord : les lignes se dessinent par-dessus.
        let frame = NSBox::new(mtm);
        frame.setBoxType(NSBoxType::Custom);
        frame.setTitlePosition(NSTitlePosition::NoTitle);
        // Couleurs du système : justes en clair comme en sombre.
        frame.setFillColor(&NSColor::controlBackgroundColor());
        frame.setBorderColor(&NSColor::separatorColor());
        frame.setBorderWidth(1.0);
        frame.setCornerRadius(CORNER);
        root.addSubview(&frame);
        let mut lines = Vec::new();
        let mut rows = Vec::new();
        for (i, row) in section.rows.iter().enumerate() {
            if i > 0 {
                let line = NSBox::new(mtm);
                line.setBoxType(NSBoxType::Separator);
                root.addSubview(&line);
                lines.push(line);
            }
            rows.push(build_row(row, root.as_super(), target, mtm));
        }
        sections.push(SectionViews {
            header,
            frame,
            lines,
            rows,
        });
    }

    let style = NSWindowStyleMask::Titled | NSWindowStyleMask::Closable;
    // SAFETY: fenêtre créée sur le fil principal et gardée ici ; AppKit ne
    // la libère pas à la fermeture (réglé juste après).
    let window = unsafe {
        NSWindow::initWithContentRect_styleMask_backing_defer(
            NSWindow::alloc(mtm),
            rect(0.0, 0.0, MIN_WIDTH, 100.0),
            style,
            NSBackingStoreType::Buffered,
            false,
        )
    };
    // SAFETY: la fenêtre est à nous (`Retained`) : AppKit ne doit pas la
    // libérer en plus quand on la ferme.
    unsafe { window.setReleasedWhenClosed(false) };
    window.setTitle(&NSString::from_str(&form.title));
    window.setContentView(Some(root.as_super()));
    // Tab passe d'un contrôle à l'autre de haut en bas (accès complet au clavier).
    window.setAutorecalculatesKeyViewLoop(true);
    window.setDelegate(Some(ProtocolObject::from_ref(&*actions)));

    // Assez large pour la plus longue case.
    let widest = sections
        .iter()
        .flat_map(|section| &section.rows)
        .filter_map(|row| match row {
            RowViews::Check { check, .. } => Some(check.intrinsicContentSize().width),
            RowViews::Choice { label, popup, .. } => Some(
                label.intrinsicContentSize().width
                    + BUTTON_GAP
                    + popup.intrinsicContentSize().width,
            ),
            _ => None,
        })
        .fold(0.0, f64::max);
    let width = (widest + 2.0 * (MARGIN + PAD_X))
        .clamp(MIN_WIDTH, MAX_WIDTH)
        .ceil();
    Built {
        form: form.clone(),
        window,
        root,
        width,
        sections,
    }
}

/// Décalage du libellé d'une case par rapport à son bord gauche.
fn title_indent(check: &NSButton) -> f64 {
    let Some(cell) = check.cell() else {
        return INDENT;
    };
    let title = cell.titleRectForBounds(check.bounds());
    let indent = title.origin.x - check.alignmentRectInsets().left;
    if (12.0..=32.0).contains(&indent) {
        indent
    } else {
        INDENT
    }
}

/// Abscisse du milieu de chaque repère d'un curseur posé en `x`, sur
/// `width`. AppKit ne calcule ces repères qu'au premier dessin du curseur :
/// avant, ils sont répartis sur toute sa largeur, comme il le fera à
/// quelques points près (la mise en page est refaite après ce dessin).
fn tick_centers(slider: &NSSlider, x: f64, width: f64) -> Vec<f64> {
    let count = usize::try_from(slider.numberOfTickMarks()).unwrap_or(0);
    let left = slider.frame().origin.x;
    let drawn: Vec<f64> = (0..count)
        .map(|i| {
            let tick = slider.rectOfTickMarkAtIndex(i as isize);
            left + tick.origin.x + tick.size.width / 2.0
        })
        .collect();
    let spread = match (drawn.first(), drawn.last()) {
        (Some(first), Some(last)) => last - first,
        _ => 0.0,
    };
    if count < 2 || (spread > width * 0.75 && drawn.windows(2).all(|w| w[1] > w[0])) {
        return drawn;
    }
    (0..count)
        .map(|i| x + width * i as f64 / (count - 1) as f64)
        .collect()
}

/// Place une ligne dont le haut est à `top` ; renvoie sa hauteur.
fn layout_row(row: &RowViews, x: f64, top: f64, width: f64) -> f64 {
    match row {
        RowViews::Check {
            check, help, extra, ..
        } => {
            let size = check.intrinsicContentSize();
            place(check, x, top, size.width.min(width), size.height);
            let indent = title_indent(check);
            let (text_x, text_w) = (x + indent, width - indent);
            let mut y = top + size.height;
            if let Some(help) = help {
                let h = wrapped_height(help, text_w);
                y += HELP_GAP;
                place(help, text_x, y, text_w, h);
                y += h;
            }
            if let Some(extra) = extra.as_ref().filter(|extra| extra.shown) {
                let h = wrapped_height(&extra.note, text_w);
                y += EXTRA_GAP;
                place(&extra.note, text_x, y, text_w, h);
                y += h + SMALL_GAP;
                let b = extra.button.intrinsicContentSize();
                place(&extra.button, text_x, y, b.width, b.height);
                y += b.height;
            }
            y - top
        }
        RowViews::Steps {
            label,
            slider,
            marks,
            help,
            ..
        } => {
            let l = label.intrinsicContentSize();
            place(label, x, top, width, l.height);
            let mut y = top + l.height + SMALL_GAP;
            let s = slider.intrinsicContentSize();
            place(slider, x, y, width, s.height);
            y += s.height + 2.0;
            // Le nom de chaque cran sous son repère, sans dépasser du curseur.
            let mut height: f64 = 0.0;
            for (mark, center) in marks.iter().zip(tick_centers(slider, x, width)) {
                let m = mark.intrinsicContentSize();
                let left = (center - m.width / 2.0).min(x + width - m.width).max(x);
                place(mark, left, y, m.width, m.height);
                height = height.max(m.height);
            }
            y += height;
            if let Some(help) = help {
                let h = wrapped_height(help, width);
                y += HELP_GAP;
                place(help, x, y, width, h);
                y += h;
            }
            y - top
        }
        RowViews::Text { text, button } => {
            let b = button
                .as_ref()
                .map_or(NSSize::new(0.0, 0.0), |b| b.intrinsicContentSize());
            let text_w = if button.is_some() {
                width - b.width - BUTTON_GAP
            } else {
                width
            };
            let h = wrapped_height(text, text_w);
            let height = h.max(b.height);
            place(text, x, top + (height - h) / 2.0, text_w, h);
            if let Some(button) = button {
                place(
                    button,
                    x + width - b.width,
                    top + (height - b.height) / 2.0,
                    b.width,
                    b.height,
                );
            }
            height
        }
        RowViews::Choice {
            label, popup, help, ..
        } => {
            let l = label.intrinsicContentSize();
            let p = popup.intrinsicContentSize();
            let popup_w = p.width.min(width * 0.6);
            let height = l.height.max(p.height);
            place(
                label,
                x,
                top + (height - l.height) / 2.0,
                (width - popup_w - BUTTON_GAP).max(0.0),
                l.height,
            );
            place(
                popup,
                x + width - popup_w,
                top + (height - p.height) / 2.0,
                popup_w,
                p.height,
            );
            let mut y = top + height;
            if let Some(help) = help {
                let h = wrapped_height(help, width);
                y += HELP_GAP;
                place(help, x, y, width, h);
                y += h;
            }
            y - top
        }
    }
}

/// Met tout en place et ajuste la hauteur de la fenêtre (son haut reste
/// où il est).
fn layout(built: &Built) {
    let boxed = built.width - 2.0 * MARGIN;
    let inner = boxed - 2.0 * PAD_X;
    let x = MARGIN + PAD_X;
    let mut y = MARGIN;
    for (k, section) in built.sections.iter().enumerate() {
        if k > 0 {
            y += SECTION_GAP;
        }
        let h = section.header.intrinsicContentSize().height;
        place(&section.header, MARGIN + 2.0, y, boxed - 4.0, h);
        y += h + HEADER_GAP;
        let top = y;
        for (i, row) in section.rows.iter().enumerate() {
            if let Some(line) = i.checked_sub(1).and_then(|j| section.lines.get(j)) {
                line.setFrame(rect(x, y, inner, 1.0));
            }
            y += PAD_Y;
            y += layout_row(row, x, y, inner);
            y += PAD_Y;
        }
        section.frame.setFrame(rect(MARGIN, top, boxed, y - top));
    }
    let size = NSSize::new(built.width, (y + MARGIN).ceil());
    let window = &built.window;
    let old = window.frame();
    let mut frame = window.frameRectForContentRect(NSRect::new(NSPoint::new(0.0, 0.0), size));
    frame.origin.x = old.origin.x;
    frame.origin.y = old.origin.y + old.size.height - frame.size.height;
    window.setFrame_display(frame, true);
}

/// Montre ce que l'app demande ; `true` s'il faut refaire la mise en page
/// (un complément apparaît ou disparaît).
fn apply(built: &mut Built, values: &[FormValue]) -> bool {
    let mut moved = false;
    let rows = built
        .sections
        .iter_mut()
        .flat_map(|section| section.rows.iter_mut());
    for row in rows {
        for value in values {
            match (&mut *row, *value) {
                (RowViews::Check { id, check, .. }, FormValue::Check(wanted, on))
                    if *id == wanted =>
                {
                    check.setState(if on {
                        NSControlStateValueOn
                    } else {
                        NSControlStateValueOff
                    });
                }
                (RowViews::Steps { id, slider, .. }, FormValue::Step(wanted, step))
                    if *id == wanted =>
                {
                    slider.setIntegerValue(step as isize);
                }
                (RowViews::Choice { id, popup, .. }, FormValue::Step(wanted, index))
                    if *id == wanted =>
                {
                    popup.selectItemAtIndex(index as isize);
                }
                (
                    RowViews::Check {
                        extra: Some(extra), ..
                    },
                    FormValue::Shown(wanted, shown),
                ) if extra.id == wanted && extra.shown != shown => {
                    extra.shown = shown;
                    extra.note.setHidden(!shown);
                    extra.button.setHidden(!shown);
                    moved = true;
                }
                _ => {}
            }
        }
    }
    moved
}

/// Raccourcis de la fenêtre des réglages : ⌘W la ferme, ⌘Q quitte Mouflon.
/// Ils passent par le menu principal de l'app, qu'une app de la barre des
/// menus ne montre jamais (il n'existe que pour eux).
fn install_key_commands(mtm: MainThreadMarker, target: &AnyObject) {
    thread_local! {
        static INSTALLED: Cell<bool> = const { Cell::new(false) };
    }
    if INSTALLED.with(|done| done.replace(true)) {
        return;
    }
    let app = NSApplication::sharedApplication(mtm);
    let commands = NSMenu::new(mtm);
    // SAFETY: `performClose:` est une méthode de NSWindow ; sans cible,
    // AppKit l'envoie à la fenêtre au premier plan.
    let close = unsafe {
        NSMenuItem::initWithTitle_action_keyEquivalent(
            NSMenuItem::alloc(mtm),
            ns_string!("Close"),
            Some(sel!(performClose:)),
            ns_string!("w"),
        )
    };
    commands.addItem(&close);
    // SAFETY: `quit:` est une action d'`Actions`, la cible donnée juste après.
    let quit = unsafe {
        NSMenuItem::initWithTitle_action_keyEquivalent(
            NSMenuItem::alloc(mtm),
            ns_string!("Quit"),
            Some(sel!(quit:)),
            ns_string!("q"),
        )
    };
    // SAFETY: la cible est un `Actions`, qui répond à `quit:`.
    unsafe { quit.setTarget(Some(target)) };
    commands.addItem(&quit);
    let item = NSMenuItem::new(mtm);
    item.setSubmenu(Some(&commands));
    let main = NSMenu::new(mtm);
    main.addItem(&item);
    app.setMainMenu(Some(&main));
}

/// Ouvre la fenêtre des réglages devant les autres apps (elle est créée à
/// la première ouverture, et refaite si l'app la décrit autrement).
pub fn show_form(form: &Form, values: &[FormValue]) -> bool {
    let Some(mtm) = MainThreadMarker::new() else {
        return false;
    };
    let actions = actions(mtm);
    install_key_commands(mtm, actions.as_ref());
    FORM.with(|slot| {
        let mut slot = slot.borrow_mut();
        let fresh = slot.as_ref().is_none_or(|built| built.form != *form);
        if fresh {
            if let Some(old) = slot.take() {
                old.window.orderOut(None);
            }
            *slot = Some(build(form, mtm));
        }
        let Some(built) = slot.as_mut() else {
            return false;
        };
        apply(built, values);
        layout(built);
        if fresh {
            // Un premier dessin, hors de l'écran, pour que le curseur
            // calcule ses repères ; les noms des crans s'y alignent ensuite.
            built.window.displayIfNeeded();
            layout(built);
            built.window.center();
        }
        activate(&NSApplication::sharedApplication(mtm));
        built.window.makeKeyAndOrderFront(None);
        // Devant même si macOS n'a pas encore fait passer Mouflon au
        // premier plan (l'activation est une demande, depuis macOS 14).
        built.window.orderFrontRegardless();
        // Devant les animaux, même s'ils couvrent tout l'écran.
        super::front::bring_forward(mtm, Some(&built.window));
        true
    })
}

/// Montre ces valeurs dans la fenêtre des réglages, si elle existe (ouverte
/// ou non) : l'app les donne à chaque changement, d'où qu'il vienne.
pub fn update_form(values: &[FormValue]) {
    if MainThreadMarker::new().is_none() {
        return;
    }
    FORM.with(|slot| {
        if let Some(built) = slot.borrow_mut().as_mut()
            && apply(built, values)
        {
            layout(built);
        }
    });
}

/// Enregistre en PNG l'image de la fenêtre des réglages, barre de titre
/// comprise, en apparence claire ou sombre : de quoi vérifier sa mise en
/// page sans écran (la CI). `false` si elle n'existe pas.
pub fn snapshot_form(path: &Path, dark: bool) -> bool {
    if MainThreadMarker::new().is_none() {
        return false;
    }
    FORM.with(|slot| {
        let slot = slot.borrow();
        let Some(built) = slot.as_ref() else {
            return false;
        };
        // SAFETY: constantes NSString d'AppKit, valides pendant toute la vie
        // du processus.
        let name = unsafe {
            if dark {
                NSAppearanceNameDarkAqua
            } else {
                NSAppearanceNameAqua
            }
        };
        built
            .window
            .setAppearance(NSAppearance::appearanceNamed(name).as_deref());
        layout(built);
        // SAFETY: lecture de la vue qui contient la nôtre (celle de la
        // fenêtre entière), sur le fil principal ; la référence comptée la
        // garde en vie le temps de l'image.
        let whole = unsafe { built.root.superview() };
        let view: &NSView = whole.as_deref().unwrap_or(built.root.as_super());
        let bounds = view.bounds();
        let saved = view
            .bitmapImageRepForCachingDisplayInRect(bounds)
            .and_then(|image| {
                view.cacheDisplayInRect_toBitmapImageRep(bounds, &image);
                let properties = NSDictionary::new();
                // SAFETY: dictionnaire vide : aucune propriété à vérifier.
                unsafe {
                    image
                        .representationUsingType_properties(NSBitmapImageFileType::PNG, &properties)
                }
            })
            .is_some_and(|png| std::fs::write(path, png.to_vec()).is_ok());
        // Retour à l'apparence du système.
        built.window.setAppearance(None);
        saved
    })
}
