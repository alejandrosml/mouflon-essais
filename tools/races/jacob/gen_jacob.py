#!/usr/bin/env python3
# Générateur du dessin du mouton de Jacob (bélier à quatre cornes), vu de profil,
# tourné vers la droite. Unités : centimètres de l'animal réel ; sol à y = 200.
import math
import os
import random
import sys

sys.path.insert(0, os.path.join(os.path.dirname(os.path.abspath(__file__)), ".."))
import articulations  # noqa: E402  (tools/races/articulations.py)
import variantes  # noqa: E402
import xml.etree.ElementTree as ET  # noqa: E402

# Les autres dessins de la race (tools/races/variantes.py), d'après la fiche
# (§ 2.1, 6.10). Sans variante : le bélier à quatre cornes.
VARIANTE = sys.argv[2] if len(sys.argv) > 2 else ""
PROPORTIONS = {
    "brebis": dict(  # Brebis de Jacob à quatre cornes : 67 cm au garrot, cornes fines et courtes, tête et encolure plus fines, petite mamelle.
        garrot=(75, 67), encolure=(1.0, .86), tete=(.96, .93)),
    "agneau": dict(  # Agneau de Jacob de deux semaines : 40 cm au garrot, déjà pie, sans cornes (de simples bourgeons).
        variantes.AGNEAU, garrot=(75, 40), encolure=(1.1, .85), pattes=(1.1, 1.09)),
}

sys.path.insert(0, __file__.rsplit("/", 1)[0])
from geo import *  # noqa
from shapely.geometry import Polygon, LineString, Point
from shapely.ops import unary_union
from shapely import affinity

OUT = sys.argv[1] if len(sys.argv) > 1 else "jacob.svg"
GY = 200.0

# --- Palette (d'après la fiche de la race, section 4.5)
WOOL_LIT, WOOL_MID, WOOL_SHADOW = "#EDE9DD", "#D8D1C2", "#B2A994"
HAIR_WHITE = "#F3F3EE"
BLACK_TRUE, BLACK_WOOL, BLACK_SHEEN = "#181617", "#262326", "#3C3843"
BLACK_CHOC = "#4E3A2C"
SKIN_BLACK = "#252223"
HORN_DARK, HORN_MID, HORN_RIDGE, HORN_SPEC = "#161211", "#3D352F", "#7A736C", "#BDBBC2"
HOOF = "#1E1B19"
IRIS, IRIS_DARK, PUPIL = "#A46B2C", "#6B4422", "#0C0A09"
EAR_IN = "#3A3333"


# =============================================================== SVG
class Doc:
    def __init__(self):
        self.defs = []
        self.body = []

    def _stops(self, stops):
        out = []
        for st in stops:
            o, c = st[0], st[1]
            a = st[2] if len(st) > 2 else 1.0
            out.append(f'<stop offset="{fm(o)}" stop-color="{c}"' + (f' stop-opacity="{fm(a)}"' if a < 1 else "") + "/>")
        return "".join(out)

    def lin(self, gid, x1, y1, x2, y2, stops):
        self.defs.append(f'<linearGradient id="{gid}" gradientUnits="userSpaceOnUse" x1="{fm(x1)}" y1="{fm(y1)}" '
                         f'x2="{fm(x2)}" y2="{fm(y2)}">{self._stops(stops)}</linearGradient>')
        return f"url(#{gid})"

    def rad(self, gid, cx, cy, r, stops, fx=None, fy=None):
        f = f' fx="{fm(fx)}" fy="{fm(fy)}"' if fx is not None else ""
        self.defs.append(f'<radialGradient id="{gid}" gradientUnits="userSpaceOnUse" cx="{fm(cx)}" cy="{fm(cy)}" '
                         f'r="{fm(r)}"{f}>{self._stops(stops)}</radialGradient>')
        return f"url(#{gid})"

    def add(self, s):
        self.body.append(s)

    def open(self, gid, comment=None):
        if comment:
            self.body.append(f"<!-- {comment} -->")
        self.body.append(f'<g id="{gid}">')

    def close(self):
        self.body.append("</g>")

    def path(self, d, fill="none", stroke=None, sw=None, op=None, sop=None, rule=None, cap="round", join="round"):
        if not d:
            return
        a = [f'<path d="{d}" fill="{fill}"']
        if op is not None and op < 1:
            a.append(f' fill-opacity="{fm(op)}"')
        if rule:
            a.append(f' fill-rule="{rule}"')
        if stroke:
            a.append(f' stroke="{stroke}" stroke-width="{fm(sw)}"')
            if sop is not None and sop < 1:
                a.append(f' stroke-opacity="{fm(sop)}"')
            a.append(f' stroke-linecap="{cap}" stroke-linejoin="{join}"')
        a.append("/>")
        self.body.append("".join(a))

    def pivot(self, name, p):
        self.body.append(f'<circle id="pivot-{name}" cx="{fm(p[0])}" cy="{fm(p[1])}" r="0.8"/>')


D = Doc()


# =============================================================== Marques
class Marks:
    """Petites marques groupées par teinte (un seul tracé par teinte)."""

    def __init__(self):
        self.groups = {}

    def add(self, key, d):
        self.groups.setdefault(key, []).append(d)

    def emit(self, doc):
        for (color, sw, op), ds in sorted(self.groups.items(), key=lambda kv: (kv[0][2], kv[0][0])):
            doc.path("".join(ds), stroke=color, sw=sw, sop=op)
        self.groups = {}


def rel(p0, p1, p2, p3):
    return (f"M{pt(p0)}c{pt((p1[0] - p0[0], p1[1] - p0[1]))} {pt((p2[0] - p0[0], p2[1] - p0[1]))} "
            f"{pt((p3[0] - p0[0], p3[1] - p0[1]))}")


def arc1(c, r, a0, a1):
    """Un arc de cercle en une seule courbe de Bézier (jusqu'à 120° environ)."""
    k = 4 / 3 * math.tan((a1 - a0) / 4) * r
    p0 = (c[0] + r * math.cos(a0), c[1] + r * math.sin(a0))
    p3 = (c[0] + r * math.cos(a1), c[1] + r * math.sin(a1))
    p1 = (p0[0] - k * math.sin(a0), p0[1] + k * math.cos(a0))
    p2 = (p3[0] + k * math.sin(a1), p3[1] - k * math.cos(a1))
    return rel(p0, p1, p2, p3)


def curl(c, r, a0, sweep):
    """Une boucle serrée (une courbe) : la frisure."""
    p0 = (c[0] + r * math.cos(a0), c[1] + r * math.sin(a0))
    a1 = a0 + sweep
    p3 = (c[0] + r * 0.8 * math.cos(a1), c[1] + r * 0.8 * math.sin(a1))
    k = 4 / 3 * math.tan(abs(sweep) / 4) * r * (1 if sweep > 0 else -1)
    p1 = (p0[0] - k * math.sin(a0), p0[1] + k * math.cos(a0))
    p2 = (p3[0] + k * 0.8 * math.sin(a1), p3[1] - k * 0.8 * math.cos(a1))
    return rel(p0, p1, p2, p3)


def wave(c, length, ang, amp):
    """Une petite onde en S : les vagues serrées de la frisure."""
    ca, sa = math.cos(ang), math.sin(ang)
    p0 = (c[0] - ca * length / 2, c[1] - sa * length / 2)
    p3 = (c[0] + ca * length / 2, c[1] + sa * length / 2)
    n = (-sa * amp, ca * amp)
    p1 = (p0[0] + ca * length / 3 + n[0], p0[1] + sa * length / 3 + n[1])
    p2 = (p0[0] + 2 * ca * length / 3 - n[0], p0[1] + 2 * sa * length / 3 - n[1])
    return rel(p0, p1, p2, p3)


def q(v, steps=3):
    return round(clamp(v) * steps) / steps


def squiggle(p, length, ang, bend, rng):
    """Un petit trait irrégulier (une seule courbe) : creux entre deux mèches."""
    ca, sa = math.cos(ang), math.sin(ang)
    p0 = (p[0] - ca * length / 2, p[1] - sa * length / 2)
    p3 = (p[0] + ca * length / 2, p[1] + sa * length / 2)
    n = (-sa, ca)
    b1 = bend * rng.uniform(0.4, 1.3)
    b2 = bend * rng.uniform(-0.4, 1.3)
    p1 = (p0[0] + ca * length / 3 + n[0] * b1, p0[1] + sa * length / 3 + n[1] * b1)
    p2 = (p0[0] + 2 * ca * length / 3 + n[0] * b2, p0[1] + 2 * sa * length / 3 + n[1] * b2)
    return rel(p0, p1, p2, p3)


def locks(m, region, kind, rng, light, r=1.45, scale=1.0, tip=None, crimp=None):
    """Toison de 4 à 6 cm : une surface dense de mèches serrées (« pop-corn »).
    Sous les mèches, des creux d'ombre irréguliers ; ailleurs, des pointes qui
    prennent la lumière (en haut à gauche) ; et la frisure, de fines vagues
    serrées, surtout sur l'encolure et l'épaule. Tout est tiré au hasard
    (forme, taille, sens, opacité) pour ne pas faire un motif."""
    if region.is_empty:
        return
    from shapely.prepared import prep
    inside = prep(region)
    for p in poisson(region, r * scale, rng):
        l = q(light(p))
        if kind == "white" and rng.random() < 0.6 * l:
            continue
        if not inside.contains(Point(p).buffer(0.9 * scale, 6)):
            continue
        k = rng.random()
        size = rng.uniform(0.5, 1.2) * scale
        if k < 0.45:
            mid = math.pi / 2 + rng.uniform(-0.8, 0.6)
            span = rng.uniform(0.9, 2.6)
            sh = arc1(p, size, mid - span / 2, mid + span / 2)
        elif k < 0.85:
            sh = squiggle(p, size * rng.uniform(1.4, 2.4), rng.uniform(-0.7, 0.7), 0.35 * size, rng)
        else:
            sh = curl(p, size * 0.6, rng.uniform(0, 6.28), rng.uniform(3.6, 4.6) * (1 if rng.random() < 0.5 else -1))
        if kind == "white":
            op = q(rng.uniform(0.12, 0.3) + 0.22 * (1 - l), 4)
            m.add((mixc("#b8ae99", "#8a7f69", 1 - l), round(rng.choice((0.22, 0.28)) * scale, 2), op), sh)
        else:
            m.add(("#050404", round(rng.choice((0.26, 0.34)) * scale, 2), rng.choice((0.45, 0.6))), sh)
        if rng.random() < (0.55 if kind == "white" else 0.75):
            hp = (p[0] + rng.uniform(-0.8, 0.5), p[1] + rng.uniform(-1.0, -0.2))
            hr = size * rng.uniform(0.4, 0.7)
            hm = -math.pi / 2 + rng.uniform(-0.9, 0.4)
            hs = rng.uniform(0.7, 1.7)
            hl = arc1(hp, hr, hm - hs / 2, hm + hs / 2)
            if kind == "white":
                m.add(("#fefdf9", round(0.24 * scale, 2), q(0.1 + 0.6 * l, 4)), hl)
            else:
                t = q(tip(p)) if tip else 0.0
                m.add((mixc("#3d3639", "#6a4c37", t), round(0.27 * scale, 2), q(0.3 + 0.55 * max(t, 0.5 * l))), hl)
        if crimp and rng.random() < crimp(p):
            ang = math.pi / 2 + rng.uniform(-0.7, 0.7)
            w = wave((p[0] + rng.uniform(-0.4, 0.4), p[1] + rng.uniform(-0.4, 0.4)), rng.uniform(0.9, 1.3) * scale, ang,
                     0.28 * scale)
            if kind == "white":
                m.add(("#a59b86", round(0.18 * scale, 2), 0.35), w)
            else:
                m.add((mixc("#463d42", "#6c503d", q(tip(p)) if tip else 0), round(0.18 * scale, 2), 0.5), w)


def soft_lumps(m, region, kind, rng, r=4.5, tip=None):
    """Mèches groupées plus larges : de grandes ombres très douces."""
    if region.is_empty:
        return
    from shapely.prepared import prep
    inside = prep(region)
    for p in poisson(region, r, rng):
        rad = rng.uniform(1.6, 2.3)
        if not inside.contains(Point(p).buffer(rad + 0.4, 8)):
            continue
        a = rng.uniform(-0.3, 0.3)
        if kind == "white":
            m.add(("#9d927c", 0.7, 0.09), arc1(p, rad, 0.0 + a, 1.6 + a))
        else:
            t = q(tip(p)) if tip else 0
            m.add(("#060405", 0.75, 0.3), arc1(p, rad, 0.0 + a, 1.6 + a))
            m.add((mixc("#3b3236", "#5f4637", t), 0.6, q(0.12 + 0.25 * t)), arc1(p, rad * 0.9, 3.5 + a, 4.9 + a))


def edge_locks(m, line_pts, rng, color_at, step=0.8, scale=1.0, closed=True):
    """Petites mèches sur un bord : le contour doux d'une toison."""
    pts = resample(line_pts + ([line_pts[0]] if closed else []), step)
    ns = normals(pts, False)
    for i, p in enumerate(pts):
        if rng.random() < 0.45:
            continue
        res = color_at(p)
        if not res:
            continue
        col, sw, op = res
        a0 = math.atan2(-ns[i][1], -ns[i][0]) + rng.uniform(-0.9, 0.9)
        rr = rng.uniform(0.3, 0.65) * scale
        c = (p[0] - ns[i][0] * rr * rng.uniform(0.2, 0.7), p[1] - ns[i][1] * rr * rng.uniform(0.2, 0.7))
        sp = rng.uniform(0.6, 1.2)
        m.add((col, sw, op), arc1(c, rr, a0 - sp, a0 + sp))


# =============================================================== Repère de la tête
POLL = (132.6, 116.0)
TH = math.radians(40.0)  # chanfrein à 40° sous l'horizontale
HK = 1.0  # tête de 24,8 cm de la nuque au bout du nez (0,33 du garrot)
HDY = -2.0  # tête relevée d'autant (« up-headed »)


def L(x, y):
    c, s = math.cos(TH), math.sin(TH)
    x, y = x * HK, y * HK
    return (POLL[0] + x * c - y * s, POLL[1] + x * s + y * c)


def Ls(pts):
    return [L(*p) for p in pts]


def up(pts):
    """Points du dessin donnés pour la tête d'avant (relevée de HDY)."""
    return [(x, y + HDY) for x, y in pts]


# =============================================================== Pattes
PIVOT_Y = 152.0


def leg_parts(kind, dx=0.0):
    """Contour d'une patte : une seule pièce, de l'épaule ou la hanche au sabot."""
    if kind == "front":
        front = [(116.4, 148), (117.2, 158), (117.0, 166), (116.3, 172), (115.6, 176.4), (115.4, 178.4),
                 (115.75, 180.0), (115.85, 181.9), (115.35, 183.8), (114.75, 185.6), (114.55, 188.8), (114.7, 191.2),
                 (115.2, 192.8), (116.0, 194.6), (116.75, 196.6)]
        back = [(112.65, 197.1), (111.95, 195.8), (111.35, 194.2), (111.05, 192.6), (111.35, 190.8), (111.6, 188.0),
                (111.65, 185.4), (111.4, 183.6), (110.95, 182.3), (111.05, 180.6), (110.7, 178.6), (109.9, 176.2),
                (108.6, 172), (107.4, 166), (107.2, 158), (108.0, 148)]
        cap = [(109.7, 145.6), (112.5, 144.6), (115.3, 145.6)]
        pivot = (112.5, PIVOT_Y)
        hoof = [(112.35, 200), (117.85, 200), (117.95, 199.55), (117.55, 198.45), (116.95, 197.1), (116.55, 196.1),
                (115.2, 195.9), (113.9, 196.15), (112.95, 196.75), (112.4, 197.6), (112.1, 198.8)]
        wool_y = 177.2
        spot = ((115.0, 181.3), 0.95, 2.3)
        dew = (111.15, 193.7)
        fet_y = 192.6
    else:
        front = [(55.2, 148), (55.5, 156), (54.7, 161.5), (52.9, 166.2), (50.3, 170.4), (48.1, 173.8), (46.6, 176.2),
                 (45.6, 178.6), (45.2, 181.5), (45.0, 185.0), (44.95, 188.0), (45.1, 191.2), (45.55, 193.0),
                 (46.3, 194.7), (47.1, 196.6)]
        back = [(43.0, 197.1), (42.3, 195.8), (41.7, 194.2), (41.4, 192.6), (41.65, 190.6), (41.8, 187.6),
                (41.7, 184.6), (41.3, 182.4), (40.4, 180.8), (39.1, 179.5), (38.5, 178.3), (38.8, 176.9), (39.2, 174.4),
                (38.9, 171.0), (37.9, 167.0), (36.6, 162.0), (35.9, 157.0), (37.2, 149.0)]
        cap = [(42.0, 145.6), (47.0, 143.8), (52.4, 145.0)]
        pivot = (48.6, PIVOT_Y)
        hoof = [(42.7, 200), (48.2, 200), (48.3, 199.55), (47.9, 198.45), (47.3, 197.1), (46.9, 196.1), (45.55, 195.9),
                (44.25, 196.15), (43.3, 196.75), (42.75, 197.6), (42.45, 198.8)]
        wool_y = 175.0
        spot = ((39.9, 178.4), 1.7, 2.0)
        dew = (41.5, 193.7)
        fet_y = 192.6
    sh = lambda pts: [(x + dx, y) for x, y in pts]
    return dict(outline=sh(front + back + cap), pivot=(pivot[0] + dx, pivot[1]), hoof=sh(hoof), wool_y=wool_y,
                spot=((spot[0][0] + dx, spot[0][1]), spot[1], spot[2]), dew=(dew[0] + dx, dew[1]),
                front=sh(front), back=sh(back), fet_y=fet_y)


def draw_leg(gid, kind, dx, far, pastern_mark, comment):
    P = leg_parts(kind, dx)
    D.open(gid, comment)
    pts = cr_sample(P["outline"], True, 0.3)
    leg = poly(pts)
    x0 = min(p[0] for p in pts)
    x1 = max(p[0] for p in pts)
    if far:
        hair = D.lin(gid + "-h", x0, 0, x1, 0, [(0, "#b4afa3"), (1, "#8b8578")])
        woolg = D.lin(gid + "-w", 0, 150, 0, 178, [(0, "#9b9383"), (1, "#7f7768")])
    else:
        hair = D.lin(gid + "-h", x0, 0, x1, 0, [(0, "#fcfcf9"), (0.45, HAIR_WHITE), (1, "#cbc9bf")])
        woolg = D.lin(gid + "-w", 0, 150, 0, 178, [(0, "#d3ccbc"), (1, "#b9b09d")])
    D.path(poly_d(pts), fill=hair)
    rng = random.Random(len(gid) * 31 + int(dx * 10) + (7 if kind == "front" else 0))
    # La laine descend jusqu'au-dessus du genou ou du jarret : son bord est fait de
    # mèches, pas d'un trait. Plus bas, poil ras et blanc.
    edge = [(x0 - 4, P["wool_y"])]
    x = x0 - 3
    while x < x1 + 4:
        edge.append((x, P["wool_y"] + rng.uniform(-0.55, 0.55)))
        x += rng.uniform(0.6, 1.1)
    cut = poly([(x0 - 4, 140), (x1 + 4, 140)] + edge[::-1])
    wool = leg.intersection(cut.buffer(0.35).buffer(-0.35))
    D.path(geom_d(wool), fill=woolg)
    m = Marks()
    locks(m, wool, "white", rng, lambda p: 0.25, r=1.15, scale=0.85)
    for p in resample(edge, 0.7):
        if leg.buffer(-0.25).contains(Point(p)) and rng.random() < 0.8:
            m.add(("#9e9585" if not far else "#6c6556", 0.26, 0.55), arc1(p, rng.uniform(0.4, 0.55), 0.2, 2.6))
            m.add(("#efebe2" if not far else "#a39b8b", 0.24, 0.7), arc1((p[0] - 0.15, p[1] - 0.3), 0.45, 3.4, 4.9))
    m.emit(D)
    hair_zone = leg.difference(cut).buffer(-0.3)
    hs = []
    for p in poisson(hair_zone, 0.8, rng):
        Lh = rng.uniform(0.6, 1.1)
        hs.append(poly_d([(p[0], p[1] - Lh / 2), (p[0] + 0.1, p[1] + Lh / 2)], closed=False))
    D.path("".join(hs), stroke="#bdbbb2" if not far else "#77756d", sw=0.13, sop=0.45)
    # Articulations : l'os du carpe derrière le genou ; le jarret ; le boulet.
    col_sh = "#857d6f" if not far else "#56504a"
    f, b = P["front"], P["back"]
    if kind == "front":
        D.path(curve_d([(b[9][0] + 0.5, 180.2), (b[8][0] + 0.6, 182.2), (b[7][0] + 0.55, 184.0)]), stroke=col_sh,
               sw=0.32, sop=0.35)
        D.path(curve_d([(f[10][0] - 0.45, 189.5), (f[11][0] - 0.5, 191.4), (f[12][0] - 0.6, 192.8)]), stroke=col_sh,
               sw=0.3, sop=0.3)
    else:
        D.path(curve_d([(b[12][0] + 0.7, 175.6), (b[10][0] + 0.9, 178.6), (b[8][0] + 0.7, 181.2)]), stroke=col_sh,
               sw=0.36, sop=0.4)
        D.path(curve_d([(f[6][0] - 0.5, 176.6), (f[7][0] - 0.55, 179.4), (f[8][0] - 0.5, 182.4)]), stroke=col_sh,
               sw=0.3, sop=0.28)
    # Tache noire au genou (devant du carpe) ou au jarret (pointe et côté).
    c, rx, ry = P["spot"]
    r2 = random.Random(int(c[0] * 7))
    k1, k2 = r2.uniform(0, 6.28), r2.uniform(0, 6.28)
    sp = []
    for i in range(36):
        a = 2 * math.pi * i / 36
        rr = 1 + 0.12 * math.sin(3 * a + k1) + 0.07 * math.sin(7 * a + k2)
        sp.append((c[0] + rx * rr * math.cos(a), c[1] + ry * rr * math.sin(a)))
    D.path(geom_d(leg.intersection(poly(sp))), fill=BLACK_TRUE if not far else "#141213")
    # Petite marque noire juste au-dessus du sabot.
    if pastern_mark:
        y0 = P["fet_y"] + 2.0
        edge2 = [(x0 - 3 + i * 0.7, y0 + 0.3 * math.sin(i * 1.9) + 0.2 * math.cos(i * 3.3)) for i in range(28)]
        D.path(geom_d(leg.intersection(poly(edge2 + [(x1 + 4, 199), (x0 - 3, 199)]))),
               fill=BLACK_TRUE if not far else "#141213")
    # Onglon (ergot) à l'arrière du boulet.
    ex, ey = P["dew"]
    ex += 0.35
    D.path(smooth_d([(ex - 0.32, ey - 0.42), (ex + 0.2, ey - 0.5), (ex + 0.3, ey + 0.12), (ex - 0.05, ey + 0.5),
                     (ex - 0.42, ey + 0.12)]), fill="#1a1718" if not far else "#0f0d0d")
    # Sabot : petit, pointu, noir ; fendu (on voit la fente et un peu de l'autre onglon).
    hoof = P["hoof"]
    hx0, hx1 = hoof[0][0], hoof[1][0]
    hoofg = D.lin(gid + "-hf", hx0, 0, hx1, 0, [(0, "#0f0d0d"), (0.55, HOOF), (1, "#2e2a28")] if not far else
                  [(0, "#0b0909"), (1, "#181514")])
    D.path(poly_d([(hx1 - 0.3, 200), (hx1 + 0.35, 200), (hx1 + 0.3, 199.55), (hx1 - 0.35, 198.8)]), fill="#0d0b0b")
    upper = cr_sample(hoof[1:] + [hoof[0]], False, 0.2)
    sole = [(hoof[0][0], GY)] + [(x, min(y, GY)) for x, y in upper[:-1]] + [(hoof[0][0], GY)]
    D.path(poly_d(sole[:-1]), fill=hoofg)
    D.path(curve_d([(hx1 - 0.25, 199.85), (hx1 - 0.75, 198.6), (hx1 - 1.2, 197.3), (hx1 - 1.5, 196.5)]),
           stroke="#040303", sw=0.18, sop=0.9)
    if not far:
        D.path(curve_d([(hx1 - 0.6, 199.3), (hx1 - 1.1, 198.0), (hx1 - 1.5, 196.9)]), stroke="#6a625d", sw=0.22,
               sop=0.45)
    cor = []
    for i in range(7):
        x = lerp(hx0 + 0.9, hx1 - 1.6, i / 6)
        y = 196.35 - 0.25 * math.sin(i / 6 * math.pi)
        cor.append(poly_d([(x, y - 0.6), (x + 0.12, y + rng.uniform(0.15, 0.45))], closed=False))
    if not pastern_mark:
        D.path("".join(cor), stroke="#f4f4ef" if not far else "#9c9a92", sw=0.2, sop=0.75)
    D.pivot(gid, P["pivot"])
    D.close()
    return P


# =============================================================== Corps
BODY_PTS = [
    # garrot, dos (léger creux derrière le garrot), rein, hanches
    (106.0, 124.0), (100.5, 124.6), (94, 125.2), (86, 125.3), (77, 124.9), (68, 124.2), (60.5, 123.7), (54.5, 123.9),
    # croupe qui s'arrondit jusqu'à l'attache de la queue, puis la fesse en « D »
    (48.8, 125.0), (44.0, 126.9), (40.2, 129.6), (37.0, 133.0), (34.6, 137.2), (32.9, 142.0), (32.1, 147.0),
    (32.2, 152.0), (33.2, 156.8), (35.0, 160.9), (37.5, 164.2),
    # dessous de la cuisse, pli du grasset (le flanc remonte), ventre en tonneau
    (41.0, 166.4), (45.4, 167.2), (49.6, 166.3), (53.2, 164.6), (56.6, 163.9), (60.0, 165.0), (64.6, 167.3),
    (71, 169.6), (79, 171.3), (88, 172.4), (97, 172.9), (104.5, 173.2), (110.5, 173.0),
    # poitrail arrondi, pointe de la poitrine, gorge
    (116.0, 171.6), (120.4, 168.9), (123.8, 165.0), (126.0, 160.4), (126.9, 155.6), (126.8, 151.2), (127.3, 146.8),
    (128.3, 141.8), (129.2, 137.2), (129.8, 133.6), (129.6, 130.4),
    # haut de l'encolure, caché sous la tête
    (127.0, 126.4), (123.2, 123.2), (118.4, 121.4), (113.6, 121.2), (109.4, 122.2),
]

# Taches (repère du dessin) ; ce qui dépasse de la silhouette est coupé.
PATCH_CAPE = [  # la cape : dessus et côtés de l'encolure, garrot, épaule jusqu'au coude
    (143, 104), (137, 119), (126.7, 130.0), (125.7, 134.0), (124.7, 138.2), (123.6, 142.4), (122.6, 146.4),
    (121.4, 150.2), (119.8, 153.6), (118.6, 157.0), (117.4, 159.8), (115.2, 161.2), (113.2, 159.8), (111.8, 157.0),
    (109.6, 156.4), (108.0, 159.0), (105.8, 161.8), (103.0, 162.2), (100.8, 159.8), (99.6, 156.0), (97.9, 154.0),
    (96.4, 150.8), (97.4, 146.2), (96.0, 141.4), (97.2, 136.6), (95.6, 131.8), (96.6, 127.4), (95.4, 117),
    (104, 108), (122, 102),
]
PATCH_RIB = [  # derrière l'épaule, du dessus jusqu'au milieu du flanc, en biais
    (91.8, 117), (90.0, 125.2), (90.8, 130.0), (89.0, 134.8), (89.6, 139.8), (87.4, 144.2), (87.6, 148.0), (85.0, 151.4),
    (81.4, 152.2), (78.8, 149.8), (78.2, 145.6), (79.6, 141.6), (78.4, 137.2), (80.4, 133.0), (79.2, 128.6),
    (81.0, 124.2), (80.2, 117),
]
PATCH_FLANK = [(65.6, 138.4), (68.6, 140.8), (67.4, 143.6), (69.0, 146.8), (66.8, 149.4), (65.0, 153.2), (61.6, 152.4),
               (59.0, 154.0), (57.4, 150.6), (58.4, 147.6), (56.6, 144.2), (59.4, 142.0), (60.2, 138.8), (62.8, 139.8)]
PATCH_HIP = [(56.6, 116), (55.6, 126.6), (56.6, 132.4), (54.6, 137.6), (55.0, 143.4), (52.4, 148.4), (48.8, 152.4),
             (44.8, 154.2), (41.4, 152.4), (39.6, 147.8), (39.0, 142.6), (40.4, 137.6), (39.6, 132.8), (42.2, 127.2),
             (44.8, 116)]
SPOTS = [((70.6, 127.2), 2.5, 2.1, 5), ((79.6, 160.2), 1.5, 1.35, 6), ((92.4, 162.4), 1.0, 0.95, 7)]


def patch_poly(pts, seed, amp=1.0):
    s = cr_sample(pts, True, 0.3)
    s = displace(s, Noise1D(seed, [(4.5, 0.4), (1.8, 0.22), (0.75, 0.1)]), amp)
    return poly(s)


def spot_poly(c, rx, ry, seed):
    r = random.Random(seed)
    k = r.uniform(0, 6.28)
    pts = []
    for i in range(32):
        a = 2 * math.pi * i / 32
        rr = 1 + 0.16 * math.sin(2 * a + k) + 0.08 * math.sin(5 * a + 2 * k)
        pts.append((c[0] + rx * rr * math.cos(a), c[1] + ry * rr * math.sin(a)))
    return poly(pts)


def build_body():
    smooth = cr_sample(BODY_PTS, True, 0.4)
    body0 = poly(smooth)
    # Mèches qui cassent le contour : des touffes de laine irrégulières sur le bord.
    rng = random.Random(23)
    pts = resample(smooth + [smooth[0]], 0.8)
    ns = normals(pts, False)
    tufts = []
    i = 0
    while i < len(pts) - 1:
        p, n = pts[i], ns[i]
        rx_, ry_ = rng.uniform(0.7, 1.5), rng.uniform(0.45, 0.9)
        o = ry_ * rng.uniform(0.2, 0.75)
        c = (p[0] - n[0] * o, p[1] - n[1] * o)
        e = affinity.scale(Point(c).buffer(1.0, 8), rx_, ry_)
        e = affinity.rotate(e, math.degrees(math.atan2(n[0], -n[1])) + rng.uniform(-25, 25), origin=c)
        tufts.append(e)
        i += rng.randint(2, 5)
    for c, rx_, ry_, ang in [((104.0, 124.4), 2.4, 1.1, -5), ((58.0, 124.0), 2.2, 1.0, 5), ((39.0, 129.0), 2.0, 1.1, 40),
                             ((32.6, 150.0), 1.2, 2.4, 0), ((35.6, 161.0), 1.3, 2.0, 30), ((75.0, 170.6), 2.6, 1.2, 8),
                             ((92.0, 172.6), 2.2, 1.0, 2), ((121.6, 167.6), 1.8, 1.2, -40)]:
        e = affinity.rotate(affinity.scale(Point(c).buffer(1.0, 10), rx_, ry_), ang, origin=c)
        tufts.append(e)
    body = unary_union([body0] + tufts).buffer(0.2).buffer(-0.2)
    outline = list(body.exterior.coords)[:-1]
    patches = [patch_poly(PATCH_CAPE, 11), patch_poly(PATCH_RIB, 12), patch_poly(PATCH_FLANK, 13, 0.8),
               patch_poly(PATCH_HIP, 14)]
    for c, rx, ry, seed in SPOTS:
        patches.append(spot_poly(c, rx, ry, seed))
    black = unary_union(patches).intersection(body)
    white = body.difference(black)
    return outline, body, black, white


def light_body(p):
    return 1 - 0.85 * smoothstep(124, 172, p[1]) - 0.1 * smoothstep(60, 25, p[0])


def sun_tip(p):
    """Pointes brûlées par le soleil (brun chocolat), surtout sur le dessus."""
    return 1 - smoothstep(119, 152, p[1])


def neck_crimp(p):
    """La frisure se voit surtout sur l'encolure et l'épaule, peu sur l'arrière."""
    return 0.08 + 0.4 * smoothstep(85, 118, p[0])


def draw_body(outline, body, black, white):
    D.open("body", "Corps : toison de 4 à 6 cm, pie noir et blanc ; l'encolure en fait partie")
    gw = D.lin("wool-white", 0, 120, 0, 174, [(0, "#f2efe7"), (0.3, WOOL_LIT), (0.66, "#dcd5c7"), (1, "#bcb3a0")])
    gb = D.lin("wool-black", 0, 117, 0, 174, [(0, "#45332a"), (0.25, "#302726"), (0.6, BLACK_WOOL), (1, "#1a1719")])
    fuzz = displace(resample(outline + [outline[0]], 0.35)[:-1], Noise1D(5, [(1.1, 0.12), (0.5, 0.05)]), 1.0)
    D.path(poly_d(fuzz), fill=gw)
    # Le volume du tonneau : plus clair en haut à gauche, plus sombre au pourtour.
    vol = D.rad("wool-vol", 74, 129, 62, [(0, "#ffffff", 0.2), (0.45, "#ffffff", 0.0), (0.74, "#5f5544", 0.0),
                                          (1, "#5f5544", 0.28)])
    D.path(poly_d(fuzz), fill=vol)
    D.path(geom_d(black, 0.02), fill=gb)
    bvol = D.rad("black-vol", 74, 127, 62, [(0, "#7a5a44", 0.16), (0.5, "#7a5a44", 0.0), (0.78, "#000000", 0.0),
                                            (1, "#000000", 0.3)])
    D.path(geom_d(black, 0.02), fill=bvol)
    # Volumes très doux : coude, flanc, cuisse, ventre, gorge.
    soft = [
        ([(108.6, 162.5), (110.8, 167.0), (113.6, 171.0)], "#8a7f6a", 1.8, 0.16),
        ([(41.5, 166.0), (52, 166.0), (60, 166.6), (80, 170.8), (109, 173.4)], "#82775f", 2.4, 0.14),
        ([(129.0, 132.6), (128.2, 137.0)], "#7d7260", 1.6, 0.1),
    ]
    for pts, col, sw, op in soft:
        D.path(curve_d(pts), stroke=col, sw=sw, sop=op)
    D.path(curve_d([(51.0, 127.6), (55.5, 126.9), (59.0, 127.4)]), stroke="#ffffff", sw=1.4, sop=0.14)
    m = Marks()
    rng = random.Random(77)
    soft_lumps(m, white.buffer(-0.8), "white", rng)
    soft_lumps(m, black.buffer(-0.8), "black", rng, tip=sun_tip)
    m.emit(D)
    locks(m, white.buffer(-0.3), "white", rng, light_body, r=1.3, crimp=neck_crimp)
    locks(m, black.buffer(-0.3), "black", rng, light_body, r=1.3, tip=sun_tip, crimp=neck_crimp)
    m.emit(D)
    # Bords des taches nets, mais laineux : des pointes de mèches qui débordent.
    for b in polys_of(black):
        ring = list(b.exterior.coords)[:-1]
        line = LineString(ring + [ring[0]]).intersection(body.buffer(-0.7))
        segs = [list(g.coords) for g in getattr(line, "geoms", [line]) if g.geom_type == "LineString"]
        for sgm in segs:
            if len(sgm) < 3:
                continue
            for p in resample(sgm, 1.1):
                if rng.random() < 0.55:
                    a = rng.uniform(0, 6.28)
                    m.add(("#1c181a", 0.3, 0.75), arc1(p, rng.uniform(0.35, 0.5), a, a + 2.2))

    def col_at(c):
        if black.buffer(0.3).contains(Point(c)):
            return ("#130f11", 0.3, 0.9)
        return (mixc("#cdc5b4", "#998e79", smoothstep(128, 172, c[1])), 0.26, 0.55)
    edge_locks(m, outline, rng, col_at, step=0.7)
    m.emit(D)
    # Un trait discret sur les bords blancs, pour la lisibilité sur un fond clair.
    rim = LineString(fuzz + [fuzz[0]]).difference(black.buffer(0.6))
    segs = [list(g.coords) for g in getattr(rim, "geoms", [rim]) if g.geom_type == "LineString" and g.length > 1]
    D.path("".join(poly_d(s_, closed=False) for s_ in segs), stroke="#a59b88", sw=0.3, sop=0.55)
    D.pivot("body", (80.5, 172.4))
    D.close()


# =============================================================== Queue
def draw_tail():
    # Queue naturelle d'environ 37 cm (0,5 du garrot) : elle s'arrête une main
    # au-dessus du jarret.
    center = cr_sample([(37.2, 131.8), (34.0, 134.6), (31.2, 139.4), (29.4, 145.6), (28.8, 152.6), (28.9, 159.4),
                        (29.4, 164.8), (30.0, 168.6)], False, 0.3)
    hw = lambda f: 2.85 - 0.95 * f
    left, right = tube(center, hw)
    cuts = [0.0, 0.36, 0.68, 1.0]
    gt = D.lin("tail-g", 25, 0, 33, 0, [(0, "#eeeae1"), (0.55, "#dbd4c6"), (1, "#a9a08c")])
    n = len(center)
    segs, pivots = [], []
    for k in range(3):
        i0 = int(cuts[k] * (n - 1))
        i1 = int(cuts[k + 1] * (n - 1))
        lo, hi = max(0, i0 - 3), min(n - 1, i1 + 3)
        seg = poly(left[lo:hi + 1] + right[lo:hi + 1][::-1])
        caps = [Point(center[lo]).buffer(hw(lo / (n - 1))), Point(center[hi]).buffer(hw(hi / (n - 1)))]
        segs.append(unary_union([seg] + caps))
        pivots.append(center[i0])
    names = ["tail", "tail-mid", "tail-tip"]
    notes = ["Queue naturelle, laineuse, jusqu'au-dessus du jarret : trois morceaux qui ondulent", None, None]
    rng = random.Random(5)
    for k in range(3):
        D.open(names[k], notes[k])
        g = segs[k]
        ring = list(g.exterior.coords)[:-1]
        fz = displace(ring, Noise1D(40 + k, [(1.1, 0.16), (0.5, 0.06)]), 1.0)
        D.path(poly_d(fz), fill=gt)
        m = Marks()
        locks(m, g.buffer(-0.25), "white", rng, lambda p: 0.55 - 0.4 * smoothstep(135, 165, p[1]), r=1.15, scale=0.85)
        edge_locks(m, ring, rng, lambda c: ("#b3aa97", 0.26, 0.7), step=0.75, scale=0.85)
        m.emit(D)
        D.pivot(names[k], pivots[k])
    D.close()
    D.close()
    D.close()


# =============================================================== Cornes
def horn(center, hw, lit_side=1, seed=1, far=False, tip_smooth=0.16, root_cap=False, spacing=1.0, keel=False):
    """Corne noire annelée : tube effilé, côté éclairé, crêtes et sillons serrés."""
    if VARIANTE == "agneau":
        return
    if VARIANTE == "brebis":
        # Fines, courtes, délicates (§ 6.10) : le début de l'axe, à demi-épaisseur.
        center = center[:max(4, int(len(center) * .62))]
        fort = hw
        hw = lambda f: fort(f) * .55  # noqa: E731
    left, right = tube(center, hw)
    outline = left + right[::-1]
    base = HORN_MID if not far else "#241e1b"
    D.path(poly_d(outline), fill=base)
    if root_cap:
        D.path(smooth_d(arc_pts(center[0], hw(0), 0, 2 * math.pi, 18)[:-1]), fill=base)
    a, b = (-1.0, -0.2) if lit_side > 0 else (0.2, 1.0)
    c_, d_ = (0.3, 1.0) if lit_side > 0 else (-1.0, -0.3)
    D.path(poly_d(band(center, hw, a, b)), fill="#5a5048" if not far else "#2e2723", op=0.7)
    D.path(poly_d(band(center, hw, c_, d_)), fill=HORN_DARK, op=0.85)
    s = arclen(center, False)
    total = s[-1]
    rng = random.Random(seed)
    crest, groove, annual = [], [], []
    f = 0.02
    k = 0
    while f < 1 - tip_smooth:
        p, t = at_frac(center, f)
        nrm = (-t[1], t[0])
        w = hw(f) * 0.98
        bow = 0.25 * w
        pts = []
        for i in range(7):
            u = -1 + 2 * i / 6
            off = bow * (1 - u * u)
            pts.append((p[0] + nrm[0] * w * u - t[0] * off, p[1] + nrm[1] * w * u - t[1] * off))
        lit = pts[:4] if lit_side > 0 else pts[3:]
        crest.append(curve_d(lit))
        groove.append(curve_d([(x - t[0] * 0.3, y - t[1] * 0.3) for x, y in pts]))
        if k % 5 == 4:
            annual.append(curve_d([(x - t[0] * 0.15, y - t[1] * 0.15) for x, y in pts]))
        f += (0.62 + 1.0 * f) * spacing * (1 + rng.uniform(-0.2, 0.2)) / total
        k += 1
    D.path("".join(groove), stroke="#090605", sw=0.3, sop=0.7 if not far else 0.6)
    D.path("".join(annual), stroke="#040202", sw=0.5, sop=0.6)
    D.path("".join(crest), stroke=HORN_RIDGE if not far else "#4a433d", sw=0.24, sop=0.5 if not far else 0.35)
    if keel:
        # L'arête de la corne tourne autour d'elle : la légère torsion vers l'extérieur.
        kl = []
        for i in range(16):
            ff = 0.04 + 0.8 * i / 15
            p, t = at_frac(center, ff)
            nrm = (-t[1], t[0])
            w = hw(ff) * lerp(0.62, -0.35, ff / 0.84)
            kl.append((p[0] + nrm[0] * w, p[1] + nrm[1] * w))
        D.path(curve_d(kl), stroke="#100c0b", sw=0.55, sop=0.4)
        D.path(curve_d([(x - 0.35, y - 0.1) for x, y in kl]), stroke="#6e665f", sw=0.2, sop=0.3)
    if not far:
        sp = []
        for i in range(12):
            ff = 0.1 + 0.72 * i / 11
            p, t = at_frac(center, ff)
            nrm = (-t[1], t[0])
            w = hw(ff) * (-0.6 if lit_side > 0 else 0.6)
            sp.append((p[0] + nrm[0] * w, p[1] + nrm[1] * w))
        D.path(curve_d(sp), stroke=HORN_SPEC, sw=0.3, sop=0.18)
    D.path(poly_d(outline), stroke="#070504", sw=0.26, sop=0.85)
    return outline


# =============================================================== Tête
HEAD_RUN1 = [  # du coin de l'encoche : le bas de la mâchoire, l'arrière, le dessus, le chanfrein, le nez
    (15.4, 11.75), (12.5, 12.25), (9.0, 12.8), (6.0, 13.15), (4.0, 13.0), (2.6, 11.9), (1.7, 10.1), (0.4, 8.7),
    (-1.8, 7.6), (-3.6, 6.0), (-4.7, 3.6), (-4.6, 1.0), (-3.2, -0.6), (-1.0, -1.0), (2.0, -0.95), (5.0, -0.85),
    (8.0, -0.7), (11.0, -0.3), (14.0, 0.15), (17.0, 0.6), (19.2, 1.1), (20.8, 1.75), (22.0, 2.75), (22.65, 4.0),
    (22.85, 5.35), (22.7, 6.55), (22.25, 7.4),
]
MOUTH = [(22.25, 7.4), (21.1, 7.85), (19.3, 8.1), (17.3, 8.3), (15.4, 8.45)]
JAW_PIVOT = (15.4, 8.45)
EYE_L = (8.3, 5.2)
BLAZE_LO = [(20.5, 1.65), (20.0, 3.05), (18.5, 3.85), (16.2, 3.7), (13.4, 3.2), (11.0, 2.6), (9.0, 2.3), (7.0, 2.25),
            (5.0, 2.05), (2.5, 1.7), (0.2, 1.15), (-1.9, 0.45), (-3.0, -0.75)]
TOP_LINE = [(-3.2, -0.6), (-1.0, -1.0), (2.0, -0.95), (5.0, -0.85), (8.0, -0.7), (11.0, -0.3), (14.0, 0.15),
            (17.0, 0.6), (19.2, 1.1), (20.5, 1.65)]
NOSE = [(20.5, 1.65), (21.4, 2.1), (22.0, 2.75), (22.65, 4.0), (22.85, 5.35), (22.7, 6.55), (22.25, 7.4), (21.3, 7.85),
        (20.6, 6.6), (20.2, 4.9), (20.15, 3.1)]

# La lèvre supérieure (repère de la tête) : sous la narine, du bout de la
# truffe à la commissure, jusque sous la ligne de la bouche ; elle se
# retrousse autour de son arrière pour le flehmen et découvre la gencive (le
# bourrelet du haut, sans incisives), d'un rose sombre sous la peau noire.
LIP = [(18.8, 6.95), (20.4, 6.75), (21.3, 6.7), (22.0, 6.65), (23.6, 6.4), (23.6, 8.5), (22.0, 8.3),
       (21.1, 8.3), (19.3, 8.55), (18.8, 8.6)]
LIP_PIVOT = (19.0, 7.15)
GUM = [(19.3, 7.4), (20.3, 7.2), (21.2, 6.98), (22.0, 6.75), (22.35, 7.05), (21.9, 7.3), (21.0, 7.62),
       (20.0, 7.85), (19.3, 7.95)]
GUM_RIDGE = [(19.6, 7.75), (20.6, 7.58), (21.4, 7.4), (21.95, 7.1)]
GUM_FILL, GUM_CREASE = "#a86f7b", "#7d4a57"

# Cornes, oreilles, nuque (repère du dessin, tête relevée de HDY)
def crescent(base, length, a0, radius, n=80):
    """Ligne médiane d'une corne en croissant : elle part de `a0` (degrés) et
    tourne régulièrement vers l'arrière."""
    pts = [base]
    a = math.radians(a0)
    ds = length / n
    x, y = base
    for _ in range(n):
        x += ds * math.cos(a)
        y += ds * math.sin(a)
        pts.append((x, y))
        a -= ds / radius
    return pts


def horn_hw(base_hw):
    return lambda f: 0.1 + base_hw * (1 - f ** 1.5) ** 0.85


UPPER_NEAR = crescent(up([(134.2, 121.4)])[0], 37.5, -94.0, 40.0)
UPPER_FAR = crescent(up([(132.6, 121.4)])[0], 35.0, -100.0, 34.0)
LOWER_NEAR = up([(126.0, 120.8), (123.0, 118.4), (119.6, 118.3), (116.8, 120.6), (115.5, 124.8), (115.8, 129.6),
                 (117.9, 133.8), (121.5, 136.4), (125.7, 137.2), (129.3, 136.0)])
EAR_BASE = up([(124.9, 125.6)])[0]
EAR_FAR_BASE = up([(123.6, 123.0)])[0]
HEAD_PIVOT = up([(121.4, 122.6)])[0]


def head_outline():
    return runs_sample([Ls(HEAD_RUN1), Ls(MOUTH)], 0.3)


def draw_jaw():
    D.open("jaw", "Mâchoire : le bas du museau ; dessus, cachés bouche fermée, l'intérieur et les incisives du bas")
    J = L(*JAW_PIVOT)
    D.path(poly_d(Ls([(13.6, 8.0), (16.0, 7.85), (16.2, 11.85), (13.6, 12.1)])), fill="#1b1819")
    D.path(smooth_d(Ls([(15.4, 8.55), (16.4, 6.6), (18.8, 5.7), (21.2, 5.8), (22.4, 6.8), (22.3, 8.0), (21.7, 8.65),
                        (17.8, 8.95)])), fill="#2b1a1c")
    D.path(smooth_d(Ls([(16.4, 8.45), (18.4, 7.75), (20.4, 7.55), (21.3, 7.95), (20.6, 8.5), (17.6, 8.7)])),
           fill="#8a5a5d")
    D.path(curve_d(Ls([(17.2, 8.2), (19.2, 7.8), (20.6, 7.8)])), stroke="#b07e80", sw=0.22, sop=0.6)
    # Incisives du bas, couchées vers l'avant (en haut, le bourrelet : pas de dents).
    teeth = []
    for i in range(4):
        x = 20.2 + i * 0.32
        y = 7.98 - i * 0.07
        teeth.append(poly_d(Ls([(x - 0.1, y + 0.1), (x + 0.15, y - 0.75), (x + 0.42, y - 0.72), (x + 0.26, y + 0.1)])))
    D.path("".join(teeth), fill="#e8dfc9", stroke="#8c836f", sw=0.08)
    # La lèvre inférieure et le menton, noirs ; le haut de la pièce est caché
    # sous la lèvre supérieure (pas de jour entre les lèvres, bouche fermée).
    jaw_run = [(15.4, 8.45), (17.3, 8.3), (19.3, 8.1), (21.1, 7.85), (21.85, 7.55), (22.1, 8.15), (21.95, 9.0),
               (21.55, 9.95), (20.5, 10.8), (18.6, 11.35), (16.6, 11.65), (15.4, 11.75), (13.8, 11.95)]
    pts = runs_sample([Ls(jaw_run[:4]), Ls(jaw_run[3:])], 0.25) + Ls([(13.8, 8.65)])
    jg = D.lin("jaw-g", *L(18, 7.4), *L(18, 11.8), [(0, "#211e1f"), (1, "#121011")])
    D.path(poly_d(pts), fill=jg)
    D.path(curve_d(Ls([(13.8, 12.07), (15.4, 11.87), (16.8, 11.75), (18.7, 11.45), (20.2, 10.9)])), stroke="#e6e5de",
           sw=0.42, sop=0.8)
    D.path(curve_d(Ls([(22.0, 8.35), (21.85, 9.05), (21.45, 9.95), (20.6, 10.65)])), stroke="#3c3738", sw=0.22, sop=0.6)
    D.pivot("jaw", J)
    D.close()


def eye_almond(E, a=2.05, b_up=1.38, b_lo=1.05, ang=math.radians(16), n=24):
    ca, sa = math.cos(ang), math.sin(ang)
    upper, lower = [], []
    for i in range(n + 1):
        u = -1 + 2 * i / n
        upper.append((a * u, -b_up * (1 - u * u) ** 0.7 * (1 - 0.1 * u)))
        lower.append((a * u, b_lo * (1 - u * u) ** 0.9 * (1 + 0.06 * u)))
    rot = lambda p: (E[0] + p[0] * ca - p[1] * sa, E[1] + p[0] * sa + p[1] * ca)
    ring = upper + lower[::-1][1:-1]
    return [rot(p) for p in ring], [rot(p) for p in upper], [rot(p) for p in lower]


def draw_eyes(head_fill):
    E = L(*EYE_L)
    D.open("eyes", "Œil : iris ambré, pupille en fente horizontale (gardée à l'horizontale par le moteur)")
    ring, upper, lower = eye_almond(E)
    scl = D.rad("sclera", E[0] - 0.4, E[1] - 0.4, 2.5, [(0, "#8b7d70"), (1, "#4c423a")])
    D.path(poly_d(ring), fill=scl)
    D.open("pupils")
    ir = D.rad("iris", E[0], E[1] + 0.2, 2.1, [(0, "#cf9f50"), (0.45, "#b67c36"), (0.78, IRIS), (0.93, IRIS_DARK),
                                               (1, "#3b2511")], fx=E[0] - 0.2, fy=E[1] + 0.55)
    D.path(smooth_d(arc_pts(E, 2.1, 0, 2 * math.pi, 24)[:-1]), fill=ir)
    rng = random.Random(3)
    st = []
    for i in range(30):
        a = 2 * math.pi * i / 30 + rng.uniform(-0.05, 0.05)
        r1 = rng.uniform(1.45, 2.0)
        st.append(poly_d([(E[0] + 0.6 * math.cos(a), E[1] + 0.6 * math.sin(a)),
                          (E[0] + r1 * math.cos(a), E[1] + r1 * math.sin(a))], closed=False))
    D.path("".join(st), stroke="#6a421b", sw=0.09, sop=0.5)
    D.path(smooth_d(arc_pts(E, 1.98, 0, 2 * math.pi, 24)[:-1]), stroke="#2e1c0c", sw=0.24, sop=0.65)
    pw, ph = 2.4, 0.66
    D.add(f'<rect x="{fm(E[0] - pw / 2)}" y="{fm(E[1] - ph / 2)}" width="{fm(pw)}" height="{fm(ph)}" '
          f'rx="{fm(ph / 2)}" fill="{PUPIL}"/>')
    D.pivot("pupils", E)
    D.close()
    D.add(f'<ellipse cx="{fm(E[0] - 0.66)}" cy="{fm(E[1] - 0.6)}" rx="0.46" ry="0.32" fill="#ffffff" '
          f'fill-opacity="0.92" transform="rotate(18 {fm(E[0] - 0.66)} {fm(E[1] - 0.6)})"/>')
    D.add(f'<circle cx="{fm(E[0] + 0.82)}" cy="{fm(E[1] + 0.52)}" r="0.16" fill="#ffffff" fill-opacity="0.45"/>')
    D.path(curve_d(upper[5:15]), stroke="#ffffff", sw=0.35, sop=0.1)
    frame = arc_pts(E, 1, 0, 2 * math.pi, 40)[:-1]
    frame = [(E[0] + (x - E[0]) * 5.8, E[1] + (y - E[1]) * 5.0) for x, y in frame]
    D.path(poly_d(frame) + poly_d(ring), fill=head_fill, rule="evenodd")
    D.path(curve_d(upper), stroke="#0a0808", sw=0.44)
    D.path(curve_d(lower), stroke="#0d0b0b", sw=0.3)
    D.path(curve_d([(x, y - 0.44) for x, y in upper[3:-3]]), stroke="#423d3e", sw=0.24, sop=0.7)
    D.path(curve_d([(x, y + 0.32) for x, y in lower[4:-4]]), stroke="#363132", sw=0.2, sop=0.5)
    D.pivot("eyes", E)
    D.close()
    return E, upper


def ear(gid, base, ang, length, width, far=False, comment=None):
    """Oreille de mouton tenue de côté, presque à l'horizontale. De profil, elle
    sort vers nous à travers la boucle de la corne : on la voit raccourcie, son
    creux gris tourné vers l'avant et le dehors, bordé de noir."""
    D.open(gid, comment)
    ca, sa = math.cos(ang), math.sin(ang)
    T = lambda u, v: (base[0] + u * ca - v * sa, base[1] + u * sa + v * ca)
    top = [T(0, -0.9), T(length * 0.22, -width * 0.42), T(length * 0.55, -width * 0.5), T(length * 0.82, -width * 0.36),
           T(length, -width * 0.04)]
    bot = [T(length * 0.9, width * 0.26), T(length * 0.62, width * 0.48), T(length * 0.3, width * 0.45), T(0, 0.9)]
    shape = cr_sample(top + bot, True, 0.2)
    if not far:
        D.path(smooth_d([T(length * 0.15, width * 0.5), T(length * 0.6, width * 0.75), T(length * 1.0, width * 0.35),
                         T(length * 0.6, width * 0.4)]), fill="#030202", op=0.55)
    fill = D.lin(gid + "-g", *T(0, -width / 2), *T(0, width / 2),
                 [(0, "#4a4342"), (0.35, "#262122"), (1, "#121011")] if not far else [(0, "#1c1a1a"), (1, "#0e0d0d")])
    D.path(poly_d(shape), fill=fill)
    # Le creux : peau gris foncé, quelques poils clairs, une ombre au fond.
    inner = [T(length * 0.16, -width * 0.05), T(length * 0.45, -width * 0.22), T(length * 0.76, -width * 0.14),
             T(length * 0.88, width * 0.06), T(length * 0.66, width * 0.3), T(length * 0.36, width * 0.32),
             T(length * 0.14, width * 0.16)]
    ing = (D.rad(gid + "-in", *T(length * 0.3, width * 0.05), width * 0.75,
                 [(0, "#1f1919"), (0.55, "#4a3f3f"), (1, "#706262")]) if not far else "#272121")
    D.path(smooth_d(inner), fill=ing)
    if not far:
        hs = []
        for i in range(9):
            u = length * (0.3 + 0.06 * i)
            hs.append(poly_d([T(u, width * 0.28), T(u + 0.55, width * 0.08)], closed=False))
        D.path("".join(hs), stroke="#b9adad", sw=0.1, sop=0.6)
        D.path(curve_d(top[1:]), stroke="#8a8281", sw=0.3, sop=0.7)
        D.path(curve_d(bot[:-1]), stroke="#060404", sw=0.32, sop=0.85)
    D.pivot(gid, base)
    D.close()


def tuft_patch(pts, seed, rng):
    ts = displace(cr_sample(pts, True, 0.25), Noise1D(seed, [(1.0, 0.28), (0.45, 0.1)]), 1.0)
    tp = poly(ts)
    tg = D.lin(f"tuft{seed}", 0, 106, 0, 121, [(0, "#f3f0e8"), (0.6, WOOL_LIT), (1, "#c9c1b0")])
    D.path(poly_d(ts), fill=tg)
    m = Marks()
    locks(m, tp.buffer(-0.2), "white", rng, lambda p: 0.8, r=1.0, scale=0.75)
    edge_locks(m, ts, rng, lambda c: ("#b3aa97", 0.24, 0.8), step=0.6, scale=0.75)
    m.emit(D)


def draw_head():
    D.open("head", "Tête : cornes, toupet, face de blaireau ; elle tourne autour de la nuque")
    hgrad = D.lin("face", 124, 113, 142, 139, [(0, "#2c2829"), (0.5, "#1c191a"), (1, "#121011")])
    # Corne du haut, côté caché : derrière la nôtre, sa pointe plus en arrière (le V).
    horn(UPPER_FAR, horn_hw(3.4), lit_side=1, seed=21, far=True)
    # Haut du cou porté par la tête : laine noire de la cape, sous la tête.
    ext = up([(128.4, 116.0), (124.4, 116.5), (120.4, 117.6), (116.6, 119.4), (113.2, 121.7), (110.4, 124.0),
              (111.0, 127.2), (113.0, 130.2), (116.0, 132.8), (119.6, 134.6), (123.0, 135.3), (126.0, 134.4),
              (128.4, 131.8), (128.9, 126.0), (128.8, 120.0)])
    ext_s = displace(cr_sample(ext, True, 0.3), Noise1D(8, [(1.2, 0.16), (0.55, 0.07)]), 1.0)
    ext_p = poly(ext_s)
    gb = D.lin("cape-head", 0, 113, 0, 134, [(0, "#3e3029"), (0.3, "#2d2527"), (1, "#1f1c1e")])
    D.path(poly_d(ext_s), fill=gb)
    m = Marks()
    rng = random.Random(9)
    soft_lumps(m, ext_p.buffer(-0.8), "black", rng, r=3.8, tip=sun_tip)
    locks(m, ext_p.buffer(-0.25), "black", rng, lambda p: 0.55, r=1.25, tip=sun_tip, crimp=lambda p: 0.45)
    edge_locks(m, ext_s[: len(ext_s) // 2], rng, lambda c: ("#130f11", 0.3, 0.9), step=0.7, closed=False)
    m.emit(D)
    # Le toupet blanc de la nuque, entre les deux cornes du haut.
    tuft_patch(up([(126.8, 112.6), (128.4, 110.4), (130.8, 109.4), (133.0, 109.8), (134.4, 111.6), (134.2, 114.0),
                   (132.2, 115.6), (129.4, 116.2), (127.4, 115.4)]), 17, random.Random(17))
    # Corne du haut, de notre côté : épaisse à la base, en croissant vers l'arrière.
    # Dessinée avant la tête, elle sort de derrière la nuque, sous le poil blanc.
    uc = UPPER_NEAR
    horn(uc, horn_hw(3.75), lit_side=1, seed=31, keel=True)
    ear("ear-far", EAR_FAR_BASE, math.radians(196), 6.6, 3.1, far=True, comment="Oreille de l'autre côté")
    # Corne du bas, côté caché : on n'en voit que la pointe, sous la nôtre, devant la gorge.
    far_tip = up([(121.8, 136.6), (124.6, 138.0), (127.6, 138.4), (130.2, 137.7)])
    if VARIANTE != "brebis":
        horn(cr_sample(far_tip, False, 0.25), lambda f: 0.1 + 1.25 * (1 - f) ** 0.9, lit_side=1, seed=23,
             far=True, tip_smooth=0.3, spacing=0.85)
    draw_jaw()
    out = head_outline()
    D.path(poly_d(out), fill=hgrad)
    # Modelé de la face : arcade, pommette, joue (masséter), côté du museau.
    D.path(curve_d(Ls([(5.0, 7.9), (8.3, 8.5), (11.4, 8.1), (13.8, 7.2)])), stroke="#3a3536", sw=1.0, sop=0.35)
    D.path(smooth_d(Ls([(2.6, 8.8), (6.2, 8.8), (9.6, 10.0), (10.2, 11.7), (6.6, 12.2), (3.4, 11.4)])), fill="#ffffff",
           op=0.05)
    D.path(curve_d(Ls([(11.5, 4.4), (15.5, 4.8), (19.2, 5.2)])), stroke="#3c3738", sw=1.1, sop=0.3)
    D.path(curve_d(Ls([(3.6, 12.5), (8.0, 12.55), (13.0, 11.9)])), stroke="#060505", sw=0.8, sop=0.5)
    E, upper = draw_eyes(hgrad)
    D.path(curve_d([(x, y - 1.0) for x, y in upper[4:-5]]), stroke="#47413f", sw=0.28, sop=0.75)
    # Chanfrein blanc, du toupet au nez ; un trait gris clair le détache d'un fond clair.
    bl = cr_sample([L(x, y - 0.06) for x, y in TOP_LINE] + Ls(BLAZE_LO[1:-1]), True, 0.25)
    bgrad = D.lin("blaze", *L(10, -1.0), *L(10, 3.4), [(0, "#fbfbf8"), (0.55, HAIR_WHITE), (1, "#d0cec5")])
    D.path(poly_d(bl), fill=bgrad)
    bpoly = poly(bl)
    rng = random.Random(4)
    hs = []
    for p in poisson(bpoly.buffer(-0.25), 0.6, rng):
        a = TH + math.pi + rng.uniform(-0.2, 0.2)
        Lh = rng.uniform(0.5, 0.85)
        hs.append(poly_d([p, (p[0] + Lh * math.cos(a), p[1] + Lh * math.sin(a))], closed=False))
    D.path("".join(hs), stroke="#cfcdc4", sw=0.12, sop=0.5)
    # Bord du chanfrein : quelques poils blancs mordent sur le noir, et l'inverse.
    es = cr_sample(Ls(BLAZE_LO[1:-1]), False, 0.2)
    ns_ = normals(es, False)
    eh_w, eh_b = [], []
    for i in range(0, len(es), 2):
        if rng.random() < 0.45:
            continue
        p = es[i]
        Lh = rng.uniform(0.2, 0.5)
        sgn = 1 if rng.random() < 0.6 else -1
        tgt = eh_w if sgn > 0 else eh_b
        j = rng.uniform(-0.3, 0.3)
        tgt.append(poly_d([(p[0] - sgn * ns_[i][0] * 0.1, p[1] - sgn * ns_[i][1] * 0.1),
                           (p[0] + sgn * (ns_[i][0] + j) * Lh, p[1] + sgn * (ns_[i][1] - j) * Lh)], closed=False))
    D.path("".join(eh_w), stroke="#e9e8e2", sw=0.13, sop=0.6)
    D.path("".join(eh_b), stroke="#1e1b1c", sw=0.13, sop=0.6)
    D.path(curve_d([L(x, y - 0.06) for x, y in TOP_LINE[1:]]), stroke="#a29f96", sw=0.28, sop=0.6)
    tl = cr_sample(Ls([(-2.6, -0.75), (0.0, -0.98), (2.5, -0.95), (5.0, -0.85), (7.0, -0.75)]), False, 0.2)
    ns_ = normals(tl, False)
    hh = []
    for i in range(0, len(tl), 2):
        if rng.random() < 0.4:
            continue
        p = tl[i]
        Lh = rng.uniform(0.3, 0.9)
        back = rng.uniform(0.15, 0.6)
        hh.append(poly_d([(p[0] + ns_[i][0] * 0.5, p[1] + ns_[i][1] * 0.5),
                          (p[0] - ns_[i][0] * Lh - back * math.cos(TH), p[1] - ns_[i][1] * Lh - back * math.sin(TH))],
                         closed=False))
    D.path("".join(hh), stroke="#f2f2ec", sw=0.24, sop=0.9)
    # Truffe noire, narine en virgule, lèvre fendue.
    D.path(smooth_d(Ls(NOSE)), fill=SKIN_BLACK)
    D.path(smooth_d(Ls([(21.0, 2.25), (21.8, 2.7), (22.3, 3.4), (21.75, 3.5), (21.1, 3.0)])), fill="#6a6668", op=0.4)
    D.path(smooth_d(Ls([(22.5, 3.4), (22.72, 4.0), (22.4, 4.95), (21.75, 5.85), (21.1, 6.35), (21.05, 5.85),
                        (21.45, 5.1), (21.8, 4.3), (21.95, 3.6)])), fill="#070606")
    D.path(curve_d(Ls([(22.68, 3.2), (22.92, 4.1), (22.6, 5.05)])), stroke="#6a6567", sw=0.24, sop=0.6)
    D.path(curve_d(Ls([(22.55, 6.35), (22.45, 6.95), (22.2, 7.4)])), stroke="#050404", sw=0.2, sop=0.8)
    D.path(curve_d(Ls(MOUTH)), stroke="#050404", sw=0.3, sop=0.9)
    # Liseré blanc sous la mâchoire, jusqu'à l'angle (il rejoint la gorge blanche).
    D.path(curve_d(Ls([(4.4, 12.8), (8.0, 12.67), (11.6, 12.22), (13.8, 12.07)])), stroke="#e6e5de", sw=0.42,
           sop=0.8)
    # Poil ras de la face, couché vers l'arrière.
    face = poly(out).difference(bpoly.buffer(0.2)).difference(Point(E).buffer(2.6)).difference(
        poly(Ls(NOSE)).buffer(0.3))
    hs = []
    for p in poisson(face.buffer(-0.3), 0.7, rng):
        a = TH + math.pi + rng.uniform(-0.35, 0.15)
        Lh = rng.uniform(0.5, 0.9)
        hs.append(poly_d([p, (p[0] + Lh * math.cos(a), p[1] + Lh * math.sin(a))], closed=False))
    D.path("".join(hs), stroke="#403b3d", sw=0.13, sop=0.45)
    # La laine blanche entre la corne du haut et celle du bas : de la laine entre
    # les deux paires (cornes non soudées), qui recouvre l'arrière de la base.
    tuft_patch(up([(130.9, 113.4), (130.8, 116.0), (130.4, 118.4), (129.5, 120.6), (128.3, 121.7), (127.0, 121.3),
                   (128.1, 119.7), (128.5, 117.1), (127.9, 114.9), (128.9, 113.0), (130.0, 112.6)]), 19, rng)
    ear("ear-near", EAR_BASE, math.radians(174), 6.6, 4.4, comment="Oreille noire, presque à l'horizontale, dans la "
                                                                     "boucle de la corne du bas")
    # Corne du bas, de notre côté : en arrière par-dessus l'oreille, puis vers le bas
    # et vers l'avant, la pointe dégagée de la joue et de la gorge.
    horn(cr_sample(LOWER_NEAR, False, 0.25), lambda f: 0.14 + 2.25 * (1 - f) ** 0.8, lit_side=1, seed=41,
         root_cap=True, tip_smooth=0.14, spacing=0.85)
    r0 = LOWER_NEAR[0]
    m = Marks()
    for i in range(6):
        a = math.radians(195 + 27 * i)
        p = (r0[0] + 2.45 * math.cos(a), r0[1] + 2.45 * math.sin(a))
        m.add(("#e7e2d6", 0.26, 0.9), arc1(p, 0.45, a + 0.6, a + 2.8))
    m.emit(D)
    D.pivot("head", HEAD_PIVOT)
    D.close()


# =============================================================== Articulations
def articule(svg):
    """Le dessin coupé à ses articulations (voir tools/races/articulations.py) :
    chaque patte au carpe ou au jarret (les taches noires du genou et du
    jarret) ; l'encolure, l'avant noir du haut de la toison, devant une
    épaule arrondie qui descend du garrot vers le poitrail, posée sous le
    corps ; la tête tourne à la nuque, sous les cornes ; la lèvre
    supérieure, détachée de la tête, avec la gencive qu'elle cache."""
    def lip(root, paints):
        articulations.lift_part(root, paints, "head", Ls(LIP), "lip", L(*LIP_PIVOT),
                                under=articulations.gum(Ls(GUM), Ls(GUM_RIDGE), GUM_FILL, GUM_CREASE, 0.25))

    return articulations.articulate_breed(
        svg,
        carpus_y=181.3,
        hock_y=178.4,
        neck=dict(
            source="body",
            area=articulations.shoulder((100.0, 123.5), (131.5, 142.0), bulge=5.0, far=200.0, top=40.0),
            pivot=(108.0, 138.0),
            poll=(119.0, 113.0),
            muzzle=(144.5, 135.0),
            margin=6.0,
            filler=(1.0, 9.0),
        ),
        extra=lip,
    )


# =============================================================== Document
def main():
    outline, body, black, white = build_body()
    print(f"couleur sur la laine du corps (ce côté) : {black.area / body.area * 100:.1f} %")
    draw_leg("leg-back-far", "hind", 6.0, True, False, "Pattes : chacune d'une pièce, sous la toison ; celles de "
                                                       "l'autre côté d'abord")
    draw_leg("leg-front-far", "front", -6.0, True, True, None)
    draw_leg("leg-back-near", "hind", 0.0, False, True, None)
    draw_leg("leg-front-near", "front", 0.0, False, False, None)
    draw_tail()
    draw_body(outline, body, black, white)
    draw_head()
    D.pivot("ground", (80.5, GY))
    head = ('<svg xmlns="http://www.w3.org/2000/svg" viewBox="14 66 150 140" width="150" height="140">\n'
            "  <!--\n"
            "    Jacob : bélier de Jacob à quatre cornes, pie noir et blanc, vu de profil.\n"
            "    D'après la fiche de la race (standards de la JSBA et de la Jacob Sheep Society).\n"
            "    Unités : le centimètre de l'animal (75 cm au garrot), sol à y = 200.\n"
            "    © 2026 Alejandro San Martin Lamas — MIT OR Apache-2.0\n"
            "  -->\n")
    svg = head + "<defs>" + "".join(D.defs) + "</defs>\n" + "\n".join(D.body) + "\n</svg>\n"
    svg = articule(svg)
    if VARIANTE:
        return variante(OUT, svg)
    with open(OUT, "w") as fh:
        fh.write(svg)
    print(f"{OUT} : {len(svg) / 1024:.0f} Ko")


def variante(path, svg):
    """La brebis ou l'agneau : ce qui change du bélier, puis ses proportions."""
    root = ET.fromstring(svg)
    if VARIANTE == "brebis":
        # Petite mamelle sans laine, près du corps (§ 6.10).
        variantes.mamelle(root, *variantes.sous_le_ventre(root, 10, -2), largeur=8.5, hauteur=5.5)
    variantes.ecrire(path, root, **PROPORTIONS[VARIANTE])


main()
