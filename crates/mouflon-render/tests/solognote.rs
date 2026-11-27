//! Test de bout en bout : la brebis peinte sur son squelette, la Solognote,
//! se charge, vit un moment et se dessine en images seulement (le rendu
//! direct la pose, sans aucun tracé), dans toutes ses poses.

use std::path::Path;
use std::sync::Arc;

use mouflon_core::{Flock, Input, Rect, Screen, Support, TICK, Vec2, World, load_pet};
use mouflon_render::display::Command;
use mouflon_render::{DisplayList, Frame, Look, PetArt, Style, paint_pet};

#[test]
fn the_solognote_loads_lives_and_paints_in_images_only() {
    let path = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../pets/solognote/pet.toml");
    let def = Arc::new(load_pet(&path).expect("pet.toml de la Solognote"));
    let art = PetArt::load(&def).expect("morceaux de la Solognote");
    let Look::Skeleton(skeleton) = &art.look else {
        panic!("un squelette attendu");
    };
    assert_eq!(art.height, 39.79);
    // Plus large que haute, comme une brebis.
    assert!(
        art.width > art.height * 1.2 && art.width < art.height * 1.9,
        "{} pour {}",
        art.width,
        art.height
    );
    // Réduits à la finesse de l'écran : quelques centaines de Ko, pas des Mo.
    assert!(
        skeleton.byte_size() < 2 << 20,
        "{} octets",
        skeleton.byte_size()
    );

    let world = World::new(vec![Screen {
        frame: Rect::new(0.0, 0.0, 1280.0, 800.0),
        usable: Rect::new(0.0, 25.0, 1280.0, 700.0),
        scale: 2.0,
    }]);
    let mut flock = Flock::new();
    let id = flock.spawn(Arc::clone(&def), 5, &world, Some(600.0));
    let input = Input {
        cursor: Some(Vec2::new(640.0, 600.0)),
        local_hour: 15.0,
        ..Input::default()
    };
    let canvas = art.canvas();
    let mut seen = std::collections::BTreeSet::new();
    let mut grounded = false;
    for step in 0..(90.0 / TICK) as usize {
        flock.advance(TICK, &world, &input);
        if step % 30 != 0 {
            continue;
        }
        let pet = flock.pets().iter().find(|p| p.id() == id).expect("animal");
        grounded |= matches!(pet.support(), Support::Ground { .. });
        let view = pet.view(1.0);
        seen.insert(view.behavior.to_owned());
        let mut list = DisplayList::new((canvas.width * 2.0) as u32, (canvas.height * 2.0) as u32);
        paint_pet(
            &art,
            &view,
            &Frame {
                origin: view.ground - canvas.anchor,
                scale: 2.0,
            },
            &Style::default(),
            &mut list,
        );
        let images = list
            .commands
            .iter()
            .filter(|c| matches!(c, Command::Image { .. }))
            .count();
        assert!(images >= 10, "{} : {images} images", view.behavior);
        assert!(
            list.commands
                .iter()
                .all(|c| !matches!(c, Command::Fill { .. } | Command::Stroke { .. })),
            "{} : des tracés",
            view.behavior
        );
        // Ses morceaux restent dans sa fenêtre.
        for command in &list.commands {
            if let Command::Image {
                bitmap, transform, ..
            } = command
            {
                let rect = transform.transform_rect_bbox(mouflon_render::kurbo::Rect::new(
                    0.0,
                    0.0,
                    f64::from(bitmap.width),
                    f64::from(bitmap.height),
                ));
                assert!(
                    rect.x0 > -4.0
                        && rect.y0 > -4.0
                        && rect.x1 < list.width as f64 + 4.0
                        && rect.y1 < list.height as f64 + 4.0,
                    "{} : morceau hors de la fenêtre {rect:?}",
                    view.behavior
                );
            }
        }
    }
    assert!(grounded, "jamais posée");
    assert!(seen.len() >= 3, "{seen:?}");
}
