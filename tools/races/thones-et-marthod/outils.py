"""Outils du générateur de la Thônes et Marthod : le repère, l'écriture du
SVG (nombres courts, tracés relatifs), les courbes lisses, les couleurs et les
mèches de laine.

Repère de travail : le « monde », en centimètres de l'animal. x vers l'avant
(0 à l'arrière de la culotte), h vers le haut (0 au sol). Dans le SVG,
1 unité = 1 cm : X = x + OX, Y = OY - h.

Plusieurs fonctions viennent de tools/races/nez-noir/gen_nez_noir.py (les
courbes de Catmull-Rom écrites en commandes relatives, l'écriture compacte des
nombres), copiées ici et adaptées.

© 2026 Alejandro San Martin Lamas — MIT OR Apache-2.0
"""
import math

OX, OY = 9.0, 105.0


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
    P = [S(x, h) for (x, h) in pts]
    out = [f"M{nums(P[0], nd)}"]
    for p in P[1:]:
        out.append("L" + nums(p, nd))
    if closed:
        out.append("z")
    return "".join(out)


def quad(p0, c, p1, nd=1):
    """Un petit arc (une courbe du second degré) : le départ en absolu, le
    reste relatif à lui (plus court)."""
    q = 10 ** nd

    def r(v):
        return round(v * q) / q

    a, b, e = S(*p0), S(*c), S(*p1)
    a = (r(a[0]), r(a[1]))
    return f"M{nums(a, nd)}q{nums((r(b[0] - a[0]), r(b[1] - a[1]), r(e[0] - a[0]), r(e[1] - a[1])), nd)}"


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
        out.append(pts[-1])
    return out


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


def dist_to(polyl, x, h):
    """La distance d'un point au contour (fermé)."""
    best = 1e9
    n = len(polyl)
    for i in range(n):
        (x1, h1), (x2, h2) = polyl[i], polyl[(i + 1) % n]
        dx, dh = x2 - x1, h2 - h1
        L2 = dx * dx + dh * dh or 1e-9
        u = max(0.0, min(1.0, ((x - x1) * dx + (h - h1) * dh) / L2))
        best = min(best, math.hypot(x - x1 - u * dx, h - h1 - u * dh))
    return best


def ellipse_pts(cx, ch, rx, rh, rot=0.0, n=16):
    c, s = math.cos(rot), math.sin(rot)
    out = []
    for k in range(n):
        a = 2 * math.pi * k / n
        ex, eh = rx * math.cos(a), rh * math.sin(a)
        out.append((cx + ex * c - eh * s, ch + ex * s + eh * c))
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


def shade(a, k):
    return hexc(tuple(v * k for v in rgb(a)))


def ramp(t, stops):
    """La couleur à t (0 à 1) d'une suite de (position, couleur)."""
    t = max(0.0, min(1.0, t))
    for (t0, c0), (t1, c1) in zip(stops, stops[1:]):
        if t <= t1:
            return mix(c0, c1, (t - t0) / (t1 - t0) if t1 > t0 else 0.0)
    return stops[-1][1]


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

    def open(self, gid=None, **attrs):
        a = "".join(f' {k.replace("_", "-")}="{v}"' for k, v in attrs.items())
        self.out.append(f'<g id="{gid}"{a}>' if gid else f"<g{a}>")

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
        dans le SVG : positif vers le bas à droite). Sert aux taches douces
        qui modèlent la face (sans filtre)."""
        assert gid not in self.ids, gid
        self.ids.add(gid)
        X, Y = S(*c)
        ca, sa = math.cos(angle), math.sin(angle)
        m = nums((rx * ca, rx * sa, -ry * sa, ry * ca, X, Y), 3)
        self.defs.append(f'<radialGradient id="{gid}" gradientUnits="userSpaceOnUse" cx="0" cy="0" r="1" '
                         f'gradientTransform="matrix({m})">{self._stops(stops)}</radialGradient>')
        return f"url(#{gid})"

    def el(self, d, fill=None, stroke=None, sw=None, fo=None, so=None, cap=None, join=None, rule=None):
        """Un tracé ; chaque attribut est écrit sur le tracé lui-même (le
        recoupage des articulations ne lit pas ceux d'un groupe)."""
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

    def line(self, pts, stroke, sw, so=None, nd=2, t=1.0):
        """Un trait lisse et ouvert, aux bouts ronds."""
        self.el(cr(pts, closed=False, nd=nd, t=t), stroke=stroke, sw=sw, so=so, cap="round", join="round")

    def pivot(self, name, x, h):
        X, Y = S(x, h)
        self.out.append(f'<circle id="pivot-{name}" cx="{num(X)}" cy="{num(Y)}" r="0.8"/>')


# ------------------------------------------------------------------ mèches

def ringlet(x0, h0, L, w, ang, rng, pitch=1.15, bend=0.0, hook=0.0, taper=0.3, step=0.5):
    """Une mèche en tire-bouchon serré qui pend de (x0, h0) : `ang` est son
    angle compté depuis la verticale (positif : vers l'avant), `bend` sa
    courbure le long de la mèche, `hook` la pointe qui se recourbe (radians).

    Rend (contour, ombres, lumières) : le contour, un tube aux festons
    alternés (un feston par tour de chaque côté, décalés d'un demi-tour : la
    torsade) ; puis, pour chaque tour (une « perle » de la spirale, comme un
    fil de téléphone), le creux d'ombre sous la perle et le reflet sur son
    dessus, des arcs (s, a, c, b) en travers de la mèche, un peu penchés (s :
    où ils sont le long de la mèche)."""
    n = max(4, int(round(L / step)))
    ds = L / n
    x, h = x0, h0
    C = []
    for i in range(n + 1):
        s = i * ds
        fr = s / L
        th = ang + bend * fr + hook * max(0.0, (fr - 0.7) / 0.3) ** 1.6
        C.append((x, h, th, s))
        x += ds * math.sin(th)
        h -= ds * math.cos(th)
    ph = rng.uniform(0, 1)

    def half(s):
        fr = s / L
        hw = w / 2 * (1.0 - taper * fr ** 1.3)
        if fr > 0.78:
            hw *= 1.0 - 0.7 * ((fr - 0.78) / 0.22) ** 1.8
        return hw

    def axis(s):
        j = min(n - 1, max(0, int(s / ds)))
        u = (s - j * ds) / ds
        (x1, h1, t1, _), (x2, h2, t2, _) = C[j], C[j + 1]
        return (x1 + (x2 - x1) * u, h1 + (h2 - h1) * u, t1 + (t2 - t1) * u)

    def edge(s, side, frac=1.0):
        cx, ch, th = axis(max(0.0, min(L, s)))
        hw = half(max(0.0, min(L, s))) * frac
        return (cx + math.cos(th) * hw * side, ch + math.sin(th) * hw * side)

    left, right = [], []
    for (cx, ch, th, s) in C:
        hw = half(s)
        k = 2 * math.pi * (s / pitch + ph)
        bl = hw * (1.0 + 0.22 * max(0.0, math.cos(k)) ** 2)
        br = hw * (1.0 + 0.22 * max(0.0, math.cos(k + math.pi)) ** 2)
        nx, nh = math.cos(th), math.sin(th)       # vers l'avant de la mèche
        right.append((cx + nx * br, ch + nh * br))
        left.append((cx - nx * bl, ch - nh * bl))
    xe, he, the, _ = C[-1]
    tip = (xe + math.sin(the) * 0.2, he - math.cos(the) * 0.2)
    th0 = C[0][2]
    cap = (x0 - math.sin(th0) * 0.45, h0 + math.cos(th0) * 0.45)
    outline = [cap] + right + [tip] + left[::-1]
    hand = 1 if rng.random() < 0.5 else -1
    darks, lights = [], []
    s = ph * pitch
    while s < L - 0.35:
        # l'ombre sous la perle : un sourire d'un bord à l'autre, le bord
        # « avant » de la spire un peu plus bas (la torsade)
        a = edge(s + 0.42 * pitch, -hand, 0.9)
        b = edge(s + 0.62 * pitch, hand, 0.9)
        mx, mh, _ = axis(min(L, s + 0.78 * pitch))
        darks.append((s, a, (2 * mx - (a[0] + b[0]) / 2, 2 * mh - (a[1] + b[1]) / 2), b))
        # le reflet sur le dessus de la perle, plus court, vers la lumière
        if s + 0.1 * pitch < L - 0.5:
            a = edge(s + 0.12 * pitch, -hand, 0.55)
            b = edge(s + 0.24 * pitch, hand, 0.35)
            mx, mh, _ = axis(max(0.0, s + 0.02 * pitch))
            lights.append((s, a, (2 * mx - (a[0] + b[0]) / 2, 2 * mh - (a[1] + b[1]) / 2), b))
        s += pitch
    return outline, darks, lights
