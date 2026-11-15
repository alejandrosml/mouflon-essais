#!/usr/bin/env bash
# Pose un état du code dans le dépôt public des essais, où les essais des
# trois systèmes tournent sans être comptés. Ce qui est posé, c'est le
# contenu de l'archive du code source (scripts/package-source.sh) et rien
# d'autre : ni l'historique, ni les notes de travail.
#
#   bash scripts/push-trial.sh <archive .zip> <dépôt git> <branche> [repère]
#
# L'état part en un seul commit, sans parent, poussé en force sur la branche
# `essai/<branche>` : il remplace le précédent au lieu de s'y ajouter. Le
# repère (le commit du dépôt de travail, en abrégé) est écrit dans son
# message, pour savoir de quel envoi viennent les essais. La CI s'en sert ;
# pour pousser, il lui faut la clé du dépôt des essais.
set -euo pipefail

ZIP="${1:?l archive du code source}"
DEPOT="${2:?le dépôt git où poser le code}"
BRANCHE="${3:?la branche du dépôt de travail}"
REPERE="${4:-}"

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
