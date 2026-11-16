//! Dessin d'un animal dans sa fenêtre : ombre, personnage, lumière, particules.

use std::sync::Arc;

use mouflon_core::{
    Appearance, Drawn, Facings, Gesture, Particle, ParticleKind, Pet, PetDef, PetView, PoseSignals,
    Shadow, Support, Variant, Vec2 as CoreVec2,
};
use peniko::kurbo::{Affine, BezPath, Point, Rect, Shape};
use peniko::{Color, Extend, Fill, Mix};

use crate::animate::{PoseTransforms, pose};
use crate::bake::{BakedRig, Step};
use crate::cache;
use crate::display::{Command, DisplayList, Paint, Stop, rgba};
use crate::rig::{Item, Part, Rig, RigError};
use crate::skeleton::{SkeletonArt, SkeletonArtError, paint_skeleton};
use crate::sprites::{SpriteError, SpriteFrames};

#[derive(Debug, thiserror::Error)]
pub enum ArtError {
    #[error(transparent)]
    Rig(#[from] RigError),
    #[error(transparent)]
    Sprites(#[from] SpriteError),
    #[error(transparent)]
    Skeleton(#[from] SkeletonArtError),
}

/// Apparence prête à dessiner.
#[derive(Clone, Debug)]
pub enum Look {
    Rig(Arc<Rig>),
    Sprites(Arc<SpriteFrames>),
    Skeleton(Arc<SkeletonArt>),
}

#[derive(Clone, Debug)]
pub struct PetArt {
    pub look: Look,
    /// Taille affichée du dessin (une case pour des sprites), en points.
    pub width: f32,
    pub height: f32,
    /// Intensité de l'éclairage dynamique sur le corps (0 = aucun).
    pub lighting: f32,
    /// Corps de l'animal, en points : demi-largeur et hauteur au-dessus du
    /// point d'appui (le curseur l'attrape là, les clics passent ailleurs).
    pub body: (f32, f32),
    /// Effets déjà dessinés dans les images.
    pub drawn: Drawn,
    /// Dessin vectoriel cuit en images (voir [`crate::bake`]) : le rendu
    /// direct le pose morceau par morceau. `None` : tracés dessinés tels quels.
    pub baked: Option<Arc<BakedRig>>,
    /// Distance parcourue pendant un cycle de marche (points) : les pattes
    /// d'un dessin vectoriel balaient juste ce qu'il faut pour ne pas glisser.
    pub stride: f32,
    /// Clé des planches au moment où elles ont été lues (voir
    /// [`cache::key`]) : les images gardées sur le disque portent celle des
    /// planches dont elles viennent, même si un fichier change entre-temps.
    sheet_key: Option<u64>,
    /// Images reprises telles quelles du disque.
    from_cache: bool,
    /// Ses autres dessins, par sexe et par âge, prêts eux aussi : ceux que
    /// peuvent porter les animaux de sa définition (voir
    /// [`PetDef::worn_variants`] et [`PetArt::worn`]).
    pub variants: Vec<(Variant, Arc<PetArt>)>,
}

/// Fenêtre d'un animal : taille en points et position du point d'appui.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Canvas {
    pub width: f32,
    pub height: f32,
    pub anchor: CoreVec2,
}

impl PetArt {
    pub fn load(def: &PetDef) -> Result<Self, ArtError> {
        Self::load_cached(def, None)
    }

    /// Comme [`Self::load`], en reprenant d'abord les images déjà prêtes
    /// gardées dans `cache` (voir [`crate::cache`]). Les dessins par sexe et
    /// par âge que ses animaux peuvent porter se préparent avec lui.
    pub fn load_cached(def: &PetDef, cache: Option<&cache::Store>) -> Result<Self, ArtError> {
        let mut art = Self::load_drawing(def, cache)?;
        for (variant, drawn) in def.worn_variants() {
            art.variants
                .push((variant, Arc::new(Self::load_drawing(drawn, cache)?)));
        }
        Ok(art)
    }

    /// Le dessin qu'il porte : celui de son sexe et de son âge
    /// ([`mouflon_core::Pet::variant`]) s'il est prêt, sinon celui de base.
    #[must_use]
    pub fn worn(&self, variant: Option<Variant>) -> &PetArt {
        variant
            .and_then(|v| self.variants.iter().find(|(w, _)| *w == v))
            .map_or(self, |(_, art)| art)
    }

    /// Comme [`PetArt::worn`], pour garder le dessin d'un animal : le sien,
    /// partagé.
    #[must_use]
    pub fn worn_by(art: &Arc<PetArt>, pet: &Pet) -> Arc<PetArt> {
        pet.variant()
            .and_then(|v| art.variants.iter().find(|(w, _)| *w == v))
            .map_or_else(|| Arc::clone(art), |(_, worn)| Arc::clone(worn))
    }

    /// Un seul dessin : celui de base, sans les autres.
    fn load_drawing(def: &PetDef, cache: Option<&cache::Store>) -> Result<Self, ArtError> {
        match &def.appearance {
            Appearance::Rig { svg, height } => {
                let rig = Rig::parse(svg)?;
                let k = f64::from(*height) / rig.bounds.height().max(1.0);
                let width = (rig.bounds.width() * k) as f32;
                let lighting = 0.6;
                let baked = BakedRig::bake(&rig, k, lighting).map(Arc::new);
                Ok(Self {
                    width,
                    height: *height,
                    look: Look::Rig(Arc::new(rig)),
                    lighting,
                    body: (width * 0.46, *height * 1.02),
                    drawn: Drawn::default(),
                    baked,
                    stride: def.stride(),
                    sheet_key: None,
                    from_cache: false,
                    variants: Vec::new(),
                })
            }
            Appearance::Skeleton { skeleton, height } => {
                let art = SkeletonArt::load(Arc::clone(skeleton), *height)?;
                let k = f64::from(*height) / art.bounds.height().max(1.0);
                let width = (art.bounds.width() * k) as f32;
                Ok(Self {
                    width,
                    height: *height,
                    look: Look::Skeleton(Arc::new(art)),
                    lighting: 0.0,
                    body: (width * 0.46, *height * 1.02),
                    drawn: Drawn::default(),
                    baked: None,
                    stride: def.stride(),
                    sheet_key: None,
                    from_cache: false,
                    variants: Vec::new(),
                })
            }
            Appearance::Sprites(sheet) => {
                let sheet_key = cache.and_then(|_| cache::key(sheet));
                let kept = cache.zip(sheet_key).and_then(|(store, key)| {
                    let path = cache::path(&store.dir, &def.meta.id, sheet);
                    cache::load(store, &path, key, sheet)
                });
                let from_cache = kept.is_some();
                let frames = match kept {
                    Some(frames) => frames,
                    None => SpriteFrames::decode(sheet)?,
                };
                let p = def.physics;
                Ok(Self {
                    width: frames.tile_width as f32 * sheet.scale,
                    height: frames.tile_height as f32 * sheet.scale,
                    look: Look::Sprites(Arc::new(frames)),
                    lighting: 0.0,
                    body: (p.width * 0.46, p.height * 1.02),
                    drawn: sheet.drawn,
                    baked: None,
                    stride: def.stride(),
                    sheet_key,
                    from_cache,
                    variants: Vec::new(),
                })
            }
        }
    }

    /// Images reprises telles quelles du disque (voir [`crate::cache`]).
    #[must_use]
    pub fn from_cache(&self) -> bool {
        self.from_cache
    }

    /// Des images restent-elles à compresser (voir
    /// [`SpriteFrames::compressible`]) ?
    #[must_use]
    pub fn compressible(&self) -> bool {
        matches!(&self.look, Look::Sprites(frames) if frames.compressible())
    }

    /// Octets qu'occupent les images d'un animal en images (ses autres
    /// dessins compris).
    #[must_use]
    pub fn image_bytes(&self) -> usize {
        let own = match &self.look {
            Look::Sprites(frames) => frames.byte_size(),
            Look::Skeleton(art) => art.byte_size(),
            Look::Rig(_) => 0,
        };
        own + self
            .variants
            .iter()
            .map(|(_, art)| art.image_bytes())
            .sum::<usize>()
    }

    /// Le même dessin, images compressées en BC7 sur `threads` fils (voir
    /// [`SpriteFrames::compressed`]) ; `None` s'il n'y a rien à compresser.
    #[must_use]
    pub fn compressed(&self, threads: usize, setup: &(dyn Fn() + Sync)) -> Option<Self> {
        let Look::Sprites(frames) = &self.look else {
            return None;
        };
        if !frames.compressible() {
            return None;
        }
        Some(Self {
            look: Look::Sprites(Arc::new(frames.compressed(threads, setup))),
            ..self.clone()
        })
    }

    /// Garde les images dans `cache` pour les prochains lancements.
    pub fn keep(&self, def: &PetDef, cache: &cache::Store) -> std::io::Result<()> {
        let (Look::Sprites(frames), Appearance::Sprites(sheet), Some(key)) =
            (&self.look, &def.appearance, self.sheet_key)
        else {
            return Ok(());
        };
        cache::save(&cache::path(&cache.dir, &def.meta.id, sheet), key, frames)
    }

    /// Hauteur du haut de chaque image au-dessus du point d'appui (points),
    /// pour un animal en images (voir [`SpriteFrames::tops`]) ; un dessin
    /// animé par le moteur suit sa pose.
    #[must_use]
    pub fn tops(&self) -> Option<Arc<[f32]>> {
        match &self.look {
            Look::Sprites(frames) => Some(Arc::from(frames.tops())),
            Look::Rig(_) | Look::Skeleton(_) => None,
        }
    }

    /// Place du dessin devant et derrière son point d'appui (points), pour
    /// un animal en images ; un dessin vectoriel suit sa largeur physique.
    #[must_use]
    pub fn reach(&self) -> Option<(f32, f32)> {
        match &self.look {
            Look::Sprites(frames) => frames.reach(),
            Look::Rig(_) | Look::Skeleton(_) => None,
        }
    }

    /// Sous le flanc de la mère, puis portée du museau de l'agneau qui tète.
    /// Coordonnées relatives au point d'appui, en fractions de hauteur du
    /// dessin (x vers la tête, y vers le bas). L'app les transmet au cœur
    /// une fois chargé ; aucune lecture d'image pendant la simulation.
    /// La mamelle est un repère implicite sous l'arrière du ventre, pas un
    /// détail ajouté au dessin. Les planches de sprites n'ont pas ce geste.
    #[must_use]
    pub fn nursing_points(&self) -> Option<(CoreVec2, CoreVec2)> {
        let (ground, height, udder, muzzle) = match &self.look {
            Look::Rig(rig) => {
                if !rig.has_part(Part::Body) || !rig.has_part(Part::Head) {
                    return None;
                }
                let body = rig.body_extent();
                let h = rig.bounds.height().max(1.0);
                let hip = if rig.has_part(Part::LegBackNear) {
                    rig.pivot(Part::LegBackNear).x
                } else {
                    body.min_x() + body.width() * 0.2
                };
                let udder = crate::joints::udder_point(body, hip, rig.ground, h, rig.joints.neck);
                let muzzle = rig
                    .muzzle
                    .unwrap_or_else(|| crate::animate::muzzle_of(rig.part_bounds(Part::Head)));
                let posed = pose(
                    rig,
                    &PoseSignals {
                        gesture: Some(Gesture::Nurse),
                        gesture_amount: 1.0,
                        gesture_time: 1.0,
                        ..PoseSignals::default()
                    },
                    1.0,
                    0.0,
                );
                let mut transform = posed.part(Part::Head);
                let mut parent = rig.parent(Part::Head);
                for _ in 0..Part::ALL.len() {
                    let Some(part) = parent else {
                        break;
                    };
                    transform = posed.part(part) * transform;
                    parent = rig.parent(part);
                }
                let muzzle = rig.joints.neck.map_or_else(
                    || posed.root * transform * muzzle,
                    |neck| neck.nursing_muzzle(),
                );
                (rig.ground, h, udder, muzzle)
            }
            Look::Skeleton(art) => {
                let (udder, muzzle) = art.nursing_points()?;
                (art.ground(), art.bounds.height().max(1.0), udder, muzzle)
            }
            Look::Sprites(_) => return None,
        };
        let normalized = |point: Point| {
            let v = (point - ground) / height;
            CoreVec2::new(v.x as f32, v.y as f32)
        };
        Some((normalized(udder), normalized(muzzle)))
    }

    /// Fenêtre assez grande pour l'écrasement, les oreilles, les bulles,
    /// l'ombre et une culbute.
    #[must_use]
    pub fn canvas(&self) -> Canvas {
        let width = (self.width * 2.1).max(self.height * 2.2).max(120.0).ceil();
        let below = (self.height * 0.65).max(16.0).ceil();
        let height = (self.height * 2.25).max(100.0).ceil() + below;
        Canvas {
            width,
            height,
            anchor: CoreVec2::new(width * 0.5, height - below),
        }
    }
}

/// Réglages visuels globaux.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct Style {
    pub dark: bool,
}

/// Cible du dessin : coin haut gauche (repère du monde) et pixels par point.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Frame {
    pub origin: CoreVec2,
    pub scale: f64,
}

impl Frame {
    #[must_use]
    pub fn world_to_px(&self) -> Affine {
        Affine::scale(self.scale)
            * Affine::translate((-f64::from(self.origin.x), -f64::from(self.origin.y)))
    }
}

fn pt(v: CoreVec2) -> Point {
    Point::new(f64::from(v.x), f64::from(v.y))
}

/// Ajoute à `out` le dessin complet d'un animal.
pub fn paint_pet(
    art: &PetArt,
    view: &PetView<'_>,
    frame: &Frame,
    style: &Style,
    out: &mut DisplayList,
) {
    let world = frame.world_to_px();
    paint_shadow(art, view, world, style, out);
    let layered = view.opacity < 0.999;
    if layered {
        let bounds = out.bounds().to_path(0.1);
        out.push_layer(
            bounds,
            Affine::IDENTITY,
            view.opacity.clamp(0.0, 1.0),
            Mix::Normal,
        );
    }
    match &art.look {
        Look::Rig(rig) => paint_rig(rig, art, view, world, out),
        Look::Sprites(frames) => paint_sprite(art, frames, view, world, out),
        Look::Skeleton(skeleton) => {
            // Points par pixel de la toile.
            let k = f64::from(art.height) / skeleton.bounds.height().max(1.0);
            let pose = skeleton.pose(&view.pose, view.facing, f64::from(art.stride) / k);
            paint_skeleton(
                skeleton,
                &pose,
                crate::gesture::lids(&view.pose),
                world,
                pt(view.ground),
                k,
                out,
            );
        }
    }
    if layered {
        out.pop_layer();
    }
    paint_particles(view.particles, world, style, out);
}

/// L'ombre que Mouflon dessine sous l'animal, s'il en dessine une : sa
/// surface, et la part de l'animal au-dessus d'elle (de quoi savoir si elle
/// se voit encore).
fn drawn_shadow(art: &PetArt, view: &PetView<'_>) -> Option<(Shadow, ShadowShape)> {
    let shadow = view.shadow?;
    // Les images dessinent déjà leur ombre au sol : la nôtre ne sert qu'en l'air.
    if art.drawn.shadow && matches!(view.support, Support::Ground { .. }) {
        return None;
    }
    let shape = ShadowShape::of(art, view, &shadow);
    (shape.cast_alpha >= MIN_SHADOW).then_some((shadow, shape))
}

/// En deçà de cette opacité, une ombre ne se voit plus.
const MIN_SHADOW: f32 = 0.006;

/// L'ombre d'un animal, comme sous un ciel couvert : le corps cache surtout
/// la lumière d'en haut. Au contact, une ombre sombre et serrée sous les
/// pattes (le corps bouche presque tout le ciel) ; autour, une ombre douce
/// à la taille de ce qui touche le sol, le corps tourné compris. En
/// s'élevant, l'ombre de contact s'efface vite, et l'ombre portée s'élargit,
/// devient plus floue et pâlit, comme celle d'une main qu'on lève au-dessus
/// d'une table. Elle reste sur sa surface : au bout d'un rebord, il ne reste
/// que la part de l'ombre qui tombe dessus.
///
/// Deux formes seulement, toujours les mêmes à l'échelle près (flou compris) :
/// une seule image cuite pour chacune, partagée par tous les animaux.
#[derive(Clone, Copy, Debug, PartialEq)]
struct ShadowShape {
    /// Ombre portée : milieu, largeur, opacité.
    cast_x: f64,
    cast_w: f64,
    cast_alpha: f32,
    /// Ombre de contact (largeur nulle : aucune).
    contact_x: f64,
    contact_w: f64,
    contact_alpha: f32,
}

/// Hauteur d'une ombre portée, pour sa largeur (vue de face, une ellipse
/// très aplatie), et son flou, pour sa hauteur.
const CAST_FLAT: f64 = 0.17;
const CAST_BLUR: f64 = 0.5;
/// Les mêmes pour l'ombre de contact, plus serrée et plus nette.
const CONTACT_FLAT: f64 = 0.1;
const CONTACT_BLUR: f64 = 0.3;

impl ShadowShape {
    fn of(art: &PetArt, view: &PetView<'_>, shadow: &Shadow) -> Self {
        let opacity = view.opacity.clamp(0.0, 1.0);
        // Demi-largeur de ce qui pose au sol : le corps, tourné de sa
        // culbute (debout sur la tête, il est plus étroit ; couché sur le
        // flanc, sa hauteur s'étale), écrasé à l'atterrissage.
        let (half, height) = (f64::from(art.body.0) * 0.92, f64::from(art.body.1));
        let (sin, cos) = f64::from(view.pose.roll).sin_cos();
        let squash = 1.0 + 0.2 * f64::from(view.pose.squash.max(0.0));
        let reach = (half * cos.abs() + 0.45 * height * sin.abs()) * squash;
        let distance = f64::from(shadow.distance.max(0.0));
        let height = height.max(1.0);
        // Portée : plus large, plus floue et plus pâle avec la hauteur.
        let spread = 1.0 + distance / (2.5 * height);
        let fade = 1.0 / (1.0 + distance / (1.2 * height)).powi(2);
        // Contact : s'efface en s'élevant d'un tiers de sa hauteur.
        let touch = (1.0 - distance / (0.35 * height)).max(0.0);
        let x = f64::from(view.ground.x);
        let (lo, hi) = (f64::from(shadow.x0), f64::from(shadow.x1));
        let (cast_x, cast_w, cast_part) = clip_shadow(x, 2.0 * reach * spread, lo, hi);
        let (contact_x, contact_w, contact_part) =
            clip_shadow(x, 2.0 * reach * 0.62 * (0.7 + 0.3 * touch), lo, hi);
        Self {
            cast_x,
            cast_w,
            cast_alpha: (0.27 * fade * cast_part) as f32 * opacity,
            contact_x,
            contact_w: if touch > 0.0 { contact_w } else { 0.0 },
            contact_alpha: (0.27 * 0.9 * touch * touch * contact_part) as f32 * opacity,
        }
    }
}

/// La part d'une ombre (milieu `x`, largeur `w`) qui tombe sur la surface de
/// `lo` à `hi` : son milieu, sa largeur, et ce qu'il en reste (0 à 1). Elle
/// garde sa forme et rétrécit (une seule image cuite, à l'échelle près).
fn clip_shadow(x: f64, w: f64, lo: f64, hi: f64) -> (f64, f64, f64) {
    let (left, right) = ((x - w / 2.0).max(lo), (x + w / 2.0).min(hi));
    if right <= left || w <= 0.0 {
        return (x, 0.0, 0.0);
    }
    let kept = (right - left) / w;
    if kept >= 0.999 {
        return (x, w, 1.0);
    }
    ((left + right) / 2.0, right - left, kept.sqrt())
}

fn paint_shadow(
    art: &PetArt,
    view: &PetView<'_>,
    world: Affine,
    style: &Style,
    out: &mut DisplayList,
) {
    let Some((shadow, shape)) = drawn_shadow(art, view) else {
        return;
    };
    // Sur un fond sombre, une ombre un peu plus franche se voit autant.
    let boost = if style.dark { 0.42 / 0.27 } else { 1.0 };
    let y = f64::from(shadow.y);
    let mut ellipse = |x: f64, w: f64, flat: f64, blur: f64, lift: f64, alpha: f32| {
        if w <= 0.0 || alpha * boost < MIN_SHADOW {
            return;
        }
        let h = w * flat;
        let cy = y - h * lift;
        out.push(Command::BlurredRect {
            rect: Rect::new(x - w / 2.0, cy - h / 2.0, x + w / 2.0, cy + h / 2.0),
            radius: h / 2.0,
            std_dev: h * blur,
            color: Color::BLACK.with_alpha((alpha * boost).min(1.0)),
            transform: world,
        });
    };
    ellipse(
        shape.cast_x,
        shape.cast_w,
        CAST_FLAT,
        CAST_BLUR,
        0.1,
        shape.cast_alpha,
    );
    ellipse(
        shape.contact_x,
        shape.contact_w,
        CONTACT_FLAT,
        CONTACT_BLUR,
        -0.3,
        shape.contact_alpha,
    );
}

fn paint_rig(rig: &Rig, art: &PetArt, view: &PetView<'_>, world: Affine, out: &mut DisplayList) {
    let k = f64::from(art.height) / rig.bounds.height().max(1.0);
    let poses: PoseTransforms = pose(rig, &view.pose, view.facing, f64::from(art.stride) / k);
    let base = world
        * Affine::translate(pt(view.ground).to_vec2())
        * Affine::scale(k)
        * Affine::translate(-rig.ground.to_vec2())
        * poses.root;
    let mut stack: Vec<Affine> = Vec::with_capacity(8);
    stack.push(base);
    // Cuit : chaque suite de tracés d'une partie est une image posée d'un
    // coup. Le programme du dessin cuit ne garde que les parties et les
    // morceaux : une centaine de pas, au lieu des milliers de tracés du rig.
    if let Some(baked) = art.baked.as_deref() {
        for step in baked.program() {
            let top = *stack.last().unwrap_or(&base);
            match *step {
                Step::Begin(part) => stack.push(top * poses.part(part)),
                Step::End => {
                    stack.pop();
                }
                Step::Piece(rank) => {
                    let Some(piece) = baked.drawn(rank) else {
                        continue;
                    };
                    // Retourné (déterminant négatif), le corps a son reflet
                    // de l'autre côté.
                    let bitmap = match &piece.mirrored {
                        Some(mirrored) if top.determinant() < 0.0 => mirrored,
                        _ => &piece.bitmap,
                    };
                    out.push(Command::Image {
                        bitmap: bitmap.clone(),
                        transform: top * piece.placement,
                        alpha: 1.0,
                        smooth: true,
                    });
                }
            }
        }
        return;
    }
    let canvas = out.bounds().to_path(0.1);
    for (index, item) in rig.items.iter().enumerate() {
        let top = *stack.last().unwrap_or(&base);
        match item {
            Item::BeginPart(part) => stack.push(top * poses.part(*part)),
            Item::EndPart => {
                stack.pop();
            }
            Item::BeginLayer { alpha } => {
                out.push_layer(canvas.clone(), Affine::IDENTITY, *alpha, Mix::Normal)
            }
            Item::EndLayer => out.pop_layer(),
            Item::Fill {
                path,
                transform,
                paint,
                brush_transform,
                rule,
            } => out.push(Command::Fill {
                path: path.clone(),
                transform: top * *transform,
                paint: paint.clone(),
                brush_transform: *brush_transform,
                rule: *rule,
            }),
            Item::Stroke {
                path,
                transform,
                paint,
                brush_transform,
                style,
            } => out.push(Command::Stroke {
                path: path.clone(),
                transform: top * *transform,
                paint: paint.clone(),
                brush_transform: *brush_transform,
                style: style.clone(),
            }),
        }
        // La fin d'une suite de la toison : sa lumière, posée avec elle.
        if matches!(item, Item::Fill { .. } | Item::Stroke { .. })
            && art.lighting > 0.0
            && let Some(Some(fills)) = rig.fleece_runs.get(index)
        {
            paint_lighting(rig, art, fills, top, out);
        }
    }
}

/// Lumière venue d'en haut à gauche de l'écran, quel que soit le sens de
/// l'animal : le reflet glisse sur le corps quand il se retourne, ce qui
/// donne du volume au dessin plat. Comme celle des morceaux cuits, c'est la
/// lumière de toute la toison au repos, posée sur chaque suite de ses
/// tracés avec sa partie (`transform`) et découpée dans ses seules surfaces
/// (`fills`) : une encolure qui descend brouter emporte la sienne, sans
/// laisser de voile là où elle était.
fn paint_lighting(
    rig: &Rig,
    art: &PetArt,
    fills: &BezPath,
    transform: Affine,
    out: &mut DisplayList,
) {
    let Some((silhouette, local)) = &rig.body_silhouette else {
        return;
    };
    let bbox = local.transform_rect_bbox(silhouette.bounding_box());
    if bbox.width() <= 0.0 || bbox.height() <= 0.0 {
        return;
    }
    // Retourné, l'animal montre à gauche de l'écran le côté droit du dessin.
    let x = if transform.determinant() < 0.0 {
        bbox.max_x() - bbox.width() * 0.32
    } else {
        bbox.min_x() + bbox.width() * 0.32
    };
    let light = Point::new(x, bbox.min_y() + bbox.height() * 0.12);
    let radius = bbox.width().max(bbox.height()) * 1.05;
    out.push_layer(
        fills.clone(),
        transform,
        art.lighting.clamp(0.0, 1.0),
        Mix::SoftLight,
    );
    let paint = Paint::Radial {
        start: light,
        start_radius: 0.0,
        end: light,
        end_radius: radius as f32,
        stops: vec![
            Stop {
                offset: 0.0,
                color: Color::WHITE.with_alpha(0.95),
            },
            Stop {
                offset: 0.42,
                color: Color::WHITE.with_alpha(0.0),
            },
            Stop {
                offset: 0.62,
                color: Color::BLACK.with_alpha(0.0),
            },
            Stop {
                offset: 1.0,
                color: Color::BLACK.with_alpha(0.75),
            },
        ],
        extend: Extend::Pad,
    };
    out.push(Command::Fill {
        path: bbox.inflate(2.0, 2.0).to_path(0.1),
        transform,
        paint,
        brush_transform: None,
        rule: Fill::NonZero,
    });
    out.pop_layer();
}

fn paint_sprite(
    art: &PetArt,
    frames: &SpriteFrames,
    view: &PetView<'_>,
    world: Affine,
    out: &mut DisplayList,
) {
    let Some(index) = view.sprite_frame else {
        return;
    };
    let Some(pick) = frames.pick(index, view.facing) else {
        return;
    };
    let frame = pick.frame;
    let bitmap = &frame.bitmap;
    let s = f64::from(frames.scale);
    let g = pt(view.ground);
    let offset = f64::from(view.sprite_offset_y);
    let (ax, ay) = (f64::from(pick.anchor_x), f64::from(frames.anchor.1));
    let (fx, fy, texel) = (
        f64::from(frame.x),
        f64::from(frame.y),
        f64::from(frame.texel),
    );
    let motion = SpriteMotion::of(view, art);
    // L'image est retournée quand son sens dessiné n'est pas celui de l'animal.
    let flipped = motion.mirror < 0.0;
    if motion.is_rest() {
        // Coin de l'image calé sur les pixels de l'écran : le dessin reste net.
        let device = world.as_coeffs()[0];
        let k = s * device;
        let anchor = world * Point::new(g.x, g.y + offset);
        let bw = f64::from(bitmap.width) * texel;
        let left = if flipped {
            anchor.x - (fx + bw - ax) * k
        } else {
            anchor.x + (fx - ax) * k
        };
        let (x, y) = (left.round(), (anchor.y + (fy - ay) * k).round());
        let pixel = texel * k;
        let transform = if flipped {
            Affine::translate((x + bw * k, y)) * Affine::scale_non_uniform(-pixel, pixel)
        } else {
            Affine::translate((x, y)) * Affine::scale(pixel)
        };
        out.push(Command::Image {
            bitmap: bitmap.clone(),
            transform,
            alpha: 1.0,
            // Un pixel d'image par pixel d'écran (ou un multiple entier : le pixel
            // art agrandi) reste franc ; sinon, l'image se lisse.
            smooth: pixel < 0.999 || (pixel - pixel.round()).abs() > 0.01,
        });
        return;
    }
    // En mouvement : écrasement depuis les pattes, balancement sous la main qui
    // le tient, inclinaison en vol, culbute, demi-tour qui pivote.
    // Tenu, il se balance autour du point où le curseur le tient ; il roule
    // (culbute, physique réaliste) autour du milieu de son corps.
    let middle = Point::new(g.x, g.y - f64::from(art.body.1 / 1.02) * 0.5 + offset);
    let pivot = match (view.support, view.grip) {
        (Support::Held, Some(grip)) => pt(grip),
        (Support::Held, None) => {
            Point::new(g.x, g.y - f64::from(frames.tile_height) * s * 0.95 + offset)
        }
        _ => middle,
    };
    let sx = motion.mirror.signum() * motion.mirror.abs().max(0.12) * (1.0 + 0.5 * motion.squash);
    let sy = 1.0 - motion.squash;
    let transform = world
        * Affine::rotate_about(motion.roll, middle)
        * Affine::rotate_about(motion.angle, pivot)
        * Affine::translate((g.x, g.y + offset))
        * Affine::scale_non_uniform(sx, sy)
        * Affine::scale(s)
        * Affine::translate((fx - ax, fy - ay))
        * Affine::scale(texel);
    out.push(Command::Image {
        bitmap: bitmap.clone(),
        transform,
        alpha: 1.0,
        smooth: true,
    });
}

/// Mouvement ajouté par Mouflon aux images d'un animal en sprites.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct SpriteMotion {
    /// 1 : image telle qu'elle est dessinée ; -1 : en miroir ; entre les deux, il pivote.
    pub mirror: f64,
    /// > 0 : écrasé ; < 0 : étiré.
    pub squash: f64,
    /// Inclinaison et balancement dans le repère de l'écran (radians, sens
    /// horaire), autour de la main qui le tient ou du milieu du corps.
    pub angle: f64,
    /// Culbute, autour du milieu du corps.
    pub roll: f64,
}

impl SpriteMotion {
    #[must_use]
    pub fn of(view: &PetView<'_>, art: &PetArt) -> Self {
        let facing = f64::from(view.facing.clamp(-1.0, 1.0));
        let lean = f64::from(view.pose.lean) * facing.signum();
        let angle = match view.support {
            Support::Held => f64::from(view.pose.swing) * 0.9,
            Support::Air => lean * 0.8,
            Support::Ground { .. } => lean * 0.3,
        };
        let facings = match &art.look {
            Look::Sprites(frames) => frames.facings,
            Look::Rig(_) | Look::Skeleton(_) => Facings::Right,
        };
        let mirror = match facings {
            // Les sprites desktopPet regardent vers la gauche.
            Facings::Left => -facing,
            Facings::Right => facing,
            // Une image par sens : jamais retournée, elle se resserre seulement
            // pendant un demi-tour qui n'est pas dessiné.
            Facings::Both => facing.abs(),
        };
        Self {
            mirror,
            squash: if art.drawn.squash {
                0.0
            } else {
                f64::from(view.pose.squash)
            },
            angle,
            roll: f64::from(view.pose.roll),
        }
    }

    /// Image au repos : on la dessine nette, au pixel près.
    #[must_use]
    pub fn is_rest(&self) -> bool {
        self.squash.abs() < 0.004
            && self.angle.abs() < 0.003
            && self.roll.abs() < 0.003
            && self.mirror.abs() > 0.999
    }

    /// Le mouvement au millième près : deux mouvements de même clé se
    /// dessinent à moins d'un cinquième de pixel l'un de l'autre, même pour
    /// une image de 320 pixels. Au repos, l'image se pose au pixel près quel
    /// que soit ce qui reste du mouvement : une seule clé.
    #[must_use]
    pub fn key(&self) -> [i32; 4] {
        let thousandths = |v: f64| (v * 1000.0).round() as i32;
        if self.is_rest() {
            return [thousandths(self.mirror.signum()), 0, 0, 0];
        }
        [
            thousandths(self.mirror),
            thousandths(self.squash),
            thousandths(self.angle),
            thousandths(self.roll),
        ]
    }
}

/// Ce qui change, à l'œil, l'image d'un animal en sprites. Son déplacement
/// passe par la position de sa fenêtre : on ne le redessine que quand cette
/// clé change, quelques fois par seconde au lieu de 120.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct SpriteKey {
    frame: Option<u32>,
    /// Décalage vertical de l'image, au quart de point.
    offset: i32,
    opacity: u8,
    mirrored: bool,
    /// L'ombre, tant qu'elle se voit : sa distance à l'animal au demi-point
    /// (elle reste au sol quand la fenêtre bouge), son écrasement au millième,
    /// et ses bouts par rapport à l'animal (au bout d'un rebord), au demi-point.
    shadow: Option<[i32; 4]>,
    /// Le mouvement ajouté par Mouflon : une inclinaison qui ne change pas
    /// (au plafond, contre un mur) ne fait rien redessiner.
    motion: [i32; 4],
}

impl SpriteKey {
    /// La clé de l'image de `view`, ou rien quand l'animal se redessine à
    /// chaque image (dessin vectoriel, particules).
    #[must_use]
    pub fn of(art: &PetArt, view: &PetView<'_>) -> Option<Self> {
        if !matches!(art.look, Look::Sprites(_)) || !view.particles.is_empty() {
            return None;
        }
        Some(Self {
            frame: view.sprite_frame,
            offset: (view.sprite_offset_y * 4.0).round() as i32,
            opacity: (view.opacity.clamp(0.0, 1.0) * 255.0).round() as u8,
            mirrored: view.facing > 0.0,
            shadow: drawn_shadow(art, view).map(|(shadow, shape)| {
                let x = f64::from(view.ground.x);
                let left = shape.cast_x - shape.cast_w / 2.0 - x;
                let right = shape.cast_x + shape.cast_w / 2.0 - x;
                [
                    (shadow.distance * 2.0).round() as i32,
                    (view.pose.squash.max(0.0) * 1000.0).round() as i32,
                    (left * 2.0).round() as i32,
                    (right * 2.0).round() as i32,
                ]
            }),
            motion: SpriteMotion::of(view, art).key(),
        })
    }
}

fn paint_particles(particles: &[Particle], world: Affine, style: &Style, out: &mut DisplayList) {
    for p in particles {
        let t = f64::from(p.progress());
        let center = pt(p.pos);
        match p.kind {
            ParticleKind::Dust => {
                let r = f64::from(p.size) * (1.0 + 1.3 * t);
                let alpha = 0.5 * (1.0 - t).powf(1.6) as f32;
                let tone = if style.dark {
                    rgba(190, 196, 210, 1.0)
                } else {
                    rgba(168, 160, 150, 1.0)
                };
                let paint = Paint::Radial {
                    start: center,
                    start_radius: 0.0,
                    end: center,
                    end_radius: r as f32,
                    stops: vec![
                        Stop {
                            offset: 0.0,
                            color: tone.with_alpha(alpha),
                        },
                        Stop {
                            offset: 1.0,
                            color: tone.with_alpha(0.0),
                        },
                    ],
                    extend: Extend::Pad,
                };
                out.disc(center, r, world, paint);
            }
            ParticleKind::Bubble => {
                let fade = smooth(0.0, 0.12, t) * (1.0 - smooth(0.72, 1.0, t));
                let r = f64::from(p.size) * (0.75 + 0.7 * t);
                let ink = if style.dark {
                    rgba(220, 232, 255, 1.0)
                } else {
                    rgba(120, 150, 210, 1.0)
                };
                out.disc(
                    center,
                    r,
                    world,
                    Paint::Radial {
                        start: center + (r * 0.25, r * 0.3),
                        start_radius: 0.0,
                        end: center,
                        end_radius: r as f32,
                        stops: vec![
                            Stop {
                                offset: 0.0,
                                color: ink.with_alpha(0.0),
                            },
                            Stop {
                                offset: 0.8,
                                color: ink.with_alpha(0.16 * fade as f32),
                            },
                            Stop {
                                offset: 1.0,
                                color: ink.with_alpha(0.55 * fade as f32),
                            },
                        ],
                        extend: Extend::Pad,
                    },
                );
                let spark = center + (-r * 0.35, -r * 0.38);
                out.disc(
                    spark,
                    r * 0.22,
                    world,
                    Paint::Solid(Color::WHITE.with_alpha(0.85 * fade as f32)),
                );
            }
            ParticleKind::Leaf => {
                // Une petite feuille d'automne qui tournoie.
                let fade = smooth(0.0, 0.1, t) * (1.0 - smooth(0.75, 1.0, t));
                let s = f64::from(p.size);
                let turn = f64::from(p.phase) + t * 9.0;
                // Cuite une fois pour toutes : le rendu direct la pose.
                let Some(leaf) = crate::props::leaf((p.phase * 7.0) as usize) else {
                    continue;
                };
                // Vue de profil une fois sur deux : elle s'aplatit en tournant.
                let flat = 0.35 + 0.65 * (turn * 1.3).cos().abs();
                let at = world
                    * Affine::translate(center.to_vec2())
                    * Affine::rotate(turn)
                    * Affine::scale_non_uniform(s, s * flat);
                out.push(leaf.draw(at, 0.9 * fade as f32));
            }
        }
    }
}

fn smooth(a: f64, b: f64, x: f64) -> f64 {
    let t = ((x - a) / (b - a)).clamp(0.0, 1.0);
    t * t * (3.0 - 2.0 * t)
}

/// Chemin d'une forme quelconque (utile aux outils).
#[must_use]
pub fn shape_path(shape: &impl Shape) -> BezPath {
    shape.to_path(0.1)
}
