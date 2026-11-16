//! La galerie des animaux : une carte par espèce, avec son aperçu animé
//! (l'animal qui marche, dessiné par Mouflon), son nom, sa description et
//! un bouton pour en ajouter un à l'écran.
//!
//! Les cartes se rangent par trois, de haut en bas, famille par famille
//! (sous le nom de chacune, s'il y en a plusieurs) ; au-delà de la hauteur
//! de l'écran, la galerie défile. Les aperçus arrivent après coup
//! ([`set_gallery_preview`]) : l'app les dessine sur un fil à part.

use std::cell::RefCell;
use std::path::Path;

use objc2::rc::Retained;
use objc2::runtime::ProtocolObject;
use objc2::{AnyThread, MainThreadMarker, MainThreadOnly, sel};
use objc2_app_kit::{
    NSAppearance, NSAppearanceCustomization, NSAppearanceNameAqua, NSAppearanceNameDarkAqua,
    NSApplication, NSBackingStoreType, NSBitmapFormat, NSBitmapImageFileType, NSBitmapImageRep,
    NSBox, NSBoxType, NSButton, NSColor, NSDeviceRGBColorSpace, NSFont, NSImage, NSImageScaling,
    NSImageView, NSPopUpButton, NSScrollView, NSTitlePosition, NSView, NSWindow, NSWindowStyleMask,
};
use objc2_foundation::{NSDictionary, NSSize, NSString, NSTimer};

use super::actions::actions;
use super::form::{CORNER, FlippedView, label, place, rect, wrapped_height, wrapping};
use super::ui::activate;
use crate::{GalleryCard, GalleryImage};

/// Largeur du contenu, marges, écart entre les cartes, colonnes.
const WIDTH: f64 = 660.0;
const MARGIN: f64 = 20.0;
const GAP: f64 = 14.0;
const COLUMNS: usize = 3;
/// Marge intérieure d'une carte, hauteur de son aperçu.
const PAD: f64 = 12.0;
const PREVIEW: f64 = 120.0;
/// Hauteur la plus grande de la fenêtre : au-delà, elle défile.
const MAX_HEIGHT: f64 = 640.0;
/// Avant le nom d'une famille (après la précédente), et après lui.
const FAMILY_GAP: f64 = 10.0;
const HEADER_GAP: f64 = 8.0;
/// Images par seconde des aperçus.
const PREVIEW_FPS: f64 = 10.0;

struct Card {
    id: u32,
    frame: Retained<NSBox>,
    image: Retained<NSImageView>,
    name: Retained<objc2_app_kit::NSTextField>,
    detail: Retained<objc2_app_kit::NSTextField>,
    button: Retained<NSButton>,
    /// La liste de ce qu'on peut ajouter (un bélier, une brebis…), si la
    /// carte en a une.
    choices: Option<Retained<NSPopUpButton>>,
    frames: Vec<Retained<NSImage>>,
    shown: usize,
}

struct Gallery {
    /// Ce que l'app a décrit : une autre description refait la galerie.
    cards_given: Vec<GalleryCard>,
    window: Retained<NSWindow>,
    scroll: Retained<NSScrollView>,
    root: Retained<FlippedView>,
    cards: Vec<Card>,
    /// Le nom de chaque famille, quand il y en a plusieurs : le rang de sa
    /// première carte, et son titre.
    families: Vec<(usize, Retained<objc2_app_kit::NSTextField>)>,
    timer: Option<Retained<NSTimer>>,
}

thread_local! {
    static GALLERY: RefCell<Option<Gallery>> = const { RefCell::new(None) };
}

/// Une image AppKit faite de pixels RGBA prémultipliés, à `scale` pixels
/// par point.
fn ns_image(frame: &GalleryImage) -> Option<Retained<NSImage>> {
    let (w, h) = (frame.width as isize, frame.height as isize);
    if w <= 0 || h <= 0 || frame.rgba.len() < (w * h * 4) as usize {
        return None;
    }
    // SAFETY: AppKit alloue lui-même les pixels (`planes` nul) ; RGBA 8 bits,
    // alpha prémultiplié (format par défaut), lignes de `w * 4` octets.
    let rep = unsafe {
        NSBitmapImageRep::initWithBitmapDataPlanes_pixelsWide_pixelsHigh_bitsPerSample_samplesPerPixel_hasAlpha_isPlanar_colorSpaceName_bitmapFormat_bytesPerRow_bitsPerPixel(
            NSBitmapImageRep::alloc(),
            std::ptr::null_mut(),
            w,
            h,
            8,
            4,
            true,
            false,
            NSDeviceRGBColorSpace,
            NSBitmapFormat::empty(),
            w * 4,
            32,
        )
    }?;
    let data = rep.bitmapData();
    if data.is_null() {
        return None;
    }
    // SAFETY: `data` pointe vers les `w * h * 4` octets alloués par AppKit
    // pour cette image ; nos pixels en ont autant (vérifié plus haut).
    unsafe {
        std::ptr::copy_nonoverlapping(frame.rgba.as_ptr(), data, (w * h * 4) as usize);
    }
    let points = NSSize::new(
        f64::from(frame.width) / frame.scale.max(0.1),
        f64::from(frame.height) / frame.scale.max(0.1),
    );
    rep.setSize(points);
    let image = NSImage::initWithSize(NSImage::alloc(), points);
    image.addRepresentation(&rep);
    Some(image)
}

fn build(title: &str, cards: &[GalleryCard], mtm: MainThreadMarker) -> Gallery {
    let actions = actions(mtm);
    let root = FlippedView::new(mtm);
    let card_width = (WIDTH - 2.0 * MARGIN - (COLUMNS as f64 - 1.0) * GAP) / COLUMNS as f64;
    let mut built = Vec::new();
    let several = cards
        .windows(2)
        .any(|pair| pair[0].family != pair[1].family);
    let mut families = Vec::new();
    for (i, card) in cards.iter().enumerate() {
        if several && (i == 0 || cards[i - 1].family != card.family) {
            let header = label(&card.family, mtm);
            header.setFont(Some(
                &NSFont::boldSystemFontOfSize(NSFont::systemFontSize()),
            ));
            root.addSubview(&header);
            families.push((i, header));
        }
        let frame = NSBox::new(mtm);
        frame.setBoxType(NSBoxType::Custom);
        frame.setTitlePosition(NSTitlePosition::NoTitle);
        frame.setFillColor(&NSColor::controlBackgroundColor());
        frame.setBorderColor(&NSColor::separatorColor());
        frame.setBorderWidth(1.0);
        frame.setCornerRadius(CORNER);
        root.addSubview(&frame);
        let image = NSImageView::new(mtm);
        image.setImageScaling(NSImageScaling::ScaleProportionallyDown);
        root.addSubview(&image);
        let name = label(&card.name, mtm);
        name.setFont(Some(
            &NSFont::boldSystemFontOfSize(NSFont::systemFontSize()),
        ));
        root.addSubview(&name);
        let detail = wrapping(&card.detail, true, mtm);
        root.addSubview(&detail);
        // SAFETY: `gallery:` est une action d'`Actions` (la cible), qui vit
        // jusqu'à la fin du processus.
        let button = unsafe {
            NSButton::buttonWithTitle_target_action(
                &NSString::from_str(&card.button),
                Some(&actions),
                Some(sel!(gallery:)),
                mtm,
            )
        };
        button.setTag(card.id as isize);
        root.addSubview(&button);
        // À côté du bouton, ce qu'il ajoute : le bouton lit ce que la liste
        // montre (voir `choice`), elle n'a pas d'action à elle.
        let choices = (!card.choices.is_empty()).then(|| {
            let popup = NSPopUpButton::initWithFrame_pullsDown(
                NSPopUpButton::alloc(mtm),
                rect(0.0, 0.0, 110.0, 24.0),
                false,
            );
            for choice in &card.choices {
                popup.addItemWithTitle(&NSString::from_str(choice));
            }
            root.addSubview(&popup);
            popup
        });
        built.push(Card {
            id: card.id,
            frame,
            image,
            name,
            detail,
            button,
            choices,
            frames: Vec::new(),
            shown: 0,
        });
    }
    let scroll = NSScrollView::new(mtm);
    scroll.setDrawsBackground(false);
    scroll.setHasVerticalScroller(true);
    scroll.setDocumentView(Some(&root));

    let style =
        NSWindowStyleMask::Titled | NSWindowStyleMask::Closable | NSWindowStyleMask::Miniaturizable;
    // SAFETY: fenêtre créée sur le fil principal et gardée ici ; AppKit ne
    // la libère pas à la fermeture (réglé juste après).
    let window = unsafe {
        NSWindow::initWithContentRect_styleMask_backing_defer(
            NSWindow::alloc(mtm),
            rect(0.0, 0.0, WIDTH, 300.0),
            style,
            NSBackingStoreType::Buffered,
            false,
        )
    };
    // SAFETY: la fenêtre est à nous (`Retained`).
    unsafe { window.setReleasedWhenClosed(false) };
    window.setTitle(&NSString::from_str(title));
    window.setContentView(Some(&scroll));
    window.setDelegate(Some(ProtocolObject::from_ref(&*actions)));
    let gallery = Gallery {
        cards_given: cards.to_vec(),
        window,
        scroll,
        root,
        cards: built,
        families,
        timer: None,
    };
    layout(&gallery, card_width);
    gallery
}

/// Met les cartes en place, par rangées de trois, famille par famille, et
/// ajuste la fenêtre.
fn layout(gallery: &Gallery, card_width: f64) {
    let inner = card_width - 2.0 * PAD;
    let mut y = MARGIN;
    // Les familles : du rang de leur première carte à celui de la suivante.
    let mut starts: Vec<usize> = gallery.families.iter().map(|(i, _)| *i).collect();
    if starts.first() != Some(&0) {
        starts.insert(0, 0);
    }
    for (f, &start) in starts.iter().enumerate() {
        let end = starts.get(f + 1).copied().unwrap_or(gallery.cards.len());
        if let Some((_, header)) = gallery.families.iter().find(|(i, _)| *i == start) {
            if f > 0 {
                y += FAMILY_GAP;
            }
            let h = header.intrinsicContentSize().height;
            place(header, MARGIN + 2.0, y, WIDTH - 2.0 * MARGIN - 4.0, h);
            y += h + HEADER_GAP;
        }
        y = layout_rows(&gallery.cards[start..end], card_width, inner, y);
    }
    let total = (y - GAP + MARGIN).max(2.0 * MARGIN);
    gallery.root.setFrame(rect(0.0, 0.0, WIDTH, total));
    let shown = total.min(MAX_HEIGHT);
    let window = &gallery.window;
    let old = window.frame();
    let mut frame = window.frameRectForContentRect(rect(0.0, 0.0, WIDTH, shown));
    frame.origin.x = old.origin.x;
    // Le haut reste où il est.
    frame.origin.y = old.origin.y + old.size.height - frame.size.height;
    window.setFrame_display(frame, true);
    gallery.scroll.setFrame(rect(0.0, 0.0, WIDTH, shown));
}

/// Les cartes d'une famille, par rangées de trois, à partir de `y` ; rend
/// le haut de ce qui suit.
fn layout_rows(cards: &[Card], card_width: f64, inner: f64, mut y: f64) -> f64 {
    for row in cards.chunks(COLUMNS) {
        // La rangée prend la hauteur de sa plus haute carte.
        let heights: Vec<f64> = row
            .iter()
            .map(|card| {
                let name = card.name.intrinsicContentSize().height;
                let detail = wrapped_height(&card.detail, inner);
                let button = card.button.intrinsicContentSize().height;
                PAD + PREVIEW + 8.0 + name + 4.0 + detail + 12.0 + button + PAD
            })
            .collect();
        let height = heights.iter().copied().fold(0.0, f64::max);
        for (k, card) in row.iter().enumerate() {
            let x = MARGIN + k as f64 * (card_width + GAP);
            card.frame.setFrame(rect(x, y, card_width, height));
            let mut cy = y + PAD;
            card.image.setFrame(rect(x + PAD, cy, inner, PREVIEW));
            cy += PREVIEW + 8.0;
            let name = card.name.intrinsicContentSize().height;
            place(&card.name, x + PAD, cy, inner, name);
            cy += name + 4.0;
            let detail = wrapped_height(&card.detail, inner);
            place(&card.detail, x + PAD, cy, inner, detail);
            // Le bouton en bas de la carte, aligné d'une carte à l'autre.
            let b = card.button.intrinsicContentSize();
            place(
                &card.button,
                x + PAD,
                y + height - PAD - b.height,
                b.width,
                b.height,
            );
            // Sa liste de choix prend le reste de la largeur.
            if let Some(choices) = &card.choices {
                let c = choices.intrinsicContentSize();
                place(
                    choices,
                    x + PAD + b.width + 6.0,
                    y + height - PAD - 0.5 * (b.height + c.height),
                    (inner - b.width - 6.0).max(60.0),
                    c.height,
                );
            }
        }
        y += height + GAP;
    }
    y
}

/// Le rang de ce que montre la liste de choix de la carte `id` (0 sans
/// liste, ou sans galerie).
pub(super) fn choice(id: u32) -> u8 {
    GALLERY.with_borrow(|gallery| {
        gallery
            .as_ref()
            .and_then(|g| g.cards.iter().find(|card| card.id == id))
            .and_then(|card| card.choices.as_ref())
            .and_then(|popup| u8::try_from(popup.indexOfSelectedItem()).ok())
            .unwrap_or(0)
    })
}

/// Montre la galerie (la refait si les cartes ont changé) au premier plan.
pub fn show_gallery(title: &str, cards: &[GalleryCard]) -> bool {
    let Some(mtm) = MainThreadMarker::new() else {
        return false;
    };
    GALLERY.with(|slot| {
        let mut slot = slot.borrow_mut();
        let fresh = slot.as_ref().is_none_or(|g| g.cards_given != cards);
        if fresh {
            if let Some(old) = slot.take() {
                stop(&old);
                old.window.orderOut(None);
            }
            let gallery = build(title, cards, mtm);
            gallery.window.center();
            *slot = Some(gallery);
        }
        let Some(gallery) = slot.as_mut() else {
            return false;
        };
        start(gallery);
        activate(&NSApplication::sharedApplication(mtm));
        gallery.window.makeKeyAndOrderFront(None);
        gallery.window.orderFrontRegardless();
        super::front::bring_forward(mtm, Some(&gallery.window));
        true
    })
}

/// Les images de l'aperçu animé d'une carte (son identifiant).
pub fn set_gallery_preview(id: u32, frames: &[GalleryImage]) {
    if MainThreadMarker::new().is_none() {
        return;
    }
    GALLERY.with(|slot| {
        let mut slot = slot.borrow_mut();
        let Some(gallery) = slot.as_mut() else {
            return;
        };
        let Some(card) = gallery.cards.iter_mut().find(|c| c.id == id) else {
            return;
        };
        card.frames = frames.iter().filter_map(ns_image).collect();
        card.shown = 0;
        card.image.setImage(card.frames.first().map(|i| &**i));
    });
}

/// L'image suivante de chaque aperçu (la minuterie de la galerie).
pub(super) fn tick() {
    GALLERY.with(|slot| {
        let Ok(mut slot) = slot.try_borrow_mut() else {
            return;
        };
        let Some(gallery) = slot.as_mut() else {
            return;
        };
        if !gallery.window.isVisible() {
            stop(gallery);
            gallery.timer = None;
            return;
        }
        for card in &mut gallery.cards {
            if card.frames.len() > 1 {
                card.shown = (card.shown + 1) % card.frames.len();
                card.image.setImage(Some(&card.frames[card.shown]));
            }
        }
    });
}

/// La galerie se ferme : ses aperçus s'arrêtent.
pub(super) fn closed(window: &NSWindow) {
    GALLERY.with(|slot| {
        let Ok(mut slot) = slot.try_borrow_mut() else {
            return;
        };
        if let Some(gallery) = slot.as_mut()
            && std::ptr::eq(&*gallery.window, window)
        {
            stop(gallery);
            gallery.timer = None;
        }
    });
}

fn start(gallery: &mut Gallery) {
    if gallery.timer.is_some() {
        return;
    }
    let Some(mtm) = MainThreadMarker::new() else {
        return;
    };
    let actions = actions(mtm);
    // SAFETY: `galleryTick:` est une action d'`Actions`, qui vit jusqu'à la
    // fin du processus ; la minuterie est arrêtée quand la galerie se ferme.
    let timer = unsafe {
        NSTimer::scheduledTimerWithTimeInterval_target_selector_userInfo_repeats(
            1.0 / PREVIEW_FPS,
            &actions,
            sel!(galleryTick:),
            None,
            true,
        )
    };
    gallery.timer = Some(timer);
}

fn stop(gallery: &Gallery) {
    if let Some(timer) = &gallery.timer {
        timer.invalidate();
    }
}

/// Enregistre en PNG l'image de la galerie, en apparence claire ou sombre
/// (la vitrine de la CI). `false` si elle n'existe pas.
pub fn snapshot_gallery(path: &Path, dark: bool) -> bool {
    if MainThreadMarker::new().is_none() {
        return false;
    }
    GALLERY.with(|slot| {
        let slot = slot.borrow();
        let Some(gallery) = slot.as_ref() else {
            return false;
        };
        // SAFETY: constantes NSString d'AppKit, valides toute la vie du processus.
        let name = unsafe {
            if dark {
                NSAppearanceNameDarkAqua
            } else {
                NSAppearanceNameAqua
            }
        };
        gallery
            .window
            .setAppearance(NSAppearance::appearanceNamed(name).as_deref());
        let view: &NSView = &gallery.root;
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
        gallery.window.setAppearance(None);
        saved
    })
}
