//! Compte des allocations, pour les essais : combien, combien d'octets, la
//! mémoire encore vivante et son maximum, en tout et sur le fil principal
//! (celui de l'animation, où une allocation de trop coûte une image).
//!
//! L'app déclare [`CountingAlloc`] comme allocateur ; le compte ne commence
//! qu'avec [`set_alloc_counting`]. Hors essai, il ne coûte qu'une lecture
//! atomique par allocation.

use std::alloc::{GlobalAlloc, Layout, System};
use std::sync::atomic::{AtomicBool, AtomicI64, AtomicU64, AtomicUsize, Ordering::Relaxed};

static ENABLED: AtomicBool = AtomicBool::new(false);
/// Fil principal (`pthread_self`, ou son numéro sous Windows), 0 tant
/// qu'il n'est pas connu.
static MAIN: AtomicUsize = AtomicUsize::new(0);
static ALLOCS: AtomicU64 = AtomicU64::new(0);
static REALLOCS: AtomicU64 = AtomicU64::new(0);
static FREES: AtomicU64 = AtomicU64::new(0);
static BYTES: AtomicU64 = AtomicU64::new(0);
/// Octets vivants depuis le début du compte (les blocs alloués avant puis
/// libérés le font descendre : c'est un écart, pas un total).
static LIVE: AtomicI64 = AtomicI64::new(0);
static PEAK: AtomicI64 = AtomicI64::new(0);
static MAIN_ALLOCS: AtomicU64 = AtomicU64::new(0);
static MAIN_BYTES: AtomicU64 = AtomicU64::new(0);

/// L'allocateur du système, qui compte (voir le module).
#[derive(Clone, Copy, Debug, Default)]
pub struct CountingAlloc;

/// Les comptes depuis [`set_alloc_counting`].
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct AllocCounts {
    pub allocs: u64,
    pub reallocs: u64,
    pub frees: u64,
    /// Octets demandés (allocations et agrandissements).
    pub bytes: u64,
    /// Octets vivants : alloués moins libérés depuis le début du compte.
    pub live: i64,
    /// Plus haut niveau de `live`.
    pub peak: i64,
    /// Allocations (et agrandissements) faites par le fil principal.
    pub main_allocs: u64,
    pub main_bytes: u64,
}

/// Lance (ou arrête) le compte. Le fil qui l'appelle est pris pour le fil
/// principal.
pub fn set_alloc_counting(on: bool) {
    MAIN.store(thread_key(), Relaxed);
    ENABLED.store(on, Relaxed);
}

/// Les comptes jusqu'ici.
#[must_use]
pub fn alloc_counts() -> AllocCounts {
    AllocCounts {
        allocs: ALLOCS.load(Relaxed),
        reallocs: REALLOCS.load(Relaxed),
        frees: FREES.load(Relaxed),
        bytes: BYTES.load(Relaxed),
        live: LIVE.load(Relaxed),
        peak: PEAK.load(Relaxed),
        main_allocs: MAIN_ALLOCS.load(Relaxed),
        main_bytes: MAIN_BYTES.load(Relaxed),
    }
}

/// Un nombre propre au fil courant, lu sans rien allouer.
#[cfg(unix)]
fn thread_key() -> usize {
    // SAFETY: `pthread_self` ne fait que lire le registre du fil courant.
    unsafe { libc::pthread_self() as usize }
}

/// Un nombre propre au fil courant, lu sans rien allouer.
#[cfg(windows)]
fn thread_key() -> usize {
    // SAFETY: `GetCurrentThreadId` ne fait que lire le bloc du fil courant.
    unsafe { windows_sys::Win32::System::Threading::GetCurrentThreadId() as usize }
}

#[cfg(not(any(unix, windows)))]
fn thread_key() -> usize {
    0
}

fn grew(size: usize, delta: i64) {
    let size = size as u64;
    BYTES.fetch_add(size, Relaxed);
    let live = LIVE.fetch_add(delta, Relaxed) + delta;
    PEAK.fetch_max(live, Relaxed);
    let main = MAIN.load(Relaxed);
    if main != 0 && thread_key() == main {
        MAIN_ALLOCS.fetch_add(1, Relaxed);
        MAIN_BYTES.fetch_add(size, Relaxed);
    }
}

// SAFETY: chaque méthode passe la demande telle quelle à l'allocateur du
// système, qui en respecte le contrat ; le compte n'y touche pas (il
// n'alloue rien et ne lit que des atomiques).
unsafe impl GlobalAlloc for CountingAlloc {
    unsafe fn alloc(&self, layout: Layout) -> *mut u8 {
        // SAFETY: même contrat que l'appelant.
        let ptr = unsafe { System.alloc(layout) };
        if !ptr.is_null() && ENABLED.load(Relaxed) {
            ALLOCS.fetch_add(1, Relaxed);
            grew(layout.size(), layout.size() as i64);
        }
        ptr
    }

    unsafe fn alloc_zeroed(&self, layout: Layout) -> *mut u8 {
        // SAFETY: même contrat que l'appelant.
        let ptr = unsafe { System.alloc_zeroed(layout) };
        if !ptr.is_null() && ENABLED.load(Relaxed) {
            ALLOCS.fetch_add(1, Relaxed);
            grew(layout.size(), layout.size() as i64);
        }
        ptr
    }

    unsafe fn dealloc(&self, ptr: *mut u8, layout: Layout) {
        // SAFETY: même contrat que l'appelant.
        unsafe { System.dealloc(ptr, layout) };
        if ENABLED.load(Relaxed) {
            FREES.fetch_add(1, Relaxed);
            LIVE.fetch_sub(layout.size() as i64, Relaxed);
        }
    }

    unsafe fn realloc(&self, ptr: *mut u8, layout: Layout, new_size: usize) -> *mut u8 {
        // SAFETY: même contrat que l'appelant.
        let new = unsafe { System.realloc(ptr, layout, new_size) };
        if !new.is_null() && ENABLED.load(Relaxed) {
            REALLOCS.fetch_add(1, Relaxed);
            grew(new_size, new_size as i64 - layout.size() as i64);
        }
        new
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn counts_allocations_made_through_it() {
        set_alloc_counting(true);
        let before = alloc_counts();
        let layout = Layout::from_size_align(1000, 8).unwrap();
        // SAFETY: taille non nulle ; le bloc est rendu tout de suite avec
        // la même disposition.
        unsafe {
            let ptr = CountingAlloc.alloc(layout);
            assert!(!ptr.is_null());
            let ptr = CountingAlloc.realloc(ptr, layout, 3000);
            assert!(!ptr.is_null());
            CountingAlloc.dealloc(ptr, Layout::from_size_align(3000, 8).unwrap());
        }
        let after = alloc_counts();
        assert!(after.allocs > before.allocs);
        assert!(after.reallocs > before.reallocs);
        assert!(after.frees > before.frees);
        assert!(after.bytes >= before.bytes + 4000);
        // Ce fil est celui du compte : ses allocations y sont.
        assert!(after.main_allocs >= before.main_allocs + 2);
        assert!(after.peak >= 3000);
    }
}
