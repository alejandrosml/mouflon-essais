//! Les articulations : l'encolure qui porte la tête, les pattes en deux
//! segments (le carpe, le jarret), la lèvre, les mèches de la toison. Sur un
//! petit mouton d'essai rangé comme le dit le contrat, sur le même rangé
//! autrement (l'encolure sous le corps, la tête à part), et sur un squelette.

use std::f64::consts::TAU;
use std::sync::Arc;

use mouflon_core::{Gesture, PngData, PoseSignals, parse_skeleton};
use mouflon_render::animate::{PoseTransforms, pose};
use mouflon_render::kurbo::{Affine, Point};
use mouflon_render::rig::{Part, Rig};
use mouflon_render::skeleton::SkeletonArt;

/// Un mouton d'essai, de 100 de haut, sol à 96 : chaque patte en deux
/// segments, l'encolure qui porte la tête, une lèvre, une mèche.
const NESTED: &str = r##"<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 120 100">
  <g id="leg-back-near"><g id="leg-back-near-lower"><rect x="26" y="76" width="8" height="20" fill="#333"/>
    <circle id="pivot-leg-back-near-lower" cx="30" cy="80" r="0.5"/></g>
    <rect x="26" y="60" width="8" height="22" fill="#444"/><circle id="pivot-leg-back-near" cx="30" cy="64" r="0.5"/></g>
  <g id="leg-front-near"><g id="leg-front-near-lower"><rect x="70" y="76" width="8" height="20" fill="#333"/>
    <circle id="pivot-leg-front-near-lower" cx="74" cy="80" r="0.5"/></g>
    <rect x="70" y="60" width="8" height="22" fill="#444"/><circle id="pivot-leg-front-near" cx="74" cy="64" r="0.5"/></g>
  <g id="body"><ellipse cx="50" cy="55" rx="35" ry="22" fill="#eee"/>
    <g id="sway-1"><ellipse cx="50" cy="79" rx="4" ry="5" fill="#ddd"/><circle id="pivot-sway-1" cx="50" cy="75" r="0.5"/></g>
    <circle id="pivot-body" cx="50" cy="77" r="0.5"/></g>
  <g id="neck"><rect x="70" y="38" width="16" height="22" fill="#ccc"/>
    <g id="head"><ellipse cx="92" cy="38" rx="13" ry="9" fill="#888"/>
      <g id="lip"><rect x="98" y="40" width="6" height="4" fill="#777"/><circle id="pivot-lip" cx="98" cy="42" r="0.5"/></g>
      <circle id="pivot-head" cx="82" cy="33" r="0.5"/><circle id="pivot-muzzle" cx="103" cy="45" r="0.5"/></g>
    <circle id="pivot-neck" cx="74" cy="55" r="0.5"/></g>
  <circle id="pivot-ground" cx="50" cy="96" r="1"/>
</svg>"##;

/// Le même, l'encolure dessinée sous le corps et la tête à part, par-dessus.
fn apart() -> String {
    let neck_start = NESTED.find("  <g id=\"neck\">").unwrap();
    let neck_end = NESTED.find("  <circle id=\"pivot-ground\"").unwrap();
    let head_start = NESTED.find("    <g id=\"head\">").unwrap();
    let head_end = NESTED.find("    <circle id=\"pivot-neck\"").unwrap();
    let head = &NESTED[head_start..head_end];
    let neck = NESTED[neck_start..neck_end].replace(head, "");
    let body_start = NESTED.find("  <g id=\"body\">").unwrap();
    let without = format!("{}{}", &NESTED[..neck_start], &NESTED[neck_end..]);
    let before_body = &without[..body_start];
    let rest = &without[body_start..];
    let ground = rest.find("  <circle id=\"pivot-ground\"").unwrap();
    format!(
        "{before_body}{neck}{}{head}{}",
        &rest[..ground],
        &rest[ground..]
    )
}

fn rig() -> Rig {
    Rig::parse(NESTED.as_bytes()).unwrap()
}

/// Une partie dans le dessin, avec tout ce qui la porte dans le document.
fn absolute(rig: &Rig, p: &PoseTransforms, part: Part) -> Affine {
    let mut t = p.part(part);
    let mut up = rig.parent(part);
    while let Some(parent) = up {
        t = p.part(parent) * t;
        up = rig.parent(parent);
    }
    p.root * t
}

/// L'angle d'une transformation (radians, sens des aiguilles d'une montre).
fn angle(t: Affine) -> f64 {
    let [a, b, ..] = t.as_coeffs();
    b.atan2(a)
}

/// De combien une transformation penche la verticale (radians ; positif : le
/// bas part vers l'arrière, les x négatifs). Une rotation ou un cisaillement.
fn lean(t: Affine) -> f64 {
    let [_, _, c, d, ..] = t.as_coeffs();
    (-c).atan2(d)
}

fn signals(f: impl FnOnce(&mut PoseSignals)) -> PoseSignals {
    let mut s = PoseSignals::default();
    f(&mut s);
    s
}

const U: f64 = 1.0; // un centième de la hauteur du mouton d'essai (100)
const GROUND: f64 = 96.0;

#[test]
fn a_grazing_sheep_puts_its_muzzle_on_the_ground_in_front_of_its_forefeet() {
    let rig = rig();
    let muzzle = rig.muzzle.unwrap();
    let rest = pose(&rig, &PoseSignals::default(), 1.0, 0.0);
    let grazing = pose(&rig, &signals(|s| s.graze = 1.0), 1.0, 0.0);
    let m = absolute(&rig, &grazing, Part::Head) * muzzle;
    assert!((m.y - GROUND).abs() < 1.5 * U, "museau en {m:?}");
    assert!(m.x > 78.0, "devant les sabots des antérieurs : {m:?}");
    // C'est surtout l'encolure qui descend : la nuque descend d'au moins la
    // moitié de ce que descend le museau.
    let poll = rig.pivot(Part::Head);
    let poll_drop = (absolute(&rig, &grazing, Part::Head) * poll).y
        - (absolute(&rig, &rest, Part::Head) * poll).y;
    let muzzle_drop = m.y - (absolute(&rig, &rest, Part::Head) * muzzle).y;
    assert!(poll_drop > 0.5 * muzzle_drop, "{poll_drop} {muzzle_drop}");
    // La tête ne s'étire jamais : elle tourne et se déplace d'une pièce.
    let [a, b, c, d, ..] = absolute(&rig, &grazing, Part::Head).as_coeffs();
    assert!((a * d - b * c - 1.0).abs() < 1e-9);
}

#[test]
fn an_alert_sheep_raises_its_neck_and_a_charging_one_lowers_it() {
    let rig = rig();
    let poll = |s: &PoseSignals| {
        let p = pose(&rig, s, 1.0, 0.0);
        (absolute(&rig, &p, Part::Neck) * rig.pivot(Part::Head)).y
    };
    let rest = poll(&PoseSignals::default());
    assert!(poll(&signals(|s| s.ears_back = -1.0)) < rest - 1.0 * U);
    assert!(poll(&signals(|s| s.head_low = 1.0)) > rest + 1.0 * U);
}

#[test]
fn a_sleeping_sheep_stretches_its_neck_and_lays_its_chin_down() {
    let rig = rig();
    let p = pose(
        &rig,
        &signals(|s| {
            s.sit = 1.0;
            s.sleep = 1.0;
            s.sleep_low = 1.0;
        }),
        1.0,
        0.0,
    );
    let head = rig.part_bounds(Part::Head);
    let t = absolute(&rig, &p, Part::Head);
    let lowest = [
        Point::new(head.min_x(), head.max_y()),
        Point::new(head.max_x(), head.max_y()),
        Point::new(head.center().x, head.max_y()),
    ]
    .map(|c| (t * c).y)
    .into_iter()
    .fold(f64::NEG_INFINITY, f64::max);
    assert!(
        lowest > GROUND - 2.5 * U && lowest < GROUND + 0.5,
        "{lowest}"
    );
}

/// De combien le bas d'une patte plie par rapport au haut (radians).
fn bend(p: &PoseTransforms, lower: Part) -> f64 {
    angle(p.part(lower))
}

#[test]
fn the_stance_leg_stays_straight_and_the_carpus_and_hock_bend_in_flight() {
    let rig = rig();
    let (mut stance, mut front_flight, mut hind_flight) = (0.0f64, 0.0f64, 0.0f64);
    for i in 0..48 {
        let s = signals(|s| {
            s.walk = 1.0;
            s.gait_phase = (TAU * f64::from(i) / 48.0) as f32;
        });
        let p = pose(&rig, &s, 1.0, 60.0);
        for (leg, lower, front) in [
            (Part::LegFrontNear, Part::LegFrontNearLower, true),
            (Part::LegBackNear, Part::LegBackNearLower, false),
        ] {
            let b = rig.part_bounds(lower);
            let hoof = absolute(&rig, &p, lower) * Point::new(b.center().x, b.max_y());
            let k = bend(&p, lower);
            if hoof.y > GROUND - 0.05 {
                stance = stance.max(k.abs());
            } else if front {
                front_flight = front_flight.max(k);
            } else {
                hind_flight = hind_flight.min(k);
            }
            let _ = leg;
        }
    }
    assert!(stance < 0.12, "patte d'appui pliée de {stance}");
    // Le canon d'un antérieur part en arrière (sens des aiguilles d'une
    // montre), celui d'un postérieur vers l'avant.
    assert!(front_flight > 0.5, "carpe : {front_flight}");
    assert!(hind_flight < -0.5, "jarret : {hind_flight}");
}

#[test]
fn a_lying_sheep_folds_its_forelegs_under_its_chest_and_its_hind_legs_along_its_belly() {
    let rig = rig();
    let p = pose(&rig, &signals(|s| s.sit = 1.0), 1.0, 0.0);
    for (lower, front) in [
        (Part::LegFrontNearLower, true),
        (Part::LegBackNearLower, false),
    ] {
        let b = rig.part_bounds(lower);
        let t = absolute(&rig, &p, lower);
        let knee = t * rig.pivot(lower);
        let hoof = t * Point::new(b.center().x, b.max_y());
        // Le canon couché, à plat : vers l'arrière devant, vers l'avant derrière.
        if front {
            assert!(hoof.x < knee.x - 10.0, "{knee:?} {hoof:?}");
        } else {
            assert!(hoof.x > knee.x + 10.0, "{knee:?} {hoof:?}");
        }
        assert!((hoof.y - knee.y).abs() < 4.0, "{knee:?} {hoof:?}");
        // Au sol, pas dessous.
        assert!(knee.y < GROUND + 0.5 && knee.y > GROUND - 8.0, "{knee:?}");
    }
    // Il se couche l'avant d'abord : à mi-chemin, l'antérieur est déjà plié,
    // le postérieur pas encore.
    let half = pose(&rig, &signals(|s| s.sit = 0.35), 1.0, 0.0);
    assert!(bend(&half, Part::LegFrontNearLower).abs() > 1.0);
    assert!(bend(&half, Part::LegBackNearLower).abs() < 0.4);
}

#[test]
fn a_jump_tucks_the_legs_and_a_landing_bends_the_knees() {
    let rig = rig();
    let up = pose(
        &rig,
        &signals(|s| {
            s.air = 1.0;
            s.rise = 1.0;
        }),
        1.0,
        0.0,
    );
    assert!(bend(&up, Part::LegFrontNearLower) > 1.0);
    assert!(bend(&up, Part::LegBackNearLower) < -0.8);
    // L'atterrissage : le corps descend, les genoux plient, les sabots
    // restent au sol.
    let landing = pose(&rig, &signals(|s| s.squash = 0.16), 1.0, 0.0);
    assert!(bend(&landing, Part::LegFrontNearLower).abs() > 0.2);
    let b = rig.part_bounds(Part::LegFrontNearLower);
    let hoof =
        absolute(&rig, &landing, Part::LegFrontNearLower) * Point::new(b.center().x, b.max_y());
    assert!((hoof.y - GROUND).abs() < 0.6, "{hoof:?}");
}

#[test]
fn gestures_bend_the_right_joints() {
    let rig = rig();
    let at = |g: Gesture, t: f32| {
        pose(
            &rig,
            &signals(|s| {
                s.gesture = Some(g);
                s.gesture_amount = 1.0;
                s.gesture_time = t;
            }),
            1.0,
            0.0,
        )
    };
    // Gratter le sol, taper du pied : le carpe plie, la patte levée (le
    // sabot racle d'abord vers l'arrière, puis se lève).
    assert!(bend(&at(Gesture::Paw, 0.26), Part::LegFrontNearLower) > 0.5);
    assert!(bend(&at(Gesture::Stamp, 0.25), Part::LegFrontNearLower) > 0.3);
    // Se cabrer : les antérieurs repliés.
    assert!(bend(&at(Gesture::Rear, 0.6), Part::LegFrontNearLower) > 1.0);
    // Se gratter d'un postérieur : le jarret plie.
    assert!(bend(&at(Gesture::Scratch, 0.8), Part::LegBackNearLower) < -0.5);
    // S'étirer : la patte arrière tendue loin derrière, le jarret ouvert.
    assert!(bend(&at(Gesture::Stretch, 1.65), Part::LegBackNearLower).abs() < 0.25);
}

#[test]
fn the_flehmen_curls_the_upper_lip_and_raises_the_head() {
    let rig = rig();
    let rest = pose(&rig, &PoseSignals::default(), 1.0, 0.0);
    let flehmen = pose(
        &rig,
        &signals(|s| {
            s.gesture = Some(Gesture::Flehmen);
            s.gesture_amount = 1.0;
            s.gesture_time = 3.0;
        }),
        1.0,
        0.0,
    );
    // La lèvre se retrousse : elle tourne vers le haut autour de son arrière.
    assert!(
        angle(flehmen.part(Part::Lip)) < -0.6,
        "{}",
        angle(flehmen.part(Part::Lip))
    );
    assert!(angle(rest.part(Part::Lip)).abs() < 1e-9);
    // Le nez se relève, l'encolure tendue vers le haut.
    let muzzle = rig.muzzle.unwrap();
    let up = (absolute(&rig, &flehmen, Part::Head) * muzzle).y;
    let before = (absolute(&rig, &rest, Part::Head) * muzzle).y;
    assert!(up < before - 4.0, "{before} {up}");
    assert!(angle(absolute(&rig, &flehmen, Part::Head)) < -0.4);
}

#[test]
fn the_fleece_trails_when_running_and_swings_when_shaking() {
    let rig = rig();
    let sway = |s: &PoseSignals| lean(pose(&rig, s, 1.0, 0.0).part(Part::Sway1));
    assert!(sway(&PoseSignals::default()).abs() < 1e-9);
    // Les mèches traînent : positif, leur bout part vers l'arrière.
    let trailing = signals(|s| s.fleece = 0.2);
    assert!(sway(&trailing) > 0.15);
    // Elles penchent sans quitter leur attache : toute la ligne horizontale
    // du pivot reste en place (un feston attaché sur toute sa largeur ne
    // s'ouvre pas), seul le bas part.
    let t = pose(&rig, &trailing, 1.0, 0.0).part(Part::Sway1);
    for x in [46.0, 50.0, 54.0] {
        let p = Point::new(x, 75.0);
        assert!((t * p - p).hypot() < 1e-9, "{x}");
    }
    let bottom = t * Point::new(50.0, 84.0);
    assert!(
        bottom.x < 50.0 - 1.0 && (bottom.y - 84.0).abs() < 1e-9,
        "{bottom:?}"
    );
    // Elles suivent la secousse, d'un côté puis de l'autre.
    let shaking: Vec<f64> = (0..24)
        .map(|i| {
            sway(&signals(|s| {
                s.gesture = Some(Gesture::Shake);
                s.gesture_amount = 1.0;
                s.gesture_time = 0.3 + i as f32 * 0.01;
            }))
        })
        .collect();
    let (lo, hi) = shaking
        .iter()
        .fold((f64::MAX, f64::MIN), |(lo, hi), &v| (lo.min(v), hi.max(v)));
    assert!(lo < -0.2 && hi > 0.2, "{lo} {hi}");
}

#[test]
fn a_head_drawn_apart_from_its_neck_still_follows_it() {
    let nested = rig();
    let apart = Rig::parse(apart().as_bytes()).unwrap();
    assert!(nested.follows_contract());
    assert!(!apart.follows_contract());
    assert_eq!(apart.parent(Part::Head), None);
    for s in [
        PoseSignals::default(),
        signals(|s| s.graze = 1.0),
        signals(|s| {
            s.walk = 1.0;
            s.gait_phase = 1.3;
            s.ears_back = -1.0;
        }),
    ] {
        let (a, b) = (pose(&nested, &s, 1.0, 0.0), pose(&apart, &s, 1.0, 0.0));
        for part in [Part::Neck, Part::Head, Part::Lip] {
            let (ta, tb) = (absolute(&nested, &a, part), absolute(&apart, &b, part));
            let p = Point::new(90.0, 40.0);
            assert!((ta * p - tb * p).hypot() < 1e-6, "{part:?}");
        }
    }
}

/// Un PNG plein de `color` sur `rect`, dans une toile de 400 × 300.
fn png(rect: (u32, u32, u32, u32), color: [u8; 4]) -> PngData {
    let (cw, ch) = (400, 300);
    let mut rgba = vec![0u8; (cw * ch * 4) as usize];
    let (x, y, w, h) = rect;
    for yy in y..y + h {
        for xx in x..x + w {
            let i = ((yy * cw + xx) * 4) as usize;
            rgba[i..i + 4].copy_from_slice(&color);
        }
    }
    let mut out = Vec::new();
    {
        let mut encoder = png::Encoder::new(&mut out, cw, ch);
        encoder.set_color(png::ColorType::Rgba);
        encoder.set_depth(png::BitDepth::Eight);
        let mut writer = encoder.write_header().unwrap();
        writer.write_image_data(&rgba).unwrap();
    }
    PngData::from(out)
}

const SKELETON: &str = r#"
    size = [400, 300]
    ground = [200, 280]
    muzzle = [330, 175]
    [[bone]]
    name = "racine"
    at = [200, 280]
    [[bone]]
    name = "corps"
    parent = "racine"
    at = [200, 180]
    role = "body"
    [[bone]]
    name = "patte"
    parent = "corps"
    at = [260, 210]
    role = "leg-front-near"
    [[bone]]
    name = "patte-bas"
    parent = "patte"
    at = [260, 245]
    role = "leg-front-near-lower"
    [[bone]]
    name = "meche"
    parent = "corps"
    at = [200, 205]
    role = "sway"
    [[bone]]
    name = "encolure"
    parent = "corps"
    at = [250, 175]
    role = "neck"
    [[bone]]
    name = "tete"
    parent = "encolure"
    at = [280, 140]
    role = "head"
    [[bone]]
    name = "levre"
    parent = "tete"
    at = [320, 162]
    role = "lip"
    [[piece]]
    bone = "patte-bas"
    image = "bas.png"
    [[piece]]
    bone = "patte"
    image = "haut.png"
    [[piece]]
    bone = "encolure"
    image = "encolure.png"
    [[piece]]
    bone = "corps"
    image = "corps.png"
    [[piece]]
    bone = "meche"
    image = "meche.png"
    [[piece]]
    bone = "tete"
    image = "tete.png"
    [[piece]]
    bone = "levre"
    image = "levre.png"
"#;

fn skeleton() -> SkeletonArt {
    let skeleton = parse_skeleton(SKELETON, |name| {
        Ok(match name {
            "corps.png" => png((120, 140, 160, 70), [240, 240, 240, 255]),
            "encolure.png" => png((240, 130, 40, 60), [230, 230, 230, 255]),
            "tete.png" => png((270, 120, 60, 50), [90, 80, 80, 255]),
            "levre.png" => png((318, 158, 14, 8), [80, 70, 70, 255]),
            "meche.png" => png((192, 205, 16, 14), [220, 220, 220, 255]),
            "haut.png" => png((255, 200, 10, 50), [60, 50, 50, 255]),
            _ => png((255, 240, 10, 40), [50, 40, 40, 255]),
        })
    })
    .unwrap();
    SkeletonArt::load(Arc::new(skeleton), 60.0).unwrap()
}

#[test]
fn a_skeleton_grazes_with_its_neck_and_folds_its_legs_when_lying() {
    let art = skeleton();
    // Au repos, rien ne bouge.
    let rest = art.pose(&PoseSignals::default(), 1.0, 0.0);
    for bone in &rest.bones {
        let p = Point::new(250.0, 200.0);
        assert!((*bone * p - p).hypot() < 1e-6);
    }
    // Il broute : le museau au sol, devant le sabot.
    let grazing = art.pose(&signals(|s| s.graze = 1.0), 1.0, 0.0);
    let m = grazing.bones[6] * Point::new(330.0, 175.0);
    assert!((m.y - 280.0).abs() < 4.0, "{m:?}");
    assert!(m.x > 265.0, "{m:?}");
    // Couché : le canon replié vers l'arrière, à plat.
    let lying = art.pose(&signals(|s| s.sit = 1.0), 1.0, 0.0);
    let knee = lying.bones[3] * Point::new(260.0, 245.0);
    let hoof = lying.bones[3] * Point::new(260.0, 280.0);
    assert!(
        hoof.x < knee.x - 20.0 && (hoof.y - knee.y).abs() < 10.0,
        "{knee:?} {hoof:?}"
    );
    // Le flehmen retrousse la lèvre ; la toison traîne.
    let flehmen = art.pose(
        &signals(|s| {
            s.gesture = Some(Gesture::Flehmen);
            s.gesture_amount = 1.0;
            s.gesture_time = 3.0;
        }),
        1.0,
        0.0,
    );
    let lip = flehmen.bones[6].inverse() * flehmen.bones[7];
    assert!(angle(lip) < -0.6, "{}", angle(lip));
    let trailing = art.pose(&signals(|s| s.fleece = 0.2), 1.0, 0.0);
    assert!(lean(trailing.bones[1].inverse() * trailing.bones[4]) > 0.15);
}

/// Le Mannequin, l'ancien animal intégré, gardé dans les données de test
/// (`donnees/mannequin-articule.svg`) : à sa taille, à son pas et à ses
/// vitesses d'alors.
fn mannequin() -> Arc<mouflon_core::PetDef> {
    const TOML: &str = r#"
        [pet]
        id = "mannequin"
        name = "Mannequin"
        [appearance]
        rig = "mannequin-articule.svg"
        height = 54.0
        [physics]
        width = 64.0
        stride = 29.0
        [reflexes]
        spawn = "fall"
        fall = "fall"
        land = "stand"
        [[behavior]]
        name = "stand"
        motion = "stand"
        duration = [1.5, 4.0]
        next = "stand"
        [[behavior]]
        name = "walk"
        motion = "walk"
        speed = 46.0
        duration = [3.0, 9.0]
        next = "stand"
        [[behavior]]
        name = "trot"
        motion = "walk"
        speed = 80.0
        duration = [2.0, 5.0]
        next = "stand"
        [[behavior]]
        name = "run"
        motion = "run"
        speed = 150.0
        duration = [1.2, 2.8]
        next = "stand"
        [[behavior]]
        name = "fall"
        motion = "fall"
        duration = "forever"
    "#;
    let dir = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/donnees");
    Arc::new(
        mouflon_core::parse_pet_toml(TOML, |name: &str| {
            std::fs::read(dir.join(name))
                .map_err(|e| mouflon_core::LoadError::Format(format!("{name} : {e}")))
        })
        .expect("le Mannequin"),
    )
}

/// Au pas, au trot et au galop, les sabots posés ne glissent pas : la patte
/// en deux segments pose son sabot là où il doit être, et le genou plie de
/// lui-même. On suit le vrai sabot, au bout du bas de la patte, sur le
/// Mannequin des données de test (les races du dépôt : `glissement.rs`).
#[test]
fn a_two_segment_leg_keeps_its_hoof_planted_at_every_gait() {
    use mouflon_core::{Input, Pet, Rect, Screen, TICK, World};
    use mouflon_render::PetArt;
    use mouflon_render::kurbo::Vec2 as KVec2;

    let def = mannequin();
    let art = PetArt::load(&def).expect("dessin du Mannequin");
    let mouflon_render::Look::Rig(rig) = &art.look else {
        panic!("le Mannequin est un dessin vectoriel")
    };
    assert!(rig.has_part(Part::LegFrontNearLower) && rig.has_part(Part::LegBackNearLower));
    let world = World::new(vec![Screen {
        frame: Rect::new(0.0, 0.0, 4000.0, 600.0),
        usable: Rect::new(0.0, 0.0, 4000.0, 560.0),
        scale: 2.0,
    }]);
    let k = f64::from(art.height) / rig.bounds.height();
    let stride = f64::from(art.stride) / k;
    for gait in ["walk", "trot", "run"] {
        let mut pet = Pet::spawn(0, Arc::clone(&def), 3, &world, Some(800.0));
        let input = Input::default();
        for _ in 0..(2.0 / TICK) as usize {
            pet.step(TICK, &world, &input);
        }
        assert!(pet.force(gait));
        for _ in 0..(1.0 / TICK) as usize {
            pet.step(TICK, &world, &input);
        }
        for (leg, lower) in [
            (Part::LegFrontNear, Part::LegFrontNearLower),
            (Part::LegBackNear, Part::LegBackNearLower),
        ] {
            let b = rig.part_bounds(lower);
            let hoof = Point::new(b.center().x, b.max_y());
            let mut previous: Option<Point> = None;
            let (mut planted, mut slide, mut speed) = (0usize, 0.0f64, 0.0f64);
            let mut probe = pet.clone();
            for _ in 0..(1.5 / TICK) as usize {
                probe.step(TICK, &world, &input);
                if probe.behavior_name() != gait {
                    probe.force(gait);
                }
                let view = probe.view(1.0);
                let p = pose(rig, &view.pose, view.facing, stride);
                let base = Affine::translate(KVec2::new(
                    f64::from(view.ground.x),
                    f64::from(view.ground.y),
                )) * Affine::scale(k)
                    * Affine::translate(-rig.ground.to_vec2())
                    * p.root;
                let foot = base * p.part(leg) * p.part(lower) * hoof;
                let ground = f64::from(view.ground.y);
                // Posé : au sol, à un vingtième de point près.
                if let Some(before) = previous
                    && foot.y > ground - 0.05
                    && before.y > ground - 0.05
                {
                    planted += 1;
                    slide += (foot.x - before.x).abs() / f64::from(TICK);
                    speed += f64::from(probe.velocity().x.abs());
                }
                previous = Some(foot);
            }
            assert!(
                planted > 30,
                "{gait} {leg:?} : sabot au sol pendant {planted} pas"
            );
            let (mean, speed) = (slide / planted as f64, speed / planted as f64);
            assert!(
                mean < 0.05 * speed,
                "{gait} {leg:?} : glisse à {mean:.1} pt/s pour {speed:.0} pt/s"
            );
        }
    }
}

#[test]
fn the_real_bleu_du_maine_grazes_with_its_muzzle_on_the_ground() {
    let path = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../pets/bleu-du-maine/bleu-du-maine.svg");
    let rig = Rig::parse(&std::fs::read(path).unwrap()).unwrap();
    let h = rig.bounds.height();
    let p = pose(&rig, &signals(|s| s.graze = 1.0), 1.0, 0.0);
    let m = absolute(&rig, &p, Part::Head) * rig.muzzle.unwrap();
    assert!((m.y - rig.ground.y).abs() < 0.02 * h, "{m:?}");
}
