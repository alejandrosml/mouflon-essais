//! `film` : la vidéo de la page de présentation. Six races sur un bureau,
//! et en une vingtaine de secondes ce que font les moutons de Mouflon :
//! tomber du haut de l'écran, brouter, sauter sur une fenêtre plus haute,
//! gratter le sol et charger, jouer comme des agneaux, se laisser attraper
//! et lancer, tourner le dos au vent, s'endormir.
//!
//! ```text
//! cargo run -p mouflon-preview --release -- pets <dossier> film [durée] [une image sur N]
//! ```
//!
//! Tout vient de la simulation réelle ; la scène ne fait que demander un
//! comportement à un animal à un instant donné (`Pet::force`), comme le
//! ferait le hasard. Les instants sont repris dans `tools/vitrine/page.py`
//! (`SCENES`), qui écrit la légende sous la vidéo : à changer ensemble.

use super::*;

/// Un rôle : le dessin, l'instant et l'abscisse de l'arrivée.
const CAST: [(&str, f32, f32); 6] = [
    ("floucat/pet.toml", 0.4, 100.0),
    ("mouflon-corse/pet.toml", 1.2, 300.0),
    ("jacob/pet.toml#male", 2.0, 905.0),
    ("southdown/pet.toml#lamb", 7.2, 730.0),
    ("ouessant/pet.toml#lamb", 7.6, 835.0),
    ("nez-noir/pet.toml#female", 11.0, 560.0),
];
const MENEUR: usize = 0;
const MOUFLON: usize = 1;
const BELIER: usize = 2;
const AGNELLE: usize = 3;
const AGNEAU: usize = 4;
const BREBIS: usize = 5;

/// Le curseur : il arrive à `GRAB - 1,6`, prend la brebis à `GRAB`, la lance à `THROW`.
const GRAB: f32 = 15.4;
const THROW: f32 = 16.5;

pub fn render(args: &[String], out_dir: &Path) -> Result<(), Error> {
    let pets = Path::new(args.get(1).map_or("pets", String::as_str));
    let arg = |i: usize, default: f32| args.get(i).and_then(|a| a.parse().ok()).unwrap_or(default);
    let duration = arg(4, 24.0);
    let every = (arg(5, 1.0) as u32).max(1);
    let size = 2.2;

    let mut casts = Vec::new();
    for (path, _, _) in CAST {
        let def = Arc::new(load_choice(&pets.join(path).to_string_lossy())?.scaled(size));
        let art = vector(PetArt::load(&def)?);
        casts.push((def, art));
    }

    // Une fenêtre basse, une plus haute juste à côté (le rocher du
    // mouflon), et le sol libre à droite.
    let low = Rect::new(20.0, 372.0, 400.0, 110.0);
    let corner = low.right();
    let rise = default_rise(&casts[MOUFLON].0);
    println!("la fenêtre haute : {rise:.0} points au-dessus de la basse");
    let high = Rect::new(corner, low.y - rise, 270.0, 40.0 + rise);
    let windows = [low, high];
    let rects = [
        WindowRect { id: 1, frame: low },
        WindowRect { id: 2, frame: high },
    ];
    let mut world = World::new(vec![screen()]);
    world.set_windows(&rects, LedgeOptions::default());

    let frames_dir = out_dir.join("frames");
    std::fs::create_dir_all(&frames_dir)?;
    let mut flock = Flock::new();
    flock.set_realistic(true);
    let mut ids: Vec<Option<u32>> = vec![None; CAST.len()];
    let mut leaped = false;
    let mut grab_start: Option<Vec2> = None;
    let mut jobs: Vec<(PathBuf, DisplayList)> = Vec::new();
    let mut written = 0_u32;
    let steps = (duration * FPS as f32) as u32;
    let input = Input {
        local_hour: 15.0,
        ..Input::default()
    };

    for step in 0..steps {
        let t = step as f32 / FPS as f32;
        let at = |s: f32| (t - s).abs() < 0.5 / FPS as f32;
        for (i, (_, arrival, x)) in CAST.iter().enumerate() {
            if ids[i].is_none() && t >= *arrival {
                let id = flock.spawn(
                    Arc::clone(&casts[i].0),
                    7 + i as u64 * 7919,
                    &world,
                    Some(*x),
                );
                if let Some(pet) = flock.get_mut(id) {
                    let art = casts[i].1.worn(pet.variant());
                    if let Some((front, back)) = art.reach() {
                        pet.set_reach(front, back);
                    }
                    if let Some(tops) = art.tops() {
                        pet.set_tops(tops);
                    }
                }
                ids[i] = Some(id);
            }
        }
        // Le meneur broute, sa cloche au cou ; plus tard il rumine, puis dort.
        if at(2.6) || at(7.5) {
            play(&mut flock, &ids, MENEUR, 0.0, "graze");
        }
        if at(12.5) {
            play(&mut flock, &ids, MENEUR, 0.0, "sit");
        }
        if at(14.5) {
            play(&mut flock, &ids, MENEUR, 0.0, "ruminate");
        }
        if at(21.3) {
            play(&mut flock, &ids, MENEUR, 0.0, "sleep");
        }

        // Le bélier Jacob gratte le sol, recule, et charge.
        if at(3.6) {
            play(&mut flock, &ids, BELIER, -1.0, "paw");
        }
        if at(4.8) {
            play(&mut flock, &ids, BELIER, -1.0, "back_up");
        }
        if at(10.5) {
            play(&mut flock, &ids, BELIER, 0.0, "shake");
        }
        if at(13.0) {
            play(&mut flock, &ids, BELIER, 0.0, "graze");
        }

        // Les deux agneaux : coups de front, puis cabrioles.
        if at(9.0) {
            play(&mut flock, &ids, AGNELLE, 1.0, "play_back");
            play(&mut flock, &ids, AGNEAU, -1.0, "play_back");
        }
        if at(12.6) {
            play(&mut flock, &ids, AGNELLE, 1.0, "gambol");
            play(&mut flock, &ids, AGNEAU, -1.0, "pivot");
        }
        if at(14.4) {
            play(&mut flock, &ids, AGNEAU, -1.0, "gambol");
            play(&mut flock, &ids, AGNELLE, 0.0, "pronk");
        }
        if at(20.6) {
            play(&mut flock, &ids, AGNELLE, 0.0, "sit");
        }

        // La brebis Nez noir bêle ; le curseur la prend et la lance.
        if at(13.4) {
            play(&mut flock, &ids, BREBIS, 1.0, "bleat");
        }

        // Le coup de vent.
        if at(18.2) {
            flock.blow(1.0, 4.0);
        }

        // Le mouflon marche jusqu'au pied de la fenêtre haute et y saute
        // comme sur un rocher ; plus tard, il s'en va.
        if !leaped
            && t >= 2.6
            && let Some(pet) = ids[MOUFLON].and_then(|id| flock.get_mut(id))
            && matches!(pet.support(), Support::Ground { .. })
        {
            if pet.position().x + casts[MOUFLON].1.width * 0.5 >= corner - 3.0 {
                pet.force("leap");
                leaped = true;
            } else if pet.behavior_name() != "walk" {
                pet.face(1.0);
                pet.force("walk");
            }
        }
        if at(9.6) {
            play(&mut flock, &ids, MOUFLON, 1.0, "look");
        }
        if at(11.6) {
            play(&mut flock, &ids, MOUFLON, 1.0, "stamp");
        }
        if at(17.4)
            && let Some(id) = ids[MOUFLON]
        {
            flock.leave(id, &world, &input);
        }

        // Le curseur.
        let lift = casts[BREBIS].0.physics.height * 0.55;
        let cursor = ids[BREBIS]
            .and_then(|id| flock.pets().iter().find(|p| p.id() == id))
            .and_then(|pet| cursor(t, pet.position() + Vec2::new(0.0, -lift), &mut grab_start));
        let moved = Input { cursor, ..input };
        let alpha = flock.advance(1.0 / FPS as f32, &world, &moved);
        if let Some(pet) = ids[BREBIS].and_then(|id| flock.get_mut(id))
            && let Some(c) = cursor
        {
            if at(GRAB) {
                pet.grab(c, &world);
            }
            if (GRAB..THROW).contains(&t) {
                pet.drag_to(c);
            }
        }
        if at(THROW)
            && let Some(pet) = ids[BREBIS].and_then(|id| flock.get_mut(id))
        {
            pet.release(&world);
        }

        if step % FPS == 0 {
            let states: Vec<String> = flock
                .pets()
                .iter()
                .map(|p| {
                    let who = ids.iter().position(|id| *id == Some(p.id())).unwrap_or(9);
                    format!(
                        "{who}:{}@{:.0},{:.0}",
                        p.behavior_name(),
                        p.position().x,
                        p.position().y
                    )
                })
                .collect();
            println!("{t:4.1} s : {}", states.join("  "));
        }

        // Une image sur deux est dessinée : 30 images par seconde.
        if step % 2 == 1 {
            continue;
        }
        let shot = step / 2;
        if shot % every != 0 {
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
            paint_cursor(&mut list, c, flock.pets().iter().any(Pet::is_held));
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

/// Où est le curseur à l'instant `t` : il vient du haut à droite jusqu'à
/// `head`, prend l'animal, le balance en arrière puis le lance vers la droite.
fn cursor(t: f32, head: Vec2, grab_start: &mut Option<Vec2>) -> Option<Vec2> {
    if (GRAB - 1.6..GRAB).contains(&t) {
        let from = Vec2::new(SCREEN_W - 60.0, 90.0);
        let k = ease((t - (GRAB - 1.6)) / 1.2);
        return Some(
            from.lerp(head + Vec2::new(30.0, -40.0), k)
                .lerp(head, ease((t - (GRAB - 0.4)) / 0.4)),
        );
    }
    if (GRAB..THROW + 0.5).contains(&t) {
        let start = *grab_start.get_or_insert(head);
        let end = start + Vec2::new(150.0, -150.0);
        // Un petit arc, puis un geste vif au moment du lâcher : l'animal part avec élan.
        let u = ((t - GRAB) / (THROW - GRAB)).clamp(0.0, 1.0);
        let k = if u < 0.8 {
            0.5 * ease(u / 0.8)
        } else {
            0.5 + 0.5 * (u - 0.8) / 0.2
        };
        let arc = Vec2::new(0.0, -50.0 * (k * std::f32::consts::PI).sin());
        return Some(start.lerp(end, k) + arc);
    }
    None
}

/// Demande un comportement à un rôle (et le tourne, si `facing` n'est pas nul).
fn play(flock: &mut Flock, ids: &[Option<u32>], who: usize, facing: f32, name: &str) {
    if let Some(pet) = ids[who].and_then(|id| flock.get_mut(id)) {
        if facing != 0.0 {
            pet.face(facing);
        }
        if !pet.force(name) {
            eprintln!("{} ne sait pas « {name} »", CAST[who].0);
        }
    }
}
