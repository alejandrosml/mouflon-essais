//! Relevé des fenêtres des autres apps, du cadre du Dock et du dossier des
//! animaux, à côté de l'affichage.
//!
//! Les questions au système passent par un processus auxiliaire (voir
//! [`crate::helper`]) : ses relevés arrivent par un tube, lus sur un fil à
//! part, et l'app n'en garde que le dernier. S'il s'arrête, il est relancé.
//! S'il ne peut pas être lancé, le relevé se fait sur un fil de l'app (comme
//! avant, avec le risque d'à-coups que cela comporte), et on le dit au journal.

use std::io::{BufRead, BufReader, Write};
use std::path::PathBuf;
use std::process::{Child, ChildStdin, Stdio};
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::mpsc::Sender;
use std::sync::{Arc, Condvar, Mutex};
use std::time::{Duration, Instant};

use crate::helper::{Requests, Surveyor};
use crate::library::{Stamp, stamp};
use crate::survey::{Command, HELPER_ARG, Reader, Survey};

/// Le dossier des animaux change rarement : relu de temps en temps.
const FOLDER_EVERY: Duration = Duration::from_secs(2);
/// Essais : un relevé plus long est noté au journal ; un résumé toutes les…
const SLOW_POLL: Duration = Duration::from_millis(15);
const SUMMARY_EVERY: Duration = Duration::from_secs(10);
/// Un processus auxiliaire qui s'arrête est relancé après ce délai, au plus
/// tant de fois (au-delà, le relevé se fait dans l'app).
const RESTART_AFTER: Duration = Duration::from_secs(1);
const MAX_RESTARTS: u32 = 5;

/// Un relevé : celui du processus auxiliaire.
pub type Snapshot = Survey;

/// Ce que l'app veut du relevé, et ce qu'il lui rapporte.
#[derive(Debug, Default)]
struct Shared {
    latest: Mutex<Option<Survey>>,
    /// Dernier état relevé du dossier des animaux.
    pets: Mutex<Option<Stamp>>,
    /// Les demandes en cours (rejouées à un processus relancé).
    fast: AtomicBool,
    dock: AtomicBool,
    /// Demandes à transmettre (le fil d'écriture les prend), et de quoi
    /// réveiller ceux qui les attendent (ou attendent l'arrêt).
    outbox: Mutex<Vec<Command>>,
    wake: Condvar,
    /// Un relevé est arrivé (avec `latest`).
    arrived: Condvar,
    stop: AtomicBool,
    /// Relevé du dossier le plus long depuis le dernier résumé (µs).
    folder_worst: AtomicU64,
}

impl Shared {
    fn send(&self, command: Command) {
        if let Ok(mut outbox) = self.outbox.lock() {
            outbox.push(command);
            self.wake.notify_all();
        }
    }
}

#[derive(Debug)]
pub struct Watcher {
    shared: Arc<Shared>,
}

impl Watcher {
    /// Lance le relevé ; il s'arrête avec le `Watcher`. `pets` : le dossier
    /// des animaux à surveiller ; `journal` : celui d'un essai (et son
    /// début), où noter la durée des relevés et ce qui arrive au processus
    /// auxiliaire.
    pub fn start(
        dock: bool,
        pets: Option<PathBuf>,
        journal: Option<(Sender<String>, Instant)>,
    ) -> Self {
        let shared = Arc::new(Shared::default());
        shared.dock.store(dock, Ordering::SeqCst);
        {
            let shared = Arc::clone(&shared);
            let spawned = std::thread::Builder::new()
                .name("mouflon-releve".into())
                .spawn(move || survey(&shared, journal));
            if let Err(e) = spawned {
                log::error!("impossible de lancer le relevé des fenêtres : {e}");
            }
        }
        if let Some(dir) = pets {
            let shared = Arc::clone(&shared);
            let spawned = std::thread::Builder::new()
                .name("mouflon-dossier".into())
                .spawn(move || watch_folder(&shared, &dir));
            if let Err(e) = spawned {
                log::warn!("dossier des animaux non surveillé : {e}");
            }
        }
        Self { shared }
    }

    /// Le dernier relevé, s'il est nouveau.
    pub fn take(&self) -> Option<Snapshot> {
        self.shared.latest.lock().ok()?.take()
    }

    /// Attend le premier relevé, au plus `timeout`.
    pub fn wait_first(&self, timeout: Duration) -> Option<Snapshot> {
        let started = Instant::now();
        let mut latest = self.shared.latest.lock().ok()?;
        loop {
            if let Some(snapshot) = latest.take() {
                return Some(snapshot);
            }
            let left = timeout.checked_sub(started.elapsed())?;
            latest = self.shared.arrived.wait_timeout(latest, left).ok()?.0;
        }
    }

    /// Le dernier état relevé du dossier des animaux, s'il est nouveau.
    pub fn take_pets(&self) -> Option<Stamp> {
        self.shared.pets.lock().ok()?.take()
    }

    /// Relève souvent (ou plus).
    pub fn set_fast(&self, fast: bool) {
        if self.shared.fast.swap(fast, Ordering::SeqCst) != fast {
            self.shared.send(Command::Fast(fast));
        }
    }

    pub fn set_dock(&self, dock: bool) {
        self.shared.dock.store(dock, Ordering::SeqCst);
        self.shared.send(Command::Dock(dock));
    }

    /// Relève tout (Dock et autorisation compris) sans attendre le prochain tour.
    pub fn refresh_now(&self) {
        self.shared.send(Command::Now);
    }
}

impl Drop for Watcher {
    fn drop(&mut self) {
        self.shared.stop.store(true, Ordering::SeqCst);
        self.shared.wake.notify_all();
        self.shared.arrived.notify_all();
    }
}

/// Durée des relevés, pour le journal d'un essai.
struct Timing {
    journal: Sender<String>,
    begun: Instant,
    since: Instant,
    polls: u32,
    total: Duration,
    worst: Duration,
    dock_worst: Duration,
    windows: usize,
}

impl Timing {
    fn new((journal, begun): (Sender<String>, Instant)) -> Self {
        Self {
            journal,
            begun,
            since: Instant::now(),
            polls: 0,
            total: Duration::ZERO,
            worst: Duration::ZERO,
            dock_worst: Duration::ZERO,
            windows: 0,
        }
    }

    fn at(&self) -> f64 {
        self.begun.elapsed().as_secs_f64()
    }

    fn note(&self, what: &str) {
        let _ = self.journal.send(format!("E {:.4} {what}", self.at()));
    }

    /// Un relevé reçu.
    fn poll(&mut self, survey: &Survey, shared: &Shared) {
        let (windows, dock) = (survey.windows_took, survey.dock_took);
        self.polls += 1;
        self.total += windows;
        self.worst = self.worst.max(windows);
        self.dock_worst = self.dock_worst.max(dock);
        self.windows = survey.windows.len();
        let ms = |d: Duration| d.as_secs_f64() * 1000.0;
        if windows + dock > SLOW_POLL {
            self.note(&format!(
                "relevé lent (processus auxiliaire) : fenêtres {:.1} ms ({}), Dock {:.1} ms",
                ms(windows),
                self.windows,
                ms(dock),
            ));
        }
        if self.since.elapsed() >= SUMMARY_EVERY {
            let folder = Duration::from_micros(shared.folder_worst.swap(0, Ordering::Relaxed));
            let _ = self.journal.send(format!(
                "SW {:.4} releves {} moyen_ms {:.2} max_ms {:.2} dock_max_ms {:.2} dossier_max_ms {:.2} fenetres {}",
                self.at(),
                self.polls,
                ms(self.total) / f64::from(self.polls.max(1)),
                ms(self.worst),
                ms(self.dock_worst),
                ms(folder),
                self.windows,
            ));
            *self = Self::new((self.journal.clone(), self.begun));
        }
    }
}

/// Le relevé : un processus auxiliaire (relancé s'il s'arrête), sinon un
/// fil de l'app.
fn survey(shared: &Arc<Shared>, journal: Option<(Sender<String>, Instant)>) {
    let mut timing = journal.map(Timing::new);
    let mut restarts = 0;
    while !shared.stop.load(Ordering::SeqCst) {
        let child = if cfg!(target_os = "macos") {
            spawn_helper()
        } else {
            Err("pas de processus auxiliaire sur ce système".to_owned())
        };
        let mut child = match child {
            Ok(child) => child,
            Err(e) => {
                log::warn!("relevé dans l'app : {e}");
                if let Some(timing) = &timing {
                    timing.note(&format!("relevé dans l'app ({e})"));
                }
                in_process(shared, timing.as_mut());
                return;
            }
        };
        if let Some(timing) = &timing {
            timing.note(&format!("relevé : processus auxiliaire n° {}", child.id()));
        }
        // Le processus relancé reprend où en était l'app.
        let mut stdin = child.stdin.take();
        if let Some(stdin) = stdin.as_mut() {
            let replay = [
                Command::Fast(shared.fast.load(Ordering::SeqCst)),
                Command::Dock(shared.dock.load(Ordering::SeqCst)),
            ];
            for command in replay {
                let _ = writeln!(stdin, "{}", command.line());
            }
        }
        let writer = stdin.map(|stdin| {
            let shared = Arc::clone(shared);
            std::thread::Builder::new()
                .name("mouflon-releve-demandes".into())
                .spawn(move || write_commands(&shared, stdin))
        });
        if let Some(Err(e)) = &writer {
            log::warn!("demandes au relevé impossibles : {e}");
        }
        if let Some(stdout) = child.stdout.take() {
            read_surveys(shared, stdout, timing.as_mut());
        }
        // Fin du tube : le processus s'est arrêté (ou l'app quitte).
        let status = child.wait();
        if shared.stop.load(Ordering::SeqCst) {
            return;
        }
        restarts += 1;
        let what =
            format!("relevé : processus auxiliaire arrêté ({status:?}), relance n° {restarts}");
        log::warn!("{what}");
        if let Some(timing) = &timing {
            timing.note(&what);
        }
        if restarts > MAX_RESTARTS {
            log::error!("relevé : processus auxiliaire instable, relevé dans l'app");
            in_process(shared, timing.as_mut());
            return;
        }
        std::thread::sleep(RESTART_AFTER);
    }
}

fn spawn_helper() -> Result<Child, String> {
    let exe = std::env::current_exe().map_err(|e| format!("programme introuvable : {e}"))?;
    std::process::Command::new(exe)
        .arg(HELPER_ARG)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::inherit())
        .spawn()
        .map_err(|e| format!("processus auxiliaire impossible : {e}"))
}

/// Lit les relevés du processus auxiliaire jusqu'à la fin du tube.
fn read_surveys(
    shared: &Shared,
    stdout: std::process::ChildStdout,
    mut timing: Option<&mut Timing>,
) {
    let mut reader = Reader::default();
    for line in BufReader::new(stdout).lines() {
        let Ok(line) = line else {
            break;
        };
        match reader.line(&line) {
            Ok(Some(survey)) => publish(shared, survey, timing.as_deref_mut()),
            Ok(None) => {}
            Err(e) => log::warn!("relevé : {e}"),
        }
        if shared.stop.load(Ordering::SeqCst) {
            break;
        }
    }
}

fn publish(shared: &Shared, survey: Survey, timing: Option<&mut Timing>) {
    if let Some(timing) = timing {
        timing.poll(&survey, shared);
    }
    if let Ok(mut latest) = shared.latest.lock() {
        *latest = Some(survey);
        shared.arrived.notify_all();
    }
}

/// Transmet les demandes de l'app au processus auxiliaire.
fn write_commands(shared: &Shared, mut stdin: ChildStdin) {
    loop {
        let commands = {
            let Ok(mut outbox) = shared.outbox.lock() else {
                return;
            };
            while outbox.is_empty() && !shared.stop.load(Ordering::SeqCst) {
                outbox = match shared.wake.wait(outbox) {
                    Ok(outbox) => outbox,
                    Err(_) => return,
                };
            }
            std::mem::take(&mut *outbox)
        };
        if shared.stop.load(Ordering::SeqCst) {
            // Le tube se ferme : le processus auxiliaire s'arrête.
            return;
        }
        for command in commands {
            if writeln!(stdin, "{}", command.line()).is_err() {
                return;
            }
        }
        if stdin.flush().is_err() {
            return;
        }
    }
}

/// Le relevé sur un fil de l'app, quand le processus auxiliaire manque.
fn in_process(shared: &Shared, mut timing: Option<&mut Timing>) {
    let requests = Requests::default();
    let mut surveyor = Surveyor::new(false, None);
    let mut last: Option<Instant> = None;
    while !shared.stop.load(Ordering::SeqCst) {
        let urgent = {
            let Ok(mut outbox) = shared.outbox.lock() else {
                return;
            };
            let commands = std::mem::take(&mut *outbox);
            let urgent = commands.contains(&Command::Now);
            for command in commands {
                requests.apply(command);
            }
            urgent
        };
        requests
            .fast
            .store(shared.fast.load(Ordering::SeqCst), Ordering::SeqCst);
        requests
            .dock
            .store(shared.dock.load(Ordering::SeqCst), Ordering::SeqCst);
        let now = Instant::now();
        let due = crate::helper::next_poll(last, requests.fast.load(Ordering::SeqCst), urgent);
        if let Some(due) = due.filter(|&due| due > now) {
            if let Ok(outbox) = shared.outbox.lock() {
                let _ = shared.wake.wait_timeout_while(outbox, due - now, |outbox| {
                    outbox.is_empty() && !shared.stop.load(Ordering::SeqCst)
                });
            }
            continue;
        }
        last = Some(now);
        let survey = surveyor.poll(&requests, false);
        publish(shared, survey, timing.as_deref_mut());
    }
}

/// Relit le dossier des animaux de temps en temps.
fn watch_folder(shared: &Shared, dir: &std::path::Path) {
    while !shared.stop.load(Ordering::SeqCst) {
        let started = Instant::now();
        let latest = stamp(dir);
        let took = started.elapsed().as_micros() as u64;
        shared.folder_worst.fetch_max(took, Ordering::Relaxed);
        if let Ok(mut pets) = shared.pets.lock() {
            *pets = Some(latest);
        }
        let Ok(guard) = shared.outbox.lock() else {
            return;
        };
        let _ = shared
            .wake
            .wait_timeout_while(guard, FOLDER_EVERY, |_| !shared.stop.load(Ordering::SeqCst));
    }
}
