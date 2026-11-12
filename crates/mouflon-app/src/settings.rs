//! Réglages persistants et emplacements des fichiers.

use std::path::{Path, PathBuf};

use mouflon_core::Mood;
use serde::{Deserialize, Serialize};

/// Tailles proposées dans le menu (petite, normale, grande, très grande) :
/// facteur appliqué à la taille prévue par chaque animal.
pub const SIZES: [f32; 4] = [0.75, 1.0, 1.5, 2.0];

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct Settings {
    /// Nombre d'animaux au lancement.
    pub start_count: u32,
    /// Identifiant de l'animal préféré (`[pet] id` de son fichier) ; sans
    /// choix, une mascotte de Mouflon, sinon le premier animal livré dans
    /// l'app, sinon le Bleu du Maine (l'animal intégré). Un animal renommé
    /// depuis (voir [`RENAMED_PETS`]) reste le préféré.
    #[serde(deserialize_with = "pet_id::deserialize")]
    pub default_pet: Option<String>,
    /// Ne marcher que sur les fenêtres de l'app au premier plan : évite les
    /// « fenêtres fantômes » que macOS signale sur les autres Spaces.
    pub frontmost_only: bool,
    /// Mode collisions : deux animaux qui se rentrent dedans partent à la renverse.
    pub collisions: bool,
    /// Physique réaliste (voir `mouflon_core::physics`) : les fenêtres
    /// emportent les animaux et peuvent les faire tomber, lancés ils tournent
    /// et rebondissent. Sans elle, celle d'eSheep et de desktopPet.
    pub realistic_physics: bool,
    /// Taille des animaux, par rapport à celle prévue par chacun.
    pub size: f32,
    /// Marcher sous le Dock : les animaux descendent jusqu'en bas de l'écran,
    /// passent derrière le Dock et peuvent sauter dessus (il faut
    /// l'autorisation d'Accessibilité pour savoir où il est). Sans cela, ils
    /// marchent sur le Dock.
    #[serde(alias = "walk_on_dock")]
    pub walk_under_dock: bool,
    /// Les animaux font entendre leurs sons (bêlements, clochette…).
    pub sounds: bool,
    /// De temps en temps, un coup de vent ou une soucoupe volante (voir
    /// `mouflon_core::surprise`).
    pub surprises: bool,
    /// Le tempérament des animaux (voir [`MOODS`]).
    #[serde(with = "mood_name")]
    pub mood: Mood,
    /// Les raccourcis clavier de Mouflon, dans toutes les apps (voir
    /// `crate::keys`).
    pub shortcuts: bool,
    /// Échap deux fois de suite, ou tenue, fait quitter Mouflon (il faut
    /// l'autorisation d'Accessibilité pour l'entendre dans les autres apps).
    pub escape_quits: bool,
    /// Avec plusieurs écrans, les animaux vont sur tous (quatre au plus,
    /// voir `crate::screens::chosen`) ; sinon, seulement sur le principal.
    pub all_screens: bool,
}

/// Les animaux renommés d'une version à l'autre : (ancien identifiant,
/// nouveau). Des réglages qui nomment l'ancien désignent le nouveau.
const RENAMED_PETS: [(&str, &str); 4] = [
    ("mannequin", "bleu-du-maine"),
    ("aquarelle", "solognote"),
    ("pelote", "merinos-d-arles"),
    ("lainou", "southdown"),
];

/// L'identifiant d'aujourd'hui d'un animal, peut-être renommé depuis.
fn current_pet_id(id: &str) -> &str {
    RENAMED_PETS
        .iter()
        .find(|(old, _)| *old == id)
        .map_or(id, |(_, new)| new)
}

/// Un animal s'écrit par son identifiant dans le fichier des réglages ;
/// l'ancien identifiant d'un animal renommé donne le nouveau.
mod pet_id {
    use serde::{Deserialize, Deserializer};

    pub fn deserialize<'de, D: Deserializer<'de>>(
        deserializer: D,
    ) -> Result<Option<String>, D::Error> {
        let id = Option::<String>::deserialize(deserializer)?;
        Ok(id.map(|id| super::current_pet_id(&id).to_owned()))
    }
}

/// Les tempéraments proposés, dans l'ordre de la fenêtre des réglages : au
/// hasard (chacun le sien), calmes, actifs, énervés.
pub const MOODS: [Mood; 4] = [Mood::Mixed, Mood::Calm, Mood::Lively, Mood::Rowdy];

/// Le tempérament s'écrit par son nom dans le fichier des réglages ; un nom
/// inconnu (ou autre chose qu'un nom) donne celui par défaut, sans perdre
/// les autres réglages.
mod mood_name {
    use mouflon_core::Mood;
    use serde::{Deserialize, Deserializer, Serializer};

    const NAMES: [(Mood, &str); 5] = [
        (Mood::Mixed, "mixed"),
        (Mood::Calm, "calm"),
        (Mood::Lively, "lively"),
        (Mood::Rowdy, "rowdy"),
        (Mood::Even, "even"),
    ];

    pub fn serialize<S: Serializer>(mood: &Mood, serializer: S) -> Result<S::Ok, S::Error> {
        let name = NAMES
            .iter()
            .find(|(m, _)| m == mood)
            .map_or("mixed", |(_, name)| name);
        serializer.serialize_str(name)
    }

    pub fn deserialize<'de, D: Deserializer<'de>>(deserializer: D) -> Result<Mood, D::Error> {
        let value = toml::Value::deserialize(deserializer)?;
        Ok(value
            .as_str()
            .and_then(|name| {
                NAMES
                    .iter()
                    .find(|(_, n)| n.eq_ignore_ascii_case(name.trim()))
            })
            .map_or(super::MOODS[0], |(mood, _)| *mood))
    }
}

impl Default for Settings {
    fn default() -> Self {
        Self {
            start_count: 1,
            default_pet: None,
            frontmost_only: true,
            collisions: false,
            realistic_physics: true,
            size: 1.0,
            walk_under_dock: false,
            sounds: true,
            surprises: true,
            mood: MOODS[0],
            shortcuts: true,
            escape_quits: false,
            all_screens: true,
        }
    }
}

/// La taille qui va à l'écran principal, au premier lancement : grande sur
/// un écran fin et vaste (Retina, au moins 1 600 points de large, où les
/// animaux paraîtraient petits), normale ailleurs, où tout est déjà plus
/// gros. `screens` : le premier est l'écran principal.
#[must_use]
pub fn default_size(screens: &[mouflon_platform::OsScreen]) -> f32 {
    match screens.first() {
        Some(main) if main.scale >= 1.5 && main.frame.w >= 1600.0 => SIZES[2],
        _ => SIZES[1],
    }
}

/// Rang dans [`SIZES`] de la taille la plus proche de `size`.
#[must_use]
pub fn size_index_of(size: f32) -> usize {
    SIZES
        .iter()
        .enumerate()
        .min_by(|(_, a), (_, b)| (*a - size).abs().total_cmp(&(*b - size).abs()))
        .map_or(1, |(i, _)| i)
}

impl Settings {
    /// Rang de la taille dans [`SIZES`] (la plus proche, pour une valeur écrite à la main).
    #[must_use]
    pub fn size_index(&self) -> usize {
        size_index_of(self.size)
    }

    /// Taille à appliquer : une valeur de [`SIZES`].
    #[must_use]
    pub fn size_factor(&self) -> f32 {
        SIZES[self.size_index()]
    }

    /// Rang du tempérament dans [`MOODS`] (« tels que faits », qu'on ne
    /// choisit qu'à la main dans le fichier, se montre comme au hasard).
    #[must_use]
    pub fn mood_index(&self) -> usize {
        MOODS.iter().position(|m| *m == self.mood).unwrap_or(0)
    }

    #[must_use]
    pub fn load(path: &Path) -> Self {
        match std::fs::read_to_string(path) {
            Ok(text) => toml::from_str(&text).unwrap_or_else(|e| {
                log::warn!(
                    "réglages illisibles ({}), valeurs par défaut : {e}",
                    path.display()
                );
                Self::default()
            }),
            Err(_) => Self::default(),
        }
    }

    #[cfg_attr(not(any(target_os = "macos", target_os = "windows")), allow(dead_code))]
    pub fn save(&self, path: &Path) {
        if let Some(dir) = path.parent() {
            let _ = std::fs::create_dir_all(dir);
        }
        match toml::to_string_pretty(self) {
            Ok(text) => {
                if let Err(e) = std::fs::write(path, text) {
                    log::warn!("impossible d'enregistrer les réglages : {e}");
                }
            }
            Err(e) => log::warn!("réglages non sérialisables : {e}"),
        }
    }
}

/// Dossiers de l'application (sur macOS : `~/Library/Application Support/…`).
#[derive(Clone, Debug)]
pub struct Paths {
    pub settings: PathBuf,
    pub pets: PathBuf,
    /// Images des animaux déjà prêtes (sur macOS : `~/Library/Caches/…`),
    /// que le système peut effacer : elles seraient simplement refaites.
    pub images: PathBuf,
}

impl Paths {
    #[must_use]
    pub fn new() -> Option<Self> {
        let paths = Self::of(&directories::ProjectDirs::from(
            "com",
            "alejandrosanmartinlamas",
            "Mouflon",
        )?);
        // Mouflon s'appelait Bergerie : on reprend ses animaux et ses réglages.
        if let Some(old) =
            directories::ProjectDirs::from("com", "alejandrosanmartinlamas", "Bergerie")
        {
            paths.migrate_from(&Self::of(&old));
        }
        Some(paths)
    }

    /// Le fichier où le programme écrit son journal quand il n'a pas de
    /// console (Windows), à côté des images gardées.
    #[must_use]
    pub fn log_file() -> Option<PathBuf> {
        let dirs = directories::ProjectDirs::from("com", "alejandrosanmartinlamas", "Mouflon")?;
        Some(dirs.cache_dir().join("journal.txt"))
    }

    fn of(dirs: &directories::ProjectDirs) -> Self {
        Self {
            settings: dirs.config_dir().join("settings.toml"),
            pets: dirs.data_dir().join("pets"),
            images: dirs.cache_dir().join("images"),
        }
    }

    /// Reprend les animaux installés et les réglages d'un ancien emplacement,
    /// une seule fois (rien n'est fait si le nouvel emplacement existe déjà).
    /// L'animal préféré n'est pas repris : la mascotte de Mouflon accueille
    /// au premier lancement.
    fn migrate_from(&self, old: &Self) {
        if old.pets.is_dir() && !self.pets.exists() {
            match crate::library::copy_dir(&old.pets, &self.pets) {
                Ok(()) => log::info!("animaux repris de {}", old.pets.display()),
                Err(e) => log::warn!("impossible de reprendre {} : {e}", old.pets.display()),
            }
        }
        if old.settings.is_file() && !self.settings.exists() {
            let settings = Settings {
                default_pet: None,
                ..Settings::load(&old.settings)
            };
            settings.save(&self.settings);
            log::info!("réglages repris de {}", old.settings.display());
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_size_snaps_to_a_menu_step() {
        let with = |size: f32| Settings {
            size,
            ..Settings::default()
        };
        assert_eq!(Settings::default().size_index(), 1);
        assert_eq!(with(2.0).size_factor(), 2.0);
        // Une valeur écrite à la main prend la taille du menu la plus proche.
        assert_eq!(with(1.4).size_factor(), 1.5);
        assert_eq!(with(0.1).size_index(), 0);
        // Les anciens réglages, sans taille, gardent la taille normale.
        let old: Settings = toml::from_str("collisions = true").unwrap();
        assert_eq!(old.size_factor(), 1.0);
    }

    #[test]
    fn the_mood_is_kept_by_name_and_a_bad_one_spoils_nothing_else() {
        let calm = Settings {
            mood: Mood::Rowdy,
            ..Settings::default()
        };
        let text = toml::to_string_pretty(&calm).unwrap();
        assert!(text.contains("mood = \"rowdy\""), "{text}");
        assert_eq!(toml::from_str::<Settings>(&text).unwrap(), calm);
        // Les anciens réglages, sans tempérament : chacun le sien.
        let old: Settings = toml::from_str("collisions = true").unwrap();
        assert_eq!(old.mood, Mood::Mixed);
        assert!(old.shortcuts && !old.escape_quits);
        // Un nom inconnu, ou un nombre : le tempérament par défaut, et le
        // reste est gardé.
        for line in ["mood = \"furieux\"", "mood = 3"] {
            let odd: Settings = toml::from_str(&format!("{line}\ncollisions = true")).unwrap();
            assert_eq!(odd.mood, Mood::Mixed);
            assert!(odd.collisions);
        }
        let even: Settings = toml::from_str("mood = \"Even\"").unwrap();
        assert_eq!((even.mood, even.mood_index()), (Mood::Even, 0));
    }

    #[test]
    fn the_first_size_suits_the_main_screen() {
        let screen = |w: f64, h: f64, scale: f64| mouflon_platform::OsScreen {
            frame: mouflon_platform::OsRect {
                x: 0.0,
                y: 0.0,
                w,
                h,
            },
            visible: mouflon_platform::OsRect {
                x: 0.0,
                y: 25.0,
                w,
                h: h - 25.0,
            },
            scale,
        };
        // Un MacBook Pro de 16 pouces réglé sur « plus d'espace » : grande.
        assert_eq!(default_size(&[screen(2056.0, 1329.0, 2.0)]), 1.5);
        assert_eq!(default_size(&[screen(1728.0, 1117.0, 2.0)]), 1.5);
        // Un petit portable, ou un écran sans Retina : tout y est plus gros.
        assert_eq!(default_size(&[screen(1470.0, 956.0, 2.0)]), 1.0);
        assert_eq!(default_size(&[screen(1920.0, 1080.0, 1.0)]), 1.0);
        // L'écran principal compte, pas les autres.
        assert_eq!(
            default_size(&[screen(1920.0, 1080.0, 1.0), screen(2056.0, 1329.0, 2.0)]),
            1.0
        );
        assert_eq!(default_size(&[]), 1.0);
    }

    #[test]
    fn a_favourite_pet_saved_under_its_old_name_is_its_renamed_breed() {
        for (old, new) in [
            ("mannequin", "bleu-du-maine"),
            ("aquarelle", "solognote"),
            ("pelote", "merinos-d-arles"),
            ("lainou", "southdown"),
        ] {
            let settings: Settings =
                toml::from_str(&format!("default_pet = \"{old}\"\ncollisions = true")).unwrap();
            assert_eq!(settings.default_pet.as_deref(), Some(new));
            assert!(settings.collisions, "les autres réglages sont gardés");
            // Enregistrés de nouveau : sous le nouveau nom.
            let saved = toml::to_string(&settings).unwrap();
            assert!(
                saved.contains(&format!("default_pet = \"{new}\"")),
                "{saved}"
            );
        }
        // Les autres animaux gardent leur nom, et sans préféré, rien ne change.
        let poete: Settings = toml::from_str("default_pet = \"poete\"").unwrap();
        assert_eq!(poete.default_pet.as_deref(), Some("poete"));
        let none: Settings = toml::from_str("collisions = true").unwrap();
        assert_eq!(none.default_pet, None);
    }

    #[test]
    fn keeps_the_dock_choice_made_under_its_old_name() {
        let old: Settings = toml::from_str("walk_on_dock = true").unwrap();
        assert!(old.walk_under_dock);
        let saved = toml::to_string(&old).unwrap();
        assert!(saved.contains("walk_under_dock = true"), "{saved}");
    }

    #[test]
    fn takes_over_the_old_pets_and_settings_once() {
        let root = std::env::temp_dir().join(format!("mouflon-migration-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&root);
        let old = Paths {
            settings: root.join("ancien/settings.toml"),
            pets: root.join("ancien/pets"),
            images: root.join("ancien/images"),
        };
        let new = Paths {
            settings: root.join("nouveau/settings.toml"),
            pets: root.join("nouveau/pets"),
            images: root.join("nouveau/images"),
        };
        std::fs::create_dir_all(old.pets.join("esheep")).unwrap();
        std::fs::write(old.pets.join("esheep/animations.xml"), "<animations/>").unwrap();
        Settings {
            start_count: 3,
            default_pet: Some("esheep".into()),
            frontmost_only: false,
            collisions: true,
            realistic_physics: false,
            size: 1.0,
            walk_under_dock: false,
            sounds: true,
            surprises: false,
            mood: Mood::Calm,
            shortcuts: false,
            escape_quits: true,
            all_screens: false,
        }
        .save(&old.settings);

        new.migrate_from(&old);
        assert!(new.pets.join("esheep/animations.xml").is_file());
        let settings = Settings::load(&new.settings);
        assert_eq!(settings.start_count, 3);
        assert!(settings.collisions && !settings.frontmost_only);
        assert_eq!(
            settings.default_pet, None,
            "la mascotte de Mouflon accueille"
        );

        // Une seule fois : un réglage changé depuis n'est pas écrasé.
        Settings::default().save(&new.settings);
        new.migrate_from(&old);
        assert_eq!(Settings::load(&new.settings), Settings::default());
        let _ = std::fs::remove_dir_all(&root);
    }
}
