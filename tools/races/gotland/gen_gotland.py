#!/usr/bin/env python3
"""Écrit une brebis Gotland adulte en toison d'automne, d'après nature.

Une unité = un centimètre ; x vers l'avant, h au-dessus du sol. Brebis de
70 cm au garrot sous 7 cm de laine, profil droit, sans cornes. Les dimensions,
la palette et les dix détails viennent de docs/races/gotland.md. Les photos
relues sont consignées dans README.md ; aucune photo n'est embarquée.

Les petites vrilles du dos et du cou, les anneaux souples du flanc et les
festons du ventre sont de la même toison : ombre du creux, ruban gris, deux
fibres de lumière discontinues. Leurs tailles, inclinaisons et ouvertures
varient avec une graine fixe. La laine n'est ni un semis de disques, ni une
frange de mèches longues. Les pattes et la lèvre sont découpées par l'outil
commun après le dessin ; l'encolure entière passe sous le corps.

Usage : python3 tools/races/gotland/gen_gotland.py pets/gotland/gotland.svg
© 2026 Alejandro San Martin Lamas — MIT OR Apache-2.0
"""
import math
from pathlib import Path
import random
import sys
import xml.etree.ElementTree as ET

HERE = Path(__file__).resolve().parent
sys.path.insert(0, str(HERE.parent))
sys.path.insert(0, str(HERE))
import articulations as A  # noqa: E402
import variantes  # noqa: E402

# Les autres dessins de la race (tools/races/variantes.py), d'après la fiche.
# Sans variante : la brebis.
VARIANTE = sys.argv[2] if len(sys.argv) > 2 else ""
PROPORTIONS = {
    "belier": dict(  # Bélier de Gotland : 78 cm au garrot, sans cornes, encolure plus épaisse et plus haute, sans crinière.
        garrot=(70, 78), encolure=(1.04, 1.16), tete=(1.04, 1.06), port=3),
    "agneau": dict(  # Agneau de Gotland de deux semaines : 42 cm au garrot, né noir, toison de naissance noire et frisée.
        variantes.AGNEAU, garrot=(70, 42), corps=(.84, .8)),
}
from shapely.geometry import Point, Polygon  # noqa: E402
from outils import (Doc, OY, S, add, clamp, cr, dense, dist, element, geom_d,  # noqa: E402
                    lerp, lstroke, mix, mul, norm, poisson, poly, qstroke,
                    ramp, ringlet, rot, shade, sub)

D = Doc()
R = random.Random(1971)  # enregistrement séparé du Gute (fiche § 1)
GARROT, TOISON, CARPE, JARRET = 70.0, 77.0, 18.4, 20.0
POLL, NECK_BASE = (100.0, 91.0), (82.5, 54.0)
HEAD_ANGLE = math.radians(35)
HU = (math.cos(HEAD_ANGLE), -math.sin(HEAD_ANGLE))
HV = (-math.sin(HEAD_ANGLE), -math.cos(HEAD_ANGLE))


def HL(u, v):
    return add(POLL, add(mul(HU, u), mul(HV, v)))


def HP(points):
    return [HL(*p) for p in points]


def ellipse(c, rx, ry, angle=0.0, n=24):
    return [add(c, rot((rx * math.cos(t * 2 * math.pi / n), ry * math.sin(t * 2 * math.pi / n)), angle))
            for t in range(n)]


def line(points, color, width, opacity=1.0):
    D.path(cr(points, False), stroke=color, sw=width, so=opacity, cap="round", join="round")


# La masse : dos droit, corps profond, ventre presque horizontal. Les
# festons sont dessinés par les boucles, sans contour ondulé de nuage.
BODY = [(1.4, 66.0), (3.0, 71.0), (8.0, 74.6), (17.0, 76.3), (29.0, 76.8),
        (44.0, 76.4), (58.0, 76.5), (70.0, 77.0), (76.0, 76.6), (81.0, 74.7),
        (85.0, 70.4), (88.0, 63.0), (90.0, 53.0), (90.0, 43.5), (87.0, 36.0),
        (81.0, 31.2), (73.0, 29.5), (60.0, 29.0), (46.0, 29.2), (31.0, 29.6),
        (19.0, 31.5), (10.0, 35.0), (4.0, 41.0), (1.6, 50.0), (0.8, 59.0)]
NECK = [(75.0, 64.0), (77.0, 74.0), (81.0, 82.5), (87.0, 88.3), (94.0, 92.2),
        (99.2, 93.0), (103.6, 90.5), (106.0, 85.0), (106.4, 78.0), (104.8, 70.2),
        (101.5, 62.0), (97.4, 52.0), (92.8, 43.0), (86.0, 41.0), (79.0, 46.0), (75.5, 55.0)]
BREG = Polygon(dense(BODY, True, 8)).buffer(0)
NREG = Polygon(dense(NECK, True, 8)).buffer(0)


def fleece_color(p, neck=False):
    """Le gris bleu propre : lumière du dos, flanc moyen, ventre sombre.
    L'encolure se fonce progressivement, sans une bavette noire distincte."""
    x, h = p
    c = ramp([(27, "#55585B"), (40, "#777A80"), (55, "#92959B"),
              (67, "#A9ADB4"), (79, "#C4C7CE")], h)
    cape = clamp((x - 73) / 25) * clamp((h - 51) / 30)
    if neck:
        cape = max(cape, clamp((h - 72) / 24) * 0.7)
    c = mix(c, "#3D4145", cape * 0.64)
    if neck:
        c = mix(c, "#252C2F", clamp((h - 79) / 14) * .9)
    return c


def lock_paint(color):
    """Un modelé commun par niveau de gris : le dégradé suit la boîte de
    chaque mèche. Quelques dizaines de définitions, pas une par boucle."""
    rgb = [int(color[i:i + 2], 16) for i in (1, 3, 5)]
    luminance = round((rgb[0] * .22 + rgb[1] * .7 + rgb[2] * .08) / 6) * 6
    color = "#" + "".join(f"{max(0, min(255, luminance + d)):02X}" for d in (-2, 0, 3))
    gid = "meche" + color[1:]
    if gid not in D.ids:
        D.ids.add(gid)
        D.defs.append(f'<radialGradient id="{gid}" cx="35%" cy="25%" r="75%">'
                      f'<stop offset="0" stop-color="{shade(color, 1.06)}"/>'
                      f'<stop offset=".5" stop-color="{color}"/>'
                      f'<stop offset="1" stop-color="{shade(color, .78)}"/></radialGradient>')
    return f"url(#{gid})"


def curl(c, size, angle, color, rng, edge=False):
    """Une boucle en relief, ouverte et oblique, dont le retour se cache
    sous son propre ruban. L'éclat ne suit qu'une portion de la vrille."""
    family = rng.random()
    if family < .48:
        # Petites mèches torsadées superposées, distinctes des anneaux vus
        # de bout : elles changent d'axe et se recourbent à leur extrémité.
        length = size * rng.uniform(1.8, 3.3) * (.62 if edge else 1)
        root = add(c, (0, length * .35))
        out, shadows, lights = ringlet(*root, length, size * rng.uniform(.8, 1.3),
                                      angle * .5, rng, pitch=size * rng.uniform(.6, .92),
                                      bend=rng.uniform(-.5, .5),
                                      hook=rng.choice((-1, 1)) * rng.uniform(1.0, 2.7),
                                      step=.35)
        D.path(cr(out, nd=1), fill=lock_paint(color))
        sw = size * rng.uniform(.08, .18)
        for _, a, q, b in shadows:
            D.path(qstroke(a, q, b, 2), stroke=shade(color, rng.uniform(.48, .68)),
                   sw=sw, so=.8, cap="round")
        for _, a, q, b in lights:
            brightness = clamp(int(color[3:5], 16) / 160, .35, 1)
            hi = mix(color, "#F5F6F8", rng.uniform(.4, .7) * brightness)
            D.path(qstroke(a, q, b, 2), stroke=hi, sw=sw * .8, so=.85, cap="round")
            if rng.random() < .7:
                off = (-.08, .12)
                D.path(qstroke(add(a, off), add(q, off), add(b, off), 2), stroke=hi,
                       sw=.04, so=.65, cap="round")
        # Quelques fibres suivent la torsade en longueur, très proches de
        # sa couleur : la matière reste soyeuse, elle ne devient pas perle.
        ns = (len(out) - 2) // 2
        right, left = out[1:1 + ns], list(reversed(out[-ns:]))
        fibers = []
        for fraction in (.25, .45, .65):
            pts = [lerp(a, b, fraction) for a, b in zip(left, right)][1:-2]
            if len(pts) > 1:
                fibers.append(cr(pts, False, nd=1))
        D.path("".join(fibers), stroke=mix(color, "#E5E8EE", .28), sw=.045,
               so=.42, cap="round")
        return
    rx, ry = size * rng.uniform(.62, 1.0), size * rng.uniform(.8, 1.45)
    phase = rng.uniform(-.5, .5)
    turn = rng.uniform(2.05, 2.9) * math.pi if family < .82 else rng.uniform(1.1, 1.85) * math.pi
    turn *= rng.choice((-1, 1))
    start = rng.uniform(-math.pi, math.pi)
    pts = []
    for i in range(16):
        t = i / 15
        a = start + turn * t
        radius = 1.0 - .76 * t
        # Légère torsion, pas une ellipse parfaite répétée sur le flanc.
        u = rx * radius * math.cos(a) + .1 * rx * math.sin(a * 2 + phase)
        v = ry * radius * math.sin(a)
        pts.append(add(c, rot((u, v), angle)))
    # Un petit paquet de fibres plein, aux bords irréguliers. Les mèches
    # se recouvrent ; le fond ne reste visible qu'au creux des anneaux.
    contour = []
    wobble = rng.uniform(0, 6)
    for j in range(12):
        a = j * math.pi / 6
        r = 1 + .1 * math.sin(a * 3 + wobble) + .055 * math.sin(a * 5 - wobble)
        contour.append(add(c, rot((rx * 1.13 * r * math.cos(a), ry * 1.12 * r * math.sin(a)), angle)))
    if family < .82:
        D.path(cr(contour, True, nd=1), fill=lock_paint(color))
    # La vrille est un ruban de largeur variable, jamais un trait de tuyau
    # régulier : une rive sombre s'enfonce, l'autre accroche la lumière.
    sw = rng.uniform(.27, .68) * size
    D.path(cr(pts, False, nd=1), stroke=shade(color, .58), sw=sw + .15, cap="round")
    D.path(cr([add(p, (-.07, .08)) for p in pts], False, nd=1), stroke=shade(color, 1.03),
           sw=sw * .8, cap="round")
    # Le reflet est un ruban dédoublé : contraste et lustre à grande taille,
    # ton fondu à la taille habituelle d'un compagnon de bureau.
    brightness = clamp(int(color[3:5], 16) / 160, .35, 1)
    hi = mix(color, "#F4F5F8", rng.uniform(.42, .7) * brightness)
    # Éclats discontinus du côté haut-gauche, commun à toute la toison.
    # Pas un anneau blanc entier autour d'un cœur noir.
    runs, run = [], []
    for p in pts:
        v = sub(p, c)
        lit = (-v[0] * .6 + v[1]) / (math.hypot(*v) or 1) > .25
        if lit and rng.random() > .15:
            run.append(add(p, (-.09, .13)))
        else:
            if len(run) > 1:
                runs.append(run)
            run = []
    if len(run) > 1:
        runs.append(run)
    if runs:
        D.path("".join(cr(run, False, nd=1) for run in runs), stroke=hi,
               sw=sw * .18, so=.85, cap="round")
        fibers = []
        for off in ((-.07, .08), (.07, -.08)):
            fibers.extend(cr([add(p, off) for p in run], False, nd=1) for run in runs)
        D.path("".join(fibers), stroke=mix(hi, "#FFFFFF", .2), sw=.035,
               so=.62, cap="round")


def fleece(region, name, neck=False):
    """Semis de Poisson : ni rangées, ni pavage de la surface. Le diamètre
    apparent des anneaux du flanc est de 1,5 à 2,5 cm (fiche § 3.2)."""
    rng = random.Random(1971 if neck else 1920)
    D.path(geom_d(region, simplify=.025), fill="url(#fond)")
    # Semis dense de petites boucles de fond : moins contrastées et jamais
    # détachées de la masse, elles ferment les creux des grandes mèches.
    points = poisson(region.buffer(-.35), .99 if neck else 1.22, rng)
    points.sort(key=lambda p: (-p[1] + .4 * math.sin(p[0]), p[0]))
    for p in points:
        small = neck or p[1] > 69
        size = rng.uniform(.7, .92) if small else rng.uniform(.87, 1.3)
        pack = .07 * math.sin(p[0] / 5.3 + p[1] / 3.7) * math.sin(p[1] / 4.9 - p[0] / 7.9)
        col = shade(fleece_color(p, neck), rng.uniform(.95, 1.08) + pack)
        ang = .4 * math.sin(p[0] / 4.3) + .3 * math.sin(p[1] / 3.7) + rng.uniform(-1.3, 1.3)
        curl(p, size, ang, col, rng)
    # La silhouette est faite de vrilles bien ancrées, petites sur le dos,
    # un peu plus grandes au ventre, sans jarre ni frange de laine longue.
    boundary = list(region.exterior.coords)
    total = region.exterior.length
    s = .0
    while s < total:
        p = region.exterior.interpolate(s)
        q = region.exterior.interpolate((s + .2) % total)
        pos = (p.x, p.y)
        # Les bords enfouis de l'encolure n'ont pas besoin de festons.
        visible = not neck or p.y > 76 or p.x > 98
        if visible:
            c = fleece_color(pos, neck)
            size = rng.uniform(.55, .78) if p.y > 68 or neck else rng.uniform(.72, .95)
            tangent = math.atan2(q.y - p.y, q.x - p.x)
            # Un fond festonné, puis la vrille, toujours solidaires.
            curl(pos, size, tangent + rng.uniform(-.5, .5), c, rng, True)
        s += rng.uniform(.65, 1.75)


def body():
    D.open("body")
    fleece(BREG, "corps")
    D.pivot("body", (46, 30))
    D.close()


def neck():
    D.open("neck")
    fleece(NREG, "encolure", True)
    D.pivot("neck", NECK_BASE)
    D.close()


def sway(i, x):
    """Un feston de boucles, attaché sous le corps : la toison d'automne
    bouge, sans ajouter de longues mèches étrangères au Gotland."""
    D.open(f"sway-{i}")
    shape = [(x - 3.0, 31.4), (x - 2.9, 29.9), (x - 1.7, 28.5), (x, 28.0),
             (x + 1.8, 28.6), (x + 3.1, 30.2), (x + 3.0, 31.4)]
    D.path(cr(shape), fill=fleece_color((x, 30)))
    rng = random.Random(30 + i)
    for dx, h in ((-2, 29.5), (-.7, 28.8), (1.1, 29.0), (2.2, 29.8)):
        curl((x + dx, h), .72, rng.uniform(-2, 2), fleece_color((x, h)), rng)
    D.pivot(f"sway-{i}", (x, 31.4))
    D.close()


def hair(region, rng, count, flow, light, dark):
    """Le poil très ras, couché : aucune hachure ne sort de la forme."""
    if region.is_empty:
        return
    x0, y0, x1, y1 = region.bounds
    D.begin_batch()
    for _ in range(count):
        p = (rng.uniform(x0, x1), rng.uniform(y0, y1))
        if not region.contains(Point(p)):
            continue
        v = mul(flow(p), rng.uniform(.3, .75))
        q = add(p, v)
        if region.contains(Point(q)):
            D.stroke(lstroke(p, q, 2), light if rng.random() < .35 else dark,
                     .055, rng.uniform(.14, .32))
    D.flush()


LEGS = [("leg-back-far", "back", 21.0, True), ("leg-front-far", "front", 75.0, True),
        ("leg-back-near", "back", 11.0, False), ("leg-front-near", "front", 83.0, False)]


def leg(name, kind, x, far):
    """Membres graciles : canon de 3,5 cm, carpe devant, jarret derrière ;
    laine jusqu'au-dessus des articulations, onglons noirs serrés."""
    back = kind == "back"
    pivot = (x + (6 if back else -1), 49 if back else 47)
    knee = JARRET if back else CARPE
    pts = ([(x + 1.0, 50), (x + 9.0, 46), (x + 10.5, 39), (x + 7.6, 31), (x + 1.2, 22),
            (x - .3, 20), (x + .8, 15), (x + 1.2, 7), (x + 2.3, 3), (x + 4.5, .15),
            (x - 1.0, .15), (x - 1.1, 3), (x - 2.2, 7), (x - 2.5, 15), (x - 2.8, 21),
            (x - 1.7, 27), (x + 1.8, 38)] if back else
           [(x - 4, 48), (x + 3.1, 47), (x + 3.2, 38), (x + 2.4, 30), (x + 2.1, 23),
            (x + 1.9, 18.4), (x + 1.4, 11), (x + 1.7, 6), (x + 2.7, 3), (x + 4.5, .15),
            (x - .9, .15), (x - 1.0, 3), (x - 2.0, 6), (x - 2.2, 13), (x - 2.4, 18.4),
            (x - 2.5, 26), (x - 3.5, 36)])
    reg = Polygon(dense(pts, True, 5)).buffer(0)
    D.open(name)
    g = D.lin(name + "-poil", (x - 2.4, 0), (x + 3, 0),
              [(0, "#111310"), (.32, "#343734" if not far else "#232723"),
               (.65, "#242824" if not far else "#151915"), (1, "#111310")])
    D.path(cr(pts), fill=g)
    hair(reg.buffer(-.25), random.Random(int(x)), 400, lambda p: norm((.03, -1)),
         "#6C706B", "#10120F")
    # Court bourrelet laineux au-dessus du carpe/jarret ; le bas reste nu.
    cap = [(x - 3.7, 43), (x + 5.5, 43), (x + 4.2, knee + 5.6),
           (x + 2.1, knee + 4.3), (x + 1.1, knee + 4.9), (x - .3, knee + 4.1),
           (x - 1.5, knee + 5.0), (x - 2.9, knee + 5.1)]
    capreg = Polygon(cap).intersection(reg)
    D.path(geom_d(capreg, simplify=.04), fill=fleece_color((x, 35)))
    rng = random.Random(int(x * 10))
    for p in poisson(capreg.buffer(-.6), 1.3, rng):
        curl(p, rng.uniform(.55, .8), rng.uniform(-2, 2),
             shade(fleece_color((x, 35)), .8 if far else 1), rng)
    for dx in (-1.7, -.6, .5, 1.6):
        p = (x + dx, knee + 5.2 + rng.uniform(-.4, .5))
        if reg.contains(Point(p)):
            curl(p, rng.uniform(.5, .68), rng.uniform(-.6, .6), fleece_color((x, 32)), rng)
    # Les deux onglons touchent exactement le sol ; l'autre doigt dépasse.
    D.path(cr([(x + 1.5, 3.2), (x + 3, 2), (x + 4.9, .05), (x + .5, .05), (x, 1.2)]),
           fill="#0D0F0D")
    hoof = [(x - .5, 3.2), (x + 1.7, 3.1), (x + 2.9, 1.7), (x + 4.3, .05),
            (x - 1.0, .05), (x - 1.3, .8), (x - .9, 2.2)]
    D.path(cr(hoof, t=.65), fill=D.lin(name + "-onglon", (x - 1, 0), (x + 4, 0),
                                     [(0, "#10100F"), (.55, "#373733" if not far else "#252723"),
                                      (1, "#171715")]))
    line([(x + 1.7, 2.8), (x + 2.6, 1.5), (x + 3.8, .25)], "#77766D", .10, .5)
    line([(x - .5, 3.1), (x + .8, 3.25), (x + 1.8, 3.0)], "#74766E", .11, .45)
    D.path(cr(ellipse((x - 1.9, 4.5), .5, .42, -.5, 10)), fill="#141411")
    D.pivot(name, pivot)
    D.close()


def tail():
    """Queue naturellement courte, 12 cm ; la pointe noire est poilue,
    la base laineuse et large est cachée dans la croupe."""
    D.open("tail")
    shape = [(2.2, 71), (-.9, 70), (-2.5, 67), (-3.2, 62.5), (-2.9, 59.8),
             (-1.6, 58.6), (-.6, 61.4), (1.3, 65), (3.8, 68.5)]
    reg = Polygon(dense(shape, True, 6)).buffer(0)
    D.path(cr(shape), fill="url(#fond)")
    rng = random.Random(100)
    for p in poisson(reg.buffer(-.45), 1.0, rng):
        curl(p, .6, rng.uniform(-3, 3), fleece_color(p), rng)
    tip = [(-3.2, 62.2), (-1.1, 61.4), (-1.7, 58), (-2.7, 57.2), (-3.2, 58.7)]
    D.path(cr(tip), fill="#222522")
    hair(Polygon(tip), rng, 90, lambda p: norm((.1, -1)), "#666B64", "#111410")
    D.pivot("tail", (2, 70))
    D.close()


DORSAL = [(-2, 1), (-1.5, -1.1), (0, -2.7), (3.0, -3.1), (6, -2.95),
          (10, -2.6), (14, -2.2), (18, -1.65), (21.8, -.6), (23.1, .5), (23.6, 2), (23.1, 3.9)]
MOUTH = [(23.1, 3.9), (21.5, 4.2), (19.0, 4.8), (16.1, 5.3)]
UNDER = [(15.4, 8.2), (12.8, 9.8), (8.5, 11.3), (4, 11.4), (.2, 10), (-2.2, 7), (-2.6, 3.4), (-2, 1)]
HEAD = HP(DORSAL + MOUTH[1:] + [(15.4, 8.2)] + UNDER[1:-1])
HREG = Polygon(dense(HEAD, True, 6)).buffer(0)
LIP = HP([(19.4, 4.9), (19.3, 3.5), (20.6, 2.65), (22.5, 2.5),
          (23.7, 2.65), (23.4, 3.9), (22, 4.5), (20.2, 5.0)])


def ear(gid, base, direction, far=False):
    """Oreille noire en feuille : moyenne, presque horizontale. L'intérieur
    pigmenté porte quelques poils gris ; il n'est jamais rose."""
    v = norm(direction)
    n = (-v[1], v[0])

    def P(u, w):
        if far:
            u, w = u * .75, w * .78
        return add(base, add(mul(v, u), mul(n, w)))

    shape = [P(0, -.9), P(3, -2.0), P(7.4, -1.9), P(10.0, -.3), P(8.2, 1.4), P(4.8, 2.0), P(1.4, 1.3)]
    D.open(gid)
    D.path(cr(shape), fill=D.lin(gid + "-peau", P(3, 2), P(3, -2),
                                [(0, "#252829"), (.5, "#121414"), (1, "#080909")]))
    D.path(cr([P(1, -.1), P(4, -1.2), P(8.8, -.6), P(7.2, .9), P(4, 1.2)]),
           fill="#2A2B2B" if not far else "#161919")
    line([P(1, -.5), P(4.2, -.7), P(8.5, -.3)], "#5C6060", .14, .55)
    rng = random.Random(54 if far else 55)
    D.begin_batch()
    for _ in range(50):
        u, w = rng.uniform(1.4, 8.0), rng.uniform(-.8, .8)
        D.stroke(qstroke(P(u, w), P(u + .6, w + .2), P(u + 1.1, w + .13), 2), "#727675", .055, .4)
    D.flush()
    line([P(.5, 1), P(4, 1.85), P(7.3, 1.5)], "#6A706F", .13, .45)
    D.pivot(gid, base)
    D.close()


def eye():
    c = HL(5.9, .75)
    shape = [add(c, p) for p in [(-1.6, -.05), (-1.3, .75), (-.55, 1.0), (.45, .85),
                                (1.3, .15), (1.5, -.35), (.6, -.85), (-.5, -.8)]]
    D.open("eyes")
    D.path(cr(shape), fill="#090B0B")
    D.open("pupils")
    D.path(cr(ellipse(c, 1.12, .82)), fill=D.lin("ambre", add(c, (0, 1)), add(c, (0, -1)),
                                                        [(0, "#69502E"), (.45, "#A7894F"), (1, "#C9B785")]))
    rng = random.Random(70)
    for i in range(28):
        a = i * math.pi * 2 / 28
        p = add(c, (.51 * math.cos(a), .43 * math.sin(a)))
        q = add(c, (1.03 * math.cos(a), .73 * math.sin(a)))
        line([p, q], "#E3C889" if i % 3 else "#5C4729", .045, .7)
    D.path(cr([add(c, p) for p in [(-.95, .18), (-.9, -.22), (.9, -.22), (.96, .13), (.4, .22), (-.5, .22)]], t=.4),
           fill="#101716")
    D.pivot("pupils", c)
    D.close()
    D.path(cr(ellipse(add(c, (-.42, .44)), .24, .13, .1, 12)), fill="#E8E9DC", fo=.88)
    line([add(c, p) for p in [(-1.5, .1), (-1.1, .84), (-.2, 1), (.8, .62), (1.4, -.1)]], "#0A0B0A", .3)
    line([add(c, p) for p in [(-1.1, -.5), (-.45, -.78), (.55, -.67), (1.12, -.38)]], "#7A7D77", .09, .45)
    for p in [(-.9, .8), (-.35, .96), (.2, .9)]:
        line([add(c, p), add(c, (p[0] - .22, p[1] + .42))], "#0B0B0B", .11)
    D.pivot("eyes", c)
    D.close()


def head():
    D.open("head")
    ear("ear-far", HL(.6, -1.2), (1.0, .2), True)
    # Mâchoire d'abord : l'intérieur n'apparaît qu'à l'ouverture. Aucun
    # liseré rose ou blanc sur la bouche fermée d'une face noire.
    D.open("jaw")
    inner = HP([(3, 7), (15, 4.9), (22.8, 4), (22.3, 6.3), (14, 8.6), (7, 11.3), (2.7, 10)])
    D.path(cr(inner), fill="#291B1E")
    D.path(cr(HP([(10, 7.6), (18.5, 5.5), (22.3, 5.3), (21.5, 6.1), (14, 8.5)])), fill="#835C61")
    chin = HP([(3, 8), (15.5, 5.1), (23.15, 3.85), (22.7, 5.7), (19.5, 7.8),
               (12.5, 10.5), (7, 12), (3.1, 11.4)])
    D.path(cr(chin), fill=D.lin("menton", HL(10, 7), HL(10, 12),
                               [(0, "#262929"), (.6, "#303333"), (1, "#151818")]))
    D.pivot("jaw", HL(15.8, 5.5))
    D.close()
    D.path(cr(HEAD), fill=D.lin("tete", HL(8, -3), HL(8, 11.3),
                               [(0, "#42484C"), (.28, "#30363B"), (.68, "#202428"), (1, "#141719")]))
    # Le relief satiné reste dans le noir : chanfrein droit, tempe et os
    # sous l'œil, ombre de la joue. Pas de tache grise indépendante.
    for gid, c, rx, ry, color, alpha in [
            ("front", HL(6, -1), 4, 1.5, "#7C858B", .28),
            ("chanfrein", HL(15, -.5), 8, 1.5, "#7E8990", .22),
            ("joue", HL(7, 7), 5, 3.2, "#080B0D", .32),
            ("sous-oeil", HL(8, 3), 3, 1.2, "#7A8387", .15)]:
        g = D.rad(gid, c, ry, [(0, color, alpha), (1, color, 0)], sx=rx / ry,
                  angle=-HEAD_ANGLE)
        D.path(geom_d(HREG, simplify=.03), fill=g)
    hair(HREG.buffer(-.3), random.Random(91), 3600, lambda p: HU, "#48535A", "#1D252B")
    # Truffe nue et humide ; une seule narine de profil, en virgule.
    nose = HP([(20, .2), (22, -.1), (23.3, .6), (23.65, 2.1), (23.1, 3.8),
               (21.6, 4.3), (19.8, 3.8), (20.7, 2.5)])
    D.path(cr(nose), fill=D.lin("truffe", HL(21, -.3), HL(21, 4.1),
                               [(0, "#414646"), (.5, "#292D2C"), (1, "#181B1A")]))
    nostril = HP([(19.7, 1.55), (19.9, .85), (20.4, .45), (21.15, .5),
                   (21.85, 1.25), (21.8, 1.7), (21.3, 1.5), (20.65, 1.2),
                   (20.2, 1.45), (19.95, 1.85)])
    D.path(cr(nostril), fill="#080B0B")
    line(HP([(20.2, .8), (20.8, .48), (21.6, .7)]), "#818783", .12, .7)
    line(HP([(22.3, 2.6), (22.85, 2.8), (23.05, 3.2)]), "#94968C", .10, .6)
    line(HP(MOUTH), "#080C0C", .20)
    line(HP([(17.0, 6.25), (19.6, 5.55), (22, 4.7)]), "#5C6260", .10, .4)
    # La calotte : quelques minuscules vrilles noires, derrière la nuque,
    # laissant entièrement nue la face sous la base des oreilles.
    rng = random.Random(99)
    for _ in range(30):
        p = HL(rng.uniform(-2.0, 2.0), rng.uniform(-2.2, 1.0))
        curl(p, rng.uniform(.3, .5), rng.uniform(-3, 3), "#2B3033", rng)
    # La lisière du cou : de petites vrilles noires recouvrent seulement
    # le bord arrière du crâne, pas la face. Elles fondent la joue dans le
    # poil court et la laine sombre de la nuque.
    for j in range(16):
        p = HL(-1.8 + rng.uniform(-.35, .25), 1.3 + j * .48)
        curl(p, rng.uniform(.22, .38), rng.uniform(-2, 2), "#293236", rng)
    eye()
    ear("ear-near", HL(.3, -.9), (-1, .08))
    D.pivot("head", POLL)
    D.pivot("muzzle", HL(22.6, 4.8))
    D.close()


def articulate(svg):
    """Disques de recouvrement aux carpes/jarrets ; petite lèvre de 17 %
    de la longueur de tête, bourrelet rose sombre entièrement caché."""
    root = ET.fromstring(svg, parser=ET.XMLParser(target=ET.TreeBuilder(insert_comments=True)))
    paints = A.Paints(root)
    for name, kind, _, _ in LEGS:
        joint, half = A.leg_joint(svg, name, OY - (CARPE if kind == "front" else JARRET))
        A.split_leg(root, paints, name, joint, half + .4)
    pad = HP([(19.7, 4.3), (19.8, 3.6), (20.9, 3.1), (22.3, 3), (23, 3.25), (22.6, 3.8), (21.2, 4.25)])
    ridge = HP([(20.2, 4.05), (21.3, 3.8), (22.45, 3.4)])
    gum = A.gum([S(p) for p in pad], [S(p) for p in ridge], "#82545C", "#4E2F37", .17)
    A.lift_part(root, paints, "head", [S(p) for p in LIP], "lip", S(HL(19.7, 3.8)), under=gum)
    ET.indent(root, space="  ")
    return ET.tostring(root, encoding="unicode") + "\n"


def main(path):
    D.lin("fond", (0, 79), (0, 27), [(0, "#9499A0"), (.4, "#676D73"), (1, "#343B3F")])
    for args in LEGS:
        leg(*args)
    tail()
    neck()
    for i, x in enumerate((36, 50, 64), 1):
        sway(i, x)
    body()
    head()
    D.pivot("ground", (49, 0))
    comment = ("Gotland (Gotlandsfår), brebis adulte en pleine toison d'automne.\n"
               "70 cm au garrot sous la laine ; une unité = un centimètre.\n"
               "Anneaux gris bleu lustrés, face et pattes noires nues, sans cornes, queue courte.\n"
               "D'après docs/races/gotland.md ; écrit par tools/races/gotland/gen_gotland.py.\n"
               "© 2026 Alejandro San Martin Lamas — MIT OR Apache-2.0")
    svg = articulate(D.render(136, 104, comment))
    if VARIANTE:
        return variante(path, svg)
    Path(path).parent.mkdir(parents=True, exist_ok=True)
    Path(path).write_text(svg, encoding="utf-8")
    print(f"{path} : {len(svg.encode()) / 1024:.0f} Ko, {svg.count('<path')} tracés")


def variante(path, svg):
    """Les autres dessins : ce qui change, puis les proportions."""
    root = ET.fromstring(svg)
    if VARIANTE == "belier":
        variantes.bourses(root, *variantes.sous_le_ventre(root, 6, -5), largeur=9, hauteur=15,
                  peau="#5B6066", ombre="#262A2E")
    if VARIANTE == "agneau":
        # Né noir : il ne grisonne qu'entre un et deux mois (§ 1.6).
        variantes.recolorer(root, variantes.rampe("#0C0D0F", "#4A4C52", bas=.1, haut=.8),
                    dans=("body", "neck", "tail", "sway-1", "sway-2", "sway-3",
                          "leg-front-near", "leg-front-far", "leg-back-near", "leg-back-far"))
    variantes.ecrire(path, root, **PROPORTIONS[VARIANTE])


if __name__ == "__main__":
    main(sys.argv[1] if len(sys.argv) > 1 else str(HERE.parents[2] / "pets/gotland/gotland.svg"))
