//! Petits dessins du décor (les feuilles d'un coup de vent, la soucoupe
//! volante), cuits une fois pour toutes en images. Le rendu direct les pose
//! comme les images d'un animal, tournées et étirées : aucun tracé à refaire
//! à chaque image (Vello, autrefois, préparait pour eux une vingtaine de
//! programmes du GPU, plus d'une seconde sur certains Mac, puis refaisait
//! une dizaine de passes de calcul par fenêtre et par image).

use std::sync::OnceLock;

use peniko::kurbo::{Affine, BezPath, Point, Rect};
use peniko::{Color, Extend};

use crate::bake::to_bitmap;
use crate::display::{Bitmap, Command, DisplayList, LineCap, LineJoin, Paint, Stop, StrokeStyle};
use crate::raster::rasterize;

/// Un dessin cuit, et où il se pose : `placement` va du repère de l'image
/// (pixels) vers celui du dessin.
#[derive(Clone, Debug)]
pub struct Prop {
    pub bitmap: Bitmap,
    pub placement: Affine,
}

impl Prop {
    /// La commande qui le pose, `transform` allant du repère du dessin vers
    /// les pixels de la fenêtre.
    #[must_use]
    pub fn draw(&self, transform: Affine, alpha: f32) -> Command {
        Command::Image {
            bitmap: self.bitmap.clone(),
            transform: transform * self.placement,
            alpha,
            smooth: true,
        }
    }
}

/// Cuit ce que `draw` dessine dans `bounds` (repère du dessin), à
/// `pixels_per_unit` pixels par unité, bordé d'un pixel transparent (les
/// bords restent lisses une fois l'image tournée). `draw` reçoit la liste et
/// la transformation du repère du dessin vers les pixels de l'image.
pub(crate) fn bake(
    bounds: Rect,
    pixels_per_unit: f64,
    draw: impl FnOnce(&mut DisplayList, Affine),
) -> Option<Prop> {
    let w = (bounds.width() * pixels_per_unit).ceil() + 2.0;
    let h = (bounds.height() * pixels_per_unit).ceil() + 2.0;
    if !(w.is_finite() && h.is_finite()) || w > 4096.0 || h > 4096.0 {
        return None;
    }
    let to_px = Affine::translate((1.0, 1.0))
        * Affine::scale(pixels_per_unit)
        * Affine::translate((-bounds.x0, -bounds.y0));
    let mut list = DisplayList::new(w as u32, h as u32);
    draw(&mut list, to_px);
    let pixmap = rasterize(&list)?;
    Some(Prop {
        bitmap: to_bitmap(&pixmap),
        placement: to_px.inverse(),
    })
}

/// Une couleur plus claire (`k` > 1) ou plus sombre (`k` < 1).
#[must_use]
pub(crate) fn shade(color: Color, k: f32) -> Color {
    let [r, g, b, a] = color.components;
    Color::new([
        (r * k).clamp(0.0, 1.0),
        (g * k).clamp(0.0, 1.0),
        (b * k).clamp(0.0, 1.0),
        a,
    ])
}

/// Teintes des feuilles d'automne.
pub const LEAF_TONES: [Color; 4] = [
    Color::from_rgba8(222, 132, 52, 255),
    Color::from_rgba8(196, 162, 58, 255),
    Color::from_rgba8(176, 82, 52, 255),
    Color::from_rgba8(132, 160, 72, 255),
];

/// Pixels d'image par unité d'une feuille (une feuille de taille 1 fait deux
/// unités de long) : de quoi rester nette jusqu'à une dizaine de points de
/// long, sur un écran Retina.
const LEAF_PIXELS: f64 = 20.0;

/// La feuille de teinte `tone` (rang dans [`LEAF_TONES`], en boucle), dans
/// un repère où elle va de -1 à 1 en longueur, la pointe vers la droite.
#[must_use]
pub fn leaf(tone: usize) -> Option<&'static Prop> {
    static LEAVES: OnceLock<Vec<Option<Prop>>> = OnceLock::new();
    LEAVES
        .get_or_init(|| LEAF_TONES.iter().map(|&tone| bake_leaf(tone)).collect())
        .get(tone % LEAF_TONES.len())?
        .as_ref()
}

fn bake_leaf(tone: Color) -> Option<Prop> {
    let bounds = Rect::new(-1.4, -0.5, 1.05, 0.5);
    bake(bounds, LEAF_PIXELS, |out, at| {
        // La queue, fine et un peu courbée.
        let mut stem = BezPath::new();
        stem.move_to((-0.95, 0.0));
        stem.quad_to((-1.15, 0.02), (-1.32, 0.14));
        out.push(Command::Stroke {
            path: stem,
            transform: at,
            paint: Paint::Solid(shade(tone, 0.62)),
            brush_transform: None,
            style: StrokeStyle {
                width: 0.08,
                cap: LineCap::Round,
                join: LineJoin::Round,
                miter_limit: 4.0,
            },
        });
        // Le limbe, éclairé d'en haut.
        let mut blade = BezPath::new();
        blade.move_to((-1.0, 0.0));
        blade.quad_to((-0.1, -0.86), (1.0, 0.0));
        blade.quad_to((-0.1, 0.74), (-1.0, 0.0));
        blade.close_path();
        out.fill(
            blade,
            at,
            Paint::Linear {
                start: Point::new(-0.4, -0.45),
                end: Point::new(0.4, 0.45),
                stops: vec![
                    Stop {
                        offset: 0.0,
                        color: shade(tone, 1.18),
                    },
                    Stop {
                        offset: 1.0,
                        color: shade(tone, 0.82),
                    },
                ],
                extend: Extend::Pad,
            },
        );
        // La nervure.
        let mut vein = BezPath::new();
        vein.move_to((-0.95, 0.0));
        vein.quad_to((0.0, -0.05), (0.82, -0.01));
        out.push(Command::Stroke {
            path: vein,
            transform: at,
            paint: Paint::Solid(shade(tone, 0.6).with_alpha(0.6)),
            brush_transform: None,
            style: StrokeStyle {
                width: 0.06,
                cap: LineCap::Round,
                join: LineJoin::Round,
                miter_limit: 4.0,
            },
        });
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn leaves_are_baked_once_with_their_tones() {
        let a = leaf(0).expect("feuille");
        let again = leaf(LEAF_TONES.len()).expect("feuille");
        assert_eq!(a.bitmap.id(), again.bitmap.id(), "cuite une seule fois");
        assert_ne!(a.bitmap.id(), leaf(1).unwrap().bitmap.id());
        // Bordée de transparent, opaque au milieu.
        let (w, h) = (a.bitmap.width, a.bitmap.height);
        assert!(w > 40 && h > 15, "{w}×{h}");
        let crate::display::Pixels::Rgba(blob) = &a.bitmap.pixels else {
            panic!("RGBA attendu");
        };
        let px = |x: u32, y: u32| blob.data()[((y * w + x) * 4 + 3) as usize];
        assert_eq!(px(0, 0), 0);
        // Le milieu de la feuille (x = 0, y = 0 dans son repère).
        let mid = a.placement.inverse() * Point::ZERO;
        assert!(px(mid.x as u32, mid.y as u32) > 200);
    }
}
