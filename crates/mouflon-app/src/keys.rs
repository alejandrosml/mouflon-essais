//! Les raccourcis clavier de Mouflon, valables dans toutes les apps, et la
//! touche Échap qui le fait quitter (deux fois de suite, ou tenue).
//!
//! Le système les entend (voir `mouflon_platform::set_shortcuts` et
//! `listen_escape`) ; ici, ce qu'ils font.

use std::time::{Duration, Instant};

use mouflon_platform::KeyMove;

use crate::menu::Action;

/// Un raccourci : ⌃⌥⌘ et sa lettre.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Shortcut {
    /// ⌃⌥⌘N : un animal (le préféré).
    Add,
    /// ⌃⌥⌘T : dix animaux au hasard.
    AddRandom,
    /// ⌃⌥⌘B : tous disent au revoir.
    GoodbyeAll,
    /// ⌃⌥⌘P : pause, ou reprise.
    Pause,
    /// ⌃⌥⌘S : les sons, ou le silence.
    Sounds,
    /// ⌃⌥⌘Q : quitter Mouflon.
    Quit,
}

impl Shortcut {
    pub const ALL: [Self; 6] = [
        Self::Add,
        Self::AddRandom,
        Self::GoodbyeAll,
        Self::Pause,
        Self::Sounds,
        Self::Quit,
    ];

    /// Son identifiant auprès du système.
    #[must_use]
    pub fn id(self) -> u32 {
        Self::ALL.iter().position(|s| *s == self).unwrap_or(0) as u32 + 1
    }

    #[must_use]
    pub fn from_id(id: u32) -> Option<Self> {
        Self::ALL.get(id.checked_sub(1)? as usize).copied()
    }

    /// Sa lettre (avec ⌃⌥⌘).
    #[must_use]
    pub fn key(self) -> char {
        match self {
            Self::Add => 'n',
            Self::AddRandom => 't',
            Self::GoodbyeAll => 'b',
            Self::Pause => 'p',
            Self::Sounds => 's',
            Self::Quit => 'q',
        }
    }

    /// Ce qu'il fait, d'après l'état de l'app : en pause, il fait reprendre ;
    /// les sons coupés, il les remet.
    #[must_use]
    pub fn action(self, paused: bool, sounds: bool) -> Action {
        match self {
            Self::Add => Action::AddDefault,
            Self::AddRandom => Action::AddRandom,
            Self::GoodbyeAll => Action::RemoveAll,
            Self::Pause => Action::SetPaused(!paused),
            Self::Sounds => Action::SetSounds(!sounds),
            Self::Quit => Action::Quit,
        }
    }

    /// Tous, tels que le système les réserve.
    #[must_use]
    pub fn for_system() -> Vec<mouflon_platform::Shortcut> {
        Self::ALL
            .iter()
            .map(|s| mouflon_platform::Shortcut {
                id: s.id(),
                key: s.key(),
            })
            .collect()
    }
}

/// Deux appuis sur Échap à moins de ce temps l'un de l'autre : Mouflon quitte.
pub const DOUBLE_PRESS: Duration = Duration::from_millis(450);
/// Échap tenue aussi longtemps : Mouflon quitte.
pub const HOLD: Duration = Duration::from_millis(1000);
/// Plus rien de la touche depuis ce temps : un relâchement a été manqué,
/// la touche ne compte plus comme tenue depuis l'appui d'avant.
const STALE: Duration = Duration::from_millis(2500);

/// Échap deux fois de suite, ou tenue : Mouflon quitte. La touche tenue se
/// reconnaît à la répétition du clavier (sans elle, seul le double appui
/// compte) : un relâchement manqué ne fait jamais quitter tout seul.
#[derive(Debug, Default)]
pub struct EscapeGesture {
    /// Le dernier appui (pas une répétition).
    pressed: Option<Instant>,
    /// Tenue depuis…
    held_since: Option<Instant>,
    /// La dernière nouvelle de la touche.
    last: Option<Instant>,
}

impl EscapeGesture {
    /// La touche bouge : `true` s'il faut quitter.
    pub fn key(&mut self, change: KeyMove, now: Instant) -> bool {
        let stale = self
            .last
            .is_none_or(|last| now.saturating_duration_since(last) > STALE);
        self.last = Some(now);
        match change {
            KeyMove::Down => {
                let double = self
                    .pressed
                    .is_some_and(|at| now.saturating_duration_since(at) <= DOUBLE_PRESS);
                // Un troisième appui rapide ne compte pas pour un deuxième double.
                self.pressed = if double { None } else { Some(now) };
                self.held_since = Some(now);
                double
            }
            KeyMove::Repeat => {
                if stale || self.held_since.is_none() {
                    self.held_since = Some(now);
                }
                self.held_since
                    .is_some_and(|since| now.saturating_duration_since(since) >= HOLD)
            }
            KeyMove::Up => {
                self.held_since = None;
                false
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_shortcut_has_its_own_letter_and_id() {
        let mut keys: Vec<char> = Shortcut::ALL.iter().map(|s| s.key()).collect();
        keys.sort_unstable();
        keys.dedup();
        assert_eq!(keys.len(), Shortcut::ALL.len());
        for shortcut in Shortcut::ALL {
            assert_eq!(Shortcut::from_id(shortcut.id()), Some(shortcut));
        }
        assert_eq!(Shortcut::from_id(0), None);
        assert_eq!(Shortcut::from_id(99), None);
        assert_eq!(Shortcut::Pause.action(true, true), Action::SetPaused(false));
        assert_eq!(
            Shortcut::Sounds.action(false, false),
            Action::SetSounds(true)
        );
    }

    /// Chaque langue dit les vrais raccourcis.
    #[test]
    fn the_settings_name_the_real_shortcuts() {
        for (lang, text) in crate::i18n::LOCALES {
            let texts: crate::i18n::Texts = toml::from_str(text).unwrap();
            for shortcut in Shortcut::ALL {
                let shown = format!("⌃⌥⌘{}", shortcut.key().to_ascii_uppercase());
                assert!(
                    texts.shortcuts_help.contains(&shown),
                    "{lang} : {shown} manque dans « {} »",
                    texts.shortcuts_help
                );
            }
        }
    }

    #[test]
    fn escape_twice_quits() {
        let t0 = Instant::now();
        let ms = |n: u64| t0 + Duration::from_millis(n);
        let mut g = EscapeGesture::default();
        assert!(!g.key(KeyMove::Down, ms(0)));
        assert!(!g.key(KeyMove::Up, ms(80)));
        assert!(g.key(KeyMove::Down, ms(300)));
        // Trop lents : rien.
        let mut g = EscapeGesture::default();
        assert!(!g.key(KeyMove::Down, ms(0)));
        assert!(!g.key(KeyMove::Up, ms(80)));
        assert!(!g.key(KeyMove::Down, ms(900)));
        assert!(!g.key(KeyMove::Up, ms(980)));
        // …mais ce dernier appui peut commencer un double.
        assert!(g.key(KeyMove::Down, ms(1200)));
    }

    #[test]
    fn escape_held_quits_but_a_missed_release_does_not() {
        let t0 = Instant::now();
        let ms = |n: u64| t0 + Duration::from_millis(n);
        let mut g = EscapeGesture::default();
        assert!(!g.key(KeyMove::Down, ms(0)));
        let mut at = 500;
        while at < 1000 {
            assert!(!g.key(KeyMove::Repeat, ms(at)), "{at} ms");
            at += 80;
        }
        assert!(g.key(KeyMove::Repeat, ms(1060)));

        // Un relâchement manqué, longtemps avant : une répétition seule ne
        // fait pas quitter, il faut tenir de nouveau.
        let mut g = EscapeGesture::default();
        assert!(!g.key(KeyMove::Down, ms(0)));
        assert!(!g.key(KeyMove::Repeat, ms(60_000)));
        assert!(!g.key(KeyMove::Repeat, ms(60_500)));
        assert!(g.key(KeyMove::Repeat, ms(61_000)));
    }
}
