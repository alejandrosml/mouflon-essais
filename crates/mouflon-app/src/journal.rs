//! Le journal du programme sur le disque, sans faire attendre personne.
//!
//! Sous Windows, le programme n'a pas de console : ce qu'il note va dans un
//! fichier. Or écrire dans un fichier peut attendre longtemps (un antivirus
//! qui relit le fichier, un disque dur qui se réveille). Un essai de la CI
//! l'a montré : une écriture de plus de deux secondes, pendant laquelle
//! l'horloge de l'écran puis le fil de l'affichage, qui notaient chacun une
//! ligne, sont restés arrêtés — et les moutons avec eux.
//!
//! Ici, celui qui note une ligne la pose dans une file et repart aussitôt ;
//! un fil à part l'écrit. Avant de quitter (ou quand une panique arrête le
//! programme), [`settle`] laisse à ce fil le temps de finir.
//!
//! (Sur Mac et sous Linux, le journal va sur la sortie d'erreur, comme
//! toujours : rien ne passe par ici.)

use std::io::{self, Write};
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::mpsc::{Receiver, SyncSender, TrySendError, sync_channel};
use std::sync::{Arc, OnceLock};
use std::time::{Duration, Instant};

/// Lignes en attente d'écriture, au plus. Au-delà (un disque arrêté pour de
/// bon), les nouvelles se perdent, et leur nombre est dit quand l'écriture
/// reprend : la mémoire ne s'emplit pas d'un journal que personne n'écrit.
const PENDING: usize = 4096;

enum Message {
    Line(Vec<u8>),
    /// Prévenir quand tout ce qui précède est écrit.
    Written(SyncSender<()>),
}

/// La file du journal, vue de ceux qui attendent qu'elle soit écrite.
#[derive(Clone, Debug)]
pub struct Queue {
    lines: SyncSender<Message>,
}

impl Queue {
    /// Attend que tout ce qui a été noté jusqu'ici soit écrit, `limit` au
    /// plus. `false` si l'écriture n'a pas fini à temps.
    pub fn settle(&self, limit: Duration) -> bool {
        let until = Instant::now() + limit;
        let (done, wait) = sync_channel(1);
        let mut ask = Message::Written(done);
        // (La file peut être pleine : la demande y entre dès qu'une place
        // se libère.)
        loop {
            match self.lines.try_send(ask) {
                Ok(()) => break,
                Err(TrySendError::Disconnected(_)) => return false,
                Err(TrySendError::Full(back)) => {
                    if Instant::now() >= until {
                        return false;
                    }
                    ask = back;
                    std::thread::sleep(Duration::from_millis(2));
                }
            }
        }
        wait.recv_timeout(until.saturating_duration_since(Instant::now()))
            .is_ok()
    }
}

/// Le bout de la file où l'on pose les lignes : ce que le journal prend pour
/// un fichier.
#[derive(Debug)]
pub struct Deferred {
    lines: SyncSender<Message>,
    lost: Arc<AtomicU64>,
}

impl Write for Deferred {
    /// Pose la ligne dans la file et rend la main : jamais d'attente. (File
    /// pleine ou fil d'écriture arrêté : la ligne est perdue, et comptée.)
    fn write(&mut self, line: &[u8]) -> io::Result<usize> {
        if self.lines.try_send(Message::Line(line.to_vec())).is_err() {
            self.lost.fetch_add(1, Ordering::Relaxed);
        }
        Ok(line.len())
    }

    /// Rien à attendre ici (le journal le demande après chaque ligne) : voir
    /// [`Queue::settle`].
    fn flush(&mut self) -> io::Result<()> {
        Ok(())
    }
}

/// Le fil d'écriture : chaque ligne, dans l'ordre où elle a été notée.
fn write_lines(mut sink: impl Write, lines: &Receiver<Message>, lost: &AtomicU64) {
    for message in lines {
        match message {
            Message::Line(line) => {
                let missing = lost.swap(0, Ordering::Relaxed);
                if missing > 0 {
                    // (D'un seul tenant : une remarque, une écriture.)
                    let note = format!(
                        "… {missing} ligne(s) du journal perdue(s) : le disque n'écrivait plus\n"
                    );
                    let _ = sink.write_all(note.as_bytes());
                }
                let _ = sink.write_all(&line);
            }
            Message::Written(done) => {
                let _ = sink.flush();
                // (Une place lui est réservée : jamais d'attente.)
                let _ = done.try_send(());
            }
        }
    }
}

/// Fait écrire dans `sink` par un fil à part, avec une file de `pending`
/// lignes. `None` si ce fil ne peut pas être lancé.
fn spawn(sink: impl Write + Send + 'static, pending: usize) -> Option<(Deferred, Queue)> {
    let (lines, taken) = sync_channel(pending);
    let lost = Arc::new(AtomicU64::new(0));
    let counted = Arc::clone(&lost);
    std::thread::Builder::new()
        .name("journal".to_owned())
        .spawn(move || write_lines(sink, &taken, &counted))
        .ok()?;
    let queue = Queue {
        lines: lines.clone(),
    };
    Some((Deferred { lines, lost }, queue))
}

/// La file du journal du programme, s'il s'écrit par un fil à part.
static QUEUE: OnceLock<Queue> = OnceLock::new();

/// Le journal du programme s'écrira dans `sink` par un fil à part : rend de
/// quoi y poser des lignes. `None` si ce fil ne peut pas être lancé.
pub fn start(sink: impl Write + Send + 'static) -> Option<Deferred> {
    let (deferred, queue) = spawn(sink, PENDING)?;
    let _ = QUEUE.set(queue);
    Some(deferred)
}

/// Avant de quitter : laisse au journal le temps de s'écrire, `limit` au
/// plus. (Sans fil d'écriture, chaque ligne est déjà partie : rien à
/// attendre.)
pub fn settle(limit: Duration) {
    if let Some(queue) = QUEUE.get() {
        queue.settle(limit);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::Mutex;
    use std::sync::mpsc::{Sender, channel};

    /// Ce qui retient les écritures d'un disque : il dit qu'une écriture
    /// commence, puis attend qu'on la laisse passer.
    #[derive(Clone)]
    struct Gate {
        started: Sender<()>,
        pass: Arc<Mutex<Receiver<()>>>,
    }

    /// Un « disque » : ce qui y est écrit se relit, et il peut retenir
    /// chaque écriture jusqu'à ce qu'on la laisse passer.
    #[derive(Clone, Default)]
    struct Disk {
        written: Arc<Mutex<Vec<u8>>>,
        gate: Option<Gate>,
    }

    impl Disk {
        fn text(&self) -> String {
            String::from_utf8(self.written.lock().unwrap().clone()).unwrap()
        }
    }

    impl Write for Disk {
        fn write(&mut self, bytes: &[u8]) -> io::Result<usize> {
            if let Some(gate) = &self.gate {
                let _ = gate.started.send(());
                let _ = gate.pass.lock().unwrap().recv();
            }
            self.written.lock().unwrap().extend_from_slice(bytes);
            Ok(bytes.len())
        }

        fn flush(&mut self) -> io::Result<()> {
            Ok(())
        }
    }

    /// Un disque qui retient ses écritures : de quoi savoir qu'une écriture
    /// a commencé, et de quoi en laisser passer.
    fn held_disk() -> (Disk, Receiver<()>, Sender<()>) {
        let (started, begun) = channel();
        let (pass, wait) = channel();
        let disk = Disk {
            written: Arc::default(),
            gate: Some(Gate {
                started,
                pass: Arc::new(Mutex::new(wait)),
            }),
        };
        (disk, begun, pass)
    }

    #[test]
    fn every_line_reaches_the_file_in_order_once_the_log_has_settled() {
        let disk = Disk::default();
        let (mut log, queue) = spawn(disk.clone(), 64).unwrap();
        let mut expected = String::new();
        for n in 0..40 {
            let line = format!("ligne {n}\n");
            log.write_all(line.as_bytes()).unwrap();
            log.flush().unwrap();
            expected.push_str(&line);
        }
        assert!(queue.settle(Duration::from_secs(5)));
        assert_eq!(disk.text(), expected);
    }

    #[test]
    fn a_disk_that_does_not_answer_keeps_nobody_waiting() {
        let (disk, begun, pass) = held_disk();
        let (mut log, queue) = spawn(disk.clone(), 64).unwrap();
        // Dix lignes notées pendant que le disque retient la première :
        // arriver jusqu'ici, c'est ne pas avoir attendu.
        for n in 0..10 {
            log.write_all(format!("ligne {n}\n").as_bytes()).unwrap();
            log.flush().unwrap();
        }
        begun.recv().unwrap();
        assert_eq!(disk.text(), "");
        // Tant que le disque ne répond pas, le journal n'est pas écrit, et
        // l'attente a une fin.
        assert!(!queue.settle(Duration::from_millis(30)));
        // Le disque répond de nouveau : tout s'écrit, dans l'ordre.
        for _ in 0..10 {
            pass.send(()).unwrap();
        }
        assert!(queue.settle(Duration::from_secs(5)));
        let expected: String = (0..10).map(|n| format!("ligne {n}\n")).collect();
        assert_eq!(disk.text(), expected);
    }

    #[test]
    fn lines_noted_while_the_queue_is_full_are_counted_as_lost() {
        let (disk, begun, pass) = held_disk();
        let (mut log, queue) = spawn(disk.clone(), 4).unwrap();
        // Le disque tient la première ligne ; quatre attendent dans la file ;
        // les cinq suivantes n'ont plus de place.
        log.write_all(b"ligne 0\n").unwrap();
        begun.recv().unwrap();
        for n in 1..10 {
            log.write_all(format!("ligne {n}\n").as_bytes()).unwrap();
        }
        // (La première écriture, la remarque, puis les quatre de la file.)
        for _ in 0..6 {
            pass.send(()).unwrap();
        }
        assert!(queue.settle(Duration::from_secs(5)));
        let text = disk.text();
        assert!(
            text.starts_with("ligne 0\n… 5 ligne(s) du journal perdue(s)"),
            "{text}"
        );
        assert!(
            text.ends_with("ligne 1\nligne 2\nligne 3\nligne 4\n"),
            "{text}"
        );
        assert!(!text.contains("ligne 5"), "{text}");
    }
}
