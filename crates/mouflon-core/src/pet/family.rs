//! Liens durables entre une brebis et ses agneaux, approche et tétée.
//!
//! Les rendez-vous restent sur une seule surface : l'agneau marche jusqu'à
//! sa place, puis l'encolure rejoint la mamelle. Les identifiants persistent,
//! les rangs du vecteur sont seulement un cache reconstruit après changement.

use std::collections::HashMap;
use std::sync::Arc;

use super::{Flock, Gesture, Pet, PetEvent, PoseSignals, Support};
use crate::def::{Age, Appearance, Motion, PetDef, Rules, SexRatio, Variant};
use crate::math::{Vec2, approach, clamp_between, lerp};
use crate::world::{Surface, World};

/// Durées et nombres de tétées observés chez sept Mérinos à agneau unique
/// (Gordon & Siegmann 1991). Entre les deux âges, interpolation d'animation,
/// et non mesure publiée. L'âge de l'animal ne change pas pendant une session.
fn nursing_schedule(weeks: f32) -> (f32, f32) {
    let t = ((weeks - 2.0) / 4.0).clamp(0.0, 1.0);
    (lerp(41.1, 11.9, t), 86_400.0 / lerp(36.0, 14.3, t))
}

fn valid_lamb_age(age: Age) -> bool {
    matches!(age, Age::Lamb { weeks } if weeks.is_finite() && weeks > 0.0 && weeks <= 52.0)
}

fn family_motions(def: &PetDef) -> bool {
    def.behaviors.iter().any(|b| b.motion == Motion::Stand)
        && def
            .behaviors
            .iter()
            .any(|b| matches!(b.motion, Motion::Walk { speed } if speed > 0.0))
}

/// Taille provisoire du jeune par rapport au dessin adulte, pour un animal
/// qui n'a pas de dessin d'agneau (`[appearance.lamb]`) : choix visuel, pas
/// une courbe de croissance.
fn lamb_scale(weeks: f32) -> f32 {
    lerp(0.55, 0.70, ((weeks - 2.0) / 5.0).clamp(0.0, 1.0))
}

impl PetDef {
    /// Cette espèce a-t-elle des agneaux (voir [`PetDef::as_lamb`]) ? Un
    /// mouton du format de Mouflon, dessiné en vecteurs ou peint.
    #[must_use]
    pub fn has_lambs(&self) -> bool {
        self.rules == Rules::Mouflon
            && self.meta.category() == "sheep"
            && !matches!(self.appearance, Appearance::Sprites(_))
            && matches!(self.age, Age::Adult)
            && family_motions(self)
    }

    /// Prépare hors du fil d'affichage le même animal en agneau : avec son
    /// dessin d'agneau s'il en a un (voir [`PetDef::set_variants`]), à sa
    /// taille ; sinon avec le dessin de sa mère (celui de la brebis, ou le
    /// dessin de base), réduit, qui n'est pas une anatomie d'agneau.
    /// Le sexe fixé pour le dessin de la mère (`sex = "female"`) ne passe
    /// pas à ses petits : un agneau naît mâle ou femelle à chances égales.
    #[must_use]
    pub fn as_lamb(&self, weeks: f32) -> Option<Self> {
        if !weeks.is_finite()
            || !(0.0..=52.0).contains(&weeks)
            || weeks == 0.0
            || self.rules != Rules::Mouflon
            || self.meta.category() != "sheep"
            || matches!(self.appearance, Appearance::Sprites(_))
        {
            return None;
        }
        let mut def = match self.variant(Variant::Lamb) {
            Some(lamb) => (**lamb).clone(),
            None => self
                .variant(Variant::Female)
                .map_or(self, |ewe| &**ewe)
                .scaled(lamb_scale(weeks)),
        };
        // Un agneau n'a qu'un dessin : le sien.
        def.set_variants(Vec::new()).ok()?;
        def.speak_as_lamb();
        def.age = Age::Lamb { weeks };
        def.sex_ratio = SexRatio::default();
        Some(def)
    }
}

#[derive(Clone, Copy, Debug, Default)]
pub(super) enum FamilyAction {
    #[default]
    None,
    Follow {
        goal: f32,
    },
    Approach {
        goal: f32,
    },
    Wait {
        elapsed: f32,
    },
    Nurse {
        elapsed: f32,
        target: Vec2,
    },
}

impl FamilyAction {
    pub(super) fn active(self) -> bool {
        !matches!(self, Self::None)
    }

    fn goal(self) -> Option<f32> {
        match self {
            Self::Follow { goal } | Self::Approach { goal } => Some(goal),
            _ => None,
        }
    }

    pub(super) fn gesture(self) -> Option<(Gesture, f32)> {
        match self {
            Self::Nurse { elapsed, .. } => Some((Gesture::Nurse, elapsed)),
            Self::Wait { elapsed } if elapsed < 0.92 => Some((Gesture::MaternalCall, elapsed)),
            _ => None,
        }
    }
}

#[derive(Clone, Copy, Debug, Default)]
pub(super) struct FamilyState {
    pub mother: Option<u32>,
    pub partner: Option<u32>,
    pub action: FamilyAction,
    /// Mamelle et museau en tétée, fractions de hauteur depuis les sabots.
    points: Option<(Vec2, Vec2)>,
    /// La dernière cible reste valable pendant le fondu de sortie du geste.
    fading_target: Option<Vec2>,
    /// Amortit la respiration globale de la mère pendant qu'elle attend.
    anchor: f32,
}

impl FamilyState {
    pub(super) fn step(&mut self, dt: f32) {
        let target = if matches!(self.action, FamilyAction::Wait { .. }) {
            1.0
        } else {
            0.0
        };
        self.anchor = approach(self.anchor, target, dt * 3.0);
    }
    pub(super) fn stop(&mut self) {
        if let FamilyAction::Nurse { target, .. } = self.action {
            self.fading_target = Some(target);
        }
        self.partner = None;
        self.action = FamilyAction::None;
    }

    pub(super) fn clear_finished_nursing(
        &mut self,
        grounded: bool,
        gesture: Option<Gesture>,
        amount: f32,
    ) {
        if !grounded || gesture != Some(Gesture::Nurse) || amount <= 0.0 {
            self.fading_target = None;
        }
    }
}

#[derive(Clone, Copy, Debug, Default)]
enum Stage {
    #[default]
    Free,
    Approach {
        elapsed: f32,
        facing: f32,
        mother_x: f32,
    },
    Nurse {
        elapsed: f32,
        facing: f32,
        mother_x: f32,
    },
}

#[derive(Clone, Debug)]
struct Link {
    mother: u32,
    lamb: u32,
    indices: (usize, usize),
    due: f32,
    stage: Stage,
}

#[derive(Clone, Debug, Default)]
pub(super) struct Families {
    links: Vec<Link>,
    generation: u64,
    indices: HashMap<u32, usize>,
}

impl Families {
    /// Cet animal a-t-il un agneau ?
    pub(super) fn is_mother(&self, id: u32) -> bool {
        self.links.iter().any(|link| link.mother == id)
    }
}

impl Pet {
    /// Une brebis adulte native, dessinée en vecteurs ou peinte sur un
    /// squelette, qui sait se tenir debout et marcher : elle peut avoir un
    /// agneau (son menu le propose). Une brebis en images n'a pas de tétée.
    #[must_use]
    pub fn can_mother(&self) -> bool {
        self.is_ewe()
            && self.def.rules == Rules::Mouflon
            && !matches!(self.def.appearance, Appearance::Sprites(_))
            && family_motions(&self.def)
    }

    /// Peut recevoir un agneau maintenant : posée, ni tenue ni sur le
    /// départ, pour que le petit n'apparaisse pas dans les airs. Couchée,
    /// endormie ou occupée, elle le reçoit aussi (son dessin se prépare sur
    /// un autre fil, pendant qu'elle continue de vivre) ; la tétée attend
    /// qu'elle soit libre.
    #[must_use]
    pub fn can_have_lamb(&self) -> bool {
        self.can_mother() && self.can_react() && self.hold.is_none()
    }

    /// Identifiant durable de sa mère ; aucun pour un adulte ou un orphelin.
    #[must_use]
    pub fn mother(&self) -> Option<u32> {
        self.family.mother
    }

    /// Vrai pendant la tétée, après la marche d'approche et l'arrêt.
    #[must_use]
    pub fn is_nursing(&self) -> bool {
        matches!(self.family.action, FamilyAction::Nurse { .. })
    }

    /// Donne les points tirés du dessin, sans conserver celui-ci dans le cœur.
    /// Les coordonnées sont des fractions de sa hauteur, depuis `ground`,
    /// x vers le museau et y vers le bas. Une donnée non finie est refusée.
    pub fn set_nursing_points(&mut self, udder: Vec2, muzzle: Vec2) -> bool {
        if [udder.x, udder.y, muzzle.x, muzzle.y]
            .into_iter()
            .any(|v| !v.is_finite() || v.abs() > 2.0)
            || udder.y >= 0.0
            || muzzle.y >= 0.0
        {
            return false;
        }
        self.family.points = Some((udder, muzzle));
        true
    }

    fn nursing_points(&self) -> (Vec2, Vec2) {
        self.family
            .points
            .unwrap_or((Vec2::new(-0.18, -0.34), Vec2::new(0.44, -0.68)))
    }

    /// Pendant l'approche, ralentir avant le but avec l'accélération normale.
    /// Le même déplacement nourrit l'horloge des sabots : aucune translation
    /// artificielle de l'animal pour le placer sous le ventre.
    pub(super) fn family_speed(&self, ordinary: f32, dt: f32) -> f32 {
        let Some(goal) = self.family.action.goal() else {
            return if self.family.action.active() {
                0.0
            } else {
                ordinary
            };
        };
        let distance = goal - self.pos.x;
        let braking = (2.0 * self.def.physics.walk_accel * distance.abs()).sqrt();
        ordinary
            .abs()
            .min(braking)
            .min(distance.abs() / dt.max(1e-6))
            * distance.signum()
    }

    /// Une dernière fraction de pas suffit à atteindre le but ; le déplacement
    /// réellement parcouru, jamais un saut, reste celui que voient les sabots.
    pub(super) fn family_step(&mut self, dt: f32) {
        let mut dx = self.vel.x * dt;
        if let Some(goal) = self.family.action.goal() {
            let left = goal - self.pos.x;
            if dx * left >= 0.0 && dx.abs() > left.abs() {
                dx = left;
                self.vel.x = dx / dt.max(1e-6);
            }
        }
        self.pos.x += dx;
    }

    pub(super) fn family_pose(&self, mut pose: PoseSignals) -> PoseSignals {
        if matches!(self.support, Support::Ground { .. })
            && pose.gesture == Some(Gesture::Nurse)
            && pose.gesture_amount > 0.0
        {
            pose.nursing_target = match self.family.action {
                FamilyAction::Nurse { target, .. } => Some(target),
                _ => self.family.fading_target,
            };
        }
        let free = 1.0 - self.family.anchor;
        pose.breath *= free;
        pose.lean *= free;
        pose.squash *= free;
        if matches!(self.family.action, FamilyAction::Wait { .. }) {
            pose.jaw = 0.0;
        }
        pose
    }

    fn family_available(&self) -> bool {
        self.can_react()
            && self.hold.is_none()
            && self.angle.abs() < 0.02
            && !self.is_asleep()
            && matches!(
                self.def.behaviors[self.behavior].motion,
                Motion::Stand | Motion::Look | Motion::Graze | Motion::Walk { .. }
            )
    }

    fn family_stand(&mut self) -> bool {
        let Some(stand) = self
            .def
            .behaviors
            .iter()
            .position(|b| b.motion == Motion::Stand)
        else {
            return false;
        };
        self.enter(stand);
        self.echo = None;
        self.regroup = None;
        self.invite = None;
        true
    }

    fn family_walk(&mut self, goal: f32, nursing: bool) -> bool {
        let Some(walk) = self
            .def
            .behaviors
            .iter()
            .position(|b| matches!(b.motion, Motion::Walk { speed } if speed > 0.0))
        else {
            return false;
        };
        self.enter(walk);
        self.echo = None;
        self.regroup = None;
        self.invite = None;
        self.facing = (goal - self.pos.x).signum();
        self.family.action = if nursing {
            FamilyAction::Approach { goal }
        } else {
            FamilyAction::Follow { goal }
        };
        true
    }
}

/// Les partenaires se laissent passer sous le flanc, sans désactiver leurs
/// collisions avec les autres animaux ni masquer la perte de leur support.
pub(super) fn family_pair(a: &Pet, b: &Pet) -> bool {
    a.family.partner == Some(b.id) && b.family.partner == Some(a.id)
}

/// Une mère et son agneau ne se heurtent jamais (mode collisions) : il se
/// glisse sous son flanc, et ils se quittent après la tétée, encore l'un
/// sous l'autre, sans se renverser. Le reste du troupeau les heurte.
pub(super) fn kin(a: &Pet, b: &Pet) -> bool {
    a.family.mother == Some(b.id) || b.family.mother == Some(a.id) || family_pair(a, b)
}

/// Peut-elle recevoir ce jeune déjà préparé, qui arrive (tout juste là, ou
/// de retour d'une mise à jour) ?
fn takes_lamb(m: &Pet, def: &Arc<PetDef>) -> bool {
    m.can_mother()
        && !m.finished
        && m.leaving.is_none()
        && valid_lamb_age(def.age)
        && def.meta.category() == "sheep"
        && def.rules == Rules::Mouflon
        && !matches!(def.appearance, Appearance::Sprites(_))
        && def.physics.height < m.def.physics.height
        && family_motions(def)
}

impl Flock {
    /// Lie explicitement un jeune à une brebis de sa famille. Un lien existant
    /// n'est jamais remplacé par la simple proximité d'une autre brebis.
    pub fn bind_mother(&mut self, lamb: u32, mother: u32) -> bool {
        if lamb == mother {
            return false;
        }
        let Some(mi) = self.pets.iter().position(|p| p.id == mother) else {
            return false;
        };
        let Some(li) = self.pets.iter().position(|p| p.id == lamb) else {
            return false;
        };
        let (m, l) = (&self.pets[mi], &self.pets[li]);
        if !m.is_ewe()
            || !valid_lamb_age(l.age())
            || !l.sheep
            || m.def.rules != Rules::Mouflon
            || l.def.rules != Rules::Mouflon
            || matches!(m.def.appearance, Appearance::Sprites(_))
            || matches!(l.def.appearance, Appearance::Sprites(_))
            || l.def.physics.height >= m.def.physics.height
            || m.finished
            || l.finished
            || m.leaving.is_some()
            || l.leaving.is_some()
        {
            return false;
        }
        if let Some(existing) = l.family.mother {
            return existing == mother;
        }
        self.pets[li].family.mother = Some(mother);
        self.families.links.push(Link {
            mother,
            lamb,
            indices: (mi, li),
            due: 0.0,
            stage: Stage::Free,
        });
        self.families.generation = u64::MAX;
        true
    }

    /// Crée la définition réduite d'un jeune et l'ajoute auprès de sa mère.
    /// L'app prépare plutôt la définition et le dessin hors de son fil
    /// d'affichage, puis emploie [`Self::spawn_lamb_with`].
    pub fn spawn_lamb(&mut self, mother: u32, seed: u64, weeks: f32, world: &World) -> Option<u32> {
        let m = self.pets.iter().find(|p| p.id == mother && p.is_ewe())?;
        // Le dessin d'agneau est rangé avec l'espèce, pas avec celui de la mère.
        let def = Arc::new(m.species.as_lamb(weeks)?);
        self.spawn_lamb_with(mother, def, seed, world)
    }

    /// Ajoute un agneau dont la définition et le dessin sont déjà préparés.
    /// Il apparaît sur la surface de sa mère, sans tomber sur son dos.
    pub fn spawn_lamb_with(
        &mut self,
        mother: u32,
        def: Arc<PetDef>,
        seed: u64,
        world: &World,
    ) -> Option<u32> {
        let m = self.pets.iter().find(|p| p.id == mother && p.is_ewe())?;
        let Support::Ground { surface } = m.support else {
            return None;
        };
        let s = *world.surface(surface)?;
        if !m.can_have_lamb()
            || !valid_lamb_age(def.age)
            || def.meta.category() != "sheep"
            || def.rules != Rules::Mouflon
            || matches!(def.appearance, Appearance::Sprites(_))
            || def.physics.height >= m.def.physics.height
            || !family_motions(&def)
        {
            return None;
        }
        let margin = def.physics.width * 0.5;
        if s.x1 - s.x0 < 2.0 * margin {
            return None;
        }
        // Un rebord de la largeur exacte du jeune peut, arrondi, donner des
        // bornes croisées d'un rien : `clamp_between` ne panique pas.
        let x = clamp_between(
            m.pos.x + m.facing * (m.def.physics.width + def.physics.width) * 0.6,
            s.x0 + margin,
            s.x1 - margin,
        );
        let id = self.next_id;
        self.next_id += 1;
        let mut pet = Pet::spawn_at(id, def, seed, world, Vec2::new(x, s.y));
        pet.set_ground(&s);
        pet.family_stand();
        pet.anim.air = 0.0;
        pet.take_temper(self.mood);
        self.pets.push(pet);
        self.generation += 1;
        self.bind_mother(id, mother).then_some(id)
    }

    /// Ajoute un agneau déjà préparé qui arrive avec sa mère : il tombe du
    /// haut de l'écran derrière elle, où qu'elle soit (encore en l'air, tout
    /// juste arrivée), lié à elle. `None`, sans rien ajouter, si elle ne peut
    /// pas être sa mère.
    pub fn spawn_lamb_arriving(
        &mut self,
        mother: u32,
        def: Arc<PetDef>,
        seed: u64,
        world: &World,
    ) -> Option<u32> {
        let m = self.pets.iter().find(|p| p.id == mother)?;
        if !takes_lamb(m, &def) {
            return None;
        }
        // Derrière elle, dans son écran.
        let (left, right) = world.span(m.pos.x, m.pos.y - m.def.physics.height, m.pos.y);
        let margin = def.physics.width;
        let x = clamp_between(
            m.pos.x - m.facing * (m.def.physics.width + def.physics.width) * 0.6,
            left + margin,
            right - margin,
        );
        let id = self.next_id;
        self.next_id += 1;
        let mut pet = Pet::spawn_near(id, def, seed, world, m.pos, Some(x), None);
        pet.take_temper(self.mood);
        self.pets.push(pet);
        self.generation += 1;
        if self.bind_mother(id, mother) {
            Some(id)
        } else {
            self.remove(id);
            None
        }
    }

    /// Ajoute un agneau déjà préparé qui revient à sa place, lié à sa mère
    /// (le troupeau d'avant une mise à jour) : à `from` s'il est donné (dans
    /// la lumière de la soucoupe), sinon en tombant du haut de l'écran à
    /// l'abscisse `x`. `None`, sans rien ajouter, si elle ne peut pas être
    /// sa mère.
    pub fn spawn_lamb_back(
        &mut self,
        mother: u32,
        def: Arc<PetDef>,
        seed: u64,
        world: &World,
        x: f32,
        from: Option<Vec2>,
    ) -> Option<u32> {
        let m = self.pets.iter().find(|p| p.id == mother)?;
        if !takes_lamb(m, &def) {
            return None;
        }
        let id = match from {
            Some(at) => self.spawn_at(def, seed, world, at, None),
            None => self.spawn_as(def, seed, world, Some(x), None),
        };
        if self.bind_mother(id, mother) {
            Some(id)
        } else {
            self.remove(id);
            None
        }
    }

    pub(super) fn forget_family(&mut self, id: u32) {
        self.families
            .links
            .retain(|link| link.mother != id && link.lamb != id);
        for pet in &mut self.pets {
            if pet.family.mother == Some(id) {
                pet.family.mother = None;
                pet.family.stop();
            }
            if pet.id == id || pet.family.partner == Some(id) {
                pet.family.stop();
            }
        }
    }

    pub(super) fn step_families(&mut self, dt: f32, world: &World) {
        if self.families.links.is_empty() {
            return;
        }
        let mut families = std::mem::take(&mut self.families);
        // `pets_mut` peut réordonner les animaux : les identifiants vérifient
        // le cache, même si aucune apparition n'a changé la génération.
        let stale = families.generation != self.generation
            || families.links.iter().any(|l| {
                self.pets.get(l.indices.0).map(|p| p.id) != Some(l.mother)
                    || self.pets.get(l.indices.1).map(|p| p.id) != Some(l.lamb)
            });
        if stale {
            families.indices.clear();
            families
                .indices
                .extend(self.pets.iter().enumerate().map(|(i, p)| (p.id, i)));
            families.links.retain_mut(|link| {
                match (
                    families.indices.get(&link.mother),
                    families.indices.get(&link.lamb),
                ) {
                    (Some(&mi), Some(&li)) => {
                        link.indices = (mi, li);
                        true
                    }
                    _ => false,
                }
            });
            for pet in &mut self.pets {
                if pet
                    .family
                    .mother
                    .is_some_and(|id| !families.indices.contains_key(&id))
                {
                    pet.family.mother = None;
                    pet.family.stop();
                }
                if pet
                    .family
                    .partner
                    .is_some_and(|id| !families.indices.contains_key(&id))
                {
                    pet.family.stop();
                }
            }
            families.generation = self.generation;
        }
        for link in &mut families.links {
            if self.still == Some(link.mother) || self.still == Some(link.lamb) {
                continue;
            }
            let (m, l) = self.pair_mut(link.indices.0 as u32, link.indices.1 as u32);
            step_link(link, m, l, dt, world);
        }
        self.families = families;
    }
}

fn surface_pair<'a>(m: &Pet, l: &Pet, world: &'a World) -> Option<&'a Surface> {
    let (Support::Ground { surface: ms }, Support::Ground { surface: ls }) = (m.support, l.support)
    else {
        return None;
    };
    if ms != ls || (m.pos.y - l.pos.y).abs() > 0.2 {
        return None;
    }
    world.surface(ms)
}

fn nursing_place(m: &Pet, l: &Pet, surface: &Surface, world: &World) -> Option<(f32, Vec2)> {
    let (udder, _) = m.nursing_points();
    let (_, muzzle) = l.nursing_points();
    let facing = -m.facing;
    let udder = m.pos + Vec2::new(udder.x * m.facing, udder.y) * m.def.physics.height;
    let goal = udder.x - facing * muzzle.x * l.def.physics.height;
    let half = l.def.physics.width * 0.5;
    let (left, right) = world.span(l.pos.x, l.pos.y - l.def.physics.height, l.pos.y);
    if goal - half < surface.x0.max(left) || goal + half > surface.x1.min(right) {
        return None;
    }
    let target =
        Vec2::new((udder.x - goal) * facing, udder.y - l.pos.y) * (1.0 / l.def.physics.height);
    // Trop haut ou trop loin : aucun étirement irréaliste pour inventer le
    // contact. Les points précis du dessin remplacent ces bornes prudentes.
    if !(0.1..1.1).contains(&-target.y) || !(0.05..1.2).contains(&target.x) {
        return None;
    }
    Some((goal, target))
}

fn stop_link(link: &mut Link, m: &mut Pet, l: &mut Pet, pause: f32) {
    if m.family.partner == Some(l.id) {
        m.family.stop();
    }
    if l.family.mother == Some(m.id) {
        l.family.stop();
    }
    link.stage = Stage::Free;
    link.due = pause;
}

fn step_link(link: &mut Link, m: &mut Pet, l: &mut Pet, dt: f32, world: &World) {
    let Age::Lamb { weeks } = l.age() else {
        stop_link(link, m, l, 60.0);
        return;
    };
    let (duration, interval) = nursing_schedule(weeks);
    link.due = (link.due - dt).max(0.0);
    let Some(surface) = surface_pair(m, l, world) else {
        stop_link(link, m, l, link.due.max(5.0));
        return;
    };
    if !m.family_available() || !l.family_available() || !m.is_ewe() {
        stop_link(link, m, l, link.due.max(5.0));
        return;
    }
    match link.stage {
        Stage::Free => {
            if m.family.action.active() && m.family.partner != Some(l.id) {
                return;
            }
            let distance = (m.pos.x - l.pos.x).abs();
            if link.due > 0.0
                || weeks > 7.0
                || m.vel.x.abs() > 0.1
                || distance > m.def.physics.width * 4.0
            {
                // Suivre sa mère, sans quitter le rebord ni chasser un autre
                // comportement social déjà en cours.
                if distance > m.def.physics.width * 2.0 && !l.family.action.active() {
                    let side = (l.pos.x - m.pos.x).signum();
                    let half = l.def.physics.width * 0.5;
                    if surface.x1 - surface.x0 < 2.0 * half {
                        return;
                    }
                    let goal = clamp_between(
                        m.pos.x + side * m.def.physics.width * 1.25,
                        surface.x0 + half,
                        surface.x1 - half,
                    );
                    l.family_walk(goal, false);
                } else if let FamilyAction::Follow { goal } = l.family.action
                    && (goal - l.pos.x).abs() < 0.05
                    && l.vel.x.abs() < 0.1
                {
                    l.family_stand();
                }
                return;
            }
            if distance > m.def.physics.width * 4.0 {
                return;
            }
            let Some((goal, _)) = nursing_place(m, l, surface, world) else {
                return;
            };
            if !m.family_stand() || !l.family_walk(goal, true) {
                return;
            }
            m.family.partner = Some(l.id);
            l.family.partner = Some(m.id);
            m.family.action = FamilyAction::Wait { elapsed: 0.0 };
            m.events.push(PetEvent::MaternalCall);
            link.stage = Stage::Approach {
                elapsed: 0.0,
                facing: m.facing,
                mother_x: m.pos.x,
            };
        }
        Stage::Approach {
            elapsed,
            facing,
            mother_x,
        } => {
            if !family_pair(m, l)
                || (m.pos.x - mother_x).abs() > 0.2
                || m.facing != facing
                || elapsed > 15.0
            {
                stop_link(link, m, l, 5.0);
                return;
            }
            let Some((goal, target)) = nursing_place(m, l, surface, world) else {
                stop_link(link, m, l, 5.0);
                return;
            };
            m.family.action = FamilyAction::Wait {
                elapsed: elapsed + dt,
            };
            if (l.pos.x - goal).abs() < 0.05 && l.vel.x.abs() < 0.1 {
                l.facing = -m.facing;
                if l.anim.walk < 0.01 && (l.anim.flip - l.facing).abs() < 0.01 {
                    l.family_stand();
                    l.family.partner = Some(m.id);
                    l.family.action = FamilyAction::Nurse {
                        elapsed: 0.0,
                        target,
                    };
                    link.stage = Stage::Nurse {
                        elapsed: 0.0,
                        facing,
                        mother_x,
                    };
                    return;
                }
            }
            link.stage = Stage::Approach {
                elapsed: elapsed + dt,
                facing,
                mother_x,
            };
        }
        Stage::Nurse {
            elapsed,
            facing,
            mother_x,
        } => {
            if !family_pair(m, l)
                || !l.is_nursing()
                || (m.pos.x - mother_x).abs() > 0.2
                || m.facing != facing
                || l.facing != -facing
                || l.vel.x.abs() > 0.1
            {
                stop_link(link, m, l, 5.0);
                return;
            }
            if elapsed + dt >= duration {
                stop_link(link, m, l, (interval - duration).max(0.0));
                return;
            }
            if let FamilyAction::Nurse { target, .. } = l.family.action {
                l.family.action = FamilyAction::Nurse {
                    elapsed: elapsed + dt,
                    target,
                };
            }
            m.family.action = FamilyAction::Wait {
                elapsed: 1.0 + elapsed,
            };
            link.stage = Stage::Nurse {
                elapsed: elapsed + dt,
                facing,
                mother_x,
            };
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{Input, Rect, Screen, SexRatio, TICK, parse_pet_toml};

    fn definition() -> PetDef {
        parse_pet_toml(
            r#"
            [pet]
            id = "family"
            name = "Mouton"
            category = "sheep"
            sex = "female"
            [appearance]
            rig = "sheep.svg"
            height = 60.0
            [physics]
            width = 64.0
            bounce = 0.0
            [reflexes]
            spawn = "fall"
            fall = "fall"
            land = "stand"
            [[behavior]]
            name = "stand"
            motion = "stand"
            duration = "forever"
            [[behavior]]
            name = "walk"
            motion = "walk"
            speed = 50.0
            duration = "forever"
            [[behavior]]
            name = "fall"
            motion = "fall"
            duration = "forever"
            [[behavior]]
            name = "flehmen"
            motion = "flehmen"
            duration = 1.0
            next = "stand"
            [[behavior]]
            name = "bleat"
            motion = "bleat"
            duration = 1.0
            next = "stand"
            [[behavior]]
            name = "rest"
            motion = "sit"
            duration = "forever"
        "#,
            |_: &str| Ok(b"<svg/>".to_vec()),
        )
        .unwrap()
    }

    fn world() -> World {
        World::new(vec![Screen {
            frame: Rect::new(0.0, 0.0, 1000.0, 600.0),
            usable: Rect::new(0.0, 25.0, 1000.0, 550.0),
            scale: 2.0,
        }])
    }

    fn plant(flock: &mut Flock, id: u32, world: &World, x: f32) {
        let surface = *world.floor_under(x).unwrap();
        let pet = flock.get_mut(id).unwrap();
        pet.pos = Vec2::new(x, surface.y);
        pet.prev_pos = pet.pos;
        pet.vel = Vec2::ZERO;
        pet.set_ground(&surface);
        pet.family_stand();
        pet.anim.air = 0.0;
    }

    fn family(facing: f32, realistic: bool, collisions: bool) -> (Flock, World, u32, u32) {
        let world = world();
        let mut flock = Flock::new();
        flock.set_realistic(realistic);
        flock.set_collisions(collisions);
        let mother = flock.spawn(Arc::new(definition()), 42, &world, Some(450.0));
        plant(&mut flock, mother, &world, 450.0);
        flock.get_mut(mother).unwrap().face(facing);
        let lamb = flock.spawn_lamb(mother, 43, 2.0, &world).unwrap();
        (flock, world, mother, lamb)
    }

    fn tick(flock: &mut Flock, world: &World) {
        flock.advance(
            TICK,
            world,
            &Input {
                local_hour: 12.0,
                ..Input::default()
            },
        );
    }

    fn await_nursing(flock: &mut Flock, world: &World, lamb: u32) {
        for _ in 0..2400 {
            tick(flock, world);
            if flock.get_mut(lamb).unwrap().is_nursing() {
                return;
            }
        }
        let l = flock.get_mut(lamb).unwrap();
        panic!(
            "no nursing: position {:?}, velocity {:?}, action {:?}",
            l.pos, l.vel, l.family.action
        );
    }

    #[test]
    fn nursing_uses_the_published_means_without_treating_error_bars_as_ranges() {
        assert_eq!(nursing_schedule(1.0), (41.1, 2400.0));
        assert_eq!(nursing_schedule(2.0), (41.1, 2400.0));
        for weeks in [6.0, 7.0] {
            let (duration, interval) = nursing_schedule(weeks);
            assert!((duration - 11.9).abs() < 1e-5);
            assert!((interval - 86_400.0 / 14.3).abs() < 1e-3);
        }
    }

    #[test]
    fn only_an_adult_ewe_can_be_bound_to_a_smaller_native_lamb() {
        let (mut flock, world, mother, lamb) = family(1.0, false, false);
        assert!(flock.bind_mother(lamb, mother));
        assert!(!flock.bind_mother(mother, lamb));
        assert!(!flock.bind_mother(lamb, lamb));
        assert!(!flock.bind_mother(lamb, 999));
        let mut ram = definition();
        ram.sex_ratio = SexRatio::MALE;
        let ram = flock.spawn(Arc::new(ram), 44, &world, Some(300.0));
        assert!(!flock.bind_mother(lamb, ram));
        assert!(flock.spawn_lamb(ram, 45, 2.0, &world).is_none());
        assert!(definition().as_lamb(f32::NAN).is_none());
        assert!(definition().as_lamb(f32::INFINITY).is_none());
        assert!(definition().as_lamb(0.0).is_none());
        assert!(definition().as_lamb(-1.0).is_none());
        assert!(definition().as_lamb(53.0).is_none());
    }

    #[test]
    fn a_lamb_walks_under_its_mother_then_nurses_facing_the_other_way_without_sliding() {
        for realistic in [false, true] {
            for collisions in [false, true] {
                for facing in [-1.0, 1.0] {
                    let (mut flock, world, mother, lamb) = family(facing, realistic, collisions);
                    let start = flock.get_mut(lamb).unwrap().pos;
                    let mut moved = false;
                    for _ in 0..2400 {
                        let before = flock.get_mut(lamb).unwrap().pos;
                        tick(&mut flock, &world);
                        let l = flock.get_mut(lamb).unwrap();
                        assert!(
                            (l.pos.x - before.x).abs() <= 0.3,
                            "one step {:?} -> {:?}",
                            before,
                            l.pos
                        );
                        moved |= l.pos != start;
                        if l.is_nursing() {
                            break;
                        }
                    }
                    assert!(moved);
                    assert!(
                        flock.get_mut(lamb).unwrap().is_nursing(),
                        "real={realistic}, collisions={collisions}, facing={facing}"
                    );
                    let fixed = flock.get_mut(lamb).unwrap().pos;
                    for _ in 0..240 {
                        tick(&mut flock, &world);
                        let l = flock.get_mut(lamb).unwrap();
                        assert_eq!(l.pos, fixed);
                        assert_eq!(l.vel, Vec2::ZERO);
                        assert_eq!(l.facing, -facing);
                        assert_eq!(l.mother(), Some(mother));
                        assert!(l.view(1.0).pose.nursing_target.is_some());
                    }
                }
            }
        }
    }

    #[test]
    fn a_maternal_call_happens_once_and_keeps_the_mouth_closed() {
        let (mut flock, world, mother, lamb) = family(1.0, false, false);
        let mut calls = 0;
        for _ in 0..2400 {
            tick(&mut flock, &world);
            let m = flock.get_mut(mother).unwrap();
            calls += m
                .events
                .iter()
                .filter(|e| matches!(e, PetEvent::MaternalCall))
                .count();
            m.events.clear();
            if m.view(1.0).pose.gesture == Some(Gesture::MaternalCall) {
                assert_eq!(m.view(1.0).pose.jaw, 0.0);
            }
            if flock.get_mut(lamb).unwrap().is_nursing() {
                break;
            }
        }
        assert_eq!(calls, 1);
    }

    #[test]
    fn nursing_ends_after_the_published_duration_and_does_not_repeat_immediately() {
        let (mut flock, world, _, lamb) = family(1.0, false, false);
        await_nursing(&mut flock, &world, lamb);
        let mut elapsed = 0.0;
        while flock.get_mut(lamb).unwrap().is_nursing() {
            tick(&mut flock, &world);
            elapsed += TICK;
            assert!(elapsed < 42.0);
        }
        assert!((elapsed - 41.1).abs() < 0.05, "{elapsed}");
        for _ in 0..3600 {
            tick(&mut flock, &world);
            assert!(!flock.get_mut(lamb).unwrap().is_nursing());
        }
    }

    #[test]
    fn the_nursing_target_survives_both_completion_and_a_grounded_interruption_until_fade_out() {
        for complete in [true, false] {
            let (mut flock, world, mother, lamb) = family(1.0, false, false);
            await_nursing(&mut flock, &world, lamb);
            for _ in 0..120 {
                tick(&mut flock, &world);
            }
            let before = flock.get_mut(lamb).unwrap().view(1.0).pose;
            assert_eq!(before.gesture_amount, 1.0);
            assert!(before.nursing_target.is_some());
            if complete {
                while flock.get_mut(lamb).unwrap().is_nursing() {
                    tick(&mut flock, &world);
                }
            } else {
                flock.remove(mother);
                let pet = flock.get_mut(lamb).unwrap();
                assert!(!pet.is_nursing());
                assert_eq!(pet.view(1.0).pose.nursing_target, before.nursing_target);
                tick(&mut flock, &world);
            }
            let mut previous = before.gesture_amount;
            let mut faded = false;
            for _ in 0..120 {
                let pet = flock.get_mut(lamb).unwrap();
                let pose = pet.view(1.0).pose;
                if pose.gesture != Some(Gesture::Nurse) {
                    assert_eq!(pose.gesture_amount, 0.0);
                    assert_eq!(pose.nursing_target, None);
                    assert_eq!(pet.family.fading_target, None);
                    faded = true;
                    break;
                }
                assert_eq!(pose.nursing_target, before.nursing_target);
                assert!(pose.gesture_amount < previous);
                assert!(previous - pose.gesture_amount < 0.12);
                previous = pose.gesture_amount;
                tick(&mut flock, &world);
            }
            assert!(faded);
        }
    }

    #[test]
    fn a_nursing_target_is_discarded_when_the_lamb_leaves_the_ground_or_changes_gesture() {
        for airborne in [true, false] {
            let (mut flock, world, _, lamb) = family(1.0, false, false);
            await_nursing(&mut flock, &world, lamb);
            for _ in 0..120 {
                tick(&mut flock, &world);
            }
            let pet = flock.get_mut(lamb).unwrap();
            assert!(pet.view(1.0).pose.nursing_target.is_some());
            if airborne {
                pet.grab(pet.pos, &world);
                assert_eq!(pet.view(1.0).pose.nursing_target, None);
            } else {
                assert!(pet.force("bleat"));
            }
            tick(&mut flock, &world);
            let pet = flock.get_mut(lamb).unwrap();
            assert_eq!(pet.view(1.0).pose.nursing_target, None);
            assert_eq!(pet.family.fading_target, None);
            if !airborne {
                assert_eq!(pet.view(1.0).pose.gesture, Some(Gesture::Bleat));
            }
        }
    }

    #[test]
    fn family_links_survive_vector_reordering_and_are_removed_with_the_mother() {
        let (mut flock, world, mother, lamb) = family(1.0, false, false);
        await_nursing(&mut flock, &world, lamb);
        flock.pets_mut().reverse();
        tick(&mut flock, &world);
        assert!(flock.get_mut(lamb).unwrap().is_nursing());
        assert_eq!(flock.get_mut(lamb).unwrap().mother(), Some(mother));
        flock.remove(mother);
        let l = flock.get_mut(lamb).unwrap();
        assert_eq!(l.mother(), None);
        assert!(!l.is_nursing());
        tick(&mut flock, &world);
        assert!(flock.families.links.is_empty());
    }

    #[test]
    fn picking_up_either_partner_interrupts_nursing_without_relocating_the_other() {
        for pick_mother in [false, true] {
            let (mut flock, world, mother, lamb) = family(1.0, false, false);
            await_nursing(&mut flock, &world, lamb);
            let picked = if pick_mother { mother } else { lamb };
            let other = if pick_mother { lamb } else { mother };
            let before = flock.get_mut(other).unwrap().pos;
            let p = flock.get_mut(picked).unwrap();
            p.grab(p.pos, &world);
            tick(&mut flock, &world);
            assert!(!flock.get_mut(lamb).unwrap().is_nursing());
            assert_eq!(flock.get_mut(other).unwrap().pos, before);
            assert_eq!(flock.get_mut(lamb).unwrap().mother(), Some(mother));
        }
    }

    #[test]
    fn the_lamb_follows_its_own_mother_without_switching_to_a_closer_ewe() {
        let (mut flock, world, mother, lamb) = family(1.0, false, false);
        let stranger = flock.spawn(Arc::new(definition()), 99, &world, Some(150.0));
        plant(&mut flock, stranger, &world, 150.0);
        plant(&mut flock, lamb, &world, 100.0);
        let start = flock.get_mut(lamb).unwrap().pos.x;
        for _ in 0..480 {
            tick(&mut flock, &world);
        }
        assert!(flock.get_mut(lamb).unwrap().pos.x > start + 50.0);
        assert_eq!(flock.get_mut(lamb).unwrap().mother(), Some(mother));
        assert!(!flock.bind_mother(lamb, stranger));
    }

    #[test]
    fn family_simulation_is_identical_for_the_same_seed() {
        let (mut a, world, _, _) = family(-1.0, true, true);
        let mut b = a.clone();
        for _ in 0..7200 {
            tick(&mut a, &world);
            tick(&mut b, &world);
            for (p, q) in a.pets().iter().zip(b.pets()) {
                assert_eq!(p.pos, q.pos);
                assert_eq!(p.vel, q.vel);
                assert_eq!(p.view(1.0).pose, q.view(1.0).pose);
            }
        }
    }

    #[test]
    fn removing_the_mother_during_following_does_not_leave_a_suspended_behavior() {
        let (mut flock, world, mother, lamb) = family(1.0, false, false);
        plant(&mut flock, lamb, &world, 100.0);
        tick(&mut flock, &world);
        assert!(matches!(
            flock.get_mut(lamb).unwrap().family.action,
            FamilyAction::Follow { .. }
        ));
        flock.remove(mother);
        let l = flock.get_mut(lamb).unwrap();
        assert_eq!(l.mother(), None);
        assert!(!l.family.action.active());
        let before = l.behavior_time;
        tick(&mut flock, &world);
        assert!(flock.get_mut(lamb).unwrap().behavior_time > before);
    }

    #[test]
    fn a_lamb_never_nurses_across_different_surfaces_or_follows_a_stale_goal() {
        use crate::{LedgeOptions, WindowRect};
        let (mut flock, mut world, mother, lamb) = family(1.0, false, false);
        plant(&mut flock, lamb, &world, 100.0);
        tick(&mut flock, &world);
        world.set_windows(
            &[WindowRect {
                id: 70,
                frame: Rect::new(350.0, 300.0, 300.0, 150.0),
            }],
            LedgeOptions::default(),
        );
        let surface = *world.surfaces().iter().find(|s| s.y == 300.0).unwrap();
        let m = flock.get_mut(mother).unwrap();
        m.pos = Vec2::new(450.0, 300.0);
        m.prev_pos = m.pos;
        m.set_ground(&surface);
        tick(&mut flock, &world);
        assert!(!flock.get_mut(lamb).unwrap().family.action.active());
        for _ in 0..600 {
            tick(&mut flock, &world);
            assert!(!flock.get_mut(lamb).unwrap().is_nursing());
        }
        assert_eq!(flock.get_mut(lamb).unwrap().mother(), Some(mother));
    }

    #[test]
    fn a_disappearing_ledge_interrupts_nursing_and_both_animals_fall() {
        use crate::{LedgeOptions, WindowRect};
        let (mut flock, mut world, mother, lamb) = family(1.0, false, false);
        world.set_windows(
            &[WindowRect {
                id: 70,
                frame: Rect::new(250.0, 300.0, 450.0, 150.0),
            }],
            LedgeOptions::default(),
        );
        let surface = *world.surfaces().iter().find(|s| s.y == 300.0).unwrap();
        for id in [mother, lamb] {
            let p = flock.get_mut(id).unwrap();
            p.pos.y = 300.0;
            p.prev_pos = p.pos;
            p.set_ground(&surface);
        }
        await_nursing(&mut flock, &world, lamb);
        world.set_windows(&[], LedgeOptions::default());
        tick(&mut flock, &world);
        assert!(!flock.get_mut(lamb).unwrap().is_nursing());
        assert_eq!(flock.get_mut(lamb).unwrap().support, Support::Air);
        assert_eq!(flock.get_mut(mother).unwrap().support, Support::Air);
    }

    #[test]
    fn a_ledge_narrower_than_the_lamb_never_causes_an_invalid_clamp() {
        use crate::{LedgeOptions, WindowRect};
        let (mut flock, mut world, mother, lamb) = family(1.0, false, false);
        world.set_windows(
            &[WindowRect {
                id: 70,
                frame: Rect::new(440.0, 300.0, 15.0, 150.0),
            }],
            LedgeOptions {
                min_span: 1.0,
                headroom: 24.0,
            },
        );
        let surface = *world.surfaces().iter().find(|s| s.y == 300.0).unwrap();
        for (id, x) in [(mother, 450.0), (lamb, 100.0)] {
            let p = flock.get_mut(id).unwrap();
            p.pos = Vec2::new(x, 300.0);
            p.prev_pos = p.pos;
            p.set_ground(&surface);
        }
        flock.step_families(TICK, &world);
        assert!(!flock.get_mut(lamb).unwrap().family.action.active());
        assert!(flock.spawn_lamb(mother, 99, 2.0, &world).is_none());
    }

    #[test]
    fn a_failed_lamb_request_does_not_add_an_orphan_or_consume_an_identifier() {
        let (mut flock, world, mother, _) = family(1.0, false, false);
        let count = flock.pets.len();
        let next = flock.next_id;
        let generation = flock.generation;
        let mut invalid = definition().as_lamb(2.0).unwrap();
        invalid.age = Age::Lamb { weeks: f32::NAN };
        assert!(
            flock
                .spawn_lamb_with(mother, Arc::new(invalid), 88, &world)
                .is_none()
        );
        assert!(
            flock
                .spawn_lamb_with(mother, Arc::new(definition()), 89, &world)
                .is_none()
        );
        assert!(
            flock
                .spawn_lamb_with(
                    999,
                    Arc::new(definition().as_lamb(2.0).unwrap()),
                    90,
                    &world
                )
                .is_none()
        );
        assert_eq!(flock.pets.len(), count);
        assert_eq!(flock.next_id, next);
        assert_eq!(flock.generation, generation);
    }

    #[test]
    fn a_desktoppet_ewe_cannot_receive_a_native_lamb_or_mutate_the_flock() {
        let world = world();
        let mut flock = Flock::new();
        let mut original = definition();
        original.rules = Rules::DesktopPet;
        let mother = flock.spawn(Arc::new(original), 17, &world, Some(450.0));
        plant(&mut flock, mother, &world, 450.0);
        assert!(!flock.get_mut(mother).unwrap().can_have_lamb());
        let before = (flock.pets.len(), flock.next_id, flock.generation);
        assert!(
            flock
                .spawn_lamb_with(
                    mother,
                    Arc::new(definition().as_lamb(2.0).unwrap()),
                    18,
                    &world
                )
                .is_none()
        );
        assert_eq!((flock.pets.len(), flock.next_id, flock.generation), before);
    }

    #[test]
    fn a_lamb_without_a_walking_behavior_is_rejected_before_spawning() {
        let (mut flock, world, mother, _) = family(1.0, false, false);
        let mut def = definition().as_lamb(2.0).unwrap();
        let walk = def.behaviors.iter_mut().find(|b| b.name == "walk").unwrap();
        walk.motion = Motion::Look;
        let before = (flock.pets.len(), flock.next_id, flock.generation);
        assert!(
            flock
                .spawn_lamb_with(mother, Arc::new(def), 99, &world)
                .is_none()
        );
        assert_eq!((flock.pets.len(), flock.next_id, flock.generation), before);
    }

    #[test]
    fn regrouping_never_reserves_a_nursing_pair() {
        let (mut flock, world, mother, lamb) = family(1.0, false, false);
        let outsider = flock.spawn(Arc::new(definition()), 44, &world, Some(100.0));
        plant(&mut flock, outsider, &world, 100.0);
        await_nursing(&mut flock, &world, lamb);
        assert!(!flock.regroup());
        for id in [mother, lamb] {
            let pet = flock.get_mut(id).unwrap();
            assert!(pet.family.action.active());
            assert!(pet.regroup.is_none());
        }
        tick(&mut flock, &world);
        assert!(flock.get_mut(lamb).unwrap().is_nursing());
    }

    #[test]
    fn opening_a_partner_menu_pauses_the_rendezvous_without_moving_the_lamb() {
        for menu_on_mother in [false, true] {
            let (mut flock, world, mother, lamb) = family(1.0, false, false);
            await_nursing(&mut flock, &world, lamb);
            let fixed = flock.get_mut(lamb).unwrap().pos;
            flock.hold_still(Some(if menu_on_mother { mother } else { lamb }));
            for _ in 0..120 {
                tick(&mut flock, &world);
                assert_eq!(flock.get_mut(lamb).unwrap().pos, fixed);
                assert!(flock.get_mut(lamb).unwrap().is_nursing());
            }
            flock.hold_still(None);
            tick(&mut flock, &world);
            assert!(flock.get_mut(lamb).unwrap().is_nursing());
        }
    }

    #[test]
    fn a_lamb_is_ram_or_ewe_whatever_sex_was_fixed_for_its_mothers_drawing() {
        // Le dessin de la mère est une brebis (`sex = "female"`) : ses
        // agneaux naissent mâles ou femelles, à chances égales.
        let young = Arc::new(definition().as_lamb(2.0).unwrap());
        assert_eq!(young.sex_ratio, SexRatio::default());
        let world = world();
        let sexes: Vec<_> = (0..64)
            .map(|seed| Pet::spawn(seed, Arc::clone(&young), u64::from(seed), &world, None).sex())
            .collect();
        assert!(sexes.contains(&crate::Sex::Male) && sexes.contains(&crate::Sex::Female));
    }

    #[test]
    fn a_resting_ewe_still_receives_her_prepared_lamb_and_nurses_once_she_stands() {
        // Le dessin du jeune se prépare sur un autre fil : la mère a pu se
        // coucher entre-temps. Elle le reçoit quand même ; la tétée attend
        // qu'elle soit debout.
        let world = world();
        let mut flock = Flock::new();
        let mother = flock.spawn(Arc::new(definition()), 42, &world, Some(450.0));
        plant(&mut flock, mother, &world, 450.0);
        assert!(flock.get_mut(mother).unwrap().force("rest"));
        for _ in 0..120 {
            tick(&mut flock, &world);
        }
        assert!(flock.get_mut(mother).unwrap().can_have_lamb());
        let young = Arc::new(definition().as_lamb(2.0).unwrap());
        let lamb = flock
            .spawn_lamb_with(mother, young, 43, &world)
            .expect("une brebis couchée reçoit son agneau");
        for _ in 0..1200 {
            tick(&mut flock, &world);
            assert!(!flock.get_mut(lamb).unwrap().is_nursing());
        }
        assert_eq!(flock.get_mut(mother).unwrap().behavior_name(), "rest");
        assert!(flock.get_mut(mother).unwrap().force("stand"));
        await_nursing(&mut flock, &world, lamb);
    }

    #[test]
    fn only_a_drawn_native_ewe_can_be_a_mother() {
        // Une brebis en images (planche, desktopPet) n'a pas de tétée :
        // son menu ne propose pas d'agneau.
        let world = world();
        let mut flock = Flock::new();
        let mut sprites = definition();
        sprites.appearance = Appearance::Sprites(crate::def::SpriteSheet::single(
            crate::desktoppet::tests::png_header(32, 16).into(),
            2,
            1,
        ));
        let mut desktoppet = definition();
        desktoppet.rules = Rules::DesktopPet;
        let mut ram = definition();
        ram.sex_ratio = SexRatio::MALE;
        for (def, mother) in [
            (definition(), true),
            (sprites, false),
            (desktoppet, false),
            (ram, false),
        ] {
            let female = def.sex_ratio == SexRatio::FEMALE;
            let id = flock.spawn(Arc::new(def), 42, &world, Some(450.0));
            plant(&mut flock, id, &world, 450.0);
            let pet = flock.get_mut(id).unwrap();
            assert_eq!(pet.is_ewe(), female);
            assert_eq!(pet.can_mother(), mother);
            assert_eq!(pet.can_have_lamb(), mother);
        }
    }

    #[test]
    fn a_mother_and_her_lamb_still_overlapping_after_nursing_part_without_a_collision() {
        // Après la tétée, l'agneau est encore sous le flanc de sa mère ; en
        // mode collisions, celui qui repart le premier ne doit pas les
        // envoyer tous deux à la renverse.
        for realistic in [false, true] {
            for mother_leaves in [true, false] {
                let (mut flock, world, mother, lamb) = family(1.0, realistic, true);
                await_nursing(&mut flock, &world, lamb);
                while flock.get_mut(lamb).unwrap().is_nursing() {
                    tick(&mut flock, &world);
                }
                for id in [mother, lamb] {
                    flock.get_mut(id).unwrap().take_events();
                }
                let leaving = if mother_leaves { mother } else { lamb };
                assert!(flock.get_mut(leaving).unwrap().force("walk"));
                for _ in 0..240 {
                    tick(&mut flock, &world);
                    for id in [mother, lamb] {
                        let bumped = flock
                            .get_mut(id)
                            .unwrap()
                            .take_events()
                            .into_iter()
                            .any(|e| matches!(e, PetEvent::Bumped { .. }));
                        assert!(
                            !bumped,
                            "réaliste = {realistic}, la mère part = {mother_leaves} : n° {id} heurté"
                        );
                    }
                }
            }
        }
    }

    #[test]
    fn refusing_flehmen_on_either_partner_leaves_the_nursing_pair_intact() {
        let (mut flock, world, mother, lamb) = family(1.0, false, false);
        await_nursing(&mut flock, &world, lamb);
        for (id, partner) in [(mother, lamb), (lamb, mother)] {
            let pet = flock.get_mut(id).unwrap();
            assert!(!pet.force("flehmen"));
            assert!(pet.family.action.active());
            assert_eq!(pet.family.partner, Some(partner));
        }
        tick(&mut flock, &world);
        assert!(flock.get_mut(lamb).unwrap().is_nursing());
    }
}
