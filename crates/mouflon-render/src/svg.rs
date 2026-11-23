//! Traduction de la liste d'affichage en SVG, pour l'outil d'aperçu et les tests.
//! Le résultat, rendu par resvg, est ce que montre l'écran.

use std::fmt::Write as _;

use base64::Engine as _;
use peniko::kurbo::{Affine, BezPath, Rect};
use peniko::{Color, Extend, Fill, Mix};

use crate::display::{Command, DisplayList, LineCap, LineJoin, Paint, Stop};
use crate::sprites::encode_png_rgba;

/// Écrit un document SVG complet.
#[must_use]
pub fn to_svg(list: &DisplayList) -> String {
    let mut w = Writer::default();
    for command in &list.commands {
        w.command(command);
    }
    while w.open_layers > 0 {
        w.body.push_str("</g>");
        w.open_layers -= 1;
    }
    format!(
        r#"<svg xmlns="http://www.w3.org/2000/svg" width="{0}" height="{1}" viewBox="0 0 {0} {1}"><defs>{2}</defs>{3}</svg>"#,
        list.width, list.height, w.defs, w.body
    )
}

#[derive(Default)]
struct Writer {
    defs: String,
    body: String,
    next: u32,
    open_layers: u32,
}

fn matrix(t: Affine) -> String {
    let [a, b, c, d, e, f] = t.as_coeffs();
    format!("matrix({a:.5} {b:.5} {c:.5} {d:.5} {e:.4} {f:.4})")
}

fn hex(c: Color) -> (String, f32) {
    let [r, g, b, a] = c.components;
    let byte = |v: f32| (v.clamp(0.0, 1.0) * 255.0).round() as u8;
    (
        format!("#{:02x}{:02x}{:02x}", byte(r), byte(g), byte(b)),
        a.clamp(0.0, 1.0),
    )
}

fn spread(extend: Extend) -> &'static str {
    match extend {
        Extend::Pad => "pad",
        Extend::Repeat => "repeat",
        Extend::Reflect => "reflect",
    }
}

fn blend(mix: Mix) -> &'static str {
    match mix {
        Mix::Multiply => "multiply",
        Mix::Screen => "screen",
        Mix::Overlay => "overlay",
        Mix::Darken => "darken",
        Mix::Lighten => "lighten",
        Mix::ColorDodge => "color-dodge",
        Mix::ColorBurn => "color-burn",
        Mix::HardLight => "hard-light",
        Mix::SoftLight => "soft-light",
        Mix::Difference => "difference",
        Mix::Exclusion => "exclusion",
        Mix::Hue => "hue",
        Mix::Saturation => "saturation",
        Mix::Color => "color",
        Mix::Luminosity => "luminosity",
        _ => "normal",
    }
}

impl Writer {
    fn id(&mut self, prefix: &str) -> String {
        self.next += 1;
        format!("{prefix}{}", self.next)
    }

    fn stops(&mut self, stops: &[Stop]) {
        for s in stops {
            let (color, alpha) = hex(s.color);
            let _ = write!(
                self.defs,
                r#"<stop offset="{:.4}" stop-color="{color}" stop-opacity="{alpha:.4}"/>"#,
                s.offset
            );
        }
    }

    /// Renvoie la valeur d'attribut de remplissage et l'opacité associée.
    fn paint(&mut self, paint: &Paint, brush_transform: Option<Affine>) -> (String, f32) {
        let gt = brush_transform
            .map(|t| format!(r#" gradientTransform="{}""#, matrix(t)))
            .unwrap_or_default();
        match paint {
            Paint::Solid(c) => hex(*c),
            Paint::Linear {
                start,
                end,
                stops,
                extend,
            } => {
                let id = self.id("g");
                let _ = write!(
                    self.defs,
                    r#"<linearGradient id="{id}" gradientUnits="userSpaceOnUse" x1="{:.4}" y1="{:.4}" x2="{:.4}" y2="{:.4}" spreadMethod="{}"{gt}>"#,
                    start.x,
                    start.y,
                    end.x,
                    end.y,
                    spread(*extend)
                );
                self.stops(stops);
                self.defs.push_str("</linearGradient>");
                (format!("url(#{id})"), 1.0)
            }
            Paint::Radial {
                start,
                start_radius,
                end,
                end_radius,
                stops,
                extend,
            } => {
                let id = self.id("g");
                let _ = write!(
                    self.defs,
                    r#"<radialGradient id="{id}" gradientUnits="userSpaceOnUse" cx="{:.4}" cy="{:.4}" r="{:.4}" fx="{:.4}" fy="{:.4}" fr="{:.4}" spreadMethod="{}"{gt}>"#,
                    end.x,
                    end.y,
                    end_radius,
                    start.x,
                    start.y,
                    start_radius,
                    spread(*extend)
                );
                self.stops(stops);
                self.defs.push_str("</radialGradient>");
                (format!("url(#{id})"), 1.0)
            }
        }
    }

    fn command(&mut self, command: &Command) {
        match command {
            Command::Fill {
                path,
                transform,
                paint,
                brush_transform,
                rule,
            } => {
                let (fill, opacity) = self.paint(paint, *brush_transform);
                let rule = if *rule == Fill::EvenOdd {
                    "evenodd"
                } else {
                    "nonzero"
                };
                let _ = write!(
                    self.body,
                    r#"<path d="{}" transform="{}" fill="{fill}" fill-opacity="{opacity:.4}" fill-rule="{rule}"/>"#,
                    path.to_svg(),
                    matrix(*transform)
                );
            }
            Command::Stroke {
                path,
                transform,
                paint,
                brush_transform,
                style,
            } => {
                let (stroke, opacity) = self.paint(paint, *brush_transform);
                let cap = match style.cap {
                    LineCap::Butt => "butt",
                    LineCap::Round => "round",
                    LineCap::Square => "square",
                };
                let join = match style.join {
                    LineJoin::Miter => "miter",
                    LineJoin::Round => "round",
                    LineJoin::Bevel => "bevel",
                };
                let _ = write!(
                    self.body,
                    r#"<path d="{}" transform="{}" fill="none" stroke="{stroke}" stroke-opacity="{opacity:.4}" stroke-width="{:.4}" stroke-linecap="{cap}" stroke-linejoin="{join}" stroke-miterlimit="{:.3}"/>"#,
                    path.to_svg(),
                    matrix(*transform),
                    style.width,
                    style.miter_limit
                );
            }
            Command::BlurredRect {
                rect,
                radius,
                std_dev,
                color,
                transform,
            } => {
                let id = self.id("f");
                let region: Rect = rect.inflate(std_dev * 3.0, std_dev * 3.0);
                let _ = write!(
                    self.defs,
                    r#"<filter id="{id}" filterUnits="userSpaceOnUse" x="{:.4}" y="{:.4}" width="{:.4}" height="{:.4}"><feGaussianBlur stdDeviation="{std_dev:.4}"/></filter>"#,
                    region.x0,
                    region.y0,
                    region.width(),
                    region.height()
                );
                let (fill, opacity) = hex(*color);
                let _ = write!(
                    self.body,
                    r#"<rect x="{:.4}" y="{:.4}" width="{:.4}" height="{:.4}" rx="{radius:.4}" fill="{fill}" fill-opacity="{opacity:.4}" filter="url(#{id})" transform="{}"/>"#,
                    rect.x0,
                    rect.y0,
                    rect.width(),
                    rect.height(),
                    matrix(*transform)
                );
            }
            Command::Image {
                bitmap,
                transform,
                alpha,
                smooth,
            } => {
                let png = base64::engine::general_purpose::STANDARD.encode(encode_png_rgba(bitmap));
                let rendering = if *smooth {
                    "optimizeQuality"
                } else {
                    "optimizeSpeed"
                };
                let _ = write!(
                    self.body,
                    r#"<image width="{}" height="{}" preserveAspectRatio="none" image-rendering="{rendering}" opacity="{alpha:.4}" transform="{}" href="data:image/png;base64,{png}"/>"#,
                    bitmap.width,
                    bitmap.height,
                    matrix(*transform)
                );
            }
            Command::Disc {
                center,
                radius,
                paint,
                transform,
            } => {
                let (fill, opacity) = self.paint(paint, None);
                let _ = write!(
                    self.body,
                    r#"<circle cx="{:.4}" cy="{:.4}" r="{radius:.4}" transform="{}" fill="{fill}" fill-opacity="{opacity:.4}"/>"#,
                    center.x,
                    center.y,
                    matrix(*transform)
                );
            }
            Command::PushLayer {
                clip,
                transform,
                alpha,
                mix,
            } => {
                let id = self.id("c");
                let _ = write!(
                    self.defs,
                    r#"<clipPath id="{id}" clipPathUnits="userSpaceOnUse"><path d="{}" transform="{}"/></clipPath>"#,
                    clip_path(clip),
                    matrix(*transform)
                );
                let _ = write!(
                    self.body,
                    r#"<g clip-path="url(#{id})" opacity="{alpha:.4}" style="mix-blend-mode:{}">"#,
                    blend(*mix)
                );
                self.open_layers += 1;
            }
            Command::PopLayer => {
                if self.open_layers > 0 {
                    self.body.push_str("</g>");
                    self.open_layers -= 1;
                }
            }
        }
    }
}

fn clip_path(path: &BezPath) -> String {
    path.to_svg()
}
