#!/usr/bin/env bash
# Pose un état du code dans le dépôt public des essais, où les essais des
# trois systèmes tournent. Ce qui est posé, c'est le contenu de l'archive du
# code source (scripts/package-source.sh) et rien d'autre : ni l'historique,
# ni ce que l'archive écarte.
#
#   bash scripts/push-trial.sh <archive .zip> <dépôt git> <branche> [commit]
#
# L'état part en un seul commit, fait au moment de l'envoi, sans parent,
# poussé en force sur la branche `essai/<branche>` : il remplace le précédent
# au lieu de s'y ajouter. Le commit d'où vient l'état, s'il est donné (en
# entier, quarante signes), est écrit à la fin de son message : l'archive du
# code faite là-bas le porte alors en commentaire.
#
# Du lundi au vendredi, entre 6 h et 18 h, heure de Paris, rien n'est posé.
set -euo pipefail

ZIP="${1:?l archive du code source}"
DEPOT="${2:?le dépôt git où poser le code}"
BRANCHE="${3:?la branche}"
REPERE="${4:-}"

# Du lundi (1) au vendredi (5), de 6 h à 17 h 59, heure de Paris : rien.
# Une heure qui ne se lit pas, ou un décalage qui n'est pas celui de Paris
# (+0100 ou +0200 : sans son fichier de fuseau, `date` donnerait l'heure
# UTC), arrête tout aussi.
PARIS="$(TZ=Europe/Paris date '+%u %H%M %z')"
case "$PARIS" in
    [1-5]\ 0[6-9][0-5][0-9]\ +0[12]00 | [1-5]\ 1[0-7][0-5][0-9]\ +0[12]00)
        echo "✗ il est $PARIS à Paris (jour de la semaine, heure, décalage) : du lundi au vendredi, rien n'est posé entre 6 h et 18 h" >&2
        exit 1 ;;
    [1-7]\ [0-1][0-9][0-5][0-9]\ +0[12]00 | [1-7]\ 2[0-3][0-5][0-9]\ +0[12]00) ;;
    *)
        echo "✗ le jour et l'heure de Paris ne se lisent pas : « $PARIS »" >&2
        exit 1 ;;
esac

T="$(mktemp -d)"
trap 'rm -rf "$T"' EXIT
unzip -q "$ZIP" -d "$T"
CODE=("$T"/Mouflon-*)
[[ ${#CODE[@]} -eq 1 && -f "${CODE[0]}/Cargo.toml" ]] \
    || { echo "✗ $ZIP n'est pas une archive du code de Mouflon" >&2; exit 1; }
cd "${CODE[0]}"
VERSION="$(sed -n 's/^version = "\(.*\)"/\1/p' Cargo.toml | head -1)"

git init -q -b etat
# `-f` : tout ce que l'archive contient, même ce que son .gitignore écarterait.
git add -A -f
git -c user.name="Alejandro San Martin Lamas" \
    -c user.email="alejandrosml@users.noreply.github.com" \
    commit -q -m "Mouflon $VERSION : état à essayer${REPERE:+ ($REPERE)}"
git push -q --force "$DEPOT" "HEAD:refs/heads/essai/$BRANCHE"
echo "✓ posé dans essai/$BRANCHE : $(git rev-parse --short HEAD), $(git ls-files | wc -l) fichiers"
