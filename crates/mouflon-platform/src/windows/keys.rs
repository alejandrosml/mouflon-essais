//! Les raccourcis clavier de Mouflon dans toutes les apps (Ctrl+Alt+Win et
//! une lettre), et la touche Échap entendue partout.
//!
//! Les raccourcis sont réservés auprès de Windows (`RegisterHotKey`), qui
//! prévient la fenêtre cachée du fil principal : aucune autre touche ne
//! passe par Mouflon. La lettre compte, pas la place de la touche : sur un
//! clavier AZERTY comme sur un QWERTY, Ctrl+Alt+Win+Q est la touche marquée Q.
//!
//! Pour Échap (une option, coupée d'origine), Mouflon demande à Windows de
//! lui signaler les touches du clavier même quand il n'est pas au premier
//! plan (« raw input »), et ne regarde que celle-là : rien d'autre n'est lu,
//! gardé ni transmis.

use std::cell::{Cell, RefCell};
use std::ffi::c_void;

use windows_sys::Win32::Foundation::{LPARAM, WPARAM};
use windows_sys::Win32::UI::Input::KeyboardAndMouse::{
    MOD_ALT, MOD_CONTROL, MOD_NOREPEAT, MOD_WIN, RegisterHotKey, UnregisterHotKey, VK_ESCAPE,
};
use windows_sys::Win32::UI::Input::{
    GetRawInputData, HRAWINPUT, RAWINPUT, RAWINPUTDEVICE, RAWINPUTHEADER, RID_INPUT,
    RIDEV_INPUTSINK, RIDEV_REMOVE, RIM_TYPEKEYBOARD, RegisterRawInputDevices,
};
use windows_sys::Win32::UI::WindowsAndMessaging::RI_KEY_BREAK;

use super::host::{emit, owner};
use crate::{KeyMove, Shortcut, UiEvent};

thread_local! {
    /// Les raccourcis réservés en ce moment (leurs identifiants).
    static HELD: RefCell<Vec<u32>> = const { RefCell::new(Vec::new()) };
    /// Échap est enfoncée (pour distinguer l'appui de sa répétition).
    static ESCAPE_DOWN: Cell<bool> = const { Cell::new(false) };
}

/// Réserve ces raccourcis (et rend ceux d'avant) : Ctrl+Alt+Win et la lettre
/// de chacun. Renvoie ceux qui n'ont pas pu l'être (déjà pris par une autre
/// app).
#[must_use]
pub fn set_shortcuts(shortcuts: &[Shortcut]) -> Vec<u32> {
    let Some(owner) = owner() else {
        return shortcuts.iter().map(|s| s.id).collect();
    };
    HELD.with_borrow_mut(|held| {
        for id in held.drain(..) {
            // SAFETY: notre fenêtre cachée, sur ce fil, et un raccourci
            // qu'elle a réservé.
            unsafe {
                UnregisterHotKey(owner, id as i32);
            }
        }
        let mut missed = Vec::new();
        for shortcut in shortcuts {
            let letter = shortcut.key.to_ascii_uppercase();
            // Le code d'une touche-lettre est sa majuscule : il suit la
            // disposition du clavier.
            let taken = letter.is_ascii_uppercase()
                // SAFETY: notre fenêtre cachée, sur ce fil ; des constantes.
                && unsafe {
                    RegisterHotKey(
                        owner,
                        shortcut.id as i32,
                        MOD_CONTROL | MOD_ALT | MOD_WIN | MOD_NOREPEAT,
                        letter as u32,
                    )
                } != 0;
            if taken {
                held.push(shortcut.id);
            } else {
                missed.push(shortcut.id);
            }
        }
        missed
    })
}

/// Un raccourci réservé vient d'être frappé (la fenêtre cachée le dit).
pub(super) fn hotkey(id: WPARAM) {
    if let Ok(id) = u32::try_from(id) {
        emit(UiEvent::Shortcut(id));
    }
}

/// Entend la touche Échap dans toutes les apps (`on`), ou plus. `true` si
/// elle est entendue partout.
pub fn listen_escape(on: bool) -> bool {
    let Some(owner) = owner() else {
        return false;
    };
    // Le clavier (page 1, usage 6) : signalé à notre fenêtre même quand elle
    // n'est pas au premier plan, ou plus signalé du tout.
    let keyboard = RAWINPUTDEVICE {
        usUsagePage: 1,
        usUsage: 6,
        dwFlags: if on { RIDEV_INPUTSINK } else { RIDEV_REMOVE },
        hwndTarget: if on { owner } else { std::ptr::null_mut() },
    };
    // SAFETY: une seule description, donnée avec sa taille ; la fenêtre est
    // la nôtre, sur ce fil.
    let done =
        unsafe { RegisterRawInputDevices(&keyboard, 1, size_of::<RAWINPUTDEVICE>() as u32) != 0 };
    ESCAPE_DOWN.set(false);
    on && done
}

/// Une touche du clavier a bougé quelque part (la fenêtre cachée le dit) :
/// seule Échap est regardée.
pub(super) fn raw_input(handle: LPARAM) {
    let mut input = RAWINPUT::default();
    let mut size = size_of::<RAWINPUT>() as u32;
    // SAFETY: `handle` vient du message que Windows nous envoie à
    // l'instant ; `input` est à nous, donné avec sa taille (celle d'une
    // entrée de clavier ou de souris, qui y tient toujours).
    let read = unsafe {
        GetRawInputData(
            handle as HRAWINPUT,
            RID_INPUT,
            (&raw mut input).cast::<c_void>(),
            &mut size,
            size_of::<RAWINPUTHEADER>() as u32,
        )
    };
    if read == u32::MAX || read == 0 || input.header.dwType != RIM_TYPEKEYBOARD {
        return;
    }
    // SAFETY: l'en-tête dit que l'entrée vient d'un clavier : c'est cette
    // forme de l'union que Windows a remplie.
    let key = unsafe { input.data.keyboard };
    if key.VKey != VK_ESCAPE {
        return;
    }
    let released = u32::from(key.Flags) & RI_KEY_BREAK != 0;
    let change = match (released, ESCAPE_DOWN.replace(!released)) {
        (true, _) => KeyMove::Up,
        (false, false) => KeyMove::Down,
        (false, true) => KeyMove::Repeat,
    };
    emit(UiEvent::Escape(change));
}
