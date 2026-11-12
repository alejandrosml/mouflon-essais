//! Surprises : de temps en temps, un coup de vent traverse l'écran, ou une
//! soucoupe volante vient emprunter un animal pour le déposer ailleurs.
//!
//! - Le vent ([`Gust`]) se lève, souffle par rafales, puis retombe : les
//!   animaux en l'air dérivent, ceux au sol se penchent contre lui, des
//!   feuilles passent, et certains lèvent la tête (réflexe `wind`).
//! - La soucoupe ([`Saucer`]) arrive au-dessus d'un animal posé, l'aspire
//!   dans son faisceau (elle le tient comme la main de l'utilisateur : il
//!   pend et se balance), l'emporte un peu plus loin, le lâche, puis s'en
//!   va. On peut le lui reprendre : elle renonce et repart. Un animal lancé
//!   qui la percute (son corps, pas son faisceau) la fait fumer : elle
//!   s'enfuit en trombe, ou tombe, reste un moment au sol à fumer, puis
//!   repart en boitant.
//!
//! Tout reste déterministe : les surprises ne tirent au sort qu'avec la
//! graine qu'on leur donne.

use crate::math::{Rect, Rng, Vec2, clamp_between, lerp};
use crate::pet::Pet;
use crate::world::World;

/// Accélération d'un animal en l'air au plus fort du vent (pt/s²) : il
/// dérive nettement, sans être emporté comme une feuille.
pub const GUST_PEAK: f32 = 900.0;

/// Un coup de vent qui traverse l'écran.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Gust {
    /// Sens : 1 vers la droite, -1 vers la gauche.
    pub dir: f32,
    t: f32,
    duration: f32,
    /// Les animaux ont déjà réagi à son arrivée.
    pub(crate) noticed: bool,
}

impl Gust {
    #[must_use]
    pub fn new(dir: f32, duration: f32) -> Self {
        Self {
            dir: if dir < 0.0 { -1.0 } else { 1.0 },
            t: 0.0,
            duration: duration.max(1.0),
            noticed: false,
        }
    }

    /// Force du moment, de 0 à 1 : il se lève, souffle par rafales, retombe.
    #[must_use]
    pub fn strength(&self) -> f32 {
        let u = self.t / self.duration;
        let envelope = smooth(0.0, 0.18, u) * (1.0 - smooth(0.65, 1.0, u));
        let gusts = 0.72 + 0.28 * (self.t * 5.3).sin() * (self.t * 1.7 + 0.4).cos();
        (envelope * gusts).clamp(0.0, 1.0)
    }

    /// Accélération qu'il donne à un animal en l'air (pt/s², signée).
    #[must_use]
    pub fn accel(&self) -> f32 {
        self.dir * GUST_PEAK * self.strength()
    }

    /// Avance de `dt` ; `false` une fois retombé.
    pub(crate) fn step(&mut self, dt: f32) -> bool {
        self.t += dt;
        self.t < self.duration
    }
}

fn smooth(a: f32, b: f32, x: f32) -> f32 {
    let t = ((x - a) / (b - a)).clamp(0.0, 1.0);
    t * t * (3.0 - 2.0 * t)
}

/// Taille de la soucoupe (points) : son corps, et la plus grande longueur
/// de son faisceau, sous elle.
pub const SAUCER_WIDTH: f32 = 116.0;
pub const SAUCER_HEIGHT: f32 = 52.0;
pub const BEAM_MAX: f32 = 260.0;

/// Ce que fait la soucoupe.
#[derive(Clone, Copy, Debug, PartialEq)]
enum Phase {
    /// Elle arrive au-dessus de l'animal.
    Arrive,
    /// Elle se place, les lumières s'affolent.
    Hover,
    /// Le faisceau l'aspire.
    Lift,
    /// Elle l'emporte.
    Carry,
    /// Le faisceau s'éteint : il tombe.
    Drop,
    /// Elle s'en va.
    Leave,
    /// Percutée : elle tangue et fume.
    Hit,
    /// Elle s'enfuit en trombe, en fumant.
    Flee,
    /// Elle tombe en fumant.
    Crash,
    /// Au sol, elle fume.
    Wreck,
    /// Elle repart en boitant.
    Limp,
    Gone,
}

/// Ce qu'il faut pour dessiner la soucoupe.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct SaucerView {
    /// Centre de la soucoupe (repère du monde).
    pub pos: Vec2,
    /// Penchée selon sa vitesse (radians, sens horaire).
    pub tilt: f32,
    /// Faisceau : de 0 (éteint) à 1, et jusqu'où il descend sous elle.
    pub beam: f32,
    pub beam_len: f32,
    /// Phase du clignotement de ses lumières (tours).
    pub lights: f32,
    /// Percutée : la fumée (de 0 à 1) et depuis quand elle fume (secondes).
    pub smoke: f32,
    pub smoke_time: f32,
}

/// Une soucoupe volante en visite.
#[derive(Clone, Debug)]
pub struct Saucer {
    id: u32,
    phase: Phase,
    t: f32,
    pos: Vec2,
    prev: Vec2,
    vel: Vec2,
    from: Vec2,
    to: Vec2,
    target: u32,
    /// Là où le faisceau tient l'animal (ce qu'on lui a donné au dernier pas).
    grip: Option<Vec2>,
    /// D'où il a été pris, sous la soucoupe (le faisceau le monte de là).
    lift_from: Vec2,
    /// Là où elle est venue le chercher : toute sa visite se passe sur cet
    /// écran-là (voir `World::arena_at`).
    home: Vec2,
    beam: f32,
    beam_len: f32,
    lights: f32,
    /// Percutée : ce qu'elle fera ensuite (s'enfuir ou tomber), sa fumée,
    /// depuis quand, et son inclinaison propre (elle tangue, puis gît
    /// penchée).
    flee: bool,
    smoke: f32,
    smoke_time: f32,
    spin: f32,
    rng: Rng,
}

/// Durées des phases (secondes).
const ARRIVE: f32 = 2.6;
const HOVER: f32 = 0.9;
const LIFT: f32 = 2.0;
const CARRY: f32 = 2.8;
const DROP: f32 = 0.6;
const LEAVE: f32 = 1.7;
const HIT: f32 = 0.8;
const FLEE: f32 = 1.0;
const WRECK: f32 = 3.5;
const LIMP: f32 = 2.6;
/// Sa chute (pt/s²) : un peu moins que la pesanteur, elle plane encore.
const FALL: f32 = 1500.0;

/// La vitesse d'un animal par rapport à elle, au-delà de laquelle il la
/// percute (pt/s) ; en deçà, il ne fait que la frôler.
pub const HIT_SPEED: f32 = 220.0;

/// Hauteur de la soucoupe au-dessus du sol de l'animal qu'elle vient prendre.
const ABOVE: f32 = 190.0;
/// L'animal pend dans le faisceau, ce peu sous la soucoupe (points).
const HANG: f32 = 16.0;

impl Saucer {
    /// Une soucoupe qui vient chercher l'animal `target`, posé en `ground`.
    #[must_use]
    pub fn new(id: u32, target: u32, ground: Vec2, arena: Rect, seed: u64) -> Self {
        let mut rng = Rng::new(seed);
        // Elle arrive du côté de l'écran le plus loin de lui, par le haut.
        let from_left = ground.x > arena.x + arena.w * 0.5;
        let x = if from_left {
            arena.left() - SAUCER_WIDTH
        } else {
            arena.right() + SAUCER_WIDTH
        };
        let start = Vec2::new(x, arena.top() + rng.range(30.0, 120.0));
        let hover = Self::above(ground, arena);
        Self {
            id,
            phase: Phase::Arrive,
            t: 0.0,
            pos: start,
            prev: start,
            vel: Vec2::ZERO,
            from: start,
            to: hover,
            target,
            grip: None,
            lift_from: ground,
            home: ground,
            beam: 0.0,
            beam_len: 0.0,
            lights: 0.0,
            flee: false,
            smoke: 0.0,
            smoke_time: 0.0,
            spin: 0.0,
            rng,
        }
    }

    /// Là où elle se tient au-dessus d'un animal posé en `ground`.
    fn above(ground: Vec2, arena: Rect) -> Vec2 {
        let top = arena.top() + SAUCER_HEIGHT * 0.5 + 8.0;
        Vec2::new(
            clamp_between(
                ground.x,
                arena.left() + SAUCER_WIDTH,
                arena.right() - SAUCER_WIDTH,
            ),
            (ground.y - ABOVE).max(top),
        )
    }

    #[must_use]
    pub fn id(&self) -> u32 {
        self.id
    }

    /// L'animal qu'elle est venue chercher.
    #[must_use]
    pub fn target(&self) -> u32 {
        self.target
    }

    /// Partie, plus rien à dessiner.
    #[must_use]
    pub fn is_gone(&self) -> bool {
        self.phase == Phase::Gone
    }

    #[must_use]
    pub fn view(&self) -> SaucerView {
        SaucerView {
            pos: self.pos,
            tilt: (self.vel.x * 0.0005).clamp(-0.32, 0.32) + self.spin,
            beam: self.beam,
            beam_len: self.beam_len,
            lights: self.lights,
            smoke: self.smoke,
            smoke_time: self.smoke_time,
        }
    }

    /// Peut-on la percuter (elle ne l'est pas déjà) ?
    #[must_use]
    pub fn can_be_hit(&self) -> bool {
        !matches!(
            self.phase,
            Phase::Hit | Phase::Flee | Phase::Crash | Phase::Wreck | Phase::Limp | Phase::Gone
        )
    }

    /// Le point `p` (un animal de rayon `radius`) touche-t-il son corps (la
    /// coque ou le dôme, pas le faisceau) ?
    #[must_use]
    pub fn touches(&self, p: Vec2, radius: f32) -> bool {
        let inside = |center: Vec2, rx: f32, ry: f32| {
            let d = p - center;
            (d.x / (rx + radius)).powi(2) + (d.y / (ry + radius)).powi(2) <= 1.0
        };
        let (w, h) = (SAUCER_WIDTH, SAUCER_HEIGHT);
        inside(self.pos + Vec2::new(0.0, h * 0.08), w * 0.5, h * 0.27)
            || inside(self.pos + Vec2::new(0.0, -h * 0.12), w * 0.24, h * 0.42)
    }

    /// L'animal qu'elle tient dans son faisceau, s'il y en a un.
    #[must_use]
    pub fn holds(&self) -> bool {
        self.grip.is_some()
    }

    /// Percutée : elle lâche ce qu'elle tient, tangue, fume, puis s'enfuit
    /// ou tombe (au hasard). Renvoie `true` si elle tenait l'animal qu'elle
    /// était venue chercher (qu'il faut lâcher).
    pub fn hit(&mut self) -> bool {
        let held = self.grip.take().is_some();
        self.flee = self.rng.chance(0.5);
        self.smoke = 1.0;
        self.smoke_time = 0.0;
        self.beam = 0.0;
        self.go(Phase::Hit, self.pos);
        held
    }

    /// Sa vitesse (pt/s).
    #[must_use]
    pub fn velocity(&self) -> Vec2 {
        self.vel
    }

    fn go(&mut self, phase: Phase, to: Vec2) {
        self.phase = phase;
        self.t = 0.0;
        self.from = self.pos;
        self.to = to;
    }

    /// Elle renonce (l'animal lui a été repris, ou il est parti) : elle s'en va.
    fn give_up(&mut self, arena: Rect) {
        self.grip = None;
        let side = if self.pos.x > arena.x + arena.w * 0.5 {
            1.0
        } else {
            -1.0
        };
        let away = Vec2::new(self.pos.x + side * 300.0, arena.top() - SAUCER_HEIGHT * 3.0);
        self.go(Phase::Leave, away);
    }

    /// Un pas de sa visite ; elle tient l'animal `pet` (s'il est encore là)
    /// comme une main.
    pub(crate) fn step(&mut self, dt: f32, pet: Option<&mut Pet>, world: &World) {
        let arena = world.arena_at(self.home);
        self.t += dt;
        self.lights += dt * if self.beam > 0.05 { 3.2 } else { 1.1 };
        let ease = |t: f32, d: f32| smooth(0.0, 1.0, t / d);
        // L'animal lui a-t-il été repris (la main de l'utilisateur, un autre
        // geste) ? Elle le laisse et s'en va.
        if let (Some(grip), Some(pet)) = (self.grip, pet.as_deref())
            && pet.hold_target() != Some(grip)
        {
            self.give_up(arena);
        }
        let mut pet = pet;
        let gone = pet.is_none()
            || pet
                .as_deref()
                .is_some_and(|p| p.is_leaving() || p.is_finished());
        if gone && !matches!(self.phase, Phase::Leave | Phase::Gone) {
            // Il part (au revoir) : s'il était dans le faisceau, il en tombe.
            if let (Some(_), Some(pet)) = (self.grip, pet.as_deref_mut()) {
                pet.release(world);
            }
            self.give_up(arena);
        }
        match self.phase {
            Phase::Arrive => {
                // Elle suit l'animal s'il bouge encore.
                if let Some(pet) = pet.as_deref() {
                    self.to = Self::above(pet.position(), arena);
                }
                let u = ease(self.t, ARRIVE);
                let arc = (u * std::f32::consts::PI).sin() * -60.0;
                self.pos = self.from.lerp(self.to, u) + Vec2::new(0.0, arc);
                if self.t >= ARRIVE {
                    self.go(Phase::Hover, self.to);
                }
            }
            Phase::Hover => {
                let wobble = Vec2::new((self.t * 9.0).sin() * 3.0, (self.t * 6.0).sin() * 2.0);
                self.pos = self.to + wobble;
                self.beam = (self.t / HOVER).min(1.0) * 0.35;
                if self.t >= HOVER {
                    self.go(Phase::Lift, self.to);
                }
            }
            Phase::Lift => {
                self.pos = self.to + Vec2::new(0.0, (self.t * 6.0).sin() * 1.5);
                self.beam = lerp(0.35, 1.0, (self.t / 0.3).min(1.0));
                if let Some(pet) = pet {
                    let bottom = self.pos + Vec2::new(0.0, SAUCER_HEIGHT * 0.45 + HANG);
                    if self.grip.is_none() {
                        // Pris par le haut du dos.
                        let body = pet.body();
                        let top = body.center - Vec2::new(0.0, body.half.y * 0.85);
                        pet.grab(top, world);
                        self.lift_from = top;
                    }
                    // Il monte doucement, puis de plus en plus vite.
                    let u = (self.t / LIFT).clamp(0.0, 1.0);
                    let rise = u * u * (3.0 - 2.0 * u);
                    let grip = self.lift_from.lerp(bottom, rise)
                        + Vec2::new((self.t * 4.0).sin() * 4.0 * (1.0 - rise), 0.0);
                    pet.drag_to(grip);
                    self.grip = Some(grip);
                    self.beam_len = (self.lift_from.y - bottom.y + pet_height(pet)).max(40.0);
                }
                if self.t >= LIFT {
                    // Un peu plus loin, au moins à 280 points, sans sortir de l'écran.
                    let span = (arena.w - 2.0 * SAUCER_WIDTH).max(1.0);
                    let mut x = arena.left() + SAUCER_WIDTH + self.rng.range(0.0, span);
                    if (x - self.pos.x).abs() < 280.0 {
                        x = if self.pos.x > arena.x + arena.w * 0.5 {
                            (self.pos.x - 320.0).max(arena.left() + SAUCER_WIDTH)
                        } else {
                            (self.pos.x + 320.0).min(arena.right() - SAUCER_WIDTH)
                        };
                    }
                    let y = arena.top() + self.rng.range(90.0, 220.0);
                    self.go(Phase::Carry, Vec2::new(x, y));
                }
            }
            Phase::Carry => {
                let u = ease(self.t, CARRY);
                self.pos = self.from.lerp(self.to, u)
                    + Vec2::new(0.0, (u * std::f32::consts::PI).sin() * -40.0);
                if let Some(pet) = pet {
                    // Il pend dans le faisceau, juste sous elle.
                    let grip = self.pos + Vec2::new(0.0, SAUCER_HEIGHT * 0.45 + HANG);
                    pet.drag_to(grip);
                    self.grip = Some(grip);
                    self.beam_len = pet_height(pet) * 1.4;
                }
                if self.t >= CARRY {
                    self.go(Phase::Drop, self.to);
                }
            }
            Phase::Drop => {
                if let Some(pet) = pet
                    && self.grip.take().is_some()
                {
                    pet.release(world);
                }
                self.pos = self.to + Vec2::new(0.0, (self.t * 7.0).sin() * 2.0);
                self.beam = (1.0 - self.t / DROP).max(0.0);
                if self.t >= DROP {
                    let side = if self.to.x > arena.x + arena.w * 0.5 {
                        1.0
                    } else {
                        -1.0
                    };
                    self.go(
                        Phase::Leave,
                        Vec2::new(self.pos.x + side * 420.0, arena.top() - SAUCER_HEIGHT * 3.0),
                    );
                }
            }
            Phase::Leave => {
                self.beam = (self.beam - dt * 3.0).max(0.0);
                // De plus en plus vite.
                let u = (self.t / LEAVE).clamp(0.0, 1.0);
                self.pos = self.from.lerp(self.to, u * u);
                if self.t >= LEAVE {
                    self.phase = Phase::Gone;
                }
            }
            Phase::Hit => {
                // Elle tangue, de moins en moins, et commence à fumer.
                let sway = (1.0 - self.t / HIT).max(0.0);
                self.spin = 0.35 * sway * (self.t * 22.0).sin();
                self.pos = self.to + Vec2::new((self.t * 31.0).sin() * 4.0 * sway, self.t * 10.0);
                if self.t >= HIT {
                    if self.flee {
                        let side = if self.pos.x > arena.x + arena.w * 0.5 {
                            1.0
                        } else {
                            -1.0
                        };
                        let away = Vec2::new(
                            self.pos.x + side * (arena.w * 0.7 + SAUCER_WIDTH),
                            arena.top() - SAUCER_HEIGHT * 4.0,
                        );
                        self.go(Phase::Flee, away);
                    } else {
                        self.vel = Vec2::new(self.rng.range(-80.0, 80.0), 0.0);
                        self.go(Phase::Crash, self.pos);
                    }
                }
            }
            Phase::Flee => {
                // En trombe, de plus en plus vite, en zigzaguant un peu.
                let u = (self.t / FLEE).clamp(0.0, 1.0);
                self.spin = 0.25 * (self.t * 14.0).sin() * (1.0 - u);
                self.pos = self.from.lerp(self.to, u * u * u)
                    + Vec2::new(0.0, (self.t * 17.0).sin() * 6.0);
                if self.t >= FLEE {
                    self.phase = Phase::Gone;
                }
            }
            Phase::Crash => {
                // Elle tombe en tournoyant jusqu'au sol (le sol de l'écran
                // sous elle), puis s'y pose, penchée.
                let mut vel = self.vel;
                vel.y += FALL * dt;
                vel.x *= (-1.5 * dt).exp();
                self.pos += vel * dt;
                self.spin = (self.spin + dt * 1.6 * if vel.x < 0.0 { -1.0 } else { 1.0 })
                    .clamp(-0.45, 0.45);
                let floor = world
                    .floor_below(self.pos.x, self.pos.y)
                    .map_or(arena.bottom(), |f| f.y);
                let rest = floor - SAUCER_HEIGHT * 0.3;
                if self.pos.y >= rest {
                    self.pos.y = rest;
                    self.spin = 0.22 * if self.spin < 0.0 { -1.0 } else { 1.0 };
                    self.go(Phase::Wreck, self.pos);
                }
                // La vitesse de ce pas sert à tomber (et à la dessiner penchée).
                self.vel = vel;
                self.prev = self.pos;
                self.lights += dt * 4.0;
                return;
            }
            Phase::Wreck => {
                // Au sol, elle fume ; les lumières clignotent mal.
                self.pos = self.to;
                if self.t >= WRECK {
                    let side = if self.pos.x > arena.x + arena.w * 0.5 {
                        1.0
                    } else {
                        -1.0
                    };
                    let away =
                        Vec2::new(self.pos.x + side * 360.0, arena.top() - SAUCER_HEIGHT * 3.0);
                    self.go(Phase::Limp, away);
                }
            }
            Phase::Limp => {
                // Elle repart en boitant : par à-coups, penchée, en fumant
                // de moins en moins.
                let u = (self.t / LIMP).clamp(0.0, 1.0);
                let hop = (self.t * 7.0).sin().abs() * 10.0 * (1.0 - u);
                self.spin = 0.22 * (1.0 - u) * if self.spin < 0.0 { -1.0 } else { 1.0 }
                    + 0.08 * (self.t * 9.0).sin();
                self.pos = self.from.lerp(self.to, u * u) - Vec2::new(0.0, hop);
                if self.t >= LIMP {
                    self.phase = Phase::Gone;
                }
            }
            Phase::Gone => {}
        }
        if self.smoke > 0.0 {
            self.smoke_time += dt;
            // Elle fume tant qu'elle est au sol ; en partant, la fumée
            // s'éclaircit.
            if matches!(self.phase, Phase::Limp | Phase::Flee) {
                self.smoke = (self.smoke - dt * 0.35).max(0.15);
            }
        }
        self.vel = (self.pos - self.prev) * (1.0 / dt.max(1e-4));
        self.prev = self.pos;
    }
}

fn pet_height(pet: &Pet) -> f32 {
    pet.body().half.y * 2.0
}
