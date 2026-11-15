//! Menu surgissant : le menu d'un animal, au clic droit.

use std::cell::RefCell;

use objc2::rc::Retained;
use objc2::{MainThreadMarker, MainThreadOnly, available, sel};
use objc2_app_kit::{NSControlStateValueOn, NSEvent, NSMenu, NSMenuItem};
use objc2_foundation::{
    NSArray, NSObjectNSDelayedPerforming, NSPoint, NSRunLoopCommonModes, NSString, ns_string,
};

use super::actions::{actions, emit};
use crate::{PopupItem, UiEvent};

thread_local! {
    /// Le menu à ouvrir au prochain tour de la boucle, et où.
    static PENDING: RefCell<Option<(Retained<NSMenu>, NSPoint)>> = const { RefCell::new(None) };
}

/// Ouvre un menu sous le curseur. L'article choisi arrive ensuite à l'app
/// comme [`UiEvent::Menu`], avec son identifiant, puis
/// [`UiEvent::MenuClosed`] quand le menu se referme.
///
/// Le menu s'ouvre au tour suivant de la boucle d'événements, une fois rendu
/// le gestionnaire de l'app qui le demande : un menu ouvert garde la main
/// tant qu'il est affiché, et ouvert depuis ce gestionnaire, il retenait
/// tous les autres événements (les battements de l'horloge de l'écran
/// compris) — tous les animaux s'arrêtaient. Ainsi, ils continuent de vivre
/// pendant qu'on choisit.
pub fn pop_up_menu(items: &[PopupItem]) {
    let Some(mtm) = MainThreadMarker::new() else {
        return;
    };
    let menu = build(items, mtm);
    // Sans vue, la position est dans le repère de l'écran, celui du curseur
    // au moment du clic.
    PENDING.with(|pending| *pending.borrow_mut() = Some((menu, NSEvent::mouseLocation())));
    let target = actions(mtm);
    // SAFETY: constante de Foundation, valide pendant toute la vie du processus.
    let modes = NSArray::from_slice(&[unsafe { NSRunLoopCommonModes }]);
    // SAFETY: `popUp:` est une méthode d'`Actions` qui prend un objet (ici
    // aucun) ; la cible vit jusqu'à la fin du processus.
    unsafe {
        target.performSelector_withObject_afterDelay_inModes(sel!(popUp:), None, 0.0, &modes);
    }
}

/// Ouvre le menu en attente (appelé par la boucle d'événements, hors de
/// tout gestionnaire de l'app) et attend qu'il se referme.
pub(super) fn show_pending() {
    let Some((menu, at)) = PENDING.with(|pending| pending.borrow_mut().take()) else {
        return;
    };
    menu.popUpMenuPositioningItem_atLocation_inView(None, at, None);
    emit(UiEvent::MenuClosed);
}

fn build(items: &[PopupItem], mtm: MainThreadMarker) -> Retained<NSMenu> {
    let menu = NSMenu::new(mtm);
    // Chaque article dit lui-même s'il est disponible.
    menu.setAutoenablesItems(false);
    let target = actions(mtm);
    for item in items {
        match item {
            PopupItem::Header(title) => {
                let title = NSString::from_str(title);
                let header = if available!(macos = 14.0) {
                    NSMenuItem::sectionHeaderWithTitle(&title, mtm)
                } else {
                    let header = NSMenuItem::new(mtm);
                    header.setTitle(&title);
                    header.setEnabled(false);
                    header
                };
                menu.addItem(&header);
            }
            PopupItem::Item {
                id,
                label,
                checked,
                enabled,
            } => {
                // SAFETY: `choose:` est une action d'`Actions`, la cible
                // donnée ci-dessous, qui vit jusqu'à la fin du processus.
                let entry = unsafe {
                    NSMenuItem::initWithTitle_action_keyEquivalent(
                        NSMenuItem::alloc(mtm),
                        &NSString::from_str(label),
                        Some(sel!(choose:)),
                        ns_string!(""),
                    )
                };
                // SAFETY: la cible est un `Actions`, qui répond à `choose:`.
                unsafe { entry.setTarget(Some(&target)) };
                entry.setTag(*id as isize);
                entry.setEnabled(*enabled);
                if *checked {
                    entry.setState(NSControlStateValueOn);
                }
                menu.addItem(&entry);
            }
            PopupItem::Submenu { label, items } => {
                let entry = NSMenuItem::new(mtm);
                entry.setTitle(&NSString::from_str(label));
                entry.setSubmenu(Some(&build(items, mtm)));
                menu.addItem(&entry);
            }
            PopupItem::Separator => menu.addItem(&NSMenuItem::separatorItem(mtm)),
        }
    }
    menu
}
