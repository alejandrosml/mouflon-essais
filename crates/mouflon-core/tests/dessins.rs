//! Un dessin par sexe et par âge (`[appearance.male]`, `[appearance.female]`,
//! `[appearance.lamb]`) : chaque animal porte, dès son apparition, celui de
//! son sexe et de son âge, à sa taille ; sans lui, le dessin de base.

use std::sync::Arc;

use mouflon_core::def::Motion;
use mouflon_core::{
    Age, Appearance, Flock, Input, Pet, PetDef, Rect, Screen, Sex, SexRatio, Support, TICK,
    Variant, World, parse_pet_toml,
};

/// Le bélier du dessin de base, et les dessins par sexe ou par âge donnés.
/// Chaque fichier « lu » contient son propre nom : on voit quel dessin
/// porte un animal.
fn definition(identity: &str, drawings: &str) -> Result<PetDef, mouflon_core::LoadError> {
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
{drawings}
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
}

/// La brebis et l'agneau, avec leurs tailles.
const EWE_AND_LAMB: &str = r#"
[appearance.female]
rig = "brebis.svg"
height = 45.0
width = 55.0
stride = 36.0

[appearance.lamb]
rig = "agneau.svg"
height = 30.0
width = 32.0
stride = 28.0
"#;

fn world() -> World {
    World::new(vec![Screen {
        frame: Rect::new(0.0, 0.0, 1440.0, 900.0),
        usable: Rect::new(0.0, 25.0, 1440.0, 800.0),
        scale: 2.0,
    }])
}

/// Le fichier du dessin et sa hauteur affichée.
fn drawing(def: &PetDef) -> (String, f32) {
    match &def.appearance {
        Appearance::Rig { svg, height } => (String::from_utf8(svg.to_vec()).unwrap(), *height),
        _ => panic!("dessin vectoriel attendu"),
    }
}

fn walk_speed(def: &PetDef) -> f32 {
    def.behaviors
        .iter()
        .find_map(|b| match b.motion {
            Motion::Walk { speed } => Some(speed),
            _ => None,
        })
        .unwrap()
}

fn close(a: f32, b: f32) -> bool {
    (a - b).abs() < 1e-3 * b.abs().max(1.0)
}

#[test]
fn a_pet_file_can_give_a_drawing_for_the_ewe_and_for_the_lamb() {
    let def = definition("sex = { male = 0.5 }", EWE_AND_LAMB).unwrap();
    assert_eq!(drawing(&def), ("belier.svg".into(), 50.0));
    assert!(def.variant(Variant::Male).is_none());

    // La brebis : sa taille, sa largeur, sa foulée ; ses vitesses et sa
    // pesanteur suivent la longueur de ses pattes (36 / 40).
    let ewe = def.variant(Variant::Female).unwrap();
    assert_eq!(drawing(ewe), ("brebis.svg".into(), 45.0));
    assert_eq!(
        (ewe.physics.height, ewe.physics.width, ewe.physics.stride),
        (45.0, 55.0, 36.0)
    );
    assert!(close(walk_speed(ewe), 36.0));
    assert!(close(ewe.physics.gravity, def.physics.gravity * 0.9));

    // L'agneau, de même (28 / 40).
    let lamb = def.variant(Variant::Lamb).unwrap();
    assert_eq!(drawing(lamb), ("agneau.svg".into(), 30.0));
    assert_eq!(
        (lamb.physics.height, lamb.physics.width, lamb.physics.stride),
        (30.0, 32.0, 28.0)
    );
    assert!(close(walk_speed(lamb), 28.0));

    // Le reste est celui de l'animal : nom, sexe, âge, comportements.
    // L'agneau dessiné est un agneau, de deux semaines sans autre âge.
    assert_eq!(ewe.age, Age::Adult);
    assert_eq!(lamb.age, Age::Lamb { weeks: 2.0 });
    assert_eq!(def.lamb_weeks(), 2.0);
    for drawn in [ewe, lamb] {
        assert_eq!(drawn.meta, def.meta);
        assert_eq!(drawn.sex_ratio, def.sex_ratio);
        assert_eq!(drawn.behaviors.len(), def.behaviors.len());
        assert_eq!(drawn.variants().count(), 0);
    }
    // Le dessin de base n'a pas changé.
    assert_eq!(
        (def.physics.height, def.physics.width, def.physics.stride),
        (50.0, 60.0, 40.0)
    );
    assert!(close(walk_speed(&def), 40.0));
}

#[test]
fn without_width_or_stride_a_drawing_follows_the_ratio_of_heights() {
    let def = definition(
        "sex = { male = 0.5 }",
        "[appearance.female]\nrig = \"brebis.svg\"\nheight = 40.0\n",
    )
    .unwrap();
    let ewe = def.variant(Variant::Female).unwrap();
    assert!(close(ewe.physics.height, 40.0));
    assert!(close(ewe.physics.width, 48.0));
    assert!(close(ewe.physics.stride, 32.0));
    assert!(close(walk_speed(ewe), 32.0));
}

#[test]
fn each_animal_wears_the_drawing_of_its_sex_from_its_first_step() {
    let world = world();
    let def = Arc::new(definition("sex = { male = 0.5 }", EWE_AND_LAMB).unwrap());
    let (mut rams, mut ewes) = (0, 0);
    for id in 0..64 {
        let pet = Pet::spawn(id, Arc::clone(&def), 9, &world, Some(600.0));
        assert!(Arc::ptr_eq(pet.species(), &def));
        match pet.sex() {
            Sex::Male => {
                rams += 1;
                assert_eq!(pet.variant(), None);
                assert!(Arc::ptr_eq(pet.def(), &def));
            }
            Sex::Female => {
                ewes += 1;
                assert_eq!(pet.variant(), Some(Variant::Female));
                assert_eq!(drawing(pet.def()), ("brebis.svg".into(), 45.0));
                assert_eq!(pet.def().physics.width, 55.0);
            }
        }
    }
    assert!(rams > 10 && ewes > 10, "{rams} béliers, {ewes} brebis");
}

#[test]
fn drawings_do_not_change_which_sex_an_animal_is_given() {
    let world = world();
    let plain = Arc::new(definition("sex = { male = 0.3 }", "").unwrap());
    let drawn = Arc::new(definition("sex = { male = 0.3 }", EWE_AND_LAMB).unwrap());
    for id in 0..256 {
        let a = Pet::spawn(id, Arc::clone(&plain), 0x51, &world, Some(600.0));
        let b = Pet::spawn(id, Arc::clone(&drawn), 0x51, &world, Some(600.0));
        assert_eq!(a.sex(), b.sex());
    }
}

#[test]
fn a_lamb_wears_the_lamb_drawing_at_its_own_size_whatever_its_age() {
    let def = definition("sex = \"female\"", EWE_AND_LAMB).unwrap();
    for weeks in [2.0, 7.0] {
        let lamb = def.as_lamb(weeks).unwrap();
        assert_eq!(drawing(&lamb), ("agneau.svg".into(), 30.0));
        assert_eq!((lamb.physics.height, lamb.physics.width), (30.0, 32.0));
        assert!(close(walk_speed(&lamb), 28.0));
        assert_eq!(lamb.age, Age::Lamb { weeks });
        assert_eq!(lamb.sex_ratio, SexRatio::default());
        assert_eq!(lamb.variants().count(), 0);
    }
}

#[test]
fn without_a_lamb_drawing_the_lamb_is_its_mothers_drawing_reduced() {
    // La brebis dessinée : son agneau provisoire est la brebis réduite.
    let def = definition(
        "sex = \"female\"",
        "[appearance.female]\nrig = \"brebis.svg\"\nheight = 45.0\n",
    )
    .unwrap();
    let lamb = def.as_lamb(2.0).unwrap();
    assert_eq!(drawing(&lamb).0, "brebis.svg");
    assert!(close(lamb.physics.height, 45.0 * 0.55));
    // Sans aucun autre dessin : le dessin de base réduit, comme avant.
    let def = definition("sex = \"female\"", "").unwrap();
    let lamb = def.as_lamb(2.0).unwrap();
    assert_eq!(drawing(&lamb).0, "belier.svg");
    assert!(close(lamb.physics.height, 50.0 * 0.55));
}

#[test]
fn a_ewe_gives_birth_to_a_lamb_that_wears_the_lamb_drawing() {
    let world = world();
    let def = Arc::new(definition("sex = \"female\"", EWE_AND_LAMB).unwrap());
    let mut flock = Flock::new();
    let mother = flock.spawn(Arc::clone(&def), 3, &world, Some(600.0));
    let input = Input {
        local_hour: 15.0,
        ..Input::default()
    };
    for _ in 0..(3.0 / TICK) as usize {
        flock.advance(TICK, &world, &input);
    }
    let ewe = flock.pets().iter().find(|p| p.id() == mother).unwrap();
    assert!(matches!(ewe.support(), Support::Ground { .. }));
    assert_eq!(drawing(ewe.def()).0, "brebis.svg");
    let lamb = flock.spawn_lamb(mother, 11, 2.0, &world).unwrap();
    let lamb = flock.pets().iter().find(|p| p.id() == lamb).unwrap();
    assert_eq!(drawing(lamb.def()), ("agneau.svg".into(), 30.0));
    assert_eq!(lamb.age(), Age::Lamb { weeks: 2.0 });
    assert_eq!(lamb.mother(), Some(mother));
}

#[test]
fn resizing_keeps_the_drawing_of_its_sex_at_the_new_size() {
    let world = world();
    let def = Arc::new(definition("sex = \"female\"", EWE_AND_LAMB).unwrap());
    let mut pet = Pet::spawn(4, Arc::clone(&def), 2, &world, Some(600.0));
    assert_eq!(pet.variant(), Some(Variant::Female));
    let big = Arc::new(def.scaled(2.0));
    assert!(pet.rescale(Arc::clone(&big), 2.0));
    assert!(Arc::ptr_eq(pet.species(), &big));
    assert_eq!(pet.variant(), Some(Variant::Female));
    assert_eq!(drawing(pet.def()), ("brebis.svg".into(), 90.0));
    assert_eq!(pet.def().physics.width, 110.0);
}

#[test]
fn the_drawings_grow_with_the_animal() {
    let def = definition("sex = { male = 0.5 }", EWE_AND_LAMB).unwrap();
    let big = def.scaled(2.0);
    let ewe = big.variant(Variant::Female).unwrap();
    assert_eq!(drawing(ewe), ("brebis.svg".into(), 90.0));
    assert_eq!(
        (ewe.physics.height, ewe.physics.width, ewe.physics.stride),
        (90.0, 110.0, 72.0)
    );
    assert!(close(walk_speed(ewe), 72.0));
    assert_eq!(drawing(big.variant(Variant::Lamb).unwrap()).1, 60.0);
}

#[test]
fn only_the_drawings_its_animals_can_wear_are_prepared() {
    let worn = |identity: &str, drawings: &str| -> Vec<Variant> {
        definition(identity, drawings)
            .unwrap()
            .worn_variants()
            .map(|(v, _)| v)
            .collect()
    };
    let all = format!("{EWE_AND_LAMB}\n[appearance.male]\nrig = \"belier-2.svg\"\nheight = 52.0\n");
    // L'agneau vient d'une brebis (`PetDef::as_lamb`), avec son propre dessin.
    assert_eq!(
        worn("sex = { male = 0.5 }", &all),
        [Variant::Male, Variant::Female]
    );
    assert_eq!(worn("sex = \"female\"", &all), [Variant::Female]);
    assert_eq!(worn("sex = \"male\"", &all), [Variant::Male]);
    // Un agneau déclaré dans son fichier porte le dessin d'agneau.
    assert_eq!(worn("age = \"lamb\"", &all), [Variant::Lamb]);
    let world = world();
    let def = Arc::new(definition("age = \"lamb\"", &all).unwrap());
    let pet = Pet::spawn(1, def, 1, &world, Some(600.0));
    assert_eq!(pet.variant(), Some(Variant::Lamb));
    assert_eq!(drawing(pet.def()).0, "agneau.svg");
}

#[test]
fn a_drawing_by_sex_or_age_is_checked_when_the_file_is_read() {
    let bad = [
        // Sa hauteur affichée est obligatoire.
        "[appearance.female]\nrig = \"brebis.svg\"\n",
        // Un seul dessin par section.
        "[appearance.female]\nrig = \"brebis.svg\"\nskeleton = \"s.toml\"\nheight = 40.0\n",
        "[appearance.female]\nheight = 40.0\n",
        // Des tailles positives et finies.
        "[appearance.lamb]\nrig = \"agneau.svg\"\nheight = 0.0\n",
        "[appearance.lamb]\nrig = \"agneau.svg\"\nheight = 30.0\nwidth = -2.0\n",
        "[appearance.lamb]\nrig = \"agneau.svg\"\nheight = 30.0\nstride = inf\n",
        // Seulement male, female et lamb, et leurs clés.
        "[appearance.ram]\nrig = \"belier.svg\"\nheight = 40.0\n",
        "[appearance.female]\nrig = \"brebis.svg\"\nheight = 40.0\nspeed = 3.0\n",
    ];
    for drawings in bad {
        assert!(
            definition("", drawings).is_err(),
            "accepté à tort : {drawings}"
        );
    }
    // Une planche d'images n'a pas d'autres dessins.
    let sprites = parse_pet_toml(
        r#"
[pet]
id = "planche"
name = "Planche"
[appearance]
sprites = "planche.png"
tiles = [2, 1]
[appearance.female]
rig = "brebis.svg"
height = 40.0
[[behavior]]
name = "stand"
motion = "stand"
"#,
        |name: &str| Ok(name.as_bytes().to_vec()),
    );
    let error = sprites.err().unwrap().to_string();
    assert!(error.contains("dessin de base vectoriel"), "{error}");
}

#[test]
fn the_lamb_drawing_can_say_how_old_its_lamb_is() {
    // Sans âge, l'agneau dessiné a deux semaines : l'âge où l'app le fait
    // naître. Dessiné plus grand (une agnelle de huit semaines), il le dit.
    let young = definition("", EWE_AND_LAMB).unwrap();
    assert_eq!(young.lamb_weeks(), 2.0);
    let older = definition(
        "",
        r#"
[appearance.lamb]
rig = "agneau.svg"
height = 34.0
weeks = 8
"#,
    )
    .unwrap();
    assert_eq!(older.lamb_weeks(), 8.0);
    assert_eq!(
        older.variant(Variant::Lamb).unwrap().age,
        Age::Lamb { weeks: 8.0 }
    );
    // L'agneau préparé à cet âge le garde, dans son dessin.
    let lamb = older.as_lamb(older.lamb_weeks()).unwrap();
    assert_eq!(lamb.age, Age::Lamb { weeks: 8.0 });
    assert_eq!(drawing(&lamb), ("agneau.svg".into(), 34.0));
    // Sans dessin d'agneau, deux semaines.
    assert_eq!(definition("", "").unwrap().lamb_weeks(), 2.0);

    // L'âge ne se donne qu'au dessin de l'agneau, entre 0 et 52 semaines.
    for (section, weeks) in [
        ("female", "8"),
        ("lamb", "0"),
        ("lamb", "60"),
        ("lamb", "nan"),
    ] {
        let refused = definition(
            "",
            &format!(
                "[appearance.{section}]\nrig = \"autre.svg\"\nheight = 40.0\nweeks = {weeks}\n"
            ),
        );
        assert!(refused.is_err(), "{section} : weeks = {weeks}");
    }
}
