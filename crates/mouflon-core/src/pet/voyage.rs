//! Voyager d'un écran à l'autre.
//!
//! Quand les sols de deux écrans se rejoignent (au même niveau, ou à une
//! petite marche près), on passe de l'un à l'autre en marchant, sans y
//! penser. Quand ils ne se rejoignent pas, chacun resterait chez soi : un
//! écran plus haut à côté est un mur, celui du dessus est hors d'atteinte.
//! Alors, de temps en temps, l'envie de voyager prend un animal. Il choisit
//! un écran où il peut aller, gagne l'endroit d'où partir, et :
//!
//! - **vers un écran voisin dont le sol est plus haut** : un grand bond
//!   par-dessus le bord (le saut sur un rebord de `pet/leap.rs`, sans la
//!   limite de sa portée) ;
//! - **vers un écran voisin dont le sol est plus bas** : il saute du bout de
//!   son sol, et tombe sur l'autre ;
//! - **vers l'écran du dessus** : un grand bond droit devant lui, qui
//!   traverse le sol de cet écran en montant et s'y pose en retombant (un
//!   sol ne retient que ce qui descend) ;
//! - **vers l'écran du dessous** : un petit saut, et il passe à travers son
//!   sol (et la barre des tâches), jusqu'au sol d'en dessous.
//!
//! Partout, son chemin reste sur les écrans : un passage n'est proposé que
//! si tout son corps y est montré d'un bout à l'autre.
//!
//! Ne voyagent pas : un animal desktopPet (il suit son propre scénario), un
//! agneau qui a sa mère, une brebis qui a son agneau (ils resteraient
//! séparés), ni celui qui dort, joue, tète ou qu'on tient. Sur un bureau de
//! plain-pied (un écran, ou des écrans pareils côte à côte), personne n'a
//! rien à franchir : rien ici ne s'exécute, et aucun tirage n'est fait.

use std::sync::Arc;

use crate::def::Motion;
use crate::layout::TOUCH;
use crate::math::{Rect, Vec2};
use crate::world::{Surface, SurfaceId, SurfaceKind, World};

use super::{Flock, LANDING_REACH, Pet, Support, TAKEOFF_CROUCH};

/// Entre deux envies de voyager d'un même animal (secondes) ; la première
/// vient plus tôt. Dans un grand troupeau, chacun voyage moins souvent
/// (voir [`Flock::voyage_delay`]).
pub const VOYAGE_EVERY: (f32, f32) = (60.0, 240.0);
const FIRST: (f32, f32) = (15.0, 150.0);
/// Après une envie restée sans suite (détourné en chemin, pas de passage
/// pour l'instant) : il y repense dans ce temps-là.
const AGAIN: (f32, f32) = (12.0, 40.0);
/// Occupé au moment où l'envie lui vient (il dort, il joue, on le tient) :
/// elle attend.
const BUSY: f32 = 3.0;
/// Il renonce s'il n'est pas arrivé à son point de départ au bout de ce
/// temps (secondes), ou pas posé ailleurs ce temps-là après être parti.
const WALK_MAX: f32 = 45.0;
const ACT_MAX: f32 = 12.0;
/// Détourné en chemin (une rencontre, un sursaut), il repart une fois sa
/// réaction finie : tant de fois au plus.
const DETOURS: u8 = 4;
/// La marge de son chemin, de chaque côté (points).
const MARGIN: f32 = 4.0;
/// Le petit saut qui le fait quitter son sol : sa hauteur, en part de la
/// sienne.
const HOP: f32 = 0.3;
/// Il a traversé son sol quand il est plus bas que lui de ceci (points).
const PASSED: f32 = 4.0;
/// Vers l'écran du dessus ou du dessous, il part de près de là où il est :
/// à ceci au plus (en longueurs de son corps).
const NEARBY: f32 = 4.0;

/// Par où il passe.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Way {
    /// Un grand bond sur le sol, plus haut, d'un écran voisin.
    Up,
    /// Un saut du bout de son sol vers celui, plus bas, d'un écran voisin.
    Down,
    /// Un grand bond à travers le sol de l'écran du dessus.
    Above,
    /// Un petit saut à travers son propre sol, vers l'écran du dessous.
    Below,
}

#[derive(Clone, Copy, Debug, PartialEq)]
enum Stage {
    /// Il gagne son point de départ.
    Walk,
    /// Il est parti (il se prépare, il est en l'air).
    Go,
}

/// Un voyage vers un autre écran.
#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) struct Voyage {
    pub way: Way,
    /// Le sol qu'il quitte, celui où il va.
    pub from: SurfaceId,
    pub to: SurfaceId,
    /// D'où il part, sur son sol, et dans quel sens.
    pub x: f32,
    pub dir: f32,
    /// Vers l'écran du dessus : le point fixe au-dessus duquel passe son
    /// bond (voir `Leap::through`).
    corner: f32,
    stage: Stage,
    time: f32,
    /// Détourné en chemin : ce qu'il s'est mis à faire, qu'on le laisse
    /// finir ; et combien de fois c'est arrivé.
    detour: Option<usize>,
    detours: u8,
}

/// Comment s'est fini un voyage.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum Outcome {
    /// Posé ailleurs que sur le sol qu'il quittait.
    Arrived,
    /// Resté chez lui.
    GaveUp,
}

impl Pet {
    /// Est-il en voyage vers un autre écran, et par où ?
    #[must_use]
    pub fn voyage(&self) -> Option<Way> {
        self.voyage.map(|v| v.way)
    }

    /// Les passages vers les autres écrans, depuis le sol `floor` qui le
    /// porte. `u`, `v` (de 0 à 1) : le hasard de l'endroit d'où partir,
    /// quand il y a le choix.
    fn voyages(&self, world: &World, floor: &Surface, u: f32, v: f32) -> Vec<Voyage> {
        let p = self.def.physics;
        let (w, h) = (p.width, p.height);
        let half = w * 0.5;
        let step = self.floor_step();
        let can_leap = self.leap_behavior().is_some();
        let mut ways = Vec::new();
        let mut add = |way, to: &Surface, x, dir, corner| {
            ways.push(Voyage {
                way,
                from: floor.id,
                to: to.id,
                x,
                dir,
                corner,
                stage: Stage::Walk,
                time: 0.0,
                detour: None,
                detours: 0,
            });
        };
        for g in world.floors().filter(|g| g.id != floor.id) {
            // (Positif : l'autre sol est plus haut.)
            let rise = floor.y - g.y;
            // Un écran voisin : son sol commence où finit celui-ci.
            for dir in [1.0_f32, -1.0] {
                let (end, start) = if dir > 0.0 {
                    (floor.x1, g.x0)
                } else {
                    (floor.x0, g.x1)
                };
                if (start - end).abs() > TOUCH || rise.abs() < step {
                    // (Ailleurs ; ou une petite marche : il y va en marchant.)
                    continue;
                }
                if rise > 0.0 {
                    if can_leap && let Some(leap) = self.far_leap(world, floor, g, dir, None, None)
                    {
                        add(Way::Up, g, leap.takeoff(w), dir, leap.corner);
                    }
                    continue;
                }
                // Plus bas : au bout de son sol, l'autre écran le montre-t-il
                // en entier, debout à cette hauteur ?
                // (Avec le petit saut qui le fait partir.)
                let top = floor.y - h - HOP * h - 2.0;
                let (left, right) = world.span(end - dir * half, top, floor.y - step);
                let open = if dir > 0.0 {
                    right >= end + w
                } else {
                    left <= end - w
                };
                // Il s'arrête le museau au bord.
                let x = end - dir * (w * 0.32 + MARGIN);
                if open && floor.contains_x(x - dir * half, 0.0) {
                    add(Way::Down, g, x, dir, end);
                }
            }
            // L'écran du dessus, ou du dessous : leurs sols se recouvrent.
            let (a, b) = (floor.x0.max(g.x0), floor.x1.min(g.x1));
            if b - a < w + 2.0 * MARGIN || rise.abs() < h + step {
                continue;
            }
            let near = |lo: f32, hi: f32| {
                let reach = NEARBY * w;
                let (lo, hi) = (
                    lo.max((self.pos.x - reach).min(hi)),
                    hi.min((self.pos.x + reach).max(lo)),
                );
                lo + (hi - lo) * u
            };
            if rise > 0.0 {
                let Some((gap, run)) = self.far_run(rise).filter(|_| can_leap) else {
                    continue;
                };
                // L'envol et la réception tiennent sous l'autre sol.
                let dir = if v < 0.5 { 1.0 } else { -1.0 };
                let (lo, hi) = (a + half + MARGIN, b - half - MARGIN - run);
                if hi < lo {
                    continue;
                }
                let x = if dir > 0.0 {
                    near(lo, hi)
                } else {
                    near(lo + run, hi + run)
                };
                let corner = x + dir * (gap + half);
                let Some(leap) = self.far_leap(world, floor, g, dir, Some(corner), None) else {
                    continue;
                };
                let landing = leap.landing(w);
                let path = Rect::new(
                    x.min(landing) - half - MARGIN,
                    g.y - 2.0 * h,
                    (landing - x).abs() + w + 2.0 * MARGIN,
                    floor.y - (g.y - 2.0 * h),
                );
                if world.holds(path) {
                    add(Way::Above, g, x, dir, corner);
                }
            } else {
                let (lo, hi) = (a + half + MARGIN, b - half - MARGIN);
                if hi < lo {
                    continue;
                }
                let x = near(lo, hi);
                // Le premier sol sous le sien, et rien que des écrans entre.
                let first = world
                    .floor_below(x, floor.y + 4.0)
                    .is_some_and(|below| below.id == g.id);
                let path = Rect::new(
                    x - half - MARGIN,
                    floor.y - h,
                    w + 2.0 * MARGIN,
                    g.y - (floor.y - h),
                );
                if first && world.holds(path) {
                    let dir = if v < 0.5 { 1.0 } else { -1.0 };
                    add(Way::Below, g, x, dir, x);
                }
            }
        }
        ways
    }

    /// Il part en voyage : en route vers son point de départ. `false` s'il
    /// ne sait pas marcher.
    fn begin_voyage(&mut self, voyage: Voyage) -> bool {
        if !self.walk_to(voyage.x) {
            return false;
        }
        self.voyage = Some(voyage);
        true
    }

    /// Il se met en route vers l'abscisse `x` de son sol : au pas, ou au
    /// trot s'il est loin (rien à faire s'il y est déjà). `false` s'il ne
    /// sait pas marcher.
    fn walk_to(&mut self, x: f32) -> bool {
        let def = Arc::clone(&self.def);
        let distance = (x - self.pos.x).abs();
        if distance <= 1.0 {
            return true;
        }
        let movers: [&str; 3] = if distance > def.physics.width * 3.0 {
            ["trot", "walk", "marche"]
        } else {
            ["walk", "marche", "trot"]
        };
        let Some(mover) = movers.into_iter().find_map(|name| def.behavior_index(name)) else {
            return false;
        };
        self.facing = (x - self.pos.x).signum();
        self.enter(mover);
        self.goal = Some(x);
        self.began = None;
        true
    }

    /// Libre de se remettre en route (comme pour partir : voir
    /// `Flock::free_to_play`) ?
    fn free_to_walk(&self) -> bool {
        self.can_react()
            && !self.is_asleep()
            && self.regroup.is_none()
            && self.invite.is_none()
            && !self.family.action.active()
            && matches!(
                self.def.energy(self.behavior),
                crate::Energy::Rest | crate::Energy::Walk
            )
    }

    /// Un pas de son voyage. Renvoie comment il a fini, s'il a fini.
    pub(super) fn step_voyage(&mut self, dt: f32, world: &World) -> Option<Outcome> {
        let mut voyage = self.voyage?;
        voyage.time += dt;
        let on = match self.support {
            Support::Ground { surface } => Some(surface),
            _ => None,
        };
        let end = |pet: &mut Self, outcome| {
            pet.voyage = None;
            pet.passing = None;
            Some(outcome)
        };
        if self.hold.is_some() || self.leaving.is_some() || self.finished {
            return end(self, Outcome::GaveUp);
        }
        match voyage.stage {
            Stage::Walk => {
                if on != Some(voyage.from) || voyage.time > WALK_MAX {
                    return end(self, Outcome::GaveUp);
                }
                let p = self.def.physics;
                let there = (self.pos.x - voyage.x).abs() <= p.width * 0.25 + 2.0;
                if self.goal.is_none() && there {
                    // Arrivé.
                    if !self.leave_floor(world, &voyage) {
                        return end(self, Outcome::GaveUp);
                    }
                    voyage.stage = Stage::Go;
                    voyage.time = 0.0;
                } else if self.goal.is_none() {
                    // Détourné en chemin (une rencontre, un sursaut, le côté
                    // d'une fenêtre) : on le laisse finir ce qu'il s'est mis
                    // à faire, puis il repart.
                    match voyage.detour {
                        None if voyage.detours >= DETOURS => {
                            return end(self, Outcome::GaveUp);
                        }
                        None => {
                            voyage.detour = Some(self.behavior);
                            voyage.detours += 1;
                        }
                        Some(doing) if doing != self.behavior && self.free_to_walk() => {
                            if !self.walk_to(voyage.x) {
                                return end(self, Outcome::GaveUp);
                            }
                            voyage.detour = None;
                        }
                        Some(_) => {}
                    }
                }
            }
            Stage::Go => {
                // Passé sous son sol, il se posera sur le premier venu (et
                // sera ramené tout de suite s'il n'y en a plus : voir
                // `World::floor_missed`).
                if self.passing.is_some()
                    && world
                        .surface(voyage.from)
                        .is_none_or(|from| self.pos.y > from.y + PASSED)
                {
                    self.passing = None;
                }
                let waiting = match voyage.way {
                    Way::Up | Way::Above => self.leap.is_some(),
                    Way::Down | Way::Below => self.takeoff.is_some(),
                };
                match on {
                    Some(surface) if surface != voyage.from => {
                        return end(self, Outcome::Arrived);
                    }
                    // Encore sur son sol : il se prépare ; sinon, il y est
                    // retombé, ou il a renoncé.
                    Some(_) if !waiting => return end(self, Outcome::GaveUp),
                    _ if voyage.time > ACT_MAX => return end(self, Outcome::GaveUp),
                    _ => {}
                }
            }
        }
        self.voyage = Some(voyage);
        None
    }

    /// Arrivé à son point de départ, il quitte son sol. `false` si le
    /// passage n'est plus là (les écrans ont changé).
    fn leave_floor(&mut self, world: &World, voyage: &Voyage) -> bool {
        let def = Arc::clone(&self.def);
        let p = def.physics;
        let (Some(from), Some(to)) = (
            world.surface(voyage.from).copied(),
            world.surface(voyage.to).copied(),
        ) else {
            return false;
        };
        if from.kind != SurfaceKind::Floor || to.kind != SurfaceKind::Floor {
            return false;
        }
        match voyage.way {
            Way::Up | Way::Above => {
                let corner = (voyage.way == Way::Above).then_some(voyage.corner);
                let (Some(leaping), Some(leap)) = (
                    self.leap_behavior(),
                    self.far_leap(world, &from, &to, voyage.dir, corner, Some(self.pos.x)),
                ) else {
                    return false;
                };
                // Le saut qu'on lui propose : `enter` le lui fait commencer.
                self.facing = voyage.dir;
                self.leap_asked = false;
                self.leap_offer = Some(leap);
                self.enter(leaping);
                self.leap.is_some()
            }
            Way::Down | Way::Below => {
                if voyage.way == Way::Below {
                    // Toujours un sol sous le sien, et des écrans entre eux.
                    let path = Rect::new(
                        self.pos.x - p.width * 0.5 - MARGIN,
                        from.y - p.height,
                        p.width + 2.0 * MARGIN,
                        to.y - (from.y - p.height),
                    );
                    let first = world
                        .floor_below(self.pos.x, from.y + 4.0)
                        .is_some_and(|below| below.id == to.id);
                    if !first || to.y <= from.y || !world.holds(path) {
                        return false;
                    }
                }
                let gravity = p.gravity.max(1.0);
                let up = (2.0 * gravity * HOP * p.height).sqrt();
                let forward = if voyage.way == Way::Down {
                    // De quoi retomber au-delà du bout de son sol.
                    let end = if voyage.dir > 0.0 { from.x1 } else { from.x0 };
                    let clear = (end - self.pos.x) * voyage.dir + p.width * LANDING_REACH + MARGIN;
                    1.3 * clear.max(0.0) * gravity / (2.0 * up)
                } else {
                    p.width * 0.4
                };
                let vel = Vec2::new(voyage.dir * forward, -up);
                self.facing = voyage.dir;
                let hop = def
                    .behavior_index("hop_down")
                    .filter(|&i| matches!(def.behaviors[i].motion, Motion::Jump { .. }))
                    .or_else(|| {
                        def.behaviors
                            .iter()
                            .position(|b| matches!(b.motion, Motion::Jump { .. }) && !b.spin)
                    });
                if let Some(hop) = hop {
                    self.enter(hop);
                }
                if matches!(self.support, Support::Ground { .. }) {
                    if hop.is_some() && self.takeoff.is_some() {
                        // Il se ramasse d'abord, puis saute de l'élan voulu.
                        self.takeoff = Some((TAKEOFF_CROUCH, vel));
                    } else {
                        self.launch(vel);
                        if let Some(air) = def.default_air() {
                            self.enter(air);
                        }
                    }
                } else {
                    // (Un animal en images est déjà parti.)
                    self.vel = vel;
                }
                if voyage.way == Way::Below {
                    self.passing = Some(voyage.from);
                }
                true
            }
        }
    }
}

impl Flock {
    /// Fait partir cet animal en voyage vers un autre écran, tout de suite
    /// (un essai, un aperçu) : vers celui où se trouve `toward` s'il est
    /// donné, sinon vers l'un de ceux où il peut aller. `false` s'il n'est
    /// pas libre de partir, ou s'il n'y a pas de passage.
    pub fn voyage(&mut self, id: u32, world: &World, toward: Option<Vec2>) -> bool {
        let Some(index) = self.pets.iter().position(|p| p.id == id) else {
            return false;
        };
        self.start_voyage(index, world, toward)
    }

    /// Les voyages depuis le début : combien d'animaux sont partis vers un
    /// autre écran, combien y sont arrivés (les autres ont renoncé, ou sont
    /// encore en route).
    #[must_use]
    pub fn voyages(&self) -> (u32, u32) {
        self.voyages
    }

    /// Change le temps entre deux envies de voyager d'un animal (secondes) :
    /// pour un essai, où l'on veut les voir passer d'un écran à l'autre sans
    /// attendre. `None` : comme d'habitude ([`VOYAGE_EVERY`]).
    pub fn set_voyage_every(&mut self, every: Option<(f32, f32)>) {
        self.voyage_every = every.map(|(a, b)| (a.max(0.5), b.max(a.max(0.5))));
        for pet in &mut self.pets {
            pet.voyage_in = 0.0;
        }
    }

    /// Un tirage de 0 à 1 (splitmix64 : sans toucher aux tirages des
    /// animaux, qui vivent donc de même qu'ils voyagent ou non).
    fn voyage_draw(&mut self) -> f32 {
        self.voyage_seed = self.voyage_seed.wrapping_add(0x9E37_79B9_7F4A_7C15);
        let mut z = self.voyage_seed;
        z = (z ^ (z >> 30)).wrapping_mul(0xBF58_476D_1CE4_E5B9);
        z = (z ^ (z >> 27)).wrapping_mul(0x94D0_49BB_1331_11EB);
        z ^= z >> 31;
        (z >> 40) as f32 / (1u64 << 24) as f32
    }

    /// Le temps jusqu'à la prochaine envie, tiré dans `range`. Les envies
    /// ordinaires s'espacent dans un grand troupeau : au-delà de six
    /// animaux, on en verrait sauter sans arrêt.
    fn voyage_delay(&mut self, range: (f32, f32), ordinary: bool) -> f32 {
        let (range, crowd) = match (ordinary, self.voyage_every) {
            (true, Some(every)) => (every, 1.0),
            (true, None) => (range, (self.pets.len() as f32 / 6.0).clamp(1.0, 5.0)),
            (false, Some(every)) => ((every.0.min(range.0), every.1.min(range.1)), 1.0),
            (false, None) => (range, 1.0),
        };
        (range.0 + (range.1 - range.0) * self.voyage_draw()) * crowd
    }

    /// Cet animal est-il libre de partir en voyage ?
    fn free_to_voyage(&self, index: usize, world: &World) -> Option<Surface> {
        let pet = &self.pets[index];
        let Support::Ground { surface } = pet.support else {
            return None;
        };
        let floor = world.surface(surface).copied()?;
        (floor.kind == SurfaceKind::Floor
            && self.free_to_play(index)
            && pet.voyage.is_none()
            && pet.mother().is_none()
            && !self.families.is_mother(pet.id))
        .then_some(floor)
    }

    fn start_voyage(&mut self, index: usize, world: &World, toward: Option<Vec2>) -> bool {
        if world.is_plain() {
            return false;
        }
        let Some(floor) = self.free_to_voyage(index, world) else {
            return false;
        };
        let (u, v, pick) = (self.voyage_draw(), self.voyage_draw(), self.voyage_draw());
        let mut ways = self.pets[index].voyages(world, &floor, u, v);
        if let Some(point) = toward {
            // Le sol de l'écran où se trouve ce point.
            let wanted = world
                .screen_at(point)
                .and_then(|s| world.floor_at(s.usable.x + s.usable.w * 0.5, s.usable.bottom()));
            ways.retain(|way| wanted.is_some_and(|floor| floor.id == way.to));
        }
        if ways.is_empty() {
            return false;
        }
        let way = ways[((pick * ways.len() as f32) as usize).min(ways.len() - 1)];
        let began = self.pets[index].begin_voyage(way);
        self.voyages_live |= began;
        self.voyages.0 += u32::from(began);
        began
    }

    /// Les envies de voyager, et les voyages en cours (voir le module).
    pub(super) fn step_voyages(&mut self, dt: f32, world: &World) {
        if world.is_plain() {
            // Rien à franchir. (Les écrans viennent de changer : ceux qui
            // étaient en route n'ont plus où aller.)
            if self.voyages_live {
                for pet in &mut self.pets {
                    pet.voyage = None;
                    pet.passing = None;
                }
                self.voyages_live = false;
            }
            return;
        }
        for index in 0..self.pets.len() {
            if self.is_still(index) {
                continue;
            }
            if self.pets[index].voyage.is_some() {
                if let Some(outcome) = self.pets[index].step_voyage(dt, world) {
                    self.voyages.1 += u32::from(outcome == Outcome::Arrived);
                    self.pets[index].voyage_in = match outcome {
                        Outcome::Arrived => self.voyage_delay(VOYAGE_EVERY, true),
                        Outcome::GaveUp => self.voyage_delay(AGAIN, false),
                    };
                }
                continue;
            }
            if self.pets[index].voyage_in <= 0.0 {
                // Sa première envie.
                self.pets[index].voyage_in = self.voyage_delay(FIRST, true);
                continue;
            }
            self.pets[index].voyage_in -= dt;
            if self.pets[index].voyage_in > 0.0 {
                continue;
            }
            self.pets[index].voyage_in = if self.free_to_voyage(index, world).is_none() {
                BUSY
            } else if self.start_voyage(index, world, None) {
                // (Tiré de nouveau à la fin du voyage.)
                BUSY
            } else {
                self.voyage_delay(AGAIN, false)
            };
        }
    }
}
