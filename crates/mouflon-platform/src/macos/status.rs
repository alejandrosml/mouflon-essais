//! L'icône de Mouflon dans la barre des menus, avec son menu tel qu'AppKit
//! le montre lui-même : on appuie sur l'icône, on glisse jusqu'à un article
//! et on relâche dessus (ou on clique, puis on clique un article), comme
//! pour toutes les icônes de la barre des menus.
//!
//! (La bibliothèque `tray-icon` ouvrait le menu en simulant un clic sur
//! l'icône : le menu restait ouvert, mais relâcher le bouton sur un article
//! ne le choisissait pas.)

use muda::ContextMenu as _;
use objc2::rc::Retained;
use objc2::{AnyThread, MainThreadMarker};
use objc2_app_kit::{
    NSBitmapFormat, NSBitmapImageRep, NSDeviceRGBColorSpace, NSImage, NSMenu, NSStatusBar,
    NSStatusItem, NSVariableStatusItemLength,
};
use objc2_foundation::{NSSize, NSString};

/// L'icône dans la barre des menus. Elle s'en va quand on la lâche.
pub struct StatusItem {
    item: Retained<NSStatusItem>,
    /// Le menu, gardé en vie aussi longtemps que l'icône.
    _menu: muda::Menu,
}

impl std::fmt::Debug for StatusItem {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("StatusItem").finish_non_exhaustive()
    }
}

impl StatusItem {
    /// Une icône faite de pixels RGBA (`size` × `size`, alpha
    /// prémultiplié), affichée en `points` × `points` et teintée par macOS
    /// selon la barre des menus (image « modèle »), qui montre `menu`.
    /// `None` hors du fil principal.
    #[must_use]
    pub fn new(
        rgba: &[u8],
        size: u32,
        points: f64,
        tooltip: &str,
        menu: &muda::Menu,
    ) -> Option<Self> {
        let mtm = MainThreadMarker::new()?;
        let ns_menu = menu.ns_menu().cast::<NSMenu>();
        if ns_menu.is_null() {
            return None;
        }
        let item = NSStatusBar::systemStatusBar().statusItemWithLength(NSVariableStatusItemLength);
        if let Some(button) = item.button(mtm) {
            if let Some(image) = template_image(rgba, size, points) {
                button.setImage(Some(&image));
            }
            button.setToolTip(Some(&NSString::from_str(tooltip)));
        }
        // SAFETY: muda garde son NSMenu (un `Retained`) tant que le menu
        // vit ; on en garde une copie (qui partage ce NSMenu) dans l'icône.
        item.setMenu(Some(unsafe { &*ns_menu }));
        Some(Self {
            item,
            _menu: menu.clone(),
        })
    }
}

impl Drop for StatusItem {
    fn drop(&mut self) {
        NSStatusBar::systemStatusBar().removeStatusItem(&self.item);
    }
}

/// Une image AppKit « modèle » faite de pixels RGBA prémultipliés.
fn template_image(rgba: &[u8], size: u32, points: f64) -> Option<Retained<NSImage>> {
    let side = size as isize;
    if side <= 0 || rgba.len() < (side * side * 4) as usize {
        return None;
    }
    // SAFETY: AppKit alloue lui-même les pixels (`planes` nul) ; RGBA 8 bits,
    // alpha prémultiplié (format par défaut), lignes de `side * 4` octets.
    let rep = unsafe {
        NSBitmapImageRep::initWithBitmapDataPlanes_pixelsWide_pixelsHigh_bitsPerSample_samplesPerPixel_hasAlpha_isPlanar_colorSpaceName_bitmapFormat_bytesPerRow_bitsPerPixel(
            NSBitmapImageRep::alloc(),
            std::ptr::null_mut(),
            side,
            side,
            8,
            4,
            true,
            false,
            NSDeviceRGBColorSpace,
            NSBitmapFormat::empty(),
            side * 4,
            32,
        )
    }?;
    let data = rep.bitmapData();
    if data.is_null() {
        return None;
    }
    // SAFETY: `data` pointe vers les `side * side * 4` octets alloués par
    // AppKit pour cette image ; nos pixels en ont autant (vérifié plus haut).
    unsafe {
        std::ptr::copy_nonoverlapping(rgba.as_ptr(), data, (side * side * 4) as usize);
    }
    let points = NSSize::new(points, points);
    rep.setSize(points);
    let image = NSImage::initWithSize(NSImage::alloc(), points);
    image.addRepresentation(&rep);
    image.setTemplate(true);
    Some(image)
}
