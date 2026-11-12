#!/bin/bash
# Le même essai de trois minutes, mais avec les animaux dessinés sur le GPU
# dans la fenêtre de chaque écran, au lieu des calques Core Animation : de
# quoi comparer les deux façons d'afficher. Les résultats vont dans
# essais/<date>-gpu/.
ici="$(cd "$(dirname "$0")" && pwd)"
MOUFLON_ESSAI_GPU=1 exec "$ici/Essai de 3 minutes.command"
