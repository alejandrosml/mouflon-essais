//! Petits outils mathématiques : vecteurs, rectangles, ressorts amortis et
//! générateur pseudo-aléatoire déterministe.

use std::ops::{Add, AddAssign, Div, Mul, MulAssign, Neg, Sub, SubAssign};

/// Vecteur 2D en points logiques.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct Vec2 {
    pub x: f32,
    pub y: f32,
}

impl Vec2 {
    pub const ZERO: Self = Self { x: 0.0, y: 0.0 };

    #[must_use]
    pub const fn new(x: f32, y: f32) -> Self {
        Self { x, y }
    }

    #[must_use]
    pub fn length(self) -> f32 {
        self.x.hypot(self.y)
    }

    #[must_use]
    pub fn normalize_or_zero(self) -> Self {
        let len = self.length();
        if len > f32::EPSILON {
            self / len
        } else {
            Self::ZERO
        }
    }

    #[must_use]
    pub fn lerp(self, other: Self, t: f32) -> Self {
        self + (other - self) * t
    }

    /// Limite la norme du vecteur à `max`.
    #[must_use]
    pub fn clamp_length(self, max: f32) -> Self {
        let len = self.length();
        if len > max && len > f32::EPSILON {
            self * (max / len)
        } else {
            self
        }
    }

    #[must_use]
    pub fn dot(self, other: Self) -> f32 {
        self.x * other.x + self.y * other.y
    }

    /// Produit vectoriel en 2D (un nombre) : positif quand `other` tourne
    /// dans le sens horaire de l'écran par rapport à `self` (y vers le bas).
    #[must_use]
    pub fn cross(self, other: Self) -> f32 {
        self.x * other.y - self.y * other.x
    }

    /// Le vecteur tourné d'un quart de tour dans le sens horaire de l'écran :
    /// la vitesse d'un point à `self` du centre d'un corps qui tourne à un
    /// radian par seconde.
    #[must_use]
    pub fn perp(self) -> Self {
        Self::new(-self.y, self.x)
    }

    /// Le vecteur tourné de `angle` radians dans le sens horaire de l'écran.
    #[must_use]
    pub fn rotated(self, angle: f32) -> Self {
        let (s, c) = angle.sin_cos();
        Self::new(c * self.x - s * self.y, s * self.x + c * self.y)
    }
}

impl Add for Vec2 {
    type Output = Self;
    fn add(self, rhs: Self) -> Self {
        Self::new(self.x + rhs.x, self.y + rhs.y)
    }
}
impl Sub for Vec2 {
    type Output = Self;
    fn sub(self, rhs: Self) -> Self {
        Self::new(self.x - rhs.x, self.y - rhs.y)
    }
}
impl Mul<f32> for Vec2 {
    type Output = Self;
    fn mul(self, rhs: f32) -> Self {
        Self::new(self.x * rhs, self.y * rhs)
    }
}
impl Div<f32> for Vec2 {
    type Output = Self;
    fn div(self, rhs: f32) -> Self {
        Self::new(self.x / rhs, self.y / rhs)
    }
}
impl Neg for Vec2 {
    type Output = Self;
    fn neg(self) -> Self {
        Self::new(-self.x, -self.y)
    }
}
impl AddAssign for Vec2 {
    fn add_assign(&mut self, rhs: Self) {
        self.x += rhs.x;
        self.y += rhs.y;
    }
}
impl SubAssign for Vec2 {
    fn sub_assign(&mut self, rhs: Self) {
        self.x -= rhs.x;
        self.y -= rhs.y;
    }
}
impl MulAssign<f32> for Vec2 {
    fn mul_assign(&mut self, rhs: f32) {
        self.x *= rhs;
        self.y *= rhs;
    }
}

/// Rectangle aligné sur les axes : origine en haut à gauche, y vers le bas.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct Rect {
    pub x: f32,
    pub y: f32,
    pub w: f32,
    pub h: f32,
}

impl Rect {
    #[must_use]
    pub const fn new(x: f32, y: f32, w: f32, h: f32) -> Self {
        Self { x, y, w, h }
    }

    #[must_use]
    pub fn left(&self) -> f32 {
        self.x
    }
    #[must_use]
    pub fn right(&self) -> f32 {
        self.x + self.w
    }
    #[must_use]
    pub fn top(&self) -> f32 {
        self.y
    }
    #[must_use]
    pub fn bottom(&self) -> f32 {
        self.y + self.h
    }
    #[must_use]
    pub fn center(&self) -> Vec2 {
        Vec2::new(self.x + self.w * 0.5, self.y + self.h * 0.5)
    }

    #[must_use]
    pub fn contains(&self, p: Vec2) -> bool {
        p.x >= self.left() && p.x <= self.right() && p.y >= self.top() && p.y <= self.bottom()
    }

    #[must_use]
    pub fn union(&self, other: &Self) -> Self {
        let l = self.left().min(other.left());
        let t = self.top().min(other.top());
        let r = self.right().max(other.right());
        let b = self.bottom().max(other.bottom());
        Self::new(l, t, r - l, b - t)
    }

    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.w <= 0.0 || self.h <= 0.0
    }
}

#[must_use]
pub fn lerp(a: f32, b: f32, t: f32) -> f32 {
    a + (b - a) * t
}

/// Rapproche `current` de `target` d'au plus `max_delta`.
#[must_use]
pub fn approach(current: f32, target: f32, max_delta: f32) -> f32 {
    if current < target {
        (current + max_delta).min(target)
    } else {
        (current - max_delta).max(target)
    }
}

/// Lissage exponentiel indépendant du pas de temps : `rate` en 1/s.
#[must_use]
pub fn damp(current: f32, target: f32, rate: f32, dt: f32) -> f32 {
    lerp(current, target, 1.0 - (-rate * dt).exp())
}

/// `x.rem_euclid(period)`, sans division dans le cas d'une phase qui vient
/// de dépasser sa période (elle avance d'un peu à chaque pas) : `x - period`
/// y est exact (lemme de Sterbenz), donc le même résultat au bit près.
#[must_use]
pub fn wrap_phase(x: f32, period: f32) -> f32 {
    if (0.0..period).contains(&x) {
        x
    } else if x >= period && x < 2.0 * period {
        x - period
    } else {
        x.rem_euclid(period)
    }
}

/// `x` ramené entre `lo` et `hi`, sans jamais paniquer comme `f32::clamp`
/// quand les bornes sont calculées. Un rebord exactement aussi large que
/// l'animal, une fois arrondi, peut donner `lo` plus grand que `hi` d'un
/// rien (`x0 + w / 2` arrondi vers le haut, `x1 - w / 2` vers le bas) :
/// on prend alors leur milieu. Une borne non finie est ignorée.
#[must_use]
pub fn clamp_between(x: f32, lo: f32, hi: f32) -> f32 {
    if lo <= hi {
        x.clamp(lo, hi)
    } else if lo.is_finite() && hi.is_finite() {
        0.5 * (lo + hi)
    } else if lo.is_finite() {
        x.max(lo)
    } else if hi.is_finite() {
        x.min(hi)
    } else {
        x
    }
}

#[must_use]
pub fn smoothstep(edge0: f32, edge1: f32, x: f32) -> f32 {
    let t = ((x - edge0) / (edge1 - edge0)).clamp(0.0, 1.0);
    t * t * (3.0 - 2.0 * t)
}

/// Ressort amorti (oscillateur harmonique), intégré de façon semi-implicite.
///
/// C'est la brique de toute l'animation secondaire : écrasement à
/// l'atterrissage, oreilles, inclinaison du corps.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Spring {
    pub value: f32,
    pub velocity: f32,
    pub target: f32,
    stiffness: f32,
    damping: f32,
}

impl Spring {
    /// `frequency` en Hz, `damping_ratio` : 1.0 = amortissement critique,
    /// en dessous le ressort rebondit.
    #[must_use]
    pub fn new(frequency: f32, damping_ratio: f32) -> Self {
        let omega = std::f32::consts::TAU * frequency;
        Self {
            value: 0.0,
            velocity: 0.0,
            target: 0.0,
            stiffness: omega * omega,
            damping: 2.0 * damping_ratio * omega,
        }
    }

    pub fn step(&mut self, dt: f32) {
        let accel = self.stiffness * (self.target - self.value) - self.damping * self.velocity;
        self.velocity += accel * dt;
        self.value += self.velocity * dt;
    }

    /// Donne une impulsion (en unités par seconde).
    pub fn kick(&mut self, impulse: f32) {
        self.velocity += impulse;
    }

    pub fn reset(&mut self, value: f32) {
        self.value = value;
        self.velocity = 0.0;
        self.target = value;
    }
}

/// Générateur pseudo-aléatoire PCG32 : déterministe, rapide, sans dépendance.
#[derive(Clone, Debug)]
pub struct Rng {
    state: u64,
    inc: u64,
}

impl Rng {
    #[must_use]
    pub fn new(seed: u64) -> Self {
        let mut rng = Self {
            state: 0,
            inc: (seed << 1) | 1,
        };
        rng.next_u32();
        rng.state = rng.state.wrapping_add(seed ^ 0x9E37_79B9_7F4A_7C15);
        rng.next_u32();
        rng
    }

    pub fn next_u32(&mut self) -> u32 {
        let old = self.state;
        self.state = old
            .wrapping_mul(6_364_136_223_846_793_005)
            .wrapping_add(self.inc);
        let xorshifted = (((old >> 18) ^ old) >> 27) as u32;
        let rot = (old >> 59) as u32;
        xorshifted.rotate_right(rot)
    }

    /// Un autre générateur, tiré de l'état de celui-ci sans le faire avancer :
    /// ses tirages ne changent rien à la suite de celui-ci.
    #[must_use]
    pub fn fork(&self, salt: u64) -> Self {
        Self::new(self.state ^ self.inc.rotate_left(17) ^ salt)
    }

    /// Nombre dans [0, 1).
    pub fn next_f32(&mut self) -> f32 {
        (self.next_u32() >> 8) as f32 / (1u32 << 24) as f32
    }

    pub fn range(&mut self, min: f32, max: f32) -> f32 {
        if max <= min {
            min
        } else {
            lerp(min, max, self.next_f32())
        }
    }

    pub fn chance(&mut self, probability: f32) -> bool {
        self.next_f32() < probability
    }

    /// Tire un indice selon des poids positifs. `None` si tous les poids sont nuls.
    pub fn pick_weighted(&mut self, weights: impl Iterator<Item = f32> + Clone) -> Option<usize> {
        let total: f32 = weights.clone().map(|w| w.max(0.0)).sum();
        if total <= 0.0 {
            return None;
        }
        let mut roll = self.next_f32() * total;
        let mut last = None;
        for (i, w) in weights.enumerate() {
            let w = w.max(0.0);
            if w <= 0.0 {
                continue;
            }
            last = Some(i);
            if roll < w {
                return Some(i);
            }
            roll -= w;
        }
        last
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Les bornes d'un rebord aussi large que l'animal, à l'arrondi près :
    /// celles qui ont fait paniquer l'essai de charge de la CI (le fil de
    /// l'affichage s'arrêtait, les animaux figés).
    #[test]
    fn clamping_between_crossed_bounds_takes_their_middle_instead_of_panicking() {
        let (lo, hi) = (15.918_855_f32, 15.918_854_f32);
        assert!(lo > hi);
        let x = clamp_between(3.0, lo, hi);
        assert!(x >= hi && x <= lo, "{x}");
        assert_eq!(clamp_between(3.0, 1.0, 2.0), 2.0);
        assert_eq!(clamp_between(1.5, 1.0, 2.0), 1.5);
        assert_eq!(clamp_between(0.0, 1.0, 2.0), 1.0);
        assert_eq!(clamp_between(5.0, f32::NAN, 2.0), 2.0);
        assert_eq!(clamp_between(-5.0, 1.0, f32::NAN), 1.0);
        assert_eq!(clamp_between(5.0, f32::NAN, f32::NAN), 5.0);
        // Un rebord de la largeur exacte de l'animal, partout sur l'écran :
        // jamais de panique, et le point reste entre les deux bords.
        let mut rng = Rng::new(11);
        for _ in 0..100_000 {
            let x0 = rng.range(-3000.0, 3000.0);
            let half = rng.range(1.0, 120.0);
            let x1 = x0 + 2.0 * half;
            let x = clamp_between(rng.range(-4000.0, 4000.0), x0 + half, x1 - half);
            assert!(x >= x0 && x <= x1, "{x0} {x1} {x}");
        }
    }

    /// Une phase qui avance, recule ou saute loin revient dans sa période
    /// exactement comme avec le reste de la division.
    #[test]
    fn wrapped_phases_match_the_remainder_bit_for_bit() {
        let mut rng = Rng::new(7);
        for period in [std::f32::consts::TAU, std::f32::consts::TAU * 8.0, 3600.0] {
            let mut phase = 0.0f32;
            for _ in 0..200_000 {
                phase += rng.range(0.0, period * 0.05);
                let wrapped = wrap_phase(phase, period);
                assert_eq!(wrapped.to_bits(), phase.rem_euclid(period).to_bits());
                phase = wrapped;
            }
            for x in [-1.0, -period * 3.5, period * 7.25, f32::MAX, 0.0, period] {
                assert_eq!(
                    wrap_phase(x, period).to_bits(),
                    x.rem_euclid(period).to_bits(),
                    "{x}"
                );
            }
        }
    }

    #[test]
    fn spring_settles_on_target() {
        let mut s = Spring::new(3.0, 0.4);
        s.kick(-5.0);
        for _ in 0..1200 {
            s.step(1.0 / 120.0);
        }
        assert!(s.value.abs() < 1e-3, "value = {}", s.value);
    }

    #[test]
    fn rng_is_deterministic_and_in_range() {
        let mut a = Rng::new(42);
        let mut b = Rng::new(42);
        for _ in 0..1000 {
            let x = a.next_f32();
            assert_eq!(x, b.next_f32());
            assert!((0.0..1.0).contains(&x));
        }
    }

    #[test]
    fn weighted_pick_respects_zero_weights() {
        let mut rng = Rng::new(7);
        let weights = [0.0, 3.0, 0.0, 1.0];
        let mut counts = [0u32; 4];
        for _ in 0..4000 {
            counts[rng.pick_weighted(weights.iter().copied()).unwrap()] += 1;
        }
        assert_eq!(counts[0], 0);
        assert_eq!(counts[2], 0);
        assert!(counts[1] > counts[3] * 2, "{counts:?}");
    }
}
