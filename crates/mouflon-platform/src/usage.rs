//! Ressources du processus, lues sans lancer de programme : mémoire, temps
//! de calcul et réveils (pour les essais). Lancer `ps` toutes les dix
//! secondes coûtait quelques millisecondes au fil principal, de quoi faire
//! sauter l'image qui suivait chaque relevé.

use std::time::Duration;

/// Ce que le processus a consommé jusqu'ici.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct ProcessUsage {
    /// Mémoire résidente, en octets.
    pub resident: u64,
    /// Empreinte mémoire comptée par le système (celle du Moniteur
    /// d'activité), si on la connaît. La mémoire libérée que le système n'a
    /// pas encore reprise compte dans la mémoire résidente, pas ici.
    pub footprint: Option<u64>,
    /// Temps de calcul cumulé de tous les fils (utilisateur et système).
    pub cpu: Duration,
    /// Réveils du processus par une interruption (minuterie…) depuis son
    /// lancement, si on les connaît.
    pub wakeups: Option<u64>,
}

/// Temps de calcul cumulé de tous les fils du processus.
#[cfg(unix)]
fn cpu_time() -> Option<Duration> {
    let mut usage = std::mem::MaybeUninit::<libc::rusage>::zeroed();
    // SAFETY: `getrusage` remplit la structure dont on lui passe l'adresse,
    // valide et assez grande ; `RUSAGE_SELF` désigne le processus courant.
    if unsafe { libc::getrusage(libc::RUSAGE_SELF, usage.as_mut_ptr()) } != 0 {
        return None;
    }
    // SAFETY: mise à zéro à la création, puis remplie par `getrusage`.
    let usage = unsafe { usage.assume_init() };
    let time = |t: libc::timeval| {
        Duration::from_secs(u64::try_from(t.tv_sec).unwrap_or(0))
            + Duration::from_micros(u64::try_from(t.tv_usec).unwrap_or(0))
    };
    Some(time(usage.ru_utime) + time(usage.ru_stime))
}

/// Mémoire, temps de calcul et réveils, d'après le noyau.
#[cfg(target_os = "macos")]
#[must_use]
pub fn process_usage() -> Option<ProcessUsage> {
    let mut info = std::mem::MaybeUninit::<libc::rusage_info_v2>::zeroed();
    // SAFETY: pour la version `RUSAGE_INFO_V2`, `proc_pid_rusage` remplit une
    // `rusage_info_v2` à l'adresse donnée (l'API la type `rusage_info_t *`) ;
    // le processus visé est le nôtre.
    let status = unsafe {
        libc::proc_pid_rusage(
            libc::getpid(),
            libc::RUSAGE_INFO_V2,
            info.as_mut_ptr().cast::<libc::rusage_info_t>(),
        )
    };
    if status != 0 {
        return None;
    }
    // SAFETY: mise à zéro à la création, puis remplie par `proc_pid_rusage`.
    let info = unsafe { info.assume_init() };
    Some(ProcessUsage {
        resident: info.ri_resident_size,
        footprint: Some(info.ri_phys_footprint),
        cpu: cpu_time()?,
        wakeups: Some(info.ri_interrupt_wkups),
    })
}

/// Mémoire résidente (d'après `/proc`) et temps de calcul.
#[cfg(all(unix, not(target_os = "macos")))]
#[must_use]
pub fn process_usage() -> Option<ProcessUsage> {
    // Tailles en pages : programme entier, puis partie résidente.
    let statm = std::fs::read_to_string("/proc/self/statm").ok()?;
    let pages: u64 = statm.split_whitespace().nth(1)?.parse().ok()?;
    // SAFETY: `sysconf` ne fait que lire une constante du système.
    let page = unsafe { libc::sysconf(libc::_SC_PAGESIZE) };
    Some(ProcessUsage {
        resident: pages * u64::try_from(page).unwrap_or(4096),
        footprint: None,
        cpu: cpu_time()?,
        wakeups: None,
    })
}

/// Mémoire (celle que le Gestionnaire des tâches montre, et celle qui
/// n'appartient qu'à ce processus) et temps de calcul, d'après Windows.
#[cfg(target_os = "windows")]
#[must_use]
pub fn process_usage() -> Option<ProcessUsage> {
    use windows_sys::Win32::Foundation::FILETIME;
    use windows_sys::Win32::System::ProcessStatus::{
        K32GetProcessMemoryInfo, PROCESS_MEMORY_COUNTERS, PROCESS_MEMORY_COUNTERS_EX,
    };
    use windows_sys::Win32::System::Threading::{GetCurrentProcess, GetProcessTimes};

    let mut memory = PROCESS_MEMORY_COUNTERS_EX {
        cb: size_of::<PROCESS_MEMORY_COUNTERS_EX>() as u32,
        ..PROCESS_MEMORY_COUNTERS_EX::default()
    };
    let (mut created, mut exited) = (FILETIME::default(), FILETIME::default());
    let (mut kernel, mut user) = (FILETIME::default(), FILETIME::default());
    // SAFETY: la pseudo-poignée du processus courant est toujours valide ;
    // chaque structure est à nous, et celle de la mémoire est donnée avec sa
    // taille (la version étendue commence comme la version courte).
    let known = unsafe {
        let process = GetCurrentProcess();
        K32GetProcessMemoryInfo(
            process,
            (&raw mut memory).cast::<PROCESS_MEMORY_COUNTERS>(),
            memory.cb,
        ) != 0
            && GetProcessTimes(process, &mut created, &mut exited, &mut kernel, &mut user) != 0
    };
    if !known {
        return None;
    }
    // Des centaines de nanosecondes.
    let ticks = |t: FILETIME| (u64::from(t.dwHighDateTime) << 32) | u64::from(t.dwLowDateTime);
    Some(ProcessUsage {
        resident: memory.WorkingSetSize as u64,
        footprint: Some(memory.PrivateUsage as u64),
        cpu: Duration::from_nanos((ticks(kernel) + ticks(user)).saturating_mul(100)),
        wakeups: None,
    })
}

/// Rien à lire sur ce système.
#[cfg(not(any(unix, target_os = "windows")))]
#[must_use]
pub fn process_usage() -> Option<ProcessUsage> {
    None
}

#[cfg(all(test, any(unix, target_os = "windows")))]
mod tests {
    use super::*;

    #[test]
    fn reads_memory_and_counts_cpu_time() {
        let before = process_usage().expect("ressources lisibles");
        assert!(before.resident > 0);
        // Un peu de calcul : le temps compté ne recule jamais.
        let mut x = 0u64;
        for i in 0..2_000_000u64 {
            x = x.wrapping_mul(6_364_136_223_846_793_005).wrapping_add(i);
        }
        std::hint::black_box(x);
        let after = process_usage().expect("ressources lisibles");
        assert!(after.cpu >= before.cpu);
    }
}
