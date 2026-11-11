//! Couché sur le sternum, un mouton replie ses pattes sous lui : les
//! antérieurs sous le poitrail, les postérieurs le long du ventre, les
//! sabots rentrés. Le haut d'une patte (le bras et l'avant-bras, la cuisse
//! et la jambe, dessinés d'une pièce) était plus long que la hanche n'est
//! haute au-dessus du sol : il s'allongeait devant le poitrail et derrière
//! la croupe, de 22 u chez l'agnelle Southdown, de 16 et 13 u chez le
//! Mérinos d'Arles tondu, et le sabot replié passait sous le sol. Il rentre
//! maintenant sous le corps.

use std::path::Path;
use std::sync::Arc;

use mouflon_core::{BoneRole, PoseSignals, parse_skeleton};
use mouflon_render::animate::{PoseTransforms, pose};
use mouflon_render::kurbo::{
    Affine, BezPath, Line, ParamCurve, ParamCurveExtrema, Point, Rect, Shape, Vec2,
};
use mouflon_render::rig::{Item, Part, Rig};
use mouflon_render::skeleton::SkeletonArt;

const LEGS: [Part; 4] = [
    Part::LegFrontNear,
    Part::LegFrontFar,
    Part::LegBackNear,
    Part::LegBackFar,
];

/// De combien (en u, un centième de la hauteur du dessin) une patte
/// repliée peut dépasser du corps : le genou un peu devant la poitrine, le
/// jarret un peu derrière la fesse.
const OUT: f64 = 3.0;
/// Ce qu'on tolère sous le sol (l'anticrénelage), en u.
const UNDER: f64 = 0.3;

fn pets() -> std::path::PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../../pets")
}

/// Tous les dessins vectoriels du dépôt, avec leurs pattes en deux segments.
fn rigs() -> Vec<(String, Rig)> {
    let mut out = Vec::new();
    for entry in std::fs::read_dir(pets()).unwrap() {
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

/// Les tracés que dessine chaque patte elle-même (sans son bas), et son bas.
fn leg_paths(rig: &Rig) -> Vec<(Part, BezPath)> {
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
            } => {
                if let Some(&part) = inside.last()
                    && (part.is_leg() || part.is_lower_leg())
                {
                    out.push((part, *transform * path.clone()));
                }
            }
            _ => {}
        }
    }
    out
}

/// La boîte des tracés d'une partie, posés.
fn drawn(rig: &Rig, posed: &PoseTransforms, paths: &[(Part, BezPath)], part: Part) -> Rect {
    let at = absolute(rig, posed, part);
    paths
        .iter()
        .filter(|(p, _)| *p == part)
        .map(|(_, path)| (at * path.clone()).bounding_box())
        .reduce(|a, b| a.union(b))
        .unwrap()
}

/// La boîte d'un rectangle du dessin, posé.
fn posed_rect(t: Affine, r: Rect) -> Rect {
    let corners = [
        Point::new(r.x0, r.y0),
        Point::new(r.x1, r.y0),
        Point::new(r.x0, r.y1),
        Point::new(r.x1, r.y1),
    ]
    .map(|c| t * c);
    corners
        .iter()
        .skip(1)
        .fold(Rect::from_points(corners[0], corners[0]), |r, &c| {
            r.union_pt(c)
        })
}

/// Il se couche, couché, endormi, la tête au sol.
fn lying_poses() -> Vec<PoseSignals> {
    let mut out: Vec<PoseSignals> = [0.25, 0.5, 0.75, 0.9]
        .map(|sit| PoseSignals {
            sit,
            ..PoseSignals::default()
        })
        .to_vec();
    let lying = PoseSignals {
        sit: 1.0,
        breath: 0.5,
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
        PoseSignals {
            sleep_low: 1.0,
            twitch: 1.0,
            ..asleep
        },
    ]);
    out
}

/// De combien une boîte dépasse d'une autre devant l'animal et derrière
/// lui (positif : elle dépasse), selon le sens où il regarde.
fn beyond(shape: Rect, body: Rect, facing: f32) -> (f64, f64) {
    if facing > 0.0 {
        (shape.x1 - body.x1, body.x0 - shape.x0)
    } else {
        (body.x0 - shape.x0, shape.x1 - body.x1)
    }
}

/// Il se replie déjà : l'avant d'abord, l'arrière à partir du tiers du
/// coucher (voir `animate::pose`).
fn folding(front: bool, sit: f32) -> bool {
    if front { sit > 0.0 } else { sit > 0.35 }
}

#[test]
fn folded_legs_stay_under_the_body_of_every_breed_and_never_below_the_ground() {
    let rigs = rigs();
    assert!(rigs.len() >= 25, "{} dessins", rigs.len());
    let mut legs = 0;
    let mut wrong = Vec::new();
    for (name, rig) in &rigs {
        let paths = leg_paths(rig);
        let u = rig.bounds.height() / 100.0;
        let body_rest = rig.own_bounds(Part::Body).unwrap();
        for facing in [1.0, -1.0] {
            for signals in lying_poses() {
                let posed = pose(rig, &signals, facing, 0.0);
                let ground = (posed.root * rig.ground).y;
                let body = posed_rect(absolute(rig, &posed, Part::Body), body_rest);
                for leg in LEGS {
                    let lower = leg.lower().unwrap();
                    let front = matches!(leg, Part::LegFrontNear | Part::LegFrontFar);
                    if !rig.has_part(lower) || !folding(front, signals.sit) {
                        continue;
                    }
                    legs += 1;
                    let shape =
                        drawn(rig, &posed, &paths, leg).union(drawn(rig, &posed, &paths, lower));
                    let below = (shape.y1 - ground) / u;
                    if below >= UNDER {
                        wrong.push(format!(
                            "{name} {leg:?}, sens {facing}, assis {}, endormi {}, tête au sol {} : {below:.2} u sous le sol",
                            signals.sit, signals.sleep, signals.sleep_low
                        ));
                    }
                    // Couché tout à fait : sous le corps, au plus un peu devant
                    // la poitrine ou derrière la fesse.
                    if signals.sit < 1.0 {
                        continue;
                    }
                    let (ahead, behind) = beyond(shape, body, facing);
                    if ahead / u >= OUT || behind / u >= OUT {
                        wrong.push(format!(
                            "{name} {leg:?}, sens {facing}, endormi {} : {:.1} u devant, {:.1} u derrière",
                            signals.sleep,
                            ahead / u,
                            behind / u
                        ));
                    }
                }
            }
        }
    }
    assert!(wrong.is_empty(), "{}", wrong.join("\n"));
    assert!(legs > 1000, "{legs}");
}

#[test]
fn a_lamb_folds_its_long_legs_under_its_chest_and_along_its_belly() {
    // L'agnelle Southdown, huit semaines : ses genoux allaient au-delà du
    // museau. Le carpe se pose sous le poitrail, le jarret sous la fesse,
    // les sabots sous le ventre.
    let rig = Rig::parse(include_bytes!(
        "../../../pets/southdown/southdown-agneau.svg"
    ))
    .unwrap();
    let u = rig.bounds.height() / 100.0;
    let posed = pose(
        &rig,
        &PoseSignals {
            sit: 1.0,
            ..PoseSignals::default()
        },
        1.0,
        0.0,
    );
    let body = posed_rect(
        absolute(&rig, &posed, Part::Body),
        rig.own_bounds(Part::Body).unwrap(),
    );
    let ground = (posed.root * rig.ground).y;
    for leg in [Part::LegFrontNear, Part::LegBackNear] {
        let lower = leg.lower().unwrap();
        let t = absolute(&rig, &posed, lower);
        let knee = t * rig.pivot(lower);
        let b = rig.part_bounds(lower);
        let hoof = t * Point::new(b.center().x, b.max_y());
        // L'articulation au sol, sous le corps.
        assert!(ground - knee.y < 4.0 * u, "{leg:?} : genou en {knee:?}");
        assert!(
            knee.x > body.x0 && knee.x < body.x1,
            "{leg:?} : genou en {knee:?}, corps {body:?}"
        );
        // Le sabot rentré sous le ventre : derrière le genou devant, devant
        // le jarret derrière, et au-dessus du sol.
        assert!(hoof.x > body.x0 + 5.0 * u && hoof.x < body.x1 - 5.0 * u);
        assert!(hoof.y <= ground, "{leg:?} : sabot en {hoof:?}");
        if leg == Part::LegFrontNear {
            assert!(hoof.x < knee.x - 10.0 * u, "{knee:?} {hoof:?}");
        } else {
            assert!(hoof.x > knee.x + 10.0 * u, "{knee:?} {hoof:?}");
        }
    }
}

/// Les tracés du corps (surfaces et traits : le Zwartbles est peint en
/// lignes serrées), chacun dans le repère du dessin.
fn body_paths(rig: &Rig) -> Vec<BezPath> {
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
            } if inside.last() == Some(&Part::Body) => out.push(*transform * path.clone()),
            _ => {}
        }
    }
    out
}

/// Le corps cache le point `p` (dans le repère du dessin) : il est entre
/// le haut et le bas de ses tracés, à la verticale.
fn hidden(paths: &[BezPath], p: Point) -> bool {
    let line = Line::new((p.x, -1e4), (p.x, 1e4));
    let (mut top, mut bottom) = (f64::INFINITY, f64::NEG_INFINITY);
    for path in paths {
        for seg in path.segments() {
            let b = ParamCurveExtrema::bounding_box(&seg);
            if p.x < b.x0 || p.x > b.x1 {
                continue;
            }
            for hit in seg.intersect_line(line) {
                let y = seg.eval(hit.segment_t).y;
                top = top.min(y);
                bottom = bottom.max(y);
            }
        }
    }
    top < p.y && p.y < bottom
}

/// La patte est dessinée sous le corps : son premier tracé vient avant
/// ceux du corps.
fn under_body(rig: &Rig, leg: Part) -> bool {
    let mut inside = Vec::new();
    for item in &rig.items {
        match item {
            Item::BeginPart(p) => inside.push(*p),
            Item::EndPart => {
                inside.pop();
            }
            Item::Fill { .. } | Item::Stroke { .. } => match inside.last() {
                Some(&Part::Body) => return false,
                Some(&part) if part == leg => return true,
                _ => {}
            },
            _ => {}
        }
    }
    false
}

/// L'aire d'une transformation : 1 pour un déplacement et une rotation ;
/// moins, le haut de la patte raccourci.
fn area(t: Affine) -> f64 {
    let [a, b, c, d, ..] = t.as_coeffs();
    a * d - b * c
}

#[test]
fn the_upper_leg_slides_under_the_body_without_coming_out_through_the_back() {
    // Le haut d'une patte qui rentre sous le corps y glisse le long de
    // lui-même : son bord du haut reste caché par le corps, il ne sort pas
    // par le dos. Les pattes longues du mouflon, dessinées jusqu'au garrot,
    // n'ont pas toute la place : elles se raccourcissent du reste.
    let (mut legs, mut whole) = (0, 0);
    let mut wrong = Vec::new();
    for (name, rig) in &rigs() {
        let paths = body_paths(rig);
        for signals in [
            PoseSignals {
                sit: 1.0,
                ..PoseSignals::default()
            },
            PoseSignals {
                sit: 1.0,
                sleep: 1.0,
                sleep_low: 1.0,
                breath: 1.0,
                ..PoseSignals::default()
            },
        ] {
            let posed = pose(rig, &signals, 1.0, 0.0);
            let into_body = absolute(rig, &posed, Part::Body).inverse();
            for leg in LEGS {
                let Some(top) = rig.own_bounds(leg) else {
                    continue;
                };
                if !under_body(rig, leg) || !rig.has_part(leg.lower().unwrap()) {
                    continue;
                }
                let at = absolute(rig, &posed, leg);
                legs += 1;
                let squash = area(at);
                // Un agneau, haut sur pattes sous un corps peu profond, a
                // moins de place sous le dos : le haut de ses pattes se
                // raccourcit davantage.
                let least = if name.contains("-agneau") { 0.45 } else { 0.6 };
                if squash <= least {
                    wrong.push(format!("{name} {leg:?} : raccourci à {squash:.2}"));
                }
                if squash > 0.95 {
                    whole += 1;
                }
                // Ce que le corps cachait du haut de la patte, debout, il le
                // cache encore couché.
                for k in [0.25, 0.5, 0.75] {
                    let rest = Point::new(top.x0 + k * top.width(), top.y0);
                    if !hidden(&paths, rest) {
                        continue;
                    }
                    // À un centième de la hauteur près : couché un peu plus
                    // haut (voir `lying_lift`), le haut d'une patte d'agneau
                    // affleure la ligne du dos.
                    let p = at * rest;
                    let q = into_body * p;
                    let u = rig.bounds.height() / 100.0;
                    if !hidden(&paths, q) && !hidden(&paths, q + Vec2::new(0.0, u)) {
                        wrong.push(format!(
                            "{name} {leg:?}, endormi {} : le haut de la patte sort du corps en ({:.1}, {:.1})",
                            signals.sleep, p.x, p.y
                        ));
                    }
                }
            }
        }
    }
    assert!(wrong.is_empty(), "{}", wrong.join("\n"));
    // Presque toutes glissent d'une pièce, ou peu s'en faut.
    assert!(whole * 10 > legs * 9, "{whole} sur {legs}");
}

#[test]
fn a_lamb_and_a_shorn_merino_slide_their_upper_legs_almost_whole() {
    for (name, drawing) in [
        ("southdown", "southdown-agneau.svg"),
        ("floucat", "floucat.svg"),
    ] {
        let rig = Rig::parse(&std::fs::read(pets().join(name).join(drawing)).unwrap()).unwrap();
        for sleep in [0.0, 1.0] {
            let posed = pose(
                &rig,
                &PoseSignals {
                    sit: 1.0,
                    sleep,
                    ..PoseSignals::default()
                },
                1.0,
                0.0,
            );
            for leg in LEGS {
                // Le haut glisse sous le corps, raccourci d'un dixième au plus.
                let squash = area(absolute(&rig, &posed, leg));
                assert!(squash > 0.9, "{name} {leg:?} : {squash}");
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
fn painted_legs_fold_under_the_body_too() {
    // La Solognote, peinte sur son squelette.
    for name in ["solognote"] {
        let dir = pets().join(name);
        let text = std::fs::read_to_string(dir.join("squelette.toml")).unwrap();
        let skeleton = parse_skeleton(&text, |image| {
            std::fs::read(dir.join(image))
                .map(Into::into)
                .map_err(|e| e.to_string())
        })
        .unwrap();
        let pieces: Vec<(usize, Vec<Point>)> = skeleton
            .pieces
            .iter()
            .map(|piece| {
                let at = Point::new(f64::from(piece.at.x), f64::from(piece.at.y));
                (piece.bone, visible(&piece.png.bytes().unwrap(), at))
            })
            .collect();
        let role = |bone: usize| skeleton.bones[bone].role;
        let art = SkeletonArt::load(Arc::new(skeleton.clone()), 60.0).unwrap();
        let ground = art.ground().y;
        let u = art.bounds.height() / 100.0;
        let mut wrong = Vec::new();
        for facing in [1.0, -1.0] {
            for signals in lying_poses() {
                let posed = art.pose(&signals, facing, 0.0);
                let shape = |wanted: &dyn Fn(Option<BoneRole>) -> bool| {
                    let mut r: Option<Rect> = None;
                    for (bone, points) in &pieces {
                        if !wanted(role(*bone)) {
                            continue;
                        }
                        let t = posed.root * posed.bones[*bone];
                        for p in points {
                            let q = t * *p;
                            r = Some(r.map_or(Rect::from_points(q, q), |r| r.union_pt(q)));
                        }
                    }
                    r.unwrap()
                };
                let body = shape(&|r| r == Some(BoneRole::Body));
                for (front, near) in [(true, true), (true, false), (false, true), (false, false)] {
                    if !folding(front, signals.sit) {
                        continue;
                    }
                    let leg = shape(&|r| {
                        r == Some(BoneRole::Leg { front, near })
                            || r == Some(BoneRole::LegLower { front, near })
                    });
                    let below = (leg.y1 - ground) / u;
                    if below >= UNDER {
                        wrong.push(format!(
                            "{name} devant {front} près {near}, sens {facing}, assis {} : {below:.2} u sous le sol",
                            signals.sit
                        ));
                    }
                    if signals.sit < 1.0 {
                        continue;
                    }
                    let (ahead, behind) = beyond(leg, body, facing);
                    if ahead / u >= OUT || behind / u >= OUT {
                        wrong.push(format!(
                            "{name} devant {front} près {near}, sens {facing} : {:.1} u devant, {:.1} u derrière",
                            ahead / u,
                            behind / u
                        ));
                    }
                }
            }
        }
        assert!(wrong.is_empty(), "{}", wrong.join("\n"));
    }
}
