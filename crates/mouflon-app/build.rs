//! Sous Windows, le programme porte son icône, sa description (ce que
//! montrent l'Explorateur et le Gestionnaire des tâches) et son manifeste,
//! `packaging/windows/mouflon.exe.manifest`. Ailleurs, rien.
//!
//! Le manifeste dit à Windows, avant de lancer Mouflon :
//! - que ses fenêtres (réglages, galerie, « À propos ») ont les boutons et
//!   les cases d'aujourd'hui, pas ceux de Windows 95 (Common-Controls 6) ;
//! - qu'il se dessine net sur chaque écran, à l'échelle de chacun
//!   (`dpiAwareness`) ;
//! - qu'il est fait pour Windows 10 et 11, et n'a besoin d'aucun droit
//!   particulier (`asInvoker`).
//!
//! (Il reste sans commentaire ni accent : Windows refuse de lancer un
//! programme dont le manifeste lui déplaît.)

fn main() {
    println!("cargo:rerun-if-changed=build.rs");
    println!("cargo:rerun-if-changed=../../packaging/windows/mouflon.ico");
    println!("cargo:rerun-if-changed=../../packaging/windows/mouflon.exe.manifest");
    #[cfg(windows)]
    if std::env::var("CARGO_CFG_TARGET_OS").as_deref() == Ok("windows") {
        let mut resource = winresource::WindowsResource::new();
        resource
            .set_icon("../../packaging/windows/mouflon.ico")
            .set_manifest_file("../../packaging/windows/mouflon.exe.manifest")
            .set("ProductName", "Mouflon")
            .set("FileDescription", "Mouflon")
            .set("LegalCopyright", "© 2026 Alejandro San Martin Lamas")
            .set("CompanyName", "Alejandro San Martin Lamas");
        // Sans les outils de Windows (le compilateur de ressources), le
        // programme se construit quand même, sans icône ni manifeste (ses
        // fenêtres ont alors l'allure d'un vieux Windows).
        if let Err(e) = resource.compile() {
            println!("cargo:warning=icône et manifeste du programme non posés : {e}");
        }
    }
}
