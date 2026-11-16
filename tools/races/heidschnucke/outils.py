"""Outils du générateur de la Heidschnucke : le repère, l'écriture du
SVG (nombres courts, tracés relatifs), les courbes lisses, les couleurs et les
mèches de laine.

Repère de travail : le « monde », en centimètres de l'animal. x vers l'avant
(0 à l'arrière de la culotte), h vers le haut (0 au sol). Dans le SVG,
1 unité = 1 cm : X = x + OX, Y = OY - h.

Les fonctions viennent de tools/races/thones-et-marthod/outils.py.
Plusieurs fonctions viennent de tools/races/nez-noir/gen_nez_noir.py (les
courbes de Catmull-Rom écrites en commandes relatives, l'écriture compacte des
nombres), copiées ici et adaptées.

© 2026 Alejandro San Martin Lamas — MIT OR Apache-2.0"""
import math

OX, OY = 9.0, 105.0


def S(x, h):
    """Du monde au SVG."""
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
    P = [S(x, h) for (x, h) in pts]
    out = [f"M{nums(P[0], nd)}"]
    for p in P[1:]:
        out.append("L" + nums(p, nd))
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
