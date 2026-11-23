//! Moteur de comportement de Mouflon.
//!
//! Ce crate ne touche à aucune API système. Il simule les animaux dans un
//! repère « bureau » abstrait (points logiques, origine en haut à gauche de
//! l'écran principal, y vers le bas) et fournit à chaque image une
//! [`PetView`] que le rendu transforme en pixels.
//!
//! - [`world`] : écrans, sols et rebords de fenêtres visibles ;
//! - [`def`] : définition d'un animal (comportements, physique, apparence) ;
//! - [`format`] : format natif `pet.toml` ;
//! - [`desktoppet`] : import des animaux desktopPet (eSheep 64) ;
//! - [`pet`] : simulation à pas fixe et signaux d'animation.
#![forbid(unsafe_code)]

pub mod def;
pub mod desktoppet;
pub mod expr;
pub mod format;
mod layout;
pub mod math;
pub mod pet;
pub mod physics;
pub mod show;
pub mod skeleton;
pub mod surprise;
pub mod temper;
pub mod world;

pub use def::{
    Age, Appearance, CATEGORIES, Drawn, FLEECE_LENGTH, Facings, Gesture, Inbetweens, LAMB_WEEKS,
    PetDef, PetMeta, PngData, Pose, REGIONS, Rules, Sex, SexRatio, Sound, SoundCue, SpritePage,
    SpriteSheet, Variant, VariantDrawing,
};
pub use format::{LoadError, Resources, load_pet, parse_pet_toml};
pub use math::{Rect, Vec2};
pub use pet::{
    AWAY_AFTER, Activity, CLOCK_LOOP, Flock, Input, LegCycle, Particle, ParticleKind, Pet,
    PetEvent, PetView, PoseSignals, RUN_STRIDE, SLOW_RATE, STARTLE_SPEED, Shadow, Support, TICK,
    TYPING_WINDOW, VOYAGE_EVERY, VoyageWay,
};
pub use skeleton::{
    Bone, BonePose, BoneRole, Clip, Key, Motions, Piece, Shown, Skeleton, SkeletonError, Track,
    parse_skeleton,
};
pub use surprise::{BEAM_MAX, Gust, SAUCER_HEIGHT, SAUCER_WIDTH, Saucer, SaucerView};
pub use temper::{Energy, Mood, Temper};
pub use world::{
    DOCK_ID, IdHasher, LedgeOptions, Screen, Surface, SurfaceKind, Wall, WindowRect, World,
    exact_dock,
};
