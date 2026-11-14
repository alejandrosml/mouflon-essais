//! Couche système de Mouflon.
//!
//! C'est le **seul** crate du projet autorisé à contenir de l'`unsafe` : appeler
//! macOS (ou un autre système) depuis Rust l'est par nature, car le
//! compilateur ne peut pas vérifier du code C ou Objective-C. Tout est isolé
//! ici, derrière des fonctions sûres, et chaque bloc unsafe est justifié.
//!
//! Repère commun : points logiques, origine en haut à gauche de l'écran
//! principal, y vers le bas (celui de winit et de `CGWindowListCopyWindowInfo`).

#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct OsRect {
    pub x: f64,
    pub y: f64,
    pub w: f64,
    pub h: f64,
}

/// Fenêtre d'une autre application.
#[derive(Clone, Debug, PartialEq)]
pub struct OsWindow {
    pub id: u64,
    pub pid: i32,
    pub frame: OsRect,
    /// Niveau d'empilement (macOS : 0 pour une fenêtre ordinaire, davantage
    /// pour les panneaux flottants, le Dock, la barre des menus…).
    pub layer: i32,
    /// Nom du processus propriétaire (pour le journal).
    pub owner: String,
}

/// Niveau du Dock : au-dessus, ce sont des éléments du système (barre des
/// menus, menus, bulles de notification…).
const DOCK_LEVEL: i32 = 20;
/// Niveau des panneaux d'accessibilité, dont le visualiseur de clavier.
const ASSISTIVE_TECH_HIGH_LEVEL: i32 = 1500;

impl OsWindow {
    /// Panneau qui flotte au-dessus des applications (visualiseur de clavier,
    /// vidéo en incrustation…) : visible quelle que soit l'app au premier plan.
    #[must_use]
    pub fn is_floating(&self) -> bool {
        self.layer != 0
    }

    /// Panneau d'accessibilité du système : visualiseur de clavier, clavier
    /// d'accessibilité. Il vit à un niveau élevé (celui des menus), mais c'est
    /// un vrai panneau posé sur le bureau.
    #[must_use]
    pub fn is_accessibility_panel(&self) -> bool {
        let owner = self.owner.to_lowercase();
        [
            "assistivecontrol",
            "keyboardviewer",
            "keyboard viewer",
            "visualiseur de clavier",
        ]
        .iter()
        .any(|name| owner.contains(name))
    }

    /// Peut-on marcher dessus ? Oui pour les fenêtres ordinaires, les panneaux
    /// flottants et utilitaires et les panneaux d'accessibilité ; non pour le
    /// Dock, la barre des menus, les menus et les notifications. (Les calques
    /// transparents qui couvrent tout un écran sont écartés à part, avec la
    /// taille des écrans.)
    #[must_use]
    pub fn is_walkable(&self) -> bool {
        (0..DOCK_LEVEL).contains(&self.layer)
            || self.layer == ASSISTIVE_TECH_HIGH_LEVEL
            || self.is_accessibility_panel()
    }
}

/// Identifiant du visualiseur de clavier quand on le lit dans ses réglages
/// (voir [`keyboard_viewer_frame`]) : hors des numéros de fenêtres de macOS
/// (32 bits), et distinct de celui du Dock dans le cœur (`1 << 40`).
pub const KEYBOARD_VIEWER_ID: u64 = 1 << 41;
/// Niveau du visualiseur de clavier quand macOS l'annonçait encore (celui
/// des menus surgissants) : devant les fenêtres des apps et le Dock.
pub const KEYBOARD_VIEWER_LEVEL: i32 = 101;
/// Nom de son app dans la liste des fenêtres (et pour `lsappinfo`).
pub const KEYBOARD_VIEWER_OWNER: &str = "AssistiveControl";

/// Cadre du visualiseur de clavier, dans le repère commun, d'après le
/// réglage où son app le garde à jour (`PanelFrame`, sous la forme
/// `"{{x, y}, {largeur, hauteur}}"`, dans le repère de Cocoa : origine en bas
/// à gauche de l'écran principal, y vers le haut). `primary_height` :
/// hauteur de l'écran principal. Rien si le texte ne décrit pas un
/// rectangle.
///
/// Depuis macOS 27.2, le système n'annonce plus le visualiseur dans la
/// liste des fenêtres : relevé du 9 novembre 2026, clavier affiché, aucune
/// fenêtre de son app, même cachée. Son réglage, lui, suivait le clavier au
/// point près (`{{1584, 583}, {320, 188}}` sur un écran de 1 329 points de
/// haut : le clavier était de 558 à 746 sur la capture d'écran).
#[must_use]
pub fn keyboard_viewer_frame(panel_frame: &str, primary_height: f64) -> Option<OsRect> {
    let numbers: Vec<f64> = panel_frame
        .split(|c: char| !(c.is_ascii_digit() || matches!(c, '-' | '+' | '.')))
        .filter(|token| !token.is_empty())
        .map(str::parse)
        .collect::<Result<_, _>>()
        .ok()?;
    let &[x, y, w, h] = numbers.as_slice() else {
        return None;
    };
    let finite = numbers.iter().all(|v| v.is_finite()) && primary_height.is_finite();
    (finite && w > 0.0 && h > 0.0).then_some(OsRect {
        x,
        y: primary_height - (y + h),
        w,
        h,
    })
}

/// Ajoute le visualiseur de clavier lu dans ses réglages à la liste des
/// fenêtres (de la plus en avant à la plus en arrière), à sa place : devant
/// celles de niveau inférieur. Rien si la liste a déjà une fenêtre de son
/// app (avant macOS 27.2, le système l'annonçait lui-même, et sa fenêtre
/// fait foi).
pub fn add_keyboard_viewer(windows: &mut Vec<OsWindow>, viewer: OsWindow) {
    if windows.iter().any(OsWindow::is_accessibility_panel) {
        return;
    }
    let at = windows
        .iter()
        .position(|w| w.layer < viewer.layer)
        .unwrap_or(windows.len());
    windows.insert(at, viewer);
}

/// Un écran : cadre complet, zone utile (sans barre des menus ni Dock) et échelle.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct OsScreen {
    pub frame: OsRect,
    pub visible: OsRect,
    pub scale: f64,
}

/// Identifiant du processus courant.
#[must_use]
pub fn current_pid() -> i32 {
    std::process::id() as i32
}

/// Rend au système la mémoire libérée que l'allocateur garde encore pour
/// la réutiliser (sur Mac, des pages restées « sales » qui comptent dans
/// l'empreinte de l'app pendant des minutes). Renvoie les octets rendus.
/// À appeler hors du fil de l'affichage : le parcours prend quelques
/// millisecondes.
pub fn release_free_memory() -> usize {
    #[cfg(target_os = "macos")]
    {
        unsafe extern "C" {
            /// `<malloc/malloc.h>` (macOS 10.7) : `zone` nul = toutes les
            /// zones, `goal` nul = tout ce qui peut l'être.
            fn malloc_zone_pressure_relief(zone: *mut std::ffi::c_void, goal: usize) -> usize;
        }
        // SAFETY: fonction de la bibliothèque C du système, sûre depuis
        // n'importe quel fil ; un pointeur nul désigne toutes les zones.
        unsafe { malloc_zone_pressure_relief(std::ptr::null_mut(), 0) }
    }
    #[cfg(not(target_os = "macos"))]
    {
        0
    }
}

/// Fait passer le fil courant après tout le reste : un travail de fond (la
/// compression des images) qui ne doit jamais retenir l'affichage. Sur Mac,
/// la classe de service « utilitaire » : le système le place de préférence
/// sur les cœurs économes, et le ralentit quand la batterie est basse.
pub fn lower_thread_priority() {
    #[cfg(target_os = "macos")]
    {
        // SAFETY: fonction de libpthread sans pointeur, qui ne change que la
        // classe de service du fil qui l'appelle.
        let status =
            unsafe { libc::pthread_set_qos_class_self_np(libc::qos_class_t::QOS_CLASS_UTILITY, 0) };
        if status != 0 {
            log::debug!("classe de service du fil inchangée ({status})");
        }
    }
    #[cfg(target_os = "windows")]
    windows::set_thread_priority(false);
}

/// Fait passer le fil courant devant (classe « interactive ») : celui qui
/// fait les images ne doit jamais attendre un cœur derrière du travail de fond.
pub fn raise_thread_priority() {
    #[cfg(target_os = "macos")]
    {
        // SAFETY: fonction de libpthread sans pointeur, qui ne change que la
        // classe de service du fil qui l'appelle.
        let status = unsafe {
            libc::pthread_set_qos_class_self_np(libc::qos_class_t::QOS_CLASS_USER_INTERACTIVE, 0)
        };
        if status != 0 {
            log::debug!("classe de service du fil inchangée ({status})");
        }
    }
    #[cfg(target_os = "windows")]
    windows::set_thread_priority(true);
}

/// Fait `work` dans une réserve d'objets à libérer (« autorelease pool ») :
/// sur un autre fil que le fil principal, les objets du système rendus
/// ainsi pendant une image sont libérés à sa fin, au lieu de s'accumuler.
pub fn with_release_pool<R>(work: impl FnOnce() -> R) -> R {
    #[cfg(target_os = "macos")]
    {
        objc2::rc::autoreleasepool(|_| work())
    }
    #[cfg(not(target_os = "macos"))]
    {
        work()
    }
}

/// Une ligne des crédits de la fenêtre « À propos ».
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum AboutLine<'a> {
    Text(&'a str),
    /// Lien cliquable : texte affiché, adresse.
    Link(&'a str, &'a str),
}

/// Contenu de la fenêtre « À propos ».
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct About<'a> {
    pub name: &'a str,
    pub version: &'a str,
    /// Mention de copyright (« © 2026 … »).
    pub copyright: &'a str,
    /// Crédits, de haut en bas.
    pub lines: &'a [AboutLine<'a>],
}

/// Ce que l'utilisateur fait dans les fenêtres et les menus propres à
/// Mouflon (réglages, menu d'un animal). L'app les reçoit par
/// [`set_ui_handler`] ; les identifiants sont ceux qu'elle a donnés.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum UiEvent {
    /// Article choisi dans un menu surgissant ([`pop_up_menu`]).
    Menu(u32),
    /// Le menu surgissant s'est refermé (après l'article choisi, s'il y en a un).
    MenuClosed,
    /// Case des réglages cochée (`true`) ou décochée.
    Check(u32, bool),
    /// Cran choisi sur un curseur des réglages, ou choix dans une liste
    /// ([`FormRow::Choice`]) : son rang.
    Step(u32, usize),
    /// Bouton des réglages.
    Button(u32),
    /// La fenêtre des réglages passe au premier plan : ce qu'elle montre de
    /// l'état du système (autorisations…) est à relire, il a pu changer.
    FormShown,
    /// ⌘Q pendant que la fenêtre des réglages est au premier plan.
    Quit,
    /// « Ajouter » sur une carte de la galerie (son identifiant), et le
    /// rang de ce que sa liste de choix montrait (0 sans liste).
    GalleryAdd(u32, u8),
    /// Un raccourci clavier de Mouflon ([`set_shortcuts`]), dans n'importe
    /// quelle app (son identifiant).
    Shortcut(u32),
    /// La touche Échap, dans n'importe quelle app ([`listen_escape`]).
    Escape(KeyMove),
    /// Le programme vient d'être lancé une seconde fois (voir
    /// [`already_running`]) : celui-ci, le premier, est prévenu.
    Relaunched,
}

/// Ce que fait une touche écoutée.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum KeyMove {
    /// Enfoncée.
    Down,
    /// Toujours enfoncée : la répétition automatique du clavier.
    Repeat,
    /// Relâchée.
    Up,
}

/// Un raccourci clavier de Mouflon, valable dans toutes les apps : ⌃⌥⌘ et
/// une lettre. La lettre compte, pas la place de la touche : sur un clavier
/// AZERTY comme sur un QWERTY, ⌃⌥⌘Q est la touche marquée Q.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Shortcut {
    /// Rendu par [`UiEvent::Shortcut`].
    pub id: u32,
    /// La lettre, en minuscule (a à z).
    pub key: char,
}

/// Une carte de la galerie des animaux ([`show_gallery`]).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct GalleryCard {
    /// Rendu par [`UiEvent::GalleryAdd`].
    pub id: u32,
    pub name: String,
    /// Sa description et son auteur.
    pub detail: String,
    /// Le libellé du bouton (« Ajouter »).
    pub button: String,
    /// Ce qu'on peut choisir d'ajouter, dans une liste à côté du bouton
    /// (« Au hasard », « Un bélier »…) ; vide : pas de liste.
    pub choices: Vec<String>,
    /// Le nom de sa famille (« Moutons ») : les cartes d'une même famille se
    /// suivent, sous son nom quand il y a plusieurs familles.
    pub family: String,
}

/// Une image de l'aperçu animé d'une carte : pixels RGBA prémultipliés, et
/// combien de pixels par point.
#[derive(Clone, Debug, PartialEq)]
pub struct GalleryImage {
    pub width: u32,
    pub height: u32,
    pub rgba: Vec<u8>,
    pub scale: f64,
}

/// Un article de menu surgissant (le menu d'un animal, au clic droit).
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum PopupItem {
    /// Titre de section, qu'on ne choisit pas (le nom de l'animal).
    Header(String),
    /// Article à choisir, coché ou non.
    Item {
        id: u32,
        label: String,
        checked: bool,
        enabled: bool,
    },
    /// Sous-menu.
    Submenu {
        label: String,
        items: Vec<PopupItem>,
    },
    Separator,
}

/// La fenêtre des réglages : des sections de lignes, de haut en bas.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Form {
    pub title: String,
    pub sections: Vec<FormSection>,
}

/// Une section : un titre, puis ses lignes dans un cadre.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct FormSection {
    pub title: String,
    pub rows: Vec<FormRow>,
}

/// Une ligne des réglages.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum FormRow {
    /// Case à cocher, avec une explication dessous et, au besoin, un
    /// complément qui n'apparaît que quand l'app le demande.
    Check {
        id: u32,
        label: String,
        help: Option<String>,
        extra: Option<FormExtra>,
    },
    /// Curseur à crans : un libellé au-dessus, le nom de chaque cran
    /// dessous, puis une explication s'il y en a une.
    Steps {
        id: u32,
        label: String,
        steps: Vec<String>,
        help: Option<String>,
    },
    /// Un texte, et un bouton à sa droite.
    Text {
        text: String,
        button: Option<FormButton>,
    },
    /// Un choix dans une liste : le libellé à gauche, la liste à droite,
    /// une explication dessous. Montré et rendu par rang
    /// ([`FormValue::Step`], [`UiEvent::Step`]).
    Choice {
        id: u32,
        label: String,
        choices: Vec<String>,
        help: Option<String>,
    },
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct FormButton {
    pub id: u32,
    pub label: String,
}

/// Complément d'une case : une remarque et un bouton, montrés ou cachés
/// ensemble ([`FormValue::Shown`], avec l'identifiant du bouton).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct FormExtra {
    pub note: String,
    pub button: FormButton,
}

/// Ce que la fenêtre des réglages montre : l'app les donne à chaque
/// changement, d'où qu'il vienne.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum FormValue {
    Check(u32, bool),
    /// Le cran d'un curseur, ou le rang du choix d'une liste.
    Step(u32, usize),
    /// Montre ou cache un complément (identifiant de son bouton).
    Shown(u32, bool),
}

/// Mouflon s'ouvre-t-il à l'ouverture de session ? C'est le système qui le
/// sait : il le garde, et l'utilisateur peut le changer de son côté
/// (Réglages Système → Général → Ouverture).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum LoginItem {
    /// Pas réglable d'ici : macOS 12 ou plus ancien, ou Mouflon lancé hors
    /// de son app (`Mouflon.app`).
    Unavailable,
    Off,
    On,
    /// Demandé, mais en attente de l'accord de l'utilisateur dans Réglages
    /// Système.
    NeedsApproval,
}

impl LoginItem {
    /// La case « Ouvrir à l'ouverture de session » est-elle cochée ?
    #[must_use]
    pub fn is_requested(self) -> bool {
        matches!(self, Self::On | Self::NeedsApproval)
    }
}

mod alloc;
mod beats;
pub mod diag;
#[cfg(not(any(unix, target_os = "windows")))]
use generic as os;
#[cfg(target_os = "macos")]
mod macos;
#[cfg(target_os = "windows")]
mod windows;
#[cfg(all(unix, not(target_os = "macos")))]
mod x11;
#[cfg(all(unix, not(target_os = "macos")))]
use x11 as os;
mod mapped;
mod usage;

pub use alloc::{AllocCounts, CountingAlloc, alloc_counts, set_alloc_counting};
pub use beats::Beats;
pub use mapped::{SharedBytes, map_file};
pub use usage::{ProcessUsage, process_usage};

/// Un battement de l'horloge de l'écran, en secondes de l'horloge des
/// médias ([`diag::media_time`]).
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct Beat {
    /// Heure à laquelle l'image préparée maintenant sera affichée.
    pub target: f64,
    /// Heure du rafraîchissement qui vient d'avoir lieu (la synchronisation
    /// verticale) : `target - vsync` est la durée d'une image de l'écran.
    pub vsync: f64,
    /// Heure à laquelle l'horloge nous a appelés.
    pub called: f64,
}

/// Le compositeur du système qui montre les calques de la scène
/// ([`SceneLayer`]) : son nom, pour les journaux ; vide là où il n'y en a pas
/// (la scène passe alors par le GPU).
pub const LAYERS: &str = if cfg!(target_os = "macos") {
    "Core Animation"
} else if cfg!(target_os = "windows") {
    "DirectComposition"
} else {
    ""
};

/// Ce programme tourne-t-il déjà ? Sous Windows, rien n'empêche de le lancer
/// deux fois : le second lancement prévient le premier (qui reçoit
/// [`UiEvent::Relaunched`]) et n'a plus qu'à s'arrêter. Ailleurs, le système
/// s'en charge (ou chaque lancement vit sa vie) : toujours `false`.
#[must_use]
pub fn already_running() -> bool {
    #[cfg(target_os = "windows")]
    {
        windows::already_running()
    }
    #[cfg(not(target_os = "windows"))]
    {
        false
    }
}

/// Ouvre une page du web dans le navigateur de l'utilisateur.
pub fn open_page(url: &str) -> bool {
    #[cfg(target_os = "macos")]
    {
        macos::open_page(url)
    }
    #[cfg(not(target_os = "macos"))]
    {
        // (L'Explorateur de Windows et `xdg-open` ouvrent aussi les adresses.)
        open_folder(std::path::Path::new(url))
    }
}

/// Le dossier des outils du système, sous Windows (`System32`), tel que
/// Windows le donne. Ailleurs : `None`.
#[must_use]
pub fn system_dir() -> Option<std::path::PathBuf> {
    #[cfg(target_os = "windows")]
    {
        windows::system_dir()
    }
    #[cfg(not(target_os = "windows"))]
    {
        None
    }
}

/// Ce programme prend la suite du Mouflon qui vient de le lancer (une mise à
/// jour : l'ancienne version attend de voir démarrer la nouvelle) : sous
/// Windows, il devient « le » Mouflon de la session, sans s'arrêter devant
/// celui qui s'en va. Ailleurs, rien à faire.
pub fn succeed_running() {
    #[cfg(target_os = "windows")]
    windows::succeed_running();
}

/// Le compositeur des calques a perdu son appareil graphique (Windows :
/// pilote mis à jour, session à distance, carte qui change) : les calques
/// ne montrent plus rien tant que la scène n'est pas reposée dans chaque
/// fenêtre ([`attach_scene`]). Jamais ailleurs.
#[must_use]
pub fn scene_lost() -> bool {
    #[cfg(target_os = "windows")]
    {
        windows::scene_lost()
    }
    #[cfg(not(target_os = "windows"))]
    {
        false
    }
}

/// Le rang du compositeur des calques : quand il change (voir
/// [`scene_lost`]), les calques et les images faits avant ne servent plus.
#[must_use]
pub fn scene_generation() -> u64 {
    #[cfg(target_os = "windows")]
    {
        windows::scene_generation()
    }
    #[cfg(not(target_os = "windows"))]
    {
        0
    }
}

#[cfg(target_os = "macos")]
pub use macos::StatusItem;
#[cfg(target_os = "macos")]
pub use macos::{
    FrameClock, LoopWaker, SceneImage, SceneLayer, ThreadClock, accessibility_trusted,
    attach_scene, control_key_down, cursor_position, dock_frame, follow_window_moves,
    frontmost_pid, keep_pet_window, listen_escape, login_item, login_item_available,
    open_accessibility_settings, open_folder, open_login_items_settings, play_sound, pop_up_menu,
    preferred_languages, prepare_pet_window, render_scene, request_accessibility, run_loop_for,
    scene_transaction, screens, set_gallery_preview, set_login_item, set_shortcuts, set_ui_handler,
    set_window_moves_handler, show_about, show_form, show_gallery, snapshot_form, snapshot_gallery,
    update_form, user_activity, warm_up_sounds, window_max_fps, windows,
};

/// Ce que fait l'utilisateur, tous programmes confondus : quand, jamais quoi.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct UserActivity {
    /// Secondes depuis la dernière touche frappée (laquelle, on ne le sait pas).
    pub since_key: f64,
    /// Secondes depuis son dernier geste : clavier, souris, trackpad.
    pub idle: f64,
}

#[cfg(not(target_os = "macos"))]
mod fallback {
    //! Ce que ni Windows ni Linux n'ont (ou pas de la même façon que le
    //! Mac) : pas de Dock, pas d'autorisation d'Accessibilité, pas de signal
    //! des fenêtres qui bougent, et une attente toute simple à la place de la
    //! boucle d'événements du système.

    use std::sync::{Arc, Condvar, Mutex};

    /// Pas de signal des fenêtres qui bougent ici : le relevé suffit.
    pub fn set_window_moves_handler(_handler: impl Fn() + 'static) {}

    #[must_use]
    pub fn follow_window_moves(_pids: &[i32]) -> usize {
        0
    }

    /// Pas de fréquence d'écran connue ici (celle de winit sert).
    #[must_use]
    pub fn window_max_fps(_window: &impl raw_window_handle::HasWindowHandle) -> Option<f64> {
        None
    }

    /// Pas d'autorisation d'Accessibilité à demander ici.
    #[must_use]
    pub fn accessibility_trusted() -> bool {
        false
    }

    pub fn request_accessibility() -> bool {
        false
    }

    /// Pas de Dock.
    #[must_use]
    pub fn dock_frame() -> Option<super::OsRect> {
        None
    }

    /// Le clic droit suffit ici.
    #[must_use]
    pub fn control_key_down() -> bool {
        false
    }

    pub fn open_accessibility_settings() -> bool {
        false
    }

    /// De quoi réveiller un fil qui attend dans [`run_loop_for`].
    #[derive(Debug, Default)]
    struct Signal {
        woken: Mutex<bool>,
        changed: Condvar,
    }

    thread_local! {
        /// Le signal du fil courant.
        static SIGNAL: Arc<Signal> = Arc::new(Signal::default());
    }

    /// La boucle d'attente d'un fil, sans boucle d'événements du système :
    /// une simple attente, que le [`LoopWaker`] de ce fil interrompt.
    pub fn run_loop_for(timeout: std::time::Duration) {
        let signal = SIGNAL.with(Arc::clone);
        let Ok(woken) = signal.woken.lock() else {
            return;
        };
        if let Ok((mut woken, _)) = signal
            .changed
            .wait_timeout_while(woken, timeout, |woken| !*woken)
        {
            *woken = false;
        }
    }

    /// De quoi réveiller, depuis un autre fil, le fil qui l'a demandé (voir
    /// [`run_loop_for`]).
    #[derive(Debug)]
    pub struct LoopWaker {
        signal: Arc<Signal>,
    }

    impl LoopWaker {
        /// Pour réveiller le fil courant.
        #[must_use]
        pub fn current() -> Option<Self> {
            Some(Self {
                signal: SIGNAL.with(Arc::clone),
            })
        }

        pub fn wake(&self) {
            if let Ok(mut woken) = self.signal.woken.lock() {
                *woken = true;
                self.signal.changed.notify_all();
            }
        }
    }

    /// Pas d'horloge d'écran sur le fil principal ici : quand l'affichage
    /// y tourne, il se rythme avec une minuterie.
    #[derive(Debug)]
    pub enum FrameClock {}

    impl FrameClock {
        pub fn new(_tick: impl Fn(super::Beat) + 'static) -> Option<Self> {
            None
        }

        #[must_use]
        pub fn max_fps(&self) -> f32 {
            match *self {}
        }

        #[must_use]
        pub fn is_running(&self) -> bool {
            match *self {}
        }

        #[must_use]
        pub fn frame_duration(&self) -> f64 {
            match *self {}
        }

        pub fn set_running(&self, _running: bool) {
            match *self {}
        }

        pub fn set_rate(&self, _fps: f32) {
            match *self {}
        }
    }

    /// Pas d'image de la scène à relire ici.
    #[must_use]
    pub fn render_scene(
        _root: &super::SceneLayer,
        _width: u32,
        _height: u32,
        _scale: f64,
    ) -> Option<(u32, u32, Vec<u8>)> {
        None
    }

    #[cfg(test)]
    mod tests {
        use super::*;

        #[test]
        fn a_waker_wakes_its_own_thread_and_no_other() {
            use std::sync::mpsc::channel;
            use std::time::{Duration, Instant};

            let (ready, wakers) = channel();
            let (done, results) = channel();
            let spawn = |name: &'static str| {
                let (ready, done) = (ready.clone(), done.clone());
                std::thread::spawn(move || {
                    ready
                        .send((name, LoopWaker::current().expect("réveil")))
                        .unwrap();
                    let started = Instant::now();
                    run_loop_for(Duration::from_secs(5));
                    done.send((name, started.elapsed())).unwrap();
                })
            };
            let (first, second) = (spawn("premier"), spawn("second"));
            let mut found: Vec<(&str, LoopWaker)> =
                vec![wakers.recv().unwrap(), wakers.recv().unwrap()];
            found.sort_by_key(|(name, _)| *name);
            // Seul le premier est réveillé : lui seul finit tout de suite.
            std::thread::sleep(Duration::from_millis(50));
            found[0].1.wake();
            let (name, took) = results.recv().unwrap();
            assert_eq!(name, "premier");
            assert!(took < Duration::from_secs(4), "{took:?}");
            assert!(results.recv_timeout(Duration::from_millis(200)).is_err());
            found[1].1.wake();
            assert_eq!(results.recv().unwrap().0, "second");
            first.join().unwrap();
            second.join().unwrap();
            // Un réveil donné avant l'attente n'est pas perdu.
            let waker = LoopWaker::current().expect("réveil");
            waker.wake();
            let started = Instant::now();
            run_loop_for(Duration::from_secs(5));
            assert!(started.elapsed() < Duration::from_secs(4));
        }
    }
}

#[cfg(all(not(target_os = "macos"), not(target_os = "windows")))]
mod plain {
    //! Ce qui manque encore hors du Mac et de Windows : ni galerie, ni
    //! raccourcis dans toutes les apps, ni calques (la scène passe par le
    //! GPU), ni horloge de l'écran.

    /// Pas de galerie ici : les animaux s'ajoutent par le menu.
    #[must_use]
    pub fn show_gallery(_title: &str, _cards: &[super::GalleryCard]) -> bool {
        false
    }

    /// Pas encore de raccourcis dans toutes les apps ici : aucun n'est pris.
    #[must_use]
    pub fn set_shortcuts(shortcuts: &[super::Shortcut]) -> Vec<u32> {
        shortcuts.iter().map(|s| s.id).collect()
    }

    /// Pas d'écoute de la touche Échap ici.
    pub fn listen_escape(_on: bool) -> bool {
        false
    }

    pub fn set_gallery_preview(_id: u32, _frames: &[super::GalleryImage]) {}

    #[must_use]
    pub fn snapshot_gallery(_path: &std::path::Path, _dark: bool) -> bool {
        false
    }

    pub fn snapshot_form(_path: &std::path::Path, _dark: bool) -> bool {
        false
    }

    /// Les fenêtres d'animaux restent ici des fenêtres ordinaires : rien à garder.
    pub fn keep_pet_window(_window: &impl raw_window_handle::HasWindowHandle) {}

    /// Pas de calques ici : la scène passe par le GPU.
    #[derive(Debug, Clone)]
    pub struct SceneImage {
        bytes: usize,
    }

    impl SceneImage {
        #[must_use]
        pub fn from_bgra(width: u32, height: u32, bgra: &[u8]) -> Option<Self> {
            (bgra.len() == width as usize * height as usize * 4)
                .then_some(Self { bytes: bgra.len() })
        }

        #[must_use]
        pub fn byte_size(&self) -> usize {
            self.bytes
        }
    }

    #[derive(Debug, Clone, Default)]
    pub struct SceneLayer;

    impl SceneLayer {
        #[must_use]
        pub fn new() -> Self {
            Self
        }
        pub fn add(&self, _child: &Self) {}
        pub fn insert(&self, _child: &Self, _index: u32) {}
        pub fn remove(&self) {}
        pub fn set_position(&self, _x: f64, _y: f64) {}
        pub fn set_size(&self, _width: f64, _height: f64) {}
        pub fn set_transform(&self, _transform: [f64; 6]) {}
        pub fn set_opacity(&self, _opacity: f32) {}
        pub fn set_hidden(&self, _hidden: bool) {}
        pub fn set_image(&self, _image: Option<&SceneImage>) {}
        pub fn set_smooth(&self, _smooth: bool) {}
        pub fn set_group_opacity(&self, _group: bool) {}
    }

    pub fn scene_transaction<R>(change: impl FnOnce() -> R) -> R {
        change()
    }

    #[must_use]
    pub fn attach_scene(_window: &impl raw_window_handle::HasWindowHandle) -> Option<SceneLayer> {
        None
    }

    /// Pas d'horloge d'écran sur un autre fil ici.
    #[derive(Debug)]
    pub enum ThreadClock {}

    impl ThreadClock {
        pub fn new(
            _beats: std::sync::Arc<super::Beats>,
            _wake: std::sync::Arc<super::LoopWaker>,
        ) -> Option<Self> {
            None
        }

        pub fn start_here(&self) {
            match *self {}
        }

        #[must_use]
        pub fn max_fps(&self) -> f32 {
            match *self {}
        }

        #[must_use]
        pub fn is_running(&self) -> bool {
            match *self {}
        }

        #[must_use]
        pub fn frame_duration(&self) -> f64 {
            match *self {}
        }

        pub fn set_running(&self, _running: bool) {
            match *self {}
        }

        pub fn set_rate(&self, _fps: f32) {
            match *self {}
        }
    }

    /// Ouvre un dossier dans le gestionnaire de fichiers du système.
    pub fn open_folder(path: &std::path::Path) -> bool {
        std::process::Command::new("xdg-open")
            .arg(path)
            .spawn()
            .is_ok()
    }
}

#[cfg(not(any(unix, target_os = "windows")))]
mod generic {
    //! Ce qu'un système sans couche dédiée sait faire sans rien demander :
    //! aucun rebord de fenêtre, l'animal marche sur le bas de l'écran.
    use super::{
        About, AboutLine, Form, FormValue, LoginItem, OsScreen, OsWindow, PopupItem, UiEvent,
        plain::open_folder,
    };

    #[must_use]
    pub fn screens() -> Vec<OsScreen> {
        Vec::new()
    }

    #[must_use]
    pub fn windows() -> Vec<OsWindow> {
        Vec::new()
    }

    #[must_use]
    pub fn frontmost_pid() -> Option<i32> {
        None
    }

    #[must_use]
    pub fn cursor_position() -> Option<(f64, f64)> {
        None
    }

    #[must_use]
    pub fn user_activity() -> Option<super::UserActivity> {
        None
    }

    pub fn prepare_pet_window(_window: &impl raw_window_handle::HasWindowHandle) {}

    /// Langues de l'utilisateur, d'après les variables d'environnement POSIX
    /// (`LANGUAGE`, puis `LC_ALL`, `LC_MESSAGES` et `LANG`), en codes BCP 47.
    #[must_use]
    pub fn preferred_languages() -> Vec<String> {
        let mut tags: Vec<String> = std::env::var("LANGUAGE")
            .unwrap_or_default()
            .split(':')
            .filter(|tag| !tag.is_empty())
            .map(|tag| tag.replace('_', "-"))
            .collect();
        for var in ["LC_ALL", "LC_MESSAGES", "LANG"] {
            let value = std::env::var(var).unwrap_or_default();
            // « fr_FR.UTF-8 » ou « es_ES@euro » → « fr-FR », « es-ES ».
            let tag = value.split(['.', '@']).next().unwrap_or_default();
            if !tag.is_empty() && tag != "C" && tag != "POSIX" {
                tags.push(tag.replace('_', "-"));
            }
        }
        tags
    }

    /// Pas encore de fenêtre « À propos » ici : on ouvre le premier lien
    /// des crédits (la page du projet) dans le navigateur.
    pub fn show_about(about: &About<'_>) {
        let link = about.lines.iter().find_map(|line| match line {
            AboutLine::Link(_, url) => Some(*url),
            AboutLine::Text(_) => None,
        });
        if let Some(url) = link {
            open_folder(std::path::Path::new(url));
        }
    }

    /// Pas encore de fenêtres ni de menus surgissants à Mouflon ici : rien
    /// ne viendra par là.
    pub fn set_ui_handler(_handler: impl Fn(UiEvent) + 'static) {}

    pub fn pop_up_menu(_items: &[PopupItem]) {}

    /// Pas de fenêtre des réglages ici : `false`, à l'app de proposer
    /// autre chose (le fichier des réglages).
    pub fn show_form(_form: &Form, _values: &[FormValue]) -> bool {
        false
    }

    pub fn update_form(_values: &[FormValue]) {}

    #[must_use]
    pub fn login_item_available() -> bool {
        false
    }

    #[must_use]
    pub fn login_item() -> LoginItem {
        LoginItem::Unavailable
    }

    pub fn set_login_item(_on: bool) -> LoginItem {
        LoginItem::Unavailable
    }

    pub fn open_login_items_settings() {}

    /// Pas encore de sons sur ce système.
    pub fn play_sound(_data: &std::sync::Arc<[u8]>, _volume: f32) {}

    pub fn warm_up_sounds(_sample: Option<&std::sync::Arc<[u8]>>) {}

    /// Pas de fenêtre à qui rattacher celles des écrans.
    #[must_use]
    pub fn overlay_owner() -> Option<isize> {
        None
    }
}

#[cfg(not(target_os = "macos"))]
pub use fallback::{
    FrameClock, LoopWaker, accessibility_trusted, control_key_down, dock_frame,
    follow_window_moves, open_accessibility_settings, render_scene, request_accessibility,
    run_loop_for, set_window_moves_handler, window_max_fps,
};
#[cfg(all(not(target_os = "macos"), not(target_os = "windows")))]
pub use os::{
    cursor_position, frontmost_pid, login_item, login_item_available, open_login_items_settings,
    overlay_owner, play_sound, pop_up_menu, preferred_languages, prepare_pet_window, screens,
    set_login_item, set_ui_handler, show_about, show_form, update_form, user_activity,
    warm_up_sounds, windows,
};
#[cfg(all(not(target_os = "macos"), not(target_os = "windows")))]
pub use plain::{
    SceneImage, SceneLayer, ThreadClock, attach_scene, keep_pet_window, listen_escape, open_folder,
    scene_transaction, set_gallery_preview, set_shortcuts, show_gallery, snapshot_form,
    snapshot_gallery,
};
#[cfg(target_os = "windows")]
pub use windows::{
    SceneImage, SceneLayer, ThreadClock, attach_scene, check_pet_window, cursor_position,
    detach_scene as keep_pet_window, frontmost_pid, listen_escape, login_item,
    login_item_available, message, open_folder, open_login_items_settings, overlay_owner,
    place_pet_window, play_sound, pop_up_menu, preferred_languages, prepare_pet_window,
    scene_problem, scene_transaction, screens, set_gallery_preview, set_login_item,
    set_pet_window_hittest, set_shortcuts, set_ui_handler, show_about, show_form, show_gallery,
    snapshot_about, snapshot_form, snapshot_gallery, update_form, user_activity, warm_up_sounds,
    windows,
};

#[cfg(test)]
mod tests {
    use super::*;

    fn at(layer: i32) -> OsWindow {
        OsWindow {
            id: 1,
            pid: 2,
            frame: OsRect::default(),
            layer,
            owner: String::new(),
        }
    }

    #[test]
    fn the_keyboard_viewer_is_walkable_at_any_level() {
        let viewer = OsWindow {
            owner: "AssistiveControl".into(),
            ..at(101)
        };
        assert!(viewer.is_accessibility_panel() && viewer.is_walkable());
    }

    #[test]
    fn walks_on_apps_and_floating_panels_but_not_on_system_chrome() {
        assert!(at(0).is_walkable() && !at(0).is_floating());
        assert!(
            at(3).is_walkable() && at(3).is_floating(),
            "panneau flottant"
        );
        assert!(at(1500).is_walkable(), "visualiseur de clavier");
        assert!(!at(20).is_walkable(), "Dock");
        assert!(!at(24).is_walkable(), "barre des menus");
        assert!(!at(101).is_walkable(), "menu");
        assert!(!at(-20).is_walkable());
    }

    /// Le relevé du 9 novembre 2026 (macOS 27.2) : le clavier affiché de 558
    /// à 746 sur la capture d'écran, son réglage `{{1584, 583}, {320, 188}}`
    /// sur un écran de 1 329 points de haut.
    #[test]
    fn the_keyboard_viewer_frame_is_read_from_its_settings_in_the_common_frame() {
        let rect = |x, y, w, h| Some(OsRect { x, y, w, h });
        assert_eq!(
            keyboard_viewer_frame("{{1584, 583}, {320, 188}}", 1329.0),
            rect(1584.0, 558.0, 320.0, 188.0)
        );
        // Sur un écran à gauche de l'écran principal et plus bas que lui.
        assert_eq!(
            keyboard_viewer_frame("{{-1200.5, -40}, {320.5, 188}}", 1329.0),
            rect(-1200.5, 1181.0, 320.5, 188.0)
        );
        for nonsense in [
            "",
            "clavier",
            "{{1584, 583}, {320}}",
            "{{1584, 583}, {0, 188}}",
            "{{1, 2}, {3, 4}, {5, 6}}",
        ] {
            assert_eq!(keyboard_viewer_frame(nonsense, 1329.0), None, "{nonsense}");
        }
    }

    #[test]
    fn the_keyboard_viewer_goes_in_front_of_app_windows_unless_macos_lists_it() {
        let viewer = OsWindow {
            id: KEYBOARD_VIEWER_ID,
            owner: KEYBOARD_VIEWER_OWNER.into(),
            ..at(KEYBOARD_VIEWER_LEVEL)
        };
        assert!(viewer.is_accessibility_panel() && viewer.is_walkable());
        // macOS 27.2 : une fenêtre du système tout devant, la barre des
        // menus, le Dock, une app ; le clavier passe après la première.
        let mut windows = vec![at(2_147_483_630), at(24), at(20), at(0)];
        add_keyboard_viewer(&mut windows, viewer.clone());
        let layers: Vec<i32> = windows.iter().map(|w| w.layer).collect();
        assert_eq!(layers, [2_147_483_630, KEYBOARD_VIEWER_LEVEL, 24, 20, 0]);
        // Avant macOS 27.2, le système l'annonçait : sa fenêtre fait foi.
        let listed = OsWindow {
            owner: "AssistiveControl".into(),
            ..at(101)
        };
        let mut windows = vec![listed.clone(), at(0)];
        add_keyboard_viewer(&mut windows, viewer);
        assert_eq!(windows, [listed, at(0)]);
    }
}
