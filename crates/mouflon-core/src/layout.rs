//! La forme des écrans, pour ce qui s'y déplace.
//!
//! Des écrans de même hauteur, côte à côte, font un seul grand rectangle :
//! tout y est simple. Mais on les range aussi l'un au-dessus de l'autre,
//! décalés, en L, grands et petits mêlés, et il reste alors, autour d'eux et
//! entre eux, des endroits que personne ne voit. Un animal qui y entrerait
//! serait coupé au bord d'un écran, puis disparaîtrait.
//!
//! Les rectangles sont rangés ici en **bandes** verticales : entre deux
//! abscisses où aucun rectangle ne commence ni ne finit, les hauteurs
//! couvertes sont les mêmes partout. Quatre écrans font sept bandes au
//! plus : chaque question (jusqu'où aller de côté ? où est le plafond ?) les
//! parcourt sans rien allouer.

use crate::math::{Rect, Vec2};

/// Deux bords plus proches que ceci se touchent (points) : des écrans
/// voisins dont les mesures viennent d'un arrondi restent voisins.
pub(crate) const TOUCH: f32 = 1.0;
/// Deux abscisses plus proches que ceci n'en font qu'une.
const SAME: f32 = 1e-3;

/// Une bande : de `x0` à `x1`, les hauteurs couvertes sont
/// `runs[first..first + len]` (de haut en bas, sans se toucher).
#[derive(Clone, Copy, Debug, PartialEq)]
struct Strip {
    x0: f32,
    x1: f32,
    first: u32,
    len: u32,
}

/// Des rectangles rangés en bandes (voir le module).
#[derive(Clone, Debug, Default, PartialEq)]
pub(crate) struct Strips {
    strips: Vec<Strip>,
    /// Hauteurs couvertes : haut, bas.
    runs: Vec<(f32, f32)>,
    /// Pour ranger (gardés d'une fois à l'autre).
    xs: Vec<f32>,
    ys: Vec<(f32, f32)>,
}

impl Strips {
    /// Range ces rectangles, en gardant la mémoire déjà prise. Ceux qui
    /// n'ont pas de surface, ou dont une mesure n'est pas un nombre, ne
    /// comptent pas.
    pub(crate) fn rebuild(&mut self, rects: impl Iterator<Item = Rect> + Clone) {
        let good = |r: &Rect| {
            r.x.is_finite()
                && r.y.is_finite()
                && r.w.is_finite()
                && r.h.is_finite()
                && r.w > 0.0
                && r.h > 0.0
        };
        self.strips.clear();
        self.runs.clear();
        let mut xs = std::mem::take(&mut self.xs);
        let mut ys = std::mem::take(&mut self.ys);
        xs.clear();
        for r in rects.clone().filter(good) {
            xs.push(r.left());
            xs.push(r.right());
        }
        xs.sort_by(f32::total_cmp);
        xs.dedup_by(|b, a| *b - *a <= SAME);
        for pair in xs.windows(2) {
            let (a, b) = (pair[0], pair[1]);
            ys.clear();
            ys.extend(
                rects
                    .clone()
                    .filter(good)
                    .filter(|r| r.left() <= a + SAME && r.right() >= b - SAME)
                    .map(|r| (r.top(), r.bottom())),
            );
            if ys.is_empty() {
                continue;
            }
            ys.sort_by(|p, q| p.0.total_cmp(&q.0));
            let first = self.runs.len();
            for &(top, bottom) in &ys {
                match self.runs[first..].last_mut() {
                    Some(last) if top <= last.1 + TOUCH => last.1 = last.1.max(bottom),
                    _ => self.runs.push((top, bottom)),
                }
            }
            self.strips.push(Strip {
                x0: a,
                x1: b,
                first: first as u32,
                len: (self.runs.len() - first) as u32,
            });
        }
        self.xs = xs;
        self.ys = ys;
    }

    fn runs_of(&self, strip: usize) -> &[(f32, f32)] {
        let s = &self.strips[strip];
        &self.runs[s.first as usize..(s.first + s.len) as usize]
    }

    /// Les hauteurs couvertes de la bande `strip` où se trouve `y`.
    fn run_at(&self, strip: usize, y: f32) -> Option<(f32, f32)> {
        self.runs_of(strip)
            .iter()
            .copied()
            .find(|run| y >= run.0 - TOUCH && y <= run.1 + TOUCH)
    }

    /// La bande `strip` couvre-t-elle tout le segment de `top` à `bottom` ?
    fn covers(&self, strip: usize, top: f32, bottom: f32) -> bool {
        self.runs_of(strip)
            .iter()
            .any(|run| run.0 <= top + TOUCH && run.1 >= bottom - TOUCH)
    }

    /// La bande `strip` touche-t-elle la suivante ?
    fn joins_next(&self, strip: usize) -> bool {
        self.strips
            .get(strip + 1)
            .is_some_and(|next| next.x0 <= self.strips[strip].x1 + TOUCH)
    }

    /// De la bande `strip`, où le segment de `top` à `bottom` est couvert,
    /// jusqu'où va-t-on de chaque côté sans qu'il cesse de l'être ?
    fn spread(&self, strip: usize, top: f32, bottom: f32) -> (f32, f32) {
        let (mut lo, mut hi) = (self.strips[strip].x0, self.strips[strip].x1);
        let mut j = strip;
        while j > 0 && self.joins_next(j - 1) && self.covers(j - 1, top, bottom) {
            j -= 1;
            lo = self.strips[j].x0;
        }
        let mut j = strip;
        while self.joins_next(j) && self.covers(j + 1, top, bottom) {
            j += 1;
            hi = self.strips[j].x1;
        }
        (lo, hi)
    }

    /// Un seul rectangle, d'un seul tenant ?
    pub(crate) fn is_one_rect(&self) -> bool {
        let Some(first) = self.runs.first().copied() else {
            return false;
        };
        self.strips.iter().enumerate().all(|(i, s)| {
            s.len == 1
                && (self.runs[s.first as usize].0 - first.0).abs() <= SAME
                && (self.runs[s.first as usize].1 - first.1).abs() <= SAME
                && (i + 1 == self.strips.len() || self.joins_next(i))
        })
    }

    /// Autour de l'abscisse `x`, l'intervalle des abscisses où tout le
    /// segment vertical de `top` à `bottom` reste couvert. Le bas du segment
    /// doit être couvert en `x` (sinon : `None`) ; ce qui, en `x`, dépasse
    /// déjà par le haut ne compte pas (un animal dont la tête passe le haut
    /// de son écran n'est pas arrêté pour cela).
    pub(crate) fn span(&self, x: f32, top: f32, bottom: f32) -> Option<(f32, f32)> {
        let mut best: Option<(f32, f32)> = None;
        for (i, s) in self.strips.iter().enumerate() {
            if x < s.x0 - TOUCH || x > s.x1 + TOUCH {
                continue;
            }
            let Some(run) = self.run_at(i, bottom) else {
                continue;
            };
            let found = self.spread(i, top.max(run.0).min(bottom), bottom);
            // (Sur le bord commun de deux bandes : le plus large des deux.)
            if best.is_none_or(|b| found.1 - found.0 > b.1 - b.0) {
                best = Some(found);
            }
        }
        best
    }

    /// Comme [`Self::span`], pour un point qui n'est couvert nulle part :
    /// l'intervalle le plus proche, où ramener ce qui s'est égaré.
    pub(crate) fn nearest_span(&self, x: f32, top: f32, bottom: f32) -> Option<(f32, f32)> {
        let mut best: Option<(f32, f32, usize, (f32, f32))> = None;
        for (i, s) in self.strips.iter().enumerate() {
            let dx = (s.x0 - x).max(x - s.x1).max(0.0);
            for &run in self.runs_of(i) {
                let dy = (run.0 - bottom).max(bottom - run.1).max(0.0);
                if best.is_none_or(|(bx, by, ..)| (dx, dy) < (bx, by)) {
                    best = Some((dx, dy, i, run));
                }
            }
        }
        let (.., strip, run) = best?;
        let bottom = bottom.clamp(run.0, run.1);
        Some(self.spread(strip, top.clamp(run.0, bottom), bottom))
    }

    /// Le haut de ce qui est couvert à la hauteur `y`, pour un corps large
    /// de `x0` à `x1` : le plus bas des hauts des bandes qu'il occupe (il
    /// ne dépasse d'aucune). `None` si rien n'est couvert là.
    pub(crate) fn ceiling(&self, x0: f32, x1: f32, y: f32) -> Option<f32> {
        let (x0, x1) = (x0.min(x1), x0.max(x1));
        let mut ceiling: Option<f32> = None;
        for (i, s) in self.strips.iter().enumerate() {
            // (Un corps sans largeur compte dans la bande où il est.)
            let apart = if x1 - x0 <= SAME {
                x1 < s.x0 || x0 > s.x1
            } else {
                x1 <= s.x0 + SAME || x0 >= s.x1 - SAME
            };
            if apart {
                continue;
            }
            if let Some(run) = self.run_at(i, y) {
                ceiling = Some(ceiling.map_or(run.0, |c| c.max(run.0)));
            }
        }
        ceiling
    }

    /// Tout le rectangle, de `x0` à `x1` et de `top` à `bottom`, est-il
    /// couvert d'un seul tenant ? (Le chemin d'un animal qui passe d'un
    /// écran à celui du dessus, ou du dessous.)
    pub(crate) fn holds(&self, x0: f32, x1: f32, top: f32, bottom: f32) -> bool {
        let (x0, x1) = (x0.min(x1), x0.max(x1));
        let mut reached: Option<f32> = None;
        for (i, s) in self.strips.iter().enumerate() {
            if s.x1 <= x0 + SAME || s.x0 >= x1 - SAME {
                continue;
            }
            if s.x0 > reached.unwrap_or(x0) + TOUCH || !self.covers(i, top, bottom) {
                return false;
            }
            reached = Some(s.x1);
        }
        reached.is_some_and(|x| x >= x1 - TOUCH)
    }

    /// Le point est-il couvert ?
    pub(crate) fn contains(&self, p: Vec2) -> bool {
        self.strips
            .iter()
            .enumerate()
            .any(|(i, s)| p.x >= s.x0 - SAME && p.x <= s.x1 + SAME && self.run_at(i, p.y).is_some())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn strips(rects: &[Rect]) -> Strips {
        let mut strips = Strips::default();
        strips.rebuild(rects.iter().copied());
        strips
    }

    #[test]
    fn side_by_side_screens_of_the_same_height_make_one_rectangle() {
        let s = strips(&[
            Rect::new(0.0, 0.0, 1440.0, 900.0),
            Rect::new(1440.0, 0.0, 1920.0, 900.0),
        ]);
        assert!(s.is_one_rect());
        assert_eq!(s.span(100.0, 700.0, 800.0), Some((0.0, 3360.0)));
        assert_eq!(s.ceiling(90.0, 110.0, 500.0), Some(0.0));
        // Seul : pareil.
        let one = strips(&[Rect::new(0.0, 0.0, 1440.0, 900.0)]);
        assert!(one.is_one_rect());
        assert_eq!(one.span(100.0, 700.0, 800.0), Some((0.0, 1440.0)));
        // Rien : rien.
        let none = strips(&[]);
        assert!(!none.is_one_rect());
        assert_eq!(none.span(0.0, 0.0, 1.0), None);
        assert_eq!(none.nearest_span(0.0, 0.0, 1.0), None);
        assert_eq!(none.ceiling(0.0, 1.0, 0.0), None);
    }

    #[test]
    fn a_rectangle_is_held_only_where_screens_cover_all_of_it() {
        // Un écran, un autre plus étroit posé dessus, un troisième à droite,
        // plus bas.
        let s = strips(&[
            Rect::new(0.0, 0.0, 1000.0, 600.0),
            Rect::new(200.0, -500.0, 600.0, 500.0),
            Rect::new(1000.0, 300.0, 800.0, 600.0),
        ]);
        // Sous l'écran du dessus, d'un écran à l'autre.
        assert!(s.holds(300.0, 700.0, -400.0, 550.0));
        // À cheval sur son bord : rien au-dessus, à gauche de lui.
        assert!(!s.holds(150.0, 300.0, -400.0, 550.0));
        assert!(s.holds(50.0, 150.0, 100.0, 550.0));
        // Vers l'écran de droite, aux hauteurs que les deux couvrent.
        assert!(s.holds(900.0, 1100.0, 350.0, 550.0));
        assert!(!s.holds(900.0, 1100.0, 100.0, 550.0));
        // Ce qui dépasse de tous les écrans.
        assert!(!s.holds(1700.0, 1900.0, 350.0, 550.0));
        assert!(!s.holds(-50.0, 50.0, 100.0, 200.0));
        // Un trou entre deux écrans.
        let apart = strips(&[
            Rect::new(0.0, 0.0, 1000.0, 700.0),
            Rect::new(1100.0, 0.0, 1000.0, 700.0),
        ]);
        assert!(!apart.holds(900.0, 1200.0, 100.0, 200.0));
        assert!(!strips(&[]).holds(0.0, 1.0, 0.0, 1.0));
    }

    /// Un petit écran à gauche, un grand à droite qui monte plus haut et
    /// descend plus bas.
    fn small_and_tall() -> Strips {
        strips(&[
            Rect::new(0.0, 0.0, 1000.0, 600.0),
            Rect::new(1000.0, -300.0, 1600.0, 1200.0),
        ])
    }

    #[test]
    fn a_body_goes_sideways_only_where_its_whole_height_is_on_a_screen() {
        let s = small_and_tall();
        assert!(!s.is_one_rect());
        // À une hauteur que les deux écrans couvrent : d'un bout à l'autre.
        assert_eq!(s.span(500.0, 400.0, 500.0), Some((0.0, 2600.0)));
        assert_eq!(s.span(1500.0, 400.0, 500.0), Some((0.0, 2600.0)));
        // Plus bas que le petit, ou plus haut : le grand seulement.
        assert_eq!(s.span(1500.0, 700.0, 800.0), Some((1000.0, 2600.0)));
        assert_eq!(s.span(1500.0, -200.0, -100.0), Some((1000.0, 2600.0)));
        // À cheval sur le bas du petit écran : ses pieds n'y tiendraient pas.
        assert_eq!(s.span(1500.0, 560.0, 640.0), Some((1000.0, 2600.0)));
        // La tête passe déjà le haut de son écran : elle ne compte pas.
        assert_eq!(s.span(500.0, -50.0, 30.0), Some((0.0, 2600.0)));
        // Hors de tout écran : rien ; le plus proche, pour y revenir.
        assert_eq!(s.span(500.0, 700.0, 800.0), None);
        assert_eq!(s.nearest_span(500.0, 700.0, 800.0), Some((0.0, 2600.0)));
        assert_eq!(s.nearest_span(-80.0, 100.0, 200.0), Some((0.0, 2600.0)));
    }

    #[test]
    fn the_ceiling_is_the_lowest_top_under_a_body() {
        let s = small_and_tall();
        assert_eq!(s.ceiling(400.0, 460.0, 300.0), Some(0.0));
        assert_eq!(s.ceiling(1400.0, 1460.0, 300.0), Some(-300.0));
        // À cheval sur les deux : le haut du plus bas des deux.
        assert_eq!(s.ceiling(970.0, 1030.0, 300.0), Some(0.0));
        // Juste au bord, sans y entrer.
        assert_eq!(s.ceiling(1000.0, 1060.0, 300.0), Some(-300.0));
        assert_eq!(s.ceiling(940.0, 1000.0, 300.0), Some(0.0));
        // Au-dessus du petit écran, il n'y a rien.
        assert_eq!(s.ceiling(400.0, 460.0, -100.0), None);
    }

    #[test]
    fn stacked_screens_join_from_top_to_bottom() {
        // Un écran au-dessus de l'autre, plus étroit et décalé.
        let s = strips(&[
            Rect::new(0.0, 0.0, 1500.0, 900.0),
            Rect::new(200.0, -1000.0, 1200.0, 1000.0),
        ]);
        // Sous l'écran du haut : le plafond est tout en haut.
        assert_eq!(s.ceiling(600.0, 660.0, 500.0), Some(-1000.0));
        // À côté : celui de l'écran du bas.
        assert_eq!(s.ceiling(60.0, 120.0, 500.0), Some(0.0));
        // En haut, on ne sort pas de l'écran du haut par les côtés.
        assert_eq!(s.span(600.0, -500.0, -400.0), Some((200.0, 1400.0)));
        // En bas, toute la largeur.
        assert_eq!(s.span(600.0, 400.0, 500.0), Some((0.0, 1500.0)));
        assert!(s.contains(Vec2::new(300.0, -500.0)));
        assert!(!s.contains(Vec2::new(100.0, -500.0)));
    }

    #[test]
    fn a_gap_between_two_screens_is_a_wall_and_a_hair_is_not() {
        let apart = strips(&[
            Rect::new(0.0, 0.0, 1000.0, 600.0),
            Rect::new(1040.0, 0.0, 1000.0, 600.0),
        ]);
        assert_eq!(apart.span(500.0, 400.0, 500.0), Some((0.0, 1000.0)));
        assert_eq!(apart.span(1500.0, 400.0, 500.0), Some((1040.0, 2040.0)));
        assert!(!apart.is_one_rect());
        // Un arrondi d'un demi-point entre deux écrans voisins.
        let close = strips(&[
            Rect::new(0.0, 0.0, 1000.0, 600.0),
            Rect::new(1000.5, 0.0, 1000.0, 600.0),
        ]);
        assert_eq!(close.span(500.0, 400.0, 500.0), Some((0.0, 2000.5)));
        assert!(close.is_one_rect());
    }

    #[test]
    fn mirrored_and_overlapping_screens_count_once() {
        let s = strips(&[
            Rect::new(0.0, 0.0, 1000.0, 600.0),
            Rect::new(0.0, 0.0, 1000.0, 600.0),
            Rect::new(500.0, 100.0, 1000.0, 600.0),
        ]);
        assert_eq!(s.span(200.0, 300.0, 400.0), Some((0.0, 1500.0)));
        assert_eq!(s.span(200.0, 20.0, 80.0), Some((0.0, 1000.0)));
        assert_eq!(s.ceiling(700.0, 760.0, 300.0), Some(0.0));
        assert_eq!(s.ceiling(1200.0, 1260.0, 300.0), Some(100.0));
    }

    #[test]
    fn rectangles_without_a_surface_or_a_number_are_left_out() {
        let s = strips(&[
            Rect::new(0.0, 0.0, 1000.0, 600.0),
            Rect::new(1000.0, 0.0, 0.0, 600.0),
            Rect::new(f32::NAN, 0.0, 100.0, 600.0),
            Rect::new(2000.0, 0.0, f32::INFINITY, 600.0),
        ]);
        assert!(s.is_one_rect());
        assert_eq!(s.span(500.0, 400.0, 500.0), Some((0.0, 1000.0)));
    }
}
