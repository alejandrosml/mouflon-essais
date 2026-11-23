//! Dessin sur le processeur d'une liste de commandes (tiny-skia) : pour les
//! petites images qui ne passent pas par le GPU, comme les aperçus de la
//! galerie des animaux.
//!
//! Le rendu est celui du GPU, aux détails près : les ombres floutées y sont
//! des formes adoucies, les modes de fusion des calques y sont ignorés.

use peniko::kurbo::{Affine, Circle, Ellipse, Point, Shape};
use peniko::{Extend, Fill};
use tiny_skia::{FillRule, Mask, Pixmap, PixmapPaint, Transform};

use crate::bake::{to_path, to_shader, to_stroke, to_transform};
use crate::display::{Command, DisplayList, Paint, Stop};

/// Un calque ouvert : ce qui était dessiné dessous, son opacité et sa découpe.
struct Layer {
    below: Pixmap,
    alpha: f32,
    clip: Option<Mask>,
}

/// Dessine la liste ; pixels RGBA prémultipliés, aux côtés de la liste.
#[must_use]
pub fn rasterize(list: &DisplayList) -> Option<Pixmap> {
    let (width, height) = (list.width.max(1), list.height.max(1));
    let mut pixmap = Pixmap::new(width, height)?;
    let mut layers: Vec<Layer> = Vec::new();
    for command in &list.commands {
        match command {
            Command::Fill {
                path,
                transform,
                paint,
                brush_transform,
                rule,
            } => {
                fill(
                    &mut pixmap,
                    path,
                    *transform,
                    paint,
                    brush_transform.unwrap_or(Affine::IDENTITY),
                    *rule,
                );
            }
            Command::Stroke {
                path,
                transform,
                paint,
                brush_transform,
                style,
            } => {
                let (Some(path), Some(shader)) = (
                    to_path(path),
                    to_shader(paint, brush_transform.unwrap_or(Affine::IDENTITY)),
                ) else {
                    continue;
                };
                let paint = tiny_skia::Paint {
                    shader,
                    anti_alias: true,
                    ..tiny_skia::Paint::default()
                };
                pixmap.stroke_path(
                    &path,
                    &paint,
                    &to_stroke(style),
                    to_transform(*transform),
                    None,
                );
            }
            Command::BlurredRect {
                rect,
                std_dev,
                color,
                transform,
                ..
            } => {
                // Une ombre douce : une ellipse qui s'efface vers ses bords,
                // de la taille du rectangle flouté.
                let center = rect.center();
                let (rx, ry) = (
                    rect.width() * 0.5 + 2.0 * std_dev,
                    rect.height() * 0.5 + 2.0 * std_dev,
                );
                let shape = Ellipse::new(center, (rx, ry), 0.0).to_path(0.1);
                let origin = Point::ZERO;
                let paint = Paint::Radial {
                    start: origin,
                    start_radius: 0.0,
                    end: origin,
                    end_radius: 1.0,
                    stops: vec![
                        Stop {
                            offset: 0.0,
                            color: *color,
                        },
                        Stop {
                            offset: 0.45,
                            color: color.multiply_alpha(0.8),
                        },
                        Stop {
                            offset: 1.0,
                            color: color.multiply_alpha(0.0),
                        },
                    ],
                    extend: Extend::Pad,
                };
                let brush = Affine::translate(center.to_vec2()) * Affine::scale_non_uniform(rx, ry);
                fill(
                    &mut pixmap,
                    &shape,
                    *transform,
                    &paint,
                    brush,
                    Fill::NonZero,
                );
            }
            Command::Image {
                bitmap,
                transform,
                alpha,
                smooth,
            } => {
                let (tw, th) = bitmap.texture_size();
                let Some(size) = tiny_skia::IntSize::from_wh(tw, th) else {
                    continue;
                };
                let Some(image) = Pixmap::from_vec(bitmap.texture_rgba(), size) else {
                    continue;
                };
                // L'image part au GPU bordée d'un pixel : son coin est en (1, 1).
                let at = *transform * Affine::translate((-1.0, -1.0));
                let paint = PixmapPaint {
                    opacity: alpha.clamp(0.0, 1.0),
                    quality: if *smooth {
                        tiny_skia::FilterQuality::Bilinear
                    } else {
                        tiny_skia::FilterQuality::Nearest
                    },
                    ..PixmapPaint::default()
                };
                pixmap.draw_pixmap(0, 0, image.as_ref(), &paint, to_transform(at), None);
            }
            Command::Disc {
                center,
                radius,
                paint,
                transform,
            } => {
                let shape = Circle::new(*center, *radius).to_path(0.1);
                fill(
                    &mut pixmap,
                    &shape,
                    *transform,
                    paint,
                    Affine::IDENTITY,
                    Fill::NonZero,
                );
            }
            Command::PushLayer {
                clip,
                transform,
                alpha,
                ..
            } => {
                let mask = to_path(clip).and_then(|path| {
                    let mut mask = Mask::new(width, height)?;
                    mask.fill_path(&path, FillRule::Winding, true, to_transform(*transform));
                    Some(mask)
                });
                let fresh = Pixmap::new(width, height)?;
                layers.push(Layer {
                    below: std::mem::replace(&mut pixmap, fresh),
                    alpha: *alpha,
                    clip: mask,
                });
            }
            Command::PopLayer => {
                let Some(layer) = layers.pop() else {
                    continue;
                };
                let above = std::mem::replace(&mut pixmap, layer.below);
                let paint = PixmapPaint {
                    opacity: layer.alpha.clamp(0.0, 1.0),
                    ..PixmapPaint::default()
                };
                pixmap.draw_pixmap(
                    0,
                    0,
                    above.as_ref(),
                    &paint,
                    Transform::identity(),
                    layer.clip.as_ref(),
                );
            }
        }
    }
    // Des calques restés ouverts : refermés.
    while let Some(layer) = layers.pop() {
        let above = std::mem::replace(&mut pixmap, layer.below);
        let paint = PixmapPaint {
            opacity: layer.alpha.clamp(0.0, 1.0),
            ..PixmapPaint::default()
        };
        pixmap.draw_pixmap(
            0,
            0,
            above.as_ref(),
            &paint,
            Transform::identity(),
            layer.clip.as_ref(),
        );
    }
    Some(pixmap)
}

fn fill(
    pixmap: &mut Pixmap,
    path: &peniko::kurbo::BezPath,
    transform: Affine,
    paint: &Paint,
    brush: Affine,
    rule: Fill,
) {
    let (Some(path), Some(shader)) = (to_path(path), to_shader(paint, brush)) else {
        return;
    };
    let paint = tiny_skia::Paint {
        shader,
        anti_alias: true,
        ..tiny_skia::Paint::default()
    };
    let rule = match rule {
        Fill::NonZero => FillRule::Winding,
        Fill::EvenOdd => FillRule::EvenOdd,
    };
    pixmap.fill_path(&path, &paint, rule, to_transform(transform), None);
}

#[cfg(test)]
mod tests {
    use peniko::Color;
    use peniko::kurbo::Rect;

    use super::*;

    #[test]
    fn fills_discs_and_layers_land_where_they_should() {
        let mut list = DisplayList::new(40, 20);
        list.fill(
            Rect::new(0.0, 0.0, 10.0, 10.0).to_path(0.1),
            Affine::IDENTITY,
            Paint::Solid(Color::from_rgba8(255, 0, 0, 255)),
        );
        list.push_layer(
            Rect::new(20.0, 0.0, 40.0, 20.0).to_path(0.1),
            Affine::IDENTITY,
            0.5,
            peniko::Mix::Normal,
        );
        list.disc(
            Point::new(30.0, 10.0),
            6.0,
            Affine::IDENTITY,
            Paint::Solid(Color::from_rgba8(0, 0, 255, 255)),
        );
        list.pop_layer();
        let pixmap = rasterize(&list).unwrap();
        let at = |x: u32, y: u32| pixmap.pixel(x, y).unwrap();
        assert_eq!(at(5, 5).red(), 255);
        assert_eq!(at(5, 5).alpha(), 255);
        // Le disque, à moitié transparent par son calque.
        let blue = at(30, 10);
        assert!((blue.alpha() as i32 - 128).abs() <= 2, "{blue:?}");
        assert_eq!(at(15, 15).alpha(), 0);
    }
}
