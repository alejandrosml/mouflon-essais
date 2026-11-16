//! La boucle d'événements du fil courant, pour le processus auxiliaire du
//! relevé : il y attend son prochain relevé, et s'y réveille quand
//! l'Accessibilité signale une fenêtre qui bouge ou quand l'app demande
//! quelque chose.

use std::time::Duration;

use objc2_core_foundation::{
    CFRetained, CFRunLoop, CFRunLoopSource, CFRunLoopSourceContext, kCFRunLoopDefaultMode,
};

/// Ce que fait la source quand on la signale : rien de plus que réveiller
/// la boucle (qui revient alors de [`run_loop_for`]).
unsafe extern "C-unwind" fn nothing(_info: *mut std::ffi::c_void) {}

/// Fait tourner la boucle du fil courant pendant `timeout` au plus. Elle
/// revient plus tôt après un signal traité : une fenêtre qui bouge (voir
/// [`super::follow_window_moves`]), ou un [`LoopWaker`] de ce fil.
pub fn run_loop_for(timeout: Duration) {
    if CFRunLoop::current().is_none() {
        std::thread::sleep(timeout);
        return;
    }
    // SAFETY: constante de Core Foundation, valide toute la vie du processus.
    let mode = unsafe { kCFRunLoopDefaultMode };
    CFRunLoop::run_in_mode(mode, timeout.as_secs_f64().max(0.0), true);
}

/// De quoi réveiller, depuis un autre fil, la boucle d'un fil qui attend
/// dans [`run_loop_for`] : une source de la boucle qu'on signale. Un signal
/// reste en attente tant que la boucle ne l'a pas traité, même si elle ne
/// tournait pas à cet instant : aucun réveil ne se perd.
#[derive(Debug)]
pub struct LoopWaker {
    run_loop: CFRetained<CFRunLoop>,
    source: CFRetained<CFRunLoopSource>,
}

// SAFETY: `CFRunLoopSourceSignal` et `CFRunLoopWakeUp` peuvent être appelés
// depuis n'importe quel fil (Core Foundation : « Thread Safety and Run Loop
// Objects ») ; le `LoopWaker` ne fait rien d'autre depuis un autre fil.
unsafe impl Send for LoopWaker {}
// SAFETY: voir ci-dessus ; aucun état n'est modifié de notre côté.
unsafe impl Sync for LoopWaker {}

impl LoopWaker {
    /// Pour la boucle du fil courant, qui garde sa source jusqu'à la fin du
    /// processus (elle l'empêche aussi de revenir tout de suite faute de
    /// rien à attendre).
    #[must_use]
    pub fn current() -> Option<Self> {
        let run_loop = CFRunLoop::current()?;
        let mut context = CFRunLoopSourceContext {
            version: 0,
            info: std::ptr::null_mut(),
            retain: None,
            release: None,
            copyDescription: None,
            equal: None,
            hash: None,
            schedule: None,
            cancel: None,
            perform: Some(nothing),
        };
        // SAFETY: allocateur par défaut ; le contexte (version 0) est une
        // variable locale que Core Foundation copie ; `nothing` a la
        // signature attendue et n'utilise pas `info` (nul).
        let source = unsafe { CFRunLoopSource::new(None, 0, &mut context) }?;
        // SAFETY: constante de Core Foundation, valide toute la vie du processus.
        let mode = unsafe { kCFRunLoopDefaultMode };
        run_loop.add_source(Some(&source), mode);
        Some(Self { run_loop, source })
    }

    /// Fait revenir [`run_loop_for`] tout de suite (ou dès son prochain appel).
    pub fn wake(&self) {
        self.source.signal();
        self.run_loop.wake_up();
    }
}
