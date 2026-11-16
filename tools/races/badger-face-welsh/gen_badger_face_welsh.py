#!/usr/bin/env python3
"""Génère le rig SVG du Badger Face Welsh Mountain, type Torddu (« ventre
noir ») : un bélier cornu de trois ans en fin d'été (trois à quatre mois de
laine depuis la tonte), de profil, tourné vers la droite, d'après la fiche
docs/races/badger-face-welsh.md.

Ce qui le fait reconnaître (fiche, § 9.1) : un petit mouton de colline bas
sur pattes, en tonneau, au cou court et épais ; un corps crème souligné d'une
bande noire continue, du dessous de la mâchoire à la gorge, au poitrail, au
ventre, aux culottes et sous la queue jusqu'au bout ; une frange fauve-rousse
le long du noir de l'encolure et du poitrail ; la face crème-fauve barrée de
deux bandes noires qui enferment l'œil et descendent vers le nez ; le museau
et la mâchoire noirs ; de petites oreilles noires ; des cornes sombres d'à
peu près un tour ; des pattes noires rayées de fauve devant ; une queue
entière, crème dessus, noire dessous.

Photos regardées (jamais copiées dans le dépôt ; voir la fiche, § 10.2) :
les profils de béliers 1 et 2 (proportions, bande noire, pattes, cornes), la
tête 11 (les bandes de la face, la lunette, la mâchoire noire, l'iris), la
planche du standard 12 (oreilles, queue, culottes, rayures des pattes).

Repère de travail : le « monde », en centimètres de l'animal réel. x vers
l'avant (0 à l'arrière de la culotte), h vers le haut (0 au sol). Dans le SVG,
1 unité = 1 cm : X = x + OX, Y = OY - h (voir outils.py). Les outils et la
façon de faire viennent du générateur de la Thônes et Marthod : les mèches en
tire-bouchons (ici plus courtes et serrées, la toison « en chou-fleur »), la
tête dans son propre repère, la corne le long d'un axe, l'encolure à part
sous le corps ; puis tools/races/articulations.py recoupe chaque patte au
carpe ou au jarret et détache la lèvre supérieure de la tête.

Usage : gen_badger_face_welsh.py <sortie.svg>
        python3 tools/races/variantes.py badger-face-welsh   # la brebis et l'agneau

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
from outils import (OY, S, Doc, cr, dense, dist_to, ellipse_pts, inside, mix, num, poly,  # noqa: E402
                    quad, ramp, ringlet, runs_path, shade, vspan)
from shapely.geometry import Point, Polygon  # noqa: E402

# ------------------------------------------------------------------ mesures (fiche, § 2)

# Les autres dessins de la race (tools/races/variantes.py), d'après la fiche
# (§ 1.5 et 2.2). Sans variante : le bélier cornu.
VARIANTE = sys.argv[2] if len(sys.argv) > 2 else ""
PROPORTIONS = {
    "brebis": dict(  # Brebis Torddu : 58 cm au garrot, sans cornes, tête plus fine, cou moins épais, petite mamelle noire.
        garrot=(64, 58), encolure=(1.0, .85), tete=(1.03, .94)),
    "agneau": dict(  # Agneau Torddu de deux semaines : 35 cm au garrot, né avec le motif en place, sans cornes.
        variantes.AGNEAU, garrot=(64, 35)),
}

GARROT = 64.0          # H : du sol au dessus de la toison au garrot (§ 2.2 : 64 cm [E])
STERNUM = 23.0         # dégagement sous le sternum, toison comprise (0,36 H)
VENTRE = 19.0          # dégagement sous le ventre (0,30 H)
TETE = 22.0            # HL, de la nuque au bout du nez (0,34 H)
OREILLE = 7.6          # petite : 0,35 HL (§ 5)
CARPE = 13.4           # centre du carpe (0,21 H, § 2.6)
JARRET = 17.3          # pointe du jarret (0,27 H)
COURONNE = 2.4         # couronne de l'onglon : petit onglon
LAINE_AV = 21.5        # l'avant-bras sort de la toison à 0,30-0,36 H (§ 2.6)
LAINE_AR = 20.6        # la jambe est presque toute dans la culotte, jusqu'au-dessus du jarret
QUEUE_BOUT = 16.0      # le bout de la queue au niveau du jarret ou un peu dessous (§ 2.2)
# La nuque : 1,24 H du sol ; l'encolure, du garrot à la nuque, fait 0,40 H
# et monte à 38° au repos (§ 2.5 : 35-45°).
NUQUE = (80.0, 79.5)
# L'axe de la tête, de la nuque au bout du nez, sous l'horizontale : la tête
# en coin, le profil droit (§ 5).
HEAD_TILT = math.radians(42.0)

# Les pattes (x du canon) : les antérieures sous le garrot, les postérieures
# sous la pointe de la fesse ; celles du fond décalées pour qu'on voie les
# quatre. Le canon est fin sans être grêle (§ 7) : 2,7 cm.
FORE_NEAR, FORE_FAR = 66.0, 60.0
HIND_NEAR, HIND_FAR = 11.5, 17.5
# Les épaules et les hanches (pivots des pattes), dans le corps.
SHOULDER_H, HIP_H = 38.0, 42.0
# La base de l'encolure, bas dans l'épaule : d'ici, le museau atteint l'herbe
# devant les antérieurs (le test `troupeau` le vérifie).
NECK_BASE = (73.0, 44.0)

# ------------------------------------------------------------------ palette (fiche, § 4.6)

CREAM_LIGHT, CREAM, CREAM_SHADOW, CREAM_DEEP = "#ECE7E1", "#E3DCD3", "#B09680", "#8F7B62"
FAWN_LIGHT, FAWN, FAWN_SHADOW = "#D8B48E", "#B88A62", "#7E5638"
WOOL_BLACK_LIGHT, WOOL_BLACK, WOOL_BLACK_DEEP = "#3A3740", "#201D1E", "#0D0B0B"
HAIR_BLACK, HAIR_BLACK_SHEEN = "#1C1C22", "#3A3A44"
TAN_LIGHT, TAN, TAN_SHADOW = "#A9754F", "#8A5A32", "#5E3C20"
FACE_LIGHT, FACE, FACE_SHADOW, FACE_DEEP = "#F2ECE2", "#D9CBBA", "#A9A69E", "#8A8278"
NOSE, NOSE_SHEEN, NOSTRIL = "#2A2A30", "#555E6E", "#0E0E10"
HORN_LIGHT, HORN, HORN_SHADOW, HORN_DEEP = "#5A5A66", "#34343C", "#26262C", "#151418"
HORN_TIP, HORN_TIP_LIGHT = "#4A474D", "#625E66"
HOOF_LIGHT, HOOF, HOOF_DEEP = "#3A3638", "#1E1C1C", "#0C0B0B"
IRIS_LIGHT, IRIS, IRIS_DARK, PUPIL = "#B78757", "#744D31", "#5C4221", "#0E0C0B"
CATCH = "#F2EEE4"
GUM, GUM_DARK = "#8E6E70", "#5E4648"
EAR_INSIDE = "#2A2627"
# Le fond des creux entre les mèches, sous le crème (dégradé commun au corps,
# à l'encolure et aux pattes : au repos, aucune jointure ne se voit).
DEEP_TOP, DEEP_MID, DEEP_LOW = "#D4C7B6", "#B09A84", CREAM_DEEP

D = Doc()

# ------------------------------------------------------------------ les zones de couleur

# Le patron du Torddu (§ 4.1, 4.2) : tout le dessous est noir, d'un seul
# tenant. La limite haute de la bande du ventre, de l'avant à l'arrière : elle
# monte devant jusqu'à la pointe de l'épaule (poitrail noir), descend en une
# bande de 8 cm sous le flanc, puis remonte derrière la cuisse (la culotte)
# jusque sous l'attache de la queue.
BELLY_TOP = [(84.0, 40.5), (80.0, 39.0), (77.6, 35.4), (74.6, 31.6), (70.0, 29.0), (63.0, 27.8), (55.0, 27.2),
             (46.0, 26.8), (37.0, 26.9), (30.0, 27.6), (25.0, 28.8), (21.0, 30.6), (16.5, 31.8), (11.5, 32.2),
             (7.5, 33.0), (4.6, 35.4), (3.4, 39.8), (3.2, 44.8), (3.0, 49.0), (2.0, 52.4)]
# La limite de la bande de la gorge : tout ce qui est devant est noir, de la
# mâchoire au poitrail, où elle rejoint la bande du ventre (§ 4.2).
THROAT = [(78.0, 30.0), (76.6, 34.0), (75.6, 40.0), (76.0, 46.0), (76.8, 52.0), (77.8, 58.0), (79.1, 64.0),
          (80.6, 69.0), (82.4, 74.0), (84.6, 79.0)]
FRINGE = 3.4           # la frange fauve-rousse le long du noir de l'encolure et du poitrail (2 à 5 cm)

BELLY_POLY = Polygon(BELLY_TOP + [(-6.0, 52.4), (-6.0, -2.0), (90.0, -2.0), (90.0, 40.5)]).buffer(0)
THROAT_POLY = Polygon([(x, h) for (x, h) in THROAT] + [(120.0, 79.0), (120.0, 30.0)]).buffer(0)
BLACK_POLY = BELLY_POLY.union(THROAT_POLY)
# La frange : une bande le long de la limite de la gorge et du haut du
# poitrail, du côté crème.
FRINGE_POLY = THROAT_POLY.buffer(FRINGE).union(
    Polygon([(66.0, 27.0), (72.0, 29.0), (77.5, 34.5), (80.5, 39.0), (84.0, 40.5), (84.0, 45.0), (79.5, 43.8),
             (76.0, 39.5), (72.0, 34.0), (66.0, 31.0)]).buffer(0)).difference(BLACK_POLY)


def zone(x, h):
    """La couleur de la laine à cet endroit : « noir », « fauve » ou « creme »."""
    p = Point(x, h)
    if BLACK_POLY.contains(p):
        return "noir"
    if FRINGE_POLY.contains(p):
        return "fauve"
    return "creme"


# ------------------------------------------------------------------ la toison

STOPS = {
    "creme": [(0.0, CREAM_SHADOW), (0.55, CREAM), (1.0, CREAM_LIGHT)],
    "fauve": [(0.0, FAWN_SHADOW), (0.55, FAWN), (1.0, FAWN_LIGHT)],
    "noir": [(0.0, WOOL_BLACK_DEEP), (0.6, WOOL_BLACK), (1.0, WOOL_BLACK_LIGHT)],
}


def wool(light, z="creme"):
    return ramp(light, STOPS[z])


def bucket(light, n=10):
    """La lumière ramenée à quelques niveaux : les mèches d'un même niveau
    partagent la teinte de leurs spires (un seul tracé par teinte)."""
    return round(max(0.0, min(1.0, light)) * (n - 1)) / (n - 1)


def crimp_colors(base, z):
    """Le creux d'ombre sous chaque tour de la mèche, et le reflet sur son
    dessus : sur la laine noire, un reflet gris-bleu, éteint (le noir reste
    noir)."""
    if z == "noir":
        return shade(base, 0.55), mix(base, "#8D8A96", 0.38)
    return shade(base, 0.72), mix(base, "#FFFFFF", 0.78)


class Crimps:
    """Une couche de mèches : leurs contours, remplis dans l'ordre, puis
    leurs spires (le creux d'ombre sous chaque tour, le reflet sur son
    dessus), rangées par teinte et tracées d'un seul trait par teinte. On ne
    garde que les spires qu'aucune mèche posée après dans la couche ne
    recouvre (comme chez la Thônes et Marthod)."""

    def __init__(self):
        self.locks = []

    def add(self, fill, outline, dk, lt, darks, lights):
        self.locks.append((fill, outline, dk, lt, darks, lights))

    def emit(self, sw=0.18, so=0.5, lsw=0.17, lso=0.8):
        cell = 2.5
        grid = {}
        boxes = []
        for i, lk in enumerate(self.locks):
            out = lk[1]
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
        for (fill, out, _, _, _, _) in self.locks:
            D.el(cr(out, nd=1, t=0.85), fill=fill)
        for i, (_, _, dk, lt, darks, lights) in enumerate(self.locks):
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


def lock(x, h, L, w, ang, light, rng, crimps, z=None, pitch=None, bend=None, hook=None, dim=1.0):
    """Une mèche en tire-bouchon serré, de la couleur de sa zone (prise à sa
    racine si `z` n'est pas donnée)."""
    z = z or zone(x, h - 0.35 * L)
    pitch = pitch or rng.uniform(0.85, 1.05)
    bend = rng.uniform(-0.35, 0.35) if bend is None else bend
    hook = rng.choice((-1, 1)) * rng.uniform(0.4, 1.3) if hook is None else hook
    outline, darks, lights = ringlet(x, h, L, w, ang, rng, pitch=pitch, bend=bend, hook=hook)
    base = wool(bucket(light), z)
    if dim != 1.0:
        base = shade(base, dim)
    dk, lt = crimp_colors(base, z)
    crimps.add(shade(base, 1.0 + rng.uniform(-0.025, 0.025)), outline, dk, lt, darks, lights)


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


def zone_fond(gid, outline, inset_by, dim=1.0):
    """Le fond des creux, sous les mèches : crème dégradé, puis la bande noire
    et la frange fauve par-dessus, rognées au même contour rentré."""
    pts = inset(outline, inset_by)
    k = dim
    D.lin(gid, (0, 80), (0, 18), [(0, shade(DEEP_TOP, k)), (0.5, shade(DEEP_MID, k)), (1, shade(DEEP_LOW, k))])
    D.el(poly(pts, nd=1), fill=f"url(#{gid})")
    area = Polygon(pts).buffer(0)
    for poly_z, col in ((FRINGE_POLY, shade("#7A5A40", k)), (BLACK_POLY, shade("#141213", k))):
        part = area.intersection(poly_z)
        for g in getattr(part, "geoms", [part]):
            if g.geom_type == "Polygon" and g.area > 0.05:
                D.el(poly(list(g.simplify(0.08).exterior.coords)[:-1], nd=1), fill=col)


# ================================================================== le corps

# La silhouette de la toison du corps (sans l'encolure), dans le sens des
# aiguilles d'une montre depuis l'attache de la queue : une croupe courte et
# arrondie qui tombe vers une attache de queue un peu basse, le dos presque
# droit, le garrot noyé dans la toison, la racine du cou ; le devant qui
# recouvre la base de l'encolure et le poitrail bombé en avant des
# antérieurs ; le ventre rond, presque droit ; le flanc qui remonte vers la
# cuisse (§ 2.4 : un tonneau sur des pattes courtes).
BODY = [
    (3.6, 52.6), (6.4, 57.2), (10.4, 60.4), (15.8, 62.4), (22.5, 63.2), (30.0, 63.4), (38.0, 63.3),
    (46.0, 63.2), (52.5, 63.4), (57.2, 64.0), (60.6, 65.0), (63.2, 66.0),
    (66.0, 65.5), (69.4, 63.2), (72.8, 59.4), (75.9, 54.6), (78.5, 49.2), (80.4, 43.6), (81.3, 38.0),
    (81.0, 32.6), (79.5, 28.2), (76.8, 25.2), (73.0, 23.5), (68.0, 22.8),
    (62.0, 21.8), (54.0, 20.3), (45.0, 19.4), (36.0, 19.2), (29.0, 19.7), (24.6, 21.2),
    (21.2, 24.2), (17.6, 27.6), (13.6, 30.2), (9.2, 31.4), (5.2, 33.0), (2.2, 36.2), (0.4, 40.6),
    (0.0, 45.2), (1.0, 49.4),
]
BODY_D = dense(BODY, True, 8)


def staples(x, h):
    """Les paquets de mèches : une variation douce de la lumière, à l'échelle
    de quelques centimètres (la toison « en chou-fleur », § 3.2)."""
    return 0.08 * math.sin(x / 3.9 + h / 3.1) * math.sin(h / 4.1 - x / 6.3 + 1.3) + 0.05 * math.sin(x / 2.3 - h / 4.7)


def packs(x, h):
    """L'inclinaison des paquets de mèches : un champ lisse, de 4 à 6 cm, pour
    que les mèches voisines penchent ensemble et que les paquets voisins
    penchent autrement."""
    return 0.45 * math.sin(x / 3.3 + 1.6 * math.sin(h / 4.6)) + 0.3 * math.sin(h / 2.9 - x / 5.7 + 0.8)


def body_light(x, h):
    sp = vspan(BODY_D, x)
    hb, ht = sp if sp else (19.0, 63.0)
    tv = max(0.0, min(1.0, (h - hb) / max(1.0, ht - hb)))
    light = 0.14 + 0.84 * tv ** 0.75 + staples(x, h)
    if x > 74 and h < 56:
        light -= 0.12 * min(1.0, (x - 74) / 7.0)
    if x < 4:
        light -= 0.06
    return light


def body():
    rng = random.Random(1976)  # la société de race est fondée en novembre 1976 (BFS)
    D.comment("Corps : la toison de fin d'été, serrée et bouclée, crème dessus, noire dessous")
    D.open("body")
    # le scrotum d'abord, sous la toison : il suit le corps quand il se
    # couche, se cabre ou respire
    if not VARIANTE:
        scrotum()
    # le fond : les creux entre les mèches, un peu en retrait du bord (ce
    # sont les pointes des mèches qui font le contour)
    zone_fond("fond", BODY_D, 0.9)
    crimps = Crimps()
    dh = 1.6
    roots = hex_roots(BODY_D, 1.35, dh, rng, 0.5, jitter=0.42)
    locks = []
    for (x, h) in roots:
        hb = vspan(BODY_D, x)[0]
        L = rng.uniform(2.4, 3.4) * (1.0 + 0.12 * math.sin(x / 4.9 - h / 3.4))
        bottom = h - dh < hb + 0.6
        if bottom:
            L = max(L, h - hb + rng.uniform(0.5, 1.0))
        # des mèches qui partent dans tous les sens, par paquets : une
        # surface grumeleuse, pas des mèches qui pendent
        ang = rng.uniform(-0.45, 0.45) + packs(x, h)
        if x < 8:
            ang -= 0.5 * (8 - x) / 8
        if x > 76 and h < 58:
            ang += 0.4 * min(1.0, (x - 76) / 6)
        locks.append((h, x, L, ang, bottom))
    locks.sort()
    for (h, x, L, ang, bottom) in locks:
        light = body_light(x, h - L * 0.4) + rng.uniform(-0.07, 0.07)
        lock(x, h, L, rng.uniform(1.5, 1.9), ang, light, rng, crimps,
             hook=rng.choice((-1, 1)) * rng.uniform(0.9, 1.7) if bottom else None)
    crimps.emit()
    # le bord : des bouts de mèches sur le dos, la croupe et le devant
    edge_locks(BODY_D, 1.05, rng, crimps,
               select=lambda x, h: (h > 54 and x < 68) or (x < 2.0 and h > 36),
               length=(2.0, 2.9), width=(1.45, 1.8), light_of=lambda x, h: body_light(x, h) + 0.04,
               droop=0.4, root_in=1.9)
    edge_locks(BODY_D, 1.3, rng, crimps,
               select=lambda x, h: x > 68 and h > 26,
               length=(2.2, 3.0), width=(1.4, 1.75), light_of=lambda x, h: body_light(x, h) - 0.03,
               droop=0.7, root_in=1.5)
    crimps.emit()
    # la jarre : quelques poils plus raides et plus brillants qui dépassent
    # du dos (« pure or white with some kemp », § 3.1)
    jarre = []
    n = len(BODY_D)
    for i in range(0, n, 2):
        x, h = BODY_D[i]
        if not (6 < x < 62 and h > 60) or rng.random() < 0.65:
            continue
        a, b = BODY_D[(i - 2) % n], BODY_D[(i + 2) % n]
        tx, th = b[0] - a[0], b[1] - a[1]
        L = math.hypot(tx, th) or 1.0
        nx, nh = -th / L, tx / L
        ln = rng.uniform(1.2, 2.2)
        bnd = rng.uniform(-0.4, 0.4)
        pts = [(x - nx * 0.5, h - nh * 0.5), (x + nx * ln * 0.5 - nh * bnd * 0.3, h + nh * ln * 0.5 + nx * bnd * 0.3),
               (x + nx * ln - nh * bnd, h + nh * ln + nx * bnd)]
        jarre.append(cr(pts, closed=False, nd=1))
    D.el("".join(jarre), stroke="#FBF8F0", sw=0.06, so=0.7, cap="round")
    D.pivot("body", 42.0, 19.0)
    D.close()


# ================================================================== l'encolure

# L'encolure, à part, sous le corps : courte, forte et épaisse (§ 2.5), elle
# se fond dans les épaules ; la gorge noire descend de sous la mâchoire au
# poitrail. Sous la tête, une épaisseur cachée au repos garde l'encolure
# épaisse quand le moteur l'allonge ; sous le corps, tout le bas autour de sa
# base.
NECK = [
    (62.6, 60.0), (63.4, 65.6), (65.8, 68.8), (69.0, 71.8), (72.6, 74.8), (76.0, 77.6), (79.0, 79.6),
    (81.6, 79.8), (84.0, 77.4), (85.6, 73.8), (85.6, 70.2), (84.4, 66.6), (83.0, 62.6), (82.2, 57.6),
    (82.0, 52.2), (82.2, 46.6), (82.4, 41.0), (80.6, 36.4), (76.4, 34.4), (71.0, 35.0), (66.6, 38.2),
    (63.8, 43.6), (62.6, 51.0),
]
NECK_D = dense(NECK, True, 6)


def neck_light(x, h):
    t = max(0.0, min(1.0, (h - 40) / 40.0))
    return 0.36 + 0.55 * t + staples(x, h)


def neck():
    rng = random.Random(2012)  # le timbre de Royal Mail (§ 1.2)
    D.comment("Encolure : sous le corps et sous la tête, qui la recouvrent à ses deux bouts")
    D.open("neck")
    zone_fond("fond-encolure", NECK_D, 0.8)
    crimps = Crimps()
    pts = hex_roots(NECK_D, 1.3, 1.5, rng, 0.9, jitter=0.35)
    pts.sort(key=lambda p: (p[1], p[0]))
    for (x, h) in pts:
        ang = rng.uniform(-1.1, 1.1) + (0.4 if x > 80 else 0.0)
        lock(x, h, rng.uniform(2.2, 3.0), rng.uniform(1.4, 1.75), ang, neck_light(x, h) + rng.uniform(-0.07, 0.07),
             rng, crimps)
    crimps.emit()
    # la crête et la gorge
    edge_locks(NECK_D, 1.3, rng, crimps,
               select=lambda x, h: (h > 66 and x < 80 and h < 81.0) or (x > 81 and 40 < h < 70.0),
               length=(2.1, 2.9), width=(1.4, 1.7), light_of=lambda x, h: neck_light(x, h) + 0.03,
               droop=0.25, root_in=1.4)
    crimps.emit()
    D.pivot("neck", *NECK_BASE)
    D.close()


# ================================================================== les pattes

def black_leg_paint(name, cx, far):
    """Le dégradé du poil ras noir d'une patte : un reflet bleuté sur l'arrière
    (la lumière vient d'en haut à gauche), le noir profond devant."""
    k = 0.8 if far else 1.0
    return D.lin(f"poil-{name}", (cx - 2.3, 0), (cx + 2.6, 0),
                 [(0, shade(HAIR_BLACK, 0.9 * k)), (0.2, shade(HAIR_BLACK_SHEEN, k)), (0.55, shade(HAIR_BLACK, k)),
                  (1, shade(WOOL_BLACK_DEEP, k))])


def hair_strokes(edges, n, rng, color, so, seed_len=(0.5, 1.1), sw=0.06):
    """Le poil ras, en traits fins le long de la patte : `edges` donne, à
    chaque hauteur, l'arrière et l'avant de la zone."""
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
    D.el("".join(out), stroke=color, sw=sw, so=so, cap="round")


def ragged_line(pts, rng, amp=0.07, step=0.2):
    """Un bord de poil net mais un peu irrégulier (la rayure fauve, la limite
    d'une bande) : la ligne rééchantillonnée, chaque point poussé de part et
    d'autre de quelques millimètres."""
    P = dense(pts, False, 10)
    out = [P[0]]
    acc = 0.0
    for a, b in zip(P, P[1:]):
        acc += math.dist(a, b)
        if acc >= step:
            out.append(b)
            acc = 0.0
    if out[-1] != P[-1]:
        out.append(P[-1])
    res = [out[0]]
    for i in range(1, len(out) - 1):
        p, q = out[i - 1], out[i + 1]
        tx, th = q[0] - p[0], q[1] - p[1]
        L = math.hypot(tx, th) or 1.0
        nx, nh = -th / L, tx / L
        k = amp * (0.6 * math.sin(i * 1.1) + 0.4 * math.sin(i * 2.7 + 1.0)) + rng.uniform(-0.4, 0.4) * amp
        res.append((out[i][0] + nx * k, out[i][1] + nh * k))
    res.append(out[-1])
    return res


def tan_stripe(name, front_edge, inner, rng, far):
    """La rayure fauve du devant de la patte (§ 4.3) : entre le bord avant de
    la patte et une ligne au tiers de sa largeur, au bord un peu irrégulier ;
    son poil ras en traits fins, plus clair sur le relief."""
    k = 0.8 if far else 1.0
    g = D.lin(f"fauve-{name}", (inner[0][0] - 0.3, 0), (front_edge[0][0] + 0.4, 0),
              [(0, shade(TAN_SHADOW, k)), (0.45, shade(TAN, k)), (0.8, shade(TAN_LIGHT, k)), (1, shade(TAN, k))])
    rim = ragged_line(inner, rng, amp=0.06)
    D.el(cr(front_edge + rim[::-1], nd=2, t=0.9), fill=g)
    return rim


def hoof(heel, toe, far, name):
    """L'onglon de profil, petit et serré, noir (§ 4.2, 7) ; le bout de l'autre
    doigt dépasse un peu devant."""
    g = D.lin(f"onglon-{name}", (heel, 0), (toe, 0), [(0, HOOF_DEEP), (0.45, HOOF), (0.85, HOOF_LIGHT if not far else HOOF),
                                                       (1, HOOF)])
    top = COURONNE
    other = [(toe - 1.6, top - 0.3), (toe - 0.7, 1.5), (toe + 0.25, 0.05), (toe - 1.2, 0.05), (toe - 1.9, 1.3)]
    D.el(cr(other, t=0.7), fill=HOOF_DEEP)
    wall = [(toe - 1.5, top), (toe - 0.75, 1.65), (toe - 0.25, 0.7), (toe, 0.05)]
    sole = [(toe, 0.05), (heel + 0.3, 0.05)]
    back = [(heel + 0.3, 0.05), (heel, 0.45), (heel - 0.08, 1.2), (heel + 0.25, 2.0), (heel + 1.0, top + 0.08),
            (toe - 1.5, top)]
    D.el(runs_path([wall, sole, back], nd=2), fill=g)
    D.line([(toe - 1.3, top - 0.15), (toe - 0.65, 1.5), (toe - 0.22, 0.3)], HOOF_LIGHT if not far else "#2A2728", 0.12,
           so=0.7)
    D.line([(heel + 0.4, 2.2), (toe - 1.6, top - 0.04)], "#3E3A3B" if not far else "#2A2728", 0.11, so=0.6)


def dewclaw(x, h, far):
    D.el(cr(ellipse_pts(x, h, 0.4, 0.3, 0.5, 8)), fill="#070606" if not far else "#050404")


def hidden_by_body(x, h, L, ang, depth=2.0):
    """Une mèche du haut d'une patte que le corps cache toujours : sa racine
    et sa pointe à plus de `depth` cm en dedans de la toison du corps."""
    tip = (x + L * math.sin(ang), h - L * math.cos(ang))
    return all(inside(BODY_D, *p) and dist_to(BODY_D, *p) > depth for p in ((x, h), tip))


def wool_sleeve(name, outline, rng, light_of, far, zone_of, length=(2.4, 3.2), dx=1.3, dh=1.6):
    """La laine du haut de la patte, qui s'arrête en festons de mèches
    au-dessus du genou ou du jarret ; `zone_of` dit la couleur de chaque mèche.
    Les mèches toujours cachées sous le corps ne sont pas dessinées (le fond
    reste, sous elles)."""
    pts = dense(outline, True, 6)
    crimps = Crimps()
    roots = hex_roots(pts, dx, dh, rng, 0.6)
    locks = []
    for (x, h) in roots:
        hb = vspan(pts, x)[0]
        L = rng.uniform(*length)
        bottom = h - dh < hb + 0.6
        if bottom:
            L = max(L, h - hb + rng.uniform(0.7, 1.4))
        locks.append((h, x, L, rng.uniform(-0.3, 0.3), bottom))
    locks.sort()
    for (h, x, L, ang, bottom) in locks:
        light = light_of(x, h - L * 0.5) + rng.uniform(-0.06, 0.06)
        w = rng.uniform(1.4, 1.75)
        if hidden_by_body(x, h, L, ang):
            continue
        lock(x, h, L, w, ang, light, rng, crimps, z=zone_of(x, h - 0.4 * L),
             hook=rng.choice((-1, 1)) * rng.uniform(0.9, 1.6) if bottom else None, dim=0.84 if far else 1.0)
    crimps.emit()


def sleeve_fond(name, outline, far, zone_of):
    """Le fond sous les mèches de la manche : noir, ou crème là où la laine est
    crème (le haut de la cuisse)."""
    kk = 0.84 if far else 1.0
    pts = inset(dense(outline, True, 6), 0.6)
    D.el(poly(pts, nd=1), fill=shade("#141213", kk))
    area = Polygon(pts).buffer(0)
    cream = []
    xs = [p[0] for p in pts]
    hs = [p[1] for p in pts]
    step = 0.5
    y = min(hs)
    while y < max(hs):
        x = min(xs)
        while x < max(xs):
            if zone_of(x + step / 2, y + step / 2) == "creme":
                cream.append(Polygon([(x, y), (x + step, y), (x + step, y + step), (x, y + step)]))
            x += step
        y += step
    if cream:
        from shapely.ops import unary_union
        part = unary_union(cream).buffer(0.3).buffer(-0.3).intersection(area)
        D.lin(f"fond-{name}", (0, 46), (0, 26), [(0, shade(DEEP_MID, kk)), (1, shade(DEEP_LOW, kk))])
        for g in getattr(part, "geoms", [part]):
            if g.geom_type == "Polygon" and g.area > 0.1:
                D.el(poly(list(g.simplify(0.1).exterior.coords)[:-1], nd=1), fill=f"url(#fond-{name})")


def foreleg(cx, near):
    name = "leg-front-near" if near else "leg-front-far"
    far = not near
    rng = random.Random(int(cx * 10) + 7)
    D.open(name)
    # l'avant-bras court, le carpe un peu plus large (les os du genou devant,
    # l'os crochu derrière), le canon fin, le boulet, le paturon court
    # incliné à 55-60° (§ 2.6)
    front = [(cx + 2.6, 38.0), (cx + 2.45, 30.0), (cx + 2.25, 22.0), (cx + 2.1, 17.8), (cx + 2.3, 15.8),
             (cx + 2.42, 14.2), (cx + 2.28, 12.5), (cx + 1.85, 11.0), (cx + 1.5, 9.4), (cx + 1.42, 7.4),
             (cx + 1.55, 6.0), (cx + 1.9, 5.0), (cx + 2.4, 4.0), (cx + 2.85, 3.2), (cx + 3.2, COURONNE + 0.15)]
    back = [(cx + 0.25, COURONNE + 0.25), (cx - 0.25, 3.6), (cx - 0.75, 4.6), (cx - 1.25, 5.8), (cx - 1.5, 7.1),
            (cx - 1.3, 8.6), (cx - 1.25, 10.4), (cx - 1.4, 11.8), (cx - 1.8, 12.8), (cx - 2.05, 14.0),
            (cx - 1.95, 15.4), (cx - 1.85, 17.4), (cx - 1.95, 22.0), (cx - 2.25, 30.0), (cx - 2.45, 38.0)]
    D.el(runs_path([front, [front[-1], back[0]], back + [(cx, 39.0), front[0]]], nd=2),
         fill=black_leg_paint(name, cx, far))
    k = 0.8 if far else 1.0
    edges = [(4.5, cx - 1.0, cx + 2.2), (8.0, cx - 1.3, cx + 1.4), (12.0, cx - 1.4, cx + 2.0),
             (14.2, cx - 2.0, cx + 2.35), (16.5, cx - 1.9, cx + 2.2), (21.5, cx - 2.0, cx + 2.2)]
    hair_strokes(edges, 70, rng, shade(HAIR_BLACK_SHEEN, 1.15 * k), 0.5)
    if near:
        # le fauve (§ 4.3) : l'avant-bras sort de la toison fauve-roux sur sa
        # face avant et extérieure (noir derrière) ; au genou, une tache noire
        # qui fait le tour, la rayure passant devant sans se couper ; sur le
        # canon, une rayure au tiers de la largeur, devant, jusqu'au boulet ;
        # le paturon, la couronne et l'onglon restent noirs
        fe = [(cx + 2.27, LAINE_AV + 1.0), (cx + 2.2, 19.0), (cx + 2.12, 17.6), (cx + 2.3, 15.8), (cx + 2.42, 14.2),
              (cx + 2.28, 12.5), (cx + 1.85, 11.0), (cx + 1.5, 9.4), (cx + 1.42, 7.4), (cx + 1.52, 6.1)]
        inner = [(cx - 1.05, LAINE_AV + 1.0), (cx - 1.0, 19.0), (cx - 0.6, 17.3), (cx + 0.35, 16.0),
                 (cx + 1.05, 14.6), (cx + 0.9, 12.8), (cx + 0.5, 11.0), (cx + 0.35, 9.2), (cx + 0.4, 7.4),
                 (cx + 0.9, 6.1)]
        tan_stripe(name, fe, inner, rng, far)
        hair_strokes([(6.5, cx + 0.6, cx + 1.4), (11.0, cx + 0.6, cx + 1.7), (14.2, cx + 1.2, cx + 2.2),
                      (17.0, cx - 0.3, cx + 2.0), (LAINE_AV, cx - 0.8, cx + 2.1)], 46, rng, TAN_LIGHT, 0.45)
        hair_strokes([(6.5, cx + 0.5, cx + 1.2), (11.0, cx + 0.5, cx + 1.3), (14.2, cx + 1.1, cx + 1.6),
                      (17.0, cx - 0.4, cx + 0.8), (LAINE_AV, cx - 0.8, cx + 0.6)], 28, rng, TAN_SHADOW, 0.4)
    # l'ombre derrière le canon et le genou ; le relief du genou ; le pli sous
    # les os du carpe
    D.line([(cx - 1.05, 7.0), (cx - 0.95, 10.0), (cx - 1.1, 12.0), (cx - 1.6, 13.4), (cx - 1.7, 15.2)],
           shade("#08080A", k), 0.4, so=0.5)
    D.line([(cx + 0.2, 13.0), (cx + 1.0, 13.1), (cx + 1.7, 12.9)], shade(TAN_SHADOW if near else "#08080A", k), 0.12,
           so=0.35)
    D.line([(cx - 1.0, 5.9), (cx - 0.45, 4.6), (cx + 0.1, 3.6)], shade(HAIR_BLACK_SHEEN, 1.3 * k), 0.24, so=0.6)
    dewclaw(cx - 1.25, 5.2, far)
    hoof(cx + 0.05, cx + 4.4, far, name)
    # la laine de l'avant-bras, en festons au-dessus du genou : elle est dans
    # la bande noire du poitrail et du ventre
    sleeve = [(cx - 4.0, 38.0), (cx - 3.8, 32.0), (cx - 3.4, 26.6), (cx - 2.9, LAINE_AV + 1.3),
              (cx - 1.3, LAINE_AV + 0.3), (cx + 0.9, LAINE_AV + 0.5), (cx + 2.8, LAINE_AV + 1.2),
              (cx + 3.4, 26.6), (cx + 3.8, 32.0), (cx + 4.0, 38.0), (cx, 40.0)]

    def fore_zone(x, h):
        return "noir"

    sleeve_fond(name, sleeve, far, fore_zone)
    wool_sleeve(name, sleeve, rng, lambda x, h: 0.36 + 0.02 * (h - 24), far, fore_zone)
    D.pivot(name, cx + 0.4, SHOULDER_H)
    D.close()


def hindleg(cx, near):
    name = "leg-back-near" if near else "leg-back-far"
    far = not near
    rng = random.Random(int(cx * 10) + 3)
    D.open(name)
    # la jambe qui remonte en avant vers le grasset, le jarret ouvert à
    # 140-150° (§ 2.6) et sa pointe nette, le canon vertical, le paturon court
    front = [(cx + 8.5, 44.0), (cx + 6.4, 36.0), (cx + 4.6, 30.0), (cx + 3.0, 25.0), (cx + 1.95, 21.4),
             (cx + 1.6, 19.2), (cx + 1.42, 16.4), (cx + 1.32, 12.2), (cx + 1.36, 8.6), (cx + 1.55, 6.3),
             (cx + 1.95, 5.1), (cx + 2.5, 4.0), (cx + 3.1, 3.2), (cx + 3.55, COURONNE + 0.15)]
    back = [(cx + 0.55, COURONNE + 0.25), (cx + 0.0, 3.5), (cx - 0.55, 4.7), (cx - 1.1, 6.0), (cx - 1.4, 7.5),
            (cx - 1.25, 9.4), (cx - 1.2, 12.4), (cx - 1.4, 15.1), (cx - 1.95, 16.5), (cx - 2.5, 17.5),
            (cx - 2.45, 18.8), (cx - 1.85, 20.6), (cx - 0.6, 24.0), (cx + 1.4, 30.0), (cx + 3.4, 37.0),
            (cx + 4.6, 44.0)]
    D.el(runs_path([front, [front[-1], back[0]], back + [(cx + 6.6, 46.0), front[0]]], nd=2),
         fill=black_leg_paint(name, cx, far))
    k = 0.8 if far else 1.0
    edges = [(4.5, cx - 1.0, cx + 2.0), (8.0, cx - 1.3, cx + 1.4), (14.0, cx - 1.3, cx + 1.4),
             (16.6, cx - 1.9, cx + 1.5), (18.8, cx - 2.3, cx + 1.7), (21.0, cx - 1.8, cx + 2.2)]
    hair_strokes(edges, 70, rng, shade(HAIR_BLACK_SHEEN, 1.15 * k), 0.5)
    if near:
        # la rayure fauve sur la face avant et extérieure du canon, du jarret
        # au boulet ; l'arrière du jarret et du canon reste noir (§ 4.3)
        fe = [(cx + 1.62, 19.4), (cx + 1.5, 17.6), (cx + 1.42, 16.4), (cx + 1.32, 12.2), (cx + 1.36, 8.6),
              (cx + 1.5, 6.3)]
        inner = [(cx + 0.7, 19.0), (cx - 0.1, 17.6), (cx - 0.25, 16.0), (cx - 0.3, 12.2), (cx - 0.15, 8.6),
                 (cx + 0.65, 6.3)]
        tan_stripe(name, fe, inner, rng, far)
        hair_strokes([(6.6, cx + 0.3, cx + 1.3), (12.0, cx + 0.1, cx + 1.2), (16.0, cx + 0.2, cx + 1.3),
                      (18.6, cx + 0.8, cx + 1.5)], 34, rng, TAN_LIGHT, 0.45)
        hair_strokes([(6.6, cx + 0.2, cx + 0.9), (12.0, cx + 0.0, cx + 0.7), (16.0, cx + 0.1, cx + 0.8),
                      (18.6, cx + 0.7, cx + 1.1)], 20, rng, TAN_SHADOW, 0.4)
    # le tendon et la pointe du jarret, l'ombre derrière le canon
    D.line([(cx - 1.0, 8.0), (cx - 0.95, 12.0), (cx - 1.1, 14.8), (cx - 1.6, 16.4), (cx - 2.1, 17.4)],
           shade("#08080A", k), 0.4, so=0.5)
    D.line([(cx - 2.2, 17.7), (cx - 2.3, 18.6), (cx - 2.0, 19.5)], shade(HAIR_BLACK_SHEEN, 1.4 * k), 0.2, so=0.6)
    D.line([(cx - 0.8, 6.3), (cx - 0.25, 4.8), (cx + 0.4, 3.6)], shade(HAIR_BLACK_SHEEN, 1.3 * k), 0.24, so=0.6)
    dewclaw(cx - 0.95, 5.6, far)
    hoof(cx + 0.45, cx + 4.75, far, name)
    # la cuisse et la jambe dans la laine : la culotte noire derrière, de la
    # queue au jarret ; le haut de la cuisse crème, devant ; le bas de la
    # jambe noir, où la bande du ventre rejoint la culotte (§ 4.2)
    sleeve = [(cx - 9.4, 46.0), (cx - 9.3, 41.0), (cx - 8.5, 36.0), (cx - 6.9, 31.4), (cx - 4.7, 26.6),
              (cx - 2.7, 22.2), (cx - 0.6, LAINE_AR), (cx + 1.8, LAINE_AR + 0.3), (cx + 3.8, 22.6),
              (cx + 5.8, 26.6), (cx + 8.0, 30.6), (cx + 10.0, 35.0), (cx + 11.2, 40.0), (cx + 11.4, 46.0),
              (cx + 1.0, 49.0)]
    rim = ragged_line([(cx - 3.6, 49.0), (cx - 3.9, 44.0), (cx - 3.0, 38.0), (cx - 1.0, 33.4), (cx + 2.4, 30.8),
                       (cx + 6.0, 30.2), (cx + 9.6, 31.8), (cx + 13.0, 33.0)], random.Random(int(cx * 3)), amp=0.5,
                      step=0.8)
    rim_poly = Polygon(rim + [(cx + 13.0, 52.0), (cx - 3.6, 52.0)]).buffer(0)

    def hind_zone(x, h):
        return "creme" if rim_poly.contains(Point(x, h)) else "noir"

    sleeve_fond(name, sleeve, far, hind_zone)
    wool_sleeve(name, sleeve, rng,
                lambda x, h: 0.3 + 0.36 * max(0.0, (h - 26) / 20) - 0.1 * max(0.0, (x - cx) / 10),
                far, hind_zone, length=(2.5, 3.4))
    D.pivot(name, cx + 2.6, HIP_H)
    D.close()


def scrotum():
    """Le scrotum du bélier, ras et noir (§ 2.4, 4.2), entre les postérieurs,
    discret : il dépasse à peine sous la culotte, devant la cuisse du fond."""
    g = D.lin("scrotum", (HIND_FAR + 5.0, 30), (HIND_FAR + 10.0, 18), [(0, "#2A2A30"), (0.5, "#18171A"),
                                                                        (1, "#0A0909")])
    pts = [(HIND_FAR + 4.6, 30.0), (HIND_FAR + 4.2, 25.5), (HIND_FAR + 5.2, 21.2), (HIND_FAR + 7.4, 19.6),
           (HIND_FAR + 9.6, 20.6), (HIND_FAR + 10.4, 24.0), (HIND_FAR + 9.8, 28.6), (HIND_FAR + 9.0, 31.0)]
    D.el(cr(pts, t=0.9), fill=g)
    D.line([(HIND_FAR + 6.0, 21.4), (HIND_FAR + 7.6, 20.4), (HIND_FAR + 9.0, 21.0)], "#3A3A44", 0.25, so=0.5)


# ================================================================== la queue

def tail():
    """La queue entière (§ 2.2, 4.2), jusqu'au jarret : crème dehors, noire
    dessous jusqu'au bout. Elle pend derrière la culotte, en trois morceaux qui
    ondulent ; vue de profil, son côté crème est bordé de noir le long de son
    dessous (le bord tourné vers la culotte), et le bout est noir dessous ;
    sous le corps, qui recouvre son attache."""
    rng = random.Random(1924)  # Roberts étudie le patron badgerface à Bangor (§ 1.1)
    axis = [(4.0, 53.0), (1.8, 49.0), (-0.4, 43.0), (-1.6, 36.5), (-1.9, 30.0), (-1.6, 24.0), (-1.0, QUEUE_BOUT + 0.8)]
    AX = dense(axis, False, 8)

    def at(h):
        for (x0, h0), (x1, h1) in zip(AX, AX[1:]):
            if h1 <= h <= h0:
                u = (h0 - h) / (h0 - h1) if h0 != h1 else 0
                return x0 + (x1 - x0) * u
        return AX[-1][0]

    def width(h):
        return 3.6 + 2.0 * max(0.0, min(1.0, (h - QUEUE_BOUT) / 34.0))

    gc = D.lin("fond-queue", (-5, 0), (5, 0), [(0, DEEP_TOP), (0.55, DEEP_MID), (0.7, "#141213"), (1, "#141213")])

    def segment(top, bottom, overlap, last=False):
        # le fond s'arrête au haut du morceau : au-dessus, ce sont les mèches
        # du morceau précédent qu'on voit ; les premières mèches de celui-ci,
        # plus haut, couvrent la jointure quand la queue ondule
        hs = [bottom + (top - bottom) * k / 8 for k in range(9)]
        shape = [(at(h) + width(h) / 2 - 0.4, h) for h in hs[::-1]] + [(at(h) - width(h) / 2 + 0.4, h) for h in hs]
        D.el(cr(shape, nd=1, t=0.8), fill=gc)
        crimps = Crimps()
        locks = []
        h = top + overlap * 0.6
        while h > bottom + 0.8:
            w = width(h)
            n = max(2, int(round(w / 1.35)))
            for k in range(n):
                u = (k + 0.5) / n
                x = at(h) - w / 2 + w * u + rng.uniform(-0.2, 0.2)
                end = h - rng.uniform(2.6, 3.4)
                tip = last and end < bottom + 1.0
                L = (h - bottom + rng.uniform(1.0, 1.8)) if tip else (h - end)
                ang = math.atan2(at(h - L) - at(h), L) * 0.9 + (u - 0.5) * 0.3 + rng.uniform(-0.12, 0.12)
                # le dessous, noir : le tiers du devant (vers la culotte), et
                # tout le bout
                z = "noir" if (u > 0.64 or (last and h < bottom + 3.2 and u > 0.34)) else "creme"
                light = 0.66 - 0.34 * u + 0.25 * max(0.0, (h - 40) / 14) + rng.uniform(-0.05, 0.05)
                locks.append((h + rng.uniform(-0.3, 0.3), x, L, ang, light, tip, z))
            h -= rng.uniform(1.6, 2.0)
        locks.sort()
        for (root, x, L, ang, light, tip, z) in locks:
            lock(x, root, L, rng.uniform(1.4, 1.7), ang, light, rng, crimps, z=z,
                 hook=rng.choice((-1, 1)) * rng.uniform(1.0, 1.8) if tip else None)
        crimps.emit()

    D.comment("Queue : entière, crème dehors, noire dessous jusqu'au bout, en trois morceaux")
    D.open("tail")
    segment(53.0, 41.0, 0.0)
    D.open("tail-mid")
    segment(43.0, 29.0, 1.4)
    D.open("tail-tip")
    segment(31.0, QUEUE_BOUT, 1.4, last=True)
    D.pivot("tail-tip", at(30.0), 30.0)
    D.close()
    D.pivot("tail-mid", at(42.0), 42.0)
    D.close()
    D.pivot("tail", 3.4, 52.4)
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


# Le profil de la tête (§ 5) : un coin de longueur modérée, le front large et
# plat de la nuque à l'œil, le chanfrein droit (jamais creusé), le museau
# large et arrondi, plus court et plus massif que celui du Jacob ; la mâchoire
# forte, au bord inférieur presque droit ; la ganache qui remonte vers la
# nuque sous la laine de l'encolure. 22 cm de la nuque au bout du nez, 11,5 cm
# à l'angle de la mâchoire.
DORSAL = [(-1.2, 0.8), (-0.9, -0.7), (0.3, -1.75), (1.9, -2.35), (4.0, -2.7), (6.4, -2.85), (8.8, -2.85),
          (11.0, -2.75), (13.2, -2.55), (15.2, -2.28), (17.0, -1.95), (18.6, -1.55), (19.9, -1.0), (20.9, -0.3),
          (21.6, 0.6), (21.95, 1.6), (21.95, 2.6), (21.65, 3.45), (21.15, 4.0)]
MOUTH = [(21.15, 4.0), (20.2, 4.25), (18.9, 4.48), (17.6, 4.65), (16.4, 4.78)]
NOTCH = [(16.4, 4.78), (16.25, 5.75), (15.95, 6.7), (15.45, 7.5), (14.9, 8.0)]
JAWLINE = [(14.9, 8.0), (13.5, 8.65), (11.8, 9.15), (10.0, 9.4), (8.4, 9.3), (7.0, 8.9)]
BACKLINE = [(7.0, 8.85), (5.6, 9.2), (4.1, 9.1), (2.6, 8.4), (1.2, 7.1), (0.1, 5.4), (-0.7, 3.4), (-1.1, 2.0),
            (-1.2, 0.8)]
HEAD_OUT = DORSAL + MOUTH[1:] + NOTCH[1:] + JAWLINE[1:] + BACKLINE[1:-1]

EYE_L = (8.8, 1.25)             # l'œil : à 0,40 HL de la nuque, assez haut (§ 5)
EAR_L = (4.9, 3.2)              # l'oreille sort sous la base de la corne, derrière l'œil
MUZZLE_L = (21.1, 4.6)          # ce qui touche l'herbe : le bas des lèvres
JAW_PIVOT_L = (13.6, 5.3)       # la charnière de la mâchoire, sous la joue
# La lèvre supérieure, du dessous du naseau à la commissure.
LIP_L = [(17.6, 3.05), (18.8, 2.8), (19.9, 2.55), (20.9, 2.4), (21.65, 2.35), (22.05, 2.75), (21.95, 3.5),
         (21.4, 4.05), (20.4, 4.3), (19.0, 4.55), (17.8, 4.72), (16.85, 4.86), (16.6, 4.3), (17.05, 3.55)]
LIP_PIVOT_L = (17.6, 3.3)
# Le naseau, en virgule oblique qui remonte vers l'arrière (§ 5).
NOSTRIL_L = [(21.45, 1.6), (21.3, 1.95), (20.9, 2.0), (20.5, 1.7), (20.1, 1.25), (19.7, 0.8), (20.15, 0.88),
             (20.6, 1.15), (21.05, 1.3)]


def head_outline():
    """Le contour de la face, échantillonné exactement comme le trace
    `runs_path` (les mêmes suites lisses, les mêmes angles)."""
    pts = []
    for run in (DORSAL, MOUTH, NOTCH, JAWLINE + BACKLINE[1:]):
        pts += dense(run, False, 8)[:-1]
    return pts


EYE_ROT = math.radians(-14.0)   # l'œil moins penché que la tête


def eye_local(ex, ey, scale=1.0):
    ex, ey = ex * scale, ey * scale
    c, s = math.cos(EYE_ROT), math.sin(EYE_ROT)
    return (EYE_L[0] + ex * c - ey * s, EYE_L[1] + ex * s + ey * c)


def eye_window(scale=1.0):
    """L'ouverture de l'œil (repère de la tête) : une amande, le coin de
    devant un peu plus bas ; « prominent and bright » (§ 5)."""
    base = [(-1.3, 0.04), (-0.9, -0.62), (-0.04, -0.82), (0.8, -0.68), (1.33, -0.08), (0.8, 0.52), (0.0, 0.68),
            (-0.87, 0.47)]
    return [eye_local(ex, ey, scale) for (ex, ey) in base]


def eye_pt(ex, ey):
    return HL(*eye_local(ex, ey))


# Les bandes noires de la face (§ 4.4 ; photo 11), dans le repère de la tête.
# Le « sourcil » part du haut de la base de l'oreille, passe en arc au-dessus
# de l'œil (le plus large, 2,6 cm : son bord bas frôle la paupière) puis
# descend en s'amincissant le long du côté du chanfrein, jusqu'à mi-chemin de
# l'œil au naseau. La bande du dessous part du bas de la base de l'oreille,
# passe sous l'œil et remonte le rejoindre devant lui : l'œil est enfermé
# dans une lunette noire. Derrière l'œil, entre les deux bandes qui s'écartent
# vers l'oreille, la face reste crème ; entre le sourcil et le profil du nez,
# une étroite bande crème court du front au nez.
BROW_SPINE = [(4.6, 1.0), (5.6, -0.15), (6.9, -0.75), (8.3, -0.95), (9.7, -0.98), (11.1, -1.1), (12.5, -1.15),
              (13.8, -1.12), (14.9, -1.02)]
BROW_HALF = [0.75, 0.95, 1.12, 1.15, 1.1, 0.92, 0.66, 0.4, 0.08]
UNDER_SPINE = [(3.9, 3.05), (5.4, 2.95), (6.9, 2.75), (8.3, 2.5), (9.5, 2.15), (10.4, 1.45), (10.8, 0.6)]
UNDER_HALF = [0.62, 0.6, 0.56, 0.52, 0.5, 0.5, 0.48]


def _ribbon(spine, half):
    """Une bande le long d'une ligne, de demi-largeur variable (repère de la
    tête), aux bouts arrondis."""
    pts = dense(spine, False, 6)
    n = len(pts)
    left, right = [], []
    for i in range(n):
        u = i / (n - 1) * (len(half) - 1)
        k = min(int(u), len(half) - 2)
        w = half[k] + (half[k + 1] - half[k]) * (u - k)
        a, b = pts[max(i - 1, 0)], pts[min(i + 1, n - 1)]
        tx, ty = b[0] - a[0], b[1] - a[1]
        L = math.hypot(tx, ty) or 1.0
        nx, ny = -ty / L, tx / L
        left.append((pts[i][0] + nx * w, pts[i][1] + ny * w))
        right.append((pts[i][0] - nx * w, pts[i][1] - ny * w))
    return Polygon(left + right[::-1]).buffer(0.12).buffer(-0.12)


def brow_shape():
    """Le sourcil, la lunette et la bande du dessous, réunis (repère de la
    tête)."""
    lunette = Polygon(eye_window(1.0)).buffer(0.62)
    shape = _ribbon(BROW_SPINE, BROW_HALF).union(_ribbon(UNDER_SPINE, UNDER_HALF)).union(lunette)
    shape = shape.buffer(0.25).buffer(-0.25)
    return list(shape.exterior.coords)[:-1]


def head():
    rng = random.Random(1976)
    D.comment("Tête : un coin au profil droit, face crème-fauve barrée de deux bandes noires qui enferment "
              "l'œil, museau et mâchoire noirs, petites oreilles noires, cornes sombres d'un tour")
    D.open("head")
    ear("ear-far", far=True)
    jaw()
    if not VARIANTE:
        far_horn()
    # le fond de l'orbite, sous l'œil : le noir des paupières, qu'on voit
    # quand il cligne
    D.el(cr(HLs(eye_window(1.5)), nd=2), fill=HAIR_BLACK)
    eyes()
    face(rng)
    ear("ear-near", far=False)
    if not VARIANTE:
        near_horn(rng)
    D.pivot("head", *NUQUE)
    D.pivot("muzzle", *HL(*MUZZLE_L))
    D.close()


def ear(name, far):
    """L'oreille, petite et noire, dedans comme dehors (« small and dark
    inside », § 5) : une feuille courte et pointue, portée à l'horizontale,
    un peu relevée, vers le côté et l'arrière ; elle sort sous la spirale de
    la corne."""
    base = HL(*EAR_L)
    if far:
        base = (base[0] - 0.3, base[1] + 0.9)
    ang = math.radians(188.0 if not far else 182.0)
    a = (math.cos(ang), math.sin(ang))          # le long de l'oreille, vers sa pointe
    nrm = (-a[1], a[0])                         # vers le haut de l'oreille
    L = OREILLE * (0.86 if not far else 0.6)

    def P(u, v):
        return (base[0] + a[0] * u + nrm[0] * v, base[1] + a[1] * u + nrm[1] * v)

    D.open(name)
    shape = [(-0.6, -0.75), (0.6, -1.25), (2.0, -1.65), (3.4, -1.7), (4.7, -1.3), (5.7, -0.75), (L - 0.25, -0.18),
             (L, 0.15), (5.9, 0.6), (4.7, 1.1), (3.3, 1.42), (1.9, 1.3), (0.6, 0.95), (-0.6, 0.6)]
    if far:
        D.el(cr([P(u, v) for (u, v) in shape]), fill=WOOL_BLACK_DEEP)
    else:
        g = D.lin("oreille", P(3, 1.6), P(3, -1.8), [(0, "#3A3640"), (0.45, HAIR_BLACK), (1, "#0A090B")])
        D.el(cr([P(u, v) for (u, v) in shape]), fill=g)
        # la conque, sombre aussi ; le bord du pavillon qui prend la lumière
        concha = [(0.8, -0.1), (2.0, -0.8), (3.4, -0.9), (4.6, -0.6), (5.4, -0.08), (4.7, 0.5), (3.3, 0.72),
                  (1.9, 0.6), (0.8, 0.35)]
        D.el(cr([P(u, v) for (u, v) in concha]), fill=EAR_INSIDE)
        D.line([P(0.8, 0.0), P(2.0, -0.08), P(3.2, 0.04)], "#08070A", 0.28, so=0.8)
        D.line([P(0.5, 0.95), P(1.9, 1.3), P(3.3, 1.45), P(4.7, 1.15), P(5.9, 0.65)], "#4C4A55", 0.16, so=0.6)
    D.pivot(name, *base)
    D.close()


def jaw():
    """La mâchoire : le bas du museau (lèvre inférieure et menton noirs,
    § 4.4), et au-dessus, cachés tant que la bouche est fermée, l'intérieur de
    la bouche, la langue et les incisives du bas."""
    rng = random.Random(1930)  # Roberts et White, le « reversed badgerface » (§ 10.1)
    D.open("jaw")
    # la joue qui s'étire quand la bouche s'ouvre (cachée au repos), noire
    # comme le bas de la face
    D.el(cr(HLs([(10.6, 5.1), (14.6, 3.7), (15.2, 7.0), (11.6, 8.3)]), t=0.6), fill=HAIR_BLACK)
    # l'intérieur de la bouche, sombre, au-dessus de la lèvre du bas
    D.el(cr(HLs([(12.8, 5.9), (14.4, 4.1), (16.2, 3.25), (18.1, 2.95), (20.3, 2.8), (21.0, 3.0), (20.2, 3.4),
                 (18.1, 3.7), (15.9, 4.1), (14.1, 5.1)]), t=0.8), fill="#2A1C1E")
    D.el(cr(HLs([(15.1, 4.35), (16.6, 3.7), (18.3, 3.4), (19.9, 3.3), (19.3, 3.6), (16.9, 4.0)]), t=0.9),
         fill="#7E5558")
    D.line(HLs([(16.0, 4.1), (17.7, 3.65), (19.1, 3.5)]), "#A07276", 0.12, so=0.7)
    # les incisives du bas (en haut, le mouton n'a qu'un bourrelet)
    D.el(cr(HLs([(20.1, 3.25), (20.85, 3.15), (20.95, 3.48), (20.2, 3.56)]), t=0.5), fill="#D2C9B4")
    # la lèvre du bas et le menton, noirs
    jw = HLs([(12.4, 6.0), (14.4, 4.5), (16.3, 3.75), (18.1, 3.58), (19.9, 3.4), (20.8, 3.36), (21.2, 3.66),
              (21.3, 4.1), (21.15, 4.7), (20.7, 5.35), (20.0, 5.95), (19.0, 6.45), (17.8, 6.9), (16.8, 7.25),
              (15.9, 7.55), (15.2, 7.7), (14.4, 7.78), (13.2, 7.86), (12.3, 7.92)])
    g = D.lin("menton", HL(17.5, 3.5), HL(17.5, 7.2), [(0, "#2C2A30"), (0.45, HAIR_BLACK), (1, WOOL_BLACK_DEEP)])
    D.el(cr(jw, t=0.85), fill=g)
    D.line(HLs([(17.5, 4.5), (19.0, 4.3), (20.4, 4.12), (21.1, 4.0)]), NOSE_SHEEN, 0.13, so=0.45)
    D.line(HLs([(21.0, 4.75), (20.6, 5.45), (19.8, 6.0)]), HAIR_BLACK_SHEEN, 0.28, so=0.45)
    hairs = []
    for _ in range(30):
        lx, ly = rng.uniform(16.8, 21.0), rng.uniform(4.5, 6.9)
        if not inside([(21.3, 4.1), (21.15, 4.7), (20.7, 5.35), (20.0, 5.95), (19.0, 6.45), (17.8, 6.9), (16.8, 7.2),
                       (16.6, 4.6)], lx, ly):
            continue
        p = HL(lx, ly)
        d = world_dir(-rng.uniform(0.25, 0.45), rng.uniform(0.15, 0.35))
        hairs.append(cr([p, (p[0] + d[0], p[1] + d[1])], closed=False, nd=2))
    D.el("".join(hairs), stroke=HAIR_BLACK_SHEEN, sw=0.05, so=0.4, cap="round")
    D.pivot("jaw", *HL(*JAW_PIVOT_L))
    D.close()


def eyes():
    """L'œil : l'iris ambre-brun (§ 5, mesuré `#744D31`) qui ressort dans la
    lunette noire, un petit reflet ; la pupille en fente horizontale (dans le
    monde, au repos)."""
    E = HL(*EYE_L)
    D.open("eyes")
    win = HLs(eye_window(1.0))
    D.el(cr(win, nd=2, t=0.9), fill="#2E1E14")
    D.open("pupils")
    ic = (E[0] + 0.04, E[1] - 0.02)
    gi = D.rad("iris", ic, 1.02, [(0, IRIS_LIGHT), (0.4, IRIS_LIGHT), (0.72, IRIS), (0.9, IRIS_DARK), (1, "#3A2614")],
               f=(ic[0] - 0.2, ic[1] + 0.26))
    D.el(cr(ellipse_pts(ic[0], ic[1], 1.02, 1.02, 0, 16), nd=2), fill=gi)
    fib = []
    for k in range(14):
        a0 = 2 * math.pi * k / 14 + 0.15
        fib.append(cr([(ic[0] + 0.38 * math.cos(a0), ic[1] + 0.38 * math.sin(a0)),
                       (ic[0] + 0.9 * math.cos(a0 + 0.1), ic[1] + 0.9 * math.sin(a0 + 0.1))], closed=False, nd=2))
    D.el("".join(fib), stroke="#5C3E22", sw=0.045, so=0.5, cap="round")
    D.el(cr(ellipse_pts(ic[0], ic[1], 0.98, 0.98, 0, 16), nd=2), stroke="#3A2614", sw=0.08, so=0.85)
    pw, ph = 1.1, 0.36
    X, Y = S(ic[0] - pw / 2, ic[1] + ph / 2)
    D.emit(f'<rect x="{num(X)}" y="{num(Y)}" width="{num(pw)}" height="{num(ph)}" rx="{num(ph / 2)}" fill="{PUPIL}"/>')
    D.pivot("pupils", *ic)
    D.close()
    # les reflets (après les pupilles : ils ne glissent pas avec l'iris)
    D.el(cr(ellipse_pts(E[0] - 0.34, E[1] + 0.3, 0.21, 0.14, 0.3, 10), nd=2), fill=CATCH, fo=0.9)
    D.el(cr(ellipse_pts(E[0] + 0.45, E[1] - 0.2, 0.08, 0.06, 0.3, 8), nd=2), fill=CATCH, fo=0.45)
    # l'ombre de la paupière du haut, le bord noir des paupières, les cils
    lid = [eye_pt(-1.36, 0.0), eye_pt(-0.9, -0.64), eye_pt(0.0, -0.85), eye_pt(0.82, -0.72), eye_pt(1.36, -0.06)]
    shadow = lid + [eye_pt(0.8, -0.36), eye_pt(0.0, -0.46), eye_pt(-0.9, -0.28)]
    D.el(cr(shadow, nd=2), fill="#120E0B", fo=0.55)
    D.el(cr(win, nd=2, t=0.9), stroke=WOOL_BLACK_DEEP, sw=0.26, join="round")
    D.line(lid, HAIR_BLACK_SHEEN, 0.12, so=0.7)
    lashes = []
    for k in range(5):
        u = (k + 0.5) / 5
        p = eye_pt(-1.05 + 2.1 * u, -0.78 - 0.08 * math.sin(math.pi * u))
        q = eye_pt(-1.05 + 2.1 * u - 0.22, -1.18 - 0.08 * math.sin(math.pi * u))
        lashes.append(cr([p, q], closed=False, nd=2))
    D.el("".join(lashes), stroke=WOOL_BLACK_DEEP, sw=0.07, cap="round")
    D.pivot("eyes", *E)
    D.close()


FACE_SOFT = 0


def soft_spot(c, rx, ry, rot, color, alpha, clip):
    """Une tache douce sur la face : un dégradé radial elliptique (centre `c`
    et rayons dans le repère de la tête, tourné avec elle de `rot`), de
    `alpha` au centre à rien au bord, rogné à `clip` : l'ombre ou la lumière
    qui donne son volume à l'os et au muscle, sans filtre ni bord visible."""
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


def ragged(pts, rng, amp=0.07, step=0.18, closed=False):
    """Un bord de poil net mais un peu irrégulier (repère de la tête)."""
    P = dense(pts, closed, 12)
    out = [P[0]]
    acc = 0.0
    for a, b in zip(P, P[1:]):
        acc += math.dist(a, b)
        if acc >= step:
            out.append(b)
            acc = 0.0
    if out[-1] != P[-1] and not closed:
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


# Le bas de la face, noir (§ 4.4) : le museau (le cuir du nez, les naseaux,
# les lèvres) et la mâchoire inférieure, d'un bandeau qui part du coin de la
# bouche et longe le dessous de la tête jusqu'à la gorge. Au-dessus, la lèvre
# supérieure reste claire juste derrière la truffe, et la joue crème.
JAW_BAND = [(22.1, 1.2), (21.7, 0.15), (21.05, -0.55), (20.2, -0.75), (19.6, -0.2), (19.4, 0.8), (19.05, 1.9),
            (18.3, 2.75), (17.2, 3.3), (15.9, 3.85), (14.6, 4.9), (13.3, 6.0), (11.6, 6.95), (9.6, 7.55),
            (7.6, 8.0), (5.8, 8.3), (4.0, 8.4), (2.4, 8.15), (0.6, 7.6), (-1.2, 7.4), (-1.6, 10.5), (23.0, 10.5)]


def face(rng):
    """La face : le crâne et la joue (percés de l'ouverture de l'œil), modelés
    d'ombres et de lumières douces ; le poil ras crème-fauve ; les bandes
    noires et la lunette ; le bas du museau et la mâchoire noirs ; la truffe
    et les naseaux."""
    win = HLs(eye_window(1.04))
    out = HLs(HEAD_OUT)
    g = D.lin("face", HL(10, -2.9), HL(10, 9.5), [(0, FACE_LIGHT), (0.4, FACE), (0.85, FACE_SHADOW), (1, FACE_DEEP)])
    D.el(runs_path([HLs(DORSAL), HLs(MOUTH), HLs(NOTCH), HLs(JAWLINE + BACKLINE[1:])], nd=2)
         + cr(win[::-1], nd=2, t=0.9), fill=g, rule="evenodd")
    head_poly = Polygon(HLs(head_outline())).buffer(0)
    clip = head_poly.difference(Polygon(HLs(eye_window(1.15))).buffer(0))
    # le modelé : la lumière sur le front plat et l'arête du nez, l'ombre
    # sous l'arcade et sur la joue, le creux devant le masséter
    soft_spot((12.4, -1.95), 6.4, 0.9, 0.06, "#FFFFFF", 0.65, clip)
    soft_spot((5.0, -1.4), 3.4, 1.4, 0.2, "#FFFFFF", 0.4, clip)
    soft_spot((11.6, 3.6), 2.6, 1.4, 0.4, FACE_DEEP, 0.32, clip)
    soft_spot((8.6, 5.4), 3.2, 2.0, -0.1, "#FFFFFF", 0.32, clip)
    soft_spot((15.6, 2.2), 1.4, 2.2, 0.0, FACE_DEEP, 0.18, clip)
    soft_spot((4.0, 6.4), 3.6, 2.2, 0.2, FACE_DEEP, 0.3, clip)
    # le poil ras : des traits fins couchés vers le nez, clairs sur le front
    # et le chanfrein, plus sombres sur la joue
    band = HLs(brow_shape())
    lower = HLs(JAW_BAND)
    hairs_l, hairs_d = [], []
    for _ in range(240):
        lx = rng.uniform(1.0, 19.5)
        ly = rng.uniform(-2.8, 7.0)
        p = HL(lx, ly)
        if not inside(out, *p) or inside(band, *p) or inside(lower, *p):
            continue
        ln = rng.uniform(0.45, 0.8)
        d = world_dir(ln, 0.12 * ln * rng.uniform(-1, 1) + 0.06 * ln)
        seg = cr([p, (p[0] + d[0], p[1] + d[1])], closed=False, nd=2)
        lit = ly < 0.2 or (6.0 < lx < 11.0 and 4.0 < ly < 6.6)
        (hairs_l if lit and rng.random() < 0.8 else hairs_d).append(seg)
    D.el("".join(hairs_l), stroke="#FFFDF6", sw=0.05, so=0.55, cap="round")
    D.el("".join(hairs_d), stroke="#8E8478", sw=0.05, so=0.4, cap="round")
    badger_bands(rng, win, head_poly)
    muzzle(rng, head_poly)


def badger_bands(rng, win, head_poly):
    """Les bandes noires de la face (§ 4.4) : le sourcil et la lunette, d'un
    seul tracé au bord net et un peu irrégulier, percé de l'ouverture de l'œil
    (elles cachent l'iris qui glisse) ; le poil noir en traits fins ; un
    reflet bleuté sur le haut du sourcil."""
    brow = ragged(brow_shape(), rng, amp=0.06, closed=True)
    shape = Polygon(HLs(brow)).buffer(0).intersection(head_poly)
    rings = [shape.exterior] + list(shape.interiors)
    D.el("".join(poly(list(r.coords)[:-1], nd=2) for r in rings) + cr(win[::-1], nd=2, t=0.9), fill=HAIR_BLACK,
         rule="evenodd")
    eye_free = shape.difference(Polygon(HLs(eye_window(1.1))).buffer(0))
    # le reflet du poil, sur le haut de l'arc (la lumière vient d'en haut)
    c = (8.4, -1.1)
    rx, ry = 3.2, 0.75
    pts = [HL(c[0] + rx * math.cos(2 * math.pi * k / 40), c[1] + ry * math.sin(2 * math.pi * k / 40)) for k in range(40)]
    sheen = Polygon(pts).intersection(eye_free.buffer(-0.1))
    if not sheen.is_empty and sheen.geom_type == "Polygon":
        g = D.ell("sourcil-reflet", HL(*c), rx, ry, HEAD_TILT, [(0, "#5A5A66", 0.55), (0.6, "#44444E", 0.25),
                                                              (1, "#3A3A44", 0.0)])
        D.el(poly(list(sheen.simplify(0.04).exterior.coords)[:-1], nd=2), fill=g)
    # le poil noir : il file vers l'avant et le bas, comme le poil de la face
    hairs = []
    inner = eye_free.buffer(-0.15)
    for _ in range(170):
        lx, ly = rng.uniform(4.0, 16.0), rng.uniform(-2.0, 4.2)
        p = HL(lx, ly)
        if not inner.contains(Point(p)):
            continue
        L = rng.uniform(0.3, 0.5)
        d = world_dir(L, 0.1 * L * rng.uniform(-1, 1) + 0.12 * L)
        hairs.append(cr([p, (p[0] + d[0], p[1] + d[1])], closed=False, nd=2))
    D.el("".join(hairs), stroke="#3A3A44", sw=0.05, so=0.5, cap="round")
    # quelques poils du bord qui débordent à peine sur le crème
    tufts = []
    for (lx, ly) in brow[2::7]:
        p = HL(lx, ly)
        d = world_dir(0.26, 0.06)
        tufts.append(cr([(p[0] - d[0] * 0.3, p[1] - d[1] * 0.3), (p[0] + d[0], p[1] + d[1])], closed=False, nd=2))
    D.el("".join(tufts), stroke=HAIR_BLACK, sw=0.05, so=0.55, cap="round")


# Le cuir du nez (le mufle nu) : du dessus du nez, autour des naseaux,
# jusqu'au bout de la lèvre supérieure par le philtrum.
RHIN = [(20.4, -1.0), (20.95, -0.6), (21.6, 0.3), (21.95, 1.4), (21.98, 2.5), (21.7, 3.35), (21.25, 3.9),
        (20.95, 3.4), (20.65, 2.6), (20.25, 1.9), (19.95, 1.1), (19.9, 0.2), (20.0, -0.55)]
RHIN_FRONT = 7          # les sept premiers points suivent le profil


def muzzle(rng, head_poly):
    """Le bas de la face noir (§ 4.4) : le poil noir du museau et de la
    mâchoire, du bord jusqu'aux lèvres et à la gorge ; le cuir du nez, nu,
    noir et luisant ; le naseau en virgule ; la lèvre supérieure ronde, fendue
    au milieu ; le sillon de la bouche."""
    band = ragged(JAW_BAND[:20], rng, amp=0.07) + JAW_BAND[20:]
    gm = D.lin("museau", HL(18, -1.0), HL(18, 8.0), [(0, "#2E2C33"), (0.3, HAIR_BLACK), (1, WOOL_BLACK_DEEP)])
    shape = Polygon(HLs(band)).buffer(0).intersection(head_poly).difference(
        Polygon(HLs(RHIN)).buffer(0).buffer(-0.14))
    polys = [shape] if shape.geom_type == "Polygon" else [g for g in shape.geoms if g.geom_type == "Polygon"]
    d = ""
    for g in polys:
        for r in [g.exterior] + list(g.interiors):
            d += poly(list(r.coords)[:-1], nd=2)
    D.el(d, fill=gm, rule="evenodd")
    # le poil noir, en traits fins couchés vers le nez et vers la gorge
    hairs = []
    zone_ = HLs(band)
    for _ in range(110):
        lx, ly = rng.uniform(1.0, 21.5), rng.uniform(-0.8, 9.4)
        p = HL(lx, ly)
        if not inside(zone_, *p) or not head_poly.contains(Point(p)) or inside(HLs(RHIN), *p):
            continue
        d_ = world_dir(rng.uniform(0.3, 0.55), rng.uniform(0.05, 0.25))
        hairs.append(cr([p, (p[0] + d_[0], p[1] + d_[1])], closed=False, nd=2))
    D.el("".join(hairs), stroke=HAIR_BLACK_SHEEN, sw=0.05, so=0.35, cap="round")
    # le bord : quelques poils noirs sur le crème
    out_t = []
    for (lx, ly) in band[2:19:3]:
        p = HL(lx, ly)
        dd = world_dir(rng.uniform(-0.1, 0.1), -rng.uniform(0.12, 0.25))
        out_t.append(cr([p, (p[0] + dd[0], p[1] + dd[1])], closed=False, nd=2))
    D.el("".join(out_t), stroke=HAIR_BLACK, sw=0.05, so=0.5, cap="round")
    # le cuir du nez : nu, noir, luisant
    gt = D.rad("truffe", HL(21.2, 1.0), 2.6, [(0, "#3E4250"), (0.5, NOSE), (1, "#18181C")], f=HL(20.8, 0.3))
    D.el(runs_path([HLs(RHIN[:RHIN_FRONT]), HLs(RHIN[RHIN_FRONT - 1:] + RHIN[:1])], nd=2), fill=gt)
    D.el(cr(HLs(NOSTRIL_L), nd=2, t=0.9), fill=NOSTRIL)
    D.line(HLs([(19.75, 0.82), (19.35, 0.6), (19.05, 0.42)]), NOSTRIL, 0.12, so=0.65)
    # le reflet humide : le bord du naseau, le dessus du nez
    D.line(HLs([(20.15, 0.55), (20.65, 0.8), (21.1, 1.0)]), "#6E7686", 0.12, so=0.6)
    D.el(cr(ellipse_pts(*HL(20.9, -0.3), 0.24, 0.11, -HEAD_TILT, 10), nd=2), fill=NOSE_SHEEN, fo=0.6)
    D.line(HLs([(21.75, 2.05), (21.7, 2.6)]), "#4A5060", 0.12, so=0.6)
    # la lèvre supérieure, fendue au milieu (le philtrum)
    D.line(HLs([(21.75, 2.15), (21.6, 2.75), (21.35, 3.45)]), "#060607", 0.12, so=0.85)
    D.el(cr(ellipse_pts(*HL(21.0, 3.05), 0.55, 0.24, -HEAD_TILT - 0.25, 12), nd=2), fill="#4A4C56", fo=0.18)
    # le sillon de la bouche
    D.line(HLs(MOUTH[:1] + [(20.2, 4.23), (18.9, 4.45), (17.6, 4.62), (16.6, 4.75)]), "#050505", 0.16, so=0.9)


# ------------------------------------------------------------------ les cornes

# L'axe de la corne du devant (repère de la tête ; § 6.2) : la base au
# sommet de la tête, derrière et au-dessus de l'œil ; elle monte un peu et
# part vers l'arrière, puis s'enroule vers le bas derrière l'oreille, autour
# de son attache ; après un tour à peu près, la pointe revient vers l'avant à
# hauteur de la joue. De profil, la spirale fait 11,7 cm de large et 13,2 cm
# de haut (0,53 × 0,60 HL ; la fiche dit 0,55 × 0,65 HL) : modérée, bien
# plus petite que la boucle d'un mouflon.
HORN_AXIS = [(6.0, -0.75), (5.01, -1.73), (3.56, -2.38), (1.85, -2.5), (0.3, -1.9), (-0.9, -0.75), (-1.65, 0.8),
             (-1.74, 2.67), (-1.02, 4.72), (0.26, 6.44), (2.04, 7.65), (3.93, 8.31), (5.61, 8.16), (6.97, 7.47),
             (7.91, 6.56), (8.42, 5.66)]
# Base ovale d'environ 0,2 HL de long ; elle s'amincit vers la pointe.
HORN_W = [3.3, 3.15, 3.0, 2.85, 2.7, 2.55, 2.38, 2.2, 2.0, 1.8, 1.58, 1.35, 1.08, 0.8, 0.48, 0.2]
HORN_CENTER_L = (2.4, 2.4)


def horn_geometry(offset=(0.0, 0.0), scale=1.0):
    C = dense(HLs(HORN_AXIS), closed=False, per=8)
    C = [(NUQUE[0] + (x - NUQUE[0]) * scale + offset[0], NUQUE[1] + (h - NUQUE[1]) * scale + offset[1]) for (x, h) in C]
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
        if (C[i][0] - center[0]) * nx + (C[i][1] - center[1]) * nh < 0:
            nx, nh = -nx, -nh
        N.append((nx, nh))
        outer.append((C[i][0] + nx * W[i] / 2, C[i][1] + nh * W[i] / 2))
        inner.append((C[i][0] - nx * W[i] / 2, C[i][1] - nh * W[i] / 2))
    return C, W, N, acc, outer, inner


def far_horn():
    """La corne de l'autre côté : cachée derrière la tête, seul le bord de
    l'arc du dessus et du dos de la spire dépasse. Elle s'arrête aux deux
    tiers, là où la corne du devant et la face la couvrent tout entière :
    plus tôt, son bout coupé se verrait au-dessus de la spire."""
    C, W, N, acc, outer, inner = horn_geometry(offset=(-0.5, 0.7), scale=0.97)
    m = int(len(C) * 0.69)
    g = D.lin("corne-loin", (C[0][0], C[0][1] + 3), (C[0][0], C[0][1] - 3),
              [(0, shade(HORN_LIGHT, 0.8)), (1, shade(HORN_SHADOW, 0.8))])
    D.el(cr(outer[:m:2] + [outer[m]] + inner[m::-2] + [inner[0]], nd=2), fill=g)
    rings = []
    s = 0.5
    while s < acc[m] - 0.3:
        i = next(k for k in range(len(acc)) if acc[k] >= s)
        o = (C[i][0] + N[i][0] * W[i] * 0.45, C[i][1] + N[i][1] * W[i] * 0.45)
        n_ = (C[i][0] - N[i][0] * W[i] * 0.2, C[i][1] - N[i][1] * W[i] * 0.2)
        rings.append(cr([o, n_], closed=False, nd=2))
        s += 0.55
    D.el("".join(rings), stroke=shade(HORN_DEEP, 0.9), sw=0.1, so=0.6, cap="round")
    D.el(cr(outer[:m:2] + [outer[m]], closed=False, nd=2), stroke=shade(HORN_DEEP, 0.8), sw=0.16, so=0.7, cap="round")


def near_horn(rng):
    """La corne du devant : sombre (gris ardoise presque noir, § 6.3), de
    section triangulaire arrondie à la base, annelée de crêtes serrées, plus
    profondes à la base et sur la face extérieure, quelques sillons de
    croissance plus marqués ; la pointe lisse, un peu usée et plus claire."""
    C, W, N, acc, outer, inner = horn_geometry()
    n = len(C)
    total = acc[-1]
    center = HL(*HORN_CENTER_L)
    # son ombre sur la joue : la pointe passe d'un doigt de la joue, elle ne
    # la touche pas (§ 6.2)
    j0 = int(n * 0.66)
    sh = [(inner[i][0] + 0.6, inner[i][1] - 0.8) for i in range(j0, n)]
    so_ = [(outer[i][0] + 0.6, outer[i][1] - 0.8) for i in range(j0, n)]
    D.el(cr(sh + so_[::-1], nd=2), fill="#2A2420", fo=0.22)
    gh = D.rad("corne", center, 7.5, [(0, HORN_DEEP), (0.3, HORN_SHADOW), (0.55, HORN), (0.82, HORN_LIGHT), (1, HORN)])
    shape = outer + inner[::-1]
    D.el(cr(shape[::2] + [shape[-1]], nd=2, t=0.9), fill=gh)
    # la face du dessus de la section, éclairée, le long du bord extérieur
    lit = range(2, int(n * 0.74))
    up = [(C[i][0] + N[i][0] * W[i] * 0.44, C[i][1] + N[i][1] * W[i] * 0.44) for i in lit]
    mid = [(C[i][0] + N[i][0] * W[i] * 0.02, C[i][1] + N[i][1] * W[i] * 0.02) for i in lit]
    D.el(cr(up[::2] + mid[::-2], nd=2), fill=HORN_LIGHT, fo=0.3)
    # le creux, sur la face qui regarde la tête
    dn = [(C[i][0] - N[i][0] * W[i] * 0.48, C[i][1] - N[i][1] * W[i] * 0.48) for i in range(2, n - 6)]
    md = [(C[i][0] - N[i][0] * W[i] * 0.1, C[i][1] - N[i][1] * W[i] * 0.1) for i in range(2, n - 6)]
    D.el(cr(dn[::2] + md[::-2], nd=2), fill=HORN_DEEP, fo=0.45)
    # les annelures : des crêtes en travers, serrées (une tous les 4 à 6 mm),
    # un creux sombre et une crête plus claire ; trois sillons de croissance
    dark, light, years = [], [], []
    s = 0.3
    marks = {total * f for f in (0.22, 0.45, 0.64)}
    while s < total - 4.2:
        i = max(1, min(n - 2, next(k for k in range(n) if acc[k] >= s)))
        nx, nh = N[i]
        dx, dh = C[i + 1][0] - C[i - 1][0], C[i + 1][1] - C[i - 1][1]
        L = math.hypot(dx, dh) or 1.0
        tx, th = dx / L, dh / L
        w = W[i] * 0.5
        bow = 0.16 + 0.05 * w
        wob = rng.uniform(-0.08, 0.08)
        p1 = (C[i][0] + nx * w * 0.97, C[i][1] + nh * w * 0.97)
        p2 = (C[i][0] - nx * w * 0.97, C[i][1] - nh * w * 0.97)
        pm = (C[i][0] + tx * (bow + wob), C[i][1] + th * (bow + wob))
        arc = quad(p1, (2 * pm[0] - (p1[0] + p2[0]) / 2, 2 * pm[1] - (p1[1] + p2[1]) / 2), p2, nd=2)
        if any(abs(s - m) < 0.3 for m in marks):
            years.append(arc)
        else:
            dark.append(arc)
        q1 = (p1[0] + tx * 0.16, p1[1] + th * 0.16)
        q2 = (C[i][0] - nx * w * 0.3 + tx * 0.16, C[i][1] - nh * w * 0.3 + th * 0.16)
        qm = (pm[0] + tx * 0.16, pm[1] + th * 0.16)
        light.append(quad(q1, (2 * qm[0] - (q1[0] + q2[0]) / 2, 2 * qm[1] - (q1[1] + q2[1]) / 2), q2, nd=2))
        s += rng.uniform(0.4, 0.58) * (1.0 + 0.15 * s / total)
    D.el("".join(dark), stroke=HORN_DEEP, sw=0.11, so=0.75, cap="round")
    D.el("".join(light), stroke=HORN_LIGHT, sw=0.09, so=0.7, cap="round")
    D.el("".join(years), stroke="#0A090B", sw=0.2, so=0.85, cap="round")
    # la pointe, lisse et plus claire
    m = next(k for k in range(n) if acc[k] >= total - 4.6)
    gt = D.lin("pointe", C[m], C[-1], [(0, HORN_TIP, 0.0), (0.35, HORN_TIP), (1, HORN_TIP_LIGHT)])
    D.el(cr(outer[m:] + inner[m:][::-1], nd=2), fill=gt)
    D.line([(C[i][0] + N[i][0] * W[i] * 0.2, C[i][1] + N[i][1] * W[i] * 0.2) for i in range(m, n - 2)],
           HORN_TIP_LIGHT, 0.2, so=0.55)
    # le contour, sombre, le long des bords
    D.el(cr(shape[::2] + [shape[-1]], nd=2, t=0.9), stroke=HORN_DEEP, sw=0.18, so=0.8, join="round")


# ================================================================== la lèvre

def gum():
    """Le bourrelet dentaire (le mouton n'a pas d'incisives en haut), caché
    sous la lèvre tant qu'elle est baissée : ce que découvre le flehmen. Il
    reste au-dessus de la ligne de la bouche ; la gencive est sombre, comme
    la peau du museau noir."""
    Q = A.Q
    pad = HLs([(17.75, 3.4), (19.0, 3.1), (20.2, 2.92), (20.75, 2.9), (21.05, 3.08), (20.85, 3.42), (20.3, 3.64),
               (19.2, 3.86), (18.2, 4.02), (17.5, 4.04)])
    return [ET.Element(Q + "path", {"d": cr(pad, nd=2), "fill": GUM}),
            ET.Element(Q + "path", {"d": cr(HLs([(17.9, 3.9), (19.2, 3.72), (20.3, 3.5), (20.85, 3.27)]), closed=False,
                                            nd=2), "fill": "none", "stroke": GUM_DARK, "stroke-width": "0.16",
                                    "stroke-linecap": "round"}),
            ET.Element(Q + "path", {"d": cr(HLs([(17.9, 3.45), (19.2, 3.2), (20.3, 3.02), (20.85, 3.05)]), closed=False,
                                            nd=2), "fill": "none", "stroke": "#B08C8E", "stroke-width": "0.12",
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
    # dessous (le crème de la face, son modelé, son poil) est caché par ce
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
    W, H = 112, 92
    svg = (f'<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 {W} {H}" width="{W}" height="{H}">\n'
           "<!--\n"
           "  Badger Face Welsh Mountain, type Torddu : un bélier cornu de trois ans en fin\n"
           "  d'été, de profil, tourné vers la droite. 1 unité = 1 cm de l'animal\n"
           f"  ({GARROT:.0f} cm au garrot). Corps crème souligné d'une bande noire continue,\n"
           "  face barrée de noir, pattes noires rayées de fauve, cornes sombres d'un tour,\n"
           "  d'après docs/races/badger-face-welsh.md ; écrit par\n"
           "  tools/races/badger-face-welsh/gen_badger_face_welsh.py.\n"
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
    """La brebis ou l'agneau : ce qui change du bélier, puis ses proportions."""
    root = ET.fromstring(svg)
    if VARIANTE == "brebis":
        # Petite mamelle, noire comme le ventre (§ 1.5).
        variantes.mamelle(root, *variantes.sous_le_ventre(root, 9, -2.5), largeur=8, hauteur=5,
                  peau="#3A332E", ombre="#191513")
    variantes.ecrire(path, root, **PROPORTIONS[VARIANTE])


if __name__ == "__main__":
    main(sys.argv[1] if len(sys.argv) > 1 else "badger-face-welsh.svg")
