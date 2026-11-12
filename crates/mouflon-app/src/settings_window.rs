//! La fenêtre des réglages : ce qu'elle contient, dans la langue de
//! l'utilisateur, ce qu'elle montre de l'état de l'app, et ce que fait
//! chacun de ses contrôles (une [`Action`], comme les menus).

use mouflon_platform::{
    Form, FormButton, FormExtra, FormRow, FormSection, FormValue, LoginItem, UiEvent,
};

use crate::i18n::Texts;
use crate::menu::Action;

// Identifiants des contrôles.
const SIZE: u32 = 1;
const SOUNDS: u32 = 2;
const COLLISIONS: u32 = 3;
const FRONTMOST: u32 = 4;
const DOCK: u32 = 5;
const LOGIN: u32 = 6;
/// « Ouvrir Réglages Système » sous « Marcher sous le Dock ».
const ACCESSIBILITY: u32 = 7;
/// « Ouvrir Réglages Système » sous « Ouvrir Mouflon à l'ouverture de session ».
const LOGIN_APPROVAL: u32 = 8;
const FOLDER: u32 = 9;
const PHYSICS: u32 = 10;
const SURPRISES: u32 = 11;
const MOOD: u32 = 12;
const SHORTCUTS: u32 = 13;
const ESCAPE: u32 = 14;
/// « Ouvrir Réglages Système » sous « Échap deux fois… ».
const ESCAPE_ACCESSIBILITY: u32 = 15;
const ALL_SCREENS: u32 = 16;

/// Ce que la fenêtre montre : les réglages de l'app, et ce qu'en dit le
/// système (autorisation d'Accessibilité, ouverture de session).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Shown {
    /// Rang de la taille dans [`crate::settings::SIZES`].
    pub size: usize,
    /// Rang du tempérament dans [`crate::settings::MOODS`].
    pub mood: usize,
    pub sounds: bool,
    pub surprises: bool,
    pub collisions: bool,
    pub realistic_physics: bool,
    pub frontmost_only: bool,
    pub all_screens: bool,
    pub walk_under_dock: bool,
    pub shortcuts: bool,
    pub escape_quits: bool,
    /// Mouflon a l'autorisation d'Accessibilité.
    pub trusted: bool,
    pub login: LoginItem,
}

fn check(id: u32, label: &str, help: Option<&str>, extra: Option<FormExtra>) -> FormRow {
    FormRow::Check {
        id,
        label: label.to_owned(),
        help: help.map(str::to_owned),
        extra,
    }
}

fn extra(note: &str, id: u32, button: &str) -> FormExtra {
    FormExtra {
        note: note.to_owned(),
        button: FormButton {
            id,
            label: button.to_owned(),
        },
    }
}

/// Le système pour lequel la fenêtre est décrite : chacun a ses mots et ses
/// réglages (le Dock et l'autorisation d'Accessibilité sont ceux du Mac).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum System {
    Mac,
    Windows,
    /// Ailleurs : les réglages se montrent en liste, sans leurs explications.
    Other,
}

impl System {
    /// Celui sur lequel on tourne.
    #[must_use]
    pub const fn current() -> Self {
        if cfg!(target_os = "macos") {
            Self::Mac
        } else if cfg!(target_os = "windows") {
            Self::Windows
        } else {
            Self::Other
        }
    }
}

/// La fenêtre : les animaux, où ils se promènent, le démarrage (si le
/// système permet de le régler d'ici) et l'installation de nouveaux animaux.
#[must_use]
pub fn form(t: &Texts, login: LoginItem) -> Form {
    form_for(System::current(), crate::i18n::code(), t, login)
}

/// La fenêtre des réglages telle qu'elle se montre sur `system`, dans la
/// langue `code` (celle des textes `t`).
#[must_use]
pub fn form_for(system: System, code: &str, t: &Texts, login: LoginItem) -> Form {
    let windows = system == System::Windows;
    let mut walks = vec![
        check(
            PHYSICS,
            &t.realistic_physics,
            Some(&t.realistic_physics_help),
            None,
        ),
        check(COLLISIONS, &t.collisions, Some(&t.collisions_help), None),
        check(
            FRONTMOST,
            &t.frontmost_only,
            Some(&t.frontmost_only_help),
            None,
        ),
        check(ALL_SCREENS, &t.all_screens, Some(&t.all_screens_help), None),
    ];
    // Pas de Dock sous Windows : la barre des tâches prend toute la largeur
    // de l'écran, les animaux marchent dessus.
    if !windows {
        walks.push(check(
            DOCK,
            &t.walk_under_dock,
            Some(&t.walk_under_dock_help),
            Some(extra(
                &t.accessibility_waiting,
                ACCESSIBILITY,
                &t.open_system_settings,
            )),
        ));
    }
    // Sous Windows, les raccourcis se font avec Ctrl+Alt+Win, et rien n'est à
    // autoriser pour entendre Échap.
    let shortcuts_help = if windows {
        t.shortcuts_help
            .replace(crate::i18n::MAC_KEYS, crate::i18n::windows_keys(code))
    } else {
        t.shortcuts_help.clone()
    };
    let escape = if windows {
        check(
            ESCAPE,
            &t.escape_quits,
            Some(&t.escape_quits_help_windows),
            None,
        )
    } else {
        check(
            ESCAPE,
            &t.escape_quits,
            Some(&t.escape_quits_help),
            Some(extra(
                &t.accessibility_waiting,
                ESCAPE_ACCESSIBILITY,
                &t.open_system_settings,
            )),
        )
    };
    let (login_waiting, login_settings) = if windows {
        (&t.login_waiting_windows, &t.open_system_settings_windows)
    } else {
        (&t.login_waiting, &t.open_system_settings)
    };
    let mut sections = vec![
        FormSection {
            title: t.pets_section.clone(),
            rows: vec![
                FormRow::Steps {
                    id: SIZE,
                    label: t.size.clone(),
                    steps: [&t.size_small, &t.size_normal, &t.size_large, &t.size_huge]
                        .into_iter()
                        .cloned()
                        .collect(),
                    help: Some(t.size_help.clone()),
                },
                FormRow::Choice {
                    id: MOOD,
                    label: t.mood.clone(),
                    choices: [&t.mood_random, &t.mood_calm, &t.mood_active, &t.mood_angry]
                        .into_iter()
                        .cloned()
                        .collect(),
                    help: Some(t.mood_help.clone()),
                },
                check(SOUNDS, &t.pet_sounds, None, None),
                check(SURPRISES, &t.surprises, Some(&t.surprises_help), None),
            ],
        },
        FormSection {
            title: t.walks_section.clone(),
            rows: walks,
        },
        FormSection {
            title: t.shortcuts_section.clone(),
            rows: vec![
                check(SHORTCUTS, &t.shortcuts, Some(&shortcuts_help), None),
                escape,
            ],
        },
    ];
    if login != LoginItem::Unavailable {
        sections.push(FormSection {
            title: t.startup_section.clone(),
            rows: vec![check(
                LOGIN,
                &t.open_at_login,
                None,
                Some(extra(login_waiting, LOGIN_APPROVAL, login_settings)),
            )],
        });
    }
    sections.push(FormSection {
        title: t.my_pets_section.clone(),
        rows: vec![FormRow::Text {
            text: t.my_pets_help.clone(),
            button: Some(FormButton {
                id: FOLDER,
                label: t.open_pets_folder.clone(),
            }),
        }],
    });
    Form {
        title: t.settings_title.clone(),
        sections,
    }
}

/// Ce que montre chaque contrôle. Une remarque n'apparaît que quand il
/// manque quelque chose : l'autorisation d'Accessibilité pour marcher sous
/// le Dock, l'accord de l'utilisateur pour l'ouverture de session.
#[must_use]
pub fn values(shown: &Shown) -> Vec<FormValue> {
    vec![
        FormValue::Step(SIZE, shown.size),
        FormValue::Step(MOOD, shown.mood),
        FormValue::Check(SOUNDS, shown.sounds),
        FormValue::Check(SURPRISES, shown.surprises),
        FormValue::Check(PHYSICS, shown.realistic_physics),
        FormValue::Check(COLLISIONS, shown.collisions),
        FormValue::Check(FRONTMOST, shown.frontmost_only),
        FormValue::Check(ALL_SCREENS, shown.all_screens),
        FormValue::Check(DOCK, shown.walk_under_dock),
        FormValue::Shown(ACCESSIBILITY, shown.walk_under_dock && !shown.trusted),
        FormValue::Check(SHORTCUTS, shown.shortcuts),
        FormValue::Check(ESCAPE, shown.escape_quits),
        FormValue::Shown(ESCAPE_ACCESSIBILITY, shown.escape_quits && !shown.trusted),
        FormValue::Check(LOGIN, shown.login.is_requested()),
        FormValue::Shown(LOGIN_APPROVAL, shown.login == LoginItem::NeedsApproval),
    ]
}

/// Ce que fait un contrôle de la fenêtre des réglages (les articles des
/// menus surgissants ne passent pas par ici).
#[must_use]
pub fn action(event: UiEvent) -> Option<Action> {
    Some(match event {
        UiEvent::Step(SIZE, size) => Action::SetSize(size),
        UiEvent::Step(MOOD, mood) => Action::SetMood(mood),
        UiEvent::Check(SHORTCUTS, on) => Action::SetShortcuts(on),
        UiEvent::Check(ESCAPE, on) => Action::SetEscapeQuits(on),
        UiEvent::Button(ESCAPE_ACCESSIBILITY) => Action::OpenAccessibilitySettings,
        UiEvent::Check(SOUNDS, on) => Action::SetSounds(on),
        UiEvent::Check(SURPRISES, on) => Action::SetSurprises(on),
        UiEvent::Check(COLLISIONS, on) => Action::SetCollisions(on),
        UiEvent::Check(PHYSICS, on) => Action::SetRealisticPhysics(on),
        UiEvent::Check(FRONTMOST, on) => Action::SetFrontmostOnly(on),
        UiEvent::Check(ALL_SCREENS, on) => Action::SetAllScreens(on),
        UiEvent::Check(DOCK, on) => Action::SetWalkUnderDock(on),
        UiEvent::Check(LOGIN, on) => Action::SetOpenAtLogin(on),
        UiEvent::Button(ACCESSIBILITY) => Action::OpenAccessibilitySettings,
        UiEvent::Button(LOGIN_APPROVAL) => Action::OpenLoginItemsSettings,
        UiEvent::Button(FOLDER) => Action::OpenPetsFolder,
        UiEvent::FormShown => Action::RefreshSettings,
        UiEvent::Quit => Action::Quit,
        _ => return None,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn texts() -> Texts {
        toml::from_str(include_str!("../locales/fr.toml")).unwrap()
    }

    fn shown() -> Shown {
        Shown {
            size: 1,
            mood: 0,
            sounds: true,
            surprises: true,
            collisions: false,
            realistic_physics: true,
            frontmost_only: true,
            all_screens: true,
            walk_under_dock: false,
            shortcuts: true,
            escape_quits: false,
            trusted: false,
            login: LoginItem::Off,
        }
    }

    /// Identifiants des contrôles de la fenêtre : cases et curseurs, puis
    /// boutons des compléments, puis autres boutons.
    fn controls(form: &Form) -> (Vec<u32>, Vec<u32>, Vec<u32>) {
        let (mut values, mut extras, mut buttons) = (Vec::new(), Vec::new(), Vec::new());
        for row in form.sections.iter().flat_map(|s| &s.rows) {
            match row {
                FormRow::Check { id, extra, .. } => {
                    values.push(*id);
                    extras.extend(extra.as_ref().map(|e| e.button.id));
                }
                FormRow::Steps { id, .. } | FormRow::Choice { id, .. } => values.push(*id),
                FormRow::Text { button, .. } => buttons.extend(button.as_ref().map(|b| b.id)),
            }
        }
        (values, extras, buttons)
    }

    /// La fenêtre telle que le Mac la montre (celle qui a tout).
    fn mac_form(login: LoginItem) -> Form {
        form_for(System::Mac, "fr", &texts(), login)
    }

    #[test]
    fn every_control_shows_a_value_and_leads_to_an_action() {
        let form = mac_form(LoginItem::Off);
        let (values_ids, extras, buttons) = controls(&form);
        let shown = values(&shown());
        for id in &values_ids {
            assert!(
                shown.iter().any(
                    |v| matches!(v, FormValue::Check(i, _) | FormValue::Step(i, _) if i == id)
                ),
                "le contrôle {id} ne montre rien"
            );
            assert!(
                action(UiEvent::Check(*id, true)).is_some()
                    || action(UiEvent::Step(*id, 2)).is_some(),
                "le contrôle {id} ne fait rien"
            );
        }
        for id in &extras {
            assert!(shown.contains(&FormValue::Shown(*id, false)));
        }
        for id in extras.iter().chain(&buttons) {
            assert!(action(UiEvent::Button(*id)).is_some(), "bouton {id}");
        }
        // Chaque valeur va à un contrôle qui existe.
        for value in &shown {
            let (FormValue::Check(id, _) | FormValue::Step(id, _) | FormValue::Shown(id, _)) =
                value;
            assert!(values_ids.contains(id) || extras.contains(id), "{value:?}");
        }
    }

    #[test]
    fn a_control_gives_the_value_it_shows() {
        assert_eq!(
            action(UiEvent::Check(SOUNDS, false)),
            Some(Action::SetSounds(false))
        );
        assert_eq!(
            action(UiEvent::Check(DOCK, true)),
            Some(Action::SetWalkUnderDock(true))
        );
        assert_eq!(action(UiEvent::Step(SIZE, 3)), Some(Action::SetSize(3)));
        assert_eq!(action(UiEvent::Step(MOOD, 3)), Some(Action::SetMood(3)));
        assert_eq!(
            action(UiEvent::Check(ESCAPE, true)),
            Some(Action::SetEscapeQuits(true))
        );
        assert_eq!(
            action(UiEvent::Check(PHYSICS, false)),
            Some(Action::SetRealisticPhysics(false))
        );
        assert_eq!(action(UiEvent::FormShown), Some(Action::RefreshSettings));
        assert_eq!(action(UiEvent::Menu(1)), None);
    }

    #[test]
    fn notes_appear_only_when_something_is_missing() {
        let shown_extra = |s: &Shown, id: u32| values(s).contains(&FormValue::Shown(id, true));
        let mut s = shown();
        assert!(
            !shown_extra(&s, ACCESSIBILITY),
            "option coupée : rien à dire"
        );
        s.walk_under_dock = true;
        assert!(shown_extra(&s, ACCESSIBILITY), "autorisation manquante");
        s.trusted = true;
        assert!(!shown_extra(&s, ACCESSIBILITY));
        s.trusted = false;
        s.walk_under_dock = false;
        assert!(!shown_extra(&s, ESCAPE_ACCESSIBILITY));
        s.escape_quits = true;
        assert!(
            shown_extra(&s, ESCAPE_ACCESSIBILITY),
            "Échap : autorisation manquante"
        );
        assert!(!shown_extra(&s, ACCESSIBILITY), "le Dock n'est pas demandé");
        s.trusted = true;
        assert!(!shown_extra(&s, ESCAPE_ACCESSIBILITY));
        assert!(!shown_extra(&s, LOGIN_APPROVAL));
        s.login = LoginItem::NeedsApproval;
        assert!(shown_extra(&s, LOGIN_APPROVAL));
        assert!(values(&s).contains(&FormValue::Check(LOGIN, true)));
    }

    #[test]
    fn the_startup_section_needs_a_system_that_can_set_it() {
        let has_login = |login| controls(&mac_form(login)).0.contains(&LOGIN);
        assert!(has_login(LoginItem::On));
        assert!(!has_login(LoginItem::Unavailable));
    }

    /// Tous les textes d'une fenêtre, à la suite.
    fn all_text(form: &Form) -> String {
        let mut text = form.title.clone();
        for section in &form.sections {
            text.push_str(&section.title);
            for row in &section.rows {
                match row {
                    FormRow::Check {
                        label, help, extra, ..
                    } => {
                        text.push_str(label);
                        text.push_str(help.as_deref().unwrap_or_default());
                        if let Some(extra) = extra {
                            text.push_str(&extra.note);
                            text.push_str(&extra.button.label);
                        }
                    }
                    FormRow::Steps {
                        label, steps, help, ..
                    } => {
                        text.push_str(label);
                        text.push_str(&steps.concat());
                        text.push_str(help.as_deref().unwrap_or_default());
                    }
                    FormRow::Choice {
                        label,
                        choices,
                        help,
                        ..
                    } => {
                        text.push_str(label);
                        text.push_str(&choices.concat());
                        text.push_str(help.as_deref().unwrap_or_default());
                    }
                    FormRow::Text { text: line, button } => {
                        text.push_str(line);
                        text.push_str(button.as_ref().map_or("", |b| b.label.as_str()));
                    }
                }
            }
        }
        text
    }

    /// Sous Windows : ni Dock ni autorisation d'Accessibilité, les touches
    /// d'un clavier de PC, et les Paramètres de Windows. Dans chaque langue.
    #[test]
    fn on_windows_the_settings_speak_of_windows() {
        for (code, source) in crate::i18n::LOCALES {
            let t: Texts = toml::from_str(source).unwrap();
            let form = form_for(System::Windows, code, &t, LoginItem::Off);
            let (values_ids, extras, buttons) = controls(&form);
            assert!(!values_ids.contains(&DOCK), "{code} : pas de Dock");
            assert_eq!(extras, [LOGIN_APPROVAL], "{code}");
            assert_eq!(buttons, [FOLDER], "{code}");
            // Chaque contrôle montre une valeur et mène à une action.
            let shown = values(&shown());
            for id in &values_ids {
                assert!(
                    shown.iter().any(
                        |v| matches!(v, FormValue::Check(i, _) | FormValue::Step(i, _) if i == id)
                    ),
                    "{code} : le contrôle {id} ne montre rien"
                );
            }
            let text = all_text(&form);
            for mac_only in [
                crate::i18n::MAC_KEYS,
                "Dock",
                t.accessibility_waiting.as_str(),
                t.open_system_settings.as_str(),
                t.login_waiting.as_str(),
            ] {
                assert!(!text.contains(mac_only), "{code} : « {mac_only} »");
            }
            for shortcut in crate::keys::Shortcut::ALL {
                let keys = format!(
                    "{}{}",
                    crate::i18n::windows_keys(code),
                    shortcut.key().to_ascii_uppercase()
                );
                assert!(text.contains(&keys), "{code} : {keys} manque");
            }
            assert!(text.contains(&t.open_system_settings_windows), "{code}");
            assert!(text.contains(&t.login_waiting_windows), "{code}");
            assert!(text.contains(&t.escape_quits_help_windows), "{code}");
        }
        // Sur un clavier allemand, la touche s'appelle « Strg ».
        assert_eq!(crate::i18n::windows_keys("de"), "Strg+Alt+Win+");
        assert_eq!(crate::i18n::windows_keys("fr"), "Ctrl+Alt+Win+");
    }

    /// Sur Mac, rien ne change : le Dock, l'Accessibilité, ⌃⌥⌘.
    #[test]
    fn on_the_mac_the_settings_keep_the_dock_and_its_keys() {
        let form = mac_form(LoginItem::Off);
        let (values_ids, extras, _) = controls(&form);
        assert!(values_ids.contains(&DOCK));
        assert_eq!(
            extras,
            [ACCESSIBILITY, ESCAPE_ACCESSIBILITY, LOGIN_APPROVAL]
        );
        let text = all_text(&form);
        assert!(text.contains("⌃⌥⌘Q") && !text.contains("Ctrl+Alt"));
    }
}
