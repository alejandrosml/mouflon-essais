//! Liste d'affichage indépendante du moteur graphique.
//!
//! Le rendu de l'animal produit une liste de commandes simples (formes,
//! dégradés, flous, calques). L'écran la reçoit en morceaux d'image (voir
//! [`crate::pieces`]), l'outil d'aperçu en SVG : une seule description,
//! deux sorties pareilles.

use std::borrow::Cow;

use peniko::kurbo::{Affine, BezPath, Point, Rect};
use peniko::{Blob, Color, Extend, Fill, Mix};

use crate::bc7;

#[derive(Clone, Debug, PartialEq)]
pub struct Stop {
    pub offset: f32,
    pub color: Color,
}

#[derive(Clone, Debug, PartialEq)]
pub enum Paint {
    Solid(Color),
    Linear {
        start: Point,
        end: Point,
        stops: Vec<Stop>,
        extend: Extend,
    },
    /// Dégradé radial à deux cercles (le cercle de départ joue le rôle du foyer).
    Radial {
        start: Point,
        start_radius: f32,
        end: Point,
        end_radius: f32,
        stops: Vec<Stop>,
        extend: Extend,
    },
}

impl Paint {
    /// Multiplie l'opacité de toutes les couleurs.
    #[must_use]
    pub fn with_alpha(mut self, alpha: f32) -> Self {
        let scale = |c: &mut Color| *c = c.multiply_alpha(alpha);
        match &mut self {
            Self::Solid(c) => scale(c),
            Self::Linear { stops, .. } | Self::Radial { stops, .. } => {
                stops.iter_mut().for_each(|s| scale(&mut s.color))
            }
        }
        self
    }
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub enum LineCap {
    Butt,
    Round,
    Square,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub enum LineJoin {
    Miter,
    Round,
    Bevel,
}

#[derive(Clone, Debug, PartialEq)]
pub struct StrokeStyle {
    pub width: f64,
    pub cap: LineCap,
    pub join: LineJoin,
    pub miter_limit: f64,
}

/// Pixels d'une image.
#[derive(Clone, Debug)]
pub enum Pixels {
    /// RGBA, alpha non prémultiplié, ligne par ligne.
    Rgba(Blob<u8>),
    /// Blocs BC7 (voir [`crate::bc7`]) de l'image prémultipliée, bordée d'un
    /// pixel transparent et complétée de transparent jusqu'à des côtés
    /// multiples de 4 (voir [`Bitmap::bc7_size`]) : 8 bits par pixel au lieu
    /// de 32, que le GPU lit tels quels.
    Bc7(Blob<u8>),
}

/// Une image, en RGBA ou compressée. Le `Blob` de ses pixels garde un
/// identifiant stable : elle n'est téléversée qu'une fois sur le GPU.
#[derive(Clone, Debug)]
pub struct Bitmap {
    pub width: u32,
    pub height: u32,
    pub pixels: Pixels,
}

impl PartialEq for Bitmap {
    fn eq(&self, other: &Self) -> bool {
        self.width == other.width && self.height == other.height && self.id() == other.id()
    }
}

impl Bitmap {
    /// Une image RGBA (alpha non prémultiplié).
    #[must_use]
    pub fn from_rgba(width: u32, height: u32, rgba: Vec<u8>) -> Self {
        Self {
            width,
            height,
            pixels: Pixels::Rgba(Blob::from(rgba)),
        }
    }

    /// Identifiant stable de ses pixels.
    #[must_use]
    pub fn id(&self) -> u64 {
        match &self.pixels {
            Pixels::Rgba(blob) | Pixels::Bc7(blob) => blob.id(),
        }
    }

    /// Octets qu'occupent ses pixels.
    #[must_use]
    pub fn byte_size(&self) -> usize {
        match &self.pixels {
            Pixels::Rgba(blob) | Pixels::Bc7(blob) => blob.len(),
        }
    }

    #[must_use]
    pub fn is_compressed(&self) -> bool {
        matches!(self.pixels, Pixels::Bc7(_))
    }

    /// Côtés d'une image BC7 de `width` × `height` pixels : l'image et sa
    /// bordure d'un pixel, complétées jusqu'au multiple de 4.
    #[must_use]
    pub fn bc7_size(width: u32, height: u32) -> (u32, u32) {
        ((width + 2).div_ceil(4) * 4, (height + 2).div_ceil(4) * 4)
    }

    /// Côtés de l'image qui part au GPU : bordée d'un pixel transparent
    /// (pour que ses bords tournés restent lisses), et complétée jusqu'au
    /// multiple de 4 si elle est compressée.
    #[must_use]
    pub fn texture_size(&self) -> (u32, u32) {
        match self.pixels {
            Pixels::Rgba(_) => (self.width + 2, self.height + 2),
            Pixels::Bc7(_) => Self::bc7_size(self.width, self.height),
        }
    }

    /// Ce qui part au GPU, en RGBA prémultiplié aux côtés de
    /// [`Self::texture_size`] (décompressé au besoin).
    #[must_use]
    pub fn texture_rgba(&self) -> Vec<u8> {
        let (tw, th) = self.texture_size();
        match &self.pixels {
            Pixels::Rgba(rgba) => {
                premultiplied_bordered(rgba.data(), self.width, self.height, (tw, th))
            }
            Pixels::Bc7(blocks) => bc7::decode(blocks.data(), tw as usize, th as usize),
        }
    }

    /// La même image compressée en BC7 (elle-même si elle l'est déjà).
    #[must_use]
    pub fn compressed(&self) -> Self {
        let Pixels::Rgba(rgba) = &self.pixels else {
            return self.clone();
        };
        let (tw, th) = Self::bc7_size(self.width, self.height);
        let padded = premultiplied_bordered(rgba.data(), self.width, self.height, (tw, th));
        Self {
            width: self.width,
            height: self.height,
            pixels: Pixels::Bc7(Blob::from(bc7::encode(&padded, tw as usize, th as usize))),
        }
    }

    /// Ses pixels en RGBA, alpha non prémultiplié (décompressés au besoin).
    #[must_use]
    pub fn to_rgba(&self) -> Cow<'_, [u8]> {
        let blocks = match &self.pixels {
            Pixels::Rgba(rgba) => return Cow::Borrowed(rgba.data()),
            Pixels::Bc7(blocks) => blocks,
        };
        let (tw, th) = self.texture_size();
        let padded = bc7::decode(blocks.data(), tw as usize, th as usize);
        let (w, h, tw) = (self.width as usize, self.height as usize, tw as usize);
        let mut out = Vec::with_capacity(w * h * 4);
        for y in 0..h {
            let row = &padded[((y + 1) * tw + 1) * 4..((y + 1) * tw + 1 + w) * 4];
            for &[r, g, b, a] in row.as_chunks::<4>().0 {
                let straight = |c: u8| {
                    if a == 0 {
                        0
                    } else {
                        ((u32::from(c) * 255 + u32::from(a) / 2) / u32::from(a)).min(255) as u8
                    }
                };
                out.extend_from_slice(&[straight(r), straight(g), straight(b), a]);
            }
        }
        Cow::Owned(out)
    }
}

/// RGBA `width` × `height` (alpha non prémultiplié) prémultiplié, posé à un
/// pixel du bord d'une image transparente de côtés `size`.
fn premultiplied_bordered(rgba: &[u8], width: u32, height: u32, size: (u32, u32)) -> Vec<u8> {
    let (w, h) = (width as usize, height as usize);
    let (tw, th) = (size.0 as usize, size.1 as usize);
    let mut out = vec![0u8; tw * th * 4];
    if rgba.len() < w * h * 4 || tw < w + 2 || th < h + 2 {
        return out;
    }
    for y in 0..h {
        let src = &rgba[y * w * 4..(y + 1) * w * 4];
        let dst = &mut out[((y + 1) * tw + 1) * 4..((y + 1) * tw + 1 + w) * 4];
        for (d, s) in dst
            .as_chunks_mut::<4>()
            .0
            .iter_mut()
            .zip(src.as_chunks::<4>().0)
        {
            let a = u16::from(s[3]);
            // (c · a + 127) / 255, arrondi.
            let mul = |c: u8| ((u16::from(c) * a + 127) / 255) as u8;
            *d = [mul(s[0]), mul(s[1]), mul(s[2]), s[3]];
        }
    }
    out
}

#[derive(Clone, Debug, PartialEq)]
pub enum Command {
    Fill {
        path: BezPath,
        transform: Affine,
        paint: Paint,
        brush_transform: Option<Affine>,
        rule: Fill,
    },
    Stroke {
        path: BezPath,
        transform: Affine,
        paint: Paint,
        brush_transform: Option<Affine>,
        style: StrokeStyle,
    },
    /// Rectangle arrondi flouté (ombres douces).
    BlurredRect {
        rect: Rect,
        radius: f64,
        std_dev: f64,
        color: Color,
        transform: Affine,
    },
    Image {
        bitmap: Bitmap,
        transform: Affine,
        alpha: f32,
        smooth: bool,
    },
    /// Disque plein (particules) : un cercle, plus simple à dessiner qu'un
    /// chemin quelconque.
    Disc {
        center: Point,
        radius: f64,
        paint: Paint,
        transform: Affine,
    },
    /// Calque : tout ce qui suit jusqu'au `PopLayer` est découpé par `clip`,
    /// puis composé avec l'opacité et le mode de fusion donnés.
    PushLayer {
        clip: BezPath,
        transform: Affine,
        alpha: f32,
        mix: Mix,
    },
    PopLayer,
}

/// Une image à dessiner : taille en pixels et commandes.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct DisplayList {
    pub width: u32,
    pub height: u32,
    pub commands: Vec<Command>,
}

impl DisplayList {
    #[must_use]
    pub fn new(width: u32, height: u32) -> Self {
        Self {
            width,
            height,
            commands: Vec::new(),
        }
    }

    pub fn clear(&mut self, width: u32, height: u32) {
        self.width = width;
        self.height = height;
        self.commands.clear();
    }

    #[must_use]
    pub fn bounds(&self) -> Rect {
        Rect::new(0.0, 0.0, f64::from(self.width), f64::from(self.height))
    }

    pub fn fill(&mut self, path: BezPath, transform: Affine, paint: Paint) {
        self.commands.push(Command::Fill {
            path,
            transform,
            paint,
            brush_transform: None,
            rule: Fill::NonZero,
        });
    }

    pub fn disc(&mut self, center: Point, radius: f64, transform: Affine, paint: Paint) {
        self.commands.push(Command::Disc {
            center,
            radius,
            paint,
            transform,
        });
    }

    pub fn push(&mut self, command: Command) {
        self.commands.push(command);
    }

    pub fn push_layer(&mut self, clip: BezPath, transform: Affine, alpha: f32, mix: Mix) {
        self.commands.push(Command::PushLayer {
            clip,
            transform,
            alpha,
            mix,
        });
    }

    pub fn pop_layer(&mut self) {
        self.commands.push(Command::PopLayer);
    }
}

/// Couleur sRGB depuis des octets.
#[must_use]
pub fn rgba(r: u8, g: u8, b: u8, a: f32) -> Color {
    Color::new([
        f32::from(r) / 255.0,
        f32::from(g) / 255.0,
        f32::from(b) / 255.0,
        a,
    ])
}
