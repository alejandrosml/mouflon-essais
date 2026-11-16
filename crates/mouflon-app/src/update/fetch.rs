//! Aller chercher une version : la liste, sa signature, puis le fichier.
//!
//! Par le `curl` du système (macOS, Windows 10 et 11, la plupart des Linux),
//! comme un navigateur le ferait : Mouflon n'a pas de code réseau à lui, et
//! la demande ne dit rien de l'utilisateur ni de sa machine. Tout ce qui est
//! reçu est vérifié avant de servir : la liste par sa signature
//! (`release.rs`), le fichier par sa taille et sa somme.

use std::io::Read;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::time::{Duration, Instant};

use super::release::{File, LIST, LIST_MAX_BYTES, LIST_SIGNATURE, Release, ReleaseError, Version};
use super::state::write_whole;
use super::tool;

/// D'où viennent les versions : la dernière, dans le dépôt public des
/// téléchargements. GitHub renvoie de là vers les fichiers de la version.
#[cfg_attr(feature = "essai-maj", allow(dead_code))]
const OFFICIAL: &str = "https://github.com/alejandrosml/mouflon-download/releases/latest/download";

/// Une signature tient en quelques centaines d'octets.
const SIGNATURE_MAX_BYTES: u64 = 4 * 1024;
/// Ce qu'on laisse à une petite demande, puis à un téléchargement (repris
/// où il en était la fois suivante, s'il n'a pas fini).
const SMALL_SECONDS: u32 = 30;
const FILE_SECONDS: u32 = 1800;
/// Ce que Mouflon laisse de plus à curl pour s'arrêter de lui-même, avant
/// de l'arrêter.
const GRACE: Duration = Duration::from_secs(15);

/// L'endroit où chercher les versions.
#[derive(Debug, Clone)]
pub struct Source {
    base: String,
    /// Les essais lisent un dossier (`file://`) ; le vrai Mouflon, jamais.
    local: bool,
}

impl Source {
    /// Le dépôt public des téléchargements, et lui seul.
    #[cfg_attr(feature = "essai-maj", allow(dead_code))]
    pub fn official() -> Source {
        Source {
            base: OFFICIAL.to_string(),
            local: false,
        }
    }

    /// Un dossier qui tient lieu de dépôt, pour les essais.
    #[cfg(any(test, feature = "essai-maj"))]
    pub fn folder(dir: &Path) -> Source {
        let path = dir.to_string_lossy().replace('\\', "/");
        let base = if path.starts_with('/') {
            format!("file://{path}")
        } else {
            format!("file:///{path}")
        };
        Source { base, local: true }
    }

    fn url(&self, name: &str) -> String {
        format!("{}/{name}", self.base)
    }
}

/// Pourquoi rien n'a été rapporté.
#[derive(Debug, thiserror::Error)]
pub enum FetchError {
    #[error("pas de curl sur ce système ({0})")]
    NoCurl(std::io::Error),
    #[error("{0} n'a pas pu être téléchargé ({1})")]
    Download(String, String),
    /// Le téléchargement s'est arrêté en route : ce qui est arrivé est
    /// gardé, et il reprendra de là.
    #[error("le téléchargement de {0} s'est arrêté en route ({1})")]
    Interrupted(String, String),
    #[error("{0}")]
    Release(#[from] ReleaseError),
    #[error("la version {0} n'a pas de fichier pour ce système")]
    NoFile(Version),
    #[error("le fichier reçu n'est pas celui que la liste annonce")]
    Mismatch,
    #[error("{0}")]
    Disk(#[from] std::io::Error),
}

/// Une version plus récente que celle qui tourne, et son fichier pour ce
/// système.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Found {
    pub version: Version,
    pub file: File,
    /// La liste qui l'annonce et sa signature, telles que reçues : gardées
    /// avec le fichier, elles permettent de le revérifier plus tard.
    list: Vec<u8>,
    signature: String,
}

impl Found {
    /// Le nom du fichier de cette version, une fois téléchargé.
    fn kept_name(&self) -> String {
        format!("Mouflon-{}-{}", self.version, self.file.name)
    }
}

/// Y a-t-il une version plus récente que `current` ? La liste est lue, sa
/// signature vérifiée avec `public_key` ; `wanted` est le fichier de ce
/// système (`release::own_file`).
pub fn check(
    source: &Source,
    public_key: &str,
    current: Version,
    wanted: &str,
) -> Result<Option<Found>, FetchError> {
    let list = small(source, LIST, LIST_MAX_BYTES)?;
    let signature = small(source, LIST_SIGNATURE, SIGNATURE_MAX_BYTES)?;
    let signature = String::from_utf8_lossy(&signature).into_owned();
    read(public_key, current, wanted, list, signature)
}

/// Lit une liste et sa signature : la version qu'elles annoncent, si elle
/// est plus récente que `current`.
fn read(
    public_key: &str,
    current: Version,
    wanted: &str,
    list: Vec<u8>,
    signature: String,
) -> Result<Option<Found>, FetchError> {
    let release = Release::read(public_key, &list, &signature)?;
    // Jamais en arrière, ni sur place : une vieille liste rejouée ne fait rien.
    if release.version <= current {
        return Ok(None);
    }
    let file = release
        .file(wanted)
        .cloned()
        .ok_or(FetchError::NoFile(release.version))?;
    Ok(Some(Found {
        version: release.version,
        file,
        list,
        signature,
    }))
}

/// La version déjà téléchargée dans `dir`, s'il y en a une qui vaut encore :
/// sa liste gardée est bien signée, elle est plus récente que `current`, et
/// son fichier est là, intact. Rien n'est demandé au réseau.
pub fn kept(
    dir: &Path,
    public_key: &str,
    current: Version,
    wanted: &str,
) -> Option<(Found, PathBuf)> {
    let list = read_small(&dir.join(LIST), LIST_MAX_BYTES)?;
    let signature = read_small(&dir.join(LIST_SIGNATURE), SIGNATURE_MAX_BYTES)?;
    let signature = String::from_utf8_lossy(&signature).into_owned();
    let found = read(public_key, current, wanted, list, signature).ok()??;
    let path = dir.join(found.kept_name());
    found
        .file
        .matches(&path)
        .unwrap_or(false)
        .then_some((found, path))
}

/// Un petit fichier du disque, s'il ne dépasse pas `max_bytes`.
fn read_small(path: &Path, max_bytes: u64) -> Option<Vec<u8>> {
    (std::fs::metadata(path).ok()?.len() <= max_bytes)
        .then(|| std::fs::read(path).ok())
        .flatten()
}

/// Télécharge le fichier d'une version dans `dir`, et le vérifie : le chemin
/// rendu est celui d'un fichier qui a la taille et la somme annoncées. Un
/// fichier déjà là et juste n'est pas repris ; un fichier faux ne reste pas.
/// La liste et sa signature sont gardées à côté (voir [`kept`]) ; ce qui
/// restait d'une autre version s'en va.
pub fn download(source: &Source, found: &Found, dir: &Path) -> Result<PathBuf, FetchError> {
    std::fs::create_dir_all(dir)?;
    let kept = found.kept_name();
    let path = dir.join(&kept);
    let partial_name = format!("{kept}.partiel");
    let partial = dir.join(&partial_name);
    for entry in std::fs::read_dir(dir)?.flatten() {
        let name = entry.file_name();
        if name != *kept && name != *partial_name {
            let _ = std::fs::remove_file(entry.path());
        }
    }
    write_whole(&dir.join(LIST), &found.list)?;
    write_whole(&dir.join(LIST_SIGNATURE), found.signature.as_bytes())?;
    if found.file.matches(&path).unwrap_or(false) {
        let _ = std::fs::remove_file(&partial);
        return Ok(path);
    }
    // Ce qui est déjà arrivé d'un téléchargement interrompu sert de départ,
    // s'il en manque encore ; tout sera vérifié à la fin.
    let arrived = std::fs::metadata(&partial).map_or(0, |meta| meta.len());
    if arrived >= found.file.size && !found.file.matches(&partial).unwrap_or(false) {
        let _ = std::fs::remove_file(&partial);
    }
    let fetched = if found.file.matches(&partial).unwrap_or(false) {
        Ok(())
    } else {
        fetch(
            source,
            &found.file.name,
            found.file.size,
            FILE_SECONDS,
            &partial,
        )
        .and_then(|()| match found.file.matches(&partial) {
            Ok(true) => Ok(()),
            Ok(false) => Err(FetchError::Mismatch),
            Err(e) => Err(FetchError::Disk(e)),
        })
    }
    .and_then(|()| std::fs::rename(&partial, &path).map_err(FetchError::Disk));
    // Seul un téléchargement interrompu laisse ce qu'il a reçu.
    if !matches!(fetched, Ok(()) | Err(FetchError::Interrupted(..))) {
        let _ = std::fs::remove_file(&partial);
    }
    fetched.map(|()| path)
}

/// Un petit fichier, en mémoire : au plus `max_bytes`. Mouflon ne garde
/// jamais plus que ce qu'un outil a le droit d'écrire (`tool::KEPT_BYTES`),
/// quoi que le serveur envoie, et n'attend pas plus que prévu.
fn small(source: &Source, name: &str, max_bytes: u64) -> Result<Vec<u8>, FetchError> {
    let limit = Duration::from_secs(u64::from(SMALL_SECONDS)) + GRACE;
    let said = match tool::run(curl(source, name, max_bytes, SMALL_SECONDS), limit) {
        Ok(said) => said,
        Err(tool::ToolError::Start(_, e)) => return Err(FetchError::NoCurl(e)),
        Err(late) => return Err(FetchError::Download(name.to_string(), late.to_string())),
    };
    if !said.status.success() {
        return Err(FetchError::Download(name.to_string(), said.complaint()));
    }
    // (`--max-filesize` ne vaut que si le serveur annonce la taille.)
    if said.raw.len() as u64 > max_bytes {
        return Err(FetchError::Download(name.to_string(), "trop gros".into()));
    }
    Ok(said.raw)
}

/// Les arrêts de curl après lesquels ce qui est arrivé peut servir de
/// départ : transfert incomplet (18), délai passé (28), connexion perdue
/// (56).
const RESUMABLE: [i32; 3] = [18, 28, 56];

/// Un fichier, sur le disque, à la suite de ce qui s'y trouve déjà. Mouflon
/// surveille lui-même sa taille et le temps : un serveur qui n'annonce pas
/// la taille, ou un curl ancien, ne remplissent pas le disque.
fn fetch(
    source: &Source,
    name: &str,
    max_bytes: u64,
    seconds: u32,
    to: &Path,
) -> Result<(), FetchError> {
    let mut child = curl(source, name, max_bytes, seconds)
        .args(["--continue-at", "-"])
        .arg("-o")
        .arg(to)
        .stdout(Stdio::null())
        .spawn()
        .map_err(FetchError::NoCurl)?;
    let limit = Duration::from_secs(u64::from(seconds)) + GRACE;
    let started = Instant::now();
    let stopped = loop {
        match child.try_wait() {
            Ok(Some(status)) => break Ok(status),
            Ok(None) => {}
            Err(e) => break Err(FetchError::Disk(e)),
        }
        if std::fs::metadata(to).is_ok_and(|meta| meta.len() > max_bytes) {
            break Err(FetchError::Download(name.to_string(), "trop gros".into()));
        }
        if started.elapsed() >= limit {
            break Err(FetchError::Interrupted(
                name.to_string(),
                "trop long".into(),
            ));
        }
        std::thread::sleep(Duration::from_millis(250));
    };
    let status = match stopped {
        Ok(status) => status,
        Err(e) => {
            let _ = child.kill();
            let _ = child.wait();
            return Err(e);
        }
    };
    if status.success() {
        return Ok(());
    }
    let mut complaint = Vec::new();
    if let Some(mut stderr) = child.stderr.take() {
        let _ = stderr.by_ref().take(4096).read_to_end(&mut complaint);
    }
    let why = said(&complaint);
    if status.code().is_some_and(|code| RESUMABLE.contains(&code)) {
        Err(FetchError::Interrupted(name.to_string(), why))
    } else {
        Err(FetchError::Download(name.to_string(), why))
    }
}

/// Ce que curl a dit en échouant, en une ligne.
fn said(stderr: &[u8]) -> String {
    String::from_utf8_lossy(stderr)
        .split_whitespace()
        .collect::<Vec<_>>()
        .join(" ")
}

/// Le curl du système, prêt à demander `name` : il suit les renvois de
/// GitHub, sans sortir de HTTPS, n'attend pas indéfiniment, et s'arrête sur
/// un fichier plus gros qu'annoncé.
fn curl(source: &Source, name: &str, max_bytes: u64, seconds: u32) -> Command {
    let protocols = if source.local { "=file" } else { "=https" };
    let mut command = Command::new(curl_path());
    command
        // Pas de réglages de l'utilisateur (`.curlrc`) : à dire en premier
        // pour être entendu.
        .arg("--disable")
        .args(["--fail", "--silent", "--show-error", "--location"])
        .args(["--proto", protocols, "--proto-redir", protocols])
        .args(["--max-time", &seconds.to_string()])
        .args(["--max-filesize", &max_bytes.to_string()])
        .arg(source.url(name))
        .stdin(Stdio::null())
        .stderr(Stdio::piped());
    tool::quiet(&mut command);
    command
}

/// Où est le curl du système. Sous Windows, celui de Windows par son chemin
/// entier : un `curl.exe` posé ailleurs ne passe pas devant.
fn curl_path() -> PathBuf {
    if cfg!(target_os = "macos") {
        PathBuf::from("/usr/bin/curl")
    } else if cfg!(target_os = "windows") {
        tool::system32("curl.exe")
    } else {
        PathBuf::from("curl")
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    // La liste signée de `release.rs` : la version 1.0.5, et un fichier pour
    // Mac de trois octets, « abc ».
    const KEY: &str =
        "ssh-ed25519 AAAAC3NzaC1lZDI1NTE5AAAAIO8IgUfcdPG/3t07v2Por7d8cg0lkJGIySZftby/cG5c";
    const LIST_TEXT: &str = "version 1.0.5
fichier Mouflon-macos-arm64.zip 3 ddaf35a193617abacc417349ae20413112e6fa4e89a97ea20a9eeee64b55d39a2192992a274fc1a836ba3c23a3feebbd454d4423643ce80e2a9ac94fa54ca49f
";
    const LIST_SIG: &str = "-----BEGIN SSH SIGNATURE-----
U1NIU0lHAAAAAQAAADMAAAALc3NoLWVkMjU1MTkAAAAg7wiBR9x08b/e3Tu/Y+ivt3xyDS
WQkYjJJl+1vL9wblwAAAAHbW91ZmxvbgAAAAAAAAAGc2hhNTEyAAAAUwAAAAtzc2gtZWQy
NTUxOQAAAECy7Zpp7yDGw4tiK0WJy09W8if6+KA90ShNJz/+/imDa9kvfGIPTPsllHDJjV
GarYEBY4Oq15jsLtSnSwQurVkA
-----END SSH SIGNATURE-----
";
    const MAC: &str = "Mouflon-macos-arm64.zip";

    /// Un dossier qui tient lieu de dépôt public, avec la version 1.0.5.
    struct Shelf(PathBuf);

    impl Shelf {
        fn new(name: &str) -> Shelf {
            let dir =
                std::env::temp_dir().join(format!("mouflon-maj-{name}-{}", std::process::id()));
            let _ = std::fs::remove_dir_all(&dir);
            std::fs::create_dir_all(dir.join("depot")).unwrap();
            let shelf = Shelf(dir);
            shelf.put(LIST, LIST_TEXT.as_bytes());
            shelf.put(LIST_SIGNATURE, LIST_SIG.as_bytes());
            shelf.put(MAC, b"abc");
            shelf
        }

        fn put(&self, name: &str, content: &[u8]) {
            std::fs::write(self.0.join("depot").join(name), content).unwrap();
        }

        fn source(&self) -> Source {
            Source::folder(&self.0.join("depot"))
        }

        fn downloads(&self) -> PathBuf {
            self.0.join("recu")
        }

        /// Ce qu'il y a dans le dossier des téléchargements, par ordre.
        fn received(&self) -> Vec<String> {
            let mut names: Vec<String> = std::fs::read_dir(self.downloads())
                .unwrap()
                .map(|e| e.unwrap().file_name().to_string_lossy().into_owned())
                .collect();
            names.sort();
            names
        }
    }

    impl Drop for Shelf {
        fn drop(&mut self) {
            let _ = std::fs::remove_dir_all(&self.0);
        }
    }

    #[test]
    fn a_newer_version_is_found_with_its_file() {
        let shelf = Shelf::new("trouve");
        let found = check(&shelf.source(), KEY, Version(1, 0, 4), MAC)
            .unwrap()
            .unwrap();
        assert_eq!(found.version, Version(1, 0, 5));
        assert_eq!((found.file.name.as_str(), found.file.size), (MAC, 3));
    }

    #[test]
    fn the_same_version_or_an_older_one_is_not_an_update() {
        let shelf = Shelf::new("pareil");
        assert_eq!(
            check(&shelf.source(), KEY, Version(1, 0, 5), MAC).unwrap(),
            None
        );
        assert_eq!(
            check(&shelf.source(), KEY, Version(1, 1, 0), MAC).unwrap(),
            None
        );
    }

    #[test]
    fn a_list_signed_by_someone_else_announces_nothing() {
        let shelf = Shelf::new("autre-cle");
        let other =
            "ssh-ed25519 AAAAC3NzaC1lZDI1NTE5AAAAILBM3oF6KrdJ6bu8ASYewhUcWz1ZquO51kldsc0ijpGa";
        assert!(matches!(
            check(&shelf.source(), other, Version(1, 0, 4), MAC),
            Err(FetchError::Release(ReleaseError::Signature(_)))
        ));
        // Ni une liste retouchée en chemin.
        shelf.put(LIST, LIST_TEXT.replace("1.0.5", "9.9.9").as_bytes());
        assert!(matches!(
            check(&shelf.source(), KEY, Version(1, 0, 4), MAC),
            Err(FetchError::Release(ReleaseError::Signature(_)))
        ));
    }

    #[test]
    fn a_version_without_a_file_for_this_system_is_said_so() {
        let shelf = Shelf::new("sans-fichier");
        assert!(matches!(
            check(
                &shelf.source(),
                KEY,
                Version(1, 0, 4),
                "Mouflon-linux-x86_64.tar.gz"
            ),
            Err(FetchError::NoFile(Version(1, 0, 5)))
        ));
    }

    #[test]
    fn a_missing_list_or_signature_is_a_failed_download() {
        let shelf = Shelf::new("absent");
        std::fs::remove_file(shelf.0.join("depot").join(LIST_SIGNATURE)).unwrap();
        assert!(matches!(
            check(&shelf.source(), KEY, Version(1, 0, 4), MAC),
            Err(FetchError::Download(name, _)) if name == LIST_SIGNATURE
        ));
        let nowhere = Source::folder(&shelf.0.join("nulle-part"));
        assert!(matches!(
            check(&nowhere, KEY, Version(1, 0, 4), MAC),
            Err(FetchError::Download(name, _)) if name == LIST
        ));
    }

    #[test]
    fn a_list_far_too_long_is_not_read() {
        let shelf = Shelf::new("trop-long");
        shelf.put(LIST, &vec![b'x'; LIST_MAX_BYTES as usize + 1]);
        assert!(matches!(
            check(&shelf.source(), KEY, Version(1, 0, 4), MAC),
            Err(FetchError::Download(name, _)) if name == LIST
        ));
    }

    #[test]
    fn the_file_is_downloaded_and_checked() {
        let shelf = Shelf::new("recu");
        let found = check(&shelf.source(), KEY, Version(1, 0, 4), MAC)
            .unwrap()
            .unwrap();
        let path = download(&shelf.source(), &found, &shelf.downloads()).unwrap();
        assert_eq!(std::fs::read(&path).unwrap(), b"abc");
        assert_eq!(
            path.file_name().unwrap(),
            "Mouflon-1.0.5-Mouflon-macos-arm64.zip"
        );
        // Déjà là, et juste : pas repris, même si le dépôt ne répond plus.
        std::fs::remove_file(shelf.0.join("depot").join(MAC)).unwrap();
        assert_eq!(
            download(&shelf.source(), &found, &shelf.downloads()).unwrap(),
            path
        );
    }

    #[test]
    fn a_file_that_is_not_the_announced_one_does_not_stay() {
        let shelf = Shelf::new("faux");
        let found = check(&shelf.source(), KEY, Version(1, 0, 4), MAC)
            .unwrap()
            .unwrap();
        // Même taille, autre contenu.
        shelf.put(MAC, b"abd");
        assert!(matches!(
            download(&shelf.source(), &found, &shelf.downloads()),
            Err(FetchError::Mismatch)
        ));
        // Plus gros qu'annoncé : curl s'arrête, ou la vérification refuse.
        shelf.put(MAC, &vec![b'a'; 100_000]);
        assert!(download(&shelf.source(), &found, &shelf.downloads()).is_err());
        // Il ne reste que la liste et sa signature.
        assert_eq!(shelf.received(), [LIST, LIST_SIGNATURE]);
        // Un fichier faux déjà posé sous le bon nom est remplacé par le bon.
        shelf.put(MAC, b"abc");
        std::fs::write(
            shelf
                .downloads()
                .join("Mouflon-1.0.5-Mouflon-macos-arm64.zip"),
            b"xyz",
        )
        .unwrap();
        let path = download(&shelf.source(), &found, &shelf.downloads()).unwrap();
        assert_eq!(std::fs::read(path).unwrap(), b"abc");
    }

    #[test]
    fn an_interrupted_download_goes_on_from_where_it_was() {
        let shelf = Shelf::new("repris");
        let found = check(&shelf.source(), KEY, Version(1, 0, 4), MAC)
            .unwrap()
            .unwrap();
        std::fs::create_dir_all(shelf.downloads()).unwrap();
        let partial = shelf
            .downloads()
            .join("Mouflon-1.0.5-Mouflon-macos-arm64.zip.partiel");
        // Le début est déjà là : seule la suite est demandée.
        std::fs::write(&partial, b"ab").unwrap();
        let path = download(&shelf.source(), &found, &shelf.downloads()).unwrap();
        assert_eq!(std::fs::read(&path).unwrap(), b"abc");
        assert!(!partial.exists());
        // Un début qui n'est pas le bon : le fichier ne passe pas, et rien
        // n'en reste ; la fois d'après repart de zéro.
        std::fs::remove_file(&path).unwrap();
        std::fs::write(&partial, b"xy").unwrap();
        assert!(matches!(
            download(&shelf.source(), &found, &shelf.downloads()),
            Err(FetchError::Mismatch)
        ));
        assert!(!partial.exists());
        let path = download(&shelf.source(), &found, &shelf.downloads()).unwrap();
        assert_eq!(std::fs::read(&path).unwrap(), b"abc");
        // Déjà entier mais pas encore rangé : il l'est, sans rien demander.
        std::fs::rename(&path, &partial).unwrap();
        std::fs::remove_dir_all(shelf.0.join("depot")).unwrap();
        let path = download(&shelf.source(), &found, &shelf.downloads()).unwrap();
        assert_eq!(std::fs::read(path).unwrap(), b"abc");
    }

    #[test]
    fn a_downloaded_version_is_found_again_without_the_network() {
        let shelf = Shelf::new("garde");
        let found = check(&shelf.source(), KEY, Version(1, 0, 4), MAC)
            .unwrap()
            .unwrap();
        // Ce qui restait d'une autre version s'en va.
        std::fs::create_dir_all(shelf.downloads()).unwrap();
        std::fs::write(shelf.downloads().join("Mouflon-1.0.3-vieux.zip"), b"x").unwrap();
        let path = download(&shelf.source(), &found, &shelf.downloads()).unwrap();
        assert_eq!(
            shelf.received(),
            [
                "Mouflon-1.0.5-Mouflon-macos-arm64.zip",
                LIST,
                LIST_SIGNATURE
            ]
        );
        // Le dépôt peut disparaître : tout est là pour revérifier.
        std::fs::remove_dir_all(shelf.0.join("depot")).unwrap();
        let again = || kept(&shelf.downloads(), KEY, Version(1, 0, 4), MAC);
        assert_eq!(again(), Some((found.clone(), path.clone())));
        // Plus rien à installer quand on en est là, ni pour un autre système.
        assert_eq!(kept(&shelf.downloads(), KEY, Version(1, 0, 5), MAC), None);
        assert_eq!(
            kept(
                &shelf.downloads(),
                KEY,
                Version(1, 0, 4),
                "Mouflon-windows-x64.zip"
            ),
            None
        );
        // Une autre clé, un fichier retouché, une liste retouchée : rien.
        let other =
            "ssh-ed25519 AAAAC3NzaC1lZDI1NTE5AAAAILBM3oF6KrdJ6bu8ASYewhUcWz1ZquO51kldsc0ijpGa";
        assert_eq!(kept(&shelf.downloads(), other, Version(1, 0, 4), MAC), None);
        std::fs::write(&path, b"abd").unwrap();
        assert_eq!(again(), None);
        std::fs::write(&path, b"abc").unwrap();
        assert!(again().is_some());
        std::fs::write(
            shelf.downloads().join(LIST),
            LIST_TEXT.replace(" 3 ", " 4 "),
        )
        .unwrap();
        assert_eq!(again(), None);
        std::fs::remove_file(shelf.downloads().join(LIST)).unwrap();
        assert_eq!(again(), None);
    }

    #[test]
    fn the_official_source_speaks_https_only() {
        let source = Source::official();
        assert!(
            source
                .url(LIST)
                .starts_with("https://github.com/alejandrosml/mouflon-download/")
        );
        let command = curl(&source, LIST, 10, 5);
        let args: Vec<_> = command
            .get_args()
            .map(|a| a.to_string_lossy().into_owned())
            .collect();
        assert_eq!(args[0], "--disable");
        let at = args.iter().position(|a| a == "--proto").unwrap();
        assert_eq!(
            (args[at + 1].as_str(), args[at + 3].as_str()),
            ("=https", "=https")
        );
    }
}
