#!/usr/bin/env python3
"""Le Texel néerlandais : bélier de trois ans, début septembre, H = 72 cm.

Profil droit d'après docs/races/texel.md et ses photographies (voir README).
Le repère est en centimètres, x vers le nez, h vers le haut, sol h = 0.
La toison courte forme des plaques serrées, sans mèches pendantes : pas de
sway. Les pattes et l'encolure sont découpées par articulations.py ; la
lèvre, la mâchoire, les oreilles et l'œil ont leurs propres articulations.

Usage : python3 tools/races/texel/gen_texel.py pets/texel/texel.svg
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
# Sans variante : le bélier.
VARIANTE = sys.argv[2] if len(sys.argv) > 2 else ""
PROPORTIONS = {
    "brebis": dict(  # Brebis Texel : 68 cm au garrot, tête et encolure plus fines, mamelle.
        garrot=(72, 68), encolure=(1.0, .88), tete=(.97, .94)),
    "agneau": dict(  # Agneau Texel de deux semaines : 41 cm au garrot, blanc, tête nue au nez noir.
        variantes.AGNEAU, garrot=(72, 41)),
}
from shapely.geometry import Point  # noqa: E402
from outils import (Doc, S, OY, Polygon, add, cr, geom_d, lerp,  # noqa: E402
                    mul, norm, poisson, qstroke, region, rot, shade, smooth, sub)

D = Doc()
R = random.Random(20150907)  # date de la photo du sujet, jamais l'horloge
POLL = (108.0, 69.0)
MUZZLE = (130.1, 51.0)
NECK = (88.0, 39.0)
CARPUS, HOCK = 19.0, 23.0

# § 2.4 : au repos, l'axe nuque → bout du nez descend de 45 à 50° sous
# l'horizontale (51° sur la photo du sujet) ; § 5.2 : museau haut et carré,
# mâchoire presque parallèle au chanfrein. La tête est dessinée dans son
# propre repère (nez à 30°), puis le bas du museau descend (DEEPEN) et le
# tout tourne autour de la nuque (TILT). Les oreilles et la pupille suivent
# sans tourner : leur port et la fente horizontale restent ceux du dessin.
TILT = math.radians(-14)
DEEPEN = 2.6


def HT(p):
    """Un point du repère de la tête, posé sur le dessin."""
    x, h = p
    h -= DEEPEN * smooth(113, 131, x) * smooth(62, 51, h)
    return add(POLL, rot(sub((x, h), POLL), TILT))


def HTs(pts):
    return [HT(p) for p in pts]


def moved(pts, anchor=None):
    """Une pièce qui suit la tête sans tourner, accrochée à son premier point."""
    a = pts[0] if anchor is None else anchor
    d = sub(HT(a), a)
    return [add(p, d) for p in pts]


def head_flow(flow):
    """Un champ de poil du repère de la tête, tourné avec elle."""
    return lambda p: rot(flow(add(POLL, rot(sub(p, POLL), -TILT))), TILT)

# § 2 : dos en table, poitrine profonde, gigot descendu. La limite antérieure
# appartient au cou ; l'arc d'épaule, calculé à la fin, reste avec le corps.
BODY = [(9, 66), (13, 72), (22, 75.4), (34, 76.8), (48, 77), (63, 76.6),
        (77, 76.2), (89, 75.8), (99, 74.0), (107.5, 70.0), (111, 65.0),
        (111.2, 59.0), (110.0, 52.5), (108.5, 46), (105, 39), (100, 32.6),
        (93, 28.5), (84, 29.5), (74, 30.8), (62, 30.4), (50, 29.5),
        (40, 30.0), (34, 34), (30.5, 37), (27.5, 33), (25, 28.6),
        (19, 26.8), (13, 28.4), (8.5, 34), (6.8, 42), (7, 53), (9, 66)]
BP = region(BODY[:-1], per=8)

# § 5 : face nue, front plat, léger stop, nez large et carré. L'encoche de
# la mâchoire est recouverte au repos, sans trait qui couperait la joue.
TOP = HTs([(105.5, 67.5), (108, 70), (112, 70.5), (116, 69), (119, 66),
           (120.7, 64.7), (124.0, 63), (128.0, 61.7), (131, 60.0), (132.3, 57.5),
           (132.5, 54.0), (131.4, 50.8)])
MOUTH = HTs([(131.4, 50.8), (128.4, 50.4), (125.0, 50.8), (121.8, 51.2)])
UNDER = HTs([(121.8, 51.2), (120.1, 49.9), (116, 50), (111.2, 51),
             (107.0, 53.7), (104.8, 58), (104.3, 62.7), (105.5, 67.5)])
HP = region(TOP + MOUTH[1:] + UNDER[1:-1])


def ellipse(c, rx, ry, angle=0, n=24):
    co, si = math.cos(angle), math.sin(angle)
    return [(c[0] + rx * math.cos(a) * co - ry * math.sin(a) * si,
             c[1] + rx * math.cos(a) * si + ry * math.sin(a) * co)
            for a in (2 * math.pi * i / n for i in range(n))]


def line(pts, color, width, opacity=1):
    D.path(cr(pts, closed=False), stroke=color, sw=width, so=opacity, cap="round")


def glow(name, center, rx, ry, color, opacity, clip, angle=0):
    shape = region(ellipse(center, rx, ry, angle)).intersection(clip)
    fill = D.rad(name, center, ry, [(0, color, opacity), (.55, color, opacity * .6),
                                  (1, color, 0)], sx=rx / ry, angle=angle)
    D.path(geom_d(shape), fill=fill)


def hair(zone, count, flow, light="#FAF4E9", dark="#928776", length=(.18, .45)):
    """Poil ras orienté, fin et couché ; jamais une toison sur la face."""
    if zone.is_empty:
        return
    x0, y0, x1, y1 = zone.bounds
    D.begin_batch()
    for _ in range(count):
        p = (R.uniform(x0, x1), R.uniform(y0, y1))
        v = norm(flow(p))
        tip = add(p, mul(v, R.uniform(*length)))
        if not zone.contains(Point(p)) or not zone.contains(Point(tip)):
            continue
        bend = R.uniform(-.08, .08)
        mid = add(lerp(p, tip, .5), (-v[1] * bend, v[0] * bend))
        D.stroke(qstroke(p, mid, tip, 2), light if R.random() < .68 else dark,
                 .045, R.choice((.24, .38, .52)))
    D.flush()


def wool(zone):
    """§ 3 : plaques irrégulières, plus longues et creusées vers le ventre.

    Les creux suivent des lignes ouvertes qui serpentent entre les mèches ;
    très courts sur le dessus, ils s'allongent sous le flanc et le col.
    La lumière reste continue sur la masse, sans dessin de cellules fermées.
    """
    D.path(geom_d(zone), fill="url(#laine)")
    # Fentes souples entre les mèches serrées : ouvertes, irrégulières et
    # verticales, jamais le contour fermé de polygones (effet de mosaïque).
    D.begin_batch()
    for x, h in poisson(zone.buffer(-.35), 1.05, R):
        depth = max(0, min(1, (71-h)/32))
        if R.random() > .20 + .70*depth:
            continue
        length = R.uniform(.35, .8) + depth*R.uniform(.7, 1.9)
        drift = R.uniform(-1.1, 1.1) + (-.25 if x > 86 else .1)
        pts = [(x, h)]
        for i in range(1, 7):
            pts.append((x + drift*i/6 + R.uniform(-.28, .28), h-length*i/6))
        if not all(zone.contains(Point(p)) for p in pts):
            continue
        # Le creux n'est profond que sous l'épaule et le flanc.
        D.stroke(cr(pts, closed=False), "#806849", .075+.07*depth, .10+.32*depth)
        D.stroke(cr([add(p, (.17, .09)) for p in pts], closed=False),
                 "#FFF0D4", .13, .10+.22*depth)
        if depth > .45 and R.random() < .55:
            p0 = pts[R.randrange(2, 5)]
            branch = [p0, add(p0, (.4, -.18)), add(p0, (.95, -.10)), add(p0, (1.35, -.35))]
            if all(zone.contains(Point(p)) for p in branch):
                D.stroke(cr(branch, closed=False), "#806849", .085, .24)
    D.flush()
    # Le grain fin, frisé et serré : petites virgules plutôt que des anneaux.
    D.begin_batch()
    for p in poisson(zone.buffer(-.18), .43, R, k=10):
        a = R.uniform(-math.pi, math.pi)
        v = (math.cos(a), math.sin(a))
        size = R.uniform(.15, .38)
        tip = add(p, mul(v, size))
        mid = add(lerp(p, tip, .5), (-v[1] * .12, v[0] * .12))
        D.stroke(qstroke(p, mid, tip, 2), R.choice(("#FFF2DB", "#A08B6C", "#D6C1A0")),
                 .055, R.choice((.22, .32, .40)))
    D.flush()
    # Une lisière de fibres très courtes donne une silhouette vivante.
    edge = zone.exterior if isinstance(zone, Polygon) else None
    if edge:
        D.begin_batch()
        for i in range(int(edge.length / .3)):
            p = edge.interpolate(i * .3)
            q = edge.interpolate(i * .3 + .06)
            n = norm((q.y - p.y, p.x - q.x))
            a = (p.x, p.y)
            b = add(a, mul(n, R.uniform(.12, .28)))
            D.stroke(qstroke(a, lerp(a, b, .5), b, 2), "#D8C6AA", .055, .65)
        D.flush()


def leg(kind, x, far=False):
    """§ 2.5 et 7 : avant-bras galbé, carpe arrondi, canon sec, deux onglons."""
    name = f"leg-{kind}-{'far' if far else 'near'}"
    D.open(name)
    if kind == "front":
        pts = [(x-4.6, 39), (x+4.5, 39), (x+4, 31), (x+2.5, 24),
               (x+2.2, 19), (x+1.2, 14), (x+1.1, 6.5), (x+3.4, 2.6),
               (x+2.2, 1), (x-2.7, 1), (x-2.8, 5), (x-2.7, 13),
               (x-3, 19), (x-4, 24), (x-5.2, 30)]
        pivot = (x, 39)
    else:
        pts = [(x-7, 40), (x+5, 40), (x+6, 34), (x+4, 29), (x+.6, 25),
               (x-.8, 23), (x+.1, 18), (x+1.5, 7), (x+3.8, 2.6),
               (x+2.7, 1), (x-2.3, 1), (x-2.6, 5.5), (x-4, 15),
               (x-5.1, 22), (x-5.4, 25), (x-7.1, 31)]
        pivot = (x-1.0, 40)
    zone = region(pts)
    fill = D.lin(name + "-poil", (x-4, 0), (x+5, 0),
                 [(0, shade("#A89A84", .83 if far else 1)),
                  (.4, shade("#EDE4D8", .82 if far else 1)),
                  (1, shade("#C4B8A7", .80 if far else 1))])
    D.path(cr(pts), fill=fill)
    # Anatomie sans traits de bande dessinée : tendon, boulet, pli du jarret.
    line([(x-.9, 7), (x-1.3, 13), (x-1.4, 17)], "#9F927D", .18, .42)
    if kind == "front":
        glow(name + "-carpe", (x-.4, 19), 2.1, 2, "#A69886", .40, zone)
        line([(x-1.8, 20.7), (x-.2, 21.2), (x+1.4, 20.4)], "#9E907C", .14, .42)
    else:
        line([(x-4.6, 24), (x-3.5, 22.3), (x-2.5, 21.9)], "#948773", .17, .47)
    hair(zone, 420, lambda p: (.08, -1), light="#F2E9DC", dark="#9B8F7B")
    # Sabot 6,5 cm, sole horizontale ; le second doigt reste lisible derrière.
    hoof = [(x-2.7, 4.1), (x+.9, 4.6), (x+3.9, 1.9), (x+4.1, .35),
            (x+2.9, 0), (x-2.9, .1), (x-3.3, 1.1)]
    D.path(cr(hoof, t=.45), fill=D.lin(name + "-onglon", (0, 4.5), (0, 0),
                                     [(0, "#514C46"), (.5, "#2C2A28"), (1, "#141312")]))
    line([(x+1.6, 3.4), (x+2.2, 1.6), (x+2.0, .15)], "#111110", .20, .9)
    line([(x-2.3, 2.9), (x-.3, 3.4), (x+1.2, 2.9)], "#8F8579", .18, .60)
    line([(x-2.2, .65), (x, .55), (x+2.9, .6)], "#736B60", .12, .50)
    D.path(cr(ellipse((x-2.7, 5.9), .45, .85, .2, 8)), fill="#6F6457")
    D.pivot(name, pivot)
    D.close()


def tail():
    D.open("tail")
    pts = [(10.5, 68.5), (6.4, 67), (4.2, 62), (3.9, 56), (5.0, 51),
           (6.4, 49), (10.0, 50), (10.0, 56), (11.0, 62), (12, 65)]
    zone = region(pts)
    D.path(cr(pts), fill="url(#laine)")
    hair(zone, 330, lambda p: (-.1, -1), length=(.22, .55), light="#E9DDC7", dark="#9C8768")
    D.pivot("tail", (10.2, 67))
    D.close()


def body():
    D.open("body")
    wool(BP)
    glow("volume-gigot", (16, 47), 15, 22, "#F6E7CC", .26, BP, -.12)
    glow("pli-gigot", (31, 40), 6, 17, "#705A3D", .25, BP, -.25)
    glow("creux-poitrail", (96, 36), 14, 13, "#6B5638", .20, BP)
    glow("volume-epaule", (100, 53), 10, 17, "#8F7958", .12, BP, -.3)
    D.pivot("body", (58, 32))
    D.close()


def ear(name, pts, far=False):
    D.open(name)
    zone = region(pts)
    D.path(cr(pts), fill=D.lin(name + "-blanc", pts[1], pts[-2],
                             [(0, "#EEE5D8"), (.55, "#DED3C3"), (1, "#ACA08E")]))
    c = (sum(p[0] for p in pts) / len(pts), sum(p[1] for p in pts) / len(pts))
    inner = [lerp(c, p, .72) for p in pts]
    D.path(cr(inner), fill=D.lin(name + "-rose", pts[2], pts[0],
                               [(0, "#C9A9A4"), (.6, "#B79A90"), (1, "#8E7470")]))
    hair(zone, 160, lambda p: sub(p, pts[0]), length=(.15, .45))
    D.pivot(name, pts[0])
    D.close()


def head():
    D.open("head")
    # L'oreille lointaine précède le crâne, l'autre est peinte par-dessus.
    # § 5.2 : la lointaine dépasse au-dessus du front, la pointe en l'air et
    # en arrière (raccourcie : elle s'écarte de nous).
    ear("ear-far", moved([(110.2, 68.6), (111.3, 70.3), (109.0, 72.9), (106.1, 74.4),
                          (105.6, 73.4), (107.7, 70.5)]), True)
    D.open("jaw")
    jaw = HTs([(110.8, 57), (118, 55), (125, 53), (131, 51.2), (130.9, 49.4),
               (127.4, 49.5), (122, 49.5), (115, 49.2), (110.5, 51.3)])
    D.path(cr(jaw), fill="url(#face)")
    D.path(cr(HTs([(116, 53.7), (123, 52.2), (130.9, 51.1), (130.1, 50), (123, 50.7),
                   (117, 52)])), fill="#44332C")
    D.path(cr(HTs([(122, 52.1), (126, 51.7), (129.5, 51.3), (126, 51.1), (123, 51.3)])),
           fill="#BB8C84")
    line(HTs([(119.5, 49.8), (125, 49.1), (129.8, 49.6)]), "#B3A797", .20, .65)
    D.pivot("jaw", HT((113.8, 52.5)))
    D.close()
    D.path(cr(TOP + MOUTH[1:] + UNDER[1:-1]), fill="url(#face)")
    glow("joue-ombre", HT((110.5, 55.5)), 9, 7, "#9E9587", .36, HP, -.3 + TILT)
    glow("arcade", HT((117, 65.6)), 5.0, 2, "#A29484", .40, HP, -.6 + TILT)
    glow("chanfrein-lumiere", HT((124, 59.6)), 6.5, 2, "#FFFCF6", .46, HP, -.65 + TILT)
    glow("joue-claire", HT((114, 57)), 4.5, 4.6, "#FFFCF6", .22, HP, TILT)
    # Poil court rayonnant autour de l'œil et couché vers le museau.
    hair(HP, 2600, head_flow(lambda p: (p[0]-116.8 + 2, (p[1]-62.8)*.65-3)))
    # Trois plis souples, transversaux au chanfrein, et leur côté éclairé.
    for pts in [[(121.8, 62.9), (121.9, 62.1), (121.6, 61.4)],
                [(124.1, 61.5), (124.2, 60.8), (123.8, 60.0)],
                [(126.5, 60), (126.5, 59.4), (126.2, 58.7)]]:
        line(HTs(pts), "#A99C8A", .15, .45)
        line(HTs([add(p, (.22, .14)) for p in pts]), "#FCF6EC", .13, .75)
    # Petit cuir noir à l'extrémité seulement, aucun masque sur la face.
    nose = HTs([(130.4, 59.8), (131.8, 58.5), (132.4, 56.0), (132.3, 53),
                (132.0, 55.0), (130.3, 54.3), (129.0, 54.7), (129.5, 56.0)])
    D.path(cr(nose), fill=D.rad("truffe", HT((131, 57.3)), 4,
                               [(0, "#4A4B50"), (.55, "#242426"), (1, "#111113")]))
    D.path(cr(HTs([(130.0, 57.3), (131.0, 57.7), (131.9, 56.8), (131.5, 55.3),
                   (130.5, 55), (129.9, 54.0), (129.4, 54.8)])), fill="#070707")
    line(HTs([(130.0, 57.9), (131.1, 58.1), (131.9, 57.2)]), "#89898A", .17, .72)
    line(HTs([(132.0, 54.8), (131.7, 53.1), (131.1, 51.2)]), "#756C60", .11, .70)
    # Lèvre indépendante, limitée à 15 % de la tête. La gencive restera
    # dessous après découpe ; le museau ne devient jamais entièrement rose.
    D.path(cr(HTs([(125.9, 51.6), (128, 51.5), (130.4, 51.4), (131.4, 50.8),
                   (129, 50.3), (126, 50.8)])), fill="#3A352E")
    line(MOUTH, "#211E1A", .16, .90)
    line(TOP[1:], "#9D9487", .16, .48)
    # Œil franc, ambre. Le contour palpébral n'est pas un masque noir.
    E0 = (117.0, 63.1)
    E = HT(E0)
    almond = HTs([(114.75, 62.95), (115.35, 64.0), (116.6, 64.55), (118, 64.2),
                  (119.05, 63.25), (118.5, 62.3), (117.2, 61.85), (115.65, 62.1)])
    D.path(cr(HTs([(114.3, 63), (115, 64.7), (116.6, 65.05), (118.3, 64.7),
                   (119.5, 63.25), (118.8, 61.75), (117.2, 61.3), (115.2, 61.7)])),
           fill="#C9B9A7")
    D.open("eyes")
    D.path(cr(almond), fill="#3B302C")
    D.open("pupils")
    # L'iris et la fente suivent l'œil sans tourner : la pupille reste à
    # l'horizontale au repos, le moteur la garde ainsi quand la tête penche.
    dE = sub(E, E0)
    D.path(cr([add(p, dE) for p in ellipse(E0, 1.68, 1.17)]),
           fill=D.rad("iris", add((116.8, 63.3), dE), 1.8,
                      [(0, "#C6A87C"), (.6, "#AE9068"), (1, "#5C4A45")]))
    for i in range(28):
        a = 2 * math.pi * i / 28
        v = (math.cos(a), math.sin(a)*.7)
        line([add(E, mul(v, .85)), add(E, mul(v, 1.52))], "#786046", .065, .60)
    D.path(cr([add(p, dE) for p in [(115.7, 63.2), (116.8, 63.35), (118.3, 63.18),
                                    (118.2, 62.8), (116.7, 62.75), (115.6, 62.85)]], t=.4),
           fill="#1A1612")
    D.pivot("pupils", E)
    D.close()
    D.path(cr([add(p, dE) for p in ellipse((116.2, 63.7), .3, .18)]), fill="#F2EEE6", fo=.90)
    D.pivot("eyes", E)
    D.close()
    line(almond[:5], "#5F5145", .22, .88)
    line(HTs([(114.8, 64.2), (116.3, 64.8), (117.9, 64.6)]), "#F6EFE3", .18, .75)
    ear("ear-near", moved([(108.7, 67.8), (104.8, 70), (99.3, 72), (99.8, 69),
                           (102.7, 66.4), (106.8, 65.8)]))
    D.pivot("head", POLL)
    D.close()


def extra(root, paints):
    # Un dos d'encolure entier, arrondi et caché sous l'épaule au repos.
    # La coupe automatique seule laisse un coin au garrot, qui devient une
    # pointe quand le cou plonge. Ce fond prolonge sa tangente vers l'arrière.
    hidden = [(52, 49), (59, 54), (73, 61.4), (89, 67.8), (104, 70.6),
              (107.5, 70), (111, 65), (111.2, 59), (110, 52.5), (108.5, 46),
              (105, 39), (100, 32.6), (93, 28.5), (84, 31), (76, 37), (60, 40)]
    neck = next(el for el in root.iter() if el.get("id") == "neck")
    backdrop = [ET.Element(A.Q + "path", {"d": cr(hidden), "fill": "url(#laine)"})]
    # Le fond découvert ne doit pas former un panneau lisse au milieu de la
    # toison. Un grain serré et quelques creux le prolongent, cachés au repos.
    zone = region(hidden)
    grain, creases = [], []
    for x, h in poisson(zone.buffer(-.25), .53, R, k=10):
        p = (x, h)
        tip = (x+R.uniform(-.15, .15), h-R.uniform(.15, .4))
        grain.append(qstroke(p, (x+.13, (h+tip[1])/2), tip, 2))
        if h < 67 and R.random() < .16:
            pts = [p, (x+.12, h-.35), (x-.14, h-.7), (x+.05, h-1.15)]
            if all(zone.contains(Point(q)) for q in pts):
                creases.append(cr(pts, closed=False))
    for paths, color, width, opacity in ((grain, "#A08B6C", .055, .3),
                                          (creases, "#806849", .09, .24)):
        backdrop.append(ET.Element(A.Q + "path", {"d": "".join(paths), "fill": "none",
                         "stroke": color, "stroke-width": str(width),
                         "stroke-opacity": str(opacity), "stroke-linecap": "round"}))
    for i, el in enumerate(backdrop):
        neck.insert(i, el)
    lip = HTs([(125.5, 52.1), (128.1, 52.2), (130.7, 52.1), (132.3, 51.6),
               (132.3, 50), (128.5, 49.9), (125.4, 50.3)])
    pad = HTs([(126.4, 51.7), (128.4, 51.9), (130.7, 51.5), (130.4, 50.9),
               (128.4, 50.7), (126.4, 51.0)])
    gum = A.gum([S(p) for p in pad],
                [S(p) for p in HTs([(126.7, 51.3), (128.5, 51.2), (130, 51.1)])],
                "#AB7B77", "#6F4847", .13)
    A.lift_part(root, paints, "head", [S(p) for p in lip], "lip", S(HT((126, 51.9))), under=gum)


def main(path):
    D.lin("laine", (0, 77), (0, 26), [(0, "#EEE2CC"), (.22, "#E8D8BD"),
          (.50, "#DFCDB2"), (.77, "#C3AE8D"), (1, "#9E8868")])
    D.lin("face", (115, 70), (115, 49), [(0, "#F4EEE6"), (.5, "#ECE4D8"),
          (.82, "#DDD3C3"), (1, "#BDB3A2")])
    leg("back", 30, True)
    leg("front", 87, True)
    tail()
    # Le scrotum couvert de poils soyeux est visible devant les postérieurs.
    scrotum = [(32, 35), (39, 34), (40, 29), (39, 25), (36.5, 23),
               (33.2, 23.4), (31.4, 25.5), (30.5, 29)]
    if not VARIANTE:
        D.path(cr(scrotum), fill=D.lin("scrotum", (30, 27), (40, 27),
               [(0, "#A68D77"), (.5, "#D5C4AE"), (1, "#AC907D")]))
        hair(region(scrotum), 270, lambda p: (0, -1), light="#EDE0CC", dark="#9A8470", length=(.3, .8))
    leg("back", 20)
    leg("front", 96)
    body()
    head()
    D.pivot("ground", (59, 0))
    svg = D.render(145, 91, """Texel : bélier néerlandais de trois ans, début septembre.
1 unité = 1 cm ; 72 cm au garrot osseux, 77 cm de toison au sol.
Généré par tools/races/texel/gen_texel.py ; fiche docs/races/texel.md.
© 2026 Alejandro San Martin Lamas — MIT OR Apache-2.0""")
    neck = dict(source="body", area=A.shoulder(S((105, 71.5)), S((99, 29.5)), 6),
                pivot=S(NECK), poll=S(POLL), muzzle=S(HT(MUZZLE)))
    svg = A.articulate_breed(svg, OY-CARPUS, OY-HOCK, neck=neck, extra=extra)
    if VARIANTE:
        return variante(path, svg)
    path = Path(path)
    path.parent.mkdir(parents=True, exist_ok=True)
    path.write_text(svg, encoding="utf-8")
    print(f"{path} : {len(svg.encode()) // 1024} Ko, {svg.count('<path')} tracés")


def variante(path, svg):
    """Les autres dessins : ce qui change, puis les proportions."""
    root = ET.fromstring(svg)
    if VARIANTE == "brebis":
        variantes.mamelle(root, *variantes.sous_le_ventre(root, 11, -2), largeur=10, hauteur=6.5,
                  peau="#E2BBAA", ombre="#B08A7A")
    variantes.ecrire(path, root, **PROPORTIONS[VARIANTE])


if __name__ == "__main__":
    main(sys.argv[1] if len(sys.argv) > 1 else HERE.parents[2] / "pets/texel/texel.svg")
