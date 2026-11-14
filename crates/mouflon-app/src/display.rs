//! L'affichage : le troupeau, le monde, la scène et le rythme des images.
//!
//! Sur Mac, il tourne sur son propre fil, réveillé par l'horloge de l'écran
//! (un `CADisplayLink` inscrit sur la boucle de ce fil) : un menu ouvert, la
//! fenêtre des réglages, une requête d'Accessibilité d'une autre app ou un
//! premier chargement de code sur le fil principal ne retardent plus une
//! image. Le fil principal lui envoie ce que fait l'utilisateur ([`Command`])
//! et reçoit ce qu'il doit montrer dans les menus ([`Report`]).
//!
//! Les demandes sont numérotées dans l'ordre où elles partent ; quand
//! l'affichage décrit le troupeau, il dit combien il en a traité : le fil
//! principal sait ainsi ce que la description sait déjà.
//!
//! Quand la scène passe par le GPU (`MOUFLON_GPU=1`, ou un système sans
//! calques Core Animation), l'affichage reste sur le fil principal, rythmé
//! par la boucle de winit : c'est le même code, avec une autre horloge.

use std::collections::{HashMap, HashSet};
use std::hash::BuildHasherDefault;
use std::sync::Arc;
use std::sync::mpsc::{Receiver, Sender};
use std::thread::JoinHandle;
use std::time::{Duration, Instant};

use chrono::Timelike;
use mouflon_core::show::{Farewell, Outcome, Scene as ShowScene, ShowView, Welcome};
use mouflon_core::{
    Activity, Flock, Input, LedgeOptions, PetDef, PetEvent, PetView, Rect, Screen, Support, TICK,
    Vec2, WindowRect, World, exact_dock,
};
use mouflon_platform as platform;
use mouflon_render::{PetArt, Style};

use crate::scene::{Overlay, Scene};
use crate::stats::FrameStats;
use crate::watch::{Snapshot, Watcher};

/// Au lancement, la scène reste vide un court instant : le temps que macOS
/// affiche les fenêtres et que tout soit en route avant que le premier
/// animal ne tombe.
pub const SETTLE: Duration = Duration::from_millis(300);
/// Sans battement de l'horloge de l'écran depuis deux images et demie
/// (entre ces deux délais), la minuterie prend le relais : un battement qui
/// manque (écran en veille, débranché, horloge qui hoquette) ne coûte qu'une
/// image ou deux. Au-delà du dernier délai, l'écran dort sans doute : on
/// ralentit.
const CLOCK_SILENT_MIN: Duration = Duration::from_millis(25);
const CLOCK_SILENT: Duration = Duration::from_millis(150);
const CLOCK_ASLEEP: Duration = Duration::from_secs(1);
/// Une horloge qui tourne mais ne bat plus depuis ce délai (écran changé,
/// réveil…) est refaite ; pas plus souvent.
const CLOCK_DEAD: Duration = Duration::from_secs(2);
/// Cadences quand les animaux se reposent ou dorment (images par seconde).
const IDLE_FPS: f64 = 30.0;
const ASLEEP_FPS: f64 = 20.0;
/// En foule, la part d'un cœur que la préparation des images s'accorde
/// avant d'espacer les dessins de chacun (voir [`Crowd`]).
const CROWD_SHARE: f64 = 0.25;
/// Les rythmes sous lesquels un animal n'est jamais redessiné, même dans la
/// plus grande foule (images par seconde) : celui qui bouge garde les douze
/// dessins par seconde du dessin animé (« sur deux »), sa place suit
/// l'écran à chaque image.
const CROWD_MOVING_FPS: f64 = 12.0;
const CROWD_IDLE_FPS: f64 = 6.0;
const CROWD_ASLEEP_FPS: f64 = 4.0;
/// L'espacement le plus grand (fois le rythme de chacun).
const CROWD_STRETCH_MAX: f64 = 10.0;
/// Ce qu'une image peut passer à rattraper la simulation après un retard
/// (voir [`SimPace`]).
const SIM_CATCH_UP: f64 = 0.008;
/// Après un mouvement vif, la cadence reste pleine ce temps-là avant de
/// ralentir : elle ne change pas d'allure à chaque rebond qui se calme.
const FULL_RATE_HOLD: Duration = Duration::from_millis(300);
/// Le curseur passe sur un animal : la cadence est pleine ce temps-là, au
/// cas où on l'attraperait.
const HOVER_FULL_RATE: Duration = Duration::from_secs(1);
/// Délai entre deux surprises tirées au hasard (minutes).
const SURPRISE_EVERY: (f32, f32) = (15.0, 40.0);
/// Volume de l'ensemble des sons des animaux (de 0 à 1) : discret, c'est un
/// compagnon de bureau.
const VOLUME: f32 = 0.55;
/// Sons lancés au plus par fenêtre de temps (les suivants se taisent).
const SOUNDS_PER_WINDOW: u32 = 4;
const SOUND_WINDOW: Duration = Duration::from_millis(500);

/// Une surprise (voir `mouflon_core::surprise`).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Surprise {
    Wind,
    Saucer,
}

/// Les réglages qui changent ce que fait l'affichage.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Live {
    pub paused: bool,
    pub sounds: bool,
    pub collisions: bool,
    pub realistic: bool,
    pub surprises: bool,
    pub frontmost_only: bool,
    pub walk_under_dock: bool,
    /// Le tempérament des animaux.
    pub mood: mouflon_core::Mood,
}

/// Un changement de dessin : les animaux dessinés d'après `from` prennent
/// `to` (agrandis ou réduits de `factor`) et `art`.
#[derive(Clone, Debug)]
pub struct Redraw {
    pub from: Arc<PetDef>,
    pub to: Arc<PetDef>,
    pub art: Arc<PetArt>,
    pub factor: f32,
}

/// Ce que le fil principal demande à l'affichage.
#[derive(Debug)]
pub enum Command {
    /// Les écrans tels que le système les donne (avant le Dock), et la
    /// fenêtre de chacun.
    Screens {
        screens: Vec<Screen>,
        overlays: Vec<Overlay>,
    },
    /// Un relevé des fenêtres arrivé avant que l'affichage soit en route.
    Survey(Snapshot),
    /// Un animal arrive, avec son dessin : du sexe demandé s'il l'est (voir
    /// [`mouflon_core::Pet::spawn_as`]) ; `lambs` : si c'est une brebis,
    /// les agneaux qui la suivent, un par agneau, chacun avec sa place s'il
    /// revient d'une mise à jour (sinon, il tombe derrière elle) ; le fil
    /// principal prépare leurs dessins : voir [`PetSummary::lambs`]).
    Spawn {
        def: Arc<PetDef>,
        art: Arc<PetArt>,
        sex: Option<mouflon_core::Sex>,
        lambs: Vec<Option<f32>>,
        /// Sur l'écran où se trouve le curseur (celui que l'utilisateur
        /// regarde : il vient de demander cet animal) ; sinon, sur
        /// n'importe lequel.
        here: bool,
        /// Il revient à cette abscisse (le troupeau d'avant une mise à
        /// jour, voir [`Command::Welcome`]) ; sinon, où le sort le dit.
        place: Option<f32>,
    },
    /// Un jeune dont le dessin a été préparé hors de l’affichage : né
    /// auprès de sa mère posée, ou (`arriving`) tombé du haut de l'écran
    /// derrière elle, qui vient d'arriver.
    SpawnLamb {
        mother: u32,
        def: Arc<PetDef>,
        art: Arc<PetArt>,
        arriving: bool,
        /// Il revient à cette place (le troupeau d'avant une mise à jour).
        place: Option<f32>,
    },
    /// Les premières images se préparent, puis la scène se montre après
    /// une courte mise en route (`settle`) : les animaux arrivent.
    Start {
        settle: Duration,
    },
    /// L'animal sous le curseur s'en va tout de suite (un autre, déposé sur
    /// lui, prend sa place).
    RemoveUnderCursor,
    /// Cet animal dit au revoir.
    Goodbye(u32),
    /// Le dernier arrivé de cette espèce (identifiant de sa définition) dit
    /// au revoir.
    GoodbyeSpecies(String),
    GoodbyeAll,
    Together,
    /// Changements de dessin ; les animaux dessinés d'après une définition
    /// de `gone` (plus dans la bibliothèque) s'en vont.
    Redraw {
        changes: Vec<Redraw>,
        gone: Vec<Arc<PetDef>>,
    },
    /// Cet animal-là change de taille.
    RedrawPet {
        id: u32,
        def: Arc<PetDef>,
        art: Arc<PetArt>,
        factor: f32,
    },
    Live(Live),
    Dark(bool),
    /// La fenêtre de l'écran `key` a changé de taille (GPU).
    Resized {
        key: u64,
        width: u32,
        height: u32,
    },
    /// Bouton gauche appuyé ou relâché sur une fenêtre d'écran (avec la
    /// position du curseur vue par l'événement).
    Press(Option<Vec2>),
    Release(Option<Vec2>),
    /// Le curseur bouge dans une fenêtre d'écran (si le système ne donne
    /// pas sa position).
    Cursor(Vec2),
    /// Le curseur entre dans une fenêtre d'écran : on va peut-être attraper
    /// un animal.
    Hover,
    /// Clic droit (ou Contrôle-clic) : le menu de l'animal sous le curseur.
    Menu,
    MenuClosed,
    /// Une mise à jour est prête : l'annonce commence dès que le troupeau
    /// est visible (voir [`mouflon_core::show::Farewell`]). L'affichage dit
    /// ensuite où elle en est ([`Report::Show`]).
    Farewell(ShowScene),
    /// La mise à jour annoncée est remise à plus tard (le menu).
    Postpone,
    /// Mouflon revient d'une mise à jour : la soucoupe repose le troupeau si
    /// c'est elle qui l'avait pris, et l'un d'eux montre `label` (le numéro
    /// de la version) ; `places` : où reviennent ceux qui reviennent.
    Welcome {
        scene: ShowScene,
        label: String,
        places: Vec<f32>,
    },
    /// Une surprise imposée, à son heure (essai, `MOUFLON_SURPRISE`).
    Surprise(Instant, Surprise),
    /// L'essai finit à cette heure.
    TrialEnd(Instant),
    /// Instantanés de la scène : où, et quand.
    Snapshots(std::path::PathBuf, Vec<Instant>),
    /// Le fil de l'affichage s'arrête (l'app quitte).
    Quit,
}

/// Un animal, pour le fil principal : de quoi faire les menus.
#[derive(Clone, Debug)]
pub struct PetSummary {
    pub id: u32,
    /// Une brebis native, dessinée en vecteurs ou peinte sur un squelette
    /// (voir [`mouflon_core::Pet::can_mother`]) : son menu propose son
    /// agneau. Une brebis en images (bandes d'images, desktopPet) n'en a pas.
    pub is_ewe: bool,
    /// Son espèce (voir [`mouflon_core::Pet::species`]), quel que soit le
    /// dessin qu'il porte : elle le relie à son entrée de la bibliothèque.
    pub def: Arc<PetDef>,
    /// Personnage secondaire ou animal qui part : pas compté dans le menu.
    #[cfg_attr(not(any(target_os = "macos", target_os = "windows")), allow(dead_code))]
    pub extra: bool,
    /// Une brebis tout juste arrivée que ses agneaux suivent : un par
    /// agneau, avec sa place s'il revient d'une mise à jour. Le fil
    /// principal prépare leurs dessins (voir [`Command::SpawnLamb`]).
    pub lambs: Vec<Option<f32>>,
}

/// Un animal du troupeau au moment où il part pour une mise à jour : de
/// quoi le faire revenir.
#[derive(Clone, Debug, PartialEq)]
pub struct Leaver {
    pub id: u32,
    pub x: f32,
    /// Le dessin qu'il porte (bélier, brebis, agneau), s'il en a un à lui.
    pub variant: Option<mouflon_core::Variant>,
    /// Un agneau qui suit sa mère : elle reviendra avec lui.
    pub mother: Option<u32>,
    /// Un agneau (qu'il suive sa mère ou non).
    pub lamb: bool,
}

/// Où en est l'annonce d'une mise à jour.
#[derive(Clone, Debug, PartialEq)]
pub enum ShowReport {
    /// Le compte tourne : on peut encore remettre à plus tard.
    Counting,
    /// Zéro : plus rien n'arrête le départ. Voici le troupeau qui part.
    Leaving(Vec<Leaver>),
    /// Le troupeau est parti : la place est libre.
    Gone,
    /// Remis à plus tard : le troupeau est resté.
    Postponed,
}

/// Ce que l'affichage dit au fil principal.
#[derive(Clone, Debug)]
pub enum Report {
    /// L'annonce d'une mise à jour avance (voir [`Command::Farewell`]).
    Show(ShowReport),
    /// Le troupeau a changé (arrivée, départ, dessin) : sa génération, le
    /// nombre de demandes traitées jusque-là, et ses animaux.
    Flock {
        generation: u64,
        done: u64,
        pets: Vec<PetSummary>,
    },
    /// Les fenêtres des écrans (leur clé) qui prennent la souris (au-dessus
    /// d'un animal, ou pendant qu'on en tient un) ou la laissent passer.
    Hittest(Vec<(u64, bool)>),
    /// Le menu de cet animal (sous le curseur) est à ouvrir.
    PetMenu {
        id: u32,
        def: Arc<PetDef>,
        can_have_lamb: bool,
    },
    /// L'autorisation d'Accessibilité, telle que le relevé la voit.
    Trusted(bool),
    /// L'horloge de l'écran ne bat plus (ou les écrans ont changé) : en
    /// faire une autre.
    NeedClock,
    /// L'essai est fini (son bilan est au journal) : l'app quitte.
    TrialOver,
    /// Le compositeur du système a perdu son appareil graphique (voir
    /// `platform::scene_lost`) : les calques ne montrent plus rien, la
    /// scène est à reposer dans les fenêtres des écrans.
    SceneLost,
}

/// Ce dont l'affichage a besoin d'une horloge de l'écran : sur le fil
/// principal ([`platform::FrameClock`]) ou sur le sien ([`platform::ThreadClock`]).
pub trait Clock {
    /// Elle arrive sur le fil de l'affichage (elle s'y inscrit, s'il le faut).
    fn attach(&self) {}
    fn max_fps(&self) -> f32;
    fn is_running(&self) -> bool;
    fn frame_duration(&self) -> f64;
    fn set_running(&self, running: bool);
    fn set_rate(&self, fps: f32);
}

impl Clock for platform::FrameClock {
    fn max_fps(&self) -> f32 {
        Self::max_fps(self)
    }
    fn is_running(&self) -> bool {
        Self::is_running(self)
    }
    fn frame_duration(&self) -> f64 {
        Self::frame_duration(self)
    }
    fn set_running(&self, running: bool) {
        Self::set_running(self, running);
    }
    fn set_rate(&self, fps: f32) {
        Self::set_rate(self, fps);
    }
}

impl Clock for platform::ThreadClock {
    fn attach(&self) {
        self.start_here();
    }
    fn max_fps(&self) -> f32 {
        Self::max_fps(self)
    }
    fn is_running(&self) -> bool {
        Self::is_running(self)
    }
    fn frame_duration(&self) -> f64 {
        Self::frame_duration(self)
    }
    fn set_running(&self, running: bool) {
        Self::set_running(self, running);
    }
    fn set_rate(&self, fps: f32) {
        Self::set_rate(self, fps);
    }
}

/// Ce que l'affichage montre autour d'une mise à jour.
enum Show {
    Farewell(Farewell),
    Welcome(Welcome),
}

/// L'annonce d'une mise à jour attend au plus ce temps-là qu'un animal qui
/// vient d'arriver soit posé.
const FAREWELL_WAIT: Duration = Duration::from_secs(4);

/// Les numéros, dans la scène, de ce qu'une annonce ajoute au troupeau.
const SHOW_BEAM: u32 = 1;
const SHOW_COUNT: u32 = 2;
const SHOW_BUBBLE: u32 = 3;
/// La soucoupe d'une annonce : un numéro qu'aucune soucoupe en visite ne
/// prend.
const SHOW_SAUCER: u32 = u32::MAX;

#[derive(Debug, Default)]
struct Pointer {
    pressed: Option<Vec2>,
    dragging: Option<u32>,
    /// Dernière position vue par les événements (si le système ne donne
    /// pas la position du curseur).
    cursor: Option<Vec2>,
}

/// Où va ce que dit l'affichage (vers la boucle du fil principal).
pub type Reports = Box<dyn Fn(Report) + Send>;

/// Ce qu'il faut à l'affichage pour commencer.
pub struct Init {
    pub live: Live,
    pub dark: bool,
    /// Le relevé des fenêtres (aucun : pas d'autres fenêtres, essais du code).
    pub watcher: Option<Arc<Watcher>>,
    pub stats: Option<FrameStats>,
    pub reports: Reports,
}

/// L'affichage : tout ce qui fait une image.
pub struct Display<C: Clock> {
    world: World,
    flock: Flock,
    scene: Scene,
    /// Le dessin de chaque animal (son numéro → son dessin).
    arts: HashMap<u32, Arc<PetArt>, BuildHasherDefault<mouflon_core::IdHasher>>,
    /// Le dessin de chaque définition à l'écran : les personnages secondaires
    /// prennent celui de leur espèce (ils en partagent la définition).
    def_arts: Vec<(Arc<PetDef>, Arc<PetArt>)>,
    /// Demandes traitées (voir le début du module), et depuis la dernière image.
    done: u64,
    commands: u32,
    /// Les animaux sont arrivés (`Command::Start`) : les images commencent.
    started: bool,
    /// Les écrans tels que le système les donne (avant le Dock).
    screens: Vec<Screen>,
    live: Live,
    style: Style,
    pointer: Pointer,
    /// La position du curseur demandée au système (sinon, celle des
    /// événements seulement : essais du code).
    system_cursor: bool,
    /// Surprises à venir : quand, et laquelle si elle est imposée ; sinon
    /// tirée au sort.
    surprises: Vec<(Instant, Option<Surprise>)>,
    /// L'annonce d'une mise à jour, ou le retour du troupeau.
    show: Option<Show>,
    /// L'annonce demandée, qui attend que le troupeau soit visible ; et
    /// depuis quand elle attend qu'un animal soit posé.
    farewell_wanted: Option<ShowScene>,
    farewell_since: Option<Instant>,
    /// Le troupeau tel qu'il était au début de l'annonce.
    leavers: Vec<Leaver>,
    /// Ce que le fil principal sait de l'annonce.
    show_reported: Option<ShowReport>,
    /// Les voyages d'un écran à l'autre déjà notés au journal d'un essai
    /// (partis, arrivés).
    voyages: (u32, u32),
    seed: u64,
    watcher: Option<Arc<Watcher>>,
    /// Dernier relevé des fenêtres des autres apps, tel quel.
    os_windows: Vec<platform::OsWindow>,
    /// Dernier cadre du Dock relevé (« Marcher sous le Dock »).
    dock_frame: Option<platform::OsRect>,
    /// Le Dock traité comme une fenêtre posée devant les autres.
    dock: Option<WindowRect>,
    dock_logged: bool,
    /// Panneaux flottants déjà signalés au journal (propriétaire, niveau).
    floating_seen: HashSet<(String, i32)>,
    frontmost: Option<i32>,
    followed: Option<u32>,
    trusted: bool,
    /// Ce que fait l'utilisateur, au dernier relevé, et quand.
    activity: Option<(Instant, platform::UserActivity)>,
    /// Personne devant la machine (essai de la CI).
    no_user: bool,
    /// Dernière note au journal d'une fenêtre qui bouge, par fenêtre.
    window_notes: HashMap<u64, Instant>,
    fast_poll_until: Instant,
    /// L'horloge de l'écran, si elle existe.
    clock: Option<C>,
    /// Dernier battement reçu, synchronisation verticale du précédent, et
    /// heure d'affichage de l'image précédente.
    clock_beat: Option<Instant>,
    last_vsync: Option<f64>,
    shown_at: Option<f64>,
    /// Une autre horloge a été demandée au fil principal ; celle d'à
    /// présent est arrivée à cette heure.
    clock_asked: bool,
    clock_since: Instant,
    /// Silence de l'horloge au-delà duquel la minuterie prend le relais (il
    /// suit la cadence).
    silence: Duration,
    last_tick: Instant,
    settle_until: Option<Instant>,
    /// Prochaine image prévue par la minuterie (`None` : tout de suite).
    next_frame: Option<Instant>,
    /// Images par seconde voulues par le dernier rythme (0 : horloge arrêtée).
    cadence: f64,
    /// Le rythme des dessins quand ils sont très nombreux.
    crowd: Crowd,
    /// Ce que la simulation rattrape en une image.
    sim_pace: SimPace,
    full_rate_until: Option<Instant>,
    refresh_hz: f64,
    /// L'animal dont le menu est ouvert (il attend sous le curseur).
    menu_pet: Option<u32>,
    /// Génération du troupeau pour laquelle le fil principal est au courant.
    reported_generation: Option<u64>,
    /// Les brebis tout juste arrivées que leur agneau doit suivre : dites au
    /// fil principal à la prochaine description du troupeau.
    wishes: Vec<(u32, Option<f32>)>,
    /// Ce que le fil principal sait des fenêtres qui prennent la souris, et
    /// de quoi le calculer à chaque image sans allouer.
    reported_hits: Vec<(u64, bool)>,
    hits: Vec<(u64, bool)>,
    stats: Option<FrameStats>,
    trial_end: Option<Instant>,
    snapshots: Option<(std::path::PathBuf, Vec<Instant>)>,
    /// La perte du compositeur a été dite au fil principal (jusqu'à ce que
    /// de nouvelles fenêtres arrivent).
    lost_reported: bool,
    sounds_window: (Instant, u32),
    /// Appel grave original, déjà en mémoire avant le premier pas.
    maternal_call: Arc<[u8]>,
    reports: Reports,
    quit: bool,
}

fn local_hour() -> f32 {
    let now = chrono::Local::now();
    now.hour() as f32 + now.minute() as f32 / 60.0
}

fn to_rect(r: platform::OsRect) -> Rect {
    Rect::new(r.x as f32, r.y as f32, r.w as f32, r.h as f32)
}

/// Le curseur est-il sur le corps de l'animal ? Hors du corps, la fenêtre
/// de l'écran laisse passer les clics vers les applications en dessous.
fn hit_test(cursor: Vec2, view: &PetView<'_>, art: &PetArt) -> bool {
    let g = view.ground;
    let (half, height) = art.body;
    cursor.x >= g.x - half
        && cursor.x <= g.x + half
        && cursor.y >= g.y - height
        && cursor.y <= g.y + 3.0
}

impl<C: Clock> Display<C> {
    #[must_use]
    pub fn new(init: Init) -> Self {
        let now = Instant::now();
        let mut flock = Flock::new();
        flock.set_voyage_every(crate::stats::voyages());
        flock.set_collisions(init.live.collisions);
        flock.set_realistic(init.live.realistic);
        flock.set_mood(init.live.mood);
        let seed = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.as_nanos() as u64)
            .unwrap_or(0x5EED);
        let mut display = Self {
            world: World::default(),
            flock,
            scene: Scene::new(),
            arts: HashMap::default(),
            def_arts: Vec::new(),
            done: 0,
            commands: 0,
            started: false,
            screens: Vec::new(),
            live: init.live,
            style: Style { dark: init.dark },
            pointer: Pointer::default(),
            system_cursor: true,
            surprises: Vec::new(),
            show: None,
            farewell_wanted: None,
            farewell_since: None,
            leavers: Vec::new(),
            show_reported: None,
            voyages: (0, 0),
            seed,
            watcher: init.watcher,
            os_windows: Vec::new(),
            dock_frame: None,
            dock: None,
            dock_logged: false,
            floating_seen: HashSet::new(),
            frontmost: None,
            followed: None,
            trusted: false,
            activity: None,
            no_user: std::env::var_os("MOUFLON_ESSAI_CI").is_some(),
            window_notes: HashMap::new(),
            fast_poll_until: now,
            clock: None,
            clock_beat: None,
            last_vsync: None,
            shown_at: None,
            clock_asked: false,
            clock_since: now,
            silence: CLOCK_SILENT,
            last_tick: now,
            settle_until: None,
            next_frame: None,
            cadence: 0.0,
            crowd: Crowd::new(),
            sim_pace: SimPace::new(),
            full_rate_until: None,
            refresh_hz: 60.0,
            menu_pet: None,
            reported_generation: None,
            wishes: Vec::new(),
            reported_hits: Vec::new(),
            hits: Vec::new(),
            stats: init.stats,
            trial_end: None,
            snapshots: None,
            lost_reported: false,
            sounds_window: (now, 0),
            maternal_call: Arc::from(include_bytes!("../assets/appel-maternel.wav").as_slice()),
            reports: init.reports,
            quit: false,
        };
        display.schedule_surprise(now);
        display
    }

    /// La scène (GPU), pour le fil principal quand l'affichage y tourne.
    pub fn scene_mut(&mut self) -> &mut Scene {
        &mut self.scene
    }

    /// Les animaux sont-ils arrivés (les images ont-elles commencé) ?
    #[must_use]
    pub fn started(&self) -> bool {
        self.started
    }

    /// Demandes traitées jusqu'ici.
    #[must_use]
    pub fn done(&self) -> u64 {
        self.done
    }

    #[must_use]
    pub fn quitting(&self) -> bool {
        self.quit
    }

    fn report(&self, report: Report) {
        (self.reports)(report);
    }

    fn note(&self, what: impl std::fmt::Display) {
        if let Some(stats) = &self.stats {
            stats.note(what);
        }
    }

    // ——— L'horloge ———

    /// Une nouvelle horloge de l'écran (l'ancienne cesse de battre).
    pub fn set_clock(&mut self, clock: Option<C>) {
        self.clock = None;
        self.shown_at = None;
        self.last_vsync = None;
        self.clock_asked = false;
        let now = Instant::now();
        self.clock_since = now;
        // Le silence se compte depuis le dernier battement entendu ; une
        // horloge toute neuve ne mène les images qu'une fois qu'elle bat.
        if self.clock_beat.is_none() {
            self.clock_beat = Some(now.checked_sub(CLOCK_SILENT).unwrap_or(now));
        }
        if let Some(clock) = &clock {
            clock.attach();
            log::info!(
                "horloge de l'écran : jusqu'à {:.0} images par seconde",
                clock.max_fps()
            );
            self.note(format_args!(
                "horloge de l'écran, {:.0} i/s au plus",
                clock.max_fps()
            ));
        }
        self.clock = clock;
    }

    /// Un battement de l'horloge vient d'arriver.
    pub fn beat_received(&mut self) {
        self.clock_beat = Some(Instant::now());
        if let Some(stats) = &self.stats {
            crate::stats::Counters::bump(&stats.counters.beats);
        }
    }

    /// L'horloge de l'écran mène-t-elle les images en ce moment ?
    #[must_use]
    pub fn clocked(&self, now: Instant) -> bool {
        self.clock.as_ref().is_some_and(Clock::is_running)
            && self
                .clock_beat
                .is_some_and(|beat| now.duration_since(beat) < self.silence)
    }

    /// Une image est-elle à faire sur la minuterie maintenant ?
    #[must_use]
    pub fn timer_due(&self, now: Instant) -> bool {
        self.started
            && !self.clocked(now)
            && self
                .next_frame
                .is_none_or(|next| now + Duration::from_millis(1) >= next)
    }

    /// Quand se réveiller au plus tard (minuterie, ou garde de l'horloge).
    #[must_use]
    pub fn wake_at(&self) -> Option<Instant> {
        self.next_frame
    }

    /// L'horloge tourne-t-elle (les battements mènent les images) ?
    #[must_use]
    pub fn clock_running(&self) -> bool {
        self.clock.as_ref().is_some_and(Clock::is_running)
    }

    // ——— Les demandes du fil principal ———

    pub fn command(&mut self, command: Command) {
        self.done += 1;
        self.commands += 1;
        // Ce que l'on vient de demander se voit tout de suite (pas besoin
        // d'une image de plus pour ce qui ne change rien à l'écran).
        let quiet = matches!(
            command,
            Command::Cursor(_)
                | Command::Hover
                | Command::Resized { .. }
                | Command::Surprise(..)
                | Command::TrialEnd(_)
                | Command::Snapshots(..)
        );
        if !quiet {
            self.next_frame = None;
        }
        match command {
            Command::Screens { screens, overlays } => {
                for note in self.scene.set_overlays(overlays) {
                    log::info!("{note}");
                    self.note(&note);
                }
                self.screens = screens;
                self.apply_screens();
                // Ce que le fil principal sait des fenêtres est redit à la
                // prochaine image (les neuves laissent passer la souris).
                self.reported_hits.clear();
                self.lost_reported = false;
            }
            Command::Survey(snapshot) => self.apply_snapshot(snapshot, Instant::now()),
            Command::Spawn {
                def,
                art,
                sex,
                lambs,
                here,
                place,
            } => self.spawn(def, art, sex, lambs, here, place),
            Command::SpawnLamb {
                mother,
                def,
                art,
                arriving,
                place,
            } => self.spawn_lamb(mother, def, art, arriving, place),
            Command::Start { settle } => self.start(settle),
            Command::RemoveUnderCursor => {
                if let Some(id) = self.pet_under_cursor() {
                    self.flock.remove(id);
                    if self.pointer.dragging == Some(id) {
                        self.pointer.dragging = None;
                    }
                }
            }
            Command::Goodbye(id) => {
                let input = self.input();
                if self.flock.leave(id, &self.world, &input) && self.pointer.dragging == Some(id) {
                    self.pointer = Pointer::default();
                }
            }
            Command::GoodbyeSpecies(species) => {
                let input = self.input();
                if let Some(id) = self.flock.leave_one(&species, &self.world, &input)
                    && self.pointer.dragging == Some(id)
                {
                    self.pointer = Pointer::default();
                }
            }
            Command::GoodbyeAll => {
                let input = self.input();
                self.flock.leave_all(&self.world, &input);
                self.pointer = Pointer::default();
            }
            Command::Together => {
                let input = self.input();
                let count = self.flock.together(&self.world, &input);
                log::info!("tous ensemble : {count} animal(aux)");
            }
            Command::Redraw { changes, gone } => self.redraw(&changes, &gone),
            Command::RedrawPet {
                id,
                def,
                art,
                factor,
            } => {
                let to = Arc::clone(&def);
                if self
                    .flock
                    .get_mut(id)
                    .is_some_and(|pet| pet.rescale(def, factor))
                {
                    self.remember_art(&to, &art);
                    self.fit_pet(id, art);
                    self.scene.forget_images();
                    // Le fil principal apprend sa nouvelle définition.
                    self.reported_generation = None;
                }
            }
            Command::Live(live) => self.set_live(live),
            Command::Dark(dark) => self.style.dark = dark,
            Command::Resized { key, width, height } => self.scene.resized(key, width, height),
            Command::Press(cursor) => {
                if cursor.is_some() {
                    self.pointer.cursor = cursor;
                }
                self.pointer.pressed = self.cursor();
                self.note(format_args!(
                    "souris appuyée sur n° {:?}",
                    self.pet_under_cursor()
                ));
                // Pendant le compte d'une mise à jour, un clic sur un
                // animal la remet à plus tard ; ensuite, plus rien ne
                // s'attrape : ils s'en vont.
                if let Some(Show::Farewell(show)) = &self.show {
                    let counting = show.is_counting();
                    self.pointer.pressed = None;
                    if counting && self.pet_under_cursor().is_some() {
                        self.postpone();
                    }
                }
            }
            Command::Release(cursor) => {
                if cursor.is_some() {
                    self.pointer.cursor = cursor;
                }
                self.release();
            }
            Command::Cursor(cursor) => self.pointer.cursor = Some(cursor),
            Command::Hover => self.full_rate_until = Some(Instant::now() + HOVER_FULL_RATE),
            Command::Menu => self.open_menu(),
            Command::MenuClosed => {
                // L'animal du menu repart d'où il était.
                self.flock.hold_still(None);
                self.menu_pet = None;
                self.note("menu refermé");
            }
            Command::Farewell(scene) => {
                self.farewell_wanted = Some(scene);
            }
            Command::Postpone => self.postpone(),
            Command::Welcome {
                scene,
                label,
                places,
            } => {
                let focus = self.focus();
                self.show = Some(Show::Welcome(Welcome::new(
                    scene,
                    &label,
                    &places,
                    &self.world,
                    focus,
                )));
                log::info!(
                    "mise à jour : le retour ({} animaux attendus)",
                    places.len()
                );
            }
            Command::Surprise(at, kind) => {
                self.surprises.push((at, Some(kind)));
                self.surprises.sort_by_key(|(at, _)| *at);
            }
            Command::TrialEnd(at) => self.trial_end = Some(at),
            Command::Snapshots(dir, times) => self.snapshots = Some((dir, times)),
            Command::Quit => self.quit = true,
        }
    }

    /// Les premières images des animaux arrivés, prêtes avant qu'ils ne se
    /// montrent ; puis une courte mise en route, et c'est parti.
    fn start(&mut self, settle: Duration) {
        let began = Instant::now();
        for pet in self.flock.pets() {
            if let Some(art) = self.arts.get(&pet.id()) {
                self.scene.warm_up(art, &pet.view(1.0), &self.style);
            }
        }
        let took = began.elapsed();
        log::info!("premières images prêtes en {} ms", took.as_millis());
        self.note(format_args!(
            "premières images prêtes ({:.1} ms)",
            took.as_secs_f32() * 1000.0
        ));
        let now = Instant::now();
        self.settle_until = Some(now + settle);
        self.last_tick = now;
        self.started = true;
        self.next_frame = None;
    }

    fn set_live(&mut self, live: Live) {
        let before = self.live;
        self.live = live;
        self.flock.set_collisions(live.collisions);
        self.flock.set_realistic(live.realistic);
        if before.mood != live.mood {
            self.flock.set_mood(live.mood);
        }
        if before.paused != live.paused {
            // Au réveil, les animaux repartent d'où ils étaient, sans
            // rattraper le temps de la pause.
            self.last_tick = Instant::now();
            self.shown_at = None;
        }
        if before.surprises != live.surprises {
            // Celles qui n'étaient pas imposées partent ou reviennent.
            self.surprises.retain(|(_, kind)| kind.is_some());
            if live.surprises {
                self.schedule_surprise(Instant::now());
            }
        }
        if before.walk_under_dock != live.walk_under_dock {
            self.dock_logged = false;
            if !live.walk_under_dock {
                self.dock_frame = None;
            }
            self.apply_screens();
        }
        if before.frontmost_only != live.frontmost_only
            || before.walk_under_dock != live.walk_under_dock
        {
            self.refresh_windows(Instant::now());
        }
    }

    // ——— Les animaux ———

    fn spawn(
        &mut self,
        def: Arc<PetDef>,
        art: Arc<PetArt>,
        sex: Option<mouflon_core::Sex>,
        lambs: Vec<Option<f32>>,
        here: bool,
        place: Option<f32>,
    ) {
        self.seed = self
            .seed
            .wrapping_mul(6_364_136_223_846_793_005)
            .wrapping_add(1);
        self.remember_art(&def, &art);
        // Avec plusieurs écrans : sur celui du curseur si on le demande (et
        // si on sait où il est), sinon au hasard parmi tous.
        let near = here.then(|| self.cursor()).flatten();
        // Celui qui revient d'une mise à jour retombe à sa place : de la
        // lumière de la soucoupe si elle est là, sinon du haut de l'écran.
        let from = place.and_then(|x| match &self.show {
            Some(Show::Welcome(show)) => show.drop_point(x, def.physics.height),
            _ => None,
        });
        let id = match (place, from, near) {
            (Some(_), Some(from), _) => self.flock.spawn_at(def, self.seed, &self.world, from, sex),
            (Some(x), None, _) => self
                .flock
                .spawn_as(def, self.seed, &self.world, Some(x), sex),
            (None, _, Some(near)) => self
                .flock
                .spawn_near(def, self.seed, &self.world, near, sex),
            (None, _, None) => self.flock.spawn_as(def, self.seed, &self.world, None, sex),
        };
        if let (Some(x), Some(_)) = (place, from)
            && let Some(Show::Welcome(show)) = &mut self.show
        {
            show.arrived(id, x, &mut self.flock, &self.world);
        }
        // Une brebis que ses agneaux suivent : le fil principal l'apprend
        // avec la description du troupeau, et prépare le dessin de chacun
        // (qui revient à sa place, s'il en a une). Celui qui revient à sa
        // place revient même si elle ne peut plus être sa mère : seul.
        let mother = self
            .flock
            .pets()
            .iter()
            .any(|p| p.id() == id && p.can_mother());
        self.wishes.extend(
            lambs
                .into_iter()
                .filter(|place| mother || place.is_some())
                .map(|place| (id, place)),
        );
        if let Some(stats) = &self.stats
            && let Some(pet) = self.flock.pets().iter().find(|p| p.id() == id)
        {
            stats.note(format_args!("arrivée de {} (n° {id})", pet.def().meta.name));
        }
        self.fit_pet(id, art);
    }

    /// Le lien est validé ici aussi : la mère peut être partie pendant que
    /// le fil de travail préparait le dessin. Aucun chargement sur ce fil.
    fn spawn_lamb(
        &mut self,
        mother: u32,
        def: Arc<PetDef>,
        art: Arc<PetArt>,
        arriving: bool,
        place: Option<f32>,
    ) {
        // Même refusée, la demande doit être acquittée : le fil principal
        // libère alors son entrée préparée et son attente d’arrivée.
        self.reported_generation = None;
        self.seed = self
            .seed
            .wrapping_mul(6_364_136_223_846_793_005)
            .wrapping_add(1);
        let lamb = match place {
            // Il revient d'une mise à jour, à sa place : de la lumière de la
            // soucoupe si elle est là, sinon du haut de l'écran.
            Some(x) => {
                let from = match &self.show {
                    Some(Show::Welcome(show)) => show.drop_point(x, def.physics.height),
                    _ => None,
                };
                let lamb = self
                    .flock
                    .spawn_lamb_back(mother, Arc::clone(&def), self.seed, &self.world, x, from)
                    // Elle ne peut pas être sa mère : il revient seul, à sa
                    // place.
                    .or_else(|| {
                        Some(match from {
                            Some(at) => self.flock.spawn_at(
                                Arc::clone(&def),
                                self.seed,
                                &self.world,
                                at,
                                None,
                            ),
                            None => self.flock.spawn_as(
                                Arc::clone(&def),
                                self.seed,
                                &self.world,
                                Some(x),
                                None,
                            ),
                        })
                    });
                if let (Some(id), Some(_)) = (lamb, from)
                    && let Some(Show::Welcome(show)) = &mut self.show
                {
                    show.arrived(id, x, &mut self.flock, &self.world);
                }
                lamb
            }
            None if arriving => {
                self.flock
                    .spawn_lamb_arriving(mother, Arc::clone(&def), self.seed, &self.world)
            }
            None => self
                .flock
                .spawn_lamb_with(mother, Arc::clone(&def), self.seed, &self.world),
        };
        if let Some(id) = lamb {
            self.remember_art(&def, &art);
            self.fit_pet(id, art);
        }
    }

    /// Le dessin d'une définition (pour les personnages secondaires qui la
    /// partagent).
    fn remember_art(&mut self, def: &Arc<PetDef>, art: &Arc<PetArt>) {
        match self.def_arts.iter_mut().find(|(d, _)| Arc::ptr_eq(d, def)) {
            Some((_, known)) => *known = Arc::clone(art),
            None => self.def_arts.push((Arc::clone(def), Arc::clone(art))),
        }
    }

    /// Un animal prend son dessin (`art` : celui de son espèce ; il en porte
    /// celui de son sexe et de son âge) : ce qui en dépend pour la
    /// simulation suit.
    fn fit_pet(&mut self, id: u32, mut art: Arc<PetArt>) {
        if let Some(pet) = self.flock.get_mut(id) {
            art = PetArt::worn_by(&art, pet);
            // Les collisions partent quand les dessins se touchent.
            if let Some((front, back)) = art.reach() {
                pet.set_reach(front, back);
            }
            if let Some((udder, muzzle)) = art.nursing_points() {
                pet.set_nursing_points(udder, muzzle);
            }
            // Un autre se pose sur ce qu'on voit de lui, même couché.
            if let Some(tops) = art.tops() {
                pet.set_tops(tops);
            }
        }
        self.arts.insert(id, art);
    }

    /// Des animaux changent de dessin (autre taille, images compressées) ;
    /// ceux dont l'espèce n'est plus dans la bibliothèque s'en vont.
    fn redraw(&mut self, changes: &[Redraw], gone: &[Arc<PetDef>]) {
        let mut fits = Vec::new();
        let mut removed = Vec::new();
        for pet in self.flock.pets_mut() {
            if gone.iter().any(|def| Arc::ptr_eq(def, pet.species())) {
                removed.push(pet.id());
                continue;
            }
            let Some(change) = changes.iter().find(|c| Arc::ptr_eq(&c.from, pet.species())) else {
                continue;
            };
            let same = Arc::ptr_eq(&change.from, &change.to);
            if same || pet.rescale(Arc::clone(&change.to), change.factor) {
                fits.push((pet.id(), Arc::clone(&change.art)));
            } else {
                removed.push(pet.id());
            }
        }
        for change in changes {
            self.remember_art(&change.to, &change.art);
        }
        for (id, art) in fits {
            self.fit_pet(id, art);
        }
        for id in removed {
            self.flock.remove(id);
            if self.pointer.dragging == Some(id) {
                self.pointer.dragging = None;
            }
        }
        self.scene.forget_images();
        // Le fil principal apprend leurs nouvelles définitions.
        self.reported_generation = None;
    }

    /// Là où l'utilisateur regarde : le bas de l'écran du curseur, à
    /// défaut celui du premier écran.
    fn focus(&self) -> Vec2 {
        let screens = self.world.screens();
        self.cursor()
            .and_then(|cursor| self.world.screen_at(cursor))
            .or(screens.first())
            .map_or(Vec2::ZERO, |screen| {
                let room = screen.room();
                Vec2::new(room.center().x, room.bottom())
            })
    }

    /// L'annonce demandée commence, maintenant que le troupeau est visible.
    /// Sans personne à l'écran, il n'y a rien à annoncer : la place est
    /// libre tout de suite.
    fn begin_farewell(&mut self) {
        if matches!(self.show, Some(Show::Farewell(_))) {
            self.farewell_wanted = None;
            return;
        }
        // Un retour se joue encore (une version sortie juste après
        // l'arrivée de celle-ci) : l'annonce attend qu'il soit fini.
        if matches!(self.show, Some(Show::Welcome(_))) {
            return;
        }
        let Some(scene) = self.farewell_wanted else {
            return;
        };
        // Ceux qui viennent d'arriver tombent encore : l'annonce attend que
        // l'un d'eux soit posé (pas plus de quelques secondes).
        let now = Instant::now();
        let since = *self.farewell_since.get_or_insert(now);
        let landed = self
            .flock
            .pets()
            .iter()
            .any(|pet| matches!(pet.support(), Support::Ground { .. }));
        if !landed && !self.flock.pets().is_empty() && now.duration_since(since) < FAREWELL_WAIT {
            return;
        }
        self.farewell_wanted = None;
        self.farewell_since = None;
        self.leavers = self
            .flock
            .pets()
            .iter()
            .filter(|pet| !pet.is_child() && !pet.is_leaving())
            .map(|pet| Leaver {
                id: pet.id(),
                x: pet.position().x,
                variant: pet.variant(),
                mother: pet.mother(),
                lamb: pet.is_lamb(),
            })
            .collect();
        let focus = self.focus();
        match Farewell::new(scene, &mut self.flock, &self.world, focus) {
            Some(show) => {
                log::info!(
                    "mise à jour : l'annonce commence ({}, {} animaux)",
                    match show.scene() {
                        ShowScene::Herald => "le mouton annonceur",
                        ShowScene::Saucer => "la soucoupe",
                    },
                    self.leavers.len()
                );
                self.show = Some(Show::Farewell(show));
                self.tell_show(ShowReport::Counting);
            }
            None => {
                log::info!("mise à jour : personne à l'écran, rien à annoncer");
                self.show = None;
                self.tell_show(ShowReport::Leaving(Vec::new()));
                self.tell_show(ShowReport::Gone);
            }
        }
    }

    /// Dit au fil principal où en est l'annonce (une fois par étape).
    fn tell_show(&mut self, report: ShowReport) {
        if self.show_reported.as_ref() != Some(&report) {
            self.show_reported = Some(report.clone());
            self.report(Report::Show(report));
        }
    }

    /// Remet la mise à jour annoncée à plus tard, si le compte tourne
    /// encore (ou si l'annonce n'a pas commencé).
    fn postpone(&mut self) {
        if self.farewell_wanted.take().is_some() {
            self.farewell_since = None;
            log::info!("mise à jour : remise à plus tard avant l'annonce");
            self.show_reported = None;
            self.tell_show(ShowReport::Postponed);
            return;
        }
        if let Some(Show::Farewell(show)) = &mut self.show
            && show.postpone()
        {
            log::info!("mise à jour : remise à plus tard pendant le compte");
        }
    }

    /// L'annonce ou le retour avancent de `dt` secondes.
    fn step_show(&mut self, dt: f32, input: &Input) {
        match &mut self.show {
            Some(Show::Farewell(show)) => {
                let counting = show.is_counting();
                show.step(dt, &mut self.flock, &self.world, input);
                let outcome = show.outcome();
                if counting && !show.is_counting() && outcome != Some(Outcome::Postponed) {
                    // Zéro : ils s'en vont. Plus rien ne se tient ni ne
                    // s'attrape, le menu ouvert ne retient plus personne.
                    // Le troupeau tel qu'il est à ce moment-là reviendra.
                    self.pointer.dragging = None;
                    self.pointer.pressed = None;
                    self.flock.hold_still(None);
                    self.menu_pet = None;
                    let leavers: Vec<Leaver> = self
                        .flock
                        .pets()
                        .iter()
                        .filter(|pet| !pet.is_child() && !pet.is_leaving())
                        .map(|pet| Leaver {
                            id: pet.id(),
                            x: pet.position().x,
                            variant: pet.variant(),
                            mother: pet.mother(),
                            lamb: pet.is_lamb(),
                        })
                        .collect();
                    if !leavers.is_empty() {
                        self.leavers = leavers;
                    }
                    let leavers = std::mem::take(&mut self.leavers);
                    self.tell_show(ShowReport::Leaving(leavers));
                }
                match outcome {
                    Some(Outcome::Gone) => {
                        self.show = None;
                        log::info!("mise à jour : le troupeau est parti");
                        self.tell_show(ShowReport::Gone);
                    }
                    Some(Outcome::Postponed) => {
                        self.show = None;
                        self.leavers.clear();
                        self.show_reported = None;
                        self.tell_show(ShowReport::Postponed);
                    }
                    None => {}
                }
            }
            Some(Show::Welcome(show)) => {
                show.step(dt, &mut self.flock, &self.world);
                if show.is_over() {
                    self.show = None;
                    log::info!("mise à jour : le retour est fini");
                }
            }
            None => {}
        }
    }

    /// Le menu de l'animal sous le curseur : il attend sous le menu, les
    /// autres continuent.
    fn open_menu(&mut self) {
        if self.pointer.dragging.is_some() {
            return;
        }
        // Ils s'en vont : plus de menu.
        if matches!(&self.show, Some(Show::Farewell(show)) if !show.is_counting()) {
            return;
        }
        let Some(id) = self.pet_under_cursor() else {
            return;
        };
        let Some(pet) = self.flock.pets().iter().find(|p| p.id() == id) else {
            return;
        };
        // Les personnages secondaires et ceux qui partent n'en ont pas.
        if pet.is_child() || pet.is_leaving() {
            return;
        }
        let def = Arc::clone(pet.species());
        let can_have_lamb = pet.can_have_lamb();
        // Le clic n'est pas un « coucou », ni le début d'un glisser.
        self.pointer.pressed = None;
        self.note(format_args!("menu de n° {id}"));
        self.flock.hold_still(Some(id));
        self.menu_pet = Some(id);
        self.report(Report::PetMenu {
            id,
            def,
            can_have_lamb,
        });
    }

    /// Bouton relâché : on lâche l'animal tenu, ou un simple clic le touche.
    fn release(&mut self) {
        self.note("souris relâchée");
        let input = self.input();
        if let Some(id) = self.pointer.dragging.take() {
            if let Some(pet) = self.flock.get_mut(id) {
                pet.release(&self.world);
            }
        } else if self.pointer.pressed.is_some()
            && let Some(id) = self.pet_under_cursor()
            && let Some(pet) = self.flock.get_mut(id)
        {
            pet.poke(&self.world, &input);
        }
        self.pointer.pressed = None;
    }

    fn cursor(&self) -> Option<Vec2> {
        self.system_cursor
            .then(platform::cursor_position)
            .flatten()
            .map(|(x, y)| Vec2::new(x as f32, y as f32))
            .or(self.pointer.cursor)
    }

    fn input(&self) -> Input {
        let activity = self.user_activity();
        Input {
            cursor: self.cursor(),
            local_hour: local_hour(),
            since_key: activity.map(|a| a.since_key as f32),
            idle: activity.map(|a| a.idle as f32),
        }
    }

    /// Ce que fait l'utilisateur, tel que le dit le dernier relevé (quatre
    /// par seconde au moins), vieilli du temps passé depuis. Pas dans la
    /// CI : personne n'y touche à la machine.
    fn user_activity(&self) -> Option<platform::UserActivity> {
        if self.no_user {
            return None;
        }
        let (at, known) = self.activity?;
        let age = at.elapsed().as_secs_f64();
        Some(platform::UserActivity {
            since_key: known.since_key + age,
            idle: known.idle + age,
        })
    }

    /// L'animal sous le curseur (le plus en avant).
    fn pet_under_cursor(&self) -> Option<u32> {
        self.pet_at(self.cursor()?, 1.0)
    }

    /// L'animal au point `p` (le plus en avant), à l'instant `alpha` entre
    /// deux pas. Les personnages secondaires laissent passer les clics.
    fn pet_at(&self, p: Vec2, alpha: f32) -> Option<u32> {
        self.flock
            .pets()
            .iter()
            .rev()
            .filter(|pet| !pet.is_child())
            .find(|pet| {
                self.arts.get(&pet.id()).is_some_and(|art| {
                    // D'abord sa place au dernier pas, avec de la marge : la
                    // vue entre deux pas n'est calculée que pour ceux qui sont
                    // tout près (une foule en compte des centaines).
                    let (half, height) = art.body;
                    let at = pet.drawn_position();
                    let margin = 64.0;
                    (p.x - at.x).abs() <= half + margin
                        && p.y >= at.y - height - margin
                        && p.y <= at.y + margin
                        && hit_test(p, &pet.view(alpha), art)
                })
            })
            .map(mouflon_core::Pet::id)
    }

    // ——— Les écrans, les fenêtres, le Dock ———

    /// Les écrans du système, le Dock en moins (s'il le faut), pour le monde.
    fn apply_screens(&mut self) {
        let mut screens = self.screens.clone();
        self.dock = None;
        if self.live.walk_under_dock
            && let Some(frame) = self.dock_frame
        {
            self.dock = exact_dock(&mut screens, to_rect(frame));
            if !self.dock_logged {
                self.dock_logged = true;
                log::info!(
                    "Dock : {:.0} × {:.0} en {:.0}, {:.0}{}",
                    frame.w,
                    frame.h,
                    frame.x,
                    frame.y,
                    if self.dock.is_some() {
                        ""
                    } else {
                        " (sur le côté, masqué ou sur toute la largeur : ignoré)"
                    }
                );
            }
        }
        if screens != self.world.screens() {
            let first = self.world.screens().is_empty();
            self.world.set_screens(screens);
            // Écran branché, débranché ou réglé autrement : l'horloge se cale
            // de nouveau sur le plus rapide.
            if !first {
                self.ask_clock();
            }
            self.refresh_windows(Instant::now());
        }
        let fastest = self.scene.screens().map(|s| s.hz).fold(0.0, f64::max);
        if fastest > 0.0 {
            self.refresh_hz = fastest.max(30.0);
        }
    }

    fn ask_clock(&mut self) {
        if !self.clock_asked {
            self.clock_asked = true;
            self.report(Report::NeedClock);
        }
    }

    /// Applique ce que le relevé a vu depuis la dernière image.
    fn apply_watch(&mut self, now: Instant) {
        let Some(watcher) = self.watcher.clone() else {
            return;
        };
        if let Some(snapshot) = watcher.take() {
            self.apply_snapshot(snapshot, now);
        }
        // Une fenêtre qui porte un animal bouge, ou on tient un animal : on
        // relève plus souvent pour qu'il suive en douceur.
        watcher.set_fast(now < self.fast_poll_until || self.pointer.dragging.is_some());
    }

    fn apply_snapshot(&mut self, snapshot: Snapshot, now: Instant) {
        // Ce que fait l'utilisateur, l'app au premier plan (sauf Mouflon
        // lui-même, avec ses réglages) et l'autorisation d'Accessibilité :
        // tout vient du relevé, aucune question au système ici.
        self.activity = snapshot.activity.map(|activity| (now, activity));
        if let Some(pid) = snapshot.frontmost
            && pid != platform::current_pid()
        {
            self.frontmost = Some(pid);
        }
        let trust_changed = snapshot.trusted != self.trusted;
        self.trusted = snapshot.trusted;
        // (L'Accessibilité est une affaire du Mac : ailleurs, rien à noter.)
        if cfg!(target_os = "macos") && (self.followed != Some(snapshot.followed) || trust_changed)
        {
            self.note(format_args!(
                "fenêtres suivies par l'Accessibilité : {} app(s){}",
                snapshot.followed,
                if snapshot.trusted {
                    ""
                } else {
                    " (pas d'autorisation)"
                }
            ));
        }
        self.followed = Some(snapshot.followed);
        if trust_changed {
            self.report(Report::Trusted(snapshot.trusted));
        }
        let dock = snapshot.dock.filter(|_| self.live.walk_under_dock);
        if dock != self.dock_frame {
            self.dock_frame = dock;
            self.apply_screens();
        }
        if self.stats.is_some() {
            self.note_window_changes(&snapshot.windows, now);
        }
        self.os_windows = snapshot.windows;
        self.refresh_windows(now);
    }

    /// Au journal d'un essai : les fenêtres des autres apps qui arrivent,
    /// partent, bougent (au plus deux fois par seconde chacune) ou changent
    /// de niveau.
    fn note_window_changes(&mut self, windows: &[platform::OsWindow], now: Instant) {
        let Some(stats) = &self.stats else {
            return;
        };
        let describe = |w: &platform::OsWindow| {
            format!(
                "n° {} de {} (pid {}, niveau {}, {:.0}×{:.0} en {:.0},{:.0})",
                w.id, w.owner, w.pid, w.layer, w.frame.w, w.frame.h, w.frame.x, w.frame.y
            )
        };
        for w in windows {
            match self.os_windows.iter().find(|old| old.id == w.id) {
                None => stats.note(format_args!("fenêtre + {}", describe(w))),
                Some(old) if old.layer != w.layer => {
                    stats.note(format_args!(
                        "fenêtre niveau {} → {}",
                        old.layer,
                        describe(w)
                    ));
                }
                Some(old) if old.frame != w.frame => {
                    let recent = self
                        .window_notes
                        .get(&w.id)
                        .is_some_and(|at| now.duration_since(*at) < Duration::from_millis(500));
                    if !recent {
                        self.window_notes.insert(w.id, now);
                        stats.note(format_args!("fenêtre ~ {}", describe(w)));
                    }
                }
                Some(_) => {}
            }
        }
        for old in &self.os_windows {
            if !windows.iter().any(|w| w.id == old.id) {
                stats.note(format_args!("fenêtre − {}", describe(old)));
                self.window_notes.remove(&old.id);
            }
        }
    }

    /// Applique le dernier relevé des fenêtres (sans rien redemander au système).
    fn refresh_windows(&mut self, now: Instant) {
        let mut windows = Vec::new();
        // Celles où l'on marche ; les autres ne font que cacher ce qui est
        // derrière elles.
        let mut walkable = Vec::new();
        let me = platform::current_pid();
        for w in &self.os_windows {
            // Les fenêtres de Mouflon ne sont jamais des rebords (le relevé
            // les écarte déjà).
            if w.pid == me {
                continue;
            }
            // Calque transparent posé sur tout un écran (le clavier d'accessibilité
            // en a deux) : ni sol, ni obstacle.
            let overlay = w.is_floating()
                && self.world.screens().iter().any(|s| {
                    f64::from(s.frame.w) * f64::from(s.frame.h) * 0.9 <= w.frame.w * w.frame.h
                });
            let walkable_level = w.is_walkable() && !overlay;
            // Panneaux flottants (visualiseur de clavier…) : notés une fois au journal.
            if w.is_floating() && self.floating_seen.insert((w.owner.clone(), w.layer)) {
                log::info!(
                    "fenêtre flottante : {} (niveau {}, {:.0} × {:.0} en {:.0}, {:.0}){}",
                    w.owner,
                    w.layer,
                    w.frame.w,
                    w.frame.h,
                    w.frame.x,
                    w.frame.y,
                    if walkable_level { "" } else { ", ignorée" }
                );
            }
            if !walkable_level {
                continue;
            }
            // Seulement sur l'app au premier plan : les fenêtres des autres
            // apps ne portent personne, mais cachent toujours le rebord
            // qu'elles couvrent (sans cela, un mouton marchait sur le bord
            // d'une fenêtre du Terminal cachée derrière le Finder, au milieu
            // de la liste des fichiers). Un panneau flottant reste visible
            // quelle que soit l'app au premier plan. (Tant que le système
            // n'a pas dit quelle app est devant, ou s'il ne le dit jamais,
            // comme un bureau X11 sans gestionnaire de fenêtres, toutes
            // portent.)
            walkable.push(
                !self.live.frontmost_only
                    || w.is_floating()
                    || self.frontmost.is_none_or(|pid| pid == w.pid),
            );
            windows.push(WindowRect {
                id: w.id,
                frame: to_rect(w.frame),
            });
        }
        // Le Dock passe devant toutes les fenêtres.
        if let Some(dock) = self.dock {
            windows.insert(0, dock);
            walkable.insert(0, true);
        }
        // Une fenêtre qui porte un animal bouge : on relève plus souvent
        // pour qu'il suive en douceur.
        let supports: Vec<(u64, f32, f32)> = self
            .flock
            .pets()
            .iter()
            .filter_map(|p| match p.support() {
                Support::Ground { surface } => {
                    self.world.surface(surface).map(|s| (surface, s.x0, s.y))
                }
                _ => None,
            })
            .collect();
        self.world
            .set_windows_among(&windows, &walkable, LedgeOptions::default());
        let moved = supports.iter().any(|&(id, x0, y)| {
            self.world
                .surface(id)
                .is_some_and(|s| (s.x0 - x0).abs() > 0.5 || (s.y - y).abs() > 0.5)
        });
        if moved {
            self.fast_poll_until = now + Duration::from_millis(1500);
        }
    }

    // ——— Les surprises ———

    /// La prochaine surprise tirée au sort, dans un quart d'heure à
    /// quarante minutes.
    fn schedule_surprise(&mut self, now: Instant) {
        if !self.live.surprises || self.surprises.iter().any(|(_, k)| k.is_none()) {
            return;
        }
        let minutes = SURPRISE_EVERY.0 + (SURPRISE_EVERY.1 - SURPRISE_EVERY.0) * self.random_unit();
        self.surprises
            .push((now + Duration::from_secs_f32(minutes * 60.0), None));
        self.surprises.sort_by_key(|(at, _)| *at);
    }

    /// Un nombre au hasard dans [0, 1) (même suite que les graines des animaux).
    fn random_unit(&mut self) -> f32 {
        self.seed = self
            .seed
            .wrapping_mul(6_364_136_223_846_793_005)
            .wrapping_add(1);
        (self.seed >> 40) as f32 / (1u64 << 24) as f32
    }

    /// Une surprise à son heure : un coup de vent, ou une soucoupe (si un
    /// animal est là pour elle). Pas devant personne : elle attend son retour.
    fn surprise(&mut self, now: Instant, input: &Input) {
        let Some((next, &(at, imposed))) = self
            .surprises
            .iter()
            .enumerate()
            .min_by_key(|(_, (at, _))| *at)
        else {
            return;
        };
        if now < at {
            return;
        }
        self.surprises.remove(next);
        if imposed.is_none() && (input.away() || self.flock.pets().is_empty()) {
            // Personne pour la voir : un peu plus tard.
            self.surprises.push((now + Duration::from_secs(60), None));
            self.surprises.sort_by_key(|(at, _)| *at);
            return;
        }
        let kind = imposed.unwrap_or(if self.random_unit() < 0.55 {
            Surprise::Wind
        } else {
            Surprise::Saucer
        });
        let saucer = match kind {
            Surprise::Saucer => {
                let seed = (self.random_unit() * 1e6) as u64;
                self.flock.invite_saucer(&self.world, seed)
            }
            Surprise::Wind => None,
        };
        let what = if saucer.is_some() {
            "une soucoupe volante".to_owned()
        } else {
            // Pas d'animal pour la soucoupe : un coup de vent à la place.
            let dir = if self.random_unit() < 0.5 { -1.0 } else { 1.0 };
            let seconds = 6.0 + 3.0 * self.random_unit();
            self.flock.blow(dir, seconds);
            format!(
                "un coup de vent vers la {} ({seconds:.1} s)",
                if dir > 0.0 { "droite" } else { "gauche" }
            )
        };
        log::info!("surprise : {what}");
        self.note(format_args!("surprise : {what}"));
        if imposed.is_none() {
            self.schedule_surprise(now);
        }
    }

    // ——— Les sons et les événements des animaux ———

    /// Joue les sons que les animaux viennent de faire (s'ils ont la parole),
    /// et vide la liste de leurs événements (notés au journal d'un essai).
    fn play_sounds(&mut self) {
        let audible = self.live.sounds && !self.live.paused;
        let now = Instant::now();
        if now.duration_since(self.sounds_window.0) >= SOUND_WINDOW {
            self.sounds_window = (now, 0);
        }
        for (rank, pet) in self.flock.pets_mut().iter_mut().enumerate() {
            for event in pet.take_events() {
                // Au journal : les événements des premiers arrivés seulement
                // (une foule en ferait des milliers par seconde).
                if let Some(stats) = &self.stats
                    && rank < crate::stats::DETAILED_PETS
                {
                    let mut what = describe(&event, pet);
                    if let PetEvent::Landed { .. } = event
                        && let Support::Ground { surface } = pet.support()
                    {
                        what.push_str(" sur ");
                        what.push_str(&surface_name(&self.world, &self.os_windows, surface));
                    }
                    stats.note(format_args!("n° {} {what}", pet.id()));
                }
                // Pas plus de quelques sons à la fois : une foule ne fait
                // qu'un brouhaha (et chaque son coûte).
                if audible && self.sounds_window.1 < SOUNDS_PER_WINDOW {
                    let sound = match event {
                        PetEvent::Sound { sound, clip } => {
                            pet.def().sounds.get(sound).and_then(|sound| {
                                sound.clips.get(clip).map(|data| (data, sound.volume))
                            })
                        }
                        PetEvent::MaternalCall => Some((&self.maternal_call, 0.65)),
                        _ => None,
                    };
                    if let Some((data, volume)) = sound {
                        self.sounds_window.1 += 1;
                        platform::play_sound(data, volume * VOLUME);
                    }
                }
            }
        }
    }

    // ——— L'image ———

    /// Une image : simulation, scène, prochain réveil. `beat` : le battement
    /// de l'horloge de l'écran qui l'a réveillée, s'il y en a un, et les
    /// battements fondus depuis l'image d'avant.
    pub fn frame(&mut self, beat: Option<platform::Beat>, coalesced: u32) {
        let now = Instant::now();
        let shown_at = beat.map(|b| b.target);
        let cadence = self.cadence;
        // Pour le journal d'un essai : ce que dit l'horloge, le retard de la
        // minuterie, les allocations de l'image.
        let diagnosing = self.stats.is_some();
        let mut beat_timing = None;
        let mut timer_late = None;
        let allocs_before = diagnosing.then(platform::alloc_counts);
        if diagnosing {
            platform::diag::frame_started();
            match beat {
                Some(t) => {
                    let interval = (t.target - t.vsync).max(1e-4);
                    let missed = self.last_vsync.map_or(0, |last| {
                        (((t.vsync - last) / interval).round() as i64 - 1).max(0) as u32
                    });
                    beat_timing = Some(crate::stats::BeatTiming {
                        latency: ((t.called - t.vsync) * 1000.0) as f32,
                        queue: ((platform::diag::media_time() - t.called) * 1000.0) as f32,
                        missed,
                        coalesced,
                    });
                }
                None => {
                    timer_late = self.next_frame.map(|planned| {
                        now.saturating_duration_since(planned).as_secs_f32() * 1000.0
                    });
                }
            }
        }
        if let Some(t) = beat {
            self.last_vsync = Some(t.vsync);
        }
        // Calé sur l'écran, on avance exactement du temps qui sépare deux
        // images affichées : les petits retards ne font plus varier la
        // vitesse des animaux d'une image à l'autre.
        let elapsed = match (shown_at, self.shown_at) {
            (Some(at), Some(before)) if at > before => (at - before) as f32,
            _ => now.duration_since(self.last_tick).as_secs_f32(),
        };
        self.shown_at = shown_at;
        self.last_tick = now;
        let mut phases = crate::stats::Phases::default();

        // Une horloge qui tourne mais ne bat plus (écran changé, réveil…) :
        // le fil principal en refait une.
        if self.clock.as_ref().is_some_and(Clock::is_running)
            && now.duration_since(self.clock_since) > CLOCK_DEAD
            && self
                .clock_beat
                .is_some_and(|beat| now.duration_since(beat) > CLOCK_DEAD)
        {
            self.ask_clock();
        }
        self.apply_watch(now);
        // Le compositeur a perdu son appareil : le fil principal repose la
        // scène dans ses fenêtres (rien ne se voit d'ici là).
        if !self.lost_reported && platform::scene_lost() {
            self.lost_reported = true;
            log::warn!("affichage perdu : la scène est à reposer");
            self.note("affichage perdu : la scène est à reposer");
            self.report(Report::SceneLost);
        }
        let mut mark = Instant::now();
        phases.watch = mark - now;

        // Mise en route du lancement : la scène est là mais vide, les
        // animaux attendent.
        let settling = self.settle_until.is_some_and(|until| now < until);
        if !settling && self.settle_until.take().is_some() {
            log::info!("mise en route terminée : les animaux arrivent");
            self.note("mise en route terminée");
        }

        let input = self.input();
        let cursor = input.cursor;
        if !settling && !self.live.paused {
            self.begin_farewell();
            // Pas de surprise pendant une annonce.
            if self.show.is_none() && self.farewell_wanted.is_none() {
                self.surprise(now, &input);
            }
        }

        // Glisser : on n'attrape l'animal qu'après un petit déplacement, un
        // simple clic reste un « coucou ». Il est pris là où l'on a cliqué,
        // et ce point reste sous le curseur.
        if !settling
            && let (Some(start), Some(c), None) =
                (self.pointer.pressed, cursor, self.pointer.dragging)
            && (c - start).length() > 3.0
            && let Some(id) = self.pet_at(start, 1.0)
            && let Some(pet) = self.flock.get_mut(id)
        {
            pet.grab(start, &self.world);
            self.pointer.dragging = Some(id);
            self.note(format_args!(
                "n° {id} attrapé en {:.0},{:.0}",
                start.x, start.y
            ));
        }
        if let (Some(id), Some(c)) = (self.pointer.dragging, cursor)
            && let Some(pet) = self.flock.get_mut(id)
        {
            pet.drag_to(c);
        }

        let simulated = elapsed.min(self.sim_pace.allowance());
        let alpha = if self.live.paused || settling {
            1.0
        } else {
            self.flock.advance(simulated, &self.world, &input)
        };
        // L'annonce d'une mise à jour va à l'heure de la montre : dix
        // secondes sont dix secondes, même si l'écran peine.
        if !(self.live.paused || settling) {
            self.step_show(elapsed, &input);
        }
        phases.sim = lap(&mut mark);
        if !(self.live.paused || settling) {
            self.sim_pace.measure(phases.sim, simulated);
        }
        let voyages = self.flock.voyages();
        if voyages != self.voyages {
            self.voyages = voyages;
            self.note(format_args!(
                "voyages : {} partis vers un autre écran, {} arrivés",
                voyages.0, voyages.1
            ));
        }
        if !settling {
            self.play_sounds();
        }
        phases.sounds = lap(&mut mark);
        self.sync_pets();
        phases.windows = lap(&mut mark);

        // La scène : chaque soucoupe, puis chaque animal, à sa place ; une
        // seule transaction pour tout ce qui change.
        self.scene.begin();
        if !settling {
            // Ce qu'une annonce ajoute : le faisceau et la soucoupe sous les
            // animaux, le compte et la bulle par-dessus (plus bas).
            let extras = match &self.show {
                Some(Show::Farewell(show)) => show.view(&self.flock, &self.world),
                Some(Show::Welcome(show)) => show.view(&self.flock, &self.world),
                None => ShowView::default(),
            };
            if let Some(beam) = &extras.beam {
                self.scene.show_beam(SHOW_BEAM, beam, &self.style);
            }
            if let Some(saucer) = &extras.saucer {
                self.scene.show_saucer(SHOW_SAUCER, saucer, &self.style);
            }
            for saucer in self.flock.saucers() {
                self.scene
                    .show_saucer(saucer.id(), &saucer.view(), &self.style);
            }
            // Chacun à son rythme : celui qui broute, rumine ou dort garde
            // ses morceaux d'une image à l'autre, même quand l'écran bat à
            // pleine cadence pour un autre qui court ; en foule, ces rythmes
            // s'espacent (voir `Crowd`). Celui qu'on tient est redessiné à
            // chaque image : il suit la main.
            let held = self.pointer.dragging;
            let screen = if cadence > 0.0 {
                cadence
            } else {
                self.refresh_hz
            };
            for pet in self.flock.pets() {
                if let Some(art) = self.arts.get(&pet.id()) {
                    let mut view = pet.view(alpha);
                    // Emporté par la soucoupe, ou reposé par elle : il
                    // s'efface, ou apparaît.
                    let seen = match &self.show {
                        Some(Show::Farewell(show)) => show.opacity(pet.id()),
                        Some(Show::Welcome(show)) => show.opacity(pet.id()),
                        None => 1.0,
                    };
                    view.opacity *= seen;
                    if held != Some(pet.id())
                        && seen >= 1.0
                        && let Some(fps) = self.crowd.rate(pet.activity(), screen)
                        && self.scene.keep_pet(
                            pet.id(),
                            art,
                            &view,
                            now,
                            Duration::from_secs_f64(1.0 / fps),
                        )
                    {
                        continue;
                    }
                    self.scene.show_pet(pet.id(), art, &view, &self.style);
                }
            }
            if let Some(count) = &extras.count {
                self.scene.show_count(SHOW_COUNT, count, &self.style);
            }
            if let Some(bubble) = &extras.bubble {
                self.scene.show_bubble(SHOW_BUBBLE, bubble, &self.style);
            }
        }
        let timings = self.scene.finish();
        // Ce que cette image a coûté (sans le décodage d'une image neuve,
        // qu'espacer les dessins n'allégerait pas), pour le rythme de la
        // foule.
        self.crowd
            .measure(timings.paint + timings.layers + timings.commit, cadence);
        phases.paint += timings.paint + timings.decode;
        phases.moves += timings.layers;
        phases.worst_move = phases.worst_move.max(timings.layers);
        phases.present += timings.commit;
        phases.renders += timings.repainted;
        if let Some(gpu) = self.scene.gpu_mut() {
            let gpu_timings = gpu.take_timings();
            phases.acquire = gpu_timings.acquire;
            phases.worst_acquire = gpu_timings.worst_acquire;
            phases.misses = gpu_timings.misses;
        }

        // Chaque fenêtre d'écran prend la souris au-dessus d'un animal (ou
        // pendant qu'on en tient un), et la laisse passer ailleurs : le fil
        // principal le fait, quand cela change.
        let mark = Instant::now();
        self.update_hits(cursor, alpha, now);
        phases.hittest += mark.elapsed();
        if !settling {
            self.take_snapshots(now);
        }

        self.schedule(now, settling);
        self.record(
            now,
            alpha,
            &phases,
            beat_timing,
            timer_late,
            cadence,
            allocs_before,
        );
    }

    /// Les fenêtres d'écran qui prennent la souris : celle où se trouve le
    /// curseur, s'il est sur un animal ; toutes, pendant qu'on en tient un.
    fn update_hits(&mut self, cursor: Option<Vec2>, alpha: f32, now: Instant) {
        let dragging = self.pointer.dragging.is_some();
        let mut hits = std::mem::take(&mut self.hits);
        hits.clear();
        hits.extend(self.scene.screens().map(|screen| {
            let over =
                cursor.is_some_and(|c| screen.rect.contains(c) && self.pet_at(c, alpha).is_some());
            (screen.key, dragging || over)
        }));
        if hits != self.reported_hits {
            let newly = hits.iter().any(|&(key, on)| {
                on && !self
                    .reported_hits
                    .iter()
                    .any(|&(k, before)| k == key && before)
            });
            // Le curseur arrive sur un animal : on va peut-être l'attraper.
            if newly && !dragging {
                self.full_rate_until = Some(now + HOVER_FULL_RATE);
            }
            self.reported_hits.clone_from(&hits);
            self.report(Report::Hittest(hits.clone()));
        }
        self.hits = hits;
    }

    /// Les nouveaux venus (personnages secondaires des scènes desktopPet
    /// compris) prennent le dessin de leur espèce ; ceux qui sont partis
    /// s'oublient ; le fil principal apprend ce qui a changé.
    fn sync_pets(&mut self) {
        let generation = self.flock.generation();
        if self.reported_generation == Some(generation) {
            return;
        }
        self.reported_generation = Some(generation);
        let pets = self.flock.pets();
        let alive: HashSet<u32> = pets.iter().map(mouflon_core::Pet::id).collect();
        self.arts.retain(|id, _| alive.contains(id));
        self.def_arts
            .retain(|(def, _)| pets.iter().any(|p| Arc::ptr_eq(p.species(), def)));
        // Un personnage secondaire prend le dessin de son espèce.
        let missing: Vec<(u32, Arc<PetArt>)> = pets
            .iter()
            .filter(|p| !self.arts.contains_key(&p.id()))
            .filter_map(|p| {
                let (_, art) = self
                    .def_arts
                    .iter()
                    .find(|(d, _)| Arc::ptr_eq(d, p.species()))?;
                Some((p.id(), Arc::clone(art)))
            })
            .collect();
        let summary = pets
            .iter()
            .map(|p| PetSummary {
                id: p.id(),
                is_ewe: p.can_mother(),
                def: Arc::clone(p.species()),
                extra: p.is_child() || p.is_leaving(),
                lambs: self
                    .wishes
                    .iter()
                    .filter(|(mother, _)| *mother == p.id())
                    .map(|(_, place)| *place)
                    .collect(),
            })
            .collect();
        self.wishes.clear();
        for (id, art) in missing {
            self.fit_pet(id, art);
        }
        self.report(Report::Flock {
            generation,
            done: self.done,
            pets: summary,
        });
    }

    /// Les instantanés de la scène prévus, à leur heure.
    fn take_snapshots(&mut self, now: Instant) {
        let Some((dir, times)) = &mut self.snapshots else {
            return;
        };
        let Some(&at) = times.first() else {
            return;
        };
        if now < at {
            return;
        }
        times.remove(0);
        let dir = dir.clone();
        let started = self.stats.as_ref().map_or(now, FrameStats::begun);
        let name = format!(
            "instantane-{:05.1}s",
            now.duration_since(started).as_secs_f32()
        );
        let written = self.scene.snapshot(&dir, &name, 1.0);
        let took = now.elapsed();
        log::info!(
            "instantané de la scène : {} image(s) en {} ms",
            written.len(),
            took.as_millis()
        );
        self.note(format_args!(
            "instantané de la scène ({name}, {} image(s), {:.1} ms)",
            written.len(),
            took.as_secs_f32() * 1000.0
        ));
    }

    /// Prochain réveil : au battement de l'horloge de l'écran, à la cadence
    /// voulue, ou par la minuterie quand l'horloge manque ou se tait.
    fn schedule(&mut self, now: Instant, settling: bool) {
        let moving = self.clock.as_ref().map_or(self.refresh_hz, |clock| {
            self.refresh_hz.max(f64::from(clock.max_fps()))
        });
        // Le curseur sur un animal, ou le bouton appuyé : on va sans doute
        // l'attraper. L'écran passe à pleine cadence d'avance.
        let anticipating =
            self.pointer.pressed.is_some() || self.full_rate_until.is_some_and(|until| now < until);
        // Une annonce (ou un retour) se joue à pleine cadence jusqu'au bout,
        // même quand le troupeau n'y est plus : la soucoupe repart, la bulle
        // s'efface.
        let showing = self.show.is_some() || self.farewell_wanted.is_some();
        let fps = if self.live.paused {
            None
        } else if settling || self.pointer.dragging.is_some() || anticipating || showing {
            Some(moving)
        } else {
            match self.flock.activity() {
                Some(Activity::Moving) => {
                    // Toute la cadence de l'écran pour un mouvement vif ; la
                    // moitié (ProMotion) quand tous vont leur petit train.
                    let needed = self.flock.smooth_rate();
                    if f64::from(needed) >= moving {
                        let hold = now + FULL_RATE_HOLD;
                        self.full_rate_until =
                            Some(self.full_rate_until.map_or(hold, |until| until.max(hold)));
                    }
                    if self.full_rate_until.is_some_and(|until| now < until) {
                        Some(moving)
                    } else {
                        Some(moving_rate(needed, moving))
                    }
                }
                Some(Activity::Idle) => Some(IDLE_FPS),
                Some(Activity::Asleep) => Some(ASLEEP_FPS),
                None => None,
            }
        };
        self.cadence = fps.unwrap_or(0.0);
        let Some(fps) = fps else {
            // Rien ne bouge : l'horloge s'arrête, on passe de temps en temps.
            if let Some(clock) = &self.clock {
                clock.set_running(false);
            }
            self.shown_at = None;
            let wake = Duration::from_millis(if self.live.paused { 250 } else { 500 });
            self.next_frame = Some(now + wake);
            if self.stats.is_some() {
                platform::diag::expect_frame(wake);
            }
            return;
        };
        let interval = Duration::from_secs_f64(1.0 / fps);
        self.silence = interval.mul_f32(2.5).clamp(CLOCK_SILENT_MIN, CLOCK_SILENT);
        let wake = match &self.clock {
            Some(clock) => {
                clock.set_rate(fps as f32);
                clock.set_running(true);
                match self.clock_beat.map(|beat| now.duration_since(beat)) {
                    // Tant que l'horloge bat, c'est elle qui réveille.
                    Some(silent) if silent < self.silence => self.silence,
                    // Muette depuis longtemps : l'écran dort, rien ne se voit.
                    Some(silent) if silent > CLOCK_ASLEEP => Duration::from_millis(100),
                    _ => interval,
                }
            }
            None => interval,
        };
        self.next_frame = Some(now + wake);
        if self.stats.is_some() {
            // L'horloge qui bat amène l'image suivante dans un intervalle ;
            // sinon, c'est la minuterie.
            let clocked = self.clock.is_some() && wake == self.silence;
            platform::diag::expect_frame(if clocked { interval } else { wake });
        }
    }

    /// Le journal d'un essai : l'image, ses parties, les premiers animaux.
    #[expect(clippy::too_many_arguments, reason = "tout ce que l'image a mesuré")]
    fn record(
        &mut self,
        now: Instant,
        alpha: f32,
        phases: &crate::stats::Phases,
        beat: Option<crate::stats::BeatTiming>,
        timer_late: Option<f32>,
        cadence: f64,
        allocs_before: Option<platform::AllocCounts>,
    ) {
        if self.stats.is_none() {
            return;
        }
        let textures = self.scene.image_usage();
        let (decoded, evicted) = self.scene.take_image_counts();
        let refresh = self
            .clock
            .as_ref()
            .map_or(0.0, |clock| clock.frame_duration() * 1000.0);
        let mut arts: Vec<&Arc<PetArt>> = Vec::new();
        for art in self.arts.values() {
            if !arts.iter().any(|a| Arc::ptr_eq(a, art)) {
                arts.push(art);
            }
        }
        let image_bytes = arts.iter().map(|a| a.image_bytes()).sum();
        let events = std::mem::take(&mut self.commands);
        let Some(stats) = &mut self.stats else {
            return;
        };
        stats.set_textures(textures);
        stats
            .counters
            .images_decoded
            .fetch_add(decoded, std::sync::atomic::Ordering::Relaxed);
        stats
            .counters
            .images_evicted
            .fetch_add(evicted, std::sync::atomic::Ordering::Relaxed);
        stats.set_images(image_bytes);
        let pets = self
            .flock
            .pets()
            .iter()
            .take(crate::stats::DETAILED_PETS)
            .map(|p| {
                let view = p.view(alpha);
                crate::stats::PetLine {
                    id: p.id(),
                    name: p.def().meta.name.as_str(),
                    behavior: p.behavior_name(),
                    frame: view.sprite_frame,
                    x: view.ground.x,
                    y: view.ground.y,
                    vx: p.velocity().x,
                    vy: p.velocity().y,
                    support: match p.support() {
                        Support::Ground { .. } => 's',
                        Support::Air => 'a',
                        Support::Held => 't',
                    },
                    roll: view.pose.roll,
                }
            });
        let allocs = allocs_before.map_or((0, 0), |before| {
            let after = platform::alloc_counts();
            (
                after.main_allocs.saturating_sub(before.main_allocs),
                after.main_bytes.saturating_sub(before.main_bytes),
            )
        });
        let record = crate::stats::FrameRecord {
            started: now,
            beat,
            timer_late,
            cadence,
            refresh,
            phases: *phases,
            allocs,
            events,
        };
        stats.record(&record, pets);
        if self.trial_end.is_some_and(|end| now >= end) {
            self.trial_end = None;
            let bilan = stats.finish();
            log::info!("essai terminé : {bilan}");
            self.report(Report::TrialOver);
        }
    }

    /// Avant de quitter : l'horloge s'arrête (sur son fil), et le journal
    /// d'un essai interrompu reçoit son bilan.
    pub fn shutdown(&mut self) {
        self.clock = None;
        if let Some(stats) = &mut self.stats {
            stats.finish();
        }
    }
}

/// Le rythme d'une foule. Tant que la préparation des images tient dans sa
/// part d'un cœur ([`CROWD_SHARE`]), chacun est redessiné à son rythme : à
/// chaque image s'il bouge, trente fois par seconde au repos, vingt
/// endormi. Au-delà (des centaines de moutons dessinés), ces rythmes
/// s'espacent tous ensemble, peu à peu, jusqu'à ce que le travail tienne de
/// nouveau ; ils se resserrent quand la foule s'éclaircit. La place de
/// chacun, elle, suit toujours l'écran à chaque image : seul le dessin
/// (les pattes, la tête, la laine) attend son tour, comme au dessin animé.
#[derive(Clone, Copy, Debug, PartialEq)]
struct Crowd {
    /// De combien les rythmes sont espacés (1 : pas du tout).
    stretch: f64,
    /// Part d'un cœur que prend la préparation des images, lissée.
    load: f64,
}

impl Crowd {
    const fn new() -> Self {
        Self {
            stretch: 1.0,
            load: 0.0,
        }
    }

    /// Une image a demandé `spent` de préparation, au rythme de `fps` images
    /// par seconde.
    fn measure(&mut self, spent: Duration, fps: f64) {
        let share = spent.as_secs_f64() * fps.max(0.0);
        // Lissée sur une vingtaine d'images : une image lourde (un animal
        // qui arrive) ne change rien.
        self.load += (share - self.load) * 0.05;
        if self.load > CROWD_SHARE {
            self.stretch = (self.stretch * 1.02).min(CROWD_STRETCH_MAX);
        } else if self.load < CROWD_SHARE * 0.6 {
            // Entre les deux, on ne touche à rien : le rythme ne bat pas.
            self.stretch = (self.stretch / 1.01).max(1.0);
        }
    }

    /// Le rythme d'un animal (dessins par seconde) sur un écran à `screen`
    /// images par seconde ; `None` : à chaque image.
    fn rate(&self, activity: Activity, screen: f64) -> Option<f64> {
        match activity {
            Activity::Moving if self.stretch <= 1.0 => None,
            Activity::Moving => Some((screen / self.stretch).max(CROWD_MOVING_FPS)),
            Activity::Idle => Some((IDLE_FPS / self.stretch).max(CROWD_IDLE_FPS)),
            Activity::Asleep => Some((ASLEEP_FPS / self.stretch).max(CROWD_ASLEEP_FPS)),
        }
    }
}

/// Le rattrapage de la simulation. Après une image en retard, la
/// simulation avance du temps perdu (un dixième de seconde au plus) : pour
/// quelques moutons, cela ne coûte rien. Dans une foule où chaque pas pèse,
/// rattraper un dixième de seconde prendrait plus d'une image, qui serait
/// en retard à son tour, et ainsi de suite. On ne rattrape alors que ce qui
/// tient en [`SIM_CATCH_UP`] : les animaux ralentissent un instant au lieu
/// de saccader.
#[derive(Clone, Copy, Debug, PartialEq)]
struct SimPace {
    /// Temps de calcul par seconde simulée, lissé.
    share: f64,
}

impl SimPace {
    const fn new() -> Self {
        Self { share: 0.0 }
    }

    /// `spent` de calcul pour `simulated` secondes simulées.
    fn measure(&mut self, spent: Duration, simulated: f32) {
        // Sans un pas entier, rien n'a été simulé.
        if simulated >= TICK {
            let share = spent.as_secs_f64() / f64::from(simulated);
            self.share += (share - self.share) * 0.1;
        }
    }

    /// Le plus long temps à simuler en une image (secondes) : jamais moins
    /// d'un pas, jamais plus que ce que le troupeau rattrape de toute façon.
    fn allowance(&self) -> f32 {
        if self.share <= 0.0 {
            return 0.1;
        }
        ((SIM_CATCH_UP / self.share) as f32).clamp(TICK, 0.1)
    }
}

/// Cadence pour des animaux qui bougent : la plus lente qui divise celle de
/// l'écran (`full`) sans descendre sous celle qu'il leur faut (`needed`).
/// Un diviseur : l'écran montre chaque image le même nombre de fois, et le
/// mouvement reste régulier.
fn moving_rate(needed: f32, full: f64) -> f64 {
    let needed = f64::from(needed);
    if !needed.is_finite() || needed >= full {
        return full;
    }
    let divisor = (full / needed.max(1.0)).floor().max(1.0);
    full / divisor
}

/// Temps écoulé depuis `mark`, qui passe à maintenant.
fn lap(mark: &mut Instant) -> Duration {
    let now = Instant::now();
    let lap = now - *mark;
    *mark = now;
    lap
}

/// Un événement d'un animal, pour le journal d'un essai.
fn describe(event: &PetEvent, pet: &mouflon_core::Pet) -> String {
    match event {
        PetEvent::Landed { impact } => format!("atterrit à {impact:.0} pt/s"),
        PetEvent::Poked => "touché du bout de la souris".to_owned(),
        PetEvent::Thrown { speed } => format!("lancé à {speed:.0} pt/s"),
        PetEvent::Bumped { speed } => format!("heurté, repart à {speed:.0} pt/s"),
        PetEvent::Met { other } => format!("rencontre n° {other}"),
        PetEvent::Startled => "sursaute (pointeur rapide)".to_owned(),
        PetEvent::Napping => "s'endort (utilisateur parti)".to_owned(),
        PetEvent::Woken => "se réveille (utilisateur revenu)".to_owned(),
        PetEvent::HitSaucer => "percute une soucoupe".to_owned(),
        PetEvent::MaternalCall => "appelle son agneau, bouche fermée".to_owned(),
        PetEvent::Sound { sound, clip } => format!(
            "son {} ({})",
            pet.def()
                .sounds
                .get(*sound)
                .map_or("?", |s| s.name.as_str()),
            clip + 1
        ),
    }
}

/// Ce sur quoi se tient une surface, pour le journal d'un essai.
fn surface_name(
    world: &World,
    windows: &[platform::OsWindow],
    surface: mouflon_core::world::SurfaceId,
) -> String {
    if world
        .surface(surface)
        .is_some_and(|s| s.kind == mouflon_core::SurfaceKind::Floor)
    {
        return "le sol de l'écran".to_owned();
    }
    if let Some(pet) = mouflon_core::physics::surface_pet(surface) {
        return format!("le dos de n° {pet}");
    }
    let window = surface >> 8;
    if window == mouflon_core::DOCK_ID {
        return "le Dock".to_owned();
    }
    match windows.iter().find(|w| w.id == window) {
        Some(w) => format!(
            "la fenêtre n° {} de {} (niveau {}, {:.0}×{:.0} en {:.0},{:.0})",
            w.id, w.owner, w.layer, w.frame.w, w.frame.h, w.frame.x, w.frame.y
        ),
        None => format!("la fenêtre n° {window} (plus au relevé)"),
    }
}

// ——— Le fil de l'affichage ———

/// L'affichage sur son propre fil : de quoi lui parler.
#[derive(Debug)]
pub struct DisplayThread {
    commands: Sender<Command>,
    clocks: Sender<Option<platform::ThreadClock>>,
    wake: Arc<platform::LoopWaker>,
    beats: Arc<platform::Beats>,
    /// Demandes envoyées (voir le début du module).
    sent: u64,
    thread: Option<JoinHandle<()>>,
}

impl DisplayThread {
    /// Lance le fil de l'affichage, qui fait son [`Display`] avec `init`.
    /// `None` s'il n'a pas pu l'être.
    pub fn spawn(init: Init) -> Option<Self> {
        let (commands, command_receiver) = std::sync::mpsc::channel();
        let (clocks, clock_receiver) = std::sync::mpsc::channel();
        let (ready, waiting) = std::sync::mpsc::channel();
        let beats = Arc::new(platform::Beats::default());
        let thread_beats = Arc::clone(&beats);
        let thread = std::thread::Builder::new()
            .name("mouflon-affichage".into())
            .spawn(move || {
                platform::raise_thread_priority();
                let Some(wake) = platform::LoopWaker::current() else {
                    let _ = ready.send(None);
                    return;
                };
                let wake = Arc::new(wake);
                if ready.send(Some(Arc::clone(&wake))).is_err() {
                    return;
                }
                let display = Display::new(init);
                run(display, &command_receiver, &clock_receiver, &thread_beats);
            });
        let thread = match thread {
            Ok(thread) => thread,
            Err(e) => {
                log::error!("impossible de lancer le fil de l'affichage : {e}");
                return None;
            }
        };
        let Some(wake) = waiting.recv().ok().flatten() else {
            log::error!("le fil de l'affichage n'a pas de boucle d'événements");
            return None;
        };
        Some(Self {
            commands,
            clocks,
            wake,
            beats,
            sent: 0,
            thread: Some(thread),
        })
    }

    /// Une demande, que l'affichage traite tout de suite.
    pub fn send(&mut self, command: Command) {
        if self.commands.send(command).is_ok() {
            self.sent += 1;
            self.wake.wake();
        }
    }

    /// Une demande qui attend le prochain réveil de l'affichage.
    pub fn send_quiet(&mut self, command: Command) {
        if self.commands.send(command).is_ok() {
            self.sent += 1;
        }
    }

    /// Demandes envoyées jusqu'ici.
    #[must_use]
    pub fn sent(&self) -> u64 {
        self.sent
    }

    /// Une horloge de l'écran pour ce fil (à créer sur le fil principal,
    /// seul à pouvoir demander l'écran).
    #[must_use]
    pub fn new_clock(&self) -> Option<platform::ThreadClock> {
        platform::ThreadClock::new(Arc::clone(&self.beats), Arc::clone(&self.wake))
    }

    /// Donne une horloge au fil (l'ancienne s'arrête).
    pub fn set_clock(&self, clock: Option<platform::ThreadClock>) {
        if self.clocks.send(clock).is_ok() {
            self.wake.wake();
        }
    }

    /// Arrête le fil et attend qu'il ait fini (journal d'un essai compris).
    pub fn stop(&mut self) {
        self.send(Command::Quit);
        if let Some(thread) = self.thread.take() {
            let _ = thread.join();
        }
    }
}

/// La boucle du fil de l'affichage : les demandes, puis une image à chaque
/// battement (ou à l'heure de la minuterie), et l'attente.
fn run(
    mut display: Display<platform::ThreadClock>,
    commands: &Receiver<Command>,
    clocks: &Receiver<Option<platform::ThreadClock>>,
    beats: &platform::Beats,
) {
    if display.stats.is_some() {
        // Les allocations comptées et la pile relevée quand une image
        // tarde : celles de ce fil, qui fait les images.
        platform::set_alloc_counting(true);
        platform::diag::register_frame_thread();
    }
    loop {
        let quit = platform::with_release_pool(|| {
            while let Ok(clock) = clocks.try_recv() {
                display.set_clock(clock);
            }
            while let Ok(command) = commands.try_recv() {
                display.command(command);
            }
            if display.quitting() {
                return true;
            }
            if let Some(beat) = beats.take() {
                display.beat_received();
                if display.started() && display.clock_running() {
                    display.frame(Some(beat), beats.take_coalesced());
                }
            } else if display.timer_due(Instant::now()) {
                display.frame(None, 0);
            }
            let now = Instant::now();
            let wait = display.wake_at().map_or(Duration::from_millis(250), |at| {
                at.saturating_duration_since(now)
            });
            platform::run_loop_for(wait.min(Duration::from_millis(250)));
            false
        });
        if quit {
            break;
        }
    }
    display.shutdown();
}

#[cfg(test)]
mod tests {
    use std::sync::mpsc::Receiver;

    use super::*;
    use crate::scene::{ScreenInfo, Surface};

    /// Une horloge qui ne bat jamais : les images viennent de la minuterie.
    struct Silent;

    impl Clock for Silent {
        fn max_fps(&self) -> f32 {
            60.0
        }
        fn is_running(&self) -> bool {
            false
        }
        fn frame_duration(&self) -> f64 {
            1.0 / 60.0
        }
        fn set_running(&self, _running: bool) {}
        fn set_rate(&self, _fps: f32) {}
    }

    fn display() -> (Display<Silent>, Receiver<Report>) {
        let (sender, receiver) = std::sync::mpsc::channel();
        let mut display = Display::new(Init {
            live: Live {
                paused: false,
                sounds: false,
                collisions: false,
                realistic: true,
                surprises: false,
                frontmost_only: false,
                walk_under_dock: false,
                mood: mouflon_core::Mood::Even,
            },
            dark: false,
            watcher: None,
            stats: None,
            reports: Box::new(move |report| {
                let _ = sender.send(report);
            }),
        });
        // Le curseur des essais, pas celui de la machine qui les fait.
        display.system_cursor = false;
        (display, receiver)
    }

    /// Un écran de 1200 × 800 points, et sa fenêtre (clé 7).
    fn screens() -> Command {
        let frame = Rect::new(0.0, 0.0, 1200.0, 800.0);
        Command::Screens {
            screens: vec![Screen {
                frame,
                usable: Rect::new(0.0, 25.0, 1200.0, 750.0),
                scale: 1.0,
            }],
            overlays: vec![Overlay {
                info: ScreenInfo {
                    key: 7,
                    rect: frame,
                    scale: 1.0,
                    hz: 60.0,
                },
                surface: Surface::Layers(platform::SceneLayer::new()),
            }],
        }
    }

    fn builtin() -> (Arc<PetDef>, Arc<PetArt>) {
        let def = Arc::new(crate::library::builtin_pet().expect("animal intégré"));
        let art = Arc::new(PetArt::load(&def).expect("dessin"));
        (def, art)
    }

    /// Les nouvelles de l'affichage depuis la dernière fois.
    fn reports(receiver: &Receiver<Report>) -> Vec<Report> {
        receiver.try_iter().collect()
    }

    fn last_flock(reports: &[Report]) -> Option<(u64, Vec<PetSummary>)> {
        reports.iter().rev().find_map(|r| match r {
            Report::Flock { done, pets, .. } => Some((*done, pets.clone())),
            _ => None,
        })
    }

    /// Comme dans l'essai du 2 novembre : seulement sur l'app au premier
    /// plan (le Terminal, qui lançait l'essai), un mouton broutait sur le
    /// bord d'une vieille fenêtre du Terminal cachée derrière le Finder, au
    /// milieu de la liste des fichiers. Une fenêtre d'une autre app ne porte
    /// personne, mais cache toujours le rebord qu'elle couvre.
    #[test]
    fn other_apps_windows_still_hide_the_ledges_behind_them() {
        let (mut display, _receiver) = display();
        display.command(screens());
        let window = |id, pid, x: f64, y: f64, w: f64, h: f64| platform::OsWindow {
            id,
            pid,
            frame: platform::OsRect { x, y, w, h },
            layer: 0,
            owner: String::new(),
        };
        // Le Finder devant (de 300 à 700), le Terminal derrière (de 100 à 900).
        display.os_windows = vec![
            window(2, 20, 300.0, 200.0, 400.0, 400.0),
            window(1, 10, 100.0, 300.0, 800.0, 300.0),
        ];
        display.frontmost = Some(10);
        display.live.frontmost_only = true;
        display.refresh_windows(Instant::now());
        let mut ledges: Vec<(f32, f32, f32)> = display
            .world
            .surfaces()
            .iter()
            .filter(|s| s.kind == mouflon_core::SurfaceKind::Window)
            .map(|s| (s.x0, s.x1, s.y))
            .collect();
        ledges.sort_by(|a, b| a.0.total_cmp(&b.0));
        assert_eq!(ledges, [(100.0, 300.0, 300.0), (700.0, 900.0, 300.0)]);
        // Sans cette option, on marche aussi sur le Finder.
        display.live.frontmost_only = false;
        display.refresh_windows(Instant::now());
        assert!(
            display
                .world
                .surfaces()
                .iter()
                .any(|s| s.kind == mouflon_core::SurfaceKind::Window && s.y == 200.0)
        );
    }

    #[test]
    fn a_ewe_can_arrive_followed_by_her_lamb_and_a_ram_or_a_ewe_be_asked_for() {
        use mouflon_core::Sex;
        let (mut display, receiver) = display();
        display.command(screens());
        // Le Bleu du Maine : des béliers et des brebis, chacun son dessin.
        let def = Arc::new(crate::library::builtin_pet().unwrap());
        let art = Arc::new(PetArt::load(&def).unwrap());
        let arrive = |display: &mut Display<Silent>, sex, with_lamb| {
            display.command(Command::Spawn {
                def: Arc::clone(&def),
                art: Arc::clone(&art),
                sex: Some(sex),
                lambs: if with_lamb { vec![None] } else { Vec::new() },
                here: false,
                place: None,
            });
            display.flock.pets().last().unwrap().id()
        };
        let ewe = arrive(&mut display, Sex::Female, true);
        let ram = arrive(&mut display, Sex::Male, true);
        let other = arrive(&mut display, Sex::Female, false);
        let sex = |display: &Display<Silent>, id| {
            let pet = display.flock.pets().iter().find(|p| p.id() == id).unwrap();
            (pet.sex(), pet.is_ram(), pet.can_mother())
        };
        assert_eq!(sex(&display, ewe), (Sex::Female, false, true));
        assert_eq!(sex(&display, ram), (Sex::Male, true, false));
        display.command(Command::Start {
            settle: Duration::ZERO,
        });
        display.frame(None, 0);
        // Seule la brebis demandée avec son agneau l'attend, et une seule fois.
        let (_, pets) = last_flock(&reports(&receiver)).unwrap();
        let wants =
            |pets: &[PetSummary], id| !pets.iter().find(|p| p.id == id).unwrap().lambs.is_empty();
        assert!(wants(&pets, ewe));
        assert!(!wants(&pets, ram), "un bélier n'a pas d'agneau");
        assert!(!wants(&pets, other));
        // Son agneau arrive derrière elle, du haut de l'écran, et c'est le sien.
        let young = Arc::new(def.as_lamb(def.lamb_weeks()).unwrap());
        display.command(Command::SpawnLamb {
            mother: ewe,
            def: Arc::clone(&young),
            art: Arc::new(PetArt::load(&young).unwrap()),
            arriving: true,
            place: None,
        });
        let lamb = display.flock.pets().iter().find(|p| p.is_lamb()).unwrap();
        assert_eq!(lamb.mother(), Some(ewe));
        assert_eq!(lamb.support(), mouflon_core::Support::Air);
        let lamb = lamb.id();
        display.frame(None, 0);
        let (_, pets) = last_flock(&reports(&receiver)).unwrap();
        assert!(!wants(&pets, ewe), "dit une seule fois");
        assert!(pets.iter().any(|p| p.id == lamb));
        // Un agneau ne suit pas un bélier : la demande est sans effet.
        let before = display.flock.pets().len();
        display.command(Command::SpawnLamb {
            mother: ram,
            def: Arc::clone(&young),
            art: Arc::new(PetArt::load(&young).unwrap()),
            arriving: true,
            place: None,
        });
        assert_eq!(display.flock.pets().len(), before);
    }

    #[test]
    fn a_prepared_lamb_joins_its_mother_and_keeps_its_age_when_resized() {
        let (mut display, receiver) = display();
        display.command(screens());
        let mut mother = crate::library::builtin_pet().unwrap();
        mother.sex_ratio = mouflon_core::SexRatio::FEMALE;
        let mother = Arc::new(mother);
        let mother_art = Arc::new(PetArt::load(&mother).unwrap());
        display.command(Command::Spawn {
            def: Arc::clone(&mother),
            art: mother_art,
            sex: None,
            lambs: Vec::new(),
            here: false,
            place: None,
        });
        let mother_id = display.flock.pets()[0].id();
        for _ in 0..600 {
            display
                .flock
                .advance(mouflon_core::TICK, &display.world, &Input::default());
        }
        display.flock.get_mut(mother_id).unwrap().force("stand");
        let young = Arc::new(mother.as_lamb(2.0).unwrap());
        let young_art = Arc::new(PetArt::load(&young).unwrap());
        display.command(Command::SpawnLamb {
            mother: mother_id,
            def: Arc::clone(&young),
            art: young_art,
            arriving: false,
            place: None,
        });
        let lamb = display.flock.pets().iter().find(|p| p.is_lamb()).unwrap();
        let id = lamb.id();
        assert_eq!(lamb.mother(), Some(mother_id));
        let age = lamb.age();
        let larger = Arc::new(young.scaled(1.5));
        display.command(Command::RedrawPet {
            id,
            def: Arc::clone(&larger),
            art: Arc::new(PetArt::load(&larger).unwrap()),
            factor: 1.5,
        });
        let lamb = display.flock.get_mut(id).unwrap();
        assert_eq!(lamb.age(), age);
        assert_eq!(lamb.mother(), Some(mother_id));
        assert!((display.arts[&id].height - larger.physics.height).abs() < 0.01);
        display.command(Command::Start {
            settle: Duration::ZERO,
        });
        display.frame(None, 0);
        let (_, pets) = last_flock(&reports(&receiver)).unwrap();
        assert!(pets.iter().find(|p| p.id == mother_id).unwrap().is_ewe);
        assert!(!pets.iter().find(|p| p.id == id).unwrap().is_ewe);
    }

    #[test]
    fn a_ewe_drawn_in_images_is_not_offered_a_lamb() {
        // Lainou est un mouton en images : pas de tétée, donc pas d'article
        // « Ajouter son agneau » grisé dans son menu.
        let (mut display, receiver) = display();
        display.command(screens());
        let path = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("../../tests/donnees/lainou/pet.toml");
        let mut lainou = mouflon_core::load_pet(&path).unwrap();
        lainou.sex_ratio = mouflon_core::SexRatio::FEMALE;
        let lainou = Arc::new(lainou);
        display.command(Command::Spawn {
            def: Arc::clone(&lainou),
            art: Arc::new(PetArt::load(&lainou).unwrap()),
            sex: None,
            lambs: Vec::new(),
            here: false,
            place: None,
        });
        let id = display.flock.pets()[0].id();
        assert!(display.flock.pets()[0].is_ewe());
        display.command(Command::Start {
            settle: Duration::ZERO,
        });
        display.frame(None, 0);
        let (_, pets) = last_flock(&reports(&receiver)).unwrap();
        assert!(!pets.iter().find(|p| p.id == id).unwrap().is_ewe);
    }

    #[test]
    fn a_lamb_prepared_after_its_mother_disappears_is_not_spawned() {
        let (mut display, receiver) = display();
        display.command(screens());
        let mut mother = crate::library::builtin_pet().unwrap();
        mother.sex_ratio = mouflon_core::SexRatio::FEMALE;
        let mother = Arc::new(mother);
        display.command(Command::Spawn {
            def: Arc::clone(&mother),
            art: Arc::new(PetArt::load(&mother).unwrap()),
            sex: None,
            lambs: Vec::new(),
            here: false,
            place: None,
        });
        let id = display.flock.pets()[0].id();
        let young = Arc::new(mother.as_lamb(2.0).unwrap());
        display.flock.remove(id);
        display.sync_pets();
        let _ = reports(&receiver);
        display.command(Command::SpawnLamb {
            mother: id,
            def: Arc::clone(&young),
            art: Arc::new(PetArt::load(&young).unwrap()),
            arriving: false,
            place: None,
        });
        assert!(display.flock.pets().is_empty());
        display.sync_pets();
        let (done, pets) =
            last_flock(&reports(&receiver)).expect("la naissance refusée est aussi acquittée");
        assert_eq!(done, 3);
        assert!(pets.is_empty());
        assert!(
            !display
                .def_arts
                .iter()
                .any(|(def, _)| Arc::ptr_eq(def, &young))
        );
    }

    #[test]
    fn nothing_moves_before_the_start() {
        let (mut display, receiver) = display();
        let (def, art) = builtin();
        display.command(screens());
        display.command(Command::Spawn {
            def,
            art,
            sex: None,
            lambs: Vec::new(),
            here: false,
            place: None,
        });
        // Pas d'image avant `Start` : les animaux attendent leurs premières images.
        assert!(!display.timer_due(Instant::now()));
        display.command(Command::Start {
            settle: Duration::ZERO,
        });
        assert!(display.timer_due(Instant::now()));
        display.frame(None, 0);
        let (done, pets) = last_flock(&reports(&receiver)).expect("le troupeau décrit");
        assert_eq!(done, 3);
        assert_eq!(pets.len(), 1);
    }

    #[test]
    fn each_animal_wears_the_drawing_of_its_sex_and_is_described_by_its_species() {
        // Le Bleu du Maine (une brebis) avec un bélier d'essai : le dessin de
        // l'Awassi, dans `[appearance.male]`.
        let (mut display, receiver) = display();
        display.command(screens());
        let path =
            std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../pets/awassi/pet.toml");
        let ram = mouflon_core::load_pet(&path).unwrap();
        let mut def = crate::library::builtin_pet().unwrap();
        def.sex_ratio = mouflon_core::SexRatio::new(0.5).unwrap();
        def.set_variants(vec![(
            mouflon_core::Variant::Male,
            mouflon_core::VariantDrawing {
                appearance: ram.appearance.clone(),
                width: Some(ram.physics.width),
                stride: Some(ram.physics.stride),
                weeks: None,
            },
        )])
        .unwrap();
        let def = Arc::new(def);
        let art = Arc::new(PetArt::load(&def).unwrap());
        for _ in 0..8 {
            display.command(Command::Spawn {
                def: Arc::clone(&def),
                art: Arc::clone(&art),
                sex: None,
                lambs: Vec::new(),
                here: false,
                place: None,
            });
        }
        let worn = |display: &Display<Silent>| {
            let mut sexes = HashSet::new();
            for pet in display.flock.pets() {
                // Le dessin de chacun est celui dont le moteur a pris la taille.
                let drawn = &display.arts[&pet.id()];
                assert!((drawn.height - pet.def().physics.height).abs() < 0.01);
                sexes.insert(pet.variant().is_some());
            }
            sexes.len()
        };
        assert_eq!(worn(&display), 2, "des béliers et des brebis");
        display.command(Command::Start {
            settle: Duration::ZERO,
        });
        display.frame(None, 0);
        let (_, pets) = last_flock(&reports(&receiver)).expect("le troupeau décrit");
        assert!(pets.iter().all(|p| Arc::ptr_eq(&p.def, &def)));

        // Toute l'espèce grandit : chacun garde le dessin de son sexe.
        let big = Arc::new(def.scaled(1.5));
        display.command(Command::Redraw {
            changes: vec![Redraw {
                from: Arc::clone(&def),
                to: Arc::clone(&big),
                art: Arc::new(PetArt::load(&big).unwrap()),
                factor: 1.5,
            }],
            gone: Vec::new(),
        });
        assert_eq!(worn(&display), 2);
        display.frame(None, 0);
        let (_, pets) = last_flock(&reports(&receiver)).expect("décrit de nouveau");
        assert_eq!(pets.len(), 8);
        assert!(pets.iter().all(|p| Arc::ptr_eq(&p.def, &big)));
    }

    #[test]
    fn the_flock_is_described_with_the_requests_already_done() {
        let (mut display, receiver) = display();
        let (def, art) = builtin();
        display.command(screens());
        for _ in 0..3 {
            display.command(Command::Spawn {
                def: Arc::clone(&def),
                art: Arc::clone(&art),
                sex: None,
                lambs: Vec::new(),
                here: false,
                place: None,
            });
        }
        display.command(Command::Start {
            settle: Duration::ZERO,
        });
        display.frame(None, 0);
        let (done, pets) = last_flock(&reports(&receiver)).expect("le troupeau décrit");
        assert_eq!((done, pets.len()), (5, 3));
        assert!(pets.iter().all(|p| Arc::ptr_eq(&p.def, &def) && !p.extra));

        // Une autre taille pour l'un d'eux : la description suit, avec la
        // demande comptée.
        let big = Arc::new(def.scaled(2.0));
        let id = pets[1].id;
        display.command(Command::RedrawPet {
            id,
            def: Arc::clone(&big),
            art: Arc::clone(&art),
            factor: 2.0,
        });
        display.frame(None, 0);
        let (done, pets) = last_flock(&reports(&receiver)).expect("décrit de nouveau");
        assert_eq!(done, 6);
        let pet = pets.iter().find(|p| p.id == id).expect("toujours là");
        assert!(Arc::ptr_eq(&pet.def, &big));

        // Toute l'espèce change de dessin ; une définition partie s'en va.
        display.command(Command::Redraw {
            changes: vec![Redraw {
                from: Arc::clone(&def),
                to: Arc::clone(&big),
                art: Arc::clone(&art),
                factor: 2.0,
            }],
            gone: vec![Arc::clone(&big)],
        });
        display.frame(None, 0);
        let (_, pets) = last_flock(&reports(&receiver)).expect("décrit encore");
        // Celui qui était déjà grand s'en va ; les deux autres grandissent.
        assert_eq!(pets.len(), 2);
        assert!(pets.iter().all(|p| p.id != id && Arc::ptr_eq(&p.def, &big)));
    }

    #[test]
    fn the_window_takes_the_mouse_over_a_pet_only() {
        let (mut display, receiver) = display();
        let (def, art) = builtin();
        display.command(screens());
        display.command(Command::Spawn {
            def,
            art,
            sex: None,
            lambs: Vec::new(),
            here: false,
            place: None,
        });
        display.command(Command::Start {
            settle: Duration::ZERO,
        });
        // Il arrive (il tombe un moment).
        for _ in 0..240 {
            display.next_frame = None;
            display.frame(None, 0);
            std::thread::sleep(Duration::from_millis(1));
        }
        let pet = display.flock.pets()[0].position();
        let hits = |receiver: &Receiver<Report>| {
            reports(receiver).into_iter().rev().find_map(|r| match r {
                Report::Hittest(hits) => Some(hits),
                _ => None,
            })
        };
        let _ = hits(&receiver);
        // Sur lui (au milieu de son corps), la fenêtre prend la souris…
        display.command(Command::Cursor(Vec2::new(pet.x, pet.y - 10.0)));
        display.frame(None, 0);
        assert_eq!(hits(&receiver), Some(vec![(7, true)]));
        // … et la laisse passer ailleurs.
        display.command(Command::Cursor(Vec2::new(pet.x + 400.0, 100.0)));
        display.frame(None, 0);
        assert_eq!(hits(&receiver), Some(vec![(7, false)]));
        // Rien de neuf : rien n'est redit.
        display.frame(None, 0);
        assert_eq!(hits(&receiver), None);
    }

    #[test]
    fn a_pet_is_grabbed_dragged_and_let_go() {
        let (mut display, receiver) = display();
        let (def, art) = builtin();
        display.command(screens());
        display.command(Command::Spawn {
            def,
            art,
            sex: None,
            lambs: Vec::new(),
            here: false,
            place: None,
        });
        display.command(Command::Start {
            settle: Duration::ZERO,
        });
        for _ in 0..240 {
            display.next_frame = None;
            display.frame(None, 0);
            std::thread::sleep(Duration::from_millis(1));
        }
        let at = display.flock.pets()[0].position();
        let start = Vec2::new(at.x, at.y - 10.0);
        // Appuyé sur lui : pas encore pris (un simple clic le touche).
        display.command(Command::Cursor(start));
        display.command(Command::Press(Some(start)));
        display.frame(None, 0);
        assert_eq!(display.pointer.dragging, None);
        // Le curseur s'éloigne : il est pris là où l'on a cliqué, et suit.
        let to = Vec2::new(start.x + 120.0, start.y - 200.0);
        display.command(Command::Cursor(to));
        display.frame(None, 0);
        let id = display.flock.pets()[0].id();
        assert_eq!(display.pointer.dragging, Some(id));
        assert_eq!(display.flock.pets()[0].support(), Support::Held);
        // Il suit le curseur.
        for _ in 0..60 {
            display.next_frame = None;
            display.frame(None, 0);
            std::thread::sleep(Duration::from_millis(1));
        }
        let held = display.flock.pets()[0].position();
        assert!((held - at).length() > 100.0, "{held:?} {at:?}");
        // Pendant qu'on le tient, la fenêtre prend la souris partout.
        let hits = reports(&receiver).into_iter().rev().find_map(|r| match r {
            Report::Hittest(hits) => Some(hits),
            _ => None,
        });
        assert_eq!(hits, Some(vec![(7, true)]));
        // Lâché : il tombe.
        display.command(Command::Release(Some(to)));
        assert_eq!(display.pointer.dragging, None);
        display.frame(None, 0);
        assert_eq!(display.flock.pets()[0].support(), Support::Air);
    }

    /// Le 9 novembre 2026, sur macOS 27.2 : le visualiseur de clavier était à
    /// l'écran, mais absent de la liste des fenêtres que donne le système, et
    /// les moutons lâchés dessus tombaient jusqu'en bas de l'écran. Le relevé
    /// le prend maintenant dans ses réglages ; un mouton lâché au-dessus s'y
    /// pose, devant la fenêtre de l'app qui est derrière.
    #[test]
    fn a_sheep_dropped_on_the_keyboard_viewer_stands_on_it() {
        let (mut display, _receiver) = display();
        let (def, art) = builtin();
        display.command(screens());
        // Ce que donne le système : une app, et pas le clavier.
        let mut windows = vec![platform::OsWindow {
            id: 3,
            pid: 30,
            frame: platform::OsRect {
                x: 100.0,
                y: 500.0,
                w: 1000.0,
                h: 250.0,
            },
            layer: 0,
            owner: "Finder".into(),
        }];
        // Le clavier d'après son réglage, sur cet écran de 800 points de haut.
        let frame = platform::keyboard_viewer_frame("{{700, 300}, {320, 188}}", 800.0)
            .expect("cadre du clavier");
        assert_eq!((frame.x, frame.y), (700.0, 312.0));
        platform::add_keyboard_viewer(
            &mut windows,
            platform::OsWindow {
                id: platform::KEYBOARD_VIEWER_ID,
                pid: 40,
                frame,
                layer: platform::KEYBOARD_VIEWER_LEVEL,
                owner: platform::KEYBOARD_VIEWER_OWNER.into(),
            },
        );
        display.os_windows = windows;
        display.refresh_windows(Instant::now());
        // Le rebord du clavier : tout son bord du haut.
        let keyboard = |world: &World, surface: u64| {
            surface >> 8 == platform::KEYBOARD_VIEWER_ID
                && world
                    .surface(surface)
                    .is_some_and(|s| s.y == 312.0 && s.x0 == 700.0 && s.x1 == 1020.0)
        };
        assert!(
            display
                .world
                .surfaces()
                .iter()
                .any(|s| s.kind == mouflon_core::SurfaceKind::Window
                    && keyboard(&display.world, s.id)),
            "pas de rebord pour le clavier"
        );
        // Le mouton arrive, on le prend et on le tient au-dessus du clavier.
        display.command(Command::Spawn {
            def,
            art,
            sex: None,
            lambs: Vec::new(),
            here: false,
            place: None,
        });
        display.command(Command::Start {
            settle: Duration::ZERO,
        });
        for _ in 0..240 {
            display.next_frame = None;
            display.frame(None, 0);
            std::thread::sleep(Duration::from_millis(1));
        }
        let at = display.flock.pets()[0].position();
        let start = Vec2::new(at.x, at.y - 10.0);
        display.command(Command::Cursor(start));
        display.command(Command::Press(Some(start)));
        display.frame(None, 0);
        // Tenu immobile un moment (l'élan de la main s'éteint en quelques
        // centièmes de seconde) : on le pose, on ne le lance pas.
        let over = Vec2::new(860.0, 200.0);
        display.command(Command::Cursor(over));
        let held_since = Instant::now();
        while held_since.elapsed() < Duration::from_millis(800) {
            display.next_frame = None;
            display.frame(None, 0);
            std::thread::sleep(Duration::from_millis(2));
        }
        assert_eq!(display.flock.pets()[0].support(), Support::Held);
        // Lâché : il se pose sur le clavier, pas sur la fenêtre derrière.
        display.command(Command::Release(Some(over)));
        let mut support = Support::Air;
        for _ in 0..600 {
            display.next_frame = None;
            display.frame(None, 0);
            support = display.flock.pets()[0].support();
            if support != Support::Air {
                break;
            }
            std::thread::sleep(Duration::from_millis(1));
        }
        let Support::Ground { surface } = support else {
            panic!("toujours en l'air : {support:?}");
        };
        let pos = display.flock.pets()[0].position();
        assert!(
            keyboard(&display.world, surface),
            "posé ailleurs que sur le clavier : {pos:?}"
        );
    }

    #[test]
    fn a_right_click_on_a_pet_asks_for_its_menu_and_holds_it_still() {
        let (mut display, receiver) = display();
        let (def, art) = builtin();
        display.command(screens());
        display.command(Command::Spawn {
            def,
            art,
            sex: None,
            lambs: Vec::new(),
            here: false,
            place: None,
        });
        display.command(Command::Start {
            settle: Duration::ZERO,
        });
        for _ in 0..240 {
            display.next_frame = None;
            display.frame(None, 0);
            std::thread::sleep(Duration::from_millis(1));
        }
        let pet = display.flock.pets()[0].position();
        let id = display.flock.pets()[0].id();
        let _ = reports(&receiver);
        display.command(Command::Cursor(Vec2::new(pet.x, pet.y - 10.0)));
        display.command(Command::Menu);
        let asked = reports(&receiver)
            .into_iter()
            .any(|r| matches!(r, Report::PetMenu { id: asked, .. } if asked == id));
        assert!(asked, "le menu de l'animal sous le curseur");
        assert_eq!(display.menu_pet, Some(id));
        display.command(Command::MenuClosed);
        assert_eq!(display.menu_pet, None);
        // Ailleurs, pas de menu.
        display.command(Command::Cursor(Vec2::new(pet.x + 400.0, 100.0)));
        display.command(Command::Menu);
        assert!(
            !reports(&receiver)
                .iter()
                .any(|r| matches!(r, Report::PetMenu { .. }))
        );
    }

    #[test]
    fn a_crowd_catches_up_only_what_fits_in_a_frame() {
        // Vingt moutons : un pas coûte dix microsecondes. Tout le retard
        // est rattrapé (un dixième de seconde au plus).
        let mut pace = SimPace::new();
        assert_eq!(pace.allowance(), 0.1);
        for _ in 0..100 {
            pace.measure(Duration::from_micros(20), 2.0 * TICK);
        }
        assert_eq!(pace.allowance(), 0.1);
        // Une foule : 2,5 ms par pas, soit 0,3 s de calcul par seconde
        // simulée. Rattraper un dixième de seconde prendrait 30 ms, deux
        // images : on n'en rattrape que ce qui tient en 8 ms.
        for _ in 0..100 {
            pace.measure(Duration::from_micros(5000), 2.0 * TICK);
        }
        let allowed = pace.allowance();
        assert!((allowed - 0.0267).abs() < 0.002, "{allowed}");
        // Une image sans pas entier ne compte pas.
        let before = pace;
        pace.measure(Duration::from_millis(50), TICK * 0.5);
        assert_eq!(pace, before);
        // Une foule que la machine ne suit plus : un pas par image, pas
        // moins.
        for _ in 0..200 {
            pace.measure(Duration::from_millis(40), 2.0 * TICK);
        }
        assert_eq!(pace.allowance(), TICK);
    }

    #[test]
    fn a_small_flock_is_drawn_at_its_own_pace() {
        // Vingt moutons : une demi-milliseconde par image à 60 images par
        // seconde, 3 % d'un cœur. Rien ne s'espace.
        let mut crowd = Crowd::new();
        for _ in 0..600 {
            crowd.measure(Duration::from_micros(500), 60.0);
        }
        assert_eq!(crowd.stretch, 1.0);
        assert_eq!(crowd.rate(Activity::Moving, 60.0), None);
        assert_eq!(crowd.rate(Activity::Idle, 60.0), Some(IDLE_FPS));
        assert_eq!(crowd.rate(Activity::Asleep, 60.0), Some(ASLEEP_FPS));
    }

    #[test]
    fn a_crowd_spaces_out_its_drawings_until_the_work_fits_and_never_below_the_floor() {
        // Une foule : la préparation coûte d'autant moins que les dessins
        // sont espacés (ici 12 ms par image sans espacement, à 60 images
        // par seconde : 72 % d'un cœur).
        let mut crowd = Crowd::new();
        let cost = |stretch: f64| Duration::from_secs_f64(0.012 / stretch);
        for _ in 0..600 {
            crowd.measure(cost(crowd.stretch), 60.0);
        }
        // Le travail tient dans sa part, sans battre autour.
        assert!(crowd.stretch > 2.5 && crowd.stretch < 4.5, "{crowd:?}");
        assert!(crowd.load <= CROWD_SHARE * 1.05, "{crowd:?}");
        let settled = crowd.stretch;
        for _ in 0..600 {
            crowd.measure(cost(crowd.stretch), 60.0);
        }
        assert!(
            (crowd.stretch - settled).abs() < 0.2,
            "{crowd:?} après {settled}"
        );
        // Celui qui bouge est redessiné moins souvent que l'écran, les
        // autres moins souvent qu'au calme.
        let moving = crowd.rate(Activity::Moving, 60.0).expect("espacé");
        assert!((CROWD_MOVING_FPS..30.0).contains(&moving), "{moving}");
        let idle = crowd.rate(Activity::Idle, 60.0).expect("au repos");
        assert!((CROWD_IDLE_FPS..IDLE_FPS).contains(&idle), "{idle}");

        // Une foule que rien n'allège : les rythmes s'arrêtent à leur
        // plancher (douze dessins par seconde pour celui qui bouge).
        let mut crowd = Crowd::new();
        for _ in 0..2000 {
            crowd.measure(Duration::from_millis(30), 60.0);
        }
        assert_eq!(crowd.stretch, CROWD_STRETCH_MAX);
        assert_eq!(crowd.rate(Activity::Moving, 120.0), Some(CROWD_MOVING_FPS));
        assert_eq!(crowd.rate(Activity::Idle, 120.0), Some(CROWD_IDLE_FPS));
        assert_eq!(crowd.rate(Activity::Asleep, 120.0), Some(CROWD_ASLEEP_FPS));

        // La foule s'éclaircit : les rythmes reviennent.
        for _ in 0..2000 {
            crowd.measure(Duration::from_micros(300), 60.0);
        }
        assert_eq!(crowd.stretch, 1.0);
        assert_eq!(crowd.rate(Activity::Moving, 120.0), None);
    }

    #[test]
    fn slow_pets_halve_a_promotion_screen_but_never_drop_below_their_need() {
        // ProMotion : la moitié pour un animal au pas.
        assert_eq!(moving_rate(60.0, 120.0), 60.0);
        // Un écran à 60 Hz ne change pas.
        assert_eq!(moving_rate(60.0, 60.0), 60.0);
        // Toujours un diviseur de la cadence de l'écran, jamais sous le besoin.
        assert_eq!(moving_rate(60.0, 144.0), 72.0);
        assert_eq!(moving_rate(60.0, 240.0), 60.0);
        assert_eq!(moving_rate(60.0, 90.0), 90.0);
        // Un mouvement vif : toute la cadence.
        assert_eq!(moving_rate(f32::INFINITY, 120.0), 120.0);
    }

    /// Tant d'images d'un écran à 60 Hz, à l'heure de l'écran (`clock`) :
    /// le temps simulé ne dépend pas de la machine qui fait l'essai.
    fn run(display: &mut Display<Silent>, frames: usize, clock: &mut f64) {
        for _ in 0..frames {
            *clock += 1.0 / 60.0;
            display.next_frame = None;
            display.frame(
                Some(platform::Beat {
                    target: *clock,
                    vsync: *clock - 1.0 / 60.0,
                    called: *clock - 1.0 / 60.0,
                }),
                0,
            );
        }
    }

    /// Deux animaux arrivés à 300 et à 900, posés.
    fn two_settled(clock: &mut f64) -> (Display<Silent>, Receiver<Report>) {
        let (mut display, receiver) = display();
        let (def, art) = builtin();
        display.command(screens());
        for x in [300.0, 900.0] {
            display.command(Command::Spawn {
                def: Arc::clone(&def),
                art: Arc::clone(&art),
                sex: None,
                lambs: Vec::new(),
                here: false,
                place: Some(x),
            });
        }
        display.command(Command::Start {
            settle: Duration::ZERO,
        });
        run(&mut display, 240, clock);
        assert_eq!(display.flock.pets().len(), 2);
        let _ = reports(&receiver);
        (display, receiver)
    }

    fn shows(receiver: &Receiver<Report>) -> Vec<ShowReport> {
        reports(receiver)
            .into_iter()
            .filter_map(|r| match r {
                Report::Show(show) => Some(show),
                _ => None,
            })
            .collect()
    }

    #[test]
    fn a_ready_update_is_announced_then_the_flock_leaves_and_says_who_it_was() {
        for scene in [ShowScene::Herald, ShowScene::Saucer] {
            let mut clock = 10.0;
            let (mut display, receiver) = two_settled(&mut clock);
            display.command(Command::Farewell(scene));
            run(&mut display, 1, &mut clock);
            assert_eq!(shows(&receiver), [ShowReport::Counting]);
            // Neuf secondes plus tard, le compte tourne encore : tout le
            // monde est là, et la scène montre ce que l'annonce ajoute.
            run(&mut display, 9 * 60, &mut clock);
            assert_eq!(shows(&receiver), []);
            assert_eq!(display.flock.pets().len(), 2);
            let extras = display
                .scene
                .entities()
                .filter(|e| !matches!(e, crate::scene::Entity::Pet(_)))
                .count();
            match scene {
                ShowScene::Herald => assert_eq!(extras, 1, "la bulle"),
                ShowScene::Saucer => assert_eq!(extras, 3, "le faisceau, la soucoupe, le compte"),
            }
            // Zéro : ils partent, et l'affichage dit qui ils étaient.
            let mut told = Vec::new();
            for _ in 0..(12 * 60) {
                run(&mut display, 1, &mut clock);
                told.extend(shows(&receiver));
                if told.last() == Some(&ShowReport::Gone) {
                    break;
                }
            }
            let [ShowReport::Leaving(leavers), ShowReport::Gone] = &told[..] else {
                panic!("{scene:?} : {told:?}");
            };
            assert_eq!(leavers.len(), 2);
            // Là où ils étaient au moment de partir (ils se sont promenés
            // pendant le compte), sur l'écran.
            let places: Vec<f32> = leavers.iter().map(|l| l.x).collect();
            assert!(
                places.iter().all(|x| (0.0..=1200.0).contains(x)),
                "{places:?}"
            );
            assert!(display.flock.pets().is_empty());
            assert!(display.show.is_none());
            // Plus rien à l'écran.
            run(&mut display, 2, &mut clock);
            assert_eq!(display.scene.entities().count(), 0);
        }
    }

    #[test]
    fn a_click_on_a_sheep_during_the_count_puts_the_update_off() {
        let mut clock = 10.0;
        let (mut display, receiver) = two_settled(&mut clock);
        display.command(Command::Farewell(ShowScene::Saucer));
        run(&mut display, 4 * 60, &mut clock);
        assert_eq!(shows(&receiver), [ShowReport::Counting]);
        // Un clic à côté ne change rien.
        display.command(Command::Press(Some(Vec2::new(600.0, 100.0))));
        display.command(Command::Release(None));
        run(&mut display, 10, &mut clock);
        assert_eq!(shows(&receiver), []);
        // Un clic sur l'un d'eux : remis à plus tard, sans l'attraper.
        let pet = display.flock.pets()[0].position();
        display.command(Command::Press(Some(Vec2::new(pet.x, pet.y - 10.0))));
        display.command(Command::Cursor(Vec2::new(pet.x + 50.0, pet.y - 80.0)));
        run(&mut display, 3 * 60, &mut clock);
        display.command(Command::Release(None));
        assert_eq!(shows(&receiver), [ShowReport::Postponed]);
        assert_eq!(display.pointer.dragging, None);
        assert_eq!(display.flock.pets().len(), 2);
        assert!(display.show.is_none());
        run(&mut display, 2, &mut clock);
        assert!(
            display
                .scene
                .entities()
                .all(|e| matches!(e, crate::scene::Entity::Pet(_))),
            "la soucoupe est repartie"
        );
        // Remise avant même d'avoir commencé (le menu, pendant la pause).
        display.command(Command::Farewell(ShowScene::Herald));
        display.command(Command::Postpone);
        run(&mut display, 5, &mut clock);
        assert_eq!(shows(&receiver), [ShowReport::Postponed]);
        assert!(display.show.is_none());
    }

    #[test]
    fn with_nobody_on_screen_the_place_is_free_at_once() {
        let (mut display, receiver) = display();
        let mut clock = 10.0;
        display.command(screens());
        display.command(Command::Start {
            settle: Duration::ZERO,
        });
        run(&mut display, 5, &mut clock);
        display.command(Command::Farewell(ShowScene::Saucer));
        run(&mut display, 2, &mut clock);
        assert_eq!(
            shows(&receiver),
            [ShowReport::Leaving(Vec::new()), ShowReport::Gone]
        );
    }

    #[test]
    fn back_from_an_update_each_one_lands_at_its_place_and_one_shows_the_version() {
        for scene in [ShowScene::Herald, ShowScene::Saucer] {
            let (mut display, _receiver) = display();
            let (def, art) = builtin();
            let mut clock = 10.0;
            display.command(screens());
            display.command(Command::Start {
                settle: Duration::ZERO,
            });
            display.command(Command::Welcome {
                scene,
                label: "1.0.5".into(),
                places: vec![300.0, 900.0],
            });
            run(&mut display, 30, &mut clock);
            for x in [300.0, 900.0] {
                display.command(Command::Spawn {
                    def: Arc::clone(&def),
                    art: Arc::clone(&art),
                    sex: None,
                    lambs: Vec::new(),
                    here: false,
                    place: Some(x),
                });
            }
            run(&mut display, 1, &mut clock);
            let ys: Vec<f32> = display
                .flock
                .pets()
                .iter()
                .map(|p| p.position().y)
                .collect();
            match scene {
                // Du haut de l'écran ; ou de la lumière de la soucoupe, qui
                // les tient d'abord.
                ShowScene::Herald => assert!(ys.iter().all(|y| *y < 120.0), "{ys:?}"),
                ShowScene::Saucer => {
                    assert!(ys.iter().all(|y| *y > 200.0), "{ys:?}");
                    assert!(display.flock.pets().iter().all(mouflon_core::Pet::is_held));
                }
            }
            // Chacun se pose à sa place (la soucoupe les repose l'un après
            // l'autre : un peu plus long) ; ensuite, il va où il veut.
            let frames = match scene {
                ShowScene::Herald => 330,
                ShowScene::Saucer => 420,
            };
            let mut landed: Vec<(u32, f32)> = Vec::new();
            for _ in 0..frames {
                run(&mut display, 1, &mut clock);
                for pet in display.flock.pets() {
                    if !pet.is_held()
                        && matches!(pet.support(), Support::Ground { .. })
                        && !landed.iter().any(|(id, _)| *id == pet.id())
                    {
                        landed.push((pet.id(), pet.position().x));
                    }
                }
            }
            let mut xs: Vec<f32> = landed.iter().map(|(_, x)| *x).collect();
            xs.sort_by(f32::total_cmp);
            assert_eq!(xs.len(), 2, "{xs:?}");
            assert!(
                (xs[0] - 300.0).abs() < 40.0 && (xs[1] - 900.0).abs() < 40.0,
                "{xs:?}"
            );
            // La bulle est là un moment, puis tout redevient ordinaire.
            let over = |display: &Display<Silent>| {
                display
                    .scene
                    .entities()
                    .filter(|e| matches!(e, crate::scene::Entity::Over(_)))
                    .count()
            };
            assert_eq!(over(&display), 1, "la bulle de la version");
            run(&mut display, 20 * 60, &mut clock);
            assert!(display.show.is_none());
            assert!(
                display
                    .scene
                    .entities()
                    .all(|e| matches!(e, crate::scene::Entity::Pet(_)))
            );
            assert_eq!(display.flock.pets().len(), 2);
        }
    }

    #[test]
    fn a_scene_keeps_the_full_rate_to_its_end_even_with_the_flock_gone() {
        let mut clock = 10.0;
        let (mut display, receiver) = two_settled(&mut clock);
        display.command(Command::Farewell(ShowScene::Saucer));
        let mut gone = false;
        let mut alone = 0;
        for _ in 0..(20 * 60) {
            // (Comme si l'élan d'un mouvement vif était retombé : en vrai,
            // la scène dure plus longtemps que lui.)
            display.full_rate_until = None;
            run(&mut display, 1, &mut clock);
            if shows(&receiver).contains(&ShowReport::Gone) {
                gone = true;
                break;
            }
            // Tant que la scène se joue, l'écran bat : même quand la
            // soucoupe repart seule, le troupeau déjà emporté (sans
            // personne, d'ordinaire, l'affichage se repose).
            assert!(display.cadence > 0.0, "l'horloge s'est arrêtée");
            if display.flock.pets().is_empty() {
                alone += 1;
            }
        }
        assert!(
            alone > 30,
            "la soucoupe est repartie seule ({alone} images)"
        );
        assert!(gone);
        // Un retour qui se joue : l'annonce demandée attend qu'il soit fini.
        display.command(Command::Welcome {
            scene: ShowScene::Herald,
            label: "1.0.5".into(),
            places: Vec::new(),
        });
        display.command(Command::Farewell(ShowScene::Herald));
        run(&mut display, 5, &mut clock);
        assert!(matches!(display.show, Some(Show::Welcome(_))));
        assert_eq!(display.farewell_wanted, Some(ShowScene::Herald));
        assert_eq!(shows(&receiver), []);
    }

    #[test]
    fn an_announcement_asked_as_the_flock_arrives_waits_for_one_to_land() {
        let (mut display, receiver) = display();
        let (def, art) = builtin();
        let mut clock = 10.0;
        display.command(screens());
        display.command(Command::Spawn {
            def,
            art,
            sex: None,
            lambs: Vec::new(),
            here: false,
            place: Some(600.0),
        });
        display.command(Command::Start {
            settle: Duration::ZERO,
        });
        display.command(Command::Farewell(ShowScene::Saucer));
        // Il tombe encore : rien ne commence.
        run(&mut display, 3, &mut clock);
        assert!(display.show.is_none());
        assert_eq!(shows(&receiver), []);
        // Posé : l'annonce commence, et c'est bien la soucoupe.
        let mut began = false;
        for _ in 0..300 {
            run(&mut display, 1, &mut clock);
            if shows(&receiver).contains(&ShowReport::Counting) {
                began = true;
                break;
            }
        }
        assert!(began);
        assert!(
            display
                .flock
                .pets()
                .iter()
                .any(|pet| matches!(pet.support(), Support::Ground { .. }))
        );
        let Some(Show::Farewell(show)) = &display.show else {
            panic!("l'annonce");
        };
        assert_eq!(show.scene(), ShowScene::Saucer);
    }

    #[test]
    fn back_from_an_update_a_ewe_comes_with_each_of_her_lambs_at_its_own_place() {
        let (mut display, receiver) = display();
        let (def, art) = builtin();
        display.command(screens());
        display.command(Command::Start {
            settle: Duration::ZERO,
        });
        display.command(Command::Spawn {
            def: Arc::clone(&def),
            art: Arc::clone(&art),
            sex: Some(mouflon_core::Sex::Female),
            lambs: vec![Some(500.0), Some(650.0)],
            here: false,
            place: Some(300.0),
        });
        display.frame(None, 0);
        let (_, pets) = last_flock(&reports(&receiver)).expect("le troupeau");
        let ewe = pets.iter().find(|p| p.is_ewe).expect("la brebis");
        assert_eq!(ewe.lambs, [Some(500.0), Some(650.0)]);
        let ewe = ewe.id;
        // Leurs dessins prêts, chacun arrive à sa place, lié à elle ; un
        // agneau sans mère arrive seul, à la sienne.
        let young = Arc::new(def.as_lamb(def.lamb_weeks()).unwrap());
        let young_art = Arc::new(PetArt::load(&young).unwrap());
        for x in [500.0, 650.0] {
            display.command(Command::SpawnLamb {
                mother: ewe,
                def: Arc::clone(&young),
                art: Arc::clone(&young_art),
                arriving: true,
                place: Some(x),
            });
        }
        display.command(Command::Spawn {
            def: Arc::clone(&young),
            art: Arc::clone(&young_art),
            sex: None,
            lambs: Vec::new(),
            here: false,
            place: Some(900.0),
        });
        let mut xs: Vec<f32> = display
            .flock
            .pets()
            .iter()
            .filter(|p| p.mother() == Some(ewe))
            .map(|p| p.position().x)
            .collect();
        xs.sort_by(f32::total_cmp);
        assert_eq!(xs.len(), 2, "{xs:?}");
        assert!(
            (xs[0] - 500.0).abs() < 2.0 && (xs[1] - 650.0).abs() < 2.0,
            "{xs:?}"
        );
        let orphan = display
            .flock
            .pets()
            .iter()
            .find(|p| p.is_lamb() && p.mother().is_none())
            .expect("l'agneau seul");
        assert!(
            (orphan.position().x - 900.0).abs() < 2.0,
            "{:?}",
            orphan.position()
        );
        // Si elle ne peut pas être sa mère (ici, un bélier), il revient quand
        // même à sa place, seul.
        display.command(Command::Spawn {
            def: Arc::clone(&def),
            art: Arc::clone(&art),
            sex: Some(mouflon_core::Sex::Male),
            lambs: vec![Some(1200.0)],
            here: false,
            place: Some(1100.0),
        });
        display.frame(None, 0);
        let (_, pets) = last_flock(&reports(&receiver)).expect("le troupeau");
        let ram = pets
            .iter()
            .find(|p| p.lambs == [Some(1200.0)])
            .expect("le bélier");
        assert!(!ram.is_ewe);
        display.command(Command::SpawnLamb {
            mother: ram.id,
            def: Arc::clone(&young),
            art: Arc::clone(&young_art),
            arriving: true,
            place: Some(1200.0),
        });
        let alone = display
            .flock
            .pets()
            .iter()
            .find(|p| p.is_lamb() && (p.position().x - 1200.0).abs() < 2.0)
            .expect("revenu seul, à sa place");
        assert_eq!(alone.mother(), None);
    }
}
