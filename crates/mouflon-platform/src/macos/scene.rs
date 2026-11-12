//! Les calques Core Animation de la scène : une fenêtre transparente par
//! écran, et dedans un calque par morceau d'image des animaux. Mouflon ne
//! dessine rien lui-même : il donne à chaque calque son image (partagée par
//! tous ceux qui la montrent), sa place, sa transformation et son opacité,
//! et le compositeur du système fait le reste — il ne recompose que ce qui
//! a changé.

use objc2::rc::Retained;
use objc2::runtime::AnyObject;
use objc2_app_kit::NSViewLayerContentsRedrawPolicy;
use objc2_core_foundation::{
    CFData, CFRetained, CFType, CGAffineTransform, CGPoint, CGRect, CGSize,
};
use objc2_core_graphics::{
    CGBitmapContextCreate, CGBitmapInfo, CGColorRenderingIntent, CGColorSpace, CGContext,
    CGDataProvider, CGImage, CGImageAlphaInfo, CGImageByteOrderInfo, kCGColorSpaceSRGB,
};
use objc2_quartz_core::{
    CAAutoresizingMask, CALayer, CATransaction, kCAFilterLinear, kCAFilterNearest,
};
use raw_window_handle::HasWindowHandle;

/// Une image prête pour un calque : pixels BGRA prémultipliés, dans
/// l'espace sRGB (le format que le compositeur affiche sans conversion).
#[derive(Debug, Clone)]
pub struct SceneImage {
    image: CFRetained<CGImage>,
    bytes: usize,
}

impl SceneImage {
    /// `bgra` : `width` × `height` pixels BGRA prémultipliés, ligne par
    /// ligne, sans bourrage. `None` si les tailles ne vont pas ensemble.
    #[must_use]
    pub fn from_bgra(width: u32, height: u32, bgra: &[u8]) -> Option<Self> {
        let (w, h) = (width as usize, height as usize);
        if w == 0 || h == 0 || bgra.len() != w * h * 4 {
            return None;
        }
        // SAFETY: constante de Core Graphics, valide toute la vie du processus.
        let srgb = CGColorSpace::with_name(Some(unsafe { kCGColorSpaceSRGB }))?;
        let data = CFData::from_bytes(bgra);
        let provider = CGDataProvider::with_cf_data(Some(&data))?;
        let info = CGBitmapInfo(
            CGImageAlphaInfo::PremultipliedFirst.0 | CGImageByteOrderInfo::Order32Little.0,
        );
        // SAFETY: 8 bits par composante, 32 par pixel, `w * 4` octets par
        // ligne : exactement ce que contient `data` (vérifié ci-dessus) ;
        // pas de table de décodage (pointeur nul permis).
        let image = unsafe {
            CGImage::new(
                w,
                h,
                8,
                32,
                w * 4,
                Some(&srgb),
                info,
                Some(&provider),
                std::ptr::null(),
                true,
                CGColorRenderingIntent::RenderingIntentDefault,
            )
        }?;
        Some(Self {
            image,
            bytes: bgra.len(),
        })
    }

    /// Octets de ses pixels.
    #[must_use]
    pub fn byte_size(&self) -> usize {
        self.bytes
    }
}

/// Un calque de la scène. Son point d'ancrage est l'origine de son repère :
/// sa place et sa transformation partent de là.
#[derive(Debug, Clone)]
pub struct SceneLayer {
    layer: Retained<CALayer>,
}

// SAFETY: Core Animation permet de changer des calques depuis n'importe
// quel fil, un seul à la fois. Les calques de la scène sont faits sur le fil
// principal (la racine, posée dans la fenêtre d'un écran) ou sur le fil de
// l'affichage, et une fois donnés au fil de l'affichage, lui seul y touche,
// dans ses propres transactions.
unsafe impl Send for SceneLayer {}

impl Default for SceneLayer {
    fn default() -> Self {
        Self::new()
    }
}

impl SceneLayer {
    #[must_use]
    pub fn new() -> Self {
        let layer = CALayer::new();
        layer.setAnchorPoint(CGPoint::ZERO);
        layer.setPosition(CGPoint::ZERO);
        Self { layer }
    }

    /// Ajoute `child` par-dessus ses autres calques.
    pub fn add(&self, child: &Self) {
        self.layer.addSublayer(&child.layer);
    }

    /// Place `child` au rang `index` parmi ses calques (0 : tout en dessous).
    pub fn insert(&self, child: &Self, index: u32) {
        self.layer.insertSublayer_atIndex(&child.layer, index);
    }

    /// Retire le calque de son parent.
    pub fn remove(&self) {
        self.layer.removeFromSuperlayer();
    }

    /// Son origine (le coin de son repère), en points, dans le repère du parent.
    pub fn set_position(&self, x: f64, y: f64) {
        self.layer.setPosition(CGPoint::new(x, y));
    }

    /// Taille, dans son propre repère (les pixels de son image).
    pub fn set_size(&self, width: f64, height: f64) {
        self.layer
            .setBounds(CGRect::new(CGPoint::ZERO, CGSize::new(width, height)));
    }

    /// Transformation affine `[a, b, c, d, tx, ty]` (x' = a x + c y + tx,
    /// y' = b x + d y + ty), de son repère vers celui de son parent.
    pub fn set_transform(&self, [a, b, c, d, tx, ty]: [f64; 6]) {
        self.layer
            .setAffineTransform(CGAffineTransform { a, b, c, d, tx, ty });
    }

    pub fn set_opacity(&self, opacity: f32) {
        self.layer.setOpacity(opacity.clamp(0.0, 1.0));
    }

    pub fn set_hidden(&self, hidden: bool) {
        self.layer.setHidden(hidden);
    }

    /// Son image, étirée sur toute sa taille ; aucune pour un calque qui ne
    /// fait que porter d'autres calques.
    pub fn set_image(&self, image: Option<&SceneImage>) {
        let contents: Option<&AnyObject> = image.map(|image| {
            let cf: &CFType = &image.image;
            cf.as_ref()
        });
        // SAFETY: le contenu d'un calque est une CGImage (ou rien), ce que
        // `contents` attend ; le calque garde l'image en vie.
        unsafe { self.layer.setContents(contents) };
    }

    /// Image lissée quand elle est agrandie ou réduite, ou au plus proche
    /// (du pixel art agrandi reste franc).
    pub fn set_smooth(&self, smooth: bool) {
        // SAFETY: constantes de Core Animation, valides toute la vie du processus.
        let filter = unsafe {
            if smooth {
                kCAFilterLinear
            } else {
                kCAFilterNearest
            }
        };
        self.layer.setMagnificationFilter(filter);
        self.layer.setMinificationFilter(filter);
    }

    /// Ses calques se composent d'abord ensemble, puis prennent son
    /// opacité d'un bloc (le corps d'un animal qui s'efface ne montre pas
    /// ses pattes à travers lui).
    pub fn set_group_opacity(&self, group: bool) {
        self.layer.setAllowsGroupOpacity(group);
    }
}

/// Fait `change` en une seule transaction Core Animation, sans animation
/// implicite : tout ce qui change à cette image arrive ensemble au
/// compositeur.
pub fn scene_transaction<R>(change: impl FnOnce() -> R) -> R {
    CATransaction::begin();
    CATransaction::setDisableActions(true);
    let result = change();
    CATransaction::commit();
    result
}

/// Pose la racine des calques de la scène dans une fenêtre, sur toute sa
/// taille, dans le repère de sa vue : celle de winit est retournée
/// (`isFlipped`), et AppKit retourne son calque — origine en haut à gauche,
/// y vers le bas, en points ; une image s'y affiche à l'endroit. Elle suit
/// la taille de la fenêtre.
pub fn attach_scene(window: &impl HasWindowHandle) -> Option<SceneLayer> {
    let (view, _) = super::view_and_window(window)?;
    // La vue ne dessine jamais dans son propre calque (ceux de la scène
    // portent tout) : pas d'image de la taille de l'écran derrière eux, et
    // rien qu'AppKit ait à redessiner — surtout pas pendant une transaction
    // envoyée par le fil de l'affichage.
    view.setLayerContentsRedrawPolicy(NSViewLayerContentsRedrawPolicy::Never);
    view.setWantsLayer(true);
    let host = view.layer()?;
    let root = SceneLayer::new();
    root.layer.setFrame(host.bounds());
    root.layer.setAutoresizingMask(
        CAAutoresizingMask::LayerWidthSizable | CAAutoresizingMask::LayerHeightSizable,
    );
    scene_transaction(|| host.addSublayer(&root.layer));
    Some(root)
}

/// Dessine sur le processeur ce que montrent les calques d'une scène de
/// `width` × `height` points, à `scale` pixels par point : de quoi vérifier
/// ce qui est à l'écran (essais). Pixels RVBA prémultipliés, la première
/// ligne en haut.
///
/// La racine est dessinée seule, sans le calque retourné de la vue qui la
/// porte à l'écran : le dessin est retourné de même ici, et l'image est
/// celle de l'écran (les essais de la CI comparent les deux).
#[must_use]
pub fn render_scene(
    root: &SceneLayer,
    width: u32,
    height: u32,
    scale: f64,
) -> Option<(u32, u32, Vec<u8>)> {
    let w = (f64::from(width) * scale).round() as usize;
    let h = (f64::from(height) * scale).round() as usize;
    if w == 0 || h == 0 || w > 16_384 || h > 16_384 {
        return None;
    }
    let mut pixels = vec![0u8; w * h * 4];
    // SAFETY: constante de Core Graphics, valide toute la vie du processus.
    let srgb = CGColorSpace::with_name(Some(unsafe { kCGColorSpaceSRGB }))?;
    let info = CGImageAlphaInfo::PremultipliedFirst.0 | CGImageByteOrderInfo::Order32Little.0;
    // SAFETY: `pixels` fait exactement `h` lignes de `w * 4` octets et vit
    // plus longtemps que le contexte (rendu avant la fin de ce bloc) ; 8
    // bits par composante, BVRA prémultiplié : un format que Core Graphics
    // accepte pour un contexte bitmap.
    let context = unsafe {
        CGBitmapContextCreate(
            pixels.as_mut_ptr().cast(),
            w,
            h,
            8,
            w * 4,
            Some(&srgb),
            info,
        )
    }?;
    CGContext::translate_ctm(Some(&context), 0.0, h as f64);
    CGContext::scale_ctm(Some(&context), scale, -scale);
    root.layer.renderInContext(&context);
    drop(context);
    for px in pixels.as_chunks_mut::<4>().0 {
        px.swap(0, 2);
    }
    Some((w as u32, h as u32, pixels))
}
