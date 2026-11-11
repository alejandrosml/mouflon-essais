//! Sexe, âge et contexte des rencontres : aucun tirage supplémentaire dans
//! la suite des mouvements ordinaires.

use super::*;
use crate::def::{Age, Sex};

/// Un flairage de brebis ne peut déclencher qu'un flehmen, dans ce délai.
/// C'est une mémoire courte du moteur, pas une mesure de persistance d'odeur.
const SNIFF_MEMORY: f32 = 15.0;

impl Pet {
    /// Son sexe, tiré une fois à l'apparition et conservé ensuite.
    #[must_use]
    pub fn sex(&self) -> Sex {
        self.sex
    }

    /// Son âge déclaré : il ne vieillit pas avec le temps passé au bureau.
    #[must_use]
    pub fn age(&self) -> Age {
        self.def.age
    }

    /// Un mâle adulte de la famille des moutons.
    #[must_use]
    pub fn is_ram(&self) -> bool {
        self.sheep && self.sex == Sex::Male && self.age() == Age::Adult
    }

    /// Une femelle adulte de la famille des moutons.
    #[must_use]
    pub fn is_ewe(&self) -> bool {
        self.sheep && self.sex == Sex::Female && self.age() == Age::Adult
    }

    #[must_use]
    pub fn is_lamb(&self) -> bool {
        matches!(self.age(), Age::Lamb { .. })
    }

    pub(super) fn step_social(&mut self, dt: f32) {
        if self.sniffed_ewe_for > 0.0 {
            self.sniffed_ewe_for = (self.sniffed_ewe_for - dt).max(0.0);
        }
    }

    /// Tous les chemins d'entrée (choix, imitation, réflexe, outil) passent
    /// par cette garde. Voir une brebis sans la flairer ne suffit pas.
    pub(super) fn behavior_allowed(&self, index: usize) -> bool {
        // Un saut sur un rebord : seulement vers un rebord à sa portée.
        if let Motion::Leap { reach } = self.def.behaviors[index].motion {
            return self.leap_allowed(reach);
        }
        self.def.behaviors[index].motion != Motion::Gesture(Gesture::Flehmen)
            || (self.is_ram()
                && self.sniffed_ewe_for > 0.0
                && self.def.behaviors[self.behavior].motion == Motion::Gesture(Gesture::Sniff)
                && matches!(self.support, Support::Ground { .. }))
    }

    pub(super) fn transition_allowed(
        &self,
        transition: &Transition,
        world: &World,
        input: &Input,
    ) -> bool {
        self.behavior_allowed(transition.to)
            && transition
                .when
                .iter()
                .all(|c| self.condition(*c, world, input))
    }

    /// Appelé seulement après une réaction réelle de rencontre.
    pub(super) fn remember_sniff(&mut self, other: &Pet) {
        if self.is_ram()
            && other.is_ewe()
            && self.def.behaviors[self.behavior].motion == Motion::Gesture(Gesture::Sniff)
        {
            self.sniffed_ewe_for = SNIFF_MEMORY;
        }
    }

    /// Les jeux des agneaux restent possibles ; chez les adultes, les
    /// défis sont surtout une affaire de béliers. Ce sont des poids de jeu,
    /// non des fréquences éthologiques mesurées.
    pub(super) fn contest_appetite(&self, other: Option<&Pet>) -> f32 {
        if !self.sheep || self.def.rules == Rules::DesktopPet {
            return 1.0;
        }
        match other {
            Some(other) if self.is_lamb() && other.is_lamb() => 1.0,
            Some(other) if self.is_ram() && other.is_ram() => 3.0,
            Some(_) => 0.25,
            None if self.is_ewe() => 0.25,
            None => 1.0,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{Rect, Screen, SexRatio, parse_pet_toml};

    fn definition(sex: Sex, age: Age) -> Arc<PetDef> {
        let mut def = parse_pet_toml(
            r#"
            [pet]
            id = "sheep"
            name = "Mouton"
            category = "sheep"
            [appearance]
            rig = "mouton.svg"
            height = 50.0
            [[behavior]]
            name = "stand"
            motion = "stand"
            duration = "forever"
            on_meet = "sniff"
            [[behavior]]
            name = "sniff"
            motion = "sniff"
            duration = 0.2
            next = "flehmen"
            [[behavior]]
            name = "flehmen"
            motion = "flehmen"
            duration = 1.0
            contagious = 1.0
            next = "stand"
            [[behavior]]
            name = "back"
            motion = "back"
            duration = 0.3
            next = "charge"
            [[behavior]]
            name = "charge"
            motion = "charge"
            duration = 0.3
            next = "stand"
            "#,
            |_: &str| Ok(b"<svg/>".to_vec()),
        )
        .unwrap();
        def.sex_ratio = if sex == Sex::Male {
            SexRatio::MALE
        } else {
            SexRatio::FEMALE
        };
        def.age = age;
        Arc::new(def)
    }

    fn world() -> World {
        World::new(vec![Screen {
            frame: Rect::new(0.0, 0.0, 1440.0, 900.0),
            usable: Rect::new(0.0, 25.0, 1440.0, 800.0),
            scale: 2.0,
        }])
    }

    fn standing(def: Arc<PetDef>, seed: u64, world: &World, x: f32, facing: f32) -> Pet {
        let floor = *world.floor_under(x).unwrap();
        let mut pet = Pet::spawn_at(seed as u32, def, seed, world, Vec2::new(x, floor.y));
        pet.set_ground(&floor);
        pet.face(facing);
        pet.force("stand");
        pet
    }

    fn step(pet: &mut Pet, world: &World, ticks: usize) {
        for _ in 0..ticks {
            pet.step(TICK, world, &Input::default());
        }
    }

    #[test]
    fn only_an_adult_ram_can_flehmen_after_sniffing_an_adult_ewe() {
        let world = world();
        for (sex, age, other_sex, other_age, allowed) in [
            (Sex::Male, Age::Adult, Sex::Female, Age::Adult, true),
            (Sex::Male, Age::Adult, Sex::Male, Age::Adult, false),
            (Sex::Female, Age::Adult, Sex::Female, Age::Adult, false),
            (Sex::Female, Age::Adult, Sex::Male, Age::Adult, false),
            (
                Sex::Male,
                Age::Lamb { weeks: 2.0 },
                Sex::Female,
                Age::Adult,
                false,
            ),
            (
                Sex::Male,
                Age::Adult,
                Sex::Female,
                Age::Lamb { weeks: 2.0 },
                false,
            ),
        ] {
            let mut a = standing(definition(sex, age), 1, &world, 600.0, 1.0);
            let mut b = standing(definition(other_sex, other_age), 2, &world, 660.0, -1.0);
            assert!(!a.force("flehmen"));
            encounter(&mut a, &mut b, &world, &Input::default());
            assert_eq!(a.behavior_name(), "sniff");
            step(&mut a, &world, 30);
            assert_eq!(
                a.behavior_name() == "flehmen",
                allowed,
                "{sex:?}/{age:?} avec {other_sex:?}/{other_age:?}"
            );
        }
    }

    #[test]
    fn sniffing_alone_or_a_female_from_another_family_does_not_allow_flehmen() {
        let world = world();
        let mut ram = standing(definition(Sex::Male, Age::Adult), 1, &world, 600.0, 1.0);
        ram.force("sniff");
        assert!(!ram.force("flehmen"));
        let mut cat = (*definition(Sex::Female, Age::Adult)).clone();
        cat.meta.category = Some("cats".into());
        let mut cat = standing(Arc::new(cat), 2, &world, 660.0, -1.0);
        ram.force("stand");
        encounter(&mut ram, &mut cat, &world, &Input::default());
        step(&mut ram, &world, 30);
        assert_ne!(ram.behavior_name(), "flehmen");
    }

    #[test]
    fn a_sniff_is_consumed_once_and_interruption_or_expiry_clears_it() {
        let world = world();
        for interrupted in [false, true] {
            let mut ram = standing(definition(Sex::Male, Age::Adult), 1, &world, 600.0, 1.0);
            let mut ewe = standing(definition(Sex::Female, Age::Adult), 2, &world, 660.0, -1.0);
            encounter(&mut ram, &mut ewe, &world, &Input::default());
            if interrupted {
                ram.force("stand");
                ram.force("sniff");
                assert!(!ram.force("flehmen"));
            } else {
                assert!(ram.force("flehmen"));
                assert!(
                    !ram.force("flehmen"),
                    "le même flairage ne doit pas être réutilisé"
                );
            }
        }
        let mut ram = standing(definition(Sex::Male, Age::Adult), 1, &world, 600.0, 1.0);
        let mut ewe = standing(definition(Sex::Female, Age::Adult), 2, &world, 660.0, -1.0);
        encounter(&mut ram, &mut ewe, &world, &Input::default());
        ram.behavior_length = 20.0;
        step(&mut ram, &world, 16 * 120);
        assert_eq!(ram.behavior_name(), "sniff");
        assert!(!ram.force("flehmen"));
    }

    #[test]
    fn a_shared_encounter_respects_the_second_animals_conditions() {
        let world = world();
        let with_reaction = |sex| {
            let mut def = (*definition(sex, Age::Adult)).clone();
            def.behaviors[0].on_meet = vec![Transition {
                to: def.behavior_index("back").unwrap(),
                weight: 1.0,
                when: vec![Condition::Sex(Sex::Male)],
            }];
            Arc::new(def)
        };
        let mut ram = standing(with_reaction(Sex::Male), 1, &world, 600.0, 1.0);
        let mut ewe = standing(with_reaction(Sex::Female), 2, &world, 660.0, -1.0);
        encounter(&mut ram, &mut ewe, &world, &Input::default());
        assert_eq!(ram.behavior_name(), "back");
        assert_eq!(ewe.behavior_name(), "stand");
    }

    #[test]
    fn mimicking_a_neighbour_does_not_bypass_transition_conditions() {
        let world = world();
        let mut def = (*definition(Sex::Female, Age::Adult)).clone();
        def.behaviors[0].on_meet = vec![Transition {
            to: def.behavior_index("back").unwrap(),
            weight: 0.0,
            when: vec![Condition::Sex(Sex::Male)],
        }];
        for seed in 0..64 {
            let mut ewe = standing(Arc::new(def.clone()), seed, &world, 600.0, 1.0);
            let mut ram = standing(definition(Sex::Male, Age::Adult), 100, &world, 660.0, 1.0);
            ram.force("back");
            encounter(&mut ewe, &mut ram, &world, &Input::default());
            assert_eq!(ewe.behavior_name(), "stand");
        }
    }

    #[test]
    fn ram_pairs_choose_contests_more_often_without_removing_lamb_play() {
        let world = world();
        let count = |sex, other_sex, age| {
            let mut def = (*definition(sex, age)).clone();
            def.behaviors[0].on_meet = vec![
                Transition {
                    to: 1,
                    weight: 1.0,
                    when: vec![],
                },
                Transition {
                    to: 3,
                    weight: 1.0,
                    when: vec![],
                },
            ];
            let mut other = def.clone();
            other.sex_ratio = if other_sex == Sex::Male {
                SexRatio::MALE
            } else {
                SexRatio::FEMALE
            };
            let (def, other) = (Arc::new(def), Arc::new(other));
            (0..1000)
                .filter(|&seed| {
                    let mut a = standing(Arc::clone(&def), seed, &world, 600.0, 1.0);
                    let mut b = standing(Arc::clone(&other), seed + 1000, &world, 660.0, -1.0);
                    encounter(&mut a, &mut b, &world, &Input::default());
                    a.behavior_name() == "back"
                })
                .count()
        };
        let rams = count(Sex::Male, Sex::Male, Age::Adult);
        let mixed = count(Sex::Male, Sex::Female, Age::Adult);
        let lambs = count(Sex::Male, Sex::Female, Age::Lamb { weeks: 2.0 });
        assert!(rams > 650 && mixed < 300, "béliers {rams}, mixtes {mixed}");
        assert!((400..600).contains(&lambs), "jeux d'agneaux {lambs}");
    }
}
