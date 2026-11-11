//! Le « monde » vu par les animaux : écrans, sols et rebords de fenêtres.
//!
//! Repère : points logiques, origine en haut à gauche de l'écran principal,
//! y vers le bas. La couche système convertit depuis le repère de l'OS.

use std::collections::HashMap;
use std::hash::{BuildHasherDefault, Hasher};

use crate::layout::Strips;
use crate::math::{Rect, Rng, Vec2};

/// Hachage des identifiants (des entiers : surfaces, animaux, images) : un
/// mélange par multiplication, bien plus rapide que celui par défaut, sans
/// rien à craindre ici d'identifiants choisis pour le mettre en défaut.
#[derive(Clone, Copy, Debug, Default)]
pub struct IdHasher(u64);

impl Hasher for IdHasher {
    fn finish(&self) -> u64 {
        self.0
    }

    fn write(&mut self, bytes: &[u8]) {
        for &b in bytes {
            self.0 = (self.0.rotate_left(5) ^ u64::from(b)).wrapping_mul(0x51_7c_c1_b7_27_22_0a_95);
        }
    }

    fn write_u64(&mut self, n: u64) {
        self.0 = (self.0 ^ n)
            .wrapping_mul(0x9e37_79b9_7f4a_7c15)
            .rotate_left(29);
    }

    fn write_u32(&mut self, n: u32) {
        self.write_u64(u64::from(n));
    }
}

/// Une table rangée par identifiant de surface (voir [`IdHasher`]).
pub type IdMap<V> = HashMap<SurfaceId, V, BuildHasherDefault<IdHasher>>;

/// Identifiant stable d'une surface, conservé tant que la fenêtre existe :
/// c'est ce qui permet à un animal de « voyager » sur une fenêtre qu'on déplace.
pub type SurfaceId = u64;

const FLOOR_ID_BIT: u64 = 1 << 63;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum SurfaceKind {
    /// Bas de la zone utile d'un écran (au-dessus du Dock ou de la barre des tâches).
    Floor,
    /// Bord supérieur visible d'une fenêtre.
    Window,
}

/// Segment horizontal sur lequel un animal peut se tenir.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Surface {
    pub id: SurfaceId,
    pub kind: SurfaceKind,
    pub x0: f32,
    pub x1: f32,
    pub y: f32,
}

impl Surface {
    #[must_use]
    pub fn width(&self) -> f32 {
        self.x1 - self.x0
    }

    #[must_use]
    pub fn contains_x(&self, x: f32, margin: f32) -> bool {
        x >= self.x0 - margin && x <= self.x1 + margin
    }
}

/// Partie visible du côté vertical d'une fenêtre : elle barre le chemin d'un
/// animal qui marche vers la fenêtre (`windowSide` de desktopPet).
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Wall {
    /// Fenêtre dont c'est le côté.
    pub window: u64,
    pub x: f32,
    /// Haut et bas de la partie visible.
    pub y0: f32,
    pub y1: f32,
    /// Côté vers lequel il regarde : -1 pour le côté gauche de la fenêtre
    /// (il arrête un animal qui va vers la droite), 1 pour le côté droit.
    pub facing: f32,
}

/// Un côté plus court que ceci (le reste est caché) ne barre rien.
const MIN_WALL: f32 = 4.0;
/// Un point à moins de ceci sous un sol y est encore posé (points).
const GROUNDED: f32 = 1.5;

/// Un écran : son cadre complet et sa zone utile (sans barre des menus ni Dock).
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Screen {
    pub frame: Rect,
    pub usable: Rect,
    pub scale: f32,
}

impl Screen {
    /// Là où un animal peut se trouver sur cet écran : de côté, sa zone
    /// utile (pas sous un Dock ou une barre des tâches rangés sur le côté) ;
    /// de haut en bas, de son bord supérieur (lancé, un animal passe
    /// derrière la barre des menus) à son sol.
    #[must_use]
    pub fn room(&self) -> Rect {
        Rect::new(
            self.usable.x,
            self.frame.y,
            self.usable.w,
            self.usable.bottom() - self.frame.y,
        )
    }
}

/// Fenêtre d'une autre application, telle que fournie par la couche système,
/// dans l'ordre d'empilement (la première est au premier plan).
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct WindowRect {
    pub id: u64,
    pub frame: Rect,
}

/// Réglages du calcul des rebords de fenêtres.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct LedgeOptions {
    /// Largeur minimale d'un morceau de rebord visible pour qu'on y marche.
    pub min_span: f32,
    /// Hauteur libre exigée entre le haut de la zone utile et le rebord :
    /// une fenêtre plein écran n'offre pas de place pour se tenir dessus.
    pub headroom: f32,
}

impl Default for LedgeOptions {
    fn default() -> Self {
        Self {
            min_span: 28.0,
            headroom: 24.0,
        }
    }
}

/// Largeur d'une colonne de l'index des surfaces (points).
const COLUMN: f32 = 64.0;
/// Colonnes de l'index, au plus (au-delà, les surfaces lointaines se
/// rangent dans les colonnes du bord).
const MAX_COLUMNS: usize = 4096;

/// Les surfaces rangées par colonnes et par identifiant : ce que cherche un
/// animal (où se poser, sur quoi il est) ne parcourt que les surfaces de sa
/// colonne, même avec un millier de dos d'animaux parmi elles.
#[derive(Clone, Debug, Default)]
struct SurfaceIndex {
    /// Bord gauche de la première colonne.
    left: f32,
    /// Les surfaces de la colonne `c` : `items[starts[c]..starts[c + 1]]`
    /// (leurs rangs dans `World::surfaces`, croissants).
    starts: Vec<u32>,
    items: Vec<u32>,
    by_id: IdMap<u32>,
    floors: Vec<u32>,
    /// Pour remplir les colonnes (gardé d'une fois à l'autre).
    fill: Vec<u32>,
}

impl SurfaceIndex {
    /// Refait l'index de ces surfaces, en gardant la mémoire déjà prise.
    fn rebuild(&mut self, surfaces: &[Surface]) {
        let index = self;
        index.by_id.clear();
        index.floors.clear();
        let (mut lo, mut hi) = (f32::INFINITY, f32::NEG_INFINITY);
        for s in surfaces
            .iter()
            .filter(|s| s.x0.is_finite() && s.x1.is_finite())
        {
            lo = lo.min(s.x0);
            hi = hi.max(s.x1);
        }
        let count = if lo.is_finite() && hi >= lo {
            (((hi - lo) / COLUMN) as usize + 1).min(MAX_COLUMNS)
        } else {
            1
        };
        index.left = if lo.is_finite() { lo } else { 0.0 };
        let mut counts = std::mem::take(&mut index.starts);
        counts.clear();
        counts.resize(count + 1, 0);
        for s in surfaces {
            let (c0, c1) = index.columns(s.x0, s.x1, count);
            for c in c0..=c1 {
                counts[c + 1] += 1;
            }
        }
        for c in 0..count {
            counts[c + 1] += counts[c];
        }
        index.items.clear();
        index.items.resize(counts[count] as usize, 0);
        let mut fill = std::mem::take(&mut index.fill);
        fill.clear();
        fill.extend_from_slice(&counts);
        for (i, s) in surfaces.iter().enumerate() {
            let (c0, c1) = index.columns(s.x0, s.x1, count);
            for next in &mut fill[c0..=c1] {
                index.items[*next as usize] = i as u32;
                *next += 1;
            }
            index.by_id.entry(s.id).or_insert(i as u32);
            if s.kind == SurfaceKind::Floor {
                index.floors.push(i as u32);
            }
        }
        index.fill = fill;
        index.starts = counts;
    }

    fn column_count(&self) -> usize {
        self.starts.len().saturating_sub(1)
    }

    /// Colonnes couvertes de `x0` à `x1` (bornées à celles de l'index).
    fn columns(&self, x0: f32, x1: f32, count: usize) -> (usize, usize) {
        let col = |x: f32| {
            let c = ((x - self.left) / COLUMN).floor();
            if c.is_nan() || c < 0.0 {
                0
            } else {
                (c as usize).min(count.saturating_sub(1))
            }
        };
        let (a, b) = (col(x0.min(x1)), col(x0.max(x1)));
        (a, b)
    }

    /// Rangs des surfaces qui peuvent contenir les abscisses de `x0` à `x1`
    /// (avec des doublons quand une surface couvre plusieurs colonnes).
    fn near(&self, x0: f32, x1: f32) -> impl Iterator<Item = usize> + '_ {
        self.near_ranks(x0, x1).iter().map(|&i| i as usize)
    }

    fn near_ranks(&self, x0: f32, x1: f32) -> &[u32] {
        let (c0, c1) = self.columns(x0, x1, self.column_count());
        let (from, to) = (
            self.starts.get(c0).copied().unwrap_or(0) as usize,
            self.starts.get(c1 + 1).copied().unwrap_or(0) as usize,
        );
        &self.items[from..to.max(from)]
    }
}

/// Les rangs des surfaces qui peuvent couvrir des abscisses (voir
/// [`World::near_surfaces`]).
#[derive(Clone, Debug)]
pub(crate) enum NearSurfaces<'a> {
    Indexed(std::slice::Iter<'a, u32>),
    All(std::ops::Range<usize>),
}

impl Iterator for NearSurfaces<'_> {
    type Item = usize;

    fn next(&mut self) -> Option<usize> {
        match self {
            Self::Indexed(ranks) => ranks.next().map(|&i| i as usize),
            Self::All(ranks) => ranks.next(),
        }
    }
}

/// La surface la plus haute (plus petit `y`) parmi celles que garde
/// `keep`, la première de la liste en cas d'égalité.
fn highest(
    surfaces: &[Surface],
    candidates: impl Iterator<Item = usize>,
    keep: impl Fn(&Surface) -> bool,
) -> Option<&Surface> {
    let mut best: Option<usize> = None;
    for i in candidates {
        let s = &surfaces[i];
        if !keep(s) {
            continue;
        }
        best = match best {
            Some(b) if surfaces[b].y < s.y || (surfaces[b].y == s.y && b <= i) => Some(b),
            _ => Some(i),
        };
    }
    best.map(|i| &surfaces[i])
}

#[derive(Clone, Debug, Default)]
pub struct World {
    screens: Vec<Screen>,
    surfaces: Vec<Surface>,
    /// L'index des surfaces, et s'il est à jour (sinon les questions les
    /// parcourent toutes : voir [`World::reindex`]).
    index: SurfaceIndex,
    indexed: bool,
    walls: Vec<Wall>,
    arena: Rect,
    bounds: Rect,
    /// La forme des écrans (voir [`crate::layout`]) : là où l'on marche et
    /// où l'on vole (de côté, la zone utile de chaque écran ; de haut en
    /// bas, de son bord supérieur à son sol), et les écrans entiers.
    rooms: Strips,
    frames: Strips,
    /// Les écrans font un seul rectangle, avec un seul niveau de sol (un
    /// seul écran, ou des écrans de même hauteur côte à côte) : rien à
    /// chercher, tout est l'arène.
    plain: bool,
    /// Fenêtres et réglages d'où viennent les rebords et les côtés actuels
    /// (`None` : à recalculer au prochain relevé).
    windows: Vec<WindowRect>,
    /// Celles sur lesquelles on marche (vide : toutes ; voir
    /// [`World::set_windows_among`]).
    walkable: Vec<bool>,
    options: Option<LedgeOptions>,
    /// Relevés des fenêtres reçus, même identiques au précédent : la
    /// physique réaliste sait ainsi qu'une fenêtre n'a pas bougé.
    polls: u64,
}

impl World {
    #[must_use]
    pub fn new(screens: Vec<Screen>) -> Self {
        let mut world = Self::default();
        world.set_screens(screens);
        world
    }

    /// Remplace les écrans et régénère les sols. Les rebords de fenêtres sont conservés.
    pub fn set_screens(&mut self, screens: Vec<Screen>) {
        self.screens = screens;
        self.arena = self
            .screens
            .iter()
            .map(|s| s.usable)
            .reduce(|a, b| a.union(&b))
            .unwrap_or_default();
        self.bounds = self
            .screens
            .iter()
            .map(|s| s.frame)
            .reduce(|a, b| a.union(&b))
            .unwrap_or_default();
        self.rooms.rebuild(self.screens.iter().map(Screen::room));
        self.frames.rebuild(self.screens.iter().map(|s| s.frame));
        // (Deux écrans pareils l'un sur l'autre font aussi un rectangle,
        // mais avec deux sols : ce n'est pas un bureau de plain-pied.)
        let level = self
            .screens
            .windows(2)
            .all(|pair| (pair[0].usable.bottom() - pair[1].usable.bottom()).abs() <= 1e-3);
        self.plain = level && self.rooms.is_one_rect() && self.frames.is_one_rect();
        self.surfaces.retain(|s| s.kind != SurfaceKind::Floor);
        // Les rebords dépendent des zones utiles : à refaire au prochain relevé.
        self.options = None;
        for (i, screen) in self.screens.iter().enumerate() {
            self.surfaces.push(Surface {
                id: FLOOR_ID_BIT | i as u64,
                kind: SurfaceKind::Floor,
                x0: screen.usable.left(),
                x1: screen.usable.right(),
                y: screen.usable.bottom(),
            });
        }
        self.reindex();
    }

    /// Remplace les rebords et les côtés des fenêtres à partir de la liste
    /// fournie par le système. Le relevé revient plusieurs fois par seconde,
    /// le plus souvent à l'identique : rien n'est alors recalculé.
    pub fn set_windows(&mut self, windows: &[WindowRect], options: LedgeOptions) {
        self.set_windows_among(windows, &[], options);
    }

    /// Les fenêtres, au premier plan d'abord, dont seules certaines portent
    /// les animaux : `walkable[i]` dit si on marche sur la i-ème (son rebord,
    /// ses côtés). Les autres ne font que cacher ce qui est derrière elles :
    /// quand les animaux ne vont que sur l'app au premier plan, une fenêtre
    /// d'une autre app posée devant cache toujours le rebord qu'elle couvre.
    /// `walkable` vide : on marche sur toutes.
    pub fn set_windows_among(
        &mut self,
        windows: &[WindowRect],
        walkable: &[bool],
        options: LedgeOptions,
    ) {
        self.polls += 1;
        if self.options == Some(options) && self.windows == windows && self.walkable == walkable {
            return;
        }
        let ledges = visible_ledges_among(windows, walkable, &self.screens, options);
        self.surfaces.retain(|s| s.kind == SurfaceKind::Floor);
        self.surfaces.extend(ledges);
        self.walls = visible_walls_among(windows, walkable);
        self.windows.clear();
        self.windows.extend_from_slice(windows);
        self.walkable.clear();
        self.walkable.extend_from_slice(walkable);
        self.options = Some(options);
        self.reindex();
    }

    /// Refait l'index des surfaces, après des changements faits par
    /// [`World::shift_windows`] ou [`World::push_surface`] (les autres le
    /// refont d'eux-mêmes). Sans lui, les questions sont justes, mais
    /// parcourent toutes les surfaces.
    pub(crate) fn reindex(&mut self) {
        self.index.rebuild(&self.surfaces);
        self.indexed = true;
    }

    /// L'index des surfaces, s'il est à jour.
    fn index(&self) -> Option<&SurfaceIndex> {
        self.indexed.then_some(&self.index)
    }

    /// Côtés visibles des fenêtres.
    #[must_use]
    pub fn walls(&self) -> &[Wall] {
        &self.walls
    }

    /// Les fenêtres du dernier relevé, au premier plan d'abord.
    #[must_use]
    pub fn windows(&self) -> &[WindowRect] {
        &self.windows
    }

    /// Nombre de relevés des fenêtres reçus jusqu'ici.
    #[must_use]
    pub fn polls(&self) -> u64 {
        self.polls
    }

    /// Devient une copie de `other`, en gardant la mémoire déjà allouée
    /// (la physique réaliste en refait une à chaque pas).
    pub(crate) fn copy_from(&mut self, other: &World) {
        self.screens.clone_from(&other.screens);
        self.surfaces.clone_from(&other.surfaces);
        // L'index sera refait (sa mémoire est gardée).
        self.indexed = false;
        self.walls.clone_from(&other.walls);
        self.windows.clone_from(&other.windows);
        self.walkable.clone_from(&other.walkable);
        self.arena = other.arena;
        self.bounds = other.bounds;
        self.rooms.clone_from(&other.rooms);
        self.frames.clone_from(&other.frames);
        self.plain = other.plain;
        self.options = other.options;
        self.polls = other.polls;
    }

    /// Déplace les rebords et les côtés de chaque fenêtre de ce que dit
    /// `offset` (identifiant de la fenêtre → décalage, ou rien).
    pub(crate) fn shift_windows(&mut self, offset: impl Fn(u64) -> Option<Vec2>) {
        self.indexed = false;
        for surface in &mut self.surfaces {
            if surface.kind == SurfaceKind::Window
                && let Some(d) = offset(surface.id >> 8)
            {
                surface.x0 += d.x;
                surface.x1 += d.x;
                surface.y += d.y;
            }
        }
        for wall in &mut self.walls {
            if let Some(d) = offset(wall.window) {
                wall.x += d.x;
                wall.y0 += d.y;
                wall.y1 += d.y;
            }
        }
    }

    /// Ajoute une surface (le dos d'un animal, en physique réaliste).
    pub(crate) fn push_surface(&mut self, surface: Surface) {
        self.indexed = false;
        self.surfaces.push(surface);
    }

    /// Premier côté de fenêtre que franchit l'avant d'un animal qui va de
    /// l'abscisse `from` à `to`, son corps couvrant les hauteurs `top` à
    /// `bottom`. Seul compte un côté tourné vers lui : on sort librement
    /// d'une fenêtre devant laquelle on se trouve.
    #[must_use]
    pub fn wall_crossed(&self, from: f32, to: f32, top: f32, bottom: f32) -> Option<&Wall> {
        if to == from {
            return None;
        }
        let dir = (to - from).signum();
        self.walls
            .iter()
            .filter(|w| {
                w.facing == -dir
                    && (w.x - from) * dir >= 0.0
                    && (to - w.x) * dir > 0.0
                    && w.y0 < bottom - 1.0
                    && w.y1 > top + 1.0
            })
            .min_by(|a, b| ((a.x - from) * dir).total_cmp(&((b.x - from) * dir)))
    }

    #[must_use]
    pub fn screens(&self) -> &[Screen] {
        &self.screens
    }

    #[must_use]
    pub fn surfaces(&self) -> &[Surface] {
        &self.surfaces
    }

    #[must_use]
    pub fn surface(&self, id: SurfaceId) -> Option<&Surface> {
        match self.index() {
            Some(index) => index.by_id.get(&id).map(|&i| &self.surfaces[i as usize]),
            None => self.surfaces.iter().find(|s| s.id == id),
        }
    }

    /// Union des zones utiles de tous les écrans.
    #[must_use]
    pub fn arena(&self) -> Rect {
        self.arena
    }

    /// Union des cadres complets de tous les écrans.
    #[must_use]
    pub fn bounds(&self) -> Rect {
        self.bounds
    }

    /// Écran qui contient le point, ou à défaut le plus proche (celui dont
    /// le bord est le plus près du point).
    #[must_use]
    pub fn screen_at(&self, p: Vec2) -> Option<&Screen> {
        let away = |s: &Screen| {
            let dx = (s.frame.left() - p.x).max(p.x - s.frame.right()).max(0.0);
            let dy = (s.frame.top() - p.y).max(p.y - s.frame.bottom()).max(0.0);
            dx.hypot(dy)
        };
        self.screens
            .iter()
            .find(|s| s.frame.contains(p))
            .or_else(|| {
                self.screens
                    .iter()
                    .min_by(|a, b| away(a).total_cmp(&away(b)))
            })
    }

    /// Les écrans font-ils un seul rectangle (un seul écran, ou des écrans
    /// de même hauteur côte à côte) ? Sinon, il y a autour d'eux des endroits
    /// que personne ne voit, où rien ne doit aller.
    #[must_use]
    pub fn is_plain(&self) -> bool {
        self.plain
    }

    /// Jusqu'où un corps, haut de `top` à `bottom`, peut aller de côté
    /// autour de l'abscisse `x` sans cesser d'être sur un écran : les bords
    /// qu'il ne doit pas passer (gauche, droite). Un écran voisin ne
    /// prolonge le chemin que s'il porte le corps sur toute sa hauteur : ni
    /// plus bas que le sol de cet écran, ni plus haut que son bord
    /// supérieur.
    ///
    /// Un corps descendu sous le sol de son écran (dans la bande de la barre
    /// des tâches, en route vers l'écran d'en dessous) est seulement tenu
    /// dans les écrans entiers ; un corps hors de tout écran reçoit les
    /// bords de l'endroit le plus proche, où il sera ramené.
    #[must_use]
    pub fn span(&self, x: f32, top: f32, bottom: f32) -> (f32, f32) {
        if self.plain {
            return (self.arena.left(), self.arena.right());
        }
        self.rooms
            .span(x, top, bottom)
            .or_else(|| self.frames.span(x, top, bottom))
            .or_else(|| self.rooms.nearest_span(x, top, bottom))
            .unwrap_or((self.arena.left(), self.arena.right()))
    }

    /// Le haut de l'écran au-dessus d'un corps large de `x0` à `x1`, à la
    /// hauteur `y` : celui du plus bas des écrans qu'il occupe, et tout en
    /// haut de l'écran du dessus quand deux écrans sont l'un sur l'autre.
    #[must_use]
    pub fn ceiling(&self, x0: f32, x1: f32, y: f32) -> f32 {
        if self.plain {
            return self.bounds.top();
        }
        self.frames
            .ceiling(x0, x1, y)
            .unwrap_or_else(|| self.bounds.top())
    }

    /// Le point est-il sur un écran ?
    #[must_use]
    pub fn on_screens(&self, p: Vec2) -> bool {
        self.frames.contains(p)
    }

    /// Les écrans montrent-ils tout ce rectangle, d'un seul tenant (barres
    /// des tâches comprises) ? C'est le chemin d'un animal qui passe d'un
    /// écran à celui du dessus ou du dessous (voir `pet/voyage.rs`).
    #[must_use]
    pub fn holds(&self, r: Rect) -> bool {
        self.frames.holds(r.left(), r.right(), r.top(), r.bottom())
    }

    /// Un écran montre-t-il au moins un bord de ce rectangle ? (Sinon, ce
    /// qui s'y trouve est entièrement sorti des écrans.)
    #[must_use]
    pub fn shows(&self, r: Rect) -> bool {
        let seen = |frame: &Rect| {
            r.right() >= frame.left()
                && r.left() <= frame.right()
                && r.bottom() >= frame.top()
                && r.top() <= frame.bottom()
        };
        if self.plain {
            return seen(&self.bounds);
        }
        self.screens.iter().any(|s| seen(&s.frame))
    }

    /// La zone utile où se passe ce qui a lieu autour de `p` (la visite
    /// d'une soucoupe) : toute l'arène quand les écrans font un seul
    /// rectangle, sinon celle de l'écran où se trouve `p`.
    #[must_use]
    pub fn arena_at(&self, p: Vec2) -> Rect {
        if self.plain {
            return self.arena;
        }
        self.screen_at(p).map_or(self.arena, |s| s.usable)
    }

    /// Le haut de la zone utile de l'écran où se trouve `p` (là d'où tombe
    /// un animal qui arrive).
    #[must_use]
    pub fn top_at(&self, p: Vec2) -> f32 {
        if self.plain {
            return self.arena.top();
        }
        self.screen_at(p)
            .map_or(self.arena.top(), |s| s.usable.top())
    }

    /// Le haut de la zone utile au-dessus de l'abscisse `x` : celle du
    /// premier écran de la liste qui s'étend au-dessus de `x` (l'écran
    /// principal vient d'abord), à défaut du plus proche de côté.
    #[must_use]
    pub fn top_over(&self, x: f32) -> f32 {
        if self.plain {
            return self.arena.top();
        }
        let aside = |s: &Screen| (s.usable.left() - x).max(x - s.usable.right()).max(0.0);
        self.screens
            .iter()
            .find(|s| aside(s) == 0.0)
            .or_else(|| {
                self.screens
                    .iter()
                    .min_by(|a, b| aside(a).total_cmp(&aside(b)))
            })
            .map_or(self.arena.top(), |s| s.usable.top())
    }

    /// Où fait-on arriver un animal ? Une abscisse au hasard, à `margin` des
    /// bords, et le haut de la zone utile de son écran : sur l'écran où se
    /// trouve `near` s'il est donné, sinon sur n'importe lequel (un écran
    /// deux fois plus large en reçoit deux fois plus). Un seul tirage.
    pub fn entry(&self, rng: &mut Rng, margin: f32, near: Option<Vec2>) -> Vec2 {
        let arena = self.arena;
        let within = |r: Rect, t: f32| {
            let lo = r.left() + margin;
            let hi = (r.right() - margin).max(lo);
            Vec2::new(lo + (hi - lo) * t, r.top())
        };
        if let Some(screen) = near.and_then(|p| self.screen_at(p)) {
            return within(screen.usable, rng.next_f32());
        }
        if self.plain || self.screens.is_empty() {
            let lo = arena.left() + margin;
            return Vec2::new(rng.range(lo, (arena.right() - margin).max(lo)), arena.top());
        }
        // Les largeurs mises bout à bout : un point au hasard sur le tout.
        let room = |s: &Screen| (s.usable.w - 2.0 * margin).max(1.0);
        let total: f32 = self.screens.iter().map(room).sum();
        let mut at = rng.range(0.0, total);
        for screen in &self.screens {
            let w = room(screen);
            if at <= w {
                return within(screen.usable, (at / w).clamp(0.0, 1.0));
            }
            at -= w;
        }
        within(self.screens[self.screens.len() - 1].usable, 1.0)
    }

    /// Surface franchie en descendant de `y0` à `y1` à l'abscisse `x` :
    /// la plus haute dont le niveau est compris dans l'intervalle.
    #[must_use]
    pub fn landing_surface(&self, x: f32, margin: f32, y0: f32, y1: f32) -> Option<&Surface> {
        let keep = |s: &Surface| s.contains_x(x, margin) && s.y >= y0 - 0.01 && s.y <= y1 + 0.01;
        self.highest_near(x - margin, x + margin, keep)
    }

    /// Surface la plus proche sous le point (pour l'ombre portée pendant un saut).
    #[must_use]
    pub fn surface_below(&self, p: Vec2, margin: f32) -> Option<&Surface> {
        let keep = |s: &Surface| s.contains_x(p.x, margin) && s.y >= p.y - 0.5;
        self.highest_near(p.x - margin, p.x + margin, keep)
    }

    /// Le rang (dans [`World::surfaces`]) de la surface `id`, la première
    /// s'il y en a plusieurs (comme [`World::surface`]).
    #[must_use]
    pub(crate) fn rank_of(&self, id: SurfaceId) -> Option<usize> {
        match self.index() {
            Some(index) => index.by_id.get(&id).map(|&i| i as usize),
            None => self.surfaces.iter().position(|s| s.id == id),
        }
    }

    /// Les rangs (dans [`World::surfaces`]) des surfaces qui peuvent couvrir
    /// les abscisses de `x0` à `x1` : celles de leurs colonnes si l'index
    /// est à jour (une surface sur plusieurs colonnes revient pour chacune),
    /// sinon toutes. Avec un millier de dos d'animaux parmi les surfaces, un
    /// animal en l'air n'en regarde ainsi qu'une poignée.
    pub(crate) fn near_surfaces(&self, x0: f32, x1: f32) -> NearSurfaces<'_> {
        match self.index() {
            Some(index) => NearSurfaces::Indexed(index.near_ranks(x0, x1).iter()),
            None => NearSurfaces::All(0..self.surfaces.len()),
        }
    }

    /// La plus haute des surfaces gardées par `keep`, parmi celles qui
    /// peuvent couvrir les abscisses de `x0` à `x1` (par l'index s'il est à
    /// jour) ; la première de la liste en cas d'égalité.
    fn highest_near(&self, x0: f32, x1: f32, keep: impl Fn(&Surface) -> bool) -> Option<&Surface> {
        match self.index() {
            Some(index) => highest(&self.surfaces, index.near(x0, x1), keep),
            None => highest(&self.surfaces, 0..self.surfaces.len(), keep),
        }
    }

    /// Sol d'un écran qui s'étend au-dessus de l'abscisse `x`, le plus proche
    /// de la hauteur `y` (pour passer d'un écran à l'autre).
    #[must_use]
    pub fn floor_at(&self, x: f32, y: f32) -> Option<&Surface> {
        self.floors()
            .filter(|s| s.contains_x(x, 0.0))
            .min_by(|a, b| (a.y - y).abs().total_cmp(&(b.y - y).abs()))
    }

    /// Les sols des écrans, dans l'ordre de la liste des surfaces.
    pub(crate) fn floors(&self) -> impl Iterator<Item = &Surface> + '_ {
        let indexed = self.index().map(|index| index.floors.as_slice());
        let all = if indexed.is_some() {
            &self.surfaces[..0]
        } else {
            &self.surfaces[..]
        };
        indexed
            .unwrap_or_default()
            .iter()
            .map(|&i| &self.surfaces[i as usize])
            .chain(all.iter().filter(|s| s.kind == SurfaceKind::Floor))
    }

    /// Le sol sous le point : parmi les sols des écrans qui s'étendent
    /// au-dessus de l'abscisse `x`, le premier en descendant depuis la
    /// hauteur `y` ; si le point est plus bas qu'eux tous, le plus bas. Hors
    /// de tout sol, celui du plus proche de côté. (Deux écrans l'un sur
    /// l'autre ont chacun leur sol à la même abscisse.)
    #[must_use]
    pub fn floor_below(&self, x: f32, y: f32) -> Option<&Surface> {
        let mut below: Option<&Surface> = None;
        let mut lowest: Option<&Surface> = None;
        for floor in self.floors().filter(|s| s.contains_x(x, 0.0)) {
            if floor.y >= y - GROUNDED && below.is_none_or(|b| floor.y < b.y) {
                below = Some(floor);
            }
            if lowest.is_none_or(|l| floor.y > l.y) {
                lowest = Some(floor);
            }
        }
        below.or(lowest).or_else(|| self.floor_under(x))
    }

    /// Le sol où ramener ce qui est tombé plus bas que tous les sols de son
    /// abscisse (lâché sous le sol, ou resté là quand un écran a disparu) ;
    /// rien tant qu'il reste un sol sous le point (`y` est alors au-dessus
    /// de lui, ou à moins d'un point dessous).
    #[must_use]
    pub fn floor_missed(&self, x: f32, y: f32) -> Option<&Surface> {
        let floor = self.floor_below(x, y)?;
        (y > floor.y + 1.0).then_some(floor)
    }

    /// Sol de l'écran situé sous l'abscisse `x` (ou du plus proche).
    #[must_use]
    pub fn floor_under(&self, x: f32) -> Option<&Surface> {
        self.floors().min_by(|a, b| {
            let da = if a.contains_x(x, 0.0) {
                0.0
            } else {
                (a.x0 - x).abs().min((a.x1 - x).abs())
            };
            let db = if b.contains_x(x, 0.0) {
                0.0
            } else {
                (b.x0 - x).abs().min((b.x1 - x).abs())
            };
            da.total_cmp(&db)
        })
    }
}

/// Calcule les morceaux visibles du bord supérieur de chaque fenêtre.
///
/// Une fenêtre placée devant en masque une partie : on soustrait sa largeur
/// au rebord si elle couvre la hauteur de ce rebord. On ne garde que les
/// morceaux assez larges et assez bas sous la barre des menus.
#[must_use]
pub fn visible_ledges(
    windows: &[WindowRect],
    screens: &[Screen],
    options: LedgeOptions,
) -> Vec<Surface> {
    visible_ledges_among(windows, &[], screens, options)
}

/// Les mêmes, seulement pour les fenêtres où l'on marche (`walkable`, vide :
/// toutes) ; les autres cachent quand même ce qu'elles couvrent.
#[must_use]
pub fn visible_ledges_among(
    windows: &[WindowRect],
    walkable: &[bool],
    screens: &[Screen],
    options: LedgeOptions,
) -> Vec<Surface> {
    let mut out = Vec::new();
    for (index, window) in windows.iter().enumerate() {
        if !walkable.get(index).copied().unwrap_or(true) {
            continue;
        }
        let y = window.frame.top();
        // Le centre de la fenêtre peut être hors écran, et son rebord
        // traverser plusieurs écrans. Chaque zone utile contribue sa part.
        let mut spans: Vec<(f32, f32)> = screens
            .iter()
            .filter(|screen| {
                y >= screen.usable.top() + options.headroom && y < screen.usable.bottom() - 1.0
            })
            .filter_map(|screen| {
                let x0 = window.frame.left().max(screen.usable.left());
                let x1 = window.frame.right().min(screen.usable.right());
                (x0 < x1).then_some((x0, x1))
            })
            .collect();
        spans.sort_by(|a, b| a.0.total_cmp(&b.0).then(a.1.total_cmp(&b.1)));
        // Deux écrans contigus ne créent pas un faux bord où se retourner ;
        // deux écrans en miroir ne créent pas deux surfaces superposées.
        let mut merged = 0;
        for i in 0..spans.len() {
            let span = spans[i];
            if merged > 0 && span.0 <= spans[merged - 1].1 {
                spans[merged - 1].1 = spans[merged - 1].1.max(span.1);
            } else {
                spans[merged] = span;
                merged += 1;
            }
        }
        spans.truncate(merged);
        for front in &windows[..index] {
            let f = front.frame;
            // On masque aussi un peu au-dessus du rebord : l'animal s'y tient debout.
            if f.top() <= y + 1.0 && f.bottom() >= y - 8.0 {
                spans = subtract_span(&spans, f.left(), f.right());
                if spans.is_empty() {
                    break;
                }
            }
        }
        for (span_index, (x0, x1)) in spans.into_iter().enumerate() {
            if x1 - x0 >= options.min_span {
                out.push(Surface {
                    id: (window.id << 8) | (span_index as u64 & 0xFF),
                    kind: SurfaceKind::Window,
                    x0,
                    x1,
                    y,
                });
            }
        }
    }
    out
}

/// Identifiant du Dock quand il est traité comme une fenêtre (voir [`exact_dock`]).
pub const DOCK_ID: u64 = 1 << 40;

/// Le Dock tel qu'il est. Quand il est en bas d'un écran (la zone utile de
/// cet écran s'arrête au-dessus de lui) et moins large que l'écran, le sol
/// de cet écran descend jusqu'en bas, de part et d'autre du Dock, et le
/// Dock devient une fenêtre posée devant les autres : on marche dessus, on
/// tombe de ses bouts, on bute contre ses côtés. Renvoie cette fenêtre, à
/// placer en tête de la liste des fenêtres ; rien pour un Dock sur le côté,
/// masqué automatiquement ou sur toute la largeur.
pub fn exact_dock(screens: &mut [Screen], dock: Rect) -> Option<WindowRect> {
    let center = dock.center();
    let screen = screens.iter_mut().find(|s| s.frame.contains(center))?;
    let strip = screen.frame.bottom() - screen.usable.bottom();
    // Dock en bas : une bande réservée sous la zone utile, où il se trouve.
    let at_bottom = strip > 1.0
        && dock.top() >= screen.usable.bottom() - dock.h * 0.5
        && dock.bottom() <= screen.frame.bottom() + 1.0;
    if !at_bottom || dock.w >= screen.frame.w - 1.0 {
        return None;
    }
    screen.usable.h = screen.frame.bottom() - screen.usable.top();
    Some(WindowRect {
        id: DOCK_ID,
        frame: dock,
    })
}

/// Calcule les parties visibles des côtés de chaque fenêtre : une fenêtre
/// placée devant cache la partie d'un côté qu'elle recouvre.
#[must_use]
pub fn visible_walls(windows: &[WindowRect]) -> Vec<Wall> {
    visible_walls_among(windows, &[])
}

/// Les mêmes, seulement pour les fenêtres où l'on marche (`walkable`, vide :
/// toutes) ; les autres cachent quand même ce qu'elles couvrent.
#[must_use]
pub fn visible_walls_among(windows: &[WindowRect], walkable: &[bool]) -> Vec<Wall> {
    let mut out = Vec::new();
    for (index, window) in windows.iter().enumerate() {
        if !walkable.get(index).copied().unwrap_or(true) {
            continue;
        }
        let f = window.frame;
        for (x, facing) in [(f.left(), -1.0), (f.right(), 1.0)] {
            let mut spans = vec![(f.top(), f.bottom())];
            for front in &windows[..index] {
                let g = front.frame;
                if g.left() < x && g.right() > x {
                    spans = subtract_span(&spans, g.top(), g.bottom());
                    if spans.is_empty() {
                        break;
                    }
                }
            }
            out.extend(
                spans
                    .into_iter()
                    .filter(|(y0, y1)| y1 - y0 >= MIN_WALL)
                    .map(|(y0, y1)| Wall {
                        window: window.id,
                        x,
                        y0,
                        y1,
                        facing,
                    }),
            );
        }
    }
    out
}

fn subtract_span(spans: &[(f32, f32)], cut0: f32, cut1: f32) -> Vec<(f32, f32)> {
    let mut out = Vec::with_capacity(spans.len() + 1);
    for &(a, b) in spans {
        if cut1 <= a || cut0 >= b {
            out.push((a, b));
            continue;
        }
        if cut0 > a {
            out.push((a, cut0));
        }
        if cut1 < b {
            out.push((cut1, b));
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    fn screen() -> Screen {
        Screen {
            frame: Rect::new(0.0, 0.0, 1440.0, 900.0),
            usable: Rect::new(0.0, 25.0, 1440.0, 800.0),
            scale: 2.0,
        }
    }

    #[test]
    fn floor_is_bottom_of_usable_area() {
        let world = World::new(vec![screen()]);
        let floor = world.floor_under(100.0).unwrap();
        assert_eq!(floor.y, 825.0);
        assert_eq!(floor.kind, SurfaceKind::Floor);
    }

    #[test]
    fn ledges_follow_the_windows_and_the_screens() {
        let window = WindowRect {
            id: 7,
            frame: Rect::new(100.0, 300.0, 400.0, 300.0),
        };
        let mut world = World::new(vec![screen()]);
        let ledge = |world: &World| {
            world
                .surfaces()
                .iter()
                .find(|s| s.kind == SurfaceKind::Window)
                .map(|s| (s.x0, s.x1, s.y))
        };
        world.set_windows(&[window], LedgeOptions::default());
        assert_eq!(ledge(&world), Some((100.0, 500.0, 300.0)));
        // Le même relevé : rien ne change.
        world.set_windows(&[window], LedgeOptions::default());
        assert_eq!(ledge(&world), Some((100.0, 500.0, 300.0)));
        // La fenêtre bouge : le rebord suit.
        let moved = WindowRect {
            frame: Rect::new(150.0, 320.0, 400.0, 300.0),
            ..window
        };
        world.set_windows(&[moved], LedgeOptions::default());
        assert_eq!(ledge(&world), Some((150.0, 550.0, 320.0)));
        // Un écran plus étroit : le même relevé redonne un rebord coupé au bord.
        let mut narrow = screen();
        narrow.frame.w = 400.0;
        narrow.usable.w = 400.0;
        world.set_screens(vec![narrow]);
        world.set_windows(&[moved], LedgeOptions::default());
        assert_eq!(ledge(&world), Some((150.0, 400.0, 320.0)));
    }

    #[test]
    fn front_window_hides_part_of_ledge() {
        let windows = [
            WindowRect {
                id: 1,
                frame: Rect::new(300.0, 150.0, 200.0, 300.0),
            },
            WindowRect {
                id: 2,
                frame: Rect::new(100.0, 200.0, 600.0, 400.0),
            },
        ];
        let ledges = visible_ledges(&windows, &[screen()], LedgeOptions::default());
        let back: Vec<_> = ledges
            .iter()
            .filter(|s| s.id >> 8 == 2)
            .map(|s| (s.x0, s.x1))
            .collect();
        assert_eq!(back, vec![(100.0, 300.0), (500.0, 700.0)]);
    }

    #[test]
    fn maximized_window_offers_no_ledge() {
        let windows = [WindowRect {
            id: 3,
            frame: Rect::new(0.0, 25.0, 1440.0, 800.0),
        }];
        assert!(visible_ledges(&windows, &[screen()], LedgeOptions::default()).is_empty());
    }

    #[test]
    fn a_window_keeps_its_visible_ledge_when_its_center_is_off_screen() {
        let windows = [WindowRect {
            id: 7,
            frame: Rect::new(-600.0, 300.0, 800.0, 300.0),
        }];
        let ledges = visible_ledges(&windows, &[screen()], LedgeOptions::default());
        assert_eq!(ledges.len(), 1);
        assert_eq!((ledges[0].x0, ledges[0].x1), (0.0, 200.0));
    }

    #[test]
    fn a_ledge_crosses_adjacent_screens_without_an_artificial_edge() {
        let right = Screen {
            frame: Rect::new(1440.0, 0.0, 1440.0, 900.0),
            usable: Rect::new(1440.0, 25.0, 1440.0, 800.0),
            ..screen()
        };
        let windows = [WindowRect {
            id: 8,
            frame: Rect::new(1300.0, 300.0, 500.0, 300.0),
        }];
        for screens in [[screen(), right], [right, screen()]] {
            let ledges = visible_ledges(&windows, &screens, LedgeOptions::default());
            assert_eq!(ledges.len(), 1);
            assert_eq!((ledges[0].x0, ledges[0].x1), (1300.0, 1800.0));
        }
    }

    #[test]
    fn ledges_preserve_screen_gaps_and_check_headroom_on_each_screen() {
        let right = Screen {
            frame: Rect::new(1600.0, 260.0, 1440.0, 900.0),
            usable: Rect::new(1600.0, 285.0, 1440.0, 800.0),
            ..screen()
        };
        let mut windows = [WindowRect {
            id: 9,
            frame: Rect::new(1300.0, 300.0, 500.0, 300.0),
        }];
        let spans = |windows: &[WindowRect]| {
            visible_ledges(windows, &[screen(), right], LedgeOptions::default())
                .into_iter()
                .map(|s| (s.x0, s.x1))
                .collect::<Vec<_>>()
        };
        // Sur l'écran de droite, 15 pt sous la barre ne suffisent pas.
        assert_eq!(spans(&windows), vec![(1300.0, 1440.0)]);
        windows[0].frame.y = 400.0;
        assert_eq!(spans(&windows), vec![(1300.0, 1440.0), (1600.0, 1800.0)]);
    }

    #[test]
    fn a_foreground_window_hides_a_ledge_across_the_screen_boundary() {
        let right = Screen {
            frame: Rect::new(1440.0, 0.0, 1440.0, 900.0),
            usable: Rect::new(1440.0, 25.0, 1440.0, 800.0),
            ..screen()
        };
        let windows = [
            WindowRect {
                id: 10,
                frame: Rect::new(1400.0, 200.0, 100.0, 300.0),
            },
            WindowRect {
                id: 11,
                frame: Rect::new(1300.0, 300.0, 500.0, 300.0),
            },
        ];
        let ledges = visible_ledges_among(
            &windows,
            &[false, true],
            &[screen(), right],
            LedgeOptions::default(),
        );
        let spans: Vec<_> = ledges.iter().map(|s| (s.x0, s.x1)).collect();
        assert_eq!(spans, vec![(1300.0, 1400.0), (1500.0, 1800.0)]);
        assert_ne!(ledges[0].id, ledges[1].id);
    }

    #[test]
    fn front_window_hides_part_of_a_side() {
        let windows = [
            WindowRect {
                id: 1,
                frame: Rect::new(50.0, 300.0, 200.0, 100.0),
            },
            WindowRect {
                id: 2,
                frame: Rect::new(100.0, 200.0, 600.0, 400.0),
            },
        ];
        let walls = visible_walls(&windows);
        let back_left: Vec<_> = walls
            .iter()
            .filter(|w| w.window == 2 && w.facing < 0.0)
            .map(|w| (w.x, w.y0, w.y1))
            .collect();
        assert_eq!(
            back_left,
            vec![(100.0, 200.0, 300.0), (100.0, 400.0, 600.0)]
        );
        let back_right: Vec<_> = walls
            .iter()
            .filter(|w| w.window == 2 && w.facing > 0.0)
            .map(|w| (w.x, w.y0, w.y1))
            .collect();
        assert_eq!(back_right, vec![(700.0, 200.0, 600.0)]);
        // La fenêtre de devant garde ses deux côtés entiers.
        assert_eq!(walls.iter().filter(|w| w.window == 1).count(), 2);
    }

    #[test]
    fn a_side_stops_only_who_walks_into_it() {
        let mut world = World::new(vec![screen()]);
        world.set_windows(
            &[WindowRect {
                id: 7,
                frame: Rect::new(500.0, 600.0, 400.0, 400.0),
            }],
            LedgeOptions::default(),
        );
        // Au sol (corps de 765 à 825), vers la droite : le côté gauche l'arrête.
        let wall = world.wall_crossed(498.0, 501.0, 765.0, 825.0).unwrap();
        assert_eq!((wall.x, wall.facing), (500.0, -1.0));
        // Vers la gauche, de l'autre côté : le côté droit.
        let wall = world.wall_crossed(902.0, 899.0, 765.0, 825.0).unwrap();
        assert_eq!((wall.x, wall.facing), (900.0, 1.0));
        // Devant la fenêtre, il en sort librement.
        assert!(world.wall_crossed(899.0, 902.0, 765.0, 825.0).is_none());
        assert!(world.wall_crossed(501.0, 498.0, 765.0, 825.0).is_none());
        // Au-dessus de la fenêtre (sur un rebord plus haut), rien ne l'arrête.
        assert!(world.wall_crossed(498.0, 501.0, 500.0, 560.0).is_none());
        // Pas encore arrivé au côté.
        assert!(world.wall_crossed(480.0, 483.0, 765.0, 825.0).is_none());
    }

    #[test]
    fn the_exact_dock_lowers_the_floor_and_becomes_a_ledge() {
        let mut screens = vec![screen()];
        let dock = Rect::new(420.0, 832.0, 600.0, 64.0);
        let window = exact_dock(&mut screens, dock).unwrap();
        assert_eq!(window.id, DOCK_ID);
        assert_eq!(
            screens[0].usable.bottom(),
            900.0,
            "le sol descend en bas de l'écran"
        );
        let mut world = World::new(screens);
        world.set_windows(&[window], LedgeOptions::default());
        assert_eq!(world.floor_under(100.0).unwrap().y, 900.0);
        let top = world
            .surfaces()
            .iter()
            .find(|s| s.kind == SurfaceKind::Window)
            .unwrap();
        assert_eq!((top.x0, top.x1, top.y), (420.0, 1020.0, 832.0));
        // On bute contre ses côtés en marchant en bas de l'écran.
        assert!(world.wall_crossed(418.0, 421.0, 840.0, 900.0).is_some());
    }

    #[test]
    fn a_dock_on_the_side_hidden_or_full_width_changes_nothing() {
        // Sur le côté gauche : la zone utile commence à sa droite.
        let mut side = vec![Screen {
            frame: Rect::new(0.0, 0.0, 1440.0, 900.0),
            usable: Rect::new(70.0, 25.0, 1370.0, 875.0),
            scale: 2.0,
        }];
        assert!(exact_dock(&mut side, Rect::new(0.0, 200.0, 66.0, 500.0)).is_none());
        assert_eq!(side[0].usable.bottom(), 900.0);
        // Masqué automatiquement : pas de bande réservée en bas.
        let mut hidden = vec![Screen {
            usable: Rect::new(0.0, 25.0, 1440.0, 875.0),
            ..screen()
        }];
        assert!(exact_dock(&mut hidden, Rect::new(420.0, 832.0, 600.0, 64.0)).is_none());
        // Toute la largeur : rien à gagner.
        let mut wide = vec![screen()];
        assert!(exact_dock(&mut wide, Rect::new(0.0, 830.0, 1440.0, 70.0)).is_none());
        assert_eq!(wide[0].usable.bottom(), 825.0);
    }

    #[test]
    fn the_surface_index_answers_like_a_full_scan() {
        let mut rng = crate::math::Rng::new(42);
        let mut unit = || (rng.next_u32() >> 8) as f32 / (1u32 << 24) as f32;
        let mut world = World::new(vec![
            Screen {
                frame: Rect::new(0.0, 0.0, 1728.0, 1117.0),
                usable: Rect::new(0.0, 38.0, 1728.0, 1010.0),
                scale: 2.0,
            },
            Screen {
                frame: Rect::new(1728.0, -200.0, 2560.0, 1440.0),
                usable: Rect::new(1728.0, -175.0, 2560.0, 1415.0),
                scale: 1.0,
            },
        ]);
        let windows: Vec<WindowRect> = (0..15)
            .map(|i| WindowRect {
                id: 10 + i,
                frame: Rect::new(
                    unit() * 3800.0,
                    100.0 + unit() * 900.0,
                    200.0 + unit() * 900.0,
                    300.0,
                ),
            })
            .collect();
        world.set_windows(&windows, LedgeOptions::default());
        // Des dos d'animaux, certains à la même hauteur (égalités).
        for k in 0..400 {
            let x = unit() * 4300.0;
            let y = if k % 7 == 0 {
                600.0
            } else {
                200.0 + unit() * 900.0
            };
            world.push_surface(Surface {
                id: (1 << 62) | k,
                kind: SurfaceKind::Window,
                x0: x,
                x1: x + 20.0 + unit() * 60.0,
                y,
            });
        }
        let mut indexed = world.clone();
        indexed.reindex();
        assert!(world.index().is_none() && indexed.index().is_some());
        let same = |a: Option<&Surface>, b: Option<&Surface>| match (a, b) {
            (Some(a), Some(b)) => a.id == b.id,
            (None, None) => true,
            _ => false,
        };
        for _ in 0..3000 {
            let x = unit() * 4600.0 - 150.0;
            let y = unit() * 1300.0 - 100.0;
            let margin = unit() * 40.0;
            let span = unit() * 500.0;
            assert!(same(
                world.landing_surface(x, margin, y, y + span),
                indexed.landing_surface(x, margin, y, y + span)
            ));
            assert!(same(
                world.surface_below(Vec2::new(x, y), margin),
                indexed.surface_below(Vec2::new(x, y), margin)
            ));
            assert!(same(world.floor_at(x, y), indexed.floor_at(x, y)));
            assert!(same(world.floor_under(x), indexed.floor_under(x)));
        }
        for s in world.surfaces() {
            assert!(same(world.surface(s.id), indexed.surface(s.id)));
        }
        assert!(indexed.surface(12_345).is_none());
    }

    #[test]
    fn only_screens_that_make_one_rectangle_on_one_level_are_plain() {
        let side = |x: f32| Screen {
            frame: Rect::new(x, 0.0, 1440.0, 900.0),
            usable: Rect::new(x, 25.0, 1440.0, 800.0),
            scale: 2.0,
        };
        // Un écran ; deux pareils côte à côte : tout est l'arène, comme avant.
        assert!(World::new(vec![screen()]).is_plain());
        let two = World::new(vec![side(0.0), side(1440.0)]);
        assert!(two.is_plain());
        assert_eq!(two.span(100.0, 700.0, 825.0), (0.0, 2880.0));
        assert_eq!(two.ceiling(100.0, 160.0, 500.0), 0.0);
        // Pas d'écran : rien de particulier non plus à chercher.
        assert!(!World::new(Vec::new()).is_plain());
        assert_eq!(World::new(Vec::new()).span(0.0, 0.0, 1.0), (0.0, 0.0));
        // L'un plus bas que l'autre, ou une barre plus haute d'un côté.
        let mut lower = side(1440.0);
        lower.frame.y = 200.0;
        lower.usable.y = 225.0;
        assert!(!World::new(vec![side(0.0), lower]).is_plain());
        let mut taller_bar = side(1440.0);
        taller_bar.usable.h = 780.0;
        assert!(!World::new(vec![side(0.0), taller_bar]).is_plain());
        // Deux écrans pareils l'un sur l'autre, sans rien entre eux : un
        // seul rectangle, mais deux sols.
        let below = Screen {
            frame: Rect::new(0.0, 900.0, 1440.0, 900.0),
            usable: Rect::new(0.0, 900.0, 1440.0, 860.0),
            scale: 2.0,
        };
        let above = Screen {
            frame: Rect::new(0.0, 0.0, 1440.0, 900.0),
            usable: Rect::new(0.0, 0.0, 1440.0, 900.0),
            scale: 2.0,
        };
        let stacked = World::new(vec![below, above]);
        assert!(!stacked.is_plain());
        // Chaque point a son sol : le premier en descendant.
        assert_eq!(stacked.floor_below(700.0, 400.0).unwrap().y, 900.0);
        assert_eq!(stacked.floor_below(700.0, 1200.0).unwrap().y, 1760.0);
        assert!(stacked.floor_missed(700.0, 1200.0).is_none());
        assert_eq!(stacked.floor_missed(700.0, 1790.0).unwrap().y, 1760.0);
        // Et le plafond de celui du bas est tout en haut de celui du haut.
        assert_eq!(stacked.ceiling(100.0, 160.0, 1500.0), 0.0);
    }

    #[test]
    fn landing_picks_highest_crossed_surface() {
        let mut world = World::new(vec![screen()]);
        world.set_windows(
            &[WindowRect {
                id: 9,
                frame: Rect::new(0.0, 400.0, 500.0, 200.0),
            }],
            LedgeOptions::default(),
        );
        let hit = world.landing_surface(200.0, 0.0, 100.0, 900.0).unwrap();
        assert_eq!(hit.y, 400.0);
        assert!(world.landing_surface(800.0, 0.0, 100.0, 500.0).is_none());
    }
}
