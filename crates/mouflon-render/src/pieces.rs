//! Tout en morceaux d'image.
//!
//! La liste d'affichage d'un animal n'a presque plus que des images ; ce qui
//! reste — l'ombre floutée, les disques des particules et de la soucoupe —
//! devient ici une image cuite une fois pour toutes (partagée par tous ceux
//! qui la montrent), avec une transformation et une opacité. Un dessin
//! vectoriel qu'on n'a pas pu cuire (un rig à groupes semi-transparents)
//! est dessiné sur le processeur, d'un bloc.
//!
//! Le résultat se pose tel quel : par des calques Core Animation sur Mac
//! (une image partagée par calque), ou par des quadrilatères sur le GPU.

use std::collections::HashMap;
use std::sync::{LazyLock, Mutex};

use peniko::kurbo::{Affine, Point, Rect, Shape, Vec2};
use peniko::{Color, Extend, Mix};

use crate::display::{Bitmap, Command, DisplayList, Paint, Stop};

/// Un morceau : une image posée par une transformation, avec une opacité.
#[derive(Clone, Debug)]
pub struct Piece {
    pub bitmap: Bitmap,
    /// Des pixels de l'image vers ceux de la liste d'origine.
    pub transform: Affine,
    pub alpha: f32,
    /// Lissée (sinon, au plus proche : du pixel art agrandi).
    pub smooth: bool,
    /// Groupe dont l'opacité s'applique à l'ensemble de ses morceaux (le
    /// corps d'un animal qui s'efface) : 0 sans groupe, sinon son rang à
    /// partir de 1 (voir [`Pieces::groups`]).
    pub group: u16,
}

/// Les morceaux d'une liste, dans l'ordre où ils se dessinent.
#[derive(Clone, Debug, Default)]
pub struct Pieces {
    pub items: Vec<Piece>,
    /// Opacité de chaque groupe : le groupe `n` a l'opacité `groups[n - 1]`.
    /// Les morceaux d'un même groupe se suivent.
    pub groups: Vec<f32>,
    /// La liste n'a pas pu être découpée : elle a été dessinée d'un bloc sur
    /// le processeur (une seule image, refaite à chaque fois).
    pub rasterized: bool,
}

impl Pieces {
    pub fn clear(&mut self) {
        self.items.clear();
        self.groups.clear();
        self.rasterized = false;
    }

    /// Les morceaux en liste d'affichage d'images (et de calques pour les
    /// groupes), pour le rendu par quadrilatères.
    pub fn write_list(&self, out: &mut DisplayList) {
        let mut open: u16 = 0;
        for piece in &self.items {
            if piece.group != open {
                if open != 0 {
                    out.pop_layer();
                }
                if piece.group != 0 {
                    let alpha = self.group_alpha(piece.group);
                    out.push_layer(
                        out.bounds().to_path(0.1),
                        Affine::IDENTITY,
                        alpha,
                        Mix::Normal,
                    );
                }
                open = piece.group;
            }
            out.push(Command::Image {
                bitmap: piece.bitmap.clone(),
                transform: piece.transform,
                alpha: piece.alpha,
                smooth: piece.smooth,
            });
        }
        if open != 0 {
            out.pop_layer();
        }
    }

    /// Opacité du groupe `group` (1 sans groupe).
    #[must_use]
    pub fn group_alpha(&self, group: u16) -> f32 {
        match group {
            0 => 1.0,
            n => self.groups.get(usize::from(n) - 1).copied().unwrap_or(1.0),
        }
    }
}

/// Découpe `list` en morceaux d'image (voir le module).
pub fn flatten(list: &DisplayList, out: &mut Pieces) {
    out.clear();
    if !splittable(list) {
        rasterize_whole(list, out);
        return;
    }
    // Groupes ouverts : le plus extérieur devient un groupe ; ceux qu'il
    // contient multiplient l'opacité de leurs morceaux.
    let mut stack: Vec<f32> = Vec::new();
    let mut group: u16 = 0;
    for command in &list.commands {
        let inner = stack.iter().skip(1).product::<f32>();
        match command {
            Command::Image {
                bitmap,
                transform,
                alpha,
                smooth,
            } => out.items.push(Piece {
                bitmap: bitmap.clone(),
                transform: *transform,
                alpha: alpha * inner,
                smooth: *smooth,
                group,
            }),
            Command::BlurredRect {
                rect,
                radius,
                std_dev,
                color,
                transform,
            } => {
                if let Some(mut piece) = soft_rect(*rect, *radius, *std_dev, *color, *transform) {
                    piece.alpha *= inner;
                    piece.group = group;
                    out.items.push(piece);
                }
            }
            Command::Disc {
                center,
                radius,
                paint,
                transform,
            } => {
                if let Some(mut piece) = disc(*center, *radius, paint, *transform) {
                    piece.alpha *= inner;
                    piece.group = group;
                    out.items.push(piece);
                }
            }
            Command::PushLayer { alpha, .. } => {
                if stack.is_empty() {
                    out.groups.push(alpha.clamp(0.0, 1.0));
                    group = out.groups.len() as u16;
                }
                stack.push(alpha.clamp(0.0, 1.0));
            }
            Command::PopLayer => {
                stack.pop();
                if stack.is_empty() {
                    group = 0;
                }
            }
            // Écartés par `splittable`.
            Command::Fill { .. } | Command::Stroke { .. } => {}
        }
    }
}

/// La liste se découpe-t-elle en morceaux ? Pas de tracé, et des calques
/// qui ne font que changer l'opacité de tout ce qu'ils contiennent.
fn splittable(list: &DisplayList) -> bool {
    let bounds = list.bounds();
    list.commands.iter().all(|command| match command {
        Command::Fill { .. } | Command::Stroke { .. } => false,
        Command::PushLayer {
            clip,
            transform,
            mix,
            ..
        } => {
            *mix == Mix::Normal
                && covers(transform.transform_rect_bbox(clip.bounding_box()), bounds)
        }
        _ => true,
    })
}

/// `outer` couvre-t-il `inner` (au demi-pixel près) ?
fn covers(outer: Rect, inner: Rect) -> bool {
    outer.x0 <= inner.x0 + 0.5
        && outer.y0 <= inner.y0 + 0.5
        && outer.x1 >= inner.x1 - 0.5
        && outer.y1 >= inner.y1 - 0.5
}

/// Toute la liste dessinée sur le processeur, en un seul morceau.
fn rasterize_whole(list: &DisplayList, out: &mut Pieces) {
    out.rasterized = true;
    let Some(pixmap) = crate::raster::rasterize(list) else {
        return;
    };
    let (width, height) = (pixmap.width(), pixmap.height());
    let mut rgba = pixmap.take();
    unpremultiply(&mut rgba);
    out.items.push(Piece {
        bitmap: Bitmap::from_rgba(width, height, rgba),
        transform: Affine::IDENTITY,
        alpha: 1.0,
        smooth: true,
        group: 0,
    });
}

/// RGBA prémultiplié → alpha non prémultiplié, sur place.
fn unpremultiply(rgba: &mut [u8]) {
    for px in rgba.as_chunks_mut::<4>().0 {
        let a = u32::from(px[3]);
        if a == 0 || a == 255 {
            continue;
        }
        for c in &mut px[..3] {
            *c = ((u32::from(*c) * 255 + a / 2) / a).min(255) as u8;
        }
    }
}

// ——— Images cuites, partagées ———

/// Côté d'une image cuite (pixels), au plus et au moins : chaque forme est
/// cuite à la puissance de deux juste au-dessus de sa taille à l'écran,
/// pour que l'image ne soit jamais réduite au point de perdre ses détails
/// (le bord fin d'une bulle).
const MAX_SIDE: f64 = 256.0;
const MIN_SIDE: f64 = 4.0;

/// La taille de cuisson pour une forme de `pixels` à l'écran.
fn bake_side(pixels: f64) -> f64 {
    if !pixels.is_finite() {
        return MAX_SIDE;
    }
    pixels
        .max(1.0)
        .log2()
        .ceil()
        .exp2()
        .clamp(MIN_SIDE, MAX_SIDE)
}

/// Le plus grand étirement d'une transformation (sa plus grande valeur
/// singulière) : combien de pixels fait, au plus, une unité.
fn stretch(transform: Affine) -> f64 {
    let [a, b, c, d, _, _] = transform.as_coeffs();
    let t = a * a + b * b + c * c + d * d;
    let det = a * d - b * c;
    ((t + (t * t - 4.0 * det * det).max(0.0).sqrt()) / 2.0).sqrt()
}
/// Au-delà, le cache des images cuites repart de zéro (il n'y en a que
/// quelques-unes en pratique : une par sorte d'ombre, de particule…).
const MAX_BAKED: usize = 512;

/// Clé d'une image cuite : ses paramètres, au millième.
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
enum BakeKey {
    Soft {
        aspect: i32,
        radius: i32,
        std_dev: i32,
        rgb: [u8; 3],
        side: u16,
    },
    Disc {
        paint: Vec<i32>,
        side: u16,
    },
}

static BAKED: LazyLock<Mutex<HashMap<BakeKey, Bitmap>>> =
    LazyLock::new(|| Mutex::new(HashMap::new()));

fn thousandths(v: f64) -> i32 {
    (v * 1000.0).round() as i32
}

/// L'image cuite de cette clé, faite par `bake` la première fois.
fn baked(key: BakeKey, bake: impl FnOnce() -> Option<Bitmap>) -> Option<Bitmap> {
    if let Ok(cache) = BAKED.lock()
        && let Some(bitmap) = cache.get(&key)
    {
        return Some(bitmap.clone());
    }
    let bitmap = bake()?;
    if let Ok(mut cache) = BAKED.lock() {
        if cache.len() >= MAX_BAKED {
            cache.clear();
        }
        return Some(cache.entry(key).or_insert(bitmap).clone());
    }
    Some(bitmap)
}

/// Nombre d'images cuites gardées (tests, journal).
#[must_use]
pub fn baked_count() -> usize {
    BAKED.lock().map_or(0, |cache| cache.len())
}

/// Le flou gaussien est coupé à 2,5 écarts types (comme Vello).
const KERNEL: f64 = 2.5;

/// Un rectangle arrondi flouté (une ombre) : une image cuite à la forme
/// réduite (hauteur 1), posée à sa taille.
fn soft_rect(
    rect: Rect,
    radius: f64,
    std_dev: f64,
    color: Color,
    transform: Affine,
) -> Option<Piece> {
    let (w, h) = (rect.width(), rect.height());
    if !(w > 0.0 && h > 0.0 && std_dev >= 0.0) {
        return None;
    }
    let [r, g, b, a] = color.components;
    if a <= 0.0 {
        return None;
    }
    let rgb = [r, g, b].map(|c| (c.clamp(0.0, 1.0) * 255.0).round() as u8);
    // Forme réduite : hauteur 1.
    let (aspect, radius_n, std_n) = (w / h, radius / h, std_dev / h);
    let margin = KERNEL * std_n;
    let (span_w, span_h) = (aspect + 2.0 * margin, 1.0 + 2.0 * margin);
    let side = bake_side(span_w.max(span_h) * h * stretch(transform));
    let key = BakeKey::Soft {
        aspect: thousandths(aspect),
        radius: thousandths(radius_n),
        std_dev: thousandths(std_n),
        rgb,
        side: side as u16,
    };
    // Pixels par unité réduite.
    let k = side / span_w.max(span_h);
    let bitmap = baked(key, || {
        bake_soft(
            aspect,
            radius_n,
            std_n,
            rgb,
            (span_w * k).ceil(),
            (span_h * k).ceil(),
            k,
        )
    })?;
    Some(Piece {
        bitmap,
        transform: transform
            * Affine::translate((rect.x0 - margin * h, rect.y0 - margin * h))
            * Affine::scale(h / k),
        alpha: a.clamp(0.0, 1.0),
        smooth: true,
        group: 0,
    })
}

/// L'ombre floutée de forme réduite (largeur `w`, hauteur 1), à `k` pixels
/// par unité, sur une image de `width` × `height` pixels.
fn bake_soft(
    w: f64,
    radius: f64,
    std_dev: f64,
    rgb: [u8; 3],
    width: f64,
    height: f64,
    k: f64,
) -> Option<Bitmap> {
    let (width, height) = (width as u32, height as u32);
    if width == 0 || height == 0 || width > 4096 || height > 4096 {
        return None;
    }
    let margin = KERNEL * std_dev;
    let mut rgba = Vec::with_capacity(width as usize * height as usize * 4);
    for y in 0..height {
        for x in 0..width {
            // Du centre du pixel au centre du rectangle.
            let local = Vec2::new(
                (f64::from(x) + 0.5) / k - margin - w / 2.0,
                (f64::from(y) + 0.5) / k - margin - 0.5,
            );
            let alpha = soft_coverage(local, w, 1.0, radius, std_dev);
            rgba.extend_from_slice(&[rgb[0], rgb[1], rgb[2], (alpha * 255.0).round() as u8]);
        }
    }
    Some(Bitmap::from_rgba(width, height, rgba))
}

/// Fonction d'erreur approchée (celle de Vello).
fn erf7(x: f64) -> f64 {
    let y = (x * std::f64::consts::FRAC_2_SQRT_PI).clamp(-100.0, 100.0);
    let yy = y * y;
    let z = y + (0.24295 + (0.03395 + 0.0104 * yy) * yy) * (y * yy);
    z / (1.0 + z * z).sqrt()
}

/// Couverture d'un rectangle arrondi flouté (largeur `w`, hauteur `h`, coins
/// de rayon `radius`, flou d'écart type `std_dev`) au point `local` (depuis
/// son centre) : la formule de Vello, celle du rendu direct.
fn soft_coverage(local: Vec2, w: f64, h: f64, radius: f64, std_dev: f64) -> f64 {
    let std_dev = std_dev.max(1e-5);
    let inv = 1.0 / std_dev;
    let min_edge = w.min(h);
    let radius_max = 0.5 * min_edge;
    let r0 = radius.hypot(std_dev * 1.15).min(radius_max);
    let r1 = radius.hypot(std_dev * 2.0).min(radius_max);
    let exponent = 2.0 * r1 / r0.max(1e-9);
    let delta =
        1.25 * std_dev * ((-(0.5 * inv * w).powi(2)).exp() - (-(0.5 * inv * h).powi(2)).exp());
    let width = w + delta.min(0.0);
    let height = h - delta.max(0.0);
    let scale = 0.5 * erf7(inv * 0.5 * (width.max(height) - 0.5 * radius));
    let y0 = local.y.abs() - (height * 0.5 - r1);
    let x0 = local.x.abs() - (width * 0.5 - r1);
    let d_pos = (x0.max(0.0).powf(exponent) + y0.max(0.0).powf(exponent)).powf(1.0 / exponent);
    let d_neg = x0.max(y0).min(0.0);
    let d = d_pos + d_neg - r1;
    (scale * (erf7(inv * (min_edge + d)) - erf7(inv * d))).clamp(0.0, 1.0)
}

/// Un disque (une particule, un anneau de la soucoupe) : une image cuite du
/// disque unité et de son remplissage, posée à sa place. L'opacité la plus
/// forte du remplissage devient celle du morceau : une particule qui
/// s'efface garde la même image.
fn disc(center: Point, radius: f64, paint: &Paint, transform: Affine) -> Option<Piece> {
    if !(radius > 0.0 && radius.is_finite()) {
        return None;
    }
    // Le remplissage dans le repère du disque unité.
    let to_unit = |p: Point| ((p - center) / radius).to_point();
    let (unit, alpha) = normalized(paint, to_unit, radius)?;
    let diameter = bake_side(2.0 * radius * stretch(transform));
    let key = BakeKey::Disc {
        paint: paint_key(&unit),
        side: diameter as u16,
    };
    let half = diameter / 2.0;
    // Un pixel et demi de plus autour, pour le bord antialiasé.
    let side = diameter + 4.0;
    let bitmap = baked(key, || bake_disc(&unit, side as u32, half))?;
    Some(Piece {
        bitmap,
        transform: transform
            * Affine::translate(center.to_vec2())
            * Affine::scale(radius / half)
            * Affine::translate((-side / 2.0, -side / 2.0)),
        alpha,
        smooth: true,
        group: 0,
    })
}

/// Le remplissage ramené au disque unité, ses couleurs à leur opacité la
/// plus forte portée à 1 ; et cette opacité.
fn normalized(
    paint: &Paint,
    to_unit: impl Fn(Point) -> Point,
    radius: f64,
) -> Option<(Paint, f32)> {
    let peak = |stops: &[Stop]| {
        stops
            .iter()
            .map(|s| s.color.components[3])
            .fold(0.0f32, f32::max)
    };
    let scaled = |stops: &[Stop], peak: f32| -> Vec<Stop> {
        stops
            .iter()
            .map(|s| Stop {
                offset: s.offset,
                color: s
                    .color
                    .with_alpha((s.color.components[3] / peak).clamp(0.0, 1.0)),
            })
            .collect()
    };
    let (unit, alpha) = match paint {
        Paint::Solid(color) => {
            let a = color.components[3];
            (Paint::Solid(color.with_alpha(1.0)), a)
        }
        Paint::Linear {
            start,
            end,
            stops,
            extend,
        } => {
            let a = peak(stops);
            (
                Paint::Linear {
                    start: to_unit(*start),
                    end: to_unit(*end),
                    stops: scaled(stops, a.max(1e-6)),
                    extend: *extend,
                },
                a,
            )
        }
        Paint::Radial {
            start,
            start_radius,
            end,
            end_radius,
            stops,
            extend,
        } => {
            let a = peak(stops);
            (
                Paint::Radial {
                    start: to_unit(*start),
                    start_radius: (f64::from(*start_radius) / radius) as f32,
                    end: to_unit(*end),
                    end_radius: (f64::from(*end_radius) / radius) as f32,
                    stops: scaled(stops, a.max(1e-6)),
                    extend: *extend,
                },
                a,
            )
        }
    };
    (alpha > 0.0).then_some((unit, alpha.clamp(0.0, 1.0)))
}

/// Les paramètres d'un remplissage, au millième.
fn paint_key(paint: &Paint) -> Vec<i32> {
    let color = |c: Color| c.components.map(|v| thousandths(f64::from(v)));
    let stops = |stops: &[Stop], out: &mut Vec<i32>| {
        for stop in stops {
            out.push(thousandths(f64::from(stop.offset)));
            out.extend(color(stop.color));
        }
    };
    let extend = |e: Extend| match e {
        Extend::Pad => 0,
        Extend::Repeat => 1,
        Extend::Reflect => 2,
    };
    let mut key = Vec::with_capacity(32);
    match paint {
        Paint::Solid(c) => {
            key.push(0);
            key.extend(color(*c));
        }
        Paint::Linear {
            start,
            end,
            stops: s,
            extend: e,
        } => {
            key.extend([1, extend(*e)]);
            key.extend([start.x, start.y, end.x, end.y].map(thousandths));
            stops(s, &mut key);
        }
        Paint::Radial {
            start,
            start_radius,
            end,
            end_radius,
            stops: s,
            extend: e,
        } => {
            key.extend([2, extend(*e)]);
            key.extend(
                [
                    start.x,
                    start.y,
                    f64::from(*start_radius),
                    end.x,
                    end.y,
                    f64::from(*end_radius),
                ]
                .map(thousandths),
            );
            stops(s, &mut key);
        }
    }
    key
}

/// Le disque unité rempli de `paint` (dans son repère), sur une image de
/// `side` pixels de côté où le rayon fait `half` pixels.
fn bake_disc(paint: &Paint, side: u32, half: f64) -> Option<Bitmap> {
    let mut rgba = Vec::with_capacity(side as usize * side as usize * 4);
    let middle = f64::from(side) / 2.0;
    for y in 0..side {
        for x in 0..side {
            let p = Point::new(
                (f64::from(x) + 0.5 - middle) / half,
                (f64::from(y) + 0.5 - middle) / half,
            );
            // Distance au bord, en pixels : couverture antialiasée.
            let distance = (p.to_vec2().hypot() - 1.0) * half;
            let cover = (0.5 - distance).clamp(0.0, 1.0);
            let [r, g, b, a] = paint_at(paint, p);
            let a = a * cover;
            let straight = |c: f64| {
                if a > 0.0 {
                    ((c / a).clamp(0.0, 1.0) * 255.0).round() as u8
                } else {
                    0
                }
            };
            // Couleurs prémultipliées par la couverture : on ne garde que l'opacité.
            rgba.extend_from_slice(&[
                straight(r * cover),
                straight(g * cover),
                straight(b * cover),
                (a.clamp(0.0, 1.0) * 255.0).round() as u8,
            ]);
        }
    }
    Some(Bitmap::from_rgba(side, side, rgba))
}

/// Couleur (prémultipliée) du remplissage au point `p`.
fn paint_at(paint: &Paint, p: Point) -> [f64; 4] {
    match paint {
        Paint::Solid(c) => premultiplied(*c),
        Paint::Linear {
            start,
            end,
            stops,
            extend,
        } => {
            let d = *end - *start;
            let t = (p - *start).dot(d) / d.hypot2().max(1e-12);
            ramp(stops, extend_t(t, *extend))
        }
        Paint::Radial {
            start,
            start_radius,
            end,
            end_radius,
            stops,
            extend,
        } => {
            let t = radial_t(
                p,
                *start,
                f64::from(*start_radius),
                *end,
                f64::from(*end_radius),
            );
            ramp(stops, extend_t(t, *extend))
        }
    }
}

fn premultiplied(c: Color) -> [f64; 4] {
    let [r, g, b, a] = c.components.map(f64::from);
    let a = a.clamp(0.0, 1.0);
    [r * a, g * a, b * a, a]
}

fn extend_t(t: f64, extend: Extend) -> f64 {
    match extend {
        Extend::Pad => t.clamp(0.0, 1.0),
        Extend::Repeat => t - t.floor(),
        Extend::Reflect => {
            let r = t - 2.0 * (t * 0.5).floor();
            if r > 1.0 { 2.0 - r } else { r }
        }
    }
}

/// Couleur des arrêts à la position `t`, interpolée entre couleurs
/// prémultipliées (comme le rendu direct).
fn ramp(stops: &[Stop], t: f64) -> [f64; 4] {
    let Some(first) = stops.first() else {
        return [0.0; 4];
    };
    if t <= f64::from(first.offset) || stops.len() < 2 {
        return premultiplied(first.color);
    }
    for pair in stops.windows(2) {
        let (a, b) = (&pair[0], &pair[1]);
        if t <= f64::from(b.offset) {
            let span = f64::from(b.offset - a.offset).max(1e-6);
            let f = ((t - f64::from(a.offset)) / span).clamp(0.0, 1.0);
            let (ca, cb) = (premultiplied(a.color), premultiplied(b.color));
            return std::array::from_fn(|i| ca[i] + (cb[i] - ca[i]) * f);
        }
    }
    premultiplied(stops[stops.len() - 1].color)
}

/// Dégradé radial à deux cercles : le plus grand `t` pour lequel le point
/// est sur le cercle de centre `start + t (end - start)`, de rayon
/// `r0 + t (r1 - r0)` positif.
fn radial_t(p: Point, start: Point, r0: f64, end: Point, r1: f64) -> f64 {
    let d = end - start;
    let dr = r1 - r0;
    let q = p - start;
    let a = d.dot(d) - dr * dr;
    let b = q.dot(d) + r0 * dr;
    let c = q.dot(q) - r0 * r0;
    if a.abs() < 1e-6 {
        return c / (2.0 * b).max(1e-6);
    }
    let s = (b * b - a * c).max(0.0).sqrt();
    let (t1, t2) = ((b + s) / a, (b - s) / a);
    let high = t1.max(t2);
    if r0 + high * dr >= 0.0 {
        high
    } else {
        t1.min(t2)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::display::rgba;

    fn list() -> DisplayList {
        DisplayList::new(200, 160)
    }

    fn pixel(bitmap: &Bitmap, x: u32, y: u32) -> [u8; 4] {
        let rgba = bitmap.to_rgba();
        let i = ((y * bitmap.width + x) * 4) as usize;
        [rgba[i], rgba[i + 1], rgba[i + 2], rgba[i + 3]]
    }

    /// Où tombe le point `p` de la liste dans l'image d'un morceau.
    fn in_image(piece: &Piece, p: Point) -> Point {
        piece.transform.inverse() * p
    }

    #[test]
    fn images_stay_as_they_are() {
        let mut l = list();
        let bitmap = Bitmap::from_rgba(2, 2, vec![255; 16]);
        l.push(Command::Image {
            bitmap: bitmap.clone(),
            transform: Affine::translate((10.0, 20.0)),
            alpha: 0.5,
            smooth: false,
        });
        let mut pieces = Pieces::default();
        flatten(&l, &mut pieces);
        assert_eq!(pieces.items.len(), 1);
        assert!(!pieces.rasterized);
        let piece = &pieces.items[0];
        assert_eq!(piece.bitmap, bitmap);
        assert_eq!(piece.transform, Affine::translate((10.0, 20.0)));
        assert_eq!((piece.alpha, piece.smooth, piece.group), (0.5, false, 0));
    }

    #[test]
    fn a_shadow_becomes_a_shared_soft_image_at_its_place() {
        let shadow = |x: f64, w: f64, alpha: f32| Command::BlurredRect {
            rect: Rect::new(x, 100.0, x + w, 100.0 + w * 0.17),
            radius: w * 0.17 / 2.0,
            std_dev: w * 0.17 * 0.5,
            color: Color::BLACK.with_alpha(alpha),
            transform: Affine::scale(2.0),
        };
        let mut l = list();
        l.push(shadow(10.0, 60.0, 0.27));
        l.push(shadow(50.0, 90.0, 0.1));
        let mut pieces = Pieces::default();
        flatten(&l, &mut pieces);
        assert_eq!(pieces.items.len(), 2);
        let (a, b) = (&pieces.items[0], &pieces.items[1]);
        // La même forme à deux tailles : une seule image cuite.
        assert_eq!(a.bitmap, b.bitmap);
        assert_eq!((a.alpha, b.alpha), (0.27, 0.1));
        // Le centre de l'ombre tombe au centre de l'image, qui y est opaque
        // (presque : la forme est floutée) et noire.
        let center = Affine::scale(2.0) * Point::new(10.0 + 30.0, 100.0 + 60.0 * 0.17 / 2.0);
        let p = in_image(a, center);
        let (w, h) = (a.bitmap.width as f64, a.bitmap.height as f64);
        assert!(
            (p.x - w / 2.0).abs() < 1.0 && (p.y - h / 2.0).abs() < 1.0,
            "{p:?} dans {w}×{h}"
        );
        let [r, g, bb, alpha] = pixel(&a.bitmap, (w / 2.0) as u32, (h / 2.0) as u32);
        assert_eq!([r, g, bb], [0, 0, 0]);
        assert!(alpha > 150, "{alpha}");
        // Les coins de l'image sont transparents (le flou est coupé avant).
        assert!(pixel(&a.bitmap, 0, 0)[3] < 3);
    }

    #[test]
    fn the_soft_image_follows_the_direct_renderer_formula() {
        // Au centre d'une ombre large, la couverture vaut presque 1 ; au
        // bord du rectangle, à peu près la moitié.
        let center = soft_coverage(Vec2::ZERO, 6.0, 1.0, 0.5, 0.5);
        let edge = soft_coverage(Vec2::new(3.0, 0.0), 6.0, 1.0, 0.5, 0.5);
        assert!(center > 0.6 && center <= 1.0, "{center}");
        assert!(edge > 0.15 && edge < 0.6, "{edge}");
    }

    #[test]
    fn discs_share_one_image_per_paint_and_fade_by_opacity() {
        let dust = |r: f64, alpha: f32| {
            let center = Point::new(50.0, 50.0);
            Command::Disc {
                center,
                radius: r,
                paint: Paint::Radial {
                    start: center,
                    start_radius: 0.0,
                    end: center,
                    end_radius: r as f32,
                    stops: vec![
                        Stop {
                            offset: 0.0,
                            color: rgba(168, 160, 150, alpha),
                        },
                        Stop {
                            offset: 1.0,
                            color: rgba(168, 160, 150, 0.0),
                        },
                    ],
                    extend: Extend::Pad,
                },
                transform: Affine::IDENTITY,
            }
        };
        let mut l = list();
        l.push(dust(5.0, 0.5));
        l.push(dust(7.0, 0.2));
        l.disc(
            Point::new(10.0, 10.0),
            3.0,
            Affine::IDENTITY,
            Paint::Solid(Color::WHITE.with_alpha(0.85)),
        );
        let mut pieces = Pieces::default();
        flatten(&l, &mut pieces);
        assert_eq!(pieces.items.len(), 3);
        let [a, b, spark] = [&pieces.items[0], &pieces.items[1], &pieces.items[2]];
        // Même remplissage, même taille à la puissance de deux près : même image.
        assert_eq!(a.bitmap, b.bitmap, "même remplissage, même image");
        assert_ne!(a.bitmap, spark.bitmap);
        assert_eq!((a.alpha, b.alpha, spark.alpha), (0.5, 0.2, 0.85));
        // Le centre du disque, au milieu de l'image ; son bord, à un rayon.
        let c = in_image(a, Point::new(50.0, 50.0));
        let side = f64::from(a.bitmap.width);
        assert!((c.x - side / 2.0).abs() < 1e-6 && (c.y - side / 2.0).abs() < 1e-6);
        let edge = in_image(a, Point::new(55.0, 50.0));
        let half = (side - 4.0) / 2.0;
        assert!((edge.x - c.x - half).abs() < 1e-6);
        // Presque plein au centre (opacité ramenée à 1, le pixel du milieu
        // est à un demi-pixel du centre), transparent au bord.
        let mid = (side / 2.0) as u32;
        assert!(pixel(&a.bitmap, mid, mid)[3] > 200);
        assert_eq!(pixel(&a.bitmap, mid + half as u32 + 1, mid)[3], 0);
        let spark_mid = spark.bitmap.width / 2;
        assert_eq!(
            &pixel(&spark.bitmap, spark_mid, spark_mid),
            &[255, 255, 255, 255]
        );
    }

    #[test]
    fn shapes_are_baked_near_their_size_on_screen() {
        assert_eq!(bake_side(3.0), MIN_SIDE);
        assert_eq!(bake_side(5.0), 8.0);
        assert_eq!(bake_side(17.0), 32.0);
        assert_eq!(bake_side(32.0), 32.0);
        assert_eq!(bake_side(5000.0), MAX_SIDE);
        assert!((stretch(Affine::scale_non_uniform(3.0, 0.5)) - 3.0).abs() < 1e-9);
        assert!((stretch(Affine::rotate(0.7) * Affine::scale(2.0)) - 2.0).abs() < 1e-9);
    }

    #[test]
    fn an_opacity_layer_becomes_a_group() {
        let mut l = list();
        let bitmap = Bitmap::from_rgba(1, 1, vec![255; 4]);
        let image = |alpha| Command::Image {
            bitmap: bitmap.clone(),
            transform: Affine::IDENTITY,
            alpha,
            smooth: true,
        };
        l.push(image(1.0));
        l.push_layer(l.bounds().to_path(0.1), Affine::IDENTITY, 0.4, Mix::Normal);
        l.push(image(1.0));
        // Un calque dans le calque : son opacité passe à ses morceaux.
        l.push_layer(l.bounds().to_path(0.1), Affine::IDENTITY, 0.5, Mix::Normal);
        l.push(image(0.8));
        l.pop_layer();
        l.pop_layer();
        l.push(image(1.0));
        let mut pieces = Pieces::default();
        flatten(&l, &mut pieces);
        let groups: Vec<u16> = pieces.items.iter().map(|p| p.group).collect();
        assert_eq!(groups, vec![0, 1, 1, 0]);
        assert_eq!(pieces.groups, vec![0.4]);
        assert!((pieces.items[2].alpha - 0.4).abs() < 1e-6);
        // Et de retour en liste : le groupe redevient un calque.
        let mut back = list();
        pieces.write_list(&mut back);
        let kinds: Vec<&str> = back
            .commands
            .iter()
            .map(|c| match c {
                Command::Image { .. } => "image",
                Command::PushLayer { .. } => "calque",
                Command::PopLayer => "fin",
                _ => "?",
            })
            .collect();
        assert_eq!(kinds, ["image", "calque", "image", "image", "fin", "image"]);
    }

    #[test]
    fn paths_are_drawn_in_one_block() {
        let mut l = list();
        l.fill(
            Rect::new(20.0, 20.0, 60.0, 60.0).to_path(0.1),
            Affine::IDENTITY,
            Paint::Solid(Color::from_rgba8(200, 30, 30, 255)),
        );
        let mut pieces = Pieces::default();
        flatten(&l, &mut pieces);
        assert!(pieces.rasterized);
        assert_eq!(pieces.items.len(), 1);
        let piece = &pieces.items[0];
        assert_eq!((piece.bitmap.width, piece.bitmap.height), (200, 160));
        assert_eq!(pixel(&piece.bitmap, 40, 40), [200, 30, 30, 255]);
        assert_eq!(pixel(&piece.bitmap, 100, 100)[3], 0);
    }

    #[test]
    fn a_clipping_or_blending_layer_is_drawn_in_one_block() {
        let mut l = list();
        l.push_layer(
            Rect::new(0.0, 0.0, 10.0, 10.0).to_path(0.1),
            Affine::IDENTITY,
            1.0,
            Mix::Normal,
        );
        l.pop_layer();
        let mut pieces = Pieces::default();
        flatten(&l, &mut pieces);
        assert!(pieces.rasterized, "découpe plus petite que la liste");
        let mut l = list();
        l.push_layer(
            l.bounds().to_path(0.1),
            Affine::IDENTITY,
            1.0,
            Mix::SoftLight,
        );
        l.pop_layer();
        flatten(&l, &mut pieces);
        assert!(pieces.rasterized, "mode de fusion");
    }
}
