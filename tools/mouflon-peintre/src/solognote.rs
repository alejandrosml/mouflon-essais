//! Brebis Solognote adulte, 65 cm au garrot, en toison de fin d'été.
//!
//! Le canon est celui de `docs/races/solognote.md`, contrôlé sur les photos
//! P4/P5 (profil et broutage) et P14/P16/P17 (encolure, face, oreilles).
//! La toile a sept pixels par centimètre : sol à 830, garrot de peau à 375.
//! La laine courte se lit en petites crêtes irrégulières et creux pigmentés,
//! jamais en grosses boules ; le poil roux reste nu au ventre et à la gorge.

use super::*;

const SOL: f32 = 830.0;
const EYE: (f32, f32) = (937.0, 337.0);
/// Hanche ou épaule, carpe ou jarret, puis abscisse du sabot.
type Leg = ((f32, f32), (f32, f32), f32);

const LEGS: [Leg; 4] = [
    ((315.0, 585.0), (290.0, 675.0), 309.0),
    ((742.0, 585.0), (748.0, 716.0), 750.0),
    ((350.0, 585.0), (322.0, 675.0), 345.0),
    ((775.0, 585.0), (781.0, 716.0), 783.0),
];

fn palette() -> Palette {
    let wool = Wash {
        light: hex(0xF0EADF),
        base: hex(0xD4C9BA),
        shadow: hex(0xA69A94),
        toward_shadow: (0.25, 1.0),
        pigment: hex(0x87786A),
        edge: 0.44,
        edge_width: 5,
        mottling: 0.07,
        grain: 0.075,
        pencil: Some((hex(0x665449), 0.42, 2)),
        seed: 1103,
    };
    let face = Wash {
        light: hex(0xA8644A),
        base: hex(0x7A3E2C),
        shadow: hex(0x4A2218),
        toward_shadow: (0.15, 1.0),
        pigment: hex(0x4A2218),
        edge: 0.48,
        edge_width: 4,
        mottling: 0.13,
        grain: 0.12,
        pencil: Some((hex(0x3A1E16), 0.65, 2)),
        seed: 1123,
    };
    let leg = Wash {
        light: hex(0x946A55),
        base: hex(0x6E4634),
        shadow: hex(0x3E2618),
        toward_shadow: (1.0, 0.2),
        pigment: hex(0x33200F),
        edge: 0.5,
        edge_width: 3,
        mottling: 0.07,
        grain: 0.08,
        pencil: Some((hex(0x2A190E), 0.65, 2)),
        seed: 1131,
    };
    let far_leg = Wash {
        light: hex(0x7A5543),
        base: hex(0x5A3828),
        shadow: hex(0x33200F),
        seed: 1137,
        ..leg
    };
    Palette {
        wool,
        face,
        leg,
        far_leg,
    }
}

/// Même champ de pigment sur le corps, l'encolure et les mèches : leurs
/// bords perdus se fondent au repos. Les cellules voisines sont déformées
/// par du bruit fin ; leur double distance dessine les creux de la laine.
fn fleece(mask: &[f32], p: &Palette) -> Layer {
    let mut layer = Layer::new();
    p.wool.paint(mask, &mut layer);
    let noise = Noise { seed: 1227 };
    for y in 0..H {
        for x in 0..W {
            let i = y * W + x;
            if mask[i] <= 0.0 {
                continue;
            }
            let (fx, fy) = (x as f32, y as f32);
            let russet =
                smoothstep(585.0, 848.0, fx) * (0.98 - 0.28 * smoothstep(450.0, 620.0, fy));
            let warm = mix(hex(0x7A5233), hex(0x4A2C14), smoothstep(370.0, 610.0, fy));
            layer.over(i, warm, russet * 0.96 * mask[i]);
            // La trame est déformée à plusieurs échelles, puis interrompue :
            // des fentes ondulantes, jamais le contour entier d'un polygone.
            let u = (fx + 6.2 * noise.fbm(fx, fy, 9.0, 3)) / 9.5;
            let v = (fy + 7.7 * noise.fbm(fx + 30.0, fy, 11.0, 3)) / 16.0;
            let (cx, cy) = (u.floor() as i32, v.floor() as i32);
            let (mut d1, mut d2) = (f32::MAX, f32::MAX);
            for yy in cy - 1..=cy + 1 {
                for xx in cx - 1..=cx + 1 {
                    let px = xx as f32 + 0.5 + 0.37 * noise.hash(xx, yy);
                    let py = yy as f32 + 0.5 + 0.37 * noise.hash(xx + 311, yy + 427);
                    let d = (u - px).powi(2) + (v - py).powi(2);
                    if d < d1 {
                        d2 = d1;
                        d1 = d;
                    } else if d < d2 {
                        d2 = d;
                    }
                }
            }
            let broken = smoothstep(-0.05, 0.36, noise.fbm(fx + 710.0, fy, 25.0, 3));
            let crease = (1.0 - smoothstep(0.015, 0.125, d2 - d1)) * broken;
            let depth =
                0.23 + 0.36 * smoothstep(370.0, 630.0, fy) + 0.16 * noise.fbm(fx, fy, 68.0, 2);
            layer.multiply(i, hex(0x70553D), crease * depth * 0.12 * mask[i]);
            // Crêtes claires plus larges, plages tranquilles, et fibres
            // très fines au pinceau sec : le papier demeure visible.
            let ridge = (1.0 - smoothstep(0.05, 0.65, d1))
                * smoothstep(-0.25, 0.5, noise.fbm(fx + 195.0, fy, 43.0, 2));
            layer.over(
                i,
                hex(0xF5EDDF),
                ridge * 0.16 * (1.0 - russet * 0.74) * mask[i],
            );
            let fibre = noise.value((fx + 2.0 * noise.value(fx / 8.0, fy / 8.0)) / 1.4, fy / 5.0);
            layer.multiply(i, hex(0x9C856B), fibre.max(0.0) * 0.11 * mask[i]);
        }
    }
    // Des mèches de 4–6 cm : crêtes sinueuses regroupées en rangées
    // irrégulières, longues et serrées sur l'épaule, plus ouvertes sur le
    // flanc. Les ombres s'interrompent, leurs crêtes se fondent dans le lavis.
    let mut grooves = PathBuilder::new();
    let mut lights = PathBuilder::new();
    for gy in 0..13 {
        for gx in 0..61 {
            if noise.hash(gx + 97, gy + 251) < -0.24 {
                continue;
            }
            let x = 153.0 + gx as f32 * 12.6 + noise.hash(gx, gy) * 3.8;
            let y = 276.0
                + gy as f32 * 33.0
                + 7.0 * (gx as f32 * 0.31 + gy as f32).sin()
                + noise.hash(gx + 97, gy) * 5.5;
            let length = 24.0 + (noise.hash(gx + 301, gy) + 1.0) * 8.0;
            let bend = 1.5 + noise.hash(gx, gy + 193) * 3.5;
            let lean = 2.5 * noise.value(x / 73.0, y / 49.0);
            grooves.move_to(x, y);
            grooves.cubic_to(
                x + bend,
                y + length * 0.22,
                x - bend,
                y + length * 0.4,
                x + lean,
                y + length * 0.56,
            );
            grooves.cubic_to(
                x + bend + lean,
                y + length * 0.75,
                x - bend * 0.4 + lean,
                y + length * 0.9,
                x + lean * 1.4,
                y + length,
            );
            lights.move_to(x - 2.8, y - 1.5);
            lights.cubic_to(
                x + bend - 2.8,
                y + length * 0.22 - 1.5,
                x - bend - 2.8,
                y + length * 0.4 - 1.5,
                x + lean - 2.8,
                y + length * 0.56 - 1.5,
            );
            lights.cubic_to(
                x + bend + lean - 2.8,
                y + length * 0.75 - 1.5,
                x - bend * 0.4 + lean - 2.8,
                y + length * 0.9 - 1.5,
                x + lean * 1.4 - 2.8,
                y + length - 1.5,
            );
        }
    }
    let grooves = stroke_mask(&grooves.finish().expect("rides de laine"), 2.2);
    let light_paths = lights.finish().expect("crêtes de laine");
    let lights = stroke_mask(&light_paths, 3.2);
    let thin_lights = stroke_mask(&light_paths, 1.5);
    for (i, m) in mask.iter().enumerate() {
        if *m <= 0.0 {
            continue;
        }
        let x = (i % W) as f32;
        let y = (i / W) as f32;
        let pressure = 0.24 + 0.24 * smoothstep(390.0, 620.0, y) + 0.12 * noise.fbm(x, y, 52.0, 2);
        layer.multiply(
            i,
            hex(0x89715D),
            grooves[i] * pressure * smoothstep(-0.42, 0.1, noise.value(x / 7.0, y / 9.0)) * m,
        );
        let brown = smoothstep(600.0, 860.0, x);
        // Sur le cou roux, les crêtes restent rousses et mates. Le pinceau
        // sec en perd des segments ; leur épaisseur varie entre 1,5 et 3 px.
        let thin = brown * (0.62 + 0.28 * noise.value(x / 11.0, y / 13.0));
        let light = lights[i] * (1.0 - thin) + thin_lights[i] * thin;
        let broken = smoothstep(-0.36, 0.25, noise.value(x / 5.0, y / 7.0));
        layer.over(
            i,
            mix(hex(0xF4EBDA), hex(0xA88A6C), brown),
            light * pressure * (0.52 - brown * 0.19) * broken * m,
        );
    }
    layer
}

/// Le dos est droit, la croupe arrondie ; les petites pointes du bas sont
/// des mèches de 4–6 cm, serrées et non une frange de longues cordes.
fn body_shape() -> SkPath {
    let mut points = vec![
        (235.0, 485.0),
        (224.0, 444.0),
        (239.0, 404.0),
        (276.0, 373.0),
        (325.0, 362.0),
    ];
    let noise = Noise { seed: 1419 };
    for k in 0..=28 {
        let x = 338.0 + k as f32 * 15.0;
        let y = 363.0 + 2.5 * noise.hash(k, 0) + 1.3 * (k as f32 * 1.4).sin();
        points.push((x, y));
    }
    points.extend([
        (780.0, 380.0),
        (805.0, 414.0),
        (817.0, 469.0),
        (805.0, 527.0),
        (783.0, 573.0),
        (748.0, 601.0),
    ]);
    // Les trois larges festons, dont on détache le bout, n'entravent pas
    // la bande de ventre nue. Les petites pointes restent dans leur lobe.
    for k in 0..=37 {
        let x = 732.0 - k as f32 * 11.0;
        let y = 599.0
            + 11.0 * (k as f32 * 1.9).sin().abs()
            + 9.0 * noise.value(k as f32 * 0.38, 1.0)
            + 4.0 * noise.hash(k, 1);
        points.push((x, y));
    }
    points.extend([
        (308.0, 615.0),
        (274.0, 602.0),
        (248.0, 565.0),
        (236.0, 522.0),
    ]);
    smooth_closed(&points)
}

fn belly(p: &Palette) -> Layer {
    let mask = mask_of(&smooth_closed(&[
        (300.0, 555.0),
        (470.0, 565.0),
        (680.0, 562.0),
        (778.0, 565.0),
        (755.0, 622.0),
        (670.0, 645.0),
        (511.0, 657.0),
        (411.0, 643.0),
        (335.0, 623.0),
    ]));
    let mut layer = Layer::new();
    let wash = Wash {
        light: hex(0x997056),
        base: hex(0x765039),
        shadow: hex(0x60412D),
        toward_shadow: (0.0, 1.0),
        seed: 1423,
        pencil: Some((hex(0x4D3224), 0.34, 2)),
        ..p.leg
    };
    wash.paint(&mask, &mut layer);
    layer
}

fn neck(p: &Palette) -> Layer {
    let mut layer = Layer::new();
    // Peau de la gorge, avec une vraie largeur cachée sous la tête et le
    // corps : les rotations n'ouvrent pas un triangle de fond.
    let skin = mask_of(&smooth_closed(&[
        (700.0, 491.0),
        (781.0, 387.0),
        (856.0, 293.0),
        (905.0, 299.0),
        (925.0, 356.0),
        (886.0, 423.0),
        (836.0, 500.0),
        (793.0, 568.0),
        (717.0, 572.0),
    ]));
    let wash = Wash {
        toward_shadow: (0.55, 1.0),
        seed: 1447,
        ..p.face
    };
    wash.paint(&skin, &mut layer);
    let wool = mask_of(&smooth_closed(&[
        (669.0, 469.0),
        (727.0, 398.0),
        (773.0, 350.0),
        (827.0, 303.0),
        (859.0, 281.0),
        (881.0, 292.0),
        (887.0, 326.0),
        (872.0, 363.0),
        (854.0, 403.0),
        (826.0, 439.0),
        (809.0, 476.0),
        (788.0, 508.0),
        (756.0, 548.0),
        (693.0, 559.0),
    ]));
    layer.stack(&fleece(&wool, p));
    layer
}

fn leg(i: usize, p: &Palette) -> (Layer, Layer) {
    let (hip, joint, foot) = LEGS[i];
    let front = i % 2 == 1;
    let wash = if i < 2 { &p.far_leg } else { &p.leg };
    let mut layer = Layer::new();
    let (hx, hy) = hip;
    let (jx, jy) = joint;
    let points = [
        (hx - 23.0, hy - 40.0),
        (hx + 23.0, hy - 40.0),
        (hx + 20.0, hy + 24.0),
        (jx + 12.5, jy - 9.0),
        (jx + 12.0, jy + 13.0),
        (foot + 8.0, SOL - 35.0),
        (foot + 13.0, SOL - 18.0),
        (foot - 10.0, SOL - 18.0),
        (foot - 9.0, SOL - 38.0),
        (jx - 11.0, jy + 13.0),
        (jx - 14.0, jy - 4.0),
        (hx - 17.0, hy + 27.0),
    ];
    let mask = mask_of(&smooth_closed(&points));
    wash.paint(&mask, &mut layer);
    // Le reflet fin du tendon est roux atténué, jamais une balzane blanche.
    let tendon = stroke_mask(
        &smooth_open(&[
            (jx + 3.0, jy + 15.0),
            (foot + 2.0, SOL - 58.0),
            (foot + 1.0, SOL - 36.0),
        ]),
        2.2,
    );
    dab(
        &mut layer,
        &blur(&tendon, 1),
        Some(&mask),
        hex(0xAB8065),
        0.38,
    );
    let hoof = mask_of(&smooth_closed(&[
        (foot - 13.0, SOL),
        (foot - 13.0, SOL - 17.0),
        (foot - 5.0, SOL - 24.0),
        (foot + 8.0, SOL - 23.0),
        (foot + 17.0, SOL - 12.0),
        (foot + 18.0, SOL),
    ]));
    let hoof_wash = Wash {
        light: hex(0x787168),
        base: hex(0x48433D),
        shadow: hex(0x272421),
        pigment: hex(0x24201C),
        toward_shadow: (1.0, 0.2),
        edge: 0.3,
        edge_width: 2,
        mottling: 0.06,
        grain: 0.08,
        pencil: Some((hex(0x25201A), 0.55, 1)),
        seed: 1501 + i as u32,
    };
    hoof_wash.paint(&hoof, &mut layer);
    let cleft = stroke_mask(
        &smooth_open(&[(foot + 5.0, SOL - 1.0), (foot + 3.0, SOL - 11.0)]),
        1.8,
    );
    dab(&mut layer, &cleft, Some(&hoof), hex(0x211D18), 0.9);
    let dewclaw = mask_of(&ellipse(foot - 10.0, SOL - 34.0, 3.0, 4.5));
    dab(&mut layer, &dewclaw, None, hex(0x4F3D2F), 0.8);
    split_joint(&layer, joint, if front { 12.5 } else { 14.0 })
}

const HEAD: [(f32, f32); 16] = [
    (866.0, 292.0),
    (885.0, 275.0),
    (915.0, 280.0),
    (946.0, 300.0),
    (973.0, 324.0),
    (991.0, 349.0),
    (1013.0, 376.0),
    (1038.0, 395.0),
    (1041.0, 416.0),
    (1025.0, 435.0),
    (1008.0, 431.0),
    (982.0, 423.0),
    (957.0, 413.0),
    (934.0, 415.0),
    (911.0, 399.0),
    (890.0, 373.0),
];
const JAW_CUT: [(f32, f32); 8] = [
    (970.0, 410.0),
    (995.0, 421.0),
    (1020.0, 428.0),
    (1059.0, 418.0),
    (1075.0, 480.0),
    (1000.0, 475.0),
    (953.0, 447.0),
    (954.0, 422.0),
];
const LIP: [(f32, f32); 7] = [
    (1018.0, 404.0),
    (1035.0, 399.0),
    (1042.0, 417.0),
    (1033.0, 428.0),
    (1022.0, 431.0),
    (1011.0, 423.0),
    (1010.0, 413.0),
];

/// Légère inclinaison naturelle de repos, nez bas. Le port des oreilles
/// et la pupille restent horizontaux ; tous leurs centres suivent la tête.
fn head_point((x, y): (f32, f32)) -> (f32, f32) {
    let (sin, cos) = 4.0_f32.to_radians().sin_cos();
    let (dx, dy) = (x - 878.0, y - 305.0);
    (878.0 + dx * cos - dy * sin, 305.0 + dx * sin + dy * cos)
}
fn head_mask(path: &SkPath) -> Vec<f32> {
    let path = path
        .clone()
        .transform(Transform::from_rotate_at(4.0, 878.0, 305.0))
        .expect("tête au repos");
    mask_of(&path)
}
fn head_stroke(path: &SkPath, width: f32) -> Vec<f32> {
    let path = path
        .clone()
        .transform(Transform::from_rotate_at(4.0, 878.0, 305.0))
        .expect("trait de la tête");
    stroke_mask(&path, width)
}

fn whole_head(p: &Palette) -> Layer {
    let mut layer = Layer::new();
    let mask = head_mask(&smooth_closed(&HEAD));
    p.face.paint(&mask, &mut layer);
    // Modelé en glacis, sans museau blanc ni joues roses.
    let cheek = blur(&head_mask(&ellipse(932.0, 386.0, 29.0, 27.0)), 12);
    dab(&mut layer, &cheek, Some(&mask), hex(0x54271F), 0.32);
    let plane = blur(&head_mask(&ellipse(973.0, 347.0, 34.0, 12.0)), 12);
    dab(&mut layer, &plane, Some(&mask), hex(0xB87858), 0.25);
    let nose = head_mask(&smooth_closed(&[
        (1034.0, 406.0),
        (1040.0, 412.0),
        (1041.0, 419.0),
        (1036.0, 424.0),
        (1030.0, 417.0),
    ]));
    dab(
        &mut layer,
        &blur(&nose, 1),
        Some(&mask),
        hex(0x594036),
        0.78,
    );
    let nostril = head_stroke(
        &smooth_open(&[(1034.0, 409.0), (1029.0, 413.0), (1031.0, 417.0)]),
        3.5,
    );
    dab(&mut layer, &nostril, Some(&mask), hex(0x2C211E), 0.96);
    let wet = head_mask(&ellipse(1037.0, 411.0, 1.2, 1.8));
    dab(&mut layer, &wet, Some(&mask), hex(0xAB9990), 0.68);
    let mouth = head_stroke(
        &smooth_open(&[(1035.0, 426.0), (1025.0, 429.0), (1010.0, 426.0)]),
        1.8,
    );
    dab(&mut layer, &mouth, Some(&mask), hex(0x39231F), 0.85);
    // Poils courts, peints au pinceau sec : tonalité unie de la race,
    // mais petites stries qui suivent le chanfrein et le plan de la joue.
    let n = Noise { seed: 1553 };
    for k in 0..72 {
        let x = 896.0 + (n.hash(k, 0) + 1.0) * 70.0;
        let y = 297.0 + (n.hash(k, 1) + 1.0) * 70.0;
        let hair = head_stroke(&smooth_open(&[(x, y), (x + 3.5, y + 4.2)]), 0.8);
        dab(&mut layer, &hair, Some(&mask), hex(0xD29B76), 0.16);
    }
    layer
}

fn head_and_jaw(p: &Palette) -> (Layer, Layer) {
    let whole = whole_head(p);
    let head_coverage = head_mask(&smooth_closed(&HEAD));
    let jaw_mask = head_mask(&polygon(&JAW_CUT));
    let away: Vec<f32> = inset(&jaw_mask, 2).iter().map(|m| 1.0 - m).collect();
    let head = whole.masked(&away);
    let mut jaw = Layer::new();
    // Réserve de joue et cavité largement remontées dans la tête fermée :
    // en s'ouvrant, la mâchoire découvre du pigment sombre, jamais du fond.
    let cheek = head_mask(&polygon(&[
        (951.0, 395.0),
        (985.0, 402.0),
        (1000.0, 426.0),
        (975.0, 452.0),
        (950.0, 430.0),
    ]));
    jaw.stack(&whole.masked(&cheek));
    let mouth = both(
        &inset(&head_coverage, 2),
        &head_mask(&smooth_closed(&[
            (966.0, 391.0),
            (1002.0, 375.0),
            (1031.0, 388.0),
            (1047.0, 410.0),
            (1042.0, 432.0),
            (1013.0, 443.0),
            (979.0, 433.0),
        ])),
    );
    dab(&mut jaw, &mouth, None, hex(0x462725), 1.0);
    let tongue = blur(&head_mask(&ellipse(1016.0, 426.0, 17.0, 5.0)), 1);
    dab(&mut jaw, &tongue, Some(&mouth), hex(0xAC6B6B), 0.9);
    jaw.stack(&whole.masked(&jaw_mask));
    (head, jaw)
}

fn ear(p: &Palette, far: bool) -> Layer {
    let mut layer = Layer::new();
    let (base, tip, width): ((f32, f32), (f32, f32), f32) = if far {
        ((881.0, 301.0), (828.0, 303.0), 26.0)
    } else {
        ((887.0, 316.0), (807.0, 315.0), 35.0)
    };
    let offset = (tip.0 - base.0, tip.1 - base.1);
    let base = head_point(base);
    let tip = (base.0 + offset.0, base.1 + offset.1);
    let dx = tip.0 - base.0;
    let dy = tip.1 - base.1;
    let len: f32 = (dx * dx + dy * dy).sqrt();
    let nx = -dy / len;
    let ny = dx / len;
    let at = |u: f32, v: f32| {
        (
            base.0 + dx * u + nx * width * v,
            base.1 + dy * u + ny * width * v,
        )
    };
    let shape = smooth_closed(&[
        at(0.0, 0.0),
        at(0.27, -0.42),
        at(0.7, -0.48),
        at(0.97, -0.22),
        at(1.0, 0.0),
        at(0.9, 0.26),
        at(0.63, 0.43),
        at(0.25, 0.31),
    ]);
    let mask = mask_of(&shape);
    let wash = Wash {
        seed: if far { 1583 } else { 1589 },
        ..p.face
    };
    wash.paint(&mask, &mut layer);
    let inner = mask_of(&leaf(
        (base.0 + (tip.0 - base.0) * 0.13, base.1 + 1.0),
        (base.0 + (tip.0 - base.0) * 0.88, tip.1 + 1.0),
        width * 0.52,
        -1.0,
    ));
    dab(
        &mut layer,
        &blur(&inner, 2),
        Some(&mask),
        hex(0xC18760),
        if far { 0.32 } else { 0.54 },
    );
    let fold = stroke_mask(
        &smooth_open(&[
            (base.0, base.1 + 1.0),
            ((base.0 + tip.0) * 0.5, (base.1 + tip.1) * 0.5 + 4.0),
            (tip.0 + if far { -6.0 } else { 6.0 }, tip.1 + 1.0),
        ]),
        1.4,
    );
    dab(&mut layer, &fold, Some(&mask), hex(0x633425), 0.6);
    layer
}

fn eye() -> Layer {
    let mut layer = Layer::new();
    let (x, y) = head_point(EYE);
    let dark = mask_of(&smooth_closed(&[
        (x - 9.5, y),
        (x - 3.2, y - 6.3),
        (x + 4.7, y - 5.5),
        (x + 9.5, y),
        (x + 3.2, y + 5.5),
        (x - 4.7, y + 4.7),
    ]));
    dab(&mut layer, &dark, None, hex(0x3A1E1A), 1.0);
    let iris = mask_of(&ellipse(x, y, 7.3, 4.2));
    dab(&mut layer, &iris, Some(&dark), hex(0xB8B9B0), 1.0);
    let lower = blur(&mask_of(&ellipse(x, y + 1.5, 5.5, 2.0)), 1);
    dab(&mut layer, &lower, Some(&iris), hex(0xE0DCC2), 0.65);
    layer
}
fn pupil() -> Layer {
    let mut layer = Layer::new();
    let (x, y) = head_point(EYE);
    let slit = stroke_mask(&smooth_open(&[(x - 4.9, y), (x + 4.9, y)]), 2.6);
    dab(&mut layer, &slit, None, hex(0x141110), 1.0);
    layer
}
fn glint() -> Layer {
    let mut layer = Layer::new();
    let (x, y) = head_point(EYE);
    let m = mask_of(&ellipse(x + 2.4, y - 2.0, 1.4, 1.25));
    dab(&mut layer, &m, None, hex(0xFFFFFF), 0.92);
    layer
}
fn closed_eye() -> Layer {
    let mut layer = Layer::new();
    let (x, y) = head_point(EYE);
    let m = stroke_mask(
        &smooth_open(&[(x - 8.8, y), (x - 1.6, y + 2.8), (x + 8.0, y - 0.4)]),
        2.0,
    );
    dab(&mut layer, &m, None, hex(0x3A1E1A), 0.97);
    layer
}

/// Une queue entière de 38 cm, fine et laineuse, descendant sous le
/// jarret ; elle est coupée en trois os en chaîne aux attaches arrondies.
fn tail(p: &Palette) -> [Layer; 3] {
    let shape = smooth_closed(&[
        (246.0, 426.0),
        (236.0, 478.0),
        (218.0, 537.0),
        (202.0, 598.0),
        (198.0, 656.0),
        (207.0, 700.0),
        (199.0, 707.0),
        (181.0, 684.0),
        (175.0, 638.0),
        (180.0, 581.0),
        (191.0, 519.0),
        (207.0, 468.0),
        (219.0, 429.0),
    ]);
    let mask = mask_of(&shape);
    let mut layer = fleece(&mask, p);
    for (i, m) in mask.iter().enumerate() {
        if *m > 0.0 {
            layer.multiply(i, hex(0xA08D74), 0.58 * m);
        }
    }
    let (top, rest) = split_joint(&layer, (207.0, 530.0), 22.0);
    let (middle, bottom) = split_joint(&rest, (187.0, 621.0), 18.0);
    [top, middle, bottom]
}

/// Exporte les morceaux seuls par défaut. Une planche de travail n'est
/// écrite que sur demande explicite (`--apercu`), au chemin choisi.
pub(super) fn paint(out: &Path, preview_path: Option<&Path>) {
    std::fs::create_dir_all(out).expect("dossier des morceaux");
    let p = palette();
    let mut parts: Vec<(&str, Layer)> = Vec::new();
    for (i, name) in [
        "patte-arriere-loin",
        "patte-avant-loin",
        "patte-arriere-pres",
        "patte-avant-pres",
    ]
    .into_iter()
    .enumerate()
    {
        let (upper, lower) = leg(i, &p);
        let lower_name = match i {
            0 => "patte-arriere-loin-bas",
            1 => "patte-avant-loin-bas",
            2 => "patte-arriere-pres-bas",
            _ => "patte-avant-pres-bas",
        };
        parts.push((lower_name, lower));
        parts.push((name, upper));
    }
    let [tail_top, tail_middle, tail_tip] = tail(&p);
    // Queue derrière le corps et les pattes du côté visible.
    parts.insert(4, ("queue", tail_top));
    parts.insert(5, ("queue-milieu", tail_middle));
    parts.insert(6, ("queue-bout", tail_tip));
    parts.push(("ventre", belly(&p)));
    parts.push(("encolure", neck(&p)));
    let mut body = fleece(&mask_of(&body_shape()), &p);
    for (name, x0, x1) in [
        ("meche-1", 635.0, 720.0),
        ("meche-2", 535.0, 620.0),
        ("meche-3", 432.0, 520.0),
    ] {
        let a = valley(&body, x0);
        let b = valley(&body, x1);
        println!(
            "{name} : [{:.1}, {:.1}]",
            (a.0 + b.0) * 0.5,
            (a.1 + b.1) * 0.5
        );
        let (lock, remainder) = split_lock(&body, a, b, 7.0);
        body = remainder;
        parts.push((name, lock));
    }
    parts.push(("corps", body));
    parts.push(("oreille-loin", ear(&p, true)));
    let (head, jaw) = head_and_jaw(&p);
    let lip_mask = head_mask(&polygon(&LIP));
    let lip = head.masked(&lip_mask);
    let mut gum = Layer::new();
    let gum_mask = both(
        &inset(&lip_mask, 3),
        &inset(&head_mask(&smooth_closed(&HEAD)), 4),
    );
    dab(&mut gum, &gum_mask, None, hex(0xB77576), 1.0);
    parts.extend([
        ("machoire", jaw),
        ("tete", head),
        ("gencive", gum),
        ("levre", lip),
        ("oreille-pres", ear(&p, false)),
        ("oeil", eye()),
        ("pupille", pupil()),
        ("reflet", glint()),
        ("oeil-ferme", closed_eye()),
    ]);
    let mut preview = preview_path.map(|_| Layer::new());
    for (name, layer) in &parts {
        layer.save(&out.join(format!("{name}.png")));
        if let Some(preview) = &mut preview
            && !matches!(*name, "oeil-ferme" | "gencive")
        {
            preview.stack(layer);
        }
        println!("{name}.png");
    }
    if let (Some(preview), Some(path)) = (preview, preview_path) {
        if let Some(parent) = path.parent().filter(|p| !p.as_os_str().is_empty()) {
            std::fs::create_dir_all(parent).expect("dossier de relecture");
        }
        preview.save(path);
    }
}
