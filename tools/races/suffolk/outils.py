"""Outils du générateur du Suffolk : le repère, l'écriture du SVG (nombres
courts, tracés relatifs), les courbes lisses, les couleurs, les formes
(shapely) et les traits regroupés par style.

Repère de travail : le « monde », en centimètres de l'animal. x vers l'avant
(0 à l'arrière de la fesse, en toison), h vers le haut (0 au sol). Dans le
SVG, 1 unité = 1 cm : X = x + OX, Y = OY - h.

Plusieurs fonctions viennent des générateurs voisins, copiées ici et
adaptées : les courbes de Catmull-Rom écrites en commandes relatives et
l'écriture compacte des nombres (tools/races/nez-noir/gen_nez_noir.py),
le bruit lisse, le déplacement d'un contour, le rééchantillonnage et le
tirage de points écartés (tools/races/jacob/geo.py).

© 2026 Alejandro San Martin Lamas — MIT OR Apache-2.0
"""
import math
import random

from shapely.geometry import LineString, Point, Polygon
from shapely.ops import unary_union
from shapely.prepared import prep

OX, OY = 6.0, 106.0


def S(x, h):
    """Du monde au SVG."""
    return (x + OX, OY - h)


def W(X, Y):
    """Du SVG au monde."""
    return (X - OX, OY - Y)


# ------------------------------------------------------------------ nombres

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
    """Nombres à la suite, sans séparateur inutile (« 1.2-3.4.5 » : 1.2, -3.4, .5)."""
    out = []
    prev = ""
    for v in vals:
        t = num(v, nd)
        if out and not t.startswith("-") and not (t.startswith(".") and "." in prev):
            out.append(" ")
        out.append(t)
        prev = t
    return "".join(out)


# ------------------------------------------------------------------ courbes

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
    """Courbe lisse (Catmull-Rom en Bézier) par des points du monde, en
    commandes relatives ; chaque décalage part du point courant arrondi (pas
    d'erreur qui s'accumule)."""
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
        cur = (r(cur[0] + end[0]), r(cur[1] + end[1]))
    if closed:
        out.append("z")
    return "".join(out)


def runs_path(runs, nd=2, t=1.0):
    """Contour fermé fait de suites lisses jointes par des angles : le dernier
    point d'une suite est le premier de la suivante ; une suite de deux points
    est un segment droit."""
    q = 10 ** nd

    def r(v):
        return round(v * q) / q

    first = S(*runs[0][0])
    cur = (r(first[0]), r(first[1]))
    out = [f"M{nums(cur, nd)}"]
    for run in runs:
        P = [S(x, h) for (x, h) in run]
        n = len(P)
        if n == 2:
            d = [r(P[1][0] - cur[0]), r(P[1][1] - cur[1])]
            out.append("l" + nums(d, nd))
            cur = (r(cur[0] + d[0]), r(cur[1] + d[1]))
            continue
        for i in range(n - 1):
            _, c1, c2, p2 = _ctrl(P, i, n, False, t)
            d = [r(c1[0] - cur[0]), r(c1[1] - cur[1]), r(c2[0] - cur[0]), r(c2[1] - cur[1]),
                 r(p2[0] - cur[0]), r(p2[1] - cur[1])]
            out.append("c" + nums(d, nd))
            cur = (r(cur[0] + d[4]), r(cur[1] + d[5]))
    out.append("z")
    return "".join(out)


def poly(pts, closed=True, nd=2):
    """Une ligne brisée par des points du monde, en commandes relatives."""
    P = [S(x, h) for (x, h) in pts]
    q = 10 ** nd

    def r(v):
        return round(v * q) / q

    cur = (r(P[0][0]), r(P[0][1]))
    out = [f"M{nums(cur, nd)}"]
    flat = []
    for p in P[1:]:
        d = (r(p[0] - cur[0]), r(p[1] - cur[1]))
        if d == (0.0, 0.0):
            continue
        flat += d
        cur = (r(cur[0] + d[0]), r(cur[1] + d[1]))
    if flat:
        out.append("l" + nums(flat, nd))
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
    """La courbe de Catmull-Rom par `pts`, échantillonnée (`per` points par
    segment)."""
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


def dense_runs(runs, per=8, t=1.0):
    """Plusieurs suites lisses mises bout à bout (les coins restent vifs),
    échantillonnées : un contour fermé."""
    out = []
    for run in runs:
        if len(run) == 2:
            seg = [run[0], run[1]]
        else:
            seg = dense(run, closed=False, per=per, t=t)
        if out and math.dist(out[-1], seg[0]) < 1e-6:
            seg = seg[1:]
        out.extend(seg)
    if len(out) > 1 and math.dist(out[0], out[-1]) < 1e-6:
        out = out[:-1]
    return out


# ------------------------------------------------------------------ couleurs

def rgb(hx):
    hx = hx.lstrip("#")
    return tuple(int(hx[i:i + 2], 16) for i in (0, 2, 4))


def hexc(c):
    return "#%02X%02X%02X" % tuple(max(0, min(255, int(round(v)))) for v in c)


def mix(a, b, t):
    A, B = rgb(a), rgb(b)
    t = max(0.0, min(1.0, t))
    return hexc(tuple(A[i] + (B[i] - A[i]) * t for i in range(3)))


def ramp(stops, t):
    """Une couleur le long d'une rampe [(position, couleur)]."""
    t = max(stops[0][0], min(stops[-1][0], t))
    for (a, ca), (b, cb) in zip(stops, stops[1:]):
        if a <= t <= b:
            return mix(ca, cb, (t - a) / (b - a) if b > a else 0.0)
    return stops[-1][1]


def clamp(v, a=0.0, b=1.0):
    return max(a, min(b, v))


def smooth(a, b, v):
    t = clamp((v - a) / (b - a))
    return t * t * (3 - 2 * t)


# ------------------------------------------------------------------ document

class Doc:
    """Le document SVG en cours d'écriture : les dégradés (en coordonnées
    absolues, `userSpaceOnUse`, pour qu'un morceau recoupé garde les
    siens) et les éléments, dans l'ordre de dessin."""

    def __init__(self):
        self.defs = []
        self.out = []
        self.ids = set()

    def _stops(self, stops):
        s = []
        for st in stops:
            o, c = st[0], st[1]
            a = st[2] if len(st) > 2 else 1.0
            s.append(f'<stop offset="{num(o, 3)}" stop-color="{c}"' +
                     (f' stop-opacity="{num(a, 3)}"' if a < 0.999 else "") + "/>")
        return "".join(s)

    def lin(self, gid, p1, p2, stops):
        """Un dégradé linéaire entre deux points du monde."""
        assert gid not in self.ids, gid
        self.ids.add(gid)
        X1, Y1 = S(*p1)
        X2, Y2 = S(*p2)
        self.defs.append(f'<linearGradient id="{gid}" gradientUnits="userSpaceOnUse" x1="{num(X1)}" y1="{num(Y1)}" '
                         f'x2="{num(X2)}" y2="{num(Y2)}">{self._stops(stops)}</linearGradient>')
        return f"url(#{gid})"

    def rad(self, gid, c, r, stops, f=None, sx=1.0, sy=1.0, rot=0.0):
        """Un dégradé radial centré en `c` (monde), de rayon `r`, foyer `f` ;
        `sx`, `sy`, `rot` (degrés, dans le sens du monde) l'étirent en
        ellipse."""
        assert gid not in self.ids, gid
        self.ids.add(gid)
        X, Y = S(*c)
        extra = ""
        if f is not None:
            FX, FY = S(*f)
            extra = f' fx="{num(FX)}" fy="{num(FY)}"'
        tr = ""
        if abs(sx - 1) > 1e-6 or abs(sy - 1) > 1e-6 or abs(rot) > 1e-6:
            # l'ellipse : tournée de -rot dans le SVG (y vers le bas), autour du centre
            tr = (f' gradientTransform="translate({num(X)} {num(Y)}) rotate({num(-rot)}) '
                  f'scale({num(sx, 3)} {num(sy, 3)}) translate({num(-X)} {num(-Y)})"')
        self.defs.append(f'<radialGradient id="{gid}" gradientUnits="userSpaceOnUse" cx="{num(X)}" cy="{num(Y)}" '
                         f'r="{num(r)}"{extra}{tr}>{self._stops(stops)}</radialGradient>')
        return f"url(#{gid})"

    def el(self, d, fill=None, stroke=None, sw=None, fo=None, so=None, cap=None, join=None, rule=None):
        if not d:
            return
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
        self.out.append("".join(a))

    def raw(self, s):
        self.out.append(s)

    def comment(self, text):
        self.out.append(f"<!-- {text} -->")

    def open(self, gid):
        self.out.append(f'<g id="{gid}">')

    def close(self):
        self.out.append("</g>")

    def pivot(self, name, x, h):
        X, Y = S(x, h)
        self.out.append(f'<circle id="pivot-{name}" cx="{num(X)}" cy="{num(Y)}" r="0.8"/>')


# ------------------------------------------------------------------ formes (shapely, en monde)

def shape(pts):
    """Un polygone shapely (monde), réparé s'il le faut."""
    p = Polygon(pts)
    if not p.is_valid:
        p = p.buffer(0)
    return p


def polys_of(g):
    if g is None or g.is_empty:
        return []
    if g.geom_type == "Polygon":
        return [g]
    if g.geom_type in ("MultiPolygon", "GeometryCollection"):
        out = []
        for x in g.geoms:
            out.extend(polys_of(x))
        return out
    return []


def lines_of(g):
    if g is None or g.is_empty:
        return []
    if g.geom_type in ("LineString", "LinearRing"):
        return [g]
    if g.geom_type in ("MultiLineString", "GeometryCollection"):
        out = []
        for x in g.geoms:
            out.extend(lines_of(x))
        return out
    return []


def geom_d(g, nd=2, simplify=0.03, min_area=0.02):
    """Les polygones d'une forme (monde) en un tracé (anneaux intérieurs
    compris : à remplir en `evenodd` ou `nonzero`, ils sont dans l'autre
    sens)."""
    parts = []
    for p in polys_of(g):
        if p.is_empty or p.area < min_area:
            continue
        if simplify:
            p = p.simplify(simplify, preserve_topology=True)
        for ring in [p.exterior, *p.interiors]:
            c = list(ring.coords)[:-1]
            if len(c) >= 3:
                parts.append(poly(c, closed=True, nd=nd))
    return "".join(parts)


def line_d(pts, nd=2):
    return poly(pts, closed=False, nd=nd)


def smooth_line_d(pts, nd=2, t=1.0):
    return cr(pts, closed=False, t=t, nd=nd)


# ------------------------------------------------------------------ traits regroupés

class Marks:
    """Des petits traits regroupés par style (couleur, largeur, opacité) : un
    seul tracé par style, le fichier en est bien plus léger. `emit` les écrit
    dans l'ordre des opacités (les plus légers d'abord)."""

    def __init__(self):
        self.groups = {}
        self.order = []

    def add(self, color, sw, op, d):
        key = (color, round(sw, 3), round(op, 3))
        if key not in self.groups:
            self.groups[key] = []
            self.order.append(key)
        self.groups[key].append(d)

    def emit(self, doc, cap="round", join="round"):
        for key in sorted(self.order, key=lambda k: (k[2], k[1], k[0])):
            color, sw, op = key
            doc.el("".join(self.groups[key]), stroke=color, sw=sw, so=op, cap=cap, join=join)
        self.groups = {}
        self.order = []


class Fills:
    """Des petites formes pleines regroupées par couleur et opacité."""

    def __init__(self):
        self.groups = {}
        self.order = []

    def add(self, color, op, d):
        key = (color, round(op, 3))
        if key not in self.groups:
            self.groups[key] = []
            self.order.append(key)
        self.groups[key].append(d)

    def emit(self, doc):
        for key in sorted(self.order, key=lambda k: (k[1], k[0])):
            color, op = key
            doc.el("".join(self.groups[key]), fill=color, fo=op)
        self.groups = {}
        self.order = []


# ------------------------------------------------------------------ bruit, contours, tirages

class Noise1D:
    """Bruit lisse (somme de sinus) le long d'une abscisse."""

    def __init__(self, seed, octaves):
        r = random.Random(seed)
        self.terms = []
        for wl, amp in octaves:
            for _ in range(2):
                self.terms.append((2 * math.pi / (wl * r.uniform(0.8, 1.25)), amp * r.uniform(0.6, 1.0),
                                   r.uniform(0, 2 * math.pi)))

    def __call__(self, s):
        return sum(a * math.sin(k * s + ph) for k, a, ph in self.terms)


class Noise2D:
    """Bruit lisse à deux dimensions (somme d'ondes planes)."""

    def __init__(self, seed, octaves):
        r = random.Random(seed)
        self.terms = []
        for wl, amp in octaves:
            for _ in range(3):
                a = r.uniform(0, math.pi)
                k = 2 * math.pi / (wl * r.uniform(0.8, 1.25))
                self.terms.append((k * math.cos(a), k * math.sin(a), amp * r.uniform(0.6, 1.0), r.uniform(0, 6.28)))

    def __call__(self, x, h):
        return sum(a * math.sin(kx * x + kh * h + ph) for kx, kh, a, ph in self.terms)


def arclen(pts):
    s = [0.0]
    for i in range(1, len(pts)):
        s.append(s[-1] + math.dist(pts[i - 1], pts[i]))
    return s


def resample(pts, step):
    """Une ligne brisée ouverte rééchantillonnée à pas constant."""
    s = arclen(pts)
    total = s[-1]
    n = max(2, int(total / step))
    out = []
    j = 0
    for i in range(n + 1):
        target = total * i / n
        while j < len(s) - 2 and s[j + 1] < target:
            j += 1
        seg = s[j + 1] - s[j] or 1.0
        t = (target - s[j]) / seg
        out.append((pts[j][0] + (pts[j + 1][0] - pts[j][0]) * t, pts[j][1] + (pts[j + 1][1] - pts[j][1]) * t))
    return out


def normals(pts, closed=True):
    """Les normales d'une ligne (à gauche du sens de parcours, dans le monde :
    vers l'extérieur d'un contour parcouru dans le sens des aiguilles d'une
    montre)."""
    n = len(pts)
    out = []
    for i in range(n):
        a = pts[(i - 1) % n] if closed else pts[max(i - 1, 0)]
        b = pts[(i + 1) % n] if closed else pts[min(i + 1, n - 1)]
        dx, dh = b[0] - a[0], b[1] - a[1]
        L = math.hypot(dx, dh) or 1.0
        out.append((-dh / L, dx / L))
    return out


def poisson(geom, r, rng, tries=30):
    """Des points répartis dans une forme, écartés d'au moins `r`."""
    minx, miny, maxx, maxy = geom.bounds
    cell = r / math.sqrt(2)
    grid = {}
    out = []
    pg = prep(geom)
    n_target = int(geom.area / (r * r) * 1.6) + 10
    attempts = 0
    while attempts < n_target * tries and len(out) < n_target:
        attempts += 1
        x = rng.uniform(minx, maxx)
        y = rng.uniform(miny, maxy)
        gx, gy = int(math.floor(x / cell)), int(math.floor(y / cell))
        ok = True
        for i in range(gx - 2, gx + 3):
            for j in range(gy - 2, gy + 3):
                q = grid.get((i, j))
                if q and (q[0] - x) ** 2 + (q[1] - y) ** 2 < r * r:
                    ok = False
                    break
            if not ok:
                break
        if not ok:
            continue
        if not pg.contains(Point(x, y)):
            continue
        grid[(gx, gy)] = (x, y)
        out.append((x, y))
    return out


def ellipse_pts(cx, ch, rx, rh, rot=0.0, n=16):
    c, s = math.cos(rot), math.sin(rot)
    out = []
    for k in range(n):
        a = 2 * math.pi * k / n
        ex, eh = rx * math.cos(a), rh * math.sin(a)
        out.append((cx + ex * c - eh * s, ch + ex * s + eh * c))
    return out


def union(*geoms):
    return unary_union([g for g in geoms if g is not None and not g.is_empty])


__all__ = [
    "OX", "OY", "S", "W", "num", "nums", "cr", "runs_path", "poly", "bez", "dense", "dense_runs", "rgb", "hexc",
    "mix", "ramp", "clamp", "smooth", "Doc", "shape", "polys_of", "lines_of", "geom_d", "line_d", "smooth_line_d",
    "Marks", "Fills", "Noise1D", "Noise2D", "arclen", "resample", "normals", "poisson", "ellipse_pts", "union",
    "LineString", "Point", "Polygon", "unary_union", "prep",
]
