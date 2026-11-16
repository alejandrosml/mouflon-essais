//! Rendu de Mouflon.
//!
//! - [`rig`] : lecture d'un dessin SVG découpé en calques nommés ;
//! - [`animate`] : animation procédurale des calques à partir des signaux du moteur ;
//! - [`paint`] : dessin complet d'un animal (ombre, lumière, particules) ;
//! - [`display`] : liste d'affichage intermédiaire ;
//! - [`pieces`] : la même liste, toute en morceaux d'image (calques, quadrilatères) ;
//! - [`bc7`] : compression des images des animaux pour le GPU ;
//! - [`cache`] : images des animaux déjà prêtes, gardées sur le disque ;
//! - [`raster`] : dessin d'une liste sur le processeur (tiny-skia) ;
//! - [`svg`] : traduction en SVG (aperçus).
//!
//! Tout ce qui se montre à l'écran devient des morceaux d'image ([`pieces`]) :
//! les tracés, les ombres et les disques ne servent qu'à les préparer.
#![forbid(unsafe_code)]

pub mod animate;
pub mod bake;
pub mod bc7;
pub mod cache;
pub mod display;
mod gesture;
mod joints;
mod neck_floor;
pub mod notice;
pub mod paint;
mod pendant;
pub mod pieces;
pub mod props;
pub mod raster;
pub mod rig;
pub mod saucer;
pub mod skeleton;
pub mod sprites;
pub mod svg;
pub mod thumbnail;

pub use display::DisplayList;
pub use paint::{Canvas, Frame, Look, PetArt, SpriteKey, SpriteMotion, Style, paint_pet};
pub use peniko;
pub use peniko::kurbo;
pub use rig::{Part, Rig};
