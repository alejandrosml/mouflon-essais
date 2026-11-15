//! Test de bout en bout : l'animal intégré à l'app, la brebis Bleu du Maine,
//! se charge, se simule et se dessine sans erreur, et la scène produite
//! n'est pas vide.

use std::path::Path;
use std::sync::Arc;

use mouflon_core::{Input, Pet, Rect, Screen, TICK, Vec2, World, load_pet};
use mouflon_render::display::Command;
use mouflon_render::{DisplayList, Frame, PetArt, Style, paint_pet};

fn bleu_du_maine() -> Arc<mouflon_core::PetDef> {
    let path = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../pets/bleu-du-maine/pet.toml");
    Arc::new(load_pet(&path).expect("pet.toml du Bleu du Maine"))
}

#[test]
fn the_bleu_du_maine_loads_simulates_and_paints() {
    let def = bleu_du_maine();
    let mut art = PetArt::load(&def).expect("dessin du Bleu du Maine");
    assert!(art.width > 0.0 && art.height > 0.0);
    let baked = art
        .baked
        .clone()
        .expect("le Bleu du Maine se cuit en images");
    assert!(baked.len() >= 10, "{} morceaux", baked.len());

    let world = World::new(vec![Screen {
        frame: Rect::new(0.0, 0.0, 1280.0, 800.0),
        usable: Rect::new(0.0, 25.0, 1280.0, 700.0),
        scale: 2.0,
    }]);
    let mut pet = Pet::spawn(0, Arc::clone(&def), 11, &world, Some(600.0));
    let input = Input {
        cursor: Some(Vec2::new(640.0, 600.0)),
        local_hour: 23.0,
        ..Input::default()
    };
    for _ in 0..(20.0 / TICK) as usize {
        pet.step(TICK, &world, &input);
    }

    let canvas = art.canvas();
    let view = pet.view(0.5);
    let origin = view.ground - canvas.anchor;
    let paint = |art: &PetArt| {
        let mut list = DisplayList::new((canvas.width * 2.0) as u32, (canvas.height * 2.0) as u32);
        paint_pet(
            art,
            &view,
            &Frame { origin, scale: 2.0 },
            &Style::default(),
            &mut list,
        );
        let count = |f: fn(&Command) -> bool| list.commands.iter().filter(|c| f(c)).count();
        let fills = count(|c| matches!(c, Command::Fill { .. }));
        let images = count(|c| matches!(c, Command::Image { .. }));
        (list, fills, images)
    };
    // Cuit : ses morceaux sont des images, sans un seul tracé.
    let (_, fills, images) = paint(&art);
    assert_eq!(fills, 0);
    assert!(images >= 10, "seulement {images} images posées");
    // Tel quel : ses tracés.
    art.baked = None;
    let (list, fills, _) = paint(&art);
    assert!(fills > 20, "seulement {fills} formes dessinées");

    let svg = mouflon_render::svg::to_svg(&list);
    assert!(svg.starts_with("<svg") && svg.ends_with("</svg>"));
}

/// Il broute : la tête descend en une seconde ou deux et grignote.
#[test]
fn the_bleu_du_maine_grazes_with_its_head_down() {
    let def = bleu_du_maine();
    let world = World::new(vec![Screen {
        frame: Rect::new(0.0, 0.0, 1280.0, 800.0),
        usable: Rect::new(0.0, 25.0, 1280.0, 700.0),
        scale: 2.0,
    }]);
    let mut pet = Pet::spawn(0, Arc::clone(&def), 5, &world, Some(600.0));
    let input = Input::default();
    for _ in 0..(3.0 / TICK) as usize {
        pet.step(TICK, &world, &input);
    }
    assert!(pet.force("graze"), "il sait brouter");
    let mut bites = Vec::new();
    for _ in 0..(2.5 / TICK) as usize {
        pet.step(TICK, &world, &input);
        bites.push(pet.view(1.0).pose.nibble);
    }
    let view = pet.view(1.0);
    assert!(view.pose.graze > 0.9, "tête basse : {}", view.pose.graze);
    let (lo, hi) = bites[bites.len() / 2..]
        .iter()
        .fold((f32::MAX, f32::MIN), |(lo, hi), &b| (lo.min(b), hi.max(b)));
    assert!(hi - lo > 1.0, "il grignote ({lo} à {hi})");
}
