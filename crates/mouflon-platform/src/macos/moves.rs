//! Fenêtres des autres apps qui bougent, signalées par l'Accessibilité.
//!
//! Avec l'autorisation d'Accessibilité (celle que demande « Marcher sous le
//! Dock »), Mouflon demande à chaque app qui a des fenêtres de le prévenir
//! quand l'une d'elles bouge ou change de taille : il relève alors leurs
//! positions tout de suite, au lieu d'attendre le prochain relevé (jusqu'à
//! 250 ms au calme). Un animal posé sur une fenêtre qu'on commence à tirer
//! part avec elle. Rien n'est lu ni changé dans ces apps : seul compte le
//! moment où une fenêtre bouge.

use std::cell::RefCell;
use std::ffi::c_void;
use std::ptr::NonNull;

use objc2_application_services::{AXError, AXIsProcessTrusted, AXObserver, AXUIElement};
use objc2_core_foundation::{CFRetained, CFRunLoop, CFString, kCFRunLoopCommonModes};

/// Ce que l'Accessibilité signale : une fenêtre bouge, change de taille.
const NOTIFICATIONS: [&str; 2] = ["AXWindowMoved", "AXWindowResized"];

/// Une app suivie : son processus, l'observateur et son élément.
struct Followed {
    pid: i32,
    observer: CFRetained<AXObserver>,
}

#[derive(Default)]
struct Moves {
    handler: Option<Box<dyn Fn()>>,
    followed: Vec<Followed>,
}

thread_local! {
    /// Sur le fil principal, dont la boucle reçoit les signaux.
    static MOVES: RefCell<Moves> = RefCell::new(Moves::default());
}

/// Ce qu'il faut faire quand une fenêtre suivie bouge (appelé sur le fil
/// principal, aussitôt).
pub fn set_window_moves_handler(handler: impl Fn() + 'static) {
    MOVES.with(|moves| moves.borrow_mut().handler = Some(Box::new(handler)));
}

/// Suit les fenêtres de ces apps (identifiants de processus), et plus celles
/// des autres. Sans l'autorisation d'Accessibilité, rien n'est suivi.
/// Renvoie le nombre d'apps suivies. À appeler sur le fil principal.
pub fn follow_window_moves(pids: &[i32]) -> usize {
    let Some(main) = CFRunLoop::main() else {
        return 0;
    };
    // SAFETY: constante de CoreFoundation, valide toute la vie du processus.
    let common = unsafe { kCFRunLoopCommonModes };
    // SAFETY: fonction sans argument d'HIServices, appelable de n'importe où.
    let trusted = unsafe { AXIsProcessTrusted() };
    MOVES.with(|moves| {
        let Ok(mut moves) = moves.try_borrow_mut() else {
            return 0;
        };
        let wanted = |pid: i32| trusted && pids.contains(&pid);
        moves.followed.retain(|f| {
            if wanted(f.pid) {
                return true;
            }
            // SAFETY: l'observateur est vivant (nous le tenons) ; sa source
            // est celle qui a été ajoutée à la boucle principale.
            let source = unsafe { f.observer.run_loop_source() };
            main.remove_source(Some(&source), common);
            false
        });
        if !trusted {
            return 0;
        }
        for &pid in pids {
            if moves.followed.iter().any(|f| f.pid == pid) {
                continue;
            }
            if let Some(observer) = observe(pid) {
                // SAFETY: l'observateur vient d'être créé et reste tenu tant
                // que sa source est dans la boucle.
                let source = unsafe { observer.run_loop_source() };
                main.add_source(Some(&source), common);
                moves.followed.push(Followed { pid, observer });
            }
        }
        moves.followed.len()
    })
}

/// Un observateur des fenêtres de l'app `pid`, ou rien si elle ne se laisse
/// pas observer (pas d'interface, déjà partie…).
fn observe(pid: i32) -> Option<CFRetained<AXObserver>> {
    let mut raw: *mut AXObserver = std::ptr::null_mut();
    // SAFETY: `moved` a la signature d'un AXObserverCallback ; `raw` est un
    // pointeur valide vers une variable locale.
    let error = unsafe { AXObserver::create(pid, Some(moved), NonNull::from(&mut raw)) };
    if error != AXError::Success {
        return None;
    }
    // SAFETY: AXObserverCreate a réussi : `raw` est un observateur dont nous
    // recevons la propriété (règle « Create » de CoreFoundation).
    let observer = unsafe { CFRetained::from_raw(NonNull::new(raw)?) };
    // SAFETY: crée l'élément d'accessibilité d'une app à partir de son pid.
    let app = unsafe { AXUIElement::new_application(pid) };
    let mut any = false;
    for name in NOTIFICATIONS {
        let name = CFString::from_static_str(name);
        // SAFETY: observateur et élément vivants, `refcon` nul (inutilisé).
        let error = unsafe { observer.add_notification(&app, &name, std::ptr::null_mut()) };
        any |= error == AXError::Success;
    }
    any.then_some(observer)
}

/// Le signal d'une fenêtre qui bouge : on prévient l'app, sans rien lire.
unsafe extern "C-unwind" fn moved(
    _observer: NonNull<AXObserver>,
    _element: NonNull<AXUIElement>,
    _notification: NonNull<CFString>,
    _refcon: *mut c_void,
) {
    MOVES.with(|moves| {
        if let Ok(moves) = moves.try_borrow()
            && let Some(handler) = &moves.handler
        {
            handler();
        }
    });
}
