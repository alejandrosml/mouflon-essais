//! Les sons des animaux, par le lecteur multimédia de Windows (MCI), sur un
//! fil à part.
//!
//! Ce lecteur lit des fichiers, pas la mémoire, et veut que le même fil
//! ouvre, joue et referme. Chaque son est donc écrit une fois dans le
//! dossier temporaire, sous un nom tiré de son contenu : d'un lancement à
//! l'autre, les mêmes sons retrouvent les mêmes fichiers, et le dossier ne
//! grossit pas.

use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::mpsc::{Receiver, RecvTimeoutError, Sender, channel};
use std::sync::{Arc, Mutex, OnceLock};
use std::time::Duration;

use windows_sys::Win32::Media::Multimedia::mciSendStringW;

use super::{from_wide, wide};

/// Sons joués en même temps, au plus.
const MAX_PLAYING: usize = 6;
/// Le début du nom des fichiers de sons de cette version.
const PREFIX: &str = "son-";

/// Un son à jouer : le contenu d'un fichier audio et son volume (de 0 à 1).
struct Job {
    data: Arc<[u8]>,
    volume: f32,
}

/// Envoie une commande au lecteur multimédia de Windows (MCI) ; sa réponse.
fn mci(command: &str) -> Option<String> {
    let command = wide(command);
    let mut answer = [0u16; 128];
    // SAFETY: la commande vit jusqu'au retour ; `answer` est à nous, donné
    // avec sa taille ; pas de fenêtre à prévenir.
    let error = unsafe {
        mciSendStringW(
            command.as_ptr(),
            answer.as_mut_ptr(),
            answer.len() as u32,
            std::ptr::null_mut(),
        )
    };
    (error == 0).then(|| from_wide(&answer))
}

/// Le nom du fichier d'un son : une empreinte de son contenu (FNV-1a, 64
/// bits), et l'extension de ce qu'il est (un WAV commence par « RIFF »).
fn file_name(data: &[u8]) -> String {
    let mut hash = 0xcbf2_9ce4_8422_2325_u64;
    for byte in data {
        hash = (hash ^ u64::from(*byte)).wrapping_mul(0x0000_0100_0000_01b3);
    }
    let extension = if data.starts_with(b"RIFF") {
        "wav"
    } else {
        "mp3"
    };
    format!("{PREFIX}{hash:016x}-{}.{extension}", data.len())
}

/// Le fichier d'un son dans `dir`, écrit s'il n'y est pas déjà (entier : il
/// est d'abord écrit sous un autre nom, puis renommé).
fn file_for(dir: &Path, data: &[u8]) -> PathBuf {
    let path = dir.join(file_name(data));
    let ready = std::fs::metadata(&path).is_ok_and(|m| m.len() == data.len() as u64);
    if !ready {
        let partial = dir.join(format!("{}.{}", file_name(data), std::process::id()));
        if std::fs::write(&partial, data).is_err() || std::fs::rename(&partial, &path).is_err() {
            let _ = std::fs::remove_file(&partial);
            log::debug!("son non écrit dans {}", dir.display());
        }
    }
    path
}

/// Les fichiers de sons laissés par les versions d'avant (un jeu par
/// lancement, jamais effacé) s'en vont.
fn sweep(dir: &Path) {
    let Ok(entries) = std::fs::read_dir(dir) else {
        return;
    };
    for entry in entries.flatten() {
        if !entry.file_name().to_string_lossy().starts_with(PREFIX) {
            let _ = std::fs::remove_file(entry.path());
        }
    }
}

/// Le fil des sons.
fn play_all(jobs: &Receiver<Job>) {
    let dir = std::env::temp_dir().join("mouflon-sons");
    let _ = std::fs::create_dir_all(&dir);
    sweep(&dir);
    let mut files: HashMap<(usize, usize), PathBuf> = HashMap::new();
    let mut playing: Vec<String> = Vec::new();
    let mut next = 0u32;
    loop {
        // Tant qu'un son joue, on repasse voir s'il a fini ; sinon, on
        // attend le prochain sans se réveiller.
        let job = if playing.is_empty() {
            match jobs.recv() {
                Ok(job) => Some(job),
                Err(_) => return,
            }
        } else {
            match jobs.recv_timeout(Duration::from_millis(500)) {
                Ok(job) => Some(job),
                Err(RecvTimeoutError::Timeout) => None,
                Err(RecvTimeoutError::Disconnected) => return,
            }
        };
        // Ceux qui ont fini sont refermés.
        playing.retain(|alias| {
            let done = mci(&format!("status {alias} mode")).is_none_or(|mode| mode != "playing");
            if done {
                mci(&format!("close {alias}"));
            }
            !done
        });
        let Some(job) = job else {
            continue;
        };
        if playing.len() >= MAX_PLAYING {
            continue;
        }
        let key = (job.data.as_ptr() as usize, job.data.len());
        let path = files
            .entry(key)
            .or_insert_with(|| file_for(&dir, &job.data));
        next = next.wrapping_add(1);
        let alias = format!("mouflon{next}");
        if mci(&format!(
            "open \"{}\" type mpegvideo alias {alias}",
            path.display()
        ))
        .is_none()
        {
            continue;
        }
        let volume = (job.volume.clamp(0.0, 1.0) * 1000.0).round() as u32;
        mci(&format!("setaudio {alias} volume to {volume}"));
        if volume > 0 && mci(&format!("play {alias}")).is_some() {
            playing.push(alias);
        } else {
            mci(&format!("close {alias}"));
        }
    }
}

fn sounds() -> Option<&'static Mutex<Sender<Job>>> {
    static SOUNDS: OnceLock<Option<Mutex<Sender<Job>>>> = OnceLock::new();
    SOUNDS
        .get_or_init(|| {
            let (sender, receiver) = channel();
            std::thread::Builder::new()
                .name("mouflon-sons".into())
                .spawn(move || play_all(&receiver))
                .ok()
                .map(|_| Mutex::new(sender))
        })
        .as_ref()
}

/// Joue un son (le contenu d'un fichier MP3 ou WAV) au volume donné, sans
/// attendre.
pub fn play_sound(data: &Arc<[u8]>, volume: f32) {
    if let Some(sender) = sounds().and_then(|s| s.lock().ok()) {
        let _ = sender.send(Job {
            data: Arc::clone(data),
            volume,
        });
    }
}

/// Met le fil des sons en route (le premier son partirait sinon en retard).
pub fn warm_up_sounds(_sample: Option<&Arc<[u8]>>) {
    let _ = sounds();
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_same_sound_always_goes_to_the_same_file() {
        let bleat = file_name(b"ID3 un belement");
        assert_eq!(bleat, file_name(b"ID3 un belement"));
        assert_ne!(bleat, file_name(b"ID3 un autre belement"));
        assert!(bleat.starts_with(PREFIX) && bleat.ends_with(".mp3"));
        assert!(file_name(b"RIFF....WAVEfmt ").ends_with(".wav"));
        let dir = std::env::temp_dir().join(format!("mouflon-sons-essai-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        // Un vieux fichier d'une version d'avant s'en va, le son reste.
        std::fs::write(dir.join("1234-5678.mp3"), b"vieux").unwrap();
        let path = file_for(&dir, b"ID3 un belement");
        assert_eq!(std::fs::read(&path).unwrap(), b"ID3 un belement");
        assert_eq!(file_for(&dir, b"ID3 un belement"), path);
        sweep(&dir);
        assert!(path.exists() && !dir.join("1234-5678.mp3").exists());
        let _ = std::fs::remove_dir_all(&dir);
    }
}
