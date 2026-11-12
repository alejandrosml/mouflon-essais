//! Simulation d'un animal : physique à pas fixe, comportements et signaux
//! d'animation procédurale (écrasement, démarche, regard, respiration…).
//!
//! Tout est déterministe pour une graine donnée : la même scène rejoue
//! exactement le même mouvement, ce qui rend les tests et l'aperçu fiables.

use std::f32::consts::TAU;
use std::sync::Arc;

use crate::def::{
    Appearance, Condition, Duration, Gesture, Motion, PetDef, Pose, Rules, Sex, SpawnPoint,
    SpriteClip, Transition, Variant,
};
use crate::expr::Expr;
use crate::math::{
    Rect, Rng, Spring, Vec2, approach, clamp_between, damp, lerp, smoothstep, wrap_phase,
};
use crate::physics::{Back, Tracker};
use crate::temper::{Mood, Temper};
use crate::world::{Surface, SurfaceId, SurfaceKind, World};
pub use legs::LegCycle;

mod family;
mod leap;
mod legs;
mod near;
mod real;
mod script;
mod social;
mod voyage;

pub use voyage::{VOYAGE_EVERY, Way as VoyageWay};

/// Pas de simulation fixe : 120 Hz, la fréquence des écrans ProMotion.
pub const TICK: f32 = 1.0 / 120.0;
/// Temps que met le dessin à rejoindre son corps replacé d'un coup (voir
/// `Pet::smooth`) : il en a fait les deux tiers en ce temps (secondes).
const SMOOTH_TIME: f32 = 0.035;
/// Vitesse à laquelle le dessin rejoint au plus son corps (points par
/// seconde) : ramené de loin (un animal qui dépassait du bord de l'écran,
/// heurté), il y revient d'un mouvement qu'on suit, pas d'un éclair.
const SMOOTH_SPEED: f32 = 1000.0;
/// Deux corps qui se chevauchaient de plus que ceci (fois la hauteur du
/// plus petit) au début du pas passent l'un devant l'autre (voir
/// `overlapped_before`). En deçà, c'est un contact qui dure (l'un appuyé
/// sur l'autre) : on les sépare.
const OVERLAP_BEFORE: f32 = 0.15;

/// Un clignement (secondes) : la paupière se ferme vite, reste close un
/// instant, se rouvre deux fois plus lentement (voir [`lid`]).
const BLINK_TIME: f32 = 0.27;
/// Cadence qui suffit à un animal qui va son petit train (images par
/// seconde) : à moins de [`SLOW_RATE`] points par seconde, sa fenêtre avance
/// d'un point au plus d'une image à l'autre.
pub const SLOW_RATE: f32 = 60.0;
/// Allongement de la foulée au galop (la foulée de course vaut 1 + ceci fois
/// celle du pas) : au galop, un mouton garde à peu près la même cadence
/// (2,3 à 2,7 foulées par seconde) et allonge ses bonds.
pub const RUN_STRIDE: f32 = 0.9;
/// Allongement de la foulée au trot (voir [`RUN_STRIDE`]) : un mouton trotte
/// à petits pas rapides (2 à 2,4 foulées par seconde), à peine plus longs
/// qu'au pas.
pub const TROT_STRIDE: f32 = 0.15;
/// Foulée du pas, en longueurs de patte (voir `Physics::stride`).
const STRIDE_PER_LEG: f32 = 1.6;
/// Au départ, la foulée ne fait que cette part de celle de l'allure (elle
/// s'allonge sur la première foulée).
const START_STRIDE: f32 = 0.5;
/// Ouverture de la bouche pendant un son de sa voix (secondes).
const VOICE_TIME: f32 = 0.75;
/// Mâchonnements par seconde : en broutant (1,3 à 1,7 par bouchée, une
/// bouchée par seconde) et en ruminant (83 à 99 par minute) ; voir
/// `docs/animation-animale.md`.
const CHEW_RATE: f32 = 1.6;
/// Avant de sauter, un animal dessiné se ramasse ce temps-là (secondes),
/// écrasé de tant : l'anticipation, qui annonce le saut.
const TAKEOFF_CROUCH: f32 = 0.12;
const TAKEOFF_SQUASH: f32 = 0.14;
/// Pour un saut sur un rebord (voir `pet/leap.rs`) : ramassé plus bas,
/// l'arrière plus bas que l'avant (inclinaison négative : le nez se lève) ;
/// en l'air, le corps suit son élan à cette part de l'angle de sa course ;
/// posé, il pique du nez sur ses antérieurs.
const LEAP_SQUASH: f32 = 0.16;
const LEAP_GATHER_LEAN: f32 = -0.28;
const LEAP_PITCH: f32 = 0.5;
const LEAP_LANDING_LEAN: f32 = 0.22;
/// Un regroupement du troupeau revient de temps en temps (secondes) ; un
/// mouton qui y prend part s'arrête à cette distance de sa place (fois sa
/// largeur), en ordre serré.
const REGROUP_EVERY: (f32, f32) = (300.0, 720.0);
const REGROUP_SPACING: f32 = 0.8;
/// Un agneau a envie d'en inviter un autre à jouer toutes les 20 à 60 s ;
/// l'autre répond au bout de ce temps (secondes).
const INVITE_EVERY: (f32, f32) = (20.0, 60.0);
const INVITE_ANSWER: f32 = 1.0;
/// Il invite un voisin de sa famille à cette distance (fois sa largeur).
const INVITE_REACH: (f32, f32) = (1.2, 3.0);
/// En broutant : une station d'herbe (secondes), puis un pas en avant qui
/// dure ceci, à cette vitesse (en foulées par seconde).
const GRAZE_STATION: f32 = 8.5;
const GRAZE_STEP_TIME: f32 = 1.0;
const GRAZE_STEP_SPEED: f32 = 0.4;
/// Rumination : chaque bol est mâché un moment, puis avalé ; une courte
/// pause, et le suivant remonte (secondes ; chez le mouton, une minute de
/// mastication et 10 à 15 s de pause, raccourcies ici).
const BOLUS_CHEW: (f32, f32) = (9.0, 16.0);
const BOLUS_PAUSE: (f32, f32) = (1.6, 3.0);
/// La toison : les mèches sont des ressorts amortis (Hz), plus lents que la
/// queue ; elles traînent de tant de radians par point par seconde de vitesse,
/// penchent de tant par point par seconde² d'accélération du corps (physique
/// réaliste), et de tant au plus fort d'un coup de vent.
const FLEECE_HZ: f32 = 2.0;
const FLEECE_TIP_HZ: f32 = 1.6;
const FLEECE_DRAG: f32 = 0.0016;
const FLEECE_SURGE: f32 = 0.0003;
const FLEECE_WIND: f32 = 0.3;
/// L'horloge d'un animal (voir [`PoseSignals::clock`]) repart de zéro toutes
/// les heures : elle garde sa précision.
pub const CLOCK_LOOP: f32 = 3600.0;
const MAX_PARTICLES: usize = 64;

/// Une touche frappée il y a moins de ceci (secondes) : l'utilisateur tape.
pub const TYPING_WINDOW: f32 = 3.0;
/// Rien touché depuis ceci (secondes) : l'utilisateur est parti.
pub const AWAY_AFTER: f32 = 180.0;
/// Un pointeur plus rapide que ceci (points par seconde) qui frôle un animal
/// le fait sursauter.
pub const STARTLE_SPEED: f32 = 1400.0;
/// Pause entre deux sursauts d'un même animal (secondes).
const STARTLE_COOLDOWN: f32 = 8.0;
/// Parti, l'utilisateur laisse les animaux s'endormir l'un après l'autre,
/// dans ce délai (secondes) ; réveillé avant la fin, l'un se rendort au
/// bout d'un moment.
const NAP_SPREAD: f32 = 40.0;

/// Ce que l'animal perçoit de l'extérieur à chaque pas.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct Input {
    /// Curseur dans le repère du monde.
    pub cursor: Option<Vec2>,
    /// Heure locale décimale (0–24).
    pub local_hour: f32,
    /// Secondes depuis la dernière touche frappée (laquelle, Mouflon ne le
    /// sait pas) ; `None` : inconnu.
    pub since_key: Option<f32>,
    /// Secondes depuis le dernier geste de l'utilisateur (clavier, souris,
    /// trackpad) ; `None` : inconnu.
    pub idle: Option<f32>,
}

impl Input {
    /// L'utilisateur tape-t-il au clavier ?
    #[must_use]
    pub fn typing(&self) -> bool {
        self.since_key.is_some_and(|s| s < TYPING_WINDOW)
    }

    /// L'utilisateur est-il parti ?
    #[must_use]
    pub fn away(&self) -> bool {
        self.idle.is_some_and(|s| s >= AWAY_AFTER)
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Support {
    Ground { surface: SurfaceId },
    Air,
    Held,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub enum PetEvent {
    Landed {
        impact: f32,
    },
    Poked,
    Thrown {
        speed: f32,
    },
    /// Heurté par un autre animal (mode collisions).
    Bumped {
        speed: f32,
    },
    /// A rencontré un autre animal et réagit.
    Met {
        other: u32,
    },
    /// Fait entendre un son : rang dans [`PetDef::sounds`] et prise choisie.
    Sound {
        sound: usize,
        clip: usize,
    },
    /// Le pointeur l'a frôlé à toute vitesse : il sursaute.
    Startled,
    /// L'utilisateur est parti : il s'endort.
    Napping,
    /// L'utilisateur est revenu : il se réveille.
    Woken,
    /// Lancé, il a percuté une soucoupe volante.
    HitSaucer,
    /// Appel grave de la brebis à son agneau : sa bouche reste fermée.
    MaternalCall,
}

/// Pause entre deux collisions d'un même animal (secondes) : le temps de se
/// relever avant qu'un autre ne puisse le renverser de nouveau.
const BUMP_COOLDOWN: f32 = 3.5;

/// Pause entre deux rencontres d'un même animal (secondes).
const MEET_COOLDOWN: f32 = 2.5;

/// Un voisin imite un comportement contagieux s'il est à moins de ceci
/// (fois la largeur de l'animal), à la même hauteur.
const ECHO_REACH: f32 = 4.0;
/// Délai avant d'imiter (secondes) : un mouton suit celui qui part avant
/// lui dans les 12 secondes (95 % des cas) ; plus vite ici.
const ECHO_DELAY: (f32, f32) = (0.4, 5.0);
/// Après avoir imité, il n'imite plus pendant ceci (secondes) : pas de
/// va-et-vient sans fin entre deux voisins.
const ECHO_COOLDOWN: f32 = 25.0;
/// Un animal occupé (un animal en images au milieu de sa séquence) imite à
/// la fin de ce qu'il fait, s'il la termine dans ce délai (secondes).
const ECHO_PATIENCE: f32 = 8.0;

/// Ce qu'un animal imite d'un voisin : un comportement (rang dans sa propre
/// définition), quand, et dans quel sens (on suit celui qui part).
#[derive(Clone, Copy, Debug)]
struct Echo {
    behavior: usize,
    delay: f32,
    facing: f32,
    /// Temps qui reste pour l'imiter (voir [`ECHO_PATIENCE`]).
    patience: f32,
}

/// Durée d'affichage d'une image intermédiaire, au plus (secondes).
const INBETWEEN_TIME: f32 = 0.08;
/// Pas d'une même image regardés, au plus, pour répartir un passage.
const MAX_HELD_STEPS: usize = 32;
/// Élan gardé au plus après un atterrissage (points par seconde) : lancé de
/// loin, il glisse de quelques dizaines de points, pas d'un bout à l'autre.
const MAX_SLIDE: f32 = 260.0;
/// Lissage de la vitesse d'un animal tenu (secondes) : l'élan qu'il garde au
/// lâcher est celui des derniers centièmes de seconde du geste.
const THROW_SMOOTHING: f32 = 0.06;
/// Au revoir : durée du fondu, et durée au-delà de laquelle l'animation
/// d'adieu est écourtée.
const FADE_TIME: f32 = 1.2;
const LEAVE_MAX: f32 = 6.0;

/// Un animal qui tombe se pose sur une surface dont le bord est à moins de
/// cette fraction de sa largeur (ses pattes la touchent encore).
const LANDING_REACH: f32 = 0.15;
/// Hauteur de son dos, debout, sans image pour le dire : cette part de sa
/// hauteur.
const BACK_HEIGHT: f32 = 0.92;
/// Couché (dessin animé par le moteur), son dos descend de cette part de sa
/// hauteur : ses pattes repliées sous lui.
const LYING_DROP: f32 = 0.3;
/// Vitesse à laquelle son dos suit le haut de son dessin (pt/s, pour un
/// animal de 60 points).
const BACK_RATE: f32 = 420.0;

/// Besoin de rafraîchissement, pour adapter la cadence d'affichage.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub enum Activity {
    Asleep,
    Idle,
    Moving,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ParticleKind {
    /// Poussière soulevée à l'atterrissage.
    Dust,
    /// Bulle de sommeil.
    Bubble,
    /// Feuille emportée par un coup de vent.
    Leaf,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Particle {
    pub kind: ParticleKind,
    /// Position dans le repère du monde.
    pub pos: Vec2,
    pub vel: Vec2,
    pub age: f32,
    pub life: f32,
    pub size: f32,
    pub phase: f32,
}

impl Particle {
    #[must_use]
    pub fn progress(&self) -> f32 {
        (self.age / self.life).clamp(0.0, 1.0)
    }
}

/// Signaux d'animation, tous lissés : le rendu les traduit en transformations.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct PoseSignals {
    /// Phase du cycle de marche (radians).
    pub gait_phase: f32,
    /// Poids de la marche et de la course (0–1).
    pub walk: f32,
    pub run: f32,
    /// Poids des poses (0–1).
    pub sit: f32,
    pub sleep: f32,
    pub dangle: f32,
    pub air: f32,
    /// > 0 : écrasé verticalement ; < 0 : étiré.
    pub squash: f32,
    /// Inclinaison du corps (radians, positif vers l'avant).
    pub lean: f32,
    /// Battement des oreilles (radians).
    pub ear: f32,
    /// Queue (radians).
    pub tail: f32,
    /// Une queue en plusieurs morceaux : de combien le deuxième, puis le
    /// dernier, tournent par rapport au précédent (radians). Chacun suit le
    /// précédent avec un temps de retard : la queue ondule.
    pub tail_mid: f32,
    pub tail_tip: f32,
    /// Paupières : 0 ouvertes, 1 fermées.
    pub blink: f32,
    /// Respiration (-1 à 1).
    pub breath: f32,
    /// Regard dans le repère de l'animal (x vers l'avant, y vers le bas), norme ≤ 1.
    pub look: Vec2,
    /// Balancement quand on le tient (radians, repère de l'écran : positif,
    /// le corps part vers la gauche).
    pub swing: f32,
    /// Culbute (radians, repère de l'écran, sens horaire) : l'animal heurté
    /// roule sur lui-même autour du milieu de son corps.
    pub roll: f32,
    /// Broute (0–1) : la tête descend jusqu'au sol.
    pub graze: f32,
    /// Coups de dents en broutant (-1 à 1).
    pub nibble: f32,
    /// En l'air : de -1 (il tombe vite) à 1 (il monte vite), pour plier ou
    /// tendre les pattes.
    pub rise: f32,
    /// Horloge propre à l'animal (secondes, en boucle sur [`CLOCK_LOOP`]) :
    /// elle fait tourner les animations qui ne suivent pas ses pas.
    pub clock: f32,
    /// Trot (0–1) : entre le pas et le galop, les pattes vont par paires
    /// diagonales.
    pub trot: f32,
    /// Longueur de la foulée en cours, en fois celle du pas (0 : d'après
    /// `run` seulement, `1 + RUN_STRIDE × run`).
    pub stride_scale: f32,
    /// Le cycle propre de chaque patte, dans l'ordre : antérieur de ce
    /// côté, antérieur de l'autre, postérieur de ce côté, postérieur de
    /// l'autre (voir [`LegCycle`] ; par défaut, aucun : le rendu prend la
    /// phase commune et les décalages de l'allure).
    pub legs: [LegCycle; 4],
    /// Bouche : 0 fermée, 1 grande ouverte (il mâche, bêle, bâille).
    pub jaw: f32,
    /// Le geste en cours, son ampleur (0–1, elle monte et retombe en
    /// douceur) et depuis quand il dure (secondes).
    pub gesture: Option<Gesture>,
    pub gesture_amount: f32,
    pub gesture_time: f32,
    /// Cible du museau en tétée : fractions de hauteur depuis le point
    /// d'appui, x vers la tête, y vers le bas.
    pub nursing_target: Option<Vec2>,
    /// Oreilles : > 0 couchées en arrière (énervé, il fonce), < 0 dressées
    /// vers l'avant (aux aguets).
    pub ears_back: f32,
    /// Tête basse (0–1) : il fonce ; > 0 un peu, énervé ou calme.
    pub head_low: f32,
    /// Pattes raides en l'air (0–1) : un saut d'agneau.
    pub stiff: f32,
    /// Oreilles passives, pendantes (0–1) : au calme, en mangeant.
    pub ears_droop: f32,
    /// Oreilles asymétriques (radians, l'une monte, l'autre descend) : la
    /// surprise.
    pub ear_split: f32,
    /// Saut tordu d'un agneau (0–1) : le corps se tord, les pattes ruent.
    pub twist: f32,
    /// Endormi la tête posée au sol, le cou tendu (0–1).
    pub sleep_low: f32,
    /// Une patte qui tressaille dans le sommeil (0–1, retombe aussitôt).
    pub twitch: f32,
    /// La toison qui bouge (radians, positif : le bas des mèches part vers
    /// l'arrière) : elle traîne quand il court, balance au rythme des pas,
    /// penche quand le corps freine ou démarre (physique réaliste) et dans le
    /// vent. Les mèches du bout (`fleece_tip`, par rapport aux premières)
    /// suivent avec un temps de retard.
    pub fleece: f32,
    pub fleece_tip: f32,
    /// Un saut sur un rebord (0–1) : en l'air, les pattes se replient
    /// serrées puis les antérieurs se tendent vers le rebord ; posé, les
    /// sabots restent plantés là où ils touchent (ils ne s'écartent pas en
    /// glissant), ce sont les genoux qui amortissent.
    pub leap: f32,
    /// En l'air, où il en est de ce saut : 0 à l'envol, 1 à la réception.
    pub leap_flight: f32,
}

impl PoseSignals {
    /// Le balancement est donné dans le repère de l'écran ; dans celui d'un
    /// dessin retourné, il change de sens.
    #[must_use]
    pub fn local_swing(&self, facing: f32) -> f32 {
        if facing < 0.0 {
            -self.swing
        } else {
            self.swing
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Shadow {
    /// Hauteur de la surface sous l'animal.
    pub y: f32,
    /// Distance entre les pattes et cette surface.
    pub distance: f32,
    /// Bouts de cette surface (le rebord d'une fenêtre, le dos d'un autre) :
    /// l'ombre ne déborde pas dans le vide.
    pub x0: f32,
    pub x1: f32,
}

impl Shadow {
    /// L'ombre sur une surface sans bouts (le sol d'un écran large).
    #[must_use]
    pub fn on(y: f32, distance: f32) -> Self {
        Self {
            y,
            distance,
            x0: f32::NEG_INFINITY,
            x1: f32::INFINITY,
        }
    }
}

/// Ce que le rendu a besoin de savoir pour dessiner une image.
#[derive(Clone, Debug)]
pub struct PetView<'a> {
    pub id: u32,
    /// Point d'appui (milieu des pattes), interpolé entre deux pas.
    pub ground: Vec2,
    /// Orientation lissée : 1 vers la droite, -1 vers la gauche, entre les deux pendant un demi-tour.
    pub facing: f32,
    pub pose: PoseSignals,
    pub shadow: Option<Shadow>,
    pub sprite_frame: Option<u32>,
    pub sprite_offset_y: f32,
    pub opacity: f32,
    pub particles: &'a [Particle],
    pub support: Support,
    pub behavior: &'a str,
    /// Tenu : le point par lequel le curseur le tient (il se balance autour).
    pub grip: Option<Vec2>,
}

#[derive(Clone, Copy, Debug)]
struct Hold {
    target: Vec2,
    offset: Vec2,
}

#[derive(Clone, Copy, Debug)]
struct GroundRef {
    surface: SurfaceId,
    x0: f32,
    x1: f32,
}

#[derive(Clone, Debug)]
struct Anim {
    flip: f32,
    gait_phase: f32,
    walk: f32,
    run: f32,
    sit: f32,
    sleep: f32,
    dangle: f32,
    air: f32,
    squash: Spring,
    lean: Spring,
    ear: Spring,
    swing: Spring,
    tail_phase: f32,
    blink: f32,
    blink_timer: f32,
    blink_left: f32,
    double_blink: bool,
    breath_phase: f32,
    /// Horloge propre à l'animal (secondes, en boucle sur une heure).
    clock: f32,
    look: Vec2,
    glance: Vec2,
    glance_timer: f32,
    bubble_timer: f32,
    /// Prochaine bouffée de poussière quand il court.
    dust_timer: f32,
    /// Prochaine feuille portée par le vent.
    leaf_timer: f32,
    /// Culbute : angle courant, angle qu'il reste à parcourir (signé) et vitesse (rad/s).
    roll: f32,
    roll_left: f32,
    roll_rate: f32,
    graze: f32,
    nibble_phase: f32,
    rise: f32,
    /// Prochain frémissement d'oreille, prochain coup de queue (secondes).
    ear_timer: f32,
    tail: Spring,
    /// Le milieu et le bout de la queue (angles absolus, comme `tail`).
    tail_mid: Spring,
    tail_tip: Spring,
    tail_timer: f32,
    trot: f32,
    stride_scale: f32,
    /// Le cycle propre de chaque patte (voir [`LegCycle`]), et ce qu'il a
    /// parcouru depuis qu'il est reparti (en foulées du pas, jusqu'à 1).
    legs: [LegCycle; 4],
    start: f32,
    /// Le geste en cours, son ampleur et depuis quand il dure.
    gesture: Option<Gesture>,
    gesture_amount: f32,
    gesture_time: f32,
    /// Bouche : ouverture, phase des mâchonnements, voix qui reste.
    jaw: f32,
    chew_phase: f32,
    voice_left: f32,
    /// Posture : oreilles couchées, tête basse, pattes raides, oreilles
    /// pendantes, oreilles asymétriques.
    ears_back: f32,
    head_low: f32,
    stiff: f32,
    ears_droop: f32,
    ear_split: f32,
    /// Rumination : temps qui reste à mâcher le bol (négatif : la pause
    /// avant le suivant).
    bolus_left: f32,
    /// Saut tordu, tête au sol en dormant (et ce qu'il choisit pour ce
    /// sommeil), patte qui tressaille.
    twist: f32,
    sleep_low: f32,
    sleep_low_target: f32,
    twitch: f32,
    /// La toison : les premières mèches, puis celles du bout.
    fleece: Spring,
    fleece_tip: Spring,
    /// Le saut sur un rebord (0–1, voir [`PoseSignals::leap`]) ; posé, le
    /// temps qui reste de sa réception, sabots plantés.
    leap: f32,
    leap_after: f32,
    leap_flight: f32,
}

impl Anim {
    fn new(facing: f32, rng: &mut Rng) -> Self {
        Self {
            flip: facing,
            gait_phase: 0.0,
            walk: 0.0,
            run: 0.0,
            sit: 0.0,
            sleep: 0.0,
            dangle: 0.0,
            air: 1.0,
            squash: Spring::new(2.6, 0.32),
            lean: Spring::new(2.2, 0.55),
            ear: Spring::new(2.4, 0.22),
            swing: Spring::new(1.3, 0.18),
            graze: 0.0,
            nibble_phase: 0.0,
            rise: 0.0,
            // Sans tirage au hasard : les animaux en images, qui n'en ont pas
            // l'usage, gardent exactement leurs tirages.
            ear_timer: 3.5,
            tail: Spring::new(3.2, 0.2),
            tail_mid: Spring::new(2.2, 0.3),
            tail_tip: Spring::new(1.8, 0.3),
            tail_timer: 5.0,
            tail_phase: rng.range(0.0, TAU),
            blink: 0.0,
            blink_timer: rng.range(1.0, 4.0),
            blink_left: 0.0,
            double_blink: false,
            breath_phase: rng.range(0.0, TAU),
            // Sans tirage de plus, voir `with_clock`.
            clock: 0.0,
            look: Vec2::new(0.3, 0.0),
            glance: Vec2::new(0.3, 0.0),
            glance_timer: 0.0,
            bubble_timer: 1.0,
            dust_timer: 0.0,
            leaf_timer: 0.0,
            roll: 0.0,
            roll_left: 0.0,
            roll_rate: 0.0,
            trot: 0.0,
            stride_scale: 1.0,
            legs: [LegCycle::default(); 4],
            start: 0.0,
            gesture: None,
            gesture_amount: 0.0,
            gesture_time: 0.0,
            jaw: 0.0,
            chew_phase: 0.0,
            voice_left: 0.0,
            ears_back: 0.0,
            head_low: 0.0,
            stiff: 0.0,
            ears_droop: 0.0,
            ear_split: 0.0,
            bolus_left: 12.0,
            twist: 0.0,
            sleep_low: 0.0,
            sleep_low_target: 0.0,
            twitch: 0.0,
            fleece: Spring::new(FLEECE_HZ, 0.28),
            fleece_tip: Spring::new(FLEECE_TIP_HZ, 0.32),
            leap: 0.0,
            leap_after: 0.0,
            leap_flight: 0.0,
        }
        .with_clock()
    }

    /// L'horloge part d'un instant propre à chaque animal (de 0 à une
    /// minute), tiré de sa respiration : pas de tirage au hasard de plus, les
    /// animaux gardent exactement leurs tirages.
    fn with_clock(mut self) -> Self {
        self.clock = self.breath_phase / TAU * 60.0;
        self
    }
}

/// Un geste (voir [`Gesture`]) : il démarre avec son comportement, monte en
/// 0,12 s et retombe pendant les 0,18 dernières secondes ; interrompu, il
/// s'éteint vite. `since` et `left` : temps écoulé et restant du comportement.
/// Arrêté au milieu d'un pas (`stepping` : une patte en l'air), il finit son
/// pas avant un geste qui se fait d'aplomb (voir [`Gesture::needs_footing`]) :
/// il ne lève pas un sabot pour gratter le sol pendant que l'autre est
/// encore en l'air.
fn step_gesture(
    a: &mut Anim,
    doing: Option<Gesture>,
    since: f32,
    left: f32,
    stepping: bool,
    dt: f32,
) {
    // Un nouveau comportement (le même geste encore, ou un autre) repart de zéro.
    if doing.is_some() && (doing != a.gesture || since < a.gesture_time) {
        a.gesture = doing;
        a.gesture_time = 0.0;
    }
    let Some(gesture) = a.gesture else {
        return;
    };
    if a.gesture_time > 0.0 || !stepping || !gesture.needs_footing() || doing != Some(gesture) {
        a.gesture_time += dt;
    }
    let target = if doing == Some(gesture) {
        smoothstep(0.0, 0.12, a.gesture_time) * smoothstep(0.0, 0.18, left.max(0.0))
    } else {
        0.0
    };
    a.gesture_amount = if target >= a.gesture_amount {
        target
    } else {
        damp(a.gesture_amount, target, 14.0, dt)
    };
    if doing.is_none() && a.gesture_amount < 0.005 {
        a.gesture = None;
        a.gesture_amount = 0.0;
    }
}

/// La bouche : il mâchonne en broutant et en ruminant, l'ouvre en faisant
/// entendre sa voix, en bêlant et en bâillant.
fn step_mouth(a: &mut Anim, chewing: bool, rng: &mut Rng, dt: f32) {
    let grazing = a.graze > 0.3;
    // Il rumine bol après bol : il mâche, avale, une pause, le suivant.
    let mut pausing = false;
    if chewing && !grazing {
        if a.bolus_left > 0.0 {
            a.bolus_left -= dt;
            if a.bolus_left <= 0.0 {
                // Avalé : une pause, puis le bol suivant remonte.
                a.bolus_left = -rng.range(BOLUS_PAUSE.0, BOLUS_PAUSE.1);
            }
        } else {
            a.bolus_left += dt;
            if a.bolus_left >= 0.0 {
                a.bolus_left = rng.range(BOLUS_CHEW.0, BOLUS_CHEW.1);
            }
        }
        pausing = a.bolus_left < 0.0;
    }
    let chewing = chewing && !pausing;
    if grazing || chewing {
        // Un peu irrégulier : la mâchoire d'un ruminant ne bat pas comme un métronome.
        let rate = CHEW_RATE * (1.0 + 0.12 * (a.chew_phase * 0.31).sin());
        a.chew_phase = wrap_phase(a.chew_phase + TAU * rate * dt, TAU * 16.0);
    }
    let chew = if grazing || chewing {
        0.22 * (0.5 - 0.5 * a.chew_phase.cos())
    } else {
        0.0
    };
    a.voice_left = (a.voice_left - dt).max(0.0);
    let voice = if a.voice_left > 0.0 {
        // Ouverte d'un coup, un léger vibrato, puis refermée.
        let t = 1.0 - a.voice_left / VOICE_TIME;
        let shape = smoothstep(0.0, 0.12, t) * (1.0 - smoothstep(0.7, 1.0, t));
        shape * (0.62 + 0.08 * (t * TAU * 5.5).sin())
    } else {
        0.0
    };
    let gesture = match a.gesture {
        Some(Gesture::Yawn) => {
            // Lente à s'ouvrir, grande ouverte, puis refermée.
            let t = a.gesture_time;
            a.gesture_amount * smoothstep(0.15, 0.7, t)
        }
        Some(Gesture::Bleat) => {
            a.gesture_amount * (0.6 + 0.08 * (a.gesture_time * TAU * 5.5).sin())
        }
        // Le flehmen : la bouche entrouverte, sous la lèvre retroussée.
        Some(Gesture::Flehmen) => a.gesture_amount * 0.14 * smoothstep(0.2, 0.7, a.gesture_time),
        Some(Gesture::MaternalCall) => 0.0,
        Some(Gesture::Nurse) => {
            a.gesture_amount * 0.035 * (0.5 + 0.5 * (a.gesture_time * TAU * 2.0).sin())
        }
        _ => 0.0,
    };
    let target = chew.max(voice).max(gesture);
    a.jaw = if a.gesture == Some(Gesture::MaternalCall) {
        0.0
    } else {
        damp(a.jaw, target, 22.0, dt)
    };
}

/// La paupière au fil d'un clignement (`t` de 0 à 1) : fermée en 0,08 s,
/// close 0,03 s, rouverte en 0,16 s.
fn lid(t: f32) -> f32 {
    if t < 0.3 {
        smoothstep(0.0, 0.3, t)
    } else if t < 0.41 {
        1.0
    } else {
        1.0 - smoothstep(0.41, 1.0, t)
    }
}

#[derive(Clone, Debug)]
pub struct Pet {
    family: family::FamilyState,
    id: u32,
    /// La définition qu'il porte : celle du dessin de son sexe et de son
    /// âge s'il en a un (voir [`PetDef::set_variants`]), sinon `species`.
    def: Arc<PetDef>,
    /// La définition avec laquelle il est apparu, commune à tous ceux de
    /// son espèce (voir [`Pet::species`]).
    species: Arc<PetDef>,
    /// Le dessin qu'il porte, s'il n'est pas celui de base.
    variant: Option<Variant>,
    sex: Sex,
    /// Famille calculée une fois, sans allouer à chaque choix de comportement.
    sheep: bool,
    /// Mémoire consommable d'une brebis flairée pendant une rencontre.
    sniffed_ewe_for: f32,
    rng: Rng,
    /// Tirages des sons, à part : ajouter (ou couper) des sons ne change
    /// rien à ce que fait l'animal.
    voice: Rng,
    pos: Vec2,
    prev_pos: Vec2,
    /// Ce dont son dessin montait au pas précédent (voir [`Pet::tilt_lift`]).
    prev_lift: Vec2,
    vel: Vec2,
    prev_vel_x: f32,
    facing: f32,
    support: Support,
    ground_ref: Option<GroundRef>,
    walking_off: bool,
    hold: Option<Hold>,
    behavior: usize,
    behavior_time: f32,
    behavior_length: f32,
    clip_step: usize,
    clip_timer: f32,
    /// Nombre d'images de la séquence en cours (les répétitions peuvent être tirées au hasard).
    clip_total: usize,
    /// Nombre aléatoire fixe pour toute la vie de l'animal (`randS` de desktopPet).
    rand_s: f32,
    /// Écran sous l'animal, pour les expressions desktopPet.
    screen: Option<(Rect, Rect)>,
    /// Déplacement par pas `[premier, dernier]` de l'animation scriptée en cours
    /// (x vers l'avant, y vers le bas), en points.
    script: [Vec2; 2],
    /// Variante de couleur de la planche de sprites.
    skin: u32,
    /// Personnage secondaire : sa profondeur (0 pour un animal principal).
    child_depth: u8,
    /// Personnage secondaire arrivé au bout de son rôle, à retirer.
    finished: bool,
    /// Personnages secondaires à faire apparaître, récupérés par le troupeau.
    children: Vec<ChildRequest>,
    /// Envoyé en arrière par une collision : il se pose sans se retourner.
    knocked: bool,
    /// Élan qui reste après un atterrissage (points par seconde) : l'animal
    /// glisse un peu avant de s'arrêter, même quand ses images le figent.
    slide: f32,
    /// Temps avant de pouvoir être heurté de nouveau.
    bump_cooldown: f32,
    /// Pause avant de pouvoir sursauter de nouveau (secondes).
    startle_cooldown: f32,
    /// Un comportement contagieux qu'il vient de commencer (rang), que ses
    /// voisins peuvent imiter (voir [`Flock::echo`]).
    began: Option<usize>,
    /// Ce qu'il imitera d'un voisin, et quand.
    echo: Option<Echo>,
    /// Il vient d'imiter : il n'imite plus pendant ce temps (secondes).
    echo_cooldown: f32,
    /// Il se ramasse avant de sauter : le temps qui reste, et l'élan qu'il
    /// prendra.
    takeoff: Option<(f32, Vec2)>,
    /// Le saut sur un rebord plus haut en cours (voir `pet/leap.rs`), et
    /// celui qui s'offre à lui pendant qu'il choisit sa réaction (au côté
    /// d'une fenêtre, au bout d'un rebord).
    leap: Option<leap::Leap>,
    leap_offer: Option<leap::Leap>,
    /// Il choisit sa réaction : un saut sur un rebord ne lui est permis que
    /// si on le lui propose (voir `Pet::leap_allowed`).
    leap_asked: bool,
    /// Il marche jusque-là (un regroupement du troupeau, le point de départ
    /// d'un voyage), puis s'arrête.
    goal: Option<f32>,
    /// Son voyage vers un autre écran (voir `pet/voyage.rs`), et dans
    /// combien de temps l'envie lui en viendra (secondes ; 0 : pas encore
    /// tiré).
    voyage: Option<voyage::Voyage>,
    voyage_in: f32,
    /// Le sol qu'il traverse en descendant vers l'écran du dessous : il ne
    /// s'y repose pas.
    passing: Option<SurfaceId>,
    /// Le regroupement auquel il prendra part : où aller, et quand partir.
    regroup: Option<(f32, f32)>,
    /// Sa prochaine envie d'inviter un autre à jouer (secondes ; jamais s'il
    /// ne sait pas jouer), et l'invitation en cours.
    invite_in: f32,
    invite: Option<Invite>,
    /// Dans combien de temps il s'endort (l'utilisateur est parti).
    nap_in: Option<f32>,
    /// Dans combien de temps il se réveille (l'utilisateur est revenu).
    wake_in: Option<f32>,
    /// Le vent du moment : accélération qu'il donne en l'air (pt/s², signée).
    wind: f32,
    /// Distance du point d'appui au bout de son dessin, devant et derrière
    /// (points) : les collisions partent quand les dessins se touchent.
    /// Sans elle, la moitié de la largeur de l'animal, des deux côtés.
    reach: Option<(f32, f32)>,
    /// Hauteur du haut de son dessin au-dessus de ses pattes (points), image
    /// par image (voir [`Pet::set_tops`]).
    tops: Option<Arc<[f32]>>,
    /// Hauteur de son dos, là où un autre se pose : le haut de son dessin,
    /// suivi en douceur (couché, il descend ; debout, il remonte).
    back_top: f32,
    /// Temps avant de pouvoir rencontrer un autre animal.
    meet_cooldown: f32,
    /// Son tempérament (voir [`crate::temper`]) : donné par le troupeau.
    temper: Temper,
    /// En train de dire au revoir.
    leaving: Option<Leaving>,
    anim: Anim,
    particles: Vec<Particle>,
    events: Vec<PetEvent>,
    shadow: Option<Shadow>,
    /// Physique réaliste (voir [`crate::physics`]) : angle du corps
    /// (radians, sens horaire de l'écran) et vitesse de rotation.
    angle: f32,
    spin: f32,
    /// Au sol en physique réaliste : la vitesse que lui donne la plateforme
    /// qui le porte (ses pattes la suivent tant qu'elles adhèrent). `vel`
    /// est alors sa vitesse à lui, par rapport à elle.
    carry: Vec2,
    /// Vitesse lissée de la main qui le tient (physique réaliste), et où
    /// elle était au pas précédent.
    hand: Vec2,
    hand_at: Option<Vec2>,
    /// Animal scripté (desktopPet) qui tombe pour de vrai, sous la pesanteur.
    free_fall: bool,
    /// Temps passé en l'air depuis qu'il a quitté le sol (physique réaliste).
    air_time: f32,
    /// La plateforme s'est dérobée sous ses pattes il y a un instant : il
    /// garde son comportement (et le reprend s'il retrouve vite le sol)
    /// avant de passer à sa chute.
    grace: bool,
    /// Le pas en cours est-il en physique réaliste ?
    realistic: bool,
    /// Vitesse dans le repère de l'écran au pas précédent (physique
    /// réaliste) : ses accélérations font bouger oreilles et queue.
    prev_world_vel: Vec2,
    /// Retard de son dessin sur son corps (points) : quand la simulation le
    /// replace d'un coup (séparé d'un autre corps, ramené à un bord), son
    /// dessin reste où il était et le rejoint en glissant, en quelques
    /// centièmes de seconde (voir [`SMOOTH_TIME`] et [`SMOOTH_SPEED`]). Rien
    /// ne saute à l'écran.
    smooth: Vec2,
    /// Le même au pas précédent (pour l'interpolation entre deux pas).
    prev_smooth: Vec2,
    /// Tout juste arrivé (apparu, réapparu) : jusqu'à ce qu'il se pose une
    /// première fois, il passe devant les autres au lieu de les heurter.
    /// Plusieurs animaux qui apparaissent ensemble se chevauchent souvent :
    /// ils ne s'écartent pas d'un coup en tombant.
    ghost: bool,
    /// Hauteur du coin sur lequel il reposait, penché, la dernière fois
    /// qu'on l'a vu au sol pendant ce pas (voir `keep_lift_on_take_off`).
    ground_lift: Vec2,
}

/// Au revoir : l'animal joue son animation d'adieu, puis s'efface.
#[derive(Clone, Copy, Debug)]
struct Leaving {
    /// Temps écoulé depuis l'au revoir.
    time: f32,
    /// Opacité restante, une fois le fondu commencé.
    fade: Option<f32>,
}

/// Demande d'apparition d'un personnage secondaire.
#[derive(Clone, Copy, Debug)]
struct ChildRequest {
    behavior: usize,
    /// Point d'appui (milieu du bas de l'image).
    pos: Vec2,
    facing: f32,
    depth: u8,
    skin: u32,
    /// `randS` du parent : dans desktopPet, il vaut pareil pour toute la
    /// scène (le bain dure autant que la plongée du mouton dans la baignoire).
    rand_s: f32,
}

impl Pet {
    /// Fait apparaître un animal en haut de l'écran (comme le mouton d'origine),
    /// à l'abscisse donnée ou au hasard.
    #[must_use]
    pub fn spawn(id: u32, def: Arc<PetDef>, seed: u64, world: &World, x: Option<f32>) -> Self {
        Self::spawn_as(id, def, seed, world, x, None)
    }

    /// Comme [`Pet::spawn`], du sexe voulu : `sex` remplace le tirage quand
    /// l'espèce a des animaux des deux sexes (une proportion de mâles
    /// strictement entre 0 et 1). Un dessin fixé à un sexe (`sex = "male"`)
    /// le garde : un bélier dessiné ne devient pas brebis. Le tirage a lieu
    /// de toute façon : demander un sexe ne décale aucun autre tirage.
    #[must_use]
    pub fn spawn_as(
        id: u32,
        def: Arc<PetDef>,
        seed: u64,
        world: &World,
        x: Option<f32>,
        sex: Option<Sex>,
    ) -> Self {
        let mut rng = Rng::new(seed ^ (u64::from(id) << 32));
        if x.is_none()
            && let Some(i) = rng.pick_weighted(def.spawns.iter().map(|s| s.weight))
        {
            let spawn = def.spawns[i].clone();
            return Self::from_spawn_point(id, def, rng, world, &spawn);
        }
        // Du haut de son écran : celui de l'abscisse demandée, sinon un
        // écran au hasard (voir `World::entry`).
        let top = match x {
            Some(x) => Vec2::new(x, world.top_over(x)),
            None => world.entry(&mut rng, def.physics.width, None),
        };
        let pos = Vec2::new(top.x, top.y + def.physics.height + 2.0);
        Self::at(id, def, rng, pos, world, sex)
    }

    /// Comme [`Pet::spawn_as`], sur l'écran où se trouve `near` (celui que
    /// l'utilisateur regarde, celui d'une mère) : il tombe du haut de cet
    /// écran-là, à l'abscisse `x` ou à une abscisse au hasard. Les animaux
    /// desktopPet gardent leurs propres points d'apparition.
    #[must_use]
    pub fn spawn_near(
        id: u32,
        def: Arc<PetDef>,
        seed: u64,
        world: &World,
        near: Vec2,
        x: Option<f32>,
        sex: Option<Sex>,
    ) -> Self {
        let mut rng = Rng::new(seed ^ (u64::from(id) << 32));
        if x.is_none()
            && let Some(i) = rng.pick_weighted(def.spawns.iter().map(|s| s.weight))
        {
            let spawn = def.spawns[i].clone();
            let mut pet = Self::bare(id, def, rng, near, None);
            pet.place_at(world, &spawn);
            return pet;
        }
        let top = match x {
            Some(x) => Vec2::new(x, world.top_at(near)),
            None => world.entry(&mut rng, def.physics.width, Some(near)),
        };
        let pos = Vec2::new(top.x, top.y + def.physics.height + 2.0);
        Self::at(id, def, rng, pos, world, sex)
    }

    /// Fait apparaître un animal à une position précise (en l'air).
    #[must_use]
    pub fn spawn_at(id: u32, def: Arc<PetDef>, seed: u64, world: &World, pos: Vec2) -> Self {
        let rng = Rng::new(seed ^ (u64::from(id) << 32));
        Self::at(id, def, rng, pos, world, None)
    }

    /// Apparition à un point desktopPet.
    fn from_spawn_point(
        id: u32,
        def: Arc<PetDef>,
        rng: Rng,
        world: &World,
        spawn: &SpawnPoint,
    ) -> Self {
        let mut pet = Self::bare(id, def, rng, Vec2::ZERO, None);
        pet.place_at(world, spawn);
        pet
    }

    /// Place l'animal à un point d'apparition desktopPet et lance sa première
    /// animation. Le point peut être hors de l'écran : le mouton entre alors en
    /// marchant par le bord, ou tombe du haut. Un animal retourné apparaît en
    /// miroir, comme dans desktopPet.
    fn place_at(&mut self, world: &World, spawn: &SpawnPoint) {
        self.update_screen(world);
        let p = self.def.physics;
        let (frame, _) = self.screen.unwrap_or_default();
        // Les scènes à plusieurs personnages (bain, mouton noir…) ne se placent
        // correctement que dans le sens d'origine du dessin.
        if !self.def.behaviors[spawn.next].children.is_empty() {
            self.facing = -1.0;
        }
        let mut left = self.eval_position(&spawn.x);
        let top = self.eval_position(&spawn.y);
        if self.facing > 0.0 {
            left = frame.w - left - p.width;
        }
        self.pos = Vec2::new(frame.x + left + p.width * 0.5, frame.y + top + p.height);
        self.prev_pos = self.pos;
        self.prev_lift = Vec2::ZERO;
        self.clear_smooth();
        self.vel = Vec2::ZERO;
        self.hold = None;
        self.support = Support::Air;
        self.ghost = true;
        self.ground_ref = None;
        self.walking_off = false;
        self.anim.flip = self.facing;
        // Déjà posé sur le sol (même hors de l'écran) ou sur une fenêtre ?
        let y = self.pos.y;
        let x = self.pos.x;
        let ground = world
            .floor_below(x, y)
            .filter(|f| (f.y - y).abs() < 1.5)
            .or_else(|| {
                world.surfaces().iter().find(|s| {
                    s.kind == SurfaceKind::Window && (s.y - y).abs() < 1.5 && s.contains_x(x, 0.0)
                })
            })
            .copied();
        if let Some(surface) = ground {
            self.pos.y = surface.y;
            self.prev_pos = self.pos;
            self.set_ground(&surface);
            self.anim.air = 0.0;
        }
        self.enter(spawn.next);
    }

    /// Nouvelle apparition (desktopPet : animal sorti de l'écran, ou sans suite).
    fn respawn(&mut self, world: &World) {
        let def = Arc::clone(&self.def);
        if let Some(i) = self.rng.pick_weighted(def.spawns.iter().map(|s| s.weight)) {
            self.place_at(world, &def.spawns[i]);
            return;
        }
        let top = world.entry(&mut self.rng, def.physics.width, None);
        self.pos = Vec2::new(top.x, top.y + def.physics.height + 2.0);
        self.prev_pos = self.pos;
        self.prev_lift = Vec2::ZERO;
        self.clear_smooth();
        self.vel = Vec2::ZERO;
        self.support = Support::Air;
        self.ghost = true;
        self.ground_ref = None;
        self.walking_off = false;
        let first = self
            .choose(&def.reflexes.spawn, world, &Input::default())
            .or(def.default_air())
            .unwrap_or(def.default_ground());
        self.enter(first);
    }

    /// Personnage secondaire (desktopPet `child`), né d'un autre animal.
    fn child(id: u32, def: Arc<PetDef>, seed: u64, world: &World, request: ChildRequest) -> Self {
        let rng = Rng::new(seed ^ (u64::from(id) << 32) ^ 0xC41D);
        let mut pet = Self::bare(id, def, rng, request.pos, None);
        pet.child_depth = request.depth;
        pet.skin = request.skin;
        pet.rand_s = request.rand_s;
        pet.facing = request.facing;
        pet.anim.flip = request.facing;
        pet.update_screen(world);
        let (x, y) = (pet.pos.x, pet.pos.y);
        let ground = world
            .floor_below(x, y)
            .filter(|f| (f.y - y).abs() < 1.5)
            .or_else(|| {
                world.surfaces().iter().find(|s| {
                    s.kind == SurfaceKind::Window && (s.y - y).abs() < 1.5 && s.contains_x(x, 0.0)
                })
            })
            .copied();
        if let Some(surface) = ground {
            pet.pos.y = surface.y;
            pet.prev_pos = pet.pos;
            pet.set_ground(&surface);
            pet.anim.air = 0.0;
        }
        pet.enter(request.behavior);
        pet
    }

    fn set_ground(&mut self, surface: &Surface) {
        self.support = Support::Ground {
            surface: surface.id,
        };
        self.ghost = false;
        self.ground_lift = self.tilt_lift();
        self.ground_ref = Some(GroundRef {
            surface: surface.id,
            x0: surface.x0,
            x1: surface.x1,
        });
        self.walking_off = false;
        self.passing = None;
    }

    fn detach(&mut self) {
        self.support = Support::Air;
        self.ground_ref = None;
        self.walking_off = false;
        self.take_off();
    }

    /// Il quitte le sol : en physique réaliste, l'élan de la plateforme qui
    /// le portait devient le sien (rien à faire en physique classique).
    fn take_off(&mut self) {
        self.vel += self.carry;
        self.carry = Vec2::ZERO;
        self.air_time = 0.0;
        if self.realistic && matches!(self.def.behaviors[self.behavior].motion, Motion::Script(_)) {
            // Un animal scripté qui quitte le sol tombe désormais pour de vrai.
            self.free_fall = true;
        }
    }

    /// La fenêtre sous l'animal a bougé (sans changer de largeur) : il voyage avec elle.
    fn ride(&mut self, surface: &Surface) {
        if let Some(r) = self.ground_ref
            && r.surface == surface.id
        {
            let dx0 = surface.x0 - r.x0;
            let dx1 = surface.x1 - r.x1;
            if (dx0 - dx1).abs() < 0.5 && dx0 != 0.0 {
                self.pos.x += dx0;
            }
        }
        self.ground_ref = Some(GroundRef {
            surface: surface.id,
            x0: surface.x0,
            x1: surface.x1,
        });
    }

    fn bare(id: u32, species: Arc<PetDef>, mut rng: Rng, pos: Vec2, wanted: Option<Sex>) -> Self {
        let voice = rng.fork(0x534F_554E_4453);
        let male = species.sex_ratio.male_probability();
        let drawn = if rng.fork(0x5345_585F_5241_5449).chance(male) {
            Sex::Male
        } else {
            Sex::Female
        };
        // Le sexe demandé (voir `Pet::spawn_as`), si l'espèce a les deux.
        let sex = wanted.filter(|_| male > 0.0 && male < 1.0).unwrap_or(drawn);
        // Il porte le dessin de son sexe et de son âge, s'il en a un : sa
        // taille, sa foulée et ses vitesses sont celles de ce dessin.
        let variant = species.variant_for(sex, species.age);
        let def = variant
            .and_then(|v| species.variant(v))
            .map_or_else(|| Arc::clone(&species), Arc::clone);
        let plays = def.behavior_index("play_invite").is_some();
        let def_back_top = def.physics.height * BACK_HEIGHT;
        let sheep = def.meta.category() == "sheep";
        let facing = if rng.chance(0.5) { 1.0 } else { -1.0 };
        let anim = Anim::new(facing, &mut rng);
        let rand_s = (10 + rng.next_u32() % 80) as f32;
        let skin = match &def.appearance {
            Appearance::Sprites(sheet) if sheet.skins > 1 => rng.next_u32() % sheet.skins,
            _ => 0,
        };
        Self {
            id,
            def,
            species,
            variant,
            family: family::FamilyState::default(),
            sex,
            sheep,
            sniffed_ewe_for: 0.0,
            rng,
            voice,
            pos,
            prev_pos: pos,
            prev_lift: Vec2::ZERO,
            vel: Vec2::ZERO,
            prev_vel_x: 0.0,
            facing,
            support: Support::Air,
            ground_ref: None,
            walking_off: false,
            hold: None,
            behavior: 0,
            behavior_time: 0.0,
            behavior_length: f32::INFINITY,
            clip_step: 0,
            clip_timer: 0.0,
            clip_total: 1,
            rand_s,
            screen: None,
            script: [Vec2::ZERO; 2],
            skin,
            child_depth: 0,
            finished: false,
            children: Vec::new(),
            knocked: false,
            slide: 0.0,
            bump_cooldown: 0.0,
            temper: Temper::Even,
            startle_cooldown: 0.0,
            began: None,
            echo: None,
            echo_cooldown: 0.0,
            takeoff: None,
            leap: None,
            leap_offer: None,
            leap_asked: false,
            goal: None,
            voyage: None,
            voyage_in: 0.0,
            passing: None,
            regroup: None,
            // Sans tirage au hasard : ceux qui ne jouent pas gardent leurs
            // tirages ; les autres se décalent d'après leur numéro.
            invite_in: if plays {
                INVITE_EVERY.0 + (id % 7) as f32 * 4.0
            } else {
                f32::INFINITY
            },
            invite: None,
            nap_in: None,
            wake_in: None,
            wind: 0.0,
            reach: None,
            tops: None,
            back_top: def_back_top,
            meet_cooldown: 0.0,
            leaving: None,
            anim,
            particles: Vec::new(),
            events: Vec::new(),
            shadow: None,
            angle: 0.0,
            spin: 0.0,
            carry: Vec2::ZERO,
            hand: Vec2::ZERO,
            hand_at: None,
            free_fall: false,
            air_time: 0.0,
            grace: false,
            realistic: false,
            prev_world_vel: Vec2::ZERO,
            smooth: Vec2::ZERO,
            prev_smooth: Vec2::ZERO,
            ghost: true,
            ground_lift: Vec2::ZERO,
        }
    }

    fn at(id: u32, def: Arc<PetDef>, rng: Rng, pos: Vec2, world: &World, sex: Option<Sex>) -> Self {
        let mut pet = Self::bare(id, def, rng, pos, sex);
        pet.update_screen(world);
        let def = Arc::clone(&pet.def);
        let first = pet
            .choose(&def.reflexes.spawn, world, &Input::default())
            .or(def.default_air())
            .unwrap_or(def.default_ground());
        pet.enter(first);
        pet
    }

    #[must_use]
    pub fn id(&self) -> u32 {
        self.id
    }

    /// La définition qu'il porte : celle du dessin de son sexe et de son âge
    /// s'il en a un (sa taille, ses vitesses), sinon celle de son espèce.
    #[must_use]
    pub fn def(&self) -> &Arc<PetDef> {
        &self.def
    }

    /// La définition avec laquelle il est apparu, commune à tous ceux de son
    /// espèce, quel que soit leur dessin : c'est elle qui le relie à son
    /// entrée du menu, à sa taille, à ses changements de dessin.
    #[must_use]
    pub fn species(&self) -> &Arc<PetDef> {
        &self.species
    }

    /// Le dessin qu'il porte (voir [`PetDef::set_variants`]) ; `None` : le
    /// dessin de base.
    #[must_use]
    pub fn variant(&self) -> Option<Variant> {
        self.variant
    }

    #[must_use]
    pub fn position(&self) -> Vec2 {
        self.pos
    }

    /// Où est son dessin, à peu près (au dernier pas, sans l'interpolation
    /// ni le coin d'un animal penché) : sa position, plus le retard de son
    /// dessin quand la simulation l'a replacé d'un coup (voir `smooth`).
    #[must_use]
    pub fn drawn_position(&self) -> Vec2 {
        self.pos + self.smooth
    }

    #[must_use]
    pub fn velocity(&self) -> Vec2 {
        self.vel
    }

    #[must_use]
    pub fn support(&self) -> Support {
        self.support
    }

    #[must_use]
    pub fn behavior_name(&self) -> &str {
        &self.def.behaviors[self.behavior].name
    }

    #[must_use]
    pub fn is_held(&self) -> bool {
        self.support == Support::Held
    }

    /// Son tempérament (voir [`crate::temper`]).
    #[must_use]
    pub fn temper(&self) -> Temper {
        self.temper
    }

    /// Prend son tempérament dans celui du troupeau : le sien, ou, pour un
    /// agneau, un cran plus vif (voir [`Temper::of_lamb`]).
    pub(crate) fn take_temper(&mut self, mood: Mood) {
        let temper = mood.temper_of(self.id);
        self.temper = if self.sheep && self.is_lamb() {
            temper.of_lamb()
        } else {
            temper
        };
    }

    /// Personnage secondaire d'une scène desktopPet : il ne s'attrape pas.
    #[must_use]
    pub fn is_child(&self) -> bool {
        self.child_depth > 0
    }

    /// Le personnage secondaire a fini son rôle.
    #[must_use]
    pub fn is_finished(&self) -> bool {
        self.finished
    }

    pub fn take_events(&mut self) -> Vec<PetEvent> {
        std::mem::take(&mut self.events)
    }

    /// Donne de quoi choisir la cadence d'affichage.
    #[must_use]
    pub fn activity(&self) -> Activity {
        let grounded = matches!(self.support, Support::Ground { .. });
        let settling = self.anim.squash.velocity.abs() > 0.02
            || (self.anim.flip - self.facing).abs() > 0.01
            || self.anim.swing.value.abs() > 0.01
            || self.anim.walk > 0.02
            || self.anim.roll_left != 0.0
            // Physique réaliste : il se redresse, ou une fenêtre l'emporte.
            || self.angle != 0.0
            || self.spin != 0.0
            || self.carry.length() > 0.5
            // Un geste (s'ébrouer, gratter le sol…) bouge vite.
            || self.anim.gesture_amount > 0.02
            // Il se prépare à sauter sur un rebord, ou s'y reçoit.
            || self.leap.is_some()
            || self.anim.leap_after > 0.0;
        let sprites = self.def.behaviors[self.behavior].clip.is_some();
        // Les bulles de sommeil montent lentement : elles n'exigent pas la pleine cadence.
        let dust = self.particles.iter().any(|p| p.kind == ParticleKind::Dust);
        if !grounded || self.vel.x.abs() > 0.5 || settling || dust {
            Activity::Moving
        } else if sprites {
            // Les images changent à leur rythme (30 ms au plus court) : inutile de courir.
            Activity::Idle
        } else if self.anim.sleep > 0.9 {
            Activity::Asleep
        } else {
            Activity::Idle
        }
    }

    /// Images par seconde qu'il lui faut pour que son mouvement reste fluide :
    /// [`SLOW_RATE`] quand il va son petit train, `f32::INFINITY` (toute la
    /// cadence de l'écran) sinon.
    ///
    /// Au pas, en grimpant ou en se promenant sur un rebord, il avance de
    /// moins d'un point d'une image à l'autre à 60 images par seconde : sa
    /// fenêtre, qui ne se pose qu'au point entier, ne saute aucun point, et
    /// ses pattes dessinées bougent comme sur n'importe quel écran à 60 Hz.
    /// Un écran ProMotion peut alors battre deux fois moins vite. En vol libre,
    /// tenu, plus vite, ou tant qu'un rebond se calme, il faut toute la cadence.
    #[must_use]
    pub fn smooth_rate(&self) -> f32 {
        let scripted = matches!(self.def.behaviors[self.behavior].motion, Motion::Script(_));
        let free = match self.support {
            Support::Held => true,
            // Une animation scriptée avance à vitesse constante, pas en chute libre.
            Support::Air => !scripted,
            Support::Ground { .. } => false,
        };
        let settling = self.anim.squash.velocity.abs() > 0.02
            || (self.anim.flip - self.facing).abs() > 0.01
            || self.anim.swing.value.abs() > 0.01
            || self.anim.roll_left != 0.0
            || self.spin != 0.0
            || self.particles.iter().any(|p| p.kind == ParticleKind::Dust);
        let world_vel = self.vel + self.carry;
        let speed = world_vel.x.abs().max(world_vel.y.abs());
        if free || settling || speed > SLOW_RATE {
            f32::INFINITY
        } else {
            SLOW_RATE
        }
    }

    // ----------------------------------------------------------------- entrées

    /// Le curseur attrape l'animal.
    pub fn grab(&mut self, cursor: Vec2, world: &World) {
        if self.is_child() || self.leaving.is_some() {
            return;
        }
        self.family.stop();
        // Pris en main, il est dessiné sous le curseur : plus de retard.
        self.clear_smooth();
        self.ghost = false;
        let offset = if self.realistic {
            // Le point de prise dans le repère du corps : l'animal pendra
            // dessous et tournera autour (voir `step_held_real`).
            self.grip_offset(cursor)
        } else {
            cursor - self.pos
        };
        self.hold = Some(Hold {
            target: cursor,
            offset,
        });
        self.support = Support::Held;
        self.vel = Vec2::ZERO;
        self.carry = Vec2::ZERO;
        self.hand = Vec2::ZERO;
        self.hand_at = Some(cursor);
        self.spin = 0.0;
        self.free_fall = false;
        self.grace = false;
        self.ground_ref = None;
        self.walking_off = false;
        self.knocked = false;
        self.squash_kick(-0.9);
        let def = Arc::clone(&self.def);
        let next = self
            .choose(&def.reflexes.held, world, &Input::default())
            .or_else(|| def.behaviors.iter().position(|b| b.motion == Motion::Held));
        if let Some(next) = next {
            self.enter(next);
        }
    }

    pub fn drag_to(&mut self, cursor: Vec2) {
        if let Some(hold) = &mut self.hold {
            hold.target = cursor;
        }
    }

    /// Au sol, il se penche contre le vent (radians, sens horaire).
    fn wind_lean(&self) -> f32 {
        if matches!(self.support, Support::Ground { .. }) {
            -(self.wind / crate::surprise::GUST_PEAK) * 0.07
        } else {
            0.0
        }
    }

    /// Là où la main (ou le faisceau d'une soucoupe) le tient.
    #[must_use]
    pub fn hold_target(&self) -> Option<Vec2> {
        self.hold.map(|h| h.target)
    }

    /// Le curseur relâche l'animal, qui garde son élan.
    pub fn release(&mut self, world: &World) {
        if self.support != Support::Held {
            return;
        }
        if self.realistic {
            // Lancé avec l'élan de la main et sa rotation (voir `throw_real`).
            self.throw_real();
        }
        self.hold = None;
        self.vel = self.vel.clamp_length(self.def.physics.max_throw_speed);
        self.support = Support::Air;
        self.air_time = 0.0;
        self.events.push(PetEvent::Thrown {
            speed: self.vel.length(),
        });
        let def = Arc::clone(&self.def);
        // Lâché sans élan, il tombe simplement (desktopPet : `fall` plutôt que `toss`).
        let gentle = self.vel.length() < 0.25 * def.physics.hard_landing_speed;
        let first = if gentle {
            &def.reflexes.fall
        } else {
            &def.reflexes.thrown
        };
        let next = self
            .choose(first, world, &Input::default())
            .or_else(|| self.choose(&def.reflexes.thrown, world, &Input::default()))
            .or_else(|| self.choose(&def.reflexes.fall, world, &Input::default()))
            .or(def.default_air());
        if let Some(next) = next {
            self.enter(next);
        }
    }

    /// Impose un comportement par son nom (outils, tests, menu de débogage).
    /// Renvoie `false` si le nom est inconnu ou si son contexte biologique
    /// manque (le flehmen demande un bélier qui vient de flairer une brebis).
    pub fn force(&mut self, name: &str) -> bool {
        match self.def.behavior_index(name) {
            Some(index) if self.behavior_allowed(index) => {
                self.enter(index);
                true
            }
            _ => false,
        }
    }

    /// Change la taille de l'animal sans l'interrompre. `def` est la même
    /// espèce à une autre échelle (voir [`PetDef::scaled`] et
    /// [`Pet::species`]) et `ratio` le rapport de la nouvelle taille à
    /// l'ancienne ; ses pattes restent où elles sont, et il garde le dessin
    /// de son sexe et de son âge. Renvoie `false` (et ne change rien) si
    /// `def` n'a pas les mêmes comportements ou si le rapport n'est pas
    /// positif et fini.
    pub fn rescale(&mut self, def: Arc<PetDef>, ratio: f32) -> bool {
        let same = def.behaviors.len() == self.def.behaviors.len()
            && def
                .behaviors
                .iter()
                .zip(&self.def.behaviors)
                .all(|(a, b)| a.name == b.name);
        if !same || !ratio.is_finite() || ratio <= 0.0 {
            return false;
        }
        self.family.stop();
        self.variant = def.variant_for(self.sex, def.age);
        self.def = self
            .variant
            .and_then(|v| def.variant(v))
            .map_or_else(|| Arc::clone(&def), Arc::clone);
        self.species = def;
        self.vel *= ratio;
        self.script = [self.script[0] * ratio, self.script[1] * ratio];
        self.reach = self
            .reach
            .map(|(front, back)| (front * ratio, back * ratio));
        if let Some(hold) = &mut self.hold {
            hold.offset *= ratio;
        }
        true
    }

    /// Tourne l'animal d'un coup vers la droite (1) ou la gauche (-1) : outils et tests.
    pub fn face(&mut self, facing: f32) {
        self.facing = if facing < 0.0 { -1.0 } else { 1.0 };
        self.anim.flip = self.facing;
    }

    /// Donne la place réelle de son dessin, devant et derrière son point
    /// d'appui (points) : il heurte ou rencontre un autre animal quand leurs
    /// dessins se touchent, pas avant ni après.
    pub fn set_reach(&mut self, front: f32, back: f32) {
        self.reach = (front > 0.0 && back > 0.0).then_some((front, back));
    }

    /// Donne la hauteur du haut de son dessin au-dessus de ses pattes
    /// (points), pour chaque image de sa planche : un animal posé sur son dos
    /// reste sur ce qu'on voit de lui, même couché ou endormi.
    pub fn set_tops(&mut self, tops: Arc<[f32]>) {
        self.tops = Some(tops);
    }

    /// Le haut de son dessin au-dessus de ses pattes, maintenant : celui de
    /// son image (décalée comme elle est dessinée), ou, pour un dessin animé
    /// par le moteur, sa hauteur debout, moins ce dont il se couche.
    fn visible_top(&self) -> f32 {
        let p = self.def.physics;
        let standing = p.height * BACK_HEIGHT;
        let behavior = &self.def.behaviors[self.behavior];
        match (&self.tops, &behavior.clip) {
            (Some(tops), Some(_)) => {
                let Some((frame, offset)) = self.current_sprite() else {
                    return standing;
                };
                match tops.get(frame as usize) {
                    Some(&top) if top > 0.0 => (top - offset).max(p.height * 0.2),
                    _ => standing,
                }
            }
            _ => standing - p.height * LYING_DROP * self.anim.sit.clamp(0.0, 1.0),
        }
    }

    /// Suit le haut de son dessin : vite, mais sans saut (un animal posé
    /// dessus descend avec lui quand il se couche).
    fn follow_back_top(&mut self, dt: f32) {
        let target = self.visible_top();
        if !matches!(self.support, Support::Ground { .. }) {
            self.back_top = target;
            return;
        }
        let rate = BACK_RATE * (self.def.physics.height / 60.0).max(0.5);
        self.back_top = approach(self.back_top, target, rate * dt);
    }

    /// Distance du point d'appui au bout de son dessin, du côté `side`
    /// (1 : vers la droite, -1 : vers la gauche).
    fn reach_toward(&self, side: f32) -> f32 {
        let (front, back) = self.reach.unwrap_or_else(|| {
            let half = self.def.physics.width * 0.5;
            (half, half)
        });
        if self.facing * side > 0.0 {
            front
        } else {
            back
        }
    }

    /// Peut-il être heurté par un autre animal (mode collisions) ?
    #[must_use]
    pub fn can_bump(&self) -> bool {
        !self.is_child()
            && !self.finished
            && !self.is_held()
            && self.leaving.is_none()
            && self.bump_cooldown <= 0.0
    }

    /// En train de dire au revoir (il s'efface ensuite).
    #[must_use]
    pub fn is_leaving(&self) -> bool {
        self.leaving.is_some()
    }

    /// Dit au revoir : l'animal joue son animation d'adieu s'il en a une et
    /// qu'il est posé, puis s'efface et quitte le troupeau.
    pub fn leave(&mut self, world: &World, input: &Input) {
        if self.leaving.is_some() || self.finished {
            return;
        }
        let mut leaving = Leaving {
            time: 0.0,
            fade: Some(1.0),
        };
        if self.hold.is_some() {
            self.release(world);
        } else if matches!(self.support, Support::Ground { .. }) && !self.is_child() {
            let def = Arc::clone(&self.def);
            if let Some(next) = self.choose(&def.reflexes.leave, world, input) {
                self.enter(next);
                leaving.fade = None;
            }
        }
        self.leaving = Some(leaving);
    }

    /// Tous ensemble : l'animal joue son comportement `together` s'il en a
    /// un et qu'il est libre (posé, ni tenu ni en partance). Renvoie `true`
    /// s'il le joue.
    pub fn together(&mut self, world: &World, input: &Input) -> bool {
        if self.is_child()
            || self.leaving.is_some()
            || !matches!(self.support, Support::Ground { .. })
        {
            return false;
        }
        let def = Arc::clone(&self.def);
        match self.choose(&def.reflexes.together, world, input) {
            Some(next) => {
                self.enter(next);
                true
            }
            None => false,
        }
    }

    /// Peut-il en rencontrer un autre (hors mode collisions) ?
    fn can_meet(&self) -> bool {
        !self.is_child()
            && !self.finished
            && self.hold.is_none()
            && self.leaving.is_none()
            && self.meet_cooldown <= 0.0
            && matches!(self.support, Support::Ground { .. })
    }

    /// Réactions possibles à une rencontre pendant le comportement en cours.
    /// Un animal desktopPet suit son animation ; un animal Mouflon prend le
    /// réflexe `meet` quand il est en chemin, et ne réagit au repos que si
    /// son comportement le prévoit.
    fn meet_reactions<'d>(&self, def: &'d PetDef) -> &'d [Transition] {
        let behavior = &def.behaviors[self.behavior];
        if !behavior.on_meet.is_empty() || def.rules == Rules::DesktopPet {
            &behavior.on_meet
        } else if self.vel.x.abs() > 1.0 {
            &def.reflexes.meet
        } else {
            &[]
        }
    }

    /// Le côté d'une fenêtre barre le chemin de l'animal qui avance de `dx` :
    /// renvoie sa réaction et l'abscisse où il s'arrête, ou rien (il passe
    /// alors devant la fenêtre). Un animal Mouflon en l'air ne réagit que si
    /// son comportement le prévoit.
    fn window_side(
        &mut self,
        dx: f32,
        world: &World,
        input: &Input,
        grounded: bool,
    ) -> Option<(usize, f32)> {
        let def = Arc::clone(&self.def);
        let p = def.physics;
        let behavior = &def.behaviors[self.behavior];
        let list: &[Transition] = if !behavior.on_window_side.is_empty() {
            &behavior.on_window_side
        } else if grounded {
            &def.reflexes.window_side
        } else {
            &[]
        };
        if list.is_empty() || dx == 0.0 {
            return None;
        }
        let dir = dx.signum();
        let half = p.width * 0.5;
        let front = self.pos.x + dir * half;
        let wall = world.wall_crossed(front, front + dx, self.pos.y - p.height, self.pos.y)?;
        // Pas la place de se tenir entre la fenêtre et le bord de l'écran : on
        // l'ignore (desktopPet).
        let (left, right) = world.span(self.pos.x, self.pos.y - p.height, self.pos.y);
        let room = if dir > 0.0 {
            wall.x - left
        } else {
            right - wall.x
        };
        if room < p.width {
            return None;
        }
        let x = wall.x - dir * half;
        // Une fenêtre plus haute, toute proche : il peut y sauter, si son
        // rebord est à sa portée.
        if grounded {
            self.offer_leap(world, dir);
        }
        let next = self.choose(list, world, input);
        self.close_leap_offer();
        Some((next?, x))
    }

    /// Heurté par un autre animal : il part en arrière avec l'élan `push`
    /// (pt/s) et fait une culbute, puis se pose sans se retourner. Renvoie
    /// `false` si l'animal ne sait pas voler (aucun comportement de chute
    /// physique) : il fait alors simplement demi-tour.
    pub fn knock(&mut self, push: Vec2, world: &World, input: &Input) -> bool {
        if !self.can_bump() {
            return false;
        }
        self.bump_cooldown = BUMP_COOLDOWN;
        let def = Arc::clone(&self.def);
        // Une chute physique, pour que l'élan compte (pas une chute image par image).
        let flying = |i: &usize| def.behaviors[*i].motion == Motion::Fall;
        let next = self
            .choose(&def.reflexes.bump, world, input)
            .filter(flying)
            .or_else(|| {
                self.choose(&def.reflexes.thrown, world, input)
                    .filter(flying)
            })
            .or(def.default_air());
        let Some(next) = next else {
            if push.x * self.facing < 0.0 {
                self.facing = -self.facing;
            }
            return false;
        };
        self.vel = push + self.carry;
        self.carry = Vec2::ZERO;
        self.air_time = 0.0;
        self.grace = false;
        self.ground_lift = self.tilt_lift();
        self.support = Support::Air;
        self.ground_ref = None;
        self.walking_off = false;
        self.knocked = true;
        self.events.push(PetEvent::Bumped {
            speed: push.length(),
        });
        self.squash_kick(-0.7);
        self.anim.ear.kick(-4.0);
        if self.realistic && !def.drawn().roll {
            // Un tour complet pendant le vol : c'est la physique qui le fait tourner.
            let flight = (2.0 * (-push.y).max(60.0) / def.physics.gravity.max(1.0)).max(0.2);
            self.spin = push.x.signum() * TAU / flight;
        } else if !def.drawn().roll {
            // Un tour complet en roulant comme une roue (vers la droite : sens
            // horaire), réparti sur la durée du vol.
            let flight = (2.0 * (-push.y).max(60.0) / def.physics.gravity.max(1.0)).max(0.2);
            let sign = if push.x >= 0.0 { 1.0 } else { -1.0 };
            self.anim.roll_left += sign * TAU;
            self.anim.roll_rate = TAU / flight;
        }
        self.enter(next);
        true
    }

    /// Écrasement ou étirement, sauf si les images le dessinent déjà.
    fn squash_kick(&mut self, amount: f32) {
        if !self.def.drawn().squash {
            self.anim.squash.kick(amount);
        }
    }

    /// Un clic sans glisser.
    pub fn poke(&mut self, world: &World, input: &Input) {
        if self.is_child() || self.leaving.is_some() {
            return;
        }
        self.events.push(PetEvent::Poked);
        self.squash_kick(0.8);
        self.anim.ear.kick(-3.0);
        if matches!(self.support, Support::Ground { .. }) {
            let def = Arc::clone(&self.def);
            if let Some(next) = self.choose(&def.reflexes.poke, world, input) {
                self.enter(next);
            }
        }
    }

    // -------------------------------------------------------------- humeurs

    /// Dort-il (ou s'endort-il) ?
    #[must_use]
    pub fn is_asleep(&self) -> bool {
        let behavior = &self.def.behaviors[self.behavior];
        behavior.sleeping || behavior.motion == Motion::Sleep
    }

    /// Peut-il réagir à ce que fait l'utilisateur (sursauter, s'endormir) ?
    /// Posé, à lui, et pas en train de partir.
    fn can_react(&self) -> bool {
        matches!(self.support, Support::Ground { .. })
            && !self.is_child()
            && self.leaving.is_none()
            && !self.finished
    }

    /// Le moment venu, il imite ce qu'a fait un voisin : tout de suite s'il
    /// est debout, à marcher, à brouter ou à regarder autour de lui ; un
    /// animal en images, à la fin de ce qu'il joue (voir
    /// [`Pet::take_echo`]). Ce qu'il imite ne se propage pas plus loin.
    fn step_echo(&mut self, dt: f32) {
        self.echo_cooldown = (self.echo_cooldown - dt).max(0.0);
        let Some(echo) = &mut self.echo else {
            return;
        };
        echo.delay -= dt;
        echo.patience -= dt;
        if echo.patience <= 0.0 {
            self.echo = None;
            return;
        }
        let free = matches!(
            self.def.behaviors[self.behavior].motion,
            Motion::Stand | Motion::Look | Motion::Graze | Motion::Walk { .. }
        );
        if echo.delay <= 0.0 && free {
            let echo = *echo;
            self.echo = None;
            self.follow(echo);
        }
    }

    /// Au bout de ce qu'il faisait, il imite son voisin plutôt que de
    /// choisir sa suite (un animal au format de Mouflon, pas desktopPet,
    /// dont les scènes s'enchaînent d'elles-mêmes).
    fn take_echo(&mut self) -> bool {
        match self.echo {
            Some(echo) if echo.delay <= 0.0 && self.def.rules != Rules::DesktopPet => {
                self.echo = None;
                self.follow(echo)
            }
            _ => false,
        }
    }

    /// Il imite son voisin, s'il le peut. Renvoie `true` s'il l'a fait.
    fn follow(&mut self, echo: Echo) -> bool {
        // En route vers sa place dans le troupeau, il n'imite personne.
        if !self.can_react()
            || self.is_asleep()
            || self.hold.is_some()
            || self.goal.is_some()
            || !self.behavior_allowed(echo.behavior)
            || self.family.action.active()
        {
            return false;
        }
        self.echo_cooldown = ECHO_COOLDOWN;
        if matches!(
            self.def.energy(echo.behavior),
            crate::Energy::Walk | crate::Energy::Dash
        ) {
            // Il part du même côté que l'autre.
            self.facing = echo.facing;
        }
        self.enter(echo.behavior);
        self.began = None;
        true
    }

    /// Le moment venu, il part rejoindre sa place dans le troupeau qui se
    /// regroupe (voir [`Flock::regroup`]) : il marche (au trot s'il sait
    /// trotter) jusque-là, puis s'arrête.
    fn step_regroup(&mut self, dt: f32) {
        let Some((target, delay)) = &mut self.regroup else {
            return;
        };
        *delay -= dt;
        if *delay > 0.0 {
            return;
        }
        let (target, late) = (*target, *delay);
        // Il attend d'être au sol et éveillé ; au-delà de dix secondes, il
        // n'y va plus.
        if !self.can_react() || self.is_asleep() || self.hold.is_some() {
            if late < -10.0 {
                self.regroup = None;
            }
            return;
        }
        self.regroup = None;
        let def = Arc::clone(&self.def);
        let distance = (target - self.pos.x).abs();
        if distance < def.physics.width * 0.3 {
            return;
        }
        // Il marche ; de loin, il trotte.
        let movers: [&str; 3] = if distance > def.physics.width * 3.0 {
            ["trot", "walk", "marche"]
        } else {
            ["walk", "marche", "trot"]
        };
        let Some(mover) = movers.into_iter().find_map(|name| def.behavior_index(name)) else {
            return;
        };
        self.facing = (target - self.pos.x).signum();
        self.enter(mover);
        self.goal = Some(target);
        self.began = None;
    }

    /// Il quitte le sol avec cet élan (et celui de ce qui le portait). Un
    /// saut qui pivote le retourne en l'air.
    fn launch(&mut self, vel: Vec2) {
        self.vel = vel + self.carry;
        self.support = Support::Air;
        self.ground_ref = None;
        self.carry = Vec2::ZERO;
        self.air_time = 0.0;
        self.squash_kick(1.4);
        if self.def.behaviors[self.behavior].spin {
            self.facing = -self.facing;
        }
    }

    /// Le temps de se ramasser passé, il saute (s'il est toujours au sol) ;
    /// sur un rebord, du bond qui l'y pose d'où il est (voir `pet/leap.rs`).
    fn step_takeoff(&mut self, dt: f32, world: &World) {
        let Some((left, vel)) = &mut self.takeoff else {
            return;
        };
        *left -= dt;
        if *left > 0.0 {
            return;
        }
        let vel = *vel;
        self.takeoff = None;
        if matches!(self.support, Support::Ground { .. }) {
            if self.leap.is_some() {
                self.leap_off(world);
            } else {
                self.launch(vel);
            }
        }
    }

    /// En broutant, un mouton fait un pas ou deux tête basse pour gagner
    /// l'herbe suivante (une station toutes les 7 à 11 s) : sa vitesse du
    /// moment (les animaux en images ont leurs propres images).
    fn graze_step(&self) -> f32 {
        if matches!(self.def.appearance, Appearance::Sprites(_)) || self.behavior_time < 2.5 {
            return 0.0;
        }
        // Chacun à son rythme, sans tirage au hasard.
        let t = self.behavior_time + (self.id % 7) as f32 * 1.13;
        if t % GRAZE_STATION > GRAZE_STATION - GRAZE_STEP_TIME {
            self.def.stride() * GRAZE_STEP_SPEED
        } else {
            0.0
        }
    }

    /// Le pointeur vient de le frôler à toute vitesse, entre `from` et `to`.
    fn startle(&mut self, from: Vec2, to: Vec2, world: &World, input: &Input) {
        if self.startle_cooldown > 0.0 || !self.can_react() || self.is_asleep() {
            return;
        }
        let p = self.def.physics;
        let center = self.pos - Vec2::new(0.0, p.height * 0.5);
        let reach = p.height * 1.1 + p.width * 0.3;
        if distance_to_segment(center, from, to) > reach {
            return;
        }
        self.startle_cooldown = STARTLE_COOLDOWN;
        // Pas à chaque fois : certains ne lèvent même pas la tête.
        if !self.rng.chance(0.7) {
            return;
        }
        self.events.push(PetEvent::Startled);
        self.squash_kick(0.6);
        self.anim.ear.kick(-5.0);
        // La surprise : les oreilles asymétriques, un instant.
        self.anim.ear_split = if self.id.is_multiple_of(2) {
            0.45
        } else {
            -0.45
        };
        let def = Arc::clone(&self.def);
        if let Some(next) = self.choose(&def.reflexes.startle, world, input) {
            self.enter(next);
        }
    }

    /// L'utilisateur part (`away`) ou revient : il s'endort bientôt, ou se
    /// réveille s'il dormait.
    fn user_away(&mut self, away: bool) {
        if away {
            self.wake_in = None;
            if !self.is_asleep() {
                self.nap_in = Some(self.rng.range(1.0, NAP_SPREAD));
            }
        } else {
            self.nap_in = None;
            if self.is_asleep() {
                self.wake_in = Some(self.rng.range(0.2, 1.8));
            }
        }
    }

    /// Les humeurs que lui donne l'utilisateur : il s'endort quand il est
    /// parti (et se rendort s'il s'est réveillé entre-temps), se réveille
    /// quand il revient.
    fn step_moods(&mut self, dt: f32, world: &World, input: &Input) {
        if let Some(left) = &mut self.nap_in {
            *left -= dt;
            if *left <= 0.0 {
                self.nap_in = None;
                if self.can_react() && !self.is_asleep() {
                    let def = Arc::clone(&self.def);
                    if let Some(next) = self.choose(&def.reflexes.sleepy, world, input) {
                        self.events.push(PetEvent::Napping);
                        self.enter(next);
                    }
                }
            }
        } else if input.away() && !self.is_asleep() && self.can_react() {
            // Réveillé pendant que l'utilisateur est toujours parti : il se
            // rendort un peu plus tard.
            self.nap_in = Some(self.rng.range(NAP_SPREAD * 0.5, NAP_SPREAD * 1.5));
        }
        if let Some(left) = &mut self.wake_in {
            *left -= dt;
            if *left <= 0.0 {
                self.wake_in = None;
                if self.is_asleep() && self.leaving.is_none() {
                    let def = Arc::clone(&self.def);
                    let own = &def.behaviors[self.behavior].next;
                    let list = if def.reflexes.wake.is_empty() {
                        own
                    } else {
                        &def.reflexes.wake
                    };
                    if let Some(next) = self.choose(list, world, input) {
                        self.events.push(PetEvent::Woken);
                        self.enter(next);
                    }
                }
            }
        }
    }

    // ------------------------------------------------------------- simulation

    pub fn step(&mut self, dt: f32, world: &World, input: &Input) {
        self.step_with(dt, world, input, None);
    }

    /// Un pas en physique réaliste : le monde est celui du suivi des
    /// fenêtres (positions lissées, vitesses, dos des autres animaux).
    pub fn step_real(&mut self, dt: f32, tracker: &Tracker, input: &Input) {
        self.step_with(dt, tracker.world(), input, Some(tracker));
    }

    fn step_with(&mut self, dt: f32, world: &World, input: &Input, tracker: Option<&Tracker>) {
        if self.realistic && tracker.is_none() {
            self.leave_realistic();
        } else if !self.realistic && tracker.is_some() {
            self.enter_realistic();
        }
        self.realistic = tracker.is_some();
        self.family.step(dt);
        self.prev_pos = self.pos;
        self.prev_lift = self.tilt_lift();
        self.ground_lift = self.prev_lift;
        self.prev_smooth = self.smooth;
        self.follow_back_top(dt);
        self.bump_cooldown = (self.bump_cooldown - dt).max(0.0);
        self.meet_cooldown = (self.meet_cooldown - dt).max(0.0);
        self.step_social(dt);
        self.startle_cooldown = (self.startle_cooldown - dt).max(0.0);
        self.step_moods(dt, world, input);
        self.step_echo(dt);
        self.step_takeoff(dt, world);
        self.step_regroup(dt);
        if let Some(leaving) = &mut self.leaving {
            leaving.time += dt;
            if leaving.fade.is_none() && leaving.time >= LEAVE_MAX {
                leaving.fade = Some(1.0);
            }
            if let Some(fade) = &mut leaving.fade {
                *fade -= dt / FADE_TIME;
                if *fade <= 0.0 {
                    self.finished = true;
                }
            }
        }
        self.update_screen(world);
        let scripted = matches!(self.def.behaviors[self.behavior].motion, Motion::Script(_));
        match (tracker, self.support) {
            (Some(tracker), _) => self.step_physics(dt, tracker, input, scripted),
            (None, Support::Held) => self.step_held(dt),
            (None, _) if scripted => self.step_script(dt, world, input),
            (None, Support::Air) => self.step_air(dt, world, input),
            (None, Support::Ground { surface }) => self.step_ground(dt, world, input, surface),
        }
        self.step_behavior(dt, world, input);
        self.step_anim(dt, input);
        self.step_particles(dt);
        self.update_shadow(world);
        self.prev_vel_x = self.vel.x;
        self.prev_world_vel = self.vel + self.carry;
        // Le dessin rejoint son corps.
        if self.smooth != Vec2::ZERO {
            let left = self.smooth.length();
            let catch_up = (left * (1.0 - (-dt / SMOOTH_TIME).exp())).min(SMOOTH_SPEED * dt);
            self.smooth *= 1.0 - catch_up / left;
            if left - catch_up < 0.05 {
                self.smooth = Vec2::ZERO;
            }
        }
    }

    /// Son dessin est là où il est (rien à rattraper).
    fn clear_smooth(&mut self) {
        self.smooth = Vec2::ZERO;
        self.prev_smooth = Vec2::ZERO;
    }

    /// Le replace d'un coup à l'abscisse `x` (un mur, un bord de l'écran, un
    /// bout de rebord) : son dessin y glisse (voir `Pet::smooth`). Revenu de
    /// plus loin que sa largeur (un animal desktopPet sorti de l'écran, qui
    /// y rebondit), son dessin repart d'à côté, encore hors de vue, et
    /// rentre en glissant.
    pub(super) fn snap_x(&mut self, x: f32) {
        let moved = x - self.pos.x;
        self.pos.x = x;
        self.smooth.x -= moved;
        let reach = self.def.physics.width;
        if self.smooth.x.abs() > reach {
            let cut = self.smooth.x - self.smooth.x.signum() * reach;
            self.smooth.x -= cut;
            self.prev_smooth.x -= cut;
        }
    }

    /// Comme [`Self::snap_x`], en hauteur (la marche entre les sols de deux
    /// écrans voisins).
    fn snap_y(&mut self, y: f32) {
        let moved = y - self.pos.y;
        if moved == 0.0 {
            return;
        }
        self.pos.y = y;
        self.smooth.y -= moved;
        let reach = self.def.physics.height;
        if self.smooth.y.abs() > reach {
            let cut = self.smooth.y - self.smooth.y.signum() * reach;
            self.smooth.y -= cut;
            self.prev_smooth.y -= cut;
        }
    }

    /// La marche qu'il passe en marchant, entre les sols de deux écrans
    /// voisins (points) : un tiers de sa hauteur, une douzaine de points au
    /// moins (deux barres des tâches d'échelles différentes n'ont pas tout
    /// à fait la même hauteur).
    fn floor_step(&self) -> f32 {
        (self.def.physics.height * 0.35).max(12.0)
    }

    fn step_ground(&mut self, dt: f32, world: &World, input: &Input, surface_id: SurfaceId) {
        // Ici, c'est sa vitesse qui porte l'élan.
        self.slide = 0.0;
        let Some(surface) = world.surface(surface_id).copied() else {
            self.lift_off(world, input);
            return;
        };

        self.ride(&surface);
        self.pos.y = surface.y;
        self.walk_on(dt, world, input, &surface);
    }

    /// Ce qu'il fait sur la surface qui le porte : il marche (ou glisse
    /// jusqu'à s'arrêter), réagit aux côtés des fenêtres, aux bouts du
    /// rebord et aux bords de l'écran. `vel` est sa vitesse par rapport à
    /// la surface.
    fn walk_on(&mut self, dt: f32, world: &World, input: &Input, surface: &Surface) {
        let p = self.def.physics;
        // Les bords de l'écran, là où il est : ceux qu'un corps de sa
        // hauteur ne passe pas (voir `World::span`). Une petite marche
        // entre les sols de deux écrans voisins ne l'arrête pas.
        let step = self.floor_step();
        let (left, right) = world.span(self.pos.x, self.pos.y - p.height, self.pos.y - step);
        let too_wide = p.width >= right - left;
        let surface = *surface;
        // Un saut sur un rebord : il le mesure, recule (ou s'avance) jusqu'à
        // son point d'envol, se ramasse (voir `pet/leap.rs`).
        let leaping = matches!(
            self.def.behaviors[self.behavior].motion,
            Motion::Leap { .. }
        );
        let leap_speed = if leaping && !too_wide {
            self.leap_ground(dt, world, &surface)
        } else {
            0.0
        };
        let motion = &self.def.behaviors[self.behavior].motion;
        let grazing = *motion == Motion::Graze;
        // À reculons, l'arrière va devant.
        let backing = matches!(motion, Motion::Back { .. }) || leap_speed < 0.0;
        let ahead = if backing { -self.facing } else { self.facing };
        let desired = if too_wide {
            0.0
        } else if grazing {
            self.graze_step() * self.facing
        } else if leaping {
            leap_speed * self.facing
        } else {
            motion.speed().map_or(0.0, |speed| speed * ahead)
        };
        let desired = self.family_speed(desired, dt);
        let accel = if desired == 0.0 {
            p.ground_friction
        } else {
            p.walk_accel
        };
        self.vel.x = approach(self.vel.x, desired, accel * dt);
        self.vel.y = 0.0;
        // Le côté d'une fenêtre barre le chemin : il réagit, ou passe devant.
        // En route vers son point d'envol, il s'y arrête.
        if desired != 0.0
            && leaping
            && let Some(wall) = world.wall_crossed(
                self.pos.x + ahead * p.width * 0.5,
                self.pos.x + ahead * p.width * 0.5 + self.vel.x * dt,
                self.pos.y - p.height,
                self.pos.y,
            )
        {
            let x = wall.x - ahead * p.width * 0.5;
            self.snap_x(x);
            self.vel.x = 0.0;
        } else if desired != 0.0
            && !leaping
            && let Some((next, x)) = self.window_side(self.vel.x * dt, world, input, true)
        {
            self.snap_x(x);
            self.vel.x = 0.0;
            self.enter(next);
            return;
        }
        self.family_step(dt);

        // Bords de la surface : le museau prévient avant que les pattes n'y arrivent,
        // pour que l'animal se retourne sans déborder sur une fenêtre voisine.
        // Le sol d'un écran voisin à la même hauteur prolonge le chemin ; à
        // une petite marche près aussi, qu'il monte ou descend sans y penser.
        let slide_margin = p.width * 0.2;
        let nose = self.pos.x + ahead * p.width * 0.32;
        let level = if world.is_plain() { 1.0 } else { step.max(1.0) };
        let next_floor = |x: f32| {
            world
                .floor_at(x, surface.y)
                .filter(|f| {
                    surface.kind == SurfaceKind::Floor
                        && f.id != surface.id
                        && (f.y - surface.y).abs() < level
                })
                .copied()
        };
        let heading_out = ((nose > surface.x1 && self.vel.x > 0.0)
            || (nose < surface.x0 && self.vel.x < 0.0))
            && next_floor(nose).is_none();
        if (grazing || backing || leaping) && heading_out {
            // Il broute au bord, ou y recule : il ne va pas plus loin.
            self.vel.x = 0.0;
        } else if !self.walking_off && desired != 0.0 && heading_out {
            self.on_edge(world, input, &surface);
        } else if !surface.contains_x(self.pos.x, 0.0)
            && let Some(next) = next_floor(self.pos.x)
        {
            // (Une marche : il est déjà sur l'autre sol, son dessin le
            // rejoint en glissant.)
            self.snap_y(next.y);
            self.set_ground(&next);
        } else if self.pos.x > surface.x1 + slide_margin || self.pos.x < surface.x0 - slide_margin {
            self.lift_off(world, input);
            return;
        }

        // Bords de l'écran.
        let half = p.width * 0.5;
        if too_wide {
            // Trop large pour tenir entre les deux bords : un seul appui
            // au milieu, sans alterner entre deux corrections incompatibles.
            self.snap_x(left + (right - left) * 0.5);
            self.vel.x = 0.0;
        } else if self.pos.x - half < left {
            self.snap_x(left + half);
            if self.vel.x < 0.0 {
                self.vel.x = 0.0;
                if !backing {
                    self.on_wall(world, input);
                }
            }
        } else if self.pos.x + half > right {
            self.snap_x(right - half);
            if self.vel.x > 0.0 {
                self.vel.x = 0.0;
                if !backing {
                    self.on_wall(world, input);
                }
            }
        }

        self.reach_goal();
    }

    /// Arrivé où il allait (sa place dans le troupeau qui se regroupe) : il
    /// s'arrête.
    pub(super) fn reach_goal(&mut self) {
        if let Some(goal) = self.goal
            && (goal - self.pos.x) * self.facing <= 0.0
        {
            self.goal = None;
            let stand = self.def.default_ground();
            self.enter(stand);
        }
    }

    fn step_air(&mut self, dt: f32, world: &World, input: &Input) {
        self.slide = 0.0;
        let p = self.def.physics;
        self.vel.y = (self.vel.y + p.gravity * dt).min(p.terminal_velocity);
        self.vel.x *= (-p.air_drag * dt).exp();
        // Le vent le fait dériver, à proportion de sa taille.
        self.vel.x += self.wind * dt * (p.height / 60.0).clamp(0.5, 2.0);
        let from = self.pos;
        let mut to = self.pos + self.vel * dt;

        // Les bords de l'écran, à sa hauteur : il rebondit là où il n'y a
        // plus d'écran pour le montrer en entier (voir `World::span`).
        let (left, right) = world.span(from.x, to.y - p.height, to.y);
        let half = p.width * 0.5;
        let wanted = to.x;
        if p.width >= right - left {
            to.x = left + (right - left) * 0.5;
            self.vel.x = 0.0;
        } else if to.x - half < left {
            to.x = left + half;
            self.vel.x = self.vel.x.abs() * 0.45;
        } else if to.x + half > right {
            to.x = right - half;
            self.vel.x = -self.vel.x.abs() * 0.45;
        }
        // Ramené de loin (lâché là où il n'y a pas d'écran, ou resté là
        // quand un écran a disparu) : son dessin le rejoint en glissant.
        if (to.x - wanted).abs() > half {
            self.smooth.x -= to.x - wanted;
            let reach = p.width;
            if self.smooth.x.abs() > reach {
                let cut = self.smooth.x - self.smooth.x.signum() * reach;
                self.smooth.x -= cut;
                self.prev_smooth.x -= cut;
            }
        }
        // Lancé vers le haut, il peut monter jusqu'au bord de l'écran, en
        // passant derrière la barre des menus (et l'encoche), puis retombe.
        let ceiling = world.ceiling(to.x - half, to.x + half, from.y) + p.height;
        if to.y < ceiling {
            to.y = ceiling;
            self.vel.y = self.vel.y.max(0.0);
        }

        if self.vel.y >= 0.0 {
            // (Le sol qu'il traverse pour gagner l'écran du dessous ne le
            // retient pas : voir `pet/voyage.rs`.)
            let passing = self.passing;
            if let Some(s) = world
                .landing_surface(to.x, p.width * LANDING_REACH, from.y, to.y)
                .copied()
                .filter(|s| Some(s.id) != passing)
            {
                self.pos = Vec2::new(to.x, s.y);
                self.land(&s, world, input);
                return;
            }
            // Plus bas que tous les sols : il y est ramené.
            if let Some(floor) = world
                .floor_missed(to.x, to.y)
                .copied()
                .filter(|floor| Some(floor.id) != passing)
            {
                self.pos = Vec2::new(clamp_between(to.x, floor.x0, floor.x1), floor.y);
                self.land(&floor, world, input);
                return;
            }
        }
        self.pos = to;
    }

    fn step_held(&mut self, dt: f32) {
        self.slide = 0.0;
        let Some(hold) = &self.hold else {
            self.support = Support::Air;
            return;
        };
        // Comme une fenêtre qu'on déplace : le point par lequel on l'a pris
        // reste sous le curseur, sans retard ni glissement.
        let target = hold.target - hold.offset;
        // Sa vitesse est celle du geste, lissée sur quelques centièmes de
        // seconde : elle donne l'élan au lâcher (rien si la main s'est
        // arrêtée) et le balancement du dessin.
        if dt > 0.0 {
            let gesture = (target - self.pos) * (1.0 / dt);
            self.vel = self.vel.lerp(gesture, 1.0 - (-dt / THROW_SMOOTHING).exp());
        }
        self.pos = target;
    }

    fn land(&mut self, surface: &Surface, world: &World, input: &Input) {
        let p = self.def.physics;
        let impact = self.vel.y.max(0.0);
        self.touch_down(impact);
        if impact > p.bounce_min_speed && p.bounce > 0.0 {
            self.vel.y = -impact * p.bounce;
            self.vel.x *= 0.75;
            return;
        }
        self.vel.x *= 0.5;
        self.settle(surface, impact, world, input);
    }

    /// Ce qui se voit et s'entend quand il touche une surface : l'événement,
    /// l'écrasement, les oreilles, la poussière.
    fn touch_down(&mut self, impact: f32) {
        self.events.push(PetEvent::Landed { impact });
        self.squash_kick((impact * 0.0016).min(3.2));
        self.anim.ear.kick((impact * 0.004).min(6.0));
        if impact > 420.0 {
            self.emit_dust(impact);
        }
    }

    /// Il se pose sur `surface` (après un choc d'`impact` pt/s) : il y reste,
    /// tourné vers où il allait, et choisit sa réaction d'atterrissage.
    fn settle(&mut self, surface: &Surface, impact: f32, world: &World, input: &Input) {
        let p = self.def.physics;
        // Lancé, il se pose tourné vers où il allait. Heurté, il est parti à
        // reculons et garde son sens ; un animal desktopPet, qui n'a pas d'idée
        // à lui pour aller ailleurs, foncerait de nouveau sur l'autre : il
        // tourne le dos au choc.
        if self.def.behaviors[self.behavior].motion == Motion::Fall
            && self.vel.x.abs() > 60.0
            && (!self.knocked || self.def.rules == Rules::DesktopPet)
        {
            self.facing = self.vel.x.signum();
        }
        self.knocked = false;
        // La culbute s'achève vite si le sol arrive avant la fin du tour.
        if self.anim.roll_left != 0.0 {
            self.anim.roll_rate = self.anim.roll_rate.max(self.anim.roll_left.abs() / 0.15);
        }
        self.vel.y = 0.0;
        // Au bout d'un saut sur un rebord, ses sabots se plantent.
        self.leap_landed();
        self.slide = self.vel.x.clamp(-MAX_SLIDE, MAX_SLIDE);
        self.set_ground(surface);
        let def = Arc::clone(&self.def);
        let own = &def.behaviors[self.behavior].on_land;
        let hard = impact >= p.hard_landing_speed && !def.reflexes.land_hard.is_empty();
        let list = if !own.is_empty() {
            own
        } else if hard {
            &def.reflexes.land_hard
        } else {
            &def.reflexes.land
        };
        let next = self
            .choose(list, world, input)
            .unwrap_or(def.default_ground());
        self.enter(next);
    }

    fn lift_off(&mut self, world: &World, input: &Input) {
        self.support = Support::Air;
        self.ground_ref = None;
        self.walking_off = false;
        self.take_off();
        let def = Arc::clone(&self.def);
        let behavior = &def.behaviors[self.behavior];
        if !behavior.motion.is_airborne() {
            let list = if behavior.on_fall.is_empty() {
                &def.reflexes.fall
            } else {
                &behavior.on_fall
            };
            let next = self.choose(list, world, input).or(def.default_air());
            if let Some(next) = next {
                self.enter(next);
            }
        }
    }

    fn on_edge(&mut self, world: &World, input: &Input, surface: &Surface) {
        let def = Arc::clone(&self.def);
        let own = &def.behaviors[self.behavior].on_edge;
        let list = if own.is_empty() {
            &def.reflexes.edge
        } else {
            own
        };
        // Un rebord plus haut juste devant : il peut y sauter, s'il est à
        // sa portée.
        self.offer_leap(world, self.facing);
        let next = self.choose(list, world, input);
        self.close_leap_offer();
        match next {
            Some(next) => self.enter(next),
            None if def.behaviors[self.behavior].walk_off_edges => {}
            None => self.facing = -self.facing,
        }
        let reach = self.def.physics.width * 0.32;
        let nose = self.pos.x + self.facing * reach;
        let outward =
            (nose > surface.x1 && self.facing > 0.0) || (nose < surface.x0 && self.facing < 0.0);
        let moving = self.def.behaviors[self.behavior].motion.speed().is_some();
        if outward && moving {
            // Il a choisi de continuer : il va tomber du rebord.
            self.walking_off = true;
        } else {
            // Garde le museau sur le rebord (sans dépasser le milieu si le rebord est étroit).
            let lo = (surface.x0 + reach).min(surface.x0 + surface.width() * 0.5);
            let hi = (surface.x1 - reach).max(surface.x1 - surface.width() * 0.5);
            self.snap_x(clamp_between(self.pos.x, lo, hi));
            if !matches!(
                def.behaviors[self.behavior].motion,
                Motion::Jump { .. } | Motion::Pronk { .. }
            ) {
                self.vel.x = 0.0;
            }
        }
    }

    fn on_wall(&mut self, world: &World, input: &Input) {
        let def = Arc::clone(&self.def);
        // Derrière ce bord, le sol d'un écran voisin, un peu plus haut :
        // comme devant le côté d'une fenêtre plus haute, il peut y sauter
        // s'il est à sa portée (voir `pet/leap.rs`).
        if !world.is_plain() && !def.reflexes.window_side.is_empty() {
            self.offer_leap(world, self.facing);
            let offered = self.leap_offer.is_some();
            let next = offered
                .then(|| self.choose(&def.reflexes.window_side, world, input))
                .flatten();
            self.close_leap_offer();
            if offered {
                match next {
                    Some(next) => self.enter(next),
                    None => self.facing = -self.facing,
                }
                return;
            }
        }
        let own = &def.behaviors[self.behavior].on_wall;
        let list = if own.is_empty() {
            &def.reflexes.wall
        } else {
            own
        };
        match self.choose(list, world, input) {
            Some(next) => self.enter(next),
            None => self.facing = -self.facing,
        }
    }

    fn step_behavior(&mut self, dt: f32, world: &World, input: &Input) {
        if self.family.action.active() && matches!(self.support, Support::Ground { .. }) {
            return;
        }
        self.behavior_time += dt;
        let def = Arc::clone(&self.def);
        let behavior = &def.behaviors[self.behavior];
        if let Some(clip) = &behavior.clip {
            self.clip_timer += dt;
            loop {
                let interval = self.clip_interval();
                if self.clip_timer < interval {
                    break;
                }
                self.clip_timer -= interval;
                self.clip_step += 1;
                let looping = behavior.duration != Duration::Clip || self.support == Support::Held;
                if looping || self.clip_step < self.clip_total {
                    self.cue_sounds(Some(clip.frame_index_at(self.clip_step)));
                }
            }
        }
        let desktoppet = def.rules == Rules::DesktopPet;
        let finished = match behavior.duration {
            Duration::Range(..) => self.behavior_time >= self.behavior_length,
            // desktopPet : T pas, puis un dernier qui garde la dernière image.
            Duration::Clip => {
                behavior.clip.is_none()
                    || self.clip_step >= self.clip_total + usize::from(desktoppet)
            }
            Duration::Forever => false,
        };
        if !finished || self.support == Support::Held || self.finished {
            return;
        }
        // L'animation d'adieu est finie : il s'efface sur sa dernière image.
        if let Some(leaving) = &mut self.leaving {
            leaving.fade.get_or_insert(1.0);
            return;
        }
        if behavior.flip_at_end {
            self.facing = -self.facing;
            // Les images ont déjà montré le demi-tour : pas de transition en plus.
            self.anim.flip = self.facing;
        }
        let scripted = matches!(behavior.motion, Motion::Script(_));
        if scripted && self.outside(world) {
            if self.is_child() {
                self.finished = true;
            } else {
                self.respawn(world);
            }
            return;
        }
        // En route vers sa place dans le troupeau : il continue.
        if let Some(goal) = self.goal
            && (goal - self.pos.x) * self.facing > 0.0
            && self.support != Support::Air
        {
            let index = self.behavior;
            self.enter(index);
            self.goal = Some(goal);
            self.began = None;
            return;
        }
        if self.take_echo() {
            return;
        }
        match self.choose(&behavior.next, world, input) {
            Some(next) => self.enter(next),
            // Un personnage secondaire s'en va ; un animal desktopPet réapparaît.
            None if self.is_child() => self.finished = true,
            None if scripted && desktoppet => self.respawn(world),
            None => {
                let next = if self.support == Support::Air {
                    self.behavior
                } else {
                    def.default_ground()
                };
                self.enter(next);
            }
        }
    }

    /// Sons du comportement en cours prévus au début (`frame` vide) ou sur
    /// cette image de sa séquence, chacun selon sa chance.
    fn cue_sounds(&mut self, frame: Option<usize>) {
        if self.leaving.is_some_and(|l| l.fade.is_some()) {
            return;
        }
        let def = Arc::clone(&self.def);
        for cue in &def.behaviors[self.behavior].sounds {
            if cue.frame == frame && self.voice.chance(cue.chance) {
                let clips = def.sounds.get(cue.sound).map_or(0, |s| s.clips.len());
                if clips > 0 {
                    if def.sounds[cue.sound].mouth {
                        self.anim.voice_left = VOICE_TIME;
                    }
                    let clip = self.voice.next_u32() as usize % clips;
                    self.events.push(PetEvent::Sound {
                        sound: cue.sound,
                        clip,
                    });
                }
            }
        }
    }

    /// Durée du pas en cours d'une séquence de sprites (interpolée sur T pas).
    fn clip_interval(&self) -> f32 {
        let Some(clip) = &self.def.behaviors[self.behavior].clip else {
            return f32::INFINITY;
        };
        let t = (self.clip_step as f32 / self.clip_total.max(1) as f32).min(1.0);
        lerp(clip.interval_start, clip.interval_end, t).max(0.016)
    }

    /// Entièrement sorti des écrans (desktopPet le fait alors réapparaître).
    fn outside(&self, world: &World) -> bool {
        let p = self.def.physics;
        !world.shows(Rect::new(
            self.pos.x - p.width * 0.5,
            self.pos.y - p.height,
            p.width,
            p.height,
        ))
    }

    fn enter(&mut self, index: usize) {
        let def = Arc::clone(&self.def);
        let mut index = index.min(def.behaviors.len() - 1);
        if !self.behavior_allowed(index) {
            index = def.default_ground();
        }
        // Un flairage ne vaut que pour la suite immédiate, et une seule
        // fois. Une rencontre qui commence Sniff remplira ensuite la mémoire.
        self.family.stop();
        self.sniffed_ewe_for = 0.0;
        let grounded = matches!(self.support, Support::Ground { .. });
        if grounded && def.behaviors[index].motion == Motion::Fall {
            index = def.default_ground();
        }
        let behavior = &def.behaviors[index];
        self.behavior = index;
        self.behavior_time = 0.0;
        self.began = (behavior.contagious > 0.0).then_some(index);
        self.takeoff = None;
        // Un saut sur un rebord : celui qu'on lui propose (sinon, il le
        // cherchera devant lui) ; tout autre comportement l'abandonne.
        if matches!(behavior.motion, Motion::Leap { .. }) {
            self.begin_leap();
        } else {
            self.leap = None;
        }
        self.leap_offer = None;
        self.goal = None;
        if def.rules != Rules::DesktopPet && !behavior.on_meet.is_empty() {
            // Son auteur a prévu une rencontre (deux agneaux qui s'élancent
            // l'un vers l'autre) : elle compte, même juste après la dernière.
            self.meet_cooldown = 0.0;
        }
        if (behavior.sleeping || behavior.motion == Motion::Sleep)
            && !matches!(def.appearance, Appearance::Sprites(_))
            && self.anim.sleep < 0.5
        {
            // Une fois sur deux, il dormira la tête posée au sol.
            self.anim.sleep_low_target = if self.rng.chance(0.5) { 1.0 } else { 0.0 };
        }
        self.clip_step = 0;
        self.clip_timer = 0.0;
        self.clip_total = match &behavior.clip {
            Some(c) => {
                let repeat = match &c.repeat_expr {
                    Some(expr) => self.eval(expr).trunc().clamp(0.0, 100_000.0) as u32,
                    None => c.repeat,
                };
                c.total_steps_with(repeat).max(1)
            }
            None => 1,
        };
        self.behavior_length = match behavior.duration {
            Duration::Range(min, max) => {
                self.rng.range(min, max) * self.temper.patience(def.energy(index))
            }
            Duration::Clip => behavior.clip.as_ref().map_or(1.0, |c| {
                self.clip_total as f32 * (c.interval_start + c.interval_end) * 0.5
            }),
            Duration::Forever => f32::INFINITY,
        };
        self.cue_sounds(None);
        // L'image du premier pas peut aussi avoir son son.
        if behavior.clip.is_some() {
            self.cue_sounds(Some(0));
        }
        match &behavior.motion {
            Motion::Turn => {
                self.facing = -self.facing;
                // Petit sursaut pendant le demi-tour : il se lit mieux qu'un retournement à plat.
                self.squash_kick(-1.1);
            }
            &(Motion::Jump { vx, vy } | Motion::Pronk { vx, vy }) if grounded => {
                let vel = Vec2::new(vx * self.facing, -vy);
                if matches!(def.appearance, Appearance::Sprites(_)) {
                    self.launch(vel);
                } else {
                    // Un animal dessiné se ramasse d'abord : l'anticipation.
                    self.takeoff = Some((TAKEOFF_CROUCH, vel));
                }
            }
            Motion::Look => self.anim.glance_timer = 0.0,
            Motion::Script(script) => {
                // Valeurs entières comme dans desktopPet, puis mises à la taille d'affichage.
                let k = script.scale;
                let mut value = |e: &Expr| self.eval(e).trunc() as f32 * k;
                let start = Vec2::new(value(&script.start[0]), value(&script.start[1]));
                let end = Vec2::new(value(&script.end[0]), value(&script.end[1]));
                self.script = [start, end];
            }
            _ => {}
        }
        // Personnages secondaires (desktopPet : cinq générations au plus).
        if !behavior.children.is_empty() && self.child_depth < 5 {
            let p = def.physics;
            let (frame, _) = self.screen.unwrap_or_default();
            let flipped = self.facing > 0.0;
            for c in &behavior.children {
                let x = self.eval_position(if flipped { &c.x_flipped } else { &c.x });
                let y = self.eval_position(&c.y);
                self.children.push(ChildRequest {
                    behavior: c.next,
                    pos: Vec2::new(frame.x + x + p.width * 0.5, frame.y + y + p.height),
                    facing: self.facing,
                    depth: self.child_depth + 1,
                    skin: self.skin,
                    rand_s: self.rand_s,
                });
            }
        }
    }

    fn update_screen(&mut self, world: &World) {
        self.screen = world.screen_at(self.pos).map(|s| (s.frame, s.usable));
    }

    /// Évalue une expression desktopPet avec les mesures de l'écran et de
    /// l'animal, en pixels du fichier d'origine (voir [`PetDef::unit`]) : ses
    /// positions et ses nombres de pas restent ceux prévus pour des images à
    /// leur taille d'origine, quelle que soit la taille d'affichage.
    fn eval(&mut self, expr: &Expr) -> f64 {
        let (frame, usable) = self.screen.unwrap_or_default();
        let p = self.def.physics;
        let unit = f64::from(self.def.unit).max(1e-6);
        let behavior = &self.def.behaviors[self.behavior];
        let offset = behavior.clip.as_ref().map_or(0.0, |c| c.offset_y);
        let image_x = f64::from(self.pos.x - p.width * 0.5 - frame.x);
        let image_y = f64::from(self.pos.y - p.height + offset - frame.y);
        let rand_s = f64::from(self.rand_s);
        // Un seul tirage par expression : toutes les occurrences de `random` valent pareil.
        let random = f64::from(self.rng.next_u32() % 100);
        expr.eval(&mut |name| {
            Some(match name {
                "screenW" => f64::from(frame.w) / unit,
                "screenH" => f64::from(frame.h) / unit,
                "areaW" => f64::from(usable.right() - frame.x) / unit,
                "areaH" => f64::from(usable.bottom() - frame.y) / unit,
                "imageW" => f64::from(p.width) / unit,
                "imageH" => f64::from(p.height) / unit,
                "imageX" => image_x / unit,
                "imageY" => image_y / unit,
                "random" => random,
                "randS" => rand_s,
                // Le zoom de desktopPet : ses images sont ici à leur taille d'origine.
                "scale" => 1.0,
                _ => return None,
            })
        })
    }

    /// Une position desktopPet (pixels entiers du fichier), en points.
    fn eval_position(&mut self, expr: &Expr) -> f32 {
        self.eval(expr).trunc() as f32 * self.def.unit
    }

    fn choose(&mut self, list: &[Transition], world: &World, input: &Input) -> Option<usize> {
        self.choose_with_partner(list, world, input, None)
    }

    fn choose_with_partner(
        &mut self,
        list: &[Transition],
        world: &World,
        input: &Input,
        partner: Option<&Pet>,
    ) -> Option<usize> {
        let candidates: Vec<&Transition> = list
            .iter()
            .filter(|t| self.transition_allowed(t, world, input))
            .collect();
        // Son tempérament penche vers certaines allures (voir `temper`).
        let temper = self.temper;
        let def = &self.def;
        // Calcule les poids avant d'emprunter le générateur : aucune
        // allocation de famille ni aucun tirage additionnel.
        let social = self.contest_appetite(partner);
        let i = self.rng.pick_weighted(candidates.iter().map(|t| {
            let contest = matches!(
                def.behaviors[t.to].motion,
                Motion::Back { .. } | Motion::Charge { .. } | Motion::Gesture(Gesture::Rear)
            );
            t.weight * temper.appetite(def.energy(t.to)) * if contest { social } else { 1.0 }
        }))?;
        Some(candidates[i].to)
    }

    fn condition(&self, condition: Condition, world: &World, input: &Input) -> bool {
        let surface_kind = match self.support {
            Support::Ground { surface } => world.surface(surface).map(|s| s.kind),
            _ => None,
        };
        let night = input.local_hour >= 22.0 || input.local_hour < 6.0;
        let center = self.pos - Vec2::new(0.0, self.def.physics.height * 0.5);
        let cursor_dist = input.cursor.map(|c| (c - center).length());
        match condition {
            Condition::OnWindow => surface_kind == Some(SurfaceKind::Window),
            Condition::OnFloor => surface_kind == Some(SurfaceKind::Floor),
            Condition::Night => night,
            Condition::Day => !night,
            Condition::CursorNear(r) => cursor_dist.is_some_and(|d| d <= r),
            Condition::CursorFar(r) => cursor_dist.is_none_or(|d| d > r),
            Condition::Grounded => surface_kind.is_some(),
            Condition::Typing => input.typing(),
            Condition::Away => input.away(),
            Condition::Present => !input.away(),
            Condition::Morning => (6.0..11.0).contains(&input.local_hour),
            Condition::Evening => (18.0..22.0).contains(&input.local_hour),
            Condition::Sex(sex) => self.sex == sex,
            Condition::Adult => !self.is_lamb(),
            Condition::Lamb => self.is_lamb(),
            Condition::Temper(temper) => self.temper == temper,
            // Le vent souffle contre son museau (signes opposés).
            Condition::IntoWind => self.wind * self.facing < -0.1 * crate::surprise::GUST_PEAK,
        }
    }

    fn step_anim(&mut self, dt: f32, input: &Input) {
        let p = self.def.physics;
        let grounded = matches!(self.support, Support::Ground { .. });
        let behavior = &self.def.behaviors[self.behavior];
        // Il se ramasse avant de sauter : debout, écrasé.
        let crouching = self.takeoff.is_some() && grounded;
        // Un saut sur un rebord (voir `pet/leap.rs`) : il le mesure du
        // regard, se ramasse l'arrière plus bas, bondit, se reçoit.
        let gathering = crouching && self.gathering_for_leap();
        let leap_progress = self.leap_progress().filter(|_| !grounded);
        let leap_gaze = self.leap_gaze();
        let in_leap = self.leap.is_some();
        let pose = match self.support {
            Support::Held => Pose::Dangle,
            Support::Air => Pose::Air,
            Support::Ground { .. } if crouching => Pose::Stand,
            Support::Ground { .. } => behavior.motion.pose(),
        };
        let twisting = behavior.twist && pose == Pose::Air;
        let looking = behavior.motion == Motion::Look;
        let sprites = matches!(self.def.appearance, Appearance::Sprites(_));
        let drawn = self.def.drawn();
        let realistic = self.realistic;
        // Ce que fait le comportement en cours, pour les gestes, la bouche et
        // la posture (les animaux dessinés en vecteurs ou en squelette).
        let doing = match behavior.motion {
            Motion::Gesture(gesture) if grounded => Some(gesture),
            _ => None,
        };
        let family_gesture = grounded.then(|| self.family.action.gesture()).flatten();
        let doing = family_gesture.map(|(g, _)| g).or(doing);
        let since = family_gesture.map_or(self.behavior_time, |(_, t)| t);
        let left = if family_gesture.is_some() {
            f32::INFINITY
        } else {
            self.behavior_length - self.behavior_time
        };
        let chewing = behavior.chew && grounded;
        let charging = matches!(behavior.motion, Motion::Charge { .. }) && grounded;
        let pronking = matches!(behavior.motion, Motion::Pronk { .. }) && pose == Pose::Air;
        let tilt = behavior.tilt;
        let temper = self.temper;
        // Les allures se choisissent d'après la vitesse rapportée à la
        // longueur des pattes (comme le nombre de Froude, voir
        // `docs/animation-animale.md`) : agrandi, un animal garde les siennes.
        let legs = (self.def.stride() / STRIDE_PER_LEG).max(1.0);
        let stride_walk = self.def.stride();
        // Physique réaliste : l'inertie des oreilles et de la queue quand le
        // corps accélère (une fenêtre qui l'emporte, un choc…).
        let surge = if realistic && dt > 0.0 {
            let accel = (self.vel + self.carry - self.prev_world_vel) * (1.0 / dt);
            -accel.x * self.facing
        } else {
            0.0
        };
        let inertia = surge + self.wind * self.facing * 0.5;
        // Ce qui pousse la toison vers l'avant de l'animal (négatif : vers
        // l'arrière) : l'élan du corps qui freine, le vent dans le dos.
        let fleece_push = (surge * FLEECE_SURGE).clamp(-0.3, 0.3)
            + self.wind * self.facing / crate::surprise::GUST_PEAK * FLEECE_WIND;
        let wind = self.wind;
        let fleece_pace = self.def.physics.fleece_pace();
        let Self {
            anim: a,
            family,
            rng,
            vel,
            facing,
            pos,
            particles,
            prev_vel_x,
            ..
        } = self;

        // Un dessin vectoriel pivote en douceur ; des sprites se retournent d'un coup.
        // Un demi-tour dessiné dans les images (`flip` de desktopPet) est appliqué
        // d'un coup ailleurs ; ici, les autres demi-tours pivotent en douceur,
        // plus vite pour des sprites.
        a.flip = damp(a.flip, *facing, if sprites { 30.0 } else { 15.0 }, dt);

        // Démarche calée sur la distance parcourue : les pattes ne glissent
        // pas ; à reculons, elles font leur cycle à l'envers.
        let speed = if grounded { vel.x.abs() } else { 0.0 };
        let along = if grounded { vel.x * *facing } else { 0.0 };
        // Au trot puis au galop, la foulée s'allonge (des bonds plutôt que
        // des pas pressés).
        a.stride_scale = 1.0 + TROT_STRIDE * a.trot * (1.0 - a.run) + RUN_STRIDE * a.run;
        if !sprites {
            // D'aplomb, il repart d'un antérieur (voir `legs::MOVE_OFF`).
            let starting = a.start == 0.0 && along != 0.0;
            if starting && legs::square(&a.legs) {
                a.gait_phase = legs::MOVE_OFF * TAU;
            }
            // Il repart à petits pas : arrêté, ses quatre sabots sont sous
            // lui, et ne peuvent pas prendre d'un coup leurs places de la
            // marche (étalées sur toute une foulée). La foulée commence à
            // moitié et s'allonge sur la première foulée parcourue.
            a.start = if along == 0.0 {
                0.0
            } else {
                (a.start + (along * dt).abs() / stride_walk.max(1.0)).min(1.0)
            };
            a.stride_scale *= START_STRIDE + (1.0 - START_STRIDE) * smoothstep(0.0, 1.0, a.start);
            if starting {
                legs::move_off(
                    &mut a.legs,
                    a.gait_phase,
                    a.stride_scale,
                    a.trot * (1.0 - a.run),
                    a.run,
                );
            }
        }
        let stride = stride_walk * a.stride_scale;
        a.gait_phase = wrap_phase(a.gait_phase + along / stride * TAU * dt, TAU);
        // Des images ont leurs pas dans leurs images : pas de cycle par patte.
        if !sprites {
            legs::advance(
                &mut a.legs,
                &legs::Gaiting {
                    phase: a.gait_phase,
                    travel: along * dt,
                    speed,
                    dt,
                    stride: stride_walk,
                    scale: a.stride_scale,
                    trot: a.trot * (1.0 - a.run),
                    run: a.run,
                    grounded,
                    // Couché ou en train de se coucher, au milieu d'un
                    // geste, ramassé pour sauter : il ne se remet pas
                    // d'aplomb.
                    still: matches!(pose, Pose::Sit | Pose::Sleep)
                        || a.sit > 0.02
                        || doing.is_some()
                        || crouching,
                },
            );
        }
        // Pas, trot, galop : en longueurs de patte par seconde. Les animaux
        // en images ont leurs allures dans leurs images.
        let rel = speed / legs;
        let (walk, trot, run) = if !grounded {
            (0.0, 0.0, 0.0)
        } else if sprites {
            (
                smoothstep(2.0, 36.0, speed),
                0.0,
                smoothstep(80.0, 150.0, speed),
            )
        } else {
            (
                smoothstep(0.05, 0.6, rel),
                smoothstep(2.6, 3.4, rel),
                smoothstep(4.6, 6.0, rel),
            )
        };
        a.walk = damp(a.walk, walk, 10.0, dt);
        a.trot = damp(a.trot, trot, 7.0, dt);
        a.run = damp(a.run, run, 6.0, dt);

        let weight = |w: Pose| if pose == w { 1.0 } else { 0.0 };
        a.sit = damp(a.sit, weight(Pose::Sit) + weight(Pose::Sleep), 5.0, dt);
        a.sleep = damp(a.sleep, weight(Pose::Sleep), 2.0, dt);
        a.dangle = damp(a.dangle, weight(Pose::Dangle), 9.0, dt);
        a.air = damp(a.air, weight(Pose::Air), 12.0, dt);
        a.graze = damp(a.graze, weight(Pose::Graze), 2.6, dt);
        if a.graze > 0.02 {
            // Une bouchée par seconde (0,93 à 0,96 s chez la brebis), un peu
            // irrégulière.
            let rate = TAU * (1.05 + 0.15 * (a.nibble_phase * 0.37).sin());
            a.nibble_phase = wrap_phase(a.nibble_phase + rate * dt, TAU * 8.0);
        }
        let rise = if pose == Pose::Air {
            (-vel.y / (0.3 * p.terminal_velocity)).clamp(-1.0, 1.0)
        } else {
            0.0
        };
        a.rise = damp(a.rise, rise, 12.0, dt);

        a.leap_after = (a.leap_after - dt).max(0.0);
        let leap_weight = if in_leap || a.leap_after > 0.0 {
            1.0
        } else {
            0.0
        };
        a.leap = damp(a.leap, leap_weight, 12.0, dt);
        a.leap_flight = leap_progress.unwrap_or(0.0);
        a.squash.target = if gathering {
            LEAP_SQUASH
        } else if crouching {
            TAKEOFF_SQUASH
        } else if pose == Pose::Air && !drawn.squash {
            -(vel.y.abs() / p.terminal_velocity).min(1.0) * 0.08
        } else {
            0.0
        };
        a.squash.step(dt);
        a.squash.value = a.squash.value.clamp(-0.3, 0.38);

        let accel_x = (vel.x - *prev_vel_x) / dt;
        let mut lean = (accel_x * *facing * 0.00025).clamp(-0.16, 0.16) + a.run * 0.08 + tilt;
        if pose == Pose::Air {
            lean += (vel.x * *facing * 0.00028).clamp(-0.35, 0.35);
        }
        a.lean.target = lean;
        a.lean.step(dt);
        // Sur un rebord : ramassé, l'arrière plus bas (le nez se lève) ; en
        // l'air, le corps suit son élan, le nez levé en montant, baissé vers
        // le rebord en descendant ; posé, il pique un peu du nez sur ses
        // antérieurs, qui amortissent.
        let leap_lean = if gathering {
            Some(LEAP_GATHER_LEAN)
        } else if let Some(progress) = leap_progress {
            // Il se redresse avant de toucher le rebord : il s'y pose
            // d'aplomb, les pattes tendues sous lui.
            let climb = (-vel.y).atan2(vel.x.abs().max(1.0));
            let level = 1.0 - smoothstep(0.7, 0.95, progress);
            Some((-LEAP_PITCH * climb * level).clamp(-0.6, 0.45))
        } else if grounded && a.leap_after > 0.0 {
            Some(LEAP_LANDING_LEAN * (a.leap_after / leap::LANDING))
        } else {
            None
        };
        if let Some(target) = leap_lean {
            a.lean.value = damp(a.lean.value, target, 16.0, dt);
            a.lean.velocity = 0.0;
            a.lean.target = target;
        }

        a.ear.target = if pose == Pose::Air {
            (-vel.y * 0.0007).clamp(-0.7, 0.7)
        } else {
            0.0
        } - a.sleep * 0.25
            + (inertia * 0.00018).clamp(-0.45, 0.45);
        // Au calme, une oreille frémit de temps en temps et la queue bat un
        // coup (les dessins vectoriels seulement : les images ont les leurs).
        // L'humeur en change le rythme : un énervé fouette de la queue et
        // remue les oreilles bien plus souvent qu'un calme.
        let (ear_pace, tail_pace, tail_force) = match temper {
            Temper::Even => (1.0, 1.0, 1.0),
            Temper::Calm => (1.6, 1.8, 0.8),
            Temper::Lively => (0.6, 0.6, 1.0),
            Temper::Rowdy => (0.45, 0.35, 1.4),
        };
        if grounded && a.sleep > 0.5 && !sprites {
            // Endormi, une oreille ou une patte tressaille de temps en temps
            // (le sommeil paradoxal).
            a.ear_timer -= dt;
            if a.ear_timer <= 0.0 {
                if rng.chance(0.35) {
                    a.twitch = 1.0;
                } else {
                    let side = if rng.chance(0.5) { 1.0 } else { -1.0 };
                    a.ear.kick(side * rng.range(1.2, 2.2));
                }
                a.ear_timer = rng.range(5.0, 14.0);
            }
        } else if grounded && !sprites {
            a.ear_timer -= dt;
            if a.ear_timer <= 0.0 {
                let side = if rng.chance(0.5) { 1.0 } else { -1.0 };
                a.ear.kick(side * rng.range(2.5, 4.5));
                a.ear_timer = rng.range(2.5, 8.0) * ear_pace;
            }
            a.tail_timer -= dt;
            if a.tail_timer <= 0.0 {
                let side = if rng.chance(0.5) { 1.0 } else { -1.0 };
                a.tail.kick(side * rng.range(5.0, 8.0) * tail_force);
                a.tail_timer = rng.range(2.0, 9.0) / (1.0 + a.walk) * tail_pace;
            }
        }
        if !sprites {
            let stepping = legs::stepping(&a.legs);
            step_gesture(a, doing, since, left, stepping, dt);
            family.clear_finished_nursing(grounded, a.gesture, a.gesture_amount);
            step_mouth(a, chewing, rng, dt);
            // Posture, d'après ce que disent les oreilles d'un mouton (voir
            // `docs/animation-animale.md`) : dressées, la colère (un
            // énervé) ; en arrière, la peur (tenu en l'air, pris dans le
            // vent) ou l'élan (il fonce, tête basse) ; pendantes, le calme
            // et le repas. Pattes raides d'un agneau qui saute.
            let (ears, head, droop): (f32, f32, f32) = if charging {
                (0.9, 1.0, 0.0)
            } else if leap_gaze.is_some() {
                // Il mesure le rebord : aux aguets, oreilles en avant, tête
                // haute.
                (-0.7, 0.0, 0.0)
            } else {
                match temper {
                    Temper::Rowdy => (-0.35, 0.15, 0.0),
                    Temper::Calm => (0.0, 0.08, 0.45),
                    Temper::Lively => (-0.15, 0.0, 0.0),
                    Temper::Even => (0.0, 0.0, 0.1),
                }
            };
            let gust = (wind.abs() / crate::surprise::GUST_PEAK).min(1.0);
            let ears = if pose == Pose::Dangle {
                0.75
            } else {
                ears + 0.6 * gust
            };
            let eating = a.graze > 0.3 || chewing;
            let droop = (droop + if eating { 0.5 } else { 0.0 }).min(1.0);
            let awake = 1.0 - a.sleep;
            a.ears_back = damp(a.ears_back, ears * awake, 6.0, dt);
            a.head_low = damp(a.head_low, head * awake, 5.0, dt);
            a.ears_droop = damp(a.ears_droop, droop * awake * (1.0 - a.dangle), 3.0, dt);
            a.ear_split = damp(a.ear_split, 0.0, 1.2, dt);
            // Un saut tordu n'est pas raide : il rue.
            let stiff = if pronking && !twisting { 1.0 } else { 0.0 };
            a.stiff = damp(a.stiff, stiff, 14.0, dt);
            a.twist = damp(a.twist, if twisting { 1.0 } else { 0.0 }, 10.0, dt);
            // Endormi, la tête descend lentement jusqu'au sol, s'il a choisi
            // de dormir ainsi ; une patte tressaille puis retombe.
            let low = a.sleep_low_target * smoothstep(0.6, 1.0, a.sleep);
            a.sleep_low = damp(a.sleep_low, low, 0.7, dt);
            a.twitch = damp(a.twitch, 0.0, 7.0, dt);
        }
        a.ear.step(dt);
        a.tail.target = (inertia * 0.00025).clamp(-0.6, 0.6);
        a.tail.step(dt);
        // La queue en chaîne : chaque morceau suit le précédent avec un temps
        // de retard, et l'élan descend jusqu'au bout (elle ondule).
        a.tail_mid.target = a.tail.value + a.tail_phase.sin() * (0.03 + a.walk * 0.07);
        a.tail_mid.step(dt);
        a.tail_tip.target = a.tail_mid.value;
        a.tail_tip.step(dt);
        // La toison : les mèches traînent avec la vitesse (la résistance de
        // l'air) ; ce qui pousse l'animal vers l'avant (il freine, le vent
        // dans le dos) porte leur bout vers l'avant (un angle négatif), ce
        // qui le pousse vers l'arrière (il démarre, le vent dans le museau)
        // vers l'arrière ; elles balancent au rythme des pas. Celles du bout
        // suivent les premières avec un temps de retard.
        let bounce = (0.04 * a.walk + 0.05 * a.trot + 0.07 * a.run) * (2.0 * a.gait_phase).sin();
        a.fleece.target =
            ((along * FLEECE_DRAG).clamp(-0.35, 0.35) - fleece_push + bounce).clamp(-0.6, 0.6);
        // Une longue mèche se balance plus lentement qu'une courte (voir
        // [`crate::def::Physics::fleece_pace`]).
        a.fleece.step(dt * fleece_pace);
        a.fleece_tip.target = a.fleece.value;
        a.fleece_tip.step(dt * fleece_pace);

        // Balancement dans le repère de l'écran : tiré vers la droite, le corps
        // reste en arrière, à gauche (rotation positive, y vers le bas).
        a.swing.target = if pose == Pose::Dangle && !realistic {
            (vel.x * 0.0012).clamp(-0.75, 0.75)
        } else {
            0.0
        };
        a.swing.step(dt);

        // Culbute : l'angle avance jusqu'à faire un tour complet, puis revient à zéro.
        if a.roll_left != 0.0 {
            let d = (a.roll_rate * dt).min(a.roll_left.abs()) * a.roll_left.signum();
            a.roll += d;
            a.roll_left -= d;
            if a.roll_left.abs() < 1e-4 {
                a.roll_left = 0.0;
                a.roll = 0.0;
            }
        }

        a.tail_phase = wrap_phase(a.tail_phase + dt * (1.6 + a.walk * 5.0 + a.run * 4.0), TAU);

        // Clignements, parfois doubles ; yeux clos pendant le sommeil.
        if a.sleep > 0.5 {
            a.blink = damp(a.blink, 1.0, 5.0, dt);
            a.blink_left = 0.0;
        } else if a.blink_left > 0.0 {
            a.blink_left -= dt;
            let t = 1.0 - (a.blink_left / BLINK_TIME).clamp(0.0, 1.0);
            a.blink = lid(t);
            if a.blink_left <= 0.0 {
                a.blink = 0.0;
                if a.double_blink {
                    a.double_blink = false;
                    a.blink_timer = 0.2;
                } else {
                    a.blink_timer = rng.range(2.5, 8.0);
                    a.double_blink = rng.chance(0.2);
                }
            }
        } else if !sprites && (a.flip - *facing).abs() > 1.5 {
            // Il fait demi-tour : il cligne en tournant la tête.
            a.blink_left = BLINK_TIME;
        } else {
            a.blink = damp(a.blink, 0.0, 14.0, dt);
            a.blink_timer -= dt;
            if a.blink_timer <= 0.0 {
                a.blink_left = BLINK_TIME;
            }
        }

        // Une vingtaine de respirations par minute au repos (16 à 34 chez
        // le mouton), plus lentes dans le sommeil.
        let period = lerp(3.0, 4.4, a.sleep);
        a.breath_phase = wrap_phase(a.breath_phase + TAU * dt / period, TAU);
        a.clock = wrap_phase(a.clock + dt, CLOCK_LOOP);

        // Regard : le curseur s'il est proche, sinon des coups d'œil.
        let head = *pos + Vec2::new(*facing * p.width * 0.32, -p.height * 0.72);
        let mut target = Vec2::new(0.35, 0.05);
        if looking {
            a.glance_timer -= dt;
            if a.glance_timer <= 0.0 {
                a.glance = Vec2::new(rng.range(-0.6, 1.0), rng.range(-0.7, 0.4));
                a.glance_timer = rng.range(0.6, 1.6);
            }
            target = a.glance;
        }
        if let Some(cursor) = input.cursor
            && grounded
            && a.sleep < 0.5
        {
            let d = cursor - head;
            let dist = d.length();
            if dist < 460.0 {
                let dir = d.normalize_or_zero();
                let strength = 0.45 + 0.55 * smoothstep(460.0, 80.0, dist);
                target = Vec2::new(dir.x * *facing, dir.y) * strength;
            }
        }
        // Il mesure du regard le rebord où il va sauter.
        if let Some(gaze) = leap_gaze {
            target = gaze * 0.95;
        }
        a.look = a.look.lerp(target, 1.0 - (-9.0 * dt).exp());

        // Petites bouffées de poussière derrière les pattes quand il court.
        let ground_speed = if grounded { vel.x.abs() } else { 0.0 };
        if ground_speed > 70.0 * (p.height / 60.0).max(0.4) && !drawn.dust {
            a.dust_timer -= dt;
            if a.dust_timer <= 0.0 && particles.len() + 2 <= MAX_PARTICLES {
                a.dust_timer = rng.range(0.16, 0.3);
                let back = -vel.x.signum();
                for _ in 0..2 {
                    particles.push(Particle {
                        kind: ParticleKind::Dust,
                        pos: *pos + Vec2::new(back * rng.range(2.0, p.width * 0.3), -1.0),
                        vel: Vec2::new(back * rng.range(18.0, 40.0), -rng.range(6.0, 22.0)),
                        age: 0.0,
                        life: rng.range(0.35, 0.55),
                        size: rng.range(1.6, 3.0) * (p.height / 60.0).clamp(0.5, 1.5),
                        phase: rng.range(0.0, TAU),
                    });
                }
            }
        } else {
            a.dust_timer = 0.05;
        }

        // Un coup de vent emporte des feuilles devant lui.
        let gust = wind.abs() / crate::surprise::GUST_PEAK;
        if gust > 0.25 {
            a.leaf_timer -= dt * gust;
            if a.leaf_timer <= 0.0 && particles.len() < MAX_PARTICLES {
                a.leaf_timer = rng.range(0.08, 0.22);
                let dir = wind.signum();
                // Elles traversent les abords de l'animal (sa fenêtre est plus
                // large) et s'effacent avant d'en sortir.
                let half = (p.width * 0.95).max(p.height * 1.2).max(55.0);
                let speed = rng.range(170.0, 320.0) * gust.max(0.5);
                particles.push(Particle {
                    kind: ParticleKind::Leaf,
                    pos: *pos + Vec2::new(-dir * half, -rng.range(-4.0, p.height * 1.3)),
                    vel: Vec2::new(dir * speed, 0.0),
                    age: 0.0,
                    life: 2.0 * half / speed,
                    size: rng.range(4.0, 6.5) * (p.height / 60.0).clamp(0.6, 1.5),
                    phase: rng.range(0.0, TAU),
                });
            }
        }

        // Bulles de sommeil.
        if a.sleep > 0.85 {
            a.bubble_timer -= dt;
            if a.bubble_timer <= 0.0 && particles.len() < MAX_PARTICLES {
                a.bubble_timer = rng.range(1.5, 2.6);
                particles.push(Particle {
                    kind: ParticleKind::Bubble,
                    pos: *pos + Vec2::new(*facing * p.width * 0.36, -p.height * 0.8),
                    vel: Vec2::new(*facing * 4.0, -16.0),
                    age: 0.0,
                    life: rng.range(2.6, 3.4),
                    size: rng.range(4.2, 6.2),
                    phase: rng.range(0.0, TAU),
                });
            }
        }
    }

    fn emit_dust(&mut self, impact: f32) {
        if self.def.drawn().dust {
            return;
        }
        let p = self.def.physics;
        let count = (4.0 + impact / 180.0).min(12.0) as usize;
        for i in 0..count {
            if self.particles.len() >= MAX_PARTICLES {
                break;
            }
            let side = if i % 2 == 0 { 1.0 } else { -1.0 };
            let speed = self.rng.range(40.0, 70.0 + impact * 0.05);
            self.particles.push(Particle {
                kind: ParticleKind::Dust,
                pos: self.pos + Vec2::new(side * self.rng.range(4.0, p.width * 0.32), -1.0),
                vel: Vec2::new(side * speed, -self.rng.range(10.0, 45.0)),
                age: 0.0,
                life: self.rng.range(0.45, 0.8),
                size: self.rng.range(3.0, 5.5),
                phase: self.rng.range(0.0, TAU),
            });
        }
    }

    fn step_particles(&mut self, dt: f32) {
        for p in &mut self.particles {
            p.age += dt;
            match p.kind {
                ParticleKind::Dust => {
                    p.vel *= (-4.5 * dt).exp();
                    p.vel.y += 20.0 * dt;
                }
                ParticleKind::Bubble => {
                    p.vel.x = (p.age * 2.3 + p.phase).sin() * 9.0;
                }
                ParticleKind::Leaf => {
                    // Elle voltige : elle monte et descend en tournoyant.
                    p.vel.y = (p.age * 7.0 + p.phase).sin() * 45.0 + 12.0;
                }
            }
            p.pos += p.vel * dt;
        }
        self.particles.retain(|p| p.age < p.life);
    }

    fn update_shadow(&mut self, world: &World) {
        // Sur le rebord d'une fenêtre, l'ombre s'arrête à ses bouts. Le dos
        // d'un autre animal est plus étroit que son corps : l'ombre y tombe
        // en entier.
        let on = |s: &Surface, distance: f32| {
            if crate::physics::surface_pet(s.id).is_some() {
                Shadow::on(s.y, distance)
            } else {
                Shadow {
                    y: s.y,
                    distance,
                    x0: s.x0,
                    x1: s.x1,
                }
            }
        };
        self.shadow = match self.support {
            Support::Ground { surface } => Some(match world.surface(surface) {
                Some(s) => Shadow {
                    y: self.pos.y,
                    ..on(s, 0.0)
                },
                None => Shadow::on(self.pos.y, 0.0),
            }),
            _ => world
                .surface_below(self.pos, self.def.physics.width * 0.2)
                .map(|s| on(s, s.y - self.pos.y)),
        };
    }

    /// Image dans la variante de couleur de l'animal : les indices du fichier
    /// désignent la première variante (desktopPet).
    /// Image affichée au pas `step` : celle de la séquence, ou, vers la fin
    /// du pas, une image intermédiaire vers celle du pas suivant. Le passage
    /// dure au plus [`INBETWEEN_TIME`] par image intermédiaire : une pose
    /// longue reste tenue, puis l'animal change de pose en douceur.
    fn sprite_frame(&self, clip: &SpriteClip, step: usize, looping: bool) -> u32 {
        let frame = clip.frame_at(step);
        let Appearance::Sprites(sheet) = &self.def.appearance else {
            return frame;
        };
        // Pas de pas suivant : la séquence s'arrête (ou garde sa dernière image).
        let total = self.clip_total.max(1);
        let last = |s: usize| !looping && s + 1 >= total;
        if step != self.clip_step || last(step) {
            return frame;
        }
        // Une image tenue plusieurs pas (desktopPet double souvent ses images) :
        // le passage vers la suivante se répartit sur toute la tenue, pas
        // seulement sur son dernier pas.
        let mut end = step;
        while end - step < MAX_HELD_STEPS && !last(end) && clip.frame_at(end + 1) == frame {
            end += 1;
        }
        if last(end) {
            return frame;
        }
        let next = clip.frame_at(end + 1);
        let Some((first, count)) = (next != frame)
            .then(|| sheet.inbetween(frame, next))
            .flatten()
        else {
            return frame;
        };
        let mut start = step;
        while step - start < MAX_HELD_STEPS && start > 0 && clip.frame_at(start - 1) == frame {
            start -= 1;
        }
        let interval = self.clip_interval();
        let hold = interval * (end - start + 1) as f32;
        let n = count as f32;
        let span = (hold * n / (n + 1.0)).min(n * INBETWEEN_TIME);
        let into = (step - start) as f32 * interval + self.clip_timer - (hold - span);
        if into < 0.0 || span <= 0.0 {
            return frame;
        }
        first + ((into / span * n) as u32).min(count - 1)
    }

    fn skinned(&self, frame: u32) -> u32 {
        match &self.def.appearance {
            Appearance::Sprites(sheet) if sheet.skins > 1 => {
                let range = sheet.tile_count() / sheet.skins;
                if frame < range {
                    frame + self.skin * range
                } else {
                    frame
                }
            }
            _ => frame,
        }
    }

    /// Parti en l'air pendant le pas alors qu'il reposait penché sur un
    /// coin (voir `tilt_lift`) : son corps repart d'où on le voyait, pas
    /// d'un coin enfoncé dans la surface. Son dessin ne tombe pas d'un coup
    /// de la hauteur du coin, et il ne passe pas à travers un rebord.
    fn keep_lift_on_take_off(&mut self) {
        if self.ground_lift != Vec2::ZERO && self.support == Support::Air {
            self.pos -= self.ground_lift;
        }
        self.ground_lift = Vec2::ZERO;
    }

    /// Posé au sol encore penché (il se redresse après une culbute, en
    /// physique réaliste) : il repose sur le coin le plus bas de son corps,
    /// pas sur ses pattes. Son dessin monte d'autant, et redescend à mesure
    /// qu'il se redresse ; sans cela, il s'enfonçait dans le sol, et sautait
    /// d'un coup vers le bas en touchant terre.
    fn tilt_lift(&self) -> Vec2 {
        if !self.realistic || self.angle == 0.0 || !matches!(self.support, Support::Ground { .. }) {
            return Vec2::ZERO;
        }
        let body = self.body();
        Vec2::new(0.0, (body.extent_y() - body.half.y).max(0.0))
    }

    /// L'image de sa planche qu'il montre maintenant, et son décalage
    /// vertical (points, vers le bas), s'il est fait d'images.
    fn current_sprite(&self) -> Option<(u32, f32)> {
        let behavior = &self.def.behaviors[self.behavior];
        let clip = behavior.clip.as_ref()?;
        let total = self.clip_total.max(1);
        let looping = self.support == Support::Held
            || behavior.motion == Motion::Fall
            || behavior.duration != Duration::Clip;
        let step = if looping {
            self.clip_step
        } else {
            self.clip_step.min(total - 1)
        };
        let t = (self.clip_step as f32 / total as f32).min(1.0);
        let offset = if self.support == Support::Held {
            0.0
        } else {
            lerp(clip.offset_y, clip.offset_y_end, t)
        };
        Some((self.skinned(self.sprite_frame(clip, step, looping)), offset))
    }

    /// Vue interpolée entre le pas précédent et le pas courant (`alpha` entre 0 et 1).
    #[must_use]
    pub fn view(&self, alpha: f32) -> PetView<'_> {
        let a = &self.anim;
        let behavior = &self.def.behaviors[self.behavior];
        // Tenu, lancé ou d'une durée propre, l'animation boucle ; sinon le
        // dernier pas garde la dernière image (voir `current_sprite`).
        let (sprite_frame, sprite_offset_y) = self
            .current_sprite()
            .map_or((None, 0.0), |(frame, offset)| (Some(frame), offset));
        let opacity = behavior.clip.as_ref().map_or(1.0, |clip| {
            let t = (self.clip_step as f32 / self.clip_total.max(1) as f32).min(1.0);
            lerp(clip.opacity_start, clip.opacity_end, t)
        });
        let fade = self
            .leaving
            .and_then(|l| l.fade)
            .map_or(1.0, |f| f.clamp(0.0, 1.0));
        let opacity = opacity * fade;
        let tail_base = a.tail.value + a.tail_phase.sin() * (0.03 + a.walk * 0.07);
        PetView {
            id: self.id,
            // Tenu, il est dessiné exactement sous le curseur, sans attendre
            // le prochain pas de la simulation.
            ground: match &self.hold {
                Some(hold) if self.realistic => self.held_ground(hold.target, hold.offset),
                Some(hold) => hold.target - hold.offset,
                // Penché au sol, il repose sur un coin : son dessin monte
                // d'autant, d'un pas à l'autre comme sa position.
                None => (self.prev_pos - self.prev_lift + self.prev_smooth).lerp(
                    self.pos - self.tilt_lift() + self.smooth,
                    alpha.clamp(0.0, 1.0),
                ),
            },
            facing: a.flip,
            pose: self.family_pose(PoseSignals {
                gait_phase: a.gait_phase,
                walk: a.walk,
                run: a.run,
                sit: a.sit,
                sleep: a.sleep,
                dangle: a.dangle,
                air: a.air,
                squash: a.squash.value,
                lean: a.lean.value,
                ear: a.ear.value,
                tail: tail_base,
                tail_mid: a.tail_mid.value - tail_base,
                tail_tip: a.tail_tip.value - a.tail_mid.value,
                blink: a.blink,
                breath: a.breath_phase.sin(),
                look: a.look,
                swing: a.swing.value,
                roll: a.roll + self.angle + self.wind_lean(),
                graze: a.graze,
                nibble: (a.nibble_phase).sin(),
                rise: a.rise,
                leap: a.leap,
                leap_flight: a.leap_flight,
                clock: a.clock,
                trot: a.trot,
                stride_scale: a.stride_scale,
                legs: a.legs,
                jaw: a.jaw,
                gesture: a.gesture,
                gesture_amount: a.gesture_amount,
                gesture_time: a.gesture_time,
                nursing_target: None,
                ears_back: a.ears_back,
                head_low: a.head_low,
                stiff: a.stiff,
                ears_droop: a.ears_droop,
                ear_split: a.ear_split,
                twist: a.twist,
                sleep_low: a.sleep_low,
                twitch: a.twitch,
                fleece: a.fleece.value,
                fleece_tip: a.fleece_tip.value - a.fleece.value,
            }),
            shadow: self.shadow,
            sprite_frame,
            sprite_offset_y,
            opacity,
            particles: &self.particles,
            support: self.support,
            behavior: &behavior.name,
            grip: self.hold.map(|hold| hold.target),
        }
    }
}

/// Un agneau en invite un autre à jouer : lequel, et dans combien de temps
/// l'autre répond (secondes).
#[derive(Clone, Copy, Debug, PartialEq)]
struct Invite {
    partner: u32,
    wait: f32,
}

/// Le troupeau : tous les animaux, avancés ensemble à pas fixe.
#[derive(Clone, Debug, Default)]
pub struct Flock {
    families: family::Families,
    pets: Vec<Pet>,
    accumulator: f32,
    next_id: u32,
    /// Mode collisions : deux animaux qui se rentrent dedans partent à la renverse.
    collisions: bool,
    /// Le tempérament des animaux (voir [`crate::temper`]).
    mood: Mood,
    /// Change quand un animal arrive, s'en va ou part (voir [`Flock::generation`]).
    generation: u64,
    /// L'animal qui attend, immobile (son menu est ouvert) : voir
    /// [`Flock::hold_still`].
    still: Option<u32>,
    /// Physique réaliste (voir [`crate::physics`]), et ce qu'il lui faut :
    /// le suivi des fenêtres et, à chaque pas, les dos des animaux.
    realistic: bool,
    tracker: Tracker,
    backs: Vec<Back>,
    /// L'utilisateur est parti (voir [`Input::away`]).
    away: bool,
    /// Le coup de vent du moment.
    gust: Option<crate::surprise::Gust>,
    /// Soucoupes en visite.
    saucers: Vec<crate::surprise::Saucer>,
    /// Le pointeur au dernier [`Flock::advance`].
    last_cursor: Option<Vec2>,
    /// Les paires d'animaux assez proches pour se toucher (voir [`near`]).
    near: near::Near,
    /// Ceux qui viennent de commencer un comportement contagieux, le temps
    /// d'un pas (voir [`Flock::echo`]).
    echo_sources: Vec<(usize, usize)>,
    /// Le prochain regroupement du troupeau (secondes ; 0 : pas encore
    /// tiré), et de quoi tirer le suivant.
    regroup_in: f32,
    regroup_seed: u64,
    /// Les voyages d'un écran à l'autre (voir `pet/voyage.rs`) : de quoi
    /// tirer les envies, le rythme d'un essai, et s'il y en a en cours.
    voyage_seed: u64,
    voyage_every: Option<(f32, f32)>,
    voyages_live: bool,
    /// Combien sont partis, combien sont arrivés (voir [`Flock::voyages`]).
    voyages: (u32, u32),
    /// Les axes du corps de chaque animal et les demi-côtés du rectangle
    /// droit qui le contient, le temps des chocs d'un pas (voir
    /// [`Flock::collide_bodies`]).
    axes: Vec<([Vec2; 2], Vec2)>,
}

/// Distance d'un point au segment `[a, b]`.
fn distance_to_segment(p: Vec2, a: Vec2, b: Vec2) -> f32 {
    let ab = b - a;
    let len2 = ab.dot(ab);
    let t = if len2 > 0.0 {
        ((p - a).dot(ab) / len2).clamp(0.0, 1.0)
    } else {
        0.0
    };
    (p - (a + ab * t)).length()
}

/// Élans donnés à deux animaux qui se rentrent dedans, ou rien.
fn bump(a: &Pet, b: &Pet) -> Option<(Vec2, Vec2)> {
    if !a.can_bump() || !b.can_bump() {
        return None;
    }
    let (pa, pb) = (a.def.physics, b.def.physics);
    let dx = b.pos.x - a.pos.x;
    // Ils se rapprochent : l'un au moins va vers l'autre (`side` : côté de b).
    let side = if dx == 0.0 { -a.facing } else { dx.signum() };
    // Les dessins se touchent, à la même hauteur.
    if dx.abs() >= a.reach_toward(side) + b.reach_toward(-side) {
        return None;
    }
    let (a_top, b_top) = (a.pos.y - pa.height * 0.8, b.pos.y - pb.height * 0.8);
    if a.pos.y <= b_top || b.pos.y <= a_top {
        return None;
    }
    let closing = (a.vel.x - b.vel.x) * side;
    let airborne = a.support == Support::Air || b.support == Support::Air;
    if closing < if airborne { 40.0 } else { 6.0 } {
        return None;
    }
    // Une mère et son agneau ne se renversent pas (vérifié en dernier : le
    // cas est rare, les tests d'avant sont moins chers).
    if family::kin(a, b) {
        return None;
    }
    // Chacun recule d'un peu plus que sa largeur, avec un bond de la moitié de sa hauteur.
    let push = |p: &crate::def::Physics, away: f32| {
        let up = (2.0 * p.gravity * p.height * 0.55).sqrt();
        let flight = 2.0 * up / p.gravity.max(1.0);
        let back = p.width * 1.3 / flight.max(0.1) + closing.min(600.0) * 0.4;
        Vec2::new(away * back, -up)
    };
    Some((push(&pa, -side), push(&pb, side)))
}

/// Ces deux animaux peuvent-ils se heurter comme des corps (physique
/// réaliste) ? Il en faut un en l'air ; ni l'un ni l'autre ne doit être un
/// personnage secondaire, en partance ou tout juste arrivé, ni porté par
/// l'autre.
fn physical_pair(a: &Pet, b: &Pet, axes_a: [Vec2; 2], axes_b: [Vec2; 2]) -> bool {
    let flying = |p: &Pet| p.support == Support::Air;
    let free = |p: &Pet| !p.is_child() && p.leaving.is_none() && !p.finished && !p.ghost;
    let carried = |p: &Pet, by: &Pet| {
        p.support
            == Support::Ground {
                surface: crate::physics::pet_surface(by.id),
            }
    };
    // Arrivant d'en haut au-dessus du dos d'un animal au sol, il s'y pose
    // (une surface), au lieu de rebondir sur son corps. À côté du dos, il
    // heurte son corps dès qu'il le touche : il ne s'y enfonce pas jusqu'à
    // la taille pour en être chassé d'un coup.
    let landing_on = |p: &Pet, axes: [Vec2; 2], by: &Pet| {
        if !flying(p) {
            return false;
        }
        let body = p.body();
        body.center.y < by.pos.y - by.def.physics.height
            && by.back().is_some_and(|back| {
                back.surface.contains_x(
                    body.lowest_point_with(axes).x,
                    p.def.physics.width * LANDING_REACH,
                )
            })
    };
    (flying(a) || flying(b))
        && free(a)
        && free(b)
        && !carried(a, b)
        && !carried(b, a)
        && !landing_on(a, axes_a, b)
        && !landing_on(b, axes_b, a)
        // Une mère et son agneau ne se heurtent pas (le cas rare, en dernier).
        && !family::kin(a, b)
}

/// Leurs corps se chevauchaient déjà franchement au début du pas : ils ne
/// sont pas l'un contre l'autre, mais l'un devant l'autre. Au sol, deux
/// animaux se croisent ainsi ; l'un part en l'air (heurté, une culbute) du
/// milieu d'un groupe, ou retombe d'un tas où il était coincé. Les séparer
/// d'un coup le jetterait de côté, d'un corps dans le suivant : il ne les
/// heurte qu'une fois sorti d'eux.
/// `reach` : la somme des demi-côtés de leurs rectangles droits.
fn overlapped_before(
    a: &Pet,
    b: &Pet,
    (axes_a, axes_b): ([Vec2; 2], [Vec2; 2]),
    reach: Vec2,
) -> bool {
    let before = |p: &Pet| crate::physics::Obb {
        center: p.prev_pos - Vec2::new(0.0, p.def.physics.height * 0.5),
        ..p.body()
    };
    let (was_a, was_b) = (before(a), before(b));
    let depth = OVERLAP_BEFORE * a.def.physics.height.min(b.def.physics.height);
    // Ils ne s'enfoncent pas l'un dans l'autre de plus que leurs rectangles
    // droits ne se chevauchent, ni de moins que les cercles qu'ils
    // contiennent : le plus souvent, pas besoin d'aller plus loin.
    let apart = was_b.center - was_a.center;
    let overlap = reach - Vec2::new(apart.x.abs(), apart.y.abs());
    if overlap.x.min(overlap.y) <= depth {
        return false;
    }
    let inner = was_a.half.x.min(was_a.half.y) + was_b.half.x.min(was_b.half.y) - depth;
    if inner > 0.0 && apart.dot(apart) < inner * inner {
        return true;
    }
    crate::physics::obb_depth_with(&was_a, axes_a, &was_b, axes_b).is_some_and(|d| d > depth)
}

/// Deux corps se heurtent : échange d'élan au point de contact (avec
/// rotation), puis on les sépare. Un animal tenu ne bouge pas (la main le
/// tient) ; un animal au sol encaisse le choc, et part à la renverse si le
/// choc est rude.
fn bodies_hit(
    a: &mut Pet,
    b: &mut Pet,
    contact: crate::physics::Contact,
    world: &World,
    input: &Input,
) {
    use crate::physics::{PET_FRICTION, PET_RESTITUTION, Rigid, collide_pair};
    let rigid = |p: &Pet| match p.support {
        Support::Held => Rigid::fixed(p.vel),
        _ => p.rigid(),
    };
    let (mut ra, mut rb) = (rigid(a), rigid(b));
    let (ca, cb) = (a.body().center, b.body().center);
    let closing = collide_pair(
        &mut ra,
        ca,
        &mut rb,
        cb,
        contact.point,
        contact.normal,
        PET_RESTITUTION,
        PET_FRICTION,
    );
    // Séparés le long de la normale, selon qui peut bouger. Un corps entré
    // loin dans un autre (lâché dessus, une foule serrée) en ressort d'un
    // coup, mais son dessin y glisse (voir `Pet::smooth`) : rien ne saute à
    // l'écran.
    let movable = |p: &Pet| if p.support == Support::Air { 1.0 } else { 0.0 };
    let (ma, mb) = (movable(a), movable(b));
    if ma + mb > 0.0 {
        let (from_a, from_b) = (a.pos, b.pos);
        let push = contact.normal * (contact.depth / (ma + mb));
        a.pos -= push * ma;
        b.pos += push * mb;
        // Poussé vers le bas, il ne passe pas sous le sol.
        a.keep_above_floor(world);
        b.keep_above_floor(world);
        a.smooth += from_a - a.pos;
        b.smooth += from_b - b.pos;
    }
    for (pet, after) in [(a, ra), (b, rb)] {
        match pet.support {
            Support::Air => {
                pet.vel = after.vel;
                pet.spin = after
                    .spin
                    .clamp(-crate::physics::MAX_SPIN, crate::physics::MAX_SPIN);
                if closing > 150.0 {
                    pet.events.push(PetEvent::Bumped { speed: closing });
                    pet.squash_kick(-0.6);
                }
            }
            Support::Ground { .. } if closing > 320.0 && pet.can_bump() => {
                // Bousculé : il part avec l'élan reçu, un peu vers le haut.
                let world_vel = pet.vel + pet.carry;
                let push = after.vel - world_vel + Vec2::new(0.0, -closing * 0.25);
                let knocked = Vec2::new(push.x, push.y.min(-120.0));
                pet.knock(knocked, world, input);
            }
            _ => {}
        }
    }
}

/// `a` vient-il, en marchant, de toucher `b` posé au même niveau ?
fn meeting(a: &Pet, b: &Pet) -> bool {
    if family::family_pair(a, b) {
        return false;
    }
    if !a.can_meet() || !b.can_meet() || (a.pos.y - b.pos.y).abs() > 2.0 {
        return false;
    }
    let dx = b.pos.x - a.pos.x;
    if dx == 0.0 || a.vel.x * dx.signum() <= 1.0 {
        return false;
    }
    // Leurs dessins se touchent à ce pas-ci.
    let side = dx.signum();
    let reach = a.reach_toward(side) + b.reach_toward(-side);
    let before = (b.prev_pos.x - a.prev_pos.x).abs();
    dx.abs() <= reach && before > reach
}

/// `a` a touché `b` : ce que chacun fait, comme dans desktopPet.
///
/// 1. S'ils ont des réactions en commun (même nom, poids non nul), les
///    deux font la même chose.
/// 2. Sinon, si `b` joue une réaction que `a` connaît (même de poids nul),
///    `a` l'imite une fois sur deux.
/// 3. Sinon, chacun réagit de son côté ; `b` seulement s'il fait face à `a`.
///
/// Sans réaction, `a` passe son chemin (il croise l'autre).
fn encounter(a: &mut Pet, b: &mut Pet, world: &World, input: &Input) {
    let (def_a, def_b) = (Arc::clone(&a.def), Arc::clone(&b.def));
    // Où leurs dessins se touchent (ce que `meeting` vient de voir), avant
    // que leurs réactions ne les retournent.
    let side = (b.pos.x - a.pos.x).signum();
    let touching = a.reach_toward(side) + b.reach_toward(-side);
    // Lancé sur l'autre (un bélier qui charge, deux agneaux qui jouent à se
    // cogner le front) : s'il réagit, c'est un choc.
    let clash = matches!(def_a.behaviors[a.behavior].motion, Motion::Charge { .. });
    let list_a = a.meet_reactions(&def_a);
    let list_b = b.meet_reactions(&def_b);
    let name_a = |t: &Transition| def_a.behaviors[t.to].name.as_str();
    let current_b = def_b.behaviors[b.behavior].name.as_str();
    // Même sens (ou `b` dans son dos) : `a` ne peut qu'imiter.
    let same_direction = a.facing == b.facing
        || (a.facing < 0.0 && b.pos.x > a.pos.x)
        || (a.facing > 0.0 && b.pos.x < a.pos.x);
    let mut common = Vec::new();
    let mut mimic = None;
    for t in list_a {
        if !a.transition_allowed(t, world, input) {
            continue;
        }
        if name_a(t) == current_b {
            mimic = Some(t.to);
        }
        if same_direction {
            if mimic.is_some() {
                break;
            }
            continue;
        }
        if t.weight > 0.0
            && list_b.iter().any(|u| {
                u.weight > 0.0
                    && def_b.behaviors[u.to].name == name_a(t)
                    && b.transition_allowed(u, world, input)
            })
        {
            common.push(t.clone());
        }
    }

    let mut a_reacts = false;
    let mut b_reacts = false;
    if !common.is_empty() {
        if let Some(next) = a.choose_with_partner(&common, world, input, Some(b)) {
            a.enter(next);
            a_reacts = true;
            if let Some(next_b) = def_b.behavior_index(&def_a.behaviors[next].name) {
                b.enter(next_b);
                b_reacts = true;
            }
        }
    } else if let Some(next) = mimic.filter(|_| a.rng.chance(0.5)) {
        a.enter(next);
        a_reacts = true;
    } else {
        if let Some(next) = a.choose_with_partner(list_a, world, input, Some(b)) {
            a.enter(next);
            a_reacts = true;
        }
        if !same_direction && let Some(next) = b.choose_with_partner(list_b, world, input, Some(a))
        {
            b.enter(next);
            b_reacts = true;
        }
    }
    if a_reacts {
        // Il s'arrête contre l'autre, sans le chevaucher : là où leurs
        // dessins se touchent, à un pas près (jamais d'un bond).
        if (b.pos.x - a.pos.x).abs() < touching {
            a.pos.x = b.pos.x - side * touching;
        }
        a.vel.x = 0.0;
        if clash {
            // Le choc les tasse tous les deux, oreilles rejetées.
            a.squash_kick(1.6);
            b.squash_kick(1.2);
            a.anim.ear.kick(-3.0);
            b.anim.ear.kick(-3.0);
        }
    }
    if a_reacts || b_reacts {
        if a_reacts {
            a.remember_sniff(b);
        }
        if b_reacts {
            b.remember_sniff(a);
        }
        a.meet_cooldown = MEET_COOLDOWN;
        b.meet_cooldown = MEET_COOLDOWN;
        a.events.push(PetEvent::Met { other: b.id });
        b.events.push(PetEvent::Met { other: a.id });
    }
}

impl Flock {
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// Ajoute un animal tombé du haut de l'écran ; renvoie son identifiant.
    pub fn spawn(&mut self, def: Arc<PetDef>, seed: u64, world: &World, x: Option<f32>) -> u32 {
        self.spawn_as(def, seed, world, x, None)
    }

    /// Comme [`Flock::spawn`], du sexe voulu s'il est donné (voir
    /// [`Pet::spawn_as`]) : un bélier ou une brebis, au choix.
    pub fn spawn_as(
        &mut self,
        def: Arc<PetDef>,
        seed: u64,
        world: &World,
        x: Option<f32>,
        sex: Option<Sex>,
    ) -> u32 {
        let id = self.next_id;
        self.next_id += 1;
        let mut pet = Pet::spawn_as(id, def, seed, world, x, sex);
        pet.take_temper(self.mood);
        self.pets.push(pet);
        self.generation += 1;
        id
    }

    /// Comme [`Flock::spawn_as`], sur l'écran où se trouve `near` (celui que
    /// l'utilisateur regarde : voir [`Pet::spawn_near`]).
    pub fn spawn_near(
        &mut self,
        def: Arc<PetDef>,
        seed: u64,
        world: &World,
        near: Vec2,
        sex: Option<Sex>,
    ) -> u32 {
        let id = self.next_id;
        self.next_id += 1;
        let mut pet = Pet::spawn_near(id, def, seed, world, near, None, sex);
        pet.take_temper(self.mood);
        self.pets.push(pet);
        self.generation += 1;
        id
    }

    /// Le tempérament des animaux, pour tous (ceux qui sont là et ceux qui
    /// arriveront) ; les personnages secondaires suivent leur rôle.
    pub fn set_mood(&mut self, mood: Mood) {
        self.mood = mood;
        for pet in &mut self.pets {
            if !pet.is_child() {
                pet.take_temper(mood);
            }
        }
    }

    #[must_use]
    pub fn mood(&self) -> Mood {
        self.mood
    }

    pub fn remove(&mut self, id: u32) {
        self.forget_family(id);
        let before = self.pets.len();
        self.pets.retain(|p| p.id() != id);
        if self.pets.len() != before {
            self.generation += 1;
        }
    }

    pub fn clear(&mut self) {
        self.families = family::Families::default();
        self.pets.clear();
        self.generation += 1;
    }

    /// Change à chaque animal qui arrive, qui commence à dire au revoir ou
    /// qui quitte le troupeau : tant qu'elle ne change pas, la liste des
    /// animaux (et de ceux qui partent) est la même. De quoi ne refaire les
    /// fenêtres et le menu que quand il le faut.
    #[must_use]
    pub fn generation(&self) -> u64 {
        self.generation
    }

    #[must_use]
    pub fn pets(&self) -> &[Pet] {
        &self.pets
    }

    pub fn pets_mut(&mut self) -> &mut [Pet] {
        &mut self.pets
    }

    #[must_use]
    pub fn get_mut(&mut self, id: u32) -> Option<&mut Pet> {
        self.pets.iter_mut().find(|p| p.id() == id)
    }

    /// Active ou coupe le mode collisions.
    pub fn set_collisions(&mut self, on: bool) {
        self.collisions = on;
    }

    /// Cet animal attend, immobile, sans que le temps passe pour lui (on a
    /// ouvert son menu : il reste sous le curseur) ; les autres continuent.
    /// `None` : il reprend où il en était.
    pub fn hold_still(&mut self, id: Option<u32>) {
        self.still = id;
        if let Some(pet) = id.and_then(|id| self.pets.iter_mut().find(|p| p.id == id)) {
            // Plus rien à interpoler : il est dessiné là où il est.
            pet.prev_pos = pet.pos;
            pet.prev_smooth = pet.smooth;
        }
    }

    /// L'animal qui attend, s'il y en a un.
    #[must_use]
    pub fn still(&self) -> Option<u32> {
        self.still
    }

    /// Physique réaliste (voir [`crate::physics`]) ou classique, celle
    /// d'eSheep et de desktopPet.
    pub fn set_realistic(&mut self, on: bool) {
        if on == self.realistic {
            return;
        }
        self.realistic = on;
        self.tracker = Tracker::default();
    }

    #[must_use]
    pub fn realistic(&self) -> bool {
        self.realistic
    }

    #[must_use]
    pub fn collisions(&self) -> bool {
        self.collisions
    }

    /// Avance la simulation du temps écoulé et renvoie le facteur
    /// d'interpolation à passer à [`Pet::view`]. Un temps négatif ou non
    /// fini n'avance pas l'horloge.
    pub fn advance(&mut self, elapsed: f32, world: &World, input: &Input) -> f32 {
        let elapsed = if elapsed.is_finite() {
            elapsed.max(0.0)
        } else {
            0.0
        };
        self.react(elapsed, world, input);
        // Au-delà d'un dixième de seconde (Mac réveillé, débogueur…), on ne
        // rattrape pas : l'animal reprend où il était au lieu de bondir.
        self.accumulator += elapsed.clamp(0.0, 0.1);
        while self.accumulator >= TICK {
            self.step_families(TICK, world);
            self.step_surprises(world, input);
            if self.realistic {
                self.step_realistic(world, input);
            } else {
                for pet in &mut self.pets {
                    if self.still != Some(pet.id) {
                        pet.step(TICK, world, input);
                    }
                }
                if self.collisions {
                    let mut near = std::mem::take(&mut self.near);
                    self.collide(near.update(&self.pets, false).reach, world, input);
                    self.near = near;
                } else {
                    self.meet(world, input);
                }
            }
            self.echo();
            self.step_regroup(TICK);
            self.step_invites(TICK);
            self.step_voyages(TICK, world);
            self.adopt_children(world);
            self.accumulator -= TICK;
        }
        self.accumulator / TICK
    }

    /// Un coup de vent se lève, vers la droite (`dir` positif) ou la gauche,
    /// pour à peu près `seconds` secondes.
    pub fn blow(&mut self, dir: f32, seconds: f32) {
        self.gust = Some(crate::surprise::Gust::new(dir, seconds));
    }

    /// Le coup de vent du moment.
    #[must_use]
    pub fn gust(&self) -> Option<&crate::surprise::Gust> {
        self.gust.as_ref()
    }

    /// Une soucoupe volante vient emprunter un animal posé (pas un
    /// personnage secondaire, ni un animal qui part ou qu'on tient) ; elle a
    /// son numéro, comme un animal. `None` s'il n'y en a aucun.
    pub fn invite_saucer(&mut self, world: &World, seed: u64) -> Option<u32> {
        let mut rng = crate::math::Rng::new(seed);
        let busy: Vec<u32> = self
            .saucers
            .iter()
            .map(crate::surprise::Saucer::target)
            .collect();
        let candidates: Vec<&Pet> = self
            .pets
            .iter()
            .filter(|p| p.can_react() && !busy.contains(&p.id) && self.still != Some(p.id))
            .collect();
        let pick = (rng.next_u32() as usize) % candidates.len().max(1);
        let pet = candidates.get(pick)?;
        let id = self.next_id;
        self.next_id += 1;
        self.saucers.push(crate::surprise::Saucer::new(
            id,
            pet.id,
            pet.position(),
            world.arena_at(pet.position()),
            rng.next_u32().into(),
        ));
        self.generation += 1;
        Some(id)
    }

    /// Les soucoupes en visite.
    #[must_use]
    pub fn saucers(&self) -> &[crate::surprise::Saucer] {
        &self.saucers
    }

    /// Un pas des surprises : le vent souffle sur chacun (et ils le
    /// remarquent quand il se lève), les soucoupes font leur visite.
    fn step_surprises(&mut self, world: &World, input: &Input) {
        let blowing = self.gust.as_mut().is_some_and(|gust| gust.step(TICK));
        let accel = match &mut self.gust {
            Some(gust) if blowing => {
                if !gust.noticed && gust.strength() > 0.45 {
                    gust.noticed = true;
                    for pet in &mut self.pets {
                        if pet.can_react() && !pet.is_asleep() && pet.rng.chance(0.6) {
                            let def = Arc::clone(&pet.def);
                            if let Some(next) = pet.choose(&def.reflexes.wind, world, input) {
                                pet.enter(next);
                            }
                        }
                    }
                }
                gust.accel()
            }
            Some(_) => {
                self.gust = None;
                0.0
            }
            None => 0.0,
        };
        for pet in &mut self.pets {
            pet.wind = accel;
        }
        if self.saucers.is_empty() {
            return;
        }
        for saucer in &mut self.saucers {
            let pet = self.pets.iter_mut().find(|p| p.id == saucer.target());
            saucer.step(TICK, pet, world);
        }
        self.hit_saucers(world, input);
        let before = self.saucers.len();
        self.saucers.retain(|s| !s.is_gone());
        if self.saucers.len() != before {
            self.generation += 1;
        }
    }

    /// Un animal en l'air qui percute une soucoupe (son corps, pas son
    /// faisceau) assez vite : il rebondit dessus en culbutant, elle lâche
    /// ce qu'elle tenait et fume.
    fn hit_saucers(&mut self, world: &World, input: &Input) {
        for s in 0..self.saucers.len() {
            if !self.saucers[s].can_be_hit() {
                continue;
            }
            let saucer = &self.saucers[s];
            let (center, speed) = (saucer.view().pos, saucer.velocity());
            let carried = saucer.holds().then(|| saucer.target());
            let hitter = self.pets.iter().position(|pet| {
                let body = pet.body();
                pet.support == Support::Air
                    && !pet.is_held()
                    && Some(pet.id) != carried
                    && (pet.vel - speed).length() >= crate::surprise::HIT_SPEED
                    && saucer.touches(body.center, body.half.x.min(body.half.y) * 0.8)
            });
            let Some(i) = hitter else {
                continue;
            };
            if self.saucers[s].hit() {
                let target = self.saucers[s].target();
                if let Some(pet) = self.pets.iter_mut().find(|p| p.id == target) {
                    pet.release(world);
                }
            }
            // Il rebondit sur elle, en culbutant.
            let pet = &mut self.pets[i];
            let away = pet.body().center - center;
            let dir = if away.length() > 1e-3 {
                away * (1.0 / away.length())
            } else {
                Vec2::new(0.0, 1.0)
            };
            let rel = (pet.vel - speed).length();
            let push = dir * (0.45 * rel + 120.0) + speed * 0.3;
            if !pet.knock(push, world, input) {
                pet.vel = push;
            }
            pet.events.push(PetEvent::HitSaucer);
            self.generation += 1;
        }
    }

    /// Ce que fait l'utilisateur, vu par tout le troupeau : un pointeur qui
    /// file près d'un animal le fait sursauter ; parti, il les laisse
    /// s'endormir ; de retour, il les réveille.
    fn react(&mut self, elapsed: f32, world: &World, input: &Input) {
        let away = input.away();
        if away != self.away {
            self.away = away;
            for pet in &mut self.pets {
                pet.user_away(away);
            }
        }
        let from = self.last_cursor;
        self.last_cursor = input.cursor;
        let (Some(from), Some(to)) = (from, input.cursor) else {
            return;
        };
        let speed = (to - from).length() / elapsed.max(1.0 / 240.0);
        if elapsed <= 0.0 || speed < STARTLE_SPEED {
            return;
        }
        for pet in &mut self.pets {
            if self.still != Some(pet.id) {
                pet.startle(from, to, world, input);
            }
        }
    }

    /// Un pas en physique réaliste : le suivi des fenêtres (et les dos des
    /// animaux, qui sont solides en mode collisions), chaque animal, puis
    /// les chocs entre eux.
    fn step_realistic(&mut self, world: &World, input: &Input) {
        self.backs.clear();
        if self.collisions {
            self.backs.extend(self.pets.iter().filter_map(Pet::back));
        }
        // Le suivi est mis de côté le temps du pas : les animaux le lisent
        // pendant que le troupeau les change.
        let mut tracker = std::mem::take(&mut self.tracker);
        tracker.update(world, TICK, &self.backs);
        for pet in &mut self.pets {
            if self.still != Some(pet.id) {
                pet.step_real(TICK, &tracker, input);
            }
        }
        if self.collisions {
            // Les paires proches, faites une fois pour les chocs entre corps
            // et pour les heurts : les corps séparés n'ont bougé que de la
            // marge.
            let mut near = std::mem::take(&mut self.near);
            let pairs = near.update(&self.pets, true);
            self.collide_bodies(pairs.bodies, tracker.world(), input);
            self.collide(pairs.reach, tracker.world(), input);
            self.near = near;
        } else {
            self.meet(tracker.world(), input);
        }
        for pet in &mut self.pets {
            if self.still != Some(pet.id) {
                pet.keep_lift_on_take_off();
            }
        }
        self.tracker = tracker;
    }

    /// Chocs entre corps (physique réaliste, mode collisions) : un animal en
    /// l'air heurte les autres, en l'air ou au sol ; tenu, il est comme la
    /// main qui le tient. Deux animaux au sol se heurtent comme en physique
    /// classique (`collide`), et l'un sur le dos de l'autre y est posé.
    fn collide_bodies(&mut self, pairs: &[(u32, u32)], world: &World, input: &Input) {
        if pairs.is_empty() {
            return;
        }
        let still = self.still;
        // Les axes de chaque corps et les demi-côtés du rectangle droit qui
        // le contient, une fois pour tout le pas : un choc déplace les corps
        // et change leur rotation, jamais leur angle. Dans une foule, un
        // corps en l'air en frôle des dizaines d'autres ; deux rectangles
        // droits qui ne se touchent pas évitent la séparation des axes.
        let mut axes = std::mem::take(&mut self.axes);
        axes.clear();
        axes.extend(self.pets.iter().map(|pet| {
            let body = pet.body();
            let [ax, ay] = body.axes();
            let extent = Vec2::new(
                body.half.x * ax.x.abs() + body.half.y * ay.x.abs(),
                body.half.x * ax.y.abs() + body.half.y * ay.y.abs(),
            );
            ([ax, ay], extent)
        }));
        for &(i, j) in pairs {
            let (a, b) = self.pair_mut(i, j);
            let ((axes_a, extent_a), (axes_b, extent_b)) = (axes[i as usize], axes[j as usize]);
            let (body_a, body_b) = (a.body(), b.body());
            let apart = body_b.center - body_a.center;
            let reach = extent_a + extent_b;
            if apart.x.abs() > reach.x + 1e-3
                || apart.y.abs() > reach.y + 1e-3
                || still == Some(a.id)
                || still == Some(b.id)
                || !physical_pair(a, b, axes_a, axes_b)
                || overlapped_before(a, b, (axes_a, axes_b), reach)
            {
                continue;
            }
            let Some(contact) = crate::physics::obb_contact_with(&body_a, axes_a, &body_b, axes_b)
            else {
                continue;
            };
            bodies_hit(a, b, contact, world, input);
        }
        self.axes = axes;
    }

    /// Deux animaux qui se rentrent dedans partent chacun à la renverse.
    fn collide(&mut self, pairs: &[(u32, u32)], world: &World, input: &Input) {
        for &(i, j) in pairs {
            let (i, j) = (i as usize, j as usize);
            if self.is_still(i) || self.is_still(j) {
                continue;
            }
            let Some((push_i, push_j)) = bump(&self.pets[i], &self.pets[j]) else {
                continue;
            };
            let (a, b) = self.pair_mut(i as u32, j as u32);
            a.knock(push_i, world, input);
            b.knock(push_j, world, input);
        }
    }

    /// Rencontres : un animal qui en atteint un autre en marchant réagit,
    /// et l'autre parfois avec lui (`petSide` de desktopPet).
    fn meet(&mut self, world: &World, input: &Input) {
        let mut near = std::mem::take(&mut self.near);
        for &(i, j) in near.update(&self.pets, false).reach {
            if self.is_still(i as usize) || self.is_still(j as usize) {
                continue;
            }
            // Chacun peut atteindre l'autre.
            for (from, to) in [(i, j), (j, i)] {
                let (a, b) = (&self.pets[from as usize], &self.pets[to as usize]);
                if !meeting(a, b) {
                    continue;
                }
                // Un énervé bouscule celui qu'il atteint au lieu de le
                // saluer : tous deux partent à la renverse.
                let jostle = a.temper.jostles().then(|| bump(a, b)).flatten();
                let (a, b) = self.pair_mut(from, to);
                if let Some((push_a, push_b)) = jostle {
                    a.knock(push_a, world, input);
                    b.knock(push_b, world, input);
                } else {
                    encounter(a, b, world, input);
                }
            }
        }
        self.near = near;
    }

    /// Les comportements contagieux : un voisin de la même famille, assez
    /// près, les imite parfois un peu plus tard (un mouton qui part entraîne
    /// les autres, un bâillement en appelle un autre, l'alerte se propage).
    fn echo(&mut self) {
        let mut sources = std::mem::take(&mut self.echo_sources);
        sources.clear();
        sources.extend(
            self.pets
                .iter_mut()
                .enumerate()
                .filter_map(|(i, pet)| pet.began.take().map(|b| (i, b))),
        );
        for &(i, b) in &sources {
            let source = &self.pets[i];
            let def = Arc::clone(&source.def);
            let behavior = &def.behaviors[b];
            let (pos, facing) = (source.pos, source.facing);
            let reach = ECHO_REACH * def.physics.width;
            let height = def.physics.height;
            let family = def.meta.category();
            for (j, pet) in self.pets.iter_mut().enumerate() {
                if j == i
                    || pet.echo.is_some()
                    || pet.echo_cooldown > 0.0
                    || (pet.pos.x - pos.x).abs() > reach
                    || (pet.pos.y - pos.y).abs() > height
                    || pet.def.meta.category() != family
                {
                    continue;
                }
                let Some(index) = pet.def.behavior_index(&behavior.name) else {
                    continue;
                };
                if pet.rng.chance(behavior.contagious) {
                    let delay = pet.rng.range(ECHO_DELAY.0, ECHO_DELAY.1);
                    pet.echo = Some(Echo {
                        behavior: index,
                        delay,
                        facing,
                        patience: ECHO_PATIENCE,
                    });
                }
            }
        }
        self.echo_sources = sources;
    }

    /// Libre de jouer : posé, éveillé, à lui, sans rien de prévu (ni
    /// regroupement, ni invitation), au repos ou en promenade.
    fn free_to_play(&self, index: usize) -> bool {
        let pet = &self.pets[index];
        matches!(pet.support, Support::Ground { .. })
            && pet.def.rules != Rules::DesktopPet
            && pet.can_react()
            && !pet.is_asleep()
            && pet.hold.is_none()
            && pet.regroup.is_none()
            && pet.goal.is_none()
            && pet.invite.is_none()
            && !pet.family.action.active()
            && !self.is_still(index)
            && matches!(
                pet.def.energy(pet.behavior),
                crate::Energy::Rest | crate::Energy::Walk
            )
    }

    /// Les agneaux s'invitent à jouer : de temps en temps, un agneau qui en
    /// a envie fixe un voisin de sa famille, tête basse, en sautillant
    /// (`play_invite`) ; l'autre accepte d'autant plus volontiers qu'il est
    /// actif, et ils s'élancent l'un vers l'autre (`play_lunge`, qui finit
    /// en coup de front).
    fn step_invites(&mut self, dt: f32) {
        // Les réponses.
        for i in 0..self.pets.len() {
            let Some(mut invite) = self.pets[i].invite else {
                continue;
            };
            invite.wait -= dt;
            if invite.wait > 0.0 {
                self.pets[i].invite = Some(invite);
                continue;
            }
            self.pets[i].invite = None;
            if self.pets[i].behavior_name() != "play_invite" {
                continue;
            }
            let Some(j) = self.pets.iter().position(|p| p.id == invite.partner) else {
                continue;
            };
            if !self.free_to_play(j) {
                continue;
            }
            let (a, b) = (&self.pets[i], &self.pets[j]);
            let width = a.def.physics.width.max(1.0);
            if (b.pos.x - a.pos.x).abs() > width * (INVITE_REACH.1 + 0.5) || a.support != b.support
            {
                continue;
            }
            let accept = (match b.temper {
                Temper::Lively => 0.85,
                Temper::Rowdy => 0.6,
                Temper::Even => 0.5,
                Temper::Calm => 0.2,
            } * b.contest_appetite(Some(a)))
            .min(1.0);
            let (Some(lunge_a), Some(lunge_b)) = (
                a.def.behavior_index("play_lunge"),
                b.def.behavior_index("play_lunge"),
            ) else {
                continue;
            };
            if !self.pets[j].rng.chance(accept) {
                continue;
            }
            let side = (self.pets[j].pos.x - self.pets[i].pos.x).signum();
            let (a, b) = self.pair_mut(i as u32, j as u32);
            a.face(side);
            b.face(-side);
            a.enter(lunge_a);
            b.enter(lunge_b);
        }
        // Les envies d'inviter.
        for i in 0..self.pets.len() {
            let pet = &mut self.pets[i];
            if pet.invite_in.is_infinite() {
                continue;
            }
            pet.invite_in -= dt;
            if pet.invite_in > 0.0 {
                continue;
            }
            pet.invite_in = pet.rng.range(INVITE_EVERY.0, INVITE_EVERY.1);
            let wants = (match pet.temper {
                Temper::Lively => 1.0,
                Temper::Even => 0.3,
                Temper::Rowdy => 0.15,
                Temper::Calm => 0.0,
            } * pet.contest_appetite(None))
            .min(1.0);
            if !pet.rng.chance(wants) || !self.free_to_play(i) {
                continue;
            }
            let pet = &self.pets[i];
            let Some(index) = pet.def.behavior_index("play_invite") else {
                continue;
            };
            let width = pet.def.physics.width.max(1.0);
            let family = pet.def.meta.category();
            let partner = (0..self.pets.len())
                .filter(|&j| j != i)
                .filter(|&j| {
                    let other = &self.pets[j];
                    let dx = (other.pos.x - pet.pos.x).abs();
                    other.support == pet.support
                        && other.def.meta.category() == family
                        && other.def.behavior_index("play_lunge").is_some()
                        && (INVITE_REACH.0 * width..=INVITE_REACH.1 * width).contains(&dx)
                        && self.free_to_play(j)
                })
                .min_by(|&a, &b| {
                    let d = |j: usize| (self.pets[j].pos.x - pet.pos.x).abs();
                    d(a).total_cmp(&d(b))
                });
            let Some(j) = partner else {
                continue;
            };
            let partner = self.pets[j].id;
            let side = (self.pets[j].pos.x - self.pets[i].pos.x).signum();
            let pet = &mut self.pets[i];
            pet.face(side);
            pet.enter(index);
            pet.invite = Some(Invite {
                partner,
                wait: INVITE_ANSWER,
            });
        }
    }

    /// De temps en temps (toutes les 5 à 12 minutes), le troupeau se
    /// regroupe ; s'il n'y a personne à regrouper, il réessaie un peu plus
    /// tard.
    fn step_regroup(&mut self, dt: f32) {
        if self.regroup_in <= 0.0 {
            self.regroup_in = self.next_regroup();
            return;
        }
        self.regroup_in -= dt;
        if self.regroup_in <= 0.0 {
            self.regroup_in = if self.regroup() {
                self.next_regroup()
            } else {
                30.0
            };
        }
    }

    /// Délai jusqu'au prochain regroupement (splitmix64 : sans toucher aux
    /// tirages des animaux).
    fn next_regroup(&mut self) -> f32 {
        self.regroup_seed = self.regroup_seed.wrapping_add(0x9E37_79B9_7F4A_7C15);
        let mut z = self.regroup_seed;
        z = (z ^ (z >> 30)).wrapping_mul(0xBF58_476D_1CE4_E5B9);
        z = (z ^ (z >> 27)).wrapping_mul(0x94D0_49BB_1331_11EB);
        z ^= z >> 31;
        let u = (z >> 40) as f32 / (1u64 << 24) as f32;
        REGROUP_EVERY.0 + (REGROUP_EVERY.1 - REGROUP_EVERY.0) * u
    }

    /// Le troupeau se regroupe, comme le font les moutons au pré : sur un
    /// même rebord, des animaux d'une même famille, dispersés, se
    /// rapprochent de leur milieu en ordre serré, sans changer d'ordre. Le
    /// plus loin de sa place part le premier ; les autres suivent comme une
    /// vague, d'autant plus tard qu'ils sont loin de lui. Renvoie `true`
    /// s'il y en avait à regrouper (trois au moins, bien dispersés).
    pub fn regroup(&mut self) -> bool {
        use std::collections::BTreeMap;
        let mut groups: BTreeMap<(SurfaceId, String), Vec<usize>> = BTreeMap::new();
        for (i, pet) in self.pets.iter().enumerate() {
            let Support::Ground { surface } = pet.support else {
                continue;
            };
            let free = matches!(
                pet.def.energy(pet.behavior),
                crate::Energy::Rest | crate::Energy::Walk
            );
            if pet.def.rules == Rules::DesktopPet
                || !free
                || !pet.can_react()
                || pet.is_asleep()
                || pet.hold.is_some()
                || pet.regroup.is_some()
                || pet.goal.is_some()
                || pet.family.action.active()
                || self.still == Some(pet.id)
            {
                continue;
            }
            groups
                .entry((surface, pet.def.meta.category().to_owned()))
                .or_default()
                .push(i);
        }
        let x = |i: usize| self.pets[i].pos.x;
        let best = groups
            .into_values()
            .filter(|group| group.len() >= 3)
            .map(|mut group| {
                group.sort_by(|&a, &b| x(a).total_cmp(&x(b)));
                let n = group.len() as f32;
                let width = group
                    .iter()
                    .map(|&i| self.pets[i].def.physics.width)
                    .sum::<f32>()
                    / n;
                let span = x(group[group.len() - 1]) - x(group[0]);
                let packed = width * REGROUP_SPACING * (n - 1.0);
                (span / packed.max(1.0), group, width, span)
            })
            .filter(|(spread, ..)| *spread > 1.8)
            .max_by(|a, b| a.0.total_cmp(&b.0));
        let Some((_, group, width, span)) = best else {
            return false;
        };
        let n = group.len();
        let center = group.iter().map(|&i| x(i)).sum::<f32>() / n as f32;
        let targets: Vec<f32> = (0..n)
            .map(|rank| center + (rank as f32 - (n - 1) as f32 * 0.5) * REGROUP_SPACING * width)
            .collect();
        let first = (0..n)
            .max_by(|&a, &b| {
                (x(group[a]) - targets[a])
                    .abs()
                    .total_cmp(&(x(group[b]) - targets[b]).abs())
            })
            .unwrap_or(0);
        let x0 = x(group[first]);
        for (rank, &i) in group.iter().enumerate() {
            let pet = &mut self.pets[i];
            let delay = if rank == first {
                0.0
            } else {
                0.6 + 5.0 * (pet.pos.x - x0).abs() / span.max(1.0) + pet.rng.range(0.0, 0.8)
            };
            pet.regroup = Some((targets[rank], delay));
        }
        true
    }

    /// Deux animaux différents du troupeau (leurs rangs), à changer ensemble.
    fn pair_mut(&mut self, i: u32, j: u32) -> (&mut Pet, &mut Pet) {
        let (i, j) = (i as usize, j as usize);
        debug_assert_ne!(i, j);
        if i < j {
            let (left, right) = self.pets.split_at_mut(j);
            (&mut left[i], &mut right[0])
        } else {
            let (left, right) = self.pets.split_at_mut(i);
            (&mut right[0], &mut left[j])
        }
    }

    /// Tous les animaux disent au revoir : chacun joue son animation
    /// d'adieu, s'efface, puis quitte le troupeau.
    pub fn leave_all(&mut self, world: &World, input: &Input) {
        for pet in &mut self.pets {
            pet.leave(world, input);
        }
        self.generation += 1;
    }

    /// Un animal de cette espèce (identifiant de sa définition) dit au
    /// revoir : le dernier arrivé parmi ceux qui ne partent pas déjà (les
    /// personnages secondaires finissent leur rôle d'eux-mêmes). Renvoie son
    /// numéro, s'il y en avait un.
    pub fn leave_one(&mut self, species: &str, world: &World, input: &Input) -> Option<u32> {
        let pet = self.pets.iter_mut().rev().find(|p| {
            !p.is_child() && !p.is_leaving() && !p.is_finished() && p.def.meta.id == species
        })?;
        pet.leave(world, input);
        self.generation += 1;
        Some(pet.id())
    }

    /// Cet animal-là (son numéro) dit au revoir, s'il ne part pas déjà et
    /// n'est pas un personnage secondaire. Renvoie `true` s'il s'en va.
    pub fn leave(&mut self, id: u32, world: &World, input: &Input) -> bool {
        let Some(pet) = self
            .pets
            .iter_mut()
            .find(|p| p.id() == id && !p.is_child() && !p.is_leaving() && !p.is_finished())
        else {
            return false;
        };
        pet.leave(world, input);
        self.generation += 1;
        true
    }

    /// Tous ensemble : chaque animal libre joue son comportement `together`.
    /// Renvoie le nombre d'animaux qui le jouent.
    pub fn together(&mut self, world: &World, input: &Input) -> usize {
        self.pets
            .iter_mut()
            .filter_map(|pet| pet.together(world, input).then_some(()))
            .count()
    }

    /// Fait apparaître les personnages secondaires demandés et retire ceux
    /// qui ont terminé.
    fn adopt_children(&mut self, world: &World) {
        let mut born = Vec::new();
        for pet in &mut self.pets {
            for request in pet.children.drain(..) {
                born.push((Arc::clone(&pet.species), pet.rng.next_u32(), request));
            }
        }
        let before = self.pets.len();
        let arrived = !born.is_empty();
        for (def, seed, request) in born {
            let id = self.next_id;
            self.next_id += 1;
            self.pets
                .push(Pet::child(id, def, u64::from(seed), world, request));
        }
        self.pets.retain(|p| !p.finished);
        if arrived || self.pets.len() != before {
            self.generation += 1;
        }
    }

    /// L'animal de ce rang attend-il, immobile ?
    fn is_still(&self, index: usize) -> bool {
        self.still.is_some() && self.still == self.pets.get(index).map(|p| p.id)
    }

    /// Activité la plus élevée du troupeau (l'animal qui attend, immobile,
    /// n'en a pas).
    #[must_use]
    pub fn activity(&self) -> Option<Activity> {
        // Une surprise en cours (soucoupe, vent) bouge à toute vitesse.
        if !self.saucers.is_empty() || self.gust.is_some() {
            return Some(Activity::Moving);
        }
        self.pets
            .iter()
            .filter(|p| self.still != Some(p.id))
            .map(Pet::activity)
            .max()
    }

    /// Images par seconde qu'il faut aux animaux qui bougent (voir
    /// [`Pet::smooth_rate`]) ; 0 si aucun ne bouge.
    #[must_use]
    pub fn smooth_rate(&self) -> f32 {
        if !self.saucers.is_empty() || self.gust.is_some() {
            return f32::INFINITY;
        }
        self.pets
            .iter()
            .filter(|p| self.still != Some(p.id) && p.activity() == Activity::Moving)
            .map(Pet::smooth_rate)
            .fold(0.0, f32::max)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::format::parse_pet_toml;
    use crate::math::Rect;
    use crate::world::{LedgeOptions, Screen, WindowRect};

    const PET: &str = r#"
        [pet]
        id = "t"
        name = "T"
        [appearance]
        rig = "a.svg"
        height = 60.0
        [reflexes]
        spawn = "fall"
        fall = "fall"
        land = "stand"
        thrown = "fall"
        edge = "turn"
        wall = "turn"
        [[behavior]]
        name = "stand"
        motion = "stand"
        duration = 0.5
        next = "walk"
        [[behavior]]
        name = "walk"
        motion = "walk"
        speed = 60.0
        duration = 30
        next = "stand"
        [[behavior]]
        name = "turn"
        motion = "turn"
        duration = 0.2
        next = "walk"
        [[behavior]]
        name = "fall"
        motion = "fall"
        duration = "forever"
    "#;

    fn def() -> Arc<PetDef> {
        Arc::new(parse_pet_toml(PET, |_: &str| Ok(b"<svg/>".to_vec())).unwrap())
    }

    fn world() -> World {
        World::new(vec![Screen {
            frame: Rect::new(0.0, 0.0, 1440.0, 900.0),
            usable: Rect::new(0.0, 25.0, 1440.0, 800.0),
            scale: 2.0,
        }])
    }

    fn run(pet: &mut Pet, world: &World, seconds: f32) {
        let input = Input {
            cursor: None,
            local_hour: 14.0,
            ..Input::default()
        };
        for _ in 0..(seconds / TICK) as usize {
            pet.step(TICK, world, &input);
        }
    }

    #[test]
    fn falls_and_lands_on_the_floor() {
        let world = world();
        let mut pet = Pet::spawn(0, def(), 1, &world, Some(400.0));
        assert_eq!(pet.support(), Support::Air);
        run(&mut pet, &world, 3.0);
        assert!(
            matches!(pet.support(), Support::Ground { .. }),
            "{:?}",
            pet.support()
        );
        assert_eq!(pet.position().y, 825.0);
        let events = pet.take_events();
        assert!(
            events
                .iter()
                .any(|e| matches!(e, PetEvent::Landed { impact } if *impact > 1000.0)),
            "{events:?}"
        );
    }

    #[test]
    fn lands_on_a_window_ledge() {
        let mut world = world();
        world.set_windows(
            &[WindowRect {
                id: 5,
                frame: Rect::new(200.0, 400.0, 600.0, 300.0),
            }],
            LedgeOptions::default(),
        );
        let mut pet = Pet::spawn(0, def(), 1, &world, Some(500.0));
        run(&mut pet, &world, 2.0);
        assert_eq!(pet.position().y, 400.0);
    }

    #[test]
    fn turns_at_the_edge_instead_of_falling() {
        let mut world = world();
        world.set_windows(
            &[WindowRect {
                id: 5,
                frame: Rect::new(200.0, 400.0, 300.0, 300.0),
            }],
            LedgeOptions::default(),
        );
        let mut pet = Pet::spawn(0, def(), 3, &world, Some(350.0));
        run(&mut pet, &world, 20.0);
        assert_eq!(pet.position().y, 400.0, "il ne doit pas tomber du rebord");
        assert!((200.0..=500.0).contains(&pet.position().x));
    }

    #[test]
    fn rides_a_moving_window_and_falls_when_it_closes() {
        let mut world = world();
        let mut frame = Rect::new(200.0, 400.0, 600.0, 300.0);
        world.set_windows(&[WindowRect { id: 5, frame }], LedgeOptions::default());
        let mut pet = Pet::spawn(0, def(), 4, &world, Some(500.0));
        run(&mut pet, &world, 2.0);
        let before = pet.position();
        frame.x += 100.0;
        frame.y -= 50.0;
        world.set_windows(&[WindowRect { id: 5, frame }], LedgeOptions::default());
        pet.step(TICK, &world, &Input::default());
        let after = pet.position();
        assert!(
            (after.x - before.x - 100.0).abs() < 2.0,
            "{before:?} -> {after:?}"
        );
        assert_eq!(after.y, 350.0);

        world.set_windows(&[], LedgeOptions::default());
        run(&mut pet, &world, 2.0);
        assert_eq!(pet.position().y, 825.0);
    }

    #[test]
    fn can_be_grabbed_and_thrown() {
        let world = world();
        let mut pet = Pet::spawn(0, def(), 5, &world, Some(700.0));
        run(&mut pet, &world, 2.0);
        let start = pet.position();
        pet.grab(start - Vec2::new(0.0, 30.0), &world);
        assert!(pet.is_held());
        for i in 0..30 {
            pet.drag_to(start + Vec2::new(i as f32 * 12.0, -300.0 - i as f32 * 4.0));
            pet.step(TICK, &world, &Input::default());
        }
        assert!(pet.position().y < start.y - 100.0);
        pet.release(&world);
        assert_eq!(pet.support(), Support::Air);
        assert!(pet.velocity().x > 0.0, "l'élan est conservé");
        run(&mut pet, &world, 4.0);
        assert!(matches!(pet.support(), Support::Ground { .. }));
    }

    #[test]
    fn a_slow_walk_needs_only_sixty_frames_per_second() {
        let world = world();
        let mut pet = Pet::spawn(0, def(), 6, &world, Some(700.0));
        pet.step(TICK, &world, &Input::default());
        assert_eq!(pet.smooth_rate(), f32::INFINITY, "en chute libre");
        run(&mut pet, &world, 3.0);
        pet.force("walk");
        run(&mut pet, &world, 2.0);
        assert!((pet.velocity().x.abs() - 60.0).abs() < 0.1);
        assert_eq!(pet.smooth_rate(), SLOW_RATE, "au pas, à 60 pt/s");

        let start = pet.position();
        pet.grab(start - Vec2::new(0.0, 30.0), &world);
        pet.step(TICK, &world, &Input::default());
        assert_eq!(pet.smooth_rate(), f32::INFINITY, "tenu");

        // Plus vite qu'un point par image à 60 images par seconde.
        let fast = Arc::new(
            parse_pet_toml(&PET.replace("speed = 60.0", "speed = 61.0"), |_: &str| {
                Ok(b"<svg/>".to_vec())
            })
            .unwrap(),
        );
        let mut pet = Pet::spawn(0, fast, 6, &world, Some(700.0));
        run(&mut pet, &world, 3.0);
        pet.force("walk");
        run(&mut pet, &world, 2.0);
        assert_eq!(pet.smooth_rate(), f32::INFINITY, "à 61 pt/s");
    }

    fn two_screens() -> World {
        World::new(vec![
            Screen {
                frame: Rect::new(0.0, 0.0, 1440.0, 900.0),
                usable: Rect::new(0.0, 25.0, 1440.0, 800.0),
                scale: 2.0,
            },
            Screen {
                frame: Rect::new(1440.0, 0.0, 1440.0, 900.0),
                usable: Rect::new(1440.0, 25.0, 1440.0, 800.0),
                scale: 2.0,
            },
        ])
    }

    #[test]
    fn walks_on_to_the_next_screen() {
        let world = two_screens();
        let mut pet = Pet::spawn(0, def(), 2, &world, Some(1380.0));
        run(&mut pet, &world, 2.0);
        pet.facing = 1.0;
        pet.force("walk");
        run(&mut pet, &world, 3.0);
        assert!(pet.position().x > 1460.0, "{:?}", pet.position());
        assert_eq!(pet.position().y, 825.0);
        let Support::Ground { surface } = pet.support() else {
            panic!("{:?}", pet.support())
        };
        assert_eq!(world.surface(surface).unwrap().x0, 1440.0);
    }

    #[test]
    fn simulation_is_deterministic() {
        let world = world();
        let mut a = Pet::spawn(0, def(), 99, &world, None);
        let mut b = Pet::spawn(0, def(), 99, &world, None);
        run(&mut a, &world, 12.0);
        run(&mut b, &world, 12.0);
        assert_eq!(a.position(), b.position());
        assert_eq!(a.behavior_name(), b.behavior_name());
    }

    #[test]
    fn flock_interpolation_factor_is_in_range() {
        let world = world();
        let mut flock = Flock::new();
        flock.spawn(def(), 1, &world, None);
        for _ in 0..100 {
            let alpha = flock.advance(1.0 / 60.0, &world, &Input::default());
            assert!((0.0..1.0).contains(&alpha));
        }
    }

    // ------------------------------------------------------------ animal en images

    fn strips() -> Arc<PetDef> {
        Arc::new(
            parse_pet_toml(
                crate::format::tests::STRIPS,
                crate::format::tests::strips_resolve,
            )
            .unwrap(),
        )
    }

    #[test]
    fn inbetweens_show_up_at_the_end_of_each_step() {
        let world = world();
        let def = Arc::new(
            parse_pet_toml(
                &crate::format::tests::strips_with_inbetweens(),
                crate::format::tests::inbetweens_resolve,
            )
            .unwrap(),
        );
        // Marche à 10 images par seconde : chaque image dure 0,1 s, la
        // seconde moitié montre l'image intermédiaire vers la suivante.
        let mut pet = standing(def, &world, 700.0, -1.0, "marche");
        let mut seen = Vec::new();
        for _ in 0..(0.4 / TICK).round() as usize {
            pet.step(TICK, &world, &Input::default());
            let frame = pet.view(1.0).sprite_frame.unwrap();
            if seen.last() != Some(&frame) {
                seen.push(frame);
            }
        }
        assert_eq!(&seen[..6], &[4, 34, 5, 35, 6, 36], "{seen:?}");
        // Après la dernière image de la marche, rien : elle est sautée.
        assert!(!seen.contains(&37), "{seen:?}");
    }

    #[test]
    fn inbetweens_spread_over_held_frames() {
        let world = world();
        let mut def = parse_pet_toml(
            &crate::format::tests::strips_with_inbetweens(),
            crate::format::tests::inbetweens_resolve,
        )
        .unwrap();
        // Chaque image tenue deux pas, comme desktopPet le fait souvent : le
        // passage vers la suivante se prend sur toute la tenue (0,2 s), pas
        // seulement sur la fin du second pas.
        let walk = def.behavior_index("marche").unwrap();
        def.behaviors[walk].clip.as_mut().unwrap().frames = vec![4, 4, 5, 5, 6, 6, 7, 7];
        let mut pet = standing(Arc::new(def), &world, 700.0, -1.0, "marche");
        let mut shown: Vec<(u32, usize)> = Vec::new();
        for _ in 0..(0.8 / TICK).round() as usize {
            pet.step(TICK, &world, &Input::default());
            let frame = pet.view(1.0).sprite_frame.unwrap();
            match shown.last_mut() {
                Some((f, n)) if *f == frame => *n += 1,
                _ => shown.push((frame, 1)),
            }
        }
        let frames: Vec<u32> = shown.iter().map(|(f, _)| *f).collect();
        assert_eq!(&frames[..6], &[4, 34, 5, 35, 6, 36], "{shown:?}");
        // 0,08 s pour l'image intermédiaire (le plus long passage), 0,12 s
        // pour l'image tenue ; avant, 0,05 et 0,15 s.
        let ticks = |frame: u32| shown.iter().find(|(f, _)| *f == frame).unwrap().1;
        assert!((9..=10).contains(&ticks(34)), "{shown:?}");
        assert!((14..=15).contains(&ticks(5)), "{shown:?}");
    }

    #[test]
    fn behaviors_play_their_sounds() {
        let world = world();
        let def = Arc::new(
            parse_pet_toml(
                &crate::format::tests::strips_with_sounds(),
                crate::format::tests::sounds_resolve,
            )
            .unwrap(),
        );
        let sounds = |pet: &mut Pet| -> Vec<usize> {
            pet.take_events()
                .into_iter()
                .filter_map(|e| match e {
                    PetEvent::Sound { sound, clip } => {
                        assert!(clip < pet.def.sounds[sound].clips.len());
                        Some(sound)
                    }
                    _ => None,
                })
                .collect()
        };
        // Le repos bêle dès qu'il commence.
        let mut pet = standing(Arc::clone(&def), &world, 700.0, -1.0, "repos");
        assert_eq!(sounds(&mut pet), vec![0]);
        // La marche fait tinter la clochette sur sa troisième image, à chaque
        // passage (trois passages de quatre images à 10 images par seconde).
        let mut pet = standing(def, &world, 700.0, -1.0, "marche");
        pet.take_events();
        let mut bells = 0;
        for _ in 0..(1.15 / TICK).round() as usize {
            pet.step(TICK, &world, &Input::default());
            bells += sounds(&mut pet).iter().filter(|s| **s == 1).count();
        }
        assert_eq!(bells, 3);
    }

    /// Animal posé au sol, dans le comportement voulu.
    fn standing(def: Arc<PetDef>, world: &World, x: f32, facing: f32, behavior: &str) -> Pet {
        let floor = *world.floor_under(x).unwrap();
        let mut pet = Pet::spawn_at(0, def, 3, world, Vec2::new(x, floor.y));
        pet.pos = Vec2::new(x, floor.y);
        pet.prev_pos = pet.pos;
        pet.set_ground(&floor);
        pet.facing = facing;
        pet.anim.flip = facing;
        assert!(pet.force(behavior));
        pet
    }

    #[test]
    fn a_strip_pet_moves_frame_by_frame() {
        let world = world();
        let mut pet = standing(strips(), &world, 700.0, 1.0, "marche");
        // 4 images à 10 im/s, 3 passages : 1,2 s, 4 pixels par image à l'échelle 0,5.
        run(&mut pet, &world, 1.19);
        assert_eq!(pet.behavior_name(), "marche");
        let moved = pet.position().x - 700.0;
        assert!((moved - 24.0).abs() < 0.6, "{moved}");
        // Fini : pas de pas supplémentaire comme dans desktopPet, il passe à la suite.
        run(&mut pet, &world, 0.02);
        assert!(pet.behavior_time < 0.02, "{}", pet.behavior_time);
    }

    #[test]
    fn a_strip_pet_turns_with_its_own_frames() {
        let world = world();
        let mut pet = standing(strips(), &world, 22.0, -1.0, "marche");
        run(&mut pet, &world, 0.35);
        assert_eq!(
            pet.behavior_name(),
            "demi_tour",
            "le bord de l'écran le fait se retourner"
        );
        assert!(
            (pet.position().x - 16.0).abs() < 0.01,
            "{:?}",
            pet.position()
        );
        assert_eq!(pet.facing, -1.0, "le demi-tour est dans les images");
        // 3 images à 12 im/s : 0,25 s, puis il regarde de l'autre côté.
        run(&mut pet, &world, 0.25);
        assert_eq!(pet.facing, 1.0);
        assert_eq!(pet.behavior_name(), "marche");
    }

    #[test]
    fn per_frame_moves_follow_the_frames() {
        let world = world();
        let mut pet = standing(strips(), &world, 700.0, 1.0, "cogne");
        // [0, -4, -2, 0] pixels à 10 im/s, à l'échelle 0,5 : 3 points en arrière.
        run(&mut pet, &world, 0.39);
        assert!(
            (pet.position().x - 697.0).abs() < 0.05,
            "{:?}",
            pet.position()
        );
    }

    #[test]
    fn a_strip_pet_falls_when_its_ledge_ends() {
        let mut world = world();
        world.set_windows(
            &[WindowRect {
                id: 5,
                frame: Rect::new(200.0, 400.0, 300.0, 300.0),
            }],
            LedgeOptions::default(),
        );
        let ledge = *world
            .surfaces()
            .iter()
            .find(|s| s.kind == crate::world::SurfaceKind::Window)
            .unwrap();
        let mut pet = standing(strips(), &world, 480.0, 1.0, "marche");
        pet.set_ground(&ledge);
        pet.pos.y = ledge.y;
        pet.walking_off = true;
        run(&mut pet, &world, 3.0);
        assert_eq!(pet.position().y, 825.0, "tombé du rebord jusqu'au sol");
        assert!(pet.position().x > 505.0, "il est parti en avant");
    }

    #[test]
    fn a_held_pet_stays_where_it_was_caught() {
        let world = world();
        let mut pet = standing(strips(), &world, 700.0, -1.0, "repos");
        // Attrapé par le bas, à droite des pattes.
        let caught = pet.position() + Vec2::new(10.0, -3.0);
        pet.grab(caught, &world);
        assert_eq!(pet.behavior_name(), "tenu");
        // Comme une fenêtre : le point de prise reste sous le curseur, à
        // chaque instant, sans glisser ni traîner derrière lui.
        for i in 1..=120 {
            let cursor = caught + Vec2::new(i as f32 * 3.0, -(i as f32) * 5.0);
            pet.drag_to(cursor);
            pet.step(TICK, &world, &Input::default());
            let held = pet.position() + Vec2::new(10.0, -3.0);
            assert!((held - cursor).length() < 1e-3, "{i} : {held:?} {cursor:?}");
        }
        // Tout en haut de l'écran aussi, et au-delà.
        let top = Vec2::new(600.0, -40.0);
        pet.drag_to(top);
        run(&mut pet, &world, 0.5);
        assert!((pet.position() + Vec2::new(10.0, -3.0) - top).length() < 1e-3);
        // La main s'arrête avant de le lâcher : il tombe sans élan.
        pet.release(&world);
        assert!(pet.velocity().length() < 5.0, "{:?}", pet.velocity());
        assert_eq!(
            pet.behavior_name(),
            "chute",
            "lâché sans élan, il tombe simplement"
        );
    }

    #[test]
    fn colliding_pets_tumble_backwards_and_land_upright() {
        let world = world();
        let mut flock = Flock::new();
        let def = strips();
        let a = flock.spawn(Arc::clone(&def), 1, &world, Some(600.0));
        let b = flock.spawn(Arc::clone(&def), 2, &world, Some(640.0));
        let input = Input::default();
        for _ in 0..(3.0 / TICK) as usize {
            flock.advance(TICK, &world, &input);
        }
        // Face à face, et l'un marche vers l'autre.
        for (id, x, facing, behavior) in [(a, 600.0, 1.0, "marche"), (b, 640.0, -1.0, "repos")] {
            let pet = flock.get_mut(id).unwrap();
            assert!(matches!(pet.support(), Support::Ground { .. }));
            pet.pos.x = x;
            pet.prev_pos = pet.pos;
            pet.facing = facing;
            pet.force(behavior);
        }
        flock.set_collisions(true);
        let mut bumped = Vec::new();
        let mut max_roll: f32 = 0.0;
        for _ in 0..(1.4 / TICK) as usize {
            flock.advance(TICK, &world, &input);
            for pet in flock.pets_mut() {
                max_roll = max_roll.max(pet.anim.roll.abs());
                for e in pet.take_events() {
                    if matches!(e, PetEvent::Bumped { .. }) {
                        bumped.push(pet.id());
                    }
                }
            }
        }
        bumped.sort_unstable();
        assert_eq!(bumped, vec![a, b], "les deux sont heurtés, une seule fois");
        assert!(max_roll > 3.0, "ils culbutent ({max_roll})");
        let pets = flock.pets();
        let (pa, pb) = (&pets[0], &pets[1]);
        assert!(
            pb.position().x - pa.position().x > 70.0,
            "écartés : {:?} {:?}",
            pa.position(),
            pb.position()
        );
        for pet in pets {
            assert!(matches!(pet.support(), Support::Ground { .. }));
            assert_eq!(pet.anim.roll, 0.0, "il finit sur ses pattes");
        }
        assert_eq!(
            (pa.facing, pb.facing),
            (1.0, -1.0),
            "ils ont reculé sans se retourner"
        );
    }

    #[test]
    fn desktoppet_pets_turn_their_backs_after_a_bump() {
        // Sans idée à lui, un animal desktopPet repartirait droit sur l'autre :
        // après la culbute, il se pose le dos tourné au choc.
        let def = fixture("pelote");
        let world = world();
        let mut flock = Flock::new();
        let a = flock.spawn(Arc::clone(&def), 1, &world, Some(600.0));
        let b = flock.spawn(Arc::clone(&def), 2, &world, Some(720.0));
        let input = Input::default();
        for _ in 0..(9.0 / TICK) as usize {
            flock.advance(TICK, &world, &input);
        }
        for (id, x, facing) in [(a, 600.0, 1.0), (b, 720.0, -1.0)] {
            let pet = flock.get_mut(id).unwrap();
            assert!(
                matches!(pet.support(), Support::Ground { .. }),
                "{}",
                pet.behavior_name()
            );
            pet.pos.x = x;
            pet.prev_pos = pet.pos;
            pet.facing = facing;
            pet.anim.flip = facing;
            assert!(pet.force("walk"));
        }
        flock.set_collisions(true);
        let mut bumped = Vec::new();
        let mut landed: Vec<(u32, f32)> = Vec::new();
        for _ in 0..(5.0 / TICK) as usize {
            flock.advance(TICK, &world, &input);
            for pet in flock.pets_mut() {
                for e in pet.take_events() {
                    match e {
                        PetEvent::Bumped { .. } => bumped.push(pet.id()),
                        PetEvent::Landed { .. }
                            if bumped.contains(&pet.id())
                                && !landed.iter().any(|(id, _)| *id == pet.id()) =>
                        {
                            landed.push((pet.id(), pet.facing));
                        }
                        _ => {}
                    }
                }
            }
        }
        bumped.sort_unstable();
        assert_eq!(bumped, vec![a, b], "les deux sont heurtés, une seule fois");
        landed.sort_by_key(|(id, _)| *id);
        assert_eq!(landed, vec![(a, -1.0), (b, 1.0)], "dos à dos");
    }

    #[test]
    fn collisions_are_off_by_default() {
        let world = world();
        let mut flock = Flock::new();
        let def = strips();
        let a = flock.spawn(Arc::clone(&def), 1, &world, Some(600.0));
        let b = flock.spawn(Arc::clone(&def), 2, &world, Some(620.0));
        for _ in 0..(3.0 / TICK) as usize {
            flock.advance(TICK, &world, &Input::default());
        }
        for pet in flock.pets_mut() {
            assert!(
                !pet.take_events()
                    .iter()
                    .any(|e| matches!(e, PetEvent::Bumped { .. })),
                "{} {a} {b}",
                pet.id()
            );
        }
    }

    /// Longue promenade d'un animal des données de test (Lainou, en bandes
    /// d'images, ou Pelote, desktopPet) sur un bureau qui change, en mode
    /// collisions : il ne sort pas des écrans, ne reste pas coincé dans une
    /// animation et joue l'essentiel de son répertoire. Renvoie le temps
    /// passé dans chaque comportement.
    fn long_life(
        pet: &str,
        minutes: f32,
        margin: f32,
        collisions: bool,
    ) -> (Arc<PetDef>, std::collections::BTreeMap<String, f32>) {
        long_life_with(pet, minutes, margin, collisions, false)
    }

    fn long_life_with(
        pet: &str,
        minutes: f32,
        margin: f32,
        collisions: bool,
        realistic: bool,
    ) -> (Arc<PetDef>, std::collections::BTreeMap<String, f32>) {
        use std::collections::BTreeMap;
        let def = fixture(pet);
        let mut world = world();
        let mut flock = Flock::new();
        flock.set_collisions(collisions);
        flock.set_realistic(realistic);
        for seed in 0..4 {
            flock.spawn(Arc::clone(&def), seed * 7919 + 1, &world, None);
        }
        let mut seconds_in: BTreeMap<String, f32> = BTreeMap::new();
        let mut same_since: Vec<(u32, String, f32)> = Vec::new();
        for i in 0..(minutes * 60.0 / TICK) as usize {
            let t = i as f32 * TICK;
            let windows = match (t / 50.0) as i32 % 3 {
                0 => vec![WindowRect {
                    id: 1,
                    frame: Rect::new(200.0 + 150.0 * (t * 0.05).sin(), 420.0, 500.0, 300.0),
                }],
                1 => vec![WindowRect {
                    id: 2,
                    frame: Rect::new(900.0, 250.0, 380.0, 500.0),
                }],
                _ => Vec::new(),
            };
            world.set_windows(&windows, LedgeOptions::default());
            let input = Input {
                cursor: Some(Vec2::new(700.0, 700.0)),
                local_hour: if t > minutes * 30.0 { 23.0 } else { 15.0 },
                ..Input::default()
            };
            flock.advance(TICK, &world, &input);
            for pet in flock.pets() {
                let p = pet.position();
                assert!(
                    p.x.is_finite()
                        && p.y.is_finite()
                        && p.x > -margin
                        && p.x < 1440.0 + margin
                        && p.y > -margin
                        && p.y <= 825.0 + 0.01,
                    "{p:?} {} angle {} spin {} vel {:?} carry {:?} support {:?} t {t}",
                    pet.behavior_name(),
                    pet.angle,
                    pet.spin,
                    pet.vel,
                    pet.carry,
                    pet.support
                );
                *seconds_in
                    .entry(pet.behavior_name().to_owned())
                    .or_default() += TICK;
                // Pas plus de cinq minutes d'affilée dans la même animation.
                let name = pet.behavior_name().to_owned();
                match same_since.iter_mut().find(|(id, _, _)| *id == pet.id()) {
                    Some(entry) if entry.1 == name => {
                        assert!(t - entry.2 < 300.0, "{} bloqué dans {name}", pet.id());
                    }
                    Some(entry) => *entry = (pet.id(), name, t),
                    None => same_since.push((pet.id(), name, t)),
                }
            }
        }
        for (name, s) in &seconds_in {
            eprintln!("{name:>20} : {s:7.0} s");
        }
        (def, seconds_in)
    }

    /// Quatre Lainou, `minutes` durant, de ce tempérament : la part de leur
    /// temps passée au repos (debout, assis, à dormir, à brouter), et le
    /// temps passé à la renverse (heurtés).
    fn lives_of(mood: Mood, minutes: f32) -> (f32, f32) {
        let def = fixture("lainou");
        let world = world();
        let mut flock = Flock::new();
        flock.set_mood(mood);
        for seed in 0..4 {
            flock.spawn(Arc::clone(&def), seed * 7919 + 1, &world, None);
        }
        let input = Input {
            cursor: Some(Vec2::new(700.0, 700.0)),
            local_hour: 15.0,
            ..Input::default()
        };
        let (mut resting, mut moving, mut tumbling) = (0.0, 0.0, 0.0);
        for _ in 0..(minutes * 60.0 / TICK) as usize {
            flock.advance(TICK, &world, &input);
            for pet in flock.pets() {
                assert_eq!(pet.temper(), mood.temper_of(pet.id()));
                match def.energy(pet.behavior) {
                    crate::Energy::Rest | crate::Energy::Sleep => resting += TICK,
                    crate::Energy::Walk | crate::Energy::Dash => moving += TICK,
                    crate::Energy::Neutral => {}
                }
                if pet.behavior_name() == "culbute" {
                    tumbling += TICK;
                }
            }
        }
        (resting / (resting + moving), tumbling)
    }

    #[test]
    fn tempers_change_how_the_flock_spends_its_time() {
        let (calm, _) = lives_of(Mood::Calm, 6.0);
        let (even, even_tumbling) = lives_of(Mood::Even, 6.0);
        let (lively, _) = lives_of(Mood::Lively, 6.0);
        let (rowdy, rowdy_tumbling) = lives_of(Mood::Rowdy, 6.0);
        eprintln!(
            "au repos : calmes {calm:.2}, tels quels {even:.2}, actifs {lively:.2}, énervés {rowdy:.2}"
        );
        assert!(calm > even + 0.1, "calmes {calm} contre {even}");
        assert!(lively < even - 0.1, "actifs {lively} contre {even}");
        assert!(rowdy < even - 0.1, "énervés {rowdy} contre {even}");
        // Sans mode collisions, seuls les énervés se bousculent.
        assert_eq!(even_tumbling, 0.0);
        assert!(
            rowdy_tumbling > 0.0,
            "les énervés ne se sont jamais bousculés"
        );
    }

    fn never_played<'a>(
        def: &'a PetDef,
        seconds_in: &std::collections::BTreeMap<String, f32>,
    ) -> Vec<&'a str> {
        def.behaviors
            .iter()
            .map(|b| b.name.as_str())
            .filter(|n| !seconds_in.contains_key(*n))
            .collect()
    }

    fn bundled(name: &str) -> Arc<PetDef> {
        let path = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
            .join(format!("../../pets/{name}/pet.toml"));
        Arc::new(crate::format::load_pet(&path).unwrap())
    }

    /// Un animal des données de test (`tests/donnees`), que l'app ne livre
    /// pas : Lainou, en bandes d'images, ou Pelote, desktopPet.
    fn fixture(name: &str) -> Arc<PetDef> {
        let path = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
            .join(format!("../../tests/donnees/{name}/pet.toml"));
        Arc::new(crate::format::load_pet(&path).unwrap())
    }

    fn bundled_lamb(name: &str) -> Arc<PetDef> {
        let mut def = (*bundled(name)).clone();
        def.age = crate::Age::Lamb { weeks: 2.0 };
        Arc::new(def)
    }

    #[test]
    fn every_sheep_knows_what_to_do_with_the_wind_and_its_temper() {
        // Le vent dans le museau : tous les moutons lui tournent le dos.
        for name in [
            "bleu-du-maine",
            "solognote",
            "merinos-d-arles",
            "southdown",
            "mouflon-corse",
            "nez-noir",
            "jacob",
            "ouessant",
        ] {
            let def = bundled(name);
            assert!(
                def.reflexes
                    .wind
                    .iter()
                    .any(|t| t.when == [Condition::IntoWind]),
                "{name}"
            );
        }
        // Les animaux d'exemple montrent tous les gestes, et des choix selon
        // le tempérament ; leurs comportements contagieux sont bien lus.
        for name in ["bleu-du-maine", "solognote"] {
            let def = bundled(name);
            for gesture in Gesture::ALL
                .into_iter()
                .filter(|g| !matches!(g, Gesture::Nurse | Gesture::MaternalCall))
            {
                assert!(
                    def.behaviors
                        .iter()
                        .any(|b| b.motion == Motion::Gesture(gesture)),
                    "{name} : {gesture:?}"
                );
            }
            for temper in [Temper::Calm, Temper::Lively, Temper::Rowdy] {
                assert!(
                    def.behaviors
                        .iter()
                        .flat_map(|b| &b.next)
                        .any(|t| t.when == [Condition::Temper(temper)]),
                    "{name} : {temper:?}"
                );
            }
            let walk = &def.behaviors[def.behavior_index("walk").unwrap()];
            assert!(walk.contagious > 0.0 && walk.contagious < 1.0);
            let ruminate = &def.behaviors[def.behavior_index("ruminate").unwrap()];
            assert!(ruminate.chew && ruminate.motion == Motion::Sit);
        }
    }

    #[test]
    fn every_breed_lives_like_a_sheep_and_keeps_the_ways_of_its_kind() {
        // Toutes les races du dépôt gardent ce que sait faire le Bleu du
        // Maine, l'exemple complet du format (le moteur trouve certains
        // comportements par leur nom : jeux, invitations…), sauf ce que leur
        // race ne fait pas :
        let without = |name: &str| -> &[&str] {
            match name {
                // le mouflon de Corse et le Dorper chargent sans se cabrer ;
                "mouflon-corse" | "dorper" => &["rear"],
                // la Racka menace par une approche lente et une secousse,
                // sans charge frontale ni cabrement ;
                "racka" => &["rear", "charge"],
                // le floucat, castré et apprivoisé, ne se bat ni ne joue à se
                // battre, et n'a pas de flehmen.
                "floucat" => &[
                    "play_lunge",
                    "play_butt",
                    "play_invite",
                    "flehmen",
                    "paw",
                    "rear",
                    "charge",
                ],
                _ => &[],
            }
        };
        let model = bundled("bleu-du-maine");
        let pets = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../pets");
        let mut breeds: Vec<String> = std::fs::read_dir(&pets)
            .unwrap()
            .filter_map(|e| e.ok()?.file_name().into_string().ok())
            .filter(|name| pets.join(name).join("pet.toml").is_file())
            .collect();
        breeds.sort();
        // Vingt-huit races, et le floucat.
        assert_eq!(breeds.len(), 29, "{breeds:?}");
        for name in &breeds {
            let def = bundled(name);
            assert_eq!(def.meta.category.as_deref(), Some("sheep"), "{name}");
            for behavior in &model.behaviors {
                let kept = def.behavior_index(&behavior.name).is_some();
                let left_out = without(name).contains(&behavior.name.as_str());
                assert!(kept != left_out, "{name} : {}", behavior.name);
            }
        }
        let next = |def: &PetDef, name: &str| -> Vec<String> {
            def.behaviors[def.behavior_index(name).unwrap()]
                .next
                .iter()
                .map(|t| def.behaviors[t.to].name.clone())
                .collect()
        };
        // Le mouflon de Corse ne se cabre pas (aucune source ne le décrit ;
        // c'est le mouflon canadien) : il recule, puis il charge. Sauvage, il
        // guette plus que les autres et fuit au galop à la première alarme.
        let mouflon = bundled("mouflon-corse");
        assert!(
            !mouflon
                .behaviors
                .iter()
                .any(|b| b.motion == Motion::Gesture(Gesture::Rear))
        );
        assert!(next(&mouflon, "back_up").iter().all(|n| n == "charge"));
        let flight = mouflon
            .reflexes
            .startle
            .iter()
            .max_by(|a, b| a.weight.total_cmp(&b.weight))
            .unwrap();
        assert_eq!(mouflon.behaviors[flight.to].name, "run");
        let look = |def: &PetDef| def.behaviors[def.behavior_index("look").unwrap()].contagious;
        assert!(look(&mouflon) > look(&model));
        // Le bélier d'Ouessant détale en bande, puis fait volte-face pour
        // regarder.
        let ouessant = bundled("ouessant");
        assert!(next(&ouessant, "run").iter().any(|n| n == "volte"));
        assert_eq!(next(&ouessant, "volte"), ["look"]);
        assert!(ouessant.behaviors[ouessant.behavior_index("run").unwrap()].contagious > 0.0);
    }

    #[test]
    fn an_angry_lainou_charges_head_down() {
        // Énervé, il fonce tête baissée : une animation qu'il ne joue pas autrement.
        let lainou = fixture("lainou");
        let (mut flock, world) = resting_flock(&lainou, 4, "repos");
        flock.set_mood(crate::Mood::Rowdy);
        let mut charged = false;
        for _ in 0..(240.0 / TICK) as usize {
            flock.advance(TICK, &world, &Input::default());
            charged |= flock.pets().iter().any(|p| p.behavior_name() == "fonce");
        }
        assert!(charged);
    }

    #[test]
    fn neighbours_of_the_same_family_follow_a_contagious_behavior() {
        // Un mouton part en marchant : ses voisins le suivent souvent, du
        // même côté, un peu plus tard ; ceux qui viennent d'imiter attendent.
        // (Le Bleu du Maine, peu grégaire, entraîne moins : on lui donne la
        // contagion d'un mouton ordinaire, une chance sur quatre.)
        let mut sheep = (*bundled("bleu-du-maine")).clone();
        let walk = sheep.behavior_index("walk").unwrap();
        sheep.behaviors[walk].contagious = 0.25;
        let sheep = Arc::new(sheep);
        let mut followed = 0;
        for round in 0..20u64 {
            let world = world();
            let mut flock = Flock::new();
            let floor = *world.floor_under(300.0).unwrap();
            for i in 0..4u32 {
                let id = flock.spawn(
                    Arc::clone(&sheep),
                    round * 97 + u64::from(i) * 31,
                    &world,
                    None,
                );
                let pet = flock.get_mut(id).unwrap();
                pet.pos = Vec2::new(500.0 + 70.0 * i as f32, floor.y);
                pet.prev_pos = pet.pos;
                pet.set_ground(&floor);
                pet.facing = -1.0;
                assert!(pet.force("stand"));
                pet.behavior_length = 60.0;
            }
            let leader = flock.pets()[0].id();
            {
                let pet = flock.get_mut(leader).unwrap();
                pet.facing = 1.0;
                assert!(pet.force("walk"));
            }
            for _ in 0..(6.0 / TICK) as usize {
                flock.advance(TICK, &world, &Input::default());
            }
            followed += flock
                .pets()
                .iter()
                .filter(|p| p.id() != leader && p.behavior_name() == "walk" && p.facing > 0.0)
                .count();
            for p in flock.pets() {
                if p.id() != leader && p.behavior_name() == "walk" {
                    assert!(p.echo_cooldown > 0.0, "il vient d'imiter : il attend");
                }
            }
        }
        // Trois voisins, une chance sur quatre chacun, sur vingt essais.
        assert!((5..=30).contains(&followed), "{followed} suiveurs");
    }

    #[test]
    fn image_pets_follow_at_the_end_of_what_they_play() {
        // Un Lainou part : ses voisins le suivent à la fin de leur séquence
        // en cours ; un animal desktopPet, dont les scènes s'enchaînent
        // d'elles-mêmes, ne suit pas.
        let lainou = fixture("lainou");
        let mut followed = 0;
        for round in 0..12u64 {
            let (mut flock, world) = resting_flock(&lainou, 4, "repos");
            for pet in flock.pets_mut() {
                pet.rng = Rng::new(round * 7919 + u64::from(pet.id()) * 31 + 1);
                pet.facing = -1.0;
            }
            let leader = flock.pets()[0].id();
            {
                let pet = flock.get_mut(leader).unwrap();
                pet.facing = 1.0;
                assert!(pet.force("marche"));
            }
            let mut seen = std::collections::BTreeSet::new();
            for _ in 0..(9.0 / TICK) as usize {
                flock.advance(TICK, &world, &Input::default());
                // (Ceux qui partent d'eux-mêmes en entraînent d'autres aussi :
                // le sens est vérifié avec le Bleu du Maine.)
                for p in flock.pets() {
                    if p.id() != leader && p.behavior_name() == "marche" && p.echo_cooldown > 0.0 {
                        seen.insert(p.id());
                    }
                }
            }
            followed += seen.len();
        }
        assert!(followed >= 3, "{followed} suiveurs");
        let pelote = fixture("pelote");
        let mut pet = Pet::spawn(0, pelote, 1, &world(), Some(300.0));
        pet.echo = Some(Echo {
            behavior: 0,
            delay: 0.0,
            facing: 1.0,
            patience: ECHO_PATIENCE,
        });
        assert!(!pet.take_echo());
    }

    #[test]
    fn a_grazing_sheep_steps_to_the_next_tuft_without_lifting_its_head() {
        let sheep = bundled("bleu-du-maine");
        let world = world();
        let mut flock = Flock::new();
        let floor = *world.floor_under(300.0).unwrap();
        let id = flock.spawn(Arc::clone(&sheep), 5, &world, None);
        {
            let pet = flock.get_mut(id).unwrap();
            pet.pos = Vec2::new(600.0, floor.y);
            pet.prev_pos = pet.pos;
            pet.set_ground(&floor);
            pet.facing = 1.0;
            assert!(pet.force("graze"));
            pet.behavior_length = 60.0;
        }
        let start = flock.pets()[0].position().x;
        let mut stepping = 0.0f32;
        for _ in 0..(20.0 / TICK) as usize {
            flock.advance(TICK, &world, &Input::default());
            let pet = &flock.pets()[0];
            assert_eq!(pet.behavior_name(), "graze");
            if pet.vel.x.abs() > 1.0 {
                // Il avance pattes comprises, la tête toujours au sol.
                stepping = stepping.max(pet.anim.walk);
                assert!(pet.anim.graze > 0.9);
            }
        }
        let moved = flock.pets()[0].position().x - start;
        assert!(moved > 10.0 && moved < 60.0, "{moved}");
        assert!(stepping > 0.6, "{stepping}");
    }

    #[test]
    fn a_square_sheep_moves_off_with_a_foreleg_alone() {
        // Debout d'aplomb, il repart (au pas, en broutant, au galop pour
        // fuir) : un antérieur se lève le premier, seul, comme le cheval qui
        // part au trot (Tans, Nauwelaerts et Clayton 2009).
        let world = world();
        for (behavior, phase) in [("walk", 0.0), ("walk", 2.0), ("run", 4.5), ("trot", 1.0)] {
            let mut pet = standing(bundled("bleu-du-maine"), &world, 300.0, 1.0, "stand");
            pet.behavior_length = 60.0;
            pet.anim.gait_phase = phase;
            run(&mut pet, &world, 1.0);
            assert!(legs::square(&pet.anim.legs));
            assert!(pet.force(behavior));
            pet.behavior_length = 60.0;
            let mut waited = 0.0;
            while !legs::stepping(&pet.anim.legs) {
                run(&mut pet, &world, TICK);
                waited += TICK;
                assert!(waited < 0.5, "{behavior}");
            }
            // Le sabot quitte le sol au pas suivant de la simulation.
            run(&mut pet, &world, TICK);
            let up: Vec<usize> = (0..4).filter(|&k| pet.anim.legs[k].raise > 0.0).collect();
            assert_eq!(up, [0], "{behavior} {phase}");
        }
    }

    #[test]
    fn a_walking_sheep_that_stops_to_paw_puts_its_hoof_down_first() {
        // Au pas, une patte au moins est toujours en l'air : il s'arrête pour
        // gratter le sol, finit son pas, et seulement alors lève un sabot.
        let world = world();
        let mut pet = standing(bundled("bleu-du-maine"), &world, 300.0, 1.0, "walk");
        pet.behavior_length = 60.0;
        run(&mut pet, &world, 1.5);
        assert!(legs::stepping(&pet.anim.legs));
        assert!(pet.force("paw"));
        pet.behavior_length = 60.0;
        let mut landing = 0.0;
        while legs::stepping(&pet.anim.legs) {
            assert_eq!(pet.anim.gesture_amount, 0.0, "{landing}");
            run(&mut pet, &world, TICK);
            landing += TICK;
            assert!(landing < 0.5, "elle se pose");
        }
        run(&mut pet, &world, 0.3);
        assert!(pet.anim.gesture_amount > 0.9, "{}", pet.anim.gesture_amount);
        assert!(!legs::stepping(&pet.anim.legs));
    }

    #[test]
    fn a_ruminating_sheep_chews_bolus_after_bolus() {
        let sheep = bundled("bleu-du-maine");
        let world = world();
        let mut pet = Pet::spawn(0, sheep, 3, &world, Some(300.0));
        run(&mut pet, &world, 3.0);
        assert!(pet.force("ruminate"));
        pet.behavior_length = 120.0;
        let (mut chewing, mut still, mut pause) = (0.0f32, 0.0f32, 0.0f32);
        for _ in 0..(60.0 / TICK) as usize {
            pet.step(TICK, &world, &Input::default());
            if pet.anim.bolus_left > 0.0 {
                chewing += TICK;
                pause = 0.0;
            } else {
                still += TICK;
                pause += TICK;
                if pause > 0.3 {
                    assert!(pet.anim.jaw < 0.05, "la bouche se referme pendant la pause");
                }
            }
        }
        assert!(chewing > 40.0 && still > 4.0, "{chewing} {still}");
    }

    #[test]
    fn a_flick_of_the_tail_runs_down_to_its_tip() {
        // Un coup de queue : le milieu suit la base avec un temps de retard,
        // le bout suit le milieu, plus tard encore (elle ondule).
        let world = world();
        let mut pet = standing(bundled("bleu-du-maine"), &world, 600.0, 1.0, "stand");
        pet.behavior_length = 60.0;
        run(&mut pet, &world, 1.0);
        pet.anim.tail.kick(8.0);
        let (mut peaks, mut best) = ([0.0f32; 3], [f32::NEG_INFINITY; 3]);
        let mut t = 0.0;
        for _ in 0..(1.0 / TICK) as usize {
            pet.step(TICK, &world, &Input::default());
            t += TICK;
            let a = &pet.anim;
            for (k, value) in [a.tail.value, a.tail_mid.value, a.tail_tip.value]
                .into_iter()
                .enumerate()
            {
                if value > best[k] {
                    (best[k], peaks[k]) = (value, t);
                }
            }
        }
        assert!(peaks[0] < peaks[1] && peaks[1] < peaks[2], "{peaks:?}");
        assert!(best[2] > 0.2, "le bout aussi a bougé : {best:?}");
        // Le dessin reçoit les angles de chaque morceau par rapport au
        // précédent.
        let view = pet.view(1.0).pose;
        let a = &pet.anim;
        assert!((view.tail + view.tail_mid - a.tail_mid.value).abs() < 1e-5);
        assert!((view.tail_mid + view.tail_tip - (a.tail_tip.value - view.tail)).abs() < 1e-5);
    }

    #[test]
    fn the_fleece_trails_behind_a_running_sheep_and_hangs_still_at_rest() {
        let world = world();
        let mut pet = standing(bundled("bleu-du-maine"), &world, 600.0, 1.0, "stand");
        pet.behavior_length = 60.0;
        run(&mut pet, &world, 2.0);
        let rest = pet.view(1.0).pose.fleece;
        assert!(rest.abs() < 0.02, "au repos : {rest}");
        // Au galop, les mèches traînent (leur bout part vers l'arrière) ;
        // celles du bout suivent les premières avec un temps de retard.
        assert!(pet.force("run"));
        let mut trailing = 0.0f32;
        let mut lag = 0.0f32;
        for _ in 0..(0.8 / TICK) as usize {
            pet.step(TICK, &world, &Input::default());
            if pet.behavior_name() != "run" {
                pet.force("run");
            }
            let pose = pet.view(1.0).pose;
            trailing = trailing.max(pose.fleece);
            lag = lag.max(pose.fleece_tip.abs());
        }
        assert!(trailing > 0.12, "les mèches traînent : {trailing}");
        assert!(lag > 0.005, "le bout suit avec retard : {lag}");
    }

    #[test]
    fn long_locks_swing_slower_than_short_ones() {
        // Un coup de vent, puis plus rien : les mèches reviennent en
        // oscillant. Celles d'une race à longue laine (quatre fois plus
        // longues) mettent deux fois plus de temps à repasser par l'aplomb,
        // comme un pendule ; et celles de son agneau, plus courtes que les
        // siennes, moins.
        let world = world();
        let swing_back = |def: Arc<PetDef>| {
            let mut pet = standing(def, &world, 600.0, 1.0, "stand");
            pet.behavior_length = 60.0;
            run(&mut pet, &world, 1.0);
            for _ in 0..(0.6 / TICK) as usize {
                pet.wind = -crate::surprise::GUST_PEAK;
                pet.step(TICK, &world, &Input::default());
            }
            assert!(pet.view(1.0).pose.fleece > 0.1);
            let mut time = 0.0;
            while pet.view(1.0).pose.fleece > 0.0 {
                pet.wind = 0.0;
                pet.step(TICK, &world, &Input::default());
                time += TICK;
                assert!(time < 5.0, "les mèches ne reviennent pas");
            }
            time
        };
        let with = |fleece: f32| {
            let mut def = (*bundled("bleu-du-maine")).clone();
            def.physics.fleece = fleece;
            Arc::new(def)
        };
        let (short, long) = (swing_back(with(9.0)), swing_back(with(36.0)));
        assert!(
            (long / short - 2.0).abs() < 0.25,
            "courtes : {short} s, longues : {long} s"
        );
        // Sans longueur donnée : le rythme de base, celui de 9 cm.
        assert!((swing_back(with(0.0)) - short).abs() < 1e-6);
        // L'agneau d'une race à longue laine : ses mèches, à sa taille.
        let racka = bundled("racka");
        let lamb = racka.variant(crate::def::Variant::Lamb).expect("l'agneau");
        assert!(racka.physics.fleece > 20.0, "{}", racka.physics.fleece);
        let ratio = lamb.physics.fleece / racka.physics.fleece;
        assert!((0.3..0.6).contains(&ratio), "{ratio}");
        assert!(lamb.physics.fleece_pace() > racka.physics.fleece_pace());
    }

    #[test]
    fn the_wind_blows_the_fleece_the_way_it_goes() {
        // Un mouton tourné vers la droite : le vent qui souffle vers la droite
        // (dans son dos) porte le bout des mèches vers l'avant (angle
        // négatif) ; le vent dans le museau, vers l'arrière.
        let world = world();
        let peak = crate::surprise::GUST_PEAK;
        for (wind, forward) in [(peak, true), (-peak, false)] {
            let mut pet = standing(bundled("bleu-du-maine"), &world, 600.0, 1.0, "stand");
            pet.behavior_length = 60.0;
            run(&mut pet, &world, 1.0);
            let mut extreme = 0.0f32;
            for _ in 0..(1.0 / TICK) as usize {
                pet.wind = wind;
                pet.step(TICK, &world, &Input::default());
                let fleece = pet.view(1.0).pose.fleece;
                if fleece.abs() > extreme.abs() {
                    extreme = fleece;
                }
            }
            if forward {
                assert!(extreme < -0.15, "vent dans le dos : {extreme}");
            } else {
                assert!(extreme > 0.15, "vent dans le museau : {extreme}");
            }
        }
    }

    #[test]
    fn a_flehmen_half_opens_the_mouth_under_the_curled_lip() {
        let world = world();
        let mut def = (*bundled("bleu-du-maine")).clone();
        def.sex_ratio = crate::SexRatio::MALE;
        let mut pet = standing(Arc::new(def.clone()), &world, 600.0, 1.0, "sniff");
        def.sex_ratio = crate::SexRatio::FEMALE;
        let ewe = standing(Arc::new(def), &world, 660.0, -1.0, "stand");
        pet.remember_sniff(&ewe);
        run(&mut pet, &world, 0.5);
        assert!(pet.force("flehmen"));
        run(&mut pet, &world, 2.0);
        let pose = pet.view(1.0).pose;
        assert_eq!(pose.gesture, Some(Gesture::Flehmen));
        assert!(pose.gesture_amount > 0.9);
        assert!(pose.jaw > 0.08 && pose.jaw < 0.25, "{}", pose.jaw);
        assert_eq!(Gesture::from_name("flehmen"), Some(Gesture::Flehmen));
    }

    #[test]
    fn a_drawn_sheep_crouches_before_it_leaps() {
        // L'anticipation : un mouton dessiné se ramasse, puis bondit.
        let world = world();
        let mut pet = standing(bundled("bleu-du-maine"), &world, 600.0, 1.0, "stand");
        run(&mut pet, &world, 0.5);
        assert!(pet.force("hop"));
        let (mut crouched, mut squash) = (0.0f32, 0.0f32);
        while matches!(pet.support, Support::Ground { .. }) {
            pet.step(TICK, &world, &Input::default());
            crouched += TICK;
            squash = squash.max(pet.view(1.0).pose.squash);
            assert!(crouched < 0.5, "il n'a jamais sauté");
        }
        assert!((0.08..0.2).contains(&crouched), "{crouched}");
        assert!(squash > 0.04, "{squash}");
        assert!(pet.vel.y < -300.0, "{}", pet.vel.y);
    }

    #[test]
    fn a_ram_backs_up_facing_forward_and_stops_at_the_end_of_a_ledge() {
        let mut world = world();
        world.set_windows(
            &[WindowRect {
                id: 5,
                frame: Rect::new(200.0, 400.0, 300.0, 300.0),
            }],
            LedgeOptions::default(),
        );
        let ledge = *world
            .surfaces()
            .iter()
            .find(|s| s.kind == crate::world::SurfaceKind::Window)
            .unwrap();
        // Au bout du recul, il se cabre ou charge : la brebis docile du Bleu
        // du Maine renonce le plus souvent, on lui ôte ce choix.
        let mut ram = (*bundled("bleu-du-maine")).clone();
        let back_up = ram.behavior_index("back_up").unwrap();
        let stand = ram.behavior_index("stand").unwrap();
        ram.behaviors[back_up].next.retain(|t| t.to != stand);
        let ram = Arc::new(ram);
        // Au sol : il recule, toujours tourné vers où il va charger.
        let mut pet = standing(Arc::clone(&ram), &world, 900.0, 1.0, "back_up");
        pet.behavior_length = 60.0;
        run(&mut pet, &world, 1.0);
        assert!(pet.position().x < 880.0, "{}", pet.position().x);
        assert_eq!(pet.facing, 1.0);
        assert_eq!(pet.behavior_name(), "back_up");
        // Sur un rebord : il s'arrête au bout plutôt que de tomber à reculons.
        let mut pet = standing(ram, &world, 260.0, 1.0, "back_up");
        pet.set_ground(&ledge);
        pet.pos.y = ledge.y;
        pet.prev_pos = pet.pos;
        pet.behavior_length = 60.0;
        run(&mut pet, &world, 4.0);
        assert_eq!(pet.support, Support::Ground { surface: ledge.id });
        assert!(pet.position().x > ledge.x0, "{}", pet.position().x);
        assert_eq!(pet.facing, 1.0);
        // Au bout du recul, il se cabre souvent, puis charge.
        pet.behavior_length = 0.0;
        run(&mut pet, &world, 0.1);
        if pet.behavior_name() == "rear" {
            assert!((pet.position().x - ledge.x0).abs() < 60.0);
            run(&mut pet, &world, 1.2);
        }
        assert_eq!(pet.behavior_name(), "charge");
    }

    #[test]
    fn playing_lambs_twist_and_spin_in_the_air() {
        let world = world();
        // Saut tordu : le corps se tord en l'air, sans les pattes raides.
        let mut pet = standing(bundled_lamb("bleu-du-maine"), &world, 600.0, 1.0, "gambol");
        let (mut twist, mut stiff) = (0.0f32, 0.0f32);
        for _ in 0..(0.6 / TICK) as usize {
            pet.step(TICK, &world, &Input::default());
            twist = twist.max(pet.view(1.0).pose.twist);
            stiff = stiff.max(pet.view(1.0).pose.stiff);
        }
        assert!(twist > 0.8, "{twist}");
        assert!(stiff < 0.1, "{stiff}");
        // Un saut d'agneau ordinaire garde les pattes raides, sans se tordre.
        let mut pet = standing(bundled_lamb("bleu-du-maine"), &world, 600.0, 1.0, "pronk");
        let (mut twist, mut stiff) = (0.0f32, 0.0f32);
        for _ in 0..(0.6 / TICK) as usize {
            pet.step(TICK, &world, &Input::default());
            twist = twist.max(pet.view(1.0).pose.twist);
            stiff = stiff.max(pet.view(1.0).pose.stiff);
        }
        assert!(twist < 0.01 && stiff > 0.8, "{twist} {stiff}");
        // Demi-tour en l'air : il quitte le sol tourné d'un côté et retombe
        // tourné de l'autre, sur place.
        let mut pet = standing(bundled_lamb("bleu-du-maine"), &world, 600.0, 1.0, "pivot");
        run(&mut pet, &world, 0.1);
        assert_eq!(pet.facing, 1.0, "il se ramasse d'abord");
        run(&mut pet, &world, 0.15);
        assert_eq!(pet.support, Support::Air);
        assert_eq!(pet.facing, -1.0);
        assert!((pet.position().x - 600.0).abs() < 2.0);
    }

    #[test]
    fn two_playing_lambs_back_off_then_butt_heads() {
        // Ils reculent ensemble, s'élancent et se cognent le front : le choc
        // les arrête là où leurs dessins se touchent, et les tasse tous deux.
        let world = world();
        let mut flock = Flock::new();
        let lamb = bundled_lamb("bleu-du-maine");
        let a = flock.spawn(Arc::clone(&lamb), 1, &world, None);
        let b = flock.spawn(Arc::clone(&lamb), 2, &world, None);
        put(&mut flock, &world, a, 600.0, 1.0, "play_back");
        put(&mut flock, &world, b, 660.0, -1.0, "play_back");
        let mut apart = 0.0f32;
        let mut butt = None;
        let mut squash = [0.0f32; 2];
        for _ in 0..(3.0 / TICK) as usize {
            advance(&mut flock, &world, TICK);
            let (pa, pb) = (find(&flock, a).unwrap(), find(&flock, b).unwrap());
            apart = apart.max(pb.position().x - pa.position().x);
            if butt.is_none() && pa.behavior_name() == "play_butt" {
                assert_eq!(pb.behavior_name(), "play_butt");
                butt = Some(pb.position().x - pa.position().x);
            }
            if butt.is_some() {
                squash[0] = squash[0].max(pa.view(1.0).pose.squash);
                squash[1] = squash[1].max(pb.view(1.0).pose.squash);
            }
        }
        let butt = butt.expect("ils ne se sont jamais cognés");
        // Des agneaux dans leur dessin d'agneau : ils reculent à leur taille.
        assert!(apart > 90.0, "ils ont reculé : {apart}");
        let width = find(&flock, a).unwrap().def().physics.width;
        assert!(
            butt > 0.5 * width && butt < 1.2 * width,
            "front contre front : {butt} pour {width} de large"
        );
        assert!(squash[0] > 0.04 && squash[1] > 0.03, "{squash:?}");
    }

    #[test]
    fn a_lively_lamb_invites_a_neighbour_to_play() {
        // Deux agneaux actifs, à deux longueurs : l'un invite l'autre, tête
        // basse ; l'autre accepte le plus souvent, et ils s'élancent l'un
        // vers l'autre jusqu'au coup de front.
        let lamb = bundled_lamb("bleu-du-maine");
        // À deux longueurs d'agneau : ils portent leur dessin d'agneau.
        let far = 500.0
            + 2.0
                * lamb
                    .variant(crate::Variant::Lamb)
                    .map_or(lamb.physics.width, |drawn| drawn.physics.width);
        let (mut invited, mut butted) = (0, 0);
        for round in 0..10u64 {
            let world = world();
            let mut flock = Flock::new();
            flock.set_mood(Mood::Lively);
            let a = flock.spawn(Arc::clone(&lamb), round * 11 + 1, &world, None);
            let b = flock.spawn(Arc::clone(&lamb), round * 11 + 2, &world, None);
            put(&mut flock, &world, a, 500.0, 1.0, "stand");
            put(&mut flock, &world, b, far, 1.0, "graze");
            for pet in flock.pets_mut() {
                pet.behavior_length = 600.0;
                pet.invite_in = 0.5;
            }
            let (mut seen_invite, mut seen_butt) = (false, false);
            for _ in 0..(4.0 / TICK) as usize {
                advance(&mut flock, &world, TICK);
                for pet in flock.pets() {
                    seen_invite |= pet.behavior_name() == "play_invite";
                    if pet.behavior_name() == "play_butt" {
                        seen_butt = true;
                        let other = flock.pets().iter().find(|p| p.id() != pet.id()).unwrap();
                        // Front contre front : tournés l'un vers l'autre.
                        assert!(pet.facing * (other.pos.x - pet.pos.x) > 0.0);
                    }
                }
            }
            invited += usize::from(seen_invite);
            butted += usize::from(seen_butt);
        }
        assert_eq!(invited, 10);
        assert!(butted >= 5, "{butted} coups de front sur 10 invitations");
        // Un calme n'invite personne (des adultes : un agneau est toujours
        // un cran plus vif que le troupeau).
        let world = world();
        let mut flock = Flock::new();
        flock.set_mood(Mood::Calm);
        let adult = bundled("bleu-du-maine");
        let far = 500.0 + 2.0 * adult.physics.width;
        let a = flock.spawn(Arc::clone(&adult), 1, &world, None);
        let b = flock.spawn(Arc::clone(&adult), 2, &world, None);
        put(&mut flock, &world, a, 500.0, 1.0, "stand");
        put(&mut flock, &world, b, far, 1.0, "stand");
        for pet in flock.pets_mut() {
            pet.behavior_length = 600.0;
            pet.invite_in = 0.5;
        }
        advance(&mut flock, &world, 3.0);
        assert!(flock.pets().iter().all(|p| p.behavior_name() == "stand"));
    }

    #[test]
    fn a_scattered_flock_regroups_in_the_same_order() {
        // Celui qui vient de loin croise parfois en route ceux qui sont déjà
        // arrivés et repartis (le Lainou, plus lent, plus souvent) : il
        // s'arrête alors un peu avant sa place.
        for (name, mover, seconds, slack) in [
            ("bleu-du-maine", "trot", 20.0, 120.0),
            ("lainou", "marche", 40.0, 120.0),
        ] {
            let def = if name == "lainou" {
                fixture(name)
            } else {
                bundled(name)
            };
            let world = world();
            let mut flock = Flock::new();
            let xs = [150.0, 420.0, 900.0, 1300.0];
            let mut ids = Vec::new();
            for (i, &x) in xs.iter().enumerate() {
                let id = flock.spawn(Arc::clone(&def), 11 + i as u64, &world, None);
                let rest = if name == "lainou" { "repos" } else { "stand" };
                put(&mut flock, &world, id, x, 1.0, rest);
                flock.get_mut(id).unwrap().behavior_length = 60.0;
                ids.push(id);
            }
            assert!(flock.regroup());
            let targets: Vec<f32> = ids
                .iter()
                .map(|&id| find(&flock, id).unwrap().regroup.unwrap().0)
                .collect();
            assert!(targets.windows(2).all(|w| w[0] < w[1]), "{targets:?}");
            let mut arrived: Vec<Option<f32>> = vec![None; ids.len()];
            let mut moved = false;
            // Un Lainou marche à petits pas : il lui faut plus longtemps.
            for _ in 0..(seconds / TICK) as usize {
                let before: Vec<bool> = ids
                    .iter()
                    .map(|&id| find(&flock, id).unwrap().goal.is_some())
                    .collect();
                advance(&mut flock, &world, TICK);
                for (k, &id) in ids.iter().enumerate() {
                    let pet = find(&flock, id).unwrap();
                    moved |= pet.behavior_name() == mover;
                    if before[k] && pet.goal.is_none() && arrived[k].is_none() {
                        arrived[k] = Some(pet.position().x);
                    }
                }
            }
            assert!(moved, "{name} : personne n'a pris « {mover} »");
            let arrived: Vec<f32> = arrived
                .iter()
                .map(|x| x.unwrap_or_else(|| panic!("{name} : pas tous arrivés : {arrived:?}")))
                .collect();
            let span = arrived[3] - arrived[0];
            assert!(span < 1150.0 * 0.3, "{name} : {arrived:?}");
            assert!(
                arrived.windows(2).all(|w| w[0] < w[1]),
                "{name} : {arrived:?}"
            );
            for (x, target) in arrived.iter().zip(&targets) {
                assert!(
                    (x - target).abs() < slack,
                    "{name} : {arrived:?} {targets:?}"
                );
            }
        }
    }

    #[test]
    fn a_sleeping_sheep_may_lay_its_head_on_the_ground_and_twitch() {
        let world = world();
        let (mut low, mut high, mut twitched) = (0, 0, false);
        for seed in 0..8u64 {
            let mut pet = standing(bundled("bleu-du-maine"), &world, 600.0, 1.0, "sit");
            pet.rng = Rng::new(seed * 7 + 3);
            run(&mut pet, &world, 2.0);
            assert!(pet.force("sleep"));
            pet.behavior_length = 600.0;
            let mut twitch = 0.0f32;
            for _ in 0..(60.0 / TICK) as usize {
                pet.step(TICK, &world, &Input::default());
                twitch = twitch.max(pet.view(1.0).pose.twitch);
            }
            let sleep_low = pet.view(1.0).pose.sleep_low;
            if sleep_low > 0.9 {
                low += 1;
            } else {
                assert!(sleep_low < 0.05, "{sleep_low}");
                high += 1;
            }
            twitched |= twitch > 0.5;
        }
        assert!(low >= 1 && high >= 1, "{low} {high}");
        assert!(twitched);
    }

    fn find(flock: &Flock, id: u32) -> Option<&Pet> {
        flock.pets().iter().find(|p| p.id() == id)
    }

    /// Un troupeau de Lainou au repos sur le sol, côte à côte.
    fn resting_flock(def: &Arc<PetDef>, count: u32, behavior: &str) -> (Flock, World) {
        let world = world();
        let mut flock = Flock::new();
        for i in 0..count {
            let id = flock.spawn(Arc::clone(def), u64::from(i) * 31 + 7, &world, None);
            let floor = *world.floor_under(300.0).unwrap();
            let pet = flock.get_mut(id).unwrap();
            pet.pos = Vec2::new(200.0 + 150.0 * i as f32, floor.y);
            pet.prev_pos = pet.pos;
            pet.set_ground(&floor);
            assert!(pet.force(behavior));
        }
        (flock, world)
    }

    #[test]
    fn a_rushing_cursor_startles_the_pets_it_brushes_past() {
        let lainou = fixture("lainou");
        let (mut flock, world) = resting_flock(&lainou, 8, "repos");
        let y = flock.pets()[0].position().y - 25.0;
        // Lent, ou loin : rien.
        let slow = |x: f32| Input {
            cursor: Some(Vec2::new(x, y)),
            ..Input::default()
        };
        for step in 0..200 {
            flock.advance(TICK, &world, &slow(100.0 + step as f32 * 2.0));
        }
        let far = |x: f32| Input {
            cursor: Some(Vec2::new(x, y - 400.0)),
            ..Input::default()
        };
        for step in 0..60 {
            flock.advance(TICK, &world, &far(step as f32 * 40.0));
        }
        assert!(
            flock.pets().iter().all(|p| p.behavior_name() != "surpris"),
            "personne ne sursaute"
        );
        // Un geste vif qui les frôle tous (4 800 pt/s) : la plupart sursautent.
        for step in 0..40 {
            flock.advance(
                TICK,
                &world,
                &Input {
                    cursor: Some(Vec2::new(100.0 + step as f32 * 40.0, y)),
                    ..Input::default()
                },
            );
        }
        let startled = flock
            .pets()
            .iter()
            .filter(|p| matches!(p.behavior_name(), "surpris" | "bele"))
            .count();
        assert!(startled >= 3, "{startled} sur 8");
        let events: Vec<PetEvent> = flock
            .pets_mut()
            .iter_mut()
            .flat_map(Pet::take_events)
            .collect();
        assert!(events.contains(&PetEvent::Startled));
    }

    #[test]
    fn pets_doze_off_when_the_user_is_away_and_wake_up_when_back() {
        for name in ["lainou", "pelote"] {
            let def = fixture(name);
            let ground = if name == "lainou" { "repos" } else { "walk" };
            let (mut flock, world) = resting_flock(&def, 3, ground);
            // En plein après-midi : la nuit, ils s'endorment d'eux-mêmes.
            let away = Input {
                idle: Some(AWAY_AFTER + 1.0),
                since_key: Some(AWAY_AFTER + 1.0),
                local_hour: 15.0,
                ..Input::default()
            };
            let mut asleep = false;
            for _ in 0..((NAP_SPREAD + 20.0) / TICK) as usize {
                flock.advance(TICK, &world, &away);
                asleep = flock.pets().iter().all(Pet::is_asleep);
                if asleep {
                    break;
                }
            }
            let names: Vec<&str> = flock.pets().iter().map(Pet::behavior_name).collect();
            assert!(asleep, "{name} : tous endormis ({names:?})");
            // Lainou dort tant que l'utilisateur n'est pas là (Pelote, lui,
            // se réveille parfois et se rendort).
            if name == "lainou" {
                for _ in 0..(30.0 / TICK) as usize {
                    flock.advance(TICK, &world, &away);
                    assert!(flock.pets().iter().all(Pet::is_asleep), "{name}");
                }
            }
            // Il revient : ils se réveillent en moins de deux secondes.
            let back = Input {
                idle: Some(0.1),
                since_key: Some(5.0),
                local_hour: 15.0,
                ..Input::default()
            };
            for _ in 0..(2.0 / TICK) as usize {
                flock.advance(TICK, &world, &back);
            }
            let names: Vec<&str> = flock.pets().iter().map(Pet::behavior_name).collect();
            assert!(
                flock.pets().iter().all(|p| !p.is_asleep()),
                "{name} : réveillés ({names:?})"
            );
            let events: Vec<PetEvent> = flock
                .pets_mut()
                .iter_mut()
                .flat_map(Pet::take_events)
                .collect();
            assert!(
                events.contains(&PetEvent::Napping) && events.contains(&PetEvent::Woken),
                "{name} : {:?}",
                events
                    .iter()
                    .filter(|e| !matches!(e, PetEvent::Sound { .. }))
                    .collect::<Vec<_>>()
            );
        }
    }

    #[test]
    fn a_gust_of_wind_carries_the_flying_and_bends_the_standing() {
        let lainou = fixture("lainou");
        let (mut flock, world) = resting_flock(&lainou, 2, "repos");
        // L'un tombe du haut de l'écran, l'autre reste posé.
        let x0 = 700.0;
        let flying = flock.spawn(Arc::clone(&lainou), 5, &world, Some(x0));
        flock.blow(1.0, 4.0);
        let mut leaves = 0;
        let mut leaned = false;
        for _ in 0..(1.5 / TICK) as usize {
            flock.advance(TICK, &world, &Input::default());
            leaves = leaves.max(
                flock
                    .pets()
                    .iter()
                    .flat_map(|p| p.particles.iter())
                    .filter(|p| p.kind == ParticleKind::Leaf)
                    .count(),
            );
            leaned |= flock.pets()[0].view(1.0).pose.roll < -0.02;
        }
        let drifted = flock.pets().iter().find(|p| p.id() == flying).unwrap();
        assert!(drifted.position().x > x0 + 40.0, "{:?}", drifted.position());
        assert!(leaves > 0, "des feuilles passent");
        assert!(leaned, "il se penche contre le vent");
        // Le vent retombe : plus rien.
        for _ in 0..(4.0 / TICK) as usize {
            flock.advance(TICK, &world, &Input::default());
        }
        assert!(flock.gust().is_none());
        assert_eq!(flock.pets()[0].view(1.0).pose.roll, 0.0);
    }

    #[test]
    fn a_saucer_borrows_a_pet_and_sets_it_down_elsewhere() {
        for realistic in [false, true] {
            let lainou = fixture("lainou");
            let (mut flock, world) = resting_flock(&lainou, 1, "dort");
            flock.set_realistic(realistic);
            let pet = flock.pets()[0].id();
            let start = flock.pets()[0].position();
            let saucer = flock.invite_saucer(&world, 3).unwrap();
            assert_ne!(saucer, pet);
            let mut lifted = false;
            let mut highest = f32::MAX;
            let mut seen_beam = false;
            for _ in 0..(16.0 / TICK) as usize {
                flock.advance(TICK, &world, &Input::default());
                let p = flock.pets().iter().find(|p| p.id() == pet).unwrap();
                lifted |= p.is_held();
                highest = highest.min(p.position().y);
                seen_beam |= flock.saucers().iter().any(|s| s.view().beam > 0.9);
            }
            assert!(lifted && seen_beam, "aspiré dans le faisceau");
            assert!(highest < start.y - 150.0, "emporté en l'air : {highest}");
            assert!(flock.saucers().is_empty(), "elle est repartie");
            let after = flock.pets().iter().find(|p| p.id() == pet).unwrap();
            assert!(
                matches!(after.support(), Support::Ground { .. }),
                "reposé : {:?}",
                after.support()
            );
            assert!(
                (after.position().x - start.x).abs() > 200.0,
                "ailleurs : {:?} → {:?}",
                start,
                after.position()
            );
        }
    }

    #[test]
    fn a_sheep_thrown_at_a_saucer_makes_it_smoke_then_flee_or_crash() {
        let sheep = bundled("bleu-du-maine");
        let (mut fled, mut crashed) = (0, 0);
        for seed in 0..8u64 {
            let world = world();
            let mut flock = Flock::new();
            let target = flock.spawn(Arc::clone(&sheep), 1, &world, None);
            put(&mut flock, &world, target, 400.0, 1.0, "sleep");
            flock.get_mut(target).unwrap().behavior_length = 600.0;
            flock.invite_saucer(&world, seed).unwrap();
            let thrower = flock.spawn(Arc::clone(&sheep), 2, &world, None);
            put(&mut flock, &world, thrower, 1100.0, -1.0, "stand");
            flock.get_mut(thrower).unwrap().behavior_length = 600.0;
            // Elle prend le dormeur dans son faisceau.
            while !flock.saucers()[0].holds() {
                advance(&mut flock, &world, TICK);
            }
            advance(&mut flock, &world, 0.5);
            // On lui lance l'autre mouton dessus, à toute vitesse.
            let at = flock.saucers()[0].view().pos;
            {
                let pet = flock.get_mut(thrower).unwrap();
                assert!(pet.force("fall"));
                pet.support = Support::Air;
                pet.ground_ref = None;
                pet.pos = at + Vec2::new(-160.0, 30.0);
                pet.prev_pos = pet.pos;
                pet.vel = Vec2::new(900.0, -60.0);
            }
            let mut hit = false;
            for _ in 0..(0.5 / TICK) as usize {
                advance(&mut flock, &world, TICK);
                let saucer = &flock.saucers()[0];
                if !saucer.can_be_hit() {
                    hit = true;
                    break;
                }
            }
            assert!(hit, "percutée");
            let saucer = &flock.saucers()[0];
            assert!(saucer.view().smoke > 0.9 && !saucer.holds());
            let pet = flock.pets().iter().find(|p| p.id() == thrower).unwrap();
            assert!(pet.vel.x < 0.0, "il rebondit : {:?}", pet.vel);
            let held = flock.pets().iter().find(|p| p.id() == target).unwrap();
            assert!(!held.is_held(), "elle a lâché le dormeur");
            // Puis elle s'enfuit, ou tombe, fume au sol et repart.
            let mut lowest = f32::MIN;
            for _ in 0..(12.0 / TICK) as usize {
                advance(&mut flock, &world, TICK);
                if let Some(saucer) = flock.saucers().first() {
                    lowest = lowest.max(saucer.view().pos.y);
                }
            }
            assert!(flock.saucers().is_empty(), "elle est partie");
            let floor = world.floor_under(400.0).unwrap().y;
            if lowest > floor - 60.0 {
                crashed += 1;
            } else {
                fled += 1;
            }
        }
        assert!(fled > 0 && crashed > 0, "{fled} fuites, {crashed} chutes");
    }

    #[test]
    fn a_pet_thrown_through_the_beam_does_not_hit_the_saucer() {
        let sheep = bundled("bleu-du-maine");
        let world = world();
        let mut flock = Flock::new();
        let target = flock.spawn(Arc::clone(&sheep), 1, &world, None);
        put(&mut flock, &world, target, 400.0, 1.0, "sleep");
        flock.get_mut(target).unwrap().behavior_length = 600.0;
        flock.invite_saucer(&world, 1).unwrap();
        let thrower = flock.spawn(Arc::clone(&sheep), 2, &world, None);
        put(&mut flock, &world, thrower, 1100.0, -1.0, "stand");
        while flock.saucers()[0].view().beam < 0.3 {
            advance(&mut flock, &world, TICK);
        }
        let at = flock.saucers()[0].view().pos;
        {
            let pet = flock.get_mut(thrower).unwrap();
            assert!(pet.force("fall"));
            pet.support = Support::Air;
            pet.ground_ref = None;
            // À travers le faisceau, bien sous la soucoupe.
            pet.pos = at + Vec2::new(-200.0, 150.0);
            pet.prev_pos = pet.pos;
            pet.vel = Vec2::new(900.0, -150.0);
        }
        for _ in 0..(0.5 / TICK) as usize {
            advance(&mut flock, &world, TICK);
            assert!(flock.saucers()[0].can_be_hit());
        }
    }

    #[test]
    fn a_saucer_gives_up_a_pet_taken_back_by_the_hand() {
        let lainou = fixture("lainou");
        let (mut flock, world) = resting_flock(&lainou, 1, "repos");
        let pet = flock.pets()[0].id();
        flock.invite_saucer(&world, 9).unwrap();
        // Dans le faisceau…
        for _ in 0..(5.0 / TICK) as usize {
            flock.advance(TICK, &world, &Input::default());
        }
        assert!(flock.pets()[0].is_held());
        // … repris par la main.
        let hand = flock.pets()[0].position() - Vec2::new(0.0, 30.0);
        flock.get_mut(pet).unwrap().grab(hand, &world);
        for _ in 0..(3.0 / TICK) as usize {
            flock.get_mut(pet).unwrap().drag_to(hand);
            flock.advance(TICK, &world, &Input::default());
        }
        assert_eq!(
            flock.pets()[0].hold_target(),
            Some(hand),
            "toujours en main"
        );
        assert!(flock.saucers().is_empty(), "la soucoupe a renoncé");
    }

    #[test]
    fn typing_and_the_time_of_day_are_conditions() {
        let world = world();
        let pet = Pet::spawn(0, fixture("lainou"), 1, &world, Some(300.0));
        let typing = Input {
            since_key: Some(1.0),
            ..Input::default()
        };
        assert!(pet.condition(Condition::Typing, &world, &typing));
        assert!(!pet.condition(Condition::Typing, &world, &Input::default()));
        let morning = Input {
            local_hour: 8.5,
            ..Input::default()
        };
        assert!(pet.condition(Condition::Morning, &world, &morning));
        assert!(!pet.condition(Condition::Evening, &world, &morning));
        let evening = Input {
            local_hour: 19.0,
            ..Input::default()
        };
        assert!(pet.condition(Condition::Evening, &world, &evening));
        assert!(!pet.condition(Condition::Away, &world, &evening));
    }

    #[test]
    fn lainou_lives_a_long_life() {
        // Sans collisions, les Lainou se rencontrent et se saluent.
        let (def, seconds_in) = long_life("lainou", 30.0, 40.0, false);
        // Tenu, lancé, heurté, sonné par une grosse chute ou en partance :
        // il y faut la main de l'utilisateur, ou une chute de très haut.
        // Dos au vent, il faut du vent ; foncer, un tempérament énervé ;
        // jouer à la course, un tempérament actif.
        let never: Vec<&str> = never_played(&def, &seconds_in)
            .into_iter()
            .filter(|n| {
                !matches!(
                    *n,
                    "tenu"
                        | "tombe"
                        | "au_revoir"
                        | "culbute"
                        | "atterrit_sonne"
                        | "etourdi"
                        | "dos_au_vent"
                        | "fonce"
                        | "joue"
                        | "joue_tour"
                )
            })
            .collect();
        assert!(never.len() <= 3, "jamais joués : {never:?}");
        assert!(
            seconds_in.contains_key("salue") || seconds_in.contains_key("fete"),
            "ils ne se sont jamais rencontrés"
        );
        // En mode collisions, ils se bousculent et ne restent pas coincés.
        let (_, seconds_in) = long_life("lainou", 10.0, 40.0, true);
        assert!(seconds_in.contains_key("culbute"));
    }

    #[test]
    fn lainou_and_pelote_live_long_lives_in_realistic_physics() {
        // Fenêtres qui bougent, qui apparaissent et disparaissent : jamais
        // hors de l'écran, jamais bloqués, et toujours de quoi vivre.
        for collisions in [false, true] {
            let (def, seconds_in) = long_life_with("lainou", 8.0, 40.0, collisions, true);
            // En mode collisions, ils culbutent au lieu de se saluer.
            let meetings = ["salue", "fete", "surpris"];
            let never: Vec<&str> = never_played(&def, &seconds_in)
                .into_iter()
                .filter(|n| {
                    !matches!(
                        *n,
                        "tenu"
                            | "tombe"
                            | "au_revoir"
                            | "culbute"
                            | "atterrit_sonne"
                            | "etourdi"
                            | "dos_au_vent"
                            | "fonce"
                    ) && !(collisions && meetings.contains(n))
                })
                .collect();
            assert!(never.len() <= 4, "jamais joués : {never:?}");
            let (_, seconds_in) = long_life_with("pelote", 8.0, 400.0, collisions, true);
            assert!(seconds_in.len() > 8, "{seconds_in:?}");
        }
    }

    #[test]
    fn pelote_lives_a_long_life() {
        // Animal desktopPet : il entre par les côtés de l'écran et y grimpe.
        let (def, seconds_in) = long_life("pelote", 30.0, 400.0, false);
        let never = never_played(&def, &seconds_in);
        eprintln!("jamais joués : {never:?}");
        // Les chutes des fenêtres sont hautes : « fall hard » plutôt que « fall soft ».
        for essential in [
            "walk",
            "run",
            "sleep1a",
            "fall",
            "fall hard",
            "vertical_walk_up",
        ] {
            assert!(
                seconds_in.contains_key(essential),
                "{essential} jamais joué"
            );
        }
        // En mode collisions, il culbute et se relève, sans y passer son temps.
        let (_, seconds_in) = long_life("pelote", 10.0, 400.0, true);
        let total: f32 = seconds_in.values().sum();
        let down = seconds_in.get("fall soft").copied().unwrap_or(0.0)
            + seconds_in.get("toss").copied().unwrap_or(0.0);
        assert!(down < total * 0.3, "{down} s à terre sur {total} s");
    }

    /// Animal qui salue ceux qu'il rencontre, se retourne devant les
    /// fenêtres, dit au revoir et sait jouer avec les autres.
    const SOCIAL: &str = r#"
        [pet]
        id = "s"
        name = "S"
        [appearance]
        rig = "a.svg"
        height = 60.0
        [reflexes]
        spawn = "fall"
        fall = "fall"
        land = "stand"
        thrown = "fall"
        edge = "turn"
        wall = "turn"
        meet = "greet"
        window_side = "turn"
        leave = "wave"
        together = "hop"
        [[behavior]]
        name = "stand"
        motion = "stand"
        duration = 0.5
        next = "walk"
        [[behavior]]
        name = "walk"
        motion = "walk"
        speed = 60.0
        duration = 30
        next = "stand"
        [[behavior]]
        name = "rest"
        motion = "sit"
        duration = 30
        next = "stand"
        [[behavior]]
        name = "turn"
        motion = "turn"
        duration = 0.2
        next = "walk"
        [[behavior]]
        name = "greet"
        motion = "stand"
        duration = 0.6
        next = "turn"
        [[behavior]]
        name = "wave"
        motion = "stand"
        duration = 0.5
        next = "stand"
        [[behavior]]
        name = "hop"
        motion = "sit"
        duration = 0.4
        next = "stand"
        [[behavior]]
        name = "fall"
        motion = "fall"
        duration = "forever"
    "#;

    fn social() -> Arc<PetDef> {
        Arc::new(parse_pet_toml(SOCIAL, |_: &str| Ok(b"<svg/>".to_vec())).unwrap())
    }

    /// Pose un animal du troupeau sur le sol, tourné et occupé comme voulu.
    fn put(flock: &mut Flock, world: &World, id: u32, x: f32, facing: f32, behavior: &str) {
        let floor = *world.floor_under(x).unwrap();
        let pet = flock.get_mut(id).unwrap();
        pet.pos = Vec2::new(x, floor.y);
        pet.prev_pos = pet.pos;
        pet.vel = Vec2::ZERO;
        pet.set_ground(&floor);
        pet.face(facing);
        assert!(pet.force(behavior));
    }

    fn advance(flock: &mut Flock, world: &World, seconds: f32) {
        let input = Input {
            cursor: None,
            local_hour: 14.0,
            ..Input::default()
        };
        for _ in 0..(seconds / TICK).round() as usize {
            flock.advance(TICK, world, &input);
        }
    }

    #[test]
    fn pets_meeting_face_to_face_greet_each_other_then_part() {
        let world = world();
        let mut flock = Flock::new();
        let a = flock.spawn(social(), 1, &world, None);
        let b = flock.spawn(social(), 2, &world, None);
        put(&mut flock, &world, a, 400.0, 1.0, "walk");
        put(&mut flock, &world, b, 560.0, -1.0, "walk");
        let reach = flock.pets()[0].def.physics.width;
        let mut met_at = None;
        for i in 0..(4.0 / TICK) as usize {
            flock.advance(TICK, &world, &Input::default());
            let (pa, pb) = (&flock.pets()[0], &flock.pets()[1]);
            assert!(
                pb.position().x - pa.position().x >= reach - 1.5,
                "ils se chevauchent"
            );
            if met_at.is_none() && pa.behavior_name() == "greet" {
                assert_eq!(pb.behavior_name(), "greet", "ils se saluent ensemble");
                met_at = Some(i);
            }
        }
        assert!(met_at.is_some(), "ils se sont rencontrés");
        let events: Vec<PetEvent> = flock.pets_mut()[0].take_events();
        assert!(events.contains(&PetEvent::Met { other: b }), "{events:?}");
        // Après le salut, demi-tour : chacun repart de son côté.
        let gap = flock.pets()[1].position().x - flock.pets()[0].position().x;
        advance(&mut flock, &world, 2.0);
        let later = flock.pets()[1].position().x - flock.pets()[0].position().x;
        assert!(later > gap + 50.0, "{gap} → {later}");
    }

    #[test]
    fn a_resting_pet_is_left_alone() {
        let world = world();
        let mut flock = Flock::new();
        let a = flock.spawn(social(), 1, &world, None);
        let b = flock.spawn(social(), 2, &world, None);
        put(&mut flock, &world, a, 400.0, 1.0, "walk");
        put(&mut flock, &world, b, 520.0, -1.0, "rest");
        let mut greeted = false;
        for _ in 0..(3.0 / TICK) as usize {
            flock.advance(TICK, &world, &Input::default());
            greeted |= flock.pets()[0].behavior_name() == "greet";
            assert_eq!(
                flock.pets()[1].behavior_name(),
                "rest",
                "il ne se dérange pas"
            );
        }
        assert!(greeted, "celui qui marche salue quand même");
    }

    #[test]
    fn pets_without_greetings_walk_past_each_other() {
        let world = world();
        let mut flock = Flock::new();
        let a = flock.spawn(def(), 1, &world, None);
        let b = flock.spawn(def(), 2, &world, None);
        put(&mut flock, &world, a, 400.0, 1.0, "walk");
        put(&mut flock, &world, b, 560.0, -1.0, "walk");
        advance(&mut flock, &world, 3.0);
        assert!(
            flock.pets()[0].position().x > flock.pets()[1].position().x,
            "ils se sont croisés"
        );
    }

    #[test]
    fn the_side_of_a_window_turns_a_pet_back_or_lets_it_pass() {
        let mut world = world();
        world.set_windows(
            &[WindowRect {
                id: 4,
                frame: Rect::new(500.0, 600.0, 400.0, 400.0),
            }],
            LedgeOptions::default(),
        );
        let mut flock = Flock::new();
        let a = flock.spawn(social(), 1, &world, None);
        put(&mut flock, &world, a, 380.0, 1.0, "walk");
        let half = flock.pets()[0].def.physics.width * 0.5;
        let mut furthest: f32 = 0.0;
        for _ in 0..(4.0 / TICK) as usize {
            flock.advance(TICK, &world, &Input::default());
            furthest = furthest.max(flock.pets()[0].position().x);
        }
        assert!(furthest <= 500.0 - half + 0.5, "{furthest}");
        assert_eq!(flock.pets()[0].facing, -1.0, "il a fait demi-tour");

        // Sans réaction prévue, il passe devant la fenêtre.
        let mut flock = Flock::new();
        let b = flock.spawn(def(), 1, &world, None);
        put(&mut flock, &world, b, 380.0, 1.0, "walk");
        advance(&mut flock, &world, 4.0);
        assert!(flock.pets()[0].position().x > 560.0);
    }

    #[test]
    fn a_pet_says_goodbye_then_fades_away() {
        let world = world();
        let mut flock = Flock::new();
        let a = flock.spawn(social(), 3, &world, None);
        put(&mut flock, &world, a, 600.0, 1.0, "stand");
        flock.leave_all(&world, &Input::default());
        let pet = &flock.pets()[0];
        assert!(pet.is_leaving());
        assert_eq!(pet.behavior_name(), "wave");
        advance(&mut flock, &world, 0.4);
        assert_eq!(flock.pets()[0].view(1.0).opacity, 1.0, "il salue d'abord");
        advance(&mut flock, &world, 0.5);
        let opacity = flock.pets()[0].view(1.0).opacity;
        assert!(opacity < 1.0 && opacity > 0.0, "puis s'efface : {opacity}");
        assert_eq!(
            flock.pets()[0].behavior_name(),
            "wave",
            "sur sa dernière pose"
        );
        advance(&mut flock, &world, 1.2);
        assert!(flock.pets().is_empty(), "il est parti");
    }

    #[test]
    fn one_pet_of_a_species_says_goodbye_the_latest_first() {
        let world = world();
        let mut flock = Flock::new();
        let first = flock.spawn(def(), 1, &world, None);
        let other = flock.spawn(social(), 2, &world, None);
        let latest = flock.spawn(def(), 3, &world, None);
        put(&mut flock, &world, first, 300.0, 1.0, "stand");
        put(&mut flock, &world, other, 600.0, 1.0, "stand");
        put(&mut flock, &world, latest, 900.0, 1.0, "stand");
        let input = Input::default();
        assert_eq!(flock.leave_one("t", &world, &input), Some(latest));
        assert_eq!(flock.leave_one("t", &world, &input), Some(first));
        assert_eq!(flock.leave_one("t", &world, &input), None, "plus aucun");
        assert_eq!(flock.leave_one("inconnu", &world, &input), None);
        let still = flock.pets().iter().find(|p| p.id() == other).unwrap();
        assert!(!still.is_leaving(), "l'autre espèce reste");
        advance(&mut flock, &world, 1.5);
        let ids: Vec<u32> = flock.pets().iter().map(Pet::id).collect();
        assert_eq!(ids, vec![other]);
    }

    #[test]
    fn the_pet_chosen_with_a_right_click_says_goodbye_alone() {
        let world = world();
        let mut flock = Flock::new();
        let first = flock.spawn(def(), 1, &world, None);
        let latest = flock.spawn(def(), 2, &world, None);
        put(&mut flock, &world, first, 300.0, 1.0, "stand");
        put(&mut flock, &world, latest, 900.0, 1.0, "stand");
        let input = Input::default();
        let generation = flock.generation();
        // Le premier, pas le dernier arrivé de son espèce.
        assert!(flock.leave(first, &world, &input));
        assert!(flock.generation() > generation, "le menu le saura");
        assert!(!flock.leave(first, &world, &input), "il part déjà");
        assert!(!flock.leave(99, &world, &input), "inconnu");
        advance(&mut flock, &world, 1.5);
        let ids: Vec<u32> = flock.pets().iter().map(Pet::id).collect();
        assert_eq!(ids, vec![latest]);
    }

    #[test]
    fn the_pet_whose_menu_is_open_waits_while_the_others_walk() {
        let world = world();
        let mut flock = Flock::new();
        let waiting = flock.spawn(def(), 1, &world, None);
        let walker = flock.spawn(def(), 2, &world, None);
        put(&mut flock, &world, waiting, 300.0, 1.0, "walk");
        put(&mut flock, &world, walker, 900.0, 1.0, "walk");
        advance(&mut flock, &world, 0.1);
        flock.hold_still(Some(waiting));
        assert_eq!(flock.still(), Some(waiting));
        let x = |flock: &Flock, id: u32| {
            flock
                .pets()
                .iter()
                .find(|p| p.id() == id)
                .unwrap()
                .view(0.5)
                .ground
                .x
        };
        let (before, other) = (x(&flock, waiting), x(&flock, walker));
        advance(&mut flock, &world, 0.5);
        assert_eq!(x(&flock, waiting), before, "il attend, sans trembler");
        assert!(x(&flock, walker) > other + 10.0, "l'autre marche");
        flock.hold_still(None);
        advance(&mut flock, &world, 0.5);
        assert!(x(&flock, waiting) > before + 10.0, "il repart");
    }

    #[test]
    fn a_pet_without_farewell_fades_at_once_even_when_held() {
        let world = world();
        let mut flock = Flock::new();
        let a = flock.spawn(def(), 3, &world, None);
        let b = flock.spawn(def(), 4, &world, None);
        put(&mut flock, &world, a, 600.0, 1.0, "walk");
        put(&mut flock, &world, b, 900.0, 1.0, "stand");
        let held = flock.get_mut(b).unwrap();
        held.grab(Vec2::new(900.0, 790.0), &world);
        flock.leave_all(&world, &Input::default());
        assert!(flock.pets().iter().all(|p| !p.is_held() && p.is_leaving()));
        advance(&mut flock, &world, 0.3);
        assert!(flock.pets().iter().all(|p| p.view(1.0).opacity < 1.0));
        // Il ne s'attrape plus pendant qu'il s'en va.
        let pet = flock.get_mut(a).unwrap();
        pet.grab(Vec2::new(600.0, 800.0), &world);
        assert!(!pet.is_held());
        advance(&mut flock, &world, 1.0);
        assert!(flock.pets().is_empty());
    }

    #[test]
    fn thrown_upward_it_slips_behind_the_menu_bar_then_falls_back() {
        let world = world();
        let mut pet = Pet::spawn(0, def(), 6, &world, Some(600.0));
        run(&mut pet, &world, 2.0);
        let h = pet.def.physics.height;
        pet.support = Support::Air;
        pet.ground_ref = None;
        pet.vel = Vec2::new(0.0, -2600.0);
        let mut highest = f32::MAX;
        for _ in 0..(3.0 / TICK) as usize {
            pet.step(TICK, &world, &Input::default());
            highest = highest.min(pet.position().y - h);
        }
        // Le haut de la tête passe au-dessus de la zone utile (sous la barre
        // des menus), sans dépasser le bord de l'écran.
        assert!((0.0..25.0).contains(&highest), "{highest}");
        assert_eq!(pet.position().y, 825.0, "il est retombé");
    }

    #[test]
    fn two_lainous_who_meet_do_the_same_thing() {
        let lainou = fixture("lainou");
        let world = world();
        for seed in 0..12 {
            let mut flock = Flock::new();
            let a = flock.spawn(Arc::clone(&lainou), seed, &world, None);
            let b = flock.spawn(Arc::clone(&lainou), seed + 100, &world, None);
            put(&mut flock, &world, a, 500.0, 1.0, "marche");
            put(&mut flock, &world, b, 640.0, -1.0, "marche");
            let mut reaction = None;
            for _ in 0..(3.0 / TICK) as usize {
                flock.advance(TICK, &world, &Input::default());
                if flock.pets()[0].meet_cooldown > 0.0 {
                    let names: Vec<&str> = flock.pets().iter().map(Pet::behavior_name).collect();
                    reaction = Some(names.join(" + "));
                    break;
                }
            }
            let reaction = reaction.expect("ils se rencontrent");
            assert!(
                [
                    "salue + salue",
                    "fete + fete",
                    "demi_tour + demi_tour",
                    "surpris + surpris"
                ]
                .contains(&reaction.as_str()),
                "{reaction}"
            );
        }
    }

    #[test]
    fn together_makes_every_free_pet_play_along() {
        let world = world();
        let mut flock = Flock::new();
        let ids: Vec<u32> = (0..3)
            .map(|i| flock.spawn(social(), i, &world, None))
            .collect();
        for (k, id) in ids.iter().enumerate() {
            put(
                &mut flock,
                &world,
                *id,
                300.0 + 200.0 * k as f32,
                1.0,
                "walk",
            );
        }
        assert_eq!(flock.together(&world, &Input::default()), 3);
        assert!(flock.pets().iter().all(|p| p.behavior_name() == "hop"));
        // Un animal sans « together » ne bouge pas.
        let mut flock = Flock::new();
        let id = flock.spawn(def(), 1, &world, None);
        put(&mut flock, &world, id, 300.0, 1.0, "walk");
        assert_eq!(flock.together(&world, &Input::default()), 0);
        assert_eq!(flock.pets()[0].behavior_name(), "walk");
    }
}
