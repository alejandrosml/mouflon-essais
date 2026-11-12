//! La soucoupe volante des surprises (voir [`mouflon_core::Saucer`]) :
//! dessinée par Mouflon, sans image. Un corps argenté et lavande, un dôme de
//! verre, des lumières qui clignotent autour, et, sous elle, le faisceau qui
//! aspire un animal.
//!
//! Le corps et le faisceau sont cuits une fois pour toutes en images (voir
//! [`crate::props`]) ; les anneaux, la lueur et les lumières sont des
//! disques : rien que des images et des disques, sans tracé.

use std::sync::OnceLock;

use mouflon_core::{BEAM_MAX, SAUCER_HEIGHT, SAUCER_WIDTH, SaucerView, Vec2 as CoreVec2};
use peniko::kurbo::{Affine, BezPath, Ellipse, Point, Rect, Shape};
use peniko::{Color, Extend};

use crate::display::{DisplayList, Paint, Stop, rgba};
use crate::paint::{Canvas, Frame, Style};
use crate::props::{Prop, bake};

/// Pixels d'image par point pour le corps : deux (Retina), et un quart de plus.
const BODY_PIXELS: f64 = 2.5;
/// Pour le faisceau, une lumière douce : moins de pixels suffisent.
const BEAM_PIXELS: f64 = 1.5;

/// Marge autour du dessin (points).
const MARGIN: f32 = 22.0;
/// Au-dessus d'elle, la place de la fumée quand on l'a percutée (points).
const SMOKE_ROOM: f32 = 90.0;

/// Taille de la fenêtre de la soucoupe, et où est son centre dedans.
#[must_use]
pub fn saucer_canvas() -> Canvas {
    let width = SAUCER_WIDTH + 2.0 * MARGIN;
    let height = SMOKE_ROOM + SAUCER_HEIGHT + BEAM_MAX + 2.0 * MARGIN;
    Canvas {
        width: width.ceil(),
        height: height.ceil(),
        anchor: CoreVec2::new(width * 0.5, SMOKE_ROOM + MARGIN + SAUCER_HEIGHT * 0.5),
    }
}

fn stops(colors: &[(f32, Color)]) -> Vec<Stop> {
    colors
        .iter()
        .map(|&(offset, color)| Stop { offset, color })
        .collect()
}

/// Haut du faisceau, sous le corps (points).
fn beam_top() -> f64 {
    f64::from(SAUCER_HEIGHT) * 0.3
}

/// Le faisceau à sa plus grande longueur et en pleine lumière, cuit une fois
/// par thème ; il s'étire en hauteur et s'efface en se posant.
fn beam_prop(dark: bool) -> Option<&'static Prop> {
    static BEAMS: OnceLock<[Option<Prop>; 2]> = OnceLock::new();
    BEAMS
        .get_or_init(|| [bake_beam(false), bake_beam(true)])
        .get(usize::from(dark))?
        .as_ref()
}

fn bake_beam(dark: bool) -> Option<Prop> {
    let w = f64::from(SAUCER_WIDTH);
    let len = f64::from(BEAM_MAX);
    let top = beam_top();
    let bounds = Rect::new(-w * 0.43, top, w * 0.43, top + len + 6.0);
    bake(bounds, BEAM_PIXELS, |out, at| {
        let mut cone = BezPath::new();
        cone.move_to((-w * 0.2, top));
        cone.line_to((w * 0.2, top));
        cone.line_to((w * 0.42, top + len));
        cone.quad_to((0.0, top + len + 10.0), (-w * 0.42, top + len));
        cone.close_path();
        let glow = if dark {
            rgba(170, 255, 225, 1.0)
        } else {
            rgba(120, 235, 200, 1.0)
        };
        out.fill(
            cone,
            at,
            Paint::Linear {
                start: Point::new(0.0, top),
                end: Point::new(0.0, top + len),
                stops: stops(&[
                    (0.0, glow.with_alpha(0.55)),
                    (0.7, glow.with_alpha(0.22)),
                    (1.0, glow.with_alpha(0.0)),
                ]),
                extend: Extend::Pad,
            },
        );
    })
}

/// Le corps (dôme, reflet, coque, ceinture, liseré), cuit une fois.
fn body_prop() -> Option<&'static Prop> {
    static BODY: OnceLock<Option<Prop>> = OnceLock::new();
    BODY.get_or_init(bake_body).as_ref()
}

fn bake_body() -> Option<Prop> {
    let w = f64::from(SAUCER_WIDTH);
    let h = f64::from(SAUCER_HEIGHT);
    let bounds = Rect::new(-w * 0.52, -h * 0.58, w * 0.52, h * 0.4);
    bake(bounds, BODY_PIXELS, |out, body| {
        // Le dôme de verre, derrière le corps.
        let dome = Ellipse::new((0.0, -h * 0.12), (w * 0.24, h * 0.42), 0.0).to_path(0.1);
        out.fill(
            dome,
            body,
            Paint::Linear {
                start: Point::new(0.0, -h * 0.55),
                end: Point::new(0.0, h * 0.1),
                stops: stops(&[
                    (0.0, rgba(214, 248, 255, 0.92)),
                    (0.6, rgba(120, 205, 225, 0.88)),
                    (1.0, rgba(60, 130, 160, 0.9)),
                ]),
                extend: Extend::Pad,
            },
        );
        // Reflet du dôme.
        let shine = Ellipse::new((-w * 0.08, -h * 0.34), (w * 0.07, h * 0.1), -0.5).to_path(0.1);
        out.fill(shine, body, Paint::Solid(Color::WHITE.with_alpha(0.7)));

        // Le corps : une ellipse argentée, lavande dessous.
        let hull = Ellipse::new((0.0, h * 0.08), (w * 0.5, h * 0.27), 0.0).to_path(0.1);
        out.fill(
            hull,
            body,
            Paint::Linear {
                start: Point::new(0.0, -h * 0.2),
                end: Point::new(0.0, h * 0.36),
                stops: stops(&[
                    (0.0, rgba(238, 236, 250, 1.0)),
                    (0.45, rgba(196, 190, 230, 1.0)),
                    (1.0, rgba(108, 100, 150, 1.0)),
                ]),
                extend: Extend::Pad,
            },
        );
        // La ceinture, plus sombre.
        let mut belt = BezPath::new();
        belt.move_to((-w * 0.5, h * 0.08));
        belt.quad_to((0.0, h * 0.3), (w * 0.5, h * 0.08));
        belt.quad_to((0.0, h * 0.42), (-w * 0.5, h * 0.08));
        belt.close_path();
        out.fill(belt, body, Paint::Solid(rgba(92, 84, 140, 0.55)));
        // Un liseré clair sur le dessus.
        let rim = Ellipse::new((0.0, h * 0.0), (w * 0.44, h * 0.06), 0.0).to_path(0.1);
        out.fill(rim, body, Paint::Solid(Color::WHITE.with_alpha(0.45)));
    })
}

/// Ajoute à `out` la soucoupe, son faisceau et ses lumières.
pub fn paint_saucer(view: &SaucerView, frame: &Frame, style: &Style, out: &mut DisplayList) {
    let center = Point::new(f64::from(view.pos.x), f64::from(view.pos.y));
    let world = frame.world_to_px();
    // Elle penche avec sa vitesse ; le faisceau, lui, tombe droit.
    let body = world * Affine::translate(center.to_vec2()) * Affine::rotate(f64::from(view.tilt));
    let at = world * Affine::translate(center.to_vec2());
    let beam = f64::from(view.beam.clamp(0.0, 1.0));
    let w = f64::from(SAUCER_WIDTH);
    let h = f64::from(SAUCER_HEIGHT);

    // Le faisceau : un cône de lumière pâle, avec des anneaux qui descendent.
    if beam > 0.01 {
        let len = f64::from(view.beam_len.clamp(20.0, BEAM_MAX));
        let top = beam_top();
        if let Some(cone) = beam_prop(style.dark) {
            let stretch = Affine::translate((0.0, top))
                * Affine::scale_non_uniform(1.0, len / f64::from(BEAM_MAX))
                * Affine::translate((0.0, -top));
            out.push(cone.draw(at * stretch, beam as f32));
        }
        for k in 0..3 {
            let phase = (f64::from(view.lights) * 0.8 + f64::from(k) / 3.0).fract();
            let y = top + phase * len;
            let half = w * (0.2 + 0.22 * phase);
            let fade = (1.0 - phase) * phase * 4.0;
            out.disc(
                Point::ZERO,
                1.0,
                at * Affine::translate((0.0, y))
                    * Affine::scale_non_uniform(half, 3.0 + 4.0 * phase),
                Paint::Solid(Color::WHITE.with_alpha((0.22 * beam * fade) as f32)),
            );
        }
    }

    // Lueur sous elle quand le faisceau s'allume.
    if beam > 0.01 {
        let c = Point::new(0.0, h * 0.25);
        out.disc(
            c,
            w * 0.45,
            body,
            Paint::Radial {
                start: c,
                start_radius: 0.0,
                end: c,
                end_radius: (w * 0.45) as f32,
                stops: stops(&[
                    (0.0, rgba(190, 255, 235, (0.5 * beam) as f32)),
                    (1.0, rgba(190, 255, 235, 0.0)),
                ]),
                extend: Extend::Pad,
            },
        );
    }

    if let Some(hull) = body_prop() {
        out.push(hull.draw(body, 1.0));
    }

    // Percutée, elle fume : des bouffées sombres qui montent, grossissent,
    // pâlissent et s'effacent (droites, quelle que soit son inclinaison).
    let smoke = f64::from(view.smoke.clamp(0.0, 1.0));
    if smoke > 0.01 {
        let t = f64::from(view.smoke_time);
        let (dark, light) = if style.dark {
            ((120.0, 120.0, 128.0), (205.0, 205.0, 212.0))
        } else {
            ((70.0, 68.0, 76.0), (170.0, 170.0, 178.0))
        };
        for k in 0..10 {
            let kf = f64::from(k);
            let phase = (t * 0.75 + kf / 10.0).fract();
            let x = -w * 0.1 + (t * 1.3 + kf * 1.7).sin() * 6.0 + (kf - 4.5) * 1.4 + phase * 14.0;
            let y = -h * 0.42 - phase * 78.0;
            let r = 6.0 + phase * 24.0;
            let mix = |a: f64, b: f64| (a + (b - a) * phase) as u8;
            let color = rgba(
                mix(dark.0, light.0),
                mix(dark.1, light.1),
                mix(dark.2, light.2),
                1.0,
            );
            let alpha = (smoke * (1.0 - phase) * (phase * 4.0).min(1.0) * 0.85) as f32;
            let c = Point::new(x, y);
            out.disc(
                c,
                r,
                at,
                Paint::Radial {
                    start: c,
                    start_radius: 0.0,
                    end: c,
                    end_radius: r as f32,
                    stops: stops(&[
                        (0.0, color.with_alpha(alpha)),
                        (0.6, color.with_alpha(alpha * 0.7)),
                        (1.0, color.with_alpha(0.0)),
                    ]),
                    extend: Extend::Pad,
                },
            );
        }
    }

    // Les lumières, qui clignotent l'une après l'autre (mal, si elle fume).
    let colors = [
        rgba(255, 212, 90, 1.0),
        rgba(255, 122, 182, 1.0),
        rgba(111, 230, 255, 1.0),
    ];
    for (i, color) in colors.iter().cycle().take(7).enumerate() {
        let k = i as f64;
        let u = k / 6.0 - 0.5;
        let x = u * w * 0.8;
        let y = h * 0.2 + (1.0 - (2.0 * u).powi(2)) * h * 0.08;
        let blink = (f64::from(view.lights) * std::f64::consts::TAU - k * 0.9).sin();
        let mut on = 0.45 + 0.55 * blink.max(0.0);
        if smoke > 0.01 {
            let flicker = (f64::from(view.lights) * 13.0 + k * 2.1).sin() > 0.35;
            on *= if flicker { 0.25 } else { 1.0 };
        }
        let color = *color;
        let c = Point::new(x, y);
        out.disc(
            c,
            7.0,
            body,
            Paint::Radial {
                start: c,
                start_radius: 0.0,
                end: c,
                end_radius: 7.0,
                stops: stops(&[
                    (0.0, color.with_alpha((0.55 * on) as f32)),
                    (1.0, color.with_alpha(0.0)),
                ]),
                extend: Extend::Pad,
            },
        );
        out.disc(
            c,
            2.6,
            body,
            Paint::Solid(color.with_alpha((0.6 + 0.4 * on) as f32)),
        );
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::display::Command;

    /// Ni tracé ni contour : rien à dessiner sur le processeur à chaque image.
    #[test]
    fn the_saucer_needs_no_vector_paths() {
        let canvas = saucer_canvas();
        for (beam, len, tilt, dark, smoke) in [
            (0.0, 0.0, 0.0, false, 0.0),
            (0.6, 120.0, 0.1, false, 0.0),
            (1.0, BEAM_MAX, -0.2, true, 0.0),
            (0.0, 0.0, 0.3, false, 1.0),
        ] {
            let view = SaucerView {
                pos: CoreVec2::new(400.0, 300.0),
                tilt,
                beam,
                beam_len: len,
                lights: 0.3,
                smoke,
                smoke_time: 1.2,
            };
            let frame = Frame {
                origin: CoreVec2::new(400.0 - canvas.anchor.x, 300.0 - canvas.anchor.y),
                scale: 2.0,
            };
            let mut out =
                DisplayList::new((canvas.width * 2.0) as u32, (canvas.height * 2.0) as u32);
            paint_saucer(&view, &frame, &Style { dark }, &mut out);
            assert!(
                out.commands
                    .iter()
                    .all(|c| !matches!(c, Command::Fill { .. } | Command::Stroke { .. })),
                "{:?}",
                out.commands
            );
            let images = out
                .commands
                .iter()
                .filter(|c| matches!(c, Command::Image { .. }))
                .count();
            assert_eq!(images, if beam > 0.0 { 2 } else { 1 });
            assert!(out.commands.len() < 64);
        }
    }
}
