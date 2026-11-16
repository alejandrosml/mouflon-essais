//! Plusieurs écrans, rangés de toutes les façons : côte à côte, l'un sur
//! l'autre, décalés, en escalier, avec un trou entre eux, des barres des
//! tâches de hauteurs différentes, un Dock sur un seul. Où qu'ils soient,
//! les animaux restent là où un écran les montre : ils ne marchent ni ne
//! volent vers un endroit que personne ne voit, y sont ramenés si on les y
//! lâche, reviennent quand leur écran disparaît, et arrivent sur tous les
//! écrans. Et ils voyagent d'eux-mêmes de l'un à l'autre, d'un grand bond ou
//! d'un saut, quand les sols ne se rejoignent pas.

use std::path::Path;
use std::sync::Arc;

use mouflon_core::math::Rng;
use mouflon_core::{
    Flock, Input, Pet, PetDef, Rect, Screen, Sex, Support, TICK, Vec2, VoyageWay, World, load_pet,
    parse_pet_toml,
};

/// Un écran dont la barre des menus prend `menu` points en haut, et la
/// barre des tâches (ou le Dock) `bar` points en bas.
fn screen(x: f32, y: f32, w: f32, h: f32, menu: f32, bar: f32) -> Screen {
    Screen {
        frame: Rect::new(x, y, w, h),
        usable: Rect::new(x, y + menu, w, h - menu - bar),
        scale: 1.0,
    }
}

/// Les dispositions essayées, par leur nom.
fn layouts() -> Vec<(&'static str, Vec<Screen>)> {
    vec![
        (
            "un seul écran",
            vec![screen(0.0, 0.0, 1440.0, 900.0, 25.0, 40.0)],
        ),
        (
            "deux écrans pareils côte à côte",
            vec![
                screen(0.0, 0.0, 1440.0, 900.0, 25.0, 40.0),
                screen(1440.0, 0.0, 1440.0, 900.0, 25.0, 40.0),
            ],
        ),
        (
            "un petit et un grand, alignés en haut",
            vec![
                screen(0.0, 0.0, 1280.0, 800.0, 0.0, 40.0),
                screen(1280.0, 0.0, 1920.0, 1200.0, 0.0, 40.0),
            ],
        ),
        (
            "alignés en bas, des barres des tâches de deux hauteurs",
            vec![
                screen(1024.0, 0.0, 1536.0, 1080.0, 0.0, 48.0),
                screen(0.0, 280.0, 1024.0, 800.0, 0.0, 38.4),
            ],
        ),
        (
            "un écran au-dessus du portable",
            vec![
                screen(0.0, 0.0, 1536.0, 864.0, 0.0, 48.0),
                screen(128.0, -1024.0, 1280.0, 1024.0, 0.0, 38.4),
            ],
        ),
        (
            "deux écrans pareils l'un sur l'autre, sans barre en haut",
            vec![
                screen(0.0, 0.0, 1280.0, 800.0, 0.0, 40.0),
                screen(0.0, -800.0, 1280.0, 800.0, 0.0, 0.0),
            ],
        ),
        (
            // Les trois écrans des photos du 10 novembre 2026.
            "un portable, un écran au-dessus, un autre à gauche",
            vec![
                screen(0.0, 0.0, 1536.0, 864.0, 0.0, 48.0),
                screen(-1024.0, -300.0, 1024.0, 819.2, 0.0, 38.4),
                screen(200.0, -819.2, 1024.0, 819.2, 0.0, 38.4),
            ],
        ),
        (
            "quatre écrans en carré",
            vec![
                screen(0.0, 0.0, 1000.0, 700.0, 0.0, 40.0),
                screen(1000.0, 0.0, 1000.0, 700.0, 0.0, 40.0),
                screen(0.0, 700.0, 1000.0, 700.0, 0.0, 40.0),
                screen(1000.0, 700.0, 1000.0, 700.0, 0.0, 40.0),
            ],
        ),
        (
            "quatre écrans en escalier",
            vec![
                screen(0.0, 0.0, 800.0, 600.0, 0.0, 40.0),
                screen(800.0, 150.0, 800.0, 600.0, 0.0, 40.0),
                screen(1600.0, 300.0, 800.0, 600.0, 0.0, 0.0),
                screen(2400.0, 450.0, 800.0, 600.0, 0.0, 40.0),
            ],
        ),
        (
            "un trou entre deux écrans",
            vec![
                screen(0.0, 0.0, 1000.0, 700.0, 0.0, 40.0),
                screen(1100.0, 100.0, 1000.0, 700.0, 0.0, 40.0),
            ],
        ),
        (
            "Mac : le Dock sur un seul écran, une barre des menus sur chacun",
            vec![
                screen(0.0, 0.0, 1728.0, 1117.0, 38.0, 80.0),
                screen(1728.0, -200.0, 2560.0, 1440.0, 25.0, 0.0),
            ],
        ),
        (
            "une barre des tâches sur le côté, entre deux écrans",
            vec![
                Screen {
                    frame: Rect::new(0.0, 0.0, 1280.0, 1024.0),
                    usable: Rect::new(0.0, 0.0, 1218.0, 1024.0),
                    scale: 1.0,
                },
                screen(1280.0, 0.0, 1280.0, 1024.0, 0.0, 40.0),
            ],
        ),
        (
            "tout à gauche et au-dessus de l'écran principal",
            vec![
                screen(0.0, 0.0, 1280.0, 720.0, 0.0, 40.0),
                screen(-1920.0, -1080.0, 1920.0, 1080.0, 0.0, 40.0),
            ],
        ),
        (
            "deux écrans en miroir et un troisième",
            vec![
                screen(0.0, 0.0, 1280.0, 720.0, 0.0, 40.0),
                screen(0.0, 0.0, 1280.0, 720.0, 0.0, 40.0),
                screen(1280.0, -200.0, 1280.0, 1024.0, 0.0, 40.0),
            ],
        ),
    ]
}

/// La disposition dont le nom contient `part`.
fn layout(part: &str) -> (&'static str, Vec<Screen>) {
    layouts()
        .into_iter()
        .find(|(name, _)| name.contains(part))
        .expect("disposition")
}

/// Quelques races du dépôt : une de plaine, un agneau, un sauteur, la
/// mascotte.
fn breeds() -> Vec<Arc<PetDef>> {
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../pets");
    [
        "bleu-du-maine",
        "southdown",
        "mouflon-corse",
        "merinos-d-arles",
        "ouessant",
    ]
    .iter()
    .map(|name| Arc::new(load_pet(&root.join(name).join("pet.toml")).expect("animal")))
    .collect()
}

/// Un mouton d'essai : il marche tout droit, fait demi-tour au bord, saute
/// sur ce qui est à sa portée, descend d'un rebord quand on le lui dit.
const WALKER: &str = r#"
    [pet]
    id = "marcheur"
    name = "Marcheur"
    [appearance]
    rig = "a.svg"
    height = 45.0
    [physics]
    width = 57.5
    stride = 28.0
    bounce = 0.0
    [reflexes]
    spawn = "fall"
    fall = "fall"
    land = "stand"
    thrown = "fall"
    held = "held"
    edge = "turn"
    wall = "turn"
    window_side = [{ to = "turn", weight = 1 }, { to = "leap", weight = 1000 }]
    [[behavior]]
    name = "stand"
    motion = "stand"
    duration = 0.5
    next = "walk"
    [[behavior]]
    name = "walk"
    motion = "walk"
    speed = 60.0
    duration = "forever"
    [[behavior]]
    name = "off"
    motion = "walk"
    speed = 60.0
    duration = "forever"
    walk_off_edges = true
    on_edge = "off"
    [[behavior]]
    name = "turn"
    motion = "turn"
    duration = 0.2
    next = "walk"
    [[behavior]]
    name = "leap"
    motion = "leap"
    reach = 2.0
    duration = "forever"
    on_land = "walk"
    [[behavior]]
    name = "held"
    motion = "held"
    duration = "forever"
    [[behavior]]
    name = "fall"
    motion = "fall"
    duration = "forever"
"#;

fn walker() -> Arc<PetDef> {
    Arc::new(parse_pet_toml(WALKER, |_: &str| Ok(b"<svg/>".to_vec())).unwrap())
}

/// La part du rectangle `r` que montrent les écrans (0 à 1).
fn shown(screens: &[Screen], r: Rect) -> f32 {
    let mut xs = vec![r.left(), r.right()];
    let mut ys = vec![r.top(), r.bottom()];
    for s in screens {
        xs.extend([s.frame.left(), s.frame.right()]);
        ys.extend([s.frame.top(), s.frame.bottom()]);
    }
    xs.sort_by(f32::total_cmp);
    ys.sort_by(f32::total_cmp);
    let mut area = 0.0;
    for x in xs.windows(2) {
        for y in ys.windows(2) {
            let mid = Vec2::new((x[0] + x[1]) * 0.5, (y[0] + y[1]) * 0.5);
            if r.contains(mid) && screens.iter().any(|s| s.frame.contains(mid)) {
                area += (x[1] - x[0]) * (y[1] - y[0]);
            }
        }
    }
    area / (r.w * r.h).max(1e-6)
}

/// Le corps d'un animal debout : sa boîte, un point de moins de chaque côté
/// (il a le droit de toucher le bord).
fn body(pet: &Pet) -> Rect {
    let p = pet.def().physics;
    let at = pet.position();
    Rect::new(
        at.x - p.width * 0.5 + 1.0,
        at.y - p.height + 1.0,
        p.width - 2.0,
        p.height - 2.0,
    )
}

fn find(flock: &Flock, id: u32) -> &Pet {
    flock.pets().iter().find(|p| p.id() == id).unwrap()
}

fn floor_of(world: &World, screen: usize) -> mouflon_core::Surface {
    let s = world.screens()[screen];
    *world
        .floor_at(s.usable.center().x, s.usable.bottom())
        .expect("sol")
}

/// Un mouton d'essai posé sur le sol de l'écran `screen`, à l'abscisse `x`,
/// qui marche dans le sens `dir` en faisant `behavior`.
fn walking(
    world: &World,
    screen: usize,
    x: f32,
    dir: f32,
    behavior: &str,
    realistic: bool,
) -> (Flock, u32) {
    let mut flock = Flock::new();
    flock.set_realistic(realistic);
    let floor = floor_of(world, screen);
    let id = flock.spawn(walker(), 7, world, Some(x));
    let pet = flock.get_mut(id).unwrap();
    pet.grab(Vec2::new(x, floor.y - 20.0), world);
    pet.drag_to(Vec2::new(x, floor.y - 20.0));
    for _ in 0..12 {
        flock.advance(TICK, world, &Input::default());
    }
    flock.get_mut(id).unwrap().release(world);
    for _ in 0..(2.0 / TICK) as usize {
        flock.advance(TICK, world, &Input::default());
    }
    let pet = flock.get_mut(id).unwrap();
    assert_eq!(
        pet.support(),
        Support::Ground { surface: floor.id },
        "posé sur le sol de l'écran {screen} : {:?}",
        pet.position()
    );
    pet.face(dir);
    assert!(pet.force(behavior));
    (flock, id)
}

#[test]
fn new_pets_arrive_on_every_screen_from_the_top_of_their_own() {
    for (name, screens) in layouts() {
        let world = World::new(screens.clone());
        let mut flock = Flock::new();
        let def = walker();
        let mut counts = vec![0u32; screens.len()];
        for seed in 0..600 {
            let id = flock.spawn(Arc::clone(&def), seed, &world, None);
            let at = find(&flock, id).position();
            // Tout en haut de la zone utile d'un écran, entre ses bords.
            let home = screens.iter().position(|s| {
                at.x >= s.usable.left()
                    && at.x <= s.usable.right()
                    && (at.y - (s.usable.top() + def.physics.height + 2.0)).abs() < 0.01
            });
            let home = home.unwrap_or_else(|| panic!("{name} : arrivé en {at:?}, nulle part"));
            counts[home] += 1;
            assert!(
                shown(&screens, body(find(&flock, id))) > 0.999,
                "{name} : arrivé coupé en {at:?}"
            );
        }
        // Chaque écran en reçoit (ceux en miroir comptent pour un seul).
        for (i, s) in screens.iter().enumerate() {
            let twin = screens[..i].iter().any(|t| t.frame == s.frame);
            let same: u32 = screens
                .iter()
                .zip(&counts)
                .filter(|(t, _)| t.frame == s.frame)
                .map(|(_, c)| *c)
                .sum();
            assert!(twin || same >= 40, "{name} : écran {i} : {counts:?}");
        }
    }
}

#[test]
fn a_pet_asked_for_on_a_screen_arrives_on_that_screen() {
    for (name, screens) in layouts() {
        let world = World::new(screens.clone());
        for (i, s) in screens.iter().enumerate() {
            for seed in 0..40 {
                let pet = Pet::spawn_near(
                    seed as u32,
                    walker(),
                    seed,
                    &world,
                    s.frame.center(),
                    None,
                    None,
                );
                let at = pet.position();
                assert!(
                    at.x > s.usable.left() && at.x < s.usable.right(),
                    "{name} : écran {i}, arrivé en {at:?}"
                );
                assert!((at.y - (s.usable.top() + 47.0)).abs() < 0.01, "{name}");
            }
        }
    }
}

/// Une vie mouvementée sur une disposition : un troupeau, quelqu'un qui
/// attrape les animaux et les lance n'importe où (le curseur, lui, ne
/// quitte jamais les écrans), du vent, des soucoupes. Renvoie ce qui ne
/// devrait pas arriver.
fn live(name: &str, screens: &[Screen], seed: u64, seconds: f32, realistic: bool) -> Vec<String> {
    let defs = breeds();
    let mut rng = Rng::new(seed);
    let world = World::new(screens.to_vec());
    let mut flock = Flock::new();
    flock.set_realistic(realistic);
    flock.set_collisions(rng.chance(0.3));
    // Des voyages d'un écran à l'autre bien plus souvent qu'en vrai : ce
    // sont eux qui passent par les endroits délicats.
    flock.set_voyage_every(Some((4.0, 25.0)));
    for _ in 0..24 {
        let def = &defs[rng.next_u32() as usize % defs.len()];
        flock.spawn(Arc::clone(def), u64::from(rng.next_u32()), &world, None);
    }
    let on_a_screen = |rng: &mut Rng| {
        let s = screens[rng.next_u32() as usize % screens.len()].frame;
        Vec2::new(
            rng.range(s.left() + 1.0, s.right() - 1.0),
            rng.range(s.top() + 1.0, s.bottom() - 1.0),
        )
    };
    let mut input = Input {
        cursor: Some(on_a_screen(&mut rng)),
        local_hour: 15.0,
        since_key: None,
        idle: Some(1.0),
    };
    let mut faults = Vec::new();
    // L'animal tenu : jusqu'à quel pas, vers où.
    let mut held: Option<(u32, usize, Vec2, Vec2)> = None;
    // Depuis quel pas chacun n'a plus été touché (tenu, lâché, soucoupe).
    let mut quiet: std::collections::HashMap<u32, usize> = std::collections::HashMap::new();
    // Depuis combien de pas chacun est en l'air, hors des écrans, coupé.
    let mut airborne: std::collections::HashMap<u32, usize> = std::collections::HashMap::new();
    let mut away: std::collections::HashMap<u32, usize> = std::collections::HashMap::new();
    let mut cut: std::collections::HashMap<u32, usize> = std::collections::HashMap::new();
    // Ce qui ne dure pas n'est pas un défaut : un animal qui roule en se
    // posant contre un bord, ou perché sur le dos d'un autre qui s'y tient,
    // en dépasse un instant.
    let lasting = (2.0 / TICK) as usize;
    let trace = std::env::var("MOUFLON_TRACE").ok().and_then(|v| {
        let mut parts = v.split(',').map(|x| x.trim().parse::<u64>().ok());
        let (s, pet, step) = (parts.next()??, parts.next()??, parts.next()??);
        (s == seed).then_some((pet as u32, step as usize))
    });
    let total = (seconds / TICK) as usize;
    let settle = (3.0 / TICK) as usize;
    let mut next_action = 0;
    for step in 0..total {
        if step >= next_action {
            next_action = step + (rng.range(0.1, 2.0) / TICK) as usize;
            let ids: Vec<u32> = flock.pets().iter().map(Pet::id).collect();
            let pick = ids[rng.next_u32() as usize % ids.len()];
            match rng.next_u32() % 10 {
                0..=4 if held.is_none() => {
                    if let Some(pet) = flock.get_mut(pick) {
                        let from = pet.position() - Vec2::new(0.0, 10.0);
                        pet.grab(from, &world);
                        let until = step + (rng.range(0.1, 1.2) / TICK) as usize;
                        held = Some((pick, until, from, on_a_screen(&mut rng)));
                    }
                }
                5 => flock.blow(
                    if rng.chance(0.5) { 1.0 } else { -1.0 },
                    rng.range(1.0, 6.0),
                ),
                6 => {
                    flock.invite_saucer(&world, u64::from(rng.next_u32()));
                }
                7 => {
                    flock.regroup();
                }
                _ => input.cursor = Some(on_a_screen(&mut rng)),
            }
        }
        // Celui qu'on tient va tout droit vers un point d'un écran : en
        // chemin, la main peut survoler un endroit sans écran (le curseur
        // de Windows, lui, le contournerait), mais on ne lâche que sur un
        // écran.
        if let Some((id, until, from, to)) = held {
            quiet.insert(id, step);
            match flock.get_mut(id) {
                Some(pet) if step < until && pet.is_held() => {
                    let t = 1.0 - (until - step) as f32 * TICK / 1.2;
                    let at = from.lerp(to, t.clamp(0.0, 1.0));
                    input.cursor = Some(at);
                    pet.drag_to(at);
                }
                Some(pet) => {
                    if pet.is_held() {
                        pet.drag_to(to);
                        pet.release(&world);
                    }
                    held = None;
                }
                None => held = None,
            }
        }
        for saucer in flock.saucers() {
            quiet.insert(saucer.target(), step);
        }
        flock.advance(TICK, &world, &input);
        for pet in flock.pets() {
            let (id, at) = (pet.id(), pet.position());
            let who = format!(
                "{name} (graine {seed}, {}) pas {step} : {} n° {id} {} {:?} en {at:?}",
                if realistic { "réaliste" } else { "classique" },
                pet.def().meta.id,
                pet.behavior_name(),
                pet.support(),
            );
            if let Some((traced, around)) = trace
                && traced == id
                && step + 400 > around
                && step < around + 20
            {
                eprintln!(
                    "  {who}, vitesse {:?}, tenu {}, au calme depuis {:?}",
                    pet.velocity(),
                    pet.is_held(),
                    quiet.get(&id).map(|at| step - at)
                );
            }
            if !(at.x.is_finite() && at.y.is_finite()) {
                faults.push(format!("{who} : pas un nombre"));
                continue;
            }
            if pet.is_held() || pet.is_child() {
                quiet.insert(id, step);
                airborne.remove(&id);
                away.remove(&id);
                cut.remove(&id);
                continue;
            }
            // En l'air sans fin ? (Comme l'essai d'endurance : un animal
            // coincé entre un bord et le dos d'un autre peut y rester un
            // moment, jusqu'à ce que l'autre bouge.)
            let up = airborne.entry(id).or_insert(0);
            if pet.support() == Support::Air {
                *up += 1;
                if *up == (45.0 / TICK) as usize {
                    faults.push(format!("{who} : en l'air depuis 45 secondes"));
                }
            } else {
                *up = 0;
            }
            // Laissé tranquille depuis trois secondes : un écran le montre,
            // et le montre en entier s'il est posé.
            if quiet.get(&id).is_some_and(|at| step < at + settle) {
                away.remove(&id);
                cut.remove(&id);
                continue;
            }
            let middle = at - Vec2::new(0.0, pet.def().physics.height * 0.5);
            let gone = away.entry(id).or_insert(0);
            *gone = if world.on_screens(middle) {
                0
            } else {
                *gone + 1
            };
            if *gone == lasting {
                faults.push(format!("{who} : là où il n'y a pas d'écran"));
            }
            let part = shown(screens, body(pet));
            let clipped = cut.entry(id).or_insert(0);
            *clipped = if matches!(pet.support(), Support::Ground { .. }) && part < 0.97 {
                *clipped + 1
            } else {
                0
            };
            if *clipped == lasting {
                faults.push(format!("{who} : coupé (montré à {:.0} %)", part * 100.0));
            }
        }
        if faults.len() > 8 {
            break;
        }
    }
    faults
}

#[test]
fn whatever_the_layout_pets_stay_where_a_screen_shows_them() {
    let mut faults = Vec::new();
    for (name, screens) in layouts() {
        for (seed, realistic) in [(1, true), (2, false), (3, true)] {
            faults.extend(live(name, &screens, seed, 40.0, realistic));
        }
    }
    assert!(faults.is_empty(), "{}", faults.join("\n"));
}

/// La version longue, à la main :
/// `cargo test --release -p mouflon-core --test ecrans -- --ignored --nocapture`
#[test]
#[ignore = "long : dix minutes simulées par disposition"]
fn a_long_life_on_every_layout() {
    let mut faults = Vec::new();
    for (name, screens) in layouts() {
        for seed in 10..14 {
            faults.extend(live(name, &screens, seed, 600.0, seed % 2 == 0));
        }
        println!("{name} : {} défaut(s) jusqu'ici", faults.len());
    }
    assert!(faults.is_empty(), "{}", faults.join("\n"));
}

#[test]
fn a_sheep_walks_across_level_floors_and_small_steps_both_ways() {
    // Des sols de plain-pied, puis deux barres des tâches de 48 et de
    // 38,4 points (125 % et 100 %) au bas de deux écrans alignés en bas.
    for part in ["pareils côte à côte", "alignés en bas"] {
        let (name, screens) = layout(part);
        let world = World::new(screens.clone());
        for realistic in [false, true] {
            for (from, to) in [(0, 1), (1, 0)] {
                let (a, b) = (screens[from], screens[to]);
                let dir = (b.frame.center().x - a.frame.center().x).signum();
                let edge = if dir > 0.0 {
                    a.frame.right()
                } else {
                    a.frame.left()
                };
                let (mut flock, id) =
                    walking(&world, from, edge - dir * 200.0, dir, "walk", realistic);
                let mut lowest = 1.0f32;
                for _ in 0..(8.0 / TICK) as usize {
                    flock.advance(TICK, &world, &Input::default());
                    lowest = lowest.min(shown(&screens, body(find(&flock, id))));
                }
                let pet = find(&flock, id);
                assert_eq!(
                    pet.support(),
                    Support::Ground {
                        surface: floor_of(&world, to).id
                    },
                    "{name}, de {from} à {to}, réaliste {realistic} : {:?}",
                    pet.position()
                );
                assert!(lowest > 0.99, "{name} : coupé en chemin ({lowest})");
            }
        }
    }
}

#[test]
fn walking_into_a_higher_floor_next_door_it_turns_back_unless_it_is_within_a_leap() {
    // À gauche, un sol 30 points plus haut (à sa portée : 35 points) ; puis
    // 120 points plus haut (hors de portée : il lui faudra l'envie de
    // voyager, et un grand bond).
    for (rise, reachable) in [(30.0, true), (120.0, false)] {
        let screens = vec![
            screen(1000.0, 0.0, 1000.0, 800.0, 0.0, 0.0),
            screen(0.0, 0.0, 1000.0, 800.0, 0.0, rise),
        ];
        let world = World::new(screens.clone());
        for realistic in [false, true] {
            let (mut flock, id) = walking(&world, 0, 1200.0, -1.0, "walk", realistic);
            let (mut leapt, mut turned) = (false, false);
            let mut lowest = 1.0f32;
            for _ in 0..(9.0 / TICK) as usize {
                flock.advance(TICK, &world, &Input::default());
                let pet = find(&flock, id);
                leapt |= pet.behavior_name() == "leap";
                turned |= pet.behavior_name() == "turn";
                lowest = lowest.min(shown(&screens, body(pet)));
                if !reachable {
                    assert!(
                        pet.position().x >= 1000.0 + 57.5 * 0.5 - 0.5,
                        "passé à travers"
                    );
                }
            }
            let pet = find(&flock, id);
            let tag = format!(
                "marche de {rise}, réaliste {realistic} : {:?}",
                pet.position()
            );
            if reachable {
                assert!(leapt, "{tag}");
                assert_eq!(
                    pet.support(),
                    Support::Ground {
                        surface: floor_of(&world, 1).id
                    },
                    "{tag}"
                );
            } else {
                assert!(turned && !leapt, "{tag}");
                assert_eq!(
                    pet.support(),
                    Support::Ground {
                        surface: floor_of(&world, 0).id
                    },
                    "{tag}"
                );
            }
            assert!(lowest > 0.99, "{tag} : coupé ({lowest})");
        }
    }
}

#[test]
fn a_lower_floor_next_door_is_reached_by_walking_off_the_edge() {
    // À gauche, un écran dont le sol est 300 points plus haut : il en
    // descend en marchant jusqu'au bout, et tombe sur le sol d'à côté.
    let screens = vec![
        screen(0.0, 0.0, 1000.0, 500.0, 0.0, 0.0),
        screen(1000.0, 0.0, 1000.0, 800.0, 0.0, 0.0),
    ];
    let world = World::new(screens.clone());
    for realistic in [false, true] {
        // Par défaut, le bout du sol le fait tourner.
        let (mut flock, id) = walking(&world, 0, 800.0, 1.0, "walk", realistic);
        for _ in 0..(6.0 / TICK) as usize {
            flock.advance(TICK, &world, &Input::default());
        }
        assert_eq!(
            find(&flock, id).support(),
            Support::Ground {
                surface: floor_of(&world, 0).id
            }
        );
        // S'il continue, il tombe sur l'écran d'à côté, sans jamais sortir.
        let (mut flock, id) = walking(&world, 0, 800.0, 1.0, "off", realistic);
        let mut lowest = 1.0f32;
        for _ in 0..(8.0 / TICK) as usize {
            flock.advance(TICK, &world, &Input::default());
            let pet = find(&flock, id);
            if !realistic || pet.support() != Support::Air {
                lowest = lowest.min(shown(&screens, body(pet)));
            }
            assert!(world.on_screens(pet.position() - Vec2::new(0.0, 20.0)));
        }
        let pet = find(&flock, id);
        assert_eq!(
            pet.support(),
            Support::Ground {
                surface: floor_of(&world, 1).id
            },
            "réaliste {realistic} : {:?}",
            pet.position()
        );
        assert!(lowest > 0.99, "coupé en tombant ({lowest})");
    }
}

/// Fait partir `id` en voyage et le suit jusqu'à ce qu'il soit posé depuis
/// une seconde ailleurs que sur le sol `from` (ou trente secondes). Renvoie
/// par où il est passé, et la plus petite part de son corps que les écrans
/// ont montrée (en physique réaliste, on ne la mesure qu'au sol : en l'air
/// il tourne, et sa boîte debout n'est plus la sienne).
fn travel(
    flock: &mut Flock,
    world: &World,
    id: u32,
    toward: Option<Vec2>,
    realistic: bool,
) -> (Option<VoyageWay>, f32) {
    let screens = world.screens().to_vec();
    let from = find(flock, id).support();
    assert!(
        flock.voyage(id, world, toward),
        "pas de passage depuis {:?}",
        find(flock, id).position()
    );
    let way = find(flock, id).voyage();
    let mut lowest = 1.0f32;
    let mut landed = 0;
    for _ in 0..(30.0 / TICK) as usize {
        flock.advance(TICK, world, &Input::default());
        let pet = find(flock, id);
        let grounded = matches!(pet.support(), Support::Ground { .. });
        if !realistic || grounded {
            lowest = lowest.min(shown(&screens, body(pet)));
        }
        let middle = pet.position() - Vec2::new(0.0, pet.def().physics.height * 0.5);
        assert!(world.on_screens(middle), "hors des écrans en {middle:?}");
        landed = if grounded && pet.support() != from {
            landed + 1
        } else {
            0
        };
        if landed as f32 * TICK > 1.0 {
            break;
        }
    }
    (way, lowest)
}

#[test]
fn a_floor_too_high_next_door_is_reached_with_a_great_leap() {
    // À gauche, un sol 120, 335 puis 600 points plus haut : bien plus que
    // sa portée (35 points). L'envie de voyager venue, il y va d'un bond.
    for rise in [120.0, 335.0, 600.0] {
        let screens = vec![
            screen(1000.0, 0.0, 1000.0, 800.0, 0.0, 0.0),
            screen(0.0, 0.0, 1000.0, 800.0, 0.0, rise),
        ];
        let world = World::new(screens.clone());
        for realistic in [false, true] {
            let (mut flock, id) = walking(&world, 0, 1500.0, 1.0, "walk", realistic);
            let (way, lowest) = travel(&mut flock, &world, id, None, realistic);
            let pet = find(&flock, id);
            let tag = format!(
                "marche de {rise}, réaliste {realistic} : {:?}",
                pet.position()
            );
            assert_eq!(way, Some(VoyageWay::Up), "{tag}");
            assert_eq!(
                pet.support(),
                Support::Ground {
                    surface: floor_of(&world, 1).id
                },
                "{tag}"
            );
            assert_eq!(pet.voyage(), None, "{tag}");
            assert!(lowest > 0.99, "{tag} : coupé ({lowest})");
        }
    }
}

#[test]
fn from_a_floor_higher_than_the_one_next_door_it_jumps_down() {
    // À droite, un sol 300 points plus bas : il saute du bout du sien.
    let screens = vec![
        screen(0.0, 0.0, 1000.0, 500.0, 0.0, 0.0),
        screen(1000.0, 0.0, 1000.0, 800.0, 0.0, 0.0),
    ];
    let world = World::new(screens.clone());
    for realistic in [false, true] {
        let (mut flock, id) = walking(&world, 0, 300.0, -1.0, "walk", realistic);
        let (way, lowest) = travel(&mut flock, &world, id, None, realistic);
        let pet = find(&flock, id);
        let tag = format!("réaliste {realistic} : {:?}", pet.position());
        assert_eq!(way, Some(VoyageWay::Down), "{tag}");
        assert_eq!(
            pet.support(),
            Support::Ground {
                surface: floor_of(&world, 1).id
            },
            "{tag}"
        );
        assert!(lowest > 0.99, "{tag} : coupé ({lowest})");
        // Et il en revient, d'un grand bond.
        let (way, lowest) = travel(&mut flock, &world, id, None, realistic);
        let pet = find(&flock, id);
        assert_eq!(way, Some(VoyageWay::Up), "retour, réaliste {realistic}");
        assert_eq!(
            pet.support(),
            Support::Ground {
                surface: floor_of(&world, 0).id
            },
            "retour, réaliste {realistic} : {:?}",
            pet.position()
        );
        assert!(lowest > 0.99, "retour : coupé ({lowest})");
    }
}

#[test]
fn it_goes_up_to_the_screen_above_through_its_floor_and_comes_back_down() {
    // (D'où il part, où il va : dans le carré, l'écran 2 est sous l'écran 0,
    // et l'essai commence par la descente.)
    for (part, start, other) in [
        ("pareils l'un sur l'autre", 0, 1),
        ("un écran au-dessus du portable", 0, 1),
        ("quatre écrans en carré", 0, 2),
    ] {
        let (name, screens) = layout(part);
        let world = World::new(screens.clone());
        let x = screens[start].frame.x + 500.0;
        let up_first = screens[other].frame.y < screens[start].frame.y;
        for realistic in [false, true] {
            let (mut flock, id) = walking(&world, start, x, 1.0, "walk", realistic);
            for (to, upwards) in [(other, up_first), (start, !up_first)] {
                let toward = screens[to].frame.center();
                let (way, lowest) = travel(&mut flock, &world, id, Some(toward), realistic);
                let pet = find(&flock, id);
                let tag = format!(
                    "{name}, vers l'écran {to}, réaliste {realistic} : {:?}",
                    pet.position()
                );
                let expected = if upwards {
                    VoyageWay::Above
                } else {
                    VoyageWay::Below
                };
                assert_eq!(way, Some(expected), "{tag}");
                assert_eq!(
                    pet.support(),
                    Support::Ground {
                        surface: floor_of(&world, to).id
                    },
                    "{tag}"
                );
                assert!(lowest > 0.99, "{tag} : coupé en chemin ({lowest})");
            }
        }
    }
}

#[test]
fn on_the_desk_of_the_photos_sheep_visit_every_screen_by_themselves() {
    // Un portable, un écran au-dessus, un autre à gauche : aucun sol n'en
    // rejoint un autre. Cinq moutons, vingt minutes, au rythme ordinaire.
    let (name, screens) = layout("un portable, un écran au-dessus");
    let world = World::new(screens.clone());
    let floors: Vec<_> = (0..screens.len()).map(|i| floor_of(&world, i).id).collect();
    for realistic in [false, true] {
        let mut flock = Flock::new();
        flock.set_realistic(realistic);
        for (i, def) in breeds().into_iter().enumerate() {
            flock.spawn(def, 40 + i as u64, &world, None);
        }
        // Où chacun était posé, et les arrivées sur chaque écran depuis un
        // autre.
        let mut last: std::collections::HashMap<u32, usize> = std::collections::HashMap::new();
        let mut arrivals = vec![0usize; screens.len()];
        let mut ways = std::collections::HashSet::new();
        let mut cut: std::collections::HashMap<u32, usize> = std::collections::HashMap::new();
        for step in 0..(1200.0 / TICK) as usize {
            flock.advance(TICK, &world, &Input::default());
            for pet in flock.pets() {
                if let Some(way) = pet.voyage() {
                    ways.insert(way);
                }
                let middle = pet.position() - Vec2::new(0.0, pet.def().physics.height * 0.5);
                assert!(
                    world.on_screens(middle),
                    "{name}, pas {step} : n° {} hors des écrans en {middle:?}",
                    pet.id()
                );
                let Support::Ground { surface } = pet.support() else {
                    continue;
                };
                // Posé : montré en entier (sauf le temps de se redresser).
                let clipped = cut.entry(pet.id()).or_insert(0);
                *clipped = if shown(&screens, body(pet)) < 0.97 {
                    *clipped + 1
                } else {
                    0
                };
                assert!(
                    *clipped < (2.0 / TICK) as usize,
                    "{name}, pas {step} : n° {} coupé en {:?}",
                    pet.id(),
                    pet.position()
                );
                let Some(now) = floors.iter().position(|&id| id == surface) else {
                    continue;
                };
                if let Some(before) = last.insert(pet.id(), now)
                    && before != now
                {
                    arrivals[now] += 1;
                }
            }
        }
        println!("{name}, réaliste {realistic} : arrivées {arrivals:?}, par {ways:?}");
        assert!(
            arrivals.iter().all(|&n| n >= 2),
            "réaliste {realistic} : arrivées par écran {arrivals:?}"
        );
        assert!(
            arrivals.iter().sum::<usize>() >= 15,
            "réaliste {realistic} : {arrivals:?}"
        );
        // Le grand bond de côté, le saut du bout du sol, la montée et la
        // descente à travers un sol : tout a servi.
        assert_eq!(ways.len(), 4, "réaliste {realistic} : {ways:?}");
    }
}

#[test]
fn a_ewe_and_her_lamb_stay_together_on_their_screen() {
    let (_, screens) = layout("un portable, un écran au-dessus");
    let world = World::new(screens);
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../pets");
    let def = Arc::new(load_pet(&root.join("bleu-du-maine/pet.toml")).expect("animal"));
    let mut flock = Flock::new();
    let ewe = flock.spawn_as(def, 3, &world, Some(700.0), Some(Sex::Female));
    // Seule, elle part quand l'envie lui en vient (on essaie sur une copie
    // du troupeau, chaque seconde, sans rien changer au vrai).
    let would_go = |flock: &Flock, id: u32| flock.clone().voyage(id, &world, None);
    let mut went = false;
    for step in 0..(30.0 / TICK) as usize {
        flock.advance(TICK, &world, &Input::default());
        went |= step % 120 == 0 && would_go(&flock, ewe);
    }
    assert!(went, "une brebis seule voyage");
    let lamb = flock.spawn_lamb(ewe, 5, 4.0, &world).expect("agneau");
    for step in 0..(120.0 / TICK) as usize {
        flock.advance(TICK, &world, &Input::default());
        if step % 120 == 0 {
            assert!(!would_go(&flock, ewe), "la brebis laisserait son agneau");
            assert!(!would_go(&flock, lamb), "l'agneau laisserait sa mère");
        }
        assert_eq!(find(&flock, ewe).voyage(), None);
        assert_eq!(find(&flock, lamb).voyage(), None);
    }
}

#[test]
fn where_the_floors_are_level_nobody_needs_to_travel() {
    // De plain-pied, ou à une petite marche près : on y va en marchant.
    for part in ["un seul écran", "pareils côte à côte", "alignés en bas"] {
        let (name, screens) = layout(part);
        let world = World::new(screens);
        let mut flock = Flock::new();
        flock.set_voyage_every(Some((1.0, 3.0)));
        let ids: Vec<u32> = breeds()
            .into_iter()
            .enumerate()
            .map(|(i, def)| flock.spawn(def, 60 + i as u64, &world, None))
            .collect();
        for step in 0..(60.0 / TICK) as usize {
            flock.advance(TICK, &world, &Input::default());
            for &id in &ids {
                assert_eq!(find(&flock, id).voyage(), None, "{name}, pas {step}");
            }
        }
        assert!(!flock.voyage(ids[0], &world, None), "{name}");
    }
}

#[test]
fn when_the_screen_below_goes_away_as_it_hops_down_it_stays_on_its_floor() {
    // Deux écrans l'un sur l'autre, et un troisième à côté de celui du haut
    // (le bureau n'est donc jamais de plain-pied). Il part vers celui du
    // dessous ; on le débranche pendant qu'il gagne son point de départ,
    // puis en plein saut. (L'écran du dessous est le dernier de la liste :
    // les deux autres gardent leur sol.)
    let above = screen(0.0, -800.0, 1280.0, 800.0, 0.0, 0.0);
    let beside = screen(1280.0, -800.0, 800.0, 500.0, 0.0, 0.0);
    let below = screen(0.0, 0.0, 1280.0, 800.0, 0.0, 40.0);
    let all = vec![above, beside, below];
    let left = vec![above, beside];
    for (realistic, hop) in [(false, false), (true, false), (false, true), (true, true)] {
        let mut world = World::new(all.clone());
        let top = floor_of(&world, 0);
        let (mut flock, id) = walking(&world, 0, 300.0, 1.0, "walk", realistic);
        // (Pas d'autre envie de voyager que celle de l'essai.)
        flock.set_voyage_every(Some((1000.0, 2000.0)));
        assert!(flock.voyage(id, &world, Some(below.frame.center())));
        assert_eq!(find(&flock, id).voyage(), Some(VoyageWay::Below));
        let mut unplugged = false;
        let mut settled = 0;
        for _ in 0..(20.0 / TICK) as usize {
            let pet = find(&flock, id);
            // (En plein saut : dès qu'il a quitté son sol.)
            let now = if hop {
                pet.support() == Support::Air
            } else {
                pet.voyage().is_some()
            };
            if now && !unplugged {
                world.set_screens(left.clone());
                unplugged = true;
            }
            flock.advance(TICK, &world, &Input::default());
            let pet = find(&flock, id);
            assert!(
                pet.position().y <= top.y + 12.0,
                "réaliste {realistic}, en plein saut {hop} : tombé sous son écran, en {:?}",
                pet.position()
            );
            // Posé depuis une seconde, son voyage oublié : c'est fini. (Plus
            // tard, l'envie lui reviendra, vers l'écran d'à côté.)
            let quiet = unplugged
                && pet.voyage().is_none()
                && matches!(pet.support(), Support::Ground { .. });
            settled = if quiet { settled + 1 } else { 0 };
            if settled == 120 {
                break;
            }
        }
        assert!(unplugged);
        assert_eq!(settled, 120, "réaliste {realistic}, en plein saut {hop}");
        let pet = find(&flock, id);
        assert_eq!(
            (pet.support(), pet.position().y),
            (Support::Ground { surface: top.id }, top.y),
            "réaliste {realistic}, en plein saut {hop}"
        );
    }
}

#[test]
fn picked_up_on_its_way_it_forgets_its_voyage() {
    let (_, screens) = layout("pareils l'un sur l'autre");
    let world = World::new(screens);
    let (mut flock, id) = walking(&world, 0, 300.0, 1.0, "walk", false);
    assert!(flock.voyage(id, &world, None));
    for _ in 0..30 {
        flock.advance(TICK, &world, &Input::default());
    }
    assert!(find(&flock, id).voyage().is_some());
    let at = find(&flock, id).position() - Vec2::new(0.0, 20.0);
    let pet = flock.get_mut(id).unwrap();
    pet.grab(at, &world);
    pet.drag_to(at);
    for _ in 0..10 {
        flock.advance(TICK, &world, &Input::default());
    }
    assert_eq!(find(&flock, id).voyage(), None);
    flock.get_mut(id).unwrap().release(&world);
    for _ in 0..(3.0 / TICK) as usize {
        flock.advance(TICK, &world, &Input::default());
    }
    // Reposé chez lui, il ne passe pas à travers son sol.
    assert_eq!(
        find(&flock, id).support(),
        Support::Ground {
            surface: floor_of(&world, 0).id
        }
    );
}

#[test]
fn where_there_is_no_screen_beyond_the_floor_it_ends_at_a_wall() {
    // Un grand écran à droite, dont le sol est plus bas que tout l'écran de
    // gauche : en marchant vers la gauche, il n'y a plus rien.
    let screens = vec![
        screen(1000.0, 0.0, 1000.0, 1200.0, 0.0, 0.0),
        screen(0.0, 0.0, 1000.0, 600.0, 0.0, 0.0),
    ];
    let world = World::new(screens.clone());
    for realistic in [false, true] {
        for behavior in ["walk", "off"] {
            let (mut flock, id) = walking(&world, 0, 1200.0, -1.0, behavior, realistic);
            for _ in 0..(10.0 / TICK) as usize {
                flock.advance(TICK, &world, &Input::default());
                let pet = find(&flock, id);
                assert!(
                    shown(&screens, body(pet)) > 0.99,
                    "{behavior}, réaliste {realistic} : {:?}",
                    pet.position()
                );
            }
            assert_eq!(
                find(&flock, id).support(),
                Support::Ground {
                    surface: floor_of(&world, 0).id
                }
            );
        }
    }
}

/// Lance un mouton d'essai : pris en `from`, mené en `to` en un dixième de
/// seconde, lâché. Renvoie le troupeau.
fn thrown(world: &World, from: Vec2, to: Vec2, realistic: bool) -> (Flock, u32) {
    let mut flock = Flock::new();
    flock.set_realistic(realistic);
    let id = flock.spawn(walker(), 3, world, Some(from.x));
    let input = Input::default();
    // Pris par le dos, là où il est ; la main l'amène d'abord en `from`,
    // sans hâte, et attend qu'il ne se balance plus.
    let pet = flock.get_mut(id).unwrap();
    let start = pet.position() - Vec2::new(0.0, 20.0);
    pet.grab(start, world);
    let carry = (0.5 / TICK) as usize;
    for i in 0..=carry + (1.5 / TICK) as usize {
        let at = start.lerp(from, (i as f32 / carry as f32).min(1.0));
        flock.get_mut(id).unwrap().drag_to(at);
        flock.advance(TICK, world, &input);
    }
    let steps = (0.1 / TICK) as usize;
    for i in 0..=steps {
        let at = from.lerp(to, i as f32 / steps as f32);
        flock.get_mut(id).unwrap().drag_to(at);
        flock.advance(TICK, world, &input);
    }
    flock.get_mut(id).unwrap().release(world);
    (flock, id)
}

#[test]
fn thrown_upwards_under_a_screen_above_it_lands_up_there() {
    // Un écran au-dessus du portable, sans rien entre les deux.
    let screens = vec![
        screen(0.0, 0.0, 1536.0, 864.0, 0.0, 48.0),
        screen(128.0, -1024.0, 1280.0, 1024.0, 0.0, 38.4),
    ];
    let world = World::new(screens.clone());
    for realistic in [false, true] {
        let (mut flock, id) = thrown(
            &world,
            Vec2::new(700.0, 300.0),
            Vec2::new(700.0, 120.0),
            realistic,
        );
        let mut highest = f32::INFINITY;
        for _ in 0..(8.0 / TICK) as usize {
            flock.advance(TICK, &world, &Input::default());
            let pet = find(&flock, id);
            highest = highest.min(pet.position().y);
            assert!(world.on_screens(pet.position() - Vec2::new(0.0, 20.0)));
        }
        let pet = find(&flock, id);
        assert!(
            highest < -100.0,
            "il est monté sur l'écran du haut ({highest})"
        );
        assert_eq!(
            pet.support(),
            Support::Ground {
                surface: floor_of(&world, 1).id
            },
            "réaliste {realistic} : {:?}",
            pet.position()
        );
        // À côté de l'écran du haut, il n'y a rien au-dessus : il retombe.
        let (mut flock, id) = thrown(
            &world,
            Vec2::new(60.0, 300.0),
            Vec2::new(60.0, 120.0),
            realistic,
        );
        for _ in 0..(8.0 / TICK) as usize {
            flock.advance(TICK, &world, &Input::default());
            let pet = find(&flock, id);
            assert!(
                pet.position().y > 30.0,
                "réaliste {realistic} : sorti par le haut en {:?}",
                pet.position()
            );
        }
        assert_eq!(
            find(&flock, id).support(),
            Support::Ground {
                surface: floor_of(&world, 0).id
            }
        );
    }
}

#[test]
fn thrown_towards_where_there_is_no_screen_it_bounces_back() {
    // Un petit écran à gauche, un grand à droite qui monte plus haut : lancé
    // du haut du grand vers la gauche, il n'y a pas d'écran pour lui.
    let screens = vec![
        screen(0.0, 400.0, 1000.0, 600.0, 0.0, 0.0),
        screen(1000.0, 0.0, 1000.0, 1000.0, 0.0, 0.0),
    ];
    let world = World::new(screens.clone());
    for realistic in [false, true] {
        let (mut flock, id) = thrown(
            &world,
            Vec2::new(1300.0, 150.0),
            Vec2::new(1150.0, 140.0),
            realistic,
        );
        for step in 0..(6.0 / TICK) as usize {
            flock.advance(TICK, &world, &Input::default());
            let pet = find(&flock, id);
            let at = pet.position();
            // Tant qu'il est plus haut que le petit écran, il reste sur le grand.
            assert!(
                at.y > 400.0 + 45.0 || at.x >= 1000.0 + 20.0,
                "réaliste {realistic}, pas {step} : {at:?}"
            );
            assert!(world.on_screens(at - Vec2::new(0.0, 20.0)), "{at:?}");
        }
        assert!(matches!(find(&flock, id).support(), Support::Ground { .. }));
    }
}

#[test]
fn dropped_where_there_is_no_screen_it_comes_back() {
    let (_, screens) = layout("un autre à gauche");
    let world = World::new(screens.clone());
    // À gauche du portable, sous l'écran de gauche ; à droite de l'écran du
    // haut ; tout en bas ; très loin.
    let nowhere = [
        Vec2::new(-400.0, 700.0),
        Vec2::new(1400.0, -400.0),
        Vec2::new(700.0, 1200.0),
        Vec2::new(-3000.0, -2000.0),
        Vec2::new(4000.0, 300.0),
    ];
    for realistic in [false, true] {
        for at in nowhere {
            assert!(!world.on_screens(at));
            let mut flock = Flock::new();
            flock.set_realistic(realistic);
            let id = flock.spawn(walker(), 5, &world, Some(100.0));
            let pet = flock.get_mut(id).unwrap();
            pet.grab(pet.position() - Vec2::new(0.0, 20.0), &world);
            pet.drag_to(at);
            flock.advance(TICK, &world, &Input::default());
            flock.get_mut(id).unwrap().release(&world);
            for _ in 0..(6.0 / TICK) as usize {
                flock.advance(TICK, &world, &Input::default());
            }
            let pet = find(&flock, id);
            assert!(
                matches!(pet.support(), Support::Ground { .. }),
                "lâché en {at:?}, réaliste {realistic} : {:?} en {:?}",
                pet.support(),
                pet.position()
            );
            assert!(
                shown(&screens, body(pet)) > 0.99,
                "lâché en {at:?}, réaliste {realistic} : resté en {:?}",
                pet.position()
            );
        }
    }
}

#[test]
fn when_a_screen_goes_away_its_pets_come_back_to_the_others() {
    for (name, screens) in layouts().into_iter().skip(1) {
        for realistic in [false, true] {
            let mut world = World::new(screens.clone());
            let mut flock = Flock::new();
            flock.set_realistic(realistic);
            let def = walker();
            for seed in 0..40 {
                flock.spawn(Arc::clone(&def), seed, &world, None);
            }
            for _ in 0..(4.0 / TICK) as usize {
                flock.advance(TICK, &world, &Input::default());
            }
            // Le dernier écran est débranché ; puis il ne reste que le premier.
            for left in [screens.len() - 1, 1] {
                let kept = &screens[..left];
                world.set_screens(kept.to_vec());
                for _ in 0..(8.0 / TICK) as usize {
                    flock.advance(TICK, &world, &Input::default());
                }
                for pet in flock.pets() {
                    assert!(
                        matches!(pet.support(), Support::Ground { .. })
                            && shown(kept, body(pet)) > 0.97,
                        "{name}, {left} écran(s), réaliste {realistic} : {:?} en {:?}",
                        pet.support(),
                        pet.position()
                    );
                }
            }
        }
    }
}

#[test]
fn a_saucer_visits_only_the_screen_of_the_pet_it_came_for() {
    let (name, screens) = layout("un autre à gauche");
    let world = World::new(screens.clone());
    for seed in 0..6u64 {
        let mut flock = Flock::new();
        let def = walker();
        for k in 0..6 {
            flock.spawn(Arc::clone(&def), seed * 10 + k, &world, None);
        }
        for _ in 0..(3.0 / TICK) as usize {
            flock.advance(TICK, &world, &Input::default());
        }
        assert!(flock.invite_saucer(&world, seed).is_some());
        let target = flock.saucers()[0].target();
        let home = *world.screen_at(find(&flock, target).position()).unwrap();
        let mut carried = false;
        for _ in 0..(30.0 / TICK) as usize {
            flock.advance(TICK, &world, &Input::default());
            let pet = find(&flock, target);
            if pet.is_held() {
                carried = true;
                let at = pet.position();
                assert!(
                    at.x >= home.usable.left() && at.x <= home.usable.right(),
                    "{name}, graine {seed} : emmené en {at:?}, hors de son écran {:?}",
                    home.frame
                );
            }
        }
        assert!(
            carried,
            "{name}, graine {seed} : la soucoupe n'a pris personne"
        );
        let pet = find(&flock, target);
        assert!(
            matches!(pet.support(), Support::Ground { .. }) && shown(&screens, body(pet)) > 0.99,
            "{name}, graine {seed} : reposé en {:?}",
            pet.position()
        );
    }
}

/// Ce que coûte la forme des écrans : mille animaux sur un seul écran, puis
/// sur trois écrans décalés (les questions sur les bandes sont alors posées
/// à chaque pas, pour chacun).
/// `cargo test --release -p mouflon-core --test ecrans -- --ignored --nocapture cost`
#[test]
#[ignore = "mesure, à lancer à la main"]
fn cost_of_a_crowd_on_several_screens() {
    let defs = breeds();
    for (name, screens) in [layout("un seul écran"), layout("un autre à gauche")] {
        for realistic in [false, true] {
            let world = World::new(screens.clone());
            let mut flock = Flock::new();
            flock.set_realistic(realistic);
            for k in 0..1000u64 {
                flock.spawn(Arc::clone(&defs[k as usize % defs.len()]), k, &world, None);
            }
            let input = Input::default();
            for _ in 0..(5.0 / TICK) as usize {
                flock.advance(TICK, &world, &input);
            }
            let steps = (10.0 / TICK) as usize;
            let started = std::time::Instant::now();
            for _ in 0..steps {
                flock.advance(TICK, &world, &input);
            }
            println!(
                "{name}, {} : {:.3} ms par pas pour mille animaux",
                if realistic { "réaliste" } else { "classique" },
                started.elapsed().as_secs_f64() * 1000.0 / steps as f64
            );
        }
    }
}
