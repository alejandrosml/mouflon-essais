//! Import des animaux desktopPet (eSheep 64 et ceux de la communauté).
//!
//! Les règles reproduisent le moteur de desktopPet d'Adriano Petrucci
//! (licence MIT, © 2016 Adriano Petrucci) :
//! - une animation compte `T` pas (images et répétitions), plus un dernier
//!   qui garde la dernière image ; à chaque pas, l'animal avance de `x`
//!   pixels dans le sens de son dessin et descend de `y` pixels, valeurs
//!   interpolées du premier au dernier pas ;
//! - l'action `flip` retourne l'animal en fin de séquence ;
//! - `border` réagit aux bords (côtés et haut de l'écran, sol, bout de la
//!   fenêtre sous les pattes, haut d'une fenêtre en descendant), `gravity`
//!   à l'absence d'appui ; l'attribut `only` réserve une suite à un type de bord ;
//! - les noms `drag`, `fall`, `toss`, `fall soft`, `fall hard`, `kill`
//!   (au revoir) et `sync` (tous ensemble) sont réservés ;
//! - les suites `only="windowSide"` réagissent au côté d'une fenêtre qui
//!   barre le chemin, `only="petSide"` à la rencontre d'un autre animal ;
//! - `child` fait apparaître des personnages secondaires.
//!
//! Le lancer (`toss`) passe par la physique de Mouflon, et le déplacement
//! est réparti sur la durée de chaque pas plutôt qu'appliqué d'un coup.

use std::collections::{HashMap, HashSet};
use std::sync::Arc;

use base64::Engine as _;
use base64::engine::{DecodePaddingMode, GeneralPurpose, GeneralPurposeConfig};
use roxmltree::{Document, Node};

use crate::def::{
    Appearance, Behavior, ChildSpawn, Condition, Duration, Motion, PetDef, PetMeta, Physics,
    Reflexes, Rules, Script, Sound, SoundCue, SpawnPoint, SpriteClip, SpriteSheet, Transition,
    png_size,
};
use crate::expr::Expr;
use crate::format::LoadError;

/// Ce qu'un `pet.toml` peut changer à un animal desktopPet.
#[derive(Clone, Debug, Default)]
pub struct Overrides {
    pub meta: Option<PetMeta>,
    /// Planche redessinée, même disposition que l'originale (elle peut être
    /// bien plus fine : son `scale` règle la taille d'affichage).
    pub sprites: Option<SpriteSheet>,
    /// Taille des cases d'origine (pixels), pour un fichier dont l'image a
    /// été retirée : les déplacements du fichier sont exprimés à cette taille.
    pub original_tile: Option<(u32, u32)>,
}

/// Événement qui déclenche une transition (`where` dans desktopPet).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Only {
    None,
    Taskbar,
    Window,
    WindowSide,
    Horizontal,
    HorizontalPlus,
    Vertical,
    VerticalPlus,
    PetSide,
}

impl Only {
    /// Valeurs exactes ; toute autre valeur vaut `none`, comme dans desktopPet.
    fn parse(value: Option<&str>) -> Self {
        match value {
            Some("taskbar") => Self::Taskbar,
            Some("window") => Self::Window,
            Some("windowSide") => Self::WindowSide,
            Some("horizontal") => Self::Horizontal,
            Some("horizontal+") => Self::HorizontalPlus,
            Some("vertical") => Self::Vertical,
            Some("vertical+") => Self::VerticalPlus,
            Some("petSide") => Self::PetSide,
            _ => Self::None,
        }
    }

    /// Une suite marquée `self` est-elle possible pour l'événement `event` ?
    fn matches(self, event: Self) -> bool {
        self == Self::None
            || self == event
            || (self == Self::HorizontalPlus
                && matches!(event, Self::Horizontal | Self::Window | Self::Taskbar))
            || (self == Self::VerticalPlus
                && matches!(event, Self::Vertical | Self::WindowSide | Self::PetSide))
    }
}

#[derive(Clone, Debug)]
struct RawNext {
    id: u32,
    weight: f32,
    only: Only,
}

#[derive(Clone, Debug)]
struct RawMove {
    x: String,
    y: String,
    interval_ms: f64,
    offset_y: f32,
    opacity: f32,
}

#[derive(Clone, Debug)]
struct RawAnim {
    id: u32,
    name: String,
    start: RawMove,
    end: RawMove,
    frames: Vec<u32>,
    repeat: String,
    repeat_from: usize,
    flip: bool,
    next: Vec<RawNext>,
    border: Vec<RawNext>,
    gravity: Option<Vec<RawNext>>,
}

#[derive(Clone, Debug)]
struct RawChild {
    animation: u32,
    x: String,
    y: String,
    next: u32,
}

fn elements<'a, 'i>(node: Node<'a, 'i>, name: &'a str) -> impl Iterator<Item = Node<'a, 'i>> + 'a {
    node.children()
        .filter(move |n| n.is_element() && n.tag_name().name() == name)
}

fn element<'a, 'i>(node: Node<'a, 'i>, name: &'a str) -> Option<Node<'a, 'i>> {
    elements(node, name).next()
}

/// Tout le texte d'un élément (il peut être découpé en plusieurs sections CDATA).
fn text_of(node: Node<'_, '_>) -> String {
    node.descendants()
        .filter(Node::is_text)
        .filter_map(|n| n.text())
        .collect::<String>()
        .trim()
        .to_owned()
}

fn text(node: Node<'_, '_>, name: &str) -> Option<String> {
    element(node, name).map(text_of)
}

fn number<T: std::str::FromStr>(node: Node<'_, '_>, name: &str) -> Option<T> {
    text(node, name).and_then(|t| t.trim().parse().ok())
}

/// Analyse tolérante : une expression illisible vaut 0 et le fichier reste utilisable.
fn expr(source: &str, context: &str) -> Expr {
    let source = source.trim();
    if source.is_empty() {
        return Expr::constant(0.0);
    }
    Expr::parse(source).unwrap_or_else(|e| {
        log::warn!("desktopPet, {context} : {e} (remplacée par 0)");
        Expr::constant(0.0)
    })
}

/// Les répétitions changent à chaque fois : desktopPet compte alors les pas autrement.
fn is_dynamic(e: &Expr) -> bool {
    e.variables()
        .iter()
        .any(|v| matches!(*v, "random" | "randS" | "imageX" | "imageY"))
}

/// Valeurs de référence pour ce qui est calculé une fois pour toutes à l'import.
struct Reference {
    image_w: f64,
    image_h: f64,
}

impl Reference {
    fn eval(&self, e: &Expr) -> f64 {
        e.eval(&mut |name| {
            Some(match name {
                "screenW" | "areaW" => 1440.0,
                "screenH" => 900.0,
                "areaH" => 860.0,
                "imageW" => self.image_w,
                "imageH" => self.image_h,
                "random" | "randS" => 50.0,
                "scale" => 1.0,
                _ => return None,
            })
        })
        .trunc()
    }
}

fn parse_move(
    node: Option<Node<'_, '_>>,
    fallback: Option<&RawMove>,
    reference: &Reference,
) -> RawMove {
    let Some(node) = node else {
        return fallback.cloned().unwrap_or(RawMove {
            x: "0".into(),
            y: "0".into(),
            interval_ms: 200.0,
            offset_y: 0.0,
            opacity: 1.0,
        });
    };
    RawMove {
        x: text(node, "x").unwrap_or_default(),
        y: text(node, "y").unwrap_or_default(),
        // L'intervalle est une expression, presque toujours constante.
        interval_ms: text(node, "interval")
            .map_or(200.0, |t| reference.eval(&expr(&t, "<interval>")))
            .max(1.0),
        offset_y: number(node, "offsety").unwrap_or(0.0),
        opacity: number::<f32>(node, "opacity")
            .unwrap_or(1.0)
            .clamp(0.0, 1.0),
    }
}

fn parse_nexts(node: Option<Node<'_, '_>>) -> Vec<RawNext> {
    let Some(node) = node else {
        return Vec::new();
    };
    elements(node, "next")
        .filter_map(|n| {
            let id = text_of(n).parse::<u32>().ok()?;
            let weight = n
                .attribute("probability")
                .and_then(|p| p.trim().parse::<f32>().ok())
                .unwrap_or(0.0);
            Some(RawNext {
                id,
                weight,
                only: Only::parse(n.attribute("only")),
            })
        })
        .collect()
}

fn key_color(name: &str) -> Option<[u8; 3]> {
    let n = name.trim();
    if let Some(hex) = n.strip_prefix('#')
        && hex.len() == 6
    {
        let byte = |i: usize| u8::from_str_radix(&hex[i..i + 2], 16).ok();
        return Some([byte(0)?, byte(2)?, byte(4)?]);
    }
    match n.to_ascii_lowercase().as_str() {
        "magenta" | "fuchsia" => Some([255, 0, 255]),
        "black" => Some([0, 0, 0]),
        "white" => Some([255, 255, 255]),
        "red" => Some([255, 0, 0]),
        "lime" => Some([0, 255, 0]),
        "green" => Some([0, 128, 0]),
        "blue" => Some([0, 0, 255]),
        "yellow" => Some([255, 255, 0]),
        "cyan" | "aqua" => Some([0, 255, 255]),
        _ => None,
    }
}

/// Base64 tolérant : espaces et retours à la ligne ignorés, `=` final facultatif.
fn decode_base64(text: &str) -> Result<Vec<u8>, base64::DecodeError> {
    let engine = GeneralPurpose::new(
        &base64::alphabet::STANDARD,
        GeneralPurposeConfig::new().with_decode_padding_mode(DecodePaddingMode::Indifferent),
    );
    let clean: String = text.chars().filter(|c| !c.is_whitespace()).collect();
    engine.decode(clean.as_bytes())
}

/// Position d'un personnage secondaire quand le parent est retourné (règle
/// textuelle de desktopPet : `-imageW` devient `+imageW`, sinon `imageW`
/// devient `(-imageW)`).
fn mirror_child_x(source: &str) -> String {
    if source.contains("-imageW") {
        source.replace("-imageW", "+imageW")
    } else {
        source.replace("imageW", "(-imageW)")
    }
}

fn slug(text: &str) -> String {
    let id = text
        .chars()
        .map(|c| {
            if c.is_ascii_alphanumeric() {
                c.to_ascii_lowercase()
            } else {
                '-'
            }
        })
        .collect::<String>()
        .split('-')
        .filter(|s| !s.is_empty())
        .collect::<Vec<_>>()
        .join("-");
    if id.is_empty() {
        "desktoppet".into()
    } else {
        id
    }
}

/// Importe un fichier `animations.xml` de desktopPet.
pub fn import(xml: &str) -> Result<PetDef, LoadError> {
    import_with(xml, Overrides::default())
}

/// Importe un fichier desktopPet en remplaçant son nom ou sa planche.
#[allow(clippy::too_many_lines)]
pub fn import_with(xml: &str, overrides: Overrides) -> Result<PetDef, LoadError> {
    let doc = Document::parse(xml).map_err(|e| LoadError::Xml(e.to_string()))?;
    let root = doc.root_element();
    if root.tag_name().name() != "animations" {
        return Err(LoadError::Xml("la racine doit être <animations>".into()));
    }

    // En-tête.
    let header = element(root, "header");
    let header_text = |name: &str| header.and_then(|h| text(h, name)).filter(|t| !t.is_empty());
    let title = header_text("title").unwrap_or_else(|| "desktopPet".into());
    let meta = overrides.meta.clone().unwrap_or_else(|| PetMeta {
        id: slug(&title),
        name: header_text("petname").unwrap_or_else(|| title.clone()),
        author: header_text("author"),
        description: header_text("info").map(|i| i.replace("[br]", "\n")),
        version: header_text("version"),
        license: None,
        category: None,
        region: None,
    });

    // Planche d'origine : elle donne la taille de référence des déplacements.
    let image =
        element(root, "image").ok_or_else(|| LoadError::Xml("élément <image> manquant".into()))?;
    let tiles = |name: &str| number::<u32>(image, name).filter(|v| *v > 0);
    let tiles_x =
        tiles("tilesx").ok_or_else(|| LoadError::Xml("<tilesx> manquant ou invalide".into()))?;
    let tiles_y =
        tiles("tilesy").ok_or_else(|| LoadError::Xml("<tilesy> manquant ou invalide".into()))?;
    let skins = tiles("skins").unwrap_or(1);
    let original = text(image, "png")
        .and_then(|t| decode_base64(&t).ok())
        .filter(|png| png_size(png).is_some());
    let original_tile = overrides
        .original_tile
        .or_else(|| {
            original
                .as_deref()
                .and_then(png_size)
                .map(|(w, h)| (w / tiles_x, h / tiles_y))
        })
        .filter(|(w, h)| *w > 0 && *h > 0);

    let mut sheet = match overrides.sprites {
        // Les images intermédiaires valent pour une seule variante de couleur.
        Some(sheet) => SpriteSheet {
            skins,
            inbetweens: sheet.inbetweens.filter(|_| skins <= 1),
            ..sheet
        },
        None => SpriteSheet {
            key_color: text(image, "transparency").as_deref().and_then(key_color),
            skins,
            ..SpriteSheet::single(
                original
                    .ok_or_else(|| LoadError::Xml("l'image <png> est absente ou illisible".into()))?
                    .into(),
                tiles_x,
                tiles_y,
            )
        },
    };
    let layout_kept = matches!(sheet.pages.as_slice(), [page] if page.tiles_x == tiles_x && page.tiles_y == tiles_y);
    if !layout_kept {
        return Err(LoadError::Format(format!(
            "la planche doit garder la disposition d'origine ({tiles_x} × {tiles_y} cases)"
        )));
    }
    let (tile_w, tile_h) = sheet
        .tile_size()
        .ok_or_else(|| LoadError::Xml("la planche n'est pas un PNG".into()))?;
    let width = tile_w as f32 * sheet.scale;
    let height = tile_h as f32 * sheet.scale;
    if !(width > 0.0 && height > 0.0) {
        return Err(LoadError::Xml(
            "image trop petite pour son nombre de cases".into(),
        ));
    }
    // Taille affichée par rapport à la planche d'origine : les déplacements suivent.
    let k = original_tile.map_or(1.0, |(w, _)| width / w as f32);
    // Les expressions du fichier parlent en pixels de ses images d'origine.
    let reference = Reference {
        image_w: f64::from(width / k),
        image_h: f64::from(height / k),
    };

    // Animations.
    let list = element(root, "animations")
        .ok_or_else(|| LoadError::Xml("élément <animations> manquant".into()))?;
    let mut raws = Vec::new();
    for node in elements(list, "animation") {
        let Some(id) = node
            .attribute("id")
            .and_then(|v| v.trim().parse::<u32>().ok())
        else {
            continue;
        };
        let name = text(node, "name").unwrap_or_default();
        let Some(sequence) = element(node, "sequence") else {
            log::warn!("desktopPet, animation {id} : pas de <sequence>, ignorée");
            continue;
        };
        let frames: Vec<u32> = elements(sequence, "frame")
            .filter_map(|f| text_of(f).parse().ok())
            .collect();
        if frames.is_empty() {
            log::warn!("desktopPet, animation {id} : aucune image, ignorée");
            continue;
        }
        let start = parse_move(element(node, "start"), None, &reference);
        let end = parse_move(element(node, "end"), Some(&start), &reference);
        raws.push(RawAnim {
            id,
            name,
            start,
            end,
            repeat: sequence.attribute("repeat").unwrap_or("0").to_owned(),
            repeat_from: sequence
                .attribute("repeatfrom")
                .and_then(|v| v.trim().parse::<usize>().ok())
                .unwrap_or(0)
                .min(frames.len() - 1),
            frames,
            flip: text(sequence, "action").is_some_and(|a| a.eq_ignore_ascii_case("flip")),
            next: parse_nexts(Some(sequence)),
            border: parse_nexts(element(node, "border")),
            gravity: element(node, "gravity").map(|g| parse_nexts(Some(g))),
        });
    }
    if raws.is_empty() {
        return Err(LoadError::Xml("aucune animation utilisable".into()));
    }
    let index: HashMap<u32, usize> = raws.iter().enumerate().map(|(i, a)| (a.id, i)).collect();

    let children: Vec<RawChild> = element(root, "childs")
        .map(|c| {
            elements(c, "child")
                .filter_map(|n| {
                    Some(RawChild {
                        animation: n.attribute("animationid")?.trim().parse().ok()?,
                        x: text(n, "x").unwrap_or_default(),
                        y: text(n, "y").unwrap_or_default(),
                        next: text(n, "next")?.parse().ok()?,
                    })
                })
                .collect()
        })
        .unwrap_or_default();

    // Transitions : seules les suites possibles pour l'événement sont gardées,
    // avec une condition quand l'événement peut survenir à plusieurs endroits.
    let target = |n: &RawNext| index.get(&n.id).copied();
    let for_event = |list: &[RawNext], event: Only| -> Vec<Transition> {
        list.iter()
            .filter(|n| n.only.matches(event))
            .filter_map(|n| {
                Some(Transition {
                    to: target(n)?,
                    weight: n.weight,
                    when: Vec::new(),
                })
            })
            .collect()
    };
    // Fin de séquence : sur une fenêtre, en bas de l'écran ou ailleurs.
    let sequence_end = |list: &[RawNext]| -> Vec<Transition> {
        list.iter()
            .filter_map(|n| {
                let when = match n.only {
                    Only::None => Vec::new(),
                    Only::Window => vec![Condition::OnWindow],
                    Only::Taskbar => vec![Condition::OnFloor],
                    Only::HorizontalPlus => vec![Condition::Grounded],
                    _ => return None,
                };
                Some(Transition {
                    to: target(n)?,
                    weight: n.weight,
                    when,
                })
            })
            .collect()
    };
    // Arrivée en descendant : sur le sol de l'écran ou sur une fenêtre.
    let landing = |list: &[RawNext]| -> Vec<Transition> {
        list.iter()
            .filter_map(|n| {
                let when = match (n.only.matches(Only::Taskbar), n.only.matches(Only::Window)) {
                    (true, true) => Vec::new(),
                    (true, false) => vec![Condition::OnFloor],
                    (false, true) => vec![Condition::OnWindow],
                    (false, false) => return None,
                };
                Some(Transition {
                    to: target(n)?,
                    weight: n.weight,
                    when,
                })
            })
            .collect()
    };

    // Noms uniques (un nom vide ou en double reçoit l'identifiant).
    let mut counts: HashMap<&str, u32> = HashMap::new();
    for a in &raws {
        *counts.entry(a.name.as_str()).or_default() += 1;
    }
    let mut used = HashSet::new();
    let names: Vec<String> = raws
        .iter()
        .map(|a| {
            let mut name = if a.name.is_empty() {
                format!("#{}", a.id)
            } else if counts[a.name.as_str()] > 1 {
                format!("{}#{}", a.name, a.id)
            } else {
                a.name.clone()
            };
            while !used.insert(name.clone()) {
                name.push('\'');
            }
            name
        })
        .collect();

    let mut behaviors = Vec::with_capacity(raws.len());
    for (i, a) in raws.iter().enumerate() {
        let context = format!("animation {} « {} »", a.id, a.name);
        let motion = match a.name.as_str() {
            "drag" => Motion::Held,
            "toss" => Motion::Fall,
            _ => {
                // Pour Mouflon, x positif va vers l'avant : l'opposé du x de desktopPet.
                let forward = |s: &str, what: &str| {
                    let e = expr(s, &format!("{context}, {what}"));
                    match e.as_constant() {
                        Some(v) => Expr::constant(-v),
                        None => expr(&format!("-({})", s.trim()), &format!("{context}, {what}")),
                    }
                };
                Motion::Script(Arc::new(Script {
                    start: [
                        forward(&a.start.x, "<start><x>"),
                        expr(&a.start.y, &format!("{context}, <start><y>")),
                    ],
                    end: [
                        forward(&a.end.x, "<end><x>"),
                        expr(&a.end.y, &format!("{context}, <end><y>")),
                    ],
                    steps: None,
                    scale: k,
                }))
            }
        };
        let (interval_start, interval_end) = if a.name == "toss" {
            (0.03, 0.03)
        } else {
            (
                (a.start.interval_ms / 1000.0) as f32,
                (a.end.interval_ms / 1000.0) as f32,
            )
        };
        let repeat = expr(&a.repeat, &format!("{context}, repeat"));
        let repeat_const = repeat.as_constant();
        let own_children = children
            .iter()
            .filter(|c| c.animation == a.id)
            .filter_map(|c| {
                Some(ChildSpawn {
                    x: expr(&c.x, "child, <x>"),
                    x_flipped: expr(&mirror_child_x(&c.x), "child, <x>"),
                    y: expr(&c.y, "child, <y>"),
                    next: *index.get(&c.next)?,
                })
            })
            .collect();
        behaviors.push(Behavior {
            name: names[i].clone(),
            motion,
            duration: Duration::Clip,
            next: sequence_end(&a.next),
            on_edge: for_event(&a.border, Only::Window),
            on_wall: for_event(&a.border, Only::Vertical),
            on_fall: a
                .gravity
                .as_deref()
                .map(|g| for_event(g, Only::None))
                .unwrap_or_default(),
            on_land: landing(&a.border),
            on_ceiling: for_event(&a.border, Only::Horizontal),
            // Les suites de poids nul comptent : l'animal peut imiter l'autre.
            on_meet: for_event(&a.border, Only::PetSide),
            on_window_side: for_event(&a.border, Only::WindowSide),
            gravity: a.gravity.is_some(),
            flip_at_end: a.flip,
            walk_off_edges: true,
            chew: false,
            tilt: 0.0,
            twist: false,
            spin: false,
            contagious: 0.0,
            clip: Some(SpriteClip {
                frames: a.frames.clone(),
                interval_start,
                interval_end,
                repeat: repeat_const.map_or(0, |r| r.trunc().clamp(0.0, 100_000.0) as u32),
                dynamic_repeat: is_dynamic(&repeat),
                repeat_expr: if repeat_const.is_some() {
                    None
                } else {
                    Some(repeat)
                },
                repeat_from: a.repeat_from,
                offset_y: a.start.offset_y * k,
                offset_y_end: a.end.offset_y * k,
                opacity_start: a.start.opacity,
                opacity_end: a.end.opacity,
            }),
            children: own_children,
            sounds: Vec::new(),
            // `sleep1a`, `sleep2a`… : il dort ; la suite (`sleep1b`) le réveille.
            sleeping: names[i].contains("sleep"),
        });
    }

    // Une animation de sommeil qui ne vient qu'après une autre (`sleep1b`
    // après `sleep1a`) est son réveil : l'animal n'y dort plus.
    let waking: Vec<usize> = (0..behaviors.len())
        .filter(|&i| behaviors[i].sleeping)
        .filter(|&i| {
            let mut from = behaviors
                .iter()
                .filter(|b| b.next.iter().any(|t| t.to == i))
                .peekable();
            from.peek().is_some() && from.all(|b| b.sleeping)
        })
        .collect();
    for i in waking {
        behaviors[i].sleeping = false;
    }

    // Animations réservées ; desktopPet se rabat sur l'animation 1.
    let named = |wanted: &str| raws.iter().rposition(|a| a.name == wanted);
    let first = index.get(&1).copied().unwrap_or(0);
    let one = |i: usize| {
        vec![Transition {
            to: i,
            weight: 1.0,
            when: Vec::new(),
        }]
    };
    // `kill` et `sync` ne comptent que s'ils ne sont pas l'animation 1
    // (desktopPet : identifiant supérieur à 1).
    let reserved = |wanted: &str| {
        named(wanted)
            .filter(|&i| raws[i].id > 1)
            .map(one)
            .unwrap_or_default()
    };
    let fall = named("fall").unwrap_or(first);
    let reflexes = Reflexes {
        spawn: one(fall),
        fall: one(fall),
        land: one(named("fall soft").unwrap_or(first)),
        land_hard: one(named("fall hard").unwrap_or(first)),
        held: one(named("drag").unwrap_or(first)),
        thrown: one(named("toss").unwrap_or(fall)),
        edge: Vec::new(),
        wall: Vec::new(),
        poke: Vec::new(),
        bump: Vec::new(),
        // Les rencontres et les côtés des fenêtres dépendent de l'animation en cours.
        meet: Vec::new(),
        window_side: Vec::new(),
        leave: reserved("kill"),
        together: reserved("sync"),
        startle: Vec::new(),
        // L'utilisateur parti, il s'endort : la première animation de sommeil.
        sleepy: named("sleep1a")
            .or_else(|| raws.iter().position(|r| r.name.contains("sleep")))
            .map(one)
            .unwrap_or_default(),
        // Réveillé : la suite de son sommeil (`sleep1a` → `sleep1b`).
        wake: Vec::new(),
        wind: Vec::new(),
    };
    // L'animation `toss` de desktopPet fait déjà tourner l'animal sur lui-même.
    sheet.drawn.roll = named("toss").is_some();

    let mut spawns = Vec::new();
    if let Some(list) = element(root, "spawns") {
        for node in elements(list, "spawn") {
            let Some(next) = text(node, "next").and_then(|t| t.parse::<u32>().ok()) else {
                continue;
            };
            let Some(&to) = index.get(&next) else {
                continue;
            };
            spawns.push(SpawnPoint {
                x: expr(&text(node, "x").unwrap_or_default(), "spawn, <x>"),
                y: expr(&text(node, "y").unwrap_or_default(), "spawn, <y>"),
                weight: node
                    .attribute("probability")
                    .and_then(|p| p.trim().parse::<f32>().ok())
                    .unwrap_or(0.0),
                next: to,
            });
        }
    }
    if element(root, "sounds").is_some_and(|s| elements(s, "sound").next().is_some()) {
        log::info!("desktopPet : les sons ne sont pas encore joués");
    }

    // Le lancer suit la physique de Mouflon, à l'échelle de l'animal.
    let physics = Physics {
        gravity: 1700.0 * k,
        terminal_velocity: 2200.0 * k,
        walk_accel: 520.0 * k,
        ground_friction: 1100.0 * k,
        bounce_min_speed: 520.0 * k,
        // desktopPet : chute « dure » au-delà de 40 pixels par 30 ms.
        hard_landing_speed: 1333.0 * k,
        max_throw_speed: 2600.0 * k,
        width,
        height,
        ..Physics::default()
    };
    let mut def = PetDef::new(
        meta,
        Appearance::Sprites(sheet),
        physics,
        behaviors,
        reflexes,
    )?;
    def.set_spawns(spawns)?;
    def.rules = Rules::DesktopPet;
    def.unit = k;

    // Sons (MP3) : chacun joué au début de son animation, selon sa probabilité (%).
    let mut sounds = Vec::new();
    if let Some(list) = element(root, "sounds") {
        for node in elements(list, "sound") {
            let Some(&behavior) = node
                .attribute("animationid")
                .and_then(|v| v.trim().parse::<u32>().ok())
                .and_then(|id| index.get(&id))
            else {
                continue;
            };
            let Some(mp3) = text(node, "base64").and_then(|t| decode_base64(&t).ok()) else {
                continue;
            };
            let chance = number::<f32>(node, "probability").unwrap_or(100.0);
            def.behaviors[behavior].sounds.push(SoundCue {
                sound: sounds.len(),
                chance: (chance / 100.0).clamp(0.0, 1.0),
                frame: None,
            });
            sounds.push(Sound {
                name: def.behaviors[behavior].name.clone(),
                clips: vec![Arc::from(mp3)],
                lamb: Vec::new(),
                volume: 1.0,
                mouth: false,
            });
        }
    }
    def.set_sounds(sounds)?;
    Ok(def)
}

#[cfg(test)]
pub(crate) mod tests {
    use super::*;

    /// PNG minimal : seul l'en-tête est lu à l'import.
    pub(crate) fn png_header(w: u32, h: u32) -> Vec<u8> {
        let mut png = b"\x89PNG\r\n\x1a\n\x00\x00\x00\x0dIHDR".to_vec();
        png.extend_from_slice(&w.to_be_bytes());
        png.extend_from_slice(&h.to_be_bytes());
        png.extend_from_slice(&[8, 6, 0, 0, 0, 0, 0, 0, 0]);
        png
    }

    /// Petit animal desktopPet complet, cases de 4 × 4 pixels.
    pub(crate) fn sample() -> String {
        let png = base64::engine::general_purpose::STANDARD.encode(png_header(8, 4));
        format!(
            r#"<?xml version="1.0" encoding="utf-8"?>
<animations xmlns="https://esheep.petrucci.ch/">
  <header><author>Test</author><title>Test Pet</title><petname>Testy</petname><version>1</version><info>a[br]b</info></header>
  <image><tilesx>2</tilesx><tilesy>1</tilesy><png><![CDATA[{png}]]></png><transparency>Magenta</transparency></image>
  <spawns>
    <spawn id="1" probability="100"><x>screenW+10</x><y>areaH-imageH</y><next probability="100">1</next></spawn>
  </spawns>
  <animations>
    <animation id="1"><name>walk</name>
      <start><x>-2</x><y>0</y><interval>200</interval></start><end><x>-2</x><y>0</y><interval>200</interval></end>
      <sequence repeat="5+random/10" repeatfrom="0"><frame>0</frame><frame>1</frame>
        <next probability="90" only="none">1</next><next probability="10" only="window">3</next><next probability="5" only="taskbar">8</next></sequence>
      <border><next probability="100" only="none">2</next><next probability="20" only="window">3</next><next probability="2" only="vertical">4</next><next probability="50" only="windowSide">2</next></border>
      <gravity><next probability="100">5</next></gravity>
    </animation>
    <animation id="2"><name>rotate</name>
      <start><x>0</x><y>0</y><interval>200</interval></start><end><x>0</x><y>0</y><interval>200</interval></end>
      <sequence repeat="0" repeatfrom="0"><frame>1</frame><action>flip</action><next probability="100">1</next></sequence>
    </animation>
    <animation id="3"><name>look_down</name>
      <start><x>0</x><y>0</y><interval>120</interval></start><end><x>0</x><y>0</y><interval>120</interval></end>
      <sequence repeat="0" repeatfrom="0"><frame>0</frame><next probability="100">1</next></sequence>
    </animation>
    <animation id="4"><name>climb</name>
      <start><x>0</x><y>-2</y><interval>150</interval></start><end><x>0</x><y>-2</y><interval>150</interval></end>
      <sequence repeat="5000" repeatfrom="0"><frame>0</frame><next probability="100">4</next></sequence>
      <border><next probability="100">2</next></border>
    </animation>
    <animation id="5"><name>fall</name>
      <start><x>0</x><y>1</y><interval>100</interval></start><end><x>0</x><y>10</y><interval>40</interval></end>
      <sequence repeat="20" repeatfrom="0"><frame>1</frame><next probability="100">5</next></sequence>
      <border><next probability="100">6</next></border>
    </animation>
    <animation id="6"><name>fall soft</name>
      <start><x>0</x><y>0</y><interval>200</interval><offsety>1</offsety></start><end><x>0</x><y>0</y><interval>200</interval><offsety>0</offsety></end>
      <sequence repeat="0" repeatfrom="0"><frame>0</frame><next probability="100">1</next></sequence>
    </animation>
    <animation id="7"><name>drag</name>
      <start><x>0</x><y>0</y><interval>50</interval></start><end><x>0</x><y>0</y><interval>50</interval></end>
      <sequence repeat="0" repeatfrom="0"><frame>0</frame><next probability="100">1</next></sequence>
    </animation>
    <animation id="8"><name>eat</name>
      <start><x>0</x><y>0</y><interval>100</interval></start><end><x>0</x><y>0</y><interval>100</interval></end>
      <sequence repeat="0" repeatfrom="0"><frame>0</frame><next probability="100">1</next></sequence>
    </animation>
    <animation id="10"><name>toss</name>
      <start><x>0</x><y>0</y><interval>30</interval></start><end><x>0</x><y>0</y><interval>30</interval></end>
      <sequence repeat="0" repeatfrom="0"><frame>0</frame><frame>1</frame></sequence>
    </animation>
    <animation id="11"><name>fall hard</name>
      <start><x>0</x><y>0</y><interval>200</interval></start><end><x>0</x><y>0</y><interval>200</interval></end>
      <sequence repeat="0" repeatfrom="0"><frame>0</frame><next probability="100">1</next></sequence>
    </animation>
    <animation id="9"><name>flower</name>
      <start><x>0</x><y>0</y><interval>100</interval></start><end><x>0</x><y>0</y><interval>100</interval></end>
      <sequence repeat="3" repeatfrom="0"><frame>1</frame></sequence>
    </animation>
  </animations>
  <childs>
    <child animationid="8"><x>imageX-imageW*0.9</x><y>imageY</y><next>9</next></child>
  </childs>
</animations>"#
        )
    }

    fn behavior<'a>(def: &'a PetDef, name: &str) -> &'a Behavior {
        &def.behaviors[def.behavior_index(name).unwrap()]
    }

    fn names(def: &PetDef, list: &[Transition]) -> Vec<String> {
        list.iter()
            .map(|t| def.behaviors[t.to].name.clone())
            .collect()
    }

    #[test]
    fn imports_behaviors_with_their_semantics() {
        let def = import(&sample()).unwrap();
        assert_eq!(def.meta.name, "Testy");
        assert_eq!(def.meta.id, "test-pet");
        assert_eq!(def.meta.description.as_deref(), Some("a\nb"));
        assert_eq!((def.physics.width, def.physics.height), (4.0, 4.0));

        let walk = behavior(&def, "walk");
        let Motion::Script(script) = &walk.motion else {
            panic!("{:?}", walk.motion)
        };
        // -2 pixels par pas dans desktopPet = 2 points vers l'avant.
        assert_eq!(script.start[0].as_constant(), Some(2.0));
        let clip = walk.clip.as_ref().unwrap();
        assert!(clip.repeat_expr.is_some() && clip.dynamic_repeat);
        assert!(walk.gravity);
        assert_eq!(names(&def, &walk.on_fall), ["fall"]);
        // Côté de l'écran : demi-tour ou escalade ; bout de fenêtre : demi-tour ou regard.
        assert_eq!(names(&def, &walk.on_wall), ["rotate", "climb"]);
        assert_eq!(names(&def, &walk.on_edge), ["rotate", "look_down"]);
        // Côté d'une fenêtre (« none » et « windowSide »), rencontre (« none »).
        assert_eq!(names(&def, &walk.on_window_side), ["rotate", "rotate"]);
        assert_eq!(names(&def, &walk.on_meet), ["rotate"]);
        // Sans animation `kill` ni `sync`, pas d'au revoir ni de jeu commun.
        assert!(def.reflexes.leave.is_empty() && def.reflexes.together.is_empty());
        // Fin de séquence : « only » devient une condition.
        assert_eq!(walk.next[1].when, vec![Condition::OnWindow]);
        assert_eq!(walk.next[2].when, vec![Condition::OnFloor]);

        let rotate = behavior(&def, "rotate");
        assert!(rotate.flip_at_end && !rotate.gravity);
        assert_eq!(names(&def, &behavior(&def, "climb").on_ceiling), ["rotate"]);
        let fall = behavior(&def, "fall");
        assert!(matches!(fall.motion, Motion::Script(_)));
        assert_eq!(names(&def, &fall.on_land), ["fall soft"]);
        let soft = behavior(&def, "fall soft").clip.as_ref().unwrap();
        assert_eq!((soft.offset_y, soft.offset_y_end), (1.0, 0.0));
        assert_eq!(behavior(&def, "drag").motion, Motion::Held);
        assert_eq!(names(&def, &def.reflexes.held), ["drag"]);
        assert_eq!(names(&def, &def.reflexes.thrown), ["toss"]);
        assert_eq!(behavior(&def, "toss").motion, Motion::Fall);

        let eat = behavior(&def, "eat");
        assert_eq!(eat.children.len(), 1);
        assert_eq!(eat.children[0].x_flipped.to_string(), "imageX+imageW*0.9");
        assert_eq!(def.spawns.len(), 1);
        assert_eq!(def.rules, Rules::DesktopPet);
        assert!(def.drawn().roll, "l'animation toss fait tourner l'animal");
        assert!(matches!(
            def.appearance,
            Appearance::Sprites(SpriteSheet {
                key_color: Some([255, 0, 255]),
                ..
            })
        ));
    }

    #[test]
    fn reads_the_sounds_of_its_animations() {
        let mp3 = base64::engine::general_purpose::STANDARD.encode(b"ID3 bee");
        let xml = sample().replace(
            "  <childs>",
            &format!(
                "  <sounds><sound animationid=\"8\"><probability>40</probability><loop>0</loop>\
                 <base64>{mp3}</base64></sound></sounds>\n  <childs>"
            ),
        );
        let def = import(&xml).unwrap();
        assert_eq!(def.sounds.len(), 1);
        assert_eq!(&*def.sounds[0].clips[0], b"ID3 bee");
        let eat = behavior(&def, "eat");
        assert_eq!(eat.sounds.len(), 1);
        assert!((eat.sounds[0].chance - 0.4).abs() < 1e-6);
        assert_eq!(eat.sounds[0].frame, None);
        assert!(behavior(&def, "drag").sounds.is_empty());
    }

    #[test]
    fn only_rules_follow_desktoppet() {
        assert!(Only::None.matches(Only::Window));
        assert!(Only::HorizontalPlus.matches(Only::Taskbar));
        assert!(!Only::HorizontalPlus.matches(Only::Vertical));
        assert!(Only::VerticalPlus.matches(Only::WindowSide));
        assert!(!Only::Window.matches(Only::Taskbar));
        assert_eq!(
            Only::parse(Some("Window")),
            Only::None,
            "valeurs sensibles à la casse"
        );
    }

    #[test]
    fn a_finer_sheet_keeps_the_motion_in_proportion() {
        // Planche quatre fois plus fine, affichée à 50 % : tout est deux fois plus grand.
        let sprites = SpriteSheet {
            scale: 0.5,
            ..SpriteSheet::single(png_header(32, 16).into(), 2, 1)
        };
        let overrides = Overrides {
            meta: None,
            sprites: Some(sprites),
            original_tile: None,
        };
        let def = import_with(&sample(), overrides).unwrap();
        assert_eq!((def.physics.width, def.physics.height), (8.0, 8.0));
        let Motion::Script(script) = &behavior(&def, "walk").motion else {
            panic!()
        };
        assert_eq!(script.scale, 2.0);
        assert_eq!(
            behavior(&def, "fall soft").clip.as_ref().unwrap().offset_y,
            2.0
        );
    }

    /// La scène du bain d'eSheep : le mouton arrive en diagonale du bord
    /// droit de l'écran et plonge dans une baignoire posée au sol (un
    /// personnage secondaire). Le nombre de pas de chaque partie est calculé
    /// d'après la taille de l'écran et des images du fichier (40 pixels).
    fn bath() -> String {
        let png = base64::engine::general_purpose::STANDARD.encode(png_header(80, 40));
        let still = |id: u32, name: &str, extra: &str| {
            format!(
                r#"<animation id="{id}"><name>{name}</name>
      <start><x>0</x><y>0</y><interval>100</interval></start><end><x>0</x><y>0</y><interval>100</interval></end>
      {extra}
    </animation>"#
            )
        };
        let animations = [
            still(1, "stand", r#"<sequence repeat="20" repeatfrom="0"><frame>0</frame><next probability="100">1</next></sequence>"#),
            still(7, "drag", r#"<sequence repeat="0" repeatfrom="0"><frame>0</frame><next probability="100">1</next></sequence>"#),
            r#"<animation id="5"><name>fall</name>
      <start><x>0</x><y>1</y><interval>100</interval></start><end><x>0</x><y>10</y><interval>40</interval></end>
      <sequence repeat="20" repeatfrom="0"><frame>1</frame><next probability="100">5</next></sequence>
      <border><next probability="100">1</next></border>
    </animation>"#.to_owned(),
            r#"<animation id="21"><name>batha</name>
      <start><x>-2</x><y>2</y><interval>30</interval></start><end><x>-2</x><y>2</y><interval>30</interval></end>
      <sequence repeatfrom="0" repeat="(areaH/2+(randS*areaH/2)/120-imageH-68)/2"><frame>0</frame><next probability="100" only="none">22</next></sequence>
    </animation>"#.to_owned(),
            r#"<animation id="22"><name>bathb</name>
      <start><x>-2</x><y>2</y><interval>30</interval></start><end><x>-2</x><y>2</y><interval>32</interval></end>
      <sequence repeatfrom="0" repeat="10"><frame>0</frame><frame>0</frame><frame>0</frame><frame>0</frame><frame>1</frame><next probability="100" only="none">47</next></sequence>
      <border><next probability="100" only="none">47</next></border>
    </animation>"#.to_owned(),
            still(47, "bathc", r#"<sequence repeat="0" repeatfrom="0"><frame>1</frame><frame>1</frame><frame>1</frame><frame>1</frame><frame>1</frame><frame>1</frame><frame>1</frame><frame>1</frame><next probability="100">1</next></sequence>"#),
            r#"<animation id="23"><name>bathw</name>
      <start><x>0</x><y>0</y><interval>30</interval></start><end><x>0</x><y>0</y><interval>30</interval></end>
      <sequence repeatfrom="0" repeat="(areaH/2+(randS*areaH/2)/120-imageH-10)/2"><frame>1</frame><next probability="100" only="none">24</next></sequence>
    </animation>"#.to_owned(),
            r#"<animation id="24"><name>bathz</name>
      <start><x>0</x><y>0</y><interval>30</interval></start><end><x>0</x><y>0</y><interval>50</interval></end>
      <sequence repeatfrom="20" repeat="5"><frame>1</frame><frame>1</frame><frame>1</frame><frame>1</frame><frame>1</frame><frame>1</frame><frame>1</frame><frame>1</frame><frame>1</frame><frame>1</frame><frame>1</frame><frame>1</frame><frame>1</frame><frame>1</frame><frame>1</frame><frame>1</frame><frame>1</frame><frame>1</frame><frame>1</frame><frame>1</frame><frame>1</frame><frame>1</frame><frame>1</frame><frame>1</frame></sequence>
    </animation>"#.to_owned(),
        ]
        .join("\n    ");
        format!(
            r#"<?xml version="1.0" encoding="utf-8"?>
<animations xmlns="https://esheep.petrucci.ch/">
  <header><author>Test</author><title>Bath</title><petname>Bath</petname><version>1</version><info>-</info></header>
  <image><tilesx>2</tilesx><tilesy>1</tilesy><png><![CDATA[{png}]]></png><transparency>Magenta</transparency></image>
  <spawns>
    <spawn id="1" probability="100"><x>screenW+10</x><y>areaH/2-(randS*areaH/2)/120-imageH</y><next probability="0">21</next></spawn>
  </spawns>
  <animations>
    {animations}
  </animations>
  <childs>
    <child animationid="21"><x>screenW+10-areaH/2-(randS*areaH/2)/120</x><y>areaH-imageH</y><next>23</next></child>
  </childs>
</animations>"#
        )
    }

    #[test]
    fn the_sheep_dives_into_the_tub_at_any_size() {
        use crate::math::Rect;
        use crate::pet::{Flock, Input, Pet, TICK};
        use crate::world::{Screen, World};
        let world = World::new(vec![Screen {
            frame: Rect::new(0.0, 0.0, 1440.0, 900.0),
            usable: Rect::new(0.0, 25.0, 1440.0, 810.0),
            scale: 2.0,
        }]);
        // Les cases font 40 pixels : la scène est réglée pour cette taille.
        // Affichée deux ou trois fois plus grande, elle doit rester juste.
        for (size, seed) in [(1.0f32, 3), (2.0, 5), (2.0, 11), (3.0, 7)] {
            let def = Arc::new(import(&bath()).unwrap().scaled(size));
            let mut flock = Flock::new();
            flock.spawn(def, seed, &world, None);
            let mut tub: Option<f32> = None;
            let mut landed = None;
            for _ in 0..(60.0 / TICK) as usize {
                flock.advance(TICK, &world, &Input::default());
                let pets = flock.pets();
                if let Some(child) = pets.iter().find(|p| p.is_child()) {
                    tub = Some(child.position().x);
                }
                let sheep = pets.iter().find(|p| !p.is_child()).unwrap();
                if sheep.behavior_name() == "bathc" {
                    landed = Some((sheep.position(), pets.iter().any(Pet::is_child)));
                    break;
                }
            }
            let (sheep, tub_there) =
                landed.unwrap_or_else(|| panic!("taille {size} : pas de bain"));
            let tub = tub.expect("une baignoire");
            assert!(
                (sheep.x - tub).abs() <= 3.0 * size,
                "taille {size} : mouton en {}, baignoire en {tub}",
                sheep.x
            );
            assert!(
                tub_there,
                "taille {size} : la baignoire est partie avant le plongeon"
            );
        }
    }

    #[test]
    fn rejects_garbage() {
        assert!(import("pas du xml").is_err());
        assert!(import("<animations/>").is_err());
    }

    /// Import d'un vrai fichier si un chemin est fourni (aucun n'est livré avec Mouflon) :
    /// `MOUFLON_DESKTOPPET=…/animations.xml cargo test -- --ignored`
    #[test]
    #[ignore = "nécessite un fichier desktopPet local"]
    fn imports_a_real_file() {
        let Ok(path) = std::env::var("MOUFLON_DESKTOPPET") else {
            return;
        };
        let xml = std::fs::read_to_string(path).unwrap();
        let def = import(&xml).unwrap();
        assert!(def.behaviors.len() > 10);
        eprintln!(
            "{} : {} comportements, {} apparitions",
            def.meta.name,
            def.behaviors.len(),
            def.spawns.len()
        );
    }
}
