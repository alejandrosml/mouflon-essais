//! Test de bout en bout de tous les animaux du dépôt (`pets/`) : chacun se
//! charge, vit un moment (actif, pour qu'il joue aussi) et se dessine. Les
//! moutons dessinés en vecteurs ont toutes leurs parties (l'encolure et les
//! pattes en deux segments comprises), se cuisent en images, leurs sabots
//! touchent le sol, et ils broutent le museau dans l'herbe. Chacun de leurs
//! dessins par sexe et par âge (`[appearance.female]`…) est vérifié comme
//! le dessin de base.

use std::path::{Path, PathBuf};
use std::sync::Arc;

use mouflon_core::{Flock, Input, Mood, PetDef, Rect, Screen, TICK, World, load_pet};
use mouflon_render::animate::pose;
use mouflon_render::kurbo::Point;
use mouflon_render::rig::Part;
use mouflon_render::{DisplayList, Frame, Look, PetArt, Style, paint_pet};

fn pets() -> Vec<PathBuf> {
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../pets");
    let mut found: Vec<PathBuf> = std::fs::read_dir(&root)
        .expect("dossier pets")
        .filter_map(Result::ok)
        .map(|entry| entry.path().join("pet.toml"))
        .filter(|path| path.exists())
        .collect();
    found.sort();
    found
}

#[test]
fn every_pet_of_the_repository_loads_lives_and_paints() {
    let world = World::new(vec![Screen {
        frame: Rect::new(0.0, 0.0, 1440.0, 900.0),
        usable: Rect::new(0.0, 25.0, 1440.0, 800.0),
        scale: 2.0,
    }]);
    let paths = pets();
    assert!(paths.len() >= 4, "{paths:?}");
    for path in paths {
        let def = Arc::new(load_pet(&path).unwrap_or_else(|e| panic!("{path:?} : {e}")));
        let art = PetArt::load(&def).unwrap_or_else(|e| panic!("{path:?} : {e}"));
        let mut flock = Flock::new();
        flock.set_mood(Mood::Lively);
        for x in [400.0, 520.0, 900.0] {
            flock.spawn(Arc::clone(&def), x as u64, &world, Some(x));
        }
        let input = Input {
            local_hour: 15.0,
            ..Input::default()
        };
        for step in 0..(40.0 / TICK) as usize {
            flock.advance(TICK, &world, &input);
            if step % 60 != 0 {
                continue;
            }
            for pet in flock.pets() {
                // Le dessin de son sexe et de son âge, s'il en a un.
                let art = art.worn(pet.variant());
                let canvas = art.canvas();
                let view = pet.view(1.0);
                let mut list =
                    DisplayList::new((canvas.width * 2.0) as u32, (canvas.height * 2.0) as u32);
                paint_pet(
                    art,
                    &view,
                    &Frame {
                        origin: view.ground - canvas.anchor,
                        scale: 2.0,
                    },
                    &Style::default(),
                    &mut list,
                );
                assert!(
                    !list.commands.is_empty(),
                    "{} ne dessine rien ({})",
                    def.meta.name,
                    view.behavior
                );
            }
        }
    }
}

/// Chaque dessin d'un animal, seul : celui de base, puis ceux de son sexe
/// et de son âge (`[appearance.male]`, `female`, `lamb`), avec leur nom.
fn drawings(def: &PetDef) -> Vec<(String, PetDef)> {
    let mut base = def.clone();
    base.set_variants(Vec::new()).unwrap();
    let mut all = vec![(def.meta.name.clone(), base)];
    for (variant, drawn) in def.variants() {
        all.push((
            format!("{} ({})", def.meta.name, variant.name()),
            (**drawn).clone(),
        ));
    }
    all
}

#[test]
fn every_drawn_sheep_has_its_parts_bakes_and_stands_on_its_hooves() {
    for path in pets() {
        let def = load_pet(&path).unwrap_or_else(|e| panic!("{path:?} : {e}"));
        for (name, drawn) in drawings(&def) {
            drawn_sheep_has_its_parts_bakes_and_stands_on_its_hooves(&name, &drawn);
        }
    }
}

fn drawn_sheep_has_its_parts_bakes_and_stands_on_its_hooves(name: &str, def: &PetDef) {
    let art = PetArt::load(def).unwrap_or_else(|e| panic!("{name} : {e}"));
    let Look::Rig(rig) = &art.look else {
        return;
    };
    // Il se cuit en images, un petit nombre de morceaux (le rendu les
    // pose sans redessiner ses milliers de tracés à chaque image).
    let baked = art
        .baked
        .as_ref()
        .unwrap_or_else(|| panic!("{name} ne se cuit pas"));
    assert!(
        !baked.is_empty() && baked.len() <= 32,
        "{name} : {} morceaux",
        baked.len()
    );
    for part in [
        Part::Body,
        Part::Neck,
        Part::Head,
        Part::Eyes,
        Part::Pupils,
        Part::LegFrontNear,
        Part::LegFrontFar,
        Part::LegBackNear,
        Part::LegBackFar,
        Part::LegFrontNearLower,
        Part::LegFrontFarLower,
        Part::LegBackNearLower,
        Part::LegBackFarLower,
        Part::Lip,
    ] {
        assert!(rig.has_part(part), "{name} : pas de « {} »", part.name());
    }
    // Les mèches de la toison vont par trois.
    let sways = Part::SWAYS.map(|p| rig.has_part(p));
    assert!(sways.iter().all(|&s| s == sways[0]), "{name} : {sways:?}");
    // Il broute le museau dans l'herbe (à 2 % de sa hauteur près), devant
    // ses antérieurs.
    let muzzle = rig
        .muzzle
        .unwrap_or_else(|| panic!("{name} : pas de « pivot-muzzle »"));
    let grazing = pose(
        rig,
        &mouflon_core::PoseSignals {
            graze: 1.0,
            ..Default::default()
        },
        1.0,
        0.0,
    );
    let mut head = grazing.part(Part::Head);
    let mut up = rig.parent(Part::Head);
    while let Some(parent) = up {
        head = grazing.part(parent) * head;
        up = rig.parent(parent);
    }
    let m = grazing.root * head * muzzle;
    let h = rig.bounds.height();
    assert!(
        (m.y - rig.ground.y).abs() < 0.02 * h,
        "{name} : museau à {:.1} du sol",
        rig.ground.y - m.y
    );
    assert!(
        m.x > rig.pivot(Part::LegFrontNearLower).x,
        "{name} : museau derrière les antérieurs"
    );
    // Debout, le bas de chaque patte est sur la ligne du sol (à 1 % de
    // sa hauteur près).
    let h = rig.bounds.height();
    let p = pose(rig, &Default::default(), 1.0, 0.0);
    for part in [
        Part::LegFrontNear,
        Part::LegFrontFar,
        Part::LegBackNear,
        Part::LegBackFar,
    ] {
        let b = rig.part_bounds(part);
        let hoof = p.root * p.part(part) * Point::new(b.center().x, b.max_y());
        assert!(
            (hoof.y - rig.ground.y).abs() < h * 0.01,
            "{name} : {} à {:.1}, le sol à {:.1}",
            part.name(),
            hoof.y,
            rig.ground.y
        );
    }
    // Le personnage se tient au-dessus de son point d'appui.
    assert!(rig.bounds.max_y() <= rig.ground.y + h * 0.01, "{name}");
}
