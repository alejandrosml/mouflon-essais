//! Les gestes sur place que le moteur anime de lui-même (s'ébrouer,
//! s'étirer, gratter le sol, taper du pied, renifler, être sonné, se
//! protéger du vent, bâiller, bêler, se gratter), et la posture que donnent
//! l'humeur ou l'élan (oreilles couchées ou dressées, tête basse, pattes
//! raides).
//!
//! Chaque geste dit ce qu'il ajoute à chaque partie du corps ; les rigs et
//! les squelettes l'appliquent de la même façon. Durées, fréquences et
//! postures viennent de ce que les études disent des moutons (voir
//! `docs/animation-animale.md`). Repère du dessin : l'animal tourné vers la
//! droite, y vers le bas ; un angle positif tourne dans le sens des
//! aiguilles d'une montre (la tête qui se penche, le sabot qui part en
//! arrière) ; les longueurs sont en centièmes de la hauteur de l'animal.

use std::f64::consts::{PI, TAU};

use mouflon_core::{Gesture, PoseSignals};
use peniko::kurbo::Vec2;

/// Ce qu'un geste ajoute à la pose.
#[derive(Clone, Copy, Debug, Default)]
pub(crate) struct Gesturing {
    /// Tête : rotation (positive : vers le bas) et déplacement.
    pub head_angle: f64,
    pub head_shift: Vec2,
    /// Corps : rotation (positive : l'avant descend) et déplacement.
    pub body_angle: f64,
    pub body_shift: Vec2,
    /// Oreilles : battement ajouté, et posture (> 0 couchées, < 0 dressées).
    pub ear: f64,
    pub ears_back: f64,
    pub tail: f64,
    /// Yeux : 0 comme d'habitude, 1 fermés.
    pub eyes: f64,
    /// La tête qui tourne d'un côté et de l'autre, vue de profil : sa
    /// largeur diminue de cette part (autour de son milieu).
    pub head_squash: f64,
    /// La toison qui ondule : le corps s'élargit de cette part et s'aplatit
    /// d'autant (négatif : l'inverse).
    pub body_squash: f64,
    /// Pattes (voir [`leg_slot`]) : rotation ajoutée (positive : le sabot
    /// part en arrière) et levée.
    pub legs: [(f64, f64); 4],
    /// Cabré (0 à 1) : tout l'animal tourne autour de ses hanches, l'avant
    /// en l'air, les pattes de derrière restant à leur place (voir
    /// [`rearing`]).
    pub rear: f64,
    /// Pour les pattes en deux segments : de combien le carpe ou le jarret
    /// plie en plus de ce que demande la place du sabot (radians ; négatif :
    /// la patte se tend).
    pub folds: [f64; 4],
    /// La lèvre supérieure retroussée (0 à 1) : le flehmen.
    pub lip: f64,
    /// Les mèches de la toison, en plus de ce que fait le moteur (radians) :
    /// les premières, puis celles du bout.
    pub fleece: f64,
    pub fleece_tip: f64,
}

/// De combien un animal cabré se dresse (radians) : le bélier qui se dresse
/// avant de charger, l'avant à une trentaine de degrés.
pub(crate) const REAR_ANGLE: f64 = 0.62;

/// L'animal cabré autour de ses hanches `hips` (repère du dessin) :
/// l'avant monte, l'arrière reste à sa place.
pub(crate) fn rearing(rear: f64, hips: peniko::kurbo::Point) -> peniko::kurbo::Affine {
    let p = hips.to_vec2();
    peniko::kurbo::Affine::translate(p)
        * peniko::kurbo::Affine::rotate(-REAR_ANGLE * rear)
        * peniko::kurbo::Affine::translate(-p)
}

/// Rang d'une patte dans [`Gesturing::legs`] : devant du côté qu'on voit,
/// devant de l'autre côté, derrière du côté qu'on voit, derrière de l'autre.
pub(crate) fn leg_slot(front: bool, near: bool) -> usize {
    match (front, near) {
        (true, true) => 0,
        (true, false) => 1,
        (false, true) => 2,
        (false, false) => 3,
    }
}

/// Monte de 0 à 1 entre `a` et `b`, y reste, redescend entre `c` et `d`.
fn bump(a: f64, b: f64, c: f64, d: f64, t: f64) -> f64 {
    smooth(a, b, t) * (1.0 - smooth(c, d, t))
}

fn smooth(a: f64, b: f64, x: f64) -> f64 {
    let t = ((x - a) / (b - a)).clamp(0.0, 1.0);
    t * t * (3.0 - 2.0 * t)
}

/// Un petit pas d'une patte qui change de place pendant un geste : la
/// `k`-ième de `n` pattes qui se suivent va à sa place pendant sa part de la
/// montée de `e` (de 0 à 1), le sabot levé ; avant et après, il est posé.
/// Rend où elle en est (de 0 à 1) et la levée de son sabot (de 0 à 1) : un
/// sabot ne bouge que levé, il ne glisse pas sur l'herbe.
fn step(e: f64, k: usize, n: usize) -> (f64, f64) {
    let s = smooth(k as f64 / n as f64, (k + 1) as f64 / n as f64, e);
    // Arrivé (ou pas encore parti), le sabot est posé : exactement zéro.
    let up = if s > 0.0 && s < 1.0 {
        (PI * s).sin()
    } else {
        0.0
    };
    (s, up)
}

/// L'ordre où les quatre pattes font chacune leur petit pas pour s'écarter
/// (leurs rangs dans [`Gesturing::legs`]) : en diagonale, comme au pas.
const STEP_ORDER: [usize; 4] = [0, 3, 1, 2];

/// Les quatre pattes écartées de `angle` (les antérieurs vers l'avant, les
/// postérieurs vers l'arrière), chacune par un petit pas haut de `lift`
/// (centièmes de la hauteur), à mesure que `e` monte de 0 à 1 ; elles
/// reviennent de même quand il retombe.
fn braced(e: f64, angle: f64, lift: f64) -> [(f64, f64); 4] {
    let mut legs = [(0.0, 0.0); 4];
    for (k, &slot) in STEP_ORDER.iter().enumerate() {
        let (at, up) = step(e, k, 4);
        let way = if slot < 2 { -1.0 } else { 1.0 };
        legs[slot] = (way * angle * at, lift * up);
    }
    legs
}

/// Où en est un geste qui déplace les pattes : il s'installe en `settle`
/// secondes (le temps de faire ses petits pas), et se défait avec l'ampleur
/// `a` du geste, qui retombe à la fin.
fn settled(a: f64, t: f64, settle: f64) -> f64 {
    smooth(0.0, settle, t).min(if t > 0.5 * settle { a } else { 1.0 })
}

/// Fréquence d'une secousse d'ébrouement (Hz) : un mouton de 55 kg secoue
/// sa toison à 6,5 Hz, un peu plus vite que la loi des mammifères ne le
/// voudrait pour sa taille (4,9 Hz).
const SHAKE_HZ: f64 = 6.0;

/// Où commence le cycle de la patte qui gratte le sol : là où le sabot, qui
/// racle vers l'arrière, passe par sa place au repos.
const PAW_START: f64 = 0.6968;

/// Ce que le geste en cours ajoute à la pose, à son ampleur du moment.
pub(crate) fn gesturing(s: &PoseSignals) -> Gesturing {
    let mut g = Gesturing::default();
    let Some(gesture) = s.gesture else {
        return g;
    };
    let a = f64::from(s.gesture_amount);
    if a <= 0.0 {
        return g;
    }
    let t = f64::from(s.gesture_time);
    match gesture {
        Gesture::Shake => {
            // La secousse part de la tête et file jusqu'à la queue : chaque
            // partie suit avec un peu de retard, la toison ondule. Vue de
            // profil, la tête qui tourne d'un côté à l'autre paraît plus
            // étroite à chaque aller et retour. Elle s'emballe, puis
            // s'amortit (l'enveloppe du geste).
            let w = TAU * SHAKE_HZ;
            let swell = a * (0.55 + 0.45 * smooth(0.0, 0.35, t));
            g.head_angle = 0.26 * swell * (w * t).sin();
            g.head_shift = Vec2::new(
                0.8 * swell * (w * t).cos(),
                -0.9 * swell * (w * t).sin().abs(),
            );
            g.head_squash = 0.2 * swell * (w * t).sin().abs();
            g.body_angle = 0.05 * swell * (w * t - 0.9).sin();
            g.body_shift = Vec2::new(1.2 * swell * (w * t - 0.9).sin(), -0.8 * swell);
            g.body_squash = 0.05 * swell * (w * t - 1.3).sin();
            g.ear = 1.1 * swell * (w * t - 0.4).sin();
            g.tail = 0.9 * swell * (w * t - 1.8).sin();
            // Les mèches suivent la secousse, avec un temps de retard.
            g.fleece = 0.45 * swell * (w * t - 1.6).sin();
            g.fleece_tip = 0.45 * swell * (w * t - 2.5).sin();
            g.eyes = 0.75 * a;
            // Pattes un peu écartées pour tenir debout : quatre petits pas
            // avant la secousse, quatre pour revenir.
            g.legs = braced(settled(a, t, 0.3), 0.1, 1.6);
        }
        Gesture::Stretch => {
            // D'abord le cou tendu vers l'avant et le bas, l'avant du corps
            // incliné, les pattes de devant poussées en avant ; puis une
            // patte arrière tendue loin derrière.
            let neck = a * bump(0.0, 0.45, 0.95, 1.35, t);
            let hind = a * bump(1.0, 1.4, 1.9, 2.3, t);
            g.head_angle = 0.34 * neck - 0.12 * hind;
            g.head_shift = Vec2::new(5.0 * neck, 3.0 * neck);
            g.body_angle = 0.12 * neck - 0.04 * hind;
            g.body_shift = Vec2::new(1.2 * neck, 1.4 * neck);
            g.body_squash = 0.03 * neck - 0.02 * hind;
            g.eyes = 0.6 * neck;
            g.ears_back = 0.5 * neck;
            // Les antérieurs avancent l'un après l'autre, d'un pas chacun,
            // et reviennent de même ; l'autre postérieur ne bouge pas.
            let (near, near_up) = step(neck, 0, 2);
            let (far, far_up) = step(neck, 1, 2);
            g.legs[leg_slot(true, true)] = (-0.45 * near, 3.0 * near_up);
            g.legs[leg_slot(true, false)] = (-0.34 * far, 3.0 * far_up);
            g.legs[leg_slot(false, true)] = (1.05 * hind, 4.0 * hind);
            // La patte arrière tendue loin derrière, le jarret ouvert.
            g.folds[leg_slot(false, true)] = -0.9 * hind;
            g.tail = -0.3 * hind;
        }
        Gesture::Paw => {
            // Une patte de devant gratte le sol : levée et lancée en avant,
            // puis tirée en arrière, le sabot raclant le sol ; tête basse.
            // Le cycle part du sabot à sa place, au milieu d'un raclement :
            // il racle d'abord vers l'arrière, se lève, se lance en avant,
            // racle encore.
            let p = (t / 0.55 + PAW_START).fract();
            let (angle, lift, fold) = if p < 0.35 {
                let s = p / 0.35;
                let up = (PI * s).sin();
                (0.45 - 1.0 * smooth(0.0, 1.0, s), 6.0 * up, 0.8 * up)
            } else {
                let s = (p - 0.35) / 0.65;
                (-0.55 + 1.0 * smooth(0.0, 1.0, s), 0.0, 0.0)
            };
            // En finissant, le sabot rejoint sa place d'un petit pas, levé
            // d'autant plus haut qu'il en est loin : il n'y glisse pas.
            let ending = if t > 0.12 { 1.0 - a } else { 0.0 };
            let home = 8.0 * ending * (1.0 - ending) * (angle.abs() / 0.55).min(1.0);
            g.legs[leg_slot(true, true)] = (angle * a, lift * a + home);
            // Levé, le carpe se plie ; il racle le sol la patte tendue.
            g.folds[leg_slot(true, true)] = fold * a;
            g.head_angle = 0.38 * a;
            g.head_shift = Vec2::new(0.8 * a, 2.0 * a);
            g.ears_back = 0.45 * a;
            g.body_angle = 0.03 * a;
            // Le raclement secoue un peu l'avant du corps.
            g.body_shift = Vec2::new(0.0, 0.4 * a * (1.0 - smooth(0.0, 0.2, p)));
        }
        Gesture::Stamp => {
            // Tête haute, oreilles dressées : une patte de devant monte
            // lentement et s'abat d'un coup.
            let p = (t / 0.45).fract();
            let lift = 7.0 * smooth(0.0, 0.5, p) * (1.0 - smooth(0.58, 0.64, p));
            g.legs[leg_slot(true, true)] = (-0.3 * a * lift / 7.0, lift * a);
            g.folds[leg_slot(true, true)] = 0.7 * a * lift / 7.0;
            g.head_angle = -0.2 * a;
            g.head_shift = Vec2::new(0.8 * a, -1.4 * a);
            g.ears_back = -0.8 * a;
            // Le coup ébranle le corps et la tête.
            let hit = bump(0.58, 0.61, 0.64, 0.8, p);
            g.body_shift = Vec2::new(0.0, 0.9 * hit * a);
            g.body_squash = 0.025 * hit * a;
            g.head_angle += 0.06 * hit * a;
        }
        Gesture::Sniff => {
            // Le museau tendu vers l'autre, de petits coups de nez rapides.
            g.head_angle = 0.2 * a + 0.04 * a * (t * TAU * 5.0).sin();
            g.head_shift = Vec2::new(4.5 * a, 1.6 * a);
            g.ears_back = -0.5 * a;
            g.body_shift = Vec2::new(1.0 * a, 0.0);
            g.body_angle = 0.02 * a;
        }
        Gesture::Daze => {
            // Sonné : la tête décrit de petits cercles, le corps oscille,
            // les yeux mi-clos, les oreilles pendantes.
            let w = TAU * 0.7;
            g.head_angle = 0.2 * a * (w * t).sin() + 0.08 * a;
            g.head_shift = Vec2::new(1.8 * a * (w * t).cos(), 1.2 * a * (w * t).sin() + 0.8 * a);
            g.body_angle = 0.04 * a * (w * t + 0.8).sin();
            g.body_shift = Vec2::new(1.0 * a * (w * t + 0.8).sin(), 0.0);
            g.eyes = 0.6 * a;
            g.ear = 0.35 * a;
            g.ears_back = 0.2 * a;
            g.tail = 0.3 * a;
        }
        Gesture::Brace => {
            // Contre le vent : tête basse rentrée, oreilles couchées, le
            // corps tassé sur ses pattes écartées, la queue serrée.
            g.head_angle = 0.45 * a;
            g.head_shift = Vec2::new(-1.5 * a, 2.5 * a);
            g.ears_back = 1.0 * a;
            g.body_shift = Vec2::new(0.0, 1.8 * a);
            g.body_squash = 0.025 * a;
            g.eyes = 0.45 * a;
            g.tail = 0.4 * a;
            // Quatre petits pas pour s'écarter, quatre pour revenir.
            g.legs = braced(settled(a, t, 0.4), 0.16, 1.6);
        }
        Gesture::Yawn => {
            // La tête se lève, les yeux se ferment pendant que la bouche
            // s'ouvre grand (la mâchoire est menée par le moteur).
            let open = a * smooth(0.15, 0.7, t);
            g.head_angle = -0.38 * open;
            g.head_shift = Vec2::new(1.2 * open, -1.6 * open);
            g.eyes = 0.95 * open;
            g.ears_back = 0.35 * open;
        }
        Gesture::Rear => {
            // Il se dresse en un tiers de seconde, les pattes de devant
            // repliées, le menton rentré, les oreilles couchées ; tient un
            // instant en pédalant un peu de l'avant ; puis retombe d'un coup,
            // tête la première.
            let up = a * bump(0.0, 0.35, 0.85, 1.1, t);
            let paddle = up * (TAU * 2.4 * t).sin();
            g.rear = up;
            g.head_angle = 0.3 * up + 0.18 * a * bump(1.0, 1.1, 1.15, 1.3, t);
            g.ears_back = 0.8 * up;
            g.legs = [
                (1.15 * up + 0.2 * paddle, 0.0),
                (0.95 * up - 0.2 * paddle, 0.0),
                (0.0, 0.0),
                (0.0, 0.0),
            ];
            // Les antérieurs repliés : le carpe plié, le canon contre l'avant-bras.
            g.folds[leg_slot(true, true)] = 1.5 * up + 0.3 * paddle;
            g.folds[leg_slot(true, false)] = 1.3 * up - 0.3 * paddle;
            g.tail = -0.3 * up;
        }
        Gesture::Invite => {
            // L'invitation au jeu des agneaux : la tête et l'avant baissés,
            // les pattes de devant avancées et écartées, les oreilles en
            // avant, il fixe l'autre en sautillant sur place.
            let bow = a * smooth(0.0, 0.25, t);
            let hop = (TAU * 2.2 * t).sin().abs();
            g.head_angle = 0.32 * bow;
            g.head_shift = Vec2::new(2.0 * bow, 4.0 * bow);
            g.body_angle = 0.1 * bow;
            g.body_shift = Vec2::new(0.0, -2.5 * bow * hop);
            g.ears_back = -0.6 * bow;
            // Les antérieurs avancent d'un pas chacun ; les postérieurs
            // restent où ils sont.
            let (near, near_up) = step(bow, 0, 2);
            let (far, far_up) = step(bow, 1, 2);
            g.legs = [
                (-0.35 * near, 2.5 * near_up),
                (-0.28 * far, 2.5 * far_up),
                (0.0, 0.0),
                (0.0, 0.0),
            ];
            g.tail = 0.4 * bow * (TAU * 3.0 * t).sin();
        }
        Gesture::Scratch => {
            // Un sabot arrière remonte gratter le ventre, à petits coups
            // rapides ; la tête se tourne vers lui, les yeux mi-clos ; sur
            // trois pattes, l'arrière s'abaisse un peu.
            let reach = a * smooth(0.0, 0.3, t);
            let stroke = (t * TAU * 4.0).sin();
            g.legs[leg_slot(false, true)] = (
                -0.62 * reach + 0.2 * reach * stroke,
                6.0 * reach + 1.8 * reach * stroke,
            );
            g.folds[leg_slot(false, true)] = 0.5 * reach;
            g.head_angle = 0.22 * reach;
            g.head_shift = Vec2::new(-1.2 * reach, 1.8 * reach);
            g.body_angle = -0.02 * reach;
            g.body_shift = Vec2::new(0.0, 0.6 * reach);
            g.eyes = 0.55 * reach;
            g.ear = 0.12 * reach * stroke;
            g.tail = 0.25 * reach * stroke;
        }
        Gesture::Bleat => {
            // Tête tendue vers l'avant et levée, un léger tremblement.
            g.head_angle = -0.24 * a + 0.02 * a * (t * TAU * 5.5).sin();
            g.head_shift = Vec2::new(1.4 * a, -a);
            g.ears_back = 0.15 * a;
            g.eyes = 0.2 * a;
        }
        Gesture::Flehmen => {
            // L'encolure se tend vers l'avant et le haut, la tête se relève
            // (le nez au ciel), puis la lèvre supérieure se retrousse ; les
            // yeux mi-clos, il hume, la tête qui balance à peine.
            let up = a * smooth(0.0, 0.7, t);
            let curl = a * smooth(0.35, 1.0, t);
            g.head_angle = -FLEHMEN_HEAD * up + 0.03 * up * (t * TAU * 0.4).sin();
            g.head_shift = Vec2::new(3.0 * up, -4.0 * up);
            g.lip = curl;
            g.eyes = 0.5 * up;
            g.ears_back = 0.35 * up;
            g.body_angle = -0.02 * up;
        }
        Gesture::Nurse => {
            // Sous le flanc, le museau se relève vers la mamelle ; le
            // placement précis est repris par l'encolure articulée. Le
            // corps et les quatre appuis restent immobiles. Le frétillement
            // de queue est illustratif, pas une fréquence mesurée.
            let up = a * smooth(0.0, 0.45, t);
            g.head_angle = -0.95 * up;
            g.head_shift = Vec2::new(1.5 * up, -2.0 * up);
            g.ears_back = 0.15 * up;
            g.tail = 0.65 * up * (t * TAU * 5.0).sin();
        }
        Gesture::MaternalCall => {
            // L'appel grave de proximité reste bouche fermée (la mâchoire
            // est menée par le cœur), le cou à peine tendu vers l'agneau.
            g.head_angle = -0.06 * a;
            g.head_shift = Vec2::new(0.5 * a, -0.3 * a);
            g.ears_back = -0.2 * a;
        }
    }
    g
}

/// De combien la tête se relève pour le flehmen (radians), en plus de
/// l'encolure qui se tend : une quarantaine de degrés en tout, le chanfrein
/// passe à peu près à l'horizontale, le menton en avant, comme sur les
/// photos de béliers (« la tête relevée d'environ 30° », Blissitt 1989).
pub(crate) const FLEHMEN_HEAD: f64 = 0.75;

/// La posture de l'humeur ou de l'élan, ajoutée au geste : tête basse,
/// oreilles couchées ou dressées ; en l'air, pattes raides et tête secouée
/// (un agneau qui saute), ou le corps qui se tord (un agneau qui joue) ;
/// une patte qui tressaille dans le sommeil.
pub(crate) fn with_posture(mut g: Gesturing, s: &PoseSignals) -> Gesturing {
    let low = f64::from(s.head_low);
    g.head_angle += 0.42 * low;
    g.head_shift += Vec2::new(0.4 * low, 1.6 * low);
    g.ears_back += f64::from(s.ears_back);
    let stiff = f64::from(s.stiff);
    if stiff > 0.0 {
        let toss = (f64::from(s.clock) * TAU * 2.2).sin();
        g.head_angle += 0.18 * stiff * toss;
        g.body_angle += 0.06 * stiff * toss;
    }
    // Le saut tordu d'un agneau : le corps se tord, la tête part de l'autre
    // côté, les pattes de derrière ruent, celles de devant se replient.
    let twist = f64::from(s.twist);
    if twist > 0.0 {
        let w = (f64::from(s.clock) * TAU * 1.8).sin();
        g.body_angle += 0.22 * twist * w;
        g.head_angle -= 0.25 * twist * w;
        g.body_squash += 0.04 * twist * w;
        g.legs[leg_slot(false, true)].0 += 0.7 * twist;
        g.legs[leg_slot(false, false)].0 += 0.55 * twist;
        g.legs[leg_slot(true, true)].0 -= 0.35 * twist;
        // Il rue les pattes tendues, et replie celles de devant.
        g.folds[leg_slot(false, true)] -= 1.2 * twist;
        g.folds[leg_slot(false, false)] -= 1.0 * twist;
        g.folds[leg_slot(true, true)] += 0.6 * twist;
        g.tail += 0.5 * twist * w;
    }
    // Une patte qui tressaille dans le sommeil.
    let twitch = f64::from(s.twitch);
    if twitch > 0.0 {
        g.legs[leg_slot(true, true)].0 -= 0.25 * twitch;
        g.ear += 0.1 * twitch;
    }
    g
}

/// Les paupières : le clignement, et les yeux que ferme un geste (0
/// ouverts, 1 fermés).
#[must_use]
pub(crate) fn lids(s: &PoseSignals) -> f32 {
    let eyes = gesturing(s).eyes as f32;
    (s.blink + eyes * (1.0 - s.blink)).min(1.0)
}

/// Oreilles couchées (> 0) ou dressées (< 0), en rotation à ajouter quand
/// une rotation positive les fait tomber : dressées, elles montent bien
/// plus qu'elles ne se couchent (couchées, elles se plaquent vers
/// l'arrière en se relevant à peine).
pub(crate) fn ears_turn(ears_back: f64) -> f64 {
    if ears_back >= 0.0 {
        -0.25 * ears_back
    } else {
        0.75 * ears_back
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn at(gesture: Gesture, amount: f32, time: f32) -> Gesturing {
        gesturing(&PoseSignals {
            gesture: Some(gesture),
            gesture_amount: amount,
            gesture_time: time,
            ..PoseSignals::default()
        })
    }

    #[test]
    fn no_gesture_adds_nothing() {
        let g = gesturing(&PoseSignals::default());
        assert_eq!(g.head_angle, 0.0);
        assert!(g.legs.iter().all(|&(a, l)| a == 0.0 && l == 0.0));
        for gesture in Gesture::ALL {
            let g = at(gesture, 0.0, 0.5);
            assert_eq!(g.head_angle, 0.0, "{gesture:?}");
        }
    }

    #[test]
    fn a_shake_runs_from_head_to_tail_at_its_frequency() {
        // Sur une période, la tête passe par ses deux extrêmes.
        let samples: Vec<f64> = (0..=40)
            .map(|i| at(Gesture::Shake, 1.0, 0.5 + i as f32 / 40.0 / SHAKE_HZ as f32).head_angle)
            .collect();
        let (lo, hi) = samples
            .iter()
            .fold((f64::MAX, f64::MIN), |(lo, hi), &v| (lo.min(v), hi.max(v)));
        assert!(hi > 0.25 && lo < -0.25, "{lo} {hi}");
    }

    #[test]
    fn pawing_lifts_then_drags_the_near_front_hoof() {
        let lifted = at(Gesture::Paw, 1.0, 0.26);
        let dragged = at(Gesture::Paw, 1.0, 0.5);
        let slot = leg_slot(true, true);
        assert!(lifted.legs[slot].1 > 1.0, "{:?}", lifted.legs[slot]);
        assert_eq!(dragged.legs[slot].1, 0.0);
        // Les autres pattes ne bougent pas.
        assert_eq!(lifted.legs[leg_slot(false, true)], (0.0, 0.0));
        // Il commence le sabot à sa place, posé, et racle d'abord vers
        // l'arrière.
        let start = at(Gesture::Paw, 1.0, 0.0).legs[slot];
        assert!(start.0.abs() < 2e-3 && start.1 == 0.0, "{start:?}");
        let scraping = at(Gesture::Paw, 1.0, 0.08).legs[slot];
        assert!(scraping.0 > 0.1 && scraping.1 == 0.0, "{scraping:?}");
    }

    /// Un sabot ne change de place que levé : entre deux instants proches,
    /// s'il est posé aux deux (aucune levée), son angle n'a pas bougé.
    fn never_slides(gesture: Gesture, length: f32, amount: impl Fn(f32) -> f32) {
        let dt = 1.0 / 240.0;
        let steps = (length / dt) as usize;
        for k in 0..steps {
            let (t0, t1) = (k as f32 * dt, (k + 1) as f32 * dt);
            let (before, after) = (at(gesture, amount(t0), t0), at(gesture, amount(t1), t1));
            for slot in 0..4 {
                let (a0, up0) = before.legs[slot];
                let (a1, up1) = after.legs[slot];
                if up0 == 0.0 && up1 == 0.0 {
                    assert!(
                        (a1 - a0).abs() < 2e-3,
                        "{gesture:?}, patte {slot}, à {t0:.3} s : {a0:.4} puis {a1:.4}, posée"
                    );
                }
            }
        }
    }

    /// L'ampleur d'un geste de `length` secondes, comme le moteur la donne :
    /// elle monte en 0,12 s et retombe pendant les 0,18 dernières.
    fn envelope(length: f32) -> impl Fn(f32) -> f32 {
        let smoothstep = |a: f32, b: f32, x: f32| {
            let t = ((x - a) / (b - a)).clamp(0.0, 1.0);
            t * t * (3.0 - 2.0 * t)
        };
        move |t| smoothstep(0.0, 0.12, t) * smoothstep(0.0, 0.18, length - t)
    }

    #[test]
    fn the_hooves_of_a_standing_gesture_only_move_while_lifted() {
        // S'ébrouer, se camper contre le vent, s'étirer, inviter à jouer :
        // les pattes s'écartent ou avancent par de petits pas, et reviennent
        // de même ; aucune ne glisse sur l'herbe.
        for (gesture, length) in [
            (Gesture::Shake, 0.9),
            (Gesture::Brace, 3.0),
            (Gesture::Stretch, 2.4),
            (Gesture::Invite, 1.4),
            (Gesture::Scratch, 1.8),
        ] {
            never_slides(gesture, length, envelope(length));
        }
        // Campé, les quatre pattes sont écartées, posées.
        let braced = at(Gesture::Brace, 1.0, 1.5);
        for (slot, (angle, lift)) in braced.legs.iter().enumerate() {
            assert!(
                (angle.abs() - 0.16).abs() < 1e-9 && lift.abs() < 1e-9,
                "{slot}"
            );
        }
        let shaking = at(Gesture::Shake, 1.0, 0.5);
        assert!(shaking.legs[0].0 < -0.09 && shaking.legs[2].0 > 0.09);
        // Chacune a fait son pas à son tour : jamais deux levées de la même
        // paire en même temps.
        for k in 0..60 {
            let legs = at(Gesture::Shake, 1.0, k as f32 * 0.005).legs;
            assert!(legs[0].1 == 0.0 || legs[1].1 == 0.0, "antérieurs à {k}");
            assert!(legs[2].1 == 0.0 || legs[3].1 == 0.0, "postérieurs à {k}");
        }
    }

    #[test]
    fn a_pawing_hoof_goes_home_with_a_step_when_the_gesture_ends() {
        // À la fin du geste, l'ampleur retombe : le sabot, où qu'il en soit
        // de son raclement, rejoint sa place levé.
        let slot = leg_slot(true, true);
        let length = 1.3;
        let amount = envelope(length);
        let mut returned = false;
        let dt = 1.0 / 240.0;
        for k in 0..(length / dt) as usize {
            let (t0, t1) = (k as f32 * dt, (k + 1) as f32 * dt);
            let (before, after) = (
                at(Gesture::Paw, amount(t0), t0),
                at(Gesture::Paw, amount(t1), t1),
            );
            let ending = t0 > length - 0.18;
            if ending && (after.legs[slot].0 - before.legs[slot].0).abs() > 2e-3 {
                assert!(
                    before.legs[slot].1 > 0.0 || after.legs[slot].1 > 0.0,
                    "à {t0}"
                );
                returned = true;
            }
        }
        assert!(returned, "le sabot était déjà à sa place");
        let done = at(Gesture::Paw, amount(length), length).legs[slot];
        assert!(done.0.abs() < 1e-6 && done.1.abs() < 1e-6);
    }

    #[test]
    fn a_stretch_reaches_forward_then_back() {
        let neck = at(Gesture::Stretch, 1.0, 0.7);
        let hind = at(Gesture::Stretch, 1.0, 1.6);
        assert!(neck.head_shift.x > 3.0 && neck.legs[leg_slot(true, true)].0 < -0.2);
        assert!(hind.legs[leg_slot(false, true)].0 > 0.7);
        assert!(neck.legs[leg_slot(false, true)].0.abs() < 0.05);
    }

    #[test]
    fn ears_rise_much_more_than_they_flatten() {
        assert!(ears_turn(-1.0) < -0.7);
        assert!(ears_turn(1.0) > -0.3 && ears_turn(1.0) < 0.0);
    }
}
