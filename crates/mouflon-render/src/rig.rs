//! Rig vectoriel : un dessin SVG découpé en calques nommés que le moteur
//! anime de façon procédurale.
//!
//! # Contrat pour l'artiste
//!
//! - Le personnage regarde vers la **droite**.
//! - Chaque partie animée est un groupe `<g>` dont l'`id` est l'un des noms
//!   de [`Part`] : `body`, `head`, `ear-near`, `ear-far`, `eyes`, `pupils`,
//!   `tail`, `leg-front-near`, `leg-front-far`, `leg-back-near`, `leg-back-far`,
//!   `jaw`, `tail-mid`, `tail-tip`, et les articulations : `neck` (l'encolure,
//!   qui porte `head`), `lip` (la lèvre supérieure, dans `head`),
//!   `leg-front-near-lower` et les trois autres bas de pattes (chacun dans sa
//!   patte), `sway-1`, `sway-2`, `sway-3` (des mèches de la toison, dans
//!   `body`), `neck-sway-1`, `neck-sway-2`, `neck-sway-3` (des touffes à
//!   racine étroite sous `neck`, dont le bout pend), `bell` (une cloche
//!   pendue au collier, dans `neck`). Toutes sont
//!   facultatives ; une partie peut en contenir
//!   d'autres (`eyes` dans `head`, `pupils` dans `eyes`, `tail-mid` dans
//!   `tail`, `tail-tip` dans `tail-mid`, `head` dans `neck`).
//! - Le pivot de chaque partie (épaule, hanche, cou, base d'oreille…) est un
//!   petit cercle d'`id` `pivot-<partie>`, par exemple `pivot-head`. Le point
//!   d'appui au sol est `pivot-ground`. Les pivots ne sont jamais dessinés.
//! - L'ordre du document est l'ordre de dessin : pattes du fond, corps,
//!   pattes de devant, tête.

use std::collections::HashMap;

use peniko::kurbo::{Affine, BezPath, Point, Rect};
use peniko::{Color, Extend, Fill};

use crate::display::{LineCap, LineJoin, Paint, Stop, StrokeStyle};

/// Parties animables d'un personnage.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Part {
    Body,
    Head,
    EarNear,
    EarFar,
    Eyes,
    Pupils,
    Tail,
    LegFrontNear,
    LegFrontFar,
    LegBackNear,
    LegBackFar,
    /// La mâchoire du bas (facultative), portée par la tête : elle s'ouvre
    /// autour de son articulation, près de la joue.
    Jaw,
    /// Une queue en plusieurs morceaux (facultatifs) : le milieu, dans
    /// `tail`, et le bout, dans `tail-mid`. Chacun suit le précédent avec un
    /// temps de retard.
    TailMid,
    TailTip,
    /// L'encolure (facultative), entre le corps et la tête : elle porte la
    /// tête et tourne autour de sa base, devant le garrot.
    Neck,
    /// La lèvre supérieure (facultative), dans la tête : elle se retrousse
    /// pour le flehmen.
    Lip,
    /// Le bas de chaque patte (facultatif), dans sa patte : le canon et le
    /// sabot, qui plient au carpe (devant) ou au jarret (derrière).
    LegFrontNearLower,
    LegFrontFarLower,
    LegBackNearLower,
    LegBackFarLower,
    /// Des mèches de la toison (facultatives), dans le corps : elles suivent
    /// le corps avec un ressort amorti, traînent quand il court, se
    /// balancent quand il s'ébroue.
    Sway1,
    Sway2,
    Sway3,
    /// Des touffes à racine étroite, cachée sous la laine de l'encolure :
    /// l'attache suit le cou, le bout pend au lieu de tourner avec lui.
    NeckSway1,
    NeckSway2,
    NeckSway3,
    /// Une cloche pendue au collier (facultative), dans `neck` (dans la
    /// partie qui porte le collier s'il n'y a pas d'encolure) : son anse
    /// suit le collier, mais elle pend sous la gravité au lieu de tourner
    /// avec le cou, se balance avec le mouvement et se pose sur l'herbe.
    Bell,
    /// Le battant de la cloche (facultatif), dans `bell` : pendu dedans, il
    /// reste à la verticale quand elle penche, jusqu'à buter contre sa paroi.
    Clapper,
}

impl Part {
    pub const ALL: [Self; 28] = [
        Self::Body,
        Self::Head,
        Self::EarNear,
        Self::EarFar,
        Self::Eyes,
        Self::Pupils,
        Self::Tail,
        Self::LegFrontNear,
        Self::LegFrontFar,
        Self::LegBackNear,
        Self::LegBackFar,
        Self::Jaw,
        Self::TailMid,
        Self::TailTip,
        Self::Neck,
        Self::Lip,
        Self::LegFrontNearLower,
        Self::LegFrontFarLower,
        Self::LegBackNearLower,
        Self::LegBackFarLower,
        Self::Sway1,
        Self::Sway2,
        Self::Sway3,
        Self::NeckSway1,
        Self::NeckSway2,
        Self::NeckSway3,
        Self::Bell,
        Self::Clapper,
    ];

    /// Les quatre pattes, puis leurs bas, dans le même ordre.
    pub const LEGS: [Self; 4] = [
        Self::LegFrontNear,
        Self::LegFrontFar,
        Self::LegBackNear,
        Self::LegBackFar,
    ];
    pub const SWAYS: [Self; 3] = [Self::Sway1, Self::Sway2, Self::Sway3];
    pub const NECK_SWAYS: [Self; 3] = [Self::NeckSway1, Self::NeckSway2, Self::NeckSway3];

    #[must_use]
    pub fn name(self) -> &'static str {
        match self {
            Self::Body => "body",
            Self::Head => "head",
            Self::EarNear => "ear-near",
            Self::EarFar => "ear-far",
            Self::Eyes => "eyes",
            Self::Pupils => "pupils",
            Self::Tail => "tail",
            Self::LegFrontNear => "leg-front-near",
            Self::LegFrontFar => "leg-front-far",
            Self::LegBackNear => "leg-back-near",
            Self::LegBackFar => "leg-back-far",
            Self::Jaw => "jaw",
            Self::TailMid => "tail-mid",
            Self::TailTip => "tail-tip",
            Self::Neck => "neck",
            Self::Lip => "lip",
            Self::LegFrontNearLower => "leg-front-near-lower",
            Self::LegFrontFarLower => "leg-front-far-lower",
            Self::LegBackNearLower => "leg-back-near-lower",
            Self::LegBackFarLower => "leg-back-far-lower",
            Self::Sway1 => "sway-1",
            Self::Sway2 => "sway-2",
            Self::Sway3 => "sway-3",
            Self::NeckSway1 => "neck-sway-1",
            Self::NeckSway2 => "neck-sway-2",
            Self::NeckSway3 => "neck-sway-3",
            Self::Bell => "bell",
            Self::Clapper => "clapper",
        }
    }

    #[must_use]
    pub fn from_id(id: &str) -> Option<Self> {
        let id = id.replace('_', "-").to_ascii_lowercase();
        Self::ALL.into_iter().find(|p| p.name() == id)
    }

    #[must_use]
    pub fn index(self) -> usize {
        self as usize
    }

    #[must_use]
    pub fn is_leg(self) -> bool {
        matches!(
            self,
            Self::LegFrontNear | Self::LegFrontFar | Self::LegBackNear | Self::LegBackFar
        )
    }

    /// La toison : le corps, l'encolure et les mèches (éclairés ensemble),
    /// et la cloche que porte l'encolure : dessinée dans `neck` avant d'en
    /// avoir sa partie, elle reste éclairée avec elle, dans la même
    /// silhouette (au repos, le dessin ne change pas).
    #[must_use]
    pub fn is_fleece(self) -> bool {
        matches!(
            self,
            Self::Body
                | Self::Neck
                | Self::Sway1
                | Self::Sway2
                | Self::Sway3
                | Self::NeckSway1
                | Self::NeckSway2
                | Self::NeckSway3
                | Self::Bell
                | Self::Clapper
        )
    }

    /// Le bas d'une patte.
    #[must_use]
    pub fn lower(self) -> Option<Self> {
        Some(match self {
            Self::LegFrontNear => Self::LegFrontNearLower,
            Self::LegFrontFar => Self::LegFrontFarLower,
            Self::LegBackNear => Self::LegBackNearLower,
            Self::LegBackFar => Self::LegBackFarLower,
            _ => return None,
        })
    }

    #[must_use]
    pub fn is_lower_leg(self) -> bool {
        matches!(
            self,
            Self::LegFrontNearLower
                | Self::LegFrontFarLower
                | Self::LegBackNearLower
                | Self::LegBackFarLower
        )
    }

    /// La partie qui la contient d'après le contrat (et que le moteur
    /// suppose) : la tête dans l'encolure s'il y en a une, les yeux, les
    /// oreilles, la mâchoire et la lèvre dans la tête, chaque bas de patte
    /// dans sa patte, les mèches dans le corps, la cloche dans l'encolure
    /// (dans le corps s'il n'y en a pas)… Le reste est à la racine.
    #[must_use]
    pub fn contract_parent(self, has_neck: bool) -> Option<Self> {
        Some(match self {
            Self::Head if has_neck => Self::Neck,
            Self::EarNear | Self::EarFar | Self::Eyes | Self::Jaw | Self::Lip => Self::Head,
            Self::Pupils => Self::Eyes,
            Self::TailMid => Self::Tail,
            Self::TailTip => Self::TailMid,
            Self::LegFrontNearLower => Self::LegFrontNear,
            Self::LegFrontFarLower => Self::LegFrontFar,
            Self::LegBackNearLower => Self::LegBackNear,
            Self::LegBackFarLower => Self::LegBackFar,
            Self::Sway1 | Self::Sway2 | Self::Sway3 => Self::Body,
            Self::NeckSway1 | Self::NeckSway2 | Self::NeckSway3 | Self::Bell => {
                if has_neck {
                    Self::Neck
                } else {
                    Self::Body
                }
            }
            Self::Clapper => Self::Bell,
            _ => return None,
        })
    }
}

/// Élément du dessin, dans l'ordre du document.
#[derive(Clone, Debug)]
pub enum Item {
    BeginPart(Part),
    EndPart,
    BeginLayer {
        alpha: f32,
    },
    EndLayer,
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
}

#[derive(Clone, Debug)]
pub struct Rig {
    /// Taille du document SVG (unités utilisateur).
    pub size: (f64, f64),
    /// Boîte englobante du dessin (sans les pivots).
    pub bounds: Rect,
    /// Point d'appui au sol, dans le repère du document.
    pub ground: Point,
    /// Le pivot de chaque partie (donné, ou deviné d'après sa forme) et sa
    /// boîte, rangés par [`Part::index`] : le moteur les lit à chaque image.
    pivots: [Point; Part::ALL.len()],
    part_bounds: [Option<Rect>; Part::ALL.len()],
    /// Ce que dessine chaque partie elle-même, sans les parties qu'elle porte.
    own_bounds: [Option<Rect>; Part::ALL.len()],
    /// La partie qui contient chaque partie dans le document (`None` : à
    /// la racine), pour les parties du dessin.
    parents: [Option<Part>; Part::ALL.len()],
    /// Les parties rangées autrement que le dit le contrat (voir
    /// [`Part::contract_parent`]) : le moteur les ramène à leur vrai parent.
    pub(crate) unusual: Vec<Part>,
    /// Les articulations, préparées une fois : l'encolure, les pattes en
    /// deux segments.
    pub(crate) joints: crate::animate::RigJoints,
    /// L'étendue de la toison (voir [`Rig::body_extent`]).
    body_extent: Rect,
    /// Ce qui touche l'herbe quand il broute (`pivot-muzzle`), s'il est donné.
    pub muzzle: Option<Point>,
    /// Le bord du haut de chaque nappe d'encolure (`pivot-neck-sway-N-top`),
    /// s'il est donné : la ligne d'où elle se balance.
    pub(crate) sheet_tops: [Option<Point>; 3],
    /// Silhouette du corps, pour l'éclairage dynamique.
    pub body_silhouette: Option<(BezPath, Affine)>,
    /// Pour la lumière du dessin vectoriel : au dernier élément de chaque
    /// suite de tracés de la toison, les surfaces de cette suite (repère du
    /// document). Chaque suite est éclairée avec sa partie, comme un morceau
    /// cuit.
    pub(crate) fleece_runs: Vec<Option<BezPath>>,
    pub items: Vec<Item>,
}

#[derive(Debug, thiserror::Error)]
pub enum RigError {
    #[error("SVG invalide : {0}")]
    Svg(#[from] usvg::Error),
    #[error("le dessin est vide")]
    Empty,
    #[error("le dessin est démesuré ou mal placé ({0})")]
    Unsound(String),
}

/// Côté le plus grand d'un dessin, en unités du SVG : un mouton en mesure
/// une centaine. Au-delà (un nombre abîmé dans le fichier, une forme
/// oubliée au loin), sa toile et ses images seraient démesurées.
const MAX_SIDE: f64 = 4096.0;

/// `rect` est-il fait de nombres, et d'une taille raisonnable ?
fn sound(rect: Rect) -> bool {
    [rect.x0, rect.y0, rect.x1, rect.y1]
        .iter()
        .all(|v| v.is_finite())
        && rect.width() <= MAX_SIDE
        && rect.height() <= MAX_SIDE
}

impl Rig {
    /// Analyse un SVG suivant le contrat décrit dans ce module.
    pub fn parse(svg: &[u8]) -> Result<Self, RigError> {
        let tree = usvg::Tree::from_data(svg, &usvg::Options::default())?;
        let mut builder = Builder::default();
        builder.walk(tree.root(), None);
        let bounds = builder.bounds.ok_or(RigError::Empty)?;
        // Un dessin abîmé (un nombre énorme, ou qui n'en est pas un) est
        // refusé ici : plus loin, il donnerait une toile démesurée, ou des
        // morceaux posés nulle part.
        if !sound(bounds) {
            return Err(RigError::Unsound(format!(
                "il s'étend de {:.0}, {:.0} à {:.0}, {:.0}",
                bounds.x0, bounds.y0, bounds.x1, bounds.y1
            )));
        }
        // Un repère ne s'éloigne pas du dessin de plus que sa taille.
        let reach = bounds.inflate(bounds.width().max(1.0), bounds.height().max(1.0));
        if let Some((name, at)) = builder
            .named_pivots
            .iter()
            .find(|(_, at)| !(at.x.is_finite() && at.y.is_finite() && reach.contains(**at)))
        {
            return Err(RigError::Unsound(format!(
                "le repère « pivot-{name} » est à {:.0}, {:.0}",
                at.x, at.y
            )));
        }
        let ground = builder
            .named_pivots
            .get("ground")
            .copied()
            .unwrap_or_else(|| Point::new(bounds.center().x, bounds.max_y()));
        let size = tree.size();
        let muzzle = builder.named_pivots.get("muzzle").copied();
        let sheet_tops = Part::NECK_SWAYS.map(|part| {
            builder
                .named_pivots
                .get(&format!("{}-top", part.name()))
                .copied()
        });
        let mut part_bounds = [None; Part::ALL.len()];
        let mut own_bounds = [None; Part::ALL.len()];
        let mut parents = [None; Part::ALL.len()];
        for part in Part::ALL {
            part_bounds[part.index()] = builder.part_bounds.get(&part).copied();
            own_bounds[part.index()] = builder.own_bounds.get(&part).copied();
            parents[part.index()] = builder.parents.get(&part).copied().flatten();
        }
        let mut rig = Self {
            size: (f64::from(size.width()), f64::from(size.height())),
            bounds,
            ground,
            pivots: [Point::ZERO; Part::ALL.len()],
            part_bounds,
            own_bounds,
            parents,
            unusual: Vec::new(),
            joints: crate::animate::RigJoints::default(),
            body_extent: bounds,
            muzzle,
            sheet_tops,
            body_silhouette: builder.body_silhouette,
            fleece_runs: fleece_runs(&builder.items),
            items: builder.items,
        };
        for part in Part::ALL {
            rig.pivots[part.index()] = builder
                .named_pivots
                .get(part.name())
                .copied()
                .unwrap_or_else(|| rig.guess_pivot(part));
        }
        // La toison : le corps, ses mèches et ce que l'encolure dessine
        // elle-même (sans la tête), où qu'elle soit posée dans le document.
        let mut extent = rig.part_bounds(Part::Body);
        if rig.has_part(Part::Body) {
            for part in Part::SWAYS.into_iter().chain(Part::NECK_SWAYS) {
                if let Some(r) = rig.part_bounds[part.index()] {
                    extent = extent.union(r);
                }
            }
            if let Some(r) = builder.own_bounds.get(&Part::Neck) {
                extent = extent.union(*r);
            }
            // La cloche, que l'encolure portait dans son dessin, compte dans
            // la longueur de la toison (son milieu, autour duquel il penche),
            // pas dans sa hauteur : elle se pose seule sur l'herbe, sans
            // retenir le corps qui se couche.
            if let Some(r) = rig.part_bounds[Part::Bell.index()] {
                extent = Rect::new(
                    extent.x0.min(r.x0),
                    extent.y0,
                    extent.x1.max(r.x1),
                    extent.y1,
                );
            }
        }
        rig.body_extent = extent;
        let neck = rig.has_part(Part::Neck);
        rig.unusual = Part::ALL
            .into_iter()
            .filter(|&p| rig.has_part(p) && rig.parent(p) != p.contract_parent(neck))
            .collect();
        rig.joints = crate::animate::RigJoints::of(&rig);
        // Couché, de combien il se pose plus haut que sur son ventre : mesuré
        // sur ses poses, puis vérifié une fois posé là (l'encolure descend
        // alors d'un rien de plus pour toucher l'herbe).
        for _ in 0..3 {
            let more = crate::animate::lying_lift(&rig);
            let lift = (rig.joints.lying_lift + more).max(0.0);
            if (lift - rig.joints.lying_lift).abs() < 1e-4 {
                break;
            }
            rig.joints.lying_lift = lift;
        }
        Ok(rig)
    }

    #[must_use]
    pub fn has_part(&self, part: Part) -> bool {
        self.part_bounds[part.index()].is_some()
    }

    /// La partie qui contient `part` dans le document (`None` : à la racine).
    #[must_use]
    pub fn parent(&self, part: Part) -> Option<Part> {
        self.parents[part.index()]
    }

    /// Les parties sont rangées comme le dit le contrat (voir
    /// [`Part::contract_parent`]).
    #[must_use]
    pub fn follows_contract(&self) -> bool {
        self.unusual.is_empty()
    }

    /// Pivot d'une partie : celui fourni par l'artiste, sinon une valeur
    /// raisonnable déduite de la forme (haut des pattes, base de la tête…).
    #[must_use]
    pub fn pivot(&self, part: Part) -> Point {
        self.pivots[part.index()]
    }

    fn guess_pivot(&self, part: Part) -> Point {
        let b = self.part_bounds(part);
        match part {
            p if p.is_leg() => Point::new(b.center().x, b.min_y() + b.height() * 0.15),
            Part::Head => Point::new(b.min_x() + b.width() * 0.3, b.max_y() - b.height() * 0.2),
            Part::EarNear | Part::EarFar => Point::new(b.center().x, b.max_y()),
            Part::Tail => Point::new(b.max_x(), b.center().y),
            // Un morceau de queue pend de son attache, en haut.
            Part::TailMid | Part::TailTip => Point::new(b.center().x, b.min_y()),
            Part::Body => Point::new(b.center().x, b.max_y()),
            // La charnière : l'arrière de la mâchoire, vers la joue.
            Part::Jaw => Point::new(b.min_x(), b.min_y() + b.height() * 0.3),
            // Le carpe ou le jarret : le haut du bas de la patte.
            p if p.is_lower_leg() => Point::new(b.center().x, b.min_y() + b.height() * 0.08),
            // La base de l'encolure, devant le garrot.
            Part::Neck => Point::new(b.min_x() + b.width() * 0.15, b.max_y() - b.height() * 0.3),
            // L'arrière de la lèvre, sous le naseau.
            Part::Lip => Point::new(b.min_x(), b.center().y),
            // Une mèche pend de son attache ; une cloche, de son anse.
            Part::Sway1
            | Part::Sway2
            | Part::Sway3
            | Part::NeckSway1
            | Part::NeckSway2
            | Part::NeckSway3
            | Part::Bell
            | Part::Clapper => Point::new(b.center().x, b.min_y()),
            _ => b.center(),
        }
    }

    /// L'étendue de la toison : la boîte du corps, de ses mèches (`sway-*`)
    /// et de l'encolure (sans la tête qu'elle porte ; avec la largeur de la
    /// cloche, pas sa hauteur). Le moteur s'en sert
    /// pour coucher l'animal (jusqu'où descend le corps) et l'incliner :
    /// découper la toison en morceaux ne la change pas, ni l'endroit où
    /// l'encolure est posée dans le document.
    #[must_use]
    pub fn body_extent(&self) -> Rect {
        self.body_extent
    }

    /// Boîte d'une partie (ou du dessin entier si elle est absente).
    #[must_use]
    pub fn part_bounds(&self, part: Part) -> Rect {
        self.part_bounds[part.index()].unwrap_or(self.bounds)
    }

    /// Ce que dessine une partie elle-même, sans les parties qu'elle porte
    /// (`None` : elle ne dessine rien, ou elle est absente).
    #[must_use]
    pub fn own_bounds(&self, part: Part) -> Option<Rect> {
        self.own_bounds[part.index()]
    }
}

/// Les surfaces de chaque suite de tracés de la toison (voir
/// [`Rig::fleece_runs`]) : les suites sont celles de la cuisson, coupées à
/// chaque début et fin de partie.
fn fleece_runs(items: &[Item]) -> Vec<Option<BezPath>> {
    let mut out = vec![None; items.len()];
    let mut parts: Vec<Part> = Vec::new();
    let mut run: Option<(usize, BezPath)> = None;
    let flush = |run: &mut Option<(usize, BezPath)>, out: &mut [Option<BezPath>]| {
        if let Some((last, fills)) = run.take()
            && !fills.elements().is_empty()
        {
            out[last] = Some(fills);
        }
    };
    for (index, item) in items.iter().enumerate() {
        match item {
            Item::BeginPart(part) => {
                flush(&mut run, &mut out);
                parts.push(*part);
            }
            Item::EndPart => {
                flush(&mut run, &mut out);
                parts.pop();
            }
            Item::BeginLayer { .. } | Item::EndLayer => flush(&mut run, &mut out),
            Item::Fill {
                path, transform, ..
            }
            | Item::Stroke {
                path, transform, ..
            } => {
                if !parts.last().is_some_and(|p| p.is_fleece()) {
                    continue;
                }
                let (last, fills) = run.get_or_insert_with(|| (index, BezPath::new()));
                *last = index;
                if matches!(item, Item::Fill { .. }) {
                    for el in path.elements() {
                        fills.push(*transform * *el);
                    }
                }
            }
        }
    }
    flush(&mut run, &mut out);
    out
}

#[derive(Default)]
struct Builder {
    items: Vec<Item>,
    bounds: Option<Rect>,
    named_pivots: HashMap<String, Point>,
    part_bounds: HashMap<Part, Rect>,
    open_parts: Vec<Part>,
    parents: HashMap<Part, Option<Part>>,
    /// Ce que dessine chaque partie elle-même (sans les parties qu'elle porte).
    own_bounds: HashMap<Part, Rect>,
    body_silhouette: Option<(BezPath, Affine)>,
}

impl Builder {
    fn walk(&mut self, group: &usvg::Group, part_hint: Option<Part>) {
        for node in group.children() {
            match node {
                usvg::Node::Group(g) => {
                    if g.clip_path().is_some() || g.mask().is_some() || !g.filters().is_empty() {
                        log::warn!(
                            "groupe « {} » : masques, découpes et filtres SVG ne sont pas encore gérés",
                            g.id()
                        );
                    }
                    let part = Part::from_id(g.id());
                    if let Some(part) = part {
                        self.parents
                            .entry(part)
                            .or_insert(self.open_parts.last().copied());
                        self.items.push(Item::BeginPart(part));
                        self.open_parts.push(part);
                    }
                    let alpha = g.opacity().get();
                    let layered = alpha < 0.999;
                    if layered {
                        self.items.push(Item::BeginLayer { alpha });
                    }
                    self.walk(g, part.or(part_hint));
                    if layered {
                        self.items.push(Item::EndLayer);
                    }
                    if part.is_some() {
                        self.items.push(Item::EndPart);
                        self.open_parts.pop();
                    }
                }
                usvg::Node::Path(path) => self.path(path),
                usvg::Node::Image(_) => {
                    log::warn!("les images bitmap dans un rig SVG sont ignorées")
                }
                usvg::Node::Text(_) => {
                    log::warn!("le texte d'un rig SVG doit être converti en tracés")
                }
            }
        }
    }

    fn path(&mut self, path: &usvg::Path) {
        let id = path.id();
        if let Some(name) = id.strip_prefix("pivot-") {
            let b = path.abs_bounding_box();
            let c = Point::new(
                f64::from(b.x() + b.width() / 2.0),
                f64::from(b.y() + b.height() / 2.0),
            );
            self.named_pivots
                .insert(name.replace('_', "-").to_ascii_lowercase(), c);
            return;
        }
        if !path.is_visible() {
            return;
        }
        let bez = to_bez(path.data());
        let transform = to_affine(path.abs_transform());
        let b = path.abs_stroke_bounding_box();
        let rect = Rect::new(
            f64::from(b.left()),
            f64::from(b.top()),
            f64::from(b.right()),
            f64::from(b.bottom()),
        );
        self.bounds = Some(self.bounds.map_or(rect, |r| r.union(rect)));
        for part in &self.open_parts {
            self.part_bounds
                .entry(*part)
                .and_modify(|r| *r = r.union(rect))
                .or_insert(rect);
        }
        if let Some(part) = self.open_parts.last() {
            self.own_bounds
                .entry(*part)
                .and_modify(|r| *r = r.union(rect))
                .or_insert(rect);
        }

        // L'encolure et les mèches sont de la toison, comme le corps : même
        // silhouette, même lumière.
        let in_body = self.open_parts.last().is_some_and(|p| p.is_fleece());
        if let Some(fill) = path.fill() {
            if in_body {
                let (sil, t) = self
                    .body_silhouette
                    .get_or_insert_with(|| (BezPath::new(), transform));
                // Les tracés du corps sont regroupés dans le repère du premier.
                let local = t.inverse() * transform;
                for el in bez.elements() {
                    sil.push(local * *el);
                }
            }
            if let Some((paint, brush)) = to_paint(fill.paint(), fill.opacity().get()) {
                let rule = match fill.rule() {
                    usvg::FillRule::NonZero => Fill::NonZero,
                    usvg::FillRule::EvenOdd => Fill::EvenOdd,
                };
                self.items.push(Item::Fill {
                    path: bez.clone(),
                    transform,
                    paint,
                    brush_transform: brush,
                    rule,
                });
            }
        }
        if let Some(stroke) = path.stroke()
            && let Some((paint, brush)) = to_paint(stroke.paint(), stroke.opacity().get())
        {
            let style = StrokeStyle {
                width: f64::from(stroke.width().get()),
                cap: match stroke.linecap() {
                    usvg::LineCap::Butt => LineCap::Butt,
                    usvg::LineCap::Round => LineCap::Round,
                    usvg::LineCap::Square => LineCap::Square,
                },
                join: match stroke.linejoin() {
                    usvg::LineJoin::Round => LineJoin::Round,
                    usvg::LineJoin::Bevel => LineJoin::Bevel,
                    usvg::LineJoin::Miter | usvg::LineJoin::MiterClip => LineJoin::Miter,
                },
                miter_limit: f64::from(stroke.to_tiny_skia().miter_limit),
            };
            self.items.push(Item::Stroke {
                path: bez,
                transform,
                paint,
                brush_transform: brush,
                style,
            });
        }
    }
}

fn to_affine(t: usvg::Transform) -> Affine {
    Affine::new([
        f64::from(t.sx),
        f64::from(t.ky),
        f64::from(t.kx),
        f64::from(t.sy),
        f64::from(t.tx),
        f64::from(t.ty),
    ])
}

fn to_bez(path: &usvg::tiny_skia_path::Path) -> BezPath {
    use usvg::tiny_skia_path::PathSegment;
    let p = |pt: usvg::tiny_skia_path::Point| Point::new(f64::from(pt.x), f64::from(pt.y));
    let mut bez = BezPath::new();
    for seg in path.segments() {
        match seg {
            PathSegment::MoveTo(a) => bez.move_to(p(a)),
            PathSegment::LineTo(a) => bez.line_to(p(a)),
            PathSegment::QuadTo(a, b) => bez.quad_to(p(a), p(b)),
            PathSegment::CubicTo(a, b, c) => bez.curve_to(p(a), p(b), p(c)),
            PathSegment::Close => bez.close_path(),
        }
    }
    bez
}

fn to_color(c: usvg::Color, alpha: f32) -> Color {
    Color::new([
        f32::from(c.red) / 255.0,
        f32::from(c.green) / 255.0,
        f32::from(c.blue) / 255.0,
        alpha,
    ])
}

fn to_stops(stops: &[usvg::Stop], alpha: f32) -> Vec<Stop> {
    stops
        .iter()
        .map(|s| Stop {
            offset: s.offset().get(),
            color: to_color(s.color(), s.opacity().get() * alpha),
        })
        .collect()
}

fn to_extend(spread: usvg::SpreadMethod) -> Extend {
    match spread {
        usvg::SpreadMethod::Pad => Extend::Pad,
        usvg::SpreadMethod::Reflect => Extend::Reflect,
        usvg::SpreadMethod::Repeat => Extend::Repeat,
    }
}

fn to_paint(paint: &usvg::Paint, alpha: f32) -> Option<(Paint, Option<Affine>)> {
    match paint {
        usvg::Paint::Color(c) => Some((Paint::Solid(to_color(*c, alpha)), None)),
        usvg::Paint::LinearGradient(g) => Some((
            Paint::Linear {
                start: Point::new(f64::from(g.x1()), f64::from(g.y1())),
                end: Point::new(f64::from(g.x2()), f64::from(g.y2())),
                stops: to_stops(g.stops(), alpha),
                extend: to_extend(g.spread_method()),
            },
            Some(to_affine(g.transform())),
        )),
        usvg::Paint::RadialGradient(g) => Some((
            Paint::Radial {
                start: Point::new(f64::from(g.fx()), f64::from(g.fy())),
                start_radius: g.fr().get(),
                end: Point::new(f64::from(g.cx()), f64::from(g.cy())),
                end_radius: g.r().get(),
                stops: to_stops(g.stops(), alpha),
                extend: to_extend(g.spread_method()),
            },
            Some(to_affine(g.transform())),
        )),
        usvg::Paint::Pattern(_) => {
            log::warn!("les motifs SVG (pattern) ne sont pas gérés");
            None
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const SVG: &str = r##"<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 100 100" width="100" height="100">
        <defs><linearGradient id="g" x1="0" y1="0" x2="0" y2="1"><stop offset="0" stop-color="#fff"/><stop offset="1" stop-color="#000"/></linearGradient></defs>
        <g id="leg-back-far"><rect x="20" y="60" width="8" height="36" rx="4" fill="#333"/><circle id="pivot-leg-back-far" cx="24" cy="62" r="1"/></g>
        <g id="body"><ellipse cx="50" cy="55" rx="35" ry="22" fill="url(#g)"/></g>
        <g id="head"><circle cx="80" cy="35" r="15" fill="#888"/>
            <g id="eyes"><circle cx="85" cy="32" r="3" fill="#fff"/><g id="pupils"><circle cx="86" cy="32" r="1.5" fill="#000"/></g></g>
        </g>
        <circle id="pivot-ground" cx="50" cy="96" r="1"/>
    </svg>"##;

    #[test]
    fn parses_parts_pivots_and_ground() {
        let rig = Rig::parse(SVG.as_bytes()).unwrap();
        assert_eq!(rig.ground, Point::new(50.0, 96.0));
        assert_eq!(rig.pivot(Part::LegBackFar), Point::new(24.0, 62.0));
        assert!(rig.has_part(Part::Head) && rig.has_part(Part::Pupils));
        assert!(!rig.has_part(Part::Tail));
        assert!(rig.body_silhouette.is_some());
        let begins = rig
            .items
            .iter()
            .filter(|i| matches!(i, Item::BeginPart(_)))
            .count();
        let ends = rig
            .items
            .iter()
            .filter(|i| matches!(i, Item::EndPart))
            .count();
        assert_eq!(begins, 5);
        assert_eq!(begins, ends);
        // Les pivots ne font pas partie du dessin.
        assert!(rig.bounds.max_y() <= 96.5);
    }

    #[test]
    fn gradients_become_linear_paints() {
        let rig = Rig::parse(SVG.as_bytes()).unwrap();
        let has_linear = rig.items.iter().any(|i| {
            matches!(
                i,
                Item::Fill {
                    paint: Paint::Linear { .. },
                    ..
                }
            )
        });
        assert!(has_linear);
    }
}
