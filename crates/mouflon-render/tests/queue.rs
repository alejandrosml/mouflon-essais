//! Couché, le corps descend au ras du sol : une longue queue (Swaledale,
//! Southdown, Mérinos d'Arles, Thônes et Marthod, la Solognote peinte sur
//! son squelette) passait jusqu'à 17 u sous le sol. Elle se courbe
//! maintenant vers l'arrière et se pose sur l'herbe ; une queue qui ne
//! touche pas le sol bouge exactement comme avant.

use std::path::Path;
use std::sync::Arc;

use mouflon_core::{BoneRole, PoseSignals, parse_skeleton};
use mouflon_render::animate::{PoseTransforms, pose};
use mouflon_render::kurbo::{Affine, Point, Shape};
use mouflon_render::rig::{Item, Part, Rig};
use mouflon_render::skeleton::SkeletonArt;

const TAIL: [Part; 3] = [Part::Tail, Part::TailMid, Part::TailTip];

fn long_tails() -> [(&'static str, Rig); 4] {
    let rig = |svg: &[u8]| Rig::parse(svg).unwrap();
    [
        (
            "swaledale",
            rig(include_bytes!("../../../pets/swaledale/swaledale.svg")),
        ),
        (
            "southdown",
            rig(include_bytes!(
                "../../../pets/southdown/southdown-agneau.svg"
            )),
        ),
        (
            "floucat",
            rig(include_bytes!("../../../pets/floucat/floucat.svg")),
        ),
        (
            "thones-et-marthod",
            rig(include_bytes!(
                "../../../pets/thones-et-marthod/thones-et-marthod.svg"
            )),
        ),
    ]
}

/// Couché (ou presque), endormi, la tête au sol, la queue lancée d'un côté
/// ou de l'autre, ondulant.
fn lying_poses() -> Vec<PoseSignals> {
    let mut out = Vec::new();
    for sit in [0.75, 0.9, 1.0] {
        let lying = PoseSignals {
            sit,
            ..PoseSignals::default()
        };
        let asleep = PoseSignals {
            sleep: 1.0,
            blink: 1.0,
            ..lying
        };
        out.extend([
            lying,
            asleep,
            PoseSignals {
                sleep_low: 1.0,
                ..asleep
            },
        ]);
        for (tail, tail_mid, tail_tip) in [
            (0.3, 0.1, 0.1),
            (-0.3, -0.1, -0.1),
            (0.6, 0.3, 0.3),
            (-0.6, -0.3, -0.3),
            (0.0, 0.3, -0.3),
        ] {
            out.push(PoseSignals {
                tail,
                tail_mid,
                tail_tip,
                ..lying
            });
        }
    }
    out
}

fn absolute(rig: &Rig, pose: &PoseTransforms, part: Part) -> Affine {
    let mut transform = pose.part(part);
    let mut parent = rig.parent(part);
    while let Some(part) = parent {
        transform = pose.part(part) * transform;
        parent = rig.parent(part);
    }
    pose.root * transform
}

/// Le point le plus bas des tracés de la queue (pas de leurs boîtes).
fn lowest_drawn_tail(rig: &Rig, posed: &PoseTransforms) -> f64 {
    let mut inside = Vec::new();
    let mut lowest = f64::NEG_INFINITY;
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
            } => {
                if let Some(&part) = inside.last()
                    && TAIL.contains(&part)
                {
                    let drawn = (absolute(rig, posed, part) * *transform) * path.clone();
                    lowest = lowest.max(drawn.bounding_box().max_y());
                }
            }
            _ => {}
        }
    }
    lowest
}

#[test]
fn a_long_tail_curves_onto_the_ground_instead_of_passing_through_it() {
    for (name, rig) in long_tails() {
        let u = rig.bounds.height() / 100.0;
        for facing in [1.0, -1.0] {
            for signals in lying_poses() {
                let posed = pose(&rig, &signals, facing, 0.0);
                let ground = (posed.root * rig.ground).y;
                let above = (ground - lowest_drawn_tail(&rig, &posed)) / u;
                assert!(
                    above >= 0.0,
                    "{name}, sens {facing}, {signals:?} : {:.1} u sous le sol",
                    -above
                );
            }
            // Couchée, elle repose sur l'herbe : elle ne se relève pas en l'air.
            let lying = PoseSignals {
                sit: 1.0,
                ..PoseSignals::default()
            };
            let posed = pose(&rig, &lying, facing, 0.0);
            let ground = (posed.root * rig.ground).y;
            let above = (ground - lowest_drawn_tail(&rig, &posed)) / u;
            assert!(
                above < 3.0,
                "{name}, sens {facing} : {above:.1} u au-dessus"
            );
        }
    }
}

#[test]
fn no_drawn_tail_passes_below_the_ground_whatever_the_breed() {
    let pets = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../pets");
    let mut rigs = 0;
    let mut wrong = Vec::new();
    for entry in std::fs::read_dir(&pets).unwrap() {
        let dir = entry.unwrap().path();
        let toml = std::fs::read_to_string(dir.join("pet.toml")).unwrap();
        // Le dessin de base, puis ceux de son sexe et de son âge
        // (`[appearance.female]`…).
        let files = toml
            .lines()
            .filter_map(|line| line.strip_prefix("rig = "))
            .map(|value| {
                value
                    .split('#')
                    .next()
                    .unwrap_or(value)
                    .trim()
                    .trim_matches('"')
            });
        for file in files {
            let rig = Rig::parse(&std::fs::read(dir.join(file)).unwrap()).unwrap();
            if !rig.has_part(Part::Tail) {
                continue;
            }
            rigs += 1;
            let u = rig.bounds.height() / 100.0;
            for facing in [1.0, -1.0] {
                for signals in lying_poses() {
                    let posed = pose(&rig, &signals, facing, 0.0);
                    let ground = (posed.root * rig.ground).y;
                    let above = (ground - lowest_drawn_tail(&rig, &posed)) / u;
                    if above < 0.0 {
                        wrong.push(format!(
                            "{file}, sens {facing}, assis {}, endormi {}, queue {} : {:.2} u sous le sol",
                            signals.sit, signals.sleep, signals.tail, -above
                        ));
                    }
                }
            }
        }
    }
    assert!(wrong.is_empty(), "{}", wrong.join("\n"));
    assert!(rigs >= 20, "{rigs} dessins");
}

/// L'angle d'une rotation (sans miroir).
fn angle(t: Affine) -> f64 {
    let [a, b, ..] = t.as_coeffs();
    b.atan2(a)
}

#[test]
fn every_piece_of_the_tail_bends_backwards_by_the_same_angle_only_when_it_touches() {
    let signals = PoseSignals {
        tail: 0.1,
        tail_mid: -0.05,
        tail_tip: 0.04,
        ..PoseSignals::default()
    };
    let (mid_angle, tip_angle) = (f64::from(signals.tail_mid), f64::from(signals.tail_tip));
    for (name, rig) in long_tails() {
        // Debout, la queue ne touche pas le sol : les angles du moteur, tels quels.
        let standing = pose(&rig, &signals, 1.0, 0.0);
        let mid = rig.pivot(Part::TailMid).to_vec2();
        let expected = Affine::translate(mid) * Affine::rotate(mid_angle) * Affine::translate(-mid);
        assert_eq!(standing.part(Part::TailMid), expected, "{name}");
        assert!((angle(standing.part(Part::TailTip)) - tip_angle).abs() < 1e-12);
        // Couchée, chaque morceau se plie vers l'arrière (un angle positif
        // tourne le bas d'une queue tournée à droite vers l'arrière, à
        // gauche) du même angle, et chacun reste accroché au précédent.
        let lying = pose(
            &rig,
            &PoseSignals {
                sit: 1.0,
                ..signals
            },
            1.0,
            0.0,
        );
        let bend = angle(lying.part(Part::TailMid)) - mid_angle;
        assert!(bend > 0.05, "{name} : {bend}");
        assert!((angle(lying.part(Part::TailTip)) - tip_angle - bend).abs() < 1e-9);
        let root = angle(lying.part(Part::Tail)) - angle(standing.part(Part::Tail));
        assert!((root - bend).abs() < 1e-9, "{name} : {root} {bend}");
        for part in [Part::TailMid, Part::TailTip] {
            let pivot = rig.pivot(part);
            let parent = absolute(&rig, &lying, rig.parent(part).unwrap());
            let child = absolute(&rig, &lying, part);
            assert!(
                (child * pivot - parent * pivot).hypot() < 1e-9,
                "{name}, {part:?}"
            );
        }
    }
}

#[test]
fn a_painted_long_tail_curves_onto_the_ground_too() {
    let dir = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../pets/solognote");
    let text = std::fs::read_to_string(dir.join("squelette.toml")).unwrap();
    let skeleton = parse_skeleton(&text, |image| {
        std::fs::read(dir.join(image))
            .map(Into::into)
            .map_err(|e| e.to_string())
    })
    .unwrap();
    // Les pixels qui se voient des morceaux de la queue (le bas de chacun).
    let mut tail: Vec<(usize, Vec<Point>)> = Vec::new();
    for piece in &skeleton.pieces {
        if skeleton.bones[piece.bone].role != Some(BoneRole::Tail) {
            continue;
        }
        let bytes = piece.png.bytes().unwrap().to_vec();
        let mut reader = png::Decoder::new(std::io::Cursor::new(bytes))
            .read_info()
            .unwrap();
        let mut rgba = vec![0; reader.output_buffer_size().unwrap()];
        let info = reader.next_frame(&mut rgba).unwrap();
        assert_eq!(info.color_type, png::ColorType::Rgba);
        let width = info.width as usize;
        let mut points = Vec::new();
        for (i, pixel) in rgba.as_chunks::<4>().0.iter().enumerate() {
            if pixel[3] > 0 {
                let x = f64::from(piece.at.x) + (i % width) as f64;
                let y = f64::from(piece.at.y) + (i / width) as f64 + 1.0;
                points.extend([Point::new(x, y), Point::new(x + 1.0, y)]);
            }
        }
        assert!(!points.is_empty());
        tail.push((piece.bone, points));
    }
    assert_eq!(tail.len(), 3);
    let art = SkeletonArt::load(Arc::new(skeleton), 60.0).unwrap();
    let ground = art.ground().y;
    let u = art.bounds.height() / 100.0;
    let lowest = |signals: &PoseSignals, facing: f32| {
        let posed = art.pose(signals, facing, 0.0);
        let mut lowest = f64::NEG_INFINITY;
        for (bone, points) in &tail {
            let at = posed.root * posed.bones[*bone];
            for point in points {
                lowest = lowest.max((at * *point).y);
            }
        }
        lowest
    };
    // Debout, elle pend bien au-dessus du sol ; couchée, elle y passait de 8,7 u.
    assert!(ground - lowest(&PoseSignals::default(), 1.0) > 15.0 * u);
    for facing in [1.0, -1.0] {
        for signals in lying_poses() {
            let above = (ground - lowest(&signals, facing)) / u;
            assert!(
                above >= 0.0,
                "sens {facing}, {signals:?} : {:.1} u sous le sol",
                -above
            );
        }
        let lying = PoseSignals {
            sit: 1.0,
            ..PoseSignals::default()
        };
        let above = (ground - lowest(&lying, facing)) / u;
        assert!(above < 3.0, "sens {facing} : {above:.1} u au-dessus");
    }
}
