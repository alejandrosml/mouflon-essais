//! Les battements d'une horloge de l'écran qui bat sur un autre fil que
//! celui qui fait les images, et de quoi les donner sous Windows : à chaque
//! image que le compositeur vient de finir ([`Grid`]), ou, pour comparer,
//! prévus d'avance entre deux rafraîchissements de l'écran ([`Plan`]).

use std::sync::Mutex;
use std::sync::atomic::{AtomicU32, Ordering};

use crate::Beat;

/// Les battements d'une horloge de l'écran : le dernier (les battements se
/// remplacent : un fil en retard rattrape tout d'une image), et combien ont
/// été remplacés avant d'être pris.
#[derive(Debug, Default)]
pub struct Beats {
    latest: Mutex<Option<Beat>>,
    coalesced: AtomicU32,
}

impl Beats {
    /// Le dernier battement, s'il n'a pas encore été pris.
    pub fn take(&self) -> Option<Beat> {
        self.latest.lock().ok()?.take()
    }

    /// Battements remplacés depuis la dernière fois.
    pub fn take_coalesced(&self) -> u32 {
        self.coalesced.swap(0, Ordering::Relaxed)
    }

    /// Un battement de l'horloge (appelé par elle).
    #[cfg_attr(not(any(target_os = "macos", target_os = "windows")), allow(dead_code))]
    pub(crate) fn push(&self, beat: Beat) {
        if let Ok(mut latest) = self.latest.lock()
            && latest.replace(beat).is_some()
        {
            self.coalesced.fetch_add(1, Ordering::Relaxed);
        }
    }
}

/// De combien l'heure retenue d'un rafraîchissement se rapproche de l'heure
/// où on l'a vu (le reste vient de la cadence de l'écran).
const FOLLOW: f64 = 0.02;
/// Au-delà de tant de rafraîchissements manqués d'un coup, on repart de zéro.
const MAX_MISSED: f64 = 4.0;
/// Un battement tous les tant de rafraîchissements, au plus.
const MAX_EVERY: f64 = 16.0;
/// Un battement prévu dans moins que cela est déjà manqué : on vise le
/// suivant (secondes).
const TOO_CLOSE: f64 = 0.000_5;

/// Un battement tous les combien de rafraîchissements, pour `rate`
/// battements par seconde sur un écran qui en fait un tous les `period` :
/// au plus un par rafraîchissement, et un diviseur de la cadence de l'écran
/// (chaque image reste affichée le même nombre de fois).
#[cfg_attr(not(target_os = "windows"), allow(dead_code))]
fn every(period: f64, rate: f64) -> f64 {
    if rate > 0.0 && period > 0.0 {
        (1.0 / (period * rate)).round().clamp(1.0, MAX_EVERY)
    } else {
        1.0
    }
}

/// Les battements d'une horloge qui se réveille d'elle-même, par une
/// minuterie fine, à des heures prévues d'avance sur la grille de l'écran :
/// à une part fixe de la période (`phase`) après chaque rafraîchissement.
///
/// Sa cadence ne dépend pas de ce que le compositeur de Windows arrive à
/// montrer : s'il est lent, des images sont calculées pour rien. C'est
/// pourquoi l'horloge suit d'ordinaire le compositeur ([`Grid`]) ; celle-ci
/// sert à comparer (voir `windows/clock.rs`).
#[derive(Clone, Copy, Debug, Default, PartialEq)]
#[cfg_attr(not(target_os = "windows"), allow(dead_code))]
pub(crate) struct Plan {
    /// Heure prévue du dernier battement donné.
    last: Option<f64>,
}

#[cfg_attr(not(target_os = "windows"), allow(dead_code))]
impl Plan {
    /// Le prochain battement, à attendre : son heure (après `now`), et le
    /// temps jusqu'au suivant. `anchor` est l'heure d'un rafraîchissement de
    /// l'écran, n'importe lequel ; l'écran en fait un tous les `period` ; on
    /// veut `rate` battements par seconde.
    pub(crate) fn next(
        &mut self,
        now: f64,
        anchor: f64,
        period: f64,
        phase: f64,
        rate: f64,
    ) -> (f64, f64) {
        let every = every(period, rate);
        let origin = anchor + phase * period;
        // Le premier créneau qu'on peut encore attendre.
        let soonest = ((now + TOO_CLOSE - origin) / period).floor() + 1.0;
        let slot = match self.last {
            // À la suite du précédent, sur la grille d'aujourd'hui (elle se
            // recale un peu à chaque lecture de l'heure de l'écran).
            Some(last) => {
                let after = ((last - origin) / period).round() + every;
                // En retard : le créneau est passé, on prend le prochain.
                // Bien trop loin (l'heure de l'écran a sauté) : de même.
                if after < soonest || after > soonest + every {
                    soonest
                } else {
                    after
                }
            }
            None => soonest,
        };
        let due = origin + slot * period;
        self.last = Some(due);
        (due, every * period)
    }

    /// L'horloge s'arrête : le prochain battement repartira de l'heure qu'il
    /// sera.
    pub(crate) fn reset(&mut self) {
        self.last = None;
    }
}

/// Les rafraîchissements de l'écran, tels que les voit un fil qui attend
/// chacun d'eux, mis en battements réguliers.
///
/// Le fil se réveille un peu après chaque rafraîchissement, jamais avec le
/// même retard. Donner ces heures de réveil telles quelles ferait avancer
/// les animaux par à-coups : ce qui compte pour eux, c'est le temps entre
/// deux images affichées, et l'écran les affiche à cadence fixe. Les heures
/// retenues suivent donc une grille au pas de l'écran, qui ne se recale sur
/// les réveils que tout doucement (ou d'un coup, après une longue pause).
#[derive(Clone, Copy, Debug, Default, PartialEq)]
#[cfg_attr(not(target_os = "windows"), allow(dead_code))]
pub(crate) struct Grid {
    /// Heure retenue du dernier rafraîchissement.
    previous: Option<f64>,
    /// Rafraîchissements depuis le dernier battement.
    since_beat: u32,
}

#[cfg_attr(not(target_os = "windows"), allow(dead_code))]
impl Grid {
    /// Un rafraîchissement vient d'avoir lieu, vu à `now` (secondes) ;
    /// l'écran en fait un tous les `period` ; on veut `rate` battements par
    /// seconde (au plus un par rafraîchissement, et un diviseur de la
    /// cadence de l'écran : chaque image reste affichée le même nombre de
    /// fois). Rend le battement, s'il y en a un à donner maintenant.
    pub(crate) fn tick(&mut self, now: f64, period: f64, rate: f64) -> Option<Beat> {
        let every = every(period, rate);
        let on_grid = self.previous.and_then(|previous| {
            let steps = ((now - previous) / period).round().max(1.0);
            let expected = previous + steps * period;
            (steps <= MAX_MISSED && (now - expected).abs() < 0.5 * period)
                .then_some((expected + FOLLOW * (now - expected), steps as u32))
        });
        let vsync = match on_grid {
            Some((vsync, steps)) => {
                self.since_beat = self.since_beat.saturating_add(steps);
                vsync
            }
            // Première fois, ou longue pause : la grille repart d'ici, par
            // un battement.
            None => {
                self.since_beat = u32::MAX;
                now
            }
        };
        self.previous = Some(vsync);
        if f64::from(self.since_beat) < every {
            return None;
        }
        self.since_beat = 0;
        Some(Beat {
            target: vsync + every * period,
            vsync,
            called: now,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const HZ60: f64 = 1.0 / 60.0;

    /// Des réveils à 60 Hz, chacun avec son retard (millisecondes).
    fn wakes(delays: &[f64]) -> Vec<f64> {
        delays
            .iter()
            .enumerate()
            .map(|(i, late)| 10.0 + i as f64 * HZ60 + late / 1000.0)
            .collect()
    }

    #[test]
    fn beats_replace_each_other_and_count_the_replaced() {
        let beats = Beats::default();
        assert!(beats.take().is_none());
        for k in 0..3 {
            beats.push(Beat {
                target: f64::from(k),
                ..Beat::default()
            });
        }
        assert_eq!(beats.take().map(|b| b.target), Some(2.0));
        assert_eq!(beats.take_coalesced(), 2);
        assert!(beats.take().is_none());
        assert_eq!(beats.take_coalesced(), 0);
    }

    #[test]
    fn jittery_wakes_become_regular_beats() {
        let mut grid = Grid::default();
        let delays = [0.2, 0.9, 0.1, 1.4, 0.3, 0.2, 1.1, 0.4, 0.2, 0.8, 0.3, 0.5];
        let beats: Vec<Beat> = wakes(&delays)
            .into_iter()
            .map(|now| grid.tick(now, HZ60, 60.0).expect("un battement par image"))
            .collect();
        for pair in beats.windows(2) {
            let step = pair[1].target - pair[0].target;
            // Le temps entre deux images affichées : celui de l'écran, à
            // quelques centièmes de milliseconde près (les réveils, eux,
            // varient de plus d'une milliseconde).
            assert!((step - HZ60).abs() < 0.000_03, "{step}");
            assert!(pair[1].vsync > pair[0].vsync);
        }
        assert!((beats[0].target - beats[0].vsync - HZ60).abs() < 1e-12);
    }

    #[test]
    fn a_slower_rate_beats_every_few_refreshes() {
        let mut grid = Grid::default();
        let seen: Vec<Option<Beat>> = wakes(&[0.0; 13])
            .into_iter()
            .map(|now| grid.tick(now, HZ60, 30.0))
            .collect();
        let given: Vec<usize> = (0..seen.len()).filter(|&i| seen[i].is_some()).collect();
        assert_eq!(given, [0, 2, 4, 6, 8, 10, 12], "un sur deux");
        let beat = seen[2].expect("battement");
        assert!((beat.target - beat.vsync - 2.0 * HZ60).abs() < 1e-9);
        // Vingt images par seconde : un rafraîchissement sur trois.
        let mut grid = Grid::default();
        let given = wakes(&[0.0; 13])
            .into_iter()
            .filter(|&now| grid.tick(now, HZ60, 20.0).is_some())
            .count();
        assert_eq!(given, 5);
        // Plus vite que l'écran : un par rafraîchissement, pas plus.
        let mut grid = Grid::default();
        let given = wakes(&[0.0; 6])
            .into_iter()
            .filter(|&now| grid.tick(now, HZ60, 144.0).is_some())
            .count();
        assert_eq!(given, 6);
    }

    #[test]
    fn a_missed_refresh_counts_double_and_a_long_pause_starts_over() {
        let mut grid = Grid::default();
        let first = grid.tick(10.0, HZ60, 60.0).expect("premier");
        // Le suivant est manqué : celui d'après arrive deux pas plus loin.
        let late = grid
            .tick(10.0 + 2.0 * HZ60 + 0.0004, HZ60, 60.0)
            .expect("après");
        assert!((late.vsync - first.vsync - 2.0 * HZ60).abs() < 0.000_02);
        // Une seconde de pause (écran éteint, session verrouillée) : on
        // repart de l'heure du réveil.
        let resumed = grid.tick(11.5, HZ60, 60.0).expect("au retour");
        assert_eq!(resumed.vsync, 11.5);
        // Un réveil bien trop tôt (une attente qui n'a pas attendu) ne
        // fait pas reculer l'heure.
        let early = grid.tick(11.5 + 0.002, HZ60, 60.0).expect("trop tôt");
        assert!(early.vsync > resumed.vsync);
    }

    /// Où tombe `time` dans la période de l'écran qui bat à `anchor` : de 0
    /// (au rafraîchissement) à 1 (au suivant).
    fn phase_of(time: f64, anchor: f64) -> f64 {
        ((time - anchor) / HZ60).rem_euclid(1.0)
    }

    #[test]
    fn planned_beats_fall_midway_between_two_refreshes() {
        let mut plan = Plan::default();
        let anchor = 100.0;
        // On demande le suivant un peu après chaque battement, jamais avec
        // le même retard.
        let mut now = 100.003;
        let mut dues = Vec::new();
        for late in [0.2, 1.4, 0.1, 0.9, 2.6, 0.3, 0.0, 1.1] {
            let (due, step) = plan.next(now, anchor, HZ60, 0.5, 60.0);
            assert!(due > now, "jamais dans le passé");
            assert!((phase_of(due, anchor) - 0.5).abs() < 1e-6);
            assert!((step - HZ60).abs() < 1e-12);
            dues.push(due);
            now = due + late / 1000.0;
        }
        for pair in dues.windows(2) {
            assert!((pair[1] - pair[0] - HZ60).abs() < 1e-9, "un par image");
        }
    }

    #[test]
    fn planned_beats_follow_the_screen_when_its_clock_is_read_again() {
        let mut plan = Plan::default();
        let (first, _) = plan.next(50.0, 50.0, HZ60, 0.5, 60.0);
        // L'heure de l'écran est relue : un autre rafraîchissement, et la
        // grille a glissé d'un tiers de milliseconde.
        let anchor = 50.0 + 7.0 * HZ60 + 0.000_33;
        let (second, _) = plan.next(first + 0.000_2, anchor, HZ60, 0.5, 60.0);
        assert!((second - first - HZ60 - 0.000_33).abs() < 1e-9);
        assert!((phase_of(second, anchor) - 0.5).abs() < 1e-6);
        // Elle saute d'une demi-période (un autre écran) : le battement
        // suivant reste à venir, à sa place sur la nouvelle grille.
        let anchor = anchor + 0.5 * HZ60;
        let (third, _) = plan.next(second + 0.000_2, anchor, HZ60, 0.5, 60.0);
        assert!(third > second + 0.000_2);
        assert!(third - second < 2.0 * HZ60);
        assert!((phase_of(third, anchor) - 0.5).abs() < 1e-6);
    }

    #[test]
    fn a_late_planned_beat_skips_to_the_next_one_still_to_come() {
        let mut plan = Plan::default();
        let (first, _) = plan.next(10.0, 10.0, HZ60, 0.5, 60.0);
        // Le fil a dormi trois images de trop : pas de battements en
        // rafale pour rattraper, le prochain est le premier encore à venir.
        let now = first + 3.4 * HZ60;
        let (next, _) = plan.next(now, 10.0, HZ60, 0.5, 60.0);
        assert!(next > now && next - now < HZ60);
        // Demandé juste avant un créneau, on ne le vise plus : trop près.
        let (after, _) = plan.next(next + HZ60 - 0.000_1, 10.0, HZ60, 0.5, 60.0);
        assert!((after - next - 2.0 * HZ60).abs() < 1e-9);
        // Après un arrêt, on repart de l'heure qu'il est.
        plan.reset();
        let (resumed, _) = plan.next(99.0, 10.0, HZ60, 0.5, 60.0);
        assert!(resumed > 99.0 && resumed - 99.0 <= HZ60 + 1e-9);
    }

    #[test]
    fn planned_beats_divide_the_rate_of_the_screen() {
        let mut plan = Plan::default();
        let mut now = 20.0;
        let mut dues = Vec::new();
        for _ in 0..5 {
            let (due, step) = plan.next(now, 20.0, HZ60, 0.5, 30.0);
            assert!((step - 2.0 * HZ60).abs() < 1e-12);
            dues.push(due);
            now = due + 0.000_3;
        }
        for pair in dues.windows(2) {
            assert!((pair[1] - pair[0] - 2.0 * HZ60).abs() < 1e-9, "un sur deux");
        }
        // La cadence remonte : de nouveau un par image, sans trou.
        let (due, step) = plan.next(now, 20.0, HZ60, 0.5, 60.0);
        assert!((step - HZ60).abs() < 1e-12);
        assert!((due - dues[4] - HZ60).abs() < 1e-9);
    }
}
