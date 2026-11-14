//! Dessins vectoriels cuits en images.
//!
//! Un rig bouge par morceaux : à chaque image, chaque partie (tête, pattes,
//! queue…) se déplace, tourne ou s'étire d'un bloc. Plutôt que de refaire
//! tous ses tracés à chaque image (avec un moteur de tracés sur le GPU,
//! comme Vello autrefois : une dizaine de passes de calcul sur toute la
//! fenêtre et plus de 150 Mo de tampons), chaque suite de tracés d'une même partie est dessinée une fois pour
//! toutes dans une image, à la finesse de l'écran. Le rendu direct n'a plus
//! qu'à poser ces images, tournées et étirées comme leur partie.
//!
//! L'éclairage du corps (lumière douce venue d'en haut à gauche de l'écran)
//! est cuit avec lui, dans les deux sens de l'animal : le reflet passe de
//! l'autre côté quand il se retourne.

use peniko::kurbo::{Affine, BezPath, PathEl, Point, Rect, Shape};
use peniko::{Color, Extend, Fill};
use tiny_skia::{
    BlendMode, FillRule, GradientStop, LinearGradient, Mask, Pixmap, RadialGradient, Shader,
    SpreadMode, Transform,
};

use crate::display::{Bitmap, LineCap, LineJoin, Paint, Stop, StrokeStyle};
use crate::rig::{Item, Part, Rig};

/// Pixels d'image par point affiché : deux (écrans Retina), et un quart de
/// plus pour les parties étirées (écrasement, respiration) et les bords tournés.
const PIXELS_PER_POINT: f64 = 2.0 * 1.25;
/// Image la plus grande qu'on accepte de cuire (pixels de côté).
const MAX_SIDE: f64 = 4096.0;

/// Un morceau cuit : son image, et où elle se pose dans le repère du dessin.
#[derive(Clone, Debug)]
pub struct Piece {
    pub bitmap: Bitmap,
    /// Le corps éclairé quand l'animal est retourné (le reflet change de côté).
    pub mirrored: Option<Bitmap>,
    /// Du repère de l'image (pixels) vers celui du dessin.
    pub placement: Affine,
}

/// Un pas du dessin d'un rig cuit : entrer dans une partie, en sortir, ou
/// poser un morceau (son rang dans [`BakedRig::drawn`]).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Step {
    Begin(Part),
    End,
    Piece(u32),
}

/// Les morceaux d'un rig, chacun repéré par le premier élément de sa suite,
/// et le programme qui les pose : une centaine de pas, que le dessin suit à
/// chaque image au lieu de parcourir les milliers de tracés du rig pour n'en
/// retenir que les quelques dizaines qui commencent un morceau.
#[derive(Clone, Debug, Default)]
pub struct BakedRig {
    /// Les morceaux, dans l'ordre du dessin, avec l'élément du rig où chacun
    /// commence.
    pieces: Vec<(usize, Piece)>,
    program: Vec<Step>,
}

impl BakedRig {
    /// Cuit le rig pour un affichage à `points_per_unit` points par unité du
    /// dessin, avec l'éclairage `lighting` sur le corps. `None` si le dessin
    /// ne se cuit pas (groupe semi-transparent, morceau démesuré…) : ses
    /// tracés sont alors dessinés tels quels, d'un bloc, sur le processeur
    /// (voir [`crate::pieces`]).
    #[must_use]
    pub fn bake(rig: &Rig, points_per_unit: f64, lighting: f32) -> Option<Self> {
        // Un groupe semi-transparent compose plusieurs morceaux ensemble.
        if rig
            .items
            .iter()
            .any(|i| matches!(i, Item::BeginLayer { .. } | Item::EndLayer))
        {
            return None;
        }
        let scale = points_per_unit * PIXELS_PER_POINT;
        if !(scale.is_finite() && scale > 0.0) {
            return None;
        }
        let mut pieces: Vec<(usize, Piece)> = Vec::new();
        let mut program: Vec<Step> = Vec::new();
        let mut parts: Vec<Part> = Vec::new();
        let mut run: Vec<usize> = Vec::new();
        let flush = |run: &mut Vec<usize>,
                     parts: &[Part],
                     pieces: &mut Vec<(usize, Piece)>,
                     program: &mut Vec<Step>|
         -> Option<()> {
            if let Some(&first) = run.first() {
                // La toison (le corps, l'encolure, les mèches) est éclairée.
                let lit = parts.last().is_some_and(|p| p.is_fleece());
                let light = (lit && lighting > 0.0).then_some(lighting);
                if let Some(piece) = bake_piece(rig, run, scale, light)? {
                    program.push(Step::Piece(u32::try_from(pieces.len()).ok()?));
                    pieces.push((first, piece));
                }
                run.clear();
            }
            Some(())
        };
        for (index, item) in rig.items.iter().enumerate() {
            match item {
                Item::BeginPart(part) => {
                    flush(&mut run, &parts, &mut pieces, &mut program)?;
                    parts.push(*part);
                    program.push(Step::Begin(*part));
                }
                Item::EndPart => {
                    flush(&mut run, &parts, &mut pieces, &mut program)?;
                    parts.pop();
                    program.push(Step::End);
                }
                Item::Fill { .. } | Item::Stroke { .. } => run.push(index),
                Item::BeginLayer { .. } | Item::EndLayer => return None,
            }
        }
        flush(&mut run, &parts, &mut pieces, &mut program)?;
        Some(Self { pieces, program })
    }

    /// Le morceau qui commence à l'élément `item` (les suivants de sa suite
    /// sont dedans).
    #[must_use]
    pub fn piece(&self, item: usize) -> Option<&Piece> {
        self.pieces
            .binary_search_by_key(&item, |(first, _)| *first)
            .ok()
            .map(|at| &self.pieces[at].1)
    }

    /// Le programme du dessin : les parties et les morceaux, dans l'ordre.
    #[must_use]
    pub fn program(&self) -> &[Step] {
        &self.program
    }

    /// Le morceau de ce rang (voir [`Step::Piece`]).
    #[must_use]
    pub fn drawn(&self, rank: u32) -> Option<&Piece> {
        self.pieces.get(rank as usize).map(|(_, piece)| piece)
    }

    /// Nombre de morceaux.
    #[must_use]
    pub fn len(&self) -> usize {
        self.pieces.len()
    }

    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.pieces.is_empty()
    }
}

/// Cuit une suite de tracés. `Some(None)` : rien de visible ; `None` : trop
/// grand pour être cuit.
fn bake_piece(rig: &Rig, items: &[usize], scale: f64, light: Option<f32>) -> Option<Option<Piece>> {
    // Boîte des tracés dans le repère du dessin, traits compris.
    let mut bounds: Option<Rect> = None;
    for &i in items {
        let b = match &rig.items[i] {
            Item::Fill {
                path, transform, ..
            } => transform.transform_rect_bbox(path.bounding_box()),
            Item::Stroke {
                path,
                transform,
                style,
                ..
            } => {
                // Le déterminant donne l'échelle d'une aire, pas celle du
                // trait : scale(10, 0.1) vaut 1 et couperait les côtés d'un
                // trait vertical. Projeter son rayon sur les deux axes
                // protège aussi les extrémités carrées et les longues
                // pointes. On garde l'ancienne marge lorsqu'elle suffit,
                // pour ne pas changer les images déjà cuites correctement.
                let old = style.width * transform.determinant().abs().sqrt() * 2.0;
                let join = match style.join {
                    LineJoin::Miter => style.miter_limit.max(1.0),
                    LineJoin::Round | LineJoin::Bevel => 1.0,
                };
                let cap = if style.cap == LineCap::Square {
                    std::f64::consts::SQRT_2
                } else {
                    1.0
                };
                let radius = style.width * 0.5 * join.max(cap);
                let [a, b, c, d, ..] = transform.as_coeffs();
                let grow_x = old.max(radius * a.hypot(c));
                let grow_y = old.max(radius * b.hypot(d));
                transform
                    .transform_rect_bbox(path.bounding_box())
                    .inflate(grow_x, grow_y)
            }
            _ => continue,
        };
        bounds = Some(bounds.map_or(b, |r| r.union(b)));
    }
    let Some(bounds) = bounds.filter(|b| b.width() > 0.0 && b.height() > 0.0) else {
        return Some(None);
    };
    // Deux pixels de marge : le bord antialiasé, et le filtrage une fois tourné.
    let pad = 2.0;
    let (w, h) = (
        (bounds.width() * scale).ceil() + 2.0 * pad,
        (bounds.height() * scale).ceil() + 2.0 * pad,
    );
    if w > MAX_SIDE || h > MAX_SIDE {
        return None;
    }
    let to_pixels = Affine::translate((pad, pad))
        * Affine::scale(scale)
        * Affine::translate(-bounds.origin().to_vec2());
    let mut pixmap = Pixmap::new(w as u32, h as u32)?;
    for &i in items {
        draw(&mut pixmap, &rig.items[i], to_pixels);
    }
    let (bitmap, mirrored) = match light.zip(rig.body_silhouette.as_ref()) {
        Some((strength, (silhouette, local))) => {
            let lit = |mirrored: bool| {
                let mut lit = pixmap.clone();
                light_body(&mut lit, silhouette, *local, to_pixels, strength, mirrored);
                to_bitmap(&lit)
            };
            (lit(false), Some(lit(true)))
        }
        None => (to_bitmap(&pixmap), None),
    };
    Some(Some(Piece {
        bitmap,
        mirrored,
        placement: to_pixels.inverse(),
    }))
}

/// Dessine un tracé du rig dans l'image.
fn draw(pixmap: &mut Pixmap, item: &Item, to_pixels: Affine) {
    let (path, transform, paint, brush) = match item {
        Item::Fill {
            path,
            transform,
            paint,
            brush_transform,
            ..
        }
        | Item::Stroke {
            path,
            transform,
            paint,
            brush_transform,
            ..
        } => (
            path,
            *transform,
            paint,
            brush_transform.unwrap_or(Affine::IDENTITY),
        ),
        _ => return,
    };
    let (Some(path), Some(shader)) = (to_path(path), to_shader(paint, brush)) else {
        return;
    };
    let paint = tiny_skia::Paint {
        shader,
        anti_alias: true,
        ..tiny_skia::Paint::default()
    };
    let transform = to_transform(to_pixels * transform);
    match item {
        Item::Fill { rule, .. } => {
            let rule = match rule {
                Fill::NonZero => FillRule::Winding,
                Fill::EvenOdd => FillRule::EvenOdd,
            };
            pixmap.fill_path(&path, &paint, rule, transform, None);
        }
        Item::Stroke { style, .. } => {
            pixmap.stroke_path(&path, &paint, &to_stroke(style), transform, None);
        }
        _ => {}
    }
}

/// L'éclairage du corps, comme celui du rendu vectoriel : une lumière douce
/// (mode « lumière tamisée ») qui éclaire le haut du côté gauche de l'écran
/// et assombrit le pourtour, dans la seule silhouette du corps.
fn light_body(
    pixmap: &mut Pixmap,
    silhouette: &BezPath,
    local: Affine,
    to_pixels: Affine,
    strength: f32,
    mirrored: bool,
) {
    let (Some(path), Some(mut mask)) = (
        to_path(silhouette),
        Mask::new(pixmap.width(), pixmap.height()),
    ) else {
        return;
    };
    mask.fill_path(
        &path,
        FillRule::Winding,
        true,
        to_transform(to_pixels * local),
    );
    let bbox = local.transform_rect_bbox(silhouette.bounding_box());
    // Retourné, l'animal montre à gauche de l'écran le côté droit du dessin.
    let x = if mirrored {
        bbox.max_x() - bbox.width() * 0.32
    } else {
        bbox.min_x() + bbox.width() * 0.32
    };
    let light = Point::new(x, bbox.min_y() + bbox.height() * 0.12);
    let radius = bbox.width().max(bbox.height()) * 1.05;
    let stop = |offset: f32, color: Color, alpha: f32| Stop {
        offset,
        color: color.with_alpha(alpha * strength),
    };
    let gradient = Paint::Radial {
        start: light,
        start_radius: 0.0,
        end: light,
        end_radius: radius as f32,
        stops: vec![
            stop(0.0, Color::WHITE, 0.95),
            stop(0.42, Color::WHITE, 0.0),
            stop(0.62, Color::BLACK, 0.0),
            stop(1.0, Color::BLACK, 0.75),
        ],
        extend: Extend::Pad,
    };
    let Some(shader) = to_shader(&gradient, Affine::IDENTITY) else {
        return;
    };
    let paint = tiny_skia::Paint {
        shader,
        blend_mode: BlendMode::SoftLight,
        anti_alias: true,
        ..tiny_skia::Paint::default()
    };
    let area = bbox.inflate(2.0, 2.0);
    if let Some(rect) = tiny_skia::Rect::from_ltrb(
        area.x0 as f32,
        area.y0 as f32,
        area.x1 as f32,
        area.y1 as f32,
    ) {
        let drawn: Vec<u8> = pixmap
            .data()
            .as_chunks::<4>()
            .0
            .iter()
            .map(|p| p[3])
            .collect();
        pixmap.fill_rect(rect, &paint, to_transform(to_pixels), Some(&mask));
        keep_coverage(pixmap, &drawn);
    }
}

/// La lumière ne change que la couleur du morceau, jamais ce qu'il couvre :
/// la silhouette est celle de toute la toison, et une lumière douce posée
/// sur du vide y peint un voile (clair près du reflet, sombre au pourtour)
/// qui suivait le morceau. L'encolure qui descend pour brouter le montrait,
/// en coin clair au garrot ou en halo autour du cou. `drawn` : l'opacité de
/// chaque pixel avant la lumière (pixels prémultipliés).
fn keep_coverage(pixmap: &mut Pixmap, drawn: &[u8]) {
    for (px, &alpha) in pixmap
        .data_mut()
        .as_chunks_mut::<4>()
        .0
        .iter_mut()
        .zip(drawn)
    {
        if px[3] == alpha {
            continue;
        }
        if alpha == 0 {
            px.fill(0);
            continue;
        }
        let k = f32::from(alpha) / f32::from(px[3]);
        for c in &mut px[..3] {
            *c = (f32::from(*c) * k).round().min(f32::from(alpha)) as u8;
        }
        px[3] = alpha;
    }
}

pub(crate) fn to_path(path: &BezPath) -> Option<tiny_skia::Path> {
    let mut builder = tiny_skia::PathBuilder::new();
    let f = |p: Point| (p.x as f32, p.y as f32);
    for el in path.elements() {
        match *el {
            PathEl::MoveTo(p) => {
                let (x, y) = f(p);
                builder.move_to(x, y);
            }
            PathEl::LineTo(p) => {
                let (x, y) = f(p);
                builder.line_to(x, y);
            }
            PathEl::QuadTo(a, p) => {
                let ((ax, ay), (x, y)) = (f(a), f(p));
                builder.quad_to(ax, ay, x, y);
            }
            PathEl::CurveTo(a, b, p) => {
                let ((ax, ay), (bx, by), (x, y)) = (f(a), f(b), f(p));
                builder.cubic_to(ax, ay, bx, by, x, y);
            }
            PathEl::ClosePath => builder.close(),
        }
    }
    builder.finish()
}

pub(crate) fn to_transform(a: Affine) -> Transform {
    let [sx, ky, kx, sy, tx, ty] = a.as_coeffs().map(|v| v as f32);
    Transform::from_row(sx, ky, kx, sy, tx, ty)
}

fn to_color(c: Color) -> tiny_skia::Color {
    let [r, g, b, a] = c.components.map(|v| v.clamp(0.0, 1.0));
    tiny_skia::Color::from_rgba(r, g, b, a).unwrap_or(tiny_skia::Color::TRANSPARENT)
}

/// Le remplissage, ses dégradés placés par `brush` (du repère du dégradé
/// vers celui du tracé).
pub(crate) fn to_shader(paint: &Paint, brush: Affine) -> Option<Shader<'static>> {
    let stops = |stops: &[Stop]| -> Vec<GradientStop> {
        stops
            .iter()
            .map(|s| GradientStop::new(s.offset, to_color(s.color)))
            .collect()
    };
    let spread = |extend: &Extend| match extend {
        Extend::Pad => SpreadMode::Pad,
        Extend::Repeat => SpreadMode::Repeat,
        Extend::Reflect => SpreadMode::Reflect,
    };
    let point = |p: Point| tiny_skia::Point::from_xy(p.x as f32, p.y as f32);
    match paint {
        Paint::Solid(c) => Some(Shader::SolidColor(to_color(*c))),
        Paint::Linear {
            start,
            end,
            stops: s,
            extend,
        } => LinearGradient::new(
            point(*start),
            point(*end),
            stops(s),
            spread(extend),
            to_transform(brush),
        ),
        Paint::Radial {
            start,
            start_radius,
            end,
            end_radius,
            stops: s,
            extend,
        } => RadialGradient::new(
            point(*start),
            *start_radius,
            point(*end),
            *end_radius,
            stops(s),
            spread(extend),
            to_transform(brush),
        ),
    }
}

pub(crate) fn to_stroke(style: &StrokeStyle) -> tiny_skia::Stroke {
    tiny_skia::Stroke {
        width: style.width as f32,
        miter_limit: style.miter_limit as f32,
        line_cap: match style.cap {
            LineCap::Butt => tiny_skia::LineCap::Butt,
            LineCap::Round => tiny_skia::LineCap::Round,
            LineCap::Square => tiny_skia::LineCap::Square,
        },
        line_join: match style.join {
            LineJoin::Miter => tiny_skia::LineJoin::Miter,
            LineJoin::Round => tiny_skia::LineJoin::Round,
            LineJoin::Bevel => tiny_skia::LineJoin::Bevel,
        },
        dash: None,
    }
}

/// Pixels prémultipliés de tiny-skia → image RGBA non prémultipliée.
pub(crate) fn to_bitmap(pixmap: &Pixmap) -> Bitmap {
    let mut rgba = Vec::with_capacity(pixmap.data().len());
    for &[r, g, b, a] in pixmap.data().as_chunks::<4>().0 {
        if a == 0 {
            rgba.extend_from_slice(&[0, 0, 0, 0]);
            continue;
        }
        let un = |c: u8| ((u32::from(c) * 255 + u32::from(a) / 2) / u32::from(a)).min(255) as u8;
        rgba.extend_from_slice(&[un(r), un(g), un(b), a]);
    }
    Bitmap::from_rgba(pixmap.width(), pixmap.height(), rgba)
}
