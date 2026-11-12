#!/usr/bin/env python3
"""Écrit la brebis Karakul de Mouflon, d'après docs/races/karakul.md.

Brebis noire grisonnante de quatre ans, avant la tonte de septembre :
66 cm au garrot, tête nue, oreilles pendantes, toison grossière à pointes
roussies et queue grasse naturelle. Le repère de travail est en centimètres,
x vers l'avant et h vers le haut ; une unité SVG vaut un centimètre réel.

Les primitives géométriques et SVG reprennent celles de la Lacaune ; le dessin,
les proportions, les mèches et la queue sont propres à cette race. Les
régions sont découpées au moment de la génération, jamais par un masque ou
une découpe SVG. Une graine fixe garde chaque mèche à sa place.

Usage : python3 tools/races/karakul/gen_karakul.py pets/karakul/karakul.svg
Dépendance : shapely (comme les autres générateurs de races).
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
    "belier": dict(  # Bélier Karakul : 75 cm au garrot, cornes en spirale ouverte, tête plus forte et plus busquée, encolure épaisse.
        garrot=(66, 75), encolure=(1.0, 1.2), tete=(1.06, 1.08), queue=(1.1, 1.05)),
    "agneau": dict(  # Agneau Karakul de deux semaines : 41 cm au garrot, noir aux boucles serrées et lustrées, haut sur pattes, longues oreilles.
        variantes.AGNEAU, garrot=(66, 41), pattes=(1.15, 1.2), corps=(.86, .8)),
}
import outils as O  # noqa: E402
from shapely.geometry import Point, Polygon, box  # noqa: E402
from outils import (Doc, S, add, mul, norm, lerp, rot, cr, dense,  # noqa: E402
                    runs_path, geom_d, mix, shade, qstroke, lstroke)

# Le décalage laisse de la place derrière la croupe pour la queue grasse.
O.OX, O.OY = 16.0, 94.0
D = Doc()
R = random.Random(661987)
GARROT, CARPE, JARRET = 66.0, 18.0, 22.5
NUQUE = (93.0, 83.5)
COU = (75.0, 52.0)
AXE = math.radians(36.0)
U, V = (math.cos(AXE), -math.sin(AXE)), (-math.sin(AXE), -math.cos(AXE))


def H(u, v):
    """Repère de la tête : u de la nuque vers le nez, v vers la ganache."""
    return add(NUQUE, add(mul(U, u), mul(V, v)))


def hs(pts):
    return [H(*p) for p in pts]


def ellipse(c, rx, ry, angle=0.0, n=24):
    return [add(c, rot((rx * math.cos(i * math.tau / n), ry * math.sin(i * math.tau / n)), angle))
            for i in range(n)]


def region(pts):
    return Polygon(dense(pts, True, 7)).buffer(0)


def glow(gid, p, rx, ry, col, alpha, angle=0.0, clip=None):
    """Un modelé doux, calculé et découpé en géométrie à la génération."""
    pp = region(ellipse(p, rx, ry, angle))
    if clip is not None:
        pp = pp.intersection(clip)
    if pp.is_empty:
        return
    D.path(geom_d(pp), fill=D.rad(gid, p, ry, [(0, col, alpha), (.55, col, alpha * .48), (1, col, 0)],
                                sx=rx / ry, angle=angle))


def hair(area, count, length, direction, light="#59595C", dark="#090909", opacity=.38, width=.08):
    """Poils ras : de petits traits couchés, groupés pour une cuisson légère."""
    if area.is_empty:
        return
    xmin, ymin, xmax, ymax = area.bounds
    D.begin_batch()
    for _ in range(count):
        p = (R.uniform(xmin, xmax), R.uniform(ymin, ymax))
        if not area.contains(Point(p)):
            continue
        a = direction(p) if callable(direction) else direction
        L = R.uniform(length * .55, length * 1.15)
        e = add(p, mul(a, L))
        if not area.contains(Point(e)):
            continue
        c = lerp(p, e, .5)
        c = add(c, (.08 * L, .04 * L))
        D.stroke(qstroke(p, c, e, 2), light if R.random() < .55 else dark, width, opacity)
    D.flush()


# ------------------------------------------------------------------ la toison
# Ni anneaux ni petits boudins d'astrakan : les adultes portent des mèches
# ouvertes, larges en haut, lustrées, aux longues ondulations lâches (§ 3).
PALETTE = [(0, "#342D28"), (.3, "#5A4F42"), (.66, "#807665"), (1, "#A59882")]


def fleece_color(h):
    return O.ramp(PALETTE, max(0, min(1, (h - 27) / 45)))


def lock(root, length, width, lean=0.0, color=None, suffix="", darken=1.0, clip=None):
    """Une mèche effilée, en vague souple, avec creux et quelques jarres.
    La racine est assez large pour recouvrir la suivante ; la pointe reste
    ouverte, irrégulière. Les mèches tournées sont toujours majoritairement
    pendantes, même sur la croupe."""
    x, h = root
    phase = R.uniform(-.8, .8)
    bend = R.uniform(-.26, .26) * width
    def center(t):
        wave = math.sin(math.pi * (t * 1.6 + phase)) - math.sin(math.pi * phase)
        return (x + lean * t + .43 * width * wave + bend * t * t, h - length * t)
    ts = [0, .16, .34, .53, .72, .88, 1.0]
    centers = [center(t) for t in ts]
    ws = [width * .12, width * .36, width * .48, width * .39, width * .27, width * .13, .01]
    outline = [(p[0] - w, p[1] + .15 * w) for p, w in zip(centers, ws)]
    outline += [(p[0] + w, p[1] - .1 * w) for p, w in zip(centers[-2::-1], ws[-2::-1])]
    base = color or fleece_color(h - .5 * length)
    base = shade(base, darken * R.uniform(.94, 1.06))
    # Le modelé du faisceau, clair sur un côté et sombre dans ses creux ;
    # les fibres claires apportent le lustre et la chaleur des pointes.
    gid = f"m{suffix}"
    mid = center(.45)
    fill = D.lin(gid, (mid[0] - .54 * width, mid[1]), (mid[0] + .54 * width, mid[1]),
                 [(0, shade(base, .78)), (.27, mix(base, "#D0C5AF", .12)),
                  (.54, base), (1, shade(base, .7))])
    shape = cr(outline, t=.75) if clip is None else geom_d(region(outline).intersection(clip))
    D.path(shape, fill=fill, fo=.96)
    # Le creux ondulé et le reflet décalé : une fibre continue, sans spires.
    center_dark = [add(center(t), (.1 * width, 0)) for t in (.08, .25, .45, .65, .85, .97)]
    if clip is None or all(clip.buffer(.1).contains(Point(p)) for p in center_dark):
        D.path(cr(center_dark, False, nd=2), stroke=shade(base, .55), sw=.10, so=.30, cap="round")
    lit = [add(center(t), (-width * (.26 - .17 * t), .05)) for t in (.07, .25, .45, .65, .85, .96)]
    if clip is None or all(clip.buffer(.1).contains(Point(p)) for p in lit):
        D.path(cr(lit, False, nd=2), stroke=mix(base, "#D0C5AF", .38), sw=.12, so=.62, cap="round")
    D.begin_batch()
    for j in range(7):
        side = (j - 3) * width * .12 + R.uniform(-.06, .06)
        t0, t1 = R.uniform(.0, .2), R.uniform(.78, 1.1)
        pts = []
        for k in range(6):
            t = t0 + (t1 - t0) * k / 5
            p = center(t)
            pts.append(add(p, (side * (1 - t * .92), 0)))
        if clip is not None and not all(clip.buffer(.1).contains(Point(p)) for p in pts):
            continue
        D.stroke(cr(pts, False, nd=2), mix(base, "#D4C7B1", .38) if j % 2 else shade(base, .62),
                 .045 if j % 2 else .06, .38)
    D.flush()


def undercoat(area, seed, count):
    """Le duvet et les mèches secondaires entre les grands faisceaux.
    Cette couche se peint dessous : seuls les interstices la découvrent.
    Son propre tirage laisse les grandes mèches et les bornes inchangées.
    Toutes les fibres restent à l'intérieur de la silhouette existante.
    """
    r = random.Random(seed)
    inner = area.buffer(-.35)
    xmin, ymin, xmax, ymax = inner.bounds
    fibers = []
    for _ in range(count):
        x, h = r.uniform(xmin, xmax), r.uniform(ymin, ymax)
        length, lean = r.uniform(1.8, 5.6), r.uniform(-.9, .55)
        phase, wave = r.uniform(-1, 1), r.uniform(.1, .32)
        pts = [(x + lean * t + wave * (math.sin(phase + t * 5) - math.sin(phase)), h - length * t)
               for t in (0, .2, .4, .6, .8, 1)]
        if not all(inner.contains(Point(p)) for p in pts):
            continue
        col = fleece_color(round(h / 5) * 5)
        fibers.append((pts, col, r.random()))
    # Quelques petits faisceaux souples, puis les fibres individuelles.
    # Leur contraste est faible : aucun motif répété ne rivalise avec les
    # longues mèches du dessus.
    for i, (pts, col, light) in enumerate(fibers):
        if i % 5:
            continue
        width = r.uniform(.12, .25)
        outline = [(p[0] - width * math.sin(math.pi * (j + .2) / 5.4), p[1])
                   for j, p in enumerate(pts)]
        outline += [(p[0] + width * math.sin(math.pi * (j + .2) / 5.4), p[1])
                    for j, p in list(enumerate(pts))[::-1]]
        shape = region(outline).intersection(inner)
        D.path(geom_d(shape), fill=mix(col, "#C8BBA2", .2), fo=.35)
    D.begin_batch()
    for pts, col, light in fibers:
        D.stroke(cr(pts, False, nd=2), mix(col, "#D0C4AC", .34) if light > .42 else shade(col, .66),
                 .07 if light > .42 else .09, .35 if light > .42 else .26)
    D.flush()


BACK = [(2.0, 59), (5, 64), (10, 68), (17, 71.2), (24, 73.2), (31, 72.7),
        (39, 71.5), (47, 70.9), (55, 71.1), (62, 71.8), (66.5, 72)]
SHOULDER = [(71.5, 71), (75, 68.3), (78, 64), (79.2, 59), (80.2, 53), (81.2, 47),
            (80.5, 42), (78, 38.7)]
UNDER = [(73, 35.2), (67, 34), (60, 33.5), (52, 33), (43, 32.8), (35, 33.5),
         (28, 34.7), (21, 37), (16, 39), (10, 41), (6, 45), (3.5, 51), (2, 59)]
BODY = BACK + SHOULDER + UNDER
BP = region(BODY)

# La nuque est à 83,5 cm, un peu au-dessus des 72 cm de laine du garrot.
# Le fond du cou est arrondi près du pivot : une grande languette vers le
# garrot se découvrirait en tablette horizontale quand elle broute.
NECK = [(67, 60), (69, 68), (71, 73), (77, 76), (82, 80), (86, 84), (90, 86),
        (94.5, 85.6), (97.2, 82), (97.4, 77), (95.3, 72), (91, 66), (87, 59),
        (84, 51), (81, 43), (76, 39), (71, 40), (67, 48), (65.5, 55)]
NP = region(NECK)


def neck():
    D.open("neck")
    d = cr(NECK)
    D.path(d, fill="url(#fond-toison)")
    undercoat(NP.intersection(box(63, 61, 94, 82)), 15643, 470)
    D.path(d, fill=D.lin("cou-noir", (80, 64), (94, 82), [(0, "#272323", 0), (.32, "#282525", .35),
                                                         (.65, "#202020", .9), (1, "#18181A")]))
    glow("cou-rondeur", (86, 72), 4.4, 13, "#6A6764", .32, -.52, NP)
    # Mèches plus courtes et plus noires au sommet, fondu de la tête au cou.
    seq = 0
    for level, start, end in [(47, 71, 79), (54, 69, 86), (61, 68, 89), (68, 68, 92), (75, 73, 94), (81, 83, 94)]:
        x = start + R.uniform(-.5, .5)
        while x < end:
            p = (x, level + R.uniform(-1.4, 1.4))
            if NP.buffer(-1).contains(Point(p)):
                col = mix(fleece_color(level), "#202022", O.smooth(65, 82, level) * .9)
                lock(p, R.uniform(6, 10), R.uniform(1.6, 2.4), .7, col, f"c{seq}")
                seq += 1
            x += R.uniform(.9, 1.5)
    hair(NP.intersection(box(80, 73, 102, 90)).buffer(-.3), 400, .65, (-.45, -.9),
         "#686669", "#0E0D0E", .32)
    D.pivot("neck", COU)
    D.close()


def body():
    D.open("body")
    cushion()
    D.path(cr(BODY), fill="url(#fond-toison)")
    # Les ombres larges restent au-dessous des mèches ; elles donnent un
    # ventre et un flanc, pas une juxtaposition de brindilles.
    glow("flanc-rond", (37, 54), 36, 19, "#B2A58E", .2, clip=BP)
    glow("ventre-creux", (38, 33), 32, 10, "#151211", .5, clip=BP)
    undercoat(BP.intersection(box(3, 53, 81, 74)), 84431, 1050)
    # Trois groupes vivants : racines sous les dernières mèches fixes,
    # pointe effilée du ventre qui accompagne l'allure (§ articulations).
    for k, (start, end) in enumerate([(20, 37), (37, 54), (54, 73)], 1):
        D.open(f"sway-{k}")
        x = start
        i = 0
        while x < end:
            root = (x, 36.5 + .013 * (x - 43) ** 2 + R.uniform(-.5, .5))
            lock(root, R.uniform(8, 10), R.uniform(2.7, 3.8), -.5,
                 fleece_color(33), f"s{k}-{i}")
            i += 1
            x += R.uniform(2.5, 3.4)
        D.pivot(f"sway-{k}", ((start + end) / 2, 36.5))
        D.close()
    # Une dernière couche courte cache la corde immobile des mèches ; le
    # volume sombre du ventre continue sous elles quand elles penchent.
    for i, x in enumerate(range(19, 76, 3)):
        p = (x, 40 + .008 * (x - 44) ** 2)
        lock(p, R.uniform(5.5, 7), 3.1, -.4, None, f"v{i}")
    # Le bord du dos, finement effiloché ; pas de contour festonné en boules.
    seq = 0
    for p in dense(BACK, False, 2):
        lock(add(p, (R.uniform(-.7, .7), R.uniform(-.2, 1))), R.uniform(4, 7),
             R.uniform(2, 3.4), -.6, fleece_color(p[1]), f"d{seq}", clip=BP)
        seq += 1
    # Plusieurs couches décalées descendent du dos vers le ventre. Leur
    # ordre recouvre les racines et casse tout alignement de rangées.
    seq = 0
    for layer, level in enumerate([37, 42, 47, 52, 57, 62, 67, 71]):
        x = 3 + R.uniform(0, 3)
        while x < 80:
            p = (x, level + R.uniform(-3.5, 3.5))
            if BP.buffer(-.8).contains(Point(p)):
                length = R.uniform(7.5, 11.5)
                if x > 68:
                    length *= .88
                lock(p, length, R.uniform(1.6, 2.6), R.uniform(-1.2, .5),
                     None, f"f{seq}")
                seq += 1
            x += R.uniform(.8, 1.35)
    # La culotte descend au-dessus du jarret, sans noyer les canons noirs.
    for i, p in enumerate([(2, 61), (1, 56), (2, 51), (4, 47), (7, 45), (10, 43), (13, 41), (16, 39)]):
        lock(p, 10 + R.uniform(-1, 1), 2.8, -2.3 if i < 4 else -.6, fleece_color(p[1] - 5), f"r{i}")
    glow("toison-volume", (39, 55), 36, 17, "#D1C5AD", .13, clip=BP)
    glow("toison-epaule", (76, 50), 7, 18, "#17130F", .2, clip=BP)
    D.pivot("body", (43, 36))
    D.close()


# ------------------------------------------------------------------ les pattes
LEGS = [("leg-back-far", False, 7, True), ("leg-front-far", True, -6, True),
        ("leg-back-near", False, 0, False), ("leg-front-near", True, 0, False)]


def leg_outline(front, dx):
    if front:
        # Antérieur presque droit, carpe un peu saillant vers l'avant.
        a = [(70, 46), (74, 44), (75, 37), (74.5, 30), (74, 24), (73.7, 20), (73.9, 18.4),
             (73.6, 16.5), (73.45, 12), (73.4, 7), (74, 4.5), (75.2, 2.5)]
        hoof = [(75.2, 2.5), (76, 1.5), (76.7, .2), (76.5, 0), (71.7, 0), (71.5, .7)]
        b = [(71.5, .7), (71.9, 2), (70.8, 3.5), (70.5, 5), (70.9, 7), (71, 12), (71, 16),
             (70.7, 18), (71, 20), (70.6, 26), (69.5, 32), (67.8, 37), (68, 42), (70, 46)]
        pivot = (71.4 + dx, 45)
    else:
        # Le jarret casse le profil à 22,5 cm : talon en arrière, canon fin.
        a = [(11, 51), (18, 46), (18, 41), (15, 36), (12, 31), (9, 26), (7.2, 23),
             (6.8, 21), (6.6, 16), (6.7, 10), (6.8, 6), (7.6, 4), (8.9, 2.5)]
        hoof = [(8.9, 2.5), (9.7, 1.5), (10.4, .2), (10.2, 0), (5.4, 0), (5.2, .7)]
        b = [(5.2, .7), (5.6, 2), (4.6, 3.5), (4.1, 5), (4.4, 7), (4.5, 12), (4.5, 17),
             (4.2, 20), (3.5, 22.5), (3.6, 25), (4.5, 30), (5.5, 37), (6, 44), (7, 49), (11, 51)]
        pivot = (11 + dx, 50)
    return [[(x + dx, h) for x, h in pts] for pts in (a, hoof, b)], pivot


def leg(name, front, dx, far):
    (a, hoof, b), pivot = leg_outline(front, dx)
    pts = a + hoof[1:-1] + b
    p = region(pts)
    xmin, _, xmax, _ = p.bounds
    D.open(name)
    d = runs_path([a, hoof, b])
    D.path(d, fill=D.lin(name + "-poil", (xmin, 18), (xmax, 18),
                         [(0, "#11100F"), (.23, "#3B3834" if not far else "#272521"),
                          (.55, "#211F1D"), (1, "#090909")]))
    # Tendons et saillies osseuses, sans chaussettes, laine ni tache claire.
    j = (72.3 + dx, CARPE) if front else (5.2 + dx, JARRET)
    glow(name + "-joint", add(j, (-.3, .7)), 1.45, 2.1, "#6D6962", .28, clip=p)
    shaft = [(j[0] - .5, 16), (j[0] - .4, 11), (j[0] - .4, 7)]
    D.path(cr(shaft, False), stroke="#706A63", sw=.25, so=.24, cap="round")
    # Le dessus du pied, petit boulet et ergots pigmentés.
    base = 72.1 + dx if front else 5.8 + dx
    D.path(cr([(base - 1.15, 4.4), (base - 1.5, 4.8), (base - 1.7, 4), (base - 1.3, 3.7)]), fill="#151210")
    # Les poils sont séparés en deux régions avant la coupe aux articulations.
    for area in [p.intersection(box(-30, j[1] + 2.4, 150, 60)), p.intersection(box(-30, 3.5, 150, j[1] - 2.4))]:
        hair(area.buffer(-.2), 440 if not far else 260, .7, (0, -1),
             "#736E67" if not far else "#49443F", "#0A0908", .35, .065)
    # Sabots noirs : onglons serrés, talon moins haut, pointe en avant.
    D.path(runs_path([hoof, [hoof[-1], add(hoof[-1], (.3, 1.2)), lerp(hoof[0], hoof[-1], .6), hoof[0]]]),
           fill=D.lin(name + "-corne", hoof[-1], hoof[1], [(0, "#0F0D0C"), (.5, "#302B27"),
                                                           (.8, "#514B44"), (1, "#1E1B19")]))
    mid = lerp(hoof[3], hoof[4], .46)
    D.path(cr([add(mid, (0, .05)), add(mid, (-.15, .7)), lerp(hoof[0], hoof[-1], .5)], False),
           stroke="#090807", sw=.16, so=.9, cap="round")
    D.path(cr([add(hoof[0], (-.25, -.2)), lerp(hoof[0], hoof[2], .6), add(hoof[2], (-.5, .1))], False),
           stroke="#7A7268", sw=.14, so=.48, cap="round")
    D.pivot(name, pivot)
    D.close()


# ------------------------------------------------------------------ la queue grasse
TAIL_P = [(4, 59), (0, 58.8), (-3.8, 56), (-6.2, 51), (-7, 45.5), (-6.7, 40.6),
          (-5.5, 36.5), (-3, 34.7), (3, 35.8), (8, 39), (11, 45), (12, 52)]


def cushion():
    """La réserve grasse suit le bassin, sans fouetter avec la queue.

    Peinte dans body avant le flanc, elle garde sa bosse derrière les
    cuisses. Seul l'appendice maigre appartient aux trois parties tail.
    """
    TP = region(TAIL_P)
    D.path(cr(TAIL_P), fill=D.lin("q-coussin", (-9, 46), (3, 46),
                                [(0, "#4C443A"), (.3, "#706353"), (.58, "#51483D"), (1, "#211C18")]))
    # Le coussin reste distinct derrière la culotte ; les mèches du haut
    # passent devant, mais le bas gras est sombre, rond et lourd (§ 2.3).
    seq = 0
    for level in (41, 46, 51, 56):
        for x in (-5, -2, 1):
            p = (x + R.uniform(-.6, .6), level + R.uniform(-1, 1))
            if TP.buffer(-.3).contains(Point(p)):
                lock(p, R.uniform(6, 9), R.uniform(1.4, 2.3), .35,
                     mix(fleece_color(level), "#463E35", .12), f"q{seq}")
                seq += 1
    glow("q-volume", (-6.2, 42.5), 2.2, 6.5, "#90816E", .16, clip=TP)


def tail():
    D.open("tail")
    # Racine de l'appendice, cachée sous le coussin au repos. Les trois
    # segments se chevauchent : aucun trou pendant une secousse.
    D.path(cr(ellipse((-4.2, 36.2), 1.4, 2.8)), fill="url(#q-noir)")
    D.open("tail-mid")
    mid = [(-5.5, 38), (-4.4, 36), (-5.9, 32.8), (-6.1, 29.5), (-4.9, 27.2),
           (-2.7, 27.5), (-3.8, 30.4), (-3.8, 32.7), (-2.8, 35.8), (-3.2, 38)]
    D.path(cr(mid), fill="url(#q-noir)")
    D.path(cr([(-4.3, 36), (-5, 33.3), (-5, 30.8), (-4.2, 28.7)], False),
           stroke="#575252", sw=.28, so=.35, cap="round")
    hair(region(mid).buffer(-.25), 100, .7, (.05, -1), "#615B59", "#0B0A0A", .35)
    D.open("tail-tip")
    tip = [(-5.8, 30), (-4.7, 27.2), (-3, 25), (-1.5, 23.5), (.1, 22.8), (1.4, 23.1),
           (2, 24.6), (1.3, 25.1), (.2, 24.8), (-1.1, 25.4), (-2.4, 27), (-3.8, 30.7)]
    D.path(cr(tip), fill="url(#q-noir)")
    D.path(cr([(-4.5, 28.9), (-3, 26.3), (-.9, 24.4), (.8, 24.1)], False), stroke="#595457", sw=.19,
           so=.4, cap="round")
    D.pivot("tail-tip", (-4.7, 29.2))
    D.close()
    D.pivot("tail-mid", (-4.2, 36.8))
    D.close()
    D.pivot("tail", (-4.2, 37.5))
    D.close()


# ------------------------------------------------------------------ tête nue, noire et busquée
# Le haut est presque droit derrière les yeux, puis s'arrondit en continu
# vers le nez : un busqué modéré de brebis, pas la grosse tête d'un bélier.
TOP = [(-1.1, .2), (-1.2, -1.3), (0, -2.7), (2, -3.1), (4, -3), (6.2, -2.8),
       (8.3, -2.5), (10.4, -2.3), (12.5, -1.8), (14.7, -1.1), (16.7, -.2), (18.5, .7),
       (20.1, 1.6), (21.2, 2.6), (21.7, 3.7), (21.7, 4.6), (21.2, 5.2)]
MOUTH = [(21.2, 5.2), (19.8, 5.3), (18.3, 5.5), (16.8, 5.7)]
NOTCH = [(16.8, 5.7), (16.3, 6.9), (15.3, 7.7)]
LOW = [(15.3, 7.7), (12.7, 8.1), (10.2, 8.3), (7.7, 8.2), (5.4, 7.9), (3.6, 7.1),
       (1.7, 5.7), (.2, 4), (-.8, 2), (-1.1, .2)]
HP = region(hs(TOP + MOUTH[1:] + NOTCH[1:] + LOW[1:]))
E = H(6.4, .6)
ALMOND = [(E[0] - 1.45, E[1] + .05), (E[0] - .9, E[1] + .8), (E[0] + .2, E[1] + .95),
          (E[0] + 1.2, E[1] + .5), (E[0] + 1.5, E[1] - .05), (E[0] + .8, E[1] - .65),
          (E[0] - .4, E[1] - .65), (E[0] - 1.2, E[1] - .3)]
AP = region(ALMOND)


def jaw():
    D.open("jaw")
    D.path(cr(hs([(13, 5.8), (16, 4.5), (18.8, 3.8), (20.5, 3.9), (21.3, 4.6), (20.4, 5.9),
                  (18.3, 7), (15, 8)])), fill="#2C171B")
    D.path(cr(hs([(16.8, 5.2), (18.4, 4.8), (20.8, 4.9), (20.5, 5.5), (18.3, 5.8)])), fill="#9B626C")
    D.path(cr(hs([(20, 4.8), (20.8, 4.7), (20.9, 5), (20.2, 5.2)])), fill="#C4B5A1")
    chin = hs([(16.6, 5.65), (18.4, 5.45), (20, 5.28), (21.3, 5.2), (21.5, 5.8), (20.8, 6.5),
               (19.5, 7), (17.6, 7.45), (15.3, 7.7), (13.8, 7.5), (14, 6.4)])
    D.path(cr(chin), fill=D.lin("menton", H(18, 5.2), H(18, 8), [(0, "#282527"), (.4, "#1A181A"), (1, "#09090A")]))
    glow("menton-lustre", H(18.4, 6.4), 2.4, .7, "#7A7375", .2, -AXE, region(chin))
    hair(region(chin).buffer(-.25), 130, .45, mul(U, -.9), "#615C60", "#070707", .3, .06)
    D.pivot("jaw", H(16.2, 5.8))
    D.close()


def eye():
    # La paupière fermée est cachée par le groupe animé tant que l'œil est ouvert.
    D.path(cr(ALMOND), fill="#211F20")
    D.path(cr([(E[0] - 1.4, E[1]), E, (E[0] + 1.4, E[1] - .03)], False), stroke="#090909", sw=.18)
    D.open("eyes")
    D.path(cr(ALMOND), fill="#3A2A21")
    D.open("pupils")
    D.path(cr(ellipse(E, 1.25, 1.25)), fill=D.rad("iris", add(E, (-.25, -.1)), 1.4,
                 [(0, "#453425"), (.35, "#8C6E52"), (.7, "#6E5646"), (1, "#34271E")]))
    D.begin_batch()
    for k in range(24):
        a = k * math.tau / 24
        p, e = add(E, (.64 * math.cos(a), .64 * math.sin(a))), add(E, (1.14 * math.cos(a), 1.14 * math.sin(a)))
        D.stroke(lstroke(p, e, 2), "#AF8B63" if k % 2 else "#3E3026", .045, .55)
    D.flush()
    pupil = [(E[0] - .85, E[1] - .08), (E[0] - .72, E[1] + .23), (E[0] + .67, E[1] + .23),
             (E[0] + .85, E[1] - .04), (E[0] + .69, E[1] - .25), (E[0] - .67, E[1] - .25)]
    D.path(cr(pupil, t=.5), fill="#0B0A0A")
    for off, r in [(-.35, .07), (-.06, .09), (.23, .075)]:
        D.path(cr(ellipse(add(E, (off, .19)), r, .07, n=8)), fill="#100D0B")
    D.pivot("pupils", E)
    D.close()
    D.path(cr(ellipse(add(E, (-.5, .4)), .22, .13)), fill="#F0ECE6", fo=.85)
    D.path(cr(ellipse(add(E, (.6, -.25)), .08, .05)), fill="#D7D2C9", fo=.55)
    D.pivot("eyes", E)
    D.close()
    # Couronne immobile autour de l'amande : elle cache l'iris déplacé.
    ring = region(ellipse(E, 3.2, 2.6)).intersection(HP).difference(AP)
    D.path(geom_d(ring), fill="url(#tete)", rule="evenodd")
    D.path(cr(ALMOND[:5], False), stroke="#050505", sw=.2, so=.85, cap="round")
    glow("arcade", add(E, (-.3, 1.6)), 2.4, .5, "#848080", .35, clip=HP)
    D.path(cr([add(E, (1.9, -.4)), add(E, (2.35, -1)), add(E, (2.7, -1.8))], False),
           stroke="#0A0808", sw=.2, so=.7, cap="round")
    D.path(cr(ALMOND[4:] + ALMOND[:1], False), stroke="#6B5D58", sw=.12, so=.55, cap="round")
    for k in range(9):
        x = -1.15 + .27 * k
        p = add(E, (x, .8 - .16 * x * x))
        D.path(qstroke(p, add(p, (.12, .22)), add(p, (.32, .25)), 2), stroke="#0A0909", sw=.065, so=.8,
               cap="round")


def ear(near):
    name = "ear-near" if near else "ear-far"
    base = H(1.7 if near else .5, -1.4 if near else -2.0)
    # Le U est étroit à la racine, large au milieu, sa pointe arrondie
    # tombe à la hauteur des naseaux, un peu en avant : l'oreille proche
    # (15,4 cm, § 2.2 : 12,5 à 18) dépasse sous la ganache, sans quoi, noire
    # sur une tête noire, elle disparaît à la taille du bureau (§ 9.1, 2).
    dx = 1.7 if near else 5.1
    hdrop = 15.4 if near else 12.3
    def f(x, t):
        return add(base, (x + dx * t, -hdrop * t))
    outline = [f(-.8, 0), f(-1.7, .14), f(-2.0, .4), f(-1.6, .69), f(-.8, .9), f(.1, 1),
               f(1.2, .95), f(2.0, .75), f(2.2, .47), f(1.4, .18), f(.6, .02)]
    EP = region(outline)
    D.open(name)
    if near:
        # L'ombre portée de l'oreille sur la joue détache son bord avant
        # et sa pointe ; elle tourne avec l'oreille quand celle-ci bat.
        for k, (x, t) in enumerate([(2.6, .32), (2.5, .58), (1.9, .8), (.6, .97)]):
            glow(f"{name}-ombre-{k}", f(x, t), 1.3, 2.1, "#000000", .5,
                 -math.atan2(dx, hdrop), HP)
    D.path(cr(outline), fill=D.lin(name + "-fond", f(-2, .4), f(2, .45),
                                 [(0, "#101010"), (.23, "#4C4950" if near else "#2D2B2E"),
                                  (.6, "#262326"), (1, "#0B0A0B")]))
    # Le pli de l'oreille est pigmenté, jamais rose.
    inner = [f(-.6, .12), f(-1, .34), f(-.8, .62), f(-.1, .87), f(.7, .83), f(1.2, .54), f(.6, .25)]
    D.path(cr(inner), fill=D.lin(name + "-pli", f(-1, .2), f(1.1, .7),
                              [(0, "#1A1517"), (.5, "#3F3337"), (1, "#161215")]))
    D.path(cr([f(-.6, .12), f(-.9, .4), f(-.5, .68), f(.1, .86)], False), stroke="#74676B", sw=.16, so=.36,
           cap="round")
    hair(EP.buffer(-.3), 280 if near else 140, .65, norm((dx * .1, -1)), "#787276", "#090809", .35, .06)
    D.pivot(name, base)
    D.close()


LIP = hs([(17.5, 5.6), (17.6, 4.9), (17.9, 4.05), (18.9, 3.7), (20, 3.75), (21.7, 3.65),
          (21.8, 4.5), (21.35, 5.25), (19.8, 5.4)])


def head():
    D.open("head")
    ear(False)
    jaw()
    d = runs_path([hs(TOP), hs(MOUTH), hs(NOTCH), hs(LOW)])
    D.path(d, fill="url(#tete)")
    glow("front-lustre", H(3, -1), 3, 1.3, "#97969C", .28, -AXE, HP)
    glow("chanfrein-lustre", H(13, -.1), 5.4, 1.0, "#92929D", .38, -AXE, HP)
    glow("joue-ronde", H(7.8, 5.5), 3.3, 2, "#656267", .24, -AXE, HP)
    glow("dessous-joue", H(8, 7.8), 5, 1.2, "#000000", .55, -AXE, HP)
    # Une crête faciale discrète et le masséter, sans dessin de masque.
    D.path(cr(hs([(3.4, 4.2), (6, 3.8), (9, 3.7), (12, 3.5)]), False), stroke="#8C858B", sw=.23, so=.26,
           cap="round")
    hair(HP.buffer(-.25).difference(AP.buffer(.4)).difference(Polygon(LIP).buffer(.4)), 2100, .52,
         mul(U, -.9), "#767378", "#070708", .28, .055)
    # Naseau oblique, truffe nue arrondie, philtrum et lèvres noires.
    nose = hs([(18, 1.8), (19.5, 1.1), (20.7, 1.9), (21.6, 3.2), (21.8, 4.1), (21.5, 4.9),
               (20, 5), (18.3, 4.3), (17.8, 3)])
    D.path(cr(nose), fill=D.rad("mufle", H(20.2, 3.5), 2.8, [(0, "#4E474D", .9), (.65, "#272328", .85),
                                                            (1, "#272328", 0)]))
    nostril = hs([(18.6, 2.8), (19.4, 2.7), (20.2, 2.9), (21.1, 3.6), (21.1, 4),
                  (20.7, 4.1), (20.1, 3.7), (19.4, 3.3)])
    D.path(cr(nostril), fill="#070608")
    D.path(cr(hs([(19.5, 3.2), (20.1, 3.5), (20.7, 3.85)]), False), stroke="#8B8187", sw=.12, so=.5,
           cap="round")
    D.path(cr(hs([(21.5, 4.2), (21.5, 4.8), (21.3, 5.2)]), False), stroke="#070607", sw=.13, so=.8)
    D.path(cr(hs(MOUTH), False), stroke="#060506", sw=.2, so=.95, cap="round")
    for i in range(13):
        p = H(R.uniform(16, 20), R.uniform(3.3, 5))
        D.path(cr(ellipse(p, .035, .045, n=6)), fill="#9F9299", fo=.35)
    eye()
    # L'oreille proche retombe derrière l'œil sans le masquer.
    ear(True)
    D.pivot("head", NUQUE)
    D.pivot("muzzle", H(21.15, 5.45))
    D.close()


def articulate(svg):
    """Les quatre pattes en deux segments, la lèvre avec sa gencive cachée."""
    root = ET.fromstring(svg)
    paints = A.Paints(root)
    for name, front, _, _ in LEGS:
        joint, half = A.leg_joint(svg, name, S((0, CARPE if front else JARRET))[1])
        A.split_leg(root, paints, name, joint, half + .35)
    gum = A.gum([S(p) for p in hs([(17.9, 4.9), (18.2, 4.3), (19.1, 4.1), (20.5, 4.1),
                                  (21, 4.4), (20.6, 4.8), (19.3, 5)])],
                [S(p) for p in hs([(18.4, 4.75), (19.3, 4.6), (20.4, 4.5)])], "#955D69", "#63323F", .17)
    A.lift_part(root, paints, "head", [S(p) for p in LIP], "lip", S(H(17.8, 4.25)), under=gum)
    ET.indent(root, space="  ")
    return ET.tostring(root, encoding="unicode") + "\n"


def main(path):
    D.lin("fond-toison", (0, 74), (0, 28), [(0, "#7E7463"), (.35, "#655A4B"),
                                           (.7, "#463D33"), (1, "#28231F")])
    D.lin("tete", H(7, -3), H(7, 8), [(0, "#3A393E"), (.28, "#262529"),
                                      (.6, "#1A191B"), (1, "#0A090A")])
    D.lin("q-noir", (-7, 30), (0, 26), [(0, "#0E0D0F"), (.35, "#363236"), (1, "#171517")])
    for name, front, dx, far in LEGS:
        leg(name, front, dx, far)
    neck()
    tail()
    body()
    head()
    D.pivot("ground", (41, 0))
    comment = ("  Karakul : brebis noire grisonnante, quatre ans, avant la tonte de septembre.\n"
               "  66 cm au garrot ; 1 unité = 1 cm réel ; tête haute, sans cornes.\n"
               "  Dessin original écrit par tools/races/karakul/gen_karakul.py d'après\n"
               "  docs/races/karakul.md ; contrat docs/rig.md.\n"
               "  © 2026 Alejandro San Martin Lamas — MIT OR Apache-2.0")
    svg = articulate(D.render(139, 98, comment))
    if VARIANTE:
        return variante(path, svg)
    path = Path(path)
    path.parent.mkdir(parents=True, exist_ok=True)
    path.write_text(svg)
    print(f"{path} : {len(svg) / 1024:.0f} Ko, {svg.count('<path')} tracés")


def variante(path, svg):
    """Les autres dessins : ce qui change, puis les proportions."""
    root = ET.fromstring(svg)
    if VARIANTE == "belier":
        # Les cornes du bélier (§ 6.2) : une spirale ouverte, qui part de la
        # nuque vers l'arrière, descend derrière l'oreille et revient vers
        # l'avant en s'écartant de la joue.
        px, py = variantes.pivots(root)["head"]
        for loin in (True, False):
            d = (1.2, -.8) if loin else (0.0, 0.0)
            axe = variantes.spirale((px + 2.2 + d[0], py - .6 + d[1]), (px - 2.6 + d[0], py + 5.6 + d[1]),
                            1.08, 9.2, sens=-1, n=34)
            variantes.corne(root, axe, 3.1 if not loin else 2.8, clair="#B9AE9C", moyen="#857767",
                    sombre="#3B332C", loin=loin, avant="ear-near")
        variantes.bourses(root, *variantes.sous_le_ventre(root, 6, -6), largeur=9, hauteur=15,
                  peau="#39343A", ombre="#151315")
    if VARIANTE == "agneau":
        # Le noir lustré de l'agneau Karakul, avant qu'il ne grisonne.
        variantes.recolorer(root, variantes.rampe("#09080A", "#3C383D", bas=.1, haut=.8),
                    dans=("body", "neck", "tail"))
    variantes.ecrire(path, root, **PROPORTIONS[VARIANTE])


if __name__ == "__main__":
    main(sys.argv[1] if len(sys.argv) > 1 else HERE.parent.parent.parent / "pets/karakul/karakul.svg")
