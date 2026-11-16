//! Les fenêtres de Mouflon passent devant ses animaux.
//!
//! Les animaux vivent dans des fenêtres flottantes, au-dessus de toutes les
//! apps ; les fenêtres de Mouflon (réglages, galerie, « À propos ») sont des
//! fenêtres ordinaires. Sans plus, un écran plein d'animaux les couvrirait
//! et prendrait tous les clics : on ne pourrait plus toucher aux réglages.
//! Tant que Mouflon est au premier plan, elles passent donc juste devant les
//! animaux ; quand on passe à une autre app, elles reprennent leur niveau
//! ordinaire, derrière les fenêtres des autres apps.
//!
//! Rien n'y demande la liste des fenêtres du système : ce relevé arrête
//! toutes les transactions Core Animation du processus le temps de se faire
//! (voir `docs/architecture.md`), et les animaux s'arrêteraient avec elles.

use std::cell::Cell;

use objc2::rc::Retained;
use objc2::runtime::AnyObject;
use objc2::{MainThreadMarker, sel};
use objc2_app_kit::{
    NSApplication, NSApplicationDidBecomeActiveNotification,
    NSApplicationDidResignActiveNotification, NSFloatingWindowLevel, NSNormalWindowLevel, NSWindow,
    NSWindowLevel,
};
use objc2_foundation::NSNotificationCenter;

use super::actions::actions;

/// Le niveau des fenêtres de Mouflon quand elles passent devant les
/// animaux : juste au-dessus du leur (celui des fenêtres flottantes).
const FRONT: NSWindowLevel = NSFloatingWindowLevel + 1;

thread_local! {
    /// Mouflon suit-il déjà son passage au premier plan et son départ ?
    static FOLLOWING: Cell<bool> = const { Cell::new(false) };
}

/// La fenêtre `shown` de Mouflon vient d'être montrée : ses fenêtres
/// passent devant les animaux (elle devant les autres), et y resteront tant
/// qu'il est au premier plan. Sans fenêtre donnée, celle qui a le clavier
/// passe devant.
pub(super) fn bring_forward(mtm: MainThreadMarker, shown: Option<&NSWindow>) {
    follow_activation(mtm);
    raise(mtm, shown);
}

/// Mouflon passe au premier plan : ses fenêtres ordinaires passent devant
/// les animaux.
pub(super) fn app_became_active(mtm: MainThreadMarker) {
    raise(mtm, None);
}

/// Mouflon quitte le premier plan : ses fenêtres reprennent leur niveau,
/// derrière celles des autres apps. Changer de niveau les mettrait devant
/// les fenêtres ordinaires, la fenêtre qu'on vient de choisir comprise ;
/// les envoyer derrière ne demande à savoir où sont les autres.
pub(super) fn app_resigned_active(mtm: MainThreadMarker) {
    let windows = NSApplication::sharedApplication(mtm).windows();
    for window in windows.iter().filter(|w| w.level() == FRONT) {
        window.setLevel(NSNormalWindowLevel);
        if window.isVisible() {
            window.orderBack(None);
        }
    }
}

/// Les fenêtres ordinaires de Mouflon passent devant les animaux ; `top`
/// (ou celle qui a le clavier) en dernier, pour être devant les autres.
fn raise(mtm: MainThreadMarker, top: Option<&NSWindow>) {
    let app = NSApplication::sharedApplication(mtm);
    let key: Option<Retained<NSWindow>> = app.keyWindow();
    let top: Option<&NSWindow> = top.or(key.as_deref());
    let is_top = |w: &NSWindow| top.is_some_and(|t| std::ptr::eq(t, w));
    let windows = app.windows();
    let mut last = None;
    for window in windows.iter() {
        if window.level() != NSNormalWindowLevel {
            continue;
        }
        if is_top(&window) {
            last = Some(window);
        } else {
            window.setLevel(FRONT);
        }
    }
    if let Some(window) = last {
        window.setLevel(FRONT);
    }
}

/// Suit, une fois pour toutes, le passage de Mouflon au premier plan et son
/// départ (voir les méthodes `appDidBecomeActive:` et `appDidResignActive:`
/// de la cible commune).
fn follow_activation(mtm: MainThreadMarker) {
    if FOLLOWING.replace(true) {
        return;
    }
    let target = actions(mtm);
    let observer: &AnyObject = target.as_ref();
    let center = NSNotificationCenter::defaultCenter();
    // SAFETY: les deux sélecteurs sont des méthodes de la cible qui prennent
    // une NSNotification ; la cible vit jusqu'à la fin du processus (voir
    // `actions`) ; les noms sont des constantes d'AppKit, valides toute la vie
    // du processus ; pas de filtre sur l'objet.
    unsafe {
        center.addObserver_selector_name_object(
            observer,
            sel!(appDidBecomeActive:),
            Some(NSApplicationDidBecomeActiveNotification),
            None,
        );
        center.addObserver_selector_name_object(
            observer,
            sel!(appDidResignActive:),
            Some(NSApplicationDidResignActiveNotification),
            None,
        );
    }
}
