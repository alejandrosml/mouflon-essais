//! Les arrivées : un bélier ou une brebis au choix, la brebis suivie de son
//! agneau, et des agneaux un cran plus vifs que le troupeau.

use std::sync::Arc;

use mouflon_core::{
    Age, Flock, Input, Mood, Pet, PetDef, Rect, Screen, Sex, Support, TICK, Temper, Variant, World,
    parse_pet_toml,
};

/// Un mouton dont le dessin de base est le bélier, avec sa brebis et son
/// agneau ; `identity` : son sexe, comme dans `[pet]`.
fn definition(identity: &str) -> PetDef {
    parse_pet_toml(
        &format!(
            r#"
[pet]
id = "sheep"
name = "Mouton"
category = "sheep"
{identity}
[appearance]
rig = "belier.svg"
height = 50.0

[appearance.female]
rig = "brebis.svg"
height = 45.0

[appearance.lamb]
rig = "agneau.svg"
height = 28.0

[physics]
width = 60.0
stride = 40.0

[[behavior]]
name = "stand"
motion = "stand"
duration = [0.5, 1.5]
next = "walk"
[[behavior]]
name = "walk"
motion = "walk"
speed = 40.0
duration = [0.5, 1.5]
next = "stand"
"#
        ),
        |name: &str| Ok(name.as_bytes().to_vec()),
    )
    .unwrap()
}

fn world() -> World {
    World::new(vec![Screen {
        frame: Rect::new(0.0, 0.0, 1440.0, 900.0),
        usable: Rect::new(0.0, 25.0, 1440.0, 800.0),
        scale: 2.0,
    }])
}

#[test]
fn a_ram_or_a_ewe_can_be_asked_for_when_the_breed_has_both() {
    let world = world();
    let def = Arc::new(definition("sex = { male = 0.25 }"));
    for id in 0..40 {
        let ram = Pet::spawn_as(
            id,
            Arc::clone(&def),
            9,
            &world,
            Some(600.0),
            Some(Sex::Male),
        );
        assert_eq!(ram.sex(), Sex::Male);
        assert!(ram.is_ram());
        // Le bélier est le dessin de base.
        assert_eq!(ram.variant(), None);
        let ewe = Pet::spawn_as(
            id,
            Arc::clone(&def),
            9,
            &world,
            Some(600.0),
            Some(Sex::Female),
        );
        assert_eq!(ewe.sex(), Sex::Female);
        assert_eq!(ewe.variant(), Some(Variant::Female));
        // Sans demande, le tirage de son fichier ; celui qui est tiré du sexe
        // demandé est le même animal, au même endroit.
        let drawn = Pet::spawn(id, Arc::clone(&def), 9, &world, Some(600.0));
        let same = if drawn.sex() == Sex::Male { &ram } else { &ewe };
        assert_eq!(drawn.position(), same.position());
        assert_eq!(drawn.behavior_name(), same.behavior_name());
        assert_eq!(drawn.view(1.0).facing, same.view(1.0).facing);
    }
}

#[test]
fn asking_for_a_sex_shifts_no_other_draw() {
    // Sans dessin par sexe, un bélier et une brebis du même numéro vivent la
    // même vie : la demande ne décale ni leurs pas ni leurs choix.
    let world = world();
    let mut def = definition("sex = { male = 0.5 }");
    def.set_variants(Vec::new()).unwrap();
    let def = Arc::new(def);
    let mut ram = Pet::spawn_as(
        3,
        Arc::clone(&def),
        21,
        &world,
        Some(500.0),
        Some(Sex::Male),
    );
    let mut ewe = Pet::spawn_as(
        3,
        Arc::clone(&def),
        21,
        &world,
        Some(500.0),
        Some(Sex::Female),
    );
    let mut plain = Pet::spawn(3, Arc::clone(&def), 21, &world, Some(500.0));
    assert_ne!(ram.sex(), ewe.sex());
    for _ in 0..(20.0 / TICK) as usize {
        for pet in [&mut ram, &mut ewe, &mut plain] {
            pet.step(TICK, &world, &Input::default());
        }
        assert_eq!(ram.position(), ewe.position());
        assert_eq!(ram.position(), plain.position());
        assert_eq!(ram.behavior_name(), plain.behavior_name());
    }
}

#[test]
fn a_drawing_fixed_to_one_sex_keeps_it_whatever_is_asked() {
    // Un bélier dessiné (`sex = "male"`) ne devient pas brebis à la demande.
    let world = world();
    for (identity, sex) in [
        ("sex = \"male\"", Sex::Male),
        ("sex = \"female\"", Sex::Female),
    ] {
        let def = Arc::new(definition(identity));
        for wanted in [Sex::Male, Sex::Female] {
            let pet = Pet::spawn_as(1, Arc::clone(&def), 4, &world, None, Some(wanted));
            assert_eq!(pet.sex(), sex, "{identity}");
        }
    }
    // Dans le troupeau aussi.
    let def = Arc::new(definition("sex = { male = 0.25 }"));
    let mut flock = Flock::new();
    let ram = flock.spawn_as(Arc::clone(&def), 1, &world, None, Some(Sex::Male));
    let ewe = flock.spawn_as(Arc::clone(&def), 2, &world, None, Some(Sex::Female));
    let sex = |id| flock.pets().iter().find(|p| p.id() == id).unwrap().sex();
    assert_eq!((sex(ram), sex(ewe)), (Sex::Male, Sex::Female));
}

#[test]
fn a_lamb_arrives_with_its_mother_falling_behind_her_and_stays_hers() {
    let world = world();
    let def = Arc::new(definition("sex = { male = 0.25 }"));
    let young = Arc::new(def.as_lamb(def.lamb_weeks()).unwrap());
    let mut flock = Flock::new();
    let mother = flock.spawn_as(Arc::clone(&def), 5, &world, Some(700.0), Some(Sex::Female));
    // Elle vient d'apparaître, encore en l'air : son agneau la suit de là.
    let lamb = flock
        .spawn_lamb_arriving(mother, Arc::clone(&young), 6, &world)
        .expect("la brebis arrive avec son agneau");
    let find =
        |flock: &Flock, id: u32| flock.pets().iter().find(|p| p.id() == id).cloned().unwrap();
    let (m, l) = (find(&flock, mother), find(&flock, lamb));
    assert_eq!(l.mother(), Some(mother));
    assert_eq!(l.age(), Age::Lamb { weeks: 2.0 });
    assert_eq!(l.support(), Support::Air);
    assert_eq!(l.variant(), None, "son dessin d'agneau est sa définition");
    assert!((l.position().x - m.position().x).abs() < 80.0);
    assert!(l.position().y < 200.0, "il tombe du haut de l'écran");
    // Ils se posent tous les deux, et il reste le sien, près d'elle.
    for _ in 0..(30.0 / TICK) as usize {
        flock.advance(TICK, &world, &Input::default());
    }
    let (m, l) = (find(&flock, mother), find(&flock, lamb));
    assert!(matches!(m.support(), Support::Ground { .. }));
    assert!(matches!(l.support(), Support::Ground { .. }));
    assert_eq!(l.mother(), Some(mother));
    assert!((l.position().x - m.position().x).abs() < 300.0);

    // Un bélier n'est la mère de personne : rien n'est ajouté.
    let ram = flock.spawn_as(Arc::clone(&def), 7, &world, Some(300.0), Some(Sex::Male));
    let before = flock.pets().len();
    assert_eq!(
        flock.spawn_lamb_arriving(ram, Arc::clone(&young), 8, &world),
        None
    );
    assert_eq!(
        flock.spawn_lamb_arriving(999, Arc::clone(&young), 8, &world),
        None
    );
    // Ni un adulte donné pour agneau.
    assert_eq!(
        flock.spawn_lamb_arriving(mother, Arc::clone(&def), 8, &world),
        None
    );
    assert_eq!(flock.pets().len(), before);
}

#[test]
fn lambs_are_a_notch_livelier_than_the_rest_of_the_flock() {
    assert_eq!(Temper::Calm.of_lamb(), Temper::Even);
    assert_eq!(Temper::Even.of_lamb(), Temper::Lively);
    assert_eq!(Temper::Lively.of_lamb(), Temper::Lively);
    assert_eq!(Temper::Rowdy.of_lamb(), Temper::Rowdy);

    let world = world();
    let def = Arc::new(definition("sex = { male = 0.25 }"));
    let young = Arc::new(def.as_lamb(2.0).unwrap());
    let mut flock = Flock::new();
    let mother = flock.spawn_as(Arc::clone(&def), 5, &world, Some(700.0), Some(Sex::Female));
    let lamb = flock
        .spawn_lamb_arriving(mother, Arc::clone(&young), 6, &world)
        .unwrap();
    // Un agneau arrivé seul (un fichier d'agneau) aussi.
    let alone = flock.spawn(Arc::clone(&young), 7, &world, Some(300.0));
    let temper =
        |flock: &Flock, id: u32| flock.pets().iter().find(|p| p.id() == id).unwrap().temper();
    for (mood, adults, lambs) in [
        (Mood::Even, Temper::Even, Temper::Lively),
        (Mood::Calm, Temper::Calm, Temper::Even),
        (Mood::Lively, Temper::Lively, Temper::Lively),
        (Mood::Rowdy, Temper::Rowdy, Temper::Rowdy),
    ] {
        flock.set_mood(mood);
        assert_eq!(temper(&flock, mother), adults, "{mood:?}");
        assert_eq!(temper(&flock, lamb), lambs, "{mood:?}");
        assert_eq!(temper(&flock, alone), lambs, "{mood:?}");
    }
    // Né sur place d'une brebis posée, dans un troupeau calme.
    flock.set_mood(Mood::Calm);
    for _ in 0..(20.0 / TICK) as usize {
        flock.advance(TICK, &world, &Input::default());
    }
    flock.remove(lamb);
    let born = (0..200)
        .find_map(|k| {
            for _ in 0..30 {
                flock.advance(TICK, &world, &Input::default());
            }
            flock.spawn_lamb_with(mother, Arc::clone(&young), 100 + k, &world)
        })
        .expect("la brebis posée reçoit son agneau");
    assert_eq!(temper(&flock, born), Temper::Even);
}
