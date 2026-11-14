//! Ce que Mouflon annonce à l'écran : un nombre dans une bulle au-dessus
//! d'un animal, ou dans la lumière sous la soucoupe (le compte à rebours
//! avant une mise à jour, puis le numéro de la nouvelle version).
//!
//! Dessiné par Mouflon, sans police : chaque chiffre est un trait arrondi,
//! tracé à la main.

use std::collections::HashMap;
use std::sync::{LazyLock, Mutex, OnceLock};

use mouflon_core::SAUCER_WIDTH;
use mouflon_core::Vec2 as CoreVec2;
use mouflon_core::show::BEAM_MOUTH;
pub use mouflon_core::show::{BeamView, BubbleView, CountView};
use peniko::kurbo::{Affine, BezPath, Cap, Join, Point, Rect, RoundedRect, Shape, Stroke, Vec2};
use peniko::{Color, Extend};

use crate::display::{Command, DisplayList, Paint, Stop, rgba};
use crate::paint::{Canvas, Frame, Style};
use crate::props::{Prop, bake};

/// L'épaisseur du trait d'un chiffre, pour une hauteur de 1.
const WEIGHT: f64 = 0.17;
/// L'espace entre deux signes.
const GAP: f64 = 0.2;

/// Le trait d'un signe dans un cadre de hauteur 1 (le haut à 0), et sa
/// largeur. Les chiffres, et le point d'un numéro de version.
fn glyph(sign: char) -> Option<(&'static str, f64)> {
    Some(match sign {
        '0' => (
            "M0.32 0.04 C0.10 0.04 0.06 0.30 0.06 0.50 C0.06 0.70 0.10 0.96 0.32 0.96 C0.54 0.96 0.58 0.70 0.58 0.50 C0.58 0.30 0.54 0.04 0.32 0.04 Z",
            0.64,
        ),
        '1' => ("M0.08 0.26 L0.30 0.05 L0.30 0.96", 0.42),
        '2' => (
            "M0.08 0.26 C0.10 0.00 0.56 -0.02 0.56 0.28 C0.56 0.50 0.20 0.66 0.07 0.95 L0.58 0.95",
            0.64,
        ),
        '3' => (
            "M0.09 0.15 C0.22 0.00 0.55 0.01 0.55 0.26 C0.55 0.42 0.40 0.47 0.27 0.47 C0.44 0.47 0.59 0.55 0.59 0.72 C0.59 0.99 0.21 1.01 0.06 0.84",
            0.65,
        ),
        '4' => ("M0.46 0.96 L0.46 0.05 L0.06 0.68 L0.62 0.68", 0.68),
        '5' => (
            "M0.54 0.05 L0.16 0.05 L0.12 0.45 C0.27 0.34 0.59 0.38 0.59 0.68 C0.59 0.99 0.21 1.01 0.06 0.84",
            0.65,
        ),
        '6' => (
            "M0.52 0.09 C0.26 0.00 0.07 0.30 0.07 0.63 C0.07 1.00 0.59 1.00 0.59 0.67 C0.59 0.36 0.12 0.36 0.07 0.63",
            0.65,
        ),
        '7' => ("M0.06 0.05 L0.58 0.05 L0.26 0.96", 0.62),
        '8' => (
            "M0.32 0.47 C0.10 0.47 0.10 0.04 0.32 0.04 C0.54 0.04 0.54 0.47 0.32 0.47 C0.04 0.47 0.04 0.96 0.32 0.96 C0.60 0.96 0.60 0.47 0.32 0.47 Z",
            0.64,
        ),
        '9' => (
            "M0.13 0.91 C0.39 1.00 0.58 0.70 0.58 0.37 C0.58 0.00 0.06 0.00 0.06 0.33 C0.06 0.64 0.53 0.64 0.58 0.37",
            0.65,
        ),
        '.' => ("M0.09 0.93 L0.09 0.94", 0.18),
        _ => return None,
    })
}

/// Le tracé d'un texte (chiffres et points), centré sur l'origine, d'une
/// hauteur de 1 : les traits de chaque signe, à remplir d'un trait arrondi.
fn strokes(text: &str) -> (BezPath, f64) {
    let signs: Vec<(BezPath, f64)> = text
        .chars()
        .filter_map(glyph)
        .filter_map(|(path, width)| Some((BezPath::from_svg(path).ok()?, width)))
        .collect();
    let width =
        signs.iter().map(|(_, w)| w).sum::<f64>() + GAP * signs.len().saturating_sub(1) as f64;
    let mut all = BezPath::new();
    let mut x = -width * 0.5;
    for (path, w) in signs {
        all.extend(Affine::translate((x, -0.5)) * path);
        x += w + GAP;
    }
    (all, width)
}

fn pen(weight: f64) -> Stroke {
    Stroke::new(weight)
        .with_caps(Cap::Round)
        .with_join(Join::Round)
}

/// Écrit `text` centré en `center` (repère de `at`), haut de `height`.
fn write(
    out: &mut DisplayList,
    text: &str,
    center: Point,
    height: f64,
    at: Affine,
    color: Color,
    weight: f64,
) {
    let (path, _) = strokes(text);
    let place = at * Affine::translate(center.to_vec2()) * Affine::scale(height);
    let outline = peniko::kurbo::stroke(path, &pen(weight), &Default::default(), 0.002);
    out.fill(outline, place, Paint::Solid(color));
}

/// La largeur de `text` écrit haut de `height`.
fn width_of(text: &str, height: f64) -> f64 {
    strokes(text).1 * height
}

/// Hauteur des chiffres d'une bulle, et sa marge (points).
const BUBBLE_TEXT: f64 = 19.0;
const BUBBLE_PAD: (f64, f64) = (13.0, 9.0);
const TAIL: f64 = 9.0;
/// Pixels d'image par point pour ce qui est cuit ici.
const PIXELS: f64 = 2.5;
/// Les textes cuits gardés en mémoire (un compte, un numéro de version, en
/// clair et en sombre) : au-delà, on repart de rien.
const KEPT: usize = 64;

type Baked = LazyLock<Mutex<HashMap<(String, bool), Option<Prop>>>>;

/// Pose l'image cuite de `text` (cuite à la première demande, par `bake`).
fn draw_baked(
    cache: &Baked,
    text: &str,
    dark: bool,
    bake: impl FnOnce() -> Option<Prop>,
    at: Affine,
    alpha: f32,
    out: &mut DisplayList,
) {
    let Ok(mut cache) = cache.lock() else {
        return;
    };
    if cache.len() >= KEPT {
        cache.clear();
    }
    let prop = cache.entry((text.to_owned(), dark)).or_insert_with(bake);
    if let Some(prop) = prop {
        out.push(prop.draw(at, alpha));
    }
}

/// La taille d'une bulle qui dit `text` : sa largeur et sa hauteur, sans sa
/// queue.
fn bubble_size(text: &str) -> (f64, f64) {
    let w = (width_of(text, BUBBLE_TEXT) + 2.0 * BUBBLE_PAD.0)
        .max(BUBBLE_TEXT + 2.0 * BUBBLE_PAD.1 + 8.0);
    (w, BUBBLE_TEXT + 2.0 * BUBBLE_PAD.1)
}

/// Une bulle qui dit `text`, la pointe de sa queue à l'origine : un galet
/// clair, sa petite queue, son ombre douce, et le nombre.
fn bake_bubble(text: &str, dark: bool) -> Option<Prop> {
    let (w, h) = bubble_size(text);
    let body = Rect::new(-w * 0.5, -TAIL - h, w * 0.5, -TAIL);
    let (fill_top, fill_bottom, edge, ink) = if dark {
        (
            rgba(58, 56, 92, 0.96),
            rgba(44, 42, 74, 0.96),
            rgba(196, 190, 255, 0.55),
            rgba(238, 236, 255, 1.0),
        )
    } else {
        (
            rgba(255, 255, 255, 0.97),
            rgba(240, 238, 252, 0.97),
            rgba(92, 84, 150, 0.42),
            rgba(64, 56, 120, 1.0),
        )
    };
    let bounds = Rect::new(body.x0 - 16.0, body.y0 - 14.0, body.x1 + 16.0, 4.0);
    bake(bounds, PIXELS, |out, at| {
        // L'ombre, douce, un peu plus bas.
        out.push(Command::BlurredRect {
            rect: body + Vec2::new(0.0, 3.0),
            radius: h * 0.5,
            std_dev: 5.0,
            color: rgba(40, 30, 90, if dark { 0.45 } else { 0.22 }),
            transform: at,
        });
        // Le galet et sa queue, d'un seul tenant.
        let mut shape = RoundedRect::from_rect(body, h * 0.5).to_path(0.05);
        let mut tail = BezPath::new();
        tail.move_to((-7.0, -TAIL - 1.5));
        tail.quad_to((-2.0, -TAIL + 2.0), (0.0, 0.0));
        tail.quad_to((3.0, -TAIL + 1.0), (9.0, -TAIL - 1.5));
        tail.close_path();
        shape.extend(tail);
        let paint = Paint::Linear {
            start: Point::new(0.0, body.y0),
            end: Point::new(0.0, body.y1),
            stops: vec![
                Stop {
                    offset: 0.0,
                    color: fill_top,
                },
                Stop {
                    offset: 1.0,
                    color: fill_bottom,
                },
            ],
            extend: Extend::Pad,
        };
        out.fill(shape.clone(), at, paint);
        let rim = peniko::kurbo::stroke(shape, &pen(1.1), &Default::default(), 0.05);
        out.fill(rim, at, Paint::Solid(edge));
        write(
            out,
            text,
            Point::new(0.0, -TAIL - h * 0.5),
            BUBBLE_TEXT,
            at,
            ink,
            WEIGHT,
        );
    })
}

/// La fenêtre d'une bulle, et où est la pointe de sa queue dedans : de quoi
/// loger le plus long des textes, à sa plus grande taille.
#[must_use]
pub fn bubble_canvas() -> Canvas {
    Canvas {
        width: 380.0,
        height: 150.0,
        anchor: CoreVec2::new(190.0, 132.0),
    }
}

/// Dessine une bulle au-dessus d'un animal (voir [`BubbleView`]) : une
/// image cuite une fois par texte, et ses étincelles.
pub fn paint_bubble(view: &BubbleView, frame: &Frame, style: &Style, out: &mut DisplayList) {
    let scale = f64::from(view.scale.max(0.0));
    let alpha = view.alpha.clamp(0.0, 1.0);
    if scale < 0.01 || alpha <= 0.0 {
        return;
    }
    let tip = Point::new(f64::from(view.tip.x), f64::from(view.tip.y));
    // Elle grandit depuis le bout de sa queue.
    let at = frame.world_to_px() * Affine::translate(tip.to_vec2()) * Affine::scale(scale);
    draw_baked(
        &BUBBLES,
        &view.text,
        style.dark,
        || bake_bubble(&view.text, style.dark),
        at,
        alpha,
        out,
    );
    // Des étincelles qui scintillent autour d'elle, chacune à son tour.
    if let Some(phase) = view.sparkle {
        let (w, h) = bubble_size(&view.text);
        let Some(star) = star_prop() else {
            return;
        };
        for (k, (x, y, r)) in [
            (w * 0.5 + 7.0, -TAIL - h - 3.0, 5.5),
            (-w * 0.5 - 8.0, -TAIL - h * 0.72, 3.8),
            (w * 0.5 + 12.0, -TAIL - h * 0.35, 3.0),
        ]
        .into_iter()
        .enumerate()
        {
            let turn = (f64::from(phase) * 1.6 + k as f64 / 3.0).fract();
            let glint = (turn * std::f64::consts::PI).sin();
            let place = at * Affine::translate((x, y)) * Affine::scale(r * (0.55 + 0.45 * glint));
            out.push(star.draw(place, alpha * (0.35 + 0.65 * glint) as f32));
        }
    }
}

static BUBBLES: Baked = LazyLock::new(Mutex::default);
static COUNTS: Baked = LazyLock::new(Mutex::default);

/// Une petite étoile dorée à quatre branches, de rayon 1, cuite une fois.
fn star_prop() -> Option<&'static Prop> {
    static STAR: OnceLock<Option<Prop>> = OnceLock::new();
    STAR.get_or_init(|| {
        bake(Rect::new(-1.0, -1.0, 1.0, 1.0), 16.0, |out, at| {
            let mut path = BezPath::new();
            let k = 0.22;
            path.move_to((0.0, -1.0));
            path.quad_to((k, -k), (1.0, 0.0));
            path.quad_to((k, k), (0.0, 1.0));
            path.quad_to((-k, k), (-1.0, 0.0));
            path.quad_to((-k, -k), (0.0, -1.0));
            path.close_path();
            out.fill(path, at, Paint::Solid(rgba(255, 214, 92, 1.0)));
        })
    })
    .as_ref()
}

/// Hauteur des chiffres sous la soucoupe (points).
const COUNT_TEXT: f64 = 30.0;

/// Le compte sous la soucoupe, centré sur l'origine : des chiffres de
/// lumière, avec leur halo.
fn bake_count(text: &str, dark: bool) -> Option<Prop> {
    let w = width_of(text, COUNT_TEXT) + COUNT_TEXT;
    let h = COUNT_TEXT * 1.9;
    let glow = if dark {
        rgba(150, 255, 225, 1.0)
    } else {
        rgba(90, 225, 195, 1.0)
    };
    bake(
        Rect::new(-w * 0.5, -h * 0.5, w * 0.5, h * 0.5),
        PIXELS,
        |out, at| {
            // Trois traits l'un sur l'autre : un large halo, un halo serré, le
            // cœur.
            for (weight, color) in [
                (WEIGHT * 3.4, glow.with_alpha(0.16)),
                (WEIGHT * 2.1, glow.with_alpha(0.32)),
                (WEIGHT, Color::WHITE.with_alpha(0.98)),
            ] {
                write(out, text, Point::ZERO, COUNT_TEXT, at, color, weight);
            }
        },
    )
}

/// La fenêtre du compte, et où est son centre dedans.
#[must_use]
pub fn count_canvas() -> Canvas {
    Canvas {
        width: 180.0,
        height: 110.0,
        anchor: CoreVec2::new(90.0, 55.0),
    }
}

/// Dessine le compte sous la soucoupe (voir [`CountView`]).
pub fn paint_count(view: &CountView, frame: &Frame, style: &Style, out: &mut DisplayList) {
    let scale = f64::from(view.scale.max(0.0));
    let alpha = view.alpha.clamp(0.0, 1.0);
    if scale < 0.01 || alpha <= 0.0 {
        return;
    }
    let center = Point::new(f64::from(view.center.x), f64::from(view.center.y));
    let at = frame.world_to_px() * Affine::translate(center.to_vec2()) * Affine::scale(scale);
    draw_baked(
        &COUNTS,
        &view.text,
        style.dark,
        || bake_count(&view.text, style.dark),
        at,
        alpha,
        out,
    );
}

/// Marge autour du faisceau large (points).
const BEAM_MARGIN: f32 = 16.0;

/// La part de sa largeur au sol qu'un faisceau cuit a à sa bouche : il y
/// en a un par valeur, et on prend le plus proche de ce qu'il faut (la
/// bouche garde à peu près sa largeur, que le faisceau soit large ou très
/// large).
const MOUTH_SHARES: [f64; 7] = [0.30, 0.20, 0.13, 0.085, 0.055, 0.035, 0.022];

/// Le faisceau large dont la bouche fait à peu près `share` de sa largeur
/// au sol, dans un cadre d'une unité de haut et de deux de large (sa bouche
/// en haut, au milieu), cuit une fois par thème : il s'étire ensuite à la
/// taille voulue. Renvoie aussi la part exacte du faisceau choisi.
fn wide_beam_prop(dark: bool, share: f64) -> Option<(&'static Prop, f64)> {
    static BEAMS: OnceLock<Vec<[Option<Prop>; 2]>> = OnceLock::new();
    let beams = BEAMS.get_or_init(|| {
        MOUTH_SHARES
            .iter()
            .map(|&share| [bake_wide_beam(false, share), bake_wide_beam(true, share)])
            .collect()
    });
    let nearest = (0..MOUTH_SHARES.len()).min_by(|a, b| {
        let off = |i: &usize| (MOUTH_SHARES[*i].ln() - share.max(1e-6).ln()).abs();
        off(a).total_cmp(&off(b))
    })?;
    let prop = beams.get(nearest)?.get(usize::from(dark))?.as_ref()?;
    Some((prop, MOUTH_SHARES[nearest]))
}

fn bake_wide_beam(dark: bool, top: f64) -> Option<Prop> {
    // Cuit plus large que haut : c'est en largeur qu'il s'étire le plus.
    let (sx, sy) = (360.0, 240.0);
    bake(Rect::new(-sx, 0.0, sx, sy * 1.03), 1.0, |out, at| {
        let glow = if dark {
            rgba(170, 255, 225, 1.0)
        } else {
            rgba(120, 235, 200, 1.0)
        };
        let fade = |a: f32, b: f32, c: f32| Paint::Linear {
            start: Point::new(0.0, 0.0),
            end: Point::new(0.0, sy),
            stops: vec![
                Stop {
                    offset: 0.0,
                    color: glow.with_alpha(a),
                },
                Stop {
                    offset: 0.75,
                    color: glow.with_alpha(b),
                },
                Stop {
                    offset: 1.0,
                    color: glow.with_alpha(c),
                },
            ],
            extend: Extend::Pad,
        };
        // Deux cônes l'un dans l'autre : le bord reste doux, le cœur est
        // plus clair.
        for (inset, paint) in [(1.0, fade(0.34, 0.15, 0.03)), (0.62, fade(0.2, 0.08, 0.0))] {
            let mut cone = BezPath::new();
            cone.move_to((-sx * top * inset, 0.0));
            cone.line_to((sx * top * inset, 0.0));
            cone.line_to((sx * inset, sy));
            cone.quad_to((0.0, sy * 1.03), (-sx * inset, sy));
            cone.close_path();
            out.fill(cone, at, paint);
        }
    })
    .map(|mut prop| {
        // Du cadre cuit au cadre d'une unité.
        prop.placement = Affine::scale_non_uniform(1.0 / sx, 1.0 / sy) * prop.placement;
        prop
    })
}

/// La fenêtre du faisceau large, et où est sa bouche dedans.
#[must_use]
pub fn beam_canvas(view: &BeamView) -> Canvas {
    let width = 2.0 * view.half_width + 2.0 * BEAM_MARGIN;
    Canvas {
        width: width.ceil(),
        height: (view.length * 1.03 + 2.0 * BEAM_MARGIN).ceil(),
        anchor: CoreVec2::new(width * 0.5, BEAM_MARGIN),
    }
}

/// Dessine le faisceau large de la soucoupe (voir [`BeamView`]) : un cône
/// de lumière pâle qui entoure tout le troupeau, des anneaux qui y
/// descendent, et la lueur sous elle.
pub fn paint_beam(view: &BeamView, frame: &Frame, style: &Style, out: &mut DisplayList) {
    let alpha = f64::from(view.alpha.clamp(0.0, 1.0));
    if alpha <= 0.005 || view.length <= 1.0 || view.half_width <= 1.0 {
        return;
    }
    let top = Point::new(f64::from(view.top.x), f64::from(view.top.y));
    let at = frame.world_to_px() * Affine::translate(top.to_vec2());
    let (half, len) = (f64::from(view.half_width), f64::from(view.length));
    let mouth = f64::from(BEAM_MOUTH).min(half);
    if let Some((cone, _)) = wide_beam_prop(style.dark, mouth / half) {
        out.push(cone.draw(at * Affine::scale_non_uniform(half, len), alpha as f32));
    }
    // Les anneaux : plus larges et plus plats en descendant.
    for k in 0..4 {
        let phase = (f64::from(view.rings) + f64::from(k) / 4.0).fract();
        let wide = (mouth + (half - mouth) * phase) * 0.96;
        let fade = (1.0 - phase) * phase * 4.0;
        out.disc(
            Point::ZERO,
            1.0,
            at * Affine::translate((0.0, phase * len))
                * Affine::scale_non_uniform(wide, 3.0 + 7.0 * phase),
            Paint::Solid(Color::WHITE.with_alpha((0.17 * alpha * fade) as f32)),
        );
    }
    // La lueur à sa bouche.
    let r = f64::from(SAUCER_WIDTH) * 0.42;
    out.disc(
        Point::ZERO,
        r,
        at,
        Paint::Radial {
            start: Point::ZERO,
            start_radius: 0.0,
            end: Point::ZERO,
            end_radius: r as f32,
            stops: vec![
                Stop {
                    offset: 0.0,
                    color: rgba(190, 255, 235, (0.5 * alpha) as f32),
                },
                Stop {
                    offset: 1.0,
                    color: rgba(190, 255, 235, 0.0),
                },
            ],
            extend: Extend::Pad,
        },
    );
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_digit_and_the_dot_can_be_written() {
        for sign in "0123456789.".chars() {
            let (path, width) = glyph(sign).unwrap();
            let path = BezPath::from_svg(path).unwrap();
            let bounds = path.bounding_box();
            // Dans son cadre : de 0 à 1 de haut, de 0 à sa largeur.
            assert!(
                bounds.y0 >= -0.03 && bounds.y1 <= 1.03,
                "{sign} : {bounds:?}"
            );
            assert!(
                bounds.x0 >= 0.0 && bounds.x1 <= width,
                "{sign} : {bounds:?}"
            );
        }
        assert!(glyph('a').is_none());
    }

    #[test]
    fn a_text_is_centered_and_as_wide_as_its_signs() {
        let (one, w1) = strokes("7");
        let (two, w2) = strokes("10");
        let (version, w3) = strokes("1.0.5");
        assert!(w2 > w1 && w3 > w2);
        for (path, width) in [(one, w1), (two, w2), (version, w3)] {
            let bounds = path.bounding_box();
            assert!(bounds.x0 >= -width * 0.5 - 1e-9 && bounds.x1 <= width * 0.5 + 1e-9);
            assert!(bounds.y0 >= -0.55 && bounds.y1 <= 0.55);
        }
        // Ce qui ne s'écrit pas ne prend pas de place.
        assert_eq!(strokes("1a0").1, strokes("10").1);
    }

    fn frame() -> Frame {
        Frame {
            origin: CoreVec2::ZERO,
            scale: 2.0,
        }
    }

    /// Rien que des images et des disques : de quoi faire des calques.
    fn only_images_and_discs(out: &DisplayList) -> bool {
        out.commands
            .iter()
            .all(|c| matches!(c, Command::Image { .. } | Command::Disc { .. }))
    }

    #[test]
    fn a_bubble_is_one_baked_image_and_its_sparkles() {
        let mut view = BubbleView {
            tip: CoreVec2::new(100.0, 100.0),
            text: "10".into(),
            scale: 0.0,
            alpha: 1.0,
            sparkle: None,
        };
        let mut out = DisplayList::new(400, 400);
        paint_bubble(&view, &frame(), &Style::default(), &mut out);
        assert!(out.commands.is_empty(), "pas encore née");
        view.scale = 1.0;
        paint_bubble(&view, &frame(), &Style::default(), &mut out);
        assert_eq!(out.commands.len(), 1);
        view.sparkle = Some(0.3);
        view.text = "1.0.5".into();
        let mut out = DisplayList::new(400, 400);
        paint_bubble(&view, &frame(), &Style::default(), &mut out);
        assert_eq!(out.commands.len(), 4, "la bulle et trois étincelles");
        assert!(only_images_and_discs(&out));
        // La plus longue bulle tient dans sa fenêtre, à sa plus grande taille.
        let (w, h) = bubble_size("10.10.10");
        let canvas = bubble_canvas();
        assert!(f64::from(canvas.width) > (w + 40.0) * 1.3);
        assert!(f64::from(canvas.anchor.y) > (h + TAIL + 14.0) * 1.3);
    }

    #[test]
    fn the_count_and_the_wide_beam_are_images_and_discs() {
        let mut out = DisplayList::new(800, 800);
        let count = CountView {
            center: CoreVec2::new(200.0, 100.0),
            text: "7".into(),
            scale: 1.1,
            alpha: 1.0,
        };
        paint_count(&count, &frame(), &Style::default(), &mut out);
        assert_eq!(out.commands.len(), 1);
        let beam = BeamView {
            top: CoreVec2::new(200.0, 60.0),
            length: 300.0,
            half_width: 180.0,
            alpha: 1.0,
            rings: 0.4,
        };
        paint_beam(&beam, &frame(), &Style::default(), &mut out);
        assert!(out.commands.len() > 3);
        assert!(only_images_and_discs(&out));
        // Le cône s'étire jusqu'à ses bords : de sa bouche au sol, et de
        // toute sa largeur.
        let Some(Command::Image {
            bitmap, transform, ..
        }) = out.commands.get(1)
        else {
            panic!("le cône d'abord");
        };
        let corner = *transform * Point::new(f64::from(bitmap.width), f64::from(bitmap.height));
        let origin = *transform * Point::ZERO;
        assert!((origin.x - (200.0 - 180.0) * 2.0).abs() < 4.0, "{origin:?}");
        assert!((corner.x - (200.0 + 180.0) * 2.0).abs() < 4.0, "{corner:?}");
        assert!((origin.y - 60.0 * 2.0).abs() < 4.0, "{origin:?}");
        assert!(corner.y > (60.0 + 300.0) * 2.0, "{corner:?}");
        let canvas = beam_canvas(&beam);
        assert!(canvas.width >= 360.0 && canvas.height >= 300.0);
        // La bouche garde à peu près sa largeur, que le faisceau soit
        // large ou très large : il sort du ventre de la soucoupe.
        for half in [190.0_f64, 400.0, 900.0, 1600.0] {
            let mouth = f64::from(BEAM_MOUTH);
            let (_, share) = wide_beam_prop(false, mouth / half).expect("cuit");
            let drawn = share * half;
            assert!(
                drawn > mouth * 0.7 && drawn < mouth * 1.45,
                "à {half} : une bouche de {drawn}"
            );
        }
        // Éteint, il ne dessine rien.
        let mut out = DisplayList::new(800, 800);
        paint_beam(
            &BeamView { alpha: 0.0, ..beam },
            &frame(),
            &Style::default(),
            &mut out,
        );
        assert!(out.commands.is_empty());
    }
}
