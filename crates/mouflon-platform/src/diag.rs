//! Diagnostics des essais (`MOUFLON_ESSAI`) : un fil à part, `mouflon-diag`,
//! surveille le fil principal et relève toutes les dix secondes ce que
//! consomment le processus, chacun de ses fils et le système.
//!
//! - **Retards et blocages.** L'app dit quand une image commence
//!   ([`frame_started`]) et quand elle attend la suivante ([`expect_frame`]).
//!   Si la suivante tarde, ou si la boucle d'événements reste occupée plus de
//!   50 ms, le fil des diagnostics relève la pile du fil principal toutes les
//!   4 ms tant que cela dure (macOS, Apple Silicon) : on voit où il était —
//!   occupé dans telle fonction, ou endormi, en attente d'un réveil qui ne
//!   vient pas. Il note aussi ses propres retards : s'il a dormi bien plus
//!   que demandé, c'est tout le processus qui n'a pas eu de cœur.
//! - **Boucle d'événements** (macOS) : temps occupé et endormi, réveils,
//!   plus longue tâche, modes (menu ouvert, fenêtre modale…).
//! - **Processus** : processeur (dont cœurs performants), énergie,
//!   instructions et cycles, temps passé à attendre un cœur, réveils,
//!   disque, mémoire, fautes de page, changements de contexte, appels
//!   système, messages Mach, fils, descripteurs, temps par classe de
//!   service ; fil par fil ; allocations ([`crate::alloc_counts`]).
//! - **Système** : processeur de toute la machine, charge, mémoire
//!   (pression, compression, échange), état thermique, économie d'énergie ;
//!   et ses notifications ([`observe_system`]) : veille, réveil, écrans,
//!   bureaux, app au premier plan, occultation des fenêtres…
//!
//! Chaque relevé est une ligne de texte, passée à la fonction donnée par
//! l'app (le journal de l'essai). Leur format est décrit dans
//! `mouflon-app/src/stats.rs`.

use std::collections::HashMap;
use std::fmt::Write as _;
use std::sync::OnceLock;
use std::sync::atomic::{AtomicU64, Ordering::Acquire, Ordering::Relaxed, Ordering::Release};
use std::time::{Duration, Instant};

use crate::alloc::{AllocCounts, alloc_counts};

#[cfg(target_os = "macos")]
use crate::macos::diag as imp;

/// Où vont les lignes du fil des diagnostics.
pub type Sink = Box<dyn Fn(String) + Send + 'static>;

/// Le fil des diagnostics se réveille à ce rythme.
const TICK: Duration = Duration::from_millis(4);
/// Un relevé des ressources toutes les…
const PERIOD: Duration = Duration::from_secs(10);
/// Boucle d'événements occupée sans répit au-delà : un blocage.
const BUSY_LIMIT: u64 = 50_000_000;
/// Un retard dont l'écart entre deux images dépasse ceci est raconté en
/// entier (piles comprises) ; les autres sont regroupés dans le relevé.
const FULL_GAP: u64 = 40_000_000;
/// Au plus autant de blocages racontés en entier par essai.
const MAX_FULL: u32 = 400;
/// Profondeur de pile relevée, et gardée dans le journal.
const MAX_FRAMES: usize = 96;
const SHOWN_FRAMES: usize = 32;
/// Piles regroupées par leurs premiers appels (les petits retards).
const GROUP_FRAMES: usize = 14;

/// Instant de référence des temps échangés entre fils (en nanosecondes).
fn epoch() -> Instant {
    static EPOCH: OnceLock<Instant> = OnceLock::new();
    *EPOCH.get_or_init(Instant::now)
}

pub(crate) fn now_ns() -> u64 {
    u64::try_from(epoch().elapsed().as_nanos()).unwrap_or(u64::MAX)
}

/// Début de l'essai : l'heure des lignes du journal se compte de là.
static BEGUN: OnceLock<Instant> = OnceLock::new();

/// Secondes depuis le début de l'essai d'un instant `ns` (voir [`now_ns`]).
fn at_of(ns: u64) -> f64 {
    let instant = epoch() + Duration::from_nanos(ns);
    let begun = BEGUN.get().copied().unwrap_or_else(epoch);
    instant.saturating_duration_since(begun).as_secs_f64()
}

static FRAME_SEQ: AtomicU64 = AtomicU64::new(0);
static FRAME_AT: AtomicU64 = AtomicU64::new(0);
/// Heure (ns) avant laquelle la prochaine image doit commencer ; `u64::MAX` :
/// aucune attendue (une image est en cours, ou l'essai n'a pas commencé).
static EXPECT_BY: AtomicU64 = AtomicU64::new(u64::MAX);
static TOLERANCE: AtomicU64 = AtomicU64::new(3_000_000);

/// Une image commence (fil principal).
pub fn frame_started() {
    FRAME_AT.store(now_ns(), Relaxed);
    EXPECT_BY.store(u64::MAX, Relaxed);
    FRAME_SEQ.fetch_add(1, Release);
}

/// La prochaine image est attendue dans `interval` (fil principal, à la fin
/// de l'image). Au-delà d'une demi-image de plus (3 ms au moins), le fil des
/// diagnostics la juge en retard et relève la pile du fil principal.
pub fn expect_frame(interval: Duration) {
    let interval = u64::try_from(interval.as_nanos()).unwrap_or(u64::MAX / 4);
    TOLERANCE.store((interval / 2).max(3_000_000), Relaxed);
    EXPECT_BY.store(now_ns().saturating_add(interval), Release);
}

/// Le fil principal, pour les relevés de pile et le compte de la boucle
/// d'événements. À appeler depuis le fil principal, avant [`start`].
pub fn register_main_thread() {
    epoch();
    imp::register_main_thread();
}

/// Le fil qui fait les images (le fil de l'affichage) : c'est sa pile que
/// l'on relève quand une image tarde. À appeler depuis ce fil.
pub fn register_frame_thread() {
    epoch();
    imp::register_frame_thread();
}

/// Lance le fil des diagnostics. `begun` : début de l'essai.
pub fn start(begun: Instant, sink: Sink) -> bool {
    epoch();
    let _ = BEGUN.set(begun);
    let spawned = std::thread::Builder::new()
        .name("mouflon-diag".into())
        .spawn(move || Watch::new(sink).run());
    spawned.is_ok()
}

/// Les notifications du système (veille, écrans, app au premier plan…),
/// en lignes `E` du journal. À appeler depuis le fil principal.
pub fn observe_system(sink: impl Fn(String) + 'static) {
    imp::observe_system(Box::new(move |what: String| {
        let at = BEGUN.get().map_or(0.0, |b| b.elapsed().as_secs_f64());
        sink(format!("E {at:.4} système : {what}"));
    }));
}

/// En-tête du journal : le Mac, son système, ses écrans, le processus.
#[must_use]
pub fn system_info() -> Vec<String> {
    imp::system_info()
}

/// Heure de l'horloge des médias (celle de l'horloge de l'écran), en secondes.
#[must_use]
pub fn media_time() -> f64 {
    imp::media_time()
}

/// La boucle d'événements depuis le lancement : réveils, temps occupé et
/// endormi (ns). Zéro hors macOS.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct LoopTotals {
    pub wakeups: u64,
    pub busy: u64,
    pub asleep: u64,
}

/// Les comptes de la boucle d'événements (fil principal ou autre).
#[must_use]
pub fn loop_totals() -> LoopTotals {
    imp::loop_totals()
}

/// Ce que fait la boucle d'événements du fil principal.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[cfg_attr(not(target_os = "macos"), allow(dead_code))]
pub(crate) enum LoopState {
    Unknown,
    /// Endormie, en attente d'un événement.
    Asleep,
    /// Occupée depuis cette heure (ns).
    Busy(u64),
}

/// Ce que le processus a consommé depuis son lancement (et quelques jauges).
/// Les durées sont en nanosecondes.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub(crate) struct ProcStats {
    pub user: u64,
    pub system: u64,
    /// Temps passé sur les cœurs performants, si le système le dit.
    pub performance: Option<u64>,
    /// Énergie consommée, en nanojoules, si le système le dit.
    pub energy_nj: Option<u64>,
    pub instructions: u64,
    pub cycles: u64,
    /// Temps passé prêt à tourner, sans cœur libre.
    pub runnable: u64,
    pub interrupt_wakeups: u64,
    pub idle_wakeups: u64,
    pub disk_read: u64,
    pub disk_written: u64,
    pub logical_writes: u64,
    pub footprint: u64,
    pub interval_max_footprint: u64,
    pub lifetime_max_footprint: u64,
    /// Temps de calcul par classe de service : interactive, initiée par
    /// l'utilisateur, par défaut, utilitaire, de fond, maintenance, ancienne.
    pub qos: [u64; 7],
    pub faults: u64,
    pub pageins: u64,
    pub cow_faults: u64,
    pub csw: u64,
    pub mach_calls: u64,
    pub unix_calls: u64,
    pub messages_sent: u64,
    pub messages_received: u64,
    pub threads: u32,
    pub running: u32,
    pub fds: u32,
}

/// Un fil du processus.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub(crate) struct ThreadStat {
    pub id: u64,
    pub name: String,
    /// Temps de calcul depuis sa création (ns).
    pub cpu: u64,
    /// 1 en cours, 2 arrêté, 3 en attente, 4 en attente non interruptible.
    pub state: i32,
    pub priority: i32,
}

/// Le système (jauges et comptes depuis son démarrage).
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub(crate) struct SystemStats {
    /// Tops du processeur de toute la machine : utilisateur, système,
    /// inactif, gentil.
    pub cpu_ticks: [u64; 4],
    pub load: [f64; 3],
    /// 1 normale, 2 alerte, 4 critique.
    pub pressure: Option<i32>,
    pub page_size: u64,
    pub free: u64,
    pub active: u64,
    pub inactive: u64,
    pub wired: u64,
    pub compressed: u64,
    pub pageins: u64,
    pub pageouts: u64,
    pub swapins: u64,
    pub swapouts: u64,
    pub decompressions: u64,
    pub swap_used: u64,
    /// 0 normal, 1 tiède, 2 chaud, 3 critique.
    pub thermal: Option<i32>,
    pub low_power: Option<bool>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Cause {
    /// L'image suivante n'est pas venue à l'heure.
    Frame,
    /// La boucle d'événements est restée occupée.
    Busy,
}

#[derive(Debug)]
struct Sample {
    frames: Vec<usize>,
    asleep: bool,
}

#[derive(Debug)]
struct Stall {
    cause: Cause,
    seq: u64,
    /// Début du retard (ns).
    from: u64,
    /// Début de la dernière image avant lui (ns).
    last_frame: u64,
    samples: Vec<Sample>,
    last_sample: u64,
}

impl Stall {
    /// Toutes les 4 ms les 200 premières fois, puis toutes les 20 ms.
    fn wants_sample(&self, now: u64) -> bool {
        match self.samples.len() {
            0..200 => true,
            200..1000 => now.saturating_sub(self.last_sample) >= 20_000_000,
            _ => false,
        }
    }
}

/// Les petits retards d'un relevé, regroupés.
#[derive(Debug, Default)]
struct Small {
    count: u32,
    samples: u32,
    asleep: u32,
    stacks: HashMap<Vec<usize>, u32>,
}

struct Watch {
    sink: Sink,
    stall: Option<Stall>,
    symbols: HashMap<usize, String>,
    small: Small,
    full: u32,
    /// Retards du fil des diagnostics lui-même (plus que demandé à dormir).
    oversleep_max: Duration,
    oversleeps: u32,
    next_period: Instant,
    period_start: Instant,
    process: Option<ProcStats>,
    system: Option<SystemStats>,
    threads: HashMap<u64, u64>,
    looped: LoopTotals,
    allocs: AllocCounts,
    /// Modes de la boucle d'événements vus pendant le relevé.
    modes: Vec<String>,
}

impl Watch {
    fn new(sink: Sink) -> Self {
        let now = Instant::now();
        Self {
            sink,
            stall: None,
            symbols: HashMap::new(),
            small: Small::default(),
            full: 0,
            oversleep_max: Duration::ZERO,
            oversleeps: 0,
            next_period: now + PERIOD,
            period_start: now,
            process: imp::process(),
            system: Some(imp::system()),
            threads: imp::threads().into_iter().map(|t| (t.id, t.cpu)).collect(),
            looped: imp::loop_totals(),
            allocs: alloc_counts(),
            modes: Vec::new(),
        }
    }

    fn emit(&self, line: String) {
        (self.sink)(line);
    }

    fn run(mut self) {
        imp::raise_thread_priority();
        let mut buffer = [0usize; MAX_FRAMES];
        loop {
            let asked = Instant::now();
            std::thread::sleep(TICK);
            let over = asked.elapsed().saturating_sub(TICK);
            if over > Duration::from_millis(8) {
                self.oversleeps += 1;
                self.oversleep_max = self.oversleep_max.max(over);
                if over > Duration::from_millis(15) {
                    let at = BEGUN.get().map_or(0.0, |b| b.elapsed().as_secs_f64());
                    self.emit(format!(
                        "BD {at:.4} diag_en_retard_ms {:.1}",
                        over.as_secs_f64() * 1000.0
                    ));
                }
            }
            for (ns, mode) in imp::take_mode_changes() {
                self.emit(format!("E {:.4} boucle : mode {mode}", at_of(ns)));
                if !self.modes.contains(&mode) {
                    self.modes.push(mode);
                }
            }
            self.watch(now_ns(), &mut buffer);
            if Instant::now() >= self.next_period {
                self.periodic();
                self.next_period += PERIOD;
            }
        }
    }

    /// Retard ou blocage en cours ? Relève la pile du fil principal.
    fn watch(&mut self, now: u64, buffer: &mut [usize; MAX_FRAMES]) {
        let seq = FRAME_SEQ.load(Acquire);
        let expect = EXPECT_BY.load(Acquire);
        let tolerance = TOLERANCE.load(Relaxed);
        let state = imp::loop_state();
        let late = expect != u64::MAX && now > expect.saturating_add(tolerance);
        let busy = match state {
            LoopState::Busy(since) => now.saturating_sub(since) > BUSY_LIMIT,
            _ => false,
        };
        if let Some(stall) = &self.stall
            && (seq != stall.seq || (!late && !busy))
            && let Some(stall) = self.stall.take()
        {
            self.finish(stall, seq, now);
        }
        if !(late || busy) {
            return;
        }
        let stall = self.stall.get_or_insert_with(|| Stall {
            cause: if late { Cause::Frame } else { Cause::Busy },
            seq,
            from: match state {
                LoopState::Busy(since) if !late => since,
                _ => expect.saturating_add(tolerance).min(now),
            },
            last_frame: FRAME_AT.load(Relaxed),
            samples: Vec::new(),
            last_sample: 0,
        });
        if stall.wants_sample(now) {
            let count = imp::sample_main(buffer);
            stall.last_sample = now;
            if count > 0 {
                stall.samples.push(Sample {
                    frames: dedup(&buffer[..count]),
                    asleep: imp::loop_state() == LoopState::Asleep,
                });
            }
        }
    }

    fn finish(&mut self, stall: Stall, seq: u64, now: u64) {
        let end = if seq == stall.seq {
            now
        } else {
            FRAME_AT.load(Relaxed)
        };
        let gap = end.saturating_sub(stall.last_frame);
        let late = end.saturating_sub(stall.from);
        let asleep = stall.samples.iter().filter(|s| s.asleep).count();
        if stall.samples.is_empty() {
            self.small.count += 1;
            return;
        }
        let full = (gap >= FULL_GAP || stall.cause == Cause::Busy) && self.full < MAX_FULL;
        if !full {
            self.small.count += 1;
            self.small.samples += stall.samples.len() as u32;
            self.small.asleep += asleep as u32;
            for sample in &stall.samples {
                let key = sample.frames[..sample.frames.len().min(GROUP_FRAMES)].to_vec();
                *self.small.stacks.entry(key).or_default() += 1;
            }
            return;
        }
        self.full += 1;
        let mut line = format!(
            "B {:.4} raison {} ecart_ms {:.1} retard_ms {:.1} echantillons {} endormi {}",
            at_of(stall.from),
            match stall.cause {
                Cause::Frame => "image",
                Cause::Busy => "boucle",
            },
            gap as f64 / 1e6,
            late as f64 / 1e6,
            stall.samples.len(),
            asleep,
        );
        let mode = imp::current_mode();
        if !mode.is_empty() {
            let _ = write!(line, " mode {mode}");
        }
        self.emit(line);
        let mut counts: HashMap<&[usize], u32> = HashMap::new();
        for sample in &stall.samples {
            *counts.entry(&sample.frames).or_default() += 1;
        }
        let mut stacks: Vec<(&[usize], u32)> = counts.into_iter().collect();
        stacks.sort_by(|a, b| b.1.cmp(&a.1).then(a.0.cmp(b.0)));
        let lines: Vec<String> = stacks
            .iter()
            .take(3)
            .map(|(frames, count)| format!("BP {count} {}", self.stack(frames)))
            .collect();
        for line in lines {
            self.emit(line);
        }
    }

    /// Une pile en texte, de l'appel le plus profond au plus ancien.
    fn stack(&mut self, frames: &[usize]) -> String {
        let mut out = String::new();
        for (i, &address) in frames.iter().take(SHOWN_FRAMES).enumerate() {
            if i > 0 {
                out.push_str(" < ");
            }
            // Au-delà de la première, ce sont des adresses de retour : le
            // symbole est celui de l'appel, juste avant.
            let lookup = if i == 0 {
                address
            } else {
                address.saturating_sub(1)
            };
            if self.symbols.len() > 50_000 {
                self.symbols.clear();
            }
            let symbol = self
                .symbols
                .entry(lookup)
                .or_insert_with(|| imp::symbolize(lookup));
            out.push_str(symbol);
        }
        if frames.len() > SHOWN_FRAMES {
            let _ = write!(out, " < … ({} de plus)", frames.len() - SHOWN_FRAMES);
        }
        out
    }

    /// Le relevé des dix secondes : processus, fils, système, boucle,
    /// allocations, petits retards.
    fn periodic(&mut self) {
        let wall = self.period_start.elapsed().as_secs_f64().max(1e-3);
        self.period_start = Instant::now();
        let at = BEGUN.get().map_or(0.0, |b| b.elapsed().as_secs_f64());

        let process = imp::process();
        if let (Some(now), Some(before)) = (process, self.process) {
            self.emit(process_line(at, &now, &before, wall));
        }
        self.process = process.or(self.process);

        let threads = imp::threads();
        let mut used: Vec<(f64, &ThreadStat)> = threads
            .iter()
            .map(|t| {
                let before = self.threads.get(&t.id).copied().unwrap_or(0);
                (t.cpu.saturating_sub(before) as f64 / 1e9 / wall * 100.0, t)
            })
            .collect();
        used.sort_by(|a, b| b.0.total_cmp(&a.0));
        let mut line = format!("SF {at:.4} fils {}", threads.len());
        for (pct, thread) in used.iter().filter(|(pct, _)| *pct >= 0.05) {
            let _ = write!(
                line,
                " {}={pct:.1}%/p{}/{}",
                thread_label(thread),
                thread.priority,
                match thread.state {
                    1 => 'R',
                    2 => 'T',
                    3 => 'S',
                    4 => 'U',
                    _ => '?',
                }
            );
        }
        self.emit(line);
        self.threads = threads.iter().map(|t| (t.id, t.cpu)).collect();

        let system = imp::system();
        if let Some(before) = self.system {
            self.emit(system_line(at, &system, &before));
        }
        self.system = Some(system);

        let looped = imp::loop_totals();
        let busy = looped.busy.saturating_sub(self.looped.busy) as f64;
        let asleep = looped.asleep.saturating_sub(self.looped.asleep) as f64;
        let mut line = format!(
            "SB {at:.4} occupe_pct {:.1} endormi_pct {:.1} reveils {} plus_long_ms {:.1} \
             petits_retards {} echantillons {} endormis {} retards_racontes {} \
             diag_retards {} diag_retard_max_ms {:.1}",
            busy / 1e9 / wall * 100.0,
            asleep / 1e9 / wall * 100.0,
            looped.wakeups.saturating_sub(self.looped.wakeups),
            imp::take_longest_busy() as f64 / 1e6,
            self.small.count,
            self.small.samples,
            self.small.asleep,
            self.full,
            self.oversleeps,
            self.oversleep_max.as_secs_f64() * 1000.0,
        );
        if !self.modes.is_empty() {
            let _ = write!(line, " modes {}", self.modes.join(","));
        }
        self.emit(line);
        self.looped = looped;
        self.oversleeps = 0;
        self.oversleep_max = Duration::ZERO;
        self.modes.clear();

        let small = std::mem::take(&mut self.small);
        let mut stacks: Vec<(Vec<usize>, u32)> = small.stacks.into_iter().collect();
        stacks.sort_by(|a, b| b.1.cmp(&a.1).then(a.0.cmp(&b.0)));
        for (frames, count) in stacks.iter().take(4) {
            let text = self.stack(frames);
            self.emit(format!("BPP {count} {text}"));
        }

        let allocs = alloc_counts();
        self.emit(alloc_line(at, &allocs, &self.allocs));
        self.allocs = allocs;
    }
}

/// Adresses consécutives identiques fondues (le registre de lien répète
/// souvent la première adresse de retour).
fn dedup(frames: &[usize]) -> Vec<usize> {
    let mut out: Vec<usize> = Vec::with_capacity(frames.len());
    for &frame in frames {
        if out.last() != Some(&frame) {
            out.push(frame);
        }
    }
    out
}

fn thread_label(thread: &ThreadStat) -> String {
    let name: String = thread
        .name
        .chars()
        .map(|c| {
            if c.is_whitespace() || c == '=' {
                '_'
            } else {
                c
            }
        })
        .collect();
    if name.is_empty() {
        format!("fil{}", thread.id % 100_000)
    } else {
        name
    }
}

const MIB: f64 = 1024.0 * 1024.0;

fn process_line(at: f64, now: &ProcStats, before: &ProcStats, wall: f64) -> String {
    let d = |a: u64, b: u64| a.saturating_sub(b);
    let ms = |ns: u64| ns as f64 / 1e6;
    let cpu = d(now.user + now.system, before.user + before.system);
    let mut out = format!(
        "SP {at:.4} cpu_pct {:.1} cpu_ms {:.0} utilisateur_ms {:.0} systeme_ms {:.0}",
        cpu as f64 / 1e9 / wall * 100.0,
        ms(cpu),
        ms(d(now.user, before.user)),
        ms(d(now.system, before.system)),
    );
    if let (Some(p), Some(q)) = (now.performance, before.performance) {
        let share = if cpu > 0 {
            d(p, q) as f64 / cpu as f64 * 100.0
        } else {
            0.0
        };
        let _ = write!(out, " coeurs_perf_pct {share:.0}");
    }
    if let (Some(e), Some(f)) = (now.energy_nj, before.energy_nj) {
        let joules = d(e, f) as f64 / 1e9;
        let _ = write!(
            out,
            " energie_mj {:.1} puissance_mw {:.1}",
            joules * 1000.0,
            joules / wall * 1000.0
        );
    }
    let instructions = d(now.instructions, before.instructions);
    let cycles = d(now.cycles, before.cycles);
    let _ = write!(
        out,
        " instr_m {:.1} cycles_m {:.1} ipc {:.2} attente_coeur_ms {:.1} \
         reveils {} reveils_inactif {} disque_lu_ko {:.0} disque_ecrit_ko {:.0} ecritures_ko {:.0} \
         fautes {} pageins {} cow {} csw {} appels_mach {} appels_unix {} msg_env {} msg_rec {} \
         fils {} en_cours {} fd {} empreinte_mo {:.1} pic_periode_mo {:.1} pic_vie_mo {:.1}",
        instructions as f64 / 1e6,
        cycles as f64 / 1e6,
        if cycles > 0 {
            instructions as f64 / cycles as f64
        } else {
            0.0
        },
        ms(d(now.runnable, before.runnable)),
        d(now.interrupt_wakeups, before.interrupt_wakeups),
        d(now.idle_wakeups, before.idle_wakeups),
        d(now.disk_read, before.disk_read) as f64 / 1024.0,
        d(now.disk_written, before.disk_written) as f64 / 1024.0,
        d(now.logical_writes, before.logical_writes) as f64 / 1024.0,
        d(now.faults, before.faults),
        d(now.pageins, before.pageins),
        d(now.cow_faults, before.cow_faults),
        d(now.csw, before.csw),
        d(now.mach_calls, before.mach_calls),
        d(now.unix_calls, before.unix_calls),
        d(now.messages_sent, before.messages_sent),
        d(now.messages_received, before.messages_received),
        now.threads,
        now.running,
        now.fds,
        now.footprint as f64 / MIB,
        now.interval_max_footprint as f64 / MIB,
        now.lifetime_max_footprint as f64 / MIB,
    );
    let names = [
        "qos_ui",
        "qos_init",
        "qos_def",
        "qos_util",
        "qos_fond",
        "qos_maint",
        "qos_anc",
    ];
    for (i, name) in names.iter().enumerate() {
        let _ = write!(out, " {name}_ms {:.0}", ms(d(now.qos[i], before.qos[i])));
    }
    out
}

fn system_line(at: f64, now: &SystemStats, before: &SystemStats) -> String {
    let ticks: Vec<u64> = (0..4)
        .map(|i| now.cpu_ticks[i].saturating_sub(before.cpu_ticks[i]))
        .collect();
    let total = ticks.iter().sum::<u64>().max(1) as f64;
    let pct = |t: u64| t as f64 / total * 100.0;
    let pages = |n: u64| (n * now.page_size) as f64 / MIB;
    let mut out = format!(
        "SS {at:.4} cpu_machine_pct {:.1} utilisateur_pct {:.1} systeme_pct {:.1} \
         charge {:.2} {:.2} {:.2}",
        100.0 - pct(ticks[2]),
        pct(ticks[0] + ticks[3]),
        pct(ticks[1]),
        now.load[0],
        now.load[1],
        now.load[2],
    );
    if let Some(level) = now.pressure {
        let _ = write!(out, " pression {level}");
    }
    let _ = write!(
        out,
        " libre_mo {:.0} actif_mo {:.0} inactif_mo {:.0} cablee_mo {:.0} compressee_mo {:.0} \
         echange_mo {:.0} pageins {} pageouts {} swapins {} swapouts {} decompressions {}",
        pages(now.free),
        pages(now.active),
        pages(now.inactive),
        pages(now.wired),
        pages(now.compressed),
        now.swap_used as f64 / MIB,
        now.pageins.saturating_sub(before.pageins),
        now.pageouts.saturating_sub(before.pageouts),
        now.swapins.saturating_sub(before.swapins),
        now.swapouts.saturating_sub(before.swapouts),
        now.decompressions.saturating_sub(before.decompressions),
    );
    if let Some(thermal) = now.thermal {
        let _ = write!(out, " thermique {thermal}");
    }
    if let Some(low) = now.low_power {
        let _ = write!(out, " economie {}", u8::from(low));
    }
    out
}

fn alloc_line(at: f64, now: &AllocCounts, before: &AllocCounts) -> String {
    format!(
        "SA {at:.4} allocs {} reallocs {} liberations {} alloue_mo {:.1} vivant_mo {:.1} \
         pic_mo {:.1} principal_allocs {} principal_ko {:.0}",
        now.allocs.saturating_sub(before.allocs),
        now.reallocs.saturating_sub(before.reallocs),
        now.frees.saturating_sub(before.frees),
        now.bytes.saturating_sub(before.bytes) as f64 / MIB,
        now.live as f64 / MIB,
        now.peak as f64 / MIB,
        now.main_allocs.saturating_sub(before.main_allocs),
        now.main_bytes.saturating_sub(before.main_bytes) as f64 / 1024.0,
    )
}

#[cfg(not(target_os = "macos"))]
mod imp {
    //! Ailleurs que sur macOS : ni pile du fil principal, ni comptes du
    //! système (les lignes de l'essai restent, avec moins de détails).
    use super::{LoopState, LoopTotals, ProcStats, SystemStats, ThreadStat};

    pub fn register_main_thread() {}
    pub fn register_frame_thread() {}
    pub fn raise_thread_priority() {}

    pub fn sample_main(_buffer: &mut [usize]) -> usize {
        0
    }

    pub fn symbolize(address: usize) -> String {
        format!("{address:#x}")
    }

    pub fn loop_state() -> LoopState {
        LoopState::Unknown
    }

    pub fn loop_totals() -> LoopTotals {
        LoopTotals::default()
    }

    pub fn take_longest_busy() -> u64 {
        0
    }

    pub fn take_mode_changes() -> Vec<(u64, String)> {
        Vec::new()
    }

    pub fn current_mode() -> String {
        String::new()
    }

    pub fn process() -> Option<ProcStats> {
        let usage = crate::process_usage()?;
        let cpu = u64::try_from(usage.cpu.as_nanos()).unwrap_or(u64::MAX);
        Some(ProcStats {
            user: cpu,
            footprint: usage.resident,
            ..ProcStats::default()
        })
    }

    pub fn threads() -> Vec<ThreadStat> {
        Vec::new()
    }

    pub fn system() -> SystemStats {
        SystemStats::default()
    }

    pub fn system_info() -> Vec<String> {
        vec![format!(
            "SYSTEME {} {}",
            std::env::consts::OS,
            std::env::consts::ARCH
        )]
    }

    pub fn observe_system(_sink: Box<dyn Fn(String)>) {}

    pub fn media_time() -> f64 {
        super::epoch().elapsed().as_secs_f64()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn process_line_gives_shares_and_power() {
        let before = ProcStats {
            user: 1_000_000_000,
            system: 500_000_000,
            performance: Some(1_000_000_000),
            energy_nj: Some(1_000_000_000),
            instructions: 1_000_000,
            cycles: 2_000_000,
            ..ProcStats::default()
        };
        let now = ProcStats {
            user: 2_000_000_000,
            system: 1_500_000_000,
            performance: Some(2_500_000_000),
            energy_nj: Some(3_000_000_000),
            instructions: 5_000_000,
            cycles: 4_000_000,
            footprint: 64 << 20,
            threads: 12,
            ..ProcStats::default()
        };
        let line = process_line(10.0, &now, &before, 10.0);
        assert!(
            line.starts_with("SP 10.0000 cpu_pct 20.0 cpu_ms 2000"),
            "{line}"
        );
        assert!(line.contains("coeurs_perf_pct 75"), "{line}");
        assert!(
            line.contains("energie_mj 2000.0 puissance_mw 200.0"),
            "{line}"
        );
        assert!(line.contains("ipc 2.00"), "{line}");
        assert!(line.contains("fils 12"), "{line}");
        assert!(line.contains("empreinte_mo 64.0"), "{line}");
    }

    #[test]
    fn system_line_gives_the_machine_load() {
        let before = SystemStats {
            cpu_ticks: [100, 50, 800, 0],
            page_size: 16384,
            ..SystemStats::default()
        };
        let now = SystemStats {
            cpu_ticks: [130, 60, 860, 0],
            page_size: 16384,
            free: 1024,
            pressure: Some(1),
            thermal: Some(0),
            low_power: Some(false),
            ..SystemStats::default()
        };
        let line = system_line(1.0, &now, &before);
        // 40 tops occupés sur 100.
        assert!(line.contains("cpu_machine_pct 40.0"), "{line}");
        assert!(line.contains("pression 1"), "{line}");
        assert!(line.contains("libre_mo 16"), "{line}");
        assert!(line.ends_with("thermique 0 economie 0"), "{line}");
    }

    #[test]
    fn repeated_return_addresses_are_merged() {
        assert_eq!(dedup(&[1, 2, 2, 3, 3, 3, 1]), vec![1, 2, 3, 1]);
    }

    #[test]
    fn a_stall_samples_often_then_less() {
        let mut stall = Stall {
            cause: Cause::Frame,
            seq: 0,
            from: 0,
            last_frame: 0,
            samples: Vec::new(),
            last_sample: 0,
        };
        assert!(stall.wants_sample(0));
        for _ in 0..200 {
            stall.samples.push(Sample {
                frames: vec![1],
                asleep: true,
            });
        }
        stall.last_sample = 100_000_000;
        assert!(!stall.wants_sample(110_000_000));
        assert!(stall.wants_sample(120_000_000));
    }

    #[test]
    fn frames_set_and_clear_the_deadline() {
        frame_started();
        assert_eq!(EXPECT_BY.load(Relaxed), u64::MAX);
        expect_frame(Duration::from_millis(8));
        let by = EXPECT_BY.load(Relaxed);
        assert!(by != u64::MAX && by >= now_ns());
        assert_eq!(TOLERANCE.load(Relaxed), 4_000_000);
    }
}
