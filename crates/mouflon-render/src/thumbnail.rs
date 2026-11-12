//! Aperçus des animaux pour la galerie : quelques images de l'animal qui
//! marche (ou qui fait ce qu'il fait au sol), dessinées par le moteur comme
//! à l'écran, sur le processeur, et recadrées ensemble sur lui.

use std::sync::Arc;

use mouflon_core::def::Motion;
use mouflon_core::{Flock, Input, PetDef, Rect, Screen, Support, Vec2, World};

use crate::display::DisplayList;
use crate::paint::{Frame, PetArt, Style, paint_pet};
use crate::raster::rasterize;

/// Une image d'aperçu : pixels RGBA prémultipliés.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Thumbnail {
    pub width: u32,
    pub height: u32,
    pub rgba: Vec<u8>,
}

/// Le comportement montré : une marche si l'animal en a une, sinon ce qu'il
/// fait au sol d'habitude.
fn shown_behavior(def: &PetDef) -> usize {
    let named = |wanted: &[&str]| {
        def.behaviors
            .iter()
            .position(|b| wanted.iter().any(|w| b.name == *w))
    };
    def.behaviors
        .iter()
        .position(|b| matches!(b.motion, Motion::Walk { .. }))
        .or_else(|| named(&["walk", "marche"]))
        .unwrap_or_else(|| def.default_ground())
}

/// `count` images de l'animal, à `fps` images par seconde, à `scale` pixels
/// par point. Vide si rien ne se dessine.
#[must_use]
pub fn preview(
    def: &Arc<PetDef>,
    art: &PetArt,
    count: usize,
    fps: f32,
    scale: f64,
) -> Vec<Thumbnail> {
    // Un petit écran rien que pour lui : il y tombe, se pose, puis marche.
    let world = World::new(vec![Screen {
        frame: Rect::new(0.0, 0.0, 4000.0, 400.0),
        usable: Rect::new(0.0, 0.0, 4000.0, 400.0),
        scale: scale as f32,
    }]);
    let mut flock = Flock::new();
    let id = flock.spawn(Arc::clone(def), 7, &world, Some(2000.0));
    let input = Input {
        local_hour: 14.0,
        ..Input::default()
    };
    // Il tombe, se pose, et se remet de son atterrissage.
    let mut grounded = 0;
    for _ in 0..600 {
        flock.advance(1.0 / 60.0, &world, &input);
        let on_ground = flock
            .pets()
            .iter()
            .any(|p| matches!(p.support(), Support::Ground { .. }));
        grounded = if on_ground { grounded + 1 } else { 0 };
        if grounded > 30 {
            break;
        }
    }
    let behavior = def.behaviors[shown_behavior(def)].name.clone();
    let mut worn = None;
    if let Some(pet) = flock.get_mut(id) {
        pet.face(1.0);
        pet.force(&behavior);
        worn = pet.variant();
    }
    // Le dessin qu'il porte (celui de son sexe, s'il en a un).
    let art = art.worn(worn);
    let canvas = art.canvas();
    let (w, h) = (
        (f64::from(canvas.width) * scale).ceil() as u32,
        (f64::from(canvas.height) * scale).ceil() as u32,
    );
    let mut frames = Vec::new();
    for _ in 0..count {
        let Some(pet) = flock.pets().iter().find(|p| p.id() == id) else {
            break;
        };
        let view = pet.view(1.0);
        let mut list = DisplayList::new(w, h);
        let frame = Frame {
            origin: Vec2::new(
                view.ground.x - canvas.anchor.x,
                view.ground.y - canvas.anchor.y,
            ),
            scale,
        };
        paint_pet(art, &view, &frame, &Style::default(), &mut list);
        if let Some(pixmap) = rasterize(&list) {
            frames.push(pixmap);
        }
        flock.advance(1.0 / fps.max(1.0), &world, &input);
    }
    crop_together(&frames)
}

/// Recadre toutes les images sur ce qu'elles montrent (ensemble : l'animal
/// ne bouge pas d'une image à l'autre), avec une petite marge.
fn crop_together(frames: &[tiny_skia::Pixmap]) -> Vec<Thumbnail> {
    let (mut x0, mut y0, mut x1, mut y1) = (u32::MAX, u32::MAX, 0, 0);
    for pixmap in frames {
        let w = pixmap.width();
        for (i, px) in pixmap.pixels().iter().enumerate() {
            if px.alpha() > 8 {
                let (x, y) = (i as u32 % w, i as u32 / w);
                x0 = x0.min(x);
                y0 = y0.min(y);
                x1 = x1.max(x);
                y1 = y1.max(y);
            }
        }
    }
    if x0 > x1 || y0 > y1 {
        return Vec::new();
    }
    let margin = 4;
    frames
        .iter()
        .map(|pixmap| {
            let (fx0, fy0) = (x0.saturating_sub(margin), y0.saturating_sub(margin));
            let fx1 = (x1 + margin).min(pixmap.width() - 1);
            let fy1 = (y1 + margin).min(pixmap.height() - 1);
            let (width, height) = (fx1 - fx0 + 1, fy1 - fy0 + 1);
            let mut rgba = Vec::with_capacity((width * height * 4) as usize);
            let data = pixmap.data();
            for y in fy0..=fy1 {
                let start = ((y * pixmap.width() + fx0) * 4) as usize;
                rgba.extend_from_slice(&data[start..start + (width * 4) as usize]);
            }
            Thumbnail {
                width,
                height,
                rgba,
            }
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_bleu_du_maine_walks_in_its_preview() {
        let path = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("../../pets/bleu-du-maine/pet.toml");
        let def = Arc::new(mouflon_core::load_pet(&path).unwrap());
        let art = PetArt::load(&def).unwrap();
        let frames = preview(&def, &art, 6, 10.0, 1.0);
        assert_eq!(frames.len(), 6);
        let first = &frames[0];
        assert!(
            first.width > 30 && first.height > 30,
            "{}×{}",
            first.width,
            first.height
        );
        assert!(
            frames
                .iter()
                .all(|f| f.width == first.width && f.height == first.height)
        );
        // Il bouge d'une image à l'autre.
        assert!(frames.windows(2).any(|w| w[0].rgba != w[1].rgba));
        // De vrais pixels, opaques par endroits.
        assert!(first.rgba.chunks(4).any(|p| p[3] == 255));
    }
}
