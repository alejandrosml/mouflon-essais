//! Le cycle propre de chaque patte, et la place de son sabot.
//!
//! Le moteur fait avancer une phase commune, au rythme de la distance
//! parcourue ; chaque patte y a son moment, selon l'allure. Mais une patte
//! tient aussi son propre cycle, d'un pas à l'autre, et la place de son
//! sabot :
//!
//! - posé, le sabot reste où il est : il recule sous le corps exactement de
//!   ce que le corps avance, quoi qu'il arrive (l'allure change, l'animal
//!   ralentit, s'arrête, repart) ;
//! - la patte se lève quand son moment vient (à la foulée et avec la part
//!   d'appui qu'elle avait en se posant), ou quand son sabot est resté trop
//!   loin derrière ; en vol, elle rattrape son moment dans l'allure en
//!   cours, un peu plus vite ou plus lentement : c'est dans l'air qu'un
//!   mouton change d'allure ;
//! - quand l'animal s'arrête, une patte en l'air finit son pas, plus court à
//!   mesure qu'il ralentit, et se pose ; puis les pattes restées loin de
//!   leur place s'y remettent d'un petit pas, l'une après l'autre dans
//!   l'ordre de l'allure (par paires diagonales au trot) : il se remet
//!   d'aplomb ;
//! - quand il repart d'aplomb, un antérieur se lève le premier, seul ;
//!   sinon, les pattes dont le moment est en vol se lèvent les premières.
//!
//! Le rendu pose le sabot où on le lui dit. Voir `docs/animation-animale.md`.

use std::f32::consts::{PI, TAU};

use crate::math::{smoothstep, wrap_phase};

/// Une patte dans son cycle, et son sabot.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct LegCycle {
    /// Où elle en est de son cycle (fraction, de 0 à 1) : à l'appui de 0 à
    /// `duty`, en vol ensuite.
    pub at: f32,
    /// Sa part d'appui, et la longueur de sa foulée (en fois celle du pas),
    /// tenues depuis qu'elle s'est posée. Zéro : pas de cycle propre (un
    /// signal fabriqué à la main) ; le rendu prend alors la phase commune.
    pub duty: f32,
    pub stride: f32,
    /// La place du sabot, en avant (positif) ou en arrière de sa place au
    /// repos, en foulées du pas.
    pub x: f32,
    /// Sa hauteur au-dessus du sol, en centièmes de la hauteur de l'animal.
    pub raise: f32,
    /// La part du vol (0 posé, 1 au milieu du vol).
    pub flight: f32,
    /// Le pas en cours : levée, part faite (0 à 1), la vitesse du sabot (en
    /// foulées par part du pas), sa hauteur, son sens (1 vers l'avant, -1 à
    /// reculons), le moment où il est parti et ce qu'il dure du cycle, où il
    /// va se poser ; un pas pour se remettre d'aplomb.
    lifted: bool,
    done: f32,
    slope: f32,
    top: f32,
    way: f32,
    began: f32,
    span: f32,
    aim: f32,
    settling: bool,
}

/// Décalage de chaque patte dans le cycle (fraction), au pas, au trot et au
/// galop, dans l'ordre : antérieur de ce côté, antérieur de l'autre,
/// postérieur de ce côté, postérieur de l'autre. Les mêmes que ceux du
/// rendu (`leg_phase`, dans `mouflon-render`), qui les explique.
const OFFSETS: [[f32; 3]; 4] = [
    [0.27, 0.53, 0.55],
    [0.77, 1.03, 0.77],
    [0.0, 0.0, 0.29],
    [0.5, 0.5, 0.45],
];

/// Part du cycle où une patte est à l'appui (comme `Gait::duty` dans le
/// rendu) : `trot`, la part du trot qui n'est pas du galop.
fn duty(trot: f32, run: f32) -> f32 {
    let d = 0.64 + (0.48 - 0.64) * trot;
    d + (0.32 - d) * run
}

fn offset(leg: usize, trot: f32, run: f32) -> f32 {
    let [walk, trot_at, gallop] = OFFSETS[leg];
    let mid = walk + (trot_at - walk) * trot;
    mid + (gallop - mid) * run
}

/// Hauteur où monte un sabot (centièmes de la hauteur), comme `Gait::lift`
/// dans le rendu : 6 à 7 % de la hauteur au garrot au pas, un peu plus au
/// trot, bien plus au galop.
fn gait_lift(trot: f32, run: f32) -> f32 {
    3.4 + 0.6 * trot + 3.5 * run
}

/// En vol, une patte en retard sur son moment va plus vite, d'autant plus
/// qu'elle est en retard (4 : un quart de cycle de retard, elle va jusqu'à
/// 2 fois plus vite au milieu du vol) ; en avance, plus lentement. Elle part
/// et se pose à son rythme (le rattrapage suit un arc, nul aux deux bouts
/// du vol : le sabot quitte le sol et s'y pose à la vitesse de l'appui).
/// Jamais moins de 0,4 fois ni plus de 2,5 fois son rythme : un changement
/// d'allure se fait en une ou deux foulées, sans patte qui recule ni qui
/// saute.
const CATCH_UP: f32 = 4.0;
const PACE: (f32, f32) = (0.4, 2.5);

/// Un antérieur monte au plus haut à 38 % de son vol (comme `stepping` dans
/// le rendu), un postérieur à mi-vol.
const FORE_LIFT_SKEW: f32 = 0.716;

/// En vol, où le sabot va se poser suit l'allure (en ce temps, secondes)
/// jusqu'à cette part du vol, puis ne bouge plus.
const AIM_TIME: f32 = 0.06;
const AIM_UNTIL: f32 = 0.6;

/// Un sabot posé resté plus loin que ceci derrière (en demi-appuis du pas)
/// se lève sans attendre son moment : il repartait d'une place qui n'était
/// pas la sienne (au départ, après un arrêt).
const REACH: f32 = 1.2;

/// Se remettre d'aplomb : un pas dure ceci (secondes, environ le vol d'un
/// pas) ; une patte à moins de cette distance de sa place (en foulées du
/// pas) y reste ; une patte se lève quand la précédente est posée (les
/// trois autres la portent), mais deux pattes dont les moments sont plus
/// proches que ceci (en cycle) partent ensemble (les paires diagonales du
/// trot) ; la levée, en centièmes de la hauteur, de la plus courte à la
/// plus longue des remises d'aplomb (la moitié, puis les deux tiers d'un
/// pas de marche : assez pour qu'on la voie quitter l'herbe).
const SETTLE_TIME: f32 = 0.24;
const SETTLE_SLACK: f32 = 0.05;
const SETTLE_PAIR: f32 = 0.12;
const SETTLE_LIFT: (f32, f32) = (1.6, 2.4);

/// Où en est l'animal, pour ses pattes.
pub(super) struct Gaiting {
    /// La phase commune, déjà avancée (radians).
    pub phase: f32,
    /// Ce que le corps a parcouru vers l'avant pendant ce pas de la
    /// simulation (points ; négatif : à reculons), et sa vitesse (pt/s).
    pub travel: f32,
    pub speed: f32,
    pub dt: f32,
    /// La foulée du pas (points), celle de l'allure en cours (en fois celle
    /// du pas), et les poids du trot (la part qui n'est pas du galop) et du
    /// galop.
    pub stride: f32,
    pub scale: f32,
    pub trot: f32,
    pub run: f32,
    /// Il marche sur un sol (sinon, en l'air ou tenu, ses pattes suivent le
    /// corps et reviennent à leur place).
    pub grounded: bool,
    /// Il ne se remet pas d'aplomb (couché ou en train de se coucher, au
    /// milieu d'un geste, ramassé pour sauter).
    pub still: bool,
}

/// Repartir d'aplomb : la phase commune (fraction du cycle) où l'antérieur
/// de ce côté a son moment de se lever au pas, et lui seul (le postérieur de
/// l'autre côté vient de se poser). Le cheval qui part au trot de l'arrêt
/// bouge toujours un antérieur le premier (Tans, Nauwelaerts et Clayton
/// 2009) : le mouton part ici d'un antérieur.
pub(super) const MOVE_OFF: f32 = 0.51;

/// Les quatre sabots sont posés à leur place : il est d'aplomb.
pub(super) fn square(legs: &[LegCycle; 4]) -> bool {
    legs.iter().all(|l| !l.lifted && l.x.abs() <= SETTLE_SLACK)
}

/// Il repart (son premier pas de la simulation en mouvement) : chaque patte
/// posée reprend son moment à la phase commune `phase` (radians), avec la
/// foulée `scale` et la part d'appui de l'allure en cours. Arrêté, rien ne
/// bougeait : le moteur ne les tenait pas à jour (voir [`advance`]).
pub(super) fn move_off(legs: &mut [LegCycle; 4], phase: f32, scale: f32, trot: f32, run: f32) {
    let now = duty(trot, run);
    for (k, leg) in legs.iter_mut().enumerate() {
        if !leg.lifted && leg.duty > 0.0 {
            leg.at = wrap_phase(phase / TAU + offset(k, trot, run), 1.0);
            leg.duty = now;
            leg.stride = scale;
        }
    }
}

/// Une patte est en l'air : un pas en cours (de l'allure, ou pour se remettre
/// d'aplomb).
pub(super) fn stepping(legs: &[LegCycle; 4]) -> bool {
    legs.iter().any(|l| l.lifted)
}

/// L'écart de `a` à `b` dans le cycle, par le plus court (de -0,5 à 0,5 ;
/// `a` et `b` à moins d'un cycle et demi l'un de l'autre).
fn gap(a: f32, b: f32) -> f32 {
    let d = a - b;
    if d >= 0.5 {
        d - 1.0
    } else if d < -0.5 {
        d + 1.0
    } else {
        d
    }
}

/// La courbe d'un pas (Hermite) : de `from` (pente `leave`) à `to` (pente
/// `land`), à la part `s` du chemin ; rend la place et la pente.
fn hermite(from: f32, leave: f32, to: f32, land: f32, s: f32) -> (f32, f32) {
    let (s2, s3) = (s * s, s * s * s);
    let place = from * (2.0 * s3 - 3.0 * s2 + 1.0)
        + leave * (s3 - 2.0 * s2 + s)
        + to * (3.0 * s2 - 2.0 * s3)
        + land * (s3 - s2);
    let slope = from * (6.0 * s2 - 6.0 * s)
        + leave * (3.0 * s2 - 4.0 * s + 1.0)
        + to * (6.0 * s - 6.0 * s2)
        + land * (3.0 * s2 - 2.0 * s);
    (place, slope)
}

/// Avance les quatre pattes d'un pas de la simulation.
pub(super) fn advance(legs: &mut [LegCycle; 4], g: &Gaiting) {
    // Arrêté, aucune patte en l'air, d'aplomb ou sans s'y remettre (couché,
    // au milieu d'un geste), ou en l'air les pattes revenues à leur place :
    // rien ne bouge (le moment de chaque patte se reprend quand il repart,
    // voir [`move_off`]). La plupart des animaux d'un troupeau, la plupart
    // du temps.
    let rested = |l: &LegCycle| {
        !l.lifted
            && l.duty > 0.0
            && if g.grounded {
                g.still || l.x.abs() <= SETTLE_SLACK
            } else {
                l.x == 0.0 && l.raise == 0.0
            }
    };
    if g.travel == 0.0 && legs.iter().all(rested) {
        return;
    }
    let now = duty(g.trot, g.run);
    let stride = g.stride.max(1e-3);
    // De combien le corps avance, en foulées du pas, et en cycles de
    // l'allure en cours ; jusqu'où se posent les sabots (la demi-foulée de
    // l'appui, en foulées du pas).
    let shift = g.travel / stride;
    let cycles = g.travel / (stride * g.scale);
    let half = g.scale * now * 0.5;
    // La démarche, selon la vitesse rapportée à la longueur des pattes (1 :
    // il marche ; 0 : il est arrêté, ses pas ne suivent plus la distance).
    let rel = g.speed / (stride / 1.6).max(1.0);
    let going = smoothstep(0.05, 0.6, rel);
    let way = if g.travel > 0.0 {
        1.0
    } else if g.travel < 0.0 {
        -1.0
    } else {
        0.0
    };
    for (k, leg) in legs.iter_mut().enumerate() {
        let target = wrap_phase(g.phase / TAU + offset(k, g.trot, g.run), 1.0);
        if leg.duty <= 0.0 {
            // Il commence debout, d'aplomb.
            *leg = LegCycle {
                at: target,
                duty: now,
                stride: g.scale,
                ..LegCycle::default()
            };
            continue;
        }
        if !g.grounded {
            // En l'air ou tenu : les pattes suivent le corps et reviennent
            // à leur place (le rendu les replie ou les tend).
            let back = (1.0 - 10.0 * g.dt).max(0.0);
            let x = leg.x * back;
            *leg = LegCycle {
                at: target,
                x: if x.abs() < 1e-4 { 0.0 } else { x },
                ..*leg
            };
            leg.lifted = false;
            leg.raise = 0.0;
            leg.flight = 0.0;
            continue;
        }
        if leg.lifted {
            step(leg, k, g, target, shift, cycles, half, going, now);
        } else {
            // Posé : le sabot reste où il est.
            leg.x -= shift;
            if way == 0.0 {
                // Arrêté : elle garde son moment dans l'allure, et prendra la
                // foulée et l'appui de l'allure en cours pour repartir.
                leg.at = target;
                leg.duty = now;
                leg.stride = g.scale;
            } else {
                leg.at += g.travel / (stride * leg.stride);
            }
            // Son moment est venu (à reculons, le cycle à l'envers), ou elle
            // est restée trop loin derrière (devant, à reculons) : même s'il
            // avance tout doucement, elle fait un petit pas.
            let far = REACH * leg.stride.max(1.0) * leg.duty * 0.5;
            let (due, over) = if way > 0.0 {
                (leg.at >= leg.duty, leg.x < -far)
            } else if way < 0.0 {
                (leg.at < 0.0 || leg.at >= leg.duty, leg.x > far)
            } else {
                (false, false)
            };
            if (going > 0.02 && due) || over {
                // Son moment est venu : elle se lève (dans le sens où il va).
                // Son vol part du début du vol, ou de plus loin si elle aurait
                // déjà dû être en l'air (au départ), mais dure au moins la
                // moitié d'un vol.
                let span = (1.0 - leg.duty).max(0.05);
                let began = if way > 0.0 {
                    leg.at.clamp(leg.duty, 1.0 - 0.5 * span)
                } else {
                    let at = if leg.at < 0.0 {
                        leg.at + 1.0
                    } else if leg.at >= leg.duty {
                        leg.at
                    } else {
                        1.0
                    };
                    at.clamp(leg.duty + 0.5 * span, 1.0)
                };
                let left = if way > 0.0 {
                    1.0 - began
                } else {
                    began - leg.duty
                };
                let rate = cycles.abs() / left + (1.0 - going) * g.dt * (1.0 / SETTLE_TIME);
                leg.lifted = true;
                leg.settling = false;
                leg.done = 0.0;
                leg.slope = if rate > 0.0 { -shift / rate } else { 0.0 };
                leg.top = gait_lift(g.trot, g.run) * going.max(0.4);
                leg.way = way;
                leg.began = began;
                leg.span = left;
                leg.aim = way * going * half;
                leg.at = began;
            }
        }
    }
    if g.grounded && g.travel == 0.0 && going < 0.05 && !g.still {
        settle(legs);
    }
}

/// Un pas en cours (en vol).
#[allow(clippy::too_many_arguments)]
fn step(
    leg: &mut LegCycle,
    k: usize,
    g: &Gaiting,
    target: f32,
    shift: f32,
    cycles: f32,
    half: f32,
    going: f32,
    now: f32,
) {
    let span = leg.span.max(0.02);
    // Le pas avance avec la distance (en rattrapant son moment) ; à l'arrêt,
    // avec le temps : la patte finit son pas.
    let rate = if leg.settling {
        g.dt * (1.0 / SETTLE_TIME)
    } else {
        let lag = gap(target, leg.at);
        let arc = 4.0 * leg.done * (1.0 - leg.done);
        let pace = (1.0 + CATCH_UP * arc * lag * leg.way).clamp(PACE.0, PACE.1);
        cycles.abs() * pace / span + (1.0 - going) * g.dt * (1.0 / SETTLE_TIME)
    };
    let per = if rate > 0.0 { 1.0 / rate } else { 0.0 };
    // Ce qui reste de ce pas de la simulation une fois le sabot posé.
    let before = leg.done;
    let excess = (leg.done + rate - 1.0).max(0.0);
    let over = excess * span;
    let after = excess * per;
    leg.done = (leg.done + rate).min(1.0);
    // Il se pose immobile par rapport au sol, à sa place (remise d'aplomb),
    // ou devant (derrière, à reculons) d'autant que la démarche est ample.
    // Le but suit l'allure dans la première moitié du vol (en douceur : il
    // s'arrête, le pas raccourcit), puis ne bouge plus : la patte finit son
    // pas, sans traîner le sabot au ras du sol.
    let land = -shift * per;
    if leg.settling {
        leg.aim = 0.0;
    } else if leg.done < AIM_UNTIL {
        let goal = leg.way * going * half;
        leg.aim += (goal - leg.aim) * (g.dt * (1.0 / AIM_TIME)).min(1.0);
    }
    let to = leg.aim;
    // Le reste du chemin, repris à chaque pas de la simulation d'où en est
    // le sabot et à sa vitesse : si l'allure change en route (il s'arrête,
    // son pas raccourcit), le sabot n'a pas d'à-coup.
    let s = leg.done;
    let left = (1.0 - before).max(1e-6);
    let (mut place, slope) = hermite(
        leg.x,
        leg.slope * left,
        to,
        land * left,
        ((s - before) * (1.0 / left)).clamp(0.0, 1.0),
    );
    let mut slope = slope * (1.0 / left);
    if s >= 1.0 {
        // Posé pendant le reste de ce pas de la simulation : il recule
        // déjà sous le corps.
        place = to - after * shift;
    }
    // Jamais le sabot ne repart en arrière par rapport au sol (en avant, à
    // reculons) : il ramenait la patte pour suivre un corps qui allait vite,
    // le corps s'arrête, il descend où il est au lieu de racler l'herbe.
    if leg.way * (place - leg.x + shift) < 0.0 {
        place = leg.x - shift;
        slope = land;
    }
    leg.x = place;
    leg.slope = slope;
    leg.flight = (PI * s).sin();
    let lift = if k < 2 && !leg.settling {
        (PI * s.powf(FORE_LIFT_SKEW)).sin()
    } else {
        leg.flight
    };
    leg.raise = leg.top * lift.max(0.0);
    if !leg.settling {
        leg.at = leg.began + leg.way * s * span;
    }
    if s >= 1.0 {
        // Il se pose : la foulée et la part d'appui de l'allure en cours.
        let settled = leg.settling;
        leg.lifted = false;
        leg.settling = false;
        leg.raise = 0.0;
        leg.flight = 0.0;
        leg.stride = g.scale;
        // Ce que la patte a pris d'avance sur ce pas de la simulation reste à
        // l'appui (le moment se garde d'une foulée à l'autre).
        leg.duty = now;
        leg.at = if settled {
            target
        } else if leg.way > 0.0 {
            over.min(now * 0.5)
        } else {
            (now - over).max(now * 0.5)
        };
    }
}

/// À l'arrêt, se remettre d'aplomb : la patte la plus loin de sa place
/// parmi celles dont le moment vient le premier se lève et va s'y poser,
/// puis la suivante (sa paire diagonale avec elle, au trot).
fn settle(legs: &mut [LegCycle; 4]) {
    if legs.iter().any(|l| l.lifted) {
        return;
    }
    // Les pattes loin de leur place, dans l'ordre de leurs moments.
    let next = |l: &LegCycle| wrap_phase(l.duty - l.at + 1.0, 1.0);
    let first = (0..4)
        .filter(|&k| !legs[k].lifted && legs[k].x.abs() > SETTLE_SLACK)
        .min_by(|&a, &b| next(&legs[a]).total_cmp(&next(&legs[b])));
    let Some(first) = first else {
        return;
    };
    let lead = legs[first].at;
    for (k, leg) in legs.iter_mut().enumerate() {
        let close = gap(leg.at, lead).abs() < SETTLE_PAIR;
        if leg.lifted || leg.x.abs() <= SETTLE_SLACK || (k != first && !close) {
            continue;
        }
        let reach = (leg.x.abs() / 0.32).clamp(0.0, 1.0);
        leg.lifted = true;
        leg.settling = true;
        leg.done = 0.0;
        leg.slope = 0.0;
        leg.top = SETTLE_LIFT.0 + (SETTLE_LIFT.1 - SETTLE_LIFT.0) * reach;
        leg.way = -leg.x.signum();
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const STRIDE: f32 = 40.0;
    const DT: f32 = 1.0 / 120.0;
    /// Ce qu'un sabot bouge au plus pendant le pas de la simulation où il se
    /// pose (points, la foulée du pas en faisant 40), au-delà de quatre fois
    /// la hauteur où il était au pas précédent : il arrive presque immobile,
    /// et ne bouge plus ensuite. Un centième de la hauteur de l'animal, en
    /// points.
    const LANDING: f32 = 0.05;
    const U: f32 = 0.45;

    /// Un animal et ses quatre pattes, mené comme le moteur le mène.
    struct Walker {
        legs: [LegCycle; 4],
        phase: f32,
        x: f32,
        start: f32,
        /// Où chaque sabot posé est dans le monde, et le pire qu'il a
        /// glissé ; combien de pattes au plus ont été en l'air ensemble.
        planted: [Option<f32>; 4],
        up: [bool; 4],
        height: [f32; 4],
        worst: f32,
        landing: f32,
        most_up: usize,
        highest: f32,
        /// Les mêmes, pour les seuls pas qui remettent d'aplomb.
        settled_up: usize,
        settle_top: f32,
    }

    impl Walker {
        fn new() -> Self {
            Self {
                legs: [LegCycle::default(); 4],
                phase: 0.0,
                x: 0.0,
                start: 0.0,
                planted: [None; 4],
                up: [false; 4],
                height: [0.0; 4],
                worst: 0.0,
                landing: 0.0,
                most_up: 0,
                highest: 0.0,
                settled_up: 0,
                settle_top: 0.0,
            }
        }

        /// Un pas de la simulation à `speed` (pt/s, négatif : à reculons),
        /// les poids du trot et du galop `(trot, run)`.
        fn tick(&mut self, speed: f32, (trot, run): (f32, f32)) {
            // Comme le moteur : la foulée de l'allure, plus courte au départ.
            let travel = speed * DT;
            self.start = if travel == 0.0 {
                0.0
            } else {
                (self.start + travel.abs() / STRIDE).min(1.0)
            };
            let first = super::super::START_STRIDE;
            let scale = (1.0 + 0.15 * trot * (1.0 - run) + 0.9 * run)
                * (first + (1.0 - first) * smoothstep(0.0, 1.0, self.start));
            self.phase = (self.phase + travel / (STRIDE * scale) * TAU).rem_euclid(TAU);
            self.x += travel;
            advance(
                &mut self.legs,
                &Gaiting {
                    phase: self.phase,
                    travel,
                    speed: speed.abs(),
                    dt: DT,
                    stride: STRIDE,
                    scale,
                    trot: trot * (1.0 - run),
                    run,
                    grounded: true,
                    still: false,
                },
            );
            let (mut up, mut settling) = (0, 0);
            for (k, leg) in self.legs.iter().enumerate() {
                // Le sabot dans le monde : le corps, plus sa place.
                let hoof = self.x + leg.x * STRIDE;
                // Posé à la fin de ce pas de la simulation : le sabot n'a pas
                // bougé depuis le précédent ; s'il vient de se poser, il est
                // arrivé presque immobile.
                if !leg.lifted
                    && let Some(b) = self.planted[k]
                {
                    let moved = (hoof - b).abs();
                    if self.up[k] {
                        // Il arrive : il bougeait encore en l'air au pas
                        // précédent, d'autant moins qu'il était près du sol.
                        let above = self.height[k] * U;
                        self.landing = self.landing.max(moved - 4.0 * above);
                    } else {
                        self.worst = self.worst.max(moved);
                    }
                }
                self.planted[k] = Some(hoof);
                self.up[k] = leg.lifted;
                self.height[k] = leg.raise;
                if leg.lifted {
                    up += 1;
                    self.highest = self.highest.max(leg.raise);
                    if leg.settling {
                        settling += 1;
                        self.settle_top = self.settle_top.max(leg.raise);
                    }
                }
            }
            self.most_up = self.most_up.max(up);
            self.settled_up = self.settled_up.max(settling);
        }

        fn run(&mut self, seconds: f32, speed: f32, weights: (f32, f32)) {
            for _ in 0..(seconds / DT) as usize {
                self.tick(speed, weights);
            }
        }
    }

    #[test]
    fn a_planted_hoof_stays_put_while_the_gait_changes_under_it() {
        // Du pas au galop en une seconde, puis du galop au pas : les poids
        // changent à chaque pas de la simulation, le sabot posé ne bouge pas.
        for (from, to) in [
            ((0.0, 0.0), (1.0, 1.0)),
            ((1.0, 1.0), (0.0, 0.0)),
            ((0.0, 0.0), (1.0, 0.0)),
        ] {
            let mut w = Walker::new();
            w.run(1.0, 150.0, from);
            w.worst = 0.0;
            w.landing = 0.0;
            for i in 0..120 {
                let t = i as f32 / 120.0;
                let weights = (from.0 + (to.0 - from.0) * t, from.1 + (to.1 - from.1) * t);
                w.tick(150.0, weights);
            }
            assert!(w.worst < 1e-3, "{from:?} → {to:?} : {}", w.worst);
            assert!(w.landing < LANDING, "{from:?} → {to:?} : {}", w.landing);
        }
    }

    #[test]
    fn the_legs_catch_up_with_the_new_gait_within_a_few_strides() {
        // Après le passage au galop, chaque patte retrouve son moment dans
        // le galop transverse (à un centième de cycle près).
        let mut w = Walker::new();
        for i in 0..120 {
            let t = i as f32 / 120.0;
            w.tick(150.0, (t, t));
        }
        // Le galop tenu deux secondes de plus : environ cinq foulées.
        w.run(2.0, 150.0, (1.0, 1.0));
        for k in 1..4 {
            let want = (offset(k, 0.0, 1.0) - offset(0, 0.0, 1.0)).rem_euclid(1.0);
            let got = (w.legs[k].at - w.legs[0].at).rem_euclid(1.0);
            let err = ((got - want + 0.5).rem_euclid(1.0) - 0.5).abs();
            assert!(err < 0.01, "patte {k} : {got} au lieu de {want}");
        }
    }

    #[test]
    fn walking_backwards_plays_the_cycle_in_reverse_without_sliding() {
        let mut w = Walker::new();
        w.run(3.0, -30.0, (0.0, 0.0));
        assert!(w.worst < 1e-3, "{}", w.worst);
        assert!(w.landing < LANDING, "{}", w.landing);
        assert!(w.most_up <= 2, "{} pattes en l'air", w.most_up);
    }

    #[test]
    fn a_sheep_that_stops_lands_its_legs_then_steps_them_home_one_at_a_time() {
        for (speed, weights) in [(60.0, (0.0, 0.0)), (110.0, (1.0, 0.0)), (190.0, (1.0, 1.0))] {
            for stop in [1.5, 1.6, 1.7, 1.8, 1.9] {
                let mut w = Walker::new();
                w.run(stop, speed, weights);
                // Il s'arrête net (en un vingtième de seconde, comme le
                // frottement du sol l'arrête) ; les allures retombent.
                let mut v = speed;
                let mut weights = weights;
                w.worst = 0.0;
                w.landing = 0.0;
                w.most_up = 0;
                w.highest = 0.0;
                for _ in 0..240 {
                    v = (v - 1100.0 * DT).max(0.0);
                    weights = (weights.0 * 0.94, weights.1 * 0.95);
                    w.tick(v, weights);
                }
                // Rien n'a glissé ; toutes les pattes sont à leur place.
                assert!(w.worst < 1e-3, "{speed} {stop} : glisse de {}", w.worst);
                assert!(
                    w.landing < LANDING,
                    "{speed} {stop} : se pose à {}",
                    w.landing
                );
                for (k, leg) in w.legs.iter().enumerate() {
                    assert!(!leg.lifted, "{speed} {stop} : patte {k} en l'air");
                    assert!(
                        leg.x.abs() <= SETTLE_SLACK + 1e-6,
                        "{speed} {stop} : patte {k} à {}",
                        leg.x
                    );
                }
                // Pour se remettre d'aplomb, pas plus de deux pattes en l'air
                // ensemble (une paire), et de petits pas.
                assert!(w.settled_up <= 2, "{speed} {stop} : {}", w.settled_up);
                assert!(w.settle_top <= SETTLE_LIFT.1 + 1e-6);
            }
        }
    }

    #[test]
    fn settling_after_a_walk_moves_one_leg_at_a_time_with_a_modest_lift() {
        let mut w = Walker::new();
        w.run(1.7, 60.0, (0.0, 0.0));
        // Arrêté : il ne fait plus que se remettre d'aplomb.
        w.most_up = 0;
        w.highest = 0.0;
        for _ in 0..4 {
            w.tick(0.0, (0.0, 0.0));
        }
        let landed = w.legs.iter().all(|l| !l.lifted || !l.settling);
        assert!(landed);
        w.run(0.3, 0.0, (0.0, 0.0));
        w.most_up = 0;
        w.highest = 0.0;
        w.run(2.0, 0.0, (0.0, 0.0));
        assert!(w.most_up <= 1, "{}", w.most_up);
        assert!(w.highest <= SETTLE_LIFT.1 + 1e-6, "{}", w.highest);
    }

    #[test]
    fn a_trotting_sheep_settles_by_diagonal_pairs() {
        // Au trot, les paires diagonales ont presque le même moment : elles
        // se remettent d'aplomb ensemble.
        let mut together = 0;
        for i in 0..10 {
            let mut w = Walker::new();
            w.run(1.5 + 0.05 * i as f32, 110.0, (1.0, 0.0));
            for _ in 0..240 {
                w.tick(0.0, (1.0, 0.0));
                let up: Vec<usize> = (0..4)
                    .filter(|&k| w.legs[k].lifted && w.legs[k].settling)
                    .collect();
                // Deux à la fois : antérieur de ce côté avec postérieur de
                // l'autre, ou l'inverse.
                if up.len() == 2 {
                    assert!(up == [0, 3] || up == [1, 2], "{up:?}");
                    together += 1;
                }
            }
        }
        assert!(together > 0);
    }

    #[test]
    fn a_square_stance_starts_walking_leg_after_leg_without_sliding() {
        let mut w = Walker::new();
        w.run(0.5, 0.0, (0.0, 0.0));
        // Il part : de 0 à 60 pt/s en un dixième de seconde.
        w.worst = 0.0;
        w.landing = 0.0;
        w.most_up = 0;
        let mut v: f32 = 0.0;
        for _ in 0..240 {
            v = (v + 520.0 * DT).min(60.0);
            w.tick(v, (0.0, 0.0));
        }
        assert!(w.worst < 1e-3, "{}", w.worst);
        assert!(w.landing < LANDING, "{}", w.landing);
        assert!(w.most_up <= 2, "{}", w.most_up);
        // Et il marche : chaque patte a repris son moment.
        w.run(2.0, 60.0, (0.0, 0.0));
        for k in 1..4 {
            let want = (offset(k, 0.0, 0.0) - offset(0, 0.0, 0.0)).rem_euclid(1.0);
            let got = (w.legs[k].at - w.legs[0].at).rem_euclid(1.0);
            let err = ((got - want + 0.5).rem_euclid(1.0) - 0.5).abs();
            assert!(err < 0.02, "patte {k} : {got} au lieu de {want}");
        }
    }
}
