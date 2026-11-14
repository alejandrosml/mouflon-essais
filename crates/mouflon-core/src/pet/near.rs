//! Les paires d'animaux assez proches pour se toucher.
//!
//! Comparer chaque animal à tous les autres coûte le carré de leur nombre :
//! un demi-million de paires pour mille moutons, cent vingt fois par
//! seconde. Or deux animaux ne se heurtent, ne se rencontrent et ne se
//! cognent que s'ils se touchent presque, et de deux façons seulement :
//!
//! - leurs dessins se touchent à la même hauteur (un heurt, une rencontre) :
//!   chacun occupe une bande à hauteur de ses pattes, large de ce qu'il
//!   atteint ;
//! - leurs corps se cognent (physique réaliste), ce qui demande que l'un
//!   d'eux soit en l'air : chacun occupe le rectangle de son corps tourné.
//!
//! Pour chaque façon, on range les rectangles par bord gauche, puis on
//! balaie : pour chacun, on ne regarde que ceux qui commencent avant son
//! bord droit. D'un pas à l'autre, les animaux bougent à peine : le
//! rangement du pas d'avant est presque bon, et le tri (qui profite de
//! l'ordre déjà là) ne coûte presque rien. Dans une foule serrée (des
//! centaines d'animaux empilés sur un petit écran), des rectangles ajustés
//! à chaque façon font bien moins de paires qu'un seul qui les couvrirait
//! toutes.

use super::{Pet, Support};

/// Ce qu'un animal peut bouger pendant qu'on traite les paires d'un pas
/// (séparé d'un autre, posé contre lui) : ajouté autour de chacun, pour ne
/// manquer aucune paire qui se toucherait après coup. Une paire manquée
/// ainsi l'est pour un pas seulement : elle est là au suivant.
const SLACK: f32 = 8.0;
/// La hauteur où deux animaux se heurtent (voir `bump`) : leurs pattes
/// sont à moins de cette part de leur hauteur l'une de l'autre.
const LEVEL: f32 = 0.8;
/// En deçà de cette vitesse de côté (pt/s), un animal ne va vers personne :
/// un heurt demande que les deux se rapprochent d'au moins 6 pt/s, une
/// rencontre que celui qui la fait avance d'au moins 1 pt/s.
const MOVING: f32 = 0.5;

/// Un rectangle, marge comprise.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
struct Span {
    x0: f32,
    x1: f32,
    y0: f32,
    y1: f32,
}

impl Span {
    /// La bande où ses dessins en touchent d'autres à sa hauteur : de ses
    /// pattes jusqu'aux quatre cinquièmes de sa hauteur, aussi large que ce
    /// qu'il atteint. Deux animaux « au même niveau » (voir `bump`) ont
    /// toujours leurs bandes qui se chevauchent : les pattes du plus haut
    /// sont dans la bande de l'autre.
    fn reach(pet: &Pet) -> Self {
        let p = pet.def.physics;
        let (front, back) = pet.reach.unwrap_or((p.width * 0.5, p.width * 0.5));
        let reach = front.max(back) + SLACK;
        Self {
            x0: pet.pos.x - reach,
            x1: pet.pos.x + reach,
            y0: pet.pos.y - p.height * LEVEL - SLACK,
            y1: pet.pos.y + SLACK,
        }
    }

    /// Le rectangle de son corps tourné.
    fn body(pet: &Pet) -> Self {
        let p = pet.def.physics;
        // Debout (presque toujours au sol), pas besoin du sinus.
        let (sin, cos) = if pet.angle == 0.0 {
            (0.0, 1.0)
        } else {
            pet.angle.sin_cos()
        };
        let (w, h) = (p.width * 0.5, p.height * 0.5);
        let half_x = cos.abs() * w + sin.abs() * h + SLACK;
        let half_y = sin.abs() * w + cos.abs() * h + SLACK;
        let center_y = pet.pos.y - h;
        Self {
            x0: pet.pos.x - half_x,
            x1: pet.pos.x + half_x,
            y0: center_y - half_y,
            y1: center_y + half_y,
        }
    }

    fn overlaps_y(&self, other: &Self) -> bool {
        (self.y0 <= other.y1) & (other.y0 <= self.y1)
    }
}

/// Ce que les balayages savent d'un animal, relevé d'un seul passage sur le
/// troupeau (un animal occupe beaucoup de mémoire : on ne le relit pas
/// pendant les balayages).
#[derive(Clone, Copy, Debug, Default)]
struct Facts {
    reach: Span,
    body: Span,
    /// Il peut heurter ou rencontrer.
    reaching: bool,
    /// Il va de côté : un heurt ou une rencontre demande que l'un des deux
    /// au moins aille vers l'autre (voir `bump` et `meeting`).
    moving: bool,
    /// Il est en l'air.
    flying: bool,
}

/// Une façon de se toucher : le rangement des rectangles, et les paires.
#[derive(Clone, Debug, Default)]
struct Sweep {
    /// Les animaux (leurs rangs), par bord gauche ; d'un pas à l'autre,
    /// l'ordre est presque bon.
    order: Vec<u32>,
    /// Ceux qui prennent part, dans cet ordre, côte à côte en mémoire (la
    /// boucle intérieure les lit à la suite) : leur rectangle, leur rang, et
    /// s'ils sont « actifs » (voir [`Self::update`]).
    sorted: Vec<(Span, u32, bool)>,
    pairs: Vec<(u32, u32)>,
}

impl Sweep {
    /// Les paires de ceux que `part` retient dont les rectangles (`span`) se
    /// chevauchent ; si `active_only`, seulement celles dont l'un au moins
    /// est actif (`active`).
    fn update(
        &mut self,
        facts: &[Facts],
        span: impl Fn(&Facts) -> Span,
        part: impl Fn(&Facts) -> bool,
        active: impl Fn(&Facts) -> bool,
        active_only: bool,
    ) {
        if self.order.len() != facts.len() {
            self.order.clear();
            self.order.extend(0..facts.len() as u32);
        }
        // Par bord gauche, puis par rang : un ordre total, le même quel que
        // soit le tri. D'un pas à l'autre, quelques voisins seulement ont
        // échangé leur place : un tri par insertion ne coûte alors qu'un
        // passage. Trop de désordre (au premier pas, après un grand saut) :
        // un tri complet.
        let before = |a: u32, b: u32| {
            span(&facts[a as usize])
                .x0
                .total_cmp(&span(&facts[b as usize]).x0)
                .then(a.cmp(&b))
                .is_lt()
        };
        if !insertion_sort(&mut self.order, before) {
            self.order.sort_by(|&a, &b| {
                span(&facts[a as usize])
                    .x0
                    .total_cmp(&span(&facts[b as usize]).x0)
                    .then(a.cmp(&b))
            });
        }
        self.sorted.clear();
        self.sorted.extend(self.order.iter().filter_map(|&i| {
            let f = &facts[i as usize];
            part(f).then(|| (span(f), i, active(f)))
        }));
        self.pairs.clear();
        let mut kept = Kept::default();
        let sorted = &self.sorted;
        // Peu d'actifs : on ne cherche qu'autour d'eux ; sinon, un seul
        // balayage de tous revient moins cher.
        let actives = sorted.iter().filter(|(_, _, active)| *active).count();
        if !active_only || actives * 4 > sorted.len() {
            for (k, &(a, i, a_active)) in sorted.iter().enumerate() {
                for &(b, j, b_active) in &sorted[k + 1..] {
                    if b.x0 > a.x1 {
                        break;
                    }
                    let keep = (a_active | b_active | !active_only) & a.overlaps_y(&b);
                    kept.push((i, j), keep, &mut self.pairs);
                }
            }
        } else {
            // On ne cherche qu'autour des actifs : leurs voisins possibles
            // commencent au plus le plus large des rectangles avant eux.
            let widest = sorted
                .iter()
                .map(|(s, _, _)| s.x1 - s.x0)
                .fold(0.0, f32::max);
            for (k, &(a, i, active)) in sorted.iter().enumerate() {
                if !active {
                    continue;
                }
                let from = sorted.partition_point(|(s, _, _)| s.x0 < a.x0 - widest);
                for (m, &(b, j, b_active)) in sorted.iter().enumerate().skip(from) {
                    if b.x0 > a.x1 {
                        break;
                    }
                    // Deux actifs : la paire une seule fois, par le premier.
                    let keep = (m != k) & (b.x1 >= a.x0) & !(b_active & (m < k)) & a.overlaps_y(&b);
                    kept.push((i, j), keep, &mut self.pairs);
                }
            }
        }
        kept.flush(&mut self.pairs);
    }
}

/// Trie `order` par insertion, tant que cela reste bon marché : `false` (et
/// un ordre à finir de trier, mêmes éléments) si les déplacements dépassent
/// quelques passages.
fn insertion_sort(order: &mut [u32], before: impl Fn(u32, u32) -> bool) -> bool {
    let limit = order.len() * 4 + 64;
    let mut moves = 0usize;
    for k in 1..order.len() {
        let item = order[k];
        let mut m = k;
        while m > 0 && before(item, order[m - 1]) {
            order[m] = order[m - 1];
            m -= 1;
            moves += 1;
        }
        order[m] = item;
        if moves > limit {
            return false;
        }
    }
    true
}

/// Les paires gardées, par paquets : sans branchement pour en garder une
/// (le processeur ne peut pas deviner lesquelles se chevauchent), chacune
/// est écrite, et le compte n'avance que pour celles qu'on garde.
#[derive(Debug)]
struct Kept {
    pairs: [(u32, u32); 64],
    count: usize,
}

impl Default for Kept {
    fn default() -> Self {
        Self {
            pairs: [(0, 0); 64],
            count: 0,
        }
    }
}

impl Kept {
    fn push(&mut self, (i, j): (u32, u32), keep: bool, out: &mut Vec<(u32, u32)>) {
        self.pairs[self.count] = (i.min(j), i.max(j));
        self.count += usize::from(keep);
        if self.count == self.pairs.len() {
            out.extend_from_slice(&self.pairs);
            self.count = 0;
        }
    }

    fn flush(&mut self, out: &mut Vec<(u32, u32)>) {
        out.extend_from_slice(&self.pairs[..self.count]);
        self.count = 0;
    }
}

/// Les paires proches d'un pas.
#[derive(Clone, Copy, Debug)]
pub(super) struct Pairs<'a> {
    /// Leurs dessins se touchent peut-être à la même hauteur (heurts,
    /// rencontres).
    pub reach: &'a [(u32, u32)],
    /// Leurs corps se cognent peut-être, l'un au moins en l'air (physique
    /// réaliste) ; vide si on ne les a pas demandées.
    pub bodies: &'a [(u32, u32)],
}

/// Les paires proches, refaites à chaque pas.
#[derive(Clone, Debug, Default)]
pub(super) struct Near {
    facts: Vec<Facts>,
    reach: Sweep,
    bodies: Sweep,
}

impl Near {
    /// Les paires `(i, j)` (rangs dans `pets`, `i < j`) qui peuvent se
    /// toucher, dans un ordre qui ne dépend que de leurs places. `bodies` :
    /// celles des corps aussi (physique réaliste).
    pub(super) fn update(&mut self, pets: &[Pet], bodies: bool) -> Pairs<'_> {
        self.facts.clear();
        self.facts.extend(pets.iter().map(|pet| Facts {
            reach: Span::reach(pet),
            body: if bodies {
                Span::body(pet)
            } else {
                Span::default()
            },
            reaching: pet.can_bump() || pet.can_meet(),
            moving: pet.vel.x.abs() > MOVING,
            flying: pet.support == Support::Air,
        }));
        // Seuls ceux qui peuvent heurter ou rencontrer (les deux d'une paire
        // le doivent) prennent part, et l'un des deux au moins doit aller de
        // côté : dans une foule, beaucoup restent sur place.
        self.reach
            .update(&self.facts, |f| f.reach, |f| f.reaching, |f| f.moving, true);
        if bodies {
            // Un choc de corps demande que l'un des deux soit en l'air.
            self.bodies
                .update(&self.facts, |f| f.body, |_| true, |f| f.flying, true);
        } else {
            self.bodies.pairs.clear();
        }
        Pairs {
            reach: &self.reach.pairs,
            bodies: &self.bodies.pairs,
        }
    }
}

#[cfg(test)]
mod tests {
    use std::sync::Arc;

    use super::*;
    use crate::math::{Rect, Vec2};
    use crate::world::{Screen, World};

    fn world() -> World {
        World::new(vec![Screen {
            frame: Rect::new(0.0, 0.0, 3000.0, 2000.0),
            usable: Rect::new(0.0, 0.0, 3000.0, 1900.0),
            scale: 2.0,
        }])
    }

    /// Une foule au hasard : des animaux de tailles, de places, d'angles et
    /// d'étendues variés, au sol ou en l'air.
    fn crowd(count: usize, seed: u64) -> Vec<Pet> {
        let path = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("../../tests/donnees/lainou/pet.toml");
        let def = Arc::new(crate::format::load_pet(&path).unwrap());
        let world = world();
        let mut rng = crate::math::Rng::new(seed);
        let mut unit = || (rng.next_u32() >> 8) as f32 / (1u32 << 24) as f32;
        (0..count)
            .map(|i| {
                let mut pet = Pet::spawn(i as u32, Arc::clone(&def), i as u64, &world, None);
                pet.pos = Vec2::new(unit() * 1500.0, 400.0 + unit() * 300.0);
                pet.angle = (unit() - 0.5) * 6.0;
                if unit() < 0.5 {
                    pet.reach = Some((20.0 + unit() * 90.0, 20.0 + unit() * 90.0));
                }
                pet.facing = if unit() < 0.5 { -1.0 } else { 1.0 };
                // La moitié marche, l'autre reste sur place.
                pet.vel.x = if unit() < 0.5 {
                    (unit() - 0.5) * 120.0
                } else {
                    0.0
                };
                pet.support = if unit() < 0.3 {
                    Support::Air
                } else {
                    Support::Ground { surface: 0 }
                };
                pet
            })
            .collect()
    }

    /// Leurs dessins se touchent à la même hauteur : un heurt ou une
    /// rencontre possible (voir `bump` et `meeting`).
    fn reaching(a: &Pet, b: &Pet) -> bool {
        let dx = b.pos.x - a.pos.x;
        let side = if dx == 0.0 { -a.facing } else { dx.signum() };
        let reach = a.reach_toward(side) + b.reach_toward(-side);
        let (ha, hb) = (a.def.physics.height, b.def.physics.height);
        let level = a.pos.y > b.pos.y - hb * LEVEL && b.pos.y > a.pos.y - ha * LEVEL;
        let bump = dx.abs() < reach && level;
        let meeting = dx.abs() <= reach && (a.pos.y - b.pos.y).abs() <= 2.0;
        // L'un des deux au moins va de côté (sinon ni heurt ni rencontre).
        let moving = a.vel.x.abs() > MOVING || b.vel.x.abs() > MOVING;
        (bump || meeting) && moving
    }

    /// Leurs corps se cognent, l'un au moins en l'air (voir `collide_bodies`).
    fn hitting(a: &Pet, b: &Pet) -> bool {
        (a.support == Support::Air || b.support == Support::Air)
            && crate::physics::obb_contact(&a.body(), &b.body()).is_some()
    }

    #[test]
    fn no_touching_pair_is_missed() {
        for seed in 0..20 {
            let pets = crowd(300, seed);
            let mut near = Near::default();
            let found = near.update(&pets, true);
            let reach: std::collections::HashSet<(u32, u32)> =
                found.reach.iter().copied().collect();
            let bodies: std::collections::HashSet<(u32, u32)> =
                found.bodies.iter().copied().collect();
            let (mut reaching_pairs, mut hitting_pairs) = (0, 0);
            for i in 0..pets.len() {
                for j in i + 1..pets.len() {
                    let pair = (i as u32, j as u32);
                    if reaching(&pets[i], &pets[j]) {
                        reaching_pairs += 1;
                        assert!(reach.contains(&pair), "paire {i}, {j} manquée (dessins)");
                    }
                    if hitting(&pets[i], &pets[j]) {
                        hitting_pairs += 1;
                        assert!(bodies.contains(&pair), "paire {i}, {j} manquée (corps)");
                    }
                }
            }
            assert!(reaching_pairs > 0 && hitting_pairs > 0);
            // Et bien moins de paires que toutes les paires.
            let all = pets.len() * (pets.len() - 1) / 2;
            assert!(reach.len() < all / 8, "{}", reach.len());
            assert!(bodies.len() < all / 8, "{}", bodies.len());
        }
    }

    #[test]
    fn pairs_are_listed_once_and_in_the_same_order_each_time() {
        let pets = crowd(200, 7);
        let mut near = Near::default();
        let first = near.update(&pets, true);
        let (reach, bodies) = (first.reach.to_vec(), first.bodies.to_vec());
        for pairs in [&reach, &bodies] {
            let set: std::collections::HashSet<_> = pairs.iter().copied().collect();
            assert_eq!(set.len(), pairs.len(), "une paire en double");
            assert!(pairs.iter().all(|&(i, j)| i < j));
        }
        // Recalculées (l'ordre de départ est celui du pas d'avant) : les mêmes,
        // dans le même ordre.
        let again = near.update(&pets, true);
        assert_eq!(again.reach, reach.as_slice());
        assert_eq!(again.bodies, bodies.as_slice());
        let mut fresh = Near::default();
        let new = fresh.update(&pets, true);
        assert_eq!(new.reach, reach.as_slice());
        assert_eq!(new.bodies, bodies.as_slice());
        // Sans les corps : aucune paire de corps.
        assert!(fresh.update(&pets, false).bodies.is_empty());
    }
}

#[cfg(test)]
mod bench {
    use std::sync::Arc;

    use super::*;
    use crate::math::{Rect, Vec2};
    use crate::world::{Screen, World};

    /// Une foule serrée figée (un petit écran, des animaux empilés, certains
    /// en l'air, la moitié en marche) : le temps de trouver ses paires.
    #[test]
    #[ignore = "mesure, à lancer à la main en --release"]
    fn dense_pile() {
        let path = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("../../tests/donnees/lainou/pet.toml");
        let def = Arc::new(crate::format::load_pet(&path).unwrap());
        let world = World::new(vec![Screen {
            frame: Rect::new(0.0, 0.0, 1024.0, 768.0),
            usable: Rect::new(0.0, 25.0, 1024.0, 670.0),
            scale: 1.0,
        }]);
        let mut rng = crate::math::Rng::new(3);
        let mut unit = || (rng.next_u32() >> 8) as f32 / (1u32 << 24) as f32;
        for count in [100usize, 500, 1000] {
            let pets: Vec<Pet> = (0..count)
                .map(|i| {
                    let mut pet = Pet::spawn(i as u32, Arc::clone(&def), i as u64, &world, None);
                    pet.pos = Vec2::new(unit() * 1024.0, 95.0 + unit() * 600.0);
                    pet.vel.x = if unit() < 0.5 { 40.0 } else { 0.0 };
                    pet.support = if unit() < 0.2 {
                        Support::Air
                    } else {
                        Support::Ground { surface: 0 }
                    };
                    pet
                })
                .collect();
            let runs = 200;
            let mut near = Near::default();
            let started = std::time::Instant::now();
            let (mut reach, mut bodies) = (0, 0);
            for _ in 0..runs {
                let pairs = near.update(&pets, true);
                (reach, bodies) = (pairs.reach.len(), pairs.bodies.len());
            }
            let ms = started.elapsed().as_secs_f64() * 1000.0 / f64::from(runs);
            println!("{count} animaux : {reach} + {bodies} paires en {ms:.3} ms");
        }
    }
}
