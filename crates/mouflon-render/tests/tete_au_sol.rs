//! Endormi la tête au sol, l'encolure se couche sur l'herbe et la tête s'y
//! pose. Si l'encolure (son collier, sa gorge, ses touffes) touche l'herbe
//! avant le menton, la tête s'arrête là, posée dessus : ni l'encolure, ni la
//! tête, ni la cloche ne passent sous l'herbe, pour toutes les races
//! dessinées. Avant, le collier du Mérinos d'Arles y passait de 6 cm, la
//! gorge de l'Ouessant de près de 2 cm.
//!
//! Seule compte l'encolure au-delà de son premier tiers. Derrière sa base,
//! la laine du poitrail que son dessin recouvre pour cacher son raccord ne
//! descend pas avec la tête en vrai ; et son premier tiers, contre le
//! poitrail, plie au lieu de tourner : sa laine se couche sur l'herbe sans
//! arrêter la tête. Ce test les mesure sans les compter (voir
//! `docs/animation-animale.md`, § 6 et § 11).

use std::path::Path;
use std::sync::Arc;

use mouflon_core::{BoneRole, PoseSignals, parse_skeleton};
use mouflon_render::animate::{PoseTransforms, pose};
use mouflon_render::kurbo::{Affine, BezPath, PathEl, Point, Vec2, flatten};
use mouflon_render::rig::{Item, Part, Rig};
use mouflon_render::skeleton::SkeletonArt;

/// Ce qu'on tolère sous le sol (l'anticrénelage), en u.
const UNDER: f64 = 0.3;

fn pets() -> std::path::PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../../pets")
}

/// Tous les dessins vectoriels du dépôt.
fn rigs() -> Vec<(String, Rig)> {
    let mut out = Vec::new();
    for entry in std::fs::read_dir(pets()).unwrap() {
        let dir = entry.unwrap().path();
        let Ok(toml) = std::fs::read_to_string(dir.join("pet.toml")) else {
            continue;
        };
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
        for (k, file) in files.enumerate() {
            let rig = Rig::parse(&std::fs::read(dir.join(file)).unwrap()).unwrap();
            let name = dir.file_name().unwrap().to_string_lossy().into_owned();
            out.push((
                if k == 0 {
                    name
                } else {
                    format!("{name} ({file})")
                },
                rig,
            ));
        }
    }
    out.sort_by(|a, b| a.0.cmp(&b.0));
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

/// La part de l'encolure, depuis sa base, qui n'arrête pas la tête.
const ROOT: f64 = 1.0 / 3.0;

/// Ce qu'on regarde : l'encolure (et ses touffes) au-delà de son premier
/// tiers (son collier, sa gorge, sa barbe), son premier tiers, la laine du
/// poitrail qu'elle dessine derrière sa base, la tête et ce qu'elle porte,
/// la cloche.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
enum Group {
    Neck,
    Root,
    Chest,
    Head,
    Bell,
}

const GROUPS: [Group; 5] = [
    Group::Neck,
    Group::Root,
    Group::Chest,
    Group::Head,
    Group::Bell,
];

/// La tête (et ce qu'elle porte) ou la cloche ; l'encolure et ses touffes
/// (`None`) se coupent en trois.
fn group_of(rig: &Rig, part: Part) -> Option<Option<Group>> {
    if part == Part::Bell {
        return Some(Some(Group::Bell));
    }
    if part == Part::Neck || Part::NECK_SWAYS.contains(&part) {
        return Some(None);
    }
    let mut up = Some(part);
    while let Some(p) = up {
        if p == Part::Head {
            return Some(Some(Group::Head));
        }
        up = rig.parent(p);
    }
    None
}

/// L'enveloppe convexe de points.
fn hull(points: &[Point]) -> Vec<Point> {
    let mut pts = points.to_vec();
    pts.sort_by(|a, b| a.x.total_cmp(&b.x).then(a.y.total_cmp(&b.y)));
    pts.dedup();
    if pts.len() < 3 {
        return pts;
    }
    let cross = |o: Point, a: Point, b: Point| (a - o).cross(b - o);
    let mut lower: Vec<Point> = Vec::new();
    for &p in &pts {
        while lower.len() >= 2 && cross(lower[lower.len() - 2], lower[lower.len() - 1], p) <= 0.0 {
            lower.pop();
        }
        lower.push(p);
    }
    let mut upper: Vec<Point> = Vec::new();
    for &p in pts.iter().rev() {
        while upper.len() >= 2 && cross(upper[upper.len() - 2], upper[upper.len() - 1], p) <= 0.0 {
            upper.pop();
        }
        upper.push(p);
    }
    lower.pop();
    upper.pop();
    lower.extend(upper);
    lower
}

/// Les sous-tracés d'un tracé (repère du dessin), aplatis en segments, et
/// s'ils sont fermés.
fn polylines(path: &BezPath, transform: Affine) -> Vec<(Vec<Point>, bool)> {
    let mut lines: Vec<(Vec<Point>, bool)> = Vec::new();
    flatten(path.elements().iter().copied(), 0.05, |el| match el {
        PathEl::MoveTo(p) => lines.push((vec![transform * p], false)),
        PathEl::LineTo(p) => match lines.last_mut() {
            Some((line, _)) => line.push(transform * p),
            None => lines.push((vec![transform * p], false)),
        },
        PathEl::ClosePath => {
            if let Some((_, closed)) = lines.last_mut() {
                *closed = true;
            }
        }
        _ => {}
    });
    lines
}

/// Les points d'une forme dont `side` est entre `lo` et `hi` (ses sommets,
/// et où ses côtés passent ces bornes), élargis d'un demi-trait `half`.
fn clipped(
    line: &[Point],
    closed: bool,
    side: impl Fn(Point) -> f64,
    (lo, hi): (f64, f64),
    half: f64,
    out: &mut Vec<Point>,
) {
    let mut keep = |p: Point| {
        if half > 0.0 {
            for k in 0..8 {
                let (s, c) = (f64::from(k) * std::f64::consts::PI / 4.0).sin_cos();
                out.push(p + Vec2::new(c, s) * half);
            }
        } else {
            out.push(p);
        }
    };
    let n = line.len();
    for k in 0..n {
        let p = line[k];
        if (lo..hi).contains(&side(p)) {
            keep(p);
        }
        if k + 1 == n && !closed {
            break;
        }
        let q = line[(k + 1) % n];
        for bound in [lo, hi] {
            let (a, b) = (side(p) - bound, side(q) - bound);
            if bound.is_finite() && (a < 0.0) != (b < 0.0) {
                keep(p + (q - p) * (a / (a - b)));
            }
        }
    }
}

/// Le contour de chaque partie de l'encolure et de la tête (repère du
/// dessin ; un trait, élargi d'un demi-trait) : l'enveloppe convexe de ses
/// tracés ; l'encolure et ses touffes, coupées en trois à la perpendiculaire
/// de l'encolure (derrière sa base, son premier tiers, au-delà).
fn outlines(rig: &Rig) -> Vec<(Part, Group, Vec<Point>)> {
    let base = rig.pivot(Part::Neck);
    let poll = rig.pivot(Part::Head);
    let length = (poll - base).hypot();
    let along = (poll - base) / length;
    let side = |p: Point| (p - base).dot(along);
    let bands = [
        (Group::Chest, (f64::NEG_INFINITY, 0.0)),
        (Group::Root, (0.0, ROOT * length)),
        (Group::Neck, (ROOT * length, f64::INFINITY)),
    ];
    let mut inside = Vec::new();
    let mut found: Vec<(Part, Group, Vec<Point>)> = Vec::new();
    for item in &rig.items {
        let (path, transform, half, filled) = match item {
            Item::BeginPart(p) => {
                inside.push(*p);
                continue;
            }
            Item::EndPart => {
                inside.pop();
                continue;
            }
            Item::Fill {
                path, transform, ..
            } => (path, *transform, 0.0, true),
            Item::Stroke {
                path,
                transform,
                style,
                ..
            } => (
                path,
                *transform,
                0.5 * style.width * transform.determinant().abs().sqrt(),
                false,
            ),
            _ => continue,
        };
        let Some(&part) = inside.last() else {
            continue;
        };
        let Some(group) = group_of(rig, part) else {
            continue;
        };
        let wanted: Vec<(Group, (f64, f64))> = match group {
            Some(group) => vec![(group, (f64::NEG_INFINITY, f64::INFINITY))],
            None => bands.to_vec(),
        };
        for (group, range) in wanted {
            let k = match found.iter().position(|(p, g, _)| *p == part && *g == group) {
                Some(k) => k,
                None => {
                    found.push((part, group, Vec::new()));
                    found.len() - 1
                }
            };
            for (line, closed) in polylines(path, transform) {
                clipped(&line, closed || filled, side, range, half, &mut found[k].2);
            }
        }
    }
    for (_, _, points) in &mut found {
        *points = hull(points);
    }
    found
}

/// Endormi, la tête qui descend au sol, puis posée ; il respire, une
/// oreille frémit.
fn head_down() -> Vec<(String, PoseSignals)> {
    let asleep = PoseSignals {
        sit: 1.0,
        sleep: 1.0,
        blink: 1.0,
        breath: 0.5,
        ..PoseSignals::default()
    };
    let mut out = Vec::new();
    for k in 1..=20 {
        let low = k as f32 / 20.0;
        out.push((
            format!("tête au sol {low}"),
            PoseSignals {
                sleep_low: low,
                ..asleep
            },
        ));
    }
    for breath in [-1.0, 0.0, 1.0] {
        for twitch in [0.0, 1.0] {
            out.push((
                format!("tête au sol, souffle {breath}, frémit {twitch}"),
                PoseSignals {
                    sleep_low: 1.0,
                    breath,
                    twitch,
                    ..asleep
                },
            ));
        }
    }
    out
}

/// Le plus bas de chaque groupe, au-dessous du sol (en u, un centième de la
/// hauteur du dessin ; négatif : au-dessus).
fn lowest(rig: &Rig, parts: &[(Part, Group, Vec<Point>)], posed: &PoseTransforms) -> [f64; 5] {
    let u = rig.bounds.height() / 100.0;
    let mut out = [f64::NEG_INFINITY; 5];
    for (part, group, points) in parts {
        let at = absolute(rig, posed, *part);
        let k = GROUPS.iter().position(|g| g == group).unwrap();
        for &p in points {
            out[k] = out[k].max(((at * p).y - rig.ground.y) / u);
        }
    }
    out
}

/// De combien l'animal entier est remonté, endormi la tête au sol (en u).
fn lifted(rig: &Rig, posed: &PoseTransforms) -> f64 {
    -posed.root.translation().y / (rig.bounds.height() / 100.0)
}

/// Le bas de la boîte de la tête posée, sous le sol (repère du dessin ;
/// négatif : au-dessus).
fn chin(rig: &Rig, posed: &PoseTransforms) -> f64 {
    let head = absolute(rig, posed, Part::Head);
    let b = rig.part_bounds(Part::Head);
    [
        Point::new(b.x0, b.y0),
        Point::new(b.x1, b.y0),
        Point::new(b.x0, b.y1),
        Point::new(b.x1, b.y1),
    ]
    .iter()
    .map(|&c| (head * c).y)
    .fold(f64::NEG_INFINITY, f64::max)
        - rig.ground.y
}

#[test]
fn asleep_with_the_head_down_no_neck_head_or_bell_goes_below_the_grass() {
    let rigs = rigs();
    assert!(rigs.len() >= 25, "{} dessins", rigs.len());
    let mut wrong = Vec::new();
    for (name, rig) in &rigs {
        let parts = outlines(rig);
        let mut worst = [f64::NEG_INFINITY; 5];
        for facing in [1.0, -1.0] {
            for (pose_name, signals) in head_down() {
                let posed = pose(rig, &signals, facing, 0.0);
                let below = lowest(rig, &parts, &posed);
                for (k, group) in GROUPS.iter().enumerate() {
                    worst[k] = worst[k].max(below[k]);
                    if below[k] > UNDER {
                        wrong.push(format!(
                            "{name} {group:?}, {pose_name}, sens {facing} : {:.2} u sous le sol",
                            below[k]
                        ));
                    }
                }
            }
        }
        // De combien la tête s'arrête au-dessus de l'herbe (le bas de sa
        // boîte, comme le moteur la pose).
        let posed = pose(
            rig,
            &PoseSignals {
                sit: 1.0,
                sleep: 1.0,
                sleep_low: 1.0,
                ..PoseSignals::default()
            },
            1.0,
            0.0,
        );
        let u = rig.bounds.height() / 100.0;
        let mut line = format!("{name:20}");
        for (group, below) in GROUPS.iter().zip(worst) {
            if below.is_finite() {
                line += &format!(" {group:?} {below:6.2} u ({:5.1}) ;", below * u);
            }
        }
        let chin = -chin(rig, &posed);
        line += &format!(" menton {:5.2} u ({:4.1})", chin / u, chin);
        println!("MESURE {line}");
    }
    assert!(wrong.is_empty(), "{}", wrong.join("\n"));
}

#[test]
fn the_head_rests_on_its_chin_or_on_its_neck_whichever_touches_the_grass_first() {
    // Endormi la tête au sol : ou bien le menton touche l'herbe (le bas de
    // la boîte de la tête, à un centième de la hauteur du sol, comme avant),
    // ou bien c'est l'encolure, à un demi-centième, et la tête s'arrête
    // au-dessus.
    for (name, rig) in rigs() {
        let parts = outlines(&rig);
        let u = rig.bounds.height() / 100.0;
        let asleep = PoseSignals {
            sit: 1.0,
            sleep: 1.0,
            sleep_low: 1.0,
            ..PoseSignals::default()
        };
        let posed = pose(&rig, &asleep, 1.0, 0.0);
        // L'animal entier remonté d'un rien (au plus 1,5 u) : le premier
        // tiers de son encolure passerait sinon sous l'herbe.
        let lift = lifted(&rig, &posed);
        assert!(
            (0.0..=1.5 + 1e-9).contains(&lift),
            "{name} : remonté de {lift}"
        );
        let chin = chin(&rig, &posed) / u + lift;
        let below = lowest(&rig, &parts, &posed);
        let neck = below[0] + lift;
        let on_chin = (chin + 1.0).abs() < 1e-6;
        let on_neck = (neck + 0.5).abs() < 0.02;
        assert!(
            on_chin && neck < -0.5 + 1e-6 || on_neck && chin < -1.0,
            "{name} : le menton à {chin:.3} u, l'encolure à {neck:.3} u, remonté de {lift:.3} u"
        );
    }
}

#[test]
fn the_merino_rests_its_head_on_its_collar_and_comes_down_without_a_jump() {
    // Le floucat : le Mérinos d'Arles qui porte le collier et le redon.
    let rig = Rig::parse(include_bytes!("../../../pets/floucat/floucat.svg")).unwrap();
    let parts = outlines(&rig);
    let u = rig.bounds.height() / 100.0;
    let asleep = |low: f32| PoseSignals {
        sit: 1.0,
        sleep: 1.0,
        breath: 0.4,
        sleep_low: low,
        ..PoseSignals::default()
    };
    for facing in [1.0, -1.0] {
        // Le collier se pose sur l'herbe, la tête au-dessus, à plus de 3 u.
        let posed = pose(&rig, &asleep(1.0), facing, 0.0);
        let below = lowest(&rig, &parts, &posed);
        assert!(below[0] > -1.0 && below[0] <= 0.0, "{below:?}");
        assert!(chin(&rig, &posed) / u < -3.0);
        // Elle descend sans saut : la nuque et le collier, pas à pas.
        let marks = |posed: &PoseTransforms| {
            let neck = absolute(&rig, posed, Part::Neck);
            [rig.pivot(Part::Head), rig.pivot(Part::Bell)].map(|p| neck * p)
        };
        let mut last = marks(&pose(&rig, &asleep(0.0), facing, 0.0));
        for k in 1..=2000 {
            let posed = pose(&rig, &asleep(k as f32 / 2000.0), facing, 0.0);
            let now = marks(&posed);
            for (a, b) in last.iter().zip(&now) {
                assert!((*a - *b).hypot() < 0.1 * u, "{k} : {a:?} puis {b:?}");
            }
            last = now;
        }
    }
}

/// Une petite encolure dressée, sa tête, et, si `collar`, un collier qui
/// pend sous la gorge.
fn small(collar: bool) -> Rig {
    let svg = format!(
        r##"<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 140 100">
        <g id="neck"><path d="M66 60L82 26L94 30L84 66Z" fill="#ddd"/>{}
            <circle id="pivot-neck" cx="72" cy="58" r="0.5"/>
            <g id="head"><ellipse cx="100" cy="30" rx="14" ry="8" fill="#888"/>
                <circle id="pivot-head" cx="88" cy="28" r="0.5"/>
                <circle id="pivot-muzzle" cx="113" cy="35" r="0.5"/></g></g>
        <g id="body"><ellipse cx="45" cy="55" rx="30" ry="18" fill="#eee"/></g>
        <circle id="pivot-ground" cx="45" cy="96" r="0.5"/></svg>"##,
        if collar {
            r##"<path d="M80 44L92 48L92 60L80 56Z" fill="#a73"/>"##
        } else {
            ""
        }
    );
    Rig::parse(svg.as_bytes()).unwrap()
}

#[test]
fn a_collar_stops_the_head_above_the_grass_and_a_bare_neck_still_lays_its_chin_down() {
    let asleep = PoseSignals {
        sit: 1.0,
        sleep: 1.0,
        sleep_low: 1.0,
        ..PoseSignals::default()
    };
    let (bare, collared) = (small(false), small(true));
    let lowest_neck = |rig: &Rig, posed: &PoseTransforms| lowest(rig, &outlines(rig), posed)[0];
    // Sans collier : le menton se pose, l'encolure reste au-dessus.
    let u = bare.bounds.height() / 100.0;
    let posed = pose(&bare, &asleep, 1.0, 0.0);
    assert!((chin(&bare, &posed) / u + 1.0).abs() < 1e-6);
    assert!(lowest_neck(&bare, &posed) < -0.5);
    // Avec : le collier touche l'herbe le premier, la tête s'arrête plus
    // haut. (L'animal entier est remonté d'un rien : le premier tiers de
    // cette grosse encolure passerait sinon sous l'herbe.)
    let u = collared.bounds.height() / 100.0;
    let posed = pose(&collared, &asleep, 1.0, 0.0);
    let lift = lifted(&collared, &posed);
    assert!((0.0..=1.5 + 1e-9).contains(&lift), "{lift}");
    let neck = lowest_neck(&collared, &posed) + lift;
    assert!((neck + 0.5).abs() < 0.02, "{neck}");
    assert!(chin(&collared, &posed) / u + lift < -1.5);
    // Debout, couché la tête haute, en broutant : le collier ne change rien
    // (couché, le corps se pose seulement un rien plus haut ou plus bas :
    // sa hauteur se mesure sur ce que fait l'encolure en s'endormant).
    for signals in [
        PoseSignals::default(),
        PoseSignals {
            sit: 1.0,
            ..PoseSignals::default()
        },
        PoseSignals {
            graze: 1.0,
            ..PoseSignals::default()
        },
    ] {
        let a = pose(&bare, &signals, 1.0, 0.0);
        let b = pose(&collared, &signals, 1.0, 0.0);
        for part in [Part::Neck, Part::Head] {
            let (x, y) = (a.part(part).as_coeffs(), b.part(part).as_coeffs());
            for (k, (p, q)) in x.iter().zip(&y).enumerate() {
                let height = k == 5 && part == Part::Neck && signals.sit > 0.0;
                let near = if height { 2.5 * u } else { 1e-9 };
                assert!((p - q).abs() < near, "{part:?} : {x:?} {y:?}");
            }
        }
    }
}

/// Les pixels qui se voient d'une image : les deux bouts de chaque rangée.
fn visible(png: &[u8], at: Point) -> Vec<Point> {
    let mut reader = png::Decoder::new(std::io::Cursor::new(png.to_vec()))
        .read_info()
        .unwrap();
    let mut rgba = vec![0; reader.output_buffer_size().unwrap()];
    let info = reader.next_frame(&mut rgba).unwrap();
    assert_eq!(info.color_type, png::ColorType::Rgba);
    let width = info.width as usize;
    let mut out = Vec::new();
    for (y, row) in rgba.chunks(width * 4).enumerate() {
        let seen: Vec<usize> = row
            .chunks(4)
            .enumerate()
            .filter(|(_, p)| p[3] > 0)
            .map(|(x, _)| x)
            .collect();
        if let (Some(&x0), Some(&x1)) = (seen.first(), seen.last()) {
            for (x, dy) in [(x0, 0), (x0, 1), (x1 + 1, 0), (x1 + 1, 1)] {
                out.push(Point::new(at.x + x as f64, at.y + (y + dy) as f64));
            }
        }
    }
    out
}

#[test]
fn asleep_with_the_head_down_the_painted_neck_and_head_stay_above_the_grass() {
    // La Solognote, peinte sur son squelette : son encolure reste loin de
    // l'herbe (le moteur ne lit pas les images de ses morceaux).
    let dir = pets().join("solognote");
    let text = std::fs::read_to_string(dir.join("squelette.toml")).unwrap();
    let skeleton = parse_skeleton(&text, |image| {
        std::fs::read(dir.join(image))
            .map(Into::into)
            .map_err(|e| e.to_string())
    })
    .unwrap();
    let group = |bone: usize| {
        let mut up = Some(bone);
        while let Some(b) = up {
            match skeleton.bones[b].role {
                Some(BoneRole::Neck) => return Some(0),
                Some(BoneRole::Head) => return Some(1),
                _ => up = skeleton.bones[b].parent,
            }
        }
        None
    };
    let pieces: Vec<(usize, usize, Vec<Point>)> = skeleton
        .pieces
        .iter()
        .filter_map(|piece| {
            let at = Point::new(f64::from(piece.at.x), f64::from(piece.at.y));
            Some((
                piece.bone,
                group(piece.bone)?,
                hull(&visible(&piece.png.bytes().unwrap(), at)),
            ))
        })
        .collect();
    let art = SkeletonArt::load(Arc::new(skeleton.clone()), 60.0).unwrap();
    let ground = art.ground().y;
    let u = art.bounds.height() / 100.0;
    let mut worst = [f64::NEG_INFINITY; 2];
    for facing in [1.0, -1.0] {
        for (_, signals) in head_down() {
            let posed = art.pose(&signals, facing, 0.0);
            for (bone, k, points) in &pieces {
                let t = posed.root * posed.bones[*bone];
                for &p in points {
                    worst[*k] = worst[*k].max(((t * p).y - ground) / u);
                }
            }
        }
    }
    println!(
        "MESURE solognote            Neck {:6.2} u ; Head {:6.2} u",
        worst[0], worst[1]
    );
    assert!(worst.iter().all(|&w| w <= UNDER), "{worst:?}");
}
