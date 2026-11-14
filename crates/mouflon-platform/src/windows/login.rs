//! Ouvrir Mouflon à l'ouverture de session : la liste des programmes de
//! démarrage de l'utilisateur, dans le registre de Windows.
//!
//! Windows garde à part ce que l'utilisateur a coupé lui-même (dans le
//! Gestionnaire des tâches ou dans Paramètres → Applications → Démarrage) :
//! Mouflon le respecte, et le dit dans ses réglages plutôt que de passer
//! outre.

use std::ffi::c_void;

use windows_sys::Win32::Foundation::{ERROR_FILE_NOT_FOUND, ERROR_SUCCESS};
use windows_sys::Win32::System::Registry::{
    HKEY_CURRENT_USER, REG_SZ, RRF_RT_REG_BINARY, RRF_RT_REG_SZ, RegDeleteKeyValueW, RegGetValueW,
    RegSetKeyValueW,
};

use super::{from_wide, wide};
use crate::LoginItem;

/// Où Windows note les programmes à lancer à l'ouverture de session.
const RUN_KEY: &str = r"Software\Microsoft\Windows\CurrentVersion\Run";
/// Où il note ceux que l'utilisateur a coupés ou remis lui-même.
const APPROVED_KEY: &str =
    r"Software\Microsoft\Windows\CurrentVersion\Explorer\StartupApproved\Run";
const RUN_NAME: &str = "Mouflon";

/// La commande qui lance ce programme-ci.
fn command() -> Option<String> {
    let exe = std::env::current_exe().ok()?;
    Some(format!("\"{}\"", exe.display()))
}

/// Ce que dit le registre : la commande notée (aucune : `Ok(None)`), ou
/// l'erreur de Windows.
fn noted() -> Result<Option<String>, u32> {
    let (key, name) = (wide(RUN_KEY), wide(RUN_NAME));
    let mut buffer = [0u16; 1024];
    let mut size = size_of_val(&buffer) as u32;
    // SAFETY: les deux textes vivent jusqu'au retour ; `buffer` est à nous,
    // donné avec sa taille en octets ; Windows n'y écrit qu'un texte terminé
    // par zéro (`RRF_RT_REG_SZ`).
    let status = unsafe {
        RegGetValueW(
            HKEY_CURRENT_USER,
            key.as_ptr(),
            name.as_ptr(),
            RRF_RT_REG_SZ,
            std::ptr::null_mut(),
            buffer.as_mut_ptr().cast::<c_void>(),
            &mut size,
        )
    };
    match status {
        ERROR_SUCCESS => Ok(Some(from_wide(&buffer))),
        ERROR_FILE_NOT_FOUND => Ok(None),
        other => Err(other),
    }
}

/// L'utilisateur a-t-il coupé Mouflon dans les programmes de démarrage de
/// Windows ? (Douze octets : le premier est impair quand c'est coupé.)
fn refused() -> bool {
    let (key, name) = (wide(APPROVED_KEY), wide(RUN_NAME));
    let mut data = [0u8; 16];
    let mut size = data.len() as u32;
    // SAFETY: comme `noted` ; `data` reçoit au plus sa taille d'octets.
    let status = unsafe {
        RegGetValueW(
            HKEY_CURRENT_USER,
            key.as_ptr(),
            name.as_ptr(),
            RRF_RT_REG_BINARY,
            std::ptr::null_mut(),
            data.as_mut_ptr().cast::<c_void>(),
            &mut size,
        )
    };
    status == ERROR_SUCCESS && size >= 1 && data[0] & 1 == 1
}

/// Note la commande de ce programme (ou l'efface).
fn note(command: Option<&str>) -> bool {
    let (key, name) = (wide(RUN_KEY), wide(RUN_NAME));
    // SAFETY: les textes vivent jusqu'au retour ; la taille donnée est celle
    // de la commande en octets, zéro final compris.
    let status = unsafe {
        match command {
            Some(command) => {
                let command = wide(command);
                RegSetKeyValueW(
                    HKEY_CURRENT_USER,
                    key.as_ptr(),
                    name.as_ptr(),
                    REG_SZ,
                    command.as_ptr().cast::<c_void>(),
                    (command.len() * 2) as u32,
                )
            }
            None => RegDeleteKeyValueW(HKEY_CURRENT_USER, key.as_ptr(), name.as_ptr()),
        }
    };
    status == ERROR_SUCCESS || (command.is_none() && status == ERROR_FILE_NOT_FOUND)
}

#[must_use]
pub fn login_item_available() -> bool {
    command().is_some()
}

/// Mouflon est-il lancé à l'ouverture de session ? S'il l'est mais que son
/// dossier a changé de place depuis, la commande notée est corrigée au
/// passage : il s'ouvrira d'où il est maintenant.
#[must_use]
pub fn login_item() -> LoginItem {
    let Some(command) = command() else {
        return LoginItem::Unavailable;
    };
    match noted() {
        Ok(None) => LoginItem::Off,
        Ok(Some(found)) => {
            if !found.eq_ignore_ascii_case(&command) && note(Some(&command)) {
                log::info!("ouverture de session : Mouflon a changé de dossier, c'est noté");
            }
            if refused() {
                LoginItem::NeedsApproval
            } else {
                LoginItem::On
            }
        }
        Err(e) => {
            log::warn!("programmes de démarrage illisibles (erreur {e})");
            LoginItem::Unavailable
        }
    }
}

/// Lance (ou non) Mouflon à l'ouverture de session.
pub fn set_login_item(on: bool) -> LoginItem {
    let Some(command) = command() else {
        return LoginItem::Unavailable;
    };
    if !note(on.then_some(command.as_str())) {
        log::warn!("programmes de démarrage : rien n'a pu être noté");
    }
    login_item()
}

/// Les applications de démarrage, dans les Paramètres de Windows.
pub fn open_login_items_settings() {
    super::ui::open("ms-settings:startupapps");
}
