#!/usr/bin/env python3
"""Génère le rig SVG du Suffolk : un bélier britannique de trois ans, en
octobre, quatre mois et demi après la tonte (toison de 3 à 4 cm), de profil,
tourné vers la droite, d'après la fiche docs/races/suffolk.md.

Repère de travail : le « monde », en centimètres de l'animal réel. x vers
l'avant (0 à l'arrière de la fesse, en toison), h vers le haut (0 au sol).
Dans le SVG, 1 unité = 1 cm : X = x + OX, Y = OY - h (voir outils.py).

Le dessin est écrit d'une pièce par patte, l'encolure à part, sous le corps
(la variante A du contrat, comme le Mannequin), puis recoupé par
tools/races/articulations.py : chaque patte au carpe ou au jarret, la lèvre
supérieure détachée de la tête avec le bourrelet dentaire dessous. La
toison (toison.py) est tirée une seule fois sur l'union des pièces de laine
du côté proche : au repos, ses crevasses passent d'une pièce à l'autre sans
rupture, et les jointures ne se voient pas.

Usage : gen_suffolk.py <sortie.svg>

© 2026 Alejandro San Martin Lamas — MIT OR Apache-2.0
"""
import math
import os
import random
import sys
import xml.etree.ElementTree as ET

HERE = os.path.dirname(os.path.abspath(__file__))
sys.path.insert(0, HERE)
sys.path.insert(0, os.path.dirname(HERE))
import articulations as A  # noqa: E402  (tools/races/articulations.py)
import variantes  # noqa: E402

# Les autres dessins de la race (tools/races/variantes.py), d'après la fiche.
# Sans variante : le bélier.
VARIANTE = sys.argv[2] if len(sys.argv) > 2 else ""
PROPORTIONS = {
    "brebis": dict(  # Brebis Suffolk : 75 cm au garrot, tête plus étroite au museau assez fin, encolure sans crête, mamelle.
        garrot=(85, 75), encolure=(1.0, .84), tete=(.96, .93)),
    "agneau": dict(  # Agneau Suffolk de deux semaines : 45 cm au garrot, tête et pattes noires, robe de naissance sombre qui blanchit en quelques mois.
        variantes.AGNEAU, garrot=(85, 45)),
}
import toison as T  # noqa: E402
from outils import (Doc, Fills, LineString, Marks, Noise1D, Point, S, clamp, cr, dense, dense_runs,  # noqa: E402
                    ellipse_pts, geom_d, line_d, mix, normals, poisson, resample, runs_path, shape, smooth, union)

# ------------------------------------------------------------------ mesures (fiche, § 2)

# Les mesures de la fiche ; les contours plus bas (le corps, l'encolure, la
# tête, les pattes) sont dessinés à ces cotes, et les autres s'en servent.
GARROT = 85.0          # hauteur au garrot, os (§ 2.2 : 85 cm)
DOS = 89.0             # le haut de la toison sur le dos : 3 à 4 cm de laine (§ 2.3 : 89)
VENTRE = 34.0          # la ligne du ventre en toison (§ 2.3 : 34)
LONGUEUR = 117.0       # du poitrail à la fesse, en toison (§ 2.3 : 117)
TETE = 31.5            # de la nuque (pivot-head) au bout du nez (§ 2.3 et 2.7 : 31 à 34)
OREILLE = 16.0         # de la base à la pointe (§ 2.3 et 2.8 : 14 à 17)
OREILLE_L = 6.8        # largeur maximale, aux deux tiers (§ 2.8 : 6 à 7)
CARPE = 20.5           # centre du carpe (§ 2.3 et 2.6 : 20 ; 0,22 à 0,25 H)
JARRET = 26.0          # pointe du jarret (§ 2.3 et 2.6 : 26) : le contour du postérieur y est dessiné
JARRET_PIVOT = 27.5    # le centre de l'articulation, un peu au-dessus de la pointe
COUDE = 42.0           # le coude (§ 2.3 : 42)
LAINE_AV = 25.0        # bas de la laine sur l'avant-bras, devant (§ 3.2 : 0,25 à 0,30 H)
LAINE_AV_DOS = 24.0    # ... un peu plus bas derrière (§ 3.2, 5)
LAINE_AR = 26.6        # bas de la laine à la pointe du jarret (§ 3.2, 6)
LAINE_AR_AVANT = 31.0  # ... elle remonte un peu devant (§ 3.2, 6)
QUEUE = 10.0           # moignon de queue coupée, ce qui dépasse de la croupe (§ 2.9 : 9 à 10)
SCROTUM_BAS = 29.5     # bas du scrotum (§ 2.3 : 29)

# Les sabots (centre de la sole) : de profil, l'antérieur sous l'épaule,
# le postérieur sous la pointe de la fesse (§ 7) ; 79 cm d'un pied à
# l'autre, comme sur P17 et P25. Les pattes du fond, décalées vers le milieu.
X_HN, X_HF = 13.5, 22.0
X_FN, X_FF = 92.5, 84.0

PIVOT_EPAULE = 54.0    # l'épaule, haut dans le corps (racine de l'antérieur)
PIVOT_HANCHE = 56.0    # la hanche (racine du postérieur)

# L'encolure et la tête.
NECK_BASE = (102.5, 65.5)       # pivot-neck : la base de l'encolure, dans l'épaule
HEAD_O = (125.2, 101.6)         # le haut du crâne, entre les oreilles (repère de la tête)
HEAD_LIFT = math.radians(6.0)   # la tête de P5, relevée de 6° : chanfrein à 64° sous l'horizontale (§ 2.5)

# Les tirages au hasard ont chacun leur graine fixe (random.Random(...)), là
# où ils servent : le même script écrit toujours le même dessin.
D = Doc()

# ------------------------------------------------------------------ palette (fiche, § 4)

FACE = "#1A1F24"
FACE_LIGHT = "#3A4248"          # reflet bleuté du poil noir
FACE_DEEP = "#0F1316"
CHANFREIN_SUN = "#3D4549"
CROWN = "#26282B"               # dessus du crâne, poil plus épais
CROWN_LIGHT = "#4A4A4D"
CROWN_DEEP = "#16171A"
CROWN_RUST = "#534533"          # la nuque un peu brunie par l'été (§ 4 : une nuance, au plus)
MUFLE = "#24282A"
MUFLE_WET = "#4A4C4D"
MUFLE_DEEP = "#141618"
NASEAU = "#0A0B0C"
EAR = "#22252A"
EAR_LIGHT = "#4E555E"
EAR_DEEP = "#15171A"
EAR_IN = "#3A2F23"
EAR_IN_LIGHT = "#5A4835"
EAR_IN_DEEP = "#27221B"
IRIS = "#6A4A33"
IRIS_LIGHT = "#79563A"
IRIS_DEEP = "#3E3023"
PUPIL = "#0C0B0A"
LID = "#101214"
LEG = "#171C1F"
LEG_LIGHT = "#2E363B"
LEG_DEEP = "#0E1214"
HOOF = "#1A1A1C"
HOOF_LIGHT = "#3C3C40"
HOOF_DEEP = "#0D0D0E"
SKIN = "#B8988C"                # peau rose (scrotum)
SKIN_LIGHT = "#D4B5A0"
SKIN_DEEP = "#7A6460"
GUM_FILL, GUM_CREASE = "#9A6670", "#6E4250"
CATCH = "#E9ECEE"


# ================================================================== le repère de la tête

def HL(dx, dh):
    """Repère de la tête (cm ; origine au haut du crâne, entre les oreilles ;
    dx vers l'avant, dh vers le haut, la tête portée comme sur P5) vers le
    monde : la même tête, relevée de HEAD_LIFT."""
    c, s = math.cos(HEAD_LIFT), math.sin(HEAD_LIFT)
    return (HEAD_O[0] + dx * c - dh * s, HEAD_O[1] + dx * s + dh * c)


def HLs(pts):
    return [HL(*p) for p in pts]


def head_dir(dx, dh):
    """Une direction du repère de la tête, dans le monde."""
    c, s = math.cos(HEAD_LIFT), math.sin(HEAD_LIFT)
    return (dx * c - dh * s, dx * s + dh * c)


# Le profil de la tête, relevé sur P5 (bélier de profil strict, dehors) et
# ramené à 31,5 cm de la nuque au bout du nez : le crâne large et plat entre
# les oreilles, puis une courbe busquée régulière jusqu'au nez (§ 5 : la
# flèche fait 7,5 % de la corde tirée du profil au droit de l'œil jusqu'au
# bout du nez), le museau large et rond, la ganache profonde (§ 5).
TOP = [(-6.0, -0.9), (-3.0, 0.35), (0.0, 0.85), (3.0, 0.95), (6.0, 0.8), (9.0, 0.3), (11.6, -0.9),
       (13.8, -2.9), (15.4, -5.4), (16.5, -8.1), (17.4, -11.2), (18.2, -14.6), (18.9, -18.1), (19.5, -21.6),
       (19.95, -24.4), (20.15, -26.3)]
NOSE_FRONT = [(20.15, -26.3), (20.1, -27.7), (19.8, -28.8), (19.25, -29.7)]
MOUTH = [(19.25, -29.7), (18.2, -30.05), (17.0, -30.2), (15.8, -30.15), (15.0, -29.95)]
NOTCH = [(15.0, -29.95), (14.7, -30.6), (14.5, -31.25)]
# La ganache : profonde, en ligne presque droite du menton à l'angle de la
# mâchoire, puis elle remonte vers la gorge et le col (§ 5).
JAWLINE = [(14.5, -31.25), (12.6, -31.75), (10.5, -32.0), (8.4, -31.95), (6.6, -31.45), (5.0, -30.7), (3.5, -30.1)]
# La limite de la laine, sur le côté : de derrière l'oreille, en oblique vers
# l'avant, jusqu'à la gorge, juste derrière l'angle de la mâchoire (§ 3.2).
COLLAR = [(3.5, -30.1), (1.9, -28.4), (-0.4, -25.6), (-2.8, -22.0), (-4.7, -17.8), (-5.9, -13.0),
          (-6.4, -8.4), (-6.5, -4.4), (-6.2, -1.4), (-6.0, -0.9)]
EYE_L = (6.5, -9.95)            # l'œil : haut sur le côté, à 0,36 de la tête depuis la nuque (P3 : 0,33 ; P5 : 0,41)
EAR_L = (-1.4, -4.9)            # la base de l'oreille : derrière l'œil, un peu plus haut (§ 5)
POLL_L = (-3.6, -5.6)           # la nuque (pivot-head) : l'articulation, derrière l'oreille
JAW_PIVOT_L = (14.3, -30.55)    # la charnière de la mâchoire, sous la joue, à hauteur de la bouche
MUZZLE_L = (17.9, -31.5)        # ce qui touche l'herbe : le bas des lèvres


# ================================================================== les formes du corps (monde)

# Le corps : un grand bloc long et profond, le dessus droit et plat jusqu'au
# garrot (noyé dans la laine), la croupe longue, la fesse ronde, le poitrail
# qui avance loin devant les antérieurs (§ 2.4 ; P17, P25 : le garrot 12 à
# 20 cm devant l'antérieur, le poitrail 25 cm devant).
BODY_TOP = [(3.2, 84.2), (6.0, 87.3), (10.5, 88.75), (17.0, 89.3), (27.0, 89.45), (39.0, 89.2), (51.0, 88.9),
            (63.0, 88.75), (74.0, 88.95), (84.0, 89.3), (93.0, 89.6), (99.0, 89.8), (103.0, 89.95)]
# L'épaule : le bord du corps sur l'encolure (une jointure cachée au repos ;
# le devant du corps quand l'encolure descend brouter).
SHOULDER = [(103.0, 89.95), (106.8, 88.6), (110.0, 85.6), (112.4, 81.0), (114.0, 75.0), (114.8, 68.4),
            (115.0, 61.6), (115.3, 55.0), (116.4, 49.2), (117.5, 45.6)]
CHEST = [(117.5, 45.6), (117.2, 43.6), (115.6, 41.0), (112.9, 37.8), (109.6, 35.7), (105.9, 34.5), (102.0, 34.05)]
# Le dessous : sur l'avant-bras (jointure cachée au repos), le passage des
# sangles derrière le coude, le ventre plat, le flanc plein qui rejoint la
# jambe.
BELLY = [(102.0, 34.05), (96.5, 33.8), (90.0, 34.1), (84.5, 35.0), (78.0, 34.7), (69.0, 34.1), (59.0, 33.95),
         (50.0, 34.3), (42.0, 35.2), (35.0, 36.6), (29.5, 38.3), (25.5, 40.0)]
# Sur la jambe (jointure cachée au repos) jusqu'à l'arrière de la cuisse.
HIP_SEAM = [(25.5, 40.0), (20.5, 41.7), (14.5, 42.5), (8.5, 42.4), (3.0, 41.9)]
REAR = [(3.0, 41.9), (1.6, 46.5), (0.6, 52.0), (0.05, 58.5), (0.2, 65.5), (0.9, 72.5), (1.9, 78.8), (3.2, 84.2)]

BODY_PTS = dense_runs([BODY_TOP, SHOULDER, CHEST, BELLY, HIP_SEAM, REAR], per=6)

# L'encolure, à part, sous le corps et sous la tête (variante A) : la crête
# du bélier, qui monte en courbe du garrot vers la nuque (§ 2.5 : 25 à 35°) ;
# le dessous qui descend presque droit vers le poitrail (P17 : 60°).
CREST = [(96.0, 89.7), (100.0, 89.85), (103.0, 90.1), (105.8, 91.0), (108.8, 92.7), (111.8, 94.8),
         (114.8, 97.0), (117.6, 98.9)]
THROAT = [(116.4, 43.0), (117.6, 45.0), (119.2, 49.4), (121.2, 54.2), (123.8, 59.2), (127.0, 64.0),
          (130.3, 68.0), (132.4, 71.4)]
NECK_CAP_R = 11.5      # le haut de l'encolure, caché sous la tête, derrière la nuque : jusqu'à 11,5 cm d'elle


HEAD_LIFT_MAX = 1.4     # de combien la tête peut s'ouvrir sur l'encolure (le flehmen : 0,75 rad ; broute : 1,35)


def rotate_w(p, c, a):
    """Un point du monde tourné de `a` (radians, sens direct : le nez monte)
    autour de `c`."""
    ca, sa = math.cos(a), math.sin(a)
    dx, dh = p[0] - c[0], p[1] - c[1]
    return (c[0] + dx * ca - dh * sa, c[1] + dx * sa + dh * ca)


def throat_fan():
    """Ce que la tête découvre en s'ouvrant sur l'encolure (le bêlement, le
    flehmen, et surtout pour brouter : la cinématique inverse du moteur
    l'ouvre alors de 77°, la tête presque verticale au bout de l'encolure
    qui descend) : l'éventail balayé par la limite du col quand la tête
    tourne à la nuque, jusqu'à HEAD_LIFT_MAX, fermé en bas par une ligne de
    gorge droite, de la fin de la gorge dessinée vers l'angle de la mâchoire
    ouverte. Caché sous la tête au repos ; découvert, il continue l'encolure
    jusqu'à la mâchoire, sans jour ni bosse."""
    poll = HL(*POLL_L)
    line = dense(HLs(COLLAR), False, 3)
    steps = 16
    quads = []
    for k in range(steps):
        a0, a1 = HEAD_LIFT_MAX * k / steps, HEAD_LIFT_MAX * (k + 1) / steps
        r0 = [rotate_w(p, poll, a0) for p in line]
        r1 = [rotate_w(p, poll, a1) for p in line]
        for i in range(len(line) - 1):
            q = shape([r0[i], r0[i + 1], r1[i + 1], r1[i]])
            if not q.is_empty:
                quads.append(q.buffer(0.02))
    fan = union(*quads)
    start = THROAT[-1]
    end = rotate_w(HL(*COLLAR[0]), poll, HEAD_LIFT_MAX)
    ux, uh = end[0] - start[0], end[1] - start[1]
    L = math.hypot(ux, uh)
    ux, uh = ux / L, uh / L
    nx, nh = -uh, ux            # à gauche de la corde : vers la nuque
    far = 200.0
    above = shape([(start[0] - ux * far, start[1] - uh * far), (start[0] + ux * far, start[1] + uh * far),
                   (start[0] + ux * far + nx * far, start[1] + uh * far + nh * far),
                   (start[0] - ux * far + nx * far, start[1] - uh * far + nh * far)])
    return fan.intersection(above), (start, end)


def neck_shape():
    """L'encolure entière : ce qui se voit au repos (la crête, la gorge),
    le haut caché sous la tête (autour de la nuque, et l'éventail que la
    tête découvre en se relevant), et le bas caché sous le corps, avec un
    disque autour de la base (il reste sous le corps quand elle tourne)."""
    collar = HLs(COLLAR)                    # de la gorge au haut de la nuque
    outline = (CREST + collar[::-1] + THROAT[::-1] +
               [(110.0, 42.0), (101.0, 46.0), (93.0, 56.0), (90.0, 70.0), (91.0, 82.0)])
    vis = shape(dense(outline, True, 4, t=0.8))
    body = shape(BODY_PTS)
    head = shape(dense_runs(head_outline_runs(), per=4))
    # derrière la nuque, sous le prolongement de la crête : ce que la tête
    # découvre en s'abaissant sur l'encolure (aux aguets, tête basse) ; rien
    # devant la nuque ni au-dessus de la crête, qui dépasserait du front ou
    # de la nuque quand la tête tourne
    (ax, ah), (bx, bh) = CREST[-2], CREST[-1]
    L = math.hypot(bx - ax, bh - ah)
    ux, uh = (bx - ax) / L, (bh - ah) / L
    below_crest = shape([(bx - ux * 200, bh - uh * 200), (bx + ux * 200, bh + uh * 200),
                         (bx + ux * 200 + uh * 200, bh + uh * 200 - ux * 200),
                         (bx - ux * 200 + uh * 200, bh - uh * 200 - ux * 200)])
    behind_poll = shape(HLs([(POLL_L[0] + 0.5, 20.0), (POLL_L[0] + 0.5, -40.0), (-60.0, -40.0), (-60.0, 20.0)]))
    cap = Point(HL(*POLL_L)).buffer(NECK_CAP_R, 32).intersection(head.buffer(-0.5))
    cap = cap.intersection(below_crest).intersection(behind_poll)
    fan, _ = throat_fan()
    # loin du front et du chanfrein : quand la tête se referme sur
    # l'encolure (aux aguets, 0,25 rad) ou se tourne vers nous (elle
    # rétrécit en s'ébrouant), son profil recule d'autant plus qu'il est loin
    # de la nuque ; l'éventail ne doit pas dépasser devant le front
    poll = HL(*POLL_L)
    profile = dense(HLs(TOP + NOSE_FRONT[1:]), False, 4)
    band = union(*(Point(p).buffer(0.32 * math.dist(p, poll) + 0.4, 12) for p in profile))
    fan = fan.intersection(union(head, vis).buffer(-0.45).difference(band))
    disc = Point(NECK_BASE).buffer(13.0, 32).intersection(body.buffer(-0.5))
    full = union(vis, cap, fan, disc)
    # le comblement, caché sous le corps : entre la base et le dessin visible
    fill = full.convex_hull.intersection(body.buffer(-0.6))
    return union(full, fill)


# ------------------------------------------------------------------ les pattes (monde)

def foreleg_outline(cx):
    """L'antérieur d'une pièce : l'avant-bras en laine (caché en haut dans le
    corps), le carpe plat et large (l'os accessoire saillant derrière), le
    canon droit et étroit, le tendon derrière, le boulet, le paturon court et
    incliné, l'onglon. Droit au repos (§ 2.6 : carpe à 180°)."""
    front = [(cx + 5.6, 46.0), (cx + 5.9, 40.0), (cx + 5.6, 34.0), (cx + 5.0, 29.5), (cx + 4.3, 26.0),
             (cx + 3.95, 23.8), (cx + 4.1, 22.4), (cx + 4.25, 20.8), (cx + 4.0, 19.0), (cx + 3.25, 17.5),
             (cx + 2.7, 15.4), (cx + 2.5, 12.8), (cx + 2.55, 10.4), (cx + 2.9, 8.5), (cx + 3.2, 7.3),
             (cx + 3.7, 5.9), (cx + 4.3, 4.6), (cx + 4.75, 3.7)]
    back = [(cx - 2.35, 3.2), (cx - 2.85, 4.9), (cx - 3.05, 6.4), (cx - 2.6, 8.4), (cx - 2.05, 10.6),
            (cx - 1.95, 13.4), (cx - 2.05, 15.9), (cx - 2.55, 17.6), (cx - 3.0, 19.2), (cx - 2.85, 20.9),
            (cx - 2.8, 22.6), (cx - 3.15, 24.4), (cx - 4.3, 27.5), (cx - 5.7, 32.0), (cx - 6.8, 37.0),
            (cx - 7.4, 42.0), (cx - 7.5, 46.0)]
    return front, back


def hindleg_outline(cx):
    """Le postérieur d'une pièce : la jambe en laine jusqu'au jarret (le gigot
    descend bas), le jarret large et net (sa pointe saillante derrière), le
    canon droit, le tendon, le boulet, le paturon court, l'onglon. Le jarret
    ouvert à 145° (§ 2.6). L'arrière de la jambe continue la fesse du corps ;
    au-dessus de la jointure, il rentre vite dedans (caché au repos)."""
    dx = cx - X_HN
    front = [(cx + 13.5, 52.0), (cx + 13.0, 46.0), (cx + 11.6, 41.5), (cx + 9.6, 38.0), (cx + 7.0, 34.6),
             (cx + 4.6, 31.8), (cx + 3.1, 29.8), (cx + 2.55, 27.8), (cx + 2.4, 26.0), (cx + 2.2, 24.0),
             (cx + 2.0, 21.0), (cx + 1.95, 17.0), (cx + 2.0, 13.0), (cx + 2.15, 10.6), (cx + 2.55, 8.5),
             (cx + 2.95, 7.2), (cx + 3.5, 5.8), (cx + 4.1, 4.5), (cx + 4.55, 3.7)]
    back = [(cx - 2.3, 3.2), (cx - 2.8, 4.9), (cx - 3.0, 6.5), (cx - 2.55, 8.6), (cx - 2.05, 10.8),
            (cx - 1.95, 14.0), (cx - 2.05, 17.5), (cx - 2.4, 20.5), (cx - 3.0, 23.0), (cx - 3.7, 24.9),
            (cx - 4.2, 26.3)]
    back += [(x + dx, h) for (x, h) in [(8.3, 28.9), (6.6, 31.6), (5.0, 34.6), (3.8, 38.0), (3.0, 41.9),
                                        (3.4, 45.6), (5.4, 49.6), (8.8, 53.6), (13.0, 57.0)]]
    return front, back


def hoof_outline(cx, hind=False):
    """L'onglon de profil : la paroi un peu bombée qui descend vers la pince
    pointue, la sole, le talon profond et son bourrelet (§ 7 : doigts égaux,
    serrés, pointés vers l'avant, talon profond)."""
    top = (cx + (4.55 if hind else 4.75), 3.7)
    toe = cx + (5.75 if hind else 5.9)
    wall = [top, (top[0] + 0.35, 2.7), (top[0] + 0.7, 1.6), (toe - 0.15, 0.6), (toe, 0.08), (toe - 0.25, 0.0)]
    sole = [(toe - 0.25, 0.0), (cx - 1.1, 0.0)]
    heel = [(cx - 1.1, 0.0), (cx - 1.7, 0.35), (cx - 2.1, 1.1), (cx - 2.3, 2.0), (cx - 2.35, 3.2)]
    return wall, sole, heel


# ================================================================== la toison : les formes, la lumière

def tail_shape():
    """Le moignon de la queue coupée : attaché haut, au bout du dos, il
    dépasse un peu de la fesse et tombe contre elle, le bout arrondi
    (§ 2.9 ; P11)."""
    pts = [(10.4, 85.6), (8.6, 88.4), (5.8, 88.25), (3.6, 87.5), (1.7, 86.0), (0.2, 83.8), (-0.65, 81.4),
           (-0.8, 79.3), (-0.1, 77.6), (1.2, 77.1), (2.5, 77.9), (3.5, 79.6), (4.8, 81.9), (6.7, 83.8),
           (8.6, 84.8)]
    return shape(dense(pts, True, 6, t=0.85))


def light_body(x, h):
    """La lumière de la toison (0 à 1) : d'en haut à gauche ; le dos et la
    croupe au soleil, le bas des flancs à l'ombre propre, le dessous dans le
    creux ; l'épaule et le gigot bombés."""
    t = clamp((h - 32.0) / 58.0)
    v = 0.12 + 0.82 * t ** 0.9
    v += 0.08 * math.exp(-((x - 14) ** 2 + (h - 64) ** 2) / 260.0)      # le gigot
    v += 0.06 * math.exp(-((x - 99) ** 2 + (h - 72) ** 2) / 200.0)      # l'épaule
    v -= 0.1 * clamp((x - 108) / 12.0) * clamp((80 - h) / 30.0)         # le poitrail, tourné vers l'avant
    return clamp(v)


def flow(x, h):
    """L'angle des crevasses (0 : verticales). Sur l'encolure, elles
    descendent de la crête vers la gorge ; ailleurs, presque droites, un peu
    couchées par la courbe de la croupe et du poitrail."""
    a = 0.0
    # l'encolure : en travers de son axe
    w = smooth(96.0, 108.0, x) * smooth(62.0, 76.0, h)
    a += 0.42 * w
    # la croupe : elles suivent la fesse
    a -= 0.25 * smooth(10.0, 0.0, x) * smooth(55.0, 80.0, h)
    return a


def depth_of(x, h):
    """Les crevasses sont fermées sur le dos (toison serrée, le contour presque
    lisse), ouvertes sur les flancs, l'épaule, le gigot et l'encolure (P1,
    P11, P17)."""
    top = smooth(88.5, 78.0, h)
    neck = smooth(100.0, 110.0, x)
    return clamp(0.12 + 0.88 * max(top, 0.7 * neck)) * (0.75 + 0.25 * smooth(30.0, 50.0, h))


GRADS = {}


def fleece_paints():
    """Les dégradés de la toison, en coordonnées absolues : les mêmes sur
    toutes les pièces du côté proche."""
    if GRADS:
        return GRADS
    GRADS["base"] = D.lin("laine", (0, 92.0), (0, 30.0),
                          [(0.0, "#EEE8DB"), (0.12, T.T_LIGHT), (0.3, "#DCD5C3"), (0.52, "#CEC6B3"),
                           (0.7, "#C5BDAA"), (0.85, "#B0A895"), (0.94, T.T_SHADE), (1.0, "#8A8371")])
    # Le volume : le bout de la croupe au soleil, le poitrail tourné vers
    # l'avant à l'ombre, le dessous du ventre dans le creux.
    GRADS["bouts"] = D.lin("laine-bouts", (-2.0, 0), (124.0, 0),
                           [(0.0, "#F6F2E9", 0.32), (0.07, "#F6F2E9", 0.0), (0.86, "#6E6756", 0.0),
                            (0.97, "#6E6756", 0.26), (1.0, "#6E6756", 0.3)])
    GRADS["dessous"] = D.lin("laine-dessous", (0, 40.0), (0, 32.5),
                             [(0.0, "#6E6756", 0.0), (0.55, "#6E6756", 0.14), (1.0, "#4E4838", 0.4)])
    GRADS["gigot"] = D.rad("laine-gigot", (13.0, 66.0), 26.0,
                           [(0.0, "#FAF6EE", 0.3), (0.55, "#FAF6EE", 0.08), (1.0, "#FAF6EE", 0.0)],
                           f=(9.0, 74.0), sx=1.0, sy=1.15)
    GRADS["epaule"] = D.rad("laine-epaule", (98.0, 73.0), 21.0,
                            [(0.0, "#FAF6EE", 0.26), (0.6, "#FAF6EE", 0.06), (1.0, "#FAF6EE", 0.0)],
                            f=(94.0, 80.0), sx=0.85, sy=1.2, rot=-25.0)
    # l'ombre derrière l'épaule et devant le gigot : des volumes, sans creux
    GRADS["sangle"] = D.rad("laine-sangle", (80.0, 50.0), 18.0,
                            [(0.0, "#6E6756", 0.14), (1.0, "#6E6756", 0.0)], sx=0.6, sy=1.4)
    GRADS["grasset"] = D.rad("laine-grasset", (30.0, 46.0), 13.0,
                             [(0.0, "#6E6756", 0.2), (1.0, "#6E6756", 0.0)], sx=0.75, sy=1.1)
    # l'encolure : la crête au soleil, la gorge à l'ombre de la tête
    GRADS["crete"] = D.rad("laine-crete", (104.0, 92.0), 13.0,
                           [(0.0, "#FAF6EE", 0.28), (1.0, "#FAF6EE", 0.0)], sx=1.6, sy=0.7, rot=22.0)
    GRADS["gorge"] = D.rad("laine-gorge", (121.0, 66.0), 13.0,
                           [(0.0, "#4E4838", 0.34), (0.5, "#6E6756", 0.14), (1.0, "#6E6756", 0.0)],
                           sx=0.8, sy=1.3, rot=-20.0)
    return GRADS


def fleece_piece(piece, cracks, seed, outline=None, extra=(), grain=True):
    """Une pièce de toison du côté proche : le fond (dégradé du corps), les
    volumes, les crevasses, le grain. `outline` : le tracé du contour (sinon
    celui de la forme)."""
    g = fleece_paints()
    d = outline or geom_d(piece, nd=2, simplify=0.02)
    D.el(d, fill=g["base"])
    for key in ("gigot", "epaule", "crete", "sangle", "grasset", "bouts", "dessous", "gorge") + tuple(extra):
        D.el(d, fill=g[key])
    T.draw_cracks(D, piece, cracks, light_body, seed=seed)
    if grain:
        T.draw_grain(D, piece, light_body, seed=seed + 1, flow=flow)


# ================================================================== les pattes

def black_leg(name, cx, front_pts, back_pts, hind, far):
    """Le bas noir d'une patte (sous la laine, jusqu'au sabot) : poil ras,
    lisse, brillant au soleil, modelé par la forme (le carpe ou le jarret, le
    canon et son tendon, le boulet, le paturon) : un reflet bleuté le long du
    devant, l'ombre derrière ; l'ergot ; l'onglon noir de jais."""
    top_h = 34.0 if not hind else 33.0
    fr = [p for p in front_pts if p[1] <= top_h]
    bk = [p for p in back_pts if p[1] <= top_h]
    wall, sole, heel = hoof_outline(cx, hind)
    run_front = [(fr[0][0], top_h + 1.5)] + fr
    run_back = bk + [(bk[-1][0], top_h + 1.5)]
    outline = dense_runs([run_front + [wall[0]], wall, sole, heel, run_back, [run_back[-1], run_front[0]]], per=6)
    sh = shape(outline)
    d = geom_d(sh, simplify=0.01)
    base = D.lin(f"{name}-noir", (cx - 3.2, 0), (cx + 4.4, 0),
                 [(0.0, "#080B0D"), (0.3, "#0D1114"), (0.62, LEG), (0.84, "#1B2125"), (1.0, "#0E1215")] if not far
                 else [(0.0, "#08090B"), (0.5, "#101417"), (1.0, "#0A0C0E")])
    D.el(d, fill=base)
    top = D.lin(f"{name}-haut", (0, top_h), (0, top_h - 8.0), [(0.0, "#07090B", 0.8), (1.0, "#07090B", 0.0)])
    D.el(d, fill=top)
    # le reflet bleuté du poil ras, qui suit le devant de la patte
    front_run = [p for p in fr if 4.5 < p[1] < (24.5 if not hind else 26.5)]
    if not far:
        # un reflet doux et large, plus vif seulement sur le canon et le boulet
        D.el(cr([(x - 0.85, h) for (x, h) in front_run], closed=False), stroke=LEG_LIGHT, sw=1.3, so=0.3,
             cap="round")
        shine = [(x - 0.55, h) for (x, h) in front_run if 7.0 < h < (17.2 if not hind else 22.0)]
        if len(shine) >= 2:
            D.el(cr(shine, closed=False), stroke="#4A5660", sw=0.3, so=0.32, cap="round")
    else:
        D.el(cr([(x - 0.6, h) for (x, h) in front_run], closed=False), stroke="#1E2529", sw=0.7, so=0.5, cap="round")
    hl = "#3A4248"
    if not hind:
        # le carpe : sa face avant, plate, prend la lumière ; le pli dessous ;
        # l'os accessoire derrière
        D.el(cr(ellipse_pts(cx + 3.0, 21.2, 0.9, 1.7, 0.0, 14)),
             fill=D.rad(f"{name}-carpe", (cx + 3.1, 21.4), 1.7, [(0.0, hl, 0.32 if not far else 0.1), (1.0, hl, 0.0)],
                        sx=0.55, sy=1.0))
        D.el(cr([(cx + 3.7, 18.3), (cx + 2.9, 17.75), (cx + 2.0, 17.6)], closed=False), stroke="#050607", sw=0.5,
             so=0.3 if not far else 0.18, cap="round")
        D.el(cr(ellipse_pts(cx - 2.45, 18.9, 0.45, 0.7, 0.0, 10)),
             fill=D.rad(f"{name}-accessoire", (cx - 2.4, 19.1), 0.75, [(0.0, hl, 0.22 if not far else 0.08),
                                                                      (1.0, hl, 0.0)]))
        tendon_top = 17.0
    else:
        # le jarret : sa pointe éclairée, le creux devant, l'ombre sous la pointe
        D.el(cr(ellipse_pts(cx - 3.3, 25.0, 0.6, 0.8, 0.4, 12)),
             fill=D.rad(f"{name}-pointe", (cx - 3.3, 25.2), 0.85, [(0.0, hl, 0.25 if not far else 0.08), (1.0, hl, 0.0)]))
        D.el(cr([(cx + 1.6, 27.0), (cx + 1.2, 25.2), (cx + 1.3, 23.4)], closed=False), stroke="#050607", sw=0.5,
             so=0.4 if not far else 0.25, cap="round")
        tendon_top = 23.0
    # le tendon derrière le canon : le sillon entre l'os et le tendon, le
    # tendon un peu éclairé
    back_run = [p for p in bk if 8.8 < p[1] < tendon_top]
    if len(back_run) >= 2:
        D.el(cr([(x + 0.95, h) for (x, h) in back_run], closed=False), stroke="#040506", sw=0.4,
             so=0.35 if not far else 0.2, cap="round")
    # le boulet : un léger renflement éclairé devant ; l'ergot noir derrière
    D.el(cr(ellipse_pts(cx + 2.0, 7.8, 0.9, 0.9, 0.0, 12)),
         fill=D.rad(f"{name}-boulet", (cx + 2.2, 7.9), 1.0, [(0.0, hl, 0.3 if not far else 0.1), (1.0, hl, 0.0)]))
    D.el(cr(ellipse_pts(cx - 3.05, 5.8, 0.5, 0.38, 0.5, 10)), fill="#060708")
    if not far:
        D.el(cr([(cx - 2.95, 6.12), (cx - 2.65, 6.32)], closed=False), stroke="#4A5660", sw=0.1, so=0.6, cap="round")
    # le poil ras : de fins traits couchés vers le bas, clairs dans le reflet
    m = Marks()
    r2 = random.Random(int(cx * 100) + (7 if hind else 3))
    inner = sh.buffer(-0.3)
    zone = inner.intersection(shape([(cx - 6, 4.0), (cx + 6, 4.0), (cx + 6, top_h), (cx - 6, top_h)]))
    for (x, h) in poisson(zone, 0.55, r2):
        L = r2.uniform(0.35, 0.65)
        edge_x = max((p[0] for p in fr if abs(p[1] - h) < 1.5), default=cx + 2.5)
        lit = clamp(1.0 - abs((edge_x - 0.7) - x) / 1.3)
        a = r2.uniform(-0.08, 0.08)
        pts = [(x, h), (x + math.sin(a) * L, h - math.cos(a) * L)]
        if far:
            if r2.random() < 0.3:
                m.add("#1C2226", 0.05, 0.4, line_d(pts))
        elif lit > 0.15 and r2.random() < 0.3 + 0.6 * lit:
            m.add("#56626C" if lit > 0.6 else "#36404A", 0.05, round(0.25 + 0.35 * lit, 1), line_d(pts))
        elif r2.random() < 0.25:
            m.add("#030405", 0.06, 0.4, line_d(pts))
    m.emit(D)
    # l'onglon : noir de jais, la paroi luisante, les lignes de pousse, la
    # fente entre les doigts ; la couronne, où le poil retombe
    coronet = [(cx - 2.25, 2.4), (cx + 0.4, 3.1), (cx + 2.7, 3.55), wall[0]]
    hoof = dense_runs([wall, sole, heel[:-1] + [coronet[0]], coronet], per=5)
    toe = wall[-2][0]
    hg = D.lin(f"{name}-onglon", (cx - 2.3, 0), (toe, 0),
               [(0.0, HOOF_DEEP), (0.4, HOOF), (0.78, "#2A2A2E" if not far else "#1C1C1F"),
                (0.93, HOOF_LIGHT if not far else "#222225"), (1.0, "#1E1E21")])
    D.el(geom_d(shape(hoof), simplify=0.01), fill=hg)
    if not far:
        # la lumière sur la paroi, les lignes de pousse, parallèles à la couronne
        D.el(cr([(wall[0][0] - 0.25, 3.45), (wall[1][0] - 0.3, 2.6), (wall[2][0] - 0.3, 1.55), (toe - 0.55, 0.45)],
                closed=False), stroke="#6E6E76", sw=0.2, so=0.45, cap="round")
        for k, off in enumerate((0.8, 1.6)):
            D.el(cr([(cx - 1.4 + 0.3 * k, 2.2 - off * 0.55), (cx + 1.0, 2.85 - off * 0.62), (cx + 3.4, 3.3 - off * 0.68),
                     (wall[0][0] + 0.2 + 0.25 * off, 3.55 - off * 0.72)], closed=False), stroke="#2E2E33", sw=0.07,
                 so=0.4, cap="round")
    # la fente entre les deux doigts, jusqu'à la pince
    D.el(cr([(cx + 2.9, 3.5), (cx + 3.5, 2.0), (cx + 4.15, 0.12)], closed=False), stroke="#030304", sw=0.17,
         so=0.9, cap="round")
    # la couronne : le poil noir retombe un peu sur l'onglon
    D.el(cr([(coronet[0][0] + 0.35, coronet[0][1] + 0.1)] + coronet[1:-1] + [(coronet[-1][0] - 0.3, coronet[-1][1])],
            closed=False), stroke="#0B0E10", sw=0.4, so=0.8)
    return sh


def wool_edge(back_pt, front_pt, seed, droop=0.9):
    """Le bas de la laine d'une patte, de l'arrière à l'avant : une ligne
    frangée, irrégulière, faite de petites mèches serrées qui pendent sur le
    poil noir (§ 3.2, 5 et 6)."""
    r2 = random.Random(seed)
    (x0, h0), (x1, h1) = back_pt, front_pt
    n = max(6, int(abs(x1 - x0) / 0.55))
    out = []
    for i in range(n + 1):
        t = i / n
        x = x0 + (x1 - x0) * t
        h = h0 + (h1 - h0) * t + 0.25 * math.sin(7.0 * t + seed)
        if 0 < i < n and i % 2 == 1:
            h -= r2.uniform(0.25, 1.0) * droop
            x += r2.uniform(-0.15, 0.15)
        out.append((x, h))
    return out


def wool_sleeve(name, piece, cx, cracks, far, bottom, top):
    """La laine du haut de la patte : du côté proche, la toison du corps (même
    dégradé, mêmes crevasses : la jointure ne se voit pas) ; plus bas, la
    laine s'amincit et le noir transparaît (§ 3.2, 5) ; elle s'arrête sur un
    bord frangé."""
    if not far:
        fleece_piece(piece, cracks, seed=int(cx * 10) + 5)
    else:
        g = D.lin(f"{name}-laine", (0, 44.0), (0, 30.0), [(0.0, "#948C7A"), (0.6, "#7A7362"), (1.0, "#5E5848")])
        D.el(geom_d(piece, simplify=0.02), fill=g)
        T.draw_cracks(D, piece, cracks, lambda x, h: 0.2, dim=0.1, seed=int(cx * 10) + 5, scale=0.9)
    # le bas de la laine, mince : le noir du poil transparaît
    D.el(geom_d(piece, simplify=0.02), fill=D.lin(f"{name}-mince", (0, top), (0, bottom - 0.6),
                                                   [(0.0, "#2A2620", 0.0), (0.55, "#2A2620", 0.18),
                                                    (1.0, "#1A1916", 0.62)]))


def fringe_tips(edge, far, seed):
    """Les pointes de mèches qui dépassent du bas de la laine, sur le poil
    noir : fines, inégales, un peu courbées, dans l'ombre."""
    r2 = random.Random(seed)
    f = Fills()
    for p in resample(edge, 0.45)[1:-1]:
        if r2.random() < 0.4:
            continue
        L = r2.uniform(0.3, 1.0)
        w = r2.uniform(0.18, 0.38)
        tilt = r2.uniform(-0.45, 0.45)
        bend = r2.uniform(-0.25, 0.25)
        x, h = p
        tip = (x + math.sin(tilt) * L + bend, h - math.cos(tilt) * L)
        mid = (x + math.sin(tilt) * L * 0.5 + bend * 0.4, h - math.cos(tilt) * L * 0.5)
        pts = [(x - w, h + 0.35), (mid[0] - w * 0.55, mid[1]), tip, (mid[0] + w * 0.55, mid[1]), (x + w, h + 0.35)]
        col = mix("#6E6756", "#9E9683", r2.uniform(0.0, 1.0)) if not far else mix("#3E392D", "#5A5444", r2.random())
        f.add(col, 1.0, cr(pts, closed=True, t=0.8))
    f.emit(D)


def foreleg(name, cx, near, cracks):
    front, back = foreleg_outline(cx)
    D.open(name)
    black_leg(name, cx, front, back, hind=False, far=not near)
    # la laine de l'avant-bras : du haut (caché dans le corps) au bord frangé,
    # un rien plus bas derrière (§ 3.2, 5) ; sur la patte du fond, on voit le
    # dedans de l'avant-bras, noir jusque sous le ventre (P1, P19, P25)
    wl_front = LAINE_AV if near else 33.4
    wl_back = LAINE_AV_DOS if near else 32.6
    fr = [p for p in front if p[1] > wl_front + 0.4]
    bk = [p for p in back if p[1] > wl_back + 0.4]
    edge = wool_edge((cx - 3.45, wl_back), (cx + 4.2, wl_front), seed=int(cx * 7), droop=0.8 if near else 0.4)
    outline = fr + edge[::-1] + bk + [(cx - 7.5, 52.0), (cx + 5.6, 52.0)]
    sleeve = shape(dense(outline, True, 3, t=0.7))
    wool_sleeve(name, sleeve, cx, cracks, far=not near, bottom=wl_back, top=wl_front + 6.0)
    fringe_tips(edge, not near, seed=int(cx * 13))
    D.pivot(name, cx - 0.5, PIVOT_EPAULE)
    D.close()


def hindleg(name, cx, near, cracks):
    front, back = hindleg_outline(cx)
    D.open(name)
    black_leg(name, cx, front, back, hind=True, far=not near)
    # la laine du gigot et de la jambe : derrière jusqu'à la pointe du jarret,
    # devant elle remonte un peu (§ 3.2, 6) ; la patte du fond montre le
    # dedans de la cuisse, noir (P1, P5, P19)
    wl_back = LAINE_AR if near else 33.0
    wl_front = LAINE_AR_AVANT if near else 34.0
    fr = [p for p in front if p[1] > wl_front + 0.4]
    bk = [p for p in back if p[1] > wl_back + 0.4]
    edge = wool_edge((cx - 4.3, wl_back), (cx + 3.1, wl_front), seed=int(cx * 7) + 1, droop=0.8 if near else 0.4)
    outline = fr + edge[::-1] + bk + [(cx + 1.0, 59.0), (cx + 13.5, 58.0)]
    sleeve = shape(dense(outline, True, 3, t=0.7))
    wool_sleeve(name, sleeve, cx, cracks, far=not near, bottom=wl_back, top=wl_front + 6.0)
    fringe_tips(edge, not near, seed=int(cx * 13) + 1)
    D.pivot(name, cx + 3.0, PIVOT_HANCHE)
    D.close()


def near_wool_shapes():
    """Les formes de laine du côté proche (pour tirer la toison une fois) :
    le corps, l'encolure, le haut des deux pattes proches, la queue."""
    fr, bk = foreleg_outline(X_FN)
    fore = shape(dense([p for p in fr if p[1] >= LAINE_AV - 1] + [p for p in bk if p[1] >= LAINE_AV_DOS - 1] +
                       [(X_FN - 7.5, 52.0), (X_FN + 5.6, 52.0)], True, 3))
    fr, bk = hindleg_outline(X_HN)
    hind = shape(dense([p for p in fr if p[1] >= LAINE_AR_AVANT - 1] + [p for p in bk if p[1] >= LAINE_AR - 1] +
                       [(X_HN + 1.0, 59.0), (X_HN + 13.5, 58.0)], True, 3))
    return union(shape(BODY_PTS), neck_shape(), fore, hind, tail_shape())


# ================================================================== le corps

def body(cracks):
    D.comment("Corps : un grand bloc de toison serrée en croûte de pain, la fesse ronde, le poitrail en avant")
    D.open("body")
    if not VARIANTE:
        scrotum()
    pts = BODY_PTS
    # le dessus un peu bosselé : chaque cellule de la toison fait sa bosse
    bumpy = T.bumpy_outline(pts, seed=31, amp=0.32, wl=3.2,
                            weight=lambda x, h: smooth(80.0, 87.0, h) + 0.6 * smooth(6.0, 1.0, x) * smooth(45, 60, h))
    piece = shape(bumpy)
    d = geom_d(piece, nd=2, simplify=0.02)
    fleece_piece(piece, cracks, seed=101, outline=d)
    edge_hairs(bumpy)
    belly_fringe()
    D.pivot("body", 58.0, VENTRE)
    D.close()


def edge_hairs(pts):
    """Sur le dos, un fin duvet de pointes de mèches qui accroche la lumière."""
    r2 = random.Random(77)
    m = Marks()
    ns = normals(pts, closed=True)
    for i in range(0, len(pts), 2):
        x, h = pts[i]
        if h < 80.0 or x > 92 or r2.random() < 0.4:
            continue
        nx, nh = ns[i]
        L = r2.uniform(0.4, 1.0)
        bend = r2.uniform(-0.4, 0.4)
        q = [(x - nx * 0.3, h - nh * 0.3), (x + nx * L * 0.5 - nh * bend * 0.2, h + nh * L * 0.5 + nx * bend * 0.2),
             (x + nx * L - nh * bend * 0.5, h + nh * L + nx * bend * 0.5)]
        m.add("#F3EEE3", 0.07, 0.6, cr(q, closed=False, nd=2))
    m.emit(D)


def belly_fringe():
    """Le bas du ventre : de courtes pointes de mèches inégales, en touffes,
    dans l'ombre (la toison est courte : rien ne pend)."""
    edge = [(101.0, 34.05), (90.0, 34.1), (78.0, 34.7), (66.0, 34.05), (52.0, 34.2), (42.0, 35.2)]
    r2 = random.Random(55)
    f = Fills()
    for p in resample(edge, 0.55)[1:-1]:
        if r2.random() < 0.45:
            continue
        x, h = p
        L = r2.uniform(0.25, 1.1) * (1.3 if r2.random() < 0.15 else 1.0)
        w = r2.uniform(0.3, 0.75)
        tilt = r2.uniform(-0.4, 0.4)
        tip = (x + math.sin(tilt) * L, h - math.cos(tilt) * L)
        pts = [(x - w, h + 0.7), (x - w * 0.5, h - L * 0.45), tip, (x + w * 0.5, h - L * 0.45), (x + w, h + 0.7)]
        f.add(mix("#7A7362", "#A39B88", r2.random()), 1.0, cr(pts, closed=True, t=0.8))
    f.emit(D)


def scrotum():
    """Le scrotum, discret, sous la laine du ventre devant le gigot, dans
    l'ombre : laine courte, et en bas la peau rose-gris (§ 7 ; P17, P19,
    P25)."""
    pts = [(27.2, 39.2), (30.6, 39.4), (32.2, 36.6), (32.0, 33.0), (30.6, 30.4), (28.9, 29.6), (27.3, 30.3),
           (26.2, 32.6), (25.9, 36.0)]
    g = D.rad("scrotum", (28.7, 34.6), 5.4, [(0.0, "#988F7D"), (0.55, "#7C7463"), (1.0, "#544C40")], f=(27.6, 36.4))
    D.el(cr(pts, closed=True), fill=g)
    D.el(cr([(27.2, 30.9), (28.9, 30.0), (30.7, 30.7)], closed=False), stroke=SKIN_DEEP, sw=0.9, so=0.35,
         cap="round")
    m = Marks()
    r2 = random.Random(29)
    for (x, h) in poisson(shape(pts).buffer(-0.5), 0.75, r2):
        a0 = r2.uniform(0, 6.28)
        q = [(x + 0.3 * math.cos(a0 + k * 0.9), h + 0.3 * math.sin(a0 + k * 0.9)) for k in range(4)]
        m.add("#B2AA97", 0.08, 0.35, cr(q, closed=False))
    m.emit(D)


# ================================================================== l'encolure

def neck(cracks):
    D.comment("Encolure : sous le corps et sous la tête, qui la recouvrent à ses deux bouts")
    D.open("neck")
    piece = neck_shape()
    fleece_piece(piece, cracks, seed=211)
    # le dessous de la gorge, à l'ombre, sur ce que la tête découvre en se
    # relevant (caché au repos)
    fan, (start, end) = throat_fan()
    fan = fan.intersection(piece.buffer(-0.05))
    ux, uh = end[0] - start[0], end[1] - start[1]
    L = math.hypot(ux, uh)
    nx, nh = -uh / L, ux / L
    D.el(geom_d(fan, simplify=0.02), fill=D.lin("gorge-dessous", start, (start[0] + nx * 5.0, start[1] + nh * 5.0),
                                                 [(0.0, "#4E4838", 0.42), (1.0, "#4E4838", 0.0)]))
    D.pivot("neck", *NECK_BASE)
    D.close()


# ================================================================== la queue

def tail(cracks):
    D.comment("Queue : un moignon laineux de queue coupée, attaché haut, contre la fesse")
    D.open("tail")
    piece = tail_shape()
    # son ombre sur la fesse (rognée au corps : rien sur le fond)
    sh = shape(ellipse_pts(2.9, 77.4, 3.4, 2.0, 0.35, 18)).intersection(shape(BODY_PTS).buffer(-0.1))
    D.el(geom_d(sh), fill=D.rad("queue-ombre", (2.9, 77.4), 3.6, [(0.0, "#4E4838", 0.4), (0.55, "#4E4838", 0.16),
                                                                   (1.0, "#4E4838", 0.0)],
                                 sx=1.0, sy=0.6, rot=20.0))
    outline = T.bumpy_outline(list(piece.exterior.coords)[:-1], seed=17, amp=0.12, wl=1.6)
    piece = shape(outline)
    d = geom_d(piece, simplify=0.02)
    g = fleece_paints()
    D.el(d, fill=g["base"])
    for key in ("gigot", "bouts"):
        D.el(d, fill=g[key])
    # le volume du moignon : clair dessus, sombre dessous, rien près de l'attache
    D.el(d, fill=D.rad("queue-dessus", (0.4, 85.4), 4.4, [(0.0, "#FAF6EE", 0.34), (1.0, "#FAF6EE", 0.0)]))
    D.el(d, fill=D.rad("queue-dessous", (3.0, 78.6), 4.8, [(0.0, "#4E4838", 0.5), (0.6, "#6E6756", 0.22),
                                                           (1.0, "#6E6756", 0.0)]))
    # le bout un peu sali (P11)
    D.el(d, fill=D.rad("queue-bout", (0.3, 78.2), 2.8, [(0.0, "#5A5444", 0.45), (1.0, "#5A5444", 0.0)]))
    T.draw_cracks(D, piece, cracks, light_body, seed=301, scale=0.85)
    T.draw_grain(D, piece, light_body, seed=302, flow=flow, scale=0.85)
    D.pivot("tail", 7.0, 86.6)
    D.close()


# ================================================================== la tête

def head_outline_runs():
    return [HLs(TOP + NOSE_FRONT[1:]), HLs(MOUTH), HLs(NOTCH), HLs(JAWLINE), HLs(COLLAR)]


def head(cracks):
    D.comment("Tête : noir de jais, nue jusque derrière les oreilles, longue, busquée ; longues oreilles en cloche")
    D.open("head")
    ear("ear-far", far=True)
    jaw()
    paints, head_shape = skull()
    eyes()
    eye_cover(paints)
    face_details(head_shape)
    collar(cracks)
    ear("ear-near", far=False)
    D.pivot("head", *HL(*POLL_L))
    D.pivot("muzzle", *HL(*MUZZLE_L))
    D.close()


def skull():
    """Le crâne et la face : le fond noir de jais, modelé par le ciel (un
    reflet gris bleu froid le long du profil busqué, sur le front, l'arcade
    et le haut de la joue : § 4), la joue ronde, le dessous dans l'ombre.
    Rend les peintures posées (le pourtour de l'œil les reprend à
    l'identique) et la forme de la tête."""
    runs = head_outline_runs()
    d = runs_path(runs, nd=2)
    head_shape = shape(dense_runs(runs, per=6))
    paints = []          # (remplissage, opacité, forme ou None : tout le crâne)
    face = D.lin("face", HL(10.0, 1.0), HL(2.0, -32.0),
                 [(0.0, "#232A2F"), (0.3, FACE), (0.7, "#14181B"), (1.0, FACE_DEEP)])
    paints.append((face, 1.0, None))
    soft = [
        # (id, centre, rayon, arrêts, étirement x, y, angle du grand axe en degrés, repère de la tête)
        # le front, large et plat entre les yeux, au ciel
        ("front", HL(8.4, -3.0), 7.5, [(0.0, "#46515A", 0.85), (0.5, CHANFREIN_SUN, 0.45), (1.0, FACE_LIGHT, 0.0)],
         1.55, 0.7, -12.0),
        # le dessus du nez, busqué : un plan clair qui suit le profil
        ("chanfrein", HL(16.6, -13.5), 9.0, [(0.0, "#46515A", 0.75), (0.45, CHANFREIN_SUN, 0.35),
                                             (1.0, FACE_LIGHT, 0.0)], 0.5, 1.45, 24.0),
        # le haut de la joue, sous l'œil
        ("pommette", HL(9.4, -15.6), 5.0, [(0.0, "#4A5660", 0.7), (0.6, FACE_LIGHT, 0.22), (1.0, FACE_LIGHT, 0.0)],
         1.15, 0.65, -45.0),
        # le masséter : la joue ronde, éclairée en haut
        ("masseter", HL(7.2, -22.0), 6.6, [(0.0, "#3E4850", 0.8), (0.6, "#2E363C", 0.3), (1.0, "#2E363C", 0.0)],
         1.0, 1.2, 30.0),
        ("museau", HL(17.2, -23.2), 4.6, [(0.0, "#46515A", 0.55), (1.0, FACE_LIGHT, 0.0)], 0.9, 1.3, 30.0),
        # sous l'œil, le creux entre la pommette et le nez
        ("creux", HL(12.6, -20.6), 4.2, [(0.0, "#050607", 0.35), (1.0, "#050607", 0.0)], 1.4, 0.7, -50.0),
        ("ganache", HL(8.8, -31.6), 9.0, [(0.0, "#040506", 0.88), (0.5, "#040506", 0.4), (1.0, "#040506", 0.0)],
         1.7, 0.42, -6.0),
        ("gorge", HL(2.5, -27.0), 6.5, [(0.0, "#040506", 0.6), (1.0, "#040506", 0.0)], 1.0, 1.3, 30.0),
        ("orbite", HL(6.9, -10.55), 3.0, [(0.0, "#030405", 0.45), (0.7, "#030405", 0.14), (1.0, "#030405", 0.0)],
         1.3, 0.85, -40.0),
        ("sous-oreille", HL(-3.0, -12.0), 7.0, [(0.0, "#050607", 0.55), (1.0, "#050607", 0.0)], 0.8, 1.5, 20.0),
        ("levres", HL(16.6, -30.0), 3.8, [(0.0, "#050607", 0.45), (1.0, "#050607", 0.0)], 1.6, 0.6, -10.0),
    ]
    for gid, c, r, stops, sx, sy, rot in soft:
        paints.append((D.rad(gid, c, r, stops, sx=sx, sy=sy, rot=rot + math.degrees(HEAD_LIFT)), 1.0, None))
    # le reflet du ciel le long du profil, du front au bout du nez : une bande
    # dans la tête, plus claire au bord, rognée au contour (pas de halo)
    rim = [HL(dx, dh) for (dx, dh) in [(0.5, 0.95), (5.0, 1.0), (9.0, 0.35), (11.6, -0.85), (13.8, -2.85),
                                       (15.4, -5.35), (16.5, -8.05), (17.4, -11.15), (18.2, -14.55),
                                       (18.9, -18.05), (19.5, -21.55), (19.95, -24.35)]]
    line = LineString(rim)
    for width, col, op in ((1.9, "#3A444B", 0.45), (0.9, "#4E5961", 0.4), (0.35, "#66727B", 0.45)):
        paints.append((col, op, line.buffer(width, 16).intersection(head_shape.buffer(-0.05))))
    # la crête de la face, sous l'œil, vers le museau : un pli de lumière à peine marqué
    crest = LineString(HLs([(9.2, -15.0), (10.9, -16.4), (12.3, -17.9)])).buffer(0.5, 8)
    paints.append(("#323B43", 0.25, crest))
    for fill, op, sh in paints:
        D.el(d if sh is None else geom_d(sh, simplify=0.02), fill=fill, fo=op)
    return paints, head_shape


def face_details(head_shape):
    """Le poil du dessus du crâne, le poil ras de la face, le mufle et la
    bouche, posés après l'œil et son pourtour."""
    # le dessus du crâne : un poil plus épais, un peu crépu, à peine bruni
    crown = HLs([(-6.0, -0.9), (-3.0, 0.35), (0.0, 0.85), (3.0, 0.95), (6.0, 0.8), (7.6, 0.45), (6.6, -1.7),
                 (4.2, -3.0), (1.0, -3.6), (-2.0, -4.0), (-5.0, -3.6), (-6.3, -2.4)])
    D.el(cr(crown, closed=True, t=0.8), fill=D.rad("crane", HL(1.0, -0.4), 7.0,
                                                    [(0.0, CROWN_LIGHT, 0.5), (0.6, CROWN, 0.3), (1.0, CROWN, 0.0)]))
    D.el(cr(crown, closed=True, t=0.8), fill=D.rad("crane-roux", HL(-2.5, -0.2), 5.0,
                                                    [(0.0, CROWN_RUST, 0.3), (1.0, CROWN_RUST, 0.0)]))
    r2 = random.Random(1859)
    m = Marks()
    for (x, h) in poisson(shape(crown).buffer(-0.3), 0.42, r2):
        a0 = r2.uniform(0, 6.28)
        rr = r2.uniform(0.16, 0.26)
        q = [(x + rr * math.cos(a0 + k * 1.1), h + rr * math.sin(a0 + k * 1.1)) for k in range(4)]
        m.add(r2.choice(("#4A4B4F", "#3C3D41", "#4E4538")), 0.07, 0.5, cr(q, closed=False))
    m.emit(D)
    face_hair(head_shape)
    nose()
    D.el(cr(HLs(MOUTH), closed=False), stroke="#040506", sw=0.24, so=0.9, cap="round")


def face_hair(head_shape):
    """Le poil ras de la face, fin et velouté, couché : du front vers le nez
    sur le chanfrein, vers l'arrière et le bas sur la joue ; de petits éclats
    clairs là où il prend le ciel, presque rien dans l'ombre."""
    r2 = random.Random(1797)
    m = Marks()
    eye = shape([eye_pt(ex * 1.25, ey * 1.35) for (ex, ey) in ALMOND])
    nose_zone = shape(HLs([(16.6, -23.0), (21.0, -23.0), (21.0, -31.0), (16.6, -31.0)]))
    zone = head_shape.buffer(-0.35).difference(eye).difference(nose_zone)
    for (x, h) in poisson(zone, 0.42, r2):
        # le repère de la tête
        lx = (x - HEAD_O[0]) * math.cos(HEAD_LIFT) + (h - HEAD_O[1]) * math.sin(HEAD_LIFT)
        lh = -(x - HEAD_O[0]) * math.sin(HEAD_LIFT) + (h - HEAD_O[1]) * math.cos(HEAD_LIFT)
        front = smooth(10.0, 15.0, lx) * smooth(-14.0, -4.0, lh) + smooth(14.0, 18.0, lx)
        ang_l = (1.0 - front) * math.radians(-118.0) + front * math.radians(-78.0)
        ang_l += r2.uniform(-0.22, 0.22)
        dx, dh = head_dir(math.cos(ang_l), math.sin(ang_l))
        L = r2.uniform(0.25, 0.5)
        pts = [(x, h), (x + dx * L, h + dh * L)]
        # la lumière : le front, le chanfrein, l'arcade, le haut de la joue
        prof = smooth(3.0, 0.4, abs(lh - (0.5 if lx < 9 else -1.5 - 1.25 * (lx - 9))) * 0.6)
        lit = clamp(0.75 * smooth(-10.0, -1.0, lh) * smooth(2.0, 9.0, lx) + 0.6 * smooth(13.0, 17.5, lx) *
                    smooth(-24.0, -10.0, lh) + 0.5 * math.exp(-((lx - 9.6) ** 2 + (lh + 16.0) ** 2) / 10.0) +
                    0.3 * prof)
        if r2.random() < 0.15 + 0.7 * lit:
            m.add("#66727C" if lit > 0.55 else "#46515A", 0.05, round(0.15 + 0.35 * lit, 1), line_d(pts))
        elif r2.random() < 0.3:
            m.add("#030405", 0.06, 0.4, line_d(pts))
    m.emit(D)


def nose():
    """Le mufle noir, nu, finement grenu, un peu luisant ; le naseau en
    virgule oblique qui remonte vers l'arrière ; la lèvre fendue."""
    mufle = HLs([(17.7, -23.7), (19.15, -23.5), (19.95, -24.4), (20.15, -26.3), (20.1, -27.7), (19.8, -28.8),
                 (19.25, -29.7), (17.9, -29.75), (17.1, -27.9), (17.0, -25.4)])
    g = D.rad("mufle", HL(19.2, -25.3), 3.8, [(0.0, MUFLE_WET, 1.0), (0.4, MUFLE, 1.0), (1.0, MUFLE_DEEP, 1.0)],
              f=HL(19.6, -24.6))
    D.el(cr(mufle, closed=True, t=0.85), fill=g, fo=0.85)
    r2 = random.Random(42)
    m = Marks()
    for (x, h) in poisson(shape(mufle).buffer(-0.25), 0.3, r2):
        m.add("#62676B", 0.05, 0.35, line_d([(x, h), (x + 0.1, h - 0.05)]))
    m.emit(D)
    # le naseau : une virgule qui remonte vers l'arrière
    nar = HLs([(19.75, -26.45), (19.5, -27.2), (18.95, -27.3), (18.45, -26.8), (18.05, -26.0), (17.75, -25.15),
               (18.25, -25.4), (18.85, -25.95)])
    D.el(cr(nar, closed=True, t=0.9), fill=NASEAU)
    D.el(cr(HLs([(18.0, -25.0), (18.55, -25.5), (19.2, -25.85), (19.75, -26.15)]), closed=False), stroke="#5C6166",
         sw=0.16, so=0.5, cap="round")
    # la fente de la lèvre (le philtrum), du naseau à la bouche
    D.el(cr(HLs([(19.85, -27.7), (19.6, -28.75), (19.2, -29.65)]), closed=False), stroke="#060708", sw=0.15,
         so=0.85, cap="round")
    # le reflet humide sur le bout du nez
    D.el(cr(HLs([(19.75, -23.9), (20.05, -24.9), (20.12, -25.8)]), closed=False), stroke="#80868C", sw=0.28,
         so=0.5, cap="round")
    D.el(cr(HLs([(19.95, -24.3), (20.08, -24.9)]), closed=False), stroke="#C9CED2", sw=0.12, so=0.45, cap="round")


def jaw():
    """La mâchoire : le bas du museau (lèvre inférieure, menton), et au-dessus,
    cachés bouche fermée, l'intérieur de la bouche, la langue, les incisives
    du bas."""
    D.open("jaw")
    D.el(cr(HLs([(13.0, -29.4), (16.5, -29.1), (18.9, -29.1), (19.3, -30.2), (17.0, -31.1), (13.0, -31.2)]),
            closed=True, t=0.8), fill="#24161A")
    D.el(cr(HLs([(14.4, -30.15), (16.3, -29.9), (18.1, -29.9), (18.5, -30.35), (16.6, -30.7), (14.6, -30.7)]),
            closed=True, t=0.85), fill="#7A4B52")
    D.el(cr(HLs([(15.1, -30.25), (16.6, -30.1), (17.9, -30.12)]), closed=False), stroke="#9E6C70", sw=0.13, so=0.7,
         cap="round")
    # les incisives du bas (en haut, le mouton n'a qu'un bourrelet)
    D.el(cr(HLs([(18.0, -29.7), (18.95, -29.65), (19.05, -30.1), (18.1, -30.2)]), closed=True, t=0.6),
         fill="#D9D0BC")
    jaw_pts = HLs([(13.4, -30.25), (15.2, -30.1), (17.0, -30.25), (18.3, -30.1), (19.25, -29.75), (19.45, -30.35),
                   (19.1, -31.0), (18.2, -31.5), (16.9, -31.65), (15.5, -31.55), (14.2, -31.35), (13.3, -31.1)])
    D.el(cr(jaw_pts, closed=True, t=0.85), fill=D.lin("menton", HL(16.0, -29.9), HL(15.8, -31.8),
                                                      [(0.0, "#222A2E"), (0.55, "#14181B"), (1.0, "#08090B")]))
    D.el(cr(HLs([(15.2, -30.3), (17.0, -30.35), (18.4, -30.2), (19.15, -29.9)]), closed=False), stroke="#3A4248",
         sw=0.16, so=0.45, cap="round")
    D.pivot("jaw", *HL(*JAW_PIVOT_L))
    D.close()


# ------------------------------------------------------------------ l'œil

EYE_ROT = math.radians(-28.0)    # la fente des paupières : le coin avant plus bas


def eye_pt(ex, ey):
    """Repère de l'œil (cm ; ex vers l'avant le long de la fente, ey vers le
    haut) vers le monde."""
    E = HL(*EYE_L)
    c, s = math.cos(EYE_ROT + HEAD_LIFT), math.sin(EYE_ROT + HEAD_LIFT)
    return (E[0] + ex * c - ey * s, E[1] + ex * s + ey * c)


ALMOND = [(-1.75, 0.05), (-1.25, 0.62), (-0.35, 0.92), (0.6, 0.86), (1.4, 0.48), (1.82, -0.05), (1.3, -0.55),
          (0.3, -0.78), (-0.75, -0.66), (-1.45, -0.35)]


def eyes():
    """L'œil : vif et plein (§ 5), l'iris brun ambré, la pupille en fente
    horizontale ; les paupières et les cils noirs, sans cercle pâle."""
    E = HL(*EYE_L)
    D.open("eyes")
    almond = [eye_pt(*p) for p in ALMOND]
    D.el(cr(almond, t=0.9), fill="#16100D")
    D.open("pupils")
    ic = eye_pt(-0.05, 0.02)
    D.el(cr(ellipse_pts(ic[0], ic[1], 1.12, 1.12, 0, 18)),
         fill=D.rad("iris", ic, 1.12, [(0.0, "#8C6640"), (0.32, IRIS_LIGHT), (0.68, IRIS), (0.9, IRIS_DEEP),
                                       (1.0, "#2A2018")], f=(ic[0] - 0.15, ic[1] + 0.35)))
    for k in range(14):
        a0 = 2 * math.pi * k / 14 + 0.15
        D.el(cr([(ic[0] + 0.45 * math.cos(a0), ic[1] + 0.45 * math.sin(a0)),
                 (ic[0] + 1.0 * math.cos(a0 + 0.1), ic[1] + 1.0 * math.sin(a0 + 0.1))], closed=False),
             stroke="#4A3524", sw=0.05, so=0.5, cap="round")
    pw, ph = 1.3, 0.42
    X, Y = S(ic[0] - pw / 2, ic[1] + ph / 2)
    D.raw(f'<rect x="{X:.2f}" y="{Y:.2f}" width="{pw:.2f}" height="{ph:.2f}" rx="{ph / 2:.2f}" fill="{PUPIL}"/>')
    D.pivot("pupils", *ic)
    D.close()
    # le reflet du ciel, et un petit, plus bas
    c1 = eye_pt(-0.55, 0.38)
    D.el(cr(ellipse_pts(c1[0], c1[1], 0.3, 0.2, 0.4, 10)), fill=CATCH, fo=0.9)
    c2 = eye_pt(0.75, -0.35)
    D.el(cr(ellipse_pts(c2[0], c2[1], 0.12, 0.08, 0.4, 8)), fill=CATCH, fo=0.35)
    # le bord des paupières, noir, et l'ombre de la paupière du haut sur l'œil
    D.el(cr([eye_pt(*p) for p in ALMOND[:7]], closed=False), stroke="#0A0B0C", sw=0.38, cap="round")
    D.el(cr([eye_pt(-1.5, 0.25), eye_pt(-0.4, 0.62), eye_pt(0.7, 0.58), eye_pt(1.55, 0.22)], closed=False),
         stroke="#000000", sw=0.35, so=0.35, cap="round")
    D.el(cr([eye_pt(*p) for p in ALMOND[5:] + ALMOND[:1]], closed=False), stroke="#0D0F10", sw=0.24, so=0.9,
         cap="round")
    D.pivot("eyes", *E)
    D.close()


def eye_cover(paints):
    """Le pourtour de l'œil, immobile : il cache l'iris quand il glisse (le
    regard le décale jusqu'à 2,6 cm) ; il reprend à l'identique les
    peintures du crâne, si bien qu'on ne le voit pas. Puis la paupière du
    haut, lourde, son pli et ses cils noirs."""
    outer = [eye_pt(4.2 * math.cos(2 * math.pi * k / 20), 3.6 * math.sin(2 * math.pi * k / 20)) for k in range(20)]
    win = [eye_pt(ex * 1.04, ey * 1.07) for (ex, ey) in ALMOND]
    ring = shape(outer).difference(shape(win))
    d = cr(outer, t=0.9) + cr(win[::-1], t=0.9)
    for fill, op, sh in paints:
        if sh is None:
            D.el(d, fill=fill, fo=op, rule="evenodd")
        else:
            part = sh.intersection(ring)
            if not part.is_empty:
                D.el(geom_d(part, simplify=0.02), fill=fill, fo=op)
    # le pli de la paupière du haut, l'ombre douce sous l'arcade
    D.el(cr([eye_pt(-1.6, 1.05), eye_pt(-0.3, 1.45), eye_pt(1.0, 1.35), eye_pt(1.9, 0.75)], closed=False),
         stroke="#030405", sw=0.26, so=0.7, cap="round")
    D.el(cr([eye_pt(-1.9, 1.9), eye_pt(-0.2, 2.5), eye_pt(1.6, 2.3), eye_pt(2.9, 1.4)], closed=False),
         stroke=FACE_LIGHT, sw=1.1, so=0.14, cap="round")
    # les cils du haut, noirs
    for k in range(7):
        u = (k + 0.5) / 7
        p = eye_pt(-1.4 + 3.0 * u, 0.62 + 0.3 * math.sin(math.pi * u))
        q = eye_pt(-1.4 + 3.0 * u + 0.45, 1.05 + 0.32 * math.sin(math.pi * u))
        D.el(cr([p, q], closed=False), stroke=LID, sw=0.09, so=0.95, cap="round")


# ------------------------------------------------------------------ le col de laine

def collar(cracks):
    """Le col de laine blanche qui encadre la tête : une limite nette, presque
    coupée au ciseau, un peu frangée, en oblique derrière l'oreille et la
    mâchoire (§ 3.2) ; la laine se pose un peu sur le noir. Le col suit la
    tête ; au repos, il prolonge l'encolure sans jointure (mêmes crevasses,
    mêmes dégradés)."""
    edge = HLs(COLLAR)
    r2 = random.Random(1887)
    noise = Noise1D(1887, [(2.4, 0.35), (1.1, 0.15)])
    back = []
    for i, (dx, dh) in enumerate(COLLAR):
        w = 3.6 + noise(i * 1.7)
        back.append(HL(dx - w * 0.9, dh - w * 0.35))
    band = shape(dense(edge, False, 4) + dense(back[::-1], False, 4))
    tufts = []
    for p in resample(edge, 0.62)[1:-1]:
        if r2.random() < 0.3:
            continue
        tufts.append(Point(p).buffer(r2.uniform(0.3, 0.75), 6))
    # étroite : quand la tête se relève ou s'abaisse, le col tourne avec elle
    # sans déborder de l'encolure (c'est l'encolure qui porte la laine)
    head_shape = shape(dense_runs(head_outline_runs(), per=6))
    band = union(band.intersection(head_shape.buffer(1.2)), *tufts)
    d = geom_d(band, simplify=0.02)
    g = fleece_paints()
    D.el(d, fill=g["base"])
    for key in ("gigot", "epaule", "crete", "sangle", "grasset", "bouts", "dessous", "gorge"):
        D.el(d, fill=g[key])
    # l'ombre de la tête et de l'oreille sur le col, sous la mâchoire : elle
    # s'éteint avant le bord du col
    D.el(d, fill=D.rad("col-ombre", HL(3.2, -27.0), 4.2, [(0.0, "#4E4838", 0.32), (0.6, "#4E4838", 0.12),
                                                           (1.0, "#4E4838", 0.0)], sx=0.8, sy=1.4, rot=40.0))
    T.draw_cracks(D, band, cracks, light_body, seed=402)
    T.draw_grain(D, band, light_body, seed=401, flow=flow)
    # le bord : des pointes de laine claires posées sur le noir
    m = Marks()
    for p in resample(edge, 0.5)[1:-1]:
        if r2.random() < 0.35:
            continue
        x, h = p
        a = r2.uniform(0, 6.28)
        q = [(x + 0.28 * math.cos(a), h + 0.28 * math.sin(a)), (x, h),
             (x + 0.28 * math.cos(a + 2.0), h + 0.28 * math.sin(a + 2.0))]
        m.add("#E7E0D0", 0.1, 0.65, cr(q, closed=False))
    m.emit(D)


# ------------------------------------------------------------------ les oreilles

def ear(name, far):
    """Une longue oreille noire, fine, en cloche : étroite à la base, large aux
    deux tiers, le bout en pointe mousse un peu tourné vers l'avant (on en
    voit l'intérieur), le bord un peu ourlé ; elle pend sous l'horizontale,
    derrière l'œil, le long de la joue (§ 5 ; P3, P5, P11)."""
    D.open(name)
    base = HL(*EAR_L) if not far else HL(EAR_L[0] + 1.6, EAR_L[1] + 0.6)
    ang = math.radians(-118.0 if not far else -112.0)     # 62° sous l'horizontale, vers l'arrière
    k = OREILLE / 15.5 if not far else 0.9

    u = (math.cos(ang), math.sin(ang))                    # le long de l'oreille (monde)
    v = (-u[1], u[0])                                     # en travers, vers l'avant (le côté de la face)
    if v[0] < 0:
        v = (-v[0], -v[1])
    w = OREILLE_L / 7.3

    def P(a, b):
        return (base[0] + a * k * u[0] + b * k * w * v[0], base[1] + a * k * u[1] + b * k * w * v[1])

    front = [(0.0, 1.4), (1.6, 2.1), (3.8, 2.85), (6.4, 3.45), (9.0, 3.75), (11.3, 3.6), (13.2, 2.85),
             (14.6, 1.6), (15.4, 0.2)]
    back = [(15.1, -1.0), (14.1, -2.3), (12.3, -3.2), (9.9, -3.55), (7.4, -3.2), (4.8, -2.5), (2.3, -1.85),
            (0.0, -1.5), (-1.3, 0.0)]
    outline = [P(*p) for p in front + back]
    if far:
        D.el(cr(outline, t=0.9), fill=D.lin("oreille-fond", P(2.0, 0), P(14.0, 0),
                                            [(0.0, "#1A1F23"), (1.0, "#0B0D0F")]))
        D.pivot(name, *base)
        D.close()
        return
    # l'ombre de l'oreille sur le col et la joue
    D.el(cr([(x + 0.35, y - 0.55) for (x, y) in outline], t=0.9), fill="#000000", fo=0.25)
    D.el(cr(outline, t=0.9), fill=D.lin("oreille", P(1.0, 0.0), P(15.0, 0.0),
                                        [(0.0, "#2C3339"), (0.45, EAR), (1.0, "#101215")]))
    # la lumière du ciel sur le haut de la feuille et le long de son bord arrière
    D.el(cr(outline, t=0.9), fill=D.lin("oreille-reflet", P(7.0, -3.6), P(7.0, 2.0),
                                        [(0.0, EAR_LIGHT, 0.75), (0.45, EAR_LIGHT, 0.2), (1.0, EAR_LIGHT, 0.0)]))
    # le creux de la cloche, le long du milieu, et sa lèvre éclairée
    D.el(cr([P(1.2, 0.5), P(4.5, 0.75), P(8.5, 0.6), P(12.0, 0.3)], closed=False), stroke="#07090B", sw=1.2,
         so=0.25, cap="round")
    D.el(cr([P(1.6, -0.6), P(5.0, -0.6), P(9.0, -0.85), P(12.6, -0.7)], closed=False), stroke="#4C555E", sw=0.5,
         so=0.22, cap="round")
    # l'intérieur, vu près du bout (le bout tourne un peu vers l'avant)
    inner = [P(8.0, 3.35), P(10.0, 3.55), P(11.9, 3.35), P(13.5, 2.6), P(14.7, 1.4), P(15.2, 0.3), P(14.3, 0.95),
             P(12.6, 1.9), P(10.5, 2.5), P(8.6, 2.85)]
    D.el(cr(inner, t=0.9), fill=D.lin("oreille-dedans", P(11.0, 3.6), P(11.0, 1.2),
                                      [(0.0, EAR_IN_LIGHT), (0.5, EAR_IN), (1.0, EAR_IN_DEEP)]))
    m = Marks()
    for i in range(10):
        t = 8.6 + 0.65 * i
        m.add("#7A6853", 0.05, 0.55, line_d([P(t, 3.1), P(t + 0.5, 2.4)]))
    m.emit(D)
    # le bord ourlé, derrière : un liseré clair, l'ombre du pli dedans
    D.el(cr([P(0.6, -1.45), P(2.6, -1.8), P(5.0, -2.4), P(7.6, -3.05), P(10.0, -3.35), P(12.3, -3.0), P(14.0, -2.15)],
            closed=False), stroke="#69737D", sw=0.18, so=0.5, cap="round")
    D.el(cr([P(2.6, -1.35), P(5.0, -1.95), P(7.6, -2.55), P(10.0, -2.85), P(12.2, -2.5)], closed=False),
         stroke="#06080A", sw=0.3, so=0.3, cap="round")
    # le poil ras, dans le sens de l'oreille
    r2 = random.Random(29)
    m = Marks()
    for _ in range(70):
        a = r2.uniform(0.8, 14.0)
        b = r2.uniform(-3.0, 3.0) * math.sin(math.pi * min(1.0, (a + 1.0) / 16.0))
        p, q = P(a, b), P(a + r2.uniform(0.4, 0.75), b)
        m.add("#4A535C" if b < 0 else "#0B0D0F", 0.05, 0.4, line_d([p, q]))
    m.emit(D)
    D.pivot(name, *base)
    D.close()


# ================================================================== la lèvre

# La lèvre supérieure (repère de la tête) : sous la narine, du bout du nez à
# la commissure, jusque sous la ligne de la bouche (15 % de la tête) ; elle
# se retrousse autour de son arrière pour le flehmen et découvre le
# bourrelet dentaire, rose sombre sous la peau noire.
LIP_L = [(15.0, -28.15), (16.5, -27.95), (17.9, -27.8), (19.3, -27.75), (20.5, -27.55), (20.6, -28.9),
         (19.85, -30.15), (18.3, -30.5), (16.8, -30.65), (15.2, -30.55), (14.6, -29.4)]
LIP_PIVOT_L = (15.2, -28.5)
GUM_L = [(15.6, -28.6), (16.9, -28.45), (18.3, -28.4), (19.35, -28.45), (19.5, -28.9), (18.75, -29.2),
         (17.3, -29.3), (15.9, -29.25)]
GUM_RIDGE_L = [(15.9, -28.95), (17.3, -28.85), (18.6, -28.8), (19.25, -28.75)]


# ================================================================== assemblage

def articule(svg):
    """Le dessin coupé à ses articulations (voir tools/races/articulations.py) :
    chaque patte au carpe ou au jarret (le haut garde un disque autour de
    l'articulation, le bas glisse dessous) ; la lèvre supérieure détachée de
    la tête, avec le bourrelet dentaire qu'elle cache."""

    def lip(root, paints):
        def w(pts):
            return [S(*HL(*p)) for p in pts]
        A.lift_part(root, paints, "head", w(LIP_L), "lip", S(*HL(*LIP_PIVOT_L)),
                    under=A.gum(w(GUM_L), w(GUM_RIDGE_L), GUM_FILL, GUM_CREASE, 0.22))

    return A.articulate_breed(svg, carpus_y=S(0, CARPE)[1], hock_y=S(0, JARRET_PIVOT)[1], extra=lip)


def main(path):
    region = near_wool_shapes()
    # Une repousse courte : les ouvertures restent de petits sillons souples,
    # noyés dans le grain, et non de grandes plaques de laine (photos P5, P17).
    cracks = T.Cracks(region, random.Random(1859), flow, depth_of, spacing=1.65, length=(2.5, 5.5))
    far_region = union(*(shape(dense(o[0] + o[1], True, 3)) for o in
                         (foreleg_outline(X_FF), hindleg_outline(X_HF))))
    far_cracks = T.Cracks(far_region, random.Random(1797), lambda x, h: 0.0, lambda x, h: 0.6, spacing=2.4)
    D.comment("Pattes : celles du fond d'abord, puis les proches ; toutes sous la toison")
    hindleg("leg-back-far", X_HF, False, far_cracks)
    foreleg("leg-front-far", X_FF, False, far_cracks)
    hindleg("leg-back-near", X_HN, True, cracks)
    foreleg("leg-front-near", X_FN, True, cracks)
    neck(cracks)
    body(cracks)
    tail(cracks)
    head(cracks)
    D.pivot("ground", (X_HN + X_FN) / 2 + 2.0, 0.0)
    # Le nez dépasse X = 154 : garder le museau entier aussi dans un lecteur
    # SVG qui découpe au viewBox, et pas seulement dans le moteur de rigs.
    W, H = 160, 110
    svg = (f'<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 {W} {H}" width="{W}" height="{H}">\n'
           "<!--\n"
           "  Suffolk : un bélier britannique de trois ans, en octobre, quatre mois et\n"
           "  demi après la tonte (toison de 3 à 4 cm), de profil, tourné vers la droite.\n"
           f"  1 unité = 1 cm de l'animal ({GARROT:.0f} cm au garrot). Tête et pattes noir de\n"
           "  jais, nues ; col de laine net derrière la mâchoire ; longues oreilles en\n"
           "  cloche ; toison serrée en croûte de pain ; queue coupée ; d'après\n"
           "  docs/races/suffolk.md ; écrit par tools/races/suffolk/gen_suffolk.py.\n"
           "  © 2026 Alejandro San Martin Lamas — MIT OR Apache-2.0\n"
           "-->\n"
           "<defs>" + "".join(D.defs) + "</defs>\n" + "\n".join(D.out) + "\n</svg>\n")
    svg = articule(svg)
    if VARIANTE:
        return variante(path, svg)
    with open(path, "w") as f:
        f.write(svg)
    print(f"{path} : {len(svg.encode()) / 1024:.0f} Ko, {svg.count('<path')} tracés")


def variante(path, svg):
    """Les autres dessins : ce qui change, puis les proportions."""
    root = ET.fromstring(svg)
    if VARIANTE == "brebis":
        variantes.mamelle(root, *variantes.sous_le_ventre(root, 12, -2), largeur=11, hauteur=7,
                  peau="#B8988C", ombre="#7C6058")
    if VARIANTE == "agneau":
        # La robe de naissance, brun noir chinée, avant que la laine blanche ne pousse.
        variantes.recolorer(root, variantes.rampe("#2A231F", "#8A7868", bas=.2, haut=.95),
                    dans=("body", "neck", "tail", "leg-back-near", "leg-back-far",
                          "leg-front-near", "leg-front-far"),
                    si=lambda c: variantes.clarte(c) > .35)
    variantes.ecrire(path, root, **PROPORTIONS[VARIANTE])


if __name__ == "__main__":
    main(sys.argv[1] if len(sys.argv) > 1 else "suffolk.svg")
