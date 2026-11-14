//! Les raccourcis clavier de Mouflon, dans toutes les apps.
//!
//! - ⌃⌥⌘ et une lettre : des raccourcis réservés à Mouflon auprès de macOS
//!   (les « hot keys » de Carbon, la façon qu'a macOS d'en réserver). Pas
//!   d'autorisation à demander, et l'app au premier plan ne les reçoit pas.
//!   La touche se cherche d'après sa lettre, sur la disposition du clavier
//!   (la touche marquée A d'un clavier AZERTY est celle du Q d'un QWERTY) ;
//!   les raccourcis sont réservés de nouveau quand on en change.
//! - Échap : elle n'est pas réservée (toutes les apps en ont besoin).
//!   Mouflon l'écoute seulement, avec l'autorisation d'Accessibilité, sans
//!   rien garder ni faire des autres touches.

use std::cell::RefCell;
use std::ffi::c_void;
use std::ptr::NonNull;

use block2::RcBlock;
use objc2::rc::Retained;
use objc2::runtime::AnyObject;
use objc2::{MainThreadMarker, sel};
use objc2_app_kit::{NSEvent, NSEventMask, NSEventType};
use objc2_core_foundation::{CFData, CFRetained, CFType};
use objc2_foundation::{NSDistributedNotificationCenter, NSString};

use super::actions::{actions, emit};
use crate::{KeyMove, Shortcut, UiEvent};

type OsStatus = i32;
const NO_ERR: OsStatus = 0;

/// Quatre lettres en un nombre, comme les constantes de Carbon.
const fn four(code: &[u8; 4]) -> u32 {
    u32::from_be_bytes(*code)
}

/// Les raccourcis de Mouflon, parmi ceux de toutes les apps.
const SIGNATURE: u32 = four(b"Mflo");
const EVENT_CLASS_KEYBOARD: u32 = four(b"keyb");
const EVENT_HOT_KEY_PRESSED: u32 = 5;
const EVENT_PARAM_DIRECT_OBJECT: u32 = four(b"----");
const TYPE_EVENT_HOT_KEY_ID: u32 = four(b"hkid");
/// ⌃⌥⌘ (`controlKey`, `optionKey`, `cmdKey`).
const MODIFIERS: u32 = 0x1000 | 0x0800 | 0x0100;
/// `kUCKeyActionDisplay`, `kUCKeyTranslateNoDeadKeysMask`.
const KEY_ACTION_DISPLAY: u16 = 3;
const NO_DEAD_KEYS: u32 = 1;
/// La touche Échap (`kVK_Escape`).
const ESCAPE: u16 = 0x35;

#[repr(C)]
#[derive(Clone, Copy)]
struct EventTypeSpec {
    event_class: u32,
    event_kind: u32,
}

#[repr(C)]
#[derive(Clone, Copy, Default)]
struct EventHotKeyId {
    signature: u32,
    id: u32,
}

type EventHandler = unsafe extern "C" fn(*mut c_void, *mut c_void, *mut c_void) -> OsStatus;

#[link(name = "Carbon", kind = "framework")]
unsafe extern "C" {
    fn GetApplicationEventTarget() -> *mut c_void;
    fn InstallEventHandler(
        target: *mut c_void,
        handler: EventHandler,
        count: usize,
        list: *const EventTypeSpec,
        user_data: *mut c_void,
        out_ref: *mut *mut c_void,
    ) -> OsStatus;
    fn RegisterEventHotKey(
        code: u32,
        modifiers: u32,
        id: EventHotKeyId,
        target: *mut c_void,
        options: u32,
        out_ref: *mut *mut c_void,
    ) -> OsStatus;
    fn UnregisterEventHotKey(hot_key: *mut c_void) -> OsStatus;
    fn GetEventParameter(
        event: *mut c_void,
        name: u32,
        desired_type: u32,
        actual_type: *mut u32,
        buffer_size: usize,
        actual_size: *mut usize,
        data: *mut c_void,
    ) -> OsStatus;
    fn TISCopyCurrentASCIICapableKeyboardLayoutInputSource() -> *mut c_void;
    fn TISGetInputSourceProperty(source: *mut c_void, key: *const c_void) -> *mut c_void;
    static kTISPropertyUnicodeKeyLayoutData: *const c_void;
    fn LMGetKbdType() -> u8;
    fn UCKeyTranslate(
        layout: *const c_void,
        code: u16,
        action: u16,
        modifiers: u32,
        keyboard: u32,
        options: u32,
        dead_keys: *mut u32,
        max_len: usize,
        len: *mut usize,
        chars: *mut u16,
    ) -> OsStatus;
}

#[derive(Default)]
struct Keys {
    /// Les raccourcis que l'app veut.
    wanted: Vec<Shortcut>,
    /// Ceux que macOS a réservés à Mouflon : identifiant, réservation.
    held: Vec<(u32, NonNull<c_void>)>,
    /// Ce qu'on fait des raccourcis est en place (une fois pour toutes).
    handler: bool,
    /// Mouflon suit les changements de disposition du clavier.
    watching_layout: bool,
    /// Les écoutes de la touche Échap : dans les autres apps, dans Mouflon.
    escape: Option<(Retained<AnyObject>, Retained<AnyObject>)>,
}

thread_local! {
    static KEYS: RefCell<Keys> = RefCell::new(Keys::default());
}

/// Réserve ces raccourcis à Mouflon, dans toutes les apps, et rend ceux
/// qu'il avait. Renvoie ceux qui n'ont pas pu l'être (une autre app les a
/// déjà, ou aucune touche ne donne leur lettre). À appeler sur le fil
/// principal.
pub fn set_shortcuts(shortcuts: &[Shortcut]) -> Vec<u32> {
    let Some(mtm) = MainThreadMarker::new() else {
        return shortcuts.iter().map(|s| s.id).collect();
    };
    KEYS.with(|keys| {
        let Ok(mut keys) = keys.try_borrow_mut() else {
            return shortcuts.iter().map(|s| s.id).collect();
        };
        keys.wanted = shortcuts.to_vec();
        if !keys.wanted.is_empty() {
            watch_layout(&mut keys, mtm);
        }
        register(&mut keys)
    })
}

/// La disposition du clavier a changé : les raccourcis sont réservés de
/// nouveau, sur leurs nouvelles touches.
pub(super) fn layout_changed() {
    KEYS.with(|keys| {
        if let Ok(mut keys) = keys.try_borrow_mut()
            && !keys.wanted.is_empty()
        {
            let missed = register(&mut keys);
            if !missed.is_empty() {
                log::info!("raccourcis non réservés après le changement de clavier : {missed:?}");
            }
        }
    });
}

fn register(keys: &mut Keys) -> Vec<u32> {
    for (_, held) in keys.held.drain(..) {
        // SAFETY: une réservation faite par RegisterEventHotKey, rendue une
        // seule fois (elle quitte la liste).
        unsafe { UnregisterEventHotKey(held.as_ptr()) };
    }
    if keys.wanted.is_empty() {
        return Vec::new();
    }
    install_handler(keys);
    let layout = Layout::current();
    // SAFETY: fonction sans argument de Carbon (fil principal).
    let target = unsafe { GetApplicationEventTarget() };
    let mut missed = Vec::new();
    for shortcut in &keys.wanted {
        let code = layout
            .as_ref()
            .and_then(|layout| layout.code_of(shortcut.key))
            .or_else(|| ansi_code(shortcut.key));
        let Some(code) = code else {
            missed.push(shortcut.id);
            continue;
        };
        let id = EventHotKeyId {
            signature: SIGNATURE,
            id: shortcut.id,
        };
        let mut held = std::ptr::null_mut();
        // SAFETY: `held` pointe vers une variable locale ; la cible est
        // celle de l'app, valide toute la vie du processus.
        let status =
            unsafe { RegisterEventHotKey(u32::from(code), MODIFIERS, id, target, 0, &mut held) };
        match NonNull::new(held) {
            Some(held) if status == NO_ERR => keys.held.push((shortcut.id, held)),
            _ => missed.push(shortcut.id),
        }
    }
    missed
}

/// Ce que Mouflon fait d'un raccourci : il le passe à l'app.
fn install_handler(keys: &mut Keys) {
    if keys.handler {
        return;
    }
    let spec = EventTypeSpec {
        event_class: EVENT_CLASS_KEYBOARD,
        event_kind: EVENT_HOT_KEY_PRESSED,
    };
    // SAFETY: `hot_key_pressed` a la signature d'un EventHandlerProcPtr ;
    // `spec` est lu pendant l'appel ; pas de données ni de référence gardée.
    let status = unsafe {
        InstallEventHandler(
            GetApplicationEventTarget(),
            hot_key_pressed,
            1,
            &spec,
            std::ptr::null_mut(),
            std::ptr::null_mut(),
        )
    };
    keys.handler = status == NO_ERR;
}

/// Un raccourci de Mouflon : son identifiant part à l'app.
unsafe extern "C" fn hot_key_pressed(
    _call: *mut c_void,
    event: *mut c_void,
    _data: *mut c_void,
) -> OsStatus {
    let mut id = EventHotKeyId::default();
    // SAFETY: `event` est l'événement que Carbon nous passe ; `id` a la
    // taille et la forme d'un EventHotKeyID.
    let status = unsafe {
        GetEventParameter(
            event,
            EVENT_PARAM_DIRECT_OBJECT,
            TYPE_EVENT_HOT_KEY_ID,
            std::ptr::null_mut(),
            std::mem::size_of::<EventHotKeyId>(),
            std::ptr::null_mut(),
            (&raw mut id).cast(),
        )
    };
    if status == NO_ERR && id.signature == SIGNATURE {
        emit(UiEvent::Shortcut(id.id));
    }
    NO_ERR
}

/// Suit les changements de disposition du clavier (voir `keyboardChanged:`
/// de la cible commune).
fn watch_layout(keys: &mut Keys, mtm: MainThreadMarker) {
    if keys.watching_layout {
        return;
    }
    keys.watching_layout = true;
    let target = actions(mtm);
    let observer: &AnyObject = target.as_ref();
    let name = NSString::from_str("com.apple.Carbon.TISNotifySelectedKeyboardInputSourceChanged");
    // SAFETY: `keyboardChanged:` est une méthode de la cible qui prend une
    // NSNotification ; la cible vit jusqu'à la fin du processus.
    unsafe {
        NSDistributedNotificationCenter::defaultCenter().addObserver_selector_name_object(
            observer,
            sel!(keyboardChanged:),
            Some(&name),
            None,
        );
    }
}

/// La disposition du clavier pour les raccourcis : celle d'un alphabet
/// latin choisie en dernier (celle que macOS prend pour ⌘ et une lettre,
/// même quand on écrit en russe ou en japonais).
struct Layout {
    /// La source, qui garde les données en vie.
    _source: CFRetained<CFType>,
    data: *const c_void,
    keyboard: u32,
}

impl Layout {
    fn current() -> Option<Self> {
        // SAFETY: fonction sans argument ; elle rend une référence à nous
        // (règle « Copy » de CoreFoundation), ou rien.
        let raw = unsafe { TISCopyCurrentASCIICapableKeyboardLayoutInputSource() };
        // SAFETY: référence à nous, reprise une seule fois.
        let source = unsafe { CFRetained::from_raw(NonNull::new(raw.cast::<CFType>())?) };
        // SAFETY: constante de Carbon, valide toute la vie du processus ;
        // la source est vivante ; le résultat (règle « Get ») vit avec elle.
        let data = unsafe { TISGetInputSourceProperty(raw, kTISPropertyUnicodeKeyLayoutData) };
        let data = NonNull::new(data.cast::<CFData>())?;
        // SAFETY: un CFData vivant tant que la source l'est (gardée ici).
        let bytes = unsafe { data.as_ref() }.byte_ptr();
        if bytes.is_null() {
            return None;
        }
        Some(Self {
            _source: source,
            data: bytes.cast(),
            // SAFETY: fonction sans argument de Carbon.
            keyboard: u32::from(unsafe { LMGetKbdType() }),
        })
    }

    /// La touche (son code) qui donne cette lettre, sans modificateur.
    fn code_of(&self, letter: char) -> Option<u16> {
        (0..128u16).find(|&code| {
            let mut dead = 0u32;
            let mut len = 0usize;
            let mut chars = [0u16; 4];
            // SAFETY: `data` est une disposition (UCKeyboardLayout) vivante
            // tant que `self` l'est ; les sorties pointent vers des variables
            // locales de la taille annoncée.
            let status = unsafe {
                UCKeyTranslate(
                    self.data,
                    code,
                    KEY_ACTION_DISPLAY,
                    0,
                    self.keyboard,
                    NO_DEAD_KEYS,
                    &mut dead,
                    chars.len(),
                    &mut len,
                    chars.as_mut_ptr(),
                )
            };
            status == NO_ERR
                && len == 1
                && char::from_u32(u32::from(chars[0]))
                    .is_some_and(|c| c.to_ascii_lowercase() == letter)
        })
    }
}

/// La touche d'une lettre sur un clavier QWERTY (sans disposition lisible).
fn ansi_code(letter: char) -> Option<u16> {
    const LETTERS: [(char, u16); 26] = [
        ('a', 0x00),
        ('s', 0x01),
        ('d', 0x02),
        ('f', 0x03),
        ('h', 0x04),
        ('g', 0x05),
        ('z', 0x06),
        ('x', 0x07),
        ('c', 0x08),
        ('v', 0x09),
        ('b', 0x0B),
        ('q', 0x0C),
        ('w', 0x0D),
        ('e', 0x0E),
        ('r', 0x0F),
        ('y', 0x10),
        ('t', 0x11),
        ('o', 0x1F),
        ('u', 0x20),
        ('i', 0x22),
        ('p', 0x23),
        ('l', 0x25),
        ('j', 0x26),
        ('k', 0x28),
        ('n', 0x2D),
        ('m', 0x2E),
    ];
    LETTERS
        .iter()
        .find(|(c, _)| *c == letter)
        .map(|&(_, code)| code)
}

/// Écoute la touche Échap (`true`), dans toutes les apps, ou cesse.
/// Renvoie `false` si macOS ne la laissera pas entendre hors de Mouflon :
/// il y faut l'autorisation d'Accessibilité. À appeler sur le fil principal
/// (de nouveau quand l'autorisation arrive : l'écoute est refaite).
pub fn listen_escape(on: bool) -> bool {
    if MainThreadMarker::new().is_none() {
        return false;
    }
    KEYS.with(|keys| {
        let Ok(mut keys) = keys.try_borrow_mut() else {
            return false;
        };
        if let Some((global, local)) = keys.escape.take() {
            // SAFETY: deux écoutes rendues par `addGlobalMonitor…` et
            // `addLocalMonitor…`, retirées une seule fois.
            unsafe {
                NSEvent::removeMonitor(&global);
                NSEvent::removeMonitor(&local);
            }
        }
        if !on {
            return true;
        }
        let mask = NSEventMask::KeyDown | NSEventMask::KeyUp;
        // Dans les autres apps : on regarde passer, sans rien retenir.
        let global = RcBlock::new(|event: NonNull<NSEvent>| {
            // SAFETY: AppKit passe un événement vivant le temps de l'appel.
            escape_seen(unsafe { event.as_ref() });
        });
        // Dans Mouflon (ses réglages au premier plan) : l'événement suit
        // son cours.
        let local = RcBlock::new(|event: NonNull<NSEvent>| -> *mut NSEvent {
            // SAFETY: comme ci-dessus.
            escape_seen(unsafe { event.as_ref() });
            event.as_ptr()
        });
        let global = NSEvent::addGlobalMonitorForEventsMatchingMask_handler(mask, &global);
        // SAFETY: le bloc rend l'événement reçu, un pointeur valide.
        let local = unsafe { NSEvent::addLocalMonitorForEventsMatchingMask_handler(mask, &local) };
        match (global, local) {
            (Some(global), Some(local)) => keys.escape = Some((global, local)),
            (global, local) => {
                for monitor in [global, local].into_iter().flatten() {
                    // SAFETY: une écoute qui vient d'être posée.
                    unsafe { NSEvent::removeMonitor(&monitor) };
                }
                return false;
            }
        }
        super::accessibility_trusted()
    })
}

/// Une touche passe : seule Échap compte.
fn escape_seen(event: &NSEvent) {
    if event.keyCode() != ESCAPE {
        return;
    }
    let change = match event.r#type() {
        NSEventType::KeyDown if event.isARepeat() => KeyMove::Repeat,
        NSEventType::KeyDown => KeyMove::Down,
        NSEventType::KeyUp => KeyMove::Up,
        _ => return,
    };
    emit(UiEvent::Escape(change));
}
