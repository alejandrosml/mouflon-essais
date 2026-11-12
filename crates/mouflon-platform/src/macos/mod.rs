//! Implémentation macOS.

mod actions;
mod clock;
pub(crate) mod diag;
mod form;
mod front;
mod gallery;
mod keys;
mod login;
mod moves;
mod popup;
mod runloop;
mod scene;
mod sound;
mod status;
mod ui;

pub use actions::set_ui_handler;
pub use clock::{FrameClock, ThreadClock};
pub use form::{show_form, snapshot_form, update_form};
pub use gallery::{set_gallery_preview, show_gallery, snapshot_gallery};
pub use keys::{listen_escape, set_shortcuts};
pub use login::{login_item, login_item_available, open_login_items_settings, set_login_item};
pub use moves::{follow_window_moves, set_window_moves_handler};
pub use popup::pop_up_menu;
pub use runloop::{LoopWaker, run_loop_for};
pub use scene::{SceneImage, SceneLayer, attach_scene, render_scene, scene_transaction};
pub use sound::{play_sound, warm_up_sounds};
pub use status::StatusItem;
pub use ui::show_about;

use std::ptr::NonNull;

use objc2::ClassType;
use objc2::MainThreadMarker;
use objc2::rc::Retained;
use objc2::runtime::{AnyObject, NSObjectProtocol};
use objc2_app_kit::{
    NSEvent, NSEventModifierFlags, NSPanel, NSRunningApplication, NSScreen, NSView, NSWindow,
    NSWindowCollectionBehavior, NSWindowStyleMask, NSWorkspace,
};
use objc2_application_services::{
    AXError, AXIsProcessTrusted, AXIsProcessTrustedWithOptions, AXUIElement, AXValue, AXValueType,
    kAXTrustedCheckOptionPrompt,
};
use objc2_core_foundation::{
    CFArray, CFBoolean, CFDictionary, CFNumber, CFPreferencesAppSynchronize,
    CFPreferencesCopyAppValue, CFRetained, CFString, CFType, CGPoint, CGRect, CGSize,
};
use objc2_core_graphics::{
    CGDisplayBounds, CGEvent, CGEventSource, CGEventSourceStateID, CGEventType, CGMainDisplayID,
    CGRectMakeWithDictionaryRepresentation, CGWindowListCopyWindowInfo, CGWindowListOption,
    kCGNullWindowID, kCGWindowAlpha, kCGWindowBounds, kCGWindowIsOnscreen, kCGWindowLayer,
    kCGWindowNumber, kCGWindowOwnerName, kCGWindowOwnerPID,
};
use objc2_foundation::{
    NSKeyValueObservingOptions, NSLocale, NSObject, NSObjectNSKeyValueObserverRegistration,
    NSObjectNSKeyValueObservingCustomization, NSString, NSURL, ns_string,
};
use raw_window_handle::{HasWindowHandle, RawWindowHandle};

use crate::{OsRect, OsScreen, OsWindow, current_pid};

/// Taille minimale d'une fenêtre pour qu'on la considère (élimine les
/// fenêtres techniques invisibles que certaines apps laissent à l'écran).
const MIN_SIZE: f64 = 48.0;

fn cocoa_to_top_left(rect: CGRect, primary_height: f64) -> OsRect {
    OsRect {
        x: rect.origin.x,
        y: primary_height - (rect.origin.y + rect.size.height),
        w: rect.size.width,
        h: rect.size.height,
    }
}

fn primary_height(mtm: MainThreadMarker) -> Option<f64> {
    NSScreen::screens(mtm)
        .iter()
        .next()
        .map(|s| s.frame().size.height)
}

/// Écrans connectés. Doit être appelé depuis le fil principal (sinon : liste vide).
#[must_use]
pub fn screens() -> Vec<OsScreen> {
    let Some(mtm) = MainThreadMarker::new() else {
        log::warn!("screens() appelé hors du fil principal");
        return Vec::new();
    };
    let Some(primary) = primary_height(mtm) else {
        return Vec::new();
    };
    NSScreen::screens(mtm)
        .iter()
        .map(|screen| OsScreen {
            frame: cocoa_to_top_left(screen.frame(), primary),
            visible: cocoa_to_top_left(screen.visibleFrame(), primary),
            scale: screen.backingScaleFactor(),
        })
        .collect()
}

fn number(dict: &CFDictionary<CFString, CFType>, key: &CFString) -> Option<f64> {
    dict.get(key)?.downcast::<CFNumber>().ok()?.as_f64()
}

fn string(dict: &CFDictionary<CFString, CFType>, key: &CFString) -> Option<String> {
    Some(dict.get(key)?.downcast::<CFString>().ok()?.to_string())
}

/// Fenêtres visibles des autres applications, de la plus en avant à la plus
/// en arrière, tous niveaux confondus (voir [`OsWindow::is_walkable`]).
#[must_use]
pub fn windows() -> Vec<OsWindow> {
    let options =
        CGWindowListOption::OptionOnScreenOnly | CGWindowListOption::ExcludeDesktopElements;
    let Some(list) = CGWindowListCopyWindowInfo(options, kCGNullWindowID) else {
        return Vec::new();
    };
    // SAFETY: la documentation de CGWindowListCopyWindowInfo garantit un
    // tableau de dictionnaires dont les clés sont des CFString.
    let list: CFRetained<CFArray<CFDictionary<CFString, CFType>>> =
        unsafe { CFRetained::cast_unchecked(list) };
    // SAFETY: ces constantes sont des CFString immuables exportées par
    // CoreGraphics, valides pendant toute la vie du processus.
    let (k_bounds, k_layer, k_number, k_pid, k_owner, k_alpha, k_onscreen) = unsafe {
        (
            kCGWindowBounds,
            kCGWindowLayer,
            kCGWindowNumber,
            kCGWindowOwnerPID,
            kCGWindowOwnerName,
            kCGWindowAlpha,
            kCGWindowIsOnscreen,
        )
    };
    let me = current_pid();
    let mut out = Vec::new();
    for info in list.iter() {
        let Some(layer) = number(&info, k_layer) else {
            continue;
        };
        let pid = number(&info, k_pid).unwrap_or(-1.0) as i32;
        if pid == me {
            continue;
        }
        if number(&info, k_alpha).is_some_and(|alpha| alpha < 0.05) {
            continue;
        }
        let onscreen = info
            .get(k_onscreen)
            .and_then(|v| v.downcast::<CFBoolean>().ok())
            .is_none_or(|b| b.as_bool());
        if !onscreen {
            continue;
        }
        let Some(bounds) = info
            .get(k_bounds)
            .and_then(|v| v.downcast::<CFDictionary>().ok())
        else {
            continue;
        };
        let mut rect = CGRect::default();
        // SAFETY: `bounds` est le dictionnaire de rectangle fourni par
        // CoreGraphics et `rect` un pointeur valide vers une variable locale.
        if !unsafe { CGRectMakeWithDictionaryRepresentation(Some(&bounds), &mut rect) } {
            continue;
        }
        if rect.size.width < MIN_SIZE || rect.size.height < MIN_SIZE {
            continue;
        }
        let id = number(&info, k_number).unwrap_or(0.0) as u64;
        out.push(OsWindow {
            id,
            pid,
            frame: OsRect {
                x: rect.origin.x,
                y: rect.origin.y,
                w: rect.size.width,
                h: rect.size.height,
            },
            layer: layer as i32,
            owner: string(&info, k_owner).unwrap_or_default(),
        });
    }
    // Le visualiseur de clavier n'est plus dans cette liste depuis macOS
    // 27.2 : on le prend dans ses réglages (rien si la liste l'a déjà).
    if !out.iter().any(OsWindow::is_accessibility_panel)
        && let Some(viewer) = keyboard_viewer()
    {
        crate::add_keyboard_viewer(&mut out, viewer);
    }
    out
}

/// Domaine des réglages du visualiseur de clavier, et identifiant de son app.
const KEYBOARD_VIEWER_DOMAIN: &str = "com.apple.AssistiveControl.virtualKeyboard";
const KEYBOARD_VIEWER_BUNDLE: &str = "com.apple.inputmethod.AssistiveControl";

/// Un réglage d'une autre app, relu à chaque fois : son app peut l'avoir
/// changé depuis la dernière lecture (le clavier déplacé, ouvert, fermé).
fn preference(domain: &str, key: &str) -> Option<CFRetained<CFType>> {
    let domain = CFString::from_str(domain);
    CFPreferencesAppSynchronize(&domain);
    CFPreferencesCopyAppValue(&CFString::from_str(key), &domain)
}

/// Le visualiseur de clavier (le clavier d'accessibilité de macOS) quand il
/// est ouvert, d'après ses réglages : `virtualKeyboardOnOff` dans ceux de
/// l'Accessibilité (ouvert ou non), `PanelFrame` dans les siens (son cadre,
/// que son app garde à jour quand on le déplace). Rien s'il est fermé, si son
/// app ne tourne pas ou si les réglages ne sont pas lisibles (bac à sable de
/// l'App Store). Pas encore vérifié : le clavier réduit à son icône (aucun
/// réglage relevé ne le dit ; s'il garde ce cadre, on y marcherait encore).
fn keyboard_viewer() -> Option<OsWindow> {
    let on = preference("com.apple.universalaccess", "virtualKeyboardOnOff")?;
    let on = match on.downcast::<CFBoolean>() {
        Ok(flag) => flag.as_bool(),
        Err(other) => other
            .downcast::<CFNumber>()
            .ok()
            .and_then(|n| n.as_i64())
            .is_some_and(|n| n != 0),
    };
    if !on {
        return None;
    }
    let app = NSRunningApplication::runningApplicationsWithBundleIdentifier(&NSString::from_str(
        KEYBOARD_VIEWER_BUNDLE,
    ))
    .firstObject()?;
    let text = preference(KEYBOARD_VIEWER_DOMAIN, "PanelFrame")?
        .downcast::<CFString>()
        .ok()?
        .to_string();
    // Le repère de Cocoa part du bas de l'écran principal (celui de la barre
    // des menus) ; Core Graphics le donne depuis n'importe quel fil.
    let primary_height = CGDisplayBounds(CGMainDisplayID()).size.height;
    Some(OsWindow {
        id: crate::KEYBOARD_VIEWER_ID,
        pid: app.processIdentifier(),
        frame: crate::keyboard_viewer_frame(&text, primary_height)?,
        layer: crate::KEYBOARD_VIEWER_LEVEL,
        owner: crate::KEYBOARD_VIEWER_OWNER.to_owned(),
    })
}

/// Processus de l'application au premier plan.
#[must_use]
pub fn frontmost_pid() -> Option<i32> {
    NSWorkspace::sharedWorkspace()
        .frontmostApplication()
        .map(|app| app.processIdentifier())
}

/// Ouvre un dossier dans le Finder. Passe par NSWorkspace plutôt que par la
/// commande `open` : c'est ce que permet le bac à sable de l'App Store.
pub fn open_folder(path: &std::path::Path) -> bool {
    let url = NSURL::fileURLWithPath(&NSString::from_str(&path.to_string_lossy()));
    NSWorkspace::sharedWorkspace().openURL(&url)
}

/// La touche Contrôle est-elle enfoncée ? Sur Mac, Contrôle-clic vaut
/// clic droit.
#[must_use]
pub fn control_key_down() -> bool {
    NSEvent::modifierFlags_class().contains(NSEventModifierFlags::Control)
}

/// Ouvre Réglages Système à la liste des apps autorisées à utiliser
/// l'Accessibilité, où l'utilisateur peut cocher Mouflon.
pub fn open_accessibility_settings() -> bool {
    NSURL::URLWithString(ns_string!(
        "x-apple.systempreferences:com.apple.preference.security?Privacy_Accessibility"
    ))
    .is_some_and(|url| NSWorkspace::sharedWorkspace().openURL(&url))
}

/// Position du curseur dans le repère commun, depuis n'importe quel fil :
/// un événement vide de Core Graphics la porte, déjà dans ce repère (origine
/// en haut à gauche de l'écran principal, y vers le bas).
#[must_use]
pub fn cursor_position() -> Option<(f64, f64)> {
    let event = CGEvent::new(None)?;
    let p = CGEvent::location(Some(&event));
    Some((p.x, p.y))
}

/// Depuis quand l'utilisateur n'a rien frappé au clavier, et rien fait du
/// tout : ce que le système en dit à tous les programmes (le moment, jamais
/// la touche ; sans autorisation particulière).
#[must_use]
pub fn user_activity() -> Option<crate::UserActivity> {
    let state = CGEventSourceStateID::CombinedSessionState;
    // `kCGAnyInputEventType` : (CGEventType)~0.
    let any = CGEventType(u32::MAX);
    let since_key = CGEventSource::seconds_since_last_event_type(state, CGEventType::KeyDown);
    let idle = CGEventSource::seconds_since_last_event_type(state, any);
    (since_key.is_finite() && idle.is_finite()).then_some(crate::UserActivity {
        since_key: since_key.max(0.0),
        idle: idle.max(0.0),
    })
}

/// Images par seconde que l'écran d'une fenêtre sait afficher, au plus
/// (120 sur un écran ProMotion). `None` si on ne sait pas.
#[must_use]
pub fn window_max_fps(window: &impl HasWindowHandle) -> Option<f64> {
    let handle = window.window_handle().ok()?;
    let RawWindowHandle::AppKit(appkit) = handle.as_raw() else {
        return None;
    };
    MainThreadMarker::new()?;
    // `maximumFramesPerSecond` date de macOS 12.
    if !objc2::available!(macos = 12.0) {
        return None;
    }
    // SAFETY: winit garantit que `ns_view` pointe vers une NSView vivante tant
    // que la fenêtre existe ; on est sur le fil principal (vérifié ci-dessus)
    // et on ne garde qu'une référence comptée pendant l'appel.
    let view: Retained<NSView> =
        unsafe { Retained::retain(appkit.ns_view.as_ptr().cast::<NSView>()) }?;
    let fps = view.window()?.screen()?.maximumFramesPerSecond();
    (fps > 0).then_some(fps as f64)
}

/// Règle une fenêtre d'animal : visible sur tous les Spaces et par-dessus les
/// apps en plein écran, ignorée par ⌘-Tab et Mission Control, jamais masquée
/// quand Mouflon passe en arrière-plan.
pub fn prepare_pet_window(window: &impl HasWindowHandle) {
    let Some((_, ns_window)) = view_and_window(window) else {
        return;
    };
    make_non_activating(&ns_window);
    ns_window.setCollectionBehavior(
        NSWindowCollectionBehavior::CanJoinAllSpaces
            | NSWindowCollectionBehavior::Stationary
            | NSWindowCollectionBehavior::IgnoresCycle
            | NSWindowCollectionBehavior::FullScreenAuxiliary,
    );
    ns_window.setHidesOnDeactivate(false);
    ns_window.setHasShadow(false);
}

/// La vue et la fenêtre AppKit d'une fenêtre winit (sur le fil principal).
fn view_and_window(
    window: &impl HasWindowHandle,
) -> Option<(Retained<NSView>, Retained<NSWindow>)> {
    let handle = window.window_handle().ok()?;
    let RawWindowHandle::AppKit(appkit) = handle.as_raw() else {
        return None;
    };
    MainThreadMarker::new()?;
    // SAFETY: winit garantit que `ns_view` pointe vers une NSView vivante tant
    // que la fenêtre existe ; on est sur le fil principal (vérifié ci-dessus)
    // et on ne garde que des références comptées.
    let view: Retained<NSView> =
        unsafe { Retained::retain(appkit.ns_view.as_ptr().cast::<NSView>()) }?;
    let ns_window = view.window()?;
    Some((view, ns_window))
}

/// Garde en vie, jusqu'à la fin du processus, la fenêtre d'un animal qui va
/// être fermée (ou que l'app quitte). Devenue un panneau après sa création,
/// elle ne peut pas être libérée sans dommage : AppKit, en la libérant,
/// voudrait défaire des observations qu'il avait posées sous son ancienne
/// classe, et lèverait une exception (`NSRangeException`) qui arrêterait
/// l'app. On lui garde donc une référence qu'on ne rend jamais, même à la
/// sortie du processus. Fermée, elle ne coûte presque rien : on lui retire
/// en plus ses calques de dessin (Metal), qui gardent des images de la
/// taille de la fenêtre.
pub fn keep_pet_window(window: &impl HasWindowHandle) {
    let Some((view, ns_window)) = view_and_window(window) else {
        return;
    };
    if !ns_window.isKindOfClass(NSPanel::class()) {
        // Classe d'origine : la fenêtre se libère sans problème.
        return;
    }
    if let Some(layer) = view.layer() {
        // SAFETY: lecture de la liste des sous-calques, sur le fil principal ;
        // chacun est retiré de son calque parent (celui de la vue), ce qui ne
        // touche pas à la fenêtre ni à ses observations.
        if let Some(sublayers) = unsafe { layer.sublayers() } {
            for sublayer in &sublayers {
                sublayer.removeFromSuperlayer();
            }
        }
    }
    // Une référence de plus, jamais rendue : la fenêtre n'est jamais libérée.
    std::mem::forget(ns_window);
}

/// Fait de la fenêtre d'un animal un panneau « non activant » : un clic ou
/// un glisser ne fait pas passer Mouflon au premier plan, et l'app où l'on
/// travaille garde la main (et le clavier).
///
/// winit crée des `NSWindow` ; seul un `NSPanel` peut porter le style
/// `NonactivatingPanel`. On change donc la classe de la fenêtre, comme le
/// fait le greffon tauri-nspanel. C'est sans danger si les deux classes ont
/// la même taille d'instance, ce que l'on vérifie : sinon, la fenêtre reste
/// une fenêtre ordinaire (qui active Mouflon quand on attrape l'animal).
///
/// winit observe aussi le thème de la fenêtre (`effectiveAppearance`, par
/// l'observation clé-valeur de Foundation) et retire cet observateur quand
/// la fenêtre se ferme. Cette observation est rangée sous la classe de la
/// fenêtre : la classe changée, le retrait échouerait (`NSRangeException`)
/// et l'app s'arrêterait dès qu'un animal s'en va ou qu'on la quitte. On
/// retire donc l'observateur de winit avant le changement de classe, et on
/// le remet après, tel qu'il l'avait posé.
fn make_non_activating(window: &NSWindow) {
    let current = window.class();
    let panel = NSPanel::class();
    if !window.isKindOfClass(panel) {
        if current.instance_size() != panel.instance_size() {
            log::warn!(
                "fenêtre d'animal : {} ne peut pas devenir un panneau non activant",
                current.name().to_string_lossy()
            );
            return;
        }
        let object: &NSObject = window.as_ref();
        let key = ns_string!("effectiveAppearance");
        // SAFETY: `delegate` est une méthode de `NSWindow` sans argument, qui
        // renvoie un objet (le délégué de winit) ou nil ; on le garde en vie
        // le temps des appels ci-dessous.
        let observer: Option<Retained<NSObject>> = unsafe { objc2::msg_send![window, delegate] };
        // winit n'observe la fenêtre que par son délégué : sans délégué ni
        // observation en cours, il n'y a rien à retirer ni à remettre.
        let observer = observer.filter(|_| !object.observationInfo().is_null());
        if let Some(observer) = &observer {
            // SAFETY: winit (0.30) pose cet observateur, avec ce délégué et
            // cette clé, à la création de la fenêtre ; il est bien enregistré.
            unsafe { object.removeObserver_forKeyPath(observer, key) };
        }
        // SAFETY:
        // - `NSPanel` hérite de `NSWindow` sans variable d'instance de plus :
        //   les deux tailles d'instance sont égales (vérifié ci-dessus), et la
        //   classe de winit (`WinitWindow`) n'en ajoute aucune non plus ; elle
        //   ne remplace que `canBecomeKeyWindow` et `canBecomeMainWindow`,
        //   dont on veut justement retrouver le comportement d'origine (une
        //   fenêtre sans bordure ne devient pas la fenêtre active).
        // - Tout le reste du programme ne voit cette fenêtre que comme une
        //   `NSWindow`, dont `NSPanel` garde toutes les méthodes.
        // - On est sur le fil principal (seul endroit où AppKit touche à la
        //   fenêtre), avant le premier affichage de l'animal.
        unsafe {
            let object: &AnyObject = window.as_ref();
            AnyObject::set_class(object, panel);
        }
        if let Some(observer) = &observer {
            // SAFETY: l'observateur est remis comme winit le pose (nouvelle et
            // ancienne valeurs, sans contexte) ; winit le retire à la
            // fermeture de la fenêtre, et le délégué vit aussi longtemps
            // qu'elle.
            unsafe {
                object.addObserver_forKeyPath_options_context(
                    observer,
                    key,
                    NSKeyValueObservingOptions::New | NSKeyValueObservingOptions::Old,
                    std::ptr::null_mut(),
                );
            }
        }
    }
    window.setStyleMask(window.styleMask() | NSWindowStyleMask::NonactivatingPanel);
    log::info!("fenêtre d'animal : panneau non activant");
}

/// Mouflon a-t-il l'autorisation d'Accessibilité (Réglages Système →
/// Confidentialité et sécurité → Accessibilité) ?
#[must_use]
pub fn accessibility_trusted() -> bool {
    // SAFETY: fonction sans argument, utilisable depuis n'importe quel fil.
    unsafe { AXIsProcessTrusted() }
}

/// Demande l'autorisation d'Accessibilité : macOS affiche sa fenêtre si
/// Mouflon ne l'a pas encore. Renvoie l'état actuel (la réponse de
/// l'utilisateur arrive plus tard).
pub fn request_accessibility() -> bool {
    // SAFETY: `kAXTrustedCheckOptionPrompt` est une CFString immuable exportée
    // par ApplicationServices, valide pendant toute la vie du processus.
    let key: &CFString = unsafe { kAXTrustedCheckOptionPrompt };
    let options = CFDictionary::from_slices(&[key], &[CFBoolean::new(true)]);
    // SAFETY: le dictionnaire a des clés CFString et des valeurs CFBoolean,
    // comme le demande la fonction.
    unsafe { AXIsProcessTrustedWithOptions(Some(options.as_opaque())) }
}

/// Attribut d'un élément d'accessibilité, s'il existe.
fn ax_attribute(element: &AXUIElement, name: &'static str) -> Option<CFRetained<CFType>> {
    let attribute = CFString::from_static_str(name);
    let mut value: *const CFType = std::ptr::null();
    // SAFETY: `value` pointe vers une variable locale ; en cas de succès,
    // la fonction y écrit une référence dont on devient propriétaire
    // (règle « Copy » de Core Foundation).
    let error = unsafe { element.copy_attribute_value(&attribute, NonNull::from(&mut value)) };
    if error != AXError::Success {
        return None;
    }
    let value = NonNull::new(value.cast_mut())?;
    // SAFETY: référence non nulle et possédée (voir ci-dessus).
    Some(unsafe { CFRetained::from_raw(value) })
}

/// Ce qu'une valeur d'accessibilité peut contenir : le type Rust et sa
/// description pour `AXValueGetValue` vont ensemble.
trait AxGeometry: Default {
    const KIND: AXValueType;
}

impl AxGeometry for CGPoint {
    const KIND: AXValueType = AXValueType::CGPoint;
}

impl AxGeometry for CGSize {
    const KIND: AXValueType = AXValueType::CGSize;
}

/// Point ou taille d'un élément d'accessibilité.
fn ax_geometry<T: AxGeometry>(element: &AXUIElement, name: &'static str) -> Option<T> {
    let value = ax_attribute(element, name)?.downcast::<AXValue>().ok()?;
    let mut out = T::default();
    // SAFETY: `out` est une variable locale de type `T`, celui que
    // `T::KIND` décrit ; la fonction renvoie `false` sans rien écrire si la
    // valeur contient autre chose.
    let ok = unsafe { value.value(T::KIND, NonNull::from(&mut out).cast()) };
    ok.then_some(out)
}

/// Cadre des icônes du Dock (repère commun), lu par l'Accessibilité. Rien
/// sans l'autorisation, ou si le Dock ne répond pas.
#[must_use]
pub fn dock_frame() -> Option<OsRect> {
    if !accessibility_trusted() {
        return None;
    }
    let dock = NSRunningApplication::runningApplicationsWithBundleIdentifier(&NSString::from_str(
        "com.apple.dock",
    ))
    .firstObject()?;
    // SAFETY: `processIdentifier` est celui d'une app en cours ; la fonction
    // renvoie l'élément racine de son interface, possédé (+1).
    let app = unsafe { AXUIElement::new_application(dock.processIdentifier()) };
    // Chaque question est un échange avec le Dock : un Dock qui ne répond
    // pas ne doit pas figer les animaux (6 secondes par défaut).
    // SAFETY: `app` est un élément valide ; le délai est un nombre positif.
    unsafe { app.set_messaging_timeout(0.25) };
    let children = ax_attribute(&app, "AXChildren")?
        .downcast::<CFArray>()
        .ok()?;
    // SAFETY: l'attribut « AXChildren » est un tableau d'éléments d'accessibilité.
    let children: CFRetained<CFArray<AXUIElement>> =
        unsafe { CFRetained::cast_unchecked(children) };
    // La liste des icônes : la première dont le rôle est « AXList ».
    let list = children.iter().find(|child| {
        ax_attribute(child, "AXRole")
            .and_then(|role| role.downcast::<CFString>().ok())
            .is_some_and(|role| role.to_string() == "AXList")
    })?;
    let origin: CGPoint = ax_geometry(&list, "AXPosition")?;
    let size: CGSize = ax_geometry(&list, "AXSize")?;
    (size.width > 0.0 && size.height > 0.0).then_some(OsRect {
        x: origin.x,
        y: origin.y,
        w: size.width,
        h: size.height,
    })
}

/// Langues de l'utilisateur, dans son ordre de préférence (codes BCP 47 :
/// « fr-FR », « es-419 »…), langue choisie pour Mouflon seul comprise.
#[must_use]
pub fn preferred_languages() -> Vec<String> {
    NSLocale::preferredLanguages()
        .iter()
        .map(|tag| tag.to_string())
        .collect()
}
