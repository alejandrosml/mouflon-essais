//! La fenêtre « À propos » de Mouflon.

use objc2::rc::Retained;
use objc2::runtime::AnyObject;
use objc2::{AllocAnyThread, MainThreadMarker, available};
use objc2_app_kit::{
    NSAboutPanelOptionApplicationName, NSAboutPanelOptionApplicationVersion,
    NSAboutPanelOptionCredits, NSAboutPanelOptionVersion, NSApplication, NSColor, NSFont,
    NSFontAttributeName, NSForegroundColorAttributeName, NSLinkAttributeName,
    NSMutableParagraphStyle, NSParagraphStyleAttributeName, NSTextAlignment,
};
use objc2_foundation::{
    NSAttributedString, NSAttributedStringKey, NSDictionary, NSMutableAttributedString, NSString,
    NSURL, ns_string,
};

use crate::{About, AboutLine};

/// Fait passer Mouflon au premier plan : une app de la barre des menus ne
/// l'est jamais d'elle-même, et ses fenêtres resteraient derrière celles de
/// l'app où l'on travaille.
pub(super) fn activate(app: &NSApplication) {
    if available!(macos = 14.0) {
        app.activate();
    } else {
        #[allow(deprecated)]
        app.activateIgnoringOtherApps(true);
    }
}

/// Texte des crédits : un paragraphe centré par ligne, liens cliquables.
fn credits(lines: &[AboutLine<'_>]) -> Retained<NSAttributedString> {
    let text = NSMutableAttributedString::new();
    let style = NSMutableParagraphStyle::new();
    style.setAlignment(NSTextAlignment::Center);
    style.setParagraphSpacing(5.0);
    let font = NSFont::systemFontOfSize(NSFont::smallSystemFontSize());
    // Couleur du système : lisible en clair comme en sombre.
    let color = NSColor::secondaryLabelColor();
    for (i, line) in lines.iter().enumerate() {
        let (shown, url) = match line {
            AboutLine::Text(shown) => (*shown, None),
            AboutLine::Link(shown, url) => (*shown, NSURL::URLWithString(&NSString::from_str(url))),
        };
        let mut string = shown.to_owned();
        if i + 1 < lines.len() {
            string.push('\n');
        }
        // SAFETY: statiques NSString exportées par AppKit, valides pendant
        // toute la vie du processus.
        let mut keys: Vec<&NSAttributedStringKey> = unsafe {
            vec![
                NSFontAttributeName,
                NSParagraphStyleAttributeName,
                NSForegroundColorAttributeName,
            ]
        };
        let mut values: Vec<&AnyObject> = vec![font.as_ref(), style.as_ref(), color.as_ref()];
        if let Some(url) = &url {
            // SAFETY: comme ci-dessus.
            keys.push(unsafe { NSLinkAttributeName });
            values.push(url.as_ref());
        }
        let attributes = NSDictionary::from_slices(&keys, &values);
        // SAFETY: chaque attribut a une valeur du type qu'AppKit attend :
        // NSFont, NSParagraphStyle, NSColor et NSURL.
        let piece = unsafe {
            NSAttributedString::initWithString_attributes(
                NSAttributedString::alloc(),
                &NSString::from_str(&string),
                Some(&attributes),
            )
        };
        text.appendAttributedString(&piece);
    }
    Retained::into_super(text)
}

/// Ouvre la fenêtre « À propos » de macOS, devant les autres apps : icône,
/// nom, version, crédits (avec liens) et mention de copyright.
pub fn show_about(about: &About<'_>) {
    let Some(mtm) = MainThreadMarker::new() else {
        return;
    };
    let app = NSApplication::sharedApplication(mtm);
    let name = NSString::from_str(about.name);
    let version = NSString::from_str(about.version);
    // Pas de numéro de build entre parenthèses : il redirait la version.
    let build = NSString::from_str("");
    let copyright = NSString::from_str(about.copyright);
    let credits = credits(about.lines);
    // SAFETY: statiques NSString exportées par AppKit, valides pendant toute
    // la vie du processus.
    let keys: [&NSString; 5] = unsafe {
        [
            NSAboutPanelOptionApplicationName,
            NSAboutPanelOptionApplicationVersion,
            NSAboutPanelOptionVersion,
            ns_string!("Copyright"),
            NSAboutPanelOptionCredits,
        ]
    };
    let values: [&AnyObject; 5] = [
        name.as_ref(),
        version.as_ref(),
        build.as_ref(),
        copyright.as_ref(),
        credits.as_ref(),
    ];
    let options = NSDictionary::from_slices(&keys, &values);
    activate(&app);
    // SAFETY: les clés sont celles que la méthode connaît, avec des valeurs
    // du type attendu (NSString, et NSAttributedString pour les crédits).
    unsafe { app.orderFrontStandardAboutPanelWithOptions(&options) };
    super::front::bring_forward(mtm, None);
}
