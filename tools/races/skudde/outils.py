"""Outils du générateur de la Skudde : repère, courbes, régions, écriture SVG.

Repère de travail : le « monde », en centimètres de l'animal. x vers l'avant
(0 à l'arrière de la fesse), h vers le haut (0 au sol). Dans le SVG, 1 unité
= 1 cm : X = x + OX, Y = OY - h.

Les courbes sont des Catmull-Rom (passant par leurs points) écrites en
Bézier cubiques ; les régions (où semer les poils, ce qui reste d'une forme
coupée par une autre) sont calculées avec shapely, puis écrites en lignes
brisées assez fines pour paraître lisses.

© 2026 Alejandro San Martin Lamas — MIT OR Apache-2.0
"""
import math

from shapely.geometry import LineString, MultiPolygon, Point, Polygon
from shapely.ops import unary_union

OX, OY = 9.0, 82.0


# ------------------------------------------------------------------ nombres

def num(v, nd=2):
    """Un nombre court : « 0.5 » devient « .5 », pas de zéros inutiles."""
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


def nums(vals, nd=2):
    """Des nombres à la suite, sans séparateur inutile."""
    out = []
    prev = ""
    for v in vals:
        t = num(v, nd)
        if out and not t.startswith("-") and not (t.startswith(".") and "." in prev):
            out.append(" ")
        out.append(t)
        prev = t
    return "".join(out)


# ------------------------------------------------------------------ points

def S(p):
    """Du monde au SVG."""
    return (p[0] + OX, OY - p[1])


def W(p):
    """Du SVG au monde."""
    return (p[0] - OX, OY - p[1])


def add(a, b):
    return (a[0] + b[0], a[1] + b[1])


def sub(a, b):
    return (a[0] - b[0], a[1] - b[1])


def mul(a, k):
    return (a[0] * k, a[1] * k)


def lerp(a, b, t):
    return (a[0] + (b[0] - a[0]) * t, a[1] + (b[1] - a[1]) * t)


def dist(a, b):
    return math.hypot(a[0] - b[0], a[1] - b[1])


def norm(a):
    n = math.hypot(a[0], a[1]) or 1.0
    return (a[0] / n, a[1] / n)


def rot(v, a):
    c, s = math.cos(a), math.sin(a)
    return (v[0] * c - v[1] * s, v[0] * s + v[1] * c)


def dot(a, b):
    return a[0] * b[0] + a[1] * b[1]


def clamp(x, a=0.0, b=1.0):
    return max(a, min(b, x))


def smooth(a, b, x):
    t = clamp((x - a) / (b - a)) if b != a else (1.0 if x >= b else 0.0)
    return t * t * (3 - 2 * t)


# ------------------------------------------------------------------ couleurs

def rgb(c):
    c = c.lstrip("#")
    return tuple(int(c[i:i + 2], 16) for i in (0, 2, 4))


def hexc(t):
    return "#%02X%02X%02X" % tuple(max(0, min(255, int(round(v)))) for v in t)


def mix(a, b, t):
    A, B = rgb(a), rgb(b)
    t = clamp(t)
    return hexc(tuple(A[i] + (B[i] - A[i]) * t for i in range(3)))


def shade(c, k):
    """k < 1 assombrit, k > 1 éclaircit vers le blanc."""
    r = rgb(c)
    if k <= 1:
        return hexc(tuple(v * k for v in r))
    t = k - 1
    return hexc(tuple(v + (255 - v) * t for v in r))


def ramp(stops, t):
    """La couleur d'une suite de (position, couleur) en t."""
    if t <= stops[0][0]:
        return stops[0][1]
    for (a, ca), (b, cb) in zip(stops, stops[1:]):
        if t <= b:
            return mix(ca, cb, (t - a) / (b - a) if b > a else 0.0)
    return stops[-1][1]


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


def bez(p0, c1, c2, p3, u):
    a = (1 - u) ** 3
    b = 3 * (1 - u) ** 2 * u
    c = 3 * (1 - u) * u * u
    d = u ** 3
    return (a * p0[0] + b * c1[0] + c * c2[0] + d * p3[0], a * p0[1] + b * c1[1] + c * c2[1] + d * p3[1])


def dense(pts, closed=True, per=10, t=1.0):
    """Les points d'une Catmull-Rom (monde), `per` par segment."""
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


def chains(runs, per=10, t=1.0):
    """Un contour fermé fait de suites lisses jointes par des angles (la
    dernière d'une suite est la première de la suivante), en points."""
    out = []
    for run in runs:
        if len(run) == 2:
            out.append(run[0])
            continue
        pts = dense(run, closed=False, per=per, t=t)
        out.extend(pts[:-1])
    return out


def cr(pts, closed=True, t=1.0, nd=2):
    """Courbe lisse par des points du monde, tracé relatif (c puis s)."""
    P = [S(p) for p in pts]
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
    """Contour fermé fait de plusieurs suites lisses jointes par des angles."""
    q = 10 ** nd

    def r(v):
        return round(v * q) / q

    first = S(runs[0][0])
    cur = (r(first[0]), r(first[1]))
    out = [f"M{nums(cur, nd)}"]
    for run in runs:
        P = [S(p) for p in run]
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
    """Ligne brisée relative par des points du monde."""
    P = [S(p) for p in pts]
    q = 10 ** nd

    def r(v):
        return round(v * q) / q

    cur = (r(P[0][0]), r(P[0][1]))
    out = [f"M{nums(cur, nd)}"]
    flat = []
    for p in P[1:]:
        nxt = (r(p[0]), r(p[1]))
        d = (r(nxt[0] - cur[0]), r(nxt[1] - cur[1]))
        if d == (0, 0):
            continue
        flat += d
        cur = nxt
    if flat:
        out.append("l" + nums(flat, nd))
    if closed:
        out.append("z")
    return "".join(out)


# ------------------------------------------------------------------ régions

def region(pts, closed=True, per=8, t=1.0):
    """Un polygone shapely par une Catmull-Rom fermée (monde)."""
    return Polygon(dense(pts, closed, per, t)).buffer(0)


def region_runs(runs, per=8):
    return Polygon(chains(runs, per)).buffer(0)


def polys_of(geom):
    if geom.is_empty:
        return []
    if isinstance(geom, Polygon):
        return [geom]
    return [g for g in getattr(geom, "geoms", []) if isinstance(g, Polygon)]


def geom_d(geom, nd=2, simplify=0.02):
    """Une région shapely en tracé (lignes brisées)."""
    out = []
    for p in polys_of(geom):
        if simplify:
            p = p.simplify(simplify, preserve_topology=True)
        for ring in [p.exterior, *p.interiors]:
            c = list(ring.coords)[:-1]
            if len(c) < 3:
                continue
            out.append(poly(c, True, nd))
    return "".join(out)


def inside(geom, p):
    return geom.contains(Point(p))


def poisson(geom, r, rng, k=18, limit=None):
    """Des points au hasard dans `geom`, écartés d'au moins `r` (Bridson),
    dans un ordre fixé par `rng`."""
    if geom.is_empty:
        return []
    x0, y0, x1, y1 = geom.bounds
    cell = r / math.sqrt(2)
    nx, ny = int((x1 - x0) / cell) + 2, int((y1 - y0) / cell) + 2
    grid = {}
    pts = []
    active = []
    prepared = geom

    def ok(p):
        gx, gy = int((p[0] - x0) / cell), int((p[1] - y0) / cell)
        for i in range(gx - 2, gx + 3):
            for j in range(gy - 2, gy + 3):
                q = grid.get((i, j))
                if q is not None and dist(p, q) < r:
                    return False
        return True

    def put(p):
        grid[(int((p[0] - x0) / cell), int((p[1] - y0) / cell))] = p
        pts.append(p)
        active.append(p)

    # plusieurs germes, pour les régions en plusieurs morceaux
    seeds = 0
    tries = 0
    while seeds < 6 and tries < 400:
        tries += 1
        p = (rng.uniform(x0, x1), rng.uniform(y0, y1))
        if prepared.contains(Point(p)) and ok(p):
            put(p)
            seeds += 1
    while active:
        i = rng.randrange(len(active))
        c = active[i]
        found = False
        for _ in range(k):
            a = rng.uniform(0, 2 * math.pi)
            d = rng.uniform(r, 2 * r)
            p = (c[0] + d * math.cos(a), c[1] + d * math.sin(a))
            if not (x0 <= p[0] <= x1 and y0 <= p[1] <= y1):
                continue
            if ok(p) and prepared.contains(Point(p)):
                put(p)
                found = True
                break
        if not found:
            active[i] = active[-1]
            active.pop()
        if limit and len(pts) >= limit:
            break
    _ = (nx, ny)
    return pts


# ------------------------------------------------------------------ SVG

class Doc:
    """Le document : ses dégradés et ses éléments, dans l'ordre."""

    def __init__(self):
        self.defs = []
        self.out = []
        self.ids = set()
        self.batches = None

    def emit(self, s):
        self.out.append(s)

    def comment(self, text):
        self.out.append(f"<!-- {text} -->")

    def open(self, gid):
        self.flush()
        self.out.append(f'<g id="{gid}">')

    def close(self):
        self.flush()
        self.out.append("</g>")

    def pivot(self, name, p):
        self.flush()
        X, Y = S(p)
        self.out.append(f'<circle id="pivot-{name}" cx="{num(X)}" cy="{num(Y)}" r="0.8"/>')

    def _stops(self, stops):
        out = []
        for s in stops:
            o, c = s[0], s[1]
            a = s[2] if len(s) > 2 else 1.0
            out.append(f'<stop offset="{num(o, 3)}" stop-color="{c}"'
                       + (f' stop-opacity="{num(a, 3)}"' if a < 0.999 else "") + "/>")
        return "".join(out)

    def lin(self, gid, p1, p2, stops):
        """Dégradé linéaire entre deux points du monde."""
        assert gid not in self.ids, gid
        self.ids.add(gid)
        X1, Y1 = S(p1)
        X2, Y2 = S(p2)
        self.defs.append(f'<linearGradient id="{gid}" gradientUnits="userSpaceOnUse" x1="{num(X1)}" '
                         f'y1="{num(Y1)}" x2="{num(X2)}" y2="{num(Y2)}">{self._stops(stops)}</linearGradient>')
        return f"url(#{gid})"

    def rad(self, gid, c, r, stops, f=None, sx=1.0, angle=0.0):
        """Dégradé radial (monde), éventuellement aplati (`sx` le long de
        `angle`, en radians dans le monde)."""
        assert gid not in self.ids, gid
        self.ids.add(gid)
        X, Y = S(c)
        extra = ""
        if f is not None:
            FX, FY = S(f)
            extra += f' fx="{num(FX)}" fy="{num(FY)}"'
        if abs(sx - 1.0) > 1e-6 or abs(angle) > 1e-6:
            deg = -math.degrees(angle)
            extra += (f' gradientTransform="translate({num(X)} {num(Y)}) rotate({num(deg)}) '
                      f'scale({num(sx, 3)} 1) rotate({num(-deg)}) translate({num(-X)} {num(-Y)})"')
        self.defs.append(f'<radialGradient id="{gid}" gradientUnits="userSpaceOnUse" cx="{num(X)}" cy="{num(Y)}" '
                         f'r="{num(r)}"{extra}>{self._stops(stops)}</radialGradient>')
        return f"url(#{gid})"

    def path(self, d, fill=None, stroke=None, sw=None, fo=None, so=None, cap=None, join=None, rule=None, op=None):
        self.flush()
        self.out.append(element(d, fill, stroke, sw, fo, so, cap, join, rule, op))

    # Des traits fins (poils, grain de la laine) regroupés par couleur, largeur
    # et opacité dans un seul tracé : le fichier en est bien plus léger.
    def begin_batch(self):
        self.flush()
        self.batches = {}

    def stroke(self, d, color, sw, so=1.0):
        key = (color, round(sw, 3), round(so, 2))
        if self.batches is None:
            self.out.append(element(d, None, color, sw, None, so, "round"))
            return
        self.batches.setdefault(key, []).append(d)

    def flush(self):
        if not self.batches:
            self.batches = None
            return
        for (color, sw, so), ds in self.batches.items():
            self.out.append(element("".join(ds), None, color, sw, None, so, "round"))
        self.batches = None

    def render(self, w, h, comment):
        self.flush()
        head = (f'<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 {num(w)} {num(h)}" '
                f'width="{num(w)}" height="{num(h)}">\n<!--\n{comment}\n-->\n')
        return head + "<defs>" + "".join(self.defs) + "</defs>\n" + "\n".join(self.out) + "\n</svg>\n"


def element(d, fill=None, stroke=None, sw=None, fo=None, so=None, cap=None, join=None, rule=None, op=None):
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
    if op is not None and op < 0.999:
        a.append(f' opacity="{num(op, 3)}"')
    a.append("/>")
    return "".join(a)


def qstroke(p, c, e, nd=1):
    """Un trait courbe (Bézier quadratique) du monde, en relatif."""
    P, C, E = S(p), S(c), S(e)
    q = 10 ** nd
    r = lambda v: round(v * q) / q
    x0, y0 = r(P[0]), r(P[1])
    return f"M{nums((x0, y0), nd)}q{nums((r(C[0] - x0), r(C[1] - y0), r(E[0] - x0), r(E[1] - y0)), nd)}"


def lstroke(p, e, nd=1):
    P, E = S(p), S(e)
    q = 10 ** nd
    r = lambda v: round(v * q) / q
    x0, y0 = r(P[0]), r(P[1])
    return f"M{nums((x0, y0), nd)}l{nums((r(E[0] - x0), r(E[1] - y0)), nd)}"


def matched_gradient(doc, gid, stops, n, line_a, line_b, s_a, s_b):
    """Un dégradé linéaire de direction `n` (unitaire, monde) qui prend, le
    long du segment line_a → line_b, les mêmes valeurs (s_a en line_a, s_b en
    line_b) qu'un autre dégradé : deux pièces qui se recouvrent le long de
    ce segment ont alors la même couleur au repos."""
    proj = dot(sub(line_b, line_a), n)
    lam = proj / (s_b - s_a)
    g0 = sub(line_a, mul(n, s_a * lam))
    g1 = add(g0, mul(n, lam))
    return doc.lin(gid, g0, g1, stops)


def union(*gs):
    return unary_union([g for g in gs if g is not None and not g.is_empty])


def line(pts):
    return LineString(pts)


__all__ = [n for n in dir() if not n.startswith("__")] + ["MultiPolygon", "Point", "Polygon"]
