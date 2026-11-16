//! L'app, sur le fil principal : les menus, les réglages, la bibliothèque des
//! animaux, les fenêtres des écrans et les événements du système.
//!
//! Ce qui fait les images — le troupeau, le monde, la scène et leur rythme —
//! est à l'affichage (voir [`crate::display`]) : sur son propre fil quand la
//! scène passe par des calques Core Animation, ici sinon. Le fil principal
//! lui envoie des demandes et reçoit ce qu'il doit savoir pour les menus.

use std::cell::Cell;
use std::collections::{HashMap, HashSet};
use std::rc::Rc;
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use mouflon_core::{PetDef, Rect, Screen, Vec2};
use mouflon_platform as platform;
use mouflon_render::PetArt;
use winit::application::ApplicationHandler;
use winit::event::{ElementState, MouseButton, WindowEvent};
use winit::event_loop::{ActiveEventLoop, ControlFlow, EventLoopProxy};
use winit::window::{Theme, WindowId};

use crate::display::{
    Command, Display, DisplayThread, Init, Leaver, Live, PetSummary, Redraw, Report, SETTLE,
    ShowReport, Surprise,
};
use crate::library::{Entry, Library};
use crate::menu::{Action, PetChoice, PetMenuState, Wanted};
use crate::screens::ScreenWindows;
use crate::settings::{Paths, Settings};
use crate::settings_window::Shown;
use crate::stats::{Counters, FrameStats, Notes};
use crate::watch::Watcher;

/// Relevé des écrans et du dossier des animaux (le reste se relève à côté).
const SCREEN_POLL: Duration = Duration::from_secs(2);
/// La compression des images d'une espèce attend ce temps après son
/// arrivée : les premières chutes passent avant.
const COMPRESS_DELAY: Duration = Duration::from_secs(3);
/// Images gardées sur le disque qui n'ont pas servi depuis ce temps : effacées.
const IMAGES_KEPT: Duration = Duration::from_secs(90 * 24 * 3600);
/// Mention de copyright de la fenêtre « À propos ».
const COPYRIGHT: &str = "© 2026 Alejandro San Martin Lamas";

#[derive(Debug)]
pub enum UserEvent {
    #[cfg(any(target_os = "macos", target_os = "windows"))]
    Menu(crate::menu::MenuEvent),
    /// Battement de l'horloge de l'écran, quand l'affichage est sur ce fil.
    Frame,
    /// Le dessin de cet animal (identifiant) est prêt.
    ArtReady(String),
    /// Les images de cet animal sont compressées (voir `App::compress_later`).
    ArtCompressed(String),
    /// Le dessin à sa nouvelle taille de cet animal (son numéro, la définition
    /// exacte attendue) est prêt (voir `App::set_pet_size`).
    PetSizeReady(u32, Arc<PetDef>),
    /// Le dessin de l’agneau est prêt; sa mère est revalidée à réception.
    LambReady {
        goal: LambGoal,
        from: Arc<PetDef>,
        entry: Entry,
    },
    /// L'aperçu d'une espèce (son rang) pour la galerie est prêt.
    GalleryPreview(u32, Vec<platform::GalleryImage>),
    /// Un article du menu d'un animal, un contrôle de la fenêtre des réglages.
    Ui(platform::UiEvent),
    /// Ce que le système dit de l'ouverture à l'ouverture de session (voir
    /// `App::ask_login_item`).
    LoginItem(platform::LoginItem),
    /// Ce que dit l'affichage.
    Display(Report),
    /// Ce que rapporte la recherche d'une nouvelle version (voir
    /// `crate::update`).
    Update(crate::update::Event),
}

/// Ce qu'un essai fait de lui-même à mi-parcours, pour passer aussi par un
/// départ et une arrivée.
#[derive(Clone, Copy, Debug)]
enum TrialStep {
    /// Le dernier arrivé de cette espèce dit au revoir.
    Goodbye(usize),
    /// Un animal de cette espèce arrive.
    Add(usize),
    /// Essai de charge : des animaux arrivent jusqu'à en avoir tant (des
    /// espèces du lancement, à tour de rôle).
    Crowd(usize),
}

/// La vitrine de la CI : à l'heure dite, la fenêtre des réglages s'ouvre,
/// puis son image est enregistrée (claire, puis avec ses remarques, claire
/// et sombre), avec le texte des menus ; l'app quitte ensuite.
#[derive(Debug)]
struct Showcase {
    dir: std::path::PathBuf,
    at: Instant,
    opened: bool,
}

/// Le troupeau, tel que l'affichage l'a décrit en dernier (et ce que le fil
/// principal lui a demandé depuis).
/// Pourquoi on prépare un agneau.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum LambGoal {
    /// Né auprès de cette brebis posée (« Ajouter son agneau »).
    Born(u32),
    /// Il suit cette brebis qui vient d'arriver : il tombe derrière elle,
    /// ou revient à sa place (le troupeau d'avant une mise à jour).
    Arriving(u32, Option<f32>),
    /// Un agneau seul : demandé dans la galerie, ou qui revient à sa place
    /// sans sa mère.
    Alone(Option<f32>),
}

#[derive(Debug, Default)]
#[cfg_attr(not(any(target_os = "macos", target_os = "windows")), allow(dead_code))]
struct FlockView {
    generation: Option<u64>,
    pets: Vec<PetSummary>,
}

impl FlockView {
    /// Le troupeau tel que l'affichage l'a décrit après avoir traité `done`
    /// demandes, avec les changements demandés depuis (`handed`, dont on
    /// oublie ceux que la description montre déjà).
    fn described(
        generation: u64,
        done: u64,
        pets: Vec<PetSummary>,
        handed: &mut Vec<(u64, Handed)>,
    ) -> Self {
        handed.retain(|(at, _)| *at > done);
        let mut flock = Self {
            generation: Some(generation),
            pets,
        };
        for (_, change) in handed.iter() {
            flock.apply(change);
        }
        flock
    }

    fn def_of(&self, id: u32) -> Option<&Arc<PetDef>> {
        self.pets.iter().find(|p| p.id == id).map(|p| &p.def)
    }

    /// Une définition peut être partagée par plusieurs nouveaux agneaux :
    /// chacun reçoit la taille choisie avant leur premier rapport.
    fn take_offspring_sizes(
        &self,
        waiting: &mut Vec<OffspringSize>,
        unseen: &[(u64, Arc<PetDef>)],
    ) -> Vec<(u32, usize)> {
        let mut sizes = Vec::new();
        waiting.retain_mut(|request| {
            for pet in self
                .pets
                .iter()
                .filter(|p| Arc::ptr_eq(&p.def, &request.def))
            {
                if !request.applied.contains(&pet.id) {
                    request.applied.push(pet.id);
                    if !sizes.iter().any(|(id, _)| *id == pet.id) {
                        sizes.push((pet.id, request.size));
                    }
                }
            }
            unseen.iter().any(|(_, d)| Arc::ptr_eq(d, &request.def))
        });
        sizes
    }

    fn apply(&mut self, handed: &Handed) {
        match handed {
            Handed::Pet(id, def) => {
                if let Some(pet) = self.pets.iter_mut().find(|p| p.id == *id) {
                    pet.def = Arc::clone(def);
                }
            }
            Handed::Defs(map) => {
                for pet in &mut self.pets {
                    if let Some((_, to)) = map.iter().find(|(from, _)| Arc::ptr_eq(from, &pet.def))
                    {
                        pet.def = Arc::clone(to);
                    }
                }
            }
        }
    }
}

#[derive(Debug)]
struct OffspringSize {
    def: Arc<PetDef>,
    size: usize,
    /// Les rapports peuvent décrire les frères l’un après l’autre.
    applied: Vec<u32>,
}

/// Un changement de définition demandé à l'affichage, qu'une description du
/// troupeau faite avant de le recevoir ne montre pas encore.
#[derive(Debug)]
enum Handed {
    /// Cet animal a pris cette définition (une autre taille).
    Pet(u32, Arc<PetDef>),
    /// Les animaux de chaque première définition ont pris la seconde.
    Defs(Vec<(Arc<PetDef>, Arc<PetDef>)>),
}

/// L'affichage sur le fil principal, rythmé par la boucle de winit.
struct Here {
    display: Display<platform::FrameClock>,
    /// Dernier battement de l'horloge pas encore traité (les battements se
    /// remplacent : si le fil a pris du retard, une seule image rattrape le
    /// tout), et battements remplacés depuis la dernière image.
    beat: Rc<Cell<Option<platform::Beat>>>,
    coalesced: Rc<Cell<u32>>,
}

/// Où tourne l'affichage.
enum Displayer {
    /// Pas encore en route (ou arrêté).
    Off,
    /// Sur son propre fil (calques Core Animation).
    Thread(DisplayThread),
    /// Ici (GPU).
    Here(Box<Here>),
}

impl Displayer {
    fn send(&mut self, command: Command) {
        match self {
            Self::Off => {}
            Self::Thread(thread) => thread.send(command),
            Self::Here(here) => here.display.command(command),
        }
    }

    /// Une demande qui peut attendre la prochaine image.
    fn send_quiet(&mut self, command: Command) {
        match self {
            Self::Off => {}
            Self::Thread(thread) => thread.send_quiet(command),
            Self::Here(here) => here.display.command(command),
        }
    }

    /// Demandes envoyées jusqu'ici (voir [`crate::display`]).
    fn sent(&self) -> u64 {
        match self {
            Self::Off => 0,
            Self::Thread(thread) => thread.sent(),
            Self::Here(here) => here.display.done(),
        }
    }

    fn stop(&mut self) {
        match self {
            Self::Off => {}
            Self::Thread(thread) => thread.stop(),
            Self::Here(here) => here.display.shutdown(),
        }
        *self = Self::Off;
    }
}

#[cfg_attr(not(any(target_os = "macos", target_os = "windows")), allow(dead_code))]
pub struct App {
    settings: Settings,
    paths: Option<Paths>,
    library: Library,
    started: bool,
    display: Displayer,
    /// Une fenêtre transparente par écran, où la scène se pose.
    windows: ScreenWindows,
    /// Les écrans tels que le système les donne, au dernier relevé.
    screens: Vec<Screen>,
    next_screen_poll: Instant,
    /// Relevé des fenêtres, du Dock et du dossier des animaux (à côté).
    watcher: Option<Arc<Watcher>>,
    flock: FlockView,
    /// Changements de définition pas encore vus par une description du
    /// troupeau : le rang de leur demande, et le changement.
    handed: Vec<(u64, Handed)>,
    /// Animaux demandés pas encore vus par une description du troupeau :
    /// le rang de leur demande, et leur définition.
    unseen: Vec<(u64, Arc<PetDef>)>,
    /// Les réglages qui changent ce que fait l'affichage (un essai peut en
    /// imposer), et ce qu'il en sait.
    live: Live,
    live_sent: Option<Live>,
    dark: bool,
    /// L'autorisation d'Accessibilité, telle que le relevé la voit.
    trusted: bool,
    /// Échap deux fois, ou tenue (voir `crate::keys`).
    escape: crate::keys::EscapeGesture,
    /// Dernière position du curseur vue par les événements des fenêtres.
    cursor: Option<Vec2>,
    /// Animaux demandés dont le dessin se prépare encore (identifiant → nombre).
    pending: HashMap<String, Vec<(Wanted, bool)>>,
    /// Les animaux demandés en ce moment arrivent sur l'écran où se trouve
    /// le curseur (un seul animal, demandé par un menu ou par un second
    /// lancement : là où l'on regarde) ; sinon, sur n'importe quel écran.
    here: bool,
    /// Les tirages de l'app (une brebis sur quatre arrive avec son agneau).
    rng: mouflon_core::math::Rng,
    /// Les brebis arrivent parfois suivies de leur agneau ; pas pendant un
    /// essai, où le nombre d'animaux est compté.
    lambs_follow: bool,
    /// Dessins en préparation sur un fil à part (identifiants).
    decoding: HashSet<String>,
    /// Animaux qui attendent leur dessin à une autre taille, et cette entrée.
    resizing: Vec<(u32, Entry)>,
    /// Définitions des jeunes, distinctes de leurs parents dans la bibliothèque.
    offspring: Vec<Entry>,
    /// Cuissons de jeunes en cours, partagées dès la première demande.
    preparing_offspring: Vec<(Entry, usize)>,
    /// Une taille globale demandée avant le premier rapport d’un jeune.
    offspring_sizes: Vec<OffspringSize>,
    /// Sources encore affichées après un rechargement de la bibliothèque.
    retired_entries: Vec<Entry>,
    /// Essai (`MOUFLON_ESSAI`) : étapes à venir.
    trial_steps: Vec<(Instant, TrialStep)>,
    /// Le journal d'un essai, vu d'ici (les images y sont notées par
    /// l'affichage).
    notes: Option<Notes>,
    /// Temps passé à faire des images pendant le gestionnaire en cours
    /// (affichage sur ce fil) : le reste est noté s'il est long (essais).
    frame_time: Duration,
    /// Pour réveiller la boucle depuis l'horloge, le menu ou un autre fil.
    proxy: Arc<Mutex<EventLoopProxy<UserEvent>>>,
    #[cfg(any(target_os = "macos", target_os = "windows"))]
    menu: Option<crate::menu::TrayMenu>,
    /// Ce que le menu montre du troupeau (le préféré, le nombre d'animaux de
    /// chaque espèce) : mis à jour seulement quand cela change.
    #[cfg(any(target_os = "macos", target_os = "windows"))]
    menu_flock: Option<(usize, Vec<usize>)>,
    #[cfg(any(target_os = "macos", target_os = "windows"))]
    menu_generation: Option<u64>,
    /// Ouverture à l'ouverture de session, telle que le système l'a dite à
    /// la dernière ouverture des réglages (c'est lui qui la garde).
    login: platform::LoginItem,
    /// L'animal dont le menu (clic droit) a été ouvert en dernier, le rang
    /// de son espèce dans la bibliothèque, et sa définition.
    menu_pet: Option<(u32, Option<usize>, Arc<PetDef>)>,
    /// Vitrine de la CI (`MOUFLON_VITRINE`) : images de la fenêtre des réglages.
    showcase: Option<Showcase>,
    /// La recherche des nouvelles versions (voir `crate::update`).
    updater: Option<crate::update::Updater>,
    /// Ce Mouflon a été lancé par un relais (la version d'avant lui laisse
    /// la place, ou il revient).
    arrival: Option<crate::update::Arrival>,
    /// L'heure, au plus tard, de son signe de vie à la version d'avant (elle
    /// attend de le voir démarrer) : donné dès qu'un animal est là.
    confirm_at: Option<Instant>,
    /// Une version plus récente existe, que ce Mouflon n'installe pas
    /// lui-même.
    update_available: Option<crate::update::Version>,
    /// La version prête à prendre la place de celle-ci, quand elle quitte.
    relay: Option<Box<crate::update::Ready>>,
    /// La version prête, pendant que l'affichage l'annonce (voir
    /// [`Command::Farewell`]) ; `counting` : le compte tourne, on peut
    /// encore la remettre à plus tard.
    ready: Option<Box<crate::update::Ready>>,
    counting: bool,
    scene: Option<mouflon_core::show::Scene>,
    /// Le troupeau au moment où il part, pour la version qui arrive.
    herd: Option<crate::update::Herd>,
}

/// Ce qu'une version qui arrive laisse passer, une fois démarrée, avant de
/// donner signe de vie si aucun animal n'est encore là (leurs dessins
/// peuvent être longs à préparer).
const ARRIVAL_SETTLE: Duration = Duration::from_secs(10);

/// Ouvre la fenêtre « À propos » : version, description, page de présentation
/// (le `homepage` du workspace : on y trouve la dernière version et son code
/// source ; à défaut, son `repository`), crédits, licence et copyright.
fn show_about() {
    use platform::AboutLine;
    let t = crate::i18n::texts();
    let shown = |url: &'static str| url.trim_start_matches("https://").trim_end_matches('/');
    let homepage = env!("CARGO_PKG_HOMEPAGE");
    let page = if homepage.is_empty() {
        env!("CARGO_PKG_REPOSITORY")
    } else {
        homepage
    };
    let lines = [
        AboutLine::Text(&t.description),
        AboutLine::Link(shown(page), page),
        AboutLine::Text(&t.credits),
        AboutLine::Text(&t.license),
    ];
    platform::show_about(&platform::About {
        name: "Mouflon",
        version: env!("CARGO_PKG_VERSION"),
        copyright: COPYRIGHT,
        lines: &lines,
    });
}

fn to_rect(r: platform::OsRect) -> Rect {
    Rect::new(r.x as f32, r.y as f32, r.w as f32, r.h as f32)
}

/// Les écrans, tels que le système les donne ; sur les systèmes sans couche
/// dédiée (Wayland sans XWayland…), ceux de winit. Et s'ils sont « exacts » :
/// en pixels, à une seule échelle pour tout le bureau (Windows, X11 ; voir
/// [`ScreenWindows::sync`]).
fn system_screens(event_loop: &ActiveEventLoop) -> (Vec<Screen>, bool) {
    let screens: Vec<Screen> = platform::screens()
        .into_iter()
        .map(|s| Screen {
            frame: to_rect(s.frame),
            usable: to_rect(s.visible),
            scale: s.scale as f32,
        })
        .collect();
    if !screens.is_empty() {
        // Essais : des écrans simulés, découpés dans ceux du système (voir
        // `crate::screens::asked`).
        let scale = screens[0].scale;
        let screens = crate::screens::asked(scale).unwrap_or(screens);
        return (screens, !cfg!(target_os = "macos"));
    }
    let screens = event_loop
        .available_monitors()
        .map(|m| {
            let scale = m.scale_factor();
            let p = m.position().to_logical::<f64>(scale);
            let s = m.size().to_logical::<f64>(scale);
            let frame = Rect::new(p.x as f32, p.y as f32, s.width as f32, s.height as f32);
            let usable = Rect::new(frame.x, frame.y, frame.w, (frame.h - 48.0).max(100.0));
            Screen {
                frame,
                usable,
                scale: scale as f32,
            }
        })
        .collect();
    (screens, false)
}

#[cfg_attr(not(any(target_os = "macos", target_os = "windows")), allow(dead_code))]
impl App {
    #[must_use]
    pub fn new(
        settings: Settings,
        paths: Option<Paths>,
        library: Library,
        proxy: EventLoopProxy<UserEvent>,
        arrival: Option<crate::update::Arrival>,
    ) -> Self {
        let now = Instant::now();
        let live = Live {
            paused: false,
            sounds: settings.sounds,
            collisions: settings.collisions,
            realistic: settings.realistic_physics,
            surprises: settings.surprises,
            frontmost_only: settings.frontmost_only,
            walk_under_dock: settings.walk_under_dock,
            mood: settings.mood,
        };
        Self {
            settings,
            paths,
            library,
            started: false,
            display: Displayer::Off,
            windows: ScreenWindows::new(),
            screens: Vec::new(),
            next_screen_poll: now,
            watcher: None,
            flock: FlockView::default(),
            handed: Vec::new(),
            unseen: Vec::new(),
            live,
            live_sent: None,
            dark: false,
            trusted: false,
            escape: crate::keys::EscapeGesture::default(),
            cursor: None,
            pending: HashMap::new(),
            here: false,
            rng: mouflon_core::math::Rng::new(
                std::time::SystemTime::now()
                    .duration_since(std::time::UNIX_EPOCH)
                    .map_or(0x4D4F_5546, |d| d.as_nanos() as u64),
            ),
            lambs_follow: crate::stats::trial().is_none(),
            decoding: HashSet::new(),
            resizing: Vec::new(),
            offspring: Vec::new(),
            preparing_offspring: Vec::new(),
            offspring_sizes: Vec::new(),
            retired_entries: Vec::new(),
            trial_steps: Vec::new(),
            notes: None,
            frame_time: Duration::ZERO,
            proxy: Arc::new(Mutex::new(proxy)),
            #[cfg(any(target_os = "macos", target_os = "windows"))]
            menu: None,
            #[cfg(any(target_os = "macos", target_os = "windows"))]
            menu_flock: None,
            #[cfg(any(target_os = "macos", target_os = "windows"))]
            menu_generation: None,
            login: platform::LoginItem::Unavailable,
            menu_pet: None,
            showcase: None,
            updater: None,
            arrival,
            confirm_at: None,
            update_available: None,
            relay: None,
            ready: None,
            counting: false,
            scene: None,
            herd: None,
        }
    }

    fn send(&mut self, command: Command) {
        self.display.send(command);
    }

    /// Note au journal d'un essai (s'il y en a un).
    fn note(&self, what: impl std::fmt::Display) {
        if let Some(notes) = &self.notes {
            notes.note(what);
        }
    }

    /// Un changement de définition vient d'être demandé : la description
    /// du troupeau le montre dès maintenant.
    fn hand(&mut self, handed: Handed) {
        self.flock.apply(&handed);
        self.handed.push((self.display.sent(), handed));
    }

    // ——— Les écrans ———

    /// Relit les écrans : leurs fenêtres sont refaites s'ils ont changé, et
    /// l'affichage l'apprend.
    fn refresh_screens(&mut self, event_loop: &ActiveEventLoop) {
        let (screens, exact) = self.chosen_screens(event_loop);
        let windows = self.windows.sync(event_loop, &screens, exact);
        let hz = self.windows.refresh_hz();
        if windows || hz || screens != self.screens {
            self.screens = screens;
            self.send_screens();
        }
        if windows
            && let Some(dark) = self.windows.dark()
            && dark != self.dark
        {
            self.dark = dark;
            self.send(Command::Dark(dark));
        }
    }

    /// Les écrans où vivent les animaux : ceux du système, quatre au plus,
    /// ou le principal seul si les réglages le demandent (voir
    /// `crate::screens::chosen`). Notés au journal quand ils changent.
    fn chosen_screens(&self, event_loop: &ActiveEventLoop) -> (Vec<Screen>, bool) {
        let (screens, exact) = system_screens(event_loop);
        let chosen = crate::screens::chosen(&screens, !self.settings.all_screens);
        if chosen != self.screens {
            let left = screens.len() - chosen.len().min(screens.len());
            log::info!(
                "écrans : {}{}",
                crate::screens::describe(&chosen),
                if left > 0 {
                    format!(" ; {left} autre(s) laissé(s) de côté")
                } else {
                    String::new()
                }
            );
        }
        (chosen, exact)
    }

    /// Le compositeur du système a perdu son appareil graphique : la scène
    /// est reposée dans chaque fenêtre d'écran, et l'affichage refait tout.
    /// (Si ce n'est pas encore possible, le relevé des écrans y revient.)
    fn restore_scene(&mut self) {
        if self.windows.reattach() {
            log::info!("affichage : scène reposée dans les fenêtres des écrans");
            self.note("scène reposée dans les fenêtres des écrans");
            self.send_screens();
        } else {
            log::warn!("affichage : la scène n'a pas pu être reposée, nouvel essai bientôt");
        }
    }

    fn send_screens(&mut self) {
        let command = Command::Screens {
            screens: self.screens.clone(),
            overlays: self.windows.overlays(),
        };
        self.send(command);
    }

    /// (Re)crée l'horloge de l'écran, sur l'écran le plus rapide (seul le
    /// fil principal peut le demander), et la donne à l'affichage.
    fn start_clock(&mut self) {
        let proxy = Arc::clone(&self.proxy);
        match &mut self.display {
            Displayer::Off => {}
            Displayer::Thread(thread) => {
                let clock = thread.new_clock();
                thread.set_clock(clock);
            }
            Displayer::Here(here) => {
                // L'ancienne d'abord : elle cesse de battre.
                here.display.set_clock(None);
                here.beat.set(None);
                let beat = Rc::clone(&here.beat);
                let coalesced = Rc::clone(&here.coalesced);
                let clock = platform::FrameClock::new(move |tick| {
                    // Un seul battement en attente à la fois.
                    if beat.replace(Some(tick)).is_some() {
                        coalesced.set(coalesced.get() + 1);
                    } else if let Ok(proxy) = proxy.lock() {
                        let _ = proxy.send_event(UserEvent::Frame);
                    }
                });
                here.display.set_clock(clock);
            }
        }
    }

    /// Relève les écrans et le dossier des animaux.
    fn poll(&mut self, event_loop: &ActiveEventLoop, now: Instant) {
        self.next_screen_poll = now + SCREEN_POLL;
        self.refresh_screens(event_loop);
        self.windows.check();
        // Une scène perdue qui n'a pas pu être reposée tout de suite.
        if platform::scene_lost() {
            self.restore_scene();
        }
        // Un animal ajouté au dossier apparaît dans le menu sans relancer l'app.
        if let Some(stamp) = self.watcher.as_ref().and_then(|w| w.take_pets())
            && self.library.changed_since(&stamp)
        {
            log::info!("dossier des animaux modifié : rechargement");
            self.preserve_live_entries();
            self.library.reload();
            self.rebuild_menu();
        }
    }

    // ——— Les animaux et leurs dessins ———

    /// Un animal de cette espèce arrive, tiré comme le veut son fichier :
    /// pour une race, surtout des brebis, parfois un bélier.
    fn spawn(&mut self, index: usize) {
        self.spawn_wanted(index, Wanted::Any);
    }

    /// Un animal de cette espèce arrive : tiré comme le veut son fichier, ou
    /// le bélier, la brebis ou l'agneau qu'on demande (la galerie). Une
    /// brebis sur [`crate::menu::LAMB_EVERY`] arrive suivie de son agneau.
    fn spawn_wanted(&mut self, index: usize, wanted: Wanted) {
        let Some(entry) = self.library.pets.get(index).cloned() else {
            return;
        };
        if wanted == Wanted::Lamb && entry.def.has_lambs() {
            let from = Arc::clone(&entry.def);
            self.prepare_lamb(&entry, from, LambGoal::Alone(None));
            return;
        }
        // Les dessins d'un animal se préparent à sa première apparition, sur
        // un fil à part : les autres continuent de bouger pendant ce temps, et
        // il arrive dès que c'est prêt.
        if !entry.art_ready() && self.prepare_art(&entry) {
            self.pending
                .entry(entry.def.meta.id.clone())
                .or_default()
                .push((wanted, self.here));
            return;
        }
        // Illisible : rien à montrer (l'erreur est au journal).
        let Some(art) = entry.art() else {
            return;
        };
        self.compress_later(&entry);
        let with_lamb = self.lambs_follow
            && wanted != Wanted::Ram
            && entry.def.has_lambs()
            && self.rng.next_u32().is_multiple_of(crate::menu::LAMB_EVERY);
        self.send(Command::Spawn {
            def: Arc::clone(&entry.def),
            art,
            sex: wanted.sex(),
            lambs: if with_lamb { vec![None] } else { Vec::new() },
            here: self.here,
            place: None,
        });
        self.unseen
            .push((self.display.sent(), Arc::clone(&entry.def)));
    }

    /// L'entrée de la bibliothèque d'un animal du troupeau d'avant la mise à
    /// jour, à sa taille. Aucune si son espèce n'est plus là.
    fn entry_back(&mut self, animal: &crate::update::Animal) -> Option<Entry> {
        let index = self
            .library
            .pets
            .iter()
            .position(|e| e.def.meta.id == animal.species)?;
        match animal.size {
            Some(size) if (size - self.library.size()).abs() > 1e-3 => {
                self.library.at_size(index, size)
            }
            _ => self.library.pets.get(index).cloned(),
        }
    }

    /// Un animal du troupeau d'avant la mise à jour revient à sa place (son
    /// dessin est prêt : voir le lancement), avec les agneaux qui le
    /// suivaient (`lambs` : leurs places), chacun à la sienne. Un agneau
    /// sans sa mère revient seul, à sa place.
    fn spawn_back(&mut self, animal: &crate::update::Animal, entry: &Entry, lambs: Vec<f32>) {
        use crate::update::Worn;
        if animal.worn == Some(Worn::Lamb) && entry.def.has_lambs() {
            let from = Arc::clone(&entry.def);
            self.prepare_lamb(entry, from, LambGoal::Alone(Some(animal.x)));
            return;
        }
        let Some(art) = entry.art() else {
            return;
        };
        self.compress_later(entry);
        // Une mère est une brebis, même dessinée sans variante.
        let sex = match animal.worn {
            _ if !lambs.is_empty() => Some(mouflon_core::Sex::Female),
            Some(Worn::Ram) => Some(mouflon_core::Sex::Male),
            Some(Worn::Ewe) => Some(mouflon_core::Sex::Female),
            Some(Worn::Lamb) | None => None,
        };
        self.send(Command::Spawn {
            def: Arc::clone(&entry.def),
            art,
            sex,
            lambs: lambs.into_iter().map(Some).collect(),
            here: false,
            place: Some(animal.x),
        });
        self.unseen
            .push((self.display.sent(), Arc::clone(&entry.def)));
    }

    /// Un rechargement remplace les entrées de la bibliothèque, mais pas
    /// les animaux affichés ni les dessins qui finissent de changer leur taille.
    fn preserve_live_entries(&mut self) {
        let sources: Vec<Entry> = self
            .flock
            .pets
            .iter()
            .map(|p| &p.def)
            .chain(self.unseen.iter().map(|(_, d)| d))
            .filter_map(|def| self.library.entry_of(def).cloned())
            .chain(self.resizing.iter().map(|(_, entry)| entry.clone()))
            .collect();
        for entry in sources {
            if !self
                .retired_entries
                .iter()
                .any(|e| Arc::ptr_eq(&e.def, &entry.def))
            {
                self.retired_entries.push(entry);
            }
        }
    }

    /// « Ajouter son agneau » : il naît auprès de cette brebis.
    fn add_lamb(&mut self, mother: u32) {
        self.lamb_of(mother, LambGoal::Born(mother));
    }

    /// Prépare l'agneau de cette brebis (voir [`Self::prepare_lamb`]).
    fn lamb_of(&mut self, mother: u32, goal: LambGoal) {
        let Some(pet) = self.flock.pets.iter().find(|p| p.id == mother && !p.extra) else {
            return;
        };
        // Si elle ne peut pas être sa mère, celui qui revient à sa place
        // (d'une mise à jour) y revient seul ; les autres ne viennent pas.
        let goal = match goal {
            _ if pet.is_ewe => goal,
            LambGoal::Arriving(_, Some(x)) => LambGoal::Alone(Some(x)),
            _ => return,
        };
        let from = Arc::clone(&pet.def);
        let Some(parent) = self
            .library
            .entry_of(&from)
            .or_else(|| {
                self.retired_entries
                    .iter()
                    .find(|e| Arc::ptr_eq(&e.def, &from))
            })
            .cloned()
        else {
            return;
        };
        self.prepare_lamb(&parent, from, goal);
    }

    /// Le dessin d’un jeune se prépare sur un fil à part, même quand sa
    /// mère est déjà à l’écran : il a une autre taille de cuisson.
    fn prepare_lamb(&mut self, parent: &Entry, from: Arc<PetDef>, goal: LambGoal) {
        // L'agneau a l'âge de son dessin (deux semaines, sauf `weeks`).
        let weeks = parent.lamb_weeks();
        let entry = self
            .offspring
            .iter()
            .find(|e| e.is_lamb_of(parent, weeks))
            .cloned()
            .or_else(|| {
                self.preparing_offspring
                    .iter()
                    .find(|(e, _)| e.is_lamb_of(parent, weeks))
                    .map(|(e, _)| e.clone())
            })
            .or_else(|| parent.as_lamb(weeks));
        let Some(entry) = entry else {
            return;
        };
        if let Some((_, count)) = self
            .preparing_offspring
            .iter_mut()
            .find(|(e, _)| Arc::ptr_eq(&e.def, &entry.def))
        {
            *count += 1;
        } else {
            self.preparing_offspring.push((entry.clone(), 1));
        }
        let requested = Arc::clone(&entry.def);
        let proxy = Arc::clone(&self.proxy);
        let spawned = std::thread::Builder::new()
            .name("mouflon-agneau".into())
            .spawn(move || {
                let _ = entry.art();
                if let Ok(proxy) = proxy.lock() {
                    let _ = proxy.send_event(UserEvent::LambReady { goal, from, entry });
                }
            });
        if let Err(e) = spawned {
            self.finish_lamb_loading(&requested);
            log::warn!("impossible de préparer l’agneau : {e}");
        }
    }

    fn finish_lamb_loading(&mut self, def: &Arc<PetDef>) {
        if let Some(at) = self
            .preparing_offspring
            .iter()
            .position(|(e, _)| Arc::ptr_eq(&e.def, def))
        {
            self.preparing_offspring[at].1 -= 1;
            if self.preparing_offspring[at].1 == 0 {
                self.preparing_offspring.remove(at);
            }
        }
    }

    fn lamb_ready(&mut self, goal: LambGoal, from: Arc<PetDef>, entry: Entry) {
        self.finish_lamb_loading(&entry.def);
        let mother = match goal {
            LambGoal::Born(mother) | LambGoal::Arriving(mother, _) => {
                let same = self
                    .flock
                    .pets
                    .iter()
                    .find(|p| p.id == mother && p.is_ewe && !p.extra)
                    .map(|pet| Arc::ptr_eq(&from, &pet.def));
                match same {
                    Some(true) => Some(mother),
                    Some(false) => {
                        // Un changement de taille est arrivé pendant le
                        // chargement.
                        self.lamb_of(mother, goal);
                        return;
                    }
                    // Partie entre-temps : celui qui revient à sa place (d'une
                    // mise à jour) y revient seul.
                    None if matches!(goal, LambGoal::Arriving(_, Some(_))) => None,
                    None => return,
                }
            }
            LambGoal::Alone(_) => None,
        };
        let Some(art) = entry.art() else {
            return;
        };
        let def = Arc::clone(&entry.def);
        let place = match goal {
            LambGoal::Arriving(_, place) | LambGoal::Alone(place) => place,
            LambGoal::Born(_) => None,
        };
        if !self
            .offspring
            .iter()
            .any(|e| Arc::ptr_eq(&e.def, &entry.def))
        {
            self.offspring.push(entry);
        }
        match mother {
            Some(mother) => self.send(Command::SpawnLamb {
                mother,
                def: Arc::clone(&def),
                art,
                arriving: matches!(goal, LambGoal::Arriving(..)),
                place,
            }),
            // Un agneau seul arrive comme n'importe quel animal.
            None => self.send(Command::Spawn {
                def: Arc::clone(&def),
                art,
                sex: None,
                lambs: Vec::new(),
                here: self.here && place.is_none(),
                place,
            }),
        }
        self.unseen.push((self.display.sent(), def));
    }

    /// Lance le décodage des dessins de ces animaux, une espèce par fil, sans
    /// les attendre : `Entry::art` attend ensuite ceux qui ne sont pas prêts
    /// (ou décode sur place si un fil n'a pas pu être lancé).
    fn decode_ahead(&self, starters: &[usize]) -> Vec<Entry> {
        let mut species: Vec<Entry> = Vec::new();
        for entry in starters.iter().filter_map(|&i| self.library.pets.get(i)) {
            if !species.iter().any(|e| e.def.meta.id == entry.def.meta.id) {
                species.push(entry.clone());
            }
        }
        for entry in species.iter().filter(|e| !e.art_ready()) {
            let entry = entry.clone();
            let spawned = std::thread::Builder::new()
                .name("mouflon-dessins".into())
                .spawn(move || {
                    let _ = entry.art();
                });
            if let Err(e) = spawned {
                log::warn!("dessins préparés sur le fil principal : {e}");
            }
        }
        species
    }

    /// Prépare les dessins d'un animal sur un fil à part ; `UserEvent::ArtReady`
    /// arrive quand c'est fait. `false` si le fil n'a pas pu être lancé.
    fn prepare_art(&mut self, entry: &Entry) -> bool {
        let id = entry.def.meta.id.clone();
        if self.decoding.contains(&id) {
            return true;
        }
        let job = (entry.clone(), id.clone(), Arc::clone(&self.proxy));
        let spawned = std::thread::Builder::new()
            .name("mouflon-dessins".into())
            .spawn(move || {
                let (entry, id, proxy) = job;
                let _ = entry.art();
                if let Ok(proxy) = proxy.lock() {
                    let _ = proxy.send_event(UserEvent::ArtReady(id));
                }
            });
        match spawned {
            Ok(_) => {
                self.decoding.insert(id);
                true
            }
            Err(e) => {
                log::warn!("dessins préparés sur le fil principal : {e}");
                false
            }
        }
    }

    /// Compresse les images d'une espèce sur des fils de fond, un peu après
    /// son arrivée (voir [`crate::library::Entry::compress`]) ;
    /// `UserEvent::ArtCompressed` arrive quand c'est fait. Rien à faire pour
    /// un dessin vectoriel, du pixel art agrandi, ou des images reprises
    /// déjà compressées du disque.
    fn compress_later(&self, entry: &Entry) {
        if !entry.compressible() {
            return;
        }
        let job = (entry.clone(), Arc::clone(&self.proxy));
        let spawned = std::thread::Builder::new()
            .name("mouflon-compression".into())
            .spawn(move || {
                let (entry, proxy) = job;
                platform::lower_thread_priority();
                std::thread::sleep(COMPRESS_DELAY);
                let threads =
                    std::thread::available_parallelism().map_or(1, |n| (n.get() / 2).clamp(1, 4));
                let Some(done) = entry.compress(threads, &platform::lower_thread_priority) else {
                    return;
                };
                log::info!(
                    "images de {} compressées en {} ms sur {threads} fils : {:.1} → {:.1} Mo",
                    entry.def.meta.name,
                    done.took.as_millis(),
                    done.before as f64 / 1e6,
                    done.after as f64 / 1e6,
                );
                if let Ok(proxy) = proxy.lock() {
                    let _ = proxy.send_event(UserEvent::ArtCompressed(entry.def.meta.id.clone()));
                }
            });
        if let Err(e) = spawned {
            log::warn!("images non compressées : {e}");
        }
    }

    /// Les images d'une espèce sont compressées : ses animaux prennent le
    /// dessin compressé ; les images d'avant s'oublient.
    fn art_compressed(&mut self, id: &str) {
        // Toutes les tailles de l'espèce dont le dessin est prêt (jamais de
        // décodage ici).
        let ready: Vec<(Arc<PetDef>, Arc<PetArt>)> = self
            .library
            .sizes_of(id)
            .filter(|entry| entry.art_ready())
            .filter_map(|entry| Some((Arc::clone(&entry.def), entry.art()?)))
            .collect();
        if ready.is_empty() {
            return;
        }
        let pets = self
            .flock
            .pets
            .iter()
            .filter(|p| ready.iter().any(|(def, _)| Arc::ptr_eq(def, &p.def)))
            .count();
        let bytes: usize = ready.iter().map(|(_, art)| art.image_bytes()).sum();
        let changes = ready
            .into_iter()
            .map(|(def, art)| Redraw {
                from: Arc::clone(&def),
                to: def,
                art,
                factor: 1.0,
            })
            .collect();
        self.send(Command::Redraw {
            changes,
            gone: Vec::new(),
        });
        self.note(format_args!(
            "images de {id} compressées ({:.1} Mo, {pets} animal(aux))",
            bytes as f64 / 1e6
        ));
        // Les images d'origine sont libérées : leurs pages retournent au
        // système tout de suite, plutôt que dans quelques minutes.
        let _ = std::thread::Builder::new()
            .name("mouflon-menage".into())
            .spawn(|| {
                platform::lower_thread_priority();
                let released = platform::release_free_memory();
                if released >= 1 << 20 {
                    log::info!(
                        "{:.1} Mo de mémoire libérée rendus au système",
                        released as f64 / 1e6
                    );
                }
            });
    }

    /// Les dessins d'un animal sont prêts : ceux qui l'attendaient arrivent.
    fn art_ready(&mut self, id: &str) {
        self.decoding.remove(id);
        let waiting = self.pending.remove(id).unwrap_or_default();
        if let Some(index) = self.library.pets.iter().position(|e| e.def.meta.id == id) {
            for (wanted, here) in waiting {
                let before = std::mem::replace(&mut self.here, here);
                self.spawn_wanted(index, wanted);
                self.here = before;
            }
        }
    }

    /// Un fichier ou un dossier d'animal déposé sur un animal : on l'installe,
    /// et l'animal visé laisse sa place au nouveau venu (comme dans desktopPet).
    fn install_pet(&mut self, path: &std::path::Path) {
        self.preserve_live_entries();
        match self.library.install(path) {
            Ok(id) => {
                log::info!("animal installé : {id} ({})", path.display());
                self.rebuild_menu();
                self.send(Command::RemoveUnderCursor);
                let index = self.library.index_of(&id);
                if self.settings.default_pet.as_deref() != Some(id.as_str()) {
                    self.settings.default_pet = Some(id);
                    self.save_settings();
                }
                self.spawn(index);
            }
            Err(e) => log::warn!("impossible d'installer {} : {e}", path.display()),
        }
    }

    /// Le dernier arrivé de cette espèce (rang dans la bibliothèque) dit au
    /// revoir.
    fn goodbye_one(&mut self, index: usize) {
        if let Some(entry) = self.library.pets.get(index) {
            let species = entry.def.meta.id.clone();
            self.send(Command::GoodbyeSpecies(species));
        }
    }

    /// Taille d'un animal d'après sa définition : la sienne s'il en a une à
    /// lui, celle de tous sinon.
    fn size_of(&self, def: &Arc<PetDef>) -> f32 {
        self.offspring
            .iter()
            .find(|e| Arc::ptr_eq(&e.def, def))
            .or_else(|| self.library.entry_of(def))
            .or_else(|| {
                self.retired_entries
                    .iter()
                    .find(|e| Arc::ptr_eq(&e.def, def))
            })
            .map_or(self.library.size(), |entry| entry.size)
    }

    /// Change la taille de tous les animaux (ceux qui en avaient une à eux
    /// compris), sans les interrompre.
    fn set_size(&mut self, index: usize) {
        let size = crate::settings::SIZES[index.min(crate::settings::SIZES.len() - 1)];
        // Chaque définition à l'écran (celles des animaux qui viennent
        // d'être demandés comprises), et sa taille.
        let mut defs: Vec<Arc<PetDef>> = Vec::new();
        let unseen = self.unseen.iter().map(|(_, def)| def);
        for def in self.flock.pets.iter().map(|p| &p.def).chain(unseen) {
            if !defs.iter().any(|d| Arc::ptr_eq(d, def)) {
                defs.push(Arc::clone(def));
            }
        }
        let all_sizes_match = defs.iter().all(|d| (self.size_of(d) - size).abs() < 1e-3);
        let young: Vec<u32> = self
            .flock
            .pets
            .iter()
            .filter(|p| self.offspring.iter().any(|e| Arc::ptr_eq(&e.def, &p.def)))
            .map(|p| p.id)
            .collect();
        let before: Vec<(Arc<PetDef>, f32)> = defs
            .into_iter()
            .filter(|d| !self.offspring.iter().any(|e| Arc::ptr_eq(&e.def, d)))
            .map(|def| {
                let size = self.size_of(&def);
                (def, size)
            })
            .collect();
        let unchanged =
            (size - self.settings.size_factor()).abs() < f32::EPSILON && all_sizes_match;
        // Un choix même égal à la taille courante annule les anciens travaux.
        self.resizing.clear();
        self.offspring_sizes.clear();
        if unchanged {
            return;
        }
        self.offspring_sizes = self
            .unseen
            .iter()
            .filter(|(_, d)| self.offspring.iter().any(|e| Arc::ptr_eq(&e.def, d)))
            .map(|(_, d)| OffspringSize {
                def: Arc::clone(d),
                size: index,
                applied: Vec::new(),
            })
            .collect();
        self.settings.size = size;
        self.save_settings();
        self.library.set_size(size);
        // Les dessins de chaque espèce à l'écran, à la nouvelle taille,
        // préparés en parallèle (l'affichage, lui, continue).
        let mut species = HashSet::new();
        let needed: Vec<Entry> = before
            .iter()
            .filter_map(|(def, _)| self.library.current(def))
            .filter(|entry| species.insert(entry.def.meta.id.clone()))
            .cloned()
            .collect();
        std::thread::scope(|scope| {
            for entry in &needed {
                scope.spawn(move || entry.art());
            }
        });
        let mut changes = Vec::new();
        let mut gone = Vec::new();
        for (def, old) in before {
            let found = self
                .library
                .current(&def)
                .and_then(|entry| Some((Arc::clone(&entry.def), entry.art()?)));
            match found {
                Some((to, art)) => changes.push(Redraw {
                    from: def,
                    to,
                    art,
                    factor: size / old,
                }),
                // Plus dans la bibliothèque : on n'a plus de quoi le dessiner.
                None => gone.push(def),
            }
        }
        let map = changes
            .iter()
            .map(|c| (Arc::clone(&c.from), Arc::clone(&c.to)))
            .collect();
        self.send(Command::Redraw { changes, gone });
        self.hand(Handed::Defs(map));
        for id in young {
            self.set_pet_size(id, index);
        }
        log::info!(
            "taille des animaux : × {size} ({} à l'écran)",
            self.flock.pets.len()
        );
    }

    /// Change la taille d'un seul animal (son menu). Son dessin à cette
    /// taille se prépare sur un fil à part s'il ne l'est pas déjà : les
    /// animaux continuent de bouger, et il grandit (ou rapetisse) dès que
    /// c'est prêt (`UserEvent::PetSizeReady`).
    fn set_pet_size(&mut self, id: u32, index: usize) {
        let index = index.min(crate::settings::SIZES.len() - 1);
        let size = crate::settings::SIZES[index];
        let Some(def) = self.flock.def_of(id).cloned() else {
            return;
        };
        // Rechoisir sa taille actuelle annule aussi un ancien changement
        // qui attendait encore son dessin.
        self.resizing.retain(|(pet, _)| *pet != id);
        if (self.size_of(&def) - size).abs() < 1e-3 {
            return;
        }
        let species = def.meta.id.clone();
        let young = self
            .offspring
            .iter()
            .find(|e| Arc::ptr_eq(&e.def, &def))
            .cloned();
        let entry = if let Some(young) = young {
            if let Some(entry) = self
                .offspring
                .iter()
                .find(|e| e.same_base_at_size(&young, size))
            {
                Some(entry.clone())
            } else {
                let entry = young.at_size(size);
                self.offspring.push(entry.clone());
                Some(entry)
            }
        } else {
            self.library
                .pets
                .iter()
                .position(|e| e.def.meta.id == species)
                .and_then(|i| self.library.at_size(i, size))
        };
        let Some(entry) = entry else {
            return;
        };
        self.resizing.retain(|(pet, _)| *pet != id);
        if entry.art_ready() {
            self.apply_pet_size(id, &entry);
            return;
        }
        self.resizing.push((id, entry.clone()));
        let job = (entry, Arc::clone(&self.proxy));
        let spawned = std::thread::Builder::new()
            .name("mouflon-dessins".into())
            .spawn(move || {
                let (entry, proxy) = job;
                let _ = entry.art();
                if let Ok(proxy) = proxy.lock() {
                    let _ = proxy.send_event(UserEvent::PetSizeReady(id, Arc::clone(&entry.def)));
                }
            });
        if let Err(e) = spawned {
            log::warn!("impossible de préparer la nouvelle taille : {e}");
            self.resizing.retain(|(pet, _)| *pet != id);
        }
    }

    /// Le dessin attendu par un animal pour sa nouvelle taille est prêt.
    fn pet_size_ready(&mut self, id: u32, def: &Arc<PetDef>) {
        if let Some(entry) = take_ready_resize(&mut self.resizing, id, def) {
            self.apply_pet_size(id, &entry);
        }
    }

    fn apply_pet_size(&mut self, id: u32, entry: &Entry) {
        let Some(art) = entry.art() else {
            return;
        };
        let Some(def) = self.flock.def_of(id).cloned() else {
            return;
        };
        let old = self.size_of(&def);
        self.send(Command::RedrawPet {
            id,
            def: Arc::clone(&entry.def),
            art,
            factor: entry.size / old,
        });
        self.hand(Handed::Pet(id, Arc::clone(&entry.def)));
        self.compress_later(entry);
        // Les tailles dont plus personne ne se sert sont oubliées, et leurs
        // images avec.
        let used: Vec<Arc<PetDef>> = self
            .flock
            .pets
            .iter()
            .map(|p| Arc::clone(&p.def))
            .chain(self.resizing.iter().map(|(_, e)| Arc::clone(&e.def)))
            .collect();
        self.library
            .forget_unused_sizes(|def| used.iter().any(|u| Arc::ptr_eq(u, def)));
        log::info!("n° {id} : taille × {}", entry.size);
    }

    /// L'affichage décrit le troupeau : ce qu'il ne savait pas encore y
    /// est ajouté, et le menu suit.
    fn flock_changed(&mut self, generation: u64, done: u64, pets: Vec<PetSummary>) {
        self.unseen.retain(|(at, _)| *at > done);
        // Un animal est là : cette version a bien démarré.
        if !pets.is_empty() {
            self.confirm_arrival();
        }
        // Les brebis tout juste arrivées que leur agneau suit.
        let followed: Vec<(u32, Option<f32>)> = pets
            .iter()
            .flat_map(|p| p.lambs.iter().map(move |place| (p.id, *place)))
            .collect();
        self.flock = FlockView::described(generation, done, pets, &mut self.handed);
        for (mother, place) in followed {
            self.lamb_of(mother, LambGoal::Arriving(mother, place));
        }
        let used = |entry: &Entry| {
            self.flock
                .pets
                .iter()
                .any(|p| Arc::ptr_eq(&p.def, &entry.def))
                || self.unseen.iter().any(|(_, d)| Arc::ptr_eq(d, &entry.def))
                || self
                    .resizing
                    .iter()
                    .any(|(_, e)| Arc::ptr_eq(&e.def, &entry.def))
        };
        self.offspring.retain(used);
        self.retired_entries.retain(used);
        let sizes = self
            .flock
            .take_offspring_sizes(&mut self.offspring_sizes, &self.unseen);
        for (id, size) in sizes {
            self.set_pet_size(id, size);
        }
        self.sync_menu();
    }

    // ——— Les menus ———

    /// Reconstruit le menu après un changement de la bibliothèque.
    fn rebuild_menu(&mut self) {
        #[cfg(any(target_os = "macos", target_os = "windows"))]
        {
            self.menu = None;
            match crate::menu::TrayMenu::new(&self.library, self.settings.sounds) {
                Ok(menu) => {
                    menu.set_paused(self.live.paused);
                    menu.show_shortcuts(self.settings.shortcuts);
                    self.menu = Some(menu);
                }
                Err(e) => log::error!("impossible de recréer le menu : {e}"),
            }
            self.menu_flock = None;
            self.menu_generation = None;
            self.sync_menu();
        }
    }

    /// Le menu suit le troupeau : « Ajouter Southdown » nomme l'animal préféré,
    /// et chaque espèce dit combien des siens sont à l'écran (ceux qui
    /// partent déjà et les personnages secondaires ne comptent pas).
    fn sync_menu(&mut self) {
        #[cfg(any(target_os = "macos", target_os = "windows"))]
        {
            let Some(menu) = &self.menu else {
                return;
            };
            // Le préféré ne change qu'avec un animal ajouté : la génération
            // du troupeau suffit à savoir s'il y a du nouveau.
            if self.menu_generation.is_some() && self.menu_generation == self.flock.generation {
                return;
            }
            self.menu_generation = self.flock.generation;
            let favorite = self
                .library
                .default_index(self.settings.default_pet.as_deref());
            let mut counts = vec![0; self.library.pets.len()];
            for pet in self.flock.pets.iter().filter(|p| !p.extra) {
                let id = pet.def.meta.id.as_str();
                if let Some(i) = self.library.pets.iter().position(|e| e.def.meta.id == id) {
                    counts[i] += 1;
                }
            }
            if self
                .menu_flock
                .as_ref()
                .is_some_and(|(f, c)| *f == favorite && *c == counts)
            {
                return;
            }
            menu.set_flock(favorite, &counts);
            self.menu_flock = Some((favorite, counts));
        }
    }

    /// Le menu d'un animal, au clic droit (ou Contrôle-clic) sur lui :
    /// l'affichage a trouvé l'animal sous le curseur, qui attend sous le menu.
    fn open_pet_menu(&mut self, id: u32, def: Arc<PetDef>, can_have_lamb: bool) {
        let species = self
            .library
            .pets
            .iter()
            .position(|e| e.def.meta.id == def.meta.id);
        let state = PetMenuState {
            is_ewe: self
                .flock
                .pets
                .iter()
                .any(|p| p.id == id && p.is_ewe && !p.extra),
            can_have_lamb,
            paused: self.live.paused,
            sounds: self.settings.sounds,
            size: crate::settings::size_index_of(self.size_of(&def)),
        };
        // La ligne de la mise à jour d'abord, s'il y en a une.
        let mut items = self
            .update_line()
            .map(|line| line.items(crate::i18n::texts()))
            .unwrap_or_default();
        items.extend(crate::menu::pet_menu(
            &def.meta.name,
            state,
            crate::i18n::texts(),
        ));
        // Sous Windows, l'icône de Mouflon est souvent rangée derrière la
        // flèche « ^ » : le menu d'un animal permet aussi de quitter.
        #[cfg(target_os = "windows")]
        items.extend(crate::menu::leave_menu(crate::i18n::texts()));
        // Sans icône dans une barre des menus (Linux), le menu d'un animal
        // propose aussi ce qui ne le regarde pas lui seul.
        #[cfg(not(any(target_os = "macos", target_os = "windows")))]
        {
            let names: Vec<&str> = self
                .library
                .pets
                .iter()
                .map(|e| e.def.meta.name.as_str())
                .collect();
            let kinds: Vec<(&str, Option<&str>)> = self
                .library
                .pets
                .iter()
                .map(|e| (e.def.meta.category(), e.def.meta.region()))
                .collect();
            items.extend(crate::menu::global_menu(
                &names,
                &kinds,
                crate::i18n::texts(),
            ));
        }
        self.menu_pet = Some((id, species, def));
        // Le menu s'ouvre au prochain tour de la boucle (voir
        // `platform::pop_up_menu`).
        platform::pop_up_menu(&items);
    }

    /// Ce que fait l'article choisi dans le menu d'un animal.
    fn pet_menu_action(&self, id: u32) -> Option<Action> {
        if let Some(choice) = crate::menu::GlobalChoice::from_id(id) {
            return Some(choice.action());
        }
        let (pet, species, _) = self.menu_pet.as_ref()?;
        PetChoice::from_id(id)?.action(*pet, *species)
    }

    /// Met la sortie audio en route avant le premier son, avec un son de
    /// l'animal du lancement (joué sans volume) : son décodeur se charge aussi.
    fn warm_up_sounds(&self) {
        let index = self
            .library
            .default_index(self.settings.default_pet.as_deref());
        let sample = self
            .library
            .pets
            .get(index)
            .and_then(|entry| entry.def.sounds.first())
            .and_then(|sound| sound.clips.first());
        platform::warm_up_sounds(sample);
    }

    fn handle_action(&mut self, event_loop: &ActiveEventLoop, action: Action) {
        self.note(format_args!("menu {action:?}"));
        // Un seul animal demandé : il arrive sur l'écran où l'on regarde.
        self.here = matches!(
            action,
            Action::AddDefault | Action::Add(_) | Action::AddWanted(..) | Action::AddOneRandom
        );
        self.do_action(event_loop, action);
        self.here = false;
    }

    /// Un animal d'une espèce tirée au hasard, `count` fois (pas le
    /// préféré : de quoi découvrir les autres).
    fn add_random(&mut self, count: usize) {
        let species = self.library.pets.len();
        if species == 0 {
            return;
        }
        let seed = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map_or(0, |d| d.as_nanos() as u64);
        let mut rng = mouflon_core::math::Rng::new(seed);
        for _ in 0..count {
            self.spawn(rng.next_u32() as usize % species);
        }
    }

    fn do_action(&mut self, event_loop: &ActiveEventLoop, action: Action) {
        match action {
            Action::AddDefault => {
                let index = self
                    .library
                    .default_index(self.settings.default_pet.as_deref());
                self.spawn(index);
            }
            Action::Add(index) => {
                self.spawn(index);
                self.make_favorite(index);
            }
            Action::AddWanted(index, wanted) => {
                self.spawn_wanted(index, wanted);
                self.make_favorite(index);
            }
            Action::AddLamb(mother) => self.add_lamb(mother),
            Action::AddRandom => self.add_random(crate::menu::RANDOM_COUNT),
            Action::AddOneRandom => {
                log::info!("Mouflon lancé une seconde fois : un animal au hasard arrive");
                self.add_random(1);
            }
            Action::AddEach(section) => {
                let kinds: Vec<(&str, Option<&str>)> = self
                    .library
                    .pets
                    .iter()
                    .map(|entry| (entry.def.meta.category(), entry.def.meta.region()))
                    .collect();
                let pets = crate::menu::sections(&kinds)
                    .into_iter()
                    .nth(section)
                    .map(|section| section.pets)
                    .unwrap_or_default();
                for index in pets {
                    self.spawn(index);
                }
            }
            Action::RemoveOne(index) => self.goodbye_one(index),
            Action::RemoveAll => self.send(Command::GoodbyeAll),
            Action::Goodbye(id) => self.send(Command::Goodbye(id)),
            Action::Together => self.send(Command::Together),
            Action::SetPaused(paused) => self.live.paused = paused,
            Action::SetSounds(on) => self.set_sounds(on),
            Action::SetSize(index) => self.set_size(index),
            Action::SetPetSize(pet, index) => self.set_pet_size(pet, index),
            Action::SetCollisions(on) => self.set_collisions(on),
            Action::SetRealisticPhysics(on) => self.set_realistic_physics(on),
            Action::SetSurprises(on) => self.set_surprises(on),
            Action::SetFrontmostOnly(on) => self.set_frontmost_only(on),
            Action::SetAllScreens(on) => self.set_all_screens(on),
            Action::SetAutoUpdate(on) => self.set_auto_update(on),
            Action::UpdateLine => self.on_update_line(),
            Action::SetWalkUnderDock(on) => self.set_walk_under_dock(on),
            Action::SetMood(index) => self.set_mood(index),
            Action::SetShortcuts(on) => self.set_shortcuts(on),
            Action::SetEscapeQuits(on) => self.set_escape_quits(on),
            Action::SetOpenAtLogin(on) => {
                // La case reste comme on l'a cochée en attendant la réponse.
                self.login = if on {
                    platform::LoginItem::On
                } else {
                    platform::LoginItem::Off
                };
                self.ask_login_item(Some(on));
            }
            Action::OpenSettings => self.open_settings(),
            Action::OpenGallery => self.open_gallery(false),
            Action::RefreshSettings => {
                // Relus à côté ; la fenêtre suit quand les réponses arrivent.
                self.ask_login_item(None);
                if let Some(watcher) = &self.watcher {
                    watcher.refresh_now();
                }
            }
            Action::OpenPetsFolder => {
                if let Some(paths) = &self.paths {
                    let _ = std::fs::create_dir_all(&paths.pets);
                    if !platform::open_folder(&paths.pets) {
                        log::warn!("impossible d'ouvrir {}", paths.pets.display());
                    }
                }
            }
            Action::OpenAccessibilitySettings => {
                if !platform::open_accessibility_settings() {
                    log::warn!("impossible d'ouvrir les réglages d'Accessibilité");
                }
            }
            Action::OpenLoginItemsSettings => platform::open_login_items_settings(),
            Action::About => show_about(),
            Action::Quit => event_loop.exit(),
        }
        // La barre des menus, la fenêtre des réglages et l'affichage
        // suivent l'état qui en résulte, d'où que vienne le changement.
        self.publish();
    }

    /// Montre l'état de l'app partout où il se voit : la barre des menus
    /// (pause, sons, animaux à l'écran), la fenêtre des réglages, et
    /// l'affichage pour ce qui le concerne. Le menu d'un animal, lui, est
    /// refait à chaque clic droit.
    fn publish(&mut self) {
        #[cfg(any(target_os = "macos", target_os = "windows"))]
        {
            if let Some(menu) = &self.menu {
                menu.set_paused(self.live.paused);
                menu.set_sounds(self.settings.sounds);
            }
            // Le préféré a pu changer sans qu'un animal arrive encore.
            self.menu_generation = None;
        }
        self.sync_menu();
        platform::update_form(&crate::settings_window::values(&self.shown()));
        if self.live_sent != Some(self.live) {
            self.live_sent = Some(self.live);
            self.send(Command::Live(self.live));
        }
    }

    /// Ce que la fenêtre des réglages montre de l'état de l'app et du système.
    fn shown(&self) -> Shown {
        Shown {
            size: self.settings.size_index(),
            mood: self.settings.mood_index(),
            sounds: self.settings.sounds,
            surprises: self.settings.surprises,
            // Celles de l'affichage : un essai peut les imposer.
            collisions: self.live.collisions,
            realistic_physics: self.live.realistic,
            frontmost_only: self.settings.frontmost_only,
            all_screens: self.settings.all_screens,
            walk_under_dock: self.settings.walk_under_dock,
            shortcuts: self.settings.shortcuts,
            escape_quits: self.settings.escape_quits,
            auto_update: self.settings.auto_update,
            trusted: self.trusted,
            login: self.login,
        }
    }

    fn save_settings(&self) {
        if let Some(paths) = &self.paths {
            self.settings.save(&paths.settings);
        }
    }

    /// Demande au système, sur un fil à part, où en est l'ouverture à
    /// l'ouverture de session (`set` : la demander ou la retirer d'abord).
    /// Il met parfois une cinquantaine de millisecondes à répondre ; la
    /// réponse arrive par `UserEvent::LoginItem`.
    fn ask_login_item(&self, set: Option<bool>) {
        if !platform::login_item_available() {
            return;
        }
        let proxy = Arc::clone(&self.proxy);
        let spawned = std::thread::Builder::new()
            .name("mouflon-ouverture".into())
            .spawn(move || {
                let state = match set {
                    Some(on) => platform::set_login_item(on),
                    None => platform::login_item(),
                };
                if let Ok(proxy) = proxy.lock() {
                    let _ = proxy.send_event(UserEvent::LoginItem(state));
                }
            });
        if let Err(e) = spawned {
            log::warn!("ouverture à l'ouverture de session non relue : {e}");
        }
    }

    /// Le dernier animal choisi devient celui du lancement et du premier
    /// article du menu.
    fn make_favorite(&mut self, index: usize) {
        if let Some(entry) = self.library.pets.get(index)
            && self.settings.default_pet.as_deref() != Some(entry.def.meta.id.as_str())
        {
            self.settings.default_pet = Some(entry.def.meta.id.clone());
            self.save_settings();
        }
    }

    fn set_mood(&mut self, index: usize) {
        let Some(&mood) = crate::settings::MOODS.get(index) else {
            return;
        };
        if self.settings.mood == mood {
            return;
        }
        self.settings.mood = mood;
        self.live.mood = mood;
        self.save_settings();
    }

    fn set_shortcuts(&mut self, on: bool) {
        if self.settings.shortcuts == on {
            return;
        }
        self.settings.shortcuts = on;
        self.save_settings();
        self.apply_shortcuts();
    }

    /// Les raccourcis de Mouflon sont réservés auprès du système (ou rendus),
    /// et le menu les montre (ou plus).
    fn apply_shortcuts(&self) {
        let wanted = if self.settings.shortcuts {
            crate::keys::Shortcut::for_system()
        } else {
            Vec::new()
        };
        let missed = platform::set_shortcuts(&wanted);
        if !wanted.is_empty() && !missed.is_empty() {
            let keys: Vec<char> = missed
                .iter()
                .filter_map(|id| crate::keys::Shortcut::from_id(*id))
                .map(|s| s.key().to_ascii_uppercase())
                .collect();
            log::info!("raccourcis déjà pris par une autre app (ou indisponibles ici) : {keys:?}");
        }
        #[cfg(any(target_os = "macos", target_os = "windows"))]
        if let Some(menu) = &self.menu {
            menu.show_shortcuts(self.settings.shortcuts);
        }
    }

    fn set_escape_quits(&mut self, on: bool) {
        if self.settings.escape_quits == on {
            return;
        }
        self.settings.escape_quits = on;
        self.save_settings();
        // Entendre Échap dans les autres apps demande l'autorisation
        // d'Accessibilité : macOS propose de l'accorder si Mouflon ne l'a pas.
        if on && !platform::request_accessibility() {
            log::info!("Échap : en attente de l'autorisation d'Accessibilité");
        }
        let heard = platform::listen_escape(on);
        if on && !heard {
            log::info!("Échap : seulement dans Mouflon tant que l'autorisation manque");
        }
    }

    /// Un raccourci de Mouflon, d'où que ce soit.
    fn shortcut_pressed(&mut self, id: u32) -> Option<Action> {
        if !self.settings.shortcuts {
            return None;
        }
        let shortcut = crate::keys::Shortcut::from_id(id)?;
        Some(shortcut.action(self.live.paused, self.settings.sounds))
    }

    /// La touche Échap bouge, dans n'importe quelle app.
    fn escape_moved(&mut self, change: platform::KeyMove) -> Option<Action> {
        if !self.settings.escape_quits {
            return None;
        }
        self.escape
            .key(change, Instant::now())
            .then_some(Action::Quit)
    }

    fn set_sounds(&mut self, on: bool) {
        if self.settings.sounds == on {
            return;
        }
        self.settings.sounds = on;
        self.live.sounds = on;
        self.save_settings();
        if on {
            self.warm_up_sounds();
        }
    }

    fn set_surprises(&mut self, on: bool) {
        if self.settings.surprises == on {
            return;
        }
        self.settings.surprises = on;
        self.live.surprises = on;
        self.save_settings();
    }

    // ——— La mise à jour ———

    fn set_auto_update(&mut self, on: bool) {
        if self.settings.auto_update == on {
            return;
        }
        self.settings.auto_update = on;
        self.save_settings();
        if let Some(updater) = &self.updater {
            updater.enable(on);
        }
        // (Décoché, Mouflon ne parle plus de version non plus ; et celle
        // qu'il annonçait ne s'installe pas.)
        if !on && self.ready.is_some() {
            self.send(Command::Postpone);
        }
        self.show_update_line();
    }

    /// La recherche des nouvelles versions commence, à côté.
    fn start_updater(&mut self) {
        if !crate::update::offered() {
            return;
        }
        let Some(dirs) = self.paths.as_ref().map(|p| p.updates.clone()) else {
            return;
        };
        let proxy = Arc::clone(&self.proxy);
        self.updater = crate::update::Updater::start(
            dirs,
            self.settings.auto_update,
            self.arrival,
            move |event| {
                if let Ok(proxy) = proxy.lock() {
                    let _ = proxy.send_event(UserEvent::Update(event));
                }
            },
        );
    }

    /// Ce que la recherche a trouvé.
    fn on_update(&mut self, event: crate::update::Event) {
        match event {
            crate::update::Event::Ready(ready) => {
                // Le réglage a pu changer pendant le téléchargement.
                if !self.settings.auto_update {
                    ready.discard();
                    return;
                }
                // Une annonce à la fois : l'autre version attendra demain.
                if self.ready.is_some() || self.relay.is_some() {
                    ready.discard();
                    return;
                }
                log::info!("mise à jour : Mouflon {} est prêt", ready.version);
                // L'une des deux mises en scène, au hasard ; la suite vient
                // de l'affichage (voir `on_show`).
                let scene = if self.rng.next_u32().is_multiple_of(2) {
                    mouflon_core::show::Scene::Herald
                } else {
                    mouflon_core::show::Scene::Saucer
                };
                self.ready = Some(ready);
                self.herd = None;
                self.scene = Some(scene);
                self.send(Command::Farewell(scene));
            }
            crate::update::Event::Available(version) => {
                if self.update_available != Some(version) {
                    log::info!("mise à jour : les menus disent que Mouflon {version} existe");
                    self.update_available = Some(version);
                    self.show_update_line();
                }
            }
        }
    }

    /// Ce que les menus disent de la mise à jour, s'il y a quelque chose à
    /// en dire.
    fn update_line(&self) -> Option<crate::menu::UpdateLine> {
        if self.counting && self.ready.is_some() {
            return Some(crate::menu::UpdateLine::Coming);
        }
        self.update_available
            .filter(|_| self.settings.auto_update)
            .map(crate::menu::UpdateLine::Available)
    }

    /// L'affichage dit où en est l'annonce de la mise à jour.
    fn on_show(&mut self, event_loop: &ActiveEventLoop, report: ShowReport) {
        match report {
            ShowReport::Counting => {
                self.counting = true;
                self.show_update_line();
            }
            ShowReport::Leaving(leavers) => {
                self.counting = false;
                self.show_update_line();
                self.herd = Some(self.herd_of(&leavers));
            }
            ShowReport::Gone => {
                self.counting = false;
                let Some(ready) = self.ready.take() else {
                    return;
                };
                // Le troupeau est noté pour la version qui arrive, puis
                // celle-ci s'arrête : le relais se fait après (voir `main`).
                if let Some(dirs) = self.paths.as_ref().map(|p| p.updates.clone()) {
                    self.herd.take().unwrap_or_default().save(&dirs);
                }
                self.relay = Some(ready);
                event_loop.exit();
            }
            ShowReport::Postponed => {
                self.counting = false;
                self.herd = None;
                self.postpone_ready();
                self.show_update_line();
            }
        }
    }

    /// La version prête est remise au prochain lancement : ce qui était
    /// préparé est défait, et l'état le retient (si Mouflon se met encore à
    /// jour lui-même).
    fn postpone_ready(&mut self) {
        let Some(ready) = self.ready.take() else {
            return;
        };
        let version = ready.version;
        ready.discard();
        if !self.settings.auto_update {
            return;
        }
        log::info!("mise à jour : Mouflon {version} est remis au prochain lancement");
        if let Some(dirs) = self.paths.as_ref().map(|p| p.updates.clone()) {
            crate::update::postpone(&dirs, version);
        }
    }

    /// Le troupeau qui part, tel que la version qui arrive le fera revenir
    /// (voir [`herd_animals`]).
    fn herd_of(&self, leavers: &[Leaver]) -> crate::update::Herd {
        use crate::update::{Herd, Staged};
        let animals = herd_animals(leavers, |id| {
            let pet = self.flock.pets.iter().find(|p| p.id == id)?;
            Some((pet.def.meta.id.clone(), self.size_of(&pet.def)))
        });
        Herd {
            scene: match self.scene {
                Some(mouflon_core::show::Scene::Saucer) => Staged::Saucer,
                _ => Staged::Herald,
            },
            at: 0,
            animals,
        }
    }

    /// Met le menu de la barre des menus d'accord avec [`Self::update_line`]
    /// (le menu d'un animal, lui, est refait à chaque ouverture).
    fn show_update_line(&self) {
        #[cfg(any(target_os = "macos", target_os = "windows"))]
        if let Some(menu) = &self.menu {
            menu.set_update(self.update_line());
        }
    }

    /// La ligne de la mise à jour a été choisie dans un menu.
    fn on_update_line(&mut self) {
        match self.update_line() {
            // La page de présentation : la dernière version s'y télécharge
            // d'un clic.
            Some(crate::menu::UpdateLine::Available(_)) => {
                platform::open_page(env!("CARGO_PKG_HOMEPAGE"));
            }
            // Plus tard : l'affichage arrête l'annonce, puis le dit.
            Some(crate::menu::UpdateLine::Coming) => self.send(Command::Postpone),
            None => {}
        }
    }

    /// La version prête à prendre la place de celle-ci, s'il y en a une
    /// (à demander une fois la boucle quittée).
    pub fn take_relay(&mut self) -> Option<Box<crate::update::Ready>> {
        self.relay.take()
    }

    /// Le signe de vie d'une version lancée par celle d'avant (une seule
    /// fois ; écrit à côté : rien n'attend le disque ici).
    fn confirm_arrival(&mut self) {
        let Some(arrival) = self.arrival.take() else {
            return;
        };
        self.confirm_at = None;
        if let Some(dirs) = self.paths.as_ref().map(|p| p.updates.clone()) {
            let _ = std::thread::Builder::new()
                .name("mouflon-relais".into())
                .spawn(move || {
                    arrival.confirm(&dirs);
                    // Cette version a bien démarré : le troupeau noté pour
                    // elle a servi.
                    crate::update::Herd::forget(&dirs);
                });
        }
    }

    fn set_realistic_physics(&mut self, on: bool) {
        if self.settings.realistic_physics == on && self.live.realistic == on {
            return;
        }
        self.settings.realistic_physics = on;
        self.live.realistic = on;
        self.save_settings();
    }

    fn set_collisions(&mut self, on: bool) {
        if self.settings.collisions == on && self.live.collisions == on {
            return;
        }
        self.settings.collisions = on;
        self.live.collisions = on;
        self.save_settings();
    }

    fn set_frontmost_only(&mut self, on: bool) {
        if self.settings.frontmost_only == on {
            return;
        }
        self.settings.frontmost_only = on;
        self.live.frontmost_only = on;
        self.save_settings();
    }

    /// Sur tous les écrans, ou sur le principal seulement : les fenêtres des
    /// écrans suivent au prochain relevé (tout de suite), et les animaux
    /// restés sur un écran qu'on leur retire reviennent d'eux-mêmes.
    fn set_all_screens(&mut self, on: bool) {
        if self.settings.all_screens == on {
            return;
        }
        self.settings.all_screens = on;
        self.save_settings();
        self.next_screen_poll = Instant::now();
    }

    fn set_walk_under_dock(&mut self, on: bool) {
        if self.settings.walk_under_dock == on {
            return;
        }
        self.settings.walk_under_dock = on;
        self.live.walk_under_dock = on;
        self.save_settings();
        // Lire le Dock demande l'autorisation d'Accessibilité : macOS
        // propose de l'accorder si Mouflon ne l'a pas encore.
        if on && !platform::request_accessibility() {
            log::info!("Marcher sous le Dock : en attente de l'autorisation d'Accessibilité");
        }
        // Le relevé lit le Dock tout de suite (ou cesse de le lire).
        if let Some(watcher) = &self.watcher {
            watcher.set_dock(on);
        }
    }

    /// Ouvre la fenêtre des réglages, avec ce que le système a dit en
    /// dernier de l'ouverture de session (relue à côté : la fenêtre suit).
    fn open_settings(&mut self) {
        self.ask_login_item(None);
        let form = crate::settings_window::form(
            crate::i18n::texts(),
            self.login,
            crate::update::in_settings(),
        );
        let values = crate::settings_window::values(&self.shown());
        if !platform::show_form(&form, &values)
            && let Some(paths) = &self.paths
        {
            // Pas de fenêtre des réglages sur ce système : le fichier, à
            // modifier à la main (il est lu au lancement).
            self.settings.save(&paths.settings);
            platform::open_folder(&paths.settings);
        }
    }

    /// La galerie des animaux : une carte par espèce de la bibliothèque. Les
    /// aperçus se dessinent sur un fil à part et arrivent un à un
    /// (`UserEvent::GalleryPreview`) ; `now` : tout de suite, sur ce fil (la
    /// vitrine de la CI).
    fn open_gallery(&mut self, now: bool) {
        let t = crate::i18n::texts();
        // Famille par famille, comme dans le menu.
        let kinds: Vec<(&str, Option<&str>)> = self
            .library
            .pets
            .iter()
            .map(|entry| (entry.def.meta.category(), entry.def.meta.region()))
            .collect();
        let sections = crate::menu::sections(&kinds);
        let several = sections
            .windows(2)
            .any(|pair| pair[0].family != pair[1].family);
        // Une race qui a ses béliers, ses brebis et ses agneaux : la carte
        // laisse choisir (dans l'ordre de `Wanted::ALL`).
        let choices = vec![
            t.gallery_any.clone(),
            t.gallery_ram.clone(),
            t.gallery_ewe.clone(),
            t.gallery_lamb.clone(),
        ];
        let cards: Vec<platform::GalleryCard> = sections
            .into_iter()
            .flat_map(|section| {
                let family = crate::menu::section_name(t, &section, several);
                section.pets.into_iter().map(move |i| (i, family.clone()))
            })
            .map(|(i, family)| {
                let entry = &self.library.pets[i];
                let meta = &entry.def.meta;
                let mut detail = meta.description.clone().unwrap_or_default();
                if let Some(author) = meta.author.as_deref().filter(|a| !a.is_empty()) {
                    if !detail.is_empty() {
                        detail.push('\n');
                    }
                    detail.push_str(&t.gallery_by.replace("{author}", author));
                }
                let male = entry.def.sex_ratio.male_probability();
                platform::GalleryCard {
                    id: i as u32,
                    name: meta.name.clone(),
                    detail,
                    button: t.gallery_add.clone(),
                    choices: if entry.def.has_lambs() && male > 0.0 && male < 1.0 {
                        choices.clone()
                    } else {
                        Vec::new()
                    },
                    family,
                }
            })
            .collect();
        if !platform::show_gallery(&t.gallery_title, &cards) {
            return;
        }
        let entries: Vec<(u32, Entry)> = self
            .library
            .pets
            .iter()
            .cloned()
            .enumerate()
            .map(|(i, e)| (i as u32, e))
            .collect();
        let previews = move |send: &dyn Fn(u32, Vec<platform::GalleryImage>)| {
            for (index, entry) in &entries {
                let Some(art) = entry.art_for_preview() else {
                    continue;
                };
                let frames = mouflon_render::thumbnail::preview(&entry.def, &art, 10, 10.0, 2.0)
                    .into_iter()
                    .map(|thumb| platform::GalleryImage {
                        width: thumb.width,
                        height: thumb.height,
                        rgba: thumb.rgba,
                        scale: 2.0,
                    })
                    .collect();
                send(*index, frames);
            }
        };
        if now {
            previews(&|index, frames| platform::set_gallery_preview(index, &frames));
            return;
        }
        let proxy = Arc::clone(&self.proxy);
        let spawned = std::thread::Builder::new()
            .name("mouflon-galerie".into())
            .spawn(move || {
                platform::lower_thread_priority();
                previews(&|index, frames| {
                    if let Ok(proxy) = proxy.lock() {
                        let _ = proxy.send_event(UserEvent::GalleryPreview(index, frames));
                    }
                });
            });
        if let Err(e) = spawned {
            log::warn!("aperçus de la galerie indisponibles : {e}");
        }
    }

    /// La vitrine de la CI, à son heure (voir [`Showcase`]).
    fn showcase(&mut self, event_loop: &ActiveEventLoop, now: Instant) {
        let Some(showcase) = self.showcase.as_mut() else {
            return;
        };
        if now < showcase.at {
            return;
        }
        if !showcase.opened {
            showcase.opened = true;
            // (Plus longtemps si on veut la voir à l'écran avant.)
            let shown = std::env::var("MOUFLON_VITRINE_ATTENTE")
                .ok()
                .and_then(|seconds| seconds.trim().parse::<f64>().ok())
                .filter(|seconds| seconds.is_finite() && (0.0..=60.0).contains(seconds))
                .unwrap_or(1.0);
            showcase.at = now + Duration::from_secs_f64(shown);
            self.open_settings();
            return;
        }
        let Some(showcase) = self.showcase.take() else {
            return;
        };
        let t = crate::i18n::texts();
        let code = crate::i18n::code();
        let _ = std::fs::create_dir_all(&showcase.dir);
        let image = |name: &str| showcase.dir.join(format!("reglages-{code}-{name}.png"));
        let mut saved = vec![platform::snapshot_form(&image("clair"), false)];
        // Avec ses remarques (autorisation et accord en attente), et la
        // section « Démarrage » même si le système ne la permet pas ici.
        let full = Shown {
            walk_under_dock: true,
            escape_quits: true,
            trusted: false,
            login: platform::LoginItem::NeedsApproval,
            ..self.shown()
        };
        let form = crate::settings_window::form(t, platform::LoginItem::Off, true);
        platform::show_form(&form, &crate::settings_window::values(&full));
        saved.push(platform::snapshot_form(&image("remarques-clair"), false));
        saved.push(platform::snapshot_form(&image("remarques-sombre"), true));
        let state = PetMenuState {
            is_ewe: false,
            can_have_lamb: false,
            paused: false,
            sounds: true,
            size: 1,
        };
        // La galerie, avec ses aperçus.
        self.open_gallery(true);
        saved.push(platform::snapshot_gallery(
            &showcase.dir.join(format!("galerie-{code}-clair.png")),
            false,
        ));
        saved.push(platform::snapshot_gallery(
            &showcase.dir.join(format!("galerie-{code}-sombre.png")),
            true,
        ));
        // « À propos » (une fenêtre de Mouflon sous Windows ; ailleurs,
        // celle du système).
        #[cfg(target_os = "windows")]
        {
            show_about();
            for (name, dark) in [("clair", false), ("sombre", true)] {
                let path = showcase.dir.join(format!("a-propos-{code}-{name}.png"));
                saved.push(platform::snapshot_about(&path, dark));
            }
        }
        let menu = crate::menu::pet_menu("Southdown", state, t);
        let text = format!("{menu:#?}\n");
        let _ = std::fs::write(showcase.dir.join(format!("menu-animal-{code}.txt")), text);
        log::info!("vitrine : images enregistrées {saved:?}");
        event_loop.exit();
    }

    // ——— Les essais ———

    /// Une étape de l'essai, notée au journal.
    fn trial_step(&mut self, step: TrialStep) {
        let name = |index: usize| {
            self.library
                .pets
                .get(index)
                .map_or("?", |entry| entry.def.meta.name.as_str())
        };
        match step {
            TrialStep::Goodbye(index) => {
                self.note(format_args!("essai : au revoir à un(e) {}", name(index)));
            }
            TrialStep::Add(index) => {
                self.note(format_args!("essai : un(e) {} de plus", name(index)));
            }
            TrialStep::Crowd(count) => {
                self.note(format_args!("essai de charge : {count} animaux"));
            }
        }
        match step {
            TrialStep::Goodbye(index) => self.goodbye_one(index),
            TrialStep::Add(index) => self.spawn(index),
            TrialStep::Crowd(count) => self.crowd(count),
        }
    }

    /// Essai de charge : des animaux arrivent jusqu'à en avoir `count`, des
    /// espèces du lancement à tour de rôle (dessins déjà prêts).
    fn crowd(&mut self, count: usize) {
        let mut kinds: Vec<usize> = Vec::new();
        for pet in &self.flock.pets {
            if let Some(i) = self
                .library
                .pets
                .iter()
                .position(|e| Arc::ptr_eq(&e.def, &pet.def))
                && !kinds.contains(&i)
            {
                kinds.push(i);
            }
        }
        if kinds.is_empty() {
            kinds.push(
                self.library
                    .default_index(self.settings.default_pet.as_deref()),
            );
        }
        let have = self.flock.pets.len();
        for k in have..count {
            self.spawn(kinds[k % kinds.len()]);
        }
    }

    /// Le journal d'un essai : ce qu'il faut savoir avant la première image,
    /// et les diagnostics. Renvoie de quoi mesurer les images.
    fn open_journal(&mut self, begun: Instant, duration: Duration) -> FrameStats {
        let stats = FrameStats::new(begun).with_journal();
        stats.raw(format!(
            "ESSAI Mouflon {} ({} s)",
            env!("CARGO_PKG_VERSION"),
            duration.as_secs_f32()
        ));
        stats.raw(format!(
            "VERSION {} commit {} {}",
            env!("CARGO_PKG_VERSION"),
            option_env!("GITHUB_SHA").map_or("local", |sha| &sha[..sha.len().min(12)]),
            if cfg!(debug_assertions) {
                "debug"
            } else {
                "release"
            },
        ));
        let wall = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap_or_default();
        stats.raw(format!(
            "DEBUT {:.3} {}",
            wall.as_secs_f64(),
            chrono::Local::now().format("%Y-%m-%d %H:%M:%S%.3f %z")
        ));
        for line in platform::diag::system_info() {
            stats.raw(line);
        }
        stats.raw(format!(
            "REGLAGES taille {} sons {} collisions {} physique_realiste {} premier_plan {} dock {} surprises {} scene {} animaux {:?}",
            self.settings.size_factor(),
            self.settings.sounds,
            self.live.collisions,
            self.live.realistic,
            self.settings.frontmost_only,
            self.settings.walk_under_dock,
            self.settings.surprises,
            if crate::screens::gpu_asked() {
                "gpu"
            } else {
                "calques"
            },
            crate::stats::trial_pets(),
        ));
        // Diagnostics : pile du fil des images quand une image tarde,
        // ressources du processus, de ses fils et du système, allocations,
        // notifications du système (voir `platform::diag`).
        platform::set_alloc_counting(true);
        platform::diag::register_main_thread();
        if let Some(sender) = stats.sender() {
            crate::stats::install_panic_hook(sender.clone(), begun);
            let to_journal = sender.clone();
            platform::diag::start(
                begun,
                Box::new(move |line| {
                    let _ = to_journal.send(line);
                }),
            );
            platform::diag::observe_system(move |line| {
                let _ = sender.send(line);
            });
        }
        self.notes = Some(stats.notes());
        stats
    }

    /// L'affichage, en route : sur son propre fil si toutes les fenêtres
    /// des écrans montrent la scène en calques (et que le fil peut être
    /// lancé), ici sinon.
    fn start_display(&mut self, stats: Option<FrameStats>, watcher: &Arc<Watcher>) {
        let init = |stats| {
            let proxy = Arc::clone(&self.proxy);
            Init {
                live: self.live,
                dark: self.dark,
                watcher: Some(Arc::clone(watcher)),
                stats,
                reports: Box::new(move |report| {
                    if let Ok(proxy) = proxy.lock() {
                        let _ = proxy.send_event(UserEvent::Display(report));
                    }
                }),
            }
        };
        // `MOUFLON_AFFICHAGE=principal` : l'affichage reste ici (pour comparer).
        let here = std::env::var("MOUFLON_AFFICHAGE").is_ok_and(|v| v == "principal");
        if self.windows.all_layers() && !here {
            match DisplayThread::spawn(init(stats)) {
                Some(thread) => {
                    // Un écran branché plus tard sans calques resterait vide
                    // plutôt que de faire venir le GPU sur ce fil.
                    self.windows.set_layers_only(true);
                    self.display = Displayer::Thread(thread);
                    log::info!(
                        "affichage : calques {}, sur son propre fil",
                        platform::LAYERS
                    );
                    if let Some(notes) = &self.notes {
                        notes.raw(format!(
                            "SCENE calques {}, une fenêtre par écran, fil de l'affichage",
                            platform::LAYERS
                        ));
                    }
                    return;
                }
                None => {
                    // Le journal des images est perdu avec le fil : on le dit.
                    self.note(
                        "fil de l'affichage impossible : images sur le fil principal, sans mesures",
                    );
                    self.display = Displayer::Here(Box::new(Here {
                        display: Display::new(init(None)),
                        beat: Rc::new(Cell::new(None)),
                        coalesced: Rc::new(Cell::new(0)),
                    }));
                    return;
                }
            }
        }
        self.display = Displayer::Here(Box::new(Here {
            display: Display::new(init(stats)),
            beat: Rc::new(Cell::new(None)),
            coalesced: Rc::new(Cell::new(0)),
        }));
        let how = if self.windows.all_layers() {
            format!("calques {}", platform::LAYERS)
        } else {
            "GPU".to_owned()
        };
        log::info!("affichage : {how}, sur le fil principal");
        if let Some(notes) = &self.notes {
            notes.raw(format!("SCENE {how}, une fenêtre par écran, fil principal"));
        }
    }

    /// Sous Windows, les animaux se montrent en calques du système ou pas du
    /// tout (voir `ScreenWindows`) : si aucune fenêtre d'écran n'a pu recevoir
    /// les siens, rien ne se verrait jamais. On le dit, et l'app s'arrête.
    /// `true` s'il n'y a rien à montrer.
    fn cannot_show(&mut self, event_loop: &ActiveEventLoop) -> bool {
        // (La vitrine de la CI ne montre que les fenêtres de Mouflon : elle
        // se passe de la scène.)
        if !cfg!(target_os = "windows")
            || !self.windows.is_empty()
            || crate::screens::gpu_asked()
            || crate::stats::showcase().is_some()
        {
            return false;
        }
        #[cfg(target_os = "windows")]
        {
            let why = platform::scene_problem().unwrap_or_default();
            log::error!("rien ne peut être montré sur cette machine : {why}");
            self.note(format_args!("rien ne peut être montré : {why}"));
            if crate::stats::trial().is_none() {
                let text = format!("{}\n\n{why}", crate::i18n::texts().cannot_show);
                platform::message("Mouflon", text.trim());
            }
        }
        event_loop.exit();
        true
    }

    /// Avant de quitter : l'affichage s'arrête (le journal d'un essai
    /// interrompu reçoit son bilan et part sur le disque), et les fenêtres
    /// des écrans ne seront jamais libérées (voir `keep_pet_window`).
    pub fn shutdown(&mut self) {
        // Quitté pendant l'annonce d'une mise à jour : elle se fera au
        // prochain lancement.
        self.postpone_ready();
        self.display.stop();
        self.windows.shutdown();
        // Sous Windows, l'icône près de l'horloge s'enlève en même temps que
        // le menu (sinon elle resterait, fantôme, jusqu'à ce qu'on la survole).
        #[cfg(target_os = "windows")]
        {
            self.menu = None;
        }
    }

    // ——— Ce que dit l'affichage ———

    fn on_report(&mut self, event_loop: &ActiveEventLoop, report: Report) {
        match report {
            Report::Show(report) => self.on_show(event_loop, report),
            Report::Flock {
                generation,
                done,
                pets,
            } => self.flock_changed(generation, done, pets),
            Report::Hittest(hits) => {
                for (key, on) in hits {
                    self.windows.set_hittest(key, on);
                }
            }
            Report::PetMenu {
                id,
                def,
                can_have_lamb,
            } => self.open_pet_menu(id, def, can_have_lamb),
            Report::Trusted(trusted) => {
                // L'autorisation arrive : l'écoute d'Échap est refaite, pour
                // l'entendre aussi dans les autres apps.
                if trusted && !self.trusted && self.settings.escape_quits {
                    platform::listen_escape(true);
                }
                self.trusted = trusted;
                // La fenêtre des réglages montre ce qui manque (ou plus).
                platform::update_form(&crate::settings_window::values(&self.shown()));
            }
            Report::NeedClock => self.start_clock(),
            Report::TrialOver => event_loop.exit(),
            Report::SceneLost => self.restore_scene(),
        }
    }
}

impl ApplicationHandler<UserEvent> for App {
    fn resumed(&mut self, event_loop: &ActiveEventLoop) {
        if self.started {
            return;
        }
        self.started = true;
        let begun = Instant::now();
        // Les images gardées pour des animaux ou des tailles délaissés s'effacent.
        if let Some(dir) = self.paths.as_ref().map(|p| p.images.clone()) {
            let _ = std::thread::Builder::new()
                .name("mouflon-menage".into())
                .spawn(move || {
                    platform::lower_thread_priority();
                    let removed = mouflon_render::cache::prune(&dir, IMAGES_KEPT);
                    if removed > 0 {
                        log::info!("{removed} fichier(s) d'images délaissés effacés");
                    }
                });
        }
        // Essai (MOUFLON_ESSAI) : journal complet dès le début.
        let trial = crate::stats::trial();
        let mut stats = None;
        if let Some(duration) = trial {
            if crate::stats::trial_collisions() {
                self.live.collisions = true;
            }
            stats = Some(self.open_journal(begun, duration));
        }
        // Le relevé des fenêtres (et du Dock) commence tout de suite, à côté.
        let watcher = Arc::new(Watcher::start(
            self.settings.walk_under_dock,
            self.library.dir().map(std::path::Path::to_path_buf),
            self.notes
                .as_ref()
                .and_then(|notes| Some((notes.sender()?, notes.begun()))),
        ));
        // Ce que le système dit de l'ouverture de session, relu à côté.
        if platform::login_item_available() {
            self.login = platform::LoginItem::Off;
            self.ask_login_item(None);
        }

        // Animaux du lancement : ceux de l'essai s'il en demande, sinon le préféré.
        let mut starters: Vec<usize> = Vec::new();
        if trial.is_some() {
            for id in crate::stats::trial_pets() {
                match self.library.pets.iter().position(|e| e.def.meta.id == id) {
                    Some(i) => starters.push(i),
                    None => log::warn!("essai : animal « {id} » introuvable"),
                }
            }
        }
        // Mouflon revient d'une mise à jour (ou la version suivante n'a pas
        // pris la place) : le troupeau d'avant revient, chacun à sa place.
        let herd = match (&trial, self.arrival, &self.paths) {
            (None, Some(_), Some(paths)) => {
                let sizes = crate::settings::SIZES;
                let least = sizes.iter().copied().fold(f32::INFINITY, f32::min);
                let most = sizes.iter().copied().fold(f32::NEG_INFINITY, f32::max);
                crate::update::Herd::read(&paths.updates, (least, most))
            }
            // Lancé comme d'habitude : un troupeau noté pour une version
            // qui n'est jamais arrivée ne sert plus.
            (_, None, Some(paths)) => {
                crate::update::Herd::forget(&paths.updates);
                None
            }
            _ => None,
        };
        let back: Vec<(usize, crate::update::Animal, Entry)> = herd
            .iter()
            .flat_map(|herd| herd.animals.iter().enumerate())
            .filter_map(|(i, animal)| Some((i, animal.clone(), self.entry_back(animal)?)))
            .collect();
        // (Un troupeau noté dont plus aucune espèce n'est là : les animaux
        // du lancement arrivent comme d'habitude.)
        if back.is_empty() && starters.is_empty() {
            let index = self
                .library
                .default_index(self.settings.default_pet.as_deref());
            starters = vec![index; self.settings.start_count.clamp(1, 32) as usize];
        }
        // Leurs dessins se décodent dès maintenant, une espèce par fil, pendant
        // que le fil principal crée le menu et met l'affichage en route.
        let mut species = self.decode_ahead(&starters);
        for (_, _, entry) in &back {
            if !species.iter().any(|e| Arc::ptr_eq(&e.def, &entry.def)) {
                species.push(entry.clone());
                if !entry.art_ready() {
                    let entry = entry.clone();
                    let _ = std::thread::Builder::new()
                        .name("mouflon-dessins".into())
                        .spawn(move || {
                            let _ = entry.art();
                        });
                }
            }
        }

        // Le menu d'un animal et la fenêtre des réglages : leurs événements
        // arrivent par la boucle, comme ceux de la barre des menus.
        let proxy = Arc::clone(&self.proxy);
        platform::set_ui_handler(move |event| {
            if let Ok(proxy) = proxy.lock() {
                let _ = proxy.send_event(UserEvent::Ui(event));
            }
        });
        // Vitrine de la CI : la fenêtre des réglages s'ouvre après l'arrivée
        // des animaux.
        if let Some(dir) = crate::stats::showcase() {
            self.showcase = Some(Showcase {
                dir,
                at: begun + Duration::from_secs(3),
                opened: false,
            });
        }
        #[cfg(any(target_os = "macos", target_os = "windows"))]
        {
            let proxy = Arc::clone(&self.proxy);
            crate::menu::MenuEvent::set_event_handler(Some(move |event| {
                if let Ok(proxy) = proxy.lock() {
                    let _ = proxy.send_event(UserEvent::Menu(event));
                }
            }));
            match crate::menu::TrayMenu::new(&self.library, self.settings.sounds) {
                Ok(menu) => self.menu = Some(menu),
                Err(e) => log::error!("impossible de créer le menu : {e}"),
            }
        }
        // Les noms et les nombres du menu sont justes avant même la première image.
        self.sync_menu();
        // Les raccourcis clavier, dans toutes les apps (montrés dans le menu).
        if trial.is_none() {
            self.apply_shortcuts();
            if self.settings.escape_quits {
                platform::listen_escape(true);
            }
        }

        // Préchargement, avant la première image : dessins des animaux du
        // lancement, affichage, sons, premier relevé des fenêtres. Rien de
        // tout cela ne doit tomber pendant la première chute.
        let mut mark = Instant::now();
        let mut steps = vec![("menu", mark - begun)];
        if self.settings.sounds {
            self.warm_up_sounds();
        }
        steps.push(("sons", lap(&mut mark)));
        // Le premier relevé, et les écrans : une fenêtre transparente par
        // écran, où l'affichage pose la scène.
        let first = watcher.wait_first(Duration::from_millis(500));
        self.watcher = Some(Arc::clone(&watcher));
        let (screens, exact) = self.chosen_screens(event_loop);
        self.screens = screens;
        self.windows.sync(event_loop, &self.screens, exact);
        if let Some(dark) = self.windows.dark() {
            self.dark = dark;
        }
        self.next_screen_poll = Instant::now() + SCREEN_POLL;
        steps.push(("relevé et fenêtres", lap(&mut mark)));
        if self.cannot_show(event_loop) {
            return;
        }
        self.start_display(stats, &watcher);
        self.live_sent = Some(self.live);
        self.send_screens();
        if let Some(snapshot) = first {
            self.send(Command::Survey(snapshot));
        }
        steps.push(("affichage", lap(&mut mark)));
        // Le GPU, seulement s'il sert (sans calques Core Animation).
        if let Displayer::Here(here) = &mut self.display
            && let Some(gpu) = here.display.scene_mut().gpu_mut()
        {
            gpu.prepare_ahead();
            if let Some(gpu) = gpu.describe() {
                log::info!("GPU : {gpu}");
                if let Some(notes) = &self.notes {
                    notes.raw(format!("GPU {gpu}"));
                }
            }
            steps.push(("GPU", lap(&mut mark)));
        }
        // Les dessins décodés à côté : on attend ceux qui ne sont pas prêts.
        for entry in &species {
            let _ = entry.art();
        }
        steps.push(("fin des dessins", lap(&mut mark)));
        // Le premier sur l'écran où l'on regarde, les autres sur n'importe
        // lequel (un essai, lui, ne dépend pas de l'endroit du curseur).
        for (k, &i) in starters.iter().enumerate() {
            self.here = k == 0 && trial.is_none();
            self.spawn(i);
        }
        self.here = false;
        // Le retour : la soucoupe repose le troupeau si c'est elle qui
        // l'avait pris, et l'un d'eux montre le numéro de la nouvelle
        // version (pas celui d'une version qui n'a fait que revenir).
        if trial.is_none()
            && let Some(arrival) = self.arrival
        {
            let new = arrival == crate::update::Arrival::New;
            let scene = match herd.as_ref().map(|herd| herd.scene) {
                Some(crate::update::Staged::Saucer) if new => mouflon_core::show::Scene::Saucer,
                _ => mouflon_core::show::Scene::Herald,
            };
            if new || !back.is_empty() {
                self.send(Command::Welcome {
                    scene,
                    label: if new {
                        crate::update::Version::current().to_string()
                    } else {
                        String::new()
                    },
                    places: back.iter().map(|(_, animal, _)| animal.x).collect(),
                });
            }
            // Chacun revient à sa place. Un agneau dont la mère revient (et
            // peut en avoir) arrive avec elle, chacun à sa place ; sans elle,
            // il revient seul, à sa place.
            let mothers: Vec<usize> = back
                .iter()
                .filter(|(_, animal, entry)| {
                    !matches!(
                        animal.worn,
                        Some(crate::update::Worn::Lamb | crate::update::Worn::Ram)
                    ) && entry.def.has_lambs()
                })
                .map(|(i, _, _)| *i)
                .collect();
            for (i, animal, entry) in &back {
                if animal.mother.is_some_and(|m| mothers.contains(&m)) {
                    continue;
                }
                let lambs: Vec<f32> = back
                    .iter()
                    .filter(|(_, lamb, _)| mothers.contains(i) && lamb.mother == Some(*i))
                    .map(|(_, lamb, _)| lamb.x)
                    .collect();
                self.spawn_back(animal, entry, lambs);
            }
        }
        // Leurs premières images se préparent, puis ils arrivent.
        self.send(Command::Start { settle: SETTLE });
        steps.push(("animaux", lap(&mut mark)));
        self.start_clock();
        // Une surprise tout de suite, pour la voir (`MOUFLON_SURPRISE=vent`
        // ou `soucoupe`).
        let soon = Instant::now() + SETTLE + Duration::from_secs(4);
        match std::env::var("MOUFLON_SURPRISE").ok().as_deref() {
            Some("vent") => self.send(Command::Surprise(soon, Surprise::Wind)),
            Some("soucoupe") => self.send(Command::Surprise(soon, Surprise::Saucer)),
            _ => {}
        }
        // L'annonce d'une mise à jour tout de suite, pour la voir pendant un
        // essai (`MOUFLON_ANNONCE=annonceur` ou `soucoupe`) : le troupeau
        // s'en va, et rien d'autre ne se passe.
        if trial.is_some() {
            match std::env::var("MOUFLON_ANNONCE").ok().as_deref() {
                Some("annonceur") => {
                    self.send(Command::Farewell(mouflon_core::show::Scene::Herald));
                }
                Some("soucoupe") => {
                    self.send(Command::Farewell(mouflon_core::show::Scene::Saucer));
                }
                _ => {}
            }
        }
        let detail: Vec<String> = steps
            .iter()
            .map(|(step, time)| format!("{step} {}", time.as_millis()))
            .collect();
        log::info!(
            "préchargement : {} ms ({})",
            begun.elapsed().as_millis(),
            detail.join(", ")
        );
        // La recherche d'une nouvelle version, à côté ; et, pour une version
        // qui arrive, son signe de vie à celle d'avant.
        self.start_updater();
        if self.arrival.is_some() {
            self.confirm_at = Some(Instant::now() + ARRIVAL_SETTLE);
        }
        // Instantanés de la scène (`MOUFLON_INSTANTANES=dossier`) : pendant
        // un essai, à l'arrivée, au coup de vent, à la soucoupe et vers la
        // fin ; sinon, toutes les trente secondes pendant cinq minutes.
        if let Some(dir) = std::env::var_os("MOUFLON_INSTANTANES").filter(|d| !d.is_empty()) {
            let start = Instant::now();
            let mut times: Vec<Instant> = match trial {
                Some(d) => [
                    Duration::from_secs(4),
                    Duration::from_secs(12),
                    d / 3 + Duration::from_secs(2),
                    d * 2 / 3 + Duration::from_secs(5),
                    d * 2 / 3 + Duration::from_secs(9),
                    d.saturating_sub(Duration::from_secs(8)),
                ]
                .into_iter()
                .map(|t| start + t)
                .collect(),
                None => (1..=10)
                    .map(|k| start + Duration::from_secs(30 * k))
                    .collect(),
            };
            times.sort();
            self.send(Command::Snapshots(dir.into(), times));
        }
        if let Some(duration) = trial {
            log::info!("essai de {:.1} s", duration.as_secs_f32());
            let start = Instant::now();
            self.send(Command::TrialEnd(start + duration));
            // À la moitié, puis aux trois quarts, le dernier arrivé d'une
            // espèce s'en va et un autre arrive : de la même espèce la
            // première fois, la seconde fois la Solognote (des morceaux
            // peints sur un squelette), pour passer aussi par elle.
            let mut kinds: Vec<usize> = Vec::new();
            for &i in &starters {
                if !kinds.contains(&i) {
                    kinds.push(i);
                }
            }
            let painted = self
                .library
                .pets
                .iter()
                .position(|e| e.def.meta.id == "solognote");
            // Essai de charge : des paliers d'animaux au lieu des départs et arrivées.
            if crate::stats::trial_load() {
                for (fraction, count) in crate::stats::LOAD_STAGES {
                    self.trial_steps
                        .push((start + duration.mul_f32(fraction), TrialStep::Crowd(count)));
                }
            }
            let swaps: &[Duration] = if crate::stats::trial_load() {
                &[]
            } else {
                &[duration / 2, duration * 3 / 4]
            };
            for (k, &at) in swaps.iter().enumerate() {
                if let Some(&kind) = kinds.get(k % kinds.len().max(1)) {
                    let arrives = if k == 1 {
                        painted.unwrap_or(kind)
                    } else {
                        kind
                    };
                    self.trial_steps
                        .push((start + at, TrialStep::Goodbye(kind)));
                    self.trial_steps
                        .push((start + at + Duration::from_secs(5), TrialStep::Add(arrives)));
                }
            }
            self.trial_steps.sort_by_key(|(at, _)| *at);
            // Les surprises passent aussi : un coup de vent au premier tiers,
            // une soucoupe aux deux tiers.
            self.send(Command::Surprise(start + duration / 3, Surprise::Wind));
            self.send(Command::Surprise(
                start + duration * 2 / 3,
                Surprise::Saucer,
            ));
            if let Some(notes) = &self.notes {
                for (i, screen) in self.screens.iter().enumerate() {
                    notes.raw(format!(
                        "ECRAN {i} {:.0}x{:.0} en {:.0},{:.0} echelle {}",
                        screen.frame.w,
                        screen.frame.h,
                        screen.frame.x,
                        screen.frame.y,
                        screen.scale
                    ));
                }
                notes.raw(format!(
                    "PRECHARGEMENT {} ms ({})",
                    begun.elapsed().as_millis(),
                    detail.join(", ")
                ));
            }
        }
    }

    #[cfg_attr(
        not(any(target_os = "macos", target_os = "windows")),
        allow(unused_variables)
    )]
    fn user_event(&mut self, event_loop: &ActiveEventLoop, event: UserEvent) {
        let started = self.handler_start();
        let kind = match &event {
            #[cfg(any(target_os = "macos", target_os = "windows"))]
            UserEvent::Menu(_) => "menu de la barre",
            UserEvent::Frame => "battement",
            UserEvent::ArtReady(_) => "dessin prêt",
            UserEvent::ArtCompressed(_) => "images compressées",
            UserEvent::PetSizeReady(..) => "dessin à une autre taille prêt",
            UserEvent::LambReady { .. } => "dessin de l’agneau prêt",
            UserEvent::GalleryPreview(..) => "aperçu de la galerie prêt",
            UserEvent::Ui(_) => "menu d'un animal ou réglages",
            UserEvent::LoginItem(_) => "ouverture de session relue",
            UserEvent::Display(_) => "nouvelles de l'affichage",
            UserEvent::Update(_) => "nouvelles de la mise à jour",
        };
        // Les battements sont comptés par l'affichage.
        if let Some(notes) = &self.notes
            && !matches!(event, UserEvent::Frame)
        {
            Counters::bump(&notes.counters.user);
        }
        match event {
            #[cfg(any(target_os = "macos", target_os = "windows"))]
            UserEvent::Menu(event) => {
                if let Some(action) = self.menu.as_ref().and_then(|m| m.action(&event)) {
                    self.handle_action(event_loop, action);
                }
            }
            UserEvent::Frame => {
                if let Displayer::Here(here) = &mut self.display
                    && let Some(tick) = here.beat.take()
                {
                    here.display.beat_received();
                    if here.display.started() && here.display.clock_running() {
                        let frame = Instant::now();
                        here.display.frame(Some(tick), here.coalesced.replace(0));
                        self.frame_time += frame.elapsed();
                    }
                }
            }
            UserEvent::ArtReady(id) => self.art_ready(&id),
            UserEvent::ArtCompressed(id) => self.art_compressed(&id),
            UserEvent::PetSizeReady(pet, def) => self.pet_size_ready(pet, &def),
            UserEvent::LambReady { goal, from, entry } => self.lamb_ready(goal, from, entry),
            UserEvent::GalleryPreview(index, frames) => {
                platform::set_gallery_preview(index, &frames);
            }
            UserEvent::LoginItem(state) => {
                if state != self.login {
                    self.login = state;
                    self.publish();
                }
            }
            UserEvent::Ui(platform::UiEvent::MenuClosed) => {
                // L'animal du menu repart d'où il était.
                self.send(Command::MenuClosed);
            }
            UserEvent::Ui(event) => {
                let action = match event {
                    platform::UiEvent::Menu(id) => self.pet_menu_action(id),
                    platform::UiEvent::Shortcut(id) => self.shortcut_pressed(id),
                    // Mouflon lancé une seconde fois : un animal arrive, on
                    // voit qu'il est là.
                    platform::UiEvent::Relaunched => Some(Action::AddOneRandom),
                    platform::UiEvent::Escape(change) => self.escape_moved(change),
                    // « Ajouter » sur une carte de la galerie : son espèce.
                    platform::UiEvent::GalleryAdd(index, choice) => Some(Action::AddWanted(
                        index as usize,
                        Wanted::ALL
                            .get(choice as usize)
                            .copied()
                            .unwrap_or_default(),
                    )),
                    other => crate::settings_window::action(other),
                };
                if let Some(action) = action {
                    self.handle_action(event_loop, action);
                }
            }
            UserEvent::Display(report) => self.on_report(event_loop, report),
            UserEvent::Update(event) => self.on_update(event),
        }
        self.handler_done(kind, started);
    }

    fn window_event(
        &mut self,
        _event_loop: &ActiveEventLoop,
        window_id: WindowId,
        event: WindowEvent,
    ) {
        let started = self.handler_start();
        let kind = window_event_kind(&event);
        if let Some(notes) = &self.notes {
            let c = &notes.counters;
            match &event {
                WindowEvent::CursorMoved { .. } => Counters::bump(&c.cursor_moves),
                WindowEvent::MouseInput { .. } => Counters::bump(&c.clicks),
                WindowEvent::CursorEntered { .. } => Counters::bump(&c.hovers),
                WindowEvent::RedrawRequested => Counters::bump(&c.redraws),
                _ => Counters::bump(&c.other_window),
            }
        }
        self.on_window_event(window_id, event);
        self.handler_done(kind, started);
    }

    fn about_to_wait(&mut self, event_loop: &ActiveEventLoop) {
        let started = self.handler_start();
        if let Some(notes) = &self.notes {
            Counters::bump(&notes.counters.waits);
        }
        if !self.started {
            self.handler_done("avant l'attente", started);
            return;
        }
        let now = Instant::now();
        if now >= self.next_screen_poll {
            self.poll(event_loop, now);
        }
        // Un essai fait partir un animal à mi-parcours, puis en fait venir un.
        while self.trial_steps.first().is_some_and(|(at, _)| now >= *at) {
            let (_, step) = self.trial_steps.remove(0);
            self.trial_step(step);
        }
        self.showcase(event_loop, now);
        if self.confirm_at.is_some_and(|at| now >= at) {
            self.confirm_arrival();
        }
        // L'affichage sur ce fil : l'horloge de l'écran donne le rythme ;
        // sans elle (ou si elle se tait), on avance ici, à l'heure prévue.
        if let Displayer::Here(here) = &mut self.display
            && here.display.timer_due(Instant::now())
        {
            let frame = Instant::now();
            here.display.frame(None, 0);
            self.frame_time += frame.elapsed();
        }
        // Prochain réveil : le prochain relevé, la prochaine étape de
        // l'essai, la vitrine, la prochaine image (affichage sur ce fil).
        let mut next = self.next_screen_poll;
        if let Some((at, _)) = self.trial_steps.first() {
            next = next.min(*at);
        }
        if let Some(showcase) = &self.showcase {
            next = next.min(showcase.at);
        }
        if let Some(at) = self.confirm_at {
            next = next.min(at);
        }
        if let Displayer::Here(here) = &self.display
            && let Some(at) = here.display.wake_at()
        {
            next = next.min(at);
        }
        event_loop.set_control_flow(ControlFlow::WaitUntil(next));
        self.handler_done("avant l'attente", started);
    }
}

impl App {
    /// Début d'un gestionnaire d'événements (pour noter ceux qui sont lents).
    fn handler_start(&mut self) -> Option<Instant> {
        self.frame_time = Duration::ZERO;
        self.notes.is_some().then(Instant::now)
    }

    /// Fin d'un gestionnaire : au-delà de 4 ms hors de la préparation des
    /// images (notée à part), il est noté au journal d'un essai.
    fn handler_done(&mut self, kind: &str, started: Option<Instant>) {
        let (Some(started), Some(notes)) = (started, self.notes.as_ref()) else {
            return;
        };
        let other = started.elapsed().saturating_sub(self.frame_time);
        if other > Duration::from_millis(4) {
            let ms = other.as_secs_f32() * 1000.0;
            Counters::bump(&notes.counters.slow_handlers);
            notes
                .counters
                .slowest_handler_us
                .fetch_max((ms * 1000.0) as u32, std::sync::atomic::Ordering::Relaxed);
            notes.note(format_args!("gestionnaire lent : {kind} {ms:.1} ms"));
        }
    }

    fn on_window_event(&mut self, window_id: WindowId, event: WindowEvent) {
        // Seules les fenêtres des écrans parlent à l'affichage.
        let Some(screen) = self.windows.get(window_id) else {
            return;
        };
        let key = screen.info.key;
        match event {
            // Le curseur arrive sur un animal : on va peut-être l'attraper.
            WindowEvent::CursorEntered { .. } => self.display.send_quiet(Command::Hover),
            // Clic droit : le menu de l'animal.
            WindowEvent::MouseInput {
                state: ElementState::Pressed,
                button: MouseButton::Right,
                ..
            } => self.send(Command::Menu),
            WindowEvent::MouseInput {
                state,
                button: MouseButton::Left,
                ..
            } => match state {
                // Contrôle-clic : le menu de l'animal aussi, comme partout sur
                // Mac. Rien n'est appuyé ensuite : le bouton relâché (si le
                // menu le laisse passer) n'est pas un « coucou ».
                ElementState::Pressed if platform::control_key_down() => {
                    self.send(Command::Menu);
                }
                ElementState::Pressed => self.send(Command::Press(self.cursor)),
                ElementState::Released => self.send(Command::Release(self.cursor)),
            },
            WindowEvent::CursorMoved { position, .. } => {
                self.cursor = self.windows.to_global(window_id, position);
                // Sur ce fil, l'affichage prend aussi la position des
                // événements (au cas où le système ne la donnerait pas) ;
                // sur le sien, il la demande lui-même au système.
                if matches!(self.display, Displayer::Here(_))
                    && let Some(cursor) = self.cursor
                {
                    self.display.send_quiet(Command::Cursor(cursor));
                }
            }
            WindowEvent::ScaleFactorChanged {
                scale_factor,
                mut inner_size_writer,
            } => {
                // Une fenêtre d'écran posée au pixel près garde sa taille
                // (voir `ScreenWindows::kept_size`).
                if let Some(size) = self.windows.kept_size(window_id) {
                    let _ = inner_size_writer.request_inner_size(size);
                }
                if self.windows.scale_changed(window_id, scale_factor) {
                    self.send_screens();
                }
            }
            WindowEvent::Resized(size) => self.send(Command::Resized {
                key,
                width: size.width,
                height: size.height,
            }),
            WindowEvent::DroppedFile(path) => self.install_pet(&path),
            // Les animaux se redessinent d'eux-mêmes (le thème fait partie
            // de ce qui les identifie).
            WindowEvent::ThemeChanged(theme) => {
                self.dark = theme == Theme::Dark;
                self.send(Command::Dark(self.dark));
            }
            _ => {}
        }
    }
}

/// Seul le dessin de la demande encore courante peut être appliqué. Une
/// ancienne réponse pour la même taille ne doit jamais attendre la nouvelle
/// cuisson sur le fil principal.
fn take_ready_resize(
    resizing: &mut Vec<(u32, Entry)>,
    id: u32,
    def: &Arc<PetDef>,
) -> Option<Entry> {
    let at = resizing
        .iter()
        .position(|(pet, entry)| *pet == id && Arc::ptr_eq(&entry.def, def) && entry.art_ready())?;
    Some(resizing.remove(at).1)
}

/// Temps écoulé depuis `mark`, qui passe à maintenant.
fn lap(mark: &mut Instant) -> Duration {
    let now = Instant::now();
    let lap = now - *mark;
    *mark = now;
    lap
}

/// Le nom d'un événement de fenêtre, pour le journal d'un essai.
fn window_event_kind(event: &WindowEvent) -> &'static str {
    match event {
        WindowEvent::MouseInput { .. } => "clic",
        WindowEvent::CursorMoved { .. } => "souris",
        WindowEvent::CursorEntered { .. } => "souris entrée",
        WindowEvent::CursorLeft { .. } => "souris sortie",
        WindowEvent::RedrawRequested => "redessin demandé",
        WindowEvent::Resized(_) => "taille",
        WindowEvent::Moved(_) => "déplacée",
        WindowEvent::ScaleFactorChanged { .. } => "échelle",
        WindowEvent::ThemeChanged(_) => "thème",
        WindowEvent::DroppedFile(_) => "fichier déposé",
        WindowEvent::Focused(_) => "focus",
        WindowEvent::Occluded(_) => "occultée",
        _ => "autre événement de fenêtre",
    }
}

/// Le troupeau qui part, tel que la version qui arrive le fera revenir :
/// chacun avec son espèce et sa taille (`describe` ; sans elles, il n'est
/// pas noté), son dessin et sa place ; un agneau qui suit sa mère, avec le
/// rang de celle-ci dans la liste (elle reviendra avec lui, chacun à sa
/// place).
fn herd_animals(
    leavers: &[Leaver],
    describe: impl Fn(u32) -> Option<(String, f32)>,
) -> Vec<crate::update::Animal> {
    use crate::update::{Animal, Worn};
    let known: Vec<(&Leaver, String, f32)> = leavers
        .iter()
        .filter_map(|leaver| {
            let (species, size) = describe(leaver.id)?;
            Some((leaver, species, size))
        })
        .collect();
    let rank = |id: u32| known.iter().position(|(leaver, _, _)| leaver.id == id);
    known
        .iter()
        .map(|(leaver, species, size)| Animal {
            species: species.clone(),
            worn: if leaver.lamb {
                Some(Worn::Lamb)
            } else {
                leaver.variant.map(|variant| match variant {
                    mouflon_core::Variant::Male => Worn::Ram,
                    mouflon_core::Variant::Female => Worn::Ewe,
                    mouflon_core::Variant::Lamb => Worn::Lamb,
                })
            },
            x: leaver.x,
            size: Some(*size),
            mother: leaver
                .mother
                .and_then(&rank)
                .filter(|&m| !known[m].0.lamb && known[m].0.mother.is_none()),
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn defs() -> (Arc<PetDef>, Arc<PetDef>) {
        let def = Arc::new(crate::library::builtin_pet().expect("animal intégré"));
        let big = Arc::new(def.scaled(2.0));
        (def, big)
    }

    fn pets(def: &Arc<PetDef>, ids: &[u32]) -> Vec<PetSummary> {
        ids.iter()
            .map(|&id| PetSummary {
                id,
                is_ewe: false,
                def: Arc::clone(def),
                extra: false,
                lambs: Vec::new(),
            })
            .collect()
    }

    #[test]
    fn a_description_made_before_a_request_keeps_what_was_asked() {
        let (def, big) = defs();
        // La cinquième demande : n° 2 prend une autre taille.
        let mut handed = vec![(5, Handed::Pet(2, Arc::clone(&big)))];
        // Décrit après quatre demandes : elle n'y est pas encore.
        let flock = FlockView::described(1, 4, pets(&def, &[1, 2]), &mut handed);
        assert!(Arc::ptr_eq(flock.def_of(2).expect("n° 2"), &big));
        assert!(Arc::ptr_eq(flock.def_of(1).expect("n° 1"), &def));
        assert_eq!(handed.len(), 1);
        // Décrit après la cinquième : la description fait foi.
        let flock = FlockView::described(2, 5, pets(&def, &[1, 2]), &mut handed);
        assert!(Arc::ptr_eq(flock.def_of(2).expect("n° 2"), &def));
        assert!(handed.is_empty());
    }

    #[test]
    fn a_new_size_for_everyone_shows_before_the_display_tells() {
        let (def, big) = defs();
        let mut handed = vec![(3, Handed::Defs(vec![(Arc::clone(&def), Arc::clone(&big))]))];
        let flock = FlockView::described(1, 2, pets(&def, &[1, 2, 3]), &mut handed);
        assert!(flock.pets.iter().all(|p| Arc::ptr_eq(&p.def, &big)));
        assert!(flock.def_of(4).is_none());
    }
    #[test]
    fn every_new_lamb_sharing_an_image_gets_the_pending_size_once() {
        let def = Arc::new(crate::library::builtin_pet().unwrap().as_lamb(2.0).unwrap());
        let other = Arc::new((*def).clone());
        let pending = Arc::new((*def).clone());
        let flock = FlockView {
            generation: Some(1),
            pets: vec![
                PetSummary {
                    id: 4,
                    def: Arc::clone(&def),
                    is_ewe: false,
                    extra: false,
                    lambs: Vec::new(),
                },
                PetSummary {
                    id: 8,
                    def: Arc::clone(&def),
                    is_ewe: false,
                    extra: false,
                    lambs: Vec::new(),
                },
                PetSummary {
                    id: 9,
                    def: other,
                    is_ewe: false,
                    extra: false,
                    lambs: Vec::new(),
                },
            ],
        };
        let mut waiting = vec![
            OffspringSize {
                def: Arc::clone(&def),
                size: 3,
                applied: vec![],
            },
            OffspringSize {
                def,
                size: 3,
                applied: vec![],
            },
            OffspringSize {
                def: Arc::clone(&pending),
                size: 2,
                applied: vec![],
            },
        ];
        assert_eq!(
            flock.take_offspring_sizes(&mut waiting, &[(10, Arc::clone(&pending))]),
            [(4, 3), (8, 3)]
        );
        assert_eq!(waiting.len(), 1);
        assert!(flock.take_offspring_sizes(&mut waiting, &[]).is_empty());
        assert!(
            waiting.is_empty(),
            "la naissance refusée n’attend plus après son accusé"
        );
    }
    #[test]
    fn a_stale_size_completion_never_takes_a_new_request_for_the_same_size() {
        let library = Library::load(None, None, 1.0);
        let parent = library.pets[0].clone();
        let young = parent.as_lamb(2.0).unwrap();
        let old = young.at_size(2.0);
        let current = young.at_size(2.0);
        let mut pending = vec![(7, current.clone())];
        assert!(take_ready_resize(&mut pending, 7, &old.def).is_none());
        assert!(take_ready_resize(&mut pending, 7, &current.def).is_none());
        assert_eq!(
            pending.len(),
            1,
            "aucune attente ni retrait pendant le décodage"
        );
        current.art().expect("dessin préparé sur le fil de travail");
        assert!(take_ready_resize(&mut pending, 7, &current.def).is_some());
        assert!(pending.is_empty());
    }
    #[test]
    fn a_pending_global_size_survives_partial_reports_of_shared_lambs() {
        let def = Arc::new(crate::library::builtin_pet().unwrap().as_lamb(2.0).unwrap());
        let mut flock = FlockView {
            generation: Some(1),
            pets: vec![PetSummary {
                id: 4,
                def: Arc::clone(&def),
                is_ewe: false,
                extra: false,
                lambs: Vec::new(),
            }],
        };
        let mut waiting = vec![OffspringSize {
            def: Arc::clone(&def),
            size: 3,
            applied: vec![],
        }];
        assert_eq!(
            flock.take_offspring_sizes(&mut waiting, &[(12, Arc::clone(&def))]),
            [(4, 3)]
        );
        assert_eq!(waiting.len(), 1);
        assert!(
            flock
                .take_offspring_sizes(&mut waiting, &[(12, Arc::clone(&def))])
                .is_empty()
        );
        flock.pets.push(PetSummary {
            id: 8,
            def,
            is_ewe: false,
            extra: false,
            lambs: Vec::new(),
        });
        assert_eq!(flock.take_offspring_sizes(&mut waiting, &[]), [(8, 3)]);
        assert!(waiting.is_empty());
    }

    #[test]
    fn a_ewe_is_noted_with_each_of_her_lambs_and_an_orphan_lamb_alone() {
        use crate::update::Worn;
        let leaver = |id, x, lamb, mother| Leaver {
            id,
            x,
            variant: None,
            mother,
            lamb,
        };
        let leavers = [
            leaver(1, 100.0, false, None),
            leaver(2, 140.0, true, Some(1)),
            leaver(3, 180.0, true, Some(1)),
            leaver(4, 500.0, true, None),
            leaver(5, 600.0, true, Some(9)),
            leaver(6, 700.0, false, None),
            leaver(7, 720.0, true, Some(2)),
        ];
        // (Le n° 6 n'est pas décrit : il n'est pas noté, et les rangs
        // suivent ; le n° 7 suit un agneau, ce n'est pas une mère.)
        let animals = herd_animals(&leavers, |id| {
            (id != 6).then(|| ("texel".to_string(), 1.25))
        });
        let mothers: Vec<Option<usize>> = animals.iter().map(|a| a.mother).collect();
        assert_eq!(mothers, [None, Some(0), Some(0), None, None, None]);
        let xs: Vec<f32> = animals.iter().map(|a| a.x).collect();
        assert_eq!(xs, [100.0, 140.0, 180.0, 500.0, 600.0, 720.0]);
        assert_eq!(animals[0].worn, None);
        assert!(animals[1..].iter().all(|a| a.worn == Some(Worn::Lamb)));
    }
}
