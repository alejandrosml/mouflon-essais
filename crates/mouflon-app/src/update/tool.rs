//! Les outils du système dont la mise à jour se sert (`tar`, `ditto`,
//! `codesign`…), lancés pour un temps compté : aucun ne peut retenir Mouflon.

use std::io::Read;
use std::path::PathBuf;
use std::process::{Child, Command, ExitStatus, Stdio};
use std::sync::mpsc::Receiver;
use std::time::{Duration, Instant};

/// Entre deux regards sur un outil qui tourne.
const STEP: Duration = Duration::from_millis(20);
/// Ce qu'on attend encore, l'outil fini, pour lire ce qu'il a écrit (un
/// programme qu'il aurait lancé peut garder ses tubes ouverts).
const LAST_WORDS: Duration = Duration::from_secs(2);

/// Ce qu'un outil a rendu. De ce qu'il écrit, le premier mégaoctet est
/// gardé : le reste est lu et laissé.
#[derive(Debug)]
pub struct Said {
    pub status: ExitStatus,
    pub stdout: String,
    pub stderr: String,
    /// Sa sortie, octet pour octet.
    pub raw: Vec<u8>,
}

/// Ce qu'on garde, au plus, de ce qu'un outil écrit.
pub const KEPT_BYTES: u64 = 1 << 20;

impl Said {
    /// Ce que l'outil a dit en échouant, en une ligne.
    pub fn complaint(&self) -> String {
        let text = if self.stderr.trim().is_empty() {
            &self.stdout
        } else {
            &self.stderr
        };
        let line = text.split_whitespace().collect::<Vec<_>>().join(" ");
        if line.is_empty() {
            self.status.to_string()
        } else {
            line
        }
    }
}

/// Pourquoi un outil n'a rien rendu.
#[derive(Debug, thiserror::Error)]
pub enum ToolError {
    #[error("{0} ne se lance pas ({1})")]
    Start(String, std::io::Error),
    #[error("{0} n'a pas fini en {1} s")]
    Late(String, u64),
}

/// Lance `command`, sans entrée, et attend sa fin au plus `limit` : passé ce
/// temps, il est arrêté. Ce qu'il écrit est lu au fur et à mesure (un outil
/// bavard ne reste pas bloqué sur un tube plein).
pub fn run(mut command: Command, limit: Duration) -> Result<Said, ToolError> {
    let name = command.get_program().to_string_lossy().into_owned();
    quiet(&mut command);
    let mut child = command
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .map_err(|e| ToolError::Start(name.clone(), e))?;
    let stdout = reader(child.stdout.take());
    let stderr = reader(child.stderr.take());
    match wait(&mut child, limit) {
        Some(status) => {
            let raw = heard(stdout);
            Ok(Said {
                status,
                stdout: String::from_utf8_lossy(&raw).into_owned(),
                stderr: String::from_utf8_lossy(&heard(stderr)).into_owned(),
                raw,
            })
        }
        None => Err(ToolError::Late(name, limit.as_secs())),
    }
}

/// Attend la fin de `child` au plus `limit` ; sinon l'arrête, et rend `None`.
pub fn wait(child: &mut Child, limit: Duration) -> Option<ExitStatus> {
    let started = Instant::now();
    loop {
        match child.try_wait() {
            Ok(Some(status)) => return Some(status),
            Ok(None) if started.elapsed() < limit => std::thread::sleep(STEP),
            _ => {
                let _ = child.kill();
                let _ = child.wait();
                return None;
            }
        }
    }
}

/// Lit un tube sur un fil à part, jusqu'à sa fermeture : ce qui a été lu
/// arrive par le canal rendu.
fn reader<R: Read + Send + 'static>(pipe: Option<R>) -> Option<Receiver<Vec<u8>>> {
    let mut pipe = pipe?;
    let (tx, rx) = std::sync::mpsc::channel();
    std::thread::Builder::new()
        .name("mouflon-outil".into())
        .spawn(move || {
            let mut said = Vec::new();
            let _ = pipe.by_ref().take(KEPT_BYTES).read_to_end(&mut said);
            let _ = tx.send(said);
            let _ = std::io::copy(&mut pipe, &mut std::io::sink());
        })
        .ok()?;
    Some(rx)
}

fn heard(reader: Option<Receiver<Vec<u8>>>) -> Vec<u8> {
    reader
        .and_then(|r| r.recv_timeout(LAST_WORDS).ok())
        .unwrap_or_default()
}

/// Sous Windows, pas de fenêtre de console le temps d'un outil.
pub fn quiet(command: &mut Command) {
    #[cfg(target_os = "windows")]
    {
        use std::os::windows::process::CommandExt;
        const CREATE_NO_WINDOW: u32 = 0x0800_0000;
        command.creation_flags(CREATE_NO_WINDOW);
    }
    #[cfg(not(target_os = "windows"))]
    let _ = command;
}

/// Un outil de Windows, par son chemin entier : un programme du même nom
/// posé ailleurs ne passe pas devant. Le dossier du système est demandé à
/// Windows, pas lu dans une variable qu'un autre programme aurait changée.
pub fn system32(name: &str) -> PathBuf {
    mouflon_platform::system_dir()
        .unwrap_or_else(|| PathBuf::from("C:\\Windows\\System32"))
        .join(name)
}

#[cfg(all(test, unix))]
mod tests {
    use super::*;

    fn sh(script: &str) -> Command {
        let mut command = Command::new("sh");
        command.args(["-c", script]);
        command
    }

    #[test]
    fn a_tool_is_heard_on_both_sides() {
        let said = run(
            sh("echo dedans; echo dehors >&2; exit 3"),
            Duration::from_secs(10),
        )
        .unwrap();
        assert_eq!(said.status.code(), Some(3));
        assert_eq!(said.stdout.trim(), "dedans");
        assert_eq!(said.complaint(), "dehors");
    }

    #[test]
    fn a_tool_that_lingers_is_stopped() {
        let started = Instant::now();
        let late = run(sh("sleep 30"), Duration::from_millis(200));
        assert!(matches!(late, Err(ToolError::Late(_, _))), "{late:?}");
        assert!(started.elapsed() < Duration::from_secs(5));
    }

    #[test]
    fn a_talkative_tool_does_not_block() {
        // Plus qu'un tube n'en retient : il faut lire pendant qu'il écrit.
        let said = run(
            sh("head -c 3000000 /dev/zero | tr '\\0' a; echo fin >&2"),
            Duration::from_secs(20),
        )
        .unwrap();
        assert!(said.status.success());
        assert_eq!(said.raw.len() as u64, KEPT_BYTES);
        assert_eq!(said.stderr.trim(), "fin");
    }

    #[test]
    fn a_missing_tool_is_said() {
        let missing = run(
            Command::new("mouflon-outil-qui-n-existe-pas"),
            Duration::from_secs(1),
        );
        assert!(
            matches!(missing, Err(ToolError::Start(_, _))),
            "{missing:?}"
        );
    }
}
