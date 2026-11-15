//! Essais de fluidité : `MOUFLON_ESSAI=180` lance Mouflon pour 180 secondes
//! et écrit un journal complet (`MOUFLON_JOURNAL`, par défaut
//! `mouflon-essai.log` dans le dossier courant), lu par
//! `tools/essai/analyse.py`. Le plus simple : `tools/essai/Essai de 3
//! minutes.command`, qui observe aussi le système autour.
//!
//! Options : `MOUFLON_ESSAI_ANIMAUX=southdown,southdown,merinos-d-arles`
//! (animaux du lancement, plusieurs fois le même si on veut) et
//! `MOUFLON_ESSAI_COLLISIONS=1` (mode collisions pendant l'essai).
//!
//! # Le journal
//!
//! Une ligne par chose notée ; le premier mot dit laquelle, le deuxième est
//! l'heure, en secondes depuis le début de l'essai (sauf l'en-tête).
//!
//! - En-tête : `ESSAI`, `VERSION`, `DEBUT` (heure du Mac, pour caler les
//!   relevés faits à côté), `SYSTEME`, `PROCESSUS`, `ECRAN_DETAIL`,
//!   `REGLAGES`, `GPU`, `ECRAN`, `PRECHARGEMENT`.
//! - `F` : une image. `ecart` depuis la précédente et `prep` (temps de
//!   préparation), en ms ; `clk` (réveillée par l'horloge de l'écran) ou
//!   `tmr` (par la minuterie) ; `cad` (images par seconde voulues), `ecr`
//!   (durée d'une image de l'écran). Pour `clk` : `vs` (de la
//!   synchronisation verticale à l'appel de l'horloge), `q` (de l'appel au
//!   début de l'image), `mv` (rafraîchissements sautés), `co` (battements
//!   fondus, arrivés avant que le précédent soit traité). Pour `tmr` :
//!   `tard` (retard de la minuterie). Puis le temps de chaque partie :
//!   `rel` relevés, `sim` simulation, `son` sons, `fen` fenêtres ouvertes ou
//!   fermées, `dep` fenêtres déplacées (`dmax` : le plus long déplacement),
//!   `cur` zones cliquables, `des` dessin, `att` attente d'une image d'écran
//!   (`amax` : la plus longue, `rate` : images refusées), `env` envoi, `r`
//!   fenêtres redessinées ; `al` et `ko` : allocations du fil principal ;
//!   `ev` : événements des fenêtres reçus depuis l'image précédente. Enfin,
//!   après des `|`, chaque animal : numéro, comportement, image, position,
//!   vitesse, appui (`s` sol, `a` air, `t` tenu), roulis.
//! - `E` : un événement (collision, son, fenêtre, menu, notification du
//!   système, mode de la boucle d'événements, gestionnaire lent…).
//! - `B` : un retard ou un blocage du fil principal (voir
//!   `mouflon_platform::diag`), suivi de ses piles les plus vues (`BP`) ;
//!   `BD` : le fil des diagnostics lui-même en retard (tout le processus
//!   attendait un cœur).
//! - Toutes les dix secondes : `S` (images, à-coups, parties, mémoire,
//!   processeur), `SH` (horloge de l'écran, événements, gestionnaires), et
//!   du fil des diagnostics `SP` (processus), `SF` (chaque fil), `SS`
//!   (système), `SB` (boucle d'événements, petits retards et leurs piles
//!   `BPP`), `SA` (allocations), et du fil de relevé `SW` (relevés des
//!   fenêtres).
//! - `PANIQUE` : l'app a planté (message et endroit).
//! - `BILAN` : le bilan de fin.

use std::collections::{BTreeMap, HashMap};
use std::fmt::Write as _;
use std::io::Write as _;
use std::path::PathBuf;
use std::sync::Arc;
use std::sync::atomic::{AtomicU32, Ordering};
use std::sync::mpsc::{Receiver, RecvTimeoutError, Sender, channel};
use std::thread::JoinHandle;
use std::time::{Duration, Instant};

use mouflon_platform as platform;

/// Durée d'essai demandée (variable d'environnement `MOUFLON_ESSAI`, en secondes).
#[must_use]
pub fn trial() -> Option<Duration> {
    let seconds: f64 = std::env::var("MOUFLON_ESSAI").ok()?.trim().parse().ok()?;
    (seconds.is_finite() && seconds > 0.0).then(|| Duration::from_secs_f64(seconds))
}

/// Vitrine de la CI (`MOUFLON_VITRINE=dossier`) : la fenêtre des réglages
/// s'ouvre, son image est enregistrée dans ce dossier, puis l'app quitte.
#[must_use]
pub fn showcase() -> Option<std::path::PathBuf> {
    let dir = std::env::var_os("MOUFLON_VITRINE")?;
    (!dir.is_empty()).then(|| std::path::PathBuf::from(dir))
}

/// Animaux du lancement demandés pour l'essai (identifiants).
#[must_use]
pub fn trial_pets() -> Vec<String> {
    std::env::var("MOUFLON_ESSAI_ANIMAUX")
        .unwrap_or_default()
        .split(',')
        .map(str::trim)
        .filter(|id| !id.is_empty())
        .map(str::to_owned)
        .collect()
}

/// L'essai met-il le mode collisions ?
#[must_use]
pub fn trial_collisions() -> bool {
    std::env::var("MOUFLON_ESSAI_COLLISIONS").is_ok_and(|v| v.trim() == "1")
}

/// Le rythme des voyages d'un écran à l'autre, pour un essai ou pour les
/// montrer (`MOUFLON_VOYAGES=5,15` : à chaque animal, l'envie de changer
/// d'écran revient toutes les 5 à 15 secondes ; un seul nombre : entre ce
/// temps et le double). Sans cela, le rythme ordinaire
/// (`mouflon_core::VOYAGE_EVERY`).
#[must_use]
pub fn voyages() -> Option<(f32, f32)> {
    parse_voyages(&std::env::var("MOUFLON_VOYAGES").ok()?)
}

fn parse_voyages(text: &str) -> Option<(f32, f32)> {
    let mut parts = text.split(',').map(|part| part.trim().parse::<f32>().ok());
    let first = parts.next()??;
    let last = match parts.next() {
        Some(last) => last?,
        None => first * 2.0,
    };
    (first.is_finite() && last.is_finite() && first > 0.0 && last >= first).then_some((first, last))
}

/// Essai de charge (`MOUFLON_ESSAI_CHARGE=1`) : des animaux ajoutés par
/// paliers pendant l'essai (voir [`LOAD_STAGES`]).
#[must_use]
pub fn trial_load() -> bool {
    std::env::var("MOUFLON_ESSAI_CHARGE").is_ok_and(|v| v.trim() == "1")
}

/// Les paliers d'un essai de charge : à quelle fraction de l'essai, et
/// combien d'animaux en tout.
pub const LOAD_STAGES: [(f32, usize); 4] = [(0.1, 20), (0.3, 100), (0.5, 500), (0.7, 1000)];

/// Animaux détaillés dans chaque ligne d'image du journal (les premiers
/// arrivés) : au-delà, le journal grossirait de plusieurs mégaoctets par
/// seconde.
pub const DETAILED_PETS: usize = 32;

/// Temps passé dans chaque partie d'une image.
#[derive(Clone, Copy, Debug, Default)]
pub struct Phases {
    /// Relevés appliqués (écrans, fenêtres, Dock, dossier des animaux).
    pub watch: Duration,
    /// Simulation des animaux (et prise à la souris).
    pub sim: Duration,
    /// Sons lancés (et événements des animaux).
    pub sounds: Duration,
    /// Fenêtres ouvertes ou fermées.
    pub windows: Duration,
    /// Calques mis à jour (ou fenêtres déplacées).
    pub moves: Duration,
    /// Dessin : listes d'affichage, morceaux, images décompressées.
    pub paint: Duration,
    /// Attente d'une image d'écran libre (GPU).
    pub acquire: Duration,
    /// Envoi au compositeur : la transaction des calques (ou le rendu GPU).
    pub present: Duration,
    /// Animaux redessinés.
    pub renders: u32,
    /// Zones cliquables des fenêtres changées (au gré du curseur).
    pub hittest: Duration,
    /// Le plus long déplacement d'une fenêtre.
    pub worst_move: Duration,
    /// La plus longue attente d'une image d'écran.
    pub worst_acquire: Duration,
    /// Images d'écran refusées (surface à refaire, perdue…).
    pub misses: u32,
}

impl Phases {
    fn add(&mut self, other: &Self) {
        self.watch += other.watch;
        self.sim += other.sim;
        self.sounds += other.sounds;
        self.windows += other.windows;
        self.moves += other.moves;
        self.paint += other.paint;
        self.acquire += other.acquire;
        self.present += other.present;
        self.renders += other.renders;
        self.hittest += other.hittest;
        self.worst_move = self.worst_move.max(other.worst_move);
        self.worst_acquire = self.worst_acquire.max(other.worst_acquire);
        self.misses += other.misses;
    }

    /// Forme courte pour le journal : millisecondes de chaque partie.
    fn compact(&self) -> String {
        let ms = |d: Duration| d.as_secs_f32() * 1000.0;
        let mut out = format!(
            "rel {:.2} sim {:.2} son {:.2} fen {:.2} dep {:.2} des {:.2} att {:.2} env {:.2} r {} \
             cur {:.2} dmax {:.2} amax {:.2}",
            ms(self.watch),
            ms(self.sim),
            ms(self.sounds),
            ms(self.windows),
            ms(self.moves),
            ms(self.paint),
            ms(self.acquire),
            ms(self.present),
            self.renders,
            ms(self.hittest),
            ms(self.worst_move),
            ms(self.worst_acquire),
        );
        if self.misses > 0 {
            let _ = write!(out, " rate {}", self.misses);
        }
        out
    }
}

impl std::fmt::Display for Phases {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let ms = |d: Duration| d.as_secs_f32() * 1000.0;
        write!(
            f,
            "relevés {:.1}, simulation {:.1}, sons {:.1}, fenêtres {:.1}, déplacements {:.1}, \
             zones cliquables {:.1}, dessin {:.1}, attente de l'écran {:.1}, envoi {:.1} \
             ({} rendu(s))",
            ms(self.watch),
            ms(self.sim),
            ms(self.sounds),
            ms(self.windows),
            ms(self.moves),
            ms(self.hittest),
            ms(self.paint),
            ms(self.acquire),
            ms(self.present),
            self.renders,
        )
    }
}

/// Ce que l'horloge de l'écran dit d'une image (millisecondes).
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct BeatTiming {
    /// De la synchronisation verticale à l'appel de l'horloge : le retard
    /// du fil principal à prendre le battement.
    pub latency: f32,
    /// De l'appel de l'horloge au début de l'image : le passage par la
    /// boucle d'événements.
    pub queue: f32,
    /// Rafraîchissements de l'écran sautés depuis le battement précédent.
    pub missed: u32,
    /// Battements arrivés avant que le précédent ait été traité (fondus).
    pub coalesced: u32,
}

/// Une image, pour le journal.
#[derive(Clone, Copy, Debug)]
pub struct FrameRecord {
    /// Quand on a commencé à la préparer.
    pub started: Instant,
    /// Réveillée par l'horloge de l'écran : ce qu'elle en dit.
    pub beat: Option<BeatTiming>,
    /// Réveillée par la minuterie : son retard sur l'heure prévue (ms).
    pub timer_late: Option<f32>,
    /// Images par seconde voulues pour elle (0 : l'horloge était arrêtée,
    /// rien ne bougeait).
    pub cadence: f64,
    /// Durée d'une image de l'écran, en ms (0 : inconnue).
    pub refresh: f64,
    pub phases: Phases,
    /// Allocations du fil principal pendant l'image, et leurs octets.
    pub allocs: (u64, u64),
    /// Événements des fenêtres reçus depuis l'image précédente.
    pub events: u32,
}

/// Ce que la boucle d'événements a reçu depuis le dernier résumé.
#[derive(Debug, Default)]
pub struct Counters {
    pub cursor_moves: AtomicU32,
    pub clicks: AtomicU32,
    /// Le curseur entre dans la fenêtre d'un écran.
    pub hovers: AtomicU32,
    pub redraws: AtomicU32,
    /// Autres événements des fenêtres (déplacées, redimensionnées…).
    pub other_window: AtomicU32,
    /// Battements de l'horloge de l'écran reçus.
    pub beats: AtomicU32,
    /// Autres événements de l'app (menus, réglages, dessins prêts…).
    pub user: AtomicU32,
    /// Passages de la boucle avant de s'endormir.
    pub waits: AtomicU32,
    /// Gestionnaires lents (hors images) et le plus lent, en µs.
    pub slow_handlers: AtomicU32,
    pub slowest_handler_us: AtomicU32,
    /// Images décompressées pour les calques de la scène, et oubliées.
    pub images_decoded: AtomicU32,
    pub images_evicted: AtomicU32,
}

impl Counters {
    /// Un de plus.
    pub fn bump(counter: &AtomicU32) {
        counter.fetch_add(1, Ordering::Relaxed);
    }

    /// Le compte, remis à zéro.
    fn take(counter: &AtomicU32) -> u32 {
        counter.swap(0, Ordering::Relaxed)
    }
}

/// De quoi écrire au journal d'un essai depuis un autre fil que celui des
/// images (le fil principal : menus, réglages, gestionnaires lents), et
/// compter ses événements.
#[derive(Clone, Debug)]
pub struct Notes {
    sender: Option<Sender<String>>,
    begun: Instant,
    pub counters: Arc<Counters>,
}

impl Notes {
    /// Note ce qui arrive, avec l'heure depuis le début de l'essai.
    pub fn note(&self, what: impl std::fmt::Display) {
        let line = format!("E {:.4} {what}", self.begun.elapsed().as_secs_f32());
        match &self.sender {
            Some(sender) => {
                let _ = sender.send(line);
            }
            None => log::info!("{line}"),
        }
    }

    /// Une ligne telle quelle.
    pub fn raw(&self, line: impl Into<String>) {
        let line = line.into();
        match &self.sender {
            Some(sender) => {
                let _ = sender.send(line);
            }
            None => log::info!("{line}"),
        }
    }

    #[must_use]
    pub fn begun(&self) -> Instant {
        self.begun
    }

    #[must_use]
    pub fn sender(&self) -> Option<Sender<String>> {
        self.sender.clone()
    }
}

/// Un animal à une image : ce que le journal en garde.
#[derive(Clone, Debug)]
pub struct PetLine<'a> {
    pub id: u32,
    pub name: &'a str,
    pub behavior: &'a str,
    pub frame: Option<u32>,
    pub x: f32,
    pub y: f32,
    pub vx: f32,
    pub vy: f32,
    /// `s` au sol, `a` en l'air, `t` tenu.
    pub support: char,
    pub roll: f32,
}

/// Une image lente : quand (secondes depuis le début), combien (ms), et où.
#[derive(Clone, Copy, Debug)]
struct Slow {
    at: f32,
    work: f32,
    phases: Phases,
}

/// Images préparées en plus de ce temps : notées en détail.
const SLOW_MS: f32 = 12.0;
/// Images lentes gardées pour le bilan (les pires).
const SLOW_KEPT: usize = 12;
/// Un résumé dans le journal toutes les…
const SUMMARY_EVERY: Duration = Duration::from_secs(10);

/// Par comportement : temps passé, images dessinées, images de la séquence.
#[derive(Clone, Copy, Debug, Default)]
struct BehaviorStats {
    time: f32,
    frames: u32,
    changes: u32,
    worst_gap: f32,
}

/// Écarts entre les images, temps passé à les préparer, et journal.
#[derive(Debug)]
pub struct FrameStats {
    begun: Instant,
    /// Écart avec l'image précédente, en millisecondes.
    gaps: Vec<f32>,
    /// Temps de préparation de chaque image, en millisecondes.
    work: Vec<f32>,
    /// Images et plus grand écart, seconde par seconde.
    seconds: Vec<(u32, f32)>,
    slow: Vec<Slow>,
    /// Images données par l'horloge de l'écran (les autres : la minuterie).
    beats: usize,
    last: Option<Instant>,
    /// Comportement et image de chaque animal à la dernière image.
    seen: HashMap<u32, (String, Option<u32>)>,
    behaviors: BTreeMap<String, BehaviorStats>,
    journal: Option<Journal>,
    /// Depuis le dernier résumé.
    period: Period,
    /// Images des animaux sur le GPU : nombre et octets.
    textures: (usize, usize),
    /// Octets des images des animaux à l'écran, en mémoire.
    images: usize,
    /// Ressources du processus au résumé précédent.
    usage: Option<(Instant, platform::ProcessUsage)>,
    /// Ce que la boucle d'événements a reçu depuis le dernier résumé
    /// (comptés aussi depuis le fil principal).
    pub counters: Arc<Counters>,
}

#[derive(Debug, Default)]
struct Period {
    started: Option<Instant>,
    frames: u32,
    worst_gap: f32,
    worst_work: f32,
    late: u32,
    phases: Phases,
    /// Images réveillées par l'horloge, battements fondus, rafraîchissements
    /// sautés, plus grands retards d'appel et de passage (ms).
    beats: u32,
    coalesced: u32,
    missed: u32,
    worst_latency: f32,
    worst_queue: f32,
    worst_timer: f32,
    allocs: u64,
    alloc_bytes: u64,
}

impl FrameStats {
    #[must_use]
    pub fn new(begun: Instant) -> Self {
        Self {
            begun,
            gaps: Vec::new(),
            work: Vec::new(),
            seconds: Vec::new(),
            slow: Vec::new(),
            beats: 0,
            last: None,
            seen: HashMap::new(),
            behaviors: BTreeMap::new(),
            journal: None,
            period: Period::default(),
            textures: (0, 0),
            images: 0,
            // Point de départ du temps de calcul du premier résumé.
            usage: platform::process_usage().map(|usage| (begun, usage)),
            counters: Arc::new(Counters::default()),
        }
    }

    /// De quoi écrire au journal et compter ses événements depuis le fil
    /// principal.
    #[must_use]
    pub fn notes(&self) -> Notes {
        Notes {
            sender: self.sender(),
            begun: self.begun,
            counters: Arc::clone(&self.counters),
        }
    }

    /// De quoi écrire dans le journal depuis un autre fil (diagnostics,
    /// relevé des fenêtres), s'il y en a un.
    #[must_use]
    pub fn sender(&self) -> Option<Sender<String>> {
        self.journal.as_ref().map(|j| j.sender.clone())
    }

    /// Début de l'essai.
    #[must_use]
    pub fn begun(&self) -> Instant {
        self.begun
    }

    /// Avec un journal complet : le fichier `MOUFLON_JOURNAL`, ou
    /// `mouflon-essai.log` dans le dossier courant.
    #[must_use]
    pub fn with_journal(self) -> Self {
        let path = std::env::var_os("MOUFLON_JOURNAL")
            .map_or_else(|| PathBuf::from("mouflon-essai.log"), PathBuf::from);
        self.with_journal_at(path)
    }

    #[must_use]
    pub fn with_journal_at(mut self, path: PathBuf) -> Self {
        self.journal = Journal::open(path);
        self
    }

    /// Secondes depuis le début de l'essai.
    fn at(&self) -> f32 {
        self.begun.elapsed().as_secs_f32()
    }

    /// Note ce qui arrive pendant l'essai, avec l'heure depuis son début : on
    /// retrouve ainsi ce qui accompagne une image lente.
    pub fn note(&self, what: impl std::fmt::Display) {
        let line = format!("E {:.4} {what}", self.at());
        match &self.journal {
            Some(journal) => journal.write(line),
            None => log::info!("{line}"),
        }
    }

    /// Une ligne telle quelle dans le journal (en-tête, état du système…).
    pub fn raw(&self, line: impl Into<String>) {
        if let Some(journal) = &self.journal {
            journal.write(line.into());
        }
    }

    /// Note une image (finie maintenant) et l'état de chaque animal à ce moment.
    pub fn record<'a>(&mut self, frame: &FrameRecord, pets: impl Iterator<Item = PetLine<'a>>) {
        let FrameRecord {
            started,
            beat,
            timer_late,
            cadence,
            refresh,
            phases,
            allocs,
            events,
        } = *frame;
        let gap = self
            .last
            .map(|last| started.duration_since(last).as_secs_f32() * 1000.0);
        self.last = Some(started);
        let work = started.elapsed().as_secs_f32() * 1000.0;
        self.work.push(work);
        if beat.is_some() {
            self.beats += 1;
        }
        let at = started.duration_since(self.begun).as_secs_f32();
        let second = at as usize;
        if self.seconds.len() <= second {
            self.seconds.resize(second + 1, (0, 0.0));
        }
        self.seconds[second].0 += 1;
        if let Some(gap) = gap {
            self.gaps.push(gap);
            self.seconds[second].1 = self.seconds[second].1.max(gap);
        }
        if work >= SLOW_MS {
            self.slow.push(Slow { at, work, phases });
            self.slow.sort_by(|a, b| b.work.total_cmp(&a.work));
            self.slow.truncate(SLOW_KEPT);
        }

        // L'image dans le journal : temps, horloge, parties, puis chaque animal.
        let mut line = format!(
            "F {at:.4} ecart {:.2} prep {work:.2} {} cad {cadence:.0} ecr {refresh:.1}",
            gap.unwrap_or(0.0),
            if beat.is_some() { "clk" } else { "tmr" },
        );
        if let Some(beat) = beat {
            let _ = write!(
                line,
                " vs {:.2} q {:.2} mv {} co {}",
                beat.latency, beat.queue, beat.missed, beat.coalesced
            );
        }
        if let Some(late) = timer_late {
            let _ = write!(line, " tard {late:.2}");
        }
        let _ = write!(
            line,
            " {} al {} ko {:.1} ev {events}",
            phases.compact(),
            allocs.0,
            allocs.1 as f64 / 1024.0
        );
        let mut changes = Vec::new();
        for pet in pets {
            let _ = write!(
                line,
                " | {} {} {} {:.1},{:.1} {:.0},{:.0} {} {:.2}",
                pet.id,
                pet.behavior,
                pet.frame.map_or(-1, i64::from),
                pet.x,
                pet.y,
                pet.vx,
                pet.vy,
                pet.support,
                pet.roll,
            );
            let stats = self.behaviors.entry(pet.behavior.to_owned()).or_default();
            stats.frames += 1;
            stats.time += gap.unwrap_or(0.0) / 1000.0;
            stats.worst_gap = stats.worst_gap.max(gap.unwrap_or(0.0));
            match self.seen.get(&pet.id) {
                Some((behavior, frame)) if behavior == pet.behavior => {
                    if *frame != pet.frame {
                        stats.changes += 1;
                    }
                }
                _ => changes.push(format!("{} n° {} : {}", pet.name, pet.id, pet.behavior)),
            }
            self.seen
                .insert(pet.id, (pet.behavior.to_owned(), pet.frame));
        }
        for change in changes {
            self.note(change);
        }
        if let Some(journal) = &self.journal {
            journal.write(line);
        }

        // Résumé de la période.
        let period = &mut self.period;
        let period_start = *period.started.get_or_insert(started);
        period.frames += 1;
        period.worst_gap = period.worst_gap.max(gap.unwrap_or(0.0));
        period.worst_work = period.worst_work.max(work);
        if work >= SLOW_MS {
            period.late += 1;
        }
        period.phases.add(&phases);
        if let Some(beat) = beat {
            period.beats += 1;
            period.coalesced += beat.coalesced;
            period.missed += beat.missed;
            period.worst_latency = period.worst_latency.max(beat.latency);
            period.worst_queue = period.worst_queue.max(beat.queue);
        }
        if let Some(late) = timer_late {
            period.worst_timer = period.worst_timer.max(late);
        }
        period.allocs += allocs.0;
        period.alloc_bytes += allocs.1;
        if started.duration_since(period_start) >= SUMMARY_EVERY {
            let frames = period.frames.max(1);
            let mean = |d: Duration| d.as_secs_f32() * 1000.0 / frames as f32;
            let line = format!(
                "S {at:.4} images {} plus_grand_ecart {:.1} plus_longue_prep {:.1} lentes {} \
                 moyennes rel {:.3} sim {:.3} son {:.3} fen {:.3} dep {:.3} des {:.3} att {:.3} \
                 env {:.3} rendus {} textures_gpu {} mo_gpu {:.1} mo_images {:.1} {}",
                period.frames,
                period.worst_gap,
                period.worst_work,
                period.late,
                mean(period.phases.watch),
                mean(period.phases.sim),
                mean(period.phases.sounds),
                mean(period.phases.windows),
                mean(period.phases.moves),
                mean(period.phases.paint),
                mean(period.phases.acquire),
                mean(period.phases.present),
                period.phases.renders,
                self.textures.0,
                self.textures.1 as f64 / 1e6,
                self.images as f64 / 1e6,
                usage_since_last(&mut self.usage, started),
            );
            let c = &*self.counters;
            let slowest = Counters::take(&c.slowest_handler_us) as f32 / 1000.0;
            let clock = format!(
                "SH {at:.4} horloge {} fondus {} sautes {} appel_max_ms {:.1} passage_max_ms {:.1} \
                 minuterie_max_ms {:.1} dmax {:.1} amax {:.1} rates {} allocs_principal {} \
                 ko_principal {:.0} souris {} clics {} survols {} redessins {} fenetres {} \
                 battements {} app {} attentes {} lents {} plus_lent_ms {:.1} \
                 images_faites {} images_oubliees {}",
                period.beats,
                period.coalesced,
                period.missed,
                period.worst_latency,
                period.worst_queue,
                period.worst_timer,
                period.phases.worst_move.as_secs_f32() * 1000.0,
                period.phases.worst_acquire.as_secs_f32() * 1000.0,
                period.phases.misses,
                period.allocs,
                period.alloc_bytes as f64 / 1024.0,
                Counters::take(&c.cursor_moves),
                Counters::take(&c.clicks),
                Counters::take(&c.hovers),
                Counters::take(&c.redraws),
                Counters::take(&c.other_window),
                Counters::take(&c.beats),
                Counters::take(&c.user),
                Counters::take(&c.waits),
                Counters::take(&c.slow_handlers),
                slowest,
                Counters::take(&c.images_decoded),
                Counters::take(&c.images_evicted),
            );
            *period = Period::default();
            match &self.journal {
                Some(journal) => {
                    journal.write(line);
                    journal.write(clock);
                }
                None => log::info!("{line}"),
            }
        }
    }

    /// Images prêtes à l'affichage (calques ou GPU : nombre, octets), pour
    /// les résumés.
    pub fn set_textures(&mut self, textures: (usize, usize)) {
        self.textures = textures;
    }

    /// Octets des images des animaux à l'écran, en mémoire, pour les résumés.
    pub fn set_images(&mut self, bytes: usize) {
        self.images = bytes;
    }

    #[must_use]
    pub fn report(&self) -> String {
        let gaps = sorted(&self.gaps);
        let work = sorted(&self.work);
        // Écarts par tranche : à 120 Hz, presque tout doit tomber sous 10 ms
        // pendant que les animaux bougent (30 i/s au repos : vers 33 ms).
        let buckets = [10.0, 20.0, 40.0, 60.0];
        let mut counts = [0usize; 5];
        for gap in &gaps {
            let i = buckets.iter().position(|b| gap <= b).unwrap_or(4);
            counts[i] += 1;
        }
        let mut out = format!(
            "{} images ({} sur l'horloge de l'écran) ; écarts : médian {:.1} ms, \
             99 % sous {:.1} ms, max {:.1} ms (≤ 10 ms : {}, ≤ 20 : {}, ≤ 40 : {}, \
             ≤ 60 : {}, au-delà : {}) ; préparation : médiane {:.2} ms, 99 % sous \
             {:.2} ms, max {:.1} ms",
            self.work.len(),
            self.beats,
            percentile(&gaps, 0.5),
            percentile(&gaps, 0.99),
            gaps.last().copied().unwrap_or(0.0),
            counts[0],
            counts[1],
            counts[2],
            counts[3],
            counts[4],
            percentile(&work, 0.5),
            percentile(&work, 0.99),
            work.last().copied().unwrap_or(0.0),
        );
        out.push_str("\n  seconde par seconde (images, plus grand écart en ms) :");
        for (i, (frames, gap)) in self.seconds.iter().enumerate() {
            let _ = write!(out, " {i}:{frames}/{gap:.0}");
        }
        if !self.slow.is_empty() {
            out.push_str("\n  images les plus lentes à préparer :");
            for slow in &self.slow {
                let _ = write!(
                    out,
                    "\n    à {:.2} s : {:.1} ms ({})",
                    slow.at, slow.work, slow.phases
                );
            }
        }
        if !self.behaviors.is_empty() {
            out.push_str(
                "\n  par comportement (temps, images de la séquence par seconde, plus grand écart) :",
            );
            for (name, stats) in &self.behaviors {
                let rate = if stats.time > 0.0 {
                    stats.changes as f32 / stats.time
                } else {
                    0.0
                };
                let _ = write!(
                    out,
                    "\n    {name} : {:.1} s, {rate:.1} images/s, {:.0} ms",
                    stats.time, stats.worst_gap
                );
            }
        }
        out
    }

    /// Fin de l'essai : le bilan au journal (et au fichier, attendu jusqu'au bout).
    pub fn finish(&mut self) -> String {
        let report = self.report();
        if let Some(journal) = self.journal.take() {
            journal.write(format!("BILAN {report}"));
            let path = journal.finish();
            log::info!("journal de l'essai : {}", path.display());
        }
        report
    }
}

/// Une panique de l'app est notée au journal (`PANIQUE`), avant le message
/// habituel ; le fil du journal a le temps de l'écrire.
pub fn install_panic_hook(sender: Sender<String>, begun: Instant) {
    let previous = std::panic::take_hook();
    std::panic::set_hook(Box::new(move |info| {
        let at = begun.elapsed().as_secs_f64();
        let message = info.to_string().replace('\n', " ⏎ ");
        if sender.send(format!("PANIQUE {at:.4} {message}")).is_ok() {
            // Le journal s'écrit au plus tard toutes les 500 ms.
            std::thread::sleep(Duration::from_millis(700));
        }
        previous(info);
    }));
}

/// Ressources du processus depuis le résumé précédent, `last` (lues sans
/// lancer de programme : le fil principal n'attend pas).
fn usage_since_last(last: &mut Option<(Instant, platform::ProcessUsage)>, now: Instant) -> String {
    let Some(usage) = platform::process_usage() else {
        return String::new();
    };
    let line = usage_line(&usage, *last, now);
    *last = Some((now, usage));
    line
}

/// Mémoire (résidente, puis comptée par le système) et, depuis le relevé
/// précédent, part d'un cœur occupée et réveils par seconde.
fn usage_line(
    usage: &platform::ProcessUsage,
    before: Option<(Instant, platform::ProcessUsage)>,
    now: Instant,
) -> String {
    const MIB: f64 = 1024.0 * 1024.0;
    let mut out = format!("memoire_mo {:.0}", usage.resident as f64 / MIB);
    if let Some(footprint) = usage.footprint {
        let _ = write!(out, " empreinte_mo {:.0}", footprint as f64 / MIB);
    }
    if let Some((then, before)) = before {
        let wall = now.duration_since(then).as_secs_f64();
        if wall > 0.0 {
            let cpu = usage.cpu.saturating_sub(before.cpu).as_secs_f64();
            let _ = write!(out, " cpu {:.0}", 100.0 * cpu / wall);
            if let (Some(now), Some(then)) = (usage.wakeups, before.wakeups) {
                let _ = write!(
                    out,
                    " reveils_s {:.0}",
                    now.saturating_sub(then) as f64 / wall
                );
            }
        }
    }
    out
}

/// Fichier du journal, écrit sur un fil à part : l'animation n'attend
/// jamais le disque.
#[derive(Debug)]
struct Journal {
    sender: Sender<String>,
    writer: JoinHandle<()>,
    path: PathBuf,
}

impl Journal {
    fn open(path: PathBuf) -> Option<Self> {
        let (file, path) = match std::fs::File::create(&path) {
            Ok(file) => (file, path),
            Err(e) => {
                // Lancé depuis le Finder, le dossier courant n'est pas à nous.
                let fallback = std::env::temp_dir().join("mouflon-essai.log");
                log::warn!(
                    "journal {} : {e} ; écrit dans {}",
                    path.display(),
                    fallback.display()
                );
                (std::fs::File::create(&fallback).ok()?, fallback)
            }
        };
        let (sender, receiver) = channel();
        let writer = std::thread::Builder::new()
            .name("mouflon-journal".into())
            .spawn(move || write_all(file, &receiver))
            .ok()?;
        log::info!("journal de l'essai : {}", path.display());
        Some(Self {
            sender,
            writer,
            path,
        })
    }

    fn write(&self, line: String) {
        let _ = self.sender.send(line);
    }

    /// Ferme le journal et attend qu'il soit écrit en entier. D'autres fils
    /// (diagnostics, relevé des fenêtres) gardent de quoi y écrire : le fil
    /// du journal s'arrête sur une marque de fin, pas quand plus personne ne
    /// peut écrire ; ce qu'ils envoient ensuite est perdu.
    fn finish(self) -> PathBuf {
        let Self {
            sender,
            writer,
            path,
        } = self;
        let _ = sender.send(END.to_owned());
        let _ = writer.join();
        path
    }
}

/// Marque de fin du journal (jamais dans une ligne : caractère de contrôle).
const END: &str = "\u{4}";

fn write_all(file: std::fs::File, lines: &Receiver<String>) {
    let mut out = std::io::BufWriter::new(file);
    loop {
        match lines.recv_timeout(Duration::from_millis(500)) {
            Ok(line) if line == END => break,
            Ok(line) => {
                let _ = writeln!(out, "{line}");
            }
            Err(RecvTimeoutError::Timeout) => {
                let _ = out.flush();
            }
            Err(RecvTimeoutError::Disconnected) => break,
        }
    }
    let _ = out.flush();
}

fn sorted(values: &[f32]) -> Vec<f32> {
    let mut values = values.to_vec();
    values.sort_by(f32::total_cmp);
    values
}

/// Valeur sous laquelle tombe la part `p` des valeurs triées.
fn percentile(sorted: &[f32], p: f32) -> f32 {
    if sorted.is_empty() {
        return 0.0;
    }
    let i = ((sorted.len() - 1) as f32 * p).round() as usize;
    sorted[i.min(sorted.len() - 1)]
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_pace_of_voyages_is_one_number_or_two() {
        assert_eq!(parse_voyages("5,15"), Some((5.0, 15.0)));
        assert_eq!(parse_voyages(" 2.5 , 4 "), Some((2.5, 4.0)));
        // Un seul nombre : entre ce temps et le double.
        assert_eq!(parse_voyages("8"), Some((8.0, 16.0)));
        // Ce qui ne veut rien dire ne change rien.
        for text in ["", "vite", "0", "-3", "9,4", "5,", "5,x", "nan"] {
            assert_eq!(parse_voyages(text), None, "{text:?}");
        }
    }

    #[test]
    fn usage_line_gives_the_share_of_a_core_and_wakeups_per_second() {
        let start = Instant::now();
        let before = platform::ProcessUsage {
            resident: 100 << 20,
            footprint: Some(80 << 20),
            cpu: Duration::from_millis(500),
            wakeups: Some(1000),
        };
        let after = platform::ProcessUsage {
            resident: 120 << 20,
            footprint: Some(90 << 20),
            cpu: Duration::from_millis(2500),
            wakeups: Some(1500),
        };
        let line = usage_line(
            &after,
            Some((start, before)),
            start + Duration::from_secs(10),
        );
        assert_eq!(line, "memoire_mo 120 empreinte_mo 90 cpu 20 reveils_s 50");
        // Premier relevé, système qui ne donne ni empreinte ni réveils.
        let bare = platform::ProcessUsage {
            footprint: None,
            wakeups: None,
            ..after
        };
        assert_eq!(usage_line(&bare, None, start), "memoire_mo 120");
    }

    fn frame(
        started: Instant,
        beat: bool,
        cadence: f64,
        refresh: f64,
        phases: Phases,
    ) -> FrameRecord {
        FrameRecord {
            started,
            beat: beat.then_some(BeatTiming {
                latency: 0.5,
                queue: 0.25,
                missed: 0,
                coalesced: 0,
            }),
            timer_late: (!beat).then_some(1.5),
            cadence,
            refresh,
            phases,
            allocs: (3, 2048),
            events: 2,
        }
    }

    fn pet(behavior: &str, frame: u32) -> PetLine<'_> {
        PetLine {
            id: 0,
            name: "Laine",
            behavior,
            frame: Some(frame),
            x: 10.0,
            y: 20.0,
            vx: 0.0,
            vy: 0.0,
            support: 's',
            roll: 0.0,
        }
    }

    #[test]
    fn reports_gaps_and_work() {
        let start = Instant::now();
        let mut stats = FrameStats::new(start);
        for i in 0..5u64 {
            stats.record(
                &frame(
                    start + Duration::from_millis(i * 8),
                    i > 0,
                    120.0,
                    8.3,
                    Phases::default(),
                ),
                [pet("marche", i as u32 / 2)].into_iter(),
            );
        }
        let report = stats.report();
        assert!(report.starts_with("5 images (4 sur l'horloge"), "{report}");
        assert!(report.contains("médian 8.0 ms"), "{report}");
        assert!(report.contains("≤ 10 ms : 4"), "{report}");
        assert!(report.contains(" 0:5/8"), "{report}");
        // Deux changements d'image en 32 ms : 62,5 images par seconde.
        assert!(report.contains("marche : 0.0 s, 62.5 images/s"), "{report}");
        assert_eq!(percentile(&[], 0.5), 0.0);
    }

    #[test]
    fn keeps_the_slowest_frames() {
        let start = Instant::now() - Duration::from_secs(1);
        let mut stats = FrameStats::new(start);
        stats.record(
            &frame(
                start,
                false,
                60.0,
                0.0,
                Phases {
                    sounds: Duration::from_millis(900),
                    ..Phases::default()
                },
            ),
            std::iter::empty(),
        );
        let report = stats.report();
        assert!(report.contains("images les plus lentes"), "{report}");
        assert!(report.contains("sons 900.0"), "{report}");
    }

    #[test]
    fn finishing_does_not_wait_for_other_writers() {
        let path = std::env::temp_dir().join(format!("mouflon-fin-{}.log", std::process::id()));
        let mut stats = FrameStats::new(Instant::now()).with_journal_at(path.clone());
        let other = stats.sender().expect("journal ouvert");
        let _ = other.send("SP 1.0 cpu_pct 3".to_owned());
        stats.finish();
        // Un autre fil écrit encore : rien ne bloque, sa ligne est perdue.
        let _ = other.send("SP 2.0 cpu_pct 4".to_owned());
        let text = std::fs::read_to_string(&path).unwrap();
        let _ = std::fs::remove_file(&path);
        assert!(
            text.contains("SP 1.0") && !text.contains("SP 2.0"),
            "{text}"
        );
    }

    #[test]
    fn writes_a_full_journal() {
        let path = std::env::temp_dir().join(format!("mouflon-journal-{}.log", std::process::id()));
        let start = Instant::now();
        let mut stats = FrameStats::new(start).with_journal_at(path.clone());
        stats.raw("ESSAI test");
        stats.record(
            &frame(start, true, 30.0, 8.3, Phases::default()),
            [pet("marche", 3)].into_iter(),
        );
        stats.note("heurt 0 1");
        stats.finish();
        let text = std::fs::read_to_string(&path).unwrap();
        let _ = std::fs::remove_file(&path);
        assert!(text.starts_with("ESSAI test\n"), "{text}");
        assert!(
            text.contains("clk cad 30 ecr 8.3 vs 0.50 q 0.25 mv 0 co 0 rel "),
            "{text}"
        );
        assert!(text.contains(" al 3 ko 2.0 ev 2 | "), "{text}");
        assert!(text.contains("| 0 marche 3 10.0,20.0 0,0 s 0.00"), "{text}");
        assert!(text.contains("E ") && text.contains("heurt 0 1"), "{text}");
        assert!(text.contains("BILAN 1 images"), "{text}");
    }
}
