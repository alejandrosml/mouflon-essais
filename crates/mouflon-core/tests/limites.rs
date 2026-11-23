//! Cas limites du monde et du pas fixe : aucun état non fini, et aucun
//! aller-retour entre deux bords qu'un animal trop large touche à la fois.

use std::sync::Arc;

use mouflon_core::physics::Tracker;
use mouflon_core::{
    Flock, Input, LedgeOptions, Pet, PetDef, Rect, Screen, Support, TICK, Vec2, WindowRect, World,
    parse_pet_toml,
};

fn definition() -> PetDef {
    parse_pet_toml(
        r#"
        [pet]
        id = "limites"
        name = "Limites"
        [appearance]
        rig = "mouton.svg"
        height = 60.0
        [physics]
        width = 64.0
        bounce = 0.0
        [reflexes]
        spawn = "fall"
        fall = "fall"
        land = "stand"
        [[behavior]]
        name = "stand"
        motion = "stand"
        duration = "forever"
        [[behavior]]
        name = "walk"
        motion = "walk"
        speed = 60.0
        duration = "forever"
        [[behavior]]
        name = "fall"
        motion = "fall"
        duration = "forever"
        "#,
        |_: &str| Ok(b"<svg/>".to_vec()),
    )
    .unwrap()
}

fn world() -> World {
    World::new(vec![Screen {
        frame: Rect::new(100.0, 0.0, 200.0, 900.0),
        usable: Rect::new(100.0, 25.0, 200.0, 800.0),
        scale: 2.0,
    }])
}

#[test]
fn invalid_elapsed_times_do_not_poison_the_next_fixed_step() {
    for realistic in [false, true] {
        let world = world();
        let input = Input::default();
        let mut actual = Flock::new();
        actual.set_realistic(realistic);
        actual.spawn(Arc::new(definition()), 17, &world, Some(200.0));
        let mut expected = actual.clone();
        actual.advance(TICK * 0.5, &world, &input);
        expected.advance(TICK * 0.5, &world, &input);
        let before = actual.pets()[0].position();
        for elapsed in [f32::NAN, f32::INFINITY, f32::NEG_INFINITY, -TICK] {
            assert_eq!(actual.advance(elapsed, &world, &input), 0.5);
            assert_eq!(actual.pets()[0].position(), before);
        }
        for _ in 0..120 {
            let alpha = actual.advance(TICK, &world, &input);
            assert_eq!(alpha, expected.advance(TICK, &world, &input));
            assert_eq!(actual.pets()[0].position(), expected.pets()[0].position());
            assert_eq!(actual.pets()[0].velocity(), expected.pets()[0].velocity());
        }
        assert_ne!(actual.pets()[0].position(), before);
    }
}

#[test]
fn irregular_frames_keep_the_same_fixed_steps() {
    let world = world();
    let input = Input::default();
    let mut regular = Flock::new();
    regular.spawn(Arc::new(definition()), 23, &world, Some(200.0));
    let mut irregular = regular.clone();
    for _ in 0..120 {
        for _ in 0..4 {
            regular.advance(TICK, &world, &input);
        }
        for elapsed in [TICK * 0.25, TICK * 0.75, TICK * 3.0] {
            irregular.advance(elapsed, &world, &input);
        }
        assert_eq!(regular.pets()[0].position(), irregular.pets()[0].position());
        assert_eq!(regular.pets()[0].velocity(), irregular.pets()[0].velocity());
    }
}

#[test]
fn a_pet_wider_than_the_screen_stays_centered_in_the_air_and_on_the_ground() {
    for realistic in [false, true] {
        let world = world();
        let input = Input::default();
        let mut def = definition();
        def.physics.width = 320.0;
        let mut flock = Flock::new();
        flock.set_realistic(realistic);
        let id = flock.spawn(Arc::new(def), 31, &world, Some(150.0));
        for _ in 0..360 {
            flock.advance(TICK, &world, &input);
            let pet = &flock.pets()[0];
            assert_eq!(pet.position().x, 200.0, "réaliste = {realistic}");
            assert_eq!(pet.velocity().x, 0.0);
            assert!(pet.position().y.is_finite());
        }
        assert!(matches!(flock.pets()[0].support(), Support::Ground { .. }));
        assert!(flock.get_mut(id).unwrap().force("walk"));
        let facing = flock.pets()[0].view(1.0).facing;
        for _ in 0..120 {
            flock.advance(TICK, &world, &input);
            assert_eq!(flock.pets()[0].position().x, 200.0);
            assert_eq!(flock.pets()[0].velocity().x, 0.0);
            assert_eq!(flock.pets()[0].view(1.0).facing, facing);
        }
    }
}

#[test]
fn non_finite_scales_leave_the_definition_unchanged() {
    let def = definition();
    for scale in [f32::NAN, f32::INFINITY, f32::NEG_INFINITY, 0.0, -1.0] {
        let scaled = def.scaled(scale);
        assert_eq!(scaled.physics, def.physics);
        assert_eq!(scaled.unit, def.unit);
        for (actual, expected) in scaled.behaviors.iter().zip(&def.behaviors) {
            assert_eq!(actual.motion, expected.motion);
        }
    }
}

#[test]
fn a_non_finite_resize_does_not_change_a_live_pet() {
    let world = world();
    let input = Input::default();
    let mut flock = Flock::new();
    let def = Arc::new(definition());
    let id = flock.spawn(Arc::clone(&def), 41, &world, Some(200.0));
    flock.advance(TICK, &world, &input);
    let pet = flock.get_mut(id).unwrap();
    let (position, velocity) = (pet.position(), pet.velocity());
    for ratio in [f32::NAN, f32::INFINITY, f32::NEG_INFINITY, 0.0, -1.0] {
        assert!(!pet.rescale(Arc::clone(&def), ratio));
        assert_eq!(pet.position(), position);
        assert_eq!(pet.velocity(), velocity);
        assert!(Arc::ptr_eq(pet.def(), &def));
    }
}

#[test]
fn small_and_large_pets_land_without_non_finite_positions() {
    for realistic in [false, true] {
        for scale in [0.01, 1.0, 8.0] {
            let world = world();
            let input = Input::default();
            let mut tracker = Tracker::default();
            let mut pet = Pet::spawn_at(
                0,
                Arc::new(definition().scaled(scale)),
                47,
                &world,
                Vec2::new(180.0, 823.0),
            );
            for _ in 0..120 {
                if realistic {
                    tracker.update(&world, TICK, &[]);
                    pet.step_real(TICK, &tracker, &input);
                } else {
                    pet.step(TICK, &world, &input);
                }
                assert!(pet.position().x.is_finite() && pet.position().y.is_finite());
                assert!(pet.velocity().length().is_finite());
                assert!(pet.body().angle.is_finite());
            }
            assert!(matches!(pet.support(), Support::Ground { .. }));
            assert!((pet.position().y - 825.0).abs() < 0.01);
        }
    }
}

#[test]
fn non_finite_body_sizes_are_rejected_at_load_time() {
    for field in ["width", "height"] {
        for value in ["inf", "-inf", "nan", "0.0", "-1.0"] {
            let source = format!(
                r#"
                [pet]
                id = "taille-invalide"
                name = "Taille invalide"
                [appearance]
                rig = "mouton.svg"
                height = 60.0
                [physics]
                {field} = {value}
                [[behavior]]
                name = "stand"
                motion = "stand"
                duration = "forever"
                "#
            );
            assert!(
                parse_pet_toml(&source, |_: &str| Ok(b"<svg/>".to_vec())).is_err(),
                "{field} = {value}"
            );
        }
    }
}

/// Une fenêtre déborde de l'écran : son côté est dehors. Un animal lâché
/// au-delà de ce côté (on l'a tiré contre le bord de l'écran) revient dans
/// l'écran ; le côté de la fenêtre, qu'il ne franchit pas de lui-même, ne
/// le renvoie pas dehors.
#[test]
fn a_pet_dropped_past_a_window_side_beyond_the_screen_edge_comes_back() {
    for realistic in [true, false] {
        let mut world = World::new(vec![Screen {
            frame: Rect::new(0.0, 0.0, 1000.0, 800.0),
            usable: Rect::new(0.0, 25.0, 1000.0, 735.0),
            scale: 2.0,
        }]);
        // Elle dépasse de 30 points à droite (son côté droit est à 1 030)
        // et descend sous le sol.
        world.set_windows(
            &[WindowRect {
                id: 7,
                frame: Rect::new(600.0, 300.0, 430.0, 500.0),
            }],
            LedgeOptions::default(),
        );
        let mut input = Input {
            cursor: Some(Vec2::new(500.0, 500.0)),
            local_hour: 15.0,
            ..Input::default()
        };
        let mut flock = Flock::new();
        flock.set_realistic(realistic);
        let id = flock.spawn(std::sync::Arc::new(definition()), 3, &world, Some(900.0));
        for _ in 0..240 {
            flock.advance(TICK, &world, &input);
        }
        // On l'attrape et on le tire au-delà du bord, plus loin que le côté
        // de la fenêtre, à mi-hauteur de celle-ci.
        let mut at = flock.pets()[0].position() - Vec2::new(0.0, 20.0);
        flock.get_mut(id).unwrap().grab(at, &world);
        for _ in 0..90 {
            at = at + (Vec2::new(1110.0, 520.0) - at) * 0.1;
            input.cursor = Some(at);
            flock.get_mut(id).unwrap().drag_to(at);
            flock.advance(TICK, &world, &input);
        }
        assert!(flock.pets()[0].position().x > 1080.0, "pas assez loin");
        flock.get_mut(id).unwrap().release(&world);
        // Dès le premier pas, il est dans l'écran (son dessin y glisse).
        flock.advance(TICK, &world, &input);
        let x = flock.pets()[0].position().x;
        assert!(
            (0.0..=1000.0).contains(&x),
            "réaliste {realistic} : encore dehors au premier pas, à {x}"
        );
        for _ in 0..(3.0 / TICK) as usize {
            flock.advance(TICK, &world, &input);
        }
        let pet = &flock.pets()[0];
        let x = pet.position().x;
        assert!(
            (0.0..=1000.0).contains(&x),
            "réaliste {realistic} : resté dehors, à {:?} ({}, {:?})",
            pet.position(),
            pet.behavior_name(),
            pet.support()
        );
        assert!(
            matches!(pet.support(), Support::Ground { .. }),
            "réaliste {realistic} : pas posé ({:?})",
            pet.support()
        );
    }
}
