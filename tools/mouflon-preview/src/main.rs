//! Aperçu hors écran de Mouflon.
//!
//! Simule une scène de bureau (fenêtres, Dock, curseur) avec le vrai moteur,
//! dessine chaque image avec le vrai rendu, puis l'exporte en PNG via SVG et
//! resvg. Sert à juger le rendu et la fluidité sans lancer l'app.
//!
//! ```text
//! cargo run -p mouflon-preview --release -- <pet.toml|animations.xml> <dossier> [video|poses|gestes|all|icon]
//! cargo run -p mouflon-preview --release -- <animal> <dossier> troupeau [secondes] [nombre]
//! ```
//!
//! Le mode `troupeau` fait apparaître plusieurs animaux à leurs points
//! d'apparition (utile pour les animaux desktopPet et leurs scènes).
//!
//! ```text
//! cargo run -p mouflon-preview --release -- <pet1.toml,pet2.toml,…> <dossier> races [secondes] [écart] [taille] [prise]
//! ```
//!
//! `races` : le troupeau de la page de présentation. Une race par chemin
//! (séparés par des virgules, sans espace) ; elles tombent du haut de
//! l'écran l'une après l'autre, `écart` secondes entre deux (1 par défaut),
//! chacune à un endroit différent, puis vivent leur vie. Au bout d'un moment
//! le curseur en attrape une (`prise` : son rang, 2 par défaut) et la lance.
//! `taille` agrandit les animaux comme le réglage « Taille » de l'app
//! (2,4 par défaut) ; une image sur deux est dessinée (30 images par
//! seconde), sur tous les cœurs de la machine.
//!
//! ```text
//! cargo run -p mouflon-preview --release -- <animal> <dossier> rencontre [comportement de marche]
//! cargo run -p mouflon-preview --release -- <animal> <dossier> planche
//! ```
//!
//! `rencontre` : deux animaux marchent l'un vers l'autre en mode collisions,
//! puis le premier est lancé sur le second. `planche` : toutes les images de
//! chaque comportement, dessinées par Mouflon dans les deux sens, sur la
//! ligne du sol (pour vérifier le point d'appui et le rognage).
//!
//! ```text
//! cargo run -p mouflon-preview --release -- <animal> <dossier> physique
//! ```
//!
//! `physique` : la physique réaliste. Une fenêtre emporte l'animal, file sous
//! lui d'un coup sec, se dérobe, le lance en l'air ; puis la main le prend
//! par l'arrière, le balance et le lance en tournant.
//!
//! ```text
//! cargo run -p mouflon-preview --release -- <animal> <dossier> jeux
//! ```
//!
//! `jeux` : les jeux d'agneaux d'un animal qui les connaît (le Bleu du Maine,
//! la Solognote) : deux agneaux reculent, s'élancent et se cognent le front, un
//! bélier recule avant de charger ; puis sauts tordus et demi-tours en l'air.
//!
//! `soucoupe` : une soucoupe emporte un animal ; on lui en lance un autre
//! dessus : elle le lâche, fume, puis s'enfuit ou tombe.
//!
//! ```text
//! cargo run -p mouflon-preview --release -- <animal> <dossier> toison
//! ```
//!
//! `toison` : les mèches (`sway`) d'un mouton simulé, à dix instants : au
//! repos, au galop, après un arrêt net, quand il s'ébroue, puis dans un coup
//! de vent qui le prend de dos.
//!
//! `couche` : le coucher de près, tourné à droite puis à gauche (il se
//! couche, couché, endormi, la tête au sol), une ligne sur le sol.
//!
//! `cloche` : la tête, l'encolure et ce qui pend sous la gorge (le redon du
//! Mérinos d'Arles), en gros plan, une ligne sur le sol : `cloche.png`, des
//! poses choisies (broutage, coucher, sommeil la tête au sol, allures,
//! ébrouement, tenu, saut, demi-tour) ; `cloche-mouvement.png`, un vrai
//! mouton simulé (il part au pas, galope, s'arrête net, s'ébroue, puis un
//! coup de vent le prend de dos).
//!
//! `aplomb` : de près sur les pattes, un pas en broutant, un arrêt après le
//! pas, un arrêt après le galop, un geste juste après l'arrêt
//! (`aplomb-broute.png`, `aplomb-pas.png`, `aplomb-galop.png`,
//! `aplomb-geste.png`) : toutes les images superposées en haut, puis une
//! par trentième de seconde, posées sur des traits fixes du sol.
//!
//! `famille` : une brebis et son agneau, de l'approche à la fin de la tétée,
//! à deux et sept semaines, dans les deux sens (planche et légende Markdown).
//!
//! `mesure` (`mouflon-preview dessin.svg - mesure`) : la boîte de la
//! silhouette d'un dessin vectoriel, son point d'appui, la longueur de sa
//! patte, et les `height`, `width` et `stride` à écrire dans son `pet.toml`.
//!
//! `dessins` : les dessins d'une race (le dessin de base, puis ceux de
//! `[appearance.male]`, `female` et `lamb`, ou l'agneau provisoire) côte à
//! côte, à la même échelle, debout et broutant, avec leur largeur et leur
//! foulée (planche et légende Markdown).
//!
//! Un dessin choisi s'ajoute au chemin : `pets/awassi/pet.toml#female`
//! (`#male`, `#lamb`) montre celui-là dans n'importe quelle planche, porté
//! par tous. Sans choix, les planches d'un seul dessin montrent celui de
//! base, et dans les scènes où vivent plusieurs animaux chacun porte le
//! dessin de son sexe et de son âge.
#![forbid(unsafe_code)]

use std::f32::consts::TAU;
use std::path::{Path, PathBuf};
use std::sync::Arc;

use mouflon_core::{
    Age, Flock, Gesture, Input, LedgeOptions, Particle, ParticleKind, Pet, PetEvent, PetView,
    PoseSignals, Rect, Screen, SexRatio, Shadow, Support, Variant, Vec2, WindowRect, World,
    load_pet,
};
use mouflon_render::display::{Command, DisplayList, Paint, Stop, rgba};
use mouflon_render::kurbo::{Affine, BezPath, Circle, Point, Rect as KRect, RoundedRect, Shape};
use mouflon_render::peniko::{Color, Extend, Mix};
use mouflon_render::svg::to_svg;
use mouflon_render::{Frame, Look, Part, PetArt, Style, paint_pet};
use resvg::{tiny_skia, usvg};

type Error = Box<dyn std::error::Error>;

mod family;
mod film;

const SCREEN_W: f32 = 960.0;
const SCREEN_H: f32 = 600.0;
const MENU_H: f32 = 26.0;
const DOCK_H: f32 = 64.0;
const SCALE: f64 = 1.5;
const FPS: u32 = 60;

fn main() -> Result<(), Error> {
    let args: Vec<String> = std::env::args().collect();
    let pet_path = args
        .get(1)
        .map_or("pets/bleu-du-maine/pet.toml", String::as_str);
    let out_dir = PathBuf::from(args.get(2).map_or("apercu", String::as_str));
    let mode = args.get(3).map_or("all", String::as_str);
    if mode == "mesure" {
        return measure(pet_path);
    }
    std::fs::create_dir_all(&out_dir)?;

    if mode == "races" {
        return render_races(&args, &out_dir);
    }
    if mode == "film" {
        return film::render(&args, &out_dir);
    }

    let def = Arc::new(load_choice(pet_path)?);
    // L'aperçu (SVG) garde les tracés vectoriels plutôt que les images cuites.
    let art = vector(PetArt::load(&def)?);
    println!(
        "{} : {} comportements, {:.0}×{:.0} pt",
        def.meta.name,
        def.behaviors.len(),
        art.width,
        art.height
    );
    // Les planches d'un seul dessin montrent celui de base (un autre :
    // `pet.toml#female`…) ; les scènes où vivent plusieurs animaux, chacun
    // avec le dessin de son sexe et de son âge.
    let one = Arc::new(only_base(&def)?);
    if mode == "dessins" {
        let path = out_dir.join("dessins.png");
        render_drawings(&def, &path)?;
        println!(
            "planche des dessins de la race : {} (légende : dessins.md)",
            path.display()
        );
    }

    if mode == "portraits" {
        let scale = args.get(4).and_then(|a| a.parse().ok()).unwrap_or(5.0);
        for name in render_portraits(&def, &out_dir, scale)? {
            println!("portrait : {}", out_dir.join(name).display());
        }
    }
    if mode == "poses" || mode == "all" {
        let path = out_dir.join("poses.png");
        render_poses(&art, &path)?;
        println!("planche de poses : {}", path.display());
    }
    if mode == "gestes" || mode == "all" {
        let path = out_dir.join("gestes.png");
        render_gestures(&art, &path)?;
        println!("planche des allures et des gestes : {}", path.display());
    }
    if mode == "allures" || mode == "all" {
        let path = out_dir.join("allures.png");
        render_gaits(&art, &path)?;
        println!("planche des allures, phase par phase : {}", path.display());
    }
    if mode == "couche" {
        let path = out_dir.join("couche.png");
        render_lying(&art, &path)?;
        println!("planche du coucher, de près : {}", path.display());
    }
    if mode == "cou" {
        let path = out_dir.join("cou.png");
        render_neck(&art, &path)?;
        println!("l'encolure et sa laine, de près : {}", path.display());
    }
    if mode == "toison" {
        let path = out_dir.join("toison.png");
        render_fleece(&one, &art, &path)?;
        println!("planche de la toison : {}", path.display());
    }
    if mode == "cloche" {
        let path = out_dir.join("cloche.png");
        render_bell(&art, &path)?;
        println!("la tête et la cloche, de près : {}", path.display());
        let path = out_dir.join("cloche-mouvement.png");
        render_bell_motion(&one, &art, &path)?;
        println!(
            "la tête et la cloche d'un mouton simulé : {}",
            path.display()
        );
    }
    if mode == "aplomb" {
        for (name, what) in render_settle(&one, &art, &out_dir)? {
            println!("{what} : {}", out_dir.join(name).display());
        }
    }
    if mode == "famille" {
        family::render(&def, &out_dir)?;
        println!(
            "planche mère–agneau : {}",
            out_dir.join("famille.png").display()
        );
    }
    if mode == "video" || mode == "all" || mode == "troupeau" {
        let frames = out_dir.join("frames");
        std::fs::create_dir_all(&frames)?;
        let options = if mode == "troupeau" {
            Scene {
                duration: args.get(4).and_then(|a| a.parse().ok()).unwrap_or(30.0),
                count: args.get(5).and_then(|a| a.parse().ok()).unwrap_or(3),
                spawn_x: None,
            }
        } else {
            Scene {
                duration: 18.0,
                count: 1,
                spawn_x: Some(300.0),
            }
        };
        let count = render_video(&def, &art, &frames, &options)?;
        println!("{count} images dans {}", frames.display());
    }
    if mode == "icon" {
        let path = out_dir.join("AppIcon.png");
        render_icon(&art, &path)?;
        println!("icône : {}", path.display());
    }
    if mode == "rencontre" {
        let frames = out_dir.join("frames");
        std::fs::create_dir_all(&frames)?;
        let walk = args.get(4).map_or_else(
            || {
                ["walk", "marche"]
                    .into_iter()
                    .find(|n| def.behavior_index(n).is_some())
                    .unwrap_or("walk")
                    .to_owned()
            },
            Clone::clone,
        );
        let count = render_meeting(&def, &art, &frames, &walk)?;
        println!("{count} images dans {}", frames.display());
    }
    if mode == "salut" {
        let frames = out_dir.join("frames");
        std::fs::create_dir_all(&frames)?;
        let count = render_greeting(&def, &art, &frames)?;
        println!("{count} images dans {}", frames.display());
    }
    if mode == "soucoupe" {
        let frames = out_dir.join("frames");
        std::fs::create_dir_all(&frames)?;
        let count = render_saucer_hit(&def, &art, &frames)?;
        println!("{count} images dans {}", frames.display());
    }
    if mode == "jeux" {
        let frames = out_dir.join("frames");
        std::fs::create_dir_all(&frames)?;
        let count = render_play(&def, &art, &frames)?;
        println!("{count} images dans {}", frames.display());
    }
    if mode == "physique" {
        let frames = out_dir.join("frames");
        std::fs::create_dir_all(&frames)?;
        let count = render_physics(&def, &art, &frames)?;
        println!("{count} images dans {}", frames.display());
    }
    if mode == "galerie" {
        // L'aperçu de la galerie, tel que l'app le dessine (sur le
        // processeur, images cuites comprises).
        let art = PetArt::load(&one)?;
        let frames = mouflon_render::thumbnail::preview(&one, &art, 8, 10.0, 2.0);
        for (k, thumb) in frames.iter().enumerate() {
            let size =
                tiny_skia::IntSize::from_wh(thumb.width, thumb.height).ok_or("aperçu vide")?;
            let pixmap =
                tiny_skia::Pixmap::from_vec(thumb.rgba.clone(), size).ok_or("aperçu invalide")?;
            pixmap.save_png(out_dir.join(format!("galerie_{k}.png")))?;
        }
        println!(
            "{} images d'aperçu dans {}",
            frames.len(),
            out_dir.display()
        );
    }
    if mode == "surprises" {
        let frames = out_dir.join("frames");
        std::fs::create_dir_all(&frames)?;
        let count = render_surprises(&def, &art, &frames)?;
        println!("{count} images dans {}", frames.display());
    }
    if mode == "planche" {
        for (facing, name) in [(-1.0, "planche-gauche.png"), (1.0, "planche-droite.png")] {
            let path = out_dir.join(name);
            render_sheet(&one, &art, facing, &path)?;
            println!("planche : {}", path.display());
        }
    }
    if mode == "rocher" {
        let rise = args
            .get(4)
            .and_then(|s| s.parse::<f32>().ok())
            .unwrap_or_else(|| default_rise(&one));
        let path = out_dir.join("rocher.png");
        render_rock(&one, &art, rise, false, &path)?;
        println!("planche du saut sur un rebord : {}", path.display());
    }
    if mode == "voyage" {
        // Le sol de l'écran voisin, bien plus haut que sa portée.
        let rise = args
            .get(4)
            .and_then(|s| s.parse::<f32>().ok())
            .unwrap_or(250.0);
        let path = out_dir.join("voyage.png");
        render_rock(&one, &art, rise, true, &path)?;
        println!(
            "planche du grand bond vers l'écran voisin : {}",
            path.display()
        );
    }
    Ok(())
}

// ---------------------------------------------------------------- mesure

/// Les mesures d'un dessin vectoriel (`fichier.svg - mesure`), pour remplir
/// son `pet.toml` : la boîte de sa silhouette (sans les pivots), son point
/// d'appui et le haut de sa patte de devant, dans les unités du dessin (un
/// centimètre de l'animal), puis `height`, `width` et `stride` à 0,5 point
/// par centimètre (la foulée : 1,6 fois la longueur de la patte).
fn measure(svg: &str) -> Result<(), Error> {
    let rig = mouflon_render::Rig::parse(&std::fs::read(svg)?)?;
    let b = rig.bounds;
    let ground = rig.ground;
    let leg = ground.y - rig.pivot(mouflon_render::Part::LegFrontNear).y;
    println!(
        "boite {:.4} {:.4} {:.4} {:.4}\nsol {:.4} {:.4}\npatte {:.4}\nheight {:.3}\nwidth {:.3}\nstride {:.3}",
        b.x0,
        b.y0,
        b.x1,
        b.y1,
        ground.x,
        ground.y,
        leg,
        b.height() * 0.5,
        b.width() * 0.5,
        leg * 0.8,
    );
    // La longueur des mèches qui se balancent (`fleece` dans `[physics]`,
    // en centimètres : une unité du dessin) : jusqu'où pend la plus longue
    // sous son attache.
    let locks = mouflon_render::Part::SWAYS
        .into_iter()
        .chain(mouflon_render::Part::NECK_SWAYS)
        .filter_map(|part| Some(rig.own_bounds(part)?.max_y() - rig.pivot(part).y))
        .fold(0.0, f64::max);
    if locks > 0.0 {
        println!("fleece {locks:.1}");
    }
    Ok(())
}

// ---------------------------------------------------------------- dessins

/// Un animal, ou l'un de ses dessins par sexe et par âge :
/// `pets/awassi/pet.toml#female` (`male`, `female` ou `lamb`, les noms des
/// sections `[appearance.…]`). Un dessin choisi est montré seul : l'animal
/// est du sexe (ou de l'âge) de ce dessin, et le porte même si la race ne
/// l'a pas encore (c'est alors le dessin de base, ou pour `lamb` l'agneau
/// que met au monde la brebis).
fn load_choice(arg: &str) -> Result<mouflon_core::PetDef, Error> {
    let (path, choice) = match arg.split_once('#') {
        Some((path, choice)) => (path, Some(choice)),
        None => (arg, None),
    };
    let def = load_pet(Path::new(path))?;
    let Some(choice) = choice else {
        return Ok(def);
    };
    let variant = Variant::ALL
        .into_iter()
        .find(|v| v.name() == choice)
        .ok_or_else(|| format!("dessin inconnu « {choice} » : male, female ou lamb"))?;
    if def.variant(variant).is_none() {
        println!("{} n'a pas de dessin « {choice} »", def.meta.name);
    }
    let mut drawn = match variant {
        // Le jeune d'une race adulte : celui que met au monde la brebis.
        Variant::Lamb if def.age == Age::Adult => {
            def.as_lamb(2.0).ok_or("cet animal n'a pas d'agneau")?
        }
        _ => def
            .variant(variant)
            .map_or_else(|| def.clone(), |drawn| (**drawn).clone()),
    };
    drawn.set_variants(Vec::new())?;
    match variant {
        Variant::Male => drawn.sex_ratio = SexRatio::MALE,
        Variant::Female => drawn.sex_ratio = SexRatio::FEMALE,
        Variant::Lamb => {}
    }
    Ok(drawn)
}

/// Les tracés vectoriels plutôt que les images cuites (l'aperçu passe par
/// SVG), pour lui et ses autres dessins.
fn vector(mut art: PetArt) -> PetArt {
    art.baked = None;
    for (_, drawn) in &mut art.variants {
        Arc::make_mut(drawn).baked = None;
    }
    art
}

/// Le même animal, avec son seul dessin de base, porté par tous.
fn only_base(def: &mouflon_core::PetDef) -> Result<mouflon_core::PetDef, Error> {
    let mut one = def.clone();
    one.set_variants(Vec::new())?;
    Ok(one)
}

/// La vitesse de marche d'un animal (le premier comportement qui marche).
fn walk_speed(def: &mouflon_core::PetDef) -> Option<f32> {
    def.behaviors.iter().find_map(|b| match b.motion {
        mouflon_core::def::Motion::Walk { speed } => Some(speed),
        _ => None,
    })
}

/// Les portraits d'un animal pour la page de présentation
/// (`tools/vitrine`) : chacun de ses dessins debout, tourné vers la droite,
/// sur fond transparent, sans ombre, à `px_per_cm` pixels par centimètre de
/// l'animal. `male.png`, `female.png` et `lamb.png` (le dessin de base porte
/// le nom de celui que ses autres dessins ne montrent pas ; `base.png` s'il
/// est seul, ou si l'on ne peut pas le dire). Le sol est à `SOL` pixels du
/// bas de chaque image.
fn render_portraits(
    def: &mouflon_core::PetDef,
    dir: &Path,
    px_per_cm: f64,
) -> Result<Vec<String>, Error> {
    const SOL: f32 = 24.0;
    let drawn: Vec<Variant> = def.variants().map(|(variant, _)| variant).collect();
    let base = match (
        drawn.contains(&Variant::Male),
        drawn.contains(&Variant::Female),
        drawn.contains(&Variant::Lamb),
    ) {
        (true, false, true) => "female",
        (false, true, true) => "male",
        (true, true, false) => "lamb",
        _ => "base",
    };
    let mut drawings = vec![(base.to_owned(), only_base(def)?)];
    for (variant, other) in def.variants() {
        drawings.push((variant.name().to_owned(), only_base(other)?));
    }
    let mut names = Vec::new();
    for (name, one) in drawings {
        let art = vector(PetArt::load(&one)?);
        // Un point du dessin vaut deux centimètres de l'animal.
        let zoom = 2.0 * px_per_cm;
        let width = (f64::from(art.width.max(art.height)) * zoom * 1.8) as u32 + 80;
        let height = (f64::from(art.height) * zoom * 1.5) as u32 + 80;
        let mut list = DisplayList::new(width, height);
        let ground = Vec2::new(width as f32 * 0.5, height as f32 - SOL);
        let view = PetView {
            id: 0,
            ground: Vec2::ZERO,
            facing: 1.0,
            pose: PoseSignals::default(),
            shadow: None,
            sprite_frame: None,
            sprite_offset_y: 0.0,
            opacity: 1.0,
            particles: &[],
            support: Support::Air,
            behavior: "",
            grip: None,
        };
        let frame = Frame {
            origin: Vec2::new(-ground.x / zoom as f32, -ground.y / zoom as f32),
            scale: zoom,
        };
        paint_pet(&art, &view, &frame, &Style::default(), &mut list);
        let file = format!("{name}.png");
        rasterize(&list, &dir.join(&file))?;
        names.push(file);
    }
    Ok(names)
}

/// La planche des dessins d'une race, côte à côte à la même échelle (une
/// unité du dessin, un centimètre, vaut 0,5 point chez tous) : le dessin de
/// base, puis ceux du mâle, de la femelle et de l'agneau s'il les a ; sans
/// dessin d'agneau, l'agneau que la brebis met au monde (le dessin de la
/// mère, réduit). En haut debout, en bas broutant, tournés vers la droite ;
/// une ligne rouge marque le sol, une barre grise la largeur (`width`,
/// centrée sur le point d'appui), une barre bleue la foulée (`stride`). La
/// légende va dans `dessins.md`.
fn render_drawings(def: &mouflon_core::PetDef, path: &Path) -> Result<(), Error> {
    let base = match def.age {
        Age::Lamb { weeks } => format!("le dessin de base (un agneau de {weeks:.0} semaines)"),
        Age::Adult if def.sex_ratio == SexRatio::MALE => "le dessin de base (un mâle)".into(),
        Age::Adult if def.sex_ratio == SexRatio::FEMALE => "le dessin de base (une femelle)".into(),
        Age::Adult => "le dessin de base".into(),
    };
    let mut drawings = vec![(base, only_base(def)?)];
    for (variant, drawn) in def.variants() {
        drawings.push((
            format!("`[appearance.{}]`", variant.name()),
            only_base(drawn)?,
        ));
    }
    if def.age == Age::Adult
        && def.variant(Variant::Lamb).is_none()
        && let Some(lamb) = def.as_lamb(2.0)
    {
        drawings.push((
            "l'agneau provisoire, faute de `[appearance.lamb]` : le dessin de la mère, réduit"
                .into(),
            lamb,
        ));
    }
    let arts = drawings
        .iter()
        .map(|(_, d)| PetArt::load(d).map(vector))
        .collect::<Result<Vec<_>, _>>()?;
    let tallest = arts.iter().map(|a| a.height).fold(1.0, f32::max);
    let longest = arts.iter().map(|a| a.width).fold(1.0, f32::max);
    let zoom = f64::from(260.0 / tallest);
    let cell_w = longest * zoom as f32 + 80.0;
    let cell_h = tallest * zoom as f32 + 110.0;
    let width = (cell_w * arts.len() as f32) as u32;
    let height = (cell_h * 2.0) as u32;
    let mut list = DisplayList::new(width, height);
    list.fill(
        rounded(Rect::new(0.0, 0.0, width as f32, height as f32), 0.0),
        Affine::IDENTITY,
        Paint::Solid(rgba(240, 242, 247, 1.0)),
    );
    let mut notes = format!(
        "# Les dessins de {}\n\nDe gauche à droite, à la même échelle (0,5 point par centimètre de l'animal) ; en haut debout, en bas broutant. Ligne rouge : le sol. Barre grise : la largeur (`width`), centrée sur le point d'appui. Barre bleue : la foulée (`stride`). Les vitesses, les sauts et la pesanteur de chaque dessin suivent la longueur de ses pattes (voir `docs/format.md`).\n\n",
        def.meta.name
    );
    let base_speed = walk_speed(def);
    for (col, ((label, drawn), art)) in drawings.iter().zip(&arts).enumerate() {
        let p = drawn.physics;
        let speed = match (walk_speed(drawn), base_speed) {
            (Some(v), Some(b)) if b > 0.0 => format!(", vitesses × {:.2}", v / b),
            _ => String::new(),
        };
        notes.push_str(&format!(
            "{}. {label} : {:.1} pt de haut ({:.0} cm), largeur {:.1} pt, foulée {:.1} pt{speed}.\n",
            col + 1,
            art.height,
            art.height * 2.0,
            p.width,
            drawn.stride(),
        ));
        for row in 0..2 {
            let ground = Vec2::new(
                (col as f32 + 0.5) * cell_w,
                (row as f32 + 1.0) * cell_h - 60.0,
            );
            let view = PetView {
                id: col as u32,
                ground: Vec2::ZERO,
                facing: 1.0,
                pose: PoseSignals {
                    graze: if row == 1 { 1.0 } else { 0.0 },
                    ..PoseSignals::default()
                },
                shadow: Some(Shadow::on(0.0, 0.0)),
                sprite_frame: None,
                sprite_offset_y: 0.0,
                opacity: 1.0,
                particles: &[],
                support: Support::Air,
                behavior: "",
                grip: None,
            };
            let frame = Frame {
                origin: Vec2::new(-ground.x / zoom as f32, -ground.y / zoom as f32),
                scale: zoom,
            };
            paint_pet(art, &view, &frame, &Style::default(), &mut list);
            let z = zoom as f32;
            for (bar, y, color) in [
                (cell_w - 24.0, ground.y, rgba(200, 80, 80, 0.55)),
                (p.width * z, ground.y + 12.0, rgba(110, 110, 120, 0.7)),
                (drawn.stride() * z, ground.y + 22.0, rgba(70, 110, 200, 0.7)),
            ] {
                list.fill(
                    rounded(Rect::new(ground.x - bar * 0.5, y, bar, 3.0), 1.5),
                    Affine::IDENTITY,
                    Paint::Solid(color),
                );
            }
        }
    }
    rasterize(&list, path)?;
    std::fs::write(path.with_extension("md"), notes)?;
    Ok(())
}

// ---------------------------------------------------------------- rocher

/// Un rebord à sa portée : les quatre cinquièmes de son plus haut saut
/// (`reach`, en longueurs de patte), 20 points sans saut sur un rebord.
fn default_rise(def: &mouflon_core::PetDef) -> f32 {
    let reach = def
        .behaviors
        .iter()
        .filter_map(|b| match b.motion {
            mouflon_core::def::Motion::Leap { reach } => Some(reach),
            _ => None,
        })
        .fold(0.0, f32::max);
    if reach > 0.0 {
        0.8 * reach * def.stride() / 1.6
    } else {
        20.0
    }
}

/// Une image de la scène du rocher : ce qu'il faut pour redessiner l'animal.
#[derive(Clone, Copy)]
struct Shot {
    t: f32,
    ground: Vec2,
    facing: f32,
    pose: PoseSignals,
    support: Support,
    shadow: Option<Shadow>,
}

/// `rocher` : deux fenêtres côte à côte, la seconde (derrière) `rise`
/// points plus haut. L'animal marche sur la première vers la seconde. La
/// planche : en haut, son saut en chronophotographie (une image sur trois,
/// de l'envol à la réception) ; dessous, les moments qui comptent (il
/// arrive, mesure le rebord, recule, se ramasse, s'envole, monte, au
/// sommet, descend, se pose, amortit, repart). Sans saut (une version du
/// moteur qui ne sait pas sauter sur un rebord, un rebord hors de portée),
/// les mêmes cases suivent ce qu'il fait au pied de la fenêtre.
///
/// `voyage` : au lieu de deux fenêtres, deux écrans côte à côte, le sol du
/// second `rise` points plus haut (bien plus que sa portée). L'animal a
/// envie de voyager : il gagne son point d'envol et fait son grand bond
/// (voir `mouflon-core/src/pet/voyage.rs`).
fn render_rock(
    def: &Arc<mouflon_core::PetDef>,
    art: &PetArt,
    rise: f32,
    voyage: bool,
    path: &Path,
) -> Result<(), Error> {
    const RATE: f32 = 60.0;
    let corner = 530.0;
    let low = if voyage {
        Rect::new(0.0, 380.0, corner, 160.0)
    } else {
        Rect::new(150.0, 380.0, 400.0, 160.0)
    };
    let high = Rect::new(corner, low.y - rise, 300.0, 160.0 + rise);
    let mut world = if voyage {
        // Deux écrans de même hauteur : la barre des tâches du second est
        // plus haute de `rise` points.
        let bottom = low.y + 60.0;
        let top = (high.y - 400.0).min(0.0);
        World::new(vec![
            Screen {
                frame: Rect::new(0.0, top, corner, bottom - top),
                usable: Rect::new(0.0, top, corner, low.y - top),
                scale: SCALE as f32,
            },
            Screen {
                frame: Rect::new(corner, top, 600.0, bottom - top),
                usable: Rect::new(corner, top, 600.0, high.y - top),
                scale: SCALE as f32,
            },
        ])
    } else {
        World::new(vec![screen()])
    };
    if !voyage {
        let windows = [
            WindowRect { id: 1, frame: low },
            WindowRect { id: 2, frame: high },
        ];
        world.set_windows(&windows, LedgeOptions::default());
    }
    let mut flock = Flock::new();
    flock.set_realistic(true);
    let id = flock.spawn(Arc::clone(def), 7, &world, Some(270.0));
    let art = art.worn(
        flock
            .pets()
            .iter()
            .find(|p| p.id() == id)
            .and_then(Pet::variant),
    );
    give_reach(&mut flock, art);
    let input = Input {
        local_hour: 15.0,
        ..Input::default()
    };
    // Il tombe sur la première fenêtre, puis marche vers la seconde.
    for _ in 0..(1.6 * RATE) as usize {
        flock.advance(1.0 / RATE, &world, &input);
    }
    if !voyage && let Some(pet) = flock.get_mut(id) {
        pet.face(1.0);
        for walk in ["walk", "marche"] {
            if pet.force(walk) {
                break;
            }
        }
    }
    let mut shots = Vec::new();
    let mut contact = None;
    let mut log = Vec::new();
    let mut travelling = false;
    for frame in 0..(30.0 * RATE) as usize {
        let t = frame as f32 / RATE;
        // L'envie de voyager lui vient dès qu'il est libre de partir.
        if voyage && !travelling {
            travelling = flock.voyage(id, &world, None);
        }
        // Il marche jusqu'au pied de la fenêtre ; ensuite, il fait ce qu'il veut.
        if !voyage
            && contact.is_none()
            && let Some(pet) = flock.get_mut(id)
            && pet.behavior_name() != "walk"
            && pet.behavior_name() != "marche"
            && matches!(pet.support(), Support::Ground { .. })
        {
            pet.face(1.0);
            for walk in ["walk", "marche"] {
                if pet.force(walk) {
                    break;
                }
            }
        }
        // Au pied de la fenêtre, il saute s'il sait sauter sur un rebord
        // (pour le montrer : sinon, une race de plaine fait plus souvent
        // demi-tour).
        let mut arrived = false;
        if !voyage
            && contact.is_none()
            && let Some(pet) = flock.get_mut(id)
            && pet.position().x + art.width * 0.5 >= corner - 3.0
        {
            pet.force("leap");
            arrived = true;
        }
        let alpha = flock.advance(1.0 / RATE, &world, &input);
        let Some(pet) = flock.pets().iter().find(|p| p.id() == id) else {
            break;
        };
        let view = pet.view(alpha);
        if contact.is_some() && frame % 6 == 0 {
            log.push(format!(
                "{:.1} {} ({:.0},{:.0})",
                t,
                pet.behavior_name(),
                view.ground.x,
                view.ground.y
            ));
        }
        shots.push(Shot {
            t,
            ground: view.ground,
            facing: view.facing,
            pose: view.pose,
            support: view.support,
            shadow: view.shadow,
        });
        // Il arrive au pied de la seconde fenêtre (l'avant de son corps) ;
        // en voyage, à son point d'envol, où il commence son saut.
        let there = if voyage {
            pet.behavior_name() == "leap"
        } else {
            arrived || view.ground.x + art.width * 0.5 >= corner - 1.0
        };
        if contact.is_none() && there {
            contact = Some(shots.len() - 1);
        }
        if let Some(c) = contact
            && t > shots[c].t + 6.0
        {
            break;
        }
    }
    let contact = contact.ok_or(if voyage {
        "il n'est jamais parti en voyage"
    } else {
        "il n'est jamais arrivé au pied de la fenêtre"
    })?;
    let after = |i: usize, seconds: f32| {
        let at = shots[i].t + seconds;
        shots
            .iter()
            .position(|s| s.t >= at)
            .unwrap_or(shots.len() - 1)
    };
    let flight: Vec<usize> = (contact..shots.len())
        .filter(|&i| shots[i].support == Support::Air)
        .collect();
    let mut keys: Vec<(usize, &str)> = Vec::new();
    keys.push((after(contact, -0.5), "arrive"));
    if let (Some(&off), Some(&down)) = (flight.first(), flight.last()) {
        let still = (contact..off)
            .filter(|&i| (shots[i].ground.x - shots[contact].ground.x).abs() < 0.5)
            .collect::<Vec<_>>();
        if let Some(&m) = still.get(still.len() / 2) {
            keys.push((m, "mesure"));
        }
        let back = (contact..off)
            .filter(|&i| i > 0 && shots[i].ground.x < shots[i - 1].ground.x - 0.05)
            .collect::<Vec<_>>();
        if let Some(&b) = back.get(back.len() / 2) {
            keys.push((b, "recule"));
        }
        keys.push((off - 1, "se ramasse"));
        keys.push(((off + 1).min(down), "s'envole"));
        let top = flight
            .iter()
            .copied()
            .min_by(|&a, &b| shots[a].ground.y.total_cmp(&shots[b].ground.y))
            .unwrap_or(off);
        keys.push(((off + top) / 2, "monte"));
        keys.push((top, "au sommet"));
        keys.push(((top + 3 * down) / 4, "descend"));
        keys.push(((down + 1).min(shots.len() - 1), "se pose"));
        keys.push((after(down + 1, 0.1), "amortit"));
        keys.push((after(down + 1, 0.7), "repart"));
    } else {
        for (s, label) in [
            (0.0, "au pied"),
            (0.25, "+0,25 s"),
            (0.5, "+0,5 s"),
            (1.0, "+1 s"),
            (1.6, "+1,6 s"),
            (2.4, "+2,4 s"),
            (3.2, "+3,2 s"),
            (4.0, "+4 s"),
        ] {
            keys.push((after(contact, s), label));
        }
    }
    // Ce qu'on voit : autour du coin, de quoi voir l'élan et le sommet.
    let w = art.width;
    let h = art.height;
    let area = if voyage {
        // Tout son chemin, de son point d'envol au sommet de son bond.
        let after = &shots[contact..];
        let left = after.iter().map(|s| s.ground.x).fold(corner, f32::min);
        let top = after.iter().map(|s| s.ground.y).fold(high.y, f32::min);
        KRect::new(
            f64::from(left - 1.1 * w),
            f64::from(top - 1.3 * h - 8.0),
            f64::from(corner + 1.8 * w),
            f64::from(low.y + 10.0),
        )
    } else {
        KRect::new(
            f64::from(corner - 1.6 * w),
            f64::from(high.y - 1.15 * h - 0.35 * rise - 8.0),
            f64::from(corner + 1.8 * w),
            f64::from(low.y + 10.0),
        )
    };
    let zoom = 360.0 / area.width();
    let (cell_w, cell_h) = (360.0, (area.height() * zoom).ceil());
    const COLS: usize = 4;
    let rows = (keys.len() + 1).div_ceil(COLS);
    let width = (cell_w * COLS as f64) as u32;
    let height = (cell_h * rows as f64) as u32;
    let mut sheet = tiny_skia::Pixmap::new(width, height).ok_or("planche vide")?;
    sheet.fill(tiny_skia::Color::from_rgba8(246, 247, 251, 255));
    // Une case : le bureau et ses fenêtres, puis les images de l'animal.
    let mut cell = |frames: &[(Shot, f32)], col: usize, row: usize| -> Result<(), Error> {
        let mut list = DisplayList::new(cell_w as u32, cell_h as u32);
        let to_cell = Affine::scale(zoom) * Affine::translate((-area.x0, -area.y0));
        list.fill(
            rounded(Rect::new(0.0, 0.0, cell_w as f32, cell_h as f32), 0.0),
            Affine::IDENTITY,
            Paint::Solid(rgba(108, 96, 176, 1.0)),
        );
        for r in [low, high] {
            list.fill(
                rounded(r, 10.0),
                to_cell,
                Paint::Solid(rgba(246, 247, 251, 1.0)),
            );
            list.fill(
                rounded(Rect::new(r.x, r.y, r.w, 22.0), 10.0),
                to_cell,
                Paint::Solid(rgba(226, 229, 238, 1.0)),
            );
        }
        for (shot, opacity) in frames {
            let view = PetView {
                id: 0,
                ground: shot.ground,
                facing: shot.facing,
                pose: shot.pose,
                shadow: shot.shadow,
                sprite_frame: None,
                sprite_offset_y: 0.0,
                opacity: *opacity,
                particles: &[],
                support: shot.support,
                behavior: "",
                grip: None,
            };
            // Un point du monde va en (point - origine) × zoom dans la case.
            let frame = Frame {
                origin: Vec2::new(area.x0 as f32, area.y0 as f32),
                scale: zoom,
            };
            paint_pet(art, &view, &frame, &Style::default(), &mut list);
        }
        let pixmap = to_pixmap(&list)?;
        sheet.draw_pixmap(
            (col as f64 * cell_w) as i32 + 1,
            (row as f64 * cell_h) as i32 + 1,
            pixmap.as_ref(),
            &tiny_skia::PixmapPaint::default(),
            tiny_skia::Transform::from_scale(
                ((cell_w - 2.0) / cell_w) as f32,
                ((cell_h - 2.0) / cell_h) as f32,
            ),
            None,
        );
        Ok(())
    };
    // La première case : tout le saut en chronophotographie ; puis les
    // moments, dans l'ordre.
    let (first, last, step) = match (flight.first(), flight.last()) {
        (Some(&off), Some(&down)) => (off.saturating_sub(6), (down + 9).min(shots.len() - 1), 3),
        _ => (
            keys.first().map_or(0, |k| k.0),
            keys.last().map_or(shots.len() - 1, |k| k.0),
            12,
        ),
    };
    let onion: Vec<(Shot, f32)> = (first..=last)
        .step_by(step)
        .map(|i| (shots[i], 0.35))
        .chain(std::iter::once((shots[last], 1.0)))
        .collect();
    cell(&onion, 0, 0)?;
    for (k, (i, _)) in keys.iter().enumerate() {
        cell(&[(shots[*i], 1.0)], (k + 1) % COLS, (k + 1) / COLS)?;
    }
    sheet.save_png(path)?;
    let legend: Vec<String> = keys
        .iter()
        .map(|(i, label)| format!("{label} ({:.2} s)", shots[*i].t - shots[contact].t))
        .collect();
    println!(
        "rebord {rise:.0} points plus haut ; en haut, tout le saut ; puis : {}",
        legend.join(", ")
    );
    std::fs::write(path.with_extension("txt"), log.join("\n"))?;
    Ok(())
}

/// Dessine une liste en mémoire (voir [`rasterize`]).
fn to_pixmap(list: &DisplayList) -> Result<tiny_skia::Pixmap, Error> {
    let svg = to_svg(list);
    let tree = usvg::Tree::from_str(&svg, &usvg::Options::default())?;
    let mut pixmap =
        tiny_skia::Pixmap::new(list.width, list.height).ok_or("taille d'image invalide")?;
    resvg::render(
        &tree,
        tiny_skia::Transform::identity(),
        &mut pixmap.as_mut(),
    );
    Ok(pixmap)
}

// ---------------------------------------------------------------- rencontre

/// Comme l'app : les animaux se touchent quand leurs dessins se touchent
/// (chacun le sien, celui de son sexe et de son âge).
fn give_reach(flock: &mut Flock, art: &PetArt) {
    for pet in flock.pets_mut() {
        let art = art.worn(pet.variant());
        if let Some((front, back)) = art.reach() {
            pet.set_reach(front, back);
        }
        if let Some(tops) = art.tops() {
            pet.set_tops(tops);
        }
    }
}

/// Deux animaux qui se rentrent dedans (mode collisions), puis un lancer.
fn render_meeting(
    def: &Arc<mouflon_core::PetDef>,
    art: &PetArt,
    dir: &Path,
    walk: &str,
) -> Result<u32, Error> {
    const RATE: u32 = 30;
    let mut world = World::new(vec![screen()]);
    world.set_windows(&[], LedgeOptions::default());
    let mut flock = Flock::new();
    flock.set_collisions(true);
    let a = flock.spawn(Arc::clone(def), 3, &world, Some(400.0));
    let b = flock.spawn(Arc::clone(def), 5, &world, Some(580.0));
    give_reach(&mut flock, art);
    let duration = 11.0;
    let frames = (duration * RATE as f32) as u32;
    let mut grabbed_at: Option<Vec2> = None;
    for frame in 0..frames {
        let t = frame as f32 / RATE as f32;
        let at = |s: f32| (t - s).abs() < 0.5 / RATE as f32;
        // Ils se font face et marchent l'un vers l'autre.
        if at(1.6) {
            for (id, facing) in [(a, 1.0), (b, -1.0)] {
                if let Some(pet) = flock.get_mut(id) {
                    pet.face(facing);
                    pet.force(walk);
                }
            }
        }
        // Puis le premier est attrapé et lancé sur le second.
        let cursor = flock.pets().iter().find(|p| p.id() == a).and_then(|pet| {
            let head = pet.position() + Vec2::new(0.0, -30.0);
            match t {
                t if (6.5..7.2).contains(&t) => Some(head),
                t if (7.2..8.2).contains(&t) => {
                    // Il le soulève, recule, puis le lance à ras du sol vers l'autre.
                    let u = (t - 7.2) / 1.0;
                    let up = Vec2::new(150.0, 420.0);
                    let to = Vec2::new(330.0, 470.0);
                    let start = *grabbed_at.get_or_insert(head);
                    if u < 0.5 {
                        Some(start.lerp(up, ease(u * 2.0)))
                    } else {
                        let v = (u - 0.5) * 2.0;
                        Some(up.lerp(to, v * v))
                    }
                }
                _ => None,
            }
        });
        if let Some(pet) = flock.get_mut(a) {
            if at(7.2)
                && let Some(c) = cursor
            {
                pet.grab(c, &world);
            }
            if pet.is_held()
                && let Some(c) = cursor
            {
                pet.drag_to(c);
            }
            if at(8.2) {
                pet.release(&world);
            }
        }
        let input = Input {
            cursor,
            local_hour: 15.0,
            ..Input::default()
        };
        let alpha = flock.advance(1.0 / RATE as f32, &world, &input);
        let mut list = DisplayList::new(
            (f64::from(SCREEN_W) * SCALE) as u32,
            (f64::from(SCREEN_H) * SCALE) as u32,
        );
        let script = Script {
            w1: Rect::new(-1000.0, 0.0, 1.0, 1.0),
            w2: Rect::new(-1000.0, 0.0, 1.0, 1.0),
        };
        paint_desktop(&mut list, &script, 0.0);
        for pet in flock.pets() {
            paint_pet(
                art.worn(pet.variant()),
                &pet.view(alpha),
                &Frame {
                    origin: Vec2::ZERO,
                    scale: SCALE,
                },
                &Style::default(),
                &mut list,
            );
        }
        if let Some(c) = cursor {
            let held = flock.pets().iter().any(Pet::is_held);
            paint_cursor(&mut list, c, held);
        }
        rasterize(&list, &dir.join(format!("frame_{frame:05}.png")))?;
        if frame % RATE == 0 {
            let states: Vec<String> = flock
                .pets()
                .iter()
                .map(|p| {
                    format!(
                        "{}@({:.0},{:.0})",
                        p.behavior_name(),
                        p.position().x,
                        p.position().y
                    )
                })
                .collect();
            println!("{t:4.1} s : {}", states.join(", "));
        }
    }
    Ok(frames)
}

// ---------------------------------------------------------------- jeux

/// Les jeux d'agneaux : deux agneaux qui reculent, s'élancent et se cognent
/// le front, un bélier qui recule avant de charger ; puis un saut tordu et
/// un demi-tour en l'air.
fn render_play(def: &Arc<mouflon_core::PetDef>, art: &PetArt, dir: &Path) -> Result<u32, Error> {
    const RATE: u32 = 30;
    let mut world = World::new(vec![screen()]);
    world.set_windows(&[], LedgeOptions::default());
    let mut flock = Flock::new();
    // Les rôles de la scène sont explicites ; les trois conservent le
    // gabarit du dessin chargé pour montrer les gestes à la même échelle.
    let mut young = (**def).clone();
    young.set_variants(Vec::new())?;
    young.age = mouflon_core::Age::Lamb { weeks: 8.0 };
    let young = Arc::new(young);
    let mut ram = (**def).clone();
    ram.set_variants(Vec::new())?;
    ram.age = mouflon_core::Age::Adult;
    ram.sex_ratio = mouflon_core::SexRatio::MALE;
    let a = flock.spawn(Arc::clone(&young), 3, &world, Some(330.0));
    let b = flock.spawn(young, 5, &world, Some(420.0));
    let c = flock.spawn(Arc::new(ram), 7, &world, Some(780.0));
    give_reach(&mut flock, art);
    let duration = 8.0;
    let frames = (duration * RATE as f32) as u32;
    for frame in 0..frames {
        let t = frame as f32 / RATE as f32;
        let at = |s: f32| (t - s).abs() < 0.5 / RATE as f32;
        let mut play = |id: u32, facing: f32, name: &str| {
            if let Some(pet) = flock.get_mut(id) {
                pet.face(facing);
                if !pet.force(name) {
                    eprintln!("{} ne sait pas « {name} »", def.meta.name);
                }
            }
        };
        if at(1.5) {
            play(a, 1.0, "play_back");
            play(b, -1.0, "play_back");
            play(c, -1.0, "back_up");
        }
        if at(5.0) {
            play(a, 1.0, "gambol");
            play(b, -1.0, "pivot");
        }
        let input = Input {
            local_hour: 15.0,
            ..Input::default()
        };
        let alpha = flock.advance(1.0 / RATE as f32, &world, &input);
        let mut list = DisplayList::new(
            (f64::from(SCREEN_W) * SCALE) as u32,
            (f64::from(SCREEN_H) * SCALE) as u32,
        );
        paint_desktop_with(&mut list, &[]);
        for pet in flock.pets() {
            paint_pet(
                art.worn(pet.variant()),
                &pet.view(alpha),
                &Frame {
                    origin: Vec2::ZERO,
                    scale: SCALE,
                },
                &Style::default(),
                &mut list,
            );
        }
        rasterize(&list, &dir.join(format!("frame_{frame:05}.png")))?;
        if frame % (RATE / 5) == 0 {
            let states: Vec<String> = flock
                .pets()
                .iter()
                .map(|p| format!("{}@{:.0}", p.behavior_name(), p.position().x))
                .collect();
            println!("{t:4.1} s : {}", states.join(", "));
        }
    }
    Ok(frames)
}

// ---------------------------------------------------------------- physique

/// La fenêtre de la scène `physique` à l'instant `t` : posée, emportée vers
/// la droite, tirée d'un coup sec vers la gauche, tirée vers le bas plus vite
/// que la pesanteur, puis montée vite et arrêtée net.
fn physics_window(t: f32) -> Rect {
    let mut r = Rect::new(120.0, 300.0, 380.0, 190.0);
    r.x += 140.0 * ease((t - 1.8) / 1.2);
    r.x -= 150.0 * ((t - 3.4) / 0.15).clamp(0.0, 1.0);
    r.y += 170.0 * ((t - 4.4) / 0.1).clamp(0.0, 1.0);
    r.y -= 230.0 * ((t - 6.0) / 0.4).clamp(0.0, 1.0);
    r
}

fn render_physics(def: &Arc<mouflon_core::PetDef>, art: &PetArt, dir: &Path) -> Result<u32, Error> {
    const RATE: u32 = 30;
    let side = Rect::new(660.0, 170.0, 260.0, 330.0);
    let windows = |t: f32| {
        vec![
            WindowRect { id: 2, frame: side },
            WindowRect {
                id: 1,
                frame: physics_window(t),
            },
        ]
    };
    let mut world = World::new(vec![screen()]);
    world.set_windows(&windows(0.0), LedgeOptions::default());
    let mut flock = Flock::new();
    flock.set_realistic(true);
    flock.set_collisions(true);
    let a = flock.spawn(Arc::clone(def), 3, &world, Some(330.0));
    let b = flock.spawn(Arc::clone(def), 5, &world, Some(580.0));
    give_reach(&mut flock, art);
    let duration = 14.0;
    let frames = (duration * RATE as f32) as u32;
    let mut grip: Option<Vec2> = None;
    for frame in 0..frames {
        let t = frame as f32 / RATE as f32;
        let at = |s: f32| (t - s).abs() < 0.5 / RATE as f32;
        world.set_windows(&windows(t), LedgeOptions::default());
        // Arrêtés pour la démonstration des fenêtres.
        if at(1.6) {
            for id in [a, b] {
                if let Some(pet) = flock.get_mut(id) {
                    let stand = ["stand", "repos", "sit"]
                        .into_iter()
                        .find(|n| def.behavior_index(n).is_some());
                    if let Some(stand) = stand {
                        pet.force(stand);
                    }
                }
            }
        }
        // La main le prend par l'arrière du dos, le balance, puis le lance.
        let cursor = flock.pets().iter().find(|p| p.id() == a).and_then(|pet| {
            let body = pet.body();
            let back = body.center
                + Vec2::new(
                    -body.half.x * 0.6 * pet.view(1.0).facing.signum(),
                    -body.half.y * 0.7,
                );
            match t {
                t if (7.4..8.0).contains(&t) => {
                    let from = Vec2::new(SCREEN_W - 80.0, 120.0);
                    Some(from.lerp(back, ease((t - 7.4) / 0.6)))
                }
                t if (8.0..9.2).contains(&t) => {
                    let start = *grip.get_or_insert(back);
                    let u = t - 8.0;
                    Some(start + Vec2::new(-90.0 * (u * 5.0).sin(), -60.0 * ease(u / 0.5)))
                }
                t if (9.2..9.45).contains(&t) => {
                    let start = grip.unwrap_or(back) + Vec2::new(-90.0 * (6.0_f32).sin(), -60.0);
                    let u = (t - 9.2) / 0.25;
                    Some(start + Vec2::new(260.0 * u * u, -120.0 * u))
                }
                _ => None,
            }
        });
        if let Some(pet) = flock.get_mut(a) {
            if at(8.0)
                && let Some(c) = cursor
            {
                pet.grab(c, &world);
            }
            if pet.is_held()
                && let Some(c) = cursor
            {
                pet.drag_to(c);
            }
            if at(9.45) {
                pet.release(&world);
            }
        }
        if at(11.5)
            && let Some(pet) = flock.get_mut(b)
        {
            for walk in ["walk", "marche"] {
                if pet.force(walk) {
                    break;
                }
            }
        }
        let input = Input {
            cursor,
            local_hour: 15.0,
            ..Input::default()
        };
        let alpha = flock.advance(1.0 / RATE as f32, &world, &input);
        let mut list = DisplayList::new(
            (f64::from(SCREEN_W) * SCALE) as u32,
            (f64::from(SCREEN_H) * SCALE) as u32,
        );
        paint_desktop_with(&mut list, &[physics_window(t), side]);
        for pet in flock.pets() {
            paint_pet(
                art.worn(pet.variant()),
                &pet.view(alpha),
                &Frame {
                    origin: Vec2::ZERO,
                    scale: SCALE,
                },
                &Style::default(),
                &mut list,
            );
        }
        if let Some(c) = cursor {
            let held = flock.pets().iter().any(Pet::is_held);
            paint_cursor(&mut list, c, held);
        }
        rasterize(&list, &dir.join(format!("frame_{frame:05}.png")))?;
        if frame % (RATE / 2) == 0 {
            let states: Vec<String> = flock
                .pets()
                .iter()
                .map(|p| {
                    format!(
                        "{}@({:.0},{:.0}) {:?} roll {:.2}",
                        p.behavior_name(),
                        p.position().x,
                        p.position().y,
                        p.support(),
                        p.view(1.0).pose.roll
                    )
                })
                .collect();
            println!("{t:4.1} s : {}", states.join(", "));
        }
    }
    Ok(frames)
}

// ---------------------------------------------------------------- surprises

/// Un coup de vent, puis la visite d'une soucoupe.
fn render_surprises(
    def: &Arc<mouflon_core::PetDef>,
    art: &PetArt,
    dir: &Path,
) -> Result<u32, Error> {
    const RATE: u32 = 30;
    let shelf = Rect::new(140.0, 330.0, 420.0, 170.0);
    let mut world = World::new(vec![screen()]);
    world.set_windows(
        &[WindowRect {
            id: 1,
            frame: shelf,
        }],
        LedgeOptions::default(),
    );
    let mut flock = Flock::new();
    flock.set_realistic(true);
    let a = flock.spawn(Arc::clone(def), 3, &world, Some(300.0));
    let _b = flock.spawn(Arc::clone(def), 8, &world, Some(760.0));
    give_reach(&mut flock, art);
    let duration = 22.0;
    let frames = (duration * RATE as f32) as u32;
    for frame in 0..frames {
        let t = frame as f32 / RATE as f32;
        let at = |s: f32| (t - s).abs() < 0.5 / RATE as f32;
        if at(1.5) {
            for pet in flock.pets_mut() {
                pet.force("repos");
            }
            flock.blow(1.0, 6.0);
        }
        // Un des deux lâché de haut en plein vent, pour le voir dériver.
        if at(2.6)
            && let Some(pet) = flock.get_mut(a)
        {
            pet.grab(pet.position() - Vec2::new(0.0, 40.0), &world);
            pet.drag_to(pet.position() - Vec2::new(0.0, 220.0));
        }
        if at(3.0)
            && let Some(pet) = flock.get_mut(a)
        {
            pet.release(&world);
        }
        if at(8.5) {
            for pet in flock.pets_mut() {
                pet.force("repos");
            }
            flock.invite_saucer(&world, 11);
        }
        let input = Input {
            local_hour: 15.0,
            ..Input::default()
        };
        let alpha = flock.advance(1.0 / RATE as f32, &world, &input);
        let mut list = DisplayList::new(
            (f64::from(SCREEN_W) * SCALE) as u32,
            (f64::from(SCREEN_H) * SCALE) as u32,
        );
        paint_desktop_with(&mut list, &[shelf]);
        let frame_of_screen = Frame {
            origin: Vec2::ZERO,
            scale: SCALE,
        };
        for saucer in flock.saucers() {
            mouflon_render::saucer::paint_saucer(
                &saucer.view(),
                &frame_of_screen,
                &Style::default(),
                &mut list,
            );
        }
        for pet in flock.pets() {
            paint_pet(
                art.worn(pet.variant()),
                &pet.view(alpha),
                &frame_of_screen,
                &Style::default(),
                &mut list,
            );
        }
        rasterize(&list, &dir.join(format!("frame_{frame:05}.png")))?;
        if frame % RATE == 0 {
            let states: Vec<String> = flock
                .pets()
                .iter()
                .map(|p| {
                    format!(
                        "{}@({:.0},{:.0}) {:?}",
                        p.behavior_name(),
                        p.position().x,
                        p.position().y,
                        p.support()
                    )
                })
                .collect();
            println!(
                "{t:4.1} s : {} ; soucoupes {}",
                states.join(", "),
                flock.saucers().len()
            );
        }
    }
    Ok(frames)
}

/// Une soucoupe emporte un animal ; la main en lance un autre dessus.
fn render_saucer_hit(
    def: &Arc<mouflon_core::PetDef>,
    art: &PetArt,
    dir: &Path,
) -> Result<u32, Error> {
    const RATE: u32 = 30;
    let mut world = World::new(vec![screen()]);
    world.set_windows(&[], LedgeOptions::default());
    let mut flock = Flock::new();
    let a = flock.spawn(Arc::clone(def), 3, &world, Some(330.0));
    give_reach(&mut flock, art);
    let mut thrower = None;
    let duration = 14.0;
    let frames = (duration * RATE as f32) as u32;
    let mut grab_from = Vec2::ZERO;
    for frame in 0..frames {
        let t = frame as f32 / RATE as f32;
        let at = |s: f32| (t - s).abs() < 0.5 / RATE as f32;
        if at(1.2) {
            if let Some(pet) = flock.get_mut(a) {
                pet.force("sleep");
            }
            flock.invite_saucer(&world, 5);
            thrower = Some(flock.spawn(Arc::clone(def), 8, &world, Some(760.0)));
            give_reach(&mut flock, art);
        }
        // La main prend l'autre, recule, puis le lance sur la soucoupe.
        let saucer = flock.saucers().first().map(|s| s.view().pos);
        if let (Some(id), Some(target)) = (thrower, saucer) {
            let aim = target + Vec2::new(-30.0, 0.0);
            let back = target + Vec2::new(-260.0, 40.0);
            if let Some(pet) = flock.get_mut(id) {
                if at(6.6) {
                    grab_from = pet.position() - Vec2::new(0.0, 30.0);
                    pet.grab(grab_from, &world);
                }
                if pet.is_held() && t < 7.3 {
                    let u = ((t - 6.6) / 0.7).clamp(0.0, 1.0);
                    pet.drag_to(grab_from.lerp(back, ease(u)));
                } else if pet.is_held() && t < 7.42 {
                    let u = ((t - 7.3) / 0.12).clamp(0.0, 1.0);
                    pet.drag_to(back.lerp(aim, u));
                }
                if at(7.42) {
                    pet.release(&world);
                }
            }
        }
        let input = Input {
            local_hour: 15.0,
            ..Input::default()
        };
        let alpha = flock.advance(1.0 / RATE as f32, &world, &input);
        let mut list = DisplayList::new(
            (f64::from(SCREEN_W) * SCALE) as u32,
            (f64::from(SCREEN_H) * SCALE) as u32,
        );
        paint_desktop_with(&mut list, &[]);
        let frame_of_screen = Frame {
            origin: Vec2::ZERO,
            scale: SCALE,
        };
        for saucer in flock.saucers() {
            mouflon_render::saucer::paint_saucer(
                &saucer.view(),
                &frame_of_screen,
                &Style::default(),
                &mut list,
            );
        }
        for pet in flock.pets() {
            paint_pet(
                art.worn(pet.variant()),
                &pet.view(alpha),
                &frame_of_screen,
                &Style::default(),
                &mut list,
            );
        }
        rasterize(&list, &dir.join(format!("frame_{frame:05}.png")))?;
        if frame % (RATE / 5) == 0 {
            let states: Vec<String> = flock
                .pets()
                .iter()
                .map(|p| {
                    format!(
                        "{}@({:.0},{:.0})",
                        p.behavior_name(),
                        p.position().x,
                        p.position().y
                    )
                })
                .collect();
            let saucers: Vec<String> = flock
                .saucers()
                .iter()
                .map(|s| {
                    let v = s.view();
                    format!("({:.0},{:.0}) fumée {:.2}", v.pos.x, v.pos.y, v.smoke)
                })
                .collect();
            println!(
                "{t:4.1} s : {} ; soucoupe {}",
                states.join(", "),
                saucers.join(" ")
            );
        }
    }
    Ok(frames)
}

// ---------------------------------------------------------------- salut

/// Sans mode collisions : deux animaux se rencontrent et réagissent, un
/// troisième bute contre le côté d'une fenêtre, puis « Tous ensemble » et
/// « Au revoir ».
fn render_greeting(
    def: &Arc<mouflon_core::PetDef>,
    art: &PetArt,
    dir: &Path,
) -> Result<u32, Error> {
    const RATE: u32 = 30;
    let mut world = World::new(vec![screen()]);
    // Une fenêtre qui descend jusqu'au Dock, à droite.
    let window = Rect::new(700.0, 300.0, 240.0, SCREEN_H - DOCK_H - 300.0 + 20.0);
    world.set_windows(
        &[WindowRect {
            id: 1,
            frame: window,
        }],
        LedgeOptions::default(),
    );
    let walk = ["walk", "marche"]
        .into_iter()
        .find(|n| def.behavior_index(n).is_some())
        .unwrap_or("walk");
    let mut flock = Flock::new();
    let a = flock.spawn(Arc::clone(def), 3, &world, Some(250.0));
    let b = flock.spawn(Arc::clone(def), 5, &world, Some(430.0));
    let c = flock.spawn(Arc::clone(def), 8, &world, Some(600.0));
    give_reach(&mut flock, art);
    let input = Input {
        cursor: None,
        local_hour: 15.0,
        ..Input::default()
    };
    // Une fois tout le monde posé (la chute d'un animal desktopPet est
    // lente), les deux premiers marchent l'un vers l'autre et le troisième
    // vers la fenêtre ; puis « Tous ensemble » et « Au revoir ».
    let landing = [
        "fall",
        "fall soft",
        "fall hard",
        "arrivee",
        "atterrit",
        "atterrit_sonne",
    ];
    let mut start: Option<f32> = None;
    let mut met = false;
    let mut frame = 0;
    loop {
        let t = frame as f32 / RATE as f32;
        let settled = flock.pets().iter().all(|p| {
            matches!(p.support(), Support::Ground { .. }) && !landing.contains(&p.behavior_name())
        });
        if start.is_none() && t >= 1.0 && (settled || t > 12.0) {
            start = Some(t);
            for (id, facing) in [(a, 1.0), (b, -1.0), (c, 1.0)] {
                if let Some(pet) = flock.get_mut(id) {
                    pet.face(facing);
                    pet.force(walk);
                }
            }
        }
        // Jusqu'à leur rencontre, les deux premiers continuent de marcher.
        if start.is_some() && !met {
            for (id, facing) in [(a, 1.0), (b, -1.0)] {
                if let Some(pet) = flock.get_mut(id)
                    && pet.behavior_name() != walk
                {
                    pet.face(facing);
                    pet.force(walk);
                }
            }
        }
        let since = start.map_or(-1.0, |s| t - s);
        let at = |s: f32| (since - s).abs() < 0.5 / RATE as f32;
        if since > 12.5 || flock.pets().is_empty() && since > 0.0 {
            break;
        }
        if at(7.0) {
            println!(
                "{t:4.1} s : tous ensemble ({})",
                flock.together(&world, &input)
            );
        }
        if at(9.5) {
            println!("{t:4.1} s : au revoir");
            flock.leave_all(&world, &input);
        }
        let alpha = flock.advance(1.0 / RATE as f32, &world, &input);
        for pet in flock.pets_mut() {
            met |= pet
                .take_events()
                .iter()
                .any(|e| matches!(e, PetEvent::Met { .. }));
        }
        let mut list = DisplayList::new(
            (f64::from(SCREEN_W) * SCALE) as u32,
            (f64::from(SCREEN_H) * SCALE) as u32,
        );
        let script = Script {
            w1: window,
            w2: Rect::new(-1000.0, 0.0, 1.0, 1.0),
        };
        paint_desktop(&mut list, &script, 0.0);
        for pet in flock.pets() {
            paint_pet(
                art.worn(pet.variant()),
                &pet.view(alpha),
                &Frame {
                    origin: Vec2::ZERO,
                    scale: SCALE,
                },
                &Style::default(),
                &mut list,
            );
        }
        rasterize(&list, &dir.join(format!("frame_{frame:05}.png")))?;
        if frame % (RATE / 2) == 0 {
            let states: Vec<String> = flock
                .pets()
                .iter()
                .map(|p| {
                    format!(
                        "{}@{:.0}{}",
                        p.behavior_name(),
                        p.position().x,
                        if p.is_leaving() { " (s'en va)" } else { "" }
                    )
                })
                .collect();
            println!("{t:4.1} s : {}", states.join(", "));
        }
        frame += 1;
    }
    Ok(frame)
}

// ---------------------------------------------------------------- planche

/// Toutes les images de chaque comportement, une ligne par comportement.
fn render_sheet(
    def: &mouflon_core::PetDef,
    art: &PetArt,
    facing: f32,
    path: &Path,
) -> Result<(), Error> {
    let rows: Vec<(&str, &mouflon_core::def::SpriteClip)> = def
        .behaviors
        .iter()
        .filter_map(|b| Some((b.name.as_str(), b.clip.as_ref()?)))
        .collect();
    let cell_w = art.width.max(art.body.0 * 2.2) * 1.1;
    let cell_h = art.height * 1.25;
    let cols = rows.iter().map(|(_, c)| c.frames.len()).max().unwrap_or(1);
    let zoom = 2.0;
    let width = (cell_w * cols as f32 * zoom as f32).ceil() as u32;
    let height = (cell_h * rows.len().max(1) as f32 * zoom as f32).ceil() as u32;
    let mut list = DisplayList::new(width, height);
    list.fill(
        rounded(Rect::new(0.0, 0.0, width as f32, height as f32), 0.0),
        Affine::IDENTITY,
        Paint::Solid(rgba(236, 240, 232, 1.0)),
    );
    for (row, (name, clip)) in rows.iter().enumerate() {
        println!(
            "ligne {:2} : {name} ({} images)",
            row + 1,
            clip.frames.len()
        );
        let ground_y = (row as f32 + 0.86) * cell_h;
        list.fill(
            rounded(
                Rect::new(0.0, ground_y * zoom as f32, width as f32, 1.0),
                0.0,
            ),
            Affine::IDENTITY,
            Paint::Solid(rgba(220, 120, 120, 0.6)),
        );
        for (col, frame) in clip.frames.iter().enumerate() {
            let ground = Vec2::new((col as f32 + 0.5) * cell_w, ground_y);
            let view = PetView {
                id: 0,
                ground,
                facing,
                pose: PoseSignals::default(),
                shadow: Some(Shadow::on(ground.y, 0.0)),
                sprite_frame: Some(*frame),
                sprite_offset_y: 0.0,
                opacity: 1.0,
                particles: &[],
                support: Support::Ground { surface: 0 },
                behavior: name,
                grip: None,
            };
            paint_pet(
                art,
                &view,
                &Frame {
                    origin: Vec2::ZERO,
                    scale: zoom,
                },
                &Style::default(),
                &mut list,
            );
        }
    }
    rasterize(&list, path)
}

/// Icône d'application 1024×1024 : carré arrondi façon macOS et personnage au centre.
fn render_icon(art: &PetArt, path: &Path) -> Result<(), Error> {
    let size = 1024u32;
    let mut list = DisplayList::new(size, size);
    // Gabarit des icônes macOS : carré de 824 px centré, rayon ≈ 185 px.
    let tile = Rect::new(100.0, 100.0, 824.0, 824.0);
    list.push(Command::BlurredRect {
        rect: KRect::new(100.0, 118.0, 924.0, 942.0),
        radius: 185.0,
        std_dev: 22.0,
        color: Color::BLACK.with_alpha(0.28),
        transform: Affine::IDENTITY,
    });
    list.fill(
        rounded(tile, 185.0),
        Affine::IDENTITY,
        Paint::Linear {
            start: Point::new(0.0, 100.0),
            end: Point::new(0.0, 924.0),
            stops: vec![
                Stop {
                    offset: 0.0,
                    color: rgba(226, 255, 245, 1.0),
                },
                Stop {
                    offset: 0.55,
                    color: rgba(150, 228, 210, 1.0),
                },
                Stop {
                    offset: 1.0,
                    color: rgba(62, 168, 160, 1.0),
                },
            ],
            extend: Extend::Pad,
        },
    );
    list.fill(
        Circle::new((400.0, 260.0), 420.0).to_path(0.1),
        Affine::IDENTITY,
        Paint::Radial {
            start: Point::new(400.0, 260.0),
            start_radius: 0.0,
            end: Point::new(400.0, 260.0),
            end_radius: 420.0,
            stops: vec![
                Stop {
                    offset: 0.0,
                    color: Color::WHITE.with_alpha(0.35),
                },
                Stop {
                    offset: 1.0,
                    color: Color::WHITE.with_alpha(0.0),
                },
            ],
            extend: Extend::Pad,
        },
    );
    let zoom = 560.0 / f64::from(art.height.max(art.width));
    let ground = Vec2::new(512.0, 760.0);
    let frame = Frame {
        origin: Vec2::new(-ground.x / zoom as f32, -ground.y / zoom as f32),
        scale: zoom,
    };
    let view = PetView {
        id: 0,
        ground: Vec2::ZERO,
        facing: 1.0,
        pose: PoseSignals {
            look: Vec2::new(0.45, 0.1),
            breath: 0.3,
            ..PoseSignals::default()
        },
        shadow: Some(Shadow::on(0.0, 0.0)),
        sprite_frame: Some(0),
        sprite_offset_y: 0.0,
        opacity: 1.0,
        particles: &[],
        support: Support::Air,
        behavior: "",
        grip: None,
    };
    paint_pet(art, &view, &frame, &Style::default(), &mut list);
    rasterize(&list, path)
}

// ---------------------------------------------------------------- scénario

fn ease(t: f32) -> f32 {
    let t = t.clamp(0.0, 1.0);
    t * t * (3.0 - 2.0 * t)
}

struct Script {
    w1: Rect,
    w2: Rect,
}

impl Script {
    fn back_window(&self, t: f32) -> Rect {
        // La fenêtre du fond glisse vers la droite : le personnage voyage avec elle.
        let shift = 90.0 * ease((t - 3.0) / 1.6);
        Rect::new(self.w1.x + shift, self.w1.y, self.w1.w, self.w1.h)
    }

    fn windows(&self, t: f32) -> Vec<WindowRect> {
        vec![
            WindowRect {
                id: 2,
                frame: self.w2,
            },
            WindowRect {
                id: 1,
                frame: self.back_window(t),
            },
        ]
    }
}

fn screen() -> Screen {
    Screen {
        frame: Rect::new(0.0, 0.0, SCREEN_W, SCREEN_H),
        usable: Rect::new(0.0, MENU_H, SCREEN_W, SCREEN_H - MENU_H - DOCK_H),
        scale: SCALE as f32,
    }
}

/// Réglages d'une scène d'aperçu.
struct Scene {
    duration: f32,
    count: u32,
    /// Abscisse d'apparition ; sinon les points d'apparition de l'animal.
    spawn_x: Option<f32>,
}

fn render_video(
    def: &Arc<mouflon_core::PetDef>,
    art: &PetArt,
    dir: &Path,
    scene: &Scene,
) -> Result<u32, Error> {
    let script = Script {
        w1: Rect::new(110.0, 300.0, 400.0, 220.0),
        w2: Rect::new(560.0, 190.0, 340.0, 300.0),
    };
    let mut world = World::new(vec![screen()]);
    world.set_windows(&script.windows(0.0), LedgeOptions::default());
    let mut flock = Flock::new();
    let first = flock.spawn(Arc::clone(def), 7, &world, scene.spawn_x);
    for i in 1..scene.count {
        flock.spawn(
            Arc::clone(def),
            7 + u64::from(i) * 7919,
            &world,
            scene.spawn_x,
        );
    }

    let frames = (scene.duration * FPS as f32) as u32;
    let mut grab_start: Option<Vec2> = None;
    let mut cursor_path: Option<(Vec2, Vec2)> = None;

    for frame in 0..frames {
        let t = frame as f32 / FPS as f32;
        world.set_windows(&script.windows(t), LedgeOptions::default());

        // Curseur : il s'approche du premier animal, l'attrape et le lance.
        let cursor = flock
            .pets()
            .iter()
            .find(|p| p.id() == first)
            .and_then(|pet| cursor_at(t, pet, &mut grab_start, &mut cursor_path));
        let input = Input {
            cursor,
            local_hour: 15.0,
            ..Input::default()
        };

        let alpha = flock.advance(1.0 / FPS as f32, &world, &input);
        if let Some(pet) = flock.get_mut(first) {
            script_events(t, pet, &world, &input, cursor);
        }

        let mut list = DisplayList::new(
            (f64::from(SCREEN_W) * SCALE) as u32,
            (f64::from(SCREEN_H) * SCALE) as u32,
        );
        paint_desktop(&mut list, &script, t);
        for pet in flock.pets() {
            paint_pet(
                art.worn(pet.variant()),
                &pet.view(alpha),
                &Frame {
                    origin: Vec2::ZERO,
                    scale: SCALE,
                },
                &Style::default(),
                &mut list,
            );
        }
        if let Some(c) = cursor {
            let held = flock.pets().iter().any(Pet::is_held);
            paint_cursor(&mut list, c, held);
        }
        rasterize(&list, &dir.join(format!("frame_{frame:05}.png")))?;
    }
    Ok(frames)
}

fn cursor_at(
    t: f32,
    pet: &Pet,
    grab_start: &mut Option<Vec2>,
    path: &mut Option<(Vec2, Vec2)>,
) -> Option<Vec2> {
    cursor_at_height(t, pet, 44.0, grab_start, path)
}

/// Le geste du curseur, pour un animal dont la tête est à `lift` points au-dessus de ses pieds.
fn cursor_at_height(
    t: f32,
    pet: &Pet,
    lift: f32,
    grab_start: &mut Option<Vec2>,
    path: &mut Option<(Vec2, Vec2)>,
) -> Option<Vec2> {
    let head = pet.position() + Vec2::new(0.0, -lift);
    if (5.0..6.6).contains(&t) {
        // Le curseur arrive par la droite et s'arrête au-dessus du personnage.
        let from = Vec2::new(SCREEN_W - 60.0, 140.0);
        let k = ease((t - 5.0) / 1.2);
        return Some(
            from.lerp(head + Vec2::new(30.0, -40.0), k)
                .lerp(head, ease((t - 6.2) / 0.4)),
        );
    }
    if (6.6..7.75).contains(&t) {
        let start = *grab_start.get_or_insert(head);
        let end = Vec2::new(500.0, 110.0);
        *path = Some((start, end));
        // Un petit arc, puis un geste vif au moment du lâcher : l'animal part avec élan.
        let u = ((t - 6.6) / 1.15).clamp(0.0, 1.0);
        let k = if u < 0.8 {
            0.5 * ease(u / 0.8)
        } else {
            0.5 + 0.5 * (u - 0.8) / 0.2
        };
        let arc = Vec2::new(0.0, -60.0 * (k * std::f32::consts::PI).sin());
        return Some(start.lerp(end, k) + arc);
    }
    None
}

/// Les points d'où tombent les races de `races` : bien écartés, sur les deux
/// fenêtres et sur le sol entre elles.
const RACE_DROPS: [f32; 4] = [140.0, 750.0, 330.0, 495.0];

/// La vidéo `races` : plusieurs races, qui arrivent l'une après l'autre.
fn render_races(args: &[String], out_dir: &Path) -> Result<(), Error> {
    let paths: Vec<&str> = args
        .get(1)
        .map_or("", String::as_str)
        .split(',')
        .filter(|p| !p.is_empty())
        .collect();
    if paths.is_empty() {
        return Err("races : donne au moins un animal (chemins séparés par des virgules)".into());
    }
    let arg = |i: usize, default: f32| args.get(i).and_then(|a| a.parse().ok()).unwrap_or(default);
    let duration = arg(4, 18.0);
    let gap = arg(5, 1.0);
    let size = arg(6, 2.4);
    let grabbed = (arg(7, 2.0) as usize).min(paths.len() - 1);

    let mut casts = Vec::new();
    for path in &paths {
        let def = Arc::new(load_choice(path)?.scaled(size));
        let art = vector(PetArt::load(&def)?);
        println!("{} : ×{size}", def.meta.name);
        casts.push((def, art));
    }

    // Deux fenêtres, assez larges pour plusieurs grands moutons, et le sol entre elles.
    let windows = [
        Rect::new(30.0, 310.0, 400.0, 190.0),
        Rect::new(560.0, 190.0, 380.0, 230.0),
    ];
    let rects = [
        WindowRect {
            id: 2,
            frame: windows[1],
        },
        WindowRect {
            id: 1,
            frame: windows[0],
        },
    ];
    let mut world = World::new(vec![screen()]);
    world.set_windows(&rects, LedgeOptions::default());

    let lead = 0.4;
    let last_arrival = lead + gap * (casts.len() - 1) as f32;
    // Le geste du curseur (5 s après le début de son script) commence 5 s après la dernière arrivée :
    // tout le monde a eu le temps de vivre un peu.
    let shift = last_arrival;
    let lift = casts[grabbed].0.physics.height * 1.1;

    let frames_dir = out_dir.join("frames");
    std::fs::create_dir_all(&frames_dir)?;
    let mut flock = Flock::new();
    let mut ids: Vec<Option<u32>> = vec![None; casts.len()];
    let mut grab_start: Option<Vec2> = None;
    let mut cursor_path: Option<(Vec2, Vec2)> = None;
    let mut jobs: Vec<(PathBuf, DisplayList)> = Vec::new();
    let mut written = 0_u32;
    let steps = (duration * FPS as f32) as u32;

    for step in 0..steps {
        let t = step as f32 / FPS as f32;
        for (i, (def, _)) in casts.iter().enumerate() {
            if ids[i].is_none() && t >= lead + gap * i as f32 {
                let x = RACE_DROPS[i % RACE_DROPS.len()];
                ids[i] = Some(flock.spawn(Arc::clone(def), 7 + i as u64 * 7919, &world, Some(x)));
            }
        }

        // Le curseur attrape l'animal choisi, quand il est là, et le lance.
        let ts = t - shift;
        let target = ids[grabbed];
        let cursor = flock
            .pets()
            .iter()
            .find(|p| Some(p.id()) == target)
            .and_then(|pet| cursor_at_height(ts, pet, lift, &mut grab_start, &mut cursor_path));
        let input = Input {
            cursor,
            local_hour: 15.0,
            ..Input::default()
        };
        let alpha = flock.advance(1.0 / FPS as f32, &world, &input);
        if let Some(pet) = target.and_then(|id| flock.get_mut(id)) {
            let at = |s: f32| (ts - s).abs() < 0.5 / FPS as f32;
            if at(6.6)
                && let Some(c) = cursor
            {
                pet.grab(c, &world);
            }
            if (6.6..7.75).contains(&ts)
                && let Some(c) = cursor
            {
                pet.drag_to(c);
            }
            if at(7.75) {
                pet.release(&world);
            }
        }

        // Une image sur deux est dessinée : 30 images par seconde.
        if step % 2 == 1 {
            continue;
        }
        let mut list = DisplayList::new(
            (f64::from(SCREEN_W) * SCALE) as u32,
            (f64::from(SCREEN_H) * SCALE) as u32,
        );
        paint_desktop_with(&mut list, &windows);
        for pet in flock.pets() {
            let Some(index) = ids.iter().position(|id| *id == Some(pet.id())) else {
                continue;
            };
            paint_pet(
                casts[index].1.worn(pet.variant()),
                &pet.view(alpha),
                &Frame {
                    origin: Vec2::ZERO,
                    scale: SCALE,
                },
                &Style::default(),
                &mut list,
            );
        }
        if let Some(c) = cursor {
            let held = flock.pets().iter().any(Pet::is_held);
            paint_cursor(&mut list, c, held);
        }
        jobs.push((frames_dir.join(format!("frame_{written:05}.png")), list));
        written += 1;
        if jobs.len() >= 8 {
            rasterize_all(std::mem::take(&mut jobs))?;
        }
    }
    rasterize_all(jobs)?;
    println!("{written} images dans {}", frames_dir.display());
    Ok(())
}

/// Dessine ces images en PNG, sur tous les cœurs de la machine.
fn rasterize_all(jobs: Vec<(PathBuf, DisplayList)>) -> Result<(), Error> {
    let threads = std::thread::available_parallelism().map_or(1, usize::from);
    let chunk = jobs.len().div_ceil(threads).max(1);
    let errors: Vec<String> = std::thread::scope(|scope| {
        let handles: Vec<_> = jobs
            .chunks(chunk)
            .map(|part| {
                scope.spawn(move || {
                    part.iter()
                        .filter_map(|(path, list)| rasterize(list, path).err())
                        .map(|e| e.to_string())
                        .collect::<Vec<_>>()
                })
            })
            .collect();
        handles
            .into_iter()
            .flat_map(|h| h.join().unwrap_or_else(|_| vec!["fil planté".to_owned()]))
            .collect()
    });
    match errors.first() {
        Some(e) => Err(e.clone().into()),
        None => Ok(()),
    }
}

fn script_events(t: f32, pet: &mut Pet, world: &World, input: &Input, cursor: Option<Vec2>) {
    let at = |s: f32| (t - s).abs() < 0.5 / FPS as f32;
    if at(1.0) {
        pet.force("walk");
    }
    if at(5.0) {
        pet.force("look");
    }
    if at(6.6)
        && let Some(c) = cursor
    {
        pet.grab(c, world);
    }
    if (6.6..7.75).contains(&t)
        && let Some(c) = cursor
    {
        pet.drag_to(c);
    }
    if at(7.75) {
        pet.release(world);
    }
    if at(10.2) && matches!(pet.support(), Support::Ground { .. }) {
        pet.force("walk");
    }
    if at(12.4) {
        pet.force("sit");
    }
    if at(13.6) {
        pet.force("sleep");
    }
    if at(17.0) {
        pet.poke(world, input);
    }
}

// ---------------------------------------------------------------- décor

fn rounded(r: Rect, radius: f64) -> BezPath {
    RoundedRect::new(
        f64::from(r.x),
        f64::from(r.y),
        f64::from(r.right()),
        f64::from(r.bottom()),
        radius,
    )
    .to_path(0.05)
}

fn world_to_px() -> Affine {
    Affine::scale(SCALE)
}

fn paint_desktop(list: &mut DisplayList, script: &Script, t: f32) {
    paint_desktop_with(list, &[script.back_window(t), script.w2]);
}

/// Le bureau, avec ces fenêtres (de l'arrière vers l'avant).
fn paint_desktop_with(list: &mut DisplayList, windows: &[Rect]) {
    let w = Affine::scale(SCALE);
    let screen = Rect::new(0.0, 0.0, SCREEN_W, SCREEN_H);
    list.fill(
        rounded(screen, 0.0),
        w,
        Paint::Linear {
            start: Point::new(0.0, 0.0),
            end: Point::new(f64::from(SCREEN_W) * 0.4, f64::from(SCREEN_H)),
            stops: vec![
                Stop {
                    offset: 0.0,
                    color: rgba(40, 60, 120, 1.0),
                },
                Stop {
                    offset: 0.55,
                    color: rgba(108, 96, 176, 1.0),
                },
                Stop {
                    offset: 1.0,
                    color: rgba(232, 150, 170, 1.0),
                },
            ],
            extend: Extend::Pad,
        },
    );
    list.fill(
        Circle::new((720.0, 120.0), 420.0).to_path(0.1),
        w,
        Paint::Radial {
            start: Point::new(720.0, 120.0),
            start_radius: 0.0,
            end: Point::new(720.0, 120.0),
            end_radius: 420.0,
            stops: vec![
                Stop {
                    offset: 0.0,
                    color: rgba(150, 190, 255, 0.45),
                },
                Stop {
                    offset: 1.0,
                    color: rgba(150, 190, 255, 0.0),
                },
            ],
            extend: Extend::Pad,
        },
    );
    // Barre des menus.
    list.fill(
        rounded(Rect::new(0.0, 0.0, SCREEN_W, MENU_H), 0.0),
        w,
        Paint::Solid(Color::WHITE.with_alpha(0.16)),
    );

    for window in windows {
        paint_window(list, *window);
    }

    // Dock.
    let dock = Rect::new(
        SCREEN_W / 2.0 - 250.0,
        SCREEN_H - DOCK_H + 8.0,
        500.0,
        DOCK_H - 16.0,
    );
    list.push(Command::BlurredRect {
        rect: KRect::new(
            f64::from(dock.x),
            f64::from(dock.y) + 6.0,
            f64::from(dock.right()),
            f64::from(dock.bottom()) + 6.0,
        ),
        radius: 18.0,
        std_dev: 10.0,
        color: Color::BLACK.with_alpha(0.22),
        transform: w,
    });
    list.fill(
        rounded(dock, 18.0),
        w,
        Paint::Solid(Color::WHITE.with_alpha(0.28)),
    );
    let icons = [
        (255, 94, 87),
        (255, 189, 46),
        (40, 200, 64),
        (64, 156, 255),
        (175, 82, 222),
        (255, 105, 180),
        (90, 200, 250),
    ];
    for (i, (r, g, b)) in icons.into_iter().enumerate() {
        let x = dock.x + 22.0 + i as f32 * 66.0;
        let icon = Rect::new(x, dock.y + 6.0, 36.0, 36.0);
        list.fill(
            rounded(icon, 9.0),
            w,
            Paint::Linear {
                start: Point::new(0.0, f64::from(icon.y)),
                end: Point::new(0.0, f64::from(icon.bottom())),
                stops: vec![
                    Stop {
                        offset: 0.0,
                        color: rgba(r, g, b, 1.0),
                    },
                    Stop {
                        offset: 1.0,
                        color: rgba((r / 4) * 3, (g / 4) * 3, (b / 4) * 3, 1.0),
                    },
                ],
                extend: Extend::Pad,
            },
        );
    }
}

fn paint_window(list: &mut DisplayList, r: Rect) {
    let w = world_to_px();
    list.push(Command::BlurredRect {
        rect: KRect::new(
            f64::from(r.x),
            f64::from(r.y) + 12.0,
            f64::from(r.right()),
            f64::from(r.bottom()) + 12.0,
        ),
        radius: 16.0,
        std_dev: 18.0,
        color: Color::BLACK.with_alpha(0.32),
        transform: w,
    });
    list.fill(rounded(r, 16.0), w, Paint::Solid(rgba(246, 247, 251, 1.0)));
    list.fill(
        rounded(Rect::new(r.x, r.y, r.w, 40.0), 16.0),
        w,
        Paint::Linear {
            start: Point::new(0.0, f64::from(r.y)),
            end: Point::new(0.0, f64::from(r.y) + 40.0),
            stops: vec![
                Stop {
                    offset: 0.0,
                    color: rgba(236, 238, 245, 1.0),
                },
                Stop {
                    offset: 1.0,
                    color: rgba(246, 247, 251, 1.0),
                },
            ],
            extend: Extend::Pad,
        },
    );
    for (i, (cr, cg, cb)) in [(255, 95, 87), (254, 188, 46), (40, 200, 64)]
        .into_iter()
        .enumerate()
    {
        let c = Point::new(
            f64::from(r.x) + 20.0 + i as f64 * 20.0,
            f64::from(r.y) + 20.0,
        );
        list.fill(
            Circle::new(c, 6.0).to_path(0.05),
            w,
            Paint::Solid(rgba(cr, cg, cb, 1.0)),
        );
    }
    let mut y = r.y + 64.0;
    let mut k = 0;
    while y + 14.0 < r.bottom() - 16.0 {
        let width = r.w * [0.72, 0.55, 0.8, 0.4, 0.66][k % 5] - 32.0;
        list.fill(
            rounded(Rect::new(r.x + 24.0, y, width, 10.0), 5.0),
            w,
            Paint::Solid(rgba(222, 226, 236, 1.0)),
        );
        y += 26.0;
        k += 1;
    }
}

fn paint_cursor(list: &mut DisplayList, c: Vec2, grabbing: bool) {
    let mut path = BezPath::new();
    if grabbing {
        list.fill(
            Circle::new((f64::from(c.x), f64::from(c.y)), 9.0).to_path(0.05),
            world_to_px(),
            Paint::Solid(Color::WHITE.with_alpha(0.95)),
        );
        list.fill(
            Circle::new((f64::from(c.x), f64::from(c.y)), 6.5).to_path(0.05),
            world_to_px(),
            Paint::Solid(rgba(30, 30, 40, 0.9)),
        );
        return;
    }
    let p = |x: f64, y: f64| Point::new(f64::from(c.x) + x, f64::from(c.y) + y);
    path.move_to(p(0.0, 0.0));
    path.line_to(p(0.0, 18.0));
    path.line_to(p(4.6, 13.8));
    path.line_to(p(8.2, 21.0));
    path.line_to(p(11.0, 19.6));
    path.line_to(p(7.6, 12.8));
    path.line_to(p(13.4, 12.8));
    path.close_path();
    list.push(Command::Stroke {
        path: path.clone(),
        transform: world_to_px(),
        paint: Paint::Solid(Color::WHITE),
        brush_transform: None,
        style: mouflon_render::display::StrokeStyle {
            width: 2.4,
            cap: mouflon_render::display::LineCap::Round,
            join: mouflon_render::display::LineJoin::Round,
            miter_limit: 4.0,
        },
    });
    list.fill(path, world_to_px(), Paint::Solid(rgba(20, 20, 24, 1.0)));
}

// ---------------------------------------------------------------- poses

type PoseCase = (PoseSignals, f32, Option<f32>, Vec<Particle>);

/// La taille des cases d'une planche, son agrandissement, et une ligne sur
/// le sol (par-dessus l'animal : ce qui passerait dessous se voit).
struct Layout {
    cell_w: f32,
    cell_h: f32,
    zoom: f64,
    ground_line: bool,
}

/// Une planche de poses, `cols` par rangée, sur un fond clair.
fn draw_cases(art: &PetArt, path: &Path, cases: Vec<PoseCase>, cols: u32) -> Result<(), Error> {
    let layout = Layout {
        cell_w: 420.0,
        cell_h: 380.0,
        zoom: 3.4,
        ground_line: false,
    };
    draw_cases_in(art, path, cases, cols, &layout)
}

fn draw_cases_in(
    art: &PetArt,
    path: &Path,
    cases: Vec<PoseCase>,
    cols: u32,
    layout: &Layout,
) -> Result<(), Error> {
    let Layout {
        cell_w,
        cell_h,
        zoom,
        ground_line,
    } = *layout;
    let rows = (cases.len() as u32).div_ceil(cols);
    let width = (cell_w * cols as f32) as u32;
    let height = (cell_h * rows as f32) as u32;
    let mut list = DisplayList::new(width, height);
    list.fill(
        rounded(Rect::new(0.0, 0.0, width as f32, height as f32), 0.0),
        Affine::IDENTITY,
        Paint::Linear {
            start: Point::new(0.0, 0.0),
            end: Point::new(0.0, f64::from(height)),
            stops: vec![
                Stop {
                    offset: 0.0,
                    color: rgba(244, 245, 250, 1.0),
                },
                Stop {
                    offset: 1.0,
                    color: rgba(226, 229, 240, 1.0),
                },
            ],
            extend: Extend::Pad,
        },
    );
    for (i, (pose, facing, shadow, particles)) in cases.into_iter().enumerate() {
        let col = i as u32 % cols;
        let row = i as u32 / cols;
        let ground_px = Vec2::new(
            col as f32 * cell_w + cell_w / 2.0,
            row as f32 * cell_h + cell_h - 70.0,
        );
        let lift = shadow.unwrap_or(0.0);
        let frame = Frame {
            origin: Vec2::new(-ground_px.x / zoom as f32, -ground_px.y / zoom as f32),
            scale: zoom,
        };
        let particles: Vec<Particle> = particles
            .into_iter()
            .map(|p| Particle {
                pos: p.pos + Vec2::new(0.0, -lift),
                ..p
            })
            .collect();
        let view = PetView {
            id: i as u32,
            ground: Vec2::new(0.0, -lift),
            facing,
            pose,
            shadow: shadow.map(|d| Shadow::on(0.0, d)),
            sprite_frame: None,
            sprite_offset_y: 0.0,
            opacity: 1.0,
            particles: &particles,
            support: Support::Air,
            behavior: "",
            grip: None,
        };
        paint_pet(art, &view, &frame, &Style::default(), &mut list);
        if ground_line {
            list.fill(
                rounded(
                    Rect::new(
                        col as f32 * cell_w + 12.0,
                        ground_px.y - lift * zoom as f32,
                        cell_w - 24.0,
                        1.0,
                    ),
                    0.0,
                ),
                Affine::IDENTITY,
                Paint::Solid(rgba(200, 80, 80, 0.55)),
            );
        }
    }
    rasterize(&list, path)
}

/// Le coucher de près, tourné à droite puis à gauche : il se couche
/// (l'avant d'abord ; il se relève par les mêmes poses, l'arrière
/// d'abord), couché qui rumine, endormi, endormi la tête au sol, une patte
/// qui tressaille. L'animal fait 220 pixels de haut, quelle que soit sa
/// taille ; une ligne marque le sol.
fn render_lying(art: &PetArt, path: &Path) -> Result<(), Error> {
    let base = PoseSignals {
        look: Vec2::new(0.35, 0.05),
        breath: 0.2,
        ..PoseSignals::default()
    };
    let asleep = PoseSignals {
        sit: 1.0,
        sleep: 1.0,
        blink: 1.0,
        ..base
    };
    let poses = [
        PoseSignals { sit: 0.25, ..base },
        PoseSignals { sit: 0.5, ..base },
        PoseSignals { sit: 0.75, ..base },
        PoseSignals { sit: 0.9, ..base },
        PoseSignals {
            sit: 1.0,
            jaw: 0.22,
            ..base
        },
        asleep,
        PoseSignals {
            sleep_low: 1.0,
            ..asleep
        },
        PoseSignals {
            sleep_low: 1.0,
            twitch: 1.0,
            ..asleep
        },
    ];
    let cases: Vec<PoseCase> = [1.0, -1.0]
        .into_iter()
        .flat_map(|facing| poses.map(|pose| (pose, facing, Some(0.0), vec![])))
        .collect();
    let zoom = 220.0 / f64::from(art.height.max(1.0));
    let layout = Layout {
        // Couché, il s'allonge : la tête posée en avant, la queue sur l'herbe.
        cell_w: (f64::from(art.width.max(art.height)) * zoom * 1.75) as f32,
        cell_h: 330.0,
        zoom,
        ground_line: true,
    };
    draw_cases_in(art, path, cases, 4, &layout)
}

/// L'encolure et ce qu'elle porte (laine, bavette, barbe, cloche), pas à
/// pas : debout, il baisse la tête pour brouter (première rangée) ; couché,
/// il somnole, puis pose la tête sur l'herbe (seconde rangée). Une ligne
/// marque le sol, par-dessus l'animal.
fn render_neck(art: &PetArt, path: &Path) -> Result<(), Error> {
    let base = PoseSignals {
        breath: 0.2,
        ..PoseSignals::default()
    };
    let lying = PoseSignals { sit: 1.0, ..base };
    let asleep = PoseSignals {
        sleep: 1.0,
        blink: 1.0,
        ..lying
    };
    let mut poses = Vec::new();
    for k in 0..=5 {
        poses.push(PoseSignals {
            graze: k as f32 / 5.0,
            ..base
        });
    }
    poses.push(PoseSignals {
        ears_back: -1.0,
        look: Vec2::new(0.4, -0.7),
        ..base
    });
    poses.push(PoseSignals {
        head_low: 1.0,
        ..base
    });
    poses.push(lying);
    poses.push(asleep);
    for k in 1..=6 {
        poses.push(PoseSignals {
            sleep_low: k as f32 / 6.0,
            ..asleep
        });
    }
    let cases: Vec<PoseCase> = poses
        .into_iter()
        .map(|pose| (pose, 1.0, Some(0.0), vec![]))
        .collect();
    let zoom = 200.0 / f64::from(art.height.max(1.0));
    let layout = Layout {
        cell_w: (f64::from(art.width.max(art.height)) * zoom * 1.7) as f32,
        cell_h: 300.0,
        zoom,
        ground_line: true,
    };
    draw_cases_in(art, path, cases, 8, &layout)
}

fn render_poses(art: &PetArt, path: &Path) -> Result<(), Error> {
    let bubbles = [
        Particle {
            kind: ParticleKind::Bubble,
            pos: Vec2::new(30.0, -70.0),
            vel: Vec2::ZERO,
            age: 1.0,
            life: 3.0,
            size: 4.0,
            phase: 0.0,
        },
        Particle {
            kind: ParticleKind::Bubble,
            pos: Vec2::new(44.0, -96.0),
            vel: Vec2::ZERO,
            age: 1.9,
            life: 3.0,
            size: 3.4,
            phase: 0.0,
        },
    ];
    let dust: Vec<Particle> = (0..8)
        .map(|i| {
            let side = if i % 2 == 0 { 1.0 } else { -1.0 };
            Particle {
                kind: ParticleKind::Dust,
                pos: Vec2::new(side * (18.0 + i as f32 * 5.0), -4.0 - (i % 3) as f32 * 3.0),
                vel: Vec2::ZERO,
                age: 0.18,
                life: 0.7,
                size: 4.0,
                phase: 0.0,
            }
        })
        .collect();

    let base = PoseSignals {
        look: Vec2::new(0.35, 0.05),
        ..PoseSignals::default()
    };
    let cases: Vec<PoseCase> = vec![
        (
            PoseSignals {
                breath: 0.4,
                ..base
            },
            1.0,
            Some(0.0),
            vec![],
        ),
        (
            PoseSignals {
                walk: 1.0,
                gait_phase: TAU * 0.2,
                lean: 0.03,
                ..base
            },
            1.0,
            Some(0.0),
            vec![],
        ),
        (
            PoseSignals {
                squash: 0.3,
                ear: 0.6,
                ..base
            },
            -1.0,
            Some(0.0),
            dust,
        ),
        (
            PoseSignals {
                air: 1.0,
                rise: 0.9,
                squash: -0.07,
                lean: 0.22,
                ear: -0.55,
                look: Vec2::new(0.6, 0.4),
                ..base
            },
            1.0,
            Some(20.0),
            vec![],
        ),
        (
            PoseSignals {
                dangle: 1.0,
                swing: 0.32,
                ear: 0.3,
                look: Vec2::new(0.2, 0.5),
                ..base
            },
            -1.0,
            Some(34.0),
            vec![],
        ),
        (
            PoseSignals {
                sit: 1.0,
                sleep: 1.0,
                blink: 1.0,
                breath: 0.8,
                ear: -0.25,
                ..base
            },
            1.0,
            Some(0.0),
            bubbles.to_vec(),
        ),
        // Couché, éveillé.
        (
            PoseSignals {
                sit: 1.0,
                breath: 0.3,
                ..base
            },
            1.0,
            Some(0.0),
            vec![],
        ),
        // Broute.
        (
            PoseSignals {
                graze: 1.0,
                nibble: 0.6,
                look: Vec2::new(0.2, 0.3),
                ..base
            },
            1.0,
            Some(0.0),
            vec![],
        ),
        // Galop, puis descente d'un saut (pattes tendues vers le sol).
        (
            PoseSignals {
                run: 1.0,
                walk: 1.0,
                gait_phase: TAU * 0.35,
                lean: 0.08,
                ..base
            },
            1.0,
            Some(0.0),
            vec![],
        ),
    ];

    draw_cases(art, path, cases, 3)
}

/// Les allures et les gestes animés par le moteur : trot, élan d'un énervé,
/// saut d'agneau, puis chaque geste à un moment parlant.
fn render_gestures(art: &PetArt, path: &Path) -> Result<(), Error> {
    let base = PoseSignals {
        look: Vec2::new(0.35, 0.05),
        breath: 0.2,
        ..PoseSignals::default()
    };
    let gesture = |g: Gesture, time: f32| PoseSignals {
        gesture: Some(g),
        gesture_amount: 1.0,
        gesture_time: time,
        jaw: match g {
            Gesture::Yawn => 1.0,
            Gesture::Bleat => 0.65,
            _ => 0.0,
        },
        ..base
    };
    let ground = |pose: PoseSignals| -> PoseCase { (pose, 1.0, Some(0.0), vec![]) };
    let cases: Vec<PoseCase> = vec![
        // Trot : une paire diagonale à l'appui, puis l'autre.
        ground(PoseSignals {
            walk: 1.0,
            trot: 1.0,
            stride_scale: 1.5,
            gait_phase: TAU * 0.05,
            ..base
        }),
        ground(PoseSignals {
            walk: 1.0,
            trot: 1.0,
            stride_scale: 1.5,
            gait_phase: TAU * 0.55,
            ..base
        }),
        // Il fonce, tête basse, oreilles couchées.
        ground(PoseSignals {
            walk: 1.0,
            run: 1.0,
            stride_scale: 2.2,
            gait_phase: TAU * 0.35,
            lean: 0.08,
            head_low: 1.0,
            ears_back: 0.9,
            ..base
        }),
        // Saut d'agneau : pattes raides.
        (
            PoseSignals {
                air: 1.0,
                rise: 0.6,
                stiff: 1.0,
                ..base
            },
            1.0,
            Some(26.0),
            vec![],
        ),
        // Saut tordu : le corps se tord, la tête part de l'autre côté, les
        // pattes de derrière ruent (deux moments de la torsion).
        (
            PoseSignals {
                air: 1.0,
                rise: 0.6,
                twist: 1.0,
                clock: 0.139,
                ..base
            },
            1.0,
            Some(26.0),
            vec![],
        ),
        (
            PoseSignals {
                air: 1.0,
                rise: 0.6,
                twist: 1.0,
                clock: 0.417,
                ..base
            },
            1.0,
            Some(26.0),
            vec![],
        ),
        ground(gesture(Gesture::Shake, 0.55)),
        ground(gesture(Gesture::Shake, 0.61)),
        ground(gesture(Gesture::Stretch, 0.75)),
        ground(gesture(Gesture::Stretch, 1.65)),
        // Il racle d'abord vers l'arrière, lève le sabot, le lance en avant,
        // racle encore.
        ground(gesture(Gesture::Paw, 0.26)),
        ground(gesture(Gesture::Paw, 0.5)),
        ground(gesture(Gesture::Stamp, 0.25)),
        ground(gesture(Gesture::Sniff, 0.6)),
        ground(gesture(Gesture::Daze, 0.5)),
        ground(gesture(Gesture::Brace, 0.8)),
        ground(gesture(Gesture::Yawn, 0.9)),
        ground(gesture(Gesture::Bleat, 0.4)),
        ground(gesture(Gesture::Scratch, 0.8)),
        ground(gesture(Gesture::Rear, 0.6)),
        // Le flehmen : encolure tendue, tête relevée, lèvre retroussée.
        ground(PoseSignals {
            jaw: 0.14,
            ..gesture(Gesture::Flehmen, 3.0)
        }),
        // Aux aguets (oreilles dressées), puis couché qui rumine (mâchoire).
        ground(PoseSignals {
            ears_back: -1.0,
            look: Vec2::new(0.8, -0.3),
            ..base
        }),
        ground(PoseSignals {
            sit: 1.0,
            jaw: 0.22,
            ..base
        }),
        // Endormi la tête au sol, puis une patte qui tressaille.
        ground(PoseSignals {
            sit: 1.0,
            sleep: 1.0,
            blink: 1.0,
            sleep_low: 1.0,
            ..base
        }),
        ground(PoseSignals {
            sit: 1.0,
            sleep: 1.0,
            blink: 1.0,
            sleep_low: 1.0,
            twitch: 1.0,
            ..base
        }),
    ];
    draw_cases(art, path, cases, 4)
}

/// Les allures phase par phase (le pas, le trot, le galop, huit instants
/// de la foulée chacun), puis il se couche (l'avant d'abord) et saute (il
/// replie les pattes, les tend vers le sol, atterrit).
fn render_gaits(art: &PetArt, path: &Path) -> Result<(), Error> {
    let base = PoseSignals {
        look: Vec2::new(0.35, 0.05),
        breath: 0.2,
        ..PoseSignals::default()
    };
    let ground = |pose: PoseSignals| -> PoseCase { (pose, 1.0, Some(0.0), vec![]) };
    let mut cases: Vec<PoseCase> = Vec::new();
    for (trot, run, stride) in [(0.0, 0.0, 1.0), (1.0, 0.0, 1.15), (0.0, 1.0, 1.9)] {
        for i in 0..8 {
            cases.push(ground(PoseSignals {
                walk: 1.0,
                trot,
                run,
                stride_scale: stride,
                gait_phase: TAU * i as f32 / 8.0,
                lean: 0.08 * run,
                ..base
            }));
        }
    }
    for sit in [0.25, 0.5, 0.75, 1.0] {
        cases.push(ground(PoseSignals { sit, ..base }));
    }
    for (rise, squash, lift) in [(0.9, -0.07, 24.0), (0.0, 0.0, 30.0), (-0.8, 0.0, 14.0)] {
        cases.push((
            PoseSignals {
                air: 1.0,
                rise,
                squash,
                ..base
            },
            1.0,
            Some(lift),
            vec![],
        ));
    }
    cases.push(ground(PoseSignals {
        squash: 0.16,
        ..base
    }));
    draw_cases(art, path, cases, 4)
}

/// La toison qui bouge, sur un vrai mouton simulé : au repos, au galop,
/// juste après un arrêt net (les mèches partent vers l'avant, puis
/// reviennent), pendant qu'il s'ébroue, puis un coup de vent dans le dos.
fn render_fleece(def: &Arc<mouflon_core::PetDef>, art: &PetArt, path: &Path) -> Result<(), Error> {
    let script = [(2.0, "stand"), (3.2, "run"), (4.6, "stand"), (5.5, "shake")];
    let shots = [1.8, 2.6, 3.1, 3.25, 3.4, 3.6, 3.9, 4.8, 4.86, 7.0];
    let cases = simulate(def, &script, 5.8, &shots);
    draw_cases(art, path, cases, 5)
}

/// Un vrai mouton simulé, tourné à droite, qui suit `script` (jusqu'à tel
/// instant, tel comportement ; il reste debout ensuite) ; un coup de vent le
/// prend de dos à l'instant `gust` ; ses poses aux instants `shots`.
fn simulate(
    def: &Arc<mouflon_core::PetDef>,
    script: &[(f32, &str)],
    gust: f32,
    shots: &[f32],
) -> Vec<PoseCase> {
    const DT: f32 = 1.0 / 120.0;
    let world = World::new(vec![screen()]);
    let mut flock = Flock::new();
    let id = flock.spawn(Arc::clone(def), 7, &world, Some(150.0));
    let input = Input {
        local_hour: 15.0,
        ..Input::default()
    };
    let mut cases: Vec<PoseCase> = Vec::new();
    let mut t = 0.0;
    let mut next = 0;
    let mut blown = false;
    while next < shots.len() {
        let mut wanted = "stand";
        let mut since = 0.0;
        for &(until, name) in script {
            if t < until {
                wanted = name;
                break;
            }
            since = until;
        }
        if t >= gust && !blown {
            flock.blow(1.0, 4.0);
            blown = true;
        }
        // Un geste n'est lancé qu'une fois, au début de sa place dans le script.
        if let Some(pet) = flock.get_mut(id)
            && pet.behavior_name() != wanted
            && (wanted != "shake" || t < since + 0.02)
        {
            pet.face(1.0);
            pet.force(wanted);
        }
        flock.advance(DT, &world, &input);
        t += DT;
        if t >= shots[next] {
            if let Some(pet) = flock.pets().iter().find(|p| p.id() == id) {
                let view = pet.view(1.0);
                cases.push((view.pose, view.facing, Some(0.0), vec![]));
            }
            next += 1;
        }
    }
    cases
}

/// La fenêtre d'un gros plan sur la tête et ce qui pend sous la gorge, en
/// points depuis le point d'appui de l'animal tourné à droite : de l'épaule
/// (un peu derrière la base de l'encolure) au bout du museau, du haut du
/// dessin à un peu sous le sol.
fn head_window(art: &PetArt) -> KRect {
    let (w, h) = (f64::from(art.width), f64::from(art.height));
    let Look::Rig(rig) = &art.look else {
        return KRect::new(0.1 * w, -1.1 * h, 0.9 * w, 0.12 * h);
    };
    let size = rig.bounds.height().max(1.0);
    let k = h / size;
    let g = rig.ground;
    let back = if rig.has_part(Part::Neck) {
        rig.pivot(Part::Neck).x
    } else {
        rig.part_bounds(Part::Head).min_x()
    };
    KRect::new(
        (back - 0.12 * size - g.x) * k,
        (rig.bounds.min_y() - 0.1 * size - g.y) * k,
        (rig.bounds.max_x() + 0.22 * size - g.x) * k,
        0.12 * size * k,
    )
}

/// Des gros plans sur la tête (voir [`head_window`]), `cols` par rangée,
/// chacun découpé à sa case, une ligne sur le sol (par-dessus l'animal : ce
/// qui passerait dessous se voit). Tourné à gauche, la fenêtre passe de
/// l'autre côté du point d'appui ; en l'air, elle suit l'animal.
fn draw_closeups(art: &PetArt, path: &Path, cases: Vec<PoseCase>, cols: u32) -> Result<(), Error> {
    let window = head_window(art);
    let zoom = 420.0 / window.height();
    let cell_w = (window.width() * zoom).ceil() as f32;
    let cell_h = (window.height() * zoom).ceil() as f32;
    let rows = (cases.len() as u32).div_ceil(cols);
    let width = (cell_w * cols as f32) as u32;
    let height = (cell_h * rows as f32) as u32;
    let mut list = DisplayList::new(width, height);
    list.fill(
        rounded(Rect::new(0.0, 0.0, width as f32, height as f32), 0.0),
        Affine::IDENTITY,
        Paint::Linear {
            start: Point::new(0.0, 0.0),
            end: Point::new(0.0, f64::from(height)),
            stops: vec![
                Stop {
                    offset: 0.0,
                    color: rgba(244, 245, 250, 1.0),
                },
                Stop {
                    offset: 1.0,
                    color: rgba(226, 229, 240, 1.0),
                },
            ],
            extend: Extend::Pad,
        },
    );
    for (i, (pose, facing, shadow, particles)) in cases.into_iter().enumerate() {
        let (x0, y0) = (
            (i as u32 % cols) as f32 * cell_w,
            (i as u32 / cols) as f32 * cell_h,
        );
        let lift = shadow.unwrap_or(0.0);
        // Pendant un demi-tour, le dessin se resserre autour du point d'appui.
        let squeeze = 0.34 + 0.66 * f64::from(facing.abs().min(1.0));
        let left = if facing < 0.0 {
            -window.x1 * squeeze
        } else {
            window.x0 * squeeze
        };
        // Le point d'appui de l'animal (en l'air, au-dessus du sol).
        let at = Vec2::new(x0 - (left * zoom) as f32, y0 - (window.y0 * zoom) as f32);
        let cell = rounded(Rect::new(x0, y0, cell_w, cell_h), 0.0);
        list.push_layer(cell, Affine::IDENTITY, 1.0, Mix::Normal);
        let particles: Vec<Particle> = particles
            .into_iter()
            .map(|p| Particle {
                pos: p.pos + Vec2::new(0.0, -lift),
                ..p
            })
            .collect();
        let view = PetView {
            id: i as u32,
            ground: Vec2::new(0.0, -lift),
            facing,
            pose,
            shadow: shadow.map(|d| Shadow::on(0.0, d)),
            sprite_frame: None,
            sprite_offset_y: 0.0,
            opacity: 1.0,
            particles: &particles,
            support: Support::Air,
            behavior: "",
            grip: None,
        };
        let frame = Frame {
            origin: Vec2::new(-at.x / zoom as f32, -lift - at.y / zoom as f32),
            scale: zoom,
        };
        paint_pet(art, &view, &frame, &Style::default(), &mut list);
        list.fill(
            rounded(Rect::new(x0, at.y + lift * zoom as f32, cell_w, 1.0), 0.0),
            Affine::IDENTITY,
            Paint::Solid(rgba(200, 80, 80, 0.6)),
        );
        list.pop_layer();
        list.fill(
            rounded(Rect::new(x0 + cell_w - 1.0, y0, 1.0, cell_h), 0.0),
            Affine::IDENTITY,
            Paint::Solid(rgba(150, 155, 175, 0.5)),
        );
        list.fill(
            rounded(Rect::new(x0, y0 + cell_h - 1.0, cell_w, 1.0), 0.0),
            Affine::IDENTITY,
            Paint::Solid(rgba(150, 155, 175, 0.5)),
        );
    }
    rasterize(&list, path)
}

/// La tête et la cloche de près, dans l'ordre, cinq par rangée : au repos
/// (aucun signal : le dessin tel quel), broute à mi-hauteur, broute à fond
/// (deux coups de dents, puis tourné à gauche) ; il se couche, couché la
/// tête haute, endormi, la tête qui descend, endormi la tête au sol (puis
/// tourné à gauche) ; au pas, au trot, quatre instants du galop ; trois
/// instants de l'ébrouement, tenu en l'air, au sommet d'un saut, un
/// demi-tour, et un mouton tourné à gauche qui galope.
fn render_bell(art: &PetArt, path: &Path) -> Result<(), Error> {
    let base = PoseSignals {
        look: Vec2::new(0.35, 0.05),
        breath: 0.2,
        ..PoseSignals::default()
    };
    let lying = PoseSignals { sit: 1.0, ..base };
    let asleep = PoseSignals {
        sleep: 1.0,
        blink: 1.0,
        ..lying
    };
    let gallop = |phase: f32| PoseSignals {
        walk: 1.0,
        run: 1.0,
        stride_scale: 1.9,
        gait_phase: TAU * phase,
        lean: 0.08,
        ..base
    };
    let shake = |time: f32| PoseSignals {
        gesture: Some(Gesture::Shake),
        gesture_amount: 1.0,
        gesture_time: time,
        ..base
    };
    let ground = |pose: PoseSignals, facing: f32| -> PoseCase { (pose, facing, Some(0.0), vec![]) };
    let cases: Vec<PoseCase> = vec![
        ground(PoseSignals::default(), 1.0),
        ground(PoseSignals { graze: 0.5, ..base }, 1.0),
        ground(
            PoseSignals {
                graze: 1.0,
                nibble: 1.0,
                ..base
            },
            1.0,
        ),
        ground(
            PoseSignals {
                graze: 1.0,
                nibble: -1.0,
                ..base
            },
            1.0,
        ),
        ground(
            PoseSignals {
                graze: 1.0,
                nibble: 1.0,
                ..base
            },
            -1.0,
        ),
        ground(PoseSignals { sit: 0.5, ..base }, 1.0),
        ground(lying, 1.0),
        ground(asleep, 1.0),
        ground(
            PoseSignals {
                sleep_low: 0.5,
                ..asleep
            },
            1.0,
        ),
        ground(
            PoseSignals {
                sleep_low: 1.0,
                ..asleep
            },
            1.0,
        ),
        ground(
            PoseSignals {
                sleep_low: 1.0,
                ..asleep
            },
            -1.0,
        ),
        ground(
            PoseSignals {
                walk: 1.0,
                gait_phase: TAU * 0.2,
                lean: 0.03,
                ..base
            },
            1.0,
        ),
        ground(
            PoseSignals {
                walk: 1.0,
                trot: 1.0,
                stride_scale: 1.15,
                gait_phase: TAU * 0.3,
                ..base
            },
            1.0,
        ),
        ground(gallop(0.1), 1.0),
        ground(gallop(0.35), 1.0),
        ground(gallop(0.6), 1.0),
        ground(gallop(0.85), 1.0),
        ground(shake(0.55), 1.0),
        ground(shake(0.61), 1.0),
        ground(shake(0.68), 1.0),
        (
            PoseSignals {
                dangle: 1.0,
                swing: 0.32,
                look: Vec2::new(0.2, 0.5),
                ..base
            },
            -1.0,
            Some(34.0),
            vec![],
        ),
        (
            PoseSignals {
                air: 1.0,
                rise: 0.9,
                squash: -0.07,
                lean: 0.22,
                ..base
            },
            1.0,
            Some(20.0),
            vec![],
        ),
        ground(base, 0.3),
        ground(gallop(0.35), -1.0),
    ];
    draw_closeups(art, path, cases, 5)
}

/// La tête et la cloche d'un vrai mouton simulé, cinq par rangée : debout,
/// il part au pas (trois instants), marche ; part au galop, galope ; s'arrête
/// net (cinq instants) ; s'ébroue (quatre instants) ; puis un coup de vent
/// le prend de dos (trois instants).
fn render_bell_motion(
    def: &Arc<mouflon_core::PetDef>,
    art: &PetArt,
    path: &Path,
) -> Result<(), Error> {
    let script = [
        (1.5, "stand"),
        (3.0, "walk"),
        (4.2, "run"),
        (5.6, "stand"),
        (6.5, "shake"),
    ];
    let shots = [
        1.4, 1.6, 1.75, 1.9, 2.6, 3.2, 4.0, 4.25, 4.35, 4.5, 4.7, 5.0, 5.7, 5.78, 5.86, 5.94, 7.0,
        7.4, 7.8, 8.6,
    ];
    let cases = simulate(def, &script, 6.8, &shots);
    draw_closeups(art, path, cases, 5)
}

/// Se remettre d'aplomb, de près sur les pattes : un pas en broutant, un
/// arrêt après le pas, un arrêt après le galop, un geste juste après
/// l'arrêt (gratter le sol, ou s'ébrouer ; le vrai moteur). Pour chacun,
/// une image par trentième de seconde, toutes posées au même endroit du sol
/// (des traits tous les 5 points, fixes dans le monde : un sabot posé reste
/// sur le même trait d'une image à l'autre), puis toutes superposées (un
/// sabot posé reste net, un sabot qui glisse laisse une traînée).
fn render_settle(
    def: &Arc<mouflon_core::PetDef>,
    art: &PetArt,
    dir: &Path,
) -> Result<Vec<(String, &'static str)>, Error> {
    const DT: f32 = 1.0 / 120.0;
    let wide = World::new(vec![Screen {
        frame: Rect::new(0.0, 0.0, 40000.0, 600.0),
        usable: Rect::new(0.0, 0.0, 40000.0, 560.0),
        scale: 2.0,
    }]);
    let input = Input {
        local_hour: 15.0,
        ..Input::default()
    };
    // Chaque comportement dure le temps qu'on lui donne.
    let mut lasting = (**def).clone();
    for behavior in &mut lasting.behaviors {
        if matches!(behavior.duration, mouflon_core::def::Duration::Range(..)) {
            behavior.duration = mouflon_core::def::Duration::Range(600.0, 600.0);
        }
    }
    let lasting = Arc::new(lasting);
    let gesture = ["paw", "shake"]
        .into_iter()
        .find(|g| def.behavior_index(g).is_some())
        .unwrap_or("stand");
    /// Une planche : le comportement forcé jusqu'à tel instant (il tombe
    /// d'abord du haut de l'écran), et la fenêtre filmée.
    struct Scene<'a> {
        name: &'static str,
        what: &'static str,
        script: Vec<(&'a str, f32)>,
        window: (f32, f32),
    }
    let scenes = [
        Scene {
            name: "aplomb-broute.png",
            what: "un pas en broutant",
            script: vec![("stand", 2.0), ("graze", 12.0)],
            window: (9.3, 11.6),
        },
        Scene {
            name: "aplomb-pas.png",
            what: "arrêt après le pas",
            script: vec![("stand", 2.0), ("walk", 4.0), ("stand", 6.0)],
            window: (3.75, 5.55),
        },
        Scene {
            name: "aplomb-galop.png",
            what: "arrêt après le galop",
            script: vec![("stand", 2.0), ("run", 4.4), ("stand", 6.0)],
            window: (4.2, 6.0),
        },
        Scene {
            name: "aplomb-geste.png",
            what: "un geste juste après l'arrêt",
            script: vec![
                ("stand", 2.0),
                ("walk", 4.0),
                (gesture, 6.5),
                ("stand", 7.5),
            ],
            window: (3.9, 5.0),
        },
    ];
    let mut written = Vec::new();
    for Scene {
        name,
        what,
        script,
        window: (t0, t1),
    } in scenes
    {
        let mut pet = Pet::spawn(0, Arc::clone(&lasting), 7, &wide, Some(20000.0));
        pet.face(1.0);
        let mut shots: Vec<(PoseSignals, f32, f32)> = Vec::new();
        let mut t = 0.0f32;
        let mut tick = 0u32;
        for (behavior, until) in &script {
            while t < *until {
                if pet.behavior_name() != *behavior {
                    pet.force(behavior);
                }
                pet.step(DT, &wide, &input);
                t += DT;
                tick += 1;
                if t >= t0 && t <= t1 && tick.is_multiple_of(4) {
                    let view = pet.view(1.0);
                    shots.push((view.pose, view.facing, view.ground.x));
                }
            }
        }
        draw_settle(art, &dir.join(name), &shots)?;
        written.push((name.to_owned(), what));
    }
    Ok(written)
}

fn draw_settle(art: &PetArt, path: &Path, shots: &[(PoseSignals, f32, f32)]) -> Result<(), Error> {
    const ZOOM: f32 = 4.0;
    const COLS: usize = 8;
    let Some(&(_, _, still)) = shots.last() else {
        return Ok(());
    };
    // Ce qu'on voit : les pattes et le bas du corps, de part et d'autre du
    // corps arrêté, et assez derrière pour le voir arriver.
    let behind = shots
        .iter()
        .map(|s| (s.2 - still).abs())
        .fold(0.0f32, f32::max);
    let (left, right) = (-0.62 * art.width - behind, 0.62 * art.width);
    let (above, below) = (0.58 * art.height, 4.0);
    let cell_w = (right - left) * ZOOM;
    let cell_h = (above + below) * ZOOM;
    let rows = shots.len().div_ceil(COLS);
    let width = (cell_w * COLS as f32) as u32;
    let onion_h = cell_h + 8.0;
    let height = (onion_h + cell_h * rows as f32) as u32;
    let mut list = DisplayList::new(width, height);
    list.fill(
        rounded(Rect::new(0.0, 0.0, width as f32, height as f32), 0.0),
        Affine::IDENTITY,
        Paint::Solid(rgba(246, 247, 251, 1.0)),
    );
    let facing = shots.last().map_or(1.0, |s| s.1);
    // Une case : son coin, les traits fixes du sol, puis les images.
    let cell =
        |list: &mut DisplayList, x: f32, y: f32, frames: &[(PoseSignals, f32, f32)], alpha: f32| {
            let clip = rounded(Rect::new(x, y, cell_w - 2.0, cell_h - 2.0), 0.0);
            list.push_layer(
                clip,
                Affine::IDENTITY,
                1.0,
                mouflon_render::peniko::Mix::Normal,
            );
            let ground_y = y + above * ZOOM;
            // Un trait tous les 5 points, fixe dans le monde.
            let first = ((still + left) / 5.0).floor() as i32;
            let last = ((still + right) / 5.0).ceil() as i32;
            for k in first..=last {
                let wx = k as f32 * 5.0 - still;
                let px = x + (if facing < 0.0 { -wx } else { wx } - left) * ZOOM;
                list.fill(
                    rounded(Rect::new(px, ground_y - 6.0 * ZOOM, 1.0, 9.0 * ZOOM), 0.0),
                    Affine::IDENTITY,
                    Paint::Solid(rgba(120, 130, 170, if k % 2 == 0 { 0.35 } else { 0.18 })),
                );
            }
            let n = frames.len();
            for (i, (pose, facing, gx)) in frames.iter().enumerate() {
                let offset = gx - still;
                let px = x + (offset * if *facing < 0.0 { -1.0 } else { 1.0 } - left) * ZOOM;
                let frame = Frame {
                    origin: Vec2::new(-px / ZOOM, -ground_y / ZOOM),
                    scale: f64::from(ZOOM),
                };
                let view = PetView {
                    id: 0,
                    ground: Vec2::new(0.0, 0.0),
                    facing: *facing,
                    pose: *pose,
                    shadow: None,
                    sprite_frame: None,
                    sprite_offset_y: 0.0,
                    opacity: if i + 1 == n { 1.0 } else { alpha },
                    particles: &[],
                    support: Support::Air,
                    behavior: "",
                    grip: None,
                };
                paint_pet(art, &view, &frame, &Style::default(), list);
            }
            list.fill(
                rounded(Rect::new(x, ground_y, cell_w, 1.0), 0.0),
                Affine::IDENTITY,
                Paint::Solid(rgba(200, 80, 80, 0.6)),
            );
            list.pop_layer();
        };
    // En haut, toutes les images superposées ; puis une par case.
    cell(&mut list, 0.0, 0.0, shots, 0.12);
    for (i, shot) in shots.iter().enumerate() {
        let (col, row) = (i % COLS, i / COLS);
        cell(
            &mut list,
            col as f32 * cell_w,
            onion_h + row as f32 * cell_h,
            std::slice::from_ref(shot),
            1.0,
        );
    }
    rasterize(&list, path)
}

// ---------------------------------------------------------------- export

fn rasterize(list: &DisplayList, path: &Path) -> Result<(), Error> {
    let svg = to_svg(list);
    let tree = usvg::Tree::from_str(&svg, &usvg::Options::default())?;
    let mut pixmap =
        tiny_skia::Pixmap::new(list.width, list.height).ok_or("taille d'image invalide")?;
    resvg::render(
        &tree,
        tiny_skia::Transform::identity(),
        &mut pixmap.as_mut(),
    );
    pixmap.save_png(path)?;
    Ok(())
}
