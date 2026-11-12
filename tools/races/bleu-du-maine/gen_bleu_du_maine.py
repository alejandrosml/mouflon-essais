#!/usr/bin/env python3
"""Dessine la Bleu du Maine : brebis de trois à quatre ans en toison
d'automne, tête haute, calme, de profil vers la droite.

Les mesures et les couleurs viennent de docs/races/bleu-du-maine.md ; les
photos P11/P21 donnent la toison au pré, P24/P25 les aplombs et P14/P27 la
tête. Aucune photographie n'est incorporée au dessin. Une unité est un
centimètre. Le repère du dessin est x vers la droite, h au-dessus du sol ;
outils.S le transforme en SVG (y = 102 - h).

La toison n'est pas une suite d'anglaises : ses petits paquets serrés se
recouvrent en pavés irréguliers, traversés de courtes ondulations. Le bord
du dos reste tassé ; seule une frange modeste pend sous le ventre.

Usage : python3 tools/races/bleu-du-maine/gen_bleu_du_maine.py
        [pets/bleu-du-maine/bleu-du-maine.svg]

© 2026 Alejandro San Martin Lamas — MIT OR Apache-2.0
"""
import math
from pathlib import Path
import random
import sys
import xml.etree.ElementTree as ET

HERE = Path(__file__).resolve().parent
sys.path.insert(0, str(HERE))
sys.path.insert(0, str(HERE.parent))
import articulations as A  # noqa: E402
import variantes  # noqa: E402

# Les autres dessins de la race (tools/races/variantes.py), d'après la fiche.
# Sans variante : la brebis.
VARIANTE = sys.argv[2] if len(sys.argv) > 2 else ""
PROPORTIONS = {
    "belier": dict(  # Bélier Bleu du Maine : 74 cm au garrot, sans cornes, tête plus large, encolure plus épaisse.
        garrot=(68, 74), encolure=(1.0, 1.16), tete=(1.05, 1.08)),
    "agneau": dict(  # Agneau Bleu du Maine de deux semaines : 41 cm au garrot, petite tête ardoise, toison courte.
        variantes.AGNEAU, garrot=(68, 41), tete=(1.0, 1.1)),
}
from outils import (Doc, Point, Polygon, S, clamp, cr, dense, geom_d, mix,  # noqa: E402
                    num, poisson, runs_path, shade)

# ------------------------------------------------------------------ mesures
GARROT = 68.0                 # fiche § 2.1, brebis mesurées en Flandre
CARPE, JARRET = 16.5, 18.5     # § 2.5, nus et distincts de la laine
POLL = (110.0, 88.0)          # nuque à 1,29 H ; cou à environ 50°
NECK_BASE = (99.0, 52.0)      # assez bas pour que le museau atteigne l'herbe
HEAD_ANGLE = math.radians(43.0)
HU = (math.cos(HEAD_ANGLE), -math.sin(HEAD_ANGLE))
HV = (-math.sin(HEAD_ANGLE), -math.cos(HEAD_ANGLE))

# La silhouette laineuse mesure 95 cm du poitrail à la culotte (1,40 H).
# La frange du ventre reste à 23 cm du sol, soit 34 % de H (§ 2.3).
BODY = [(16.0, 55.0), (16.5, 62.0), (19.0, 68.0), (24.0, 72.0),
        (31.0, 74.2), (40.0, 74.7), (50.0, 74.4), (60.0, 73.9),
        (70.0, 73.8), (79.0, 74.0), (84.0, 73.8), (88.0, 73.0),
        (92.0, 70.5), (95.5, 66.0), (99.0, 61.5), (103.0, 56.8), (107.0, 50.0),
        (111.0, 42.0), (110.4, 35.8), (107.5, 31.0), (102.0, 28.2),
        (94.0, 26.2), (91.5, 25.6), (90.0, 24.9), (88.6, 25.2),
        (87.2, 25.8), (85.0, 26.1), (83.8, 25.3), (82.0, 24.7),
        (80.3, 25.2), (79.0, 26.1), (76.4, 25.5), (75.0, 24.7),
        (73.7, 25.3), (72.2, 25.7), (70.6, 24.5), (69.0, 25.2),
        (67.6, 23.8), (65.2, 22.7), (62.9, 23.3), (61.0, 25.2),
        (58.8, 24.7), (57.1, 23.8), (55.6, 24.7), (53.2, 25.0),
        (51.5, 25.4), (50.1, 24.6), (48.8, 24.0), (47.3, 24.7),
        (46.2, 25.4), (44.6, 25.5), (43.6, 24.9), (42.0, 25.7),
        (39.0, 26.1), (37.0, 25.7), (36.0, 27.2), (31.0, 29.0),
        (26.0, 30.0), (22.0, 33.0), (19.0, 39.0), (16.7, 47.0)]
# La pièce de l'encolure dépasse sous le corps. Son bord caché, rond,
# évite une pointe au garrot quand la brebis baisse la tête.
NECK = [(87.0, 50.0), (85.0, 60.0), (87.0, 70.0), (91.0, 77.0),
        (96.0, 82.0), (101.0, 86.0), (106.0, 88.6), (109.8, 88.2),
        (113.0, 84.0), (115.0, 79.0), (118.0, 72.0), (119.0, 65.0),
        (118.0, 58.0), (115.5, 51.5), (113.0, 45.8), (112.0, 42.0),
        (110.8, 38.8), (109.0, 36.0), (104.0, 35.2),
        (98.0, 37.2), (92.0, 40.0), (89.0, 44.0)]
BODY_POLY = Polygon(dense(BODY, True, 8)).buffer(0)
NECK_POLY = Polygon(dense(NECK, True, 8)).buffer(0)

# ------------------------------------------------------------------ palette
WOOL_LT, WOOL, WOOL_SH, WOOL_DEEP = "#E6DCC6", "#CDBFA3", "#9C8E74", "#6E6250"
FACE_LT, FACE, FACE_SH = "#7C8492", "#50565E", "#34383E"
LEG_LT, LEG, LEG_SH = "#5E6770", "#343B3E", "#1E2321"
HOOF_LT, HOOF, HOOF_SH = "#4A4E50", "#232628", "#121414"
IRIS_LT, IRIS, IRIS_SH = "#B08A55", "#8A6236", "#4C2D1B"

D = Doc()


def HL(u, v):
    """Du repère de la tête au monde (v vers la ganache)."""
    return (POLL[0] + u * HU[0] + v * HV[0],
            POLL[1] + u * HU[1] + v * HV[1])


def HLS(points):
    return [HL(u, v) for u, v in points]


def ellipse(c, rx, ry, angle=0.0, n=20):
    ca, sa = math.cos(angle), math.sin(angle)
    return [(c[0] + rx * math.cos(a) * ca - ry * math.sin(a) * sa,
             c[1] + rx * math.cos(a) * sa + ry * math.sin(a) * ca)
            for a in [2 * math.pi * k / n for k in range(n)]]


def line(points, color, width=0.1, opacity=1.0):
    D.stroke(cr(points, False), color, width, opacity)


def fill(points, color, opacity=1.0):
    D.path(cr(points), fill=color, fo=opacity)


def wool_color(light):
    light = clamp(light)
    return mix(WOOL_SH, WOOL, light * 2) if light < 0.5 else mix(WOOL, WOOL_LT, 2 * light - 1)


def fleece_light(x, h):
    """Modelé commun au corps et au cou : haut clair, bas et gorge ombrés."""
    t = clamp((h - 23.0) / 52.0)
    packs = 0.06 * math.sin(x / 4.8 + h / 3.7) * math.cos(h / 4.6 - x / 8.1)
    return clamp(0.10 + 0.85 * t ** 0.72 + packs - 0.08 * clamp((x - 100) / 16))


def wool_surface(region, seed, dim=1.0, density=1.0):
    """Petits paquets irréguliers et serrés. Les formes sont intersectées
    géométriquement au contour avant écriture : aucun masque SVG.

    Les points sont en quinconce avec un fort décalage aléatoire. Les
    pavés voisins se recouvrent, leur pointe arrondie porte deux à quatre
    ondulations courtes. Les quelques sillons verticaux suivent la mèche,
    sans quadrillage régulier ni anneaux en tire-bouchons (§ 3.2).
    """
    rng = random.Random(seed)
    D.path(geom_d(region), fill="url(#fond-laine)")
    inside_arc, inside_edge = region.buffer(-.12), region.buffer(-.15)
    x0, h0, x1, h1 = region.bounds
    roots = []
    dy, dx = 1.9 * density, 1.9 * density
    h = h0 - 1.0
    row = 0
    while h < h1 + 5:
        x = x0 - 2.0 + (row % 2) * dx / 2
        while x < x1 + 3:
            px, ph = x + rng.uniform(-0.32, 0.32) * dx, h + rng.uniform(-0.32, 0.32) * dy
            roots.append((ph, px))
            x += dx
        h += dy
        row += 1
    roots.sort()
    for ph, px in roots:
        length, width = rng.uniform(2.9, 4.8) * density, rng.uniform(2.9, 3.8) * density
        tilt = 0.25 * math.sin(px / 6.0 + ph / 7.2) + rng.uniform(-0.40, 0.40)

        def P(u, v):
            return (px + u + v * tilt, ph - v)

        contour = [P(-width * .35, -.25), P(-width * .04, -.65), P(width * .32, -.35),
                   P(width * .54, length * .12), P(width * .48, length * .30),
                   P(width * .56, length * .49), P(width * .39, length * .77),
                   P(width * .13, length * .93), P(-width * .08, length * .86),
                   P(-width * .30, length * .95), P(-width * .52, length * .70),
                   P(-width * .45, length * .50), P(-width * .54, length * .28)]
        part = Polygon(dense(contour, True, 4)).buffer(0).intersection(region)
        if part.is_empty or part.area < 0.4:
            continue
        light = fleece_light(px, ph - length * .45) + rng.uniform(-.12, .12)
        color = shade(wool_color(light), dim)
        D.path(geom_d(part, nd=2, simplify=.03), fill=color)
        # Le creux à droite d'un paquet et les petites ondulations qui
        # attrapent la lumière, avec la même couleur de fond d'un bout à l'autre.
        for j in range(rng.randint(1, 2)):
            v = length * rng.uniform(.30, .72)
            skew = rng.uniform(-.35, .35)
            arc = [P(-width * rng.uniform(.18, .35), v - skew),
                   P(-width * .04, v + rng.uniform(.05, .22)),
                   P(width * rng.uniform(.14, .30), v + skew)]
            if all(inside_arc.contains(Point(p)) for p in arc):
                line(arc, shade(color, .82), .09 * density, .34)
                arc2 = [(p[0] - .04, p[1] + .18) for p in arc]
                line(arc2, mix(color, "#FFF9EB", .49), .10 * density, .47)
        edge = [P(width * .43, length * .24), P(width * .45, length * .55), P(width * .29, length * .78)]
        if rng.random() < .45 and all(inside_edge.contains(Point(p)) for p in edge):
            line(edge, shade(color, .82), .10 * density, .26)
    # Des fibres fines par-dessus les paquets : elles cassent la régularité
    # des formes, et les pointes ne deviennent jamais des anneaux fermés.
    D.begin_batch()
    for p in poisson(region.buffer(-.35), .83 * density, rng, limit=2600):
        length = rng.uniform(.25, .65) * density
        end = (p[0] + rng.uniform(-.18, .18), p[1] - length)
        if region.contains(Point(end)):
            line([p, ((p[0] + end[0]) / 2 - .1, p[1] - length * .4), end],
                 mix(wool_color(round(fleece_light(*p) * 16) / 16), "#FFF9E8", .65), .045, .42)
    D.flush()
    # Quelques fibres courtes sur le bord tassé : elles rompent la coupe
    # géométrique du contour, sans transformer le dos en frange.
    if region.geom_type == "Polygon":
        ring = region.exterior
        steps = max(1, int(ring.length / .55))
        D.begin_batch()
        for k in range(steps):
            if rng.random() < .38:
                continue
            at = ring.length * k / steps
            p = ring.interpolate(at)
            a, b = ring.interpolate(max(0, at - .08)), ring.interpolate(min(ring.length, at + .08))
            tx, th = b.x - a.x, b.y - a.y
            ln = math.hypot(tx, th) or 1
            nx, nh = (th / ln, -tx / ln) if ring.is_ccw else (-th / ln, tx / ln)
            length = rng.uniform(.06, .22)
            q = (p.x + nx * length, p.y + nh * length)
            line([(p.x - nx * .1, p.y - nh * .1), q],
                 shade(wool_color(round(fleece_light(p.x, p.y) * 16) / 16), dim), .055, .70)
        D.flush()


def soft_shape(name, outline, center, radius, color, alpha=0.5, sx=1.0):
    """Un modelé doux sans filtre : dégradé transparent sur un tracé."""
    grad = D.rad(name, center, radius, [(0, color, alpha), (.55, color, alpha * .6), (1, color, 0)], sx=sx)
    D.path(cr(outline), fill=grad)


# ------------------------------------------------------------------ pattes
LEGS = [("leg-back-far", 37.0, False, False),
        ("leg-front-far", 94.0, True, False),
        ("leg-back-near", 29.0, False, True),
        ("leg-front-near", 99.0, True, True)]


def hoof(name, x, far):
    """Les deux doigts serrés : un coin mat, une petite fente oblique,
    la couronne et deux stries de croissance très discrètes."""
    dim = .77 if far else 1.0
    g = D.lin("onglon-" + name, (x - 1.2, 3.0), (x + 3.8, .3),
              [(0, shade(HOOF_SH, dim)), (.40, shade(HOOF, dim)),
               (.75, shade(HOOF_LT, dim)), (1, shade(HOOF, dim))])
    fill([(x + 1.3, 2.45), (x + 2.4, 2.3), (x + 3.5, .07),
          (x + .8, .07), (x, 1.0)], shade(HOOF_SH, dim))
    D.path(runs_path([[(x - 1.1, 2.8), (x + .3, 3.0), (x + 1.5, 2.5), (x + 2.5, .9), (x + 3.0, .06)],
                      [(x + 3.0, .06), (x - 1.3, .06)],
                      [(x - 1.3, .06), (x - 1.5, .6), (x - 1.4, 1.8), (x - 1.1, 2.8)]]), fill=g)
    line([(x + 1.3, 2.3), (x + 2.3, 1), (x + 2.65, .3)], HOOF_SH, .12, .80)
    line([(x - .9, 2.6), (x + .3, 2.7), (x + 1.2, 2.3)], shade(HOOF_LT, dim), .12, .45)
    line([(x - .8, 1.65), (x + .3, 1.75), (x + 1.65, 1.4)], shade(HOOF_LT, dim), .06, .30)


def leg(name, x, front, near):
    rng = random.Random(1936 + int(x * 11))
    dim = 1.0 if near else .74
    D.open(name)
    if front:
        # Canon : 2,8 cm environ (4 % H). Carpe un peu plus large, à 16,5.
        right = [(x + 3.7, 39), (x + 2.7, 31), (x + 1.7, 23),
                 (x + 2.0, 18.7), (x + 2.1, 16.5), (x + 1.6, 14.2),
                 (x + 1.3, 8.4), (x + 1.7, 5), (x + 2.2, 2.9)]
        left = [(x - 1.0, 2.9), (x - 1.7, 5.5), (x - 1.5, 8.7),
                (x - 1.4, 14), (x - 2.0, 16.4), (x - 1.9, 18.7),
                (x - 2.0, 24), (x - 2.7, 31), (x - 3.7, 39)]
        sleeve = [(x - 5, 42), (x + 5, 42), (x + 3.5, 32),
                  (x + 2.9, 30.0), (x + .7, 29.2), (x - 1.7, 29.8),
                  (x - 3.1, 31.2), (x - 4.0, 34)]
    else:
        # Jambe inclinée vers l'arrière ; jarret à 18,5 cm, canon sous lui.
        right = [(x + 10, 42), (x + 7, 33), (x + 3.5, 25),
                 (x + 1.6, 20), (x + 1.4, 16), (x + 1.5, 9),
                 (x + 1.8, 5), (x + 2.5, 2.9)]
        left = [(x - .8, 2.9), (x - 1.5, 5.3), (x - 1.4, 9),
                (x - 1.4, 15.0), (x - 2.5, 18.5), (x - 2.0, 21),
                (x + .2, 27), (x + 2.2, 34), (x + 3.0, 42)]
        sleeve = [(x - 5, 44), (x + 13, 44), (x + 10, 34),
                  (x + 5.5, 28), (x + 3.3, 26.7), (x + 1.2, 26.2),
                  (x - .6, 27.2), (x - 2.0, 31), (x - 4.4, 36)]
    outline = right + left
    region = Polygon(dense(outline, True, 6)).buffer(0)
    paint = D.lin("peau-" + name, (x - 2.2, 20), (x + 3, 20),
                  [(0, shade(LEG_SH, dim)), (.25, shade(LEG_LT, dim)),
                   (.55, shade(LEG, dim)), (1, shade(LEG_SH, dim))])
    D.path(cr(outline), fill=paint)
    D.begin_batch()
    for p in poisson(region.buffer(-.3), .45, rng, limit=300):
        e = (p[0] + rng.uniform(-.10, .10), p[1] - rng.uniform(.3, .7))
        if region.contains(Point(e)):
            line([p, e], shade(LEG_LT, dim), .045, .44)
    D.flush()
    if front:
        fill(ellipse((x + .7, 16.6), .95, 1.35, .1), shade("#3A3C36", dim), .66)
        line([(x - 1.0, 19), (x - .8, 17), (x - 1.0, 15.5)], shade(LEG_SH, dim), .20, .65)
        line([(x + 1.1, 18.7), (x + 1.6, 16.7), (x + 1.1, 14.7)], shade(LEG_LT, dim), .20, .50)
    else:
        line([(x + 1.6, 26), (x - .5, 21.6), (x - 2, 18.7)], shade(LEG_SH, dim), .28, .60)
        line([(x - 1.9, 19.5), (x - 2.1, 18.4), (x - 1.6, 17.3)], shade(LEG_LT, dim), .23, .55)
    # Tendon du canon et petit ergot derrière le paturon.
    line([(x - .9, 14.2), (x - .9, 10), (x - .6, 6.6)], shade(LEG_SH, dim), .22, .55)
    fill(ellipse((x - 1.25, 4.5), .35, .55, -.4), shade(HOOF_SH, dim))
    hoof(name, x + .3, not near)
    wool_surface(Polygon(dense(sleeve, True, 6)).buffer(0), 1950 + int(x), dim=dim, density=.65)
    D.pivot(name, (x, 35.0))
    D.close()


# ------------------------------------------------------------------ queue
def tail():
    """Queue naturelle laineuse de 30 cm (fiche § 2.3), du haut de la
    croupe au voisinage du jarret, en trois pièces bien recouvertes."""
    pieces = [("tail", (23.0, 64.0), [(21.5, 65.5), (25.0, 65), (25.5, 61), (23.0, 56), (21.5, 54),
                                    (20.3, 45), (19, 40), (14.8, 40), (14.7, 47), (15.3, 54), (17, 60), (19, 63)]),
              ("tail-mid", (17.0, 42.0), [(14.5, 44), (19.5, 44), (18.9, 35), (17.9, 30), (14.0, 30), (13.8, 37)]),
              ("tail-tip", (15.9, 31.7), [(13.8, 33), (18.5, 33), (18.3, 26), (17, 22.5), (14.8, 22), (13.5, 26)])]
    for i, (name, pivot, outline) in enumerate(pieces):
        D.open(name)
        wool_surface(Polygon(dense(outline, True, 6)).buffer(0), 1927 + i, dim=.92, density=.58)
        D.pivot(name, pivot)
    for _ in pieces:
        D.close()


def neck():
    D.open("neck")
    wool_surface(NECK_POLY, 1870, density=.82)
    D.pivot("neck", NECK_BASE)
    D.close()


def body():
    D.open("body")
    # Un soupçon de mamelle ardoise, presque entièrement sous la laine.
    fill([(37.0, 29), (43.8, 28.8), (43.5, 26.0), (41.5, 24.9), (38.3, 25.1)], "#4A5058")
    fill([(41.3, 25.2), (42.1, 25.2), (41.9, 24.4), (41.5, 24.2)], "#34383E")
    wool_surface(BODY_POLY, 1936)
    D.pivot("body", (63.0, 23.0))
    D.close()


# ------------------------------------------------------------------ tête
# Le chanfrein presque droit, légèrement busqué ; la tête s'élargit vers
# la joue, puis un petit menton arrondi. La bouche est une encoche pour
# que la mâchoire ouvre réellement un espace quand elle bêle.
DORSAL = [(-1.4, .2), (-1.3, -1.6), (.5, -3.0), (3.0, -3.55),
          (5.6, -3.6), (8.4, -3.5), (11.4, -3.15), (14.6, -2.8),
          (17.6, -2.2), (20.4, -1.5), (22.8, -.65), (24.5, .15),
          (25.6, 1.1), (26.0, 2.2), (25.8, 3.45), (25.2, 4.2)]
MOUTH = [(25.2, 4.2), (24.0, 4.55), (22.0, 4.9), (20.3, 5.2)]
NOTCH = [(20.3, 5.2), (19.8, 6.3), (18.8, 8.3)]
UNDER = [(18.8, 8.3), (16.0, 9.5), (12.5, 10.5), (9.1, 10.8),
         (6, 10.5), (3.4, 9.3), (1.7, 7.8), (.4, 5.7), (-.5, 3.3), (-1.4, .2)]
HEAD_REGION = Polygon(dense(HLS(DORSAL + MOUTH[1:] + NOTCH[1:] + UNDER[1:-1]), True, 6)).buffer(0)
EYE_UV = (8.3, 1.6)
EYE = HL(*EYE_UV)


def ear(name, far):
    """Oreille fine en feuille de saule, horizontale, ardoise, sombre au
    dedans ; le bord recourbé prend un reflet froid. L'autre pavillon se
    voit juste derrière le crâne (§ 5, P14/P27)."""
    base = HL(3.0, -.25 if not far else .6)
    dx = -1.0 if not far else .95
    length = 10.9 if not far else 5.8
    dy = -.06 if not far else .015

    def P(u, v):
        return (base[0] + dx * u, base[1] + dy * u + v * (.60 if far else 1.0))

    outline = [P(-.7, -.7), P(1.1, -1.2), P(length * .45, -2.3),
               P(length * .76, -1.7), P(length, -.15),
               P(length * .77, 1.4), P(length * .49, 2.0), P(1, 1.15)]
    D.open(name)
    paint = D.lin("peau-" + name, P(3, 2), P(3, -2.2),
                  [(0, "#6E7480" if not far else "#535965"), (.45, "#464B53"), (1, "#2A2D33")])
    D.path(cr(outline), fill=paint)
    fill([P(.6, .1), P(2.3, -.65), P(length * .5, -1.35),
          P(length * .82, -.6), P(length * .89, .1),
          P(length * .53, 1.05), P(2.0, .7)], "#2E2A2D" if not far else "#29272C")
    line([P(1.1, .95), P(length * .29, 1.55), P(length * .49, 1.62)], "#7A8290", .14, .45)
    line([P(length * .65, 1.38), P(length * .82, .85), P(length * .93, .25)], "#7A8290", .12, .35)
    line([P(.7, -.15), P(1.7, -.3), P(length * .38, -.05)], "#1B1414", .28, .40)
    line([P(.8, .5), P(2.1, .65), P(3.1, .35)], "#8A929F", .27, .28)
    rng = random.Random(104 if far else 105)
    ear_region = Polygon(dense(outline, True, 4))
    D.begin_batch()
    for _ in range(45):
        u = rng.uniform(1, length * .84)
        v = rng.uniform(-1.1, .9)
        p, e = P(u, v), P(u + .45, v + .04)
        if ear_region.contains(Point(e)):
            line([p, e], "#9199A7", .04, .23)
    D.flush()
    D.pivot(name, base)
    D.close()


def jaw():
    D.open("jaw")
    # Le fond de la bouche et la langue sont cachés au repos derrière la
    # tête. Les lèvres sont bleu-noir ; pas de gencive rose ajoutée.
    fill(HLS([(16.5, 6.2), (19.8, 4.5), (23, 3.9), (25, 4),
              (24.5, 5), (22, 6), (19, 8)]), "#15171B")
    fill(HLS([(20, 5.1), (22.8, 4.5), (24.5, 4.4), (24.0, 5.0), (21.5, 5.8)]), "#554B56")
    paint = D.lin("menton", HL(23, 4), HL(23, 9.0), [(0, "#393E42"), (.4, "#252A2E"), (1, "#171B1E")])
    D.path(cr(HLS([(17.1, 7.2), (20.3, 5.3), (22.8, 4.9), (25.0, 4.3),
                    (25.6, 4.4), (25.7, 5.4), (25.1, 6.3), (23.3, 7.3),
                    (20.5, 8.6), (18.4, 9.1), (16.6, 8.9)])), fill=paint)
    line(HLS([(22.0, 5.5), (24.0, 5.1), (25.4, 4.9)]), "#6D7480", .13, .38)
    line(HLS([(25.1, 6), (23.8, 7), (22.2, 7.8)]), "#50565E", .22, .40)
    D.pivot("jaw", HL(17.8, 6.4))
    D.close()


def eyes():
    """L'iris ambre est entièrement dans une amande sombre et sa pupille
    est horizontale au repos. La paupière lourde cache le haut de l'iris."""
    D.open("eyes")
    e = EYE
    # L'axe des paupières est presque horizontal, plus ouvert derrière.
    almond = [(e[0] - 1.6, e[1] + .05), (e[0] - .9, e[1] + .85),
              (e[0] + .1, e[1] + 1), (e[0] + 1.1, e[1] + .60),
              (e[0] + 1.6, e[1] - .18), (e[0] + .8, e[1] - .75),
              (e[0] - .4, e[1] - .80), (e[0] - 1.3, e[1] - .43)]
    fill(almond, "#211F21")
    D.open("pupils")
    iris = D.rad("iris-ambre", e, 1.3, [(0, IRIS_LT), (.42, IRIS), (.82, IRIS_SH), (1, "#2D251E")],
                 f=(e[0] - .25, e[1] + .4))
    D.path(cr(ellipse(e, 1.18, .82)), fill=iris)
    rng = random.Random(25)
    for i in range(28):
        a = i * 2 * math.pi / 28
        line([(e[0] + .47 * math.cos(a), e[1] + .25 * math.sin(a)),
              (e[0] + 1.05 * math.cos(a), e[1] + .69 * math.sin(a))],
             "#C7A06A", rng.uniform(.035, .065), .40)
    X, Y = S((e[0] - .77, e[1] + .19))
    D.emit(f'<rect x="{num(X)}" y="{num(Y)}" width="1.54" height=".38" rx=".19" fill="#141312"/>')
    D.pivot("pupils", e)
    D.close()
    fill(ellipse((e[0] - .37, e[1] + .38), .22, .17), "#EDEBE4", .90)
    fill(ellipse((e[0] + .55, e[1] - .29), .08, .06), "#C7CCD3", .50)
    line(almond + [almond[0]], "#1E232B", .25, .95)
    line(almond[:5], "#282D36", .37, .95)
    D.pivot("eyes", e)
    D.close()


def face():
    paint = D.lin("tete-ardoise", HL(9, -3.5), HL(9, 10),
                  [(0, FACE_LT), (.28, "#68717F"), (.57, FACE), (1, FACE_SH)])
    D.path(runs_path([HLS(DORSAL), HLS(MOUTH), HLS(NOTCH), HLS(UNDER)]), fill=paint)
    # Les reliefs se fondent dans la peau. Les dégradés restent dans la
    # silhouette ; aucune ellipse claire ne dépasse autour des yeux.
    soft_shape("joue-lumiere", HLS([(2, 2), (6, 1), (12, 3), (16, 7), (12, 10), (5, 9)]),
               HL(7, 5), 8.4, "#A2ABB8", .21, sx=.70)
    soft_shape("chanfrein-lumiere", HLS([(8, -3), (18, -2), (24, .4), (22, 2), (15, 1), (9, -.5)]),
               HL(15, -1), 8.7, "#B2BCCB", .15, sx=.58)
    soft_shape("ganache-ombre", HLS([(2, 6), (7, 7), (16, 7), (18, 8.5), (11, 10.5), (4, 9)]),
               HL(9, 9.3), 8, "#1E252D", .26, sx=.65)
    nose = HLS([(19.8, -.5), (23.2, -.5), (25, .4), (25.9, 2.2), (25.3, 4.1),
                (22.2, 4.8), (20, 3.4), (19.5, 1.5)])
    soft_shape("mufle", nose, HL(24, 2.7), 6.8, "#191C1D", .70, sx=.8)
    # Le velours ras : un semis très fin, direction du poil allant vers le
    # nez sur le chanfrein et vers la ganache sur la joue. Rien de laineux.
    rng = random.Random(1855)
    D.begin_batch()
    for p in poisson(HEAD_REGION.buffer(-.22), .32, rng, limit=2000):
        u = (p[0] - POLL[0]) * HU[0] + (p[1] - POLL[1]) * HU[1]
        v = (p[0] - POLL[0]) * HV[0] + (p[1] - POLL[1]) * HV[1]
        length = rng.uniform(.19, .42)
        du, dv = (.7, .45) if v > 3 else (1, .1)
        e = (p[0] + length * (HU[0] * du + HV[0] * dv),
             p[1] + length * (HU[1] * du + HV[1] * dv))
        if HEAD_REGION.contains(Point(e)):
            line([p, e], "#B3BBC7" if rng.random() < .62 else "#252B35", .035, .23 if u < 20 else .13)
    D.flush()
    # Quatre plis transversaux de l'adulte, doux et discontinus (§ 5).
    for i, u in enumerate((4.6, 6.15, 8.3, 10.9)):
        pts = HLS([(u - .48, -3.1 + i * .13), (u - .34, -2.30 + i * .09),
                   (u + .14, -1.70 + i * .12), (u + .18, -.90 - (i % 2) * .27)])
        line(pts if i != 2 else pts[:-1], "#3E4552", .11 + .015 * (i % 2), .31 + i * .02)
        line([(p[0] - .07, p[1] + .15) for p in pts[1:3]], "#9AA4B3", .13, .35)
    # Le relief de l'orbite, puis une petite fossette lacrymale.
    line(HLS([(5.3, .2), (6.0, -.1), (7.8, -.25), (9.3, .1), (10.4, .8)]), "#A0AAB8", .28, .38)
    line(HLS([(9.7, 2.8), (10.3, 3.9), (11.7, 4.4)]), "#272D36", .19, .55)
    # Le naseau visible, en virgule oblique, avec le bord humide.
    nostril = [(25.2, 1.5), (25.2, 2.4), (24.6, 2.65), (23.8, 2.3),
               (23.0, 1.2), (22.4, .7), (23.4, 1), (24.2, 1.35)]
    fill(HLS(nostril), "#12161C")
    line(HLS([(22.9, .8), (23.7, 1.15), (24.5, 1.38), (25.0, 1.75)]), "#6D7480", .17, .62)
    line(HLS(MOUTH), "#111313", .20, .92)
    line(HLS([(24.9, 3.85), (23.8, 4.25), (22.4, 4.5)]), "#69717E", .11, .46)
    # Contours doux, sans trait sur l'encoche de la mâchoire ni sur le
    # raccord arrière sous la laine de l'encolure.
    line(HLS(DORSAL), "#343B45", .17, .40)
    line(HLS(UNDER[:7]), "#252B32", .18, .35)


LIP = [(21, 4.75), (21, 3.6), (22.2, 3.1), (24.2, 2.9),
       (25.9, 3.0), (25.8, 3.6), (25.2, 4.3), (23.8, 4.7), (21.6, 5.1)]


def head():
    D.open("head")
    ear("ear-far", True)
    jaw()
    face()
    eyes()
    ear("ear-near", False)
    D.pivot("head", POLL)
    D.pivot("muzzle", HL(25.1, 5.0))
    D.close()


def articulate(svg):
    """Le contrat commun : deux segments par patte, lèvre et bourrelet
    dentaire, puis trois festons du ventre attachés par leur bord haut."""
    root = ET.fromstring(svg, parser=ET.XMLParser(target=ET.TreeBuilder(insert_comments=True)))
    paints = A.Paints(root)
    for name, x, front, near in LEGS:
        joint, half = A.leg_joint(svg, name, S((0, CARPE if front else JARRET))[1])
        A.split_leg(root, paints, name, joint, half + .30)
    pad = HLS([(21.4, 4.6), (21.6, 3.8), (22.8, 3.5), (24.6, 3.4),
               (25.2, 3.7), (24.6, 4.15), (23.2, 4.4)])
    gum = A.gum([S(p) for p in pad], [S(p) for p in HLS([(21.8, 4.2), (23.1, 4.0), (24.5, 3.9)])],
                "#2E3440", "#1C242F", .15)
    A.lift_part(root, paints, "head", [S(p) for p in HLS(LIP)], "lip", S(HL(21.4, 3.8)), under=gum)
    # La découpe est au creux de chaque feston, avec du recouvrement sous
    # le corps : seules les pointes pendent et balaient, sans trou au ventre.
    for i, (x0, x1, h) in enumerate(((46.2, 51.5, 25.4), (61, 69, 25.2), (79, 85, 26.1)), 1):
        area = [(x0, 0), (x1, 0), (x1, h), (x0, h)]
        A.cut_under(root, paints, "body", [S(p) for p in area], f"sway-{i}", S(((x0 + x1) / 2, h)), margin=1.3)
    ET.indent(root, space="  ")
    return ET.tostring(root, encoding="unicode") + "\n"


def main(path):
    D.lin("fond-laine", (0, 78), (0, 21), [(0, "#CDBFA5"), (.5, "#AB9A7C"), (1, WOOL_DEEP)])
    for args in LEGS:
        leg(*args)
    tail()
    neck()
    body()
    head()
    D.pivot("ground", (64, 0))
    comment = ("  Bleu du Maine : brebis adulte en toison d'automne, profil droit.\n"
               "  68 cm au garrot, une unité = un centimètre. Tête nue ardoise, sans cornes.\n"
               "  Généré par tools/races/bleu-du-maine/gen_bleu_du_maine.py.\n"
               "  © 2026 Alejandro San Martin Lamas — MIT OR Apache-2.0")
    svg = articulate(D.render(148, 108, comment))
    if VARIANTE:
        return variante(path, svg)
    path = Path(path)
    path.parent.mkdir(parents=True, exist_ok=True)
    path.write_text(svg, encoding="utf-8")
    print(f"{path} : {len(svg) / 1024:.0f} Ko, {svg.count('<path')} tracés")


def variante(path, svg):
    """Les autres dessins : ce qui change, puis les proportions."""
    root = ET.fromstring(svg)
    if VARIANTE == "belier":
        variantes.bourses(root, *variantes.sous_le_ventre(root, 6, -4), largeur=9, hauteur=14,
                  peau="#CDBFA5", ombre="#8E8273")
    variantes.ecrire(path, root, **PROPORTIONS[VARIANTE])


if __name__ == "__main__":
    main(sys.argv[1] if len(sys.argv) > 1 else HERE.parents[2] / "pets/bleu-du-maine/bleu-du-maine.svg")
