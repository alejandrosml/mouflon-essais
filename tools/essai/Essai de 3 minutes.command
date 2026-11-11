#!/bin/bash
# Essai de trois minutes de Mouflon : double-clique ce fichier.
#
# Mouflon (celui de dist/) tourne trois minutes en notant tout ce qui peut
# expliquer un à-coup : chaque image, chaque événement, la pile du fil qui
# fait les images quand l'une d'elles tarde, les ressources du processus, de chacun de
# ses fils et du système. Autour de lui, le Mac est observé : `sample` (où
# passe le temps de calcul), `top` (les autres apps), `log stream` (messages
# du système), `vm_stat`, `pmset` (chaleur, batterie). Tout va dans
# essais/<date>/, à côté de dist/.
#
# Pendant l'essai, sers-toi des animaux comme d'habitude : attrape-les,
# lance-les, ouvre leur menu (clic droit), ajoutes-en par la barre des menus,
# déplace des fenêtres sous eux. À la fin, Mouflon et cette fenêtre se
# ferment, et une notification (avec un son) te prévient.
#
# Variables : MOUFLON_ESSAI_DUREE (secondes, 180 par défaut) ;
# MOUFLON_ESSAI_CI=1 pour la CI (pas de notification, la fenêtre reste) ;
# MOUFLON_ESSAI_GPU=1 pour dessiner les animaux sur le GPU au lieu des
# calques Core Animation (voir l'autre fichier, « … (GPU).command ») ;
# MOUFLON_ESSAI_INSTANTANES=1 pour des instantanés de ce qui est à l'écran
# (dans instantanes/ ; chacun retient l'affichage quelques millisecondes) ;
# MOUFLON_ESSAI_CHARGE=1 pour un essai de charge (voir « Essai de
# charge.command ») ; MOUFLON_ESSAI_ANIMAUX pour d'autres animaux au
# départ (leurs identifiants, « merinos-d-arles,southdown » par défaut) ;
# MOUFLON_ESSAI_SUFFIXE pour marquer le dossier des résultats (« races »
# donne essais/<date>-races/).

set -u
DUREE="${MOUFLON_ESSAI_DUREE:-180}"
CI="${MOUFLON_ESSAI_CI:-}"
GPU="${MOUFLON_ESSAI_GPU:-}"
CHARGE="${MOUFLON_ESSAI_CHARGE:-}"
INSTANTANES="${MOUFLON_ESSAI_INSTANTANES:-}"
ANIMAUX="${MOUFLON_ESSAI_ANIMAUX:-merinos-d-arles,southdown}"
SUFFIXE="${MOUFLON_ESSAI_SUFFIXE:-}"

# Le dossier du projet : celui de ce fichier, ou deux crans au-dessus
# (tools/essai/ dans le dépôt).
ici="$(cd "$(dirname "$0")" && pwd)"
racine="$ici"
[ -d "$racine/dist/Mouflon.app" ] || racine="$(cd "$ici/../.." && pwd)"
APP="$racine/dist/Mouflon.app"
BIN="$APP/Contents/MacOS/mouflon"
if [ ! -x "$BIN" ]; then
    echo "Mouflon introuvable : $APP"
    exit 1
fi
OUT="$racine/essais/$(date +%Y-%m-%d_%Hh%M)${GPU:+-gpu}${CHARGE:+-charge}${SUFFIXE:+-$SUFFIXE}"
mkdir -p "$OUT"
REPERES="$OUT/reperes.txt"
debut=$(date +%s)
# Une ligne par étape : heure du Mac (secondes), depuis le lancement de
# Mouflon, et ce qui se passe (l'analyse les cale sur le journal).
repere() {
    local maintenant
    maintenant=$(date +%s)
    echo "$maintenant +$(( maintenant - debut )) s : $*" >> "$REPERES"
}
# Arrête un programme lancé à côté, de force s'il le faut.
arrete() {
    local pid
    for pid in "$@"; do
        kill "$pid" 2>/dev/null
    done
    for _ in 1 2 3 4 5; do
        local reste=""
        for pid in "$@"; do
            kill -0 "$pid" 2>/dev/null && reste=1
        done
        [ -z "$reste" ] && return
        sleep 1
    done
    for pid in "$@"; do
        kill -9 "$pid" 2>/dev/null
    done
}

if [ -n "$GPU" ]; then
    echo "Les animaux dessinés sur le GPU (essai de comparaison)."
fi
if [ -n "$CHARGE" ]; then
    echo "Essai de charge : 20, puis 100, 500 et 1 000 animaux."
fi
echo "Essai de $DUREE secondes de Mouflon. Laisse cette fenêtre ouverte :"
echo "elle se fermera d'elle-même. Sers-toi des animaux comme d'habitude."
echo "Résultats : $OUT"

# Le Mouflon déjà lancé s'arrête : l'essai lance le sien.
if [ -z "$CI" ]; then
    pkill -x mouflon >/dev/null 2>&1 && sleep 1
fi

# Le Mac avant l'essai.
{
    echo "== date"; date
    echo "== sw_vers"; sw_vers
    echo "== uname"; uname -a
    echo "== sysctl"
    sysctl hw.model machdep.cpu.brand_string hw.memsize hw.ncpu hw.physicalcpu \
        hw.perflevel0.physicalcpu hw.perflevel1.physicalcpu kern.boottime \
        vm.swapusage kern.memorystatus_vm_pressure_level 2>&1
    echo "== uptime"; uptime
    echo "== pmset -g batt"; pmset -g batt
    echo "== pmset -g therm"; pmset -g therm
    echo "== pmset -g"; pmset -g
    echo "== pmset -g assertions"; pmset -g assertions
    echo "== vm_stat"; vm_stat
    echo "== apparence"; defaults read -g AppleInterfaceStyle 2>&1
    echo "== accessibilité (réduire les animations, la transparence)"
    defaults read com.apple.universalaccess reduceMotion 2>&1
    defaults read com.apple.universalaccess reduceTransparency 2>&1
    # Stage Manager (GloballyEnabled) et ce qu'il garde sur les côtés : des
    # fenêtres que macOS annonce mais qu'on ne voit pas toujours.
    echo "== Stage Manager (com.apple.WindowManager)"
    defaults read com.apple.WindowManager 2>&1
    echo "== Dock (masquage, position, taille)"
    defaults read com.apple.dock autohide 2>&1
    defaults read com.apple.dock orientation 2>&1
    defaults read com.apple.dock tilesize 2>&1
    echo "== écrans"; system_profiler SPDisplaysDataType 2>&1
    echo "== Mouflon"; ls -la "$BIN"; codesign -dv "$APP" 2>&1
} > "$OUT/systeme-avant.txt" 2>&1
# Les apps qui calculent, sur une seconde : le premier relevé de `top` n'a
# pas encore de mesure du processeur (tout y est à 0 %), on garde le second.
# Avant le lancement, c'est la référence (WindowServer, entre autres).
releve_top() {
    top -l 2 -s 1 -n 30 -o cpu -stats pid,command,cpu,mem,power,threads,state 2>&1 \
        | awk '/^Processes:/ { n++ } n >= 2'
}
releve_top > "$OUT/top-avant.txt"

# Ce qui observe le Mac pendant l'essai.
top -l 0 -s 2 -n 15 -o cpu -stats pid,command,cpu,power,threads,idlew,mem,state,csw \
    > "$OUT/top.txt" 2>&1 &
TOP=$!
vm_stat 5 > "$OUT/vm_stat.txt" 2>&1 &
VMSTAT=$!
pmset -g thermlog > "$OUT/thermique.txt" 2>&1 &
THERM=$!
# Messages du système : ceux de Mouflon (AppKit, Metal, Core Animation… y
# écrivent), ceux qui le nomment (runningboardd : sieste des apps, priorité),
# et les erreurs du serveur de fenêtres.
log stream --style compact --level debug --predicate \
    'process == "mouflon" OR (process == "runningboardd" AND eventMessage CONTAINS[c] "mouflon") OR (process == "kernel" AND eventMessage CONTAINS[c] "mouflon") OR (process == "WindowServer" AND (messageType == error OR messageType == fault))' \
    > "$OUT/log.txt" 2>&1 &
LOG=$!

# L'essai.
touch "$OUT/.debut"
debut=$(date +%s)
repere "lancement de Mouflon"
MOUFLON_ESSAI="$DUREE" MOUFLON_ESSAI_COLLISIONS=1 \
    MOUFLON_ESSAI_ANIMAUX="$ANIMAUX" MOUFLON_GPU="${GPU:+1}" \
    MOUFLON_ESSAI_CHARGE="${CHARGE:+1}" \
    MOUFLON_INSTANTANES="${INSTANTANES:+$OUT/instantanes}" \
    MOUFLON_JOURNAL="$OUT/journal.log" \
    RUST_LOG=warn,mouflon=info,mouflon_core=info,mouflon_render=info,mouflon_platform=info \
    "$BIN" > "$OUT/sortie.log" 2>&1 &
PID=$!

# `sample` : où passe le temps de calcul, fil par fil, pendant presque tout
# l'essai (un relevé toutes les 5 ms).
sleep 3
SAMPLE_DUREE=$(( DUREE - 12 ))
[ "$SAMPLE_DUREE" -lt 5 ] && SAMPLE_DUREE=5
repere "sample pendant $SAMPLE_DUREE s"
sample "$PID" "$SAMPLE_DUREE" 5 -mayDie -file "$OUT/sample.txt" > "$OUT/sample-sortie.txt" 2>&1 &
SAMPLE=$!
sleep 1
if ! kill -0 "$SAMPLE" 2>/dev/null && [ -n "$CI" ]; then
    repere "sample refusé : nouvel essai avec sudo"
    # shellcheck disable=SC2024 # le fichier est à nous, pas à root
    sudo -n sample "$PID" "$SAMPLE_DUREE" 5 -mayDie -file "$OUT/sample.txt" \
        > "$OUT/sample-sortie.txt" 2>&1 &
    SAMPLE=$!
fi

# On attend la fin de l'essai (Mouflon quitte de lui-même), au plus une
# minute et demie de plus.
memoire_faite=""
ecoule=0
while kill -0 "$PID" 2>/dev/null; do
    sleep 1
    ecoule=$(( $(date +%s) - debut ))
    if [ $(( ecoule % 15 )) -eq 0 ]; then
        printf "  %3d s / %d s\n" "$ecoule" "$DUREE"
    fi
    # Avec les instantanés : l'écran lui-même, deux fois (ce que macOS
    # montre vraiment, à comparer aux instantanés des calques).
    if [ -n "$INSTANTANES" ]; then
        for t in 13 $(( DUREE * 2 / 3 + 6 )); do
            if [ "$ecoule" -eq "$t" ]; then
                mkdir -p "$OUT/instantanes"
                repere "capture de l'écran"
                screencapture -x "$OUT/instantanes/ecran-${t}s.png" \
                    >> "$OUT/instantanes/capture.log" 2>&1 &
            fi
        done
    fi
    # La mémoire de Mouflon par catégories, vers la fin (le relevé l'arrête
    # un instant : l'à-coup est noté dans les repères).
    if [ -z "$memoire_faite" ] && [ "$ecoule" -ge $(( DUREE - 6 )) ]; then
        memoire_faite=1
        repere "footprint (Mouflon arrêté un instant)"
        footprint "$PID" > "$OUT/memoire.txt" 2>&1 || vmmap -summary "$PID" > "$OUT/memoire.txt" 2>&1
        repere "footprint fini"
    fi
    if [ "$ecoule" -ge $(( DUREE + 90 )) ]; then
        repere "Mouflon ne s'arrête pas : arrêt forcé"
        kill "$PID" 2>/dev/null
        sleep 3
        kill -9 "$PID" 2>/dev/null
        break
    fi
done
wait "$PID" 2>/dev/null
code=$?
repere "Mouflon terminé (code $code)"

# `sample` finit de lui-même ; il écrit ses résultats après l'essai.
for _ in $(seq 1 60); do
    kill -0 "$SAMPLE" 2>/dev/null || break
    sleep 1
done
arrete "$SAMPLE" "$TOP" "$VMSTAT" "$THERM" "$LOG"
repere "relevés arrêtés"

# Le Mac après l'essai.
{
    echo "== date"; date
    echo "== pmset -g batt"; pmset -g batt
    echo "== pmset -g therm"; pmset -g therm
    echo "== vm_stat"; vm_stat
    echo "== sysctl"; sysctl vm.swapusage kern.memorystatus_vm_pressure_level 2>&1
} > "$OUT/systeme-apres.txt" 2>&1
releve_top > "$OUT/top-apres.txt"
# Un rapport de plantage de Mouflon pendant l'essai ?
find "$HOME/Library/Logs/DiagnosticReports" -name 'mouflon*' -newer "$OUT/.debut" \
    -exec cp {} "$OUT/" \; 2>/dev/null

# L'analyse, si Python est là sans rien demander (outils de développement installés).
if [ -n "$CI" ] || xcode-select -p >/dev/null 2>&1; then
    python3 "$racine/tools/essai/analyse.py" "$OUT" > "$OUT/analyse.txt" 2>&1 || true
fi

if grep -q "essai terminé" "$OUT/sortie.log" 2>/dev/null; then
    message="Essai terminé : tu peux revenir sur Claude."
    echo "$message"
else
    message="Mouflon s'est arrêté avant la fin (code $code) : voir sortie.log."
    echo "$message"
fi
echo "Résultats : $OUT"

if [ -z "$CI" ]; then
    # Une notification et un son, puis cette fenêtre se ferme (une fois le
    # script fini, pour que Terminal ne demande rien).
    osascript -e "display notification \"$message\" with title \"Mouflon\"" >/dev/null 2>&1
    afplay /System/Library/Sounds/Glass.aiff >/dev/null 2>&1
    TTY="$(tty)"
    nohup osascript \
        -e 'delay 1.5' \
        -e 'tell application "Terminal"' \
        -e '  set cible to missing value' \
        -e '  repeat with w in windows' \
        -e '    try' \
        -e '      repeat with t in tabs of w' \
        -e "        if tty of t is \"$TTY\" then set cible to id of w" \
        -e '      end repeat' \
        -e '    end try' \
        -e '  end repeat' \
        -e '  if cible is not missing value then close window id cible' \
        -e 'end tell' >/dev/null 2>&1 &
fi
exit 0
