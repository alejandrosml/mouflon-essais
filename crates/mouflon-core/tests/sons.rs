//! Le pack du mouflon charge ses appels et les transmet au lecteur sans
//! ouvrir la mâchoire pour un souffle nasal.

use std::path::Path;
use std::sync::Arc;

use mouflon_core::{Input, Pet, PetEvent, Rect, Screen, TICK, World, load_pet};

fn world() -> World {
    World::new(vec![Screen {
        frame: Rect::new(0.0, 0.0, 1440.0, 900.0),
        usable: Rect::new(0.0, 25.0, 1440.0, 800.0),
        scale: 2.0,
    }])
}

#[test]
fn the_mouflons_pack_loads_its_own_alarm_and_courtship_calls() {
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../pets/mouflon-corse");
    let def = load_pet(&root.join("pet.toml")).expect("le pack se charge");
    for (behavior, name, file) in [
        ("stamp", "alarme", "mouflon-alarme.wav"),
        ("flehmen", "grondement", "mouflon-grondement.wav"),
    ] {
        let index = def
            .sounds
            .iter()
            .position(|sound| sound.name == name)
            .unwrap();
        let sound = &def.sounds[index];
        assert!(!sound.mouth, "le souffle n'ouvre pas la bouche");
        assert_eq!(sound.clips.len(), 1);
        assert_eq!(
            sound.clips[0].as_ref(),
            std::fs::read(root.join("sons").join(file)).unwrap()
        );
        let cues = &def.behaviors[def.behavior_index(behavior).unwrap()].sounds;
        assert_eq!(cues.len(), 1);
        assert_eq!(cues[0].sound, index);
        assert_eq!(cues[0].frame, None);
        assert_eq!(cues[0].chance, 1.0);
    }
}

#[test]
fn stamping_emits_one_alarm_without_opening_the_mouflons_mouth() {
    let path = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../pets/mouflon-corse/pet.toml");
    let def = Arc::new(load_pet(&path).unwrap());
    let alarm = def
        .sounds
        .iter()
        .position(|sound| sound.name == "alarme")
        .unwrap();
    let world = world();
    let mut pet = Pet::spawn(0, def, 3, &world, Some(600.0));
    for _ in 0..240 {
        pet.step(TICK, &world, &Input::default());
    }
    // Un éventuel son d'atterrissage a fini : seul le chuintement ci-dessous
    // peut maintenant rouvrir la mâchoire.
    assert!(pet.force("stand"));
    for _ in 0..120 {
        pet.step(TICK, &world, &Input::default());
    }
    pet.take_events();
    assert!(pet.force("stamp"));
    let sounds: Vec<_> = pet
        .take_events()
        .into_iter()
        .filter_map(|event| match event {
            PetEvent::Sound { sound, clip } => Some((sound, clip)),
            _ => None,
        })
        .collect();
    assert_eq!(sounds, [(alarm, 0)]);
    for _ in 0..40 {
        pet.step(TICK, &world, &Input::default());
        assert!(pet.view(1.0).pose.jaw < 0.05);
        assert!(
            !pet.take_events()
                .iter()
                .any(|event| matches!(event, PetEvent::Sound { .. }))
        );
    }
}

/// Un mouton dont le bêlement a des prises d'agneau.
fn pack_with_a_lamb_voice(identity: &str, drawings: &str) -> mouflon_core::PetDef {
    mouflon_core::parse_pet_toml(
        &format!(
            r#"
[pet]
id = "sheep"
name = "Mouton"
category = "sheep"
{identity}
[appearance]
rig = "brebis.svg"
height = 40.0
{drawings}
[sounds]
bele = {{ files = ["adulte-1.mp3", "adulte-2.mp3"], lamb = ["agneau.mp3"], volume = 0.7, mouth = true }}
pose = "pose.mp3"

[[behavior]]
name = "stand"
motion = "stand"
duration = [0.5, 1.5]
sound = "bele"
next = "stand"
"#
        ),
        |name: &str| Ok(name.as_bytes().to_vec()),
    )
    .unwrap()
}

/// Les prises d'un son, par leur nom de fichier.
fn clips(def: &mouflon_core::PetDef, name: &str) -> Vec<String> {
    def.sounds
        .iter()
        .find(|sound| sound.name == name)
        .unwrap()
        .clips
        .iter()
        .map(|clip| String::from_utf8(clip.to_vec()).unwrap())
        .collect()
}

#[test]
fn a_lamb_bleats_with_its_own_voice_when_the_pack_gives_one() {
    const LAMB: &str = "[appearance.lamb]\nrig = \"agneau.svg\"\nheight = 25.0\n";
    let def = pack_with_a_lamb_voice("", LAMB);
    // Les adultes gardent leur voix ; l'agneau dessiné a la sienne.
    assert_eq!(clips(&def, "bele"), ["adulte-1.mp3", "adulte-2.mp3"]);
    let drawn = def.variant(mouflon_core::Variant::Lamb).unwrap();
    assert_eq!(clips(drawn, "bele"), ["agneau.mp3"]);
    // Un son sans prise d'agneau ne change pas.
    assert_eq!(clips(drawn, "pose"), ["pose.mp3"]);
    // L'agneau que l'app prépare, avec ou sans dessin d'agneau.
    assert_eq!(clips(&def.as_lamb(2.0).unwrap(), "bele"), ["agneau.mp3"]);
    let plain = pack_with_a_lamb_voice("", "");
    assert_eq!(clips(&plain.as_lamb(2.0).unwrap(), "bele"), ["agneau.mp3"]);
    // Un fichier qui déclare un agneau.
    let declared = pack_with_a_lamb_voice("age = \"lamb\"", "");
    assert_eq!(clips(&declared, "bele"), ["agneau.mp3"]);
    // La voix reste la sienne (`mouth`), au même volume.
    let voice = &drawn.sounds[drawn.sounds.iter().position(|s| s.name == "bele").unwrap()];
    assert!(voice.mouth && (voice.volume - 0.7).abs() < 1e-6);
}

#[test]
fn every_breed_of_the_repository_gives_its_lamb_a_lamb_voice() {
    let pets = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../pets");
    let lamb_voice = std::fs::read(pets.join("southdown/sons/agneau-1.mp3")).unwrap();
    for entry in std::fs::read_dir(&pets).unwrap() {
        let dir = entry.unwrap().path();
        let name = dir.file_name().unwrap().to_string_lossy().into_owned();
        // Le floucat, castré, n'a pas d'agneau.
        if name == "floucat" || !dir.join("pet.toml").is_file() {
            continue;
        }
        let def = load_pet(&dir.join("pet.toml")).unwrap();
        let lamb = def.as_lamb(def.lamb_weeks()).expect(&name);
        let voice = lamb.sounds.iter().find(|s| s.name == "bele").expect(&name);
        assert_eq!(voice.clips.len(), 1, "{name}");
        assert_eq!(voice.clips[0].as_ref(), lamb_voice, "{name}");
        // Ses parents ont une autre voix.
        let adult = def.sounds.iter().find(|s| s.name == "bele").unwrap();
        assert!(
            adult.clips.iter().all(|clip| clip.as_ref() != lamb_voice),
            "{name}"
        );
    }
}
