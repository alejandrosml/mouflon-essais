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

/// Mascottes de Mouflon, par ordre de préférence au premier lancement.
const MASCOTS: [&str; 3] = ["poete", "merinos-d-arles", "southdown"];

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

    /// Indice de l'animal d'identifiant donné, ou du premier.
    #[must_use]
    pub fn index_of(&self, id: &str) -> usize {
        self.pets
            .iter()
            .position(|e| e.def.meta.id == id)
            .unwrap_or(0)
    }

    /// L'animal préféré s'il existe, sinon la mascotte de Mouflon (Poete
    /// dans les builds qui le contiennent, puis le Mérinos d'Arles, puis
    /// l'agneau Southdown), sinon le premier animal livré dans l'app, sinon
    /// l'animal intégré.
    #[must_use]
    pub fn default_index(&self, preferred: Option<&str>) -> usize {
        let find = |id: &str| self.pets.iter().position(|e| e.def.meta.id == id);
        preferred
            .and_then(find)
            .or_else(|| MASCOTS.iter().find_map(|id| find(id)))
            .or_else(|| self.bundled.iter().find_map(|id| find(id)))
            .unwrap_or(0)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

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
