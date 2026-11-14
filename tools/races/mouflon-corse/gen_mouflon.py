#!/usr/bin/env python3
"""Génère pets/mouflon-corse/mouflon-corse.svg : un bélier de mouflon de Corse (type du
Cinto), adulte (environ huit ans), en pelage d'hiver, vu strictement de profil, tourné
vers la droite.

Unités : 1 unité SVG = 1 cm de l'animal réel. Sol en y = 112. Garrot à 75 cm.
"""
import math
import os
import sys

sys.path.insert(0, os.path.join(os.path.dirname(os.path.abspath(__file__)), ".."))
import articulations  # noqa: E402  (tools/races/articulations.py)
import variantes  # noqa: E402
import xml.etree.ElementTree as ET  # noqa: E402

from geom import (RNG, Svg, P, add, sub, mul, lerp, length, norm, perp, rot, f2, f1,
                  smooth_d, spline_pieces, sample_spline, sample_chains, poly_d,
                  point_in_poly, bbox, resample, cum_length, mix, shade, hair_strokes,
                  hex2rgb, rgb2hex)

OUT = sys.argv[1] if len(sys.argv) > 1 else "mouflon-corse.svg"
# Le deuxième argument : la finesse du poil (un nombre), ou un autre dessin
# de l'espèce (tools/races/variantes.py), d'après la fiche (§ 2, 4.3, 6.6) :
# `brebis`, la mouflonne du Cinto, sans cornes, sans selle ni jabot ;
# `agneau`. Sans lui : le bélier.
VARIANTE = sys.argv[2] if len(sys.argv) > 2 and sys.argv[2] in variantes.SECTIONS else ""
DETAIL = float(sys.argv[2]) if len(sys.argv) > 2 and not VARIANTE else 1.0
PROPORTIONS = {
    "brebis": dict(  # Mouflonne du Cinto : 65 cm au garrot, sans cornes, sans selle ni jabot, pelage d'hiver brun beige.
        garrot=(75, 65), encolure=(1.0, .85), tete=(.95, .92)),
    "agneau": dict(  # Agneau de mouflon de deux semaines : 39 cm au garrot, fauve clair, sans selle ni cornes.
        variantes.AGNEAU, garrot=(75, 39), corps=(.86, .92), pattes=(1.08, 1.07)),
}

# ---------------------------------------------------------------- palette (fiche § 4.2)
BLACK, BLACK_SH, BLACK_LT = "#211a16", "#120e0c", "#45372e"
CHOC, CHOC_SH, CHOC_LT, TOPLINE = "#4a3226", "#2a1c15", "#6e4b37", "#33241c"
CHES, CHES_SH, CHES_LT = "#8f5a3a", "#5a3725", "#b97c52"
SADDLE, SADDLE_SH, SADDLE_LT, GRIZ = "#e9e3d8", "#bdb3a6", "#f6f2ea", "#a39484"
if VARIANTE:
    # Ni la mouflonne ni l'agneau n'ont de vraie selle : elle prend le ton du flanc.
    SADDLE, SADDLE_SH, SADDLE_LT, GRIZ = CHES, CHES_SH, CHES_LT, CHES
WHITE, WHITE_SH, WHITE_LT = "#ede8df", "#c2b8ab", "#f7f4ee"
STOCK, STOCK_SH, STOCK_LT, STOCK_DIRT = "#ece7de", "#bfb5a8", "#f6f3ed", "#bdb7ae"
MUZ, MUZ_SH, MUZ_LT = "#f2eee7", "#cfc6ba", "#fbf9f5"
FACE, FACE_SH, FACE_LT, FACE_GRIZ = "#5e4e42", "#3b3029", "#8a796b", "#8a8278"
RING, RING_SH, RING_LT = "#d9d2c6", "#a89f93", "#eee9e1"
EAR, EAR_SH, EAR_LT = "#6f6052", "#4a3e35", "#958473"
EAR_RIM, EAR_HAIR, EAR_SKIN = "#e3ddd3", "#ece6dc", "#c9aca2"
TAIL, TAIL_SH, TAIL_LT = "#241a15", "#140f0c", "#43352c"
HOOF, HOOF_SH, HOOF_LT = "#2b2725", "#171514", "#4d4743"
HORN0, HORN1, HORN2 = "#5e4c3c", "#85705a", "#bfae95"
HORN_GROOVE, HORN_RIDGE = "#3e3329", "#a88f73"
IRIS, IRIS_EDGE, IRIS_GLINT = "#b8863b", "#8c6227", "#d9b26a"
PUPIL, GRANULA, LIDRIM = "#0e0b0a", "#1a1411", "#1e1916"
NOSE, NOSTRIL, NOSE_LT = "#2b2523", "#161211", "#4a403c"
MOUTH = "#3b312c"
SCROT, SCROT_SH = "#e5ddd3", "#b8a89c"
OUTLINE = "#1f1813"

GROUND = 112.0
svg = Svg()


def nhair(n):
    return max(0, int(round(n * DETAIL)))


# ====================================================================== REPÈRE DE LA TÊTE
E = P(128.5, 33.0)            # centre de l'œil (à peu près à hauteur du garrot)
HA = math.radians(38.0)       # inclinaison de l'axe de la tête (nuque -> bout du nez) sous l'horizontale
SH = 1.08                     # tête dessinée dans un repère de 25 cm, agrandie à HL = 27 cm
U = (math.cos(HA), math.sin(HA))
V = (-math.sin(HA), math.cos(HA))


def H(a, b):
    return (E[0] + SH * (a * U[0] + b * V[0]), E[1] + SH * (a * U[1] + b * V[1]))


def HL(pts):
    return [H(a, b) for (a, b) in pts]


C = P(116.0, 32.0)            # centre de la spirale de la corne : à hauteur de l'œil, 0,46 HL en arrière

PIV_NECK = P(96.0, 56.0)      # base de l'encolure, dans l'épaule : elle porte la tête
PIV_HEAD = P(114.0, 31.0)     # la nuque, sous la base des cornes : la tête y tourne
MUZZLE = P(138.5, 47.0)       # ce qui touche l'herbe : le bout du museau, sous les naseaux
PIV_JAW = H(6.0, 5.7)
PIV_EAR = H(-5.4, 1.2)
PIV_EAR_FAR = add(PIV_EAR, (1.2, -1.4))
PIV_TAIL = P(35.6, 43.2)
LEG_Y = 54.0
PIV_FN, PIV_FF = P(104.5, LEG_Y), P(99.0, LEG_Y)
PIV_BN, PIV_BF = P(49.0, LEG_Y), P(54.5, LEG_Y)
GROUND_X = 76.5

# ====================================================================== PATTES


def foreleg(dx):
    o = lambda x, y: (x + dx, y)
    front = [o(109.0, 50.0), o(110.2, 58.0), o(109.6, 66.0), o(108.0, 72.5), o(106.6, 78.5),
             o(105.5, 83.6), o(104.9, 86.8), o(105.35, 88.6), o(105.2, 90.6), o(104.45, 92.6),
             o(104.0, 95.5), o(103.9, 99.8), o(104.35, 102.2), o(104.95, 104.8), o(105.55, 107.2)]
    hoof = [o(105.55, 107.2), o(106.6, 109.5), o(107.55, 111.7), o(107.2, 112.0),
            o(102.2, 112.0), o(101.6, 111.3)]
    heel_top = [o(101.15, 108.4)]
    back = [o(101.6, 111.3), o(101.2, 108.8), o(100.85, 106.0), o(100.2, 103.4), o(100.5, 100.8),
            o(100.95, 98.0), o(100.95, 94.0), o(100.5, 91.6), o(100.1, 90.2), o(100.6, 88.2),
            o(100.0, 84.0), o(99.1, 79.0), o(97.8, 75.2), o(97.0, 70.0), o(97.3, 60.0), o(98.6, 50.0)]
    return front, back, hoof, heel_top


def hindleg(dx):
    o = lambda x, y: (x + dx, y)
    front = [o(53.0, 49.0), o(55.0, 57.0), o(56.0, 64.5), o(55.9, 70.0), o(54.0, 74.6),
             o(50.6, 78.2), o(46.6, 80.9), o(43.8, 82.8), o(42.4, 85.0), o(41.9, 88.0),
             o(42.0, 92.0), o(42.15, 96.0), o(42.3, 100.0), o(42.9, 102.6), o(43.6, 105.4), o(44.35, 107.4)]
    hoof = [o(44.35, 107.4), o(45.4, 109.6), o(46.35, 111.7), o(46.0, 112.0),
            o(41.0, 112.0), o(40.4, 111.3)]
    heel_top = [o(39.9, 108.5)]
    back = [o(40.4, 111.3), o(39.95, 108.8), o(39.5, 106.1), o(38.6, 103.4), o(38.5, 100.4),
            o(38.3, 94.0), o(37.9, 89.0), o(37.2, 86.0), o(35.6, 83.6), o(34.9, 81.8),
            o(35.3, 79.5), o(35.4, 76.5), o(34.6, 73.0), o(33.4, 69.0), o(32.4, 63.0), o(33.0, 56.0), o(34.5, 49.0)]
    return front, back, hoof, heel_top


def hoof_and_feet(hoof, heel_top, back, dk):
    hp = heel_top + hoof
    hd = spline_pieces([[hp[0], hp[1]], hp[1:4], hp[3:6], hp[5:7] + [hp[0]]])
    hx0, hy0, hx1, hy1 = bbox(hp)
    gh = svg.lin(hx0, hy0, hx1, hy1, [(0, dk(HOOF_SH)), (0.55, dk(HOOF)), (1, dk(HOOF_LT))])
    svg.path(hd, fill=gh)
    toe = hoof[2]
    cor = hoof[0]
    # la fente entre les deux onglons, et un reflet sur la paroi
    svg.path(f"M{f2(toe[0]-1.55)} 112L{f2(toe[0]-1.4)} 110.6Q{f2(cor[0]-0.75)} 108.8 {f2(cor[0]-1.15)} 107.85",
             stroke=dk(HOOF_SH), sw=0.3, sop=0.9, cap="round")
    svg.path(f"M{f2(cor[0]-0.05)} 107.75Q{f2(cor[0]+0.7)} 109.1 {f2(toe[0]-0.4)} 111.45",
             stroke=dk(HOOF_LT), sw=0.26, sop=0.55, cap="round")
    # couronne : le poil du paturon retombe sur le sabot
    hb = heel_top[0]
    svg.path(f"M{f2(hb[0]+0.1)} {f2(hb[1]+0.15)}Q{f2((hb[0]+cor[0])/2)} {f2(min(hb[1], cor[1])-0.5)} {f2(cor[0]+0.15)} {f2(cor[1]+0.35)}",
             stroke=dk(STOCK_DIRT), sw=0.55, sop=0.7, cap="round")
    # ergots : deux petites bosses cornées et sombres, à l'arrière du boulet
    dc = add(back[3], (0.15, 0.55))
    svg.path(smooth_d([add(dc, (-0.42, -0.3)), add(dc, (0.25, -0.38)), add(dc, (0.32, 0.28)), add(dc, (-0.28, 0.42))], True),
             fill=dk(HOOF), stroke=dk(HOOF_SH), sw=0.1)
    dc2 = add(dc, (0.55, 0.45))
    svg.path(smooth_d([add(dc2, (-0.28, -0.2)), add(dc2, (0.2, -0.28)), add(dc2, (0.26, 0.2)), add(dc2, (-0.2, 0.32))], True),
             fill=dk(HOOF_SH))


def leg_group(name, kind, dx, pivot, far):
    svg.open(name)
    k = 0.74 if far else 1.0          # les pattes du fond, dans l'ombre
    dk = lambda c: shade(c, k)
    front, back, hoof, heel_top = (foreleg if kind == "front" else hindleg)(dx)
    outline = front + hoof[1:-1] + back
    d = smooth_d(outline, closed=True)
    poly = sample_spline(outline, closed=True, n=8)
    x0, y0, x1, y1 = bbox(poly)
    fx = dx
    if kind == "front":
        g = svg.lin(x0, 0, x1, 0, [(0, dk(STOCK_SH)), (0.3, dk(STOCK)), (0.7, dk(STOCK_LT)), (1, dk(STOCK))])
        svg.path(d, fill=g)
        # devant de l'avant-bras sombre jusqu'au genou, une tache étroite sur le genou et un
        # trait qui descend en partie le long du canon ; l'arrière de l'avant-bras blanc
        dark = [P(96.5 + fx, 50), P(110.5 + fx, 50), P(110.4 + fx, 58), P(109.8 + fx, 66), P(108.3 + fx, 72.5),
                P(106.9 + fx, 78.5), P(105.8 + fx, 83.6), P(105.25 + fx, 86.8), P(105.7 + fx, 88.6),
                P(105.55 + fx, 90.6), P(104.8 + fx, 92.6), P(104.35 + fx, 95.5), P(104.2 + fx, 97.6),
                P(103.55 + fx, 97.0), P(103.45 + fx, 95.0), P(103.6 + fx, 92.8), P(103.6 + fx, 91.0),
                P(103.3 + fx, 89.0), P(102.6 + fx, 87.4), P(102.3 + fx, 84.0), P(101.6 + fx, 80.0),
                P(100.6 + fx, 76.0), P(99.6 + fx, 73.0), P(97.4 + fx, 70.0), P(96.6 + fx, 60)]
        gd = svg.lin(x0, 0, x1, 0, [(0, dk(BLACK_SH)), (0.6, dk(BLACK)), (1, dk(BLACK_LT))])
        svg.path(smooth_d(dark, True), fill=gd)
        # un liseré brun et doux, là où le noir rejoint l'arrière blanc de l'avant-bras
        edge = [P(102.3 + fx, 84.0), P(101.6 + fx, 80.0), P(100.6 + fx, 76.0), P(99.6 + fx, 73.0)]
        svg.path(smooth_d(edge, False), stroke=dk(CHOC), sw=0.7, sop=0.55, cap="round")
        if not far:
            # poil sombre plus long sur le devant de l'avant-bras (bélier en hiver) : un bord ébouriffé
            fr = [P(108.8, 71.0), P(107.3, 76.0), P(106.1, 81.0), P(105.3, 85.4)]
            for i in range(nhair(18)):
                t = RNG.random()
                j = min(2, int(t * 3))
                a = lerp(fr[j], fr[j + 1], t * 3 - j)
                L = RNG.uniform(1.1, 2.0)
                e = add(a, (RNG.uniform(0.15, 0.7), L))
                svg.path(f"M{f2(a[0]-0.35)} {f2(a[1])}Q{f2(a[0]+0.45)} {f2(a[1]+L*0.5)} {f2(e[0])} {f2(e[1])}",
                         stroke=RNG.choice([BLACK, BLACK_SH, "#2e241e"]), sw=RNG.uniform(0.22, 0.36),
                         sop=RNG.uniform(0.6, 0.95), cap="round")
            # poil blanc à l'arrière de l'avant-bras et sur la balzane
            for i in range(nhair(26)):
                y = RNG.uniform(76.0, 106.0)
                xa = 100.3 + 0.12 * (y - 76) if y < 88 else 101.4 + RNG.uniform(0.2, 1.8)
                a = (xa + RNG.uniform(0.2, 1.4), y)
                e = add(a, (RNG.uniform(-0.15, 0.15), RNG.uniform(0.9, 1.6)))
                svg.path(f"M{f2(a[0])} {f2(a[1])}L{f2(e[0])} {f2(e[1])}", stroke=RNG.choice([STOCK_SH, STOCK_LT]),
                         sw=0.15, sop=0.5, cap="round")
    else:
        g = svg.lin(x0, 0, x1, 0, [(0, dk(STOCK)), (0.4, dk(STOCK_LT)), (0.75, dk(STOCK)), (1, dk(STOCK_SH))])
        svg.path(d, fill=g)
        # jambe châtaine dans le prolongement de la cuisse, bande blanche derrière, au-dessus du jarret
        brown = [P(34.0 + fx, 48), P(54.0 + fx, 48), P(56.4 + fx, 57), P(57.0 + fx, 64.5), P(56.8 + fx, 70.0),
                 P(54.6 + fx, 75.0), P(51.0 + fx, 78.6), P(46.8 + fx, 81.4), P(44.0 + fx, 83.4),
                 P(42.8 + fx, 85.6), P(42.5 + fx, 88.4), P(41.0 + fx, 86.6), P(39.8 + fx, 84.6),
                 P(38.2 + fx, 82.0), P(37.4 + fx, 78.6), P(37.0 + fx, 75.0), P(36.0 + fx, 71.0),
                 P(34.6 + fx, 65.0), P(33.6 + fx, 56.0)]
        gb = svg.lin(0, 66, 0, 88, [(0, dk(shade(CHES, 0.8))), (0.42, dk(shade(CHES, 0.72))),
                                    (0.7, dk(CHES_SH)), (1, dk(CHOC_SH))])
        svg.path(smooth_d(brown, True), fill=gb)
        # devant de la jambe et du jarret, brun foncé
        darkf = [P(56.6 + fx, 66.0), P(56.2 + fx, 71.0), P(54.0 + fx, 75.6), P(50.4 + fx, 79.2),
                 P(46.4 + fx, 81.9), P(43.6 + fx, 84.0), P(42.6 + fx, 88.4), P(41.9 + fx, 91.0),
                 P(41.4 + fx, 88.0), P(42.2 + fx, 84.6), P(45.2 + fx, 81.4), P(49.0 + fx, 78.4),
                 P(52.4 + fx, 75.0), P(54.4 + fx, 70.6)]
        svg.path(smooth_d(darkf, True), fill=dk(CHOC_SH), fop=0.85)
        if not far:
            for i in range(nhair(30)):
                y = RNG.uniform(72.0, 84.0)
                a = (RNG.uniform(38.0, 50.0 - (y - 72) * 0.5) + fx, y)
                e = add(a, (RNG.uniform(-0.6, -0.2), RNG.uniform(1.0, 1.8)))
                svg.path(f"M{f2(a[0])} {f2(a[1])}L{f2(e[0])} {f2(e[1])}", stroke=RNG.choice([CHES, CHES_SH, CHOC, CHES_LT]),
                         sw=0.2, sop=0.5, cap="round")
    # un peu de saleté grise au bas de la balzane (dans l'ombre, près du sabot)
    gdirt = svg.lin(0, 99.0, 0, 108.0, [(0, dk(STOCK_DIRT), 0.0), (1, dk(STOCK_DIRT), 0.75)])
    svg.path(d, fill=gdirt)
    hoof_and_feet(hoof, heel_top, back, dk)
    svg.path(d, stroke=dk(OUTLINE), sw=0.28, sop=0.5, join="round")
    svg.circle_pivot(name, pivot)
    svg.close()


# ====================================================================== CORPS
WITHERS = P(99.0, 35.6)
BODY = [
    WITHERS, P(103.4, 37.6), P(107.4, 41.4), P(110.4, 46.4), P(112.3, 52.2), P(112.6, 58.0),
    P(111.3, 63.2), P(108.6, 67.6), P(104.6, 71.0), P(99.4, 72.9), P(93.4, 73.8), P(87.0, 73.9),
    P(80.6, 73.2), P(74.2, 71.8), P(68.6, 69.8), P(64.0, 67.7), P(60.2, 66.6), P(57.4, 67.7),
    P(55.2, 70.6), P(53.4, 73.6), P(50.0, 75.3), P(45.6, 75.8), P(40.4, 75.2), P(35.6, 73.3), P(31.6, 69.8),
    P(28.5, 64.6), P(26.5, 59.6), P(26.0, 54.4), P(26.9, 49.4), P(29.0, 45.4), P(32.8, 41.9),
    P(38.4, 39.3), P(45.4, 38.0), P(52.6, 38.2), P(60.0, 38.9), P(68.0, 39.4), P(76.0, 39.6),
    P(84.0, 39.1), P(90.6, 38.0), P(95.6, 36.6),
]
FORE = [P(98.5, 35.9), P(103.4, 37.6), P(107.4, 41.4), P(110.4, 46.4), P(112.3, 52.2), P(112.6, 58.0),
        P(111.3, 63.2), P(108.6, 67.6), P(104.6, 71.0), P(99.4, 72.9), P(93.4, 73.8), P(91.4, 72.8),
        P(90.6, 67.0), P(92.4, 60.0), P(94.4, 51.6), P(95.6, 43.0)]
BAND = [P(96.5, 58.4), P(90.0, 59.9), P(80.0, 61.0), P(71.0, 61.6), P(65.0, 62.4), P(61.2, 63.4),
        P(60.8, 65.4), P(65.0, 65.6), P(71.0, 66.2), P(80.0, 67.2), P(90.0, 68.0), P(96.0, 69.0)]
BELLY = [P(96.0, 69.0), P(90.0, 68.0), P(80.0, 67.2), P(71.0, 66.2), P(65.0, 65.6), P(60.8, 65.4),
         P(60.2, 66.6), P(64.0, 67.7), P(68.6, 69.8), P(74.2, 71.8), P(80.6, 73.2), P(87.0, 73.9),
         P(93.4, 73.8)]
RUMP = [P(31.0, 43.2), P(32.4, 47.0), P(32.6, 52.2), P(31.6, 58.2), P(31.6, 64.2), P(33.0, 69.8),
        P(36.0, 73.4), P(35.6, 73.3), P(31.6, 69.8), P(28.5, 64.6), P(26.5, 59.6), P(26.0, 54.4),
        P(26.9, 49.4), P(29.0, 45.4)]
SADDLE_PTS = [P(71.4, 41.2), P(73.2, 41.9), P(75.4, 41.6), P(77.0, 43.8), P(77.4, 46.8), P(76.6, 49.4),
              P(76.9, 51.8), P(75.4, 54.4), P(73.6, 56.6), P(71.8, 57.2), P(70.2, 55.6), P(68.6, 53.4),
              P(67.6, 50.6), P(66.6, 47.8), P(67.2, 44.6), P(69.0, 42.0)]

HQ = 5.0   # on avance l'arrière-train : un tronc ramassé, comme sur les photographies


def WX(p):
    x, y = p
    return (x + HQ * min(1.0, max(0.0, (68.0 - x) / 24.0)), y)


BODY = [WX(p) for p in BODY]
BAND = [WX(p) for p in BAND]
BELLY = [WX(p) for p in BELLY]
RUMP = [WX(p) for p in RUMP]
SADDLE_PTS = [(x + 2.0, y) for (x, y) in SADDLE_PTS]
SADDLE_C = P(74.0, 49.4)

BODY_POLY = sample_spline(BODY, True, n=10)
FORE_POLY = sample_spline(FORE, True, n=8)
BAND_POLY = sample_spline(BAND, True, n=8)
BELLY_POLY = sample_spline(BELLY, True, n=8)
RUMP_POLY = sample_spline(RUMP, True, n=8)
SADDLE_POLY = sample_spline(SADDLE_PTS, True, n=8)


def base_coat(x):
    """Dégradé horizontal du pelage : noir devant, chocolat au milieu, châtain derrière."""
    stops = [(113, BLACK), (95, BLACK), (88, CHOC_SH), (78, CHOC), (65.5, mix(CHOC, CHES, 0.5)),
             (55.5, CHES), (31, CHES)]
    for i in range(len(stops) - 1):
        (xa, ca), (xb, cb) = stops[i], stops[i + 1]
        if xb <= x <= xa:
            return mix(ca, cb, (xa - x) / (xa - xb))
    return stops[0][1] if x > 113 else stops[-1][1]


def coat_at(p):
    if point_in_poly(p, SADDLE_POLY):
        return SADDLE
    if point_in_poly(p, RUMP_POLY):
        return WHITE
    if point_in_poly(p, BELLY_POLY):
        return WHITE
    if point_in_poly(p, BAND_POLY) or point_in_poly(p, FORE_POLY):
        return BLACK
    c = base_coat(p[0])
    if p[1] < 41.5:
        c = mix(c, TOPLINE, 0.5)
    return c


def flow_body(p):
    x, y = p
    # tronc : vers l'arrière, un peu vers le bas ; épaule : vers le bas et l'arrière ;
    # cuisse et croupe : vers le bas
    if x > 88:
        d = (-0.45, 0.9)
    elif x < 53:
        t = min(1, (53 - x) / 14)
        d = (-0.85 + 0.6 * t, 0.5 + 0.45 * t)
    else:
        d = (-1.0, 0.22 + 0.3 * max(0, (y - 52) / 22))
    return norm(d)


def body_group():
    svg.open("body")
    d = smooth_d(BODY, True)
    # pelage de fond
    g = svg.lin(113, 0, 31, 0, [(0, BLACK), (0.22, BLACK), (0.31, CHOC_SH), (0.43, CHOC),
                                (0.58, mix(CHOC, CHES, 0.5)), (0.7, CHES), (1, CHES)])
    svg.path(d, fill=g)
    # modelé : ligne du dessus et ligne du ventre plus sombres, dos éclairé
    gv = svg.lin(0, 37, 0, 77, [(0, TOPLINE, 0.9), (0.12, TOPLINE, 0.25), (0.25, CHOC_LT, 0.0),
                                (0.75, "#000000", 0.0), (1, "#000000", 0.3)])
    svg.path(d, fill=gv)
    # cuisse : une masse musculaire arrondie, d'un châtain plus clair
    svg.path(smooth_d([WX(p) for p in [P(33.0, 42.0), P(46.0, 39.0), P(58.0, 43.0), P(60.0, 56.0),
                                       P(54.0, 68.0), P(41.0, 71.0), P(33.4, 63.0), P(32.4, 50.0)]], True),
             fill=svg.rad(50.3, 51.0, 11.0, [(0, CHES_LT, 0.42), (0.6, CHES_LT, 0.15), (1, CHES_LT, 0.0)]))
    # ombre sous la cuisse, là où sort la jambe
    svg.path(smooth_d([WX(p) for p in [P(30.5, 66.0), P(42.0, 70.0), P(56.0, 69.5), P(57.6, 71.0),
                                       P(52.4, 76.2), P(40.4, 76.4), P(31.0, 70.6)]], True),
             fill=svg.lin(0, 66.5, 0, 76.5, [(0, CHES_SH, 0.0), (1, CHES_SH, 0.55)]))
    # l'avant du corps, noir (poitrail, bréchet, devant de l'épaule, base du cou)
    gf = svg.lin(89, 0, 113, 0, [(0, BLACK, 0.0), (0.3, BLACK, 0.95), (1, BLACK)])
    svg.path(smooth_d(FORE, True), fill=gf)
    # omoplate : un léger reflet chocolat sur l'épaule sombre, éclairée d'en haut
    svg.path(smooth_d([P(98.6, 36.4), P(97.0, 44.0), P(95.2, 53.0), P(92.0, 60.0), P(86.0, 59.0),
                       P(84.0, 50.0), P(86.0, 41.0), P(91.0, 37.6)], True),
             fill=svg.rad(91.5, 45.5, 11.0, [(0, CHOC_LT, 0.5), (0.6, CHOC, 0.25), (1, CHOC, 0.0)]))
    # la pointe de l'épaule et le poitrail prennent un peu la lumière
    svg.path(smooth_d([P(103.0, 48.0), P(109.0, 50.0), P(111.6, 56.0), P(110.0, 62.0), P(104.0, 62.0),
                       P(101.0, 55.0)], True),
             fill=svg.rad(106.5, 55.0, 6.0, [(0, BLACK_LT, 0.6), (1, BLACK_LT, 0.0)]))
    # bande sombre du flanc, aux bords nets, du coude au pli du flanc
    svg.path(smooth_d(BAND, True), fill=BLACK)
    # ventre blanc sous la bande
    gb = svg.lin(0, 67.5, 0, 76, [(0, WHITE), (0.6, WHITE), (1, WHITE_SH)])
    svg.path(smooth_d(BELLY, True), fill=gb)
    # miroir blanc à l'arrière des cuisses, précédé d'un mince trait sombre
    svg.path(smooth_d(RUMP, True), stroke=CHES_SH, sw=0.9, sop=0.75)
    gr = svg.lin(31, 0, 40, 0, [(0, WHITE_SH), (0.35, WHITE), (1, WHITE_LT)])
    svg.path(smooth_d(RUMP, True), fill=gr)
    # selle : bord grisonnant, centre blanc
    svg.path(smooth_d(SADDLE_PTS, True), fill=GRIZ, fop=0.45)
    sad2 = [lerp(p, SADDLE_C, 0.13) for p in SADDLE_PTS]
    gs = svg.rad(SADDLE_C[0] - 0.6, 47.5, 8.5, [(0, SADDLE_LT), (0.7, SADDLE), (1, SADDLE_SH)])
    svg.path(smooth_d(sad2, True), fill=gs)
    # scrotum : pâle, pendant entre les pattes arrière (à peine plus bas que le point le plus
    # bas de la cuisse, pour que le ventre touche encore le sol quand l'animal se couche)
    scr = [P(59.5, 68.4), P(61.9, 68.3), P(62.4, 71.0), P(62.8, 74.0), P(61.7, 76.3), P(60.0, 76.9),
           P(58.6, 75.5), P(58.4, 72.6), P(58.9, 70.2)]
    if not VARIANTE:
        svg.path(smooth_d(scr, True), fill=svg.lin(0, 68.3, 0, 76.9, [(0, CHES_SH), (0.3, SCROT_SH),
                                                                       (0.6, mix(SCROT, SCROT_SH, 0.35)),
                                                                       (1, mix(SCROT, SCROT_SH, 0.75))]))
    for i in range(nhair(14) if not VARIANTE else 0):
        a = (RNG.uniform(58.8, 62.4), RNG.uniform(70.5, 75.0))
        e = add(a, (RNG.uniform(-0.3, 0.3), RNG.uniform(0.8, 1.6)))
        if point_in_poly(e, sample_spline(scr, True, n=6)):
            svg.path(f"M{f2(a[0])} {f2(a[1])}L{f2(e[0])} {f2(e[1])}", stroke=RNG.choice([SCROT_SH, WHITE_LT]),
                     sw=0.18, sop=0.6, cap="round")
    if not VARIANTE:
        svg.path(smooth_d(scr[2:7], False), stroke=OUTLINE, sw=0.2, sop=0.25, cap="round")
    # texture du poil
    body_hair()
    # contour, sauf là où les pattes sortent de sous le corps
    runs, cur = [], []
    for p in BODY_POLY + BODY_POLY[:1]:
        skip = p[1] > 70.5 and (93.0 < p[0] < 110.0 or 38.0 < p[0] < 61.0)
        if skip:
            if len(cur) > 1:
                runs.append(cur)
            cur = []
        else:
            cur.append(p)
    if len(cur) > 1:
        runs.append(cur)
    for r in runs:
        svg.path(poly_d(r, closed=False), stroke=OUTLINE, sw=0.3, sop=0.45, join="round", cap="round")
    svg.circle_pivot("body", P(72.0, 75.8))
    svg.close()


def lighter(c, k):
    return shade(c, k)


def body_hair():
    inner = [lerp(p, P(70, 56), 0.035) for p in BODY_POLY]

    def col(p):
        c = coat_at(p)
        r = RNG.random()
        if c in (SADDLE, WHITE):
            return RNG.choice([WHITE_SH, SADDLE_LT, GRIZ]) if r < 0.6 else WHITE_LT
        if r < 0.5:
            return shade(c, 0.7)
        return shade(c, 1.2) if c != BLACK else BLACK_LT
    hair_strokes(svg, inner, nhair(900), flow_body, col, 1.3, 2.4, 0.2, 0.32, 0.25, 0.5,
                 curl=0.18, jitter=0.18)
    # des poils bruns et grisonnants qui estompent le bord de la selle
    ring = SADDLE_POLY
    for i in range(nhair(130)):
        a = ring[RNG.randrange(len(ring))]
        c = SADDLE_C
        out = norm(sub(a, c))
        t = RNG.uniform(-1.6, 1.4)
        p = add(a, mul(out, t))
        dd = flow_body(p)
        e = add(p, mul(dd, RNG.uniform(1.0, 2.2)))
        # des poils blancs qui s'égarent dans le brun, et des bruns dans le blanc
        col = RNG.choice([SADDLE, SADDLE_LT, GRIZ]) if t > 0 else RNG.choice([CHOC, GRIZ, CHOC_LT])
        svg.path(f"M{f1(p[0])} {f1(p[1])}L{f1(e[0])} {f1(e[1])}", stroke=col,
                 sw=RNG.uniform(0.2, 0.3), sop=RNG.uniform(0.55, 0.85), cap="round")
    # le long du dos, le contour cassé du pelage d'hiver : de petites touffes
    top = [p for p in BODY_POLY if p[1] < 41.0 and 38 < p[0] < 99]
    for i in range(nhair(70)):
        a = top[RNG.randrange(len(top))]
        e = add(a, (RNG.uniform(-1.5, -0.8), RNG.uniform(-0.05, 0.3)))
        svg.path(f"M{f2(a[0])} {f2(a[1]+0.45)}Q{f2((a[0]+e[0])/2)} {f2(min(a[1], e[1])-0.12)} {f2(e[0])} {f2(e[1])}",
                 stroke=RNG.choice([TOPLINE, CHOC_SH]), sw=0.22, sop=0.6, cap="round")
    # jabot du bas : de longs poils noirs sur le poitrail, qui descendent entre les pattes avant
    for i in range(0 if VARIANTE else nhair(60)):
        y = RNG.uniform(51.0, 70.0)
        x = 113.0 - 0.06 * (y - 58) ** 2 / 2 - RNG.uniform(0.6, 4.0)
        L = RNG.uniform(3.0, 6.0)
        e = (x - RNG.uniform(0.8, 2.2), min(y + L, 74.6))
        svg.path(f"M{f2(x)} {f2(y)}Q{f2(x+0.6)} {f2((y+e[1])/2)} {f2(e[0])} {f2(e[1])}",
                 stroke=RNG.choice([BLACK, BLACK_SH, BLACK_LT, "#2a211c"]), sw=RNG.uniform(0.3, 0.5),
                 sop=RNG.uniform(0.6, 0.95), cap="round")
    # poil blanc plus bouffant au bord du miroir
    for i in range(nhair(40)):
        a = RUMP_POLY[RNG.randrange(len(RUMP_POLY))]
        if a[0] < 34.5:
            continue
        e = add(a, (RNG.uniform(-1.2, 0.6), RNG.uniform(0.8, 1.8)))
        svg.path(f"M{f2(a[0])} {f2(a[1])}L{f2(e[0])} {f2(e[1])}", stroke=RNG.choice([WHITE, WHITE_LT, WHITE_SH]),
                 sw=0.3, sop=0.8, cap="round")


# ====================================================================== QUEUE
def tail_group():
    svg.open("tail")
    t = [(x + HQ, y) for (x, y) in [P(31.8, 41.9), P(29.8, 42.6), P(28.3, 44.4), P(27.5, 47.0), P(27.3, 49.6),
                                    P(27.9, 50.9), P(28.9, 50.3), P(29.6, 48.2), P(30.6, 46.0), P(32.0, 44.4),
                                    P(33.0, 43.0)]]
    gt = svg.lin(27.3 + HQ, 46, 32.6 + HQ, 44, [(0, TAIL_SH), (0.55, TAIL), (1, TAIL_LT)])
    svg.path(smooth_d(t, True), fill=gt)
    for i in range(nhair(10)):
        a = (RNG.uniform(28.6, 31.4) + HQ, RNG.uniform(43.0, 46.0))
        e = add(a, (RNG.uniform(-1.3, -0.6), RNG.uniform(1.8, 3.2)))
        svg.path(f"M{f2(a[0])} {f2(a[1])}L{f2(e[0])} {f2(e[1])}", stroke=RNG.choice([TAIL_SH, TAIL_LT, TAIL]),
                 sw=0.25, sop=0.8, cap="round")
    # poils pâles du dessous, au bord du miroir blanc
    for i in range(nhair(6)):
        a = (RNG.uniform(27.6, 28.6) + HQ, RNG.uniform(45.5, 49.5))
        e = add(a, (RNG.uniform(-0.5, 0.1), RNG.uniform(0.8, 1.4)))
        svg.path(f"M{f2(a[0])} {f2(a[1])}L{f2(e[0])} {f2(e[1])}", stroke=WHITE_SH, sw=0.2, sop=0.7, cap="round")
    svg.circle_pivot("tail", PIV_TAIL)
    svg.close()


# ====================================================================== CORNES
def horn_centerline():
    pts = [P(125.6, 28.6), P(125.4, 24.6), P(124.0, 21.4), P(121.0, 19.4), P(116.6, 18.9),
           P(111.6, 20.2), P(107.6, 23.6), P(105.4, 28.4), P(105.2, 33.6), P(106.8, 38.6),
           P(110.2, 42.4), P(114.8, 44.6), P(119.2, 45.3), P(122.6, 44.6), P(125.0, 43.0), P(126.2, 41.6)]
    poly = sample_spline(pts, closed=False, n=30)
    return resample(poly, 0.45)


def horn_width(s):
    table = [(0.0, 6.6), (0.08, 6.4), (0.2, 6.1), (0.45, 5.7), (0.62, 5.2), (0.76, 4.5),
             (0.86, 3.6), (0.94, 2.6), (1.0, 1.5)]
    for i in range(len(table) - 1):
        a, b = table[i], table[i + 1]
        if a[0] <= s <= b[0]:
            t = (s - a[0]) / (b[0] - a[0])
            return a[1] + (b[1] - a[1]) * t
    return table[-1][1]


def horn_geometry(offset=(0.0, 0.0), scale=1.0):
    cl = horn_centerline()
    cl = [add(C, add(mul(sub(p, C), scale), offset)) for p in cl]
    cen = add(C, offset)
    L = cum_length(cl)
    tot = L[-1]
    outer, inner, nrm = [], [], []
    for i, p in enumerate(cl):
        a = cl[max(i - 1, 0)]
        b = cl[min(i + 1, len(cl) - 1)]
        n = perp(norm(sub(b, a)))
        if (p[0] - cen[0]) * n[0] + (p[1] - cen[1]) * n[1] < 0:
            n = mul(n, -1)
        w = horn_width(L[i] / tot) * scale
        outer.append(add(p, mul(n, w / 2)))
        inner.append(add(p, mul(n, -w / 2)))
        nrm.append(n)
    return cl, L, tot, outer, inner, nrm


RINGS = [0.045, 0.10, 0.17, 0.25, 0.35, 0.48, 0.65, 0.91]   # anneaux de croissance annuels (base -> pointe)


def horn_color(s):
    if s < 0.55:
        return mix(HORN0, HORN1, s / 0.55)
    return mix(HORN1, HORN2, (s - 0.55) / 0.45)


def horn(far=False, s0=0.0, s1=1.0):
    """Dessine la corne entre s0 et s1 (0 = base, 1 = pointe). La corne proche se dessine en
    deux fois : sa racine avant la face (le poil du front cache l'endroit où elle sort du
    crâne), le reste après l'oreille."""
    off = (0.7, -1.4) if far else (0.0, 0.0)
    cl, L, tot, outer, inner, nrm = horn_geometry(off, 0.99 if far else 1.0)
    k = 0.6 if far else 1.0
    n = len(cl)
    idx = lambda s: min(n - 1, max(0, int(round(s * (n - 1)))))
    i0, i1 = idx(s0), idx(s1)
    lightdir = norm((-0.5, -0.87))
    tang = lambda i: norm(sub(cl[min(i + 1, n - 1)], cl[max(i - 1, 0)]))
    # le corps de la corne en tranches, chacune ombrée en travers : arête éclairée, milieu,
    # face intérieure dans l'ombre
    step = 2
    a = i0
    while a < i1:
        b = min(i1, a + step)
        aa, bb = max(i0, a - 1), min(i1, b + 1)       # un léger chevauchement : pas de raccord visible
        m = (a + b) // 2
        s = L[m] / tot
        base = horn_color(s)
        nm = nrm[m]
        lit = nm[0] * lightdir[0] + nm[1] * lightdir[1]
        c_keel = shade(base, (1.16 + 0.10 * lit) * k)
        c_mid = shade(base, (1.0 + 0.07 * lit) * k)
        c_dark = shade(base, (0.66 + 0.06 * lit) * k)
        po, pi = outer[m], inner[m]
        g = svg.lin(po[0], po[1], pi[0], pi[1], [(0, c_mid), (0.14, c_keel), (0.42, c_mid), (0.85, c_dark), (1, shade(c_dark, 0.9))])
        svg.path(poly_d(outer[aa:bb + 1] + list(reversed(inner[aa:bb + 1]))), fill=g)
        a = b
    # crêtes d'ornement : stries transversales serrées, plus marquées devant et sur la face extérieure
    s = L[i0] + 0.45
    while s < L[i1] - 0.4:
        i = min(range(i0, i1 + 1), key=lambda q: abs(L[q] - s))
        ss = L[i] / tot
        t = tang(i)
        o_, in_ = outer[i], inner[i]
        bow = mul(t, 0.25 + 0.35 * (1 - ss))
        worn = ss > 0.88
        op = (0.55 if not worn else 0.22) * (0.7 if far else 1.0)
        e = lerp(o_, in_, 0.9)
        m_ = add(lerp(o_, e, 0.5), bow)
        svg.path(f"M{f2(o_[0])} {f2(o_[1])}Q{f2(m_[0])} {f2(m_[1])} {f2(e[0])} {f2(e[1])}",
                 stroke=shade(HORN_GROOVE, 0.9 * k), sw=0.15, sop=op, cap="round")
        if not far and not worn:
            o2 = add(o_, mul(t, 0.3))
            e2 = add(lerp(o_, in_, 0.42), mul(t, 0.3))
            m2 = add(lerp(o2, e2, 0.5), bow)
            svg.path(f"M{f2(o2[0])} {f2(o2[1])}Q{f2(m2[0])} {f2(m2[1])} {f2(e2[0])} {f2(e2[1])}",
                     stroke=shade(HORN_RIDGE, 1.12), sw=0.12, sop=0.55, cap="round")
        s += 0.62 * (0.8 + 0.7 * ss)
    # anneaux de croissance annuels : sillons profonds, chacun suivi d'un léger renflement
    for r in RINGS:
        i = idx(r)
        if not (i0 <= i <= i1) or (r == s0 and s0 > 0 and False):
            continue
        t = tang(i)
        o_, in_ = outer[i], inner[i]
        m_ = add(lerp(o_, in_, 0.45), mul(t, 0.5))
        svg.path(f"M{f2(o_[0])} {f2(o_[1])}Q{f2(m_[0])} {f2(m_[1])} {f2(in_[0])} {f2(in_[1])}",
                 stroke=shade(HORN_GROOVE, 0.7 * k), sw=0.42 if r > 0.05 else 0.28,
                 sop=0.95 if r > 0.05 else 0.6, cap="round")
        if not far:
            o2, i2 = add(o_, mul(t, 0.38)), add(in_, mul(t, 0.38))
            m2 = add(lerp(o2, i2, 0.45), mul(t, 0.5))
            svg.path(f"M{f2(o2[0])} {f2(o2[1])}Q{f2(m2[0])} {f2(m2[1])} {f2(i2[0])} {f2(i2[1])}",
                     stroke=shade(HORN_RIDGE, 1.15), sw=0.2, sop=0.55, cap="round")
    # l'arête fronto-nucale, le long du bord extérieur, prend la lumière
    ka, kb = max(i0, idx(0.02)), min(i1, idx(0.9))
    if kb > ka + 1:
        keel = [lerp(outer[i], inner[i], 0.12) for i in range(ka, kb + 1)]
        svg.path(poly_d(keel, closed=False), stroke=shade(HORN_RIDGE, 1.28) if not far else shade(HORN_RIDGE, k),
                 sw=0.4, sop=0.55 if not far else 0.22, cap="round")
    # pointe usée, plus lisse, plus pâle
    if i1 == n - 1:
        tip = [lerp(outer[i], inner[i], 0.38) for i in range(idx(0.93), n - 2)]
        svg.path(poly_d(tip, closed=False), stroke=shade(HORN2, 1.12 * k), sw=0.65, sop=0.45, cap="round")
    # contour (plus sombre), le long des bords seulement : pas de trait en travers de la racine
    edge = shade("#3a2f26", k)
    if i1 == n - 1:
        svg.path(poly_d(outer[i0:] + list(reversed(inner[i0:])), closed=False), stroke=edge, sw=0.28,
                 sop=0.8, join="round", cap="round")
    else:
        svg.path(poly_d(outer[i0:i1 + 1], closed=False), stroke=edge, sw=0.28, sop=0.8, cap="round")
        svg.path(poly_d(inner[i0:i1 + 1], closed=False), stroke=edge, sw=0.28, sop=0.8, cap="round")
    return outer, inner


# ====================================================================== TÊTE
HEAD_LOCAL = {
    "dorsal": [(-9.0, -1.0), (-6.5, -3.2), (-3.0, -4.2), (0.5, -4.3), (4.0, -4.05), (8.0, -3.35),
               (11.5, -2.2), (14.2, -0.6), (15.8, 1.0), (16.3, 2.4), (16.1, 3.4), (15.3, 4.25)],
    "mouth": [(15.3, 4.25), (12.8, 4.55), (10.6, 4.7), (9.3, 4.8)],
    "notch": [(9.3, 4.8), (8.2, 5.7), (6.9, 7.0), (5.6, 8.25)],
    "mandible": [(5.6, 8.25), (3.0, 9.0), (0.0, 10.0), (-2.5, 11.2), (-4.8, 10.8), (-6.6, 8.6),
                 (-8.2, 5.5), (-9.2, 2.0), (-9.0, -1.0)],
}

CREST = [P(112.5, 25.0), P(107.2, 28.4), P(102.8, 32.2), P(99.6, 35.4), P(97.0, 38.2)]
NECK_BASE = [P(97.0, 38.2), P(96.2, 42.8), P(97.8, 47.4), P(101.4, 50.6), P(106.0, 52.6), P(110.6, 53.6)]
VENTRAL = [P(110.6, 53.6), P(113.8, 51.6), P(117.0, 49.0), P(120.4, 46.9), P(124.2, 45.7), P(128.6, 45.4)]
NECK_TOP = [P(128.6, 45.4), P(128.9, 42.2), P(124.0, 38.0), P(121.4, 32.0), P(118.4, 26.6), P(112.5, 25.0)]
NECK_POLY = sample_chains([CREST, NECK_BASE, VENTRAL, NECK_TOP], n=8)


def collar():
    """Le cou : l'encolure, qui porte la tête et descend avec elle pour brouter."""
    d = poly_d(NECK_POLY)
    g = svg.lin(112, 26, 115, 47, [(0, CHES), (0.3, mix(CHES, CHOC, 0.4)), (0.56, BLACK), (1, BLACK)])
    svg.path(d, fill=g)
    # la base du cou fonce et se fond dans le noir de l'avant du corps
    gbase = svg.lin(98, 48, 106, 38, [(0, BLACK), (0.45, BLACK, 0.85), (1, BLACK, 0.0)])
    svg.path(d, fill=gbase)
    # crinière le long de la crête du cou : une bande plus sombre, au poil un peu plus long
    mane = [P(114.0, 24.8), P(107.2, 28.4), P(102.8, 32.2), P(99.6, 35.4), P(97.4, 38.0),
            P(99.6, 39.2), P(103.4, 36.0), P(107.8, 32.4), P(112.2, 29.4), P(116.0, 28.0)]
    svg.path(smooth_d(mane, True), fill=CHOC_SH, fop=0.75)
    # poil : côté du cou (vers le bas et l'arrière), crinière (vers l'arrière, un peu vers
    # le haut), jabot du haut (pendant)
    def flow(p):
        return norm((-0.75, 0.66))

    def col(p):
        x, y = p
        r = RNG.random()
        base = CHES if (y < 31 + (116 - x) * 0.3 and x > 104) else BLACK
        if base == CHES:
            return RNG.choice([CHES_LT, CHES_SH, CHES, CHOC]) if r < 0.8 else CHOC_LT
        return RNG.choice([BLACK_LT, BLACK_SH, "#2e241e"])
    inner = [lerp(p, P(110, 40), 0.06) for p in NECK_POLY]
    hair_strokes(svg, inner, nhair(220), flow, col, 1.6, 2.8, 0.22, 0.34, 0.3, 0.6, curl=0.2, jitter=0.2)
    for i in range(nhair(40)):
        t = RNG.random()
        a = lerp(CREST[0], CREST[-2], t)
        a = add(a, (RNG.uniform(-0.5, 0.5), RNG.uniform(0.0, 1.2)))
        e = add(a, (RNG.uniform(-2.6, -1.4), RNG.uniform(-0.9, 0.1)))
        svg.path(f"M{f2(a[0])} {f2(a[1])}Q{f2((a[0]+e[0])/2)} {f2(min(a[1],e[1])-0.5)} {f2(e[0])} {f2(e[1])}",
                 stroke=RNG.choice([CHOC_SH, TOPLINE, BLACK, CHOC]), sw=RNG.uniform(0.3, 0.45),
                 sop=RNG.uniform(0.6, 0.9), cap="round")
    # jabot du haut : de longs poils noirs qui pendent de la gorge, le long de la ligne ventrale
    vpoly = sample_spline(VENTRAL, False, n=10)
    for i in range(0 if VARIANTE else nhair(100)):
        j = RNG.randrange(len(vpoly))
        t = j / (len(vpoly) - 1)            # 0 au poitrail, 1 derrière le menton
        a = vpoly[j]
        a = add(a, (RNG.uniform(-3.0, 0.2) * (1 - 0.7 * t), RNG.uniform(-1.0, 0.4)))
        L = RNG.uniform(3.0, 6.5) * (1.0 - 0.8 * t)   # courts derrière le menton, longs plus bas sur le cou
        e = (a[0] - RNG.uniform(0.3, 1.6) * (1 - 0.6 * t), min(a[1] + L, 54.6))
        svg.path(f"M{f2(a[0])} {f2(a[1])}Q{f2(a[0]+0.7)} {f2((a[1]+e[1])/2)} {f2(e[0])} {f2(e[1])}",
                 stroke=RNG.choice([BLACK, BLACK_SH, BLACK_LT, "#2a211c"]), sw=RNG.uniform(0.3, 0.5),
                 sop=RNG.uniform(0.6, 0.95), cap="round")


def head_group():
    # L'encolure (le collier) porte la tête. La corne et l'oreille du fond,
    # peintes derrière le collier, viennent maintenant après lui (dans la
    # tête) : elles sont rognées de ce qu'il cachait (voir `articule`).
    svg.open("neck")
    collar()
    svg.open("head")
    svg.open("horn-far")
    if not VARIANTE:
        horn(far=True)
    svg.close()
    # oreille du fond : presque cachée, une fine bande dépasse au-dessus de l'oreille proche
    svg.open("ear-far")
    eb = PIV_EAR_FAR
    ear_far = ear_shape(eb, (-0.9, -0.44), 7.4, 1.5)
    svg.path(smooth_d(ear_far, True), fill=shade(EAR, 0.62))
    svg.path(smooth_d(ear_far, True), stroke=shade(EAR_RIM, 0.62), sw=0.28, sop=0.7)
    svg.circle_pivot("ear-far", eb)
    svg.close()
    jaw_group()
    if not VARIANTE:
        horn(far=False, s0=0.0, s1=RINGS[2])  # la racine, derrière le poil du front
    face()
    eyes_group()
    # oreille proche : sa base sous celle de la corne, pointée vers l'arrière et un peu vers
    # le haut, dans la spirale
    svg.open("ear-near")
    eb = PIV_EAR
    ear = ear_shape(eb, (-0.88, -0.48), 8.2, 1.8)
    tipp = add(eb, mul(norm((-0.88, -0.48)), 8.2))
    ge = svg.lin(eb[0], eb[1], tipp[0], tipp[1], [(0, EAR_SH), (0.5, EAR), (1, mix(EAR, EAR_LT, 0.5))])
    svg.path(smooth_d(ear, True), fill=ge)
    # poil blanc de l'intérieur, visible le long du bord inférieur, et le liseré blanc
    inner = ear_shape(add(eb, (-0.5, 0.95)), (-0.9, -0.44), 6.2, 0.38)
    svg.path(smooth_d(inner, True), fill=EAR_HAIR, fop=0.6)
    svg.path(smooth_d(ear, True), stroke=EAR_RIM, sw=0.18, sop=0.6)
    svg.circle_pivot("ear-near", eb)
    svg.close()
    if not VARIANTE:
        horn(far=False, s0=RINGS[2], s1=1.0)
    svg.circle_pivot("head", PIV_HEAD)
    svg.circle_pivot("muzzle", MUZZLE)
    svg.close()
    svg.circle_pivot("neck", PIV_NECK)
    svg.close()


def ear_shape(base, d, L, w):
    d = norm(d)
    n = perp(d)   # pointe « vers le bas » pour une oreille dirigée vers l'arrière et le haut
    prof = [(0.0, 0.95), (0.12, 1.0), (0.35, 0.95), (0.6, 0.72), (0.82, 0.42), (1.0, 0.0)]
    up = [add(base, add(mul(d, L * t), mul(n, -w * k))) for t, k in prof]
    lo = [add(base, add(mul(d, L * t), mul(n, w * k * 1.1))) for t, k in reversed(prof[:-1])]
    return up + lo


def jaw_group():
    svg.open("jaw")
    cheek = HL([(9.6, 4.6), (8.0, 4.1), (5.0, 4.6), (3.8, 7.6), (5.6, 8.4), (7.0, 7.0), (8.4, 5.6)])
    svg.path(smooth_d(cheek, True), fill=mix(FACE, MUZ, 0.6))
    mouth_in = HL([(9.1, 4.85), (10.6, 3.55), (14.2, 3.35), (15.5, 4.0), (15.1, 4.65), (12.6, 4.8), (10.2, 4.9)])
    svg.path(smooth_d(mouth_in, True), fill="#2a1a16")
    tongue = HL([(10.4, 4.75), (12.0, 4.35), (14.2, 4.3), (14.7, 4.6), (13.4, 4.85), (11.2, 4.9)])
    svg.path(smooth_d(tongue, True), fill="#8c5f58")
    jaw = HL([(15.0, 4.5), (14.85, 5.35), (13.4, 6.4), (10.6, 7.15), (7.6, 7.85), (4.6, 8.55),
              (3.6, 7.0), (5.2, 5.5), (7.6, 4.85), (9.6, 4.7), (12.6, 4.6)])
    a, b = H(9, 3.5), H(9, 8.5)
    gj = svg.lin(a[0], a[1], b[0], b[1], [(0, MUZ_LT), (0.6, MUZ), (1, MUZ_SH)])
    svg.path(smooth_d(jaw, True), fill=gj)
    lip = HL([(15.0, 4.55), (14.85, 5.35), (13.4, 6.4), (10.6, 7.15), (7.6, 7.85)])
    svg.path(smooth_d(lip, False), stroke=MUZ_SH, sw=0.3, sop=0.7, cap="round")
    svg.circle_pivot("jaw", PIV_JAW)
    svg.close()


FACE_FILL = None
EYE_T = math.radians(12)


def eye_pt(x, y):
    return rot(add(E, (x * SH, y * SH)), EYE_T, E)


def face_hair(region, n, avoid_eye=0.0):
    flow = lambda p: norm(sub(H(-1, 0), H(1, 0.25)))

    def fcol(p):
        q = sub(p, E)
        a_ = (q[0] * U[0] + q[1] * U[1]) / SH
        if a_ > 8.0:
            return RNG.choice([MUZ, MUZ_SH, MUZ_LT])
        return RNG.choice([FACE_LT, FACE_SH, FACE, FACE_GRIZ, mix(FACE, FACE_LT, 0.5)])
    weight = None
    if avoid_eye:
        weight = lambda p: 0.0 if length(sub(p, E)) < avoid_eye else 1.0
    hair_strokes(svg, region, n, flow, fcol, 0.7, 1.3, 0.15, 0.22, 0.22, 0.45, curl=0.15, jitter=0.25,
                 weight=weight)


def eye_region_marks(hole=None):
    """Tour de l'œil, trait des paupières fermées et glande préorbitaire : dessinés sur la
    tête (vus quand l'œil est fermé), puis de nouveau sur le cache de l'œil (vus quand il est
    ouvert, l'ouverture évidée)."""
    ring = [eye_pt(2.05 * math.cos(t), 1.6 * math.sin(t)) for t in [i * math.pi / 8 for i in range(16)]]
    gring = svg.rad(E[0], E[1], 2.1 * SH, [(0, RING_LT), (0.62, RING), (0.8, RING, 0.55), (1, RING, 0.0)])
    if hole is None:
        svg.path(smooth_d(ring, True), fill=gring)
    else:
        svg.path(smooth_d(ring, True) + poly_d(list(reversed(hole))), fill=gring, extra='fill-rule="evenodd"')
    g0, g1 = eye_pt(1.75, 0.45), eye_pt(2.95, 1.3)
    svg.path(f"M{f2(g0[0])} {f2(g0[1])}Q{f2((g0[0]+g1[0])/2+0.1)} {f2((g0[1]+g1[1])/2-0.2)} {f2(g1[0])} {f2(g1[1])}",
             stroke=FACE_SH, sw=0.36, sop=0.85, cap="round")


def face():
    global FACE_FILL
    chains = [HL(HEAD_LOCAL["dorsal"]), HL(HEAD_LOCAL["mouth"]), HL(HEAD_LOCAL["notch"]),
              HL(HEAD_LOCAL["mandible"])]
    hpoly = sample_chains(chains, n=8)
    hd = poly_d(hpoly)
    a, b = H(-2, -4.5), H(-1, 11)
    FACE_FILL = svg.lin(a[0], a[1], b[0], b[1], [(0, FACE_LT), (0.4, FACE), (1, FACE_SH)])
    svg.path(hd, fill=FACE_FILL)
    # joue plus sombre sur le masséter, qui reste sous la région de l'œil
    ck = H(-1.0, 8.2)
    svg.path(smooth_d(HL([(-7, 6), (-2, 5.2), (5, 6.2), (6.5, 8.4), (1, 10.6), (-6, 10.4)]), True),
             fill=svg.rad(ck[0], ck[1], 5.5, [(0, FACE_SH, 0.6), (1, FACE_SH, 0.0)]))
    # une bande plus sombre sur le chanfrein, là où le gris-brun rejoint le masque blanc
    nb = H(5.6, -1.6)
    svg.path(smooth_d(HL([(2.8, -3.6), (6.6, -3.3), (8.2, -2.4), (8.0, 1.4), (5.4, 2.2), (2.8, 0.6)]), True),
             fill=svg.rad(nb[0], nb[1], 3.6 * SH, [(0, FACE_SH, 0.45), (1, FACE_SH, 0.0)]))
    # masque blanc du museau, qui remonte le chanfrein jusqu'à mi-chemin de l'œil environ
    mask = HL([(7.0, -3.1), (11.5, -1.9), (14.2, -0.4), (15.8, 1.1), (16.3, 2.4), (16.1, 3.4), (15.3, 4.25),
               (12.8, 4.55), (10.6, 4.7), (9.3, 4.8), (8.2, 5.7), (7.2, 6.6), (6.2, 4.0), (6.4, 0.5)])
    m0, m1 = H(5.8, 0), H(10.5, 0)
    gm = svg.lin(m0[0], m0[1], m1[0], m1[1], [(0, MUZ, 0.0), (0.5, MUZ, 0.8), (1, MUZ, 1.0)])
    svg.path(smooth_d(mask, True), fill=gm)
    # texture du poil de la face (ras, serré, couché vers l'arrière), grisonnant sur le front
    face_hair([lerp(p, E, 0.07) for p in hpoly], nhair(150))
    fore = HL([(-2.5, -4.0), (4.0, -3.8), (7.0, -3.0), (5.0, -1.6), (-1.0, -2.2)])
    hair_strokes(svg, fore, nhair(26), lambda p: norm(sub(H(-1, 0), H(1, 0.3))),
                 [FACE_GRIZ, RING_SH, FACE_LT], 0.6, 1.0, 0.13, 0.18, 0.4, 0.7, curl=0.1, jitter=0.3)
    # poil du front sur la racine de la corne (la corne sort du crâne dans une collerette de poils)
    dors = sample_spline(HL(HEAD_LOCAL["dorsal"][:5]), False, n=10)
    for i in range(nhair(70)):
        q = dors[RNG.randrange(len(dors))]
        p = add(q, (RNG.uniform(-0.3, 0.3), RNG.uniform(0.15, 0.9)))
        e = add(p, (RNG.uniform(-0.55, 0.15), RNG.uniform(-1.05, -0.55)))
        svg.path(f"M{f2(p[0])} {f2(p[1])}Q{f2((p[0]+e[0])/2+0.15)} {f2((p[1]+e[1])/2)} {f2(e[0])} {f2(e[1])}",
                 stroke=RNG.choice([FACE, FACE_LT, mix(FACE, FACE_LT, 0.5), FACE_GRIZ]), sw=RNG.uniform(0.16, 0.24),
                 sop=RNG.uniform(0.55, 0.85), cap="round")
    # truffe (planum nasale) et la fente de la narine, en S
    pad = HL([(15.0, 0.25), (15.85, 1.0), (16.35, 2.3), (16.1, 3.1), (15.3, 2.45), (14.8, 1.25)])
    svg.path(smooth_d(pad, True), fill=NOSE)
    svg.path(smooth_d(HL([(15.2, 0.6), (15.85, 1.2), (16.15, 2.1)]), False), stroke=NOSE_LT, sw=0.18, sop=0.7, cap="round")
    nos = HL([(14.0, 1.0), (14.7, 1.35), (15.05, 1.95), (15.6, 2.55)])
    svg.path(smooth_d(nos, False), stroke=NOSTRIL, sw=0.42, cap="round")
    # sillon du philtrum, qui fend la lèvre supérieure
    ph = HL([(16.05, 3.0), (15.75, 3.7), (15.45, 4.2)])
    svg.path(smooth_d(ph, False), stroke=MUZ_SH, sw=0.2, sop=0.8, cap="round")
    # ligne de la bouche
    svg.path(smooth_d(HL(HEAD_LOCAL["mouth"]), False), stroke=MOUTH, sw=0.3, sop=0.85, cap="round")
    # contour (ni le long de l'encoche de la mâchoire, ni sous la racine de la corne)
    svg.path(smooth_d(HL(HEAD_LOCAL["dorsal"][3:]), False), stroke=OUTLINE, sw=0.28, sop=0.55, cap="round")
    svg.path(smooth_d(HL(HEAD_LOCAL["mandible"][:5]), False), stroke=OUTLINE, sw=0.28, sop=0.45, cap="round")
    # tour de l'œil, paupières fermées et glande (vus quand l'œil se ferme)
    eye_region_marks()
    svg.path(smooth_d([eye_pt(-1.35, 0.05), eye_pt(0.0, 0.3), eye_pt(1.35, 0.1)], False),
             stroke=LIDRIM, sw=0.32, cap="round")


def eyes_group():
    svg.open("eyes")
    almond_up = [eye_pt(-1.38, 0.05), eye_pt(-0.7, -0.62), eye_pt(0.05, -0.86), eye_pt(0.8, -0.6), eye_pt(1.38, 0.08)]
    almond_lo = [eye_pt(1.38, 0.08), eye_pt(0.7, 0.62), eye_pt(-0.05, 0.74), eye_pt(-0.8, 0.52), eye_pt(-1.38, 0.05)]
    almond = sample_chains([almond_up, almond_lo], n=6)
    svg.path(poly_d(almond), fill="#3a2614")
    # iris et fente horizontale, que le moteur garde à l'horizontale. Dessinés un peu en arrière
    # du centre : au repos, le moteur avance le regard d'environ (0,35 ; 0,05) de sa course.
    svg.open("pupils")
    ic = add(E, (-0.82, -0.12))
    ir = 0.98 * SH
    gi = svg.rad(ic[0], ic[1], ir, [(0, IRIS_GLINT), (0.45, IRIS), (0.85, IRIS_EDGE), (1, "#5a3b18")])
    svg.path(f"M{f2(ic[0]-ir)} {f2(ic[1])}A{f2(ir)} {f2(ir)} 0 1 0 {f2(ic[0]+ir)} {f2(ic[1])}"
             f"A{f2(ir)} {f2(ir)} 0 1 0 {f2(ic[0]-ir)} {f2(ic[1])}Z", fill=gi)
    for k in range(12):
        ang = k * math.pi / 6 + 0.2
        p0 = add(ic, (0.45 * SH * math.cos(ang), 0.45 * SH * math.sin(ang)))
        p1 = add(ic, (0.9 * SH * math.cos(ang), 0.9 * SH * math.sin(ang)))
        svg.path(f"M{f2(p0[0])} {f2(p0[1])}L{f2(p1[0])} {f2(p1[1])}", stroke=IRIS_EDGE, sw=0.07, sop=0.55)
    pw, ph = 1.24 * SH, 0.31 * SH
    svg.path(f"M{f2(ic[0]-pw/2+ph/2)} {f2(ic[1]-ph/2)}L{f2(ic[0]+pw/2-ph/2)} {f2(ic[1]-ph/2)}"
             f"A{f2(ph/2)} {f2(ph/2)} 0 0 1 {f2(ic[0]+pw/2-ph/2)} {f2(ic[1]+ph/2)}"
             f"L{f2(ic[0]-pw/2+ph/2)} {f2(ic[1]+ph/2)}A{f2(ph/2)} {f2(ph/2)} 0 0 1 {f2(ic[0]-pw/2+ph/2)} {f2(ic[1]-ph/2)}Z",
             fill=PUPIL)
    # granula iridica : des grains sombres, plus gros et plus nombreux sur le bord supérieur
    for gx, gs in ((-0.34, 0.1), (-0.08, 0.13), (0.18, 0.12), (0.4, 0.08)):
        svg.path(f"M{f2(ic[0]+gx-gs)} {f2(ic[1]-ph/2+0.03)}Q{f2(ic[0]+gx)} {f2(ic[1]-ph/2-gs*1.1)} "
                 f"{f2(ic[0]+gx+gs)} {f2(ic[1]-ph/2+0.03)}Z", fill=GRANULA)
    svg.circle_pivot("pupils", ic)
    svg.close()
    # reflets sur la cornée
    h1 = eye_pt(-0.42, -0.4)
    svg.path(f"M{f2(h1[0]-0.26)} {f2(h1[1])}a0.26 0.19 0 1 0 0.52 0a0.26 0.19 0 1 0 -0.52 0Z", fill="#ffffff", fop=0.9)
    h2 = eye_pt(0.55, 0.32)
    svg.path(f"M{f2(h2[0]-0.1)} {f2(h2[1])}a0.1 0.08 0 1 0 0.2 0a0.1 0.08 0 1 0 -0.2 0Z", fill="#ffffff", fop=0.55)
    # la région de l'œil, peinte du dégradé même de la face, autour de l'ouverture : elle
    # cache l'iris quand le regard glisse au-delà des paupières
    mc = add(E, (-0.9, -0.1))
    outer = [add(mc, (4.0 * math.cos(q), 3.3 * math.sin(q))) for q in [i * math.pi / 14 for i in range(28)]]
    svg.path(poly_d(outer) + poly_d(list(reversed(almond))), fill=FACE_FILL, extra='fill-rule="evenodd"')
    face_hair([lerp(p, mc, 0.05) for p in outer], nhair(40), avoid_eye=1.9)
    eye_region_marks(hole=almond)
    svg.path(smooth_d(almond_up, False), stroke=LIDRIM, sw=0.27, cap="round")
    svg.path(smooth_d(almond_lo, False), stroke=LIDRIM, sw=0.18, sop=0.8, cap="round")
    # quelques cils fins sur la paupière supérieure
    for k in range(5):
        p = lerp(almond_up[1], almond_up[3], k / 4)
        e = add(p, (0.15 + 0.08 * k, -0.45))
        svg.path(f"M{f2(p[0])} {f2(p[1])}L{f2(e[0])} {f2(e[1])}", stroke=LIDRIM, sw=0.08, sop=0.7, cap="round")
    svg.circle_pivot("eyes", E)
    svg.close()



# ====================================================================== ASSEMBLAGE
def main():
    leg_group("leg-back-far", "back", 5.5 + HQ, PIV_BF, True)
    leg_group("leg-front-far", "front", -5.5, PIV_FF, True)
    leg_group("leg-back-near", "back", HQ, PIV_BN, False)
    leg_group("leg-front-near", "front", 0.0, PIV_FN, False)
    body_group()
    tail_group()
    head_group()
    svg.emit(f'<circle id="pivot-ground" cx="{f2(GROUND_X)}" cy="{f2(GROUND)}" r="0.8"/>')
    comment = ("    Mouflon de Corse : bélier adulte (environ huit ans) du Cinto, en pelage d'hiver,\n"
               "    vu de profil. 1 unité = 1 cm ; garrot à 75 cm, sol à y = 112.\n"
               "    L'encolure (neck, le collier) porte la tête, qui tourne à la nuque ; les\n"
               "    pattes sont coupées au carpe et au jarret (tools/races/articulations.py).\n"
               "    © 2026 Alejandro San Martin Lamas — MIT OR Apache-2.0")
    if VARIANTE:
        return variante(OUT, articule(svg.render(170, 116, comment)))
    with open(OUT, "w") as f:
        f.write(articule(svg.render(170, 116, comment)))


def variante(path, texte):
    """La mouflonne ou l'agneau : ce qui change du bélier, puis ses proportions."""
    root = ET.fromstring(texte)
    # Le pelage d'hiver de la femelle : brun beige, sans le noir du jabot et du
    # poitrail ; l'agneau, fauve plus clair.
    clair = variantes.teinte("#6E5644" if VARIANTE == "brebis" else "#A47E5A", .5)
    variantes.recolorer(root, clair, dans=("neck", "body"), sauf=("head",),
                        si=lambda c: variantes.clarte(c) < .3)
    if VARIANTE == "agneau":
        variantes.recolorer(root, variantes.teinte("#B08559", .35), dans=("neck", "body", "tail"),
                            sauf=("head",), si=lambda c: .12 < variantes.clarte(c) < .6)
    variantes.ecrire(path, root, **PROPORTIONS[VARIANTE])


# La lèvre supérieure (repère de la tête) : sous la narine et le bas de la
# truffe, jusque sous la ligne de la bouche ; elle se retrousse autour de son
# arrière pour le flehmen et découvre la gencive (le bourrelet du haut, sans
# incisives), rose sous le museau blanc.
LIP = [(12.4, 3.6), (13.6, 3.35), (14.8, 3.1), (15.6, 2.95), (17.2, 2.8), (17.2, 5.0), (15.3, 4.75),
       (13.6, 4.95), (12.4, 5.05)]
LIP_PIVOT = (12.6, 3.8)
GUM = [(12.7, 3.95), (14.0, 3.75), (15.0, 3.55), (15.6, 3.5), (15.85, 3.85), (15.2, 4.02), (14.0, 4.22),
       (12.8, 4.33)]
GUM_RIDGE = [(13.0, 4.15), (14.1, 4.0), (15.0, 3.85), (15.5, 3.75)]
GUM_FILL, GUM_CREASE = "#c98a94", "#a0606e"


def articule(text):
    """Les pattes coupées au carpe (la bosse du genou) et au jarret (sa
    pointe) ; la corne et l'oreille du fond rognées de ce que le collier
    cachait quand il était peint par-dessus elles ; la lèvre supérieure,
    détachée de la tête, avec la gencive qu'elle cache : au repos, rien ne
    change."""
    def extra(root, paints):
        for gid in ("horn-far", "ear-far"):
            articulations.clip_away(root, paints, gid, NECK_POLY)
        articulations.lift_part(root, paints, "head", HL(LIP), "lip", H(*LIP_PIVOT),
                                under=articulations.gum(HL(GUM), HL(GUM_RIDGE), GUM_FILL, GUM_CREASE, 0.22))
    return articulations.articulate_breed(text, carpus_y=89.0, hock_y=84.0, extra=extra)


if __name__ == "__main__":
    main()
