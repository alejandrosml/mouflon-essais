//! La liste des fichiers d'une version, telle que l'auteur la signe.
//!
//! Un texte court, publié avec la version (`Mouflon-version.txt`, et sa
//! signature `Mouflon-version.txt.sig`) :
//!
//! ```text
//! version 1.0.5
//! fichier Mouflon-macos-arm64.zip 38014168 <somme SHA-512, 128 chiffres>
//! fichier Mouflon-windows-x64.zip 38579864 <somme SHA-512>
//! …
//! ```
//!
//! Une ligne qui commence par un autre mot est ignorée : une version future
//! peut en ajouter sans qu'un Mouflon plus ancien cesse de lire la liste (il
//! lui faut la lire pour se mettre à jour). Ce que disent `version` et
//! `fichier` ne changera donc pas.

use std::io::Read;
use std::path::Path;

use ed25519_compact::sha512;

use super::signature::{self, SignatureError};

/// Les noms sous lesquels la liste et sa signature sortent avec une version.
pub const LIST: &str = "Mouflon-version.txt";
pub const LIST_SIGNATURE: &str = "Mouflon-version.txt.sig";

/// Une liste bien plus longue que celle-là n'est pas une liste.
pub const LIST_MAX_BYTES: u64 = 64 * 1024;

/// Un numéro de version : « 1.0.5 ».
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub struct Version(pub u32, pub u32, pub u32);

/// Le numéro de ce Mouflon-ci.
#[cfg(not(feature = "essai-maj"))]
const OWN: &str = env!("CARGO_PKG_VERSION");
/// Un programme d'essai peut en porter un autre, donné à sa compilation :
/// c'est ainsi qu'un essai a sous la main une version « plus récente ».
#[cfg(feature = "essai-maj")]
const OWN: &str = match option_env!("MOUFLON_ESSAI_VERSION") {
    Some(version) => version,
    None => env!("CARGO_PKG_VERSION"),
};

impl Version {
    /// La version de ce Mouflon-ci.
    pub fn current() -> Version {
        Version::parse(OWN).unwrap_or(Version(0, 0, 0))
    }

    /// « 1.0.5 », et rien d'autre : trois nombres, deux points.
    pub fn parse(text: &str) -> Option<Version> {
        let mut parts = text.split('.');
        let mut number = || {
            let part = parts.next()?;
            (!part.is_empty() && part.len() <= 6 && part.bytes().all(|b| b.is_ascii_digit()))
                .then(|| part.parse().ok())
                .flatten()
        };
        let version = Version(number()?, number()?, number()?);
        parts.next().is_none().then_some(version)
    }
}

impl std::fmt::Display for Version {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}.{}.{}", self.0, self.1, self.2)
    }
}

/// Un fichier d'une version : son nom, sa taille, sa somme.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct File {
    pub name: String,
    pub size: u64,
    pub sha512: [u8; 64],
}

/// Une version, telle que sa liste l'annonce.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Release {
    pub version: Version,
    pub files: Vec<File>,
}

/// Pourquoi une liste n'est pas reçue.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum ReleaseError {
    #[error("{0}")]
    Signature(#[from] SignatureError),
    #[error("la liste n'est pas un texte")]
    NotText,
    #[error("la liste ne dit pas sa version, ou la dit deux fois")]
    Version,
    #[error("ligne {0} : un fichier s'écrit « fichier <nom> <taille> <somme> »")]
    File(usize),
    #[error("ligne {0} : ce fichier est déjà dans la liste")]
    Twice(usize),
}

impl Release {
    /// Lit une liste, si elle est bien signée par `public_key` (voir
    /// `signature::verify`). Rien n'en est lu avant que la signature ne soit
    /// vérifiée.
    pub fn read(public_key: &str, list: &[u8], signature: &str) -> Result<Release, ReleaseError> {
        signature::verify(public_key, list, signature)?;
        Release::parse(std::str::from_utf8(list).map_err(|_| ReleaseError::NotText)?)
    }

    fn parse(text: &str) -> Result<Release, ReleaseError> {
        let mut version = None;
        let mut files: Vec<File> = Vec::new();
        for (index, line) in text.lines().enumerate() {
            let mut words = line.split_whitespace();
            match words.next() {
                Some("version") => {
                    let read = words.next().and_then(Version::parse);
                    if version.is_some() || read.is_none() || words.next().is_some() {
                        return Err(ReleaseError::Version);
                    }
                    version = read;
                }
                Some("fichier") => {
                    let file = file(&mut words).ok_or(ReleaseError::File(index + 1))?;
                    if files
                        .iter()
                        .any(|f| f.name.eq_ignore_ascii_case(&file.name))
                    {
                        return Err(ReleaseError::Twice(index + 1));
                    }
                    files.push(file);
                }
                // Autre chose : pour un Mouflon plus récent que celui-ci.
                _ => {}
            }
        }
        Ok(Release {
            version: version.ok_or(ReleaseError::Version)?,
            files,
        })
    }

    /// Le fichier de ce nom.
    pub fn file(&self, name: &str) -> Option<&File> {
        self.files.iter().find(|f| f.name == name)
    }
}

/// « <nom> <taille> <somme> », et rien après.
fn file<'a>(words: &mut impl Iterator<Item = &'a str>) -> Option<File> {
    let name = words.next().filter(|n| plain_name(n))?;
    let size = words
        .next()
        .filter(|s| s.len() <= 12 && s.bytes().all(|b| b.is_ascii_digit()))?;
    let sha512 = hex(words.next()?)?;
    words.next().is_none().then_some(File {
        name: name.to_string(),
        size: size.parse().ok()?,
        sha512,
    })
}

/// Un nom de fichier tout simple : des lettres, des chiffres, « . », « - » et
/// « _ », sans rien qui mène ailleurs que dans le dossier où on le pose.
fn plain_name(name: &str) -> bool {
    name.len() <= 80
        && name
            .bytes()
            .all(|b| b.is_ascii_alphanumeric() || matches!(b, b'.' | b'-' | b'_'))
        && name
            .bytes()
            .next()
            .is_some_and(|b| b.is_ascii_alphanumeric())
        && !name.contains("..")
}

/// Une somme SHA-512 en chiffres hexadécimaux, minuscules.
fn hex(text: &str) -> Option<[u8; 64]> {
    let text = text.as_bytes();
    if text.len() != 128 {
        return None;
    }
    let digit = |b: u8| match b {
        b'0'..=b'9' => Some(b - b'0'),
        b'a'..=b'f' => Some(b - b'a' + 10),
        _ => None,
    };
    let mut sum = [0u8; 64];
    for (byte, pair) in sum.iter_mut().zip(text.chunks(2)) {
        *byte = digit(pair[0])? << 4 | digit(pair[1])?;
    }
    Some(sum)
}

/// Le fichier que ce Mouflon-ci doit prendre dans une version, s'il en est
/// un pour ce système et ce processeur.
pub const fn own_file() -> Option<&'static str> {
    if cfg!(all(target_os = "macos", target_arch = "aarch64")) {
        Some("Mouflon-macos-arm64.zip")
    } else if cfg!(all(target_os = "windows", target_arch = "x86_64")) {
        Some("Mouflon-windows-x64.zip")
    } else if cfg!(all(target_os = "linux", target_arch = "x86_64")) {
        Some("Mouflon-linux-x86_64.tar.gz")
    } else {
        None
    }
}

impl File {
    /// Vrai si le fichier posé à `path` est bien celui-ci : même taille, même
    /// somme. Il est lu en entier, par morceaux.
    pub fn matches(&self, path: &Path) -> std::io::Result<bool> {
        let mut file = std::fs::File::open(path)?;
        if file.metadata()?.len() != self.size {
            return Ok(false);
        }
        let mut hash = sha512::Hash::new();
        let mut buffer = vec![0u8; 64 * 1024];
        let mut read = 0u64;
        loop {
            let n = file.read(&mut buffer)?;
            if n == 0 {
                break;
            }
            read += n as u64;
            hash.update(&buffer[..n]);
        }
        Ok(read == self.size && hash.finalize() == self.sha512)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const SUM: &str = "cf83e1357eefb8bdf1542850d66d8007d620e4050b5715dc83f4a921d36ce9ce47d0d13c5d85f2b0ff8318d2877eec2f63b931bd47417a81a538327af927da3e";

    fn list(lines: &[&str]) -> String {
        lines
            .iter()
            .map(|l| l.replace("<somme>", SUM) + "\n")
            .collect()
    }

    #[test]
    fn a_list_gives_its_version_and_its_files() {
        let release = Release::parse(&list(&[
            "version 1.0.5",
            "fichier Mouflon-macos-arm64.zip 38014168 <somme>",
            "fichier Mouflon-linux-x86_64.tar.gz 0 <somme>",
        ]))
        .unwrap();
        assert_eq!(release.version, Version(1, 0, 5));
        assert_eq!(release.files.len(), 2);
        let mac = release.file("Mouflon-macos-arm64.zip").unwrap();
        assert_eq!(
            (mac.size, mac.sha512[0], mac.sha512[63]),
            (38_014_168, 0xcf, 0x3e)
        );
        assert!(release.file("Mouflon-windows-x64.zip").is_none());
    }

    #[test]
    fn lines_a_newer_mouflon_would_understand_are_ignored() {
        let release = Release::parse(&list(&[
            "",
            "exige macos 27",
            "version 2.0.0",
            "  fichier   a.zip   12   <somme>  ",
            "fichiers en plus : aucun",
        ]))
        .unwrap();
        assert_eq!(release.version, Version(2, 0, 0));
        assert_eq!(release.files.len(), 1);
    }

    #[test]
    fn a_list_without_its_version_or_with_two_is_refused() {
        for lines in [
            &["fichier a.zip 1 <somme>"][..],
            &["version 1.0.5", "version 1.0.6"],
            &["version 1.0"],
            &["version 1.0.5 beta"],
            &["version v1.0.5"],
            &["version 1.0.-5"],
            &["version 1.0.5.1"],
            &["version 1..5"],
            &["version 1.0.99999999999"],
            &[],
        ] {
            assert_eq!(
                Release::parse(&list(lines)),
                Err(ReleaseError::Version),
                "{lines:?}"
            );
        }
    }

    #[test]
    fn a_file_line_that_is_not_exactly_a_name_a_size_and_a_sum_is_refused() {
        let short = &SUM[..126];
        let upper = SUM.to_uppercase();
        for line in [
            "fichier",
            "fichier a.zip",
            "fichier a.zip 12",
            "fichier a.zip 12 <somme> encore",
            "fichier a.zip douze <somme>",
            "fichier a.zip -1 <somme>",
            "fichier a.zip 1.5 <somme>",
            "fichier a.zip 9999999999999 <somme>",
            &format!("fichier a.zip 12 {short}"),
            &format!("fichier a.zip 12 {upper}"),
            &format!("fichier a.zip 12 {}zz", short),
            "fichier ../a.zip 12 <somme>",
            "fichier dossier/a.zip 12 <somme>",
            "fichier dossier\\a.zip 12 <somme>",
            "fichier .cache 12 <somme>",
            "fichier a..zip 12 <somme>",
            "fichier C:a.zip 12 <somme>",
            "fichier été.zip 12 <somme>",
        ] {
            let text = list(&["version 1.0.5", line]);
            assert_eq!(
                Release::parse(&text),
                Err(ReleaseError::File(2)),
                "{line:?}"
            );
        }
    }

    #[test]
    fn the_same_file_twice_is_refused() {
        let text = list(&[
            "version 1.0.5",
            "fichier a.zip 1 <somme>",
            "fichier A.ZIP 2 <somme>",
        ]);
        assert_eq!(Release::parse(&text), Err(ReleaseError::Twice(3)));
    }

    #[test]
    fn versions_compare_as_numbers() {
        assert!(Version(1, 0, 10) > Version(1, 0, 9));
        assert!(Version(1, 1, 0) > Version(1, 0, 99));
        assert!(Version(2, 0, 0) > Version(1, 99, 99));
        assert_eq!(Version::parse("1.0.5"), Some(Version(1, 0, 5)));
        assert_eq!(Version(1, 0, 5).to_string(), "1.0.5");
        assert!(Version::current() > Version(0, 0, 0));
    }

    // Une vraie liste et sa vraie signature (`ssh-keygen -Y sign -n mouflon`),
    // faites avec une clé d'essai dont la partie privée n'existe plus.
    const KEY: &str =
        "ssh-ed25519 AAAAC3NzaC1lZDI1NTE5AAAAIO8IgUfcdPG/3t07v2Por7d8cg0lkJGIySZftby/cG5c";
    const SIGNED_LIST: &str = "version 1.0.5
fichier Mouflon-macos-arm64.zip 3 ddaf35a193617abacc417349ae20413112e6fa4e89a97ea20a9eeee64b55d39a2192992a274fc1a836ba3c23a3feebbd454d4423643ce80e2a9ac94fa54ca49f
";
    const ITS_SIGNATURE: &str = "-----BEGIN SSH SIGNATURE-----
U1NIU0lHAAAAAQAAADMAAAALc3NoLWVkMjU1MTkAAAAg7wiBR9x08b/e3Tu/Y+ivt3xyDS
WQkYjJJl+1vL9wblwAAAAHbW91ZmxvbgAAAAAAAAAGc2hhNTEyAAAAUwAAAAtzc2gtZWQy
NTUxOQAAAECy7Zpp7yDGw4tiK0WJy09W8if6+KA90ShNJz/+/imDa9kvfGIPTPsllHDJjV
GarYEBY4Oq15jsLtSnSwQurVkA
-----END SSH SIGNATURE-----
";

    #[test]
    fn a_signed_list_is_read() {
        let release = Release::read(KEY, SIGNED_LIST.as_bytes(), ITS_SIGNATURE).unwrap();
        assert_eq!(release.version, Version(1, 0, 5));
        let file = release.file("Mouflon-macos-arm64.zip").unwrap();
        assert_eq!(file.size, 3);
        assert_eq!(file.sha512, sha512::Hash::hash(b"abc"));
    }

    #[test]
    fn nothing_is_read_from_a_list_that_is_not_signed() {
        // Une liste signée, puis retouchée : la version annoncée n'est pas lue.
        let touched = SIGNED_LIST.replace("1.0.5", "9.9.9");
        assert_eq!(
            Release::read(KEY, touched.as_bytes(), ITS_SIGNATURE),
            Err(ReleaseError::Signature(SignatureError::Mismatch))
        );
        assert_eq!(
            Release::read(KEY, SIGNED_LIST.as_bytes(), "pas une signature"),
            Err(ReleaseError::Signature(SignatureError::Unreadable))
        );
        // Signée, mais par une autre clé que celle qu'on attend.
        let other =
            "ssh-ed25519 AAAAC3NzaC1lZDI1NTE5AAAAILBM3oF6KrdJ6bu8ASYewhUcWz1ZquO51kldsc0ijpGa";
        assert_eq!(
            Release::read(other, SIGNED_LIST.as_bytes(), ITS_SIGNATURE),
            Err(ReleaseError::Signature(SignatureError::OtherKey))
        );
    }

    #[test]
    fn a_file_matches_only_with_its_size_and_its_sum() {
        let dir = std::env::temp_dir().join(format!("mouflon-somme-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join("a.zip");
        // Plus long qu'un morceau de lecture.
        let content: Vec<u8> = (0..200_000u32).map(|i| (i % 251) as u8).collect();
        std::fs::write(&path, &content).unwrap();
        let file = File {
            name: "a.zip".into(),
            size: content.len() as u64,
            sha512: sha512::Hash::hash(&content),
        };
        assert!(file.matches(&path).unwrap());

        let mut other = file.clone();
        other.sha512[10] ^= 1;
        assert!(!other.matches(&path).unwrap(), "une autre somme");
        let mut other = file.clone();
        other.size += 1;
        assert!(!other.matches(&path).unwrap(), "une autre taille");
        let mut changed = content.clone();
        changed[150_000] ^= 1;
        std::fs::write(&path, &changed).unwrap();
        assert!(!file.matches(&path).unwrap(), "un octet changé");
        assert!(file.matches(&dir.join("absent.zip")).is_err());
        // Le fichier vide, et sa somme bien connue.
        std::fs::write(&path, b"").unwrap();
        let empty = File {
            name: "a.zip".into(),
            size: 0,
            sha512: hex(SUM).unwrap(),
        };
        assert!(empty.matches(&path).unwrap());
        std::fs::remove_dir_all(&dir).unwrap();
    }
}
