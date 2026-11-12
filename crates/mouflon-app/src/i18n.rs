//! Textes de l'interface, dans la langue choisie dans macOS (Réglages Système →
//! Général → Langue et région, langue propre à Mouflon comprise). Mouflon parle
//! anglais, français, espagnol, allemand, italien, néerlandais, portugais (du
//! Brésil et du Portugal), russe, japonais, coréen et chinois (simplifié et
//! traditionnel) ; pour une autre langue, il parle anglais.
//!
//! Ajouter une langue : copier `locales/en.toml` sous le code de la langue
//! (ISO 639-1 : `pl.toml`, `sv.toml`…), le traduire, l'ajouter à [`LOCALES`] et
//! à `CFBundleLocalizations` dans `packaging/macos/Info.plist.in`.

use std::sync::OnceLock;

use serde::Deserialize;

/// Langues connues : code (comme les dossiers `.lproj` de macOS) et textes.
/// La première sert de repli.
pub const LOCALES: &[(&str, &str)] = &[
    ("en", include_str!("../locales/en.toml")),
    ("fr", include_str!("../locales/fr.toml")),
    ("es", include_str!("../locales/es.toml")),
    ("de", include_str!("../locales/de.toml")),
    ("it", include_str!("../locales/it.toml")),
    ("nl", include_str!("../locales/nl.toml")),
    ("pt", include_str!("../locales/pt.toml")),
    ("pt-PT", include_str!("../locales/pt-PT.toml")),
    ("ru", include_str!("../locales/ru.toml")),
    ("ja", include_str!("../locales/ja.toml")),
    ("ko", include_str!("../locales/ko.toml")),
    ("zh-Hans", include_str!("../locales/zh-Hans.toml")),
    ("zh-Hant", include_str!("../locales/zh-Hant.toml")),
];

/// Pays où l'on écrit le portugais du Portugal (l'autre variante, celle du
/// Brésil, sert aussi quand la région n'est pas dite).
const EUROPEAN_PORTUGUESE: &[&str] = &["pt", "ao", "mz", "cv", "gw", "st", "tl", "mo"];
/// Régions où l'on écrit le chinois traditionnel, quand l'écriture n'est pas dite.
const TRADITIONAL_CHINESE: &[&str] = &["tw", "hk", "mo"];

/// Code de [`LOCALES`] qui convient à une étiquette BCP 47 (« fr-FR »,
/// « zh-Hant-TW », « pt_BR »…), s'il y en a un.
fn locale_of(tag: &str) -> Option<&'static str> {
    let tag = tag.replace('_', "-").to_ascii_lowercase();
    let mut parts = tag.split('-');
    let primary = parts.next()?;
    let rest: Vec<&str> = parts.collect();
    // Écriture : quatre lettres (« hant ») ; région : deux lettres ou trois chiffres.
    let script = rest.iter().find(|p| p.len() == 4);
    let region = rest
        .iter()
        .find(|p| p.len() == 2 || (p.len() == 3 && p.chars().all(|c| c.is_ascii_digit())));
    let code = match primary {
        "zh" => match script {
            Some(&"hant") => "zh-Hant",
            Some(_) => "zh-Hans",
            None if region.is_some_and(|r| TRADITIONAL_CHINESE.contains(r)) => "zh-Hant",
            None => "zh-Hans",
        },
        "pt" if region.is_some_and(|r| EUROPEAN_PORTUGUESE.contains(r)) => "pt-PT",
        other => other,
    };
    LOCALES
        .iter()
        .map(|(c, _)| *c)
        .find(|c| c.eq_ignore_ascii_case(code))
}

/// Les textes d'une langue : chaque fichier doit les donner tous, sans autre clé.
#[derive(Clone, Debug, Deserialize)]
#[serde(deny_unknown_fields)]
#[cfg_attr(not(any(target_os = "macos", target_os = "windows")), allow(dead_code))]
pub struct Texts {
    // Barre des menus.
    /// « Ajouter Southdown » : `{name}` est remplacé par le nom de l'animal
    /// (le préféré dans la barre des menus, celui du clic droit sinon).
    pub add_named: String,
    /// Sous-menu des autres espèces (« Ajouter un autre animal »).
    pub add_pet: String,
    /// Dans le menu d’une brebis adulte.
    pub add_lamb: String,
    /// Son premier article, qui ouvre la galerie des animaux.
    pub gallery: String,
    /// Sous-menu « Dire au revoir », et son premier article (« À tous »).
    pub goodbye: String,
    pub goodbye_all: String,
    pub together: String,
    /// « Mettre en pause », et « Reprendre » pendant la pause.
    pub pause: String,
    pub resume: String,
    pub sounds: String,
    /// « Réglages… » : ouvre la fenêtre des réglages.
    pub settings: String,
    pub about: String,
    pub quit: String,
    /// « Ajouter dix animaux au hasard ».
    pub add_random: String,
    /// « Un de chaque » : un animal de chaque espèce d'une famille.
    pub one_of_each: String,
    /// Les familles d'animaux (voir [`mouflon_core::CATEGORIES`]).
    pub category_sheep: String,
    pub category_farm: String,
    pub category_cats: String,
    pub category_dogs: String,
    pub category_birds: String,
    pub category_wild: String,
    pub category_fantasy: String,
    pub category_others: String,
    /// Les régions d'origine des races (voir [`mouflon_core::REGIONS`]) ;
    /// « Ailleurs » : celles qui ne disent pas la leur.
    pub region_france: String,
    pub region_british_isles: String,
    pub region_europe: String,
    pub region_asia_africa: String,
    pub region_americas: String,
    pub region_oceania: String,
    pub region_other: String,
    // Menu d'un animal (clic droit).
    /// « Dire au revoir à Southdown » : `{name}` est le nom de l'animal.
    pub goodbye_named: String,
    /// « Taille de Southdown » : la taille de cet animal seulement.
    pub size_named: String,
    pub size: String,
    pub size_small: String,
    pub size_normal: String,
    pub size_large: String,
    pub size_huge: String,
    // Fenêtre des réglages.
    pub settings_title: String,
    pub pets_section: String,
    /// Sous le curseur de taille : il règle celle de tous les animaux.
    pub size_help: String,
    pub pet_sounds: String,
    /// « Surprises », et ce qu'elles sont.
    pub surprises: String,
    pub surprises_help: String,
    pub walks_section: String,
    pub collisions: String,
    pub collisions_help: String,
    pub realistic_physics: String,
    pub realistic_physics_help: String,
    pub frontmost_only: String,
    pub frontmost_only_help: String,
    /// Avec plusieurs écrans : sur tous, ou sur le principal seulement.
    pub all_screens: String,
    pub all_screens_help: String,
    pub walk_under_dock: String,
    pub walk_under_dock_help: String,
    /// Sous « Marcher sous le Dock » tant que l'autorisation manque.
    pub accessibility_waiting: String,
    pub startup_section: String,
    pub open_at_login: String,
    /// Sous « Ouvrir Mouflon à l'ouverture de session » tant que
    /// l'utilisateur n'a pas donné son accord.
    pub login_waiting: String,
    pub open_system_settings: String,
    pub my_pets_section: String,
    pub my_pets_help: String,
    pub open_pets_folder: String,
    /// L'humeur des animaux : au hasard, calmes, actifs, énervés.
    pub mood: String,
    pub mood_random: String,
    pub mood_calm: String,
    pub mood_active: String,
    pub mood_angry: String,
    pub mood_help: String,
    /// Les raccourcis clavier, et Échap pour quitter.
    pub shortcuts_section: String,
    pub shortcuts: String,
    pub shortcuts_help: String,
    pub escape_quits: String,
    pub escape_quits_help: String,
    // Sous Windows seulement : là, rien à autoriser pour entendre Échap, et
    // les programmes de démarrage se règlent dans les Paramètres.
    pub escape_quits_help_windows: String,
    /// Sous « Ouvrir Mouflon à l'ouverture de session » quand l'utilisateur
    /// l'a coupé dans les applications de démarrage de Windows.
    pub login_waiting_windows: String,
    pub open_system_settings_windows: String,
    /// Le programme doit s'arrêter (suivi du chemin de son journal).
    #[cfg_attr(not(target_os = "windows"), allow(dead_code))]
    pub crashed: String,
    /// Rien ne peut être montré sur cette machine.
    #[cfg_attr(not(target_os = "windows"), allow(dead_code))]
    pub cannot_show: String,
    // Galerie des animaux : son titre, le bouton d'une carte, l'auteur
    // (`{author}`).
    pub gallery_title: String,
    pub gallery_add: String,
    pub gallery_by: String,
    /// Ce que la galerie ajoute d'une race : au hasard (comme le menu), ou
    /// précisément un bélier, une brebis, un agneau.
    pub gallery_any: String,
    pub gallery_ram: String,
    pub gallery_ewe: String,
    pub gallery_lamb: String,
    // Fenêtre « À propos ».
    pub description: String,
    pub credits: String,
    pub license: String,
}

impl Texts {
    /// Tous les textes, pour les vérifier ensemble.
    #[cfg(test)]
    fn all(&self) -> Vec<&str> {
        [
            &self.add_named,
            &self.add_pet,
            &self.gallery,
            &self.goodbye,
            &self.goodbye_all,
            &self.together,
            &self.pause,
            &self.resume,
            &self.sounds,
            &self.settings,
            &self.about,
            &self.quit,
            &self.goodbye_named,
            &self.size_named,
            &self.size,
            &self.size_small,
            &self.size_normal,
            &self.size_large,
            &self.size_huge,
            &self.settings_title,
            &self.pets_section,
            &self.size_help,
            &self.pet_sounds,
            &self.surprises,
            &self.surprises_help,
            &self.walks_section,
            &self.collisions,
            &self.collisions_help,
            &self.realistic_physics,
            &self.realistic_physics_help,
            &self.frontmost_only,
            &self.frontmost_only_help,
            &self.all_screens,
            &self.all_screens_help,
            &self.walk_under_dock,
            &self.walk_under_dock_help,
            &self.accessibility_waiting,
            &self.startup_section,
            &self.open_at_login,
            &self.login_waiting,
            &self.open_system_settings,
            &self.my_pets_section,
            &self.my_pets_help,
            &self.open_pets_folder,
            &self.escape_quits_help_windows,
            &self.login_waiting_windows,
            &self.open_system_settings_windows,
            &self.crashed,
            &self.cannot_show,
            &self.gallery_title,
            &self.gallery_add,
            &self.gallery_by,
            &self.gallery_any,
            &self.gallery_ram,
            &self.gallery_ewe,
            &self.gallery_lamb,
            &self.description,
            &self.credits,
            &self.license,
        ]
        .into_iter()
        .map(String::as_str)
        .collect()
    }
}

/// Code de la première langue préférée que Mouflon parle, l'anglais à défaut.
/// `preferred` : langues de l'utilisateur dans son ordre (« fr-FR », « es-419 »…).
#[must_use]
pub fn language(preferred: &[String]) -> &'static str {
    preferred
        .iter()
        .find_map(|tag| locale_of(tag))
        .unwrap_or(LOCALES[0].0)
}

fn parse(code: &str) -> Texts {
    let (_, source) = LOCALES
        .iter()
        .find(|(c, _)| *c == code)
        .unwrap_or(&LOCALES[0]);
    toml::from_str(source).unwrap_or_else(|e| {
        log::error!("locales/{code}.toml illisible, textes anglais : {e}");
        toml::from_str(LOCALES[0].1).expect("locales/en.toml")
    })
}

/// Langue de l'interface et ses textes, choisis au premier appel.
fn chosen() -> &'static (&'static str, Texts) {
    static CHOSEN: OnceLock<(&'static str, Texts)> = OnceLock::new();
    CHOSEN.get_or_init(|| {
        // `MOUFLON_LANGUE=de` : une autre langue que celle de macOS, pour
        // voir la fenêtre des réglages dans chacune (vitrine de la CI).
        let mut preferred = mouflon_platform::preferred_languages();
        if let Ok(tag) = std::env::var("MOUFLON_LANGUE") {
            preferred.insert(0, tag);
        }
        let code = language(&preferred);
        log::info!("langue de l'interface : {code}");
        (code, parse(code))
    })
}

/// Textes de l'interface dans la langue de l'utilisateur.
#[cfg_attr(not(any(target_os = "macos", target_os = "windows")), allow(dead_code))]
pub fn texts() -> &'static Texts {
    &chosen().1
}

/// Code de la langue de l'interface (« fr », « zh-Hant »…).
pub fn code() -> &'static str {
    chosen().0
}

/// Les touches qui vont avec la lettre d'un raccourci de Mouflon sur Mac,
/// telles que les textes les écrivent.
pub const MAC_KEYS: &str = "⌃⌥⌘";

/// Les mêmes touches sous Windows, telles qu'on les lit sur un clavier de
/// la langue `code` (« Strg » sur un clavier allemand).
#[must_use]
pub fn windows_keys(code: &str) -> &'static str {
    if code == "de" {
        "Strg+Alt+Win+"
    } else {
        "Ctrl+Alt+Win+"
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn tags(list: &[&str]) -> Vec<String> {
        list.iter().map(|s| (*s).to_owned()).collect()
    }

    #[test]
    fn every_language_gives_every_text() {
        for (code, source) in LOCALES {
            let texts: Texts =
                toml::from_str(source).unwrap_or_else(|e| panic!("locales/{code}.toml : {e}"));
            for text in texts.all() {
                assert!(!text.trim().is_empty(), "{code} : texte vide");
            }
            assert!(texts.about.contains("Mouflon") && texts.quit.contains("Mouflon"));
            assert!(texts.gallery_by.contains("{author}"), "{code}");
            assert!(
                texts.gallery.ends_with('…') || texts.gallery.ends_with('⋯'),
                "{code} : la galerie s'ouvre dans une fenêtre"
            );
            for named in [&texts.add_named, &texts.goodbye_named, &texts.size_named] {
                assert!(
                    named.contains("{name}"),
                    "{code} : « {named} » sans {{name}}"
                );
            }
            // « Réglages… » ouvre une fenêtre : des points de suspension,
            // comme partout sur Mac.
            assert!(
                texts.settings.ends_with('…') || texts.settings.ends_with('⋯'),
                "{code} : « {} » sans points de suspension",
                texts.settings
            );
        }
    }

    #[test]
    fn follows_the_first_language_it_speaks() {
        assert_eq!(language(&tags(&["fr-FR", "en-US"])), "fr");
        assert_eq!(language(&tags(&["sv-SE", "es-419", "fr-FR"])), "es");
        assert_eq!(language(&tags(&["EN-gb"])), "en");
        assert_eq!(language(&tags(&["fr_CA"])), "fr");
        assert_eq!(language(&tags(&["de-CH"])), "de");
        assert_eq!(language(&tags(&["ja-JP"])), "ja");
        // Une langue qu'il ne parle pas encore, ou aucune : l'anglais.
        assert_eq!(language(&tags(&["sv-SE", "fi-FI"])), "en");
        assert_eq!(language(&[]), "en");
    }

    #[test]
    fn tells_the_two_portugueses_apart() {
        assert_eq!(language(&tags(&["pt-BR"])), "pt");
        assert_eq!(language(&tags(&["pt"])), "pt");
        assert_eq!(language(&tags(&["pt-PT"])), "pt-PT");
        assert_eq!(language(&tags(&["pt_AO"])), "pt-PT");
    }

    #[test]
    fn tells_simplified_and_traditional_chinese_apart() {
        assert_eq!(language(&tags(&["zh-Hans-CN"])), "zh-Hans");
        assert_eq!(language(&tags(&["zh-Hant-TW"])), "zh-Hant");
        assert_eq!(language(&tags(&["zh-Hant-HK"])), "zh-Hant");
        // Écriture traditionnelle à Hong Kong si elle n'est pas dite, simplifiée sinon.
        assert_eq!(language(&tags(&["zh-HK"])), "zh-Hant");
        assert_eq!(language(&tags(&["zh-TW"])), "zh-Hant");
        assert_eq!(language(&tags(&["zh-CN"])), "zh-Hans");
        assert_eq!(language(&tags(&["zh"])), "zh-Hans");
        assert_eq!(language(&tags(&["zh-Hans-HK"])), "zh-Hans");
    }

    #[test]
    fn speaks_every_language_it_lists() {
        assert_eq!(parse("fr").quit, "Quitter Mouflon");
        assert_eq!(parse("es").quit, "Salir de Mouflon");
        assert_eq!(parse("de").quit, "Mouflon beenden");
        assert_eq!(parse("ja").quit, "Mouflonを終了");
        assert_eq!(parse("zh-Hant").quit, "結束 Mouflon");
        assert_eq!(parse("xx").quit, "Quit Mouflon");
        // Chaque code se retrouve tel quel depuis une étiquette du système.
        for (code, _) in LOCALES {
            assert_eq!(language(&tags(&[code])), *code);
        }
    }

    #[test]
    fn the_app_bundle_lists_every_language() {
        let plist = include_str!("../../../packaging/macos/Info.plist.in");
        for (code, _) in LOCALES {
            assert!(
                plist.contains(&format!("<string>{code}</string>")),
                "{code} manque dans CFBundleLocalizations"
            );
        }
    }
}
