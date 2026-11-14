//! Format natif des animaux Mouflon : un fichier `pet.toml` à côté de ses
//! ressources (dessin SVG, planche ou bandes d'images).
//!
//! ```toml
//! [pet]
//! id = "bleu-du-maine"
//! name = "Bleu du Maine"
//!
//! [appearance]
//! rig = "bleu-du-maine.svg"
//! height = 64.0
//!
//! [reflexes]
//! spawn = "fall"
//! land = "land"
//!
//! [[behavior]]
//! name = "stand"
//! motion = "stand"
//! duration = [1.5, 4.0]
//! next = [{ to = "walk", weight = 3 }, { to = "sit" }]
//! ```
//!
//! Un animal en images a une bande par animation (et par sens) ; chaque
//! comportement joue la sienne et avance image par image :
//!
//! ```toml
//! [appearance]
//! strips_left = "planches/gauche/{name}.png"
//! strips_right = "planches/droite/{name}.png"
//! frame = [256, 256]
//! scale = 0.375
//! anchor = [128, 226]
//!
//! [[behavior]]
//! name = "marche"
//! fps = 10
//! loops = [3, 6]
//! move = [5.5, 0]
//! ```
//!
//! Un `pet.toml` peut aussi reprendre les comportements d'un fichier desktopPet
//! avec une planche redessinée (plus fine, même disposition) :
//!
//! ```toml
//! [pet]
//! id = "mouton"
//! name = "Mouton"
//!
//! [appearance]
//! sprites = "mouton-4k.png"
//! tiles = [16, 11]
//! scale = 0.25
//!
//! [desktoppet]
//! animations = "animations.xml"
//! ```

use std::collections::{BTreeMap, HashMap};
use std::path::{Path, PathBuf};
use std::sync::Arc;

use serde::Deserialize;

use crate::def::{
    Age, Appearance, Behavior, Condition, DefError, Drawn, Duration, Facings, Gesture, Inbetweens,
    Motion, PetDef, PetMeta, Physics, PngData, Reflexes, Script, Sex, SexRatio, Sound, SoundCue,
    SpriteClip, SpritePage, SpriteSheet, Transition, Variant, VariantDrawing, png_size,
};
use crate::expr::Expr;
use crate::math::Vec2;
use crate::temper::Temper;

#[derive(Debug, thiserror::Error)]
pub enum LoadError {
    #[error("impossible de lire {path}: {source}")]
    Io {
        path: PathBuf,
        source: std::io::Error,
    },
    #[error("fichier TOML invalide : {0}")]
    Toml(#[from] toml::de::Error),
    #[error("fichier XML desktopPet invalide : {0}")]
    Xml(String),
    #[error("{0}")]
    Format(String),
    #[error(transparent)]
    Def(#[from] DefError),
}

fn format_error(message: impl Into<String>) -> LoadError {
    LoadError::Format(message.into())
}

/// Charge un animal depuis un `pet.toml` ou un XML desktopPet.
pub fn load_pet(path: &Path) -> Result<PetDef, LoadError> {
    let bytes = read_file(path)?;
    let text = String::from_utf8(bytes)
        .map_err(|_| format_error(format!("{} n'est pas en UTF-8", path.display())))?;
    match path
        .extension()
        .and_then(|e| e.to_str())
        .map(str::to_ascii_lowercase)
        .as_deref()
    {
        Some("xml") => crate::desktoppet::import(text.trim_start_matches('\u{feff}')),
        _ => {
            let base = path.parent().unwrap_or_else(|| Path::new("."));
            parse_pet_toml(&text, Folder { base })
        }
    }
}

fn read_file(path: &Path) -> Result<Vec<u8>, LoadError> {
    std::fs::read(path).map_err(|source| LoadError::Io {
        path: path.to_path_buf(),
        source,
    })
}

/// Ressources nommées dans un `pet.toml` (images, sons, autres fichiers).
pub trait Resources {
    /// Contenu complet d'une ressource.
    fn read(&self, name: &str) -> Result<Vec<u8>, LoadError>;

    /// Une planche PNG. Par défaut lue tout de suite ; un dossier ne lit que
    /// son en-tête, et le reste au décodage des images.
    fn png(&self, name: &str) -> Result<PngData, LoadError> {
        Ok(PngData::from(self.read(name)?))
    }
}

/// Une fonction qui lit chaque ressource (essais, animaux en mémoire).
impl<F: Fn(&str) -> Result<Vec<u8>, LoadError>> Resources for F {
    fn read(&self, name: &str) -> Result<Vec<u8>, LoadError> {
        self(name)
    }
}

/// Le dossier d'un `pet.toml`.
struct Folder<'a> {
    base: &'a Path,
}

impl Resources for Folder<'_> {
    fn read(&self, name: &str) -> Result<Vec<u8>, LoadError> {
        read_file(&self.base.join(name))
    }

    fn png(&self, name: &str) -> Result<PngData, LoadError> {
        use std::io::Read as _;
        let path = self.base.join(name);
        let io = |source| LoadError::Io {
            path: path.clone(),
            source,
        };
        let mut header = [0u8; 24];
        std::fs::File::open(&path)
            .and_then(|mut f| f.read_exact(&mut header))
            .map_err(io)?;
        let size = png_size(&header)
            .ok_or_else(|| format_error(format!("{} n'est pas un PNG", path.display())))?;
        Ok(PngData::File {
            path: Arc::from(path.as_path()),
            size,
        })
    }
}

/// Première case et nombre d'images de chaque bande.
type StripIndex = HashMap<String, (u32, u32)>;

/// Où trouver les images d'un comportement.
enum FrameSource {
    /// Dessin vectoriel : pas d'images.
    None,
    /// Planche unique : numéros de cases (dans le premier sens).
    Sheet { count: u32 },
    /// Bandes d'images.
    Strips(StripIndex),
}

/// Analyse un `pet.toml`. `resolve` lit une ressource nommée dans le fichier.
pub fn parse_pet_toml(text: &str, resolve: impl Resources) -> Result<PetDef, LoadError> {
    let file: PetFile = toml::from_str(text)?;
    let sex_ratio = file.pet.sex_ratio()?;
    let age = file.pet.age()?;
    let meta = PetMeta {
        id: file.pet.id.clone(),
        name: file.pet.name.clone(),
        author: file.pet.author.clone(),
        description: file.pet.description.clone(),
        version: file.pet.version.clone(),
        license: file.pet.license.clone(),
        category: file.pet.category.clone(),
        region: file.pet.region.clone(),
    };

    if let Some(desktoppet) = &file.desktoppet {
        let mut def = parse_desktoppet_sidecar(&file, desktoppet, meta, &resolve)?;
        def.sex_ratio = sex_ratio;
        def.age = age;
        return Ok(def);
    }

    let Some(appearance_toml) = &file.appearance else {
        return Err(format_error(
            "il manque la section [appearance] (dessin, planche ou bandes d'images)",
        ));
    };
    // Une bande par comportement : celle qui porte son nom, sauf `strip = …`.
    let mut strip_names: Vec<String> = Vec::new();
    for b in &file.behaviors {
        let name = b.strip.clone().unwrap_or_else(|| b.name.clone());
        if !strip_names.contains(&name) {
            strip_names.push(name);
        }
    }
    let (appearance, frames) = parse_appearance(appearance_toml, &strip_names, &resolve)?;

    let mut physics = Physics::default();
    match &appearance {
        Appearance::Rig { height, .. } | Appearance::Skeleton { height, .. } => {
            physics.height = *height;
            physics.width = *height * 1.2;
        }
        Appearance::Sprites(sheet) => {
            let (w, _) = sheet.tile_size().unwrap_or((0, 0));
            let [_, anchor_y] = sheet.anchor_or_default();
            physics.width = w as f32 * sheet.scale;
            // La boîte de l'animal s'arrête au point d'appui.
            physics.height = anchor_y * sheet.scale;
        }
    }
    file.physics.apply(&mut physics);

    let names: Vec<&str> = file.behaviors.iter().map(|b| b.name.as_str()).collect();
    if let Some(clash) = file.choices.keys().find(|k| names.contains(&k.as_str())) {
        return Err(format_error(format!(
            "« {clash} » désigne à la fois un comportement et une liste de [choices]"
        )));
    }
    let resolve_items =
        |from: &str, items: &[TransitionToml]| -> Result<Vec<Transition>, LoadError> {
            items
                .iter()
                .map(|t| {
                    let to = names.iter().position(|n| *n == t.to).ok_or_else(|| {
                        DefError::UnknownTarget {
                            from: from.to_owned(),
                            to: t.to.clone(),
                        }
                    })?;
                    let when = t
                        .when
                        .iter()
                        .map(|w| parse_condition(w))
                        .collect::<Result<_, _>>()?;
                    Ok(Transition {
                        to,
                        weight: t.weight,
                        when,
                    })
                })
                .collect()
        };
    let resolve_list =
        |from: &str, list: &Option<TransitionsToml>| -> Result<Vec<Transition>, LoadError> {
            match list {
                None => Ok(Vec::new()),
                // Un nom seul : une liste de [choices], ou un comportement.
                Some(TransitionsToml::One(name)) => match file.choices.get(name) {
                    Some(items) => resolve_items(from, items),
                    None => resolve_items(from, &[TransitionToml::to(name)]),
                },
                Some(TransitionsToml::Many(items)) => resolve_items(from, items),
            }
        };

    let scale = match &appearance {
        Appearance::Sprites(sheet) => sheet.scale,
        Appearance::Rig { .. } | Appearance::Skeleton { .. } => 1.0,
    };
    let sounds = load_sounds(&file.sounds, &resolve)?;
    let mut behaviors = Vec::with_capacity(file.behaviors.len());
    for b in &file.behaviors {
        let clip = b.clip(&frames)?;
        let cues = sound_cues(&b.name, b.sound.as_ref(), &sounds, clip.as_ref())?;
        let motion = b.motion(clip.as_ref(), scale)?;
        let scripted = matches!(motion, Motion::Script(_));
        behaviors.push(Behavior {
            name: b.name.clone(),
            duration: match &b.duration {
                Some(d) => d.to_duration()?,
                None if clip.is_some() => Duration::Clip,
                None => Duration::Range(2.0, 4.0),
            },
            motion,
            next: resolve_list(&b.name, &b.next)?,
            on_edge: resolve_list(&b.name, &b.on_edge)?,
            on_wall: resolve_list(&b.name, &b.on_wall)?,
            on_fall: resolve_list(&b.name, &b.on_fall)?,
            on_land: resolve_list(&b.name, &b.on_land)?,
            on_ceiling: Vec::new(),
            on_meet: resolve_list(&b.name, &b.on_meet)?,
            on_window_side: resolve_list(&b.name, &b.on_window_side)?,
            // Une animation image par image vérifie qu'elle a encore un appui.
            gravity: b.gravity.unwrap_or(scripted),
            flip_at_end: b.turns,
            walk_off_edges: b.walk_off_edges,
            clip,
            children: Vec::new(),
            sounds: cues,
            sleeping: b.sleep,
            chew: b.chew,
            tilt: b.tilt.unwrap_or(0.0),
            twist: b.twist,
            spin: b.spin,
            contagious: b.contagious.unwrap_or(0.0).clamp(0.0, 1.0),
        });
    }

    let r = &file.reflexes;
    let reflexes = Reflexes {
        spawn: resolve_list("spawn", &r.spawn)?,
        fall: resolve_list("fall", &r.fall)?,
        land: resolve_list("land", &r.land)?,
        land_hard: resolve_list("land_hard", &r.land_hard)?,
        held: resolve_list("held", &r.held)?,
        thrown: resolve_list("thrown", &r.thrown)?,
        edge: resolve_list("edge", &r.edge)?,
        wall: resolve_list("wall", &r.wall)?,
        poke: resolve_list("poke", &r.poke)?,
        bump: resolve_list("bump", &r.bump)?,
        meet: resolve_list("meet", &r.meet)?,
        window_side: resolve_list("window_side", &r.window_side)?,
        leave: resolve_list("leave", &r.leave)?,
        together: resolve_list("together", &r.together)?,
        startle: resolve_list("startle", &r.startle)?,
        sleepy: resolve_list("sleepy", &r.sleepy)?,
        wake: resolve_list("wake", &r.wake)?,
        wind: resolve_list("wind", &r.wind)?,
    };

    let mut def = PetDef::new(meta, appearance, physics, behaviors, reflexes)?;
    def.sex_ratio = sex_ratio;
    def.age = age;
    def.set_sounds(sounds)?;
    if matches!(age, Age::Lamb { .. }) {
        def.speak_as_lamb();
    }
    // En dernier : chaque dessin par sexe ou par âge copie l'animal complet.
    def.set_variants(parse_variants(appearance_toml, &resolve)?)?;
    Ok(def)
}

/// Sons nommés de la section [sounds] : leurs fichiers sont lus tout de suite.
fn load_sounds(
    table: &BTreeMap<String, SoundToml>,
    resolve: &impl Resources,
) -> Result<Vec<Sound>, LoadError> {
    table
        .iter()
        .map(|(name, sound)| {
            let (files, lamb, volume, mouth) = match sound {
                SoundToml::One(file) => (std::slice::from_ref(file), &[][..], 1.0, false),
                SoundToml::Many {
                    files,
                    lamb,
                    volume,
                    mouth,
                } => (files.as_slice(), lamb.as_slice(), *volume, *mouth),
            };
            if files.is_empty() {
                return Err(format_error(format!("son « {name} » : aucun fichier")));
            }
            if !(0.0..=1.0).contains(&volume) {
                return Err(format_error(format!(
                    "son « {name} » : volume {volume} (attendu entre 0 et 1)"
                )));
            }
            let read = |files: &[String]| {
                files
                    .iter()
                    .map(|file| resolve.read(file).map(Arc::from))
                    .collect::<Result<_, _>>()
            };
            Ok(Sound {
                name: name.clone(),
                clips: read(files)?,
                lamb: read(lamb)?,
                volume,
                mouth,
            })
        })
        .collect()
}

/// Sons d'un comportement (`sound = …`), d'après les noms de la section [sounds].
fn sound_cues(
    behavior: &str,
    cues: Option<&SoundCuesToml>,
    sounds: &[Sound],
    clip: Option<&SpriteClip>,
) -> Result<Vec<SoundCue>, LoadError> {
    let bad = |what: String| format_error(format!("comportement « {behavior} » : {what}"));
    let list: Vec<&SoundCueToml> = match cues {
        None => Vec::new(),
        Some(SoundCuesToml::One(cue)) => vec![cue],
        Some(SoundCuesToml::Many(cues)) => cues.iter().collect(),
    };
    list.into_iter()
        .map(|cue| {
            let (name, chance, frame) = match cue {
                SoundCueToml::Name(name) => (name, 1.0, None),
                SoundCueToml::Full {
                    name,
                    chance,
                    frame,
                } => (name, *chance, *frame),
            };
            let sound = sounds
                .iter()
                .position(|s| s.name == *name)
                .ok_or_else(|| bad(format!("son « {name} » inconnu (voir [sounds])")))?;
            if !(0.0..=1.0).contains(&chance) {
                return Err(bad(format!("chance {chance} (attendu entre 0 et 1)")));
            }
            if let Some(f) = frame {
                let frames = clip.map_or(0, |c| c.frames.len());
                if f >= frames {
                    return Err(bad(format!(
                        "son sur l'image {f}, mais la séquence n'en a que {frames}"
                    )));
                }
            }
            Ok(SoundCue {
                sound,
                chance,
                frame,
            })
        })
        .collect()
}

/// Un dessin vectoriel (`rig`) ou un squelette (`skeleton`), affiché à
/// `height` points de haut ; `None` s'il n'y a ni l'un ni l'autre.
fn parse_drawing(
    rig: Option<&str>,
    skeleton: Option<&str>,
    height: f32,
    resolve: &impl Resources,
) -> Result<Option<Appearance>, LoadError> {
    if let Some(file) = skeleton {
        let text = String::from_utf8(resolve.read(file)?)
            .map_err(|_| format_error(format!("{file} n'est pas en UTF-8")))?;
        // Les images se trouvent à partir du dossier du squelette.
        let base = Path::new(file)
            .parent()
            .map(Path::to_path_buf)
            .unwrap_or_default();
        let skeleton = crate::skeleton::parse_skeleton(&text, |image| {
            let path = base.join(image);
            let name = path.to_str().ok_or("chemin illisible")?;
            resolve.png(name).map_err(|e| e.to_string())
        })
        .map_err(|e| format_error(format!("{file} : {e}")))?;
        return Ok(Some(Appearance::Skeleton {
            skeleton: Arc::new(skeleton),
            height,
        }));
    }
    if let Some(svg) = rig {
        return Ok(Some(Appearance::Rig {
            svg: Arc::from(resolve.read(svg)?),
            height,
        }));
    }
    Ok(None)
}

/// Les dessins par sexe et par âge : `[appearance.male]`,
/// `[appearance.female]`, `[appearance.lamb]` (voir `docs/format.md`).
fn parse_variants(
    a: &AppearanceToml,
    resolve: &impl Resources,
) -> Result<Vec<(Variant, VariantDrawing)>, LoadError> {
    a.variants()
        .map(|(variant, v)| {
            let name = variant.name();
            if usize::from(v.rig.is_some()) + usize::from(v.skeleton.is_some()) != 1 {
                return Err(format_error(format!(
                    "[appearance.{name}] doit contenir un seul de : `rig = \"fichier.svg\"` \
                     ou `skeleton = \"squelette.toml\"`"
                )));
            }
            let height = v.height.ok_or_else(|| {
                format_error(format!(
                    "[appearance.{name}] : il manque `height`, la hauteur affichée du dessin \
                     (en points)"
                ))
            })?;
            let appearance =
                parse_drawing(v.rig.as_deref(), v.skeleton.as_deref(), height, resolve)?
                    .ok_or_else(|| format_error(format!("[appearance.{name}] : aucun dessin")))?;
            Ok((
                variant,
                VariantDrawing {
                    appearance,
                    width: v.width,
                    stride: v.stride,
                    weeks: v.weeks,
                },
            ))
        })
        .collect()
}

fn parse_appearance(
    a: &AppearanceToml,
    strip_names: &[String],
    resolve: &impl Resources,
) -> Result<(Appearance, FrameSource), LoadError> {
    let strips = a.strips_left.is_some() || a.strips_right.is_some();
    let kinds = usize::from(a.rig.is_some())
        + usize::from(a.skeleton.is_some())
        + usize::from(a.sprites.is_some())
        + usize::from(strips);
    if kinds != 1 {
        return Err(format_error(
            "[appearance] doit contenir un seul de : `rig = \"fichier.svg\"`, \
             `skeleton = \"squelette.toml\"`, `sprites = \"planche.png\"` ou \
             `strips_left`/`strips_right` (bandes d'images)",
        ));
    }
    if a.variants().next().is_some() && (a.sprites.is_some() || strips) {
        return Err(format_error(
            "[appearance.male], [appearance.female] et [appearance.lamb] demandent un dessin \
             de base vectoriel (`rig`) ou un squelette (`skeleton`), pas des images",
        ));
    }
    if let Some(appearance) = parse_drawing(
        a.rig.as_deref(),
        a.skeleton.as_deref(),
        a.height.unwrap_or(64.0),
        resolve,
    )? {
        return Ok((appearance, FrameSource::None));
    }

    let (pages, facings, frames) = if let Some(png) = &a.sprites {
        let [tiles_x, tiles_y] = a.tiles.ok_or_else(|| {
            format_error("une planche de sprites doit préciser `tiles = [colonnes, lignes]`")
        })?;
        if tiles_x == 0 || tiles_y == 0 {
            return Err(format_error("`tiles` : il faut au moins une case"));
        }
        let facings = match a.facing.as_deref() {
            None | Some("left") => Facings::Left,
            Some("right") => Facings::Right,
            Some(other) => {
                return Err(format_error(format!(
                    "`facing` : « {other} » inconnu (\"left\" ou \"right\")"
                )));
            }
        };
        let page = SpritePage {
            png: resolve.png(png)?,
            tiles_x,
            tiles_y,
        };
        let count = tiles_x * tiles_y;
        (vec![page], facings, FrameSource::Sheet { count })
    } else {
        if a.facing.is_some() {
            return Err(format_error(
                "`facing` ne s'emploie qu'avec une planche unique : avec des bandes, \
                 le sens vient de `strips_left` et `strips_right`",
            ));
        }
        let (pages, index, facings) = load_strips(a, strip_names, resolve)?;
        (pages, facings, FrameSource::Strips(index))
    };

    let mut drawn = Drawn::default();
    for effect in a.drawn.iter().flatten() {
        match effect.as_str() {
            "shadow" => drawn.shadow = true,
            "dust" => drawn.dust = true,
            "squash" => drawn.squash = true,
            "roll" => drawn.roll = true,
            other => {
                return Err(format_error(format!(
                    "`drawn` : effet « {other} » inconnu (shadow, dust, squash, roll)"
                )));
            }
        }
    }
    let scale = a.scale.unwrap_or(1.0);
    if !(scale > 0.0 && scale.is_finite()) {
        return Err(format_error("`scale` doit être positif"));
    }
    let mut sheet = SpriteSheet {
        pages,
        key_color: a.key_color.as_deref().map(parse_hex_color).transpose()?,
        scale,
        skins: 1,
        facings,
        anchor: a.anchor,
        drawn,
        inbetweens: None,
    };
    let Some((tile_w, tile_h)) = sheet.tile_size().filter(|(w, h)| *w > 0 && *h > 0) else {
        return Err(format_error(
            "la planche n'est pas un PNG lisible, ou elle est trop petite pour ses cases",
        ));
    };
    if let Some(path) = &a.inbetweens {
        let strips = match &frames {
            FrameSource::Strips(index) => Some((index, strip_names)),
            _ => None,
        };
        sheet.inbetweens = Some(Arc::new(load_inbetweens(path, &sheet, strips, resolve)?));
    }
    let frames = match frames {
        // Planche unique : les comportements désignent les cases du premier sens.
        FrameSource::Sheet { .. } => FrameSource::Sheet {
            count: sheet.frames_per_facing(),
        },
        other => other,
    };
    if let Some([x, y]) = a.anchor
        && !(x.is_finite() && y.is_finite())
    {
        return Err(format_error(format!(
            "`anchor` : coordonnées invalides (case de {tile_w} × {tile_h} pixels)"
        )));
    }
    Ok((Appearance::Sprites(sheet), frames))
}

/// Bandes d'images : une par nom, dans chaque sens donné. Les bandes tournées
/// vers la droite suivent celles tournées vers la gauche, dans le même ordre.
fn load_strips(
    a: &AppearanceToml,
    names: &[String],
    resolve: &impl Resources,
) -> Result<(Vec<SpritePage>, StripIndex, Facings), LoadError> {
    let [frame_w, frame_h] = a.frame.ok_or_else(|| {
        format_error("avec des bandes d'images, précise leur taille : `frame = [largeur, hauteur]`")
    })?;
    if frame_w == 0 || frame_h == 0 {
        return Err(format_error("`frame` : taille d'image nulle"));
    }
    let facings = match (&a.strips_left, &a.strips_right) {
        (Some(_), Some(_)) => Facings::Both,
        (None, Some(_)) => Facings::Right,
        _ => Facings::Left,
    };
    let patterns: Vec<&String> = [&a.strips_left, &a.strips_right]
        .into_iter()
        .flatten()
        .collect();
    let mut pages = Vec::new();
    let mut index = StripIndex::new();
    for (side, pattern) in patterns.into_iter().enumerate() {
        if !pattern.contains("{name}") {
            return Err(format_error(format!(
                "« {pattern} » : le chemin des bandes doit contenir {{name}}, remplacé par le nom de l'animation"
            )));
        }
        let mut offset = 0;
        for name in names {
            let path = pattern.replace("{name}", name);
            let png = resolve.png(&path)?;
            let (w, h) = png
                .size()
                .ok_or_else(|| format_error(format!("{path} n'est pas un PNG")))?;
            if w % frame_w != 0 || h % frame_h != 0 || w == 0 || h == 0 {
                return Err(format_error(format!(
                    "{path} : {w} × {h} pixels, ce n'est pas un nombre entier d'images de {frame_w} × {frame_h}"
                )));
            }
            let (tiles_x, tiles_y) = (w / frame_w, h / frame_h);
            let count = tiles_x * tiles_y;
            if side == 0 {
                index.insert(name.clone(), (offset, count));
            } else if index.get(name).map(|(_, n)| *n) != Some(count) {
                return Err(format_error(format!(
                    "{path} : {count} images, pas autant que dans l'autre sens"
                )));
            }
            pages.push(SpritePage {
                png,
                tiles_x,
                tiles_y,
            });
            offset += count;
        }
    }
    Ok((pages, index, facings))
}

/// Images intermédiaires décrites par le fichier `path` (écrit par
/// `tools/inbetween`). Avec des bandes, chaque bande a la sienne, qui donne
/// `count` images après chacune de ses images (la dernière va vers la
/// première) ; une bande sans la sienne n'en a pas. Avec une planche
/// unique, une planche à part et la liste des paires d'images.
fn load_inbetweens(
    path: &str,
    sheet: &SpriteSheet,
    strips: Option<(&StripIndex, &[String])>,
    resolve: &impl Resources,
) -> Result<Inbetweens, LoadError> {
    let bad = |what: String| format_error(format!("{path} : {what}"));
    let text = String::from_utf8(resolve.read(path)?).map_err(|_| bad("pas en UTF-8".into()))?;
    let file: InbetweensToml = toml::from_str(&text)?;
    let (tile_w, tile_h) = sheet.tile_size().unwrap_or((0, 0));
    // Les images intermédiaires sont numérotées après toutes les cases.
    let base = sheet.tile_count();
    let mut out = Inbetweens::default();
    let page = |png: PngData, what: &str| -> Result<SpritePage, LoadError> {
        let (w, h) = png
            .size()
            .ok_or_else(|| bad(format!("{what} n'est pas un PNG")))?;
        if w % tile_w.max(1) != 0 || h % tile_h.max(1) != 0 || w == 0 || h == 0 {
            return Err(bad(format!(
                "{what} : {w} × {h} pixels, ce n'est pas un nombre entier d'images de {tile_w} × {tile_h}"
            )));
        }
        Ok(SpritePage {
            png,
            tiles_x: w / tile_w,
            tiles_y: h / tile_h,
        })
    };

    let Some((index, names)) = strips else {
        // Planche unique : une planche à part et la liste des paires.
        let sprites = file
            .sprites
            .as_deref()
            .ok_or_else(|| bad("il manque `sprites = \"planche.png\"`".into()))?;
        let [tiles_x, tiles_y] = file
            .tiles
            .ok_or_else(|| bad("il manque `tiles = [colonnes, lignes]`".into()))?;
        let page = page(resolve.png(sprites)?, sprites)?;
        if (page.tiles_x, page.tiles_y) != (tiles_x, tiles_y) {
            return Err(bad(format!(
                "{sprites} : {} × {} cases, pas {tiles_x} × {tiles_y}",
                page.tiles_x, page.tiles_y
            )));
        }
        let tiles = page.tiles_x * page.tiles_y;
        let frames = sheet.frames_per_facing();
        for [from, to, first, count] in file.pairs {
            if from >= frames || to >= frames {
                return Err(bad(format!(
                    "paire {from} → {to} : la planche n'a que {frames} images"
                )));
            }
            if first + count > tiles {
                return Err(bad(format!(
                    "paire {from} → {to} : cases {first} à {} pour {tiles} cases",
                    first + count
                )));
            }
            out.pairs.insert((from, to), (base + first, count));
        }
        out.pages.push(page);
        return Ok(out);
    };

    // Bandes : une par animation et par sens, `count` images après chaque
    // image (ou le nombre propre à l'animation, dans `[counts]`).
    let default_count = file.count.unwrap_or(1);
    if default_count == 0 || file.counts.values().any(|c| *c == 0) {
        return Err(bad("`count` doit valoir au moins 1".into()));
    }
    let patterns: Vec<&String> = [&file.strips_left, &file.strips_right]
        .into_iter()
        .flatten()
        .collect();
    let sides = if sheet.facings == Facings::Both { 2 } else { 1 };
    if patterns.len() != sides || patterns.iter().any(|p| !p.contains("{name}")) {
        return Err(bad(format!(
            "il faut {sides} chemin(s) de bandes (`strips_left`, `strips_right`) contenant {{name}}"
        )));
    }
    let mut pages: Vec<Vec<SpritePage>> = vec![Vec::new(); sides];
    let mut offset = 0;
    for name in names {
        let Some(&(first, frames)) = index.get(name) else {
            continue;
        };
        if frames < 2 {
            continue;
        }
        let count = file
            .counts
            .get(name.as_str())
            .copied()
            .unwrap_or(default_count);
        // Une bande sans ses images intermédiaires (dans un sens ou l'autre)
        // n'en a pas : elle reste telle qu'elle est dessinée.
        let Ok(found) = patterns
            .iter()
            .map(|pattern| {
                let path = pattern.replace("{name}", name);
                resolve.png(&path).map(|png| (path, png))
            })
            .collect::<Result<Vec<_>, _>>()
        else {
            continue;
        };
        for (side, (path, png)) in found.into_iter().enumerate() {
            let page = page(png, &path)?;
            if page.tiles_x * page.tiles_y != frames * count {
                return Err(bad(format!(
                    "{path} : {} images, il en faut {count} par image de la bande ({frames} images)",
                    page.tiles_x * page.tiles_y
                )));
            }
            pages[side].push(page);
        }
        let skip = file.skip.get(name.as_str()).map_or(&[][..], Vec::as_slice);
        for i in 0..frames {
            if !skip.contains(&i) {
                let pair = (first + i, first + (i + 1) % frames);
                out.pairs.insert(pair, (base + offset + i * count, count));
            }
        }
        offset += frames * count;
    }
    out.pages = pages.into_iter().flatten().collect();
    Ok(out)
}

/// `pet.toml` qui reprend un fichier desktopPet : nom et planche viennent du
/// TOML, les comportements du XML.
fn parse_desktoppet_sidecar(
    file: &PetFile,
    desktoppet: &DesktopPetToml,
    meta: PetMeta,
    resolve: &impl Resources,
) -> Result<PetDef, LoadError> {
    if !file.behaviors.is_empty() {
        return Err(format_error(
            "avec [desktoppet], les comportements viennent du fichier XML : retirez les [[behavior]]",
        ));
    }
    let sprites = match &file.appearance {
        None => None,
        Some(a) => match parse_appearance(a, &[], resolve)? {
            (Appearance::Sprites(sheet), FrameSource::Sheet { .. }) => Some(sheet),
            _ => {
                return Err(format_error(
                    "un animal desktopPet s'affiche avec une planche de sprites (`sprites = …`)",
                ));
            }
        },
    };
    let xml = String::from_utf8(resolve.read(&desktoppet.animations)?)
        .map_err(|_| format_error(format!("{} n'est pas en UTF-8", desktoppet.animations)))?;
    let mut def = crate::desktoppet::import_with(
        xml.trim_start_matches('\u{feff}'),
        crate::desktoppet::Overrides {
            meta: Some(meta),
            sprites,
            original_tile: desktoppet.tile.map(|[w, h]| (w, h)),
        },
    )?;
    // Sons du pet.toml, après ceux du fichier XML, joués avec les animations
    // nommées dans [desktoppet.sounds].
    let mut sounds = std::mem::take(&mut def.sounds);
    let first = sounds.len();
    sounds.extend(load_sounds(&file.sounds, resolve)?);
    for (animation, cues) in &desktoppet.sounds {
        let index = def.behavior_index(animation).ok_or_else(|| {
            format_error(format!(
                "[desktoppet.sounds] : l'animation « {animation} » n'existe pas"
            ))
        })?;
        let behavior = &mut def.behaviors[index];
        let cues = sound_cues(
            animation,
            Some(cues),
            &sounds[first..],
            behavior.clip.as_ref(),
        )?;
        behavior.sounds.extend(cues.into_iter().map(|cue| SoundCue {
            sound: cue.sound + first,
            ..cue
        }));
    }
    def.set_sounds(sounds)?;
    for (reflex, reaction) in &desktoppet.reactions {
        let items = match reaction {
            TransitionsToml::One(name) => vec![TransitionToml::to(name)],
            TransitionsToml::Many(items) => items.clone(),
        };
        let mut list = Vec::with_capacity(items.len());
        for item in &items {
            let animation = &item.to;
            let to = def.behavior_index(animation).ok_or_else(|| {
                format_error(format!(
                    "[desktoppet.reactions] : l'animation « {animation} » n'existe pas"
                ))
            })?;
            let when = item
                .when
                .iter()
                .map(|c| parse_condition(c))
                .collect::<Result<Vec<_>, _>>()?;
            list.push(Transition {
                to,
                weight: item.weight,
                when,
            });
        }
        match reflex.as_str() {
            "startle" => def.reflexes.startle = list,
            "sleepy" => def.reflexes.sleepy = list,
            "wake" => def.reflexes.wake = list,
            "wind" => def.reflexes.wind = list,
            "poke" => def.reflexes.poke = list,
            other => {
                return Err(format_error(format!(
                    "[desktoppet.reactions] : réflexe inconnu « {other} » (startle, sleepy, wake, wind, poke)"
                )));
            }
        }
    }
    Ok(def)
}

fn parse_hex_color(s: &str) -> Result<[u8; 3], LoadError> {
    let hex = s.trim().trim_start_matches('#');
    let bad = || format_error(format!("couleur invalide « {s} » (attendu #RRGGBB)"));
    if hex.len() != 6 {
        return Err(bad());
    }
    let byte = |i: usize| u8::from_str_radix(&hex[i..i + 2], 16).map_err(|_| bad());
    Ok([byte(0)?, byte(2)?, byte(4)?])
}

fn parse_condition(s: &str) -> Result<Condition, LoadError> {
    let (key, arg) = s.split_once(':').map_or((s, None), |(k, a)| (k, Some(a)));
    let radius = || -> Result<f32, LoadError> {
        arg.map_or(Ok(220.0), |a| {
            a.trim()
                .parse::<f32>()
                .map_err(|_| format_error(format!("rayon invalide dans « {s} »")))
        })
    };
    Ok(match key.trim() {
        "on_window" => Condition::OnWindow,
        "on_floor" => Condition::OnFloor,
        "night" => Condition::Night,
        "day" => Condition::Day,
        "cursor_near" => Condition::CursorNear(radius()?),
        "cursor_far" => Condition::CursorFar(radius()?),
        "typing" => Condition::Typing,
        "away" => Condition::Away,
        "present" => Condition::Present,
        "morning" => Condition::Morning,
        "evening" => Condition::Evening,
        "calm" => Condition::Temper(Temper::Calm),
        "lively" => Condition::Temper(Temper::Lively),
        "rowdy" => Condition::Temper(Temper::Rowdy),
        "even" => Condition::Temper(Temper::Even),
        "into_wind" => Condition::IntoWind,
        "male" => Condition::Sex(Sex::Male),
        "female" => Condition::Sex(Sex::Female),
        "adult" => Condition::Adult,
        "lamb" => Condition::Lamb,
        other => return Err(format_error(format!("condition inconnue « {other} »"))),
    })
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct PetFile {
    pet: MetaToml,
    #[serde(default)]
    appearance: Option<AppearanceToml>,
    #[serde(default)]
    desktoppet: Option<DesktopPetToml>,
    #[serde(default)]
    physics: PhysicsToml,
    #[serde(default)]
    reflexes: ReflexesToml,
    /// Listes de suites nommées, réutilisables par `next = "nom"`.
    #[serde(default)]
    choices: BTreeMap<String, Vec<TransitionToml>>,
    /// Sons nommés : un fichier, ou `{ files = [...], volume = … }`.
    #[serde(default)]
    sounds: BTreeMap<String, SoundToml>,
    #[serde(rename = "behavior", default)]
    behaviors: Vec<BehaviorToml>,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct MetaToml {
    id: String,
    name: String,
    author: Option<String>,
    description: Option<String>,
    version: Option<String>,
    license: Option<String>,
    /// Sa famille (`sheep`, `cats`… : voir [`crate::def::CATEGORIES`]).
    category: Option<String>,
    /// La région d'origine de sa race (`france`… : voir [`crate::def::REGIONS`]).
    region: Option<String>,
    sex: Option<SexToml>,
    age: Option<String>,
    age_weeks: Option<f32>,
}

#[derive(Deserialize)]
#[serde(untagged)]
enum SexToml {
    Fixed(String),
    Ratio(SexRatioToml),
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct SexRatioToml {
    male: f32,
}

impl MetaToml {
    fn sex_ratio(&self) -> Result<SexRatio, LoadError> {
        match &self.sex {
            None => Ok(SexRatio::default()),
            Some(SexToml::Fixed(sex)) => match sex.as_str() {
                "male" => Ok(SexRatio::MALE),
                "female" => Ok(SexRatio::FEMALE),
                other => Err(format_error(format!(
                    "sexe inconnu « {other} » (male, female ou {{ male = proportion }})"
                ))),
            },
            Some(SexToml::Ratio(ratio)) => Ok(SexRatio::new(ratio.male)?),
        }
    }

    fn age(&self) -> Result<Age, LoadError> {
        match self.age.as_deref().unwrap_or("adult") {
            "adult" if self.age_weeks.is_none() => Ok(Age::Adult),
            "adult" => Err(format_error("age_weeks demande age = \"lamb\"")),
            "lamb" => {
                let weeks = self.age_weeks.unwrap_or(2.0);
                if !(weeks > 0.0 && weeks <= 52.0) {
                    return Err(format_error(
                        "l'âge de l'agneau doit être fini, strictement positif et au plus 52 semaines",
                    ));
                }
                Ok(Age::Lamb { weeks })
            }
            other => Err(format_error(format!(
                "âge inconnu « {other} » (adult ou lamb)"
            ))),
        }
    }
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct DesktopPetToml {
    /// Fichier `animations.xml` de desktopPet.
    animations: String,
    /// Taille des cases d'origine, si l'image a été retirée du fichier XML
    /// (les déplacements y sont exprimés à cette taille).
    tile: Option<[u32; 2]>,
    /// Sons à jouer avec les animations du fichier : nom de l'animation →
    /// son(s) de la section [sounds].
    #[serde(default)]
    sounds: BTreeMap<String, SoundCuesToml>,
    /// Réactions de Mouflon que le fichier ne prévoit pas : réflexe
    /// (`startle`, `sleepy`, `wake`, `wind`, `poke`) → nom de l'animation
    /// jouée, ou une liste `{ to, weight, when }` comme ailleurs.
    #[serde(default)]
    reactions: BTreeMap<String, TransitionsToml>,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct AppearanceToml {
    rig: Option<String>,
    /// Squelette à morceaux d'images (voir `docs/squelette.md`).
    skeleton: Option<String>,
    height: Option<f32>,
    sprites: Option<String>,
    tiles: Option<[u32; 2]>,
    key_color: Option<String>,
    scale: Option<f32>,
    /// Sens des images d'une planche unique : "left" (défaut) ou "right".
    facing: Option<String>,
    /// Bandes d'images tournées vers la gauche, `{name}` remplacé par le nom de l'animation.
    strips_left: Option<String>,
    /// Bandes tournées vers la droite (sans elles, les images sont retournées).
    strips_right: Option<String>,
    /// Taille d'une image dans les bandes, en pixels.
    frame: Option<[u32; 2]>,
    /// Point d'appui (milieu des pattes) dans une image, en pixels.
    anchor: Option<[f32; 2]>,
    /// Ancienne clé, acceptée et ignorée : l'animal reste pris là où on
    /// l'attrape, comme une fenêtre qu'on déplace.
    #[allow(dead_code)]
    hold: Option<toml::Value>,
    /// Effets déjà dessinés dans les images.
    drawn: Option<Vec<String>>,
    /// Fichier des images intermédiaires (écrit par `tools/inbetween`).
    inbetweens: Option<String>,
    /// Les dessins du mâle adulte, de la femelle adulte et du jeune
    /// (`[appearance.male]`…), si ce ne sont pas celui de base.
    male: Option<VariantToml>,
    female: Option<VariantToml>,
    lamb: Option<VariantToml>,
}

impl AppearanceToml {
    /// Les dessins par sexe et par âge qu'il donne.
    fn variants(&self) -> impl Iterator<Item = (Variant, &VariantToml)> {
        [
            (Variant::Male, &self.male),
            (Variant::Female, &self.female),
            (Variant::Lamb, &self.lamb),
        ]
        .into_iter()
        .filter_map(|(variant, v)| v.as_ref().map(|v| (variant, v)))
    }
}

/// Un dessin par sexe ou par âge : `[appearance.female]`…
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct VariantToml {
    rig: Option<String>,
    skeleton: Option<String>,
    /// Sa hauteur affichée, en points (0,5 pt par centimètre de l'animal).
    height: Option<f32>,
    /// Sa largeur et sa foulée (comme dans `[physics]`) ; sans elles, celles
    /// du dessin de base, au rapport des tailles.
    width: Option<f32>,
    stride: Option<f32>,
    /// L'âge de l'agneau dessiné, en semaines (`[appearance.lamb]` seulement).
    weeks: Option<f32>,
}

/// Fichier des images intermédiaires.
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct InbetweensToml {
    /// Bandes : nombre d'images intermédiaires après chaque image…
    count: Option<u32>,
    /// … sauf pour ces animations, qui en ont un nombre à elles (une chute
    /// rapide en veut davantage).
    #[serde(default)]
    counts: BTreeMap<String, u32>,
    /// Bandes d'images intermédiaires, `{name}` remplacé par le nom de l'animation.
    strips_left: Option<String>,
    strips_right: Option<String>,
    /// Bandes : images (numéros dans la bande) qui n'en ont pas après elles,
    /// trop différentes de la suivante.
    #[serde(default)]
    skip: BTreeMap<String, Vec<u32>>,
    /// Planche unique : la planche des images intermédiaires…
    sprites: Option<String>,
    tiles: Option<[u32; 2]>,
    /// … et ses paires : `[image de départ, image d'arrivée, première case, nombre]`.
    #[serde(default)]
    pairs: Vec<[u32; 4]>,
}

#[derive(Deserialize, Default)]
#[serde(deny_unknown_fields)]
struct PhysicsToml {
    gravity: Option<f32>,
    terminal_velocity: Option<f32>,
    air_drag: Option<f32>,
    walk_accel: Option<f32>,
    ground_friction: Option<f32>,
    bounce: Option<f32>,
    bounce_min_speed: Option<f32>,
    hard_landing_speed: Option<f32>,
    max_throw_speed: Option<f32>,
    width: Option<f32>,
    height: Option<f32>,
    stride: Option<f32>,
    /// La longueur des mèches qui se balancent, en centimètres.
    fleece: Option<f32>,
}

impl PhysicsToml {
    fn apply(&self, p: &mut Physics) {
        let set = |dst: &mut f32, src: Option<f32>| {
            if let Some(v) = src {
                *dst = v;
            }
        };
        set(&mut p.gravity, self.gravity);
        set(&mut p.terminal_velocity, self.terminal_velocity);
        set(&mut p.air_drag, self.air_drag);
        set(&mut p.walk_accel, self.walk_accel);
        set(&mut p.ground_friction, self.ground_friction);
        set(&mut p.bounce, self.bounce);
        set(&mut p.bounce_min_speed, self.bounce_min_speed);
        set(&mut p.hard_landing_speed, self.hard_landing_speed);
        set(&mut p.max_throw_speed, self.max_throw_speed);
        set(&mut p.width, self.width);
        set(&mut p.height, self.height);
        set(&mut p.stride, self.stride);
        set(&mut p.fleece, self.fleece);
    }
}

#[derive(Deserialize, Default)]
#[serde(deny_unknown_fields)]
struct ReflexesToml {
    spawn: Option<TransitionsToml>,
    fall: Option<TransitionsToml>,
    land: Option<TransitionsToml>,
    land_hard: Option<TransitionsToml>,
    held: Option<TransitionsToml>,
    thrown: Option<TransitionsToml>,
    edge: Option<TransitionsToml>,
    wall: Option<TransitionsToml>,
    poke: Option<TransitionsToml>,
    bump: Option<TransitionsToml>,
    meet: Option<TransitionsToml>,
    window_side: Option<TransitionsToml>,
    leave: Option<TransitionsToml>,
    together: Option<TransitionsToml>,
    startle: Option<TransitionsToml>,
    sleepy: Option<TransitionsToml>,
    wake: Option<TransitionsToml>,
    wind: Option<TransitionsToml>,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct BehaviorToml {
    name: String,
    /// Facultatif pour un animal en images : il avance alors image par image.
    motion: Option<String>,
    speed: Option<f32>,
    /// Saut : `[vitesse vers l'avant, vitesse vers le haut]`.
    jump: Option<[f32; 2]>,
    /// Saut sur un rebord (`motion = "leap"`) : le plus haut rebord qu'il
    /// atteint, en longueurs de patte.
    reach: Option<f32>,
    duration: Option<DurationToml>,
    next: Option<TransitionsToml>,
    on_edge: Option<TransitionsToml>,
    on_wall: Option<TransitionsToml>,
    on_fall: Option<TransitionsToml>,
    on_land: Option<TransitionsToml>,
    on_meet: Option<TransitionsToml>,
    on_window_side: Option<TransitionsToml>,
    /// Sans transition de bord : continuer tout droit et tomber du rebord.
    #[serde(default)]
    walk_off_edges: bool,
    /// Il dort (ou s'endort) : l'utilisateur qui revient le réveille.
    #[serde(default)]
    sleep: bool,
    /// Il mâche pendant ce comportement (rumine, couché).
    #[serde(default)]
    chew: bool,
    /// Penché en avant (radians) pendant ce comportement.
    tilt: Option<f32>,
    /// Chance qu'un voisin de la même famille l'imite (0–1).
    contagious: Option<f32>,
    /// Saut tordu (le corps se tord, les pattes ruent).
    #[serde(default)]
    twist: bool,
    /// Il se retourne en l'air.
    #[serde(default)]
    spin: bool,
    /// Bande jouée (par défaut, celle qui porte le nom du comportement).
    strip: Option<String>,
    /// Images jouées : numéros dans la bande (ou dans la planche unique).
    frames: Option<Vec<u32>>,
    /// Images par seconde : un nombre, ou `[début, fin]` pour un rythme qui
    /// change le long de la séquence (un choc vif, puis un retour plus lent).
    fps: Option<FpsToml>,
    /// Nombre de passages : un nombre, ou `[min, max]` tiré au hasard.
    loops: Option<LoopsToml>,
    /// Les passages suivants reprennent à cette image (numéro dans `frames`).
    loop_from: Option<usize>,
    /// Déplacement à chaque image, en pixels de la bande (x vers l'avant,
    /// y vers le bas) : `[x, y]`, ou une paire par image.
    #[serde(rename = "move")]
    moves: Option<MoveToml>,
    /// Les images montrent un demi-tour : à la fin, l'animal regarde de l'autre côté.
    #[serde(default)]
    turns: bool,
    /// Vérifier à chaque pas qu'il a encore un appui (par défaut pour les
    /// animations image par image).
    gravity: Option<bool>,
    /// Sons : un nom de la section [sounds] (joué au début), ou
    /// `{ name = …, chance = 0.3, frame = 2 }`, ou une liste.
    sound: Option<SoundCuesToml>,
}

impl BehaviorToml {
    fn sprite_keys(&self) -> bool {
        self.strip.is_some()
            || self.frames.is_some()
            || self.fps.is_some()
            || self.loops.is_some()
            || self.loop_from.is_some()
            || self.moves.is_some()
            || self.turns
    }

    /// Séquence d'images du comportement.
    fn clip(&self, source: &FrameSource) -> Result<Option<SpriteClip>, LoadError> {
        let bad = |what: String| format_error(format!("comportement « {} » : {what}", self.name));
        let frames: Vec<u32> = match source {
            FrameSource::None => {
                if self.sprite_keys() {
                    return Err(bad(
                        "images, bande ou déplacement image par image demandés pour un dessin vectoriel".into(),
                    ));
                }
                return Ok(None);
            }
            FrameSource::Sheet { count } => {
                let frames = self.frames.clone().ok_or_else(|| {
                    bad("précise ses images : `frames = [numéros des cases]`".into())
                })?;
                if let Some(f) = frames.iter().find(|f| **f >= *count) {
                    return Err(bad(format!(
                        "la case {f} n'existe pas (la planche en a {count} par sens)"
                    )));
                }
                frames
            }
            FrameSource::Strips(index) => {
                let strip = self.strip.as_deref().unwrap_or(&self.name);
                let &(offset, count) = index
                    .get(strip)
                    .ok_or_else(|| bad(format!("bande « {strip} » introuvable")))?;
                match &self.frames {
                    None => (offset..offset + count).collect(),
                    Some(list) => {
                        if let Some(f) = list.iter().find(|f| **f >= count) {
                            return Err(bad(format!(
                                "l'image {f} n'existe pas (la bande « {strip} » en a {count})"
                            )));
                        }
                        list.iter().map(|f| offset + f).collect()
                    }
                }
            }
        };
        if frames.is_empty() {
            return Err(bad("aucune image".into()));
        }
        let (fps_start, fps_end) = match self.fps {
            None => (10.0, 10.0),
            Some(FpsToml::Fixed(fps)) => (fps, fps),
            Some(FpsToml::Ramp([start, end])) => (start, end),
        };
        if ![fps_start, fps_end]
            .iter()
            .all(|fps| *fps > 0.0 && fps.is_finite())
        {
            return Err(bad("`fps` doit être positif".into()));
        }
        let (repeat, repeat_expr) = match self.loops {
            None => (0, None),
            Some(LoopsToml::Fixed(n)) => (n.max(1) - 1, None),
            Some(LoopsToml::Range([a, b])) => {
                let (lo, hi) = (a.min(b).max(1), a.max(b).max(1));
                if lo == hi {
                    (lo - 1, None)
                } else {
                    // `random` vaut de 0 à 99 : répétitions de lo − 1 à hi − 1.
                    let source = format!("{} + random * {} / 100", lo - 1, hi - lo + 1);
                    let expr = Expr::parse(&source).map_err(|e| bad(e.to_string()))?;
                    (lo - 1, Some(expr))
                }
            }
        };
        let repeat_from = self.loop_from.unwrap_or(0);
        if repeat_from >= frames.len() {
            return Err(bad(format!(
                "`loop_from = {repeat_from}` : la séquence n'a que {} images",
                frames.len()
            )));
        }
        Ok(Some(SpriteClip {
            frames,
            interval_start: 1.0 / fps_start,
            interval_end: 1.0 / fps_end,
            repeat,
            repeat_expr,
            // Les passages suivants rejouent les images à partir de `loop_from`.
            dynamic_repeat: true,
            repeat_from,
            offset_y: 0.0,
            offset_y_end: 0.0,
            opacity_start: 1.0,
            opacity_end: 1.0,
        }))
    }

    fn motion(&self, clip: Option<&SpriteClip>, scale: f32) -> Result<Motion, LoadError> {
        let Some(motion) = self.motion.as_deref() else {
            let Some(clip) = clip else {
                return Err(format_error(format!(
                    "comportement « {} » : il manque `motion` (stand, walk, run, turn, sit, \
                     sleep, look, jump, fall, land, held)",
                    self.name
                )));
            };
            // Animal en images : il avance du déplacement prévu à chaque image.
            let script = match &self.moves {
                None => Script::constant(0.0, 0.0, scale),
                Some(MoveToml::One([x, y])) => Script::constant(*x, *y, scale),
                Some(MoveToml::PerFrame(list)) => {
                    if list.len() != clip.frames.len() {
                        return Err(format_error(format!(
                            "comportement « {} » : `move` donne {} déplacements pour {} images",
                            self.name,
                            list.len(),
                            clip.frames.len()
                        )));
                    }
                    Script {
                        steps: Some(list.iter().map(|[x, y]| Vec2::new(*x, *y)).collect()),
                        ..Script::constant(0.0, 0.0, scale)
                    }
                }
            };
            return Ok(Motion::Script(Arc::new(script)));
        };
        if self.moves.is_some() || self.turns {
            return Err(format_error(format!(
                "comportement « {} » : `move` et `turns` vont avec une animation image par \
                 image, sans `motion`",
                self.name
            )));
        }
        let need_speed = |default: f32| self.speed.unwrap_or(default);
        Ok(match motion {
            "stand" => Motion::Stand,
            "walk" => Motion::Walk {
                speed: need_speed(46.0),
            },
            "run" => Motion::Run {
                speed: need_speed(150.0),
            },
            "turn" => Motion::Turn,
            "sit" => Motion::Sit,
            "sleep" => Motion::Sleep,
            "look" => Motion::Look,
            "graze" => Motion::Graze,
            "jump" => {
                let [vx, vy] = self.jump.unwrap_or([120.0, 520.0]);
                Motion::Jump { vx, vy }
            }
            "pronk" => {
                let [vx, vy] = self.jump.unwrap_or([25.0, 420.0]);
                Motion::Pronk { vx, vy }
            }
            "charge" => Motion::Charge {
                speed: need_speed(175.0),
            },
            "back" => Motion::Back {
                speed: need_speed(28.0),
            },
            "leap" => {
                let reach = self.reach.unwrap_or(LEAP_REACH);
                if !(reach.is_finite() && reach > 0.0 && reach <= 12.0) {
                    return Err(format_error(format!(
                        "comportement « {} » : `reach` doit être entre 0 et 12 longueurs de \
                         patte ({reach})",
                        self.name
                    )));
                }
                Motion::Leap { reach }
            }
            "fall" => Motion::Fall,
            "land" => Motion::Land,
            "held" => Motion::Held,
            other => match Gesture::from_name(other) {
                Some(gesture) => Motion::Gesture(gesture),
                None => {
                    let known: Vec<&str> = MOTIONS
                        .into_iter()
                        .chain(Gesture::ALL.map(Gesture::name))
                        .collect();
                    return Err(format_error(format!(
                        "comportement « {} » : mouvement inconnu « {other} » ({})",
                        self.name,
                        known.join(", ")
                    )));
                }
            },
        })
    }
}

/// Les mouvements du moteur, par leur nom dans un `pet.toml` ; les gestes
/// s'y ajoutent (voir [`Gesture::name`]).
const MOTIONS: [&str; 16] = [
    "stand", "walk", "run", "charge", "back", "turn", "sit", "sleep", "look", "graze", "jump",
    "pronk", "leap", "fall", "land", "held",
];

/// Sans `reach`, un saut sur un rebord monte jusqu'à deux longueurs de
/// patte : à peu près la hauteur au garrot d'un mouton (voir
/// `docs/animation-animale.md`, « Sauter sur un rebord plus haut »).
const LEAP_REACH: f32 = 2.0;

#[derive(Deserialize, Clone, Copy)]
#[serde(untagged)]
enum FpsToml {
    Fixed(f32),
    Ramp([f32; 2]),
}

#[derive(Deserialize, Clone, Copy)]
#[serde(untagged)]
enum LoopsToml {
    Fixed(u32),
    Range([u32; 2]),
}

#[derive(Deserialize)]
#[serde(untagged)]
enum SoundToml {
    One(String),
    Many {
        files: Vec<String>,
        /// Les prises de l'agneau, s'il a sa voix à lui.
        #[serde(default)]
        lamb: Vec<String>,
        #[serde(default = "one")]
        volume: f32,
        /// Sa voix : il ouvre la bouche en le faisant entendre.
        #[serde(default)]
        mouth: bool,
    },
}

#[derive(Deserialize)]
#[serde(untagged)]
enum SoundCuesToml {
    One(SoundCueToml),
    Many(Vec<SoundCueToml>),
}

#[derive(Deserialize)]
#[serde(untagged)]
enum SoundCueToml {
    Name(String),
    Full {
        name: String,
        /// Chance de l'entendre à chaque occasion (de 0 à 1).
        #[serde(default = "one")]
        chance: f32,
        /// Image de la séquence (rang dans ses images) qui le déclenche, à
        /// chaque passage ; sans valeur, au début du comportement.
        frame: Option<usize>,
    },
}

#[derive(Deserialize)]
#[serde(untagged)]
enum MoveToml {
    One([f32; 2]),
    PerFrame(Vec<[f32; 2]>),
}

#[derive(Deserialize)]
#[serde(untagged)]
enum DurationToml {
    Fixed(f32),
    Range([f32; 2]),
    Keyword(String),
}

impl DurationToml {
    fn to_duration(&self) -> Result<Duration, LoadError> {
        match self {
            Self::Fixed(v) => Ok(Duration::Range(*v, *v)),
            Self::Range([a, b]) => Ok(Duration::Range(a.min(*b), a.max(*b))),
            Self::Keyword(k) if k == "forever" => Ok(Duration::Forever),
            Self::Keyword(k) => Err(format_error(format!(
                "durée inconnue « {k} » (nombre, [min, max] ou \"forever\")"
            ))),
        }
    }
}

#[derive(Deserialize)]
#[serde(untagged)]
enum TransitionsToml {
    One(String),
    Many(Vec<TransitionToml>),
}

#[derive(Deserialize, Clone)]
#[serde(deny_unknown_fields)]
struct TransitionToml {
    to: String,
    #[serde(default = "one")]
    weight: f32,
    #[serde(default)]
    when: OneOrMany,
}

impl TransitionToml {
    fn to(name: &str) -> Self {
        Self {
            to: name.to_owned(),
            weight: 1.0,
            when: OneOrMany::default(),
        }
    }
}

fn one() -> f32 {
    1.0
}

#[derive(Deserialize, Clone, Default)]
#[serde(untagged)]
enum OneOrMany {
    #[default]
    None,
    One(String),
    Many(Vec<String>),
}

impl OneOrMany {
    fn iter(&self) -> impl Iterator<Item = &String> {
        let slice: &[String] = match self {
            Self::None => &[],
            Self::One(s) => std::slice::from_ref(s),
            Self::Many(v) => v,
        };
        slice.iter()
    }
}

#[cfg(test)]
pub(crate) mod tests {
    use super::*;
    use crate::desktoppet::tests::png_header;

    const SAMPLE: &str = r#"
        [pet]
        id = "test"
        name = "Test"

        [appearance]
        rig = "art.svg"
        height = 50.0

        [physics]
        gravity = 2000.0

        [reflexes]
        spawn = "fall"
        land = [{ to = "stand", weight = 1 }]
        edge = [{ to = "turn", weight = 3 }, { to = "stand", when = ["night", "on_window"] }]

        [[behavior]]
        name = "stand"
        motion = "stand"
        duration = [1.0, 2.0]
        next = [{ to = "walk", weight = 2 }, { to = "turn" }]

        [[behavior]]
        name = "walk"
        motion = "walk"
        speed = 40.0
        duration = 5

        [[behavior]]
        name = "turn"
        motion = "turn"
        duration = 0.3
        next = "stand"

        [[behavior]]
        name = "fall"
        motion = "fall"
        duration = "forever"
    "#;

    fn fake_resolve(name: &str) -> Result<Vec<u8>, LoadError> {
        assert_eq!(name, "art.svg");
        Ok(b"<svg/>".to_vec())
    }

    #[test]
    fn parses_sample_pet() {
        let def = parse_pet_toml(SAMPLE, fake_resolve).unwrap();
        assert_eq!(def.behaviors.len(), 4);
        assert_eq!(def.physics.gravity, 2000.0);
        assert_eq!(def.physics.height, 50.0);
        assert_eq!(def.behaviors[1].motion, Motion::Walk { speed: 40.0 });
        assert_eq!(def.behaviors[1].duration, Duration::Range(5.0, 5.0));
        assert_eq!(def.behaviors[3].duration, Duration::Forever);
        assert_eq!(def.reflexes.edge.len(), 2);
        assert_eq!(
            def.reflexes.edge[1].when,
            vec![Condition::Night, Condition::OnWindow]
        );
        assert_eq!(def.default_ground(), 0);
        assert_eq!(def.default_air(), Some(3));
    }

    #[test]
    fn parses_encounters_window_sides_and_farewells() {
        let social = SAMPLE
            .replace(
                "edge = [{ to = \"turn\", weight = 3 }",
                "meet = [{ to = \"stand\", weight = 2 }, { to = \"turn\", weight = 0 }]\n        \
                 window_side = \"turn\"\n        leave = \"stand\"\n        together = \"turn\"\n        \
                 edge = [{ to = \"turn\", weight = 3 }",
            )
            .replace(
                "speed = 40.0",
                "speed = 40.0\n        on_meet = \"turn\"\n        on_window_side = \"stand\"",
            );
        let def = parse_pet_toml(&social, fake_resolve).unwrap();
        let r = &def.reflexes;
        assert_eq!((r.meet.len(), r.meet[1].weight), (2, 0.0));
        assert_eq!(
            (r.window_side[0].to, r.leave[0].to, r.together[0].to),
            (2, 0, 2)
        );
        let walk = &def.behaviors[1];
        assert_eq!((walk.on_meet[0].to, walk.on_window_side[0].to), (2, 0));
        // Une cible inconnue est signalée, comme ailleurs.
        let broken = social.replace("leave = \"stand\"", "leave = \"bye\"");
        assert!(parse_pet_toml(&broken, fake_resolve).is_err());
    }

    #[test]
    fn unknown_target_is_reported() {
        let broken = SAMPLE.replace("next = \"stand\"", "next = \"dance\"");
        let err = parse_pet_toml(&broken, fake_resolve)
            .unwrap_err()
            .to_string();
        assert!(err.contains("dance"), "{err}");
    }

    #[test]
    fn reuses_a_desktoppet_file_with_a_finer_sheet() {
        let toml = r#"
            [pet]
            id = "mouton"
            name = "Mouton"
            author = "Alejandro San Martin Lamas"

            [appearance]
            sprites = "mouton.png"
            tiles = [2, 1]
            scale = 0.25

            [desktoppet]
            animations = "animations.xml"
        "#;
        let def = parse_pet_toml(toml, |name: &str| match name {
            "animations.xml" => Ok(crate::desktoppet::tests::sample().into_bytes()),
            // Cases de 16 × 16 pixels affichées à 25 % : la taille d'origine (4 points).
            "mouton.png" => Ok(png_header(32, 16)),
            other => panic!("{other}"),
        })
        .unwrap();
        assert_eq!(def.meta.name, "Mouton");
        assert_eq!(
            def.meta.author.as_deref(),
            Some("Alejandro San Martin Lamas")
        );
        assert_eq!((def.physics.width, def.physics.height), (4.0, 4.0));
        assert!(def.behavior_index("walk").is_some());
        assert_eq!(def.spawns.len(), 1);
    }

    #[test]
    fn a_leap_onto_a_ledge_has_a_reach_in_leg_lengths() {
        let leap = |reach: &str| {
            let toml = SAMPLE.replace(
                "motion = \"turn\"\n        duration = 0.3",
                &format!("motion = \"leap\"\n        {reach}\n        duration = 0.3"),
            );
            parse_pet_toml(&toml, fake_resolve).map(|def| def.behaviors[2].motion.clone())
        };
        assert!(matches!(leap(""), Ok(Motion::Leap { reach }) if reach == LEAP_REACH));
        assert!(matches!(leap("reach = 2.9"), Ok(Motion::Leap { reach }) if reach == 2.9));
        for wrong in ["reach = 0.0", "reach = -1.0", "reach = 40.0"] {
            let err = leap(wrong).unwrap_err().to_string();
            assert!(err.contains("reach"), "{wrong} : {err}");
        }
    }

    #[test]
    fn unknown_motion_is_reported() {
        let broken = SAMPLE.replace("motion = \"walk\"", "motion = \"moonwalk\"");
        let err = parse_pet_toml(&broken, fake_resolve)
            .unwrap_err()
            .to_string();
        assert!(err.contains("moonwalk"), "{err}");
        // Le message rappelle tous les mouvements possibles, gestes compris.
        let (_, list) = err.rsplit_once('(').unwrap();
        let listed: Vec<&str> = list.trim_end_matches(')').split(", ").collect();
        for name in MOTIONS.into_iter().chain(Gesture::ALL.map(Gesture::name)) {
            assert!(listed.contains(&name), "{name} : {err}");
        }
    }

    /// Animal en bandes d'images, deux sens : images de 64 × 64 pixels.
    pub(crate) const STRIPS: &str = r#"
        [pet]
        id = "laine"
        name = "Laine"

        [appearance]
        strips_left = "g/{name}.png"
        strips_right = "d/{name}.png"
        frame = [64, 64]
        scale = 0.5
        anchor = [32, 56]
        hold = [40, 24]
        drawn = ["shadow", "squash"]

        [reflexes]
        spawn = "chute"
        fall = "chute"
        land = "repos"
        held = "tenu"
        thrown = "chute"

        [choices]
        envie = [{ to = "repos", weight = 1 }, { to = "marche", weight = 3 }]

        [[behavior]]
        name = "repos"
        fps = 8
        loops = [1, 2]
        next = "envie"

        [[behavior]]
        name = "marche"
        fps = 10
        loops = 3
        move = [4, 0]
        on_wall = "demi_tour"
        next = "envie"

        [[behavior]]
        name = "demi_tour"
        fps = 12
        turns = true
        next = "marche"

        [[behavior]]
        name = "cogne"
        strip = "marche"
        frames = [3, 2, 1, 0]
        move = [[0, 0], [-4, 0], [-2, 0], [0, 0]]
        next = "repos"

        [[behavior]]
        name = "chute"
        motion = "fall"
        duration = "forever"

        [[behavior]]
        name = "tenu"
        motion = "held"
        duration = "forever"
    "#;

    /// Bandes : repos 4 images, marche 4, demi-tour 3, chute 2, tenu 2.
    pub(crate) fn strips_resolve(name: &str) -> Result<Vec<u8>, LoadError> {
        let (dir, file) = name.split_once('/').unwrap();
        assert!(dir == "g" || dir == "d", "{name}");
        let frames = match file {
            "repos.png" | "marche.png" => 4,
            "demi_tour.png" => 3,
            "chute.png" | "tenu.png" => 2,
            other => panic!("{other}"),
        };
        Ok(png_header(64 * frames, 64))
    }

    #[test]
    fn parses_a_pet_drawn_in_strips() {
        let def = parse_pet_toml(STRIPS, strips_resolve).unwrap();
        let Appearance::Sprites(sheet) = &def.appearance else {
            panic!()
        };
        assert_eq!(sheet.facings, Facings::Both);
        // Bandes dans l'ordre des comportements, gauche puis droite : 15 images par sens.
        assert_eq!(sheet.pages.len(), 10);
        assert_eq!(sheet.frames_per_facing(), 15);
        assert!(sheet.drawn.shadow && sheet.drawn.squash && !sheet.drawn.dust);
        // Boîte : la largeur d'une image et la hauteur jusqu'au point d'appui.
        assert_eq!((def.physics.width, def.physics.height), (32.0, 28.0));
        // L'ancienne clé `hold` est acceptée, et sans effet.

        let get = |name: &str| &def.behaviors[def.behavior_index(name).unwrap()];
        let repos = get("repos").clip.as_ref().unwrap();
        assert_eq!(repos.frames, vec![0, 1, 2, 3]);
        assert!((repos.interval_start - 0.125).abs() < 1e-6);
        assert!(repos.repeat_expr.is_some(), "un ou deux passages");
        let marche = get("marche");
        assert_eq!(marche.clip.as_ref().unwrap().frames, vec![4, 5, 6, 7]);
        assert_eq!(marche.clip.as_ref().unwrap().repeat, 2);
        assert_eq!(marche.duration, Duration::Clip);
        assert!(marche.gravity, "image par image, il vérifie son appui");
        let Motion::Script(script) = &marche.motion else {
            panic!()
        };
        assert_eq!(script.start[0].as_constant(), Some(4.0));
        assert_eq!(script.scale, 0.5);
        // La liste nommée remplace `next = "envie"`.
        assert_eq!(marche.next.len(), 2);
        assert!(get("demi_tour").flip_at_end);
        let cogne = get("cogne");
        assert_eq!(cogne.clip.as_ref().unwrap().frames, vec![7, 6, 5, 4]);
        let Motion::Script(script) = &cogne.motion else {
            panic!()
        };
        assert_eq!(script.steps.as_deref().unwrap()[1], Vec2::new(-4.0, 0.0));
        assert_eq!(get("chute").motion, Motion::Fall);
        assert_eq!(get("tenu").motion, Motion::Held);
        assert_eq!(def.default_ground(), 0);
    }

    #[test]
    fn strips_are_checked() {
        let missing_frame = STRIPS.replace("frames = [3, 2, 1, 0]", "frames = [4]");
        let err = parse_pet_toml(&missing_frame, strips_resolve)
            .unwrap_err()
            .to_string();
        assert!(err.contains("l'image 4"), "{err}");

        let wrong_moves = STRIPS.replace("[[0, 0], [-4, 0], [-2, 0], [0, 0]]", "[[0, 0]]");
        let err = parse_pet_toml(&wrong_moves, strips_resolve)
            .unwrap_err()
            .to_string();
        assert!(err.contains("1 déplacements pour 4 images"), "{err}");

        // Le sens droit doit avoir autant d'images que le gauche.
        let err = parse_pet_toml(STRIPS, |name: &str| {
            if name == "d/marche.png" {
                Ok(png_header(64 * 3, 64))
            } else {
                strips_resolve(name)
            }
        })
        .unwrap_err()
        .to_string();
        assert!(err.contains("pas autant"), "{err}");

        let clash = STRIPS
            .replace("envie = [", "repos = [")
            .replace("\"envie\"", "\"repos\"");
        assert!(parse_pet_toml(&clash, strips_resolve).is_err());
    }

    /// Le même animal avec des images intermédiaires : une après chaque
    /// image du repos et de la marche, rien pour les autres bandes, et rien
    /// après la dernière image de la marche.
    pub(crate) fn strips_with_inbetweens() -> String {
        STRIPS.replace(
            "drawn = [\"shadow\", \"squash\"]",
            "drawn = [\"shadow\", \"squash\"]\n        inbetweens = \"entre.toml\"",
        )
    }

    pub(crate) fn inbetweens_resolve(name: &str) -> Result<Vec<u8>, LoadError> {
        match name {
            "entre.toml" => Ok(br#"
                count = 1
                strips_left = "eg/{name}.png"
                strips_right = "ed/{name}.png"
                skip = { marche = [3] }
            "#
            .to_vec()),
            "eg/repos.png" | "ed/repos.png" | "eg/marche.png" | "ed/marche.png" => {
                Ok(png_header(64 * 4, 64))
            }
            _ if name.starts_with('e') => Err(LoadError::Format(format!("{name} absent"))),
            _ => strips_resolve(name),
        }
    }

    #[test]
    fn a_behavior_can_speed_up_or_slow_down() {
        // Un choc vif (20 images par seconde), puis un retour plus lent (8).
        let toml = STRIPS.replace(
            "name = \"cogne\"\n        strip = \"marche\"",
            "name = \"cogne\"\n        fps = [20, 8]\n        strip = \"marche\"",
        );
        let def = parse_pet_toml(&toml, strips_resolve).unwrap();
        let clip = def.behaviors[def.behavior_index("cogne").unwrap()]
            .clip
            .as_ref()
            .unwrap();
        assert!((clip.interval_start - 0.05).abs() < 1e-6);
        assert!((clip.interval_end - 0.125).abs() < 1e-6);
        let bad = STRIPS.replace(
            "name = \"repos\"\n        fps = 8",
            "name = \"repos\"\n        fps = [8, 0]",
        );
        assert!(parse_pet_toml(&bad, strips_resolve).is_err());
    }

    #[test]
    fn strips_can_have_inbetweens() {
        let def = parse_pet_toml(&strips_with_inbetweens(), inbetweens_resolve).unwrap();
        let sheet = def.sprites().unwrap();
        let inbetweens = sheet.inbetweens.as_deref().unwrap();
        // Repos et marche, à gauche puis à droite ; numéros après les 30 cases.
        assert_eq!(inbetweens.pages.len(), 4);
        assert_eq!(sheet.inbetween(0, 1), Some((30, 1)));
        assert_eq!(
            sheet.inbetween(3, 0),
            Some((33, 1)),
            "la boucle revient au début"
        );
        assert_eq!(sheet.inbetween(4, 5), Some((34, 1)));
        assert_eq!(sheet.inbetween(6, 7), Some((36, 1)));
        assert_eq!(sheet.inbetween(7, 4), None, "image sautée");
        assert_eq!(sheet.inbetween(1, 0), None, "pas à rebours");
        assert_eq!(sheet.inbetween(8, 9), None, "demi-tour : pas d'images");

        // Une bande d'images intermédiaires doit en avoir une par image.
        let err = parse_pet_toml(&strips_with_inbetweens(), |name: &str| {
            if name == "ed/marche.png" {
                Ok(png_header(64 * 3, 64))
            } else {
                inbetweens_resolve(name)
            }
        })
        .unwrap_err()
        .to_string();
        assert!(err.contains("ed/marche.png"), "{err}");
    }

    #[test]
    fn a_fast_strip_can_have_more_inbetweens() {
        // La marche en a deux après chaque image, le repos une seule.
        let resolve = |name: &str| match name {
            "entre.toml" => Ok(br#"
                count = 1
                strips_left = "eg/{name}.png"
                strips_right = "ed/{name}.png"
                skip = { marche = [3] }
                counts = { marche = 2 }
            "#
            .to_vec()),
            "eg/marche.png" | "ed/marche.png" => Ok(png_header(64 * 8, 64)),
            _ => inbetweens_resolve(name),
        };
        let def = parse_pet_toml(&strips_with_inbetweens(), resolve).unwrap();
        let sheet = def.sprites().unwrap();
        assert_eq!(sheet.inbetween(0, 1), Some((30, 1)));
        assert_eq!(sheet.inbetween(4, 5), Some((34, 2)));
        assert_eq!(sheet.inbetween(6, 7), Some((38, 2)));
        assert_eq!(sheet.inbetween(7, 4), None, "image sautée");

        // Une bande à deux images par image doit les avoir toutes.
        let short = |name: &str| match name {
            "eg/marche.png" | "ed/marche.png" => Ok(png_header(64 * 4, 64)),
            _ => resolve(name),
        };
        assert!(parse_pet_toml(&strips_with_inbetweens(), short).is_err());
    }

    #[test]
    fn a_single_sheet_can_have_inbetweens() {
        let toml = r#"
            [pet]
            id = "p"
            name = "P"
            [appearance]
            sprites = "p.png"
            tiles = [4, 1]
            inbetweens = "e.toml"
            [[behavior]]
            name = "pas"
            frames = [0, 1, 2]
        "#;
        let resolve = |pairs: &'static str| {
            move |name: &str| match name {
                "p.png" => Ok(png_header(64, 16)),
                "e.png" => Ok(png_header(48, 32)),
                "e.toml" => Ok(
                    format!("sprites = \"e.png\"\ntiles = [3, 2]\npairs = {pairs}").into_bytes(),
                ),
                other => panic!("{other}"),
            }
        };
        let def = parse_pet_toml(toml, resolve("[[0, 1, 0, 3], [1, 2, 3, 2]]")).unwrap();
        let sheet = def.sprites().unwrap();
        assert_eq!(sheet.inbetween(0, 1), Some((4, 3)));
        assert_eq!(sheet.inbetween(1, 2), Some((7, 2)));
        assert_eq!(sheet.inbetween(2, 0), None);
        let err = parse_pet_toml(toml, resolve("[[0, 9, 0, 1]]"))
            .unwrap_err()
            .to_string();
        assert!(err.contains("0 → 9"), "{err}");
        let err = parse_pet_toml(toml, resolve("[[0, 1, 5, 2]]"))
            .unwrap_err()
            .to_string();
        assert!(err.contains("6 cases"), "{err}");
    }

    /// Le même animal, qui bêle et dont la clochette tinte en marchant.
    pub(crate) fn strips_with_sounds() -> String {
        STRIPS
            .replace(
                "[choices]",
                "[sounds]\n        bele = { files = [\"s/bele-1.mp3\", \"s/bele-2.mp3\"], volume = 0.8 }\n        clochette = \"s/clochette.mp3\"\n\n        [choices]",
            )
            .replace(
                "name = \"repos\"\n        fps = 8",
                "name = \"repos\"\n        sound = \"bele\"\n        fps = 8",
            )
            .replace(
                "name = \"marche\"\n        fps = 10",
                "name = \"marche\"\n        sound = [{ name = \"clochette\", frame = 2 }, { name = \"bele\", chance = 0.25 }]\n        fps = 10",
            )
    }

    pub(crate) fn sounds_resolve(name: &str) -> Result<Vec<u8>, LoadError> {
        match name.strip_prefix("s/") {
            Some(file) => Ok(file.as_bytes().to_vec()),
            None => strips_resolve(name),
        }
    }

    #[test]
    fn behaviors_can_make_sounds() {
        let def = parse_pet_toml(&strips_with_sounds(), sounds_resolve).unwrap();
        assert_eq!(def.sounds.len(), 2);
        let bele = &def.sounds[0];
        assert_eq!((bele.name.as_str(), bele.volume), ("bele", 0.8));
        assert_eq!(&*bele.clips[1], b"bele-2.mp3");
        assert_eq!(def.sounds[1].volume, 1.0);
        let get = |name: &str| &def.behaviors[def.behavior_index(name).unwrap()];
        assert_eq!(
            get("repos").sounds,
            vec![SoundCue {
                sound: 0,
                chance: 1.0,
                frame: None
            }]
        );
        assert_eq!(
            get("marche").sounds,
            vec![
                SoundCue {
                    sound: 1,
                    chance: 1.0,
                    frame: Some(2)
                },
                SoundCue {
                    sound: 0,
                    chance: 0.25,
                    frame: None
                },
            ]
        );
        assert!(get("demi_tour").sounds.is_empty());

        for (broken, expected) in [
            (
                strips_with_sounds().replace("sound = \"bele\"", "sound = \"meuh\""),
                "meuh",
            ),
            (
                strips_with_sounds().replace("frame = 2", "frame = 4"),
                "image 4",
            ),
            (
                strips_with_sounds().replace("chance = 0.25", "chance = 25"),
                "chance 25",
            ),
        ] {
            let err = parse_pet_toml(&broken, sounds_resolve)
                .unwrap_err()
                .to_string();
            assert!(err.contains(expected), "{err}");
        }
    }

    #[test]
    fn a_single_sheet_can_face_right() {
        let toml = r#"
            [pet]
            id = "p"
            name = "P"
            [appearance]
            sprites = "p.png"
            tiles = [4, 1]
            facing = "right"
            [[behavior]]
            name = "repos"
            frames = [0, 1]
            [[behavior]]
            name = "pas"
            frames = [2, 3, 4]
        "#;
        let err = parse_pet_toml(toml, |_: &str| Ok(png_header(64, 16)))
            .unwrap_err()
            .to_string();
        assert!(err.contains("la case 4"), "{err}");
        let def = parse_pet_toml(&toml.replace("[2, 3, 4]", "[2, 3]"), |_: &str| {
            Ok(png_header(64, 16))
        })
        .unwrap();
        assert_eq!(def.sprites().unwrap().facings, Facings::Right);
        // Sans point d'appui : le bas de la case.
        assert_eq!((def.physics.width, def.physics.height), (16.0, 16.0));
    }
}
