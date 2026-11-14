//! Sons des animaux : NSSound joue les fichiers que macOS sait lire (MP3,
//! AAC, WAV, AIFF…), directement depuis la mémoire.
//!
//! Tout se passe sur un fil à part : préparer un son (le décodeur, la sortie
//! audio au premier son) prend de quelques millisecondes à une fraction de
//! seconde, que l'animation ne doit pas attendre.

use std::sync::mpsc::{Receiver, RecvTimeoutError, Sender, channel};
use std::sync::{Arc, Mutex, OnceLock};
use std::time::Duration;

use objc2::AllocAnyThread;
use objc2::rc::{Retained, autoreleasepool};
use objc2_app_kit::NSSound;
use objc2_foundation::NSData;

/// Sons joués en même temps, au plus : au-delà, un nouveau son est ignoré
/// (un troupeau qui bêle d'un coup reste supportable).
const MAX_PLAYING: usize = 6;

/// Un son à jouer : le contenu d'un fichier audio et son volume (de 0 à 1).
struct Job {
    data: Arc<[u8]>,
    volume: f32,
}

/// Fil des sons, lancé au premier son.
fn sounds() -> Option<&'static Mutex<Sender<Job>>> {
    static SOUNDS: OnceLock<Option<Mutex<Sender<Job>>>> = OnceLock::new();
    SOUNDS
        .get_or_init(|| {
            let (sender, receiver) = channel();
            match std::thread::Builder::new()
                .name("mouflon-sons".into())
                .spawn(move || play_all(&receiver))
            {
                Ok(_) => Some(Mutex::new(sender)),
                Err(e) => {
                    log::error!("impossible de lancer le fil des sons : {e}");
                    None
                }
            }
        })
        .as_ref()
}

/// Joue un son (contenu d'un fichier audio) au volume donné (de 0 à 1), sans
/// attendre. Un son que macOS ne sait pas lire, ou de trop quand six jouent
/// déjà, est ignoré.
pub fn play_sound(data: &Arc<[u8]>, volume: f32) {
    send(Arc::clone(data), volume.clamp(0.0, 1.0));
}

/// Met la sortie audio en route en jouant sans volume `sample` (un son des
/// animaux, pour que son décodeur soit chargé aussi), ou un court silence :
/// le premier son joué par l'app charge tout le nécessaire, et le premier
/// bêlement partirait sinon en retard.
pub fn warm_up_sounds(sample: Option<&Arc<[u8]>>) {
    let data = sample.map_or_else(|| Arc::from(silence()), Arc::clone);
    send(data, 0.0);
}

fn send(data: Arc<[u8]>, volume: f32) {
    if let Some(sounds) = sounds()
        && let Ok(sender) = sounds.lock()
    {
        let _ = sender.send(Job { data, volume });
    }
}

/// Le fil des sons : joue ce qu'on lui envoie, et garde chaque son en vie
/// jusqu'à la fin de sa lecture.
fn play_all(jobs: &Receiver<Job>) {
    let mut playing: Vec<Retained<NSSound>> = Vec::new();
    loop {
        let job = match jobs.recv_timeout(Duration::from_millis(500)) {
            Ok(job) => Some(job),
            Err(RecvTimeoutError::Timeout) => None,
            Err(RecvTimeoutError::Disconnected) => return,
        };
        autoreleasepool(|_| {
            playing.retain(|sound| sound.isPlaying());
            let Some(job) = job else {
                return;
            };
            if playing.len() >= MAX_PLAYING {
                return;
            }
            let data = NSData::with_bytes(&job.data);
            let Some(sound) = NSSound::initWithData(NSSound::alloc(), &data) else {
                log::warn!("son illisible ({} octets)", job.data.len());
                return;
            };
            sound.setVolume(job.volume);
            if sound.play() {
                playing.push(sound);
            }
        });
    }
}

/// Un vingtième de seconde de silence, en WAV (PCM 16 bits mono, 8 kHz).
fn silence() -> Vec<u8> {
    const RATE: u32 = 8_000;
    const SAMPLES: u32 = RATE / 20;
    let data = SAMPLES * 2;
    let mut wav = Vec::with_capacity(44 + data as usize);
    wav.extend_from_slice(b"RIFF");
    wav.extend_from_slice(&(36 + data).to_le_bytes());
    wav.extend_from_slice(b"WAVEfmt ");
    wav.extend_from_slice(&16u32.to_le_bytes());
    wav.extend_from_slice(&1u16.to_le_bytes()); // PCM
    wav.extend_from_slice(&1u16.to_le_bytes()); // mono
    wav.extend_from_slice(&RATE.to_le_bytes());
    wav.extend_from_slice(&(RATE * 2).to_le_bytes());
    wav.extend_from_slice(&2u16.to_le_bytes());
    wav.extend_from_slice(&16u16.to_le_bytes());
    wav.extend_from_slice(b"data");
    wav.extend_from_slice(&data.to_le_bytes());
    wav.resize(44 + data as usize, 0);
    wav
}
