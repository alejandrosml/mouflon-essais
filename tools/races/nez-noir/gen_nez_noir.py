#!/usr/bin/env python3
"""Génère le rig SVG de la Nez noir du Valais : une brebis adulte en pleine
toison, de profil, tournée vers la droite.

Repère de travail : le « monde », en centimètres de l'animal réel. x vers
l'avant (0 à l'arrière de la croupe), h vers le haut (0 au sol). Dans le SVG,
1 unité = 1 cm : X = x + OX, Y = OY - h.

Usage : gen_nez_noir.py <sortie.svg>
"""
import math
import os
import random
import sys
import xml.etree.ElementTree as ET

sys.path.insert(0, os.path.join(os.path.dirname(os.path.abspath(__file__)), ".."))
import articulations  # noqa: E402  (tools/races/articulations.py)
import variantes  # noqa: E402

# Les autres dessins de la race (tools/races/variantes.py), d'après la fiche.
# Sans variante : la brebis.
VARIANTE = sys.argv[2] if len(sys.argv) > 2 else ""
PROPORTIONS = {
    "belier": dict(  # Bélier Nez noir du Valais : 80 cm au garrot, spirale de corne plus large et plus forte, pas de tache noire sous la queue.
        garrot=(75, 80), encolure=(1.0, 1.12), tete=(1.05, 1.06)),
    "agneau": dict(  # Agneau Nez noir du Valais de deux semaines : 45 cm au garrot, marques noires déjà en place, boucles serrées, sans cornes.
        variantes.AGNEAU, garrot=(75, 45), corps=(.84, .8)),
}

OX, OY = 7.0, 112.0
rng = random.Random(1962)  # l'année où la race fut reconnue


# ------------------------------------------------------------------ outils

def S(x, h):
    return (x + OX, OY - h)


def num(v, nd=2):
    s = f"{v:.{nd}f}"
    if "." in s:
        s = s.rstrip("0").rstrip(".")
    if s.startswith("0."):
        s = s[1:]
    elif s.startswith("-0."):
        s = "-" + s[2:]
    if s in ("-0", "", "-"):
        s = "0"
    return s


def nums(vals, nd):
    """Nombres à la suite, sans séparateur inutile ("1.2-3.4.5" : 1.2, -3.4, .5)."""
    out = []
    prev = ""
    for v in vals:
        t = num(v, nd)
        if out and not t.startswith("-") and not (t.startswith(".") and "." in prev):
            out.append(" ")
        out.append(t)
        prev = t
    return "".join(out)


def _ctrl(P, i, n, closed, t):
    if closed:
        p0, p1, p2, p3 = P[(i - 1) % n], P[i % n], P[(i + 1) % n], P[(i + 2) % n]
    else:
        p0 = P[max(i - 1, 0)]
        p1 = P[i]
        p2 = P[i + 1]
        p3 = P[min(i + 2, n - 1)]
    c1 = (p1[0] + (p2[0] - p0[0]) * t / 6.0, p1[1] + (p2[1] - p0[1]) * t / 6.0)
    c2 = (p2[0] - (p3[0] - p1[0]) * t / 6.0, p2[1] - (p3[1] - p1[1]) * t / 6.0)
    return p1, c1, c2, p2


def cr(pts, closed=True, t=1.0, nd=2):
    """Courbe lisse (Catmull-Rom en Bézier) par des points du monde ; tracé
    relatif. Le premier point de contrôle d'un segment est le reflet du
    dernier du précédent : les segments suivants s'écrivent avec `s`."""
    P = [S(x, h) for (x, h) in pts]
    n = len(P)
    q = 10 ** nd

    def r(v):
        return round(v * q) / q

    cur = (r(P[0][0]), r(P[0][1]))
    out = [f"M{nums(cur, nd)}"]
    segs = n if closed else n - 1
    for i in range(segs):
        _, c1, c2, p2 = _ctrl(P, i, n, closed, t)
        if i == 0:
            d = [r(c1[0] - cur[0]), r(c1[1] - cur[1]), r(c2[0] - cur[0]), r(c2[1] - cur[1]),
                 r(p2[0] - cur[0]), r(p2[1] - cur[1])]
            out.append("c" + nums(d, nd))
            end = (d[4], d[5])
        else:
            d = [r(c2[0] - cur[0]), r(c2[1] - cur[1]), r(p2[0] - cur[0]), r(p2[1] - cur[1])]
            out.append("s" + nums(d, nd))
            end = (d[2], d[3])
        cur = (cur[0] + end[0], cur[1] + end[1])
    if closed:
        out.append("z")
    return "".join(out)


def runs_path(runs, nd=2, t=1.0):
    """Contour fermé fait de plusieurs suites lisses jointes par des angles :
    chaque suite est une liste de points ; la dernière d'une suite est la
    première de la suivante."""
    q = 10 ** nd

    def r(v):
        return round(v * q) / q

    first = S(*runs[0][0])
    cur = (r(first[0]), r(first[1]))
    out = [f"M{num(cur[0], nd)} {num(cur[1], nd)}"]
    for run in runs:
        P = [S(x, h) for (x, h) in run]
        n = len(P)
        if n == 2:
            d = [r(P[1][0] - cur[0]), r(P[1][1] - cur[1])]
            out.append("l" + nums(d, nd))
            cur = (cur[0] + d[0], cur[1] + d[1])
            continue
        for i in range(n - 1):
            _, c1, c2, p2 = _ctrl(P, i, n, False, t)
            d = [r(c1[0] - cur[0]), r(c1[1] - cur[1]), r(c2[0] - cur[0]), r(c2[1] - cur[1]),
                 r(p2[0] - cur[0]), r(p2[1] - cur[1])]
            out.append("c" + nums(d, nd))
            cur = (cur[0] + d[4], cur[1] + d[5])
    out.append("z")
    return "".join(out)


def poly(pts, closed=True, nd=2):
    P = [S(x, h) for (x, h) in pts]
    out = [f"M{num(P[0][0], nd)} {num(P[0][1], nd)}"]
    for p in P[1:]:
        out.append(f"L{num(p[0], nd)} {num(p[1], nd)}")
    if closed:
        out.append("z")
    return "".join(out)


def bez(p0, c1, c2, p3, u):
    a = (1 - u) ** 3
    b = 3 * (1 - u) ** 2 * u
    c = 3 * (1 - u) * u * u
    d = u ** 3
    return (a * p0[0] + b * c1[0] + c * c2[0] + d * p3[0], a * p0[1] + b * c1[1] + c * c2[1] + d * p3[1])


def dense(pts, closed=True, per=10, t=1.0):
    n = len(pts)
    segs = n if closed else n - 1
    out = []
    for i in range(segs):
        p1, c1, c2, p2 = _ctrl(pts, i, n, closed, t)
        for k in range(per):
            out.append(bez(p1, c1, c2, p2, k / per))
    if not closed:
        out.append(pts[-1])
    return out


def vspan(polyl, x):
    hs = []
    n = len(polyl)
    for i in range(n):
        (x1, h1), (x2, h2) = polyl[i], polyl[(i + 1) % n]
        if (x1 <= x < x2) or (x2 <= x < x1):
            u = (x - x1) / (x2 - x1)
            hs.append(h1 + u * (h2 - h1))
    if len(hs) < 2:
        return None
    return (min(hs), max(hs))


def inside(polyl, x, h):
    c = False
    n = len(polyl)
    for i in range(n):
        (x1, h1), (x2, h2) = polyl[i], polyl[(i + 1) % n]
        if (h1 > h) != (h2 > h):
            xi = x1 + (h - h1) / (h2 - h1) * (x2 - x1)
            if x < xi:
                c = not c
    return c


def rgb(hx):
    hx = hx.lstrip("#")
    return tuple(int(hx[i:i + 2], 16) for i in (0, 2, 4))


def hexc(c):
    return "#%02X%02X%02X" % tuple(max(0, min(255, int(round(v)))) for v in c)


def mix(a, b, t):
    A, B = rgb(a), rgb(b)
    t = max(0.0, min(1.0, t))
    return hexc(tuple(A[i] + (B[i] - A[i]) * t for i in range(3)))


def shade(a, k):
    A = rgb(a)
    return hexc(tuple(v * k for v in A))


OUT = []


def emit(s):
    OUT.append(s)


def el(d, fill=None, stroke=None, sw=None, fo=None, so=None, cap=None, join=None, rule=None):
    a = [f'<path d="{d}" fill="{fill or "none"}"']
    if fo is not None and fo < 0.999:
        a.append(f' fill-opacity="{num(fo, 3)}"')
    if rule:
        a.append(f' fill-rule="{rule}"')
    if stroke:
        a.append(f' stroke="{stroke}" stroke-width="{num(sw, 3)}"')
        if so is not None and so < 0.999:
            a.append(f' stroke-opacity="{num(so, 3)}"')
        if cap:
            a.append(f' stroke-linecap="{cap}"')
        if join:
            a.append(f' stroke-linejoin="{join}"')
    a.append("/>")
    emit("".join(a))


def pivot(name, x, h):
    X, Y = S(x, h)
    emit(f'<circle id="pivot-{name}" cx="{num(X)}" cy="{num(Y)}" r="0.8"/>')


def ellipse_pts(cx, ch, rx, rh, rot=0.0, n=16):
    c, s = math.cos(rot), math.sin(rot)
    out = []
    for k in range(n):
        a = 2 * math.pi * k / n
        ex, eh = rx * math.cos(a), rh * math.sin(a)
        out.append((cx + ex * c - eh * s, ch + ex * s + eh * c))
    return out


# ------------------------------------------------------------------ palette (fiche, § 4.9)

W_LIGHT = "#F8F4EA"
W_BASE = "#EEE7D8"
W_SHADOW = "#CFC2A6"
W_DEEP = "#9C8C70"
W_TIP = "#D9C8A6"      # pointes un peu beiges (lanoline, poussière d'alpage)
B_BASE = "#161414"     # laine noire
B_SHEEN = "#3B3633"
B_DEEP = "#0B0A0A"
B_WARM = "#2E241D"
NOSE = "#24262A"
NOSE_SHEEN = "#55585E"
NOSTRIL = "#0F1012"
HORN = "#CDB693"
HORN_LIGHT = "#E6D9C0"
HORN_SHADOW = "#9A7D5C"
HORN_STREAK = "#4F3D2E"
HOOF = "#1B1918"
HOOF_LIGHT = "#3F3C3A"
HOOF_DEEP = "#0E0D0C"
IRIS = "#9A7A4C"
IRIS_LIGHT = "#C49A5A"
LIMBUS = "#3C3127"
PUPIL = "#0D0C0B"
CATCH = "#E8E4DC"
INNER_EAR = "#2A2422"
EAR_GLOW = "#5C4232"


# ------------------------------------------------------------------ mèches

def lock_pts(x0, h0, L, w0, ang=0.0, waves=1.6, amp=0.4, phase=0.0, curl=1.0,
             turn=3.0, clen=2.4, wtip=0.22, step=1.2, cap=0.35):
    """Contour d'une mèche ondulée qui pend de (x0, h0), angle `ang` compté depuis
    la verticale (positif : vers l'avant). La pointe s'enroule (`curl` : +1
    vers l'avant, -1 vers l'arrière)."""
    n = max(3, int(round(L / step)))
    ds = L / n
    x, h = x0, h0
    C = []
    for i in range(n + 1):
        s = i * ds
        fr = s / L
        th = ang + amp * math.sin(2 * math.pi * waves * fr + phase) * min(1.0, fr * 4.0)
        if s > L - clen and clen > 0:
            u = (s - (L - clen)) / clen
            th += curl * turn * u ** 1.7
        C.append((x, h, th, fr))
        if i < n:
            x += ds * math.sin(th)
            h -= ds * math.cos(th)
    left, right = [], []
    for (cx, ch, th, fr) in C:
        w = w0 * (1.0 - 0.45 * fr ** 1.2)
        if fr > 0.78:
            k = (fr - 0.78) / 0.22
            w = w + (wtip - w) * k ** 1.3
        nx, nh = math.cos(th), math.sin(th)
        left.append((cx + nx * w / 2, ch + nh * w / 2))
        right.append((cx - nx * w / 2, ch - nh * w / 2))
    xe, he, the, _ = C[-1]
    tip = (xe + math.sin(the) * wtip * 0.8, he - math.cos(the) * wtip * 0.8)
    pts = left + [tip] + right[::-1]
    if cap > 0:
        th0 = C[0][2]
        pts.append((x0 - math.sin(th0) * cap, h0 + math.cos(th0) * cap))
    return pts


def curl_stroke_pts(cx, ch, r, a0, sweep, n=6):
    """Petit arc (boucle de laine) autour de (cx, ch)."""
    return [(cx + r * math.cos(a0 + sweep * k / (n - 1)), ch + r * math.sin(a0 + sweep * k / (n - 1)))
            for k in range(n)]


# ------------------------------------------------------------------ défs

DEFS = []


def lin(id_, x1, h1, x2, h2, stops):
    X1, Y1 = S(x1, h1)
    X2, Y2 = S(x2, h2)
    s = "".join(f'<stop offset="{num(o, 3)}" stop-color="{c}"' + (f' stop-opacity="{num(a, 3)}"' if a < 1 else "") + "/>"
                for (o, c, a) in stops)
    DEFS.append(f'<linearGradient id="{id_}" gradientUnits="userSpaceOnUse" x1="{num(X1)}" y1="{num(Y1)}" x2="{num(X2)}" y2="{num(Y2)}">{s}</linearGradient>')


def rad(id_, cx, ch, r, stops, fx=None, fh=None):
    X, Y = S(cx, ch)
    f = ""
    if fx is not None:
        FX, FY = S(fx, fh)
        f = f' fx="{num(FX)}" fy="{num(FY)}"'
    s = "".join(f'<stop offset="{num(o, 3)}" stop-color="{c}"' + (f' stop-opacity="{num(a, 3)}"' if a < 1 else "") + "/>"
                for (o, c, a) in stops)
    DEFS.append(f'<radialGradient id="{id_}" gradientUnits="userSpaceOnUse" cx="{num(X)}" cy="{num(Y)}" r="{num(r)}"{f}>{s}</radialGradient>')



# ------------------------------------------------------------------ mèches et boucles

LOCK_EDGE = "#BFAE8E"


def open_group(stroke=LOCK_EDGE, sw=0.12, so=0.45):
    """Un groupe sans nom (pas une partie) qui donne le même liseré à toutes
    ses mèches : le fichier en est bien plus léger."""
    emit(f'<g stroke="{stroke}" stroke-width="{num(sw, 3)}" stroke-opacity="{num(so, 3)}">')


def close_group():
    emit("</g>")


def lock(x, h, L, w, ang, color, turn=None, step=1.3, waves=None, amp=None, clen=None, curl=None):
    pts = lock_pts(x, h, L, w, ang, waves=waves if waves is not None else L / 10.0 * rng.uniform(1.3, 1.8),
                   amp=amp if amp is not None else rng.uniform(0.3, 0.45), phase=rng.uniform(0, 6.28),
                   curl=curl if curl is not None else rng.choice((-1, 1)),
                   turn=turn if turn is not None else rng.uniform(2.6, 4.0),
                   clen=clen if clen is not None else min(2.6, 0.4 * L), step=step)
    el(cr(pts, nd=1), fill=color)


def black_curl(x, h, r, sheen=B_SHEEN, deep=None, op=0.75, sw=0.15):
    a0 = rng.uniform(0, 6.28)
    sweep = rng.choice((-1, 1)) * rng.uniform(2.6, 3.8)
    if deep:
        el(cr(curl_stroke_pts(x + 0.06, h - 0.06, r, a0, sweep, 5), closed=False, nd=1), stroke=deep, sw=sw + 0.05,
           so=0.8, cap="round")
    el(cr(curl_stroke_pts(x, h, r, a0, sweep, 5), closed=False), stroke=sheen, sw=sw, so=op, cap="round")


def scatter(poly_pts, n, min_d, ok=None):
    """Des points au hasard dans un polygone, écartés d'au moins `min_d`."""
    P = dense(poly_pts, True, 5)
    xs = [p[0] for p in P]
    hs = [p[1] for p in P]
    out = []
    tries = 0
    while len(out) < n and tries < n * 120:
        tries += 1
        x = rng.uniform(min(xs), max(xs))
        h = rng.uniform(min(hs), max(hs))
        if not inside(P, x, h):
            continue
        if ok and not ok(x, h):
            continue
        if any((x - a) ** 2 + (h - b) ** 2 < min_d * min_d for (a, b) in out):
            continue
        out.append((x, h))
    return out


# ================================================================== le corps

# Un grand tonneau de laine : dos légèrement voûté, croupe et poitrail ronds.
BODY = [
    (9.0, 81.0), (14.0, 84.6), (20.0, 87.1), (30.0, 88.8), (42.0, 89.8), (55.0, 90.2), (68.0, 90.0),
    (79.0, 89.6), (87.0, 89.8), (93.0, 90.7), (98.0, 91.9), (103.0, 92.9), (107.0, 93.2), (110.5, 92.5),
    (114.0, 90.4), (117.2, 86.8), (119.5, 81.6), (120.8, 75.0), (121.3, 67.5), (120.9, 60.0), (119.4, 52.6),
    (116.8, 45.8), (113.2, 39.6), (108.8, 34.4), (104.0, 30.8), (98.0, 28.8), (91.0, 27.6), (82.0, 26.2),
    (72.0, 25.2), (62.0, 24.7), (52.0, 24.9), (42.0, 25.8), (33.0, 27.2), (26.0, 28.3), (19.0, 29.0),
    (13.5, 29.6), (9.0, 31.4), (5.4, 34.6), (2.8, 39.5), (1.2, 45.5), (0.5, 52.0), (0.4, 58.5), (1.0, 65.0),
    (2.6, 71.2), (5.2, 76.6),
]
BODY_D = dense(BODY, True, 12)

FORE_NEAR, FORE_FAR = 101.0, 90.5
HIND_NEAR, HIND_FAR = 18.5, 29.5


# La frange du ventre, entre les pattes : le bout des mèches du bas de chaque
# colonne, sous l'ourlet, balance (trois mèches, `sway-1` à l'avant), de x0 à
# x1 (monde) ; le haut de ces mèches reste à sa place (voir `articule`).
FRINGE = [(68.0, 84.0), (52.0, 68.0), (36.0, 52.0)]
# Trois petites touffes du cou (x0, x1, hauteur de coupe). Leur racine
# déborde sous la laine courte : le moteur suit ce point d'attache, mais
# garde les fibres pendantes quand l'encolure descend.
NECK_FRINGE = [(108.0, 113.0, 82.0), (113.0, 118.0, 80.0), (118.0, 122.0, 76.0)]


def fringe_group(x):
    for k, (x0, x1) in enumerate(FRINGE):
        if x0 <= x < x1:
            return k + 1
    return None


def fringe_cut(k):
    """Le milieu de la ligne où la `k`-ième mèche de la frange est coupée
    (monde) : au bas du fond de la toison, 2,4 cm au-dessus de l'ourlet ;
    plus bas, il n'y a que le bout des mèches du bas de chaque colonne."""
    x0, x1 = FRINGE[k - 1]
    xs = [x0 + (x1 - x0) * i / 8 for i in range(9)]
    hb = sum(vspan(BODY_D, x)[0] for x in xs) / len(xs)
    return (x0 + x1) / 2, hb + 2.4


def fringe(x):
    """Longueur de la frange sous l'ourlet (cm) : des touffes qui pendent
    plus bas que leurs voisines ; moins devant les genoux et les jarrets,
    pour que leurs taches noires restent bien visibles."""
    clump = max(0.0, math.sin(2 * math.pi * x / 6.3 + 0.7)) ** 2
    m = 1.0 + 4.2 * clump + rng.uniform(0.0, 1.0)
    for c in (FORE_NEAR, FORE_FAR):
        if abs(x - c) < 8.5:
            m = min(m, 1.4)
    for c in (HIND_NEAR, HIND_FAR):
        if abs(x - c) < 10:
            m = min(m, 1.2)
    return m


def wool_color(x, h, jitter):
    # Lumière d'en haut à gauche : le dos et la croupe plus clairs, le ventre
    # et l'avant plus sombres.
    t = (h - 26.0) / 62.0
    light = 0.42 + 0.55 * t - 0.14 * (x / 120.0) + jitter
    light = max(0.0, min(1.0, light))
    if light > 0.5:
        return mix(W_BASE, W_LIGHT, (light - 0.5) * 2)
    return mix("#DED3BC", W_BASE, light * 2)


def body():
    lin("toison", 0, 92, 0, 22, [(0, "#E9E1CF", 1), (0.5, "#DACFB6", 1), (1, "#BCAB8A", 1)])
    emit('<g id="body">')
    # Le fond (les creux entre les mèches), un peu en retrait de l'ourlet,
    # de la culotte et du poitrail : ce sont les pointes des mèches qui
    # dessinent le bord, effiloché.
    fond = []
    for (x, h) in BODY:
        if h < 37:
            h += 2.4
        if x < 8 and 36 < h < 80:
            x += 1.1
        if x > 112 and 32 < h < 78:
            x -= 1.1
        fond.append((x, h))
    el(cr(fond, nd=2), fill="url(#toison)")

    locks = []
    x = 0.7
    while x < 121.8:
        sp = vspan(BODY_D, x)
        if sp:
            hb, ht = sp
            root = ht + rng.uniform(-1.3, 0.45)
            while True:
                L = rng.uniform(9.0, 17.0)
                last = root - L <= hb + 2.5
                if last:
                    L = (root - hb) + fringe(x)
                slant = rng.uniform(-0.06, 0.06)
                if x < 6:
                    slant -= 0.14
                if x > 115:
                    slant += 0.12
                locks.append(dict(x=x + rng.uniform(-0.3, 0.3), h=root, L=L, w=rng.uniform(1.7, 2.3), ang=slant,
                                  last=last))
                if last:
                    break
                root = root - L * rng.uniform(0.55, 0.74)
        x += rng.uniform(1.55, 2.0)

    locks.sort(key=lambda d: (d["h"], d["x"]))
    open_group()
    for d in locks:
        c = wool_color(d["x"], d["h"] - d["L"] * 0.5, rng.uniform(-0.07, 0.07))
        if d["last"]:
            c = mix(c, W_TIP, 0.18)
        start = len(OUT)
        lock(d["x"], d["h"], d["L"], d["w"], d["ang"], c,
             turn=rng.uniform(2.8, 4.2) if d["last"] else rng.uniform(1.4, 3.2),
             step=1.45 if d["last"] else 2.05)
        for k, (x0, x1, _) in enumerate(NECK_FRINGE, 1):
            if x0 <= d["x"] < x1:
                for i in range(start, len(OUT)):
                    OUT[i] = OUT[i].replace("<path ", f'<path class="neck-fringe-{k}" ', 1)
        # Les mèches de la frange, marquées pour `articule`.
        k = fringe_group(d["x"]) if d["last"] else None
        if k:
            for i in range(start, len(OUT)):
                OUT[i] = OUT[i].replace("<path ", f'<path class="frange-{k}" ', 1)

    # Mèches du bord : la silhouette effilochée de la culotte et du poitrail,
    # en touffes.
    edge = []
    rear = [p for p in BODY_D if p[0] < 9.5 and 34 < p[1] < 80]
    front = [p for p in BODY_D if p[0] > 108 and 32 < p[1] < 76]
    for group, side in ((rear, -1), (front, 1)):
        for (ex, eh) in group[::2]:
            if rng.random() < 0.25:
                continue
            ang = side * rng.uniform(0.2, 0.55)
            L = rng.uniform(3.4, 6.4)
            edge.append((eh, ex - side * 1.0, eh + 1.3, L, rng.uniform(1.3, 1.8), ang,
                         wool_color(ex, eh, rng.uniform(-0.05, 0.05))))
    edge.sort(key=lambda e: e[0])
    for (_, x0, h0, L, w, ang, c) in edge:
        start = len(OUT)
        lock(x0, h0, L, w, ang, c, turn=rng.uniform(2.4, 3.8), step=1.1, waves=rng.uniform(0.6, 1.0), amp=0.35,
             clen=1.7)
        if x0 > 118 and h0 > 62:
            # Les pointes du bord du cou pendent aussi : sans elles,
            # une frange se coucherait encore devant les antérieurs.
            for i in range(start, len(OUT)):
                OUT[i] = OUT[i].replace("<path ", '<path class="neck-fringe-3" ', 1)
    close_group()

    # Frisure du dessus de la toison.
    x = 6.0
    while x < 100:
        sp = vspan(BODY_D, x)
        if sp:
            ht = sp[1]
            el(cr(curl_stroke_pts(x + rng.uniform(-0.4, 0.4), ht - rng.uniform(1.0, 4.0), rng.uniform(0.45, 0.75),
                                  rng.uniform(0, 6.28), rng.uniform(3.0, 4.2)), closed=False, nd=1),
               stroke="#CDBFA2", sw=0.2, so=0.6, cap="round")
        x += rng.uniform(2.6, 3.8)

    # Quelques boucles qui se dressent sur le dos, et les jarres (poils
    # grossiers) qui s'échappent de la toison : un fin halo.
    open_group()
    x = 9.0
    while x < 104:
        sp = vspan(BODY_D, x)
        if sp:
            lock(x, sp[1] - 0.6, rng.uniform(2.6, 3.8), rng.uniform(1.4, 1.8), rng.uniform(-0.7, 0.7),
                 wool_color(x, sp[1], 0.12), turn=rng.uniform(3.2, 4.4), step=0.8, waves=0.7, amp=0.4)
        x += rng.uniform(5.0, 9.0)
    close_group()
    emit('<g fill="none" stroke="#F6F2E8" stroke-width=".07" stroke-opacity=".7" stroke-linecap="round">')
    n = len(BODY_D)
    for i in range(0, n, 3):
        x, h = BODY_D[i]
        if h < 40 or rng.random() < 0.45:
            continue
        a, b = BODY_D[(i - 2) % n], BODY_D[(i + 2) % n]
        tx, th = b[0] - a[0], b[1] - a[1]
        L = math.hypot(tx, th) or 1.0
        nx, nh = -th / L, tx / L          # vers l'extérieur (contour parcouru dans le sens horaire)
        ln = rng.uniform(1.4, 3.0)
        bend = rng.uniform(-0.6, 0.6)
        pts = [(x - nx * 0.6, h - nh * 0.6), (x + nx * ln * 0.5 - nh * bend * 0.3, h + nh * ln * 0.5 + nx * bend * 0.3),
               (x + nx * ln - nh * bend, h + nh * ln + nx * bend)]
        emit(f'<path d="{cr(pts, closed=False, nd=1)}"/>')
    emit("</g>")

    # La tache noire sous la queue (une brebis) : la queue la cache presque
    # toute ; un mince croissant noir entre la queue et la fesse.
    if VARIANTE != "belier":
        el(cr(ellipse_pts(7.6, 68.5, 2.2, 4.6, 0.1, 14)), fill=B_BASE)
    for k in range(3 if VARIANTE != "belier" else 0):
        black_curl(8.3 + rng.uniform(-0.6, 0.8), 68.5 + rng.uniform(-3.0, 3.0), 0.4)
    pivot("body", 60.0, 25.0)
    emit("</g>")


# ================================================================== les pattes

def edge_locks(edge_pts, side, near, skip=None, every=2.0, inset=0.9):
    """Des mèches qui pendent le long d'un bord de patte, pointes vers le bas
    et un peu vers l'extérieur (`side` : +1 devant, -1 derrière)."""
    D = dense(edge_pts, closed=False, per=10)
    acc = [0.0]
    for i in range(1, len(D)):
        acc.append(acc[-1] + math.hypot(D[i][0] - D[i - 1][0], D[i][1] - D[i - 1][1]))
    s = rng.uniform(0, every * 0.5)
    out = []
    while s < acc[-1]:
        i = next(k for k in range(len(acc)) if acc[k] >= s)
        x, h = D[i]
        if not (skip and skip[0] < h < skip[1]):
            out.append((x, h))
        s += every * rng.uniform(0.8, 1.15)
    for (x, h) in out:
        if near:
            c = mix("#D4C7AB", "#EEE6D6", rng.uniform(0.2, 1.0) * min(1.0, (h - 8) / 22))
        else:
            c = mix("#A4957A", "#C2B497", rng.uniform(0.2, 1.0))
        lock(x - side * inset, h + 1.6, rng.uniform(3.2, 4.6), rng.uniform(1.3, 1.7), side * rng.uniform(0.12, 0.32),
             c, turn=rng.uniform(2.2, 3.4), step=0.9, waves=0.8, amp=0.3, clen=1.4)


def boot_and_hoof(cx, far):
    """Botte noire (boulet, paturon incliné) et onglons noirs fendus."""
    b_fill = "#110F0F" if far else B_BASE
    sheen = "#262220" if far else B_SHEEN
    front = [(cx + 3.6, 10.9), (cx + 3.62, 9.2), (cx + 3.8, 7.6), (cx + 4.3, 5.9), (cx + 4.9, 4.5), (cx + 5.25, 3.45)]
    coronet = [(cx + 5.25, 3.45), (cx - 0.35, 3.0)]
    rear = [(cx - 0.35, 3.0), (cx - 1.4, 3.85), (cx - 2.6, 5.0), (cx - 3.55, 6.35), (cx - 3.9, 7.8), (cx - 3.65, 9.3),
            (cx - 3.45, 10.9)]
    top = [(cx - 3.45, 10.9), (cx - 2.4, 11.2), (cx - 1.2, 10.95), (cx + 0.1, 11.25), (cx + 1.3, 10.95),
           (cx + 2.5, 11.2), (cx + 3.6, 10.9)]
    el(runs_path([front, coronet, rear, top], nd=2), fill=b_fill)
    for k in range(5):
        black_curl(cx + rng.uniform(-2.8, 2.9), rng.uniform(6.4, 10.2), rng.uniform(0.32, 0.5), sheen=sheen, op=0.7)
    el(cr(ellipse_pts(cx - 3.75, 6.7, 0.5, 0.38, 0.6, 8)), fill="#0A0909")   # l'ergot
    gid = f"onglon{int(round(cx * 10))}"
    lin(gid, cx - 1.0, 1.5, cx + 6.2, 1.5,
        [(0, HOOF_DEEP, 1), (0.5, HOOF, 1), (0.88, HOOF_LIGHT if not far else "#2A2726", 1), (1, HOOF, 1)])
    wall = [(cx + 5.15, 3.5), (cx + 5.6, 2.35), (cx + 5.95, 1.15), (cx + 6.15, 0.35), (cx + 6.0, 0.0)]
    sole = [(cx + 6.0, 0.0), (cx - 0.15, 0.0)]
    heel = [(cx - 0.15, 0.0), (cx - 0.75, 0.35), (cx - 0.95, 1.2), (cx - 0.75, 2.2), (cx - 0.3, 2.95),
            (cx + 2.4, 3.6), (cx + 5.15, 3.5)]
    el(runs_path([wall, sole, heel], nd=2), fill=f"url(#{gid})")
    el(cr([(cx + 3.3, 3.45), (cx + 3.55, 1.7), (cx + 4.0, 0.08)], closed=False),
       stroke="#4E4A47" if not far else "#2C2928", sw=0.18, so=0.9, cap="round")
    el(cr([(cx + 4.6, 3.3), (cx + 5.25, 1.8), (cx + 5.6, 0.4)], closed=False),
       stroke="#6A6562" if not far else "#383432", sw=0.14, so=0.55, cap="round")
    el(cr([(cx - 0.2, 2.95), (cx + 2.4, 3.55), (cx + 5.1, 3.45)], closed=False),
       stroke="#45413E" if not far else "#262321", sw=0.16, so=0.8, cap="round")


def white_over_boot(cx, near):
    """La laine blanche du canon qui retombe un peu sur le haut de la botte."""
    for k in range(5):
        x0 = cx - 3.0 + k * 1.55 + rng.uniform(-0.2, 0.2)
        c = mix("#E2D8C4", "#D2C5A9", rng.uniform(0, 1)) if near else mix("#B4A68A", "#A29377", rng.uniform(0, 1))
        pts = lock_pts(x0, 13.4, rng.uniform(2.3, 3.0), rng.uniform(1.25, 1.6), rng.uniform(-0.1, 0.1), waves=0.6,
                       amp=0.25, phase=rng.uniform(0, 6.28), curl=rng.choice((-1, 1)), turn=1.6, clen=0.9, step=0.7)
        el(cr(pts, nd=2), fill=c)


def spot_edge_curls(pts, near, n=7):
    for k in range(n):
        x, h = pts[int(k * (len(pts) - 1) / max(1, n - 1))]
        black_curl(x, h, rng.uniform(0.35, 0.5), sheen=B_SHEEN if near else "#262220")


def leg_locks(cx, near, x_rng, h_rng, n=5):
    for k in range(n):
        c = mix("#E8DFCC", "#D6C9AE", rng.uniform(0, 1)) if near else mix("#BDAF92", "#A99A7D", rng.uniform(0, 1))
        lock(cx + rng.uniform(*x_rng), rng.uniform(*h_rng), rng.uniform(4.0, 6.0), rng.uniform(1.0, 1.4),
             rng.uniform(-0.05, 0.05), c, turn=2.0, step=1.0, waves=0.9, amp=0.3, clen=1.2)


def foreleg(cx, near):
    name = "leg-front-near" if near else "leg-front-far"
    emit(f'<g id="{name}">')
    front = [(cx + 6.0, 56.0), (cx + 6.3, 46.0), (cx + 6.0, 38.0), (cx + 5.6, 31.0), (cx + 5.1, 26.0),
             (cx + 4.85, 23.5), (cx + 5.2, 21.0), (cx + 5.0, 18.0), (cx + 4.3, 15.6), (cx + 3.85, 13.4),
             (cx + 3.6, 11.2), (cx + 3.6, 10.4)]
    rear = [(cx - 3.4, 10.4), (cx - 3.45, 13.5), (cx - 3.75, 16.0), (cx - 4.3, 18.6), (cx - 4.5, 21.0),
            (cx - 4.5, 23.4), (cx - 4.9, 26.5), (cx - 5.4, 31.0), (cx - 5.8, 38.0), (cx - 6.1, 46.0),
            (cx - 5.8, 56.0)]
    el(runs_path([front, [front[-1], rear[0]], rear + [(cx, 57.5), front[0]]], nd=2),
       fill="url(#patteNear)" if near else "url(#patteFar)")
    open_group("#AE9D7E" if near else "#8B7C62", 0.1, 0.5)
    edge_locks(front[2:10], +1, near, skip=(14.5, 25.5))
    edge_locks(rear[1:9][::-1], -1, near)
    leg_locks(cx, near, (-3.6, 3.4), (19.0, 31.0))
    close_group()
    # la tache noire, sur l'AVANT du genou ; une bande blanche dessous
    kf = [(cx + 4.95, 24.3), (cx + 5.15, 22.6), (cx + 5.45, 20.8), (cx + 5.3, 18.8), (cx + 4.95, 17.0), (cx + 4.4, 15.6)]
    kb = [(cx + 4.4, 15.6), (cx + 3.3, 15.85), (cx + 2.0, 16.7), (cx + 1.2, 18.4), (cx + 1.0, 20.4), (cx + 1.4, 22.4),
          (cx + 2.6, 23.8), (cx + 3.9, 24.5), (cx + 5.0, 24.3)]
    el(runs_path([kf, kb], nd=2), fill=B_BASE if near else "#110F0F")
    spot_edge_curls(kb, near, 6)
    spot_edge_curls(kf, near, 4)
    boot_and_hoof(cx, not near)
    white_over_boot(cx, near)
    pivot(name, cx - 0.3, 46.0)
    emit("</g>")


def hindleg(cx, near):
    name = "leg-back-near" if near else "leg-back-far"
    emit(f'<g id="{name}">')
    front = [(cx + 8.0, 58.0), (cx + 7.6, 48.0), (cx + 6.8, 40.0), (cx + 6.0, 33.0), (cx + 5.2, 28.5), (cx + 4.4, 25.5),
             (cx + 3.7, 23.0), (cx + 3.4, 21.0), (cx + 3.35, 18.0), (cx + 3.4, 14.0), (cx + 3.55, 11.0), (cx + 3.6, 10.4)]
    rear = [(cx - 3.4, 10.4), (cx - 3.45, 14.0), (cx - 3.7, 17.0), (cx - 4.5, 19.4), (cx - 5.6, 21.2), (cx - 6.4, 22.8),
            (cx - 6.6, 24.2), (cx - 7.0, 26.0), (cx - 7.8, 29.0), (cx - 8.8, 33.0), (cx - 9.6, 39.0),
            (cx - 10.0, 46.0), (cx - 9.6, 53.0), (cx - 8.8, 58.0)]
    el(runs_path([front, [front[-1], rear[0]], rear + [(cx, 59.5), front[0]]], nd=2),
       fill="url(#patteNear)" if near else "url(#patteFar)")
    open_group("#AE9D7E" if near else "#8B7C62", 0.1, 0.5)
    edge_locks(front[2:10], +1, near)
    edge_locks(rear[1:11][::-1], -1, near, skip=(17.5, 27.5))
    leg_locks(cx, near, (-5.0, 3.4), (19.0, 32.0))
    close_group()
    # la tache noire, sur la pointe du jarret (à l'ARRIÈRE) ; du blanc dessous
    hr = [(cx - 4.4, 18.8), (cx - 5.0, 19.6), (cx - 5.95, 21.1), (cx - 6.7, 22.7), (cx - 6.9, 24.2), (cx - 7.3, 26.2)]
    hf = [(cx - 7.3, 26.2), (cx - 5.7, 26.8), (cx - 4.1, 26.0), (cx - 3.1, 24.3), (cx - 2.85, 22.3), (cx - 3.15, 20.4),
          (cx - 4.4, 18.8)]
    el(runs_path([hr, hf], nd=2), fill=B_BASE if near else "#110F0F")
    spot_edge_curls(hf, near, 6)
    spot_edge_curls(hr, near, 4)
    boot_and_hoof(cx, not near)
    white_over_boot(cx, near)
    pivot(name, cx + 2.5, 49.0)
    emit("</g>")


# ================================================================== la queue

def tail():
    """Longue queue naturelle : une corde de laine faite de mèches, qui pend
    contre l'arrière des cuisses en suivant la courbe de la croupe, et
    s'amincit jusqu'à une houppe juste au-dessus du jarret ; en trois
    morceaux qui ondulent."""

    def body_top(x):
        sp = vspan(BODY_D, x)
        return sp[1] if sp else 0.0

    def segment(top, bottom, back, front, tassel=False, first=False):
        # `back`, `front` : bords de la corde, fonctions de la hauteur
        hs = [bottom + (top - bottom) * k / 6 for k in range(7)]
        if not first:
            hs = [bottom + (top - 3.5 - bottom) * k / 6 for k in range(7)]
        fill_pts = [(front(h) - 0.7, h) for h in hs[::-1]] + [(back(h) + 0.7, h) for h in hs]
        if first:
            fill_pts = [(x, min(h, body_top(x) - 1.0)) for (x, h) in fill_pts]
        el(cr(fill_pts, nd=1, t=0.8), fill="#D4C7AB")
        w_top = front(top) - back(top)
        count = max(3, int(round(w_top / 1.75)))
        open_group()
        roots = []
        for k in range(count):
            u = (k + 0.5) / count
            x0 = back(top) + w_top * u + rng.uniform(-0.25, 0.25)
            r0 = top - rng.uniform(0.0, 2.0)
            if first:
                r0 = min(r0, body_top(x0) - rng.uniform(0.3, 1.2))
            roots.append((r0, x0, u))
        roots.sort()
        for (r0, x0, u) in roots:
            xb = back(bottom) + (front(bottom) - back(bottom)) * u
            L = (r0 - bottom) + (rng.uniform(1.5, 2.8) if not tassel else rng.uniform(3.0, 4.6))
            ang = math.atan2(xb - x0, r0 - bottom) * 0.9
            if tassel:
                ang += (0.5 - u) * 0.3
            c = wool_color(x0, (r0 + bottom) / 2, -0.05 + 0.08 * (1 - u)) if not tassel else \
                mix("#F2ECE0", "#D6C9AD", 0.2 + 0.6 * u)
            lock(x0, r0, L, rng.uniform(1.7, 2.1) * (0.85 if tassel else 1.0), ang, c,
                 turn=rng.uniform(3.0, 4.4) if tassel else rng.uniform(1.0, 1.8), step=1.4, clen=2.4)
        # le bord arrière, effiloché comme le reste de la culotte
        if not tassel:
            for k in range(3):
                h = top - (top - bottom) * (k + 0.6) / 3.2
                lock(back(h) + 1.0, h + 1.2, rng.uniform(3.4, 5.0), rng.uniform(1.3, 1.7), -rng.uniform(0.25, 0.5),
                     wool_color(back(h), h, 0.05), turn=rng.uniform(2.6, 3.6), step=1.0)
        close_group()
        # le sillon d'ombre entre la queue et la fesse
        if not tassel:
            gtop = top - (5.0 if first else 1.6)
            gbot = bottom + 1.5
            n = 7
            groove = [(front(gtop + (gbot - gtop) * k / n) + 0.15 + 0.3 * math.sin(k * 1.9 + top),
                       gtop + (gbot - gtop) * k / n) for k in range(n + 1)]
            el(cr(groove, closed=False), stroke="#A18F6E", sw=1.1, so=0.18, cap="round")
            el(cr(groove, closed=False), stroke="#8E7C5B", sw=0.2, so=0.4, cap="round")

    def lerp_edge(pts):
        def f(h):
            for (h0, x0), (h1, x1) in zip(pts, pts[1:]):
                if h0 >= h >= h1:
                    return x0 + (x1 - x0) * (h0 - h) / (h0 - h1)
            return pts[0][1] if h > pts[0][0] else pts[-1][1]
        return f

    # bords de la corde (hauteur, x), de la racine à la houppe
    back = lerp_edge([(80.0, 6.6), (76.6, 4.9), (71.2, 2.3), (65.0, 0.7), (58.5, 0.1), (52.0, 0.4), (45.5, 1.1),
                      (40.0, 1.9), (34.0, 2.6), (28.0, 3.4)])
    front = lerp_edge([(80.0, 10.2), (72.0, 9.6), (62.0, 9.0), (52.0, 8.4), (44.0, 7.7), (36.0, 6.9), (28.0, 6.0)])
    emit('<g id="tail">')
    segment(79.0, 57.5, back, front, first=True)
    emit('<g id="tail-mid">')
    segment(62.0, 42.5, back, front)
    emit('<g id="tail-tip">')
    segment(47.0, 31.5, back, front, tassel=True)
    pivot("tail-tip", 4.4, 45.2)
    emit("</g>")
    pivot("tail-mid", 4.6, 60.6)
    emit("</g>")
    pivot("tail", 6.6, 77.0)
    emit("</g>")


# ================================================================== la tête

THETA = math.radians(40.0)      # tête levée, aux aguets : chanfrein à 40° sous l'horizontale
PX, PH = 114.0, 102.0           # la nuque (sommet du crâne), dans le monde
K = 30.0 / 26.0                 # tête de 30 cm de la nuque au bout du nez (0,4 fois le garrot)
HU = (math.cos(THETA), -math.sin(THETA))
HV = (-math.sin(THETA), -math.cos(THETA))


def HL(lx, ly):
    """Repère de la tête (unités d'une tête de 26 cm ; origine à la nuque, x
    vers le bout du nez, y vers la mâchoire) vers le monde."""
    lx, ly = lx * K, ly * K
    return (PX + lx * HU[0] + ly * HV[0], PH + lx * HU[1] + ly * HV[1])


def HLs(pts):
    return [HL(*p) for p in pts]


# Profil (nez busqué, museau large et franc) avec l'encoche de la mâchoire.
DORSAL = [(-3.4, 0.2), (-1.9, -1.0), (0.4, -1.15), (3.0, -0.95), (6.0, -0.7), (9.0, -0.35), (12.0, 0.05),
          (15.0, 0.6), (17.8, 1.4), (20.3, 2.55), (22.5, 3.9), (24.2, 5.25), (25.35, 6.6), (25.95, 7.85),
          (26.05, 9.0), (25.75, 9.85), (25.25, 10.3)]
MOUTH = [(25.25, 10.3), (24.0, 10.6), (22.4, 10.82), (20.7, 10.98), (19.3, 11.25), (18.4, 11.65)]
NOTCH = [(18.4, 11.65), (18.05, 12.5), (17.9, 13.3)]
JAWLINE = [(17.9, 13.3), (14.9, 13.75), (11.9, 14.1), (9.2, 14.5), (7.4, 15.0), (5.2, 15.5)]
SKULLBACK = [(5.2, 15.5), (3.0, 15.0), (0.6, 13.5), (-1.6, 11.0), (-3.2, 7.8), (-3.9, 4.3), (-3.8, 1.9), (-3.4, 0.2)]
MASK_REAR = [(7.3, 15.05), (7.15, 12.6), (7.35, 10.3), (7.5, 8.0), (7.6, 5.8), (7.85, 3.6), (8.35, 1.8), (9.0, 0.35),
             (9.55, -0.1)]
EYE_L = (11.9, 4.35)
EAR_L = (2.2, 5.6)
HORN_L = (3.9, -0.7)

# La lèvre supérieure (repère de la tête) : sous la narine, du bout du nez à
# la commissure, jusque sous la ligne de la bouche ; elle se retrousse autour
# de son arrière pour le flehmen et découvre la gencive (le bourrelet du
# haut, sans incisives), d'un rose sombre sous la peau noire.
LIP = [(21.6, 9.4), (23.3, 9.15), (24.4, 9.1), (25.5, 8.95), (27.2, 8.7), (27.2, 11.4), (25.0, 11.1),
       (23.4, 11.2), (21.6, 11.4)]
LIP_PIVOT = (21.8, 9.6)
GUM = [(22.1, 10.25), (23.4, 10.0), (24.6, 9.75), (25.35, 9.5), (25.65, 9.85), (25.1, 10.12), (24.0, 10.4),
       (22.9, 10.6), (22.1, 10.7)]
GUM_RIDGE = [(22.5, 10.45), (23.6, 10.25), (24.6, 10.02), (25.25, 9.82)]
GUM_FILL, GUM_CREASE = "#a86f7b", "#7d4a57"

# Le capuchon (monde) : la nuque qui descend en pente douce jusqu'au garrot,
# le derrière de la tête et la gorge ; son bas reste dans la toison du corps.
NAPE = [(112.0, 107.2), (108.6, 105.6), (105.3, 102.7), (102.1, 99.0), (98.9, 95.4), (95.6, 92.4), (92.2, 90.4),
        (90.2, 89.5), (92.4, 89.9), (94.6, 89.3), (97.0, 90.1), (99.4, 89.4), (101.9, 90.2), (104.4, 89.4),
        (106.8, 89.8), (108.8, 87.8), (110.8, 85.4), (112.6, 85.0), (114.0, 88.5), (114.6, 92.0), (114.8, 98.0),
        (114.5, 103.5)]


def head():
    E = HL(*EYE_L)
    emit('<g id="head">')

    # --- l'oreille du fond : cachée derrière la nuque (sa pointe dépasse
    # quand les oreilles se dressent)
    emit('<g id="ear-far">')
    base = HL(3.0, 4.2)
    ang = math.radians(-6.0)
    a = (-math.cos(ang), -math.sin(ang))
    nrm = (-a[1], a[0])
    ear = [(-1.0, -0.8), (1.6, -1.5), (4.4, -2.0), (7.0, -1.85), (9.0, -1.15), (10.3, -0.2), (10.0, 0.55),
           (8.0, 1.4), (5.2, 1.85), (2.6, 1.65), (0.2, 1.1), (-1.0, 0.7)]
    el(cr([(base[0] + ex * a[0] + ey * nrm[0], base[1] + ex * a[1] + ey * nrm[1]) for (ex, ey) in ear]),
       fill="#0D0B0B")
    pivot("ear-far", *base)
    emit("</g>")

    # --- le capuchon de laine blanche, en mèches qui pendent
    rad("capuchon", 106.0, 98.0, 16.0, [(0, "#E9E1D0", 1), (0.6, "#DED3BD", 1), (1, "#C8B999", 1)], fx=104.0, fh=103.0)
    el(cr(NAPE, nd=2), fill="url(#capuchon)")
    nape_roots = scatter([(111.5, 106.4), (108.0, 104.6), (104.6, 101.6), (101.2, 97.6), (97.6, 93.8),
                          (93.6, 91.2), (97.0, 91.4), (102.0, 92.0), (107.0, 92.0), (110.6, 90.0), (112.6, 92.0),
                          (113.5, 100.0)], 52, 1.4)
    nape_roots.sort(key=lambda p: p[1])
    open_group()
    for (x, h) in nape_roots:
        light = 0.55 + 0.035 * (h - 95) - 0.02 * (x - 105) + rng.uniform(-0.1, 0.1)
        L = rng.uniform(4.5, 7.5)
        lock(x, h + 0.6, L, rng.uniform(1.5, 2.0), rng.uniform(-0.22, 0.05), mix("#D3C6AB", "#F6F1E6", light),
             turn=rng.uniform(2.6, 4.0), step=1.0)
    # le bord de la nuque : des mèches qui dépassent, tournées vers l'arrière
    for (x, h) in dense(NAPE[:8], closed=False, per=2)[1:-1]:
        lock(x + 0.6, h + 0.8, rng.uniform(3.0, 4.6), rng.uniform(1.3, 1.7), rng.uniform(-0.75, -0.35),
             mix("#D8CCB2", "#F3EEE3", rng.uniform(0.35, 0.8)), turn=rng.uniform(2.6, 3.8), step=0.9)
    close_group()

    # --- la mâchoire : le bas du museau, et derrière, la bouche
    emit('<g id="jaw">')
    el(cr(HLs([(15.6, 10.2), (19.4, 10.1), (19.4, 12.8), (15.6, 13.7)]), t=0.6), fill=B_BASE)
    el(cr(HLs([(17.6, 11.6), (18.8, 10.35), (21.2, 9.65), (24.0, 9.2), (25.55, 9.55), (25.35, 10.4), (23.0, 11.05),
               (20.0, 11.45), (18.2, 12.0)]), t=0.8), fill="#2B1D1C")
    el(cr(HLs([(19.2, 11.1), (20.8, 10.7), (22.8, 10.65), (24.1, 10.78), (23.5, 11.2), (21.3, 11.35)]), t=0.9),
       fill="#7E5753")
    el(cr(HLs([(20.4, 10.83), (22.4, 10.77), (23.5, 10.86)]), closed=False), stroke="#A07470", sw=0.15, so=0.7,
       cap="round")
    # les incisives du bas (en haut, le mouton n'a qu'un bourrelet)
    el(cr(HLs([(24.25, 10.15), (25.1, 10.05), (25.15, 10.42), (24.3, 10.52)]), t=0.5), fill="#D3CAB6")
    jaw = HLs([(17.2, 11.5), (18.9, 11.1), (20.7, 10.9), (22.4, 10.73), (24.0, 10.5), (25.3, 10.22), (25.35, 11.0),
               (24.8, 11.95), (23.5, 12.7), (21.3, 13.15), (18.8, 13.5), (16.9, 13.6), (16.6, 12.4)])
    el(cr(jaw, t=0.85), fill=B_BASE)
    el(cr(HLs([(22.7, 10.68), (24.0, 10.48), (25.3, 10.22), (25.35, 10.95), (24.7, 11.4), (23.5, 11.25)]), t=0.8),
       fill=NOSE)
    el(cr(HLs([(23.1, 10.9), (24.3, 10.78), (25.05, 10.62)]), closed=False), stroke=NOSE_SHEEN, sw=0.12, so=0.55,
       cap="round")
    for (lx, ly) in ((19.0, 12.3), (20.8, 12.6), (22.6, 12.2), (17.8, 12.8)):
        x, h = HL(lx, ly)
        black_curl(x, h, 0.42, op=0.55)
    pivot("jaw", *HL(17.9, 11.9))
    emit("</g>")

    # --- le crâne (le fond des joues, sous la laine)
    el(runs_path([HLs(DORSAL), HLs(MOUTH), HLs(NOTCH), HLs(JAWLINE + SKULLBACK[1:])], nd=2), fill="#E3D9C5")

    # --- le masque noir : museau, chanfrein jusqu'à mi-front, le tour des
    # yeux, la joue jusqu'à la mâchoire
    mask_front = [p for p in DORSAL if p[0] >= 9.6]
    jaw_back = [(17.9, 13.3), (14.9, 13.75), (11.9, 14.1), (9.2, 14.5), (7.3, 15.05)]
    mask_runs = [HLs([(9.55, -0.1)] + mask_front), HLs(MOUTH), HLs(NOTCH), HLs(jaw_back), HLs(MASK_REAR)]
    lin("masque", *HL(12.0, -1.0), *HL(12.0, 15.0), [(0, "#24201E", 1), (0.45, B_BASE, 1), (1, "#0E0C0C", 1)])
    el(runs_path(mask_runs, nd=2), fill="url(#masque)")

    # --- la truffe : peau nue gris ardoise, narine en virgule, reflet humide
    rad("truffe", *HL(24.9, 6.9), 3.6, [(0, "#3E4147", 1), (0.6, NOSE, 1), (1, "#141518", 1)])
    nose = HLs([(23.3, 5.15), (24.2, 5.85), (25.35, 6.95), (25.95, 8.0), (26.05, 9.0), (25.75, 9.85), (25.25, 10.3),
                (24.55, 10.4), (24.15, 9.6), (23.95, 8.3), (23.6, 6.6)])
    el(cr(nose, t=0.9), fill="url(#truffe)")
    el(cr(HLs([(25.3, 8.35), (24.95, 8.65), (24.55, 8.35), (24.25, 7.6), (23.95, 6.55), (24.3, 6.8), (24.85, 7.5),
               (25.25, 7.95)]), t=0.9), fill=NOSTRIL)
    el(cr(HLs([(24.1, 6.15), (24.85, 6.65), (25.5, 7.4)]), closed=False), stroke=NOSE_SHEEN, sw=0.25, so=0.75,
       cap="round")
    el(cr(HLs([(25.6, 8.85), (25.4, 9.5), (25.05, 9.95)]), closed=False), stroke="#121316", sw=0.15, so=0.8,
       cap="round")

    # --- l'œil : iris ambré, pupille en fente horizontale
    emit('<g id="eyes">')
    rot = math.radians(-16.0)   # le coin avant un peu plus bas
    almond_l = [(-1.95, 0.08), (-1.33, -0.92), (-0.1, -1.28), (1.08, -1.05), (1.95, -0.14), (1.08, 0.94),
                (-0.1, 1.17), (-1.33, 0.82)]

    def eye_pt(ex, ey):
        c, s = math.cos(rot), math.sin(rot)
        return (E[0] + ex * c + ey * s, E[1] + ex * s - ey * c)

    almond = [eye_pt(ex, ey) for (ex, ey) in almond_l]
    el(cr(almond, t=0.9), fill="#1A1411")
    emit('<g id="pupils">')
    ic = (E[0] - 0.6, E[1] - 0.05)
    rad("iris", ic[0], ic[1], 1.65, [(0, "#D8B06C", 1), (0.35, IRIS_LIGHT, 1), (0.68, IRIS, 1), (0.88, "#7A6040", 1),
                                     (0.96, LIMBUS, 1), (1, "#2A211B", 1)],
        fx=ic[0] + 0.15, fh=ic[1] - 0.55)
    el(cr(ellipse_pts(ic[0], ic[1], 1.65, 1.65, 0, 16)), fill="url(#iris)")
    for k in range(12):
        a0 = 2 * math.pi * k / 12 + 0.2
        el(cr([(ic[0] + 0.62 * math.cos(a0), ic[1] + 0.62 * math.sin(a0)),
               (ic[0] + 1.45 * math.cos(a0 + 0.12), ic[1] + 1.45 * math.sin(a0 + 0.12))], closed=False),
           stroke="#6E5536", sw=0.06, so=0.45, cap="round")
    pw, ph = 1.9, 0.58
    X, Y = S(ic[0] - pw / 2, ic[1] + ph / 2)
    emit(f'<rect x="{num(X)}" y="{num(Y)}" width="{num(pw)}" height="{num(ph)}" rx="{num(ph / 2)}" fill="{PUPIL}"/>')
    pivot("pupils", *ic)
    emit("</g>")
    el(cr(ellipse_pts(E[0] - 1.05, E[1] + 0.5, 0.4, 0.28, 0.3, 10)), fill=CATCH, fo=0.92)
    el(cr(ellipse_pts(E[0] + 0.6, E[1] - 0.56, 0.17, 0.12, 0.3, 8)), fill=CATCH, fo=0.4)
    el(cr(almond, t=0.9), stroke="#0E0C0C", sw=0.6, join="round")
    lid = [eye_pt(-2.0, 0.15), eye_pt(-1.28, -1.12), eye_pt(0.0, -1.48), eye_pt(1.12, -1.22), eye_pt(2.0, -0.22)]
    el(cr(lid, closed=False), stroke=B_SHEEN, sw=0.22, so=0.8, cap="round")
    for k in range(5):
        u = (k + 0.5) / 5
        p = eye_pt(-1.5 + 3.0 * u, -1.18 - 0.28 * math.sin(math.pi * u))
        q = eye_pt(-1.5 + 3.0 * u - 0.38, -1.74 - 0.28 * math.sin(math.pi * u))
        el(cr([p, q], closed=False), stroke=B_DEEP, sw=0.12, cap="round")
    pivot("eyes", *E)
    emit("</g>")

    # --- le pourtour de l'œil, immobile : il cache l'iris quand il glisse
    cover_outer = [eye_pt(-3.2, 0.4), eye_pt(-2.8, -2.0), eye_pt(-0.6, -3.2), eye_pt(2.8, -3.1), eye_pt(5.6, -1.5),
                   eye_pt(6.2, 0.6), eye_pt(5.0, 2.8), eye_pt(1.2, 3.4), eye_pt(-1.9, 2.8), eye_pt(-3.1, 1.7)]
    win = [eye_pt(ex * 1.04, ey * 1.06) for (ex, ey) in almond_l]
    el(cr(cover_outer, t=0.9) + cr(win[::-1], t=0.9), fill=B_BASE, rule="evenodd")

    # --- la laine noire bouclée du masque (pas sur la truffe ni sur l'œil)
    mask_poly = HLs([(9.55, -0.1)] + mask_front + MOUTH + NOTCH + jaw_back + MASK_REAR)
    nose_d = dense(nose, True, 4)
    win_d = dense([eye_pt(ex * 1.3, ey * 1.45) for (ex, ey) in almond_l], True, 4)

    def ok(x, h):
        return not inside(nose_d, x, h) and not inside(win_d, x, h)

    for (x, h) in scatter(mask_poly, 110, 0.9, ok):
        black_curl(x, h, rng.uniform(0.3, 0.52), sheen=rng.choice((B_SHEEN, B_SHEEN, B_WARM)), op=0.5, sw=0.13)
    # --- la joue blanche derrière le masque (elle cache aussi l'iris s'il
    # glissait trop en arrière), et la gorge blanche sous la mâchoire
    cheek = HLs([(7.55, 0.4), (7.25, 2.5), (7.1, 5.0), (7.0, 7.5), (6.9, 10.0), (6.8, 12.5), (6.85, 15.2),
                 (4.0, 15.8), (2.6, 12.0), (3.4, 6.0), (4.6, 1.0)])
    el(cr(cheek, t=0.9), fill="#E1D7C2")
    open_group()
    roots = scatter(HLs([(2.5, 0.0), (7.4, 0.2), (7.0, 15.4), (2.0, 16.0), (1.5, 8.0)]), 18, 1.25)
    roots.sort(key=lambda p: p[1])
    for (x, h) in roots:
        lock(x, h + 0.5, rng.uniform(3.0, 4.6), rng.uniform(1.3, 1.7), rng.uniform(-0.15, 0.2),
             mix("#D6CAAF", "#F2ECE1", 0.35 + 0.03 * (h - 85) + rng.uniform(-0.1, 0.1)), turn=rng.uniform(2.8, 4.0),
             step=0.9)
    # les mèches du bord du masque, qui débordent un peu sur le noir
    for k in range(9):
        u = k / 8
        x, h = HL(7.2 - 0.4 * u + rng.uniform(-0.1, 0.1), 1.2 + 13.2 * u)
        lock(x - 0.3, h + 0.8, rng.uniform(2.4, 3.2), rng.uniform(1.2, 1.5), rng.uniform(0.1, 0.35),
             mix("#DCD1B8", "#F1EBDF", 0.75 - 0.35 * u + rng.uniform(-0.08, 0.08)), turn=rng.uniform(2.6, 3.6),
             step=0.8)
    # la gorge : de la laine blanche qui pend sous l'angle de la mâchoire
    for (lx, ly) in ((4.2, 15.6), (6.0, 15.4), (2.6, 15.2), (7.4, 15.0), (5.1, 16.6)):
        x, h = HL(lx, ly)
        lock(x, h + 0.6, rng.uniform(4.0, 5.5), rng.uniform(1.5, 1.9), rng.uniform(-0.1, 0.15),
             mix("#D2C5A9", "#EAE3D4", rng.uniform(0.3, 0.7)), turn=rng.uniform(2.6, 3.8), step=1.0)
    close_group()

    # --- le toupet : un grand dôme de laine blanche frisée sur le front et la
    # nuque, entre les cornes ; ses boucles retombent sur le haut du masque
    TK = [(-4.8, 1.4), (-4.3, -1.8), (-2.6, -4.4), (0.2, -6.0), (3.6, -6.6), (7.0, -6.0), (9.6, -4.4), (11.4, -2.3),
          (12.4, 0.1), (12.7, 2.1), (11.9, 3.1), (10.8, 2.6), (9.8, 3.2), (8.7, 2.3), (7.3, 2.6), (5.9, 1.6),
          (4.2, 1.8), (2.6, 1.1), (0.8, 1.5), (-1.2, 1.3), (-3.0, 2.0)]
    rad("toupet", *HL(3.0, -3.5), 10.5, [(0, "#FBF8F1", 1), (0.6, "#ECE4D3", 1), (1, "#D0C2A5", 1)])
    el(cr(HLs(TK)), fill="url(#toupet)")
    crown = HL(2.0, -2.5)
    tk_roots = scatter(HLs([(-4.4, 0.8), (-4.0, -1.8), (-2.4, -4.6), (0.4, -6.1), (3.6, -6.6), (6.9, -6.0),
                            (9.5, -4.4), (11.3, -2.2), (11.6, 0.6), (8.0, 0.8), (3.0, 0.6), (-1.0, 0.4),
                            (-4.6, 1.6)]), 54, 1.2)
    tk_roots.sort(key=lambda p: -p[1])
    open_group("#B9A98A", 0.11, 0.6)
    for (x, h) in tk_roots:
        vx, vh = x - crown[0], h - crown[1]
        ang = math.atan2(0.45 * vx, -(vh - 4.0))          # vers le bas, un peu vers l'extérieur
        ang = max(-1.2, min(1.2, ang))
        light = 0.66 + 0.045 * (h - 103) - 0.02 * (x - 117) + rng.uniform(-0.1, 0.1)
        lock(x, h + 0.4, rng.uniform(2.6, 4.2), rng.uniform(1.5, 1.95), ang, mix("#D7CAAE", "#FCFAF5", light),
             turn=rng.uniform(3.4, 4.6), step=0.7, waves=0.8, amp=0.45)
    # les boucles du bord du front, qui retombent sur le noir du masque
    for (lx, ly) in ((12.1, -0.4), (11.6, 1.4), (10.4, 2.2), (9.1, 2.1), (7.7, 2.0)):
        x, h = HL(lx, ly)
        lock(x, h + 0.6, rng.uniform(2.8, 3.8), rng.uniform(1.4, 1.8), rng.uniform(0.15, 0.45),
             mix("#D8CCB1", "#F6F2E9", rng.uniform(0.45, 0.75)), turn=rng.uniform(3.0, 4.0), step=0.8, waves=0.8,
             amp=0.4)
    close_group()

    # --- l'oreille du devant : noire jusqu'à la base, portée de côté sous la corne
    emit('<g id="ear-near">')
    base = HL(*EAR_L)
    ang = math.radians(22.0)
    a = (-math.cos(ang), -math.sin(ang))
    nrm = (-a[1], a[0])     # vers le bas de l'oreille

    def ear_pt(ex, ey):
        return (base[0] + ex * a[0] + ey * nrm[0], base[1] + ex * a[1] + ey * nrm[1])

    ear = [(-1.4, -1.0), (1.5, -1.8), (4.4, -2.35), (7.2, -2.2), (9.5, -1.5), (11.2, -0.45), (11.0, 0.5),
           (9.0, 1.6), (6.0, 2.2), (2.9, 2.05), (0.3, 1.45), (-1.4, 0.9)]
    lin("oreille", *ear_pt(4.0, -2.5), *ear_pt(4.0, 2.2), [(0, "#2E2826", 1), (0.45, B_BASE, 1), (1, "#0B0909", 1)])
    el(cr([ear_pt(*p) for p in ear]), fill="url(#oreille)")
    el(cr([ear_pt(1.6, -1.7), ear_pt(4.5, -2.2), ear_pt(7.3, -2.05), ear_pt(9.6, -1.35)], closed=False),
       stroke=EAR_GLOW, sw=0.24, so=0.55, cap="round")
    el(cr([ear_pt(1.0, 1.15), ear_pt(4.0, 1.6), ear_pt(7.0, 1.4), ear_pt(9.8, 0.6)], closed=False),
       stroke=INNER_EAR, sw=0.55, so=0.9, cap="round")
    for k in range(8):
        u = 0.12 + 0.1 * k
        el(cr([ear_pt(10.4 * u, -0.95 + 0.3 * math.sin(k)), ear_pt(10.4 * u + 1.3, -0.75 + 0.3 * math.sin(k))],
              closed=False), stroke="#2F2A28", sw=0.1, so=0.7, cap="round")
    pivot("ear-near", *base)
    emit("</g>")

    # --- collier de laine blanche autour de la base de l'oreille
    open_group()
    for (lx, ly) in ((1.0, 3.7), (0.3, 7.1), (1.9, 8.2), (3.3, 4.2), (-0.7, 5.3), (3.7, 7.1)):
        x, h = HL(lx, ly)
        lock(x, h + 0.8, rng.uniform(2.6, 3.4), rng.uniform(1.4, 1.8), rng.uniform(-0.3, 0.2),
             mix("#D9CDB3", "#F2ECE0", rng.uniform(0.35, 0.8)), turn=rng.uniform(3.0, 4.0), step=0.8)
    close_group()

    # --- la corne du devant : spirale pâle et côtelée qui part en arrière,
    # descend derrière l'oreille et finit vers l'avant, bien dégagée de la joue
    bx, bh = HL(*HORN_L)
    offs = [(0, 0), (-1.32, 3.28), (-4.34, 5.42), (-8.0, 5.54), (-10.78, 3.66), (-12.04, 0.76), (-11.59, -2.02),
            (-9.71, -3.47), (-7.12, -3.22)]
    horn = [(bx + dx, bh + dh) for (dx, dh) in offs]
    if VARIANTE == "belier":
        # La spirale du bélier adulte : plus large, plus épaisse (§ 1).
        horn = [(bx + 1.32 * dx, bh + 1.32 * dh) for (dx, dh) in offs]
        horn_band(horn, [4.4, 4.15, 3.85, 3.5, 3.1, 2.65, 2.1, 1.45, 0.5])
    elif VARIANTE != "agneau":
        horn_band(horn, [3.0, 2.85, 2.65, 2.42, 2.18, 1.9, 1.55, 1.12, 0.45])

    # --- des boucles du toupet par-dessus la base de la corne
    open_group("#B9A98A", 0.11, 0.6)
    for (lx, ly) in ((3.0, -2.6), (5.2, -3.2), (1.6, -1.8), (6.8, -2.3), (4.2, -1.2)):
        x, h = HL(lx, ly)
        vx, vh = x - crown[0], h - crown[1] - 2.0
        lock(x, h + 0.5, rng.uniform(3.0, 4.2), rng.uniform(1.5, 1.9), max(-1.4, min(1.4, math.atan2(vx, -vh))),
             mix("#DCD0B6", "#FAF6EE", rng.uniform(0.55, 0.95)), turn=rng.uniform(3.0, 4.2), step=0.8, waves=0.9,
             amp=0.45)
    close_group()

    pivot("head", 109.5, 86.0)
    emit("</g>")


def horn_band(center, widths):
    """Une corne : une bande effilée le long d'une spirale, claire dessus,
    sombre dessous, côtelée en travers, la pointe lisse et pâle."""
    C = dense(center, closed=False, per=6)
    nW = len(widths)
    W = []
    for i in range(len(C)):
        u = i / (len(C) - 1) * (nW - 1)
        k = min(int(u), nW - 2)
        f = u - k
        W.append(widths[k] * (1 - f) + widths[k + 1] * f)
    left, right, N = [], [], []
    for i in range(len(C)):
        a = C[max(i - 1, 0)]
        b = C[min(i + 1, len(C) - 1)]
        dx, dh = b[0] - a[0], b[1] - a[1]
        L = math.hypot(dx, dh) or 1
        nx, nh = -dh / L, dx / L
        N.append((nx, nh))
        left.append((C[i][0] + nx * W[i] / 2, C[i][1] + nh * W[i] / 2))
        right.append((C[i][0] - nx * W[i] / 2, C[i][1] - nh * W[i] / 2))
    outline = left + [C[-1]] + right[::-1]
    el(cr(outline[::2] + [outline[-1]], nd=2, t=0.9), fill=HORN, stroke="#7C6347", sw=0.2, so=0.75, join="round")
    inner = [(C[i][0] - N[i][0] * W[i] * 0.22, C[i][1] - N[i][1] * W[i] * 0.22) for i in range(len(C))]
    outer = [(C[i][0] + N[i][0] * W[i] * 0.28, C[i][1] + N[i][1] * W[i] * 0.28) for i in range(len(C))]
    el(cr(right[::2] + inner[::-2], nd=2, t=0.9), fill=HORN_SHADOW, fo=0.8)
    m = int(len(C) * 0.8)
    el(cr(left[:m:2] + outer[m - 1::-2], nd=2, t=0.9), fill=HORN_LIGHT, fo=0.65)
    streak = [(C[i][0] - N[i][0] * W[i] * 0.05, C[i][1] - N[i][1] * W[i] * 0.05) for i in range(4, int(len(C) * 0.6), 3)]
    el(cr(streak, closed=False), stroke=HORN_STREAK, sw=0.18, so=0.3, cap="round")
    acc = [0.0]
    for i in range(1, len(C)):
        acc.append(acc[-1] + math.hypot(C[i][0] - C[i - 1][0], C[i][1] - C[i - 1][1]))
    total = acc[-1]
    s = 0.45
    while s < total - 2.8:
        i = max(1, min(len(C) - 2, next(k for k in range(len(acc)) if acc[k] >= s)))
        w = W[i] * 0.5
        nx, nh = N[i]
        dx, dh = C[i + 1][0] - C[i - 1][0], C[i + 1][1] - C[i - 1][1]
        L = math.hypot(dx, dh) or 1
        tx, th = dx / L, dh / L
        p1 = (C[i][0] + nx * w * 0.95, C[i][1] + nh * w * 0.95)
        pm = (C[i][0] + tx * 0.22, C[i][1] + th * 0.22)
        p2 = (C[i][0] - nx * w * 0.95, C[i][1] - nh * w * 0.95)
        el(cr([p1, pm, p2], closed=False), stroke="#8C7154", sw=0.15, so=0.75, cap="round")
        el(cr([(p1[0] + tx * 0.18, p1[1] + th * 0.18), (pm[0] + tx * 0.18, pm[1] + th * 0.18)], closed=False),
           stroke=HORN_LIGHT, sw=0.11, so=0.75, cap="round")
        s += 0.58 + 0.04 * s
    el(cr([C[-10], C[-5], C[-1]], closed=False), stroke=HORN_LIGHT, sw=0.4, so=0.55, cap="round")


# ================================================================== articulations

def articule(svg):
    """Le dessin coupé à ses articulations (voir tools/races/articulations.py) :
    chaque patte au carpe ou au jarret (le milieu de la tache noire du genou,
    la pointe du jarret) ; l'encolure, l'avant du haut de la toison, devant
    la droite qui descend du garrot vers le poitrail, posée sous le corps ; la
    tête tourne à la nuque, derrière les cornes ; la lèvre supérieure,
    détachée de la tête, avec la gencive qu'elle cache ; le bout des
    mèches de la frange du ventre, coupé au bas du fond de la toison, en
    trois mèches qui balancent ; trois touffes d'encolure dont la racine
    suit le cou et dont le bout reste pendant."""
    withers, chest = S(99.0, 90.0), S(127.0, 66.0)

    def lip(root, paints):
        def w(pts):
            return [S(*HL(*p)) for p in pts]
        articulations.lift_part(root, paints, "head", w(LIP), "lip", S(*HL(*LIP_PIVOT)),
                                under=articulations.gum(w(GUM), w(GUM_RIDGE), GUM_FILL, GUM_CREASE, 0.3))
        neck = next(el for el in root.iter() if el.get("id") == "neck")
        for k, (x0, x1, h) in enumerate(NECK_FRINGE, 1):
            cut = S((x0 + x1) / 2, h)
            articulations.hang_strands(
                neck, paints, f"neck-fringe-{k}", cut[1], f"neck-sway-{k}", cut, margin=3.0)
        # Les marques des mèches restées dans le corps ne servent plus.
        for el in root.iter():
            if el.get("class", "").startswith("neck-fringe-"):
                el.attrib.pop("class")
        for k in range(1, len(FRINGE) + 1):
            cut = S(*fringe_cut(k))
            articulations.hang_strands(root, paints, f"frange-{k}", cut[1], f"sway-{k}", cut)

    return articulations.articulate_breed(
        svg,
        carpus_y=S(0, 20.0)[1],
        hock_y=S(0, 22.0)[1],
        neck=dict(
            source="body",
            area=articulations.shoulder(withers, chest, bulge=9.0),
            pivot=S(92.0, 52.0),
            poll=S(105.0, 90.0),
            muzzle=S(127.0, 72.5),
            margin=6.0,
            filler=(1.0, 10.0),
        ),
        extra=lip,
    )


# ================================================================== assemblage

def main(path):
    lin("patteNear", 0, 34, 0, 10, [(0, "#E6DCC8", 1), (1, "#D2C5A9", 1)])
    lin("patteFar", 0, 34, 0, 10, [(0, "#C2B396", 1), (1, "#A99A7C", 1)])
    emit("<!-- Pattes : toutes sous la toison, celles du fond d'abord -->")
    hindleg(HIND_FAR, near=False)
    foreleg(FORE_FAR, near=False)
    hindleg(HIND_NEAR, near=True)
    foreleg(FORE_NEAR, near=True)
    emit("<!-- Corps : la toison, la base du cou et la tache noire sous la queue -->")
    body()
    emit("<!-- Queue : longue, laineuse, en trois morceaux -->")
    tail()
    emit("<!-- Tête : masque noir, toupet blanc, oreilles noires, cornes en spirale -->")
    head()
    pivot("ground", (HIND_NEAR + FORE_NEAR) / 2, 0.0)

    W, H = 142, 118
    head_s = (f'<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 {W} {H}" width="{W}" height="{H}">\n'
              "  <!--\n"
              "    Nez noir du Valais (Walliser Schwarznasenschaf) : une brebis adulte en\n"
              "    pleine toison, de profil. 1 unité = 1 cm de l'animal (75 cm au garrot).\n"
              "    Mèches, boucles et marques tracées d'après le standard de la race ;\n"
              "    rig au contrat de docs/rig.md (pattes sous la toison, queue en trois morceaux).\n"
              "    © 2026 Alejandro San Martin Lamas — MIT OR Apache-2.0\n"
              "  -->\n")
    svg = head_s + "<defs>" + "".join(DEFS) + "</defs>\n" + "\n".join(OUT) + "\n</svg>\n"
    svg = articule(svg)
    if VARIANTE:
        return variante(path, svg)
    with open(path, "w") as f:
        f.write(svg)
    print(f"{path} : {len(svg) / 1024:.0f} Ko, {svg.count('<path')} tracés")


def variante(path, svg):
    """Les autres dessins : ce qui change, puis les proportions."""
    root = ET.fromstring(svg)
    pass
    variantes.ecrire(path, root, **PROPORTIONS[VARIANTE])


if __name__ == "__main__":
    main(sys.argv[1] if len(sys.argv) > 1 else "nez-noir.svg")
