#!/usr/bin/env python3
"""Dessine une brebis Zwartbles adulte en début d'automne pour Mouflon.

Une unité = un centimètre de l'animal. Repère : x vers l'avant, h vers le
haut, sol h=0 ; outils.S applique seulement la translation et retourne h.
80 cm au garrot sous une repousse de 4 à 6 cm ; tête de 30 cm, longue queue
non coupée de 50 cm. Les choix viennent de docs/races/zwartbles.md.

Les parties sont dessinées avec leurs recouvrements dès le départ : le cou
derrière le corps, les canons derrière les avant-bras, les trois segments
de la queue emboîtés. Aucune découpe SVG, filtre ni image de référence.
Graine fixe : le fichier est reproductible, octet pour octet.

Usage : python3 tools/races/zwartbles/gen_zwartbles.py pets/zwartbles/zwartbles.svg
© 2026 Alejandro San Martin Lamas — MIT OR Apache-2.0
"""
import math
from pathlib import Path
import random
import sys

from shapely.geometry import LineString, Point, Polygon
import xml.etree.ElementTree as ET

sys.path.insert(0, str(Path(__file__).resolve().parents[1]))
import variantes  # noqa: E402

# Les autres dessins de la race (tools/races/variantes.py), d'après la fiche
# (§ 1.4, 1.5). Sans variante : la brebis.
VARIANTE = sys.argv[2] if len(sys.argv) > 2 else ""
PROPORTIONS = {
    "belier": dict(  # Bélier Zwartbles : 90 cm au garrot, sans cornes, avant-main plus musclé, tête plus large, cou plus fort.
        garrot=(80, 90), encolure=(1.0, 1.15), tete=(1.05, 1.08)),
    "agneau": dict(  # Agneau Zwartbles de deux semaines : 48 cm au garrot, noir de jais frisé, liste, balzanes et bout de queue blancs, tête étroite.
        variantes.AGNEAU, garrot=(80, 48), tete=(1.0, 1.1)),
}
from outils import (Doc, add, sub, mul, lerp, norm, clamp, mix, shade,
                    cr, dense, poly, geom_d, poisson, qstroke, lstroke)

D = Doc()
R = random.Random(1983)  # fondation du livre néerlandais (§ 1 de la fiche)
W, H = 154, 116
GARROT, CARPE, JARRET = 80.0, 20.0, 25.0
POLL = (111.5, 96.0)
ANGLE = math.radians(39.0)
HU = (math.cos(ANGLE), -math.sin(ANGLE))
HV = (-math.sin(ANGLE), -math.cos(ANGLE))


def HL(u, v):
    """Le repère de la tête : u nuque → nez, v chanfrein → mâchoire."""
    return add(POLL, add(mul(HU, u), mul(HV, v)))


def HP(pts):
    return [HL(*p) for p in pts]


def region(pts):
    return Polygon(dense(pts, True, 5)).buffer(0)


def ellipse(c, rx, ry, angle=0.0, n=24):
    ca, sa = math.cos(angle), math.sin(angle)
    return [(c[0] + rx * math.cos(t) * ca - ry * math.sin(t) * sa,
             c[1] + rx * math.cos(t) * sa + ry * math.sin(t) * ca)
            for t in [2 * math.pi * k / n for k in range(n)]]


def glow(gid, c, rx, ry, col, alpha, geom, angle=0.0):
    """Un modelé doux : géométrie calculée, jamais de masque à l'affichage."""
    outline = region(ellipse(c, rx, ry, angle))
    fill = D.rad(gid, c, ry, [(0, col, alpha), (0.5, col, alpha * 0.55), (1, col, 0)],
                 sx=rx / ry, angle=angle)
    D.path(geom_d(outline.intersection(geom)), fill=fill)


FLEECE_STOPS = [(0, '#514136'), (0.18, '#45362E'), (0.45, '#33261F'),
               (0.75, '#2B211C'), (1, '#1E1816')]


def wool_at(h):
    t = clamp((88 - h) / 57)
    for (a, ca), (b, cb) in zip(FLEECE_STOPS, FLEECE_STOPS[1:]):
        if t <= b:
            return mix(ca, cb, (t - a) / (b - a))
    return FLEECE_STOPS[-1][1]


def hair(geom, rng, flow, white=False, spacing=0.42, length=(0.3, 0.65), far=False):
    """Poils ras couchés ; leur couleur dépend de la robe, pas de la lumière seule."""
    if geom.is_empty:
        return
    D.begin_batch()
    for p in poisson(geom.buffer(-0.18), spacing, rng):
        v = flow(p)
        ln = rng.uniform(*length)
        tip = add(p, mul(v, ln))
        if not geom.contains(Point(tip)):
            continue
        mid = add(lerp(p, tip, 0.5), (rng.uniform(-0.08, 0.08), rng.uniform(-0.08, 0.08)))
        if white:
            col = rng.choice(('#FBF9F0', '#E2DED3', '#B9B3A6'))
            alpha = 0.45 if not far else 0.28
        else:
            col = rng.choice(('#4E4D4B', '#343739', '#101113'))
            alpha = 0.36 if not far else 0.2
        D.stroke(qstroke(p, mid, tip, 2), col, 0.055 if spacing < 0.5 else 0.075, alpha)
    D.flush()


def fleece(geom, seed, scale=1.0, light=1.0):
    """Toison de 4–6 cm, frisée serrée : des paquets irréguliers, non des
    pois ou de longues cordelettes. Chaque mèche se relève en petites ondes ;
    le creux sombre et le reflet chaud suivent la même onde. Le semis est
    décalé et incliné par paquets pour ne pas lire une grille sur le flanc.
    Les morceaux sont réellement bornés par leur région avant écriture.
    """
    rng = random.Random(seed)
    x0, h0, x1, h1 = geom.bounds
    expanded = geom.buffer(0.8)
    roots = []
    dh, dx = 1.65 * scale, 1.45 * scale
    row, h = 0, h0 + 0.4
    while h < h1 + 4:
        x = x0 - 2 + (row % 2) * dx / 2
        while x < x1 + 2:
            px = x + rng.uniform(-0.55, 0.55) * dx
            ph = h + rng.uniform(-0.4, 0.4) * dh
            if expanded.contains(Point(px, ph)):
                roots.append((ph, px))
            x += dx
        h += dh
        row += 1
    roots.sort()
    for h, x in roots:
        L = rng.uniform(3.4, 5.0) * scale
        w = rng.uniform(1.45, 2.0) * scale
        a = (0.23 * math.sin(x / 5 + h / 6) + 0.17 * math.sin(h / 3.9 - x / 6)
             + rng.uniform(-0.28, 0.28))
        axis = (math.sin(a), -math.cos(a))
        side = (math.cos(a), math.sin(a))
        phase = rng.uniform(0, 6.28)
        pitch = rng.uniform(0.83, 1.15) * scale
        left, right = [], []
        steps = 13
        for k in range(steps):
            t = k / (steps - 1)
            wave = math.sin(2 * math.pi * L * t / pitch + phase)
            c = add((x, h), add(mul(axis, t * L), mul(side, wave * w * 0.12)))
            taper = (0.35 + 0.65 * math.sin(math.pi * (0.12 + t * 0.76)))
            hw = w * 0.5 * taper * (1 + 0.11 * wave)
            left.append(add(c, mul(side, -hw)))
            right.append(add(c, mul(side, hw)))
        shape = Polygon(left + right[::-1]).buffer(0).intersection(geom)
        if shape.is_empty:
            continue
        var = (0.018 * math.sin(x / 3.2) * math.sin(h / 3.9)
               + 0.025 * math.sin(x / 6.1 - h / 5.3) + rng.uniform(-0.015, 0.015))
        base = shade(wool_at(h - L * 0.4), light * (1 + var))
        # Les paquets n'ont pas de long contour dessiné : il se lirait en
        # épi ou en écaille. Leur relief passe par les frisures elles-mêmes,
        # et le dégradé commun porte le volume du corps et de l'encolure.
        # Trois ou quatre frisures par mèche, d'amplitude variée : elles
        # s'interrompent près du bord plutôt que traverser les mèches voisines.
        dks, lts = [], []
        for t in (0.2, 0.41, 0.63, 0.83):
            c = add((x, h), mul(axis, t * L))
            # Chaque paquet se tord indépendamment : les frisures ont des
            # angles et des ouvertures différents, jamais des « sourires »
            # alignés comme des écailles sur le flanc.
            rr = rng.uniform(0.34, 0.7) * w
            theta = a + rng.uniform(-2.5, 2.5)
            ss = (math.cos(theta), math.sin(theta))
            aa = (-math.sin(theta), math.cos(theta))
            p = add(c, mul(ss, -rr / 2))
            q = add(c, add(mul(aa, rng.uniform(0.35, 0.75) * scale), mul(ss, rng.uniform(-0.15, 0.15))))
            e = add(c, mul(ss, rr / 2))
            if geom.contains(Point(p)) and geom.contains(Point(e)):
                dks.append(qstroke(p, q, e, 2))
                lts.append(qstroke(add(p, (0, 0.09 * scale)), add(q, (0, 0.12 * scale)), add(e, (0, 0.09 * scale)), 2))
        if dks:
            D.path(''.join(dks), stroke=shade(base, 0.58), sw=0.16 * scale, so=0.6, cap='round')
            D.path(''.join(lts), stroke=shade(base, 1.27), sw=0.105 * scale, so=0.48, cap='round')


# La poire : culotte et ventre profonds derrière, poitrail plus mince. La
# croupe (83 cm peau) dépasse le garrot (80), tous deux avec 4–5 cm de laine.
BODY = [(1, 72), (3, 79), (8, 84.4), (16, 87.3), (25, 87.6), (36, 87), (49, 86.4),
        (62, 85.9), (74, 85.3), (83, 85), (87, 84.5), (91, 81), (94, 75),
        (97, 67), (99, 58), (100, 48), (99, 42), (96, 38), (91, 36), (84, 34.9),
        (75, 33), (65, 31.8), (54, 30.6), (43, 30.0), (34, 31), (28, 33.6),
        (23, 37), (17, 38.8), (11, 40), (6, 43), (2, 49), (0, 56), (-0.5, 64)]

# Le garrot et le bord caché de l'encolure suivent le même principe que la
# Lacaune : deux courbes tangentes s'emboîtent. Quand le cou descend, son
# bord caché prolonge la pente arrondie de l'épaule, au lieu d'ouvrir une
# marche puis une bosse au-dessus du cou.
NECK_PIVOT = (89, 55)
WITHERS = (83, 85)
# Angle du cou relatif au corps en broutant, mesuré dans le moteur :
# 1,616 rad avec mastication, 1,623 sans. Le corps tourne déjà de 0,05 rad.
GRAZE_TURN = 1.62
SHOULDER_SLOPE = -35.0


def arc(start, h0, h1, rho, left):
    sign = 1 if left else -1
    nrm = math.radians(h0 + sign * 90)
    c = add(start, (rho * math.cos(nrm), rho * math.sin(nrm)))
    n = max(3, round(abs(h1 - h0) / 4))
    return [add(c, (rho * math.cos(math.radians(h0 + (h1 - h0) * i / n - sign * 90)),
                    rho * math.sin(math.radians(h0 + (h1 - h0) * i / n - sign * 90))))
            for i in range(n + 1)]


def ray(start, heading, length):
    a = math.radians(heading)
    return add(start, (length * math.cos(a), length * math.sin(a)))


def neck_rotate(p, a):
    x, h = sub(p, NECK_PIVOT)
    return add(NECK_PIVOT, (x * math.cos(a) + h * math.sin(a),
                           -x * math.sin(a) + h * math.cos(a)))


_shoulder = arc(WITHERS, -2, SHOULDER_SLOPE, 14, False)
SHOULDER_TANGENT = _shoulder[-1]
_shoulder.append(ray(_shoulder[-1], SHOULDER_SLOPE, 9))
_shoulder.extend(arc(_shoulder[-1], SHOULDER_SLOPE, -90, 4, False)[1:])
BODY = BODY[:10] + _shoulder[1:] + [(102, 60), (102, 51), (100, 44)] + BODY[17:]


def neck_hidden():
    v = sub(WITHERS, NECK_PIVOT)
    h0 = math.degrees(math.atan2(v[0], -v[1])) % 360
    h1 = SHOULDER_SLOPE + 180 + math.degrees(GRAZE_TURN)
    a = math.radians(SHOULDER_SLOPE)
    normal = (-math.sin(a), math.cos(a))
    target = sum(p * q for p, q in zip(SHOULDER_TANGENT, normal)) - 0.05
    low, high = 0.5, 50.0
    for _ in range(60):
        radius = (low + high) / 2
        q = neck_rotate(arc(WITHERS, h0, h1, radius, True)[-1], GRAZE_TURN)
        if sum(p * z for p, z in zip(q, normal)) > target:
            high = radius
        else:
            low = radius
    pts = arc(WITHERS, h0, h1, radius, True)
    q = neck_rotate(pts[-1], GRAZE_TURN)
    length = ((q[0] - SHOULDER_TANGENT[0]) * math.cos(a)
              + (q[1] - SHOULDER_TANGENT[1]) * math.sin(a) - 0.7)
    pts.append(ray(pts[-1], h1, max(0.1, length)))
    pts.extend(arc(pts[-1], h1, h1 + 80, 3.0, True)[1:])
    return pts
def wool_outline(pts, seed, amount=0.32):
    """Le relief des petites pointes de laine sur un contour non tondu."""
    rr = random.Random(seed)
    fine = dense(pts, True, 12)
    out = []
    for k, p in enumerate(fine):
        t = norm(sub(fine[(k + 1) % len(fine)], fine[k - 1]))
        n = (-t[1], t[0])
        wave = amount * (0.52 * math.sin(k * 2.3) + rr.uniform(-0.48, 0.48))
        out.append(add(p, mul(n, wave)))
    return out


BODY = wool_outline(BODY, 890)
BP = Polygon(BODY).buffer(0)
# Le col monte sans toupet derrière les oreilles. Une base cachée entière,
# arrondie autour du pivot, devient visible quand le cou se baisse.
NECK = ([(108, 99), (102, 98), (94, 93), (86, 87), WITHERS]
        + neck_hidden()[1:]
        + [(72, 58), (75, 49), (80, 42), (87, 39), (95, 40), (100, 44),
           (104, 51), (108, 60), (111, 68), (114, 75), (117, 81),
           (117, 88), (117, 94), (113, 98)])
NECK = wool_outline(NECK, 891, 0.24)
# Même sens extérieur que BODY et les régions découpées par les outils.
NECK.reverse()
NP = Polygon(NECK).buffer(0)


def paint_region_strokes(geom, paint, step=0.1, width=0.8):
    """Un aplat opaque par traits jointifs, sans aucun remplissage SVG.

    Le masque de lumière du moteur est réuni au repos puis suit seulement
    BODY. Un fill de NECK y laisserait une silhouette claire fantôme lorsque
    le cou se baisse ; le rendu cuit peut aussi éclairer les pixels vides du
    cou avec le masque de BODY. Toutes les parties de laine sont donc faites
    uniquement de traits : aucun masque global n'est produit pour cette
    brebis, dont les dégradés et frisures portent eux-mêmes le volume.
    Chaque balayage tient dans un seul chemin, sans stries ni transparence.
    """
    # À petite taille, le rastériseur SVG antialiase chaque trait. Un fort
    # recouvrement garde le fond opaque. Les lignes partent d'une région
    # érodée ; leurs bouts ronds retrouvent le contour sans le grossir.
    step = min(step, width / 8)
    core = geom.buffer(-width / 2 + step * 0.3)
    x0, h0, x1, h1 = core.bounds
    spans = []
    h = h0 + step / 2
    while h < h1:
        cut = core.intersection(LineString([(x0 - 1, h), (x1 + 1, h)]))
        pieces = [cut] if cut.geom_type == 'LineString' else getattr(cut, 'geoms', [])
        for line in pieces:
            if line.geom_type == 'LineString' and line.length > 0.001:
                spans.append(poly(list(line.coords), False))
        h += step
    D.path(''.join(spans), stroke=paint, sw=width, cap='round', join='round')
    # La courbe de bord reste fine, lisse et pleine entre deux balayages.
    D.path(geom_d(geom, simplify=0), stroke=paint, sw=step * 0.6,
           cap='round', join='round')


def neck():
    D.open('neck')
    contour = Polygon(dense(NECK, True, 4, t=0.65)).buffer(0)
    paint_region_strokes(contour, 'url(#laine)')
    # Sans modelés radiaux dans cette pièce fortement tournée : les petites
    # frisures et le dégradé commun suffisent à porter son volume, y compris
    # sur les bords cachés qui deviennent visibles aux angles extrêmes.
    fleece(NP, 8023, scale=0.94)
    # Une courte zone à poil ras derrière la ganache se fond sous la tête.
    bare = region(HP([(-2, -1), (2, -2), (7, 0), (8, 6), (5, 11), (1, 12), (-3, 8)]))
    bare = bare.intersection(NP)
    paint_region_strokes(bare, 'url(#tete)')
    hair(bare, random.Random(102), lambda p: norm((-0.3, -1)))
    D.pivot('neck', NECK_PIVOT)
    D.close()


def body():
    # Trois petits festons à racine cachée. La toison moyenne ne réclame
    # qu'une oscillation discrète ; aucun long cordon sous le ventre.
    for i, (x, hh) in enumerate(((40, 30.6), (56, 31), (72, 33.2)), 1):
        D.open(f'sway-{i}')
        pts = [(x - 4, hh + 6), (x + 4, hh + 6), (x + 3.6, hh),
               (x + 1.5, hh - 1), (x - 1, hh - 1.3), (x - 3.7, hh + 0.2)]
        pp = region(pts)
        paint_region_strokes(pp, 'url(#laine)')
        fleece(pp, 2080 + i, scale=0.8)
        D.pivot(f'sway-{i}', (x, hh + 3))
        D.close()
    D.open('body')
    # La courte portion proximale de la queue suit la croupe. Les trois
    # tronçons mobiles, derrière le corps, reprennent sous cette portion :
    # le moteur ne transmet pas la rotation BODY à TAIL dans une ruade.
    # Sa fin se noie dans la culotte, pour ne pas laisser de seconde branche.
    tail_piece([(2.5, 78), (0.8, 74), (0.6, 71), (2.1, 68.5)],
               [5.4, 5.2, 4.7, 4.1], 'q-base', stroke_fill=True)
    # La mamelle d'une brebis d'automne, beaucoup moins développée qu'en
    # pleine lactation : une petite saillie sombre entre ventre et cuisse.
    udder = [(22, 36), (26, 37), (32, 35), (32, 31), (30, 28.7), (27, 28),
             (24.5, 29), (23, 31)]
    if not VARIANTE:
        paint_region_strokes(region(udder), D.lin('mamelle', (25, 36), (28, 28),
                                                [(0, '#25201E'), (0.5, '#3A3533'), (1, '#282624')]))
    teat = [(29.2, 29.2), (30, 28.7), (29.8, 27.2), (29.2, 27.1)]
    if not VARIANTE:
        paint_region_strokes(region(teat), '#36302E', width=0.3)
    contour = Polygon(dense(BODY, True, 4, t=0.65)).buffer(0)
    paint_region_strokes(contour, 'url(#laine)')
    fleece(BP, 1843)
    # Quelques fibres courtes détachent le dos et la culotte ; pas de jarres
    # blancs sur cette brebis jeune et pas de longues mèches rousses.
    outline = BODY
    D.begin_batch()
    rng = random.Random(92)
    for k, p in enumerate(outline):
        if k % 3 or p[1] < 43:
            continue
        a, b = outline[k - 1], outline[(k + 1) % len(outline)]
        v = norm(sub(b, a))
        n = (-v[1], v[0])
        if BP.contains(Point(add(p, mul(n, 0.3)))):
            n = mul(n, -1)
        ln = rng.uniform(0.2, 0.58)
        D.stroke(qstroke(add(p, mul(n, -0.2)), add(add(p, mul(n, 0.3)), mul(v, 0.16)),
                          add(add(p, mul(n, ln * 0.6)), mul(v, 0.28)), 2),
                 shade(wool_at(p[1]), 1.06), 0.11, 0.6)
    D.flush()
    D.pivot('body', (50, 31))
    D.close()


# Pattes articulées : chaque bas commence par un capot rond centré sur le
# genou ; l'avant-bras finit par le même disque, plus large, au-dessus.
LEGS = [('leg-back-far', False, 8, True), ('leg-front-far', True, -7, True),
        ('leg-back-near', False, 0, False), ('leg-front-near', True, 0, False)]


def leg(name, front, dx, far):
    x = (87 if front else 8.0) + dx
    jh = CARPE if front else JARRET
    pivot = (86 + dx, 53) if front else (18 + dx, 58)
    joint = (x, jh)
    # Le canon est presque droit ; le boulet se porte un peu en avant.
    lower = [(x - 2.4, jh), (x - 2, jh + 1.8), (x, jh + 2.45), (x + 2, jh + 1.8),
             (x + 2.4, jh), (x + 2, 14), (x + 1.8, 9), (x + 2.05, 6.8),
             (x + 2.8, 4.3), (x + 3.7, 2.5), (x + 4.5, 0.25), (x + 4.3, 0),
             (x - 1.7, 0), (x - 2.1, 0.6), (x - 1.8, 2.5), (x - 2.6, 4.1),
             (x - 2.3, 6.8), (x - 2.1, 10), (x - 2.2, 16)]
    lp = region(lower)
    k = 0.77 if far else 1
    D.open(name)
    D.open(name + '-lower')
    D.path(cr(lower), fill=D.lin(name + '-noir-bas', (x - 3, 0), (x + 3, 0),
                                [(0, shade('#101214', k)), (0.3, shade('#353638', k)),
                                 (0.6, shade('#1D1F21', k)), (1, shade('#0E0F10', k))]))
    sh = 8.0 if front else 12.0
    sock = region([(x - 5, -1), (x + 6, -1), (x + 6, sh - 0.9), (x + 1.4, sh + 0.9),
                   (x + 0.2, sh + 0.1), (x - 1.3, sh + 0.3), (x - 5, sh - 0.8)]).intersection(lp)
    D.path(geom_d(sock), fill=D.lin(name + '-balzane', (x - 2.5, 0), (x + 2.5, 0),
                                  [(0, shade('#BDB8AC', k)), (0.25, shade('#F4F2EA', k)),
                                   (0.65, shade('#E8E6DC', k)), (1, shade('#BDB8AC', k))]))
    rng = random.Random(30 + round(dx) + (50 if front else 0))
    hair(lp.difference(sock).buffer(-0.12), rng, lambda p: (0, -1), spacing=0.55, far=far)
    hair(sock, rng, lambda p: norm((0.05, -1)), white=True, spacing=0.48, far=far)
    # Le tendon et l'os du boulet, deux reflets fins plutôt qu'une patte tube.
    D.path(cr([(x - 0.8, jh - 3), (x - 0.7, 14), (x - 0.65, 9.6)], False),
           stroke=shade('#4D4B47', k), sw=0.27, so=0.3, cap='round')
    hoof = [(x - 1.9, 2.1), (x + 0.1, 2.75), (x + 3.7, 2.5), (x + 4.5, 0.25),
            (x + 4.3, 0), (x - 1.7, 0), (x - 2.1, 0.6)]
    D.path(cr(hoof), fill=D.lin(name + '-onglon', (x - 2, 1), (x + 4.5, 1),
                              [(0, shade('#161514', k)), (0.5, shade('#38332D', k)), (1, shade('#24221F', k))]))
    D.path(cr([(x - 1.6, 0.12), (x + 1.5, 0.12), (x + 4.1, 0.12)], False), stroke='#665E4F', sw=0.12, so=0.7)
    D.path(cr([(x + 2.0, 2.5), (x + 2.2, 1.3), (x + 2.7, 0.1)], False), stroke='#111111', sw=0.18, so=0.9)
    for z in range(3):
        D.path(cr([(x + 2.4 + z * 0.45, 2.0), (x + 2.8 + z * 0.45, 0.6)], False),
               stroke='#534B40', sw=0.06, so=0.45)
    D.path(cr([(x - 2.3, 4.7), (x - 3.0, 4.35), (x - 2.8, 3.8), (x - 2.2, 4.0)]), fill='#26221F')
    D.pivot(name + '-lower', joint)
    D.close()
    if front:
        upper = [(x - 6.2, 55), (x + 5.3, 55), (x + 4.2, 42), (x + 3.4, 32),
                 (x + 2.75, 23), (x + 2.6, jh), (x + 1.9, jh - 1.9),
                 (x, jh - 2.7), (x - 1.9, jh - 1.9), (x - 2.6, jh),
                 (x - 2.9, 29), (x - 4.3, 35), (x - 6.8, 42)]
    else:
        upper = [(x - 3.2, 61), (x + 13.8, 61), (x + 14.2, 50), (x + 12.0, 43),
                 (x + 8.3, 36), (x + 4.3, 29), (x + 2.7, jh),
                 (x + 1.9, jh - 2.0), (x, jh - 2.7), (x - 2, jh - 1.9),
                 (x - 2.7, jh), (x - 3.2, 28), (x - 1.8, 36), (x - 1.0, 46)]
    up = region(upper)
    D.path(cr(upper), fill=D.lin(name + '-noir-haut', (x - 3, 0), (x + 3, 0),
                               [(0, shade('#101214', k)), (0.3, shade('#353638', k)),
                                (0.6, shade('#1D1F21', k)), (1, shade('#0E0F10', k))]))
    hair(up, rng, lambda p: norm((0 if front else -0.45, -1)), spacing=0.54, far=far)
    D.path(cr([(x - 0.9, jh + 3), (x - 0.8, jh + 0.9), (x - 0.3, jh - 0.7)], False),
           stroke=shade('#4D4B47', k), sw=0.25, so=0.4, cap='round')
    D.pivot(name, pivot)
    D.close()


def tail():
    """50 cm de queue naturelle droite ; ses 13 derniers cm sont blancs.
    Les trois tronçons se chevauchent par des caps ronds, jamais de jour."""
    D.open('tail')
    # Le pivot s'enfouit dans la croupe pour rester couvert au demi-coucher
    # et dans les deux sens de ruade. La courte portion haute, dans BODY,
    # préserve l'attache naturelle à 78 cm et les 50 cm de longueur utile.
    tail_piece([(12, 70), (7, 73), (2, 72), (-1, 66), (-3.8, 59)],
               [6.5, 5.8, 5.0, 4.5, 4.1], 'q1')
    D.open('tail-mid')
    tail_piece([(-3.4, 62), (-4.0, 54), (-4.1, 47), (-4, 42)], [4.3, 3.9, 3.4, 3.0], 'q2')
    D.open('tail-tip')
    tail_piece([(-4.05, 45), (-4.0, 40), (-3.8, 34), (-3.3, 29.2)], [3.3, 2.8, 2.1, 1.2], 'q3')
    D.pivot('tail-tip', (-4.05, 44))
    D.close()
    D.pivot('tail-mid', (-3.4, 61))
    D.close()
    D.pivot('tail', (12, 70))
    D.close()


def tail_piece(pts, widths, gid, stroke_fill=False):
    centers = dense(pts, False, 6)
    left, right = [], []
    for i, p in enumerate(centers):
        t = i / (len(centers) - 1) * (len(widths) - 1)
        j = min(int(t), len(widths) - 2)
        w = widths[j] + (widths[j + 1] - widths[j]) * (t - j)
        v = norm(sub(centers[min(i + 1, len(centers) - 1)], centers[max(0, i - 1)]))
        n = (-v[1], v[0])
        left.append(add(p, mul(n, w / 2)))
        right.append(add(p, mul(n, -w / 2)))
    shape = left + [add(centers[-1], (0, -widths[-1] * 0.4))] + right[::-1] + [add(centers[0], (0, widths[0] / 2))]
    tp = region(shape)
    if stroke_fill:
        paint_region_strokes(tp, 'url(#laine)')
    else:
        D.path(cr(shape), fill='url(#laine)')
    fleece(tp, sum(map(ord, gid)) + 600, scale=0.66, light=0.94)
    white = tp.intersection(Polygon([(-20, -1), (10, -1), (10, 42), (-20, 42)]))
    if not white.is_empty:
        paint = D.lin(gid + '-blanc', (-7, 0), (-3, 0),
                      [(0, '#BDB8AC'), (0.35, '#F4F2EA'), (1, '#D7D4C8')])
        if stroke_fill:
            paint_region_strokes(white, paint)
        else:
            D.path(geom_d(white), fill=paint)
        hair(white, random.Random(600), lambda p: norm((0.05, -1)), white=True, spacing=0.3, length=(0.5, 1.0))


# La tête est longue et étroite : de la nuque au nez, 30 cm. Légère
# convexité du chanfrein ; joue longue et fine, aucun stop ni front laineux.
DORSAL = [(-1.4, 0), (-1.0, -1.5), (0.6, -2.4), (3, -2.8), (6, -2.7), (10, -2.25),
          (14, -1.7), (18, -1.1), (22, -0.5), (25.5, 0.25), (28, 1.2),
          (29.2, 2.0), (29.7, 3.0), (29.8, 4.25), (29.3, 5.1)]
MOUTH = [(29.3, 5.1), (27.5, 5.3), (25.5, 5.5), (23.4, 5.8), (21.5, 6.1)]
NOTCH = [(21.5, 6.1), (20.3, 7.4), (19, 8.9)]
UNDER = [(19, 8.9), (16.5, 8.7), (13, 9.6), (9, 11.0), (5, 10.8), (2.0, 8.8),
         (0, 5.5), (-1.2, 2.7), (-1.4, 0)]
HEAD = DORSAL + MOUTH[1:] + NOTCH[1:] + UNDER[1:-1]
HDP = region(HP(HEAD))
# Une bande blanche au-dessus de l'œil, vue par la tranche, et non un masque.
BLAZE = DORSAL + [(28.9, 5.35), (27, 5.55), (25.2, 5.8), (23.5, 5.65),
                 (24.2, 3.4), (24.5, 2.15), (21.0, 0.7), (16, -0.15),
                 (11, -0.7), (7, -1.05), (3.2, -1.0), (0.7, -0.9), (-0.8, -0.6)]
BLAZE_P = region(HP(BLAZE)).intersection(HDP)
EYE = HL(7.7, 0.9)


def jaw():
    D.open('jaw')
    # La partie intérieure remonte derrière le contour fermé ; elle ne se
    # révèle que quand le moteur ouvre la bouche. Pas de dents du haut.
    pts = [(18.5, 5.2), (22, 4.6), (26.5, 4.7), (29.4, 5.1), (29.4, 6.15),
           (28, 7.25), (25, 8.35), (21, 8.9), (18.8, 8.5)]
    jp = region(HP(pts))
    D.path(cr(HP(pts)), fill='url(#tete)')
    D.path(cr(HP([(19, 4.8), (23.2, 4.8), (27.5, 4.9), (29, 5.0), (27, 5.15), (23, 5.6), (19, 6.0)])),
           fill='#26191B')
    # La zone blanche du menton enveloppe le seul bout du museau.
    chin = region(HP([(25.3, 5.0), (29.4, 5.1), (29.4, 6.15), (28, 7.25), (25.0, 8.35), (24.5, 6.6)]))
    D.path(geom_d(chin.intersection(jp)), fill='url(#blanc)')
    D.path(cr(HP([(20.3, 4.8), (23.4, 4.7), (26.2, 4.6), (27.6, 4.6)])), fill='#9F6C72', fo=0.65)
    D.pivot('jaw', HL(20.5, 6.4))
    D.close()


def eye():
    # La paupière fermée, sous le globe animé, sert au sommeil et au clignement.
    D.path(cr([add(EYE, (-1.6, 0.0)), add(EYE, (0, -0.2)), add(EYE, (1.6, 0.0))], False),
           stroke='#4B4740', sw=0.21, so=0.8, cap='round')
    D.open('eyes')
    almond = [(EYE[0] - 1.7, EYE[1] - 0.06), (EYE[0] - 1.1, EYE[1] + 0.62),
              (EYE[0] + 0.1, EYE[1] + 0.83), (EYE[0] + 1.25, EYE[1] + 0.5),
              (EYE[0] + 1.7, EYE[1] - 0.1), (EYE[0] + 0.8, EYE[1] - 0.65),
              (EYE[0] - 0.55, EYE[1] - 0.7)]
    D.path(cr(almond), fill='#362D23')
    D.open('pupils')
    ic = add(EYE, (-0.2, 0.02))
    D.path(cr(ellipse(ic, 1.23, 1.05)), fill=D.rad('iris', add(ic, (0, -0.4)), 1.35,
                                               [(0, '#ECCA7C'), (0.55, '#AA8747'), (0.9, '#6B512A'), (1, '#403122')]))
    for i in range(36):
        a = i * math.pi / 18
        p = add(ic, (0.48 * math.cos(a), 0.4 * math.sin(a)))
        e = add(ic, (1.12 * math.cos(a + 0.04), 0.94 * math.sin(a + 0.04)))
        D.path(lstroke(p, e, 2), stroke='#E2BF70' if i % 2 else '#5E4427', sw=0.045, so=0.5)
    D.path(cr([(ic[0] - 0.94, ic[1] + 0.12), (ic[0] - 0.7, ic[1] + 0.29),
               (ic[0] + 0.68, ic[1] + 0.29), (ic[0] + 0.94, ic[1] + 0.08),
               (ic[0] + 0.7, ic[1] - 0.2), (ic[0] - 0.65, ic[1] - 0.2)]), fill='#1C1714')
    D.pivot('pupils', ic)
    D.close()
    D.path(cr(ellipse(add(EYE, (-0.65, 0.4)), 0.17, 0.12)), fill='#F0EEE8', fo=0.92)
    D.path(cr(ellipse(add(EYE, (0.65, -0.25)), 0.08, 0.06)), fill='#E6DED0', fo=0.45)
    D.pivot('eyes', EYE)
    D.close()
    # Le pourtour cache la partie du regard qui dépasse de l'amande, sans
    # masque SVG (même principe que les deux dernières races).
    ring = region(ellipse(EYE, 3.6, 2.9)).intersection(HDP).difference(region(almond))
    D.path(geom_d(ring), fill='url(#tete)', rule='evenodd')
    # La paupière supérieure lourde recouvre une part de l'iris et de sa
    # bordure : l'œil n'est pas un anneau posé sur la joue.
    D.path(cr([add(EYE, (-1.7, 0.0)), add(EYE, (-1.1, 0.72)), add(EYE, (0.1, 0.84)),
               add(EYE, (1.25, 0.5)), add(EYE, (1.7, -0.1)), add(EYE, (0.9, 0.34)),
               add(EYE, (0.0, 0.43)), add(EYE, (-1, 0.27))]), fill='#17191B')
    D.path(cr(almond, False), stroke='#0A0B0C', sw=0.19, so=0.85, cap='round')
    D.path(cr([add(EYE, (-1.5, 0.1)), add(EYE, (-0.7, 0.48)), add(EYE, (0.4, 0.45)), add(EYE, (1.4, 0.08))], False),
           stroke='#514F48', sw=0.1, so=0.55, cap='round')
    D.path(cr([add(EYE, (-1.4, 0.8)), add(EYE, (-0.4, 1.2)), add(EYE, (0.8, 1.05))], False),
           stroke='#4A4A48', sw=0.17, so=0.5, cap='round')


def ear(far=False):
    # Pavillon de 12,8 cm, horizontal (3° relevé), sombre dedans comme dehors.
    # L'oreille du fond dépasse un peu au-dessus de celle du flanc visible.
    base = HL(-0.2 if far else 2, -1.3 if far else 0.1)
    ang = math.radians(37 if far else 3)
    uu = (-math.cos(ang), math.sin(ang))
    vv = (-uu[1], uu[0])
    sz = 0.73 if far else 1.0
    f = lambda u, v: add(base, add(mul(uu, u * sz), mul(vv, v * sz)))
    k = 0.73 if far else 1
    out = [(0, -0.8), (2.3, -1.8), (5.6, -2.7), (9, -2.3), (11.8, -0.8),
           (12.8, 0.2), (12.0, 1.0), (9.0, 2.0), (5.4, 2.0), (2.4, 1.1), (0, 0.8)]
    ep = region([f(*p) for p in out])
    name = 'ear-far' if far else 'ear-near'
    D.open(name)
    D.path(cr([f(*p) for p in out]), fill=D.lin(name + '-peau', f(5, -2.5), f(5, 2.0),
                                            [(0, shade('#3A3B3B', k)), (0.45, shade('#1D1F21', k)), (1, shade('#101113', k))]))
    cup = [(1.6, 0.1), (4.3, -0.3), (7.1, -0.5), (9.8, -0.2), (11.8, 0.35),
           (9.5, 1.2), (6.3, 1.5), (3.3, 0.85)]
    D.path(cr([f(*p) for p in cup]), fill=D.lin(name + '-creux', f(5, -0.6), f(5, 1.5),
                                            [(0, shade('#111114', k)), (0.55, shade('#2A2931', k)), (1, shade('#39383D', k))]))
    D.path(cr([f(1.6, 0.3), f(4.5, 0.2), f(8, 0.35), f(11.6, 0.45)], False),
           stroke=shade('#50504F', k), sw=0.14, so=0.7, cap='round')
    hair(ep, random.Random(109 if far else 110), lambda p: uu, spacing=0.36, far=far)
    D.path(cr([f(*p) for p in out[1:6]], False), stroke=shade('#55534E', k), sw=0.16, so=0.6, cap='round')
    D.pivot(name, base)
    D.close()


def lip():
    # Le bourrelet supérieur reste invisible sous la lèvre au repos.
    pad = [(25, 4.5), (26.4, 3.95), (28.1, 3.9), (29.1, 4.15), (28.8, 4.65), (27.1, 4.9), (25.4, 5.1)]
    D.path(cr(HP(pad)), fill='#B17B80')
    D.path(cr(HP([(25.6, 4.8), (27, 4.5), (28.5, 4.4)]), False), stroke='#6A3C46', sw=0.18)
    D.open('lip')
    pts = [(24.7, 5.7), (24.8, 4.1), (26.5, 3.5), (28.1, 3.55), (29.7, 3.8),
           (29.8, 4.3), (29.3, 5.1), (27.5, 5.3), (25.5, 5.5)]
    D.path(cr(HP(pts)), fill='url(#blanc)')
    hair(region(HP(pts)), random.Random(882), lambda p: HU, white=True, spacing=0.28, length=(0.2, 0.4))
    D.path(cr(HP(MOUTH[:4]), False), stroke='#373336', sw=0.18, so=0.9, cap='round')
    D.pivot('lip', HL(25, 4.5))
    D.close()


def head():
    D.open('head')
    ear(True)
    jaw()
    D.path(cr(HP(HEAD)), fill='url(#tete)')
    D.path(geom_d(BLAZE_P), fill='url(#blanc)')
    # Modelé de l'arcade, du masséter et de l'arête de la joue : la tête
    # reste noire et froide devant la laine chaude.
    dark = HDP.difference(BLAZE_P)
    glow('joue', HL(8, 6.8), 5.4, 2.5, '#515150', 0.34, dark, angle=-ANGLE)
    glow('arête-face', HL(15, 3.1), 7.8, 0.65, '#66645E', 0.28, dark, angle=-ANGLE)
    glow('sous-joue', HL(10, 9.2), 7.0, 1.3, '#07080A', 0.45, dark, angle=-ANGLE)
    hair(dark.difference(region(ellipse(EYE, 2.8, 2.2))), random.Random(606),
         lambda p: norm(add(mul(HU, -0.93), mul(HV, 0.2))), spacing=0.31, length=(0.24, 0.5))
    hair(BLAZE_P, random.Random(607), lambda p: HU, white=True, spacing=0.3, length=(0.26, 0.52))
    eye()
    # Naseau en virgule, noir sur le museau blanc, grain humide discret.
    nose = [(26.8, 1.5), (28, 1.8), (29.2, 2.4), (29.7, 3.1), (29.7, 4),
            (28.7, 4.25), (27.3, 3.8), (26.2, 2.8)]
    D.path(cr(HP(nose)), fill=D.lin('truffe', HL(27, 1.6), HL(28.8, 4),
                                  [(0, '#52545A'), (0.45, '#26282D'), (1, '#191A1D')]))
    nostril = [(26.5, 2.65), (27.25, 2.15), (28.2, 2.35), (29, 2.95), (28.9, 3.45),
               (28.35, 3.6), (27.65, 3.1), (27.1, 2.8)]
    D.path(cr(HP(nostril)), fill='#0D0D0F')
    D.path(cr(HP([(27.1, 2.16), (27.9, 2.24), (28.6, 2.65)]), False), stroke='#767A80', sw=0.12, so=0.75, cap='round')
    # Le petit creux du larmier est un pli sombre, jamais un second œil.
    D.path(cr(HP([(10.1, 1.8), (11.4, 2.25), (12.6, 2.9)]), False), stroke='#0B0C0E', sw=0.13, so=0.5, cap='round')
    for k in range(13):
        p = HL(23.8 + R.random() * 3, 3.3 + R.random() * 1.5)
        D.path(cr(ellipse(p, 0.045, 0.045, n=6)), fill='#7D7C75', fo=0.55)
    D.path(cr(HP(MOUTH), False), stroke='#18171A', sw=0.22, so=0.9, cap='round')
    lip()
    ear()
    D.pivot('head', POLL)
    D.pivot('muzzle', HL(29.4, 5.6))
    D.close()


def main(path):
    D.lin('laine', (0, 88), (0, 31), FLEECE_STOPS)
    D.lin('tete', HL(9, -2.7), HL(9, 10), [(0, '#393B3C'), (0.3, '#272A2C'),
                                          (0.55, '#1D1F21'), (1, '#0E0F10')])
    D.lin('blanc', HL(12, -3), HL(12, 9), [(0, '#F4F2EA'), (0.5, '#E8E6DC'), (1, '#BDB8AC')])
    for args in LEGS:
        leg(*args)
    tail()
    neck()
    body()
    head()
    D.pivot('ground', (49, 0))
    comment = ('Zwartbles : brebis adulte de 3–4 ans, début d’automne, toison de 4–6 cm.\n'
               '80 cm au garrot ; une unité = un cm. Profil droit, sans cornes ; liste\n'
               'continue, quatre balzanes, queue naturelle de 50 cm au dernier quart blanc.\n'
               'D’après docs/races/zwartbles.md ; généré par tools/races/zwartbles/gen_zwartbles.py.\n'
               '© 2026 Alejandro San Martin Lamas — MIT OR Apache-2.0')
    svg = D.render(W, H, comment)
    if VARIANTE:
        return variante(path, svg)
    Path(path).parent.mkdir(parents=True, exist_ok=True)
    Path(path).write_text(svg, encoding='utf-8')
    print(f'{path} : {len(svg.encode()) / 1024:.0f} Ko, {svg.count("<path")} tracés')


def variante(path, svg):
    """Le bélier ou l'agneau : ce qui change de la brebis, puis ses proportions."""
    root = ET.fromstring(svg)
    if VARIANTE == "belier":
        # Le scrotum, couvert d'un poil fin, sans laine (§ 1.4).
        variantes.bourses(root, *variantes.sous_le_ventre(root, 7, -3), largeur=9.5, hauteur=15,
                          peau="#3A3533", ombre="#161413")
    if VARIANTE == "agneau":
        # Noir de jais à la naissance : la toison ne brunit au soleil que plus tard.
        variantes.recolorer(root, variantes.rampe("#070708", "#2C2A2B", bas=.03, haut=.45),
                            dans=("body", "neck", "sway-1", "sway-2", "sway-3"),
                            si=lambda c: variantes.clarte(c) < .5)
    variantes.ecrire(path, root, **PROPORTIONS[VARIANTE])


if __name__ == '__main__':
    main(sys.argv[1] if len(sys.argv) > 1 else 'zwartbles.svg')
