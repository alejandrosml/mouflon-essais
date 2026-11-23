#!/usr/bin/env python3
"""Dessine un bélier Herdwick de trois ans et demi, fin octobre, sans teinture.

Une unité = un centimètre de l'animal ; x vers l'avant, h au-dessus du sol.
La silhouette et les poils suivent docs/races/herdwick.md et les photographies
de la HSBA : un tronc profond, de la laine courte gris bleuté chinée, une
crinière sombre et une tête forte, blanche, sans toupet. Les cornes sont de
section ronde et finement striées, serrées derrière l'œil, sans grosses côtes.

Usage : python3 tools/races/herdwick/gen_herdwick.py pets/herdwick/herdwick.svg
© 2026 Alejandro San Martin Lamas — MIT OR Apache-2.0
"""
import math
import os
import random
import sys
import xml.etree.ElementTree as ET

HERE = os.path.dirname(os.path.abspath(__file__))
sys.path.insert(0, os.path.dirname(HERE))
sys.path.insert(0, HERE)
import articulations as A  # noqa: E402
import variantes  # noqa: E402

# Les autres dessins de la race (tools/races/variantes.py), d'après la fiche.
# Sans variante : le bélier.
VARIANTE = sys.argv[2] if len(sys.argv) > 2 else ""
PROPORTIONS = {
    "brebis": dict(  # Brebis Herdwick : 63 cm au garrot, sans cornes ni crinière, ossature plus fine, tête féminine, toison un peu plus claire.
        garrot=(70, 63), pattes=(.92, 1.0), encolure=(1.0, .88), tete=(.98, .93)),
    "agneau": dict(  # Agneau Herdwick de deux semaines : 38 cm au garrot, né tout noir, le bout des oreilles givré.
        variantes.AGNEAU, garrot=(70, 38)),
}
from shapely.geometry import Point, Polygon, LineString  # noqa: E402
from outils import (Doc, S, add, clamp, cr, dense, dist, element, geom_d, lerp,
                    lstroke, mix, mul, norm, poisson, poly, qstroke, rot,
                    runs_path, shade, ramp, smooth, sub)  # noqa: E402

OUT = sys.argv[1] if len(sys.argv) > 1 else "herdwick.svg"
rng = random.Random(1916)
doc = Doc()

# ------------------------------------------------------------------ mesures (§ 2)
H_CARPE, H_JARRET = 16.5, 22.5
POLL = (104.0, 86.0)
NECK_PIVOT = (84.0, 52.0)
TH = math.radians(37.0)
HU, HV = (math.cos(TH), -math.sin(TH)), (-math.sin(TH), -math.cos(TH))
KU, KV = 29.0 / 23.08, 1.25

def HL(u, v):
    return add(POLL, add(mul(HU, u * KU), mul(HV, v * KV)))

def HLs(points):
    return [HL(*p) for p in points]

def hang(du, dv):
    return math.atan2(du * KU * HU[1] + dv * KV * HV[1], du * KU * HU[0] + dv * KV * HV[0])

# Un front large, une bosse douce, une ganache profonde et un mufle carré.
# Le menton est une pièce mobile cachant l'intérieur de la bouche au repos.
DORSAL = [(-1.6, 0), (-1.3, -1.5), (0, -2.9), (2, -3.9), (4.6, -4.25),
          (7, -4.2), (9.5, -4.0), (12, -3.55), (14.5, -2.85), (17, -1.9),
          (19.2, -.7), (20.8, .15), (22, 1.1), (22.8, 2.1), (23.08, 3),
          (23.03, 3.9), (22.7, 4.82)]
MOUTH = [(22.7, 4.82), (21.7, 4.98), (20.5, 5.13), (19.1, 5.32), (17.7, 5.52)]
NOTCH = [(17.7, 5.52), (17.1, 6.75), (16.3, 8.45)]
UNDER = [(16.3, 8.45), (14.2, 10.5), (11.8, 12.3), (9.3, 13.25), (6.8, 13.1),
         (4.8, 12.5), (3.1, 11.35), (1.7, 9.8), (.6, 7.8), (-.3, 5.55),
         (-.85, 3.95), (-1.25, 2.25), (-1.52, 1.05), (-1.6, 0)]
HEAD_OUT = HLs(DORSAL + MOUTH[1:] + NOTCH[1:] + UNDER[1:-1])
HEAD_POLY = Polygon(dense(HEAD_OUT, True, 6)).buffer(0)
EYE_L, EAR_L, EAR_FAR_L = (8.2, .05), (2.8, 5.0), (.4, 4.0)
JAW_L, LIP_PIVOT_L, MUZZLE_L = (17, 5.9), (19.25, 3.9), (22.5, 5.3)
E = HL(*EYE_L)
EYE_TILT, EYE_SCALE = math.radians(-7), 1.08
EU = (math.cos(EYE_TILT), math.sin(EYE_TILT))
EV = (EU[1], -EU[0])

def EL(a, b):
    return add(E, add(mul(EU, a * EYE_SCALE), mul(EV, b * EYE_SCALE)))

ALMOND_L = [(-1.38, .05), (-1.08, -.46), (-.55, -.73), (.1, -.78), (.72, -.63),
            (1.18, -.28), (1.38, .16), (1.02, .46), (.42, .62), (-.25, .62),
            (-.86, .46), (-1.2, .26)]
ALMOND = [EL(*p) for p in ALMOND_L]
IRIS_C, IRIS_R = EL(-.3, -.04), 1.22 * EYE_SCALE

# ------------------------------------------------------------------ palette (§ 4)
FACE, FACE_LT, FACE_SH = "#E3E2DD", "#F7F7F2", "#B5B5AD"
COLD, COLD_LT, COLD_SH = "#AFB9BC", "#CDD3D2", "#8E9A9E"
CHANF_LT = "#EBEDE7"
SKIN, SKIN_LT, SKIN_SH = "#C5C7BC", "#E0E2D8", "#7D7E76"
NOSTRIL, NOSTRIL_DEEP = "#242322", "#0B0B0B"
LIPS, LIPS_LT, LIPS_SH = "#CDCEC5", "#E5E6DC", "#66665E"
EAR, EAR_LT, EAR_SH = "#DDDED7", "#F5F5ED", "#ADADA4"
EAR_IN_SH = "#64413A"
LASH, LASH_LT = "#E8E7DD", "#FCFCF4"
IRIS, IRIS_LT, IRIS_RING, IRIS_GOLD, PUPIL = "#7A6248", "#9C8364", "#493B2E", "#A18B62", "#121212"
GUM, GUM_SH = "#B77E82", "#824B54"
HEAD_STOPS = [(0, "#FAFAF4"), (.25, "#ECEDE6"), (.48, FACE), (.72, "#CCCEC7"), (1, "#A8B1B1")]

# ------------------------------------------------------------------ silhouette laineuse
# 70 cm au garrot à la peau ; crinière 76,6 cm, croupe laineuse 72,5 cm.
BACK = [(0, 59), (.4, 64), (3, 68), (8, 70.7), (17, 72.2), (28, 72.2),
        (42, 72.7), (55, 73.3), (67, 74.3), (75, 76), (80, 76.6)]
SHOULDER = [(80, 76.6), (84, 72), (85.7, 65), (86.5, 56), (88.7, 44), (90, 36), (85, 30)]
BELLY = [(85, 30), (82, 30), (80, 33.3), (78, 34.5), (77, 31), (74, 28.6), (62, 28), (49, 28.2), (37, 29.2), (26, 31.5),
         (21, 35), (16, 37), (9, 39), (4, 43), (1, 49), (0, 59)]
BODY_PTS = BACK + SHOULDER[1:] + BELLY[1:-1]
BODY_POLY = Polygon(dense(BODY_PTS, True, 8)).buffer(0)
NECK_OUT = [(60, 73), (72, 75), (80, 79), (88, 83.5), (96, 88), (104.5, 89),
            (108, 84), (107, 76), (103, 69), (99, 61), (97, 50), (94, 39),
            (89, 32.5), (79, 34), (66, 43), (56, 59)]
NECK_POLY = Polygon(dense(NECK_OUT, True, 8)).buffer(0)
FLEECE_STOPS = [(0, "#878B91"), (.2, "#777B82"), (.46, "#62656D"), (.7, "#4D5058"), (1, "#30343B")]


def _arc(start, h0, h1, radius, left=False):
    sign = 1 if left else -1
    a = math.radians(h0 + sign * 90)
    c = add(start, (radius * math.cos(a), radius * math.sin(a)))
    n = max(2, int(abs(h1 - h0) / 4) + 1)
    return [add(c, (radius * math.cos(math.radians(h0 + (h1 - h0) * i / n - sign * 90)),
                    radius * math.sin(math.radians(h0 + (h1 - h0) * i / n - sign * 90)))) for i in range(n + 1)]


def _ray(start, angle, length):
    n = max(1, int(length))
    d = (math.cos(math.radians(angle)), math.sin(math.radians(angle)))
    return [add(start, mul(d, length * i / n)) for i in range(1, n + 1)]


def _turn(p, a):
    return add(NECK_PIVOT, rot(sub(p, NECK_PIVOT), -a))


def neck_geometry():
    """Même construction tangente que la Lacaune : le cou baissé prolonge
    l'arrondi du garrot au lieu de découvrir un coin de laine cachée."""
    global SHOULDER, BODY_PTS, BODY_POLY, NECK_OUT, NECK_POLY, CREST, THROAT
    withers = BACK[-1]
    h0 = math.degrees(math.atan2(BACK[-1][1] - BACK[-2][1], BACK[-1][0] - BACK[-2][0]))
    shoulder_arc = _arc(withers, h0, -40, 11)
    tangent = shoulder_arc[-1]
    shoulder_line = shoulder_arc + _ray(tangent, -40, 8.5)
    SHOULDER = shoulder_line + [(97, 63), (96, 53), (94, 44), (90, 35), (85, 30)]
    BODY_PTS = BACK + SHOULDER[1:] + BELLY[1:-1]
    BODY_POLY = Polygon(dense(BODY_PTS, True, 8)).buffer(0)
    a = 1.63
    v = sub(withers, NECK_PIVOT)
    start = math.degrees(math.atan2(v[0], -v[1])) % 360
    finish = -40 + 180 + math.degrees(a)
    normal = (math.sin(math.radians(40)), math.cos(math.radians(40)))
    target = tangent[0] * normal[0] + tangent[1] * normal[1] - .05
    lo, hi = .5, 40
    for _ in range(60):
        radius = (lo + hi) / 2
        q = _turn(_arc(withers, start, finish, radius, True)[-1], a)
        if q[0] * normal[0] + q[1] * normal[1] > target:
            hi = radius
        else:
            lo = radius
    hidden = _arc(withers, start, finish, radius, True)
    q = _turn(hidden[-1], a)
    direction = (math.cos(math.radians(-40)), math.sin(math.radians(-40)))
    length = max(0, (q[0] - tangent[0]) * direction[0] + (q[1] - tangent[1]) * direction[1] - .6)
    hidden += _ray(hidden[-1], finish, length)
    hidden += _arc(hidden[-1], finish, finish + 80, 3, True)[1:]
    CREST = [HL(-1.6, 0), (98, 88), (93, 86), (88, 83.5), (84, 81), withers]
    THROAT = [(91, 36), (95, 43), (98, 53), (99, 60), HL(6.8, 13.1)]
    NECK_OUT = CREST + hidden[1:] + [(76, 46), (78, 38), (86, 33)] + THROAT + \
        HLs([(7.2, 10), (7, 6), (6, 1), (3, -2), (-1, -1)])
    NECK_POLY = Polygon(dense(NECK_OUT, True, 8)).buffer(0)


neck_geometry()


# Le modelé du poil ras, les paupières et la bouche réemploient les outils
# éprouvés de la Lacaune, avec un canon et une palette propres au Herdwick.
def strokes_in(region, r, flow, length, style, rng_, skip=None, curl=0.18, nd=1):
    """Sème des poils (courts traits courbes) dans `region` : `flow(p)` donne
    leur direction, `style(p)` leur (couleur, largeur, opacité) ; ils sont
    regroupés par style dans des tracés communs (voir Doc.stroke)."""
    pts = poisson(region, r, rng_)
    n = 0
    for p in pts:
        if skip and skip(p):
            continue
        d = flow(p)
        L = rng_.uniform(*length)
        e = add(p, mul(d, L))
        nn = (-d[1], d[0])
        c = add(lerp(p, e, 0.5), mul(nn, rng_.uniform(-curl, curl) * L))
        st = style(p)
        if st is None:
            continue
        col, sw, so = st
        doc.stroke(qstroke(p, c, e, nd), col, sw, so)
        n += 1
    return n

def soft(shape_d, stops, p1, p2, gid):
    """Un modelé : la forme `shape_d` peinte d'un dégradé linéaire (transparent
    aux bouts, le plus souvent)."""
    doc.path(shape_d, fill=doc.lin(gid, p1, p2, stops))

def ellipse(c, rx, ry, angle=0.0, n=20):
    return [add(c, rot((rx * math.cos(2 * math.pi * k / n), ry * math.sin(2 * math.pi * k / n)), angle))
            for k in range(n)]

def glow(gid, c, rx, ry, color, alpha, angle=0.0, inner=0.0, clip=None):
    """Une ombre ou une lumière douce : une ellipse peinte d'un dégradé radial
    qui s'éteint au bord ; `clip` la garde dans une silhouette."""
    stops = [(0, color, alpha), (inner, color, alpha), (1, color, 0.0)] if inner else \
        [(0, color, alpha), (1, color, 0.0)]
    fill = doc.rad(gid, c, ry, stops, sx=rx / ry, angle=angle)
    if clip is None:
        doc.path(cr(ellipse(c, rx, ry, angle)), fill=fill)
    else:
        shape = Polygon(dense(ellipse(c, rx, ry, angle), True, 4)).buffer(0).intersection(clip)
        if not shape.is_empty:
            doc.path(geom_d(shape, simplify=0.02), fill=fill)

def band(pts, w0, w1, closed=False):
    """Un ruban le long d'une ligne de points, de largeur w0 à w1."""
    D = dense(pts, False, 6)
    left, right = [], []
    for i, p in enumerate(D):
        a = D[max(0, i - 1)]
        b = D[min(len(D) - 1, i + 1)]
        t = norm(sub(b, a))
        nn = (-t[1], t[0])
        w = w0 + (w1 - w0) * i / (len(D) - 1)
        left.append(add(p, mul(nn, w / 2)))
        right.append(add(p, mul(nn, -w / 2)))
    return left + right[::-1]

def groove(gid, pts, width, color, alpha, clip):
    """Un sillon ou un relief doux le long d'une ligne : une suite de lueurs
    allongées qui s'éteignent de tous côtés (pas de bord dur)."""
    for k, (p, q) in enumerate(zip(pts, pts[1:])):
        c = lerp(p, q, 0.5)
        L = dist(p, q)
        ang = math.atan2(q[1] - p[1], q[0] - p[0])
        glow(f"{gid}{k}", c, L * 0.7 + width * 0.5, width / 2, color, alpha, angle=ang, clip=clip)

def ear_frame(base, angle_deg):
    a = math.radians(angle_deg)
    d = (-math.cos(a), -math.sin(a))        # vers l'arrière, `angle_deg` sous l'horizontale
    n = (-d[1], d[0])                       # vers le bas de l'oreille
    return lambda u, v: add(base, add(mul(d, u), mul(n, v)))

EAR_UP = [(-0.9, -0.9), (0.6, -1.3), (2.2, -1.95), (3.9, -2.35), (5.6, -2.45), (7.3, -2.25), (8.7, -1.75),
          (9.8, -1.0), (10.5, -0.15), (10.65, 0.55)]

EAR_LO = [(10.65, 0.55), (10.3, 1.35), (9.4, 2.0), (8.0, 2.45), (6.3, 2.6), (4.5, 2.5), (2.7, 2.1), (1.1, 1.4),
          (-0.9, 0.9)]

EAR_CUP = [(0.9, 0.45), (2.5, 0.3), (4.3, 0.18), (6.1, 0.15), (7.8, 0.28), (9.05, 0.55), (9.85, 0.85), (9.85, 1.25),
           (9.1, 1.7), (7.9, 2.08), (6.3, 2.22), (4.5, 2.12), (2.8, 1.78), (1.3, 1.2), (0.7, 0.85)]

def ear_draw(F, gid, far=False):
    """Une oreille longue et large, épaisse, au bout arrondi, portée à
    l'horizontale (§ 5) : dehors blanc au poil ras, et en dessous son creux
    rose en cuillère, ombré sous le pli du bord du haut, éclairé vers le bas,
    bordé d'un liseré blanc (l'épaisseur de l'oreille)."""
    k = 0.74 if far else 1.0
    out = [F(*p) for p in EAR_UP + EAR_LO[1:-1]]
    g = doc.lin(gid, F(4.0, -2.4), F(4.0, 2.6), [(0, shade(EAR_LT, k)), (0.4, shade(EAR, k)),
                                                (1, shade("#B9BDBE", k * 0.95))])
    doc.path(cr(out), fill=g)
    # le creux rose : une surface, sombre sous le pli, claire vers le bas
    cup = [F(*p) for p in EAR_CUP]
    gi = doc.lin(gid + "-in", F(5.0, 0.15), F(5.0, 2.2),
                 [(0, shade("#9D8278", k)), (0.3, shade("#B8A298", k)), (0.7, shade("#D9CDC4", k)),
                  (1, shade("#E7DED2", k))])
    doc.path(cr(cup), fill=gi)
    cup_poly = Polygon(cup).buffer(0)
    if not far:
        # le creux luit un peu au milieu, l'ombre du pli dessus
        ea = sub(F(1, 0), F(0, 0))
        glow(gid + "-luisant", F(6.0, 1.55), 2.6, 0.38, "#F2D9D2", 0.45, angle=math.atan2(ea[1], ea[0]),
             clip=cup_poly)
        doc.path(cr([F(1.0, 0.5), F(2.6, 0.36), F(4.4, 0.25), F(6.2, 0.22), F(7.8, 0.35), F(9.1, 0.62),
                     F(9.75, 0.9)], closed=False), stroke=shade("#6E4740", k), sw=0.22, so=0.4, cap="round")
        # la nervure et les poils fins, blancs, de l'intérieur
        doc.path(cr([F(1.8, 1.15), F(4.0, 1.3), F(6.6, 1.28), F(8.6, 1.05)], closed=False),
                 stroke="#9A6A62", sw=0.14, so=0.4, cap="round")
        doc.begin_batch()
        for i in range(34):
            u = rng.uniform(1.4, 9.2)
            v = rng.uniform(0.4, 1.9) * (1 - (u - 1.4) / 16)
            p = F(u, v)
            if not cup_poly.contains(Point(p)):
                continue
            e = F(u + rng.uniform(0.4, 0.8), v + rng.uniform(0.1, 0.35))
            doc.stroke(lstroke(p, e), "#F1E4DF", 0.06, 0.55)
        doc.flush()
    # le bord du haut, éclairé, et le poil ras du dehors
    doc.path(cr([F(*p) for p in EAR_UP[1:]], closed=False), stroke=shade(EAR_LT, 1.05 * k), sw=0.24, so=0.6,
             cap="round")
    doc.path(cr([F(*p) for p in EAR_UP[4:] + EAR_LO[1:-1]], closed=False), stroke=shade("#8A9295", k), sw=0.14,
             so=0.45, cap="round")
    if not far:
        doc.begin_batch()
        for i in range(70):
            u = rng.uniform(0.0, 9.6)
            vmax = 2.0 * (1 - u / 13.0)
            v = rng.uniform(-vmax, -0.6)
            p = F(u, v)
            e = F(u + rng.uniform(0.45, 0.8), v + rng.uniform(-0.05, 0.08))
            col = "#FFFFFF" if rng.random() < 0.55 else "#AEB6B9"
            doc.stroke(lstroke(p, e), col, 0.05, 0.35)
        doc.flush()
    # la base de l'oreille, roulée en cornet : elle se fond dans la tête
    doc.path(cr([F(-0.9, -0.85), F(0.4, -1.1), F(1.1, 0.0), F(0.4, 1.0), F(-0.9, 0.85), F(-1.2, 0.0)]),
             fill=doc.lin(gid + "-base", F(-0.9, 0.0), F(1.4, 0.0), [(0, shade("#E9E8E3", k), 1.0),
                                                                     (1, shade("#E9E8E3", k), 0.0)]))
    doc.path(cr([F(1.0, 1.15), F(1.7, 0.6), F(2.4, 0.5)], closed=False), stroke=shade(EAR_IN_SH, k), sw=0.2,
             so=0.35, cap="round")

def ear_shadow():
    """L'ombre douce de l'oreille sur la nuque et le haut de la joue."""
    eb = HL(*EAR_L)
    F = ear_frame(eb, 9.0)
    pts = [F(-0.6, 0.7), F(2.0, 2.4), F(5.0, 3.2), F(7.4, 2.9), F(5.0, 1.6), F(2.0, 0.7)]
    geom = Polygon(dense(pts, True, 6)).buffer(0).intersection(HEAD_POLY.union(NECK_POLY))
    doc.path(geom_d(geom), fill=doc.lin("oreille-ombre", F(3.0, 0.8), F(3.0, 3.0),
                                        [(0, COLD_SH, 0.35), (1, COLD_SH, 0.0)]))

def ear_far():
    doc.open("ear-far")
    eb = HL(*EAR_FAR_L)
    F = ear_frame(eb, 2.0)
    ear_draw(lambda u, v: F(u * 0.6, v * 0.6), "oreille-loin", far=True)
    doc.pivot("ear-far", eb)
    doc.close()

def ear_near():
    doc.open("ear-near")
    eb = HL(*EAR_L)
    F = ear_frame(eb, 9.0)
    ear_draw(F, "oreille")
    doc.pivot("ear-near", eb)
    doc.close()

def jaw():
    """La mâchoire du bas : le menton, la lèvre inférieure ; cachés au repos,
    l'intérieur de la bouche, la langue, les incisives du bas, la joue qui
    s'étire."""
    doc.open("jaw")
    # la joue qui s'étire derrière la commissure
    doc.path(cr(HLs([(14.6, 5.9), (16.8, 5.1), (18.6, 5.0), (18.9, 6.1), (17.6, 7.6), (15.3, 8.4)])),
             fill=doc.lin("joue-etiree", HL(16.5, 5.0), HL(16.5, 8.4), [(0, "#D7D6D0"), (1, "#B4BCBD")]))
    # l'intérieur de la bouche et la langue
    doc.path(cr(HLs([(17.9, 5.3), (18.6, 4.3), (20.2, 3.75), (21.8, 3.6), (22.45, 4.1), (22.25, 4.75), (21.4, 4.95),
                     (20.3, 5.08), (19.1, 5.25)])), fill="#3E2428")
    doc.path(cr(HLs([(18.5, 5.1), (19.6, 4.67), (21.0, 4.45), (21.7, 4.55), (20.8, 4.8), (19.3, 5.05)])),
             fill="#B47A7E")
    doc.path(cr(HLs([(19.2, 4.85), (20.4, 4.6), (21.3, 4.58)]), closed=False), stroke="#D9A0A2", sw=0.12,
             so=0.7, cap="round")
    # les incisives du bas (en haut, le mouton n'a qu'un bourrelet)
    doc.path(cr(HLs([(21.2, 4.62), (21.95, 4.5), (22.1, 4.78), (21.35, 4.88)])), fill="#E9E3D5")
    # le menton, rond et profond (P15), et la lèvre inférieure
    chin = HLs([(17.7, 5.52), (19.1, 5.32), (20.5, 5.13), (21.7, 4.98), (22.6, 4.86), (22.85, 5.3), (22.75, 6.0),
                (22.4, 6.62), (21.85, 7.18), (21.05, 7.7), (19.85, 8.15), (18.4, 8.38), (16.3, 8.5), (14.6, 8.9),
                (13.2, 9.1), (13.2, 7.3), (15.3, 6.1)])
    doc.path(cr(chin), fill=doc.lin("menton", HL(19.5, 5.0), HL(19.5, 8.4),
                                    [(0, "#E4DED9"), (0.4, "#D6D3CD"), (1, "#A7B0B3")]))
    # la lèvre inférieure, rose gris, et le renflement du menton
    doc.path(cr(HLs([(17.8, 5.55), (19.6, 5.28), (21.2, 5.06), (22.55, 4.88), (22.8, 5.35), (21.9, 5.68),
                     (20.4, 5.84), (18.9, 5.88)])),
             fill=doc.lin("levre-bas", HL(20.0, 5.0), HL(20.0, 5.9), [(0, LIPS), (1, mix(LIPS, "#D3D1CB", 0.6))]))
    cp = Polygon(chin).buffer(0)
    glow("menton-bosse", HL(21.1, 6.7), 1.4, 1.0, "#F6F4F0", 0.6, angle=-TH, clip=cp)
    glow("menton-ombre", HL(19.6, 7.9), 2.6, 0.7, COLD_SH, 0.3, angle=-TH + 0.1, clip=cp)
    doc.begin_batch()
    for i in range(36):
        u = rng.uniform(15.0, 22.0)
        v = rng.uniform(6.2, 8.2)
        p = HL(u, v)
        if not HEAD_POLY.buffer(2.0).contains(Point(p)):
            continue
        e = HL(u - rng.uniform(0.4, 0.7), v + rng.uniform(0.0, 0.2))
        doc.stroke(lstroke(p, e), "#FFFFFF" if rng.random() < 0.5 else "#9FA8AB", 0.07, 0.45)
    doc.flush()
    doc.pivot("jaw", HL(*JAW_L))
    doc.close()

def closed_eye():
    """L'œil fermé, sous le groupe des yeux : on le voit quand il cligne."""
    lid = [EL(*p) for p in [(-1.42, 0.05), (-0.8, 0.12), (0.0, 0.16), (0.8, 0.14), (1.42, 0.16)]]
    doc.path(cr(ALMOND), fill=doc.lin("paupiere", EL(0, -0.8), EL(0, 0.7), [(0, "#ECE9E4"), (0.6, "#DAD6D1"),
                                                                             (1, "#C9C6C2")]))
    doc.path(cr(lid, closed=False), stroke="#7E6A68", sw=0.16, so=0.85, cap="round")
    for k in range(9):
        t = (k + 0.5) / 9
        p = EL(-1.2 + 2.4 * t, 0.14 + 0.03 * math.sin(math.pi * t))
        doc.path(qstroke(p, add(p, rot((0.2, -0.25), EYE_TILT)), add(p, rot((0.32, -0.45), EYE_TILT)), 2),
                 stroke=LASH_LT, sw=0.055, so=0.85, cap="round")

def eyes():
    """L'œil (§ 5) : iris ambre brun avec un anneau plus clair
    autour de la pupille en fente horizontale ; cils blancs, paupières sombres et fins cils blancs."""
    doc.open("eyes")
    # le globe, brun chaud (le blanc de l'œil d'un mouton ne se voit presque pas)
    doc.path(cr(ALMOND), fill=doc.rad("globe", EL(-0.2, -0.1), 1.7, [(0, "#7A6A54"), (0.7, "#5C4E3E"),
                                                                     (1, "#3E342A")]))
    doc.open("pupils")
    ic = IRIS_C
    doc.path(cr(ellipse(ic, IRIS_R, IRIS_R, 0, 18)),
             fill=doc.rad("iris", add(ic, (0.0, 0.05)), IRIS_R,
                          [(0, "#4E4434"), (0.22, IRIS_GOLD), (0.4, "#9C8364"), (0.66, "#7A6248"),
                           (0.86, "#65523D"), (1, IRIS_RING)], f=add(ic, (-0.12, 0.18))))
    for k in range(22):
        a0 = 2 * math.pi * k / 22 + 0.15
        doc.path(lstroke(add(ic, (0.56 * math.cos(a0), 0.56 * math.sin(a0))),
                         add(ic, (1.23 * math.cos(a0 + 0.08), 1.23 * math.sin(a0 + 0.08))), 2),
                 stroke="#7A6F57" if k % 2 else "#D2C8A6", sw=0.045, so=0.42, cap="round")
    pw, ph = 1.56, 0.43
    pc = add(ic, (0.0, 0.08))
    doc.path(runs_path([[add(pc, (-pw / 2 + ph / 2, ph / 2)), add(pc, (pw / 2 - ph / 2, ph / 2))],
                        [add(pc, (pw / 2 - ph / 2, ph / 2)), add(pc, (pw / 2 - 0.02, 0.0)),
                         add(pc, (pw / 2 - ph / 2, -ph / 2))],
                        [add(pc, (pw / 2 - ph / 2, -ph / 2)), add(pc, (-pw / 2 + ph / 2, -ph / 2))],
                        [add(pc, (-pw / 2 + ph / 2, -ph / 2)), add(pc, (-pw / 2 + 0.02, 0.0)),
                         add(pc, (-pw / 2 + ph / 2, ph / 2))]]), fill=PUPIL)
    # les grains de l'iris (granula iridica), sur le bord du haut de la pupille
    for gx, gs in ((-0.34, 0.06), (-0.08, 0.08), (0.18, 0.075), (0.42, 0.05)):
        p = add(pc, (gx, ph / 2 - 0.02))
        doc.path(cr([add(p, (-gs, 0)), add(p, (0, gs * 0.9)), add(p, (gs, 0))]), fill="#2A231B")
    doc.pivot("pupils", ic)
    doc.close()
    # les reflets sur la cornée (la lumière d'en haut à gauche)
    doc.path(cr(ellipse(EL(-0.62, -0.34), 0.2, 0.13, EYE_TILT, 10)), fill="#FFFFFF", fo=0.8)
    doc.path(cr(ellipse(EL(0.5, 0.3), 0.1, 0.07, EYE_TILT, 8)), fill="#FFFFFF", fo=0.4)
    # le bord de la paupière du haut et ses longs cils blancs, qui pendent
    # un peu sur l'œil (P4, P15) : ils clignent avec lui
    up = [EL(*p) for p in ALMOND_L[:7]]
    doc.path(cr(up, closed=False), stroke="#302C27", sw=0.16, so=0.8, cap="round")
    # une frange serrée (P15) : l'ombre qu'elle jette sur l'œil, puis les cils,
    # fins et inégaux, quelques-uns plus gris derrière
    glow("cils-ombre", EL(0.1, -0.42), 1.25, 0.3, "#1C1612", 0.35, angle=EYE_TILT, clip=Polygon(ALMOND))
    lr = random.Random(77)
    n = 24
    for k in range(n):
        t = (k + lr.uniform(0.15, 0.85)) / n
        a = -1.2 + 2.45 * t
        b = -0.66 - 0.12 * math.sin(math.pi * t) + 0.22 * t * t
        L = 0.34 + 0.32 * math.sin(math.pi * (0.25 + 0.6 * t)) + lr.uniform(-0.1, 0.1)
        p = EL(a, b)
        c = EL(a + 0.22 * L, b - 0.18 * L)
        tip = EL(a + 0.42 * L, b + 0.62 * L)
        col, w = ((LASH_LT, 0.042) if k % 3 else (LASH, 0.046)) if k % 5 else ("#B8B2A4", 0.04)
        doc.path(qstroke(p, c, tip, 2), stroke=col, sw=w, so=0.85, cap="round")
    doc.pivot("eyes", E)
    doc.close()

def cover():
    """Le pourtour de l'œil, immobile, peint du dégradé même de la tête : il
    cache l'iris quand le regard le pousse hors de l'amande."""
    outer = Polygon(ellipse(add(E, (-0.4, 0.0)), 4.4, 3.6, EYE_TILT, 28)).intersection(HEAD_POLY.buffer(-0.1))
    hole = Polygon(ALMOND).buffer(0)
    ring = outer.difference(hole)
    doc.path(geom_d(ring, simplify=0.01), fill="url(#tete)", rule="evenodd")

def face():
    """Le modelé et le poil de la face, le mufle, le naseau, la bouche, et
    l'habillage de l'œil (paupières roses, arcade, pli, larmier)."""
    hd = runs_path([HLs(DORSAL), HLs(MOUTH), HLs(NOTCH), HLs(UNDER)])
    eye_keep = Polygon(ALMOND).buffer(0.25)
    # --- le modelé (la lumière d'en haut à gauche) : front et chanfrein
    # éclairés, joue modelée par le masséter, l'arête de la face, l'ombre
    # froide sous la mâchoire (le reflet argenté)
    HP = HEAD_POLY
    glow("f-front", HL(4.8, -2.0), 4.6, 1.7, "#FFFFFF", 0.6, angle=hang(1.0, 0.0), clip=HP)
    # la bosse du nez, devant les yeux : elle prend la lumière ; l'ombre
    # douce sous elle, sur le côté du chanfrein
    glow("f-bosse", HL(12.0, -2.2), 4.4, 1.15, "#FFFFFF", 0.75, angle=hang(1.0, 0.2), clip=HP)
    glow("f-chanfrein", HL(17.6, -0.6), 3.4, 0.9, CHANF_LT, 0.55, angle=hang(1.0, 0.36), clip=HP)
    glow("f-chanfrein-ombre", HL(14.0, 0.4), 5.4, 1.0, COLD_SH, 0.2, angle=hang(1.0, 0.25), clip=HP)
    glow("f-joue", HL(8.8, 5.9), 5.0, 3.0, "#FFFFFF", 0.4, angle=hang(1.0, 0.1), clip=HP)
    soft(hd, [(0, COLD, 0.0), (0.5, COLD, 0.0), (1, COLD, 0.5)], HL(9.0, 2.6), HL(9.0, 9.6), "f-froid")
    # l'arête de la joue (la crête faciale) : une arête claire sous l'œil, qui
    # file vers le museau, l'ombre de la joue dessous
    groove("f-crete", HLs([(12.4, 3.2), (10.2, 3.0), (8.0, 3.0), (5.8, 3.3), (4.0, 3.75)]), 0.9, "#FFFFFF", 0.3, HP)
    groove("f-crete-ombre", HLs([(12.4, 3.9), (10.2, 3.75), (8.0, 3.8), (5.8, 4.15), (4.2, 4.6)]), 1.2, COLD_SH,
           0.32, HP)
    # le bord arrière de la ganache, à peine (une ombre douce, sans trait)
    doc.path(cr(band(HLs([(3.4, 9.0), (1.9, 8.1), (0.9, 6.5), (0.4, 4.4)]), 1.3, 0.7)),
             fill=doc.lin("f-ganache", HL(3.4, 9.0), HL(0.4, 4.2), [(0, COLD_SH, 0.32), (1, COLD_SH, 0.0)]))
    # l'arcade au-dessus de l'œil (l'os, éclairé) et le creux sous elle
    glow("f-arcade", EL(-0.1, -1.6), 2.0, 0.6, "#FFFFFF", 0.75, angle=EYE_TILT, clip=HP)
    glow("f-orbite", EL(0.1, -1.0), 1.8, 0.45, COLD_SH, 0.3, angle=EYE_TILT, clip=HP)
    # le pli de peau qui descend en oblique derrière l'œil (P15)
    groove("f-pli", [EL(-1.6, 0.45), EL(-2.3, 1.05), EL(-2.9, 1.7)], 0.45, COLD_SH, 0.26, HP)
    doc.path(cr([EL(-1.6, 0.68), EL(-2.3, 1.28), EL(-2.8, 1.9)], closed=False), stroke="#FFFFFF", sw=0.14, so=0.35,
             cap="round")
    # le creux devant l'œil (la fosse du larmier) et sa fente sombre, qui
    # descend en oblique vers le museau (P15)
    glow("f-creux-oeil", EL(2.7, 1.25), 1.7, 1.0, COLD_SH, 0.4, angle=EYE_TILT - 0.7, clip=HP)
    doc.path(cr([EL(1.7, 0.55), EL(2.2, 1.05), EL(2.6, 1.7), EL(2.85, 2.25)], closed=False), stroke="#7A7472",
             sw=0.18, so=0.55, cap="round")
    doc.path(cr([EL(1.95, 0.45), EL(2.45, 0.95), EL(2.85, 1.6)], closed=False), stroke="#FFFFFF", sw=0.12, so=0.4,
             cap="round")
    # --- le poil ras de la face, couché vers l'arrière ; rangé à part autour
    # de la lèvre (que le moteur détache)
    lip_area = Polygon(LIP_POLY).buffer(0)
    region = HEAD_POLY.buffer(-0.2).difference(eye_keep)

    def flow(p):
        q = sub(p, POLL)
        u = q[0] * HU[0] + q[1] * HU[1]
        v = q[0] * HV[0] + q[1] * HV[1]
        dl = (-1.0, 0.12 + 0.05 * v)
        return norm(add(mul(HU, dl[0]), mul(HV, dl[1])))

    def style(p):
        q = sub(p, POLL)
        v = q[0] * HV[0] + q[1] * HV[1]
        r = rng.random()
        if v < 1.5:
            return ("#FFFFFF", 0.055, 0.32) if r < 0.55 else ("#BFC6C9", 0.05, 0.2)
        if v < 5.0:
            return ("#FFFFFF", 0.055, 0.26) if r < 0.4 else ("#A7B1B6", 0.05, 0.2)
        return ("#F2F2EE", 0.055, 0.2) if r < 0.3 else ("#8F9AA0", 0.05, 0.22)
    doc.begin_batch()
    strokes_in(region.difference(lip_area.buffer(0.4)), 0.36, flow, (0.3, 0.6), style, rng, curl=0.1)
    doc.flush()
    doc.begin_batch()
    strokes_in(lip_area.buffer(-0.35), 0.4, flow, (0.35, 0.6), style, rng, curl=0.1)
    doc.flush()
    # --- le mufle : gros, haut et charnu, rose gris, nu, finement grenu
    # (P15) ; le grand naseau en virgule qui remonte vers l'arrière
    nose = HLs([(19.3, 0.5), (20.5, -0.05), (21.6, 0.6), (22.4, 1.35), (22.92, 2.2), (23.06, 3.0), (22.98, 3.85),
                (22.66, 4.55), (21.6, 4.7), (20.4, 4.45), (19.5, 3.55), (19.0, 2.0)])
    nose_poly = Polygon(nose).buffer(0)
    doc.path(cr(nose), fill=doc.rad("mufle", HL(21.9, 2.3), 2.7, [(0, SKIN_LT, 0.7), (0.5, SKIN, 0.9),
                                                                  (0.8, SKIN, 0.55), (1, SKIN, 0.0)]))
    # le dessous du mufle à l'ombre, rose gris froid (P15 : #928E96)
    doc.path(cr(nose), fill=doc.lin("mufle-ombre", HL(21.0, 2.6), HL(21.0, 4.7),
                                    [(0, "#928E96", 0.0), (1, "#928E96", 0.5)]))
    glow("mufle-lumiere", HL(21.4, 0.9), 1.3, 0.55, "#C7C8BF", 0.7, angle=hang(1.0, 0.6), clip=nose_poly)
    nostril = HLs([(20.1, 2.05), (20.8, 2.1), (21.5, 2.2), (22.1, 2.36), (22.6, 2.56), (22.86, 2.86), (22.78, 3.2),
                   (22.46, 3.3), (22.0, 3.12), (21.4, 2.78), (20.8, 2.46), (20.25, 2.2)])
    doc.path(cr(nostril), fill=doc.lin("naseau", HL(22.7, 2.9), HL(20.1, 2.1), [(0, "#120D0F"), (0.45, NOSTRIL),
                                                                            (1, "#4D4945")]))
    doc.path(cr(HLs([(20.6, 2.36), (21.3, 2.6), (21.95, 2.92), (22.4, 3.12)]), closed=False), stroke="#B9B9B1",
             sw=0.1, so=0.6, cap="round")
    # l'aile du naseau, au-dessus, et le sillon qui fend la lèvre (le philtrum)
    doc.path(cr(HLs([(19.9, 1.72), (20.8, 1.7), (21.7, 1.85), (22.45, 2.2)]), closed=False), stroke=SKIN_SH,
             sw=0.14, so=0.5, cap="round")
    doc.path(cr(HLs([(22.95, 3.4), (22.88, 4.08), (22.72, 4.7)]), closed=False), stroke=SKIN_SH, sw=0.14, so=0.6,
             cap="round")
    # quelques points sombres (les follicules des vibrisses) sur le côté du museau
    for k in range(16):
        p = HL(rng.uniform(17.4, 21.2), rng.uniform(2.6, 4.6))
        doc.path(cr(ellipse(p, 0.05, 0.05, 0, 6)), fill="#8E8C8C", fo=0.6)
    # la bouche : la ligne des lèvres, du bout du nez à la commissure
    doc.path(cr(HLs(MOUTH), closed=False), stroke=LIPS_SH, sw=0.13, so=0.75, cap="round")
    doc.path(cr(HLs([(17.8, 5.5), (17.45, 5.38), (17.2, 5.18)]), closed=False), stroke=LIPS_SH, sw=0.13, so=0.4,
             cap="round")
    doc.path(cr(HLs([(22.5, 4.62), (21.4, 4.8), (20.1, 4.98)]), closed=False), stroke=LIPS, sw=0.32, so=0.7,
             cap="round")
    # --- l'habillage de l'œil, immobile, autour du trou du pourtour : les
    # paupières bordées de rose, le pli de la paupière, la caroncule
    rim_out = [EL(a * 1.1, b * 1.2 + (0.04 if b > 0 else -0.03)) for a, b in ALMOND_L]
    doc.path(cr(rim_out) + cr(ALMOND[::-1]), fill=doc.lin("paupieres-roses", EL(0, -0.95), EL(0, 0.8),
                                                          [(0, "#7C7770"), (0.45, "#3E3933"), (1, "#211F1C")]), rule="evenodd")
    doc.path(cr([EL(-1.15, -0.85), EL(-0.4, -1.18), EL(0.5, -1.16), EL(1.15, -0.72)], closed=False), stroke=COLD_SH,
             sw=0.12, so=0.3, cap="round")
    doc.path(cr(ellipse(EL(1.26, 0.17), 0.2, 0.13, EYE_TILT, 8)), fill="#79625C", fo=0.9)
    # le contour de la tête, discret ; ni le long de l'encoche de la mâchoire,
    # ni sur la nuque (l'encolure continue la tête)
    doc.path(cr(HLs(DORSAL), closed=False), stroke="#848C90", sw=0.26, so=0.5, cap="round")
    doc.path(cr(HLs(UNDER[:7]), closed=False), stroke="#848C90", sw=0.24, so=0.45, cap="round")

LIP_L = [(19.1, 5.4), (19.05, 4.6), (19.1, 3.85), (19.9, 3.45), (20.9, 3.32), (21.8, 3.38), (22.55, 3.3),
         (23.06, 3.5), (23.02, 4.2), (22.8, 4.78), (22.5, 4.92), (21.7, 5.06), (20.5, 5.21)]

LIP_POLY = HLs(LIP_L)

def gum():
    """Le bourrelet dentaire (le mouton n'a pas d'incisives en haut), rose,
    caché sous la lèvre tant qu'elle est baissée : bien au-dessus de la ligne
    de la bouche (articulations.gum), et le fond sombre de la lèvre."""
    pad = HLs([(19.4, 4.85), (19.55, 4.25), (20.05, 3.95), (20.9, 3.78), (22.0, 3.74), (22.65, 3.92), (22.6, 4.32),
               (22.15, 4.58), (21.1, 4.72), (20.05, 4.84)])
    ridge = HLs([(19.75, 4.62), (20.6, 4.5), (21.6, 4.4), (22.25, 4.22)])
    out = A.gum([S(p) for p in pad], [S(p) for p in ridge], GUM, GUM_SH, 0.22)
    dark = HLs([(19.55, 4.2), (20.2, 4.02), (21.3, 3.95), (22.35, 4.02)])
    el = ET.fromstring(element(cr(dark, closed=False), stroke="#7D4550", sw=0.18, so=0.7, cap="round")
                       .replace("<path ", f'<path xmlns="{A.SVG_NS}" ', 1))
    el.tag = A.Q + "path"
    return out + [el]


# ------------------------------------------------------------------ laine rêche et jarre blanc
def fleece_at(p):
    """Même couleur à la même place sur le corps et l'encolure au repos."""
    x, h = p
    c = ramp(FLEECE_STOPS, clamp((77 - h) / 49))
    black = smooth(59, 99, x) * .86
    c = mix(c, "#17191D", black)
    # La culotte garde davantage de jarre clair, sans devenir une tache.
    c = mix(c, "#A0A2A0", .24 * (1 - smooth(4, 24, x)) * smooth(31, 55, h))
    return c


def fibre_flow(p):
    x, h = p
    dx = -.22 - .32 * (1 - smooth(5, 35, x)) + .34 * smooth(58, 95, x)
    return norm((dx, -1))


def fleece_background(shape, gid):
    d = geom_d(shape, simplify=.015)
    doc.path(d, fill="url(#laine)")
    if not VARIANTE:
        doc.path(d, fill="url(#criniere)")
    glow(gid + "-culotte", (7, 49), 16, 20, "#B4B6B1", .18, clip=shape)
    glow(gid + "-lombes", (40, 62), 31, 15, "#C0C3C8", .14, clip=shape)
    glow(gid + "-ventre", (53, 30), 41, 10, "#11151C", .26, clip=shape)


def fleece_texture(shape, seed, mane=False):
    """Des petits faisceaux pointus, puis leurs fibres : jamais des bouclettes.

    Les faisceaux donnent le relief de loin ; le mélange irrégulier de
    fibres sombres et de jarre clair fait le chiné à courte distance.
    Le point de sortie est vérifié : aucune découpe SVG n'est nécessaire.
    """
    rr = random.Random(seed)
    region = shape.buffer(-.25)
    pts = poisson(region, .98 if not mane else .9, rr)
    doc.begin_batch()
    for p in pts:
        flow = rot(fibre_flow(p), rr.uniform(-.55, .55))
        side = (-flow[1], flow[0])
        L = rr.uniform(.8, 2.2) * (1.5 if mane else 1)
        e = add(p, mul(flow, L))
        if not region.contains(Point(e)):
            continue
        # Des creux courts entre petits paquets de laine, irréguliers et
        # mats ; aucune forme fermée en écaille ni rangée de boucles.
        bend = rr.uniform(-.28, .28)
        c = add(lerp(p, e, .5), mul(side, bend))
        doc.stroke(qstroke(p, c, e, 2), "#151A22", .12 if rr.random() < .65 else .18, .3)
        if rr.random() < .8:
            p2 = add(p, mul(side, rr.uniform(.18, .35)))
            e2 = add(e, mul(side, rr.uniform(.05, .18)))
            doc.stroke(qstroke(p2, add(c, mul(side, .2)), e2, 2), "#A4AAB5", .11, .24)
        if rr.random() < .38:
            p3 = add(p, mul(side, -.22))
            e3 = add(lerp(p, e, .8), mul(side, -.1))
            doc.stroke(qstroke(p3, add(c, mul(side, -.18)), e3, 2), "#D4D6D4", .065, .4)
    doc.flush()
    doc.begin_batch()
    # Les petits poils noirs et les fibres blanches se mêlent intimement.
    for p in poisson(region, .39 if not mane else .35, rr):
        d = rot(fibre_flow(p), rr.uniform(-.42, .42))
        L = rr.uniform(.55, 1.2) * (2.1 if mane else 1)
        e = add(p, mul(d, L))
        if not shape.contains(Point(e)):
            continue
        n = (-d[1], d[0])
        bend = rr.uniform(-.22, .22) * L
        c = add(lerp(p, e, .55), mul(n, bend))
        r = rr.random()
        if r < .2:
            col, sw, so = "#C7CBD0", .065, .3
        elif r < .38:
            col, sw, so = "#E1E2DE", .055, .42 if mane else .3
        elif r < .78:
            col, sw, so = "#1C2026", .075, .34
        else:
            col, sw, so = "#0D1015", .065, .38
        doc.stroke(qstroke(p, c, e, 2), col, sw, so)
    doc.flush()


def fringe(line, shape, seed, length=1.0):
    """Le bord cassé par le jarre, plus long et hérissé sur le col."""
    rr = random.Random(seed)
    line = LineString(dense(line, False, 12))
    doc.begin_batch()
    for k in range(int(line.length / .28)):
        t = (k + rr.random()) * .28
        p = tuple(line.interpolate(t).coords[0])
        a = tuple(line.interpolate(max(0, t - .1)).coords[0])
        b = tuple(line.interpolate(min(line.length, t + .1)).coords[0])
        n = norm((-(b[1] - a[1]), b[0] - a[0]))
        if shape.contains(Point(add(p, mul(n, .25)))):
            n = mul(n, -1)
        d = rot(n, rr.uniform(-.7, .7))
        p = add(p, mul(n, -.35))
        e = add(p, mul(d, rr.uniform(.35, 1.2) * length))
        c = add(lerp(p, e, .5), mul(n, rr.uniform(-.1, .1)))
        r = rr.random()
        col = mix(fleece_at(p), "#DEE1E1", .42) if r < .55 else shade(fleece_at(p), .65)
        doc.stroke(qstroke(p, c, e, 2), col, .065 if r < .55 else .09, .85)
    doc.flush()


def neck():
    doc.open("neck")
    fleece_background(NECK_POLY, "cou")
    fleece_texture(NECK_POLY, 222, mane=True)
    fringe(CREST, NECK_POLY, 717, 1.1)
    fringe(THROAT, NECK_POLY, 718, 1.4)
    doc.pivot("neck", NECK_PIVOT)
    doc.close()


def body():
    doc.open("body")
    # Trois petites pointes sous le ventre suivent la laine en mouvement.
    for i, x in enumerate((38, 54, 69), 1):
        h = 28.2 if i != 1 else 29.0
        doc.open(f"sway-{i}")
        p = [(x - 4, h + 2), (x + 4, h + 2), (x + 2.4, h - .2),
             (x + .5, h - 1.2), (x - 1.8, h - .5)]
        region = Polygon(dense(p)).buffer(0)
        fleece_background(region, f"meche-{i}")
        fleece_texture(region, 500 + i)
        doc.pivot(f"sway-{i}", (x, h + 1.9))
        doc.close()
    fleece_background(BODY_POLY, "tronc")
    fleece_texture(BODY_POLY, 1916)
    fringe(BACK, BODY_POLY, 21, .55)
    fringe(BELLY[:-1], BODY_POLY, 22, 1.1)
    # La laine s'échancre derrière le coude : le blanc appartient à la
    # patte de devant, qui l'emporte avec elle dans les poses.
    doc.close()


# ------------------------------------------------------------------ membres solides et pieds clairs
LEGS = [("leg-back-far", "back", 15.0, True), ("leg-front-far", "front", 77.0, True),
        ("leg-back-near", "back", 7.0, False), ("leg-front-near", "front", 85.0, False)]


def leg(name, kind, x, far):
    front = kind == "front"
    o = lambda a, h: (x + a, h)
    if front:
        top = [(-4, 45), (3, 45), (3.2, 36), (2.8, 28), (2.2, 21),
               (2.6, 18), (2.4, 15.2), (1.9, 12), (1.8, 7), (2.2, 4.7), (3.5, 2.8)]
        back = [(-2.6, 4.0), (-3.3, 4.6), (-2.6, 7), (-2.5, 12), (-2.9, 15.5),
                (-3.2, 17.7), (-2.9, 20.2), (-3.4, 27), (-5.3, 34), (-5.7, 39)]
        pivot = (x - .5, 44)
    else:
        top = [(0, 49), (10, 48), (13, 42), (11, 36), (7.5, 30), (3.2, 25),
               (2.1, 22), (1.4, 18), (1.1, 9), (1.2, 6), (2, 4), (3.5, 2.8)]
        back = [(-2.8, 4.0), (-3.5, 4.7), (-2.7, 7), (-2.8, 14), (-3, 19),
                (-4.2, 22), (-4.4, 24), (-3.2, 28), (-2.2, 33), (-1.9, 39), (-1.1, 45)]
        pivot = (x + 5.5, 47)
    sole = [(3.5, 2.8), (3.9, .4), (3.8, 0), (-2.8, 0), (-3.05, .6), (-2.6, 4.0)]
    outline = [o(*p) for p in top + sole[1:] + back]
    lp = Polygon(dense(outline, True, 6)).buffer(0)
    k = .80 if far else 1
    colors = [shade(c, k) for c in ("#9EA8AA", "#F0F1E8", "#DCDDD3", "#B4B9B4", "#949F9F")]
    doc.open(name)
    d = cr(outline)
    doc.path(d, fill=doc.lin(name + "-blanc", o(-4, 18), o(4, 18),
                            list(zip((0, .22, .5, .8, 1), colors))))
    soft(d, [(0, "#434A4E", .6), (1, "#434A4E", 0)], o(0, 35), o(0, 25), name + "-haut")
    soft(d, [(0, "#B1A994", 0), (1, "#A69A80", .3)], o(0, 8), o(0, 2.5), name + "-terre")
    jh = H_CARPE if front else H_JARRET
    glow(name + "-joint", o(.2, jh + .5), 2.4, 2.6, shade("#F8F8F0", k), .28, clip=lp)
    for xx in (-2.25, 1.6):
        doc.path(cr([o(xx, jh - 2), o(xx + .15, 12), o(xx, 7.3)], False),
                 stroke=shade("#8D999A" if xx < 0 else "#FFFFFF", k), sw=.21, so=.5, cap="round")
    doc.begin_batch()
    rr = random.Random(409 + int(x))
    # Pas de tracé commun de part et d'autre du futur genou.
    for hi, lo in ((44, jh + 2.5), (jh - 2.5, 3.3)):
        patch = lp.intersection(Polygon([(-20, lo), (150, lo), (150, hi), (-20, hi)])).buffer(-.15)
        strokes_in(patch, .32, lambda p: norm((-.18 if not front and p[1] > jh else .02, -1)),
                   (.45, .85), lambda p: (shade("#FFFFFF" if rr.random() < .65 else "#899697", k), .055, .45), rr,
                   curl=.1)
        doc.flush()
        doc.begin_batch()
    doc.flush()
    hoof = [o(*p) for p in sole]
    doc.path(runs_path([hoof[:3], [hoof[2], hoof[3]], [hoof[3], hoof[4], hoof[5], hoof[0]]]),
             fill=doc.lin(name + "-onglon", o(-3, 1), o(5, 1),
                                  [(0, shade("#827966", k)), (.3, shade("#DED6C0", k)),
                                   (.68, shade("#ECE4D0", k)), (1, shade("#B1A68D", k))]))
    doc.path(cr([o(-3, .25), o(1, .18), o(4.8, .2)], False), stroke="#756F61", sw=.23, so=.7)
    doc.path(cr([o(1.4, .15), o(1.1, 1.1), o(.7, 2.5)], False), stroke=shade("#736C5D", k), sw=.16, so=.8)
    doc.path(cr([o(-2.8, 2.7), o(.4, 2.8), o(3.5, 2.65)], False), stroke=shade("#928C7D", k), sw=.21, so=.6)
    for j in range(4):
        yy = .6 + j * .4
        doc.path(cr([o(1.7, yy), o(2.8, yy + .1), o(4.1 - yy * .4, yy)], False),
                 stroke=shade("#A59B85", k), sw=.055, so=.45)
    doc.path(cr([o(-3.2, 4), o(-4, 4.8), o(-3.6, 5.2), o(-2.9, 4.8)]), fill=shade("#B0A68E", k))
    doc.pivot(name, pivot)
    doc.close()


# ------------------------------------------------------------------ queue naturelle, en chaîne
def tail():
    # L'attache est cachée par la fesse ; 32 cm jusqu'à l'extrémité.
    pieces = [[ (1, 63), (-2, 63), (-5, 58), (-6, 51), (-5.8, 46), (-2, 46), (-1.8, 52), (0, 57)],
              [(-5.8, 50), (-6.4, 44), (-6.2, 36), (-2.6, 36), (-2.5, 43), (-2, 50)],
              [(-6.2, 39), (-6.4, 33), (-5.2, 28.5), (-3.5, 28), (-2.6, 31), (-2.6, 38)]]
    for i, pts in enumerate(pieces):
        gid = ("tail", "tail-mid", "tail-tip")[i]
        doc.open(gid)
        shape = Polygon(dense(pts)).buffer(0)
        fleece_background(shape, gid)
        fleece_texture(shape, 900 + i)
        fringe(pts, shape, 111 + i, .8)
        doc.pivot(gid, ((-1, 59), (-5, 48), (-5, 37))[i])
    for _ in pieces:
        doc.close()


# ------------------------------------------------------------------ cornes rondes, compactes
def horn(far=False):
    """Un ruban tubulaire en hélice, du crâne vers l'arrière puis vers l'avant.

    À la base : sept centimètres ; au bout : quelques millimètres. Les
    stries fines changent d'espacement et disparaissent vers la pointe.
    """
    pts = [(109.0, 84.0), (101.0, 87.4), (93.6, 85.0), (91.0, 80.6),
           (91.1, 75.2), (94.4, 71.2), (99.4, 70.8), (103.1, 73.0),
           (104.8, 77.4), (103.4, 81.3), (100.6, 82.5), (98.5, 80.9)]
    if far:
        pts = [add(p, (1.0, .15)) for p in pts]
    center = dense(pts, False, 12)
    n = len(center)
    normals, widths = [], []
    for i, p in enumerate(center):
        t = norm(sub(center[min(n - 1, i + 1)], center[max(0, i - 1)]))
        normals.append((-t[1], t[0]))
        u = i / (n - 1)
        widths.append(6.8 * (1 - u) ** .82 + .09)
    def side(k):
        return [add(p, mul(q, k * w / 2)) for p, q, w in zip(center, normals, widths)]
    # La racine entre réellement sous le poil du crâne. La soustraction
    # géométrique ne peint aucun cache par-dessus la tête, donc ni disque
    # blanc, ni raccord droit, ni œil recouvert.
    buried = HEAD_POLY
    def horn_shape(a, b):
        left, right = side(a), side(b)
        root = Polygon(left[:25] + right[:25][::-1]).buffer(0).difference(buried)
        rest = Polygon(left[24:] + right[24:][::-1]).buffer(0)
        return geom_d(root.union(rest), simplify=.003)
    k = .75 if far else 1.0
    doc.path(horn_shape(1, -1), fill=shade("#B99F7D", k))
    # Bandes jointives : le tube a un bord sombre et un long reflet doux.
    # Les traits sont discrets : aucune grosse annelure de mouflon.
    colors = [(0, "#6E5841"), (.15, "#AA8C67"), (.36, "#E2D1AF"),
              (.53, "#DBC59F"), (.75, "#BDA27B"), (1, "#796046")]
    for j in range(28):
        a, b = 1 - 2 * j / 28, 1 - 2 * (j + 1) / 28
        doc.path(horn_shape(a, b), fill=shade(ramp(colors, (j + .5) / 28), k))
    doc.begin_batch()
    rr = random.Random(881 if far else 882)
    for i in range(2, n - 10, 3):
        u = i / (n - 1)
        if u > .85:
            continue
        p, nn, w = center[i], normals[i], widths[i]
        if i < 25:
            continue
        t = norm(sub(center[min(n - 1, i + 1)], center[max(0, i - 1)]))
        a = add(p, mul(nn, w * .46))
        b = add(p, mul(nn, -w * .45))
        c = add(p, mul(t, rr.uniform(-.38, -.1)))
        doc.stroke(qstroke(a, c, b, 2), shade("#594735", k), rr.uniform(.035, .075), rr.uniform(.18, .38))
        if i % 9 == 0:
            p = add(p, mul(t, -.18))
            doc.stroke(qstroke(add(p, mul(nn, w * .44)), add(p, mul(t, -.3)), add(p, mul(nn, -w * .42)), 2),
                       shade("#F0E2C8", k), .04, .4)
    for f in (-.62, -.18, .3, .68):
        strand = [add(p, mul(nn, f * w / 2)) for p, nn, w in zip(center, normals, widths)][25:]
        doc.stroke(poly(strand, False), shade("#F0DDAD" if f > 0 else "#705943", k), .04, .2)
    doc.flush()
    # La pointe claire, à l'écart de l'œil, n'est ni noire ni acérée.
    if not far:
        st = int(.87 * n)
        q = side(.6)[st:] + side(-.6)[st:][::-1]
        doc.path(poly(q), fill="#E3D1AE", fo=.45)


def head():
    doc.open("head")
    ear_far()
    jaw()
    doc.path(runs_path([HLs(DORSAL), HLs(MOUTH), HLs(NOTCH), HLs(UNDER)]), fill="url(#tete)")
    closed_eye()
    eyes()
    cover()
    face()
    ear_shadow()
    ear_near()
    if not VARIANTE:
        horn()
    # Quelques poils raides au sommet du crâne : pas un toupet de laine.
    doc.begin_batch()
    for i in range(65):
        u = rng.uniform(-1, 7)
        p = HL(u, -3.5 - .6 * math.sin(max(0, u) / 7 * math.pi))
        if not HEAD_POLY.buffer(.2).contains(Point(p)):
            continue
        doc.stroke(lstroke(p, add(p, (-.2, rng.uniform(.25, .65))), 2), "#F8F9F1", .055, .8)
    doc.flush()
    doc.pivot("head", POLL)
    doc.pivot("muzzle", HL(*MUZZLE_L))
    doc.close()


def articulate(svg):
    parser = ET.XMLParser(target=ET.TreeBuilder(insert_comments=True))
    root = ET.fromstring(svg, parser=parser)
    paints = A.Paints(root)
    for name, kind, x, far in LEGS:
        jh = H_CARPE if kind == "front" else H_JARRET
        joint, half = A.leg_joint(svg, name, S((0, jh))[1])
        A.split_leg(root, paints, name, joint, half + .4)
    A.lift_part(root, paints, "head", [S(p) for p in LIP_POLY], "lip", S(HL(*LIP_PIVOT_L)), under=gum())
    ET.indent(root, space="  ")
    return ET.tostring(root, encoding="unicode") + "\n"


def main(path):
    doc.lin("laine", (0, 77), (0, 28), FLEECE_STOPS)
    doc.lin("criniere", (59, 0), (99, 0), [(0, "#15171B", 0), (.5, "#15171B", .35), (1, "#15171B", .86)])
    doc.lin("tete", HL(8, -4.2), HL(8, 10.2), HEAD_STOPS)
    tail()
    for args in LEGS:
        leg(*args)
    neck()
    body()
    head()
    doc.pivot("ground", (47, 0))
    comment = ("  Herdwick : bélier de trois ans et demi, fin octobre, trois mois après la tonte.\n"
               "  Gris naturel sans marques ; tête et membres blancs givrés, crinière sombre.\n"
               "  1 unité = 1 cm ; 70 cm au garrot à la peau. Généré par\n"
               "  tools/races/herdwick/gen_herdwick.py d'après docs/races/herdwick.md.\n"
               "  © 2026 Alejandro San Martin Lamas — MIT OR Apache-2.0")
    svg = articulate(doc.render(140, 104, comment))
    if VARIANTE:
        return variante(path, svg)
    with open(path, "w", encoding="utf-8") as f:
        f.write(svg)
    print(f"{path} : {len(svg) / 1024:.0f} Ko, {svg.count('<path')} tracés")


def variante(path, svg):
    """Les autres dessins : ce qui change, puis les proportions."""
    root = ET.fromstring(svg)
    if VARIANTE == "brebis":
        variantes.recolorer(root, variantes.eclaircir(.12), dans=("body", "neck", "tail"))
    if VARIANTE == "agneau":
        # Né noir, tête et pattes comprises ; il blanchit de la tête dans
        # son premier été (§ 4). Seul le bout des oreilles est déjà givré.
        noir = variantes.rampe("#0E0F12", "#3A3B40", bas=.1, haut=.9)
        variantes.recolorer(root, noir, sauf=("eyes", "ear-near", "ear-far"))
        variantes.recolorer(root, variantes.teinte("#17181C", .6), dans=("ear-near", "ear-far"))
    variantes.ecrire(path, root, **PROPORTIONS[VARIANTE])


if __name__ == "__main__":
    main(OUT)
