#!/usr/bin/env python3
"""Bélier Skudde blanc adulte, en toison d'automne, de profil droit.

Une unité vaut un centimètre ; h monte depuis le sol. Mesures, choix de
couleur et anatomie : docs/races/skudde.md. Les longs poils sont lisses,
effilés et légèrement ondulés : ce ne sont pas des boucles de laine.
La crinière couvre le cou ET l'épaule ; la tête et les pattes restent rases.

Usage : python3 tools/races/skudde/gen_skudde.py pets/skudde/skudde.svg
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
# Sans variante : le bélier.
VARIANTE = sys.argv[2] if len(sys.argv) > 2 else ""
PROPORTIONS = {
    "brebis": dict(  # Brebis Skudde : 52 cm au garrot, de petits moignons noirâtres devant l'oreille, sans crinière, tête fine et droite.
        garrot=(58, 52), encolure=(1.0, .86), tete=(.96, .93)),
    "agneau": dict(  # Agneau Skudde de deux semaines : 31 cm au garrot, boucles serrées, marque rouille aux pattes, sans cornes.
        variantes.AGNEAU, garrot=(58, 31), corps=(.84, .8)),
}
from shapely.geometry import LineString
from outils import (Doc, OY, S, Polygon, Point, cr, dense, poly, geom_d,  # noqa: E402
                    poisson, mix, shade, ramp, qstroke, lstroke, lerp, norm)

doc = Doc()
rng = random.Random(1941)  # souche conservée à Munich (fiche § 10.3)
GARROT, CARPE, JARRET = 58.0, 15.5, 18.5
POLL = (85.0, 69.0)
NECK_BASE = (73.0, 42.5)
TILT = math.radians(37)


def HL(u, v):
    """Repère de la tête : u vers le nez, v vers le dessous de la joue."""
    return (POLL[0] + u * math.cos(TILT) - v * math.sin(TILT),
            POLL[1] - u * math.sin(TILT) - v * math.cos(TILT))


def HLs(pts):
    return [HL(*p) for p in pts]


def ellipse(c, rx, ry, angle=0, n=24):
    return [(c[0] + rx * math.cos(i * 2 * math.pi / n) * math.cos(angle)
             - ry * math.sin(i * 2 * math.pi / n) * math.sin(angle),
             c[1] + rx * math.cos(i * 2 * math.pi / n) * math.sin(angle)
             + ry * math.sin(i * 2 * math.pi / n) * math.cos(angle)) for i in range(n)]


BODY = [(1, 53), (3, 59), (9, 63), (19, 64.5), (32, 64.8), (46, 64.6),
        (59, 65), (66, 65.5), (71, 62), (76, 55), (79, 45), (78, 35),
        (72, 29), (61, 27), (48, 26.5), (35, 27), (26, 29), (20, 33),
        (13, 35), (7, 36), (3, 40), (1, 46)]
NECK = [(56, 49), (57, 58), (62, 64), (68, 68), (74, 71), (81, 72),
        (86, 70), (89, 64), (91, 57), (91, 50), (88, 41), (83, 34),
        (77, 27.5), (69, 30), (62, 35), (57, 43)]
BP = Polygon(dense(BODY, True, 8)).buffer(0)
NP = Polygon(dense(NECK, True, 8)).buffer(0)

WOOL = [(0, "#8C7A62"), (.25, "#B9A88A"), (.55, "#D5C5A8"), (1, "#EDE2CD")]
MANE = [(0, "#9C8C73"), (.4, "#CFC3AC"), (1, "#F0E8D6")]


def light_at(x, h, mane=False):
    """Volumes doux, en paquets irréguliers : aucune trame en colonnes."""
    v = .03 + .92 * max(0, min(1, (h - 24) / 43))
    v += .14 * math.sin(x / 5.2 + h / 4.1) * math.sin(h / 3.8 - x / 7.5)
    if mane:
        v += .13 - .22 * max(0, (x - 82) / 10)
    return max(0, min(1, v))


def lock(x, h, length, width, bend, light, mane=False):
    """Mèche de longs poils, plusieurs pointes fines au bout d'un fuseau.
    Racine et dessus seront recouverts par les mèches plus hautes.
    Le modelé est discret, les fibres restent plus visibles que le contour.
    """
    col = ramp(MANE if mane else WOOL, light)
    wave = rng.uniform(.35, .8) * min(1, length / 8) * (-1 if rng.random() < .5 else 1)
    phase = rng.uniform(-.3, .3)

    def center(t):
        # Une légère vague en S, jamais une boucle ni un crochet régulier.
        w = wave * math.sin(t * math.pi * 2 + phase) * math.sin(t * math.pi)
        return (x + bend * t + w, h - length * t)

    def side(t, sign):
        p = center(t)
        taper = max(0, 1 - t) ** .65
        return (p[0] + sign * width * .5 * taper, p[1])

    ts = [0, .15, .3, .48, .66, .82, .94, 1]
    tip = center(1)
    outline = [side(t, -1) for t in ts] + [side(t, 1) for t in ts[-2::-1]] + [(x, h + width * .22)]
    # Modelé transversal doux : un creux sombre, une face ivoire et son
    # retour dans l'ombre. Les mèches partagent quelques dégradés.
    level = round(max(0, min(1, light)) * 14)
    gid = f"fibre-{'criniere' if mane else 'flanc'}-{level}"
    if gid not in doc.ids:
        doc.ids.add(gid)
        base = ramp(MANE if mane else WOOL, level / 14)
        doc.defs.append(f'<linearGradient id="{gid}" x1="0" y1="0" x2="1" y2="0">'
                        f'<stop offset="0" stop-color="{shade(base, .75)}"/>'
                        f'<stop offset=".35" stop-color="{base}"/>'
                        f'<stop offset=".64" stop-color="{shade(base, 1.11)}"/>'
                        f'<stop offset="1" stop-color="{shade(base, .88)}"/></linearGradient>')
    doc.path(cr(outline, t=.7), fill=f"url(#{gid})")
    # Le cœur de chaque paquet reçoit une ombre diffuse, en dégradé peint.
    for frac in (-.32, -.1, .14, .33):
        pts = []
        start = rng.uniform(.05, .23)
        end = rng.uniform(.9, 1.04)
        for k in range(9):
            t = start + (end - start) * k / 8
            p = center(t)
            pts.append((p[0] + width * frac * max(0, 1 - t) ** .6, p[1]))
        doc.path(cr(pts, False, t=.85),
                 stroke=shade(col, .68 if frac < 0 else 1.17),
                 sw=.075 if mane else .065, so=.32 if frac < 0 else .55, cap="round")
    # Fins jarres indépendants et de longueurs variées au bout du paquet.
    if rng.random() < .7:
        pts = [center(t) for t in (.28, .45, .65, .82, 1.05)]
        doc.path(cr(pts, False), stroke=shade(col, 1.15), sw=.045, so=.75, cap="round")


def fleece(gid, shape, mane=False):
    doc.open(gid)
    if gid == "body" and not VARIANTE:
        # Scrotum discret du bélier, sous la frange entre les postérieurs.
        doc.path(cr([(18, 36), (21, 36), (21.2, 32), (20, 30.8), (18.5, 32)]),
                 fill=doc.lin("scrotum", (18, 28), (21.2, 28), [(0,"#9E9381"), (.4,"#D0C3AE"), (1,"#9E9381")]))
    # Le duvet sous les longs poils, en retrait des pointes.
    fill = doc.lin(f"duvet-{gid}", (0, 68), (0, 23),
                   [(0, "#E4D8C1"), (.45, "#CDBB9A"), (1, "#978167")])
    doc.path(geom_d(shape.buffer(-.6)), fill=fill)
    roots = poisson(shape.buffer(-.8), 1.4 if mane else 1.5, rng)
    roots.sort(key=lambda p: (p[1], p[0]))
    for x, h in roots:
        # La crinière du bélier : la brebis et l'agneau ont à l'encolure la
        # laine du corps (§ 1.4).
        L = rng.uniform(10.5, 14.2) if mane and not VARIANTE else rng.uniform(7.2, 10.0)
        # Empêcher les mèches de transformer la toison d'automne en robe
        # hivernale ; une frange irrégulière dépasse de 0,5 à 3 cm.
        cut = shape.intersection(LineString([(x, 0), (x, 100)]))
        bottom = cut.bounds[1]
        L = min(L, h - bottom + rng.uniform(.5, 3.0))
        if L < 1.6:
            continue
        # Sous la croupe, laisser paraître la cuisse et le jarret ras.
        if not mane and x < 20:
            L *= .78
        # La longueur et le sens varient par paquets, sans rangées régulières.
        bend = 1.9 * math.sin(x / 5.3 + h / 9) + rng.uniform(-1.2, 1.2)
        if not mane and x < 8:
            bend -= .7
        if mane:
            bend += 1.0
        lock(x, h, L, rng.uniform(1.45, 3.1), bend,
             light_at(x, h - L * .4, mane) + rng.uniform(-.055, .055), mane)
    if gid == "body":
        # La crinière déborde sur la pointe de l'épaule. Ces paquets suivent
        # le tronc, tandis que ceux de la gorge suivent l'encolure.
        shoulder = Polygon([(59,60),(67,64),(73,61),(77,52),(76,40),(69,38),(63,44)])
        for x,h in sorted(poisson(shoulder, 1.8, rng), key=lambda p:p[1]):
            lock(x,h,rng.uniform(10.8,14.5),rng.uniform(1.4,2.5),rng.uniform(.4,1.8),
                 light_at(x,h,True)-.04,True)
    # De petites mèches en biais épousent le dessus et masquent les racines
    # du rideau, sans faire de ligne peignée ou de boucles de Mérinos.
    if gid == "body":
        for x in range(5, 69):
            if rng.random()<.2:
                continue
            xx=x+rng.uniform(-.45,.45)
            cut=shape.intersection(LineString([(xx,0),(xx,100)]))
            hh=cut.bounds[3]-rng.uniform(.3,1.2)
            lock(xx,hh,rng.uniform(1.7,3.4),rng.uniform(.65,1.4),rng.uniform(-1.2,.5),
                 light_at(xx,hh)+rng.uniform(-.08,.08))
    # Le dessus doit rester hirsute sans faire de longues pointes vers le ciel.
    rim = []
    pts = list(shape.exterior.coords)
    for i, (x, h) in enumerate(pts[::3]):
        if h < (62 if not mane else 67):
            continue
        L = rng.uniform(.4, 1.1)
        rim.append(qstroke((x, h - .25), (x - .3, h + L * .5), (x + .2, h + L), 2))
    doc.path("".join(rim), stroke="#EADFCB", sw=.08, so=.8, cap="round")
    doc.pivot(gid, NECK_BASE if mane else (41, 28))
    doc.close()


def hoof(cx, far):
    """Onglons gris-brun, petits et serrés, fente et quelques stries de pousse."""
    k = .82 if far else 1
    g = doc.lin(f"sabot-{cx}", (cx - 1.8, 1), (cx + 2.5, 1),
                [(0, shade("#3A2E26", k)), (.35, shade("#7A6858", k)),
                 (.72, shade("#A89886", k)), (1, shade("#5E4C3E", k))])
    pts = [(cx - 1.55, 2.3), (cx + 1.25, 2.6), (cx + 2.45, .28),
           (cx + 2.3, 0), (cx - 1.65, 0), (cx - 1.9, .65)]
    doc.path(cr(pts, t=.5), fill=g)
    doc.path(cr([(cx + .25, .1), (cx + .12, .9), (cx - .05, 1.8)], False),
             stroke="#3A2E26", sw=.13, so=.9, cap="round")
    for t in (.33, .65):
        doc.path(cr([(cx - 1.4, 2.1 * t), (cx, 2.4 * t), (cx + 1.7, 1.9 * t)], False),
                 stroke=shade("#A89886", k), sw=.065, so=.5)


def leg(gid, cx, front, far=False):
    """Patte fine, poil ras ; le haut rentre largement dans la toison."""
    doc.open(gid)
    if front:
        pts = [(cx - 3.4, 43), (cx + 3.5, 43), (cx + 2.4, 31), (cx + 1.65, 22),
               (cx + 1.7, 16), (cx + 1.2, 13), (cx + 1.05, 6), (cx + 1.7, 2),
               (cx - 1.65, 2), (cx - 1.8, 4.6), (cx - 1.4, 7), (cx - 1.5, 13),
               (cx - 1.85, 16), (cx - 1.7, 18), (cx - 2.6, 28)]
        pivot = (cx, 41)
    else:
        pts = [(cx - 1.8, 47), (cx + 7, 47), (cx + 9, 39), (cx + 7, 33),
               (cx + 4.4, 28), (cx + 1.5, 21), (cx + 1.3, 18), (cx + 1.25, 7),
               (cx + 1.65, 2), (cx - 1.6, 2), (cx - 1.8, 4.5), (cx - 1.3, 7),
               (cx - 1.4, 16), (cx - 2.15, 19), (cx - 2.1, 22), (cx - .8, 30)]
        pivot = (cx + 3.5, 44)
    k = .83 if far else 1
    g = doc.lin(f"poil-{gid}", (cx - 2.2, 15), (cx + 3, 15),
                [(0, shade("#9F938C", k)), (.25, shade("#E6DCD5", k)),
                 (.6, shade("#D6C9C0", k)), (1, shade("#A69991", k))])
    doc.path(cr(pts, t=.65), fill=g)
    shape = Polygon(dense(pts, True, 6)).buffer(-.3)
    # Le tendon et le pli du carpe/jarret n'ont pas de contour épais.
    hh = CARPE if front else JARRET
    doc.path(cr([(cx - 1.2, hh + 2), (cx - 1.45, hh), (cx - .8, hh - 1.4)], False),
             stroke=shade("#8E827B", k), sw=.15, so=.5)
    doc.begin_batch()
    for x, h in poisson(shape, .65, rng):
        L = rng.uniform(.3, .8)
        end = (x + (.14 if front or h < JARRET else -.25), h - L)
        if shape.contains(Point(end)):
            doc.stroke(qstroke((x, h), ((x + end[0]) / 2 + .06, h - L * .5), end),
                       shade("#EEE5DC" if rng.random() < .7 else "#9B8E83", k), .065, .5)
    doc.flush()
    # Ergots derrière le boulet.
    doc.path(cr([(cx - 1.6, 4.8), (cx - 2.1, 4.2), (cx - 1.9, 3.8), (cx - 1.4, 4)]),
             fill=shade("#5E4C3E", k))
    hoof(cx, far)
    doc.pivot(gid, pivot)
    doc.close()


def tail():
    """Queue de 18 cm : triangle large à la racine, pointe au-dessus du jarret."""
    doc.open("tail")
    pts = [(3, 58), (-1.6, 57), (-3.1, 51), (-2.5, 45), (-1.4, 40.5),
           (-.9, 40.8), (.2, 46), (2.7, 52)]
    g = doc.lin("queue", (-3, 57), (0, 40), [(0, "#DDD1BA"), (.4, "#C9B9A0"), (1, "#E0D6C5")])
    doc.path(cr(pts, t=.65), fill=g)
    doc.begin_batch()
    region = Polygon(dense(pts, True, 5)).buffer(-.2)
    for x, h in poisson(region, .45, rng):
        end = (x + rng.uniform(-.2, .3), h - rng.uniform(.55, 1.15))
        if region.contains(Point(end)):
            doc.stroke(qstroke((x, h), (x - .2, (h + end[1]) / 2), end),
                       "#F0E8D9" if rng.random() < .6 else "#A69377", .08, .65)
    doc.flush()
    doc.pivot("tail", (2, 56))
    doc.close()


HORN = [(89.3, 67.6), (84, 72.1), (75, 73.2), (69, 70.7), (65.5, 65),
        (65.3, 59), (68.2, 53.8), (73.3, 51.8), (79.2, 52.3), (84.2, 55.3),
        (86.9, 59.5), (86.7, 62.1), (85.2, 63.6)]
# L'axe est resserré latéralement autour de sa racine, puis ses normales
# et sa section de 6,7 cm sont recalculées dans horn(). La bobine n'est pas
# un cercle d'Ouessant : sa boîte hors tout mesure 20,63 × 26,07 cm
# (35,6 × 44,9 % des 58 cm au garrot ; fiche § 2.3 et § 6).
HORN = [(89.3 + (x - 89.3) * .64, h) for x, h in HORN]



def horn(far=False):
    """Corne en colimaçon, miel ambré, annelures en travers et arête éclairée.
    Le dernier quart se relève devant la joue, peu écarté latéralement.
    """
    axis = dense(HORN, False, 9)
    if VARIANTE == "brebis":
        # De petits moignons de 3 à 5 cm (§ 1.4).
        axis = axis[:14]
    if far:
        axis = [(x - .45, h + .8) for x, h in axis[:42]]
    n = len(axis)
    outer, inner, normals, widths = [], [], [], []
    for i, p in enumerate(axis):
        a, b = axis[max(0, i - 1)], axis[min(n - 1, i + 1)]
        tx, th = norm((b[0] - a[0], b[1] - a[1]))
        normal = (-th, tx)
        # Largeur décroissante ; pointe lisse sur les 6 derniers cm.
        u = i / (108 if far else n - 1)
        w = 6.7 * max(0, 1 - u) ** .85 + .05
        if VARIANTE == "brebis":
            w = 2.6 * max(0, 1 - i / (n - 1)) ** .7 + .05
        if far and i > n-12:
            w *= (n-1-i)/11
        widths.append(w)
        normals.append(normal)
        outer.append((p[0] + normal[0] * w / 2, p[1] + normal[1] * w / 2))
        inner.append((p[0] - normal[0] * w / 2, p[1] - normal[1] * w / 2))
    tag = "lointaine" if far else "proche"
    g = doc.lin(f"corne-{tag}", (69, 74), (85, 53),
                [(0, "#D2BC9C"), (.28, "#AD885D"), (.6, "#795638"), (1, "#AC7F49")])
    doc.path(poly(outer + inner[::-1]), fill=g)
    # Le contour suit les côtés, sans tracer une tranche fermée à la
    # racine : celle-ci se perd dans les poils courts du crâne.
    doc.path(poly(outer[1:] + inner[:0:-1], False), stroke="#5A422B", sw=.13, so=.8)
    # Deux faces et une arête donnent à la corne sa section triangulaire.
    side = [(p[0] + normals[i][0] * widths[i] * .08,
             p[1] + normals[i][1] * widths[i] * .08) for i, p in enumerate(axis)]
    doc.path(poly(outer + side[::-1]), fill="#E1C7A1", fo=.22)
    doc.path(poly(side + inner[::-1]), fill="#4A3826", fo=.16)
    ann_dark, ann_light = [], []
    acc = 0
    for i in range(1, n - 2):
        acc += math.dist(axis[i], axis[i - 1])
        if acc < rng.uniform(.42, .73) or (not far and i > n * .82):
            continue
        acc = 0
        p, nn, w = axis[i], normals[i], widths[i]
        tangent = norm((axis[i + 1][0] - axis[i - 1][0], axis[i + 1][1] - axis[i - 1][1]))
        a = (p[0] + nn[0] * w * .48, p[1] + nn[1] * w * .48)
        b = (p[0] - nn[0] * w * .48, p[1] - nn[1] * w * .48)
        c = (p[0] + tangent[0] * .45, p[1] + tangent[1] * .45)
        ann_dark.append(qstroke(a, c, b, 2))
        aa = (a[0] + tangent[0] * .15, a[1] + tangent[1] * .15)
        bb = (p[0] + tangent[0] * .15, p[1] + tangent[1] * .15)
        ann_light.append(qstroke(aa, (c[0] + tangent[0] * .15, c[1] + tangent[1] * .15), bb, 2))
    doc.path("".join(ann_dark), stroke="#4A3826", sw=.09, so=.54, cap="round")
    doc.path("".join(ann_light), stroke="#E0C9A6", sw=.08, so=.65, cap="round")
    doc.path(cr(outer[2:-2:2], False), stroke="#D2BC9C", sw=.18, so=.65)
    for frac in (-.3, -.1, .13, .31):
        pts = [(p[0] + normals[i][0] * widths[i] * frac,
                p[1] + normals[i][1] * widths[i] * frac) for i, p in enumerate(axis)]
        doc.path(cr(pts[2:-2:2], False), stroke="#4A3826" if frac < 0 else "#DBC09A", sw=.07, so=.32)


HEAD = [(-1, -1), (3, -2), (7, -2.4), (11, -2.1), (15, -1.5), (19, -.5),
        (22, 1), (23, 2.6), (22.6, 4), (20.8, 4.9), (18, 5.2), (14, 5.6),
        (10, 8.6), (6, 10), (1, 8.3), (-2, 4.8)]


def ear(gid, far=False):
    base = (84.1, 65.6) if not far else (85.4, 68.2)
    x, h = base
    doc.open(gid)
    scale = .48 if far else 1
    E = lambda u,v: (x+u*scale,h+v*scale)
    # Feuille courte, une attache serrée, pointe arrondie mais non ovale.
    pts = [E(.25,0),E(-1.2,1.05),E(-3.7,1.35),E(-5.8,.8),E(-6.6,.2),
           E(-5.45,-.45),E(-3.0,-.9),E(-.8,-.5)]
    outer = doc.rad(f"{gid}-dehors",E(-3.4,.65),3.7*scale,
                    [(0,"#EEE9E0"),(.5,"#DAD3C8"),(1,"#A9A39C")])
    doc.path(cr(pts,t=.75), fill=outer)
    cup = [E(-.6,.03),E(-2.0,.65),E(-4.4,.55),E(-5.6,.17),E(-3.4,-.48),E(-1.1,-.3)]
    inner = doc.rad(f"{gid}-dedans",E(-2.5,-.15),2.7*scale,
                    [(0,"#9F8577"),(.45,"#C3A99A"),(1,"#E0D1BF")])
    doc.path(cr(cup),fill=inner)
    doc.path(cr([E(-.55,.3),E(-2.4,1.1),E(-4.3,.95),E(-6.1,.25)],False),
             stroke="#EBE5DB",sw=.16,so=.85,cap="round")
    # Petits poils blancs à l'entrée et sur le bord ; base sous la laine.
    doc.begin_batch()
    for k in range(18 if not far else 9):
        t=k/17
        p=E(-.7-5.2*t,.25+.65*math.sin(t*math.pi))
        end=(p[0]-.2*scale,p[1]-.32*scale)
        doc.stroke(qstroke(p,(p[0]-.12,p[1]-.1),end,2),"#F1EADF",.055,.65)
    doc.flush()
    doc.pivot(gid,base)
    doc.close()


def head():
    doc.open("head")
    if VARIANTE != "agneau":
        horn(True)
    ear("ear-far", True)
    # La mâchoire est la première pièce sous le crâne ; le fond rose
    # n'apparaît que quand le moteur ouvre la bouche.
    doc.open("jaw")
    doc.path(cr(HLs([(8, 4.2), (15, 3.4), (21.6, 3.8), (21.6, 5.5), (18, 7), (11, 9), (6, 8)])), fill="#7F5552")
    doc.path(cr(HLs([(9, 5.3), (16, 5), (22.3, 4.8), (22.4, 5.9), (18, 7), (11, 9), (7, 8)])),
             fill=doc.lin("machoire", HL(10, 5), HL(14, 9), [(0, "#D4CFC6"), (1, "#ACA59A")]))
    doc.path(cr(HLs([(16.2, 5.35), (20, 5.05), (22, 5)]), False), stroke="#6F6666", sw=.13)
    doc.pivot("jaw", HL(7, 5.5))
    doc.close()
    headpoly = Polygon(dense(HLs(HEAD), True, 6)).buffer(0)
    face = doc.lin("face", HL(6, -2), HL(11, 10),
                   [(0, "#E8E6E0"), (.5, "#D9D5CE"), (1, "#A3A099")])
    doc.path(cr(HLs(HEAD), t=.72), fill=face)
    # Modelé du chanfrein, masséter et sillon lacrymal, par dégradés peints.
    for tag, c, rx, ry, color, alpha in [
            ("joue", HL(7, 5), 4.2, 3.2, "#F0EDE6", .45),
            ("chanfrein", HL(13, -.5), 6.2, 1.3, "#F2EFE9", .55),
            ("orbite", HL(9.3, 2.5), 2.1, 1.3, "#8E8A85", .3)]:
        geom = Polygon(ellipse(c, rx, ry, -TILT)).intersection(headpoly)
        g = doc.rad(tag, c, rx, [(0, color, alpha), (1, color, 0)], sx=1)
        doc.path(geom_d(geom), fill=g)
    doc.begin_batch()
    for x, h in poisson(headpoly.buffer(-.3), .4, rng):
        L = rng.uniform(.22, .46)
        e = (x + L * .75, h - L * .65)
        if headpoly.contains(Point(e)):
            doc.stroke(lstroke((x, h), e, 2), "#F0ECE5" if rng.random() < .72 else "#99948E", .05, .5)
    doc.flush()
    # Petites taches irrégulières de pigment, sans masque ni grand aplat.
    for u, v, rr in [(12, .8, .35), (16.4, 1, .25), (18.2, 2.8, .3), (19.6, 3.6, .22),
                     (15.3, 4.3, .17), (11, 6.8, .24), (17.8, -.15, .17)]:
        p = HL(u, v)
        pts = [(p[0] + rr * rng.uniform(.65, 1.2) * math.cos(i * math.pi / 4),
                p[1] + rr * rng.uniform(.5, 1) * math.sin(i * math.pi / 4)) for i in range(8)]
        doc.path(cr(pts), fill="#2E2B2E", fo=.85)
    # Le nez gris ardoise, une virgule sombre pour la narine.
    doc.path(cr(HLs([(20.6, .1), (22.3, 1), (23, 2.6), (22.7, 4), (21.5, 4.7),
                    (20.5, 3.4), (20.1, 1.4)])),
             fill=doc.lin("truffe", HL(20, 0), HL(23, 4), [(0, "#59575A"), (.6, "#3A3A40"), (1, "#25252A")]))
    doc.path(cr(HLs([(21.1, 1.4), (22, 1.7), (22.1, 2.7), (21.6, 3.4), (21.65, 2.4), (20.9, 2.2)])),
             fill="#141416")
    doc.path(cr(HLs([(21, 1.1), (21.8, 1.1), (22.3, 1.65)]), False), stroke="#989397", sw=.1, so=.7)
    doc.path(cr(HLs([(22.65, 3.4), (22.3, 4.2), (21.8, 4.7), (19.8, 5)]), False),
             stroke="#302A2C", sw=.14)
    # L'œil reste une amande petite, jamais un grand œil de personnage.
    ec = HL(7.7, 1.9)
    ex, eh = ec
    almond = [(ex - 1.3, eh), (ex - .55, eh + .59), (ex + .6, eh + .5),
              (ex + 1.35, eh - .05), (ex + .55, eh - .5), (ex - .6, eh - .46)]
    doc.open("eyes")
    doc.path(cr(almond), fill="#81766C")
    doc.path(cr([(ex-1.45,eh),(ex-.6,eh+.7),(ex+.7,eh+.6),(ex+1.55,eh-.05)],False),
             stroke="#4D443E",sw=.12,so=.8,cap="round")
    doc.open("pupils")
    doc.path(cr(ellipse(ec, .83, .63)), fill="#B39A5A")
    doc.path(cr(ellipse((ex, eh + .2), .83, .32)), fill="#665A38", fo=.45)
    doc.path(cr([(ex - .65, eh + .14), (ex + .62, eh + .14), (ex + .7, eh - .07),
                 (ex + .54, eh - .22), (ex - .58, eh - .22), (ex - .72, eh - .04)], t=.5), fill="#0B0B0D")
    doc.pivot("pupils", ec)
    doc.close()
    doc.path(cr(ellipse((ex - .37, eh + .3), .13, .1)), fill="#EEEAE1", fo=.8)
    doc.path(cr([(ex - 1.3, eh + .22), (ex - .4, eh + .75), (ex + .7, eh + .6)], False),
             stroke="#E2DED8", sw=.13, so=.9)
    doc.pivot("eyes", ec)
    doc.close()
    doc.path(cr(ellipse((83.8,63.8),3.2,3.5)),
             fill=doc.rad("oreille-attache",(83.8,63.8),3.5,[(0,"#7A6A55",.4),(1,"#7A6A55",0)]))
    ear("ear-near")
    if VARIANTE != "agneau":
        horn()
    # La racine de la corne rentre dans le crâne : une petite pièce de peau
    # rase recouvre son terminal, et non une tranche de corne plaquée dessus.
    root_skin = [(86.6,64.8),(88.1,66.8),(89.8,68.9),(91.55,70.5),
                 (92.2,69.8),(93.1,68.0),(91.8,65.4),(89.1,63.8)]
    doc.path(cr(root_skin,t=.65),fill=face)
    root_poly=Polygon(dense(root_skin,True,6)).buffer(-.15)
    doc.begin_batch()
    for x,h in poisson(root_poly,.28,rng):
        e=(x+.24,h-.18)
        if root_poly.contains(Point(e)):
            doc.stroke(lstroke((x,h),e,2),"#EEE9E0" if rng.random()<.7 else "#B4ADA3",.055,.65)
    doc.flush()
    # De très courts poils chevauchent irrégulièrement la naissance de la
    # corne et en interrompent le bord, sans constituer un toupet.
    for k in range(20):
        t=k/19
        p=(87.1+4.4*t,65.0+5.1*t)
        p=(p[0]+rng.uniform(-.12,.12),p[1]+rng.uniform(-.12,.12))
        doc.path(qstroke((p[0]+.25,p[1]-.1),(p[0]+.1,p[1]+.1),(p[0]-.32,p[1]+.25),2),
                 stroke="#DED8CD" if k%3 else "#AFA99D",sw=.11,so=.9,cap="round")
    # Racine de la corne noyée dans la calotte : pas une tranche carrée.
    cap = Polygon([(83, 69.4), (86, 70.8), (89, 70), (90, 68.8), (87.8, 67.2), (84, 67.7)])
    for x, h in sorted(poisson(cap, .5, rng), key=lambda p:p[1]):
        lock(x, h, rng.uniform(.65, 1.1), .65, -.15, .85)
    # Calotte de laine courte sur la nuque seulement, face bien dégagée.
    for x, h in sorted(poisson(Polygon([(80, 70), (85.2, 70), (87, 67.8), (83, 68)]), .7, rng), key=lambda p:p[1]):
        lock(x, h, rng.uniform(.7, 1.2), .65, -.3, .8)
    doc.pivot("head", POLL)
    doc.pivot("muzzle", HL(22.4, 5))
    doc.close()


def articulate(text):
    parser = ET.XMLParser(target=ET.TreeBuilder(insert_comments=True))
    root = ET.fromstring(text, parser=parser)
    paints = A.Paints(root)
    for name, hh in [("leg-front-near", CARPE), ("leg-front-far", CARPE),
                     ("leg-back-near", JARRET), ("leg-back-far", JARRET)]:
        joint, half = A.leg_joint(text, name, OY - hh)
        A.split_leg(root, paints, name, joint, half + .35)
    # La lèvre emporte une petite zone du mufle, avec un bourrelet rose dessous.
    lip = HLs([(19.2, 3.9), (22, 3.9), (23.3, 3.8), (23.4, 5.5), (19, 5.6)])
    # Le bourrelet doit être entièrement caché par la lèvre au repos.
    # lift_part le peint entre la tête et la lèvre ; la zone sûre est donc
    # à l'intérieur de la lèvre ET du remplissage opaque de la tête.
    lip_area = Polygon([S(p) for p in lip]).buffer(-.15)
    head_area = Polygon([S(p) for p in dense(HLs(HEAD),True,8)]).buffer(-.15)
    safe_gum = lip_area.intersection(head_area)
    pad = Polygon([S(p) for p in HLs([(19.8,4.16),(21.7,4.12),(21.75,4.46),(19.8,4.6)])])
    ridge = LineString([S(p) for p in HLs([(19.9,4.49),(21.6,4.36)])]).buffer(.055)
    under = [ET.Element(A.Q+"path",{"d":A._poly_d(pad.intersection(safe_gum)),"fill":"#C79591"}),
             ET.Element(A.Q+"path",{"d":A._poly_d(ridge.intersection(safe_gum)),"fill":"#8F6264"})]
    A.lift_part(root, paints, "head", [S(p) for p in lip], "lip", S(HL(19.4, 4.2)), under=under)
    # Trois paquets pendants du ventre : les racines restent largement
    # couvertes par la toison, seuls les bouts libres se cisaillent.
    for i, (xa, xb) in enumerate([(27, 39), (39, 52), (52, 65)], 1):
        area = [S((xa, 27.5)), S((xb, 27.5)), S((xb, 10)), S((xa, 10))]
        A.cut_under(root, paints, "body", area, f"sway-{i}", S(((xa + xb) / 2, 27.5)), margin=1.2)
    ET.indent(root, space=" ")
    return ET.tostring(root, encoding="unicode") + "\n"


def main(path):
    leg("leg-back-far", 11, False, True)
    leg("leg-front-far", 58, True, True)
    tail()
    leg("leg-back-near", 5, False)
    leg("leg-front-near", 66, True)
    fleece("neck", NP, True)
    fleece("body", BP)
    head()
    doc.pivot("ground", (35, 0))
    svg = doc.render(120, 86, """Skudde : bélier blanc de quatre à six ans, toison d'automne.
58 cm au garrot ; 1 unité = 1 cm. Profil gauche, tête vers la droite.
Toison mixte à longues mèches pointues, crinière au cou et à l'épaule,
cornes ambrées en colimaçon, tête et pattes rases, queue courte triangulaire.
D'après docs/races/skudde.md ; écrit par tools/races/skudde/gen_skudde.py.
© 2026 Alejandro San Martin Lamas — MIT OR Apache-2.0""")
    svg = articulate(svg)
    if VARIANTE:
        return variante(path, svg)
    Path(path).write_text(svg, encoding="utf-8")
    print(f"{path} : {len(svg.encode()) / 1024:.0f} Ko, {svg.count('<path')} tracés")


def variante(path, svg):
    """Les autres dessins : ce qui change, puis les proportions."""
    root = ET.fromstring(svg)
    if VARIANTE == "agneau":
        # La marque rouille des agneaux, sur les pattes ; elle disparaît chez l'adulte (§ 1.5).
        variantes.recolorer(root, variantes.teinte("#B4763F", .3),
                    dans=("leg-front-near", "leg-front-far", "leg-back-near", "leg-back-far"),
                    si=lambda c: variantes.clarte(c) > .45)
    variantes.ecrire(path, root, **PROPORTIONS[VARIANTE])


if __name__ == "__main__":
    main(sys.argv[1] if len(sys.argv) > 1 else HERE.parents[2] / "pets/skudde/skudde.svg")
