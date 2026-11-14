//! Ce que Mouflon montre quand il se met à jour : l'annonce, le départ du
//! troupeau, puis son retour.
//!
//! Deux mises en scène ([`Scene`]) :
//!
//! - **le mouton annonceur** : une bulle compte au-dessus de l'un d'eux, de
//!   dix à zéro ; à zéro il bêle, et tout le troupeau quitte l'écran en
//!   bondissant (voir [`Flock::bound_away`]) ;
//! - **la soucoupe** : elle arrive par la gauche, haut au-dessus du
//!   troupeau, et va et vient d'un bout à l'autre pendant que le compte
//!   s'affiche dans sa lumière ; à zéro, l'un d'eux bêle, et elle repasse de
//!   gauche à droite : chacun que son faisceau atteint monte dans la
//!   lumière et la suit ; puis elle les aspire tous ensemble, et s'en va.
//!
//! Tant que le compte tourne, on peut tout remettre à plus tard
//! ([`Farewell::postpone`]) : la bulle s'en va, ou la soucoupe repart seule,
//! et le troupeau reste.
//!
//! Au retour ([`Welcome`]), le troupeau revient. La soucoupe, si c'est elle
//! qui l'avait pris, arrive par la gauche avec tout le monde dans sa
//! lumière, puis repasse de gauche à droite et repose chacun à sa place.
//! L'un d'eux montre ensuite un moment le numéro de la nouvelle version dans
//! une bulle.
//!
//! Tout est déterministe : rien n'est tiré au sort ici.

use crate::math::{Rect, Vec2};
use crate::pet::{Flock, Input, Pet, Support};
use crate::surprise::{SAUCER_HEIGHT, SAUCER_WIDTH, SaucerView};
use crate::world::{Screen, World};

/// Le compte à rebours (secondes).
pub const COUNT_FROM: u32 = 10;
/// Le temps du bêlement, entre le zéro et le départ (secondes).
const BLEAT_TIME: f32 = 0.95;
/// La soucoupe : son arrivée, l'allumage du faisceau, son départ (secondes).
const SAUCER_ARRIVES: f32 = 1.6;
const BEAM_ON: f32 = 0.9;
const SAUCER_LEAVES: f32 = 1.2;
/// Sa vitesse quand elle repasse pour prendre le troupeau, puis pour le
/// reposer (points par seconde).
const SWEEP_SPEED: f32 = 520.0;
const SET_SPEED: f32 = 380.0;
/// Celui que son faisceau atteint monte dans sa lumière en ce temps-là ;
/// celui qu'elle va chercher ailleurs (sur un autre écran), en celui-ci ;
/// puis elle les aspire tous ensemble en celui-là (secondes).
const RISE: f32 = 0.75;
const RISE_FAR: f32 = 1.3;
const SUCK: f32 = 0.8;
/// Celui qu'elle repose descend jusqu'au sol en ce temps-là (secondes).
const DESCEND: f32 = 0.8;
/// La bulle : son entrée, sa sortie, et le temps qu'elle reste au retour.
const BUBBLE_IN: f32 = 0.25;
const BUBBLE_OUT: f32 = 0.3;
const LABEL_TIME: f32 = 6.0;
/// Le faisceau : un cône qui sort du ventre de la soucoupe (sa
/// demi-largeur à la bouche, en points), s'ouvre de tant par point de
/// descente, et garde au sol une demi-largeur entre ces bornes (points).
pub const BEAM_MOUTH: f32 = 34.0;
const BEAM_SPREAD: f32 = 0.36;
const BEAM_FLOOR: (f32, f32) = (150.0, 300.0);
/// Le faisceau prend celui dont le milieu est dans cette part de sa
/// largeur, à sa hauteur.
const CATCH: f32 = 0.9;
/// Sa course va d'un bout à l'autre du troupeau, plus cette marge (points),
/// sur au moins cette part de la largeur de l'écran.
const SWEEP_MARGIN: f32 = 90.0;
const SWEEP_MIN: f32 = 0.4;
/// La soucoupe ne vient que si elle peut se tenir au moins à cette hauteur
/// au-dessus du plus haut des animaux posés (de sa bouche à son dos, en
/// points).
const MIN_DROP: f32 = 90.0;
/// La place qu'il faut à une bulle au-dessus de sa pointe, et de chaque
/// côté (points).
const BUBBLE_ROOM: (f32, f32) = (74.0, 78.0);
/// La soucoupe se tient au moins à cette hauteur au-dessus du plus haut des
/// animaux, et du sol (points).
const ABOVE_FLOCK: f32 = 170.0;
const ABOVE_FLOOR: f32 = 360.0;
/// Le troupeau bouge pendant le compte : la course et la hauteur de la
/// soucoupe le suivent, avec ce retard (secondes).
const FOLLOW: f32 = 0.35;
/// Le plus long pas d'un coup (secondes) : après un arrêt (l'ordinateur
/// dormait), l'annonce reprend où elle en était au lieu de sauter.
const MAX_STEP: f32 = 0.25;

/// La mise en scène.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Scene {
    /// Le mouton annonceur.
    Herald,
    /// La soucoupe.
    Saucer,
}

/// Une bulle au-dessus d'un animal, avec un nombre dedans.
#[derive(Clone, Debug, PartialEq)]
pub struct BubbleView {
    /// La pointe de la bulle, au-dessus de la tête (repère du monde).
    pub tip: Vec2,
    pub text: String,
    /// Sa taille (1 : la taille ordinaire) et son opacité.
    pub scale: f32,
    pub alpha: f32,
    /// Des étincelles autour d'elle : leur phase (tours), ou aucune.
    pub sparkle: Option<f32>,
}

/// Un nombre dans la lumière, sous la soucoupe.
#[derive(Clone, Debug, PartialEq)]
pub struct CountView {
    pub center: Vec2,
    pub text: String,
    pub scale: f32,
    pub alpha: f32,
}

/// Le faisceau large de la soucoupe : un cône de lumière, de sa bouche
/// (`top`) jusqu'au sol.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct BeamView {
    pub top: Vec2,
    pub length: f32,
    /// Sa demi-largeur au sol.
    pub half_width: f32,
    pub alpha: f32,
    /// La phase des anneaux qui y descendent (tours).
    pub rings: f32,
}

impl BeamView {
    /// Sa demi-largeur à cette profondeur (0 : sa bouche ; 1 : le sol).
    #[must_use]
    pub fn half_width_at(&self, depth: f32) -> f32 {
        let mouth = BEAM_MOUTH.min(self.half_width);
        mouth + (self.half_width - mouth) * depth.clamp(0.0, 1.0)
    }
}

/// Ce qu'il y a à dessiner en plus du troupeau.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct ShowView {
    pub beam: Option<BeamView>,
    pub saucer: Option<SaucerView>,
    pub count: Option<CountView>,
    pub bubble: Option<BubbleView>,
}

/// Comment l'annonce a fini.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Outcome {
    /// Le troupeau est parti : la place est libre.
    Gone,
    /// Remis à plus tard : le troupeau est resté.
    Postponed,
}

fn smooth(a: f32, b: f32, x: f32) -> f32 {
    let t = ((x - a) / (b - a)).clamp(0.0, 1.0);
    t * t * (3.0 - 2.0 * t)
}

/// La pointe d'une bulle au-dessus de la tête de cet animal. Elle reste sur
/// son écran : trop près du haut ou d'un côté, la bulle se pose sur lui
/// plutôt que de sortir.
fn above(pet: &Pet, world: &World) -> Vec2 {
    let p = pet.def().physics;
    let pos = pet.drawn_position();
    let tip = Vec2::new(
        pos.x + pet.facing() * p.width * 0.34,
        pos.y - p.height * 1.04,
    );
    let Some(screen) = world.screen_at(pet.position() - Vec2::new(0.0, 1.0)) else {
        return tip;
    };
    let frame = screen.frame;
    let side = BUBBLE_ROOM.1.min(frame.w * 0.5);
    Vec2::new(
        tip.x.clamp(frame.left() + side, frame.right() - side),
        tip.y.max(frame.top() + BUBBLE_ROOM.0),
    )
}

/// A-t-il la place d'une bulle au-dessus de la tête, sur son écran ?
fn roomy(pet: &Pet, world: &World) -> bool {
    let p = pet.def().physics;
    let pos = pet.position();
    world
        .screen_at(pos - Vec2::new(0.0, 1.0))
        .is_some_and(|screen| {
            let frame = screen.frame;
            pos.y - p.height * 1.04 >= frame.top() + BUBBLE_ROOM.0
                && pos.x >= frame.left() + BUBBLE_ROOM.1
                && pos.x <= frame.right() - BUBBLE_ROOM.1
        })
}

/// Peut-il porter la bulle, ou bêler pour les autres ?
fn fit(pet: &Pet) -> bool {
    !pet.is_child() && !pet.is_leaving() && !pet.is_finished()
}

/// Celui qui annonce : posé, avec la place d'une bulle au-dessus de lui, le
/// plus près de `focus` ; à défaut, posé ; à défaut, n'importe lequel.
fn herald(flock: &Flock, world: &World, focus: Vec2) -> Option<u32> {
    let near = |pet: &&Pet| (pet.position() - focus).length();
    let grounded = |pet: &&Pet| matches!(pet.support(), Support::Ground { .. });
    let pets = || flock.pets().iter().filter(|pet| fit(pet));
    let nearest = |pets: &mut dyn Iterator<Item = &Pet>| {
        pets.min_by(|a, b| near(a).total_cmp(&near(b))).map(Pet::id)
    };
    nearest(&mut pets().filter(grounded).filter(|pet| roomy(pet, world)))
        .or_else(|| nearest(&mut pets().filter(grounded)))
        .or_else(|| nearest(&mut pets()))
}

/// En douceur vers la fin.
fn ease_out(u: f32) -> f32 {
    let u = u.clamp(0.0, 1.0);
    1.0 - (1.0 - u) * (1.0 - u)
}

/// En douceur depuis le début.
fn ease_in(u: f32) -> f32 {
    let u = u.clamp(0.0, 1.0);
    u * u
}

/// Le corps d'un animal : ses bords, et son haut.
type Body = (f32, f32, f32);

/// Les bords et le haut de tous ces corps ensemble (`None` : aucun).
fn extent(bodies: &[Body]) -> Option<Body> {
    bodies
        .iter()
        .copied()
        .reduce(|a, b| (a.0.min(b.0), a.1.max(b.1), a.2.min(b.2)))
}

/// La bouche du faisceau, sous la soucoupe en `pos`.
fn mouth(pos: Vec2) -> Vec2 {
    pos + Vec2::new(0.0, SAUCER_HEIGHT * 0.3)
}

/// La demi-largeur au sol du faisceau qui sort de sa bouche à la hauteur
/// `top` et descend jusqu'à `floor` : un cône d'angle fixe, borné.
fn beam_width(top: f32, floor: f32) -> f32 {
    (BEAM_MOUTH + (floor - top).max(0.0) * BEAM_SPREAD).clamp(BEAM_FLOOR.0, BEAM_FLOOR.1)
}

/// Sa demi-largeur à la hauteur `y`.
fn beam_width_at(top: f32, floor: f32, y: f32) -> f32 {
    let depth = ((y - top) / (floor - top).max(1.0)).clamp(0.0, 1.0);
    BEAM_MOUTH + (beam_width(top, floor) - BEAM_MOUTH) * depth
}

/// Le faisceau de la soucoupe en `pos`, jusqu'au sol.
fn beam(pos: Vec2, floor: f32, alpha: f32, time: f32) -> Option<BeamView> {
    let top = mouth(pos);
    (alpha > 0.005 && floor > top.y + 1.0).then(|| BeamView {
        top,
        length: floor - top.y,
        half_width: beam_width(top.y, floor),
        alpha,
        rings: time * 0.55,
    })
}

/// La place du `rank`-ième animal dans la lumière de la soucoupe en `pos`
/// (le milieu de son corps) : chacun à sa profondeur entre sa bouche et
/// `low` (tous y sont plus haut que là où ils étaient, ou que là où elle les
/// pose), un peu d'un côté ou de l'autre, qui flotte.
fn slot(pos: Vec2, floor: f32, low: f32, rank: usize, time: f32) -> Vec2 {
    let top = mouth(pos);
    let k = rank as f32;
    let depth = 0.26 + 0.3 * (k * 0.618_034).fract();
    let side = ((k * 0.381_966 + 0.19).fract() - 0.5) * 2.0;
    let y = top.y + depth * (low.min(floor) - top.y).max(1.0);
    Vec2::new(
        top.x + side * beam_width_at(top.y, floor, y) * 0.5,
        y + (time * 1.7 + k * 1.3).sin() * 4.0,
    )
}

/// Les pieds du plus haut des animaux posés dans `room` (`None` : aucun).
fn highest_feet(flock: &Flock, room: Rect) -> Option<f32> {
    flock
        .pets()
        .iter()
        .filter(|pet| fit(pet) && matches!(pet.support(), Support::Ground { .. }))
        .map(Pet::position)
        .filter(|feet| room.contains(*feet - Vec2::new(0.0, 1.0)))
        .map(|feet| feet.y)
        .reduce(f32::min)
}

/// L'écran au-dessus duquel la soucoupe vole.
#[derive(Clone, Copy, Debug, PartialEq)]
struct Sky {
    room: Rect,
    /// Le haut de l'écran.
    top: f32,
}

impl Sky {
    fn of(screen: &Screen) -> Self {
        Self {
            room: screen.room(),
            top: screen.frame.top(),
        }
    }

    /// Le sol : le bas de son faisceau.
    fn floor(&self) -> f32 {
        self.room.bottom()
    }

    /// Cette place est-elle sous elle, sur cet écran ?
    fn holds(&self, x: f32) -> bool {
        x >= self.room.left() && x <= self.room.right()
    }

    /// Les bornes de sa course : elle reste tout entière sur l'écran.
    fn bounds(&self) -> (f32, f32) {
        let reach = SAUCER_WIDTH * 0.5 + 8.0;
        let (lo, hi) = (self.room.left() + reach, self.room.right() - reach);
        if lo <= hi {
            (lo, hi)
        } else {
            (self.room.center().x, self.room.center().x)
        }
    }

    /// La hauteur où elle se tient au-dessus d'animaux dont le plus haut a
    /// son dos à `top` ; plus haut encore de `lift`.
    fn altitude(&self, top: f32, lift: f32) -> f32 {
        let floor = self.floor();
        (top - ABOVE_FLOCK - lift)
            .min(floor - ABOVE_FLOOR - lift)
            .max(self.top + SAUCER_HEIGHT * 0.5 + 36.0)
            .min(floor - SAUCER_HEIGHT * 2.0)
    }

    /// Sa course au-dessus de ce qui va de `left` à `right` : d'un bout à
    /// l'autre, plus une marge, sur au moins une part de l'écran, sans en
    /// sortir.
    fn course(&self, left: f32, right: f32) -> (f32, f32) {
        let (lo, hi) = self.bounds();
        let least = (self.room.w * SWEEP_MIN).min(hi - lo);
        let (mut a, mut b) = (left - SWEEP_MARGIN, right + SWEEP_MARGIN);
        if b - a < least {
            let middle = (a + b) * 0.5;
            (a, b) = (middle - least * 0.5, middle + least * 0.5);
        }
        if a < lo {
            b += lo - a;
            a = lo;
        }
        if b > hi {
            a -= b - hi;
            b = hi;
        }
        let a = a.max(lo);
        (a, b.min(hi).max(a))
    }

    /// Hors de l'écran, au-dessus, d'un côté de `x` (`side` : -1 à gauche,
    /// 1 à droite).
    fn away(&self, x: f32, side: f32) -> Vec2 {
        Vec2::new(x + side * self.room.w * 0.5, self.top - SAUCER_HEIGHT * 2.0)
    }
}

/// La soucoupe : où elle est, comment elle penche ; quand elle repart,
/// depuis quand et d'où.
#[derive(Clone, Copy, Debug, PartialEq)]
struct Ufo {
    pos: Vec2,
    tilt: f32,
    leaving: Option<(f32, Vec2)>,
}

impl Ufo {
    fn at(pos: Vec2) -> Self {
        Self {
            pos,
            tilt: 0.0,
            leaving: None,
        }
    }

    /// Elle est là maintenant, penchée dans le sens de sa course.
    fn put(&mut self, pos: Vec2, dt: f32) {
        if dt > 0.0 {
            let lean = ((pos.x - self.pos.x) / dt / 1500.0).clamp(-0.3, 0.3);
            self.tilt += (lean - self.tilt) * (1.0 - (-dt / 0.25).exp());
        }
        self.pos = pos;
    }

    /// Elle glisse vers `goal`, avec un peu de retard.
    fn glide(&mut self, goal: Vec2, dt: f32) {
        self.put(self.pos.lerp(goal, 1.0 - (-dt / FOLLOW).exp()), dt);
    }

    /// Elle file vers `goal` à `speed`, sans le dépasser ; `true` : elle y
    /// est.
    fn travel(&mut self, goal: Vec2, speed: f32, dt: f32) -> bool {
        let to = goal - self.pos;
        let far = to.length();
        let step = speed * dt;
        if far <= step.max(0.5) {
            self.put(goal, dt);
            true
        } else {
            self.put(self.pos + to * (step / far), dt);
            false
        }
    }

    /// Elle repart (une seule fois) : maintenant, de là où elle est.
    fn leave(&mut self, time: f32) {
        if self.leaving.is_none() {
            self.leaving = Some((time, self.pos));
        }
    }

    /// Elle s'éloigne, vers le haut et la droite ; `true` : elle est partie.
    fn fly_away(&mut self, sky: &Sky, time: f32, dt: f32) -> bool {
        let Some((since, from)) = self.leaving else {
            return false;
        };
        let u = ((time - since) / SAUCER_LEAVES).clamp(0.0, 1.0);
        self.put(from.lerp(sky.away(from.x, 1.0), u * u), dt);
        time - since >= SAUCER_LEAVES
    }

    /// Partie depuis assez longtemps pour ne plus se voir.
    fn gone(&self, time: f32) -> bool {
        self.leaving
            .is_some_and(|(since, _)| time - since >= SAUCER_LEAVES)
    }

    /// Son faisceau s'éteint quand elle repart.
    fn fade(&self, time: f32) -> f32 {
        self.leaving
            .map_or(1.0, |(since, _)| 1.0 - smooth(0.0, 0.3, time - since))
    }

    fn view(&self, time: f32) -> SaucerView {
        SaucerView {
            pos: self.pos + Vec2::new(0.0, (time * 2.1).sin() * 2.5),
            tilt: self.tilt,
            beam: 0.0,
            beam_len: 0.0,
            lights: time * 1.3,
            smoke: 0.0,
            smoke_time: 0.0,
        }
    }
}

/// L'écran où sont le plus d'animaux (à défaut, celui de `focus`, sinon le
/// premier).
fn busiest<'w>(flock: &Flock, world: &'w World, focus: Vec2) -> Option<&'w Screen> {
    let screens = world.screens();
    let count = |screen: &Screen| {
        let room = screen.room();
        flock
            .pets()
            .iter()
            .filter(|pet| fit(pet) && room.contains(pet.position() - Vec2::new(0.0, 1.0)))
            .count()
    };
    screens
        .iter()
        .max_by_key(|screen| (count(screen), usize::from(screen.room().contains(focus))))
        .filter(|screen| count(screen) > 0)
        .or_else(|| world.screen_at(focus))
        .or(screens.first())
}

/// Les corps de ceux qui sont sur cet écran (`settled` : seulement ceux qui
/// sont posés).
fn bodies(flock: &Flock, room: Rect, settled: bool) -> Vec<Body> {
    flock
        .pets()
        .iter()
        .filter(|pet| fit(pet))
        .filter(|pet| !settled || matches!(pet.support(), Support::Ground { .. }))
        .filter_map(|pet| {
            let p = pet.def().physics;
            let pos = pet.position();
            room.contains(pos - Vec2::new(0.0, 1.0)).then_some((
                pos.x - p.width * 0.5,
                pos.x + p.width * 0.5,
                pos.y - p.height,
            ))
        })
        .collect()
}

#[derive(Clone, Copy, Debug, PartialEq)]
enum Step {
    /// Le compte tourne : ce qu'il en reste (secondes).
    Count(f32),
    /// Zéro : il bêle.
    Bleat(f32),
    /// Le troupeau s'en va.
    Leave(f32),
    /// Remis à plus tard : la bulle s'efface, la soucoupe repart.
    Cancel(f32),
    Done(Outcome),
}

/// Un animal que le faisceau a pris : d'où il monte (le milieu de son
/// corps), depuis quand, en combien de temps, et son rang dans la lumière.
#[derive(Clone, Copy, Debug)]
struct Caught {
    id: u32,
    from: Vec2,
    at: f32,
    rise: f32,
    rank: usize,
}

/// La soucoupe d'une annonce.
#[derive(Clone, Debug)]
struct Sweep {
    sky: Sky,
    ufo: Ufo,
    /// D'où elle arrive : au-dessus de l'écran, à gauche.
    from: Vec2,
    /// Sa hauteur et sa course, qui suivent le troupeau pendant le compte.
    altitude: f32,
    course: (f32, f32),
    /// Ceux que son faisceau a pris.
    caught: Vec<Caught>,
    /// Elle a fini de repasser : elle prend ceux qui restent, où qu'ils
    /// soient. Puis elle les aspire tous, depuis `sucking`.
    passed: bool,
    sucking: Option<f32>,
    /// Elle les tient plus haut que ceci : les pieds du plus haut de ceux
    /// qui sont posés (ou le sol).
    low: f32,
}

impl Sweep {
    /// Au-dessus des animaux de cet écran, si elle peut les dominer tous :
    /// pas avec un animal posé tout en haut de l'écran (celui qui tombe
    /// encore, ou qu'on tient en l'air, ne compte pas).
    fn over(flock: &Flock, screen: &Screen) -> Option<Self> {
        let sky = Sky::of(screen);
        let (left, right, top) = extent(&bodies(flock, sky.room, false))?;
        let altitude = sky.altitude(top, 0.0);
        let lowest = mouth(Vec2::new(0.0, altitude)).y + MIN_DROP;
        if bodies(flock, sky.room, true)
            .iter()
            .any(|body| body.2 < lowest)
        {
            return None;
        }
        let course = sky.course(left, right);
        let from = sky.away(course.0, -1.0);
        let low = highest_feet(flock, sky.room).map_or(sky.floor(), |feet| feet.min(sky.floor()));
        Some(Self {
            sky,
            ufo: Ufo::at(from),
            from,
            altitude,
            course,
            caught: Vec::new(),
            passed: false,
            sucking: None,
            low,
        })
    }

    fn has(&self, id: u32) -> bool {
        self.caught.iter().any(|caught| caught.id == id)
    }

    /// Pendant le compte (`spent` : ce qui en est passé) : elle arrive au
    /// bout gauche de sa course, va jusqu'au bout droit et revient ; au
    /// zéro, elle est à gauche.
    fn patrol(&mut self, flock: &Flock, spent: f32, dt: f32) {
        if let Some((left, right, top)) = extent(&bodies(flock, self.sky.room, false)) {
            let k = 1.0 - (-dt / FOLLOW).exp();
            self.altitude += (self.sky.altitude(top, 0.0) - self.altitude) * k;
            let (a, b) = self.sky.course(left, right);
            self.course.0 += (a - self.course.0) * k;
            self.course.1 += (b - self.course.1) * k;
        }
        if let Some(feet) = highest_feet(flock, self.sky.room) {
            self.low = feet.min(self.sky.floor());
        }
        let (a, b) = self.course;
        let at = if spent < SAUCER_ARRIVES {
            self.from.lerp(
                Vec2::new(a, self.altitude),
                smooth(0.0, SAUCER_ARRIVES, spent),
            )
        } else {
            let u =
                ((spent - SAUCER_ARRIVES) / (COUNT_FROM as f32 - SAUCER_ARRIVES)).clamp(0.0, 1.0);
            let x = a + (b - a) * (1.0 - (u * std::f32::consts::TAU).cos()) * 0.5;
            Vec2::new(x, self.altitude)
        };
        self.ufo.put(at, dt);
    }

    /// Après le zéro : elle repasse de gauche à droite, et chacun que son
    /// faisceau atteint monte dans sa lumière ; elle va ensuite chercher
    /// ceux qui restent (sur un autre écran) ; quand tous sont là-haut, elle
    /// les aspire ensemble et s'en va. `true` : elle est partie.
    fn take(&mut self, flock: &mut Flock, world: &World, time: f32, dt: f32) -> bool {
        let floor = self.sky.floor();
        let room = self.sky.room;
        let on_screen = |pet: &Pet| room.contains(pet.position() - Vec2::new(0.0, 1.0));
        // Ceux qu'elle prend : sur son écran, ceux qui sont dans sa lumière ;
        // une fois passée, tous ceux qui restent.
        if self.sucking.is_none() {
            let top = mouth(self.ufo.pos);
            for pet in flock.pets_mut() {
                if !fit(pet) || self.has(pet.id()) {
                    continue;
                }
                let middle = pet.position() - Vec2::new(0.0, pet.def().physics.height * 0.5);
                let lit = on_screen(&*pet)
                    && middle.y > top.y
                    && (middle.x - top.x).abs() <= beam_width_at(top.y, floor, middle.y) * CATCH;
                if !lit && !self.passed {
                    continue;
                }
                if pet.is_held() {
                    pet.release(world);
                }
                pet.grab(middle, world);
                let rank = self.caught.len();
                self.caught.push(Caught {
                    id: pet.id(),
                    from: middle,
                    at: time,
                    rise: if lit { RISE } else { RISE_FAR },
                    rank,
                });
            }
        }
        // Elle avance jusqu'au plus à droite de ceux qu'elle n'a pas encore
        // pris sur son écran ; quand il n'y en a plus, elle a fini de passer.
        if !self.passed {
            let ahead = flock
                .pets()
                .iter()
                .filter(|&pet| fit(pet) && on_screen(pet) && !self.has(pet.id()))
                .map(|pet| pet.position().x + pet.def().physics.width * 0.5)
                .reduce(f32::max);
            self.passed = match ahead {
                Some(x) => {
                    let goal = (x + SWEEP_MARGIN * 0.5)
                        .min(self.sky.bounds().1)
                        .max(self.ufo.pos.x);
                    self.ufo
                        .travel(Vec2::new(goal, self.altitude), SWEEP_SPEED, dt)
                }
                None => true,
            };
        }
        // Ceux qu'elle tient montent à leur place dans sa lumière et y
        // flottent ; quand tous y sont, elle les aspire ensemble.
        if self.passed
            && self.sucking.is_none()
            && self
                .caught
                .iter()
                .all(|caught| time >= caught.at + caught.rise)
        {
            self.sucking = Some(time);
        }
        let top = mouth(self.ufo.pos);
        let suck = self.sucking.map(|since| ease_in((time - since) / SUCK));
        for caught in &self.caught {
            let Some(pet) = flock.get_mut(caught.id) else {
                continue;
            };
            let place = slot(self.ufo.pos, floor, self.low, caught.rank, time);
            let at = caught
                .from
                .lerp(place, ease_out((time - caught.at) / caught.rise));
            pet.drag_to(suck.map_or(at, |u| at.lerp(top + Vec2::new(0.0, 12.0), u)));
        }
        // Aspirés : il n'y a plus personne, elle s'en va.
        if self.sucking.is_some_and(|since| time - since >= SUCK) {
            if !flock.pets().is_empty() {
                flock.clear();
            }
            self.ufo.leave(time);
        }
        let sky = self.sky;
        self.ufo.fly_away(&sky, time, dt)
    }

    /// L'opacité de cet animal : il s'efface en entrant dans la soucoupe.
    fn opacity(&self, id: u32, time: f32) -> f32 {
        match self.sucking {
            Some(since) if self.has(id) => 1.0 - smooth(0.3, 1.0, (time - since) / SUCK),
            _ => 1.0,
        }
    }
}

/// L'annonce d'une mise à jour, jusqu'au départ du troupeau.
#[derive(Clone, Debug)]
pub struct Farewell {
    scene: Scene,
    step: Step,
    time: f32,
    herald: Option<u32>,
    /// La soucoupe, s'il y en a une.
    sweep: Option<Sweep>,
    /// Le compte au moment où on l'a remis à plus tard.
    stopped_at: f32,
}

impl Farewell {
    /// L'annonce commence. `focus` : là où l'utilisateur regarde (le milieu
    /// de son écran), pour choisir qui annonce. Rien à montrer (`None`) si
    /// le troupeau est vide.
    #[must_use]
    pub fn new(scene: Scene, flock: &mut Flock, world: &World, focus: Vec2) -> Option<Self> {
        let herald = herald(flock, world, focus)?;
        flock.calm(world);
        // Elle ne vient que si elle peut dominer tout le monde : avec un
        // animal perché tout en haut de l'écran, c'est le mouton qui annonce.
        let sweep = (scene == Scene::Saucer)
            .then(|| busiest(flock, world, focus).copied())
            .flatten()
            .and_then(|screen| Sweep::over(flock, &screen));
        let scene = if sweep.is_some() {
            Scene::Saucer
        } else {
            Scene::Herald
        };
        Some(Self {
            scene,
            step: Step::Count(COUNT_FROM as f32),
            time: 0.0,
            herald: Some(herald),
            sweep,
            stopped_at: 0.0,
        })
    }

    #[must_use]
    pub fn scene(&self) -> Scene {
        self.scene
    }

    /// Le compte tourne-t-il encore (on peut remettre à plus tard) ?
    #[must_use]
    pub fn is_counting(&self) -> bool {
        matches!(self.step, Step::Count(_))
    }

    /// Fini : le troupeau est parti, ou tout est remis à plus tard.
    #[must_use]
    pub fn outcome(&self) -> Option<Outcome> {
        match self.step {
            Step::Done(outcome) => Some(outcome),
            _ => None,
        }
    }

    /// Remet à plus tard, si le compte tourne encore. Renvoie `true` si
    /// c'est fait.
    pub fn postpone(&mut self) -> bool {
        let Step::Count(left) = self.step else {
            return false;
        };
        self.stopped_at = left;
        self.step = Step::Cancel(0.0);
        if let Some(sweep) = &mut self.sweep {
            sweep.ufo.leave(self.time);
        }
        true
    }

    /// Le nombre affiché.
    fn count(&self) -> u32 {
        match self.step {
            Step::Count(left) => (left.ceil() as u32).min(COUNT_FROM),
            Step::Cancel(_) => (self.stopped_at.ceil() as u32).min(COUNT_FROM),
            _ => 0,
        }
    }

    /// Depuis combien de temps ce nombre est affiché (secondes).
    fn since_change(&self) -> f32 {
        match self.step {
            Step::Count(left) => left.ceil() - left,
            Step::Bleat(t) => t,
            _ => 1.0,
        }
    }

    /// Ce qui est passé du compte (secondes).
    fn spent(&self) -> f32 {
        match self.step {
            Step::Count(left) => COUNT_FROM as f32 - left,
            _ => COUNT_FROM as f32,
        }
    }

    /// Avance de `dt` secondes.
    pub fn step(&mut self, dt: f32, flock: &mut Flock, world: &World, input: &Input) {
        let dt = if dt.is_finite() {
            dt.clamp(0.0, MAX_STEP)
        } else {
            0.0
        };
        self.time += dt;
        // Celui qui annonce est parti entre-temps (on lui a dit au revoir) :
        // un autre prend sa place.
        if self
            .herald
            .is_some_and(|id| !flock.pets().iter().any(|pet| pet.id() == id && fit(pet)))
        {
            let focus = self.sweep.as_ref().map_or(Vec2::ZERO, |sweep| {
                Vec2::new(sweep.ufo.pos.x, sweep.sky.floor())
            });
            self.herald = herald(flock, world, focus);
        }
        // Pendant le compte, la soucoupe va et vient au-dessus du troupeau.
        let spent = self.spent();
        if matches!(self.step, Step::Count(_) | Step::Bleat(_))
            && let Some(sweep) = &mut self.sweep
        {
            sweep.patrol(flock, spent, dt);
        }
        match self.step {
            Step::Count(left) => {
                if flock.pets().is_empty() {
                    // Plus personne à emmener : la place est libre.
                    self.step = Step::Leave(0.0);
                    return;
                }
                // L'annonceur reste où il est : il ne se remet pas en route.
                if self.scene == Scene::Herald
                    && let Some(pet) = self.herald.and_then(|id| flock.get_mut(id))
                    && matches!(pet.support(), Support::Ground { .. })
                    && pet.velocity().x.abs() > 1.0
                {
                    pet.force("stand");
                }
                let left = left - dt;
                self.step = if left > 0.0 {
                    Step::Count(left)
                } else {
                    // Zéro : il bêle (s'il sait).
                    if let Some(pet) = self.herald.and_then(|id| flock.get_mut(id))
                        && matches!(pet.support(), Support::Ground { .. })
                    {
                        pet.force("bleat");
                    }
                    Step::Bleat(0.0)
                };
            }
            Step::Bleat(t) => {
                let t = t + dt;
                if t < BLEAT_TIME {
                    self.step = Step::Bleat(t);
                    return;
                }
                self.step = Step::Leave(0.0);
                if self.sweep.is_none() {
                    flock.bound_away(world, input);
                }
            }
            Step::Leave(t) => {
                self.step = Step::Leave(t + dt);
                let gone = match &mut self.sweep {
                    None => flock.pets().is_empty(),
                    Some(sweep) => sweep.take(flock, world, self.time, dt),
                };
                if gone {
                    self.step = Step::Done(Outcome::Gone);
                }
            }
            Step::Cancel(t) => {
                let t = t + dt;
                let over = match &mut self.sweep {
                    Some(sweep) => {
                        let sky = sweep.sky;
                        sweep.ufo.fly_away(&sky, self.time, dt)
                    }
                    None => t >= BUBBLE_OUT,
                };
                self.step = if over {
                    Step::Done(Outcome::Postponed)
                } else {
                    Step::Cancel(t)
                };
            }
            Step::Done(_) => {}
        }
    }

    /// L'opacité de cet animal : il s'efface en entrant dans la soucoupe.
    #[must_use]
    pub fn opacity(&self, id: u32) -> f32 {
        self.sweep
            .as_ref()
            .map_or(1.0, |sweep| sweep.opacity(id, self.time))
    }

    /// Ce qu'il y a à dessiner en plus du troupeau.
    #[must_use]
    pub fn view(&self, flock: &Flock, world: &World) -> ShowView {
        let mut view = ShowView::default();
        let pop = 1.0 + 0.2 * (-self.since_change() * 9.0).exp();
        let text = self.count().to_string();
        match &self.sweep {
            None => {
                let alpha = match self.step {
                    Step::Count(left) => smooth(0.0, BUBBLE_IN, COUNT_FROM as f32 - left),
                    Step::Bleat(_) => 1.0,
                    Step::Cancel(t) => 1.0 - smooth(0.0, BUBBLE_OUT, t),
                    Step::Leave(_) | Step::Done(_) => 0.0,
                };
                let herald = self
                    .herald
                    .and_then(|id| flock.pets().iter().find(|pet| pet.id() == id));
                if let (Some(pet), true) = (herald, alpha > 0.005) {
                    view.bubble = Some(BubbleView {
                        tip: above(pet, world),
                        text,
                        scale: pop * (0.6 + 0.4 * alpha),
                        alpha,
                        sparkle: None,
                    });
                }
            }
            Some(sweep) => {
                // Elle arrive pendant que le compte commence ; son faisceau
                // s'allume une fois qu'elle est là, et s'éteint quand elle
                // repart.
                let lit = smooth(
                    SAUCER_ARRIVES * 0.75,
                    SAUCER_ARRIVES * 0.75 + BEAM_ON,
                    self.spent(),
                );
                let done = matches!(self.step, Step::Done(_));
                let alpha = if done {
                    0.0
                } else {
                    lit * sweep.ufo.fade(self.time)
                };
                if !done && !sweep.ufo.gone(self.time) {
                    view.saucer = Some(sweep.ufo.view(self.time));
                }
                view.beam = beam(sweep.ufo.pos, sweep.sky.floor(), alpha, self.time);
                let shown = matches!(self.step, Step::Count(_) | Step::Bleat(_) | Step::Cancel(_));
                if shown && alpha > 0.3 {
                    view.count = Some(CountView {
                        center: mouth(sweep.ufo.pos) + Vec2::new(0.0, 74.0),
                        text,
                        scale: pop,
                        alpha: smooth(0.3, 0.9, alpha),
                    });
                }
            }
        }
        view
    }
}

/// Au retour, la soucoupe repasse au plus tard à ce moment-là, même si tous
/// ceux qu'elle ramène ne sont pas encore arrivés ; elle repart au plus tard
/// à celui-ci (secondes).
const PASS_LATEST: f32 = 8.0;
const WAIT_MAX: f32 = 20.0;
/// Sans personne pour la porter, la bulle est abandonnée après ce temps.
const LABEL_MAX: f32 = 40.0;
/// Celui qu'elle ramène apparaît dans sa lumière en ce temps-là (secondes).
const APPEARS: f32 = 0.35;
/// Au retour, elle arrive en ce temps-là, et son faisceau est allumé à
/// celui-ci (secondes).
const RETURNS: f32 = 1.1;
const LIT: f32 = 1.7;
/// Au retour, elle se tient plus haut de ceci qu'au-dessus d'un troupeau
/// au sol (points).
const RETURN_LIFT: f32 = 140.0;

/// Un animal que la soucoupe ramène : sa place (abscisse), son rang dans sa
/// lumière, où elle le tient (le milieu de son corps), quand il apparaît ;
/// puis quand elle a commencé à le descendre, et d'où.
#[derive(Clone, Copy, Debug)]
struct Arrival {
    id: u32,
    x: f32,
    rank: usize,
    at: Vec2,
    shows_at: f32,
    descent: Option<(f32, Vec2)>,
    released: bool,
}

/// La soucoupe du retour.
#[derive(Clone, Debug)]
struct Delivery {
    sky: Sky,
    ufo: Ufo,
    /// D'où elle arrive : au-dessus de l'écran, à gauche.
    from: Vec2,
    altitude: f32,
    course: (f32, f32),
    /// Elle les tient plus haut que ceci : le plus haut des niveaux où elle
    /// les posera.
    low: f32,
    /// Combien d'animaux elle ramène : ceux dont la place est sous elle.
    expected: usize,
    arrivals: Vec<Arrival>,
    /// Elle repasse pour les reposer ; elle a fini de repasser ; elle est
    /// partie.
    passing: bool,
    passed: bool,
    gone: bool,
}

impl Delivery {
    /// Au-dessus de ces places, sur l'écran où le plus d'entre elles sont.
    fn over(places: &[f32], world: &World, focus: Vec2) -> Option<Self> {
        let screens = world.screens();
        let count = |screen: &Screen| {
            let sky = Sky::of(screen);
            places.iter().filter(|x| sky.holds(**x)).count()
        };
        let screen = screens
            .iter()
            .max_by_key(|screen| count(screen))
            .filter(|screen| count(screen) > 0)
            .or_else(|| world.screen_at(focus))
            .or(screens.first())?;
        let sky = Sky::of(screen);
        let here: Vec<f32> = places.iter().copied().filter(|x| sky.holds(*x)).collect();
        let middle = sky.room.center().x;
        let left = here.iter().copied().reduce(f32::min).unwrap_or(middle);
        let right = here.iter().copied().reduce(f32::max).unwrap_or(middle);
        let course = sky.course(left, right);
        // Elle les posera chacun sur ce qui est sous sa place (une fenêtre,
        // ou le sol) : elle les tient tous plus haut que le plus haut de ces
        // niveaux, et se tient bien au-dessus.
        let low = here
            .iter()
            .map(|x| {
                world
                    .surface_below(Vec2::new(*x, sky.top), 0.0)
                    .map_or(sky.floor(), |surface| surface.y.min(sky.floor()))
            })
            .fold(sky.floor(), f32::min);
        let altitude = sky.altitude(low - 60.0, RETURN_LIFT);
        let from = sky.away(course.0, -1.0);
        Some(Self {
            sky,
            ufo: Ufo::at(from),
            from,
            altitude,
            course,
            low,
            expected: here.len(),
            arrivals: Vec::new(),
            passing: false,
            passed: false,
            gone: false,
        })
    }

    /// Ce sous quoi elle tient ceux qui arrivent : elle ; tant qu'elle
    /// arrive, là où elle va.
    fn anchor(&self, time: f32) -> Vec2 {
        if time < RETURNS {
            Vec2::new(self.course.0, self.altitude)
        } else {
            self.ufo.pos
        }
    }

    /// Avance ; `true` : elle est partie.
    fn step(&mut self, flock: &mut Flock, world: &World, time: f32, dt: f32) -> bool {
        if self.gone {
            return true;
        }
        let start = Vec2::new(self.course.0, self.altitude);
        if self.ufo.leaving.is_none() {
            if time < RETURNS {
                self.ufo
                    .put(self.from.lerp(start, smooth(0.0, RETURNS, time)), dt);
            } else if !self.passing {
                self.ufo.glide(start, dt);
            }
        }
        // Elle repasse quand tous ceux qu'elle ramène sont dans sa lumière
        // (ou qu'elle a assez attendu).
        if !self.passing && time >= RETURNS {
            let seen = self
                .arrivals
                .iter()
                .map(|arrival| arrival.shows_at + APPEARS)
                .fold(LIT, f32::max);
            let all = self.arrivals.len() >= self.expected;
            self.passing = (all && time >= seen + 0.4) || time >= PASS_LATEST;
        }
        if self.passing && !self.passed && self.ufo.leaving.is_none() {
            self.passed = self
                .ufo
                .travel(Vec2::new(self.course.1, self.altitude), SET_SPEED, dt);
        }
        // Chacun : elle le tient à sa place dans sa lumière ; arrivée
        // au-dessus de sa place (ou une fois passée), elle le descend
        // jusqu'au sol, puis le lâche.
        let (floor, low) = (self.sky.floor(), self.low);
        let anchor = self.anchor(time);
        let (over, passing, passed) = (self.ufo.pos.x, self.passing, self.passed);
        for arrival in self.arrivals.iter_mut().filter(|arrival| !arrival.released) {
            let Some(pet) = flock.get_mut(arrival.id) else {
                arrival.released = true;
                continue;
            };
            if !pet.is_held() {
                arrival.released = true;
                continue;
            }
            if arrival.descent.is_none()
                && time >= arrival.shows_at
                && (passed || (passing && over >= arrival.x))
            {
                arrival.descent = Some((time, arrival.at));
            }
            let target = match arrival.descent {
                None => slot(anchor, floor, low, arrival.rank, time),
                Some((since, from)) => {
                    let u = (time - since) / DESCEND;
                    if u >= 1.0 {
                        pet.release(world);
                        arrival.released = true;
                        continue;
                    }
                    // Sur ce qui est sous sa place : une fenêtre, ou le sol.
                    let height = pet.def().physics.height;
                    let level = world
                        .surface_below(Vec2::new(arrival.x, from.y + height * 0.5), 0.0)
                        .map_or(floor, |surface| surface.y.min(floor));
                    let ground = Vec2::new(arrival.x, level - height * 0.5 - 6.0);
                    from.lerp(ground, smooth(0.0, 1.0, u))
                }
            };
            arrival.at = target;
            pet.drag_to(target);
        }
        // Tous reposés : elle s'en va (au plus tard au bout d'un moment).
        if self.ufo.leaving.is_none()
            && ((passed && self.arrivals.iter().all(|arrival| arrival.released))
                || time >= WAIT_MAX)
        {
            for arrival in self.arrivals.iter_mut().filter(|arrival| !arrival.released) {
                if let Some(pet) = flock.get_mut(arrival.id) {
                    pet.release(world);
                }
                arrival.released = true;
            }
            self.ufo.leave(time);
        }
        let sky = self.sky;
        self.gone = self.ufo.fly_away(&sky, time, dt);
        self.gone
    }
}

/// Le retour du troupeau : la soucoupe le repose (si c'est elle qui l'avait
/// pris), et l'un d'eux montre le numéro de la nouvelle version.
#[derive(Clone, Debug)]
pub struct Welcome {
    label: String,
    time: f32,
    /// Là où l'utilisateur regarde.
    focus: Vec2,
    /// La soucoupe, s'il y en a une.
    delivery: Option<Delivery>,
    herald: Option<u32>,
    /// Depuis quand la bulle est montrée.
    shown: Option<f32>,
}

impl Welcome {
    /// Le retour commence. `label` : ce que la bulle montrera (le numéro de
    /// la version ; rien : pas de bulle). `places` : les abscisses où les
    /// animaux vont revenir, s'ils reviennent.
    #[must_use]
    pub fn new(scene: Scene, label: &str, places: &[f32], world: &World, focus: Vec2) -> Self {
        let delivery = (scene == Scene::Saucer && !places.is_empty())
            .then(|| Delivery::over(places, world, focus))
            .flatten();
        Self {
            label: label.chars().take(12).collect(),
            time: 0.0,
            focus,
            delivery,
            herald: None,
            shown: None,
        }
    }

    /// D'où arrive celui qui revient à l'abscisse `x` (`height` : sa
    /// hauteur) : de la lumière de la soucoupe, si elle est encore là et que
    /// sa place est sous elle.
    #[must_use]
    pub fn drop_point(&self, x: f32, height: f32) -> Option<Vec2> {
        let delivery = self
            .delivery
            .as_ref()
            .filter(|delivery| delivery.ufo.leaving.is_none() && delivery.sky.holds(x))?;
        let middle = slot(
            delivery.anchor(self.time),
            delivery.sky.floor(),
            delivery.low,
            delivery.arrivals.len(),
            self.time,
        );
        Some(middle + Vec2::new(0.0, height * 0.5))
    }

    /// Cet animal vient d'arriver là où [`Self::drop_point`] l'a dit, pour
    /// revenir à l'abscisse `x` : elle le tient dans sa lumière (invisible
    /// tant que son faisceau n'est pas allumé), puis le repose à sa place.
    pub fn arrived(&mut self, id: u32, x: f32, flock: &mut Flock, world: &World) {
        let time = self.time;
        let Some(delivery) = self
            .delivery
            .as_mut()
            .filter(|delivery| delivery.ufo.leaving.is_none() && delivery.sky.holds(x))
        else {
            return;
        };
        let Some(pet) = flock.get_mut(id) else {
            return;
        };
        let at = pet.position() - Vec2::new(0.0, pet.def().physics.height * 0.5);
        pet.grab(at, world);
        // Ils apparaissent l'un après l'autre, de près.
        let rank = delivery.arrivals.len();
        delivery.arrivals.push(Arrival {
            id,
            x,
            rank,
            at,
            shows_at: time.max(LIT) + (rank as f32 * 0.06).min(0.6),
            descent: None,
            released: false,
        });
    }

    /// L'opacité de cet animal : celui qu'elle ramène apparaît en un
    /// instant, une fois son faisceau allumé.
    #[must_use]
    pub fn opacity(&self, id: u32) -> f32 {
        self.delivery
            .as_ref()
            .and_then(|delivery| delivery.arrivals.iter().find(|arrival| arrival.id == id))
            .map_or(1.0, |arrival| {
                smooth(0.0, APPEARS, self.time - arrival.shows_at)
            })
    }

    /// Fini : plus rien à montrer.
    #[must_use]
    pub fn is_over(&self) -> bool {
        let saucer_gone = self.delivery.as_ref().is_none_or(|delivery| delivery.gone);
        let label_done = self.label.is_empty()
            || self.shown.is_some_and(|since| since >= LABEL_TIME)
            || (self.shown.is_none() && self.time >= LABEL_MAX);
        saucer_gone && label_done
    }

    /// Avance de `dt` secondes.
    pub fn step(&mut self, dt: f32, flock: &mut Flock, world: &World) {
        let dt = if dt.is_finite() {
            dt.clamp(0.0, MAX_STEP)
        } else {
            0.0
        };
        self.time += dt;
        let time = self.time;
        if let Some(delivery) = &mut self.delivery {
            delivery.step(flock, world, time, dt);
        }
        let flock = &*flock;
        let grounded = |pet: &&Pet| fit(pet) && matches!(pet.support(), Support::Ground { .. });
        // La bulle : au-dessus du premier qui s'est posé, tant qu'il est là.
        if let Some(since) = &mut self.shown {
            *since += dt;
        }
        let here = |id: u32| flock.pets().iter().any(|pet| pet.id() == id && fit(pet));
        if self.herald.is_none_or(|id| !here(id)) {
            // Posé, avec la place d'une bulle au-dessus de lui si possible.
            let focus = self.delivery.as_ref().map_or(self.focus, |delivery| {
                Vec2::new(delivery.ufo.pos.x, delivery.sky.floor())
            });
            let nearest = |roomy_only: bool| {
                flock
                    .pets()
                    .iter()
                    .filter(grounded)
                    .filter(|pet| !roomy_only || roomy(pet, world))
                    .min_by(|a, b| {
                        let near = |pet: &Pet| (pet.position() - focus).length();
                        near(a).total_cmp(&near(b))
                    })
                    .map(Pet::id)
            };
            self.herald = nearest(true).or_else(|| nearest(false));
        }
        if self.shown.is_none() && self.herald.is_some() && !self.label.is_empty() {
            self.shown = Some(0.0);
        }
    }

    /// Ce qu'il y a à dessiner en plus du troupeau.
    #[must_use]
    pub fn view(&self, flock: &Flock, world: &World) -> ShowView {
        let mut view = ShowView::default();
        if let Some(delivery) = &self.delivery {
            let alpha = smooth(RETURNS * 0.8, LIT, self.time) * delivery.ufo.fade(self.time);
            if !delivery.gone {
                view.saucer = Some(delivery.ufo.view(self.time));
            }
            view.beam = beam(delivery.ufo.pos, delivery.sky.floor(), alpha, self.time);
        }
        let herald = self
            .herald
            .and_then(|id| flock.pets().iter().find(|pet| pet.id() == id));
        if let (Some(pet), Some(since)) = (herald, self.shown)
            && since < LABEL_TIME
        {
            let alpha =
                smooth(0.0, BUBBLE_IN, since) * (1.0 - smooth(LABEL_TIME - 0.5, LABEL_TIME, since));
            view.bubble = Some(BubbleView {
                tip: above(pet, world),
                text: self.label.clone(),
                scale: (0.6 + 0.4 * alpha) * (1.0 + 0.2 * (-since * 7.0).exp()),
                alpha,
                sparkle: Some(since * 0.5),
            });
        }
        view
    }
}

#[cfg(test)]
mod tests {
    use std::sync::Arc;

    use super::*;
    use crate::PetDef;

    fn bundled(name: &str) -> Arc<PetDef> {
        let path = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
            .join(format!("../../pets/{name}/pet.toml"));
        Arc::new(crate::format::load_pet(&path).expect("un animal du dépôt"))
    }

    fn desk() -> World {
        let frame = Rect::new(0.0, 0.0, 1440.0, 900.0);
        World::new(vec![Screen {
            frame,
            usable: Rect::new(0.0, 25.0, 1440.0, 835.0),
            scale: 2.0,
        }])
    }

    fn herd(world: &World, places: &[f32]) -> Flock {
        let mut flock = Flock::new();
        flock.set_realistic(true);
        for (i, x) in places.iter().enumerate() {
            let name = ["bleu-du-maine", "ouessant", "texel"][i % 3];
            flock.spawn(bundled(name), 3 + i as u64 * 7919, world, Some(*x));
        }
        for _ in 0..240 {
            flock.advance(1.0 / 60.0, world, &Input::default());
        }
        flock
    }

    const DT: f32 = 1.0 / 60.0;

    /// Un pas de tout : le troupeau, puis l'annonce.
    fn tick(show: &mut Farewell, flock: &mut Flock, world: &World) {
        flock.advance(DT, world, &Input::default());
        show.step(DT, flock, world, &Input::default());
    }

    #[test]
    fn the_bubble_counts_from_ten_to_zero_then_the_flock_bounds_away() {
        let world = desk();
        let mut flock = herd(&world, &[300.0, 700.0, 1100.0]);
        let focus = Vec2::new(720.0, 860.0);
        let mut show =
            Farewell::new(Scene::Herald, &mut flock, &world, focus).expect("un troupeau");
        let herald = show.herald.expect("un annonceur");
        let mut seen = Vec::new();
        let mut time = 0.0;
        while show.is_counting() {
            tick(&mut show, &mut flock, &world);
            time += DT;
            let view = show.view(&flock, &world);
            let bubble = view.bubble.expect("la bulle, tout le compte");
            assert!(view.saucer.is_none() && view.beam.is_none() && view.count.is_none());
            if seen.last() != Some(&bubble.text) {
                seen.push(bubble.text.clone());
            }
            // Au-dessus de la tête de celui qui annonce.
            let pet = flock.pets().iter().find(|p| p.id() == herald).expect("là");
            assert!(bubble.tip.y < pet.position().y - 20.0);
            assert!((bubble.tip.x - pet.position().x).abs() < 60.0);
            assert_eq!(flock.pets().len(), 3, "personne ne part avant zéro");
        }
        assert!((time - 10.0).abs() < 0.1, "dix secondes : {time}");
        assert_eq!(
            seen,
            ["10", "9", "8", "7", "6", "5", "4", "3", "2", "1", "0"]
        );
        // Zéro : il bêle, puis tout le monde s'en va.
        assert_eq!(show.view(&flock, &world).bubble.expect("zéro").text, "0");
        let pet = flock.pets().iter().find(|p| p.id() == herald).expect("là");
        assert_eq!(pet.behavior_name(), "bleat");
        assert!(!show.postpone(), "trop tard pour remettre");
        let mut left = 0.0;
        while show.outcome().is_none() {
            tick(&mut show, &mut flock, &world);
            left += DT;
            assert!(left < 9.0, "encore {} animaux", flock.pets().len());
        }
        assert_eq!(show.outcome(), Some(Outcome::Gone));
        assert!(flock.pets().is_empty());
        assert_eq!(show.view(&flock, &world), ShowView::default());
    }

    #[test]
    fn postponed_the_bubble_goes_and_the_flock_stays() {
        let world = desk();
        let mut flock = herd(&world, &[300.0, 700.0]);
        let mut show =
            Farewell::new(Scene::Herald, &mut flock, &world, Vec2::new(720.0, 860.0)).expect("là");
        for _ in 0..200 {
            tick(&mut show, &mut flock, &world);
        }
        assert!(show.postpone());
        assert!(!show.is_counting());
        assert!(!show.postpone(), "une seule fois");
        // La bulle garde son nombre en s'effaçant.
        assert_eq!(
            show.view(&flock, &world).bubble.expect("encore là").text,
            "7"
        );
        for _ in 0..40 {
            tick(&mut show, &mut flock, &world);
        }
        assert_eq!(show.outcome(), Some(Outcome::Postponed));
        assert_eq!(flock.pets().len(), 2);
        assert!(!flock.is_leaving_the_screen());
        assert_eq!(show.view(&flock, &world), ShowView::default());
    }

    #[test]
    fn postponed_the_saucer_leaves_alone() {
        let world = desk();
        let mut flock = herd(&world, &[400.0, 900.0]);
        let mut show =
            Farewell::new(Scene::Saucer, &mut flock, &world, Vec2::new(720.0, 860.0)).expect("là");
        for _ in 0..300 {
            tick(&mut show, &mut flock, &world);
        }
        assert!(show.postpone());
        let mut left = 0.0;
        while show.outcome().is_none() {
            tick(&mut show, &mut flock, &world);
            left += DT;
            assert!(left < 3.0);
        }
        assert_eq!(show.outcome(), Some(Outcome::Postponed));
        assert_eq!(flock.pets().len(), 2);
        assert!(flock.pets().iter().all(|pet| !pet.is_held()));
    }

    #[test]
    fn an_empty_flock_has_nothing_to_announce() {
        let world = desk();
        let mut flock = Flock::new();
        assert!(Farewell::new(Scene::Herald, &mut flock, &world, Vec2::ZERO).is_none());
        assert!(Farewell::new(Scene::Saucer, &mut flock, &world, Vec2::ZERO).is_none());
    }

    #[test]
    fn back_home_one_of_them_shows_the_version_for_a_while() {
        let world = desk();
        let places = [300.0, 700.0, 1100.0];
        let mut show = Welcome::new(
            Scene::Herald,
            "1.0.5",
            &places,
            &world,
            Vec2::new(720.0, 860.0),
        );
        assert!(
            show.drop_point(700.0, 60.0).is_none(),
            "pas de soucoupe : du haut de l'écran"
        );
        let mut flock = Flock::new();
        flock.set_realistic(true);
        for (i, x) in places.iter().enumerate() {
            flock.spawn(bundled("bleu-du-maine"), 5 + i as u64, &world, Some(*x));
        }
        let mut shown = 0.0;
        let mut time = 0.0;
        while !show.is_over() {
            flock.advance(DT, &world, &Input::default());
            show.step(DT, &mut flock, &world);
            time += DT;
            let view = show.view(&flock, &world);
            assert!(view.saucer.is_none() && view.beam.is_none());
            if let Some(bubble) = view.bubble {
                assert_eq!(bubble.text, "1.0.5");
                assert!(bubble.sparkle.is_some());
                shown += DT;
            }
            assert!(time < 20.0);
        }
        assert!((shown - LABEL_TIME).abs() < 0.3, "{shown}");
        assert_eq!(flock.pets().len(), 3);
    }

    /// Un bureau avec une fenêtre dont le haut est à `top`.
    fn desk_with_window(top: f32) -> World {
        let mut world = desk();
        world.set_windows(
            &[crate::world::WindowRect {
                id: 1,
                frame: Rect::new(420.0, top, 600.0, 860.0 - top),
            }],
            crate::world::LedgeOptions::default(),
        );
        world
    }

    #[test]
    fn with_a_sheep_perched_at_the_very_top_the_herald_announces_instead() {
        // Une fenêtre tout en haut de l'écran : la soucoupe ne pourrait pas
        // se tenir au-dessus de ceux qui y sont.
        let world = desk_with_window(70.0);
        let mut flock = herd(&world, &[500.0, 800.0, 200.0]);
        let perched: Vec<u32> = flock
            .pets()
            .iter()
            .filter(|pet| pet.position().y < 100.0)
            .map(Pet::id)
            .collect();
        assert!(!perched.is_empty(), "personne sur la fenêtre");
        let mut show =
            Farewell::new(Scene::Saucer, &mut flock, &world, Vec2::new(720.0, 860.0)).expect("là");
        assert_eq!(show.scene(), Scene::Herald);
        // Et celui qui annonce a la place d'une bulle au-dessus de lui :
        // pas l'un de ceux qui sont perchés.
        let herald = show.herald.expect("un annonceur");
        assert!(!perched.contains(&herald));
        for _ in 0..60 {
            tick(&mut show, &mut flock, &world);
        }
        let view = show.view(&flock, &world);
        assert!(view.saucer.is_none() && view.beam.is_none());
        let bubble = view.bubble.expect("la bulle");
        assert!(bubble.tip.y >= BUBBLE_ROOM.0, "{:?}", bubble.tip);
    }

    #[test]
    fn a_bubble_never_leaves_the_screen() {
        // Tous perchés tout en haut : l'un d'eux annonce quand même, et sa
        // bulle reste sur l'écran (posée sur lui).
        let world = desk_with_window(95.0);
        let mut flock = herd(&world, &[500.0, 800.0]);
        assert!(
            flock.pets().iter().all(|pet| pet.position().y < 120.0),
            "tous sur la fenêtre"
        );
        let mut show =
            Farewell::new(Scene::Herald, &mut flock, &world, Vec2::new(720.0, 860.0)).expect("là");
        for _ in 0..60 {
            tick(&mut show, &mut flock, &world);
        }
        let bubble = show.view(&flock, &world).bubble.expect("la bulle");
        assert!(bubble.tip.y >= BUBBLE_ROOM.0, "{:?}", bubble.tip);
        // Au bord de l'écran, elle ne déborde pas de côté non plus.
        let world = desk();
        let mut flock = herd(&world, &[12.0]);
        let mut show =
            Farewell::new(Scene::Herald, &mut flock, &world, Vec2::new(720.0, 860.0)).expect("là");
        for _ in 0..30 {
            tick(&mut show, &mut flock, &world);
        }
        let bubble = show.view(&flock, &world).bubble.expect("la bulle");
        assert!(
            bubble.tip.x >= BUBBLE_ROOM.1 && bubble.tip.x <= 1440.0 - BUBBLE_ROOM.1,
            "{:?}",
            bubble.tip
        );
    }

    #[test]
    fn sheep_still_falling_from_the_top_do_not_keep_the_saucer_away() {
        let world = desk();
        let mut flock = Flock::new();
        flock.set_realistic(true);
        // L'un est posé ; l'autre vient d'arriver, tout en haut de l'écran.
        flock.spawn(bundled("texel"), 3, &world, Some(500.0));
        for _ in 0..240 {
            flock.advance(DT, &world, &Input::default());
        }
        flock.spawn(bundled("ouessant"), 4, &world, Some(900.0));
        assert!(flock.pets()[1].position().y < 120.0);
        let mut show =
            Farewell::new(Scene::Saucer, &mut flock, &world, Vec2::new(720.0, 860.0)).expect("là");
        assert_eq!(show.scene(), Scene::Saucer);
        // Une fois tout le monde posé, elle est au-dessus de tous.
        for _ in 0..300 {
            tick(&mut show, &mut flock, &world);
        }
        let view = show.view(&flock, &world);
        let saucer = view.saucer.expect("elle");
        for pet in flock.pets() {
            assert!(saucer.pos.y < pet.position().y - pet.def().physics.height - 150.0);
        }
        assert!(view.beam.is_some());
    }

    /// Le départ de la soucoupe, après le compte, jusqu'au bout : qui elle a
    /// pris (son numéro, où il était, où elle était), et si elle les a tous
    /// tenus ensemble dans sa lumière à un moment.
    fn sweep_away(
        show: &mut Farewell,
        flock: &mut Flock,
        world: &World,
    ) -> (Vec<(u32, f32, f32)>, bool) {
        let mut caught = Vec::new();
        let mut together = false;
        let mut risen = false;
        let mut time = 0.0;
        while show.outcome().is_none() {
            let free: Vec<(u32, f32)> = flock
                .pets()
                .iter()
                .filter(|pet| !pet.is_held())
                .map(|pet| (pet.id(), pet.position().x))
                .collect();
            tick(show, flock, world);
            time += DT;
            if let Some(saucer) = show.view(flock, world).saucer {
                for pet in flock.pets().iter().filter(|pet| pet.is_held()) {
                    if let Some((id, x)) = free.iter().find(|(id, _)| *id == pet.id()) {
                        caught.push((*id, *x, saucer.pos.x));
                    }
                }
            }
            // Quand elle commence à les aspirer, chacun est monté dans sa
            // lumière, plus haut que là où elle l'a pris.
            if let Some(sweep) = &show.sweep
                && sweep.sucking.is_some()
                && !risen
            {
                risen = true;
                for caught in &sweep.caught {
                    let pet = flock
                        .pets()
                        .iter()
                        .find(|p| p.id() == caught.id)
                        .expect("là");
                    let middle = pet.position().y - pet.def().physics.height * 0.5;
                    assert!(
                        middle < caught.from.y - 10.0,
                        "{middle} : pas monté de {:?}",
                        caught.from
                    );
                }
            }
            if !flock.pets().is_empty() && flock.pets().iter().all(Pet::is_held) {
                together = true;
            }
            assert!(time < 20.0, "encore {} animaux", flock.pets().len());
        }
        assert!(risen, "aspirés une fois montés");
        (caught, together)
    }

    #[test]
    fn the_saucer_glides_over_the_flock_then_sweeps_them_all_into_her_light() {
        let world = desk();
        let mut flock = herd(&world, &[250.0, 600.0, 760.0, 1150.0]);
        let mut show =
            Farewell::new(Scene::Saucer, &mut flock, &world, Vec2::new(720.0, 860.0)).expect("là");
        assert_eq!(show.scene(), Scene::Saucer);
        // Pendant le compte : elle arrive par la gauche, puis va et vient
        // au-dessus de tout le troupeau, haut, son faisceau d'une largeur
        // ordinaire, le compte dans sa lumière ; personne n'est pris.
        let (mut least, mut most) = (f32::INFINITY, f32::NEG_INFINITY);
        let mut first = None;
        while show.is_counting() {
            tick(&mut show, &mut flock, &world);
            let view = show.view(&flock, &world);
            let saucer = view.saucer.expect("elle");
            first.get_or_insert(saucer.pos.x);
            assert!(flock.pets().iter().all(|pet| !pet.is_held()));
            if show.time < SAUCER_ARRIVES + 0.2 {
                continue;
            }
            least = least.min(saucer.pos.x);
            most = most.max(saucer.pos.x);
            for pet in flock.pets() {
                let top = pet.position().y - pet.def().physics.height;
                assert!(saucer.pos.y < top - 100.0, "{saucer:?}");
            }
            let beam = view.beam.expect("son faisceau");
            assert!(beam.half_width <= BEAM_FLOOR.1 + 0.5, "{}", beam.half_width);
            assert!(view.count.is_some());
        }
        assert!(
            first.is_some_and(|x| x < least),
            "elle arrive par la gauche"
        );
        let xs: Vec<f32> = flock.pets().iter().map(|pet| pet.position().x).collect();
        let left = xs.iter().copied().fold(f32::INFINITY, f32::min);
        let right = xs.iter().copied().fold(f32::NEG_INFINITY, f32::max);
        assert!(
            least < left && most > right,
            "{least}..{most} pour {left}..{right}"
        );
        // Zéro : elle repasse de gauche à droite, et chacun monte dans sa
        // lumière quand elle arrive au-dessus de lui ; puis elle les aspire
        // tous ensemble.
        let (caught, together) = sweep_away(&mut show, &mut flock, &world);
        assert_eq!(caught.len(), 4, "{caught:?}");
        for &(_, x, at) in &caught {
            assert!((x - at).abs() < BEAM_FLOOR.1, "pris en {x}, elle en {at}");
        }
        assert!(
            caught.windows(2).all(|w| w[0].2 <= w[1].2 + 1.0),
            "de gauche à droite : {caught:?}"
        );
        assert!(together, "tous ensemble dans sa lumière");
        assert_eq!(show.outcome(), Some(Outcome::Gone));
        assert!(flock.pets().is_empty());
        assert_eq!(show.view(&flock, &world), ShowView::default());
    }

    #[test]
    fn those_on_a_window_are_swept_up_too() {
        // Une fenêtre à mi-hauteur : trois dessus, deux au sol.
        let world = desk_with_window(420.0);
        let mut flock = herd(&world, &[480.0, 700.0, 960.0, 150.0, 1300.0]);
        let high: Vec<u32> = flock
            .pets()
            .iter()
            .filter(|pet| pet.position().y < 500.0)
            .map(Pet::id)
            .collect();
        assert!(high.len() >= 2, "{} sur la fenêtre", high.len());
        let mut show =
            Farewell::new(Scene::Saucer, &mut flock, &world, Vec2::new(720.0, 860.0)).expect("là");
        assert_eq!(show.scene(), Scene::Saucer);
        while show.is_counting() {
            tick(&mut show, &mut flock, &world);
            if show.time > SAUCER_ARRIVES + 0.2 {
                let saucer = show.view(&flock, &world).saucer.expect("elle");
                for pet in flock.pets() {
                    let top = pet.position().y - pet.def().physics.height;
                    assert!(saucer.pos.y < top - 60.0, "{saucer:?}");
                }
            }
        }
        let (caught, together) = sweep_away(&mut show, &mut flock, &world);
        assert_eq!(caught.len(), 5, "{caught:?}");
        assert!(high.iter().all(|id| caught.iter().any(|c| c.0 == *id)));
        assert!(together);
        assert!(flock.pets().is_empty());
    }

    #[test]
    fn back_home_the_saucer_brings_them_in_her_light_and_sets_each_down_at_its_place() {
        // Une fenêtre sous la place du milieu.
        let world = desk_with_window(420.0);
        let places = [300.0, 700.0, 1100.0];
        let mut show = Welcome::new(
            Scene::Saucer,
            "1.0.5",
            &places,
            &world,
            Vec2::new(720.0, 860.0),
        );
        let mut flock = Flock::new();
        flock.set_realistic(true);
        // Ils arrivent tout de suite, avant même qu'elle soit là : elle les
        // tient, invisibles, jusqu'à ce que son faisceau soit allumé.
        let mut ids = Vec::new();
        for (i, x) in places.iter().enumerate() {
            let def = bundled("ouessant");
            let from = show
                .drop_point(*x, def.physics.height)
                .expect("de la soucoupe");
            let id = flock.spawn_at(def, 9 + i as u64, &world, from, None);
            show.arrived(id, *x, &mut flock, &world);
            assert!(show.opacity(id) < 0.01, "pas encore visible");
            ids.push((id, *x));
        }
        // Une place hors de tout écran : pas d'elle.
        assert!(show.drop_point(-5000.0, 60.0).is_none());
        let mut time = 0.0;
        let mut set_down: Vec<(u32, f32)> = Vec::new();
        let mut landed: Vec<(u32, f32)> = Vec::new();
        let mut all_in_light = false;
        let mut gone_at = None;
        while !show.is_over() {
            let held: Vec<u32> = flock
                .pets()
                .iter()
                .filter(|pet| pet.is_held())
                .map(Pet::id)
                .collect();
            flock.advance(DT, &world, &Input::default());
            show.step(DT, &mut flock, &world);
            time += DT;
            for pet in flock.pets() {
                if held.contains(&pet.id()) && !pet.is_held() {
                    set_down.push((pet.id(), pet.position().x));
                }
                if set_down.iter().any(|(id, _)| *id == pet.id())
                    && !landed.iter().any(|(id, _)| *id == pet.id())
                    && matches!(pet.support(), Support::Ground { .. })
                {
                    landed.push((pet.id(), pet.position().y));
                }
            }
            let view = show.view(&flock, &world);
            if view.saucer.is_some()
                && flock
                    .pets()
                    .iter()
                    .all(|pet| pet.is_held() && show.opacity(pet.id()) > 0.99)
            {
                all_in_light = true;
            }
            if view.saucer.is_none() && gone_at.is_none() {
                gone_at = Some(time);
            }
            assert!(time < 30.0);
        }
        assert!(all_in_light, "tous dans sa lumière avant d'être reposés");
        assert_eq!(set_down.len(), 3, "{set_down:?}");
        for (id, x) in &set_down {
            let place = ids.iter().find(|(i, _)| i == id).expect("un des siens").1;
            assert!((x - place).abs() < 30.0, "reposé en {x} au lieu de {place}");
        }
        assert!(
            set_down.windows(2).all(|w| w[0].1 < w[1].1),
            "de gauche à droite : {set_down:?}"
        );
        // Chacun sur ce qui est sous sa place : la fenêtre, ou le sol.
        assert_eq!(landed.len(), 3, "{landed:?}");
        for (id, y) in &landed {
            let place = ids.iter().find(|(i, _)| i == id).expect("un des siens").1;
            let level = world
                .surface_below(Vec2::new(place, 0.0), 0.0)
                .expect("un sol")
                .y;
            assert!(
                (y - level).abs() < 1.0,
                "posé en {y} au lieu de {level} ({place})"
            );
        }
        assert!(
            landed.iter().any(|(_, y)| *y < 500.0),
            "l'un d'eux sur la fenêtre"
        );
        assert!(gone_at.is_some_and(|t| t < 14.0), "{gone_at:?}");
        assert_eq!(flock.pets().len(), 3);
        for pet in flock.pets() {
            assert!(matches!(pet.support(), Support::Ground { .. }));
            assert!((show.opacity(pet.id()) - 1.0).abs() < 1e-6);
        }
        assert!(show.drop_point(700.0, 40.0).is_none());
    }

    #[test]
    fn a_lamb_comes_back_to_its_place_bound_to_its_mother() {
        let world = desk();
        let mut flock = Flock::new();
        flock.set_realistic(true);
        let def = bundled("texel");
        let ewe = flock.spawn_as(
            Arc::clone(&def),
            3,
            &world,
            Some(400.0),
            Some(crate::Sex::Female),
        );
        let ram = flock.spawn_as(
            Arc::clone(&def),
            4,
            &world,
            Some(900.0),
            Some(crate::Sex::Male),
        );
        let young = Arc::new(def.as_lamb(def.lamb_weeks()).expect("un agneau"));
        // Du haut de l'écran, à sa place ; ou de la lumière de la soucoupe.
        let a = flock
            .spawn_lamb_back(ewe, Arc::clone(&young), 5, &world, 650.0, None)
            .expect("le sien");
        let at = Vec2::new(300.0, 420.0);
        let b = flock
            .spawn_lamb_back(ewe, Arc::clone(&young), 6, &world, 200.0, Some(at))
            .expect("le sien aussi");
        assert!(
            flock
                .spawn_lamb_back(ram, Arc::clone(&young), 7, &world, 800.0, None)
                .is_none(),
            "pas d'un bélier"
        );
        let pet = |id: u32| flock.pets().iter().find(|p| p.id() == id).expect("là");
        assert_eq!(pet(a).mother(), Some(ewe));
        assert_eq!(pet(b).mother(), Some(ewe));
        assert!(
            (pet(a).position().x - 650.0).abs() < 2.0,
            "{:?}",
            pet(a).position()
        );
        assert!(pet(a).position().y < 120.0, "{:?}", pet(a).position());
        assert!(
            (pet(b).position() - at).length() < 2.0,
            "{:?}",
            pet(b).position()
        );
        assert_eq!(flock.pets().len(), 4);
    }
}
