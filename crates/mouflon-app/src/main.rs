//! Mouflon : un gestionnaire de compagnons de bureau, léger et beau.
//!
//! © 2026 Alejandro San Martin Lamas — MIT OR Apache-2.0.
#![forbid(unsafe_code)]
// Sous Windows, pas de console noire derrière les moutons (sauf en mise au
// point : `cargo run` garde la sienne, pour lire le journal).
#![cfg_attr(
    all(target_os = "windows", not(debug_assertions)),
    windows_subsystem = "windows"
)]
// Le binaire n'a que des tests unitaires : le reste paraît inutilisé dans ce contexte.
#![cfg_attr(test, allow(dead_code))]

mod app;
mod display;
mod gpu;
mod helper;
mod i18n;
mod icon;
mod journal;
mod keys;
mod library;
mod menu;
mod quick;
mod scene;
mod screens;
mod settings;
mod settings_window;
mod stats;
mod survey;
mod update;
mod watch;

use winit::event_loop::EventLoop;

/// L'allocateur du système, qui compte les allocations pendant les essais
/// (une lecture atomique par allocation le reste du temps).
#[global_allocator]
static ALLOCATOR: mouflon_platform::CountingAlloc = mouflon_platform::CountingAlloc;

/// Le journal du programme : sur la sortie d'erreur, d'ordinaire. Sous
/// Windows, le programme n'a pas de console : il écrit dans un fichier
/// (`journal.txt`, à côté des images gardées ; celui du lancement d'avant
/// reste dans `journal-precedent.txt`), de quoi comprendre après coup ce qui
/// s'est passé sur une machine qu'on n'a pas sous la main. Ce fichier est
/// écrit par un fil à part (voir [`journal`]) : noter une ligne ne fait
/// jamais attendre le disque.
fn start_log() -> Option<std::path::PathBuf> {
    let mut builder = env_logger::Builder::from_env(env_logger::Env::default().default_filter_or(
        "warn,mouflon=info,mouflon_core=info,mouflon_render=info,mouflon_platform=info",
    ));
    builder.format_timestamp_millis();
    let mut path = None;
    // (Un programme d'essai de la mise à jour aussi, sur les trois systèmes :
    // la version lancée par un relais n'a pas d'autre endroit où écrire.)
    if cfg!(any(
        all(target_os = "windows", not(debug_assertions)),
        feature = "essai-maj"
    )) && let Some(file) = settings::Paths::log_file()
        && let Some(dir) = file.parent()
        && std::fs::create_dir_all(dir).is_ok()
    {
        // Un essai de la mise à jour lance plusieurs Mouflon à la suite : à
        // chacun son journal, et tous restent.
        #[cfg(feature = "essai-maj")]
        let file = dir.join(format!(
            "journal-{}-{}.txt",
            chrono::Local::now().format("%H%M%S%3f"),
            std::process::id()
        ));
        #[cfg(not(feature = "essai-maj"))]
        let _ = std::fs::rename(&file, dir.join("journal-precedent.txt"));
        if let Ok(log) = std::fs::File::create(&file) {
            match journal::start(log) {
                Some(deferred) => {
                    builder.target(env_logger::Target::Pipe(Box::new(deferred)));
                    path = Some(file);
                }
                // (Pas de fil pour l'écrire : droit dans le fichier, alors.)
                None => {
                    if let Ok(log) = std::fs::File::options().append(true).open(&file) {
                        builder.target(env_logger::Target::Pipe(Box::new(log)));
                        path = Some(file);
                    }
                }
            }
        }
    }
    builder.init();
    path
}

/// Une panique, sur n'importe quel fil, est notée au journal. Sous Windows,
/// elle arrête aussi le programme, en le disant : sans cela, une panique du
/// fil de l'affichage laisserait des animaux figés à l'écran, sans que rien
/// ne dise pourquoi. (Pendant un essai, le journal de l'essai la note, et
/// rien ne s'affiche.)
fn watch_panics(log: Option<std::path::PathBuf>) {
    // Ce qu'il faudra dire, préparé d'avance : rien n'est à chercher pendant
    // une panique.
    #[cfg(target_os = "windows")]
    let farewell = {
        let mut text = i18n::texts().crashed.clone();
        if let Some(log) = &log {
            text.push_str("\n\n");
            text.push_str(&log.display().to_string());
        }
        text
    };
    #[cfg(not(target_os = "windows"))]
    let _ = log;
    let previous = std::panic::take_hook();
    std::panic::set_hook(Box::new(move |info| {
        log::error!("panique : {}", info.to_string().replace('\n', " ⏎ "));
        // Le journal sur le disque avant toute chose : c'est lui qu'on lira.
        journal::settle(LOG_WAIT);
        previous(info);
        // Une version qui vient d'arriver et n'a pas encore donné signe de
        // vie s'arrête là : celle d'avant, qui veille, reprend sa place.
        if update::relay::on_trial() {
            std::process::exit(3);
        }
        #[cfg(target_os = "windows")]
        if stats::trial().is_none() && stats::showcase().is_none() {
            mouflon_platform::message("Mouflon", &farewell);
            std::process::exit(1);
        }
    }));
}

/// Ce qu'on laisse au journal pour finir de s'écrire avant de quitter.
const LOG_WAIT: std::time::Duration = std::time::Duration::from_secs(2);

/// Tant qu'il vit, le programme tourne ; quand il est défait (à la sortie
/// de `main`, par où que ce soit), le journal a le temps de s'écrire.
struct LogSettled;

impl Drop for LogSettled {
    fn drop(&mut self) {
        journal::settle(LOG_WAIT);
    }
}

fn main() {
    let first = std::env::args_os().nth(1);
    // « Quelle version es-tu ? » : le dire, et rien d'autre. C'est ainsi
    // qu'un Mouflon qui tourne essaie la version qu'il va installer.
    if first.as_deref() == Some(std::ffi::OsStr::new(update::home::VERSION_ARG)) {
        use std::io::Write;
        let said = update::home::banner(update::Version::current());
        let _ = writeln!(std::io::stdout(), "{said}");
        return;
    }
    let helper = first.as_deref() == Some(std::ffi::OsStr::new(survey::HELPER_ARG));
    // Lancé par la version d'avant, qui lui laisse la place (ou par
    // lui-même, quand la suivante n'a pas pu la prendre).
    let arrival = update::Arrival::read(first.as_deref());
    if arrival.is_some() {
        mouflon_platform::succeed_running();
    }
    // Lancé une seconde fois (Windows) : le premier est prévenu, et fait
    // venir un animal au hasard ; celui-ci s'arrête là, avant d'avoir touché
    // à quoi que ce soit (le journal est celui du premier : c'est lui qui
    // note ce second lancement). Un essai, lui, tourne toujours.
    else if !helper
        && stats::trial().is_none()
        && stats::showcase().is_none()
        && mouflon_platform::already_running()
    {
        return;
    }
    let log = start_log();
    watch_panics(log);
    let _settled = LogSettled;

    // Le processus auxiliaire du relevé (voir `helper`) : rien d'autre.
    if helper {
        helper::run();
        return;
    }

    let paths = settings::Paths::new();
    if let (Some(arrival), Some(paths)) = (arrival, &paths) {
        arrival.announce(&paths.updates);
        // Un programme d'essai fait pour ne pas démarrer : l'essai du retour
        // à l'ancienne version.
        #[cfg(feature = "essai-maj")]
        if arrival == update::Arrival::New && option_env!("MOUFLON_ESSAI_PANNE").is_some() {
            log::error!("essai : cette version s'arrête au démarrage");
            journal::settle(LOG_WAIT);
            std::process::exit(3);
        }
    }
    // Premier lancement : pas encore de réglages enregistrés.
    let first_launch = paths.as_ref().is_some_and(|p| !p.settings.exists());
    let mut settings = paths
        .as_ref()
        .map(|p| settings::Settings::load(&p.settings))
        .unwrap_or_default();
    if first_launch {
        // La taille qui va à l'écran, gardée ensuite comme un choix.
        settings.size = settings::default_size(&mouflon_platform::screens());
        if let Some(paths) = &paths {
            settings.save(&paths.settings);
        }
        log::info!("premier lancement : taille × {}", settings.size);
    }
    // Une version remise à plus tard s'installe maintenant, avant que rien
    // ne se montre : si elle prend le relais, celle-ci a fini.
    if arrival.is_none()
        && settings.auto_update
        && update::offered()
        && let Some(paths) = &paths
        && update::at_launch(&paths.updates)
    {
        return;
    }
    // Les images prêtes se projettent en mémoire depuis le disque.
    let images = paths.as_ref().map(|p| {
        mouflon_render::cache::Store::new(&p.images).opened_with(mouflon_platform::map_file)
    });
    let mut library = library::Library::load(
        paths.as_ref().map(|p| p.pets.as_path()),
        images,
        settings.size_factor(),
    );
    // `mouflon chemin/vers/animal…` : charge ces animaux en plus et lance le
    // premier (pratique pour essayer un animal sans l'installer).
    let mut tried = None;
    for arg in std::env::args_os().skip(1 + usize::from(arrival.is_some())) {
        let id = library.load_path(std::path::Path::new(&arg));
        tried = tried.or(id);
    }
    if let Some(id) = tried {
        settings.default_pet = Some(id);
    }
    if library.pets.is_empty() {
        log::error!("aucun animal disponible");
        return;
    }

    let mut builder = EventLoop::<app::UserEvent>::with_user_event();
    #[cfg(target_os = "macos")]
    {
        use winit::platform::macos::{ActivationPolicy, EventLoopBuilderExtMacOS};
        // Pas d'icône dans le Dock ni dans ⌘-Tab : tout passe par la barre des menus.
        builder
            .with_activation_policy(ActivationPolicy::Accessory)
            .with_default_menu(false);
    }
    // Sous Linux, par X11 quand un serveur X répond (aussi sous Wayland, à
    // travers XWayland) : c'est là que Mouflon voit les fenêtres des autres
    // apps, pose les siennes où il veut et les garde devant. Sans serveur
    // X, winit choisit (Wayland), et les animaux restent au bas de l'écran.
    #[cfg(all(unix, not(target_os = "macos")))]
    {
        use winit::platform::x11::EventLoopBuilderExtX11;
        if !mouflon_platform::screens().is_empty() {
            builder.with_x11();
        }
    }
    let event_loop = match builder.build() {
        Ok(event_loop) => event_loop,
        Err(e) => {
            log::error!("impossible de démarrer : {e}");
            return;
        }
    };
    let proxy = event_loop.create_proxy();
    let updates = paths.as_ref().map(|p| p.updates.clone());
    let mut app = app::App::new(settings, paths, library, proxy, arrival);
    if let Err(e) = event_loop.run_app(&mut app) {
        log::error!("{e}");
    }
    app.shutdown();
    // Une version est prête : elle prend la place de celle-ci, qui veille
    // sur son démarrage avant de s'en aller. Si elle ne prend pas, ce
    // Mouflon-ci se relance tel qu'il était.
    if let (Some(ready), Some(dirs)) = (app.take_relay(), &updates) {
        let version = ready.version;
        if let update::relay::Outcome::Stayed(stayed) = update::relay::go(*ready, dirs) {
            log::warn!(
                "mise à jour : Mouflon {version} n'a pas pris la place ({})",
                stayed.why
            );
            if let Err(e) = stayed.restart() {
                log::error!("mise à jour : Mouflon ne se relance pas ({e})");
            }
        }
    }
    // On sort sans détruire les fenêtres des écrans une à une : ce sont des
    // panneaux dont la classe a changé après leur création, et AppKit, en
    // les libérant, échoue à défaire certaines de ses observations internes
    // (NSRangeException, l'app « quitte de manière imprévue »). Elles sont
    // toutes gardées (`keep_pet_window`) ; le système reprend tout à la
    // sortie du processus. (Sortir ainsi ne défait rien : le journal reçoit
    // ici son temps pour s'écrire.)
    journal::settle(LOG_WAIT);
    std::process::exit(0);
}
