//! Pelote, gardé comme donnée de test (`tests/donnees/pelote`) : un animal
//! desktopPet redessiné, dont le fichier XML a perdu son image au profit
//! d'une planche de 240 pixels.

use std::path::Path;
use std::sync::Arc;

use mouflon_core::{Input, Pet, Rect, Screen, Shadow, Support, TICK, World, load_pet};
use mouflon_render::display::Command;
use mouflon_render::{DisplayList, Frame, Look, PetArt, SpriteKey, Style, paint_pet};

#[test]
fn pelote_loads_at_80_points_and_paints_both_ways() {
    let path = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../tests/donnees/pelote/pet.toml");
    let def = Arc::new(load_pet(&path).expect("pet.toml de Pelote"));
    assert_eq!(def.meta.name, "Pelote");
    let art = PetArt::load(&def).expect("planche de Pelote");
    let Look::Sprites(frames) = &art.look else {
        panic!("Pelote est fait d'images")
    };
    // 12 × 10 cases de 240 pixels, affichées en 80 points (160 pixels Retina).
    assert_eq!((art.width, art.height), (80.0, 80.0));
    assert!(frames.frames.len() >= 119, "{}", frames.frames.len());
    assert!(frames.frames.iter().all(|f| f.bitmap.width <= 160));
    // Ses images intermédiaires suivent la planche : la marche (images 1 et
    // 0) en a trois dans chaque sens.
    let sheet = def.sprites().expect("planche");
    let (first, count) = sheet.inbetween(1, 0).expect("marche adoucie");
    assert_eq!(count, 3);
    assert!(first >= 120 && (first + count) as usize <= frames.frames.len());
    // Ses sons : bêlements, clochette, broutage, bain, atterrissage ; le
    // trèfle se broute en musique.
    assert_eq!(def.sounds.len(), 5);
    assert!(def.sounds.iter().all(|s| !s.clips.is_empty()));
    let eat = &def.behaviors[def.behavior_index("eat").expect("eat")];
    assert_eq!(def.sounds[eat.sounds[0].sound].name, "broute");

    let world = World::new(vec![Screen {
        frame: Rect::new(0.0, 0.0, 1280.0, 800.0),
        usable: Rect::new(0.0, 25.0, 1280.0, 700.0),
        scale: 2.0,
    }]);
    let mut pet = Pet::spawn(0, Arc::clone(&def), 5, &world, Some(600.0));
    let input = Input::default();
    for _ in 0..(6.0 / TICK) as usize {
        pet.step(TICK, &world, &input);
    }
    assert!(matches!(pet.support(), Support::Ground { .. }));
    for facing in [-1.0, 1.0] {
        assert!(pet.force("walk"));
        let mut view = pet.view(1.0);
        view.facing = facing;
        let canvas = art.canvas();
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
        assert_eq!(images, 1, "une image par animal");
    }
}

/// Pelote ne se redessine que quand son image change à l'œil : ni pour une
/// inclinaison qui ne bouge pas (au plafond, contre un mur), ni pour une ombre
/// trop loin pour se voir.
#[test]
fn pelote_is_redrawn_only_when_its_image_changes() {
    let path = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../tests/donnees/pelote/pet.toml");
    let def = Arc::new(load_pet(&path).expect("pet.toml de Pelote"));
    let art = PetArt::load(&def).expect("planche de Pelote");
    let world = World::new(vec![Screen {
        frame: Rect::new(0.0, 0.0, 1280.0, 800.0),
        usable: Rect::new(0.0, 25.0, 1280.0, 700.0),
        scale: 2.0,
    }]);
    let mut pet = Pet::spawn(0, Arc::clone(&def), 5, &world, Some(600.0));
    for _ in 0..(6.0 / TICK) as usize {
        pet.step(TICK, &world, &Input::default());
    }
    assert!(pet.force("walk"));
    let mut view = pet.view(1.0);
    let key = |view: &mouflon_core::PetView<'_>| SpriteKey::of(&art, view).expect("en images");

    // Au sol, un reste d'inclinaison sous le seuil du repos : l'image se pose
    // au pixel près de toute façon.
    assert!(matches!(view.support, Support::Ground { .. }));
    view.pose.lean = 0.001;
    let still = key(&view);
    view.pose.lean = 0.002;
    assert_eq!(key(&view), still, "toujours au repos");

    // Au plafond : penché d'un rien, toujours pareil.
    view.support = Support::Air;
    view.pose.lean = 0.0076;
    let tilted = key(&view);
    view.pose.lean = 0.00762;
    assert_eq!(key(&view), tilted, "même inclinaison à l'œil");
    view.pose.lean = 0.02;
    assert_ne!(key(&view), tilted, "inclinaison qui se voit");

    // Une ombre à 600 points sous l'animal ne se voit pas : qu'elle s'éloigne
    // ou se rapproche d'un point ne change rien ; à 100 points, si.
    let mut shadow_at = |distance: f32| {
        view.shadow = Some(Shadow::on(view.ground.y + distance, distance));
        key(&view)
    };
    assert_eq!(shadow_at(600.0), shadow_at(601.0));
    assert_ne!(shadow_at(100.0), shadow_at(101.0));
}
