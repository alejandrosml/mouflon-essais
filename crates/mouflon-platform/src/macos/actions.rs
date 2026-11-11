//! Ce que les menus et la fenêtre des réglages de Mouflon envoient à l'app.
//!
//! AppKit prévient d'un clic par « cible et action » : un objet
//! Objective-C, la cible, reçoit un message (l'action) avec le contrôle
//! cliqué. Une seule cible sert à tout : elle lit l'identifiant du contrôle
//! (son `tag`) et sa valeur, et les passe à l'app sous forme d'[`UiEvent`].

use std::cell::{OnceCell, RefCell};

use objc2::rc::Retained;
use objc2::runtime::{AnyObject, NSObject, NSObjectProtocol};
use objc2::{MainThreadMarker, MainThreadOnly, define_class, msg_send};
use objc2_app_kit::{
    NSButton, NSControlStateValueOn, NSMenuItem, NSPopUpButton, NSSlider, NSWindow,
    NSWindowDelegate,
};
use objc2_foundation::NSNotification;

use crate::UiEvent;

/// Ce que l'app fait d'un événement.
type Handler = Box<dyn Fn(UiEvent)>;

thread_local! {
    /// Où vont les événements : réglé par l'app (`set_ui_handler`).
    static HANDLER: RefCell<Option<Handler>> = const { RefCell::new(None) };
    /// La cible de tous les contrôles et menus de Mouflon. AppKit ne la
    /// retient pas (référence faible) : elle vit ici jusqu'à la fin.
    static ACTIONS: OnceCell<Retained<Actions>> = const { OnceCell::new() };
}

/// Ce que l'app fait des événements de ses menus et de ses réglages. Le
/// gestionnaire est appelé sur le fil principal, pendant qu'AppKit traite
/// le clic : il doit seulement transmettre (à la boucle de l'app).
pub fn set_ui_handler(handler: impl Fn(UiEvent) + 'static) {
    HANDLER.with(|slot| *slot.borrow_mut() = Some(Box::new(handler)));
}

pub(super) fn emit(event: UiEvent) {
    HANDLER.with(|slot| {
        if let Some(handler) = slot.borrow().as_ref() {
            handler(event);
        }
    });
}

/// Identifiant donné par l'app (le `tag` d'AppKit, jamais négatif ici).
fn id_of(tag: isize) -> u32 {
    u32::try_from(tag).unwrap_or(u32::MAX)
}

define_class!(
    // SAFETY:
    // - NSObject n'impose rien à ses sous-classes.
    // - `Actions` n'implémente pas `Drop`.
    #[unsafe(super(NSObject))]
    #[thread_kind = MainThreadOnly]
    #[name = "MouflonActions"]
    pub(super) struct Actions;

    // SAFETY: NSObjectProtocol n'a pas d'exigence de plus.
    unsafe impl NSObjectProtocol for Actions {}

    // SAFETY: toutes les méthodes de NSWindowDelegate sont facultatives ;
    // celle-ci a la signature du protocole.
    unsafe impl NSWindowDelegate for Actions {
        // SAFETY: `windowDidBecomeKey:` reçoit une NSNotification.
        #[unsafe(method(windowDidBecomeKey:))]
        fn window_did_become_key(&self, _notification: &NSNotification) {
            emit(UiEvent::FormShown);
        }

        // SAFETY: `windowWillClose:` reçoit une NSNotification, dont l'objet
        // est la fenêtre qui se ferme.
        #[unsafe(method(windowWillClose:))]
        fn window_will_close(&self, notification: &NSNotification) {
            if let Some(window) = notification
                .object()
                .and_then(|object| object.downcast::<NSWindow>().ok())
            {
                super::gallery::closed(&window);
            }
        }
    }

    impl Actions {
        // SAFETY: action d'un article de menu : l'expéditeur est cet article
        // (seuls des NSMenuItem ont cette action).
        #[unsafe(method(choose:))]
        fn choose(&self, sender: &NSMenuItem) {
            emit(UiEvent::Menu(id_of(sender.tag())));
        }

        // SAFETY: action d'une case à cocher (seuls des NSButton l'ont).
        #[unsafe(method(check:))]
        fn check(&self, sender: &NSButton) {
            let on = sender.state() == NSControlStateValueOn;
            emit(UiEvent::Check(id_of(sender.tag()), on));
        }

        // SAFETY: action d'un curseur (seuls des NSSlider l'ont).
        #[unsafe(method(step:))]
        fn step(&self, sender: &NSSlider) {
            let step = usize::try_from(sender.integerValue()).unwrap_or(0);
            emit(UiEvent::Step(id_of(sender.tag()), step));
        }

        // SAFETY: action d'une liste de choix (seuls des NSPopUpButton l'ont).
        #[unsafe(method(pick:))]
        fn pick(&self, sender: &NSPopUpButton) {
            let index = usize::try_from(sender.indexOfSelectedItem()).unwrap_or(0);
            emit(UiEvent::Step(id_of(sender.tag()), index));
        }

        // SAFETY: action d'un bouton (seuls des NSButton l'ont).
        #[unsafe(method(press:))]
        fn press(&self, sender: &NSButton) {
            emit(UiEvent::Button(id_of(sender.tag())));
        }

        // SAFETY: action d'un bouton « Ajouter » de la galerie (des NSButton).
        #[unsafe(method(gallery:))]
        fn gallery(&self, sender: &NSButton) {
            let id = id_of(sender.tag());
            emit(UiEvent::GalleryAdd(id, super::gallery::choice(id)));
        }

        // SAFETY: appelée par la minuterie de la galerie, avec elle-même
        // (qu'on ne lit pas).
        #[unsafe(method(galleryTick:))]
        fn gallery_tick(&self, _timer: Option<&AnyObject>) {
            super::gallery::tick();
        }

        // SAFETY: action de l'article ⌘Q ; l'expéditeur n'est pas lu.
        #[unsafe(method(quit:))]
        fn quit(&self, _sender: Option<&AnyObject>) {
            emit(UiEvent::Quit);
        }

        // SAFETY: appelée par la boucle d'événements (`performSelector:…`),
        // sans objet.
        #[unsafe(method(popUp:))]
        fn pop_up(&self, _sender: Option<&AnyObject>) {
            super::popup::show_pending();
        }

        // SAFETY: appelée par le centre de notifications, avec une
        // NSNotification : Mouflon passe au premier plan.
        #[unsafe(method(appDidBecomeActive:))]
        fn app_did_become_active(&self, _notification: &NSNotification) {
            super::front::app_became_active(self.mtm());
        }

        // SAFETY: comme ci-dessus : Mouflon quitte le premier plan.
        #[unsafe(method(appDidResignActive:))]
        fn app_did_resign_active(&self, _notification: &NSNotification) {
            super::front::app_resigned_active(self.mtm());
        }

        // SAFETY: comme ci-dessus (centre distribué) : la disposition du
        // clavier a changé.
        #[unsafe(method(keyboardChanged:))]
        fn keyboard_changed(&self, _notification: &NSNotification) {
            super::keys::layout_changed();
        }
    }
);

impl Actions {
    fn new(mtm: MainThreadMarker) -> Retained<Self> {
        let this = Self::alloc(mtm).set_ivars(());
        // SAFETY: `init` est l'initialiseur de NSObject.
        unsafe { msg_send![super(this), init] }
    }
}

/// La cible commune des contrôles et des menus.
pub(super) fn actions(mtm: MainThreadMarker) -> Retained<Actions> {
    ACTIONS.with(|cell| cell.get_or_init(|| Actions::new(mtm)).clone())
}
