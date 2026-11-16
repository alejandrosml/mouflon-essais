#!/usr/bin/env python3
"""Générateur du rig SVG du mouton d'Ouessant (bélier noir) pour Mouflon.

Unités : H = 100 unités = hauteur au garrot (47 cm). Sol en y = GY.
Les formes sont décrites en (x, h), h = hauteur au-dessus du sol ;
y = GY - h dans le SVG.
"""
import itertools
import math
import os
import random
import sys

sys.path.insert(0, os.path.join(os.path.dirname(os.path.abspath(__file__)), ".."))
import articulations  # noqa: E402  (tools/races/articulations.py)
import variantes  # noqa: E402

# Les autres dessins de la race (tools/races/variantes.py), d'après la fiche
# (§ 1.3, 2.1). Sans variante : le bélier. Le dessin de base compte en
# centièmes de hauteur au garrot ; les variantes sont écrites en centimètres.
VARIANTE = sys.argv[2] if len(sys.argv) > 2 and sys.argv[2] in variantes.SECTIONS else ""
PROPORTIONS = {
    "brebis": dict(  # Brebis d'Ouessant : 43 cm au garrot, sans cornes ni cravate, tête fine au profil droit, encolure mince.
        garrot=(100, 43), encolure=(1.0, .86), tete=(.97, .93)),
    "agneau": dict(  # Agneau d'Ouessant de deux semaines : 26 cm au garrot, noir, sans cornes.
        variantes.AGNEAU, garrot=(100, 26)),
}

GY = 160.0
OUT = sys.argv[1] if len(sys.argv) > 1 else "ouessant.svg"
DEBUG = "--debug" in sys.argv
PREC = [1]


def Y(h):
    return GY - h


def f(v):
    s = f"{v:.{PREC[0]}f}".rstrip("0").rstrip(".")
    if s in ("-0", "", "0"):
        return "0"
    if s.startswith("0."):
        s = s[1:]
    elif s.startswith("-0."):
        s = "-" + s[2:]
    return s


def join_nums(nums):
    """Nombres séparés au plus court : pas d'espace devant un signe moins ou
    un point quand le nombre précédent a déjà un point."""
    out = ""
    for k, t in enumerate(nums):
        if k and not t.startswith("-") and not (t.startswith(".") and "." in nums[k - 1]):
            out += " "
        out += t
    return out


def P(p):
    return join_nums([f(p[0]), f(p[1])])


def rnd(v):
    return round(v, PREC[0])


def H(points):
    return [(x, Y(h)) for x, h in points]


def hp(x, h):
    return (x, Y(h))


# ---------------------------------------------------------------- tracés


def smooth(points, closed=True, k=1.0, corners=()):
    """Spline de Catmull-Rom → courbes de Bézier, en commandes relatives
    (chaque décalage part du point courant arrondi : pas d'erreur cumulée)."""
    n = len(points)
    cur = (rnd(points[0][0]), rnd(points[0][1]))
    d = [f"M{P(cur)}c"]
    nums = []
    rng_ = range(n) if closed else range(n - 1)
    for i in rng_:
        p0 = points[(i - 1) % n] if (closed or i > 0) else points[i]
        p1 = points[i]
        p2 = points[(i + 1) % n]
        p3 = points[(i + 2) % n] if (closed or i + 2 < n) else points[(i + 1) % n]
        k1 = 0.0 if i in corners else k
        k2 = 0.0 if ((i + 1) % n) in corners else k
        c1 = (p1[0] + (p2[0] - p0[0]) * k1 / 6, p1[1] + (p2[1] - p0[1]) * k1 / 6)
        c2 = (p2[0] - (p3[0] - p1[0]) * k2 / 6, p2[1] - (p3[1] - p1[1]) * k2 / 6)
        end = (rnd(p2[0]), rnd(p2[1]))
        for q in (c1, c2):
            nums += [f(q[0] - cur[0]), f(q[1] - cur[1])]
        nums += [f(end[0] - cur[0]), f(end[1] - cur[1])]
        cur = end
    d.append(join_nums(nums))
    if closed:
        d.append("z")
    return "".join(d)


def sample_curve(points, per=8, closed=True):
    n = len(points)
    out = []
    rng_ = range(n) if closed else range(n - 1)
    for i in rng_:
        p0 = points[(i - 1) % n] if (closed or i > 0) else points[i]
        p1 = points[i]
        p2 = points[(i + 1) % n]
        p3 = points[(i + 2) % n] if (closed or i + 2 < n) else points[(i + 1) % n]
        for s in range(per):
            t = s / per
            t2, t3 = t * t, t * t * t
            out.append(
                tuple(
                    0.5
                    * (
                        2 * p1[c]
                        + (-p0[c] + p2[c]) * t
                        + (2 * p0[c] - 5 * p1[c] + 4 * p2[c] - p3[c]) * t2
                        + (-p0[c] + 3 * p1[c] - 3 * p2[c] + p3[c]) * t3
                    )
                    for c in range(2)
                )
            )
    if not closed:
        out.append(points[-1])
    return out


def inside(pt, polygon):
    x, y = pt
    c = False
    j = len(polygon) - 1
    for i in range(len(polygon)):
        xi, yi = polygon[i]
        xj, yj = polygon[j]
        if (yi > y) != (yj > y) and x < (xj - xi) * (y - yi) / (yj - yi + 1e-12) + xi:
            c = not c
        j = i
    return c


def resample(poly_, step):
    """Points régulièrement espacés le long d'un polygone fermé."""
    out = []
    n = len(poly_)
    acc = 0.0
    for i in range(n):
        a, b = poly_[i], poly_[(i + 1) % n]
        L = math.hypot(b[0] - a[0], b[1] - a[1])
        while acc <= L:
            t = acc / (L + 1e-12)
            out.append((a[0] + (b[0] - a[0]) * t, a[1] + (b[1] - a[1]) * t))
            acc += step
        acc -= L
    return out


def signed_area(poly_):
    return 0.5 * sum(poly_[i][0] * poly_[(i + 1) % len(poly_)][1] - poly_[(i + 1) % len(poly_)][0] * poly_[i][1] for i in range(len(poly_)))


def lumpy(points, rng, step=2.6, amp=1.0, skip=None):
    """Contour bosselé (mèches) : on rééchantillonne la courbe et on pousse
    chaque point vers l'extérieur, en festons irréguliers. `skip(x, y)` :
    vrai là où le contour doit rester lisse."""
    base = resample(sample_curve(points, per=10), step)
    sgn = 1.0 if signed_area(base) > 0 else -1.0
    out = []
    n = len(base)
    phase = 0.0
    for i, (x, y) in enumerate(base):
        a = base[(i - 1) % n]
        b = base[(i + 1) % n]
        tx, ty = b[0] - a[0], b[1] - a[1]
        L = math.hypot(tx, ty) + 1e-9
        # normale extérieure (repère SVG, y vers le bas)
        nx, ny = -ty / L * sgn, tx / L * sgn
        nx, ny = -nx, -ny
        phase += rng.uniform(0.9, 1.5)
        bump = amp * (0.35 + 0.65 * abs(math.sin(phase))) * rng.uniform(0.6, 1.25)
        if skip and skip(x, y):
            bump = 0.0
        out.append((x + nx * bump, y + ny * bump))
    return out


# ---------------------------------------------------------------- SVG

defs = []


def _stops(stops):
    s = []
    for st in stops:
        o, c = st[0], st[1]
        a = st[2] if len(st) > 2 else 1
        s.append(f'<stop offset="{o:g}" stop-color="{c}"' + (f' stop-opacity="{a:g}"' if a < 1 else "") + "/>")
    return "".join(s)


def lin(id_, p1, p2, stops):
    defs.append(
        f'<linearGradient id="{id_}" gradientUnits="userSpaceOnUse" x1="{p1[0]:.2f}" y1="{p1[1]:.2f}" x2="{p2[0]:.2f}" y2="{p2[1]:.2f}">{_stops(stops)}</linearGradient>'
    )
    return f"url(#{id_})"


def rad(id_, c, r, stops, fc=None):
    foc = f' fx="{fc[0]:.2f}" fy="{fc[1]:.2f}"' if fc else ""
    defs.append(
        f'<radialGradient id="{id_}" gradientUnits="userSpaceOnUse" cx="{c[0]:.2f}" cy="{c[1]:.2f}" r="{r:.2f}"{foc}>{_stops(stops)}</radialGradient>'
    )
    return f"url(#{id_})"


def path(d, fill="none", stroke=None, sw=None, op=None, sop=None, rule=None):
    a = [f'<path d="{d}" fill="{fill}"']
    if op is not None and op < 1:
        a.append(f' fill-opacity="{op:.2f}"')
    if rule:
        a.append(f' fill-rule="{rule}"')
    if stroke:
        a.append(f' stroke="{stroke}" stroke-width="{sw:.2f}"')
        if sop is not None and sop < 1:
            a.append(f' stroke-opacity="{sop:.2f}"')
    a.append("/>")
    return "".join(a)


def line(pts, stroke, sw, sop=None):
    return path(smooth(pts, closed=False), stroke=stroke, sw=sw, sop=sop)


def pivot(name, p):
    x, y = p
    if DEBUG:
        return f'<circle cx="{x:.2f}" cy="{y:.2f}" r="1.0" fill="#ff2a6d" stroke="#ffffff" stroke-width="0.35"/>'
    return f'<circle id="pivot-{name}" cx="{x:.2f}" cy="{y:.2f}" r="0.8"/>'


# ---------------------------------------------------------------- mèches


def curl_lock(x, y, ang, length, width, curl=0.0, wave=0.0, n=3):
    """Une mèche torsadée : racine large, pointe effilée qui se recourbe.
    Rend (chemin, bord éclairé, ligne médiane)."""
    pts = [(x, y)]
    for i in range(1, n + 1):
        t = i / n
        th = ang + curl * t**1.7 + wave * math.sin(t * math.pi * 2)
        px, py = pts[-1]
        pts.append((px + length / n * math.cos(th), py + length / n * math.sin(th)))
    left, right = [], []
    for i, (px, py) in enumerate(pts[:-1]):
        t = i / n
        dx, dy = pts[i + 1][0] - px, pts[i + 1][1] - py
        L = math.hypot(dx, dy) + 1e-9
        nx, ny = -dy / L, dx / L
        hw = width / 2 * (1 - t) ** 0.8 * (0.9 + 0.25 * math.sin(math.pi * min(1, t * 1.5)))
        left.append((px + nx * hw, py + ny * hw))
        right.append((px - nx * hw, py - ny * hw))
    outline = left + [pts[-1]] + list(reversed(right))
    d = smooth(outline, corners=(n,))
    edge = smooth(left + [pts[-1]], closed=False)
    return d, edge, pts


# ================================================================ dégradés partagés

TOP, BOT = Y(114), Y(40)
G_LOCK = [
    lin("lk0", (0, TOP), (0, BOT), [(0, "#32261e"), (0.25, "#28211c"), (0.55, "#1f1b18"), (1, "#161412")]),
    lin("lk1", (0, TOP), (0, BOT), [(0, "#3e2c20"), (0.22, "#2c231d"), (0.55, "#211c19"), (1, "#171513")]),
    lin("lk2", (0, TOP), (0, BOT), [(0, "#27221e"), (0.4, "#211e1b"), (0.75, "#1b1816"), (1, "#131210")]),
    lin("lk3", (0, TOP), (0, BOT), [(0, "#2c241d"), (0.3, "#25201c"), (0.65, "#1d1a17"), (1, "#151311")]),
]
G_LOCK_FAR = lin("lkfar", (0, TOP), (0, BOT), [(0, "#211c18"), (0.5, "#181614"), (1, "#11100f")])
G_FLEECE = lin("fleece", (0, Y(112)), (0, Y(42)), [(0, "#1c1815"), (0.4, "#171513"), (1, "#100f0e")])
G_CRAVAT = lin("cravat-g", (0, Y(110)), (0, Y(50)), [(0, "#1c1b1a"), (0.6, "#151414"), (1, "#0e0e0d")])
RUST = ["#6e4a33", "#7e5238", "#5a3a27", "#8a5a3c"]
SHEEN = ["#3a332d", "#423a33", "#302b27"]

# ================================================================ repères

POLL = (157.5, 130.0)
EYE = hp(166.2, 115.6)
COIL = (149.4, 112.2)
JAW_HINGE = hp(174.6, 96.5)
EAR_BASE = hp(158.4, 114.0)
HEAD_PIVOT = hp(150.0, 108.5)
FORE_PIVOT = hp(143.0, 66.0)
HIND_PIVOT = hp(50.0, 70.0)
FAR_FORE_DX = -6.6
FAR_HIND_DX = 6.4
BELLY = 45.0
# Les mèches pendantes du ventre qui balancent, de x0 à x1, entre les pattes
# (`sway-1` à l'avant) ; les autres, vers les jarrets et le bréchet, restent.
BELLY_SWAYS = [(112.0, 130.0), (94.0, 112.0), (76.0, 94.0)]


def belly_sway(x):
    for k, (x0, x1) in enumerate(BELLY_SWAYS):
        if x0 <= x < x1:
            return k + 1
    return 0


# ================================================================ texture de toison


def fleece_texture(region, rng, density=3.1, length=(4.5, 7.5), width=(2.4, 3.4), direction=None, far=False, rust_above=94.0, op_of=None, glint=0.38, sheen=0.7, mane=None):
    g = []
    poly_ = sample_curve(region)
    xs = [p[0] for p in poly_]
    ys = [p[1] for p in poly_]
    pts = []
    yy = min(ys) - 1.0
    row = 0
    while yy < max(ys) + 1:
        xx = min(xs) - 1.5 + (row % 2) * density * 0.5
        while xx < max(xs) + 1.5:
            p = (xx + rng.uniform(-1.0, 1.0), yy + rng.uniform(-0.9, 0.9))
            if inside(p, poly_):
                pts.append(p)
            xx += density
        yy += density * 0.8
        row += 1
    for px, py in sorted(pts, key=lambda p: -p[1]):
        h = GY - py
        a = direction(px, h) if direction else 100.0
        a += rng.uniform(-16, 16)
        ln = rng.uniform(*length)
        wd = rng.uniform(*width)
        curl = math.radians(rng.choice([-1, 1]) * rng.uniform(40, 130))
        m = mane(px, h) if mane else 0.0
        if m > 0 and rng.random() < m:
            # poil de la cravate : long, presque droit, pendant
            a = rng.uniform(84, 98)
            ln = rng.uniform(9.0, 14.0)
            wd = rng.uniform(2.4, 3.4)
            d, edge, cl = curl_lock(px, py, math.radians(a), ln, wd, curl=math.radians(rng.choice([-1, 1]) * rng.uniform(10, 40)), wave=rng.uniform(0.15, 0.3) * rng.choice([-1, 1]), n=4)
            g.append(path(d, fill=G_CRAVAT))
            g.append(path(smooth(cl[0:4], closed=False), stroke=rng.choice(["#3b3936", "#46433f", "#302e2c"]), sw=rng.uniform(0.3, 0.5), sop=0.7))
            continue
        d, edge, cl = curl_lock(px, py, math.radians(a), ln, wd, curl=curl, wave=rng.uniform(-0.15, 0.15))
        op = op_of(px, h) if op_of else 1.0
        if op <= 0.03:
            continue
        fill = G_LOCK_FAR if far else rng.choice(G_LOCK)
        g.append(path(d, fill=fill, op=op))
        if not far and rng.random() < sheen:
            if h > rust_above and rng.random() < glint:
                col, so = rng.choice(RUST), 0.7
            else:
                col, so = rng.choice(SHEEN), 0.7
            g.append(path(edge, stroke=col, sw=rng.uniform(0.35, 0.55), sop=so * op))
    return g


def edge_tufts(outline, rng, step, select, length=(3.0, 5.0), width=(2.2, 3.2), bias=0.0, rust_above=96.0, far=False):
    """Mèches qui dépassent du contour (bord bosselé de la toison).
    `select(x, h)` → vrai si le bord porte des mèches à cet endroit."""
    g = []
    base = resample(outline, step)
    sgn = 1.0 if signed_area(base) > 0 else -1.0
    n = len(base)
    for i, (x, y) in enumerate(base):
        h = GY - y
        if not select(x, h):
            continue
        a = base[(i - 1) % n]
        b = base[(i + 1) % n]
        tx, ty = b[0] - a[0], b[1] - a[1]
        L = math.hypot(tx, ty) + 1e-9
        nx, ny = ty / L * sgn, -tx / L * sgn
        ang = math.atan2(ny, nx)
        # les mèches retombent : on penche la direction vers le bas
        ang = math.atan2(math.sin(ang) + bias, math.cos(ang))
        ang += math.radians(rng.uniform(-18, 18))
        root = (x - nx * 1.6, y - ny * 1.6)
        d, edge, _ = curl_lock(root[0], root[1], ang, rng.uniform(*length), rng.uniform(*width), curl=math.radians(rng.choice([-1, 1]) * rng.uniform(40, 110)))
        g.append(path(d, fill=G_LOCK_FAR if far else rng.choice(G_LOCK)))
        if not far and rng.random() < 0.6:
            col = rng.choice(RUST) if (h > rust_above and rng.random() < 0.55) else rng.choice(SHEEN)
            g.append(path(edge, stroke=col, sw=0.4, sop=0.7))
    return g


# ================================================================ corps

BODY_PTS = [
    (32.6, 101.0),  # racine de la queue, au bout de la ligne du dos
    (36.0, 103.8),
    (40.6, 105.9),
    (47.0, 107.4),
    (55.0, 108.2),  # croupe arrondie
    (64.0, 108.5),  # rein
    (76.0, 108.3),
    (88.0, 107.9),
    (98.0, 107.5),  # très léger creux derrière le garrot
    (106.0, 107.8),
    (112.0, 108.6),  # garrot
    (118.0, 109.7),  # la ligne du cou monte doucement vers la tête
    (125.0, 111.0),
    (132.0, 111.6),
    (138.0, 111.3),
    (143.5, 110.2),  # haut du cou, bien arrondi (caché par la spirale et la laine de la joue)
    (148.4, 108.0),
    (152.2, 104.8),
    (154.6, 100.8),  # gorge, sous la tête
    (156.0, 96.0),
    (157.8, 90.0),
    (159.6, 82.0),
    (160.4, 74.0),  # poitrail, rond et profond
    (159.6, 66.0),
    (157.2, 58.4),
    (153.0, 51.8),
    (147.4, 47.2),
    (140.4, 44.8),  # bréchet
    (130.0, 43.8),
    (118.0, 43.9),
    (106.0, 44.6),
    (95.0, 45.7),
    (85.0, 47.2),
    (77.0, 49.4),
    (71.0, 52.6),  # le ventre remonte vers le flanc (pli du grasset)
    (66.0, 57.0),
    (60.0, 59.4),
    (53.0, 58.6),  # dessous de la cuisse (caché par la patte arrière)
    (45.4, 56.0),
    (38.4, 54.4),
    (33.0, 55.6),
    (29.6, 59.2),  # la croupe descend dans la culotte
    (27.6, 65.0),
    (26.5, 72.0),
    (26.0, 79.4),
    (25.8, 86.6),  # pointe des fesses
    (27.0, 93.0),
    (29.4, 97.8),
]
BODY = H(BODY_PTS)


def body_group():
    rng = random.Random(33)
    g = ['<g id="body" stroke-linecap="round" stroke-linejoin="round">']
    # silhouette bosselée, sauf sous la tête (cachée) et sur la gorge
    outline = lumpy(BODY, rng, step=2.4, amp=1.1, skip=lambda x, y: x > 146 and GY - y > 100)
    g.append(path(smooth(outline), fill=G_FLEECE))

    def direction(x, h):
        a = 100.0
        if x < 40:
            a = 116.0 + (40 - x) * 1.6  # croupe : vers l'arrière
        if x > 146:
            a = 90.0  # poitrail
        if h > 104:
            a -= 20
        return a

    def mane(x, h):
        """0 : laine bouclée ; 1 : poil long de la cravate (devant du cou et
        du poitrail, sous la gorge)."""
        if h < 54 or h > 110:
            return 0.0
        k = (x - 141.0) / 9.0
        return max(0.0, min(1.0, k))

    g += fleece_texture(BODY, rng, density=3.3, length=(4.8, 7.6), width=(2.6, 3.6), direction=direction, mane=mane)
    # le bord : mèches qui dépassent (dos, croupe, ventre, poitrail)
    g += edge_tufts(
        sample_curve(BODY),
        rng,
        2.7,
        select=lambda x, h: not (x > 145 and h > 98) and not (x < 74 and h < 60),
        length=(3.2, 5.6),
        width=(2.3, 3.3),
        bias=0.9,
    )
    # mèches pendantes du ventre, les plus longues ; celles d'entre les pattes
    # balancent, accrochées à leur racine (trois mèches, `sway-1` à l'avant)
    hanging = []
    for i in range(26):
        x = 76 + i * 2.65 + rng.uniform(-0.8, 0.8)
        # elles suivent la ligne du dessous : le ventre, puis le bréchet
        h = BELLY + 2.4 + rng.uniform(-0.5, 0.9) + (0.12 * (x - 128) if x > 128 else 0) + (0.25 * (84 - x) if x < 84 else 0)
        d, edge, _ = curl_lock(x, Y(h), math.radians(rng.uniform(84, 104)), rng.uniform(5.0, 8.0), rng.uniform(2.3, 3.2), curl=math.radians(rng.choice([-1, 1]) * rng.uniform(30, 80)))
        out = [path(d, fill=rng.choice(G_LOCK))]
        if rng.random() < 0.5:
            out.append(path(edge, stroke="#2e2925", sw=0.4, sop=0.7))
        hanging.append((belly_sway(x), x, h, out))
    for k, group in itertools.groupby(hanging, key=lambda m: m[0]):
        group = list(group)
        if k:
            g.append(f'<g id="sway-{k}">')
        for _, _, _, out in group:
            g += out
        if k:
            # l'attache : la racine moyenne des mèches du groupe
            g.append(pivot(f"sway-{k}", hp(sum(m[1] for m in group) / len(group),
                                            sum(m[2] for m in group) / len(group))))
            g.append("</g>")
    # crinière rude sur la nuque, de la base du cou vers la tête
    def crest(x):
        # hauteur de la ligne du dessus du corps en x (interpolée)
        top = [(xx, hh) for xx, hh in BODY_PTS[4:16]]
        for (x1, h1), (x2, h2) in zip(top, top[1:]):
            if x1 <= x <= x2:
                return h1 + (h2 - h1) * (x - x1) / (x2 - x1)
        return top[-1][1]

    for i in range(9):
        x = 110.0 + i * 2.6 + rng.uniform(-0.5, 0.5)
        h = crest(x) - 1.6 + rng.uniform(-0.5, 0.3)
        d, edge, _ = curl_lock(x, Y(h), math.radians(rng.uniform(165, 210)), rng.uniform(3.5, 5.5), rng.uniform(1.9, 2.7), curl=math.radians(rng.choice([-1, 1]) * rng.uniform(30, 90)))
        g.append(path(d, fill=G_LOCK[1]))
        g.append(path(edge, stroke=rng.choice(RUST[:3]), sw=0.4, sop=0.6))
    if not VARIANTE:
        g += cravat(rng)
    g.append(pivot("body", hp(95.0, BELLY)))
    g.append("</g>")
    return "\n".join(g)


def cravat(rng):
    """La cravate du bélier : une crinière de longs poils rudes, ondulés, plus
    sombres et luisants que la toison, de la gorge au bréchet (à hauteur du
    coude). Elle naît dans la laine du cou (aucune bande, aucun bord net) et
    déborde devant le poitrail en mèches inégales."""
    g = []
    roots = []
    body_poly = sample_curve(BODY)
    for i in range(34):
        t = i / 33
        # racines sur le devant et le bas du cou : une bande de 9 unités
        # qui suit le contour, en retrait du bord
        edge_x = 155.4 + 3.8 * math.sin(t * 2.3)
        x = edge_x - rng.uniform(1.0, 9.0) * (1.0 - 0.3 * t)
        h = 106.0 - t * 48.0 + rng.uniform(-1.2, 1.2)
        # toujours dans la toison du cou, à deux unités au moins du bord : la
        # tête qui s'écarte (elle broute, dort) ne découvre aucune racine
        while not (inside((x, Y(h + 2.0)), body_poly) and inside((x + 2.0, Y(h)), body_poly)):
            h -= 0.5
            x -= 0.2
        L = rng.uniform(11.0, 18.0) * (1.0 - 0.45 * t)
        roots.append((x, h, L))
    for x, h, L in sorted(roots, key=lambda r: (r[1], r[0])):
        a = math.radians(rng.uniform(80, 95))
        wd = rng.uniform(2.6, 4.0)
        d, edge, cl = curl_lock(x, Y(h), a, L, wd, curl=math.radians(rng.choice([-1, 1]) * rng.uniform(15, 50)), wave=rng.uniform(0.15, 0.32) * rng.choice([-1, 1]), n=4)
        fill = lin(f"cr{len(defs)}", (x - wd / 2, 0), (x + wd / 2, 0), [(0, "#252423"), (0.45, "#191818"), (1, "#0d0d0c")])
        g.append(path(d, fill=fill))
        # reflet luisant le long de la mèche (poil rude, brillant)
        g.append(path(smooth(cl[0:4], closed=False), stroke=rng.choice(["#3b3936", "#46433f", "#34322f"]), sw=rng.uniform(0.35, 0.55), sop=0.75))
        g.append(path(edge, stroke="#2c2a28", sw=0.3, sop=0.8))
    return g


# ================================================================ queue


def tail_group():
    """Queue courte et fine, laineuse, jamais coupée : elle pend contre les
    fesses, du bout de la ligne du dos, et finit bien au-dessus du jarret."""
    rng = random.Random(7)
    g = ['<g id="tail" stroke-linecap="round" stroke-linejoin="round">']
    root = hp(30.0, 99.6)
    shape = H([(31.6, 101.2), (27.4, 100.0), (24.0, 95.6), (22.4, 89.0), (21.9, 82.4), (22.4, 77.8), (24.2, 76.0), (25.8, 78.0), (26.8, 83.6), (27.8, 90.0), (29.8, 96.0)])
    outline = lumpy(shape, rng, step=2.0, amp=0.7, skip=lambda x, y: x > 27.5)
    g.append(path(smooth(outline), fill=lin("tail-g", hp(21, 0), hp(29, 0), [(0, "#262019"), (0.5, "#1b1815"), (1, "#131110")])))
    for i in range(9):
        h = 98.2 - i * 2.6
        x = 24.8 - 0.14 * i + rng.uniform(-0.5, 0.5)
        d, edge, _ = curl_lock(x, Y(h), math.radians(rng.uniform(98, 125)), rng.uniform(3.0, 4.4), rng.uniform(1.8, 2.5), curl=math.radians(rng.choice([-1, 1]) * rng.uniform(40, 100)))
        g.append(path(d, fill=rng.choice(G_LOCK)))
        if rng.random() < 0.6:
            g.append(path(edge, stroke=rng.choice(SHEEN), sw=0.38, sop=0.7))
    g.append(pivot("tail", root))
    g.append("</g>")
    return "\n".join(g)


# ================================================================ pattes


def hoof(g, name, heel_x, far):
    """Onglon de profil (semelle sur le sol), l'autre doigt juste devant."""
    toe = heel_x + 7.6
    pts = H([(heel_x + 0.3, 4.5), (heel_x + 3.6, 5.0), (toe - 2.2, 4.4), (toe - 0.5, 1.6), (toe + 0.3, 0.0), (heel_x - 0.3, 0.0), (heel_x - 0.6, 1.8)])
    other = [(x + 1.0, y - 0.3) for x, y in pts]
    g.append(path(smooth(other, corners=(3, 4, 5)), fill="#0a0a0b"))
    fill = lin(f"{name}-h", hp(0, 5), hp(0, 0), [(0, "#2a2a2d" if not far else "#18181a"), (0.55, "#171718"), (1, "#0d0d0e")])
    g.append(path(smooth(pts, corners=(4, 5)), fill=fill))
    if not far:
        g.append(line(H([(heel_x + 0.6, 4.1), (heel_x + 3.6, 4.5), (toe - 2.2, 4.0)]), "#3b3b3f", 0.45, 0.7))
        g.append(line(H([(heel_x + 1.0, 2.6), (toe - 1.4, 2.4)]), "#2a2a2d", 0.35, 0.6))
    g.append(path(f"M{P(hp(heel_x, 0.32))}L{P(hp(toe, 0.32))}", stroke="#4a443d", sw=0.5, sop=0.55 if not far else 0.3))


def leg_group(name, kind, dx, far):
    rng = random.Random(len(name) * 31 + int(dx * 10))
    g = [f'<g id="{name}" stroke-linecap="round" stroke-linejoin="round">']
    if kind == "fore":
        x0 = 143.0 + dx
        piv = (FORE_PIVOT[0] + dx, FORE_PIVOT[1])
        bare = H(
            [
                (x0 - 4.0, 58),
                (x0 + 3.6, 58),
                (x0 + 3.3, 40),
                (x0 + 3.5, 31),
                (x0 + 4.2, 27.0),  # avant du genou
                (x0 + 4.0, 23.5),
                (x0 + 3.2, 20),
                (x0 + 3.1, 12),
                (x0 + 3.5, 9.0),  # boulet
                (x0 + 4.7, 6.6),
                (x0 + 6.1, 4.4),  # paturon
                (x0 + 2.2, 4.3),
                (x0 + 0.6, 5.6),
                (x0 - 0.9, 7.8),
                (x0 - 1.2, 10),
                (x0 - 1.0, 14),
                (x0 - 1.0, 20),
                (x0 - 0.9, 24),
                (x0 - 1.5, 27.5),
                (x0 - 1.8, 31),
                (x0 - 3.2, 40),
            ]
        )
        heel = x0 + 1.3
        wool = H(
            [
                (x0 - 6.8, 70),
                (x0 + 5.4, 70),
                (x0 + 6.0, 58),
                (x0 + 5.6, 48),
                (x0 + 5.0, 40),
                (x0 + 4.6, 34.0),
                (x0 + 4.3, 30.2),
                (x0 + 2.6, 28.6),
                (x0 + 0.6, 29.8),
                (x0 - 1.6, 28.4),
                (x0 - 3.4, 30.4),
                (x0 - 4.8, 34.6),
                (x0 - 6.0, 42),
                (x0 - 7.0, 52),
                (x0 - 7.4, 62),
            ]
        )
        top_h, knee_h = 70.0, 28.5
        sheen = [H([(x0 - 0.3, 24.5), (x0 - 0.2, 17), (x0 - 0.2, 11)]), H([(x0 + 1.0, 25.8), (x0 + 2.5, 26.8), (x0 + 3.6, 25.4)])]
        dew = x0 - 0.9
    else:
        # Postérieur : la cuisse et la jambe dans la laine (le grasset en
        # avant, sous le flanc ; la jambe qui redescend en arrière jusqu'au
        # jarret, pointu derrière) ; sous le jarret, le canon vertical ; le
        # paturon incliné vers l'avant. Une seule pièce.
        x0 = 38.0 + dx
        piv = (HIND_PIVOT[0] + dx, HIND_PIVOT[1])
        bare = H(
            [
                (x0 + 1.0, 50.0),  # sous la laine
                (x0 + 16.0, 50.0),
                (x0 + 10.4, 41.0),
                (x0 + 6.4, 35.6),  # devant de la jambe, qui remonte vers le grasset
                (x0 + 4.0, 31.6),  # angle du jarret, devant
                (x0 + 3.0, 29.0),
                (x0 + 2.6, 25.0),
                (x0 + 2.6, 16.0),
                (x0 + 2.9, 11.0),
                (x0 + 3.5, 8.8),  # boulet
                (x0 + 4.7, 6.6),
                (x0 + 6.1, 4.4),  # paturon
                (x0 + 2.0, 4.3),
                (x0 + 0.2, 5.6),
                (x0 - 1.3, 7.8),
                (x0 - 1.8, 10.5),
                (x0 - 2.0, 16.0),
                (x0 - 2.0, 22.0),
                (x0 - 2.2, 26.4),
                (x0 - 3.2, 29.4),
                (x0 - 4.8, 31.6),  # pointe du jarret
                (x0 - 4.6, 34.0),
                (x0 - 3.6, 37.0),  # tendon, creux
                (x0 - 1.8, 41.0),
                (x0 + 0.6, 46.0),
            ]
        )
        heel = x0 - 0.9
        wool = H(
            [
                (x0 + 6.0, 76.0),  # haut, dans la croupe (fondu)
                (x0 + 20.0, 74.0),
                (x0 + 27.0, 66.0),
                (x0 + 30.4, 58.0),  # grasset : il bombe en avant, sous le pli du flanc
                (x0 + 29.0, 52.6),
                (x0 + 24.0, 47.0),
                (x0 + 17.6, 42.6),
                (x0 + 11.6, 39.0),
                (x0 + 9.6, 39.4),
                (x0 + 8.0, 38.2),  # la laine descend jusqu'au jarret
                (x0 + 5.6, 39.0),
                (x0 + 3.2, 37.6),
                (x0 + 0.8, 38.8),
                (x0 - 1.6, 37.4),
                (x0 - 3.4, 38.6),
                (x0 - 4.6, 41.6),
                (x0 - 6.0, 46.0),  # arrière de la cuisse (la culotte), dans le
                (x0 - 7.2, 52.0),  # prolongement de la croupe
                (x0 - 8.2, 58.0),
                (x0 - 6.6, 62.0),  # au-dessus : sous la toison du corps (fondu)
                (x0 - 4.2, 66.0),
                (x0 - 1.0, 70.0),
            ]
        )
        top_h, knee_h = 76.0, 36.0
        sheen = [H([(x0 - 1.2, 25.6), (x0 - 1.2, 18), (x0 - 1.0, 11)]), H([(x0 - 3.8, 33.6), (x0 - 3.6, 31.2), (x0 - 2.0, 29.2)]), H([(x0 + 1.0, 34.0), (x0 + 3.6, 33.0)])]
        dew = x0 - 1.4
    leg_fill = lin(f"{name}-g", hp(x0 - 4, 0), hp(x0 + 5, 0), [(0, "#2a2a2c" if not far else "#18181a"), (0.4, "#18181a" if not far else "#121213"), (1, "#0e0e0f")])
    g.append(path(smooth(bare, corners=(0, 1)), fill=leg_fill))
    if not far:
        for s in sheen:
            g.append(line(s, "#45474c", 0.6, 0.55))
    g.append(path(smooth(H([(dew - 0.2, 8.6), (dew - 1.3, 7.4), (dew - 0.9, 6.5), (dew + 0.4, 7.1)])), fill="#0b0b0b"))
    hoof(g, name, heel, far)
    # laine de l'avant-bras ou de la culotte : opaque en bas, fondue en haut
    wf = "#191614" if not far else "#121110"
    fade = lin(f"{name}-w", hp(0, top_h), hp(0, top_h - 16), [(0, wf, 0.0), (0.6, wf, 0.85), (1, wf)])
    wool_l = lumpy(wool, rng, step=2.2, amp=0.8, skip=lambda x, y: GY - y > top_h - 10)
    g.append(path(smooth(wool_l), fill=fade))

    def op_of(x, h):
        return max(0.0, min(1.0, (top_h - 3.0 - h) / 9.0))

    g += fleece_texture(wool, rng, density=2.8, length=(3.6, 5.4), width=(2.0, 3.0), direction=lambda x, h: 96.0 if kind == "fore" else 104.0, far=far, op_of=op_of, rust_above=999)
    # frange de mèches au genou ou au jarret
    for i in range(5):
        x = (x0 - 4.6 + i * 2.3) if kind == "fore" else (x0 - 3.4 + i * 2.5)
        x += rng.uniform(-0.4, 0.4)
        h = knee_h + 3.4 + rng.uniform(-0.6, 0.6)
        d, edge, _ = curl_lock(x, Y(h), math.radians(rng.uniform(86, 100)), rng.uniform(3.4, 4.6), rng.uniform(1.8, 2.5), curl=math.radians(rng.choice([-1, 1]) * rng.uniform(30, 90)))
        g.append(path(d, fill=G_LOCK_FAR if far else rng.choice(G_LOCK)))
        if not far and rng.random() < 0.5:
            g.append(path(edge, stroke=rng.choice(SHEEN), sw=0.35, sop=0.6))
    g.append(pivot(name, piv))
    g.append("</g>")
    return "\n".join(g)


# ================================================================ tête


def horn_centerline(dx=0.0, dh=0.0, n=80):
    phi0, phi1 = 58.0, 318.0
    pts = []
    for i in range(n + 1):
        t = i / n
        deg = phi0 + (phi1 - phi0) * t
        # rayon : base près du crâne, s'ouvre derrière, se resserre à peine en bas
        r = 15.4 + 3.6 * math.sin(math.radians(min(180.0, (deg - phi0) * 1.12))) + 0.9 * t
        phi = math.radians(deg)
        x = COIL[0] + r * math.cos(phi) + dx
        h = COIL[1] + r * math.sin(phi) + dh
        w = 12.0 * (1 - t) ** 0.62 + 0.35
        # la base s'enfonce dans le crâne : un peu moins large tout au bout
        w *= 0.8 + 0.2 * min(1.0, t / 0.06)
        pts.append(((x, Y(h)), w, phi))
    return pts


def horn_sides(cl, k_out=0.5, k_in=0.5):
    outer, inner = [], []
    for (x, y), w, phi in cl:
        nx, ny = math.cos(phi), -math.sin(phi)
        outer.append((x + nx * w * k_out, y + ny * w * k_out))
        inner.append((x - nx * w * k_in, y - ny * w * k_in))
    return outer, inner


def horn(far=False):
    g = []
    if far:
        # corne du fond : on n'en voit que l'arc supérieur, juste au-dessus
        # de la corne proche, et la pointe sous la mâchoire
        cl = []
        for k, ((x, y), w, phi) in enumerate(horn_centerline()):
            t = k / 80
            ramp = min(1.0, t / 0.3) * (1.0 if t < 0.7 else max(0.0, 1 - (t - 0.7) / 0.2))
            cl.append(((x - 2.0 * ramp, y - 1.8 * ramp), w * 0.96, phi))
        o, i = horn_sides(cl)
        g.append(path(smooth(o + list(reversed(i))), fill="#2a241e"))
        g.append(path(smooth(o, closed=False), stroke="#4a4036", sw=0.5, sop=0.6))
        return g
    cl = horn_centerline()
    o, i = horn_sides(cl)
    # le bout de la base, arrondi vers l'intérieur du crâne (caché par le
    # chignon et le toupet)
    (bx0, by0), w0, p0 = cl[0]
    root = (bx0 + 2.2 * math.sin(p0), by0 + 2.2 * math.cos(p0))
    shape = o + list(reversed(i)) + [root]
    g.append(path(smooth(shape), fill=rad("horn-g", hp(COIL[0] - 1, COIL[1] + 5), 24, [(0, "#2a241f"), (0.5, "#3f362d"), (0.7, "#5a4e41"), (0.86, "#6d5f4e"), (1, "#5e5244")])))
    # face qui tourne vers la tête (moitié intérieure) : plus sombre
    o2, i2 = horn_sides(cl, k_out=-0.08, k_in=0.5)
    g.append(path(smooth(o2 + list(reversed(i2))), fill="#1d1814", op=0.5))
    # base sombre, près du crâne
    base = cl[:18]
    ob, ib = horn_sides(base)
    g.append(path(smooth(ob + list(reversed(ib))), fill=lin("horn-base", base[0][0], base[-1][0], [(0, "#2a241f", 0.9), (1, "#2a241f", 0.0)])))
    # section triangulaire : la facette du dessus, le long du bord extérieur,
    # prend la lumière sur le haut de la spirale
    lit = cl[4:int(len(cl) * 0.62)]
    ol, il = horn_sides(lit, k_out=0.47, k_in=-0.12)
    g.append(path(smooth(ol + list(reversed(il))), fill=lin("horn-lit", lit[0][0], lit[-1][0], [(0, "#8a7a63", 0.0), (0.3, "#8a7a63", 0.45), (0.7, "#8a7a63", 0.3), (1, "#8a7a63", 0.0)])))
    # stries : crêtes claires, sillons sombres, ondulées, serrées vers la base,
    # quelques anneaux de croissance plus profonds
    n = len(cl)
    acc = [0.0]
    for k in range(1, n):
        acc.append(acc[-1] + math.hypot(cl[k][0][0] - cl[k - 1][0][0], cl[k][0][1] - cl[k - 1][0][1]))
    total = acc[-1]
    rng = random.Random(4)
    s = 0.8
    ring = 0
    PREC[0] = 2
    while s < total * 0.87:
        j = next(k for k in range(1, n) if acc[k] >= s)
        t = (s - acc[j - 1]) / (acc[j] - acc[j - 1] + 1e-9)
        (xa, ya), wa, pa = cl[j - 1]
        (xb, yb), wb, pb = cl[j]
        x, y, w, phi = xa + (xb - xa) * t, ya + (yb - ya) * t, wa + (wb - wa) * t, pa + (pb - pa) * t
        nx, ny = math.cos(phi), -math.sin(phi)
        tx, ty = -ny, nx  # vers la pointe
        frac = s / total
        span_o = 0.5 - rng.uniform(0.0, 0.08)
        span_i = 0.5 - rng.uniform(0.02, 0.2)
        a = (x + nx * w * span_o, y + ny * w * span_o)
        b = (x - nx * w * span_i, y - ny * w * span_i)
        bow = 0.45 + 0.17 * w
        wob = rng.uniform(-0.35, 0.35)
        c1 = (x + nx * w * 0.2 + tx * (bow + wob), y + ny * w * 0.2 + ty * (bow + wob))
        c2 = (x - nx * w * 0.2 + tx * (bow - wob), y - ny * w * 0.2 + ty * (bow - wob))
        deep = rng.random() < 0.14
        fade = 1.0 if frac < 0.7 else max(0.2, 1 - (frac - 0.7) / 0.2)
        k_w = 0.7 + 0.3 * w / 12
        contrast = rng.uniform(0.55, 1.0)
        g.append(path(f"M{P(a)}C{P(c1)} {P(c2)} {P(b)}", stroke="#1b1612" if deep else "#2a231d", sw=(0.8 if deep else 0.42) * k_w, sop=0.9 * fade * contrast))
        sh = 0.45 if not deep else 0.6
        a2 = (a[0] + tx * sh, a[1] + ty * sh)
        b2 = (b[0] + tx * sh, b[1] + ty * sh)
        c12 = (c1[0] + tx * sh, c1[1] + ty * sh)
        c22 = (c2[0] + tx * sh, c2[1] + ty * sh)
        g.append(path(f"M{P(a2)}C{P(c12)} {P(c22)} {P(b2)}", stroke="#9e8f78", sw=0.3 * k_w, sop=0.6 * fade * contrast))
        s += (0.8 + 1.6 * frac**1.3) * rng.uniform(0.75, 1.3)
        ring += 1
    # quille du bord d'attaque (en haut et devant)
    keel = horn_sides(cl[2:-8], k_out=0.38, k_in=0)[0]
    g.append(line(keel, "#b3a487", 0.5, 0.45))
    PREC[0] = 1
    # pointe : lisse, plus claire, usée
    tip = cl[-16:]
    ot, it = horn_sides(tip)
    g.append(path(smooth(ot + list(reversed(it))), fill=lin("horn-tip", tip[0][0], tip[-1][0], [(0, "#988870", 0.0), (0.5, "#988870", 0.42), (1, "#a89a82", 0.6)])))
    # contour sombre discret
    g.append(path(smooth(shape), stroke="#15120f", sw=0.45, sop=0.85))
    # la pointe s'écarte de la joue et tourne vers nous : son bord supérieur
    # accroche la lumière
    flare = horn_sides(cl[-18:-1], k_out=0.3, k_in=0)[0]
    g.append(line(flare, "#c9bba0", 0.45, 0.55))
    return g


# La lèvre supérieure (monde) : sous la narine, du bout du nez à la
# commissure, jusque sous la ligne de la bouche ; elle se retrousse autour de
# son arrière pour le flehmen et découvre la gencive (le bourrelet du haut,
# sans incisives), d'un rose sombre sous la peau noire.
LIP = [(181.2, 98.0), (183.4, 98.4), (185.2, 98.75), (186.2, 98.85), (188.6, 98.9), (188.6, 95.4),
       (185.0, 95.1), (182.4, 94.9), (181.2, 94.9)]
LIP_PIVOT = (181.4, 97.8)
GUM = [(181.9, 96.55), (183.6, 96.85), (185.0, 97.2), (185.95, 97.65), (186.3, 97.2), (185.9, 96.95),
       (184.8, 96.05), (183.4, 95.85), (181.9, 95.8)]
GUM_RIDGE = [(182.3, 96.2), (183.8, 96.4), (185.1, 96.65), (185.9, 97.25)]
GUM_FILL, GUM_CREASE = "#a86f7b", "#7d4a57"


def lip(root, paints):
    """La lèvre supérieure, détachée de la tête, avec la gencive qu'elle cache."""
    articulations.lift_part(root, paints, "head", H(LIP), "lip", hp(*LIP_PIVOT),
                            under=articulations.gum(H(GUM), H(GUM_RIDGE), GUM_FILL, GUM_CREASE, 0.3))


def head_shape_points():
    return H(
        [
            (179.4, 95.5),  # 0 commissure (encoche)
            (182.4, 95.5),
            (185.0, 95.7),  # lèvre supérieure
            (186.3, 96.7),
            (187.0, 98.6),  # bout du nez
            (186.9, 100.6),
            (186.0, 102.7),
            (184.0, 105.6),
            (180.6, 109.6),
            (176.0, 114.6),  # chanfrein, léger busqué
            (171.5, 119.4),
            (167.5, 123.3),  # au-dessus de l'œil (orbite)
            (163.5, 126.4),
            (159.0, 129.4),  # chignon
            (154.5, 130.6),
            (149.5, 128.8),
            (145.5, 123.0),  # nuque
            (144.5, 115.0),
            (146.0, 107.0),
            (149.5, 102.4),  # angle de la mâchoire
            (154.0, 99.6),
            (159.5, 96.9),
            (165.0, 94.4),
            (170.0, 92.6),
            (174.2, 92.1),  # 24 fin de l'encoche (bas)
            (173.6, 94.4),  # 25
            (174.6, 96.5),  # 26 charnière
        ]
    )


def head_group():
    g = ['<g id="head" stroke-linecap="round" stroke-linejoin="round">']
    # oreille du fond : seule sa pointe dépasse derrière le chignon
    g.append('<g id="ear-far">')
    g.append(path(smooth(H([(153.0, 125.8), (148.0, 128.4), (143.0, 129.2), (140.4, 127.8), (143.0, 125.8), (148.0, 124.2), (152.6, 122.6)])), fill="#131212"))
    g.append(pivot("ear-far", hp(152.0, 124.0)))
    g.append("</g>")
    if not VARIANTE:
        g += horn(far=True)
    # mâchoire : le bas du museau, et au-dessus l'intérieur de la bouche
    g.append('<g id="jaw">')
    g.append(path(smooth(H([(173.4, 97.6), (177.0, 98.0), (181.0, 97.6), (184.6, 97.0), (185.6, 95.6), (184.0, 94.4), (179.0, 94.2), (174.4, 94.8)])), fill="#22161a"))
    g.append(path(smooth(H([(175.6, 95.0), (179.0, 95.6), (182.4, 95.5), (183.6, 94.9), (181.0, 94.4), (177.0, 94.3)])), fill="#4d3135"))
    jaw = H([(174.6, 96.5), (177.0, 95.9), (179.4, 95.5), (182.4, 95.5), (185.0, 95.6), (185.4, 94.4), (184.4, 92.4), (182.0, 91.2), (178.0, 91.5), (174.2, 92.1), (173.6, 94.4)])
    g.append(path(smooth(jaw, corners=(0, 10)), fill=lin("jaw-g", hp(0, 96), hp(0, 91), [(0, "#202022"), (1, "#0e0e0f")])))
    g.append(line(H([(180.0, 95.1), (182.6, 94.9), (184.4, 94.7)]), "#3a3b3f", 0.35, 0.55))
    g.append(line(H([(176.0, 92.0), (180.0, 91.6), (183.2, 92.0)]), "#2c2d31", 0.4, 0.5))
    g.append(pivot("jaw", JAW_HINGE))
    g.append("</g>")
    hp_ = head_shape_points()
    face = lin("face-g", hp(170.0, 124.0), hp(160.0, 96.0), [(0, "#26272a"), (0.45, "#1b1b1c"), (1, "#111112")])
    g.append(path(smooth(hp_, corners=(0, 24, 25, 26)), fill=face))
    g.append(path(smooth(hp_[0:25], closed=False), stroke="#0a0a0a", sw=0.42, sop=0.55))
    # poil ras : reflets froids sur le chanfrein, la joue, la truffe
    g.append(line(H([(170.5, 119.0), (175.5, 114.4), (180.0, 109.6), (183.4, 105.6)]), "#45474c", 0.9, 0.45))
    g.append(line(H([(164.5, 124.6), (168.0, 122.0)]), "#45474c", 0.7, 0.4))
    g.append(line(H([(168.0, 105.0), (173.5, 101.6), (178.0, 99.6)]), "#3c3e43", 0.8, 0.3))
    g.append(line(H([(184.6, 104.6), (186.2, 102.2)]), "#5a5d66", 0.55, 0.55))
    rng = random.Random(5)
    for k in range(70):
        x = rng.uniform(160, 185)
        h = rng.uniform(93.5, 126)
        if not inside((x, Y(h)), hp_[0:24]):
            continue
        if math.hypot(x - 166.2, (h - 115.6) * 1.3) < 6.8 or x < 158:
            continue
        a = math.radians(rng.uniform(200, 228))
        p0 = (x, Y(h))
        p1 = (x + 1.5 * math.cos(a), Y(h) - 1.5 * math.sin(a))
        g.append(path(f"M{P(p0)}L{P(p1)}", stroke=rng.choice(["#2b2c30", "#303136", "#141415"]), sw=0.28, sop=0.6))
    # narine, sillon de la lèvre, bouche
    g.append(line(H([(186.0, 99.4), (185.4, 100.6), (184.6, 101.4), (183.6, 101.6)]), "#070707", 0.75))
    g.append(line(H([(186.2, 99.2), (186.6, 98.6)]), "#070707", 0.85))
    g.append(line(H([(186.9, 98.0), (186.4, 96.8)]), "#070707", 0.4))
    g.append(line(H([(186.2, 96.4), (185.0, 95.7), (182.4, 95.4), (179.4, 95.5)]), "#060606", 0.55))
    PREC[0] = 2
    g += eyes_group()
    PREC[0] = 1
    # arcade sourcilière : l'orbite saillante (ne cligne pas)
    g.append(line(H([(161.6, 119.4), (164.6, 120.6), (168.2, 120.3), (170.2, 118.8)]), "#45474c", 0.7, 0.38))
    g.append(line(H([(161.0, 112.0), (164.0, 110.6), (168.0, 110.6)]), "#33353a", 0.6, 0.35))
    g += cheek_wool()
    g += topknot()
    g += ear_near()
    # ombre portée de la corne sur la joue : elle ne la touche pas
    cl = horn_centerline()
    o, i = horn_sides(cl[44:], k_out=0.1, k_in=0.5)
    n_s = len(i)
    shadow = [(x + 1.2 + 0.9 * k / n_s, y + 1.4 + 1.0 * k / n_s) for k, (x, y) in enumerate(i)]
    if not VARIANTE:
        g.append(path(smooth(shadow + list(reversed(o))), fill="#000000", op=0.2))
        g += horn()
        g += horn_tufts()
    g.append(pivot("head", HEAD_PIVOT))
    g.append("</g>")
    return "\n".join(g)


def eyes_group():
    g = ['<g id="eyes">']
    ex, ey = EYE
    tilt = math.radians(24)
    ct, st = math.cos(tilt), math.sin(tilt)

    def E(a, b):
        return (ex + a * ct - b * st, ey + a * st + b * ct)

    opening = [E(3.2, 0.2), E(1.75, -1.55), E(-0.4, -1.9), E(-2.35, -1.35), E(-3.2, -0.05), E(-1.85, 1.08), E(0.4, 1.45), E(2.35, 0.92)]
    g.append(path(smooth(opening), fill=rad("eyeball", (ex, ey), 3.7, [(0, "#5a4a36"), (0.65, "#33281d"), (1, "#1a1511")])))
    px, py = ex + 0.15, ey + 0.05
    g.append('<g id="pupils">')
    g.append(f'<circle cx="{px:.2f}" cy="{py:.2f}" r="3.3" fill="{rad("iris-g", (px, py), 3.35, [(0, "#e2cc98"), (0.4, "#d2b884"), (0.72, "#bb9c63"), (0.9, "#8c7650"), (1, "#5a4a33")], fc=(px - 0.7, py - 0.6))}"/>')
    rng = random.Random(9)
    for k in range(20):
        a = 2 * math.pi * k / 20 + rng.uniform(-0.08, 0.08)
        r0, r1 = 1.25, 3.05
        g.append(path(f"M{P((px + r0 * math.cos(a), py + r0 * math.sin(a) * 0.85))}L{P((px + r1 * math.cos(a), py + r1 * math.sin(a)))}", stroke="#a3854f" if k % 2 else "#ead8ad", sw=0.2, sop=0.45))
    g.append(f'<circle cx="{px:.2f}" cy="{py:.2f}" r="3.1" fill="none" stroke="#6b5434" stroke-width="0.35" stroke-opacity="0.7"/>')
    g.append(f'<rect x="{px - 1.85:.2f}" y="{py - 0.56:.2f}" width="3.7" height="1.12" rx="0.56" fill="#0b0b0b"/>')
    g.append(pivot("pupils", (px, py)))
    g.append("</g>")
    lidshade = [E(3.2, 0.2), E(1.75, -1.55), E(-0.4, -1.9), E(-2.35, -1.35), E(-3.2, -0.05), E(-1.6, -0.65), E(0.4, -0.9), E(2.1, -0.5)]
    g.append(path(smooth(lidshade), fill="#2e2316", op=0.6))
    hx, hy = E(-0.9, -0.75)
    g.append(f'<ellipse cx="{hx:.2f}" cy="{hy:.2f}" rx="0.66" ry="0.44" fill="#f4eee0" fill-opacity="0.92" transform="rotate(-18 {hx:.2f} {hy:.2f})"/>')
    hx2, hy2 = E(1.4, 0.55)
    g.append(f'<circle cx="{hx2:.2f}" cy="{hy2:.2f}" r="0.24" fill="#f4eee0" fill-opacity="0.5"/>')
    # cadre couleur du visage : cache l'iris qui déborde quand il regarde de côté
    outer = [E(8.4, 0), E(5.8, -5.8), E(0, -7.4), E(-5.8, -6.0), E(-8.6, 0), E(-5.8, 5.8), E(0, 7.0), E(5.8, 5.4)]
    g.append(path(smooth(outer) + smooth(list(reversed(opening))), fill="url(#face-g)", rule="evenodd"))
    upper = [E(3.3, 0.25), E(1.75, -1.67), E(-0.4, -2.03), E(-2.45, -1.43), E(-3.3, -0.05)]
    g.append(line(upper, "#0e0e0e", 1.0))
    g.append(line([E(2.7, -1.6), E(0.6, -2.9), E(-1.7, -2.75), E(-3.1, -1.5)], "#2e2f33", 0.45, 0.8))
    for a, b, ang in [(1.9, -1.55, -55), (0.9, -1.86, -70), (-0.2, -2.0, -88), (-1.2, -1.86, -104), (-2.1, -1.55, -118), (-2.75, -1.05, -130)]:
        x0, y0 = E(a, b)
        aa = math.radians(ang) + tilt
        g.append(path(f"M{P((x0, y0))}L{P((x0 + 1.0 * math.cos(aa) - 0.25, y0 + 1.0 * math.sin(aa)))}", stroke="#0e0e0e", sw=0.28))
    g.append(line([E(-3.3, -0.05), E(-1.85, 1.15), E(0.4, 1.53), E(2.35, 1.0), E(3.3, 0.25)], "#0e0e0e", 0.5))
    g.append(line([E(-1.4, 1.8), E(0.6, 2.05), E(2.3, 1.55)], "#3a3b3f", 0.35, 0.45))
    g.append(pivot("eyes", (ex, ey)))
    g.append("</g>")
    return g


def cheek_wool():
    """Laine bouclée dans la spirale (autour de l'oreille), derrière l'œil, et
    sur la nuque : elle recouvre le haut du cou."""
    rng = random.Random(11)
    region = H([(161.4, 125.6), (161.8, 119.0), (160.4, 113.0), (158.6, 107.0), (155.0, 101.6), (149.0, 98.4), (141.0, 99.4), (135.0, 104.0), (133.0, 112.0), (134.6, 120.5), (139.0, 127.0), (145.5, 131.0), (152.5, 131.6), (157.5, 130.0)])
    g = [path(smooth(region), fill=lin("cheek-g", hp(0, 131), hp(0, 99), [(0, "#271f19"), (0.5, "#1d1916"), (1, "#141211")]))]
    g += fleece_texture(region, rng, density=2.5, length=(3.0, 4.6), width=(1.9, 2.7), direction=lambda x, h: 110.0 + (150 - x) * 1.2, rust_above=110.0, glint=0.42)
    return g


def topknot():
    rng = random.Random(12)
    region = H([(155.6, 131.4), (160.0, 130.6), (163.6, 128.2), (165.6, 125.6), (164.4, 122.6), (161.6, 121.2), (159.0, 122.8), (156.0, 125.2)])
    g = [path(smooth(region), fill="#201c1a")]
    g += fleece_texture(region, rng, density=2.0, length=(2.2, 3.2), width=(1.5, 2.1), direction=lambda x, h: 150.0, rust_above=120.0, glint=0.35)
    return g


def ear_near():
    """Petite oreille fine, au bout arrondi, portée presque à l'horizontale vers
    l'arrière dans la spirale de la corne. Elle pointe aussi un peu vers nous :
    on voit la conque, sombre, cernée d'un bord de poil ras qui prend la
    lumière."""
    g = ['<g id="ear-near">']
    bx, by = EAR_BASE
    ang = math.radians(198)  # vers l'arrière, un peu vers le bas
    ca_, sa_ = math.cos(ang), math.sin(ang)

    def R(a, b):
        # a : le long de l'oreille ; b : vers le bas de l'oreille
        return (bx + a * ca_ - b * sa_, by + a * sa_ + b * ca_)

    outer = [R(-0.8, -2.8), R(2.0, -3.8), R(5.4, -4.2), R(8.8, -3.8), R(11.6, -2.6), R(13.4, -0.8), R(13.2, 1.0), R(11.0, 2.9), R(7.4, 4.1), R(3.6, 4.0), R(0.4, 2.8)]
    g.append(path(smooth(outer), fill=lin("ear-g", R(0, -4.5), R(0, 4.5), [(0, "#3a3735"), (0.3, "#262423"), (1, "#151414")])))
    # la conque : creux sombre
    concha = [R(1.6, -0.6), R(4.6, -1.8), R(7.8, -1.8), R(10.0, -0.8), R(10.4, 0.8), R(9.0, 2.3), R(6.2, 2.9), R(3.2, 2.6), R(1.4, 1.4)]
    g.append(path(smooth(concha), fill=rad("ear-in", R(5.0, 0.6), 5.6, [(0, "#141212"), (0.45, "#221f1f"), (0.85, "#2a2626"), (1, "#332f2e")])))
    # l'entrée du conduit, au fond de la conque, et deux plis du cartilage
    g.append(line([R(1.8, 0.3), R(3.6, 0.0), R(5.2, 0.2)], "#050505", 0.6, 0.75))
    g.append(line([R(4.6, -1.2), R(7.4, -1.3), R(9.6, -0.5)], "#3c3736", 0.35, 0.5))
    g.append(line([R(5.2, 1.6), R(7.8, 1.5), R(9.6, 0.9)], "#3c3736", 0.35, 0.45))
    # bords du pavillon, accrochés par la lumière (poil ras)
    g.append(line([R(0.2, -3.2), R(3.4, -4.2), R(7.6, -4.1), R(11.0, -2.9), R(13.1, -0.8)], "#5a5653", 0.55, 0.85))
    g.append(line([R(13.0, 0.4), R(11.6, 2.4), R(8.8, 3.8), R(5.0, 4.2), R(2.0, 3.6)], "#45413e", 0.5, 0.8))
    g.append(line([R(1.6, -1.5), R(5.0, -2.8), R(8.6, -2.6), R(10.8, -1.3)], "#3a3634", 0.4, 0.7))
    g.append(pivot("ear-near", (bx, by)))
    g.append("</g>")
    # laine autour de la base de l'oreille (ne bouge pas avec elle)
    rng = random.Random(31)
    for k in range(6):
        x = bx + rng.uniform(-1.5, 1.8)
        y = by + rng.uniform(-2.6, 2.8)
        d, edge, _ = curl_lock(x, y, math.radians(rng.uniform(140, 220)), rng.uniform(2.0, 2.8), rng.uniform(1.5, 2.1), curl=math.radians(rng.choice([-1, 1]) * rng.uniform(40, 110)))
        g.append(path(d, fill=rng.choice([G_LOCK[0], G_LOCK[3]])))
        if rng.random() < 0.5:
            g.append(path(edge, stroke="#5a3a27", sw=0.3, sop=0.65))
    return g


def horn_tufts():
    rng = random.Random(21)
    g = []
    for k in range(11):
        x = rng.uniform(153.5, 163.0)
        h = rng.uniform(123.5, 129.0)
        d, edge, _ = curl_lock(x, Y(h), math.radians(rng.uniform(150, 230)), rng.uniform(1.8, 2.8), rng.uniform(1.3, 2.0), curl=math.radians(rng.choice([-1, 1]) * rng.uniform(40, 120)))
        g.append(path(d, fill=rng.choice([G_LOCK[0], G_LOCK[3]])))
        if rng.random() < 0.5:
            g.append(path(edge, stroke="#5a3a27", sw=0.3, sop=0.7))
    return g


# ================================================================ assemblage

parts = [
    leg_group("leg-back-far", "hind", FAR_HIND_DX, True),
    leg_group("leg-front-far", "fore", FAR_FORE_DX, True),
    tail_group(),
    body_group(),
    leg_group("leg-back-near", "hind", 0.0, False),
    leg_group("leg-front-near", "fore", 0.0, False),
    head_group(),
]
ground = hp((37.0 + 152.0) / 2, 0.0)
svg = [
    '<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 16 200 148" width="200" height="148">',
    "  <!--",
    "    Ouessant : bélier noir de la race d'Ouessant, l'un des plus petits",
    "    moutons du monde, de profil et tourné vers la droite. Rig pour Mouflon.",
    "    © 2026 Alejandro San Martin Lamas — MIT OR Apache-2.0",
    "  -->",
    "<defs>",
    *defs,
    "</defs>",
    *parts,
    pivot("ground", ground),
    "</svg>",
]
# Le dessin coupé à ses articulations (voir tools/races/articulations.py) :
# chaque patte au carpe (devant du genou) ou au jarret (sa pointe) ;
# l'encolure, le haut de l'avant de la toison et la cravate, devant une
# épaule arrondie qui descend du garrot vers le poitrail, posée sous le
# corps ; la tête tourne à la nuque, derrière la spirale de la corne ; la
# lèvre supérieure, détachée de la tête, avec la gencive qu'elle cache.
text = articulations.articulate_breed(
    "\n".join(svg) + "\n",
    carpus_y=Y(26.0),
    hock_y=Y(31.6),
    neck=dict(
        source="body",
        area=articulations.shoulder((122.0, 48.6), (162.0, 86.0), bulge=6.0, far=230.0, top=0.0),
        pivot=(128.0, 92.0),
        poll=(146.0, 44.0),
        muzzle=(183.0, 66.5),
        margin=6.0,
        filler=(1.0, 12.0),
    ),
    extra=lip,
)
if VARIANTE:
    variantes.ecrire(OUT, text, **PROPORTIONS[VARIANTE])
    sys.exit(0)
with open(OUT, "w") as fh:
    fh.write(text)
n_paths = sum(s.count("<path") + s.count("<circle") + s.count("<rect") + s.count("<ellipse") for s in svg)
print(f"écrit {OUT} ({n_paths} formes)")
