//! Les menus de Mouflon : celui de son icône dans la barre des menus (macOS)
//! ou la zone de notification (Windows), et celui d'un animal (clic droit).
//! Ce sont de vrais menus natifs : ils prennent d'eux-mêmes le style du
//! système.
//!
//! Les menus et la fenêtre des réglages mènent aux mêmes [`Action`], et
//! montrent tous le même état, celui de l'app : un réglage changé d'un côté
//! se voit aussitôt de l'autre.

use mouflon_platform::PopupItem;

use crate::i18n::Texts;

/// Ce que l'utilisateur demande, d'un menu ou de la fenêtre des réglages.
/// Un réglage y vient avec sa nouvelle valeur, jamais « l'inverse de
/// l'actuelle » : deux endroits qui le changent ne peuvent pas se contredire.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[cfg_attr(not(any(target_os = "macos", target_os = "windows")), allow(dead_code))]
pub enum Action {
    /// Ajoute l'animal préféré (le dernier choisi).
    AddDefault,
    /// Ajoute cet animal de la bibliothèque (un de plus), qui devient le préféré.
    Add(usize),
    /// Ajoute un agneau lié à cette brebis.
    AddLamb(u32),
    /// Ajoute [`RANDOM_COUNT`] animaux, chacun d'une espèce tirée au hasard.
    AddRandom,
    /// Ajoute un seul animal, d'une espèce tirée au hasard (Mouflon lancé
    /// une seconde fois : on voit qu'il est déjà là).
    AddOneRandom,
    /// Ajoute cet animal de la bibliothèque, tel qu'on le demande dans la
    /// galerie : un bélier, une brebis, un agneau.
    AddWanted(usize, Wanted),
    /// Ajoute un animal de chaque espèce de cette section (son rang dans
    /// [`sections`]).
    AddEach(usize),
    /// Dit au revoir à un animal de cette espèce (le dernier arrivé).
    RemoveOne(usize),
    RemoveAll,
    /// Cet animal-là (son numéro) dit au revoir.
    Goodbye(u32),
    /// Tous les animaux jouent ensemble (`sync` de desktopPet).
    Together,
    SetPaused(bool),
    /// Les animaux font entendre leurs sons, ou se taisent.
    SetSounds(bool),
    /// Taille de tous les animaux : rang dans [`crate::settings::SIZES`].
    SetSize(usize),
    /// Taille de cet animal-là (son numéro) seulement.
    SetPetSize(u32, usize),
    SetCollisions(bool),
    SetRealisticPhysics(bool),
    /// De temps en temps, un coup de vent ou une soucoupe volante.
    SetSurprises(bool),
    SetFrontmostOnly(bool),
    /// Avec plusieurs écrans : sur tous, ou sur le principal seulement.
    SetAllScreens(bool),
    /// Marcher sous le Dock (il faut l'autorisation d'Accessibilité).
    SetWalkUnderDock(bool),
    /// Le tempérament des animaux : rang dans [`crate::settings::MOODS`].
    SetMood(usize),
    /// Les raccourcis clavier de Mouflon, dans toutes les apps.
    SetShortcuts(bool),
    /// Échap deux fois, ou tenue, fait quitter Mouflon.
    SetEscapeQuits(bool),
    /// Ouvrir Mouflon à l'ouverture de session.
    SetOpenAtLogin(bool),
    OpenSettings,
    /// La galerie des animaux, avec leurs aperçus.
    OpenGallery,
    /// La fenêtre des réglages revient au premier plan : ce qu'elle montre
    /// de l'état du système (autorisations) est relu.
    RefreshSettings,
    OpenPetsFolder,
    OpenAccessibilitySettings,
    OpenLoginItemsSettings,
    About,
    Quit,
}

/// Combien d'animaux « Ajouter dix animaux au hasard » fait arriver.
pub const RANDOM_COUNT: usize = 10;

/// Les espèces de la bibliothèque rangées par famille : les familles connues
/// dans leur ordre ([`mouflon_core::CATEGORIES`]), puis les autres par ordre
/// alphabétique ; dans chacune, les espèces dans l'ordre de la bibliothèque.
/// `categories[i]` : la famille de la i-ème espèce.
#[must_use]
pub fn groups(categories: &[&str]) -> Vec<(String, Vec<usize>)> {
    let mut groups: Vec<(String, Vec<usize>)> = Vec::new();
    for (index, category) in categories.iter().enumerate() {
        match groups.iter_mut().find(|(c, _)| c == category) {
            Some((_, pets)) => pets.push(index),
            None => groups.push(((*category).to_owned(), vec![index])),
        }
    }
    let rank = |c: &str| {
        mouflon_core::CATEGORIES
            .iter()
            .position(|known| *known == c)
            .unwrap_or(mouflon_core::CATEGORIES.len())
    };
    groups.sort_by(|(a, _), (b, _)| rank(a).cmp(&rank(b)).then_with(|| a.cmp(b)));
    groups
}

/// Une section du menu et de la galerie : une famille d'animaux, ou, pour
/// une famille qui compte beaucoup d'espèces venues de plusieurs régions
/// (les races de moutons), l'une de ses régions.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Section {
    /// La famille (`sheep`…).
    pub family: String,
    /// La région (`france`…) quand la famille est rangée par régions ; une
    /// chaîne vide : les espèces qui ne disent pas la leur.
    pub region: Option<String>,
    /// Ses espèces, par leur rang dans la bibliothèque.
    pub pets: Vec<usize>,
}

/// Une famille se range par régions à partir de tant d'espèces (si elles
/// viennent d'au moins deux régions).
pub const SPLIT: usize = 10;

/// Les sections du menu, dans l'ordre : les familles comme [`groups`] ; une
/// grande famille est coupée en régions, les régions connues d'abord, dans
/// leur ordre ([`mouflon_core::REGIONS`]), puis les autres par ordre
/// alphabétique, puis les espèces sans région. `kinds[i]` : la famille et la
/// région de la i-ème espèce.
#[must_use]
pub fn sections(kinds: &[(&str, Option<&str>)]) -> Vec<Section> {
    let categories: Vec<&str> = kinds.iter().map(|(category, _)| *category).collect();
    let mut out = Vec::new();
    for (family, pets) in groups(&categories) {
        let mut regions: Vec<(&str, Vec<usize>)> = Vec::new();
        for &i in &pets {
            let region = kinds[i].1.unwrap_or("");
            match regions.iter_mut().find(|(r, _)| *r == region) {
                Some((_, list)) => list.push(i),
                None => regions.push((region, vec![i])),
            }
        }
        if pets.len() < SPLIT || regions.len() < 2 {
            out.push(Section {
                family,
                region: None,
                pets,
            });
            continue;
        }
        let rank = |r: &str| {
            if r.is_empty() {
                return usize::MAX;
            }
            mouflon_core::REGIONS
                .iter()
                .position(|known| *known == r)
                .unwrap_or(mouflon_core::REGIONS.len())
        };
        regions.sort_by(|(a, _), (b, _)| rank(a).cmp(&rank(b)).then_with(|| a.cmp(b)));
        out.extend(regions.into_iter().map(|(region, pets)| Section {
            family: family.clone(),
            region: Some(region.to_owned()),
            pets,
        }));
    }
    out
}

/// Le nom d'une région dans la langue de l'utilisateur ; une région inconnue
/// (donnée par un `pet.toml`) garde le sien, avec une majuscule ; sans
/// région, « Ailleurs ».
#[must_use]
pub fn region_name(t: &Texts, region: &str) -> String {
    match region {
        "france" => t.region_france.clone(),
        "british-isles" => t.region_british_isles.clone(),
        "europe" => t.region_europe.clone(),
        "asia-africa" => t.region_asia_africa.clone(),
        "americas" => t.region_americas.clone(),
        "oceania" => t.region_oceania.clone(),
        "" => t.region_other.clone(),
        other => {
            let mut chars = other.chars();
            chars.next().map_or_else(String::new, |first| {
                first.to_uppercase().chain(chars).collect()
            })
        }
    }
}

/// Le titre d'une section : sa région, précédée de sa famille quand il y a
/// plusieurs familles (`several`) ; sans région, sa famille.
#[must_use]
pub fn section_name(t: &Texts, section: &Section, several: bool) -> String {
    match &section.region {
        Some(region) if several => format!(
            "{} · {}",
            category_name(t, &section.family),
            region_name(t, region)
        ),
        Some(region) => region_name(t, region),
        None => category_name(t, &section.family),
    }
}

/// Ce qu'on demande d'un animal qu'on ajoute : un de son espèce tiré comme
/// le veut son fichier, ou précisément un bélier, une brebis, un agneau.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum Wanted {
    #[default]
    Any,
    Ram,
    Ewe,
    Lamb,
}

impl Wanted {
    /// Les choix de la galerie, dans l'ordre de sa liste.
    pub const ALL: [Self; 4] = [Self::Any, Self::Ram, Self::Ewe, Self::Lamb];

    /// Le sexe demandé, pour un adulte.
    #[must_use]
    pub fn sex(self) -> Option<mouflon_core::Sex> {
        match self {
            Self::Ram => Some(mouflon_core::Sex::Male),
            Self::Ewe => Some(mouflon_core::Sex::Female),
            Self::Any | Self::Lamb => None,
        }
    }
}

/// Une brebis sur tant arrive suivie de son agneau.
pub const LAMB_EVERY: u32 = 4;

/// Le nom d'une famille dans la langue de l'utilisateur ; une famille
/// inconnue (donnée par un `pet.toml`) garde le sien, avec une majuscule.
#[must_use]
pub fn category_name(t: &Texts, category: &str) -> String {
    match category {
        "sheep" => t.category_sheep.clone(),
        "farm" => t.category_farm.clone(),
        "cats" => t.category_cats.clone(),
        "dogs" => t.category_dogs.clone(),
        "birds" => t.category_birds.clone(),
        "wild" => t.category_wild.clone(),
        "fantasy" => t.category_fantasy.clone(),
        "others" => t.category_others.clone(),
        other => {
            let mut chars = other.chars();
            chars.next().map_or_else(String::new, |first| {
                first.to_uppercase().chain(chars).collect()
            })
        }
    }
}

/// Une espèce dans le menu : son nom, suivi du nombre d'animaux à l'écran
/// quand il y en a plusieurs (un seul se voit bien sans le dire).
#[cfg_attr(not(any(target_os = "macos", target_os = "windows")), allow(dead_code))]
fn counted(name: &str, count: usize) -> String {
    if count <= 1 {
        name.to_owned()
    } else {
        format!("{name} ({count})")
    }
}

/// Un article du menu d'un animal. Son identifiant dit ce qu'il fait, et
/// pour un réglage, la valeur qu'il donne (celle qu'on voyait en cliquant).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PetChoice {
    /// Cet animal dit au revoir.
    Goodbye,
    /// Un animal de plus de la même espèce.
    AddSame,
    AddLamb,
    Size(usize),
    Sounds(bool),
    Pause(bool),
    Settings,
}

impl PetChoice {
    const SIZES: u32 = 100;

    fn id(self) -> u32 {
        match self {
            Self::Goodbye => 1,
            Self::AddSame => 2,
            Self::Settings => 3,
            Self::AddLamb => 4,
            Self::Sounds(on) => 10 + u32::from(on),
            Self::Pause(on) => 20 + u32::from(on),
            Self::Size(i) => Self::SIZES + i as u32,
        }
    }

    #[must_use]
    pub fn from_id(id: u32) -> Option<Self> {
        Some(match id {
            1 => Self::Goodbye,
            2 => Self::AddSame,
            3 => Self::Settings,
            4 => Self::AddLamb,
            10 | 11 => Self::Sounds(id == 11),
            20 | 21 => Self::Pause(id == 21),
            id if (Self::SIZES..Self::SIZES + 100).contains(&id) => {
                Self::Size((id - Self::SIZES) as usize)
            }
            _ => return None,
        })
    }

    /// Ce que fait ce choix pour l'animal `pet`, d'une espèce de la
    /// bibliothèque (`species` : son rang, s'il y est encore).
    #[must_use]
    pub fn action(self, pet: u32, species: Option<usize>) -> Option<Action> {
        Some(match self {
            Self::Goodbye => Action::Goodbye(pet),
            Self::AddSame => Action::Add(species?),
            Self::AddLamb => Action::AddLamb(pet),
            Self::Size(i) => Action::SetPetSize(pet, i),
            Self::Sounds(on) => Action::SetSounds(on),
            Self::Pause(on) => Action::SetPaused(on),
            Self::Settings => Action::OpenSettings,
        })
    }
}

/// Un article général, là où Mouflon n'a pas d'icône dans une barre des
/// menus (Linux) : le menu d'un animal les propose à la suite des siens.
/// Leurs numéros commencent là où finissent ceux de [`PetChoice`].
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum GlobalChoice {
    /// Un animal de cette espèce de la bibliothèque.
    Add(usize),
    AddRandom,
    GoodbyeAll,
    Together,
    About,
    Quit,
}

impl GlobalChoice {
    const FIRST: u32 = 1000;
    const SPECIES: u32 = 2000;

    #[cfg_attr(target_os = "macos", allow(dead_code))]
    fn id(self) -> u32 {
        match self {
            Self::AddRandom => Self::FIRST,
            Self::GoodbyeAll => Self::FIRST + 1,
            Self::Together => Self::FIRST + 2,
            Self::About => Self::FIRST + 3,
            Self::Quit => Self::FIRST + 4,
            Self::Add(species) => Self::SPECIES + species as u32,
        }
    }

    #[must_use]
    pub fn from_id(id: u32) -> Option<Self> {
        Some(match id {
            1000 => Self::AddRandom,
            1001 => Self::GoodbyeAll,
            1002 => Self::Together,
            1003 => Self::About,
            1004 => Self::Quit,
            id if id >= Self::SPECIES => Self::Add((id - Self::SPECIES) as usize),
            _ => return None,
        })
    }

    #[must_use]
    pub fn action(self) -> Action {
        match self {
            Self::Add(species) => Action::Add(species),
            Self::AddRandom => Action::AddRandom,
            Self::GoodbyeAll => Action::RemoveAll,
            Self::Together => Action::Together,
            Self::About => Action::About,
            Self::Quit => Action::Quit,
        }
    }
}

/// Les articles généraux, à mettre à la suite du menu d'un animal : ajouter
/// un autre animal (chaque espèce, rangée par famille et par région comme
/// dans le menu de la barre des menus), dix au hasard, dire au revoir à
/// tous, jouer ensemble, « À propos », quitter. `names[i]` : le nom de la
/// i-ème espèce de la bibliothèque ; `kinds[i]` : sa famille et sa région.
#[cfg_attr(any(target_os = "macos", target_os = "windows"), allow(dead_code))]
#[must_use]
pub fn global_menu(names: &[&str], kinds: &[(&str, Option<&str>)], t: &Texts) -> Vec<PopupItem> {
    let item = |choice: GlobalChoice, label: &str| PopupItem::Item {
        id: choice.id(),
        label: label.to_owned(),
        checked: false,
        enabled: true,
    };
    let sections = sections(kinds);
    let split = sections.len() > 1;
    let mut add = Vec::new();
    for section in &sections {
        let pets: Vec<PopupItem> = section
            .pets
            .iter()
            .filter_map(|&i| Some(item(GlobalChoice::Add(i), names.get(i)?)))
            .collect();
        if !split {
            add = pets;
            break;
        }
        // « Moutons › France », ou la famille seule.
        let family = category_name(t, &section.family);
        let label = match section.region.as_deref() {
            Some(region) => format!("{family} · {}", region_name(t, region)),
            None => family,
        };
        add.push(PopupItem::Submenu { label, items: pets });
    }
    vec![
        PopupItem::Separator,
        PopupItem::Submenu {
            label: t.add_pet.clone(),
            items: add,
        },
        item(GlobalChoice::AddRandom, &t.add_random),
        item(GlobalChoice::GoodbyeAll, &t.goodbye_all),
        item(GlobalChoice::Together, &t.together),
        PopupItem::Separator,
        item(GlobalChoice::About, &t.about),
        item(GlobalChoice::Quit, &t.quit),
    ]
}

/// Un libellé de menu et, à sa droite, le raccourci clavier qui fait la
/// même chose (`keys`, écrit en toutes lettres : « Ctrl+Alt+Win+Q ») : un
/// menu de Windows range à droite ce qui suit une tabulation.
#[cfg_attr(not(any(target_os = "macos", target_os = "windows")), allow(dead_code))]
#[must_use]
pub fn keyed(label: &str, keys: Option<&str>) -> String {
    match keys {
        Some(keys) => format!("{label}\t{keys}"),
        None => label.to_owned(),
    }
}

/// La fin du menu d'un animal sous Windows, où l'icône de Mouflon est
/// souvent rangée hors de vue : « À propos », et quitter.
#[cfg_attr(not(target_os = "windows"), allow(dead_code))]
#[must_use]
pub fn leave_menu(t: &Texts) -> Vec<PopupItem> {
    let item = |choice: GlobalChoice, label: &str| PopupItem::Item {
        id: choice.id(),
        label: label.to_owned(),
        checked: false,
        enabled: true,
    };
    vec![
        PopupItem::Separator,
        item(GlobalChoice::About, &t.about),
        item(GlobalChoice::Quit, &t.quit),
    ]
}

/// Ce que le menu d'un animal montre de lui et de l'état de l'app.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct PetMenuState {
    /// Vraie brebis adulte native, qui peut avoir un agneau.
    pub is_ewe: bool,
    pub can_have_lamb: bool,
    pub paused: bool,
    pub sounds: bool,
    /// Rang de sa taille à lui dans [`crate::settings::SIZES`].
    pub size: usize,
}

/// Le menu d'un animal (clic droit) : son nom, ce qu'on peut faire de lui
/// (dont sa taille, à lui seul), puis les réglages qu'on change le plus,
/// comme dans la barre des menus.
#[must_use]
pub fn pet_menu(name: &str, state: PetMenuState, t: &Texts) -> Vec<PopupItem> {
    let item = |choice: PetChoice, label: &str, checked: bool| PopupItem::Item {
        id: choice.id(),
        label: label.to_owned(),
        checked,
        enabled: true,
    };
    let sizes = [&t.size_small, &t.size_normal, &t.size_large, &t.size_huge]
        .into_iter()
        .enumerate()
        .map(|(i, label)| item(PetChoice::Size(i), label, i == state.size))
        .collect();
    let pause = if state.paused { &t.resume } else { &t.pause };
    let mut items = vec![
        PopupItem::Header(name.to_owned()),
        item(
            PetChoice::Goodbye,
            &t.goodbye_named.replace("{name}", name),
            false,
        ),
        item(
            PetChoice::AddSame,
            &t.add_named.replace("{name}", name),
            false,
        ),
        PopupItem::Submenu {
            label: t.size_named.replace("{name}", name),
            items: sizes,
        },
        PopupItem::Separator,
        item(PetChoice::Sounds(!state.sounds), &t.sounds, state.sounds),
        item(PetChoice::Pause(!state.paused), pause, false),
        PopupItem::Separator,
        item(PetChoice::Settings, &t.settings, false),
    ];
    if state.is_ewe {
        items.insert(
            3,
            PopupItem::Item {
                id: PetChoice::AddLamb.id(),
                label: t.add_lamb.clone(),
                checked: false,
                enabled: state.can_have_lamb,
            },
        );
    }
    items
}

#[cfg(any(target_os = "macos", target_os = "windows"))]
mod native {
    use std::cell::Cell;

    use tray_icon::menu::accelerator::{Accelerator, CMD_OR_CTRL, Code, Modifiers};
    use tray_icon::menu::{
        CheckMenuItem, Menu, MenuEvent, MenuId, MenuItem, PredefinedMenuItem, Submenu,
    };
    #[cfg(target_os = "windows")]
    use tray_icon::{Icon, TrayIcon, TrayIconBuilder};

    use std::cell::RefCell;

    use super::{Action, category_name, counted, region_name, sections};
    use crate::library::Library;

    /// Raccourci affiché dans le menu (⌘, ou ⌘Q sur Mac).
    fn shortcut(key: Code) -> Option<Accelerator> {
        cfg!(target_os = "macos").then(|| Accelerator::new(CMD_OR_CTRL, key))
    }

    /// Un raccourci de Mouflon dans toutes les apps (⌃⌥⌘ et sa lettre),
    /// montré dans le menu par macOS. (Sous Windows, il est écrit à droite
    /// du libellé : voir [`super::keyed`].)
    fn global(shortcut: crate::keys::Shortcut) -> Option<Accelerator> {
        use crate::keys::Shortcut;
        let code = match shortcut {
            Shortcut::Add => Code::KeyN,
            Shortcut::AddRandom => Code::KeyT,
            Shortcut::GoodbyeAll => Code::KeyB,
            Shortcut::Pause => Code::KeyP,
            Shortcut::Sounds => Code::KeyS,
            Shortcut::Quit => Code::KeyQ,
        };
        cfg!(target_os = "macos")
            .then(|| Accelerator::new(Modifiers::CONTROL | Modifiers::ALT | Modifiers::META, code))
    }

    /// Le menu de l'icône de Mouflon, en dix articles :
    ///
    /// - « Ajouter Southdown » (le préféré), « Ajouter un autre animal ▸ »
    ///   (la galerie, puis chaque espèce, avec le nombre d'animaux déjà à
    ///   l'écran s'il y en a plusieurs : un clic en ajoute un de plus ; rangées
    ///   par famille quand il y en a plusieurs, et une grande famille, les
    ///   races de moutons, par régions, chaque rangement avec « Un de
    ///   chaque »), « Ajouter dix animaux au hasard », « Dire au revoir ▸ »
    ///   (à tous, ou à un animal d'une espèce qui est à l'écran),
    ///   « Faire jouer ensemble » ;
    /// - « Mettre en pause » (ou « Reprendre »), « Sons » ;
    /// - « Réglages… » (⌘,), qui ouvre la fenêtre des réglages ;
    /// - « À propos », « Quitter » (⌘Q).
    ///
    /// Sur Mac, AppKit montre lui-même le menu de l'icône : on peut appuyer
    /// sur l'icône, glisser jusqu'à un article et relâcher dessus.
    pub struct TrayMenu {
        #[cfg(target_os = "macos")]
        _status: Option<mouflon_platform::StatusItem>,
        #[cfg(target_os = "windows")]
        _tray: TrayIcon,
        add_favorite: MenuItem,
        /// Le libellé du préféré, sans son raccourci.
        favorite: RefCell<String>,
        /// Les raccourcis de toutes les apps sont montrés dans le menu.
        keys_shown: Cell<bool>,
        gallery: MenuId,
        add: Vec<(MenuItem, usize)>,
        random: MenuItem,
        /// « Un de chaque » de chaque section (son rang dans [`sections`]).
        each: Vec<(MenuItem, usize)>,
        goodbye_menu: Submenu,
        goodbye_all: MenuItem,
        /// Une espèce par article, dans l'ordre de la bibliothèque : seules
        /// celles qui ont un animal à l'écran sont dans le menu (`shown`).
        goodbye: Vec<(MenuItem, usize)>,
        shown: RefCell<Vec<bool>>,
        names: Vec<String>,
        together: MenuItem,
        pause: MenuItem,
        /// Ce que montre l'article de pause : en pause, il propose de reprendre.
        paused: Cell<bool>,
        sounds: CheckMenuItem,
        settings: MenuId,
        about: MenuId,
        quit: MenuId,
        quit_item: MenuItem,
    }

    impl std::fmt::Debug for TrayMenu {
        fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
            f.debug_struct("TrayMenu").finish_non_exhaustive()
        }
    }

    /// Ce que rend une modification d'un menu.
    #[allow(non_camel_case_types)]
    type muda_result = Result<(), tray_icon::menu::Error>;

    impl TrayMenu {
        pub fn new(library: &Library, sounds_on: bool) -> Result<Self, Box<dyn std::error::Error>> {
            let t = crate::i18n::texts();
            let menu = Menu::new();
            let names: Vec<String> = library
                .pets
                .iter()
                .map(|entry| entry.def.meta.name.clone())
                .collect();

            // Les animaux.
            let add_favorite = MenuItem::new(&t.add_named, true, None);
            menu.append(&add_favorite)?;
            let add_menu = Submenu::new(&t.add_pet, true);
            // D'abord la galerie, où chacun se voit bouger.
            let gallery = MenuItem::new(&t.gallery, true, None);
            add_menu.append(&gallery)?;
            add_menu.append(&PredefinedMenuItem::separator())?;
            let kinds: Vec<(&str, Option<&str>)> = library
                .pets
                .iter()
                .map(|entry| (entry.def.meta.category(), entry.def.meta.region()))
                .collect();
            let sections = sections(&kinds);
            let mut add = Vec::new();
            let mut each = Vec::new();
            // Les espèces d'une section, puis « Un de chaque » (`first` : avant
            // elles, comme dans le menu d'une famille).
            let mut fill = |menu: &Submenu, k: usize, first: bool| -> muda_result {
                let pets = &sections[k].pets;
                let item = MenuItem::new(&t.one_of_each, pets.len() > 1, None);
                if first {
                    menu.append(&item)?;
                    menu.append(&PredefinedMenuItem::separator())?;
                }
                for &i in pets {
                    let species = MenuItem::new(&names[i], true, None);
                    menu.append(&species)?;
                    add.push((species, i));
                }
                if !first && pets.len() > 1 {
                    menu.append(&PredefinedMenuItem::separator())?;
                    menu.append(&item)?;
                }
                each.push((item, k));
                Ok(())
            };
            // Les sections de chaque famille, à la suite.
            let mut families: Vec<(&str, Vec<usize>)> = Vec::new();
            for (k, section) in sections.iter().enumerate() {
                match families.last_mut() {
                    Some((family, list)) if *family == section.family => list.push(k),
                    _ => families.push((&section.family, vec![k])),
                }
            }
            if let [(_, only)] = families.as_slice() {
                if let [k] = only.as_slice() {
                    // Une seule famille, peu d'espèces : elles tout de suite.
                    fill(&add_menu, *k, false)?;
                } else {
                    // Une grande famille : une région par sous-menu.
                    for &k in only {
                        let region = sections[k].region.as_deref().unwrap_or_default();
                        let menu = Submenu::new(region_name(t, region), true);
                        fill(&menu, k, false)?;
                        add_menu.append(&menu)?;
                    }
                }
            } else {
                // Une famille par sous-menu : un de chaque, puis ses espèces ;
                // une grande famille, ses régions.
                for (family, list) in &families {
                    let family = Submenu::new(category_name(t, family), true);
                    if let [k] = list.as_slice() {
                        fill(&family, *k, true)?;
                    } else {
                        for &k in list {
                            let region = sections[k].region.as_deref().unwrap_or_default();
                            let menu = Submenu::new(region_name(t, region), true);
                            fill(&menu, k, false)?;
                            family.append(&menu)?;
                        }
                    }
                    add_menu.append(&family)?;
                }
            }
            menu.append(&add_menu)?;
            let random = MenuItem::new(&t.add_random, !names.is_empty(), None);
            menu.append(&random)?;
            let goodbye_menu = Submenu::new(&t.goodbye, true);
            let goodbye_all = MenuItem::new(&t.goodbye_all, false, None);
            goodbye_menu.append(&goodbye_all)?;
            goodbye_menu.append(&PredefinedMenuItem::separator())?;
            // Les espèces viennent dans ce menu quand elles ont un animal à
            // l'écran (voir `set_flock`) : il reste court.
            let goodbye: Vec<(MenuItem, usize)> = names
                .iter()
                .enumerate()
                .map(|(i, name)| (MenuItem::new(name, true, None), i))
                .collect();
            menu.append(&goodbye_menu)?;
            let together = MenuItem::new(&t.together, false, None);
            menu.append(&together)?;
            menu.append(&PredefinedMenuItem::separator())?;

            // Ce qu'on change souvent.
            let pause = MenuItem::new(&t.pause, true, None);
            menu.append(&pause)?;
            let sounds = CheckMenuItem::new(&t.sounds, true, sounds_on, None);
            menu.append(&sounds)?;
            menu.append(&PredefinedMenuItem::separator())?;

            // Tout le reste est dans la fenêtre des réglages.
            let settings = MenuItem::new(&t.settings, true, shortcut(Code::Comma));
            menu.append(&settings)?;
            menu.append(&PredefinedMenuItem::separator())?;

            let about = MenuItem::new(&t.about, true, None);
            menu.append(&about)?;
            let quit = MenuItem::new(&t.quit, true, shortcut(Code::KeyQ));
            menu.append(&quit)?;

            let size = 36;
            // Sur Mac, une icône « modèle » que macOS teinte selon la barre
            // des menus, et dont AppKit montre lui-même le menu.
            #[cfg(target_os = "macos")]
            let status = mouflon_platform::StatusItem::new(
                &crate::icon::tray_icon(size),
                size,
                18.0,
                "Mouflon",
                &menu,
            );
            #[cfg(target_os = "windows")]
            let tray = TrayIconBuilder::new()
                .with_menu(Box::new(menu))
                .with_tooltip("Mouflon")
                .with_icon(Icon::from_rgba(crate::icon::tray_icon(size), size, size)?)
                .build()?;

            Ok(Self {
                #[cfg(target_os = "macos")]
                _status: status,
                #[cfg(target_os = "windows")]
                _tray: tray,
                add_favorite,
                favorite: RefCell::new(t.add_named.clone()),
                keys_shown: Cell::new(false),
                gallery: gallery.id().clone(),
                add,
                random,
                each,
                goodbye_menu,
                goodbye_all,
                shown: RefCell::new(vec![false; goodbye.len()]),
                goodbye,
                names,
                together,
                pause,
                paused: Cell::new(false),
                sounds,
                settings: settings.id().clone(),
                about: about.id().clone(),
                quit: quit.id().clone(),
                quit_item: quit,
            })
        }

        /// Le préféré (rang dans la bibliothèque) et le nombre d'animaux de
        /// chaque espèce à l'écran : les articles d'ajout et d'au revoir suivent.
        pub fn set_flock(&self, favorite: usize, counts: &[usize]) {
            let t = crate::i18n::texts();
            if let Some(name) = self.names.get(favorite) {
                let label = t.add_named.replace("{name}", name);
                self.add_favorite
                    .set_text(self.keyed(&label, crate::keys::Shortcut::Add));
                *self.favorite.borrow_mut() = label;
            }
            let count = |i: usize| counts.get(i).copied().unwrap_or(0);
            for (item, i) in &self.add {
                item.set_text(counted(&self.names[*i], count(*i)));
            }
            // « À tous », le séparateur, puis les espèces à l'écran, dans
            // l'ordre de la bibliothèque.
            let mut shown = self.shown.borrow_mut();
            let mut position = 2;
            for (k, (item, i)) in self.goodbye.iter().enumerate() {
                let here = count(*i) > 0;
                if here {
                    item.set_text(counted(&self.names[*i], count(*i)));
                }
                let changed = match (here, shown[k]) {
                    (true, false) => self.goodbye_menu.insert(item, position),
                    (false, true) => self.goodbye_menu.remove(item),
                    _ => Ok(()),
                };
                match changed {
                    Ok(()) => shown[k] = here,
                    Err(e) => log::warn!("menu « Dire au revoir » : {e}"),
                }
                position += usize::from(shown[k]);
            }
            let anyone = counts.iter().any(|c| *c > 0);
            self.goodbye_all.set_enabled(anyone);
            self.together.set_enabled(anyone);
        }

        pub fn action(&self, event: &MenuEvent) -> Option<Action> {
            let id = &event.id;
            if let Some((_, i)) = self.add.iter().find(|(item, _)| item.id() == id) {
                return Some(Action::Add(*i));
            }
            if let Some((_, i)) = self.goodbye.iter().find(|(item, _)| item.id() == id) {
                return Some(Action::RemoveOne(*i));
            }
            if let Some((_, g)) = self.each.iter().find(|(item, _)| item.id() == id) {
                return Some(Action::AddEach(*g));
            }
            if id == self.add_favorite.id() {
                Some(Action::AddDefault)
            } else if id == self.random.id() {
                Some(Action::AddRandom)
            } else if id == &self.gallery {
                Some(Action::OpenGallery)
            } else if id == self.goodbye_all.id() {
                Some(Action::RemoveAll)
            } else if id == self.together.id() {
                Some(Action::Together)
            } else if id == self.pause.id() {
                Some(Action::SetPaused(!self.paused.get()))
            } else if id == self.sounds.id() {
                // La case vient de changer d'elle-même : c'est ce que l'on voit.
                Some(Action::SetSounds(self.sounds.is_checked()))
            } else if id == &self.settings {
                Some(Action::OpenSettings)
            } else if id == &self.about {
                Some(Action::About)
            } else if id == &self.quit {
                Some(Action::Quit)
            } else {
                None
            }
        }

        pub fn set_paused(&self, paused: bool) {
            if self.paused.replace(paused) != paused {
                self.pause.set_text(self.pause_label());
            }
        }

        /// Ce que dit l'article de pause : en pause, il propose de reprendre.
        fn pause_label(&self) -> String {
            let t = crate::i18n::texts();
            let label = if self.paused.get() {
                &t.resume
            } else {
                &t.pause
            };
            self.keyed(label, crate::keys::Shortcut::Pause)
        }

        /// Un libellé et, sous Windows quand les raccourcis de toutes les
        /// apps sont montrés, le sien à sa droite.
        fn keyed(&self, label: &str, shortcut: crate::keys::Shortcut) -> String {
            let keys = (cfg!(target_os = "windows") && self.keys_shown.get()).then(|| {
                let together = crate::i18n::windows_keys(crate::i18n::code());
                format!("{together}{}", shortcut.key().to_ascii_uppercase())
            });
            super::keyed(label, keys.as_deref())
        }

        pub fn set_sounds(&self, value: bool) {
            self.sounds.set_checked(value);
        }

        /// Les articles qui ont un raccourci dans toutes les apps le
        /// montrent (`on`), ou plus ; « Quitter » montre alors ⌃⌥⌘Q, sinon ⌘Q.
        pub fn show_shortcuts(&self, on: bool) {
            use crate::keys::Shortcut;
            if cfg!(target_os = "windows") && self.keys_shown.replace(on) != on {
                // Sous Windows, le raccourci s'écrit avec le libellé.
                let t = crate::i18n::texts();
                self.add_favorite
                    .set_text(self.keyed(&self.favorite.borrow(), Shortcut::Add));
                self.random
                    .set_text(self.keyed(&t.add_random, Shortcut::AddRandom));
                self.goodbye_all
                    .set_text(self.keyed(&t.goodbye_all, Shortcut::GoodbyeAll));
                self.pause.set_text(self.pause_label());
                self.sounds
                    .set_text(self.keyed(&t.sounds, Shortcut::Sounds));
                self.quit_item.set_text(self.keyed(&t.quit, Shortcut::Quit));
            }
            let shown = |s: Shortcut| if on { global(s) } else { None };
            let results = [
                self.add_favorite.set_accelerator(shown(Shortcut::Add)),
                self.random.set_accelerator(shown(Shortcut::AddRandom)),
                self.goodbye_all
                    .set_accelerator(shown(Shortcut::GoodbyeAll)),
                self.pause.set_accelerator(shown(Shortcut::Pause)),
                self.sounds.set_accelerator(shown(Shortcut::Sounds)),
                self.quit_item
                    .set_accelerator(shown(Shortcut::Quit).or_else(|| shortcut(Code::KeyQ))),
            ];
            for result in results {
                if let Err(e) = result {
                    log::warn!("raccourci du menu : {e}");
                }
            }
        }
    }
}

#[cfg(any(target_os = "macos", target_os = "windows"))]
pub use native::TrayMenu;
#[cfg(any(target_os = "macos", target_os = "windows"))]
pub use tray_icon::menu::MenuEvent;

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_general_items_add_any_species_and_quit() {
        let t = texts();
        let names = ["Texel", "Racka", "Pelote"];
        let kinds = [
            ("sheep", Some("europe")),
            ("sheep", Some("europe")),
            ("other", None),
        ];
        let menu = global_menu(&names, &kinds, &t);
        // Chaque article rend l'action qu'on attend de lui.
        let mut found = Vec::new();
        fn walk(items: &[PopupItem], found: &mut Vec<(String, u32)>) {
            for item in items {
                match item {
                    PopupItem::Item { id, label, .. } => found.push((label.clone(), *id)),
                    PopupItem::Submenu { items, .. } => walk(items, found),
                    _ => {}
                }
            }
        }
        walk(&menu, &mut found);
        let action = |label: &str| {
            let (_, id) = found.iter().find(|(l, _)| l == label).expect(label);
            // Jamais un numéro du menu de l'animal lui-même.
            assert!(PetChoice::from_id(*id).is_none(), "{label} : {id}");
            GlobalChoice::from_id(*id).expect(label).action()
        };
        assert_eq!(action("Texel"), Action::Add(0));
        assert_eq!(action("Racka"), Action::Add(1));
        assert_eq!(action("Pelote"), Action::Add(2));
        assert_eq!(action(&t.add_random), Action::AddRandom);
        assert_eq!(action(&t.goodbye_all), Action::RemoveAll);
        assert_eq!(action(&t.quit), Action::Quit);
        assert_eq!(action(&t.about), Action::About);
        assert_eq!(GlobalChoice::from_id(999), None);
    }

    fn texts() -> Texts {
        toml::from_str(include_str!("../locales/fr.toml")).unwrap()
    }

    #[test]
    fn a_big_family_is_split_by_region_and_a_small_one_stays_whole() {
        // Douze moutons de trois régions (une inconnue, un sans région), et
        // deux chats : les moutons par régions, les connues d'abord dans
        // leur ordre, puis les autres, puis « Ailleurs » ; les chats ensemble.
        let regions = [
            Some("europe"),
            Some("france"),
            Some("patagonie"),
            Some("france"),
            None,
            Some("british-isles"),
            Some("france"),
            Some("europe"),
            Some("france"),
            Some("british-isles"),
            Some("asia-africa"),
            Some("france"),
        ];
        let mut kinds: Vec<(&str, Option<&str>)> = regions.iter().map(|r| ("sheep", *r)).collect();
        kinds.insert(3, ("cats", Some("france")));
        kinds.push(("cats", None));
        let sections = sections(&kinds);
        let shown: Vec<(&str, Option<&str>, &[usize])> = sections
            .iter()
            .map(|s| (s.family.as_str(), s.region.as_deref(), s.pets.as_slice()))
            .collect();
        assert_eq!(
            shown,
            [
                ("sheep", Some("france"), &[1, 4, 7, 9, 12][..]),
                ("sheep", Some("british-isles"), &[6, 10][..]),
                ("sheep", Some("europe"), &[0, 8][..]),
                ("sheep", Some("asia-africa"), &[11][..]),
                ("sheep", Some("patagonie"), &[2][..]),
                ("sheep", Some(""), &[5][..]),
                ("cats", None, &[3, 13][..]),
            ]
        );
        // Chaque espèce est dans une section, une seule fois.
        let mut all: Vec<usize> = sections.iter().flat_map(|s| s.pets.clone()).collect();
        all.sort_unstable();
        assert_eq!(all, (0..kinds.len()).collect::<Vec<_>>());
        let t = texts();
        assert_eq!(section_name(&t, &sections[0], true), "Moutons · France");
        assert_eq!(section_name(&t, &sections[1], false), "Îles Britanniques");
        assert_eq!(section_name(&t, &sections[4], false), "Patagonie");
        assert_eq!(section_name(&t, &sections[5], false), "Ailleurs");
        assert_eq!(section_name(&t, &sections[6], true), "Chats");

        // Peu d'espèces, ou toutes de la même région : la famille reste entière.
        let few: Vec<(&str, Option<&str>)> =
            regions[..SPLIT - 1].iter().map(|r| ("sheep", *r)).collect();
        assert_eq!(super::sections(&few).len(), 1);
        let same = vec![("sheep", Some("france")); SPLIT + 3];
        let whole = super::sections(&same);
        assert_eq!(whole.len(), 1);
        assert_eq!(whole[0].region, None);
    }

    #[test]
    fn the_breeds_of_the_repository_are_found_by_region() {
        // Les races du dépôt disent toutes d'où elles viennent, dans les
        // régions que le menu sait nommer.
        let pets = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../pets");
        let mut breeds = Vec::new();
        for entry in std::fs::read_dir(&pets).unwrap() {
            let file = entry.unwrap().path().join("pet.toml");
            if file.is_file() {
                breeds.push(mouflon_core::load_pet(&file).unwrap());
            }
        }
        breeds.sort_by(|a, b| a.meta.name.cmp(&b.meta.name));
        let kinds: Vec<(&str, Option<&str>)> = breeds
            .iter()
            .map(|def| (def.meta.category(), def.meta.region()))
            .collect();
        for (def, (_, region)) in breeds.iter().zip(&kinds) {
            let region = region.unwrap_or_else(|| panic!("{} : sans région", def.meta.id));
            assert!(mouflon_core::REGIONS.contains(&region), "{}", def.meta.id);
        }
        let sections = sections(&kinds);
        let sizes: Vec<(&str, usize)> = sections
            .iter()
            .map(|s| (s.region.as_deref().unwrap(), s.pets.len()))
            .collect();
        assert_eq!(
            sizes,
            [
                ("france", 10),
                ("british-isles", 7),
                ("europe", 9),
                ("asia-africa", 3)
            ]
        );
        // Le floucat est avec les races de France, après son Mérinos d'Arles.
        let name = |i: usize| breeds[i].meta.name.as_str();
        let france: Vec<&str> = sections[0].pets.iter().map(|&i| name(i)).collect();
        assert!(france.contains(&"Floucat") && france.contains(&"Mérinos d'Arles"));
    }

    #[test]
    fn what_is_asked_of_a_new_animal_says_its_sex() {
        assert_eq!(Wanted::default(), Wanted::Any);
        assert_eq!(Wanted::ALL[0], Wanted::Any);
        assert_eq!(Wanted::Any.sex(), None);
        assert_eq!(Wanted::Ram.sex(), Some(mouflon_core::Sex::Male));
        assert_eq!(Wanted::Ewe.sex(), Some(mouflon_core::Sex::Female));
        assert_eq!(Wanted::Lamb.sex(), None);
    }

    #[test]
    fn species_are_grouped_by_family_known_ones_first() {
        let groups = groups(&["others", "sheep", "cats", "sheep", "poissons", "dragons"]);
        let order: Vec<(&str, &[usize])> = groups
            .iter()
            .map(|(c, pets)| (c.as_str(), pets.as_slice()))
            .collect();
        assert_eq!(
            order,
            [
                ("sheep", &[1, 3][..]),
                ("cats", &[2][..]),
                ("others", &[0][..]),
                ("dragons", &[5][..]),
                ("poissons", &[4][..]),
            ]
        );
        let t = texts();
        assert_eq!(category_name(&t, "sheep"), "Moutons");
        assert_eq!(category_name(&t, "poissons"), "Poissons");
    }

    #[test]
    fn a_label_carries_its_shortcut_on_the_right_of_a_windows_menu() {
        assert_eq!(keyed("Quitter", None), "Quitter");
        assert_eq!(
            keyed("Quitter", Some("Ctrl+Alt+Win+Q")),
            "Quitter\tCtrl+Alt+Win+Q"
        );
        // Les touches telles qu'on les lit sur le clavier de chaque langue.
        for (code, keys) in [("fr", "Ctrl+Alt+Win+"), ("de", "Strg+Alt+Win+")] {
            assert_eq!(crate::i18n::windows_keys(code), keys);
        }
    }

    #[test]
    fn a_species_shows_how_many_are_on_screen_from_two() {
        assert_eq!(counted("Southdown", 0), "Southdown");
        assert_eq!(counted("Southdown", 1), "Southdown");
        assert_eq!(counted("Southdown", 2), "Southdown (2)");
        assert_eq!(counted("Southdown", 12), "Southdown (12)");
    }

    #[test]
    fn every_choice_of_a_pet_menu_keeps_its_meaning_through_its_id() {
        let choices = [
            PetChoice::Goodbye,
            PetChoice::AddSame,
            PetChoice::AddLamb,
            PetChoice::Settings,
            PetChoice::Sounds(false),
            PetChoice::Sounds(true),
            PetChoice::Pause(false),
            PetChoice::Pause(true),
            PetChoice::Size(0),
            PetChoice::Size(3),
        ];
        for choice in choices {
            assert_eq!(PetChoice::from_id(choice.id()), Some(choice));
        }
        assert_eq!(PetChoice::from_id(0), None);
        assert_eq!(PetChoice::from_id(99), None);
    }

    /// Les cases du menu d'un animal montrent l'état de l'app, et chaque
    /// réglage y donne la valeur contraire de celle qu'on voit.
    #[test]
    fn a_pet_menu_shows_the_state_and_offers_its_opposite() {
        let t = texts();
        for paused in [false, true] {
            for sounds in [false, true] {
                for size in 0..4 {
                    let state = PetMenuState {
                        is_ewe: false,
                        can_have_lamb: false,
                        paused,
                        sounds,
                        size,
                    };
                    let items = pet_menu("Mérinos d'Arles", state, &t);
                    assert_eq!(items[0], PopupItem::Header("Mérinos d'Arles".into()));
                    let mut seen = Vec::new();
                    let mut collect = |item: &PopupItem| {
                        if let PopupItem::Item {
                            id, label, checked, ..
                        } = item
                        {
                            seen.push((PetChoice::from_id(*id).unwrap(), label.clone(), *checked));
                        }
                    };
                    for item in &items {
                        match item {
                            PopupItem::Submenu { items, .. } => items.iter().for_each(&mut collect),
                            other => collect(other),
                        }
                    }
                    let find = |wanted: fn(&PetChoice) -> bool| {
                        seen.iter().find(|(c, ..)| wanted(c)).unwrap().clone()
                    };
                    let (choice, _, checked) = find(|c| matches!(c, PetChoice::Sounds(_)));
                    assert_eq!((choice, checked), (PetChoice::Sounds(!sounds), sounds));
                    let (choice, label, _) = find(|c| matches!(c, PetChoice::Pause(_)));
                    assert_eq!(choice, PetChoice::Pause(!paused));
                    assert_eq!(
                        label,
                        if paused {
                            "Reprendre"
                        } else {
                            "Mettre en pause"
                        }
                    );
                    let checked: Vec<usize> = seen
                        .iter()
                        .filter_map(|(c, _, on)| match c {
                            PetChoice::Size(i) if *on => Some(*i),
                            _ => None,
                        })
                        .collect();
                    assert_eq!(checked, vec![size], "une seule taille cochée");
                    assert!(
                        seen.iter()
                            .any(|(_, l, _)| l == "Dire au revoir à Mérinos d'Arles")
                    );
                }
            }
        }
    }

    #[test]
    fn only_an_adult_ewe_offers_to_add_her_lamb() {
        for is_ewe in [false, true] {
            let items = pet_menu(
                "Bleu du Maine",
                PetMenuState {
                    is_ewe,
                    can_have_lamb: is_ewe,
                    paused: false,
                    sounds: true,
                    size: 1,
                },
                &texts(),
            );
            let offered = items.iter().any(|item| matches!(item, PopupItem::Item { id, label, enabled: true, .. } if *id == PetChoice::AddLamb.id() && label == "Ajouter son agneau"));
            assert_eq!(offered, is_ewe);
        }
        let waiting = pet_menu(
            "Brebis",
            PetMenuState {
                is_ewe: true,
                can_have_lamb: false,
                paused: false,
                sounds: true,
                size: 1,
            },
            &texts(),
        );
        assert!(waiting.iter().any(|item| matches!(item, PopupItem::Item { id, enabled: false, .. } if *id == PetChoice::AddLamb.id())));
        assert_eq!(
            PetChoice::AddLamb.action(42, None),
            Some(Action::AddLamb(42))
        );
    }

    #[test]
    fn a_pet_menu_acts_on_that_pet_and_its_species() {
        assert_eq!(
            PetChoice::Goodbye.action(7, Some(2)),
            Some(Action::Goodbye(7))
        );
        assert_eq!(PetChoice::AddSame.action(7, Some(2)), Some(Action::Add(2)));
        // L'espèce n'est plus dans la bibliothèque : rien à ajouter.
        assert_eq!(PetChoice::AddSame.action(7, None), None);
        // Sa taille à lui seul.
        assert_eq!(
            PetChoice::Size(3).action(7, None),
            Some(Action::SetPetSize(7, 3))
        );
        assert_eq!(
            PetChoice::Settings.action(7, None),
            Some(Action::OpenSettings)
        );
    }
}
