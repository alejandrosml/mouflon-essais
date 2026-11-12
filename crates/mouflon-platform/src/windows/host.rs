//! La fenêtre cachée du fil principal. Elle ne se voit jamais, mais c'est à
//! elle que Windows s'adresse pour tout ce qui ne vient pas d'une fenêtre de
//! Mouflon : un raccourci clavier, la touche Échap, un second lancement du
//! programme, la session qu'on verrouille, l'écran qui s'éteint. Elle possède
//! aussi les menus surgissants et les fenêtres des écrans (qui restent ainsi
//! hors de la barre des tâches et d'Alt+Tab).

use std::cell::{Cell, RefCell};
use std::mem::ManuallyDrop;
use std::rc::Rc;
use std::sync::OnceLock;

use windows_sys::Win32::Foundation::{
    ERROR_ACCESS_DENIED, ERROR_ALREADY_EXISTS, GetLastError, HANDLE, HWND, LPARAM, LRESULT, WPARAM,
};
use windows_sys::Win32::System::LibraryLoader::GetModuleHandleW;
use windows_sys::Win32::System::Power::{POWERBROADCAST_SETTING, RegisterPowerSettingNotification};
use windows_sys::Win32::System::RemoteDesktop::{
    NOTIFY_FOR_THIS_SESSION, WTSRegisterSessionNotification,
};
use windows_sys::Win32::System::Threading::CreateMutexW;
use windows_sys::Win32::UI::WindowsAndMessaging::{
    ChangeWindowMessageFilterEx, CreateWindowExW, DefWindowProcW, FindWindowW, MSGFLT_ALLOW,
    PBT_POWERSETTINGCHANGE, PostMessageW, RegisterClassExW, RegisterWindowMessageW, WM_APP,
    WM_HOTKEY, WM_INPUT, WM_POWERBROADCAST, WM_WTSSESSION_CHANGE, WNDCLASSEXW, WS_EX_TOOLWINDOW,
    WS_POPUP, WTS_SESSION_LOCK, WTS_SESSION_UNLOCK,
};
use windows_sys::core::GUID;

use super::wide;
use crate::UiEvent;

/// La classe et le titre de la fenêtre cachée : un second lancement de
/// Mouflon la retrouve par eux.
const CLASS: &str = "MouflonHote";
const TITLE: &str = "Mouflon";
/// Un menu surgissant attend d'être montré (voir [`super::menu`]).
pub(super) const WM_APP_POPUP: u32 = WM_APP + 1;
/// « Écran allumé, éteint ou assombri » (`GUID_CONSOLE_DISPLAY_STATE`).
const DISPLAY_STATE: GUID = GUID {
    data1: 0x6fe6_9556,
    data2: 0x704a,
    data3: 0x47a0,
    data4: [0x8f, 0x24, 0xc2, 0x8d, 0x93, 0x6f, 0xda, 0x47],
};
/// Passé à `RegisterPowerSettingNotification` : les nouvelles vont à une
/// fenêtre (`DEVICE_NOTIFY_WINDOW_HANDLE`).
const TO_A_WINDOW: u32 = 0;

/// Qui reçoit ce que l'utilisateur fait dans les menus et les fenêtres de
/// Mouflon.
type Handler = Rc<dyn Fn(UiEvent)>;

thread_local! {
    static OWNER: Cell<Option<isize>> = const { Cell::new(None) };
    /// (Jamais défait : une fenêtre peut encore annoncer quelque chose
    /// pendant que le fil se défait, à la sortie du programme.)
    static HANDLER: ManuallyDrop<RefCell<Option<Handler>>> =
        const { ManuallyDrop::new(RefCell::new(None)) };
    /// Session verrouillée, écran éteint : personne ne voit les animaux.
    static LOCKED: Cell<bool> = const { Cell::new(false) };
    static SCREEN_OFF: Cell<bool> = const { Cell::new(false) };
}

/// Le message que s'envoient deux lancements de Mouflon (le même numéro
/// pour tous les programmes qui le demandent sous ce nom).
fn relaunch_message() -> u32 {
    static MESSAGE: OnceLock<u32> = OnceLock::new();
    *MESSAGE.get_or_init(|| {
        let name = wide("MouflonSecondLancement");
        // SAFETY: le nom vit jusqu'au retour ; Windows rend le numéro du
        // message.
        unsafe { RegisterWindowMessageW(name.as_ptr()) }
    })
}

/// Ce que dit Windows quand on lui demande le verrou de Mouflon : un autre
/// Mouflon le tient-il déjà ? `made` : la demande a rendu une poignée ;
/// `error` : ce qu'elle a laissé comme erreur. Le verrou existe déjà, ou
/// (pas de poignée) il existe mais nous est refusé : il est celui d'un
/// Mouflon lancé « en tant qu'administrateur ».
fn held_by_another(made: bool, error: u32) -> bool {
    if made {
        error == ERROR_ALREADY_EXISTS
    } else {
        error == ERROR_ACCESS_DENIED
    }
}

/// Le premier Mouflon lancé met un moment à ouvrir sa fenêtre cachée (il
/// charge d'abord ses animaux) : un second lancement qui arrive entre-temps
/// la cherche ce temps-là, par petits pas, avant de renoncer à le prévenir.
const FIRST_WAIT: std::time::Duration = std::time::Duration::from_secs(4);
const FIRST_STEP: std::time::Duration = std::time::Duration::from_millis(100);

/// Ce programme tourne-t-il déjà dans cette session ? Si oui, le premier
/// lancé est prévenu (il fait venir un animal : on voit qu'il est là), et
/// celui-ci n'a plus qu'à s'arrêter : il n'y a jamais deux Mouflon, deux
/// troupeaux, deux icônes près de l'horloge. Sinon, celui-ci devient le
/// premier (jusqu'à sa fin).
#[must_use]
pub fn already_running() -> bool {
    let name = wide("Local\\Mouflon-une-seule-instance");
    // SAFETY: le nom vit jusqu'au retour ; pas d'attributs de sécurité. La
    // poignée du verrou n'est jamais refermée : il vit autant que le
    // processus, et Windows le défait à sa sortie. L'erreur est lue
    // aussitôt, avant tout autre appel.
    let (made, error) = unsafe {
        let mutex = CreateMutexW(std::ptr::null(), 0, name.as_ptr());
        (!mutex.is_null(), GetLastError())
    };
    if !held_by_another(made, error) {
        return false;
    }
    let (class, title) = (wide(CLASS), wide(TITLE));
    let started = std::time::Instant::now();
    loop {
        // SAFETY: les deux textes vivent jusqu'au retour ; la fenêtre
        // trouvée (s'il y en a une) reçoit un message sans pointeur.
        let told = unsafe {
            let first = FindWindowW(class.as_ptr(), title.as_ptr());
            !first.is_null() && PostMessageW(first, relaunch_message(), 0, 0) != 0
        };
        if told || started.elapsed() >= FIRST_WAIT {
            break;
        }
        std::thread::sleep(FIRST_STEP);
    }
    true
}

/// La fenêtre cachée du fil principal (créée au premier appel) : `None` si
/// Windows la refuse.
#[must_use]
pub fn overlay_owner() -> Option<isize> {
    if let Some(owner) = OWNER.get() {
        return Some(owner);
    }
    let (class, title) = (wide(CLASS), wide(TITLE));
    // SAFETY: la classe est déclarée avec notre procédure de fenêtre, qui
    // vit autant que le programme ; les textes vivent jusqu'au retour ; pas
    // de parent, de menu ni de données. La fenêtre appartient à ce fil.
    let hwnd = unsafe {
        let instance = GetModuleHandleW(std::ptr::null());
        let declared = WNDCLASSEXW {
            cbSize: size_of::<WNDCLASSEXW>() as u32,
            style: 0,
            lpfnWndProc: Some(host_proc),
            cbClsExtra: 0,
            cbWndExtra: 0,
            hInstance: instance,
            hIcon: std::ptr::null_mut(),
            hCursor: std::ptr::null_mut(),
            hbrBackground: std::ptr::null_mut(),
            lpszMenuName: std::ptr::null(),
            lpszClassName: class.as_ptr(),
            hIconSm: std::ptr::null_mut(),
        };
        // (Déjà déclarée : la création de la fenêtre le dira.)
        RegisterClassExW(&declared);
        CreateWindowExW(
            WS_EX_TOOLWINDOW,
            class.as_ptr(),
            title.as_ptr(),
            WS_POPUP,
            0,
            0,
            0,
            0,
            std::ptr::null_mut(),
            std::ptr::null_mut(),
            instance,
            std::ptr::null(),
        )
    };
    if hwnd.is_null() {
        log::error!("fenêtre cachée de Mouflon refusée par Windows");
        return None;
    }
    // SAFETY: la fenêtre vient d'être créée sur ce fil ; les deux
    // inscriptions ne font que demander des messages (elles sont défaites
    // avec la fenêtre, à la sortie du processus). Le message d'un second
    // lancement est accepté même d'un programme aux droits moindres (ce
    // Mouflon-ci lancé « en tant qu'administrateur », le second d'un
    // double-clic) : il ne porte rien, il fait seulement venir un animal.
    unsafe {
        ChangeWindowMessageFilterEx(hwnd, relaunch_message(), MSGFLT_ALLOW, std::ptr::null_mut());
        if WTSRegisterSessionNotification(hwnd, NOTIFY_FOR_THIS_SESSION) == 0 {
            log::debug!("verrouillage de la session non signalé ici");
        }
        if RegisterPowerSettingNotification(hwnd as HANDLE, &DISPLAY_STATE, TO_A_WINDOW) == 0 {
            log::debug!("extinction de l'écran non signalée ici");
        }
    }
    OWNER.set(Some(hwnd as isize));
    Some(hwnd as isize)
}

/// La fenêtre cachée, si elle existe déjà ou peut être créée.
pub(super) fn owner() -> Option<HWND> {
    overlay_owner().map(|owner| owner as HWND)
}

/// Qui reçoit ce que l'utilisateur fait dans les menus et les fenêtres de
/// Mouflon.
pub fn set_ui_handler(handler: impl Fn(UiEvent) + 'static) {
    HANDLER.with(|slot| *slot.borrow_mut() = Some(Rc::new(handler)));
}

pub(super) fn emit(event: UiEvent) {
    let handler = HANDLER.with(|slot| slot.borrow().clone());
    if let Some(handler) = handler {
        handler(event);
    }
}

/// Personne ne voit les animaux (session verrouillée, écran éteint) : l'horloge
/// de l'écran se tait, et l'affichage ralentit de lui-même.
fn nobody_watching() {
    super::clock::set_asleep(LOCKED.get() || SCREEN_OFF.get());
}

/// Ce que Windows dit à la fenêtre cachée.
unsafe extern "system" fn host_proc(
    hwnd: HWND,
    message: u32,
    wparam: WPARAM,
    lparam: LPARAM,
) -> LRESULT {
    match message {
        WM_HOTKEY => super::keys::hotkey(wparam),
        WM_INPUT => super::keys::raw_input(lparam),
        WM_APP_POPUP => super::menu::show_pending(),
        WM_WTSSESSION_CHANGE => {
            match wparam as u32 {
                WTS_SESSION_LOCK => LOCKED.set(true),
                WTS_SESSION_UNLOCK => LOCKED.set(false),
                _ => {}
            }
            nobody_watching();
        }
        WM_POWERBROADCAST if wparam as u32 == PBT_POWERSETTINGCHANGE && lparam != 0 => {
            // SAFETY: pour ce message, `lparam` est un pointeur vers un
            // réglage d'alimentation, valide le temps de l'appel ; on n'en
            // lit que l'en-tête et le premier octet de données.
            let (setting, length, state) = unsafe {
                let setting = &*(lparam as *const POWERBROADCAST_SETTING);
                (setting.PowerSetting, setting.DataLength, setting.Data[0])
            };
            let same = setting.data1 == DISPLAY_STATE.data1
                && setting.data2 == DISPLAY_STATE.data2
                && setting.data3 == DISPLAY_STATE.data3
                && setting.data4 == DISPLAY_STATE.data4;
            if same && length >= 1 {
                // 0 : éteint ; 1 : allumé ; 2 : assombri.
                SCREEN_OFF.set(state == 0);
                nobody_watching();
            }
        }
        other if other == relaunch_message() => emit(UiEvent::Relaunched),
        _ => {}
    }
    // SAFETY: les arguments sont ceux que Windows vient de nous donner.
    unsafe { DefWindowProcW(hwnd, message, wparam, lparam) }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_second_launch_knows_the_first_even_when_it_cannot_open_its_lock() {
        // Le verrou vient d'être créé : personne d'autre.
        assert!(!held_by_another(true, 0));
        // Il existait déjà : un autre Mouflon tourne.
        assert!(held_by_another(true, ERROR_ALREADY_EXISTS));
        // Il existe, mais nous est refusé : celui d'un Mouflon lancé « en
        // tant qu'administrateur ».
        assert!(held_by_another(false, ERROR_ACCESS_DENIED));
        // Windows n'a pas pu le créer, pour une autre raison : on se lance
        // quand même (mieux vaut deux troupeaux qu'aucun).
        assert!(!held_by_another(false, 8));
    }

    #[test]
    fn a_second_launch_is_seen_by_the_first() {
        // Le premier lancement prend le verrou ; le second le trouve pris.
        // (Dans un même processus, la seconde demande rend une autre
        // poignée du même verrou, avec « existe déjà ».)
        let name = wide("Local\\Mouflon-essai-une-seule-instance");
        // SAFETY: le nom vit jusqu'au retour ; pas d'attributs de sécurité ;
        // les poignées sont laissées au processus de l'essai.
        let (first, second) = unsafe {
            let first = CreateMutexW(std::ptr::null(), 0, name.as_ptr());
            let first = (!first.is_null(), GetLastError());
            let second = CreateMutexW(std::ptr::null(), 0, name.as_ptr());
            (first, (!second.is_null(), GetLastError()))
        };
        assert!(!held_by_another(first.0, first.1));
        assert!(held_by_another(second.0, second.1));
    }
}
