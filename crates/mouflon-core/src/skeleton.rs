//! Squelettes à morceaux peints : un personnage fait de morceaux d'image
//! (peints à la main, dans Blender avec Grease Pencil, par une IA…) posés sur
//! des os, animés par des animations à clés et par le moteur.
//!
//! Un squelette se décrit dans un fichier TOML (voir `docs/squelette.md`) :
//!
//! - une **toile** de référence (pixels) où tout est dessiné de profil,
//!   tourné vers la droite, et le point où les pattes touchent le sol ;
//! - des **os** : chacun a son articulation dans la toile, un parent (sauf
//!   le premier), et peut-être un rôle que le moteur anime de lui-même
//!   (regard, clignements, respiration, oreilles, queue) ;
//! - des **morceaux**, de l'arrière vers l'avant : une image chacun, posée
//!   sur un os (l'image fait la taille de la toile, ou se place avec `at`) ;
//! - des **animations** à clés : pour chaque os, des angles, des
//!   déplacements et des échelles à des instants donnés, lissés entre eux ;
//! - ce que l'animal **joue** selon ce qu'il fait : au pas, au galop, assis,
//!   endormi, en broutant, en l'air, tenu, au calme.
//!
//! Ce module lit et interpole ; le dessin est dans `mouflon-render`.

use std::collections::{BTreeMap, HashMap};

use serde::Deserialize;

use crate::def::PngData;
use crate::math::Vec2;

/// Ce que le moteur fait d'un os de lui-même, en plus des animations.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum BoneRole {
    /// Le corps : il respire.
    Body,
    /// La tête : elle suit le curseur du regard, se penche en dormant.
    Head,
    /// Une oreille : elle bat en tombant, frémit, tombe en dormant.
    Ear,
    /// La queue : elle balance et bat de temps en temps.
    Tail,
    /// Un œil : il cligne, se ferme en dormant (il s'écrase vers son
    /// articulation, au milieu de l'œil).
    Eye,
    /// Une pupille : elle suit le curseur.
    Pupil,
    /// Une patte (de devant ou de derrière, du côté qu'on voit ou de
    /// l'autre) : sans animation de marche, le moteur la fait marcher, en
    /// tournant autour de sa hanche (l'articulation de l'os).
    Leg { front: bool, near: bool },
    /// La mâchoire du bas : elle s'ouvre autour de son articulation (la
    /// charnière, près de la joue) quand il mâche, bêle ou bâille.
    Jaw,
    /// L'encolure, entre le corps et la tête (la tête est son enfant) : elle
    /// tourne autour de sa base, devant le garrot ; pour brouter, c'est
    /// surtout elle qui descend.
    Neck,
    /// Le bas d'une patte (enfant de sa patte) : le canon et le sabot, qui
    /// plient au carpe (devant) ou au jarret (derrière), son articulation.
    LegLower { front: bool, near: bool },
    /// La lèvre supérieure (enfant de la tête) : elle se retrousse pour le
    /// flehmen, autour de son articulation, sous le naseau.
    Lip,
    /// Une mèche de la toison : elle suit le corps avec un ressort amorti.
    /// Plusieurs os `sway` : les suivants traînent un peu plus.
    Sway,
}

impl BoneRole {
    fn parse(name: &str) -> Option<Self> {
        Some(match name {
            "body" => Self::Body,
            "head" => Self::Head,
            "ear" => Self::Ear,
            "tail" => Self::Tail,
            "eye" => Self::Eye,
            "pupil" => Self::Pupil,
            "jaw" => Self::Jaw,
            "neck" => Self::Neck,
            "lip" => Self::Lip,
            "sway" => Self::Sway,
            "leg-front-near-lower" => Self::LegLower {
                front: true,
                near: true,
            },
            "leg-front-far-lower" => Self::LegLower {
                front: true,
                near: false,
            },
            "leg-back-near-lower" => Self::LegLower {
                front: false,
                near: true,
            },
            "leg-back-far-lower" => Self::LegLower {
                front: false,
                near: false,
            },
            "leg-front-near" => Self::Leg {
                front: true,
                near: true,
            },
            "leg-front-far" => Self::Leg {
                front: true,
                near: false,
            },
            "leg-back-near" => Self::Leg {
                front: false,
                near: true,
            },
            "leg-back-far" => Self::Leg {
                front: false,
                near: false,
            },
            _ => return None,
        })
    }
}

/// Un os : son articulation dans la toile (pixels), son parent.
#[derive(Clone, Debug, PartialEq)]
pub struct Bone {
    pub name: String,
    pub parent: Option<usize>,
    pub at: Vec2,
    pub role: Option<BoneRole>,
}

/// Quand un morceau se montre.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum Shown {
    #[default]
    Always,
    /// Les yeux ouverts (un œil peint).
    EyesOpen,
    /// Les yeux fermés (paupières peintes) : sans ces morceaux, un œil se
    /// ferme en s'écrasant.
    EyesClosed,
}

/// Un morceau : son image (PNG), posée dans la toile à `at` (coin haut
/// gauche, pixels), sur l'os `bone`.
#[derive(Clone, Debug)]
pub struct Piece {
    pub bone: usize,
    pub image: String,
    pub png: PngData,
    pub at: Vec2,
    pub shown: Shown,
}

/// Une clé : un instant (fraction de l'animation, de 0 à 1) et une valeur.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Key<T> {
    pub t: f32,
    pub value: T,
}

/// Ce qu'une animation fait d'un os.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Track {
    pub bone: usize,
    /// Angle (radians, sens horaire de l'écran).
    pub rotate: Vec<Key<f32>>,
    /// Déplacement (pixels de la toile).
    pub moves: Vec<Key<Vec2>>,
    /// Échelle (1 : la taille dessinée).
    pub scale: Vec<Key<Vec2>>,
}

/// Une animation à clés.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Clip {
    pub name: String,
    /// Durée d'un tour (secondes) quand elle tourne sur l'horloge ; au pas et
    /// au galop, un tour est une foulée, quelle que soit la vitesse.
    pub length: f32,
    pub tracks: Vec<Track>,
}

/// L'écart d'un os à sa pose dessinée.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct BonePose {
    pub rotate: f32,
    pub moves: Vec2,
    pub scale: Vec2,
}

impl Default for BonePose {
    fn default() -> Self {
        Self {
            rotate: 0.0,
            moves: Vec2::ZERO,
            scale: Vec2::new(1.0, 1.0),
        }
    }
}

impl BonePose {
    /// Entre `self` (0) et `other` (1).
    #[must_use]
    pub fn lerp(self, other: Self, t: f32) -> Self {
        Self {
            rotate: self.rotate + (other.rotate - self.rotate) * t,
            moves: self.moves.lerp(other.moves, t),
            scale: self.scale.lerp(other.scale, t),
        }
    }
}

/// Valeurs qu'on sait lisser entre deux clés.
pub trait Animatable: Copy {
    fn mix(a: Self, b: Self, t: f32) -> Self;
}

impl Animatable for f32 {
    fn mix(a: Self, b: Self, t: f32) -> Self {
        a + (b - a) * t
    }
}

impl Animatable for Vec2 {
    fn mix(a: Self, b: Self, t: f32) -> Self {
        a.lerp(b, t)
    }
}

/// La valeur des clés à l'instant `t` (de 0 à 1, en boucle) : lissée entre
/// deux clés (départ et arrivée en douceur), la première avant la première
/// clé et la dernière après la dernière.
#[must_use]
pub fn sample<T: Animatable>(keys: &[Key<T>], t: f32) -> Option<T> {
    let first = keys.first()?;
    let last = keys.last()?;
    let t = t.rem_euclid(1.0);
    if t <= first.t {
        return Some(first.value);
    }
    if t >= last.t {
        return Some(last.value);
    }
    let i = keys.windows(2).position(|w| t >= w[0].t && t <= w[1].t)?;
    let (a, b) = (keys[i], keys[i + 1]);
    let span = (b.t - a.t).max(1e-6);
    let u = ((t - a.t) / span).clamp(0.0, 1.0);
    let smooth = u * u * (3.0 - 2.0 * u);
    Some(T::mix(a.value, b.value, smooth))
}

impl Clip {
    /// Ce qu'elle fait de chaque os à l'instant `t` (fraction d'un tour).
    #[must_use]
    pub fn pose(&self, bones: usize, t: f32) -> Vec<BonePose> {
        let mut out = vec![BonePose::default(); bones];
        for track in &self.tracks {
            let Some(pose) = out.get_mut(track.bone) else {
                continue;
            };
            if let Some(r) = sample(&track.rotate, t) {
                pose.rotate = r;
            }
            if let Some(m) = sample(&track.moves, t) {
                pose.moves = m;
            }
            if let Some(s) = sample(&track.scale, t) {
                pose.scale = s;
            }
        }
        out
    }

    /// Mêle l'animation à `poses`, avec le poids `weight` (0 : rien ne
    /// change, 1 : elle seule), à l'instant `t` : seuls les os et les
    /// mouvements (angle, déplacement, échelle) qu'elle anime changent, le
    /// reste garde ce que les animations d'avant y ont mis.
    pub fn blend_into(&self, t: f32, weight: f32, poses: &mut [BonePose]) {
        let w = weight.clamp(0.0, 1.0);
        if w <= 0.0 {
            return;
        }
        for track in &self.tracks {
            let Some(pose) = poses.get_mut(track.bone) else {
                continue;
            };
            if let Some(r) = sample(&track.rotate, t) {
                pose.rotate += (r - pose.rotate) * w;
            }
            if let Some(m) = sample(&track.moves, t) {
                pose.moves = pose.moves.lerp(m, w);
            }
            if let Some(s) = sample(&track.scale, t) {
                pose.scale = pose.scale.lerp(s, w);
            }
        }
    }
}

/// Ce que l'animal joue selon ce qu'il fait (rangs dans
/// [`Skeleton::clips`]).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Motions {
    pub idle: Option<usize>,
    pub walk: Option<usize>,
    pub run: Option<usize>,
    pub sit: Option<usize>,
    pub sleep: Option<usize>,
    pub graze: Option<usize>,
    pub air: Option<usize>,
    pub dangle: Option<usize>,
}

/// Un squelette à morceaux peints.
#[derive(Clone, Debug)]
pub struct Skeleton {
    /// Taille de la toile de référence (pixels).
    pub size: Vec2,
    /// Où les pattes touchent le sol, dans la toile.
    pub ground: Vec2,
    /// Ce qui touche l'herbe quand il broute (le bout du museau, sous les
    /// naseaux), s'il est donné ; sinon, le moteur le devine d'après la tête.
    pub muzzle: Option<Vec2>,
    /// Les os, chaque parent avant ses enfants.
    pub bones: Vec<Bone>,
    /// Les morceaux, de l'arrière vers l'avant.
    pub pieces: Vec<Piece>,
    pub clips: Vec<Clip>,
    pub motions: Motions,
}

impl Skeleton {
    /// L'os de ce nom.
    #[must_use]
    pub fn bone(&self, name: &str) -> Option<usize> {
        self.bones.iter().position(|b| b.name == name)
    }
}

/// Le fichier du squelette tel qu'il s'écrit.
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct SkeletonToml {
    size: [f32; 2],
    ground: [f32; 2],
    muzzle: Option<[f32; 2]>,
    #[serde(default, rename = "bone")]
    bones: Vec<BoneToml>,
    #[serde(default, rename = "piece")]
    pieces: Vec<PieceToml>,
    #[serde(default, rename = "clip")]
    clips: Vec<ClipToml>,
    #[serde(default)]
    motions: BTreeMap<String, String>,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct BoneToml {
    name: String,
    parent: Option<String>,
    at: [f32; 2],
    role: Option<String>,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct PieceToml {
    bone: String,
    image: String,
    #[serde(default)]
    at: [f32; 2],
    /// "open" ou "closed" : seulement les yeux ouverts, ou fermés.
    eyes: Option<String>,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct ClipToml {
    name: String,
    #[serde(default = "one")]
    length: f32,
    #[serde(default)]
    bones: BTreeMap<String, TrackToml>,
}

fn one() -> f32 {
    1.0
}

#[derive(Deserialize, Default)]
#[serde(deny_unknown_fields)]
struct TrackToml {
    /// `[instant, degrés]`.
    #[serde(default)]
    rotate: Vec<[f32; 2]>,
    /// `[instant, x, y]`.
    #[serde(default, rename = "move")]
    moves: Vec<[f32; 3]>,
    /// `[instant, échelle]` ou `[instant, en largeur, en hauteur]`.
    #[serde(default)]
    scale: Vec<Vec<f32>>,
}

/// Ce qui ne va pas dans un fichier de squelette.
#[derive(Debug, thiserror::Error)]
pub enum SkeletonError {
    #[error("squelette illisible : {0}")]
    Toml(String),
    #[error("squelette : {0}")]
    Invalid(String),
}

/// Lit un squelette ; `read` donne une image par son nom.
pub fn parse_skeleton(
    text: &str,
    mut read: impl FnMut(&str) -> Result<PngData, String>,
) -> Result<Skeleton, SkeletonError> {
    let file: SkeletonToml =
        toml::from_str(text).map_err(|e| SkeletonError::Toml(e.to_string()))?;
    let bad = |m: String| SkeletonError::Invalid(m);
    if file.bones.is_empty() {
        return Err(bad("il faut au moins un os ([[bone]])".into()));
    }
    let mut index: HashMap<&str, usize> = HashMap::new();
    let mut bones = Vec::new();
    for (i, b) in file.bones.iter().enumerate() {
        if index.insert(b.name.as_str(), i).is_some() {
            return Err(bad(format!("l'os « {} » est défini deux fois", b.name)));
        }
        let parent = match &b.parent {
            None if i == 0 => None,
            None => {
                return Err(bad(format!(
                    "l'os « {} » n'a pas de parent (seul le premier en est dispensé)",
                    b.name
                )));
            }
            Some(p) => Some(*index.get(p.as_str()).ok_or_else(|| {
                bad(format!(
                    "l'os « {} » a pour parent « {p} », qui n'est pas défini avant lui",
                    b.name
                ))
            })?),
        };
        let role = match &b.role {
            None => None,
            Some(r) => Some(BoneRole::parse(r).ok_or_else(|| {
                bad(format!(
                    "rôle inconnu « {r} » (body, neck, head, ear, tail, eye, pupil, \
                     jaw, lip, sway, leg-front-near, leg-front-far, leg-back-near, \
                     leg-back-far, et leurs bas : leg-front-near-lower…)"
                ))
            })?),
        };
        bones.push(Bone {
            name: b.name.clone(),
            parent,
            at: Vec2::new(b.at[0], b.at[1]),
            role,
        });
    }
    let bone_of = |name: &str, what: &str| {
        index
            .get(name)
            .copied()
            .ok_or_else(|| bad(format!("{what} : l'os « {name} » n'existe pas")))
    };
    let mut pieces = Vec::new();
    for p in &file.pieces {
        let bone = bone_of(&p.bone, &format!("morceau « {} »", p.image))?;
        let shown = match p.eyes.as_deref() {
            None => Shown::Always,
            Some("open") => Shown::EyesOpen,
            Some("closed") => Shown::EyesClosed,
            Some(other) => {
                return Err(bad(format!(
                    "morceau « {} » : `eyes = \"{other}\"` inconnu (\"open\" ou \"closed\")",
                    p.image
                )));
            }
        };
        let png = read(&p.image).map_err(|e| bad(format!("{} : {e}", p.image)))?;
        pieces.push(Piece {
            bone,
            image: p.image.clone(),
            png,
            at: Vec2::new(p.at[0], p.at[1]),
            shown,
        });
    }
    let mut clips = Vec::new();
    for c in &file.clips {
        let mut tracks = Vec::new();
        for (name, t) in &c.bones {
            let bone = bone_of(name, &format!("animation « {} »", c.name))?;
            let scale = t
                .scale
                .iter()
                .map(|k| match k.as_slice() {
                    [t, s] => Ok(Key {
                        t: *t,
                        value: Vec2::new(*s, *s),
                    }),
                    [t, sx, sy] => Ok(Key {
                        t: *t,
                        value: Vec2::new(*sx, *sy),
                    }),
                    _ => Err(bad(format!(
                        "animation « {} », os « {name} » : une échelle s'écrit [instant, échelle] ou [instant, largeur, hauteur]",
                        c.name
                    ))),
                })
                .collect::<Result<Vec<_>, _>>()?;
            let mut track = Track {
                bone,
                rotate: t
                    .rotate
                    .iter()
                    .map(|[t, deg]| Key {
                        t: *t,
                        value: deg.to_radians(),
                    })
                    .collect(),
                moves: t
                    .moves
                    .iter()
                    .map(|[t, x, y]| Key {
                        t: *t,
                        value: Vec2::new(*x, *y),
                    })
                    .collect(),
                scale,
            };
            track.rotate.sort_by(|a, b| a.t.total_cmp(&b.t));
            track.moves.sort_by(|a, b| a.t.total_cmp(&b.t));
            track.scale.sort_by(|a, b| a.t.total_cmp(&b.t));
            tracks.push(track);
        }
        if c.length <= 0.0 {
            return Err(bad(format!(
                "animation « {} » : `length` doit être positif",
                c.name
            )));
        }
        clips.push(Clip {
            name: c.name.clone(),
            length: c.length,
            tracks,
        });
    }
    let clip_of = |state: &str| -> Result<Option<usize>, SkeletonError> {
        match file.motions.get(state) {
            None => Ok(None),
            Some(name) => clips
                .iter()
                .position(|c| c.name == *name)
                .map(Some)
                .ok_or_else(|| {
                    bad(format!(
                        "[motions] {state} : l'animation « {name} » n'existe pas"
                    ))
                }),
        }
    };
    for state in file.motions.keys() {
        if !matches!(
            state.as_str(),
            "idle" | "walk" | "run" | "sit" | "sleep" | "graze" | "air" | "dangle"
        ) {
            return Err(bad(format!(
                "[motions] : « {state} » inconnu (idle, walk, run, sit, sleep, graze, air, dangle)"
            )));
        }
    }
    let motions = Motions {
        idle: clip_of("idle")?,
        walk: clip_of("walk")?,
        run: clip_of("run")?,
        sit: clip_of("sit")?,
        sleep: clip_of("sleep")?,
        graze: clip_of("graze")?,
        air: clip_of("air")?,
        dangle: clip_of("dangle")?,
    };
    Ok(Skeleton {
        size: Vec2::new(file.size[0], file.size[1]),
        ground: Vec2::new(file.ground[0], file.ground[1]),
        muzzle: file.muzzle.map(|[x, y]| Vec2::new(x, y)),
        bones,
        pieces,
        clips,
        motions,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    const SKELETON: &str = r#"
        size = [200, 100]
        ground = [100, 90]

        [[bone]]
        name = "corps"
        at = [100, 60]
        role = "body"

        [[bone]]
        name = "patte"
        parent = "corps"
        at = [120, 70]

        [[piece]]
        bone = "corps"
        image = "corps.png"

        [[piece]]
        bone = "patte"
        image = "patte.png"
        at = [110, 60]

        [[clip]]
        name = "marche"
        [clip.bones.patte]
        rotate = [[0, 20], [0.5, -20], [1, 20]]
        [clip.bones.corps]
        move = [[0, 0, 0], [0.5, 0, -4], [1, 0, 0]]
        scale = [[0, 1], [0.5, 1.1, 0.9]]

        [motions]
        walk = "marche"
    "#;

    fn read(name: &str) -> Result<PngData, String> {
        Ok(PngData::from(name.as_bytes().to_vec()))
    }

    #[test]
    fn a_skeleton_names_its_neck_lower_legs_lip_and_locks() {
        let text = r#"
            size = [100, 100]
            ground = [50, 95]
            muzzle = [90, 70]
            [[bone]]
            name = "racine"
            at = [50, 95]
            [[bone]]
            name = "corps"
            parent = "racine"
            at = [50, 50]
            role = "body"
            [[bone]]
            name = "encolure"
            parent = "corps"
            at = [70, 45]
            role = "neck"
            [[bone]]
            name = "levre"
            parent = "encolure"
            at = [85, 60]
            role = "lip"
            [[bone]]
            name = "meche"
            parent = "corps"
            at = [50, 70]
            role = "sway"
            [[bone]]
            name = "patte"
            parent = "corps"
            at = [70, 65]
            role = "leg-front-far"
            [[bone]]
            name = "patte-bas"
            parent = "patte"
            at = [70, 80]
            role = "leg-front-far-lower"
        "#;
        let skeleton = parse_skeleton(text, |_| Err("pas d'image".into())).unwrap();
        let roles: Vec<Option<BoneRole>> = skeleton.bones.iter().map(|b| b.role).collect();
        assert_eq!(roles[2], Some(BoneRole::Neck));
        assert_eq!(roles[3], Some(BoneRole::Lip));
        assert_eq!(roles[4], Some(BoneRole::Sway));
        assert_eq!(
            roles[6],
            Some(BoneRole::LegLower {
                front: true,
                near: false
            })
        );
        assert_eq!(skeleton.muzzle, Some(Vec2::new(90.0, 70.0)));
    }

    #[test]
    fn reads_bones_pieces_clips_and_motions() {
        let s = parse_skeleton(SKELETON, read).unwrap();
        assert_eq!(s.bones.len(), 2);
        assert_eq!(s.bones[1].parent, Some(0));
        assert_eq!(s.bones[0].role, Some(BoneRole::Body));
        assert_eq!(s.pieces[1].at, Vec2::new(110.0, 60.0));
        assert_eq!(&*s.pieces[0].png.bytes().unwrap(), b"corps.png");
        assert_eq!(s.motions.walk, Some(0));
        assert_eq!(s.motions.run, None);
        let mid = s.clips[0].pose(2, 0.5);
        assert!((mid[1].rotate + 20f32.to_radians()).abs() < 1e-5);
        assert_eq!(mid[0].moves, Vec2::new(0.0, -4.0));
        assert_eq!(mid[0].scale, Vec2::new(1.1, 0.9));
        // Lissé entre deux clés, en boucle.
        let quarter = s.clips[0].pose(2, 0.25)[1].rotate;
        assert!(quarter.abs() < 1e-5, "{quarter}");
        let again = s.clips[0].pose(2, 1.5)[1].rotate;
        assert!((again - mid[1].rotate).abs() < 1e-5);
    }

    #[test]
    fn blending_touches_only_what_the_clip_animates() {
        let s = parse_skeleton(SKELETON, read).unwrap();
        let mut poses = vec![BonePose::default(); 2];
        poses[0].rotate = 0.3;
        poses[1].moves = Vec2::new(5.0, 0.0);
        s.clips[0].blend_into(0.5, 0.5, &mut poses);
        // Le corps : son angle reste (l'animation ne le tourne pas), il
        // descend à moitié (déplacement animé : -4 au milieu).
        assert!((poses[0].rotate - 0.3).abs() < 1e-6);
        assert_eq!(poses[0].moves, Vec2::new(0.0, -2.0));
        // La patte : à mi-chemin de son angle animé, son déplacement reste.
        assert!((poses[1].rotate + 10f32.to_radians()).abs() < 1e-5);
        assert_eq!(poses[1].moves, Vec2::new(5.0, 0.0));
    }

    #[test]
    fn explains_what_is_wrong() {
        let err = |text: &str| parse_skeleton(text, read).unwrap_err().to_string();
        assert!(err("size = [1, 1]\nground = [0, 0]").contains("au moins un os"));
        let orphan = "size = [1, 1]\nground = [0, 0]\n[[bone]]\nname = \"a\"\nat = [0, 0]\n[[bone]]\nname = \"b\"\nat = [0, 0]";
        assert!(err(orphan).contains("n'a pas de parent"));
        let unknown = SKELETON.replace("parent = \"corps\"", "parent = \"tete\"");
        assert!(err(&unknown).contains("n'est pas défini avant lui"));
        let motion = SKELETON.replace("walk = \"marche\"", "walk = \"course\"");
        assert!(err(&motion).contains("course"));
        let role = SKELETON.replace("role = \"body\"", "role = \"aile\"");
        assert!(err(&role).contains("rôle inconnu"));
    }
}
