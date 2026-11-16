//! Physique réaliste : les animaux sont des corps rigides.
//!
//! Avec la physique « classique », celle d'eSheep et de desktopPet, un animal
//! colle à la fenêtre qui le porte et tombe sans tourner. Ici, chacun est un
//! rectangle qui a une masse, une vitesse, un angle et une vitesse de
//! rotation :
//!
//! - les fenêtres sont des plateformes qui bougent ([`Tracker`]) : elles
//!   emportent l'animal tant que ses pattes adhèrent ([`GRIP`]), le laissent
//!   tomber si elles descendent plus vite que la pesanteur, le lancent en
//!   l'air si elles montent puis s'arrêtent net ;
//! - tenu, il pend sous le point par lequel on l'a pris et se balance comme
//!   un pendule ; lâché, il garde son élan et sa rotation ;
//! - en l'air, il tourne sur lui-même, rebondit sur les bords des écrans et
//!   les côtés des fenêtres, heurte les autres animaux (ils échangent leur
//!   élan, [`obb_contact`], [`collide_pair`]) et peut se poser sur leur dos ;
//! - au sol, il se remet sur ses pattes ([`upright_target`]).
//!
//! Les chocs se règlent par impulsions (vitesse normale et frottement de
//! Coulomb au point de contact) : un animal qui touche le sol d'un coin se
//! met à tourner. Tout reste déterministe, au pas fixe de la simulation.
//!
//! Ce module ne contient que la mécanique ; [`crate::pet`] s'en sert quand
//! le troupeau est en physique réaliste ([`crate::Flock::set_realistic`]).

use std::f32::consts::TAU;

use crate::math::{Vec2, clamp_between, lerp};
use crate::world::{Surface, SurfaceId, SurfaceKind, World};

/// Identifiants des « surfaces » que sont les dos des animaux (un bit que
/// n'ont ni les rebords de fenêtres ni les sols).
pub const PET_SURFACE: u64 = 1 << 62;

/// La surface que forme le dos de cet animal.
#[must_use]
pub fn pet_surface(id: u32) -> SurfaceId {
    PET_SURFACE | u64::from(id)
}

/// L'animal dont cette surface est le dos, s'il y en a un.
#[must_use]
pub fn surface_pet(surface: SurfaceId) -> Option<u32> {
    (surface & PET_SURFACE != 0 && surface & (1 << 63) == 0).then_some(surface as u32)
}

/// Adhérence des pattes : la plateforme qui porte un animal peut changer sa
/// vitesse d'au plus cette fraction de la pesanteur (un coefficient de
/// frottement). Au-delà, il glisse : une fenêtre secouée d'un coup sec le
/// laisse en arrière.
pub const GRIP: f32 = 1.8;

/// Rebond sur les bords des écrans et les côtés des fenêtres.
pub const WALL_RESTITUTION: f32 = 0.42;
/// Frottement contre un mur, le sol, un autre animal.
pub const WALL_FRICTION: f32 = 0.35;
pub const GROUND_FRICTION: f32 = 0.6;
pub const PET_RESTITUTION: f32 = 0.35;
pub const PET_FRICTION: f32 = 0.3;

/// Freinage de la rotation dans les airs (1/s).
pub const SPIN_DRAG: f32 = 0.35;
/// Rotation la plus vive (tours par seconde) : un lancer reste lisible.
pub const MAX_SPIN: f32 = 3.0 * TAU;

/// Un animal tenu se raidit : rappel vers la verticale (1/s²) et freinage
/// du balancement (1/s). Sans eux, pris par une patte, il pendrait la tête
/// en bas et se balancerait sans fin.
pub const HOLD_STIFFNESS: f32 = 16.0;
pub const HOLD_DAMPING: f32 = 2.6;

/// Au sol, il se remet sur ses pattes en à peu près ce temps (secondes).
pub const RIGHTING_TIME: f32 = 0.32;

/// Lissage de la position montrée d'une fenêtre (secondes) et part d'un
/// intervalle entre deux relevés dont on l'anticipe (un relevé qui la
/// trouve immobile arrête aussitôt l'anticipation).
const WINDOW_SMOOTHING: f32 = 0.03;
const WINDOW_LEAD: f32 = 1.0;
/// Sans relevé depuis ce nombre d'intervalles, la fenêtre s'est arrêtée.
const WINDOW_STOPPED: f64 = 1.6;
/// Une fenêtre qui saute plus loin que ceci d'un relevé à l'autre a été
/// replacée (agrandie, envoyée sur un autre écran…) : elle n'a pas d'élan.
const WINDOW_JUMP: f32 = 600.0;
/// Écart entre la fenêtre relevée et sa position lissée au-delà duquel on
/// ne lisse plus (points, plus ce que sa vitesse explique) : lisser un grand
/// écart lui donnerait une vitesse qu'elle n'a pas eue, et lancerait les
/// animaux qu'elle porte.
const WINDOW_RESIDUAL: f32 = 24.0;

/// Un corps au moment d'un choc : vitesse de son centre, vitesse de
/// rotation (radians par seconde, sens horaire), inverses de sa masse et de
/// son moment d'inertie (zéro : immobile, comme une fenêtre ou une main).
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct Rigid {
    pub vel: Vec2,
    pub spin: f32,
    pub inv_mass: f32,
    pub inv_inertia: f32,
}

impl Rigid {
    /// Un corps rectangulaire de masse unité, de cette largeur et hauteur.
    #[must_use]
    pub fn boxed(vel: Vec2, spin: f32, width: f32, height: f32) -> Self {
        Self {
            vel,
            spin,
            inv_mass: 1.0,
            inv_inertia: 1.0 / box_inertia(1.0, width, height),
        }
    }

    /// Un corps qu'aucun choc ne fait dévier (une main, une fenêtre).
    #[must_use]
    pub fn fixed(vel: Vec2) -> Self {
        Self {
            vel,
            ..Self::default()
        }
    }

    /// Vitesse du point à `r` du centre.
    #[must_use]
    pub fn point_velocity(&self, r: Vec2) -> Vec2 {
        self.vel + r.perp() * self.spin
    }

    /// Applique l'impulsion `impulse` au point à `r` du centre.
    pub fn apply(&mut self, r: Vec2, impulse: Vec2) {
        self.vel += impulse * self.inv_mass;
        self.spin += r.cross(impulse) * self.inv_inertia;
    }

    /// Résistance d'un point à une impulsion dans la direction `dir`.
    fn mass_along(&self, r: Vec2, dir: Vec2) -> f32 {
        let rn = r.cross(dir);
        self.inv_mass + rn * rn * self.inv_inertia
    }
}

/// Moment d'inertie d'un rectangle plein autour de son centre.
#[must_use]
pub fn box_inertia(mass: f32, width: f32, height: f32) -> f32 {
    mass * (width * width + height * height) / 12.0
}

/// Choc d'un corps contre un obstacle qui va à `obstacle` (pt/s), au point à
/// `r` de son centre, de normale `n` (unitaire, tournée vers le corps) :
/// rebond `restitution` et frottement `friction`. Renvoie la vitesse à
/// laquelle le point allait vers l'obstacle (0 s'il s'en éloignait : rien
/// n'est changé).
pub fn collide_static(
    body: &mut Rigid,
    r: Vec2,
    n: Vec2,
    obstacle: Vec2,
    restitution: f32,
    friction: f32,
) -> f32 {
    let vn = (body.point_velocity(r) - obstacle).dot(n);
    if vn >= 0.0 {
        return 0.0;
    }
    let jn = -(1.0 + restitution) * vn / body.mass_along(r, n);
    body.apply(r, n * jn);
    let t = n.perp();
    let vt = (body.point_velocity(r) - obstacle).dot(t);
    let jt = clamp_between(-vt / body.mass_along(r, t), -friction * jn, friction * jn);
    body.apply(r, t * jt);
    -vn
}

/// Choc entre deux corps au point `point`, de normale `n` (unitaire, de `a`
/// vers `b`). Renvoie la vitesse à laquelle ils se rapprochaient (0 s'ils
/// s'éloignaient : rien n'est changé).
#[allow(clippy::too_many_arguments)]
pub fn collide_pair(
    a: &mut Rigid,
    a_center: Vec2,
    b: &mut Rigid,
    b_center: Vec2,
    point: Vec2,
    n: Vec2,
    restitution: f32,
    friction: f32,
) -> f32 {
    let (ra, rb) = (point - a_center, point - b_center);
    let closing = (a.point_velocity(ra) - b.point_velocity(rb)).dot(n);
    if closing <= 0.0 {
        return 0.0;
    }
    let k = a.mass_along(ra, n) + b.mass_along(rb, n);
    if k <= f32::EPSILON {
        return 0.0;
    }
    let jn = (1.0 + restitution) * closing / k;
    a.apply(ra, n * -jn);
    b.apply(rb, n * jn);
    let t = n.perp();
    let slip = (a.point_velocity(ra) - b.point_velocity(rb)).dot(t);
    let kt = a.mass_along(ra, t) + b.mass_along(rb, t);
    if kt > f32::EPSILON {
        let jt = clamp_between(slip / kt, -friction * jn, friction * jn);
        a.apply(ra, t * -jt);
        b.apply(rb, t * jt);
    }
    closing
}

/// Un rectangle qui tourne (le corps d'un animal) : centre, demi-largeur et
/// demi-hauteur, angle (radians, sens horaire de l'écran).
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Obb {
    pub center: Vec2,
    pub half: Vec2,
    pub angle: f32,
}

impl Obb {
    /// Ses axes : un seul sinus pour les deux, aucun debout (le plus
    /// souvent, au sol).
    pub(crate) fn axes(&self) -> [Vec2; 2] {
        if self.angle == 0.0 {
            return [Vec2::new(1.0, 0.0), Vec2::new(0.0, 1.0)];
        }
        let (s, c) = self.angle.sin_cos();
        [Vec2::new(c, s), Vec2::new(-s, c)]
    }

    #[must_use]
    pub fn corners(&self) -> [Vec2; 4] {
        self.corners_with(self.axes())
    }

    fn corners_with(&self, [ax, ay]: [Vec2; 2]) -> [Vec2; 4] {
        let (x, y) = (ax * self.half.x, ay * self.half.y);
        let c = self.center;
        [c - x - y, c + x - y, c + x + y, c - x + y]
    }

    /// Distance du centre au point le plus bas (et le plus à droite) :
    /// demi-hauteur et demi-largeur du rectangle tourné.
    #[must_use]
    pub fn extent_y(&self) -> f32 {
        let (s, c) = self.angle.sin_cos();
        self.half.x * s.abs() + self.half.y * c.abs()
    }

    #[must_use]
    pub fn extent_x(&self) -> f32 {
        let (s, c) = self.angle.sin_cos();
        self.half.x * c.abs() + self.half.y * s.abs()
    }

    /// Le point le plus bas : un coin, ou le milieu du bas quand le rectangle
    /// est (presque) droit.
    #[must_use]
    pub fn lowest_point(&self) -> Vec2 {
        self.support(Vec2::new(0.0, 1.0))
    }

    /// Le même, ses axes déjà connus.
    pub(crate) fn lowest_point_with(&self, axes: [Vec2; 2]) -> Vec2 {
        self.support_with(axes, Vec2::new(0.0, 1.0))
    }

    /// Le point le plus avancé dans la direction `dir` : un coin, ou le
    /// milieu d'un côté qui lui fait face.
    #[must_use]
    pub fn support(&self, dir: Vec2) -> Vec2 {
        self.support_with(self.axes(), dir)
    }

    fn support_with(&self, axes: [Vec2; 2], dir: Vec2) -> Vec2 {
        let corners = self.corners_with(axes);
        let best = corners
            .iter()
            .map(|c| c.dot(dir))
            .fold(f32::NEG_INFINITY, f32::max);
        // La moyenne des coins presque aussi avancés (sans rien allouer).
        let (mut sum, mut count) = (Vec2::ZERO, 0u32);
        for c in corners.iter().filter(|c| best - c.dot(dir) < 0.5) {
            sum += *c;
            count += 1;
        }
        sum / count.max(1) as f32
    }

    /// Ses côtés projetés sur `axis`, ses axes déjà connus.
    fn project_with(&self, [ax, ay]: [Vec2; 2], axis: Vec2) -> (f32, f32) {
        let r = self.half.x * ax.dot(axis).abs() + self.half.y * ay.dot(axis).abs();
        let c = self.center.dot(axis);
        (c - r, c + r)
    }

    /// Carré du rayon du cercle qui le contient, autour de son centre.
    fn radius_squared(&self) -> f32 {
        self.half.dot(self.half)
    }
}

/// Deux rectangles qui se chevauchent : la normale (de `a` vers `b`), la
/// profondeur, et le point de contact.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Contact {
    pub normal: Vec2,
    pub depth: f32,
    pub point: Vec2,
}

/// Les deux rectangles se touchent-ils ? Séparation des axes : on cherche
/// l'axe (un côté de l'un ou de l'autre) où ils se chevauchent le moins.
#[must_use]
pub fn obb_contact(a: &Obb, b: &Obb) -> Option<Contact> {
    // Leurs cercles ne se touchent pas : leurs rectangles non plus (sans
    // calculer leurs axes).
    if apart_circles(a, b) {
        return None;
    }
    separating_axes(a, a.axes(), b, b.axes())
}

/// Leurs cercles ne se touchent pas. (ra + rb)² = ra² + rb² + 2 ra rb,
/// avec une seule racine ; un peu de marge : jamais écarté à tort par un
/// arrondi.
fn apart_circles(a: &Obb, b: &Obb) -> bool {
    let (ra2, rb2) = (a.radius_squared(), b.radius_squared());
    let reach = ra2 + rb2 + 2.0 * (ra2 * rb2).sqrt() + 0.1;
    let apart = b.center - a.center;
    apart.dot(apart) > reach
}

/// [`obb_contact`], leurs axes ([`Obb::axes`]) déjà connus : dans une
/// foule, chaque corps en rencontre beaucoup d'autres au même pas.
#[must_use]
pub(crate) fn obb_contact_with(
    a: &Obb,
    axes_a: [Vec2; 2],
    b: &Obb,
    axes_b: [Vec2; 2],
) -> Option<Contact> {
    if apart_circles(a, b) {
        return None;
    }
    separating_axes(a, axes_a, b, axes_b)
}

/// De combien ils s'enfoncent l'un dans l'autre (séparation des axes), sans
/// chercher le point de contact ; leurs axes déjà connus.
pub(crate) fn obb_depth_with(
    a: &Obb,
    axes_a: [Vec2; 2],
    b: &Obb,
    axes_b: [Vec2; 2],
) -> Option<f32> {
    if apart_circles(a, b) {
        return None;
    }
    let mut depth = f32::INFINITY;
    for axis in axes_a.into_iter().chain(axes_b) {
        let (a0, a1) = a.project_with(axes_a, axis);
        let (b0, b1) = b.project_with(axes_b, axis);
        let overlap = a1.min(b1) - a0.max(b0);
        if overlap <= 0.0 {
            return None;
        }
        depth = depth.min(overlap);
    }
    Some(depth)
}

/// La séparation des axes elle-même.
fn separating_axes(a: &Obb, axes_a: [Vec2; 2], b: &Obb, axes_b: [Vec2; 2]) -> Option<Contact> {
    let mut best: Option<(f32, Vec2)> = None;
    for axis in axes_a.into_iter().chain(axes_b) {
        let (a0, a1) = a.project_with(axes_a, axis);
        let (b0, b1) = b.project_with(axes_b, axis);
        let overlap = a1.min(b1) - a0.max(b0);
        if overlap <= 0.0 {
            return None;
        }
        if best.is_none_or(|(depth, _)| overlap < depth) {
            best = Some((overlap, axis));
        }
    }
    let (depth, mut normal) = best?;
    if (b.center - a.center).dot(normal) < 0.0 {
        normal = -normal;
    }
    // Entre le point de `a` le plus avancé vers `b` et celui de `b` le plus
    // avancé vers `a`.
    let point = (a.support_with(axes_a, normal) + b.support_with(axes_b, -normal)) * 0.5;
    Some(Contact {
        normal,
        depth,
        point,
    })
}

/// L'angle « debout » vers lequel un animal se rétablit au sol : le tour
/// complet le plus proche, ou, s'il roule encore vite, le suivant dans le
/// sens où il roule (il finit sa culbute).
#[must_use]
pub fn upright_target(angle: f32, spin: f32) -> f32 {
    let turns = angle / TAU;
    let k = if spin > 5.0 {
        turns.ceil()
    } else if spin < -5.0 {
        turns.floor()
    } else {
        turns.round()
    };
    k * TAU
}

/// Un pas du rétablissement au sol : ressort amorti critique vers
/// [`upright_target`]. Renvoie l'angle et la vitesse de rotation ; zéro et
/// zéro une fois debout.
#[must_use]
pub fn righting_step(angle: f32, spin: f32, dt: f32) -> (f32, f32) {
    let target = upright_target(angle, spin);
    let w = TAU / RIGHTING_TIME * 0.5;
    let accel = -w * w * (angle - target) - 2.0 * w * spin;
    let spin = spin + accel * dt;
    let angle = angle + spin * dt;
    if (angle - target).abs() < 0.002 && spin.abs() < 0.05 {
        (0.0, 0.0)
    } else {
        (angle, spin)
    }
}

/// Le dos d'un animal, sur lequel un autre peut se poser, et sa vitesse.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Back {
    pub surface: Surface,
    pub vel: Vec2,
}

/// Suivi des fenêtres pour la physique réaliste.
///
/// Les fenêtres des autres apps ne sont connues que par des relevés, toutes
/// les 33 à 250 ms ; entre deux, elles bougent à l'écran. Le suivi estime la
/// vitesse de chacune d'après ses derniers relevés, anticipe un peu où elle
/// est (au plus une demi-période), et lisse sa position : un animal posé
/// dessus est emporté en douceur, avec la bonne vitesse, au lieu de sauter
/// d'un relevé à l'autre. Il tient aussi le « monde lissé » que voient les
/// animaux : les rebords et les côtés des fenêtres à leur position lissée,
/// et les dos des animaux comme autant de rebords.
#[derive(Clone, Debug, Default)]
pub struct Tracker {
    time: f64,
    /// Relevés des fenêtres vus (voir [`World::polls`]).
    polls: u64,
    windows: Vec<Tracked>,
    world: World,
    /// Vitesse des dos des animaux (les fenêtres ont la leur, les sols aucune).
    backs: crate::world::IdMap<Vec2>,
    /// La vitesse et le saut de chaque surface du monde lissé, par rang.
    ranked: Vec<(Vec2, Vec2)>,
}

#[derive(Clone, Copy, Debug)]
struct Tracked {
    id: u64,
    /// Dernière position relevée (coin haut gauche), sa largeur, et quand.
    known: Vec2,
    width: f32,
    known_at: f64,
    /// Intervalle habituel entre deux relevés qui changent.
    interval: f64,
    /// Vitesse estimée d'après les relevés.
    estimate: Vec2,
    /// Position lissée, et sa vitesse.
    shown: Vec2,
    vel: Vec2,
    /// Déplacée d'un coup à ce pas (voir [`Tracker::jumped`]).
    jumped: Vec2,
    seen: bool,
}

impl Tracker {
    /// Le monde tel que la physique le voit (voir [`Tracker`]).
    #[must_use]
    pub fn world(&self) -> &World {
        &self.world
    }

    /// Vitesse d'une surface (pt/s) : celle de sa fenêtre, du dos qu'elle
    /// est, ou rien pour un sol.
    #[must_use]
    pub fn velocity(&self, surface: SurfaceId) -> Vec2 {
        if let Some(&vel) = self.backs.get(&surface) {
            return vel;
        }
        match self.world.surface(surface) {
            Some(s) if s.kind == SurfaceKind::Window => self.window_velocity(s.id >> 8),
            _ => Vec2::ZERO,
        }
    }

    /// Déplacement d'un coup de la fenêtre de cette surface à ce pas :
    /// replacée (un autre écran, une autre taille), ou vue bouger pour la
    /// première fois après un calme, sans savoir comment elle est partie.
    /// Les animaux posés dessus la suivent tel quel.
    #[must_use]
    pub fn jumped(&self, surface: SurfaceId) -> Vec2 {
        match self.world.surface(surface) {
            Some(s) if s.kind == SurfaceKind::Window => self
                .windows
                .iter()
                .find(|t| t.id == s.id >> 8)
                .map_or(Vec2::ZERO, |t| t.jumped),
            _ => Vec2::ZERO,
        }
    }

    /// Vitesse d'une fenêtre (son identifiant), connue ou pas.
    #[must_use]
    pub fn window_velocity(&self, window: u64) -> Vec2 {
        self.windows
            .iter()
            .find(|t| t.id == window)
            .map_or(Vec2::ZERO, |t| t.vel)
    }

    /// Avance de `dt` : prend en compte le dernier relevé de `world`, lisse
    /// la position des fenêtres et refait le monde lissé, avec ces dos.
    pub fn update(&mut self, world: &World, dt: f32, backs: &[Back]) {
        self.time += f64::from(dt);
        let time = self.time;
        let polled = world.polls() != self.polls;
        self.polls = world.polls();
        for tracked in &mut self.windows {
            tracked.seen = false;
        }
        for window in world.windows() {
            let at = Vec2::new(window.frame.x, window.frame.y);
            let Some(tracked) = self.windows.iter_mut().find(|t| t.id == window.id) else {
                self.windows.push(Tracked {
                    id: window.id,
                    known: at,
                    width: window.frame.w,
                    known_at: time,
                    interval: 1.0 / 30.0,
                    estimate: Vec2::ZERO,
                    shown: at,
                    vel: Vec2::ZERO,
                    jumped: Vec2::ZERO,
                    seen: true,
                });
                continue;
            };
            tracked.seen = true;
            tracked.jumped = Vec2::ZERO;
            if window.frame.w != tracked.width {
                // Redimensionnée : le bord gauche bouge seul, le rebord ne se
                // déplace pas sous les animaux (seule sa hauteur compte).
                tracked.width = window.frame.w;
                tracked.estimate.x = 0.0;
                tracked.shown.x = at.x;
                tracked.vel.x = 0.0;
                tracked.known.x = at.x;
            }
            if at == tracked.known {
                // Un relevé tout frais la trouve à la même place : elle s'est arrêtée.
                if polled {
                    tracked.estimate = Vec2::ZERO;
                }
                continue;
            }
            let since = (time - tracked.known_at).max(1e-3);
            let jump = (at - tracked.known).length();
            if jump > WINDOW_JUMP || since > 0.5 {
                // Replacée d'un coup, ou un premier relevé après un long calme
                // (on ne sait pas comment elle est partie) : pas d'élan à en
                // tirer. Elle est aussitôt à sa place, et ce qu'elle porte la
                // suit tel quel, sans glisser.
                tracked.estimate = Vec2::ZERO;
                tracked.jumped = at - tracked.shown;
                tracked.shown = at;
                tracked.vel = Vec2::ZERO;
            } else {
                let measured = (at - tracked.known) * (1.0 / since as f32);
                tracked.estimate = tracked.estimate.lerp(measured, 0.7);
                tracked.interval = (lerp(tracked.interval as f32, since as f32, 0.5) as f64)
                    .clamp(1.0 / 120.0, 0.25);
                // Un relevé en retard la montre bien plus loin que ce qu'on
                // montrait : elle y est aussitôt, et ce qu'elle porte la suit.
                let residual = at - tracked.shown;
                let explained = WINDOW_RESIDUAL
                    + tracked.estimate.length() * (WINDOW_SMOOTHING + tracked.interval as f32);
                if residual.length() > explained {
                    tracked.jumped = residual;
                    tracked.shown = at;
                }
            }
            tracked.known = at;
            tracked.known_at = time;
        }
        self.windows.retain(|t| t.seen);
        let follow = 1.0 - (-dt / WINDOW_SMOOTHING).exp();
        for tracked in &mut self.windows {
            let since = time - tracked.known_at;
            if since > tracked.interval * WINDOW_STOPPED {
                tracked.estimate = Vec2::ZERO;
            }
            let ahead = since.min(tracked.interval * f64::from(WINDOW_LEAD)) as f32;
            let target = tracked.known + tracked.estimate * ahead;
            let before = tracked.shown;
            tracked.shown = before.lerp(target, follow);
            tracked.vel = (tracked.shown - before) * (1.0 / dt.max(1e-4));
        }

        self.world.copy_from(world);
        let windows = &self.windows;
        self.world.shift_windows(|id| {
            windows
                .iter()
                .find(|t| t.id == id)
                .map(|t| t.shown - t.known)
        });
        // La vitesse et le saut de chaque surface, par rang : ceux de sa
        // fenêtre, ou rien pour un sol ; puis les dos, avec leur vitesse.
        self.ranked.clear();
        for s in self.world.surfaces() {
            self.ranked.push(match s.kind {
                SurfaceKind::Window => self
                    .windows
                    .iter()
                    .find(|t| t.id == s.id >> 8)
                    .map_or((Vec2::ZERO, Vec2::ZERO), |t| (t.vel, t.jumped)),
                _ => (Vec2::ZERO, Vec2::ZERO),
            });
        }
        self.backs.clear();
        for back in backs {
            self.world.push_surface(back.surface);
            self.backs.insert(back.surface.id, back.vel);
            self.ranked.push((back.vel, Vec2::ZERO));
        }
        // Une seule fois pour toutes les fenêtres et tous les dos.
        self.world.reindex();
    }

    /// Vitesse de la surface de ce rang dans le monde lissé (voir
    /// [`Tracker::velocity`]) : sans rien chercher, pour un animal en l'air
    /// qui passe en revue les surfaces de ses colonnes.
    #[must_use]
    pub(crate) fn velocity_at(&self, rank: usize) -> Vec2 {
        self.ranked.get(rank).map_or(Vec2::ZERO, |&(vel, _)| vel)
    }

    /// Déplacement d'un coup de la surface de ce rang (voir
    /// [`Tracker::jumped`]).
    #[must_use]
    pub(crate) fn jumped_at(&self, rank: usize) -> Vec2 {
        self.ranked
            .get(rank)
            .map_or(Vec2::ZERO, |&(_, jumped)| jumped)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::math::Rect;
    use crate::world::{LedgeOptions, Screen, WindowRect};

    #[test]
    fn a_ball_dropped_flat_bounces_straight_up() {
        let mut body = Rigid::boxed(Vec2::new(0.0, 500.0), 0.0, 60.0, 40.0);
        let hit = collide_static(
            &mut body,
            Vec2::new(0.0, 20.0),
            Vec2::new(0.0, -1.0),
            Vec2::ZERO,
            0.5,
            0.3,
        );
        assert_eq!(hit, 500.0);
        assert!((body.vel.y + 250.0).abs() < 1e-3, "{body:?}");
        assert!(body.spin.abs() < 1e-6);
    }

    #[test]
    fn landing_on_a_corner_starts_a_spin() {
        // Le coin droit touche d'abord : le corps tourne dans le sens
        // inverse des aiguilles d'une montre (il bascule vers la gauche).
        let mut body = Rigid::boxed(Vec2::new(0.0, 500.0), 0.0, 60.0, 40.0);
        collide_static(
            &mut body,
            Vec2::new(30.0, 20.0),
            Vec2::new(0.0, -1.0),
            Vec2::ZERO,
            0.0,
            0.0,
        );
        assert!(body.spin < -1.0, "{body:?}");
        assert!(body.vel.y > -1e-3 && body.vel.y < 500.0);
    }

    #[test]
    fn a_moving_window_carries_through_friction() {
        // Posé sur une fenêtre qui part vers la droite : le frottement
        // l'entraîne, sans plus que ce que permet son poids.
        let mut body = Rigid::boxed(Vec2::new(0.0, 10.0), 0.0, 60.0, 40.0);
        collide_static(
            &mut body,
            Vec2::new(0.0, 20.0),
            Vec2::new(0.0, -1.0),
            Vec2::new(300.0, 0.0),
            0.0,
            0.5,
        );
        assert!(body.vel.x > 0.0 && body.vel.x <= 5.0 + 1e-3, "{body:?}");
    }

    #[test]
    fn two_equal_bodies_trade_their_speeds_head_on() {
        let mut a = Rigid::boxed(Vec2::new(400.0, 0.0), 0.0, 60.0, 40.0);
        let mut b = Rigid::boxed(Vec2::new(-100.0, 0.0), 0.0, 60.0, 40.0);
        let closing = collide_pair(
            &mut a,
            Vec2::new(0.0, 0.0),
            &mut b,
            Vec2::new(60.0, 0.0),
            Vec2::new(30.0, 0.0),
            Vec2::new(1.0, 0.0),
            1.0,
            0.0,
        );
        assert_eq!(closing, 500.0);
        assert!((a.vel.x + 100.0).abs() < 1e-3 && (b.vel.x - 400.0).abs() < 1e-3);
        // L'élan se conserve.
        assert!((a.vel.x + b.vel.x - 300.0).abs() < 1e-3);
    }

    #[test]
    fn rectangles_report_their_overlap() {
        let a = Obb {
            center: Vec2::new(0.0, 0.0),
            half: Vec2::new(30.0, 20.0),
            angle: 0.0,
        };
        let b = Obb {
            center: Vec2::new(55.0, 5.0),
            ..a
        };
        let contact = obb_contact(&a, &b).expect("ils se touchent");
        assert_eq!(contact.normal, Vec2::new(1.0, 0.0));
        assert!((contact.depth - 5.0).abs() < 1e-3);
        assert!((contact.point.x - 27.5).abs() < 1e-3, "{contact:?}");
        let far = Obb {
            center: Vec2::new(61.0, 0.0),
            ..a
        };
        assert!(obb_contact(&a, &far).is_none());
        // Tourné d'un huitième de tour, il dépasse plus loin.
        let tilted = Obb {
            angle: TAU / 8.0,
            ..far
        };
        assert!(obb_contact(&a, &tilted).is_some());
    }

    #[test]
    fn extents_follow_the_angle() {
        let body = Obb {
            center: Vec2::ZERO,
            half: Vec2::new(30.0, 20.0),
            angle: TAU / 4.0,
        };
        assert!((body.extent_y() - 30.0).abs() < 1e-3);
        assert!((body.extent_x() - 20.0).abs() < 1e-3);
        let lowest = Obb { angle: 0.0, ..body }.lowest_point();
        assert!(lowest.x.abs() < 1e-3 && (lowest.y - 20.0).abs() < 1e-3);
    }

    #[test]
    fn righting_settles_on_the_nearest_whole_turn() {
        let (mut angle, mut spin) = (0.9, 0.0);
        for _ in 0..240 {
            (angle, spin) = righting_step(angle, spin, 1.0 / 120.0);
        }
        assert_eq!((angle, spin), (0.0, 0.0));
        // En roulant vite, il finit le tour.
        assert_eq!(upright_target(4.0, 8.0), TAU);
        assert_eq!(upright_target(4.0, 0.0), TAU);
        assert_eq!(upright_target(2.0, 0.0), 0.0);
        assert_eq!(upright_target(-2.0, -8.0), -TAU);
    }

    #[test]
    fn pets_backs_have_their_own_ids() {
        assert_eq!(surface_pet(pet_surface(7)), Some(7));
        assert_eq!(surface_pet(5 << 8), None);
        assert_eq!(surface_pet((1 << 63) | 2), None);
    }

    fn world_with(window: Rect) -> World {
        let mut world = World::new(vec![Screen {
            frame: Rect::new(0.0, 0.0, 1440.0, 900.0),
            usable: Rect::new(0.0, 25.0, 1440.0, 800.0),
            scale: 2.0,
        }]);
        world.set_windows(
            &[WindowRect {
                id: 3,
                frame: window,
            }],
            LedgeOptions::default(),
        );
        world
    }

    #[test]
    fn the_tracker_smooths_a_dragged_window_and_knows_its_speed() {
        let mut tracker = Tracker::default();
        let dt = 1.0 / 120.0;
        let mut x = 200.0;
        let mut ledge_x = Vec::new();
        // Une fenêtre tirée vers la droite à 600 pt/s, relevée toutes les 33 ms.
        let mut world = world_with(Rect::new(x, 400.0, 500.0, 300.0));
        let poll = |world: &mut World, x: f32| {
            world.set_windows(
                &[WindowRect {
                    id: 3,
                    frame: Rect::new(x, 400.0, 500.0, 300.0),
                }],
                LedgeOptions::default(),
            );
        };
        for step in 0..120 {
            if step % 4 == 0 {
                x = 200.0 + 600.0 * step as f32 * dt;
                poll(&mut world, x);
            }
            tracker.update(&world, dt, &[]);
            let ledge = tracker
                .world()
                .surfaces()
                .iter()
                .find(|s| s.kind == SurfaceKind::Window);
            ledge_x.push(ledge.unwrap().x0);
        }
        let speed = tracker.velocity(3 << 8).x;
        assert!((speed - 600.0).abs() < 60.0, "{speed}");
        // Le rebord lissé avance à chaque pas, sans attendre le relevé suivant.
        let steps: Vec<f32> = ledge_x.windows(2).map(|w| w[1] - w[0]).collect();
        assert!(
            steps[60..].iter().all(|&d| d > 1.0 && d < 12.0),
            "{steps:?}"
        );
        // Arrêtée, elle s'immobilise dès le relevé suivant.
        for step in 0..60 {
            if step % 4 == 0 {
                poll(&mut world, x);
            }
            tracker.update(&world, dt, &[]);
        }
        assert!(tracker.velocity(3 << 8).length() < 1.0);
        let ledge = tracker
            .world()
            .surfaces()
            .iter()
            .find(|s| s.kind == SurfaceKind::Window)
            .unwrap();
        assert!((ledge.x0 - x).abs() < 1.0, "{} {x}", ledge.x0);
    }

    #[test]
    fn a_window_resized_from_its_left_edge_does_not_carry_anything() {
        let mut tracker = Tracker::default();
        let dt = 1.0 / 120.0;
        let mut world = world_with(Rect::new(300.0, 400.0, 500.0, 300.0));
        tracker.update(&world, dt, &[]);
        for step in 1..60 {
            if step % 4 == 0 {
                // Le bord gauche recule, le droit reste.
                let x = 300.0 - step as f32 * 3.0;
                world.set_windows(
                    &[WindowRect {
                        id: 3,
                        frame: Rect::new(x, 400.0, 800.0 - x, 300.0),
                    }],
                    LedgeOptions::default(),
                );
            }
            tracker.update(&world, dt, &[]);
            assert_eq!(tracker.velocity(3 << 8).x, 0.0);
        }
    }

    #[test]
    fn the_tracker_adds_the_backs_of_pets() {
        let mut tracker = Tracker::default();
        let world = world_with(Rect::new(200.0, 400.0, 500.0, 300.0));
        let back = Back {
            surface: Surface {
                id: pet_surface(4),
                kind: SurfaceKind::Window,
                x0: 100.0,
                x1: 140.0,
                y: 760.0,
            },
            vel: Vec2::new(40.0, 0.0),
        };
        tracker.update(&world, 1.0 / 120.0, &[back]);
        assert!(tracker.world().surface(pet_surface(4)).is_some());
        assert_eq!(tracker.velocity(pet_surface(4)), Vec2::new(40.0, 0.0));
        assert!(
            world.surface(pet_surface(4)).is_none(),
            "le vrai monde ne change pas"
        );
    }
}
