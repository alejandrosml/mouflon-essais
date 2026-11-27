#!/bin/sh
# Vérifie les poses du dessin livré avec le vrai moteur, sans ajouter de
# dépendance de test ni modifier le Cargo.lock de l'espace de travail.
set -eu

repo_dir=$(CDPATH= cd -- "$(dirname -- "$0")/../.." && pwd)
rustfmt --edition 2024 --check "$repo_dir/tools/mouflon-peintre/tests/moteur/solognote.rs"
task_dir=$(mktemp -d "${TMPDIR:-/tmp}/mouflon-solognote.XXXXXX")
trap 'rm -rf -- "$task_dir"' EXIT HUP INT TERM
mkdir "$task_dir/tests"
cp "$repo_dir/tools/mouflon-peintre/tests/moteur/solognote.rs" "$task_dir/tests/solognote.rs"
# Les versions déjà retenues par le dépôt servent aussi au contrôle.
cp "$repo_dir/Cargo.lock" "$task_dir/Cargo.lock"
python3 - "$repo_dir" "$task_dir" <<'PY'
import json
import pathlib
import sys

repo, task = map(pathlib.Path, sys.argv[1:])
manifest = '''[package]
name = "mouflon-controle-solognote"
version = "0.0.0"
edition = "2024"
publish = false

[workspace]

[dependencies]
png = "0.18"
'''
for crate in ("mouflon-core", "mouflon-render"):
    manifest += f'{crate} = {{ path = {json.dumps(str(repo / "crates" / crate))} }}\n'
manifest += '''
[profile.release]
opt-level = 3
lto = "thin"
codegen-units = 1
strip = "debuginfo"
'''
(task / "Cargo.toml").write_text(manifest)
PY
MOUFLON_REPOSITORY="$repo_dir" CARGO_TARGET_DIR="${CARGO_TARGET_DIR:-$repo_dir/target}" \
    cargo test --release --manifest-path "$task_dir/Cargo.toml" "$@"
