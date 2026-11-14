//! Squelettes à morceaux peints (voir [`mouflon_core::skeleton`]).
//!
//! Au chargement, chaque morceau est décodé, recadré sur ce qu'il montre (une
//! image exportée à la taille de toute la toile est surtout transparente) et
//! réduit, avec un filtre de surface, à la finesse de l'écran. À chaque image,
//! les os prennent leur pose : les animations à clés, mêlées selon ce que fait
//! l'animal, puis les mouvements du moteur pour les os qui ont un rôle (celui
//! des dessins vectoriels, voir [`crate::animate`]). Chaque morceau se pose
//! avec son os, comme une image du rendu direct : aucun tracé.
//!
//! Les mouvements du moteur sont calculés dans le repère de la toile, comme
//! si chaque os avec un rôle était libre (le corps monte et descend, les
//! pattes restent posées au sol), puis ramenés à son parent : un squelette
//! peut ranger ses os comme il veut, les sabots ne glissent pas pour autant.

use std::sync::Arc;

use mouflon_core::{BonePose, BoneRole, Gesture, PoseSignals, Shown, Skeleton};
use peniko::kurbo::{Affine, Point, Rect, Vec2};

use crate::animate::{
    Gait, LIP_CURL, TAIL_PIECES, TailRest, about, absorb, ease, leg_phase, muzzle_of,
    root_transform, rotate_about, scale_about, stepping, stride_scale, sway, sway_shear, tail_bend,
    untilt,
};
use crate::display::{Bitmap, Command, DisplayList};
use crate::gesture::{Gesturing, ears_turn, gesturing, leg_slot, with_posture};
use crate::joints::{Chain, HeadIntent, LegDrive, LegRest, NeckDrive, NeckPose, NeckRest};
use crate::sprites::{SpriteError, coverage, decode_png_rgba};

/// Pixels d'image par point affiché : deux (écrans Retina), et un quart de
/// plus pour les morceaux étirés et tournés.
const PIXELS_PER_POINT: f64 = 2.5;

#[derive(Debug, thiserror::Error)]
pub enum SkeletonArtError {
    #[error("{image} : {source}")]
    Image { image: String, source: SpriteError },
    #[error("le squelette ne montre rien : ses morceaux sont vides")]
    Empty,
}

/// Un morceau prêt à poser.
#[derive(Clone, Debug)]
struct PieceArt {
    bone: usize,
    bitmap: Bitmap,
    /// Des pixels de l'image vers ceux de la toile.
    placement: Affine,
    shown: Shown,
}

/// Un squelette prêt à dessiner.
#[derive(Clone, Debug)]
pub struct SkeletonArt {
    pub skeleton: Arc<Skeleton>,
    pieces: Vec<PieceArt>,
    /// Ce qui est dessiné, dans la toile (pixels).
    pub bounds: Rect,
    /// Ce que montrent un os et tous ceux qu'il porte.
    carried: Vec<Option<Rect>>,
    /// Des paupières peintes : les yeux ne s'écrasent pas pour se fermer.
    painted_lids: bool,
    /// Rang de chaque os parmi ceux de son rôle (oreilles, pattes…).
    rank: Vec<usize>,
    /// Pour une oreille : 1 si elle part vers l'avant de l'animal (la
    /// droite de la toile), -1 vers l'arrière ; elle tombe en tournant de
    /// ce côté-là.
    side: Vec<f64>,
    /// L'encolure et la tête qu'elle porte, s'il y en a une.
    neck: Option<NeckRest>,
    /// Les pattes en deux segments (voir [`leg_slot`]) : l'os du haut, celui
    /// du bas, la chaîne au repos.
    legs: [Option<(usize, usize, LegRest)>; 4],
    /// Le sabot de l'antérieur qu'on voit, au repos.
    fore_hoof: Point,
    /// La queue, de la racine au bout (les os `tail` dont chacun porte le
    /// suivant) : l'articulation de chaque os, et ce qu'il montre lui-même.
    tail: Option<TailRest>,
    /// La toison : ce que montrent les morceaux du corps, de l'encolure et
    /// des mèches (le moteur couche et incline l'animal d'après elle :
    /// découper la toison en morceaux ne la change pas).
    fleece: Option<Rect>,
}

/// Les transformations d'une image.
#[derive(Clone, Debug)]
pub struct SkeletonPose {
    /// L'animal entier (écrasement, inclinaison, retournement, culbute).
    pub root: Affine,
    /// Chaque os, de la toile au repos vers la toile en mouvement.
    pub bones: Vec<Affine>,
}

/// Pixels RGBA (non prémultipliés) d'une image décodée.
struct Decoded {
    width: usize,
    height: usize,
    rgba: Vec<u8>,
}

impl Decoded {
    /// Le rectangle des pixels qui se voient, s'il y en a.
    fn visible(&self) -> Option<(usize, usize, usize, usize)> {
        let (mut x0, mut y0, mut x1, mut y1) = (usize::MAX, usize::MAX, 0, 0);
        for y in 0..self.height {
            let row = &self.rgba[y * self.width * 4..(y + 1) * self.width * 4];
            let mut any = false;
            for (x, p) in row.as_chunks::<4>().0.iter().enumerate() {
                if p[3] > 0 {
                    x0 = x0.min(x);
                    x1 = x1.max(x + 1);
                    any = true;
                }
            }
            if any {
                y0 = y0.min(y);
                y1 = y1.max(y + 1);
            }
        }
        (x0 < x1 && y0 < y1).then_some((x0, y0, x1, y1))
    }
}

/// La partie `(x0, y0, x1, y1)` de l'image, réduite à `out_w` × `out_h` par
/// un filtre de surface, couleurs pondérées par leur opacité (pas de liseré
/// sombre) : RGBA non prémultiplié.
fn reduce(image: &Decoded, area: (usize, usize, usize, usize), out_w: u32, out_h: u32) -> Vec<u8> {
    let (x0, y0, x1, y1) = area;
    let (w, h) = (x1 - x0, y1 - y0);
    let columns = coverage(w as u32, out_w);
    let rows = coverage(h as u32, out_h);
    let (ow, oh) = (out_w as usize, out_h as usize);
    // Passe horizontale, en prémultiplié.
    let mut wide = vec![[0.0f32; 4]; ow * h];
    for y in 0..h {
        let start = ((y0 + y) * image.width + x0) * 4;
        let row = image.rgba[start..start + w * 4].as_chunks::<4>().0;
        for (i, taps) in columns.iter().enumerate() {
            let mut acc = [0.0f32; 4];
            for &(sx, weight) in taps {
                let p = row[sx];
                let a = f32::from(p[3]) / 255.0 * weight;
                acc[0] += f32::from(p[0]) * a;
                acc[1] += f32::from(p[1]) * a;
                acc[2] += f32::from(p[2]) * a;
                acc[3] += a;
            }
            wide[y * ow + i] = acc;
        }
    }
    // Passe verticale, puis retour au non prémultiplié.
    let mut out = Vec::with_capacity(ow * oh * 4);
    for taps in &rows {
        for i in 0..ow {
            let mut acc = [0.0f32; 4];
            for &(sy, weight) in taps {
                let p = wide[sy * ow + i];
                for c in 0..4 {
                    acc[c] += p[c] * weight;
                }
            }
            let a = acc[3];
            if a <= 1e-6 {
                out.extend_from_slice(&[0, 0, 0, 0]);
                continue;
            }
            let channel = |v: f32| (v / a).round().clamp(0.0, 255.0) as u8;
            out.extend_from_slice(&[
                channel(acc[0]),
                channel(acc[1]),
                channel(acc[2]),
                (a * 255.0).round().clamp(0.0, 255.0) as u8,
            ]);
        }
    }
    out
}

fn union(a: Option<Rect>, b: Rect) -> Rect {
    a.map_or(b, |a| a.union(b))
}

impl SkeletonArt {
    /// Prépare `skeleton`, affiché à `height` points de haut.
    pub fn load(skeleton: Arc<Skeleton>, height: f32) -> Result<Self, SkeletonArtError> {
        // Décodés et recadrés : ce que chaque morceau montre, dans la toile.
        let mut decoded = Vec::with_capacity(skeleton.pieces.len());
        let mut bounds: Option<Rect> = None;
        for piece in &skeleton.pieces {
            let image_error = |source| SkeletonArtError::Image {
                image: piece.image.clone(),
                source,
            };
            let bytes = piece.png.bytes().map_err(|e| image_error(e.into()))?;
            let (width, height, rgba) = decode_png_rgba(&bytes).map_err(image_error)?;
            let image = Decoded {
                width: width as usize,
                height: height as usize,
                rgba,
            };
            let visible = image.visible();
            if let Some((x0, y0, x1, y1)) = visible {
                let at = Point::new(f64::from(piece.at.x), f64::from(piece.at.y));
                let rect = Rect::new(x0 as f64, y0 as f64, x1 as f64, y1 as f64) + at.to_vec2();
                bounds = Some(union(bounds, rect));
            }
            decoded.push((image, visible));
        }
        let bounds = bounds.ok_or(SkeletonArtError::Empty)?;
        // Points par pixel de la toile, puis pixels d'image par pixel de la
        // toile : jamais agrandi, réduit s'il est plus fin que l'écran.
        let k = f64::from(height) / bounds.height().max(1.0);
        let ratio = (k * PIXELS_PER_POINT).min(1.0);
        let bones = skeleton.bones.len();
        let mut own: Vec<Option<Rect>> = vec![None; bones];
        let mut pieces = Vec::with_capacity(decoded.len());
        for (piece, (image, visible)) in skeleton.pieces.iter().zip(&decoded) {
            let Some(area) = *visible else {
                continue;
            };
            let (x0, y0, x1, y1) = area;
            let (w, h) = (x1 - x0, y1 - y0);
            let out_w = ((w as f64 * ratio).ceil() as u32).max(1);
            let out_h = ((h as f64 * ratio).ceil() as u32).max(1);
            let rgba = if out_w as usize == w && out_h as usize == h {
                let mut rgba = Vec::with_capacity(w * h * 4);
                for y in y0..y1 {
                    let start = (y * image.width + x0) * 4;
                    rgba.extend_from_slice(&image.rgba[start..start + w * 4]);
                }
                rgba
            } else {
                reduce(image, area, out_w, out_h)
            };
            let origin = Vec2::new(
                f64::from(piece.at.x) + x0 as f64,
                f64::from(piece.at.y) + y0 as f64,
            );
            let placement = Affine::translate(origin)
                * Affine::scale_non_uniform(
                    w as f64 / f64::from(out_w),
                    h as f64 / f64::from(out_h),
                );
            if let Some(slot) = own.get_mut(piece.bone) {
                let rect = Rect::from_origin_size(origin.to_point(), (w as f64, h as f64));
                *slot = Some(union(*slot, rect));
            }
            pieces.push(PieceArt {
                bone: piece.bone,
                bitmap: Bitmap::from_rgba(out_w, out_h, rgba),
                placement,
                shown: piece.shown,
            });
        }
        // Ce que porte chaque os : les enfants viennent après leur parent.
        let mut carried = own.clone();
        for i in (0..bones).rev() {
            if let (Some(parent), Some(rect)) = (skeleton.bones[i].parent, carried[i]) {
                carried[parent] = Some(union(carried[parent], rect));
            }
        }
        let mut rank = vec![0; bones];
        let mut seen: Vec<BoneRole> = Vec::new();
        for (i, bone) in skeleton.bones.iter().enumerate() {
            if let Some(role) = bone.role {
                rank[i] = seen.iter().filter(|r| **r == role).count();
                seen.push(role);
            }
        }
        let side = skeleton
            .bones
            .iter()
            .zip(&carried)
            .map(|(bone, rect)| match rect {
                Some(rect) if rect.center().x < f64::from(bone.at.x) => -1.0,
                _ => 1.0,
            })
            .collect();
        let painted_lids = skeleton.pieces.iter().any(|p| p.shown == Shown::EyesClosed);
        let at = |i: usize| {
            let a = skeleton.bones[i].at;
            Point::new(f64::from(a.x), f64::from(a.y))
        };
        let with_role =
            |wanted: BoneRole| skeleton.bones.iter().position(|b| b.role == Some(wanted));
        // L'encolure : son os, et la tête, son enfant.
        let neck = with_role(BoneRole::Neck).and_then(|n| {
            let head = with_role(BoneRole::Head)?;
            let rect = carried[head]?;
            Some(NeckRest {
                base: at(n),
                poll: at(head),
                muzzle: skeleton.muzzle.map_or_else(
                    || muzzle_of(rect),
                    |m| Point::new(f64::from(m.x), f64::from(m.y)),
                ),
                head: rect,
            })
        });
        // Les pattes en deux segments : chaque bas de patte, enfant de sa
        // patte. Couché, le haut d'une patte peinte sous le corps y glisse,
        // tant que son bord du haut reste sous le dos (voir
        // [`crate::joints::LegRest::new`]).
        let body = with_role(BoneRole::Body);
        let first_piece = |bone: usize| skeleton.pieces.iter().position(|p| p.bone == bone);
        // Le haut du corps à l'abscisse `x` de la toile : le premier pixel
        // qui se voit de ses morceaux, de haut en bas.
        let back = |x: f64| {
            skeleton
                .pieces
                .iter()
                .zip(&decoded)
                .filter(|(piece, _)| Some(piece.bone) == body)
                .filter_map(|(piece, (image, _))| {
                    let col = (x - f64::from(piece.at.x)).floor();
                    if col < 0.0 || col >= image.width as f64 {
                        return None;
                    }
                    let col = col as usize;
                    (0..image.height)
                        .find(|&y| image.rgba[(y * image.width + col) * 4 + 3] > 0)
                        .map(|y| f64::from(piece.at.y) + y as f64)
                })
                .fold(f64::INFINITY, f64::min)
        };
        let headroom = |upper: usize| {
            let under = match (body.and_then(first_piece), first_piece(upper)) {
                (Some(body), Some(leg)) => leg < body,
                _ => false,
            };
            match own[upper] {
                Some(top) if under => {
                    let room = [0.25, 0.5, 0.75]
                        .into_iter()
                        .map(|k| top.min_y() - back(top.min_x() + k * top.width()))
                        .fold(f64::INFINITY, f64::min);
                    (room - 0.02 * bounds.height()).max(0.0)
                }
                _ => 0.0,
            }
        };
        // Le contour de ce que peint un os : sur chaque ligne de ses
        // morceaux, du premier au dernier pixel qui se voit.
        let outline = |bone: usize| {
            let mut points = Vec::new();
            for (piece, (image, visible)) in skeleton.pieces.iter().zip(&decoded) {
                let Some((x0, y0, x1, y1)) = *visible else {
                    continue;
                };
                if piece.bone != bone {
                    continue;
                }
                for y in y0..y1 {
                    let row = &image.rgba[(y * image.width + x0) * 4..(y * image.width + x1) * 4];
                    let seen = |p: &[u8; 4]| p[3] > 0;
                    let pixels = row.as_chunks::<4>().0;
                    let (Some(first), Some(last)) =
                        (pixels.iter().position(seen), pixels.iter().rposition(seen))
                    else {
                        continue;
                    };
                    let left = f64::from(piece.at.x) + (x0 + first) as f64;
                    let right = f64::from(piece.at.x) + (x0 + last + 1) as f64;
                    let top = f64::from(piece.at.y) + y as f64;
                    points.extend([
                        Point::new(left, top),
                        Point::new(right, top),
                        Point::new(left, top + 1.0),
                        Point::new(right, top + 1.0),
                    ]);
                }
            }
            crate::pendant::convex_hull(&points)
        };
        let mut legs = [None; 4];
        let mut fore_hoof = None;
        for (i, bone) in skeleton.bones.iter().enumerate() {
            let Some(BoneRole::LegLower { front, near }) = bone.role else {
                continue;
            };
            let Some(upper) = bone.parent else {
                continue;
            };
            if skeleton.bones[upper].role != Some(BoneRole::Leg { front, near }) {
                continue;
            }
            let Some(rect) = carried[i] else {
                continue;
            };
            let tip = Point::new(rect.center().x, rect.max_y());
            if front && near {
                fore_hoof = Some(tip);
            }
            legs[leg_slot(front, near)] = Some((
                upper,
                i,
                LegRest::new(
                    Chain::new(at(upper), at(i), tip),
                    (front, near),
                    own[upper],
                    rect,
                    body.and_then(|b| own[b]),
                    headroom(upper),
                    &outline(upper),
                ),
            ));
        }
        let fore_hoof = fore_hoof
            .or_else(|| {
                let leg = with_role(BoneRole::Leg {
                    front: true,
                    near: true,
                })?;
                let rect = carried[leg]?;
                Some(Point::new(rect.center().x, rect.max_y()))
            })
            .unwrap_or(Point::new(
                bounds.center().x + bounds.height() * 0.3,
                f64::from(skeleton.ground.y),
            ));
        // La queue : ses os dans l'ordre de leur rang, tant que chacun est
        // l'enfant du précédent.
        let mut tail = Vec::new();
        let mut last = None;
        for (i, bone) in skeleton.bones.iter().enumerate() {
            if bone.role != Some(BoneRole::Tail) {
                continue;
            }
            if last.is_some() && bone.parent != last {
                break;
            }
            tail.push((at(i), own[i]));
            last = Some(i);
        }
        let fleece = skeleton
            .bones
            .iter()
            .zip(&own)
            .filter(|(bone, _)| {
                matches!(
                    bone.role,
                    Some(BoneRole::Body | BoneRole::Neck | BoneRole::Sway)
                )
            })
            .filter_map(|(_, rect)| *rect)
            .reduce(|a, b| a.union(b));
        Ok(Self {
            skeleton,
            pieces,
            bounds,
            carried,
            painted_lids,
            rank,
            side,
            neck,
            legs,
            fore_hoof,
            tail: TailRest::new(tail),
            fleece,
        })
    }

    /// Octets des images des morceaux.
    #[must_use]
    pub fn byte_size(&self) -> usize {
        self.pieces
            .iter()
            .map(|p| p.bitmap.width as usize * p.bitmap.height as usize * 4)
            .sum()
    }

    /// Le point d'appui, dans la toile.
    #[must_use]
    pub fn ground(&self) -> Point {
        let g = self.skeleton.ground;
        Point::new(f64::from(g.x), f64::from(g.y))
    }

    /// Les sabots des pattes en deux segments (antérieur de ce côté, de
    /// l'autre, postérieur de ce côté, de l'autre) : l'os du bas, et le
    /// point que la patte pose au sol, au repos (le bas de son morceau, au
    /// milieu, dans la toile).
    #[must_use]
    pub fn hooves(&self) -> [Option<(usize, Point)>; 4] {
        self.legs
            .map(|leg| leg.map(|(_, lower, rest)| (lower, rest.chain.tip)))
    }

    /// Les repères de tétée dans la toile : sous l'arrière du ventre,
    /// puis la portée du museau relevé. Calculés au chargement.
    pub(crate) fn nursing_points(&self) -> Option<(Point, Point)> {
        let head = self.role(BoneRole::Head)?;
        let body = self.fleece?;
        let h = self.bounds.height().max(1.0);
        let hip = self
            .role(BoneRole::Leg {
                front: false,
                near: true,
            })
            .map_or(body.min_x() + body.width() * 0.2, |i| {
                f64::from(self.skeleton.bones[i].at.x)
            });
        let udder = crate::joints::udder_point(body, hip, self.ground(), h, self.neck);
        let muzzle = self
            .neck
            .map(|n| n.muzzle)
            .or_else(|| self.carried[head].map(muzzle_of))?;
        let posed = self.pose(
            &PoseSignals {
                gesture: Some(Gesture::Nurse),
                gesture_amount: 1.0,
                gesture_time: 1.0,
                ..PoseSignals::default()
            },
            1.0,
            0.0,
        );
        let muzzle = self.neck.map_or_else(
            || posed.root * posed.bones[head] * muzzle,
            |neck| neck.nursing_muzzle(),
        );
        Some((udder, muzzle))
    }

    /// L'os qui a ce rôle (le premier), s'il y en a un.
    fn role(&self, wanted: BoneRole) -> Option<usize> {
        self.skeleton
            .bones
            .iter()
            .position(|b| b.role == Some(wanted))
    }

    /// La pose d'une image. `stride` : distance parcourue pendant un cycle de
    /// marche, en pixels de la toile (0 : 0,9 fois sa hauteur).
    #[must_use]
    pub fn pose(&self, s: &PoseSignals, facing: f32, stride: f64) -> SkeletonPose {
        let sk = &*self.skeleton;
        let count = sk.bones.len();
        let h = self.bounds.height().max(1.0);
        let u = h / 100.0;
        let g = self.ground();

        // --- Animations à clés : le calme, puis ce que fait l'animal par-dessus.
        let mut clips = vec![BonePose::default(); count];
        let m = &sk.motions;
        let on_clock = |clip: usize| s.clock / sk.clips[clip].length;
        let on_gait = s.gait_phase / std::f32::consts::TAU;
        if let Some(idle) = m.idle {
            sk.clips[idle].blend_into(on_clock(idle), 1.0, &mut clips);
        }
        for (clip, weight, gait) in [
            (m.sit, s.sit, false),
            (m.sleep, s.sleep, false),
            (m.graze, s.graze, false),
            (m.walk, s.walk, true),
            (m.run, s.run, true),
            (m.air, s.air, false),
            (m.dangle, s.dangle, false),
        ] {
            if let Some(clip) = clip {
                let t = if gait { on_gait } else { on_clock(clip) };
                sk.clips[clip].blend_into(t, weight, &mut clips);
            }
        }
        // Ce que les animations jouent, le moteur ne le refait pas.
        let mut p = *s;
        let skip = |clip: Option<usize>, signal: &mut f32| {
            if clip.is_some() {
                *signal = 0.0;
            }
        };
        skip(m.walk, &mut p.walk);
        skip(m.run, &mut p.run);
        // Une marche ou une course à clés joue aussi le trot.
        skip(m.walk.or(m.run), &mut p.trot);
        skip(m.sit, &mut p.sit);
        skip(m.sleep, &mut p.sleep);
        skip(m.graze, &mut p.graze);
        skip(m.air, &mut p.air);
        skip(m.dangle, &mut p.dangle);

        // --- L'animal entier.
        let hold_x = self
            .role(BoneRole::Body)
            .map_or(g.x, |b| f64::from(sk.bones[b].at.x));
        let hold = Point::new(hold_x, self.bounds.min_y() + h * 0.12);
        let root = root_transform(s, facing, g, h, hold);

        // --- Les os avec un rôle, comme s'ils étaient libres (repère de la toile).
        let engine = self.engine(&p, s, facing, stride, g, h, u);
        let mut free = vec![Affine::IDENTITY; count];
        let mut bones = vec![Affine::IDENTITY; count];
        for (i, bone) in sk.bones.iter().enumerate() {
            let parent_free = bone.parent.map_or(Affine::IDENTITY, |b| free[b]);
            let at = Point::new(f64::from(bone.at.x), f64::from(bone.at.y));
            free[i] = match bone.role {
                None => parent_free,
                Some(role) => engine.bone(role, at, self.rank[i], parent_free, self, i),
            };
            // Ramené à son parent, puis l'animation à clés, autour de l'articulation.
            let engine_local = parent_free.inverse() * free[i];
            // La tétée pose le museau et les appuis en coordonnées précises.
            // Les clés structurelles s'effacent avec l'entrée du geste et
            // reviennent avec sa sortie, sans déplacer les appuis en pleine
            // tétée. Les petites parties libres gardent toutes leurs clés.
            let clip = if s.gesture == Some(Gesture::Nurse)
                && !matches!(
                    bone.role,
                    Some(
                        BoneRole::Ear
                            | BoneRole::Eye
                            | BoneRole::Pupil
                            | BoneRole::Tail
                            | BoneRole::Jaw
                            | BoneRole::Lip
                            | BoneRole::Sway
                    )
                ) {
                clips[i].lerp(BonePose::default(), s.gesture_amount.clamp(0.0, 1.0))
            } else {
                clips[i]
            };
            let keyed = about(
                at,
                Affine::translate((f64::from(clip.moves.x), f64::from(clip.moves.y)))
                    * Affine::rotate(f64::from(clip.rotate))
                    * Affine::scale_non_uniform(f64::from(clip.scale.x), f64::from(clip.scale.y)),
            );
            let parent = bone.parent.map_or(Affine::IDENTITY, |b| bones[b]);
            bones[i] = parent * engine_local * keyed;
        }
        // Cabré : tout l'animal tourne autour de ses hanches, sauf les
        // pattes de derrière, qui restent où elles sont.
        let mut root = root;
        if engine.act.rear > 0.0 {
            let hind =
                |role: Option<BoneRole>| matches!(role, Some(BoneRole::Leg { front: false, .. }));
            let hips: Vec<Point> = sk
                .bones
                .iter()
                .filter(|b| hind(b.role))
                .map(|b| Point::new(f64::from(b.at.x), f64::from(b.at.y)))
                .collect();
            if !hips.is_empty() {
                let n = hips.len() as f64;
                let at = Point::new(
                    hips.iter().map(|p| p.x).sum::<f64>() / n,
                    hips.iter().map(|p| p.y).sum::<f64>() / n,
                );
                let rear = crate::gesture::rearing(engine.act.rear, at);
                root *= rear;
                let back = rear.inverse();
                // Les pattes de derrière et tout ce qu'elles portent (leur bas,
                // leurs sabots) : les parents viennent avant leurs enfants.
                let mut carried = vec![false; count];
                for (i, bone) in sk.bones.iter().enumerate() {
                    carried[i] = hind(bone.role) || bone.parent.is_some_and(|p| carried[p]);
                    if carried[i] {
                        bones[i] = back * bones[i];
                    }
                }
            }
        }
        SkeletonPose { root, bones }
    }

    /// Les mouvements du moteur pour cette image (voir [`Engine::bone`]).
    #[allow(clippy::too_many_arguments)]
    fn engine(
        &self,
        p: &PoseSignals,
        s: &PoseSignals,
        facing: f32,
        stride: f64,
        g: Point,
        h: f64,
        u: f64,
    ) -> Engine {
        let f = |v: f32| f64::from(v);
        let phase = f(p.gait_phase);
        let gait = Gait::of(p);
        let (sit, sleep, air) = (f(p.sit), f(p.sleep), f(p.air));
        let graze = ease(0.0, 1.0, f(p.graze));
        let act = with_posture(gesturing(s), s);
        let body = self.fleece.unwrap_or(self.bounds);
        let clearance = (g.y - body.max_y() - 0.6 * u).max(0.0);
        let front_down = ease(0.0, 0.65, sit);
        let back_down = ease(0.35, 1.0, sit);
        let drop_front = clearance * front_down;
        let drop_back = clearance * back_down;
        let half_length = (body.width() * 0.5).max(u);
        let pitch = ((drop_front - drop_back) / (2.0 * half_length)).atan();
        let segmented = self.legs.iter().any(Option::is_some);
        let landing = f(s.squash).clamp(-0.15, 0.16);
        let bob = gait.bob(phase, u) + act.body_shift.y * u + absorb(segmented, landing, h);
        let rock = gait.pitch(phase, u, 2.0 * half_length);
        let body_t =
            Affine::translate((act.body_shift.x * u, bob + 0.5 * (drop_front + drop_back)))
                * rotate_about(body.center(), pitch + rock + 0.05 * graze + act.body_angle)
                * scale_about(g, 1.0 + 0.04 * sleep, 1.0 - 0.05 * sleep)
                * scale_about(body.center(), 1.0 + act.body_squash, 1.0 - act.body_squash);
        // Les pattes : le balayage de l'appui couvre exactement la distance
        // que le corps parcourt pendant ce temps.
        let stride_walk = if stride > 0.0 { stride } else { h * 0.9 };
        let stride = stride_walk * stride_scale(s);
        let duty = gait.duty();
        let rise = f(s.rise);
        // Un agneau qui saute garde les pattes raides.
        let loose = 1.0 - f(s.stiff);
        // Sur un rebord (voir `crate::animate::leap_leg`).
        let leap = f(s.leap).clamp(0.0, 1.0);
        let leaping = leap * air * loose;
        let nursing = s.gesture == Some(Gesture::Nurse);
        let nursing_unroot = if nursing {
            let hold_x = self
                .role(BoneRole::Body)
                .map_or(g.x, |i| f64::from(self.skeleton.bones[i].at.x));
            root_transform(
                s,
                1.0,
                g,
                h,
                Point::new(hold_x, self.bounds.min_y() + h * 0.12),
            )
            .inverse()
        } else {
            Affine::IDENTITY
        };
        // Couchée, une longue queue se courbe sur le sol au lieu de s'y
        // enfoncer (voir `tail_bend`) : ses os comme le moteur les tourne.
        let tail_bend = {
            let base = Affine::translate((act.body_shift.x * u, bob + drop_back));
            let angle = |rank: usize| match rank {
                0 => f(s.tail) - 0.2 * f(p.dangle) + act.tail,
                1 => f(s.tail_mid),
                _ => f(s.tail_tip),
            };
            self.tail.as_ref().map_or([0.0; TAIL_PIECES], |tail| {
                tail_bend(base, tail, angle, g.y - 0.6 * u)
            })
        };
        let mut engine = Engine {
            signals: *p,
            all: *s,
            facing,
            g,
            u,
            h,
            phase,
            bob,
            drop_front,
            drop_back,
            graze,
            body_t,
            stride,
            stride_walk,
            duty,
            gait,
            act,
            loose,
            tuck: air * ease(0.0, 1.0, rise) * loose * (1.0 - leaping),
            reach: air * ease(0.0, 1.0, -rise) * loose * (1.0 - leaping),
            leaping,
            flight: f(s.leap_flight),
            splay: f(s.squash).clamp(-0.15, 0.16).max(0.0) * 3.0 * (1.0 - leap),
            front_down,
            back_down,
            tail_bend,
            neck: None,
            legs: [None; 4],
        };
        // Les articulations : l'encolure, et les pattes en deux segments.
        if let Some(rest) = &self.neck {
            let intent = engine.head_base();
            let sleep_low = f(s.sleep_low);
            let drive = NeckDrive {
                base: body_t * rest.base,
                intent: HeadIntent {
                    angle: intent.0,
                    shift: intent.1,
                },
                alert: (-f(s.ears_back)).clamp(0.0, 1.0) * (1.0 - graze) * (1.0 - sit),
                charge: f(s.head_low).clamp(0.0, 1.0),
                doze: sleep * (1.0 - sleep_low),
                graze,
                nibble: f(p.nibble),
                graze_at: Point::new(self.fore_hoof.x, g.y - 0.3 * u),
                sleep_low,
                lying: sit,
                nursing: s.nursing_target.filter(|_| nursing).map(|target| {
                    (
                        nursing_unroot * (g + Vec2::new(f(target.x), f(target.y)) * h),
                        f(s.gesture_amount),
                    )
                }),
                ground: g.y,
                u,
                squash: engine.act.head_squash,
            };
            engine.neck = Some(rest.pose(&drive, None));
        }
        let drive = LegDrive {
            gait: engine.gait,
            phase,
            duty,
            stride: engine.stride,
            stride_walk: engine.stride_walk,
            legs: engine.signals.legs,
            u,
            ground: g.y,
            air,
            tuck: engine.tuck,
            reach: engine.reach,
            leaping: engine.leaping,
            flight: engine.flight,
            loose,
            dangle: f(p.dangle),
            swing: f(s.swing),
            splay: engine.splay,
            front_down,
            back_down,
            act: engine.act,
            untilt: if nursing {
                nursing_unroot
            } else {
                untilt(s, g, h)
            },
        };
        for (slot, leg) in self.legs.iter().enumerate() {
            if let Some((_, _, rest)) = leg {
                engine.legs[slot] = Some(drive.pose(rest, body_t * rest.chain.root));
            }
        }
        engine
    }
}

/// Ce que le moteur fait des os pour une image : les grandeurs partagées
/// par tous, calculées une fois.
struct Engine {
    /// Les signaux, sans ce que jouent les animations à clés.
    signals: PoseSignals,
    /// Tous les signaux (le regard, les clignements, la respiration…).
    all: PoseSignals,
    /// Tourné vers la droite (1) ou la gauche (-1).
    facing: f32,
    g: Point,
    u: f64,
    h: f64,
    phase: f64,
    bob: f64,
    drop_front: f64,
    drop_back: f64,
    graze: f64,
    body_t: Affine,
    stride: f64,
    stride_walk: f64,
    duty: f64,
    gait: Gait,
    /// Le geste en cours et la posture.
    act: Gesturing,
    /// 1 - raideur des pattes (un agneau qui saute les garde raides).
    loose: f64,
    tuck: f64,
    reach: f64,
    /// Un saut sur un rebord, en l'air (voir `LegDrive::leaping`).
    leaping: f64,
    flight: f64,
    splay: f64,
    front_down: f64,
    back_down: f64,
    /// De combien chaque os de la queue se plie vers l'arrière pour se
    /// poser sur le sol (0 : elle n'y touche pas).
    tail_bend: [f64; TAIL_PIECES],
    /// L'encolure et la tête, s'il y a une encolure.
    neck: Option<NeckPose>,
    /// Les pattes en deux segments : le haut, libre dans la toile, et le bas,
    /// dans le repère du haut.
    legs: [Option<(Affine, Affine)>; 4],
}

impl Engine {
    /// Ce que le moteur veut de la tête d'une pièce, avant de la poser au
    /// sol pour brouter ou pour dormir : sa rotation et son déplacement.
    fn head_base(&self) -> (f64, Vec2) {
        let f = |v: f32| f64::from(v);
        let (p, s) = (&self.signals, &self.all);
        let (sleep, air, dangle) = (f(p.sleep), f(p.air), f(p.dangle));
        let look = Vec2::new(f(s.look.x), f(s.look.y));
        let u = self.u;
        let rise = f(s.rise);
        let act = &self.act;
        let angle =
            look.y * 0.16 + 0.42 * sleep - 0.06 * air + f(s.lean) * 0.3 + self.gait.nod(self.phase)
                - 0.12 * dangle
                - 0.12 * air * ease(0.0, 1.0, rise)
                + 0.1 * air * ease(0.0, 1.0, -rise)
                + act.head_angle;
        let shift = Vec2::new(
            look.x * u * 1.4 + (act.head_shift.x + act.body_shift.x) * u,
            look.y * u
                + self.bob * 1.15
                + self.gait.head_bob(self.phase, u)
                + self.drop_front
                + sleep * self.h * 0.06
                + act.head_shift.y * u,
        );
        (angle, shift)
    }

    /// Où le moteur met l'os `index` (articulation `at`, `rank`-ième de son
    /// rôle), libre dans la toile ; `parent` : où est son parent.
    fn bone(
        &self,
        role: BoneRole,
        at: Point,
        rank: usize,
        parent: Affine,
        art: &SkeletonArt,
        index: usize,
    ) -> Affine {
        let f = |v: f32| f64::from(v);
        let (p, s) = (&self.signals, &self.all);
        let (sleep, air, dangle) = (f(p.sleep), f(p.air), f(p.dangle));
        let breath = f(s.breath);
        let look = Vec2::new(f(s.look.x), f(s.look.y));
        let u = self.u;
        match role {
            BoneRole::Body => self.body_t,
            BoneRole::Head => {
                if let Some(neck) = &self.neck {
                    return neck.head;
                }
                let act = &self.act;
                let (mut angle, mut shift) = self.head_base();
                let low = f(s.sleep_low);
                if low > 0.0
                    && let Some(head) = art.carried[index]
                {
                    // Endormi la tête au sol, le cou tendu.
                    let bow = 0.3;
                    let turn = rotate_about(at, angle + bow);
                    let lowest = crate::animate::lowest_point(&turn, head) + shift.y;
                    let lower = (self.g.y - u - lowest).max(0.0);
                    angle += low * bow;
                    shift += Vec2::new(low * u, low * lower);
                }
                if self.graze > 0.0
                    && let Some(head) = art.carried[index]
                {
                    // Bouche au ras du sol, juste devant les pattes de devant.
                    let bow = 0.85 + 0.05 * f(p.nibble);
                    let turn = rotate_about(at, bow);
                    let lowest = [
                        Point::new(head.max_x(), head.max_y()),
                        Point::new(head.max_x(), head.min_y()),
                        Point::new(head.min_x(), head.max_y()),
                    ]
                    .into_iter()
                    .map(|corner| (turn * corner).y)
                    .fold(f64::NEG_INFINITY, f64::max);
                    let lower = (self.g.y - 1.5 * u - lowest).max(0.0);
                    angle += self.graze * bow;
                    shift += Vec2::new(
                        self.graze * 1.5 * u,
                        self.graze * (lower + 0.5 * u * f(p.nibble)),
                    );
                }
                // Vue de profil, la tête qui tourne paraît plus étroite.
                let middle = art.carried[index].map_or(at, |b| b.center());
                Affine::translate(shift)
                    * rotate_about(at, angle)
                    * scale_about(middle, 1.0 - act.head_squash, 1.0)
            }
            BoneRole::Ear => {
                // La première oreille (derrière) bouge un peu moins ; elles
                // tombent vers le bas, de quelque côté qu'elles partent.
                let k = if rank == 0 { 0.8 } else { 1.0 } * art.side[index];
                // Pendantes au calme et en mangeant ; asymétriques sous la
                // surprise (celle de derrière part de l'autre côté).
                let droop = 0.3 * sleep + 0.15 * dangle + 0.1 * self.graze + 0.55 * f(s.ears_droop);
                let pose = self.act.ear + ears_turn(self.act.ears_back);
                let split = if rank == 0 { -1.0 } else { 1.0 } * f(s.ear_split);
                parent
                    * rotate_about(
                        at,
                        k * (f(s.ear) + 0.05 * breath + droop + pose) + art.side[index] * split,
                    )
            }
            BoneRole::Eye => {
                if art.painted_lids {
                    parent
                } else {
                    let blink = f(s.blink);
                    let closed = (blink + self.act.eyes * (1.0 - blink)).min(1.0);
                    let open = (1.0 - 0.94 * closed).max(0.06);
                    parent * scale_about(at, 1.0, open)
                }
            }
            // Elles suivent le regard, et restent à l'horizontale quand la
            // tête penche (l'angle de la tête : celui du parent).
            BoneRole::Pupil => {
                let head = parent.as_coeffs()[1].atan2(parent.as_coeffs()[0]);
                parent
                    * Affine::translate((look.x * u * 2.6, look.y * u * 2.0))
                    * rotate_about(at, crate::animate::level_pupils(s, self.facing, head))
            }
            // Une queue en plusieurs os : le premier suit l'arrière du corps,
            // les suivants chacun le précédent, avec un temps de retard ;
            // couchée, elle se courbe sur le sol.
            BoneRole::Tail => {
                let bend = |angle: f64| angle + self.tail_bend.get(rank).copied().unwrap_or(0.0);
                match rank {
                    0 => {
                        Affine::translate((self.act.body_shift.x * u, self.bob + self.drop_back))
                            * rotate_about(at, bend(f(s.tail) - 0.2 * dangle + self.act.tail))
                    }
                    1 => parent * rotate_about(at, bend(f(s.tail_mid))),
                    _ => parent * rotate_about(at, bend(f(s.tail_tip))),
                }
            }
            // Elle s'ouvre vers le bas autour de sa charnière.
            BoneRole::Jaw => parent * rotate_about(at, 0.32 * f(s.jaw)),
            BoneRole::Neck => self.neck.map_or(parent, |n| n.neck),
            BoneRole::LegLower { front, near } => {
                parent * self.legs[leg_slot(front, near)].map_or(Affine::IDENTITY, |l| l.1)
            }
            // Elle se retrousse vers le haut, autour de son articulation.
            BoneRole::Lip => parent * rotate_about(at, -LIP_CURL * self.act.lip),
            BoneRole::Sway => parent * sway_shear(at, sway(s, &self.act, self.facing, rank.min(2))),
            BoneRole::Leg { front, near } if self.legs[leg_slot(front, near)].is_some() => {
                self.legs[leg_slot(front, near)].map_or(parent, |l| l.0)
            }
            BoneRole::Leg { front, near } => {
                let leg_len = (self.g.y - at.y).max(u * 8.0);
                let amplitude = (self.stride * self.duty / (2.0 * leg_len)).clamp(0.1, 0.75);
                let lift_amount = self.gait.lift(u);
                let moving = self.gait.moving();
                let cycle = self.phase + leg_phase(front, near, &self.gait);
                let (step, raised, support) = stepping(cycle, self.duty, amplitude, front);
                let mut angle = moving * step;
                // En l'air : repliées en montant, tendues vers le sol en descendant.
                angle += if front {
                    -0.15 * air * self.loose + self.tuck - 0.3 * self.reach
                } else {
                    0.15 * air * self.loose + 0.55 * self.tuck - 0.12 * self.reach
                } * (1.0 - self.leaping);
                angle += self.leaping * crate::animate::leap_leg(front, self.flight).0;
                angle += dangle * (if front { -0.1 } else { 0.12 } + f(s.swing) * 0.4);
                // À l'atterrissage, elles s'écartent ; couché, elles se replient.
                angle += if front { -self.splay } else { self.splay };
                angle += if front {
                    1.3 * self.front_down
                } else {
                    -1.25 * self.back_down
                };
                // Le geste (gratter le sol, taper du pied, s'étirer…).
                let (gesture_angle, gesture_lift) = self.act.legs[leg_slot(front, near)];
                angle += gesture_angle;
                let lift = raised * lift_amount + gesture_lift * u;
                let drop = if front {
                    self.drop_front
                } else {
                    self.drop_back
                };
                // En appui, la patte penchée s'allonge juste assez pour rester au sol.
                let planted = moving * support * (1.0 / (moving * step).cos().max(0.5) - 1.0);
                let stretch = 1.0 + planted + 0.07 * dangle + 0.04 * air * (1.0 - self.leaping);
                Affine::translate((self.act.body_shift.x * u, drop - lift))
                    * rotate_about(at, angle)
                    * scale_about(at, 1.0, stretch)
            }
        }
    }
}

/// Ajoute à `out` les morceaux du squelette, posés. `world` : du repère du
/// monde vers les pixels de la fenêtre ; `ground` : le point d'appui de
/// l'animal dans le monde ; `k` : points par pixel de la toile.
pub fn paint_skeleton(
    art: &SkeletonArt,
    pose: &SkeletonPose,
    blink: f32,
    world: Affine,
    ground: Point,
    k: f64,
    out: &mut DisplayList,
) {
    let base = world
        * Affine::translate(ground.to_vec2())
        * Affine::scale(k)
        * Affine::translate(-art.ground().to_vec2())
        * pose.root;
    let closed = blink >= 0.5;
    for piece in &art.pieces {
        match piece.shown {
            Shown::EyesOpen if closed => continue,
            Shown::EyesClosed if !closed => continue,
            _ => {}
        }
        let Some(bone) = pose.bones.get(piece.bone) else {
            continue;
        };
        out.push(Command::Image {
            bitmap: piece.bitmap.clone(),
            transform: base * *bone * piece.placement,
            alpha: 1.0,
            smooth: true,
        });
    }
}

#[cfg(test)]
mod tests {
    use std::f64::consts::TAU as CYCLE;

    use super::*;
    use mouflon_core::{PngData, parse_skeleton};

    /// Un PNG plein de `color` sur `w` × `h`, posé dans une toile plus grande
    /// (transparente ailleurs) à `(x, y)`.
    fn png(canvas: (u32, u32), rect: (u32, u32, u32, u32), color: [u8; 4]) -> PngData {
        let (cw, ch) = canvas;
        let mut rgba = vec![0u8; (cw * ch * 4) as usize];
        let (x, y, w, h) = rect;
        for yy in y..y + h {
            for xx in x..x + w {
                let i = ((yy * cw + xx) * 4) as usize;
                rgba[i..i + 4].copy_from_slice(&color);
            }
        }
        let mut out = Vec::new();
        {
            let mut encoder = png::Encoder::new(&mut out, cw, ch);
            encoder.set_color(png::ColorType::Rgba);
            encoder.set_depth(png::BitDepth::Eight);
            let mut writer = encoder.write_header().unwrap();
            writer.write_image_data(&rgba).unwrap();
        }
        PngData::from(out)
    }

    const TOML: &str = r#"
        size = [400, 300]
        ground = [200, 280]

        [[bone]]
        name = "racine"
        at = [200, 280]

        [[bone]]
        name = "corps"
        parent = "racine"
        at = [200, 180]
        role = "body"

        [[bone]]
        name = "tete"
        parent = "corps"
        at = [280, 150]
        role = "head"

        [[bone]]
        name = "patte"
        parent = "corps"
        at = [260, 210]
        role = "leg-front-near"

        [[bone]]
        name = "oeil"
        parent = "tete"
        at = [300, 130]
        role = "eye"

        [[piece]]
        bone = "corps"
        image = "corps.png"

        [[piece]]
        bone = "tete"
        image = "tete.png"

        [[piece]]
        bone = "patte"
        image = "patte.png"
    "#;

    fn art(toml: &str) -> SkeletonArt {
        let skeleton = parse_skeleton(toml, |name| {
            Ok(match name {
                // Le corps : de 120 à 280 en largeur, de 140 à 210 en hauteur.
                "corps.png" => png((400, 300), (120, 140, 160, 70), [240, 240, 240, 255]),
                "tete.png" => png((400, 300), (270, 110, 50, 50), [90, 80, 80, 255]),
                // La patte : de la hanche (260, 210) au sol (280).
                "patte.png" => png((400, 300), (255, 200, 10, 80), [60, 50, 50, 255]),
                // Une paupière fermée, sur l'œil.
                "ferme.png" => png((400, 300), (295, 128, 12, 4), [60, 50, 50, 255]),
                _ => png((10, 10), (0, 0, 1, 1), [0, 0, 0, 255]),
            })
        })
        .unwrap();
        SkeletonArt::load(Arc::new(skeleton), 60.0).unwrap()
    }

    #[test]
    fn pieces_are_cropped_and_reduced_to_the_screen() {
        let art = art(TOML);
        // De la tête (110) au sol (280) : 170 pixels pour 60 points.
        assert_eq!(art.bounds, Rect::new(120.0, 110.0, 320.0, 280.0));
        assert_eq!(art.pieces.len(), 3);
        // 60 / 170 points par pixel, 2,5 pixels par point : un peu réduit.
        let body = &art.pieces[0];
        assert_eq!((body.bitmap.width, body.bitmap.height), (142, 62));
        // Son coin se pose au coin de ce qu'il montre dans la toile.
        let corner = body.placement * Point::ZERO;
        assert!((corner - Point::new(120.0, 140.0)).hypot() < 1e-9);
        let far = body.placement * Point::new(142.0, 62.0);
        assert!((far - Point::new(280.0, 210.0)).hypot() < 1e-9);
    }

    #[test]
    fn at_rest_nothing_moves() {
        let art = art(TOML);
        let pose = art.pose(&PoseSignals::default(), 1.0, 0.0);
        assert!((pose.root * art.ground() - art.ground()).hypot() < 1e-9);
        for bone in &pose.bones {
            assert!((*bone * Point::new(250.0, 200.0) - Point::new(250.0, 200.0)).hypot() < 1e-6);
        }
    }

    #[test]
    fn walking_keeps_the_hooves_on_the_ground_and_swings_the_leg() {
        let art = art(TOML);
        let foot = Point::new(260.0, 280.0);
        let mut lowest = f64::NEG_INFINITY;
        let mut angles = Vec::new();
        for i in 0..24 {
            let signals = PoseSignals {
                walk: 1.0,
                gait_phase: (i as f64 / 24.0 * CYCLE) as f32,
                ..PoseSignals::default()
            };
            let pose = art.pose(&signals, 1.0, 0.0);
            let leg = pose.bones[3];
            let moved = leg * foot;
            lowest = lowest.max(moved.y);
            let hip = leg * Point::new(260.0, 210.0);
            angles.push((moved.x - hip.x).atan2(moved.y - hip.y));
        }
        // Le sabot ne passe jamais sous le sol, et la patte balaie d'avant en arrière.
        assert!(lowest <= 280.5, "{lowest}");
        let swing = angles.iter().cloned().fold(f64::NEG_INFINITY, f64::max)
            - angles.iter().cloned().fold(f64::INFINITY, f64::min);
        assert!(swing > 0.2, "{swing}");
    }

    #[test]
    fn a_nursing_skeleton_keeps_all_four_hooves_planted_while_breathing() {
        let path =
            std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../pets/solognote/pet.toml");
        let pet = crate::PetArt::load(&mouflon_core::load_pet(&path).unwrap()).unwrap();
        let (udder, muzzle) = pet.nursing_points().unwrap();
        let crate::Look::Skeleton(art) = pet.look else {
            panic!("squelette peint");
        };
        assert_eq!(art.legs.iter().flatten().count(), 4);
        for facing in [-1.0, 1.0] {
            let resting = art.pose(&PoseSignals::default(), facing, 0.0);
            for i in 0..24 {
                let signals = PoseSignals {
                    gesture: Some(Gesture::Nurse),
                    gesture_amount: 1.0,
                    gesture_time: 1.0 + i as f32 / 60.0,
                    nursing_target: Some(mouflon_core::Vec2::new(muzzle.x, udder.y / 0.58)),
                    breath: (i as f32 * 0.2).sin(),
                    ..PoseSignals::default()
                };
                let posed = art.pose(&signals, facing, 0.0);
                for (_, lower, leg) in art.legs.iter().flatten() {
                    let rest = resting.root * resting.bones[*lower] * leg.chain.tip;
                    let at = posed.root * posed.bones[*lower] * leg.chain.tip;
                    assert!(
                        (at - rest).hypot() < 0.03,
                        "sabot {lower}, sens {facing} : {at:?} au lieu de {rest:?}"
                    );
                }
            }
        }
    }

    #[test]
    fn structural_keyframes_return_smoothly_when_nursing_ends() {
        let toml = format!(
            r#"{TOML}
            [[clip]]
            name = "calme"
            length = 2.0
            [clip.bones.racine]
            move = [[0, 12, -8]]
            scale = [[0, 1.1, 0.95]]
            [clip.bones.corps]
            rotate = [[0, 20]]
            [motions]
            idle = "calme"
            "#
        );
        let keyed = art(&toml);
        let plain = art(TOML);
        let point = Point::new(250.0, 200.0);
        for facing in [-1.0, 1.0] {
            let signals = PoseSignals {
                gesture: Some(Gesture::Nurse),
                gesture_amount: 1.0,
                gesture_time: 2.0,
                ..PoseSignals::default()
            };
            let full = keyed.pose(&signals, facing, 0.0);
            let no_keys = plain.pose(&signals, facing, 0.0);
            let mut previous = full.root * full.bones[1] * point;
            assert!((previous - no_keys.root * no_keys.bones[1] * point).hypot() < 1e-6);
            let resting = keyed.pose(&PoseSignals::default(), facing, 0.0);
            let rest = resting.root * resting.bones[1] * point;
            assert!(
                (rest - previous).hypot() > 5.0,
                "les clés déplacent le corps"
            );
            for step in 1..=100 {
                let pose = keyed.pose(
                    &PoseSignals {
                        gesture_amount: 1.0 - step as f32 / 100.0,
                        ..signals
                    },
                    facing,
                    0.0,
                );
                let at = pose.root * pose.bones[1] * point;
                assert!((at - previous).hypot() < 2.0, "sortie {step}");
                previous = at;
            }
            assert!(
                (previous - rest).hypot() < 1e-6,
                "aucun saut à la fin du geste"
            );
        }
    }

    #[test]
    fn the_head_follows_the_body_and_bows_to_graze() {
        let art = art(TOML);
        let signals = PoseSignals {
            graze: 1.0,
            ..PoseSignals::default()
        };
        let pose = art.pose(&signals, 1.0, 0.0);
        // Le bas du museau descend près du sol.
        let muzzle = pose.bones[2] * Point::new(320.0, 160.0);
        assert!(muzzle.y > 240.0, "{muzzle:?}");
        // L'œil suit la tête.
        let eye = pose.bones[4] * Point::new(300.0, 130.0);
        let head = pose.bones[2] * Point::new(300.0, 130.0);
        assert!((eye - head).hypot() < 1e-6);
    }

    #[test]
    fn a_skeleton_keeps_its_slit_pupils_level_while_grazing() {
        let toml = format!(
            "{TOML}
            [[bone]]
            name = \"pupille\"
            parent = \"oeil\"
            at = [300, 130]
            role = \"pupil\"
            "
        );
        let art = art(&toml);
        let signals = PoseSignals {
            graze: 1.0,
            ..PoseSignals::default()
        };
        let pose = art.pose(&signals, 1.0, 0.0);
        let tilt = |t: Affine| {
            let [a, b, ..] = t.as_coeffs();
            (b / a).atan()
        };
        assert!(tilt(pose.bones[2]).abs() > 0.5, "la tête broute");
        assert!(tilt(pose.bones[5]).abs() < 0.02, "{}", tilt(pose.bones[5]));
    }

    #[test]
    fn a_sleeping_skeleton_lays_its_head_down() {
        let art = art(TOML);
        let muzzle = |sleep_low: f32| {
            let signals = PoseSignals {
                sit: 1.0,
                sleep: 1.0,
                sleep_low,
                ..PoseSignals::default()
            };
            let pose = art.pose(&signals, 1.0, 0.0);
            (pose.bones[2] * Point::new(320.0, 160.0)).y
        };
        let (up, down) = (muzzle(0.0), muzzle(1.0));
        assert!(down > up + 10.0, "{up} {down}");
        assert!(down <= 281.0, "museau sous le sol : {down}");
    }

    #[test]
    fn keyed_animations_replace_the_engine_for_their_motion() {
        let toml = format!(
            "{TOML}
            [[clip]]
            name = \"marche\"
            [clip.bones.patte]
            rotate = [[0, 30], [0.5, -30], [1, 30]]

            [motions]
            walk = \"marche\"
            "
        );
        let art = art(&toml);
        let at = |phase: f64| {
            let signals = PoseSignals {
                walk: 1.0,
                gait_phase: (phase * CYCLE) as f32,
                ..PoseSignals::default()
            };
            let pose = art.pose(&signals, 1.0, 0.0);
            let foot = pose.bones[3] * Point::new(260.0, 280.0);
            let hip = pose.bones[3] * Point::new(260.0, 210.0);
            (foot.x - hip.x).atan2(foot.y - hip.y)
        };
        // 30° dans le sens horaire au départ : le sabot part vers l'arrière.
        assert!(
            (at(0.0).to_degrees() + 30.0).abs() < 0.5,
            "{}",
            at(0.0).to_degrees()
        );
        assert!(
            (at(0.5).to_degrees() - 30.0).abs() < 0.5,
            "{}",
            at(0.5).to_degrees()
        );
        // Le corps ne monte plus et ne descend plus de lui-même.
        let signals = PoseSignals {
            walk: 1.0,
            gait_phase: 0.8,
            ..PoseSignals::default()
        };
        let pose = art.pose(&signals, 1.0, 0.0);
        assert!(
            (pose.bones[1] * Point::new(200.0, 180.0) - Point::new(200.0, 180.0)).hypot() < 1e-6
        );
    }

    #[test]
    fn painted_lids_replace_squashed_eyes() {
        let lids = TOML.replace(
            "[[piece]]\n        bone = \"patte\"",
            "[[piece]]\n        bone = \"oeil\"\n        image = \"ferme.png\"\n        eyes = \"closed\"\n\n        [[piece]]\n        bone = \"patte\"",
        );
        let art = art(&lids);
        assert!(art.painted_lids);
        let signals = PoseSignals {
            blink: 1.0,
            ..PoseSignals::default()
        };
        let pose = art.pose(&signals, 1.0, 0.0);
        // L'œil ne s'écrase pas : la paupière peinte se montre.
        let top = pose.bones[4] * Point::new(300.0, 120.0);
        assert!((top.y - 120.0).abs() < 1e-6);
        let mut out = DisplayList::new(100, 100);
        paint_skeleton(
            &art,
            &pose,
            1.0,
            Affine::IDENTITY,
            Point::new(50.0, 90.0),
            0.3,
            &mut out,
        );
        assert_eq!(out.commands.len(), 4);
        let mut open = DisplayList::new(100, 100);
        paint_skeleton(
            &art,
            &pose,
            0.0,
            Affine::IDENTITY,
            Point::new(50.0, 90.0),
            0.3,
            &mut open,
        );
        assert_eq!(open.commands.len(), 3);
    }
}
