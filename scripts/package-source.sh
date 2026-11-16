#!/usr/bin/env bash
# L'archive du code source d'une version : dist/Mouflon-<version>-source.zip.
# Elle sort avec chaque version, à côté des programmes : c'est par elle que
# Mouflon reste open source (MIT ou Apache-2.0, au choix). Elle contient les
# fichiers d'un commit, ses deux licences comprises, et rien de son
# historique. Elle suffit à reconstruire Mouflon : la CI construit le paquet
# Linux à partir d'elle seule.
#
# Elle ne contient que ce qui est nommé plus bas : rien de nouveau n'y entre
# sans y avoir été ajouté. Son README est un texte court écrit pour elle
# (packaging/source/README.md, posé à sa racine).
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
# Ce que l'archive peut contenir : rien de nouveau à la racine, dans docs/ ni
# dans tools/ n'y entre sans être nommé ici.
RACINE_PUBLIQUE=".gitattributes .github .gitignore CREDITS.md Cargo.lock Cargo.toml LICENSE-APACHE LICENSE-MIT README.md crates docs packaging pets scripts tests tools"
DOCS_PUBLICS="format.md poses.png rig.md sources.md squelette.md"
OUTILS_PUBLICS="audio essai inbetween mouflon-licenses mouflon-peintre mouflon-preview races"
# Ses seuls fichiers Markdown, dans l'ordre de `LC_ALL=C sort`.
MARKDOWN="CREDITS.md README.md docs/format.md docs/rig.md docs/sources.md docs/squelette.md"
# Ses seuls fichiers et dossiers cachés, tous à sa racine.
CACHES=".gitattributes .github .gitignore"

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
    # Ce qui ne doit jamais partir, quoi que disent les listes : une
    # expression par ligne (packaging/source/interdits).
    git show "$REF:packaging/source/interdits" > "$TMP/interdits.brut" 2>/dev/null \
        || rate "$REF : pas de packaging/source/interdits"
    { grep -vE '^(#|$)' "$TMP/interdits.brut" || true; } > "$TMP/interdits"
    [[ -s "$TMP/interdits" ]] || rate "packaging/source/interdits ne nomme rien"
    # Le .gitattributes de l'archive : celui du dépôt, sans ce qui dit ce
    # qu'on écarte.
    git show "$REF:.gitattributes" | sed "/^# Ce qui ne part pas dans l'archive/,\$d" > "$TMP/.gitattributes"
    # `--add-file` pose ces deux fichiers à la racine (sous le `--prefix` qui
    # le précède) ; ceux du dépôt sont écartés par .gitattributes.
    git archive --worktree-attributes --format=zip --prefix="Mouflon-$VERSION/" \
        --add-file="$TMP/README.md" --add-file="$TMP/.gitattributes" -o "$TMP/source.zip" "$REF"
else
    # Un état déjà publié : tous ses fichiers, tels quels.
    : > "$TMP/interdits"
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
CHEMINS="$(sed "s#^Mouflon-$VERSION/##" <<< "$LISTE" | grep -v '^$' || true)"
TROUVES="$({ grep -iE '\.(md|markdown|mdx)$' <<< "$CHEMINS" || true; } | LC_ALL=C sort | tr '\n' ' ')"
[[ "$TROUVES" == "$MARKDOWN " ]] \
    || rate "les fichiers Markdown de l'archive devraient être « $MARKDOWN », et ce sont « ${TROUVES% } »"
# Rien que ce qui est nommé plus haut : à la racine, dans docs/, dans tools/.
nouveaux() {  # nouveaux <dossier/ ou rien> <noms permis> : ce qui s'y trouve sans y être permis
    { grep "^$1" <<< "$CHEMINS" || true; } | sed "s#^$1##" | cut -d/ -f1 | { grep -v '^$' || true; } | LC_ALL=C sort -u \
        | { grep -vxF -f <(tr ' ' '\n' <<< "$2") || true; } | tr '\n' ' '
}
EN_TROP="$(nouveaux "" "$RACINE_PUBLIQUE")$(nouveaux "docs/" "$DOCS_PUBLICS" | sed 's#[^ ]\{1,\}#docs/&#g')$(nouveaux "tools/" "$OUTILS_PUBLICS" | sed 's#[^ ]\{1,\}#tools/&#g')"
[[ -z "$EN_TROP" ]] \
    || rate "l'archive contient du nouveau, qu'on n'a pas nommé dans ce script : ${EN_TROP% }"
CACHES_EN_TROP="$({ grep -E '(^|/)\.[^/]' <<< "$CHEMINS" || true; } | sed -E 's#^((.*/)?\.[^/]+).*#\1#' | LC_ALL=C sort -u \
    | { grep -vxF -f <(tr ' ' '\n' <<< "$CACHES") || true; } | tr '\n' ' ')"
[[ -z "$CACHES_EN_TROP" ]] \
    || rate "l'archive contient des fichiers cachés qu'on n'a pas nommés dans ce script : ${CACHES_EN_TROP% }"
if [[ -s "$TMP/interdits" ]]; then
    PRIVES="$({ grep -iE -f "$TMP/interdits" <<< "$CHEMINS" || true; } | head -5 | tr '\n' ' ')"
    [[ -z "$PRIVES" ]] || rate "l'archive contient ce qui ne doit jamais partir : ${PRIVES% }"
fi
! unzip -p "$TMP/source.zip" "Mouflon-$VERSION/.gitattributes" | grep "export-ignore" > /dev/null \
    || rate "le .gitattributes de l'archive dit encore ce qu'on écarte"
# Un dossier dont tout le contenu est écarté resterait là, vide, avec son nom.
VIDES="$(awk '/\/$/ { d[$0] = 1; next }
    { n = split($0, p, "/"); s = ""; for (i = 1; i < n; i++) { s = s p[i] "/"; u[s] = 1 } }
    END { for (x in d) if (!(x in u)) printf "%s ", x }' <<< "$LISTE")"
[[ -z "$VIDES" ]] || rate "dossiers vides dans l'archive, à écarter eux aussi dans .gitattributes : $VIDES"

mv "$TMP/source.zip" "$ZIP"
echo "✓ $ZIP ($(wc -l <<< "$LISTE") entrées)"
