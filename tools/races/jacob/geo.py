# Outils de géométrie pour le générateur du dessin du mouton de Jacob.
import math
import random

from shapely.geometry import Polygon, MultiPolygon, LineString, Point, GeometryCollection
from shapely.ops import unary_union


def fm(v):
    s = f"{v:.2f}"
    if "." in s:
        s = s.rstrip("0").rstrip(".")
    if s in ("-0", ""):
        s = "0"
    return s


def pt(p):
    return f"{fm(p[0])} {fm(p[1])}"


def lerp(a, b, t):
    return a + (b - a) * t


def lerp2(p, q, t):
    return (lerp(p[0], q[0], t), lerp(p[1], q[1], t))


def clamp(x, a=0.0, b=1.0):
    return max(a, min(b, x))


def smoothstep(a, b, x):
    t = clamp((x - a) / (b - a))
    return t * t * (3 - 2 * t)


def hexc(c):
    c = c.lstrip("#")
    return tuple(int(c[i:i + 2], 16) for i in (0, 2, 4))


def mixc(a, b, t):
    ra, ga, ba = hexc(a)
    rb, gb, bb = hexc(b)
    t = clamp(t)
    return "#%02x%02x%02x" % (round(lerp(ra, rb, t)), round(lerp(ga, gb, t)), round(lerp(ba, bb, t)))


def cr_point(p0, p1, p2, p3, t):
    t2, t3 = t * t, t * t * t
    return tuple(
        0.5 * (2 * p1[k] + (-p0[k] + p2[k]) * t + (2 * p0[k] - 5 * p1[k] + 4 * p2[k] - p3[k]) * t2
               + (-p0[k] + 3 * p1[k] - 3 * p2[k] + p3[k]) * t3)
        for k in (0, 1)
    )


def cr_sample(pts, closed=True, step=0.45):
    """Catmull-Rom échantillonnée (polyligne)."""
    n = len(pts)
    out = []
    segs = range(n) if closed else range(n - 1)
    for i in segs:
        p0 = pts[(i - 1) % n] if closed else pts[max(i - 1, 0)]
        p1 = pts[i]
        p2 = pts[(i + 1) % n] if closed else pts[i + 1]
        p3 = pts[(i + 2) % n] if closed else pts[min(i + 2, n - 1)]
        k = max(2, int(math.dist(p1, p2) / step))
        for j in range(k):
            out.append(cr_point(p0, p1, p2, p3, j / k))
    if not closed:
        out.append(tuple(pts[-1]))
    return out


def runs_sample(runs, step=0.45):
    """Plusieurs morceaux lisses (ouverts) mis bout à bout : les coins restent vifs."""
    out = []
    for r in runs:
        s = cr_sample(r, closed=False, step=step)
        if out and math.dist(out[-1], s[0]) < 1e-6:
            s = s[1:]
        out.extend(s)
    if math.dist(out[0], out[-1]) < 1e-6:
        out = out[:-1]
    return out


def smooth_d(pts, closed=True):
    n = len(pts)
    d = ["M" + pt(pts[0])]
    segs = range(n) if closed else range(n - 1)
    for i in segs:
        p0 = pts[(i - 1) % n] if closed else pts[max(i - 1, 0)]
        p1 = pts[i]
        p2 = pts[(i + 1) % n] if closed else pts[i + 1]
        p3 = pts[(i + 2) % n] if closed else pts[min(i + 2, n - 1)]
        c1 = (p1[0] + (p2[0] - p0[0]) / 6, p1[1] + (p2[1] - p0[1]) / 6)
        c2 = (p2[0] - (p3[0] - p1[0]) / 6, p2[1] - (p3[1] - p1[1]) / 6)
        d.append("C" + pt(c1) + " " + pt(c2) + " " + pt(p2))
    if closed:
        d.append("Z")
    return "".join(d)


def poly_d(pts, closed=True):
    if not pts:
        return ""
    s = "M" + pt(pts[0]) + "L" + " ".join(pt(p) for p in pts[1:])
    return s + ("Z" if closed else "")


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


def geom_d(g, simplify=0.03):
    parts = []
    for p in polys_of(g):
        if p.is_empty or p.area < 0.02:
            continue
        if simplify:
            p = p.simplify(simplify, preserve_topology=True)
        parts.append(poly_d(list(p.exterior.coords)[:-1]))
        for r in p.interiors:
            parts.append(poly_d(list(r.coords)[:-1]))
    return "".join(parts)


def normals(pts, closed=True):
    n = len(pts)
    out = []
    for i in range(n):
        a = pts[(i - 1) % n] if closed else pts[max(i - 1, 0)]
        b = pts[(i + 1) % n] if closed else pts[min(i + 1, n - 1)]
        dx, dy = b[0] - a[0], b[1] - a[1]
        L = math.hypot(dx, dy) or 1.0
        out.append((dy / L, -dx / L))
    return out


def arclen(pts, closed=True):
    s = [0.0]
    for i in range(1, len(pts)):
        s.append(s[-1] + math.dist(pts[i - 1], pts[i]))
    return s


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
    ns = normals(pts, closed)
    s = arclen(pts, closed)
    out = []
    for i, p in enumerate(pts):
        w = weight(p) if weight else 1.0
        d = noise(s[i]) * amp * w
        out.append((p[0] + ns[i][0] * d, p[1] + ns[i][1] * d))
    return out


def poly(pts):
    p = Polygon(pts)
    if not p.is_valid:
        p = p.buffer(0)
    return p


def bez(p0, p1, p2, p3, t):
    u = 1 - t
    return (u ** 3 * p0[0] + 3 * u * u * t * p1[0] + 3 * u * t * t * p2[0] + t ** 3 * p3[0],
            u ** 3 * p0[1] + 3 * u * u * t * p1[1] + 3 * u * t * t * p2[1] + t ** 3 * p3[1])


def bez_sample(p0, p1, p2, p3, n=60):
    return [bez(p0, p1, p2, p3, i / n) for i in range(n + 1)]


def resample(pts, step):
    """Polyligne ouverte rééchantillonnée à pas constant."""
    s = arclen(pts, False)
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
        out.append(lerp2(pts[j], pts[j + 1], t))
    return out


def tube(center, hw):
    """Polygone d'un tube le long d'une ligne (demi-largeur hw(f), f de 0 à 1)."""
    s = arclen(center, False)
    total = s[-1]
    ns = normals(center, False)
    left, right = [], []
    for i, p in enumerate(center):
        w = hw(s[i] / total)
        left.append((p[0] + ns[i][0] * w, p[1] + ns[i][1] * w))
        right.append((p[0] - ns[i][0] * w, p[1] - ns[i][1] * w))
    return left, right


def band(center, hw, a, b):
    """Bande entre les fractions a et b de la demi-largeur (de -1 à 1)."""
    s = arclen(center, False)
    total = s[-1]
    ns = normals(center, False)
    one, two = [], []
    for i, p in enumerate(center):
        w = hw(s[i] / total)
        one.append((p[0] + ns[i][0] * w * a, p[1] + ns[i][1] * w * a))
        two.append((p[0] + ns[i][0] * w * b, p[1] + ns[i][1] * w * b))
    return one + two[::-1]


def at_frac(center, f):
    s = arclen(center, False)
    total = s[-1]
    target = f * total
    for i in range(1, len(center)):
        if s[i] >= target:
            seg = s[i] - s[i - 1] or 1.0
            t = (target - s[i - 1]) / seg
            p = lerp2(center[i - 1], center[i], t)
            dx = center[i][0] - center[i - 1][0]
            dy = center[i][1] - center[i - 1][1]
            L = math.hypot(dx, dy) or 1.0
            return p, (dx / L, dy / L)
    dx = center[-1][0] - center[-2][0]
    dy = center[-1][1] - center[-2][1]
    L = math.hypot(dx, dy) or 1.0
    return center[-1], (dx / L, dy / L)


def poisson(geom, r, rng, tries=30):
    """Points répartis (tirage avec distance minimale) dans une forme."""
    minx, miny, maxx, maxy = geom.bounds
    cell = r / math.sqrt(2)
    grid = {}
    out = []
    from shapely.prepared import prep
    pg = prep(geom)
    n_target = int(geom.area / (r * r) * 1.6) + 10
    attempts = 0
    while attempts < n_target * tries and len(out) < n_target:
        attempts += 1
        x = rng.uniform(minx, maxx)
        y = rng.uniform(miny, maxy)
        gx, gy = int(x / cell), int(y / cell)
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


def arc_pts(c, r, a0, a1, n=6):
    return [(c[0] + r * math.cos(lerp(a0, a1, i / n)), c[1] + r * math.sin(lerp(a0, a1, i / n))) for i in range(n + 1)]


def curve_d(pts):
    """Courbe ouverte lisse (Catmull-Rom en Bézier)."""
    return smooth_d(pts, closed=False)
