//! Horloge de l'écran (CADisplayLink, macOS 14) : les images des animaux
//! suivent exactement le rafraîchissement de l'écran (120 Hz avec ProMotion).
//! Une minuterie, elle, dérive peu à peu par rapport à l'écran : de temps en
//! temps, une image reste affichée deux fois et la suivante saute, et le
//! mouvement fait un petit à-coup.

use std::cell::Cell;
use std::sync::Arc;

use objc2::rc::Retained;
use objc2::runtime::{AnyObject, NSObject};
use objc2::{
    AllocAnyThread, DefinedClass, MainThreadMarker, MainThreadOnly, available, define_class,
    msg_send, sel,
};
use objc2_app_kit::NSScreen;
use objc2_foundation::{NSObjectProtocol, NSRunLoop, NSRunLoopCommonModes};
use objc2_quartz_core::{CACurrentMediaTime, CADisplayLink, CAFrameRateRange};

use super::runloop::LoopWaker;
use crate::{Beat, Beats};

/// Ce que l'horloge appelle à chaque battement : l'heure à laquelle l'image
/// préparée sera affichée, celle du rafraîchissement qui vient d'avoir
/// lieu, et celle de l'appel (le retard de l'appel dit si le fil principal
/// était pris).
struct Ivars {
    tick: Box<dyn Fn(Beat)>,
}

define_class!(
    // SAFETY: NSObject n'impose rien à ses sous-classes, et `FrameTarget`
    // n'implémente pas `Drop` (ses variables sont libérées par objc2).
    #[unsafe(super(NSObject))]
    #[thread_kind = MainThreadOnly]
    #[name = "MouflonFrameTarget"]
    #[ivars = Ivars]
    struct FrameTarget;

    impl FrameTarget {
        #[unsafe(method(tick:))]
        fn tick(&self, link: &CADisplayLink) {
            (self.ivars().tick)(Beat {
                target: link.targetTimestamp(),
                vsync: link.timestamp(),
                called: CACurrentMediaTime(),
            });
        }
    }

    unsafe impl NSObjectProtocol for FrameTarget {}
);

impl FrameTarget {
    fn new(mtm: MainThreadMarker, tick: Box<dyn Fn(Beat)>) -> Retained<Self> {
        let this = Self::alloc(mtm).set_ivars(Ivars { tick });
        // SAFETY: `init` de NSObject, sur un objet alloué dont les variables
        // d'instance sont prêtes.
        unsafe { msg_send![super(this), init] }
    }
}

/// Horloge calée sur l'écran le plus rapide. Elle appelle sa fonction sur le
/// fil principal, à chaque rafraîchissement, tant qu'elle tourne.
pub struct FrameClock {
    link: Retained<CADisplayLink>,
    _target: Retained<FrameTarget>,
    /// Images par seconde que l'écran sait afficher, au plus.
    max_fps: f32,
    /// Cadence demandée (images par seconde).
    rate: Cell<f32>,
}

impl std::fmt::Debug for FrameClock {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("FrameClock")
            .field("max_fps", &self.max_fps)
            .field("rate", &self.rate.get())
            .finish_non_exhaustive()
    }
}

impl FrameClock {
    /// Crée l'horloge, arrêtée. `None` hors du fil principal, sans écran ou
    /// avant macOS 14 (l'app se rythme alors avec une minuterie).
    pub fn new(tick: impl Fn(Beat) + 'static) -> Option<Self> {
        let mtm = MainThreadMarker::new()?;
        if !available!(macos = 14.0) {
            return None;
        }
        // L'écran le plus rapide (celui du MacBook Pro, en général) : un
        // animal posé sur un écran plus lent y est simplement redessiné
        // moins souvent.
        let screen = NSScreen::screens(mtm)
            .iter()
            .max_by_key(|screen| screen.maximumFramesPerSecond())
            .or_else(|| NSScreen::mainScreen(mtm))?;
        let max_fps = screen.maximumFramesPerSecond().clamp(30, 240) as f32;
        let target = FrameTarget::new(mtm, Box::new(tick));
        let object: &AnyObject = target.as_ref();
        // SAFETY: la cible répond à `tick:`, défini ci-dessus avec un argument
        // objet (le CADisplayLink) ; macOS 14 est vérifié plus haut.
        let link = unsafe { screen.displayLinkWithTarget_selector(object, sel!(tick:)) };
        link.setPaused(true);
        // SAFETY: la boucle principale et le mode commun sont des objets de
        // Foundation valides pendant toute la vie du processus ; on est sur
        // le fil principal. Le mode commun garde l'horloge en marche pendant
        // qu'un menu est ouvert.
        unsafe { link.addToRunLoop_forMode(&NSRunLoop::mainRunLoop(), NSRunLoopCommonModes) };
        Some(Self {
            link,
            _target: target,
            max_fps,
            rate: Cell::new(0.0),
        })
    }

    /// Images par seconde que l'écran de l'horloge sait afficher, au plus.
    #[must_use]
    pub fn max_fps(&self) -> f32 {
        self.max_fps
    }

    #[must_use]
    pub fn is_running(&self) -> bool {
        !self.link.isPaused()
    }

    /// Durée d'une image de l'écran au dernier battement, en secondes :
    /// 1/120 s pour ProMotion à plein régime, davantage quand l'écran ralentit.
    #[must_use]
    pub fn frame_duration(&self) -> f64 {
        self.link.targetTimestamp() - self.link.timestamp()
    }

    /// Fait tourner l'horloge, ou l'arrête (plus rien ne bouge à l'écran).
    pub fn set_running(&self, running: bool) {
        if self.link.isPaused() == running {
            self.link.setPaused(!running);
        }
    }

    /// Cadence voulue, en images par seconde : celle de l'écran quand un
    /// animal se déplace, moins quand il se repose (l'écran ProMotion peut
    /// alors ralentir et économiser l'énergie).
    pub fn set_rate(&self, fps: f32) {
        let fps = fps.clamp(10.0, self.max_fps);
        if (self.rate.get() - fps).abs() < 0.5 {
            return;
        }
        self.rate.set(fps);
        self.link.setPreferredFrameRateRange(CAFrameRateRange {
            minimum: (fps * 0.5).max(10.0),
            maximum: fps,
            preferred: fps,
        });
    }
}

impl Drop for FrameClock {
    fn drop(&mut self) {
        // Le CADisplayLink retient sa cible : on le défait pour libérer les deux.
        self.link.invalidate();
    }
}

/// Ce que l'horloge d'un autre fil fait à chaque battement : le noter, et
/// réveiller ce fil.
struct ThreadIvars {
    beats: Arc<Beats>,
    wake: Arc<LoopWaker>,
}

define_class!(
    // SAFETY: NSObject n'impose rien à ses sous-classes, et
    // `ThreadFrameTarget` n'implémente pas `Drop`.
    #[unsafe(super(NSObject))]
    #[name = "MouflonThreadFrameTarget"]
    #[ivars = ThreadIvars]
    struct ThreadFrameTarget;

    impl ThreadFrameTarget {
        #[unsafe(method(tick:))]
        fn tick(&self, link: &CADisplayLink) {
            self.ivars().beats.push(Beat {
                target: link.targetTimestamp(),
                vsync: link.timestamp(),
                called: CACurrentMediaTime(),
            });
            self.ivars().wake.wake();
        }
    }

    unsafe impl NSObjectProtocol for ThreadFrameTarget {}
);

/// Horloge calée sur l'écran le plus rapide, qui bat sur le fil de
/// l'affichage : créée sur le fil principal (seul à pouvoir demander l'écran),
/// elle est ensuite mise en route sur la boucle de ce fil ([`Self::start_here`])
/// et ne sert plus qu'à lui. À chaque battement, elle note le battement dans
/// ses [`Beats`] et réveille le fil (son `LoopWaker`).
pub struct ThreadClock {
    link: Retained<CADisplayLink>,
    _target: Retained<ThreadFrameTarget>,
    max_fps: f32,
    rate: Cell<f32>,
}

// SAFETY: l'horloge est créée sur le fil principal puis donnée au fil de
// l'affichage, qui est le seul à s'en servir ensuite (la mettre en route sur
// sa boucle, régler sa cadence, l'arrêter, la défaire). Un CADisplayLink
// s'utilise depuis le fil de la boucle où il est inscrit ; sa cible ne touche
// qu'à des objets faits pour plusieurs fils (`Beats`, `LoopWaker`).
unsafe impl Send for ThreadClock {}

impl std::fmt::Debug for ThreadClock {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("ThreadClock")
            .field("max_fps", &self.max_fps)
            .field("rate", &self.rate.get())
            .finish_non_exhaustive()
    }
}

impl ThreadClock {
    /// Crée l'horloge, arrêtée et pas encore en route. `None` hors du fil
    /// principal, sans écran ou avant macOS 14.
    pub fn new(beats: Arc<Beats>, wake: Arc<LoopWaker>) -> Option<Self> {
        let mtm = MainThreadMarker::new()?;
        if !available!(macos = 14.0) {
            return None;
        }
        let screen = NSScreen::screens(mtm)
            .iter()
            .max_by_key(|screen| screen.maximumFramesPerSecond())
            .or_else(|| NSScreen::mainScreen(mtm))?;
        let max_fps = screen.maximumFramesPerSecond().clamp(30, 240) as f32;
        let target = ThreadFrameTarget::alloc().set_ivars(ThreadIvars { beats, wake });
        // SAFETY: `init` de NSObject, sur un objet alloué dont les variables
        // d'instance sont prêtes.
        let target: Retained<ThreadFrameTarget> = unsafe { msg_send![super(target), init] };
        let object: &AnyObject = target.as_ref();
        // SAFETY: la cible répond à `tick:` (un argument objet, le
        // CADisplayLink) ; macOS 14 est vérifié plus haut.
        let link = unsafe { screen.displayLinkWithTarget_selector(object, sel!(tick:)) };
        link.setPaused(true);
        Some(Self {
            link,
            _target: target,
            max_fps,
            rate: Cell::new(0.0),
        })
    }

    /// L'inscrit sur la boucle du fil courant (le fil de l'affichage), dans
    /// tous ses modes : il bat ensuite sur ce fil.
    pub fn start_here(&self) {
        // SAFETY: la boucle du fil courant et le mode commun sont valides ;
        // le lien n'est inscrit que sur cette boucle, d'où il sera utilisé.
        unsafe {
            self.link
                .addToRunLoop_forMode(&NSRunLoop::currentRunLoop(), NSRunLoopCommonModes)
        };
    }

    #[must_use]
    pub fn max_fps(&self) -> f32 {
        self.max_fps
    }

    #[must_use]
    pub fn is_running(&self) -> bool {
        !self.link.isPaused()
    }

    /// Durée d'une image de l'écran au dernier battement, en secondes.
    #[must_use]
    pub fn frame_duration(&self) -> f64 {
        self.link.targetTimestamp() - self.link.timestamp()
    }

    pub fn set_running(&self, running: bool) {
        if self.link.isPaused() == running {
            self.link.setPaused(!running);
        }
    }

    /// Cadence voulue (voir [`FrameClock::set_rate`]).
    pub fn set_rate(&self, fps: f32) {
        let fps = fps.clamp(10.0, self.max_fps);
        if (self.rate.get() - fps).abs() < 0.5 {
            return;
        }
        self.rate.set(fps);
        self.link.setPreferredFrameRateRange(CAFrameRateRange {
            minimum: (fps * 0.5).max(10.0),
            maximum: fps,
            preferred: fps,
        });
    }
}

impl Drop for ThreadClock {
    fn drop(&mut self) {
        // Le CADisplayLink retient sa cible : on le défait pour libérer les deux.
        self.link.invalidate();
    }
}
