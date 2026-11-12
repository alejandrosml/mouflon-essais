#!/usr/bin/env bash
# Construit Mouflon pour Linux (x86_64) : dist/Mouflon-<version>-linux-x86_64.tar.gz,
# un dossier « Mouflon » à décompresser où l'on veut (le programme, ses
# animaux, de quoi l'ajouter au menu des applications). Aperçu : voir
# packaging/linux/LISEZMOI.txt.
#
#   bash scripts/bundle-linux.sh
set -euo pipefail

cd "$(dirname "$0")/.."
VERSION="$(sed -n 's/^version = "\(.*\)"/\1/p' Cargo.toml | head -1)"
echo "→ compilation (release)"
cargo build --release --locked -p mouflon-app

OUT=dist/Mouflon
echo "→ assemblage de $OUT"
rm -rf dist
mkdir -p "$OUT/pets"
cp target/release/mouflon "$OUT/mouflon"
cp LICENSE-MIT LICENSE-APACHE CREDITS.md packaging/linux/LISEZMOI.txt "$OUT/"
cp packaging/linux/mouflon.desktop packaging/linux/mouflon.png packaging/linux/installer.sh "$OUT/"
chmod +x "$OUT/installer.sh"
# Les licences des bibliothèques compilées dans le programme.
cargo run -q --locked -p mouflon-licenses -- x86_64-unknown-linux-gnu > "$OUT/THIRD-PARTY-LICENSES.txt"
# Les animaux du dépôt (le Bleu du Maine est intégré au programme).
for pet in pets/*/; do
    name="$(basename "$pet")"
    [[ "$name" == bleu-du-maine ]] && continue
    cp -R "$pet" "$OUT/pets/$name"
done
# Ni fichiers cachés, ni aperçus de travail.
find "$OUT/pets" -name '.*' -prune -exec rm -rf {} +
find "$OUT/pets" -name 'apercu.png' -delete

TAR="dist/Mouflon-$VERSION-linux-x86_64.tar.gz"
tar -C dist -czf "$TAR" Mouflon
echo "✓ $TAR"
