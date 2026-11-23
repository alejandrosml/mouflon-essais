//! Ouvrir Mouflon à l'ouverture de session (`SMAppService`, macOS 13).
//!
//! Le système garde ce choix (Réglages Système → Général → Ouverture) : on
//! le lui demande à chaque fois, sans copie de notre côté qui pourrait le
//! contredire.

use objc2::available;
use objc2_foundation::NSBundle;
use objc2_service_management::{SMAppService, SMAppServiceStatus};

use crate::LoginItem;

/// Mouflon tourne-t-il depuis son app (`Mouflon.app`) ? Lancé seul (pendant
/// le développement), il n'a rien à inscrire.
fn in_app_bundle() -> bool {
    NSBundle::mainBundle().bundleIdentifier().is_some()
}

fn service() -> Option<objc2::rc::Retained<SMAppService>> {
    if !available!(macos = 13.0) || !in_app_bundle() {
        return None;
    }
    // SAFETY: méthode de classe sans argument, présente depuis macOS 13
    // (vérifié ci-dessus) ; elle renvoie l'objet qui représente l'app.
    Some(unsafe { SMAppService::mainAppService() })
}

fn status_of(service: &SMAppService) -> LoginItem {
    // SAFETY: lecture d'une propriété, sans argument.
    match unsafe { service.status() } {
        SMAppServiceStatus::Enabled => LoginItem::On,
        SMAppServiceStatus::RequiresApproval => LoginItem::NeedsApproval,
        _ => LoginItem::Off,
    }
}

/// L'ouverture à l'ouverture de session est-elle réglable d'ici (macOS 13
/// et plus, Mouflon lancé depuis son app) ? Ne demande rien au système : à
/// appeler n'importe quand.
#[must_use]
pub fn login_item_available() -> bool {
    available!(macos = 13.0) && in_app_bundle()
}

/// Où en est l'ouverture à l'ouverture de session. Le système met parfois
/// une cinquantaine de millisecondes à répondre : à demander hors du fil
/// de l'affichage.
#[must_use]
pub fn login_item() -> LoginItem {
    service().map_or(LoginItem::Unavailable, |service| status_of(&service))
}

/// Demande (ou retire) l'ouverture à l'ouverture de session (hors du fil
/// de l'affichage, comme [`login_item`]), et renvoie
/// ce qu'en dit ensuite le système : c'est ce qui compte, même quand la
/// demande a renvoyé une erreur (déjà inscrit, accord en attente…).
pub fn set_login_item(on: bool) -> LoginItem {
    let Some(service) = service() else {
        return LoginItem::Unavailable;
    };
    // SAFETY: méthodes sans argument de l'objet de l'app (macOS 13) ; une
    // erreur arrive comme une NSError, sans exception.
    let result = unsafe {
        if on {
            service.registerAndReturnError()
        } else {
            service.unregisterAndReturnError()
        }
    };
    let status = status_of(&service);
    if let Err(error) = result {
        log::warn!(
            "ouverture à l'ouverture de session ({}) : {}, état {status:?}",
            if on { "demandée" } else { "retirée" },
            error.localizedDescription()
        );
    } else {
        log::info!("ouverture à l'ouverture de session : {status:?}");
    }
    status
}

/// Ouvre Réglages Système à la liste des apps ouvertes à l'ouverture de
/// session, où l'utilisateur donne son accord.
pub fn open_login_items_settings() {
    if available!(macos = 13.0) {
        // SAFETY: méthode de classe sans argument, présente depuis macOS 13.
        unsafe { SMAppService::openSystemSettingsLoginItems() };
    }
}
