//! Licences des bibliothèques compilées dans Mouflon.
//!
//! Les licences MIT, BSD, Apache… demandent de joindre leur texte aux copies
//! distribuées du programme : `scripts/bundle-macos.sh` place ce fichier dans
//! l'app (`Contents/Resources/THIRD-PARTY-LICENSES.txt`).
//!
//! ```text
//! cargo run -p mouflon-licenses -- [cible] > THIRD-PARTY-LICENSES.txt
//! ```
//!
//! La cible vaut `aarch64-apple-darwin` par défaut. Seules les dépendances
//! normales de `mouflon-app` sont retenues (ni outils, ni tests).
#![forbid(unsafe_code)]

use std::collections::{BTreeMap, HashMap, HashSet, VecDeque};
use std::path::Path;
use std::process::{Command, ExitCode};

use serde::Deserialize;

#[derive(Deserialize)]
struct Metadata {
    packages: Vec<Package>,
    resolve: Resolve,
}

#[derive(Deserialize)]
struct Package {
    id: String,
    name: String,
    version: String,
    license: Option<String>,
    license_file: Option<String>,
    manifest_path: String,
    repository: Option<String>,
    #[serde(default)]
    authors: Vec<String>,
    /// `None` pour les crates du dépôt.
    source: Option<String>,
}

#[derive(Deserialize)]
struct Resolve {
    nodes: Vec<Node>,
}

#[derive(Deserialize)]
struct Node {
    id: String,
    deps: Vec<NodeDep>,
}

#[derive(Deserialize)]
struct NodeDep {
    pkg: String,
    dep_kinds: Vec<DepKind>,
}

#[derive(Deserialize)]
struct DepKind {
    /// `None` : dépendance normale ; sinon `dev` ou `build`.
    kind: Option<String>,
}

/// Texte de la licence MIT, pour les crates qui la déclarent sans en livrer le fichier.
const MIT_TEXT: &str =
    "Permission is hereby granted, free of charge, to any person obtaining a copy
of this software and associated documentation files (the \"Software\"), to deal
in the Software without restriction, including without limitation the rights
to use, copy, modify, merge, publish, distribute, sublicense, and/or sell
copies of the Software, and to permit persons to whom the Software is
furnished to do so, subject to the following conditions:

The above copyright notice and this permission notice shall be included in all
copies or substantial portions of the Software.

THE SOFTWARE IS PROVIDED \"AS IS\", WITHOUT WARRANTY OF ANY KIND, EXPRESS OR
IMPLIED, INCLUDING BUT NOT LIMITED TO THE WARRANTIES OF MERCHANTABILITY,
FITNESS FOR A PARTICULAR PURPOSE AND NONINFRINGEMENT. IN NO EVENT SHALL THE
AUTHORS OR COPYRIGHT HOLDERS BE LIABLE FOR ANY CLAIM, DAMAGES OR OTHER
LIABILITY, WHETHER IN AN ACTION OF CONTRACT, TORT OR OTHERWISE, ARISING FROM,
OUT OF OR IN CONNECTION WITH THE SOFTWARE OR THE USE OR OTHER DEALINGS IN THE
SOFTWARE.";

/// Fichiers de licence d'un crate : LICENSE*, LICENCE*, COPYING*, NOTICE*.
fn license_texts(package: &Package) -> Vec<(String, String)> {
    let Some(dir) = Path::new(&package.manifest_path).parent() else {
        return Vec::new();
    };
    let mut names: Vec<String> = std::fs::read_dir(dir)
        .map(|entries| {
            entries
                .filter_map(Result::ok)
                .filter(|e| e.path().is_file())
                .filter_map(|e| e.file_name().into_string().ok())
                .filter(|name| {
                    let upper = name.to_ascii_uppercase();
                    ["LICENSE", "LICENCE", "COPYING", "NOTICE"]
                        .iter()
                        .any(|prefix| upper.starts_with(prefix))
                })
                .collect()
        })
        .unwrap_or_default();
    if let Some(file) = &package.license_file
        && !names.contains(file)
    {
        names.push(file.clone());
    }
    names.sort();
    names
        .into_iter()
        .filter_map(|name| {
            let text = std::fs::read_to_string(dir.join(&name)).ok()?;
            let text = text.trim().replace("\r\n", "\n");
            (!text.is_empty()).then_some((name, text))
        })
        .collect()
}

fn main() -> ExitCode {
    let target = std::env::args()
        .nth(1)
        .unwrap_or_else(|| "aarch64-apple-darwin".to_owned());
    let cargo = std::env::var("CARGO").unwrap_or_else(|_| "cargo".to_owned());
    let output = match Command::new(cargo)
        .args([
            "metadata",
            "--format-version",
            "1",
            "--locked",
            "--filter-platform",
            &target,
        ])
        .output()
    {
        Ok(output) if output.status.success() => output,
        Ok(output) => {
            eprintln!(
                "cargo metadata : {}",
                String::from_utf8_lossy(&output.stderr)
            );
            return ExitCode::FAILURE;
        }
        Err(e) => {
            eprintln!("cargo metadata : {e}");
            return ExitCode::FAILURE;
        }
    };
    let metadata: Metadata = match serde_json::from_slice(&output.stdout) {
        Ok(m) => m,
        Err(e) => {
            eprintln!("réponse de cargo metadata illisible : {e}");
            return ExitCode::FAILURE;
        }
    };

    // Dépendances normales de l'app, de proche en proche.
    let packages: HashMap<&str, &Package> = metadata
        .packages
        .iter()
        .map(|p| (p.id.as_str(), p))
        .collect();
    let nodes: HashMap<&str, &Node> = metadata
        .resolve
        .nodes
        .iter()
        .map(|n| (n.id.as_str(), n))
        .collect();
    let Some(root) = metadata
        .packages
        .iter()
        .find(|p| p.name == "mouflon-app" && p.source.is_none())
    else {
        eprintln!("mouflon-app introuvable");
        return ExitCode::FAILURE;
    };
    let mut seen: HashSet<&str> = HashSet::from([root.id.as_str()]);
    let mut queue = VecDeque::from([root.id.as_str()]);
    while let Some(id) = queue.pop_front() {
        let Some(node) = nodes.get(id) else { continue };
        for dep in &node.deps {
            let normal = dep.dep_kinds.iter().any(|k| k.kind.is_none());
            if normal && seen.insert(dep.pkg.as_str()) {
                queue.push_back(dep.pkg.as_str());
            }
        }
    }
    let mut used: Vec<&Package> = seen
        .iter()
        .filter_map(|id| packages.get(id).copied())
        .filter(|p| p.source.is_some())
        .collect();
    used.sort_by(|a, b| (&a.name, &a.version).cmp(&(&b.name, &b.version)));

    // Un même texte (la licence Apache, par exemple) n'est écrit qu'une fois.
    let mut texts: BTreeMap<String, Vec<String>> = BTreeMap::new();
    let mut missing = Vec::new();
    println!("Mouflon — third-party licenses / licences des bibliothèques");
    println!("Target / cible : {target}");
    println!();
    println!(
        "Mouflon includes the following libraries. Mouflon contient les bibliothèques suivantes."
    );
    println!();
    for package in &used {
        let label = format!("{} {}", package.name, package.version);
        println!(
            "- {label} — {}{}",
            package.license.as_deref().unwrap_or("?"),
            package
                .repository
                .as_deref()
                .map(|r| format!(" — {r}"))
                .unwrap_or_default()
        );
        let found = license_texts(package);
        if found.is_empty() {
            missing.push(*package);
        }
        for (file, text) in found {
            texts
                .entry(text)
                .or_default()
                .push(format!("{label} ({file})"));
        }
    }
    for (text, users) in &texts {
        println!();
        println!("{}", "=".repeat(78));
        println!("Used by / utilisé par : {}", users.join(", "));
        println!("{}", "-".repeat(78));
        println!("{text}");
    }
    // Sans fichier dans le paquet : le texte MIT quand la licence le permet,
    // avec les auteurs déclarés ; sinon, le texte de la licence figure plus haut.
    let (mit, other): (Vec<&Package>, Vec<&Package>) = missing
        .iter()
        .partition(|p| p.license.as_deref().is_some_and(|l| l.contains("MIT")));
    if !mit.is_empty() {
        println!();
        println!("{}", "=".repeat(78));
        println!("MIT License (no license file in the package / sans fichier dans le paquet)");
        println!("{}", "-".repeat(78));
        for package in &mit {
            let authors = if package.authors.is_empty() {
                format!("the {} developers", package.name)
            } else {
                package.authors.join(", ")
            };
            println!(
                "{} {} : Copyright (c) {authors}",
                package.name, package.version
            );
        }
        println!();
        println!("{MIT_TEXT}");
    }
    for package in &other {
        println!();
        println!("{}", "=".repeat(78));
        println!(
            "{} {} — {} (license text above / texte de la licence ci-dessus){}",
            package.name,
            package.version,
            package.license.as_deref().unwrap_or("?"),
            package
                .repository
                .as_deref()
                .map(|r| format!(" — {r}"))
                .unwrap_or_default()
        );
    }
    eprintln!(
        "{} bibliothèques, {} textes de licence, {} sans fichier",
        used.len(),
        texts.len(),
        missing.len()
    );
    ExitCode::SUCCESS
}
