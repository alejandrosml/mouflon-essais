#!/usr/bin/env python3
"""Génère le rig SVG de la Thônes et Marthod : un bélier de quatre ans en
toison d'été (tondu fin avril, trois mois de pousse), de profil, tourné vers
la droite, d'après la fiche docs/races/thones-et-marthod.md.

Repère de travail : le « monde », en centimètres de l'animal réel. x vers
l'avant (0 à l'arrière de la culotte), h vers le haut (0 au sol). Dans le SVG,
1 unité = 1 cm : X = x + OX, Y = OY - h (voir outils.py).

Le dessin est d'abord écrit d'une pièce par patte, l'encolure à part sous le
corps (la variante A du contrat, comme le Mannequin), puis recoupé par
tools/races/articulations.py : chaque patte au carpe ou au jarret, la lèvre
supérieure détachée de la tête avec le bourrelet dentaire dessous.

Usage : gen_thones_et_marthod.py <sortie.svg>

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
import articulations as A  # noqa: E402
import variantes  # noqa: E402

# Les autres dessins de la race (tools/races/variantes.py), d'après la fiche.
# Sans variante : le bélier.
VARIANTE = sys.argv[2] if len(sys.argv) > 2 else ""
PROPORTIONS = {
    "brebis": dict(  # Brebis Thônes et Marthod : 65 cm au garrot, cornes plus fines d'un demi-tour à trois quarts de tour, tête et encolure plus fines.
        garrot=(76, 65), encolure=(1.0, .86), tete=(.96, .93)),
    "agneau": dict(  # Agneau Thônes et Marthod de deux semaines : 39 cm au garrot, laine blanche courte, lunettes et museau noirs, sans cornes.
        variantes.AGNEAU, garrot=(76, 39), corps=(.84, .8)),
}
from outils import (OY, S, Doc, cr, dense, dist_to, ellipse_pts, hspan, inside, mix, num, poly,  # noqa: E402
                    quad, ramp, ringlet, runs_path, shade, vspan)
from shapely.geometry import Point, Polygon  # noqa: E402

# ------------------------------------------------------------------ mesures (fiche, § 2)

GARROT = 76.0          # hauteur au garrot, peau (§ 2.1 et 2.3 : 76 cm [EST])
STERNUM = 39.0         # sous le sternum, peau (§ 2.3 : 38-40) ; la frange descend à 33
TETE = 31.0            # de la nuque au bout du nez (§ 2.3 : 30-31)
OREILLE = 15.0         # de la base à la pointe (§ 2.3 : 14-16), raccourcie de profil
CARPE = 23.0           # centre du carpe (§ 2.5 : 22-24)
JARRET = 26.5          # pointe du jarret (§ 2.5 : 25-28)
COURONNE = 3.5         # couronne de l'onglon (§ 2.3 : 3-4)
LAINE_AV = 28.6        # bord bas de la laine, antérieur (§ 2.3 : 27-30, au-dessus du genou)
LAINE_AR = 31.6        # bord bas de la laine, postérieur (§ 2.3 : 30-34, au-dessus du jarret)
QUEUE_BOUT = 30.0      # la mèche du bout de la queue arrive au jarret (§ 2.3 : queue de 30-34 cm)
CHAUSSETTE_AV = 8.8    # haut des chaussettes noires, au boulet : 0,10-0,12 H (§ 4.3 d)
CHAUSSETTE_AR = 9.8    # les postérieures un peu plus hautes (§ 4.3 d)
# La nuque : 1,29 H du sol (§ 2.4 : ≈ 1,30 H) et, mesuré sur P6, à 31 cm
# devant le garrot, en avant du poitrail : la tête se porte loin devant.
NUQUE = (96.0, 98.0)
# L'axe de la tête, de la nuque au bout du nez, sous l'horizontale (P6 :
# 35°) ; le chanfrein y fait 38° (§ 2.4 : ≈ 40°), la mâchoire est presque
# horizontale : de profil, la tête est un coin.
HEAD_TILT = math.radians(35.0)

# Les pattes (x du canon) : les antérieures sous le garrot, les postérieures
# sous la pointe de la fesse (photo P6) ; celles du fond décalées pour qu'on
# voie les quatre. Le canon fait 4 % de H (§ 7) : 3 cm.
FORE_NEAR, FORE_FAR = 72.0, 65.0
HIND_NEAR, HIND_FAR = 11.0, 18.0
# Les épaules et les hanches (pivots des pattes), dans le corps.
SHOULDER_H, HIP_H = 49.0, 55.0
# La base de l'encolure : devant la pointe de l'épaule, au bas de la base du
# cou ; d'ici, le museau atteint l'herbe en broutant.
NECK_BASE = (89.5, 58.5)

# ------------------------------------------------------------------ palette (fiche, § 0 et § 4.2)

FLEECE_LIGHT, FLEECE, FLEECE_SHADOW, FLEECE_DEEP = "#F5EFE2", "#E4DACA", "#B3A386", "#7A6A50"
HAIR_LIGHT, HAIR, HAIR_SHADOW, HAIR_DEEP = "#F8F6F0", "#E8E4DA", "#C8C5BC", "#877D70"
BLACK_SHEEN, BLACK, BLACK_DEEP = "#3E3B37", "#1A1918", "#0A0A0A"
NOSE_SHEEN, NOSE, NOSTRIL = "#5A5A5C", "#2A2A2B", "#0E0E0E"
HORN_LIGHT, HORN, HORN_SHADOW, HORN_STREAK, HORN_DEEP = "#C4AA91", "#9C8064", "#735637", "#4F3D2A", "#3F2C1A"
HORN_TIP, HORN_TIP_LIGHT = "#B49A78", "#CDB89A"
HOOF_LIGHT, HOOF, HOOF_DEEP = "#4D4A45", "#2A2826", "#121110"
IRIS_LIGHT, IRIS, LIMBUS, PUPIL = "#C9B48C", "#B09A6E", "#6F5636", "#24261C"
CATCH = "#F2EEE4"
GUM, GUM_DARK = "#B9898C", "#8A5C63"
# Le fond des creux entre les mèches (dégradé commun au corps, à l'encolure et
# aux mèches : au repos, aucune jointure ne se voit).
DEEP_TOP, DEEP_MID, DEEP_LOW = "#D2C5AA", "#AD9C7F", FLEECE_DEEP

D = Doc()

# ------------------------------------------------------------------ la toison

LOCK_STOPS = [(0.0, FLEECE_SHADOW), (0.55, FLEECE), (1.0, FLEECE_LIGHT)]


def wool(light):
    return ramp(light, LOCK_STOPS)


def bucket(light, n=10):
    """La lumière ramenée à quelques niveaux : les mèches d'un même niveau
    partagent la teinte de leurs spires (un seul tracé par teinte)."""
    return round(max(0.0, min(1.0, light)) * (n - 1)) / (n - 1)


class Crimps:
    """Une couche de mèches : leurs contours, remplis dans l'ordre, puis
    leurs spires (le creux d'ombre sous chaque tour, le reflet sur son
    dessus), rangées par teinte et tracées d'un seul trait par teinte. On ne
    garde que les spires qu'aucune mèche posée après dans la couche ne
    recouvre : les tracer toutes à la fin revient alors au même que de les
    tracer mèche par mèche, pour bien moins d'éléments."""

    def __init__(self):
        self.locks = []

    def add(self, fill, outline, base, darks, lights):
        self.locks.append((fill, outline, base, darks, lights))

    def emit(self, sw=0.2, so=0.5, lsw=0.2, lso=0.85):
        cell = 2.5
        grid = {}
        boxes = []
        for i, (_, out, _, _, _) in enumerate(self.locks):
            xs = [p[0] for p in out]
            hs = [p[1] for p in out]
            box = (min(xs), min(hs), max(xs), max(hs))
            boxes.append(box)
            for gx in range(int(box[0] // cell), int(box[2] // cell) + 1):
                for gy in range(int(box[1] // cell), int(box[3] // cell) + 1):
                    grid.setdefault((gx, gy), []).append(i)

        def covered(i, p):
            for j in grid.get((int(p[0] // cell), int(p[1] // cell)), ()):
                if j <= i:
                    continue
                bx = boxes[j]
                if bx[0] <= p[0] <= bx[2] and bx[1] <= p[1] <= bx[3] and inside(self.locks[j][1], p[0], p[1]):
                    return True
            return False

        def visible(i, a, c, b):
            m = (0.25 * a[0] + 0.5 * c[0] + 0.25 * b[0], 0.25 * a[1] + 0.5 * c[1] + 0.25 * b[1])
            return not (covered(i, m) or covered(i, a) or covered(i, b))

        dark, light = {}, {}
        for (fill, out, _, _, _) in self.locks:
            D.el(cr(out, nd=1, t=0.85), fill=fill)
        for i, (_, _, base, darks, lights) in enumerate(self.locks):
            dk = shade(base, 0.72)
            lt = mix(base, "#FFFFFF", 0.78)
            for (_, a, c, b) in darks:
                if visible(i, a, c, b):
                    dark.setdefault(dk, []).append(quad(a, c, b))
            for (_, a, c, b) in lights:
                if visible(i, a, c, b):
                    light.setdefault(lt, []).append(quad(a, c, b))
        for col in sorted(dark):
            D.el("".join(dark[col]), stroke=col, sw=sw, so=so, cap="round")
        for col in sorted(light):
            D.el("".join(light[col]), stroke=col, sw=lsw, so=lso, cap="round")
        self.locks = []


def lock(x, h, L, w, ang, light, rng, crimps, visible=None, pitch=None, bend=None, hook=None, dim=1.0):
    """Une mèche en tire-bouchon, posée dans la couche `crimps` (dessinée à
    son `emit`, qui calcule elle-même ce que recouvrent les mèches suivantes ;
    `visible` n'est plus lu, gardé pour dire au lecteur quelle part de la
    mèche dépasse de la rangée suivante)."""
    pitch = pitch or rng.uniform(1.0, 1.25)
    bend = rng.uniform(-0.3, 0.3) if bend is None else bend
    hook = rng.choice((-1, 1)) * rng.uniform(0.4, 1.3) if hook is None else hook
    outline, darks, lights = ringlet(x, h, L, w, ang, rng, pitch=pitch, bend=bend, hook=hook)
    base = wool(bucket(light))
    if dim != 1.0:
        base = shade(base, dim)
    crimps.add(shade(base, 1.0 + rng.uniform(-0.025, 0.025)), outline, base, darks, lights)


def hex_roots(pts, dx, dh, rng, margin, jitter=0.3):
    """Les racines des mèches en quinconce (une rangée sur deux décalée d'une
    demi-mèche), un peu au hasard, à `margin` au moins du bord."""
    xs = [p[0] for p in pts]
    hs = [p[1] for p in pts]
    out = []
    row = 0
    h = max(hs) - margin
    while h > min(hs):
        x = min(xs) + (row % 2) * dx / 2
        while x < max(xs):
            px = x + rng.uniform(-jitter, jitter) * dx
            ph = h + rng.uniform(-jitter, jitter) * dh
            if inside(pts, px, ph) and dist_to(pts, px, ph) > margin:
                out.append((px, ph))
            x += dx
        h -= dh
        row += 1
    return out


def inset(pts, d, tol=0.12):
    """Le contour rentré de `d` (pour le fond des creux, sous les mèches)."""
    g = Polygon(pts).buffer(-d, join_style=1).simplify(tol)
    if g.geom_type != "Polygon":
        g = max(g.geoms, key=lambda q: q.area)
    return list(g.exterior.coords)[:-1]


def edge_locks(outline, step, rng, crimps, select, length, width, light_of, droop=0.5, root_in=1.4, dim=1.0):
    """Des mèches sur un bord : elles partent un peu en dedans du contour
    (parcouru dans le sens des aiguilles d'une montre) et pointent vers
    l'extérieur, retombant plus ou moins (`droop`)."""
    n = len(outline)
    acc = 0.0
    out = []
    for i in range(n):
        a, b = outline[i], outline[(i + 1) % n]
        acc += math.dist(a, b)
        if acc < step:
            continue
        acc = rng.uniform(-0.25, 0.25) * step
        x, h = b
        if not select(x, h):
            continue
        p, q = outline[(i - 1) % n], outline[(i + 2) % n]
        tx, th = q[0] - p[0], q[1] - p[1]
        L = math.hypot(tx, th) or 1.0
        nx, nh = -th / L, tx / L
        dx, dh = nx, nh - droop
        ang = math.atan2(dx, -dh) + rng.uniform(-0.3, 0.3)
        out.append((h, x - nx * root_in, h - nh * root_in, ang))
    out.sort()
    for (_, x0, h0, ang) in out:
        lock(x0, h0, rng.uniform(*length), rng.uniform(*width), ang, light_of(x0, h0), rng, crimps, dim=dim)


# ================================================================== le corps

# La silhouette de la toison du corps (sans l'encolure), dans le sens des
# aiguilles d'une montre depuis l'attache de la queue : croupe un peu
# tombante, dos droit, garrot, la racine du cou sur le garrot (P3 : une crête
# de laine), le devant qui recouvre la base de l'encolure, le poitrail, le
# ventre relevé vers l'arrière, la culotte sur le haut des cuisses.
BODY = [
    (4.0, 70.5), (7.5, 75.4), (12.5, 78.6), (19.0, 80.3), (27.0, 81.0), (37.0, 81.2), (47.0, 81.0),
    (57.0, 80.9), (63.0, 81.3), (67.5, 82.4), (71.5, 84.0), (75.0, 85.6),
    (78.2, 84.7), (81.6, 81.6), (85.0, 77.4), (88.2, 72.4), (90.6, 67.0), (92.0, 61.4), (92.4, 55.6),
    (91.8, 49.8), (90.2, 44.6), (87.6, 40.2), (84.0, 37.4), (79.5, 36.0), (74.0, 35.3), (67.0, 35.0),
    (59.0, 34.8), (50.0, 35.0), (42.5, 35.7), (36.5, 37.1), (31.5, 39.6), (27.5, 42.6), (23.5, 45.3),
    (18.5, 46.8), (12.5, 47.4), (7.0, 48.5), (3.3, 51.0), (1.0, 54.8), (0.0, 59.5), (0.5, 64.3), (1.9, 67.9),
]
BODY_D = dense(BODY, True, 8)


def staples(x, h):
    """Les paquets de mèches : une variation douce de la lumière, à l'échelle
    de quelques centimètres (la toison n'est pas un tapis uniforme)."""
    return 0.07 * math.sin(x / 5.3 + h / 3.7) * math.sin(h / 4.9 - x / 7.9 + 1.3) + 0.04 * math.sin(x / 2.9 - h / 6.1)


def packs(x, h):
    """L'inclinaison des paquets de mèches (radians) : un champ lisse, de 5 à
    8 cm, pour que les mèches voisines penchent ensemble et que les paquets
    voisins penchent autrement : la toison ne se lit plus en colonnes."""
    return 0.3 * math.sin(x / 4.3 + 1.6 * math.sin(h / 6.1)) + 0.2 * math.sin(h / 3.7 - x / 7.3 + 0.8)


def body_light(x, h):
    sp = vspan(BODY_D, x)
    hb, ht = sp if sp else (33.0, 81.0)
    tv = max(0.0, min(1.0, (h - hb) / max(1.0, ht - hb)))
    light = 0.12 + 0.86 * tv ** 0.8 + staples(x, h)
    if x > 83 and h < 66:
        light -= 0.14 * min(1.0, (x - 83) / 9.0)
    if x < 5:
        light -= 0.06
    return light


def fringe(x):
    """Ce que les mèches du bas dépassent sous la ligne du ventre : la frange,
    un peu plus longue au milieu du ventre (§ 3.3 : courte en été)."""
    if 38 < x < 84:
        return 1.3 + 1.5 * math.sin(math.pi * (x - 38) / 46)
    return 1.0


def body():
    rng = random.Random(1975)  # l'année où commence le sauvetage de la race (S2)
    D.lin("fond", (0, 82), (0, 32), [(0, DEEP_TOP), (0.5, DEEP_MID), (1, DEEP_LOW)])
    D.comment("Corps : la toison d'été, des mèches en tire-bouchons serrés de 5 à 7 cm")
    D.open("body")
    # le fond : les creux entre les mèches, un peu en retrait du bord (ce
    # sont les pointes des mèches qui font le contour)
    D.el(poly(inset(BODY_D, 1.0), nd=1), fill="url(#fond)")
    crimps = Crimps()
    dh = 2.1
    roots = hex_roots(BODY_D, 1.6, dh, rng, 0.55, jitter=0.42)
    locks = []
    for (x, h) in roots:
        hb = vspan(BODY_D, x)[0]
        L = rng.uniform(3.4, 5.0) * (1.0 + 0.12 * math.sin(x / 5.9 - h / 4.4))
        bottom = h - dh < hb + 0.7
        if bottom:
            L = max(L, h - hb + fringe(x) + rng.uniform(0.0, 0.7))
        ang = rng.uniform(-0.2, 0.2) + packs(x, h)
        if rng.random() < 0.06:            # une mèche qui s'écarte
            ang += rng.choice((-1, 1)) * rng.uniform(0.5, 0.9)
            L *= 1.2
        if x < 12:
            ang -= 0.5 * (12 - x) / 12
        if x > 81 and h < 68:
            ang += 0.42 * min(1.0, (x - 81) / 10)
        locks.append((h, x, L, ang, bottom))
    locks.sort()
    for (h, x, L, ang, bottom) in locks:
        light = body_light(x, h - L * 0.4) + rng.uniform(-0.07, 0.07)
        lock(x, h, L, rng.uniform(1.6, 2.0), ang, light, rng, crimps, visible=None if bottom else dh + 0.5,
             hook=rng.choice((-1, 1)) * rng.uniform(0.9, 1.7) if bottom else None)
    crimps.emit()
    # le bord : des bouts de mèches sur le dos, la croupe, la culotte, et sur
    # le devant qui recouvre l'encolure et le poitrail
    edge_locks(BODY_D, 1.15, rng, crimps,
               select=lambda x, h: (h > 60 and x < 77) or (x < 2.5 and h > 50),
               length=(2.6, 3.8), width=(1.55, 1.95), light_of=lambda x, h: body_light(x, h) + 0.04,
               droop=0.45, root_in=2.3)
    edge_locks(BODY_D, 1.5, rng, crimps,
               select=lambda x, h: x > 77 and h > 40,
               length=(2.8, 4.0), width=(1.45, 1.8), light_of=lambda x, h: body_light(x, h) - 0.03,
               droop=0.8, root_in=1.6)
    crimps.emit()
    # les jarres : quelques poils plus raides et plus brillants qui dépassent
    # du dos et de la croupe (S13 ; P16)
    jarre = []
    n = len(BODY_D)
    for i in range(0, n, 2):
        x, h = BODY_D[i]
        if not (5 < x < 76 and h > 74) or rng.random() < 0.55:
            continue
        a, b = BODY_D[(i - 2) % n], BODY_D[(i + 2) % n]
        tx, th = b[0] - a[0], b[1] - a[1]
        L = math.hypot(tx, th) or 1.0
        nx, nh = -th / L, tx / L
        ln = rng.uniform(1.8, 3.2)
        bnd = rng.uniform(-0.5, 0.5)
        pts = [(x - nx * 0.6, h - nh * 0.6), (x + nx * ln * 0.5 - nh * bnd * 0.3, h + nh * ln * 0.5 + nx * bnd * 0.3),
               (x + nx * ln - nh * bnd, h + nh * ln + nx * bnd)]
        jarre.append(cr(pts, closed=False, nd=1))
    D.el("".join(jarre), stroke="#FBF8F0", sw=0.07, so=0.75, cap="round")
    D.pivot("body", 50.0, 33.0)
    D.close()


# ================================================================== l'encolure

# L'encolure, à part, sous le corps (variante A) : la crête ; une gorge et un
# avant de poitrail pleins de laine, la gorge qui sort de sous la mâchoire à
# l'aplomb du masséter (P6) ; sous la tête, une épaisseur cachée au repos,
# qui garde l'encolure épaisse quand le moteur l'allonge (endormi, tête au
# sol) ; et sous le corps, tout le bas autour de sa base (il ne bouge pas
# quand elle tourne : aucun jour ne s'ouvre au poitrail).
NECK = [
    (78.6, 68.0), (78.2, 74.0), (79.0, 80.0), (80.8, 85.6), (83.6, 89.4), (86.8, 92.6), (90.0, 95.4),
    (93.2, 97.6), (97.0, 99.0), (101.5, 97.8), (104.4, 94.6), (105.9, 90.4), (106.6, 85.8), (106.5, 81.4),
    (105.8, 77.4), (104.7, 73.4), (103.2, 69.4), (101.3, 65.4), (99.3, 61.4), (97.4, 57.4), (95.6, 53.6),
    (93.0, 50.4), (88.5, 49.4), (84.5, 50.2), (81.2, 53.2), (79.4, 58.0), (78.6, 63.2),
]
NECK_D = dense(NECK, True, 6)


def neck_light(x, h):
    # la lumière du corps là où ils se raccordent ; la gorge, tournée vers
    # le bas, dans l'ombre
    t = max(0.0, min(1.0, (h - 58) / 40.0))
    light = 0.4 + 0.55 * t + staples(x, h)
    sp = hspan(NECK_D, h)
    if sp:
        light -= 0.24 * max(0.0, 1.0 - (sp[1] - x) / 6.0)
    return light


def neck():
    rng = random.Random(1932)  # l'union des deux souches (S2)
    D.comment("Encolure : sous le corps et sous la tête, qui la recouvrent à ses deux bouts")
    D.open("neck")
    D.el(poly(inset(NECK_D, 0.8), nd=1), fill="url(#fond)")
    crimps = Crimps()
    # des boucles serrées dans tous les sens : la peau de l'encolure tourne
    # avec elle (il broute la tête au sol), sans sens de « pendre » qui tienne
    pts = hex_roots(NECK_D, 1.55, 1.9, rng, 1.0, jitter=0.35)
    pts.sort(key=lambda p: (p[1], p[0]))
    for (x, h) in pts:
        ang = rng.uniform(-1.2, 1.2) + (0.45 if x > 90 else 0.0)
        lock(x, h, rng.uniform(2.6, 3.5), rng.uniform(1.5, 1.85), ang, neck_light(x, h) + rng.uniform(-0.07, 0.07),
             rng, crimps, visible=2.4)
    crimps.emit()
    # la crête (le bélier y porte une crête de laine, P3) et la gorge
    edge_locks(NECK_D, 1.45, rng, crimps,
               select=lambda x, h: (h > 83 and x < 94 and h < 98.0) or (x > 92 and 52 < h < 96.0),
               length=(2.5, 3.5), width=(1.45, 1.8), light_of=lambda x, h: neck_light(x, h) + 0.03,
               droop=0.25, root_in=1.5)
    crimps.emit()
    D.pivot("neck", *NECK_BASE)
    D.close()


# ================================================================== les pattes

def leg_paint(name, cx, far):
    """Le dégradé du poil ras blanc d'une patte : la lumière en haut à gauche,
    l'ombre sur le devant (à droite) et sur l'arrière."""
    k = 0.8 if far else 1.0
    return D.lin(f"poil-{name}", (cx - 2.6, 0), (cx + 2.8, 0),
                 [(0, shade(HAIR_SHADOW, k)), (0.22, shade(HAIR_LIGHT, k)), (0.55, shade(HAIR, k)),
                  (1, shade(HAIR_SHADOW, k * 0.94))])


def hair_strokes(edges, n, rng, color, so, seed_len=(0.8, 1.6)):
    """Le poil ras, en traits fins le long de la patte : `edges` donne, à
    chaque hauteur, l'arrière et l'avant de la patte."""
    out = []
    for _ in range(n):
        h = rng.uniform(edges[0][0], edges[-1][0])
        xb = xf = None
        for (h0, b0, f0), (h1, b1, f1) in zip(edges, edges[1:]):
            if h0 <= h <= h1:
                u = (h - h0) / (h1 - h0)
                xb, xf = b0 + (b1 - b0) * u, f0 + (f1 - f0) * u
                break
        if xb is None:
            continue
        x = xb + (xf - xb) * rng.uniform(0.1, 0.9)
        ln = rng.uniform(*seed_len)
        tilt = rng.uniform(-0.12, 0.12)
        out.append(cr([(x, h + ln / 2), (x + tilt * ln, h - ln / 2)], closed=False, nd=1))
    D.el("".join(out), stroke=color, sw=0.07, so=so, cap="round")


def fringe_hairs(x0, x1, h, rng, far):
    """Le bord du haut de la chaussette, presque droit, un peu effrangé : des
    poils noirs qui montent dans le blanc, des blancs qui descendent dans le
    noir (§ 4.3 d)."""
    k = 0.8 if far else 1.0
    up, down = [], []
    x = x0 + 0.1
    while x < x1:
        up.append(cr([(x, h - 0.25), (x + rng.uniform(-0.12, 0.12), h + rng.uniform(0.2, 0.55))], closed=False, nd=2))
        if rng.random() < 0.5:
            xx = x + 0.11
            down.append(cr([(xx, h + 0.12), (xx + rng.uniform(-0.1, 0.1), h - rng.uniform(0.2, 0.45))],
                           closed=False, nd=2))
        x += rng.uniform(0.2, 0.32)
    D.el("".join(up), stroke=BLACK, sw=0.1, so=0.85, cap="round")
    D.el("".join(down), stroke=shade(HAIR, k), sw=0.08, so=0.55, cap="round")


def hoof(heel, toe, far, name):
    """L'onglon de profil, petit et serré, noir à gris ardoise avec un léger
    reflet gris (§ 7) ; le bout de l'autre doigt dépasse un peu devant."""
    g = D.lin(f"onglon-{name}", (heel, 0), (toe, 0), [(0, HOOF_DEEP), (0.45, HOOF), (0.85, HOOF_LIGHT if not far else HOOF),
                                                       (1, HOOF)])
    top = COURONNE
    other = [(toe - 2.0, top - 0.4), (toe - 0.9, 1.9), (toe + 0.3, 0.06), (toe - 1.5, 0.06), (toe - 2.4, 1.6)]
    D.el(cr(other, t=0.7), fill=HOOF_DEEP)
    wall = [(toe - 1.9, top), (toe - 0.95, 2.1), (toe - 0.3, 0.85), (toe, 0.06)]
    sole = [(toe, 0.06), (heel + 0.35, 0.06)]
    back = [(heel + 0.35, 0.06), (heel, 0.55), (heel - 0.1, 1.5), (heel + 0.3, 2.5), (heel + 1.2, top + 0.1),
            (toe - 1.9, top)]
    D.el(runs_path([wall, sole, back], nd=2), fill=g)
    D.line([(toe - 1.6, top - 0.2), (toe - 0.8, 1.85), (toe - 0.28, 0.35)], HOOF_LIGHT if not far else "#3A3734", 0.15,
           so=0.7)
    D.line([(heel + 0.5, 2.7), (toe - 2.0, top - 0.05)], "#4A4641" if not far else "#33302D", 0.13, so=0.6)


def dewclaw(x, h, far):
    D.el(cr(ellipse_pts(x, h, 0.5, 0.38, 0.5, 8)), fill="#0D0C0B" if not far else "#080807")


def hidden_by_body(x, h, L, ang, depth=2.5):
    """Une mèche du haut d'une patte que le corps cache toujours : sa racine
    et sa pointe à plus de `depth` cm en dedans de la toison du corps. Près
    de l'épaule ou de la hanche, la patte tourne sans la faire sortir."""
    tip = (x + L * math.sin(ang), h - L * math.cos(ang))
    return all(inside(BODY_D, *p) and dist_to(BODY_D, *p) > depth for p in ((x, h), tip))


def wool_sleeve(name, outline, rng, light_of, far, length=(3.2, 4.4), dx=1.5, dh=2.1):
    """La laine du haut de la patte (avant-bras, cuisse et jambe), qui
    s'arrête en festons de mèches au-dessus du genou ou du jarret. Les
    mèches toujours cachées sous le corps ne sont pas dessinées (le fond
    reste, sous elles)."""
    pts = dense(outline, True, 6)
    D.el(poly(inset(pts, 0.7), nd=1), fill=f"url(#fond-{name})")
    crimps = Crimps()
    roots = hex_roots(pts, dx, dh, rng, 0.7)
    locks = []
    for (x, h) in roots:
        hb = vspan(pts, x)[0]
        L = rng.uniform(*length)
        bottom = h - dh < hb + 0.6
        if bottom:
            L = max(L, h - hb + rng.uniform(0.9, 1.8))
        locks.append((h, x, L, rng.uniform(-0.22, 0.22), bottom))
    locks.sort()
    for (h, x, L, ang, bottom) in locks:
        light = light_of(x, h - L * 0.5) + rng.uniform(-0.06, 0.06)
        w = rng.uniform(1.45, 1.8)
        if hidden_by_body(x, h, L, ang):
            continue
        lock(x, h, L, w, ang, light, rng, crimps, visible=None if bottom else dh + 0.5,
             hook=rng.choice((-1, 1)) * rng.uniform(0.9, 1.6) if bottom else None, dim=0.84 if far else 1.0)
    crimps.emit()


def foreleg(cx, near):
    name = "leg-front-near" if near else "leg-front-far"
    far = not near
    rng = random.Random(int(cx * 10) + 7)
    D.open(name)
    # le canon fin, le carpe un peu plus large (les os du genou devant, l'os
    # crochu derrière), le boulet, le paturon court incliné à 60° (§ 2.5)
    front = [(cx + 2.9, 50.0), (cx + 2.7, 40.0), (cx + 2.45, 31.0), (cx + 2.3, 27.0), (cx + 2.55, 25.2),
             (cx + 2.72, 23.6), (cx + 2.55, 21.6), (cx + 2.05, 19.8), (cx + 1.7, 17.6), (cx + 1.58, 14.5),
             (cx + 1.6, 11.6), (cx + 1.85, 10.0), (cx + 2.3, 8.8), (cx + 2.95, 7.0), (cx + 3.55, 5.3),
             (cx + 3.95, COURONNE + 0.2)]
    back = [(cx + 0.3, COURONNE + 0.3), (cx - 0.3, 4.9), (cx - 0.95, 6.5), (cx - 1.55, 8.2), (cx - 1.9, 9.9),
            (cx - 1.5, 12.0), (cx - 1.42, 15.0), (cx - 1.5, 19.0), (cx - 1.95, 21.4), (cx - 2.35, 22.8),
            (cx - 2.25, 24.4), (cx - 2.1, 26.2), (cx - 2.2, 30.0), (cx - 2.55, 40.0), (cx - 2.8, 50.0)]
    D.el(runs_path([front, [front[-1], back[0]], back + [(cx, 51.0), front[0]]], nd=2), fill=leg_paint(name, cx, far))
    edges = [(10.0, cx - 1.8, cx + 1.7), (14.0, cx - 1.4, cx + 1.55), (19.0, cx - 1.5, cx + 1.9),
             (21.6, cx - 1.95, cx + 2.5), (23.6, cx - 2.3, cx + 2.65), (25.4, cx - 2.2, cx + 2.5),
             (29.0, cx - 2.2, cx + 2.4)]
    k = 0.8 if far else 1.0
    hair_strokes(edges, 70, rng, shade("#FFFFFF", k), 0.55)
    hair_strokes(edges, 45, rng, shade("#B8B3A8", k), 0.5)
    # l'ombre derrière le canon et le genou ; la lumière sur le devant du
    # genou ; le pli sous les os du carpe
    D.line([(cx - 1.25, 10.5), (cx - 1.15, 15.0), (cx - 1.25, 19.0), (cx - 1.85, 21.8), (cx - 2.0, 24.5)],
           shade(HAIR_DEEP, k), 0.5, so=0.32)
    D.line([(cx + 1.35, 19.0), (cx + 2.05, 21.2), (cx + 2.35, 23.4), (cx + 2.1, 25.6)], shade("#FFFFFF", k), 0.35,
           so=0.55)
    D.line([(cx + 0.2, 21.2), (cx + 1.2, 21.3), (cx + 2.0, 21.0)], shade(HAIR_DEEP, k), 0.14, so=0.22)
    # la chaussette, l'ergot, l'onglon
    top = CHAUSSETTE_AV
    sfill = D.lin(f"chaussette-{name}", (cx - 2.5, 0), (cx + 4.5, 0),
                  [(0, shade(BLACK_SHEEN, k)), (0.3, BLACK), (1, BLACK_DEEP)])
    s_front = [(cx + 1.78, top + 0.3), (cx + 2.0, 9.4), (cx + 2.3, 8.8), (cx + 2.95, 7.0), (cx + 3.55, 5.3),
               (cx + 3.95, COURONNE + 0.2)]
    s_back = [(cx + 0.3, COURONNE + 0.3), (cx - 0.3, 4.9), (cx - 0.95, 6.5), (cx - 1.55, 8.2), (cx - 1.88, 9.3),
              (cx - 1.85, top + 0.2)]
    s_top = [(cx - 1.85, top + 0.2), (cx - 1.0, top + 0.4), (cx - 0.1, top + 0.12), (cx + 0.8, top + 0.45),
             (cx + 1.78, top + 0.3)]
    D.el(runs_path([s_front, [s_front[-1], s_back[0]], s_back, s_top], nd=2), fill=sfill)
    fringe_hairs(cx - 1.8, cx + 1.75, top + 0.3, rng, far)
    D.line([(cx - 1.2, 8.6), (cx - 0.6, 6.6), (cx + 0.1, 5.0)], shade(BLACK_SHEEN, 1.25 * k), 0.3, so=0.6)
    dewclaw(cx - 1.5, 7.2, far)
    hoof(cx + 0.05, cx + 5.5, far, name)
    # la laine de l'avant-bras, en festons au-dessus du genou
    sleeve = [(cx - 4.4, 50.0), (cx - 4.2, 42.0), (cx - 3.8, 35.5), (cx - 3.3, LAINE_AV + 1.6),
              (cx - 1.5, LAINE_AV + 0.4), (cx + 1.0, LAINE_AV + 0.6), (cx + 3.1, LAINE_AV + 1.4),
              (cx + 3.7, 35.0), (cx + 4.1, 42.0), (cx + 4.4, 50.0), (cx, 52.0)]
    kk = 0.84 if far else 1.0
    D.lin(f"fond-{name}", (0, 50), (0, LAINE_AV), [(0, shade(DEEP_MID, kk)), (1, shade(DEEP_LOW, kk))])
    wool_sleeve(name, sleeve, rng, lambda x, h: 0.3 + 0.03 * (h - 30) / 5, far)
    D.pivot(name, cx + 0.5, SHOULDER_H)
    D.close()


def hindleg(cx, near):
    name = "leg-back-near" if near else "leg-back-far"
    far = not near
    rng = random.Random(int(cx * 10) + 3)
    D.open(name)
    # la jambe qui remonte en avant vers le grasset, le jarret ouvert à
    # 145-150° (§ 2.5) et sa pointe nette, le canon vertical et long, le
    # paturon à 57°
    front = [(cx + 10.0, 52.0), (cx + 7.4, 42.0), (cx + 5.4, 35.0), (cx + 3.5, 30.4), (cx + 2.2, 27.6),
             (cx + 1.8, 25.5), (cx + 1.6, 22.0), (cx + 1.5, 17.0), (cx + 1.52, 13.0), (cx + 1.75, 11.0),
             (cx + 2.2, 9.8), (cx + 2.95, 7.9), (cx + 3.7, 6.0), (cx + 4.45, COURONNE + 0.2)]
    back = [(cx + 0.75, COURONNE + 0.3), (cx + 0.05, 5.0), (cx - 0.7, 6.8), (cx - 1.4, 8.6), (cx - 1.8, 10.6),
            (cx - 1.5, 13.0), (cx - 1.42, 18.0), (cx - 1.6, 21.8), (cx - 2.25, 24.6), (cx - 2.95, 26.5),
            (cx - 2.9, 28.2), (cx - 2.1, 30.6), (cx - 0.6, 34.8), (cx + 1.8, 42.0), (cx + 4.0, 52.0)]
    D.el(runs_path([front, [front[-1], back[0]], back + [(cx + 7.0, 54.0), front[0]]], nd=2),
         fill=leg_paint(name, cx, far))
    edges = [(11.0, cx - 1.7, cx + 1.7), (15.0, cx - 1.4, cx + 1.5), (21.8, cx - 1.55, cx + 1.6),
             (24.6, cx - 2.2, cx + 1.75), (26.5, cx - 2.85, cx + 2.0), (28.2, cx - 2.8, cx + 2.6),
             (31.0, cx - 1.9, cx + 3.6)]
    k = 0.8 if far else 1.0
    hair_strokes(edges, 75, rng, shade("#FFFFFF", k), 0.55)
    hair_strokes(edges, 45, rng, shade("#B8B3A8", k), 0.5)
    # le tendon et la pointe du jarret, l'ombre derrière le canon, la lumière
    # sur le devant
    D.line([(cx - 1.2, 12.0), (cx - 1.15, 17.0), (cx - 1.35, 21.5), (cx - 2.0, 24.4), (cx - 2.55, 26.3)],
           shade(HAIR_DEEP, k), 0.5, so=0.3)
    D.line([(cx - 2.3, 27.6), (cx - 1.6, 30.0), (cx - 0.4, 33.2)], shade(HAIR_DEEP, k), 0.38, so=0.38)
    D.line([(cx - 2.6, 25.2), (cx - 2.75, 26.6), (cx - 2.55, 27.8)], shade("#FFFFFF", k), 0.25, so=0.6)
    D.line([(cx + 1.2, 12.0), (cx + 1.15, 18.0), (cx + 1.3, 23.0), (cx + 1.8, 26.4)], shade("#FFFFFF", k), 0.3,
           so=0.45)
    top = CHAUSSETTE_AR
    sfill = D.lin(f"chaussette-{name}", (cx - 2.5, 0), (cx + 4.5, 0),
                  [(0, shade(BLACK_SHEEN, k)), (0.3, BLACK), (1, BLACK_DEEP)])
    s_front = [(cx + 1.66, top + 0.3), (cx + 1.9, 10.4), (cx + 2.2, 9.8), (cx + 2.95, 7.9), (cx + 3.7, 6.0),
               (cx + 4.45, COURONNE + 0.2)]
    s_back = [(cx + 0.75, COURONNE + 0.3), (cx + 0.05, 5.0), (cx - 0.7, 6.8), (cx - 1.4, 8.6), (cx - 1.75, 9.9),
              (cx - 1.72, top + 0.2)]
    s_top = [(cx - 1.72, top + 0.2), (cx - 0.9, top + 0.45), (cx + 0.0, top + 0.15), (cx + 0.9, top + 0.5),
             (cx + 1.66, top + 0.3)]
    D.el(runs_path([s_front, [s_front[-1], s_back[0]], s_back, s_top], nd=2), fill=sfill)
    fringe_hairs(cx - 1.65, cx + 1.6, top + 0.3, rng, far)
    D.line([(cx - 0.9, 9.4), (cx - 0.2, 7.3), (cx + 0.6, 5.4)], shade(BLACK_SHEEN, 1.25 * k), 0.3, so=0.6)
    dewclaw(cx - 1.1, 8.0, far)
    hoof(cx + 0.5, cx + 5.9, far, name)
    # la cuisse et la jambe dans la laine : la culotte, jusqu'au-dessus du jarret
    sleeve = [(cx - 9.0, 56.0), (cx - 8.8, 50.0), (cx - 8.1, 45.5), (cx - 6.9, 40.8), (cx - 5.1, 36.6),
              (cx - 3.0, LAINE_AR + 1.0), (cx - 0.5, LAINE_AR), (cx + 2.2, LAINE_AR + 0.2), (cx + 4.6, LAINE_AR + 1.6),
              (cx + 7.0, 36.6), (cx + 9.0, 40.8), (cx + 10.7, 45.4), (cx + 11.8, 50.4), (cx + 11.6, 56.0),
              (cx + 1.0, 60.0)]
    kk = 0.84 if far else 1.0
    D.lin(f"fond-{name}", (0, 56), (0, LAINE_AR), [(0, shade(DEEP_MID, kk)), (1, shade(DEEP_LOW, kk))])
    wool_sleeve(name, sleeve, rng,
                lambda x, h: 0.3 + 0.32 * max(0.0, (h - 34) / 20) - 0.1 * max(0.0, (x - cx) / 10),
                far, length=(3.4, 4.6))
    D.pivot(name, cx + 3.0, HIP_H)
    D.close()


# ================================================================== la queue

def tail():
    """La queue entière et laineuse (§ 1.3), une corde de mèches qui pend
    derrière la culotte jusqu'au jarret, en trois morceaux qui ondulent ;
    sous le corps, qui recouvre son attache."""
    rng = random.Random(2002)  # le centre de béliers (S5)
    axis = [(4.2, 70.5), (1.6, 66.5), (-0.9, 61.0), (-2.2, 54.5), (-2.6, 48.0), (-2.3, 41.5), (-1.6, 35.5),
            (-0.9, QUEUE_BOUT + 1.0)]
    AX = dense(axis, False, 8)

    def at(h):
        for (x0, h0), (x1, h1) in zip(AX, AX[1:]):
            if h1 <= h <= h0:
                u = (h0 - h) / (h0 - h1) if h0 != h1 else 0
                return x0 + (x1 - x0) * u
        return AX[-1][0]

    def width(h):
        return 4.2 + 3.4 * max(0.0, min(1.0, (h - QUEUE_BOUT) / 38.0))

    g = D.lin("fond-queue", (-7, 0), (6, 0), [(0, "#C2B396"), (0.6, DEEP_MID), (1, DEEP_LOW)])

    def segment(top, bottom, overlap, last=False):
        hs = [bottom + (top + overlap - bottom) * k / 8 for k in range(9)]
        shape = [(at(h) + width(h) / 2 - 0.5, h) for h in hs[::-1]] + [(at(h) - width(h) / 2 + 0.5, h) for h in hs]
        D.el(cr(shape, nd=1, t=0.8), fill=g)
        crimps = Crimps()
        locks = []
        h = top + overlap * 0.6
        while h > bottom + 1.0:
            w = width(h)
            n = max(2, int(round(w / 1.6)))
            for k in range(n):
                u = (k + 0.5) / n
                x = at(h) - w / 2 + w * u + rng.uniform(-0.25, 0.25)
                end = h - rng.uniform(3.4, 4.4)
                tip = last and end < bottom + 1.2
                L = (h - bottom + rng.uniform(1.3, 2.3)) if tip else (h - end)
                ang = math.atan2(at(h - L) - at(h), L) * 0.9 + (u - 0.5) * 0.3 + rng.uniform(-0.12, 0.12)
                light = 0.62 - 0.38 * u + 0.25 * max(0.0, (h - 50) / 20) + rng.uniform(-0.05, 0.05)
                locks.append((h + rng.uniform(-0.3, 0.3), x, L, ang, light, tip))
            h -= rng.uniform(2.0, 2.4)
        locks.sort()
        for (root, x, L, ang, light, tip) in locks:
            lock(x, root, L, rng.uniform(1.5, 1.8), ang, light, rng, crimps, visible=None if tip else 2.6,
                 hook=rng.choice((-1, 1)) * rng.uniform(1.0, 1.8) if tip else None)
        crimps.emit()

    D.comment("Queue : entière, laineuse, jusqu'au jarret, en trois morceaux")
    D.open("tail")
    segment(70.0, 54.5, 0.0)
    D.open("tail-mid")
    segment(56.5, 42.0, 1.5)
    D.open("tail-tip")
    segment(44.0, QUEUE_BOUT, 1.5, last=True)
    D.pivot("tail-tip", at(43.0), 43.0)
    D.close()
    D.pivot("tail-mid", at(55.5), 55.5)
    D.close()
    D.pivot("tail", 3.8, 69.5)
    D.close()


# ================================================================== la tête

HU = (math.cos(HEAD_TILT), -math.sin(HEAD_TILT))
HV = (-math.sin(HEAD_TILT), -math.cos(HEAD_TILT))


def HL(lx, ly):
    """Repère de la tête (en cm ; origine à la nuque, lx le long de l'axe de
    la nuque au bout du nez, ly vers la mâchoire) vers le monde."""
    return (NUQUE[0] + lx * HU[0] + ly * HV[0], NUQUE[1] + lx * HU[1] + ly * HV[1])


def HLs(pts):
    return [HL(*p) for p in pts]


def world_dir(lx, ly):
    """Une direction du repère de la tête, dans le monde."""
    return (lx * HU[0] + ly * HV[0], lx * HU[1] + ly * HV[1])


# Le profil de la tête, superposé à P6 (même axe nuque → nez) et repris des
# béliers P1 et P3 : le front large et bombé sous le toupet, le chanfrein un
# peu convexe (« à peine busqué », § 5 : 42° sous l'horizontale), le bout du
# nez arrondi et charnu, sa face presque perpendiculaire à l'axe de la tête
# (P6), la lèvre supérieure ronde ; la mâchoire haute, le menton plein, la
# ligne de la mâchoire un peu bombée sous le masséter, puis la ganache qui
# remonte vers la nuque sous la laine de la joue. 30,9 cm de la nuque au
# naseau, 15,2 cm à l'angle de la mâchoire (§ 2.3 : 30-31 et 14-15). L'œil est
# à 5,5 cm sous le profil du front (P6 : 5,1) : quand le moteur fait glisser
# l'iris (jusqu'à 2,7 cm), il ne sort jamais du contour de la tête.
DORSAL = [(-1.5, 1.0), (-1.2, -0.9), (0.3, -2.3), (2.4, -3.15), (5.4, -3.65), (8.6, -3.9), (11.8, -3.95),
          (14.8, -3.85), (17.6, -3.6), (20.2, -3.25), (22.7, -2.85), (25.0, -2.4), (27.0, -1.85), (28.6, -1.15),
          (29.8, -0.2), (30.55, 0.95), (30.9, 2.15), (30.85, 3.3), (30.5, 4.25), (29.9, 4.85)]
MOUTH = [(29.9, 4.85), (28.6, 5.15), (26.8, 5.45), (25.0, 5.68), (23.3, 5.85)]
NOTCH = [(23.3, 5.85), (23.15, 7.1), (22.8, 8.35), (22.15, 9.45), (21.5, 10.05)]
JAWLINE = [(21.5, 10.05), (19.8, 10.95), (17.6, 11.75), (15.6, 12.1), (13.6, 11.95), (11.8, 11.3)]
BACKLINE = [(11.8, 11.25), (9.8, 11.9), (7.4, 12.1), (5.0, 11.4), (3.0, 9.8), (1.3, 7.5), (0.0, 4.9), (-1.0, 2.6),
            (-1.5, 1.0)]
HEAD_OUT = DORSAL + MOUTH[1:] + NOTCH[1:] + JAWLINE[1:] + BACKLINE[1:-1]

EYE_L = (13.2, 1.65)            # l'œil : à 57 % du nez vers la nuque (§ 5), haut sur le côté de la tête (P6)
EAR_L = (8.2, 4.5)              # l'oreille sort sous la base de la corne, derrière l'œil
MUZZLE_L = (29.8, 6.0)          # ce qui touche l'herbe : le bas des lèvres
JAW_PIVOT_L = (19.0, 7.0)       # la charnière de la mâchoire, sous la joue
# La lèvre supérieure, du dessous du naseau à la commissure.
LIP_L = [(25.0, 3.9), (26.6, 3.55), (28.2, 3.25), (29.5, 3.0), (30.55, 2.9), (31.1, 3.3), (31.0, 4.3),
         (30.3, 5.0), (29.0, 5.3), (27.0, 5.6), (25.2, 5.85), (23.9, 6.05), (23.55, 5.5), (24.2, 4.6)]
LIP_PIVOT_L = (25.0, 4.2)
# Le naseau, en virgule : le bout rond en bas et en avant, fendu vers
# l'arrière et le haut (§ 5).
NOSTRIL_L = [(30.3, 2.1), (30.1, 2.6), (29.55, 2.65), (29.0, 2.25), (28.45, 1.65), (27.9, 1.05), (28.5, 1.15),
             (29.15, 1.55), (29.75, 1.7)]

# Les lunettes : une tache ovale autour de l'œil, un peu plus haute que
# large (P6 : 0,20 de la tête en largeur, 0,27 en hauteur), l'œil dans sa
# moitié haute, son grand axe vertical dans le monde (§ 4.3 b).
LUN_C = (EYE_L[0] + 1.0, EYE_L[1] + 1.43)
LUN_A, LUN_B = 3.05, 4.15


def head_outline():
    """Le contour de la face, échantillonné exactement comme le trace
    `runs_path` (les mêmes suites lisses, les mêmes angles)."""
    pts = []
    for run in (DORSAL, MOUTH, NOTCH, JAWLINE + BACKLINE[1:]):
        pts += dense(run, False, 8)[:-1]
    return pts


def dorsal_at(lx):
    """La hauteur (ly) du profil du dessus à l'abscisse lx de la tête."""
    pts = dense(DORSAL, False, 8)
    for (x0, y0), (x1, y1) in zip(pts, pts[1:]):
        if x0 <= lx <= x1:
            return y0 + (y1 - y0) * (lx - x0) / (x1 - x0)
    return pts[-1][1]


def lunette_pts(rng, n=30, grow=0.0, rough=False):
    """Le contour des lunettes, dans le repère de la tête. `rough` : le bord
    du poil, net et un peu irrégulier (§ 4.3 b), pas un galet lisse."""
    down = (math.sin(HEAD_TILT), math.cos(HEAD_TILT))     # le bas du monde, dans le repère de la tête
    fw = (math.cos(HEAD_TILT), -math.sin(HEAD_TILT))      # l'avant du monde
    pts = []
    tufts = [(rng.uniform(0, 2 * math.pi), rng.uniform(0.025, 0.045) * rng.choice((-1, 1))) for _ in range(5)] \
        if rough else []
    for k in range(n):
        a = 2 * math.pi * k / n
        r = 1.0 + 0.04 * math.sin(3 * a + 0.7) + 0.025 * math.sin(7 * a + 2.1)
        if rough:
            r += 0.01 * math.sin(13 * a + 0.3) + 0.006 * math.sin(23 * a + 1.9) + rng.uniform(-0.004, 0.004)
            for (a0, depth) in tufts:
                da = (a - a0 + math.pi) % (2 * math.pi) - math.pi
                r += depth * max(0.0, 1.0 - abs(da) / 0.09)
        # la petite larme vers le bas et l'avant (§ 4.3 b), loin du museau
        lar = 0.1 * max(0.0, math.cos(a - 1.15)) ** 6
        u = (LUN_A + grow) * math.cos(a) * r
        v = (LUN_B + grow) * math.sin(a) * r * (1 + lar)
        pts.append((LUN_C[0] + u * fw[0] + v * down[0], LUN_C[1] + u * fw[1] + v * down[1]))
    return pts


EYE_ROT = math.radians(-15.0)   # l'œil moins penché que la tête


def eye_local(ex, ey, scale=1.0):
    ex, ey = ex * scale, ey * scale
    c, s = math.cos(EYE_ROT), math.sin(EYE_ROT)
    return (EYE_L[0] + ex * c - ey * s, EYE_L[1] + ex * s + ey * c)


def eye_window(scale=1.0):
    """L'ouverture de l'œil (repère de la tête) : une amande, le coin de
    devant un peu plus bas."""
    base = [(-1.65, 0.05), (-1.15, -0.78), (-0.05, -1.03), (1.0, -0.86), (1.68, -0.1), (1.02, 0.66), (0.0, 0.86),
            (-1.1, 0.6)]
    return [eye_local(ex, ey, scale) for (ex, ey) in base]


def eye_pt(ex, ey):
    return HL(*eye_local(ex, ey))


def head():
    rng = random.Random(1870)  # les deux souches de Marthod et de Thônes (S5)
    D.comment("Tête : fine et longue, face blanche à poil ras, lunettes et museau noirs, toupet bouclé, "
              "oreilles noires, cornes brun caramel en large spirale")
    D.open("head")
    ear("ear-far", far=True)
    jaw()
    if VARIANTE != "agneau":
        far_horn()
    # le fond de l'orbite, sous l'œil : le noir des paupières, qu'on voit
    # quand il cligne
    D.el(cr(HLs(eye_window(1.5)), nd=2), fill=BLACK)
    eyes()
    face(rng)
    ear("ear-near", far=False)
    if VARIANTE != "agneau":
        near_horn(rng)
    toupet_front(rng)
    D.pivot("head", *NUQUE)
    D.pivot("muzzle", *HL(*MUZZLE_L))
    D.close()


def ear(name, far):
    """L'oreille, entièrement noire jusqu'à la base (§ 4.3 c), moyenne, en
    feuille de laurier, la pointe arrondie ; portée à l'horizontale vers le
    côté et un peu vers l'arrière, tombant de 20 à 30° au calme (§ 5) : de
    profil, elle vient vers nous, raccourcie."""
    base = HL(*EAR_L)
    if far:
        base = (base[0] - 0.4, base[1] + 1.2)
    ang = math.radians(204.0 if not far else 192.0)
    a = (math.cos(ang), math.sin(ang))          # le long de l'oreille, vers sa pointe
    nrm = (-a[1], a[0])                         # vers le haut de l'oreille
    L = OREILLE * (0.53 if not far else 0.36)

    def P(u, v):
        return (base[0] + a[0] * u + nrm[0] * v, base[1] + a[1] * u + nrm[1] * v)

    D.open(name)
    # la feuille : étroite à la base, la plus large au milieu, la pointe
    # arrondie un peu relevée
    shape = [(-0.8, -0.95), (0.8, -1.6), (2.6, -2.35), (4.4, -2.55), (6.0, -2.1), (7.2, -1.2), (L - 0.3, -0.25),
             (L, 0.3), (7.5, 0.85), (6.0, 1.6), (4.2, 2.05), (2.4, 1.8), (0.8, 1.3), (-0.8, 0.85)]
    if far:
        D.el(cr([P(u, v) for (u, v) in shape]), fill=BLACK_DEEP)
    else:
        g = D.lin("oreille", P(4, 2.4), P(4, -2.6), [(0, "#3A3631"), (0.45, BLACK), (1, "#0C0B0B")])
        D.el(cr([P(u, v) for (u, v) in shape]), fill=g)
        # la conque, noire aussi, à peine plus brune dans l'ombre ; le bord du
        # pavillon qui prend la lumière
        concha = [(1.0, -0.15), (2.6, -1.05), (4.4, -1.2), (6.0, -0.85), (7.0, -0.1), (6.2, 0.65), (4.3, 0.95),
                  (2.4, 0.8), (1.0, 0.45)]
        D.el(cr([P(u, v) for (u, v) in concha]), fill="#2B2420")
        D.line([P(1.0, 0.0), P(2.6, -0.1), P(4.2, 0.05)], "#0A0908", 0.35, so=0.8)
        D.line([P(0.6, 1.25), P(2.4, 1.7), P(4.3, 1.92), P(6.2, 1.55), P(7.6, 0.85)], "#4C4843", 0.2, so=0.55)
        D.line([P(1.2, -1.45), P(3.0, -2.05), P(5.2, -2.05), P(7.0, -1.25)], "#34302C", 0.18, so=0.7)
    D.pivot(name, *base)
    D.close()


def jaw():
    """La mâchoire : le bas du museau (lèvre inférieure et menton, noirs, § 4.3
    a), et au-dessus, cachés tant que la bouche est fermée, l'intérieur de la
    bouche, la langue et les incisives du bas. Ce qui ne doit pas se voir
    reste dans le contour de la tête ; la lèvre du bas, un peu en retrait de
    celle du haut, et le menton plein, arrondi (P6)."""
    rng = random.Random(1996)  # le premier comptage des brebis (S7)
    D.open("jaw")
    # la joue qui s'étire quand la bouche s'ouvre (cachée au repos)
    D.el(cr(HLs([(15.5, 7.0), (21.0, 5.0), (21.6, 9.4), (16.6, 11.1)]), t=0.6), fill=HAIR_SHADOW)
    # l'intérieur de la bouche, sombre, au-dessus de la lèvre du bas
    D.el(cr(HLs([(18.5, 7.9), (20.5, 5.5), (23.0, 4.3), (25.5, 3.95), (28.6, 3.7), (29.6, 4.0), (28.4, 4.5),
                 (25.5, 4.9), (22.5, 5.45), (20.0, 6.8)]), t=0.8), fill="#2E1E1F")
    D.el(cr(HLs([(21.4, 5.8), (23.4, 4.95), (25.8, 4.5), (28.0, 4.35), (27.2, 4.75), (23.8, 5.35)]), t=0.9),
         fill="#8C5F62")
    D.line(HLs([(22.6, 5.5), (25.0, 4.85), (27.0, 4.65)]), "#B07D80", 0.15, so=0.7)
    # les incisives du bas (en haut, le mouton n'a qu'un bourrelet)
    D.el(cr(HLs([(28.3, 4.3), (29.4, 4.2), (29.5, 4.62), (28.4, 4.72)]), t=0.5), fill="#D8D0BC")
    # la lèvre du bas et le menton
    jw = HLs([(17.6, 8.0), (20.4, 6.0), (23.0, 5.0), (25.5, 4.75), (28.0, 4.5), (29.3, 4.45), (29.9, 4.85),
              (30.0, 5.4), (29.8, 6.2), (29.2, 7.1), (28.2, 7.9), (26.7, 8.55), (25.0, 9.15), (23.6, 9.6),
              (22.4, 9.98), (21.4, 10.15), (20.4, 10.25), (18.6, 10.4), (17.4, 10.5)])
    g = D.lin("menton", HL(25, 4.6), HL(25, 9.4), [(0, "#2C2A28"), (0.45, BLACK), (1, BLACK_DEEP)])
    D.el(cr(jw, t=0.85), fill=g)
    # la lèvre du bas, humide, sous le sillon de la bouche ; la rondeur du
    # menton qui prend un peu de lumière ; le poil ras, noir, du menton
    D.line(HLs([(24.6, 5.95), (26.8, 5.7), (28.8, 5.45), (29.75, 5.3)]), NOSE_SHEEN, 0.16, so=0.45)
    D.line(HLs([(29.6, 6.3), (29.0, 7.2), (27.9, 7.9)]), BLACK_SHEEN, 0.35, so=0.4)
    hairs = []
    for _ in range(36):
        lx, ly = rng.uniform(23.8, 29.6), rng.uniform(5.9, 9.0)
        if not inside(DORSAL[-1:] + [(30.0, 5.4), (29.8, 6.2), (29.2, 7.1), (28.2, 7.9), (26.7, 8.55), (25.0, 9.1),
                                     (23.6, 9.45), (23.4, 6.0)], lx, ly):
            continue
        p = HL(lx, ly)
        d = world_dir(-rng.uniform(0.3, 0.6), rng.uniform(0.2, 0.45))
        hairs.append(cr([p, (p[0] + d[0], p[1] + d[1])], closed=False, nd=2))
    D.el("".join(hairs), stroke=BLACK_SHEEN, sw=0.055, so=0.35, cap="round")
    D.pivot("jaw", *HL(*JAW_PIVOT_L))
    D.close()


def eyes():
    """L'œil : l'iris paille ambrée qui ressort dans le noir des lunettes
    (§ 5), la pupille en fente horizontale (dans le monde, au repos)."""
    E = HL(*EYE_L)
    D.open("eyes")
    win = HLs(eye_window(1.0))
    D.el(cr(win, nd=2, t=0.9), fill="#3A2C20")
    D.open("pupils")
    ic = (E[0] + 0.05, E[1] - 0.02)
    gi = D.rad("iris", ic, 1.35, [(0, IRIS_LIGHT), (0.45, IRIS_LIGHT), (0.74, IRIS), (0.9, "#8F7A55"), (1, LIMBUS)],
               f=(ic[0] - 0.25, ic[1] + 0.35))
    D.el(cr(ellipse_pts(ic[0], ic[1], 1.35, 1.35, 0, 16), nd=2), fill=gi)
    fib = []
    for k in range(16):
        a0 = 2 * math.pi * k / 16 + 0.15
        fib.append(cr([(ic[0] + 0.5 * math.cos(a0), ic[1] + 0.5 * math.sin(a0)),
                       (ic[0] + 1.2 * math.cos(a0 + 0.1), ic[1] + 1.2 * math.sin(a0 + 0.1))], closed=False, nd=2))
    D.el("".join(fib), stroke="#8C7550", sw=0.05, so=0.5, cap="round")
    D.el(cr(ellipse_pts(ic[0], ic[1], 1.3, 1.3, 0, 16), nd=2), stroke=LIMBUS, sw=0.1, so=0.85)
    pw, ph = 1.45, 0.48
    X, Y = S(ic[0] - pw / 2, ic[1] + ph / 2)
    D.emit(f'<rect x="{num(X)}" y="{num(Y)}" width="{num(pw)}" height="{num(ph)}" rx="{num(ph / 2)}" fill="{PUPIL}"/>')
    D.pivot("pupils", *ic)
    D.close()
    # les reflets (après les pupilles : ils ne glissent pas avec l'iris)
    D.el(cr(ellipse_pts(E[0] - 0.45, E[1] + 0.4, 0.28, 0.19, 0.3, 10), nd=2), fill=CATCH, fo=0.9)
    D.el(cr(ellipse_pts(E[0] + 0.6, E[1] - 0.28, 0.11, 0.08, 0.3, 8), nd=2), fill=CATCH, fo=0.45)
    # l'ombre de la paupière du haut, le bord noir des paupières, les cils
    lid = [eye_pt(-1.7, 0.0), eye_pt(-1.15, -0.8), eye_pt(0.0, -1.06), eye_pt(1.02, -0.9), eye_pt(1.7, -0.08)]
    shadow = lid + [eye_pt(1.0, -0.45), eye_pt(0.0, -0.58), eye_pt(-1.1, -0.35)]
    D.el(cr(shadow, nd=2), fill="#1C1611", fo=0.55)
    D.el(cr(win, nd=2, t=0.9), stroke=BLACK_DEEP, sw=0.34, join="round")
    D.line(lid, BLACK_SHEEN, 0.16, so=0.7)
    lashes = []
    for k in range(6):
        u = (k + 0.5) / 6
        p = eye_pt(-1.35 + 2.7 * u, -0.98 - 0.1 * math.sin(math.pi * u))
        q = eye_pt(-1.35 + 2.7 * u - 0.28, -1.5 - 0.1 * math.sin(math.pi * u))
        lashes.append(cr([p, q], closed=False, nd=2))
    D.el("".join(lashes), stroke=BLACK_DEEP, sw=0.09, cap="round")
    D.pivot("eyes", *E)
    D.close()


FACE_SOFT = 0


def soft_spot(c, rx, ry, rot, color, alpha, clip):
    """Une tache douce sur la face : un dégradé radial elliptique (centre `c`
    et rayons dans le repère de la tête, tourné avec elle de `rot`), de
    `alpha` au centre à rien au bord, rogné à `clip` (la face sans l'œil) :
    l'ombre ou la lumière qui donne son volume à l'os et au muscle, sans
    filtre ni bord visible."""
    global FACE_SOFT
    FACE_SOFT += 1
    pts = []
    for k in range(40):
        t = 2 * math.pi * k / 40
        ex, ey = rx * math.cos(t), ry * math.sin(t)
        pts.append(HL(c[0] + ex * math.cos(rot) - ey * math.sin(rot), c[1] + ex * math.sin(rot) + ey * math.cos(rot)))
    shape = Polygon(pts).intersection(clip)
    if shape.is_empty:
        return
    polys = [shape] if shape.geom_type == "Polygon" else [g for g in getattr(shape, "geoms", []) if g.geom_type == "Polygon"]
    d = "".join(poly(list(g.simplify(0.04).exterior.coords)[:-1], nd=2) for g in polys if g.area > 0.01)
    if not d:
        return
    g = D.ell(f"modele{FACE_SOFT}", HL(*c), rx, ry, HEAD_TILT + rot,
              [(0, color, alpha), (0.55, color, alpha * 0.45), (1, color, 0.0)])
    D.el(d, fill=g)


def ragged(pts, rng, amp=0.09, step=0.22, closed=False):
    """Un bord de poil net mais un peu irrégulier : la ligne rééchantillonnée,
    chaque point poussé de part et d'autre de quelques millimètres, avec
    quelques mèches de poil un peu plus marquées."""
    P = dense(pts, closed, 12)
    out = [P[0]]
    acc = 0.0
    for a, b in zip(P, P[1:]):
        acc += math.dist(a, b)
        if acc >= step:
            out.append(b)
            acc = 0.0
    if out[-1] != P[-1]:
        out.append(P[-1])
    n = len(out)
    res = []
    ph1, ph2 = rng.uniform(0, 6.28), rng.uniform(0, 6.28)
    for i, (x, y) in enumerate(out):
        if (i == 0 or i == n - 1) and not closed:
            res.append((x, y))
            continue
        p, q = out[(i - 1) % n], out[(i + 1) % n]
        tx, ty = q[0] - p[0], q[1] - p[1]
        L = math.hypot(tx, ty) or 1.0
        nx, ny = -ty / L, tx / L
        k = amp * (0.6 * math.sin(i * 0.9 + ph1) + 0.4 * math.sin(i * 2.3 + ph2)) + rng.uniform(-0.3, 0.3) * amp
        if rng.random() < 0.08:
            k += rng.choice((-1, 1)) * amp * 1.6
        res.append((x + nx * k, y + ny * k))
    return res


def face(rng):
    """La face : le crâne et la joue (percés de l'ouverture de l'œil), modelés
    d'ombres et de lumières douces (l'os du front et du chanfrein, le
    masséter, le creux devant la joue, la ganache) ; le poil ras blanc ; les
    lunettes ; le museau noir, son cuir et ses naseaux ; la laine bouclée de
    la joue (chez le bélier, § 3.3) et le toupet."""
    win = HLs(eye_window(1.04))
    out = HLs(HEAD_OUT)
    g = D.lin("face", HL(16, -3.9), HL(14, 12.5), [(0, HAIR_LIGHT), (0.42, HAIR), (0.82, HAIR_SHADOW), (1, "#B3AEA2")])
    # le tracé principal, avec l'encoche de la mâchoire (sans contour le long
    # de l'encoche) et l'ouverture de l'œil (les paupières et l'iris dessous)
    D.el(runs_path([HLs(DORSAL), HLs(MOUTH), HLs(NOTCH), HLs(JAWLINE + BACKLINE[1:])], nd=2)
         + cr(win[::-1], nd=2, t=0.9), fill=g, rule="evenodd")
    # le modelé (repère de la tête ; lumière d'en haut et de l'avant) : la
    # lumière sur l'arête du nez et sur le masséter, l'ombre sous l'arcade
    # et sous l'œil, le creux devant la joue, l'ombre sous la ganache
    clip = Polygon(HLs(head_outline())).buffer(0).difference(Polygon(HLs(eye_window(1.15))).buffer(0))
    soft_spot((19.8, -2.55), 7.2, 1.15, 0.14, "#FFFFFF", 0.75, clip)
    soft_spot((27.2, -0.9), 1.6, 0.9, 0.5, "#FFFFFF", 0.4, clip)
    soft_spot((16.4, -1.0), 2.4, 1.15, 0.35, HAIR_DEEP, 0.32, clip)
    soft_spot((18.4, 5.3), 3.3, 1.5, 0.55, HAIR_DEEP, 0.36, clip)
    soft_spot((21.4, 6.6), 2.0, 3.0, 0.0, HAIR_DEEP, 0.28, clip)
    soft_spot((15.0, 7.6), 3.8, 2.8, -0.2, "#FFFFFF", 0.42, clip)
    soft_spot((16.4, 11.2), 6.8, 1.6, -0.1, "#877D70", 0.55, clip)
    soft_spot((23.0, 2.6), 1.6, 2.8, 0.0, HAIR_DEEP, 0.16, clip)
    # le museau noir (§ 4.3 a) : son bord descend du chanfrein, 3 cm
    # au-dessus du naseau, jusqu'à la commissure, net mais un peu irrégulier
    top = (26.1, dorsal_at(26.1))
    mz_back = ragged([(23.3, 5.85), (23.05, 4.6), (23.2, 3.3), (23.6, 2.0), (24.2, 0.7), (24.95, -0.55),
                      (25.7, -1.65), top], rng, amp=0.08)
    mz_zone = HLs(mz_back + [(31.5, -2.5), (31.5, 6.0)])
    # le poil ras : des traits fins couchés vers le nez, clairs sur le
    # chanfrein et le masséter, plus sombres sur la joue et sous la mâchoire
    lunp = HLs(lunette_pts(random.Random(0), grow=0.35))
    hairs_l, hairs_d = [], []
    for _ in range(300):
        lx = rng.uniform(9.5, 26.0)
        ly = rng.uniform(-3.8, 12.0)
        p = HL(lx, ly)
        if not inside(out, *p) or inside(lunp, *p) or inside(mz_zone, *p):
            continue
        ln = rng.uniform(0.6, 1.1)
        d = world_dir(ln, 0.12 * ln * rng.uniform(-1, 1) + 0.08 * ln)
        seg = cr([p, (p[0] + d[0], p[1] + d[1])], closed=False, nd=2)
        lit = ly < 0.0 or (12.0 < lx < 17.5 and 5.0 < ly < 9.5)
        (hairs_l if lit and rng.random() < 0.8 else hairs_d).append(seg)
    D.el("".join(hairs_l), stroke="#FFFFFF", sw=0.06, so=0.65, cap="round")
    D.el("".join(hairs_d), stroke="#A39D91", sw=0.06, so=0.42, cap="round")
    lunettes(rng, win)
    muzzle(rng, mz_back, top)
    cheek_and_toupet(rng)


def hex_lattice(c, R, dx, dh, rng, jitter=0.25):
    """Des points en quinconce autour de `c` (repère de la tête), un peu au
    hasard : des rangs de poils réguliers sans être alignés."""
    out = []
    row = 0
    y = c[1] - R
    while y <= c[1] + R:
        x = c[0] - R + (row % 2) * dx / 2
        while x <= c[0] + R:
            out.append((x + rng.uniform(-jitter, jitter) * dx, y + rng.uniform(-jitter, jitter) * dh))
            x += dx
        y += dh
        row += 1
    return out


def lunettes(rng, win):
    """Les lunettes : l'ovale noir autour de l'œil, au bord net et un peu
    irrégulier (§ 4.3 b), percé de l'ouverture de l'œil (elles cachent l'iris
    qui glisse) ; le poil noir en traits fins, couché vers l'avant et le bas ;
    un léger reflet sur le haut, du côté de la lumière."""
    lun = HLs(lunette_pts(rng, n=110, rough=True))
    D.el(poly(lun, nd=2) + cr(win[::-1], nd=2, t=0.9), fill=BLACK, rule="evenodd")
    shape = Polygon(lun).buffer(0).difference(Polygon(HLs(eye_window(1.1))).buffer(0))
    # le reflet du poil, en haut et en arrière (la lumière vient d'en haut à gauche)
    rx, ry = 1.9, 2.4
    c = (LUN_C[0] - 1.0, LUN_C[1] - 1.9)
    pts = [HL(c[0] + rx * math.cos(2 * math.pi * k / 40), c[1] + ry * math.sin(2 * math.pi * k / 40)) for k in range(40)]
    sheen = Polygon(pts).intersection(shape.buffer(-0.12))
    if not sheen.is_empty and sheen.geom_type == "Polygon":
        g = D.ell("lunette-reflet", HL(*c), rx, ry, HEAD_TILT, [(0, "#6B665F", 0.55), (0.6, "#4A4641", 0.25),
                                                            (1, "#3E3B37", 0.0)])
        D.el(poly(list(sheen.simplify(0.04).exterior.coords)[:-1], nd=2), fill=g)
    # le poil noir, en traits fins : il rayonne autour de l'œil puis file
    # vers l'avant et le bas, comme le poil ras de la face
    E = HL(*EYE_L)
    flow = world_dir(0.75, 0.42)
    nf = math.hypot(*flow)
    hairs = []
    inner = shape.buffer(-0.18)
    for (lx, ly) in hex_lattice(LUN_C, 4.6, 0.62, 0.55, rng):
        p = HL(lx, ly)
        if not inner.contains(Point(p)):
            continue
        vx, vh = p[0] - E[0], p[1] - E[1]
        r = math.hypot(vx, vh) or 1.0
        dx, dh = vx / r + 0.9 * flow[0] / nf, vh / r + 0.9 * flow[1] / nf
        n = math.hypot(dx, dh) or 1.0
        L = rng.uniform(0.38, 0.55)
        hairs.append(cr([p, (p[0] + dx / n * L, p[1] + dh / n * L)], closed=False, nd=2))
    D.el("".join(hairs), stroke="#3A3733", sw=0.055, so=0.5, cap="round")
    # quelques poils du bord qui débordent à peine sur le blanc
    tufts = []
    for (x, h) in lun[3::9]:
        d = world_dir(0.3, 0.08)
        tufts.append(cr([(x - d[0] * 0.3, h - d[1] * 0.3), (x + d[0], h + d[1])], closed=False, nd=2))
    D.el("".join(tufts), stroke=BLACK, sw=0.06, so=0.6, cap="round")


def muzzle(rng, mz_back, top):
    """Le museau noir (§ 4.3 a, § 5) : le poil noir, du bord jusqu'aux lèvres ;
    le cuir du nez, nu, gris-noir, un peu plus clair que le poil, avec son
    reflet humide ; le naseau en virgule ; la lèvre supérieure ronde, fendue
    au milieu ; le sillon de la bouche. Le menton noir est dans la mâchoire :
    l'encoche de la tête, arrondie vers l'arrière, le montre jusqu'à la
    ganache."""
    front = [top] + [p for p in dense(DORSAL, False, 8) if p[0] > top[0] + 0.05]
    mouth = dense(MOUTH, False, 6)
    black = mz_back + front[1:] + mouth[1:-1]
    gm = D.lin("museau", HL(26, -2.2), HL(26, 6.0), [(0, "#2E2C29"), (0.3, BLACK), (1, BLACK_DEEP)])
    # le poil noir : le bord irrégulier (de la commissure au chanfrein), le
    # profil jusqu'au bout de la lèvre, la bouche ; percé sous le cuir du nez
    # (un peu en retrait de son bord) : là où la lèvre coupe le cuir, aucun
    # noir dessous ne transparaît le long de la coupe
    shape = Polygon(HLs(black)).buffer(0).difference(Polygon(HLs(RHIN)).buffer(0).buffer(-0.18))
    rings = [shape.exterior] + list(shape.interiors)
    D.el("".join(poly(list(r.coords)[:-1], nd=2) for r in rings), fill=gm, rule="evenodd")
    # le poil du museau, en traits fins couchés vers le nez
    hairs = []
    zone = HLs(black)
    for _ in range(90):
        lx, ly = rng.uniform(23.0, 29.0), rng.uniform(-2.2, 5.6)
        p = HL(lx, ly)
        if not inside(zone, *p) or inside(HLs(RHIN), *p):
            continue
        d = world_dir(rng.uniform(0.35, 0.65), rng.uniform(0.05, 0.3))
        hairs.append(cr([p, (p[0] + d[0], p[1] + d[1])], closed=False, nd=2))
    D.el("".join(hairs), stroke=BLACK_SHEEN, sw=0.055, so=0.32, cap="round")
    # le bord : quelques poils noirs sur le blanc, quelques blancs sur le noir
    out_t, in_t = [], []
    for (lx, ly) in mz_back[2:-1:5]:
        p = HL(lx, ly)
        d = world_dir(-rng.uniform(0.12, 0.25), rng.uniform(-0.08, 0.08))
        out_t.append(cr([p, (p[0] + d[0], p[1] + d[1])], closed=False, nd=2))
        if rng.random() < 0.5:
            d = world_dir(rng.uniform(0.15, 0.3), rng.uniform(-0.08, 0.08))
            in_t.append(cr([p, (p[0] + d[0], p[1] + d[1])], closed=False, nd=2))
    D.el("".join(out_t), stroke=BLACK, sw=0.06, so=0.5, cap="round")
    D.el("".join(in_t), stroke=HAIR, sw=0.05, so=0.5, cap="round")
    # le cuir du nez : nu, mat, gris-noir, plus clair que le poil
    gt = D.rad("truffe", HL(29.4, 1.0), 3.5, [(0, "#4A4A4D"), (0.5, "#353537"), (1, "#222224")],
               f=HL(28.9, 0.2))
    D.el(runs_path([HLs(RHIN[:RHIN_FRONT]), HLs(RHIN[RHIN_FRONT - 1:] + RHIN[:1])], nd=2), fill=gt)
    # le naseau, en virgule, et son ombre ; la fente qui file vers l'arrière
    D.el(cr(HLs(NOSTRIL_L), nd=2, t=0.9), fill=NOSTRIL)
    D.line(HLs([(27.95, 1.05), (27.4, 0.75), (27.0, 0.5)]), NOSTRIL, 0.15, so=0.65)
    # le reflet humide : le bord du naseau, le dessus du nez
    D.line(HLs([(28.4, 0.7), (29.05, 1.05), (29.65, 1.3)]), "#8A8B8F", 0.14, so=0.6)
    D.el(cr(ellipse_pts(*HL(29.2, -0.45), 0.3, 0.14, -HEAD_TILT, 10), nd=2), fill="#9A9CA0", fo=0.45)
    D.line(HLs([(30.55, 2.75), (30.45, 3.4)]), "#6E6F73", 0.14, so=0.6)
    # la lèvre supérieure : ronde sous le naseau, fendue au milieu (le
    # philtrum), le bord qui prend la lumière
    D.line(HLs([(30.55, 2.85), (30.35, 3.6), (30.05, 4.45)]), "#0B0B0B", 0.14, so=0.85)
    D.el(cr(ellipse_pts(*HL(29.6, 3.9), 0.7, 0.3, -HEAD_TILT - 0.25, 12), nd=2), fill="#4E4F52", fo=0.18)
    # le sillon de la bouche
    D.line(HLs(MOUTH[:1] + [(28.6, 5.12), (26.8, 5.42), (25.0, 5.65), (23.5, 5.82)]), "#060606", 0.2, so=0.9)


# Le cuir du nez (le mufle nu) : du dessus du nez, autour des naseaux,
# jusqu'au bout de la lèvre supérieure par le philtrum.
RHIN = [(28.0, -1.42), (28.6, -1.15), (29.8, -0.2), (30.55, 0.95), (30.9, 2.15), (30.85, 3.3), (30.5, 4.25),
        (29.95, 4.8), (29.5, 4.35), (29.25, 3.45), (28.75, 2.65), (28.15, 1.6), (27.7, 0.35), (27.55, -0.6)]
RHIN_FRONT = 8          # les huit premiers points suivent le profil


def cheek_and_toupet(rng):
    """La laine bouclée du bélier : derrière l'œil, sur la joue, autour de la
    base de l'oreille et jusqu'à l'angle de la mâchoire (§ 3.3, P1), qui
    rejoint celle de l'encolure ; et le toupet."""
    cheek = [(9.6, -1.4), (10.3, 1.0), (10.0, 4.0), (9.8, 7.0), (10.4, 9.3), (11.4, 10.9), (12.4, 11.6),
             (10.0, 12.3), (7.4, 12.5), (5.0, 11.8), (3.0, 10.1), (1.3, 7.7), (0.0, 5.0), (-1.0, 2.6), (-1.6, 1.0),
             (-1.0, -0.6), (2.0, -1.3), (5.5, -1.6), (8.0, -1.7)]
    CH = HLs(cheek)
    gc = D.lin("joue", HL(5, -1.6), HL(7, 15.0), [(0, DEEP_TOP), (0.6, DEEP_MID), (1, "#8E7D60")])
    D.el(cr(CH, nd=1), fill=gc)
    crimps = Crimps()
    CHD = dense(CH, True, 4)
    pts = hex_roots(CHD, 1.3, 1.45, rng, 0.25, jitter=0.35)
    pts.sort(key=lambda p: (p[1], p[0]))
    # les lunettes, un peu agrandies : aucune boucle de la joue n'y mord,
    # leur bord reste net (§ 4.3 b)
    keep_off = HLs(lunette_pts(random.Random(0), grow=0.5))

    def clear_angle(x, h, L, ang):
        for _ in range(6):
            tip = (x + L * math.sin(ang), h - L * math.cos(ang))
            mid = (x + 0.5 * L * math.sin(ang), h - 0.5 * L * math.cos(ang))
            if not (inside(keep_off, *tip) or inside(keep_off, *mid)):
                return ang
            ang -= 0.45          # vers l'arrière
        return ang

    for (x, h) in pts:
        light = 0.6 + 0.03 * (h - 88) + rng.uniform(-0.08, 0.08)
        L = rng.uniform(2.0, 2.7)
        ang = clear_angle(x, h, L, rng.uniform(-1.2, 1.0))
        lock(x, h, L, rng.uniform(1.3, 1.6), ang, light, rng, crimps, visible=1.9)
    crimps.emit(sw=0.17, lsw=0.16)
    # le bord de la laine de la joue, qui déborde un peu sur le blanc
    for (lx, ly) in ((9.4, -0.6), (9.5, 1.6), (9.3, 3.8), (9.2, 6.0), (9.6, 8.2), (10.4, 10.2), (11.4, 11.4)):
        x, h = HL(lx, ly)
        L = rng.uniform(1.4, 1.8)
        ang = clear_angle(x - 0.3, h + 0.3, L, rng.uniform(0.2, 0.6))
        lock(x - 0.3, h + 0.3, L, rng.uniform(1.25, 1.45), ang, 0.58 + 0.03 * (h - 88), rng, crimps)
    crimps.emit(sw=0.17, lsw=0.16)
    # le toupet : un dôme de laine blanche bouclée, ébouriffée, sur le front
    # large et la nuque, qui monte entre les cornes et descend jusqu'au haut
    # des lunettes, sans cacher les yeux (§ 5)
    TK = HLs([(-1.8, 3.2), (-2.6, 0.0), (-1.8, -3.0), (0.6, -4.9), (3.8, -6.0), (7.4, -6.3), (10.6, -5.9),
              (13.2, -5.1), (15.0, -4.3), (16.0, -3.4), (15.6, -2.4), (14.6, -1.5), (13.3, -0.85), (11.9, -0.45),
              (10.3, -0.35), (8.4, -0.6), (6.4, -0.3), (4.2, 0.0), (2.0, 0.2), (0.2, 0.6)])
    gt = D.rad("toupet", HL(5.5, -3.6), 9.8, [(0, "#EDE5D6"), (0.6, "#D5C8AF"), (1, "#B9A98C")])
    D.el(cr(TK, nd=1), fill=gt)
    TKD = dense(TK, True, 4)
    tk = hex_roots(TKD, 1.3, 1.45, rng, 0.3, jitter=0.35)
    tk.sort(key=lambda p: (p[1], p[0]))
    crown = HL(5.5, -3.0)
    eye_w = HL(*EYE_L)
    for (x, h) in tk:
        vx, vh = x - crown[0], h - crown[1]
        ang = max(-1.4, min(1.4, math.atan2(0.6 * vx, -(vh - 3.0)))) + rng.uniform(-0.5, 0.5)
        L = rng.uniform(1.9, 2.6)
        if math.hypot(x - eye_w[0], h - eye_w[1]) < 5.0:
            ang = rng.uniform(2.2, 3.0)        # près de l'œil, les boucles se relèvent vers l'arrière
        ang = clear_angle(x, h, L, ang)
        light = 0.72 + 0.05 * (h - crown[1]) + rng.uniform(-0.08, 0.08)
        lock(x, h, L, rng.uniform(1.3, 1.55), ang, light, rng, crimps, visible=1.8)
    # le bord du toupet, sur le front et au-dessus de l'œil : une frange de
    # boucles, pas un bord lisse (elles ne descendent pas sur les lunettes)
    rim = [(13.0, -4.8), (14.4, -4.1), (15.4, -3.3), (15.6, -2.5), (14.9, -1.7), (13.8, -1.1), (12.6, -0.75),
           (11.4, -0.55), (10.2, -0.5), (9.0, -0.65)]
    for (lx, ly) in rim:
        x, h = HL(lx, ly)
        out_dir = world_dir(0.55, 0.85)          # vers l'avant et le bas de la tête
        ang = math.atan2(out_dir[0], -out_dir[1]) + rng.uniform(-0.5, 0.5)
        L = rng.uniform(1.5, 2.1)
        ang = clear_angle(x, h, L, ang)
        lock(x, h, L, rng.uniform(1.25, 1.45), ang, 0.74 + rng.uniform(-0.06, 0.08), rng, crimps)
    crimps.emit(sw=0.17, lsw=0.16)


def toupet_front(rng):
    """Des boucles du toupet par-dessus la base de la corne (elle y est à
    demi noyée, § 6.2)."""
    crimps = Crimps()
    for (lx, ly) in ((8.0, -3.6), (10.2, -3.4), (6.4, -4.4), (9.2, -4.8), (11.2, -2.9), (7.2, -2.4), (5.0, -3.2),
                     (3.4, -3.9)):
        x, h = HL(lx, ly)
        lock(x, h + 0.4, rng.uniform(1.9, 2.5), rng.uniform(1.3, 1.5), rng.uniform(1.6, 2.9),
             0.78 + rng.uniform(-0.06, 0.08), rng, crimps)
    crimps.emit(sw=0.17, lsw=0.16)


# ------------------------------------------------------------------ les cornes

# L'axe de la corne du devant (repère de la tête) : de la base, sur le haut
# du crâne, vers l'arrière et un peu vers le haut ; elle descend derrière
# l'oreille, revient sous l'oreille à hauteur de la joue, et son dernier quart
# se dégage vers nous, la pointe vers l'avant et un peu vers le bas (§ 6.2).
# La boucle, presque ronde et ouverte vers l'avant, va du dessus du crâne à
# peu près au niveau de la bouche : 0,55 à 0,6 de la longueur de la tête.
HORN_AXIS = [(9.2, -1.2), (7.3, -2.6), (4.9, -3.3), (2.3, -3.2), (0.0, -2.3), (-1.8, -0.6), (-2.9, 1.7),
             (-3.1, 4.3), (-2.3, 6.8), (-0.6, 8.8), (1.8, 10.2), (4.6, 10.8), (7.5, 10.7), (10.2, 10.1),
             (12.6, 9.3), (14.0, 8.8), (15.0, 8.4)]
# Large et épaisse à la base (P1, P3 ; § 6.2 : ≈ 0,15 de la tête), elle
# s'amincit régulièrement : la boucle est serrée, son centre petit.
HORN_W = [6.0, 5.8, 5.6, 5.35, 5.1, 4.85, 4.6, 4.35, 4.05, 3.7, 3.3, 2.85, 2.35, 1.75, 1.1, 0.35]
HORN_CENTER_L = (3.2, 3.6)


def horn_geometry(offset=(0.0, 0.0), scale=1.0):
    C = dense(HLs(HORN_AXIS), closed=False, per=8)
    C = [(NUQUE[0] + (x - NUQUE[0]) * scale + offset[0], NUQUE[1] + (h - NUQUE[1]) * scale + offset[1]) for (x, h) in C]
    if VARIANTE == "brebis":
        # La brebis : plus fines, un demi-tour à trois quarts de tour (§ 1.4).
        C = C[:int(len(C) * .5)]
        scale *= .58
    n = len(C)
    W = []
    for i in range(n):
        u = i / (n - 1) * (len(HORN_W) - 1)
        k = min(int(u), len(HORN_W) - 2)
        f = u - k
        W.append((HORN_W[k] * (1 - f) + HORN_W[k + 1] * f) * scale)
    acc = [0.0]
    for i in range(1, n):
        acc.append(acc[-1] + math.dist(C[i], C[i - 1]))
    outer, inner, N = [], [], []
    center = HL(*HORN_CENTER_L)
    for i in range(n):
        a, b = C[max(i - 1, 0)], C[min(i + 1, n - 1)]
        dx, dh = b[0] - a[0], b[1] - a[1]
        L = math.hypot(dx, dh) or 1.0
        nx, nh = -dh / L, dx / L
        # la normale vers l'extérieur de la spirale
        if (C[i][0] - center[0]) * nx + (C[i][1] - center[1]) * nh < 0:
            nx, nh = -nx, -nh
        N.append((nx, nh))
        outer.append((C[i][0] + nx * W[i] / 2, C[i][1] + nh * W[i] / 2))
        inner.append((C[i][0] - nx * W[i] / 2, C[i][1] - nh * W[i] / 2))
    return C, W, N, acc, outer, inner


def far_horn():
    """La corne de l'autre côté : cachée derrière la tête, seul l'arc du
    dessus dépasse au-dessus du toupet (§ 6.2, 6)."""
    C, W, N, acc, outer, inner = horn_geometry(offset=(-0.6, 0.9), scale=0.98)
    m = int(len(C) * 0.42)
    g = D.lin("corne-loin", (C[0][0], C[0][1] + 3), (C[0][0], C[0][1] - 4),
              [(0, shade(HORN_LIGHT, 0.72)), (1, shade(HORN_SHADOW, 0.72))])
    D.el(cr(outer[:m:2] + [outer[m]] + inner[m::-2] + [inner[0]], nd=2), fill=g)
    rings = []
    s = 0.6
    while s < acc[m] - 0.4:
        i = next(k for k in range(len(acc)) if acc[k] >= s)
        o = (C[i][0] + N[i][0] * W[i] * 0.45, C[i][1] + N[i][1] * W[i] * 0.45)
        n_ = (C[i][0] - N[i][0] * W[i] * 0.2, C[i][1] - N[i][1] * W[i] * 0.2)
        rings.append(cr([o, n_], closed=False, nd=2))
        s += 0.7
    D.el("".join(rings), stroke=shade(HORN_STREAK, 0.85), sw=0.12, so=0.55, cap="round")
    D.el(cr(outer[:m:2] + [outer[m]], closed=False, nd=2), stroke=shade(HORN_DEEP, 0.8), sw=0.2, so=0.7, cap="round")


def near_horn(rng):
    """La corne du devant : large et aplatie à la base, presque triangulaire,
    annelée de crêtes serrées (une tous les 5 à 8 mm) sauf sur la pointe,
    lisse et plus claire ; brun caramel, plus foncée dans les creux, avec des
    stries foncées en long (§ 6.3)."""
    C, W, N, acc, outer, inner = horn_geometry()
    n = len(C)
    total = acc[-1]
    center = HL(*HORN_CENTER_L)
    # son ombre sur la joue : la pointe passe à une main de la joue (§ 6.2),
    # elle ne la touche pas
    j0 = int(n * 0.62)
    sh = [(inner[i][0] + 0.9, inner[i][1] - 1.1) for i in range(j0, n)]
    so_ = [(outer[i][0] + 0.9, outer[i][1] - 1.1) for i in range(j0, n)]
    D.el(cr(sh + so_[::-1], nd=2), fill="#3A3020", fo=0.2)
    gh = D.rad("corne", center, 10.0, [(0, HORN_DEEP), (0.3, HORN_SHADOW), (0.55, HORN), (0.82, HORN_LIGHT), (1, HORN)])
    shape = outer + inner[::-1]
    D.el(cr(shape[::2] + [shape[-1]], nd=2, t=0.9), fill=gh)
    # la face du dessus de la section, éclairée, le long du bord extérieur
    lit = range(2, int(n * 0.72))
    up = [(C[i][0] + N[i][0] * W[i] * 0.44, C[i][1] + N[i][1] * W[i] * 0.44) for i in lit]
    mid = [(C[i][0] + N[i][0] * W[i] * 0.02, C[i][1] + N[i][1] * W[i] * 0.02) for i in lit]
    D.el(cr(up[::2] + mid[::-2], nd=2), fill=HORN_LIGHT, fo=0.32)
    # le creux, sur la face qui regarde la tête
    dn = [(C[i][0] - N[i][0] * W[i] * 0.48, C[i][1] - N[i][1] * W[i] * 0.48) for i in range(2, n - 6)]
    md = [(C[i][0] - N[i][0] * W[i] * 0.1, C[i][1] - N[i][1] * W[i] * 0.1) for i in range(2, n - 6)]
    D.el(cr(dn[::2] + md[::-2], nd=2), fill=HORN_DEEP, fo=0.4)
    # les stries en long, foncées
    streaks = []
    for f in (-0.25, 0.05, 0.28):
        pts = [(C[i][0] + N[i][0] * W[i] * f, C[i][1] + N[i][1] * W[i] * f) for i in range(3, int(n * 0.8), 3)]
        streaks.append(cr(pts, closed=False, nd=2))
    D.el("".join(streaks), stroke=HORN_STREAK, sw=0.16, so=0.3, cap="round")
    # les annelures : des crêtes en travers, serrées, un creux sombre et une
    # crête claire, sur toute la corne sauf les 6 derniers centimètres
    dark, light = [], []
    s = 0.35
    while s < total - 6.0:
        i = max(1, min(n - 2, next(k for k in range(n) if acc[k] >= s)))
        nx, nh = N[i]
        dx, dh = C[i + 1][0] - C[i - 1][0], C[i + 1][1] - C[i - 1][1]
        L = math.hypot(dx, dh) or 1.0
        tx, th = dx / L, dh / L
        w = W[i] * 0.5
        bow = 0.2 + 0.06 * w
        wob = rng.uniform(-0.1, 0.1)
        p1 = (C[i][0] + nx * w * 0.97, C[i][1] + nh * w * 0.97)
        p2 = (C[i][0] - nx * w * 0.97, C[i][1] - nh * w * 0.97)
        pm = (C[i][0] + tx * (bow + wob), C[i][1] + th * (bow + wob))
        dark.append(quad(p1, (2 * pm[0] - (p1[0] + p2[0]) / 2, 2 * pm[1] - (p1[1] + p2[1]) / 2), p2, nd=2))
        q1 = (p1[0] + tx * 0.2, p1[1] + th * 0.2)
        q2 = (C[i][0] - nx * w * 0.3 + tx * 0.2, C[i][1] - nh * w * 0.3 + th * 0.2)
        qm = (pm[0] + tx * 0.2, pm[1] + th * 0.2)
        light.append(quad(q1, (2 * qm[0] - (q1[0] + q2[0]) / 2, 2 * qm[1] - (q1[1] + q2[1]) / 2), q2, nd=2))
        s += rng.uniform(0.5, 0.75) * (1.0 + 0.15 * s / total)
    D.el("".join(dark), stroke=HORN_DEEP, sw=0.14, so=0.72, cap="round")
    D.el("".join(light), stroke=HORN_LIGHT, sw=0.11, so=0.75, cap="round")
    # la pointe, lisse et plus claire
    m = next(k for k in range(n) if acc[k] >= total - 7.0)
    gt = D.lin("pointe", C[m], C[-1], [(0, HORN_TIP, 0.0), (0.35, HORN_TIP), (1, HORN_TIP_LIGHT)])
    D.el(cr(outer[m:] + inner[m:][::-1], nd=2), fill=gt)
    D.line([(C[i][0] + N[i][0] * W[i] * 0.2, C[i][1] + N[i][1] * W[i] * 0.2) for i in range(m, n - 2)],
           HORN_TIP_LIGHT, 0.3, so=0.6)
    # la quille du bord d'attaque, et le contour, sombre, le long des bords
    keel = [(C[i][0] + N[i][0] * W[i] * 0.4, C[i][1] + N[i][1] * W[i] * 0.4) for i in range(2, n - 10)]
    D.line(keel[::2], HORN_LIGHT, 0.3, so=0.5)
    D.el(cr(shape[::2] + [shape[-1]], nd=2, t=0.9), stroke=HORN_DEEP, sw=0.22, so=0.75, join="round")


# ================================================================== la lèvre

def gum():
    """Le bourrelet dentaire (le mouton n'a pas d'incisives en haut), caché
    sous la lèvre tant qu'elle est baissée : ce que découvre le flehmen. Il
    reste au-dessus de la ligne de la bouche."""
    Q = A.Q
    pad = HLs([(25.2, 4.6), (27.0, 4.2), (28.6, 3.95), (29.4, 3.9), (29.85, 4.15), (29.6, 4.6), (28.8, 4.9),
               (27.2, 5.2), (25.8, 5.42), (24.8, 5.45)])
    return [ET.Element(Q + "path", {"d": cr(pad, nd=2), "fill": GUM}),
            ET.Element(Q + "path", {"d": cr(HLs([(25.4, 5.25), (27.2, 5.0), (28.8, 4.7), (29.6, 4.4)]), closed=False,
                                            nd=2), "fill": "none", "stroke": GUM_DARK, "stroke-width": "0.2",
                                    "stroke-linecap": "round"}),
            ET.Element(Q + "path", {"d": cr(HLs([(25.4, 4.65), (27.2, 4.3), (28.8, 4.05), (29.6, 4.1)]), closed=False,
                                            nd=2), "fill": "none", "stroke": "#D4AAA8", "stroke-width": "0.15",
                                    "stroke-opacity": "0.7", "stroke-linecap": "round"})]


# ================================================================== assemblage

def articulate(text):
    """Le dessin recoupé : les pattes au carpe et au jarret (le haut garde un
    disque autour de l'articulation), la lèvre supérieure détachée de la tête,
    le bourrelet dentaire dessous."""
    parser = ET.XMLParser(target=ET.TreeBuilder(insert_comments=True))
    root = ET.fromstring(text, parser=parser)
    paints = A.Paints(root)
    for name, h in (("leg-back-far", JARRET), ("leg-front-far", CARPE), ("leg-back-near", JARRET),
                    ("leg-front-near", CARPE)):
        joint, half = A.leg_joint(text, name, OY - h)
        A.split_leg(root, paints, name, joint, half + 0.4, (0.0, 1.0))
    lip = [S(*p) for p in HLs(LIP_L)]
    # la lèvre est tout entière dans le noir du museau : ce qu'elle emporte de
    # dessous (le blanc de la face, son modelé, son poil) est caché par ce
    # noir ; on ne le garde pas, sans quoi son bord, coupé deux fois au même
    # endroit, laisserait un fil clair le long de la coupe au repos
    lifted = A.lift_part(root, paints, "head", lip, "lip", S(*HL(*LIP_PIVOT_L)), under=gum())
    kids = list(lifted)
    first = next(i for i, el in enumerate(kids) if el.get("fill") == "url(#museau)")
    for el in kids[:first]:
        lifted.remove(el)
    ET.indent(root, space=" ")
    return ET.tostring(root, encoding="unicode") + "\n"


def main(path):
    D.comment("Pattes : celles du fond d'abord, toutes sous la toison")
    hindleg(HIND_FAR, near=False)
    foreleg(FORE_FAR, near=False)
    tail()
    hindleg(HIND_NEAR, near=True)
    foreleg(FORE_NEAR, near=True)
    neck()
    body()
    head()
    D.pivot("ground", (HIND_NEAR + FORE_NEAR) / 2 + 2.0, 0.0)
    W, H = 132, 108
    svg = (f'<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 {W} {H}" width="{W}" height="{H}">\n'
           "<!--\n"
           "  Thônes et Marthod : un bélier de quatre ans en toison d'été (trois mois de\n"
           "  pousse), de profil, tourné vers la droite. 1 unité = 1 cm de l'animal\n"
           f"  ({GARROT:.0f} cm au garrot). Lunettes noires séparées du museau noir, oreilles\n"
           "  et chaussettes noires, cornes brun caramel en large spirale horizontale,\n"
           "  d'après docs/races/thones-et-marthod.md ; écrit par\n"
           "  tools/races/thones-et-marthod/gen_thones_et_marthod.py.\n"
           "  © 2026 Alejandro San Martin Lamas — MIT OR Apache-2.0\n"
           "-->\n"
           "<defs>" + "".join(D.defs) + "</defs>\n" + "\n".join(D.out) + "\n</svg>\n")
    svg = articulate(svg)
    if VARIANTE:
        return variante(path, svg)
    with open(path, "w") as f:
        f.write(svg)
    print(f"{path} : {len(svg.encode()) / 1024:.0f} Ko, {svg.count('<path')} tracés")


def variante(path, svg):
    """Les autres dessins : ce qui change, puis les proportions."""
    root = ET.fromstring(svg)
    pass
    variantes.ecrire(path, root, **PROPORTIONS[VARIANTE])


if __name__ == "__main__":
    main(sys.argv[1] if len(sys.argv) > 1 else "thones-et-marthod.svg")
