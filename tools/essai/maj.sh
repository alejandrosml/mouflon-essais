#!/usr/bin/env bash
# Essai de la mise à jour de Mouflon par lui-même, d'un bout à l'autre, sur
# Linux et sur Mac, sans rien demander au réseau.
#
#   bash tools/essai/maj.sh [dossier où laisser les journaux]
#
# Trois programmes d'essai sont construits (`--features essai-maj`) : la
# version d'ici, une « plus récente », et une « plus récente » faite pour ne
# pas démarrer. Un dossier tient lieu de dépôt des versions ; leur liste est
# signée avec une clé faite pour l'occasion, que ces programmes portent à la
# place de celle de l'auteur. Un programme d'essai ne s'installe jamais à la
# place du vrai Mouflon, ni le vrai à la sienne.
#
# Trois situations :
#   1. une version plus récente sort : Mouflon la trouve, l'installe, lui
#      laisse la place, et fait le ménage ;
#   2. elle ne démarre pas : l'ancienne reprend sa place, telle qu'elle
#      était ; au lancement suivant elle réessaie une fois, puis plus jamais ;
#   3. une version remise à plus tard s'installe au lancement suivant, sans
#      rien demander au dépôt.
#
# Sur Linux, il faut un écran (`xvfb-run -a bash tools/essai/maj.sh`) ; sur
# Mac, l'app de dist/ (`bash scripts/bundle-macos.sh`).
#
# Sur Mac, l'essai se sert des dossiers de Mouflon du compte (ce qu'il
# retient d'une mise à jour, ce qu'il a téléchargé) et les vide : il ne se
# lance que sur une machine d'essai (la CI), ou avec MOUFLON_ESSAI_ICI=1.
set -euo pipefail
cd "$(dirname "$0")/../.."
RACINE="$PWD"
SORTIE="${1:-essais/maj}"
mkdir -p "$SORTIE"
SORTIE="$(cd "$SORTIE" && pwd)"

MAC=""
[[ "$(uname)" == Darwin ]] && MAC=1
if [[ -n "$MAC" && -z "${CI:-}" && -z "${MOUFLON_ESSAI_ICI:-}" ]]; then
    echo "✗ sur Mac, cet essai vide les dossiers de mise à jour de Mouflon de ce compte : il ne se lance que sur une machine d'essai (MOUFLON_ESSAI_ICI=1 pour le faire quand même)" >&2
    exit 1
fi
VERSION="$(sed -n 's/^version = "\(.*\)"/\1/p' Cargo.toml | head -1)"
NOUVELLE=90.0.0
CASSEE=90.0.1

W="$(mktemp -d "${TMPDIR:-/tmp}/mouflon-maj.XXXXXX")"
# Le chemin tel que le système le donne aux programmes (sur Mac, /var est un
# lien) : c'est par lui qu'on les retrouve.
W="$(cd "$W" && pwd -P)"
# Un espace dans le chemin : c'est souvent le cas chez les gens.
CHEZ="$W/chez moi"
mkdir -p "$W/programmes" "$W/depot" "$CHEZ"

if [[ -n "$MAC" ]]; then
    CIBLE="--target aarch64-apple-darwin"
    CONSTRUIT="target/aarch64-apple-darwin/release/mouflon"
    FICHIER="Mouflon-macos-arm64.zip"
    PROGRAMME="$CHEZ/Mouflon.app/Contents/MacOS/mouflon"
    # Là où une préparation ou une ancienne version resteraient.
    AUTOUR="$CHEZ"
    ETAT="$HOME/Library/Application Support/com.alejandrosanmartinlamas.Mouflon/maj"
    CACHE="$HOME/Library/Caches/com.alejandrosanmartinlamas.Mouflon"
    [[ -d dist/Mouflon.app ]] || { echo "✗ pas de dist/Mouflon.app : bash scripts/bundle-macos.sh d'abord" >&2; exit 1; }
    somme() { shasum -a 512 "$1" | cut -d' ' -f1; }
else
    CIBLE=""
    CONSTRUIT="target/release/mouflon"
    FICHIER="Mouflon-linux-x86_64.tar.gz"
    PROGRAMME="$CHEZ/Mouflon/mouflon"
    AUTOUR="$CHEZ/Mouflon"
    export XDG_DATA_HOME="$W/donnees" XDG_CONFIG_HOME="$W/reglages" XDG_CACHE_HOME="$W/cache"
    ETAT="$XDG_DATA_HOME/mouflon/maj"
    CACHE="$XDG_CACHE_HOME/mouflon"
    somme() { sha512sum "$1" | cut -d' ' -f1; }
fi

dit() { echo "→ $*"; }
rate() {
    echo "✗ $*" >&2
    echo "ÉCHEC : $*" >> "$SORTIE/maj.txt"
    exit 1
}

# Arrête tous les Mouflon de l'essai (et ce qu'ils ont lancé).
arrete() {
    pkill -f "$W/" 2>/dev/null || true
    sleep 1
    pkill -9 -f "$W/" 2>/dev/null || true
}

# À la fin, quoi qu'il arrive : plus aucun Mouflon, et les journaux gardés.
fin() {
    arrete
    mkdir -p "$SORTIE/journaux"
    cp "$CACHE"/journal-*.txt "$SORTIE/journaux/" 2>/dev/null || true
    cp "$W"/console-*.log "$SORTIE/journaux/" 2>/dev/null || true
    cp "$ETAT/etat.toml" "$SORTIE/etat.toml" 2>/dev/null || true
    rm -rf "$W"
}
trap fin EXIT

# attend <secondes> <commande…> : vrai dès que la commande réussit.
attend() {
    local reste="$1"
    shift
    while ! "$@" >/dev/null 2>&1; do
        reste=$((reste - 1))
        [[ $reste -gt 0 ]] || return 1
        sleep 1
    done
}

# ——— La clé d'essai et les trois programmes ———

ssh-keygen -q -t ed25519 -N "" -C essai -f "$W/cle"
CLE="$(cut -d' ' -f1,2 "$W/cle.pub")"

# construit <nom> [VARIABLE=valeur…]
construit() {
    local nom="$1"
    shift
    dit "construction du programme d'essai « $nom »"
    # shellcheck disable=SC2086
    env "$@" MOUFLON_ESSAI_CLE="$CLE" \
        cargo build --release --locked -p mouflon-app --features essai-maj $CIBLE
    cp "$CONSTRUIT" "$W/programmes/$nom"
}
construit ici
construit nouvelle MOUFLON_ESSAI_VERSION=$NOUVELLE
construit cassee MOUFLON_ESSAI_VERSION=$CASSEE MOUFLON_ESSAI_PANNE=1

# ——— Poser un Mouflon, tel qu'une version le livre ———

# pose <nom du programme> <version> <dossier> : Mouflon.app (Mac) ou le
# dossier Mouflon (Linux) dans <dossier>.
pose() {
    local nom="$1" version="$2" ou="$3"
    mkdir -p "$ou"
    if [[ -n "$MAC" ]]; then
        rm -rf "$ou/Mouflon.app"
        cp -R dist/Mouflon.app "$ou/Mouflon.app"
        cp "$W/programmes/$nom" "$ou/Mouflon.app/Contents/MacOS/mouflon"
        plutil -replace CFBundleShortVersionString -string "$version" "$ou/Mouflon.app/Contents/Info.plist"
        codesign --force --sign - "$ou/Mouflon.app" 2>/dev/null
    else
        rm -rf "$ou/Mouflon"
        if [[ -d dist/Mouflon/pets ]]; then
            cp -R dist/Mouflon "$ou/Mouflon"
        else
            mkdir -p "$ou/Mouflon/pets"
            cp -R pets/merinos-d-arles pets/southdown "$ou/Mouflon/pets/"
            cp packaging/linux/LISEZMOI.txt "$ou/Mouflon/"
        fi
        cp "$W/programmes/$nom" "$ou/Mouflon/mouflon"
        echo "$version" > "$ou/Mouflon/version-de-l-essai.txt"
    fi
}

# depot <nom du programme> <version> : le dépôt ne contient plus que cette
# version, avec sa liste signée.
depot() {
    local nom="$1" version="$2"
    rm -rf "$W/depot" "$W/fabrique"
    mkdir -p "$W/depot"
    pose "$nom" "$version" "$W/fabrique"
    if [[ -n "$MAC" ]]; then
        ditto -c -k --keepParent "$W/fabrique/Mouflon.app" "$W/depot/$FICHIER"
    else
        tar -C "$W/fabrique" -czf "$W/depot/$FICHIER" Mouflon
    fi
    {
        echo "version $version"
        echo "fichier $FICHIER $(wc -c < "$W/depot/$FICHIER" | tr -d ' ') $(somme "$W/depot/$FICHIER")"
    } > "$W/depot/Mouflon-version.txt"
    ssh-keygen -q -Y sign -n mouflon -f "$W/cle" "$W/depot/Mouflon-version.txt"
}

# installe : un Mouflon d'ici tout neuf chez l'utilisateur, qui n'a rien
# retenu d'une mise à jour passée.
installe() {
    arrete
    rm -rf "$CHEZ" "$ETAT" "$CACHE/maj"
    mkdir -p "$CHEZ"
    pose ici "$VERSION" "$CHEZ"
}

# lance <n> [VARIABLE=valeur…] : le Mouflon installé, comme le lancerait
# l'utilisateur.
lance() {
    local n="$1"
    shift
    env "$@" "$PROGRAMME" > "$W/console-$n.log" 2>&1 &
}

dit_sa_version() { "$PROGRAMME" --version; }
tourne() { pgrep -f "$W/.*$1" >/dev/null; }
sans_reste() { ! ls -A "$AUTOUR" | grep -q '^\.mouflon-'; }
retenu() { grep -qF "$1" "$ETAT/etat.toml"; }

: > "$SORTIE/maj.txt"

# ——— 1. Une version plus récente sort ———

dit "1. une version plus récente sort"
installe
depot nouvelle $NOUVELLE
[[ "$(dit_sa_version)" == "Mouflon $VERSION (essai)" ]] || rate "le Mouflon installé ne dit pas sa version"
lance 1 MOUFLON_ESSAI_SOURCE="$W/depot"
attend 180 retenu "installee = \"$NOUVELLE\"" || rate "1 : la version $NOUVELLE n'a pas été installée"
[[ "$(dit_sa_version)" == "Mouflon $NOUVELLE (essai)" ]] || rate "1 : le programme en place n'est pas la $NOUVELLE"
tourne "relais=arrivee" || rate "1 : la nouvelle version ne tourne pas"
[[ ! -e "$CACHE/maj" ]] || rate "1 : le téléchargement est resté"
# L'ancienne version reste à côté deux minutes (le temps que la nouvelle soit
# sûrement sur le disque), puis la nouvelle l'enlève.
[[ -d "$AUTOUR/.mouflon-ancien" ]] || rate "1 : l'ancienne version n'a pas attendu à côté"
attend 330 sans_reste || rate "1 : l'ancienne version ou la préparation sont restées : $(ls -A "$AUTOUR")"
tourne "relais=arrivee" || rate "1 : la nouvelle version s'est arrêtée"
if [[ -z "$MAC" ]]; then
    [[ "$(cat "$CHEZ/Mouflon/version-de-l-essai.txt")" == "$NOUVELLE" ]] || rate "1 : ce qui accompagne le programme n'a pas suivi"
fi
echo "1. version plus récente : installée, lancée, ménage fait" | tee -a "$SORTIE/maj.txt"

# ——— 2. Elle ne démarre pas ———

dit "2. une version qui ne démarre pas"
installe
depot cassee $CASSEE
cp -R "$CHEZ" "$W/temoin"
tel_quel() {
    diff -r "$W/temoin" "$CHEZ" > "$W/console-2-diff.log" 2>&1 || return 1
    if [[ -n "$MAC" ]]; then
        codesign --verify --deep --strict "$CHEZ/Mouflon.app" || return 1
    fi
}
lance 2a MOUFLON_ESSAI_SOURCE="$W/depot"
attend 180 retenu "manquees = [\"$CASSEE\"]" || rate "2 : l'installation manquée de la $CASSEE n'a pas été retenue"
attend 60 tourne "relais=retour" || rate "2 : l'ancienne version n'est pas revenue"
[[ "$(dit_sa_version)" == "Mouflon $VERSION (essai)" ]] || rate "2 : le programme en place n'est plus celui d'avant"
attend 60 sans_reste || rate "2 : il reste quelque chose de l'essai manqué : $(ls -A "$AUTOUR")"
tel_quel || rate "2 : le Mouflon revenu n'est pas tel qu'il était"
# Le Mouflon revenu ne réessaie pas de toute sa vie : dix secondes plus
# tard, il est toujours là, et rien de plus n'est retenu.
sleep 10
tourne "relais=retour" || rate "2 : le Mouflon revenu s'est arrêté"
retenu "installee" && rate "2 : une version est dite installée"
retenu "refusees = []" || rate "2 : la version est refusée dès la première fois"
sans_reste || rate "2 : le Mouflon revenu a réessayé"
# Au lancement suivant, il réessaie une fois : c'est la deuxième, la version
# est refusée.
arrete
lance 2b MOUFLON_ESSAI_SOURCE="$W/depot"
attend 180 retenu "refusees = [\"$CASSEE\"]" || rate "2 : la version $CASSEE n'a pas été refusée la deuxième fois"
attend 60 tourne "relais=retour" || rate "2 : l'ancienne version n'est pas revenue la deuxième fois"
attend 60 sans_reste || rate "2 : il reste quelque chose du deuxième essai manqué"
tel_quel || rate "2 : le Mouflon revenu la deuxième fois n'est pas tel qu'il était"
# Et plus jamais : un lancement de plus la trouve, et n'y touche pas.
arrete
lance 2c MOUFLON_ESSAI_SOURCE="$W/depot"
sleep 15
tourne "Mouflon" || rate "2 : le Mouflon lancé ensuite s'est arrêté"
tourne "relais=" && rate "2 : la version refusée a été réessayée"
sans_reste || rate "2 : la version refusée a été préparée de nouveau"
tel_quel || rate "2 : la version refusée a changé quelque chose"
rm -rf "$W/temoin"
echo "2. version qui ne démarre pas : l'ancienne revient telle quelle ; refusée à la deuxième fois" | tee -a "$SORTIE/maj.txt"

# ——— 3. Une version remise à plus tard ———

dit "3. une version remise à plus tard"
installe
depot nouvelle $NOUVELLE
mkdir -p "$ETAT" "$CACHE/maj"
cp "$W/depot/$FICHIER" "$CACHE/maj/Mouflon-$NOUVELLE-$FICHIER"
cp "$W/depot/Mouflon-version.txt" "$W/depot/Mouflon-version.txt.sig" "$CACHE/maj/"
echo "reportee = \"$NOUVELLE\"" > "$ETAT/etat.toml"
# Le dépôt disparaît : tout ce qu'il faut est déjà là.
rm -rf "$W/depot"
lance 3
attend 180 retenu "installee = \"$NOUVELLE\"" || rate "3 : la version remise à plus tard n'a pas été installée"
[[ "$(dit_sa_version)" == "Mouflon $NOUVELLE (essai)" ]] || rate "3 : le programme en place n'est pas la $NOUVELLE"
tourne "relais=arrivee" || rate "3 : la nouvelle version ne tourne pas"
retenu "reportee" && rate "3 : la version est encore dite remise à plus tard"
[[ -d "$AUTOUR/.mouflon-ancien" ]] || rate "3 : l'ancienne version n'a pas attendu à côté"
echo "3. version remise à plus tard : installée au lancement" | tee -a "$SORTIE/maj.txt"

echo "essai de la mise à jour : tout est bon" | tee -a "$SORTIE/maj.txt"
