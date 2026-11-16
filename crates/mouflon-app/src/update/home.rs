//! Là où ce Mouflon est installé, et comment une version y prend la place
//! d'une autre.
//!
//! Sur Mac, c'est l'app (`Mouflon.app`) qui est remplacée, d'un seul geste.
//! Sous Windows et Linux, c'est le dossier décompressé : le programme, et
//! avec lui ce qui l'accompagne quand ses animaux sont à côté de lui (le
//! dossier `pets`). Chaque chose de la nouvelle version remplace celle du
//! même nom ; dans `pets`, chaque animal remplace le sien, et ce que
//! l'utilisateur y a posé n'est pas touché. Un programme sorti seul de son
//! dossier est remplacé seul.
//!
//! Tout se prépare à côté (`.mouflon-maj`), dans le même dossier : l'échange
//! n'est plus alors qu'une suite de renommages, que l'on sait défaire.
//! L'ancienne version attend dans `.mouflon-ancien`, avec la note de
//! l'échange ([`Record`]) : si tout s'arrête au milieu (une coupure de
//! courant), le prochain Mouflon lancé la lit, et finit ou défait
//! ([`Home::tidy`]).

use std::path::{Path, PathBuf};
use std::process::Command;
use std::time::{Duration, SystemTime};

use super::release::{File, Version};
use super::state::{Dirs, write_whole};
use super::tool::{self, ToolError};

/// Ce qui se prépare, ce qui attend, ce qui est renvoyé.
const STAGING: &str = ".mouflon-maj";
const BACKUP: &str = ".mouflon-ancien";
const REFUSED: &str = ".mouflon-refuse";
/// La note de l'échange, dans `.mouflon-ancien` ; et, à côté d'elle, ce qui
/// dit que l'échange est fini (voir [`Record`]). Des noms qu'aucune place
/// du dossier ne peut porter (voir [`plain`]).
const RECORD: &str = ".mouflon-echange.txt";
const DONE: &str = ".mouflon-fait";
const DONE_READY: &str = ".mouflon-fait.pret";
/// Dans `.mouflon-maj` : le fichier de la version, le temps de l'ouvrir.
const PACKAGE: &str = "paquet";
/// Le dossier des animaux livrés, à côté du programme.
const PETS: &str = "pets";
/// Dans le dossier d'une version (Windows, Linux) : ce qu'elle ne livre
/// plus, et qu'il faut enlever en l'installant (voir [`removals`]).
const REMOVALS: &str = "a-retirer.txt";

/// Ce qu'on demande à un programme pour qu'il dise sa version et s'arrête.
pub const VERSION_ARG: &str = "--version";

/// Ce qu'on laisse à un outil du système, et à la nouvelle version pour
/// dire son numéro.
const TOOL_TIME: Duration = Duration::from_secs(180);
const HELLO_TIME: Duration = Duration::from_secs(30);
/// Une ancienne version reste à côté de la nouvelle au moins ce temps-là
/// après l'échange : ce qui vient d'être écrit est alors sûrement sur le
/// disque, même si le courant tombe.
const KEPT_FOR: Duration = Duration::from_secs(120);

/// Ce que répond ce Mouflon à [`VERSION_ARG`]. Un programme d'essai le dit :
/// le vrai Mouflon ne prend jamais sa place, ni lui celle du vrai.
pub fn banner(version: Version) -> String {
    if cfg!(feature = "essai-maj") {
        format!("Mouflon {version} (essai)")
    } else {
        format!("Mouflon {version}")
    }
}

/// La forme d'un Mouflon installé.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Layout {
    /// Une app de Mac : `Mouflon.app`, le programme dedans.
    MacApp,
    /// Un dossier `Mouflon`, le programme à sa racine.
    Folder { program: &'static str },
}

impl Layout {
    pub const fn current() -> Layout {
        if cfg!(target_os = "macos") {
            Layout::MacApp
        } else if cfg!(target_os = "windows") {
            Layout::Folder {
                program: "Mouflon.exe",
            }
        } else {
            Layout::Folder { program: "mouflon" }
        }
    }

    /// Dans ce qu'on a décompressé : ce qui porte tout, et le programme.
    fn unpacked(self, staging: &Path) -> (PathBuf, PathBuf) {
        match self {
            Layout::MacApp => {
                let app = staging.join("Mouflon.app");
                let program = app.join("Contents").join("MacOS").join("mouflon");
                (app, program)
            }
            Layout::Folder { program } => {
                let root = staging.join("Mouflon");
                let program = root.join(program);
                (root, program)
            }
        }
    }
}

/// Ce que ce Mouflon peut faire d'une version plus récente.
#[derive(Debug)]
pub enum Standing {
    /// Se remplacer lui-même.
    Own(Home),
    /// Dire seulement qu'elle existe : il n'a pas le droit d'écrire là où il
    /// est, ou macOS le fait tourner depuis une copie de passage.
    Tell,
    /// Rien : une construction de travail, ou une app de l'App Store (qui
    /// s'en charge).
    Never,
}

/// Ce que le ménage a laissé (voir [`Home::tidy`]).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Tidy {
    /// Plus rien d'une mise à jour passée.
    Clean,
    /// L'ancienne version d'un échange réussi attend encore : à refaire
    /// plus tard.
    Later,
    /// Une ancienne version n'a pas pu reprendre toute sa place, et reste à
    /// côté : rien ne s'installe tant que c'est ainsi.
    Stuck,
}

/// L'endroit où ce Mouflon est installé.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Home {
    layout: Layout,
    /// Le dossier où se font les échanges : celui qui contient l'app (Mac)
    /// ou le programme (ailleurs).
    dir: PathBuf,
    /// Le nom de l'app ou du programme dans ce dossier, tel que
    /// l'utilisateur l'a peut-être renommé.
    name: String,
    /// Ailleurs que sur Mac : le dossier est celui de Mouflon (ses animaux
    /// sont à côté du programme), et ce qu'une version livre s'y remplace.
    whole: bool,
}

/// Pourquoi une version n'a pas pris la place.
#[derive(Debug, thiserror::Error)]
pub enum InstallError {
    #[error("{0}")]
    Tool(#[from] ToolError),
    #[error("{0} : {1}")]
    Disk(&'static str, std::io::Error),
    #[error("le fichier ne se décompresse pas ({0})")]
    Unpack(String),
    #[error("le fichier téléchargé a changé depuis sa vérification")]
    Changed,
    /// Ce Mouflon-ci ne se prête pas à la comparaison (déplacé pendant
    /// qu'il tourne, par exemple) : la version n'y est pour rien.
    #[cfg_attr(not(target_os = "macos"), allow(dead_code))]
    #[error("{0}")]
    Own(String),
    #[error("le fichier ne contient pas Mouflon")]
    Layout,
    #[error("{0}")]
    NotIt(String),
    #[error("la nouvelle version ne démarre pas ici ({0})")]
    Start(String),
}

/// Ce qu'un refus dit de la version.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Blame {
    /// Rien : c'est le moment qui ne va pas (le disque, un outil). On
    /// réessaiera.
    Moment,
    /// Peut-être : elle n'a pas su démarrer cette fois. Une fois de plus,
    /// et elle sera refusée.
    Attempt,
    /// Tout : ce n'est pas le Mouflon annoncé. Refusée.
    Version,
}

impl InstallError {
    pub fn blame(&self) -> Blame {
        match self {
            InstallError::Layout | InstallError::NotIt(_) => Blame::Version,
            InstallError::Start(_) => Blame::Attempt,
            InstallError::Tool(_)
            | InstallError::Disk(..)
            | InstallError::Unpack(_)
            | InstallError::Changed
            | InstallError::Own(_) => Blame::Moment,
        }
    }
}

fn disk(what: &'static str) -> impl FnOnce(std::io::Error) -> InstallError {
    move |e| InstallError::Disk(what, e)
}

/// Une chose à échanger : ce qui arrive (rien, pour ce que la nouvelle
/// version retire), et la place que cela prend dans le dossier (un nom, ou
/// `pets/<nom>`).
#[derive(Debug, Clone, PartialEq, Eq)]
struct Move {
    from: Option<PathBuf>,
    to: String,
}

impl Home {
    /// Où est ce Mouflon, et ce qu'il peut y faire.
    pub fn find() -> Standing {
        let Ok(exe) = std::env::current_exe() else {
            return Standing::Never;
        };
        // (Sous Windows, le chemin est déjà entier ; le « canoniser » lui
        // donnerait une forme que les outils du système lisent mal.)
        let exe = if cfg!(windows) {
            exe
        } else {
            exe.canonicalize().unwrap_or(exe)
        };
        let working = cfg!(debug_assertions) || in_cargo_target(&exe);
        Home::of(&exe, Layout::current(), working)
    }

    /// Ce que peut faire le Mouflon dont le programme est `exe`. `working` :
    /// c'est une construction de travail.
    fn of(exe: &Path, layout: Layout, working: bool) -> Standing {
        let (own, whole) = match layout {
            Layout::MacApp => {
                let Some(app) = bundle_of(exe) else {
                    return Standing::Never;
                };
                if store_copy(&app) || !mac::replaces_itself(&app) {
                    return Standing::Never;
                }
                if app.to_string_lossy().contains("/AppTranslocation/") {
                    return Standing::Tell;
                }
                (app, false)
            }
            Layout::Folder { .. } => {
                if working {
                    return Standing::Never;
                }
                let whole = exe.parent().is_some_and(|dir| dir.join(PETS).is_dir());
                (exe.to_path_buf(), whole)
            }
        };
        let Some(dir) = own.parent() else {
            return Standing::Never;
        };
        // Un nom que la note de l'échange ne saurait pas écrire : Mouflon
        // dira seulement qu'une version existe.
        let Some(name) = own
            .file_name()
            .and_then(|n| n.to_str())
            .filter(|n| plain(n) && !same_name(n, PETS))
        else {
            return Standing::Tell;
        };
        let home = Home {
            layout,
            dir: dir.to_path_buf(),
            name: name.to_owned(),
            whole,
        };
        if home.writable() {
            Standing::Own(home)
        } else {
            Standing::Tell
        }
    }

    /// Peut-on préparer une version ici ? (On essaie : c'est la seule
    /// réponse sûre, sur les trois systèmes.)
    fn writable(&self) -> bool {
        let probe = self.dir.join(format!("{STAGING}-{}", std::process::id()));
        let made = std::fs::create_dir(&probe).is_ok();
        let _ = std::fs::remove_dir(&probe);
        made
    }

    /// Le ménage, à faire en tenant le verrou : un échange resté en plan est
    /// défait (voir [`Record`]), l'ancienne version d'un échange fini s'en
    /// va quand c'est la nouvelle qui tourne (`own` : la version de ce
    /// Mouflon-ci) et qu'un moment a passé, puis ce qu'une mise à jour passée
    /// a laissé est enlevé.
    pub fn tidy(&self, own: Version) -> Tidy {
        let backup = self.dir.join(BACKUP);
        let is_dir = backup.symlink_metadata().is_ok_and(|meta| meta.is_dir());
        if is_dir {
            match Record::read(&backup) {
                // L'échange est fini : la nouvelle version est en place. Si
                // c'est elle qui tourne (ou une plus récente encore),
                // l'ancienne s'en va, mais pas tout de suite ; un Mouflon
                // d'une version d'avant, encore en marche, n'y touche pas.
                Some(record) if Record::finished(&backup) => {
                    if own < record.version || !Record::finished_for(&backup, KEPT_FOR) {
                        return Tidy::Later;
                    }
                    log::info!("mise à jour : l'ancienne version s'en va");
                    // D'un seul geste : ce qui part ainsi ne sera jamais
                    // pris pour quelque chose à remettre.
                    let refused = self.dir.join(REFUSED);
                    if clear(&refused)
                        .and_then(|()| rename(&backup, &refused))
                        .is_err()
                    {
                        return Tidy::Later;
                    }
                }
                // Un échange ou un retour interrompus : l'ancienne version
                // reprend sa place, entière.
                Some(record) => {
                    log::warn!(
                        "mise à jour : l'installation de Mouflon {} s'était arrêtée en route, l'ancienne version reprend sa place",
                        record.version
                    );
                    if let Err(e) = restore(&self.dir, &record.moves) {
                        // Tout garder : ce qui manque est encore là-dedans.
                        log::error!("mise à jour : elle ne l'a pas toute reprise ({e})");
                        return Tidy::Stuck;
                    }
                }
                // Pas de note lisible : rien de ce qui attend là et manque
                // dans le dossier n'est jeté.
                None => {
                    if let Err(e) = rescue(&self.dir, &backup) {
                        log::error!(
                            "mise à jour : {} garde ce qui n'a pas pu être remis ({e})",
                            backup.display()
                        );
                        return Tidy::Stuck;
                    }
                }
            }
        }
        // (Un fichier ou un lien de ce nom n'est rien de tout cela.)
        for left in [REFUSED, BACKUP, STAGING] {
            let path = self.dir.join(left);
            if there(&path) {
                match clear(&path) {
                    Ok(()) => log::info!("mise à jour : {} enlevé", path.display()),
                    Err(e) => log::info!("mise à jour : {} reste ({e})", path.display()),
                }
            }
        }
        Tidy::Clean
    }

    /// Prépare la version `version` à partir de son fichier `package`, tel
    /// que la liste signée l'annonce (`file`) : copié à côté de la version
    /// qui tourne, revérifié là, décompressé, reconnu, et lancé une fois pour
    /// dire son numéro. Rien n'est encore changé à ce Mouflon-ci. (À faire
    /// après [`Home::tidy`].)
    pub fn prepare(
        &self,
        package: &Path,
        file: &File,
        version: Version,
        lock: Lock,
    ) -> Result<Ready, InstallError> {
        let staging = self.dir.join(STAGING);
        clear(&staging).map_err(disk("la place de la préparation"))?;
        std::fs::create_dir(&staging).map_err(disk("la place de la préparation"))?;
        let moves = self
            .stage(package, file, version, &staging)
            .inspect_err(|_| {
                let _ = std::fs::remove_dir_all(&staging);
            })?;
        Ok(Ready {
            version,
            dir: self.dir.clone(),
            program: self.name.clone(),
            layout: self.layout,
            moves,
            _lock: lock,
        })
    }

    fn stage(
        &self,
        package: &Path,
        file: &File,
        version: Version,
        staging: &Path,
    ) -> Result<Vec<Move>, InstallError> {
        // Le fichier est vérifié là où il sera ouvert, dans le dossier de
        // Mouflon : ce qui est décompressé est bien ce que l'auteur a signé.
        let copy = staging.join(PACKAGE);
        std::fs::copy(package, &copy).map_err(disk("la copie du fichier téléchargé"))?;
        if !file
            .matches(&copy)
            .map_err(disk("la relecture du fichier téléchargé"))?
        {
            return Err(InstallError::Changed);
        }
        unpack(&copy, staging)?;
        let _ = std::fs::remove_file(&copy);
        let (root, program) = self.layout.unpacked(staging);
        if !program.is_file() {
            return Err(InstallError::Layout);
        }
        if self.layout == Layout::MacApp {
            mac::verify(&root, &self.dir.join(&self.name), version)?;
        }
        hello(&program, version)?;
        // Sur le disque pour de bon, avant de prendre la place de quoi que
        // ce soit.
        settle_on_disk(&root);
        let arriving = |from: PathBuf, to: String| Move {
            from: Some(from),
            to,
        };
        let Layout::Folder { program: packed } = self.layout else {
            return Ok(vec![arriving(root, self.name.clone())]);
        };
        if !self.whole {
            return Ok(vec![arriving(program, self.name.clone())]);
        }
        // Les animaux sont-ils sur le même disque que le programme ? (Un
        // dossier `pets` qui est un lien vers un autre disque ne s'échange
        // pas par renommages : mieux vaut le savoir avant de commencer.)
        let probe = (
            staging.join("sonde"),
            self.dir.join(PETS).join(".mouflon-sonde"),
        );
        std::fs::write(&probe.0, b"").map_err(disk("la place de la préparation"))?;
        let reached = std::fs::rename(&probe.0, &probe.1);
        let _ = std::fs::remove_file(&probe.0);
        let _ = std::fs::remove_file(&probe.1);
        reached.map_err(disk("le dossier des animaux n'est pas avec le programme"))?;
        let mut moves = Vec::new();
        for (name, path) in entries(&root)? {
            if name == REMOVALS {
                continue;
            }
            if name == PETS && path.is_dir() {
                // Chaque animal remplace le sien : ce que l'utilisateur a
                // posé à côté d'eux n'est pas touché.
                for (pet, path) in entries(&path)? {
                    moves.push(arriving(path, format!("{PETS}/{pet}")));
                }
            } else if name == packed {
                moves.push(arriving(path, self.name.clone()));
            } else {
                moves.push(arriving(path, name));
            }
        }
        // Ce que cette version ne livre plus s'en va (dans l'ancienne
        // version, d'où cela reviendrait avec elle).
        for gone in removals(&root.join(REMOVALS)) {
            // Jamais le programme, ni ce que cette version livre (quelle que
            // soit la casse : Windows ne la distingue pas).
            let kept =
                same_name(&gone, &self.name) || moves.iter().any(|m| same_name(&m.to, &gone));
            if !kept && there(&self.dir.join(&gone)) {
                moves.push(Move {
                    from: None,
                    to: gone,
                });
            }
        }
        Ok(moves)
    }
}

/// Ce qu'un dossier contient : noms et chemins, par ordre. Un nom qui ne
/// s'écrit pas simplement n'est pas d'une version de Mouflon.
fn entries(dir: &Path) -> Result<Vec<(String, PathBuf)>, InstallError> {
    let mut found = Vec::new();
    for entry in std::fs::read_dir(dir).map_err(disk("la nouvelle version"))? {
        let entry = entry.map_err(disk("la nouvelle version"))?;
        let name = entry.file_name();
        let name = name
            .to_str()
            .filter(|n| plain(n))
            .ok_or(InstallError::Layout)?;
        found.push((name.to_owned(), entry.path()));
    }
    found.sort();
    Ok(found)
}

/// Un nom de fichier simple, sur les trois systèmes : ni chemin, ni lecteur
/// (`C:nom` sortirait du dossier sous Windows), ni caractère de contrôle ou
/// réservé, ni point ou espace au bout, ni nom d'appareil de Windows (`NUL`,
/// `CON`, `COM1`…, avec ou sans extension), ni l'un des noms de travail de
/// la mise à jour.
fn plain(name: &str) -> bool {
    let stem = name.split('.').next().unwrap_or(name).to_ascii_uppercase();
    let device = matches!(stem.as_str(), "CON" | "PRN" | "AUX" | "NUL")
        || (stem.len() == 4
            && (stem.starts_with("COM") || stem.starts_with("LPT"))
            && stem.ends_with(|c: char| c.is_ascii_digit() && c != '0'));
    !name.is_empty()
        && name.len() <= 200
        && name != "."
        && name != ".."
        && !device
        && !name.starts_with(".mouflon-")
        && !name.starts_with(' ')
        && !name.ends_with(['.', ' '])
        && !name
            .chars()
            .any(|c| c.is_control() || "/\\:*?\"<>|".contains(c))
}

/// Deux noms désignent-ils la même chose, là où le système ne distingue pas
/// les majuscules ? (Dans le doute, oui : on en touche moins.)
fn same_name(a: &str, b: &str) -> bool {
    a.to_lowercase() == b.to_lowercase()
}

/// Une place dans le dossier de Mouflon : un nom simple (pas celui du
/// dossier des animaux, qui ne s'échange pas en bloc), ou `pets/<nom>`.
fn place(text: &str) -> bool {
    match text.split_once('/') {
        Some((PETS, pet)) => plain(pet),
        Some(_) => false,
        None => plain(text) && !same_name(text, PETS),
    }
}

/// Ce qu'une version ne livre plus (son fichier `a-retirer.txt`, une place
/// par ligne : `vieux.dll`, `pets/pelote`). Ce qui n'est pas une place du
/// dossier de Mouflon est ignoré ; `pets` lui-même ne se retire pas.
fn removals(list: &Path) -> Vec<String> {
    std::fs::read_to_string(list)
        .unwrap_or_default()
        .lines()
        .map(str::trim)
        .filter(|line| !line.is_empty() && !line.starts_with('#'))
        .filter(|line| place(line))
        .map(str::to_owned)
        .collect()
}

/// Une app de Mac venue de l'App Store porte son reçu, ou son profil.
fn store_copy(app: &Path) -> bool {
    let contents = app.join("Contents");
    contents.join("_MASReceipt").exists() || contents.join("embedded.provisionprofile").exists()
}

/// Ce Mouflon vient-il de l'App Store ? (C'est alors lui qui le met à jour.)
pub fn from_a_store() -> bool {
    static STORE: std::sync::OnceLock<bool> = std::sync::OnceLock::new();
    *STORE.get_or_init(|| {
        cfg!(target_os = "macos")
            && std::env::current_exe()
                .ok()
                .and_then(|exe| bundle_of(&exe.canonicalize().unwrap_or(exe)))
                .is_some_and(|app| store_copy(&app))
    })
}

/// L'app qui contient ce programme, s'il est dans une app de Mac.
fn bundle_of(exe: &Path) -> Option<PathBuf> {
    let macos = exe.parent()?;
    let contents = macos.parent()?;
    let app = contents.parent()?;
    (macos.file_name()? == "MacOS"
        && contents.file_name()? == "Contents"
        && app.extension()? == "app")
        .then(|| app.to_path_buf())
}

/// Le programme sort-il d'une compilation (le dossier `target` de cargo, que
/// son fichier `CACHEDIR.TAG` signale) ? Celui-là ne se remplace pas.
fn in_cargo_target(exe: &Path) -> bool {
    exe.ancestors()
        .skip(1)
        .take(4)
        .any(|dir| dir.join("CACHEDIR.TAG").is_file())
}

/// Vide une place (un dossier, un fichier, un lien) ; rien là : rien à faire.
fn clear(path: &Path) -> std::io::Result<()> {
    match path.symlink_metadata() {
        Ok(meta) if meta.is_dir() => std::fs::remove_dir_all(path),
        Ok(_) => std::fs::remove_file(path),
        Err(_) => Ok(()),
    }
}

/// Y a-t-il quelque chose là (même un lien qui ne mène nulle part) ?
fn there(path: &Path) -> bool {
    path.symlink_metadata().is_ok()
}

/// Renomme. Sous Windows, un antivirus ou un indexeur tient parfois un
/// fichier un instant : on insiste deux secondes avant de renoncer.
fn rename(from: &Path, to: &Path) -> std::io::Result<()> {
    let tries = if cfg!(windows) { 10 } else { 1 };
    let mut tried = 0;
    loop {
        match std::fs::rename(from, to) {
            Ok(()) => return Ok(()),
            Err(e) => {
                tried += 1;
                if tried >= tries || e.kind() == std::io::ErrorKind::NotFound {
                    return Err(e);
                }
                std::thread::sleep(Duration::from_millis(200));
            }
        }
    }
}

/// Renomme vers une place dont le dossier n'existe peut-être pas encore.
fn rename_into(from: &Path, to: &Path) -> std::io::Result<()> {
    if let Some(parent) = to.parent() {
        std::fs::create_dir_all(parent)?;
    }
    rename(from, to)
}

/// Demande au système d'écrire pour de bon tout ce que contient `root` (un
/// fichier ou un dossier) : au mieux, sans s'arrêter sur un refus.
fn settle_on_disk(root: &Path) {
    let Ok(meta) = root.symlink_metadata() else {
        return;
    };
    if meta.is_dir() {
        if let Ok(entries) = std::fs::read_dir(root) {
            for entry in entries.flatten() {
                settle_on_disk(&entry.path());
            }
        }
    } else if meta.is_file() {
        flush(root);
    }
}

/// Demande au système d'écrire pour de bon ce fichier. (Ouvert en écriture
/// si possible : Windows ne le fait qu'à cette condition.)
fn flush(path: &Path) {
    let file = std::fs::File::options()
        .write(true)
        .open(path)
        .or_else(|_| std::fs::File::open(path));
    if let Ok(file) = file {
        let _ = file.sync_all();
    }
}

/// Décompresse le fichier d'une version dans `into`, par l'outil du système.
fn unpack(package: &Path, into: &Path) -> Result<(), InstallError> {
    let mut command;
    if cfg!(target_os = "macos") {
        // `ditto` garde ce qui fait une app signée (liens, attributs).
        command = Command::new("/usr/bin/ditto");
        command.args(["-x", "-k"]).arg(package).arg(into);
    } else if cfg!(target_os = "windows") {
        // Le `tar` de Windows (10 et 11) ouvre aussi les zip.
        command = Command::new(tool::system32("tar.exe"));
        command.arg("-xf").arg(package).arg("-C").arg(into);
    } else {
        // Les fichiers sont à qui lance Mouflon, pas à qui a fait l'archive.
        command = Command::new("tar");
        command
            .args(["--no-same-owner", "-xzf"])
            .arg(package)
            .arg("-C")
            .arg(into);
    }
    let said = tool::run(command, TOOL_TIME)?;
    if said.status.success() {
        Ok(())
    } else {
        Err(InstallError::Unpack(said.complaint()))
    }
}

/// Lance la nouvelle version, une fois, pour qu'elle dise son numéro : elle
/// tourne donc sur cette machine, et c'est bien la version annoncée.
fn hello(program: &Path, version: Version) -> Result<(), InstallError> {
    let mut command = Command::new(program);
    command.arg(VERSION_ARG);
    let said = tool::run(command, HELLO_TIME).map_err(|e| InstallError::Start(e.to_string()))?;
    if !said.status.success() {
        return Err(InstallError::Start(said.complaint()));
    }
    let wanted = banner(version);
    if said.stdout.trim() == wanted {
        Ok(())
    } else {
        Err(InstallError::NotIt(format!(
            "le fichier contient « {} », pas « {wanted} »",
            said.stdout.trim()
        )))
    }
}

/// Une seule mise à jour à la fois sur ce compte : qui tient le verrou
/// prépare, échange et veille ; les autres Mouflon laissent faire. Il tombe
/// avec celui qui le tient, même arrêté net.
#[derive(Debug)]
pub struct Lock(#[allow(dead_code)] std::fs::File);

impl Lock {
    /// Prend le verrou, s'il est libre.
    pub fn take(dirs: &Dirs) -> Option<Lock> {
        std::fs::create_dir_all(&dirs.state).ok()?;
        let file = std::fs::OpenOptions::new()
            .create(true)
            .truncate(false)
            .write(true)
            .open(dirs.lock_file())
            .ok()?;
        file.try_lock().ok()?;
        Some(Lock(file))
    }
}

/// La note d'un échange, écrite dans `.mouflon-ancien` avant de rien
/// déplacer : la version qui arrive, et chaque place touchée. Avec elle, un
/// échange interrompu se défait.
///
/// La note ne se récrit pas. Que l'échange soit **fini** se dit à côté
/// d'elle, par un fichier vide préparé d'avance et renommé à ce moment-là
/// ([`DONE`]) : un geste qui ne demande pas de place sur le disque. Tant
/// que ce fichier n'y est pas (ou dès qu'il est retiré, quand la nouvelle
/// version n'a pas démarré), l'ancienne version doit reprendre sa place.
#[derive(Debug, Clone, PartialEq, Eq)]
struct Record {
    version: Version,
    moves: Vec<Kept>,
}

/// Une place touchée par l'échange.
#[derive(Debug, Clone, PartialEq, Eq)]
struct Kept {
    /// Il y avait quelque chose à cette place.
    had: bool,
    /// Quelque chose de la nouvelle version y arrive.
    arrives: bool,
    to: String,
}

impl Record {
    /// Écrit la note, et prépare ce qui dira la fin de l'échange.
    fn write(&self, backup: &Path) -> std::io::Result<()> {
        let mut text = format!("version {}\n", self.version);
        for kept in &self.moves {
            text.push_str(&format!(
                "place {} {} {}\n",
                u8::from(kept.had),
                u8::from(kept.arrives),
                kept.to
            ));
        }
        // Complète ou pas du tout, et sur le disque avant le premier geste.
        text.push_str("fin\n");
        let path = backup.join(RECORD);
        write_whole(&path, text.as_bytes())?;
        flush(&path);
        std::fs::write(backup.join(DONE_READY), b"")?;
        if cfg!(unix)
            && let Ok(dir) = std::fs::File::open(backup)
        {
            let _ = dir.sync_all();
        }
        Ok(())
    }

    fn read(backup: &Path) -> Option<Record> {
        let text = std::fs::read_to_string(backup.join(RECORD)).ok()?;
        let mut lines = text.lines();
        let version = Version::parse(lines.next()?.strip_prefix("version ")?)?;
        let mut moves = Vec::new();
        let mut complete = false;
        for line in lines {
            if line == "fin" {
                complete = true;
                break;
            }
            let mut words = line.strip_prefix("place ")?.splitn(3, ' ');
            let flag = |word: Option<&str>| match word {
                Some("1") => Some(true),
                Some("0") => Some(false),
                _ => None,
            };
            let (had, arrives) = (flag(words.next())?, flag(words.next())?);
            let to = words.next().filter(|to| place(to))?;
            moves.push(Kept {
                had,
                arrives,
                to: to.to_owned(),
            });
        }
        complete.then_some(Record { version, moves })
    }

    /// L'échange est fini : dit d'un renommage, qui passe ou ne passe pas.
    fn finish(backup: &Path) -> std::io::Result<()> {
        rename(&backup.join(DONE_READY), &backup.join(DONE))
    }

    /// L'échange est à défaire : ce qui disait sa fin est retiré (renommé,
    /// ou à défaut effacé).
    fn reopen(backup: &Path) {
        let done = backup.join(DONE);
        if there(&done) && rename(&done, &backup.join(DONE_READY)).is_err() {
            let _ = std::fs::remove_file(&done);
        }
    }

    /// L'échange est-il fini (et pas en train d'être défait) ?
    fn finished(backup: &Path) -> bool {
        backup.join(DONE).is_file()
    }

    /// L'échange est-il fini depuis plus de `age` ? (Une horloge remise en
    /// arrière ne fait pas attendre plus longtemps.)
    fn finished_for(backup: &Path, age: Duration) -> bool {
        std::fs::metadata(backup.join(DONE))
            .and_then(|meta| meta.modified())
            .ok()
            .is_none_or(|written| {
                SystemTime::now()
                    .duration_since(written)
                    .map_or(true, |spent| spent >= age)
            })
    }
}

/// Remet l'ancienne version à sa place : pour chaque place touchée, ce qui
/// est arrivé de la nouvelle s'en va (dans `.mouflon-refuse`), ce qui
/// attendait dans `.mouflon-ancien` revient. Se refait sans dommage : ce qui
/// est déjà revenu n'est plus touché. Rend la première erreur, mais va au
/// bout.
fn restore(dir: &Path, moves: &[Kept]) -> std::io::Result<()> {
    let (backup, refused) = (dir.join(BACKUP), dir.join(REFUSED));
    // Un dossier à nous, et vide : pas un lien qui mènerait ailleurs.
    let mut first = clear(&refused).and_then(|()| std::fs::create_dir(&refused));
    for kept in moves.iter().rev() {
        let (here, waiting) = (dir.join(&kept.to), backup.join(&kept.to));
        let out = refused.join(&kept.to);
        // Ce qui attend n'est jamais enlevé avant d'avoir repris sa place ;
        // sans rien qui attende, ce qui est là n'est celui de la nouvelle
        // version que s'il n'y avait rien avant.
        let leaves = there(&here) && kept.arrives && (there(&waiting) || !kept.had);
        let done = if leaves {
            rename_into(&here, &out)
        } else {
            Ok(())
        }
        .and_then(|()| {
            if !there(&waiting) {
                return Ok(());
            }
            rename_into(&waiting, &here).inspect_err(|_| {
                // L'ancien ne revient pas : plutôt le nouveau que rien.
                if leaves {
                    let _ = rename(&out, &here);
                }
            })
        });
        first = first.and(done);
    }
    first
}

/// Sans note lisible : ce qui attend dans `.mouflon-ancien` et manque dans
/// le dossier y retourne (un nom, ou un animal de `pets`). Le reste peut
/// s'en aller.
fn rescue(dir: &Path, backup: &Path) -> std::io::Result<()> {
    let mut first = Ok(());
    let mut back = |waiting: &Path, here: &Path| {
        if !there(here) {
            log::warn!("mise à jour : {} retrouve sa place", here.display());
            first = std::mem::replace(&mut first, Ok(())).and(rename_into(waiting, here));
        }
    };
    for entry in std::fs::read_dir(backup)?.flatten() {
        let name = entry.file_name();
        // (Ni la note, ni rien de ce qui l'accompagne.)
        let Some(name) = name.to_str().filter(|name| plain(name)) else {
            continue;
        };
        if name == PETS && entry.path().is_dir() && dir.join(PETS).is_dir() {
            for pet in std::fs::read_dir(entry.path())?.flatten() {
                back(&pet.path(), &dir.join(PETS).join(pet.file_name()));
            }
        } else {
            back(&entry.path(), &dir.join(name));
        }
    }
    first
}

/// Une version prête à prendre la place de celle qui tourne.
#[derive(Debug)]
pub struct Ready {
    pub version: Version,
    dir: PathBuf,
    /// Le nom de l'app ou du programme, dans `dir`.
    program: String,
    layout: Layout,
    moves: Vec<Move>,
    _lock: Lock,
}

impl Ready {
    pub fn layout(&self) -> Layout {
        self.layout
    }

    /// L'app (Mac) ou le programme de ce Mouflon, à sa place de toujours.
    pub fn program(&self) -> PathBuf {
        self.dir.join(&self.program)
    }

    /// Renonce : ce qui était préparé est enlevé (le fichier téléchargé,
    /// lui, reste pour une autre fois).
    pub fn discard(self) {
        let _ = std::fs::remove_dir_all(self.dir.join(STAGING));
    }

    /// L'échange : chaque chose de l'ancienne version va attendre dans
    /// `.mouflon-ancien`, celle de la nouvelle prend sa place. La note de
    /// l'échange est écrite d'abord. Au premier refus, tout est remis comme
    /// avant, et ce qui était préparé s'en va.
    pub fn swap(self) -> Result<Swapped, InstallError> {
        let backup = self.dir.join(BACKUP);
        let record = Record {
            version: self.version,
            moves: self
                .moves
                .iter()
                .map(|m| Kept {
                    had: there(&self.dir.join(&m.to)),
                    arrives: m.from.is_some(),
                    to: m.to.clone(),
                })
                .collect(),
        };
        let begun = clear(&backup)
            .and_then(|()| std::fs::create_dir(&backup))
            .and_then(|()| record.write(&backup));
        if let Err(e) = begun {
            let _ = clear(&backup);
            self.discard();
            return Err(InstallError::Disk("la note de l'échange", e));
        }
        let moved = self
            .moves
            .iter()
            .zip(&record.moves)
            .try_for_each(|(m, kept)| exchange(&self.dir, m, kept))
            // Fini : c'est ce geste-là qui le dit. S'il ne passe pas,
            // l'échange n'a pas eu lieu.
            .and_then(|()| Record::finish(&backup));
        if let Err(e) = moved {
            // Ce qui a déjà changé de place y retourne ; si tout n'est pas
            // revenu, l'ancienne version reste à côté, avec sa note.
            match restore(&self.dir, &record.moves) {
                Ok(()) => {
                    let _ = clear(&self.dir.join(REFUSED));
                    let _ = clear(&backup);
                }
                Err(again) => log::error!(
                    "mise à jour : l'ancienne version n'a pas repris toute sa place ({again})"
                ),
            }
            self.discard();
            return Err(InstallError::Disk("l'échange des deux versions", e));
        }
        Ok(Swapped {
            dir: self.dir,
            record,
            _lock: self._lock,
        })
    }
}

/// Échange une chose : l'ancienne va attendre, la nouvelle prend sa place.
fn exchange(dir: &Path, m: &Move, kept: &Kept) -> std::io::Result<()> {
    let (here, waiting) = (dir.join(&m.to), dir.join(BACKUP).join(&m.to));
    // Un fichier (le programme, sous Linux) est remplacé d'un seul geste : il
    // garde un second nom du côté de l'ancienne version, puis le nouveau
    // prend sa place sans qu'elle soit jamais vide. (Windows ne laisse pas
    // remplacer ainsi un programme qui tourne ; un dossier non plus.)
    if cfg!(unix)
        && kept.had
        && let Some(from) = &m.from
        && here.symlink_metadata().is_ok_and(|meta| meta.is_file())
    {
        if let Some(parent) = waiting.parent() {
            std::fs::create_dir_all(parent)?;
        }
        if std::fs::hard_link(&here, &waiting).is_ok() {
            return rename(from, &here);
        }
    }
    if kept.had {
        rename_into(&here, &waiting)?;
    }
    match &m.from {
        Some(from) => rename_into(from, &here),
        None => Ok(()),
    }
}

/// La nouvelle version est en place ; l'ancienne attend à côté.
#[derive(Debug)]
pub struct Swapped {
    dir: PathBuf,
    record: Record,
    _lock: Lock,
}

impl Swapped {
    /// La nouvelle version a démarré. L'ancienne reste encore à côté un
    /// moment : c'est la nouvelle qui l'enlèvera (voir [`Home::tidy`]). Ce
    /// qui avait servi à préparer s'en va.
    pub fn settle(self) {
        let _ = std::fs::remove_dir_all(self.dir.join(STAGING));
    }

    /// La nouvelle version n'a pas démarré : elle s'en va, l'ancienne
    /// reprend sa place. Si tout n'a pas pu revenir, l'ancienne version
    /// reste à côté avec sa note : un prochain lancement reprendra.
    pub fn undo(self) -> std::io::Result<()> {
        let backup = self.dir.join(BACKUP);
        // L'échange n'est plus « fini » : quoi qu'il arrive ensuite, personne
        // ne jettera l'ancienne version.
        Record::reopen(&backup);
        let restored = restore(&self.dir, &self.record.moves);
        let _ = clear(&self.dir.join(REFUSED));
        let _ = clear(&self.dir.join(STAGING));
        if restored.is_ok() {
            let _ = clear(&backup);
        }
        restored
    }
}

/// Ce que macOS demande en plus : la nouvelle app doit être signée par le
/// même développeur que celle qui tourne, et notarisée par Apple. C'est
/// aussi ce qui fait que macOS lui garde les autorisations déjà données.
#[cfg(target_os = "macos")]
mod mac {
    use super::{Command, InstallError, Path, TOOL_TIME, Version, tool};

    fn said(command: Command) -> Result<tool::Said, InstallError> {
        Ok(tool::run(command, TOOL_TIME)?)
    }

    /// Ce que macOS exige de la signature d'une app pour la tenir pour la
    /// même (son « designated requirement ») : pour une version publiée,
    /// l'identifiant de Mouflon et le certificat Developer ID de son auteur.
    /// `None` : l'app n'en dit pas.
    fn requirement(app: &Path) -> Result<Option<String>, InstallError> {
        let mut command = Command::new("/usr/bin/codesign");
        command.args(["-d", "-r-"]).arg(app);
        let said = said(command)?;
        Ok(format!("{}\n{}", said.stdout, said.stderr)
            .lines()
            .find_map(|line| line.strip_prefix("designated => "))
            .map(str::trim)
            .filter(|r| !r.is_empty())
            .map(str::to_owned))
    }

    /// Une app signée pour elle seule (une construction de travail : sa
    /// signature ne nomme que son propre code) ne se remplace pas.
    pub fn replaces_itself(app: &Path) -> bool {
        cfg!(feature = "essai-maj")
            || matches!(requirement(app), Ok(Some(r)) if !r.contains("cdhash"))
    }

    /// Une clé du fichier `Info.plist` d'une app ; `None` : elle ne la dit
    /// pas.
    fn info(app: &Path, key: &str) -> Result<Option<String>, InstallError> {
        let mut command = Command::new("/usr/bin/plutil");
        command
            .args(["-extract", key, "raw", "-o", "-"])
            .arg(app.join("Contents").join("Info.plist"));
        let said = said(command)?;
        Ok(said.status.success().then(|| said.stdout.trim().to_owned()))
    }

    pub fn verify(new: &Path, current: &Path, version: Version) -> Result<(), InstallError> {
        let not_it = |text: String| Err(InstallError::NotIt(text));
        // Ce qu'on ne sait pas dire de ce Mouflon-ci (déplacé ou renommé
        // pendant qu'il tourne) n'est pas retenu contre la version.
        let own = |text: &str| InstallError::Own(text.to_owned());
        if !current.join("Contents").join("Info.plist").is_file() {
            return Err(own("ce Mouflon n'est plus là où il a été lancé"));
        }
        match info(new, "CFBundleShortVersionString")? {
            Some(v) if v == version.to_string() => {}
            Some(v) => return not_it(format!("l'app reçue est la version {v}, pas la {version}")),
            None => return not_it("l'app reçue ne dit pas sa version".into()),
        }
        let ours = info(current, "CFBundleIdentifier")?
            .ok_or_else(|| own("ce Mouflon ne dit pas son identifiant"))?;
        if info(new, "CFBundleIdentifier")?.as_deref() != Some(ours.as_str()) {
            return not_it("l'app reçue n'est pas Mouflon".into());
        }
        let mut command = Command::new("/usr/bin/codesign");
        command.args(["--verify", "--deep", "--strict"]).arg(new);
        let checked = said(command)?;
        if !checked.status.success() {
            return not_it(format!(
                "la signature de l'app reçue ne tient pas ({})",
                checked.complaint()
            ));
        }
        if cfg!(feature = "essai-maj") {
            return Ok(());
        }
        let requirement = requirement(current)?
            .filter(|r| !r.contains("cdhash"))
            .ok_or_else(|| own("ce Mouflon n'a pas de signature à laquelle comparer"))?;
        let mut command = Command::new("/usr/bin/codesign");
        command
            .args(["--verify", "--deep", "--strict", "-R"])
            .arg(format!("={requirement}"))
            .arg(new);
        let same = said(command)?;
        if !same.status.success() {
            return not_it(format!(
                "l'app reçue n'est pas signée par le même développeur ({})",
                same.complaint()
            ));
        }
        let mut command = Command::new("/usr/sbin/spctl");
        command.args(["--assess", "--type", "execute"]).arg(new);
        let notarized = said(command)?;
        if !notarized.status.success() {
            return not_it(format!(
                "l'app reçue n'est pas notarisée par Apple ({})",
                notarized.complaint()
            ));
        }
        Ok(())
    }
}

/// Ailleurs que sur Mac, rien de plus : la signature de l'auteur fait foi.
#[cfg(not(target_os = "macos"))]
mod mac {
    use super::{InstallError, Path, Version};

    pub fn replaces_itself(_app: &Path) -> bool {
        false
    }

    pub fn verify(_new: &Path, _current: &Path, _version: Version) -> Result<(), InstallError> {
        Err(InstallError::NotIt(
            "une app de Mac, ailleurs que sur Mac".into(),
        ))
    }
}

#[cfg(all(test, unix))]
pub mod tests {
    use std::os::unix::fs::PermissionsExt;

    use super::*;
    use crate::update::state::tests::{dirs, scratch};

    /// Ce que fait un faux Mouflon quand un relais le lance.
    #[derive(Clone, Copy)]
    pub enum Arrives<'a> {
        /// Il donne signe de vie dans ce fichier, puis tourne un moment.
        Well(&'a Path),
        /// Il s'arrête aussitôt.
        Badly,
        /// Il tourne sans rien dire.
        Mute,
    }

    /// Un faux Mouflon : un script qui dit sa version, et note dans
    /// `retour` (à côté de lui) qu'on l'a relancé tel quel.
    fn script(says: &str, arrives: Arrives<'_>) -> String {
        let arrival = match arrives {
            Arrives::Well(alive) => format!("echo vivant > '{}'; sleep 2", alive.display()),
            Arrives::Badly => "exit 3".to_owned(),
            Arrives::Mute => "sleep 5".to_owned(),
        };
        format!(
            "#!/bin/sh\ncase \"$1\" in\n--version) echo '{says}' ;;\n--relais=arrivee) {arrival} ;;\n--relais=retour) echo revenu > \"$(dirname \"$0\")/retour\" ;;\nesac\n"
        )
    }

    fn write(path: &Path, content: &str, executable: bool) {
        std::fs::create_dir_all(path.parent().unwrap()).unwrap();
        std::fs::write(path, content).unwrap();
        if executable {
            std::fs::set_permissions(path, std::fs::Permissions::from_mode(0o755)).unwrap();
        }
    }

    /// Le dossier d'un faux Mouflon, tel qu'une version le livre.
    fn folder(at: &Path, says: &str, arrives: Arrives<'_>, pets: &[&str]) {
        write(&at.join("mouflon"), &script(says, arrives), true);
        write(&at.join("LISEZMOI.txt"), &format!("lisez {says}"), false);
        for pet in pets {
            write(
                &at.join("pets").join(pet).join("pet.toml"),
                &format!("{pet} de {says}"),
                false,
            );
        }
    }

    /// Le fichier d'une fausse version (`Mouflon-linux-x86_64.tar.gz`).
    pub fn package(root: &Path, says: &str, arrives: Arrives<'_>) -> PathBuf {
        let made = root.join("fabrique");
        let _ = std::fs::remove_dir_all(&made);
        folder(&made.join("Mouflon"), says, arrives, &["texel", "racka"]);
        archive(root, &made)
    }

    fn archive(root: &Path, made: &Path) -> PathBuf {
        let package = root.join("Mouflon-linux-x86_64.tar.gz");
        let packed = Command::new("tar")
            .arg("-czf")
            .arg(&package)
            .arg("-C")
            .arg(made)
            .arg("Mouflon")
            .status()
            .unwrap();
        assert!(packed.success());
        package
    }

    /// Ce que la liste signée dirait de ce fichier.
    fn announced(package: &Path) -> File {
        let bytes = std::fs::read(package).unwrap();
        File {
            name: "Mouflon-linux-x86_64.tar.gz".into(),
            size: bytes.len() as u64,
            sha512: ed25519_compact::sha512::Hash::hash(&bytes),
        }
    }

    /// Un faux Mouflon 1.0.0 installé : son dossier, avec ses animaux, et ce
    /// que l'utilisateur y a posé (un animal en dossier, un autre en
    /// fichier, un lien, des notes).
    pub fn installed(root: &Path) -> Home {
        let dir = root.join("chez moi").join("Mouflon");
        folder(&dir, "Mouflon 1.0.0", Arrives::Badly, &["texel", "le-mien"]);
        write(&dir.join("notes.txt"), "à moi", false);
        write(&dir.join("pets/mon-mouton.xml"), "<animations/>", false);
        write(&root.join("ailleurs/pet.toml"), "ailleurs", false);
        std::os::unix::fs::symlink(root.join("ailleurs"), dir.join("pets/lien")).unwrap();
        home(&dir, "mouflon")
    }

    pub fn home(dir: &Path, name: &str) -> Home {
        match Home::of(
            &dir.join(name),
            Layout::Folder { program: "mouflon" },
            false,
        ) {
            Standing::Own(home) => home,
            other => panic!("{other:?}"),
        }
    }

    pub fn lock(root: &Path) -> Lock {
        Lock::take(&dirs(root)).unwrap()
    }

    /// Prépare la fausse version 9.9.9 posée dans `package`.
    pub fn prepared(home: &Home, root: &Path, package: &Path) -> Result<Ready, InstallError> {
        home.prepare(package, &announced(package), NEW, lock(root))
    }

    /// Tout ce qu'un dossier contient : chemins et contenus, par ordre.
    pub fn snapshot(dir: &Path) -> Vec<(String, String)> {
        fn walk(base: &Path, dir: &Path, out: &mut Vec<(String, String)>) {
            for entry in std::fs::read_dir(dir).unwrap() {
                let path = entry.unwrap().path();
                let name = path.strip_prefix(base).unwrap().display().to_string();
                let meta = path.symlink_metadata().unwrap();
                if meta.is_symlink() {
                    let target = std::fs::read_link(&path).unwrap();
                    out.push((name, format!("(lien vers {})", target.display())));
                } else if meta.is_dir() {
                    out.push((name, "(dossier)".into()));
                    walk(base, &path, out);
                } else {
                    out.push((name, std::fs::read_to_string(&path).unwrap()));
                }
            }
        }
        let mut out = Vec::new();
        walk(dir, dir, &mut out);
        out.sort();
        out
    }

    /// Ce qui se voit dans le dossier (sans les dossiers de la mise à jour).
    fn visible(dir: &Path) -> Vec<(String, String)> {
        snapshot(dir)
            .into_iter()
            .filter(|(path, _)| !path.starts_with(".mouflon-"))
            .collect()
    }

    /// Les dossiers de la mise à jour encore là.
    pub fn hidden(dir: &Path) -> Vec<String> {
        let mut names: Vec<String> = std::fs::read_dir(dir)
            .unwrap()
            .map(|e| e.unwrap().file_name().to_string_lossy().into_owned())
            .filter(|name| name.starts_with('.'))
            .collect();
        names.sort();
        names
    }

    fn says(program: &Path) -> String {
        let out = Command::new(program).arg(VERSION_ARG).output().unwrap();
        String::from_utf8_lossy(&out.stdout).trim().to_owned()
    }

    /// Vieillit un échange fini : il l'est depuis `by`.
    pub fn age_record(dir: &Path, by: Duration) {
        let file = std::fs::File::options()
            .write(true)
            .open(dir.join(BACKUP).join(DONE))
            .unwrap();
        file.set_modified(SystemTime::now() - by).unwrap();
    }

    const OLD: Version = Version(1, 0, 0);
    pub const NEW: Version = Version(9, 9, 9);

    #[test]
    fn a_version_is_prepared_beside_the_one_that_runs_then_takes_its_place() {
        let root = scratch("echange");
        let home = installed(&root);
        let before = snapshot(&home.dir);
        let package = package(&root, "Mouflon 9.9.9", Arrives::Badly);
        let ready = prepared(&home, &root, &package).unwrap();
        // Rien n'a changé à la version qui tourne : tout est à côté.
        assert_eq!(visible(&home.dir), before);
        assert_eq!(hidden(&home.dir), [STAGING]);
        let places: Vec<_> = ready.moves.iter().map(|m| m.to.as_str()).collect();
        assert_eq!(
            places,
            ["LISEZMOI.txt", "mouflon", "pets/racka", "pets/texel"]
        );
        assert_eq!(ready.program(), home.dir.join("mouflon"));

        let swapped = ready.swap().unwrap();
        assert_eq!(says(&home.dir.join("mouflon")), "Mouflon 9.9.9");
        let read = |path: &str| std::fs::read_to_string(home.dir.join(path)).unwrap();
        assert_eq!(read("LISEZMOI.txt"), "lisez Mouflon 9.9.9");
        assert_eq!(read("pets/texel/pet.toml"), "texel de Mouflon 9.9.9");
        assert_eq!(read("pets/racka/pet.toml"), "racka de Mouflon 9.9.9");
        // Ce que l'utilisateur avait posé n'a pas bougé : son animal en
        // dossier, celui en fichier, son lien, ses notes.
        assert_eq!(read("pets/le-mien/pet.toml"), "le-mien de Mouflon 1.0.0");
        assert_eq!(read("pets/mon-mouton.xml"), "<animations/>");
        assert_eq!(read("pets/lien/pet.toml"), "ailleurs");
        assert!(home.dir.join("pets/lien").is_symlink());
        assert_eq!(read("notes.txt"), "à moi");
        // L'ancienne version attend à côté, avec la note de l'échange.
        let backup = home.dir.join(BACKUP);
        assert_eq!(says(&backup.join("mouflon")), "Mouflon 1.0.0");
        assert!(backup.join("pets/texel/pet.toml").is_file());
        let record = Record::read(&backup).unwrap();
        assert_eq!(record.version, NEW);
        assert!(Record::finished(&backup));
        // Le programme (un fichier) a été remplacé d'un seul geste : à aucun
        // moment sa place n'a été vide.
        assert_eq!(
            record.moves[1],
            Kept {
                had: true,
                arrives: true,
                to: "mouflon".into()
            }
        );
        assert_eq!(
            record.moves[2],
            Kept {
                had: false,
                arrives: true,
                to: "pets/racka".into()
            }
        );

        // Elle n'a pas démarré : tout revient comme avant, sans reste.
        swapped.undo().unwrap();
        assert_eq!(snapshot(&home.dir), before);
        let _ = std::fs::remove_dir_all(root);
    }

    #[test]
    fn once_the_new_version_runs_the_old_one_goes_a_moment_later() {
        let root = scratch("menage");
        let home = installed(&root);
        let package = package(&root, "Mouflon 9.9.9", Arrives::Badly);
        prepared(&home, &root, &package)
            .unwrap()
            .swap()
            .unwrap()
            .settle();
        // L'ancienne version reste encore : ce qui vient d'être écrit n'est
        // peut-être pas sur le disque.
        assert_eq!(hidden(&home.dir), [BACKUP]);
        let after = visible(&home.dir);
        assert_eq!(home.tidy(NEW), Tidy::Later);
        assert_eq!(hidden(&home.dir), [BACKUP]);
        // Un moment a passé. Un Mouflon de l'ancienne version encore en
        // marche n'y touche pas (et ne défait surtout pas l'échange).
        age_record(&home.dir, Duration::from_secs(600));
        assert_eq!(home.tidy(OLD), Tidy::Later);
        assert_eq!(says(&home.dir.join("mouflon")), "Mouflon 9.9.9");
        assert_eq!(
            (hidden(&home.dir), visible(&home.dir)),
            (vec![BACKUP.to_owned()], after.clone())
        );
        // La nouvelle, elle, l'enlève (une plus récente encore le ferait
        // aussi).
        assert_eq!(home.tidy(NEW), Tidy::Clean);
        assert!(hidden(&home.dir).is_empty());
        assert_eq!(visible(&home.dir), after);
        // Un fichier ou un lien mort qui porterait le nom d'un dossier de la
        // mise à jour n'arrête rien.
        write(&home.dir.join(BACKUP), "pas un dossier", false);
        std::os::unix::fs::symlink(root.join("nulle-part"), home.dir.join(REFUSED)).unwrap();
        assert_eq!(home.tidy(NEW), Tidy::Clean);
        assert!(hidden(&home.dir).is_empty());
        // Ce qu'une préparation interrompue aurait laissé s'enlève aussi.
        for name in [STAGING, REFUSED] {
            write(&home.dir.join(name).join("reste"), "x", false);
        }
        assert_eq!(home.tidy(NEW), Tidy::Clean);
        assert!(hidden(&home.dir).is_empty());
        assert_eq!(visible(&home.dir), after);
        let _ = std::fs::remove_dir_all(root);
    }

    #[test]
    fn a_program_taken_out_of_its_folder_is_replaced_alone_under_its_own_name() {
        let root = scratch("seul");
        let dir = root.join("Bureau");
        write(
            &dir.join("moutons"),
            &script("Mouflon 1.0.0", Arrives::Badly),
            true,
        );
        write(&dir.join("LISEZMOI.txt"), "le mien, d'autre chose", false);
        let home = home(&dir, "moutons");
        assert!(!home.whole);
        let before = snapshot(&dir);
        let package = package(&root, "Mouflon 9.9.9", Arrives::Badly);
        let ready = prepared(&home, &root, &package).unwrap();
        assert_eq!(ready.moves.len(), 1);
        let swapped = ready.swap().unwrap();
        assert_eq!(says(&dir.join("moutons")), "Mouflon 9.9.9");
        // Rien d'autre n'est posé sur le bureau, rien n'y est touché.
        assert_eq!(
            std::fs::read_to_string(dir.join("LISEZMOI.txt")).unwrap(),
            "le mien, d'autre chose"
        );
        assert!(!dir.join("pets").exists());
        swapped.undo().unwrap();
        assert_eq!(snapshot(&dir), before);
        let _ = std::fs::remove_dir_all(root);
    }

    #[test]
    fn a_file_that_is_not_the_announced_mouflon_changes_nothing() {
        let root = scratch("refus");
        let home = installed(&root);
        let before = snapshot(&home.dir);
        let refused = |package: &Path| {
            let error = prepared(&home, &root, package).unwrap_err();
            assert_eq!(snapshot(&home.dir), before, "{error}");
            error
        };
        // Une autre version que celle que la liste annonce.
        let other = refused(&package(&root, "Mouflon 9.9.8", Arrives::Badly));
        assert!(matches!(other, InstallError::NotIt(_)), "{other}");
        assert_eq!(other.blame(), Blame::Version);
        // Un programme d'essai ne prend pas la place du vrai.
        let trial = refused(&package(&root, "Mouflon 9.9.9 (essai)", Arrives::Badly));
        assert!(matches!(trial, InstallError::NotIt(_)), "{trial}");
        // Un programme qui ne tourne pas ici, cette fois du moins.
        let made = root.join("fabrique");
        let _ = std::fs::remove_dir_all(&made);
        write(&made.join("Mouflon/mouflon"), "#!/bin/sh\nexit 1\n", true);
        let broken = refused(&archive(&root, &made));
        assert!(matches!(broken, InstallError::Start(_)), "{broken}");
        assert_eq!(broken.blame(), Blame::Attempt);
        // Pas de programme du tout.
        std::fs::remove_file(made.join("Mouflon/mouflon")).unwrap();
        write(&made.join("Mouflon/LISEZMOI.txt"), "rien", false);
        let empty = refused(&archive(&root, &made));
        assert!(matches!(empty, InstallError::Layout), "{empty}");
        assert_eq!(empty.blame(), Blame::Version);
        // Pas une archive : un souci du moment, pas de la version.
        let garbage = root.join("Mouflon-linux-x86_64.tar.gz");
        std::fs::write(&garbage, b"pas une archive").unwrap();
        let unreadable = refused(&garbage);
        assert!(
            matches!(unreadable, InstallError::Unpack(_)),
            "{unreadable}"
        );
        assert_eq!(unreadable.blame(), Blame::Moment);
        // Un fichier changé depuis que la liste l'a annoncé : il n'est même
        // pas ouvert.
        let good = package(&root, "Mouflon 9.9.9", Arrives::Badly);
        let mut said = announced(&good);
        said.sha512[0] ^= 1;
        let changed = home.prepare(&good, &said, NEW, lock(&root)).unwrap_err();
        assert!(matches!(changed, InstallError::Changed), "{changed}");
        assert_eq!(changed.blame(), Blame::Moment);
        assert_eq!(snapshot(&home.dir), before);
        let _ = std::fs::remove_dir_all(root);
    }

    #[test]
    fn an_exchange_that_stops_halfway_puts_everything_back() {
        let root = scratch("a-moitie");
        let home = installed(&root);
        let before = snapshot(&home.dir);
        let package = package(&root, "Mouflon 9.9.9", Arrives::Badly);
        let ready = prepared(&home, &root, &package).unwrap();
        // La dernière chose à échanger a disparu entre-temps.
        let last = ready.moves.last().unwrap().from.clone().unwrap();
        std::fs::remove_dir_all(&last).unwrap();
        let error = ready.swap().unwrap_err();
        assert!(matches!(error, InstallError::Disk(_, _)), "{error}");
        assert_eq!(error.blame(), Blame::Moment);
        assert_eq!(snapshot(&home.dir), before);
        let _ = std::fs::remove_dir_all(root);
    }

    /// Fait ce que fait l'échange, mais s'arrête en route : après `whole`
    /// choses échangées, et, si `half`, l'ancienne suivante déjà mise de
    /// côté sans que la nouvelle ait pris sa place.
    fn interrupted(ready: &Ready, whole: usize, half: bool) {
        let backup = ready.dir.join(BACKUP);
        std::fs::create_dir(&backup).unwrap();
        let record = Record {
            version: ready.version,
            moves: ready
                .moves
                .iter()
                .map(|m| Kept {
                    had: there(&ready.dir.join(&m.to)),
                    arrives: m.from.is_some(),
                    to: m.to.clone(),
                })
                .collect(),
        };
        record.write(&backup).unwrap();
        for (index, (m, kept)) in ready.moves.iter().zip(&record.moves).enumerate() {
            if index > whole || (index == whole && !half) {
                break;
            }
            if index < whole {
                exchange(&ready.dir, m, kept).unwrap();
            } else if kept.had {
                // L'ancienne déjà de côté, la nouvelle pas encore là.
                rename_into(&ready.dir.join(&m.to), &backup.join(&m.to)).unwrap();
            }
        }
    }

    #[test]
    fn an_exchange_cut_short_anywhere_is_undone_at_the_next_launch() {
        // Quatre choses à échanger : coupé avant la première, entre deux,
        // au milieu d'une, après la dernière.
        for whole in 0..=4 {
            for half in [false, true] {
                if whole == 4 && half {
                    continue;
                }
                let root = scratch(&format!("coupure-{whole}-{half}"));
                let home = installed(&root);
                let before = snapshot(&home.dir);
                let package = package(&root, "Mouflon 9.9.9", Arrives::Badly);
                let ready = prepared(&home, &root, &package).unwrap();
                interrupted(&ready, whole, half);
                // Tout s'arrête là (le verrou tombe avec le programme).
                drop(ready);
                // Le Mouflon lancé ensuite, quel qu'il soit, remet
                // l'ancienne version entière.
                let next = if whole >= 2 { NEW } else { OLD };
                assert_eq!(home.tidy(next), Tidy::Clean, "{whole} {half}");
                assert_eq!(snapshot(&home.dir), before, "{whole} {half}");
                // Et le refaire ne change plus rien.
                assert_eq!(home.tidy(next), Tidy::Clean);
                assert_eq!(snapshot(&home.dir), before);
                let _ = std::fs::remove_dir_all(root);
            }
        }
    }

    #[test]
    fn an_old_version_that_cannot_come_back_is_never_thrown_away() {
        let root = scratch("retour-empeche");
        let home = installed(&root);
        write(&home.dir.join("pets/pelote/pet.toml"), "pelote", false);
        let before = snapshot(&home.dir);
        // Une version qui retire un animal.
        let made = root.join("fabrique");
        folder(
            &made.join("Mouflon"),
            "Mouflon 9.9.9",
            Arrives::Badly,
            &["texel"],
        );
        write(&made.join("Mouflon").join(REMOVALS), "pets/pelote\n", false);
        let swapped = prepared(&home, &root, &archive(&root, &made))
            .unwrap()
            .swap()
            .unwrap();
        // Quelque chose a pris sa place entre-temps : il ne peut pas revenir.
        write(&home.dir.join("pets/pelote/intrus.txt"), "intrus", false);
        assert!(swapped.undo().is_err());
        // Le reste de l'ancienne version est revenu ; ce qui n'a pas pu
        // revenir attend toujours à côté, avec la note ; et l'échange n'est
        // plus dit « fini » : personne ne jettera ce qui attend.
        assert_eq!(says(&home.dir.join("mouflon")), "Mouflon 1.0.0");
        let backup = home.dir.join(BACKUP);
        assert!(backup.join("pets/pelote/pet.toml").is_file());
        assert!(Record::read(&backup).is_some() && !Record::finished(&backup));
        // Tant que c'est ainsi, le ménage le dit, quelle que soit la version
        // qui tourne et le temps passé, et ne jette rien.
        for own in [OLD, NEW] {
            assert_eq!(home.tidy(own), Tidy::Stuck);
            assert!(backup.join("pets/pelote/pet.toml").is_file());
        }
        // L'obstacle levé, le prochain lancement finit le retour.
        std::fs::remove_dir_all(home.dir.join("pets/pelote")).unwrap();
        assert_eq!(home.tidy(NEW), Tidy::Clean);
        assert_eq!(snapshot(&home.dir), before);
        let _ = std::fs::remove_dir_all(root);
    }

    #[test]
    fn an_exchange_is_finished_only_once_it_says_so() {
        let root = scratch("fini");
        let home = installed(&root);
        let before = snapshot(&home.dir);
        let package = package(&root, "Mouflon 9.9.9", Arrives::Badly);
        let ready = prepared(&home, &root, &package).unwrap();
        // Tout a changé de place, mais le geste qui dit « fini » n'a pas
        // passé (le courant, un disque plein) : pour la version qui arrive
        // comme pour toute autre, l'échange n'a pas eu lieu.
        interrupted(&ready, ready.moves.len(), false);
        drop(ready);
        let backup = home.dir.join(BACKUP);
        assert!(backup.join(DONE_READY).is_file() && !Record::finished(&backup));
        assert_eq!(home.tidy(NEW), Tidy::Clean);
        assert_eq!(snapshot(&home.dir), before);

        // Fini pour de bon, puis un ménage coupé après avoir perdu sa note :
        // ce que la version avait retiré ne revient pas.
        write(&home.dir.join("vieux.txt"), "d'une ancienne version", false);
        let made = root.join("fabrique");
        let _ = std::fs::remove_dir_all(&made);
        folder(
            &made.join("Mouflon"),
            "Mouflon 9.9.9",
            Arrives::Badly,
            &["texel"],
        );
        write(&made.join("Mouflon").join(REMOVALS), "vieux.txt\n", false);
        prepared(&home, &root, &archive(&root, &made))
            .unwrap()
            .swap()
            .unwrap()
            .settle();
        age_record(&home.dir, Duration::from_secs(600));
        let after = visible(&home.dir);
        // (Le ménage met d'abord toute l'ancienne version au rebut, d'un
        // geste : coupé là, il ne reste rien à prendre pour « à remettre ».)
        std::fs::rename(&backup, home.dir.join(REFUSED)).unwrap();
        std::fs::remove_file(home.dir.join(REFUSED).join(RECORD)).unwrap();
        assert_eq!(home.tidy(NEW), Tidy::Clean);
        assert_eq!(visible(&home.dir), after);
        assert!(!home.dir.join("vieux.txt").exists() && hidden(&home.dir).is_empty());
        let _ = std::fs::remove_dir_all(root);
    }

    #[test]
    fn what_waits_without_a_note_is_put_back_where_it_is_missing() {
        let root = scratch("sans-note");
        let home = installed(&root);
        let before = snapshot(&home.dir);
        // Une ancienne version mise de côté, sa note perdue : le programme
        // et un animal manquent dans le dossier, le reste y est.
        let backup = home.dir.join(BACKUP);
        std::fs::create_dir_all(backup.join("pets")).unwrap();
        std::fs::rename(home.dir.join("mouflon"), backup.join("mouflon")).unwrap();
        std::fs::rename(home.dir.join("pets/texel"), backup.join("pets/texel")).unwrap();
        write(
            &backup.join("LISEZMOI.txt"),
            "une copie plus ancienne",
            false,
        );
        // Une note coupée avant sa fin, et ce qui traîne d'une écriture.
        write(&backup.join(RECORD), "version 9.9.9\n", false);
        write(
            &backup.join(format!("{RECORD}.4242.partiel")),
            "version",
            false,
        );
        write(&backup.join(DONE_READY), "", false);
        assert_eq!(home.tidy(OLD), Tidy::Clean);
        assert_eq!(snapshot(&home.dir), before);
        let _ = std::fs::remove_dir_all(root);
    }

    #[test]
    fn what_a_version_no_longer_ships_goes_and_would_come_back_with_the_old_one() {
        let root = scratch("retraits");
        let home = installed(&root);
        write(&home.dir.join("vieux.txt"), "d'une ancienne version", false);
        write(&home.dir.join("pets/pelote/pet.toml"), "pelote", false);
        let before = snapshot(&home.dir);
        let made = root.join("fabrique");
        folder(
            &made.join("Mouflon"),
            "Mouflon 9.9.9",
            Arrives::Badly,
            &["texel"],
        );
        // Ce qu'elle retire ; et ce qui ne se retire pas : le programme, le
        // dossier des animaux, ce qu'elle livre, ce qui sort du dossier.
        write(
            &made.join("Mouflon").join(REMOVALS),
            "# plus livrés\nvieux.txt\npets/pelote\n\nmouflon\nMOUFLON\npets\nPETS\nLISEZMOI.txt\nlisezmoi.TXT\npets/TEXEL\n../notes.txt\npets/../notes.txt\n/etc/passwd\nC:notes.txt\nabsent.txt\n",
            false,
        );
        let ready = prepared(&home, &root, &archive(&root, &made)).unwrap();
        let places: Vec<_> = ready
            .moves
            .iter()
            .map(|m| (m.to.as_str(), m.from.is_some()))
            .collect();
        assert_eq!(
            places,
            [
                ("LISEZMOI.txt", true),
                ("mouflon", true),
                ("pets/texel", true),
                ("vieux.txt", false),
                ("pets/pelote", false),
            ]
        );
        let swapped = ready.swap().unwrap();
        assert!(!home.dir.join("vieux.txt").exists());
        assert!(!home.dir.join("pets/pelote").exists());
        // La liste des retraits, elle, n'est pas installée ; et ce que
        // l'utilisateur a posé reste.
        assert!(!home.dir.join(REMOVALS).exists());
        assert!(home.dir.join("pets/le-mien/pet.toml").is_file());
        assert!(home.dir.join("notes.txt").is_file());
        swapped.undo().unwrap();
        assert_eq!(snapshot(&home.dir), before);
        let _ = std::fs::remove_dir_all(root);
    }

    #[test]
    fn the_note_of_an_exchange_is_read_back_whole_or_not_at_all() {
        let root = scratch("note");
        let record = Record {
            version: NEW,
            moves: vec![
                Kept {
                    had: true,
                    arrives: true,
                    to: "Essai de Mouflon.cmd".into(),
                },
                Kept {
                    had: true,
                    arrives: false,
                    to: "pets/pelote".into(),
                },
            ],
        };
        record.write(&root).unwrap();
        assert_eq!(Record::read(&root), Some(record));
        // « Fini » se dit à côté, d'un renommage ; et se retire de même.
        assert!(!Record::finished(&root));
        Record::finish(&root).unwrap();
        assert!(Record::finished(&root) && !Record::finished_for(&root, KEPT_FOR));
        assert!(Record::finished_for(&root, Duration::ZERO));
        Record::reopen(&root);
        assert!(!Record::finished(&root));
        let text = std::fs::read_to_string(root.join(RECORD)).unwrap();
        // Coupée avant la fin, ou avec une place qui sort du dossier (par
        // un chemin, un lecteur de Windows, le dossier des animaux en
        // bloc) : pas de note.
        for spoiled in [
            text.replace("fin\n", ""),
            text.replace("pets/pelote", "../ailleurs"),
            text.replace("pets/pelote", "pets/a/b"),
            text.replace("pets/pelote", "C:ailleurs"),
            text.replace("pets/pelote", "PETS"),
            text.replace("pets/pelote", ".mouflon-echange.txt"),
            text.replace("place 1 0", "place 2 0"),
            String::new(),
        ] {
            std::fs::write(root.join(RECORD), &spoiled).unwrap();
            assert_eq!(Record::read(&root), None, "{spoiled}");
        }
        let _ = std::fs::remove_dir_all(root);
    }

    #[test]
    fn a_place_is_a_plain_name_on_every_system() {
        for good in [
            "mouflon",
            "Mouflon.exe",
            "Essai de Mouflon.cmd",
            "pets/texel",
            "LICENSE-MIT",
            // (Pas des appareils de Windows, malgré l'air.)
            "console.txt",
            "COM10",
            "com0",
            "pets/nulle",
        ] {
            assert!(place(good), "{good}");
        }
        for bad in [
            "",
            ".",
            "..",
            "pets",
            "Pets",
            "pets/",
            "pets/..",
            "a/b",
            "pets/a/b",
            "..\\x",
            "C:x",
            "pets/C:x",
            "x.",
            "x ",
            " x",
            "a*b",
            "a?b",
            "a\"b",
            "a<b",
            "a|b",
            "a\nb",
            ".mouflon-ancien",
            "pets/.mouflon-sonde",
            // Les appareils de Windows, avec ou sans extension.
            "NUL",
            "nul.txt",
            "con",
            "Aux.tar.gz",
            "COM1",
            "lpt9.log",
            "pets/prn",
        ] {
            assert!(!place(bad), "{bad:?}");
        }
    }

    #[test]
    fn a_working_build_never_replaces_itself() {
        let root = scratch("travail");
        let dir = root.join("target").join("release");
        write(&dir.join("mouflon"), "x", true);
        let layout = Layout::Folder { program: "mouflon" };
        assert!(matches!(
            Home::of(&dir.join("mouflon"), layout, true),
            Standing::Never
        ));
        assert!(!in_cargo_target(&dir.join("mouflon")));
        write(&root.join("target").join("CACHEDIR.TAG"), "", false);
        assert!(in_cargo_target(&dir.join("mouflon")));
        let cross = root.join("target/x86_64-unknown-linux-gnu/release/mouflon");
        assert!(in_cargo_target(&cross));
        // Ni une app de Mac sans signature à elle (ou ailleurs que sur Mac).
        let app = root.join("Mouflon.app/Contents/MacOS/mouflon");
        assert_eq!(bundle_of(&app), Some(root.join("Mouflon.app")));
        assert_eq!(bundle_of(&dir.join("mouflon")), None);
        if !cfg!(target_os = "macos") {
            assert!(matches!(
                Home::of(&app, Layout::MacApp, false),
                Standing::Never
            ));
        }
        // Un programme au nom impossible à noter : Mouflon dira seulement
        // qu'une version existe.
        let odd = root.join("ici").join("mou\nflon");
        write(&odd, "x", true);
        assert!(matches!(Home::of(&odd, layout, false), Standing::Tell));
        let _ = std::fs::remove_dir_all(root);
    }

    #[test]
    fn where_mouflon_may_not_write_it_only_tells() {
        let root = scratch("protege");
        let dir = root.join("opt").join("Mouflon");
        write(&dir.join("mouflon"), "x", true);
        std::fs::set_permissions(&dir, std::fs::Permissions::from_mode(0o555)).unwrap();
        // (L'administrateur écrit partout : rien à voir avec ce compte-là.)
        let barred = std::fs::create_dir(dir.join("essai")).is_err();
        let layout = Layout::Folder { program: "mouflon" };
        let standing = Home::of(&dir.join("mouflon"), layout, false);
        if barred {
            assert!(matches!(standing, Standing::Tell), "{standing:?}");
        } else {
            let _ = std::fs::remove_dir(dir.join("essai"));
            assert!(matches!(standing, Standing::Own(_)), "{standing:?}");
        }
        // Le dossier n'a rien gardé de l'essai.
        assert_eq!(std::fs::read_dir(&dir).unwrap().count(), 1);
        std::fs::set_permissions(&dir, std::fs::Permissions::from_mode(0o755)).unwrap();
        let _ = std::fs::remove_dir_all(root);
    }

    #[test]
    fn one_update_at_a_time() {
        let root = scratch("verrou");
        let first = lock(&root);
        assert!(Lock::take(&dirs(&root)).is_none());
        drop(first);
        assert!(Lock::take(&dirs(&root)).is_some());
        let _ = std::fs::remove_dir_all(root);
    }
}
