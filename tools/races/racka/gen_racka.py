#!/usr/bin/env python3
"""Un bélier Racka blanc de cinq ans, en pleine toison de mars.

Dessin original d'après nature, suivant docs/races/racka.md : une unité
vaut un centimètre ; garrot osseux de 72 cm, toison à 82 cm. Le profil
strict raccourcit les deux vis de 55 cm à 43 cm : leur V se superpose,
contrairement à une vue de face. Les photographies servent seulement de
référence et ne sont jamais incorporées au SVG.

Usage : python3 tools/races/racka/gen_racka.py pets/racka/racka.svg
Les outils de courbes sont adaptés de ceux de la Lacaune du dépôt.
© 2026 Alejandro San Martin Lamas — MIT OR Apache-2.0
"""
import math
import random
import sys
from pathlib import Path
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
    "brebis": dict(  # Brebis Racka : 66 cm au garrot, cornes vrillées plus fines d'une trentaine de centimètres, tête fine au chanfrein droit, mamelle nue.
        garrot=(72, 66), encolure=(1.0, .86), tete=(.97, .92)),
    "agneau": dict(  # Agneau Racka de deux semaines : 40 cm au garrot, né brun, sans cornes (de simples bourgeons).
        variantes.AGNEAU, garrot=(72, 40), corps=(.84, .8)),
}
from outils import (Doc, S, add, cr, dense, element, geom_d, lerp, mix,  # noqa: E402
                    mul, poly, qstroke, shade, Polygon, Point)

RNG = random.Random(1983)  # année de fondation de l'association hongroise
D = Doc()
POLL = (100.0, 96.0)
NECK_BASE = (85.0, 51.0)
HU = (math.sqrt(.5), -math.sqrt(.5))
HV = (-math.sqrt(.5), -math.sqrt(.5))
FLEECE, FLEECE_LT, FLEECE_SH = '#DCCFB6', '#F3ECDF', '#A8977A'
ROUX, ROUX_LT, ROUX_SH = '#B0703F', '#E2A97A', '#6E3A1E'


def HL(u, v):
    """Repère de la tête : u vers le museau, v vers la ganache."""
    return add(POLL, add(mul(HU, u), mul(HV, v)))


def hs(points):
    return [HL(*p) for p in points]


def ellipse(c, rx, ry, n=24):
    return [(c[0] + rx * math.cos(2 * math.pi * i / n),
             c[1] + ry * math.sin(2 * math.pi * i / n)) for i in range(n)]


def hair(region, density, direction, light, dark, length=(.5, 1.2)):
    """Poil ras, limité géométriquement : ni masque ni découpe SVG."""
    x0, y0, x1, y1 = region.bounds
    D.begin_batch()
    for _ in range(int(region.area * density)):
        p = (RNG.uniform(x0, x1), RNG.uniform(y0, y1))
        L = RNG.uniform(*length)
        tip = add(p, mul(direction, L))
        if region.contains(Point(p)) and region.contains(Point(tip)):
            bend = add(lerp(p, tip, .5), (.07, 0))
            D.stroke(qstroke(p, bend, tip, 2), light if RNG.random() < .6 else dark,
                     .05, .2)
    D.flush()


def cord(x, h, length, width, light=.65, bend=0, clip=None):
    """Une corde de jarre ondulée, irrégulière, pointue ; jamais une boucle
    ronde. Les sillons suivent la longueur ; leur pas et leur courbure varient.
    Les racines sont recouvertes par la rangée suivante, posée au-dessus.
    """
    phase = RNG.uniform(-math.pi, math.pi)
    pitch = RNG.uniform(4.5, 8.0)
    tip_curl = RNG.uniform(-.5, .5)

    def center(t):
        wiggle = (math.sin(phase + length * t / pitch * 2 * math.pi + .5 * math.sin(7 * t))
                  + .28 * math.sin(phase * 2 + 17 * t))
        return (x + bend * t + width * .23 * wiggle * (.25 + .75 * t)
                + tip_curl * t ** 7, h - length * t)

    def radius(t):
        return width * .5 * (.84 + .16 * math.sin(phase + 9 * t)) * (1 - t ** 3.4) + .02

    steps = max(24, int(length * 3))
    left, right = [], []
    for i in range(steps + 1):
        t = i / steps
        c = center(t)
        w = radius(t)
        left.append((c[0] - w, c[1]))
        right.append((c[0] + w, c[1]))
    cap = [(x + radius(0) * math.cos(math.pi * k / 8),
                h + radius(0) * .8 * math.sin(math.pi * k / 8)) for k in range(1, 8)]
    region = Polygon(left + right[::-1] + cap).buffer(0)
    if clip is not None:
        region = region.intersection(clip)
    if region.is_empty:
        return
    # L'ombre au bord, la lumière sur la crête : chaque corde a une vraie
    # épaisseur ; le bas roussi est plus mat que la racine blanche.
    # Quelques doubles cordes plus foncées, nouées par le suint,
    # surtout dans la frange et sous la gorge.
    if h < 55 and RNG.random() < .2:
        light -= RNG.uniform(.12, .25)
    base = mix('#8C7859', '#F0E7D2', light)
    gid = f'corde-{len(D.ids)}'
    fill = D.lin(gid, (x - width * .5, h), (x + width * .5, h),
                 [(0, shade(base, .82)), (.23, base), (.49, shade(base, 1.09)),
                  (.76, base), (1, shade(base, .8))])
    shape = geom_d(region, simplify=.018)
    D.path(shape, fill=fill)
    D.path(shape, fill='url(#suint)')
    D.path(shape, fill='url(#ombre-bavette)' if x > 88 else 'url(#volume-flanc)')
    D.begin_batch()
    # Quelques fibres fines, avec des longueurs différentes ; pas un motif
    # régulier transversal, qui ferait la boucle serrée d'une autre race.
    for j in range(5):
        offset = (j - 2) * .15
        points = []
        for k in range(28):
            t = .04 + .91 * k / 27
            c = center(t)
            p = (c[0] + offset * width * (1 - t ** 2)
                 + .055 * width * math.sin(11 * t + j * 1.8), c[1])
            if region.contains(Point(p)):
                points.append(p)
            elif len(points) > 1:
                D.stroke(cr(points, closed=False), '#F7F0E0' if j % 2 else '#5A4B36',
                         .06 if j % 2 else .075, .4 if j % 2 else .16)
                points = []
        if len(points) > 1:
            D.stroke(cr(points, closed=False), '#F7F0E0' if j % 2 else '#5A4B36',
                     .06 if j % 2 else .075, .4 if j % 2 else .16)
    D.flush()
    # Un ou deux poils libres autour de la pointe, plus fins que le corps
    # de la corde ; ils cassent l'aspect de languettes découpées.
    end = center(.95)
    for k in range(2):
        tip = add(end, (RNG.uniform(-.5, .5), -RNG.uniform(.5, 1.0)))
        if clip is None or clip.contains(Point(tip)):
            D.path(qstroke(add(end, (.12 * k, .3)), add(end, (.3, -.4)), tip, 2),
                   stroke=shade(base, .91), sw=.065, so=.65, cap='round')


def underwool(reg, mark=None, aside=None):
    """Le feutre ondulé derrière le jarre, aussi dans les réserves cachées.
    `mark(h, x)` : le numéro de la nappe d'encolure d'une corde (`h` : la
    hauteur de son rang, `x` : sa racine), ou rien. Les cordes d'une nappe
    sont mises de côté dans `aside` (par nappe, dans leur ordre) : `neck`
    les pose ensuite, par nappe."""
    x0, y0, x1, y1 = reg.bounds
    h = y0 + 7
    while h < y1 + 9:
        x = x0
        while x < x1:
            xx, hh = x + RNG.uniform(-1, 1), h + RNG.uniform(-3, 3)
            if reg.intersects(Point(xx, hh).buffer(3)):
                k = mark(h, xx) if mark else None
                D.flush()
                start = len(D.out)
                cord(xx, hh, RNG.uniform(9, 16), RNG.uniform(2.1, 3.5),
                     .48 + .4 * (h - y0) / max(1, y1 - y0) + RNG.uniform(-.12, .08),
                     RNG.uniform(-1.5, 1), reg)
                if k:
                    D.flush()
                    aside.setdefault(k, []).extend(D.out[start:])
                    del D.out[start:]
            x += 2.2
        h += 7.5


def leg(name, x, front, far):
    """Deux pièces à recouvrement circulaire au carpe / jarret. Les bas
    de pattes sont dessinés sous les hauts, sans trait sur la jonction.
    """
    jh = 16.0 if front else 25.0
    jx = x if front else x - 3.8
    hip = (x - .5 if front else x + 5, 46 if front else 49)
    tint = .79 if far else 1
    D.open(name)
    lower = name + '-lower'
    D.open(lower)
    points = [(jx - 1.4, jh), (jx - 1.0, jh + 1.8), (jx + 1.1, jh + 1.65),
              (jx + 1.55, jh), (jx + 1.3, 8), (jx + 1.9, 5.2),
              (jx + 3, 2.6), (jx + 2.5, 1.5), (jx - .3, 2.0),
              (jx - 1.0, 4), (jx - 1.15, 6), (jx - 1.05, 10)]
    reg = Polygon(dense(points)).buffer(0)
    fill = D.lin(name + '-canon', (jx - 1.6, 10), (jx + 1.6, 10),
                 [(0, shade('#3A2A1E', tint)), (.25, shade('#A37E64', tint)),
                  (.6, shade('#7A5A3E', tint)), (1, shade('#4B301F', tint))])
    D.path(cr(points), fill=fill)
    # Un glacis roux commun aux deux pièces conserve la couleur au
    # raccord du carpe ou du jarret, puis se fond vers le brun du paturon.
    D.path(cr(points), fill=D.lin(name + '-raccord', (0, jh+1), (0, 6),
                                 [(0, shade('#A46C43', tint)), (.25, shade('#A46C43', tint), .8),
                                  (1, shade('#A46C43', tint), 0)]))
    D.path(cr([(jx - .8, jh - 1.4), (jx - .6, 10), (jx - .5, 6.2)], False),
           stroke=shade('#49301F', tint), sw=.21, so=.5, cap='round')
    hair(reg.buffer(-.14), 4, (.08, -1), shade('#CCA080', tint), shade('#402C1D', tint))
    hoof = [(jx - .4, 3.3), (jx + 2, 3.1), (jx + 3.8, .75), (jx + 3.7, .03),
            (jx - 1.0, .03), (jx - 1.2, .6), (jx - .9, 2.0)]
    g = D.lin(name + '-onglons', (jx - .5, 1.4), (jx + 3.7, 1.4),
              [(0, shade('#2A1E16', tint)), (.45, shade('#8E7258', tint)),
               (1, shade('#5E4632', tint))])
    D.path(cr(hoof, t=.5), fill=g)
    D.path(cr([(jx + 2.0, .08), (jx + 1.8, 1.1), (jx + 1.45, 2.5)], False),
           stroke=shade('#2A1E16', tint), sw=.15, so=.9, cap='round')
    D.path(cr([(jx - 1.1, 4.3), (jx - 1.9, 4.0), (jx - 1.65, 3.35),
               (jx - 1.05, 3.7)]), fill=shade('#4F3725', tint))
    D.pivot(lower, (jx, jh))
    D.close()
    if front:
        upper = [(x - 4.0, 49), (x + 4.5, 48), (x + 3.3, 36),
                 (x + 1.55, 20), (jx + 1.5, jh), (jx + 1.05, jh - 1.25),
                 (jx - .85, jh - 1.3), (jx - 1.5, jh), (x - 1.65, 25), (x - 3, 37)]
    else:
        upper = [(x + 1, 54), (x + 11, 52), (x + 10, 43), (x + 3, 32),
                 (jx + 1.8, jh), (jx + 1.0, jh - 1.5), (jx - 1.4, jh - 1.4),
                 (jx - 1.9, jh + .5), (x - 2.5, 32), (x - .5, 43)]
    reg = Polygon(dense(upper)).buffer(0)
    g = D.lin(name + '-haut', (x - 3, 34), (x + 7, 34),
              [(0, shade(ROUX_SH, tint)), (.4, shade('#C99068', tint)),
               (.7, shade('#A06A42', tint)), (1, shade(ROUX_SH, tint))])
    D.path(cr(upper), fill=g)
    D.path(cr(upper), fill=D.lin(name + '-raccord-haut', (0, jh+8), (0, jh+1),
                                [(0, shade('#A46C43', tint), 0), (.75, shade('#A46C43', tint), .9),
                                 (1, shade('#A46C43', tint))]))
    hair(reg.buffer(-.2), 2.7, (.1, -1), shade(ROUX_LT, tint), shade(ROUX_SH, tint))
    if not front:
        # La laine de la culotte descend sur le tibia jusqu'au jarret ;
        # elle masque le haut roux, mais le canon reste à poil ras.
        woolreg = reg.intersection(Polygon([(-30, jh-.5), (160, jh-.5), (160, 58), (-30, 58)]))
        for wx in [x-1.7, x+.1, x+2.0, x+4.0, x+6.0]:
            cord(wx, 42+RNG.uniform(-2, 2), RNG.uniform(13, 17),
                 RNG.uniform(1.7, 2.7), .45 if far else .55, -2.8, woolreg)
    D.pivot(name, hip)
    D.close()


def tail():
    """Queue entière, naturelle, attachée bas ; pointe rousse à 14 cm,
    nettement sous le jarret de 25 cm, articulée en trois morceaux."""
    D.open('tail')
    D.open('tail-mid')
    D.open('tail-tip')
    pts = [(-4.9, 32), (-2, 32), (-1.6, 24), (-1.8, 17), (-3, 13.5),
           (-4.7, 14.5), (-5.3, 23)]
    D.path(cr(pts), fill=D.lin('queue-bout', (-5, 30), (-3, 13),
                             [(0, '#C2AE8C'), (.48, '#B89A75'), (1, '#94603E')]))
    tailreg = Polygon(dense(pts)).buffer(0)
    for x in [-4.4, -3.6, -2.8]:
        cord(x, 33, RNG.uniform(12, 17), 1.25, .4, -.2, tailreg)
    hair(tailreg.buffer(-.2), 5, (.1, -1), '#D4AE83', '#795035')
    D.pivot('tail-tip', (-3.5, 31))
    D.close()
    mid = [(-4.3, 49), (-.2, 49), (-1.6, 31), (-2.1, 27), (-5.4, 29), (-6.1, 38)]
    D.path(cr(mid), fill='url(#fond-toison)')
    underwool(Polygon(dense(mid)).buffer(0))
    for x in [-4.2, -2.6, -.8]:
        cord(x, 49, RNG.uniform(17, 22), 2, .55, -.5)
    D.pivot('tail-mid', (-2.4, 49))
    D.close()
    root = [(1.7, 69), (4.2, 64), (1, 51), (-.9, 44), (-5.3, 46), (-5.5, 53), (-1.7, 66)]
    D.path(cr(root), fill='url(#fond-toison)')
    underwool(Polygon(dense(root)).buffer(0))
    for x in [-3, -1, 1, 2.3]:
        cord(x, 64, RNG.uniform(16, 20), 2.4, .72, -2)
    D.pivot('tail', (2, 65))
    D.close()


BODY = [(-1, 62), (1, 73), (8, 78), (20, 80), (42, 81), (64, 81.3),
        (78, 82), (88, 77), (96, 64), (97, 46), (89, 36),
        (75, 36), (52, 37), (30, 38), (13, 36), (3, 44)]


# Les trois nappes de l'encolure : ses longues cordes pendent au lieu de
# tourner avec elle. Chaque nappe est un groupe de cordes entières, de la
# racine à la pointe ; son attache, sur l'axe de l'encolure, suit le cou, et
# la nappe pend de là, à la verticale (docs/rig.md, « Les nappes »).
#   1. le poitrail : la bavette, qui tombe à mi-canon, son feutre et son
#      fond ; son attache est la base de l'encolure, elle ne bouge pas quand
#      il broute (c'est la laine du poitrail) ;
#   2. le milieu ; 3. le haut, sous la nuque : les longues cordes.
# (cordes jusqu'à cette hauteur, attache). Le feutre du cou, court, reste à
# l'encolure et tourne avec elle ; derrière `NECK_HIDDEN`, sous le garrot,
# la réserve cachée par le corps aussi : elle fait le dessus du cou baissé.
# Avec le bord du haut de chacune (la ligne d'où elle se balance).
NECK_SHEETS = [(55.0, NECK_BASE, 55.0), (77.0, (90.0, 66.0), 66.0), (200.0, (96.7, 86.0), 97.0)]
NECK_HIDDEN = (86.0, 80.0)


def neck_sheet(h, x):
    """La nappe d'une longue corde, d'après sa racine."""
    if x < NECK_HIDDEN[0] and h < NECK_HIDDEN[1]:
        return None
    for k, (top, _, _) in enumerate(NECK_SHEETS, 1):
        if h < top:
            return k
    return None


def chest_felt(h, x):
    """Le feutre du poitrail pend avec la bavette ; celui du cou y reste."""
    return 1 if h < NECK_SHEETS[0][0] else None


def neck():
    """Encolure indépendante, large réserve cachée dans l'épaule. La
    bavette tombe en cordes sous la gorge ; le corps recouvre la racine.
    """
    D.open('neck')
    outline = [(76, 60), (77, 79), (88, 90), (95, 97), (100, 98.3),
               (105.5, 92), (107, 78), (110, 62), (110, 42),
               (105, 25), (98, 22), (89, 36), (78, 45)]
    reg = Polygon(dense(outline)).buffer(0)
    # Le fond, en deux : celui du cou, qui tourne avec lui (au-delà de sa
    # base, et un disque autour d'elle : aucune encoche à la rotation), et
    # celui du poitrail, sous la base, qui reste avec la bavette. L'encolure
    # ne balaie plus un fond sombre sous le poitrail en descendant.
    ux, uy = POLL[0] - NECK_BASE[0], POLL[1] - NECK_BASE[1]
    norm = math.hypot(ux, uy)
    ux, uy = ux / norm, uy / norm

    def beyond(a, b):
        """La bande de l'encolure de `a` à `b` le long de son axe."""
        px, py = -uy, ux
        corners = [(NECK_BASE[0] + ux * t + px * w, NECK_BASE[1] + uy * t + py * w)
                   for t, w in ((a, -200), (b, -200), (b, 200), (a, 200))]
        return Polygon(corners)

    upper = reg.intersection(beyond(-2, 200).union(Point(NECK_BASE).buffer(11)))
    chest = reg.intersection(beyond(-200, -1))
    D.path(geom_d(upper, simplify=.01), fill='url(#fond-toison)')
    # Les cordes des nappes sont mises de côté, puis posées par nappe ; le
    # feutre du cou et la réserve restent à l'encolure.
    aside = {1: [element(geom_d(chest, simplify=.01), 'url(#fond-toison)')]}
    underwool(reg, mark=chest_felt, aside=aside)

    def hung(k, *args):
        D.flush()
        start = len(D.out)
        cord(*args)
        D.flush()
        if k:
            aside.setdefault(k, []).extend(D.out[start:])
            del D.out[start:]

    # Les pointes de la bavette tombent jusqu'à mi-canon. Les racines
    # restent dans la masse du cou ; le bord droit n'est pas une coupe.
    for row, (top, low, x0, x1) in enumerate([(42, 16, 91, 106), (61, 36, 85, 108),
                                             (78, 54, 84, 107), (94, 71, 91, 102)]):
        x = x0
        while x < x1:
            w = RNG.uniform(1.6, 2.6)
            hung(neck_sheet(top, x),
                 x, top + RNG.uniform(-2, 2), top - low + RNG.uniform(-3, 3),
                 w, .37 + row * .11, RNG.uniform(-1, 1.6),
                 reg if row else None)
            x += w * .8
    # Courtes mèches roussies à la limite du poil ras, sans bavette coupée.
    for x, h in [(99, 96), (101, 93), (104, 89), (104.3, 85), (104.8, 81)]:
        hung(3, x, h, RNG.uniform(5, 8), 1.4, .32, -.7, reg)
    for k, (_, pivot, edge) in enumerate(NECK_SHEETS, 1):
        D.open(f'neck-sway-{k}')
        D.out.extend(aside.get(k, []))
        D.pivot(f'neck-sway-{k}', pivot)
        D.pivot(f'neck-sway-{k}-top', (pivot[0], edge))
        D.close()
    D.pivot('neck', NECK_BASE)
    D.close()


def body():
    # Les trois franges mobiles sont posées sous le corps, et leurs
    # racines entières restent cachées : aucune boucle n'est coupée.
    for i, (lo, hi) in enumerate([(18, 39), (39, 62), (62, 83)], 1):
        D.open(f'sway-{i}')
        for x in range(lo, hi, 2):
            cord(x + RNG.uniform(-.5, .5), 43 + RNG.uniform(-1, 1),
                 RNG.uniform(18, 23), RNG.uniform(1.8, 2.8),
                 RNG.uniform(.33, .62), RNG.uniform(-1.3, .8))
        D.pivot(f'sway-{i}', ((lo + hi) / 2, 43.5))
        D.close()
    D.open('body')
    reg = Polygon(dense(BODY)).buffer(0)
    D.path(cr(BODY), fill='url(#fond-toison)')
    underwool(reg)
    # Du bas vers le dos : les cordes se chevauchent comme le jarre réel.
    # Une légère obliquité près de la croupe, une raie presque droite au dos.
    for row, (top, low, x0, x1) in enumerate([(49, 27, 2, 94), (65, 42, 0, 94),
                                             (80, 57, 4, 86)]):
        x = x0
        while x < x1:
            w = RNG.uniform(1.7, 3.5)
            h = top + RNG.uniform(-5, 4)
            L = top - low + RNG.uniform(-3, 3)
            bend = RNG.uniform(-1.1, 1.1) - max(0, 15 - x) * .1
            light = .55 + row * .16 + RNG.uniform(-.12, .09)
            cord(x, h, L, w, light, bend, reg if row else None)
            x += w * RNG.uniform(.54, .72)
    # Quelques creux entre doubles cordes, tantôt au soleil, tantôt
    # ombrés : la lumière ne dessine pas un tapis de colonnes identiques.
    for _ in range(95):
        x, h = RNG.uniform(1, 94), RNG.uniform(33, 77)
        pts = [(x, h), (x + RNG.uniform(-.5, .5), h - 3),
               (x + RNG.uniform(-.7, .7), h - RNG.uniform(4, 8))]
        if all(reg.contains(Point(p)) for p in pts):
            D.path(cr(pts, False), stroke='#53432F', sw=RNG.uniform(.08, .18),
                   so=RNG.uniform(.1, .27), cap='round')
    # Le haut est un duvet ouvert en raie et non un dôme de festons.
    for x in range(5, 83):
        h = 78.7 + 2.2 * x / 83
        cord(x, h + RNG.uniform(-.3, .4), RNG.uniform(3.5, 7),
             RNG.uniform(1.0, 1.6), RNG.uniform(.8, .98), -.7, reg)
    # Poils libres sur l'échine : le dessus reste droit, avec une
    # raie et quelques fibres couchées, sans festons de nuage.
    for x in range(5, 83):
        h = 78.7 + 2.2 * x / 83
        a = (x, h-.1)
        c = (x - RNG.uniform(.2, .8), h+RNG.uniform(.1,.8))
        b = (x - RNG.uniform(.8, 1.5), h+RNG.uniform(.0, .7))
        D.path(qstroke(a, c, b), stroke=RNG.choice(['#DED3BD','#F2E9D6','#C9B99B']),
               sw=RNG.uniform(.07,.15), so=.9, cap='round')
    D.pivot('body', (46, 38))
    D.close()


def horn(far):
    """Projection d'une lame vrillée de 4,5 tours autour d'un axe droit.
    Le V est dans le plan frontal : le profil superpose ses deux axes.
    Les deux arêtes hélicoïdales alternent la face claire et le chant brun.
    """
    base = (101.7, 99.0) if not far else (100.6, 99.6)
    length = 43.5 if not far else 42.3
    gros = 2.9
    if VARIANTE == "brebis":
        # Plus que la longueur de la tête, une trentaine de centimètres,
        # plus fines, la même vrille (§ 1.4).
        length, gros = length * .66, 1.9
    axis = (.075, .9972)
    normal = (-axis[1], axis[0])
    phase0 = .5 if not far else -.5
    handed = 1 if not far else -1
    tint = 1 if not far else .78

    def frame(t):
        ph = phase0 + handed * 9 * math.pi * t
        width = gros * (1 - t) ** .7 + .025
        r = width * math.sqrt(math.cos(ph) ** 2 + (.58 * math.sin(ph)) ** 2)
        c = add(base, add(mul(axis, length * t), mul(normal, .23 * math.sin(math.pi * t))))
        # Carène visible, mobile de gauche à droite en suivant la torsion.
        ridge = width * .82 * math.sin(ph)
        ridge = max(-r * .93, min(r * .93, ridge))
        return c, r, ridge, ph

    outline_a, outline_b = [], []
    for k in range(241):
        c, r, _, _ = frame(k / 240)
        outline_a.append(add(c, mul(normal, r)))
        outline_b.append(add(c, mul(normal, -r)))
    D.path(poly(outline_a + outline_b[::-1]), fill=shade('#8C7052', tint))
    # Deux faces raccordées sans jours ; les couleurs tournent avec la
    # lame, elles ne font pas des bandes peintes sur un tube cylindrique.
    for k in range(240):
        t, t1 = k / 240, (k + 1.02) / 240
        c, r, ridge, ph = frame(t)
        c1, r1, ridge1, _ = frame(min(t1, 1))
        for sign in [-1, 1]:
            face = [add(c, mul(normal, ridge)), add(c, mul(normal, sign * r)),
                    add(c1, mul(normal, sign * r1)), add(c1, mul(normal, ridge1))]
            light = .45 + .42 * math.cos(ph + sign * .75)
            col = mix('#775A38', '#F2DDB4', max(.05, light))
            D.path(poly(face), fill=shade(col, tint))
    # L'arête sombre, interrompue lorsqu'elle passe derrière la face.
    run = []
    for k in range(241):
        t = k / 240
        c, _, ridge, ph = frame(t)
        if math.cos(ph) > -.8:
            run.append(add(c, mul(normal, ridge)))
        elif run:
            if len(run) > 1:
                D.path(poly(run, False), stroke=shade('#5A4030', tint), sw=.14,
                       so=.8, cap='round')
            run = []
    if len(run) > 1:
        D.path(poly(run, False), stroke=shade('#5A4030', tint), sw=.1, so=.75)
    # Annelures fines, ondulées, plus profondes à la base ; pointe lisse.
    for k in range(1, 57):
        t = k / 63
        c, r, ridge, ph = frame(t)
        a, b = add(c, mul(normal, r)), add(c, mul(normal, -r))
        mid = add(add(c, mul(normal, ridge)), mul(axis, -.25 * (1 - t)))
        D.path(qstroke(a, mid, b, 2), stroke=shade('#7B5D3E', tint),
               sw=.09 + .07 * (1 - t), so=.38, cap='round')
        a, b = add(a, mul(axis, .1)), add(b, mul(axis, .1))
        D.path(qstroke(a, add(mid, mul(axis, .1)), b, 2), stroke=shade('#F9E9C8', tint),
               sw=.065, so=.5, cap='round')


def ear(far):
    name = 'ear-far' if far else 'ear-near'
    base = HL(.6, .9) if not far else HL(1.6, -1.1)
    length = 6.7 if not far else 5.0
    direction = (-1, .04) if not far else (.9, .18)
    normal = (-direction[1], direction[0])

    def ep(u, v):
        return add(base, add(mul(direction, u), mul(normal, v)))

    outline = [ep(-.6, 0), ep(.8, -1), ep(3.5, -1.35), ep(length, -.25),
               ep(length + .1, .05), ep(length - 1.2, 1.15), ep(2.3, 1.2), ep(.2, .7)]
    D.open(name)
    g = D.lin(name + '-robe', ep(0, -1), ep(0, 1),
              [(0, '#CB976F' if not far else '#96603D'), (.4, '#B0703F'), (1, '#6E3A1E')])
    D.path(cr(outline, t=.8), fill=g)
    if not far:
        D.path(cr([ep(.8, .25), ep(2.5, -.2), ep(5.2, -.1), ep(5.7, .2),
                   ep(3.3, .85), ep(1.3, .7)]), fill='#7A4A34')
        D.path(cr([ep(.9, -.6), ep(3.7, -.85), ep(5.9, -.25)], False),
               stroke='#E2A97A', sw=.15, so=.7)
    hair(Polygon(dense(outline)).buffer(-.15), 4, direction, '#DCA478', '#6E3A1E', (.2, .65))
    D.pivot(name, base)
    D.close()


DORSAL = [(-2, 0), (-1, -2), (2, -3.2), (7, -4.0), (12, -4.2),
          (16, -4), (20, -3.1), (23, -1.2), (25.6, .6), (26.45, 2.25), (26.2, 3.65), (25.4, 4.15)]
MOUTH = [(25.4, 4.15), (23, 4.5), (20.7, 4.65)]
NOTCH = [(20.7, 4.65), (19, 5.8), (17.5, 6.8)]
UNDER = [(17.5, 6.8), (13, 6.8), (8, 6.4), (3.5, 5.6), (.5, 3.8), (-2, 0)]


def head():
    D.open('head')
    if VARIANTE != "agneau":
        horn(True)
    ear(True)
    D.open('jaw')
    jaw = hs([(5, 5), (12, 4.6), (20, 4.2), (24.8, 4.1), (25.6, 5.0),
              (23.6, 5.6), (18, 6.6), (12, 7.25), (7, 6.9), (3.4, 5.2)])
    D.path(cr(jaw), fill='url(#joue)')
    D.path(cr(hs([(17, 5), (23, 4.0), (25, 4.15), (25.1, 4.4), (21, 4.6), (17, 5.1)])),
           fill='#50302A')
    D.path(cr(hs([(20, 4.3), (23.1, 4.15), (24.9, 4.3), (24, 4.5), (20.5, 4.6)])),
           fill='#C58B87')
    D.path(cr(hs([(16, 6.5), (21, 5.55), (24.8, 4.9)]), False),
           stroke='#E5B08B', sw=.21, so=.5)
    D.pivot('jaw', HL(8.5, 5.5))
    D.close()
    outline = hs(DORSAL + MOUTH[1:] + NOTCH[1:] + UNDER[1:])
    reg = Polygon(dense(outline)).buffer(0)
    D.path(cr(outline), fill='url(#face)')
    # Joues plus rousses, chanfrein finement busqué et éclairé.
    cheek = hs([(3, 1), (9, .6), (14, 3), (17, 6.7), (11, 6.5), (5, 5.3)])
    D.path(cr(cheek), fill='url(#joue)')
    D.path(cr(hs([(7, -3.6), (12, -3.8), (17, -3.2), (21, -1.7)]), False),
           stroke='#ECC09A', sw=.3, so=.38, cap='round')
    hair(reg.buffer(-.2), 6, HU, '#F0B889', '#75401F', (.25, .65))
    # Dépression lacrymale devant l'œil, sans masque ni lunette.
    D.path(cr(hs([(10, 1.4), (11.4, 2.1), (12.4, 3.2)]), False),
           stroke='#72401F', sw=.25, so=.65, cap='round')
    D.path(cr(hs([(10.35, 1.6), (10.9, 2.45), (11.45, 2.9)]), False),
           stroke='#D49A68', sw=.09, so=.5, cap='round')
    nose = hs([(22.9, .3), (25.8, 1.05), (26.4, 2.7), (26.1, 3.8), (25.3, 4.1),
               (22, 4.5), (21.9, 3.1)])
    D.path(cr(nose), fill=D.lin('museau', HL(23, .4), HL(23, 4.5),
                              [(0, '#D7AE96'), (.5, '#C79C88'), (1, '#8A6656')]))
    D.path(cr(hs([(23.4, 1.35), (24.3, 1.15), (25.0, 1.9), (24.8, 2.7),
                   (23.8, 3.1), (23.2, 2.9), (24, 2.2)])), fill='#3A2A28')
    D.path(cr(hs([(24.15, 1.45), (24.7, 1.75), (24.65, 2.5)]), False),
           stroke='#E5C0B9', sw=.18, so=.65, cap='round')
    for p, r in [((25, 3.2), .16), ((23, 3.6), .12), ((25.2, 2.8), .08), ((22.5, 3.3), .1)]:
        D.path(cr(ellipse(HL(*p), r, r * .6, 10)), fill='#5D3C32', fo=.75)
    D.path(cr(hs([(21.8, 4.95), (23.6, 4.8), (25.4, 4.4)]), False),
           stroke='#D2A18B', sw=.18, so=.75, cap='round')
    D.path(cr(hs(MOUTH), False), stroke='#513128', sw=.13, so=.8)
    eye()
    if VARIANTE != "agneau":
        horn(False)
    # Touffe localisée entre les cornes : la face demeure rase.
    for x, h in [(98.5, 98.7), (100, 98.7), (101.6, 98.5), (102.4, 98.0)]:
        p = [(x - .8, h - .8), (x - .9, h + .8), (x - .3, h + 1.1),
             (x + .1, h + .4), (x + .8, h + .6), (x + .7, h - .9)]
        D.path(cr(p), fill='#AD7D58')
        D.path(qstroke((x-.3,h-.4),(x-.6,h+.6),(x+.2,h+.4)), stroke='#D1AC86', sw=.1, so=.7)
    ear(False)
    D.pivot('head', POLL)
    D.pivot('muzzle', HL(25.3, 5.0))
    D.close()


def eye():
    center = HL(7.6, .25)
    # Le trait de paupière fermée reste sous le globe et ne bouge pas.
    closed = [add(center, (-1.5, 0)), add(center, (0, -.3)), add(center, (1.45, -.05))]
    D.path(cr(closed, False), stroke='#6F3D26', sw=.16, so=.65)
    D.open('eyes')
    almond = [add(center, p) for p in [(-1.65, .0), (-.8, .9), (.3, 1.0), (1.55, .2),
                                      (1.45, -.25), (.4, -.72), (-.75, -.65)]]
    D.path(cr(almond), fill='#492D1E')
    D.open('pupils')
    D.path(cr(ellipse(center, 1.05, .89)),
           fill=D.rad('iris', add(center, (-.2, .2)), 1.2,
                      [(0, '#C99A5C'), (.55, '#A8763E'), (1, '#4A3020')]))
    for i in range(32):
        a = 2 * math.pi * i / 32
        p = add(center, (.61 * math.cos(a), .51 * math.sin(a)))
        q = add(center, (.98 * math.cos(a + .06), .81 * math.sin(a + .06)))
        D.path(poly([p, q], False), stroke='#E4BB77' if i % 2 else '#5E3D20', sw=.045, so=.55)
    D.path(cr([add(center, p) for p in [(-.79, .2), (.8, .2), (.94, .04),
                                      (.78, -.2), (-.78, -.2), (-.94, -.02)]], t=.35), fill='#0E0C0A')
    D.pivot('pupils', center)
    D.close()
    D.path(cr(ellipse(add(center, (-.4, .44)), .24, .13, 12)), fill='#FFF8E9', fo=.9)
    D.path(cr(almond[:4], False), stroke='#6C4131', sw=.2, so=.85)
    D.path(cr(almond[3:] + [almond[0]], False), stroke='#C69A83', sw=.14, so=.8)
    D.pivot('eyes', center)
    D.close()


def articulate(text):
    """Détache la lèvre supérieure, avec le bourrelet rose qu'elle cache, et
    pose les trois touffes de la gorge. Les autres articulations sont
    dessinées directement dans leurs groupes.
    """
    parser = ET.XMLParser(target=ET.TreeBuilder(insert_comments=True))
    root = ET.fromstring(text, parser=parser)
    paints = A.Paints(root)
    lip = hs([(22.1, 3.3), (26.6, 3), (27, 5.5), (21, 5.5), (21.8, 4.2)])
    pad = hs([(22.3, 4.3), (23, 3.7), (25.1, 3.55), (25.8, 3.9), (25.2, 4.35), (23.4, 4.5)])
    ridge = hs([(22.8, 4.15), (24, 4.05), (25.1, 3.97)])
    under = A.gum([S(p) for p in pad], [S(p) for p in ridge], '#D19A99', '#925A63', .17)
    A.lift_part(root, paints, 'head', [S(p) for p in lip], 'lip', S(HL(22.4, 3.8)), under=under)
    ET.indent(root, space='  ')
    return ET.tostring(root, encoding='unicode') + '\n'


def main(path):
    D.lin('suint', (0, 83), (0, 18), [(0, '#8B7052', 0), (.45, '#8B7052', 0),
                                         (.75, '#705133', .1), (1, '#5A4226', .34)])
    D.lin('ombre-bavette', (97, 75), (104, 24), [(0, '#493420', 0), (.45, '#493420', .02),
                                                       (1, '#493420', .38)])
    D.rad('volume-flanc', (45, 65), 48, [(0, '#695338', 0), (.62, '#695338', .015),
                                                       (1, '#695338', .17)], sx=1.4)
    D.lin('fond-toison', (0, 83), (0, 22), [(0, '#D9CCB3'), (.38, '#B9A586'),
                                         (.76, '#6B573D'), (1, '#4A3E2C')])
    D.lin('face', HL(12, -4), HL(12, 8), [(0, '#DC9D6B'), (.33, ROUX),
                                        (.7, '#A36539'), (1, ROUX_SH)])
    D.lin('joue', HL(5, -1), HL(11, 9), [(0, '#B87B4B'), (.6, '#9B592F'), (1, '#6E3A1E')])
    leg('leg-back-far', 21, False, True)
    leg('leg-front-far', 75, True, True)
    tail()
    leg('leg-back-near', 12, False, False)
    leg('leg-front-near', 82, True, False)
    neck()
    body()
    head()
    D.pivot('ground', (48, 0))
    comment = ('Racka : bélier blanc adulte en pleine toison de fin mars.\n'
               '  1 unité = 1 cm ; 72 cm au garrot osseux, 82 cm avec laine.\n'
               '  Deux vis droites en V frontal, superposées dans le profil strict.\n'
               '  Dessin original écrit par tools/races/racka/gen_racka.py.\n'
               '  © 2026 Alejandro San Martin Lamas — MIT OR Apache-2.0')
    svg = articulate(D.render(139, 158, comment))
    if VARIANTE:
        return variante(path, svg)
    Path(path).parent.mkdir(parents=True, exist_ok=True)
    Path(path).write_text(svg, encoding='utf-8')
    print(f'{path} : {len(svg) / 1024:.0f} Ko, {svg.count("<path")} tracés')


def variante(path, svg):
    """Les autres dessins : ce qui change, puis les proportions."""
    root = ET.fromstring(svg)
    if VARIANTE == "brebis":
        variantes.mamelle(root, *variantes.sous_le_ventre(root, 10, -6), largeur=10, hauteur=8,
                  peau="#D9A892", ombre="#A8765F")
    if VARIANTE == "agneau":
        # Né brun, très souvent tout entier : le brun pâlit en fauve, puis
        # la toison devient crème (§ 1.6).
        variantes.recolorer(root, variantes.teinte("#A87650", .5), dans=("body", "neck", "tail", "sway-1", "sway-2", "sway-3"),
                    si=lambda c: variantes.clarte(c) > .4)
    variantes.ecrire(path, root, **PROPORTIONS[VARIANTE])


if __name__ == '__main__':
    main(sys.argv[1] if len(sys.argv) > 1 else str(HERE.parents[2] / 'pets/racka/racka.svg'))
