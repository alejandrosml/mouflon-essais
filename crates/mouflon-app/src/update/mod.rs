//! La mise à jour de Mouflon par lui-même.
//!
//! Une version sort dans le dépôt public des téléchargements avec la liste de
//! ses fichiers, signée par l'auteur (`release.rs`, `signature.rs`). Mouflon
//! lit cette liste au lancement, puis une fois par jour (`fetch.rs`), et
//! n'installe que ce qu'elle annonce : le fichier de son système, vérifié,
//! préparé à côté de la version qui tourne (`home.rs`), puis mis à sa place
//! quand elle s'efface (`relay.rs`). Si la nouvelle version ne démarre pas,
//! l'ancienne reprend sa place.
//!
//! Rien n'est dit de l'utilisateur ni de sa machine : Mouflon demande deux
//! petits fichiers à GitHub, comme un navigateur, et rien d'autre tant qu'il
//! n'y a pas de version plus récente.

pub mod fetch;
pub mod home;
pub mod relay;
pub mod release;
pub mod signature;
pub mod state;
pub mod tool;

use std::sync::mpsc::{Receiver, RecvTimeoutError, Sender};
use std::time::{Duration, Instant, SystemTime};

pub use home::Ready;
pub use relay::Arrival;
pub use release::Version;
pub use state::{Animal, Dirs, Herd, Staged, Worn};

use fetch::{FetchError, Source};
use home::{Blame, Home, InstallError, Lock, Standing, Tidy};
use state::State;

/// La clé publique de l'auteur : la ligne de son fichier `.pub`. Les versions
/// sont signées, sur son Mac, avec la clé privée qui va avec.
#[cfg(not(feature = "essai-maj"))]
pub const PUBLIC_KEY: &str = include_str!("../../assets/cle-de-signature.pub");
/// Un programme d'essai porte une clé d'essai, donnée à sa compilation.
#[cfg(feature = "essai-maj")]
pub const PUBLIC_KEY: &str = env!("MOUFLON_ESSAI_CLE");

/// Entre deux recherches, tant que Mouflon tourne.
const DAY: Duration = Duration::from_secs(24 * 60 * 60);
/// Une recherche qui n'a pas abouti (pas de réseau au lancement, souvent)
/// est refaite plus tôt, quelques fois.
const SOON: Duration = Duration::from_secs(10 * 60);
const SOON_TIMES: u32 = 3;
/// L'heure est relue de temps en temps : un ordinateur qui a dormi n'a pas
/// vu passer la journée.
const GLANCE: Duration = Duration::from_secs(15 * 60);
/// Ce qu'une version qui arrive laisse à celle d'avant pour finir de veiller
/// sur elle, avant de faire le ménage.
const HANDOVER: Duration = Duration::from_secs(120);
/// Quand le ménage doit attendre (l'ancienne version reste un moment à côté
/// de la nouvelle), il est refait après ce temps-là.
const TIDY_LATER: Duration = Duration::from_secs(150);

/// Où ce Mouflon cherche ses versions : le dépôt public des téléchargements.
#[cfg(not(feature = "essai-maj"))]
fn source() -> Option<Source> {
    Some(Source::official())
}

/// Un programme d'essai ne cherche que dans le dossier qu'on lui donne.
#[cfg(feature = "essai-maj")]
fn source() -> Option<Source> {
    let dir = std::env::var_os("MOUFLON_ESSAI_SOURCE")?;
    Some(Source::folder(std::path::Path::new(&dir)))
}

/// Ce Mouflon cherche-t-il des versions ? Pas pendant un essai ni une
/// vitrine, ni sur un système pour lequel aucune version ne sort.
pub fn offered() -> bool {
    release::own_file().is_some()
        && (cfg!(feature = "essai-maj")
            || (crate::stats::trial().is_none() && crate::stats::showcase().is_none()))
}

/// Le réglage de la mise à jour se montre-t-il ? Là où une version sort pour
/// ce système, sauf dans une app de l'App Store (qui s'en charge).
pub fn in_settings() -> bool {
    release::own_file().is_some() && !home::from_a_store()
}

/// Ce que la recherche rapporte.
#[derive(Debug)]
pub enum Event {
    /// Une version est prête à prendre la place de celle qui tourne.
    Ready(Box<Ready>),
    /// Une version existe, que ce Mouflon n'installe pas lui-même (il n'a
    /// pas le droit d'écrire là où il est, ou elle n'a pas pu démarrer ici).
    Available(Version),
}

/// Pourquoi une recherche n'a rien rapporté.
#[derive(Debug, thiserror::Error)]
enum RoundError {
    #[error("{0}")]
    Fetch(#[from] FetchError),
    #[error("{0}")]
    Install(#[from] InstallError),
}

impl RoundError {
    /// Vrai si rien n'est retenu contre la version : c'est le moment qui ne
    /// va pas (le réseau, le disque, un outil).
    fn of_the_moment(&self) -> bool {
        match self {
            RoundError::Fetch(_) => true,
            RoundError::Install(e) => e.blame() == Blame::Moment,
        }
    }
}

/// Le fil qui cherche les versions, à côté de tout le reste.
#[derive(Debug)]
pub struct Updater {
    enabled: Sender<bool>,
}

impl Updater {
    /// Lance la recherche : tout de suite si elle est permise (`enabled`,
    /// le réglage), puis une fois par jour. `arrival` : ce Mouflon vient
    /// d'être lancé par un relais. `notify` reçoit ce qui est trouvé, sur le
    /// fil de la recherche.
    pub fn start(
        dirs: Dirs,
        enabled: bool,
        arrival: Option<Arrival>,
        notify: impl Fn(Event) + Send + 'static,
    ) -> Option<Updater> {
        let (tx, rx) = std::sync::mpsc::channel();
        std::thread::Builder::new()
            .name("mouflon-maj".into())
            .spawn(move || rounds(&dirs, enabled, arrival, &rx, &notify))
            .ok()?;
        Some(Updater { enabled: tx })
    }

    /// Le réglage a changé. Remis, la recherche reprend tout de suite.
    pub fn enable(&self, on: bool) {
        let _ = self.enabled.send(on);
    }
}

fn rounds(
    dirs: &Dirs,
    mut enabled: bool,
    arrival: Option<Arrival>,
    controls: &Receiver<bool>,
    notify: &dyn Fn(Event),
) {
    let mut standing = Home::find();
    // Le ménage d'une mise à jour passée : tout de suite si possible, sinon
    // un peu plus tard (l'ancienne version reste un moment à côté).
    let mut tidy_at = None;
    match &standing {
        Standing::Never => {
            log::info!("mise à jour : ce Mouflon-ci ne se met pas à jour lui-même");
            return;
        }
        Standing::Tell => log::info!("mise à jour : ce Mouflon ne peut pas se remplacer lui-même"),
        Standing::Own(home) => {
            if tidy(home, dirs, arrival.is_some()) == Some(Tidy::Later) {
                tidy_at = Some(Instant::now() + TIDY_LATER);
            }
        }
    }
    // Un Mouflon revenu parce que la version suivante n'a pas pris la place
    // n'installe plus rien de toute sa vie : il dit seulement qu'elle existe.
    let installs = arrival != Some(Arrival::Back);
    let mut failures = 0;
    loop {
        let mut pause = DAY;
        if enabled {
            // Où est Mouflon, cette fois-ci : on a pu le déplacer pendant
            // qu'il tourne.
            standing = Home::find();
            match round(dirs, &standing, installs) {
                Ok(found) => {
                    failures = 0;
                    if let Some(event) = found {
                        notify(event);
                    }
                }
                Err((version, e)) => {
                    log::info!("mise à jour : {e}");
                    // Un souci du moment se réessaie bientôt ; ce qui a été
                    // retenu contre la version attend le lendemain (ou le
                    // prochain lancement) : pas deux essais coup sur coup.
                    let mut soon = false;
                    if e.of_the_moment() {
                        failures += 1;
                        soon = failures <= SOON_TIMES;
                    }
                    if soon {
                        pause = SOON;
                    } else if let Some(version) = version {
                        // (Tant que Mouflon réessaie bientôt, il ne dit
                        // rien : la ligne des menus est pour ce qu'il ne
                        // fait pas lui-même.)
                        notify(Event::Available(version));
                    }
                }
            }
        }
        // Jusqu'à la prochaine recherche : à l'heure du mur, pas à celle du
        // processeur (qui s'arrête quand l'ordinateur dort).
        let since = SystemTime::now();
        loop {
            let mut wait = GLANCE.min(pause);
            if let Some(at) = tidy_at {
                wait = wait.min(at.saturating_duration_since(Instant::now()));
            }
            match controls.recv_timeout(wait) {
                // Remise : on cherche tout de suite.
                Ok(on) if on && !enabled => {
                    enabled = true;
                    break;
                }
                Ok(on) => enabled = on,
                Err(RecvTimeoutError::Timeout) => {
                    if tidy_at.is_some_and(|at| Instant::now() >= at)
                        && let Standing::Own(home) = &standing
                    {
                        tidy_at = match tidy(home, dirs, false) {
                            Some(Tidy::Clean | Tidy::Stuck) => None,
                            _ => Some(Instant::now() + TIDY_LATER),
                        };
                    }
                    if due(since, SystemTime::now(), pause) {
                        break;
                    }
                }
                Err(RecvTimeoutError::Disconnected) => return,
            }
        }
    }
}

/// Est-il l'heure de chercher de nouveau, `pause` après `since` ? (Une
/// horloge remise en arrière ne fait pas attendre plus longtemps.)
fn due(since: SystemTime, now: SystemTime, pause: Duration) -> bool {
    now.duration_since(since)
        .map_or(true, |spent| spent >= pause)
}

/// Le ménage après une mise à jour (voir `Home::tidy`), si le verrou est
/// libre. `wait` : ce Mouflon vient d'être lancé par un relais, et celui qui
/// l'a lancé tient encore le verrou quelques instants. `None` : le verrou
/// est resté pris.
fn tidy(home: &Home, dirs: &Dirs, wait: bool) -> Option<Tidy> {
    let started = Instant::now();
    loop {
        if let Some(_lock) = Lock::take(dirs) {
            return Some(home.tidy(Version::current()));
        }
        if !wait || started.elapsed() >= HANDOVER {
            return None;
        }
        std::thread::sleep(Duration::from_secs(1));
    }
}

/// Une recherche : y a-t-il une version plus récente ? Si oui (et si ce
/// Mouflon installe : `installs`), elle est téléchargée, vérifiée et
/// préparée. Une erreur vient avec la version qu'elle concerne, si on la
/// connaît déjà.
fn round(
    dirs: &Dirs,
    standing: &Standing,
    installs: bool,
) -> Result<Option<Event>, (Option<Version>, RoundError)> {
    let (Some(source), Some(wanted)) = (source(), release::own_file()) else {
        return Ok(None);
    };
    let current = Version::current();
    let found = fetch::check(&source, PUBLIC_KEY, current, wanted)
        .map_err(|e| (None, RoundError::from(e)))?;
    let Some(found) = found else {
        log::info!("mise à jour : Mouflon {current} est la dernière version");
        return Ok(None);
    };
    let version = found.version;
    log::info!("mise à jour : Mouflon {version} existe");
    let available = Ok(Some(Event::Available(version)));
    let home = match standing {
        Standing::Own(home) if installs => home,
        // (Introuvable pour le moment : rien à dire.)
        Standing::Never => return Ok(None),
        _ => return available,
    };
    let state = State::load(dirs);
    if state.refused.contains(&version) {
        return available;
    }
    // Remise à plus tard : elle s'installera au prochain lancement.
    if state.postponed == Some(version) {
        return Ok(None);
    }
    let Some(lock) = Lock::take(dirs) else {
        log::info!("mise à jour : une installation est déjà en cours");
        return Ok(None);
    };
    // Rien ne s'installe par-dessus une installation restée en plan.
    if home.tidy(current) == Tidy::Stuck {
        return available;
    }
    let failed = |e: RoundError| (Some(version), e);
    let package = fetch::download(&source, &found, &dirs.cache).map_err(|e| failed(e.into()))?;
    match home.prepare(&package, &found.file, version, lock) {
        Ok(ready) => Ok(Some(Event::Ready(Box::new(ready)))),
        Err(e) => {
            blame(dirs, version, &e);
            Err(failed(e.into()))
        }
    }
}

/// Retient contre une version ce que son installation manquée dit d'elle.
fn blame(dirs: &Dirs, version: Version, error: &InstallError) {
    let refused = match error.blame() {
        Blame::Moment => false,
        Blame::Attempt => State::failed(dirs, version),
        Blame::Version => {
            State::refuse(dirs, version);
            true
        }
    };
    if refused {
        log::warn!("mise à jour : Mouflon {version} ne sera pas réessayé ici");
    }
}

/// Remet cette version, prête, au prochain lancement (on l'a demandé pendant
/// son annonce, ou Mouflon a été quitté à ce moment-là) : son fichier reste
/// où il a été téléchargé, et [`at_launch`] l'installera.
pub fn postpone(dirs: &Dirs, version: Version) {
    State::change(dirs, |state| state.postponed = Some(version));
}

/// Au lancement, avant que rien ne se montre : la version qu'on avait remise
/// à plus tard s'installe maintenant (elle est déjà téléchargée ; rien n'est
/// demandé au réseau). Vrai si elle a pris le relais : ce Mouflon-ci n'a
/// plus qu'à s'arrêter.
pub fn at_launch(dirs: &Dirs) -> bool {
    let state = State::load(dirs);
    let Some(version) = state.postponed else {
        return false;
    };
    let forget = || State::change(dirs, |state| state.postponed = None);
    let (Some(wanted), Standing::Own(home)) = (release::own_file(), Home::find()) else {
        forget();
        return false;
    };
    let Some(lock) = Lock::take(dirs) else {
        return false;
    };
    let current = Version::current();
    let kept = fetch::kept(&dirs.cache, PUBLIC_KEY, current, wanted)
        .filter(|(found, _)| found.version == version && !state.refused.contains(&version));
    let Some((found, package)) = kept else {
        // Plus de fichier, ou plus le bon : la recherche le reprendra.
        forget();
        return false;
    };
    if home.tidy(current) == Tidy::Stuck {
        forget();
        return false;
    }
    log::info!("mise à jour : Mouflon {version}, remise à ce lancement, s'installe");
    match home.prepare(&package, &found.file, version, lock) {
        Ok(ready) => match relay::go(ready, dirs) {
            relay::Outcome::Relayed => true,
            relay::Outcome::Stayed(stayed) => {
                log::warn!(
                    "mise à jour : Mouflon {version} n'a pas pris la place ({})",
                    stayed.why
                );
                false
            }
        },
        Err(e) => {
            log::warn!("mise à jour : Mouflon {version} ne s'installe pas ({e})");
            blame(dirs, version, &e);
            forget();
            false
        }
    }
}

#[cfg(all(test, not(feature = "essai-maj")))]
mod tests {
    use super::*;

    #[test]
    fn the_next_search_goes_by_the_wall_clock() {
        let noon = SystemTime::UNIX_EPOCH + Duration::from_secs(1_800_000_000);
        let later = |seconds| noon + Duration::from_secs(seconds);
        assert!(!due(noon, later(23 * 3600), DAY));
        assert!(due(noon, later(24 * 3600), DAY));
        assert!(due(noon, later(11 * 60), SOON) && !due(noon, later(9 * 60), SOON));
        // L'horloge a été remise en arrière : on cherche, plutôt que
        // d'attendre qu'elle rattrape.
        assert!(due(later(3600), noon, DAY));
    }

    #[test]
    fn the_key_mouflon_carries_is_the_authors() {
        // Une clé Ed25519, et celle-là : en changer couperait la mise à jour
        // de tous les Mouflon installés, qui ne connaissent que celle-ci.
        assert_eq!(
            PUBLIC_KEY.trim(),
            "ssh-ed25519 AAAAC3NzaC1lZDI1NTE5AAAAIB+sUxVG7KToUkTmF1wmx98Lo3i9Bto1Nu3948PPZUC7"
        );
        // Elle se lit : une signature faite avec une autre est dite « d'une
        // autre clé », pas « illisible ».
        let other = "-----BEGIN SSH SIGNATURE-----
U1NIU0lHAAAAAQAAADMAAAALc3NoLWVkMjU1MTkAAAAg7wiBR9x08b/e3Tu/Y+ivt3xyDS
WQkYjJJl+1vL9wblwAAAAHbW91ZmxvbgAAAAAAAAAGc2hhNTEyAAAAUwAAAAtzc2gtZWQy
NTUxOQAAAEB5IGNhZchfjpSXx79e9NfHKYqBOFW1FYrmwl8W0yofbs24CkygZaY3Lm6XkB
yt80tmWAjS0sCQOBb1IitKuUsO
-----END SSH SIGNATURE-----
";
        assert_eq!(
            signature::verify(PUBLIC_KEY, b"mouflon 1\nversion 1.0.5\n", other),
            Err(signature::SignatureError::OtherKey)
        );
    }
}
