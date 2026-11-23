//! Les menus surgissants (le menu d'un animal, au clic droit) : de vrais
//! menus Win32, qui prennent d'eux-mêmes le style du système.
//!
//! Un menu ouvert retient le fil qui le montre. Il n'est donc jamais ouvert
//! pendant que l'app traite un événement : [`pop_up_menu`] le prépare, et la
//! fenêtre cachée le montre à son tour de boucle ([`show_pending`]). Les
//! animaux, eux, continuent de bouger (ils sont sur leur propre fil).

use std::cell::RefCell;
use std::mem::ManuallyDrop;

use windows_sys::Win32::Foundation::{HWND, POINT};
use windows_sys::Win32::UI::WindowsAndMessaging::{
    AppendMenuW, CreatePopupMenu, DestroyMenu, GetCursorPos, HMENU, MF_CHECKED, MF_DISABLED,
    MF_GRAYED, MF_POPUP, MF_SEPARATOR, MF_STRING, PostMessageW, SetForegroundWindow, TPM_NONOTIFY,
    TPM_RETURNCMD, TPM_RIGHTBUTTON, TrackPopupMenu, WM_NULL,
};

use super::host::{WM_APP_POPUP, emit, owner};
use super::wide;
use crate::{PopupItem, UiEvent};

/// Un menu Win32 en construction, détruit avec ses sous-menus.
struct Menu(HMENU);

impl Menu {
    fn new() -> Option<Self> {
        // SAFETY: sans argument ; rend un menu vide, ou rien.
        let menu = unsafe { CreatePopupMenu() };
        (!menu.is_null()).then_some(Self(menu))
    }

    fn item(&self, id: usize, label: &str, checked: bool, enabled: bool) {
        let label = wide(label);
        let mut flags = MF_STRING;
        if checked {
            flags |= MF_CHECKED;
        }
        if !enabled {
            flags |= MF_GRAYED | MF_DISABLED;
        }
        // SAFETY: le menu est vivant ; le texte vit jusqu'au retour (Windows
        // le copie).
        unsafe {
            AppendMenuW(self.0, flags, id, label.as_ptr());
        }
    }

    fn separator(&self) {
        // SAFETY: le menu est vivant ; un séparateur n'a ni texte ni numéro.
        unsafe {
            AppendMenuW(self.0, MF_SEPARATOR, 0, std::ptr::null());
        }
    }

    /// Ajoute `sub` comme sous-menu : il appartient désormais à ce menu, qui
    /// le détruira avec lui.
    fn submenu(&self, label: &str, sub: Self) {
        let label = wide(label);
        // SAFETY: les deux menus sont vivants ; Windows prend possession du
        // sous-menu, qu'on ne détruit donc pas nous-mêmes (`forget`).
        unsafe {
            AppendMenuW(self.0, MF_POPUP, sub.0 as usize, label.as_ptr());
        }
        std::mem::forget(sub);
    }

    /// Montre le menu sous le curseur et attend : le numéro de l'article
    /// choisi, ou 0.
    fn show(&self, owner: HWND) -> usize {
        let mut at = POINT { x: 0, y: 0 };
        // SAFETY: `owner` est notre fenêtre cachée, sur ce fil ; `at` est à
        // nous. La fenêtre passe devant le temps du menu (sinon il ne se
        // refermerait pas quand on clique ailleurs), et un message vide le
        // lui fait oublier ensuite, comme le veut l'usage.
        unsafe {
            GetCursorPos(&mut at);
            SetForegroundWindow(owner);
            let chosen = TrackPopupMenu(
                self.0,
                TPM_RETURNCMD | TPM_NONOTIFY | TPM_RIGHTBUTTON,
                at.x,
                at.y,
                0,
                owner,
                std::ptr::null(),
            );
            PostMessageW(owner, WM_NULL, 0, 0);
            chosen.max(0) as usize
        }
    }
}

impl Drop for Menu {
    fn drop(&mut self) {
        // SAFETY: le menu est à nous, et n'est le sous-menu de personne
        // (ceux-là sont oubliés dans `submenu`).
        unsafe {
            DestroyMenu(self.0);
        }
    }
}

/// Construit un menu surgissant ; les numéros des articles sont décalés de
/// un (0 veut dire « rien de choisi »).
fn popup(items: &[PopupItem]) -> Option<Menu> {
    let menu = Menu::new()?;
    for item in items {
        match item {
            PopupItem::Header(text) => menu.item(0, text, false, false),
            PopupItem::Item {
                id,
                label,
                checked,
                enabled,
            } => menu.item(*id as usize + 1, label, *checked, *enabled),
            PopupItem::Submenu { label, items } => {
                if let Some(sub) = popup(items) {
                    menu.submenu(label, sub);
                }
            }
            PopupItem::Separator => menu.separator(),
        }
    }
    Some(menu)
}

thread_local! {
    /// Le menu qui attend d'être montré (le dernier demandé).
    static PENDING: ManuallyDrop<RefCell<Option<Menu>>> =
        const { ManuallyDrop::new(RefCell::new(None)) };
}

/// Le menu d'un animal, sous le curseur : il s'ouvre au prochain tour de la
/// boucle du fil principal ; le choix part ensuite à l'app, puis
/// [`UiEvent::MenuClosed`].
pub fn pop_up_menu(items: &[PopupItem]) {
    let posted = match (popup(items), owner()) {
        (Some(menu), Some(owner)) => {
            PENDING.with(|pending| *pending.borrow_mut() = Some(menu));
            // SAFETY: notre fenêtre cachée, un message sans pointeur.
            unsafe { PostMessageW(owner, WM_APP_POPUP, 0, 0) != 0 }
        }
        _ => false,
    };
    if !posted {
        PENDING.with(|pending| *pending.borrow_mut() = None);
        // Pas de menu : l'animal qui l'attendait repart.
        emit(UiEvent::MenuClosed);
    }
}

/// Montre le menu préparé par [`pop_up_menu`] (appelé par la fenêtre
/// cachée). Le fil attend pendant qu'il est ouvert.
pub(super) fn show_pending() {
    let Some(menu) = PENDING.with(|pending| pending.borrow_mut().take()) else {
        return;
    };
    let chosen = owner().map_or(0, |owner| menu.show(owner));
    drop(menu);
    if chosen > 0 {
        emit(UiEvent::Menu(chosen as u32 - 1));
    }
    emit(UiEvent::MenuClosed);
}
