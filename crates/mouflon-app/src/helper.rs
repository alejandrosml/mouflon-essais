//! Le processus auxiliaire du relevé : le même programme, lancé avec
//! [`crate::survey::HELPER_ARG`], qui pose au système les questions qui le
//! font attendre — fenêtres des autres apps, cadre du Dock, app au premier
//! plan, autorisation d'Accessibilité, activité de l'utilisateur — et
//! envoie ses relevés à l'app par un tube (voir [`crate::survey`]).
//!
//! Pourquoi un autre processus : le relevé des fenêtres
//! (`CGWindowListCopyWindowInfo`) commence par synchroniser les transactions
//! Core Animation de *tout le processus* avec WindowServer, en gardant le
//! verrou de la connexion. Dans l'app, il bloquait l'affichage des animaux,
//! jusqu'à une demi-seconde. Ici, il ne touche ni à sa connexion ni à ses
//! fils, et une app qui ne répond pas à l'Accessibilité ne fige plus rien.

use std::cell::Cell;
use std::io::{BufRead, Write};
use std::rc::Rc;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::{Duration, Instant};

use mouflon_platform as platform;

use crate::survey::{Command, Survey};

/// Relevé des fenêtres au calme, et quand l'app le demande souvent (une
/// fenêtre qui porte un animal bouge, on en tient un).
pub const CALM: Duration = Duration::from_millis(250);
pub const FAST: Duration = Duration::from_millis(33);
/// L'autorisation d'Accessibilité et le Dock changent rarement.
const SLOW: Duration = Duration::from_secs(2);

/// Ce que l'app a demandé (lu par le relevé, écrit par le fil des demandes).
#[derive(Debug, Default)]
pub struct Requests {
    pub fast: AtomicBool,
    pub dock: AtomicBool,
    /// Tout relire au prochain relevé (Dock et autorisation compris).
    pub now: AtomicBool,
}

impl Requests {
    /// Applique une demande de l'app.
    pub fn apply(&self, command: Command) {
        match command {
            Command::Fast(on) => self.fast.store(on, Ordering::SeqCst),
            Command::Dock(on) => {
                self.dock.store(on, Ordering::SeqCst);
                self.now.store(true, Ordering::SeqCst);
            }
            Command::Now => self.now.store(true, Ordering::SeqCst),
        }
    }
}

/// Celui qui relève, d'un relevé à l'autre.
#[derive(Debug, Default)]
pub struct Surveyor {
    seq: u64,
    /// L'app pour qui l'on relève, quand ce n'est pas ce processus.
    app: Option<i32>,
    /// Suivre les fenêtres qui bougent par l'Accessibilité (dans le
    /// processus auxiliaire seulement : les signaux arrivent sur sa boucle).
    follow: bool,
    trusted: bool,
    dock: Option<platform::OsRect>,
    slow_read: Option<Instant>,
    /// Apps suivies, et si l'autorisation était là.
    followed_for: Option<(bool, Vec<i32>)>,
    followed: u32,
}

impl Surveyor {
    /// `app` : le processus de l'app quand on relève pour elle d'un autre
    /// processus (le sien est toujours écarté).
    #[must_use]
    pub fn new(follow: bool, app: Option<i32>) -> Self {
        Self {
            follow,
            app,
            ..Self::default()
        }
    }

    /// Un relevé. `nudged` : demandé par le signal d'une fenêtre qui bouge.
    pub fn poll(&mut self, requests: &Requests, nudged: bool) -> Survey {
        let started = Instant::now();
        let mut windows = platform::windows();
        let windows_took = started.elapsed();
        // Les fenêtres de l'app elle-même (celle qui a lancé ce processus)
        // ne sont pas des rebords.
        if let Some(app) = self.app {
            windows.retain(|w| w.pid != app);
        }
        let frontmost = platform::frontmost_pid();
        let activity = platform::user_activity();
        let with_dock = requests.dock.load(Ordering::SeqCst);
        let now = requests.now.swap(false, Ordering::SeqCst);
        let mut dock_took = Duration::ZERO;
        if now || self.slow_read.is_none_or(|t| t.elapsed() >= SLOW) {
            self.trusted = platform::accessibility_trusted();
            let asked = Instant::now();
            self.dock = if with_dock {
                platform::dock_frame()
            } else {
                None
            };
            dock_took = asked.elapsed();
            self.slow_read = Some(Instant::now());
        } else if !with_dock {
            self.dock = None;
        }
        if self.follow {
            self.follow_moves(&windows);
        }
        self.seq += 1;
        Survey {
            seq: self.seq,
            windows,
            dock: self.dock,
            frontmost,
            trusted: self.trusted,
            activity,
            followed: self.followed,
            windows_took,
            dock_took,
            nudged,
        }
    }

    /// Les apps dont les fenêtres servent de rebords préviennent quand l'une
    /// d'elles bouge (avec l'autorisation d'Accessibilité seulement).
    fn follow_moves(&mut self, windows: &[platform::OsWindow]) {
        let mut pids: Vec<i32> = windows
            .iter()
            .filter(|w| w.is_walkable())
            .map(|w| w.pid)
            .collect();
        pids.sort_unstable();
        pids.dedup();
        let wanted = (self.trusted, pids);
        if self.followed_for.as_ref() == Some(&wanted) {
            return;
        }
        self.followed = platform::follow_window_moves(&wanted.1) as u32;
        self.followed_for = Some(wanted);
    }
}

/// Quand faire le prochain relevé : à l'intervalle voulu après le dernier,
/// plus tôt sur un signal ou une demande, mais jamais plus souvent qu'en
/// relevé fréquent (l'Accessibilité peut signaler à chaque image une fenêtre
/// qu'on tire).
#[must_use]
pub fn next_poll(last: Option<Instant>, fast: bool, urgent: bool) -> Option<Instant> {
    let last = last?;
    let interval = if fast || urgent { FAST } else { CALM };
    Some(last + interval)
}

/// Le processus auxiliaire : relève jusqu'à ce que l'app ferme le tube.
pub fn run() {
    let Some(waker) = platform::LoopWaker::current() else {
        log::error!("relevé : pas de boucle d'événements");
        return;
    };
    let waker = Arc::new(waker);
    let requests = Arc::new(Requests::default());
    {
        let requests = Arc::clone(&requests);
        let waker = Arc::clone(&waker);
        let spawned = std::thread::Builder::new()
            .name("mouflon-releve-demandes".into())
            .spawn(move || {
                for line in std::io::stdin().lock().lines() {
                    let Ok(line) = line else {
                        break;
                    };
                    match Command::parse(&line) {
                        Some(command) => requests.apply(command),
                        None => log::warn!("relevé : demande inconnue {line:?}"),
                    }
                    waker.wake();
                }
                // L'app est partie (ou a fermé le tube) : le relevé aussi.
                std::process::exit(0);
            });
        if let Err(e) = spawned {
            log::error!("relevé : impossible de lire les demandes : {e}");
            return;
        }
    }
    // Une fenêtre suivie bouge : relevée aussitôt (voir `follow_window_moves`).
    let nudged = Rc::new(Cell::new(false));
    {
        let nudged = Rc::clone(&nudged);
        platform::set_window_moves_handler(move || nudged.set(true));
    }
    // L'app est le processus qui nous a lancés.
    #[cfg(unix)]
    let app = Some(std::os::unix::process::parent_id() as i32);
    #[cfg(not(unix))]
    let app = None;
    let mut surveyor = Surveyor::new(true, app);
    let mut out = std::io::stdout().lock();
    let mut text = String::new();
    let mut last: Option<Instant> = None;
    log::info!(
        "relevé : processus auxiliaire n° {} en route",
        std::process::id()
    );
    loop {
        let urgent = nudged.get() || requests.now.load(Ordering::SeqCst);
        let fast = requests.fast.load(Ordering::SeqCst);
        let now = Instant::now();
        match next_poll(last, fast, urgent) {
            Some(due) if due > now => {
                platform::run_loop_for(due - now);
                continue;
            }
            _ => {}
        }
        let was_nudged = nudged.replace(false);
        last = Some(now);
        let survey = surveyor.poll(&requests, was_nudged);
        text.clear();
        survey.encode(&mut text);
        if out
            .write_all(text.as_bytes())
            .and_then(|()| out.flush())
            .is_err()
        {
            // Plus personne pour lire.
            std::process::exit(0);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn polls_calmly_unless_asked_and_never_more_often_than_fast() {
        let t = Instant::now();
        assert_eq!(
            next_poll(None, false, false),
            None,
            "premier relevé tout de suite"
        );
        assert_eq!(next_poll(Some(t), false, false), Some(t + CALM));
        assert_eq!(next_poll(Some(t), true, false), Some(t + FAST));
        // Un signal ou une demande avance le relevé, sans le rendre plus
        // fréquent que le relevé rapide.
        assert_eq!(next_poll(Some(t), false, true), Some(t + FAST));
    }

    #[test]
    fn a_dock_request_rereads_everything() {
        let requests = Requests::default();
        requests.apply(Command::Dock(true));
        assert!(requests.dock.load(Ordering::SeqCst));
        assert!(requests.now.load(Ordering::SeqCst));
        requests.apply(Command::Fast(true));
        assert!(requests.fast.load(Ordering::SeqCst));
    }
}
