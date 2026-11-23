//! L'horloge de l'écran sous Windows : les images des animaux suivent le
//! rafraîchissement de l'écran, comme sur Mac. Une minuterie laissée à
//! elle-même dérive peu à peu par rapport à l'écran : de temps en temps, une
//! image reste affichée deux fois et la suivante saute, et le mouvement fait
//! un à-coup.
//!
//! Un petit fil bat pour le fil de l'affichage : il note chaque battement
//! dans ses [`Beats`] et le réveille. Il suit **le compositeur de Windows
//! lui-même** (`DwmFlush`, qui rend la main quand le compositeur vient de
//! finir une image) : l'image suivante de Mouflon part aussitôt, avec toute
//! une période devant elle, et le compositeur en reçoit une par image qu'il
//! fait, ni plus ni moins. Un compositeur lent (une machine virtuelle sans
//! carte graphique) n'est ainsi jamais débordé : mesuré sur celle de la CI,
//! il ne montre qu'environ 45 images par seconde sur 64 rafraîchissements,
//! quoi qu'on lui envoie ; en le suivant, chaque image de Mouflon est
//! montrée, alors qu'à cadence fixe un tiers serait calculé pour rien.
//!
//! Si le compositeur ne donne pas le rythme (bureau à distance réduit), le
//! fil compte le temps lui-même, avec une minuterie fine. Quand personne ne
//! peut voir les animaux (session verrouillée, écran éteint), il se tait :
//! l'affichage ralentit alors de lui-même.
//!
//! Deux autres rythmes existent pour les essais (`MOUFLON_HORLOGE`), que la
//! CI compare à chaque passage : des battements prévus d'avance entre deux
//! rafraîchissements ([`Plan`] ; `0.5` pour le milieu), et l'attente que le
//! compositeur ait pris la dernière image envoyée (`envoi`).

use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::{Arc, Condvar, Mutex, MutexGuard, OnceLock, PoisonError};
use std::time::{Duration, Instant};

use windows_sys::Win32::Foundation::{CloseHandle, HANDLE};
use windows_sys::Win32::Graphics::Dwm::{DWM_TIMING_INFO, DwmFlush, DwmGetCompositionTimingInfo};
use windows_sys::Win32::System::Performance::{QueryPerformanceCounter, QueryPerformanceFrequency};
use windows_sys::Win32::System::Threading::{
    CREATE_WAITABLE_TIMER_HIGH_RESOLUTION, CreateWaitableTimerExW, SetWaitableTimer,
    TIMER_ALL_ACCESS, WaitForMultipleObjects, WaitForSingleObject,
};

use crate::beats::{Grid, Plan};
use crate::{Beat, Beats, LoopWaker};

/// Personne ne voit les animaux (voir [`set_asleep`]).
static ASLEEP: AtomicBool = AtomicBool::new(false);
/// Sans cadence connue de l'écran : soixante images par seconde.
const DEFAULT_PERIOD: f64 = 1.0 / 60.0;
/// L'heure et la cadence de l'écran sont relues de temps en temps (elles
/// changent avec les réglages d'affichage, ou d'elles-mêmes sur un écran à
/// cadence variable).
const PERIOD_EVERY: Duration = Duration::from_secs(1);
/// Une attente du rafraîchissement plus courte que cela n'en était pas une.
const TOO_SHORT: f64 = 0.001;

/// La session est verrouillée ou l'écran éteint (`true`), ou de nouveau
/// visible : les horloges se taisent, ou reprennent.
pub(super) fn set_asleep(asleep: bool) {
    if ASLEEP.swap(asleep, Ordering::Relaxed) != asleep {
        log::info!(
            "horloge de l'écran : {}",
            if asleep {
                "personne ne regarde, elle se tait"
            } else {
                "elle reprend"
            }
        );
    }
}

/// Les secondes du compteur du système (celui dont le compositeur donne les
/// heures).
fn counter() -> f64 {
    static FREQUENCY: OnceLock<f64> = OnceLock::new();
    let frequency = *FREQUENCY.get_or_init(|| {
        let mut frequency = 0i64;
        // SAFETY: `frequency` est à nous ; l'appel ne fait que l'écrire.
        unsafe { QueryPerformanceFrequency(&mut frequency) };
        (frequency as f64).max(1.0)
    });
    let mut ticks = 0i64;
    // SAFETY: `ticks` est à nous ; l'appel ne fait que l'écrire.
    unsafe { QueryPerformanceCounter(&mut ticks) };
    ticks as f64 / frequency
}

/// Ce que le compositeur de Windows dit de l'écran.
#[derive(Clone, Copy, Debug, PartialEq)]
struct Timing {
    /// Durée d'une image de l'écran, en secondes.
    period: f64,
    /// L'heure d'un de ses rafraîchissements (le dernier ou le prochain),
    /// en secondes du compteur du système.
    vblank: Option<f64>,
    /// L'heure où le compositeur a assemblé sa dernière image, et les
    /// comptes qu'il tient (essais).
    composed: f64,
    refreshes: u64,
    frames: u64,
}

fn timing() -> Option<Timing> {
    let mut info = DWM_TIMING_INFO {
        cbSize: size_of::<DWM_TIMING_INFO>() as u32,
        ..DWM_TIMING_INFO::default()
    };
    let mut frequency = 0i64;
    // SAFETY: `info` porte sa taille et `frequency` est à nous ; sans
    // fenêtre, la question porte sur tout le bureau.
    let known = unsafe {
        DwmGetCompositionTimingInfo(std::ptr::null_mut(), &mut info) == 0
            && QueryPerformanceFrequency(&mut frequency) != 0
    };
    // (La structure est compacte : ses champs se lisent par copie.)
    let (ticks, vblank, composed) = (info.qpcRefreshPeriod, info.qpcVBlank, info.qpcCompose);
    let (refreshes, frames) = (info.cRefresh, info.cFrame);
    if !known || frequency <= 0 || ticks == 0 {
        return None;
    }
    let frequency = frequency as f64;
    let period = ticks as f64 / frequency;
    (1.0 / 500.0..=1.0 / 20.0)
        .contains(&period)
        .then_some(Timing {
            period,
            vblank: (vblank != 0).then(|| vblank as f64 / frequency),
            composed: composed as f64 / frequency,
            refreshes,
            frames,
        })
}

/// Une minuterie fine de Windows : elle réveille à quelques dixièmes de
/// milliseconde près, sans changer le pas d'horloge de tout le système.
struct Timer(HANDLE);

impl Timer {
    /// `None` sur un Windows qui n'en a pas (avant 2018).
    fn new() -> Option<Self> {
        // SAFETY: ni attributs ni nom ; rend une poignée, ou rien.
        let handle = unsafe {
            CreateWaitableTimerExW(
                std::ptr::null(),
                std::ptr::null(),
                CREATE_WAITABLE_TIMER_HIGH_RESOLUTION,
                TIMER_ALL_ACCESS,
            )
        };
        (!handle.is_null()).then_some(Self(handle))
    }

    /// Arme la minuterie pour dans `seconds` ; `false` si elle s'y refuse.
    fn arm(&self, seconds: f64) -> bool {
        // En centaines de nanosecondes, négatif : « dans tant de temps ».
        let due = -((seconds * 1e7).ceil() as i64).max(1);
        // SAFETY: la poignée est à nous et vivante ; `due` vit jusqu'au
        // retour ; ni période, ni fonction de rappel.
        unsafe { SetWaitableTimer(self.0, &due, 0, None, std::ptr::null(), 0) != 0 }
    }

    /// Dort `seconds` ; `false` si la minuterie n'a pas pu être armée.
    fn sleep(&self, seconds: f64) -> bool {
        if seconds <= 0.0 {
            return true;
        }
        // Une attente bornée : quoi qu'il arrive à la minuterie, le fil
        // repart.
        let limit = (seconds * 1000.0) as u32 + 100;
        // SAFETY: la poignée est à nous et vivante.
        self.arm(seconds) && unsafe { WaitForSingleObject(self.0, limit) } == 0
    }

    /// Attend `event` (une poignée de Windows, vivante) `seconds` au plus ;
    /// `true` s'il a été signalé.
    fn wait_for(&self, event: isize, seconds: f64) -> bool {
        let handles = [event as HANDLE, self.0];
        let limit = (seconds * 1000.0) as u32 + 100;
        // SAFETY: les deux poignées sont vivantes, et le tableau vit
        // jusqu'au retour.
        self.arm(seconds) && unsafe { WaitForMultipleObjects(2, handles.as_ptr(), 0, limit) } == 0
    }
}

impl Drop for Timer {
    fn drop(&mut self) {
        // SAFETY: la poignée est à nous, et personne d'autre ne s'en sert.
        unsafe {
            CloseHandle(self.0);
        }
    }
}

/// D'où l'horloge tient son rythme.
#[derive(Clone, Copy, Debug, PartialEq)]
enum Rhythm {
    /// Des battements prévus d'avance, à cette part de la période de
    /// l'écran.
    Planned(f64),
    /// Le compositeur de Windows, quand il a fini une image (`DwmFlush`).
    Compositor,
    /// Le compositeur de Windows, quand il a pris le dernier envoi.
    Taken,
}

/// Ce qu'un essai demande à l'horloge (`MOUFLON_HORLOGE`) : `envoi` pour
/// attendre que le compositeur ait pris chaque image, ou la part de la
/// période où battre (`0.5` : au milieu). Sans rien : le compositeur.
fn asked() -> Rhythm {
    let asked = std::env::var("MOUFLON_HORLOGE").ok();
    match asked.as_deref().map(str::trim) {
        Some("envoi") => Rhythm::Taken,
        Some(phase) => phase
            .parse::<f64>()
            .ok()
            .filter(|phase| (0.0..1.0).contains(phase))
            .map_or(Rhythm::Compositor, Rhythm::Planned),
        None => Rhythm::Compositor,
    }
}

/// Ce que l'affichage demande à l'horloge.
#[derive(Clone, Copy, Debug)]
struct Wanted {
    running: bool,
    /// Battements par seconde.
    rate: f32,
    stop: bool,
}

#[derive(Debug)]
struct Shared {
    wanted: Mutex<Wanted>,
    changed: Condvar,
    /// Durée entre deux battements au dernier battement (les bits d'un `f64`).
    frame: AtomicU64,
}

impl Shared {
    fn wanted(&self) -> MutexGuard<'_, Wanted> {
        self.wanted.lock().unwrap_or_else(PoisonError::into_inner)
    }
}

/// Horloge calée sur l'écran, qui bat pour le fil de l'affichage : à chaque
/// battement, elle le note dans ses [`Beats`] et réveille ce fil (son
/// [`LoopWaker`]).
#[derive(Debug)]
pub struct ThreadClock {
    shared: Arc<Shared>,
    max_fps: f32,
}

impl ThreadClock {
    /// Crée l'horloge, arrêtée. `None` si son fil ne peut pas être lancé
    /// (l'affichage se rythme alors avec une minuterie).
    pub fn new(beats: Arc<Beats>, wake: Arc<LoopWaker>) -> Option<Self> {
        let period = timing().map_or(DEFAULT_PERIOD, |timing| timing.period);
        let shared = Arc::new(Shared {
            wanted: Mutex::new(Wanted {
                running: false,
                rate: 0.0,
                stop: false,
            }),
            changed: Condvar::new(),
            frame: AtomicU64::new(period.to_bits()),
        });
        {
            let shared = Arc::clone(&shared);
            std::thread::Builder::new()
                .name("mouflon-horloge".into())
                .spawn(move || run(&shared, &beats, &wake, period))
                .ok()?;
        }
        Some(Self {
            shared,
            max_fps: (1.0 / period).clamp(30.0, 360.0) as f32,
        })
    }

    /// Rien à faire ici : l'horloge bat sur son propre fil.
    pub fn start_here(&self) {}

    /// Images par seconde que l'écran sait afficher, au plus.
    #[must_use]
    pub fn max_fps(&self) -> f32 {
        self.max_fps
    }

    /// Bat-elle en ce moment ? (Pas quand personne ne regarde : session
    /// verrouillée, écran éteint.)
    #[must_use]
    pub fn is_running(&self) -> bool {
        self.shared.wanted().running && !ASLEEP.load(Ordering::Relaxed)
    }

    /// Durée entre deux battements au dernier battement, en secondes.
    #[must_use]
    pub fn frame_duration(&self) -> f64 {
        f64::from_bits(self.shared.frame.load(Ordering::Relaxed))
    }

    /// Fait tourner l'horloge, ou l'arrête (plus rien ne bouge à l'écran).
    pub fn set_running(&self, running: bool) {
        let mut wanted = self.shared.wanted();
        if wanted.running != running {
            wanted.running = running;
            self.shared.changed.notify_all();
        }
    }

    /// Cadence voulue, en battements par seconde : celle de l'écran quand un
    /// animal se déplace, moins quand il se repose.
    pub fn set_rate(&self, fps: f32) {
        self.shared.wanted().rate = fps.clamp(10.0, self.max_fps);
    }
}

impl Drop for ThreadClock {
    fn drop(&mut self) {
        // Son fil s'arrête de lui-même à son prochain tour (au plus un
        // rafraîchissement de l'écran plus tard) : on ne l'attend pas, le
        // fil de l'affichage ne doit jamais attendre.
        self.shared.wanted().stop = true;
        self.shared.changed.notify_all();
    }
}

/// Ce que l'horloge a fait depuis une seconde, pour le journal d'un essai.
#[derive(Default)]
struct Trace {
    beats: u32,
    /// Retards des réveils sur l'heure prévue, en millisecondes.
    late: Vec<f32>,
    /// Où tombaient les réveils dans la période de l'écran (de 0 à 1).
    phases: Vec<f32>,
    before: Option<Timing>,
}

impl Trace {
    fn beat(&mut self, due: f64, woke: f64, timing: Option<Timing>) {
        self.beats += 1;
        self.late.push(((woke - due) * 1000.0) as f32);
        if let Some(Timing {
            period,
            vblank: Some(vblank),
            ..
        }) = timing
        {
            self.phases
                .push(((woke - vblank) / period).rem_euclid(1.0) as f32);
        }
    }

    /// Une ligne au journal, et on repart de zéro.
    fn tell(&mut self, now: f64, timing: Option<Timing>) {
        fn spread(values: &mut [f32]) -> String {
            if values.is_empty() {
                return "—".to_owned();
            }
            values.sort_by(f32::total_cmp);
            format!(
                "{:.2} / {:.2} / {:.2}",
                values[0],
                values[values.len() / 2],
                values[values.len() - 1]
            )
        }
        let screen = match (timing, self.before) {
            (Some(timing), Some(before)) => format!(
                "écran {:.3} ms, rafraîchissement annoncé il y a {:.2} ms, image assemblée il \
                 y a {:.2} ms ({} rafraîchissements et {} images du compositeur depuis)",
                timing.period * 1000.0,
                timing
                    .vblank
                    .map_or(f64::NAN, |vblank| (now - vblank) * 1000.0),
                (now - timing.composed) * 1000.0,
                timing.refreshes.wrapping_sub(before.refreshes),
                timing.frames.wrapping_sub(before.frames),
            ),
            _ => "écran : pas encore relu".to_owned(),
        };
        log::info!(
            "horloge : {} battements ; retard des réveils {} ms ; place dans la période {} ; {}",
            self.beats,
            spread(&mut self.late),
            spread(&mut self.phases),
            screen
        );
        *self = Self {
            before: timing,
            ..Self::default()
        };
    }
}

/// Le fil de l'horloge : un battement par rafraîchissement de l'écran (ou
/// tous les quelques-uns, selon la cadence voulue), tant qu'elle tourne.
fn run(shared: &Shared, beats: &Beats, wake: &LoopWaker, mut period: f64) {
    super::set_thread_priority(true);
    let mut rhythm = asked();
    // Sans minuterie fine (Windows d'avant 2018), il ne reste qu'à attendre
    // le compositeur.
    let timer = Timer::new();
    let sent = (rhythm == Rhythm::Taken)
        .then(super::scene::sent_event)
        .flatten();
    if timer.is_none() || (rhythm == Rhythm::Taken && sent.is_none()) {
        rhythm = Rhythm::Compositor;
    }
    log::info!(
        "horloge de l'écran : {}",
        match rhythm {
            Rhythm::Planned(phase) => format!(
                "battements prévus entre deux rafraîchissements (à {:.0} % de la période)",
                phase * 100.0
            ),
            Rhythm::Compositor => "à l'attente du compositeur de Windows".to_owned(),
            Rhythm::Taken => "au rythme où le compositeur de Windows prend les images".to_owned(),
        }
    );
    // Les heures des battements sont données dans le temps de l'affichage.
    let offset = crate::diag::media_time() - counter();
    // Sans heure de l'écran, la grille part d'ici.
    let origin = counter();
    let mut screen = timing();
    let mut trace = std::env::var_os("MOUFLON_ESSAI").map(|_| Trace::default());
    let mut plan = Plan::default();
    let mut grid = Grid::default();
    let mut read = Instant::now();
    // Attentes trop courtes de suite : à la troisième, le compositeur ne
    // donne pas le rythme (une seule peut être un rafraîchissement tout
    // proche, après un retard de ce fil).
    let mut short = 0u32;
    let mut by_timer = false;
    loop {
        let rate = {
            let mut wanted = shared.wanted();
            loop {
                if wanted.stop {
                    return;
                }
                if wanted.running && !ASLEEP.load(Ordering::Relaxed) {
                    break wanted.rate;
                }
                // Arrêtée : jusqu'à nouvel ordre. Personne ne regarde : on
                // repasse voir de temps en temps.
                grid = Grid::default();
                plan.reset();
                wanted = if wanted.running {
                    shared
                        .changed
                        .wait_timeout(wanted, Duration::from_millis(250))
                        .unwrap_or_else(PoisonError::into_inner)
                        .0
                } else {
                    shared
                        .changed
                        .wait(wanted)
                        .unwrap_or_else(PoisonError::into_inner)
                };
            }
        };
        if read.elapsed() >= PERIOD_EVERY {
            read = Instant::now();
            let known = timing();
            if let Some(known) = known {
                period = known.period;
                screen = Some(known);
            }
            if let Some(trace) = &mut trace {
                trace.tell(counter(), known);
            }
        }
        let beat = match (rhythm, &timer) {
            (Rhythm::Planned(phase), Some(timer)) => {
                let anchor = screen.and_then(|screen| screen.vblank).unwrap_or(origin);
                let now = counter();
                let (due, step) = plan.next(now, anchor, period, phase, f64::from(rate));
                if !timer.sleep(due - now) {
                    // La minuterie se dérobe : on dort comme on peut.
                    std::thread::sleep(Duration::from_secs_f64((due - now).clamp(0.0, 0.1)));
                }
                let woke = counter();
                if let Some(trace) = &mut trace {
                    trace.beat(due, woke, screen);
                }
                Some(Beat {
                    target: due + step + offset,
                    vsync: due + offset,
                    called: woke + offset,
                })
            }
            (Rhythm::Taken, Some(timer)) => {
                // L'affichage vient d'envoyer une image : on attend que le
                // compositeur l'ait prise, et le battement suivant part
                // aussitôt (la prochaine image a toute une période devant
                // elle). Rien d'envoyé (rien n'a bougé) : une période.
                if timer.wait_for(sent.unwrap_or(0), period) && !super::scene::wait_taken() {
                    std::thread::sleep(Duration::from_secs_f64(period.clamp(TOO_SHORT, 0.05)));
                }
                let now = crate::diag::media_time();
                let beat = grid.tick(now, period, f64::from(rate));
                if let (Some(trace), Some(beat)) = (&mut trace, beat) {
                    trace.beat(beat.vsync - offset, now - offset, screen);
                }
                beat
            }
            _ => {
                let before = crate::diag::media_time();
                // SAFETY: sans argument ; attend le prochain rafraîchissement.
                let flushed = unsafe { DwmFlush() } == 0;
                let mut now = crate::diag::media_time();
                if flushed && now - before >= TOO_SHORT {
                    short = 0;
                } else {
                    short = short.saturating_add(1);
                }
                if short >= 3 {
                    // Le compositeur n'attend pas : on compte le temps
                    // nous-mêmes.
                    let left = (period - (now - before)).clamp(TOO_SHORT, 0.05);
                    if !timer.as_ref().is_some_and(|timer| timer.sleep(left)) {
                        std::thread::sleep(Duration::from_secs_f64(left));
                    }
                    now = crate::diag::media_time();
                }
                if (short >= 3) != by_timer {
                    by_timer = short >= 3;
                    log::info!(
                        "horloge de l'écran : {}",
                        if by_timer {
                            "le compositeur ne donne pas le rythme, elle compte le temps elle-même"
                        } else {
                            "au rythme du compositeur de Windows"
                        }
                    );
                }
                let beat = grid.tick(now, period, f64::from(rate));
                if let (Some(trace), Some(beat)) = (&mut trace, beat) {
                    trace.beat(beat.vsync - offset, now - offset, screen);
                }
                beat
            }
        };
        if let Some(beat) = beat {
            shared
                .frame
                .store((beat.target - beat.vsync).to_bits(), Ordering::Relaxed);
            beats.push(beat);
            wake.wake();
        }
    }
}
