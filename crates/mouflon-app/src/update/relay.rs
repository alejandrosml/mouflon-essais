//! Le passage d'une version à la suivante.
//!
//! Le Mouflon qui tourne fait l'échange (`home`), lance la nouvelle version
//! et **veille** : il ne s'en va que lorsqu'elle a donné signe de vie. Si
//! elle ne démarre pas, il l'arrête, remet l'ancienne à sa place et la
//! version est notée comme refusée : Mouflon ne la réessaiera pas.
//!
//! La version lancée par un relais le sait par son premier argument
//! ([`ARRIVING`], ou [`RETURNING`] quand c'est l'ancienne qui revient).

use std::path::{Path, PathBuf};
use std::process::{Child, Command};
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::{Duration, Instant};

use super::home::{Layout, Ready};
use super::release::Version;
use super::state::{Dirs, State, write_whole};

/// Le premier argument d'un Mouflon lancé par un relais.
const ARRIVING: &str = "--relais=arrivee";
const RETURNING: &str = "--relais=retour";

/// Ce qu'on laisse à la nouvelle version pour donner signe de vie (elle le
/// fait dès qu'un animal est là : quelques secondes d'ordinaire ; large,
/// parce qu'une version refusée à tort ne serait jamais installée).
const PATIENCE: Duration = Duration::from_secs(90);
const STEP: Duration = Duration::from_millis(100);

/// Ce Mouflon vient d'arriver par un relais et n'a pas encore donné signe
/// de vie.
static ON_TRIAL: AtomicBool = AtomicBool::new(false);

/// Vrai tant que ce Mouflon, lancé par la version d'avant, n'a pas donné
/// signe de vie : s'il lui arrive malheur d'ici là, il doit s'arrêter, pour
/// qu'elle reprenne sa place.
pub fn on_trial() -> bool {
    ON_TRIAL.load(Ordering::Relaxed)
}

/// Comment ce Mouflon a été lancé.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Arrival {
    /// Par la version d'avant, qui attend son signe de vie.
    New,
    /// Par une version qui n'a pas pu laisser la place : c'est le même
    /// Mouflon qui revient.
    Back,
}

impl Arrival {
    /// Lu dans le premier argument du programme.
    pub fn read(first: Option<&std::ffi::OsStr>) -> Option<Arrival> {
        match first?.to_str()? {
            ARRIVING => Some(Arrival::New),
            RETURNING => Some(Arrival::Back),
            _ => None,
        }
    }

    /// La version qui arrive laisse son numéro de processus : sur Mac, c'est
    /// par lui que l'ancienne peut l'arrêter si elle ne répond pas.
    pub fn announce(self, dirs: &Dirs) {
        if self == Arrival::New {
            ON_TRIAL.store(true, Ordering::Relaxed);
            let _ = write_whole(&dirs.pid_file(), std::process::id().to_string().as_bytes());
        }
    }

    /// Le signe de vie : ce Mouflon a démarré, l'ancienne version peut s'en
    /// aller.
    pub fn confirm(self, dirs: &Dirs) {
        if self == Arrival::New {
            let said = Version::current().to_string();
            match write_whole(&dirs.alive_file(), said.as_bytes()) {
                Ok(()) => log::info!("mise à jour : Mouflon {said} a pris le relais"),
                Err(e) => log::warn!("mise à jour : signe de vie impossible ({e})"),
            }
            ON_TRIAL.store(false, Ordering::Relaxed);
        }
    }
}

/// Ce qu'est devenu le relais.
#[derive(Debug)]
pub enum Outcome {
    /// La nouvelle version tourne : ce Mouflon-ci n'a plus qu'à s'arrêter.
    Relayed,
    /// Elle n'a pas pris la place : ce Mouflon-ci est resté (ou revenu) tel
    /// qu'il était.
    Stayed(Stayed),
}

/// Le Mouflon resté en place, et pourquoi.
#[derive(Debug)]
pub struct Stayed {
    pub why: String,
    program: PathBuf,
    layout: Layout,
}

impl Stayed {
    /// Relance ce Mouflon (son affichage est déjà défait) : il revient
    /// comme il était.
    pub fn restart(&self) -> std::io::Result<()> {
        // (Sur Mac, `open` rend la main tout de suite ; ailleurs, le
        // programme lancé vit sa vie.)
        launch(&self.program, self.layout, RETURNING, false).map(drop)
    }
}

/// Fait l'échange, lance la nouvelle version et veille sur son démarrage.
pub fn go(ready: Ready, dirs: &Dirs) -> Outcome {
    go_for(ready, dirs, PATIENCE)
}

fn go_for(ready: Ready, dirs: &Dirs, patience: Duration) -> Outcome {
    let version = ready.version;
    let (program, layout) = (ready.program(), ready.layout());
    let swapped = match ready.swap() {
        Ok(swapped) => swapped,
        Err(e) => {
            State::failed(dirs, version);
            return Outcome::Stayed(stayed(&program, layout, e.to_string()));
        }
    };
    let _ = std::fs::remove_file(dirs.alive_file());
    let _ = std::fs::remove_file(dirs.pid_file());
    log::info!("mise à jour : Mouflon {version} est en place, on le lance");
    let why = match launch(&program, layout, ARRIVING, true) {
        Ok(mut child) => {
            if alive(dirs, &mut child, patience) {
                // (Sur Mac, ce qui attendait la fin de l'app n'a plus à
                // attendre ; ailleurs, c'est l'app : on la laisse.)
                if layout == Layout::MacApp {
                    let _ = child.kill();
                    let _ = child.wait();
                }
                State::installed(dirs, version);
                swapped.settle();
                let _ = std::fs::remove_file(dirs.alive_file());
                let _ = std::fs::remove_file(dirs.pid_file());
                let _ = std::fs::remove_dir_all(&dirs.cache);
                return Outcome::Relayed;
            }
            stop(&mut child, dirs);
            "elle n'a pas démarré".to_owned()
        }
        Err(e) => format!("elle ne se lance pas ({e})"),
    };
    if let Err(e) = swapped.undo() {
        log::error!("mise à jour : l'ancienne version n'a pas repris toute sa place ({e})");
    }
    let _ = std::fs::remove_file(dirs.pid_file());
    // Une fois, c'est peut-être le moment ; deux, c'est elle : refusée.
    if State::failed(dirs, version) {
        log::warn!("mise à jour : Mouflon {version} ne sera pas réessayé");
    }
    Outcome::Stayed(stayed(&program, layout, why))
}

fn stayed(program: &Path, layout: Layout, why: String) -> Stayed {
    Stayed {
        why,
        program: program.to_path_buf(),
        layout,
    }
}

/// Lance le Mouflon posé à `program` avec `arg`. `watched` : on veut savoir
/// quand il s'arrête.
fn launch(program: &Path, layout: Layout, arg: &str, watched: bool) -> std::io::Result<Child> {
    let mut command = match layout {
        // Par `open`, comme le Finder : macOS reconnaît l'app, avec ses
        // autorisations. `-n` : une de plus (l'ancienne tourne encore) ;
        // `-g` : sans passer devant l'app où l'on travaille ; `-W` : `open`
        // reste tant qu'elle tourne, et nous dit ainsi sa fin.
        Layout::MacApp => {
            let mut open = Command::new("/usr/bin/open");
            open.args(["-n", "-g"]);
            if watched {
                open.arg("-W");
            }
            open.arg(program).arg("--args").arg(arg);
            open
        }
        Layout::Folder { .. } => {
            let mut direct = Command::new(program);
            direct.arg(arg);
            direct
        }
    };
    command.stdin(std::process::Stdio::null()).spawn()
}

/// Attend le signe de vie de la version lancée : vrai s'il est venu ; faux
/// si elle s'est arrêtée avant, ou n'a rien dit en `patience`.
fn alive(dirs: &Dirs, child: &mut Child, patience: Duration) -> bool {
    let file = dirs.alive_file();
    let started = Instant::now();
    loop {
        if file.is_file() {
            return true;
        }
        let gone = !matches!(child.try_wait(), Ok(None));
        if gone || started.elapsed() >= patience {
            // (Un dernier regard : le signe a pu arriver à l'instant.)
            return file.is_file();
        }
        std::thread::sleep(STEP);
    }
}

/// Arrête la version lancée, si elle tourne encore.
fn stop(child: &mut Child, dirs: &Dirs) {
    let _ = child.kill();
    let _ = child.wait();
    // Sur Mac, on n'a arrêté que `open` : l'app, elle, a laissé son numéro.
    // Si elle n'en a pas eu le temps, on arrête tout Mouflon de ce compte
    // lancé par un relais (il n'y en a pas d'autre que celui-là).
    if cfg!(target_os = "macos") {
        let pid = std::fs::read_to_string(dirs.pid_file())
            .ok()
            .and_then(|pid| pid.trim().parse::<u32>().ok())
            .filter(|pid| *pid > 1);
        let kill = match pid {
            Some(pid) => {
                let mut kill = Command::new("/bin/kill");
                kill.arg("-9").arg(pid.to_string());
                kill
            }
            None => {
                let mut kill = Command::new("/usr/bin/pkill");
                kill.args(["-9", "-f", "--", ARRIVING]);
                kill
            }
        };
        let _ = super::tool::run(kill, Duration::from_secs(5));
        std::thread::sleep(Duration::from_millis(300));
    }
}

#[cfg(all(test, unix))]
mod tests {
    use super::*;
    use crate::update::state::tests::{dirs, scratch};

    fn sh(script: &str) -> Child {
        Command::new("sh").args(["-c", script]).spawn().unwrap()
    }

    #[test]
    fn a_relay_is_read_in_the_first_argument_only() {
        let read = |text: &str| Arrival::read(Some(std::ffi::OsStr::new(text)));
        assert_eq!(read("--relais=arrivee"), Some(Arrival::New));
        assert_eq!(read("--relais=retour"), Some(Arrival::Back));
        assert_eq!(read("--relais"), None);
        assert_eq!(read("pets/texel"), None);
        assert_eq!(Arrival::read(None), None);
    }

    #[test]
    fn only_a_new_version_gives_a_sign_of_life() {
        let root = scratch("signe");
        let dirs = dirs(&root);
        Arrival::Back.announce(&dirs);
        Arrival::Back.confirm(&dirs);
        assert!(!dirs.alive_file().exists() && !dirs.pid_file().exists());
        Arrival::New.announce(&dirs);
        Arrival::New.confirm(&dirs);
        assert_eq!(
            std::fs::read_to_string(dirs.pid_file()).unwrap(),
            std::process::id().to_string()
        );
        assert_eq!(
            std::fs::read_to_string(dirs.alive_file()).unwrap(),
            Version::current().to_string()
        );
        let _ = std::fs::remove_dir_all(root);
    }

    use crate::update::home::tests::{
        Arrives, NEW, age_record, hidden, installed, package, prepared, snapshot,
    };
    use crate::update::home::{Lock, Tidy, VERSION_ARG};

    fn says(program: &Path) -> String {
        let out = Command::new(program).arg(VERSION_ARG).output().unwrap();
        String::from_utf8_lossy(&out.stdout).trim().to_owned()
    }

    #[test]
    fn a_version_that_starts_takes_over_and_the_old_one_goes() {
        let root = scratch("relais");
        let dirs = dirs(&root);
        let home = installed(&root);
        let package = package(&root, "Mouflon 9.9.9", Arrives::Well(&dirs.alive_file()));
        std::fs::create_dir_all(&dirs.cache).unwrap();
        std::fs::write(dirs.cache.join("Mouflon-9.9.9-fichier"), b"x").unwrap();
        State::change(&dirs, |s| {
            s.postponed = Some(NEW);
            s.failed = vec![NEW];
        });
        let ready = prepared(&home, &root, &package).unwrap();
        let program = ready.program();
        let folder = program.parent().unwrap().to_path_buf();

        assert!(matches!(go(ready, &dirs), Outcome::Relayed));
        assert_eq!(says(&program), "Mouflon 9.9.9");
        let state = State::load(&dirs);
        assert_eq!((state.installed, state.postponed), (Some(NEW), None));
        assert!(state.refused.is_empty() && state.failed.is_empty());
        // Ni la préparation, ni le téléchargement, ni les signes du relais
        // ne restent ; le verrou est rendu. L'ancienne version, elle, attend
        // encore un moment : c'est la nouvelle qui l'enlèvera.
        assert_eq!(hidden(&folder), [".mouflon-ancien"]);
        assert!(!dirs.cache.exists());
        assert!(!dirs.alive_file().exists() && !dirs.pid_file().exists());
        assert!(Lock::take(&dirs).is_some());
        assert_eq!(home.tidy(NEW), Tidy::Later);
        age_record(&folder, Duration::from_secs(600));
        assert_eq!(home.tidy(NEW), Tidy::Clean);
        assert!(hidden(&folder).is_empty());
        assert_eq!(says(&program), "Mouflon 9.9.9");
        let _ = std::fs::remove_dir_all(root);
    }

    #[test]
    fn a_version_that_does_not_start_gives_the_place_back() {
        for (name, arrives, patience) in [
            ("relais-arret", Arrives::Badly, Duration::from_secs(30)),
            ("relais-muet", Arrives::Mute, Duration::from_millis(500)),
        ] {
            let root = scratch(name);
            let dirs = dirs(&root);
            let home = installed(&root);
            let package = package(&root, "Mouflon 9.9.9", arrives);
            let ready = prepared(&home, &root, &package).unwrap();
            let program = ready.program();
            let folder = program.parent().unwrap().to_path_buf();
            let before: Vec<_> = snapshot(&folder)
                .into_iter()
                .filter(|(path, _)| !path.starts_with('.'))
                .collect();

            let started = Instant::now();
            let Outcome::Stayed(stayed) = go_for(ready, &dirs, patience) else {
                panic!("{name} : elle n'a pas pu prendre le relais");
            };
            assert!(started.elapsed() < Duration::from_secs(20), "{name}");
            // Tout est comme avant, sans reste, et la version est refusée.
            assert_eq!(snapshot(&folder), before, "{name} : {}", stayed.why);
            assert_eq!(says(&program), "Mouflon 1.0.0");
            // Une fois : elle n'est pas encore refusée.
            let state = State::load(&dirs);
            assert_eq!((state.installed, state.failed), (None, vec![NEW]));
            assert!(state.refused.is_empty());
            assert!(Lock::take(&dirs).is_some());
            // Le Mouflon d'avant se relance, et sait qu'il revient.
            stayed.restart().unwrap();
            let back = folder.join("retour");
            let waited = Instant::now();
            while !back.exists() && waited.elapsed() < Duration::from_secs(10) {
                std::thread::sleep(STEP);
            }
            assert!(back.exists(), "{name}");
            let _ = std::fs::remove_dir_all(root);
        }
    }

    #[test]
    fn an_exchange_that_cannot_be_done_leaves_mouflon_as_it_was() {
        let root = scratch("relais-sans-echange");
        let dirs = dirs(&root);
        let home = installed(&root);
        let package = package(&root, "Mouflon 9.9.9", Arrives::Well(&dirs.alive_file()));
        let ready = prepared(&home, &root, &package).unwrap();
        let program = ready.program();
        let folder = program.parent().unwrap().to_path_buf();
        // Ce qui était préparé a disparu entre-temps.
        std::fs::remove_dir_all(folder.join(".mouflon-maj")).unwrap();
        let before = snapshot(&folder);
        let Outcome::Stayed(stayed) = go(ready, &dirs) else {
            panic!("rien n'était prêt");
        };
        assert_eq!(snapshot(&folder), before, "{}", stayed.why);
        assert_eq!(State::load(&dirs).failed, vec![NEW]);
        assert!(!dirs.alive_file().exists());
        let _ = std::fs::remove_dir_all(root);
    }

    #[test]
    fn the_watch_ends_on_the_sign_of_life() {
        let root = scratch("veille");
        let dirs = dirs(&root);
        std::fs::create_dir_all(&dirs.state).unwrap();
        let alive_file = dirs.alive_file();
        let mut child = sh(&format!(
            "sleep 0.3; echo 9.9.9 > '{}'; sleep 30",
            alive_file.display()
        ));
        let started = Instant::now();
        assert!(alive(&dirs, &mut child, Duration::from_secs(20)));
        assert!(started.elapsed() < Duration::from_secs(10));
        // Elle tourne encore : c'est elle, maintenant, Mouflon.
        assert!(matches!(child.try_wait(), Ok(None)));
        stop(&mut child, &dirs);
        let _ = std::fs::remove_dir_all(root);
    }

    #[test]
    fn a_version_that_dies_or_says_nothing_is_not_alive() {
        let root = scratch("veille-sans-signe");
        let dirs = dirs(&root);
        std::fs::create_dir_all(&dirs.state).unwrap();
        // Arrêtée aussitôt : on le sait sans attendre le bout de la patience.
        let mut dead = sh("exit 3");
        let started = Instant::now();
        assert!(!alive(&dirs, &mut dead, Duration::from_secs(30)));
        assert!(started.elapsed() < Duration::from_secs(10));
        // Muette : on attend, pas plus que prévu, puis on l'arrête.
        let mut mute = sh("sleep 30");
        assert!(!alive(&dirs, &mut mute, Duration::from_millis(400)));
        stop(&mut mute, &dirs);
        assert!(matches!(mute.try_wait(), Ok(Some(_))));
        let _ = std::fs::remove_dir_all(root);
    }
}
