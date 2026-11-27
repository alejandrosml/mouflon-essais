//! Bibliothèque d'animaux : l'animal intégré (le Bleu du Maine), les animaux
//! livrés dans l'app (`Mouflon.app/Contents/Resources/pets`) et ceux du
//! dossier de l'utilisateur.
//!
//! Dans le dossier `pets/` : un sous-dossier par animal au format Mouflon
//! (`pet.toml` + dessin), un dossier desktopPet (`animations.xml`), ou
//! directement des fichiers `.xml` desktopPet. Le dossier est surveillé : un
//! animal qu'on y dépose apparaît dans le menu sans relancer l'app.

use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex, PoisonError, TryLockError};
use std::time::{Instant, SystemTime};

use mouflon_core::{LoadError, PetDef, load_pet, parse_pet_toml};
use mouflon_render::PetArt;
use mouflon_render::cache::Store;

/// Mascottes de Mouflon, par ordre de préférence au premier lancement
/// (chacune dans sa famille : Poete est une mascotte numérique, les deux
/// autres des moutons).
const MASCOTS: [&str; 3] = ["poete", "merinos-d-arles", "southdown"];

/// Le choix du lancement qui mêle les familles : « Un peu de tout », sans
/// les mascottes numériques (voir [`Library::launch_pets`]).
pub const ANY_FAMILY: &str = "any";

/// La famille qui arrive au lancement tant qu'on n'en a pas choisi d'autre.
pub const FIRST_FAMILY: &str = "sheep";

/// Le rang, dans la liste `listed` que montre la fenêtre des réglages (voir
/// [`Library::launch_choices`]), de ce que vaut le choix du lancement (voir
/// [`Library::launch_here`]) ; rien si elle n'a pas de liste, ou si ce
/// choix n'y est pas (la bibliothèque a changé depuis qu'elle l'a montrée).
#[must_use]
pub fn launch_rank(listed: &[String], here: &str) -> Option<usize> {
    listed.iter().position(|choice| choice == here)
}

#[derive(Clone, Debug)]
pub struct Entry {
    /// L'animal à la taille de cette entrée.
    pub def: Arc<PetDef>,
    /// Cette taille, par rapport à celle prévue par son fichier.
    pub size: f32,
    /// L'animal à la taille prévue par son fichier.
    base: Arc<PetDef>,
    /// Base adulte dont ce jeune est issu, pour partager son dessin entre frères.
    lamb_source: Option<Arc<PetDef>>,
    /// Dessin préparé à la première apparition : décoder des centaines
    /// d'images prend un moment, et elles n'occupent la mémoire que si
    /// l'animal sert.
    art: Arc<ArtSlot>,
    /// Images déjà prêtes (voir [`mouflon_render::cache`]).
    images: Option<Store>,
}

/// Le dessin d'une espèce : préparé à la première demande (les autres
/// demandes attendent), puis remplacé par sa version aux images compressées.
#[derive(Debug, Default)]
struct ArtSlot {
    /// `None` : pas encore préparé ; `Some(None)` : illisible.
    art: Mutex<Option<Option<Arc<PetArt>>>>,
    /// La compression des images a été lancée (ou n'avait pas lieu d'être).
    compressing: AtomicBool,
}

/// Ce qu'a donné la compression des images d'une espèce.
#[derive(Debug)]
pub struct Compressed {
    /// Octets des images avant et après.
    pub before: usize,
    pub after: usize,
    pub took: std::time::Duration,
}

impl Entry {
    fn new(def: PetDef, size: f32, images: Option<Store>) -> Self {
        Self::with_base(Arc::new(def), size, images)
    }

    fn with_base(base: Arc<PetDef>, size: f32, images: Option<Store>) -> Self {
        Self {
            def: Arc::new(base.scaled(size)),
            size,
            base,
            lamb_source: None,
            art: Arc::default(),
            images,
        }
    }

    /// Met l'animal à une autre taille ; son dessin sera refait à la demande.
    fn resize(&mut self, size: f32) {
        self.def = Arc::new(self.base.scaled(size));
        self.size = size;
        self.art = Arc::default();
    }

    /// La même espèce à une autre taille, avec son propre dessin (préparé à
    /// la demande).
    pub(crate) fn at_size(&self, size: f32) -> Self {
        Self {
            def: Arc::new(self.base.scaled(size)),
            size,
            base: Arc::clone(&self.base),
            lamb_source: self.lamb_source.clone(),
            art: Arc::default(),
            images: self.images.clone(),
        }
    }

    /// Un jeune de cette espèce, à la taille d'affichage de sa mère. La
    /// réduction anatomique est dans sa base : les changements de taille ne
    /// la réappliquent pas et ne lui font pas perdre son âge.
    pub(crate) fn as_lamb(&self, weeks: f32) -> Option<Self> {
        let mut young = Self::new(self.base.as_lamb(weeks)?, self.size, self.images.clone());
        young.lamb_source = Some(Arc::clone(&self.base));
        Some(young)
    }

    /// L'âge de son agneau, en semaines : celui de son dessin d'agneau.
    pub(crate) fn lamb_weeks(&self) -> f32 {
        self.base.lamb_weeks()
    }

    pub(crate) fn is_lamb_of(&self, parent: &Self, weeks: f32) -> bool {
        self.lamb_source
            .as_ref()
            .is_some_and(|base| Arc::ptr_eq(base, &parent.base))
            && self.def.age == mouflon_core::Age::Lamb { weeks }
            && (self.size - parent.size).abs() < 1e-3
    }

    pub(crate) fn same_base_at_size(&self, other: &Self, size: f32) -> bool {
        Arc::ptr_eq(&self.base, &other.base) && (self.size - size).abs() < 1e-3
    }

    /// Est-ce l'entrée de cette espèce à cette taille ?
    fn is(&self, id: &str, size: f32) -> bool {
        self.def.meta.id == id && (self.size - size).abs() < 1e-3
    }

    /// Le dessin de l'animal a-t-il déjà été préparé (ou trouvé illisible) ?
    /// Non tant qu'il se prépare.
    #[must_use]
    pub fn art_ready(&self) -> bool {
        match self.art.art.try_lock() {
            Ok(art) => art.is_some(),
            Err(TryLockError::Poisoned(art)) => art.into_inner().is_some(),
            Err(TryLockError::WouldBlock) => false,
        }
    }

    /// Dessin de l'animal, préparé à la première demande (en reprenant les
    /// images déjà prêtes s'il y en a) ; `None` s'il est illisible (l'erreur
    /// est notée au journal une fois).
    pub fn art(&self) -> Option<Arc<PetArt>> {
        let mut art = self.art.art.lock().unwrap_or_else(PoisonError::into_inner);
        art.get_or_insert_with(|| {
            let started = Instant::now();
            match PetArt::load_cached(&self.def, self.images.as_ref()) {
                Ok(art) => {
                    log::info!(
                        "dessin de {} prêt en {} ms{}",
                        self.def.meta.name,
                        started.elapsed().as_millis(),
                        if art.from_cache() {
                            format!(
                                " (images déjà prêtes, {:.1} Mo)",
                                art.image_bytes() as f64 / 1e6
                            )
                        } else {
                            String::new()
                        }
                    );
                    Some(Arc::new(art))
                }
                Err(e) => {
                    log::error!("{} : dessin invalide : {e}", self.def.meta.name);
                    None
                }
            }
        })
        .clone()
    }

    /// Le dessin pour un aperçu (la galerie) : celui de l'animal s'il est
    /// déjà prêt, sinon un dessin préparé pour l'occasion puis oublié. La
    /// galerie montre toutes les espèces : garder leurs dessins à toutes
    /// occuperait la mémoire pour rien.
    pub fn art_for_preview(&self) -> Option<Arc<PetArt>> {
        if let Some(ready) = &*self.art.art.lock().unwrap_or_else(PoisonError::into_inner) {
            return ready.clone();
        }
        match PetArt::load_cached(&self.def, self.images.as_ref()) {
            Ok(art) => Some(Arc::new(art)),
            Err(e) => {
                log::error!("{} : dessin invalide : {e}", self.def.meta.name);
                None
            }
        }
    }

    /// Compresse les images du dessin, une seule fois, sur `threads` fils
    /// qui appellent d'abord `setup` ; elles sont gardées sur le disque pour
    /// les prochains lancements, et le dessin compressé remplace l'autre
    /// (relu du disque : ses pages ne comptent alors plus dans l'empreinte
    /// de l'app). `None` s'il n'y avait rien à compresser (ou si c'est déjà
    /// fait).
    pub fn compress(&self, threads: usize, setup: &(dyn Fn() + Sync)) -> Option<Compressed> {
        if self.art.compressing.swap(true, Ordering::AcqRel) {
            return None;
        }
        let art = self.art()?;
        let started = Instant::now();
        let mut packed = Arc::new(art.compressed(threads, setup)?);
        if let Some(store) = &self.images {
            match packed.keep(&self.def, store) {
                Ok(()) => {
                    // Les mêmes images, relues du fichier tout juste écrit.
                    if let Ok(kept) = PetArt::load_cached(&self.def, Some(store))
                        && kept.from_cache()
                        && kept.image_bytes() == packed.image_bytes()
                    {
                        packed = Arc::new(kept);
                    }
                }
                Err(e) => log::warn!(
                    "images de {} non gardées dans {} : {e}",
                    self.def.meta.name,
                    store.dir.display()
                ),
            }
        }
        *self.art.art.lock().unwrap_or_else(PoisonError::into_inner) =
            Some(Some(Arc::clone(&packed)));
        Some(Compressed {
            before: art.image_bytes(),
            after: packed.image_bytes(),
            took: started.elapsed(),
        })
    }

    /// Reste-t-il des images à compresser ? (Sans attendre un dessin en
    /// préparation.)
    #[must_use]
    pub fn compressible(&self) -> bool {
        if self.art.compressing.load(Ordering::Acquire) {
            return false;
        }
        match self.art.art.try_lock() {
            Ok(art) => art
                .as_ref()
                .is_some_and(|a| a.as_ref().is_some_and(|a| a.compressible())),
            Err(_) => false,
        }
    }
}

#[derive(Clone, Debug, Default)]
pub struct Library {
    /// Les espèces, à la taille de tous les animaux.
    pub pets: Vec<Entry>,
    /// Les mêmes à d'autres tailles, choisies pour un animal depuis son
    /// menu (oubliées quand plus personne ne s'en sert).
    resized: Vec<Entry>,
    /// Dossier des animaux de l'utilisateur.
    dir: Option<PathBuf>,
    /// Images déjà prêtes.
    images: Option<Store>,
    /// Animaux livrés dans l'app, dans l'ordre : le premier est l'animal par défaut.
    bundled: Vec<String>,
    /// L'animal intégré et ceux livrés dans l'app, lus au premier chargement.
    shipped: Option<Shipped>,
    /// Animaux passés en argument, rechargés avec le reste.
    extra: Vec<PathBuf>,
    /// Taille des animaux, par rapport à celle prévue par chacun.
    size: f32,
    /// État du dossier au dernier chargement.
    stamp: Stamp,
}

/// L'animal intégré et ceux livrés dans l'app : ils ne changent pas pendant
/// que l'app tourne, on ne les lit qu'une fois.
#[derive(Clone, Debug, Default)]
struct Shipped {
    /// L'animal intégré au programme.
    builtin: Option<Arc<PetDef>>,
    /// Ceux de l'app, chacun avec le fichier d'où il vient.
    bundled: Vec<(Arc<PetDef>, String)>,
}

impl Shipped {
    fn read() -> Self {
        let builtin = match builtin_pet() {
            Ok(def) => Some(Arc::new(def)),
            Err(e) => {
                log::error!("animal intégré invalide : {e}");
                None
            }
        };
        let bundled = bundled_dirs()
            .iter()
            .flat_map(|dir| read_pets(dir))
            .map(|(def, origin)| (Arc::new(def), origin))
            .collect();
        Self { builtin, bundled }
    }
}

/// Les animaux d'un dossier, dans l'ordre de leurs noms, chacun avec le
/// fichier d'où il vient ; ceux qui ne se lisent pas sont notés au journal.
fn read_pets(dir: &Path) -> Vec<(PetDef, String)> {
    let Ok(entries) = std::fs::read_dir(dir) else {
        return Vec::new();
    };
    let mut paths: Vec<_> = entries.filter_map(Result::ok).map(|e| e.path()).collect();
    paths.sort();
    let mut pets = Vec::new();
    for path in paths {
        let is_pet = path.is_dir()
            || path
                .extension()
                .is_some_and(|e| e.eq_ignore_ascii_case("xml"));
        let file = pet_file(&path);
        if !is_pet || !file.is_file() {
            continue;
        }
        match load_pet(&file) {
            Ok(def) => pets.push((def, file.display().to_string())),
            Err(e) => log::warn!("{} : {e}", file.display()),
        }
    }
    pets
}

/// Noms et dates de modification du dossier des animaux et de ses sous-dossiers.
pub type Stamp = Vec<(PathBuf, Option<SystemTime>)>;

pub fn stamp(dir: &Path) -> Stamp {
    let mut out = Vec::new();
    let mut visit = |path: &Path| {
        out.push((
            path.to_path_buf(),
            std::fs::metadata(path).and_then(|m| m.modified()).ok(),
        ));
    };
    visit(dir);
    if let Ok(entries) = std::fs::read_dir(dir) {
        let mut paths: Vec<PathBuf> = entries.filter_map(Result::ok).map(|e| e.path()).collect();
        paths.sort();
        for path in paths {
            visit(&path);
            if path.is_dir()
                && let Ok(inner) = std::fs::read_dir(&path)
            {
                let mut files: Vec<PathBuf> =
                    inner.filter_map(Result::ok).map(|e| e.path()).collect();
                files.sort();
                for file in files {
                    visit(&file);
                }
            }
        }
    }
    out
}

/// Animaux livrés avec Mouflon : ceux de l'app (`Mouflon.app/Contents/Resources/pets`
/// sur Mac, le dossier `pets` à côté du programme ailleurs) ou, lancé depuis
/// le dépôt (`cargo run`), ceux de `pets/` et du dossier `animaux/`.
fn bundled_dirs() -> Vec<PathBuf> {
    let Some(exe) = std::env::current_exe()
        .ok()
        .and_then(|exe| exe.canonicalize().ok())
    else {
        return Vec::new();
    };
    if let Some(contents) = exe.parent().and_then(Path::parent) {
        let dir = contents.join("Resources").join("pets");
        if dir.is_dir() {
            return vec![dir];
        }
    }
    if cfg!(test) {
        return Vec::new();
    }
    // À côté du programme (Windows et Linux : `Mouflon/mouflon` et
    // `Mouflon/pets`), ou installé à la façon d'Unix (`bin/mouflon` et
    // `share/mouflon/pets`).
    if let Some(here) = exe.parent() {
        for dir in [here.join("pets"), here.join("../share/mouflon/pets")] {
            if dir.is_dir() {
                return vec![dir];
            }
        }
    }
    // …/target/[cible/]profil/mouflon : le dépôt est quelques dossiers plus haut.
    exe.ancestors()
        .take(6)
        .find(|dir| dir.join("Cargo.toml").is_file() && dir.join("pets").is_dir())
        .map(|repo| {
            ["pets", "animaux"]
                .into_iter()
                .map(|name| repo.join(name))
                .filter(|dir| dir.is_dir())
                .collect()
        })
        .unwrap_or_default()
}

/// Fichier de description d'un animal : `pet.toml`, ou à défaut `animations.xml`.
fn pet_file(path: &Path) -> PathBuf {
    if path.is_dir() {
        let toml = path.join("pet.toml");
        if toml.is_file() {
            toml
        } else {
            path.join("animations.xml")
        }
    } else {
        path.to_path_buf()
    }
}

/// Copie un dossier et tout son contenu.
pub fn copy_dir(from: &Path, to: &Path) -> std::io::Result<()> {
    std::fs::create_dir_all(to)?;
    for entry in std::fs::read_dir(from)? {
        let entry = entry?;
        let target = to.join(entry.file_name());
        if entry.file_type()?.is_dir() {
            copy_dir(&entry.path(), &target)?;
        } else {
            std::fs::copy(entry.path(), target)?;
        }
    }
    Ok(())
}

/// L'animal intégré à l'app : le Bleu du Maine (`pets/bleu-du-maine`), sa
/// brebis, son bélier et son agneau, l'exemple complet du format. Il est là
/// même sans aucun autre animal.
pub(crate) fn builtin_pet() -> Result<PetDef, LoadError> {
    const TOML: &str = include_str!("../../../pets/bleu-du-maine/pet.toml");
    /// Ses dessins et ses sons, dans le programme.
    const FILES: &[(&str, &[u8])] = &[
        (
            "bleu-du-maine.svg",
            include_bytes!("../../../pets/bleu-du-maine/bleu-du-maine.svg"),
        ),
        (
            "bleu-du-maine-belier.svg",
            include_bytes!("../../../pets/bleu-du-maine/bleu-du-maine-belier.svg"),
        ),
        (
            "bleu-du-maine-agneau.svg",
            include_bytes!("../../../pets/bleu-du-maine/bleu-du-maine-agneau.svg"),
        ),
        (
            "sons/agneau-1.mp3",
            include_bytes!("../../../pets/bleu-du-maine/sons/agneau-1.mp3"),
        ),
        (
            "sons/mouton-1.mp3",
            include_bytes!("../../../pets/bleu-du-maine/sons/mouton-1.mp3"),
        ),
        (
            "sons/mouton-2.mp3",
            include_bytes!("../../../pets/bleu-du-maine/sons/mouton-2.mp3"),
        ),
        (
            "sons/mouton-3.mp3",
            include_bytes!("../../../pets/bleu-du-maine/sons/mouton-3.mp3"),
        ),
        (
            "sons/mouton-4.mp3",
            include_bytes!("../../../pets/bleu-du-maine/sons/mouton-4.mp3"),
        ),
        (
            "sons/broute-1.mp3",
            include_bytes!("../../../pets/bleu-du-maine/sons/broute-1.mp3"),
        ),
        (
            "sons/broute-2.mp3",
            include_bytes!("../../../pets/bleu-du-maine/sons/broute-2.mp3"),
        ),
        (
            "sons/pose-1.mp3",
            include_bytes!("../../../pets/bleu-du-maine/sons/pose-1.mp3"),
        ),
        (
            "sons/pose-2.mp3",
            include_bytes!("../../../pets/bleu-du-maine/sons/pose-2.mp3"),
        ),
    ];
    parse_pet_toml(TOML, |name: &str| {
        FILES
            .iter()
            .find(|(file, _)| *file == name)
            .map(|(_, bytes)| bytes.to_vec())
            .ok_or_else(|| LoadError::Format(format!("ressource intégrée inconnue : {name}")))
    })
}

impl Library {
    #[must_use]
    pub fn load(user_dir: Option<&Path>, images: Option<Store>, size: f32) -> Self {
        let mut library = Self {
            dir: user_dir.map(Path::to_path_buf),
            images,
            size,
            ..Self::default()
        };
        library.reload();
        library
    }

    /// Met tous les animaux à une autre taille (1 : celle prévue par chacun).
    pub fn set_size(&mut self, size: f32) {
        self.size = size;
        for entry in &mut self.pets {
            entry.resize(size);
        }
        // Tous les animaux prennent cette taille : les autres ne servent plus.
        self.resized.clear();
    }

    /// Taille de tous les animaux.
    #[must_use]
    pub fn size(&self) -> f32 {
        self.size
    }

    /// L'animal de la bibliothèque dont `def` est une version (même
    /// identifiant), à la taille de tous les animaux.
    #[must_use]
    pub fn current(&self, def: &PetDef) -> Option<&Entry> {
        self.pets.iter().find(|e| e.def.meta.id == def.meta.id)
    }

    /// L'espèce `index` à cette taille : l'entrée de tous les animaux si
    /// c'est la leur, sinon une autre, créée au besoin (son dessin se
    /// prépare à la demande).
    pub fn at_size(&mut self, index: usize, size: f32) -> Option<Entry> {
        let entry = self.pets.get(index)?;
        if entry.is(&entry.def.meta.id, size) {
            return Some(entry.clone());
        }
        if let Some(found) = self.resized.iter().find(|e| e.is(&entry.def.meta.id, size)) {
            return Some(found.clone());
        }
        let made = entry.at_size(size);
        self.resized.push(made.clone());
        Some(made)
    }

    /// L'entrée (à n'importe quelle taille) d'où vient cette définition.
    #[must_use]
    pub fn entry_of(&self, def: &Arc<PetDef>) -> Option<&Entry> {
        self.pets
            .iter()
            .chain(&self.resized)
            .find(|e| Arc::ptr_eq(&e.def, def))
    }

    /// Les entrées de cette espèce, à toutes les tailles.
    pub fn sizes_of<'a>(&'a self, id: &'a str) -> impl Iterator<Item = &'a Entry> + 'a {
        self.pets
            .iter()
            .chain(&self.resized)
            .filter(move |e| e.def.meta.id == id)
    }

    /// Oublie les autres tailles dont plus aucun animal ne se sert (et
    /// leurs dessins).
    pub fn forget_unused_sizes(&mut self, used: impl Fn(&Arc<PetDef>) -> bool) {
        self.resized.retain(|e| used(&e.def));
    }

    /// Recharge tout. Le dossier de l'utilisateur et les animaux passés en
    /// argument sont relus ; l'animal intégré et ceux livrés dans l'app, lus
    /// une seule fois, gardent leur dessin s'il est déjà prêt. Aucun dessin
    /// ne se prépare ici : chacun attend la première apparition de son
    /// animal (voir [`Entry::art`]).
    pub fn reload(&mut self) {
        let previous = std::mem::take(&mut self.pets);
        self.resized.clear();
        let shipped = self.shipped.get_or_insert_with(Shipped::read).clone();
        if let Some(def) = shipped.builtin {
            self.keep(def, "animal intégré", &previous);
        }
        let before: Vec<String> = self.pets.iter().map(|e| e.def.meta.id.clone()).collect();
        for (def, origin) in shipped.bundled {
            self.keep(def, &origin, &previous);
        }
        self.bundled = self
            .pets
            .iter()
            .map(|e| e.def.meta.id.clone())
            .filter(|id| !before.contains(id))
            .collect();
        if let Some(dir) = self.dir.clone() {
            for (def, origin) in read_pets(&dir) {
                self.add(def, &origin);
            }
            self.stamp = stamp(&dir);
        }
        for path in self.extra.clone() {
            self.load_file(&pet_file(&path));
        }
    }

    /// Dossier des animaux de l'utilisateur (surveillé pendant que l'app tourne).
    #[must_use]
    pub fn dir(&self) -> Option<&Path> {
        self.dir.as_deref()
    }

    /// Le dossier des animaux a-t-il changé depuis le dernier chargement,
    /// d'après ce relevé ?
    #[must_use]
    pub fn changed_since(&self, stamp: &Stamp) -> bool {
        self.dir.is_some() && *stamp != self.stamp
    }

    fn add(&mut self, def: PetDef, origin: &str) {
        self.insert(Entry::new(def, self.size, self.images.clone()), origin);
    }

    /// Reprend l'entrée qu'avait cet animal avant le rechargement (avec son
    /// dessin, s'il est prêt), ou lui en fait une.
    fn keep(&mut self, def: Arc<PetDef>, origin: &str, previous: &[Entry]) {
        let entry = previous
            .iter()
            .find(|e| Arc::ptr_eq(&e.base, &def))
            .cloned()
            .unwrap_or_else(|| Entry::with_base(def, self.size, self.images.clone()));
        self.insert(entry, origin);
    }

    /// Range un animal dans la bibliothèque, à la place de celui de même
    /// identifiant. Son dessin ne se lit qu'à sa première apparition, sur un
    /// fil à part : lire et cuire ceux de toutes les espèces au lancement
    /// prendrait des secondes. Un dessin illisible se signale alors au journal.
    fn insert(&mut self, entry: Entry, origin: &str) {
        log::info!("animal chargé : {} ({origin})", entry.def.meta.name);
        self.pets.retain(|e| e.def.meta.id != entry.def.meta.id);
        self.pets.push(entry);
    }

    fn load_file(&mut self, file: &Path) -> Option<String> {
        match load_pet(file) {
            Ok(def) => {
                let id = def.meta.id.clone();
                self.add(def, &file.display().to_string());
                self.pets.iter().any(|e| e.def.meta.id == id).then_some(id)
            }
            Err(e) => {
                log::error!("{} : {e}", file.display());
                None
            }
        }
    }

    /// Charge un animal désigné à la main (fichier `pet.toml`, fichier XML
    /// desktopPet ou dossier qui en contient un) ; renvoie son identifiant.
    pub fn load_path(&mut self, path: &Path) -> Option<String> {
        self.extra.push(path.to_path_buf());
        self.load_file(&pet_file(path))
    }

    /// Installe un animal (fichier ou dossier déposé sur un animal) dans le
    /// dossier de l'utilisateur, puis recharge ; renvoie son identifiant.
    pub fn install(&mut self, source: &Path) -> Result<String, String> {
        let dir = self
            .dir
            .clone()
            .ok_or("pas de dossier des animaux sur ce système")?;
        let file = pet_file(source);
        // Vérifier avant de copier : on n'installe que ce qui se charge.
        let def = load_pet(&file).map_err(|e| e.to_string())?;
        let name = |p: &Path| {
            p.file_name()
                .map(|n| n.to_os_string())
                .ok_or("nom de fichier invalide")
        };
        let generic = file
            .file_name()
            .is_some_and(|n| n == "animations.xml" || n == "pet.toml");
        let folder = if source.is_dir() {
            Some(source.to_path_buf())
        } else if generic {
            // `animations.xml` ou `pet.toml` seul : on prend son dossier (et ses dessins).
            source.parent().map(Path::to_path_buf)
        } else {
            None
        };
        // Un fichier seul doit se relire seul, là où on le pose : un fichier
        // desktopPet (`.xml`) porte ses images, mais un `pet.toml` sous un
        // autre nom serait copié sans ses dessins et jamais relu (voir
        // `read_pets`). On ne copie pas ce qui ne s'installe pas.
        let alone = file
            .extension()
            .is_some_and(|e| e.eq_ignore_ascii_case("xml"));
        if folder.is_none() && !alone {
            return Err(
                "un animal Mouflon s'installe par son dossier ou par son fichier pet.toml"
                    .to_owned(),
            );
        }
        std::fs::create_dir_all(&dir).map_err(|e| e.to_string())?;
        match folder {
            Some(folder) => {
                copy_dir(&folder, &dir.join(name(&folder)?)).map_err(|e| e.to_string())?;
            }
            None => {
                std::fs::copy(source, dir.join(name(source)?)).map_err(|e| e.to_string())?;
            }
        }
        self.reload();
        Ok(def.meta.id)
    }

    /// Le rang de l'espèce `id` dans la bibliothèque, si elle y est.
    #[must_use]
    pub fn position_of(&self, id: &str) -> Option<usize> {
        self.pets.iter().position(|e| e.def.meta.id == id)
    }

    /// Le rang d'une espèce que l'essai sait être là.
    #[cfg(test)]
    fn index_of(&self, id: &str) -> usize {
        self.position_of(id)
            .expect("espèce absente de la bibliothèque")
    }

    /// Le rang d'aujourd'hui de l'espèce qui avait le rang `rank` dans la
    /// liste `listed` (les identifiants des espèces, relevés quand une
    /// fenêtre les a montrées : la galerie) ; rien si elle n'est plus là. Le
    /// dossier des animaux a pu changer depuis : un rang gardé tel quel
    /// désignerait une autre espèce.
    #[must_use]
    pub fn listed_index(&self, listed: &[String], rank: usize) -> Option<usize> {
        let id = listed.get(rank)?;
        self.pets.iter().position(|e| &e.def.meta.id == id)
    }

    /// Les familles qui ont des animaux ici, rangées comme dans le menu.
    fn families(&self) -> Vec<String> {
        let categories: Vec<&str> = self.pets.iter().map(|e| e.def.meta.category()).collect();
        crate::menu::groups(&categories)
            .into_iter()
            .map(|(family, _)| family)
            .collect()
    }

    /// Les choix du lancement, dans l'ordre : chaque famille qui a des
    /// animaux ici (rangées comme dans le menu), puis [`ANY_FAMILY`] (« Un
    /// peu de tout ») s'il y a au moins deux familles hors des mascottes
    /// numériques. (Une famille qu'un `pet.toml` appellerait `any` ne se
    /// choisit pas seule : ce mot est celui du mélange, dont elle fait
    /// partie.)
    #[must_use]
    pub fn launch_choices(&self) -> Vec<String> {
        let families = self.families();
        let real = families
            .iter()
            .filter(|family| family.as_str() != mouflon_core::MASCOTS)
            .count();
        let mut choices: Vec<String> = families
            .into_iter()
            .filter(|family| family != ANY_FAMILY)
            .collect();
        if real >= 2 {
            choices.push(ANY_FAMILY.to_owned());
        }
        choices
    }

    /// Ce que vaut ici le choix `launch` : lui-même si c'est une famille qui
    /// a des animaux ici. Sinon (« un peu de tout », une famille qui n'a pas
    /// ou plus d'animal ici, un mot inconnu) : [`ANY_FAMILY`] s'il y a au
    /// moins deux familles de vrais animaux, la seule qu'il y ait sinon, et
    /// les mascottes numériques s'il n'y a qu'elles.
    #[must_use]
    pub fn launch_here(&self, launch: &str) -> String {
        let families = self.families();
        if launch != ANY_FAMILY && families.iter().any(|family| family == launch) {
            return launch.to_owned();
        }
        let mut real = families
            .iter()
            .filter(|family| family.as_str() != mouflon_core::MASCOTS);
        match (real.next(), real.next()) {
            (Some(_), Some(_)) => ANY_FAMILY.to_owned(),
            (Some(only), None) => only.clone(),
            _ => mouflon_core::MASCOTS.to_owned(),
        }
    }

    /// Les espèces qui arrivent au lancement, et parmi lesquelles « Ajouter
    /// dix animaux au hasard » choisit, par leur rang : celles de la famille
    /// que vaut `launch` ici (voir [`Library::launch_here`]) ; pour
    /// [`ANY_FAMILY`], toutes sauf les mascottes numériques.
    #[must_use]
    pub fn launch_pets(&self, launch: &str) -> Vec<usize> {
        let here = self.launch_here(launch);
        (0..self.pets.len())
            .filter(|&i| {
                let category = self.pets[i].def.meta.category();
                if here == ANY_FAMILY {
                    category != mouflon_core::MASCOTS
                } else {
                    category == here
                }
            })
            .collect()
    }

    /// Le choix du lancement de qui n'a encore rien choisi (des réglages
    /// d'avant ce choix, ou un premier lancement) : la famille du préféré
    /// s'il est ici et que c'est un vrai animal (qui avait un chat pour
    /// préféré garde ses chats), sinon les moutons. Une mascotte numérique
    /// préférée ne compte pas : elles n'arrivent que si on les choisit.
    #[must_use]
    pub fn first_launch(&self, preferred: Option<&str>) -> String {
        preferred
            .and_then(|id| self.pets.iter().find(|e| e.def.meta.id == id))
            .map(|entry| entry.def.meta.category())
            .filter(|family| *family != mouflon_core::MASCOTS && *family != ANY_FAMILY)
            .unwrap_or(FIRST_FAMILY)
            .to_owned()
    }

    /// `count` espèces tirées au hasard parmi celles du lancement (voir
    /// [`Library::launch_pets`]) : jamais une mascotte numérique, à moins
    /// de les avoir choisies.
    #[must_use]
    pub fn random_pets(
        &self,
        launch: &str,
        count: usize,
        rng: &mut mouflon_core::math::Rng,
    ) -> Vec<usize> {
        let among = self.launch_pets(launch);
        if among.is_empty() {
            return Vec::new();
        }
        (0..count)
            .map(|_| among[rng.next_u32() as usize % among.len()])
            .collect()
    }

    /// L'animal du premier article du menu : celui qu'on essaie (`tried`,
    /// donné sur la ligne de commande), quelle que soit sa famille ; sinon
    /// celui du lancement (voir [`Library::default_index`]).
    #[must_use]
    pub fn favorite(&self, preferred: Option<&str>, launch: &str, tried: Option<&str>) -> usize {
        tried
            .and_then(|id| self.pets.iter().position(|e| e.def.meta.id == id))
            .unwrap_or_else(|| self.default_index(preferred, launch))
    }

    /// Ceux qui arrivent au lancement, `count` animaux : celui qu'on essaie,
    /// quelle que soit sa famille ; sinon, pour « un peu de tout », chacun
    /// tiré au hasard ; sinon l'animal du lancement.
    #[must_use]
    pub fn starters(
        &self,
        preferred: Option<&str>,
        launch: &str,
        tried: Option<&str>,
        count: usize,
        rng: &mut mouflon_core::math::Rng,
    ) -> Vec<usize> {
        if tried.is_none() && self.launch_here(launch) == ANY_FAMILY {
            return self.random_pets(launch, count, rng);
        }
        vec![self.favorite(preferred, launch, tried); count]
    }

    /// Cet animal peut-il devenir le préféré ? S'il est de ceux du
    /// lancement : une mascotte numérique ajoutée au passage ne prend pas
    /// la place d'un mouton.
    #[must_use]
    pub fn can_be_favorite(&self, index: usize, launch: &str) -> bool {
        self.launch_pets(launch).contains(&index)
    }

    /// L'animal du lancement et du haut du menu, parmi ceux du choix
    /// `launch` (voir [`Library::launch_pets`]) : le préféré s'il en est,
    /// sinon la mascotte de Mouflon qui en est (le Mérinos d'Arles, puis
    /// l'agneau Southdown, chez les moutons ; Poete, chez les mascottes
    /// numériques, dans les builds qui le contiennent), sinon le premier
    /// animal livré dans l'app qui en est, sinon le premier de tous.
    #[must_use]
    pub fn default_index(&self, preferred: Option<&str>, launch: &str) -> usize {
        let among = self.launch_pets(launch);
        let find = |id: &str| {
            among
                .iter()
                .copied()
                .find(|&i| self.pets[i].def.meta.id == id)
        };
        preferred
            .and_then(find)
            .or_else(|| MASCOTS.iter().find_map(|id| find(id)))
            .or_else(|| self.bundled.iter().find_map(|id| find(id)))
            .or_else(|| among.first().copied())
            .unwrap_or(0)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Un petit animal desktopPet dans un seul fichier : son image est dans
    /// le fichier (ici, l'en-tête d'un PNG de 8 × 4 pixels, seul lu au
    /// chargement).
    const LONE_XML: &str = r#"<?xml version="1.0" encoding="utf-8"?>
<animations xmlns="https://esheep.petrucci.ch/">
  <header><author>Essai</author><title>Mouton seul</title><petname>Seul</petname><version>1</version><info>un fichier</info></header>
  <image><tilesx>2</tilesx><tilesy>1</tilesy><png><![CDATA[iVBORw0KGgoAAAANSUhEUgAAAAgAAAAECAYAAAAAAAAA]]></png><transparency>Magenta</transparency></image>
  <spawns>
    <spawn id="1" probability="100"><x>screenW+10</x><y>areaH-imageH</y><next probability="100">1</next></spawn>
  </spawns>
  <animations>
    <animation id="1"><name>walk</name>
      <start><x>-2</x><y>0</y><interval>200</interval></start><end><x>-2</x><y>0</y><interval>200</interval></end>
      <sequence repeat="5" repeatfrom="0"><frame>0</frame><frame>1</frame><next probability="100" only="none">1</next></sequence>
      <border><next probability="100" only="none">2</next></border>
      <gravity><next probability="100">5</next></gravity>
    </animation>
    <animation id="2"><name>rotate</name>
      <start><x>0</x><y>0</y><interval>200</interval></start><end><x>0</x><y>0</y><interval>200</interval></end>
      <sequence repeat="0" repeatfrom="0"><frame>1</frame><action>flip</action><next probability="100">1</next></sequence>
    </animation>
    <animation id="5"><name>fall</name>
      <start><x>0</x><y>1</y><interval>100</interval></start><end><x>0</x><y>10</y><interval>40</interval></end>
      <sequence repeat="20" repeatfrom="0"><frame>1</frame><next probability="100">5</next></sequence>
      <border><next probability="100">6</next></border>
    </animation>
    <animation id="6"><name>fall soft</name>
      <start><x>0</x><y>0</y><interval>200</interval><offsety>1</offsety></start><end><x>0</x><y>0</y><interval>200</interval><offsety>0</offsety></end>
      <sequence repeat="0" repeatfrom="0"><frame>0</frame><next probability="100">1</next></sequence>
    </animation>
    <animation id="7"><name>drag</name>
      <start><x>0</x><y>0</y><interval>50</interval></start><end><x>0</x><y>0</y><interval>50</interval></end>
      <sequence repeat="0" repeatfrom="0"><frame>0</frame><next probability="100">1</next></sequence>
    </animation>
  </animations>
</animations>"#;

    /// Vu à la relecture : un `pet.toml` déposé sous un autre nom se
    /// chargeait, était copié seul (sans ses dessins), puis n'était jamais
    /// relu ; l'app faisait alors arriver le premier animal de la liste à la
    /// place de celui qu'on visait, et en faisait le préféré. Il est refusé,
    /// et rien n'est copié. Un fichier desktopPet seul s'installe toujours.
    #[test]
    fn a_pet_file_under_another_name_is_refused_and_nothing_is_copied() {
        let root = std::env::temp_dir().join(format!("mouflon-test-nom-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&root);
        let pets = root.join("pets");
        let source = root.join("source");
        std::fs::create_dir_all(&source).unwrap();
        let bleu = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../pets/bleu-du-maine");
        std::fs::write(
            source.join("mon-animal.toml"),
            include_str!("../../../pets/bleu-du-maine/pet.toml")
                .replace("id = \"bleu-du-maine\"", "id = \"robot\""),
        )
        .unwrap();
        for drawing in [
            "bleu-du-maine.svg",
            "bleu-du-maine-belier.svg",
            "bleu-du-maine-agneau.svg",
        ] {
            std::fs::copy(bleu.join(drawing), source.join(drawing)).unwrap();
        }
        copy_dir(&bleu.join("sons"), &source.join("sons")).unwrap();
        std::fs::write(source.join("mouton.xml"), LONE_XML).unwrap();

        let mut library = Library::load(Some(&pets), None, 1.0);
        let before = library.pets.len();
        // (Il se charge là où il est, avec ses dessins : ce n'est pas un
        // fichier illisible qu'on refuse.)
        assert!(load_pet(&source.join("mon-animal.toml")).is_ok());
        let refused = library.install(&source.join("mon-animal.toml"));
        assert!(refused.is_err(), "installé : {refused:?}");
        assert!(!pets.join("mon-animal.toml").exists(), "copié quand même");
        assert_eq!(library.pets.len(), before);
        assert_eq!(library.position_of("robot"), None);

        let id = library.install(&source.join("mouton.xml")).unwrap();
        assert!(pets.join("mouton.xml").is_file());
        assert_eq!(library.pets.len(), before + 1);
        assert!(library.position_of(&id).is_some(), "installé mais pas relu");
        let _ = std::fs::remove_dir_all(&root);
    }

    #[test]
    fn installs_a_pet_folder_and_notices_changes() {
        let root = std::env::temp_dir().join(format!("mouflon-test-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&root);
        let pets = root.join("pets");
        let source = root.join("source/robot");
        std::fs::create_dir_all(&source).unwrap();
        std::fs::write(
            source.join("pet.toml"),
            include_str!("../../../pets/bleu-du-maine/pet.toml")
                .replace("id = \"bleu-du-maine\"", "id = \"robot\""),
        )
        .unwrap();
        for drawing in [
            "bleu-du-maine.svg",
            "bleu-du-maine-belier.svg",
            "bleu-du-maine-agneau.svg",
        ] {
            std::fs::copy(
                Path::new(env!("CARGO_MANIFEST_DIR"))
                    .join("../../pets/bleu-du-maine")
                    .join(drawing),
                source.join(drawing),
            )
            .unwrap();
        }
        copy_dir(
            &Path::new(env!("CARGO_MANIFEST_DIR")).join("../../pets/bleu-du-maine/sons"),
            &source.join("sons"),
        )
        .unwrap();

        let mut library = Library::load(Some(&pets), None, 1.0);
        assert_eq!(library.pets.len(), 1);
        assert_eq!(library.dir(), Some(pets.as_path()));
        assert!(!library.changed_since(&stamp(&pets)));
        let id = library.install(&source.join("pet.toml")).unwrap();
        assert_eq!(id, "robot");
        assert!(
            pets.join("robot/bleu-du-maine.svg").is_file(),
            "le dessin suit"
        );
        assert_eq!(library.pets.len(), 2);
        assert!(
            !library.changed_since(&stamp(&pets)),
            "un animal installé ne recharge pas tout"
        );

        std::fs::create_dir_all(pets.join("vide")).unwrap();
        assert!(
            library.changed_since(&stamp(&pets)),
            "un nouveau dossier est remarqué"
        );

        // Plus grand : le dessin et le corps suivent, l'original ne change pas.
        let height = |library: &Library| library.pets[0].art().unwrap().height;
        let normal = height(&library);
        let width = library.pets[0].def.physics.width;
        library.set_size(1.5);
        assert!((height(&library) - normal * 1.5).abs() < 0.01);
        assert!((library.pets[0].def.physics.width - width * 1.5).abs() < 0.01);
        library.set_size(1.0);
        assert!((height(&library) - normal).abs() < 0.01);
        let _ = std::fs::remove_dir_all(&root);
    }

    /// Un animal dans le dossier de l'utilisateur : une copie du Bleu du Maine.
    fn user_pet(pets: &Path, id: &str) -> PathBuf {
        let dir = pets.join(id);
        copy_dir(
            &Path::new(env!("CARGO_MANIFEST_DIR")).join("../../pets/bleu-du-maine"),
            &dir,
        )
        .unwrap();
        let toml = std::fs::read_to_string(dir.join("pet.toml")).unwrap();
        std::fs::write(
            dir.join("pet.toml"),
            toml.replace("id = \"bleu-du-maine\"", &format!("id = \"{id}\"")),
        )
        .unwrap();
        dir
    }

    #[test]
    fn drawings_wait_for_their_pets_first_appearance_and_shipped_ones_survive_a_reload() {
        let root = std::env::temp_dir().join(format!("mouflon-paresse-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&root);
        let pets = root.join("pets");
        user_pet(&pets, "robot");
        let mut library = Library::load(Some(&pets), None, 1.0);
        assert_eq!(library.pets.len(), 2);
        assert!(
            library.pets.iter().all(|e| !e.art_ready()),
            "au lancement, aucun dessin n'est lu ni cuit"
        );
        let builtin = library.pets[0].art().expect("dessin de l'animal intégré");
        library.pets[1].art().expect("dessin du robot");

        library.reload();
        assert!(
            library.pets[0]
                .art()
                .is_some_and(|art| Arc::ptr_eq(&art, &builtin)),
            "l'animal intégré garde son dessin"
        );
        assert!(
            !library.pets[1].art_ready(),
            "ceux du dossier de l'utilisateur sont relus"
        );

        // Une autre taille pour tous : les dessins se refont, à la demande.
        library.set_size(1.5);
        library.reload();
        assert!(library.pets.iter().all(|e| !e.art_ready()));
        assert!((library.pets[0].size - 1.5).abs() < 1e-6);
        let _ = std::fs::remove_dir_all(&root);
    }

    #[test]
    fn an_unreadable_drawing_is_found_at_its_pets_first_appearance() {
        let root = std::env::temp_dir().join(format!("mouflon-illisible-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&root);
        let pets = root.join("pets");
        let dir = user_pet(&pets, "casse");
        std::fs::write(dir.join("bleu-du-maine.svg"), "<svg><g id=\"rien\"/></svg>").unwrap();
        let library = Library::load(Some(&pets), None, 1.0);
        let entry = &library.pets[library.index_of("casse")];
        assert_eq!(entry.def.meta.id, "casse");
        assert!(entry.art().is_none(), "rien à montrer");
        assert!(entry.art_ready(), "il n'est pas relu à chaque demande");
        let _ = std::fs::remove_dir_all(&root);
    }

    #[test]
    fn a_gallery_preview_does_not_keep_its_drawing() {
        let library = Library::load(None, None, 1.0);
        let entry = &library.pets[0];
        assert!(entry.art_for_preview().is_some());
        assert!(!entry.art_ready(), "le dessin de l'aperçu est oublié");
        let ready = entry.art().unwrap();
        assert!(
            entry
                .art_for_preview()
                .is_some_and(|art| Arc::ptr_eq(&art, &ready)),
            "un dessin déjà prêt sert tel quel"
        );
    }

    #[test]
    fn a_lamb_entry_keeps_its_age_and_relative_size_across_resizes() {
        let mother = Entry::new(builtin_pet().unwrap(), 2.0, None);
        let young = mother.as_lamb(2.0).unwrap();
        // L'agneau a son dessin, à sa taille.
        let lamb_height = mother
            .base
            .variant(mouflon_core::Variant::Lamb)
            .unwrap()
            .physics
            .height;
        assert!(lamb_height < mother.base.physics.height * 0.75);
        assert_eq!(mother.lamb_weeks(), 2.0);
        assert!(young.is_lamb_of(&mother, 2.0));
        assert!(!young.is_lamb_of(&mother, 7.0));
        assert!(young.same_base_at_size(&young.clone(), 2.0));
        assert!(!young.same_base_at_size(&mother, 2.0));
        assert_eq!(young.def.age, mouflon_core::Age::Lamb { weeks: 2.0 });
        for size in [0.5, 1.0, 3.0, 2.0] {
            let resized = young.at_size(size);
            assert_eq!(resized.def.age, young.def.age);
            assert!((resized.def.physics.height - lamb_height * size).abs() < 0.01);
            assert!(
                !resized.art_ready(),
                "le dessin se prépare seulement à la demande"
            );
        }
        assert_eq!(mother.def.age, mouflon_core::Age::Adult);
    }

    #[test]
    fn a_live_mothers_source_keeps_her_personal_size_after_a_library_reload() {
        let mut library = Library::load(None, None, 1.0);
        let preserved = library.at_size(0, 3.0).unwrap();
        let lamb_height = preserved
            .base
            .variant(mouflon_core::Variant::Lamb)
            .unwrap()
            .physics
            .height;
        library.reload();
        assert!(library.entry_of(&preserved.def).is_none());
        let young = preserved.as_lamb(2.0).unwrap();
        assert_eq!(young.size, 3.0);
        assert!((young.def.physics.height - lamb_height * 3.0).abs() < 0.01);
        let resized = young.at_size(2.0);
        assert!((resized.def.physics.height - lamb_height * 2.0).abs() < 0.01);
        assert_eq!(resized.def.age, mouflon_core::Age::Lamb { weeks: 2.0 });
    }

    #[test]
    fn one_pet_can_have_its_own_size() {
        let mut library = Library::load(None, None, 1.0);
        let normal = library.pets[0].clone();
        // Le menu d'un animal : la même espèce, plus grande, à côté.
        let big = library.at_size(0, 1.5).unwrap();
        assert_eq!(big.size, 1.5);
        assert!((big.def.physics.width - normal.def.physics.width * 1.5).abs() < 0.01);
        let height = big.art().unwrap().height;
        assert!((height - normal.art().unwrap().height * 1.5).abs() < 0.01);
        assert_eq!(library.pets[0].size, 1.0, "les autres ne changent pas");
        // Redemandée : la même (son dessin déjà prêt).
        let again = library.at_size(0, 1.5).unwrap();
        assert!(Arc::ptr_eq(&again.def, &big.def));
        assert!(again.art_ready());
        assert!(library.entry_of(&big.def).is_some());
        assert_eq!(library.sizes_of(&normal.def.meta.id).count(), 2);
        // À la taille de tous : l'entrée de tous.
        let same = library.at_size(0, 1.0).unwrap();
        assert!(Arc::ptr_eq(&same.def, &library.pets[0].def));
        // Plus personne à cette taille : oubliée.
        library.forget_unused_sizes(|_| false);
        assert!(library.entry_of(&big.def).is_none());
        // Une nouvelle taille pour tous efface les tailles à part.
        let _ = library.at_size(0, 2.0);
        library.set_size(0.75);
        assert_eq!(library.sizes_of(&normal.def.meta.id).count(), 1);
    }

    /// Un animal au format desktopPet dans le dossier de l'utilisateur, sous
    /// l'identifiant `id` : Pelote, sans la famille que dit son `pet.toml`
    /// (ou avec celle qu'on lui donne).
    fn user_mascot(pets: &Path, id: &str, category: Option<&str>) {
        let dir = pets.join(id);
        copy_dir(
            &Path::new(env!("CARGO_MANIFEST_DIR")).join("../../tests/donnees/pelote"),
            &dir,
        )
        .unwrap();
        let toml: String = std::fs::read_to_string(dir.join("pet.toml"))
            .unwrap()
            .lines()
            .filter(|line| !line.starts_with("category"))
            .map(|line| {
                if line == "id = \"pelote\"" {
                    let family = category
                        .map(|c| format!("\ncategory = \"{c}\""))
                        .unwrap_or_default();
                    format!("id = \"{id}\"{family}\n")
                } else {
                    format!("{line}\n")
                }
            })
            .collect();
        assert!(toml.contains(&format!("id = \"{id}\"")));
        std::fs::write(dir.join("pet.toml"), toml).unwrap();
    }

    /// Un animal d'une autre famille dans le dossier de l'utilisateur : une
    /// copie du Bleu du Maine qui se dit de la famille `category`.
    fn user_pet_of(pets: &Path, id: &str, category: &str) {
        let dir = user_pet(pets, id);
        let toml = std::fs::read_to_string(dir.join("pet.toml")).unwrap();
        let family = format!("category = \"{category}\"");
        let toml = toml.replace("category = \"sheep\"", &family);
        assert!(toml.contains(&family), "le Bleu du Maine dit sa famille");
        std::fs::write(dir.join("pet.toml"), toml).unwrap();
    }

    fn ids(library: &Library, pets: &[usize]) -> Vec<String> {
        pets.iter()
            .map(|&i| library.pets[i].def.meta.id.clone())
            .collect()
    }

    /// Poete est une mascotte numérique : il n'arrive plus au lancement ni
    /// dans « Ajouter dix animaux au hasard », à moins qu'on ait choisi les
    /// mascottes numériques.
    #[test]
    fn a_digital_mascot_comes_at_launch_only_when_mascots_are_chosen() {
        let root = std::env::temp_dir().join(format!("mouflon-lancement-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&root);
        let pets = root.join("pets");
        user_mascot(&pets, "poete", None);
        user_pet(&pets, "robot");
        let library = Library::load(Some(&pets), None, 1.0);
        let poete = library.index_of("poete");
        assert_eq!(library.pets[poete].def.meta.id, "poete");
        assert_eq!(
            library.pets[poete].def.meta.category(),
            mouflon_core::MASCOTS,
            "un animal desktopPet est une mascotte numérique"
        );
        assert_eq!(library.launch_choices(), ["sheep", mouflon_core::MASCOTS]);

        // Des moutons au lancement (le choix d'origine) : jamais Poete, même
        // s'il était le préféré jusque-là.
        let sheep = library.launch_pets("sheep");
        assert_eq!(ids(&library, &sheep), ["bleu-du-maine", "robot"]);
        for preferred in [None, Some("poete"), Some("inconnu")] {
            let index = library.default_index(preferred, "sheep");
            assert_eq!(
                library.pets[index].def.meta.id, "bleu-du-maine",
                "préféré : {preferred:?}"
            );
        }
        // Le préféré, quand il est de la famille.
        let index = library.default_index(Some("robot"), "sheep");
        assert_eq!(library.pets[index].def.meta.id, "robot");

        // Les mascottes numériques choisies : c'est lui, et lui seul.
        assert_eq!(ids(&library, &library.launch_pets("mascots")), ["poete"]);
        for preferred in [None, Some("poete"), Some("robot")] {
            let index = library.default_index(preferred, "mascots");
            assert_eq!(library.pets[index].def.meta.id, "poete", "{preferred:?}");
        }

        // Un choix qui n'a pas d'animal ici (des vaches, pas encore là ; un
        // mot inconnu ; « un peu de tout ») vaut la seule famille de vrais
        // animaux qu'il y ait : les moutons, jamais les mascottes.
        for launch in ["cattle", "", "n'importe quoi", ANY_FAMILY] {
            assert_eq!(library.launch_here(launch), "sheep", "« {launch} »");
            assert_eq!(library.launch_pets(launch), sheep, "« {launch} »");
            let index = library.default_index(Some("poete"), launch);
            assert_ne!(index, poete, "« {launch} »");
        }
        assert_eq!(library.launch_here("mascots"), "mascots");

        // « Ajouter dix animaux au hasard » et le second lancement : jamais
        // Poete, à moins d'avoir choisi les mascottes numériques.
        let mut rng = mouflon_core::math::Rng::new(7);
        for launch in ["sheep", "cattle", "", ANY_FAMILY] {
            let drawn = library.random_pets(launch, 300, &mut rng);
            assert_eq!(drawn.len(), 300);
            assert!(!drawn.contains(&poete), "« {launch} » a tiré Poete");
            assert!(
                sheep.iter().all(|pet| drawn.contains(pet)),
                "« {launch} » ne tire pas tous les moutons"
            );
            let starters = library.starters(Some("poete"), launch, None, 5, &mut rng);
            assert_eq!(
                ids(&library, &starters),
                ["bleu-du-maine"; 5],
                "« {launch} »"
            );
        }
        assert_eq!(library.random_pets("mascots", 12, &mut rng), [poete; 12]);
        assert_eq!(
            library.starters(None, "mascots", None, 3, &mut rng),
            [poete; 3]
        );

        // Une mascotte prise au menu, ou déposée, ne devient pas la
        // préférée tant que le lancement est aux moutons.
        let robot = library.index_of("robot");
        assert!(!library.can_be_favorite(poete, "sheep"));
        assert!(!library.can_be_favorite(poete, ""));
        assert!(library.can_be_favorite(robot, "sheep"));
        assert!(library.can_be_favorite(poete, "mascots"));
        assert!(!library.can_be_favorite(robot, "mascots"));

        // L'animal qu'on essaie (donné sur la ligne de commande) arrive
        // quelle que soit sa famille, et tient le haut du menu ; un nom
        // inconnu ne change rien.
        assert_eq!(
            library.favorite(Some("robot"), "sheep", Some("poete")),
            poete
        );
        assert_eq!(
            library.starters(Some("robot"), "sheep", Some("poete"), 4, &mut rng),
            [poete; 4]
        );
        assert_eq!(
            library.favorite(Some("robot"), "sheep", Some("inconnu")),
            robot
        );
        assert_eq!(library.favorite(Some("robot"), "sheep", None), robot);

        // Qui n'a encore rien choisi : les moutons, même si Poete était son
        // préféré (les mascottes n'arrivent que si on les choisit).
        for preferred in [None, Some("poete"), Some("robot"), Some("inconnu")] {
            assert_eq!(library.first_launch(preferred), "sheep", "{preferred:?}");
        }
        let _ = std::fs::remove_dir_all(&root);
    }

    /// Les choix du lancement : les familles qui ont des animaux ici, comme
    /// dans le menu ; « Un peu de tout » dès qu'il y a deux familles de
    /// vrais animaux, et il laisse les mascottes numériques de côté.
    #[test]
    fn the_launch_choices_are_the_families_here_and_a_mix_of_the_real_ones() {
        let root = std::env::temp_dir().join(format!("mouflon-familles-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&root);
        let pets = root.join("pets");
        // Un Mouflon qui n'a que des moutons : pas de choix à montrer.
        let alone = Library::load(Some(&pets), None, 1.0);
        assert_eq!(alone.launch_choices(), ["sheep"]);
        assert_eq!(alone.launch_pets("sheep"), [0]);
        assert_eq!(alone.launch_pets("mascots"), [0], "pas de mascotte ici");

        user_pet_of(&pets, "marguerite", "cattle");
        user_pet_of(&pets, "cocotte", "chickens");
        user_mascot(&pets, "pixel", None);
        // Un animal desktopPet qui dit sa famille la garde.
        user_mascot(&pets, "laineux", Some("sheep"));
        let library = Library::load(Some(&pets), None, 1.0);
        assert_eq!(
            library.launch_choices(),
            [
                "sheep",
                "cattle",
                "chickens",
                mouflon_core::MASCOTS,
                ANY_FAMILY
            ]
        );
        assert_eq!(
            ids(&library, &library.launch_pets("cattle")),
            ["marguerite"]
        );
        assert_eq!(ids(&library, &library.launch_pets("chickens")), ["cocotte"]);
        assert_eq!(ids(&library, &library.launch_pets("mascots")), ["pixel"]);
        let mut sheep = ids(&library, &library.launch_pets("sheep"));
        sheep.sort();
        assert_eq!(sheep, ["bleu-du-maine", "laineux"]);
        let mut any = ids(&library, &library.launch_pets(ANY_FAMILY));
        any.sort();
        assert_eq!(any, ["bleu-du-maine", "cocotte", "laineux", "marguerite"]);
        // Le préféré d'une famille ne vaut que pour elle.
        let index = library.default_index(Some("marguerite"), "cattle");
        assert_eq!(library.pets[index].def.meta.id, "marguerite");
        let index = library.default_index(Some("marguerite"), "sheep");
        assert_eq!(library.pets[index].def.meta.id, "bleu-du-maine");
        // Sans préféré dans la famille : son premier animal.
        let index = library.default_index(None, "chickens");
        assert_eq!(library.pets[index].def.meta.id, "cocotte");

        // Ce que vaut ici un choix, et ce que la fenêtre des réglages en
        // montre : une famille qui n'a pas d'animal ici, un mot inconnu ou
        // rien valent « un peu de tout », puisqu'il y a plusieurs familles
        // de vrais animaux.
        let listed = library.launch_choices();
        let rank = |launch: &str| launch_rank(&listed, &library.launch_here(launch));
        assert_eq!(rank("sheep"), Some(0));
        assert_eq!(rank("cattle"), Some(1));
        assert_eq!(rank("mascots"), Some(3));
        assert_eq!(rank(ANY_FAMILY), Some(4));
        for launch in ["dogs", "", "n'importe quoi"] {
            assert_eq!(library.launch_here(launch), ANY_FAMILY, "« {launch} »");
            assert_eq!(rank(launch), Some(4), "« {launch} » vaut un peu de tout");
            assert_eq!(
                library.launch_pets(launch),
                library.launch_pets(ANY_FAMILY),
                "« {launch} » : la fenêtre et le lancement disent la même chose"
            );
        }
        // Une liste montrée avant que la bibliothèque ne change : un choix
        // qui n'y est pas ne s'y montre pas.
        assert_eq!(launch_rank(&listed[..2], ANY_FAMILY), None);
        assert_eq!(launch_rank(&[], "sheep"), None, "pas de liste");

        // « Un peu de tout » : chacun tiré au hasard dans toutes les
        // familles de vrais animaux, jamais chez les mascottes numériques.
        let pixel = library.index_of("pixel");
        let mut rng = mouflon_core::math::Rng::new(11);
        let drawn = library.random_pets(ANY_FAMILY, 400, &mut rng);
        assert!(
            !drawn.contains(&pixel),
            "« un peu de tout » a tiré une mascotte"
        );
        let mut families: Vec<&str> = drawn
            .iter()
            .map(|&i| library.pets[i].def.meta.category())
            .collect();
        families.sort_unstable();
        families.dedup();
        assert_eq!(families, ["cattle", "chickens", "sheep"]);
        let starters = library.starters(Some("marguerite"), ANY_FAMILY, None, 40, &mut rng);
        assert_eq!(starters.len(), 40);
        assert!(!starters.contains(&pixel));
        assert!(
            starters.iter().any(|&i| i != starters[0]),
            "« un peu de tout » n'a lancé qu'une espèce"
        );
        // Une famille choisie : son animal, tant de fois.
        assert_eq!(
            ids(
                &library,
                &library.starters(None, "chickens", None, 3, &mut rng)
            ),
            ["cocotte"; 3]
        );
        assert!(library.can_be_favorite(library.index_of("marguerite"), ANY_FAMILY));
        assert!(!library.can_be_favorite(pixel, ANY_FAMILY));
        assert!(!library.can_be_favorite(pixel, "dogs"));

        // Qui n'a encore rien choisi garde la famille de son préféré, si
        // c'est un vrai animal ; sinon, les moutons.
        assert_eq!(library.first_launch(Some("marguerite")), "cattle");
        assert_eq!(library.first_launch(Some("cocotte")), "chickens");
        assert_eq!(library.first_launch(Some("laineux")), "sheep");
        assert_eq!(library.first_launch(Some("pixel")), "sheep");
        assert_eq!(library.first_launch(None), "sheep");
        let _ = std::fs::remove_dir_all(&root);
    }

    /// Une carte de la galerie ajoute l'espèce qu'elle montre, même si le
    /// dossier des animaux a changé depuis l'ouverture de la galerie.
    #[test]
    fn a_gallery_card_still_adds_its_own_species_after_the_folder_changes() {
        let root = std::env::temp_dir().join(format!("mouflon-galerie-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&root);
        let pets = root.join("pets");
        user_pet(&pets, "robot");
        user_pet(&pets, "zebre");
        let mut library = Library::load(Some(&pets), None, 1.0);
        // La galerie s'ouvre : elle numérote les espèces comme elles sont.
        let listed: Vec<String> = library.pets.iter().map(|e| e.def.meta.id.clone()).collect();
        let rank_of = |id: &str| listed.iter().position(|x| x == id).unwrap();
        let (robot, zebre) = (rank_of("robot"), rank_of("zebre"));
        // Une mascotte arrive dans le dossier : les rangs se décalent.
        user_mascot(&pets, "alpha", None);
        library.reload();
        let id_at =
            |library: &Library, rank: usize| library.pets.get(rank).map(|e| e.def.meta.id.clone());
        assert_ne!(
            id_at(&library, zebre).as_deref(),
            Some("zebre"),
            "le rang gardé désigne maintenant une autre espèce : c'est ce que l'essai garde"
        );
        for (rank, id) in [(robot, "robot"), (zebre, "zebre")] {
            let now = library.listed_index(&listed, rank).unwrap();
            assert_eq!(id_at(&library, now).as_deref(), Some(id));
        }
        // Une espèce sort du dossier : sa carte n'ajoute plus rien.
        std::fs::remove_dir_all(pets.join("robot")).unwrap();
        library.reload();
        assert_eq!(
            library.listed_index(&listed, robot),
            None,
            "il n'est plus là"
        );
        let now = library.listed_index(&listed, zebre).unwrap();
        assert_eq!(id_at(&library, now).as_deref(), Some("zebre"));
        assert_eq!(library.listed_index(&listed, 99), None);
        assert_eq!(library.listed_index(&[], 0), None);
        let _ = std::fs::remove_dir_all(&root);
    }

    /// Une famille qu'un `pet.toml` appellerait `any` : ce mot est celui du
    /// mélange. Elle ne se choisit pas seule, et ses animaux en font partie.
    #[test]
    fn a_family_named_like_the_mix_is_part_of_it_and_not_a_choice_of_its_own() {
        let root = std::env::temp_dir().join(format!("mouflon-melange-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&root);
        let pets = root.join("pets");
        user_pet_of(&pets, "truc", ANY_FAMILY);
        user_mascot(&pets, "pixel", None);
        let library = Library::load(Some(&pets), None, 1.0);
        assert_eq!(
            library.launch_choices(),
            ["sheep", mouflon_core::MASCOTS, ANY_FAMILY]
        );
        assert_eq!(library.launch_here(ANY_FAMILY), ANY_FAMILY);
        let mut any = ids(&library, &library.launch_pets(ANY_FAMILY));
        any.sort();
        assert_eq!(any, ["bleu-du-maine", "truc"]);
        // Préféré, il ne fait pas du mélange un premier choix : les moutons.
        assert_eq!(library.first_launch(Some("truc")), "sheep");
        let _ = std::fs::remove_dir_all(&root);
    }

    /// Une fois les images compressées, plus rien ne retient les images
    /// d'origine ; les suivantes viennent du disque, déjà compressées.
    #[test]
    fn compressed_images_replace_the_originals_and_are_kept_on_disk() {
        let dir = std::env::temp_dir().join(format!("mouflon-images-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        let file =
            Path::new(env!("CARGO_MANIFEST_DIR")).join("../../tests/donnees/pelote/pet.toml");
        let def = load_pet(&file).expect("Pelote");
        let images = Store::new(&dir).opened_with(mouflon_platform::map_file);
        let entry = Entry::new(def.clone(), 1.0, Some(images.clone()));
        let original = Arc::downgrade(&entry.art().expect("dessin"));
        assert!(entry.compressible());
        let done = entry.compress(2, &|| {}).expect("compressées");
        assert!(done.after * 3 < done.before, "{done:?}");
        assert!(original.upgrade().is_none(), "images d'origine libérées");
        assert!(
            entry.art().is_some_and(|a| a.from_cache()),
            "relues du disque"
        );
        assert!(!entry.compressible());
        assert!(entry.compress(2, &|| {}).is_none(), "une seule fois");

        // Lancement suivant : les images viennent du disque.
        let again = Entry::new(def, 1.0, Some(images));
        let art = again.art().expect("dessin");
        assert!(art.from_cache());
        assert_eq!(art.image_bytes(), done.after);
        assert!(!again.compressible());
        let _ = std::fs::remove_dir_all(&dir);
    }
}
