//! Les ombres : au contact, une ombre sombre et serrée sous les pattes, et
//! autour une ombre douce ; en l'air, l'ombre portée s'élargit, pâlit et
//! devient plus floue ; elle reste sur sa surface. Toujours les deux mêmes
//! formes à l'échelle près : une image cuite pour chacune, partagée par tous.

use std::path::Path;
use std::sync::Arc;

use mouflon_core::{Input, Pet, PetView, Rect, Screen, Shadow, Support, TICK, World, load_pet};
use mouflon_render::display::Command;
use mouflon_render::{DisplayList, Frame, PetArt, Style, paint_pet};

/// Une ombre dessinée : sa forme et son opacité.
#[derive(Clone, Copy, Debug)]
struct Drawn {
    x0: f64,
    x1: f64,
    h: f64,
    std_dev: f64,
    alpha: f32,
}

impl Drawn {
    fn w(&self) -> f64 {
        self.x1 - self.x0
    }
}

fn shadows(art: &PetArt, view: &PetView<'_>) -> Vec<Drawn> {
    let canvas = art.canvas();
    let mut list = DisplayList::new((canvas.width * 2.0) as u32, (canvas.height * 2.0) as u32);
    // Repère de l'écran, en points.
    paint_pet(
        art,
        view,
        &Frame {
            origin: mouflon_core::Vec2::ZERO,
            scale: 1.0,
        },
        &Style::default(),
        &mut list,
    );
    list.commands
        .iter()
        .filter_map(|c| match c {
            Command::BlurredRect {
                rect,
                std_dev,
                color,
                ..
            } => Some(Drawn {
                x0: rect.x0,
                x1: rect.x1,
                h: rect.height(),
                std_dev: *std_dev,
                alpha: color.components[3],
            }),
            _ => None,
        })
        .collect()
}

/// Le Bleu du Maine posé au sol (il n'a pas d'ombre dans ses images :
/// Mouflon dessine la sienne, au sol comme en l'air).
fn sheep() -> (Pet, PetArt) {
    let path = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../pets/bleu-du-maine/pet.toml");
    let def = Arc::new(load_pet(&path).expect("pet.toml du Bleu du Maine"));
    let art = PetArt::load(&def).expect("dessin du Bleu du Maine");
    let world = World::new(vec![Screen {
        frame: Rect::new(0.0, 0.0, 1280.0, 800.0),
        usable: Rect::new(0.0, 25.0, 1280.0, 700.0),
        scale: 2.0,
    }]);
    let mut pet = Pet::spawn(0, def, 11, &world, Some(600.0));
    for _ in 0..(5.0 / TICK) as usize {
        pet.step(TICK, &world, &Input::default());
    }
    assert!(matches!(pet.support(), Support::Ground { .. }));
    assert!(pet.force("stand"));
    (pet, art)
}

#[test]
fn a_shadow_spreads_fades_and_blurs_as_the_pet_rises() {
    let (pet, art) = sheep();
    let mut view = pet.view(1.0);
    let ground = view.ground.y;
    let height = f64::from(art.body.1);

    // Au sol : l'ombre portée, et l'ombre de contact, plus serrée et plus
    // nette, sous les pattes.
    let on_ground = shadows(&art, &view);
    assert_eq!(on_ground.len(), 2, "{on_ground:?}");
    let (cast, contact) = (on_ground[0], on_ground[1]);
    assert!(contact.w() < cast.w() * 0.7);
    assert!(contact.std_dev / contact.h < cast.std_dev / cast.h);

    // En l'air, à la moitié de sa hauteur : plus de contact ; l'ombre
    // portée est plus large, plus floue et plus pâle.
    view.support = Support::Air;
    let at = |view: &mut PetView<'_>, distance: f64| {
        view.shadow = Some(Shadow::on(ground, distance as f32));
        view.ground.y = ground - distance as f32;
        shadows(&art, view)
    };
    let up = at(&mut view, height * 0.5);
    assert_eq!(up.len(), 1, "{up:?}");
    let up = up[0];
    assert!(up.w() > cast.w() * 1.1, "{} pour {}", up.w(), cast.w());
    assert!(up.std_dev > cast.std_dev);
    assert!(up.alpha < cast.alpha * 0.8);
    // Plus haut encore, encore plus pâle ; très haut, plus rien.
    let higher = at(&mut view, height * 3.0)[0];
    assert!(higher.alpha < up.alpha * 0.5 && higher.w() > up.w());
    assert!(at(&mut view, 3000.0).is_empty());

    // Toujours la même forme à l'échelle près, flou compris : une seule
    // image cuite pour toutes les ombres portées.
    for s in [cast, up, higher] {
        assert!((s.w() / s.h - cast.w() / cast.h).abs() < 1e-6);
        assert!((s.std_dev / s.h - cast.std_dev / cast.h).abs() < 1e-6);
    }
}

#[test]
fn a_shadow_stays_on_its_ledge() {
    let (pet, art) = sheep();
    let mut view = pet.view(1.0);
    let x = f64::from(view.ground.x);
    let y = view.ground.y;
    let whole = shadows(&art, &view)[0];
    let half = whole.w() / 2.0;

    // L'animal au bout d'un rebord : seule la part qui tombe dessus reste,
    // plus petite et plus pâle, sans déborder dans le vide.
    view.shadow = Some(Shadow {
        y,
        distance: 0.0,
        x0: (x - 400.0) as f32,
        x1: x as f32,
    });
    let at_end = shadows(&art, &view);
    assert!(!at_end.is_empty());
    for s in &at_end {
        assert!(s.x1 <= x + 1e-3, "{s:?} déborde du rebord (bout à {x})");
    }
    let cut = at_end[0];
    assert!((cut.w() - half).abs() < 1.0, "{} pour {half}", cut.w());
    assert!(cut.alpha < whole.alpha);

    // Tout entier au-delà du bout : plus d'ombre sur ce rebord.
    view.shadow = Some(Shadow {
        y,
        distance: 0.0,
        x0: (x - 400.0) as f32,
        x1: (x - half - 1.0) as f32,
    });
    assert!(shadows(&art, &view).is_empty());
}

#[test]
fn a_tumbling_body_casts_the_shadow_of_its_turned_shape() {
    let (pet, art) = sheep();
    let mut view = pet.view(1.0);
    let upright = shadows(&art, &view)[0].w();
    // Debout sur la tête ou sur le flanc : ce qui touche le sol change, et
    // l'ombre avec.
    view.pose.roll = std::f32::consts::FRAC_PI_2;
    let on_its_side = shadows(&art, &view)[0].w();
    let (half, height) = (f64::from(art.body.0), f64::from(art.body.1));
    let expected = upright * (0.45 * height) / (half * 0.92);
    assert!(
        (on_its_side - expected).abs() < 1.0,
        "{on_its_side} au lieu de {expected}"
    );
}
