//! Les articulations : l'encolure entre le corps et la tête, les pattes en
//! deux segments (le carpe des antérieurs, le jarret des postérieurs).
//!
//! Le même calcul sert aux dessins vectoriels ([`crate::animate`]) et aux
//! squelettes peints ([`crate::skeleton`]) : chacun donne ses points au repos
//! (dans le repère de son dessin) et où en est le corps ; on lui rend où
//! mettre chaque segment.
//!
//! Le moteur part de ce qu'il faisait d'une pièce (la tête qui tourne et se
//! déplace, le sabot qui suit son chemin), puis le répartit entre les
//! articulations par une petite cinématique inverse à deux os : pour brouter,
//! le museau va au sol devant les antérieurs, et c'est l'encolure qui descend ;
//! au pas, le sabot reste posé et le genou se plie de lui-même quand la patte
//! se raccourcit. Voir `docs/animation-animale.md`.

use std::f64::consts::PI;

use peniko::kurbo::{Affine, Point, Rect, Vec2};

use mouflon_core::LegCycle;

use crate::animate::{Gait, about, lowest_point, rotate_about, stepping};
use crate::gesture::{Gesturing, leg_slot};

/// L'angle `a` ramené entre -π et π.
fn wrap(a: f64) -> f64 {
    (a + PI).rem_euclid(2.0 * PI) - PI
}

fn angle(v: Vec2) -> f64 {
    v.y.atan2(v.x)
}

fn rotate(v: Vec2, a: f64) -> Vec2 {
    let (s, c) = a.sin_cos();
    Vec2::new(v.x * c - v.y * s, v.x * s + v.y * c)
}

/// Étire de `s` le long de la direction `dir` (radians), autour de `p`.
pub(crate) fn stretch_along(p: Point, dir: f64, s: f64) -> Affine {
    about(
        p,
        Affine::rotate(dir) * Affine::scale_non_uniform(s, 1.0) * Affine::rotate(-dir),
    )
}

/// Une chaîne de deux segments au repos : la racine (épaule, hanche, base
/// de l'encolure), l'articulation (carpe, jarret, nuque) et le bout (sabot,
/// museau). Ses longueurs et ses angles au repos sont calculés une fois.
#[derive(Clone, Copy, Debug)]
pub(crate) struct Chain {
    pub root: Point,
    pub joint: Point,
    pub tip: Point,
    /// Longueurs des deux segments, de la racine au bout au repos.
    l1: f64,
    l2: f64,
    rest_len: f64,
    /// Direction du premier segment et pli au repos.
    first_rest: f64,
    bend_rest: f64,
}

impl Chain {
    pub(crate) fn new(root: Point, joint: Point, tip: Point) -> Self {
        let first_rest = angle(joint - root);
        Self {
            root,
            joint,
            tip,
            l1: (joint - root).hypot().max(1e-6),
            l2: (tip - joint).hypot().max(1e-6),
            rest_len: (tip - root).hypot().max(1e-6),
            first_rest,
            bend_rest: wrap(angle(tip - joint) - first_rest),
        }
    }
}

/// La solution : de combien tourne le premier segment (autour de la
/// racine), de combien le second tourne en plus du premier (autour de
/// l'articulation), et de combien le tout s'étire.
#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) struct Bend {
    pub first: f64,
    pub second: f64,
    pub stretch: f64,
    /// Couché, la part du haut d'une patte qui rentre sous le corps (0 :
    /// rien) : vu de côté, il paraît d'autant plus court (voir
    /// [`LegRest::tucked`]).
    pub tuck: f64,
}

impl Bend {
    pub(crate) fn lerp(self, other: Self, t: f64) -> Self {
        Self {
            first: self.first + (other.first - self.first) * t,
            second: self.second + (other.second - self.second) * t,
            stretch: self.stretch + (other.stretch - self.stretch) * t,
            tuck: self.tuck + (other.tuck - self.tuck) * t,
        }
    }
}

impl Chain {
    /// Le pli au repos (le second segment par rapport au premier).
    fn rest_bend(&self) -> f64 {
        self.bend_rest
    }

    /// Le côté où l'articulation se plie : celui du dessin s'il est plié,
    /// sinon `natural` (1 : le second segment tourne dans le sens des
    /// aiguilles d'une montre, comme le canon d'un antérieur qui part en
    /// arrière ; -1 : l'inverse, comme celui d'un postérieur).
    fn side(&self, natural: f64) -> f64 {
        let rest = self.rest_bend();
        if rest.abs() > 0.02 {
            rest.signum()
        } else {
            natural
        }
    }

    /// La cinématique inverse : la racine posée en `root`, le bout en
    /// `target`. `fold` plie l'articulation au-delà de ce qu'il faut (le
    /// bout se rapproche alors de la racine, dans la même direction ;
    /// négatif, il la déplie, jusqu'à la tendre tout à fait) ; au
    /// delà de sa longueur, la chaîne s'étire jusqu'à `max_stretch`, puis
    /// pointe vers la cible. Plus près qu'au repos, elle se tasse d'abord
    /// (jusqu'à `give` de sa longueur) avant de plier : une patte d'appui
    /// reste droite, comme celle d'un vrai mouton.
    pub(crate) fn reach(
        &self,
        root: Point,
        target: Point,
        natural: f64,
        fold: f64,
        max_stretch: f64,
        give: f64,
    ) -> Bend {
        let (l1, l2) = (self.l1, self.l2);
        let to = target - root;
        let d = to.hypot();
        let straight = l1 + l2;
        let rest = self.rest_len;
        let stretch = if d > straight {
            (d / straight).min(max_stretch.max(1.0))
        } else if d < rest {
            (d / rest).max(1.0 - give.clamp(0.0, 0.5))
        } else {
            1.0
        };
        let d = (d / stretch).clamp((l1 - l2).abs() + 1e-6, straight);
        self.solve(to, d, l1, l2, natural, fold, stretch)
    }

    /// Comme [`Self::reach`], mais seul le premier segment s'étire (une
    /// encolure qui s'allonge sous une tête qui reste d'une pièce).
    pub(crate) fn reach_first(
        &self,
        root: Point,
        target: Point,
        natural: f64,
        max_stretch: f64,
    ) -> Bend {
        let (l1, l2) = (self.l1, self.l2);
        let to = target - root;
        let d = to.hypot();
        let stretch = ((d - l2) / l1).clamp(1.0, max_stretch.max(1.0));
        let l1 = l1 * stretch;
        let d = d.clamp((l1 - l2).abs() + 1e-6, l1 + l2);
        self.solve(to, d, l1, l2, natural, 0.0, stretch)
    }

    /// Les angles qui posent le bout à la distance `d` dans la direction `to`,
    /// les segments mesurant `l1` et `l2`.
    #[allow(clippy::too_many_arguments)]
    fn solve(
        &self,
        to: Vec2,
        d: f64,
        l1: f64,
        l2: f64,
        natural: f64,
        fold: f64,
        stretch: f64,
    ) -> Bend {
        let c = ((d * d - l1 * l1 - l2 * l2) / (2.0 * l1 * l2)).clamp(-1.0, 1.0);
        let side = self.side(natural);
        let bend = side * (c.acos() + fold).clamp(0.0, PI - 0.05);
        let offset = (l2 * bend.sin()).atan2(l1 + l2 * bend.cos());
        let first_dir = angle(to) - offset;
        Bend {
            first: wrap(first_dir - self.first_rest),
            second: wrap(bend - self.bend_rest),
            stretch,
            tuck: 0.0,
        }
    }

    /// Où sont l'articulation et le bout une fois pliés, la racine en `root`
    /// (le haut rentré de `tuck`, couché).
    #[cfg(test)]
    pub(crate) fn place(&self, root: Point, bend: Bend) -> (Point, Point) {
        let joint =
            root + rotate(self.joint - self.root, bend.first) * bend.stretch * (1.0 - bend.tuck);
        let tip = joint + rotate(self.tip - self.joint, bend.first + bend.second) * bend.stretch;
        (joint, tip)
    }
}

/// Le premier segment posé : déplacé jusqu'à `root`, tourné autour de sa
/// racine, étiré le long de lui-même.
pub(crate) fn first_segment(chain: &Chain, root: Point, bend: Bend) -> Affine {
    let turn = Affine::translate(root - chain.root) * rotate_about(chain.root, bend.first);
    if (bend.stretch - 1.0).abs() < 1e-12 {
        return turn;
    }
    turn * stretch_along(chain.root, chain.first_rest, bend.stretch)
}

/// Le second segment, dans le repère du premier : il tourne autour de
/// l'articulation, étiré le long de lui-même. Le premier l'étirerait le long
/// de sa propre direction : c'est défait, sinon une patte pliée et étirée
/// (ou tassée) poserait son sabot à côté de la cible, d'autant plus loin
/// que le jarret est plié, et le sabot posé glisserait à mesure que la
/// patte s'étire.
pub(crate) fn second_segment(chain: &Chain, bend: Bend) -> Affine {
    let turn = rotate_about(chain.joint, bend.second);
    if (bend.stretch - 1.0).abs() < 1e-12 {
        return turn;
    }
    stretch_along(chain.joint, chain.first_rest, 1.0 / bend.stretch)
        * turn
        * stretch_along(
            chain.joint,
            chain.first_rest + chain.bend_rest,
            bend.stretch,
        )
}

// --------------------------------------------------------------- encolure

/// Ce que le moteur voulait de la tête d'une pièce (voir
/// [`crate::animate::pose`]) : sa rotation autour de la nuque et son
/// déplacement, sans le broutage ni le sommeil tête au sol (refaits ici).
#[derive(Clone, Copy, Debug)]
pub(crate) struct HeadIntent {
    pub angle: f64,
    pub shift: Vec2,
}

/// L'encolure au repos : sa base (devant le garrot), la nuque, le museau (ce
/// qui touche l'herbe), et la boîte de la tête.
#[derive(Clone, Copy, Debug)]
pub(crate) struct NeckRest {
    pub base: Point,
    pub poll: Point,
    pub muzzle: Point,
    pub head: Rect,
}

/// Ce que font l'encolure et la tête : l'encolure autour de sa base (et
/// étirée le long d'elle-même), la tête autour de la nuque, d'une pièce
/// (elle ne s'étire jamais).
#[derive(Clone, Copy, Debug)]
pub(crate) struct NeckPose {
    pub neck: Affine,
    pub head: Affine,
    /// L'angle de la tête dans le dessin (pour garder les pupilles à
    /// l'horizontale).
    pub head_angle: f64,
}

/// Les angles de l'encolure (radians, positif : vers le bas). Aux aguets,
/// l'encolure se redresse ; quand il fonce, elle se tend, tête basse ; à
/// moitié endormi, elle s'affaisse un peu.
const NECK_ALERT: f64 = -0.24;
const NECK_CHARGE: f64 = 0.3;
const NECK_DOZE: f64 = 0.14;
/// En broutant, le museau va au sol à tant de fois la longueur de la tête
/// (de la nuque au museau) devant les sabots des antérieurs, et la tête
/// mordille.
pub(crate) const GRAZE_REACH: f64 = 0.85;
/// Jusqu'où l'encolure s'étire pour poser le museau au sol.
const NECK_STRETCH: f64 = 1.12;
/// Endormi la tête au sol : l'encolure s'allonge vers l'avant, la tête
/// presque à plat.
pub(crate) const SLEEP_STRETCH: f64 = 1.08;
const SLEEP_HEAD: f64 = 0.12;

/// Ce qui mène l'encolure à cette image.
#[derive(Clone, Copy, Debug)]
pub(crate) struct NeckDrive {
    /// La base de l'encolure, portée par le corps.
    pub base: Point,
    pub intent: HeadIntent,
    /// Aux aguets (0 à 1), il fonce tête basse (0 à 1), il somnole (0 à 1).
    pub alert: f64,
    pub charge: f64,
    pub doze: f64,
    /// Il broute (0 à 1), ses coups de dents (-1 à 1), et les sabots des
    /// antérieurs (le museau va au sol devant eux).
    pub graze: f64,
    pub nibble: f64,
    pub graze_at: Point,
    /// Endormi la tête au sol (0 à 1).
    pub sleep_low: f64,
    /// Couché (0 à 1) : la tête ne passe pas sous l'herbe.
    pub lying: f64,
    /// La mamelle dans le repère du dessin et l'ampleur du geste de tétée.
    /// Sans cible, le geste seul relève la tête.
    pub nursing: Option<(Point, f64)>,
    /// La hauteur du sol, un centième de la hauteur de l'animal.
    pub ground: f64,
    pub u: f64,
    /// La tête qui tourne d'un côté à l'autre, vue de profil (voir
    /// [`Gesturing::head_squash`]).
    pub squash: f64,
}

impl NeckRest {
    fn chain(&self) -> Chain {
        Chain::new(self.base, self.poll, self.muzzle)
    }

    /// Le pose de l'encolure et de la tête. `floor` : ce que l'encolure
    /// dessine au-delà de son premier tiers, s'il est connu (voir
    /// [`crate::neck_floor::NeckFloor`]).
    pub(crate) fn pose(
        &self,
        d: &NeckDrive,
        floor: Option<&crate::neck_floor::NeckFloor>,
    ) -> NeckPose {
        let neck_len = (self.poll - self.base).hypot().max(1e-6);
        let rest_dir = angle(self.poll - self.base);
        // Ce que voulait le moteur : la nuque déplacée de `shift`. L'encolure
        // tourne autour de sa base pour l'y porter, et s'allonge un peu
        // s'il le faut ; la tête garde l'angle voulu.
        let wanted = self.poll + d.intent.shift;
        let to = wanted - d.base;
        let mut first = wrap(angle(to) - rest_dir);
        let mut stretch = (to.hypot() / neck_len).clamp(0.94, 1.1);
        first += NECK_ALERT * d.alert + NECK_CHARGE * d.charge + NECK_DOZE * d.doze;
        stretch += 0.04 * d.charge;
        let mut head = d.intent.angle + 0.06 * d.alert;
        let mut bend = Bend {
            first,
            second: head - first,
            stretch,
            tuck: 0.0,
        };
        // Broute : le museau au sol, devant les antérieurs.
        if d.graze > 0.0 {
            let chain = self.chain();
            let mut target = d.graze_at + Vec2::new(GRAZE_REACH * self.head_length(), 0.0);
            target.y -= 0.6 * d.u * d.nibble.max(0.0);
            // Une encolure trop courte pour aller si loin : le museau se pose
            // plus près des antérieurs, mais il touche l'herbe.
            let reach = 0.99 * (NECK_STRETCH * chain.l1 + chain.l2);
            let dy = target.y - d.base.y;
            if dy < reach {
                target.x = target.x.min(d.base.x + (reach * reach - dy * dy).sqrt());
            }
            let mut ik = chain.reach_first(d.base, target, 1.0, NECK_STRETCH);
            ik.second += 0.05 * d.nibble;
            bend = bend.lerp(ik, d.graze);
        }
        // Endormi la tête au sol : la tête presque à plat, l'encolure tendue
        // vers l'avant juste assez pour que le menton touche. Si l'encolure
        // (son collier, sa gorge, ses touffes) touche l'herbe la première, la
        // tête s'arrête là, posée dessus.
        if d.sleep_low > 0.0 {
            let lowest = lowest_point(&rotate_about(self.poll, SLEEP_HEAD), self.head);
            let drop = lowest - self.poll.y;
            let want = d.ground - d.u - drop - d.base.y;
            let mut stretch = SLEEP_STRETCH;
            if want > stretch * neck_len {
                stretch = (want / neck_len).min(1.15);
            }
            let q = (want / (stretch * neck_len)).clamp(-1.0, 1.0);
            let mut toward = q.asin();
            if let Some(floor) = floor {
                toward = floor.limit(toward, d.base.y, d.ground, d.u, stretch);
            }
            let first = wrap(toward - rest_dir);
            let low = Bend {
                first,
                second: SLEEP_HEAD - first,
                stretch,
                tuck: 0.0,
            };
            bend = bend.lerp(low, d.sleep_low);
        }
        // Couché, la tête ne passe pas sous l'herbe : une grosse tête sur une
        // encolure courte et un corps bas (un agneau) s'y enfoncerait en
        // somnolant. L'encolure se relève et la tête se redresse, juste
        // assez pour s'y poser.
        if d.lying > 0.0 && d.graze <= 0.0 && d.nursing.is_none() {
            let low = |bend: Bend| {
                let poll = d.base + rotate(self.poll - self.base, bend.first) * bend.stretch;
                poll.y - self.poll.y
                    + lowest_point(
                        &rotate_about(self.poll, bend.first + bend.second),
                        self.head,
                    )
            };
            if low(bend) > d.ground {
                let raised = bend.first - 0.6;
                let up = Bend {
                    first: raised,
                    second: (bend.first + bend.second).min(0.0) - raised,
                    ..bend
                };
                let (mut under, mut clear) = (0.0, 1.0);
                if low(up) <= d.ground {
                    for _ in 0..16 {
                        let middle = 0.5 * (under + clear);
                        if low(bend.lerp(up, middle)) > d.ground {
                            under = middle;
                        } else {
                            clear = middle;
                        }
                    }
                }
                bend = bend.lerp(up, clear);
            }
        }
        if let Some((target, amount)) = d.nursing {
            let chain = self.chain();
            let to = target - d.base;
            let stretch = ((to.hypot() - chain.l2) / chain.l1).clamp(1.0, NECK_STRETCH);
            let l1 = chain.l1 * stretch;
            let distance = to
                .hypot()
                .clamp((l1 - chain.l2).abs() + 1e-6, l1 + chain.l2);
            // L'autre branche du pli : le cou descend sous le ventre et
            // la tête se relève, contrairement au broutage. La tête prend
            // le chemin court pendant l'entrée, jamais un tour complet.
            let c = ((distance * distance - l1 * l1 - chain.l2 * chain.l2) / (2.0 * l1 * chain.l2))
                .clamp(-1.0, 1.0);
            let fold = -c.acos().min(PI - 0.05);
            let first = wrap(
                angle(to)
                    - (chain.l2 * fold.sin()).atan2(l1 + chain.l2 * fold.cos())
                    - chain.first_rest,
            );
            let head = wrap(first + fold - chain.bend_rest);
            bend = bend.lerp(
                Bend {
                    first,
                    second: head - first,
                    stretch,
                    tuck: 0.0,
                },
                amount.clamp(0.0, 1.0),
            );
        }
        head = bend.first + bend.second;
        let neck = Affine::translate(d.base - self.base)
            * rotate_about(self.base, bend.first)
            * stretch_along(self.base, rest_dir, bend.stretch);
        let poll = d.base + rotate(self.poll - self.base, bend.first) * bend.stretch;
        let head_t = Affine::translate(poll - self.poll)
            * rotate_about(self.poll, head)
            * crate::animate::scale_about(self.head.center(), 1.0 - d.squash, 1.0);
        NeckPose {
            neck,
            head: head_t,
            head_angle: head,
        }
    }

    /// De la nuque au museau.
    pub(crate) fn head_length(&self) -> f64 {
        (self.muzzle - self.poll).hypot()
    }

    /// Portée de tétée : le cou se déploie vers la mère et l'axe du museau
    /// vise vers le haut. Placer le jeune d'après sa tête au repos obligerait
    /// certaines longues encolures à se rabattre près du sol pour toucher.
    /// L'angle est un choix de pose, pas une limite anatomique mesurée.
    pub(crate) fn nursing_muzzle(&self) -> Point {
        let neck = (self.poll - self.base).hypot();
        let head = self.head_length();
        let up: f64 = 0.55;
        let drop = (head * up.sin()).min(neck * 0.8);
        let forward = (neck * neck - drop * drop).max(0.0).sqrt();
        Point::new(self.base.x + forward + head * up.cos(), self.base.y)
    }
}

/// Repère mammaire implicite sous l'arrière du flanc. Une toison longue
/// cache le ventre : son bord n'est pas l'emplacement de la mamelle.
/// Sans variante anatomique d'agneau, la base de l'encolure permet de
/// retrouver une hauteur abdominale sous cette laine : le jeune à 60 %
/// de taille y porte la tête sans rabattre le cou vers le sol. Ce choix
/// de dessin n'est pas une mesure zoologique.
pub(crate) fn udder_point(
    body: Rect,
    hip_x: f64,
    ground: Point,
    height: f64,
    neck: Option<NeckRest>,
) -> Point {
    let mut y = body.max_y() + 0.02 * height;
    if let Some(neck) = neck {
        y = y.min(ground.y + 0.60 * (neck.base.y - ground.y));
    }
    Point::new(hip_x + body.width() * 0.08, y)
}

// --------------------------------------------------------------- pattes

/// Une patte en deux segments, au repos : l'épaule ou la hanche, le carpe
/// ou le jarret, le bas du sabot ; et comment elle se replie sous l'animal
/// couché.
#[derive(Clone, Copy, Debug)]
pub(crate) struct LegRest {
    pub chain: Chain,
    pub front: bool,
    pub near: bool,
    fold: Fold,
}

/// Comment une patte se replie sous l'animal couché (voir
/// [`LegDrive::lying`]), d'après son dessin.
#[derive(Clone, Copy, Debug)]
struct Fold {
    /// Le rayon de l'articulation : ce que le haut de la patte dessine sous
    /// son pivot (son demi-disque). Couchée, elle se pose sur le sol.
    knee: f64,
    /// La semelle : les coins du bas de la boîte du bas de la patte, du talon
    /// au bout du sabot.
    sole: [Point; 2],
    /// De combien le canon replié remonte vers le sabot (radians) : un peu,
    /// et assez pour que le sabot, tourné le bout (devant) ou le talon
    /// (derrière) vers le bas, ne passe pas sous le sol.
    rise: f64,
    /// Jusqu'où l'articulation posée au sol va devant l'épaule (le carpe)
    /// ou derrière la hanche (le jarret) : son bord au plus au devant de la
    /// poitrine, au bout de la fesse.
    room: f64,
    /// De combien le haut de la patte peut glisser le long de lui-même sous
    /// le corps sans que son bord du haut sorte du dos (0 : il est dessiné
    /// par-dessus le corps, comme les pattes du côté qu'on voit de
    /// l'Ouessant, dont la laine monte sur le flanc). Au-delà, il se
    /// raccourcit.
    headroom: f64,
    /// Ce que dessine le haut de la patte, autour de sa hanche : de son
    /// enveloppe convexe, le point le plus loin dans chacune de
    /// [`OUTLINE`] directions. Couché, il ne passe pas sous l'herbe (voir
    /// [`LegDrive::above_ground`]). `outlined` : le contour est connu.
    thigh: [Vec2; OUTLINE],
    outlined: bool,
}

/// Les directions où l'on garde un point du contour du haut d'une patte.
const OUTLINE: usize = 32;
/// Ce qu'on laisse le haut d'une patte couchée passer sous le sol, en
/// centièmes de la hauteur du dessin : le bord anticrénelé de son image.
const THIGH_UNDER: f64 = 0.05;

/// Le canon replié remonte au moins de tant vers le sabot (radians), au
/// plus de tant.
const FOLD_RISE: (f64, f64) = (0.1, 0.9);
/// Couché, le haut d'une patte rentre sous le corps au plus de cette part
/// de sa longueur.
const MAX_TUCK: f64 = 0.8;

impl LegRest {
    /// Une patte d'après son dessin : sa chaîne, ce que dessinent son haut
    /// (`upper`, sans le bas) et son bas (`lower`), le corps (`body`), et de
    /// combien son haut peut glisser sous le corps sans sortir du dos (voir
    /// [`Fold::headroom`]).
    pub(crate) fn new(
        chain: Chain,
        (front, near): (bool, bool),
        upper: Option<Rect>,
        lower: Rect,
        body: Option<Rect>,
        headroom: f64,
        outline: &[Point],
    ) -> Self {
        // Le demi-disque du haut, sous le pivot de l'articulation (à défaut,
        // une part de la largeur du bas, sabot compris).
        let width = lower.width().max(1e-6);
        let knee = upper
            .map(|r| r.max_y() - chain.joint.y)
            .filter(|r| *r > 0.0)
            .unwrap_or(0.4 * width)
            .clamp(0.15 * width, 0.6 * width);
        // Le canon replié tourne le sabot : devant, son bout vers le bas ;
        // derrière, son talon. Le bas de la boîte du bas de la patte (la
        // semelle, du talon au bout) reste au-dessus du sol.
        let rest = angle(chain.tip - chain.joint);
        let sole = [
            Point::new(lower.min_x(), lower.max_y()),
            Point::new(lower.max_x(), lower.max_y()),
        ];
        let lowest = |rise: f64| {
            let turn = if front { PI + rise } else { -rise } - rest;
            sole.iter()
                .map(|&c| rotate(c - chain.joint, turn).y)
                .fold(f64::NEG_INFINITY, f64::max)
        };
        let mut rise = FOLD_RISE.0;
        while rise < FOLD_RISE.1 && lowest(rise) > knee {
            rise += 0.01;
        }
        let room = body.map_or(f64::INFINITY, |b| {
            let end = if front {
                b.max_x() - chain.root.x
            } else {
                chain.root.x - b.min_x()
            };
            (end - knee).max(0.0)
        });
        Self {
            chain,
            front,
            near,
            fold: Fold {
                knee,
                sole,
                rise,
                room,
                headroom: headroom.max(0.0),
                thigh: std::array::from_fn(|k| {
                    let (sin, cos) = (2.0 * PI * k as f64 / OUTLINE as f64).sin_cos();
                    let toward = Vec2::new(cos, sin);
                    outline
                        .iter()
                        .map(|&p| p - chain.root)
                        .max_by(|p, q| p.dot(toward).total_cmp(&q.dot(toward)))
                        .unwrap_or(Vec2::ZERO)
                }),
                outlined: outline.len() >= 3,
            },
        }
    }

    /// Le plus bas de ce que dessine le haut de la patte, posé par `upper`
    /// et rentré sous le corps de `tuck` (voir [`Self::tucked`]).
    fn thigh_low(&self, upper: Affine, tuck: f64) -> f64 {
        let at = if tuck > 0.0 {
            self.tucked(upper, Affine::IDENTITY, tuck).0
        } else {
            upper
        };
        self.fold
            .thigh
            .iter()
            .map(|&p| (at * (self.chain.root + p)).y)
            .fold(f64::NEG_INFINITY, f64::max)
    }

    /// Le haut et le bas de la patte, le haut rentré sous le corps de
    /// `tuck` (une part de sa longueur) : il glisse le long de lui-même sous
    /// le corps, qui le cache, sans changer de forme, tant que son bord du
    /// haut reste sous le dos ; au-delà (ou dessiné par-dessus le corps), il
    /// se raccourcit aussi. L'articulation arrive au même endroit, et le bas
    /// la suit d'une pièce.
    fn tucked(&self, upper: Affine, lower: Affine, tuck: f64) -> (Affine, Affine) {
        let chain = &self.chain;
        let along = chain.joint - chain.root;
        let fold = tuck * chain.l1;
        // La place se compte à la verticale : la patte penchée glisse un peu
        // plus loin pour remonter d'autant, vers le milieu du dos.
        let upright = ((upper * chain.joint - upper * chain.root).y / chain.l1).max(0.2);
        let slide = fold.min(self.fold.headroom / upright);
        let shift = Affine::translate(-along * tuck);
        if slide >= fold - 1e-9 {
            return (upper * shift, lower);
        }
        let shorter = stretch_along(
            chain.root,
            angle(along),
            (chain.l1 - fold) / (chain.l1 - slide),
        ) * Affine::translate(-along * (slide / chain.l1));
        (upper * shorter, shorter.inverse() * shift * lower)
    }
}

/// Le côté où plie l'articulation : le canon d'un antérieur part en arrière
/// (le carpe pointe vers l'avant), celui d'un postérieur vers l'avant (le
/// jarret pointe vers l'arrière).
fn natural(front: bool) -> f64 {
    if front { 1.0 } else { -1.0 }
}

/// De combien plient les articulations en l'air (radians) : pattes
/// repliées en montant. En vol, au pas, le carpe se plie un peu plus que
/// ce que demande la levée du sabot (le canon part vers l'arrière).
const TUCK_FOLD: (f64, f64) = (1.7, 1.3);
const SWING_FOLD: (f64, f64) = (0.25, 0.0);
/// Jusqu'où une patte s'étire (au bout de l'appui, tenue en l'air), et de
/// combien elle se tasse à l'appui avant de plier.
const LEG_STRETCH: f64 = 1.25;
const LEG_GIVE: f64 = 0.07;

/// Ce qui mène les pattes à cette image : les grandeurs du moteur.
#[derive(Clone, Copy, Debug)]
pub(crate) struct LegDrive {
    pub gait: Gait,
    pub phase: f64,
    pub duty: f64,
    /// La foulée de l'allure en cours, et celle du pas (unités du dessin).
    pub stride: f64,
    pub stride_walk: f64,
    /// Le cycle propre de chaque patte, s'il y en a un (voir
    /// [`mouflon_core::LegCycle`]), dans l'ordre de [`leg_slot`].
    pub legs: [LegCycle; 4],
    pub u: f64,
    pub ground: f64,
    pub air: f64,
    pub tuck: f64,
    pub reach: f64,
    /// Un saut sur un rebord, en l'air (0–1 : `PoseSignals::leap` × `air`
    /// × `loose`), et où il en est de son vol (voir
    /// [`crate::animate::leap_leg`]).
    pub leaping: f64,
    pub flight: f64,
    pub loose: f64,
    pub dangle: f64,
    pub swing: f64,
    pub splay: f64,
    pub front_down: f64,
    pub back_down: f64,
    pub act: Gesturing,
    /// L'inclinaison de tout l'animal (l'élan, voir
    /// [`crate::animate::root_transform`]), défaite pour les sabots au sol :
    /// posés, ils restent sur le sol de l'écran, ni dessous ni au-dessus.
    pub untilt: Affine,
}

impl LegDrive {
    /// Où vont la patte et son bas : le premier segment, libre dans le
    /// dessin, la hanche (ou l'épaule) posée en `hip` ; le second, dans le
    /// repère du premier.
    pub(crate) fn pose(&self, leg: &LegRest, hip: Point) -> (Affine, Affine) {
        let bend = self.bend(leg, hip);
        let upper = first_segment(&leg.chain, hip, bend);
        let lower = second_segment(&leg.chain, bend);
        if bend.tuck > 0.0 {
            leg.tucked(upper, lower, bend.tuck)
        } else {
            (upper, lower)
        }
    }

    pub(crate) fn bend(&self, leg: &LegRest, hip: Point) -> Bend {
        let LegRest {
            chain, front, near, ..
        } = *leg;
        let u = self.u;
        let moving = self.gait.moving();
        let leg_len = (self.ground - chain.root.y).max(u * 8.0);
        // Le sabot : le moteur dit où il est (posé, il reste où il est ; il
        // se lève à son moment, et se remet à sa place quand l'animal
        // s'arrête : voir `mouflon_core::LegCycle`). Sans cycle propre (un
        // signal fait à la main), son chemin vient de la phase commune et du
        // décalage de l'allure, ample à mesure qu'il marche.
        let slot = leg_slot(front, near);
        let reach = 0.93 * leg_len;
        let own = self.legs[slot];
        let (forward, lift, flight) = if own.duty > 0.0 {
            (
                (f64::from(own.x) * self.stride_walk).clamp(-reach, reach),
                f64::from(own.raise) * u,
                f64::from(own.flight),
            )
        } else {
            let now = Step {
                duty: self.duty,
                half: (self.stride * self.duty * 0.5).min(reach),
            };
            let cycle = self.phase + crate::animate::leg_phase(front, near, &self.gait);
            let at = (cycle / (2.0 * PI)).rem_euclid(1.0);
            let (forward, raised, flight) = hoof_path(at, now, now, front);
            (moving * forward, raised * self.gait.lift(u), flight)
        };
        let rest = chain.tip;
        let mut target = self.untilt * Point::new(rest.x + forward, rest.y - lift);
        // Ce que le moteur faisait de la patte d'une pièce : la tourner
        // autour de la hanche (en l'air, tenue, à l'atterrissage, les gestes).
        let mut turn = if front {
            -0.15 * self.air * self.loose + self.tuck - 0.3 * self.reach
        } else {
            0.15 * self.air * self.loose + 0.55 * self.tuck - 0.12 * self.reach
        } * (1.0 - self.leaping);
        // Sur un rebord : les pattes repliées presque tout le vol, puis les
        // antérieurs tendus vers le rebord.
        let (leap_turn, leap_fold) = crate::animate::leap_leg(front, self.flight);
        turn += self.leaping * leap_turn;
        turn += self.dangle * (if front { -0.1 } else { 0.12 } + self.swing * 0.4);
        // À l'atterrissage, les sabots s'écartent un peu en glissant sur le
        // sol ; ce sont surtout les genoux qui plient.
        let splay = leg_len * (0.4 * self.splay).sin();
        target.x += if front { splay } else { -splay };
        let (gesture_angle, gesture_lift) = self.act.legs[slot];
        turn += gesture_angle;
        if turn != 0.0 {
            target = hip + rotate(target - hip, turn);
        }
        target.y -= gesture_lift * u;
        // Tenue ou tendue vers le sol, la patte pend de tout son long.
        let hang = 1.0 + 0.07 * self.dangle + 0.04 * self.air * (1.0 - self.leaping);
        target = hip + (target - hip) * hang;
        let (tuck, swing) = if front {
            (TUCK_FOLD.0, SWING_FOLD.0)
        } else {
            (TUCK_FOLD.1, SWING_FOLD.1)
        };
        let fold = tuck * (self.tuck + self.leaping * leap_fold)
            + swing * moving * flight * (1.0 - self.air)
            + self.act.folds[slot];
        // À l'appui, la patte reste droite (elle se tasse un rien) ; dès que
        // le sabot quitte le sol, l'articulation plie.
        let give =
            LEG_GIVE * (1.0 - (4.0 * flight).min(1.0)) * (1.0 - self.air) * (1.0 - self.dangle);
        let mut bend = chain.reach(hip, target, natural(front), fold, LEG_STRETCH, give);
        // Couché : les antérieurs repliés sous le poitrail (le carpe au sol,
        // le canon sous le corps), les postérieurs le long du ventre (le
        // jarret sous la fesse, le canon vers l'avant). Il plie l'avant
        // d'abord, et se relève l'arrière d'abord.
        let down = if front {
            self.front_down
        } else {
            self.back_down
        };
        if down > 0.0 {
            bend = self.above_ground(leg, hip, bend.lerp(self.lying(leg, hip), down));
        }
        bend
    }

    /// En se couchant ou en se relevant, la patte passe d'une pose à l'autre
    /// sans que l'articulation ni le sabot ne passent sous le sol : le haut
    /// rentre un peu plus sous le corps, le canon se replie un peu plus
    /// (le sabot reste sur le sol pendant que le genou descend).
    fn above_ground(&self, leg: &LegRest, hip: Point, bend: Bend) -> Bend {
        let chain = &leg.chain;
        let fold = &leg.fold;
        // L'articulation, son disque sur le sol au plus bas : rentrer le haut
        // la remonte vers la hanche.
        let kneeling = |mut bend: Bend| {
            let upper = first_segment(chain, hip, Bend { tuck: 0.0, ..bend });
            let low = (upper * chain.joint).y + fold.knee - self.ground;
            let rises = (upper * chain.joint - upper * chain.root).y;
            if low > 0.0 && rises > 1e-9 {
                bend.tuck = bend.tuck.max(low / rises).min(MAX_TUCK);
            }
            (upper, rises, bend)
        };
        let (mut upper, rises, mut bend) = kneeling(bend);
        // Le haut de la patte lui-même (la cuisse, l'épaule, leur laine) :
        // très penché sous un corps peu profond (un agneau haut sur pattes,
        // une race trapue), son bord passerait sous l'herbe. Il rentre un
        // peu plus sous le corps ; s'il y est déjà tout à fait, il se
        // redresse, l'articulation plus près de la hanche.
        if fold.outlined {
            let floor = self.ground + THIGH_UNDER * self.u;
            for _ in 0..3 {
                let under = leg.thigh_low(upper, bend.tuck) - floor;
                if under <= 0.0 || rises <= 1e-9 || bend.tuck >= MAX_TUCK {
                    break;
                }
                bend.tuck = (bend.tuck + under / rises).min(MAX_TUCK);
            }
            if leg.thigh_low(upper, bend.tuck) > floor {
                // Droit sous la hanche, il est au plus haut ; le canon garde
                // sa direction.
                let upright = wrap(angle(Vec2::new(0.0, 1.0)) - chain.first_rest);
                let lean = wrap(bend.first - upright);
                let leaning = |part: f64| {
                    kneeling(Bend {
                        first: upright + part * lean,
                        second: bend.second + (1.0 - part) * lean,
                        ..bend
                    })
                };
                let low = |part: f64| {
                    let (upper, _, bend) = leaning(part);
                    leg.thigh_low(upper, bend.tuck)
                };
                let (mut clear, mut touching) = (0.0, 1.0);
                if low(clear) <= floor {
                    for _ in 0..16 {
                        let middle = 0.5 * (clear + touching);
                        if low(middle) > floor {
                            touching = middle;
                        } else {
                            clear = middle;
                        }
                    }
                }
                (upper, _, bend) = leaning(clear);
            }
        }
        self.sole_above_ground(leg, upper, bend)
    }

    /// Le sabot d'une patte qui se couche (voir [`Self::above_ground`]).
    fn sole_above_ground(&self, leg: &LegRest, upper: Affine, mut bend: Bend) -> Bend {
        let chain = &leg.chain;
        let fold = &leg.fold;
        let along = chain.joint - chain.root;
        // Le sabot : la semelle tourne avec le canon autour du genou ; le
        // canon se replie (vers l'arrière devant, vers l'avant derrière)
        // juste assez pour qu'elle reste sur le sol.
        let slid = upper * Affine::translate(-along * bend.tuck);
        let lowest = |second: f64| {
            let lower = slid * second_segment(chain, Bend { second, ..bend });
            fold.sole
                .iter()
                .map(|&c| (lower * c).y)
                .fold(f64::NEG_INFINITY, f64::max)
        };
        if lowest(bend.second) <= self.ground || (slid * chain.joint).y >= self.ground {
            return bend;
        }
        let side = natural(leg.front);
        let (mut under, mut over) = (0.0, 0.5 * PI);
        if lowest(bend.second + side * over) <= self.ground {
            for _ in 0..24 {
                let mid = 0.5 * (under + over);
                if lowest(bend.second + side * mid) > self.ground {
                    under = mid;
                } else {
                    over = mid;
                }
            }
        }
        bend.second += side * over;
        bend
    }

    /// La patte repliée sous l'animal couché sur le sternum : l'articulation
    /// posée au sol sous le corps (le carpe sous le poitrail, le jarret sous
    /// la fesse), le canon replié dessous, vers l'arrière (antérieur) ou
    /// vers l'avant (postérieur), le sabot un peu relevé. Le haut de la
    /// patte, plus long que la hanche n'est haute au-dessus du sol, ne
    /// s'allonge plus devant le poitrail ou derrière la croupe : il rentre
    /// sous le corps, vers le milieu, et paraît d'autant plus court.
    fn lying(&self, leg: &LegRest, hip: Point) -> Bend {
        let chain = &leg.chain;
        let fold = &leg.fold;
        let l1 = chain.l1;
        let drop = (self.ground - fold.knee - hip.y).max(1e-3 * l1);
        // De tout son long, le haut poserait l'articulation au sol loin
        // devant (ou derrière) : elle reste sous le corps, et le haut, pas
        // plus penché que 72° devant, 74° derrière, rentre d'autant.
        let tilt: f64 = if leg.front { 1.25 } else { 1.3 };
        let full = (l1 * l1 - drop * drop).max(0.0).sqrt();
        let shortest = (1.0 - MAX_TUCK) * l1;
        let reach = full
            .min(drop * tilt.tan())
            .min(fold.room)
            .max((shortest * shortest - drop * drop).max(0.0).sqrt());
        let to = Vec2::new(if leg.front { reach } else { -reach }, drop);
        let tuck = (1.0 - to.hypot() / l1).clamp(0.0, MAX_TUCK);
        let first = wrap(angle(to) - chain.first_rest);
        // Le canon replié, le sabot un peu relevé vers le corps.
        let cannon = if leg.front {
            PI + fold.rise
        } else {
            -fold.rise
        };
        let second = wrap(cannon - angle(chain.tip - chain.joint) - first);
        Bend {
            first,
            second,
            stretch: 1.0,
            tuck,
        }
    }
}

/// Un appui : sa part du cycle, et la demi-longueur qu'y balaie le sabot
/// (unités du dessin).
#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) struct Step {
    pub duty: f64,
    pub half: f64,
}

/// Le chemin d'un sabot : `at`, où la patte en est de son cycle (fraction) ;
/// `held`, l'appui qu'elle fait ou vient de faire ; `next`, celui de l'allure
/// en cours, où elle se posera. Rend sa place, en avant (positif) ou en
/// arrière de sa place au repos (unités du dessin), sa levée (0 à 1), et la
/// part du vol (0 à l'appui, jusqu'à 1 au milieu du vol).
///
/// À l'appui, il recule régulièrement de toute la longueur de l'appui :
/// posé, il ne glisse pas. En vol, il part et arrive à la vitesse de
/// l'appui (une courbe d'Hermite) : il quitte le sol et s'y pose immobile,
/// sans racler l'herbe. Il recule donc encore un peu sous le corps en se
/// levant, et revient un peu en arrière avant de se poser, comme la patte
/// d'un vrai mouton (d'autant plus que l'appui est court : 4 % de la
/// demi-foulée au pas, 26 % au galop).
pub(crate) fn hoof_path(at: f64, held: Step, next: Step, front: bool) -> (f64, f64, f64) {
    let t = at.rem_euclid(1.0);
    let duty = held.duty.clamp(0.05, 0.95);
    if t < duty {
        return (held.half * (1.0 - 2.0 * t / duty), 0.0, 0.0);
    }
    let s = (t - duty) / (1.0 - duty);
    // Les vitesses des deux appuis, par part du vol.
    let leave = -2.0 * held.half / duty * (1.0 - duty);
    let land = -2.0 * next.half / next.duty.max(0.05) * (1.0 - duty);
    let (s2, s3) = (s * s, s * s * s);
    let forward = -held.half * (2.0 * s3 - 3.0 * s2 + 1.0)
        + leave * (s3 - 2.0 * s2 + s)
        + next.half * (3.0 * s2 - 2.0 * s3)
        + land * (s3 - s2);
    let (_, raised, _) = stepping(2.0 * PI * t, duty, 1.0, front);
    (forward, raised, (PI * s).sin())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn leg(front: bool) -> Chain {
        if front {
            Chain::new(
                Point::new(100.0, 88.0),
                Point::new(101.0, 100.0),
                Point::new(100.5, 112.0),
            )
        } else {
            Chain::new(
                Point::new(49.0, 88.0),
                Point::new(46.0, 100.0),
                Point::new(49.0, 112.0),
            )
        }
    }

    #[test]
    fn a_two_bone_chain_reaches_its_target_and_bends_the_natural_way() {
        for front in [true, false] {
            let chain = leg(front);
            // Au repos, rien ne bouge.
            let rest = chain.reach(chain.root, chain.tip, natural(front), 0.0, 1.1, 0.07);
            assert!(
                rest.first.abs() < 1e-9 && rest.second.abs() < 1e-9,
                "{rest:?}"
            );
            // Le sabot levé de 5 et avancé de 3 : il y est, et le genou plie
            // du bon côté (le carpe en avant de la ligne, le jarret en arrière).
            let target = chain.tip + Vec2::new(3.0, -5.0);
            let bend = chain.reach(chain.root, target, natural(front), 0.0, 1.1, 0.0);
            let (knee, tip) = chain.place(chain.root, bend);
            assert!((tip - target).hypot() < 1e-6, "{tip:?}");
            let line = target - chain.root;
            let side = line.cross(knee - chain.root);
            if front {
                assert!(side < 0.0, "le carpe pointe vers l'avant");
            } else {
                assert!(side > 0.0, "le jarret pointe vers l'arrière");
            }
            // Trop loin : la chaîne s'étire, puis pointe vers la cible.
            let far = chain.root + (chain.tip - chain.root) * 1.5;
            let bend = chain.reach(chain.root, far, natural(front), 0.0, 1.1, 0.0);
            assert!((bend.stretch - 1.1).abs() < 1e-9);
            // Un peu plus près qu'au repos, à l'appui : elle se tasse sans plier.
            let near = chain.root + (chain.tip - chain.root) * 0.97;
            let bend = chain.reach(chain.root, near, natural(front), 0.0, 1.1, 0.07);
            assert!(bend.second.abs() < 1e-9 && (bend.stretch - 0.97).abs() < 1e-9);
        }
    }

    /// Un agneau couché (les proportions de l'agnelle Southdown) : le sol à
    /// 64, le corps de 7,8 à 55,9, l'épaule et la hanche descendues avec
    /// lui à quelques unités du sol, des pattes plus longues que ça ;
    /// `headroom` : de combien le haut de la patte peut glisser sous le dos.
    fn lamb_leg(front: bool, headroom: f64) -> (LegRest, Point) {
        let (chain, upper, lower, hip) = if front {
            (
                Chain::new(
                    Point::new(51.0, 32.0),
                    Point::new(50.9, 48.7),
                    Point::new(51.2, 64.0),
                ),
                Rect::new(49.8, 30.0, 52.0, 50.4),
                Rect::new(49.5, 47.0, 52.9, 64.0),
                Point::new(51.0, 57.5),
            )
        } else {
            (
                Chain::new(
                    Point::new(16.0, 28.0),
                    Point::new(13.3, 46.9),
                    Point::new(14.6, 64.0),
                ),
                Rect::new(11.0, 24.0, 19.0, 49.1),
                Rect::new(12.3, 45.0, 16.9, 64.0),
                Point::new(16.0, 53.5),
            )
        };
        let body = Rect::new(7.8, 7.4, 55.9, 38.2);
        let leg = LegRest::new(
            chain,
            (front, true),
            Some(upper),
            lower,
            Some(body),
            headroom,
            &[],
        );
        (leg, hip)
    }

    fn lying_drive() -> LegDrive {
        let still = mouflon_core::PoseSignals::default();
        LegDrive {
            gait: Gait::of(&still),
            phase: 0.0,
            duty: 0.64,
            stride: 50.0,
            stride_walk: 50.0,
            legs: still.legs,
            u: 0.57,
            ground: 64.0,
            air: 0.0,
            tuck: 0.0,
            reach: 0.0,
            leaping: 0.0,
            flight: 0.0,
            loose: 1.0,
            dangle: 0.0,
            swing: 0.0,
            splay: 0.0,
            front_down: 1.0,
            back_down: 1.0,
            act: Gesturing::default(),
            untilt: Affine::IDENTITY,
        }
    }

    #[test]
    fn a_lambs_long_legs_fold_under_its_body_with_the_hooves_tucked_in() {
        let drive = lying_drive();
        let (front_end, back_end) = (55.9, 7.8);
        for front in [true, false] {
            let (leg, hip) = lamb_leg(front, f64::INFINITY);
            let bend = drive.bend(&leg, hip);
            let (knee, hoof) = leg.chain.place(hip, bend);
            let r = leg.fold.knee;
            // Le haut rentre sous le corps, de près de la moitié de sa
            // longueur ou plus.
            assert!(bend.tuck > 0.4, "{bend:?}");
            // L'articulation posée au sol, sous le corps : le carpe ne passe
            // pas le devant de la poitrine, le jarret le bout de la fesse.
            assert!((knee.y + r - drive.ground).abs() < 1e-9, "{knee:?}");
            if front {
                assert!(
                    knee.x + r <= front_end + 1e-9 && knee.x >= hip.x,
                    "{knee:?}"
                );
                assert!(hoof.x < knee.x - 10.0, "{knee:?} {hoof:?}");
            } else {
                assert!(knee.x - r >= back_end - 1e-9 && knee.x <= hip.x, "{knee:?}");
                assert!(hoof.x > knee.x + 10.0, "{knee:?} {hoof:?}");
            }
            // Le sabot rentré sous le ventre, relevé vers lui, jamais sous
            // le sol : la semelle (le bas de la boîte du bas) tourne avec le
            // canon.
            assert!(hoof.x > back_end && hoof.x < front_end, "{hoof:?}");
            assert!(hoof.y < knee.y, "{knee:?} {hoof:?}");
            let lower = if front {
                Rect::new(49.5, 47.0, 52.9, 64.0)
            } else {
                Rect::new(12.3, 45.0, 16.9, 64.0)
            };
            for corner in [
                Point::new(lower.x0, lower.y1),
                Point::new(lower.x1, lower.y1),
            ] {
                let sole = knee + rotate(corner - leg.chain.joint, bend.first + bend.second);
                assert!(sole.y <= drive.ground + 1e-9, "{sole:?}");
            }
        }
    }

    #[test]
    fn a_tucked_upper_leg_slides_under_the_body_as_far_as_the_back_allows_then_shortens() {
        let drive = lying_drive();
        let (under, hip) = lamb_leg(true, f64::INFINITY);
        let bend = drive.bend(&under, hip);
        let needed = bend.tuck * under.chain.l1;
        let (over, _) = lamb_leg(true, 0.0);
        let (between, _) = lamb_leg(true, 0.5 * needed);
        let joint = under.chain.joint;
        let tip = under.chain.tip;
        let root = under.chain.root;
        let (knee, hoof) = under.chain.place(hip, bend);
        let mut tops = Vec::new();
        for leg in [&under, &between, &over] {
            let (upper, lower) = drive.pose(leg, hip);
            // L'articulation et le bas arrivent au même endroit ; le bas
            // reste d'une pièce.
            assert!((upper * joint - knee).hypot() < 1e-9);
            assert!((upper * lower * tip - hoof).hypot() < 1e-9);
            let [a, b, c, d, ..] = (upper * lower).as_coeffs();
            assert!((a * d - b * c - 1.0).abs() < 1e-9 && (a * a + b * b - 1.0).abs() < 1e-9);
            // Le haut de la patte : d'une pièce s'il a la place de glisser,
            // raccourci sinon.
            let [a, b, c, d, ..] = upper.as_coeffs();
            let area = a * d - b * c;
            if leg.fold.headroom >= needed {
                assert!((area - 1.0).abs() < 1e-9);
            } else {
                assert!(area < 1.0 - 1e-3, "{area}");
            }
            // Son bord du haut (l'épaule) remonte au plus de ce qu'il peut.
            let rise = hip.y - (upper * root).y;
            assert!(rise <= leg.fold.headroom + 1e-9, "{rise}");
            tops.push(rise);
        }
        assert!((tops[0] - needed * (knee - hip).y / (knee - hip).hypot()).abs() < 1e-9);
        assert!(tops[2].abs() < 1e-9 && tops[1] > 0.0 && tops[1] < tops[0]);
    }

    #[test]
    fn a_planted_hoof_moves_back_at_the_speed_of_the_body() {
        // À l'appui, le sabot recule d'autant que la phase avance : comme
        // la phase suit la distance parcourue, il reste où il est posé.
        let walk = Step {
            duty: 0.64,
            half: 12.8,
        };
        let a = hoof_path(0.1, walk, walk, true).0;
        let b = hoof_path(0.3, walk, walk, true).0;
        let c = hoof_path(0.5, walk, walk, true).0;
        assert!(((a - b) - (b - c)).abs() < 1e-9);
        assert!(hoof_path(0.2, walk, walk, false).1 == 0.0);
        // En vol, il est levé ; au bout du vol, il retrouve l'avant.
        let mid = hoof_path(0.64 + 0.36 / 2.0, walk, walk, false);
        assert!(mid.1 > 0.9 && mid.2 > 0.9);
        let end = hoof_path(0.9999, walk, walk, true);
        assert!((end.0 - 12.8).abs() < 1e-2);
    }

    #[test]
    fn a_hoof_leaves_and_meets_the_ground_at_the_speed_of_the_stance() {
        // Le sabot quitte le sol et s'y pose à la vitesse de l'appui (par
        // fraction de cycle) : immobile par rapport au sol, il ne racle pas
        // l'herbe, même quand l'appui suivant est celui d'une autre allure.
        let walk = Step {
            duty: 0.64,
            half: 12.8,
        };
        let gallop = Step {
            duty: 0.32,
            half: 12.2,
        };
        for (held, next) in [(walk, walk), (gallop, gallop), (walk, gallop)] {
            let x = |t: f64| hoof_path(t, held, next, true).0;
            let e = 1e-6;
            let stance = -2.0 * held.half / held.duty;
            let after = (x(held.duty + e) - x(held.duty)) / e;
            assert!(
                (after - stance).abs() < 1e-3 * stance.abs(),
                "{after} {stance}"
            );
            let landing = -2.0 * next.half / next.duty;
            let before = (x(1.0 - e) - x(1.0 - 2.0 * e)) / e;
            assert!(
                (before - landing).abs() < 1e-3 * landing.abs(),
                "{before} {landing}"
            );
            // Et il arrive à l'avant de l'appui suivant.
            assert!((x(1.0 - 1e-9) - next.half).abs() < 1e-6);
        }
    }

    #[test]
    fn a_bent_leg_that_stretches_or_gives_still_puts_its_hoof_on_the_target() {
        // Un postérieur plié au jarret, étiré au bout de l'appui, ou tassé
        // et plus plié que d'habitude sous un corps bas : le sabot (le bout
        // de la chaîne) est là où le veut la cinématique inverse, et non à
        // côté (il glisserait à mesure que la patte s'étire).
        let chain = leg(false);
        for target in [
            chain.tip + Vec2::new(-14.0, 0.0),
            chain.tip + Vec2::new(9.0, 2.0),
            chain.root + (chain.tip - chain.root) * 0.9 + Vec2::new(3.0, 0.0),
        ] {
            let bend = chain.reach(
                chain.root,
                target,
                natural(false),
                0.0,
                LEG_STRETCH,
                LEG_GIVE,
            );
            assert!((bend.stretch - 1.0).abs() > 0.01, "{bend:?}");
            let tip =
                first_segment(&chain, chain.root, bend) * second_segment(&chain, bend) * chain.tip;
            let (_, placed) = chain.place(chain.root, bend);
            assert!((tip - placed).hypot() < 1e-9, "{tip:?} {placed:?}");
            assert!((tip - target).hypot() < 1e-6, "{tip:?} {target:?} {bend:?}");
        }
    }
}
