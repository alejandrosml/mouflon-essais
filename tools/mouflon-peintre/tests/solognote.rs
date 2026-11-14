//! Repeindre les morceaux livrés doit donner exactement les mêmes PNG.

use std::collections::BTreeSet;
use std::io::Cursor;
use std::path::{Path, PathBuf};
use std::process::Command;

fn pets() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../../pets")
}

struct Temporary(PathBuf);

impl Temporary {
    fn new() -> Self {
        let unique = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .expect("horloge")
            .as_nanos();
        let path =
            std::env::temp_dir().join(format!("mouflon-peintre-{}-{unique}", std::process::id()));
        std::fs::create_dir(&path).expect("dossier temporaire");
        Self(path)
    }
}

impl Drop for Temporary {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}

#[test]
fn the_painter_reproduces_every_solognote_piece() {
    let temporary = Temporary::new();
    let out = temporary.0.join("solognote");
    let output = Command::new(env!("CARGO_BIN_EXE_mouflon-peintre"))
        .arg("--solognote")
        .arg(&out)
        .output()
        .expect("lancer le peintre");
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    assert!(
        !out.join("apercu.png").exists(),
        "l'aperçu de travail doit être demandé explicitement"
    );
    let source = pets().join("solognote").join("morceaux");
    let pngs: BTreeSet<_> = std::fs::read_dir(&source)
        .expect("morceaux livrés")
        .map(|entry| entry.expect("morceau").path())
        .filter(|path| path.extension().is_some_and(|e| e == "png"))
        // Un aperçu de travail laissé là, ignoré par git : seuls les
        // morceaux livrés servent de référence.
        .filter(|path| path.file_name().is_some_and(|name| name != "apercu.png"))
        .collect();
    assert!(pngs.len() >= 20);
    for path in pngs {
        let name = path.file_name().expect("nom du morceau");
        let bytes = std::fs::read(&path).expect("PNG livré");
        assert!(
            bytes == std::fs::read(out.join(name)).expect("PNG repeint"),
            "solognote/{} diffère du peintre",
            name.to_string_lossy()
        );
        let mut reader = png::Decoder::new(Cursor::new(&bytes))
            .read_info()
            .expect("PNG");
        let mut rgba = vec![0; reader.output_buffer_size().expect("taille PNG")];
        let info = reader.next_frame(&mut rgba).expect("pixels PNG");
        assert_eq!(info.color_type, png::ColorType::Rgba);
        assert_eq!((info.width, info.height), (1200, 900));
        let alphas: Vec<_> = rgba[..info.buffer_size()]
            .as_chunks::<4>()
            .0
            .iter()
            .map(|p| p[3])
            .collect();
        assert!(alphas.iter().any(|&a| a > 128), "morceau vide : {name:?}");
        assert!(alphas.contains(&0), "fond opaque : {name:?}");
    }
}
