//! Ce qui pend d'une attache et se pose sur l'herbe : la cloche pendue au
//! collier (`bell`). Son anse suit le collier, mais elle pend sous la
//! gravité au lieu de tourner avec l'encolure, se balance avec le mouvement
//! et, quand elle passerait sous le sol, bascule vers l'arrière pour s'y
//! poser, jusqu'à se coucher sur le flanc (voir `docs/animation-animale.md`,
//! § 9, et `docs/rig.md`).
//!
//! [`Pendant`] ne sait rien d'une cloche : il servira aussi à de longues
//! mèches qui pendent sous le cou (la barbe, la bavette), qui tournent
//! encore avec l'encolure.

use std::f64::consts::{FRAC_PI_2, PI, TAU};

use peniko::kurbo::{Affine, BezPath, PathEl, Point, Vec2, flatten};

use crate::animate::{ease, rotate_about};
use crate::rig::{Item, Part, Rig};

/// Couché sur le flanc : basculé d'un quart de tour, son bas vers l'arrière.
pub(crate) const SIDE: f64 = FRAC_PI_2;

/// À cette part de sa longueur au-dessus du sol, l'herbe commence à
/// retenir son balancement.
const NEAR: f64 = 0.3;

/// Posé sur le sol, il bascule d'un radian au plus pendant que son attache
/// descend de cette part de sa longueur. Pendu droit, son bas est presque
/// rond (il ne remonte presque pas quand il penche un peu) : posé net sur
/// le sol, il basculerait d'un coup au premier contact. Il commence donc à
/// basculer un peu avant de toucher, comme poussé par l'herbe (4 cm plus
/// haut pour le redon du Mérinos d'Arles, qui descend de 20 cm sous son
/// anse).
const TIP: f64 = 0.5;

/// Les angles de la table de [`Pendant::ramp`] : un quart de degré.
const STEPS: usize = 360;

/// Ce qui pend d'une attache : sa forme autour de l'attache, préparée une
/// fois au chargement.
#[derive(Clone, Debug)]
pub(crate) struct Pendant {
    /// L'attache, dans le repère du dessin.
    pub pivot: Point,
    /// Depuis l'attache (y vers le bas), les sommets de son enveloppe
    /// convexe qui peuvent être son point le plus bas, de pendu droit à
    /// couché sur le flanc.
    rim: Vec<Vec2>,
    /// Le plus bas qu'il descend sous l'attache, à n'importe quel angle (son
    /// sommet le plus éloigné).
    reach: f64,
    /// Pour chaque angle de la table, de pendu droit (0) à couché sur le
    /// flanc ([`SIDE`]) : jusqu'où il descend encore sous l'attache, à cet
    /// angle ou plus basculé, rendu strictement décroissant (au moins
    /// [`TIP`] fois sa longueur par radian). Posé sur le sol, il prend
    /// l'angle où ce qu'il demande vaut la place qu'il a : l'angle suit
    /// continûment l'attache qui descend, même pour une forme qui, en
    /// basculant, remonterait puis redescendrait (un fond plat, un battant
    /// qui dépasse) ; il n'est jamais sous le sol, au pire un peu au-dessus.
    ramp: Vec<f64>,
}

impl Pendant {
    /// Ce qui pend de `pivot`, d'après les points de son dessin (repère du
    /// dessin). `None` s'il ne dessine rien.
    pub(crate) fn new(pivot: Point, points: &[Point]) -> Option<Self> {
        let around: Vec<Vec2> = convex_hull(points).iter().map(|&p| p - pivot).collect();
        let reach = around.iter().map(|v| v.hypot()).fold(0.0, f64::max);
        if reach <= 1e-9 || !reach.is_finite() {
            return None;
        }
        // Le sommet le plus bas, tous les quarts de degré, de pendu droit à
        // couché sur le flanc : les autres ne touchent jamais le sol.
        let lowest = |angle: f64| {
            let (s, c) = angle.sin_cos();
            around
                .iter()
                .copied()
                .max_by(|p, q| (p.x * s + p.y * c).total_cmp(&(q.x * s + q.y * c)))
        };
        let step = SIDE / STEPS as f64;
        let mut rim: Vec<Vec2> = Vec::new();
        for k in 0..=STEPS + 8 {
            let v = lowest((k as f64 - 4.0) * step)?;
            if !rim.contains(&v) {
                rim.push(v);
            }
        }
        let mut pendant = Self {
            pivot,
            rim,
            reach,
            ramp: vec![0.0; STEPS + 1],
        };
        let climb = TIP * reach * step;
        let mut deepest = f64::NEG_INFINITY;
        for k in (0..=STEPS).rev() {
            deepest = deepest.max(pendant.depth(k as f64 * step));
            pendant.ramp[k] = if k == STEPS {
                deepest
            } else {
                deepest.max(pendant.ramp[k + 1] + climb)
            };
        }
        Some(pendant)
    }

    /// Ce que dessine la partie `part` du rig (et ce qu'elle contient), au
    /// repos, pendu à son pivot.
    pub(crate) fn of(rig: &Rig, part: Part) -> Option<Self> {
        if !rig.has_part(part) {
            return None;
        }
        let mut points = Vec::new();
        let mut inside = Vec::new();
        for item in &rig.items {
            match item {
                Item::BeginPart(p) => inside.push(*p),
                Item::EndPart => {
                    inside.pop();
                }
                Item::Fill {
                    path, transform, ..
                } if inside.contains(&part) => outline(path, *transform, 0.0, &mut points),
                Item::Stroke {
                    path,
                    transform,
                    style,
                    ..
                } if inside.contains(&part) => {
                    let half = 0.5 * style.width * transform.determinant().abs().sqrt();
                    outline(path, *transform, half, &mut points);
                }
                _ => {}
            }
        }
        Self::new(rig.pivot(part), &points)
    }

    /// De combien il descend sous l'attache, penché de `angle` (radians,
    /// positif : son bas vers l'arrière), de pendu droit à couché sur le
    /// flanc.
    pub(crate) fn depth(&self, angle: f64) -> f64 {
        let (s, c) = angle.sin_cos();
        self.rim
            .iter()
            .map(|v| v.x * s + v.y * c)
            .fold(f64::NEG_INFINITY, f64::max)
    }

    /// Où il pend, quand il voudrait pencher de `free` (radians, positif :
    /// son bas vers l'arrière, par rapport à la verticale) et que son attache
    /// est à `room` au-dessus du sol : son angle, et de combien le soulever.
    ///
    /// Loin du sol, il penche comme il veut. Près du sol, l'herbe retient son
    /// balancement : il revient à pendre droit, comme il est dessiné. S'il
    /// passait sous le sol, il bascule vers l'arrière (l'attache avance et
    /// descend avec le cou : son bas traîne en arrière), juste assez pour
    /// s'y poser (voir [`Pendant::ramp`]), jusqu'à se coucher sur le flanc ;
    /// si l'attache est plus bas encore, il est soulevé, couché sur l'herbe.
    /// Angle et soulèvement changent continûment avec `free` et `room` :
    /// jamais de saut d'une image à l'autre. Loin du sol, il n'en coûte
    /// qu'une comparaison ; posé, une recherche dans une table.
    pub(crate) fn settle(&self, free: f64, room: f64) -> (f64, f64) {
        let margin = room - self.reach;
        // Il bascule dès que sa table le demande ; plus haut, quel que soit
        // son angle, il ne peut toucher le sol (toute sa longueur tient sous
        // l'attache), et l'herbe calme son balancement en approchant.
        let onset = (self.ramp[0] - self.reach).max(0.0);
        let near = NEAR * self.reach;
        if margin >= onset + near {
            return (free, 0.0);
        }
        if margin >= onset {
            return (ease(onset, onset + near, margin) * free, 0.0);
        }
        let angle = if room >= self.ramp[0] {
            0.0
        } else if room <= self.ramp[STEPS] {
            SIDE
        } else {
            // La table descend : le dernier angle où elle demande au moins
            // `room`, puis entre lui et le suivant.
            let (mut lo, mut hi) = (0, STEPS);
            while hi - lo > 1 {
                let middle = (lo + hi) / 2;
                if self.ramp[middle] >= room {
                    lo = middle;
                } else {
                    hi = middle;
                }
            }
            let t = (self.ramp[lo] - room) / (self.ramp[lo] - self.ramp[hi]);
            (lo as f64 + t) * SIDE / STEPS as f64
        };
        (angle, (self.depth(angle) - room).max(0.0))
    }

    /// Pendu à `anchor` (là où ce qui le porte met son attache, repère du
    /// dessin), avec le personnage entier `whole` (voir
    /// [`crate::animate::root_transform`] ; le cabré compris) : sa
    /// transformation dans le repère du dessin. Il pend à la verticale de
    /// l'écran, quoi que fasse l'animal (l'élan, le balancement quand on le
    /// tient, le cabré), sauf la culbute `roll`, qui le fait tourner avec
    /// lui ; il penche en plus de `swing` (radians, positif : son bas vers
    /// l'arrière), et se pose sur le sol, la ligne `floor` (y de l'écran,
    /// dans le repère du dessin : celle du point d'appui). Rend aussi de
    /// combien il a tourné dans le repère du dessin.
    pub(crate) fn hang(
        &self,
        anchor: Point,
        whole: Affine,
        roll: f64,
        swing: f64,
        floor: f64,
    ) -> (Affine, f64) {
        let [a, b, c, d, ..] = whole.as_coeffs();
        // Ce que l'animal entier fait pencher la verticale du dessin, et ce
        // qu'en fait la culbute seule (elle tourne en dernier).
        let tilt = b.atan2(d);
        let (sr, cr) = roll.sin_cos();
        let upright = (cr * b - sr * a).atan2(cr * d - sr * c);
        let tumble = (tilt - upright + PI).rem_euclid(TAU) - PI;
        // Les longueurs verticales de l'écran pour celles du dessin.
        let scale = b.hypot(d).max(1e-9);
        let room = (floor - (whole * anchor).y) / scale;
        let (angle, lift) = self.settle(swing + tumble, room);
        let turned = angle - tilt;
        let hung = Affine::translate(anchor - self.pivot) * rotate_about(self.pivot, turned);
        if lift <= 0.0 {
            return (hung, turned);
        }
        // Soulevé droit vers le haut de l'écran.
        let det = a * d - b * c;
        if det.abs() < 1e-12 {
            return (hung, turned);
        }
        let up = -lift * scale;
        (
            Affine::translate((-c * up / det, a * up / det)) * hung,
            turned,
        )
    }
}

/// Les sommets de l'enveloppe convexe de ce que la partie `part` du rig
/// dessine elle-même (sans les parties qu'elle porte), au repos, dans le
/// repère du dessin. Vide si elle ne dessine rien.
pub(crate) fn own_outline(rig: &Rig, part: Part) -> Vec<Point> {
    convex_hull(&own_points(rig, &[part]))
}

/// Les points des tracés que ces parties dessinent elles-mêmes (sans les
/// parties qu'elles portent), au repos, dans le repère du dessin.
pub(crate) fn own_points(rig: &Rig, of: &[Part]) -> Vec<Point> {
    let mut points = Vec::new();
    let mut inside: Vec<Part> = Vec::new();
    for item in &rig.items {
        match item {
            Item::BeginPart(p) => inside.push(*p),
            Item::EndPart => {
                inside.pop();
            }
            Item::Fill {
                path, transform, ..
            } if inside.last().is_some_and(|p| of.contains(p)) => {
                outline(path, *transform, 0.0, &mut points);
            }
            Item::Stroke {
                path,
                transform,
                style,
                ..
            } if inside.last().is_some_and(|p| of.contains(p)) => {
                let half = 0.5 * style.width * transform.determinant().abs().sqrt();
                outline(path, *transform, half, &mut points);
            }
            _ => {}
        }
    }
    points
}

/// Les points d'un tracé (repère du dessin), éloignés de `margin` (un
/// demi-trait).
fn outline(path: &BezPath, transform: Affine, margin: f64, out: &mut Vec<Point>) {
    let mut add = |p: Point| {
        let p = transform * p;
        if margin > 0.0 {
            for k in 0..8 {
                let (s, c) = (f64::from(k) * PI / 4.0).sin_cos();
                out.push(p + Vec2::new(c, s) * margin);
            }
        } else {
            out.push(p);
        }
    };
    flatten(path.elements().iter().copied(), 0.05, |el| match el {
        PathEl::MoveTo(p) | PathEl::LineTo(p) => add(p),
        _ => {}
    });
}

/// L'enveloppe convexe de points (ses sommets, sans points alignés).
pub(crate) fn convex_hull(points: &[Point]) -> Vec<Point> {
    let mut pts: Vec<Point> = points
        .iter()
        .copied()
        .filter(|p| p.x.is_finite() && p.y.is_finite())
        .collect();
    pts.sort_by(|a, b| a.x.total_cmp(&b.x).then(a.y.total_cmp(&b.y)));
    pts.dedup();
    if pts.len() < 3 {
        return pts;
    }
    let cross = |o: Point, a: Point, b: Point| (a - o).cross(b - o);
    let mut hull: Vec<Point> = Vec::with_capacity(2 * pts.len());
    for pass in 0..2 {
        let start = hull.len();
        let walk = |p: Point, hull: &mut Vec<Point>| {
            while hull.len() >= start + 2
                && cross(hull[hull.len() - 2], hull[hull.len() - 1], p) <= 0.0
            {
                hull.pop();
            }
            hull.push(p);
        };
        if pass == 0 {
            for &p in &pts {
                walk(p, &mut hull);
            }
        } else {
            for &p in pts.iter().rev() {
                walk(p, &mut hull);
            }
        }
        hull.pop();
    }
    hull
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Une cloche à fond plat (un trapèze) dont le battant dépasse au
    /// milieu : en basculant, son point le plus bas remonte (le battant),
    /// redescend (le coin avant du fond), puis remonte.
    fn flat_bell() -> Pendant {
        let mut points = vec![
            Point::new(-4.0, 0.0),
            Point::new(4.0, 0.0),
            Point::new(8.0, 23.0),
            Point::new(-8.0, 23.0),
        ];
        for k in 0..16 {
            let (s, c) = (f64::from(k) * TAU / 16.0).sin_cos();
            points.push(Point::new(1.5 * c, 24.0 + 1.5 * s));
        }
        Pendant::new(Point::ZERO, &points).unwrap()
    }

    #[test]
    fn the_convex_hull_keeps_only_the_outline() {
        let hull = convex_hull(&[
            Point::new(0.0, 0.0),
            Point::new(2.0, 0.0),
            Point::new(1.0, 1.0),
            Point::new(2.0, 2.0),
            Point::new(0.0, 2.0),
            Point::new(1.0, 0.0),
        ]);
        assert_eq!(hull.len(), 4, "{hull:?}");
        assert!(!hull.contains(&Point::new(1.0, 1.0)));
    }

    #[test]
    fn far_from_the_ground_a_pendant_swings_freely() {
        let bell = flat_bell();
        for free in [-0.5, 0.0, 0.3] {
            assert_eq!(bell.settle(free, 100.0), (free, 0.0));
        }
        // Le plus bas qu'il descend : le battant, droit sous l'attache.
        assert!((bell.depth(0.0) - 25.5).abs() < 1e-3, "{}", bell.depth(0.0));
    }

    #[test]
    fn a_pendant_settles_continuously_and_never_below_the_ground() {
        let bell = flat_bell();
        for free in [-0.6, 0.0, 0.6] {
            let mut last: Option<(f64, f64)> = None;
            // L'attache descend de bien au-dessus du sol jusqu'au sol.
            for k in 0..=20_000 {
                let room = 40.0 - 40.0 * f64::from(k) / 20_000.0;
                let (angle, lift) = bell.settle(free, room);
                let lowest = if (0.0..=SIDE).contains(&angle) {
                    bell.depth(angle) - lift
                } else {
                    // En balançant, il ne touche rien : toute sa longueur
                    // tient sous l'attache.
                    bell.reach - lift
                };
                assert!(lowest <= room + 1e-6, "{free} {room} : {angle} {lift}");
                assert!((0.0..=SIDE).contains(&angle) || room > bell.reach);
                if let Some((a, l)) = last {
                    // Un pas de 2 millièmes de sa place : il ne saute pas.
                    assert!((angle - a).abs() < 0.01, "{free} {room} : {a} puis {angle}");
                    assert!((lift - l).abs() < 0.01, "{free} {room} : {l} puis {lift}");
                }
                last = Some((angle, lift));
            }
            // L'attache au sol : couché sur le flanc, soulevé sur l'herbe.
            let (angle, lift) = bell.settle(free, 0.0);
            assert_eq!(angle, SIDE);
            assert!((lift - bell.depth(SIDE)).abs() < 1e-9);
        }
    }

    #[test]
    fn a_pendant_touching_the_ground_tips_backward_only() {
        let bell = flat_bell();
        let mut tipped = false;
        for k in 0..=400 {
            let room = 30.0 - 30.0 * f64::from(k) / 400.0;
            let (angle, _) = bell.settle(-0.4, room);
            if room < bell.reach {
                assert!(angle >= 0.0, "{room} : {angle}");
                tipped |= angle > 0.5;
            }
        }
        assert!(tipped);
    }
}
