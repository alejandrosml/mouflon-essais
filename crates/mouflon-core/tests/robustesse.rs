//! Des fiches abîmées : le `pet.toml` d'un animal (ou ses animations
//! desktopPet, ses fichiers annexes) retouché à la main, tronqué, avec des
//! nombres absurdes, ne doit jamais faire tomber l'app. Ou bien l'animal est
//! refusé au chargement, ou bien il vit, à des places qui sont des nombres.
//!
//! La version courte tourne avec les autres essais ; la longue se lance à
//! la main :
//!
//! ```text
//! cargo test --release -p mouflon-core --test robustesse -- --ignored --nocapture
//! ```

use std::path::{Path, PathBuf};
use std::sync::Arc;

use mouflon_core::math::Rng;
use mouflon_core::{
    Flock, Input, LedgeOptions, LoadError, Rect, Screen, TICK, Vec2, WindowRect, World,
    parse_pet_toml,
};

/// Ce qui remplace un nombre.
const HOSTILE: [&str; 16] = [
    "0",
    "-0",
    "1e39",
    "-1e39",
    "1e-39",
    "nan",
    "inf",
    "-inf",
    "",
    "99999999",
    "-99999999",
    "0.0000001",
    "-5",
    "1e400",
    "4294967296",
    "-1",
];

/// Abîme un texte : quelques nombres remplacés, parfois une ligne ôtée ou
/// doublée, parfois la fin coupée.
fn damage(text: &str, rng: &mut Rng) -> String {
    let bytes = text.as_bytes();
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
            out.replace_range(start..end, HOSTILE[rng.next_u32() as usize % HOSTILE.len()]);
        }
    }
    let lines: Vec<&str> = out.lines().collect();
    let at = rng.next_u32() as usize % lines.len().max(1);
    match rng.next_u32() % 8 {
        0 => {
            out = lines
                .iter()
                .enumerate()
                .filter(|(n, _)| *n != at)
                .map(|(_, line)| *line)
                .collect::<Vec<_>>()
                .join("\n");
        }
        1 => {
            let mut all = lines.clone();
            all.insert(at, lines.get(at).copied().unwrap_or(""));
            out = all.join("\n");
        }
        2 => {
            let mut cut = rng.next_u32() as usize % out.len().max(1);
            while !out.is_char_boundary(cut) {
                cut -= 1;
            }
            out.truncate(cut);
        }
        _ => {}
    }
    out
}

/// Tous les animaux du dépôt et des données d'essai.
fn pets() -> Vec<PathBuf> {
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../..");
    let mut found = Vec::new();
    for dir in ["pets", "tests/donnees"] {
        let mut here: Vec<PathBuf> = std::fs::read_dir(root.join(dir))
            .expect("dossier")
            .filter_map(Result::ok)
            .map(|entry| entry.path())
            .filter(|path| path.join("pet.toml").is_file())
            .collect();
        here.sort();
        found.extend(here);
    }
    found
}

/// Un animal dont la fiche, ou l'un de ses fichiers de texte, est abîmé :
/// refusé, ou vivant sans faute. Renvoie s'il a été accepté.
fn survives(dir: &Path, seed: u64) -> bool {
    let mut rng = Rng::new(seed);
    let mut toml = std::fs::read_to_string(dir.join("pet.toml")).expect("pet.toml");
    // La fiche elle-même, ou le premier fichier de texte qu'elle demande.
    let annex = rng.chance(0.4);
    if !annex {
        toml = damage(&toml, &mut rng);
    }
    // Pour chercher : `MOUFLON_TRACE=graine` montre ce qui a été abîmé.
    if std::env::var("MOUFLON_TRACE")
        .ok()
        .and_then(|v| v.parse::<u64>().ok())
        == Some(seed)
    {
        let before = std::fs::read_to_string(dir.join("pet.toml")).expect("pet.toml");
        for (a, b) in before.lines().zip(toml.lines()) {
            if a != b {
                eprintln!("  avant : {a}\n  après : {b}");
            }
        }
    }
    let damaged = std::cell::Cell::new(!annex);
    let rng = std::cell::RefCell::new(rng);
    let def = parse_pet_toml(&toml, |name: &str| {
        let path = dir.join(name);
        let bytes = std::fs::read(&path).map_err(|source| LoadError::Io { path, source })?;
        let text = name.ends_with(".xml") || name.ends_with(".toml");
        if text && !damaged.replace(true) {
            let text = String::from_utf8_lossy(&bytes);
            return Ok(damage(&text, &mut rng.borrow_mut()).into_bytes());
        }
        Ok(bytes)
    });
    let Ok(def) = def else {
        return false;
    };
    let def = Arc::new(def);
    let mut rng = rng.into_inner();
    let mut world = World::new(vec![Screen {
        frame: Rect::new(0.0, 0.0, 1440.0, 900.0),
        usable: Rect::new(0.0, 25.0, 1440.0, 800.0),
        scale: 2.0,
    }]);
    world.set_windows(
        &[
            WindowRect {
                id: 1,
                frame: Rect::new(200.0, 400.0, 500.0, 300.0),
            },
            WindowRect {
                id: 2,
                frame: Rect::new(800.0, 250.0, 420.0, 380.0),
            },
        ],
        LedgeOptions::default(),
    );
    let mut input = Input {
        cursor: Some(Vec2::new(700.0, 400.0)),
        local_hour: 15.0,
        ..Input::default()
    };
    for realistic in [true, false] {
        let mut flock = Flock::new();
        flock.set_realistic(realistic);
        flock.set_collisions(true);
        for x in [300.0, 320.0, 900.0, 1000.0] {
            flock.spawn(Arc::clone(&def), seed ^ x as u64, &world, Some(x));
        }
        for step in 0..(8.0 / TICK) as usize {
            if step % 97 == 0 {
                // Le curseur passe vite, on en touche un, on en lance un.
                input.cursor = Some(Vec2::new(rng.range(0.0, 1440.0), rng.range(0.0, 900.0)));
                let count = flock.pets().len().max(1);
                let id = flock
                    .pets()
                    .get(rng.next_u32() as usize % count)
                    .map(|p| p.id());
                if let Some(pet) = id.and_then(|id| flock.get_mut(id)) {
                    match rng.next_u32() % 3 {
                        0 => pet.poke(&world, &input),
                        1 => {
                            let at = pet.position();
                            pet.grab(at, &world);
                            pet.drag_to(at + Vec2::new(rng.range(-80.0, 80.0), -60.0));
                            pet.release(&world);
                        }
                        _ => {}
                    }
                }
            }
            flock.advance(TICK, &world, &input);
            for pet in flock.pets() {
                let (pos, vel) = (pet.position(), pet.velocity());
                let view = pet.view(0.5);
                assert!(
                    pos.x.is_finite()
                        && pos.y.is_finite()
                        && vel.x.is_finite()
                        && vel.y.is_finite()
                        && view.ground.x.is_finite()
                        && view.ground.y.is_finite(),
                    "{} (graine {seed}, réaliste {realistic}) : à {pos:?}, vitesse {vel:?} ({})",
                    dir.display(),
                    pet.behavior_name()
                );
            }
        }
    }
    true
}

fn damaged_pets(seeds: std::ops::Range<u64>) {
    let pets = pets();
    assert!(pets.len() >= 30, "{} animaux", pets.len());
    let (mut accepted, mut refused) = (0, 0);
    for seed in seeds {
        for (n, dir) in pets.iter().enumerate() {
            if survives(dir, seed * 257 + n as u64) {
                accepted += 1;
            } else {
                refused += 1;
            }
        }
    }
    eprintln!("{accepted} animaux abîmés acceptés et vivants, {refused} refusés");
    assert!(
        accepted > 0,
        "aucune fiche abîmée n'a été acceptée : l'essai ne dit rien"
    );
}

#[test]
fn a_damaged_pet_file_is_refused_or_lives_but_never_breaks() {
    damaged_pets(0..3);
}

#[test]
#[ignore = "essai long, à lancer à la main en --release"]
fn many_damaged_pet_files_are_refused_or_live_but_never_break() {
    damaged_pets(100..160);
}
