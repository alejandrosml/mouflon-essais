//! Des dessins abîmés : un animal fait par quelqu'un d'autre, un fichier
//! tronqué ou retouché à la main ne doit jamais faire tomber l'app. Chaque
//! dessin du dépôt est abîmé au hasard (des nombres remplacés par zéro, par
//! des valeurs énormes ou qui ne sont pas des nombres, des lignes ôtées, le
//! fichier coupé) : ou bien il est refusé au chargement, ou bien l'animal
//! vit et se dessine, à des places qui sont toutes des nombres.
//!
//! La version courte tourne avec les autres essais ; la longue se lance à
//! la main :
//!
//! ```text
//! cargo test --release -p mouflon-render --test robustesse -- --ignored --nocapture
//! ```

use std::path::Path;
use std::sync::Arc;

use mouflon_core::math::Rng;
use mouflon_core::{Flock, Input, LoadError, Mood, Rect, Screen, TICK, World, parse_pet_toml};
use mouflon_render::display::Command;
use mouflon_render::{DisplayList, Frame, PetArt, Style, paint_pet};

/// Ce qui remplace un nombre.
const HOSTILE: [&str; 14] = [
    "0",
    "-0",
    "1e39",
    "-1e39",
    "1e-39",
    "NaN",
    "inf",
    "-inf",
    "",
    "99999999",
    "-99999999",
    "0.0000001",
    "-5",
    "1e400",
];

/// Abîme un texte : quelques nombres remplacés, parfois une ligne ôtée ou
/// doublée, parfois la fin coupée.
fn damage(text: &str, rng: &mut Rng) -> String {
    let bytes = text.as_bytes();
    // Les nombres du texte : (début, fin).
    let mut numbers = Vec::new();
    let mut i = 0;
    while i < bytes.len() {
        if bytes[i].is_ascii_digit() {
            let start = i;
            while i < bytes.len() && (bytes[i].is_ascii_digit() || bytes[i] == b'.') {
                i += 1;
            }
            numbers.push((start, i));
        } else {
            i += 1;
        }
    }
    let mut out = text.to_owned();
    if !numbers.is_empty() {
        let mut picks: Vec<(usize, usize)> = (0..1 + rng.next_u32() % 4)
            .map(|_| numbers[rng.next_u32() as usize % numbers.len()])
            .collect();
        picks.sort_unstable();
        picks.dedup();
        for &(start, end) in picks.iter().rev() {
            let with = HOSTILE[rng.next_u32() as usize % HOSTILE.len()];
            out.replace_range(start..end, with);
        }
    }
    match rng.next_u32() % 8 {
        0 => {
            // Une ligne en moins.
            let lines: Vec<&str> = out.lines().collect();
            let gone = rng.next_u32() as usize % lines.len().max(1);
            out = lines
                .iter()
                .enumerate()
                .filter(|(n, _)| *n != gone)
                .map(|(_, line)| *line)
                .collect::<Vec<_>>()
                .join("\n");
        }
        1 => {
            // Une ligne en double.
            let lines: Vec<&str> = out.lines().collect();
            let twice = rng.next_u32() as usize % lines.len().max(1);
            let mut all = Vec::with_capacity(lines.len() + 1);
            for (n, line) in lines.iter().enumerate() {
                all.push(*line);
                if n == twice {
                    all.push(*line);
                }
            }
            out = all.join("\n");
        }
        2 => {
            // La fin coupée (à une limite de caractère).
            let mut at = rng.next_u32() as usize % out.len().max(1);
            while !out.is_char_boundary(at) {
                at -= 1;
            }
            out.truncate(at);
        }
        _ => {}
    }
    out
}

/// Un animal dont un fichier (son dessin, ou sa fiche) est abîmé : refusé,
/// ou vivant et dessiné sans faute. Renvoie s'il a été accepté.
fn survives(pet: &str, seed: u64, toml_too: bool) -> bool {
    let root = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../pets")
        .join(pet);
    let mut rng = Rng::new(seed);
    let mut toml = std::fs::read_to_string(root.join("pet.toml")).expect("pet.toml");
    if toml_too {
        toml = damage(&toml, &mut rng);
    }
    // Un seul de ses fichiers est abîmé (le premier dessin demandé).
    let damaged = std::cell::Cell::new(toml_too);
    let rng_cell = std::cell::RefCell::new(rng);
    let def = parse_pet_toml(&toml, |name: &str| {
        let path = root.join(name);
        let bytes = std::fs::read(&path).map_err(|source| LoadError::Io { path, source })?;
        if name.ends_with(".svg") && !damaged.replace(true) {
            let text = String::from_utf8_lossy(&bytes);
            return Ok(damage(&text, &mut rng_cell.borrow_mut()).into_bytes());
        }
        Ok(bytes)
    });
    let Ok(def) = def else {
        return false;
    };
    let def = Arc::new(def);
    let Ok(art) = PetArt::load(&def) else {
        return false;
    };
    let world = World::new(vec![Screen {
        frame: Rect::new(0.0, 0.0, 1440.0, 900.0),
        usable: Rect::new(0.0, 25.0, 1440.0, 800.0),
        scale: 2.0,
    }]);
    let mut flock = Flock::new();
    flock.set_mood(Mood::Lively);
    for x in [300.0, 700.0] {
        flock.spawn(Arc::clone(&def), seed ^ x as u64, &world, Some(x));
    }
    let input = Input {
        local_hour: 15.0,
        ..Input::default()
    };
    for step in 0..(6.0 / TICK) as usize {
        flock.advance(TICK, &world, &input);
        if step % 40 != 0 {
            continue;
        }
        for pet in flock.pets() {
            let art = art.worn(pet.variant());
            let canvas = art.canvas();
            let view = pet.view(0.5);
            assert!(
                canvas.width.is_finite()
                    && canvas.height.is_finite()
                    && canvas.width <= 16_384.0
                    && canvas.height <= 16_384.0,
                "{} (graine {seed}) : toile de {} × {}",
                def.meta.id,
                canvas.width,
                canvas.height
            );
            let mut list =
                DisplayList::new((canvas.width * 2.0) as u32, (canvas.height * 2.0) as u32);
            paint_pet(
                art,
                &view,
                &Frame {
                    origin: view.ground - canvas.anchor,
                    scale: 2.0,
                },
                &Style::default(),
                &mut list,
            );
            for command in &list.commands {
                if let Command::Image {
                    transform, alpha, ..
                } = command
                {
                    assert!(
                        transform.as_coeffs().iter().all(|c| c.is_finite()) && alpha.is_finite(),
                        "{} (graine {seed}, {}) : un morceau posé nulle part ({transform:?})",
                        def.meta.id,
                        view.behavior
                    );
                }
            }
        }
    }
    true
}

fn damaged_pets(pets: &[&str], seeds: std::ops::Range<u64>) {
    let (mut accepted, mut refused) = (0, 0);
    for seed in seeds {
        for (n, pet) in pets.iter().enumerate() {
            let seed = seed * 131 + n as u64;
            if survives(pet, seed, seed.is_multiple_of(4)) {
                accepted += 1;
            } else {
                refused += 1;
            }
        }
    }
    eprintln!("{accepted} animaux abîmés acceptés et dessinés, {refused} refusés");
    assert!(
        accepted > 0,
        "aucun dessin abîmé n'a été accepté : l'essai ne dit rien"
    );
}

#[test]
fn a_damaged_drawing_is_refused_or_drawn_but_never_breaks() {
    damaged_pets(&["bleu-du-maine", "racka", "solognote"], 0..6);
}

#[test]
#[ignore = "essai long, à lancer à la main en --release"]
fn many_damaged_drawings_are_refused_or_drawn_but_never_break() {
    damaged_pets(
        &[
            "bleu-du-maine",
            "racka",
            "solognote",
            "merinos-d-arles",
            "ouessant",
            "scottish-blackface",
            "awassi",
            "floucat",
        ],
        100..140,
    );
}
