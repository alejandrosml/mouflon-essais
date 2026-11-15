//! Définition d'un animal, indépendante du format de fichier d'origine
//! (TOML de Mouflon ou XML de desktopPet).

use std::collections::HashMap;
use std::sync::Arc;

use crate::expr::Expr;
use crate::math::Vec2;

/// Sexe d'un animal, tiré une seule fois à son apparition.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Sex {
    Male,
    Female,
}

/// Proportion de mâles à l'apparition ; les tirages de mouvement restent
/// indépendants. Les bornes fixent le sexe sans changer ce contrat.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct SexRatio {
    male: f32,
}

impl SexRatio {
    pub const MALE: Self = Self { male: 1.0 };
    pub const FEMALE: Self = Self { male: 0.0 };

    pub fn new(male: f32) -> Result<Self, DefError> {
        if !(0.0..=1.0).contains(&male) {
            return Err(DefError::Invalid(
                "la proportion de mâles doit être finie, entre 0 et 1".into(),
            ));
        }
        Ok(Self { male })
    }

    #[must_use]
    pub const fn male_probability(self) -> f32 {
        self.male
    }
}

impl Default for SexRatio {
    fn default() -> Self {
        Self { male: 0.5 }
    }
}

/// Âge déclaré, fixe : le temps passé au bureau ne fait pas vieillir
/// l'animal. Les semaines servent notamment au rythme des tétées.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub enum Age {
    #[default]
    Adult,
    Lamb {
        weeks: f32,
    },
}

/// Les dessins d'un animal par sexe et par âge (`[appearance.male]`,
/// `[appearance.female]`, `[appearance.lamb]` dans `pet.toml`, voir
/// `docs/format.md`). Chaque animal porte, dès son apparition, celui de son
/// sexe et de son âge s'il existe, sinon le dessin de base.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Variant {
    /// Le mâle adulte (le bélier).
    Male,
    /// La femelle adulte (la brebis).
    Female,
    /// Le jeune, mâle ou femelle (l'agneau).
    Lamb,
}

impl Variant {
    pub const ALL: [Self; 3] = [Self::Male, Self::Female, Self::Lamb];

    /// Le nom de sa section dans `pet.toml` (`[appearance.female]`…).
    #[must_use]
    pub fn name(self) -> &'static str {
        match self {
            Self::Male => "male",
            Self::Female => "female",
            Self::Lamb => "lamb",
        }
    }

    /// Le dessin qui revient à un animal de ce sexe et de cet âge.
    #[must_use]
    pub fn of(sex: Sex, age: Age) -> Self {
        match (age, sex) {
            (Age::Lamb { .. }, _) => Self::Lamb,
            (Age::Adult, Sex::Male) => Self::Male,
            (Age::Adult, Sex::Female) => Self::Female,
        }
    }
}

/// L'âge, en semaines, d'un agneau dont rien ne dit l'âge : celui où l'app
/// le fait naître.
pub const LAMB_WEEKS: f32 = 2.0;

/// Un autre dessin du même animal (voir [`PetDef::set_variants`]) : un
/// dessin vectoriel ou un squelette, à sa hauteur affichée, et, s'il les
/// donne, sa largeur et sa foulée (points, comme `width` et `stride` dans
/// `[physics]`).
#[derive(Clone, Debug)]
pub struct VariantDrawing {
    pub appearance: Appearance,
    pub width: Option<f32>,
    pub stride: Option<f32>,
    /// L'âge de l'agneau dessiné, en semaines (`weeks`, pour le dessin de
    /// l'agneau seulement) ; sans valeur, deux semaines.
    pub weeks: Option<f32>,
}

#[derive(Clone, Debug, Default, PartialEq)]
pub struct PetMeta {
    pub id: String,
    pub name: String,
    pub author: Option<String>,
    pub description: Option<String>,
    pub version: Option<String>,
    pub license: Option<String>,
    /// Sa famille (`category = "sheep"` dans `pet.toml`) : voir
    /// [`PetMeta::category`].
    pub category: Option<String>,
    /// D'où vient sa race (`region = "france"` dans `pet.toml`) : voir
    /// [`PetMeta::region`].
    pub region: Option<String>,
}

/// Les régions d'origine, pour ranger une famille qui compte beaucoup
/// d'animaux (les races de moutons du dépôt) : la France, les îles
/// Britanniques, le reste de l'Europe, l'Asie et l'Afrique, les Amériques,
/// l'Océanie. Un `pet.toml` peut en donner une autre, montrée telle quelle.
pub const REGIONS: [&str; 6] = [
    "france",
    "british-isles",
    "europe",
    "asia-africa",
    "americas",
    "oceania",
];

/// Les familles d'animaux, pour ranger les menus et la galerie quand il y en
/// a beaucoup : tous les moutons ensemble, les chats ensemble… Un `pet.toml`
/// peut en donner une autre, montrée telle quelle.
pub const CATEGORIES: [&str; 8] = [
    "sheep", "farm", "cats", "dogs", "birds", "wild", "fantasy", "others",
];

/// Des mots qui trahissent une famille, dans les langues de Mouflon (le nom
/// et la description d'un animal qui ne dit pas la sienne).
const CATEGORY_WORDS: [(&str, &[&str]); 7] = [
    (
        "sheep",
        &[
            "sheep",
            "mouton",
            "brebis",
            "agneau",
            "bélier",
            "lamb",
            "ewe",
            "oveja",
            "cordero",
            "carnero",
            "schaf",
            "lamm",
            "widder",
            "pecora",
            "agnello",
            "montone",
            "schaap",
            "ovelha",
            "cordeiro",
            "carneiro",
            "овц",
            "овеч",
            "барашек",
            "ягн",
            "баран",
            "羊",
            "양",
        ],
    ),
    (
        "cats",
        &[
            "cat",
            "chat",
            "chaton",
            "gato",
            "katze",
            "gatto",
            "kat",
            "кот",
            "кошк",
            "猫",
            "고양이",
            "neko",
        ],
    ),
    (
        "dogs",
        &[
            "dog",
            "chien",
            "chiot",
            "perro",
            "hund",
            "cane",
            "hond",
            "cão",
            "cachorro",
            "собак",
            "пёс",
            "犬",
            "狗",
            "개",
            "puppy",
        ],
    ),
    (
        "birds",
        &[
            "bird", "oiseau", "pájaro", "ave", "vogel", "uccello", "pássaro", "птиц", "鳥", "鸟",
            "새", "duck", "canard", "pingouin", "penguin", "owl", "hibou", "chouette",
        ],
    ),
    (
        "farm",
        &[
            "cow", "vache", "pig", "cochon", "goat", "chèvre", "horse", "cheval", "chicken",
            "poule", "donkey", "âne", "rabbit", "lapin", "vaca", "cerdo", "cabra", "kuh",
            "schwein", "ziege",
        ],
    ),
    (
        "wild",
        &[
            "fox",
            "renard",
            "bear",
            "ours",
            "wolf",
            "loup",
            "deer",
            "cerf",
            "squirrel",
            "écureuil",
            "hedgehog",
            "hérisson",
            "panda",
            "tiger",
            "tigre",
            "lion",
            "zorro",
            "fuchs",
            "volpe",
        ],
    ),
    (
        "fantasy",
        &[
            "dragon", "unicorn", "licorne", "ghost", "fantôme", "alien", "monster", "monstre",
            "robot", "fairy", "fée", "slime",
        ],
    ),
];

impl PetMeta {
    /// La région d'origine de sa race, si son `pet.toml` la dit.
    #[must_use]
    pub fn region(&self) -> Option<&str> {
        self.region
            .as_deref()
            .map(str::trim)
            .filter(|region| !region.is_empty())
    }

    /// Sa famille : celle que dit son `pet.toml`, sinon celle que trahissent
    /// son nom et sa description (un mouton desktopPet se reconnaît), sinon
    /// `others`.
    #[must_use]
    pub fn category(&self) -> &str {
        if let Some(category) = self.category.as_deref().map(str::trim)
            && !category.is_empty()
        {
            return category;
        }
        let text = format!(
            "{} {} {}",
            self.id,
            self.name,
            self.description.as_deref().unwrap_or_default()
        )
        .to_lowercase();
        // Un mot entier pour les mots courts en lettres latines (« cat »
        // n'est pas dans « catapulte ») ; partout ailleurs (« eSheep »).
        let words: Vec<&str> = text
            .split(|c: char| !c.is_alphanumeric())
            .filter(|w| !w.is_empty())
            .collect();
        let has = |needle: &str| {
            if needle.is_ascii() && needle.len() <= 4 {
                words.contains(&needle)
            } else {
                text.contains(needle)
            }
        };
        CATEGORY_WORDS
            .iter()
            .find(|(_, needles)| needles.iter().any(|n| has(n)))
            .map_or("others", |(category, _)| category)
    }
}

/// Apparence de l'animal.
#[derive(Clone, Debug)]
pub enum Appearance {
    /// Dessin vectoriel (SVG) découpé en calques nommés, animé de façon procédurale.
    Rig { svg: Arc<[u8]>, height: f32 },
    /// Planche de sprites (format desktopPet).
    Sprites(SpriteSheet),
    /// Morceaux d'images posés sur un squelette (voir [`crate::skeleton`]),
    /// affiché à `height` points de haut.
    Skeleton {
        skeleton: Arc<crate::skeleton::Skeleton>,
        height: f32,
    },
}

/// Sens dans lequel les images d'une planche sont dessinées.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum Facings {
    /// Tournées vers la gauche (desktopPet) : retournées en miroir vers la droite.
    #[default]
    Left,
    /// Tournées vers la droite : retournées en miroir vers la gauche.
    Right,
    /// Chaque image existe dans les deux sens, rien n'est retourné : dans
    /// chaque variante de couleur, la seconde moitié des cases regarde vers
    /// la droite (même ordre que la première).
    Both,
}

/// Effets que les images dessinent déjà : Mouflon ne les ajoute pas.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Drawn {
    /// Ombre au sol (Mouflon garde la sienne quand l'animal est en l'air).
    pub shadow: bool,
    /// Poussière de course et d'atterrissage.
    pub dust: bool,
    /// Écrasement et étirement.
    pub squash: bool,
    /// Culbute d'un animal lancé ou heurté : les images tournent déjà.
    pub roll: bool,
}

/// Une image PNG découpée en cases de même taille.
#[derive(Clone, Debug)]
pub struct SpritePage {
    pub png: PngData,
    pub tiles_x: u32,
    pub tiles_y: u32,
}

/// Octets d'une planche PNG : en mémoire (image intégrée à un fichier
/// desktopPet, essais), ou dans un fichier relu à chaque décodage. Une
/// planche pèse souvent des mégaoctets : un animal qui ne sert pas n'en
/// coûte ainsi rien, et un animal à l'écran ne garde que ses images décodées.
#[derive(Clone, Debug)]
pub enum PngData {
    Bytes(Arc<[u8]>),
    File {
        path: Arc<std::path::Path>,
        /// Largeur et hauteur, lues dans l'en-tête au chargement.
        size: (u32, u32),
    },
}

impl PngData {
    /// Largeur et hauteur de l'image.
    #[must_use]
    pub fn size(&self) -> Option<(u32, u32)> {
        match self {
            Self::Bytes(bytes) => png_size(bytes),
            Self::File { size, .. } => Some(*size),
        }
    }

    /// Le fichier PNG entier (lu sur le disque s'il n'est pas en mémoire).
    pub fn bytes(&self) -> std::io::Result<std::borrow::Cow<'_, [u8]>> {
        match self {
            Self::Bytes(bytes) => Ok(std::borrow::Cow::Borrowed(bytes)),
            Self::File { path, .. } => std::fs::read(path).map(std::borrow::Cow::Owned),
        }
    }
}

impl From<Vec<u8>> for PngData {
    fn from(bytes: Vec<u8>) -> Self {
        Self::Bytes(Arc::from(bytes))
    }
}

/// Images intermédiaires entre deux images d'une animation, pour un
/// mouvement plus fluide (calculées par `tools/inbetween`).
#[derive(Clone, Debug, Default)]
pub struct Inbetweens {
    /// Leurs planches, de mêmes cases que la planche principale : dans le
    /// premier sens, puis dans l'autre si les images existent dans les deux.
    pub pages: Vec<SpritePage>,
    /// (image de départ, image d'arrivée) → (première image intermédiaire,
    /// nombre d'images). Numéros du premier sens ; ceux des images
    /// intermédiaires suivent toutes les cases de la planche principale.
    pub pairs: HashMap<(u32, u32), (u32, u32)>,
}

impl Inbetweens {
    /// Nombre total de cases de leurs planches.
    #[must_use]
    pub fn tile_count(&self) -> u32 {
        self.pages.iter().map(|p| p.tiles_x * p.tiles_y).sum()
    }
}

#[derive(Clone, Debug)]
pub struct SpriteSheet {
    /// Planches dans l'ordre : les numéros des cases se suivent de l'une à
    /// l'autre (une seule pour desktopPet, une par bande d'animation sinon).
    pub pages: Vec<SpritePage>,
    /// Couleur rendue transparente (le magenta des planches desktopPet).
    pub key_color: Option<[u8; 3]>,
    /// Facteur d'affichage : 1.0 = un pixel de la planche par point.
    pub scale: f32,
    /// Variantes de couleur : la planche contient autant de blocs de cases
    /// consécutifs, et chaque animal en reçoit un (1 = une seule apparence).
    pub skins: u32,
    pub facings: Facings,
    /// Point d'appui (milieu des pattes) dans une case, en pixels, image
    /// tournée vers la gauche ; sans valeur, le milieu du bas de la case.
    pub anchor: Option<[f32; 2]>,
    pub drawn: Drawn,
    /// Images intermédiaires, s'il y en a.
    pub inbetweens: Option<Arc<Inbetweens>>,
}

impl SpriteSheet {
    /// Planche unique façon desktopPet.
    #[must_use]
    pub fn single(png: PngData, tiles_x: u32, tiles_y: u32) -> Self {
        Self {
            pages: vec![SpritePage {
                png,
                tiles_x,
                tiles_y,
            }],
            key_color: None,
            scale: 1.0,
            skins: 1,
            facings: Facings::Left,
            anchor: None,
            drawn: Drawn::default(),
            inbetweens: None,
        }
    }

    /// Images intermédiaires pour aller de l'image `from` à l'image `to`
    /// (numéros du premier sens) : première image et nombre d'images.
    #[must_use]
    pub fn inbetween(&self, from: u32, to: u32) -> Option<(u32, u32)> {
        self.inbetweens
            .as_ref()?
            .pairs
            .get(&(from, to))
            .copied()
            .filter(|&(_, count)| count > 0)
    }

    /// Nombre total de cases.
    #[must_use]
    pub fn tile_count(&self) -> u32 {
        self.pages.iter().map(|p| p.tiles_x * p.tiles_y).sum()
    }

    /// Taille d'une case en pixels, lue dans l'en-tête de la première planche.
    #[must_use]
    pub fn tile_size(&self) -> Option<(u32, u32)> {
        let page = self.pages.first()?;
        let (w, h) = page.png.size()?;
        Some((w / page.tiles_x.max(1), h / page.tiles_y.max(1)))
    }

    /// Cases que désignent les comportements : celles de la première variante
    /// de couleur, dans le premier sens.
    #[must_use]
    pub fn frames_per_facing(&self) -> u32 {
        let facings = if self.facings == Facings::Both { 2 } else { 1 };
        self.tile_count() / (self.skins.max(1) * facings)
    }

    /// Point d'appui dans une case (pixels), image tournée vers la gauche.
    #[must_use]
    pub fn anchor_or_default(&self) -> [f32; 2] {
        self.anchor.unwrap_or_else(|| {
            let (w, h) = self.tile_size().unwrap_or((0, 0));
            [w as f32 * 0.5, h as f32]
        })
    }
}

/// Largeur et hauteur d'un PNG, lues dans son en-tête.
#[must_use]
pub fn png_size(png: &[u8]) -> Option<(u32, u32)> {
    if png.len() < 24 || &png[..8] != b"\x89PNG\r\n\x1a\n" || &png[12..16] != b"IHDR" {
        return None;
    }
    let w = u32::from_be_bytes(png[16..20].try_into().ok()?);
    let h = u32::from_be_bytes(png[20..24].try_into().ok()?);
    Some((w, h))
}

/// Paramètres physiques, en points et secondes.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Physics {
    pub gravity: f32,
    pub terminal_velocity: f32,
    /// Freinage horizontal dans les airs (1/s).
    pub air_drag: f32,
    /// Accélération pour atteindre la vitesse de marche (pt/s²).
    pub walk_accel: f32,
    /// Freinage d'une glissade au sol (pt/s²).
    pub ground_friction: f32,
    /// Coefficient de rebond à l'atterrissage.
    pub bounce: f32,
    /// Vitesse d'impact minimale pour rebondir.
    pub bounce_min_speed: f32,
    /// Au-delà de cette vitesse d'impact, l'atterrissage est « dur ».
    pub hard_landing_speed: f32,
    pub max_throw_speed: f32,
    /// Boîte de l'animal : largeur et hauteur en points.
    pub width: f32,
    pub height: f32,
    /// Distance parcourue pendant un cycle de marche (points) ; 0 : 0,9 fois
    /// la hauteur. Pour qu'un dessin vectoriel ne glisse pas, environ 1,6 fois
    /// la longueur de ses pattes.
    pub stride: f32,
    /// La longueur des mèches de sa toison qui se balancent, en centimètres
    /// de l'animal (`fleece`) ; 0 : celle du rythme de base
    /// ([`FLEECE_LENGTH`]). Une longue mèche se balance plus lentement
    /// qu'une courte, comme un pendule (voir [`Physics::fleece_pace`]).
    pub fleece: f32,
}

/// La longueur de mèche (cm) dont le balancement a le rythme de base du
/// moteur : 2 Hz, la fréquence d'une mèche de 9 cm qui pend comme une tige
/// (f = √(3g / 2L) / 2π).
pub const FLEECE_LENGTH: f32 = 9.0;

impl Physics {
    /// Le rythme du balancement de sa toison, par rapport au rythme de
    /// base : la fréquence d'un pendule va comme l'inverse de la racine de
    /// sa longueur. Les cordes de 38 cm de la Racka battent à 1 Hz, les
    /// mèches de 5 cm d'un agneau à 2,7 Hz ; bornée, pour un dessin aux
    /// mèches démesurées.
    #[must_use]
    pub fn fleece_pace(&self) -> f32 {
        if self.fleece > 0.0 && self.fleece.is_finite() {
            (FLEECE_LENGTH / self.fleece).sqrt().clamp(0.4, 1.5)
        } else {
            1.0
        }
    }
}

impl Default for Physics {
    fn default() -> Self {
        Self {
            gravity: 2200.0,
            terminal_velocity: 1700.0,
            air_drag: 0.6,
            walk_accel: 520.0,
            ground_friction: 1100.0,
            bounce: 0.22,
            bounce_min_speed: 520.0,
            hard_landing_speed: 1150.0,
            max_throw_speed: 2600.0,
            width: 76.0,
            height: 64.0,
            stride: 0.0,
            fleece: 0.0,
        }
    }
}

/// Ce que fait le corps pendant un comportement.
#[derive(Clone, Debug, PartialEq)]
pub enum Motion {
    Stand,
    Walk {
        speed: f32,
    },
    Run {
        speed: f32,
    },
    /// Demi-tour, puis enchaînement.
    Turn,
    Sit,
    Sleep,
    /// Regarde autour de lui (ou le curseur).
    Look,
    /// Broute : la tête au sol, qui grignote.
    Graze,
    /// Saut : vitesse horizontale vers l'avant et verticale vers le haut.
    Jump {
        vx: f32,
        vy: f32,
    },
    Fall,
    /// Récupération après un atterrissage.
    Land,
    /// Tenu par le curseur.
    Held,
    /// Déplacement image par image façon desktopPet (voir [`Script`]).
    Script(Arc<Script>),
    /// Fonce au galop, tête baissée et oreilles couchées : l'élan d'un
    /// animal énervé, qui bouscule celui qu'il atteint.
    Charge {
        speed: f32,
    },
    /// Saute sur place, pattes raides, en secouant la tête : le jeu des
    /// agneaux (vitesses comme pour [`Motion::Jump`]).
    Pronk {
        vx: f32,
        vy: f32,
    },
    /// Un geste sur place, que le moteur anime de lui-même (voir [`Gesture`]).
    Gesture(Gesture),
    /// Recule, sans se retourner (le bélier qui prend son élan avant de
    /// charger) ; il s'arrête au bord plutôt que de tomber à reculons.
    Back {
        speed: f32,
    },
    /// Saute sur un rebord plus haut, tout proche, comme un mouflon sur un
    /// rocher, s'il est à sa portée : `reach`, la hauteur du plus haut
    /// rebord qu'il atteint d'un bond, en longueurs de patte (de l'épaule au
    /// sol, voir [`Physics::stride`]). Il le mesure du regard, recule d'un
    /// pas pour prendre son élan s'il le faut, se ramasse, bondit les pattes
    /// repliées et se reçoit sans glisser (voir `pet/leap.rs`). Sans rebord
    /// à sa portée devant lui, le moteur ne le choisit pas.
    Leap {
        reach: f32,
    },
}

/// Un geste sur place, animé par le moteur pour les animaux dessinés en
/// vecteurs ou en squelette (voir `docs/animation-animale.md` pour ce qu'en
/// disent les études sur les moutons).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Gesture {
    /// S'ébroue : une secousse qui part de la tête et file jusqu'à la queue.
    Shake,
    /// S'étire en se levant : le cou tendu, puis une patte arrière.
    Stretch,
    /// Gratte le sol d'un sabot de devant.
    Paw,
    /// Tape du pied, tête haute : l'alarme des moutons.
    Stamp,
    /// Renifle, le museau tendu.
    Sniff,
    /// Sonné : la tête tourne, les yeux mi-clos.
    Daze,
    /// Se protège (du vent) : tête basse, oreilles couchées.
    Brace,
    /// Bâille, tête levée, la bouche grande ouverte.
    Yawn,
    /// Bêle, tête levée, la bouche ouverte.
    Bleat,
    /// Se gratte d'un sabot arrière, à petits coups rapides.
    Scratch,
    /// Se cabre : l'avant se dresse sur les pattes de derrière, celles de
    /// devant repliées, puis retombe (le bélier, avant de charger).
    Rear,
    /// Invite un autre à jouer : tête et avant baissés, il le fixe en
    /// sautillant sur place.
    Invite,
    /// Le flehmen du bélier : l'encolure tendue, la tête relevée, la lèvre
    /// supérieure retroussée, il hume quelques secondes (après avoir flairé
    /// une brebis).
    Flehmen,
    /// L'agneau tète, museau levé, queue frétillante.
    Nurse,
    /// Appel maternel grave, bouche fermée.
    MaternalCall,
}

impl Gesture {
    pub const ALL: [Self; 15] = [
        Self::Shake,
        Self::Stretch,
        Self::Paw,
        Self::Stamp,
        Self::Sniff,
        Self::Daze,
        Self::Brace,
        Self::Yawn,
        Self::Bleat,
        Self::Scratch,
        Self::Rear,
        Self::Invite,
        Self::Flehmen,
        Self::Nurse,
        Self::MaternalCall,
    ];

    /// Son nom dans un `pet.toml` (`motion = "shake"`…).
    #[must_use]
    pub fn name(self) -> &'static str {
        match self {
            Self::Shake => "shake",
            Self::Stretch => "stretch",
            Self::Paw => "paw",
            Self::Stamp => "stamp",
            Self::Sniff => "sniff",
            Self::Daze => "daze",
            Self::Brace => "brace",
            Self::Yawn => "yawn",
            Self::Bleat => "bleat",
            Self::Scratch => "scratch",
            Self::Rear => "rear",
            Self::Invite => "invite",
            Self::Flehmen => "flehmen",
            Self::Nurse => "nurse",
            Self::MaternalCall => "maternal_call",
        }
    }

    #[must_use]
    pub fn from_name(name: &str) -> Option<Self> {
        Self::ALL.into_iter().find(|g| g.name() == name)
    }

    /// Le geste se fait d'aplomb : il lève un sabot (gratter le sol, taper
    /// du pied, se gratter, s'étirer), l'avant du corps (se cabrer, inviter
    /// à jouer), ou secoue tout le corps (s'ébrouer). Arrêté au milieu d'un
    /// pas, l'animal le finit avant de commencer ; les autres gestes (de la
    /// tête, de la bouche) commencent tout de suite.
    #[must_use]
    pub fn needs_footing(self) -> bool {
        matches!(
            self,
            Self::Shake
                | Self::Stretch
                | Self::Paw
                | Self::Stamp
                | Self::Scratch
                | Self::Rear
                | Self::Invite
        )
    }
}

/// Animation desktopPet : à chaque pas de la séquence, le corps avance de
/// `x` points (vers l'avant) et descend de `y` points (vers le haut si négatif),
/// valeurs interpolées du premier au dernier pas. Pas de gravité : la chute
/// elle-même est une animation, et les bords déclenchent les transitions.
///
/// Les expressions sont recalculées à chaque début de séquence (elles peuvent
/// dépendre de `random`, de la taille de l'écran ou de l'image).
#[derive(Clone, Debug, PartialEq)]
pub struct Script {
    /// `[x, y]` au premier pas.
    pub start: [Expr; 2],
    /// `[x, y]` au dernier pas.
    pub end: [Expr; 2],
    /// Déplacement propre à chaque image de la séquence (x vers l'avant, y
    /// vers le bas), dans l'ordre de ses images ; remplace `start` et `end`.
    pub steps: Option<Arc<[Vec2]>>,
    /// Facteur appliqué aux valeurs calculées (taille d'affichage de l'animal).
    pub scale: f32,
}

impl Script {
    /// Déplacement constant à chaque pas.
    #[must_use]
    pub fn constant(x: f32, y: f32, scale: f32) -> Self {
        let pair = || [Expr::constant(f64::from(x)), Expr::constant(f64::from(y))];
        Self {
            start: pair(),
            end: pair(),
            steps: None,
            scale,
        }
    }
}

impl Motion {
    /// Le comportement se déroule dans les airs.
    #[must_use]
    pub fn is_airborne(&self) -> bool {
        matches!(self, Self::Fall | Self::Jump { .. } | Self::Pronk { .. })
    }

    /// Sa vitesse au sol, s'il avance de lui-même.
    #[must_use]
    pub fn speed(&self) -> Option<f32> {
        match *self {
            Self::Walk { speed }
            | Self::Run { speed }
            | Self::Charge { speed }
            | Self::Back { speed } => Some(speed),
            _ => None,
        }
    }

    #[must_use]
    pub fn pose(&self) -> Pose {
        match self {
            Self::Stand | Self::Turn | Self::Look => Pose::Stand,
            Self::Graze => Pose::Graze,
            Self::Walk { .. } => Pose::Walk,
            Self::Run { .. } | Self::Charge { .. } => Pose::Run,
            Self::Gesture(_) | Self::Back { .. } | Self::Leap { .. } => Pose::Stand,
            Self::Sit => Pose::Sit,
            Self::Sleep => Pose::Sleep,
            Self::Jump { .. } | Self::Pronk { .. } | Self::Fall => Pose::Air,
            Self::Land => Pose::Land,
            Self::Held => Pose::Dangle,
            // Les animaux desktopPet portent leurs poses dans leurs images.
            Self::Script(_) => Pose::Stand,
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Pose {
    Stand,
    Walk,
    Run,
    Sit,
    Sleep,
    Graze,
    Air,
    Land,
    Dangle,
}

/// Durée d'un comportement.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Duration {
    /// Durée tirée au hasard dans l'intervalle (secondes).
    Range(f32, f32),
    /// Jusqu'à la fin de la séquence de sprites.
    Clip,
    /// Jusqu'à ce qu'un événement (atterrissage, bord…) le termine.
    Forever,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Condition {
    OnWindow,
    OnFloor,
    /// Entre 22 h et 6 h, heure locale.
    Night,
    Day,
    CursorNear(f32),
    CursorFar(f32),
    /// Au sol (sur une fenêtre ou en bas de l'écran).
    Grounded,
    /// L'animal a cette humeur (`calm`, `lively`, `rowdy` ou `even`) : il a
    /// ses gestes à lui quand il est énervé ou calme.
    Temper(crate::temper::Temper),
    /// Un coup de vent lui souffle au visage (`into_wind`) : il peut se
    /// retourner, dos au vent, comme le font les moutons.
    IntoWind,
    /// L'utilisateur tape au clavier : une touche il y a moins de
    /// [`crate::TYPING_WINDOW`] secondes (Mouflon sait quand, jamais laquelle).
    Typing,
    /// Personne n'a touché au clavier, à la souris ni au trackpad depuis
    /// [`crate::AWAY_AFTER`] secondes.
    Away,
    /// L'utilisateur est là (le contraire de [`Condition::Away`]).
    Present,
    /// De 6 h à 11 h, heure locale.
    Morning,
    /// De 18 h à 22 h, heure locale.
    Evening,
    /// Le sexe, à tout âge.
    Sex(Sex),
    Adult,
    Lamb,
}

#[derive(Clone, Debug, PartialEq)]
pub struct Transition {
    pub to: usize,
    pub weight: f32,
    pub when: Vec<Condition>,
}

/// Séquence de sprites façon desktopPet.
///
/// Elle compte `T` pas (voir [`SpriteClip::total_steps_with`]) suivis d'un
/// pas de plus qui garde la dernière image, comme dans desktopPet.
#[derive(Clone, Debug, PartialEq)]
pub struct SpriteClip {
    pub frames: Vec<u32>,
    /// Durée d'un pas au début et à la fin de la séquence (secondes).
    pub interval_start: f32,
    pub interval_end: f32,
    /// Nombre de répétitions après le premier passage.
    pub repeat: u32,
    /// Répétitions calculées à chaque début de séquence (expression desktopPet).
    pub repeat_expr: Option<Expr>,
    /// Le nombre de répétitions change à chaque fois (`random`…) : desktopPet
    /// compte alors les pas un peu différemment.
    pub dynamic_repeat: bool,
    /// Indice de la première image répétée.
    pub repeat_from: usize,
    /// Décalage vertical d'affichage au début et à la fin (points, positif vers le bas).
    pub offset_y: f32,
    pub offset_y_end: f32,
    pub opacity_start: f32,
    pub opacity_end: f32,
}

impl SpriteClip {
    /// Nombre de pas de la séquence.
    #[must_use]
    pub fn total_steps(&self) -> usize {
        self.total_steps_with(self.repeat)
    }

    /// Nombre de pas pour un nombre de répétitions donné, selon les règles de
    /// desktopPet : `F + F·R` sans `repeatfrom`, sinon `F + (F − RF − 1)·R`
    /// (ou `F + (F − RF)·R` si les répétitions sont tirées au hasard).
    #[must_use]
    pub fn total_steps_with(&self, repeat: u32) -> usize {
        let n = self.frames.len();
        let from = self.repeat_from.min(n.saturating_sub(1));
        let looped = if from == 0 {
            n
        } else if self.dynamic_repeat {
            n - from
        } else {
            n - from - 1
        };
        n + repeat as usize * looped
    }

    /// Image affichée au pas `step`.
    #[must_use]
    pub fn frame_at(&self, step: usize) -> u32 {
        self.frames
            .get(self.frame_index_at(step))
            .copied()
            .unwrap_or(0)
    }

    /// Position dans `frames` de l'image affichée au pas `step`.
    #[must_use]
    pub fn frame_index_at(&self, step: usize) -> usize {
        let n = self.frames.len();
        if n == 0 || step < n {
            return step.min(n.saturating_sub(1));
        }
        let from = self.repeat_from.min(n - 1);
        let looped = n - from;
        from + (step - n) % looped.max(1)
    }
}

/// Un son de l'animal : une ou plusieurs prises (fichiers audio : MP3, AAC,
/// WAV…), l'une tirée au hasard à chaque fois.
#[derive(Clone, Debug)]
pub struct Sound {
    pub name: String,
    pub clips: Vec<Arc<[u8]>>,
    /// Les prises d'un agneau (`lamb = […]`), s'il a sa voix à lui : elles
    /// remplacent `clips` chez l'agneau (voir [`PetDef::speak_as_lamb`]).
    pub lamb: Vec<Arc<[u8]>>,
    /// Volume, de 0 à 1.
    pub volume: f32,
    /// C'est sa voix : il ouvre la bouche en le faisant entendre (un
    /// bêlement, pas une clochette).
    pub mouth: bool,
}

/// Un son qu'un comportement fait entendre.
#[derive(Clone, Debug, PartialEq)]
pub struct SoundCue {
    /// Rang dans [`PetDef::sounds`].
    pub sound: usize,
    /// Chance de l'entendre à chaque occasion (de 0 à 1).
    pub chance: f32,
    /// Image de la séquence (rang dans ses images) qui le déclenche, à chaque
    /// passage ; sans valeur, au début du comportement.
    pub frame: Option<usize>,
}

#[derive(Clone, Debug)]
pub struct Behavior {
    pub name: String,
    pub motion: Motion,
    pub duration: Duration,
    pub next: Vec<Transition>,
    /// Remplace le réflexe `edge` pendant ce comportement.
    pub on_edge: Vec<Transition>,
    /// Remplace le réflexe `wall` pendant ce comportement.
    pub on_wall: Vec<Transition>,
    /// Remplace le réflexe `fall` quand le sol disparaît pendant ce comportement.
    pub on_fall: Vec<Transition>,
    /// Remplace les réflexes `land` et `land_hard` à l'atterrissage.
    pub on_land: Vec<Transition>,
    /// En montant, l'animal touche le haut de l'écran (animations scriptées).
    pub on_ceiling: Vec<Transition>,
    /// Remplace le réflexe `meet` quand l'animal en rencontre un autre
    /// pendant ce comportement (`petSide` de desktopPet). Une suite de poids
    /// nul n'est jamais tirée au sort, mais l'animal peut imiter l'autre
    /// s'il est en train de la jouer.
    pub on_meet: Vec<Transition>,
    /// Remplace le réflexe `window_side` quand le côté d'une fenêtre barre le
    /// chemin pendant ce comportement (`windowSide` de desktopPet).
    pub on_window_side: Vec<Transition>,
    /// Animations scriptées : vérifie à chaque pas que l'animal a encore un appui
    /// et passe sinon à `on_fall` (élément `gravity` de desktopPet).
    pub gravity: bool,
    /// Fait demi-tour à la fin du comportement (action `flip` de desktopPet).
    pub flip_at_end: bool,
    /// Sans transition de bord, continue tout droit et tombe du rebord au lieu de se retourner.
    pub walk_off_edges: bool,
    pub clip: Option<SpriteClip>,
    /// Personnages secondaires qui apparaissent avec ce comportement
    /// (la fleur que le mouton mange, le mouton noir…).
    pub children: Vec<ChildSpawn>,
    /// Sons qu'il fait entendre.
    pub sounds: Vec<SoundCue>,
    /// Il dort (ou s'endort) : l'utilisateur qui revient le réveille
    /// (réflexe `wake`), un pointeur qui file ne le fait pas sursauter.
    pub sleeping: bool,
    /// Il mâche pendant ce comportement (un ruminant couché qui rumine) :
    /// sa mâchoire, s'il en a une, va et vient.
    pub chew: bool,
    /// Penché en avant (radians ; négatif : en arrière) pendant ce
    /// comportement, en plus de ce que lui donne son élan : un animal en
    /// images qui fonce tête la première.
    pub tilt: f32,
    /// En sautant, le corps se tord et les pattes de derrière ruent : le
    /// saut d'un agneau qui joue (avec [`Motion::Pronk`] ou [`Motion::Jump`]).
    pub twist: bool,
    /// Il se retourne en l'air : le petit saut d'un agneau qui pivote.
    pub spin: bool,
    /// Contagieux (0–1) : la chance qu'un voisin de la même famille l'imite,
    /// un peu plus tard (un mouton qui part entraîne les autres, un
    /// bâillement en appelle d'autres). Le voisin doit avoir un comportement
    /// du même nom.
    pub contagious: f32,
}

/// Personnage secondaire (élément `child` de desktopPet). Il joue ses propres
/// animations puis disparaît quand sa séquence se termine sans suite.
#[derive(Clone, Debug, PartialEq)]
pub struct ChildSpawn {
    /// Coin haut gauche de son image, relatif à l'écran ; `imageX` et `imageY`
    /// désignent la position du parent.
    pub x: Expr,
    /// Variante de `x` quand le parent est retourné (`imageW` change de signe).
    pub x_flipped: Expr,
    pub y: Expr,
    pub next: usize,
}

/// Point d'apparition (format desktopPet) : coin haut gauche de l'image.
#[derive(Clone, Debug, PartialEq)]
pub struct SpawnPoint {
    pub x: Expr,
    pub y: Expr,
    pub weight: f32,
    pub next: usize,
}

/// Réactions aux événements physiques.
#[derive(Clone, Debug, Default)]
pub struct Reflexes {
    pub spawn: Vec<Transition>,
    pub fall: Vec<Transition>,
    pub land: Vec<Transition>,
    pub land_hard: Vec<Transition>,
    pub held: Vec<Transition>,
    pub thrown: Vec<Transition>,
    pub edge: Vec<Transition>,
    pub wall: Vec<Transition>,
    pub poke: Vec<Transition>,
    /// Heurté par un autre animal (mode collisions) ; à défaut, `thrown`.
    pub bump: Vec<Transition>,
    /// Rencontre d'un autre animal en marchant (hors mode collisions). Sans
    /// réaction, les deux animaux se croisent.
    pub meet: Vec<Transition>,
    /// Le côté d'une fenêtre barre le chemin. Sans réaction, l'animal passe
    /// devant la fenêtre.
    pub window_side: Vec<Transition>,
    /// Au revoir, quand on retire l'animal (`kill` de desktopPet) : il joue
    /// ce comportement, puis s'efface.
    pub leave: Vec<Transition>,
    /// Tous ensemble : tous les animaux jouent ce comportement en même temps
    /// (`sync` de desktopPet).
    pub together: Vec<Transition>,
    /// Le pointeur file tout près de lui : il sursaute.
    pub startle: Vec<Transition>,
    /// L'utilisateur est parti (voir [`Condition::Away`]) : il s'installe
    /// pour dormir.
    pub sleepy: Vec<Transition>,
    /// L'utilisateur revient pendant qu'il dort : il se réveille. Sans
    /// réaction prévue, il passe à la suite de son sommeil.
    pub wake: Vec<Transition>,
    /// Un coup de vent se lève (voir [`crate::surprise::Gust`]).
    pub wind: Vec<Transition>,
}

/// Règles du moteur pour ce qui n'est pas prévu par l'animal.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum Rules {
    /// Animal Mouflon : sans suite prévue, il revient à son comportement au
    /// sol ; sans réaction prévue, il fait demi-tour au bord et tombe quand
    /// le sol disparaît.
    #[default]
    Mouflon,
    /// Animal desktopPet : comme le moteur d'origine, une séquence compte un
    /// pas de plus ; sans suite, l'animal réapparaît ; sans réaction prévue,
    /// il sort de l'écran ou tombe du rebord.
    DesktopPet,
}

#[derive(Clone, Debug)]
pub struct PetDef {
    pub meta: PetMeta,
    /// Répartition du sexe à l'apparition, indépendante du comportement.
    pub sex_ratio: SexRatio,
    pub age: Age,
    pub appearance: Appearance,
    pub physics: Physics,
    pub behaviors: Vec<Behavior>,
    pub reflexes: Reflexes,
    /// Points d'apparition ; vide : l'animal tombe du haut de l'écran.
    pub spawns: Vec<SpawnPoint>,
    pub rules: Rules,
    /// Points par pixel du fichier d'origine. Les expressions desktopPet
    /// (positions, nombres de pas) sont écrites pour des images affichées à
    /// leur taille d'origine : elles se calculent dans cette unité, puis se
    /// mettent à la taille d'affichage. 1 pour un animal Mouflon.
    pub unit: f32,
    /// Sons de l'animal (voir [`Behavior::sounds`]).
    pub sounds: Vec<Sound>,
    /// Ses autres dessins, par sexe et par âge (voir
    /// [`PetDef::set_variants`]) : chacun est l'animal entier, à la taille
    /// de son dessin.
    variants: Vec<(Variant, Arc<PetDef>)>,
    by_name: HashMap<String, usize>,
    /// L'allure de chaque comportement (voir [`PetDef::energy`]).
    energies: Vec<crate::temper::Energy>,
    default_ground: usize,
    default_air: Option<usize>,
}

#[derive(Debug, thiserror::Error)]
pub enum DefError {
    #[error("l'animal ne définit aucun comportement")]
    NoBehavior,
    #[error("le comportement « {0} » est défini deux fois")]
    Duplicate(String),
    #[error("« {from} » renvoie vers « {to} », qui n'existe pas")]
    UnknownTarget { from: String, to: String },
    #[error("aucun comportement au sol (stand, walk, sit…) n'est défini")]
    NoGroundBehavior,
    #[error("{0}")]
    Invalid(String),
}

/// Ce qu'un nombre d'une fiche peut valoir au plus (points, points par
/// seconde, secondes…) : très au-delà de tout animal réel.
const MAX_NUMBER: f32 = 100_000.0;

/// `value` est-il un nombre possible pour `name` (fini, d'une grandeur
/// raisonnable, et pas négatif si `positive`) ?
fn possible(name: &str, value: f32, positive: bool) -> Result<(), DefError> {
    if value.is_finite() && value.abs() <= MAX_NUMBER && (!positive || value >= 0.0) {
        Ok(())
    } else {
        Err(DefError::Invalid(format!(
            "`{name}` vaut {value} : ce n'est pas une valeur possible"
        )))
    }
}

impl PetDef {
    /// Assemble et valide une définition.
    pub fn new(
        meta: PetMeta,
        appearance: Appearance,
        physics: Physics,
        behaviors: Vec<Behavior>,
        reflexes: Reflexes,
    ) -> Result<Self, DefError> {
        if behaviors.is_empty() {
            return Err(DefError::NoBehavior);
        }
        let mut by_name = HashMap::with_capacity(behaviors.len());
        for (i, b) in behaviors.iter().enumerate() {
            if by_name.insert(b.name.clone(), i).is_some() {
                return Err(DefError::Duplicate(b.name.clone()));
            }
        }
        let check = |from: &str, list: &[Transition]| -> Result<(), DefError> {
            for t in list {
                if t.to >= behaviors.len() {
                    return Err(DefError::UnknownTarget {
                        from: from.to_owned(),
                        to: format!("#{}", t.to),
                    });
                }
            }
            Ok(())
        };
        for b in &behaviors {
            check(&b.name, &b.next)?;
            check(&b.name, &b.on_edge)?;
            check(&b.name, &b.on_wall)?;
            check(&b.name, &b.on_fall)?;
            check(&b.name, &b.on_land)?;
            check(&b.name, &b.on_ceiling)?;
            check(&b.name, &b.on_meet)?;
            check(&b.name, &b.on_window_side)?;
            if let Some(c) = b.children.iter().find(|c| c.next >= behaviors.len()) {
                return Err(DefError::UnknownTarget {
                    from: b.name.clone(),
                    to: format!("#{}", c.next),
                });
            }
            if let Some(cue) = b.sounds.iter().find(|c| !(0.0..=1.0).contains(&c.chance)) {
                return Err(DefError::Invalid(format!(
                    "« {} » : chance de son {} (attendu entre 0 et 1)",
                    b.name, cue.chance
                )));
            }
        }
        for (name, list) in [
            ("spawn", &reflexes.spawn),
            ("fall", &reflexes.fall),
            ("land", &reflexes.land),
            ("land_hard", &reflexes.land_hard),
            ("held", &reflexes.held),
            ("thrown", &reflexes.thrown),
            ("edge", &reflexes.edge),
            ("wall", &reflexes.wall),
            ("poke", &reflexes.poke),
            ("bump", &reflexes.bump),
            ("meet", &reflexes.meet),
            ("window_side", &reflexes.window_side),
            ("leave", &reflexes.leave),
            ("together", &reflexes.together),
        ] {
            check(name, list)?;
        }
        let default_ground = behaviors
            .iter()
            .position(|b| matches!(b.motion, Motion::Stand | Motion::Look))
            .or_else(|| {
                behaviors.iter().position(|b| {
                    !b.motion.is_airborne()
                        && b.motion != Motion::Held
                        && b.motion != Motion::Gesture(Gesture::Flehmen)
                })
            })
            .ok_or(DefError::NoGroundBehavior)?;
        let default_air = behaviors.iter().position(|b| b.motion == Motion::Fall);
        let energies = behaviors.iter().map(crate::temper::Energy::of).collect();
        if !(physics.width.is_finite()
            && physics.width > 0.0
            && physics.height.is_finite()
            && physics.height > 0.0)
        {
            return Err(DefError::Invalid(
                "la taille de l'animal doit être positive et finie".into(),
            ));
        }
        if !(physics.fleece.is_finite() && (0.0..=150.0).contains(&physics.fleece)) {
            return Err(DefError::Invalid(
                "`fleece`, la longueur des mèches, se donne en centimètres, de 0 à 150".into(),
            ));
        }
        // Tous les nombres de la fiche sont des nombres, d'une grandeur
        // possible : une valeur abîmée (un fichier retouché à la main, un
        // zéro de trop, `inf`) donnerait une vitesse qui n'est plus un
        // nombre, puis un animal posé nulle part.
        for (name, value, positive) in [
            ("physics.gravity", physics.gravity, true),
            ("physics.terminal_velocity", physics.terminal_velocity, true),
            ("physics.air_drag", physics.air_drag, true),
            ("physics.walk_accel", physics.walk_accel, true),
            ("physics.ground_friction", physics.ground_friction, true),
            ("physics.bounce", physics.bounce, false),
            ("physics.bounce_min_speed", physics.bounce_min_speed, false),
            (
                "physics.hard_landing_speed",
                physics.hard_landing_speed,
                false,
            ),
            ("physics.max_throw_speed", physics.max_throw_speed, true),
            ("physics.width", physics.width, true),
            ("physics.height", physics.height, true),
            ("physics.stride", physics.stride, true),
        ] {
            possible(name, value, positive)?;
        }
        for b in &behaviors {
            let named = |what: &str| format!("{what} de « {} »", b.name);
            match b.motion {
                Motion::Walk { speed }
                | Motion::Run { speed }
                | Motion::Charge { speed }
                | Motion::Back { speed } => possible(&named("speed"), speed, false)?,
                Motion::Jump { vx, vy } | Motion::Pronk { vx, vy } => {
                    possible(&named("jump"), vx, false)?;
                    possible(&named("jump"), vy, false)?;
                }
                Motion::Leap { reach } => possible(&named("reach"), reach, true)?,
                _ => {}
            }
            if let Duration::Range(a, b) = b.duration {
                possible(&named("duration"), a, true)?;
                possible(&named("duration"), b, true)?;
            }
            possible(&named("tilt"), b.tilt, false)?;
            possible(&named("contagious"), b.contagious, true)?;
            for list in [
                &b.next,
                &b.on_edge,
                &b.on_wall,
                &b.on_fall,
                &b.on_land,
                &b.on_ceiling,
                &b.on_meet,
                &b.on_window_side,
            ] {
                for t in list {
                    possible(&named("weight"), t.weight, true)?;
                }
            }
        }
        Ok(Self {
            meta,
            sex_ratio: SexRatio::default(),
            age: Age::default(),
            appearance,
            physics,
            behaviors,
            reflexes,
            spawns: Vec::new(),
            rules: Rules::Mouflon,
            unit: 1.0,
            sounds: Vec::new(),
            variants: Vec::new(),
            by_name,
            energies,
            default_ground,
            default_air,
        })
    }

    /// L'allure du comportement `index` : ce que le tempérament de l'animal
    /// lui fait choisir plus ou moins volontiers (voir [`crate::Temper`]).
    #[must_use]
    pub fn energy(&self, index: usize) -> crate::temper::Energy {
        self.energies.get(index).copied().unwrap_or_else(|| {
            self.behaviors
                .get(index)
                .map_or(crate::temper::Energy::Neutral, crate::temper::Energy::of)
        })
    }

    /// Donne ses sons à l'animal ; chaque son désigné par un comportement doit exister.
    pub fn set_sounds(&mut self, sounds: Vec<Sound>) -> Result<(), DefError> {
        for b in &self.behaviors {
            if let Some(cue) = b.sounds.iter().find(|c| c.sound >= sounds.len()) {
                return Err(DefError::Invalid(format!(
                    "« {} » : son #{} inconnu",
                    b.name, cue.sound
                )));
            }
        }
        if let Some(sound) = sounds.iter().find(|s| s.clips.is_empty()) {
            return Err(DefError::Invalid(format!(
                "le son « {} » n'a aucun fichier",
                sound.name
            )));
        }
        self.sounds = sounds;
        Ok(())
    }

    /// Images de l'animal, s'il est fait de sprites.
    #[must_use]
    pub fn sprites(&self) -> Option<&SpriteSheet> {
        match &self.appearance {
            Appearance::Sprites(sheet) => Some(sheet),
            Appearance::Rig { .. } | Appearance::Skeleton { .. } => None,
        }
    }

    /// Effets déjà dessinés dans les images.
    #[must_use]
    pub fn drawn(&self) -> Drawn {
        self.sprites().map_or_else(Drawn::default, |s| s.drawn)
    }

    /// Ajoute des points d'apparition (leurs cibles doivent exister).
    pub fn set_spawns(&mut self, spawns: Vec<SpawnPoint>) -> Result<(), DefError> {
        if let Some(bad) = spawns.iter().find(|s| s.next >= self.behaviors.len()) {
            return Err(DefError::UnknownTarget {
                from: "spawn".into(),
                to: format!("#{}", bad.next),
            });
        }
        self.spawns = spawns;
        Ok(())
    }

    #[must_use]
    pub fn behavior_index(&self, name: &str) -> Option<usize> {
        self.by_name.get(name).copied()
    }

    /// Distance parcourue pendant un cycle de marche (points) : celle du
    /// fichier, ou 0,9 fois la hauteur. Au galop, la foulée s'allonge.
    #[must_use]
    pub fn stride(&self) -> f32 {
        if self.physics.stride > 0.0 {
            self.physics.stride
        } else {
            self.physics.height * 0.9
        }
    }

    /// Le même animal, `k` fois plus grand : dessin, corps, vitesses et
    /// pesanteur suivent, pour que tout bouge comme à sa taille d'origine,
    /// en plus grand. Les comportements gardent leur rang.
    #[must_use]
    pub fn scaled(&self, k: f32) -> Self {
        let mut def = self.clone();
        // Pas de taille nulle, négative ou indéfinie ; 1 : rien à changer.
        if !k.is_finite() || k <= 0.0 || k == 1.0 {
            return def;
        }
        match &mut def.appearance {
            Appearance::Rig { height, .. } | Appearance::Skeleton { height, .. } => *height *= k,
            Appearance::Sprites(sheet) => sheet.scale *= k,
        }
        def.unit *= k;
        let p = &mut def.physics;
        for length in [
            &mut p.gravity,
            &mut p.terminal_velocity,
            &mut p.walk_accel,
            &mut p.ground_friction,
            &mut p.bounce_min_speed,
            &mut p.hard_landing_speed,
            &mut p.max_throw_speed,
            &mut p.width,
            &mut p.height,
            &mut p.stride,
        ] {
            *length *= k;
        }
        // Ses autres dessins grandissent avec lui.
        def.variants = self
            .variants
            .iter()
            .map(|(variant, drawn)| (*variant, Arc::new(drawn.scaled(k))))
            .collect();
        for b in &mut def.behaviors {
            match &mut b.motion {
                Motion::Walk { speed }
                | Motion::Run { speed }
                | Motion::Charge { speed }
                | Motion::Back { speed } => {
                    *speed *= k;
                }
                Motion::Jump { vx, vy } | Motion::Pronk { vx, vy } => {
                    *vx *= k;
                    *vy *= k;
                }
                Motion::Script(script) => {
                    let mut s = (**script).clone();
                    s.scale *= k;
                    *script = Arc::new(s);
                }
                _ => {}
            }
            if let Some(clip) = &mut b.clip {
                clip.offset_y *= k;
                clip.offset_y_end *= k;
            }
        }
        def
    }

    /// Donne à l'animal ses autres dessins, par sexe et par âge (voir
    /// [`Variant`]). Chacun devient l'animal entier à la taille de son
    /// dessin : sa hauteur, sa largeur et sa foulée sont les siennes ; ses
    /// vitesses, ses sauts et sa pesanteur suivent la longueur de ses pattes
    /// (le rapport des foulées, ou des hauteurs si le dessin ne donne pas sa
    /// foulée), comme un animal agrandi dans les réglages
    /// ([`PetDef::scaled`]) : il bouge comme à sa taille. Sans `width` ni
    /// `stride`, sa largeur et sa foulée suivent ce même rapport. À appeler
    /// une fois la définition complète (sons, sexe, âge) : chaque dessin en
    /// est une copie. Une liste vide les retire.
    pub fn set_variants(
        &mut self,
        drawings: Vec<(Variant, VariantDrawing)>,
    ) -> Result<(), DefError> {
        let invalid = |message: String| Err(DefError::Invalid(message));
        self.variants = Vec::new();
        let (Appearance::Rig { height: base, .. } | Appearance::Skeleton { height: base, .. }) =
            self.appearance
        else {
            if drawings.is_empty() {
                return Ok(());
            }
            return invalid(
                "un dessin par sexe ou par âge demande un dessin de base vectoriel ou un \
                 squelette, pas une planche d'images"
                    .into(),
            );
        };
        let positive = |v: f32| v.is_finite() && v > 0.0;
        let mut variants: Vec<(Variant, Arc<PetDef>)> = Vec::with_capacity(drawings.len());
        for (variant, drawing) in drawings {
            let name = variant.name();
            if variants.iter().any(|(v, _)| *v == variant) {
                return invalid(format!("le dessin « {name} » est donné deux fois"));
            }
            let height = match &drawing.appearance {
                Appearance::Rig { height, .. } | Appearance::Skeleton { height, .. } => *height,
                Appearance::Sprites(_) => {
                    return invalid(format!(
                        "le dessin « {name} » doit être vectoriel ou un squelette"
                    ));
                }
            };
            if drawing.weeks.is_some() && variant != Variant::Lamb {
                return invalid(format!(
                    "le dessin « {name} » : `weeks` est l'âge de l'agneau dessiné, il ne se donne \
                     qu'au dessin de l'agneau"
                ));
            }
            if !drawing
                .weeks
                .is_none_or(|weeks| weeks.is_finite() && weeks > 0.0 && weeks <= 52.0)
            {
                return invalid(format!(
                    "le dessin « {name} » : `weeks` doit être supérieur à 0 et au plus égal à 52"
                ));
            }
            if !positive(height)
                || !drawing.width.is_none_or(positive)
                || !drawing.stride.is_none_or(positive)
            {
                return invalid(format!(
                    "le dessin « {name} » : hauteur, largeur et foulée doivent être positives et finies"
                ));
            }
            // Le rapport des longueurs de pattes (ou des hauteurs).
            let k = match drawing.stride {
                Some(stride) => stride / self.stride(),
                None => height / base,
            };
            if !positive(k) {
                return invalid(format!("le dessin « {name} » : taille impossible"));
            }
            let mut drawn = self.scaled(k);
            drawn.variants = Vec::new();
            drawn.physics.height = self.physics.height * height / base;
            if let Some(width) = drawing.width {
                drawn.physics.width = width;
            }
            if let Some(stride) = drawing.stride {
                drawn.physics.stride = stride;
            }
            // Ses mèches sont à sa taille : celles de l'agneau, plus
            // courtes, se balancent plus vite.
            drawn.physics.fleece = self.physics.fleece * height / base;
            drawn.appearance = drawing.appearance;
            if variant == Variant::Lamb {
                // L'agneau dessiné a son âge (celui du fichier, s'il déclare
                // un agneau) et sa voix.
                let weeks = drawing.weeks.or(match self.age {
                    Age::Lamb { weeks } => Some(weeks),
                    Age::Adult => None,
                });
                drawn.age = Age::Lamb {
                    weeks: weeks.unwrap_or(LAMB_WEEKS),
                };
                drawn.speak_as_lamb();
            }
            variants.push((variant, Arc::new(drawn)));
        }
        self.variants = variants;
        Ok(())
    }

    /// L'âge, en semaines, de l'agneau de cet animal : celui de son dessin
    /// d'agneau (`weeks` dans `[appearance.lamb]`), sinon deux semaines.
    #[must_use]
    pub fn lamb_weeks(&self) -> f32 {
        match self.variant(Variant::Lamb).map(|lamb| lamb.age) {
            Some(Age::Lamb { weeks }) => weeks,
            _ => LAMB_WEEKS,
        }
    }

    /// Lui donne sa voix d'agneau : les sons qui ont des prises d'agneau
    /// (`lamb = […]`) ne font plus entendre qu'elles.
    pub fn speak_as_lamb(&mut self) {
        for sound in &mut self.sounds {
            if !sound.lamb.is_empty() {
                sound.clips = std::mem::take(&mut sound.lamb);
            }
        }
    }

    /// L'animal tel que le montre ce dessin, s'il l'a.
    #[must_use]
    pub fn variant(&self, variant: Variant) -> Option<&Arc<PetDef>> {
        self.variants
            .iter()
            .find(|(v, _)| *v == variant)
            .map(|(_, def)| def)
    }

    /// Ses autres dessins (voir [`PetDef::set_variants`]).
    pub fn variants(&self) -> impl Iterator<Item = (Variant, &Arc<PetDef>)> {
        self.variants.iter().map(|(v, def)| (*v, def))
    }

    /// Le dessin que porte un animal de ce sexe et de cet âge, s'il en a un
    /// à lui ; `None` : le dessin de base.
    #[must_use]
    pub fn variant_for(&self, sex: Sex, age: Age) -> Option<Variant> {
        let variant = Variant::of(sex, age);
        self.variant(variant).map(|_| variant)
    }

    /// Les dessins que peuvent porter les animaux qui apparaissent avec
    /// cette définition (d'après leur âge et la proportion de mâles), en
    /// plus du dessin de base : ceux qu'il faut préparer pour les montrer.
    pub fn worn_variants(&self) -> impl Iterator<Item = (Variant, &Arc<PetDef>)> {
        let male = self.sex_ratio.male_probability();
        let lamb = matches!(self.age, Age::Lamb { .. });
        self.variants().filter(move |(variant, _)| match variant {
            Variant::Male => !lamb && male > 0.0,
            Variant::Female => !lamb && male < 1.0,
            Variant::Lamb => lamb,
        })
    }

    /// Comportement au sol utilisé quand rien d'autre ne s'applique.
    #[must_use]
    pub fn default_ground(&self) -> usize {
        self.default_ground
    }

    #[must_use]
    pub fn default_air(&self) -> Option<usize> {
        self.default_air
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn meta(name: &str, description: &str, category: Option<&str>) -> PetMeta {
        PetMeta {
            id: name.to_lowercase(),
            name: name.to_owned(),
            author: None,
            description: Some(description.to_owned()),
            version: None,
            license: None,
            category: category.map(str::to_owned),
            region: None,
        }
    }

    #[test]
    fn a_pet_belongs_to_the_family_it_says_or_that_shows() {
        // Ce que dit son pet.toml d'abord, même une famille inconnue.
        assert_eq!(
            meta("Mérinos d'Arles", "", Some("sheep")).category(),
            "sheep"
        );
        assert_eq!(meta("Bulle", "", Some("poissons")).category(), "poissons");
        // Sinon, son nom et sa description, dans plusieurs langues.
        assert_eq!(
            meta("Poete", "Poe, le mouton égaré", None).category(),
            "sheep"
        );
        assert_eq!(meta("eSheep", "", None).category(), "sheep");
        assert_eq!(meta("Mimi", "Un petit chat roux", None).category(), "cats");
        assert_eq!(meta("Pochi", "子犬", None).category(), "dogs");
        assert_eq!(
            meta("Kiki", "Une chouette qui vole", None).category(),
            "birds"
        );
        // Un mot court ne se trouve pas au milieu d'un autre.
        assert_eq!(meta("Boum", "Une catapulte", None).category(), "others");
        assert_eq!(meta("Truc", "", None).category(), "others");
    }

    fn clip(frames: &[u32], repeat: u32, from: usize) -> SpriteClip {
        SpriteClip {
            frames: frames.to_vec(),
            interval_start: 0.2,
            interval_end: 0.2,
            repeat,
            repeat_expr: None,
            dynamic_repeat: false,
            repeat_from: from,
            offset_y: 0.0,
            offset_y_end: 0.0,
            opacity_start: 1.0,
            opacity_end: 1.0,
        }
    }

    #[test]
    fn sprite_clip_repeats_from_index() {
        // Cas vérifié par les tests de desktopPet : [2,3,4,5], repeatfrom 2, T = 6.
        let mut c = clip(&[2, 3, 4, 5], 1, 2);
        c.dynamic_repeat = true;
        assert_eq!(c.total_steps(), 6);
        let seq: Vec<u32> = (0..c.total_steps()).map(|s| c.frame_at(s)).collect();
        assert_eq!(seq, vec![2, 3, 4, 5, 4, 5]);
    }

    #[test]
    fn total_steps_follow_desktoppet() {
        // walk : 2 images, repeat 20 → 42 pas (et non 40).
        assert_eq!(clip(&[2, 3], 20, 0).total_steps(), 42);
        // kill : 3 images, repeat 20, repeatfrom 1 (fixe) → 23 pas.
        assert_eq!(clip(&[3, 96, 96], 20, 1).total_steps(), 23);
        assert_eq!(clip(&[7], 0, 0).total_steps(), 1);
    }
}
