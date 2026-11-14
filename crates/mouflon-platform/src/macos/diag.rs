//! Diagnostics des essais, côté macOS (voir [`crate::diag`]) : pile du fil
//! principal, observation de sa boucle d'événements, ressources du
//! processus, de ses fils et du système, notifications du système.

use std::cell::{OnceCell, RefCell};
use std::ffi::{CStr, c_char, c_int, c_void};
use std::mem::MaybeUninit;
use std::sync::atomic::{AtomicBool, AtomicU32, AtomicU64, AtomicUsize, Ordering::Relaxed};
use std::sync::{Mutex, OnceLock};

use objc2::rc::{Retained, autoreleasepool};
use objc2::runtime::{AnyObject, NSObject, NSObjectProtocol};
use objc2::{MainThreadMarker, MainThreadOnly, define_class, msg_send, sel};
use objc2_app_kit::{
    NSApplication, NSApplicationDidBecomeActiveNotification,
    NSApplicationDidChangeOcclusionStateNotification,
    NSApplicationDidChangeScreenParametersNotification, NSApplicationDidResignActiveNotification,
    NSApplicationOcclusionState, NSRunningApplication, NSScreen, NSWindow,
    NSWindowDidChangeOcclusionStateNotification, NSWindowDidChangeScreenNotification,
    NSWindowOcclusionState, NSWorkspace,
    NSWorkspaceAccessibilityDisplayOptionsDidChangeNotification,
    NSWorkspaceActiveSpaceDidChangeNotification, NSWorkspaceApplicationKey,
    NSWorkspaceDidActivateApplicationNotification, NSWorkspaceDidLaunchApplicationNotification,
    NSWorkspaceDidTerminateApplicationNotification, NSWorkspaceDidWakeNotification,
    NSWorkspaceScreensDidSleepNotification, NSWorkspaceScreensDidWakeNotification,
    NSWorkspaceSessionDidBecomeActiveNotification, NSWorkspaceSessionDidResignActiveNotification,
    NSWorkspaceWillSleepNotification,
};
use objc2_core_foundation::{
    CFRetained, CFRunLoop, CFRunLoopActivity, CFRunLoopObserver, kCFRunLoopCommonModes,
    kCFRunLoopDefaultMode,
};
use objc2_foundation::{
    NSDistributedNotificationCenter, NSNotification, NSNotificationCenter, NSNotificationName,
    NSProcessInfo, NSProcessInfoPowerStateDidChangeNotification,
    NSProcessInfoThermalStateDidChangeNotification, NSString, NSSystemClockDidChangeNotification,
};

use crate::diag::{LoopState, LoopTotals, ProcStats, SystemStats, ThreadStat, now_ns};

// --- Fil principal -------------------------------------------------------

/// Port Mach du fil principal (0 : pas encore connu).
static MAIN_PORT: AtomicU32 = AtomicU32::new(0);
/// Bornes de sa pile : on ne lit rien en dehors.
static STACK_TOP: AtomicUsize = AtomicUsize::new(0);
static STACK_BOTTOM: AtomicUsize = AtomicUsize::new(0);

unsafe extern "C" {
    /// `<mach/thread_act.h>`.
    fn thread_suspend(target_act: libc::thread_act_t) -> libc::kern_return_t;
    fn thread_resume(target_act: libc::thread_act_t) -> libc::kern_return_t;
    fn thread_get_state(
        target_act: libc::thread_act_t,
        flavor: c_int,
        old_state: *mut u32,
        old_state_count: *mut libc::mach_msg_type_number_t,
    ) -> libc::kern_return_t;
    /// `<mach/mach_port.h>`.
    fn mach_port_deallocate(
        task: libc::mach_port_t,
        name: libc::mach_port_t,
    ) -> libc::kern_return_t;
    /// `<pthread/qos.h>`.
    fn pthread_get_qos_class_np(
        thread: libc::pthread_t,
        qos: *mut libc::qos_class_t,
        relative: *mut c_int,
    ) -> c_int;
    /// `<mach/mach_time.h>`.
    fn mach_timebase_info(info: *mut TimebaseInfo) -> libc::kern_return_t;
    fn mach_absolute_time() -> u64;
    /// `<mach/mach_init.h>`.
    fn mach_host_self() -> libc::mach_port_t;
    static mach_task_self_: libc::mach_port_t;
}

/// `mach_timebase_info_data_t`.
#[repr(C)]
struct TimebaseInfo {
    numer: u32,
    denom: u32,
}

/// Le port de notre propre tâche.
fn task_self() -> libc::mach_port_t {
    // SAFETY: variable de la bibliothèque du système, fixée au lancement.
    unsafe { mach_task_self_ }
}

/// Le fil courant est le fil principal : on retient de quoi relever sa
/// pile, et on observe sa boucle d'événements.
pub fn register_main_thread() {
    let Some(mtm) = MainThreadMarker::new() else {
        return;
    };
    // SAFETY: ces fonctions de libpthread ne font que lire ce qu'elles
    // savent du fil courant ; le port Mach rendu n'a pas à être libéré.
    let (port, top, size) = unsafe {
        let me = libc::pthread_self();
        (
            libc::pthread_mach_thread_np(me),
            libc::pthread_get_stackaddr_np(me) as usize,
            libc::pthread_get_stacksize_np(me),
        )
    };
    STACK_TOP.store(top, Relaxed);
    STACK_BOTTOM.store(top.saturating_sub(size), Relaxed);
    MAIN_PORT.store(port, Relaxed);
    install_observers(mtm);
}

/// Le fil dont on relève la pile quand une image tarde : celui qui les fait
/// (le fil de l'affichage), à la place du fil principal.
pub fn register_frame_thread() {
    // SAFETY: ces fonctions de libpthread ne font que lire ce qu'elles
    // savent du fil courant ; le port Mach rendu n'a pas à être libéré.
    let (port, top, size) = unsafe {
        let me = libc::pthread_self();
        (
            libc::pthread_mach_thread_np(me),
            libc::pthread_get_stackaddr_np(me) as usize,
            libc::pthread_get_stacksize_np(me),
        )
    };
    STACK_TOP.store(top, Relaxed);
    STACK_BOTTOM.store(top.saturating_sub(size), Relaxed);
    MAIN_PORT.store(port, Relaxed);
}

/// Le fil des diagnostics passe devant (classe « interactive ») : il doit
/// se réveiller à l'heure pour voir les retards des autres.
pub fn raise_thread_priority() {
    // SAFETY: ne change que la classe de service du fil qui l'appelle.
    unsafe {
        libc::pthread_set_qos_class_self_np(libc::qos_class_t::QOS_CLASS_USER_INTERACTIVE, 0);
    }
}

#[cfg(target_arch = "aarch64")]
#[repr(C)]
#[derive(Default)]
struct ArmThreadState64 {
    x: [u64; 29],
    fp: u64,
    lr: u64,
    sp: u64,
    pc: u64,
    cpsr: u32,
    pad: u32,
}

#[cfg(target_arch = "aarch64")]
const ARM_THREAD_STATE64: c_int = 6;

/// Adresse sans sa signature (authentification des pointeurs d'Apple
/// Silicon) : les adresses des processus tiennent sur 47 bits.
#[cfg(target_arch = "aarch64")]
fn strip(address: u64) -> usize {
    (address & 0x0000_7FFF_FFFF_FFFF) as usize
}

/// Relève la pile du fil principal dans `buffer` (adresse en cours, registre
/// de lien, puis les adresses de retour en remontant la chaîne des cadres) ;
/// renvoie combien. Le fil est arrêté le temps de la lecture, quelques
/// microsecondes : rien n'est alloué ni verrouillé pendant ce temps (il
/// pourrait tenir le verrou de l'allocateur).
#[cfg(target_arch = "aarch64")]
pub fn sample_main(buffer: &mut [usize]) -> usize {
    let port = MAIN_PORT.load(Relaxed);
    let top = STACK_TOP.load(Relaxed);
    let bottom = STACK_BOTTOM.load(Relaxed);
    if port == 0 || top == 0 || buffer.len() < 2 {
        return 0;
    }
    // SAFETY: `port` est celui du fil principal, vivant tant que le
    // processus l'est ; il est relancé plus bas dans tous les cas.
    if unsafe { thread_suspend(port) } != libc::KERN_SUCCESS {
        return 0;
    }
    let mut state = ArmThreadState64::default();
    let mut count = (size_of::<ArmThreadState64>() / 4) as libc::mach_msg_type_number_t;
    // SAFETY: `state` a la taille et la disposition de l'état ARM64 demandé,
    // et `count` le dit en mots de 32 bits.
    let status = unsafe {
        thread_get_state(
            port,
            ARM_THREAD_STATE64,
            (&raw mut state).cast::<u32>(),
            &mut count,
        )
    };
    let mut n = 0;
    if status == libc::KERN_SUCCESS {
        buffer[0] = strip(state.pc);
        buffer[1] = strip(state.lr);
        n = 2;
        let sp = state.sp as usize;
        let mut fp = state.fp as usize;
        while n < buffer.len() && fp >= sp.max(bottom) && fp.is_multiple_of(8) && fp + 16 <= top {
            // SAFETY: `fp` est dans la pile du fil principal (bornes vérifiées
            // ci-dessus), qui reste en place tant qu'il est arrêté : le cadre
            // y est fait de deux mots, le cadre précédent et l'adresse de retour.
            let (next, ret) = unsafe {
                (
                    std::ptr::read_volatile(fp as *const usize),
                    std::ptr::read_volatile((fp + 8) as *const u64),
                )
            };
            let ret = strip(ret);
            if ret == 0 {
                break;
            }
            buffer[n] = ret;
            n += 1;
            if next <= fp {
                break;
            }
            fp = next;
        }
    }
    // SAFETY: le fil a été arrêté ci-dessus, une fois.
    unsafe { thread_resume(port) };
    n
}

#[cfg(not(target_arch = "aarch64"))]
pub fn sample_main(_buffer: &mut [usize]) -> usize {
    0
}

/// `image`symbole+décalage` d'une adresse de code (symboles Rust démêlés).
pub fn symbolize(address: usize) -> String {
    let mut info = MaybeUninit::<libc::Dl_info>::zeroed();
    // SAFETY: `dladdr` ne fait que chercher l'adresse parmi les images
    // chargées et remplir la structure ; l'adresse n'est pas lue.
    if unsafe { libc::dladdr(address as *const c_void, info.as_mut_ptr()) } == 0 {
        return format!("{address:#x}");
    }
    // SAFETY: mise à zéro à la création, puis remplie par `dladdr`.
    let info = unsafe { info.assume_init() };
    let image = if info.dli_fname.is_null() {
        "?".to_owned()
    } else {
        // SAFETY: chaîne C terminée par un nul, celle du chemin de l'image.
        let path = unsafe { CStr::from_ptr(info.dli_fname) }.to_string_lossy();
        path.rsplit('/').next().unwrap_or_default().to_owned()
    };
    if info.dli_sname.is_null() {
        let offset = address.saturating_sub(info.dli_fbase as usize);
        return format!("{image}+{offset:#x}");
    }
    // SAFETY: chaîne C terminée par un nul, le nom du symbole.
    let name = unsafe { CStr::from_ptr(info.dli_sname) }.to_string_lossy();
    let offset = address.saturating_sub(info.dli_saddr as usize);
    format!("{image}`{:#}+{offset}", rustc_demangle::demangle(&name))
}

// --- Boucle d'événements -------------------------------------------------

static LOOP_BUSY: AtomicBool = AtomicBool::new(false);
static LOOP_OBSERVED: AtomicBool = AtomicBool::new(false);
static LOOP_SINCE: AtomicU64 = AtomicU64::new(0);
static LOOP_WAKEUPS: AtomicU64 = AtomicU64::new(0);
static LOOP_BUSY_NS: AtomicU64 = AtomicU64::new(0);
static LOOP_ASLEEP_NS: AtomicU64 = AtomicU64::new(0);
static LOOP_LONGEST: AtomicU64 = AtomicU64::new(0);
static MODE_DEFAULT: AtomicBool = AtomicBool::new(true);
/// Changements de mode pas encore notés (heure, nom).
static MODE_CHANGES: Mutex<Vec<(u64, String)>> = Mutex::new(Vec::new());
/// Le mode en cours, s'il n'est pas celui par défaut.
static CURRENT_MODE: Mutex<String> = Mutex::new(String::new());

thread_local! {
    /// Les observateurs de la boucle principale (gardés pour toujours).
    static OBSERVERS: OnceCell<[CFRetained<CFRunLoopObserver>; 2]> = const { OnceCell::new() };
}

unsafe extern "C-unwind" fn observe(
    _observer: *mut CFRunLoopObserver,
    activity: CFRunLoopActivity,
    _info: *mut c_void,
) {
    let now = now_ns();
    if activity == CFRunLoopActivity::BeforeWaiting {
        let since = LOOP_SINCE.swap(now, Relaxed);
        if LOOP_BUSY.swap(false, Relaxed) {
            let busy = now.saturating_sub(since);
            LOOP_BUSY_NS.fetch_add(busy, Relaxed);
            LOOP_LONGEST.fetch_max(busy, Relaxed);
        }
    } else if activity == CFRunLoopActivity::AfterWaiting {
        let since = LOOP_SINCE.swap(now, Relaxed);
        if !LOOP_BUSY.swap(true, Relaxed) {
            LOOP_ASLEEP_NS.fetch_add(now.saturating_sub(since), Relaxed);
        }
        LOOP_WAKEUPS.fetch_add(1, Relaxed);
        note_mode(now);
    } else if activity == CFRunLoopActivity::Entry {
        note_mode(now);
    }
}

/// Note un changement de mode de la boucle (menu ouvert, fenêtre modale…).
fn note_mode(now: u64) {
    let Some(run_loop) = CFRunLoop::main() else {
        return;
    };
    let Some(mode) = run_loop.current_mode() else {
        return;
    };
    // SAFETY: constante de CoreFoundation, valide pendant toute la vie du processus.
    let default = unsafe { kCFRunLoopDefaultMode }.is_some_and(|d| *d == *mode);
    if MODE_DEFAULT.swap(default, Relaxed) == default {
        return;
    }
    let name = mode.to_string();
    if let Ok(mut changes) = MODE_CHANGES.lock()
        && changes.len() < 1000
    {
        changes.push((now, name.clone()));
    }
    if let Ok(mut current) = CURRENT_MODE.lock() {
        *current = if default { String::new() } else { name };
    }
}

fn install_observers(_mtm: MainThreadMarker) {
    OBSERVERS.with(|cell| {
        if cell.get().is_some() {
            return;
        }
        let Some(run_loop) = CFRunLoop::main() else {
            return;
        };
        // Le réveil et l'entrée d'abord (avant tous les autres observateurs),
        // l'endormissement en dernier (après le rendu de Core Animation) :
        // le temps occupé compte tout ce que fait le fil.
        let first = CFRunLoopActivity::AfterWaiting | CFRunLoopActivity::Entry;
        let last = CFRunLoopActivity::BeforeWaiting;
        // SAFETY: `observe` a la signature attendue et ne touche que des
        // atomiques ; pas de contexte (pointeur nul accepté).
        let make = |activities: CFRunLoopActivity, order: isize| unsafe {
            CFRunLoopObserver::new(
                None,
                activities.0,
                true,
                order,
                Some(observe),
                std::ptr::null_mut(),
            )
        };
        let (Some(early), Some(late)) = (make(first, isize::MIN), make(last, isize::MAX)) else {
            return;
        };
        // SAFETY: constante de CoreFoundation.
        let common = unsafe { kCFRunLoopCommonModes };
        run_loop.add_observer(Some(&early), common);
        run_loop.add_observer(Some(&late), common);
        LOOP_SINCE.store(now_ns(), Relaxed);
        LOOP_BUSY.store(true, Relaxed);
        LOOP_OBSERVED.store(true, Relaxed);
        let _ = cell.set([early, late]);
    });
}

pub fn loop_state() -> LoopState {
    if !LOOP_OBSERVED.load(Relaxed) {
        LoopState::Unknown
    } else if LOOP_BUSY.load(Relaxed) {
        LoopState::Busy(LOOP_SINCE.load(Relaxed))
    } else {
        LoopState::Asleep
    }
}

pub fn loop_totals() -> LoopTotals {
    LoopTotals {
        wakeups: LOOP_WAKEUPS.load(Relaxed),
        busy: LOOP_BUSY_NS.load(Relaxed),
        asleep: LOOP_ASLEEP_NS.load(Relaxed),
    }
}

pub fn take_longest_busy() -> u64 {
    LOOP_LONGEST.swap(0, Relaxed)
}

pub fn take_mode_changes() -> Vec<(u64, String)> {
    MODE_CHANGES
        .lock()
        .map(|mut changes| std::mem::take(&mut *changes))
        .unwrap_or_default()
}

pub fn current_mode() -> String {
    CURRENT_MODE
        .lock()
        .map(|mode| mode.clone())
        .unwrap_or_default()
}

// --- Processus -----------------------------------------------------------

/// Base de temps des horloges Mach : nanosecondes = tops × num / dénom.
fn timebase() -> (u64, u64) {
    static TIMEBASE: OnceLock<(u64, u64)> = OnceLock::new();
    *TIMEBASE.get_or_init(|| {
        let mut info = TimebaseInfo { numer: 0, denom: 0 };
        // SAFETY: remplit la structure donnée.
        unsafe { mach_timebase_info(&mut info) };
        if info.numer == 0 || info.denom == 0 {
            (1, 1)
        } else {
            (u64::from(info.numer), u64::from(info.denom))
        }
    })
}

fn ticks_to_ns(ticks: u64) -> u64 {
    let (numer, denom) = timebase();
    u64::try_from(u128::from(ticks) * u128::from(numer) / u128::from(denom)).unwrap_or(u64::MAX)
}

pub fn media_time() -> f64 {
    // SAFETY: lecture de l'horloge Mach, sans argument.
    let ticks = unsafe { mach_absolute_time() };
    ticks_to_ns(ticks) as f64 / 1e9
}

/// `RUSAGE_INFO_V6` (macOS 13) : avec l'énergie et le temps des cœurs performants.
const RUSAGE_INFO_V6: c_int = 6;

/// Les comptes du noyau sur le processus, en mots de 64 bits (voir
/// `struct rusage_info_v6`, `<sys/resource.h>`), et si la version 6 a répondu.
fn rusage() -> Option<([u64; 128], bool)> {
    let mut buffer = [0u64; 128];
    for flavor in [RUSAGE_INFO_V6, libc::RUSAGE_INFO_V4] {
        // SAFETY: le tampon (1 Kio) est plus grand que toutes les versions de
        // `rusage_info` (464 octets pour la v6) ; le processus est le nôtre.
        let status = unsafe {
            libc::proc_pid_rusage(
                libc::getpid(),
                flavor,
                buffer.as_mut_ptr().cast::<libc::rusage_info_t>(),
            )
        };
        if status == 0 {
            return Some((buffer, flavor == RUSAGE_INFO_V6));
        }
    }
    None
}

pub fn process() -> Option<ProcStats> {
    let (r, v6) = rusage()?;
    let t = ticks_to_ns;
    let mut stats = ProcStats {
        user: t(r[2]),
        system: t(r[3]),
        idle_wakeups: r[4],
        interrupt_wakeups: r[5],
        pageins: r[6],
        footprint: r[9],
        disk_read: r[18],
        disk_written: r[19],
        // Interactive, initiée par l'utilisateur, par défaut, utilitaire,
        // de fond, maintenance, ancienne.
        qos: [
            t(r[26]),
            t(r[25]),
            t(r[20]),
            t(r[23]),
            t(r[22]),
            t(r[21]),
            t(r[24]),
        ],
        logical_writes: r[29],
        lifetime_max_footprint: r[30],
        instructions: r[31],
        cycles: r[32],
        interval_max_footprint: r[35],
        runnable: t(r[36]),
        ..ProcStats::default()
    };
    if v6 {
        stats.performance = Some(t(r[38].saturating_add(r[39])));
        stats.energy_nj = Some(r[42]);
    }
    // SAFETY: lecture de l'identifiant du processus courant.
    let pid = unsafe { libc::getpid() };
    let mut info = MaybeUninit::<libc::proc_taskinfo>::zeroed();
    let size = size_of::<libc::proc_taskinfo>() as c_int;
    // SAFETY: le tampon a exactement la taille d'une `proc_taskinfo`.
    let filled = unsafe {
        libc::proc_pidinfo(
            pid,
            libc::PROC_PIDTASKINFO,
            0,
            info.as_mut_ptr().cast(),
            size,
        )
    };
    if filled == size {
        // SAFETY: remplie par `proc_pidinfo`.
        let info = unsafe { info.assume_init() };
        let n = |v: i32| u64::from(v as u32);
        stats.faults = n(info.pti_faults);
        stats.cow_faults = n(info.pti_cow_faults);
        stats.csw = n(info.pti_csw);
        stats.mach_calls = n(info.pti_syscalls_mach);
        stats.unix_calls = n(info.pti_syscalls_unix);
        stats.messages_sent = n(info.pti_messages_sent);
        stats.messages_received = n(info.pti_messages_received);
        stats.threads = info.pti_threadnum as u32;
        stats.running = info.pti_numrunning as u32;
    }
    stats.fds = open_files(pid);
    Some(stats)
}

/// Descripteurs de fichiers ouverts.
fn open_files(pid: c_int) -> u32 {
    // SAFETY: sans tampon, `proc_pidinfo` dit seulement la taille à prévoir.
    let needed =
        unsafe { libc::proc_pidinfo(pid, libc::PROC_PIDLISTFDS, 0, std::ptr::null_mut(), 0) };
    let Ok(needed) = usize::try_from(needed) else {
        return 0;
    };
    let item = size_of::<libc::proc_fdinfo>();
    let mut list: Vec<libc::proc_fdinfo> = Vec::with_capacity(needed / item + 16);
    let bytes = (list.capacity() * item) as c_int;
    // SAFETY: le tampon a la place annoncée ; seuls les octets remplis comptent.
    let filled = unsafe {
        libc::proc_pidinfo(
            pid,
            libc::PROC_PIDLISTFDS,
            0,
            list.as_mut_ptr().cast(),
            bytes,
        )
    };
    u32::try_from(filled).unwrap_or(0) / item as u32
}

/// Les fils du processus : nom, temps de calcul, état, priorité.
pub fn threads() -> Vec<ThreadStat> {
    let task = task_self();
    let mut list: libc::thread_act_array_t = std::ptr::null_mut();
    let mut count: libc::mach_msg_type_number_t = 0;
    // SAFETY: `task_threads` remplit la liste (allouée par le noyau, rendue
    // plus bas) et son nombre.
    if unsafe { libc::task_threads(task, &mut list, &mut count) } != libc::KERN_SUCCESS
        || list.is_null()
    {
        return Vec::new();
    }
    // SAFETY: le noyau a mis `count` ports de fils à cette adresse.
    let ports = unsafe { std::slice::from_raw_parts(list, count as usize) };
    let main = MAIN_PORT.load(Relaxed);
    let mut out = Vec::with_capacity(ports.len());
    for &port in ports {
        let mut info = MaybeUninit::<libc::thread_extended_info>::zeroed();
        let mut size = libc::THREAD_EXTENDED_INFO_COUNT;
        // SAFETY: le tampon a la taille annoncée pour cette sorte d'information.
        let extended = unsafe {
            libc::thread_info(
                port,
                libc::THREAD_EXTENDED_INFO as u32,
                info.as_mut_ptr().cast(),
                &mut size,
            )
        } == libc::KERN_SUCCESS;
        let mut ident = MaybeUninit::<libc::thread_identifier_info>::zeroed();
        let mut ident_size = libc::THREAD_IDENTIFIER_INFO_COUNT;
        // SAFETY: idem.
        let identified = unsafe {
            libc::thread_info(
                port,
                libc::THREAD_IDENTIFIER_INFO as u32,
                ident.as_mut_ptr().cast(),
                &mut ident_size,
            )
        } == libc::KERN_SUCCESS;
        if extended {
            // SAFETY: remplies par `thread_info` (ou laissées à zéro).
            let (info, ident) = unsafe { (info.assume_init(), ident.assume_init()) };
            let name = if port == main {
                "principal".to_owned()
            } else {
                let bytes: Vec<u8> = info.pth_name.iter().map(|&c: &c_char| c as u8).collect();
                CStr::from_bytes_until_nul(&bytes)
                    .map(|name| name.to_string_lossy().into_owned())
                    .unwrap_or_default()
            };
            out.push(ThreadStat {
                id: if identified {
                    ident.thread_id
                } else {
                    u64::from(port)
                },
                name,
                cpu: info.pth_user_time.saturating_add(info.pth_system_time),
                state: info.pth_run_state,
                priority: info.pth_curpri,
            });
        }
        // SAFETY: `task_threads` nous a donné un droit sur ce port.
        unsafe { mach_port_deallocate(task, port) };
    }
    // SAFETY: la liste vient de `task_threads`, dans notre espace, de cette taille.
    unsafe {
        libc::vm_deallocate(
            task,
            list as libc::vm_address_t,
            count as usize * size_of::<libc::thread_act_t>(),
        );
    }
    out
}

// --- Système -------------------------------------------------------------

/// Le port de la machine (un droit gardé pour toute la vie du processus).
fn host() -> libc::mach_port_t {
    static HOST: OnceLock<libc::mach_port_t> = OnceLock::new();
    // SAFETY: rend un droit sur le port de la machine ; il est gardé.
    *HOST.get_or_init(|| unsafe { mach_host_self() })
}

fn sysctl<T: Copy>(name: &CStr) -> Option<T> {
    let mut value = MaybeUninit::<T>::zeroed();
    let mut size = size_of::<T>();
    // SAFETY: le tampon a la taille annoncée ; on ne garde la valeur que si
    // le système en a écrit exactement autant.
    let status = unsafe {
        libc::sysctlbyname(
            name.as_ptr(),
            value.as_mut_ptr().cast(),
            &mut size,
            std::ptr::null_mut(),
            0,
        )
    };
    // SAFETY: remplie par `sysctlbyname`, de la taille du type.
    (status == 0 && size == size_of::<T>()).then(|| unsafe { value.assume_init() })
}

fn sysctl_text(name: &CStr) -> Option<String> {
    let mut buffer = [0u8; 256];
    let mut size = buffer.len();
    // SAFETY: le tampon a la taille annoncée.
    let status = unsafe {
        libc::sysctlbyname(
            name.as_ptr(),
            buffer.as_mut_ptr().cast(),
            &mut size,
            std::ptr::null_mut(),
            0,
        )
    };
    if status != 0 {
        return None;
    }
    CStr::from_bytes_until_nul(&buffer[..size.min(buffer.len())])
        .ok()
        .map(|text| text.to_string_lossy().trim().to_owned())
}

pub fn system() -> SystemStats {
    let host = host();
    // SAFETY: lecture d'une constante du système.
    let page = unsafe { libc::sysconf(libc::_SC_PAGESIZE) };
    let mut out = SystemStats {
        page_size: u64::try_from(page).unwrap_or(16384),
        ..SystemStats::default()
    };
    let mut cpu = MaybeUninit::<libc::host_cpu_load_info>::zeroed();
    let mut count = libc::HOST_CPU_LOAD_INFO_COUNT;
    // SAFETY: le tampon a la taille annoncée pour cette sorte d'information.
    if unsafe {
        libc::host_statistics(
            host,
            libc::HOST_CPU_LOAD_INFO,
            cpu.as_mut_ptr().cast(),
            &mut count,
        )
    } == libc::KERN_SUCCESS
    {
        // SAFETY: remplie par `host_statistics`.
        let cpu = unsafe { cpu.assume_init() };
        for (i, ticks) in out.cpu_ticks.iter_mut().enumerate() {
            *ticks = u64::from(cpu.cpu_ticks[i]);
        }
    }
    let mut vm = MaybeUninit::<libc::vm_statistics64>::zeroed();
    let mut count = libc::HOST_VM_INFO64_COUNT;
    // SAFETY: idem.
    if unsafe {
        libc::host_statistics64(
            host,
            libc::HOST_VM_INFO64,
            vm.as_mut_ptr().cast(),
            &mut count,
        )
    } == libc::KERN_SUCCESS
    {
        // SAFETY: remplie par `host_statistics64` (le reste à zéro).
        let vm = unsafe { vm.assume_init() };
        out.free = u64::from(vm.free_count);
        out.active = u64::from(vm.active_count);
        out.inactive = u64::from(vm.inactive_count);
        out.wired = u64::from(vm.wire_count);
        out.compressed = u64::from(vm.compressor_page_count);
        out.pageins = vm.pageins;
        out.pageouts = vm.pageouts;
        out.swapins = vm.swapins;
        out.swapouts = vm.swapouts;
        out.decompressions = vm.decompressions;
    }
    if let Some(swap) = sysctl::<libc::xsw_usage>(c"vm.swapusage") {
        out.swap_used = swap.xsu_used;
    }
    out.pressure = sysctl::<c_int>(c"kern.memorystatus_vm_pressure_level");
    let mut load = [0f64; 3];
    // SAFETY: le tableau a la place des trois moyennes demandées.
    if unsafe { libc::getloadavg(load.as_mut_ptr(), 3) } == 3 {
        out.load = load;
    }
    autoreleasepool(|_| {
        let info = NSProcessInfo::processInfo();
        out.thermal = Some(info.thermalState().0 as i32);
        out.low_power = Some(info.isLowPowerModeEnabled());
    });
    out
}

/// L'en-tête du journal : le Mac, son système, ses écrans, le processus.
pub fn system_info() -> Vec<String> {
    let mut lines = Vec::new();
    autoreleasepool(|_| {
        let info = NSProcessInfo::processInfo();
        let cores =
            |name: &CStr| sysctl::<c_int>(name).map_or_else(|| "?".to_owned(), |n| n.to_string());
        let uptime_h = info.systemUptime() / 3600.0;
        lines.push(format!(
            "SYSTEME macOS {} ; modele {} ; puce {} ; coeurs {} ({} performants, {} economes) ; \
             memoire_go {:.0} ; allume_depuis_h {uptime_h:.1} ; noyau {}",
            info.operatingSystemVersionString(),
            sysctl_text(c"hw.model").unwrap_or_default(),
            sysctl_text(c"machdep.cpu.brand_string").unwrap_or_default(),
            cores(c"hw.physicalcpu"),
            cores(c"hw.perflevel0.physicalcpu"),
            cores(c"hw.perflevel1.physicalcpu"),
            info.physicalMemory() as f64 / (1u64 << 30) as f64,
            sysctl_text(c"kern.osversion").unwrap_or_default(),
        ));
        let mut qos = libc::qos_class_t::QOS_CLASS_UNSPECIFIED;
        let mut relative = 0;
        // SAFETY: lit la classe de service du fil courant.
        unsafe { pthread_get_qos_class_np(libc::pthread_self(), &mut qos, &mut relative) };
        let workspace = NSWorkspace::sharedWorkspace();
        let (numer, denom) = timebase();
        let mut process = format!(
            "PROCESSUS pid {} ; classe_principal {} ; thermique {} ; economie {} ; \
             reduire_animations {} ; reduire_transparence {} ; contraste {} ; base_temps {numer}/{denom}",
            std::process::id(),
            qos as u32,
            info.thermalState().0,
            u8::from(info.isLowPowerModeEnabled()),
            u8::from(workspace.accessibilityDisplayShouldReduceMotion()),
            u8::from(workspace.accessibilityDisplayShouldReduceTransparency()),
            u8::from(workspace.accessibilityDisplayShouldIncreaseContrast()),
        );
        if let Some(mtm) = MainThreadMarker::new() {
            let app = NSApplication::sharedApplication(mtm);
            process.push_str(&format!(
                " ; apparence {} ; app_visible {}",
                app.effectiveAppearance().name(),
                u8::from(
                    app.occlusionState()
                        .contains(NSApplicationOcclusionState::Visible)
                ),
            ));
            for (i, screen) in NSScreen::screens(mtm).iter().enumerate() {
                let frame = screen.frame();
                lines.push(format!(
                    "ECRAN_DETAIL {i} « {} » {:.0}x{:.0} echelle {} hz_max {} intervalle_min_ms {:.2} \
                     intervalle_max_ms {:.2} granularite_ms {:.2}",
                    screen.localizedName(),
                    frame.size.width,
                    frame.size.height,
                    screen.backingScaleFactor(),
                    screen.maximumFramesPerSecond(),
                    screen.minimumRefreshInterval() * 1000.0,
                    screen.maximumRefreshInterval() * 1000.0,
                    screen.displayUpdateGranularity() * 1000.0,
                ));
            }
        }
        lines.push(process);
    });
    lines
}

// --- Notifications du système -------------------------------------------

type Notice = Box<dyn Fn(String)>;

thread_local! {
    static NOTICE: RefCell<Option<Notice>> = const { RefCell::new(None) };
    static OBSERVER: OnceCell<Retained<DiagObserver>> = const { OnceCell::new() };
}

define_class!(
    // SAFETY: NSObject n'impose rien à ses sous-classes ; `DiagObserver`
    // n'implémente pas `Drop`.
    #[unsafe(super(NSObject))]
    #[thread_kind = MainThreadOnly]
    #[name = "MouflonDiagObserver"]
    struct DiagObserver;

    // SAFETY: NSObjectProtocol n'a pas d'exigence de plus.
    unsafe impl NSObjectProtocol for DiagObserver {}

    impl DiagObserver {
        // SAFETY: appelée par les centres de notifications avec une
        // NSNotification, sur le fil principal.
        #[unsafe(method(notice:))]
        fn notice(&self, notification: &NSNotification) {
            let text = describe(notification);
            NOTICE.with(|notice| {
                if let Some(notice) = notice.borrow().as_ref() {
                    notice(text);
                }
            });
        }
    }
);

impl DiagObserver {
    fn new(mtm: MainThreadMarker) -> Retained<Self> {
        let this = Self::alloc(mtm).set_ivars(());
        // SAFETY: `init` est l'initialiseur de NSObject.
        unsafe { msg_send![super(this), init] }
    }
}

/// Le nom court d'une notification, et ce qu'elle dit d'utile.
fn describe(notification: &NSNotification) -> String {
    let name = notification.name().to_string();
    let short = name
        .trim_start_matches("NS")
        .trim_end_matches("Notification")
        .to_owned();
    let object = notification.object();
    let app_of = || -> Option<String> {
        let info = notification.userInfo()?;
        // SAFETY: constante d'AppKit.
        let key: &NSString = unsafe { NSWorkspaceApplicationKey };
        let app = info.objectForKey(key.as_ref())?;
        let app = app.downcast::<NSRunningApplication>().ok()?;
        Some(format!(
            "{} (pid {})",
            app.localizedName()
                .map(|n| n.to_string())
                .unwrap_or_default(),
            app.processIdentifier()
        ))
    };
    if name.contains("Application") && name.starts_with("NSWorkspace") {
        return format!("{short} {}", app_of().unwrap_or_default());
    }
    if name == "NSWindowDidChangeOcclusionStateNotification"
        || name == "NSWindowDidChangeScreenNotification"
    {
        if let Some(window) = object.and_then(|o| o.downcast::<NSWindow>().ok()) {
            return format!(
                "{short} fenêtre {} visible {} bureau_actif {}",
                window.windowNumber(),
                u8::from(
                    window
                        .occlusionState()
                        .contains(NSWindowOcclusionState::Visible)
                ),
                u8::from(window.isOnActiveSpace()),
            );
        }
        return short;
    }
    if name == "NSApplicationDidChangeOcclusionStateNotification"
        && let Some(mtm) = MainThreadMarker::new()
    {
        let visible = NSApplication::sharedApplication(mtm)
            .occlusionState()
            .contains(NSApplicationOcclusionState::Visible);
        return format!("{short} visible {}", u8::from(visible));
    }
    if name.contains("ThermalState") || name.contains("PowerState") {
        let info = NSProcessInfo::processInfo();
        return format!(
            "{short} thermique {} economie {}",
            info.thermalState().0,
            u8::from(info.isLowPowerModeEnabled())
        );
    }
    short
}

/// Écoute les notifications du système ; chacune passe à `sink`, sur le
/// fil principal.
pub fn observe_system(sink: Box<dyn Fn(String)>) {
    let Some(mtm) = MainThreadMarker::new() else {
        return;
    };
    NOTICE.with(|notice| *notice.borrow_mut() = Some(sink));
    let observer = OBSERVER.with(|cell| cell.get_or_init(|| DiagObserver::new(mtm)).clone());
    let target: &AnyObject = observer.as_ref();
    // SAFETY: constantes d'AppKit et de Foundation, valides pendant toute la
    // vie du processus.
    let (workspace_names, app_names): (Vec<&NSNotificationName>, Vec<&NSNotificationName>) = unsafe {
        (
            vec![
                NSWorkspaceWillSleepNotification,
                NSWorkspaceDidWakeNotification,
                NSWorkspaceScreensDidSleepNotification,
                NSWorkspaceScreensDidWakeNotification,
                NSWorkspaceSessionDidBecomeActiveNotification,
                NSWorkspaceSessionDidResignActiveNotification,
                NSWorkspaceActiveSpaceDidChangeNotification,
                NSWorkspaceDidActivateApplicationNotification,
                NSWorkspaceDidLaunchApplicationNotification,
                NSWorkspaceDidTerminateApplicationNotification,
                NSWorkspaceAccessibilityDisplayOptionsDidChangeNotification,
            ],
            vec![
                NSApplicationDidChangeScreenParametersNotification,
                NSApplicationDidChangeOcclusionStateNotification,
                NSApplicationDidBecomeActiveNotification,
                NSApplicationDidResignActiveNotification,
                NSWindowDidChangeOcclusionStateNotification,
                NSWindowDidChangeScreenNotification,
                NSProcessInfoThermalStateDidChangeNotification,
                NSProcessInfoPowerStateDidChangeNotification,
                NSSystemClockDidChangeNotification,
            ],
        )
    };
    let workspace = NSWorkspace::sharedWorkspace().notificationCenter();
    for name in workspace_names {
        // SAFETY: l'observateur répond à `notice:` (une NSNotification) et
        // vit jusqu'à la fin du processus.
        unsafe {
            workspace.addObserver_selector_name_object(target, sel!(notice:), Some(name), None)
        };
    }
    let center = NSNotificationCenter::defaultCenter();
    for name in app_names {
        // SAFETY: idem.
        unsafe { center.addObserver_selector_name_object(target, sel!(notice:), Some(name), None) };
    }
    // Verrouillage de l'écran et économiseur : des notifications « distribuées ».
    let distributed = NSDistributedNotificationCenter::defaultCenter();
    for name in [
        "com.apple.screenIsLocked",
        "com.apple.screenIsUnlocked",
        "com.apple.screensaver.didstart",
        "com.apple.screensaver.didstop",
    ] {
        let name = NSString::from_str(name);
        // SAFETY: idem.
        unsafe {
            distributed.addObserver_selector_name_object(target, sel!(notice:), Some(&name), None);
        }
    }
}
