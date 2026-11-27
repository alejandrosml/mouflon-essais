#!/bin/bash
# Essai de charge : le même essai, mais des animaux arrivent par paliers
# (20, puis 100, 500 et 1 000) pendant deux minutes et demie, pour voir
# comment Mouflon tient la foule. L'analyse donne, palier par palier, les
# images en retard et le temps passé à chacune. Les résultats vont dans
# essais/<date>-charge/.
ici="$(cd "$(dirname "$0")" && pwd)"
MOUFLON_ESSAI_CHARGE=1 MOUFLON_ESSAI_DUREE="${MOUFLON_ESSAI_DUREE:-150}" \
    exec "$ici/Essai de 3 minutes.command"
