//! Le tempérament des animaux. Calmes, ils se reposent davantage ; actifs,
//! ils bougent davantage ; énervés, ils foncent et bousculent ceux qu'ils
//! rencontrent. Le tempérament ne change pas ce que l'animal sait faire :
//! seulement ce qu'il choisit le plus volontiers parmi ce que son auteur a
//! prévu, et combien de temps il y reste.

use crate::def::{Behavior, Gesture, Motion, Script};

/// L'allure d'un comportement.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Energy {
    /// Ni repos ni élan : un demi-tour, quelques pas à reculons, une chute,
    /// un atterrissage, tenu…
    Neutral,
    /// Debout, assis, à regarder autour de lui, à brouter.
    Rest,
    /// Il dort.
    Sleep,
    /// Il marche.
    Walk,
    /// Il court, il saute.
    Dash,
}

/// Mots qui disent l'allure d'une animation desktopPet, dans son nom (du
/// plus sûr au moins sûr : « fall » avant « run »).
const WORDS: [(Energy, &[&str]); 5] = [
    (
        Energy::Neutral,
        &[
            "fall", "drop", "land", "drag", "turn", "rotate", "spawn", "kill", "fade", "boing",
            "pull",
        ],
    ),
    (Energy::Sleep, &["sleep", "nap", "doze"]),
    (
        Energy::Dash,
        &[
            "run", "jump", "gallop", "dash", "hop", "bounce", "chase", "charge", "fly",
        ],
    ),
    (Energy::Walk, &["walk", "trot"]),
    (
        Energy::Rest,
        &[
            "sit", "eat", "graze", "idle", "stand", "look", "rest", "yawn", "lie", "bath", "drink",
            "sniff", "scratch", "wait",
        ],
    ),
];

impl Energy {
    /// L'allure d'un comportement : d'après son mouvement ; pour une
    /// animation desktopPet, d'après son nom, sinon d'après ce dont elle
    /// fait avancer l'animal.
    #[must_use]
    pub fn of(behavior: &Behavior) -> Self {
        if behavior.sleeping {
            return Self::Sleep;
        }
        match &behavior.motion {
            Motion::Stand | Motion::Sit | Motion::Look | Motion::Graze => Self::Rest,
            Motion::Sleep => Self::Sleep,
            Motion::Walk { .. } => Self::Walk,
            Motion::Run { .. }
            | Motion::Charge { .. }
            | Motion::Jump { .. }
            | Motion::Pronk { .. }
            | Motion::Leap { .. } => Self::Dash,
            // Reculer prépare autre chose (une charge, un coup de front) :
            // deux agneaux qui jouent reculent aussi longtemps l'un que l'autre.
            Motion::Turn | Motion::Back { .. } | Motion::Fall | Motion::Land | Motion::Held => {
                Self::Neutral
            }
            Motion::Gesture(gesture) => match gesture {
                Gesture::Shake
                | Gesture::Stretch
                | Gesture::Yawn
                | Gesture::Brace
                | Gesture::Scratch => Self::Rest,
                Gesture::Paw
                | Gesture::Stamp
                | Gesture::Sniff
                | Gesture::Daze
                | Gesture::Bleat
                | Gesture::Rear
                | Gesture::Invite
                | Gesture::Flehmen
                | Gesture::Nurse
                | Gesture::MaternalCall => Self::Neutral,
            },
            Motion::Script(script) => {
                Self::by_name(&behavior.name).unwrap_or_else(|| Self::by_steps(script))
            }
        }
    }

    fn by_name(name: &str) -> Option<Self> {
        let name = name.to_ascii_lowercase();
        WORDS
            .iter()
            .find(|(_, words)| words.iter().any(|word| name.contains(word)))
            .map(|&(energy, _)| energy)
    }

    /// Une animation qui ne fait pas avancer l'animal est un repos.
    fn by_steps(script: &Script) -> Self {
        let pace = match &script.steps {
            Some(steps) if !steps.is_empty() => {
                steps.iter().map(|s| s.x.abs() + s.y.abs()).sum::<f32>() / steps.len() as f32
            }
            Some(_) => 0.0,
            None => {
                let at = |pair: &[crate::expr::Expr; 2]| {
                    Some(pair[0].as_constant()?.abs() + pair[1].as_constant()?.abs())
                };
                match (at(&script.start), at(&script.end)) {
                    (Some(a), Some(b)) => ((a + b) * 0.5) as f32,
                    // Calculée (au hasard, d'après l'écran…) : il bouge.
                    _ => return Self::Walk,
                }
            }
        };
        if pace < 0.5 { Self::Rest } else { Self::Walk }
    }
}

/// Le tempérament d'un animal.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
pub enum Temper {
    /// Tel que son auteur l'a fait.
    #[default]
    Even,
    /// Il dort, s'assied et broute davantage, et plus longtemps.
    Calm,
    /// Il marche, court et saute davantage.
    Lively,
    /// Il fonce, et bouscule ceux qu'il rencontre.
    Rowdy,
}

impl Temper {
    /// Le tempérament d'un agneau, quand celui du troupeau est `self` : un
    /// cran plus vif que les adultes (un calme vit comme son auteur l'a fait,
    /// les autres sont actifs ; un énervé le reste). Les jeux d'agneaux, que
    /// chaque fichier réserve aux actifs, sont ainsi ceux de tous les agneaux
    /// (voir `docs/animation-animale.md`, « Les jeux d'agneaux »).
    #[must_use]
    pub fn of_lamb(self) -> Self {
        match self {
            Self::Calm => Self::Even,
            Self::Even | Self::Lively => Self::Lively,
            Self::Rowdy => Self::Rowdy,
        }
    }

    /// Combien de fois plus (ou moins) il choisit un comportement de cette
    /// allure, parmi ceux qui s'offrent à lui.
    #[must_use]
    pub fn appetite(self, energy: Energy) -> f32 {
        match (self, energy) {
            (Self::Even, _) | (_, Energy::Neutral) => 1.0,
            (Self::Calm, Energy::Rest) => 2.0,
            (Self::Calm, Energy::Sleep) => 2.5,
            (Self::Calm, Energy::Walk) => 0.7,
            (Self::Calm, Energy::Dash) => 0.25,
            (Self::Lively, Energy::Rest) => 0.6,
            (Self::Lively, Energy::Sleep) => 0.3,
            (Self::Lively, Energy::Walk) => 1.4,
            (Self::Lively, Energy::Dash) => 2.5,
            (Self::Rowdy, Energy::Rest) => 0.45,
            (Self::Rowdy, Energy::Sleep) => 0.2,
            (Self::Rowdy, Energy::Walk) => 1.0,
            (Self::Rowdy, Energy::Dash) => 3.5,
        }
    }

    /// Combien de temps il reste à un comportement de cette allure, par
    /// rapport à d'habitude.
    #[must_use]
    pub fn patience(self, energy: Energy) -> f32 {
        match (self, energy) {
            (Self::Even, _) | (_, Energy::Neutral) => 1.0,
            (Self::Calm, Energy::Rest | Energy::Sleep) => 1.5,
            (Self::Calm, Energy::Walk | Energy::Dash) => 0.8,
            (Self::Lively, Energy::Rest | Energy::Sleep) => 0.6,
            (Self::Lively, Energy::Walk | Energy::Dash) => 1.25,
            (Self::Rowdy, Energy::Rest | Energy::Sleep) => 0.5,
            (Self::Rowdy, Energy::Walk | Energy::Dash) => 1.3,
        }
    }

    /// Il bouscule ceux qu'il rencontre, au lieu de les saluer.
    #[must_use]
    pub fn jostles(self) -> bool {
        self == Self::Rowdy
    }
}

/// Le tempérament des animaux, réglé pour tout le troupeau.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
pub enum Mood {
    /// Chacun tel que son auteur l'a fait.
    #[default]
    Even,
    /// Chacun le sien, tiré au sort à son arrivée : quatre sur dix tels
    /// que leur auteur les a faits, un quart calmes, un quart actifs, un
    /// sur dix énervé.
    Mixed,
    Calm,
    Lively,
    Rowdy,
}

impl Mood {
    /// Le tempérament de l'animal `id` (son numéro dans le troupeau) : au
    /// hasard, toujours le même pour le même animal.
    #[must_use]
    pub fn temper_of(self, id: u32) -> Temper {
        match self {
            Self::Even => Temper::Even,
            Self::Calm => Temper::Calm,
            Self::Lively => Temper::Lively,
            Self::Rowdy => Temper::Rowdy,
            Self::Mixed => {
                // Un mélange de son numéro (splitmix64), ramené entre 0 et 1.
                let mut h = (u64::from(id) + 1).wrapping_mul(0x9E37_79B9_7F4A_7C15);
                h = (h ^ (h >> 30)).wrapping_mul(0xBF58_476D_1CE4_E5B9);
                h = (h ^ (h >> 27)).wrapping_mul(0x94D0_49BB_1331_11EB);
                h ^= h >> 31;
                let draw = (h >> 40) as f32 / (1u64 << 24) as f32;
                if draw < 0.4 {
                    Temper::Even
                } else if draw < 0.65 {
                    Temper::Calm
                } else if draw < 0.9 {
                    Temper::Lively
                } else {
                    Temper::Rowdy
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_mixed_flock_holds_every_temper_in_the_right_share() {
        let mut counts = [0usize; 4];
        for id in 0..10_000 {
            let i = match Mood::Mixed.temper_of(id) {
                Temper::Even => 0,
                Temper::Calm => 1,
                Temper::Lively => 2,
                Temper::Rowdy => 3,
            };
            counts[i] += 1;
        }
        for (count, share) in counts.iter().zip([0.4, 0.25, 0.25, 0.1]) {
            let got = *count as f64 / 10_000.0;
            assert!((got - share).abs() < 0.02, "{counts:?}");
        }
        // Toujours le même pour le même animal.
        assert_eq!(Mood::Mixed.temper_of(7), Mood::Mixed.temper_of(7));
        assert_eq!(Mood::Rowdy.temper_of(7), Temper::Rowdy);
    }

    #[test]
    fn tempers_lean_the_way_they_say() {
        for energy in [Energy::Neutral, Energy::Rest, Energy::Walk, Energy::Dash] {
            assert_eq!(Temper::Even.appetite(energy), 1.0);
            assert_eq!(Temper::Even.patience(energy), 1.0);
        }
        // Calme : le repos d'abord ; actif et énervé : l'élan.
        assert!(Temper::Calm.appetite(Energy::Rest) > Temper::Calm.appetite(Energy::Walk));
        assert!(Temper::Calm.appetite(Energy::Walk) > Temper::Calm.appetite(Energy::Dash));
        assert!(Temper::Lively.appetite(Energy::Dash) > Temper::Lively.appetite(Energy::Rest));
        assert!(Temper::Rowdy.appetite(Energy::Dash) > Temper::Lively.appetite(Energy::Dash));
        assert!(Temper::Calm.patience(Energy::Sleep) > 1.0);
        assert!(Temper::Rowdy.patience(Energy::Rest) < 1.0);
        // Un demi-tour ou une chute ne dépendent pas de l'humeur.
        for temper in [Temper::Calm, Temper::Lively, Temper::Rowdy] {
            assert_eq!(temper.appetite(Energy::Neutral), 1.0);
        }
        assert!(Temper::Rowdy.jostles() && !Temper::Lively.jostles());
    }

    #[test]
    fn an_animation_tells_its_pace_by_its_name_or_its_steps() {
        assert_eq!(Energy::by_name("Sleep1a"), Some(Energy::Sleep));
        assert_eq!(Energy::by_name("run_fast"), Some(Energy::Dash));
        assert_eq!(Energy::by_name("fall"), Some(Energy::Neutral));
        assert_eq!(Energy::by_name("eat flower"), Some(Energy::Rest));
        assert_eq!(Energy::by_name("pissing"), None);
        assert_eq!(
            Energy::by_steps(&Script::constant(0.0, 0.0, 1.0)),
            Energy::Rest
        );
        assert_eq!(
            Energy::by_steps(&Script::constant(2.0, 0.0, 1.0)),
            Energy::Walk
        );
    }
}
