#!/usr/bin/env python3
"""Génère le rig SVG de la brebis Lacaune (Lacaune lait, rayon de Roquefort)
pour Mouflon : une brebis de trois à quatre ans en juin, tondue à la mi-mai
(repousse de 1 à 2 cm), en pleine lactation, de profil, tournée vers la
droite. D'après la fiche docs/races/lacaune.md (les § cités plus bas).

Repère de travail : le « monde », en centimètres de l'animal. x vers
l'avant (0 à l'arrière de la fesse), h vers le haut (0 au sol). Dans le SVG,
1 unité = 1 cm : X = x + OX, Y = OY - h (voir outils.py).

Le dessin est fait d'une pièce par patte, puis recoupé au carpe et au jarret
par tools/races/articulations.py (split_leg) ; la lèvre supérieure est
détachée de la tête (lift_part), avec le bourrelet dentaire dessous.
L'encolure est dessinée à part, sous le corps (variante A du contrat) : le
corps recouvre sa base, ses remplissages au repos sont ceux du corps (même
dégradé), et sa partie cachée est taillée pour que le dessus du cou reste
lisse quand elle descend brouter.

La lumière vient d'en haut à gauche (de l'arrière), comme celle que le
moteur ajoute au corps.

Usage : python3 tools/races/lacaune/gen_lacaune.py <sortie.svg>

© 2026 Alejandro San Martin Lamas — MIT OR Apache-2.0
"""
import math
import os
import random
import sys
import xml.etree.ElementTree as ET

HERE = os.path.dirname(os.path.abspath(__file__))
sys.path.insert(0, os.path.dirname(HERE))
sys.path.insert(0, HERE)
import articulations as A  # noqa: E402
import variantes  # noqa: E402

# Les autres dessins de la race (tools/races/variantes.py), d'après la fiche.
# Sans variante : la brebis en lactation.
VARIANTE = sys.argv[2] if len(sys.argv) > 2 else ""
PROPORTIONS = {
    "belier": dict(  # Bélier Lacaune : 78 cm au garrot, sans cornes, tête plus large, encolure plus épaisse, pas de mamelle.
        garrot=(70, 78), encolure=(1.0, 1.1), tete=(1.05, 1.08)),
    "agneau": dict(  # Agneau Lacaune de deux semaines : 42 cm au garrot, blanc, au poil ras, longues pattes.
        variantes.AGNEAU, garrot=(70, 42), corps=(.84, .9)),
}
from shapely.geometry import Point  # noqa: E402
from outils import (Doc, Polygon, S, add, clamp, cr, dense, dist, element, geom_d, lerp, lstroke, mix,  # noqa: E402
                    mul, norm, poisson, poly, qstroke, rot, runs_path, shade, sub)

OUT = sys.argv[1] if len(sys.argv) > 1 else "lacaune.svg"
rng = random.Random(1902)          # le standard de la race fut fixé en 1902 (fiche § 1.1)

# ================================================================== mesures (fiche § 2)
H_GARROT = 71.2      # § 2.2 : 70 cm au garrot (71,2 au sommet du garrot, laine de 1 cm comprise)
H_CROUPE = 70.55     # § 2.3 : la croupe à 100-101 % du garrot
H_STERNUM = 34.6     # § 2.3 : sol -> sternum, 35 cm (« longs membres »)
H_VENTRE = 29.4      # § 2.3 : le point le plus bas du ventre d'une laitière, 28 cm (38-42 % H)
L_TETE = 23.5        # § 2.7 : nuque -> bout du nez ; la fiche retient 0,32 H, ses photos 0,33-0,36 (P10, P15)
H_CARPE = 17.0       # § 2.6 : 0,23-0,25 H
H_JARRET = 21.5      # § 2.6 : 0,28-0,32 H
TETE_ANGLE = 43.0    # § 2.5 : axe nuque -> nez à 43° (le chanfrein à 48-50°)
# La queue naturelle « jusqu'au jarret » (§ 2.8, S1) : de 62,6 à 25 cm du sol,
# elle mesure 39 cm en suivant la fesse (la fiche dit 30 cm : voir le rapport).
QUEUE_BAS = 25.0
# La mamelle (§ 2.9) : bas à 23,4 cm du sol, profondeur visible 15 cm,
# longueur visible 14,6 cm, trayon de 3,2 cm pointé en avant à 45°.

# ================================================================== palette (fiche § 4)
FACE, FACE_LT, FACE_SH = "#E6E4DD", "#FAFBF6", "#C9C2B4"
COLD, COLD_LT, COLD_SH = "#A5B2B9", "#BAC9D2", "#8B969A"        # le « reflet argenté » à l'ombre
CHANF, CHANF_LT, CHANF_SH = "#D4D9DC", "#EAECED", "#B9C7D3"     # chanfrein en lumière rasante
ROBE, ROBE_LT, ROBE_SH = "#E4DCCD", "#F5F2E9", "#CFBFA8"        # robe tondue au soleil
ROBE_MID, ROBE_MID_LT, ROBE_MID_SH = "#B19D81", "#C5B38F", "#826E53"
SKIN, SKIN_LT, SKIN_SH = "#C9B0AD", "#E4D5D0", "#927272"        # peau rose (ventre, nez)
NOSTRIL, NOSTRIL_DEEP = "#2A2224", "#020203"
LIPS, LIPS_LT, LIPS_SH = "#B9A9A6", "#CDBFBB", "#7A7578"
EAR, EAR_LT, EAR_SH = "#DEDFDF", "#F3F4F3", "#CBCCCD"
EAR_IN, EAR_IN_LT, EAR_IN_SH = "#A27064", "#B99489", "#885141"
LID, LID_LT, LID_SH = "#B4A1A0", "#D8CECC", "#8C7475"
LASH, LASH_LT = "#E8E2D2", "#FAFBF6"
IRIS, IRIS_LT, IRIS_RING, PUPIL = "#7A6E55", "#9C8F70", "#44362F", "#0D0C0B"
IRIS_GOLD = "#B49A5E"            # l'anneau doré autour de la pupille (§ 5, P15)
LEG, LEG_LT, LEG_SH = "#DCD6C8", "#F0EDE3", "#A39D73"
LEG_DIRT = "#8F806F"
UDDER, UDDER_LT, UDDER_SH, UDDER_DEEP = "#B8978A", "#D9C0B5", "#8E7265", "#6A5048"
HOOF, HOOF_LT, HOOF_SH = "#957054", "#B88D69", "#5A3F2C"
GUM, GUM_SH = "#C9898B", "#9C5C63"

doc = Doc()

# ================================================================== la tête : son repère
# Origine à la nuque (pivot-head, l'articulation de l'atlas, derrière la base
# de l'oreille) ; u vers le bout du nez, v vers la mâchoire.
POLL = (99.4, 84.4)
TH = math.radians(TETE_ANGLE)
HU = (math.cos(TH), -math.sin(TH))
HV = (-math.sin(TH), -math.cos(TH))


KU = L_TETE / 22.62     # le profil ci-dessous est dessiné pour une tête de 22,6 cm


def HL(u, v):
    u *= KU
    return (POLL[0] + u * HU[0] + v * HV[0], POLL[1] + u * HU[1] + v * HV[1])


def HLs(pts):
    return [HL(*p) for p in pts]


def hang(du, dv):
    """L'angle (monde, radians) d'une direction (du, dv) du repère de la tête."""
    du *= KU
    return math.atan2(du * HU[1] + dv * HV[1], du * HU[0] + dv * HV[0])


# Le profil (§ 5), relevé sur P15 (calé sur l'œil et le bout du nez) : une
# seule courbe convexe de la nuque au bout du nez, le front haut au-dessus de
# l'œil, puis les os du nez qui plongent en s'arrondissant (la flèche la plus
# forte devant les yeux) ; un gros mufle haut et rond ; la bouche basse ; une
# ganache profonde et un menton rond.
DORSAL = [(-1.6, 0.0), (-1.3, -1.15), (-0.6, -2.1), (0.4, -2.85), (1.9, -3.35), (3.6, -3.62), (5.4, -3.72),
          (7.2, -3.66), (9.0, -3.46), (10.8, -3.16), (12.6, -2.76), (14.4, -2.3), (16.2, -1.78), (17.8, -1.22),
          (19.3, -0.58), (20.6, 0.05), (21.6, 0.68), (22.35, 1.36), (22.85, 2.1), (23.08, 2.92), (23.08, 3.64),
          (22.92, 4.28), (22.7, 4.82)]
MOUTH = [(22.7, 4.82), (21.7, 4.98), (20.5, 5.13), (19.1, 5.32), (17.7, 5.52)]
NOTCH = [(17.7, 5.52), (17.1, 6.75), (16.3, 8.45)]
UNDER = [(16.3, 8.45), (14.2, 9.0), (11.8, 9.36), (9.3, 9.55), (6.8, 9.52), (4.8, 9.28), (3.1, 8.78),
         (1.7, 8.0), (0.6, 6.95), (-0.3, 5.55), (-0.85, 3.95), (-1.25, 2.25), (-1.52, 1.05), (-1.6, 0.0)]
EYE_L = (6.4, 0.6)                 # § 5 : au quart de la tête depuis la nuque, sous la ligne du front
EAR_L = (1.6, -1.25)               # § 5 : derrière et un peu au-dessus de l'œil, sur le côté du haut de la tête
EAR_FAR_L = (0.7, -2.0)
JAW_L = (17.0, 5.9)                # la charnière de la mâchoire, sous la joue, à hauteur de la bouche
LIP_PIVOT_L = (19.25, 3.9)         # l'arrière de la lèvre, sous le naseau
MUZZLE_L = (22.5, 5.3)             # ce qui touche l'herbe : le devant des lèvres

HEAD_OUT = HLs(DORSAL + MOUTH[1:] + NOTCH[1:] + UNDER[1:-1])
HEAD_POLY = Polygon(dense(HEAD_OUT, True, 6)).buffer(0)

# L'œil (§ 5) : grand, l'axe un peu oblique (le coin avant plus bas), la
# paupière du haut lourde.
E = HL(*EYE_L)
EYE_TILT = math.radians(-12.0)
EU = (math.cos(EYE_TILT), math.sin(EYE_TILT))
EV = (EU[1], -EU[0])                # vers la paupière du bas


EYE_SCALE = 1.12                    # l'œil, grand et bien ouvert (§ 5, P15)


def EL(a, b):
    a, b = a * EYE_SCALE, b * EYE_SCALE
    return (E[0] + a * EU[0] + b * EV[0], E[1] + a * EU[1] + b * EV[1])


ALMOND_L = [(-1.38, 0.05), (-1.08, -0.46), (-0.55, -0.73), (0.1, -0.78), (0.72, -0.63), (1.18, -0.28),
            (1.38, 0.16), (1.02, 0.46), (0.42, 0.62), (-0.25, 0.62), (-0.86, 0.46), (-1.2, 0.26)]
ALMOND = [EL(*p) for p in ALMOND_L]
IRIS_C = EL(-0.3, -0.04)            # un peu en arrière : au repos, le moteur avance le regard
IRIS_R = 1.22 * EYE_SCALE           # l'iris remplit presque l'amande (P15) : peu de globe sombre

# ================================================================== le corps
# Le dessus : croupe légèrement inclinée vers la queue, dos droit, garrot un
# peu saillant (§ 2.4).
BACK = [(3.0, 63.6), (6.0, 66.3), (10.0, 68.3), (15.0, 69.5), (20.0, 70.2), (24.5, H_CROUPE), (30.0, 70.1),
        (36.0, 69.6), (42.0, 69.3), (48.0, 69.2), (54.0, 69.3), (59.0, 69.6), (63.0, 70.15), (66.5, 70.8),
        (69.5, H_GARROT)]
WITHERS = BACK[-1]
# Le poitrail, de la base de la gorge au sternum.
CHEST = [(81.2, 54.6), (80.9, 51.2), (81.0, 49.2), (81.6, 46.6), (82.5, 44.2), (82.8, 42.0), (82.4, 40.0),
         (81.1, 38.3), (79.2, 36.9)]
# Le dessous : sternum à 35 cm, ventre de laitière qui descend derrière les
# coudes puis remonte au flanc ; le bas de la cuisse laineuse (la laine
# s'arrête au grasset), la culotte, la fesse.
UNDERSIDE = [(79.2, 36.9), (77.2, 35.8), (74.8, 35.05), (72.0, H_STERNUM), (68.0, 34.1), (64.0, 33.3),
             (60.0, 32.3), (56.0, 31.35), (52.0, 30.55), (48.0, 29.85), (44.0, H_VENTRE), (40.0, 29.45),
             (36.5, 30.0), (33.0, 31.2), (30.0, 33.3), (27.8, 36.0), (25.6, 38.6), (22.6, 40.1), (19.4, 41.0),
             (16.0, 41.9), (12.5, 42.6), (9.0, 43.3), (6.5, 43.9), (4.4, 44.9), (2.8, 46.5), (1.5, 48.8),
             (0.6, 51.8), (0.2, 55.0), (0.4, 58.0), (1.3, 60.8), (3.0, 63.6)]

# ================================================================== l'encolure et l'épaule
# L'encolure (variante A, sous le corps) tourne d'un bloc autour de
# NECK_PIVOT, dans l'épaule, sous le garrot : assez bas pour que l'axe du cou
# descende à 50° sous l'horizontale en broutant (§ 2.5 : 35 à 50°), le
# museau au sol. Elle ne plie pas : son coin du garrot s'en va devant
# l'épaule quand elle descend. Deux pièces se partagent donc le dessus du
# cou abaissé :
# - l'épaule (SHOULDER), au corps : du garrot, un large arrondi puis une
#   pente droite à -40°, qui s'arrête dans l'encolure (au repos, sous la
#   crête, du même dégradé : invisible) ;
# - le bord caché de l'encolure (NECK_TOP_HIDDEN), sous le corps au repos :
#   il part du garrot tangent à la crête (aucun coin), tourne, puis file
#   droit. Taillé pour qu'en broutant (l'encolure tournée de GRAZE_TURN par
#   rapport au corps) sa droite tombe sur celle de l'épaule : le dessus du
#   cou descend alors du garrot d'une seule ligne, puis s'arrondit vers la
#   nuque (P10 : le garrot arrondi, puis le cou qui plonge). Entre les deux
#   (endormi, en train de baisser la tête), il reste au plus un creux de
#   2 mm devant le garrot.
NECK_PIVOT = (72.5, 55.0)
GRAZE_TURN = 1.645      # rad (94°) : le calcul du moteur (reach_first), moins la bascule du corps
SHOULDER_ROUND = 12.0   # le rayon de l'arrondi du garrot
SHOULDER_SLOPE = -40.0  # degrés
SHOULDER_END = 84.0


def _arc(start, h0, h1, rho, left, step=4.0):
    """Un arc de rayon `rho` qui part de `start` au cap `h0` (degrés, monde)
    et tourne jusqu'au cap `h1` : vers la gauche ou vers la droite."""
    s = 1.0 if left else -1.0
    nrm = math.radians(h0 + 90.0 * s)
    c = (start[0] + rho * math.cos(nrm), start[1] + rho * math.sin(nrm))
    n = max(2, int(abs(h1 - h0) / step) + 1)
    pts = []
    for i in range(n + 1):
        a = math.radians(h0 + (h1 - h0) * i / n - 90.0 * s)
        pts.append((c[0] + rho * math.cos(a), c[1] + rho * math.sin(a)))
    return pts


def _ray(start, h, length, step=1.0):
    n = max(1, int(length / step))
    t = math.radians(h)
    return [(start[0] + math.cos(t) * length * i / n, start[1] + math.sin(t) * length * i / n)
            for i in range(1, n + 1)]


def _tourne(p, a):
    """La rotation du moteur (horaire à l'écran, `a` en radians) autour de la
    base de l'encolure."""
    vx, vy = p[0] - NECK_PIVOT[0], p[1] - NECK_PIVOT[1]
    co, si = math.cos(a), math.sin(a)
    return (NECK_PIVOT[0] + vx * co + vy * si, NECK_PIVOT[1] - vx * si + vy * co)


_BACK_DIR = math.degrees(math.atan2(BACK[-1][1] - BACK[-2][1], BACK[-1][0] - BACK[-2][0]))
_sh = _arc(WITHERS, _BACK_DIR, SHOULDER_SLOPE, SHOULDER_ROUND, left=False)
SHOULDER_TANGENT = _sh[-1]
_sh = [p for p in _sh + _ray(_sh[-1], SHOULDER_SLOPE, 16.0) if p[0] <= SHOULDER_END]
_sh += _arc(_sh[-1], SHOULDER_SLOPE, -100.0, 2.2, left=False)[1:]
SHOULDER = _sh + [(_sh[-1][0] - 0.3, _sh[-1][1] - 2.4), (83.4, 57.6), (82.4, 56.2)]
FRONT = SHOULDER + CHEST
BODY_PTS = BACK + FRONT[1:] + UNDERSIDE[1:-1]
BODY_POLY = Polygon(dense(BODY_PTS, True, 6)).buffer(0)
# Le corps sans l'épaule : son modelé (omoplate, épine...) s'y arrête ;
# l'épaule reçoit celui de l'encolure, qu'elle recouvre au repos.
BODY_CORE_POLY = Polygon(dense(BACK + [(72.6, 70.7), (75.8, 68.4), (78.2, 65.6), (80.2, 62.4), (81.2, 58.4)]
                               + CHEST + UNDERSIDE[1:-1], True, 6)).buffer(0)
SHOULDER_POLY = BODY_POLY.difference(BODY_CORE_POLY)

# La limite de la laine sur le flanc (§ 3.2, P1, P10) : la robe courte
# descend presque jusqu'à la ligne du ventre ; dessous, une bande étroite de
# peau nue, du coude au grasset. Un bord un peu irrégulier (WOOL_EDGE).
WOOL_LINE = [(69.4, 33.4), (68.2, 35.4), (66.4, 36.2), (63.0, 35.9), (59.0, 34.9), (55.0, 33.9), (51.0, 33.1),
             (47.0, 32.5), (43.0, 32.2), (39.0, 32.3), (35.5, 33.0), (32.3, 34.4), (29.6, 36.2), (27.6, 37.4),
             (26.2, 37.0)]


def _wool_edge():
    r = random.Random(5)
    out = []
    for i, p in enumerate(dense(WOOL_LINE, False, 4)):
        if 0 < i:
            n = 0.24 * math.sin(1.9 * p[0]) + 0.16 * math.sin(4.3 * p[0] + 1.1) + r.uniform(-0.14, 0.14)
            p = (p[0], p[1] + n)
        out.append(p)
    return out


WOOL_EDGE = _wool_edge()

# La mamelle (§ 2.9) : un sac arrondi entre les cuisses, attaché sous le
# ventre devant les grassets ; son arrière s'arrête devant la jambe proche.
UDDER_PTS = [(28.4, 36.6), (27.6, 33.6), (26.8, 30.8), (25.5, 28.0), (23.7, 25.6), (21.5, 24.1), (19.2, 23.5),
             (17.0, 23.8), (15.3, 25.0), (14.2, 27.0), (13.8, 29.6), (14.1, 32.4), (15.0, 35.0), (16.4, 37.6),
             (17.9, 39.8), (21.0, 41.2), (25.0, 40.4)]
UDDER_POLY = Polygon(dense(UDDER_PTS, True, 4)).buffer(0)
TEAT_BASE = (24.6, 26.6)
TEAT_DIR = norm((0.72, -0.69))     # vers l'avant et le bas, à environ 45° de la verticale

# ================================================================== l'encolure
NAPE = HL(-1.6, 0.0)
# La crête monte à 29° du garrot à la nuque (§ 2.5 : environ 30°) ; la gorge
# monte du poitrail sous la mâchoire.
CREST = [NAPE, (95.6, 84.3), (92.0, 82.9), (88.0, 80.95), (84.0, 78.6), (80.0, 75.95), (76.4, 73.5), (73.0, 71.85),
         WITHERS]
JAW_THROAT = HL(6.6, 9.3)
# La gorge : la base large (le poitrail bombe sous l'encolure), puis une
# ligne qui se creuse un peu sous la ganache (P1, la brebis du milieu).
THROAT = [(82.4, 43.6), (84.9, 45.6), (87.2, 48.1), (89.1, 50.9), (90.6, 53.9), (91.8, 57.1), (92.9, 60.5),
          (94.0, 63.9), (95.2, 67.0), (96.5, 69.8), JAW_THROAT]


def _neck_top_hidden():
    """Le bord caché qui devient le dessus du cou (voir plus haut) : un arc
    tangent au cercle du pivot au garrot, dont le rayon est cherché pour que
    sa droite, tournée de GRAZE_TURN, tombe 0,5 mm sous celle de l'épaule ;
    elle s'arrête avant l'arrondi du garrot, puis plonge vers le pivot."""
    v = (WITHERS[0] - NECK_PIVOT[0], WITHERS[1] - NECK_PIVOT[1])
    h0 = math.degrees(math.atan2(v[0], -v[1])) % 360.0
    h1 = SHOULDER_SLOPE + 180.0 + math.degrees(GRAZE_TURN)
    t = math.radians(SHOULDER_SLOPE)
    nrm = (-math.sin(t), math.cos(t))
    target = SHOULDER_TANGENT[0] * nrm[0] + SHOULDER_TANGENT[1] * nrm[1] - 0.05
    lo, hi = 0.5, 40.0
    for _ in range(60):
        rk = (lo + hi) / 2
        q = _tourne(_arc(WITHERS, h0, h1, rk, True)[-1], GRAZE_TURN)
        if q[0] * nrm[0] + q[1] * nrm[1] > target:
            hi = rk
        else:
            lo = rk
    arc = _arc(WITHERS, h0, h1, rk, True)
    q = _tourne(arc[-1], GRAZE_TURN)
    length = (q[0] - SHOULDER_TANGENT[0]) * math.cos(t) + (q[1] - SHOULDER_TANGENT[1]) * math.sin(t) - 0.6
    top = arc + _ray(arc[-1], h1, length)
    return top, _arc(top[-1], h1, h1 + 80.0, 3.0, True)[1:]


NECK_TOP_HIDDEN, _dive = _neck_top_hidden()
# Le fond de l'encolure, caché sous le corps : il comble l'avant du
# poitrail quand la tête se redresse ou descend.
NECK_LOW = [(61.8, 48.4), (64.2, 43.4), (67.8, 40.0), (72.5, 38.6), (77.5, 38.6), (80.8, 40.2), (82.4, 43.6)]
NECK_HIDDEN = NECK_TOP_HIDDEN + _dive + [lerp(_dive[-1], NECK_LOW[0], 0.5)] + NECK_LOW
# Sous la tête : l'encolure y déborde largement (la tête tourne à la nuque).
NECK_UNDER_HEAD = [JAW_THROAT, HL(7.4, 7.6), HL(7.6, 4.2), HL(6.8, 0.0), HL(4.5, -2.1), HL(1.0, -2.3),
                   HL(-1.0, -1.4)]
NECK_OUT = CREST + NECK_HIDDEN[1:] + THROAT[1:] + NECK_UNDER_HEAD[1:]
NECK_POLY = Polygon(dense(NECK_OUT, True, 6)).buffer(0)

# Le col de la laine sur le cou (§ 3.2) : un bord net à un travers de main
# derrière la base de l'oreille, qui descend presque à la verticale en
# s'inclinant un peu vers l'arrière jusqu'à 1 ou 2 cm derrière l'angle de la
# mâchoire, puis longe la gorge, nue sur le premier tiers du cou.
COLLAR = [(95.6, 84.3), (95.35, 82.0), (95.1, 79.6), (94.75, 77.4), (94.3, 75.2), (93.6, 73.2), (92.8, 71.0),
          (92.0, 68.6), (91.3, 66.0), (90.9, 63.0)]


# ================================================================== outils de dessin
def strokes_in(region, r, flow, length, style, rng_, skip=None, curl=0.18, nd=1):
    """Sème des poils (courts traits courbes) dans `region` : `flow(p)` donne
    leur direction, `style(p)` leur (couleur, largeur, opacité) ; ils sont
    regroupés par style dans des tracés communs (voir Doc.stroke)."""
    pts = poisson(region, r, rng_)
    n = 0
    for p in pts:
        if skip and skip(p):
            continue
        d = flow(p)
        L = rng_.uniform(*length)
        e = add(p, mul(d, L))
        nn = (-d[1], d[0])
        c = add(lerp(p, e, 0.5), mul(nn, rng_.uniform(-curl, curl) * L))
        st = style(p)
        if st is None:
            continue
        col, sw, so = st
        doc.stroke(qstroke(p, c, e, nd), col, sw, so)
        n += 1
    return n


def soft(shape_d, stops, p1, p2, gid):
    """Un modelé : la forme `shape_d` peinte d'un dégradé linéaire (transparent
    aux bouts, le plus souvent)."""
    doc.path(shape_d, fill=doc.lin(gid, p1, p2, stops))


def ellipse(c, rx, ry, angle=0.0, n=20):
    return [add(c, rot((rx * math.cos(2 * math.pi * k / n), ry * math.sin(2 * math.pi * k / n)), angle))
            for k in range(n)]


def glow(gid, c, rx, ry, color, alpha, angle=0.0, inner=0.0, clip=None):
    """Une ombre ou une lumière douce : une ellipse peinte d'un dégradé radial
    qui s'éteint au bord ; `clip` la garde dans une silhouette."""
    stops = [(0, color, alpha), (inner, color, alpha), (1, color, 0.0)] if inner else \
        [(0, color, alpha), (1, color, 0.0)]
    fill = doc.rad(gid, c, ry, stops, sx=rx / ry, angle=angle)
    if clip is None:
        doc.path(cr(ellipse(c, rx, ry, angle)), fill=fill)
    else:
        shape = Polygon(dense(ellipse(c, rx, ry, angle), True, 4)).buffer(0).intersection(clip)
        if not shape.is_empty:
            doc.path(geom_d(shape, simplify=0.02), fill=fill)


def band(pts, w0, w1, closed=False):
    """Un ruban le long d'une ligne de points, de largeur w0 à w1."""
    D = dense(pts, False, 6)
    left, right = [], []
    for i, p in enumerate(D):
        a = D[max(0, i - 1)]
        b = D[min(len(D) - 1, i + 1)]
        t = norm(sub(b, a))
        nn = (-t[1], t[0])
        w = w0 + (w1 - w0) * i / (len(D) - 1)
        left.append(add(p, mul(nn, w / 2)))
        right.append(add(p, mul(nn, -w / 2)))
    return left + right[::-1]


# La robe (§ 4) : au soleil sur le dos, l'ombre propre du flanc #B19D81 vers
# le bas, et vers le ventre à l'ombre (#7E715A) : une toison mate, pas un
# vernis clair.
ROBE_STOPS = [(0.0, "#F4F0E6"), (0.16, "#EEE8DB"), (0.32, ROBE), (0.5, "#D4C7B0"), (0.64, "#C4B496"),
              (0.78, "#B19D81"), (0.9, "#9E8B6F"), (1.0, "#8C7B61")]
ROBE_TOP, ROBE_BOTTOM = 72.0, 30.0
HEAD_STOPS = [(0, "#F7F7F2"), (0.2, "#F0EFEA"), (0.42, FACE), (0.62, "#D9D8D2"), (0.8, "#C3C8C6"),
              (1, "#AEB7BA")]


def robe_at(h):
    """La couleur du dégradé de la robe à la hauteur h (pour les poils)."""
    t = clamp((ROBE_TOP - h) / (ROBE_TOP - ROBE_BOTTOM))
    for (a, ca), (b, cb) in zip(ROBE_STOPS, ROBE_STOPS[1:]):
        if t <= b:
            return mix(ca, cb, (t - a) / (b - a))
    return ROBE_STOPS[-1][1]


def quant(c, levels=6):
    """Une couleur ramenée à quelques niveaux : les traits de même couleur se
    regroupent dans un seul tracé."""
    r, g, b = (int(c[i:i + 2], 16) for i in (1, 3, 5))
    q = lambda v: int(round(v / 255 * levels) * 255 / levels)
    return "#%02X%02X%02X" % (q(r), q(g), q(b))


def wool_patches(p):
    """Des plaques un peu plus denses ou plus lâches dans la toison, de
    quelques centimètres : de -1 à 1."""
    x, h = p
    return clamp(0.55 * math.sin(0.43 * x + 1.3) * math.sin(0.52 * h + 0.4) + 0.35 * math.sin(0.93 * x - 0.61 * h)
                 + 0.25 * math.sin(1.7 * x + 0.9 * h + 2.0), -1.0, 1.0)


def wool_fringe(line, rng_, poly, nd=3):
    """Le bord de la laine contre le ciel : de petites fibres qui dépassent
    de la silhouette `poly` le long de `line` (pas un trait net)."""
    pts = dense(line, False, nd)
    doc.begin_batch()
    for i, p in enumerate(pts):
        if rng_.random() < 0.25:
            continue
        a_, b_ = pts[max(0, i - 1)], pts[min(len(pts) - 1, i + 1)]
        t = norm(sub(b_, a_))
        o = (-t[1], t[0])
        if poly.contains(Point(add(p, mul(o, 0.4)))):
            o = (-o[0], -o[1])
        a = math.atan2(o[1], o[0]) + rng_.uniform(-0.7, 0.7)
        L = rng_.uniform(0.18, 0.5)
        q0 = add(p, mul(o, -0.15))
        q1 = add(p, (L * math.cos(a), L * math.sin(a)))
        base = robe_at(p[1])
        if rng_.random() < 0.7:
            doc.stroke(qstroke(q0, add(lerp(q0, q1, 0.5), (rng_.uniform(-0.1, 0.1), rng_.uniform(-0.1, 0.1))), q1),
                       quant(shade(base, 1.04), 10), 0.13, 0.75)
        else:
            doc.stroke(lstroke(q0, q1), quant(shade(base, 0.86), 10), 0.12, 0.55)
    doc.flush()


def wool_texture(region, rng_, dark=None, r=0.85):
    """La robe tondue de 1 à 2 cm (§ 3.3, P1, P15) : mate et serrée, faite de
    centaines de petites mèches frisées (un croissant d'ombre tourné au
    hasard, un éclat clair sur le dessus) et de creux minuscules entre elles,
    plus marqués dans les ombres. `dark(p)` : 0 en pleine lumière, 1 dans
    l'ombre."""
    pts = poisson(region, r, rng_)
    doc.begin_batch()
    for p in pts:
        base = robe_at(p[1])
        k = clamp((dark(p) if dark else 0.5) + 0.34 * wool_patches(p))
        # la mèche frisée : un arc de cercle ouvert, tourné au hasard
        rr = rng_.uniform(0.26, 0.42)
        a0 = rng_.uniform(0, 2 * math.pi)
        span = rng_.uniform(3.2, 4.4)
        a1 = a0 + span
        am = (a0 + a1) / 2
        p0 = add(p, (rr * math.cos(a0), rr * math.sin(a0)))
        p1 = add(p, (rr * math.cos(a1), rr * math.sin(a1)))
        c = add(p, (2 * rr * math.cos(am) - 0.5 * (p0[0] + p1[0] - 2 * p[0]),
                    2 * rr * math.sin(am) - 0.5 * (p0[1] + p1[1] - 2 * p[1])))
        so = 0.2 + round(k * 3) * 0.1
        doc.stroke(qstroke(p0, c, p1), quant(shade(base, 0.8), 10), 0.12, so)
        # l'éclat du dessus, qui prend la lumière d'en haut
        if rng_.random() < 0.75:
            q = add(p, (rng_.uniform(-0.12, 0.12), rr * 0.7))
            b = rng_.uniform(-0.5, 0.5)
            L = rng_.uniform(0.18, 0.32)
            doc.stroke(lstroke(add(q, (-L * math.cos(b), -L * math.sin(b))), add(q, (L * math.cos(b), L * math.sin(b)))),
                       quant(shade(base, 1.08), 10), 0.15, 0.18 + round((1 - k) * 2) * 0.05)
        # un creux minuscule entre deux mèches
        if rng_.random() < 0.25 + 0.5 * k:
            q = add(p, (rng_.uniform(-0.45, 0.45), rng_.uniform(-0.55, -0.35)))
            doc.stroke(lstroke(q, add(q, (0.03, -0.02))), quant(shade(base, 0.72), 10), 0.18,
                       0.2 + round(k * 2) * 0.1)
    doc.flush()
    return len(pts)


# ================================================================== pattes
def foreleg_outline(dx):
    """Un antérieur (monde), du haut caché sous le corps au sabot : avant-bras
    nu, carpe à peine renflé, canon sec, paturon court incliné, onglons."""
    o = lambda x, h: (x + dx, h)
    front = [o(75.2, 47.5), o(76.3, 41.0), o(76.4, 37.5), o(76.2, 35.0), o(75.8, 31.0), o(75.3, 27.0),
             o(74.85, 23.0), o(74.6, 20.2), o(74.78, 18.6), o(74.72, 17.2), o(74.45, 15.6), o(74.32, 13.0),
             o(74.25, 10.0), o(74.25, 7.6), o(74.5, 6.1), o(74.8, 5.0), o(75.5, 3.6), o(76.1, 2.7)]
    hoof = [o(76.1, 2.7), o(76.7, 1.6), o(77.35, 0.25), o(77.2, 0.0), o(72.15, 0.0), o(71.9, 0.6)]
    back = [o(71.9, 0.6), o(72.0, 1.6), o(72.3, 2.3), o(71.6, 3.3), o(71.0, 4.4), o(71.05, 5.6), o(71.4, 7.0),
            o(71.45, 10.0), o(71.4, 13.0), o(71.25, 15.8), o(70.95, 17.4), o(70.9, 18.8), o(71.3, 20.5),
            o(71.2, 23.5), o(70.8, 27.0), o(70.1, 30.5), o(69.0, 33.4), o(68.2, 35.6), o(68.0, 38.2),
            o(68.6, 42.0), o(69.8, 47.5)]
    return front, hoof, back


def hindleg_outline(dx):
    """Un postérieur : jambe nue du grasset au jarret, pointe du jarret sur
    l'aplomb de la fesse, angle du jarret de 152°, canon vertical."""
    o = lambda x, h: (x + dx, h)
    front = [o(17.0, 50.5), o(18.6, 45.0), o(17.5, 41.0), o(15.6, 37.5), o(13.6, 34.0), o(11.6, 30.4),
             o(9.6, 27.0), o(7.95, 24.4), o(7.0, 22.6), o(6.25, 20.6), o(5.95, 18.0), o(5.85, 13.0),
             o(5.85, 9.0), o(5.95, 6.6), o(6.15, 5.2), o(6.6, 4.0), o(7.3, 2.8)]
    hoof = [o(7.3, 2.8), o(7.95, 1.65), o(8.6, 0.25), o(8.45, 0.0), o(3.35, 0.0), o(3.1, 0.6)]
    back = [o(3.1, 0.6), o(3.2, 1.6), o(3.5, 2.3), o(2.85, 3.3), o(2.25, 4.4), o(2.35, 5.6), o(2.7, 7.2),
            o(2.75, 10.0), o(2.7, 14.0), o(2.6, 18.0), o(2.25, 20.6), o(1.7, 22.4), o(1.65, 23.8), o(2.05, 26.0),
            o(2.55, 29.5), o(3.05, 34.0), o(3.45, 39.0), o(3.6, 44.0), o(4.6, 50.5)]
    return front, hoof, back


# Les pattes : (nom, sorte, décalage, du fond ?) ; celles du fond un peu
# décalées pour qu'on les voie.
LEGS = [("leg-back-far", "back", 5.0, True), ("leg-front-far", "front", -5.0, True),
        ("leg-back-near", "back", 0.0, False), ("leg-front-near", "front", 0.0, False)]
HIP = (11.0, 50.0)       # l'articulation de la hanche (pivot du postérieur)


def leg_palette(far):
    if far:
        return dict(base="#B6B6AE", lt="#CBCBC3", sh="#8E908A", cold="#878F94", edge="#6C6F6C",
                    hair_lt="#D2D1CA", hair_sh="#7C827E", hoof="#8A7058", hoof_lt="#9F8468", hoof_sh="#5E4836",
                    dirt="#7A6E62")
    return dict(base=LEG, lt=LEG_LT, sh="#BAB8AD", cold="#A8B2B6", edge="#8C908F", hair_lt="#FBFAF6",
                hair_sh="#9DA5A8", hoof="#A88262", hoof_lt="#C49E7A", hoof_sh="#6E5039", dirt=LEG_DIRT)


def hoof_draw(hoof, back, far, pal, gid):
    """Les onglons : petits, pointus, serrés, corne pâle beige brun (§ 7),
    la fente entre les deux, un liseré de terre à la couronne."""
    toe_top, toe, sole_f, sole_b, heel = hoof[0], hoof[2], hoof[3], hoof[4], hoof[5]
    bulb = back[2]
    x0, x1 = sole_b[0], toe[0]
    g = doc.lin(gid, (x0, 1.4), (x1, 1.0), [(0, pal["hoof_sh"]), (0.3, pal["hoof"]), (0.72, pal["hoof_lt"]),
                                           (1, pal["hoof"])])
    wall = [toe_top, lerp(toe_top, toe, 0.5), toe, sole_f]
    doc.path(runs_path([wall, [sole_f, sole_b], [sole_b, heel, add(bulb, (0.1, -0.15)),
                                                 lerp(bulb, toe_top, 0.5), toe_top]]), fill=g)
    # l'ombre du talon et le dessous, plus sombres
    doc.path(runs_path([[sole_b, heel, add(bulb, (0.1, -0.15)), add(lerp(bulb, toe_top, 0.3), (0, -0.2))],
                        [add(lerp(bulb, toe_top, 0.3), (0, -0.2)), add(lerp(sole_b, sole_f, 0.3), (0, 0.4))],
                        [add(lerp(sole_b, sole_f, 0.3), (0, 0.4)), sole_b]]), fill=pal["hoof_sh"], fo=0.45)
    # la couronne : le poil retombe un peu sur la corne, un peu de terre
    cor = [add(bulb, (0.05, 0.05)), lerp(bulb, toe_top, 0.35), lerp(bulb, toe_top, 0.7), add(toe_top, (0.05, 0.0))]
    doc.path(cr(cor, closed=False), stroke=pal["dirt"], sw=0.34, so=0.55, cap="round")
    # la fente entre les onglons
    mid = lerp(sole_b, sole_f, 0.62)
    doc.path(cr([add(mid, (0.0, 0.02)), add(mid, (-0.25, 0.9)), add(lerp(bulb, toe_top, 0.55), (-0.1, -0.25))],
                closed=False), stroke=pal["hoof_sh"], sw=0.16, so=0.85, cap="round")
    # le reflet de la paroi et quelques stries de pousse
    doc.path(cr([add(toe_top, (-0.25, -0.2)), add(lerp(toe_top, toe, 0.5), (-0.3, 0.0)), add(toe, (-0.45, 0.25))],
                closed=False), stroke=shade(pal["hoof_lt"], 1.15), sw=0.24, so=0.65, cap="round")
    for k in range(3):
        t = 0.25 + 0.22 * k
        a = lerp(lerp(bulb, toe_top, 0.6), toe_top, t)
        b = lerp(lerp(sole_b, sole_f, 0.65), sole_f, t)
        doc.path(lstroke(add(a, (-0.1, -0.15)), add(b, (-0.15, 0.25)), 2), stroke=pal["hoof_sh"], sw=0.06, so=0.35,
                 cap="round")


def dewclaws(p, pal):
    """Les ergots, petits, derrière le paturon."""
    c = p
    doc.path(cr([add(c, (-0.38, 0.05)), add(c, (-0.05, 0.42)), add(c, (0.32, 0.12)), add(c, (0.05, -0.32))]),
             fill=pal["hoof_sh"])
    doc.path(cr([add(c, (-0.22, 0.18)), add(c, (0.0, 0.32)), add(c, (0.16, 0.12))], closed=False),
             stroke=pal["hoof_lt"], sw=0.08, so=0.6, cap="round")


def leg(name, kind, dx, far):
    """Une patte entière, dans son groupe : le moteur la recoupera."""
    pal = leg_palette(far)
    front, hoof, back = (foreleg_outline if kind == "front" else hindleg_outline)(dx)
    outline = front + hoof[1:-1] + back
    d = runs_path([front, [front[-1]] + hoof[1:], back + [front[0]]])
    pts = dense(outline, True, 6)
    LP = Polygon(pts).buffer(0)
    x0, y0, x1, y1 = LP.bounds
    doc.open(name)
    # le fond : poil ras blanc ; la lumière d'en haut à gauche éclaire
    # l'arrière de la patte, l'ombre froide est devant
    g = doc.lin(f"{name}-poil", (x0, 20), (x1, 20),
                [(0, pal["sh"]), (0.2, pal["lt"]), (0.5, pal["base"]), (0.82, pal["sh"]), (1, pal["cold"])])
    doc.path(d, fill=g)
    # le haut, sous le corps : l'ombre du ventre et de la cuisse
    top = 37.0 if kind == "front" else 42.5
    soft(d, [(0, "#6E675A" if not far else "#55524B", 0.55), (1, "#6E675A", 0.0)], (0, top + 2.0),
         (0, top - 4.5), f"{name}-ombre-haut")
    # le bas : un liseré de terre au-dessus des onglons (brebis propre, § 7)
    soft(d, [(0, pal["dirt"], 0.0), (1, pal["dirt"], 0.4 if not far else 0.3)], (0, 6.5), (0, 2.6),
         f"{name}-terre")
    if kind == "front":
        # le carpe : un léger renflement, l'os du carpe accessoire derrière
        glow(f"{name}-carpe", (72.85 + dx, H_CARPE + 0.9), 1.9, 2.4, pal["lt"], 0.5)
        doc.path(cr([(71.15 + dx, 19.6), (70.95 + dx, 18.6), (71.05 + dx, 17.5), (71.3 + dx, 16.6)], closed=False),
                 stroke=pal["edge"], sw=0.22, so=0.35, cap="round")
        # le tendon derrière le canon, et le boulet
        doc.path(cr([(71.9 + dx, 15.5), (71.95 + dx, 11.0), (71.9 + dx, 7.6)], closed=False),
                 stroke=pal["cold"], sw=0.35, so=0.45, cap="round")
        doc.path(cr([(74.0 + dx, 7.8), (74.25 + dx, 6.3), (74.55 + dx, 5.2)], closed=False),
                 stroke=pal["lt"], sw=0.3, so=0.55, cap="round")
        # l'avant-bras : le muscle extenseur devant, en lumière
        soft(cr([(75.6 + dx, 35.0), (75.0 + dx, 28.0), (74.5 + dx, 21.5), (73.4 + dx, 21.5), (73.4 + dx, 28.0),
                 (73.7 + dx, 35.0)]), [(0, pal["lt"], 0.0), (0.5, pal["lt"], 0.55), (1, pal["lt"], 0.0)],
             (73.2 + dx, 28), (75.8 + dx, 28), f"{name}-avant-bras")
        # la châtaigne et le pli du coude
        doc.path(cr([(69.6 + dx, 32.0), (70.4 + dx, 29.5), (70.9 + dx, 26.5)], closed=False),
                 stroke=pal["edge"], sw=0.2, so=0.28, cap="round")
        dew = (70.75 + dx, 4.35)
    else:
        # la pointe du jarret (le calcanéum), le tendon d'Achille, le devant du jarret
        doc.path(cr([(2.35 + dx, 30.0), (2.05 + dx, 26.0), (1.85 + dx, 23.6), (2.15 + dx, 22.2)], closed=False),
                 stroke=pal["edge"], sw=0.3, so=0.4, cap="round")
        glow(f"{name}-jarret", (3.6 + dx, 22.8), 2.0, 2.2, pal["lt"], 0.55)
        doc.path(cr([(6.9 + dx, 23.8), (6.3 + dx, 21.6), (6.05 + dx, 19.6)], closed=False),
                 stroke=pal["edge"], sw=0.24, so=0.3, cap="round")
        # la jambe : le gastrocnémien en lumière, le devant (le tibia) à l'ombre
        soft(cr([(4.0 + dx, 40.0), (4.6 + dx, 33.0), (4.2 + dx, 27.0), (6.6 + dx, 27.5), (9.5 + dx, 32.5),
                 (11.5 + dx, 38.5)]), [(0, pal["lt"], 0.5), (1, pal["lt"], 0.0)], (4 + dx, 34), (11 + dx, 32),
             f"{name}-jambe")
        doc.path(cr([(14.6 + dx, 37.0), (12.3 + dx, 33.0), (10.0 + dx, 29.0), (8.2 + dx, 25.6)], closed=False),
                 stroke=pal["cold"], sw=0.7, so=0.3, cap="round")
        doc.path(cr([(3.3 + dx, 15.5), (3.3 + dx, 11.0), (3.25 + dx, 7.6)], closed=False),
                 stroke=pal["cold"], sw=0.35, so=0.45, cap="round")
        doc.path(cr([(5.6 + dx, 7.8), (5.85 + dx, 6.4), (6.15 + dx, 5.3)], closed=False),
                 stroke=pal["lt"], sw=0.3, so=0.55, cap="round")
        dew = (2.1 + dx, 4.35)
    # le poil ras, en traits fins le long de la patte ; rangé par moitié
    # (au-dessus et au-dessous de l'articulation), pour que la coupe ne
    # touche pas les tracés regroupés
    joint_h = H_CARPE if kind == "front" else H_JARRET
    inner = LP.buffer(-0.25)
    halves = (inner.intersection(Polygon([(-50, joint_h + 2.7), (200, joint_h + 2.7), (200, 120), (-50, 120)])),
              inner.intersection(Polygon([(-50, joint_h - 2.7), (200, joint_h - 2.7), (200, -5), (-50, -5)])))

    def flow(p):
        if kind == "back" and p[1] > H_JARRET + 1:
            return norm((-0.5, -1.0))
        return norm((0.04, -1.0))

    def style(p):
        x_rel = (p[0] - x0) / max(1e-6, x1 - x0)
        r = rng.random()
        if p[1] < 6.5:
            return (pal["dirt"], 0.09, 0.4)
        if r < 0.5:
            return (pal["hair_lt"], 0.09, 0.5 if not far else 0.35)
        if x_rel > 0.7 or r < 0.75:
            return (pal["hair_sh"], 0.08, 0.35)
        return (pal["base"], 0.1, 0.45)
    for region in halves:
        doc.begin_batch()
        strokes_in(region, 0.55 if not far else 0.75, flow, (0.6, 1.1), style, rng, curl=0.08)
        doc.flush()
    # le contour, discret : pas de trait en haut (sous le corps)
    lim = 36.0 if kind == "front" else 41.0
    runs_ = []
    cur = []
    for p in pts + pts[:1]:
        if p[1] < lim:
            cur.append(p)
        else:
            if len(cur) > 1:
                runs_.append(cur)
            cur = []
    if len(cur) > 1:
        runs_.append(cur)
    for r_ in runs_:
        doc.path(poly(r_, False), stroke=pal["edge"], sw=0.26, so=0.6, join="round", cap="round")
    dewclaws(dew, pal)
    hoof_draw(hoof, back, far, pal, f"{name}-onglon")
    if kind == "front":
        doc.pivot(name, (71.5 + dx, 47.0))
    else:
        doc.pivot(name, (HIP[0] + dx, HIP[1]))
    doc.close()


# ================================================================== l'encolure (variante A)
#  le modelé d'un cylindre éclairé d'en haut : la crête en lumière, le côté
#  du cou (le muscle brachio-céphalique, de la tête à la pointe de l'épaule)
#  clair, le sillon de la jugulaire, le dessous de la gorge à l'ombre ; tout
#  s'éteint avant le bord du corps. (ligne, largeurs, id, dégradé)
NECK_RIBBONS = [
    ([(97.6, 84.6), (93.6, 82.6), (88.6, 79.9), (83.6, 77.0), (79.6, 74.4)], 3.0, 2.2, "cou-crete",
     (96.0, 83.6), (79.0, 74.0), [(0, "#FFFDF6", 0.5), (0.7, "#FFFDF6", 0.32), (1, "#FFFDF6", 0.0)]),
    ([(96.0, 76.2), (93.0, 71.8), (90.2, 67.2), (87.8, 62.2), (86.0, 57.0), (84.8, 52.0)], 4.0, 3.0,
     "cou-muscle", (96.0, 76.2), (84.8, 52.0), [(0, "#FBF7EE", 0.0), (0.2, "#FBF7EE", 0.4),
                                               (0.8, "#FBF7EE", 0.3), (1, "#FBF7EE", 0.0)]),
    ([(98.4, 71.4), (96.0, 67.6), (93.2, 63.6), (90.4, 59.4), (88.0, 55.0), (86.4, 50.8)], 2.8, 2.0,
     "cou-gorge", (98.4, 71.4), (86.4, 50.8), [(0, "#8E7D63", 0.12), (0.5, "#8E7D63", 0.3),
                                              (1, "#8E7D63", 0.0)]),
    ([(95.0, 70.6), (92.4, 66.6), (89.8, 62.0), (87.6, 57.4), (86.0, 52.8)], 1.2, 0.8, "cou-jugulaire",
     (95.0, 70.6), (86.0, 52.8), [(0, "#8E7D63", 0.0), (0.3, "#8E7D63", 0.2), (0.8, "#8E7D63", 0.16),
                                  (1, "#8E7D63", 0.0)]),
]


def neck_ribbons(clip, first):
    """Les rubans du modelé de l'encolure, gardés dans `clip` ; `first` : les
    dégradés sont définis ici (l'encolure), sinon repris (l'épaule)."""
    for pts, w0, w1, gid, p1, p2, stops in NECK_RIBBONS:
        geom = Polygon(band(pts, w0, w1)).buffer(0).intersection(clip)
        fill = doc.lin(gid, p1, p2, stops) if first else f"url(#{gid})"
        if not geom.is_empty:
            doc.path(geom_d(geom), fill=fill)


def neck():
    """L'encolure, sous le corps. Au repos, son dégradé est celui du corps
    (aucune jointure) ; la gorge et la nuque nues prennent celui de la tête.

    Elle tourne d'un bloc autour de sa base : de -17° (la tête haute) à +97°
    (pour brouter, le museau au sol devant les antérieurs). Sa partie cachée
    sous le corps (NECK_HIDDEN) devient le dessus du cou abaissé (voir
    SHOULDER)."""
    doc.open("neck")
    d = cr(NECK_OUT, nd=2)
    NP = NECK_POLY
    doc.path(d, fill="url(#robe)")
    global_light(d, "cou")
    # en broutant, le bord caché devient le dessus du cou : un peu de lumière
    # le long de lui, peinte seulement là où le corps la cache au repos
    inner = [add(p, (0.9, -1.1)) for p in NECK_TOP_HIDDEN[1:]]
    groove("cou-cache", inner[::2], 3.2, "#F6F2E8", 0.5, NP.intersection(BODY_POLY.buffer(-0.6)))
    neck_ribbons(NP, True)
    # le grain de la laine sur le cou, partie cachée comprise (elle sort en broutant)
    bare = bare_zone()
    wool_texture(NP.buffer(-0.3).difference(bare.buffer(0.4)), rng,
                 lambda p: 0.15 + 0.7 * clamp((ROBE_TOP - p[1]) / (ROBE_TOP - ROBE_BOTTOM)))
    # la peau nue : la nuque derrière l'oreille et la gorge (le « décolleté »,
    # § 3.2), au poil ras de la tête, blanc et brillant
    bd = geom_d(bare, simplify=0.03)
    doc.path(bd, fill="url(#tete)")
    doc.path(bd, fill=doc.lin("cou-nu-lumiere", (90.6, 75.6), (95.8, 67.8),
                              [(0, "#F7F6F2", 0.0), (0.4, "#F7F6F2", 0.55), (1, "#F7F6F2", 0.0)]))
    doc.path(bd, fill=doc.lin("cou-nu-ombre", (93.4, 71.6), (98.6, 68.0), [(0, COLD, 0.0), (1, COLD, 0.3)]))
    # le volume de la gorge : le dessous rond (la trachée) prend la lumière,
    # l'ombre froide sous la ganache et le long du bord du dessous
    glow("cou-nu-machoire", HL(6.0, 10.4), 3.2, 1.2, COLD_SH, 0.35, angle=hang(1.0, 0.05), clip=bare)
    groove("cou-nu-gorge", [(93.4, 63.6), (95.0, 66.7), (96.5, 69.9)], 1.6, COLD_SH, 0.26, bare)
    groove("cou-nu-trachee", [(92.3, 64.8), (93.9, 67.9), (95.3, 71.0)], 1.4, "#FFFFFF", 0.4, bare)
    doc.begin_batch()

    def flow(p):
        return norm((-0.55, -0.85))

    def style(p):
        r = rng.random()
        if r < 0.45:
            return ("#FFFFFF", 0.06, 0.34)
        if r < 0.8:
            return ("#A2ADB2", 0.055, 0.26)
        return ("#D9D9D3", 0.06, 0.3)
    strokes_in(bare.intersection(NP.buffer(-0.3)).difference(HEAD_POLY.buffer(0.2)), 0.4, flow,
               (0.4, 0.75), style, rng, curl=0.12)
    doc.flush()
    # le col : le bord net de la laine, en petites boucles qui débordent un
    # peu sur la peau nue
    edge = dense(COLLAR, False, 8)
    doc.begin_batch()
    for i, p in enumerate(edge):
        if i % 2:
            continue
        a = rng.uniform(0, 6.28)
        rr = rng.uniform(0.35, 0.6)
        q0 = add(p, (rr * math.cos(a) - 0.3, rr * math.sin(a)))
        q1 = add(p, (rr * math.cos(a + 2.4) - 0.3, rr * math.sin(a + 2.4)))
        c = add(p, (-0.55, -0.2))
        doc.stroke(qstroke(q0, c, q1), quant(shade(robe_at(p[1]), 0.84), 10), 0.16, 0.5)
        doc.stroke(qstroke(add(q0, (0.1, 0.25)), add(c, (0.05, 0.4)), add(q1, (0.1, 0.25))),
                   quant(shade(robe_at(p[1]), 1.35), 10), 0.12, 0.5)
    doc.flush()
    doc.path(cr(COLLAR, closed=False), stroke="#B9AC96", sw=0.5, so=0.28, cap="round")
    # le contour : la crête, la gorge, et le bord caché qui devient le dessus
    # du cou quand il broute
    # (la crête s'arrête 1,5 mm avant le garrot, où finit le trait du dos ; le
    # bord caché, rentré de 1,5 mm près du garrot, reste sous le dos au repos)
    w = WITHERS
    k = 0.15 / dist2(CREST[-2], w)
    doc.path(cr(CREST[:-1] + [lerp(w, CREST[-2], k)], closed=False), stroke="#8C8272", sw=0.3, so=0.4,
             cap="round")
    wool_fringe(CREST[1:], rng, NECK_POLY)
    top = dense(NECK_TOP_HIDDEN, False, 4)
    run, inner = 0.0, []
    for i, p in enumerate(top):
        if i:
            run += dist2(top[i - 1], p)
        a, b = top[max(0, i - 1)], top[min(len(top) - 1, i + 1)]
        t = norm((b[0] - a[0], b[1] - a[1]))
        inner.append(add(p, mul((-t[1], t[0]), 0.15 * max(0.0, 1.0 - run / 2.5))))   # vers l'intérieur
    doc.path(cr(inner, closed=False), stroke="#8C8272", sw=0.3, so=0.55, cap="round")
    doc.path(cr(THROAT[::-1], closed=False), stroke="#8C8272", sw=0.3, so=0.5, cap="round")
    doc.pivot("neck", NECK_PIVOT)
    doc.close()


def bare_zone():
    """La région nue de l'encolure : devant le col, jusqu'à la gorge ; elle
    continue sous la tête."""
    region = COLLAR + [(93.2, 63.6), (95.4, 67.2), (97.2, 70.6), (98.4, 72.6), JAW_THROAT, HL(8.0, 8.6),
                       HL(8.6, 4.6), HL(7.5, 0.0), HL(4.5, -2.1), HL(1.0, -2.3), HL(-1.0, -1.4),
                       add(NAPE, (0.15, 0.6)), (96.5, 85.8)]
    return Polygon(dense(region, True, 4)).buffer(0).intersection(NECK_POLY)


# ================================================================== le corps
def groove(gid, pts, width, color, alpha, clip):
    """Un sillon ou un relief doux le long d'une ligne : une suite de lueurs
    allongées qui s'éteignent de tous côtés (pas de bord dur)."""
    for k, (p, q) in enumerate(zip(pts, pts[1:])):
        c = lerp(p, q, 0.5)
        L = dist2(p, q)
        ang = math.atan2(q[1] - p[1], q[0] - p[0])
        glow(f"{gid}{k}", c, L * 0.7 + width * 0.5, width / 2, color, alpha, angle=ang, clip=clip)


def dist2(p, q):
    return math.hypot(q[0] - p[0], q[1] - p[1])


def global_light(d, suffix):
    """Les deux grands dégradés de la robe, peints sur le corps et sur
    l'encolure avec les mêmes définitions (au repos, aucune jointure) : le
    dessus en lumière, l'ombre propre sous le tonneau, plus forte vers
    l'avant."""
    doc.path(d, fill="url(#lumiere-dos)")
    doc.path(d, fill="url(#ombre-bas)")
    doc.path(d, fill="url(#ombre-avant)")
    _ = suffix


def body():
    """Le corps : la robe tondue d'une laitière sèche, qui suit les os
    (garrot, omoplate, coude, côtes, pointe de la hanche, cuisse plate), le
    ventre nu et rose, la mamelle. La lumière vient d'en haut à gauche."""
    doc.open("body")
    d = cr(BODY_PTS, nd=2)
    B = BODY_CORE_POLY
    doc.path(d, fill="url(#robe)")
    global_light(d, "corps")
    # --- l'épaule : l'omoplate, un plan large et éclairé, son épine, son bord
    # arrière dans l'ombre ; la pointe de l'épaule ; le triceps au-dessus du
    # coude et la sangle derrière
    # --- la ligne du dos : l'échine prend la lumière, du garrot à la croupe
    groove("c-dos", [(9.0, 67.2), (18.0, 68.7), (30.0, 68.5), (42.0, 67.9), (54.0, 67.9), (63.0, 68.6)], 2.4,
           "#FFFFFF", 0.3, B)
    glow("c-omoplate", (70.6, 58.6), 4.6, 10.6, "#FFFCF4", 0.5, angle=-0.42, clip=B)
    # l'épine de l'omoplate : une arête (sa lumière, l'ombre de la fosse derrière)
    groove("c-epine-ombre", [(66.8, 68.0), (69.0, 62.4), (71.4, 56.6), (74.0, 51.2)], 2.4, "#8E7D63", 0.12, B)
    groove("c-epine", [(68.0, 68.4), (70.2, 62.8), (72.6, 57.0), (75.2, 51.6)], 1.8, "#FFFFFF", 0.3, B)
    groove("c-omoplate-bord", [(64.2, 66.0), (65.2, 60.6), (67.2, 55.0), (69.6, 50.0)], 3.0,
           "#8E7D63", 0.22, B)
    # la pointe de l'épaule, une bosse d'os, et son ombre dessous
    glow("c-pointe-epaule", (79.2, 46.8), 2.6, 3.4, "#FBF7EE", 0.26, angle=-0.5, clip=B)
    glow("c-pointe-epaule-ombre", (77.6, 43.4), 2.6, 1.5, "#8E7D63", 0.22, angle=-0.3, clip=B)
    glow("c-triceps", (67.4, 44.4), 4.2, 5.6, "#FCF8EF", 0.3, angle=-0.25, clip=B)
    groove("c-sangle", [(62.4, 52.0), (62.2, 47.0), (62.8, 42.4), (64.6, 38.4), (66.6, 36.0)], 3.0, "#8E7D63",
           0.22, B)
    # le coude, l'os qui pointe vers l'arrière au bas de l'épaule
    glow("c-coude", (68.4, 37.6), 2.4, 1.8, "#FBF7EE", 0.28, clip=B)
    glow("c-coude-ombre", (66.2, 36.6), 1.8, 1.1, "#8E7D63", 0.26, clip=B)
    glow("c-poitrail", (79.8, 39.6), 4.0, 3.0, "#8E7D63", 0.3, clip=B)
    # --- les côtes : le tonneau éclairé à mi-flanc, quelques côtes à peine
    glow("c-cotes", (51.0, 55.0), 12.0, 5.0, "#FFFCF4", 0.3, angle=0.03, clip=B)
    for k, x in enumerate((57.5, 52.5, 47.5, 42.8)):
        groove(f"c-cote{k}-", [(x + 2.0, 58.5), (x + 0.6, 52.5), (x - 0.2, 47.0)], 1.6, "#9C8A6E", 0.13, B)
    # --- l'arrière : la pointe de la hanche et la pointe de la fesse (l'ischion),
    # deux bosses d'os sous la robe courte ; le creux du flanc devant la
    # hanche, le pli du grasset, la cuisse plate de la laitière, le sillon de
    # la fesse
    glow("c-hanche", (24.6, 64.4), 3.8, 2.6, "#FFFFFF", 0.36, angle=0.3, clip=B)
    glow("c-hanche-ombre", (27.0, 61.2), 4.0, 2.0, "#8E7D63", 0.3, angle=0.45, clip=B)
    glow("c-ischion", (5.2, 62.4), 2.3, 1.7, "#FFFFFF", 0.28, angle=0.4, clip=B)
    glow("c-ischion-ombre", (6.0, 59.6), 2.3, 1.2, "#8E7D63", 0.22, angle=0.2, clip=B)
    glow("c-flanc", (33.6, 56.0), 4.6, 6.6, "#8E7D63", 0.3, angle=0.2, clip=B)
    groove("c-pli-grasset", [(26.8, 57.0), (25.8, 52.0), (24.2, 47.6), (21.8, 43.6)], 3.0, "#8E7D63", 0.24, B)
    groove("c-pli-grasset-lumiere", [(28.2, 56.6), (27.2, 51.6), (25.6, 47.2), (23.2, 43.2)], 1.0, "#FFFCF4",
           0.25, B)
    glow("c-cuisse", (13.0, 56.0), 8.2, 7.4, "#FFFCF4", 0.32, angle=-0.3, clip=B)
    groove("c-sillon-fesse", [(5.4, 61.0), (4.4, 55.4), (4.2, 50.6), (5.0, 46.4)], 2.0, "#8E7D63", 0.24, B)
    glow("c-fesse", (1.8, 55.4), 1.8, 2.8, "#FFFFFF", 0.16, clip=B)
    groove("c-queue", [(5.6, 62.4), (4.0, 58.4), (3.0, 54.0), (2.7, 49.4)], 1.6, "#7E7466", 0.3, B)
    # le sillon devant l'épaule (le bord avant de l'omoplate), qui détache
    # l'encolure ; il s'éteint avant la jointure, invisible au repos
    groove("c-sillon-epaule", [(68.6, 64.6), (70.6, 60.0), (72.8, 55.4), (75.0, 51.4)], 2.4,
           "#8E7D63", 0.12, B)
    # --- la repousse inégale : de grandes plaques à peine plus claires ou plus
    # sombres (les passes de la tondeuse, un mois après), qui cassent l'aplat
    pr = random.Random(31)
    for k in range(30):
        c = (pr.uniform(3.0, 72.0), pr.uniform(38.0, 69.0))
        rx, ry, ang = pr.uniform(3.0, 6.5), pr.uniform(2.0, 4.2), pr.uniform(-0.7, 0.7)
        if not B.contains(Point(c)):
            continue
        if k % 2:
            glow(f"c-plaque{k}", c, rx, ry, "#8E7D63", pr.uniform(0.07, 0.12), angle=ang, clip=B)
        else:
            glow(f"c-plaque{k}", c, rx, ry, "#FFFDF6", pr.uniform(0.1, 0.16), angle=ang, clip=B)
    # --- l'épaule, devant le garrot : au repos elle couvre la base de
    # l'encolure, dont elle reprend le modelé
    neck_ribbons(SHOULDER_POLY, False)
    # --- le grain de la robe tondue : des mèches et des creux, plus marqués
    # dans les ombres (le bas du tonneau, derrière l'épaule, le creux du
    # flanc, le pli du grasset)
    wool = BODY_POLY.difference(Polygon(dense(WOOL_EDGE + [(26.0, 20.0), (70.0, 20.0)], True, 4)).buffer(0))

    def dark(p):
        t = clamp((ROBE_TOP - p[1]) / (ROBE_TOP - ROBE_BOTTOM))
        k = 0.1 + 0.75 * t
        for c, r, w in (((33.6, 56.0), 6.0, 0.3), ((65.0, 58.0), 5.0, 0.2), ((25.4, 50.0), 4.5, 0.2),
                        ((63.6, 44.0), 4.0, 0.15)):
            k += w * max(0.0, 1.0 - dist(p, c) / r)
        return k
    wool_texture(wool.intersection(BODY_POLY.buffer(-0.25)), rng, dark)
    # --- le ventre nu (§ 3.2) : une bande étroite de peau beige rosé, couverte
    # de poils fins, sous le bord un peu irrégulier de la laine
    belly = Polygon(dense([(70.2, 30.0), (70.0, 33.4)] + WOOL_EDGE[1:] + [(25.0, 33.0), (34.0, 26.0),
                                                                          (60.0, 28.0)], True, 4)).buffer(0)
    belly = belly.intersection(B)
    bd = geom_d(belly, simplify=0.03)
    doc.path(bd, fill=doc.lin("c-ventre", (0, 36.0), (0, 29.4),
                              [(0, "#DCCABE", 0.0), (0.25, "#D4BEB2", 0.85), (0.65, "#C6AC9F", 0.95),
                               (1, "#AA8F82", 1.0)]))
    doc.path(bd, fill=doc.lin("c-ventre-ombre", (0, 33.4), (0, 29.4), [(0, "#6B5044", 0.0), (1, "#6B5044", 0.32)]))
    # le bord de la laine : de petites mèches qui débordent sur la peau, plus
    # sombres dessous, quelques-unes claires
    doc.begin_batch()
    for i, p in enumerate(dense(WOOL_EDGE, False, 10)):
        if rng.random() < 0.15:
            continue
        q = add(p, (rng.uniform(-0.3, 0.3), rng.uniform(0.0, 0.35)))
        e = add(q, (rng.uniform(-0.7, -0.2), rng.uniform(-0.75, -0.3)))
        base = robe_at(p[1])
        doc.stroke(qstroke(q, add(lerp(q, e, 0.5), (0.15, 0.05)), e), quant(shade(base, 0.93), 10), 0.32, 0.42)
        if rng.random() < 0.5:
            doc.stroke(qstroke(add(q, (0.15, 0.2)), add(lerp(q, e, 0.5), (0.3, 0.2)), add(e, (0.15, 0.15))),
                       quant(shade(base, 1.05), 10), 0.2, 0.4)
    doc.flush()
    # les poils fins du ventre
    doc.begin_batch()

    def bflow(p):
        return norm((-1.0, -0.35))

    def bstyle(p):
        r = rng.random()
        if r < 0.5:
            return ("#F3E8E2", 0.07, 0.42)
        return ("#9C8478", 0.07, 0.28)
    strokes_in(belly.buffer(-0.3), 0.6, bflow, (0.5, 0.9), bstyle, rng, curl=0.15)
    doc.flush()
    # --- la mamelle
    if not VARIANTE:
        udder()
    # --- le contour, sauf le long de l'encolure (au repos, rien ne s'y voit) :
    # net le long de la peau nue du ventre, doux et frangé de fibres le long
    # de la laine (le dos, la fesse, le poitrail)
    i_skin = UNDERSIDE.index((25.6, 38.6))
    doc.path(cr(BACK, closed=False), stroke="#8C8272", sw=0.3, so=0.32, cap="round")
    doc.path(cr(UNDERSIDE[2:i_skin + 1], closed=False), stroke="#8C8272", sw=0.3, so=0.5, cap="round")
    doc.path(cr(UNDERSIDE[i_skin:], closed=False), stroke="#8C8272", sw=0.3, so=0.32, cap="round")
    doc.path(cr(CHEST[4:] + UNDERSIDE[1:3], closed=False), stroke="#8C8272", sw=0.3, so=0.32, cap="round")
    wool_fringe(BACK, rng, BODY_POLY)
    wool_fringe(UNDERSIDE[i_skin:], rng, BODY_POLY)
    wool_fringe(CHEST[4:] + UNDERSIDE[1:3], rng, BODY_POLY)
    doc.pivot("body", (44.0, H_VENTRE))
    doc.close()


def udder():
    """La mamelle d'une Lacaune en lactation (§ 2.9) : volumineuse, rose beige,
    un peu luisante, quelques veines, de fins poils blancs à l'attache ; un
    seul trayon visible, court, pointé vers l'avant et le bas."""
    thigh = BODY_POLY.intersection(Polygon([(-5, 30), (26.6, 30), (26.6, 80), (-5, 80)]))
    vis = UDDER_POLY.difference(thigh)
    d = geom_d(vis, simplify=0.01)
    doc.path(d, fill=doc.lin("mamelle", (26.0, 34.0), (16.0, 24.5),
                             [(0, "#E2C7BC"), (0.4, "#CFAEA2"), (0.8, "#B7958A"), (1, "#9C7C72")]))
    # le volume : une lumière douce sur l'avant, l'ombre dessous et derrière
    glow("mamelle-lumiere", (22.6, 30.6), 4.0, 3.4, "#F4E3DA", 0.6, angle=-0.4, clip=UDDER_POLY)
    glow("mamelle-reflet", (24.0, 29.4), 1.3, 0.7, "#FFF8F3", 0.75, angle=-0.9, clip=UDDER_POLY)
    doc.path(d, fill=doc.lin("mamelle-ombre", (0, 27.5), (0, 23.4), [(0, UDDER_DEEP, 0.0), (1, UDDER_DEEP, 0.28)]))
    # l'attache, ombrée sous le ventre
    doc.path(d, fill=doc.lin("mamelle-attache", (0, 39.0), (0, 32.5), [(0, "#6A5650", 0.5), (1, "#6A5650", 0.0)]))
    # le sillon entre les deux quartiers, à peine
    doc.path(cr([(19.0, 39.0), (18.6, 34.0), (18.4, 29.0), (18.8, 25.0)], closed=False), stroke=UDDER_SH, sw=0.5,
             so=0.2, cap="round")
    # quelques veines
    for pts in ([(26.4, 35.6), (25.6, 33.0), (25.0, 31.0), (24.0, 29.6)], [(22.4, 37.4), (21.8, 34.6), (22.2, 32.2)]):
        doc.path(cr(pts, closed=False), stroke="#A08A92", sw=0.14, so=0.25, cap="round")
    # le trayon (le gauche), court, qui sort du bas du bord avant
    tb = TEAT_BASE
    td = TEAT_DIR
    tn = (-td[1], td[0])
    L = 2.8
    tip = add(tb, mul(td, L))
    teat = [add(tb, mul(tn, 0.72)), add(add(tb, mul(td, L * 0.55)), mul(tn, 0.46)), add(tip, mul(tn, 0.32)),
            add(tip, mul(td, 0.26)), add(tip, mul(tn, -0.34)), add(add(tb, mul(td, L * 0.55)), mul(tn, -0.5)),
            add(add(tb, mul(td, -0.6)), mul(tn, -0.82))]
    doc.path(cr(teat), fill=doc.lin("trayon", add(tb, mul(tn, 1.0)), add(tb, mul(tn, -1.0)),
                                    [(0, "#E2C7BC"), (0.45, "#CBA99D"), (1, "#A68478")]))
    doc.path(cr([add(tb, mul(tn, 0.35)), add(add(tb, mul(td, L * 0.6)), mul(tn, 0.22)), add(tip, mul(td, 0.05))],
                closed=False), stroke="#F3E2DA", sw=0.22, so=0.6, cap="round")
    # les fins poils blancs de l'attache
    doc.begin_batch()
    for k in range(26):
        p = (rng.uniform(17.0, 28.0), rng.uniform(33.0, 37.5))
        if not UDDER_POLY.contains(Point(p)):
            continue
        e = add(p, (rng.uniform(-0.6, -0.2), rng.uniform(-0.7, -0.3)))
        doc.stroke(lstroke(p, e), "#F4EEE9", 0.07, 0.5)
    doc.flush()
    rim = [p for p in dense(UDDER_PTS[1:13], False, 8) if not thigh.contains(Point(p))]
    doc.path(poly(rim, False), stroke="#8A6C63", sw=0.2, so=0.4, cap="round")


# ================================================================== la queue
TAIL_C = [(3.6, 63.4), (1.9, 59.8), (0.7, 55.6), (0.1, 51.2), (0.0, 46.8), (0.15, 42.0), (0.45, 37.0),
          (0.75, 31.0), (1.0, QUEUE_BAS)]
TAIL_W = [5.0, 4.7, 4.3, 3.9, 3.6, 3.3, 3.0, 2.6, 2.1]


def tail_shape(i0, i1, cap_top=True):
    """Le contour d'un morceau de queue, du point i0 au point i1 de l'axe."""
    D = dense(TAIL_C, False, 6)
    W = []
    for k in range(len(D)):
        u = k / (len(D) - 1) * (len(TAIL_W) - 1)
        j = min(int(u), len(TAIL_W) - 2)
        f = u - j
        W.append(TAIL_W[j] * (1 - f) + TAIL_W[j + 1] * f)
    a, b = i0 * 6, min(len(D) - 1, i1 * 6)
    left, right = [], []
    for k in range(a, b + 1):
        p = D[k]
        t = norm(sub(D[min(k + 1, len(D) - 1)], D[max(k - 1, 0)]))
        n = (-t[1], t[0])
        left.append(add(p, mul(n, W[k] / 2)))
        right.append(add(p, mul(n, -W[k] / 2)))
    # le bout arrondi en bas, le haut arrondi autour de l'attache
    tip = D[b]
    t = norm(sub(D[b], D[b - 1]))
    bottom = [add(tip, mul(t, W[b] * 0.45))]
    top = []
    if cap_top:
        t0 = norm(sub(D[a + 1], D[a]))
        top = [add(D[a], mul(t0, -W[a] * 0.5))]
    return left + bottom + right[::-1] + top


def tail():
    """La queue naturelle, fine et ronde, jusqu'au jarret (§ 2.8) : laine
    courte sur le haut, poil ras dessous ; en trois morceaux qui ondulent.
    Un seul dégradé (de la laine au poil) pour les trois : au repos, les
    jointures ne se voient pas."""
    doc.open("tail")
    seg1 = tail_shape(0, 4)
    tail_piece(seg1, "q1")
    doc.open("tail-mid")
    seg2 = tail_shape(3, 6)
    tail_piece(seg2, "q2")
    doc.open("tail-tip")
    seg3 = tail_shape(5, 8)
    tail_piece(seg3, "q3")
    doc.pivot("tail-tip", TAIL_C[5])
    doc.close()
    doc.pivot("tail-mid", TAIL_C[3])
    doc.close()
    doc.pivot("tail", (3.3, 62.6))
    doc.close()


def tail_piece(seg, gid):
    P = Polygon(seg).buffer(0)
    d = cr(seg)
    doc.path(d, fill="url(#queue)")
    # ronde : la lumière sur l'arrière (à gauche), l'ombre contre la fesse
    doc.path(d, fill=doc.lin(gid + "-rond", (-2.6, 0), (3.6, 0), [(0, "#8E8576", 0.25), (0.3, "#FFFFFF", 0.35),
                                                                   (0.62, "#FFFFFF", 0.0), (1, "#7E7466", 0.4)]))
    wool_texture(P.buffer(-0.25).intersection(Polygon([(-9, 44), (12, 44), (12, 70), (-9, 70)])), rng,
                 lambda p: 0.35, r=0.7)
    doc.begin_batch()

    def flow(p):
        return norm((0.05, -1.0))

    def style(p):
        if p[1] > 46.0 + rng.uniform(0, 4):
            return None
        r = rng.random()
        if r < 0.5:
            return ("#FFFFFF", 0.07, 0.45)
        return ("#9EA6A8", 0.07, 0.35)
    strokes_in(P.buffer(-0.2), 0.42, flow, (0.6, 1.1), style, rng, curl=0.1)
    doc.flush()
    # les bords, en contour doux (pas en haut : la jointure)
    left = [p for p in dense(seg, True, 4) if p[1] < max(q[1] for q in seg) - 2.0]
    _ = left


# ================================================================== la tête
def head():
    doc.open("head")
    ear_far()
    jaw()
    doc.path(runs_path([HLs(DORSAL), HLs(MOUTH), HLs(NOTCH), HLs(UNDER)]), fill="url(#tete)")
    closed_eye()
    eyes()
    cover()
    face()
    ear_shadow()
    ear_near()
    doc.pivot("head", POLL)
    doc.pivot("muzzle", HL(*MUZZLE_L))
    doc.close()


def ear_frame(base, angle_deg):
    a = math.radians(angle_deg)
    d = (-math.cos(a), -math.sin(a))        # vers l'arrière, `angle_deg` sous l'horizontale
    n = (-d[1], d[0])                       # vers le bas de l'oreille
    return lambda u, v: add(base, add(mul(d, u), mul(n, v)))


EAR_UP = [(-0.9, -0.9), (0.6, -1.3), (2.2, -1.95), (3.9, -2.35), (5.6, -2.45), (7.3, -2.25), (8.7, -1.75),
          (9.8, -1.0), (10.5, -0.15), (10.65, 0.55)]
EAR_LO = [(10.65, 0.55), (10.3, 1.35), (9.4, 2.0), (8.0, 2.45), (6.3, 2.6), (4.5, 2.5), (2.7, 2.1), (1.1, 1.4),
          (-0.9, 0.9)]
# Le creux de l'oreille (la cuillère), qu'on voit d'en dessous : sous le pli
# du bord du haut, jusqu'au liseré blanc du bord du bas.
EAR_CUP = [(0.9, 0.45), (2.5, 0.3), (4.3, 0.18), (6.1, 0.15), (7.8, 0.28), (9.05, 0.55), (9.85, 0.85), (9.85, 1.25),
           (9.1, 1.7), (7.9, 2.08), (6.3, 2.22), (4.5, 2.12), (2.8, 1.78), (1.3, 1.2), (0.7, 0.85)]


def ear_draw(F, gid, far=False):
    """Une oreille longue et large, épaisse, au bout arrondi, portée à
    l'horizontale (§ 5) : dehors blanc au poil ras, et en dessous son creux
    rose en cuillère, ombré sous le pli du bord du haut, éclairé vers le bas,
    bordé d'un liseré blanc (l'épaisseur de l'oreille)."""
    k = 0.74 if far else 1.0
    out = [F(*p) for p in EAR_UP + EAR_LO[1:-1]]
    g = doc.lin(gid, F(4.0, -2.4), F(4.0, 2.6), [(0, shade(EAR_LT, k)), (0.4, shade(EAR, k)),
                                                (1, shade("#B9BDBE", k * 0.95))])
    doc.path(cr(out), fill=g)
    # le creux rose : une surface, sombre sous le pli, claire vers le bas
    cup = [F(*p) for p in EAR_CUP]
    gi = doc.lin(gid + "-in", F(5.0, 0.15), F(5.0, 2.2),
                 [(0, shade("#8E5C55", k)), (0.3, shade("#AE7F76", k)), (0.7, shade("#C99D93", k)),
                  (1, shade("#D9B6AC", k))])
    doc.path(cr(cup), fill=gi)
    cup_poly = Polygon(cup).buffer(0)
    if not far:
        # le creux luit un peu au milieu, l'ombre du pli dessus
        ea = sub(F(1, 0), F(0, 0))
        glow(gid + "-luisant", F(6.0, 1.55), 2.6, 0.38, "#F2D9D2", 0.45, angle=math.atan2(ea[1], ea[0]),
             clip=cup_poly)
        doc.path(cr([F(1.0, 0.5), F(2.6, 0.36), F(4.4, 0.25), F(6.2, 0.22), F(7.8, 0.35), F(9.1, 0.62),
                     F(9.75, 0.9)], closed=False), stroke=shade("#6E4740", k), sw=0.22, so=0.4, cap="round")
        # la nervure et les poils fins, blancs, de l'intérieur
        doc.path(cr([F(1.8, 1.15), F(4.0, 1.3), F(6.6, 1.28), F(8.6, 1.05)], closed=False),
                 stroke="#9A6A62", sw=0.14, so=0.4, cap="round")
        doc.begin_batch()
        for i in range(34):
            u = rng.uniform(1.4, 9.2)
            v = rng.uniform(0.4, 1.9) * (1 - (u - 1.4) / 16)
            p = F(u, v)
            if not cup_poly.contains(Point(p)):
                continue
            e = F(u + rng.uniform(0.4, 0.8), v + rng.uniform(0.1, 0.35))
            doc.stroke(lstroke(p, e), "#F1E4DF", 0.06, 0.55)
        doc.flush()
    # le bord du haut, éclairé, et le poil ras du dehors
    doc.path(cr([F(*p) for p in EAR_UP[1:]], closed=False), stroke=shade(EAR_LT, 1.05 * k), sw=0.24, so=0.6,
             cap="round")
    doc.path(cr([F(*p) for p in EAR_UP[4:] + EAR_LO[1:-1]], closed=False), stroke=shade("#8A9295", k), sw=0.14,
             so=0.45, cap="round")
    if not far:
        doc.begin_batch()
        for i in range(70):
            u = rng.uniform(0.0, 9.6)
            vmax = 2.0 * (1 - u / 13.0)
            v = rng.uniform(-vmax, -0.6)
            p = F(u, v)
            e = F(u + rng.uniform(0.45, 0.8), v + rng.uniform(-0.05, 0.08))
            col = "#FFFFFF" if rng.random() < 0.55 else "#AEB6B9"
            doc.stroke(lstroke(p, e), col, 0.05, 0.35)
        doc.flush()
    # la base de l'oreille, roulée en cornet : elle se fond dans la tête
    doc.path(cr([F(-0.9, -0.85), F(0.4, -1.1), F(1.1, 0.0), F(0.4, 1.0), F(-0.9, 0.85), F(-1.2, 0.0)]),
             fill=doc.lin(gid + "-base", F(-0.9, 0.0), F(1.4, 0.0), [(0, shade("#E9E8E3", k), 1.0),
                                                                     (1, shade("#E9E8E3", k), 0.0)]))
    doc.path(cr([F(1.0, 1.15), F(1.7, 0.6), F(2.4, 0.5)], closed=False), stroke=shade(EAR_IN_SH, k), sw=0.2,
             so=0.35, cap="round")


def ear_shadow():
    """L'ombre douce de l'oreille sur la nuque et le haut de la joue."""
    eb = HL(*EAR_L)
    F = ear_frame(eb, 9.0)
    pts = [F(-0.6, 0.7), F(2.0, 2.4), F(5.0, 3.2), F(7.4, 2.9), F(5.0, 1.6), F(2.0, 0.7)]
    geom = Polygon(dense(pts, True, 6)).buffer(0).intersection(HEAD_POLY.union(NECK_POLY))
    doc.path(geom_d(geom), fill=doc.lin("oreille-ombre", F(3.0, 0.8), F(3.0, 3.0),
                                        [(0, COLD_SH, 0.35), (1, COLD_SH, 0.0)]))


def ear_far():
    doc.open("ear-far")
    eb = HL(*EAR_FAR_L)
    F = ear_frame(eb, 2.0)
    ear_draw(lambda u, v: F(u * 0.86, v * 0.72), "oreille-loin", far=True)
    doc.pivot("ear-far", eb)
    doc.close()


def ear_near():
    doc.open("ear-near")
    eb = HL(*EAR_L)
    F = ear_frame(eb, 9.0)
    ear_draw(F, "oreille")
    doc.pivot("ear-near", eb)
    doc.close()


def jaw():
    """La mâchoire du bas : le menton, la lèvre inférieure ; cachés au repos,
    l'intérieur de la bouche, la langue, les incisives du bas, la joue qui
    s'étire."""
    doc.open("jaw")
    # la joue qui s'étire derrière la commissure
    doc.path(cr(HLs([(14.6, 5.9), (16.8, 5.1), (18.6, 5.0), (18.9, 6.1), (17.6, 7.6), (15.3, 8.4)])),
             fill=doc.lin("joue-etiree", HL(16.5, 5.0), HL(16.5, 8.4), [(0, "#D7D6D0"), (1, "#B4BCBD")]))
    # l'intérieur de la bouche et la langue
    doc.path(cr(HLs([(17.9, 5.3), (18.6, 4.3), (20.2, 3.75), (21.8, 3.6), (22.45, 4.1), (22.25, 4.75), (21.4, 4.95),
                     (20.3, 5.08), (19.1, 5.25)])), fill="#3E2428")
    doc.path(cr(HLs([(18.5, 5.1), (19.6, 4.67), (21.0, 4.45), (21.7, 4.55), (20.8, 4.8), (19.3, 5.05)])),
             fill="#B47A7E")
    doc.path(cr(HLs([(19.2, 4.85), (20.4, 4.6), (21.3, 4.58)]), closed=False), stroke="#D9A0A2", sw=0.12,
             so=0.7, cap="round")
    # les incisives du bas (en haut, le mouton n'a qu'un bourrelet)
    doc.path(cr(HLs([(21.2, 4.62), (21.95, 4.5), (22.1, 4.78), (21.35, 4.88)])), fill="#E9E3D5")
    # le menton, rond et profond (P15), et la lèvre inférieure
    chin = HLs([(17.7, 5.52), (19.1, 5.32), (20.5, 5.13), (21.7, 4.98), (22.6, 4.86), (22.85, 5.3), (22.75, 6.0),
                (22.4, 6.62), (21.85, 7.18), (21.05, 7.7), (19.85, 8.15), (18.4, 8.38), (16.3, 8.5), (14.6, 8.9),
                (13.2, 9.1), (13.2, 7.3), (15.3, 6.1)])
    doc.path(cr(chin), fill=doc.lin("menton", HL(19.5, 5.0), HL(19.5, 8.4),
                                    [(0, "#E4DED9"), (0.4, "#D6D3CD"), (1, "#A7B0B3")]))
    # la lèvre inférieure, rose gris, et le renflement du menton
    doc.path(cr(HLs([(17.8, 5.55), (19.6, 5.28), (21.2, 5.06), (22.55, 4.88), (22.8, 5.35), (21.9, 5.68),
                     (20.4, 5.84), (18.9, 5.88)])),
             fill=doc.lin("levre-bas", HL(20.0, 5.0), HL(20.0, 5.9), [(0, LIPS), (1, mix(LIPS, "#D3D1CB", 0.6))]))
    cp = Polygon(chin).buffer(0)
    glow("menton-bosse", HL(21.1, 6.7), 1.4, 1.0, "#F6F4F0", 0.6, angle=-TH, clip=cp)
    glow("menton-ombre", HL(19.6, 7.9), 2.6, 0.7, COLD_SH, 0.3, angle=-TH + 0.1, clip=cp)
    doc.begin_batch()
    for i in range(36):
        u = rng.uniform(15.0, 22.0)
        v = rng.uniform(6.2, 8.2)
        p = HL(u, v)
        if not HEAD_POLY.buffer(2.0).contains(Point(p)):
            continue
        e = HL(u - rng.uniform(0.4, 0.7), v + rng.uniform(0.0, 0.2))
        doc.stroke(lstroke(p, e), "#FFFFFF" if rng.random() < 0.5 else "#9FA8AB", 0.07, 0.45)
    doc.flush()
    doc.pivot("jaw", HL(*JAW_L))
    doc.close()


def closed_eye():
    """L'œil fermé, sous le groupe des yeux : on le voit quand il cligne."""
    lid = [EL(*p) for p in [(-1.42, 0.05), (-0.8, 0.12), (0.0, 0.16), (0.8, 0.14), (1.42, 0.16)]]
    doc.path(cr(ALMOND), fill=doc.lin("paupiere", EL(0, -0.8), EL(0, 0.7), [(0, "#ECE9E4"), (0.6, "#DAD6D1"),
                                                                             (1, "#C9C6C2")]))
    doc.path(cr(lid, closed=False), stroke="#7E6A68", sw=0.16, so=0.85, cap="round")
    for k in range(9):
        t = (k + 0.5) / 9
        p = EL(-1.2 + 2.4 * t, 0.14 + 0.03 * math.sin(math.pi * t))
        doc.path(qstroke(p, add(p, rot((0.2, -0.25), EYE_TILT)), add(p, rot((0.32, -0.45), EYE_TILT)), 2),
                 stroke=LASH_LT, sw=0.055, so=0.85, cap="round")


def eyes():
    """L'œil (§ 5) : grand, clair ; iris gris ambré pâle avec un anneau doré
    autour de la pupille en fente horizontale ; cils blancs, paupières
    bordées de rose, pas d'ombre noire autour."""
    doc.open("eyes")
    # le globe, brun chaud (le blanc de l'œil d'un mouton ne se voit presque pas)
    doc.path(cr(ALMOND), fill=doc.rad("globe", EL(-0.2, -0.1), 1.7, [(0, "#7A6A54"), (0.7, "#5C4E3E"),
                                                                     (1, "#3E342A")]))
    doc.open("pupils")
    ic = IRIS_C
    doc.path(cr(ellipse(ic, IRIS_R, IRIS_R, 0, 18)),
             fill=doc.rad("iris", add(ic, (0.0, 0.05)), IRIS_R,
                          [(0, "#4E4434"), (0.22, IRIS_GOLD), (0.4, "#C2B793"), (0.66, "#ABA07E"),
                           (0.86, "#857A5D"), (1, IRIS_RING)], f=add(ic, (-0.12, 0.18))))
    for k in range(22):
        a0 = 2 * math.pi * k / 22 + 0.15
        doc.path(lstroke(add(ic, (0.56 * math.cos(a0), 0.56 * math.sin(a0))),
                         add(ic, (1.23 * math.cos(a0 + 0.08), 1.23 * math.sin(a0 + 0.08))), 2),
                 stroke="#7A6F57" if k % 2 else "#D2C8A6", sw=0.045, so=0.42, cap="round")
    pw, ph = 1.56, 0.43
    pc = add(ic, (0.0, 0.08))
    doc.path(runs_path([[add(pc, (-pw / 2 + ph / 2, ph / 2)), add(pc, (pw / 2 - ph / 2, ph / 2))],
                        [add(pc, (pw / 2 - ph / 2, ph / 2)), add(pc, (pw / 2 - 0.02, 0.0)),
                         add(pc, (pw / 2 - ph / 2, -ph / 2))],
                        [add(pc, (pw / 2 - ph / 2, -ph / 2)), add(pc, (-pw / 2 + ph / 2, -ph / 2))],
                        [add(pc, (-pw / 2 + ph / 2, -ph / 2)), add(pc, (-pw / 2 + 0.02, 0.0)),
                         add(pc, (-pw / 2 + ph / 2, ph / 2))]]), fill=PUPIL)
    # les grains de l'iris (granula iridica), sur le bord du haut de la pupille
    for gx, gs in ((-0.34, 0.06), (-0.08, 0.08), (0.18, 0.075), (0.42, 0.05)):
        p = add(pc, (gx, ph / 2 - 0.02))
        doc.path(cr([add(p, (-gs, 0)), add(p, (0, gs * 0.9)), add(p, (gs, 0))]), fill="#2A231B")
    doc.pivot("pupils", ic)
    doc.close()
    # les reflets sur la cornée (la lumière d'en haut à gauche)
    doc.path(cr(ellipse(EL(-0.62, -0.34), 0.2, 0.13, EYE_TILT, 10)), fill="#FFFFFF", fo=0.8)
    doc.path(cr(ellipse(EL(0.5, 0.3), 0.1, 0.07, EYE_TILT, 8)), fill="#FFFFFF", fo=0.4)
    # le bord de la paupière du haut et ses longs cils blancs, qui pendent
    # un peu sur l'œil (P4, P15) : ils clignent avec lui
    up = [EL(*p) for p in ALMOND_L[:7]]
    doc.path(cr(up, closed=False), stroke="#6E5A58", sw=0.16, so=0.8, cap="round")
    # une frange serrée (P15) : l'ombre qu'elle jette sur l'œil, puis les cils,
    # fins et inégaux, quelques-uns plus gris derrière
    glow("cils-ombre", EL(0.1, -0.42), 1.25, 0.3, "#1C1612", 0.35, angle=EYE_TILT, clip=Polygon(ALMOND))
    lr = random.Random(77)
    n = 24
    for k in range(n):
        t = (k + lr.uniform(0.15, 0.85)) / n
        a = -1.2 + 2.45 * t
        b = -0.66 - 0.12 * math.sin(math.pi * t) + 0.22 * t * t
        L = 0.34 + 0.32 * math.sin(math.pi * (0.25 + 0.6 * t)) + lr.uniform(-0.1, 0.1)
        p = EL(a, b)
        c = EL(a + 0.22 * L, b - 0.18 * L)
        tip = EL(a + 0.42 * L, b + 0.62 * L)
        col, w = ((LASH_LT, 0.042) if k % 3 else (LASH, 0.046)) if k % 5 else ("#B8B2A4", 0.04)
        doc.path(qstroke(p, c, tip, 2), stroke=col, sw=w, so=0.85, cap="round")
    doc.pivot("eyes", E)
    doc.close()


def cover():
    """Le pourtour de l'œil, immobile, peint du dégradé même de la tête : il
    cache l'iris quand le regard le pousse hors de l'amande."""
    outer = Polygon(ellipse(add(E, (-0.4, 0.0)), 4.4, 3.6, EYE_TILT, 28)).intersection(HEAD_POLY.buffer(-0.1))
    hole = Polygon(ALMOND).buffer(0)
    ring = outer.difference(hole)
    doc.path(geom_d(ring, simplify=0.01), fill="url(#tete)", rule="evenodd")


def face():
    """Le modelé et le poil de la face, le mufle, le naseau, la bouche, et
    l'habillage de l'œil (paupières roses, arcade, pli, larmier)."""
    hd = runs_path([HLs(DORSAL), HLs(MOUTH), HLs(NOTCH), HLs(UNDER)])
    eye_keep = Polygon(ALMOND).buffer(0.25)
    # --- le modelé (la lumière d'en haut à gauche) : front et chanfrein
    # éclairés, joue modelée par le masséter, l'arête de la face, l'ombre
    # froide sous la mâchoire (le reflet argenté)
    HP = HEAD_POLY
    glow("f-front", HL(4.8, -2.0), 4.6, 1.7, "#FFFFFF", 0.6, angle=hang(1.0, 0.0), clip=HP)
    # la bosse du nez, devant les yeux : elle prend la lumière ; l'ombre
    # douce sous elle, sur le côté du chanfrein
    glow("f-bosse", HL(12.0, -2.2), 4.4, 1.15, "#FFFFFF", 0.75, angle=hang(1.0, 0.2), clip=HP)
    glow("f-chanfrein", HL(17.6, -0.6), 3.4, 0.9, CHANF_LT, 0.55, angle=hang(1.0, 0.36), clip=HP)
    glow("f-chanfrein-ombre", HL(14.0, 0.4), 5.4, 1.0, COLD_SH, 0.2, angle=hang(1.0, 0.25), clip=HP)
    glow("f-joue", HL(8.8, 5.9), 5.0, 3.0, "#FFFFFF", 0.4, angle=hang(1.0, 0.1), clip=HP)
    soft(hd, [(0, COLD, 0.0), (0.5, COLD, 0.0), (1, COLD, 0.5)], HL(9.0, 2.6), HL(9.0, 9.6), "f-froid")
    # l'arête de la joue (la crête faciale) : une arête claire sous l'œil, qui
    # file vers le museau, l'ombre de la joue dessous
    groove("f-crete", HLs([(12.4, 3.2), (10.2, 3.0), (8.0, 3.0), (5.8, 3.3), (4.0, 3.75)]), 0.9, "#FFFFFF", 0.3, HP)
    groove("f-crete-ombre", HLs([(12.4, 3.9), (10.2, 3.75), (8.0, 3.8), (5.8, 4.15), (4.2, 4.6)]), 1.2, COLD_SH,
           0.32, HP)
    # le bord arrière de la ganache, à peine (une ombre douce, sans trait)
    doc.path(cr(band(HLs([(3.4, 9.0), (1.9, 8.1), (0.9, 6.5), (0.4, 4.4)]), 1.3, 0.7)),
             fill=doc.lin("f-ganache", HL(3.4, 9.0), HL(0.4, 4.2), [(0, COLD_SH, 0.32), (1, COLD_SH, 0.0)]))
    # l'arcade au-dessus de l'œil (l'os, éclairé) et le creux sous elle
    glow("f-arcade", EL(-0.1, -1.6), 2.0, 0.6, "#FFFFFF", 0.75, angle=EYE_TILT, clip=HP)
    glow("f-orbite", EL(0.1, -1.0), 1.8, 0.45, COLD_SH, 0.3, angle=EYE_TILT, clip=HP)
    # le pli de peau qui descend en oblique derrière l'œil (P15)
    groove("f-pli", [EL(-1.6, 0.45), EL(-2.3, 1.05), EL(-2.9, 1.7)], 0.45, COLD_SH, 0.26, HP)
    doc.path(cr([EL(-1.6, 0.68), EL(-2.3, 1.28), EL(-2.8, 1.9)], closed=False), stroke="#FFFFFF", sw=0.14, so=0.35,
             cap="round")
    # le creux devant l'œil (la fosse du larmier) et sa fente sombre, qui
    # descend en oblique vers le museau (P15)
    glow("f-creux-oeil", EL(2.7, 1.25), 1.7, 1.0, COLD_SH, 0.4, angle=EYE_TILT - 0.7, clip=HP)
    doc.path(cr([EL(1.7, 0.55), EL(2.2, 1.05), EL(2.6, 1.7), EL(2.85, 2.25)], closed=False), stroke="#7A7472",
             sw=0.18, so=0.55, cap="round")
    doc.path(cr([EL(1.95, 0.45), EL(2.45, 0.95), EL(2.85, 1.6)], closed=False), stroke="#FFFFFF", sw=0.12, so=0.4,
             cap="round")
    # --- le poil ras de la face, couché vers l'arrière ; rangé à part autour
    # de la lèvre (que le moteur détache)
    lip_area = Polygon(LIP_POLY).buffer(0)
    region = HEAD_POLY.buffer(-0.2).difference(eye_keep)

    def flow(p):
        q = sub(p, POLL)
        u = q[0] * HU[0] + q[1] * HU[1]
        v = q[0] * HV[0] + q[1] * HV[1]
        dl = (-1.0, 0.12 + 0.05 * v)
        return norm(add(mul(HU, dl[0]), mul(HV, dl[1])))

    def style(p):
        q = sub(p, POLL)
        v = q[0] * HV[0] + q[1] * HV[1]
        r = rng.random()
        if v < 1.5:
            return ("#FFFFFF", 0.055, 0.32) if r < 0.55 else ("#BFC6C9", 0.05, 0.2)
        if v < 5.0:
            return ("#FFFFFF", 0.055, 0.26) if r < 0.4 else ("#A7B1B6", 0.05, 0.2)
        return ("#F2F2EE", 0.055, 0.2) if r < 0.3 else ("#8F9AA0", 0.05, 0.22)
    doc.begin_batch()
    strokes_in(region.difference(lip_area.buffer(0.4)), 0.36, flow, (0.3, 0.6), style, rng, curl=0.1)
    doc.flush()
    doc.begin_batch()
    strokes_in(lip_area.buffer(-0.35), 0.4, flow, (0.35, 0.6), style, rng, curl=0.1)
    doc.flush()
    # --- le mufle : gros, haut et charnu, rose gris, nu, finement grenu
    # (P15) ; le grand naseau en virgule qui remonte vers l'arrière
    nose = HLs([(19.3, 0.5), (20.5, -0.05), (21.6, 0.6), (22.4, 1.35), (22.92, 2.2), (23.06, 3.0), (22.98, 3.85),
                (22.66, 4.55), (21.6, 4.7), (20.4, 4.45), (19.5, 3.55), (19.0, 2.0)])
    nose_poly = Polygon(nose).buffer(0)
    doc.path(cr(nose), fill=doc.rad("mufle", HL(21.9, 2.3), 2.7, [(0, SKIN_LT, 0.95), (0.5, SKIN, 0.9),
                                                                  (0.8, SKIN, 0.55), (1, SKIN, 0.0)]))
    # le dessous du mufle à l'ombre, rose gris froid (P15 : #928E96)
    doc.path(cr(nose), fill=doc.lin("mufle-ombre", HL(21.0, 2.6), HL(21.0, 4.7),
                                    [(0, "#928E96", 0.0), (1, "#928E96", 0.5)]))
    glow("mufle-lumiere", HL(21.4, 0.9), 1.3, 0.55, "#F6ECE8", 0.7, angle=hang(1.0, 0.6), clip=nose_poly)
    nostril = HLs([(20.1, 2.05), (20.8, 2.1), (21.5, 2.2), (22.1, 2.36), (22.6, 2.56), (22.86, 2.86), (22.78, 3.2),
                   (22.46, 3.3), (22.0, 3.12), (21.4, 2.78), (20.8, 2.46), (20.25, 2.2)])
    doc.path(cr(nostril), fill=doc.lin("naseau", HL(22.7, 2.9), HL(20.1, 2.1), [(0, "#120D0F"), (0.45, NOSTRIL),
                                                                            (1, "#6A5658")]))
    doc.path(cr(HLs([(20.6, 2.36), (21.3, 2.6), (21.95, 2.92), (22.4, 3.12)]), closed=False), stroke="#E6CFCB",
             sw=0.1, so=0.6, cap="round")
    # l'aile du naseau, au-dessus, et le sillon qui fend la lèvre (le philtrum)
    doc.path(cr(HLs([(19.9, 1.72), (20.8, 1.7), (21.7, 1.85), (22.45, 2.2)]), closed=False), stroke=SKIN_SH,
             sw=0.14, so=0.5, cap="round")
    doc.path(cr(HLs([(22.95, 3.4), (22.88, 4.08), (22.72, 4.7)]), closed=False), stroke=SKIN_SH, sw=0.14, so=0.6,
             cap="round")
    # quelques points sombres (les follicules des vibrisses) sur le côté du museau
    for k in range(16):
        p = HL(rng.uniform(17.4, 21.2), rng.uniform(2.6, 4.6))
        doc.path(cr(ellipse(p, 0.05, 0.05, 0, 6)), fill="#8E8C8C", fo=0.6)
    # la bouche : la ligne des lèvres, du bout du nez à la commissure
    doc.path(cr(HLs(MOUTH), closed=False), stroke=LIPS_SH, sw=0.22, so=0.85, cap="round")
    doc.path(cr(HLs([(17.8, 5.5), (17.45, 5.38), (17.2, 5.18)]), closed=False), stroke=LIPS_SH, sw=0.13, so=0.4,
             cap="round")
    doc.path(cr(HLs([(22.5, 4.62), (21.4, 4.8), (20.1, 4.98)]), closed=False), stroke=LIPS, sw=0.32, so=0.7,
             cap="round")
    # --- l'habillage de l'œil, immobile, autour du trou du pourtour : les
    # paupières bordées de rose, le pli de la paupière, la caroncule
    rim_out = [EL(a * 1.1, b * 1.2 + (0.04 if b > 0 else -0.03)) for a, b in ALMOND_L]
    doc.path(cr(rim_out) + cr(ALMOND[::-1]), fill=doc.lin("paupieres-roses", EL(0, -0.95), EL(0, 0.8),
                                                          [(0, LID_LT), (0.45, LID), (1, LID_SH)]), rule="evenodd")
    doc.path(cr([EL(-1.15, -0.85), EL(-0.4, -1.18), EL(0.5, -1.16), EL(1.15, -0.72)], closed=False), stroke=COLD_SH,
             sw=0.12, so=0.3, cap="round")
    doc.path(cr(ellipse(EL(1.26, 0.17), 0.2, 0.13, EYE_TILT, 8)), fill="#C98D8E", fo=0.9)
    # le contour de la tête, discret ; ni le long de l'encoche de la mâchoire,
    # ni sur la nuque (l'encolure continue la tête)
    doc.path(cr(HLs(DORSAL), closed=False), stroke="#848C90", sw=0.26, so=0.5, cap="round")
    doc.path(cr(HLs(UNDER[:7]), closed=False), stroke="#848C90", sw=0.24, so=0.45, cap="round")


# La lèvre supérieure : du dessous du naseau à la commissure (lift_part).
# La lèvre du haut (tools/races/README.md) : sous le naseau, du bout du nez
# jusque sous la ligne de la bouche, 17 % de la longueur de la tête.
LIP_L = [(19.1, 5.4), (19.05, 4.6), (19.1, 3.85), (19.9, 3.45), (20.9, 3.32), (21.8, 3.38), (22.55, 3.3),
         (23.06, 3.5), (23.02, 4.2), (22.8, 4.78), (22.5, 4.92), (21.7, 5.06), (20.5, 5.21)]
LIP_POLY = HLs(LIP_L)


def gum():
    """Le bourrelet dentaire (le mouton n'a pas d'incisives en haut), rose,
    caché sous la lèvre tant qu'elle est baissée : bien au-dessus de la ligne
    de la bouche (articulations.gum), et le fond sombre de la lèvre."""
    pad = HLs([(19.4, 4.85), (19.55, 4.25), (20.05, 3.95), (20.9, 3.78), (22.0, 3.74), (22.65, 3.92), (22.6, 4.32),
               (22.15, 4.58), (21.1, 4.72), (20.05, 4.84)])
    ridge = HLs([(19.75, 4.62), (20.6, 4.5), (21.6, 4.4), (22.25, 4.22)])
    out = A.gum([S(p) for p in pad], [S(p) for p in ridge], GUM, GUM_SH, 0.22)
    dark = HLs([(19.55, 4.2), (20.2, 4.02), (21.3, 3.95), (22.35, 4.02)])
    el = ET.fromstring(element(cr(dark, closed=False), stroke="#7D4550", sw=0.18, so=0.7, cap="round")
                       .replace("<path ", f'<path xmlns="{A.SVG_NS}" ', 1))
    el.tag = A.Q + "path"
    return out + [el]


# ================================================================== assemblage
def gradients():
    doc.lin("robe", (0, ROBE_TOP), (0, ROBE_BOTTOM), ROBE_STOPS)
    doc.lin("queue", (0, 64.0), (0, 25.0), [(0, "#EFE9DC"), (0.3, "#E2D8C6"), (0.48, "#DDD7CB"),
                                           (0.62, "#EAE8E2"), (1, "#D9D9D4")])
    doc.lin("lumiere-dos", (0, 71.0), (0, 59.5), [(0, "#FFFDF7", 0.4), (0.45, "#FFFDF7", 0.14),
                                                  (1, "#FFFDF7", 0.0)])
    doc.lin("ombre-bas", (0, 53.0), (0, 40.5), [(0, "#9C8A6E", 0.0), (0.55, "#9C8A6E", 0.14),
                                                (1, "#9C8A6E", 0.22)])
    doc.lin("ombre-avant", (40.0, 0), (84.0, 0), [(0, "#8E7D63", 0.0), (1, "#8E7D63", 0.16)])
    doc.lin("tete", HL(8.0, -3.6), HL(8.0, 8.4), HEAD_STOPS)


def articulate(svg):
    """Recoupe les pattes au carpe et au jarret (split_leg) et détache la
    lèvre de la tête (lift_part)."""
    parser = ET.XMLParser(target=ET.TreeBuilder(insert_comments=True))
    root = ET.fromstring(svg, parser=parser)
    paints = A.Paints(root)
    text = svg
    for name, kind, dx, far in LEGS:
        jh = H_CARPE if kind == "front" else H_JARRET
        joint, half = A.leg_joint(text, name, S((0, jh))[1])
        A.split_leg(root, paints, name, joint, half + 0.4)
    A.lift_part(root, paints, "head", [S(p) for p in LIP_POLY], "lip", S(HL(*LIP_PIVOT_L)), under=gum())
    ET.indent(root, space="  ")
    return ET.tostring(root, encoding="unicode") + "\n"


def main(path):
    gradients()
    doc.comment("Pattes : toutes sous le corps, celles du fond d'abord")
    for name, kind, dx, far in LEGS:
        leg(name, kind, dx, far)
    doc.comment("Encolure : sous le corps, qui recouvre sa base")
    neck()
    doc.comment("Corps : la robe tondue, le ventre nu, la mamelle")
    body()
    doc.comment("Queue : naturelle, jusqu'au jarret, en trois morceaux")
    tail()
    doc.comment("Tête : longue, fine, busquée, sans cornes, au poil ras argenté")
    head()
    doc.pivot("ground", (39.0, 0.0))
    comment = ("  Lacaune (Lacaune lait, rayon de Roquefort) : une brebis de trois à quatre ans\n"
               "  en juin, tondue à la mi-mai, en lactation, de profil. 1 unité = 1 cm de l'animal\n"
               "  (70 cm au garrot). Dessin écrit par tools/races/lacaune/gen_lacaune.py d'après\n"
               "  la fiche docs/races/lacaune.md ; rig au contrat de docs/rig.md.\n"
               "  © 2026 Alejandro San Martin Lamas — MIT OR Apache-2.0")
    svg = doc.render(126, 98, comment)
    svg = articulate(svg)
    if VARIANTE:
        return variante(path, svg)
    with open(path, "w") as f:
        f.write(svg)
    print(f"{path} : {len(svg) / 1024:.0f} Ko, {svg.count('<path')} tracés")


def variante(path, svg):
    """Les autres dessins : ce qui change, puis les proportions."""
    root = ET.fromstring(svg)
    if VARIANTE == "belier":
        variantes.bourses(root, *variantes.sous_le_ventre(root, 8, -3), largeur=9.5, hauteur=15,
                  peau="#D9C0B5", ombre="#8E7265")
    variantes.ecrire(path, root, **PROPORTIONS[VARIANTE])


if __name__ == "__main__":
    main(OUT)
