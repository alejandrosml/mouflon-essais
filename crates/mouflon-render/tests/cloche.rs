//! Le redon du Mérinos d'Arles, une grosse sonnaille pendue au collier :
//! dessiné dans l'encolure, il tournait avec elle et passait sous l'herbe
//! dès qu'on l'aurait pendu droit (de 5 cm en broutant à fond, de 20 cm
//! endormi la tête posée, où il passait de 16 cm même en tournant avec le
//! cou). C'est maintenant une partie à lui (`bell`) : son anse suit le
//! collier, il pend à la verticale, se balance, et se pose sur l'herbe.

use std::path::Path;
use std::sync::Arc;

use mouflon_core::{
    Flock, Gesture, Input, Mood, PoseSignals, Rect, Screen, TICK, Vec2 as CoreVec2, World, load_pet,
};
use mouflon_render::PetArt;
use mouflon_render::animate::{PoseTransforms, pose};
use mouflon_render::kurbo::{Affine, BezPath, Point, Shape};
use mouflon_render::rig::{Item, Part, Rig};

/// Le floucat, le mâle castré du Mérinos d'Arles qui porte le redon.
fn merino() -> Rig {
    Rig::parse(include_bytes!("../../../pets/floucat/floucat.svg")).unwrap()
}

/// Une partie dans le repère de l'écran (la racine comprise).
fn absolute(rig: &Rig, pose: &PoseTransforms, part: Part) -> Affine {
    let mut transform = pose.part(part);
    let mut parent = rig.parent(part);
    while let Some(part) = parent {
        transform = pose.part(part) * transform;
        parent = rig.parent(part);
    }
    pose.root * transform
}

/// Les tracés d'une partie (et de ce qu'elle contient), dans le repère du
/// dessin.
fn drawn(rig: &Rig, part: Part) -> Vec<BezPath> {
    let mut inside = Vec::new();
    let mut out = Vec::new();
    for item in &rig.items {
        match item {
            Item::BeginPart(p) => inside.push(*p),
            Item::EndPart => {
                inside.pop();
            }
            Item::Fill {
                path, transform, ..
            }
            | Item::Stroke {
                path, transform, ..
            } if inside.contains(&part) => out.push(*transform * path.clone()),
            _ => {}
        }
    }
    out
}

/// De combien le redon passe sous le sol (en centièmes de la hauteur du
/// dessin ; négatif : au-dessus), d'après ses vrais tracés. Le sol est la
/// ligne de son point d'appui, à l'écran.
fn below(rig: &Rig, paths: &[BezPath], posed: &PoseTransforms) -> f64 {
    let at = absolute(rig, posed, Part::Bell);
    let lowest = paths
        .iter()
        .map(|path| (at * path.clone()).bounding_box().max_y())
        .fold(f64::NEG_INFINITY, f64::max);
    (lowest - rig.ground.y) / (rig.bounds.height() / 100.0)
}

/// De combien une transformation penche à l'écran (radians, positif : le
/// bas vers l'arrière de l'animal, quel que soit son sens).
fn tilt(t: Affine) -> f64 {
    let [a, b, ..] = t.as_coeffs();
    // Retourné, le dessin est en miroir : l'arrière est à droite.
    if t.determinant() < 0.0 {
        b.atan2(-a)
    } else {
        b.atan2(a)
    }
}

/// De combien penchent le redon et l'encolure.
fn tilts(rig: &Rig, posed: &PoseTransforms) -> (f64, f64) {
    (
        tilt(absolute(rig, posed, Part::Bell)),
        tilt(absolute(rig, posed, Part::Neck)),
    )
}

/// Toutes les poses où le cou descend, se couche, se tend ou se relève, et
/// où l'animal bouge d'un bloc : de quoi chercher le redon sous l'herbe.
fn poses() -> Vec<(String, PoseSignals)> {
    let mut out = vec![("repos".to_owned(), PoseSignals::default())];
    for k in 0..=20 {
        let graze = k as f32 / 20.0;
        for nibble in [-1.0, 0.0, 1.0] {
            out.push((
                format!("broute {graze} ({nibble})"),
                PoseSignals {
                    graze,
                    nibble,
                    ..PoseSignals::default()
                },
            ));
        }
        out.push((
            format!("broute {graze}, en marchant"),
            PoseSignals {
                graze,
                walk: 0.4,
                gait_phase: graze * 9.0,
                fleece: 0.08,
                fleece_tip: -0.05,
                ..PoseSignals::default()
            },
        ));
    }
    for k in 0..=20 {
        let sit = k as f32 / 20.0;
        out.push((
            format!("se couche {sit}"),
            PoseSignals {
                sit,
                ..PoseSignals::default()
            },
        ));
    }
    let asleep = PoseSignals {
        sit: 1.0,
        sleep: 1.0,
        blink: 1.0,
        ..PoseSignals::default()
    };
    for k in 0..=20 {
        let low = k as f32 / 20.0;
        for (breath, twitch) in [(0.0, 0.0), (1.0, 0.0), (-1.0, 1.0)] {
            out.push((
                format!("tête au sol {low} ({breath}, {twitch})"),
                PoseSignals {
                    sleep_low: low,
                    breath,
                    twitch,
                    ..asleep
                },
            ));
        }
    }
    for gesture in Gesture::ALL {
        for k in 0..12 {
            let time = k as f32 * 0.2;
            for sit in [0.0, 1.0] {
                out.push((
                    format!("{gesture:?} {time} (couché {sit})"),
                    PoseSignals {
                        gesture: Some(gesture),
                        gesture_amount: 1.0,
                        gesture_time: time,
                        sit,
                        ..PoseSignals::default()
                    },
                ));
            }
        }
    }
    for (name, s) in [
        (
            "saut",
            PoseSignals {
                air: 1.0,
                rise: 0.9,
                lean: 0.3,
                squash: -0.07,
                ..PoseSignals::default()
            },
        ),
        (
            "retombe",
            PoseSignals {
                air: 1.0,
                rise: -0.9,
                lean: -0.2,
                ..PoseSignals::default()
            },
        ),
        (
            "saut raide",
            PoseSignals {
                air: 1.0,
                stiff: 1.0,
                clock: 0.3,
                ..PoseSignals::default()
            },
        ),
        (
            "saut tordu",
            PoseSignals {
                air: 1.0,
                twist: 1.0,
                clock: 0.14,
                ..PoseSignals::default()
            },
        ),
        (
            "atterrit",
            PoseSignals {
                squash: 0.3,
                ..PoseSignals::default()
            },
        ),
        (
            "tenu, balancé",
            PoseSignals {
                dangle: 1.0,
                swing: 0.75,
                ..PoseSignals::default()
            },
        ),
        (
            "tenu, balancé de l'autre côté",
            PoseSignals {
                dangle: 1.0,
                swing: -0.75,
                ..PoseSignals::default()
            },
        ),
        (
            "vent dans le dos",
            PoseSignals {
                fleece: -0.45,
                fleece_tip: -0.15,
                graze: 0.8,
                ..PoseSignals::default()
            },
        ),
        (
            "fonce",
            PoseSignals {
                walk: 1.0,
                run: 1.0,
                head_low: 1.0,
                lean: 0.08,
                fleece: 0.35,
                ..PoseSignals::default()
            },
        ),
    ] {
        out.push((name.to_owned(), s));
    }
    out
}

#[test]
fn the_merino_redon_never_goes_below_the_grass_whatever_the_pose() {
    let rig = merino();
    assert_eq!(rig.parent(Part::Bell), Some(Part::Neck));
    let paths = drawn(&rig, Part::Bell);
    let mut worst = (f64::NEG_INFINITY, String::new());
    for facing in [1.0, 0.3, -0.3, -1.0] {
        for (name, signals) in poses() {
            let posed = pose(&rig, &signals, facing, 0.0);
            let under = below(&rig, &paths, &posed);
            if under > worst.0 {
                worst = (under, format!("{name}, sens {facing}"));
            }
        }
    }
    assert!(worst.0 <= 0.0, "{} : {:.2} u sous le sol", worst.1, worst.0);
}

#[test]
fn the_merino_redon_hangs_upright_while_it_touches_nothing() {
    let rig = merino();
    for facing in [1.0, -1.0] {
        for (name, signals) in [
            ("debout", PoseSignals::default()),
            (
                "broute la tête à mi-hauteur",
                PoseSignals {
                    graze: 0.5,
                    ..PoseSignals::default()
                },
            ),
            (
                "aux aguets",
                PoseSignals {
                    ears_back: -1.0,
                    ..PoseSignals::default()
                },
            ),
            (
                "fonce tête basse",
                PoseSignals {
                    head_low: 1.0,
                    ..PoseSignals::default()
                },
            ),
            (
                "s'étire",
                PoseSignals {
                    gesture: Some(Gesture::Stretch),
                    gesture_amount: 1.0,
                    gesture_time: 0.7,
                    ..PoseSignals::default()
                },
            ),
            (
                "le flehmen",
                PoseSignals {
                    gesture: Some(Gesture::Flehmen),
                    gesture_amount: 1.0,
                    gesture_time: 2.0,
                    ..PoseSignals::default()
                },
            ),
            (
                "couché la tête haute",
                PoseSignals {
                    sit: 1.0,
                    ..PoseSignals::default()
                },
            ),
            (
                "élan vers l'avant",
                PoseSignals {
                    lean: 0.3,
                    ..PoseSignals::default()
                },
            ),
            (
                "tenu, balancé",
                PoseSignals {
                    dangle: 1.0,
                    swing: 0.6,
                    ..PoseSignals::default()
                },
            ),
            (
                "cabré",
                PoseSignals {
                    gesture: Some(Gesture::Rear),
                    gesture_amount: 1.0,
                    gesture_time: 0.6,
                    ..PoseSignals::default()
                },
            ),
        ] {
            let posed = pose(&rig, &signals, facing, 0.0);
            let (bell, neck) = tilts(&rig, &posed);
            assert!(
                bell.abs() < 1e-6,
                "{name}, sens {facing} : le redon penche de {bell:.3} rad (l'encolure de {neck:.3})"
            );
        }
        // À mi-hauteur, l'encolure penche bien, et l'anse la suit.
        let posed = pose(
            &rig,
            &PoseSignals {
                graze: 0.5,
                ..PoseSignals::default()
            },
            facing,
            0.0,
        );
        let (_, neck) = tilts(&rig, &posed);
        assert!(neck.abs() > 0.5, "{neck}");
        let pivot = rig.pivot(Part::Bell);
        let anse = absolute(&rig, &posed, Part::Bell) * pivot;
        let collar = absolute(&rig, &posed, Part::Neck) * pivot;
        assert!((anse - collar).hypot() < 1e-9, "{anse:?} {collar:?}");
    }
}

#[test]
fn at_rest_the_redon_stays_exactly_where_it_is_drawn() {
    let rig = merino();
    for facing in [1.0, -1.0] {
        for signals in [
            PoseSignals::default(),
            PoseSignals {
                breath: 0.8,
                blink: 0.5,
                ..PoseSignals::default()
            },
        ] {
            let posed = pose(&rig, &signals, facing, 0.0);
            let bell = absolute(&rig, &posed, Part::Bell);
            let neck = absolute(&rig, &posed, Part::Neck);
            let (a, b) = (bell.as_coeffs(), neck.as_coeffs());
            for (x, y) in a.iter().zip(&b) {
                assert!((x - y).abs() < 1e-9, "{a:?}\n{b:?}");
            }
        }
    }
}

#[test]
fn the_merino_redon_tips_back_onto_the_grass_without_leaving_its_strap() {
    let rig = merino();
    let paths = drawn(&rig, Part::Bell);
    let u = rig.bounds.height() / 100.0;
    let grazing = PoseSignals {
        graze: 1.0,
        ..PoseSignals::default()
    };
    let asleep = PoseSignals {
        sit: 1.0,
        sleep: 1.0,
        sleep_low: 1.0,
        ..PoseSignals::default()
    };
    for facing in [1.0, -1.0] {
        // En broutant à fond, il bascule vers l'arrière (son bas vers les
        // antérieurs) juste assez pour se poser sur l'herbe.
        let posed = pose(&rig, &grazing, facing, 0.0);
        let (bell, _) = tilts(&rig, &posed);
        assert!(bell > 0.5 && bell < 1.2, "broute, sens {facing} : {bell}");
        let under = below(&rig, &paths, &posed);
        assert!(under > -1.5 && under <= 0.0, "broute : {under:.2} u");
        // Endormi la tête au sol, la tête posée sur le collier (voir
        // `tete_au_sol.rs`) : presque couché sur le flanc, sur l'herbe, et
        // toujours pendu à sa courroie (l'anse ne quitte pas le collier).
        let posed = pose(&rig, &asleep, facing, 0.0);
        let (bell, _) = tilts(&rig, &posed);
        assert!(
            bell > 1.3 && bell <= std::f64::consts::FRAC_PI_2 + 1e-9,
            "tête au sol, sens {facing} : {bell}"
        );
        let under = below(&rig, &paths, &posed);
        assert!(under > -1.5 && under <= 0.0, "tête au sol : {under:.2} u");
        let pivot = rig.pivot(Part::Bell);
        let anse = absolute(&rig, &posed, Part::Bell) * pivot;
        let collar = absolute(&rig, &posed, Part::Neck) * pivot;
        assert!((anse - collar).hypot() < 1e-6 * u, "{anse:?} {collar:?}");
    }
}

/// Trois points du redon (le battant, le bord avant de la bouche, le haut
/// de l'anse) à l'écran.
fn marks(rig: &Rig, posed: &PoseTransforms) -> [Point; 3] {
    marks_in(rig, absolute(rig, posed, Part::Bell))
}

fn marks_in(rig: &Rig, at: Affine) -> [Point; 3] {
    let b = rig.part_bounds(Part::Bell);
    [
        at * Point::new(b.center().x, b.max_y()),
        at * Point::new(b.max_x(), b.max_y() - 0.2 * b.height()),
        at * Point::new(rig.pivot(Part::Bell).x, b.min_y()),
    ]
}

/// Les signaux d'une pose, à l'instant `t` (de 0 à 1) d'un geste lent, avec
/// un balancement de la toison.
type Sweep = fn(f32, f32) -> PoseSignals;

#[test]
fn the_merino_redon_never_jumps_from_hanging_to_tipped_to_lying() {
    // Le cou descend pas à pas (en broutant, endormi la tête au sol, en se
    // couchant), en balançant ou non : le redon suit sans saut, du même
    // ordre que le pas, d'un bout à l'autre.
    let rig = merino();
    let u = rig.bounds.height() / 100.0;
    let sweeps: [(&str, Sweep); 4] = [
        ("broute", |t, swing| PoseSignals {
            graze: t,
            fleece: swing,
            fleece_tip: -0.5 * swing,
            ..PoseSignals::default()
        }),
        ("tête au sol", |t, swing| PoseSignals {
            sit: 1.0,
            sleep: 1.0,
            sleep_low: t,
            fleece: swing,
            ..PoseSignals::default()
        }),
        ("se couche", |t, swing| PoseSignals {
            sit: t,
            fleece: swing,
            ..PoseSignals::default()
        }),
        ("broute au vent", |t, swing| PoseSignals {
            graze: 0.6 + 0.4 * t,
            fleece: -0.4 + swing,
            fleece_tip: 0.2,
            ..PoseSignals::default()
        }),
    ];
    let steps = 2000;
    for (name, signals) in sweeps {
        for swing in [-0.4, 0.0, 0.4] {
            for facing in [1.0, -1.0] {
                let mut last: Option<[Point; 3]> = None;
                for k in 0..=steps {
                    let t = k as f32 / steps as f32;
                    let posed = pose(&rig, &signals(t, swing), facing, 0.0);
                    let now = marks(&rig, &posed);
                    if let Some(last) = last {
                        let step = last
                            .iter()
                            .zip(&now)
                            .map(|(a, b)| (*a - *b).hypot())
                            .fold(0.0, f64::max);
                        assert!(
                            step < 0.25 * u,
                            "{name} ({swing}, sens {facing}) à {t} : {:.2} u d'un coup",
                            step / u
                        );
                    }
                    last = Some(now);
                }
            }
        }
    }
}

#[test]
fn the_merino_redon_stays_above_the_grass_and_never_jumps_in_a_long_life() {
    // Le vrai moteur : quatre floucats vifs, sept minutes, tous leurs
    // comportements (broutage, sommeil la tête au sol ou non, marche,
    // galop, arrêts, ébrouements, sauts, demi-tours, coups de vent) : le
    // redon est toujours au-dessus de l'herbe, et n'a jamais sauté d'un pas
    // du moteur au suivant.
    let path = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../pets/floucat/pet.toml");
    let def = Arc::new(load_pet(&path).unwrap());
    let art = PetArt::load(&def).unwrap();
    let rig = merino();
    let paths = drawn(&rig, Part::Bell);
    let stride = f64::from(art.stride) * rig.bounds.height() / f64::from(art.height);
    let u = rig.bounds.height() / 100.0;
    let world = World::new(vec![Screen {
        frame: Rect::new(0.0, 0.0, 1440.0, 900.0),
        usable: Rect::new(0.0, 25.0, 1440.0, 800.0),
        scale: 2.0,
    }]);
    let mut flock = Flock::new();
    flock.set_mood(Mood::Lively);
    for (k, x) in [300.0, 600.0, 900.0, 1200.0].into_iter().enumerate() {
        flock.spawn(Arc::clone(&def), 11 + k as u64, &world, Some(x));
    }
    let input = Input {
        local_hour: 15.0,
        ..Input::default()
    };
    // Un sur deux suit un script (pour être sûr de tout voir : ses
    // comportements, et combien de secondes), les autres vivent leur vie ;
    // ils échangent leurs rôles à chaque ligne.
    let script = [
        ("graze", 8.0),
        ("sleep", 16.0),
        ("walk", 4.0),
        ("run", 3.0),
        ("stand", 2.0),
        ("shake", 2.0),
        ("sit", 4.0),
        ("sleep", 16.0),
        ("graze", 8.0),
        ("trot", 3.0),
        ("stand", 1.0),
        ("pronk", 2.0),
        ("hop", 2.0),
        ("ruminate", 5.0),
        ("sleep", 16.0),
        ("stretch", 3.0),
        ("graze", 6.0),
        ("walk", 3.0),
    ];
    let starts: Vec<f32> = script
        .iter()
        .scan(0.0, |t, &(_, d)| {
            let start = *t;
            *t += d;
            Some(start)
        })
        .collect();
    let cycle: f32 = script.iter().map(|&(_, d)| d).sum();
    let mut line = usize::MAX;
    let mut last: Vec<Option<(f32, [Point; 3])>> = vec![None; 4];
    let mut prev: Vec<Option<PoseSignals>> = vec![None; 4];
    let (mut worst, mut fastest) = (f64::NEG_INFINITY, 0.0_f64);
    let (mut grazing, mut lying_low) = (0, 0);
    let seconds = 420.0;
    for step in 0..(seconds / TICK) as usize {
        let t = step as f32 * TICK;
        let round = (t / cycle) as usize;
        let now = starts
            .iter()
            .rposition(|&start| start <= t - round as f32 * cycle)
            .unwrap_or(0)
            + round * script.len();
        if now != line {
            line = now;
            let wanted = script[line % script.len()].0;
            let ids: Vec<u32> = flock.pets().iter().map(|p| p.id()).collect();
            for (n, id) in ids.into_iter().enumerate() {
                if (n + line) % 2 == 0
                    && let Some(pet) = flock.get_mut(id)
                {
                    pet.force(wanted);
                }
            }
        }
        if step.is_multiple_of((37.0 / TICK) as usize) && step > 0 {
            flock.blow(
                if (t as usize / 37).is_multiple_of(2) {
                    1.0
                } else {
                    -1.0
                },
                3.0,
            );
        }
        flock.advance(TICK, &world, &input);
        for (n, pet) in flock.pets().iter().enumerate() {
            let view = pet.view(1.0);
            let s = view.pose;
            let posed = pose(&rig, &s, view.facing, stride);
            // Pendant une culbute, il vole en tournant sur lui-même : le
            // sol n'est plus sous ses sabots. Ses tracés sont lus une image
            // sur quatre (trente par seconde) : c'est le plus long.
            if s.roll.abs() < 0.1 && step.is_multiple_of(4) {
                let under = below(&rig, &paths, &posed);
                worst = worst.max(under);
                assert!(
                    under <= 0.0,
                    "à {t:.2} s, {} : {under:.2} u sous le sol ({s:?})",
                    view.behavior
                );
            }
            if s.graze > 0.95 {
                grazing += 1;
            }
            if s.sleep_low > 0.95 {
                lying_low += 1;
            }
            // Ce que fait le redon autour de là où le collier porte son
            // anse, dans le repère du dessin : un demi-tour qui resserre le
            // dessin, une culbute qui le fait tourner tout entier n'y
            // comptent pas.
            let bell = posed.root.inverse() * absolute(&rig, &posed, Part::Bell);
            let neck = posed.root.inverse() * absolute(&rig, &posed, Part::Neck);
            let collar = neck * rig.pivot(Part::Bell);
            let now = marks_in(&rig, Affine::translate(-collar.to_vec2()) * bell);
            // Un geste qui en remplace un autre sans transition (il renifle,
            // puis s'ébroue) fait sauter la tête, et la toison avec elle :
            // c'est le geste, pas le redon.
            let same_gesture = prev[n].is_some_and(|p| p.gesture == s.gesture);
            if let Some((facing, before)) = last[n]
                && facing * view.facing > 0.0
                && same_gesture
            {
                let moved = before
                    .iter()
                    .zip(&now)
                    .map(|(a, b)| (*a - *b).hypot())
                    .fold(0.0, f64::max)
                    / u;
                fastest = fastest.max(moved);
            }
            last[n] = Some((view.facing, now));
            prev[n] = Some(s);
        }
    }
    println!("le plus bas : {worst:.2} u ; le plus vite : {fastest:.2} u par pas");
    assert!(grazing > 1000 && lying_low > 1000, "{grazing} {lying_low}");
    // Il ne saute jamais : quelques centièmes de la hauteur par pas du
    // moteur (1/120 s) au plus, quand l'encolure se relève vite de l'herbe
    // ou qu'il s'ébroue ; passer d'une branche à l'autre (pendu, basculé,
    // couché) d'un coup en ferait bien plus.
    assert!(fastest < 4.0, "{fastest:.2} u d'un coup");
}

/// Un petit mouton : une encolure qui porte la tête et, sous la gorge, une
/// cloche pendue à son collier.
const SMALL: &str = r##"<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 140 100">
    <g id="neck">
        <path d="M70 58L80 30L94 30L88 62Z" fill="#ddd"/>
        <path d="M86 44L94 42" stroke="#432" stroke-width="1.5" fill="none"/>
        <g id="bell"><path d="M88 43L96 43L100 66L84 66Z" fill="#a73"/>
            <circle cx="92" cy="67" r="1.5" fill="#eed"/>
            <circle id="pivot-bell" cx="92" cy="43" r="0.5"/></g>
        <circle id="pivot-neck" cx="76" cy="56" r="0.5"/>
        <g id="head"><ellipse cx="100" cy="30" rx="14" ry="8" fill="#888"/>
            <circle id="pivot-head" cx="88" cy="28" r="0.5"/>
            <circle id="pivot-muzzle" cx="113" cy="35" r="0.5"/></g>
    </g>
    <g id="body"><ellipse cx="50" cy="55" rx="30" ry="18" fill="#eee"/></g>
    <circle id="pivot-ground" cx="50" cy="96" r="0.5"/>
</svg>"##;

#[test]
fn a_bell_hangs_from_its_collar_upright_and_rests_on_the_grass() {
    let rig = Rig::parse(SMALL.as_bytes()).unwrap();
    assert!(rig.has_part(Part::Bell) && rig.follows_contract());
    let paths = drawn(&rig, Part::Bell);
    let pivot = Point::new(92.0, 43.0);
    let size = rig.part_bounds(Part::Bell).height() / (rig.bounds.height() / 100.0);
    let mut last: Option<[Point; 3]> = None;
    for k in 0..=400 {
        let graze = k as f32 / 400.0;
        let signals = PoseSignals {
            graze,
            ..PoseSignals::default()
        };
        let posed = pose(&rig, &signals, 1.0, 0.0);
        let bell = absolute(&rig, &posed, Part::Bell);
        let neck = absolute(&rig, &posed, Part::Neck);
        // L'anse suit le collier ; la cloche pend droite tant qu'elle ne
        // touche rien, puis bascule vers l'arrière, sur l'herbe.
        assert!((bell * pivot - neck * pivot).hypot() < 1e-9);
        let under = below(&rig, &paths, &posed);
        assert!(under <= 0.0, "broute {graze} : {under:.2} u sous le sol");
        let [a, b, ..] = bell.as_coeffs();
        let tilt = b.atan2(a);
        // Loin de l'herbe (de plus du tiers de sa hauteur), elle pend
        // droite.
        if under < -(0.3 * size + 1.0) {
            assert!(tilt.abs() < 1e-9, "broute {graze} : {tilt}");
        } else {
            assert!(tilt >= -1e-9, "broute {graze} : {tilt}");
        }
        let now = marks(&rig, &posed);
        if let Some(last) = last {
            for (p, q) in last.iter().zip(&now) {
                assert!((*p - *q).hypot() < 1.0, "broute {graze}");
            }
        }
        last = Some(now);
        if graze == 1.0 {
            // Broutant à fond, l'encolure penche, et la cloche, posée sur
            // l'herbe, bascule vers l'arrière.
            let [_, nb, ..] = neck.as_coeffs();
            assert!(nb > 0.5, "l'encolure penche");
            assert!(under > -1.0 && tilt > 0.3, "{under} {tilt}");
        }
    }
    // Au repos, rien ne bouge.
    let posed = pose(&rig, &PoseSignals::default(), 1.0, 0.0);
    let rest = absolute(&rig, &posed, Part::Bell);
    assert!((rest * Point::new(100.0, 66.0) - Point::new(100.0, 66.0)).hypot() < 1e-9);
}

#[test]
fn a_bell_drawn_in_the_head_or_the_body_follows_what_carries_it() {
    // Sans encolure, la cloche suit la partie qui la contient : la tête
    // (son anse tourne avec elle) ou le corps ; elle pend toujours droite.
    let bell = r##"<g id="bell"><path d="M88 43L96 43L99 58L85 58Z" fill="#a73"/>
        <circle id="pivot-bell" cx="92" cy="43" r="0.5"/></g>"##;
    for in_head in [true, false] {
        let svg = format!(
            r##"<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 140 100">
            <g id="body"><ellipse cx="50" cy="55" rx="30" ry="18" fill="#eee"/>{}</g>
            <g id="head"><ellipse cx="96" cy="34" rx="14" ry="10" fill="#888"/>{}
                <circle id="pivot-head" cx="84" cy="40" r="0.5"/></g>
            <circle id="pivot-ground" cx="50" cy="96" r="0.5"/></svg>"##,
            if in_head { "" } else { bell },
            if in_head { bell } else { "" },
        );
        let rig = Rig::parse(svg.as_bytes()).unwrap();
        let carrier = if in_head { Part::Head } else { Part::Body };
        assert_eq!(rig.parent(Part::Bell), Some(carrier));
        for signals in [
            PoseSignals {
                graze: 0.4,
                ..PoseSignals::default()
            },
            PoseSignals {
                look: CoreVec2::new(0.5, 0.8),
                sit: 0.5,
                ..PoseSignals::default()
            },
        ] {
            let posed = pose(&rig, &signals, -1.0, 0.0);
            let at = absolute(&rig, &posed, Part::Bell);
            let pivot = Point::new(92.0, 43.0);
            let carried = absolute(&rig, &posed, carrier) * pivot;
            assert!((at * pivot - carried).hypot() < 1e-9, "{in_head}");
            let [a, b, ..] = at.as_coeffs();
            assert!(
                (b.atan2(-a)).abs() < 1e-9,
                "{in_head} : {:?}",
                at.as_coeffs()
            );
        }
    }
}

/// De combien le battant a tourné dans la cloche (radians).
fn clapper(posed: &PoseTransforms) -> f64 {
    let [a, b, ..] = posed.part(Part::Clapper).as_coeffs();
    b.atan2(a)
}

#[test]
fn the_clapper_hangs_true_inside_the_bell_and_rests_against_its_wall() {
    // Le battant pend dans le redon : au repos, il est où il est dessiné ;
    // quand le redon penche ou se balance, il reste à la verticale (il
    // tourne à rebours dans la cloche), jusqu'à buter contre la paroi.
    let rig = merino();
    assert!(rig.has_part(Part::Clapper));
    assert_eq!(rig.parent(Part::Clapper), Some(Part::Bell));
    let rest = pose(&rig, &PoseSignals::default(), 1.0, 0.0);
    assert!(clapper(&rest).abs() < 1e-9);
    // Le redon se balance un peu (la toison traîne) : le battant reste
    // droit à l'écran.
    let swinging = pose(
        &rig,
        &PoseSignals {
            fleece: 0.2,
            ..PoseSignals::default()
        },
        1.0,
        0.0,
    );
    let (bell, _) = tilts(&rig, &swinging);
    assert!(bell.abs() > 0.05, "le redon ne se balance pas : {bell}");
    let inside = absolute(&rig, &swinging, Part::Clapper);
    assert!(
        tilt(inside).abs() < 1e-6,
        "le battant penche de {}",
        tilt(inside)
    );
    assert!(clapper(&swinging).abs() > 0.05);
    // Il suit avec un temps de retard : le bout des mèches en dit autant.
    let lagging = pose(
        &rig,
        &PoseSignals {
            fleece_tip: 0.1,
            ..PoseSignals::default()
        },
        1.0,
        0.0,
    );
    assert!(clapper(&lagging).abs() > 0.02);
    // Couché sur l'herbe (il broute, il dort la tête au sol), le redon est
    // penché de bien plus que le battant ne peut s'écarter : il bute.
    for signals in [
        PoseSignals {
            graze: 1.0,
            ..PoseSignals::default()
        },
        PoseSignals {
            sit: 1.0,
            sleep: 1.0,
            sleep_low: 1.0,
            ..PoseSignals::default()
        },
    ] {
        let lying = pose(&rig, &signals, 1.0, 0.0);
        assert!(
            (clapper(&lying).abs() - 0.33).abs() < 1e-9,
            "{}",
            clapper(&lying)
        );
    }
    // Il ne sort jamais de la cloche : son bout reste entre ses bords.
    let mouth = rig.own_bounds(Part::Bell).expect("le redon");
    let tip = rig.own_bounds(Part::Clapper).expect("le battant");
    let pivot = rig.pivot(Part::Clapper);
    let reach = (tip.max_y() - pivot.y) * 0.33_f64.sin() + 0.5 * tip.width();
    assert!(
        reach < 0.5 * mouth.width(),
        "{reach} pour {}",
        mouth.width()
    );
}

#[test]
fn the_bell_lies_behind_the_foreleg_we_see_when_the_merino_grazes() {
    // L'encolure et sa cloche sont au milieu du corps : baissées pour
    // brouter, elles passent derrière les pattes qu'on voit (dessinées
    // après elles) et devant celles de l'autre côté.
    let rig = merino();
    let begins = |part: Part| {
        rig.items
            .iter()
            .position(|item| matches!(item, Item::BeginPart(p) if *p == part))
            .expect("la partie")
    };
    let (neck, bell) = (begins(Part::Neck), begins(Part::Bell));
    assert!(neck < bell);
    for near in [Part::LegFrontNear, Part::LegBackNear] {
        assert!(bell < begins(near), "{near:?}");
    }
    for far in [Part::LegFrontFar, Part::LegBackFar] {
        assert!(begins(far) < neck, "{far:?}");
    }
    // Et la cloche, couchée sur l'herbe, arrive bien jusqu'à l'antérieur :
    // c'est lui qui la cache.
    let grazing = pose(
        &rig,
        &PoseSignals {
            graze: 1.0,
            ..PoseSignals::default()
        },
        1.0,
        0.0,
    );
    let bell_at = absolute(&rig, &grazing, Part::Bell);
    let hoof = rig.own_bounds(Part::LegFrontNearLower).expect("le canon");
    let leg_at = absolute(&rig, &grazing, Part::LegFrontNearLower);
    let leg_front = (leg_at * Point::new(hoof.max_x(), hoof.max_y())).x;
    let bell_back = drawn(&rig, Part::Bell)
        .iter()
        .map(|path| (bell_at * path.clone()).bounding_box().min_x())
        .fold(f64::INFINITY, f64::min);
    assert!(bell_back < leg_front, "{bell_back} et {leg_front}");
}
