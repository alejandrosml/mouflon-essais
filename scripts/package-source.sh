#!/usr/bin/env bash
# L'archive du code source d'une version : dist/Mouflon-<version>-source.zip.
# Elle sort avec chaque version, à côté des programmes : c'est par elle que
# Mouflon reste open source (MIT ou Apache-2.0, au choix). Elle contient les
# fichiers d'un commit, ses deux licences comprises, et rien de son
# historique. Elle suffit à reconstruire Mouflon : la CI construit le paquet
# Linux à partir d'elle seule.
#
# Ce qui est marqué `export-ignore` dans .gitattributes n'y va pas : les
# outils propres à l'auteur et tous les fichiers Markdown (consignes et notes
# de travail), sauf les crédits et les trois modes d'emploi pour créer un
# animal. Son README n'est pas celui du dépôt : c'est
# packaging/source/README.md, un texte court écrit pour le public, posé ici à
# sa racine.
#
# Lancé depuis un état du code déjà publié (le contenu d'une archive, comme
# dans le dépôt des essais : pas de packaging/source/), il archive cet état
# tel quel, et lui applique les mêmes contrôles.
#
#   bash scripts/package-source.sh [commit, HEAD par défaut] [dossier, dist par défaut]
set -euo pipefail

cd "$(dirname "$0")/.."
REF="${1:-HEAD}"
OUT="${2:-dist}"
# Les seuls fichiers Markdown que le public reçoit (à tenir d'accord avec
# .gitattributes), dans l'ordre de `LC_ALL=C sort`.
MARKDOWN="CREDITS.md README.md docs/format.md docs/rig.md docs/squelette.md"

rate() { echo "✗ $*" >&2; exit 1; }

VERSION="$(git show "$REF:Cargo.toml" | sed -n 's/^version = "\(.*\)"/\1/p' | head -1)"
[[ -n "$VERSION" ]] || rate "$REF : pas de numéro de version dans Cargo.toml"

TMP="$(mktemp -d)"
trap 'rm -rf "$TMP"' EXIT
mkdir -p "$OUT"
ZIP="$OUT/Mouflon-$VERSION-source.zip"
rm -f "$ZIP"
if git cat-file -e "$REF:packaging/source/README.md" 2>/dev/null; then
    git show "$REF:packaging/source/README.md" > "$TMP/README.md"
    # `--add-file` pose le README public à la racine (sous le `--prefix` qui
    # le précède) ; celui du dépôt est écarté par .gitattributes.
    git archive --worktree-attributes --format=zip --prefix="Mouflon-$VERSION/" \
        --add-file="$TMP/README.md" -o "$TMP/source.zip" "$REF"
else
    # Un état déjà publié : tous ses fichiers, tels quels. (`git archive` en
    # écarterait encore les fichiers Markdown, que .gitattributes marque.)
    [[ "$REF" == HEAD ]] || rate "un état déjà publié ne s'archive que depuis HEAD"
    git show HEAD:README.md > "$TMP/README.md" 2>/dev/null || rate "pas de README.md"
    git checkout-index --all --force --prefix="$TMP/Mouflon-$VERSION/"
    ( cd "$TMP" && zip -q -r source.zip "Mouflon-$VERSION" )
fi

# Ce qu'elle doit contenir, et rien d'autre en Markdown.
LISTE="$(unzip -Z1 "$TMP/source.zip")"
for f in LICENSE-MIT LICENSE-APACHE CREDITS.md README.md Cargo.toml Cargo.lock; do
    grep -qxF "Mouflon-$VERSION/$f" <<< "$LISTE" || rate "l'archive n'a pas de $f"
done
unzip -p "$TMP/source.zip" "Mouflon-$VERSION/README.md" | cmp -s - "$TMP/README.md" \
    || rate "le README de l'archive n'est pas celui écrit pour le public (packaging/source/README.md)"
TROUVES="$(grep -E '\.md$' <<< "$LISTE" | sed "s#^Mouflon-$VERSION/##" | LC_ALL=C sort | tr '\n' ' ')"
[[ "$TROUVES" == "$MARKDOWN " ]] \
    || rate "les fichiers Markdown de l'archive devraient être « $MARKDOWN », et ce sont « ${TROUVES% } » (voir .gitattributes)"
# Un dossier dont tout le contenu est écarté resterait là, vide, avec son nom.
VIDES="$(awk '/\/$/ { d[$0] = 1; next }
    { n = split($0, p, "/"); s = ""; for (i = 1; i < n; i++) { s = s p[i] "/"; u[s] = 1 } }
    END { for (x in d) if (!(x in u)) printf "%s ", x }' <<< "$LISTE")"
[[ -z "$VIDES" ]] || rate "dossiers vides dans l'archive, à écarter eux aussi dans .gitattributes : $VIDES"

mv "$TMP/source.zip" "$ZIP"
echo "✓ $ZIP ($(wc -l <<< "$LISTE") entrées)"
