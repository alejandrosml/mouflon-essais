//! Un dessin par sexe et par âge, avec de vrais dessins : un animal d'essai
//! qui a le bélier Awassi pour dessin de base, la brebis Bleu du Maine pour
//! `[appearance.female]` et l'agnelle Southdown pour `[appearance.lamb]`.
//! Chaque animal se dessine avec le sien, à sa taille, et l'agneau tète sa
//! mère.

use std::path::Path;
use std::sync::Arc;

use mouflon_core::{
    Flock, Gesture, Input, PetDef, Rect, Screen, SexRatio, TICK, Variant, VariantDrawing, World,
    load_pet,
};
use mouflon_render::{DisplayList, Frame, PetArt, Style, paint_pet};

fn pet(name: &str) -> PetDef {
    let path = Path::new(env!("CARGO_MANIFEST_DIR")).join(format!("../../pets/{name}/pet.toml"));
    load_pet(&path).unwrap()
}

/// Un dessin d'une race (son dessin de base, ou celui de son agneau), avec
/// sa largeur et sa foulée.
fn drawing_of(name: &str, variant: Option<Variant>) -> VariantDrawing {
    let breed = pet(name);
    let def = variant.map_or(&breed, |v| breed.variant(v).unwrap());
    VariantDrawing {
        appearance: def.appearance.clone(),
        width: Some(def.physics.width),
        stride: Some(def.physics.stride),
        weeks: None,
    }
}

fn chimera(sex: SexRatio) -> Arc<PetDef> {
    let mut def = pet("awassi");
    def.sex_ratio = sex;
    def.set_variants(vec![
        (Variant::Female, drawing_of("bleu-du-maine", None)),
        (Variant::Lamb, drawing_of("southdown", Some(Variant::Lamb))),
    ])
    .unwrap();
    Arc::new(def)
}

fn world() -> World {
    World::new(vec![Screen {
        frame: Rect::new(0.0, 0.0, 1440.0, 900.0),
        usable: Rect::new(0.0, 25.0, 1440.0, 800.0),
        scale: 2.0,
    }])
}

#[test]
fn the_drawings_its_animals_can_wear_are_baked_with_it() {
    let def = chimera(SexRatio::new(0.5).unwrap());
    let art = PetArt::load(&def).unwrap();
    // Le bélier (dessin de base) et la brebis ; l'agneau a sa définition à
    // lui (`PetDef::as_lamb`), et son dessin avec elle.
    assert_eq!(
        art.variants.iter().map(|(v, _)| *v).collect::<Vec<_>>(),
        [Variant::Female]
    );
    let ewe = art.worn(Some(Variant::Female));
    assert!((ewe.height - pet("bleu-du-maine").physics.height).abs() < 1e-3);
    assert!(ewe.baked.is_some(), "la brebis se cuit en images");
    assert!(std::ptr::eq(art.worn(None), &art));
    assert!(std::ptr::eq(art.worn(Some(Variant::Male)), &art));
    let lamb = PetArt::load(&def.as_lamb(2.0).unwrap()).unwrap();
    let agnelle = pet("southdown");
    let agnelle = agnelle.variant(Variant::Lamb).unwrap();
    assert!((lamb.height - agnelle.physics.height).abs() < 1e-3);
    assert!(lamb.variants.is_empty());
}

#[test]
fn every_animal_is_painted_with_the_drawing_of_its_sex() {
    let world = world();
    let def = chimera(SexRatio::new(0.5).unwrap());
    let art = Arc::new(PetArt::load(&def).unwrap());
    let mut flock = Flock::new();
    for k in 0..8 {
        flock.spawn(Arc::clone(&def), k, &world, Some(200.0 + 140.0 * k as f32));
    }
    let input = Input {
        local_hour: 15.0,
        ..Input::default()
    };
    let mut worn = [false; 2];
    for step in 0..(12.0 / TICK) as usize {
        flock.advance(TICK, &world, &input);
        if step % 120 != 0 {
            continue;
        }
        for pet in flock.pets() {
            let own = PetArt::worn_by(&art, pet);
            // Le dessin peint est celui dont le moteur a pris la taille.
            assert!((own.height - pet.def().physics.height).abs() < 1e-3);
            worn[usize::from(pet.variant().is_some())] = true;
            let view = pet.view(1.0);
            let canvas = own.canvas();
            let mut list =
                DisplayList::new((canvas.width * 2.0) as u32, (canvas.height * 2.0) as u32);
            paint_pet(
                &own,
                &view,
                &Frame {
                    origin: view.ground - canvas.anchor,
                    scale: 2.0,
                },
                &Style::default(),
                &mut list,
            );
            assert!(!list.commands.is_empty());
        }
    }
    assert_eq!(worn, [true, true], "des béliers et des brebis");
}

#[test]
fn a_lamb_with_its_own_drawing_finds_its_mothers_udder() {
    let world = world();
    let def = chimera(SexRatio::FEMALE);
    let art = Arc::new(PetArt::load(&def).unwrap());
    let lamb_def = Arc::new(def.as_lamb(2.0).unwrap());
    let lamb_art = Arc::new(PetArt::load(&lamb_def).unwrap());
    let mut flock = Flock::new();
    let mother = flock.spawn(Arc::clone(&def), 33, &world, Some(450.0));
    let fit = |flock: &mut Flock, id: u32, art: &Arc<PetArt>| {
        let pet = flock.get_mut(id).unwrap();
        let art = PetArt::worn_by(art, pet);
        if let Some((udder, muzzle)) = art.nursing_points() {
            pet.set_nursing_points(udder, muzzle);
        }
    };
    fit(&mut flock, mother, &art);
    let input = Input {
        local_hour: 15.0,
        ..Input::default()
    };
    for _ in 0..360 {
        flock.advance(TICK, &world, &input);
    }
    assert!(flock.get_mut(mother).unwrap().force("stand"));
    let lamb = flock
        .spawn_lamb_with(mother, Arc::clone(&lamb_def), 51, &world)
        .expect("la brebis a son agneau");
    fit(&mut flock, lamb, &lamb_art);
    let mut nursed = false;
    for _ in 0..(90.0 / TICK) as usize {
        let alpha = flock.advance(TICK, &world, &input);
        let young = flock.pets().iter().find(|p| p.id() == lamb).unwrap();
        let pose = young.view(alpha).pose;
        if pose.gesture == Some(Gesture::Nurse) && pose.gesture_amount > 0.5 {
            nursed = true;
            break;
        }
    }
    assert!(nursed, "l'agneau Southdown tète la brebis Bleu du Maine");
}
