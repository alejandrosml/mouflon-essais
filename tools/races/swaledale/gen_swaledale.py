#!/usr/bin/env python3
"""Génère le rig SVG du Swaledale : un bélier (« tup ») de deux à trois ans,
en octobre, la toison repoussée de trois à quatre mois après la tonte de
juillet (5 à 8 cm, serrée, bouclée en surface), de profil, tourné vers la
droite, d'après la fiche docs/races/swaledale.md.

Repère de travail : le « monde », en centimètres de l'animal réel. x vers
l'avant (0 à l'arrière de la culotte), h vers le haut (0 au sol). Dans le SVG,
1 unité = 1 cm : X = x + OX, Y = OY - h (voir outils.py). La tête a son propre
repère : la nuque à l'origine, a le long du chanfrein vers le bout du nez, b
vers la mâchoire.

Le dessin est d'abord écrit d'une pièce par patte, l'encolure à part sous le
corps (la variante A du contrat, comme le Mannequin et la Thônes et
Marthod), puis recoupé par tools/races/articulations.py : chaque patte au
carpe ou au jarret, la lèvre supérieure détachée de la tête avec le
bourrelet dentaire dessous.

Usage : gen_swaledale.py <sortie.svg>

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
    "brebis": dict(  # Brebis Swaledale : 62 cm au garrot, cornes minces d'un demi-tour à trois quarts de tour, tête plus fine.
        garrot=(72, 62), encolure=(1.0, .86), tete=(.96, .93)),
    "agneau": dict(  # Agneau Swaledale de deux semaines : 37 cm au garrot, face presque toute noire, sans cornes.
        variantes.AGNEAU, garrot=(72, 37), corps=(.84, .88)),
}
from outils import (OY, S, Doc, Marks, Noise1D, cr, dense, displace, ellipse_pts, geom_d, hspan,  # noqa: E402
                    lerp, mix, normals, num, outward, poisson, poly, polys_of, ramp, resample, shade,
                    smoothstep, to_poly, vspan)
from shapely.geometry import LineString, Point, Polygon  # noqa: E402
from shapely.ops import unary_union  # noqa: E402

# ------------------------------------------------------------------ mesures (fiche, § 2)

GARROT = 72.0          # hauteur au garrot, toison comprise (§ 2.2 : 72 cm [EST])
LONGUEUR = 94.0        # pointe du poitrail -> arrière de la culotte (§ 2.2 : 1,30 H)
STERNUM = 33.0         # dégagement sous le sternum (§ 2.2 : 0,46 H)
VENTRE = 31.4          # dégagement sous le ventre (§ 2.2 : 0,43 H)
TETE = 27.0            # de la nuque au bout du museau (§ 2.2 : HL = 0,34 H, mesurée de 0,29 à 0,42 H sur
                      # les photos ; 0,375 H : la tête forte du bélier de p1 et p7, lisible à l'écran)
OREILLE = 9.3          # de la base à la pointe (§ 2.2 : 0,38 HL), raccourcie de profil
CARPE = 15.8           # centre du carpe (§ 2.6 : 0,22 H)
JARRET = 20.2          # l'articulation du jarret, sous sa pointe (§ 2.6 : pointe à 0,29 H)
BOULET = 5.0           # boulet (§ 2.6 : 0,07 H)
CULOTTE = 28.5         # bas de la laine sur la jambe : à mi-hauteur de la jambe (§ 2.4, photo p3)
QUEUE_BOUT = 11.0      # le bout de la queue, sous le jarret (§ 2.2 : 0,12-0,2 H du sol ; p3, p8)
# Le garrot et la nuque : l'encolure de 0,37 H, portée à 45° au-dessus de
# l'horizontale (§ 2.5 : 35-45° au repos, 50-55° en concours) ; la tête un peu
# relevée, le chanfrein à 34° sous l'horizontale (un bélier « vif et sur ses
# gardes », § 8).
GARROT_PT = (70.5, 72.9)
NUQUE = (89.0, 90.2)
THETA = math.radians(34.0)
# Les pattes (x du canon) : l'antérieur sous le garrot, le postérieur sous la
# pointe de la fesse (§ 2.6) ; celles du fond décalées pour qu'on voie les
# quatre. Les épaules et les hanches (pivots des pattes), dans le corps.
FORE_NEAR, FORE_FAR = 73.0, 66.6
HIND_NEAR, HIND_FAR = 9.6, 16.2
SHOULDER_H, HIP_H = 50.0, 52.0
# La base de l'encolure (pivot-neck) : devant la pointe de l'épaule, un peu
# sous la mi-hauteur de la base du cou ; d'ici l'encolure et la tête mises
# bout à bout posent le museau dans l'herbe, devant les antérieurs.
NECK_BASE = (80.0, 55.0)

# ------------------------------------------------------------------ palette (fiche, § 0 et § 4.8)

WOOL_LIGHT, WOOL, WOOL_SHADOW, WOOL_DEEP = "#E3DDD6", "#BBAB9F", "#857870", "#5F5A4E"
BLACK_SHEEN, BLACK, BLACK_DEEP = "#2E3137", "#15141A", "#07070B"
WHITE_LIGHT, WHITE, WHITE_SHADOW = "#F7F8FA", "#E7E7E9", "#ADABB1"
FROST = "#8C8A90"
THROAT_LIGHT, THROAT, THROAT_SHADOW = "#8C8A90", "#625A60", "#413B40"
EAR_LIGHT, EAR, EAR_SHADOW, EAR_SPECK, EAR_IN = "#D4D7DD", "#B4B7BE", "#5B5E60", "#2A2C33", "#C9A9A6"
NOSE_SHEEN, NOSE, NOSTRIL = "#60626A", "#2C2325", "#0B090D"
HORN_LIGHT, HORN, HORN_SHADOW, HORN_GROOVE, HORN_BASE = "#D6C1A6", "#9A7A63", "#684A3B", "#4C3830", "#5E5250"
HOOF_LIGHT, HOOF, HOOF_DEEP, HOOF_WORN = "#50525A", "#3A3C44", "#1E2028", "#706455"
IRIS_LIGHT, IRIS, LIMBUS, PUPIL, CATCH = "#A07850", "#7A5A3E", "#3B2319", "#0B0A0E", "#E8E6E0"
GUM, GUM_DARK = "#C48790", "#94606A"
SKIN = "#E2D2C6"

D = Doc()


# ================================================================== la toison

# La couleur d'un paquet de laine selon sa lumière (0 : dans l'ombre du
# ventre, 1 : le dos au soleil).
PACK_STOPS = [(0.0, "#7B6F67"), (0.28, WOOL_SHADOW), (0.55, WOOL), (0.8, "#D2C8BE"), (1.0, WOOL_LIGHT)]


def wool(light):
    return ramp(light, PACK_STOPS)


def bucket(light, n=12):
    """La lumière ramenée à quelques niveaux : les brins et les boucles
    voisins partagent leurs teintes, sans bandes franches sur le flanc."""
    return round(max(0.0, min(1.0, light)) * (n - 1)) / (n - 1)


def pack_shape(c, R, rng, squash=1.1, n=20):
    """Un paquet de laine de contour irrégulier et de volume inégal : des
    lobes arrondis et des bouts un peu allongés, sans forme répétée."""
    lobes = rng.choice((3, 4, 5, 6))
    ph = rng.uniform(0, 2 * math.pi)
    ph2 = rng.uniform(0, 2 * math.pi)
    tilt = rng.uniform(-0.45, 0.45)
    ct, st = math.cos(tilt), math.sin(tilt)
    pts = []
    for k in range(n):
        a = 2 * math.pi * k / n
        r = R * (1.0 + 0.09 * math.cos(lobes * a + ph) + 0.1 * math.cos(2 * a + ph2)) * rng.uniform(0.97, 1.03)
        ex, eh = r * math.cos(a), r * math.sin(a) * squash
        pts.append((c[0] + ex * ct - eh * st, c[1] + ex * st + eh * ct))
    return pts


class Fleece:
    """Une couche de toison d'automne : des paquets de 2 à 4 cm, serrés, au
    bout bouclé, séparés par des sillons sombres (§ 3.3). Les paquets sont
    peints du bas vers le haut avec un léger modelé local ; leurs brins,
    sillons et boucles sont groupés par style."""

    def __init__(self):
        self.tops = {}
        self.order = []
        self.marks = Marks()

    def pack(self, c, R, light, rng, far=False, curls=True):
        b = bucket(light)
        shape = cr(pack_shape(c, R, rng, rng.uniform(0.9, 1.45)), nd=2)
        self.order.append((b, shape, c, R))
        k = 0.62 if far else 1.0
        base = wool(b)
        # le dessus du paquet, tourné vers la lumière (en haut, à gauche)
        top = (c[0] - 0.16 * R, c[1] + 0.2 * R)
        self.tops.setdefault(b, []).append(cr(pack_shape(top, R * rng.uniform(0.5, 0.8), rng, 1.15), nd=2, t=0.9))
        # Un sillon interrompu, de biais : jamais un U identique sous chaque
        # paquet. Les mèches d'automne s'entremêlent au lieu de faire un pavage.
        if rng.random() < 0.55:
            a0 = math.radians(rng.uniform(-185, -135))
            a1 = a0 + rng.uniform(1.2, 2.3)
            r = R * rng.uniform(0.62, 0.92)
            self._arc(c, r, a0, a1, (shade(base, 0.58 * k), 0.14, 0.55))
            self._arc((c[0] - 0.04, c[1] + 0.14), r, a0 + 0.1, a1 - 0.1,
                      (mix(base, WOOL_LIGHT, 0.7), 0.07, 0.55))
        # Quelques séparations plus longues entre les mèches, surtout sur
        # l'épaule et le col (p1, p5) : une fente courte inclinée vers le bas,
        # avec une lèvre claire, jamais un quadrillage du flanc.
        if 71.0 < c[0] < 94.0 and c[1] > 45.0 and rng.random() < 0.2:
            p0 = (c[0] + 0.2 * R, c[1] + 0.7 * R)
            pm = (c[0] - 0.15 * R, c[1])
            p1 = (c[0] - 0.1 * R, c[1] - 0.85 * R)
            self.marks.add((shade(base, 0.64), 0.16, 0.45), p0, (c[0] - 0.45 * R, c[1] + 0.2 * R), pm)
            self.marks.add((shade(base, 0.64), 0.13, 0.4), pm, (c[0] - 0.05 * R, c[1] - 0.35 * R), p1)
            self.marks.add((mix(base, WOOL_LIGHT, 0.65), 0.09, 0.5),
                           (p0[0] - 0.16, p0[1]), (pm[0] - 0.3, pm[1] + 0.2), (p1[0] - 0.16, p1[1]))
        # Fins brins ondulés qui traversent le paquet, avec des longueurs
        # différentes : le grain de la laine doit rester fibreux en grand.
        for j in range(rng.randint(6, 10)):
            x = c[0] + rng.uniform(-0.8, 0.8) * R
            h = c[1] + rng.uniform(-0.55, 0.75) * R
            length = R * rng.uniform(0.35, 0.8)
            drift = rng.uniform(-0.4, 0.15) * length
            bend = rng.uniform(0.08, 0.25) * R
            p0 = (x, h)
            pm = (x + drift * 0.5 + bend, h - length * 0.5)
            p1 = (x + drift, h - length)
            col = mix(base, WOOL_LIGHT, 0.6) if j % 3 else shade(base, 0.72)
            style = (col, 0.055 if j % 3 else 0.045, 0.6 if j % 3 else 0.4)
            self.marks.add(style, p0, (x + bend, h - length * 0.2), pm)
            self.marks.add(style, pm, (p1[0] - bend * 0.5, h - length * 0.8), p1)
        if curls:
            # des boucles : le bout frisé des petites mèches serrées
            for j in range(rng.choice((2, 3, 3, 4))):
                p = (c[0] + rng.uniform(-0.5, 0.5) * R, c[1] + rng.uniform(-0.35, 0.4) * R)
                r = rng.uniform(0.14, 0.32)
                a = rng.uniform(0, 2 * math.pi)
                sweep = rng.choice((-1, 1)) * rng.uniform(3.6, 4.8)
                col = mix(base, "#FFFFFF", 0.35) if j % 2 == 0 else shade(base, 0.74)
                self._curl(p, r, a, sweep, (col if not far else shade(col, 0.8), 0.065, 0.65))

    def _arc(self, c, r, a0, a1, style):
        p0 = (c[0] + r * math.cos(a0), c[1] + r * math.sin(a0))
        p1 = (c[0] + r * math.cos(a1), c[1] + r * math.sin(a1))
        am = (a0 + a1) / 2
        k = 1.0 / max(0.3, math.cos((a1 - a0) / 2))
        ctl = (c[0] + r * k * math.cos(am), c[1] + r * k * math.sin(am))
        self.marks.add(style, p0, ctl, p1)

    def _curl(self, c, r, a, sweep, style):
        # une boucle presque fermée, en deux courbes
        p0 = (c[0] + r * math.cos(a), c[1] + r * math.sin(a))
        am = a + sweep / 2
        pm = (c[0] + r * 0.9 * math.cos(am), c[1] + r * 0.9 * math.sin(am))
        p1 = (c[0] + r * 0.75 * math.cos(a + sweep), c[1] + r * 0.75 * math.sin(a + sweep))
        k = 1.0 / max(0.3, math.cos(sweep / 4))
        c1 = (c[0] + r * k * math.cos(a + sweep / 4), c[1] + r * k * math.sin(a + sweep / 4))
        c2 = (c[0] + r * 0.85 * k * math.cos(a + 3 * sweep / 4), c[1] + r * 0.85 * k * math.sin(a + 3 * sweep / 4))
        self.marks.add(style, p0, c1, pm)
        self.marks.add(style, pm, c2, p1)

    def emit(self, cls=None):
        # L'ordre par bandes de 3 cm conserve le recouvrement du bas vers
        # le haut ; les mèches voisines d'une même teinte partagent un
        # tracé et son modelé. Le moteur parcourt ainsi quelques centaines
        # de remplissages, pas un remplissage pour chaque mèche.
        bands = {}
        for b, shape, c, R in self.order:
            row = math.floor(c[1] / 3.0)
            bands.setdefault((row, b), []).append(shape)
        for (row, b), shapes in sorted(bands.items()):
            gid = f"meche-{len(D.defs)}"
            base = wool(b)
            h = row * 3.0 + 1.5
            g = D.lin(gid, (0, h + 2.0), (0, h - 2.0),
                      [(0, mix(base, WOOL_LIGHT, 0.13)), (0.5, base), (1, shade(base, 0.86))])
            D.el("".join(shapes), fill=g, cls=cls)
        for b in sorted(self.tops):
            D.el("".join(self.tops[b]), fill=mix(wool(b), WOOL_LIGHT, 0.35), fo=0.12, cls=cls)
        self.tops = {}
        self.order = []
        self.marks.emit(D)


def fill_fleece(fl, region, rng, light_of, r=2.15, R=1.55, far=False, edge=None, edge_step=1.7, edge_R=1.2,
                skip=None):
    """Sème des paquets dans `region` (un polygone du monde) ; `edge` : le
    contour (fermé) dont les paquets du bord dessinent les festons, sauf là
    où `skip(x, h)` le dit."""
    pts = poisson(region.buffer(-0.45), r * 0.85, rng)
    pts.sort(key=lambda p: (p[1], p[0]))
    for p in pts:
        light = light_of(*p) + rng.uniform(-0.025, 0.025)
        fl.pack(p, R * rng.uniform(0.8, 1.4), light, rng, far=far)
    if edge is not None:
        ring = resample(edge, edge_step * 0.7, closed=True)
        ns = outward(ring)
        for p, n in zip(ring, ns):
            if skip and skip(*p):
                continue
            size = edge_R * rng.uniform(0.5, 1.25)
            inset = rng.uniform(0.3, 0.75) * size
            c = (p[0] - n[0] * inset, p[1] - n[1] * inset)
            fl.pack(c, size, light_of(*c) + rng.uniform(-0.04, 0.04), rng, far=far,
                    curls=rng.random() < 0.7)


def staples(x, h):
    """Les paquets de mèches voisins se ressemblent : une variation douce de
    la lumière, à l'échelle de quelques centimètres."""
    return 0.07 * math.sin(x / 5.3 + h / 3.7) * math.sin(h / 4.9 - x / 7.9 + 1.3) + 0.04 * math.sin(x / 2.9 - h / 6.1)


# ================================================================== le corps

# La silhouette de la toison du corps (sans l'encolure), dans le sens des
# aiguilles d'une montre depuis l'attache de la queue : croupe longue qui
# s'arrondit à peine, dos droit et horizontal, garrot noyé dans la laine, le
# devant qui recouvre la base de l'encolure, poitrail descendu et bombé, ventre
# presque droit à peine remonté au flanc, le haut de la culotte (le bas est sur
# les pattes). § 2.4 et photos p1, p3, p4, p7.
BODY = [
    (2.6, 65.0), (4.8, 67.9), (8.6, 70.2), (14.0, 71.5), (21.0, 72.0), (30.0, 72.1), (40.0, 71.9),
    (50.0, 71.7), (59.0, 71.8), (65.5, 72.2), (70.5, 72.9),
    (74.0, 72.6), (77.0, 71.2), (80.0, 68.8), (83.0, 65.7), (85.9, 61.8), (88.5, 57.4), (90.7, 52.8),
    (92.5, 48.6), (93.7, 44.8), (93.9, 41.4), (93.1, 38.4), (91.0, 35.8), (87.4, 34.0), (82.6, 33.2),
    (77.0, 33.0), (71.0, 33.1), (64.0, 32.5), (56.0, 31.8), (47.0, 31.4), (38.5, 31.8), (31.0, 33.1),
    (25.5, 35.3), (21.6, 37.6), (17.2, 39.3), (12.0, 40.1), (7.4, 40.4), (3.9, 41.3), (1.5, 43.6),
    (0.3, 47.0), (0.0, 51.0), (0.1, 55.5), (0.6, 59.8), (1.4, 62.8),
]
BODY_D = dense(BODY, True, 8)
BODY_POLY = to_poly(BODY_D)


def body_light(x, h):
    # la lumière d'en haut : du ventre (dans l'ombre) au dos, qui court
    # droit du garrot à la croupe ; à l'avant de l'épaule, où le bord du
    # corps descend vers le poitrail, ce n'est plus le dos
    sp = vspan(BODY_D, x)
    hb = sp[0] if sp else 32.0
    ht = 72.0 if 6.0 < x < 72.0 else (sp[1] if sp and x <= 6.0 else 72.0)
    tv = max(0.0, min(1.0, (h - hb) / max(1.0, ht - hb)))
    light = 0.27 + 0.64 * tv ** 0.85 + staples(x, h)
    # le poitrail et l'avant de l'épaule, tournés vers l'avant, dans l'ombre ;
    # l'arrière de la culotte aussi, un peu
    if x > 80:
        light -= 0.16 * min(1.0, (x - 80) / 12.0) * (1.0 - 0.6 * tv)
    if x < 6:
        light -= 0.08 * (6 - x) / 6
    return light


def scrotum():
    """Le scrotum du bélier, gros et laineux, gris-blanc, entre les cuisses
    (§ 2.4) : on n'en voit que le bas, sous le ventre, devant la jambe."""
    rng = random.Random(1919)
    pts = [(20.0, 37.5), (24.6, 36.8), (27.0, 33.8), (27.4, 29.8), (26.2, 26.4), (23.6, 24.6), (20.6, 25.0),
           (18.6, 27.6), (18.0, 31.6), (18.4, 35.2)]
    g = D.lin("scrotum", (22, 37), (22, 24.5), [(0, "#6C625C"), (0.45, "#8E847D"), (1, "#5A524E")])
    D.el(cr(pts, nd=2), fill=g)
    fl = Fleece()
    region = to_poly(dense(pts, True, 6))
    for p in poisson(region.buffer(-0.5), 1.25, rng):
        light = 0.18 + 0.2 * (p[1] - 24.6) / 12.0 + rng.uniform(-0.05, 0.05)
        fl.pack(p, 0.92 * rng.uniform(0.85, 1.1), light, rng)
    fl.emit()
    # l'ombre de la cuisse sur son arrière
    D.el(cr([(18.4, 35.0), (18.0, 31.4), (18.6, 27.6), (20.4, 25.4), (19.6, 29.0), (19.8, 33.4)], nd=2),
         fill="#3E3733", fo=0.45)


def body():
    rng = random.Random(1920)  # le premier livre généalogique de la race (S3)
    D.comment("Corps : la toison d'automne, paquets serrés bouclés en surface ; la base de l'encolure dessous")
    D.open("body")
    fond = D.lin("fond", (0, 73), (0, 31), [(0, wool(0.95)), (0.5, wool(0.62)), (1, wool(0.16))])
    # le fond (les creux entre les paquets) ; sur le devant, en retrait du
    # bord, pour que ce soient les paquets qui le dessinent
    front = BODY_POLY.difference(BODY_POLY.buffer(-1.2)).intersection(
        Polygon([(71.0, 44.0), (100.0, 44.0), (100.0, 80.0), (71.0, 80.0)]))
    D.el(geom_d(BODY_POLY.difference(front)), fill=fond)
    # le volume du tonneau : plus clair en haut, plus sombre au pourtour
    vol = D.ell("vol", (48, 60), 52, 26, 0.0, [(0, "#FFFFFF", 0.16), (0.5, "#FFFFFF", 0.0), (0.8, "#2A231E", 0.0),
                                               (1, "#2A231E", 0.32)])
    D.el(cr(BODY, nd=2), fill=vol)
    if not VARIANTE:
        scrotum()
    fl = Fleece()
    # pas de festons sur le devant qui recouvre l'encolure : les paquets du
    # bord la chevauchent, comme un pli de la toison
    fill_fleece(fl, BODY_POLY, rng, body_light, edge=BODY_D, skip=lambda x, h: x > 71.0 and h > 43.0)
    fl.emit()
    D.pivot("body", 50.0, 31.5)
    D.close()


# ================================================================== les pattes

def ragged(pts, seed, amp=0.28, wl=1.4):
    """Un bord de tache un peu irrégulier : le contour échantillonné, poussé
    le long de sa normale par un bruit lisse."""
    dense_pts = resample(dense(pts, True, 6), 0.25, closed=True)
    return displace(dense_pts, Noise1D(seed, [(wl, 1.0), (wl * 0.45, 0.45)]), amp)


def blob(c, rx, ry, rot, seed, n=14):
    """Une petite tache ronde et irrégulière (moucheture)."""
    r = random.Random(seed)
    k1, k2 = r.uniform(0, 6.3), r.uniform(0, 6.3)
    pts = []
    for i in range(n):
        a = 2 * math.pi * i / n
        rr = 1 + 0.18 * math.sin(2 * a + k1) + 0.1 * math.sin(5 * a + k2)
        ex, eh = rx * rr * math.cos(a), ry * rr * math.sin(a)
        pts.append((c[0] + ex * math.cos(rot) - eh * math.sin(rot), c[1] + ex * math.sin(rot) + eh * math.cos(rot)))
    return pts


def foreleg_outline(cx):
    """Antérieur, d'une pièce de l'épaule à la couronne (le sabot à part) :
    avant-bras large, genou marqué, canon plat et large (« good flat bone »,
    § 7), boulet, paturon court à 55-60°."""
    front = [(cx + 5.4, 48.0), (cx + 4.9, 41.0), (cx + 4.2, 35.5), (cx + 3.6, 30.0), (cx + 3.0, 24.6), (cx + 2.6, 20.4),
             (cx + 2.75, 18.2), (cx + 2.9, 16.6), (cx + 2.55, 14.6), (cx + 2.05, 12.6), (cx + 1.9, 10.0),
             (cx + 1.95, 7.8), (cx + 2.3, 6.2), (cx + 3.0, 4.9), (cx + 3.95, 3.75)]
    back = [(cx - 0.7, 3.25), (cx - 1.35, 3.7), (cx - 1.95, 5.0), (cx - 2.15, 6.5), (cx - 1.95, 8.6), (cx - 1.8, 11.5),
            (cx - 1.95, 13.6), (cx - 2.45, 15.2), (cx - 2.6, 16.8), (cx - 2.45, 18.6), (cx - 2.6, 21.0), (cx - 3.0, 24.6),
            (cx - 3.6, 28.5), (cx - 4.4, 32.4), (cx - 5.6, 36.0), (cx - 6.6, 40.0), (cx - 7.0, 48.0)]
    return front, back


def hindleg_outline(cx):
    """Postérieur, d'une pièce de la hanche à la couronne : le grasset en avant
    dans la toison, la jambe qui redescend vers le jarret (pointe à 0,29 H),
    le canon vertical sous la pointe de la fesse, le paturon court."""
    front = [(cx + 14.5, 50.0), (cx + 15.0, 44.5), (cx + 14.0, 39.0), (cx + 11.0, 34.0), (cx + 7.6, 29.6),
             (cx + 4.8, 25.6), (cx + 3.0, 22.4), (cx + 2.3, 20.0), (cx + 2.0, 17.2), (cx + 1.85, 13.0),
             (cx + 1.85, 9.6), (cx + 2.0, 7.6), (cx + 2.4, 6.1), (cx + 3.1, 4.8), (cx + 4.0, 3.7)]
    back = [(cx - 0.7, 3.2), (cx - 1.35, 3.65), (cx - 2.0, 5.0), (cx - 2.25, 6.5), (cx - 2.05, 8.8), (cx - 1.9, 12.0),
            (cx - 2.0, 15.5), (cx - 2.3, 18.2), (cx - 2.9, 20.0), (cx - 3.5, 21.4), (cx - 3.4, 22.8),
            (cx - 3.15, 24.6), (cx - 3.5, 27.6), (cx - 4.5, 31.6), (cx - 5.8, 36.5), (cx - 6.8, 42.0), (cx - 6.5, 50.0)]
    return front, back


def leg_shape(front, back):
    pts = dense(front, False, 6) + dense(back, False, 6)
    return pts, to_poly(pts)


def hoof(cx, far, name):
    """L'onglon de profil : deux doigts gris ardoise, mats, la pince un peu
    usée (§ 7, photo p3), le doigt de l'autre côté qui dépasse à peine
    devant ; la couronne, où le poil blanc retombe."""
    k = 0.7 if far else 1.0
    # l'autre doigt, un peu plus bas et en avant, dans l'ombre
    other = [(cx + 3.9, 3.3), (cx + 4.7, 2.0), (cx + 5.55, 0.5), (cx + 5.8, 0.0), (cx + 2.6, 0.0), (cx + 2.4, 1.6)]
    D.el(cr(other, nd=2, t=0.7), fill=shade(HOOF_DEEP, k))
    wall = [(cx + 3.95, 3.75), (cx + 5.32, 0.0)]
    sole = [(cx + 5.32, 0.0), (cx - 1.25, 0.0)]
    heel = [(cx - 1.25, 0.0), (cx - 1.6, 0.6), (cx - 1.65, 1.6), (cx - 1.35, 2.6), (cx - 0.65, 3.25), (cx + 1.6, 3.7),
            (cx + 3.95, 3.75)]
    g = D.lin(f"{name}-onglon", (cx, 3.8), (cx + 1.2, 0.0),
              [(0, shade(HOOF_LIGHT, k)), (0.35, shade(HOOF, k)), (1, shade(HOOF_DEEP, k))])
    D.el(runs(wall, sole, heel), fill=g)
    # la pince usée, couleur de corne, et la fente entre les deux doigts
    D.el(cr([(cx + 4.2, 1.2), (cx + 4.95, 1.5), (cx + 5.38, 0.4), (cx + 5.3, 0.05), (cx + 4.35, 0.05)], nd=2, t=0.7),
         fill=shade(HOOF_WORN, k), fo=0.55)
    D.line([(cx + 2.2, 3.55), (cx + 2.5, 2.0), (cx + 2.7, 0.15)], shade("#14151A", k), 0.2, so=0.85)
    if not far:
        D.line([(cx - 0.4, 3.0), (cx + 1.8, 3.45), (cx + 3.7, 3.4)], HOOF_LIGHT, 0.22, so=0.7)
        D.line([(cx + 3.1, 3.2), (cx + 3.5, 2.0), (cx + 3.9, 0.6)], "#5E6068", 0.15, so=0.55)
        D.line([(cx - 1.1, 0.25), (cx + 2.0, 0.22), (cx + 5.0, 0.25)], "#101116", 0.3, so=0.6)


def runs(*parts):
    """Un contour fermé fait de suites lisses jointes par des angles (le
    dernier point d'une suite est le premier de la suivante ; une suite de
    deux points est un segment droit)."""
    pts = []
    for part in parts:
        if len(part) == 2:
            seg = list(part)
        else:
            seg = dense(part, False, 5, t=0.9)
        if pts and math.dist(pts[-1], seg[0]) < 1e-6:
            seg = seg[1:]
        pts += seg
    if math.dist(pts[0], pts[-1]) < 1e-6:
        pts = pts[:-1]
    return poly(pts)


def dewclaw(x, h, far):
    D.el(cr([(x - 0.35, h + 0.4), (x + 0.2, h + 0.5), (x + 0.35, h - 0.15), (x - 0.05, h - 0.55), (x - 0.45, h - 0.1)],
            nd=2), fill="#101116" if far else "#23252C")


def leg_hair(leg, black, rng, far, top, marks):
    """Le poil ras des pattes, couché vers le bas : gris pâle sur le blanc,
    gris anthracite sur le noir ; plus long et hirsute à l'arrière du canon
    et au boulet (§ 7)."""
    zone = leg.buffer(-0.25)
    for p in poisson(zone, 0.5, rng):
        if p[1] > top:
            continue
        L = rng.uniform(0.35, 0.7)
        ang = math.radians(-90 + rng.uniform(-12, 8))
        e = (p[0] + L * math.cos(ang), p[1] + L * math.sin(ang))
        c = ((p[0] + e[0]) / 2 + rng.uniform(-0.08, 0.08), (p[1] + e[1]) / 2)
        if black is not None and black.contains(Point(p)):
            marks.add(("#0B0B0F" if far else "#33363C", 0.055, 0.4), p, c, e)
        else:
            if rng.random() < 0.3 and not far:
                marks.add(("#FFFFFF", 0.07, 0.5), p, c, e)
            else:
                marks.add(("#8D8C93" if far else "#C2C1C7", 0.065, 0.4), p, c, e)


def shaggy(points, rng, far, marks, n=14, back=True):
    """Des poils plus longs qui dépassent à l'arrière du canon et du boulet."""
    pts = resample(points, 0.35)
    for p in pts:
        if rng.random() < 0.45:
            continue
        L = rng.uniform(0.6, 1.3)
        ang = math.radians(rng.uniform(205, 245) if back else rng.uniform(-60, -20))
        e = (p[0] + L * math.cos(ang), p[1] + L * math.sin(ang))
        c = ((p[0] + e[0]) / 2 + rng.uniform(-0.1, 0.1), (p[1] + e[1]) / 2 + 0.12)
        marks.add(("#A5A4AA" if far else "#F2F2F4", 0.12, 0.8), (p[0] + 0.25, p[1]), c, e)


def mark_edges(black, leg, rng, far, marks):
    """Les bords des marbrures : nets mais poilus, des poils blancs qui mordent
    sur le noir et des poils noirs sur le blanc."""
    for g in polys_of(black):
        ring = list(g.exterior.coords)
        line = LineString(ring).intersection(leg.buffer(-0.35))
        segs = [list(s.coords) for s in getattr(line, "geoms", [line]) if s.geom_type == "LineString"]
        for sgm in segs:
            if len(sgm) < 2:
                continue
            for p in resample(sgm, 0.32):
                if rng.random() < 0.35:
                    continue
                L = rng.uniform(0.35, 0.75)
                ang = math.radians(-90 + rng.uniform(-35, 35))
                inside_black = rng.random() < 0.5
                e = (p[0] + L * math.cos(ang), p[1] + L * math.sin(ang))
                c = ((p[0] + e[0]) / 2, (p[1] + e[1]) / 2)
                if inside_black:
                    marks.add(("#9C9BA1" if far else "#E9E9EC", 0.12, 0.75), p, c, e)
                else:
                    marks.add(("#0A0A0D" if far else "#16151B", 0.13, 0.8), p, c, e)


def leg_group(name, kind, cx, far):
    """Une patte d'une pièce : le poil blanc marbré de noir (§ 4.5), le sabot,
    l'ergot ; pour un postérieur, la laine de la culotte sur la cuisse."""
    rng = random.Random(len(name) * 97 + int(cx * 10))
    front, back = (foreleg_outline if kind == "fore" else hindleg_outline)(cx)
    pts, leg = leg_shape(front, back)
    D.comment(f"{name} : d'une pièce, coupée au {'carpe' if kind == 'fore' else 'jarret'} par articulations.py")
    D.open(name)
    k = 0.72 if far else 1.0
    white = D.lin(f"{name}-blanc", (cx - 3.0, 0), (cx + 3.4, 0),
                  [(0, "#F5F6F8"), (0.45, "#E4E4E8"), (1, "#B8B7BE")] if not far else
                  [(0, "#B7B6BC"), (0.5, "#A4A3A9"), (1, "#86858C")])
    D.el(poly(pts), fill=white)
    # les marbrures noires
    if kind == "fore":
        # le devant de l'avant-bras noir, du coude au genou ; l'arrière blanc
        # (§ 4.5) ; chez la patte du fond, tout l'avant-bras (photos p3, p4)
        if far:
            patch = [(cx + 7.0, 48.0), (cx + 5.6, 34.0), (cx + 4.0, 24.0), (cx + 3.3, 18.6), (cx + 3.1, 16.0),
                     (cx + 1.6, 14.8), (cx - 0.6, 15.4), (cx - 2.2, 17.4), (cx - 3.6, 23.0), (cx - 4.6, 30.0),
                     (cx - 8.0, 40.0), (cx - 8.0, 48.0)]
        else:
            patch = [(cx + 7.0, 48.0), (cx + 5.6, 36.0), (cx + 4.4, 30.0), (cx + 3.6, 24.0), (cx + 3.2, 19.4),
                     (cx + 3.2, 16.6), (cx + 2.5, 15.1), (cx + 1.6, 15.4), (cx + 1.1, 17.0), (cx + 0.7, 19.8),
                     (cx + 0.5, 23.0), (cx + 0.1, 26.4), (cx - 0.7, 30.0), (cx - 1.8, 33.4), (cx - 3.2, 37.0),
                     (cx - 5.6, 41.0), (cx - 8.0, 48.0)]
        # sous le genou, le canon blanc taché de noir, plus serré vers le boulet
        spots = [((cx + 1.5, 13.0), 0.6, 1.3, 0.15), ((cx - 0.9, 10.4), 0.8, 0.6, 0.4),
                 ((cx + 1.0, 7.7), 0.85, 0.7, -0.3), ((cx - 1.2, 6.5), 0.65, 0.5, 0.2), ((cx + 0.4, 5.4), 0.5, 0.38, 0.0)]
        if far:
            spots = [((cx + 0.9, 11.5), 1.1, 1.6, 0.2), ((cx - 1.0, 8.0), 0.9, 0.8, 0.4), ((cx + 1.2, 6.3), 0.8, 0.6, 0.0),
                     ((cx - 0.6, 13.4), 0.6, 0.5, 0.0)]
        specks = (14.2, 4.6, 16)
    else:
        # une grande tache noire sur la face externe de la jambe, au-dessus et
        # autour du jarret, sous la culotte ; le tendon reste blanc (photo p3)
        patch = [(cx - 5.4, 31.6), (cx + 7.4, 33.2), (cx + 5.8, 28.2), (cx + 4.0, 25.4), (cx + 2.6, 23.0),
                 (cx + 1.4, 21.2), (cx + 0.4, 19.8), (cx - 0.6, 20.2), (cx - 1.3, 22.0), (cx - 1.8, 24.0),
                 (cx - 2.6, 25.6), (cx - 3.6, 27.4), (cx - 4.6, 29.4)]
        spots = [((cx - 2.6, 20.6), 0.7, 1.0, 0.4), ((cx + 0.9, 15.0), 0.7, 1.1, 0.3), ((cx - 0.8, 12.2), 0.5, 0.7, 0.0),
                 ((cx + 1.1, 10.0), 0.42, 0.5, 0.2), ((cx - 1.1, 8.0), 0.55, 0.45, 0.5), ((cx + 0.5, 6.6), 0.48, 0.4, 0.0),
                 ((cx - 0.3, 17.4), 0.4, 0.55, 0.3)]
        specks = (18.5, 4.8, 22)
        if far:
            patch = [(cx - 8.0, 31.8), (cx + 8.0, 33.4), (cx + 6.0, 27.0), (cx + 3.6, 22.0), (cx + 2.2, 18.0),
                     (cx + 0.4, 16.4), (cx - 1.6, 16.8), (cx - 3.0, 19.6), (cx - 4.6, 24.0), (cx - 6.5, 29.0)]
    # les mouchetures (le « poivré » du canon) : de petits points noirs
    srng = random.Random(int(cx * 31) + (7 if kind == "fore" else 0))
    top_s, bot_s, n_s = specks
    for i in range(n_s):
        h = bot_s + (top_s - bot_s) * srng.random() ** 1.4
        x = cx + srng.uniform(-1.6, 1.7)
        r = srng.uniform(0.12, 0.32)
        spots.append(((x, h), r, r * srng.uniform(0.7, 1.2), srng.uniform(0, 3)))
    black = to_poly(ragged(patch, int(cx * 13) + 5, amp=0.32))
    for i, (c, rx, ry, rot) in enumerate(spots):
        black = black.union(to_poly(blob(c, rx, ry, rot, int(cx * 7) + i)))
    black = black.intersection(leg)
    # quelques taches blanches dans le noir (le noir n'est jamais d'un bloc)
    holes = []
    for i in range(5 if kind == "fore" else 4):
        p = (cx + rng.uniform(-1.5, 2.0), rng.uniform(19.0, 31.0) if kind == "fore" else rng.uniform(21.5, 30.0))
        holes.append(to_poly(blob(p, rng.uniform(0.25, 0.55), rng.uniform(0.3, 0.7), rng.uniform(0, 3), 900 + i)))
    black = black.difference(unary_union(holes))
    gb = D.lin(f"{name}-noir", (cx - 3.0, 0), (cx + 3.5, 0),
               [(0, BLACK_SHEEN), (0.4, BLACK), (1, BLACK_DEEP)] if not far else [(0, "#121216"), (1, "#060608")])
    D.el(geom_d(black), fill=gb)
    marks = Marks()
    mark_edges(black, leg, rng, far, marks)
    top = 48.0
    leg_hair(leg, black, rng, far, top, marks)
    # poils hirsutes à l'arrière du canon et du boulet
    shaggy([(cx - 1.95, 12.5), (cx - 2.05, 9.0), (cx - 2.2, 6.4), (cx - 1.8, 4.6)], rng, far, marks)
    marks.emit(D)
    # le modelé : l'os du carpe, la pointe du jarret, le tendon
    col = "#7F7E86" if not far else "#5E5D64"
    if kind == "fore":
        D.line([(cx - 2.2, 18.2), (cx - 2.0, 16.8), (cx - 1.85, 15.3)], col, 0.3, so=0.4)
        D.line([(cx + 2.4, 18.0), (cx + 2.55, 16.6), (cx + 2.2, 15.0)], col, 0.25, so=0.3)
        D.line([(cx - 1.0, 12.8), (cx - 1.0, 9.0), (cx - 1.1, 7.2)], "#FFFFFF" if not far else "#C2C1C7", 0.35,
               so=0.35)
    else:
        D.line([(cx - 2.7, 26.8), (cx - 2.4, 23.6), (cx - 2.6, 21.6)], col, 0.3, so=0.45)
        D.line([(cx + 2.6, 21.6), (cx + 2.0, 19.8), (cx + 1.8, 18.0)], col, 0.25, so=0.3)
        D.line([(cx - 1.0, 15.6), (cx - 1.0, 11.0), (cx - 1.1, 7.6)], "#FFFFFF" if not far else "#C2C1C7", 0.35,
               so=0.35)
    dewclaw(cx - 2.15, 6.6, far)
    hoof(cx, far, name)
    # la couronne : le poil blanc retombe un peu sur le sabot
    m2 = Marks()
    for i in range(9):
        x = cx - 1.0 + i * 0.6 + rng.uniform(-0.1, 0.1)
        h = 3.25 + 0.5 * (i / 8) + rng.uniform(-0.05, 0.05)
        m2.add(("#C9C8CE" if far else "#F4F4F6", 0.14, 0.85), (x, h + 0.55), (x + 0.1, h + 0.2), (x + 0.2, h - 0.25))
    m2.emit(D)
    if kind == "hind":
        hind_sleeve(name, cx, far, rng)
    D.pivot(name, cx + (5.5 if kind == "hind" else -2.0), HIP_H if kind == "hind" else SHOULDER_H)
    D.close()


def hind_sleeve(name, cx, far, rng):
    """Le bas de la culotte, sur la cuisse et le haut de la jambe : la laine
    qui « descend le long des jarrets » (standard, § 3.1) et s'arrête net à
    mi-hauteur de la jambe (§ 2.4) ; le noir de la jambe déborde sur ses
    dernières mèches (§ 4.5)."""
    # le bord bas de la laine : plus haut devant, au grasset, que derrière
    def edge_h(x):
        return CULOTTE + 0.3 * max(0.0, x - (cx - 1.0))

    outline = [(cx - 8.6, 50.0), (cx - 8.8, 42.0), (cx - 7.6, 35.5), (cx - 6.4, 31.0), (cx - 5.6, edge_h(cx - 5.6))]
    xs = [cx - 5.6 + i * 0.9 for i in range(int((cx + 13.2 - (cx - 5.6)) / 0.9) + 1)]
    bottom = [(x, edge_h(x) + 0.5 * math.sin(x * 2.1 + cx) + rng.uniform(-0.2, 0.2)) for x in xs]
    outline += bottom + [(cx + 16.0, 38.0), (cx + 17.0, 44.5), (cx + 16.0, 50.0)]
    region = to_poly(outline)
    g = D.lin(f"{name}-laine", (0, 50), (0, 27), [(0, "#8A7E74"), (1, "#5B5249")] if not far else
              [(0, "#5E554E"), (1, "#3E3732")])
    D.el(poly(outline), fill=g)
    fl = Fleece()

    def light(x, h):
        v = 0.12 + 0.35 * smoothstep(CULOTTE, 46.0, h) + staples(x, h) - (0.1 if x > cx + 8 else 0.0)
        return v * (0.75 if far else 1.0)

    fill_fleece(fl, region, rng, light, r=2.0, R=1.45, far=far)
    # les dernières mèches du bord, qui pendent un peu ; noires au-dessus de la
    # tache du jarret
    darks = []
    for (x, h) in bottom:
        if rng.random() < 0.15:
            continue
        c = (x + rng.uniform(-0.2, 0.2), h + 0.25)
        if cx - 4.0 < x < cx + 6.0 and rng.random() < 0.22:
            darks.append(cr(pack_shape(c, 0.75, rng, 1.0), nd=1))
        else:
            fl.pack(c, 1.0 * rng.uniform(0.85, 1.1), light(*c), rng, far=far)
    fl.emit()
    D.el("".join(darks), fill="#0E0E12" if far else "#1C1B20")


# ================================================================== la queue

# La queue, longue, épaisse et laineuse (« thick long and woolly », § 2.2) :
# elle part sous la croupe, pend derrière la culotte et finit sous le jarret
# (photos p3, p8 : le bout à 0,11-0,12 H du sol). Trois morceaux, chacun
# pendu à son attache, qui ondulent.
TAIL_CENTER = [(2.6, 63.8), (1.3, 58.0), (0.6, 51.5), (0.5, 45.0), (0.9, 38.5), (1.4, 32.0), (1.8, 25.5),
               (2.0, 19.5), (2.0, 15.0), (1.8, QUEUE_BOUT + 0.6)]
TAIL_CUTS = [(63.8, 36.5), (41.0, 22.0), (26.5, QUEUE_BOUT)]
TAIL_PIVOTS = [(2.6, 63.8), (1.05, 39.4), (1.9, 24.6)]


def tail_half_width(h):
    return 2.35 + 0.65 * smoothstep(QUEUE_BOUT, 60.0, h)


def tail_segment(top, bottom, rng, seg, tassel=False):
    center = dense(TAIL_CENTER, False, 8)
    pts = [p for p in center if bottom - 0.5 <= p[1] <= top + 0.5]
    left = [(x - tail_half_width(h), h) for (x, h) in pts]
    right = [(x + tail_half_width(h), h) for (x, h) in pts]
    # le haut arrondi autour de l'attache (rien ne s'ouvre quand le morceau
    # tourne) ; le bas en houppe pour le dernier
    cx_top, h_top = pts[0]
    cap_top = [(cx_top + tail_half_width(h_top) * math.cos(a), h_top + tail_half_width(h_top) * 0.9 * math.sin(a))
               for a in [math.radians(d) for d in range(10, 171, 20)]]
    cx_b, h_b = pts[-1]
    hw = tail_half_width(h_b)
    if tassel:
        cap_bot = [(cx_b - hw * 0.8, h_b - 1.2), (cx_b - hw * 0.3, h_b - 2.6), (cx_b + 0.2, h_b - 3.2),
                   (cx_b + hw * 0.5, h_b - 2.4), (cx_b + hw * 0.85, h_b - 1.0)]
    else:
        cap_bot = [(cx_b + hw * math.cos(a), h_b + hw * 0.9 * math.sin(a)) for a in
                   [math.radians(d) for d in range(190, 351, 20)]]
    outline = right[::-1] + cap_top + left + cap_bot
    outline = [(x, h) for (x, h) in outline]
    region = to_poly(outline)
    g = D.lin(f"queue-{seg}", (-2.0, 0), (5.0, 0), [(0, "#7C7068"), (0.6, "#6A5F57"), (1, "#4E4640")])
    D.el(cr(outline[::2], nd=2, t=0.9), fill=g)
    fl = Fleece()

    def light(x, h):
        return 0.3 + 0.12 * (1.0 - (x + 1.0) / 6.0) + staples(x * 2.0, h) - 0.08 * smoothstep(30.0, 12.0, h)

    fill_fleece(fl, region, rng, light, r=1.7, R=1.25, edge=dense(outline[::2], True, 3), edge_step=1.5,
                edge_R=0.95)
    fl.emit()


def tail():
    rng = random.Random(1263)  # « Swardal » : le Swaledale dans le parler du Nord (S8)
    D.comment("Queue : longue, épaisse et laineuse, jusque sous le jarret ; trois morceaux qui ondulent")
    names = ["tail", "tail-mid", "tail-tip"]
    for k, (top, bottom) in enumerate(TAIL_CUTS):
        D.open(names[k])
        tail_segment(top, bottom, rng, k, tassel=(k == 2))
    for k in (2, 1, 0):
        D.pivot(names[k], *TAIL_PIVOTS[k])
        D.close()


# ================================================================== l'encolure

# L'encolure, à part, sous le corps (variante A) : la crête épaisse du bélier
# (§ 2.5), la gorge ; sous la tête, une épaisseur cachée au repos ; sous le
# corps, tout le bas autour de sa base, qui ne bouge guère quand elle tourne.
NECK = [
    (62.5, 60.0), (63.6, 66.0), (66.4, 71.0), (70.0, 75.0), (73.8, 78.9), (77.6, 82.4), (81.2, 85.4),
    (84.4, 87.6), (87.2, 89.0), (90.0, 89.4), (92.6, 88.0), (95.0, 84.6), (95.6, 79.4), (94.0, 75.4),
    (91.6, 72.8), (90.6, 70.6), (91.2, 66.4), (92.6, 61.4), (94.0, 56.0), (94.9, 50.4), (95.0, 45.2),
    (93.8, 40.4), (90.0, 37.4), (84.4, 36.0), (78.0, 37.0), (72.0, 40.0), (67.0, 45.2), (63.6, 52.0),
]
NECK_D = dense(NECK, True, 6)
NECK_POLY = to_poly(NECK_D)


def neck_light(x, h):
    # la crête au soleil, la gorge tournée vers le bas dans l'ombre ; la
    # lumière du corps là où ils se raccordent
    sp = hspan(NECK_D, h)
    light = 0.3 + 0.55 * smoothstep(55.0, 88.0, h) + staples(x, h)
    if sp:
        light -= 0.22 * max(0.0, 1.0 - (sp[1] - x) / 7.0)
    if x < 80 and h < 72:
        light = 0.5 * light + 0.5 * body_light(x, h)
    return light


def neck():
    rng = random.Random(1919)  # la société de race, née à Tan Hill (S2)
    D.comment("Encolure : sous le corps et sous la tête, qui la recouvrent à ses deux bouts")
    D.open("neck")
    D.el(cr(NECK, nd=2), fill="url(#fond)")
    D.el(cr(NECK, nd=2), fill="url(#vol)")
    fl = Fleece()
    fill_fleece(fl, NECK_POLY, rng, neck_light, edge=NECK_D)
    fl.emit()
    D.pivot("neck", *NECK_BASE)
    D.close()


# ================================================================== la tête

# La tête est dessinée dans un repère d'une tête de 24,5 cm, agrandi à TETE.
HK = TETE / 24.5


def HW(a, b):
    """Du repère de la tête (la nuque à l'origine, a le long du chanfrein vers
    le bout du nez, b vers la mâchoire ; des centimètres d'une tête de
    24,5 cm) au monde."""
    ca, sa = math.cos(THETA), math.sin(THETA)
    a, b = a * HK, b * HK
    return (NUQUE[0] + a * ca - b * sa, NUQUE[1] - a * sa - b * ca)


def HWs(pts):
    return [HW(a, b) for (a, b) in pts]


# Le profil (§ 5) : droit à très légèrement busqué, une petite convexité
# au-dessus du mufle ; front large et plat sans toupet ; mâchoire profonde ;
# museau large, arrondi. Le haut de l'arrière reste sous la corne (la laine du
# cou s'arrête derrière elle, § 3.2).
DORSAL = [(0.5, 1.8), (0.9, 0.0), (2.0, -1.6), (4.0, -2.7), (6.5, -3.4), (8.8, -3.3), (11.0, -2.6),
          (13.8, -1.8), (16.4, -0.85), (18.8, 0.0), (20.9, 1.0), (22.6, 2.3), (23.8, 3.8), (24.45, 5.45),
          (24.55, 7.0), (24.15, 8.25)]
MOUTH = [(24.15, 8.25), (23.1, 8.9), (21.5, 9.3), (19.8, 9.6), (18.2, 9.85)]
NOTCH = [(18.2, 9.85), (18.05, 11.15), (17.7, 12.4)]
JAWLINE = [(17.7, 12.4), (15.4, 13.05), (12.6, 13.7), (9.8, 14.3), (7.4, 14.8), (5.6, 15.0), (4.2, 14.6)]
BACKLINE = [(4.2, 14.6), (3.2, 13.4), (2.5, 11.4), (2.0, 9.0), (1.5, 6.6), (0.9, 4.2), (0.5, 1.8)]
EYE_L = (9.0, 4.7)          # l'œil : à 0,63 HL du bout du museau, haut sur le côté (§ 5)
EYE_ROT = math.radians(-18.0)   # la fente moins penchée que la tête
JAW_PIVOT_L = (17.2, 10.6)  # la charnière de la mâchoire, sous la joue, à hauteur de la bouche
MUZZLE_L = (23.5, 9.5)      # ce qui touche l'herbe : le bas des lèvres
# La lèvre supérieure (15 % de la tête), de sous le naseau à sous la ligne de
# la bouche ; elle se retrousse autour de son arrière.
LIP_L = [(20.5, 7.3), (21.6, 6.95), (22.8, 6.75), (23.9, 6.65), (24.6, 6.95), (24.65, 7.65), (24.4, 8.45),
         (24.0, 9.05), (23.0, 9.4), (21.6, 9.65), (20.3, 9.8), (19.9, 8.6)]
LIP_PIVOT_L = (20.7, 7.6)
GUM_L = [(21.0, 8.15), (22.2, 7.92), (23.3, 7.78), (23.95, 7.9), (24.05, 8.22), (23.5, 8.5), (22.3, 8.7),
         (21.1, 8.8)]
GUM_RIDGE_L = [(21.3, 8.55), (22.4, 8.42), (23.4, 8.24), (23.85, 8.1)]
# Le bord arrière du museau blanc (§ 4.2) : du chanfrein, à 0,39 HL du bout du
# nez, il descend presque à angle droit du chanfrein, un peu bombé vers
# l'arrière, jusque sous la mâchoire à 0,43 HL (photos p1, p4, p5, p7).
MUZZLE_EDGE_L = [(15.05, -3.6), (15.0, -1.2), (14.75, 1.2), (14.5, 3.7), (14.35, 6.2), (14.35, 8.7),
                 (14.5, 11.2), (14.7, 13.6), (14.9, 16.2)]
# L'anneau blanc de l'œil (§ 4.2) : plus large au-dessus et en arrière,
# mince devant et dessous ; l'œil dans son bas et son avant.
RING_REL = [(-0.7, -3.05), (-1.9, -2.75), (-2.85, -1.95), (-3.2, -0.65), (-2.95, 0.7), (-2.25, 1.75),
            (-1.15, 2.3), (0.1, 2.25), (1.1, 1.65), (1.9, 0.6), (2.15, -0.5), (1.75, -1.6), (0.8, -2.6)]


def head_outline_l():
    """Le contour de la tête (repère de la tête), échantillonné : des suites
    lisses jointes par des angles (le coin de la bouche, l'encoche)."""
    out = []
    for run in (DORSAL, MOUTH, NOTCH, JAWLINE, BACKLINE):
        seg = dense(run, False, 6)
        if out and math.dist(out[-1], seg[0]) < 1e-6:
            seg = seg[1:]
        out += seg
    return out[:-1]


def eye_pt(ex, ey):
    """Un point du repère de l'œil (ex vers l'avant de la fente, ey vers le
    bas), dans le monde."""
    c, s = math.cos(EYE_ROT), math.sin(EYE_ROT)
    return HW(EYE_L[0] + ex * c - ey * s, EYE_L[1] + ex * s + ey * c)


ALMOND = [(-1.45, 0.05), (-0.92, -0.66), (0.0, -0.9), (0.88, -0.7), (1.45, 0.06), (0.9, 0.56), (0.0, 0.7),
          (-0.9, 0.52)]


class Face:
    """La face : ses remplissages et ses poils, gardés pour être redessinés
    autour de l'œil par-dessus le groupe `eyes` (le pourtour immobile qui
    cache l'iris quand il glisse)."""

    def __init__(self):
        self.items = []

    def fill(self, geom, fill, fo=None):
        self.items.append(("fill", geom, fill, fo))

    def marks(self, m):
        self.items.append(("marks", m))

    def line(self, pts, stroke, sw, so=None):
        self.items.append(("line", pts, stroke, sw, so))

    def emit(self, clip=None):
        cp = None
        if clip is not None:
            from shapely.prepared import prep
            cp = prep(clip)
        for it in self.items:
            if it[0] == "fill":
                _, geom, fill, fo = it
                g = geom if clip is None else geom.intersection(clip)
                if not g.is_empty:
                    D.el(geom_d(g, simplify=0.01), fill=fill, fo=fo)
            elif it[0] == "marks":
                m = it[1]
                if clip is None:
                    m.emit(D)
                else:
                    sub = Marks()
                    for style, strokes in sorted(m.raw.items(), key=lambda kv: (kv[0][2], kv[0][0], kv[0][1])):
                        for (p0, c, p1) in strokes:
                            if cp.contains(Point(p0)) and cp.contains(Point(p1)):
                                sub.add(style, p0, c, p1)
                    sub.emit(D)
            else:
                _, pts, stroke, sw, so = it
                if clip is None:
                    D.line(pts, stroke, sw, so=so)
                else:
                    seg = LineString(dense(pts, False, 6)).intersection(clip)
                    for s in getattr(seg, "geoms", [seg]):
                        if s.geom_type == "LineString" and s.length > 0.2:
                            D.el(poly(list(s.coords), closed=False), stroke=stroke, sw=sw, so=so, cap="round",
                                 join="round")


class RawMarks(Marks):
    """Des traits groupés dont on garde aussi les points (pour les redessiner
    rognés)."""

    def __init__(self):
        super().__init__()
        self.raw = {}

    def add(self, style, p0, c, p1):
        super().add(style, p0, c, p1)
        self.raw.setdefault(style, []).append((p0, c, p1))

    def emit(self, doc, order=None):
        groups = self.groups
        super().emit(doc, order)
        self.groups = groups


def ragged_line(pts, seed, amp=0.22, wl=1.1, step=0.2):
    line = resample(dense(pts, False, 6), step)
    return displace(line, Noise1D(seed, [(wl, 1.0), (wl * 0.4, 0.5)]), amp, closed=False)


def frost(line_pts, rng, white_side, black_m, white_m, keep, density=0.8, reach=(0.25, 0.65)):
    """Un bord givré (§ 4.2) : 1 à 2 cm où poils noirs et blancs se mêlent.
    `white_side(p)` : vrai si le point est dans le blanc ; `keep(p)` : vrai
    s'il est sur la tête. Les poils suivent le poil de la face, couché vers
    l'arrière."""
    hair = (-math.cos(THETA), math.sin(THETA))   # vers la nuque, dans le monde
    for p in resample(line_pts, 0.1):
        if rng.random() > density:
            continue
        for into_black in (True, False):
            if rng.random() < 0.2:
                continue
            L = rng.uniform(*reach)
            ang = math.atan2(hair[1], hair[0]) + rng.uniform(-0.9, 0.9)
            d = (math.cos(ang), math.sin(ang))
            # le poil part du bord et s'avance dans l'autre couleur
            q = (p[0] + d[0] * L, p[1] + d[1] * L)
            if white_side(q) == into_black:
                d = (-d[0], -d[1])
                q = (p[0] + d[0] * L, p[1] + d[1] * L)
            start = (p[0] - d[0] * 0.18, p[1] - d[1] * 0.18)
            if not (keep(start) and keep(q)):
                continue
            c = ((start[0] + q[0]) / 2 + rng.uniform(-0.05, 0.05), (start[1] + q[1]) / 2 + rng.uniform(-0.05, 0.05))
            if into_black:
                white_m.add((WHITE if rng.random() < 0.6 else "#C9C8CE", 0.075, 0.85), start, c, q)
            else:
                black_m.add((BLACK if rng.random() < 0.7 else "#34363D", 0.085, 0.85), start, c, q)


def face_items(rng):
    """La face noire au poil ras et dur, le museau blanc et l'anneau blanc de
    l'œil, séparés par une bande noire, leurs bords nets mais givrés (§ 4.2,
    les détails 1 à 4 du § 9)."""
    F = Face()
    out_l = head_outline_l()
    head = to_poly(HWs(out_l))
    face_g = D.lin("face", HW(8.0, -2.6), HW(6.0, 15.0), [(0, "#24262C"), (0.35, BLACK), (1, BLACK_DEEP)])
    F.fill(head, face_g)
    # le modelé du poil noir : le front et le chanfrein qui prennent la
    # lumière, la pommette, le creux sous l'œil, la joue (masséter) dans l'ombre
    F.fill(head, D.ell("front-lum", HW(9.0, -0.6), 7.5, 2.4, -THETA, [(0, "#3B3E46", 0.55), (1, "#3B3E46", 0.0)]))
    F.fill(head, D.ell("joue-lum", HW(10.5, 8.6), 4.6, 2.6, -THETA - 0.2, [(0, "#30333A", 0.5), (1, "#30333A", 0.0)]))
    F.fill(head, D.ell("joue-omb", HW(9.5, 12.8), 6.0, 2.4, -THETA, [(0, "#000000", 0.45), (1, "#000000", 0.0)]))
    # la gorge : sous la mâchoire, le noir se perd dans le gris du cou (§ 4.4)
    throat = head.intersection(to_poly(HWs([(1.0, 9.0), (6.0, 11.4), (9.0, 13.2), (12.5, 14.6), (12.0, 17.0),
                                            (2.0, 17.0), (-1.0, 12.0)])))
    F.fill(throat, D.lin("gorge", HW(8.0, 11.6), HW(6.5, 15.2), [(0, THROAT, 0.0), (0.6, THROAT, 0.55),
                                                                  (1, THROAT_LIGHT, 0.85)]))
    # le museau blanc, son bord arrière un peu irrégulier
    edge = HWs(ragged_line(MUZZLE_EDGE_L, 77, amp=0.24))
    cut = to_poly(edge + [HW(40.0, 16.2), HW(40.0, -3.6)])
    muzzle = head.intersection(cut)
    mg = D.lin("museau", HW(20.0, -0.5), HW(17.0, 13.5), [(0, WHITE_LIGHT), (0.5, WHITE), (1, WHITE_SHADOW)])
    F.fill(muzzle, mg)
    # le museau charnu : le bout du nez qui prend la lumière, le côté du
    # museau qui tourne, l'ombre sous le naseau et vers la bouche
    F.fill(muzzle, D.ell("nez-lum", HW(21.5, 2.8), 3.8, 2.2, -THETA - 0.4, [(0, "#FFFFFF", 0.7), (1, "#FFFFFF", 0.0)]))
    F.fill(muzzle, D.ell("museau-omb", HW(16.0, 8.5), 3.0, 6.5, -THETA, [(0, "#8E8C94", 0.4), (1, "#8E8C94", 0.0)]))
    F.fill(muzzle, D.ell("levre-omb", HW(21.5, 8.6), 3.6, 1.4, -THETA, [(0, "#9C9AA2", 0.5), (1, "#9C9AA2", 0.0)]))
    # l'anneau blanc de l'œil, fermé de noir
    ring_l = [(EYE_L[0] + a, EYE_L[1] + b) for (a, b) in RING_REL]
    ring_pts = HWs(ragged_line(ring_l + [ring_l[0]], 41, amp=0.16, wl=0.9)[:-1])
    ring = to_poly(ring_pts)
    rg = D.lin("anneau", HW(8.0, 1.6), HW(9.4, 7.0), [(0, WHITE_LIGHT), (0.55, WHITE), (1, "#C4C3C9")])
    F.fill(ring, rg)
    # la frange givrée (1 à 2 cm de poils mêlés) le long des deux bords
    wm, bm = RawMarks(), RawMarks()
    whites = muzzle.union(ring)
    from shapely.prepared import prep
    pw = prep(whites)

    def is_white(q):
        return pw.contains(Point(q))

    on_head = prep(head.buffer(-0.15))

    def keep(q):
        return on_head.contains(Point(q))

    # la frange grise suit le bord sans ses petits zigzags
    F.line([q for q in edge if keep(q)][::5], FROST, 0.8, so=0.3)
    F.line((ring_pts + [ring_pts[0]])[::4] + [ring_pts[0]], FROST, 0.55, so=0.3)
    frost([q for q in edge if keep(q)], rng, is_white, bm, wm, keep)
    frost(ring_pts + [ring_pts[0]], rng, is_white, bm, wm, keep, density=0.5, reach=(0.15, 0.4))
    # le poil ras de toute la face, couché vers l'arrière ; plus clair sur le
    # noir du front et du chanfrein, gris pâle sur le blanc
    hm = RawMarks()
    hair = math.atan2(math.sin(THETA), -math.cos(THETA))
    eye_c = HW(*EYE_L)
    nose_l = [(23.25, 4.45), (23.95, 4.3), (24.42, 5.2), (24.58, 6.3), (24.45, 7.1), (24.08, 7.3), (23.6, 6.6),
              (23.2, 5.45)]
    nose = to_poly(HWs(nose_l))
    zone = head.buffer(-0.3).difference(Point(eye_c).buffer(1.7)).difference(nose.buffer(0.2))
    for p in poisson(zone, 0.33, rng):
        L = rng.uniform(0.12, 0.28)
        loc_b = -(p[0] - NUQUE[0]) * math.sin(THETA) - (p[1] - NUQUE[1]) * math.cos(THETA)
        ang = hair + rng.uniform(-0.55, 0.5) + (0.25 if loc_b > 8 else 0.0)
        e = (p[0] + L * math.cos(ang), p[1] + L * math.sin(ang))
        c = ((p[0] + e[0]) / 2 + rng.uniform(-0.06, 0.06), (p[1] + e[1]) / 2 + rng.uniform(-0.06, 0.06))
        if not (keep(p) and keep(e)):
            continue
        if whites.contains(Point(p)):
            if rng.random() < 0.35:
                hm.add(("#FFFFFF", 0.04, 0.55), p, c, e)
            else:
                hm.add(("#B4B3B9", 0.04, 0.4), p, c, e)
        else:
            if rng.random() < 0.3:
                hm.add(("#3B3D45", 0.045, 0.4), p, c, e)
            else:
                hm.add(("#2C2E35", 0.045, 0.45), p, c, e)
    F.marks(hm)
    F.marks(bm)
    F.marks(wm)
    return F, head, nose, nose_l, whites


def nose_and_mouth(nose_l):
    """Le mufle nu, petit, noir grisâtre, en cœur ; le naseau en virgule,
    oblique vers le haut et l'arrière ; la lèvre fendue ; la bouche (§ 5)."""
    g = D.rad("mufle", HW(23.9, 5.7), 1.6 * HK, [(0, "#5A5A62"), (0.45, "#3A3236"), (1, NOSE)])
    D.el(cr(HWs(nose_l), nd=2, t=0.9), fill=g)
    nostril = [(24.2, 6.75), (23.75, 6.35), (23.15, 5.75), (22.6, 5.15), (22.35, 4.85), (22.65, 4.95),
               (23.25, 5.35), (23.85, 5.9), (24.25, 6.35)]
    D.el(cr(HWs(nostril), nd=2, t=0.8), fill=NOSTRIL)
    D.line(HWs([(23.4, 4.35), (23.95, 4.6), (24.35, 5.25)]), NOSE_SHEEN, 0.2, so=0.7)
    D.line(HWs([(24.5, 7.2), (24.35, 7.8), (24.15, 8.25)]), "#55545B", 0.18, so=0.8)
    D.line(HWs(MOUTH), "#3C3B42", 0.28, so=0.9)
    D.line(HWs([(18.6, 9.6), (17.8, 10.0), (17.2, 10.4)]), "#6E6D74", 0.22, so=0.6)


def jaw():
    """La mâchoire : le bas du museau, blanc (le menton est blanc, § 4.2) ;
    dessus, cachés bouche fermée, l'intérieur de la bouche, la langue et la
    joue qui s'étire."""
    D.open("jaw")
    D.el(cr(HWs([(14.6, 7.4), (18.6, 7.2), (23.6, 7.2), (24.0, 8.6), (21.0, 9.6), (17.8, 10.6), (14.8, 11.4)]),
            nd=2, t=0.7), fill="#2E1C20")
    D.el(cr(HWs([(18.4, 9.3), (20.6, 8.75), (22.6, 8.45), (23.6, 8.5), (22.8, 9.05), (20.8, 9.4), (18.8, 9.75)]),
            nd=2), fill="#B06C76")
    D.line(HWs([(19.2, 9.3), (21.2, 8.9), (22.8, 8.75)]), "#D7929B", 0.16, so=0.7)
    # la joue derrière la commissure (blanche : le blanc du museau va jusque-là)
    D.el(cr(HWs([(14.2, 9.4), (18.2, 9.4), (18.0, 12.6), (14.2, 13.2)]), nd=2, t=0.5), fill="#D9D9DD")
    jaw_pts = [(16.4, 9.55), (18.2, 9.6), (19.9, 9.4), (21.5, 9.12), (23.1, 8.75), (24.1, 8.4), (24.35, 9.05),
               (24.1, 10.0), (23.35, 10.95), (21.95, 11.75), (20.0, 12.25), (18.0, 12.5), (16.4, 12.6),
               (15.6, 11.0)]
    # le même blanc que le museau (dégradé en coordonnées absolues) : au
    # repos, la mâchoire et le museau ne font qu'un
    D.el(cr(HWs(jaw_pts), nd=2, t=0.9), fill="url(#museau)")
    D.el(cr(HWs(jaw_pts), nd=2, t=0.9), fill=D.lin("menton-omb", HW(20.0, 10.0), HW(19.5, 12.6),
                                                   [(0, WHITE_SHADOW, 0.0), (1, WHITE_SHADOW, 0.6)]))
    D.line(HWs([(18.4, 9.62), (20.4, 9.4), (22.4, 9.0), (23.9, 8.55)]), "#8C8A92", 0.2, so=0.7)
    D.line(HWs([(17.0, 12.45), (19.0, 12.35), (21.0, 12.0), (22.8, 11.3)]), "#8C8A92", 0.3, so=0.45)
    rng = random.Random(5)
    m = Marks()
    inner = to_poly(HWs(jaw_pts)).buffer(-0.3)
    for _ in range(40):
        a, b = rng.uniform(16.8, 23.6), rng.uniform(9.9, 12.2)
        if not inner.contains(Point(HW(a, b))):
            continue
        p = HW(a, b)
        e = HW(a - rng.uniform(0.15, 0.35), b + rng.uniform(-0.05, 0.1))
        m.add(("#B4B3B9", 0.045, 0.4), p, ((p[0] + e[0]) / 2, (p[1] + e[1]) / 2), e)
    m.emit(D)
    D.pivot("jaw", *HW(*JAW_PIVOT_L))
    D.close()


def eyes():
    """L'œil : iris ambre (jaune ambré au soleil), cercle externe plus foncé,
    pupille en fente horizontale (§ 5) ; la paupière et les cils noirs, un
    trait sombre qui souligne l'œil dans le blanc de l'anneau."""
    E = HW(*EYE_L)
    D.open("eyes")
    almond = [eye_pt(*p) for p in ALMOND]
    D.el(cr(almond, nd=2, t=0.9), fill=D.rad("globe", eye_pt(-0.2, -0.1), 1.6 * HK,
                                             [(0, "#4A3626"), (1, "#1A110C")]))
    D.open("pupils")
    ic = eye_pt(-0.1, 0.0)
    ir = 0.98 * HK
    ig = D.rad("iris", ic, ir, [(0, "#C99A5E"), (0.3, IRIS_LIGHT), (0.62, "#8B6644"), (0.84, IRIS),
                                (0.95, "#4E3523"), (1, LIMBUS)], f=(ic[0] - 0.12, ic[1] + 0.3))
    D.el(cr(ellipse_pts(ic[0], ic[1], ir, ir, 0, 16), nd=2), fill=ig)
    rng = random.Random(3)
    m = Marks()
    for i in range(26):
        a = 2 * math.pi * i / 26 + rng.uniform(-0.06, 0.06)
        r0, r1 = 0.38 * HK, rng.uniform(0.66, 0.86) * HK
        p0 = (ic[0] + r0 * math.cos(a), ic[1] + r0 * math.sin(a))
        p1 = (ic[0] + r1 * math.cos(a), ic[1] + r1 * math.sin(a))
        m.add(("#E0B77C" if i % 3 else "#5E4029", 0.05, 0.6), p0, ((p0[0] + p1[0]) / 2, (p0[1] + p1[1]) / 2), p1)
    m.emit(D)
    pw, ph = 1.2 * HK, 0.36 * HK
    X, Y = S(ic[0] - pw / 2, ic[1] + ph / 2)
    D.emit(f'<rect x="{num(X)}" y="{num(Y)}" width="{num(pw)}" height="{num(ph)}" rx="{num(ph / 2)}" fill="{PUPIL}"/>')
    D.pivot("pupils", *ic)
    D.close()
    # les reflets sur la cornée
    D.el(cr(ellipse_pts(*eye_pt(-0.5, -0.36), 0.26 * HK, 0.17 * HK, 0.3, 10), nd=2), fill=CATCH, fo=0.92)
    D.el(cr(ellipse_pts(*eye_pt(0.62, 0.3), 0.1 * HK, 0.07 * HK, 0.0, 8), nd=2), fill=CATCH, fo=0.5)
    # l'ombre de la paupière supérieure sur le globe
    lid_sh = [eye_pt(-1.45, 0.05), eye_pt(-0.92, -0.66), eye_pt(0.0, -0.9), eye_pt(0.88, -0.7), eye_pt(1.45, 0.06),
              eye_pt(0.8, -0.38), eye_pt(0.0, -0.52), eye_pt(-0.85, -0.32)]
    D.el(cr(lid_sh, nd=2, t=0.9), fill="#000000", fo=0.32)
    # les paupières noires, les cils
    D.line([eye_pt(-1.6, 0.1), eye_pt(-0.92, -0.68), eye_pt(0.0, -0.93), eye_pt(0.9, -0.72), eye_pt(1.52, 0.06)],
           BLACK_DEEP, 0.36)
    D.line([eye_pt(1.52, 0.08), eye_pt(0.9, 0.6), eye_pt(0.0, 0.74), eye_pt(-0.92, 0.56), eye_pt(-1.7, 0.16)],
           BLACK, 0.28)
    m = Marks()
    for i in range(7):
        u = -1.0 + 1.9 * i / 6
        p = eye_pt(u, -0.8 - 0.12 * (1 - u * u))
        e = eye_pt(u + 0.3, -1.15 - 0.12 * (1 - u * u))
        m.add((BLACK_DEEP, 0.07, 0.9), p, ((p[0] + e[0]) / 2, (p[1] + e[1]) / 2 + 0.04), e)
    m.emit(D)
    D.pivot("eyes", *E)
    D.close()
    return almond


# ================================================================== les cornes

# La corne gauche du bélier, de profil (§ 6.2, photo p7) : la base épaisse,
# gris-brun, sur le haut du crâne derrière le front ; un arc large et strié
# vers l'arrière, sur la nuque ; elle descend derrière l'oreille, revient
# vers l'avant sous elle à hauteur de l'angle de la mâchoire, remonte devant
# elle juste derrière l'anneau blanc de l'œil (le cercle est fermé), repasse
# au-dessus, et le dernier quart, plus mince, s'écarte de la tête et double
# le cercle à l'arrière, la pointe émoussée tournée vers le bas. Environ un
# tour et quart (bélier de deux à trois ans).
HORN_L = [(6.0, 0.4), (4.2, 1.0), (2.3, 2.3), (0.9, 4.4), (0.6, 7.2), (1.6, 10.2), (3.6, 12.6), (6.2, 13.9),
          (8.8, 13.5), (10.4, 11.6), (10.3, 9.4), (8.8, 7.9), (6.8, 7.0), (4.6, 6.8), (2.6, 7.4), (0.2, 8.6),
          (-1.2, 10.6), (-1.3, 12.8), (-0.7, 14.6)]
HORN_W = [4.0, 4.0, 3.95, 3.9, 3.85, 3.8, 3.8, 3.75, 3.65, 3.55, 3.45, 3.35, 3.2, 3.05, 2.85, 2.55, 2.2, 1.75,
          0.8]
EAR_BASE_L = (6.0, 10.3)    # l'oreille sort du centre du cercle de la corne
LIGHT = (-0.55, 0.835)      # la lumière vient d'en haut, à gauche


class Horn:
    """Une corne : un tube effilé le long d'une spirale, dessiné par passes
    (chacune recouvre la précédente là où la spirale repasse sur elle-même) ;
    la couleur va du gris-brun de la base au brun caramel, la face tournée
    vers la lumière plus claire, les annelures serrées et profondes, plus
    fines vers la pointe, les derniers centimètres lisses (§ 6.3)."""

    def __init__(self, pts_l, widths, seed, offset=(0.0, 0.0), dim=1.0, gid="corne"):
        P = [(x + offset[0], h + offset[1]) for (x, h) in HWs(pts_l)]
        C = dense(P, False, 10)
        n = len(C)
        W = []
        for i in range(n):
            u = i / (n - 1) * (len(widths) - 1)
            k = min(int(u), len(widths) - 2)
            f = u - k
            W.append((widths[k] * (1 - f) + widths[k + 1] * f) * HK)
        N = normals(C, closed=False)
        self.C, self.W, self.N = C, W, N
        self.left = [(C[i][0] + N[i][0] * W[i] / 2, C[i][1] + N[i][1] * W[i] / 2) for i in range(n)]
        self.right = [(C[i][0] - N[i][0] * W[i] / 2, C[i][1] - N[i][1] * W[i] / 2) for i in range(n)]
        acc = [0.0]
        for i in range(1, n):
            acc.append(acc[-1] + math.dist(C[i - 1], C[i]))
        self.acc = acc
        self.total = acc[-1]
        self.dim = dim
        self.gid = gid
        self.seed = seed
        self._rings()

    def index(self, s):
        for i, a in enumerate(self.acc):
            if a >= s:
                return i
        return len(self.acc) - 1

    def color(self, f):
        stops = [(0.0, HORN_BASE), (0.2, "#6F5B4D"), (0.36, "#8C705C"), (0.5, HORN), (0.8, "#A3826A"),
                 (1.0, "#AE9076")]
        return shade(ramp(f, stops), self.dim)

    def _rings(self):
        """Les annelures : des sillons en travers, serrés près de la base, un
        peu plus espacés vers la pointe ; tous les cinq ou six, un anneau de
        croissance plus profond. Les quatre derniers centimètres, lisses."""
        rng = random.Random(self.seed)
        self.rings = []
        s = 0.5
        k = 0
        while s < self.total - 4.0:
            self.rings.append((s, k % 6 == 5, rng.uniform(-0.25, 0.25), rng.uniform(0.6, 1.0)))
            s += (0.42 + 0.5 * (s / self.total) ** 1.2) * rng.uniform(0.8, 1.2)
            k += 1

    def draw(self, s0, s1, details=True):
        """La passe de la corne entre les abscisses s0 et s1."""
        i0, i1 = self.index(max(0.0, s0)), self.index(min(self.total, s1))
        L, R, C, W, N = self.left, self.right, self.C, self.W, self.N
        # le tube, en tranches : chacune un dégradé en travers, du bord tourné
        # vers la lumière (clair) au bord dans l'ombre ; la couleur va du
        # gris-brun de la base au brun caramel
        step = 4
        for a in range(i0, i1, step):
            b = min(i1, a + step + 1)
            m = (a + b) // 2
            f = round(self.acc[m] / self.total * 24) / 24
            c0 = self.color(f)
            lit = N[m][0] * LIGHT[0] + N[m][1] * LIGHT[1]   # > 0 : le bord gauche est éclairé
            k = min(1.0, abs(lit) + 0.25) if details else 0.4
            lit_c = mix(c0, shade(HORN_LIGHT, self.dim), 0.5 * k)
            dark_c = shade(c0, 0.62)
            stops = [(0, mix(c0, lit_c, 0.6)), (0.16, lit_c), (0.42, c0), (0.78, shade(c0, 0.8)), (1, dark_c)]
            p1, p2 = (L[m], R[m]) if lit > 0 else (R[m], L[m])
            self.n_grad = getattr(self, "n_grad", 0) + 1
            g = D.lin(f"{self.gid}-{self.n_grad}", p1, p2, stops)
            D.el(poly(L[a:b + 1] + R[a:b + 1][::-1], nd=2), fill=g)
        if details:
            # les annelures
            m = Marks()
            for (s, deep, wob, contrast) in self.rings:
                if not (s0 - 0.05 <= s <= s1 + 0.05):
                    continue
                i = max(1, min(len(C) - 2, self.index(s)))
                tx, ty = C[i + 1][0] - C[i - 1][0], C[i + 1][1] - C[i - 1][1]
                tl = math.hypot(tx, ty) or 1.0
                tx, ty = tx / tl, ty / tl
                w = W[i] / 2 * 0.96
                fade = 1.0 if s < self.total - 7.0 else max(0.25, (self.total - 4.0 - s) / 3.0)
                bow = (0.22 + 0.1 * w) * (1 + wob)
                pa = (C[i][0] + N[i][0] * w, C[i][1] + N[i][1] * w)
                pb = (C[i][0] - N[i][0] * w, C[i][1] - N[i][1] * w)
                ctl = (C[i][0] + tx * bow * 2, C[i][1] + ty * bow * 2)
                op = round(min(0.9, (0.75 if deep else 0.5) * fade * contrast) * 10) / 10
                m.add((shade(HORN_GROOVE, self.dim), 0.22 if deep else 0.13, op), pa, ctl, pb)
                sh = 0.22
                pa2, pb2 = (pa[0] + tx * sh, pa[1] + ty * sh), (pb[0] + tx * sh, pb[1] + ty * sh)
                ctl2 = (ctl[0] + tx * sh, ctl[1] + ty * sh)
                m.add((shade(HORN_LIGHT, self.dim), 0.08, round(0.35 * fade * contrast * 10) / 10), pa2, ctl2, pb2)
            m.emit(D)
            # l'arête extérieure qui prend la lumière, et la ligne sombre qui suit
            # la spirale sur la face externe (photo p8)
            ka, kb = max(i0, 2), min(i1, len(C) - 6)
            if kb > ka + 2:
                keel = [(C[i][0] - N[i][0] * W[i] * 0.36, C[i][1] - N[i][1] * W[i] * 0.36) for i in range(ka, kb)]
                D.line(keel[::2], shade("#E2D0B6", self.dim), 0.14, so=0.3)
                mid = [(C[i][0] - N[i][0] * W[i] * 0.08, C[i][1] - N[i][1] * W[i] * 0.08) for i in range(ka, kb)]
                D.line(mid[::2], shade(HORN_GROOVE, self.dim), 0.28, so=0.35)
            if i1 >= len(C) - 1:
                tip = [(C[i][0] - N[i][0] * W[i] * 0.15, C[i][1] - N[i][1] * W[i] * 0.15)
                       for i in range(len(C) - 18, len(C) - 1)]
                D.line(tip[::2], shade("#CDB79A", self.dim), 0.4, so=0.5)
        # le contour, le long des bords seulement
        edge = shade("#3A2C24", self.dim)
        D.line(L[i0:i1 + 1][::2] + ([L[i1]] if (i1 - i0) % 2 else []), edge, 0.24, so=0.75)
        D.line(R[i0:i1 + 1][::2] + ([R[i1]] if (i1 - i0) % 2 else []), edge, 0.24, so=0.75)
        if i1 >= len(C) - 1:
            D.line([L[-3], C[-1], R[-3]], edge, 0.24, so=0.75)

    def s_of(self, k):
        """L'abscisse du k-ième point de contrôle."""
        n = len(self.C)
        i = round(k / (len(HORN_L) - 1) * (n - 1))
        return self.acc[i]


def horn_shadow(horn, s0, s1, dx=0.7, dy=-0.9, op=0.28):
    """L'ombre portée de la fin de la corne, qui s'écarte de la tête, sur ce
    qui est dessous."""
    i0, i1 = horn.index(s0), horn.index(s1)
    pts = [(x + dx, h + dy) for (x, h) in horn.left[i0:i1 + 1] + horn.right[i0:i1 + 1][::-1]]
    D.el(poly(pts), fill="#000000", fo=op)


def horn_root_hair(F, rng):
    """Le poil noir et ras du front recouvre la racine de la corne : une
    collerette de poils au bord effilé, peinte comme la face elle-même (la
    face, rognée à la collerette, redessinée par-dessus la corne)."""
    (a0, b0), (a1, b1) = HORN_L[0], HORN_L[1]
    ln = math.hypot(a1 - a0, b1 - b0)
    ta, tb = (a1 - a0) / ln, (b1 - b0) / ln
    na, nb = -tb, ta
    half = HORN_W[0] / 2 + 0.3
    edge = []
    for i in range(31):
        u = -half + 2 * half * i / 30
        back = 0.3 + 0.07 * math.sin(i * 2.3) + rng.uniform(-0.04, 0.04)
        edge.append((a0 + na * u + ta * back, b0 + nb * u + tb * back))
    front = [(a0 - na * half - ta * 0.8, b0 - nb * half - tb * 0.8), (a0 + na * half - ta * 0.8, b0 + nb * half - tb * 0.8)]
    cap = to_poly(HWs(edge + front))
    F.emit(clip=cap)
    m = Marks()
    for (a, b) in resample(edge, 0.1):
        L = rng.uniform(0.12, 0.24)
        p = HW(a - ta * 0.15, b - tb * 0.15)
        e = HW(a + ta * L / HK, b + tb * L / HK)
        m.add((BLACK if rng.random() < 0.7 else "#33363D", 0.045, 0.75), p, ((p[0] + e[0]) / 2, (p[1] + e[1]) / 2), e)
    m.emit(D)


# ================================================================== les oreilles

def ear(name, base, ang_deg, length, width, far=False):
    """Une oreille grise de taille moyenne (§ 4.3, § 5), en feuille assez
    étroite au bout arrondi, portée à l'horizontale ou un peu tombante : poil
    ras blanc-gris finement moucheté de noir, un liseré noir au bout,
    l'intérieur rosé-gris peu visible, le long du bord du bas."""
    rng = random.Random(len(name) * 11)
    D.open(name)
    a = math.radians(ang_deg)
    d = (math.cos(a), math.sin(a))
    nrm = (-d[1], d[0])  # vers le bas de l'oreille (elle pointe vers l'arrière)

    def T(u, v):
        return (base[0] + d[0] * u + nrm[0] * v, base[1] + d[1] * u + nrm[1] * v)

    L, w = length, width
    outline = [T(-0.8, -0.3 * w), T(0.2 * L, -0.44 * w), T(0.5 * L, -0.47 * w), T(0.78 * L, -0.34 * w),
               T(0.97 * L, -0.08 * w), T(0.96 * L, 0.16 * w), T(0.78 * L, 0.38 * w), T(0.48 * L, 0.48 * w),
               T(0.2 * L, 0.42 * w), T(-0.8, 0.3 * w)]
    k = 0.66 if far else 1.0
    g = D.lin(f"{name}-g", T(0.4 * L, -0.5 * w), T(0.4 * L, 0.5 * w),
              [(0, shade(EAR_LIGHT, k)), (0.45, shade(EAR, k)), (1, shade(EAR_SHADOW, k))])
    D.el(cr(outline, nd=2), fill=g)
    if not far:
        # l'intérieur, rosé-gris, le long du bord inférieur (la conque s'ouvre
        # vers l'avant et le dehors), plus sombre vers la base
        inner = [T(0.1 * L, 0.2 * w), T(0.42 * L, 0.14 * w), T(0.72 * L, 0.16 * w), T(0.88 * L, 0.24 * w),
                 T(0.66 * L, 0.38 * w), T(0.36 * L, 0.42 * w)]
        gi = D.lin(f"{name}-in", T(0.1 * L, 0.3 * w), T(0.8 * L, 0.3 * w), [(0, "#7E6A6B"), (1, EAR_IN)])
        D.el(cr(inner, nd=2), fill=gi)
    # le poil ras, couché vers le bout ; les mouchetures fines
    m = Marks()
    for i in range(70 if not far else 25):
        u = rng.uniform(0.05, 0.9)
        v = rng.uniform(-0.42, 0.1 if not far else 0.4) * (1.0 - 0.45 * u)
        p0 = T(u * L, v * w)
        e = T((u + rng.uniform(0.05, 0.1)) * L, v * w + rng.uniform(-0.05, 0.05))
        col = rng.choice(("#F4F5F8", "#9EA1A8", "#E2E4E8")) if not far else "#6E7076"
        m.add((col, 0.05, 0.6), p0, ((p0[0] + e[0]) / 2, (p0[1] + e[1]) / 2), e)
    m.emit(D)
    specks = []
    for i in range(38 if not far else 12):
        u = rng.uniform(0.06, 0.95)
        v = rng.uniform(-0.44, 0.15) * (1.0 - 0.5 * u)
        r = rng.uniform(0.05, 0.14) * (1.3 if rng.random() < 0.15 else 1.0)
        specks.append(cr(blob(T(u * L, v * w), r, r * rng.uniform(0.6, 1.0), a, 300 + i, n=7), nd=2))
    D.el("".join(specks), fill=shade(EAR_SPECK, k), fo=0.8)
    D.line([T(0.84 * L, -0.27 * w), T(0.95 * L, -0.1 * w), T(0.97 * L, 0.04 * w), T(0.92 * L, 0.18 * w)],
           shade(BLACK, k), 0.22, so=0.8)
    if not far:
        D.line([T(0.15 * L, -0.4 * w), T(0.5 * L, -0.43 * w), T(0.78 * L, -0.3 * w)], "#F6F7FA", 0.14, so=0.6)
        D.line([T(0.3 * L, 0.45 * w), T(0.62 * L, 0.42 * w), T(0.86 * L, 0.26 * w)], "#5E6064", 0.16, so=0.6)
    D.pivot(name, *base)
    D.close()


# ================================================================== la tête, assemblée

def closed_lid():
    """Ce qu'on voit quand l'œil se ferme (sous le groupe des yeux) : la
    fente noire des paupières, les cils."""
    D.line([eye_pt(-1.4, 0.05), eye_pt(-0.6, 0.12), eye_pt(0.4, 0.1), eye_pt(1.35, 0.04)], BLACK_DEEP, 0.4)
    D.line([eye_pt(-1.2, 0.3), eye_pt(0.0, 0.42), eye_pt(1.1, 0.3)], "#4B4D55", 0.18, so=0.6)


def far_horn():
    """La corne de l'autre côté, presque cachée par la tête : le haut de son
    arc dépasse un peu du nôtre, et sa pointe écartée reparaît devant le
    chanfrein (p1, p14, fiche § 6.2), un petit croissant plus sombre."""
    h = Horn(HORN_L[:9], HORN_W[:9], 23, offset=(0.9, 1.1), dim=0.62, gid="corne-loin")
    h.draw(0.0, h.total, details=True)
    tip = Horn([(18.8, 1.5), (18.8, -1.0), (19.5, -3.2), (20.7, -3.9), (21.9, -2.5), (22.5, -0.1)],
               [2.2, 1.8, 1.4, 1.0, 0.6, 0.22], 24, dim=0.7, gid="corne-loin-pointe")
    tip.draw(0.0, tip.total, details=True)


def head():
    rng = random.Random(1838)  # Tan Hill, l'auberge la plus haute de l'Angleterre (S2)
    D.comment("Tête : face noire au museau blanc, anneaux blancs autour des yeux, cornes rondes et basses")
    D.open("head")
    ear("ear-far", HW(5.0, 8.6), 162.0, 6.6 * HK, 3.0 * HK, far=True)
    if not VARIANTE:
        far_horn()
    jaw()
    F, head_poly, nose, nose_l, whites = face_items(rng)
    F.emit()
    nose_and_mouth(nose_l)
    closed_lid()
    almond = eyes()
    # le pourtour de l'œil, immobile, redessiné par-dessus : il cache l'iris
    # quand il glisse
    E = HW(*EYE_L)
    opening = to_poly(dense(almond, True, 6))
    F.emit(clip=Point(E).buffer(4.9).difference(opening))
    if VARIANTE == "brebis":
        # La corne mince de la brebis : un peu plus d'un demi-tour (§ 1.4, 6.4).
        fine = 12
        horn = Horn(HORN_L[:fine], [w * .5 * (1 - .8 * i / (fine - 1)) + .12
                                    for i, w in enumerate(HORN_W[:fine])], 31)
        horn.draw(0.0, horn.total)
        horn_root_hair(F, rng)
        ear("ear-near", HW(*EAR_BASE_L), 186.0, 6.0 * HK, 3.3 * HK)
    elif VARIANTE == "agneau":
        ear("ear-near", HW(*EAR_BASE_L), 186.0, 6.0 * HK, 3.3 * HK)
    else:
        horn = Horn(HORN_L, HORN_W, 31)
        s4, s9, s14 = horn.s_of(4), horn.s_of(9), horn.s_of(14.6)
        horn.draw(0.0, s4 + 0.6)
        horn_root_hair(F, rng)
        horn.draw(s4 - 0.6, s9 + 0.6)
        horn.draw(s9 - 0.6, s14 + 0.6)
        ear("ear-near", HW(*EAR_BASE_L), 186.0, 6.0 * HK, 3.3 * HK)
        horn_shadow(horn, s14, horn.total)
        horn.draw(s14 - 0.6, horn.total)
    D.pivot("head", *NUQUE)
    D.pivot("muzzle", *HW(*MUZZLE_L))
    D.close()


# ================================================================== le recoupage et l'écriture

def fix_fore_pivots(root):
    """Le pivot de chaque antérieur, placé pour que la patte, au repos, soit
    droite ou à peine pliée dans le bon sens (le canon un rien en arrière) :
    le moteur plie alors le carpe vers l'avant, comme il faut."""
    for name in ("leg-front-far", "leg-front-near"):
        _, _, leg = A._find(root, name)
        _, _, lower = A._find(root, name + "-lower")
        xs, ys = [], []
        for el, inh in A._drawn(lower, A._inherit(lower)):
            sw = float(el.get("stroke-width", 0)) if A._has_stroke(el, inh) else 0.0
            for pts, _ in A.subpaths(el):
                xs += [p[0] - sw / 2 for p in pts] + [p[0] + sw / 2 for p in pts]
                ys += [p[1] + sw / 2 for p in pts]
        tip = ((min(xs) + max(xs)) / 2, max(ys))
        jp = [c for c in lower if c.get("id") == f"pivot-{name}-lower"][0]
        joint = (float(jp.get("cx")), float(jp.get("cy")))
        rp = [c for c in leg if c.get("id") == f"pivot-{name}"][0]
        ry = float(rp.get("cy"))
        phi = math.atan2(tip[1] - joint[1], tip[0] - joint[0]) - 0.03
        t = (joint[1] - ry) / math.sin(phi)
        rp.set("cx", A._f(joint[0] - t * math.cos(phi)))


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
    lip = [S(*HW(*p)) for p in LIP_L]
    gum = A.gum([S(*HW(*p)) for p in GUM_L], [S(*HW(*p)) for p in GUM_RIDGE_L], GUM, GUM_DARK, 0.3)
    A.lift_part(root, paints, "head", lip, "lip", S(*HW(*LIP_PIVOT_L)), under=gum)
    fix_fore_pivots(root)
    ET.indent(root, space=" ")
    return ET.tostring(root, encoding="unicode") + "\n"


def main(path):
    D.comment("Pattes : celles du fond d'abord, toutes sous la toison")
    leg_group("leg-back-far", "hind", HIND_FAR, True)
    leg_group("leg-front-far", "fore", FORE_FAR, True)
    tail()
    leg_group("leg-back-near", "hind", HIND_NEAR, False)
    leg_group("leg-front-near", "fore", FORE_NEAR, False)
    neck()
    body()
    head()
    D.pivot("ground", (HIND_NEAR + FORE_NEAR) / 2 + 1.5, 0.0)
    W, H = 124, 100
    svg = (f'<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 {W} {H}" width="{W}" height="{H}">\n'
           "<!--\n"
           "  Swaledale : un bélier (« tup ») de deux à trois ans, en octobre, la toison\n"
           "  repoussée de trois à quatre mois, de profil, tourné vers la droite.\n"
           f"  1 unité = 1 cm de l'animal ({GARROT:.0f} cm au garrot). Face noire au museau blanc,\n"
           "  un anneau blanc autour de l'œil, cornes rondes et basses autour de l'oreille,\n"
           "  pattes blanches marbrées de noir, queue longue et laineuse, d'après\n"
           "  docs/races/swaledale.md ; écrit par tools/races/swaledale/gen_swaledale.py.\n"
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
    if VARIANTE == "agneau":
        # Né la face presque entièrement noire : le blanc du museau vient dans la première année.
        variantes.recolorer(root, variantes.rampe("#0D0C0D", "#4A4444", bas=.1, haut=.95), dans=("head",),
                    sauf=("eyes",), si=lambda c: variantes.clarte(c) > .5)
    variantes.ecrire(path, root, **PROPORTIONS[VARIANTE])


if __name__ == "__main__":
    main(sys.argv[1] if len(sys.argv) > 1 else "swaledale.svg")
