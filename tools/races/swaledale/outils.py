"""Outils du générateur du Swaledale : le repère, l'écriture du SVG (nombres
courts, tracés relatifs), les courbes lisses, les couleurs, le bruit le long
d'un contour, les tirages de points et les traits groupés.

Repère de travail : le « monde », en centimètres de l'animal. x vers l'avant
(0 à l'arrière de la culotte), h vers le haut (0 au sol). Dans le SVG,
1 unité = 1 cm : X = x + OX, Y = OY - h.

Repris de tools/races/thones-et-marthod/outils.py (le document, les courbes
de Catmull-Rom en commandes relatives, l'écriture compacte des nombres) et de
tools/races/jacob/geo.py (le bruit le long d'un contour, le tirage de Poisson,
les marques groupées par teinte), copiés ici et adaptés.

© 2026 Alejandro San Martin Lamas — MIT OR Apache-2.0
"""
import math
import random

from shapely.geometry import MultiPolygon, Point, Polygon
from shapely.prepared import prep

OX, OY = 10.0, 97.0


def S(x, h):
    """Du monde au SVG."""
    return (x + OX, OY - h)


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


def poly(pts, closed=True, nd=2):
    """Une ligne brisée par des points du monde, en commandes relatives."""
    P = [S(x, h) for (x, h) in pts]
    q = 10 ** nd

    def r(v):
        return round(v * q) / q

    cur = (r(P[0][0]), r(P[0][1]))
    out = [f"M{nums(cur, nd)}"]
    rel = []
    for p in P[1:]:
        nxt = (r(p[0]), r(p[1]))
        rel += [r(nxt[0] - cur[0]), r(nxt[1] - cur[1])]
        cur = nxt
    if rel:
        out.append("l" + nums(rel, nd))
    if closed:
        out.append("z")
    return "".join(out)


def geom_d(g, nd=2, simplify=0.02):
    """Le tracé d'une géométrie shapely (dans le monde) : ses polygones, trous
    compris (règle nonzero : les trous sont parcourus dans l'autre sens)."""
    out = []
    for p in polys_of(g):
        if p.is_empty or p.area < 0.004:
            continue
        if simplify:
            p = p.simplify(simplify, preserve_topology=True)
        ext = list(p.exterior.coords)[:-1]
        if signed_area(ext) < 0:
            ext = ext[::-1]
        out.append(poly(ext, nd=nd))
        for ring in p.interiors:
            c = list(ring.coords)[:-1]
            if signed_area(c) > 0:
                c = c[::-1]
            out.append(poly(c, nd=nd))
    return "".join(out)


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


def signed_area(pts):
    n = len(pts)
    return 0.5 * sum(pts[i][0] * pts[(i + 1) % n][1] - pts[(i + 1) % n][0] * pts[i][1] for i in range(n))


def bez(p0, c1, c2, p3, u):
    a = (1 - u) ** 3
    b = 3 * (1 - u) ** 2 * u
    c = 3 * (1 - u) * u * u
    d = u ** 3
    return (a * p0[0] + b * c1[0] + c * c2[0] + d * p3[0], a * p0[1] + b * c1[1] + c * c2[1] + d * p3[1])


def dense(pts, closed=True, per=10, t=1.0):
    """Les points de la courbe lisse, échantillonnée (dans le monde)."""
    n = len(pts)
    segs = n if closed else n - 1
    out = []
    for i in range(segs):
        p1, c1, c2, p2 = _ctrl(pts, i, n, closed, t)
        for k in range(per):
            out.append(bez(p1, c1, c2, p2, k / per))
    if not closed:
        out.append(tuple(pts[-1]))
    return out


def resample(pts, step, closed=False):
    """Une ligne rééchantillonnée à pas constant."""
    P = list(pts) + ([pts[0]] if closed else [])
    acc = [0.0]
    for i in range(1, len(P)):
        acc.append(acc[-1] + math.dist(P[i - 1], P[i]))
    total = acc[-1]
    n = max(2, int(total / step))
    out = []
    j = 0
    for i in range(n + (0 if closed else 1)):
        target = total * i / n
        while j < len(acc) - 2 and acc[j + 1] < target:
            j += 1
        seg = (acc[j + 1] - acc[j]) or 1.0
        u = (target - acc[j]) / seg
        out.append((P[j][0] + (P[j + 1][0] - P[j][0]) * u, P[j][1] + (P[j + 1][1] - P[j][1]) * u))
    return out


def normals(pts, closed=True):
    """La normale de chaque point (tournée de 90° dans le sens trigonométrique
    par rapport à la tangente)."""
    n = len(pts)
    out = []
    for i in range(n):
        a = pts[(i - 1) % n] if closed else pts[max(i - 1, 0)]
        b = pts[(i + 1) % n] if closed else pts[min(i + 1, n - 1)]
        dx, dy = b[0] - a[0], b[1] - a[1]
        L = math.hypot(dx, dy) or 1.0
        out.append((-dy / L, dx / L))
    return out


def outward(pts):
    """Les normales d'un contour fermé, tournées vers l'extérieur."""
    ns = normals(pts, True)
    if signed_area(pts) < 0:
        ns = [(-a, -b) for a, b in ns]
    return [(-a, -b) for a, b in ns]


def vspan(polyl, x):
    """Le bas et le haut d'un contour fermé à l'abscisse x."""
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


def hspan(polyl, h):
    """L'arrière et l'avant d'un contour fermé à la hauteur h."""
    xs = []
    n = len(polyl)
    for i in range(n):
        (x1, h1), (x2, h2) = polyl[i], polyl[(i + 1) % n]
        if (h1 <= h < h2) or (h2 <= h < h1):
            u = (h - h1) / (h2 - h1)
            xs.append(x1 + u * (x2 - x1))
    if len(xs) < 2:
        return None
    return (min(xs), max(xs))


def ellipse_pts(cx, ch, rx, rh, rot=0.0, n=16):
    c, s = math.cos(rot), math.sin(rot)
    out = []
    for k in range(n):
        a = 2 * math.pi * k / n
        ex, eh = rx * math.cos(a), rh * math.sin(a)
        out.append((cx + ex * c - eh * s, ch + ex * s + eh * c))
    return out


def smoothstep(a, b, x):
    t = max(0.0, min(1.0, (x - a) / (b - a)))
    return t * t * (3 - 2 * t)


def lerp(a, b, t):
    return a + (b - a) * t


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


def shade(a, k):
    return hexc(tuple(v * k for v in rgb(a)))


def ramp(t, stops):
    """La couleur à t (0 à 1) d'une suite de (position, couleur)."""
    t = max(0.0, min(1.0, t))
    for (t0, c0), (t1, c1) in zip(stops, stops[1:]):
        if t <= t1:
            return mix(c0, c1, (t - t0) / (t1 - t0) if t1 > t0 else 0.0)
    return stops[-1][1]


# ------------------------------------------------------------------ hasard

class Noise1D:
    """Bruit lisse (somme de sinus) le long d'une abscisse curviligne."""

    def __init__(self, seed, octaves):
        r = random.Random(seed)
        self.terms = []
        for wl, amp in octaves:
            for _ in range(2):
                self.terms.append((2 * math.pi / (wl * r.uniform(0.8, 1.25)), amp * r.uniform(0.6, 1.0),
                                   r.uniform(0, 2 * math.pi)))

    def __call__(self, s):
        return sum(a * math.sin(k * s + ph) for k, a, ph in self.terms)


def displace(pts, noise, amp=1.0, closed=True, weight=None):
    """Un contour bosselé : chaque point poussé le long de sa normale."""
    ns = normals(pts, closed)
    acc = [0.0]
    for i in range(1, len(pts)):
        acc.append(acc[-1] + math.dist(pts[i - 1], pts[i]))
    out = []
    for i, p in enumerate(pts):
        w = weight(p) if weight else 1.0
        d = noise(acc[i]) * amp * w
        out.append((p[0] + ns[i][0] * d, p[1] + ns[i][1] * d))
    return out


def poisson(geom, r, rng, tries=30, density=1.6):
    """Des points répartis dans une forme, à au moins `r` les uns des autres."""
    if geom.is_empty:
        return []
    minx, miny, maxx, maxy = geom.bounds
    cell = r / math.sqrt(2)
    grid = {}
    out = []
    pg = prep(geom)
    n_target = int(geom.area / (r * r) * density) + 6
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
        if not ok or not pg.contains(Point(x, y)):
            continue
        grid[(gx, gy)] = (x, y)
        out.append((x, y))
    return out


def to_poly(pts):
    p = Polygon(pts)
    if not p.is_valid:
        p = p.buffer(0)
    if isinstance(p, MultiPolygon):
        p = max(p.geoms, key=lambda g: g.area)
    return p


# ------------------------------------------------------------------ le document

class Doc:
    """Le SVG en cours d'écriture : ses dégradés et ses éléments, dans
    l'ordre du dessin."""

    def __init__(self):
        self.defs = []
        self.out = []
        self.ids = set()

    def emit(self, s):
        self.out.append(s)

    def comment(self, text):
        self.out.append(f"<!-- {text} -->")

    def open(self, gid=None):
        self.out.append(f'<g id="{gid}">' if gid else "<g>")

    def close(self):
        self.out.append("</g>")

    def _stops(self, stops):
        return "".join(f'<stop offset="{num(o, 3)}" stop-color="{c}"'
                       + (f' stop-opacity="{num(a, 3)}"' if a < 1 else "") + "/>"
                       for (o, c, a) in [s if len(s) == 3 else (s[0], s[1], 1.0) for s in stops])

    def lin(self, gid, p1, p2, stops):
        """Un dégradé linéaire entre deux points du monde (userSpaceOnUse)."""
        assert gid not in self.ids, gid
        self.ids.add(gid)
        X1, Y1 = S(*p1)
        X2, Y2 = S(*p2)
        self.defs.append(f'<linearGradient id="{gid}" gradientUnits="userSpaceOnUse" x1="{num(X1)}" y1="{num(Y1)}" '
                         f'x2="{num(X2)}" y2="{num(Y2)}">{self._stops(stops)}</linearGradient>')
        return f"url(#{gid})"

    def rad(self, gid, c, r, stops, f=None):
        """Un dégradé radial autour d'un point du monde (userSpaceOnUse)."""
        assert gid not in self.ids, gid
        self.ids.add(gid)
        X, Y = S(*c)
        foc = ""
        if f is not None:
            FX, FY = S(*f)
            foc = f' fx="{num(FX)}" fy="{num(FY)}"'
        self.defs.append(f'<radialGradient id="{gid}" gradientUnits="userSpaceOnUse" cx="{num(X)}" cy="{num(Y)}" '
                         f'r="{num(r)}"{foc}>{self._stops(stops)}</radialGradient>')
        return f"url(#{gid})"

    def ell(self, gid, c, rx, ry, angle, stops):
        """Un dégradé radial elliptique (userSpaceOnUse) : centre `c` dans le
        monde, rayons `rx` et `ry`, le grand axe tourné de `angle` (radians,
        dans le monde : positif vers le haut). Sert aux taches douces qui
        modèlent la face et la toison (sans filtre)."""
        assert gid not in self.ids, gid
        self.ids.add(gid)
        X, Y = S(*c)
        ca, sa = math.cos(-angle), math.sin(-angle)
        m = nums((rx * ca, rx * sa, -ry * sa, ry * ca, X, Y), 3)
        self.defs.append(f'<radialGradient id="{gid}" gradientUnits="userSpaceOnUse" cx="0" cy="0" r="1" '
                         f'gradientTransform="matrix({m})">{self._stops(stops)}</radialGradient>')
        return f"url(#{gid})"

    def el(self, d, fill=None, stroke=None, sw=None, fo=None, so=None, cap=None, join=None, rule=None, cls=None):
        """Un tracé ; chaque attribut est écrit sur le tracé lui-même (le
        recoupage des articulations ne lit pas tous ceux d'un groupe)."""
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
        if cls:
            a.append(f' class="{cls}"')
        a.append("/>")
        self.out.append("".join(a))

    def line(self, pts, stroke, sw, so=None, nd=2, t=1.0):
        """Un trait lisse et ouvert, aux bouts ronds."""
        self.el(cr(pts, closed=False, nd=nd, t=t), stroke=stroke, sw=sw, so=so, cap="round", join="round")

    def pivot(self, name, x, h):
        X, Y = S(x, h)
        self.out.append(f'<circle id="pivot-{name}" cx="{num(X)}" cy="{num(Y)}" r="0.8"/>')


class Marks:
    """De petits traits groupés par style (couleur, épaisseur, opacité) : un
    seul tracé par style, le fichier en est bien plus léger. Chaque trait est
    une courbe du second degré (départ, contrôle, arrivée) dans le monde."""

    def __init__(self):
        self.groups = {}

    def add(self, style, p0, c, p1):
        a, b, e = S(*p0), S(*c), S(*p1)
        a = (round(a[0], 1), round(a[1], 1))
        d = f"M{nums(a, 1)}q{nums((round(b[0] - a[0], 1), round(b[1] - a[1], 1), round(e[0] - a[0], 1), round(e[1] - a[1], 1)), 1)}"
        self.groups.setdefault(style, []).append(d)

    def add_d(self, style, d):
        self.groups.setdefault(style, []).append(d)

    def emit(self, doc, order=None):
        keys = sorted(self.groups, key=order) if order else sorted(self.groups, key=lambda k: (k[2], k[0], k[1]))
        for (color, sw, op) in keys:
            doc.el("".join(self.groups[(color, sw, op)]), stroke=color, sw=sw, so=op, cap="round")
        self.groups = {}
