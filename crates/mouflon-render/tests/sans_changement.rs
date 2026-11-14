//! La règle d'or des nouvelles articulations (encolure, bas des pattes,
//! lèvre, toison, arrière-train) : un dessin qui ne les a pas bouge
//! exactement comme avant, et se cuit dans les mêmes images.
//!
//! `donnees/mannequin-avant.svg` est le Mannequin tel qu'il était avant
//! d'être redécoupé ; `donnees/poses-avant.txt`, ce que le moteur faisait de
//! lui (et d'un squelette qui a tous les rôles d'alors) dans une soixantaine
//! de poses, avec l'empreinte de ses images cuites. Pour le récrire (seulement
//! si un changement voulu de ces dessins-là le justifie) :
//! `MOUFLON_DORE=ecrire cargo test -p mouflon-render --test sans_changement`.

use std::f32::consts::TAU;
use std::fmt::Write as _;
use std::path::Path;
use std::sync::Arc;

use mouflon_core::{Gesture, PngData, PoseSignals, Vec2, parse_skeleton};
use mouflon_render::animate::pose;
use mouflon_render::bake::BakedRig;
use mouflon_render::display::Pixels;
use mouflon_render::kurbo::Affine;
use mouflon_render::rig::{Item, Part, Rig};
use mouflon_render::skeleton::SkeletonArt;

/// Une soixantaine de poses : allures, poses, sauts, gestes, postures.
fn cases() -> Vec<(String, PoseSignals, f32)> {
    let base = PoseSignals {
        look: Vec2::new(0.35, 0.05),
        breath: 0.3,
        blink: 0.2,
        ..PoseSignals::default()
    };
    let mut out = vec![
        ("repos".to_owned(), PoseSignals::default(), 1.0),
        ("regard".to_owned(), base, 1.0),
        ("retourne".to_owned(), base, -1.0),
        ("demi-tour".to_owned(), base, 0.3),
    ];
    for i in 0..8 {
        let phase = TAU * i as f32 / 8.0;
        out.push((
            format!("pas-{i}"),
            PoseSignals {
                walk: 1.0,
                gait_phase: phase,
                lean: 0.03,
                ..base
            },
            1.0,
        ));
        out.push((
            format!("trot-{i}"),
            PoseSignals {
                walk: 1.0,
                trot: 1.0,
                stride_scale: 1.5,
                gait_phase: phase,
                ..base
            },
            1.0,
        ));
        out.push((
            format!("galop-{i}"),
            PoseSignals {
                walk: 1.0,
                run: 1.0,
                stride_scale: 2.0,
                gait_phase: phase,
                lean: 0.08,
                ..base
            },
            -1.0,
        ));
    }
    let more = [
        ("assis-mi", PoseSignals { sit: 0.5, ..base }),
        (
            "couche",
            PoseSignals {
                sit: 1.0,
                jaw: 0.2,
                ..base
            },
        ),
        (
            "dort",
            PoseSignals {
                sit: 1.0,
                sleep: 1.0,
                blink: 1.0,
                ..base
            },
        ),
        (
            "dort-tete-au-sol",
            PoseSignals {
                sit: 1.0,
                sleep: 1.0,
                sleep_low: 1.0,
                twitch: 1.0,
                ..base
            },
        ),
        (
            "broute",
            PoseSignals {
                graze: 1.0,
                nibble: 0.6,
                ..base
            },
        ),
        ("broute-mi", PoseSignals { graze: 0.5, ..base }),
        (
            "monte",
            PoseSignals {
                air: 1.0,
                rise: 0.9,
                squash: -0.07,
                ..base
            },
        ),
        (
            "descend",
            PoseSignals {
                air: 1.0,
                rise: -0.8,
                ..base
            },
        ),
        (
            "raide",
            PoseSignals {
                air: 1.0,
                stiff: 1.0,
                clock: 0.3,
                ..base
            },
        ),
        (
            "tordu",
            PoseSignals {
                air: 1.0,
                twist: 1.0,
                clock: 0.14,
                ..base
            },
        ),
        (
            "tenu",
            PoseSignals {
                dangle: 1.0,
                swing: 0.3,
                ..base
            },
        ),
        (
            "atterrit",
            PoseSignals {
                squash: 0.3,
                ear: 0.5,
                ..base
            },
        ),
        (
            "fonce",
            PoseSignals {
                head_low: 1.0,
                ears_back: 0.9,
                ..base
            },
        ),
        (
            "aux-aguets",
            PoseSignals {
                ears_back: -1.0,
                look: Vec2::new(0.8, -0.3),
                ear_split: 0.3,
                ..base
            },
        ),
        (
            "queue",
            PoseSignals {
                tail: 0.4,
                tail_mid: -0.2,
                tail_tip: 0.3,
                ears_droop: 1.0,
                ..base
            },
        ),
        ("culbute", PoseSignals { roll: 1.2, ..base }),
    ];
    out.extend(more.into_iter().map(|(n, s)| (n.to_owned(), s, 1.0)));
    // Les gestes d'avant les nouvelles articulations.
    for gesture in [
        Gesture::Shake,
        Gesture::Stretch,
        Gesture::Paw,
        Gesture::Stamp,
        Gesture::Sniff,
        Gesture::Daze,
        Gesture::Brace,
        Gesture::Yawn,
        Gesture::Bleat,
        Gesture::Scratch,
        Gesture::Rear,
        Gesture::Invite,
    ] {
        for time in [0.3_f32, 0.9, 1.7] {
            out.push((
                format!("{}-{time}", gesture.name()),
                PoseSignals {
                    gesture: Some(gesture),
                    gesture_amount: 1.0,
                    gesture_time: time,
                    jaw: 0.4,
                    ..base
                },
                1.0,
            ));
        }
    }
    out
}

fn line(out: &mut String, name: &str, what: &str, t: Affine) {
    let c = t.as_coeffs();
    writeln!(
        out,
        "{name} {what} {:.9} {:.9} {:.9} {:.9} {:.9} {:.9}",
        c[0], c[1], c[2], c[3], c[4], c[5]
    )
    .unwrap();
}

/// Ce que le moteur fait du Mannequin d'avant : chaque partie, dans chaque pose.
fn rig_poses(rig: &Rig, out: &mut String) {
    let stride = 29.0 / (54.0 / rig.bounds.height());
    for (name, signals, facing) in cases() {
        let p = pose(rig, &signals, facing, stride);
        line(out, &name, "racine", p.root);
        for part in [
            Part::Body,
            Part::Head,
            Part::EarNear,
            Part::EarFar,
            Part::Eyes,
            Part::Pupils,
            Part::Tail,
            Part::LegFrontNear,
            Part::LegFrontFar,
            Part::LegBackNear,
            Part::LegBackFar,
            Part::Jaw,
            Part::TailMid,
            Part::TailTip,
        ] {
            line(out, &name, part.name(), p.part(part));
        }
    }
}

/// L'empreinte (FNV-1a) des images cuites, dans l'ordre du dessin.
fn baked_prints(rig: &Rig, out: &mut String) {
    let baked = BakedRig::bake(rig, 54.0 / rig.bounds.height(), 0.6).expect("il se cuit");
    for (index, item) in rig.items.iter().enumerate() {
        if !matches!(item, Item::Fill { .. } | Item::Stroke { .. }) {
            continue;
        }
        let Some(piece) = baked.piece(index) else {
            continue;
        };
        let mut hash = 0xcbf2_9ce4_8422_2325_u64;
        let mut eat = |bytes: &[u8]| {
            for b in bytes {
                hash = (hash ^ u64::from(*b)).wrapping_mul(0x0100_0000_01b3);
            }
        };
        eat(&piece.bitmap.width.to_le_bytes());
        eat(&piece.bitmap.height.to_le_bytes());
        if let Pixels::Rgba(blob) = &piece.bitmap.pixels {
            eat(blob.data());
        }
        if let Some(m) = &piece.mirrored
            && let Pixels::Rgba(blob) = &m.pixels
        {
            eat(blob.data());
        }
        writeln!(out, "cuit {index} {hash:016x}").unwrap();
        line(out, "cuit", &index.to_string(), piece.placement);
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

/// Un squelette qui a tous les rôles d'avant les nouvelles articulations.
const SKELETON: &str = r#"
    size = [400, 300]
    ground = [200, 280]
    [[bone]]
    name = "racine"
    at = [200, 280]
    [[bone]]
    name = "corps"
    parent = "racine"
    at = [200, 180]
    role = "body"
    [[bone]]
    name = "queue"
    parent = "corps"
    at = [115, 160]
    role = "tail"
    [[bone]]
    name = "queue-2"
    parent = "queue"
    at = [108, 175]
    role = "tail"
    [[bone]]
    name = "queue-3"
    parent = "queue-2"
    at = [106, 190]
    role = "tail"
    [[bone]]
    name = "pab"
    parent = "corps"
    at = [150, 205]
    role = "leg-back-far"
    [[bone]]
    name = "pav"
    parent = "corps"
    at = [245, 205]
    role = "leg-front-far"
    [[bone]]
    name = "par"
    parent = "corps"
    at = [160, 210]
    role = "leg-back-near"
    [[bone]]
    name = "pap"
    parent = "corps"
    at = [260, 210]
    role = "leg-front-near"
    [[bone]]
    name = "tete"
    parent = "corps"
    at = [280, 150]
    role = "head"
    [[bone]]
    name = "oreille-1"
    parent = "tete"
    at = [285, 120]
    role = "ear"
    [[bone]]
    name = "oreille-2"
    parent = "tete"
    at = [290, 125]
    role = "ear"
    [[bone]]
    name = "oeil"
    parent = "tete"
    at = [300, 130]
    role = "eye"
    [[bone]]
    name = "pupille"
    parent = "oeil"
    at = [301, 131]
    role = "pupil"
    [[bone]]
    name = "machoire"
    parent = "tete"
    at = [300, 150]
    role = "jaw"
    [[piece]]
    bone = "pab"
    image = "pab.png"
    [[piece]]
    bone = "pav"
    image = "pav.png"
    [[piece]]
    bone = "queue"
    image = "queue.png"
    [[piece]]
    bone = "queue-2"
    image = "queue-2.png"
    [[piece]]
    bone = "queue-3"
    image = "queue-3.png"
    [[piece]]
    bone = "corps"
    image = "corps.png"
    [[piece]]
    bone = "par"
    image = "par.png"
    [[piece]]
    bone = "pap"
    image = "pap.png"
    [[piece]]
    bone = "oreille-1"
    image = "oreille.png"
    [[piece]]
    bone = "machoire"
    image = "machoire.png"
    [[piece]]
    bone = "tete"
    image = "tete.png"
    [[piece]]
    bone = "oreille-2"
    image = "oreille.png"
    [[piece]]
    bone = "oeil"
    image = "oeil.png"
    [[piece]]
    bone = "pupille"
    image = "pupille.png"
"#;

fn skeleton_poses(out: &mut String) {
    let skeleton = parse_skeleton(SKELETON, |name| {
        Ok(match name {
            "corps.png" => png((110, 140, 170, 72), [240, 240, 240, 255]),
            "tete.png" => png((268, 110, 52, 50), [90, 80, 80, 255]),
            "machoire.png" => png((295, 148, 24, 10), [80, 70, 70, 255]),
            "oreille.png" => png((270, 118, 18, 8), [90, 80, 80, 255]),
            "oeil.png" => png((296, 126, 9, 9), [250, 250, 250, 255]),
            "pupille.png" => png((298, 130, 6, 2), [0, 0, 0, 255]),
            "queue.png" => png((104, 155, 14, 18), [230, 230, 230, 255]),
            "queue-2.png" => png((103, 172, 9, 18), [230, 230, 230, 255]),
            "queue-3.png" => png((102, 188, 8, 14), [230, 230, 230, 255]),
            "pab.png" => png((145, 200, 10, 80), [50, 45, 45, 255]),
            "pav.png" => png((240, 200, 10, 80), [50, 45, 45, 255]),
            "par.png" => png((155, 205, 10, 75), [60, 50, 50, 255]),
            _ => png((255, 205, 10, 75), [60, 50, 50, 255]),
        })
    })
    .unwrap();
    let art = SkeletonArt::load(Arc::new(skeleton), 60.0).unwrap();
    for (name, signals, facing) in cases() {
        let p = art.pose(&signals, facing, 0.0);
        line(out, &name, "racine", p.root);
        for (i, bone) in p.bones.iter().enumerate() {
            line(out, &name, &format!("os-{i}"), *bone);
        }
    }
}

fn record() -> String {
    let dir = Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/donnees");
    let svg = std::fs::read(dir.join("mannequin-avant.svg")).unwrap();
    let rig = Rig::parse(&svg).unwrap();
    let mut out = String::new();
    rig_poses(&rig, &mut out);
    baked_prints(&rig, &mut out);
    skeleton_poses(&mut out);
    out
}

#[test]
fn a_drawing_without_the_new_joints_moves_and_bakes_exactly_as_before() {
    let path = Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/donnees/poses-avant.txt");
    let now = record();
    if std::env::var("MOUFLON_DORE").as_deref() == Ok("ecrire") {
        std::fs::write(&path, &now).unwrap();
        return;
    }
    let before = std::fs::read_to_string(&path).expect("poses-avant.txt");
    let (a, b): (Vec<&str>, Vec<&str>) = (before.lines().collect(), now.lines().collect());
    assert_eq!(a.len(), b.len(), "pas le même nombre de lignes");
    for (old, new) in a.iter().zip(&b) {
        let (wo, wn): (Vec<&str>, Vec<&str>) = (
            old.split_whitespace().collect(),
            new.split_whitespace().collect(),
        );
        assert_eq!(wo[..2], wn[..2], "{old}\n{new}");
        if wo[0] == "cuit" && wo.len() == 3 {
            assert_eq!(wo[2], wn[2], "image cuite changée : {old}");
            continue;
        }
        for (x, y) in wo[2..].iter().zip(&wn[2..]) {
            let (x, y): (f64, f64) = (x.parse().unwrap(), y.parse().unwrap());
            assert!((x - y).abs() < 1e-7, "{old}\n{new}");
        }
    }
}
