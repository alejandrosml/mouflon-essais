#!/usr/bin/env bash
# Le ménage du dépôt d'essais : il ne garde que ce qui sert encore.
#
#   bash scripts/menage.sh              fait le ménage
#   A_BLANC=1 bash scripts/menage.sh    dit seulement ce qu'il ferait
#
# 1. Une branche d'essai (`essai/…`) à laquelle rien n'est arrivé depuis
#    JOURS_BRANCHE jours (7) est supprimée : le travail qu'elle suivait est
#    fini ou abandonné.
# 2. Les passages des essais terminés depuis plus de HEURES_PASSAGE heures
#    (24) sont effacés, journaux et fichiers compris ; le dernier passage de
#    chaque branche encore là est gardé, quel que soit son âge.
#
# Il lui faut `gh`, identifié (GH_TOKEN), et GITHUB_REPOSITORY.
set -euo pipefail

R="${GITHUB_REPOSITORY:?le dépôt, comme alejandrosml/mouflon-essais}"
PREFIXE="${PREFIXE:-essai/}"
JOURS_BRANCHE="${JOURS_BRANCHE:-7}"
HEURES_PASSAGE="${HEURES_PASSAGE:-24}"
MAINTENANT="$(date -u +%s)"

# heures <date ISO> : son âge, en heures.
heures() { echo $(( (MAINTENANT - $(date -u -d "$1" +%s)) / 3600 )); }
# efface <chemin de l'API> : le supprime, ou le dit seulement (A_BLANC).
efface() {
    if [[ -n "${A_BLANC:-}" ]]; then
        echo "    (à blanc, rien n'est effacé)"
    else
        gh api -X DELETE "repos/$R/$1" > /dev/null
    fi
}

echo "== Les branches d'essai sans nouvelle depuis $JOURS_BRANCHE jours"
gh api "repos/$R/git/matching-refs/heads/${PREFIXE%/}?per_page=100" \
    --jq ".[] | select(.ref | startswith(\"refs/heads/$PREFIXE\")) | \"\(.object.sha) \(.ref | split(\"/\") | map(@uri) | join(\"/\")) \(.ref)\"" | while read -r sha adresse ref; do
    quand="$(gh api "repos/$R/git/commits/$sha" --jq .committer.date)"
    age="$(heures "$quand")"
    if (( age > JOURS_BRANCHE * 24 )); then
        echo "  ${ref#refs/heads/} (dernier état : $quand) : supprimée"
        # Son adresse, signe par signe : un « # » dans un nom de branche
        # couperait l'adresse, et c'est une autre branche qui partirait.
        efface "git/$adresse"
    else
        echo "  ${ref#refs/heads/} (dernier état : $quand) : gardée"
    fi
done

echo "== Les passages terminés depuis plus de $HEURES_PASSAGE heures"
# Toute la liste d'abord, page après page, et les suppressions ensuite :
# effacer pendant qu'on feuillette ferait sauter des lignes.
PASSAGES=""
for page in $(seq 1 100); do
    lot="$(gh api "repos/$R/actions/runs?status=completed&per_page=100&page=$page" \
        --jq '.workflow_runs[] | [.id, (.head_branch // "-"), .updated_at, .name] | map(tostring) | join("\u001f")')"
    [[ -n "$lot" ]] || break
    PASSAGES+="$lot"$'\n'
done
# Les branches encore là, une par ligne, entre deux barres pour les chercher
# sans confondre `essai/a` et `essai/ab`.
BRANCHES="|$(gh api "repos/$R/branches?per_page=100" --jq '.[].name' | tr '\n' '|')"
declare -A VU=()
GARDES=0
EFFACES=0
# Du plus récent au plus ancien : le premier passage vu pour une branche et
# un flux est son dernier.
# (Les champs sont séparés par un signe qui n'est pas un blanc : un passage
# sans branche ne décale pas les suivants.)
while IFS=$'\x1f' read -r id branche quand flux; do
    [[ -n "$id" ]] || continue
    cle="$branche|$flux"
    if [[ -z "${VU[$cle]:-}" && "$BRANCHES" == *"|$branche|"* ]]; then
        VU[$cle]=1
        GARDES=$((GARDES + 1))
    elif (( $(heures "$quand") > HEURES_PASSAGE )); then
        echo "  passage $id ($flux, $branche, $quand) : effacé"
        efface "actions/runs/$id"
        EFFACES=$((EFFACES + 1))
    else
        GARDES=$((GARDES + 1))
    fi
done <<< "$PASSAGES"
echo "  $EFFACES effacé(s), $GARDES gardé(s)"
