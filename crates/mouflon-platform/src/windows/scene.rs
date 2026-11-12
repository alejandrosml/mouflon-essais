//! Les calques de la scène sous Windows : DirectComposition. Comme sur Mac
//! avec Core Animation, Mouflon ne dessine rien lui-même : il donne à chaque
//! calque son image (partagée par tous ceux qui la montrent), sa place, sa
//! transformation et son opacité, et le compositeur de Windows (celui qui
//! assemble déjà toutes les fenêtres du bureau) fait le reste — il ne
//! recompose que ce qui a changé.
//!
//! Rien d'opaque n'est jamais posé sur l'écran : la fenêtre de chaque écran
//! n'a pas d'image à elle, seuls ses calques se voient. Si le compositeur
//! manque, il n'y a simplement rien à voir (et l'app le dit), jamais un écran
//! noir.
//!
//! Les images passent par un appareil Direct3D 11, celui de la carte
//! graphique qui affiche le bureau, ou à défaut celui que Windows calcule
//! sur le processeur. Si cet appareil se perd (pilote mis à jour, session à
//! distance, carte qui change), tout est refait : voir [`scene_lost`].

use std::cell::Cell;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::{Arc, Mutex, MutexGuard, OnceLock, PoisonError, Weak};
use std::time::{Duration, Instant};

use raw_window_handle::HasWindowHandle;
use windows::Win32::Foundation::{HMODULE, HWND, POINT};
use windows::Win32::Graphics::Direct3D::{D3D_DRIVER_TYPE_HARDWARE, D3D_DRIVER_TYPE_WARP};
use windows::Win32::Graphics::Direct3D11::{
    D3D11_BOX, D3D11_CREATE_DEVICE_BGRA_SUPPORT, D3D11_SDK_VERSION, D3D11CreateDevice,
    ID3D11Device, ID3D11DeviceContext, ID3D11Multithread, ID3D11Texture2D,
};
use windows::Win32::Graphics::DirectComposition::{
    COMPOSITION_FRAME_STATS, COMPOSITION_TARGET_ID, COMPOSITION_TARGET_STATS,
    DCOMPOSITION_BITMAP_INTERPOLATION_MODE_LINEAR,
    DCOMPOSITION_BITMAP_INTERPOLATION_MODE_NEAREST_NEIGHBOR, DCOMPOSITION_FRAME_STATISTICS,
    DCOMPOSITION_OPACITY_MODE_LAYER, DCOMPOSITION_OPACITY_MODE_MULTIPLY, DCompositionCreateDevice2,
    IDCompositionDesktopDevice, IDCompositionSurface, IDCompositionTarget, IDCompositionVisual,
    IDCompositionVisual2, IDCompositionVisual3,
};
use windows::Win32::Graphics::Dxgi::Common::{
    DXGI_ALPHA_MODE_PREMULTIPLIED, DXGI_FORMAT_B8G8R8A8_UNORM,
};
use windows::Win32::Graphics::Dxgi::{IDXGIAdapter, IDXGIDevice};
use windows::core::{HRESULT, IUnknown, Interface};
use windows_numerics::Matrix3x2;

/// Côté le plus grand d'une image de calque, en pixels (les morceaux des
/// animaux en font quelques centaines).
const MAX_SIDE: u32 = 8192;
/// L'appareil est vérifié toutes les tant de transactions.
const CHECK_EVERY: u64 = 120;

/// Ce avec quoi les calques sont faits : le compositeur, et l'appareil
/// Direct3D qui porte leurs images.
struct Compositor {
    device: IDCompositionDesktopDevice,
    d3d: ID3D11Device,
    context: ID3D11DeviceContext,
    /// Son rang : un calque ou une image ne sert qu'avec le compositeur qui
    /// l'a fait.
    generation: u64,
    lost: AtomicBool,
    /// Quelque chose a changé depuis le dernier envoi au compositeur.
    dirty: AtomicBool,
    transactions: AtomicU64,
    /// Essais : l'appareil est déclaré perdu à cette heure, pour vérifier
    /// que tout se refait (`MOUFLON_PERDRE_AFFICHAGE`, en secondes).
    lose_at: Option<Instant>,
    /// Essais : comment le compositeur de Windows reçoit les envois.
    pace: Option<Mutex<Pace>>,
}

/// Pour le journal d'un essai : ce que le compositeur de Windows a fait de
/// chaque envoi, relevé au moment de préparer le suivant. Une image de
/// Mouflon par image du compositeur, ni deux ni aucune : c'est ce qui fait
/// un mouvement régulier (voir `clock`).
#[derive(Default)]
struct Pace {
    since: Option<Instant>,
    /// Envois relevés, et envoi fait depuis le dernier relevé.
    sends: u32,
    sent: bool,
    /// Heure du relevé précédent, et de la dernière image du compositeur
    /// qu'il a vue (secondes).
    before: Option<(f64, f64)>,
    /// À chaque relevé : depuis combien de temps le compositeur a fait sa
    /// dernière image (négatif : elle est prévue pour plus tard), et dans
    /// combien de temps il prévoit la suivante (millisecondes).
    since_frame: Vec<f32>,
    until_frame: Vec<f32>,
    /// Entre deux relevés faits à une image d'écart : de combien d'images le
    /// compositeur a avancé (0, 1, 2, davantage).
    steps: [u32; 4],
    /// Sa cadence, en images par seconde.
    rate: f64,
    /// Ce que l'écran a reçu au dernier relevé : images, rafraîchissements.
    shown: Option<(u32, u32)>,
}

/// Les comptes que le compositeur de Windows 11 tient de chaque écran
/// (« compositor clock ») : combien d'images il lui a vraiment données, sur
/// combien de rafraîchissements. Ces fonctions n'existent pas sous
/// Windows 10 : elles sont cherchées à l'exécution, pour les essais
/// seulement.
#[derive(Clone, Copy)]
struct ScreenCounts {
    frame_id: unsafe extern "system" fn(i32, *mut u64) -> i32,
    statistics: unsafe extern "system" fn(
        u64,
        *mut COMPOSITION_FRAME_STATS,
        u32,
        *mut COMPOSITION_TARGET_ID,
        *mut u32,
    ) -> i32,
    target: unsafe extern "system" fn(
        u64,
        *const COMPOSITION_TARGET_ID,
        *mut COMPOSITION_TARGET_STATS,
    ) -> i32,
}

impl ScreenCounts {
    fn get() -> Option<Self> {
        static FOUND: OnceLock<Option<ScreenCounts>> = OnceLock::new();
        *FOUND.get_or_init(|| {
            use windows_sys::Win32::System::LibraryLoader::{GetModuleHandleW, GetProcAddress};
            // SAFETY: la bibliothèque de DirectComposition est déjà chargée
            // (le compositeur en vient) ; les noms sont des textes constants
            // terminés par zéro. Chaque fonction trouvée a la signature que
            // sa documentation donne (`dcomp.h`), reprise dans le type du
            // champ où elle est rangée.
            unsafe {
                let module = GetModuleHandleW(windows_sys::core::w!("dcomp.dll"));
                if module.is_null() {
                    return None;
                }
                let frame_id = GetProcAddress(module, c"DCompositionGetFrameId".as_ptr().cast())?;
                let statistics =
                    GetProcAddress(module, c"DCompositionGetStatistics".as_ptr().cast())?;
                let target =
                    GetProcAddress(module, c"DCompositionGetTargetStatistics".as_ptr().cast())?;
                Some(Self {
                    frame_id: std::mem::transmute::<
                        unsafe extern "system" fn() -> isize,
                        unsafe extern "system" fn(i32, *mut u64) -> i32,
                    >(frame_id),
                    statistics: std::mem::transmute::<
                        unsafe extern "system" fn() -> isize,
                        unsafe extern "system" fn(
                            u64,
                            *mut COMPOSITION_FRAME_STATS,
                            u32,
                            *mut COMPOSITION_TARGET_ID,
                            *mut u32,
                        ) -> i32,
                    >(statistics),
                    target: std::mem::transmute::<
                        unsafe extern "system" fn() -> isize,
                        unsafe extern "system" fn(
                            u64,
                            *const COMPOSITION_TARGET_ID,
                            *mut COMPOSITION_TARGET_STATS,
                        ) -> i32,
                    >(target),
                })
            }
        })
    }

    /// Les images données au premier écran, et ses rafraîchissements,
    /// depuis toujours (à la dernière image que le compositeur a finie).
    fn shown(self) -> Option<(u32, u32)> {
        let mut frame = 0u64;
        let mut stats = COMPOSITION_FRAME_STATS::default();
        let mut targets = [COMPOSITION_TARGET_ID::default(); 8];
        let mut count = 0u32;
        let mut target = COMPOSITION_TARGET_STATS::default();
        // SAFETY: chaque appel n'écrit que dans les valeurs données, qui
        // sont à nous et de la taille annoncée (2 : la dernière image
        // finie, `COMPOSITION_FRAME_ID_COMPLETED`).
        let known = unsafe {
            (self.frame_id)(2, &mut frame) >= 0
                && (self.statistics)(
                    frame,
                    &mut stats,
                    targets.len() as u32,
                    targets.as_mut_ptr(),
                    &mut count,
                ) >= 0
                && count >= 1
                && (self.target)(frame, &targets[0], &mut target) >= 0
        };
        known.then_some((
            target.completedStats.presentCount,
            target.completedStats.refreshCount,
        ))
    }
}

impl Pace {
    /// Avant les changements d'une image : où en est le compositeur, si
    /// l'image d'avant lui a envoyé quelque chose.
    fn seen(&mut self, stats: &DCOMPOSITION_FRAME_STATISTICS) {
        if !std::mem::take(&mut self.sent) {
            self.before = None;
            return;
        }
        let frequency = stats.timeFrequency as f64;
        let rate = &stats.currentCompositionRate;
        if frequency <= 0.0 || rate.Denominator == 0 || rate.Numerator == 0 {
            return;
        }
        self.rate = f64::from(rate.Numerator) / f64::from(rate.Denominator);
        let period = 1.0 / self.rate;
        let now = stats.currentTime as f64 / frequency;
        let frame = stats.lastFrameTime as f64 / frequency;
        let next = stats.nextEstimatedFrameTime as f64 / frequency;
        self.sends += 1;
        self.since_frame.push(((now - frame) * 1000.0) as f32);
        self.until_frame.push(((next - now) * 1000.0) as f32);
        if let Some((sent, seen)) = self.before
            && (0.5..1.5).contains(&((now - sent) / period))
        {
            let step = ((frame - seen) / period).round().clamp(0.0, 3.0) as usize;
            self.steps[step] += 1;
        }
        self.before = Some((now, frame));
        let since = *self.since.get_or_insert_with(Instant::now);
        if since.elapsed() >= Duration::from_secs(1) {
            self.tell();
        }
    }

    /// Une ligne au journal, et on repart de zéro.
    fn tell(&mut self) {
        fn spread(values: &mut [f32]) -> String {
            if values.is_empty() {
                return "—".to_owned();
            }
            values.sort_by(f32::total_cmp);
            format!(
                "{:.1} / {:.1} / {:.1}",
                values[0],
                values[values.len() / 2],
                values[values.len() - 1]
            )
        }
        // Ce que l'écran a vraiment reçu pendant ce temps (Windows 11).
        let shown = ScreenCounts::get().and_then(ScreenCounts::shown);
        let screen = match (shown, self.shown) {
            // (Des comptes qui reculent, ou qui sautent : un autre écran,
            // une autre image du compositeur. Rien à en dire.)
            (Some((images, refreshes)), Some((images_before, refreshes_before)))
                if (1..1000).contains(&refreshes.wrapping_sub(refreshes_before))
                    && images.wrapping_sub(images_before) < 1000 =>
            {
                format!(
                    " ; écran : {} images montrées en {} rafraîchissements",
                    images.wrapping_sub(images_before),
                    refreshes.wrapping_sub(refreshes_before)
                )
            }
            _ => String::new(),
        };
        log::info!(
            "envois : {} (compositeur à {:.1} images/s) ; avant chaque envoi, sa dernière image \
             datait de {} ms et la suivante était prévue dans {} ms ; d'un envoi au suivant il a \
             avancé de 0 image {} fois, 1 : {}, 2 : {}, plus : {}{screen}",
            self.sends,
            self.rate,
            spread(&mut self.since_frame),
            spread(&mut self.until_frame),
            self.steps[0],
            self.steps[1],
            self.steps[2],
            self.steps[3],
        );
        *self = Self {
            before: self.before,
            sent: self.sent,
            rate: self.rate,
            shown,
            ..Self::default()
        };
    }
}

// SAFETY: les objets de DirectComposition et l'appareil Direct3D 11 se
// laissent appeler de n'importe quel fil. Le contexte de dessin, lui, ne
// supporte qu'un fil à la fois : il est protégé par son propre verrou
// (`SetMultithreadProtected`, posé à la création), et Mouflon ne s'en sert de
// toute façon que sous le verrou des transactions de la scène.
unsafe impl Send for Compositor {}
// SAFETY: voir `Send` ci-dessus.
unsafe impl Sync for Compositor {}

impl Compositor {
    /// L'appareil de la carte graphique, sinon celui du processeur.
    fn new(generation: u64) -> Result<Self, String> {
        let software_only = std::env::var("MOUFLON_RENDU").is_ok_and(|v| v == "logiciel");
        let mut problem = String::new();
        for (driver, name) in [
            (D3D_DRIVER_TYPE_HARDWARE, "la carte graphique"),
            (D3D_DRIVER_TYPE_WARP, "le processeur"),
        ] {
            if software_only && driver == D3D_DRIVER_TYPE_HARDWARE {
                continue;
            }
            let (mut d3d, mut context) = (None, None);
            // SAFETY: pas d'adaptateur imposé ni de module logiciel ; les
            // deux sorties sont à nous et reçoivent l'appareil et son
            // contexte (ou rien, avec l'erreur).
            let made = unsafe {
                D3D11CreateDevice(
                    None::<&IDXGIAdapter>,
                    driver,
                    HMODULE::default(),
                    D3D11_CREATE_DEVICE_BGRA_SUPPORT,
                    None,
                    D3D11_SDK_VERSION,
                    Some(&mut d3d),
                    None,
                    Some(&mut context),
                )
            };
            let (Ok(()), Some(d3d), Some(context)) = (&made, d3d, context) else {
                problem = format!("Direct3D par {name} : {made:?}");
                continue;
            };
            // SAFETY: l'appareil vient d'être créé ; le compositeur est fait
            // pour lui, et son contexte reçoit un verrou.
            let device = unsafe {
                if let Ok(threads) = context.cast::<ID3D11Multithread>() {
                    let _ = threads.SetMultithreadProtected(true);
                }
                d3d.cast::<IDXGIDevice>().and_then(|dxgi| {
                    DCompositionCreateDevice2::<_, IDCompositionDesktopDevice>(&dxgi)
                })
            };
            match device {
                Ok(device) => {
                    log::info!(
                        "calques : DirectComposition, images par {name} ({}) ; Windows {}",
                        describe(&d3d),
                        super::ui::windows_build()
                    );
                    let lose_at = (generation == 1)
                        .then(|| std::env::var("MOUFLON_PERDRE_AFFICHAGE").ok())
                        .flatten()
                        .and_then(|seconds| seconds.trim().parse::<f64>().ok())
                        .filter(|seconds| seconds.is_finite() && *seconds >= 0.0)
                        .map(|seconds| Instant::now() + Duration::from_secs_f64(seconds));
                    return Ok(Self {
                        device,
                        d3d,
                        context,
                        generation,
                        lost: AtomicBool::new(false),
                        dirty: AtomicBool::new(false),
                        transactions: AtomicU64::new(0),
                        lose_at,
                        pace: std::env::var_os("MOUFLON_ESSAI").map(|_| Mutex::default()),
                    });
                }
                Err(e) => problem = format!("DirectComposition par {name} : {e}"),
            }
        }
        Err(problem)
    }

    fn is_lost(&self) -> bool {
        self.lost.load(Ordering::Relaxed)
    }

    fn lose(&self, why: &str) {
        if !self.lost.swap(true, Ordering::Relaxed) {
            log::warn!("calques : affichage perdu ({why}), à refaire");
            LOST.store(true, Ordering::Relaxed);
        }
    }

    /// Le résultat d'un appel au compositeur (qui a peut-être changé quelque
    /// chose : il y aura à envoyer) : une erreur qui dit que l'appareil n'est
    /// plus là le marque perdu.
    fn check<T>(&self, result: windows::core::Result<T>) -> Option<T> {
        self.dirty.store(true, Ordering::Relaxed);
        match result {
            Ok(value) => Some(value),
            Err(e) => {
                if device_gone(e.code()) {
                    self.lose(&e.to_string());
                } else {
                    note_refusal(&e);
                }
                None
            }
        }
    }

    /// Essais : avant les changements d'une image, ce que le compositeur a
    /// fait de l'envoi précédent.
    fn watch(&self) {
        if let Some(pace) = &self.pace
            && !self.is_lost()
            // SAFETY: l'appareil est vivant ; simple question.
            && let Ok(stats) = unsafe { self.device.GetFrameStatistics() }
            && let Ok(mut pace) = pace.lock()
        {
            pace.seen(&stats);
        }
    }

    /// Envoie au compositeur tout ce qui a changé depuis la dernière fois
    /// (rien, s'il n'y a rien : une image où personne n'a bougé ne lui coûte
    /// pas même un message).
    fn commit(&self) {
        if self.is_lost() {
            return;
        }
        if self.dirty.swap(false, Ordering::Relaxed) {
            // SAFETY: l'appareil est vivant ; l'appel n'a pas d'argument.
            let done = unsafe { self.device.Commit() };
            if let Err(e) = done {
                if device_gone(e.code()) {
                    self.lose(&e.to_string());
                } else {
                    note_refusal(&e);
                }
            }
            if let Some(pace) = &self.pace
                && let Ok(mut pace) = pace.lock()
            {
                pace.sent = true;
            }
            if let Some(event) = SENT.get() {
                // SAFETY: un événement de Windows à nous, jamais refermé.
                unsafe { windows_sys::Win32::System::Threading::SetEvent(*event as _) };
            }
        }
        if self
            .transactions
            .fetch_add(1, Ordering::Relaxed)
            .is_multiple_of(CHECK_EVERY)
        {
            // SAFETY: l'appareil est vivant ; simple question.
            if let Err(e) = unsafe { self.d3d.GetDeviceRemovedReason() } {
                self.lose(&e.to_string());
            }
            if self.lose_at.is_some_and(|at| Instant::now() >= at) {
                self.lose("essai : perte demandée");
            }
        }
    }
}

/// L'événement de Windows signalé à chaque envoi au compositeur, une fois
/// qu'une horloge l'a demandé ([`sent_event`]).
static SENT: OnceLock<isize> = OnceLock::new();

/// Un événement de Windows signalé à chaque envoi au compositeur (il se
/// remet de lui-même quand on l'a attendu) : pour une horloge qui bat au
/// rythme où le compositeur prend les envois. `None` si Windows le refuse.
pub(super) fn sent_event() -> Option<isize> {
    let event = *SENT.get_or_init(|| {
        // SAFETY: ni attributs ni nom ; rend une poignée, ou rien. Elle
        // vit autant que le processus.
        unsafe {
            windows_sys::Win32::System::Threading::CreateEventW(
                std::ptr::null(),
                0,
                0,
                std::ptr::null(),
            ) as isize
        }
    });
    (event != 0).then_some(event)
}

/// Attend que le compositeur de Windows ait pris le dernier envoi. `false`
/// s'il n'y a pas de compositeur, ou s'il le refuse.
pub(super) fn wait_taken() -> bool {
    let Some(compositor) = current() else {
        return false;
    };
    // SAFETY: l'appareil est vivant (gardé ici) ; l'appel ne fait
    // qu'attendre.
    unsafe { compositor.device.WaitForCommitCompletion() }.is_ok()
}

/// Ce qui porte les images, pour le journal : le nom que la carte se donne
/// (« Microsoft Basic Render Driver » quand Windows calcule tout sur le
/// processeur) et le niveau de Direct3D qu'elle offre. Sur une machine qu'on
/// n'a pas sous la main, c'est par là qu'on commence.
fn describe(d3d: &ID3D11Device) -> String {
    // SAFETY: l'appareil est vivant ; de simples questions, dont les
    // réponses sont rendues par copie.
    let (name, level) = unsafe {
        let name = d3d
            .cast::<IDXGIDevice>()
            .and_then(|dxgi| dxgi.GetAdapter())
            .and_then(|adapter| adapter.GetDesc())
            .map(|desc| adapter_name(&desc.Description))
            .unwrap_or_default();
        (name, d3d.GetFeatureLevel().0)
    };
    format!("« {name} », Direct3D {}", feature_level(level))
}

/// Un nom donné par Windows (UTF-16, terminé par un zéro).
fn adapter_name(wide: &[u16]) -> String {
    let end = wide.iter().position(|&c| c == 0).unwrap_or(wide.len());
    String::from_utf16_lossy(&wide[..end]).trim().to_owned()
}

/// Un niveau de Direct3D comme on le lit : « 11.1 » pour `0xb100`.
fn feature_level(level: i32) -> String {
    format!("{}.{}", (level >> 12) & 0xF, (level >> 8) & 0xF)
}

/// Les refus du compositeur qui ne tiennent pas à un appareil perdu.
static REFUSALS: AtomicU64 = AtomicU64::new(0);

/// Faut-il noter le refus n° `count` au journal ? Les premiers, puis de loin
/// en loin : un refus qui se répète à chaque image ne doit pas remplir le
/// fichier, mais il doit s'y lire (sur une machine qu'on n'a pas sous la
/// main, c'est tout ce qu'on saura).
fn worth_noting(count: u64) -> bool {
    count <= 4 || count.is_power_of_two()
}

/// Le compositeur a refusé quelque chose, sans avoir perdu son appareil.
fn note_refusal(error: &windows::core::Error) {
    let count = REFUSALS.fetch_add(1, Ordering::Relaxed) + 1;
    if worth_noting(count) {
        log::warn!("calques : le compositeur refuse ({error}) ; refus n° {count}");
    }
}

/// L'appareil graphique a-t-il disparu (pilote mis à jour, carte retirée,
/// session passée sur un autre écran) ?
fn device_gone(code: HRESULT) -> bool {
    // DXGI_ERROR_DEVICE_REMOVED, _HUNG, _RESET, _DRIVER_INTERNAL_ERROR, et
    // D2DERR_RECREATE_TARGET.
    matches!(
        code.0 as u32,
        0x887A_0005 | 0x887A_0006 | 0x887A_0007 | 0x887A_0020 | 0x8899_000C
    )
}

/// La cible d'une fenêtre : ce qui relie ses calques à l'écran.
struct Target {
    hwnd: isize,
    generation: u64,
    target: IDCompositionTarget,
}

// SAFETY: voir `Compositor` : les objets de DirectComposition se laissent
// appeler (et libérer) de n'importe quel fil.
unsafe impl Send for Target {}

#[derive(Default)]
struct State {
    compositor: Option<Arc<Compositor>>,
    /// Rang du dernier compositeur fait.
    generation: u64,
    /// Pourquoi il n'a pas pu l'être, la dernière fois.
    problem: Option<String>,
    targets: Vec<Target>,
}

static STATE: Mutex<State> = Mutex::new(State {
    compositor: None,
    generation: 0,
    problem: None,
    targets: Vec::new(),
});
/// Le compositeur en service a perdu son appareil.
static LOST: AtomicBool = AtomicBool::new(false);
/// Rang du compositeur en service (0 : aucun encore).
static GENERATION: AtomicU64 = AtomicU64::new(0);
/// Une seule transaction à la fois (fil de l'affichage, ou fil principal
/// quand il pose ou retire la scène d'une fenêtre).
static TRANSACTION: Mutex<()> = Mutex::new(());

thread_local! {
    /// Ce fil est dans une transaction (elles s'emboîtent).
    static INSIDE: Cell<bool> = const { Cell::new(false) };
}

fn state() -> MutexGuard<'static, State> {
    STATE.lock().unwrap_or_else(PoisonError::into_inner)
}

/// Le compositeur en service, s'il y en a un qui marche.
fn current() -> Option<Arc<Compositor>> {
    state()
        .compositor
        .as_ref()
        .filter(|compositor| !compositor.is_lost())
        .cloned()
}

/// Fait `work` sous le verrou des transactions. `true` en second si c'est
/// la transaction la plus extérieure de ce fil (c'est elle qui envoie tout
/// au compositeur).
fn locked<R>(work: impl FnOnce(bool) -> R) -> R {
    if INSIDE.get() {
        return work(false);
    }
    /// Remet le fil hors transaction, même si `work` panique.
    struct Leave;
    impl Drop for Leave {
        fn drop(&mut self) {
            INSIDE.set(false);
        }
    }
    let _guard = TRANSACTION.lock().unwrap_or_else(PoisonError::into_inner);
    INSIDE.set(true);
    let _leave = Leave;
    work(true)
}

/// Fait `change` en une seule transaction : tout ce qui change à cette image
/// arrive ensemble au compositeur, qui l'applique d'un coup à son prochain
/// rafraîchissement.
pub fn scene_transaction<R>(change: impl FnOnce() -> R) -> R {
    locked(|outermost| {
        if outermost && let Some(compositor) = current() {
            compositor.watch();
        }
        let result = change();
        if outermost && let Some(compositor) = current() {
            compositor.commit();
        }
        result
    })
}

/// Le compositeur a perdu son appareil graphique : les calques ne montrent
/// plus rien, il faut reposer la scène dans chaque fenêtre
/// ([`attach_scene`]), ce qui refait tout.
#[must_use]
pub fn scene_lost() -> bool {
    LOST.load(Ordering::Relaxed)
}

/// Le rang du compositeur en service : quand il change, les calques et les
/// images faits avant ne servent plus.
#[must_use]
pub fn scene_generation() -> u64 {
    GENERATION.load(Ordering::Relaxed)
}

/// Pourquoi la scène n'a pas pu être posée, si elle ne l'a pas pu.
#[must_use]
pub fn scene_problem() -> Option<String> {
    state().problem.clone()
}

/// Une image prête pour un calque : pixels BGRA prémultipliés, gardés par le
/// compositeur (dans la mémoire de la carte graphique).
#[derive(Clone)]
pub struct SceneImage {
    surface: IDCompositionSurface,
    generation: u64,
    bytes: usize,
}

impl std::fmt::Debug for SceneImage {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("SceneImage")
            .field("bytes", &self.bytes)
            .finish_non_exhaustive()
    }
}

impl SceneImage {
    /// `bgra` : `width` × `height` pixels BGRA prémultipliés, ligne par
    /// ligne, sans bourrage. `None` si les tailles ne vont pas ensemble, ou
    /// si le compositeur manque.
    #[must_use]
    pub fn from_bgra(width: u32, height: u32, bgra: &[u8]) -> Option<Self> {
        let pixels = width as usize * height as usize;
        if width == 0 || height == 0 || width > MAX_SIDE || height > MAX_SIDE {
            return None;
        }
        if bgra.len() != pixels * 4 {
            return None;
        }
        let compositor = current()?;
        locked(|_| {
            // SAFETY: l'appareil est vivant. La surface est créée à la taille
            // de l'image ; son premier dessin la couvre entière (`None`), et
            // le compositeur dit où elle se trouve dans la texture qu'il
            // prête (`offset`). Les pixels copiés sont exactement ceux de
            // `bgra` (taille vérifiée plus haut), ligne par ligne, dans ce
            // rectangle-là. La texture est rendue avant `EndDraw`.
            unsafe {
                let surface = compositor.check(compositor.device.CreateSurface(
                    width,
                    height,
                    DXGI_FORMAT_B8G8R8A8_UNORM,
                    DXGI_ALPHA_MODE_PREMULTIPLIED,
                ))?;
                let mut offset = POINT::default();
                let texture: ID3D11Texture2D =
                    compositor.check(surface.BeginDraw(None, &mut offset))?;
                let (left, top) = (offset.x.max(0) as u32, offset.y.max(0) as u32);
                let area = D3D11_BOX {
                    left,
                    top,
                    front: 0,
                    right: left + width,
                    bottom: top + height,
                    back: 1,
                };
                compositor.context.UpdateSubresource(
                    &texture,
                    0,
                    Some(&area),
                    bgra.as_ptr().cast(),
                    width * 4,
                    0,
                );
                drop(texture);
                compositor.check(surface.EndDraw())?;
                Some(Self {
                    surface,
                    generation: compositor.generation,
                    bytes: bgra.len(),
                })
            }
        })
    }

    /// Octets de ses pixels.
    #[must_use]
    pub fn byte_size(&self) -> usize {
        self.bytes
    }
}

/// Un calque, et ce qui le relie aux autres : le compositeur ne dit pas qui
/// porte qui, ni dans quel ordre.
struct Layer {
    visual: IDCompositionVisual2,
    /// Le même calque, pour son opacité (Windows 8.1 et plus récent).
    fading: Option<IDCompositionVisual3>,
    compositor: Arc<Compositor>,
    links: Mutex<Links>,
}

#[derive(Default)]
struct Links {
    parent: Weak<Layer>,
    /// Du dessous au dessus.
    children: Vec<Arc<Layer>>,
}

// SAFETY: voir `Compositor` : un calque se laisse appeler de n'importe quel
// fil, et ses liens sont sous leur propre verrou. Une fois donné au fil de
// l'affichage, lui seul y touche, dans ses transactions.
unsafe impl Send for Layer {}
// SAFETY: voir `Send` ci-dessus.
unsafe impl Sync for Layer {}

impl Layer {
    fn links(&self) -> MutexGuard<'_, Links> {
        self.links.lock().unwrap_or_else(PoisonError::into_inner)
    }

    fn new(compositor: &Arc<Compositor>) -> Option<Arc<Self>> {
        // SAFETY: l'appareil est vivant ; il rend un calque neuf.
        let visual = compositor.check(unsafe { compositor.device.CreateVisual() })?;
        let fading = visual.cast::<IDCompositionVisual3>().ok();
        Some(Arc::new(Self {
            visual,
            fading,
            compositor: Arc::clone(compositor),
            links: Mutex::new(Links::default()),
        }))
    }
}

/// Un calque de la scène. Son origine est le coin de son repère : sa place
/// et sa transformation partent de là. Sans compositeur, il ne fait rien.
#[derive(Clone)]
pub struct SceneLayer {
    layer: Option<Arc<Layer>>,
}

impl Default for SceneLayer {
    fn default() -> Self {
        Self::new()
    }
}

impl std::fmt::Debug for SceneLayer {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("SceneLayer")
            .field("live", &self.layer.is_some())
            .finish()
    }
}

impl SceneLayer {
    #[must_use]
    pub fn new() -> Self {
        Self {
            layer: current().and_then(|compositor| Layer::new(&compositor)),
        }
    }

    /// Le calque, pour un appel au compositeur ; rien s'il n'y en a pas ou
    /// si son appareil est perdu.
    fn live(&self) -> Option<&Arc<Layer>> {
        self.layer
            .as_ref()
            .filter(|layer| !layer.compositor.is_lost())
    }

    /// `self` et `child`, s'ils sont du même compositeur.
    fn pair<'a>(&'a self, child: &'a Self) -> Option<(&'a Arc<Layer>, &'a Arc<Layer>)> {
        let (parent, kid) = (self.live()?, child.live()?);
        (parent.compositor.generation == kid.compositor.generation && !Arc::ptr_eq(parent, kid))
            .then_some((parent, kid))
    }

    /// Ajoute `child` par-dessus ses autres calques.
    pub fn add(&self, child: &Self) {
        let Some((parent, kid)) = self.pair(child) else {
            return;
        };
        child.remove();
        parent.links().children.push(Arc::clone(kid));
        kid.links().parent = Arc::downgrade(parent);
        // SAFETY: deux calques vivants du même compositeur. Sans calque de
        // repère, « pas au-dessus » veut dire au-dessus de tous les autres.
        let done = unsafe {
            parent
                .visual
                .AddVisual(&kid.visual, false, None::<&IDCompositionVisual>)
        };
        parent.compositor.check(done);
    }

    /// Place `child` au rang `index` parmi ses calques (0 : tout en dessous).
    pub fn insert(&self, child: &Self, index: u32) {
        let Some((parent, kid)) = self.pair(child) else {
            return;
        };
        child.remove();
        let mut links = parent.links();
        let index = (index as usize).min(links.children.len());
        // SAFETY: des calques vivants du même compositeur ; le repère est un
        // calque déjà porté par `parent`. Sans repère, « au-dessus » veut
        // dire au-dessus d'aucun : en dessous de tous.
        let done = unsafe {
            match index
                .checked_sub(1)
                .and_then(|below| links.children.get(below))
            {
                Some(below) => parent.visual.AddVisual(&kid.visual, true, &below.visual),
                None => parent
                    .visual
                    .AddVisual(&kid.visual, true, None::<&IDCompositionVisual>),
            }
        };
        links.children.insert(index, Arc::clone(kid));
        drop(links);
        kid.links().parent = Arc::downgrade(parent);
        parent.compositor.check(done);
    }

    /// Retire le calque de son parent.
    pub fn remove(&self) {
        let Some(kid) = &self.layer else {
            return;
        };
        let parent = std::mem::take(&mut kid.links().parent).upgrade();
        let Some(parent) = parent else {
            return;
        };
        parent
            .links()
            .children
            .retain(|child| !Arc::ptr_eq(child, kid));
        if parent.compositor.is_lost() {
            return;
        }
        // SAFETY: deux calques vivants, dont le second était porté par le
        // premier.
        let done = unsafe { parent.visual.RemoveVisual(&kid.visual) };
        parent.compositor.check(done);
    }

    /// Son origine (le coin de son repère), en points, dans le repère du parent.
    pub fn set_position(&self, x: f64, y: f64) {
        if let Some(layer) = self.live() {
            // SAFETY: calque vivant ; deux nombres.
            unsafe {
                layer.compositor.check(layer.visual.SetOffsetX2(x as f32));
                layer.compositor.check(layer.visual.SetOffsetY2(y as f32));
            }
        }
    }

    /// Taille, dans son propre repère. Ici, un calque a toujours la taille de
    /// son image (en pixels de l'image) : rien à dire au compositeur.
    pub fn set_size(&self, _width: f64, _height: f64) {}

    /// Transformation affine `[a, b, c, d, tx, ty]` (x' = a x + c y + tx,
    /// y' = b x + d y + ty), de son repère vers celui de son parent.
    pub fn set_transform(&self, [a, b, c, d, tx, ty]: [f64; 6]) {
        if let Some(layer) = self.live() {
            let matrix = Matrix3x2 {
                M11: a as f32,
                M12: b as f32,
                M21: c as f32,
                M22: d as f32,
                M31: tx as f32,
                M32: ty as f32,
            };
            // SAFETY: calque vivant ; la matrice vit jusqu'au retour.
            let done = unsafe { layer.visual.SetTransform2(&matrix) };
            layer.compositor.check(done);
        }
    }

    pub fn set_opacity(&self, opacity: f32) {
        if let Some(layer) = self.live()
            && let Some(fading) = &layer.fading
        {
            // SAFETY: calque vivant ; un nombre entre 0 et 1.
            let done = unsafe { fading.SetOpacity2(opacity.clamp(0.0, 1.0)) };
            layer.compositor.check(done);
        }
    }

    pub fn set_hidden(&self, hidden: bool) {
        if let Some(layer) = self.live()
            && let Some(fading) = &layer.fading
        {
            // SAFETY: calque vivant.
            let done = unsafe { fading.SetVisible(!hidden) };
            layer.compositor.check(done);
        }
    }

    /// Son image, à sa taille ; aucune pour un calque qui ne fait que porter
    /// d'autres calques.
    pub fn set_image(&self, image: Option<&SceneImage>) {
        let Some(layer) = self.live() else {
            return;
        };
        // SAFETY: calque vivant ; l'image, s'il y en a une, est une surface
        // du même compositeur (vérifié), que le calque garde en vie.
        let done = unsafe {
            match image {
                Some(image) if image.generation == layer.compositor.generation => {
                    layer.visual.SetContent(&image.surface)
                }
                _ => layer.visual.SetContent(None::<&IUnknown>),
            }
        };
        layer.compositor.check(done);
    }

    /// Image lissée quand elle est agrandie, réduite ou tournée, ou au plus
    /// proche (du pixel art agrandi reste franc).
    pub fn set_smooth(&self, smooth: bool) {
        if let Some(layer) = self.live() {
            let mode = if smooth {
                DCOMPOSITION_BITMAP_INTERPOLATION_MODE_LINEAR
            } else {
                DCOMPOSITION_BITMAP_INTERPOLATION_MODE_NEAREST_NEIGHBOR
            };
            // SAFETY: calque vivant ; une constante.
            let done = unsafe { layer.visual.SetBitmapInterpolationMode(mode) };
            layer.compositor.check(done);
        }
    }

    /// Ses calques se composent d'abord ensemble, puis prennent son
    /// opacité d'un bloc (le corps d'un animal qui s'efface ne montre pas
    /// ses pattes à travers lui).
    pub fn set_group_opacity(&self, group: bool) {
        if let Some(layer) = self.live() {
            let mode = if group {
                DCOMPOSITION_OPACITY_MODE_LAYER
            } else {
                DCOMPOSITION_OPACITY_MODE_MULTIPLY
            };
            // SAFETY: calque vivant ; une constante.
            let done = unsafe { layer.visual.SetOpacityMode(mode) };
            layer.compositor.check(done);
        }
    }
}

/// Pose la racine des calques de la scène dans une fenêtre, sur toute sa
/// taille : origine en haut à gauche, y vers le bas, en points (la racine
/// porte l'échelle du bureau). À appeler sur le fil principal, qui possède
/// la fenêtre. Une scène déjà posée dans cette fenêtre est remplacée ; si le
/// compositeur a perdu son appareil, il est d'abord refait (pour toutes les
/// fenêtres : chacune doit recevoir une nouvelle racine).
#[must_use]
pub fn attach_scene(window: &impl HasWindowHandle) -> Option<SceneLayer> {
    let hwnd = super::hwnd_of(window)?;
    let scale = super::world_scale() as f32;
    locked(|_| {
        let compositor = {
            let mut state = state();
            if state.compositor.as_ref().is_some_and(|c| c.is_lost()) {
                // Les cibles de l'ancien appareil d'abord : une fenêtre n'en
                // porte qu'une.
                state.targets.clear();
                state.compositor = None;
            }
            if state.compositor.is_none() {
                state.generation += 1;
                match Compositor::new(state.generation) {
                    Ok(compositor) => {
                        state.problem = None;
                        state.compositor = Some(Arc::new(compositor));
                        GENERATION.store(state.generation, Ordering::Relaxed);
                        LOST.store(false, Ordering::Relaxed);
                    }
                    Err(problem) => {
                        log::error!("calques indisponibles : {problem}");
                        state.problem = Some(problem);
                        return None;
                    }
                }
            }
            Arc::clone(state.compositor.as_ref()?)
        };
        let root = Layer::new(&compositor)?;
        let matrix = Matrix3x2 {
            M11: scale,
            M12: 0.0,
            M21: 0.0,
            M22: scale,
            M31: 0.0,
            M32: 0.0,
        };
        let key = hwnd as isize;
        let mut state = state();
        let known = state
            .targets
            .iter()
            .position(|t| t.hwnd == key && t.generation == compositor.generation);
        // SAFETY: la fenêtre est vivante et à ce processus (winit la tient,
        // sur ce fil) ; les calques sont ceux du compositeur en service. La
        // racine prend l'échelle du bureau, puis la place de celle d'avant.
        let attached = unsafe {
            compositor.check(root.visual.SetTransform2(&matrix));
            let target = match known {
                Some(at) => Some(state.targets[at].target.clone()),
                None => {
                    // (Une cible d'un autre appareil pour cette fenêtre
                    // s'en va d'abord.)
                    state.targets.retain(|t| t.hwnd != key);
                    let made = compositor.device.CreateTargetForHwnd(HWND(hwnd), true);
                    let target = compositor.check(made);
                    if let Some(target) = &target {
                        state.targets.push(Target {
                            hwnd: key,
                            generation: compositor.generation,
                            target: target.clone(),
                        });
                    }
                    target
                }
            };
            target.and_then(|target| compositor.check(target.SetRoot(&root.visual)))
        };
        drop(state);
        if attached.is_none() {
            let problem = "la fenêtre de l'écran refuse ses calques".to_owned();
            log::error!("calques indisponibles : {problem}");
            self::state().problem = Some(problem);
            return None;
        }
        compositor.commit();
        Some(SceneLayer { layer: Some(root) })
    })
}

/// La fenêtre s'en va (écran débranché, changé) : sa scène la quitte.
pub fn detach_scene(window: &impl HasWindowHandle) {
    let Some(hwnd) = super::hwnd_of(window) else {
        return;
    };
    locked(|_| {
        let mut state = state();
        let before = state.targets.len();
        state.targets.retain(|t| t.hwnd != hwnd as isize);
        let removed = state.targets.len() != before;
        let compositor = state.compositor.clone();
        drop(state);
        if removed && let Some(compositor) = compositor {
            compositor.commit();
        }
    });
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_lost_device_is_told_apart_from_an_ordinary_refusal() {
        assert!(device_gone(HRESULT(0x887A_0005_u32 as i32)));
        assert!(device_gone(HRESULT(0x887A_0007_u32 as i32)));
        // Un argument refusé, plus de mémoire : l'appareil est toujours là.
        assert!(!device_gone(HRESULT(0x8007_0057_u32 as i32)));
        assert!(!device_gone(HRESULT(0x8007_000E_u32 as i32)));
    }

    #[test]
    fn the_log_names_the_graphics_card_and_its_direct3d_level() {
        let mut wide = [0u16; 128];
        for (to, from) in wide
            .iter_mut()
            .zip("Microsoft Basic Render Driver ".encode_utf16())
        {
            *to = from;
        }
        assert_eq!(adapter_name(&wide), "Microsoft Basic Render Driver");
        // (Un nom qui remplit tout, sans zéro au bout, se lit quand même.)
        assert_eq!(adapter_name(&[u16::from(b'A'); 4]), "AAAA");
        assert_eq!(feature_level(0xb100), "11.1");
        assert_eq!(feature_level(0xb000), "11.0");
        assert_eq!(feature_level(0x9300), "9.3");
    }

    #[test]
    fn a_refusal_that_repeats_is_noted_less_and_less_often() {
        let noted: Vec<u64> = (1..=5000).filter(|&n| worth_noting(n)).collect();
        assert_eq!(
            noted,
            [1, 2, 3, 4, 8, 16, 32, 64, 128, 256, 512, 1024, 2048, 4096]
        );
    }

    #[test]
    fn a_layer_without_a_compositor_does_nothing_and_an_image_needs_its_exact_pixels() {
        // Aucune scène posée dans ces essais : pas de compositeur en service.
        let (parent, child) = (SceneLayer { layer: None }, SceneLayer { layer: None });
        parent.add(&child);
        parent.insert(&child, 3);
        child.set_position(1.0, 2.0);
        child.set_transform([1.0, 0.0, 0.0, 1.0, 0.0, 0.0]);
        child.set_opacity(0.5);
        child.set_image(None);
        child.remove();
        assert!(SceneImage::from_bgra(0, 4, &[]).is_none());
        assert!(SceneImage::from_bgra(2, 2, &[0; 15]).is_none());
        assert!(SceneImage::from_bgra(MAX_SIDE + 1, 1, &[0; 4]).is_none());
    }
}
