//! L'encolure qui se couche sur l'herbe : endormi la tête au sol, elle
//! descend avec la tête, et son collier ou sa gorge peuvent toucher l'herbe
//! avant le menton. La tête s'arrête alors là, posée dessus
//! (voir `docs/animation-animale.md`, § 6).
//!
//! [`NeckFloor`] lit une fois, au chargement, ce que dessine l'encolure (ses
//! touffes pendent à part, et se tassent sur l'herbe), et range dans une table, pour chaque direction de
//! l'encolure, jusqu'où ce dessin descend sous la base : à chaque image de
//! ce sommeil, il n'en coûte qu'une recherche dans la table.
//!
//! Seul compte ce qui est au-delà du premier tiers de l'encolure. Derrière
//! sa base, la laine du poitrail que son dessin recouvre pour cacher son
//! raccord ne descend pas avec la tête ; et son premier tiers, contre le
//! poitrail, plie en vrai au lieu de tourner : sa laine se couche sur
//! l'herbe sans arrêter la tête. Le moteur, qui fait tourner l'encolure
//! d'une pièce autour de sa base, devrait sinon lever la tête de plusieurs
//! fois ce qu'il faut pour en écarter la laine.

use std::f64::consts::{FRAC_PI_2, PI};

use peniko::kurbo::{Affine, BezPath, PathEl, Point, Rect, Vec2, flatten};

use crate::pendant::convex_hull;
use crate::rig::{Item, Part, Rig};

/// Ce qu'on laisse entre l'encolure couchée et le sol, en centièmes de la
/// hauteur du dessin (le bord anticrénelé de son image, comme la cloche).
const CLEAR: f64 = 0.5;

/// La part de l'encolure, depuis sa base, qui ne compte pas (voir plus haut).
pub(crate) const ROOT: f64 = 1.0 / 3.0;

/// Les directions de la table : un quart de degré, de l'encolure dressée à
/// la verticale (−π/2) à l'encolure pendue à la verticale (π/2).
const STEPS: usize = 720;

/// La table monte au moins de tant de fois la longueur du dessin par
/// radian : strictement croissante, elle s'inverse sans saut. Là seulement
/// où le dessin remonterait en penchant davantage (l'encolure pendue presque
/// à la verticale, loin de ce que fait le sommeil), elle penche un peu
/// moins qu'elle ne pourrait.
const CLIMB: f64 = 0.05;

/// Ce que l'encolure dessine au-delà de son premier tiers, préparé au
/// chargement.
#[derive(Clone, Debug)]
pub(crate) struct NeckFloor {
    /// Les sommets de l'enveloppe convexe de ce dessin, dans le repère de
    /// l'encolure au repos : le long d'elle (de la base vers la nuque), puis
    /// à sa perpendiculaire (côté gorge).
    hull: Vec<Vec2>,
    /// L'étirement de la table : celui de l'encolure endormie la tête au sol.
    stretch: f64,
    /// Pour chaque direction de la table : jusqu'où le dessin descend sous la
    /// base, étiré de `stretch`, à cette direction ou à une moins penchée.
    ramp: Vec<f64>,
}

/// Ce que l'encolure dessine contre le poitrail : la laine derrière sa base
/// et son premier tiers, qui n'arrêtent pas la tête (voir plus haut). Couché,
/// c'est pourtant de la laine posée sur l'herbe, et l'encolure, qui tourne
/// d'une pièce autour de sa base, la balaie vers le sol en descendant : elle
/// passerait dessous.
///
/// - Derrière la base : le corps se couche assez haut pour qu'elle reste sur
///   l'herbe (voir [`crate::animate::lying_lift`]).
/// - Le premier tiers : plus le corps est haut, plus l'encolure doit pencher
///   pour poser la tête, et plus il descend ; se coucher plus haut n'y
///   ferait rien. Endormi la tête au sol, l'animal entier remonte de ce qui
///   dépasse, un rien (voir [`crate::animate::pose`]).
#[derive(Clone, Debug, Default)]
pub(crate) struct NeckRoot {
    /// L'enveloppe convexe de ce que l'encolure dessine elle-même (sans ses
    /// touffes, qui pendent) derrière sa base, au repos (repère du dessin).
    chest: Vec<Point>,
    /// Celle de ce qu'elle dessine dans son premier tiers.
    first: Vec<Point>,
}

impl NeckRoot {
    pub(crate) fn of(rig: &Rig, base: Point, poll: Point) -> Self {
        let length = (poll - base).hypot();
        if length <= 1e-9 || !length.is_finite() {
            return Self::default();
        }
        let along = (poll - base) / length;
        // Le long de l'encolure, depuis sa base.
        let side = |p: Point| (p - base).dot(along);
        let bands = [(f64::NEG_INFINITY, 0.0), (0.0, ROOT * length)];
        let mut found = [Vec::new(), Vec::new()];
        let mut inside = Vec::new();
        for item in &rig.items {
            let (path, transform, half, filled) = match item {
                Item::BeginPart(p) => {
                    inside.push(*p);
                    continue;
                }
                Item::EndPart => {
                    inside.pop();
                    continue;
                }
                Item::Fill {
                    path, transform, ..
                } => (path, *transform, 0.0, true),
                Item::Stroke {
                    path,
                    transform,
                    style,
                    ..
                } => (
                    path,
                    *transform,
                    0.5 * style.width * transform.determinant().abs().sqrt(),
                    false,
                ),
                _ => continue,
            };
            if inside.last() != Some(&Part::Neck) {
                continue;
            }
            for els in subpaths(path) {
                let (line, closed) = polyline(els);
                let line: Vec<Point> = line.into_iter().map(|p| transform * p).collect();
                let n = line.len();
                // Un tracé rempli se referme ; un trait ouvert s'arrête.
                let segments = if (closed || filled) && n > 2 {
                    n
                } else {
                    n.saturating_sub(1)
                };
                for (&(lo, hi), near) in bands.iter().zip(&mut found) {
                    let mut keep = |p: Point| {
                        if half > 0.0 {
                            for k in 0..8 {
                                let (s, c) = (f64::from(k) * PI / 4.0).sin_cos();
                                near.push(p + Vec2::new(c, s) * half);
                            }
                        } else {
                            near.push(p);
                        }
                    };
                    for (k, &p) in line.iter().enumerate() {
                        let a = side(p);
                        if a >= lo && a < hi {
                            keep(p);
                        }
                        // Là où il traverse un bord de la bande.
                        if k < segments {
                            let q = line[(k + 1) % n];
                            let b = side(q);
                            for edge in [lo, hi] {
                                if edge.is_finite() && (a < edge) != (b < edge) {
                                    keep(p + (q - p) * ((edge - a) / (b - a)));
                                }
                            }
                        }
                    }
                }
            }
        }
        let [chest, first] = found;
        Self {
            chest: convex_hull(&chest),
            first: convex_hull(&first),
        }
    }

    /// Le plus bas de la laine derrière la base quand l'encolure est posée
    /// par `neck` (repère du dessin), si elle en dessine.
    pub(crate) fn chest_low(&self, neck: &Affine) -> Option<f64> {
        lowest(&self.chest, neck)
    }

    /// Le plus bas de son premier tiers.
    pub(crate) fn first_low(&self, neck: &Affine) -> Option<f64> {
        lowest(&self.first, neck)
    }
}

fn lowest(hull: &[Point], neck: &Affine) -> Option<f64> {
    hull.iter()
        .map(|&p| (*neck * p).y)
        .fold(None, |low: Option<f64>, y| {
            Some(low.map_or(y, |l| l.max(y)))
        })
}

/// Une forme du dessin : un tracé aplati en segments (fermé : le dernier
/// point revient au premier), et le demi-trait d'un contour.
#[derive(Clone, Debug)]
pub(crate) struct Shape {
    pub line: Vec<Point>,
    pub closed: bool,
    pub half: f64,
}

/// Les sous-tracés d'un tracé (chacun commence à un `MoveTo`).
fn subpaths(path: &BezPath) -> impl Iterator<Item = &[PathEl]> {
    let els = path.elements();
    let starts: Vec<usize> = (0..els.len())
        .filter(|&k| k == 0 || matches!(els[k], PathEl::MoveTo(_)))
        .collect();
    (0..starts.len()).map(move |i| {
        let end = starts.get(i + 1).copied().unwrap_or(els.len());
        &els[starts[i]..end]
    })
}

/// Un sous-tracé aplati en segments, et s'il est fermé.
fn polyline(els: &[PathEl]) -> (Vec<Point>, bool) {
    let mut line = Vec::new();
    let mut closed = false;
    flatten(els.iter().copied(), 0.05, |el| match el {
        PathEl::MoveTo(p) | PathEl::LineTo(p) => line.push(p),
        PathEl::ClosePath => closed = true,
        _ => {}
    });
    (line, closed)
}

/// La boîte `bounds` est-elle dans le polygone convexe `hull` (sommets dans
/// le sens de [`convex_hull`]), à `margin` au moins de ses bords ?
fn contains(hull: &[Point], bounds: Rect, margin: f64) -> bool {
    if hull.len() < 3 {
        return false;
    }
    let corners = [
        Point::new(bounds.x0, bounds.y0),
        Point::new(bounds.x1, bounds.y0),
        Point::new(bounds.x0, bounds.y1),
        Point::new(bounds.x1, bounds.y1),
    ];
    hull.iter()
        .zip(hull.iter().cycle().skip(1))
        .all(|(&p, &q)| {
            let edge = q - p;
            let length = edge.hypot();
            length <= 0.0
                || corners
                    .iter()
                    .all(|&c| edge.cross(c - p) >= margin * length)
        })
}

impl NeckFloor {
    /// Ce que dessine l'encolure du rig elle-même (pas la tête qu'elle
    /// porte, ni ses touffes, qui pendent et se tassent sur l'herbe, ni la
    /// cloche, qui se pose seule), de sa base `base` à sa nuque `poll`, avec
    /// la table faite pour l'étirement `stretch`. `None` si elle ne dessine
    /// rien au-delà de son premier tiers.
    pub(crate) fn of(rig: &Rig, base: Point, poll: Point, stretch: f64) -> Option<Self> {
        let length = (poll - base).hypot();
        if length <= 1e-9 || !length.is_finite() {
            return None;
        }
        let along = (poll - base) / length;
        let mut drawn = Vec::new();
        let mut inside = Vec::new();
        for item in &rig.items {
            let (path, transform, half, filled) = match item {
                Item::BeginPart(p) => {
                    inside.push(*p);
                    continue;
                }
                Item::EndPart => {
                    inside.pop();
                    continue;
                }
                Item::Fill {
                    path, transform, ..
                } => (path, *transform, 0.0, true),
                Item::Stroke {
                    path,
                    transform,
                    style,
                    ..
                } => (
                    path,
                    *transform,
                    0.5 * style.width * transform.determinant().abs().sqrt(),
                    false,
                ),
                _ => continue,
            };
            if inside.last() != Some(&Part::Neck) {
                continue;
            }
            drawn.push((path, transform, half, filled));
        }
        // La laine compte des milliers de mèches, presque toutes à
        // l'intérieur : une enveloppe des points de leurs tracés (ils sont sur
        // le dessin) les écarte sans les aplatir. Un tracé tient dans
        // l'enveloppe de ses points de contrôle, donc dans sa boîte.
        let cut = ROOT * length;
        let beyond = |p: Point| (p - base).dot(along) >= cut;
        let mut on = Vec::new();
        for (path, to, ..) in &drawn {
            for el in path.elements() {
                if let Some(p) = el.end_point().map(|p| *to * p)
                    && beyond(p)
                {
                    on.push(p);
                }
            }
        }
        let inner = convex_hull(&on);
        let mut shapes = vec![Shape {
            line: inner.clone(),
            closed: true,
            half: 0.0,
        }];
        for (path, to, half, filled) in drawn {
            for els in subpaths(path) {
                let mut bounds: Option<Rect> = None;
                let mut grow = |p: Point| {
                    let p = to * p;
                    bounds = Some(bounds.map_or(Rect::from_points(p, p), |b| b.union_pt(p)));
                };
                for el in els {
                    match *el {
                        PathEl::MoveTo(p) | PathEl::LineTo(p) => grow(p),
                        PathEl::QuadTo(a, b) => {
                            grow(a);
                            grow(b);
                        }
                        PathEl::CurveTo(a, b, c) => {
                            grow(a);
                            grow(b);
                            grow(c);
                        }
                        PathEl::ClosePath => {}
                    }
                }
                // Tout entier dans le premier tiers ou derrière la base, ou
                // dans l'enveloppe : il ne compte pas.
                let ahead = |b: Rect| {
                    [b.x0, b.x1]
                        .iter()
                        .flat_map(|&x| [b.y0, b.y1].map(|y| Point::new(x, y)))
                        .any(|c| (c - base).dot(along) >= cut - half)
                };
                match bounds {
                    Some(bounds) if ahead(bounds) && !contains(&inner, bounds, half) => {}
                    _ => continue,
                }
                let (line, closed) = polyline(els);
                shapes.push(Shape {
                    line: line.into_iter().map(|p| to * p).collect(),
                    closed: closed || filled,
                    half,
                });
            }
        }
        Self::new(base, poll, &shapes, stretch)
    }

    /// D'après les formes du dessin (repère du dessin).
    pub(crate) fn new(base: Point, poll: Point, shapes: &[Shape], stretch: f64) -> Option<Self> {
        let length = (poll - base).hypot();
        if length <= 1e-9 || !length.is_finite() {
            return None;
        }
        let along = (poll - base) / length;
        let across = Vec2::new(-along.y, along.x);
        // Seulement au-delà du premier tiers : chaque forme coupée à la
        // perpendiculaire de l'encolure qui passe par là.
        let cut = ROOT * length;
        let side = |p: Point| (p - base).dot(along) - cut;
        let mut points = Vec::new();
        let mut kept = Vec::new();
        for shape in shapes {
            let n = shape.line.len();
            kept.clear();
            let mut keep = |p: Point| {
                if shape.half > 0.0 {
                    for k in 0..8 {
                        let (s, c) = (f64::from(k) * PI / 4.0).sin_cos();
                        kept.push(p + Vec2::new(c, s) * shape.half);
                    }
                } else {
                    kept.push(p);
                }
            };
            for k in 0..n {
                let p = shape.line[k];
                if side(p) >= 0.0 {
                    keep(p);
                }
                if k + 1 == n && !shape.closed {
                    break;
                }
                let q = shape.line[(k + 1) % n];
                if (side(p) >= 0.0) != (side(q) >= 0.0) {
                    keep(p + (q - p) * (side(p) / (side(p) - side(q))));
                }
            }
            // Chaque forme réduite à son enveloppe : il y en a des milliers
            // (la laine), l'enveloppe de tout se fait sur bien moins de points.
            points.extend(convex_hull(&kept));
        }
        let hull: Vec<Vec2> = convex_hull(&points)
            .iter()
            .map(|&p| Vec2::new((p - base).dot(along), (p - base).dot(across)))
            .collect();
        let reach = hull.iter().map(|v| v.hypot()).fold(0.0, f64::max);
        if reach <= 1e-9 || !reach.is_finite() {
            return None;
        }
        let mut floor = Self {
            hull,
            stretch,
            ramp: Vec::with_capacity(STEPS + 1),
        };
        let step = PI / STEPS as f64;
        let climb = CLIMB * reach * step;
        for k in 0..=STEPS {
            let depth = floor.depth(-FRAC_PI_2 + k as f64 * step, stretch);
            let ramp = match floor.ramp.last() {
                Some(&last) => depth.max(last + climb),
                None => depth,
            };
            floor.ramp.push(ramp);
        }
        Some(floor)
    }

    /// De combien le dessin descend sous la base quand l'encolure pointe dans
    /// la direction `toward` (radians, repère du dessin : 0 vers l'avant,
    /// positif vers le bas), étirée de `stretch`.
    pub(crate) fn depth(&self, toward: f64, stretch: f64) -> f64 {
        let (s, c) = toward.sin_cos();
        self.hull
            .iter()
            .map(|v| stretch * v.x * s + v.y * c)
            .fold(f64::NEG_INFINITY, f64::max)
    }

    /// La direction la plus penchée que peut prendre l'encolure sans que son
    /// dessin descende de plus de `room` sous sa base (au plus π/2 : pendue à
    /// la verticale). Une recherche dans la table ; pour un autre étirement
    /// que le sien (une encolure trop courte pour poser le menton, tendue
    /// davantage), une recherche plus lente.
    pub(crate) fn steepest(&self, room: f64, stretch: f64) -> f64 {
        if (stretch - self.stretch).abs() > 1e-12 {
            return self.steepest_slowly(room, stretch);
        }
        if room >= self.ramp[STEPS] {
            return FRAC_PI_2;
        }
        if room <= self.ramp[0] {
            return -FRAC_PI_2;
        }
        // Le dernier angle où la table tient dans `room`, puis entre lui et
        // le suivant.
        let (mut lo, mut hi) = (0, STEPS);
        while hi - lo > 1 {
            let middle = (lo + hi) / 2;
            if self.ramp[middle] <= room {
                lo = middle;
            } else {
                hi = middle;
            }
        }
        let t = (room - self.ramp[lo]) / (self.ramp[hi] - self.ramp[lo]);
        -FRAC_PI_2 + (lo as f64 + t) * PI / STEPS as f64
    }

    fn steepest_slowly(&self, room: f64, stretch: f64) -> f64 {
        let (mut lo, mut hi) = (-FRAC_PI_2, FRAC_PI_2);
        if self.depth(hi, stretch) <= room {
            return hi;
        }
        if self.depth(lo, stretch) > room {
            return lo;
        }
        for _ in 0..48 {
            let middle = 0.5 * (lo + hi);
            if self.depth(middle, stretch) <= room {
                lo = middle;
            } else {
                hi = middle;
            }
        }
        lo
    }

    /// La direction `toward` de l'encolure (voir [`NeckFloor::depth`]), sa
    /// base à `base_y` et le sol à `ground` (repère du dessin, `u` : un
    /// centième de sa hauteur) : la même, ou moins penchée si le dessin de
    /// l'encolure passait sous l'herbe. Elle ne change pas tant qu'il reste
    /// au-dessus.
    pub(crate) fn limit(&self, toward: f64, base_y: f64, ground: f64, u: f64, stretch: f64) -> f64 {
        toward.min(self.steepest(ground - CLEAR * u - base_y, stretch))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Une encolure dressée à 45°, longue de 40, épaisse de 12, un collier
    /// qui pend de 6 sous sa gorge à mi-longueur, et, derrière sa base, de
    /// la laine du poitrail qui descend plus bas.
    fn collared() -> NeckFloor {
        let base = Point::new(0.0, 0.0);
        let poll = Point::new(40.0 * 0.5f64.sqrt(), -40.0 * 0.5f64.sqrt());
        let along = (poll - base) / 40.0;
        let across = Vec2::new(-along.y, along.x);
        let at = |a: f64, b: f64| base + along * a + across * b;
        let shape = |corners: &[(f64, f64)]| Shape {
            line: corners.iter().map(|&(a, b)| at(a, b)).collect(),
            closed: true,
            half: 0.0,
        };
        let shapes = [
            shape(&[(0.0, -6.0), (40.0, -6.0), (40.0, 6.0), (0.0, 6.0)]),
            shape(&[(18.0, 5.0), (22.0, 5.0), (22.0, 12.0), (18.0, 12.0)]),
            shape(&[(-10.0, 0.0), (2.0, 0.0), (-4.0, 15.0)]),
        ];
        NeckFloor::new(base, poll, &shapes, 1.08).unwrap()
    }

    #[test]
    fn only_the_neck_beyond_its_first_third_counts() {
        let floor = collared();
        assert!(
            floor.hull.iter().all(|v| v.x >= ROOT * 40.0 - 1e-9),
            "{:?}",
            floor.hull
        );
        // À l'horizontale, le collier est le plus bas, à 12 sous l'axe (la
        // laine du poitrail, plus bas, n'y compte pas).
        assert!((floor.depth(0.0, 1.0) - 12.0).abs() < 1e-9);
    }

    #[test]
    fn the_table_gives_the_steepest_direction_that_keeps_the_neck_above_the_grass() {
        let floor = collared();
        for room in [5.0, 12.0, 20.0, 30.0, 42.0] {
            let toward = floor.steepest(room, 1.08);
            assert!(
                floor.depth(toward, 1.08) <= room + 1e-3,
                "{room} : {toward}"
            );
            if toward < FRAC_PI_2 {
                // Un peu plus penchée, elle passerait sous l'herbe.
                assert!(floor.depth(toward + 0.01, 1.08) > room, "{room} : {toward}");
            }
            // L'autre étirement donne la même réponse, plus lentement.
            let slow = floor.steepest(room, 1.08 + 1e-9);
            assert!((slow - toward).abs() < 1e-3, "{room} : {toward} {slow}");
        }
    }

    #[test]
    fn the_steepest_direction_changes_continuously_with_the_room_under_the_base() {
        let floor = collared();
        let mut last = floor.steepest(0.0, 1.08);
        for k in 1..=10_000 {
            let room = 50.0 * f64::from(k) / 10_000.0;
            let toward = floor.steepest(room, 1.08);
            assert!(toward >= last, "{room}");
            assert!(toward - last < 0.01, "{room} : {last} puis {toward}");
            last = toward;
        }
    }

    #[test]
    fn a_neck_far_above_the_grass_keeps_its_direction() {
        let floor = collared();
        for toward in [-1.0, 0.0, 0.7, FRAC_PI_2] {
            assert_eq!(floor.limit(toward, 0.0, 100.0, 1.0, 1.08), toward);
        }
    }
}
