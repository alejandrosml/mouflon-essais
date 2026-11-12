//! Peint les morceaux de la Solognote, l'animal à squelette du dépôt
//! (`pets/solognote`) : une brebis à l'aquarelle, sans pinceau ni logiciel
//! de dessin.
//!
//! Chaque morceau est un lavis (une forme remplie de pigment) : la lumière
//! vient d'en haut à gauche, le pigment s'accumule sur les bords en séchant
//! (ils sont plus sombres), sa densité varie doucement (le lavis n'est jamais
//! uni) et le grain du papier le pique ; un trait de crayon léger cerne la
//! forme. Les morceaux sont exportés à la taille de toute la toile, comme
//! le ferait Blender (Grease Pencil) ou un logiciel de dessin : Mouflon les
//! recadre et les réduit lui-même.
//!
//! `cargo run --release -p mouflon-peintre -- pets/solognote/morceaux`
//! repeint les morceaux ; `--apercu <image.png>` ajoute un aperçu de
//! l'ensemble, à écrire hors de `pets/` (il n'est pas livré). Le dessin et
//! la palette de la race sont dans `solognote.rs`.

mod solognote;

use std::path::{Path, PathBuf};

use tiny_skia::{
    FillRule, LineCap, LineJoin, Paint, Path as SkPath, PathBuilder, Pixmap, Stroke, Transform,
};

/// Taille de la toile (pixels).
const W: usize = 1200;
const H: usize = 900;

type Rgb = [f32; 3];

fn hex(v: u32) -> Rgb {
    [
        ((v >> 16) & 0xff) as f32 / 255.0,
        ((v >> 8) & 0xff) as f32 / 255.0,
        (v & 0xff) as f32 / 255.0,
    ]
}

fn mix(a: Rgb, b: Rgb, t: f32) -> Rgb {
    [
        a[0] + (b[0] - a[0]) * t,
        a[1] + (b[1] - a[1]) * t,
        a[2] + (b[2] - a[2]) * t,
    ]
}

fn smoothstep(a: f32, b: f32, x: f32) -> f32 {
    let t = ((x - a) / (b - a)).clamp(0.0, 1.0);
    t * t * (3.0 - 2.0 * t)
}

// ------------------------------------------------------------------ bruit

/// Bruit de valeurs lissé, de -1 à 1, et sa somme sur plusieurs octaves.
#[derive(Clone, Copy, Debug)]
struct Noise {
    seed: u32,
}

impl Noise {
    fn hash(&self, x: i32, y: i32) -> f32 {
        let mut h = (x as u32)
            .wrapping_mul(0x8da6_b343)
            .wrapping_add((y as u32).wrapping_mul(0xd816_3841))
            .wrapping_add(self.seed.wrapping_mul(0xcb1a_b31f));
        h ^= h >> 13;
        h = h.wrapping_mul(0x5bd1_e995);
        h ^= h >> 15;
        (h & 0xffff) as f32 / 32767.5 - 1.0
    }

    fn value(&self, x: f32, y: f32) -> f32 {
        let (x0, y0) = (x.floor(), y.floor());
        let (fx, fy) = (x - x0, y - y0);
        let (sx, sy) = (fx * fx * (3.0 - 2.0 * fx), fy * fy * (3.0 - 2.0 * fy));
        let (ix, iy) = (x0 as i32, y0 as i32);
        let a = self.hash(ix, iy);
        let b = self.hash(ix + 1, iy);
        let c = self.hash(ix, iy + 1);
        let d = self.hash(ix + 1, iy + 1);
        let top = a + (b - a) * sx;
        let bottom = c + (d - c) * sx;
        top + (bottom - top) * sy
    }

    /// Plusieurs octaves, la première de `scale` pixels.
    fn fbm(&self, x: f32, y: f32, scale: f32, octaves: u32) -> f32 {
        let (mut sum, mut amp, mut norm, mut f) = (0.0, 1.0, 0.0, 1.0 / scale);
        for o in 0..octaves {
            let shifted = Noise {
                seed: self.seed.wrapping_add(o * 1013),
            };
            sum += shifted.value(x * f, y * f) * amp;
            norm += amp;
            amp *= 0.5;
            f *= 2.0;
        }
        sum / norm
    }
}

// ----------------------------------------------------------------- masques

/// Ce que couvre un chemin, de 0 à 1 par pixel (antialiasé).
fn mask_of(path: &SkPath) -> Vec<f32> {
    let mut pixmap = Pixmap::new(W as u32, H as u32).expect("toile");
    let mut paint = Paint::default();
    paint.set_color_rgba8(255, 255, 255, 255);
    paint.anti_alias = true;
    pixmap.fill_path(path, &paint, FillRule::Winding, Transform::identity(), None);
    pixmap
        .data()
        .as_chunks::<4>()
        .0
        .iter()
        .map(|p| f32::from(p[3]) / 255.0)
        .collect()
}

/// Ce que couvre un trait le long d'un chemin.
fn stroke_mask(path: &SkPath, width: f32) -> Vec<f32> {
    let mut pixmap = Pixmap::new(W as u32, H as u32).expect("toile");
    let mut paint = Paint::default();
    paint.set_color_rgba8(255, 255, 255, 255);
    paint.anti_alias = true;
    let stroke = Stroke {
        width,
        line_cap: LineCap::Round,
        line_join: LineJoin::Round,
        ..Stroke::default()
    };
    pixmap.stroke_path(path, &paint, &stroke, Transform::identity(), None);
    pixmap
        .data()
        .as_chunks::<4>()
        .0
        .iter()
        .map(|p| f32::from(p[3]) / 255.0)
        .collect()
}

/// Flou : trois passes de moyenne glissante, horizontales puis verticales
/// (presque une gaussienne).
fn blur(src: &[f32], radius: usize) -> Vec<f32> {
    if radius == 0 {
        return src.to_vec();
    }
    let mut a = src.to_vec();
    let mut b = vec![0.0; src.len()];
    let window = (2 * radius + 1) as f32;
    for _ in 0..3 {
        for y in 0..H {
            let row = &a[y * W..(y + 1) * W];
            let mut acc: f32 =
                row[..=radius.min(W - 1)].iter().sum::<f32>() + row[0] * radius as f32;
            for x in 0..W {
                b[y * W + x] = acc / window;
                let add = row[(x + radius + 1).min(W - 1)];
                let sub = row[x.saturating_sub(radius)];
                acc += add - sub;
            }
        }
        for x in 0..W {
            let mut acc: f32 =
                (0..=radius.min(H - 1)).map(|y| b[y * W + x]).sum::<f32>() + b[x] * radius as f32;
            for y in 0..H {
                a[y * W + x] = acc / window;
                let add = b[(y + radius + 1).min(H - 1) * W + x];
                let sub = b[y.saturating_sub(radius) * W + x];
                acc += add - sub;
            }
        }
    }
    a
}

// ------------------------------------------------------------------ calques

/// Un morceau en train d'être peint : couleurs prémultipliées.
struct Layer {
    px: Vec<[f32; 4]>,
}

impl Layer {
    fn new() -> Self {
        Self {
            px: vec![[0.0; 4]; W * H],
        }
    }

    /// Pose `color` avec l'opacité `alpha` sur le pixel `i`.
    fn over(&mut self, i: usize, color: Rgb, alpha: f32) {
        let a = alpha.clamp(0.0, 1.0);
        if a <= 0.0 {
            return;
        }
        let p = &mut self.px[i];
        let keep = 1.0 - a;
        p[0] = color[0] * a + p[0] * keep;
        p[1] = color[1] * a + p[1] * keep;
        p[2] = color[2] * a + p[2] * keep;
        p[3] = a + p[3] * keep;
    }

    /// Assombrit (multiplie) ce qui est déjà peint, sans rien ajouter
    /// ailleurs : une ombre posée par-dessus.
    fn multiply(&mut self, i: usize, color: Rgb, amount: f32) {
        let k = amount.clamp(0.0, 1.0);
        let p = &mut self.px[i];
        for c in 0..3 {
            p[c] *= 1.0 - k + k * color[c];
        }
    }

    /// Ce calque, seulement là où est `mask` (son opacité multipliée).
    fn masked(&self, mask: &[f32]) -> Layer {
        Layer {
            px: self
                .px
                .iter()
                .zip(mask)
                .map(|(p, m)| [p[0] * m, p[1] * m, p[2] * m, p[3] * m])
                .collect(),
        }
    }

    /// Ajoute un autre calque par-dessus.
    fn stack(&mut self, other: &Layer) {
        for (p, q) in self.px.iter_mut().zip(&other.px) {
            let keep = 1.0 - q[3];
            for c in 0..4 {
                p[c] = q[c] + p[c] * keep;
            }
        }
    }

    fn save(&self, path: &Path) {
        let mut rgba = Vec::with_capacity(W * H * 4);
        for p in &self.px {
            let a = p[3].clamp(0.0, 1.0);
            if a <= 1.0 / 512.0 {
                rgba.extend_from_slice(&[0, 0, 0, 0]);
                continue;
            }
            let c = |v: f32| ((v / a).clamp(0.0, 1.0) * 255.0).round() as u8;
            rgba.extend_from_slice(&[c(p[0]), c(p[1]), c(p[2]), (a * 255.0).round() as u8]);
        }
        let file = std::fs::File::create(path).expect("fichier");
        let mut encoder = png::Encoder::new(std::io::BufWriter::new(file), W as u32, H as u32);
        encoder.set_color(png::ColorType::Rgba);
        encoder.set_depth(png::BitDepth::Eight);
        encoder.set_compression(png::Compression::High);
        let mut writer = encoder.write_header().expect("en-tête PNG");
        writer.write_image_data(&rgba).expect("PNG");
    }
}

// ------------------------------------------------------------ articulations

/// Coupe un morceau à une articulation (le carpe, le jarret) : le haut garde
/// ce qui est au-dessus de `joint` et le disque de rayon `radius` autour de
/// lui, et se pose par-dessus ; le bas garde le reste, et, sous le disque
/// (caché au repos), ce qui y est opaque. En tournant autour de `joint`, le
/// bas reste sous le disque du haut : rien ne s'ouvre au genou.
fn split_joint(layer: &Layer, joint: (f32, f32), radius: f32) -> (Layer, Layer) {
    let mut upper = vec![0.0; W * H];
    let mut lower = vec![0.0; W * H];
    for y in 0..H {
        for x in 0..W {
            let i = y * W + x;
            let (px, py) = (x as f32 + 0.5, y as f32 + 0.5);
            let d = ((px - joint.0).powi(2) + (py - joint.1).powi(2)).sqrt();
            let disc = smoothstep(radius + 0.75, radius - 0.75, d);
            let above = smoothstep(joint.1 + 0.75, joint.1 - 0.75, py);
            let up = above.max(disc);
            upper[i] = up;
            // Sous le disque, un peu au-delà de son bord : pas de jointure.
            let under = if layer.px[i][3] > 0.98 {
                smoothstep(radius + 3.0, radius + 1.5, d)
            } else {
                0.0
            };
            lower[i] = (1.0 - up).max(under);
        }
    }
    (layer.masked(&upper), layer.masked(&lower))
}

// ------------------------------------------------------------------- lavis

/// Un lavis : une forme remplie de pigment.
#[derive(Clone, Copy, Debug)]
struct Wash {
    /// Teintes au soleil, au milieu, à l'ombre.
    light: Rgb,
    base: Rgb,
    shadow: Rgb,
    /// D'où vient la lumière (vers où va l'ombre), dans la toile.
    toward_shadow: (f32, f32),
    /// Pigment accumulé sur les bords en séchant : sa teinte, sa force, sa
    /// largeur (pixels).
    pigment: Rgb,
    edge: f32,
    edge_width: usize,
    /// Le lavis n'est jamais uni : variations douces de densité.
    mottling: f32,
    /// Grain du papier.
    grain: f32,
    /// Trait de crayon autour : teinte, opacité, largeur (pixels).
    pencil: Option<(Rgb, f32, usize)>,
    seed: u32,
}

impl Wash {
    /// Peint la forme `mask` dans `layer`.
    fn paint(&self, mask: &[f32], layer: &mut Layer) {
        // La boîte de la forme : la lumière la traverse d'un bord à l'autre.
        let (mut x0, mut y0, mut x1, mut y1) = (W, H, 0, 0);
        for (i, m) in mask.iter().enumerate() {
            if *m > 0.0 {
                let (x, y) = (i % W, i / W);
                x0 = x0.min(x);
                y0 = y0.min(y);
                x1 = x1.max(x);
                y1 = y1.max(y);
            }
        }
        if x0 > x1 {
            return;
        }
        let (cx, cy) = ((x0 + x1) as f32 * 0.5, (y0 + y1) as f32 * 0.5);
        let (dx, dy) = self.toward_shadow;
        let norm = (dx * dx + dy * dy).sqrt().max(1e-6);
        let (dx, dy) = (dx / norm, dy / norm);
        let extent = ((x1 - x0) as f32 * dx.abs() + (y1 - y0) as f32 * dy.abs()).max(1.0) * 0.5;
        // Bords : où le pigment s'accumule, et le volume qui s'arrondit.
        let soft = blur(mask, self.edge_width);
        let deep = blur(mask, self.edge_width * 4);
        let pencil = self.pencil.map(|(color, alpha, width)| {
            let inner = blur(mask, width);
            (color, alpha, inner)
        });
        let noise = Noise { seed: self.seed };
        let grain = Noise {
            seed: self.seed.wrapping_add(77),
        };
        for y in y0..=y1 {
            for x in x0..=x1 {
                let i = y * W + x;
                let m = mask[i];
                if m <= 0.0 {
                    continue;
                }
                let (fx, fy) = (x as f32, y as f32);
                let t = (((fx - cx) * dx + (fy - cy) * dy) / extent).clamp(-1.0, 1.0);
                let mut color = if t < 0.0 {
                    mix(self.base, self.light, -t)
                } else {
                    mix(self.base, self.shadow, t)
                };
                // Le volume : les bords tournent vers l'ombre.
                let round = (1.0 - deep[i]).clamp(0.0, 1.0);
                color = mix(color, self.shadow, round * 0.55);
                // Densité du lavis.
                let wash = noise.fbm(fx, fy, 110.0, 4);
                let k = 1.0 + self.mottling * wash;
                color = [color[0] * k, color[1] * k, color[2] * k];
                // Le pigment des bords.
                let rim = ((m - soft[i]) * 2.0).clamp(0.0, 1.0);
                color = mix(color, self.pigment, rim * self.edge);
                // Le grain du papier creuse le lavis par endroits.
                let g = grain.fbm(fx, fy, 5.0, 2).max(0.0);
                let k = 1.0 - self.grain * g;
                color = [color[0] * k, color[1] * k, color[2] * k];
                layer.over(i, color, m);
                if let Some((ink, alpha, inner)) = &pencil {
                    let band = (m * (1.0 - smoothstep(0.55, 0.98, inner[i]))).clamp(0.0, 1.0);
                    if band > 0.0 {
                        let line = noise.fbm(fx + 400.0, fy, 9.0, 2);
                        layer.over(i, *ink, band * alpha * (0.78 + 0.22 * line));
                    }
                }
            }
        }
    }
}

/// Pose `color` là où est `mask`, avec l'opacité `alpha`, seulement sur ce
/// que `clip` couvre déjà (un détail peint dans une forme).
fn dab(layer: &mut Layer, mask: &[f32], clip: Option<&[f32]>, color: Rgb, alpha: f32) {
    for (i, m) in mask.iter().enumerate() {
        let c = clip.map_or(1.0, |c| c[i]);
        let a = m * c * alpha;
        if a > 0.0 {
            layer.over(i, color, a);
        }
    }
}

// ------------------------------------------------------------------ formes

/// Une forme fermée passant par `points`, lissée (Catmull-Rom).
fn smooth_closed(points: &[(f32, f32)]) -> SkPath {
    let n = points.len();
    let mut pb = PathBuilder::new();
    pb.move_to(points[0].0, points[0].1);
    for i in 0..n {
        let p0 = points[(i + n - 1) % n];
        let p1 = points[i];
        let p2 = points[(i + 1) % n];
        let p3 = points[(i + 2) % n];
        let c1 = (p1.0 + (p2.0 - p0.0) / 6.0, p1.1 + (p2.1 - p0.1) / 6.0);
        let c2 = (p2.0 - (p3.0 - p1.0) / 6.0, p2.1 - (p3.1 - p1.1) / 6.0);
        pb.cubic_to(c1.0, c1.1, c2.0, c2.1, p2.0, p2.1);
    }
    pb.close();
    pb.finish().expect("forme")
}

/// Une courbe ouverte passant par `points`, lissée.
fn smooth_open(points: &[(f32, f32)]) -> SkPath {
    let n = points.len();
    let mut pb = PathBuilder::new();
    pb.move_to(points[0].0, points[0].1);
    for i in 0..n - 1 {
        let p0 = points[i.saturating_sub(1)];
        let p1 = points[i];
        let p2 = points[i + 1];
        let p3 = points[(i + 2).min(n - 1)];
        let c1 = (p1.0 + (p2.0 - p0.0) / 6.0, p1.1 + (p2.1 - p0.1) / 6.0);
        let c2 = (p2.0 - (p3.0 - p1.0) / 6.0, p2.1 - (p3.1 - p1.1) / 6.0);
        pb.cubic_to(c1.0, c1.1, c2.0, c2.1, p2.0, p2.1);
    }
    pb.finish().expect("courbe")
}

/// Une forme fermée aux coins vifs.
fn polygon(points: &[(f32, f32)]) -> SkPath {
    let mut pb = PathBuilder::new();
    pb.move_to(points[0].0, points[0].1);
    for &(x, y) in &points[1..] {
        pb.line_to(x, y);
    }
    pb.close();
    pb.finish().expect("polygone")
}

/// Ce qui est à `r` pixels au moins du bord de `mask` (la forme rentrée).
fn inset(mask: &[f32], r: usize) -> Vec<f32> {
    blur(mask, r)
        .iter()
        .map(|&v| smoothstep(0.9, 0.99, v))
        .collect()
}

/// Le produit de deux masques (ce que couvrent l'un et l'autre).
fn both(a: &[f32], b: &[f32]) -> Vec<f32> {
    a.iter().zip(b).map(|(a, b)| a * b).collect()
}

fn ellipse(cx: f32, cy: f32, rx: f32, ry: f32) -> SkPath {
    let rect = tiny_skia::Rect::from_xywh(cx - rx, cy - ry, 2.0 * rx, 2.0 * ry).expect("ellipse");
    PathBuilder::from_oval(rect).expect("ellipse")
}

/// Une feuille (une oreille) de `base` à `tip`, de largeur `width`.
fn leaf(base: (f32, f32), tip: (f32, f32), width: f32, bulge: f32) -> SkPath {
    let (dx, dy) = (tip.0 - base.0, tip.1 - base.1);
    let len = (dx * dx + dy * dy).sqrt();
    let (nx, ny) = (-dy / len, dx / len);
    let at = |t: f32, side: f32| {
        let w = width * 0.5 * (std::f32::consts::PI * t.powf(0.8)).sin().max(0.0) * side;
        (
            base.0 + dx * t + nx * (w + bulge * t * (1.0 - t) * 4.0),
            base.1 + dy * t + ny * (w + bulge * t * (1.0 - t) * 4.0),
        )
    };
    let mut points = Vec::new();
    for k in 0..=8 {
        points.push(at(k as f32 / 8.0, 1.0));
    }
    for k in (1..8).rev() {
        points.push(at(k as f32 / 8.0, -1.0));
    }
    smooth_closed(&points)
}

// ---------------------------------------------------------------- la toison

/// Les lavis d'une brebis : sa laine, sa tête, ses pattes du côté visible
/// et du côté caché.
struct Palette {
    wool: Wash,
    face: Wash,
    leg: Wash,
    far_leg: Wash,
}

/// Le creux du bas de la toison le plus proche de `x`, à 12 pixels près : le
/// point où son bord du bas (là où l'opacité passe un demi) remonte le plus
/// haut ; au milieu, s'il est plat sur quelques colonnes.
fn valley(layer: &Layer, x: f32) -> (f32, f32) {
    let bottom = |col: usize| (0..H).rev().find(|&y| layer.px[y * W + col][3] > 0.5);
    let guess = x.round() as usize;
    let cols: Vec<(usize, usize)> = (guess - 12..=guess + 12)
        .filter_map(|c| bottom(c).map(|y| (c, y)))
        .collect();
    let top = cols
        .iter()
        .map(|&(_, y)| y)
        .min()
        .expect("le bas de la toison");
    let at: Vec<usize> = cols
        .iter()
        .filter(|&&(_, y)| y == top)
        .map(|&(c, _)| c)
        .collect();
    let mid = at.iter().sum::<usize>() as f32 / at.len() as f32;
    (mid + 0.5, top as f32 + 1.0)
}

/// Détache de la toison ce qu'elle peint sous la corde `a`–`b` (deux creux
/// du contour), entre les verticales de ses deux bouts : une mèche, peinte
/// sous le corps. Au-dessus de la corde, la mèche déborde de `band` pixels
/// sous le corps, qui s'y fond sur `band` pixels : en penchant, elle ne
/// découvre rien. Sous la corde, ses côtés sont nets : il n'y passe que le
/// vide entre deux festons, et la mèche n'emporte rien de ses voisines. Au
/// repos, rien ne change.
fn split_lock(layer: &Layer, a: (f32, f32), b: (f32, f32), band: f32) -> (Layer, Layer) {
    let (x0, x1) = (a.0.min(b.0), a.0.max(b.0));
    let slope = (b.1 - a.1) / (b.0 - a.0);
    let norm = (1.0 + slope * slope).sqrt();
    let mut lock = vec![0.0; W * H];
    let mut body = vec![0.0; W * H];
    for y in 0..H {
        for x in 0..W {
            let i = y * W + x;
            let (px, py) = (x as f32 + 0.5, y as f32 + 0.5);
            // Sous la corde (positif), et entre ses deux bouts (positif).
            let below = (py - a.1 - slope * (px - a.0)) / norm;
            let inside = (px - x0).min(x1 - px);
            let within = if inside >= 0.0 { 1.0 } else { 0.0 };
            lock[i] = if below >= 0.0 {
                within
            } else {
                smoothstep(-2.0 * band, -band, below) * smoothstep(-2.0 * band, -band, inside)
            };
            body[i] = 1.0 - smoothstep(-band, 0.0, below) * within;
        }
    }
    (layer.masked(&lock), layer.masked(&body))
}

fn main() {
    let mut args = std::env::args().skip(1).peekable();
    // `--solognote` reste accepté (le peintre ne peint plus qu'elle).
    if args.peek().is_some_and(|arg| arg == "--solognote") {
        args.next();
    }
    let out = args
        .next()
        .map_or_else(|| PathBuf::from("pets/solognote/morceaux"), PathBuf::from);
    let preview = args.next().map(|flag| {
        assert_eq!(flag, "--apercu", "option attendue : --apercu <image.png>");
        PathBuf::from(args.next().expect("chemin de la planche de travail"))
    });
    assert!(args.next().is_none(), "trop d'arguments");
    solognote::paint(&out, preview.as_deref());
}
