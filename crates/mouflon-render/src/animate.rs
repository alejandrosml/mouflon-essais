//! Animation procédurale : traduit les signaux de pose du moteur
//! (démarche, écrasement, regard…) en transformations des parties du rig.
//!
//! Toutes les transformations sont exprimées dans le repère du document SVG
//! et pensées pour un personnage tourné vers la droite ; le retournement est
//! appliqué en dernier.

use std::f64::consts::{FRAC_PI_2, PI};

use mouflon_core::{Gesture, PoseSignals, RUN_STRIDE};

use crate::gesture::{Gesturing, ears_turn, gesturing, leg_slot, rearing, with_posture};
use crate::joints::{Chain, HeadIntent, LegDrive, LegRest, NeckDrive, NeckRest};
use peniko::kurbo::{Affine, Line, ParamCurve, PathSeg, Point, Rect, Vec2};

use crate::neck_floor::{NeckFloor, NeckRoot};
use crate::pendant::Pendant;
use crate::rig::{Item, Part, Rig};

/// Transformations d'une image.
#[derive(Clone, Debug)]
pub struct PoseTransforms {
    /// Transformation du personnage entier (écrasement, inclinaison, retournement).
    pub root: Affine,
    parts: [Affine; Part::ALL.len()],
}

impl PoseTransforms {
    #[must_use]
    pub fn part(&self, part: Part) -> Affine {
        self.parts[part.index()]
    }
}

pub(crate) fn about(p: Point, t: Affine) -> Affine {
    Affine::translate(p.to_vec2()) * t * Affine::translate(-p.to_vec2())
}

pub(crate) fn rotate_about(p: Point, angle: f64) -> Affine {
    about(p, Affine::rotate(angle))
}

pub(crate) fn scale_about(p: Point, sx: f64, sy: f64) -> Affine {
    about(p, Affine::scale_non_uniform(sx, sy))
}

/// Lissage de 0 à 1 entre `a` et `b`.
pub(crate) fn ease(a: f64, b: f64, x: f64) -> f64 {
    let t = ((x - a) / (b - a)).clamp(0.0, 1.0);
    t * t * (3.0 - 2.0 * t)
}

/// Un saut sur un rebord (voir `PoseSignals::leap`), en l'air : de combien
/// une patte tourne autour de l'épaule ou de la hanche (radians, positif
/// vers l'arrière) et quelle part de son repli elle fait, selon où il en
/// est de son vol (`flight` : 0 à l'envol, 1 à la réception). Comme le
/// cheval qui saute et la chèvre qui grimpe : les antérieurs se replient
/// serrés dès l'envol ; les postérieurs, qui ont poussé, traînent tendus
/// vers l'arrière, puis se replient sous le ventre ; à la fin du vol, les
/// antérieurs se tendent vers le rebord, un peu en avant, puis reviennent
/// sous l'épaule (la rétraction de la patte avant le poser), et les
/// postérieurs descendent sous le corps : il touche le rebord les quatre
/// pattes tendues sous lui, comme debout, et rien ne bouge plus au sol.
pub(crate) fn leap_leg(front: bool, flight: f64) -> (f64, f64) {
    let unfold = ease(0.72, 0.92, flight);
    let under = ease(0.9, 1.0, flight);
    if front {
        let reach = ease(0.72, 0.86, flight) * (1.0 - under);
        (
            0.9 * (1.0 - ease(0.72, 0.86, flight)) - 0.35 * reach,
            1.0 - unfold,
        )
    } else {
        let pushed = ease(0.06, 0.26, flight);
        let folded = pushed * (1.0 - unfold);
        (0.6 * (1.0 - pushed) - 0.55 * folded, folded)
    }
}

/// Un tour de cycle (radians) : les instants du cycle sont donnés en
/// fractions de la foulée, comptées depuis le poser du postérieur qu'on voit.
const TURN: f64 = 2.0 * PI;

// Les mouvements du corps à chaque allure, en centièmes de la hauteur de
// l'animal, crête à crête (voir `docs/animation-animale.md`).
/// Au pas, le garrot et la croupe montent à mi-appui de leurs pattes
/// (pendule inversé), deux fois par foulée ; la croupe plus que le garrot.
const WITHERS_VAULT: f64 = 1.0;
const CROUP_VAULT: f64 = 2.2;
/// Instants où ils sont au plus haut : mi-appui d'un antérieur, d'un postérieur.
const WITHERS_HIGH: f64 = 0.59;
const CROUP_HIGH: f64 = 0.32;
/// La tête au pas : au plus bas quand le garrot est au plus haut (elle
/// charge les antérieurs).
const HEAD_NOD: f64 = 2.4;
/// Au trot, le corps rebondit comme sur un ressort : au plus bas vers le
/// milieu de l'appui de chaque paire diagonale.
const TROT_BOUNCE: f64 = 2.4;
const TROT_LOW: f64 = 0.2;
/// Au galop, une montée par foulée, au plus haut pendant la suspension
/// groupée ; le corps bascule, le nez plonge quand les antérieurs
/// reçoivent le poids (radians).
const RUN_BOUNCE: f64 = 1.4;
const RUN_HIGH: f64 = 0.19;
const RUN_PITCH: f64 = 0.08;
const RUN_NOSE_DOWN: f64 = 0.82;

/// Les allures mêlées d'un quadrupède : le pas, le trot et le galop, selon
/// les poids que donne le moteur (voir `docs/animation-animale.md`).
#[derive(Clone, Copy, Debug)]
pub(crate) struct Gait {
    pub walk: f64,
    /// Part du trot qui n'est pas encore du galop.
    pub trot: f64,
    pub run: f64,
}

impl Gait {
    pub(crate) fn of(s: &PoseSignals) -> Self {
        if s.gesture == Some(Gesture::Nurse) {
            return Self {
                walk: 0.0,
                trot: 0.0,
                run: 0.0,
            };
        }
        let run = f64::from(s.run);
        Self {
            walk: f64::from(s.walk),
            trot: f64::from(s.trot) * (1.0 - run),
            run,
        }
    }

    /// Une allure se dessine dès que l'animal avance.
    pub(crate) fn moving(&self) -> f64 {
        self.walk.max(self.run)
    }

    /// Part de l'allure qui est du pas (ni trot ni galop).
    fn walking(&self) -> f64 {
        self.walk * (1.0 - self.trot) * (1.0 - self.run)
    }

    /// Part du cycle où une patte est à l'appui : au pas, près des deux
    /// tiers (0,58 à 0,69 chez le mouton) ; au trot, une paire diagonale,
    /// un peu moins de la moitié ; au galop, moins d'un tiers.
    pub(crate) fn duty(&self) -> f64 {
        let d = 0.64 + (0.48 - 0.64) * self.trot;
        d + (0.32 - d) * self.run
    }

    /// Hauteur où monte un sabot en avançant (`u` : centième de la hauteur) :
    /// chez le mouton, 6 à 7 % de la hauteur au garrot, un peu plus au trot ;
    /// bien plus au galop.
    pub(crate) fn lift(&self, u: f64) -> f64 {
        (3.4 + 0.6 * self.trot + 3.5 * self.run) * u * self.moving()
    }

    /// Au pas, de combien montent le garrot et la croupe (en `u`).
    fn vault(&self, phase: f64) -> (f64, f64) {
        let k = self.walking();
        let front = 0.5 * WITHERS_VAULT * (2.0 * (phase - TURN * WITHERS_HIGH)).cos();
        let back = 0.5 * CROUP_VAULT * (2.0 * (phase - TURN * CROUP_HIGH)).cos();
        (k * front, k * back)
    }

    /// Montée et descente du corps (négatif : vers le haut). Au pas, il
    /// passe par-dessus chaque patte d'appui comme un pendule renversé ; au
    /// trot, il descend à chaque appui diagonal et remonte pendant le bref
    /// envol ; au galop, une grande montée par foulée.
    pub(crate) fn bob(&self, phase: f64, u: f64) -> f64 {
        let (front, back) = self.vault(phase);
        let walk = 0.5 * (front + back);
        let trot = self.trot * TROT_BOUNCE * (0.5 - 0.5 * (2.0 * (phase - TURN * TROT_LOW)).cos());
        let run = self.run * RUN_BOUNCE * (0.5 + 0.5 * (phase - TURN * RUN_HIGH).cos());
        -(walk + trot + run) * u
    }

    /// Tangage du corps (positif : l'avant descend) : au pas, la croupe et
    /// le garrot montent tour à tour ; au galop, le corps bascule à chaque
    /// foulée. `length` : longueur du corps.
    pub(crate) fn pitch(&self, phase: f64, u: f64, length: f64) -> f64 {
        let (front, back) = self.vault(phase);
        let walk = ((back - front) * u / length.max(u)).atan();
        walk + self.run * RUN_PITCH * (phase - TURN * RUN_NOSE_DOWN).cos()
    }

    /// La tête au pas, en plus du corps (positif : vers le bas).
    pub(crate) fn head_bob(&self, phase: f64, u: f64) -> f64 {
        self.walking() * 0.5 * HEAD_NOD * (2.0 * (phase - TURN * WITHERS_HIGH)).cos() * u
    }

    /// Hochement de la tête au pas : elle penche quand elle descend.
    pub(crate) fn nod(&self, phase: f64) -> f64 {
        0.04 * self.walking() * (2.0 * (phase - TURN * WITHERS_HIGH)).cos()
    }
}

/// Longueur de la foulée en cours, en fois celle du pas.
pub(crate) fn stride_scale(s: &PoseSignals) -> f64 {
    if s.stride_scale > 0.0 {
        f64::from(s.stride_scale)
    } else {
        1.0 + f64::from(RUN_STRIDE) * f64::from(s.run)
    }
}

/// Décalage de chaque patte dans le cycle (voir [`leg_phase`]).
fn leg_offset(part: Part, gait: &Gait) -> f64 {
    leg_phase(
        matches!(part, Part::LegFrontNear | Part::LegFrontFar),
        matches!(part, Part::LegFrontNear | Part::LegBackNear),
        gait,
    )
}

/// Décalage d'une patte de devant ou de derrière, du côté qu'on voit ou de
/// l'autre, dans le cycle (radians). Au pas, l'ordre du mouton : arrière,
/// avant du même côté un peu plus d'un quart de cycle après (0,27), puis
/// l'autre côté. Au trot, les paires diagonales ensemble, l'arrière posé un
/// rien avant l'avant. Au galop, le galop transverse des moutons : arrière
/// de ce côté, arrière de l'autre, avant de ce côté, avant de l'autre, puis
/// une suspension, pattes groupées. Les valeurs sont choisies pour passer
/// de l'une à l'autre sans qu'une patte fasse un tour de trop.
pub(crate) fn leg_phase(front: bool, near: bool, gait: &Gait) -> f64 {
    let (walk, trot, gallop) = match (front, near) {
        (false, true) => (0.0, 0.0, 0.29),
        (true, true) => (0.27, 0.53, 0.55),
        (false, false) => (0.5, 0.5, 0.45),
        (true, false) => (0.77, 1.03, 0.77),
    };
    let mid = walk + (trot - walk) * gait.trot;
    TURN * (mid + (gallop - mid) * gait.run)
}

/// Une patte dans son cycle (`cycle` en radians) : appui pendant la part
/// `duty` du cycle, le sabot posé pendant que la patte balaie de l'avant
/// vers l'arrière à vitesse constante (il ne glisse pas), puis retour vers
/// l'avant, levée. Rend l'angle (positif : sabot vers l'arrière), la levée
/// (0 à 1) et l'appui (0 à 1, pour garder le sabot au sol).
pub(crate) fn stepping(cycle: f64, duty: f64, amplitude: f64, front: bool) -> (f64, f64, f64) {
    let t = cycle.rem_euclid(2.0 * PI) / (2.0 * PI);
    if t < duty {
        let s = t / duty;
        // Appui : de -A à +A, régulièrement ; à ses bords, il se fond dans le vol.
        let support = (s * (1.0 - s) * 12.0).min(1.0);
        (amplitude * (2.0 * s - 1.0), 0.0, support)
    } else {
        let s = (t - duty) / (1.0 - duty);
        let e = s * s * (3.0 - 2.0 * s);
        // Un antérieur monte au plus haut tôt dans son vol (vers 0,38), un
        // postérieur à mi-vol.
        let s_lift = if front { s.powf(FORE_LIFT_SKEW) } else { s };
        (amplitude * (1.0 - 2.0 * e), (PI * s_lift).sin(), 0.0)
    }
}

/// `ln 0,5 / ln 0,38` : l'antérieur au plus haut à 38 % de son vol.
const FORE_LIFT_SKEW: f64 = 0.716;

/// De combien tourner les pupilles (radians, dans le repère du dessin) pour
/// qu'elles restent à l'horizontale : la pupille du mouton est une fente
/// horizontale, et son œil tourne dans l'orbite quand la tête penche
/// (jusqu'à une cinquantaine de degrés en broutant). `head_angle` : la tête
/// dans le dessin ; s'y ajoutent ce qui penche le personnage entier
/// (l'élan, le balancement quand on le tient, la culbute).
pub(crate) fn level_pupils(s: &PoseSignals, facing: f32, head_angle: f64) -> f64 {
    let f = |v: f32| f64::from(v);
    let side = if facing < 0.0 { -1.0 } else { 1.0 };
    let tilt = head_angle + f(s.lean) * 0.6 + f(s.local_swing(facing)) + side * f(s.roll);
    (-tilt).clamp(-PUPIL_TURN, PUPIL_TURN)
}

/// Jusqu'où l'œil tourne dans son orbite (radians).
const PUPIL_TURN: f64 = 0.9;

/// Le point le plus bas d'une boîte une fois transformée (ses quatre coins).
pub(crate) fn lowest_point(t: &Affine, b: peniko::kurbo::Rect) -> f64 {
    [
        Point::new(b.min_x(), b.min_y()),
        Point::new(b.max_x(), b.min_y()),
        Point::new(b.min_x(), b.max_y()),
        Point::new(b.max_x(), b.max_y()),
    ]
    .into_iter()
    .map(|corner| (*t * corner).y)
    .fold(f64::NEG_INFINITY, f64::max)
}

/// Un morceau de queue : son articulation, et ce qu'il dessine lui-même au
/// repos (sans les morceaux qu'il porte).
pub(crate) type TailPiece = (Point, Option<Rect>);

/// Une queue au repos, préparée une fois : ses morceaux, de la racine au
/// bout, chacun porté par le précédent, et jusqu'où elle peut descendre
/// sous l'articulation de sa racine, quels que soient ses angles (elle ne
/// s'allonge pas en se pliant).
#[derive(Clone, Debug)]
pub(crate) struct TailRest {
    pub pieces: Vec<TailPiece>,
    /// L'articulation de la racine, et la portée : de quoi savoir sans
    /// rien parcourir qu'elle est loin du sol.
    root: Point,
    reach: f64,
}

impl TailRest {
    pub(crate) fn new(pieces: Vec<TailPiece>) -> Option<Self> {
        if pieces.is_empty() {
            return None;
        }
        let mut reach = 0.0;
        for (k, &(joint, drawn)) in pieces.iter().enumerate() {
            let mut far = pieces
                .get(k + 1)
                .map_or(0.0, |next| (next.0 - joint).hypot());
            if let Some(r) = drawn {
                for corner in [
                    Point::new(r.x0, r.y0),
                    Point::new(r.x1, r.y0),
                    Point::new(r.x0, r.y1),
                    Point::new(r.x1, r.y1),
                ] {
                    far = far.max((corner - joint).hypot());
                }
            }
            reach += far;
        }
        Some(Self {
            root: pieces[0].0,
            pieces,
            reach,
        })
    }
}

/// Le plus grand nombre de morceaux d'une queue qui se plient sur le sol
/// (un dessin en a trois au plus ; un squelette peut en avoir davantage).
pub(crate) const TAIL_PIECES: usize = 8;

/// Couché ou assis, le corps descend et une longue queue passerait sous le
/// sol. Chacun de ses morceaux se plie alors vers l'arrière du même angle,
/// le plus petit qui la pose sur `floor` : elle se courbe sur l'herbe, le
/// bout presque à plat, au lieu de s'y enfoncer. `base` : où le moteur met
/// la queue, avant ses angles (un déplacement) ; `angle` : l'angle que le
/// moteur donne au `k`-ième morceau. Rend ce que chaque morceau tourne de
/// plus : rien si elle ne touche pas le sol ; loin du sol, il n'en coûte
/// qu'une comparaison.
///
/// Le bout à plat, une queue très longue sous un corps très bas (une brebis
/// de petite taille, un agneau) toucherait encore : ses morceaux se couchent
/// alors à plat l'un après l'autre, depuis le bout, les premiers se pliant
/// davantage, jusqu'à ce qu'elle repose sur l'herbe.
pub(crate) fn tail_bend(
    base: Affine,
    tail: &TailRest,
    angle: impl Fn(usize) -> f64,
    floor: f64,
) -> [f64; TAIL_PIECES] {
    const NONE: [f64; TAIL_PIECES] = [0.0; TAIL_PIECES];
    if (base * tail.root).y + tail.reach <= floor {
        return NONE;
    }
    let chain = &tail.pieces[..tail.pieces.len().min(TAIL_PIECES)];
    let lowest = |extra: [f64; TAIL_PIECES]| {
        let mut at = base;
        let mut low = f64::NEG_INFINITY;
        for (k, &(joint, drawn)) in chain.iter().enumerate() {
            at *= rotate_about(joint, angle(k) + extra[k]);
            if let Some(drawn) = drawn {
                low = low.max(lowest_point(&at, drawn));
            }
        }
        low
    };
    let start = lowest(NONE);
    if start.is_nan() || start <= floor {
        return NONE;
    }
    // Au plus, le bout se couche à l'horizontale (une queue lancée vers
    // l'avant a plus à se plier).
    let swung: f64 = (0..chain.len()).map(&angle).sum();
    let most = ((FRAC_PI_2 - swung) / chain.len() as f64).max(0.0);
    let even = |bend: f64| std::array::from_fn(|k| if k < chain.len() { bend } else { 0.0 });
    if lowest(even(most)) <= floor {
        let (mut touching, mut clear) = (0.0, most);
        for _ in 0..30 {
            let middle = 0.5 * (touching + clear);
            if lowest(even(middle)) > floor {
                touching = middle;
            } else {
                clear = middle;
            }
        }
        return even(clear);
    }
    // Au-delà : chaque morceau se plie encore, mais aucun ne dépasse
    // l'horizontale (ni ce que le moteur lui avait déjà fait dépasser).
    let further = |bend: f64| {
        let mut extra = NONE;
        let (mut turned, mut plain, mut evenly) = (0.0, 0.0, 0.0);
        for (k, slot) in extra.iter_mut().enumerate().take(chain.len()) {
            plain += angle(k);
            evenly += angle(k) + most;
            let next = (plain + (k + 1) as f64 * bend).min(FRAC_PI_2.max(evenly));
            *slot = next - turned - angle(k);
            turned = next;
        }
        extra
    };
    let flat = (FRAC_PI_2 - angle(0)).max(most);
    if lowest(further(flat)) > floor {
        return further(flat);
    }
    let (mut touching, mut clear) = (most, flat);
    for _ in 0..30 {
        let middle = 0.5 * (touching + clear);
        if lowest(further(middle)) > floor {
            touching = middle;
        } else {
            clear = middle;
        }
    }
    further(clear)
}

/// Le personnage entier, autour de son point d'appui `g` (repère du dessin,
/// `h` : sa hauteur) : écrasement, inclinaison, balancement et étirement
/// quand on le tient par `hold`, retournement, culbute. L'écrasement de
/// l'atterrissage reste mesuré : les pattes s'écartent pour amortir le reste.
pub(crate) fn root_transform(
    s: &PoseSignals,
    facing: f32,
    g: Point,
    h: f64,
    hold: Point,
) -> Affine {
    let f = |v: f32| f64::from(v);
    let (sleep, dangle, breath) = (f(s.sleep), f(s.dangle), f(s.breath));
    let landing = f(s.squash).clamp(-0.15, 0.16);
    let squash = landing + breath * (0.008 * (1.0 - sleep) + 0.022 * sleep);
    let squash_t = scale_about(g, 1.0 + 0.55 * squash, 1.0 - squash);
    let lean_pivot = Point::new(g.x, g.y - h * 0.45);
    let lean_t = rotate_about(lean_pivot, f(s.lean) * 0.6);
    let swing_t = rotate_about(hold, f(s.local_swing(facing)));
    let dangle_t = scale_about(hold, 1.0 - 0.035 * dangle, 1.0 + 0.06 * dangle);
    let fx = {
        let v = f(facing).clamp(-1.0, 1.0);
        // On ne passe jamais par une largeur nulle : le demi-tour reste lisible.
        let sign = if v < 0.0 { -1.0 } else { 1.0 };
        sign * (0.34 + 0.66 * v.abs())
    };
    let facing_t = about(Point::new(g.x, 0.0), Affine::scale_non_uniform(fx, 1.0));
    // Culbute dans le repère de l'écran, autour du milieu du corps (après le retournement).
    let roll_t = rotate_about(Point::new(g.x, g.y - h * 0.5), f(s.roll));
    roll_t * facing_t * swing_t * dangle_t * lean_t * squash_t
}

/// Calcule la pose d'une image. `stride` : distance parcourue pendant un
/// cycle de marche, en unités du dessin (0 : 0,9 fois sa hauteur).
#[must_use]
pub fn pose(rig: &Rig, s: &PoseSignals, facing: f32, stride: f64) -> PoseTransforms {
    let f = |v: f32| f64::from(v);
    let h = rig.bounds.height().max(1.0);
    let u = h / 100.0;
    let g = rig.ground;
    let phase = f(s.gait_phase);
    let gait = Gait::of(s);
    let (sit, sleep, air, dangle) = (f(s.sit), f(s.sleep), f(s.air), f(s.dangle));
    let graze = ease(0.0, 1.0, f(s.graze));
    let rise = f(s.rise);
    let breath = f(s.breath);
    let look = Vec2::new(f(s.look.x), f(s.look.y));
    // Le geste en cours et la posture (humeur, élan, saut d'agneau).
    let act = with_posture(gesturing(s), s);

    // --- Personnage entier.
    let landing = f(s.squash).clamp(-0.15, 0.16);
    let hold = Point::new(rig.pivot(Part::Body).x, rig.bounds.min_y() + h * 0.12);
    let root = root_transform(s, facing, g, h, hold);
    let nursing = s.gesture == Some(Gesture::Nurse);
    let nursing_unroot = if nursing {
        root_transform(s, 1.0, g, h, hold).inverse()
    } else {
        Affine::IDENTITY
    };

    // --- Corps couché : l'avant se plie d'abord (il s'agenouille), puis
    // l'arrière ; il se relève dans l'ordre inverse, l'arrière d'abord.
    let body = rig.body_extent();
    let has_body = rig.has_part(Part::Body);
    // Couché, il repose sur son ventre, ou un peu plus haut, sur la laine
    // du poitrail que l'encolure balaierait sinon sous l'herbe en
    // descendant (voir [`lying_lift`]).
    let clearance = if has_body {
        (g.y - body.max_y() - 0.6 * u - rig.joints.lying_lift).max(0.0)
    } else {
        h * 0.12
    };
    let front_down = ease(0.0, 0.65, sit);
    let back_down = ease(0.35, 1.0, sit);
    let drop_front = clearance * front_down;
    let drop_back = clearance * back_down;
    let half_length = if has_body {
        (body.width() * 0.5).max(u)
    } else {
        h * 0.4
    };
    let center = if has_body { body.center() } else { g };
    let pitch = ((drop_front - drop_back) / (2.0 * half_length)).atan();

    // Le corps monte et descend à chaque appui ; au galop, il se balance
    // aussi d'avant en arrière.
    // Des pattes en deux segments amortissent l'atterrissage : le corps
    // descend, les genoux plient.
    let segmented = rig.joints.segmented;
    let bob = gait.bob(phase, u) + act.body_shift.y * u + absorb(segmented, landing, h);
    let rock = gait.pitch(phase, u, 2.0 * half_length);

    let mut parts = [Affine::IDENTITY; Part::ALL.len()];
    parts[Part::Body.index()] =
        Affine::translate((act.body_shift.x * u, bob + 0.5 * (drop_front + drop_back)))
            * rotate_about(center, pitch + rock + 0.05 * graze + act.body_angle)
            * scale_about(g, 1.0 + 0.04 * sleep, 1.0 - 0.05 * sleep)
            * scale_about(center, 1.0 + act.body_squash, 1.0 - act.body_squash);

    // --- Tête : regard, hochement au pas, sommeil, et broute la tête au sol.
    let head_pivot = rig.pivot(Part::Head);
    let head_bounds = rig.part_bounds(Part::Head);
    let mut head_angle =
        look.y * 0.16 + 0.42 * sleep - 0.06 * air + f(s.lean) * 0.3 + gait.nod(phase)
            - 0.12 * dangle
            - 0.12 * air * ease(0.0, 1.0, rise)
            + 0.1 * air * ease(0.0, 1.0, -rise)
            + act.head_angle;
    let mut head_shift = Vec2::new(
        look.x * u * 1.4 + act.head_shift.x * u + act.body_shift.x * u,
        look.y * u * 1.0
            + bob * 1.15
            + gait.head_bob(phase, u)
            + drop_front
            + sleep * h * 0.06
            + act.head_shift.y * u,
    );
    // Ce que le moteur veut de la tête d'une pièce, avant de la poser au sol
    // (pour brouter, pour dormir) : l'encolure le reprend s'il y en a une.
    let intent = HeadIntent {
        angle: head_angle,
        shift: head_shift,
    };
    let low = f(s.sleep_low);
    if low > 0.0 && rig.has_part(Part::Head) {
        // Endormi la tête au sol, le cou tendu : la tête ne pend plus comme
        // assoupie, le museau va en avant, et elle descend jusqu'à poser le
        // menton.
        let angle = head_angle - 0.42 * sleep + 0.15;
        let turn = rotate_about(head_pivot, angle);
        let lowest = lowest_point(&turn, head_bounds) + head_shift.y;
        let lower = (g.y - u - lowest).max(0.0);
        head_angle += low * (angle - head_angle);
        head_shift += Vec2::new(low * 1.5 * u, low * lower);
    }
    if graze > 0.0 && rig.has_part(Part::Head) {
        // Bouche au ras du sol, juste devant les pattes de devant : la tête
        // pivote vers le bas, et le cou (dans la laine) l'abaisse du reste.
        let angle = 0.85 + 0.05 * f(s.nibble);
        // Le point le plus bas de la tête une fois penchée (coin de sa boîte,
        // côté museau) descend jusqu'au ras du sol.
        let turn = rotate_about(head_pivot, angle);
        let lowest = [
            Point::new(head_bounds.max_x(), head_bounds.max_y()),
            Point::new(head_bounds.max_x(), head_bounds.min_y()),
            Point::new(head_bounds.min_x(), head_bounds.max_y()),
        ]
        .into_iter()
        .map(|corner| (turn * corner).y)
        .fold(f64::NEG_INFINITY, f64::max);
        let lower = (g.y - 1.5 * u - lowest).max(0.0);
        head_angle += graze * angle;
        head_shift += Vec2::new(graze * 1.5 * u, graze * (lower + 0.5 * u * f(s.nibble)));
    }
    // Vue de profil, la tête qui tourne d'un côté à l'autre paraît plus étroite.
    parts[Part::Head.index()] = Affine::translate(head_shift)
        * rotate_about(head_pivot, head_angle)
        * scale_about(head_bounds.center(), 1.0 - act.head_squash, 1.0);
    // La mâchoire s'ouvre autour de sa charnière (elle suit la tête).
    parts[Part::Jaw.index()] = rotate_about(rig.pivot(Part::Jaw), 0.32 * f(s.jaw));

    // Les oreilles tombent vers le bas (et se dressent vers le haut) de
    // quelque côté qu'elles partent de leur articulation.
    let ear_side = |part: Part| {
        if rig.part_bounds(part).center().x < rig.pivot(part).x {
            -1.0
        } else {
            1.0
        }
    };
    // Pendantes au calme et en mangeant ; asymétriques sous la surprise.
    let ear_droop = 0.3 * sleep + 0.15 * dangle + 0.1 * graze + 0.55 * f(s.ears_droop);
    let ear_pose = act.ear + ears_turn(act.ears_back);
    let split = f(s.ear_split);
    parts[Part::EarNear.index()] = rotate_about(
        rig.pivot(Part::EarNear),
        ear_side(Part::EarNear) * (f(s.ear) + 0.05 * breath + ear_droop + ear_pose + split),
    );
    parts[Part::EarFar.index()] = rotate_about(
        rig.pivot(Part::EarFar),
        ear_side(Part::EarFar)
            * (f(s.ear) * 0.8 + 0.04 * breath + ear_droop * 0.8 + ear_pose * 0.9 - split),
    );

    let closed = (f(s.blink) + act.eyes * (1.0 - f(s.blink))).min(1.0);
    let open = (1.0 - 0.94 * closed).max(0.06);
    parts[Part::Eyes.index()] = scale_about(rig.pivot(Part::Eyes), 1.0, open);
    // Les pupilles suivent le regard, et restent à l'horizontale quand la
    // tête penche.
    parts[Part::Pupils.index()] = Affine::translate((look.x * u * 2.6, look.y * u * 2.0))
        * rotate_about(rig.pivot(Part::Pupils), level_pupils(s, facing, head_angle));

    // La queue suit l'arrière du corps ; ses morceaux suivent chacun le
    // précédent, avec un temps de retard. Couché, une longue queue se
    // courbe sur le sol (voir [`tail_bend`]).
    let tail_base = Affine::translate((act.body_shift.x * u, bob + drop_back));
    let mut tail = [
        (Part::Tail, f(s.tail) - 0.2 * dangle + act.tail),
        (Part::TailMid, f(s.tail_mid)),
        (Part::TailTip, f(s.tail_tip)),
    ];
    if let Some(rest) = &rig.joints.tail {
        let bend = tail_bend(tail_base, rest, |k| tail[k].1, g.y - 0.6 * u);
        for ((_, angle), more) in tail.iter_mut().zip(bend) {
            *angle += more;
        }
    }
    let [(_, tail_angle), (_, mid_angle), (_, tip_angle)] = tail;
    parts[Part::Tail.index()] = tail_base * rotate_about(rig.pivot(Part::Tail), tail_angle);
    parts[Part::TailMid.index()] = rotate_about(rig.pivot(Part::TailMid), mid_angle);
    parts[Part::TailTip.index()] = rotate_about(rig.pivot(Part::TailTip), tip_angle);

    // --- Pattes. Le balayage de l'appui couvre exactement la distance que le
    // corps parcourt pendant ce temps : les sabots restent où ils sont posés.
    let stride_walk = if stride > 0.0 { stride } else { h * 0.9 };
    let stride = stride_walk * stride_scale(s);
    let duty = gait.duty();
    let leg_len = {
        let b = rig.part_bounds(Part::LegFrontNear);
        if rig.has_part(Part::LegFrontNear) {
            (g.y - rig.pivot(Part::LegFrontNear).y)
                .max(u * 8.0)
                .max(b.height() * 0.5)
        } else {
            h * 0.3
        }
    };
    let moving = gait.moving();
    let amplitude = (stride * duty / (2.0 * leg_len)).clamp(0.1, 0.75);
    let lift_amount = gait.lift(u);
    // Un agneau qui saute garde les pattes raides.
    let loose = 1.0 - f(s.stiff);
    // Sur un rebord : les pattes repliées presque tout le vol (voir
    // `leap_leg`) ; posé, les sabots restent plantés là où ils touchent.
    let leap = f(s.leap).clamp(0.0, 1.0);
    let leaping = leap * air * loose;
    let tuck = air * ease(0.0, 1.0, rise) * loose * (1.0 - leaping);
    let reach = air * ease(0.0, 1.0, -rise) * loose * (1.0 - leaping);
    let splay = landing.max(0.0) * 3.0 * (1.0 - leap);
    for part in [
        Part::LegFrontNear,
        Part::LegBackFar,
        Part::LegFrontFar,
        Part::LegBackNear,
    ] {
        let front = matches!(part, Part::LegFrontNear | Part::LegFrontFar);
        let near = matches!(part, Part::LegFrontNear | Part::LegBackNear);
        let hip = rig.pivot(part);
        let cycle = phase + leg_offset(part, &gait);
        let (step, raised, support) = stepping(cycle, duty, amplitude, front);
        let mut angle = moving * step;
        // En l'air : pattes repliées en montant, tendues vers le sol en descendant.
        angle += if front {
            -0.15 * air * loose + 1.0 * tuck - 0.3 * reach
        } else {
            0.15 * air * loose + 0.55 * tuck - 0.12 * reach
        } * (1.0 - leaping);
        angle += leaping * leap_leg(front, f(s.leap_flight)).0;
        angle += dangle * (if front { -0.1 } else { 0.12 } + f(s.swing) * 0.4);
        // À l'atterrissage, elles s'écartent ; couché, elles se replient
        // sous le corps (l'avant vers l'arrière, l'arrière vers l'avant).
        angle += if front { -splay } else { splay };
        angle += if front {
            1.3 * front_down
        } else {
            -1.25 * back_down
        };
        // Le geste (gratter le sol, taper du pied, s'étirer…).
        let (gesture_angle, gesture_lift) = act.legs[leg_slot(front, near)];
        angle += gesture_angle;
        let lift = raised * lift_amount + gesture_lift * u;
        let drop = if front { drop_front } else { drop_back };
        // En appui, la patte penchée s'allonge juste assez pour rester au sol.
        let planted = moving * support * (1.0 / (moving * step).cos().max(0.5) - 1.0);
        let stretch = 1.0 + planted + 0.07 * dangle + 0.04 * air * (1.0 - leaping);
        parts[part.index()] = Affine::translate((act.body_shift.x * u, drop - lift))
            * rotate_about(hip, angle)
            * scale_about(hip, 1.0, stretch);
    }

    // --- Les articulations (encolure, bas des pattes, lèvre, mèches de la
    // toison) : un dessin qui n'en a pas bouge exactement comme avant.
    let body_t = parts[Part::Body.index()];
    if let Some(rest) = &rig.joints.neck {
        let drive = NeckDrive {
            base: body_t * rest.base,
            intent,
            alert: (-f(s.ears_back)).clamp(0.0, 1.0) * (1.0 - graze) * (1.0 - sit),
            charge: f(s.head_low).clamp(0.0, 1.0),
            doze: sleep * (1.0 - low),
            graze,
            nibble: f(s.nibble),
            graze_at: Point::new(rig.joints.fore_hoof.x, g.y - 0.3 * u),
            sleep_low: low,
            lying: sit,
            nursing: s.nursing_target.filter(|_| nursing).map(|target| {
                (
                    nursing_unroot * (g + Vec2::new(f(target.x), f(target.y)) * h),
                    f(s.gesture_amount),
                )
            }),
            ground: g.y,
            u,
            squash: act.head_squash,
        };
        let neck = rest.pose(&drive, rig.joints.neck_floor.as_ref());
        parts[Part::Neck.index()] = neck.neck;
        parts[Part::Head.index()] = neck.neck.inverse() * neck.head;
        parts[Part::Pupils.index()] = Affine::translate((look.x * u * 2.6, look.y * u * 2.0))
            * rotate_about(
                rig.pivot(Part::Pupils),
                level_pupils(s, facing, neck.head_angle),
            );
    }
    // Sans pattes articulées, ni cinématique inverse ni compensation des
    // appuis à préparer : les quatre transformations ci-dessus suffisent.
    if segmented {
        let legs = LegDrive {
            gait,
            phase,
            duty,
            stride,
            stride_walk,
            legs: s.legs,
            u,
            ground: g.y,
            air,
            tuck,
            reach,
            leaping,
            flight: f(s.leap_flight),
            loose,
            dangle,
            swing: f(s.swing),
            splay,
            front_down,
            back_down,
            act,
            untilt: if nursing {
                nursing_unroot
            } else {
                untilt(s, g, h)
            },
        };
        for (leg, rest) in Part::LEGS.into_iter().zip(&rig.joints.legs) {
            if let Some(rest) = rest {
                let (upper, lower) = legs.pose(rest, body_t * rest.chain.root);
                parts[leg.index()] = upper;
                if let Some(part) = leg.lower() {
                    parts[part.index()] = lower;
                }
            }
        }
    }
    if rig.has_part(Part::Lip) {
        parts[Part::Lip.index()] = rotate_about(rig.pivot(Part::Lip), -LIP_CURL * act.lip);
    }
    for (k, part) in Part::SWAYS.into_iter().enumerate() {
        if rig.has_part(part) {
            parts[part.index()] = sway_shear(rig.pivot(part), sway(s, &act, facing, k));
        }
    }

    // Les touffes d'encolure (une mèche de la gorge, ou toute une nappe de
    // longue laine : la bavette, les cordes) : leur attache suit le cou,
    // mais leurs fibres pendent à la verticale au lieu de tourner avec lui.
    // Une ligne entière maintenue fixe les aplatirait quand le cou passe à
    // l'horizontale en broutant. Elles ne pendent pas à travers l'herbe :
    // quand l'attache descend (il broute, il dort la tête au sol), leurs
    // fibres s'y tassent.
    for (k, part) in Part::NECK_SWAYS.into_iter().enumerate() {
        if rig.has_part(part) {
            let parent = if rig.has_part(Part::Neck) {
                parts[Part::Neck.index()]
            } else {
                body_t
            };
            let pivot = rig.pivot(part);
            let at = parent * pivot;
            let depth = rig.joints.tuft_depth[k];
            let room = g.y - TUFT_CLEAR * u - at.y;
            let pile = if depth > room {
                (room / depth).max(TUFT_FLAT)
            } else {
                1.0
            };
            // Elle se balance depuis sa ligne d'attache : la coupe d'une
            // mèche coupée, le bord du haut d'une nappe de mèches entières.
            let line = Point::new(pivot.x, pivot.y - rig.joints.tuft_rise[k]);
            let hanging = Affine::translate(at - pivot)
                * scale_about(pivot, 1.0, pile)
                * sway_shear(line, sway(s, &act, facing, k));
            parts[part.index()] = parent.inverse() * hanging;
        }
    }

    // La cloche pendue au collier : son anse suit ce qui la porte (le
    // collier, dans l'encolure), mais elle pend à la verticale de l'écran,
    // se balance comme les mèches du bout de la toison, en plus lourde, et
    // se pose sur l'herbe (voir [`crate::pendant`]).
    let rear = (act.rear > 0.0).then(|| rearing(act.rear, hips_of(rig)));
    if let Some(bell) = &rig.joints.bell {
        let neck = rig.has_part(Part::Neck);
        let holder = Part::Bell.contract_parent(neck);
        let carrier = rig.parent(Part::Bell).or(holder);
        let at = |part: Option<Part>| {
            part.map_or(Affine::IDENTITY, |p| contract_absolute(&parts, p, neck))
        };
        let anchor = at(carrier) * bell.pivot;
        let whole = rear.map_or(root, |rear| root * rear);
        let swing = BELL_SWAY * (f(s.fleece) + f(s.fleece_tip) + act.fleece_tip);
        // Le sol est sous ses sabots. Tenu en l'air, ou pendant une culbute
        // (l'animal tourne tout entier, en vol), il n'y a pas de sol à sa
        // portée : il s'éloigne, en douceur.
        let tumbling = (f(s.roll) + PI).rem_euclid(2.0 * PI) - PI;
        let away = dangle.max(ease(0.1, 0.5, tumbling.abs()));
        let floor = g.y - BELL_CLEAR * u + away * h;
        let (hung, turned) = bell.hang(anchor, whole, f(s.roll), swing, floor);
        parts[Part::Bell.index()] = at(holder).inverse() * hung;
        // Le battant, pendu dans la cloche : il reste à la verticale quand
        // elle penche ou se balance, jusqu'à buter contre sa paroi (couchée
        // sur l'herbe, il repose dessus), et la suit avec un temps de
        // retard : il bat d'un bord à l'autre à chaque pas.
        if rig.has_part(Part::Clapper) {
            let lag = CLAPPER_LAG * (f(s.fleece_tip) + act.fleece_tip);
            let angle = (-turned - lag).clamp(-CLAPPER_REACH, CLAPPER_REACH);
            parts[Part::Clapper.index()] = rotate_about(rig.pivot(Part::Clapper), angle);
        }
    }

    // Endormi la tête au sol, le premier tiers de l'encolure, qui descend
    // avec la tête, ne s'enfonce pas dans l'herbe : l'animal entier remonte
    // de ce qui dépasse, un rien (quelques millimètres chez l'Ouessant ; au
    // plus `ROOT_LIFT` centièmes de sa hauteur).
    let mut root = root;
    if low > 0.0
        && rig.has_part(Part::Neck)
        && let Some(lowest) = rig.joints.neck_root.first_low(&parts[Part::Neck.index()])
    {
        let under = (lowest - (g.y - WOOL_CLEAR * u)).clamp(0.0, ROOT_LIFT * u);
        if under > 0.0 {
            root *= Affine::translate((0.0, -under));
        }
    }

    // Cabré : tout l'animal tourne autour de ses hanches, sauf les pattes
    // de derrière, qui restent où elles sont.
    if let Some(rear) = rear {
        root *= rear;
        let back = rear.inverse();
        for part in [Part::LegBackNear, Part::LegBackFar] {
            parts[part.index()] = back * parts[part.index()];
        }
    }

    // Des parties rangées autrement que le dit le contrat (la tête dans le
    // corps, par exemple) : chacune est ramenée à celle qui la contient.
    if !rig.follows_contract() {
        to_document_nesting(rig, &mut parts);
    }

    PoseTransforms { root, parts }
}

/// Ce qu'on laisse entre l'herbe et la laine que l'encolure dessine contre
/// le poitrail, en centièmes de la hauteur du dessin (le bord anticrénelé
/// de son image).
const WOOL_CLEAR: f64 = 0.3;

/// Jusqu'où le battant s'écarte de l'axe de la cloche avant de buter contre
/// sa paroi (radians), et ce que le retard des mèches du bout lui ajoute.
const CLAPPER_REACH: f64 = 0.33;
const CLAPPER_LAG: f64 = 1.2;

/// Endormi la tête au sol, de combien l'animal remonte au plus pour que le
/// premier tiers de son encolure ne passe pas sous l'herbe (centièmes de sa
/// hauteur).
const ROOT_LIFT: f64 = 1.5;

/// Ce qu'on laisse entre l'herbe et une touffe qui s'y tasse, comme sous
/// l'encolure qui s'y couche (voir [`crate::neck_floor`]).
const TUFT_CLEAR: f64 = 0.5;

/// Une touffe tassée sur l'herbe garde au moins cette part de sa longueur.
const TUFT_FLAT: f64 = 0.12;

/// De combien le corps couché se pose plus haut que sur son ventre, pour que
/// la laine que l'encolure dessine derrière sa base, contre le poitrail, ne
/// passe jamais sous l'herbe : l'encolure tourne
/// d'une pièce autour de sa base, et balaie cette laine vers le sol en
/// descendant (les cordes de la Racka, la toison serrée du Texel, le gros
/// cou d'un agneau sur son petit corps).
///
/// Mesuré une fois, au chargement, sur les poses du mouton couché : éveillé,
/// qui regarde en haut et en bas, assoupi, puis la tête qui descend jusqu'au
/// sol, en respirant. Zéro pour presque tous les dessins. `rig` porte déjà
/// ses articulations ; ce qu'on rend s'ajoute à son `lying_lift`.
pub(crate) fn lying_lift(rig: &Rig) -> f64 {
    if !rig.has_part(Part::Neck) || !rig.has_part(Part::Body) {
        return 0.0;
    }
    let u = rig.bounds.height().max(1.0) / 100.0;
    let lying = PoseSignals {
        sit: 1.0,
        ..PoseSignals::default()
    };
    let mut poses = Vec::with_capacity(40);
    for look in [-1.0, 0.0, 1.0] {
        poses.push(PoseSignals {
            look: mouflon_core::Vec2::new(0.0, look),
            ..lying
        });
    }
    for breath in [-1.0, 1.0] {
        for k in 0..=16 {
            poses.push(PoseSignals {
                sleep: 1.0,
                sleep_low: k as f32 / 16.0,
                breath,
                ..lying
            });
        }
    }
    let mut under = f64::NEG_INFINITY;
    for signals in &poses {
        let posed = pose(rig, signals, 1.0, 0.0);
        let neck = posed.root * posed.part(Part::Neck);
        if let Some(low) = rig.joints.neck_root.chest_low(&neck) {
            under = under.max(low - (rig.ground.y - WOOL_CLEAR * u));
        }
    }
    if under.is_finite() { under } else { 0.0 }
}

/// Une cloche se balance comme les mèches du bout de la toison (elles
/// suivent les premières avec un temps de retard), mais moins loin : elle
/// est plus lourde.
const BELL_SWAY: f64 = 0.7;
/// Elle se pose à tant de centièmes de la hauteur au-dessus du sol (le
/// bord antialiasé de son image).
const BELL_CLEAR: f64 = 0.5;

/// La transformation d'une partie dans le repère du dessin (avant la
/// racine), les parties rangées comme le dit le contrat.
fn contract_absolute(parts: &[Affine; Part::ALL.len()], part: Part, neck: bool) -> Affine {
    let mut t = parts[part.index()];
    let mut up = part.contract_parent(neck);
    while let Some(p) = up {
        t = parts[p.index()] * t;
        up = p.contract_parent(neck);
    }
    t
}

/// Défait l'inclinaison de l'élan et le souffle de l'animal debout (le corps
/// qui s'élargit en respirant), ceux de [`root_transform`], pour un sabot
/// posé au sol : il ne bouge pas quand l'animal respire. En l'air ou tenu,
/// les pattes suivent le corps ; à l'atterrissage, elles s'écartent avec
/// l'écrasement pour l'amortir.
pub(crate) fn untilt(s: &PoseSignals, g: Point, h: f64) -> Affine {
    let f = |v: f32| f64::from(v);
    let grounded = (1.0 - f(s.air)) * (1.0 - f(s.dangle));
    let landing = f(s.squash).clamp(-0.15, 0.16);
    let breathing = f(s.breath) * 0.008 * (1.0 - f(s.sleep));
    // Au bout d'un saut sur un rebord, les sabots restent où ils se sont
    // posés : l'écrasement de la réception ne les écarte pas.
    let spread = landing * (1.0 - f(s.leap).clamp(0.0, 1.0));
    let wide = (1.0 + 0.55 * spread) / (1.0 + 0.55 * (landing + breathing));
    let tall = (1.0 - landing) / (1.0 - landing - breathing);
    let unbreathe = scale_about(
        g,
        1.0 + (wide - 1.0) * grounded,
        1.0 + (tall - 1.0) * grounded,
    );
    unbreathe * rotate_about(Point::new(g.x, g.y - h * 0.45), -f(s.lean) * 0.6 * grounded)
}

/// De combien le corps descend à l'atterrissage, sur des pattes en deux
/// segments qui amortissent (rien sur des pattes d'une pièce : le dessin
/// s'écrase seulement).
pub(crate) fn absorb(segmented: bool, landing: f64, h: f64) -> f64 {
    if segmented {
        ABSORB * h * (landing.max(0.0) / 0.16)
    } else {
        0.0
    }
}

/// À l'atterrissage le plus rude, le corps descend de tant de fois la
/// hauteur de l'animal.
const ABSORB: f64 = 0.06;

/// De combien la lèvre supérieure se retrousse (radians) : le flehmen.
pub(crate) const LIP_CURL: f64 = 0.95;

/// Une mèche penchée de `angle` (positif : son bout vers l'arrière) : elle se
/// cisaille autour de la ligne horizontale de son attache, qui reste fixe. Une
/// mèche attachée sur toute sa largeur (un feston de la toison) ne s'en
/// détache jamais, comme elle le ferait en tournant autour d'un point.
pub(crate) fn sway_shear(pivot: Point, angle: f64) -> Affine {
    let k = -angle.clamp(-0.6, 0.6).tan();
    about(pivot, Affine::new([1.0, 0.0, k, 1.0, 0.0, 0.0]))
}

/// L'angle de la `k`-ième mèche de la toison (0 : les premières, 2 : celles
/// du bout), dans le repère du corps : ce que donne le moteur (elles
/// traînent, se balancent, penchent dans le vent), la secousse d'un geste,
/// et, tenu en l'air, elles pendent vers le sol.
pub(crate) fn sway(s: &PoseSignals, act: &Gesturing, facing: f32, k: usize) -> f64 {
    let f = |v: f32| f64::from(v);
    let w = k as f64 * 0.5;
    let first = f(s.fleece) + act.fleece;
    let tip = f(s.fleece) + f(s.fleece_tip) + act.fleece_tip;
    let hang = -0.7 * f(s.local_swing(facing));
    ((1.0 - w) * first + w * tip) * (1.0 + 0.3 * w) + hang
}

/// Les articulations d'un rig, préparées une fois au chargement.
#[derive(Clone, Debug, Default)]
pub(crate) struct RigJoints {
    /// L'encolure (et la tête qu'elle porte), s'il y en a une.
    pub neck: Option<NeckRest>,
    /// Ce qu'elle dessine au-delà de son premier tiers : endormie la tête au
    /// sol, elle s'y pose sur l'herbe, et la tête s'arrête là.
    pub neck_floor: Option<NeckFloor>,
    /// Ce que l'encolure dessine contre le poitrail (voir [`NeckRoot`]).
    pub neck_root: NeckRoot,
    /// De combien le corps couché se pose plus haut que sur son ventre
    /// (voir [`lying_lift`]).
    pub lying_lift: f64,
    /// Jusqu'où pend chaque touffe d'encolure sous son attache, au repos.
    pub tuft_depth: [f64; 3],
    /// De combien la ligne autour de laquelle elle se balance est au-dessus
    /// de son attache : rien pour une mèche coupée (elle ne quitte pas sa
    /// ligne de coupe) ; pour une nappe de mèches entières, attachée par son
    /// milieu, jusqu'à son bord du haut (`pivot-neck-sway-N-top`).
    pub tuft_rise: [f64; 3],
    /// Les pattes en deux segments, dans l'ordre de [`Part::LEGS`].
    pub legs: [Option<LegRest>; 4],
    /// Le sabot de l'antérieur qu'on voit, au repos.
    pub fore_hoof: Point,
    /// Au moins une patte en deux segments.
    pub segmented: bool,
    /// La queue (`tail`, puis `tail-mid` et `tail-tip` s'il y en a).
    pub tail: Option<TailRest>,
    /// La cloche pendue au collier, s'il y en a une.
    pub bell: Option<Pendant>,
}

impl RigJoints {
    pub(crate) fn of(rig: &Rig) -> Self {
        let segmented = Part::LEGS
            .iter()
            .any(|leg| leg.lower().is_some_and(|lower| rig.has_part(lower)));
        let headroom = if segmented { headrooms(rig) } else { [0.0; 4] };
        let legs: [Option<LegRest>; 4] =
            std::array::from_fn(|i| leg_rest(rig, Part::LEGS[i], headroom[i]));
        let tail = [Part::Tail, Part::TailMid, Part::TailTip]
            .into_iter()
            .take_while(|&part| rig.has_part(part))
            .map(|part| (rig.pivot(part), rig.own_bounds(part)))
            .collect();
        let neck = (rig.has_part(Part::Neck) && rig.has_part(Part::Head)).then(|| neck_rest(rig));
        Self {
            neck,
            neck_floor: neck
                .and_then(|n| NeckFloor::of(rig, n.base, n.poll, crate::joints::SLEEP_STRETCH)),
            neck_root: neck.map_or_else(NeckRoot::default, |n| NeckRoot::of(rig, n.base, n.poll)),
            lying_lift: 0.0,
            tuft_depth: Part::NECK_SWAYS.map(|part| {
                let pivot = rig.pivot(part);
                crate::pendant::own_points(rig, &[part])
                    .iter()
                    .map(|p| p.y - pivot.y)
                    .fold(0.0, f64::max)
            }),
            // Une nappe : depuis son bord du haut, celui qui est donné ou,
            // s'il passe plus haut, celui de son dessin.
            tuft_rise: std::array::from_fn(|k| {
                let part = Part::NECK_SWAYS[k];
                let pivot = rig.pivot(part);
                rig.sheet_tops[k].map_or(0.0, |top| {
                    crate::pendant::own_points(rig, &[part])
                        .iter()
                        .map(|p| pivot.y - p.y)
                        .fold(pivot.y - top.y, f64::max)
                        .max(0.0)
                })
            }),
            segmented: legs.iter().any(Option::is_some),
            legs,
            fore_hoof: fore_hoof(rig),
            tail: TailRest::new(tail),
            bell: Pendant::of(rig, Part::Bell),
        }
    }
}

/// L'encolure au repos : sa base, la nuque, ce qui touche l'herbe.
fn neck_rest(rig: &Rig) -> NeckRest {
    let head = rig.part_bounds(Part::Head);
    NeckRest {
        base: rig.pivot(Part::Neck),
        poll: rig.pivot(Part::Head),
        muzzle: rig.muzzle.unwrap_or_else(|| muzzle_of(head)),
        head,
    }
}

/// Ce qui touche l'herbe, faute de `pivot-muzzle` : près du coin du bas et
/// de l'avant de la tête.
pub(crate) fn muzzle_of(head: Rect) -> Point {
    Point::new(
        head.max_x() - head.width() * 0.08,
        head.max_y() - head.height() * 0.12,
    )
}

/// Le sabot de l'antérieur qu'on voit, au repos.
fn fore_hoof(rig: &Rig) -> Point {
    let part = if rig.has_part(Part::LegFrontNearLower) {
        Part::LegFrontNearLower
    } else {
        Part::LegFrontNear
    };
    if rig.has_part(part) {
        let b = rig.part_bounds(part);
        Point::new(b.center().x, b.max_y())
    } else {
        Point::new(rig.ground.x + rig.bounds.height() * 0.3, rig.ground.y)
    }
}

/// Une patte en deux segments, si le dessin en a une.
fn leg_rest(rig: &Rig, leg: Part, headroom: f64) -> Option<LegRest> {
    let lower = leg.lower()?;
    if !(rig.has_part(leg) && rig.has_part(lower)) {
        return None;
    }
    let b = rig.part_bounds(lower);
    Some(LegRest::new(
        Chain::new(
            rig.pivot(leg),
            rig.pivot(lower),
            Point::new(b.center().x, b.max_y()),
        ),
        (
            matches!(leg, Part::LegFrontNear | Part::LegFrontFar),
            matches!(leg, Part::LegFrontNear | Part::LegBackNear),
        ),
        rig.own_bounds(leg),
        b,
        rig.own_bounds(Part::Body),
        headroom,
        &crate::pendant::own_outline(rig, leg),
    ))
}

/// De combien le haut de chaque patte (dans l'ordre de [`Part::LEGS`]) peut
/// glisser vers le haut sous le corps sans que son bord du haut sorte du
/// dos : de ce bord au contour du corps juste au-dessus (au quart, au milieu
/// et aux trois quarts de sa largeur), moins 2 % de la hauteur du dessin.
/// Rien pour une patte dessinée par-dessus le corps (un tracé du corps vient
/// avant le premier des siens). Un seul passage sur les tracés du corps.
fn headrooms(rig: &Rig) -> [f64; 4] {
    let tops = Part::LEGS.map(|leg| rig.own_bounds(leg));
    let samples: Vec<(usize, f64)> = tops
        .iter()
        .enumerate()
        .filter_map(|(i, top)| top.map(|top| (i, top)))
        .flat_map(|(i, top)| [0.25, 0.5, 0.75].map(|k| (i, top.min_x() + k * top.width())))
        .collect();
    // Le haut du corps à chaque abscisse : le plus haut de ses tracés,
    // surfaces ou traits (le Zwartbles est peint en lignes serrées).
    let mut back = vec![f64::INFINITY; samples.len()];
    let mut over = [false; 4];
    let mut seen = [false; 4];
    let mut body = false;
    let mut inside = Vec::new();
    let (y0, y1) = (rig.bounds.min_y() - 1.0, rig.bounds.max_y() + 1.0);
    for item in &rig.items {
        match item {
            Item::BeginPart(part) => inside.push(*part),
            Item::EndPart => {
                inside.pop();
            }
            Item::Fill {
                path, transform, ..
            }
            | Item::Stroke {
                path, transform, ..
            } => match inside.last() {
                Some(&Part::Body) => {
                    body = true;
                    for seg in path.segments() {
                        let seg = *transform * seg;
                        let (lo, hi) = control_span(seg);
                        for (j, &(_, x)) in samples.iter().enumerate() {
                            if x < lo || x > hi {
                                continue;
                            }
                            for hit in seg.intersect_line(Line::new((x, y0), (x, y1))) {
                                back[j] = back[j].min(seg.eval(hit.segment_t).y);
                            }
                        }
                    }
                }
                Some(&part) => {
                    if let Some(i) = Part::LEGS.iter().position(|&leg| leg == part)
                        && !seen[i]
                    {
                        seen[i] = true;
                        over[i] = body;
                    }
                }
                None => {}
            },
            _ => {}
        }
    }
    let margin = 0.02 * rig.bounds.height();
    let mut out = [0.0; 4];
    for (i, room) in out.iter_mut().enumerate() {
        let Some(top) = tops[i].filter(|_| !over[i]) else {
            continue;
        };
        *room = samples
            .iter()
            .zip(&back)
            .filter(|((leg, _), _)| *leg == i)
            .map(|(_, back)| top.min_y() - back - margin)
            .fold(f64::INFINITY, f64::min)
            .max(0.0);
    }
    out
}

/// L'étendue en x des points de contrôle d'un segment : elle contient le
/// segment.
fn control_span(seg: PathSeg) -> (f64, f64) {
    let xs = match seg {
        PathSeg::Line(l) => [l.p0.x, l.p1.x, l.p1.x, l.p1.x],
        PathSeg::Quad(q) => [q.p0.x, q.p1.x, q.p2.x, q.p2.x],
        PathSeg::Cubic(c) => [c.p0.x, c.p1.x, c.p2.x, c.p3.x],
    };
    (
        xs.iter().copied().fold(f64::INFINITY, f64::min),
        xs.iter().copied().fold(f64::NEG_INFINITY, f64::max),
    )
}

/// Les transformations sont calculées pour les parties rangées comme le dit
/// le contrat ([`Part::contract_parent`]) ; celles du dessin rangées
/// autrement sont ramenées à la partie qui les contient vraiment.
fn to_document_nesting(rig: &Rig, parts: &mut [Affine; Part::ALL.len()]) {
    let neck = rig.has_part(Part::Neck);
    let mut free = [None; Part::ALL.len()];
    fn free_of(
        part: Part,
        neck: bool,
        parts: &[Affine; Part::ALL.len()],
        free: &mut [Option<Affine>; Part::ALL.len()],
    ) -> Affine {
        if let Some(t) = free[part.index()] {
            return t;
        }
        let parent = part
            .contract_parent(neck)
            .map_or(Affine::IDENTITY, |p| free_of(p, neck, parts, free));
        let t = parent * parts[part.index()];
        free[part.index()] = Some(t);
        t
    }
    let mut moved = [(Part::Body, Affine::IDENTITY); Part::ALL.len()];
    for (slot, &part) in moved.iter_mut().zip(&rig.unusual) {
        let own = free_of(part, neck, parts, &mut free);
        let local = match rig.parent(part) {
            Some(p) => free_of(p, neck, parts, &mut free).inverse() * own,
            None => own,
        };
        *slot = (part, local);
    }
    for &(part, local) in &moved[..rig.unusual.len()] {
        parts[part.index()] = local;
    }
}

/// Les hanches : le milieu des articulations des pattes de derrière (à
/// défaut, l'arrière du bas du corps).
fn hips_of(rig: &Rig) -> Point {
    let legs: Vec<Point> = [Part::LegBackNear, Part::LegBackFar]
        .into_iter()
        .filter(|&p| rig.has_part(p))
        .map(|p| rig.pivot(p))
        .collect();
    if legs.is_empty() {
        let b = rig.body_extent();
        Point::new(b.min_x() + b.width() * 0.25, b.max_y())
    } else {
        let n = legs.len() as f64;
        Point::new(
            legs.iter().map(|p| p.x).sum::<f64>() / n,
            legs.iter().map(|p| p.y).sum::<f64>() / n,
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use mouflon_core::Gesture;

    const SVG: &str = r##"<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 100 100">
        <g id="leg-front-near"><rect x="70" y="60" width="8" height="36" fill="#333"/></g>
        <g id="body"><ellipse cx="50" cy="55" rx="35" ry="22" fill="#999"/></g>
        <g id="head"><circle cx="80" cy="35" r="15" fill="#888"/></g>
        <circle id="pivot-ground" cx="50" cy="96" r="1"/>
    </svg>"##;

    #[test]
    fn neutral_pose_keeps_ground_point_fixed() {
        let rig = Rig::parse(SVG.as_bytes()).unwrap();
        let p = pose(&rig, &PoseSignals::default(), 1.0, 0.0);
        let moved = p.root * rig.ground;
        assert!((moved - rig.ground).hypot() < 1e-6);
    }

    #[test]
    fn facing_left_mirrors_around_ground() {
        let rig = Rig::parse(SVG.as_bytes()).unwrap();
        let p = pose(&rig, &PoseSignals::default(), -1.0, 0.0);
        let head = Point::new(80.0, 35.0);
        let mirrored = p.root * head;
        assert!((mirrored.x - (2.0 * rig.ground.x - head.x)).abs() < 1e-6);
    }

    #[test]
    fn lying_down_kneels_in_front_first() {
        let rig = Rig::parse(SVG.as_bytes()).unwrap();
        let signals = PoseSignals {
            sit: 0.4,
            ..PoseSignals::default()
        };
        let p = pose(&rig, &signals, 1.0, 0.0);
        let body = p.part(Part::Body);
        let front = body * Point::new(80.0, 70.0);
        let back = body * Point::new(20.0, 70.0);
        assert!(front.y - 70.0 > back.y - 70.0 + 1.0, "{front:?} {back:?}");
        // Couché tout à fait : le corps touche presque le sol, les pattes repliées dessous.
        let lying = PoseSignals {
            sit: 1.0,
            ..PoseSignals::default()
        };
        let p = pose(&rig, &lying, 1.0, 0.0);
        let bottom = p.part(Part::Body) * Point::new(50.0, 77.0);
        assert!(bottom.y > 90.0 && bottom.y <= 96.0, "{bottom:?}");
        let leg = rig.part_bounds(Part::LegFrontNear);
        let foot = p.part(Part::LegFrontNear) * Point::new(leg.center().x, leg.max_y());
        assert!(foot.y <= 96.0, "sabot sous le sol : {foot:?}");
    }

    #[test]
    fn grazing_brings_the_head_down_to_the_ground() {
        let rig = Rig::parse(SVG.as_bytes()).unwrap();
        let signals = PoseSignals {
            graze: 1.0,
            ..PoseSignals::default()
        };
        let p = pose(&rig, &signals, 1.0, 0.0);
        let head = rig.part_bounds(Part::Head);
        let lowest = [
            Point::new(head.min_x(), head.max_y()),
            Point::new(head.max_x(), head.max_y()),
            Point::new(head.max_x(), head.min_y()),
        ]
        .map(|c| (p.part(Part::Head) * c).y)
        .into_iter()
        .fold(f64::NEG_INFINITY, f64::max);
        assert!(
            lowest > 90.0 && lowest < 96.0,
            "tête au plus bas en {lowest}"
        );
    }

    #[test]
    fn a_rearing_ram_rises_on_its_hind_legs() {
        let svg = r##"<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 100 100">
            <g id="leg-back-near"><rect x="26" y="60" width="8" height="36" fill="#333"/></g>
            <g id="leg-front-near"><rect x="70" y="60" width="8" height="36" fill="#333"/></g>
            <g id="body"><ellipse cx="50" cy="55" rx="35" ry="22" fill="#999"/></g>
            <g id="head"><circle cx="85" cy="35" r="12" fill="#888"/></g>
            <circle id="pivot-leg-back-near" cx="30" cy="64" r="1"/>
            <circle id="pivot-leg-front-near" cx="74" cy="64" r="1"/>
            <circle id="pivot-ground" cx="50" cy="96" r="1"/>
        </svg>"##;
        let rig = Rig::parse(svg.as_bytes()).unwrap();
        let rearing = PoseSignals {
            gesture: Some(Gesture::Rear),
            gesture_amount: 1.0,
            gesture_time: 0.6,
            ..PoseSignals::default()
        };
        let hoof = |s: &PoseSignals, part: Part, x: f64| {
            let p = pose(&rig, s, 1.0, 0.0);
            p.root * p.part(part) * Point::new(x, 96.0)
        };
        let still = PoseSignals::default();
        // Les sabots de derrière restent où ils sont ; ceux de devant montent
        // bien au-dessus du sol, la tête aussi.
        let back = hoof(&rearing, Part::LegBackNear, 30.0) - hoof(&still, Part::LegBackNear, 30.0);
        assert!(back.hypot() < 1e-6, "{back:?}");
        let front = hoof(&rearing, Part::LegFrontNear, 74.0);
        assert!(front.y < 96.0 - 20.0, "{front:?}");
        let p = pose(&rig, &rearing, 1.0, 0.0);
        let head = p.root * p.part(Part::Head) * Point::new(85.0, 35.0);
        assert!(head.y < 35.0 - 15.0, "{head:?}");
        // Retombé, il est de nouveau sur ses quatre pattes.
        let down = PoseSignals {
            gesture_time: 1.3,
            ..rearing
        };
        assert!((hoof(&down, Part::LegFrontNear, 74.0).y - 96.0).abs() < 0.5);
    }

    #[test]
    fn slit_pupils_stay_level_when_the_head_bows() {
        let svg = r##"<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 100 100">
            <g id="body"><ellipse cx="50" cy="55" rx="35" ry="22" fill="#999"/></g>
            <g id="head"><circle cx="80" cy="35" r="15" fill="#888"/>
                <g id="eyes"><circle cx="84" cy="32" r="3" fill="#fff"/>
                    <g id="pupils"><rect x="82" y="31.5" width="4" height="1" fill="#000"/></g>
                </g>
            </g>
            <circle id="pivot-ground" cx="50" cy="96" r="1"/>
        </svg>"##;
        let rig = Rig::parse(svg.as_bytes()).unwrap();
        for (graze, lean, facing) in [
            (0.0, 0.0, 1.0),
            (1.0, 0.0, 1.0),
            (1.0, 0.0, -1.0),
            (0.0, 0.25, 1.0),
        ] {
            let signals = PoseSignals {
                graze,
                lean,
                ..PoseSignals::default()
            };
            let p = pose(&rig, &signals, facing, 0.0);
            let head = p.root * p.part(Part::Head);
            let pupils = head * p.part(Part::Eyes) * p.part(Part::Pupils);
            let tilt = |t: Affine| {
                let [a, b, ..] = t.as_coeffs();
                (b / a).atan()
            };
            // La tête penche pour brouter ; la fente reste à l'horizontale.
            if graze > 0.0 {
                assert!(tilt(head).abs() > 0.5, "{}", tilt(head));
            }
            assert!(
                tilt(pupils).abs() < 0.02,
                "{graze} {facing} : {}",
                tilt(pupils)
            );
        }
        // Au-delà de ce que l'œil peut tourner, la fente penche un peu.
        let upside = PoseSignals {
            roll: 2.0,
            ..PoseSignals::default()
        };
        assert!((level_pupils(&upside, 1.0, 0.0) + PUPIL_TURN).abs() < 1e-9);
    }

    #[test]
    fn a_sleeping_sheep_lays_its_chin_on_the_ground() {
        let rig = Rig::parse(SVG.as_bytes()).unwrap();
        let head = rig.part_bounds(Part::Head);
        let lowest = |sleep_low: f32| {
            let signals = PoseSignals {
                sit: 1.0,
                sleep: 1.0,
                sleep_low,
                ..PoseSignals::default()
            };
            let p = pose(&rig, &signals, 1.0, 0.0);
            lowest_point(&p.part(Part::Head), head)
        };
        let (up, down) = (lowest(0.0), lowest(1.0));
        assert!(down > up + 5.0, "{up} {down}");
        assert!(down > 90.0 && down < 96.0, "menton en {down}");
    }

    #[test]
    fn sheep_walk_in_lateral_sequence_trot_in_diagonals_and_gallop_transversely() {
        let at = |front: bool, near: bool, gait: &Gait| leg_phase(front, near, gait) / TURN;
        // Au pas : l'antérieur du même côté se pose 0,27 de foulée après le
        // postérieur ; l'autre côté, une demi-foulée plus tard.
        let walk = Gait {
            walk: 1.0,
            trot: 0.0,
            run: 0.0,
        };
        assert!((at(true, true, &walk) - at(false, true, &walk) - 0.27).abs() < 1e-9);
        assert!((at(false, false, &walk) - at(false, true, &walk) - 0.5).abs() < 1e-9);
        // Au trot : chaque postérieur avec l'antérieur de l'autre côté, posé
        // un rien avant lui.
        let trot = Gait {
            walk: 1.0,
            trot: 1.0,
            run: 0.0,
        };
        let lag = |a: f64, b: f64| (b - a).rem_euclid(1.0);
        assert!(lag(at(false, true, &trot), at(true, false, &trot)) < 0.05);
        assert!(lag(at(false, false, &trot), at(true, true, &trot)) < 0.05);
        // Au galop transverse : les deux postérieurs, puis les deux
        // antérieurs, dans le même ordre de côté, et une suspension.
        let run = Gait {
            walk: 1.0,
            trot: 0.0,
            run: 1.0,
        };
        let order = [
            at(false, true, &run),
            at(false, false, &run),
            at(true, true, &run),
            at(true, false, &run),
        ];
        assert!(order.windows(2).all(|w| w[0] < w[1]), "{order:?}");
        let last_lift = order[3] + run.duty();
        assert!(order[0] + 1.0 - last_lift > 0.1, "une suspension groupée");
    }

    #[test]
    fn the_walk_rocks_the_body_and_the_trot_bounces_it() {
        let u = 1.0;
        let walk = Gait {
            walk: 1.0,
            trot: 0.0,
            run: 0.0,
        };
        let samples: Vec<f64> = (0..100)
            .map(|i| walk.pitch(TURN * f64::from(i) / 100.0, u, 60.0))
            .collect();
        let swing = samples.iter().copied().fold(f64::MIN, f64::max)
            - samples.iter().copied().fold(f64::MAX, f64::min);
        assert!(swing > 0.01, "la croupe et le garrot montent tour à tour");
        // Au trot, le corps est au plus bas vers le milieu de l'appui d'une
        // paire diagonale, au plus haut entre deux.
        let trot = Gait {
            walk: 1.0,
            trot: 1.0,
            run: 0.0,
        };
        let low = trot.bob(TURN * TROT_LOW, u);
        let high = trot.bob(TURN * (TROT_LOW + 0.25), u);
        assert!(low > high + 2.0, "{low} {high}");
    }

    #[test]
    fn gestures_never_push_a_hoof_into_the_ground() {
        let rig = Rig::parse(SVG.as_bytes()).unwrap();
        let leg = rig.part_bounds(Part::LegFrontNear);
        for gesture in mouflon_core::Gesture::ALL {
            for i in 0..30 {
                let signals = PoseSignals {
                    gesture: Some(gesture),
                    gesture_amount: 1.0,
                    gesture_time: i as f32 * 0.1,
                    ..PoseSignals::default()
                };
                let p = pose(&rig, &signals, 1.0, 0.0);
                let foot = p.part(Part::LegFrontNear) * Point::new(leg.center().x, leg.max_y());
                assert!(foot.y <= 96.0 + 0.5, "{gesture:?} {i} : {foot:?}");
            }
        }
    }

    #[test]
    fn the_jaw_opens_downward_with_the_head() {
        let svg = r##"<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 100 100">
            <g id="body"><ellipse cx="50" cy="55" rx="35" ry="22" fill="#999"/></g>
            <g id="head"><circle cx="80" cy="35" r="15" fill="#888"/>
              <g id="jaw"><rect x="80" y="40" width="14" height="6" fill="#777"/>
                <circle id="pivot-jaw" cx="80" cy="42" r="0.5"/></g></g>
            <circle id="pivot-ground" cx="50" cy="96" r="1"/>
        </svg>"##;
        let rig = Rig::parse(svg.as_bytes()).unwrap();
        let tip = Point::new(94.0, 42.0);
        let closed = pose(&rig, &PoseSignals::default(), 1.0, 0.0).part(Part::Jaw) * tip;
        let open = PoseSignals {
            jaw: 1.0,
            ..PoseSignals::default()
        };
        let opened = pose(&rig, &open, 1.0, 0.0).part(Part::Jaw) * tip;
        assert!(opened.y > closed.y + 3.0, "{closed:?} {opened:?}");
    }

    #[test]
    fn landing_squash_flattens() {
        let rig = Rig::parse(SVG.as_bytes()).unwrap();
        let signals = PoseSignals {
            squash: 0.3,
            ..PoseSignals::default()
        };
        let p = pose(&rig, &signals, 1.0, 0.0);
        let top = p.root * Point::new(50.0, 33.0);
        assert!(top.y > 33.0, "le haut du corps doit descendre");
    }
}
