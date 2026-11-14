# Outils pour le générateur du dessin du mouflon : points, splines, tracés, traits de poil.
import math
import random

RNG = random.Random(20261103)


def f2(v):
    s = f"{v:.2f}"
    if "." in s:
        s = s.rstrip("0").rstrip(".")
    if s == "-0":
        s = "0"
    return s


def f1(v):
    s = f"{v:.1f}"
    if s.endswith(".0"):
        s = s[:-2]
    if s == "-0":
        s = "0"
    return s


def P(x, y):
    return (float(x), float(y))


def add(a, b):
    return (a[0] + b[0], a[1] + b[1])


def sub(a, b):
    return (a[0] - b[0], a[1] - b[1])


def mul(a, k):
    return (a[0] * k, a[1] * k)


def lerp(a, b, t):
    return (a[0] + (b[0] - a[0]) * t, a[1] + (b[1] - a[1]) * t)


def length(a):
    return math.hypot(a[0], a[1])


def norm(a):
    l = length(a) or 1.0
    return (a[0] / l, a[1] / l)


def perp(a):
    # rotation de +90° dans le repère de l'écran (x vers la droite, y vers le bas) : (x, y) -> (-y, x)
    return (-a[1], a[0])


def rot(p, ang, c=(0.0, 0.0)):
    s, co = math.sin(ang), math.cos(ang)
    dx, dy = p[0] - c[0], p[1] - c[1]
    return (c[0] + dx * co - dy * s, c[1] + dx * s + dy * co)


def catmull_segments(pts, closed=False, tension=0.5):
    """Catmull-Rom uniforme passant par pts -> liste de segments cubiques (p0, c1, c2, p1)."""
    n = len(pts)
    segs = []
    rng = range(n) if closed else range(n - 1)
    for i in rng:
        p0 = pts[(i - 1) % n] if (closed or i > 0) else pts[i]
        p1 = pts[i]
        p2 = pts[(i + 1) % n]
        p3 = pts[(i + 2) % n] if (closed or i + 2 < n) else pts[(i + 1) % n]
        k = tension / 3.0 * 2.0  # tension 0,5 -> 1/3 de la tangente (Catmull-Rom classique)
        c1 = (p1[0] + (p2[0] - p0[0]) * k / 2.0, p1[1] + (p2[1] - p0[1]) * k / 2.0)
        c2 = (p2[0] - (p3[0] - p1[0]) * k / 2.0, p2[1] - (p3[1] - p1[1]) * k / 2.0)
        segs.append((p1, c1, c2, p2))
    return segs


def smooth_d(pts, closed=True, tension=0.5):
    segs = catmull_segments(pts, closed, tension)
    d = [f"M{f2(segs[0][0][0])} {f2(segs[0][0][1])}"]
    for (_, c1, c2, p1) in segs:
        d.append(f"C{f2(c1[0])} {f2(c1[1])} {f2(c2[0])} {f2(c2[1])} {f2(p1[0])} {f2(p1[1])}")
    if closed:
        d.append("Z")
    return "".join(d)


def spline_pieces(chains, closed=True):
    """chains : une liste de suites de points ; chaque suite est lissée, et elles se
    rejoignent en angles vifs. Le dernier point d'une suite doit être le premier de la suivante."""
    d = []
    first = True
    for ch in chains:
        segs = catmull_segments(ch, False)
        if first:
            d.append(f"M{f2(ch[0][0])} {f2(ch[0][1])}")
            first = False
        for (_, c1, c2, p1) in segs:
            d.append(f"C{f2(c1[0])} {f2(c1[1])} {f2(c2[0])} {f2(c2[1])} {f2(p1[0])} {f2(p1[1])}")
    if closed:
        d.append("Z")
    return "".join(d)


def sample_cubic(p0, c1, c2, p1, n=12):
    out = []
    for i in range(n + 1):
        t = i / n
        mt = 1 - t
        x = mt**3 * p0[0] + 3 * mt * mt * t * c1[0] + 3 * mt * t * t * c2[0] + t**3 * p1[0]
        y = mt**3 * p0[1] + 3 * mt * mt * t * c1[1] + 3 * mt * t * t * c2[1] + t**3 * p1[1]
        out.append((x, y))
    return out


def sample_spline(pts, closed=True, n=12, tension=0.5):
    poly = []
    for seg in catmull_segments(pts, closed, tension):
        s = sample_cubic(*seg, n=n)
        if poly:
            s = s[1:]
        poly.extend(s)
    return poly


def sample_chains(chains, n=12):
    poly = []
    for ch in chains:
        for seg in catmull_segments(ch, False):
            s = sample_cubic(*seg, n=n)
            if poly:
                s = s[1:]
            poly.extend(s)
    return poly


def poly_d(poly, closed=True):
    d = [f"M{f2(poly[0][0])} {f2(poly[0][1])}"]
    for p in poly[1:]:
        d.append(f"L{f2(p[0])} {f2(p[1])}")
    if closed:
        d.append("Z")
    return "".join(d)


def signed_area(poly):
    a = 0.0
    for i in range(len(poly)):
        x0, y0 = poly[i]
        x1, y1 = poly[(i + 1) % len(poly)]
        a += x0 * y1 - x1 * y0
    return a / 2.0


def point_in_poly(p, poly):
    x, y = p
    inside = False
    n = len(poly)
    j = n - 1
    for i in range(n):
        xi, yi = poly[i]
        xj, yj = poly[j]
        if (yi > y) != (yj > y):
            xint = (xj - xi) * (y - yi) / (yj - yi + 1e-12) + xi
            if x < xint:
                inside = not inside
        j = i
    return inside


def bbox(poly):
    xs = [p[0] for p in poly]
    ys = [p[1] for p in poly]
    return min(xs), min(ys), max(xs), max(ys)


def resample(poly, step):
    """Rééchantillonne une polyligne ouverte à pas à peu près constant le long de la ligne."""
    out = [poly[0]]
    acc = 0.0
    for i in range(1, len(poly)):
        a, b = poly[i - 1], poly[i]
        seg = length(sub(b, a))
        while acc + seg >= step:
            t = (step - acc) / seg
            a = lerp(a, b, t)
            out.append(a)
            seg = length(sub(b, a))
            acc = 0.0
        acc += seg
    if length(sub(out[-1], poly[-1])) > 1e-6:
        out.append(poly[-1])
    return out


def cum_length(poly):
    L = [0.0]
    for i in range(1, len(poly)):
        L.append(L[-1] + length(sub(poly[i], poly[i - 1])))
    return L


def hex2rgb(h):
    h = h.lstrip("#")
    return tuple(int(h[i:i + 2], 16) for i in (0, 2, 4))


def rgb2hex(c):
    return "#%02x%02x%02x" % tuple(max(0, min(255, int(round(v)))) for v in c)


def mix(c1, c2, t):
    a, b = hex2rgb(c1), hex2rgb(c2)
    return rgb2hex(tuple(a[i] + (b[i] - a[i]) * t for i in range(3)))


def shade(c, k):
    """k<1 assombrit, k>1 éclaircit vers le blanc."""
    r = hex2rgb(c)
    if k <= 1:
        return rgb2hex(tuple(v * k for v in r))
    t = k - 1
    return rgb2hex(tuple(v + (255 - v) * t for v in r))


class Svg:
    def __init__(self):
        self.parts = []
        self.defs = []
        self._gid = 0

    def emit(self, s):
        self.parts.append(s)

    def gid(self, prefix):
        self._gid += 1
        return f"{prefix}{self._gid}"

    def lin(self, x1, y1, x2, y2, stops, gid=None):
        gid = gid or self.gid("g")
        st = "".join(
            f'<stop offset="{f2(o)}" stop-color="{c}"' + (f' stop-opacity="{f2(a)}"' if a < 1 else "") + "/>"
            for (o, c, a) in [(s if len(s) == 3 else (s[0], s[1], 1.0)) for s in stops]
        )
        self.defs.append(
            f'<linearGradient id="{gid}" gradientUnits="userSpaceOnUse" x1="{f2(x1)}" y1="{f2(y1)}" x2="{f2(x2)}" y2="{f2(y2)}">{st}</linearGradient>'
        )
        return f"url(#{gid})"

    def rad(self, cx, cy, r, stops, fx=None, fy=None, gid=None, transform=None):
        gid = gid or self.gid("r")
        st = "".join(
            f'<stop offset="{f2(o)}" stop-color="{c}"' + (f' stop-opacity="{f2(a)}"' if a < 1 else "") + "/>"
            for (o, c, a) in [(s if len(s) == 3 else (s[0], s[1], 1.0)) for s in stops]
        )
        extra = ""
        if fx is not None:
            extra += f' fx="{f2(fx)}" fy="{f2(fy)}"'
        if transform:
            extra += f' gradientTransform="{transform}"'
        self.defs.append(
            f'<radialGradient id="{gid}" gradientUnits="userSpaceOnUse" cx="{f2(cx)}" cy="{f2(cy)}" r="{f2(r)}"{extra}>{st}</radialGradient>'
        )
        return f"url(#{gid})"

    def path(self, d, fill="none", stroke=None, sw=None, op=None, fop=None, sop=None,
             cap=None, join=None, extra=""):
        a = [f'<path d="{d}"', f'fill="{fill}"']
        if fop is not None and fop < 1:
            a.append(f'fill-opacity="{f2(fop)}"')
        if stroke:
            a.append(f'stroke="{stroke}"')
            if sw is not None:
                a.append(f'stroke-width="{f2(sw)}"')
            if sop is not None and sop < 1:
                a.append(f'stroke-opacity="{f2(sop)}"')
            if cap:
                a.append(f'stroke-linecap="{cap}"')
            if join:
                a.append(f'stroke-linejoin="{join}"')
        if op is not None and op < 1:
            a.append(f'opacity="{f2(op)}"')
        if extra:
            a.append(extra)
        self.emit(" ".join(a) + "/>")

    def circle_pivot(self, name, p):
        self.emit(f'<circle id="pivot-{name}" cx="{f2(p[0])}" cy="{f2(p[1])}" r="0.8"/>')

    def open(self, gid):
        self.emit(f'<g id="{gid}">')

    def close(self):
        self.emit("</g>")

    def grouped_parts(self):
        """Regroupe les suites de traits simples dans un groupe qui porte leurs attributs
        communs (sans id ni opacité : le contrat du rig et la cuisson n'en sont pas changés)."""
        out, run = [], []
        key = ' fill="none"'
        def flush():
            if len(run) >= 3:
                out.append('<g fill="none" stroke-linecap="round">')
                for p in run:
                    out.append("  " + p.replace(' fill="none"', '', 1).replace(' stroke-linecap="round"', '', 1))
                out.append('</g>')
            else:
                out.extend(run)
            run.clear()
        for p in self.parts:
            if p.startswith('<path') and key in p and 'stroke-linecap="round"' in p and 'url(#' not in p:
                run.append(p)
            else:
                flush()
                out.append(p)
        flush()
        return out

    def render(self, w, h, comment=""):
        head = (
            f'<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 {w} {h}" width="{w}" height="{h}">\n'
        )
        if comment:
            head += f"  <!--\n{comment}\n  -->\n"
        head += "  <defs>\n    " + "\n    ".join(self.defs) + "\n  </defs>\n"
        body = "\n".join("  " + p for p in self.grouped_parts())
        return head + body + "\n</svg>\n"


def hair_strokes(svg, region, n, flow, colors, lmin, lmax, wmin, wmax, omin, omax,
                 curl=0.25, rng=RNG, clip=None, cap="round", jitter=0.0, weight=None):
    """Sème de courts traits courbes dans `region` (un polygone), orientés selon `flow(p)`
    (un vecteur unitaire). colors : fonction(p) -> couleur hex, ou liste de couleurs hex.
    Rend le nombre de traits posés."""
    x0, y0, x1, y1 = bbox(region)
    count = 0
    tries = 0
    clip = clip or region
    while count < n and tries < n * 40:
        tries += 1
        p = (rng.uniform(x0, x1), rng.uniform(y0, y1))
        if not point_in_poly(p, region):
            continue
        if weight is not None and rng.random() > weight(p):
            continue
        d = flow(p)
        if jitter:
            a = math.atan2(d[1], d[0]) + rng.uniform(-jitter, jitter)
            d = (math.cos(a), math.sin(a))
        L = rng.uniform(lmin, lmax)
        e = add(p, mul(d, L))
        if not point_in_poly(e, clip):
            # trop long : on raccourcit le trait
            ok = False
            for k in (0.7, 0.5, 0.35):
                e = add(p, mul(d, L * k))
                if point_in_poly(e, clip):
                    ok = True
                    break
            if not ok:
                continue
        m = lerp(p, e, 0.5)
        c = add(m, mul(perp(d), rng.uniform(-curl, curl) * L))
        col = colors(p) if callable(colors) else rng.choice(colors)
        w = rng.uniform(wmin, wmax)
        o = rng.uniform(omin, omax)
        svg.path(f"M{f1(p[0])} {f1(p[1])}Q{f1(c[0])} {f1(c[1])} {f1(e[0])} {f1(e[1])}",
                 stroke=col, sw=w, sop=o, cap=cap)
        count += 1
    return count
