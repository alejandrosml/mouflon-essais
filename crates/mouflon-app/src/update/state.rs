//! Ce que Mouflon retient d'une mise à jour à l'autre, et les dossiers où
//! elle travaille.

use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

use super::release::Version;

/// Les dossiers de la mise à jour.
#[derive(Debug, Clone)]
pub struct Dirs {
    /// Ce qui doit durer (`etat.toml`, le verrou, le relais d'une version à
    /// la suivante) : à côté des animaux de l'utilisateur.
    pub state: PathBuf,
    /// Ce qui se retrouve (le fichier téléchargé) : avec les images gardées.
    pub cache: PathBuf,
}

impl Dirs {
    fn state_file(&self) -> PathBuf {
        self.state.join("etat.toml")
    }

    /// Le verrou : une seule mise à jour à la fois sur ce compte.
    pub fn lock_file(&self) -> PathBuf {
        self.state.join("verrou")
    }

    /// Le signe de vie de la version qui vient d'arriver (voir `relay`).
    pub fn alive_file(&self) -> PathBuf {
        self.state.join("vivant")
    }

    /// Le numéro du processus de la version qui vient d'arriver.
    pub fn pid_file(&self) -> PathBuf {
        self.state.join("pid")
    }

    /// Le troupeau d'avant la mise à jour (voir [`Herd`]).
    fn herd_file(&self) -> PathBuf {
        self.state.join("troupeau.toml")
    }
}

/// La mise en scène d'une mise à jour, telle que l'écrit [`Herd`].
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub enum Staged {
    /// Le mouton annonceur.
    #[default]
    #[serde(rename = "annonceur")]
    Herald,
    #[serde(rename = "soucoupe")]
    Saucer,
}

/// Le dessin que porte un animal.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum Worn {
    #[serde(rename = "belier")]
    Ram,
    #[serde(rename = "brebis")]
    Ewe,
    #[serde(rename = "agneau")]
    Lamb,
}

/// Un animal du troupeau d'avant la mise à jour.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Animal {
    /// Son espèce : l'identifiant de son fichier.
    #[serde(rename = "espece")]
    pub species: String,
    #[serde(rename = "dessin", default, skip_serializing_if = "Option::is_none")]
    pub worn: Option<Worn>,
    /// Où il était (points, dans le repère du bureau).
    pub x: f32,
    /// Sa taille, par rapport à celle de son fichier.
    #[serde(rename = "taille", default, skip_serializing_if = "Option::is_none")]
    pub size: Option<f32>,
    /// Un agneau qui suivait sa mère : le rang de celle-ci dans la liste,
    /// le premier animal étant au rang 0 (elle revient avec lui, chacun à
    /// sa place). Une mère absente, ou qui est elle-même un agneau, ne
    /// compte pas : il revient seul.
    #[serde(rename = "mere", default, skip_serializing_if = "Option::is_none")]
    pub mother: Option<usize>,
}

/// Le troupeau tel qu'il était quand la version d'avant a laissé la place :
/// elle l'écrit en partant, la suivante le lit en arrivant et le fait
/// revenir (ou elle-même, si la suivante n'a pas pris la place). Une clé
/// d'un autre nom est ignorée : une version peut en écrire de nouvelles
/// sans gêner celle qui lit.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct Herd {
    #[serde(default)]
    pub scene: Staged,
    /// Quand il a été écrit (secondes depuis 1970).
    #[serde(rename = "quand", default)]
    pub at: u64,
    #[serde(rename = "animal", default)]
    pub animals: Vec<Animal>,
}

/// Un troupeau écrit il y a plus longtemps que cela n'est plus celui qu'on
/// attend (secondes) : la version suivante arrive en moins de trois minutes.
const HERD_KEPT: u64 = 600;
/// On ne fait pas revenir plus d'animaux que cela.
pub const HERD_MAX: usize = 400;

fn now() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map_or(0, |since| since.as_secs())
}

impl Herd {
    /// Écrit le troupeau, pour la version qui arrive. Un troupeau vide ne
    /// se note pas (elle montrera les animaux du lancement).
    pub fn save(mut self, dirs: &Dirs) {
        if self.animals.is_empty() {
            Herd::forget(dirs);
            return;
        }
        self.at = now();
        self.animals.truncate(HERD_MAX);
        let written = toml::to_string_pretty(&self)
            .map_err(std::io::Error::other)
            .and_then(|text| write_whole(&dirs.herd_file(), text.as_bytes()));
        if let Err(e) = written {
            log::warn!("mise à jour : le troupeau n'a pas pu être noté ({e})");
        }
    }

    /// Le troupeau laissé par la version d'avant, s'il y en a un et qu'il
    /// vient d'être écrit. Le fichier reste : il ne s'en va que quand cette
    /// version a bien démarré ([`Herd::forget`]), pour que celle d'avant le
    /// retrouve si elle doit revenir. `sizes` : la plus petite et la plus
    /// grande taille qu'un animal peut avoir.
    pub fn read(dirs: &Dirs, sizes: (f32, f32)) -> Option<Herd> {
        let path = dirs.herd_file();
        // (Un fichier énorme, vide ou illisible : pas de troupeau.)
        let length = std::fs::metadata(&path).ok()?.len();
        if length == 0 || length > 256 * 1024 {
            return None;
        }
        let text = std::fs::read_to_string(&path).ok()?;
        let mut herd: Herd = toml::from_str(&text).ok()?;
        let age = now().saturating_sub(herd.at);
        if herd.at == 0 || age > HERD_KEPT {
            return None;
        }
        herd.animals.truncate(HERD_MAX);
        // Sans place, un animal ne revient pas ; les rangs des mères suivent
        // ceux qu'on garde. Une mère qui n'est pas là, ou qui est elle-même
        // un agneau, ne compte pas.
        let kept: Vec<bool> = herd.animals.iter().map(|a| a.x.is_finite()).collect();
        let mut rank = Vec::with_capacity(kept.len());
        let mut next = 0;
        for keep in &kept {
            rank.push(keep.then_some(next));
            next += usize::from(*keep);
        }
        let mothers: Vec<bool> = herd
            .animals
            .iter()
            .map(|a| a.mother.is_none() && a.worn != Some(Worn::Lamb))
            .collect();
        for (i, mut animal) in std::mem::take(&mut herd.animals).into_iter().enumerate() {
            if !kept[i] {
                continue;
            }
            animal.mother = animal
                .mother
                .filter(|&m| m != i && mothers.get(m).copied().unwrap_or(false))
                .and_then(|m| rank[m]);
            animal.size = animal
                .size
                .filter(|size| size.is_finite())
                .map(|size| size.clamp(sizes.0, sizes.1));
            herd.animals.push(animal);
        }
        (!herd.animals.is_empty()).then_some(herd)
    }

    /// Oublie le troupeau noté : la version qui l'attendait a bien démarré,
    /// ou n'est jamais arrivée.
    pub fn forget(dirs: &Dirs) {
        let _ = std::fs::remove_file(dirs.herd_file());
    }
}

/// Ce que Mouflon retient.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub struct State {
    /// La version remise à plus tard : installée au prochain lancement.
    #[serde(rename = "reportee")]
    pub postponed: Option<Version>,
    /// Les versions qui n'ont pas pu s'installer ou démarrer ici : Mouflon
    /// ne les réessaie pas (il dit seulement qu'elles existent).
    #[serde(rename = "refusees")]
    pub refused: Vec<Version>,
    /// Les installations manquées, une par essai : à la deuxième de la même
    /// version, elle est refusée (voir [`State::failed`]).
    #[serde(rename = "manquees")]
    pub failed: Vec<Version>,
    /// La dernière version que Mouflon a installée lui-même.
    #[serde(rename = "installee")]
    pub installed: Option<Version>,
}

/// Combien de fois une version peut manquer son installation avant d'être
/// refusée : une fois, c'est peut-être le moment (un fichier tenu par un
/// antivirus, une machine très occupée) ; deux, c'est elle.
const TRIES: usize = 2;

impl State {
    /// L'état gardé ; rien de gardé, ou un fichier illisible : rien de retenu.
    pub fn load(dirs: &Dirs) -> State {
        std::fs::read_to_string(dirs.state_file())
            .ok()
            .and_then(|text| toml::from_str(&text).ok())
            .unwrap_or_default()
    }

    /// Garde l'état : écrit à côté, puis mis à sa place d'un seul geste.
    pub fn save(&self, dirs: &Dirs) -> std::io::Result<()> {
        let text = toml::to_string_pretty(self).map_err(std::io::Error::other)?;
        write_whole(&dirs.state_file(), text.as_bytes())
    }

    /// Change l'état gardé, et le garde (une erreur d'écriture va au journal).
    pub fn change(dirs: &Dirs, change: impl FnOnce(&mut State)) {
        let mut state = State::load(dirs);
        change(&mut state);
        if let Err(e) = state.save(dirs) {
            log::warn!("mise à jour : l'état n'a pas pu être gardé ({e})");
        }
    }

    /// Cette version ne sera pas réessayée.
    pub fn refuse(dirs: &Dirs, version: Version) {
        State::change(dirs, |state| state.refuse_now(version));
    }

    /// Cette version a manqué son installation, cette fois. À la deuxième,
    /// elle est refusée : vrai si elle l'est maintenant.
    pub fn failed(dirs: &Dirs, version: Version) -> bool {
        let mut refused = false;
        State::change(dirs, |state| {
            state.failed.push(version);
            if state.failed.iter().filter(|v| **v == version).count() >= TRIES {
                state.refuse_now(version);
            }
            // Elle n'est plus « remise à plus tard » : la recherche la
            // reprendra, si elle n'est pas refusée.
            if state.postponed == Some(version) {
                state.postponed = None;
            }
            refused = state.refused.contains(&version);
        });
        refused
    }

    /// Cette version est installée : plus rien n'est retenu contre elle ni
    /// contre celles d'avant.
    pub fn installed(dirs: &Dirs, version: Version) {
        State::change(dirs, |state| {
            state.installed = Some(version);
            state.postponed = None;
            state.failed.clear();
            state.refused.retain(|v| *v > version);
        });
    }

    fn refuse_now(&mut self, version: Version) {
        if !self.refused.contains(&version) {
            self.refused.push(version);
        }
        self.failed.retain(|v| *v != version);
        if self.postponed == Some(version) {
            self.postponed = None;
        }
    }
}

/// Écrit un fichier en entier ou pas du tout.
pub fn write_whole(path: &Path, bytes: &[u8]) -> std::io::Result<()> {
    if let Some(dir) = path.parent() {
        std::fs::create_dir_all(dir)?;
    }
    let mut name = path.file_name().unwrap_or_default().to_os_string();
    name.push(format!(".{}.partiel", std::process::id()));
    let partial = path.with_file_name(name);
    std::fs::write(&partial, bytes)?;
    std::fs::rename(&partial, path).inspect_err(|_| {
        let _ = std::fs::remove_file(&partial);
    })
}

impl Serialize for Version {
    fn serialize<S: serde::Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        serializer.collect_str(self)
    }
}

impl<'de> Deserialize<'de> for Version {
    fn deserialize<D: serde::Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        let text = String::deserialize(deserializer)?;
        Version::parse(&text).ok_or_else(|| serde::de::Error::custom("pas un numéro de version"))
    }
}

#[cfg(test)]
pub mod tests {
    use super::*;

    /// Un dossier d'essai, vide, à soi.
    pub fn scratch(name: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!("mouflon-maj-{name}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        dir
    }

    pub fn dirs(root: &Path) -> Dirs {
        Dirs {
            state: root.join("etat"),
            cache: root.join("cache"),
        }
    }

    #[test]
    fn the_state_is_kept_and_read_back() {
        let root = scratch("etat");
        let dirs = dirs(&root);
        assert_eq!(State::load(&dirs), State::default());
        State::change(&dirs, |s| s.postponed = Some(Version(1, 0, 6)));
        State::refuse(&dirs, Version(1, 0, 6));
        State::refuse(&dirs, Version(1, 0, 6));
        State::change(&dirs, |s| s.installed = Some(Version(1, 0, 7)));
        let kept = State::load(&dirs);
        // Refusée : plus remise à plus tard, et dite une seule fois.
        assert_eq!(kept.postponed, None);
        assert_eq!(kept.refused, vec![Version(1, 0, 6)]);
        assert_eq!(kept.installed, Some(Version(1, 0, 7)));
        let text = std::fs::read_to_string(dirs.state_file()).unwrap();
        assert!(text.contains("refusees = [\"1.0.6\"]"), "{text}");
        // Rien ne traîne à côté.
        let left: Vec<_> = std::fs::read_dir(&dirs.state)
            .unwrap()
            .map(|e| e.unwrap().file_name())
            .collect();
        assert_eq!(left, ["etat.toml"]);
        let _ = std::fs::remove_dir_all(root);
    }

    #[test]
    fn a_version_is_refused_the_second_time_it_fails() {
        let root = scratch("etat-manquees");
        let dirs = dirs(&root);
        let (old, new) = (Version(1, 0, 6), Version(1, 0, 7));
        State::change(&dirs, |s| s.postponed = Some(new));
        // Une fois : peut-être le moment. Elle n'est plus remise à plus tard.
        assert!(!State::failed(&dirs, new));
        let kept = State::load(&dirs);
        assert_eq!((kept.postponed, kept.failed.clone()), (None, vec![new]));
        assert!(kept.refused.is_empty());
        // Une autre version a son propre compte.
        assert!(!State::failed(&dirs, old));
        // Deux fois : refusée, et son compte s'efface.
        assert!(State::failed(&dirs, new));
        let kept = State::load(&dirs);
        assert_eq!((kept.refused, kept.failed), (vec![new], vec![old]));
        // Une version installée efface ce qui était retenu jusqu'à elle.
        State::installed(&dirs, Version(1, 0, 8));
        let kept = State::load(&dirs);
        assert_eq!(kept.installed, Some(Version(1, 0, 8)));
        assert!(kept.refused.is_empty() && kept.failed.is_empty());
        let _ = std::fs::remove_dir_all(root);
    }

    #[test]
    fn a_spoiled_state_is_no_state() {
        let root = scratch("etat-abime");
        let dirs = dirs(&root);
        std::fs::create_dir_all(&dirs.state).unwrap();
        for text in ["reportee = 3", "refusees = [\"1.0\"]", "\u{0}\u{1}"] {
            std::fs::write(dirs.state_file(), text).unwrap();
            assert_eq!(State::load(&dirs), State::default(), "{text}");
        }
        // Une clé d'une version future ne gêne pas.
        std::fs::write(dirs.state_file(), "installee = \"1.0.5\"\nplus = 1").unwrap();
        assert_eq!(State::load(&dirs).installed, Some(Version(1, 0, 5)));
        let _ = std::fs::remove_dir_all(root);
    }

    /// Les tailles d'un animal, pour ces essais.
    const SIZES: (f32, f32) = (0.5, 2.0);

    #[test]
    fn the_herd_is_written_by_one_version_and_kept_until_the_next_has_started() {
        let dir = scratch("troupeau");
        let dirs = dirs(&dir);
        assert_eq!(Herd::read(&dirs, SIZES), None);
        let herd = Herd {
            scene: Staged::Saucer,
            at: 0,
            animals: vec![
                Animal {
                    species: "texel".into(),
                    worn: Some(Worn::Ewe),
                    x: 455.5,
                    size: Some(1.25),
                    mother: None,
                },
                Animal {
                    species: "pelote".into(),
                    worn: None,
                    x: -120.0,
                    size: None,
                    mother: None,
                },
                Animal {
                    species: "texel".into(),
                    worn: Some(Worn::Lamb),
                    x: 470.0,
                    size: Some(1.25),
                    mother: Some(0),
                },
            ],
        };
        herd.clone().save(&dirs);
        let text = std::fs::read_to_string(dirs.herd_file()).unwrap();
        assert!(text.contains("scene = \"soucoupe\""), "{text}");
        assert!(text.contains("espece = \"texel\"") && text.contains("dessin = \"brebis\""));
        assert!(text.contains("mere = 0"), "{text}");
        let read = Herd::read(&dirs, SIZES).expect("le troupeau");
        assert!(read.at > 0);
        assert_eq!(read.scene, Staged::Saucer);
        assert_eq!(read.animals, herd.animals);
        // Il reste pour celle d'avant, si elle doit revenir ; il s'en va
        // quand la version a bien démarré.
        assert_eq!(Herd::read(&dirs, SIZES), Some(read));
        Herd::forget(&dirs);
        assert_eq!(Herd::read(&dirs, SIZES), None);
        // Un troupeau vide ne se note pas, et efface celui qui était noté.
        herd.save(&dirs);
        assert!(dirs.herd_file().exists());
        Herd::default().save(&dirs);
        assert!(!dirs.herd_file().exists());
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn an_old_or_odd_herd_is_not_brought_back() {
        let dir = scratch("vieux-troupeau");
        let dirs = dirs(&dir);
        std::fs::create_dir_all(&dirs.state).unwrap();
        let write = |text: &str| std::fs::write(dirs.herd_file(), text).unwrap();
        // Écrit il y a une heure.
        write(&format!(
            "scene = \"annonceur\"\nquand = {}\n[[animal]]\nespece = \"texel\"\nx = 1.0\n",
            now() - 3600
        ));
        assert_eq!(Herd::read(&dirs, SIZES), None);
        // Illisible, sans date, vide, énorme, ou sans aucun animal.
        write("ceci n'est pas un troupeau");
        assert_eq!(Herd::read(&dirs, SIZES), None);
        write("[[animal]]\nespece = \"texel\"\nx = 1.0\n");
        assert_eq!(Herd::read(&dirs, SIZES), None);
        write("");
        assert_eq!(Herd::read(&dirs, SIZES), None);
        write(&format!(
            "quand = {}\n# {}\n",
            now(),
            "x".repeat(300 * 1024)
        ));
        assert_eq!(Herd::read(&dirs, SIZES), None);
        write(&format!("quand = {}\nscene = \"soucoupe\"\n", now()));
        assert_eq!(Herd::read(&dirs, SIZES), None);
        // Des clés qu'on ne connaît pas, une mise en scène à venir : lu
        // quand même, sans ce qui ne se lit pas.
        write(&format!(
            "quand = {}\nnouveau = 3\n[[animal]]\nespece = \"texel\"\nx = 12.0\ncouleur = \"bleu\"\n\
             [[animal]]\nespece = \"racka\"\nx = nan\n",
            now()
        ));
        let herd = Herd::read(&dirs, SIZES).expect("lu");
        assert_eq!(herd.scene, Staged::Herald);
        assert_eq!(herd.animals.len(), 1);
        assert_eq!(herd.animals[0].species, "texel");
        // Une taille hors de ce qu'un animal peut avoir est ramenée dans
        // les bornes ; une taille qui n'en est pas une est oubliée.
        write(&format!(
            "quand = {}\n[[animal]]\nespece = \"texel\"\nx = 1.0\ntaille = 4000.0\n\
             [[animal]]\nespece = \"texel\"\nx = 2.0\ntaille = -3.0\n\
             [[animal]]\nespece = \"texel\"\nx = 3.0\ntaille = inf\n",
            now()
        ));
        let sizes: Vec<Option<f32>> = Herd::read(&dirs, SIZES)
            .expect("lu")
            .animals
            .iter()
            .map(|a| a.size)
            .collect();
        assert_eq!(sizes, [Some(SIZES.1), Some(SIZES.0), None]);
        Herd::forget(&dirs);
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn a_lamb_keeps_its_mother_only_if_she_comes_back_too() {
        let dir = scratch("agneaux");
        let dirs = dirs(&dir);
        std::fs::create_dir_all(&dirs.state).unwrap();
        // 0 : une brebis ; 1 : son agneau ; 2 : un agneau dont la « mère »
        // est un agneau ; 3 : un agneau dont la mère n'a pas de place ; 4 :
        // elle ; 5 : un agneau qui se désigne lui-même ; 6 : un rang hors de
        // la liste.
        std::fs::write(
            dirs.herd_file(),
            format!(
                "quand = {}\n\
                 [[animal]]\nespece = \"texel\"\nx = 100.0\n\
                 [[animal]]\nespece = \"texel\"\ndessin = \"agneau\"\nx = 140.0\nmere = 0\n\
                 [[animal]]\nespece = \"texel\"\ndessin = \"agneau\"\nx = 160.0\nmere = 1\n\
                 [[animal]]\nespece = \"texel\"\ndessin = \"agneau\"\nx = 180.0\nmere = 4\n\
                 [[animal]]\nespece = \"texel\"\nx = nan\n\
                 [[animal]]\nespece = \"texel\"\ndessin = \"agneau\"\nx = 200.0\nmere = 5\n\
                 [[animal]]\nespece = \"texel\"\ndessin = \"agneau\"\nx = 220.0\nmere = 99\n",
                now()
            ),
        )
        .unwrap();
        let herd = Herd::read(&dirs, SIZES).expect("lu");
        let mothers: Vec<Option<usize>> = herd.animals.iter().map(|a| a.mother).collect();
        // (Celui qui n'a pas de place est retiré : les rangs suivent.)
        assert_eq!(mothers, [None, Some(0), None, None, None, None]);
        Herd::forget(&dirs);
        let _ = std::fs::remove_dir_all(&dir);
    }
}
