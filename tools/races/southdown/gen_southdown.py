#!/usr/bin/env python3
"""Une agnelle Southdown anglaise de huit semaines, au pré, en mai.

Repère en cm : x vers l'avant, h au-dessus du sol. Garrot osseux 45 cm,
laine 2 cm ; tête 16,65 cm, carpe 15,3 cm et jarret 17,1 cm. Voir la fiche
docs/races/southdown.md, en particulier §§ 2, 4, 5, 7 et 9.

La laine courte est une mosaïque irrégulière de petites boucles fermées,
modelée par les volumes, sans frange ni mèches pendantes artificielles.
Pattes dessinées entières puis recoupées par l'outil commun, comme les
deux dernières races ; encolure complète sous le corps, lèvre détachée.

Usage : python3 tools/races/southdown/gen_southdown.py pets/southdown/southdown.svg
© 2026 Alejandro San Martin Lamas — MIT OR Apache-2.0
"""

import math
from pathlib import Path
import random
import sys
import xml.etree.ElementTree as ET

HERE = Path(__file__).resolve().parent
sys.path.insert(0, str(HERE.parent))
import articulations as A
from shapely.geometry import Point, Polygon, LineString
from shapely.affinity import affine_transform
from outils import Doc, S, cr, dense, geom_d, mix, poisson, shade
import variantes  # noqa: E402

# Les dessins de la race (tools/races/variantes.py). Sans variante : la brebis
# adulte, le dessin de base ; "belier" ; "agneau" : l'agnelle de huit
# semaines, le premier dessin de la race, tel quel. Les adultes sont l'agnelle
# mise au canon de l'adulte (fiche, § 1.5, 2.1 et 2.9) : un tonneau de laine
# sur des pattes courtes, la tête petite, le museau souris, la truffe presque
# noire, l'œil ambré, les chaussettes gris-faon.
VARIANTE = sys.argv[2] if len(sys.argv) > 2 else ""
ADULTE = VARIANTE != "agneau"
AGE_AGNEAU = 8.0  # semaines : l'âge de l'agneau dessiné (weeks, dans pet.toml)
# Les pattes gardent leurs proportions (leurs articulations restent rondes) :
# le tronc, deux fois plus profond, descend sur elles (`enfonce`), et la
# laine les couvre jusqu'aux genoux et aux jarrets.
BREBIS = dict(garrot=(45, 60), corps=(1.23, 1.55), enfonce=4.85, encolure=(.85, 1.3),
              tete=(1.02, .94), oreilles=.78, queue=(1.4, .82), port=-12, port_tete=20)
PROPORTIONS = {
    "belier": dict(  # Bélier Southdown : 67 cm au garrot, sans cornes, plus lourd, encolure épaisse, tête plus large.
        garrot=(45, 67), corps=(1.25, 1.6), enfonce=5.1, encolure=(.85, 1.5),
        tete=(1.05, .98), oreilles=.74, queue=(1.4, .82), port=-12, port_tete=20),
    "agneau": dict(  # Agnelle Southdown de huit semaines, 45 cm au garrot : laine crème frisée, museau souris, truffe gris-rosé, longues pattes aux chaussettes faon, queue entière.
    ),
}


def ton(agnelle, adulte):
    """Une couleur de l'agnelle, ou celle de l'adulte (fiche, § 2.9 et 4)."""
    return adulte if ADULTE else agnelle

D = Doc()
R = random.Random(5608)
POLL = (51.0, 55.0)
NECK_PIVOT = (44.5, 34.0)
TH = math.radians(65)
HU, HV = (math.cos(TH), -math.sin(TH)), (-math.sin(TH), -math.cos(TH))


def H(u, v):
    """Repère de la tête : u vers le nez, v vers la ganache."""
    return (POLL[0] + u * HU[0] + v * HV[0], POLL[1] + u * HU[1] + v * HV[1])


def hs(pts):
    return [H(*p) for p in pts]


def ellipse(c, rx, ry, angle=0, n=16):
    ca, sa = math.cos(angle), math.sin(angle)
    return [
        (
            c[0] + rx * math.cos(a) * ca - ry * math.sin(a) * sa,
            c[1] + rx * math.cos(a) * sa + ry * math.sin(a) * ca,
        )
        for a in (2 * math.pi * i / n for i in range(n))
    ]


def shape(pts, fill, **kw):
    D.path(cr(pts), fill=fill, **kw)


def region(pts):
    return Polygon(dense(pts, True, 8)).buffer(0)


def glow(gid, c, rx, ry, col, opacity, area):
    """Une lumière douce, géométriquement rognée, sans clip-path SVG."""
    g = region(ellipse(c, rx, ry)).intersection(area)
    D.path(
        geom_d(g),
        fill=D.rad(
            gid,
            c,
            ry,
            [(0, col, opacity), (0.5, col, opacity * 0.65), (1, col, 0)],
            sx=rx / ry,
        ),
    )


def light_at(x, h):
    """Éclairage global et paquets de boucles, sans rayures régulières."""
    return max(
        0,
        min(
            1,
            0.30
            + 0.60 * (h - 26) / 22
            + 0.065 * math.sin(x / 2.8 + h / 2.1) * math.sin(h / 3.6 - x / 4.2),
        ),
    )


def wool(pts, gid, spacing=0.67, head=False, dim=1.0, contour=True):
    """Toison serrée : bourrelets irréguliers, croissants et spires courtes.

    Les centres sont distribués par Poisson (pas de grille). Les boucles
    sont limitées à leur région avant l'écriture, sans masque. Les creux
    restent discrets : l'astrakan d'un agneau n'est pas une longue laine.
    """
    area = region(pts)
    top, bottom = area.bounds[3], area.bounds[1]
    colors = [
        shade(c, dim)
        for c in (("#C2B4A5", "#E6DACB", "#F2EAE0") if head else ("#B9AEA6", "#E4DCD2", "#F3EEE7"))
    ]
    fill = D.lin(
        gid,
        (0, top if head else 47),
        (0, bottom if head else 22),
        [(0, colors[2]), (0.48, colors[1]), (1, colors[0])],
    )
    shape(pts, fill)
    centers = poisson(area.buffer(-spacing * 0.14), spacing, R)
    # Le modelé de chaque petite boucle reste lié à la lumière du volume.
    patches, dark, bright = {}, {}, {}
    for x, h in sorted(centers, key=lambda p: -p[1]):
        rad = spacing * R.uniform(0.66, 0.87)
        a = R.uniform(-math.pi, math.pi)
        l = (0.28 + 0.68 * (h - bottom) / max(1, top - bottom)) if head else light_at(x, h)
        l = round(max(0, min(1, l + R.uniform(-0.035, 0.035))) * 12) / 12
        col = (
            mix(colors[0], colors[1], l / 0.65)
            if l < 0.65
            else mix(colors[1], colors[2], (l - 0.65) / 0.35)
        )
        points = []
        for i in range(10):
            t = i * math.tau / 10
            r = rad * (1 + 0.16 * math.sin(3 * t + a) + R.uniform(-0.06, 0.06))
            points.append((x + r * math.cos(t), h + r * 0.83 * math.sin(t)))
        patch = region(points).intersection(area)
        patches.setdefault(col, []).append(geom_d(patch, simplify=0.045))
        # Deux arcs disjoints : pas d'anneau sombre autour de chaque boucle.
        for target, color, start, end, factor in (
            (dark, mix(col, "#8E837D", 0.28), a + 2.2, a + 6.3, 0.64),
            (bright, mix(col, "#FFF9EF", 0.48), a + 0.15, a + 2.4, 0.76),
        ):
            arc = [
                (
                    x + rad * factor * (1 - 0.45 * j / 8) * math.cos(t),
                    h + rad * 0.67 * (1 - 0.45 * j / 8) * math.sin(t),
                )
                for j, t in enumerate(start + (end - start) * j / 8 for j in range(9))
            ]
            if area.contains(LineString(arc)):
                # Palette quantifiée : les traits sont regroupés pour la cuisson.
                key = tuple(round(int(color[i : i + 2], 16) / 8) * 8 for i in (1, 3, 5))
                color = "#%02X%02X%02X" % tuple(min(255, v) for v in key)
                target.setdefault(color, []).append(cr(arc, closed=False))
    # Les remplissages opaques de même teinte partagent aussi un tracé :
    # le moteur parcourt les éléments même une fois le dessin cuit.
    # Seules des boucles voisines, presque de même valeur, se recouvrent.
    for col, ds in patches.items():
        D.path("".join(ds), fill=col)
    for col, arcs in dark.items():
        D.path("".join(arcs), stroke=col, sw=spacing * 0.105, so=0.52, cap="round")
    for col, arcs in bright.items():
        D.path("".join(arcs), stroke=col, sw=spacing * 0.10, so=0.72, cap="round")
    if contour:
        # Des boucles minuscules adoucissent le contour, sans l'allonger.
        edge = area.exterior
        borders = {}
        for i in range(int(edge.length / spacing)):
            p = edge.interpolate(i * spacing + R.uniform(0.1, 0.7) * spacing)
            x, h = p.x, p.y
            l = max(0, min(1, (h - bottom) / max(1, top - bottom))) if head else light_at(x, h)
            col = mix(colors[0], colors[2], round((0.25 + 0.7 * l) * 12) / 12)
            r = spacing * R.uniform(0.24, 0.36)
            borders.setdefault(col, []).append(cr(ellipse((x, h), r, r * 0.8, n=8)))
        for col, ds in borders.items():
            D.path("".join(ds), fill=col)
    return area


BODY = [
    (1.5, 40.5),
    (3, 44.0),
    (7, 46.5),
    (13, 47.2),
    (23, 47.0),
    (33, 46.8),
    (39, 47),
    (42, 46.7),
    (44.7, 44.4),
    (46, 41),
    (47, 36),
    (47.6, 31.8),
    (45, 28),
    (39, 26.5),
    (30, 26.0),
    (21, 26.6),
    (15, 27.6),
    (10, 28.2),
    (5, 28.5),
    (1.7, 31.2),
    (0, 35.8),
]
NECK = [
    (37, 43.6),
    (40, 46.8),
    (42.4, 50.2),
    (45.2, 53.8),
    (48.2, 56.2),
    (51.8, 56),
    (54, 53),
    (53, 48.5),
    (51, 43),
    (50, 38),
    (48.8, 32.5),
    (46, 29.2),
    (42, 30.2),
    (39, 34),
    (37, 38),
]


def leg(name, kind, dx, far):
    """Quatre membres fins, laine crème au-dessus des chaussettes faon."""
    front = kind == "front"
    joint_h = 15.3 if front else 17.1
    pivot_h = 32 if front else 36
    pivot_x = dx if front else dx + 1
    # Le jarret forme une pointe arrière ; le paturon porte un petit sabot.
    if front:
        outline = [
            (dx - 2.7, 34),
            (dx + 2.8, 34),
            (dx + 2.1, 24),
            (dx + 1.2, 18),
            (dx + 1.1, 15.2),
            (dx + 0.72, 6),
            (dx + 1.15, 2.3),
            (dx + 1, 1.2),
            (dx - 1.05, 1.1),
            (dx - 1.15, 3),
            (dx - 1.0, 7),
            (dx - 1.25, 15.3),
            (dx - 1.4, 22),
        ]
    else:
        outline = [
            (dx - 3.3, 37),
            (dx + 4.4, 37),
            (dx + 5, 31),
            (dx + 3.4, 25),
            (dx + 0.3, 20),
            (dx - 0.9, 17.1),
            (dx - 0.2, 12),
            (dx + 0.5, 6),
            (dx + 1.05, 2.3),
            (dx + 0.8, 1.1),
            (dx - 1.2, 1.1),
            (dx - 1.7, 3.2),
            (dx - 2.0, 10),
            (dx - 2.6, 16.9),
            (dx - 2.1, 20.2),
            (dx - 0.6, 25),
            (dx - 3, 31),
        ]
    dim = 0.85 if far else 1
    D.open(name)
    fill = D.lin(
        name + "-poil",
        (dx - 2, 15),
        (dx + 2, 15),
        [
            (0, shade(ton("#6B5244", "#665950"), dim)),
            (0.45, shade(ton("#BC9C84", "#B4A594"), dim)),
            (1, shade(ton("#9C7A64", "#94836F"), dim)),
        ],
    )
    shape(outline, fill)
    area = region(outline)
    glow(name + "-tendon", (dx + 0.2, 8), 0.65, 6, ton("#D1B49D", "#CFC2B2"), 0.38, area)
    # Le poil court se couche le long du canon.
    D.begin_batch()
    for p in poisson(area.buffer(-0.18), 0.28, R):
        x, h = p
        if 2.4 < h < joint_h + 1:
            D.stroke(
                cr([(x, h), (x + 0.03, h - 0.25), (x + 0.08, h - 0.45)], False),
                shade(ton("#DAC4AE", "#D8CBBB") if R.random() < 0.45 else ton("#775B4B", "#74675B"), dim),
                0.045,
                0.42,
            )
    D.flush()
    upper = area.intersection(
        Polygon([(-10, joint_h - 0.45), (80, joint_h - 0.45), (80, 60), (-10, 60)])
    )
    wool(list(upper.exterior.coords)[:-1], name + "-laine", 0.46, dim=dim, contour=False)
    # Quelques boucles débordent sur la limite oblique du poil faon.
    for k in range(7):
        x = dx - 1.0 + k * 0.31
        h = joint_h + 0.35 - 0.3 * math.sin(k * 0.7)
        if area.contains(Point(x, h)):
            shape(ellipse((x, h), 0.25, 0.38), shade("#D8CABB", dim))
    hoof = [
        (dx - 1.18, 2.4),
        (dx - 0.25, 2.55),
        (dx + 0.7, 2.25),
        (dx + 1.7, 0.8),
        (dx + 1.72, 0.08),
        (dx + 0.7, 0),
        (dx - 1.25, 0),
        (dx - 1.4, 0.45),
    ]
    shape(
        hoof,
        D.lin(
            name + "-onglon",
            (dx - 1, 2.4),
            (dx + 1, 0),
            [(0, shade("#5F5856", dim)), (0.6, shade("#3A3434", dim)), (1, "#1F1B1B")],
        ),
    )
    D.path(
        cr([(dx + 0.5, 1.85), (dx + 0.75, 0.8), (dx + 0.7, 0.12)], False),
        stroke="#211D1D",
        sw=0.12,
        cap="round",
    )
    D.path(
        cr([(dx - 0.85, 1.8), (dx - 0.8, 0.5)], False),
        stroke="#8C847C",
        sw=0.09,
        so=0.58,
        cap="round",
    )
    shape(ellipse((dx - 1.3, 3.6), 0.22, 0.36), shade("#62514B", dim))
    D.pivot(name, (pivot_x, pivot_h))
    D.close()


def tail():
    """Queue entière de 25 cm, fine, jusqu'au jarret, en trois segments."""
    D.open("tail")
    wool(
        [
            (2.7, 43.5),
            (0.1, 44),
            (-1.4, 40.2),
            (-2, 35),
            (-1.5, 32),
            (0.8, 32.5),
            (1.4, 37),
        ],
        "queue-racine",
        0.5,
    )
    D.pivot("tail", (1.8, 42.8))
    D.open("tail-mid")
    wool(
        [
            (-1.7, 35),
            (-2.2, 31),
            (-2.2, 27),
            (-1.9, 24),
            (0.4, 24),
            (0.6, 28),
            (0.6, 32.5),
        ],
        "queue-milieu",
        0.46,
    )
    D.pivot("tail-mid", (-0.7, 33.5))
    D.open("tail-tip")
    wool(
        [
            (-1.9, 26),
            (-1.9, 22),
            (-1.3, 18.8),
            (-0.3, 17.7),
            (0.65, 18.4),
            (0.9, 21.2),
            (0.5, 25.8),
        ],
        "queue-bout",
        0.4,
    )
    D.pivot("tail-tip", (-0.5, 25))
    D.close()
    D.close()
    D.close()


DORSAL = [
    (-1.4, 0.3),
    (-1.5, -1.3),
    (-0.3, -2.7),
    (2, -3.45),
    (4.7, -3.25),
    (7, -2.8),
    (9, -2),
    (11, -1.65),
    (13, -1.45),
    (15, -0.7),
    (16.2, 0.1),
    (16.65, 1.2),
    (16.6, 2.6),
]
MOUTH = [(16.6, 2.6), (15.7, 3.6), (14.7, 4.3), (14.0, 4.5)]
UNDER = [
    (14, 4.5),
    (12.2, 7.2),
    (9.3, 8.5),
    (6.8, 8.4),
    (4.3, 7.5),
    (2.2, 5.8),
    (0.4, 3.3),
    (-1.4, 0.3),
]
HEAD = hs(DORSAL + MOUTH[1:] + UNDER[1:-1])
EYE = H(6.1, -0.3)


def ear(far=False):
    name = "ear-far" if far else "ear-near"
    # L'oreille du fond reste sous le haut du bonnet : de profil, seul son
    # bord arrière dépasse. Placée plus haut, elle évoquerait un cornillon.
    base = H(1.0, 0.2) if far else H(1.8, 1.1)
    # Profil : la feuille s'étend vers l'arrière ; on voit surtout le dehors.
    length = 5.0 if far else 7.4

    def F(x, y):
        return (base[0] - x, base[1] + y)

    outline = [
        F(0, 0),
        F(1.9, 1.25),
        F(4.6, 1.65),
        F(length, 0.6),
        F(length - 0.5, -0.75),
        F(4.5, -1.5),
        F(1.7, -1.05),
    ]
    D.open(name)
    wool(outline, name + "-laine", 0.29, head=True, dim=0.87 if far else 1, contour=False)
    inner = [
        F(0.7, 0.15),
        F(2.8, 0.65),
        F(length - 0.6, 0.4),
        F(5, -0.28),
        F(2.5, -0.4),
    ]
    shape(
        inner,
        D.lin(
            name + "-dedans",
            F(3, 1),
            F(3, -0.8),
            [(0, "#CFAC9B"), (0.6, "#B98E7C"), (1, "#805A4B")],
        ),
    )
    D.path(
        cr([F(1.1, 0.08), F(3, -0.18), F(5, -0.2)], False),
        stroke="#D2B4A0",
        sw=0.12,
        so=0.75,
        cap="round",
    )
    D.begin_batch()
    for i in range(35):
        x = R.uniform(1, length - 0.7)
        y = R.uniform(-0.8, 1.1)
        if region(outline).contains(Point(F(x, y))):
            D.stroke(cr([F(x, y), F(x + 0.25, y - 0.06)], False), "#F4E9D9", 0.045, 0.7)
    D.flush()
    D.pivot(name, base)
    D.close()


def jaw():
    D.open("jaw")
    shape(
        hs(
            [
                (9, 4.5),
                (12.2, 2.1),
                (16.1, 2.0),
                (16.45, 2.8),
                (15.7, 4.1),
                (12.5, 6.1),
                (9.4, 6.9),
            ]
        ),
        "#69484A",
    )
    shape(hs([(11.3, 4.2), (14.7, 2.65), (16, 2.5), (15.8, 3.3), (12.4, 5.1)]), "#BD8B8D")
    shape(
        hs(
            [
                (10, 5.7),
                (13, 4.5),
                (15.2, 3.3),
                (16.6, 2.65),
                (16.8, 3.5),
                (16.2, 4.6),
                (14.5, 6.2),
                (12, 7.3),
                (10.4, 7.4),
                (9, 6.8),
            ]
        ),
        D.lin("menton", H(13, 3), H(13, 7.2),
              [(0, ton("#D7CBBF", "#B5A89C")), (1, ton("#BBA99A", "#8D8076"))]),
    )
    D.path(cr(hs(MOUTH), False), stroke="#8C726C", sw=0.08, so=0.4, cap="round")
    D.pivot("jaw", H(10, 5.4))
    D.close()


def eye():
    """Œil sombre dégagé ; l'iris et la fente horizontale suivent le regard."""
    x, h = EYE
    almond = [
        (x - 1.05, h),
        (x - 0.6, h + 0.7),
        (x + 0.15, h + 0.8),
        (x + 0.9, h + 0.35),
        (x + 1.05, h - 0.05),
        (x + 0.5, h - 0.55),
        (x - 0.25, h - 0.6),
        (x - 0.85, h - 0.35),
    ]
    shape(almond, "#B9A497")
    D.path(
        cr([(x - 1, h), (x, h - 0.17), (x + 1, h - 0.05)], False),
        stroke="#705D56",
        sw=0.14,
        cap="round",
    )
    D.open("eyes")
    shape(almond, "#392F29")
    D.open("pupils")
    shape(
        ellipse((x, h), 0.75, 0.74),
        D.rad(
            "iris",
            (x - 0.15, h + 0.2),
            0.85,
            [(0, ton("#6A4E30", "#C8963F")), (0.55, ton("#4E3927", "#9A6C2A")),
             (1, ton("#2F251D", "#573B1A"))],
        ),
    )
    # Fente horizontale au repos, même si l'axe du crâne plonge.
    shape(
        [
            (x - 0.67, h + 0.13),
            (x + 0.59, h + 0.13),
            (x + 0.69, h - 0.02),
            (x + 0.59, h - 0.15),
            (x - 0.66, h - 0.15),
            (x - 0.75, h - 0.02),
        ],
        "#121412",
    )
    D.pivot("pupils", EYE)
    D.close()
    shape(ellipse((x - 0.34, h + 0.37), 0.13, 0.10), "#FFF9F2", fo=0.75)
    D.path(cr(almond[:5], False), stroke="#5A4744", sw=0.16, cap="round")
    for k in range(5):
        a = x - 0.65 + k * 0.28
        D.path(
            cr([(a, h + 0.56), (a - 0.05, h + 0.66 + 0.02 * (k % 2))], False),
            stroke="#CDBDA8",
            sw=0.04,
            so=0.7,
            cap="round",
        )
    D.pivot("eyes", EYE)
    D.close()


LIP = hs(
    [
        (13.9, 3.7),
        (14, 2.8),
        (14.9, 2),
        (16.7, 1.8),
        (16.8, 2.6),
        (15.7, 3.7),
        (14.7, 4.4),
        (14, 4.6),
    ]
)


def head():
    D.open("head")
    ear(True)
    jaw()
    area = region(HEAD)
    paint_start = len(D.out)
    wool(HEAD, "face", 0.36, head=True)
    # Un chanfrein droit et court ; la transition souris s'efface dans la laine.
    muzzle = hs(
        [
            (7, -2.8),
            (10, -1.8),
            (13, -1.45),
            (15, -0.7),
            (16.65, 1.2),
            (16.6, 2.6),
            (14, 4.5),
            (12.2, 7.2),
            (10, 5.7),
            (8.5, 2),
        ]
    )
    # Le gris se fond aussi sur les côtés : un bord de masque opaque
    # transformerait la joue en coin. Dégradé elliptique dans toute la tête.
    shape(
        HEAD,
        D.rad(
            "souris",
            H(15.8, 1.1),
            ton(4.2, 5.4),
            [
                (0, ton("#A39489", "#796C65")),
                (0.42, ton("#B8ABA0", "#978A81")),
                (0.72, ton("#CFC4BA", "#BBAEA3"), ton(0.7, 0.8)),
                (1, "#CFC4BA", 0),
            ],
            sx=2.0,
            angle=-TH,
        ),
    )
    # Les boucles se perdent progressivement dans le poil du chanfrein.
    glow("joue-lumiere", H(6, 4.6), 2.5, 2.6, "#FFF6E6", 0.24, area)
    # Poil très fin vers le museau, sans marques sombres au front.
    mp = region(muzzle).buffer(-0.22).difference(Point(EYE).buffer(1.55))
    D.begin_batch()
    for x, h in poisson(mp, 0.23, R):
        e = (x - HU[0] * 0.32, h - HU[1] * 0.32)
        if mp.contains(Point(e)):
            D.stroke(
                cr([(x, h), e], False),
                ton("#E1D2C3", "#C9BBAE") if R.random() < 0.6 else ton("#8F8279", "#6F625A"),
                0.04,
                0.27,
            )
    D.flush()
    nose = hs(
        [
            (14.8, -0.7),
            (16, 0.0),
            (16.65, 1.2),
            (16.6, 2.6),
            (15.5, 2.95),
            (14.4, 2.3),
            (14.3, 0.6),
        ]
    )
    shape(
        nose,
        D.lin(
            "truffe",
            H(15, 0),
            H(16, 3),
            [(0, ton("#C2A79E", "#5E504E")), (0.5, ton("#9E8079", "#3B3131")),
             (1, ton("#6E5652", "#1F1A1A"))],
        ),
    )
    shape(
        hs(
            [
                (15.02, 0.85),
                (15.48, 0.74),
                (16.04, 1.04),
                (16.11, 1.49),
                (15.89, 1.75),
                (15.59, 1.56),
                (15.44, 1.15),
            ]
        ),
        ton("#3A2C2C", "#110D0D"),
    )
    D.path(
        cr(hs([(14.9, 0.5), (15.6, 0.34), (16.1, 0.7)]), False),
        stroke=ton("#D6BEB2", "#8F807B"),
        sw=0.1,
        so=0.7,
        cap="round",
    )
    for u, v, r in [
        (15.0, 1.9, 0.14),
        (14.6, 0.2, 0.12),
        (16.35, 2.2, 0.10),
        (15.5, 2.5, 0.16),
        (15.9, 0.25, 0.09),
    ]:
        shape(ellipse(H(u, v), r, r * 0.7), ton("#4A3F42", "#1A1515"), fo=0.8)
    D.path(
        cr(hs([(16.55, 1.9), (16.6, 2.6)]), False),
        stroke=ton("#6E5652", "#2B2323"),
        sw=0.12,
        cap="round",
    )
    D.path(cr(hs(MOUTH), False), stroke="#5E4A47", sw=0.10, so=0.7, cap="round")
    # Le larmier étroit reste libre de laine.
    D.path(
        cr(hs([(7.1, 0.1), (7.4, 0.35), (7.65, 0.75)]), False),
        stroke="#9A8577",
        sw=0.1,
        so=0.45,
        cap="round",
    )
    # Garder exactement la peinture de la face, pour habiller le pourtour
    # de l'œil après ses parties mobiles sans créer une pastille rasée.
    face_paints = D.out[paint_start:]
    eye()
    # Le globe reste dans l'amande quand le moteur déplace le regard.
    # Cette paupière appartient à la tête, pas aux pupilles mobiles.
    x, h = EYE
    aperture = region(
        [
            (x - 1.05, h),
            (x - 0.6, h + 0.7),
            (x + 0.15, h + 0.8),
            (x + 0.9, h + 0.35),
            (x + 1.05, h - 0.05),
            (x + 0.5, h - 0.55),
            (x - 0.25, h - 0.6),
            (x - 0.85, h - 0.35),
        ]
    )
    orbit = region(ellipse(EYE, 2.7, 2.4)).intersection(area)
    ox, oy = S((0, 0))
    ring = affine_transform(orbit.difference(aperture), [1, 0, 0, -1, ox, oy])
    group = ET.fromstring(f'<g xmlns="{A.SVG_NS}">' + "".join(face_paints) + "</g>")
    defs = ET.fromstring(f'<svg xmlns="{A.SVG_NS}"><defs>' + "".join(D.defs) + "</defs></svg>")
    # Découpe géométrique des tracés lors de la génération, aucun masque SVG.
    cover = A._clip_tree(group, ring, A.Paints(defs))
    D.emit(ET.tostring(cover, encoding="unicode"))
    ear()
    D.pivot("head", POLL)
    D.pivot("muzzle", H(16.5, 3.0))
    D.close()


LEGS = [
    ("leg-back-far", "back", 14, True),
    ("leg-front-far", "front", 37, True),
    ("leg-back-near", "back", 7, False),
    ("leg-front-near", "front", 43, False),
]


def articulate(svg):
    parser = ET.XMLParser(target=ET.TreeBuilder(insert_comments=True))
    root = ET.fromstring(svg, parser=parser)
    paints = A.Paints(root)
    for name, kind, dx, far in LEGS:
        joint, half = A.leg_joint(svg, name, S((0, 15.3 if kind == "front" else 17.1))[1])
        A.split_leg(root, paints, name, joint, half + 0.2)
    pad = hs([(14.1, 3.7), (14.4, 2.7), (15.5, 1.8), (16.2, 2.1), (15.8, 2.8), (14.5, 4.0)])
    gum = A.gum(
        [S(p) for p in pad],
        [S(p) for p in hs([(14.4, 3.5), (15.5, 2.6)])],
        "#BD898A",
        "#86575D",
        0.1,
    )
    A.lift_part(root, paints, "head", [S(p) for p in LIP], "lip", S(H(14, 3.7)), under=gum)
    ET.indent(root, space="  ")
    return ET.tostring(root, encoding="unicode") + "\n"


def main(path):
    for args in LEGS:
        leg(*args)
    tail()
    D.open("neck")
    wool(NECK, "encolure", 0.59)
    D.pivot("neck", NECK_PIVOT)
    D.close()
    D.open("body")
    area = wool(BODY, "corps", 0.66)
    # La toison courte laisse deviner l'épaule et la cuisse : modelé doux,
    # sans dessiner les plis d'un adulte tondu sur cette laine d'agneau.
    glow("epaule-ombre", (42.5, 33.5), 4.0, 7.0, "#8E837D", 0.12, area)
    glow("cuisse-ombre", (7.0, 34.0), 5.5, 7.0, "#8E837D", 0.10, area)
    glow("flanc-lumiere", (17, 39), 16, 8, "#FFF5E8", 0.18, area)
    D.pivot("body", (25, 27))
    D.close()
    head()
    D.pivot("ground", (25, 0))
    svg = D.render(
        76,
        67,
        "Southdown anglais : agnelle de huit semaines, laine entière, queue naturelle.\n"
        "1 unité = 1 cm. Garrot osseux 45 cm. Profil droit.\n"
        "Généré par tools/races/southdown/gen_southdown.py.\n"
        "© 2026 Alejandro San Martin Lamas — MIT OR Apache-2.0",
    )
    out = articulate(svg)
    if ADULTE:
        return variante(path, out)
    Path(path).parent.mkdir(parents=True, exist_ok=True)
    Path(path).write_text(out, encoding="utf-8")
    print(f'{path} : {len(out)/1024:.0f} Ko, {out.count("<path")} tracés')


def variante(path, svg):
    """La brebis ou le bélier : l'agnelle, mise au canon de l'adulte."""
    root = ET.fromstring(svg)
    if VARIANTE == "":
        # La mamelle, petite, dans la laine entre les postérieurs.
        variantes.mamelle(root, *variantes.sous_le_ventre(root, 5.0, -1.9), largeur=4.4, hauteur=2.6,
                          peau="#D9B7A9", ombre="#A9867A")
        return variantes.ecrire(path, root, **BREBIS)
    # Le scrotum, laineux, descend à mi-canon.
    variantes.bourses(root, *variantes.sous_le_ventre(root, 5, -1.5), largeur=5.2, hauteur=5.4,
                      peau="#DDD3C6", ombre="#9C9085")
    variantes.ecrire(path, root, **PROPORTIONS[VARIANTE])


if __name__ == "__main__":
    main(sys.argv[1] if len(sys.argv) > 1 else "pets/southdown/southdown.svg")
