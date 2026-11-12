//! Sauter sur un rebord plus haut, tout proche, comme un mouflon sur un
//! rocher (voir `docs/animation-animale.md`, « Sauter sur un rebord plus
//! haut »).
//!
//! Arrêté par le côté d'une fenêtre plus haute, ou au bout de son rebord
//! face à un rebord plus haut juste devant, l'animal regarde s'il est à sa
//! portée : la hauteur à monter, la distance à franchir, et la vitesse
//! d'envol qu'il faudrait, comparée à celle de son plus haut bond (`reach`,
//! en longueurs de patte). À sa portée, sa réaction peut être de sauter
//! (un comportement `motion = "leap"` dans ses réflexes `window_side` et
//! `edge`) ; hors de portée, le moteur ne le lui propose pas. Alors :
//!
//! 1. il mesure le rebord du regard, la tête levée ;
//! 2. trop près pour bondir par-dessus le coin, il recule d'un pas ou deux
//!    pour prendre son élan (plus loin, il s'avance jusqu'à son point
//!    d'envol) ;
//! 3. il se ramasse, l'arrière plus bas ;
//! 4. il bondit : une parabole qui passe au-dessus du coin avec de la
//!    marge, son sommet un peu plus haut que le rebord (comme le chien qui
//!    choisit le saut qui lui coûte le moins, avec une marge), et qui se
//!    pose juste au-delà du coin, les quatre sabots sur le rebord ;
//! 5. il se reçoit : il touche le rebord les pattes tendues sous lui, les
//!    genoux amortissent tout son élan, les sabots restent plantés là où
//!    ils se posent ; puis il repart d'aplomb, ou regarde autour de lui.
//!
//! Le **grand bond** d'un écran à l'autre (voir `pet/voyage.rs`) est le même
//! saut, sans la limite de sa portée : c'est ainsi qu'il gagne le sol d'un
//! écran voisin bien plus haut que le sien, ou celui de l'écran du dessus,
//! à travers lequel il passe en montant.

use crate::def::{Motion, PetDef};
use crate::math::Vec2;
use crate::physics::PET_SURFACE;
use crate::temper::Temper;
use crate::world::{Surface, SurfaceId, SurfaceKind, World};

use super::{Pet, STRIDE_PER_LEG, Support};

/// Une marche plus basse que ceci (en longueurs de patte) n'est pas un
/// rebord où sauter.
const MIN_RISE: f32 = 0.1;
/// Un rebord dont le coin est plus loin que ceci devant lui (en longueurs
/// de son corps) n'est plus « tout proche » ; forcé (un aperçu, un essai),
/// il va le chercher jusqu'à trois longueurs.
const NEAR: f32 = 1.0;
const FORCED_NEAR: f32 = 3.0;
/// Le sommet du bond, au-dessus du rebord : une part de sa hauteur et une
/// part de la hauteur du rebord.
const APEX_BODY: f32 = 0.3;
const APEX_RISE: f32 = 0.2;
/// En passant le coin, le bas de son corps le dépasse au moins de ceci
/// (une part de sa hauteur, plus deux points).
const CORNER_BODY: f32 = 0.08;
/// Il se pose son point d'appui à ceci au-delà du coin (en longueurs de son
/// corps) : le bas de son corps, et ses quatre sabots, sur le rebord.
const LAND_IN: f32 = 0.62;
/// Le rebord doit être au moins aussi long que ceci (en longueurs de son
/// corps) pour qu'il s'y pose.
const LAND_ROOM: f32 = 1.0;
/// Derrière son point d'envol, sa croupe reste sur son rebord avec cette
/// marge (en longueurs de son corps).
const BACK_ROOM: f32 = 0.36;
/// Pour reculer, ou s'avancer, jusqu'à son point d'envol : en longueurs de
/// patte par seconde (un pas, un pas vif).
const BACK_SPEED: f32 = 1.3;
const STEP_SPEED: f32 = 2.2;
/// Il mesure le rebord du regard ce temps-là (secondes), plus longtemps
/// s'il est haut ; puis il se ramasse.
const MEASURE: (f32, f32) = (0.45, 0.45);
const GATHER: (f32, f32) = (0.14, 0.16);
/// Sans avoir bondi au bout de ce temps (secondes), il renonce.
const GIVE_UP: f32 = 6.0;
/// Un grand bond passe moins haut au-dessus du sol qu'il vise : la part de
/// la hauteur à monter, dans son sommet, ne dépasse pas ceci (une part de
/// sa hauteur à lui).
const FAR_APEX: f32 = 0.5;
/// Après s'être posé, il se reçoit ce temps-là (secondes), sabots plantés.
pub(super) const LANDING: f32 = 0.45;

/// Où il en est de son saut.
#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) enum Stage {
    /// Il mesure le rebord du regard (le temps qui reste).
    Measure(f32),
    /// Il recule, ou s'avance, jusqu'à son point d'envol.
    Approach,
    /// Il se ramasse (voir `Pet::takeoff`).
    Gather,
    /// En l'air.
    Flight,
}

/// Un saut sur un rebord : celui qu'on lui propose, puis celui qu'il fait.
#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) struct Leap {
    /// Le rebord visé.
    pub target: SurfaceId,
    /// Le sens du saut (1 : vers la droite).
    pub dir: f32,
    /// Le coin du rebord (son bout de ce côté-ci), et sa hauteur au-dessus
    /// de lui, tels qu'ils sont à présent : si la fenêtre bouge, il suit.
    pub corner: f32,
    pub rise: f32,
    /// D'où il s'envole : la distance de l'avant de son corps au coin.
    pub gap: f32,
    /// La vitesse d'envol qu'il faut (pt/s).
    pub speed: f32,
    pub stage: Stage,
    /// Depuis combien de temps il s'y prépare (secondes).
    pub time: f32,
    /// En l'air : le temps que doit durer son vol (secondes).
    pub flight: f32,
    /// Un grand bond vers le sol d'un autre écran : sa portée ne le limite
    /// pas (voir `pet/voyage.rs`).
    pub far: bool,
    /// Il passe à travers le sol qu'il vise, en montant (l'écran du
    /// dessus) : son « coin » est un point fixe devant lui, pas un bout de
    /// ce sol.
    pub through: bool,
}

impl Leap {
    /// D'où il s'envole (son point d'appui), pour un corps large de `width`.
    pub(super) fn takeoff(&self, width: f32) -> f32 {
        self.corner - self.dir * (self.gap + width * 0.5)
    }

    /// Où il se pose (son point d'appui).
    pub(super) fn landing(&self, width: f32) -> f32 {
        self.corner + self.dir * LAND_IN * width
    }
}

/// Ce qui compte de son corps pour un bond.
#[derive(Clone, Copy, Debug)]
struct Build {
    width: f32,
    height: f32,
    leg: f32,
    gravity: f32,
    drag: f32,
}

impl Build {
    fn of(def: &PetDef) -> Self {
        let p = def.physics;
        Self {
            width: p.width,
            height: p.height,
            leg: (def.stride() / STRIDE_PER_LEG).max(1.0),
            gravity: p.gravity.max(1.0),
            drag: p.air_drag.max(0.0),
        }
    }

    /// Le chemin vers l'avant au bout de `t` secondes, pour une vitesse
    /// d'envol d'un point par seconde : l'air le freine (voir
    /// `Physics::air_drag`).
    fn ahead(&self, t: f32) -> f32 {
        if self.drag < 1e-4 {
            t
        } else {
            (1.0 - (-self.drag * t).exp()) / self.drag
        }
    }

    /// Le temps qu'il met à faire `a` (en unités de [`Self::ahead`]).
    fn time_to(&self, a: f32) -> f32 {
        if self.drag < 1e-4 {
            a
        } else {
            let left = 1.0 - self.drag * a;
            if left <= 0.0 {
                f32::INFINITY
            } else {
                -left.ln() / self.drag
            }
        }
    }
}

/// Un bond : la vitesse d'envol (vers l'avant, vers le haut, positives) et
/// la distance de l'avant de son corps au coin, d'où il part.
#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) struct Bound {
    pub forward: f32,
    pub up: f32,
    pub gap: f32,
    /// Le temps de vol, de l'envol à la réception (secondes).
    pub time: f32,
}

impl Bound {
    fn speed(&self) -> f32 {
        self.forward.hypot(self.up)
    }
}

/// Le bond qui le monte de `rise` points sur un rebord dont le coin est à
/// `gap` points devant lui (`None` : au plus près du coin qu'il peut
/// partir). Le sommet est un peu au-dessus du rebord ; le bas de son corps
/// passe le coin avec de la marge, et il se pose juste au-delà. (`far` : un
/// grand bond, dont le sommet reste près du sol visé.)
fn bound(b: &Build, rise: f32, gap: Option<f32>, far: bool) -> Option<Bound> {
    let g = b.gravity;
    let corner = CORNER_BODY * b.height + 2.0;
    // De l'avant de son corps à son point d'appui, puis au-delà du coin.
    let rest = b.width * (0.5 + LAND_IN);
    let mut apex = apex_over(b, rise, far).max(corner * 1.5);
    for _ in 0..8 {
        let up = (2.0 * g * (rise + apex)).sqrt();
        let time = up / g + (2.0 * apex / g).sqrt();
        let clear = up * up - 2.0 * g * (rise + corner);
        let (over, under) = ((up - clear.sqrt()) / g, (up + clear.sqrt()) / g);
        let (forward, gap) = match gap {
            None => {
                let forward = rest / (b.ahead(time) - b.ahead(over));
                (forward, forward * b.ahead(over))
            }
            Some(gap) => ((gap.max(0.0) + rest) / b.ahead(time), gap.max(0.0)),
        };
        // L'avant de son corps passe le coin quand il est au-dessus.
        let crossing = b.time_to(gap / forward);
        if forward.is_finite() && crossing >= over - 1e-4 && crossing <= under {
            return Some(Bound {
                forward,
                up,
                gap,
                time,
            });
        }
        apex *= 1.3;
    }
    None
}

/// De combien le sommet du bond dépasse le rebord visé.
fn apex_over(b: &Build, rise: f32, far: bool) -> f32 {
    let part = APEX_RISE * rise;
    APEX_BODY * b.height
        + if far {
            part.min(FAR_APEX * b.height)
        } else {
            part
        }
}

/// La vitesse d'envol de son plus haut bond : droit sur un rebord de
/// `reach` longueurs de patte, au plus près du coin.
fn top_speed(b: &Build, reach: f32) -> f32 {
    bound(b, reach * b.leg, None, false).map_or(0.0, |bound| bound.speed())
}

impl Pet {
    /// La plus grande portée de ses sauts sur un rebord (longueurs de
    /// patte) ; zéro : il n'en fait pas.
    fn leap_reach(&self) -> f32 {
        self.def
            .behaviors
            .iter()
            .filter_map(|b| match b.motion {
                Motion::Leap { reach } => Some(reach),
                _ => None,
            })
            .fold(0.0, f32::max)
    }

    /// Un saut de portée `reach` peut-il faire celui qu'on lui propose ?
    /// (Sans proposition en cours, rien ne l'empêche : il cherchera son
    /// rebord lui-même.)
    pub(super) fn leap_allowed(&self, reach: f32) -> bool {
        if !self.leap_asked {
            return true;
        }
        self.leap_offer
            .is_some_and(|offer| offer.speed <= top_speed(&Build::of(&self.def), reach) * 1.0001)
    }

    /// Avant de choisir sa réaction (le côté d'une fenêtre, le bout de son
    /// rebord, dans le sens `dir`) : un rebord plus haut est-il à sa portée,
    /// tout près devant ? Les sauts sur un rebord ne lui sont proposés que
    /// si c'est le cas (voir [`Self::leap_allowed`]).
    pub(super) fn offer_leap(&mut self, world: &World, dir: f32) {
        self.leap_asked = true;
        let reach = self.leap_reach();
        let surface = match self.support {
            Support::Ground { surface } => world.surface(surface).copied(),
            _ => None,
        };
        self.leap_offer = surface.and_then(|s| {
            let near = NEAR * self.def.physics.width;
            self.find_leap(world, &s, dir, reach, near)
        });
    }

    /// La réaction est choisie : on ne lui propose plus rien.
    pub(super) fn close_leap_offer(&mut self) {
        self.leap_asked = false;
    }

    /// Le rebord plus haut le plus proche devant lui (dans le sens `dir`),
    /// depuis `surface` qui le porte, à sa portée (`reach`) et dont le coin
    /// est à moins de `near` points de l'avant de son corps : le saut qu'il
    /// y ferait.
    pub(super) fn find_leap(
        &self,
        world: &World,
        surface: &Surface,
        dir: f32,
        reach: f32,
        near: f32,
    ) -> Option<Leap> {
        if reach <= 0.0 || dir == 0.0 {
            return None;
        }
        let b = Build::of(&self.def);
        let top = top_speed(&b, reach);
        let half = b.width * 0.5;
        let front = self.pos.x + dir * half;
        let (x0, x1) = if dir > 0.0 {
            (front - 1.0, front + near)
        } else {
            (front - near, front + 1.0)
        };
        let ceiling = world.ceiling(self.pos.x - half, self.pos.x + half, self.pos.y);
        let back_end = if dir > 0.0 { surface.x0 } else { surface.x1 };
        let surfaces = world.surfaces();
        let mut best: Option<Leap> = None;
        for i in world.near_surfaces(x0, x1) {
            let s = &surfaces[i];
            // Un rebord de fenêtre ; ou, quand les écrans ne sont pas de
            // plain-pied, le sol d'un écran voisin, un peu plus haut.
            if s.id == surface.id
                || s.id & PET_SURFACE != 0
                || (s.kind != SurfaceKind::Window && world.is_plain())
            {
                continue;
            }
            let rise = surface.y - s.y;
            if rise < MIN_RISE * b.leg || rise > reach * b.leg + 1e-3 {
                continue;
            }
            let corner = if dir > 0.0 { s.x0 } else { s.x1 };
            let gap = (corner - front) * dir;
            if !(-0.5..=near).contains(&gap) || s.width() < LAND_ROOM * b.width {
                continue;
            }
            // Le plus court d'abord ; s'il est déjà plus loin du coin, il
            // part d'où il est, d'un bond plus long.
            let Some(shortest) = bound(&b, rise, None, false) else {
                continue;
            };
            let jump = if gap > shortest.gap {
                bound(&b, rise, Some(gap), false)
            } else {
                Some(shortest)
            };
            let Some(jump) = jump else {
                continue;
            };
            if jump.speed() > top * 1.0001 {
                continue;
            }
            // Reculer pour prendre son élan : sa croupe reste sur son rebord.
            let takeoff = corner - dir * (jump.gap + half);
            if (takeoff - back_end) * dir < BACK_ROOM * b.width {
                continue;
            }
            // Le sommet du bond reste sous le haut de l'écran.
            let apex = s.y - (APEX_BODY * b.height + APEX_RISE * rise) - b.height;
            if apex < ceiling {
                continue;
            }
            let leap = Leap {
                target: s.id,
                dir,
                corner,
                rise,
                gap: jump.gap,
                speed: jump.speed(),
                stage: Stage::Measure(0.0),
                time: 0.0,
                flight: jump.time,
                far: false,
                through: false,
            };
            if best.is_none_or(|old| (gap, rise) < ((old.corner - front) * dir, old.rise)) {
                best = Some(leap);
            }
        }
        best
    }

    /// Sait-il sauter sur un rebord ? Le comportement qui le fait.
    pub(super) fn leap_behavior(&self) -> Option<usize> {
        self.def
            .behaviors
            .iter()
            .position(|b| matches!(b.motion, Motion::Leap { .. }))
    }

    /// Le chemin que fait son corps vers l'avant pendant le grand bond qui
    /// le monte de `rise` points, parti au plus près : de l'avant de son
    /// corps au coin, puis de son point d'envol à l'endroit où il se pose.
    pub(super) fn far_run(&self, rise: f32) -> Option<(f32, f32)> {
        let b = Build::of(&self.def);
        let jump = bound(&b, rise, None, true)?;
        Some((jump.gap, jump.gap + b.width * (0.5 + LAND_IN)))
    }

    /// Le grand bond qui le poserait sur `target`, le sol d'un autre écran,
    /// plus haut que `surface` qui le porte, dans le sens `dir` (voir
    /// `pet/voyage.rs`). `corner` : le point fixe au-dessus duquel il passe,
    /// quand il traverse ce sol en montant (l'écran du dessus) ; sinon, le
    /// bout de ce sol de son côté (un écran voisin). `from` : d'où il part
    /// (son point d'appui) ; `None` : du plus près qu'il peut.
    ///
    /// Rien s'il n'a pas la place de prendre son élan, ni celle de se poser,
    /// ou si son bond sortirait par le haut de ce que montrent les écrans.
    pub(super) fn far_leap(
        &self,
        world: &World,
        surface: &Surface,
        target: &Surface,
        dir: f32,
        corner: Option<f32>,
        from: Option<f32>,
    ) -> Option<Leap> {
        let b = Build::of(&self.def);
        let half = b.width * 0.5;
        let rise = surface.y - target.y;
        if dir == 0.0 || rise < MIN_RISE * b.leg || target.kind != SurfaceKind::Floor {
            return None;
        }
        let through = corner.is_some();
        let corner = corner.unwrap_or(if dir > 0.0 { target.x0 } else { target.x1 });
        let shortest = bound(&b, rise, None, true)?;
        let gap = from.map(|x| (corner - (x + dir * half)) * dir);
        let jump = match gap {
            Some(gap) if gap > shortest.gap => bound(&b, rise, Some(gap), true)?,
            _ => shortest,
        };
        let leap = Leap {
            target: target.id,
            dir,
            corner,
            rise,
            gap: jump.gap,
            speed: jump.speed(),
            stage: Stage::Measure(0.0),
            time: 0.0,
            flight: jump.time,
            far: true,
            through,
        };
        // Sa croupe reste sur son sol ; ses quatre sabots tiennent sur l'autre.
        let takeoff = leap.takeoff(b.width);
        let back_end = if dir > 0.0 { surface.x0 } else { surface.x1 };
        let landing = leap.landing(b.width);
        if (takeoff - back_end) * dir < BACK_ROOM * b.width
            || landing - half < target.x0
            || landing + half > target.x1
        {
            return None;
        }
        // Le sommet du bond reste sous le haut de ce que montrent les écrans.
        let top = target.y - apex_over(&b, rise, true) - b.height;
        if top < world.ceiling(takeoff - half, takeoff + half, surface.y) {
            return None;
        }
        Some(leap)
    }

    /// Il commence son saut : celui qu'on lui a proposé (sinon, il cherchera
    /// son rebord en marchant, voir [`Self::leap_ground`]).
    pub(super) fn begin_leap(&mut self) {
        self.leap = self.leap_offer.take();
        if let Some(mut leap) = self.leap {
            leap.stage = Stage::Measure(self.measure_time(&leap));
            self.leap = Some(leap);
        }
    }

    /// Le temps de mesurer le rebord du regard : plus haut, plus longtemps ;
    /// un calme prend son temps, un actif ou un énervé moins.
    fn measure_time(&mut self, leap: &Leap) -> f32 {
        let b = Build::of(&self.def);
        let high = (leap.rise / (self.leap_reach() * b.leg).max(1.0)).clamp(0.0, 1.0);
        let temper = match self.temper {
            Temper::Calm => 1.5,
            Temper::Lively => 0.65,
            Temper::Rowdy => 0.5,
            Temper::Even => 1.0,
        };
        (MEASURE.0 + MEASURE.1 * high) * temper * self.rng.range(0.85, 1.15)
    }

    /// Le saut, tant qu'il est au sol sur `surface` : il mesure le rebord,
    /// va à son point d'envol, se ramasse. Renvoie la vitesse qu'il veut
    /// (pt/s, positive vers l'avant, négative à reculons).
    pub(super) fn leap_ground(&mut self, dt: f32, world: &World, surface: &Surface) -> f32 {
        let Motion::Leap { reach } = self.def.behaviors[self.behavior].motion else {
            return 0.0;
        };
        let b = Build::of(&self.def);
        if self.leap.is_none() {
            // Forcé (un aperçu, un essai) : le rebord le plus proche devant.
            self.leap = self.find_leap(world, surface, self.facing, reach, FORCED_NEAR * b.width);
            let Some(mut leap) = self.leap else {
                self.give_up_leap();
                return 0.0;
            };
            leap.stage = Stage::Measure(self.measure_time(&leap));
            self.leap = Some(leap);
        }
        let Some(mut leap) = self.leap else {
            return 0.0;
        };
        // Le rebord a-t-il bougé ? Disparu, trop haut, ou s'il s'est
        // retourné, il renonce.
        let Some(target) = world.surface(leap.target).copied() else {
            self.give_up_leap();
            return 0.0;
        };
        if !leap.through {
            leap.corner = if leap.dir > 0.0 { target.x0 } else { target.x1 };
        }
        leap.rise = surface.y - target.y;
        leap.time += dt;
        if leap.rise < MIN_RISE * b.leg
            || (!leap.far && leap.rise > reach * b.leg + 1e-3)
            || self.facing != leap.dir
            || leap.time > GIVE_UP
        {
            self.give_up_leap();
            return 0.0;
        }
        let mut speed = 0.0;
        match leap.stage {
            Stage::Measure(left) => {
                leap.stage = if left > dt {
                    Stage::Measure(left - dt)
                } else {
                    Stage::Approach
                };
            }
            Stage::Approach => {
                let takeoff = leap.corner - leap.dir * (leap.gap + b.width * 0.5);
                let ahead = (takeoff - self.pos.x) * leap.dir;
                if ahead.abs() <= 0.5 {
                    self.vel.x = 0.0;
                    leap.stage = Stage::Gather;
                    let high = (leap.rise / (reach * b.leg)).clamp(0.0, 1.0);
                    self.takeoff = Some((GATHER.0 + GATHER.1 * high, Vec2::ZERO));
                } else {
                    let pace = if ahead > 0.0 { STEP_SPEED } else { BACK_SPEED } * b.leg;
                    speed = ahead.signum() * pace.min(ahead.abs() * 8.0 + 4.0);
                }
            }
            Stage::Gather | Stage::Flight => {}
        }
        self.leap = Some(leap);
        speed
    }

    /// Ramassé, il bondit (voir `Pet::step_takeoff`) : le bond qui le pose
    /// sur son rebord, d'où il est à présent. Il renonce si le rebord n'est
    /// plus là, ou plus à sa portée.
    pub(super) fn leap_off(&mut self, world: &World) {
        let Some(mut leap) = self.leap else {
            return;
        };
        let Motion::Leap { reach } = self.def.behaviors[self.behavior].motion else {
            self.leap = None;
            return;
        };
        let b = Build::of(&self.def);
        let jump = world.surface(leap.target).copied().and_then(|target| {
            let corner = if leap.through {
                leap.corner
            } else if leap.dir > 0.0 {
                target.x0
            } else {
                target.x1
            };
            let front = self.pos.x + leap.dir * b.width * 0.5;
            let rise = self.pos.y - target.y;
            let gap = ((corner - front) * leap.dir).max(0.0);
            (rise >= MIN_RISE * b.leg && (leap.far || rise <= reach * b.leg + 1e-3))
                .then(|| bound(&b, rise, Some(gap), leap.far))
                .flatten()
                // Une fenêtre a pu bouger un peu pendant qu'il se ramassait.
                .filter(|jump| leap.far || jump.speed() <= top_speed(&b, reach) * 1.05)
        });
        let Some(jump) = jump else {
            self.give_up_leap();
            return;
        };
        leap.stage = Stage::Flight;
        leap.flight = jump.time;
        self.leap = Some(leap);
        self.launch(Vec2::new(leap.dir * jump.forward, -jump.up));
    }

    /// Il renonce à son saut : il reste où il est, d'aplomb.
    fn give_up_leap(&mut self) {
        self.leap = None;
        self.takeoff = None;
        let stand = self.def.default_ground();
        self.enter(stand);
    }

    /// Il se pose au bout d'un saut sur un rebord : ses sabots se plantent
    /// et ses pattes amortissent tout son élan (il « colle » sa réception,
    /// comme la chèvre ou le mouflon sur un rocher étroit) ; il repartira
    /// d'aplomb, d'un antérieur.
    pub(super) fn leap_landed(&mut self) {
        if !self.leap.is_some_and(|leap| leap.stage == Stage::Flight) {
            return;
        }
        self.leap = None;
        self.anim.leap_after = LANDING;
        self.vel.x = 0.0;
        // Il se reçoit sur ses pattes : son élan ne le fait pas basculer en
        // avant (en physique réaliste, le frottement de ses sabots sur le
        // rebord le ferait tourner, comme une caisse qui glisse).
        self.angle = 0.0;
        self.spin = 0.0;
        // Ses pattes, tendues sous lui à la fin du vol, sont posées d'un
        // coup : la pose en l'air et la pose au sol sont alors les mêmes, et
        // les sabots restent là où ils touchent.
        self.anim.air = 0.0;
        self.anim.leap_flight = 0.0;
    }

    /// Où il regarde en mesurant le rebord : vers l'endroit où il se posera
    /// (dans son repère : x vers l'avant, y vers le bas), s'il le mesure.
    pub(super) fn leap_gaze(&self) -> Option<Vec2> {
        let leap = self.leap?;
        if !matches!(self.support, Support::Ground { .. })
            || !matches!(
                leap.stage,
                Stage::Measure(_) | Stage::Approach | Stage::Gather
            )
        {
            return None;
        }
        let p = self.def.physics;
        let head = self.pos + Vec2::new(self.facing * p.width * 0.32, -p.height * 0.72);
        let spot = Vec2::new(
            leap.corner + leap.dir * LAND_IN * p.width,
            self.pos.y - leap.rise,
        );
        let d = (spot - head).normalize_or_zero();
        Some(Vec2::new(d.x * self.facing, d.y))
    }

    /// Il se ramasse pour un saut sur un rebord (et non un saut d'agneau).
    pub(super) fn gathering_for_leap(&self) -> bool {
        self.leap.is_some_and(|leap| leap.stage == Stage::Gather)
    }

    /// En l'air, au cours d'un saut sur un rebord : où il en est de son vol
    /// (0 à l'envol, 1 à la réception).
    pub(super) fn leap_progress(&self) -> Option<f32> {
        self.leap
            .filter(|leap| leap.stage == Stage::Flight)
            .map(|leap| (self.air_time / leap.flight.max(1e-3)).clamp(0.0, 1.0))
    }
}

#[cfg(test)]
mod tests {
    use std::sync::Arc;

    use super::*;
    use crate::format::parse_pet_toml;
    use crate::math::Rect;
    use crate::pet::{Flock, Input, PetEvent, TICK};
    use crate::world::{LedgeOptions, Screen, WindowRect};

    /// Un mouton à la taille du Bleu du Maine (57,5 × 45 points, des pattes
    /// de 17,5 points : il saute sur un rebord de 35 points au plus), qui
    /// saute toujours quand il le peut (`edge` et `window_side`).
    const PET: &str = r#"
        [pet]
        id = "t"
        name = "T"
        [appearance]
        rig = "a.svg"
        height = 45.0
        [physics]
        width = 57.5
        stride = 28.0
        [reflexes]
        spawn = "fall"
        fall = "fall"
        land = "stand"
        thrown = "fall"
        edge = [{ to = "turn", weight = 1 }, { to = "leap", weight = 1000 }]
        wall = "turn"
        window_side = [{ to = "turn", weight = 1 }, { to = "leap", weight = 1000 }]
        [[behavior]]
        name = "stand"
        motion = "stand"
        duration = 0.5
        next = "walk"
        [[behavior]]
        name = "walk"
        motion = "walk"
        speed = 42.0
        duration = 60
        next = "stand"
        [[behavior]]
        name = "turn"
        motion = "turn"
        duration = 0.2
        next = "walk"
        [[behavior]]
        name = "leap"
        motion = "leap"
        reach = 2.0
        duration = "forever"
        on_land = "walk"
        [[behavior]]
        name = "fall"
        motion = "fall"
        duration = "forever"
    "#;

    fn def() -> Arc<PetDef> {
        Arc::new(parse_pet_toml(PET, |_: &str| Ok(b"<svg/>".to_vec())).unwrap())
    }

    /// Deux fenêtres côte à côte : la première (devant) a son rebord à
    /// y = 500, de x = 200 à 600 ; la seconde, derrière, le sien `rise`
    /// points plus haut, à partir de x = 580 + `gap`.
    fn two_windows(rise: f32, gap: f32) -> World {
        let mut world = World::new(vec![Screen {
            frame: Rect::new(0.0, 0.0, 1440.0, 900.0),
            usable: Rect::new(0.0, 25.0, 1440.0, 800.0),
            scale: 2.0,
        }]);
        let (x, y) = if gap > 0.0 {
            (600.0 + gap, 500.0 - rise)
        } else {
            (580.0, 500.0 - rise)
        };
        world.set_windows(
            &[
                WindowRect {
                    id: 1,
                    frame: Rect::new(200.0, 500.0, 400.0, 250.0),
                },
                WindowRect {
                    id: 2,
                    frame: Rect::new(x, y, 400.0, 300.0),
                },
            ],
            LedgeOptions::default(),
        );
        world
    }

    fn ledge(world: &World, window: u64) -> Surface {
        *world
            .surfaces()
            .iter()
            .find(|s| s.id >> 8 == window)
            .unwrap()
    }

    /// Un troupeau d'un mouton posé sur la première fenêtre, qui marche
    /// vers la droite.
    fn walker(def: Arc<PetDef>, world: &World, realistic: bool) -> (Flock, u32) {
        let mut flock = Flock::new();
        flock.set_realistic(realistic);
        let id = flock.spawn(def, 1, world, Some(300.0));
        for _ in 0..(1.5 / TICK) as usize {
            flock.advance(TICK, world, &Input::default());
        }
        let pet = flock.get_mut(id).unwrap();
        assert_eq!(
            pet.support(),
            Support::Ground {
                surface: ledge(world, 1).id
            }
        );
        pet.face(1.0);
        assert!(pet.force("walk"));
        (flock, id)
    }

    fn pet(flock: &Flock, id: u32) -> &Pet {
        flock.pets().iter().find(|p| p.id() == id).unwrap()
    }

    #[test]
    fn a_sheep_leaps_onto_a_higher_window_within_reach() {
        for realistic in [false, true] {
            let world = two_windows(30.0, 0.0);
            let (mut flock, id) = walker(def(), &world, realistic);
            let mut leapt = false;
            for _ in 0..(12.0 / TICK) as usize {
                flock.advance(TICK, &world, &Input::default());
                leapt |= pet(&flock, id).behavior_name() == "leap";
            }
            let p = pet(&flock, id);
            assert!(leapt, "{realistic}");
            assert_eq!(
                p.support(),
                Support::Ground {
                    surface: ledge(&world, 2).id
                },
                "{realistic} : {:?}",
                p.position()
            );
        }
    }

    #[test]
    fn a_window_out_of_reach_turns_it_back() {
        // 40 points : plus haut que ses 35 points.
        let world = two_windows(40.0, 0.0);
        let (mut flock, id) = walker(def(), &world, true);
        let mut turned = false;
        for _ in 0..(12.0 / TICK) as usize {
            flock.advance(TICK, &world, &Input::default());
            let p = pet(&flock, id);
            assert_ne!(p.behavior_name(), "leap");
            assert_eq!(
                p.support(),
                Support::Ground {
                    surface: ledge(&world, 1).id
                }
            );
            turned |= p.facing < 0.0;
        }
        assert!(turned);
    }

    #[test]
    fn it_measures_backs_up_gathers_then_bounds_over_the_corner() {
        let world = two_windows(32.0, 0.0);
        let (mut flock, id) = walker(def(), &world, true);
        let high = ledge(&world, 2);
        let (mut still, mut backed, mut gathered) = (0.0f32, 0.0f32, 0.0f32);
        let mut order = Vec::new();
        let mut landed = None;
        for _ in 0..(12.0 / TICK) as usize {
            flock.advance(TICK, &world, &Input::default());
            let events = flock.get_mut(id).unwrap().take_events();
            let p = pet(&flock, id);
            if p.behavior_name() != "leap" && order.is_empty() {
                continue;
            }
            for e in events {
                if let PetEvent::Landed { impact } = e {
                    // Il se reçoit en douceur (loin d'un atterrissage rude).
                    assert!(impact < 600.0, "{impact}");
                }
            }
            let grounded = matches!(p.support(), Support::Ground { .. });
            let stage = p.leap.map(|l| l.stage);
            if grounded && matches!(stage, Some(Stage::Measure(_))) {
                still += TICK;
                assert!(p.velocity().x.abs() < 1.0, "il mesure, immobile");
                if order.last() != Some(&"mesure") {
                    order.push("mesure");
                }
            } else if grounded && p.velocity().x < -1.0 {
                backed += TICK;
                if order.last() != Some(&"recule") {
                    order.push("recule");
                }
            } else if grounded && matches!(stage, Some(Stage::Gather)) {
                gathered += TICK;
                if order.last() != Some(&"ramasse") {
                    order.push("ramasse");
                }
            } else if !grounded {
                if order.last() != Some(&"vol") {
                    order.push("vol");
                }
                // Au-dessus du coin, le bas du corps passe plus haut que le
                // rebord.
                let body = p.body();
                let front = body.center.x + body.extent_x();
                let bottom = body.center.y + body.extent_y();
                if (front - high.x0).abs() < 4.0 {
                    assert!(bottom < high.y - 2.0, "{bottom} au coin {}", high.y);
                }
            } else if order.last() == Some(&"vol") && landed.is_none() {
                landed = Some(p.position());
            }
        }
        assert_eq!(
            order[..4],
            ["mesure", "recule", "ramasse", "vol"],
            "{order:?}"
        );
        assert!(still > 0.3, "{still}");
        assert!(backed > 0.1, "{backed}");
        assert!((0.12..0.4).contains(&gathered), "{gathered}");
        let landed = landed.expect("posé");
        assert_eq!(landed.y, high.y);
        // Ses quatre sabots sur le rebord, tout près du coin.
        assert!(landed.x - high.x0 > 57.5 * 0.4, "{landed:?}");
        assert!(landed.x - high.x0 < 57.5 * 1.2, "{landed:?}");
    }

    #[test]
    fn the_landing_keeps_its_hooves_planted() {
        let world = two_windows(30.0, 0.0);
        let (mut flock, id) = walker(def(), &world, true);
        let mut was_air = false;
        let mut checked = 0;
        // Où chaque sabot posé touche le sol (abscisse dans le monde).
        let mut planted: [Option<f32>; 4] = [None; 4];
        for _ in 0..(12.0 / TICK) as usize {
            flock.advance(TICK, &world, &Input::default());
            let p = pet(&flock, id);
            let grounded = matches!(p.support(), Support::Ground { .. });
            if !grounded && p.leap.is_some() {
                was_air = true;
                continue;
            }
            if !was_air || !grounded {
                continue;
            }
            if checked == 0 {
                // Posé : ses pattes amortissent tout son élan.
                assert_eq!(p.velocity().x, 0.0);
                assert!(p.anim.leap_after > 0.0);
            }
            let view = p.view(1.0);
            assert!(view.pose.leap > 0.5 || p.anim.leap_after <= 0.0);
            for (k, leg) in view.pose.legs.iter().enumerate() {
                let at = p.position().x + leg.x * 28.0;
                if leg.raise > 0.0 {
                    planted[k] = None;
                } else if let Some(x) = planted[k] {
                    assert!((at - x).abs() < 0.05, "patte {k} : {x} → {at}");
                } else {
                    planted[k] = Some(at);
                }
            }
            checked += 1;
            if p.anim.leap_after <= 0.0 {
                break;
            }
        }
        assert!(checked > 20, "{checked}");
    }

    #[test]
    fn across_a_gap_it_jumps_from_the_end_of_its_ledge() {
        let world = two_windows(20.0, 24.0);
        let (mut flock, id) = walker(def(), &world, true);
        for _ in 0..(12.0 / TICK) as usize {
            flock.advance(TICK, &world, &Input::default());
        }
        assert_eq!(
            pet(&flock, id).support(),
            Support::Ground {
                surface: ledge(&world, 2).id
            }
        );
    }

    #[test]
    fn without_a_ledge_in_reach_the_leap_is_never_chosen() {
        // Rien de plus haut à côté : au bout de sa fenêtre, il fait demi-tour
        // (son saut pèse mille fois plus, mais ne lui est pas proposé).
        let mut world = two_windows(30.0, 0.0);
        world.set_windows(
            &[WindowRect {
                id: 1,
                frame: Rect::new(200.0, 500.0, 400.0, 250.0),
            }],
            LedgeOptions::default(),
        );
        let (mut flock, id) = walker(def(), &world, true);
        let mut turned = 0;
        let mut facing = 1.0;
        for _ in 0..(30.0 / TICK) as usize {
            flock.advance(TICK, &world, &Input::default());
            let p = pet(&flock, id);
            assert_ne!(p.behavior_name(), "leap");
            assert_eq!(
                p.support(),
                Support::Ground {
                    surface: ledge(&world, 1).id
                }
            );
            if p.facing != facing {
                turned += 1;
                facing = p.facing;
            }
        }
        assert!(turned >= 2, "{turned}");
    }

    #[test]
    fn a_mouflon_reaches_higher_than_a_bleu_du_maine() {
        let breed = |name: &str| {
            let path = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
                .join(format!("../../pets/{name}/pet.toml"));
            Arc::new(crate::format::load_pet(&path).unwrap())
        };
        let height = |def: &PetDef| {
            let b = Build::of(def);
            let reach = def
                .behaviors
                .iter()
                .filter_map(|b| match b.motion {
                    Motion::Leap { reach } => Some(reach),
                    _ => None,
                })
                .fold(0.0, f32::max);
            reach * b.leg
        };
        let mouflon = height(&breed("mouflon-corse"));
        let bleu = height(&breed("bleu-du-maine"));
        assert!(mouflon > bleu * 1.5, "{mouflon} {bleu}");
    }

    fn build() -> Build {
        // Le Bleu du Maine : 57,5 × 45,2 points, des pattes de 17,5 points.
        Build {
            width: 57.5,
            height: 45.17,
            leg: 17.5,
            gravity: 2200.0,
            drag: 0.6,
        }
    }

    /// Où est l'avant de son corps (en avant de son point d'envol) et le
    /// bas de son corps (au-dessus de son rebord), au bout de `t` secondes.
    fn at(b: &Build, jump: &Bound, t: f32) -> (f32, f32) {
        let ahead = jump.forward * b.ahead(t) + b.width * 0.5;
        let up = jump.up * t - 0.5 * b.gravity * t * t;
        (ahead, up)
    }

    #[test]
    fn the_bound_clears_the_corner_and_lands_past_it() {
        let b = build();
        for rise in [4.0, 12.0, 20.0, 35.0, 60.0] {
            let jump = bound(&b, rise, None, false).unwrap();
            let corner = jump.gap + b.width * 0.5;
            let mut landed = None;
            let mut t = 0.0;
            while t < 2.0 {
                t += 1.0 / 600.0;
                let (front, up) = at(&b, &jump, t);
                if front >= corner && landed.is_none() {
                    // L'avant passe le coin : le bas du corps est au-dessus.
                    assert!(up >= rise + 2.0, "{rise} : {up}");
                    landed = Some(());
                }
                if landed.is_some() && up <= rise && jump.up - b.gravity * t < 0.0 {
                    // Il se pose : son point d'appui au-delà du coin.
                    let foot = front - b.width * 0.5;
                    let past = foot - corner;
                    assert!((past - LAND_IN * b.width).abs() < 1.5, "{rise} : {past}");
                    break;
                }
            }
            assert!(landed.is_some(), "{rise}");
        }
    }

    #[test]
    fn a_higher_ledge_needs_a_faster_bound_and_a_longer_run_up() {
        let b = build();
        let low = bound(&b, 10.0, None, false).unwrap();
        let high = bound(&b, 35.0, None, false).unwrap();
        assert!(high.speed() > low.speed());
        assert!(high.up > low.up);
        assert!(high.gap > low.gap, "{} {}", high.gap, low.gap);
        // Plus loin du coin, il part de là d'un bond plus long.
        let far = bound(&b, 10.0, Some(low.gap + 20.0), false).unwrap();
        assert!(far.speed() > low.speed());
        assert!((far.gap - low.gap - 20.0).abs() < 1e-3);
    }

    #[test]
    fn a_great_leap_clears_the_corner_and_tops_out_just_above_the_floor_it_reaches() {
        // Le sol d'un écran voisin, 120, 335 ou 612 points plus haut (sa
        // portée : 33 points).
        let b = build();
        for rise in [120.0, 335.0, 612.0] {
            let jump = bound(&b, rise, None, true).unwrap();
            // Son sommet reste à moins d'une hauteur de mouton au-dessus du
            // sol visé ; un saut ordinaire de cette hauteur passerait bien
            // plus haut.
            let over = jump.up * jump.up / (2.0 * b.gravity) - rise;
            assert!(over > 6.0 && over < b.height, "{rise} : {over}");
            let plain = bound(&b, rise, None, false).unwrap();
            assert!(plain.up > jump.up, "{rise}");
            // L'avant de son corps passe le coin quand ses sabots sont
            // au-dessus, et il se pose juste au-delà.
            let corner = jump.gap + b.width * 0.5;
            let (mut crossed, mut landed) = (false, false);
            let mut t = 0.0;
            while t < 3.0 && !landed {
                t += 1.0 / 600.0;
                let (front, up) = at(&b, &jump, t);
                if front >= corner && !crossed {
                    assert!(up >= rise + 2.0, "{rise} : {up}");
                    crossed = true;
                }
                if crossed && up <= rise && jump.up - b.gravity * t < 0.0 {
                    let past = front - b.width * 0.5 - corner;
                    assert!((past - LAND_IN * b.width).abs() < 1.5, "{rise} : {past}");
                    assert!((t - jump.time).abs() < 0.01, "{rise} : {t} {}", jump.time);
                    landed = true;
                }
            }
            assert!(landed, "{rise}");
        }
    }
}
