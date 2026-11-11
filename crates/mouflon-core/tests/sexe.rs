//! L'identité biologique est déterministe et ne décale pas les tirages des
//! mouvements indépendants du sexe.

use std::sync::Arc;

use mouflon_core::def::Motion;
use mouflon_core::{
    Age, Gesture, Input, Pet, PetDef, Rect, Screen, Sex, SexRatio, TICK, Variant, World,
    parse_pet_toml,
};

fn definition(identity: &str) -> Result<PetDef, mouflon_core::LoadError> {
    parse_pet_toml(
        &format!(
            r#"
[pet]
id = "sheep"
name = "Mouton"
category = "sheep"
{identity}
[appearance]
rig = "mouton.svg"
height = 50.0
[[behavior]]
name = "stand"
motion = "stand"
duration = [0.5, 1.5]
next = "walk"
[[behavior]]
name = "walk"
motion = "walk"
duration = [0.5, 1.5]
next = "stand"
"#
        ),
        |_: &str| Ok(b"<svg/>".to_vec()),
    )
}

fn world() -> World {
    World::new(vec![Screen {
        frame: Rect::new(0.0, 0.0, 1440.0, 900.0),
        usable: Rect::new(0.0, 25.0, 1440.0, 800.0),
        scale: 2.0,
    }])
}

#[test]
fn an_author_can_fix_or_weight_the_sex_and_declare_a_lambs_age() {
    let adult = definition("").unwrap();
    assert_eq!(adult.sex_ratio.male_probability(), 0.5);
    assert_eq!(adult.age, Age::Adult);
    assert_eq!(
        definition("sex = \"male\"").unwrap().sex_ratio,
        SexRatio::MALE
    );
    assert_eq!(
        definition("sex = \"female\"").unwrap().sex_ratio,
        SexRatio::FEMALE
    );
    let lamb = definition("sex = { male = 0.3 }\nage = \"lamb\"\nage_weeks = 6.5").unwrap();
    assert_eq!(lamb.sex_ratio.male_probability(), 0.3);
    assert_eq!(lamb.age, Age::Lamb { weeks: 6.5 });
    assert_eq!(
        definition("age = \"lamb\"").unwrap().age,
        Age::Lamb { weeks: 2.0 }
    );
}

#[test]
fn invalid_sex_ratios_and_ages_are_rejected_instead_of_silently_clamped() {
    for identity in [
        "sex = \"ram\"",
        "sex = { male = -0.1 }",
        "sex = { male = 1.1 }",
        "sex = { male = nan }",
        "sex = { male = inf }",
        "sex = { male = 0.5, typo = 1 }",
        "age = \"old\"",
        "age_weeks = 2.0",
        "age = \"adult\"\nage_weeks = 2.0",
        "age = \"lamb\"\nage_weeks = 0.0",
        "age = \"lamb\"\nage_weeks = -1.0",
        "age = \"lamb\"\nage_weeks = nan",
        "age = \"lamb\"\nage_weeks = inf",
        "age = \"lamb\"\nage_weeks = 53.0",
    ] {
        assert!(definition(identity).is_err(), "{identity}");
    }
    for ratio in [f32::NAN, f32::INFINITY, f32::NEG_INFINITY, -0.1, 1.1] {
        assert!(SexRatio::new(ratio).is_err());
    }
}

#[test]
fn sex_is_reproducible_and_follows_the_authors_distribution() {
    let world = world();
    let def = Arc::new(definition("sex = { male = 0.3 }").unwrap());
    let mut males = 0;
    for id in 0..4096 {
        let a = Pet::spawn(id, Arc::clone(&def), 0x1234, &world, Some(600.0));
        let b = Pet::spawn(id, Arc::clone(&def), 0x1234, &world, Some(600.0));
        assert_eq!(a.sex(), b.sex());
        males += usize::from(a.sex() == Sex::Male);
    }
    assert!((1100..1400).contains(&males), "{males} mâles sur 4096");
}

#[test]
fn choosing_a_sex_does_not_shift_unrelated_motion_or_animation_randomness() {
    let world = world();
    let mut male = Pet::spawn(
        7,
        Arc::new(definition("sex = \"male\"").unwrap()),
        5,
        &world,
        Some(600.0),
    );
    let mut female = Pet::spawn(
        7,
        Arc::new(definition("sex = \"female\"").unwrap()),
        5,
        &world,
        Some(600.0),
    );
    assert_eq!(male.sex(), Sex::Male);
    assert_eq!(female.sex(), Sex::Female);
    for _ in 0..2400 {
        male.step(TICK, &world, &Input::default());
        female.step(TICK, &world, &Input::default());
        assert_eq!(male.position(), female.position());
        assert_eq!(male.velocity(), female.velocity());
        assert_eq!(male.behavior_name(), female.behavior_name());
        assert_eq!(male.view(1.0).pose, female.view(1.0).pose);
    }
}

#[test]
fn resizing_preserves_sex_and_does_not_turn_a_lamb_into_an_adult() {
    let world = world();
    let def = Arc::new(definition("age = \"lamb\"\nage_weeks = 2.5").unwrap());
    let mut pet = Pet::spawn(8, Arc::clone(&def), 17, &world, Some(600.0));
    let sex = pet.sex();
    assert!(pet.rescale(Arc::new(def.scaled(2.0)), 2.0));
    assert_eq!(pet.sex(), sex);
    assert_eq!(pet.age(), Age::Lamb { weeks: 2.5 });
}

#[test]
fn every_breed_has_a_ram_a_ewe_and_a_lamb_each_in_its_own_drawing() {
    // Chaque race du dépôt a ses trois dessins d'après nature : le dessin de
    // base montre le bélier ou la brebis, l'autre sexe a le sien
    // (`[appearance.male]` ou `[appearance.female]`), et l'agneau aussi
    // (`[appearance.lamb]`). Le sexe se tire donc : trois brebis pour un
    // bélier, comme dans un troupeau.
    let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../pets");
    let mut breeds = 0;
    for entry in std::fs::read_dir(&root).unwrap() {
        let dir = entry.unwrap().path();
        let name = dir.file_name().unwrap().to_string_lossy().into_owned();
        if name == "floucat" || !dir.join("pet.toml").is_file() {
            continue;
        }
        breeds += 1;
        let def = mouflon_core::load_pet(&dir.join("pet.toml")).unwrap();
        assert_eq!(def.age, Age::Adult, "{name}");
        assert_eq!(def.sex_ratio, SexRatio::new(0.25).unwrap(), "{name}");
        let other_sex = [Variant::Male, Variant::Female]
            .into_iter()
            .filter(|v| def.variant(*v).is_some())
            .count();
        assert_eq!(other_sex, 1, "{name} : le dessin de l'autre sexe");
        let lamb = def.variant(Variant::Lamb);
        assert!(lamb.is_some(), "{name} : le dessin de l'agneau");
        // L'agneau est plus petit que ses parents, et il a son âge.
        let lamb = lamb.unwrap();
        assert!(lamb.physics.height < def.physics.height, "{name}");
        assert!(matches!(lamb.age, Age::Lamb { .. }), "{name}");
        // Les deux sexes s'y retrouvent à chaque tirage.
        let def = Arc::new(def);
        let world = world();
        let (mut rams, mut ewes) = (0, 0);
        for id in 0..200 {
            let pet = Pet::spawn(id, Arc::clone(&def), 7, &world, Some(600.0));
            let wears = pet.variant();
            match pet.sex() {
                Sex::Male => {
                    rams += 1;
                    assert_eq!(
                        wears == Some(Variant::Male),
                        def.variant(Variant::Male).is_some()
                    );
                }
                Sex::Female => {
                    ewes += 1;
                    assert_eq!(
                        wears == Some(Variant::Female),
                        def.variant(Variant::Female).is_some()
                    );
                }
            }
        }
        assert!(
            rams > 25 && ewes > 120,
            "{name} : {rams} béliers, {ewes} brebis"
        );
    }
    assert_eq!(breeds, 28);
    // L'agnelle Southdown a été dessinée à huit semaines : elle le dit.
    let def = mouflon_core::load_pet(&root.join("southdown/pet.toml")).unwrap();
    assert_eq!(def.lamb_weeks(), 8.0);
}

#[test]
fn the_floucat_is_a_wether_apart_from_its_breed() {
    // Le floucat du Mérinos d'Arles est un mâle castré : un animal à part,
    // déclaré mâle (le moteur n'a pas de valeur pour un castrat), sans
    // brebis ni agneau à lui, et sans le flehmen ni la charge du bélier
    // entier, qui vit dans `pets/merinos-d-arles`.
    let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../pets");
    let def = mouflon_core::load_pet(&root.join("floucat/pet.toml")).unwrap();
    assert_eq!(def.sex_ratio, SexRatio::MALE);
    assert_eq!(def.age, Age::Adult);
    assert_eq!(def.variants().count(), 0);
    assert!(def.behaviors.iter().all(|b| !matches!(
        b.motion,
        Motion::Gesture(Gesture::Flehmen) | Motion::Charge { .. }
    )));
    // Le bélier entier de la race, lui, les a.
    let breed = mouflon_core::load_pet(&root.join("merinos-d-arles/pet.toml")).unwrap();
    assert!(breed.variant(Variant::Male).is_some());
    for motion in ["flehmen", "charge"] {
        assert!(breed.behavior_index(motion).is_some(), "{motion}");
    }
}

#[test]
fn every_drawn_breed_fixes_its_sex_unless_it_has_a_drawing_for_each_sex() {
    // Sans `sex`, un bélier dessiné serait tiré brebis une fois sur deux, et
    // son menu proposerait d'ajouter son agneau. Une race qui a aussi le
    // dessin de l'autre sexe (`[appearance.female]` à côté d'un bélier, ou
    // `[appearance.male]` à côté d'une brebis) tire le sexe de chacun.
    let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../..");
    let mut breeds = 0;
    for entry in std::fs::read_dir(root.join("pets")).unwrap() {
        let dir = entry.unwrap().path();
        let name = dir.file_name().unwrap().to_string_lossy().into_owned();
        let def = mouflon_core::load_pet(&dir.join("pet.toml")).unwrap();
        let both = def.variant(mouflon_core::Variant::Male).is_some()
            || def.variant(mouflon_core::Variant::Female).is_some();
        assert!(
            def.sex_ratio == SexRatio::MALE || def.sex_ratio == SexRatio::FEMALE || both,
            "{name} : le sexe du dessin n'est pas fixé"
        );
        breeds += 1;
    }
    assert!(breeds >= 28, "{breeds} races");
    // Lainou et Pelote, gardés comme données de test, n'ont pas de sexe fixé.
    for name in ["lainou", "pelote"] {
        let file = root.join("tests/donnees").join(name).join("pet.toml");
        let def = mouflon_core::load_pet(&file).unwrap();
        assert_eq!(def.sex_ratio.male_probability(), 0.5, "{name}");
    }
}
