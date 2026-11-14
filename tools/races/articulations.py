#!/usr/bin/env python3
"""Découpe un dessin de Mouflon à ses articulations.

Un générateur de race écrit son dessin d'une pièce par patte, avec l'encolure
dans le corps (ou dans la tête). Ce module le recoupe pour le moteur :

- chaque patte en deux segments (`leg-front-near` et, dedans,
  `leg-front-near-lower`…), au carpe ou au jarret ;
- l'encolure (`neck`), prise dans le corps, qui porte la tête.

La coupe d'une articulation est faite pour que rien ne s'ouvre quand elle
tourne : d'un côté d'une droite passant par le pivot, plus un disque centré
sur le pivot. Le disque reste avec la pièce dessinée par-dessus (le haut de
la patte, l'encolure) ; l'autre pièce s'arrête sur son bord, un arc du même
cercle, qui reste sur ce cercle quand elle tourne autour du pivot. Au repos,
le dessin est le même qu'avant, à l'anticrénelage près :

- la pièce du dessus garde tout ce qui est dans sa région, dans l'ordre ;
- la pièce du dessous garde le reste ; ses remplissages opaques débordent
  sous le disque (cachés au repos), pour qu'aucune jointure ne laisse passer
  le jour ; ses traits et ses tracés transparents s'arrêtent net (rien n'est
  dessiné deux fois).

Il faut shapely (`pip install shapely`). Les dégradés en `objectBoundingBox`
sont recopiés en coordonnées absolues (un morceau coupé n'a plus la même
boîte). Voir docs/rig.md pour le contrat des parties.

© 2026 Alejandro San Martin Lamas — MIT OR Apache-2.0
"""
import math
import re
import xml.etree.ElementTree as ET

from shapely.geometry import (GeometryCollection, LinearRing, LineString, MultiLineString,
                              MultiPolygon, Point, Polygon, box)
from shapely.ops import unary_union

SVG_NS = "http://www.w3.org/2000/svg"
ET.register_namespace("", SVG_NS)
Q = "{%s}" % SVG_NS

# Finesse de l'échantillonnage des courbes (unités du dessin), et de quoi
# simplifier les morceaux coupés (0,03 unité : 0,3 mm de l'animal).
STEP = 0.12
SIMPLIFY = 0.03


# ------------------------------------------------------------------ tracés

def _tokens(d):
    for m in re.finditer(r"[MmLlHhVvCcSsQqTtAaZz]|[-+]?(?:\d+\.?\d*|\.\d+)(?:[eE][-+]?\d+)?", d):
        yield m.group(0)


def _arc_points(p0, rx, ry, phi, large, sweep, p1):
    """Échantillonne un arc elliptique SVG (paramétrisation par le centre)."""
    x1, y1 = p0
    x2, y2 = p1
    if rx == 0 or ry == 0:
        return [p1]
    phi = math.radians(phi)
    cp, sp = math.cos(phi), math.sin(phi)
    dx, dy = (x1 - x2) / 2, (y1 - y2) / 2
    x1p, y1p = cp * dx + sp * dy, -sp * dx + cp * dy
    rx, ry = abs(rx), abs(ry)
    lam = (x1p / rx) ** 2 + (y1p / ry) ** 2
    if lam > 1:
        rx *= math.sqrt(lam)
        ry *= math.sqrt(lam)
    num = rx * rx * ry * ry - rx * rx * y1p * y1p - ry * ry * x1p * x1p
    den = rx * rx * y1p * y1p + ry * ry * x1p * x1p
    k = math.sqrt(max(0.0, num / den)) if den else 0.0
    if large == sweep:
        k = -k
    cxp, cyp = k * rx * y1p / ry, -k * ry * x1p / rx
    cx = cp * cxp - sp * cyp + (x1 + x2) / 2
    cy = sp * cxp + cp * cyp + (y1 + y2) / 2

    def ang(u, v):
        a = math.atan2(u[0] * v[1] - u[1] * v[0], u[0] * v[0] + u[1] * v[1])
        return a

    t1 = ang((1, 0), ((x1p - cxp) / rx, (y1p - cyp) / ry))
    dt = ang(((x1p - cxp) / rx, (y1p - cyp) / ry), ((-x1p - cxp) / rx, (-y1p - cyp) / ry))
    if not sweep and dt > 0:
        dt -= 2 * math.pi
    elif sweep and dt < 0:
        dt += 2 * math.pi
    n = max(2, int(abs(dt) * max(rx, ry) / STEP))
    pts = []
    for i in range(1, n + 1):
        t = t1 + dt * i / n
        x, y = rx * math.cos(t), ry * math.sin(t)
        pts.append((cp * x - sp * y + cx, sp * x + cp * y + cy))
    return pts


def _cubic(p0, c1, c2, p1):
    length = math.dist(p0, c1) + math.dist(c1, c2) + math.dist(c2, p1)
    n = max(2, int(length / STEP))
    out = []
    for i in range(1, n + 1):
        t = i / n
        u = 1 - t
        out.append((u ** 3 * p0[0] + 3 * u * u * t * c1[0] + 3 * u * t * t * c2[0] + t ** 3 * p1[0],
                    u ** 3 * p0[1] + 3 * u * u * t * c1[1] + 3 * u * t * t * c2[1] + t ** 3 * p1[1]))
    return out


def _quad(p0, c, p1):
    return _cubic(p0, (p0[0] + 2 / 3 * (c[0] - p0[0]), p0[1] + 2 / 3 * (c[1] - p0[1])),
                  (p1[0] + 2 / 3 * (c[0] - p1[0]), p1[1] + 2 / 3 * (c[1] - p1[1])), p1)


def parse_d(d):
    """Les sous-tracés d'un attribut `d` : [(points, fermé)]."""
    toks = list(_tokens(d))
    i = 0
    cmd = None
    cur = (0.0, 0.0)
    start = cur
    last_ctrl = None
    subs = []
    pts = []

    def num():
        nonlocal i
        v = float(toks[i])
        i += 1
        return v

    def flush(closed):
        nonlocal pts
        if len(pts) > 1:
            subs.append((pts, closed))
        pts = []

    while i < len(toks):
        t = toks[i]
        if re.match(r"[A-Za-z]", t):
            cmd = t
            i += 1
            if cmd in "Zz":
                flush(True)
                cur = start
                pts = []
                last_ctrl = None
                continue
        rel = cmd.islower()
        c = cmd.upper()
        ox, oy = cur if rel else (0.0, 0.0)
        if c == "M":
            flush(False)
            cur = (num() + ox, num() + oy)
            start = cur
            pts = [cur]
            cmd = "l" if rel else "L"
            last_ctrl = None
        elif c == "L":
            cur = (num() + ox, num() + oy)
            pts.append(cur)
            last_ctrl = None
        elif c == "H":
            cur = (num() + ox, cur[1])
            pts.append(cur)
            last_ctrl = None
        elif c == "V":
            cur = (cur[0], num() + (cur[1] if rel else 0.0))
            pts.append(cur)
            last_ctrl = None
        elif c == "C":
            c1 = (num() + ox, num() + oy)
            c2 = (num() + ox, num() + oy)
            p1 = (num() + ox, num() + oy)
            pts += _cubic(cur, c1, c2, p1)
            cur, last_ctrl = p1, c2
        elif c == "S":
            c1 = (2 * cur[0] - last_ctrl[0], 2 * cur[1] - last_ctrl[1]) if last_ctrl else cur
            c2 = (num() + ox, num() + oy)
            p1 = (num() + ox, num() + oy)
            pts += _cubic(cur, c1, c2, p1)
            cur, last_ctrl = p1, c2
        elif c == "Q":
            q = (num() + ox, num() + oy)
            p1 = (num() + ox, num() + oy)
            pts += _quad(cur, q, p1)
            cur, last_ctrl = p1, q
        elif c == "T":
            q = (2 * cur[0] - last_ctrl[0], 2 * cur[1] - last_ctrl[1]) if last_ctrl else cur
            p1 = (num() + ox, num() + oy)
            pts += _quad(cur, q, p1)
            cur, last_ctrl = p1, q
        elif c == "A":
            rx, ry, phi = num(), num(), num()
            large, sweep = int(num()), int(num())
            p1 = (num() + ox, num() + oy)
            pts += _arc_points(cur, rx, ry, phi, large, sweep, p1)
            cur = p1
            last_ctrl = None
        else:
            raise ValueError(f"commande de tracé inconnue : {cmd}")
    flush(False)
    return subs


def _matrix(transform):
    """La matrice (a, b, c, d, e, f) d'un attribut `transform`."""
    m = (1.0, 0.0, 0.0, 1.0, 0.0, 0.0)
    if not transform:
        return m

    def mul(p, q):
        a1, b1, c1, d1, e1, f1 = p
        a2, b2, c2, d2, e2, f2 = q
        return (a1 * a2 + c1 * b2, b1 * a2 + d1 * b2, a1 * c2 + c1 * d2, b1 * c2 + d1 * d2,
                a1 * e2 + c1 * f2 + e1, b1 * e2 + d1 * f2 + f1)

    for name, args in re.findall(r"(\w+)\s*\(([^)]*)\)", transform):
        v = [float(x) for x in re.split(r"[\s,]+", args.strip()) if x]
        if name == "matrix":
            t = tuple(v)
        elif name == "translate":
            t = (1, 0, 0, 1, v[0], v[1] if len(v) > 1 else 0)
        elif name == "scale":
            t = (v[0], 0, 0, v[1] if len(v) > 1 else v[0], 0, 0)
        elif name == "rotate":
            a = math.radians(v[0])
            r = (math.cos(a), math.sin(a), -math.sin(a), math.cos(a), 0, 0)
            if len(v) == 3:
                t = mul(mul((1, 0, 0, 1, v[1], v[2]), r), (1, 0, 0, 1, -v[1], -v[2]))
            else:
                t = r
        else:
            raise ValueError(f"transformation non gérée : {name}")
        m = mul(m, t)
    return m


def _apply(m, pts):
    a, b, c, d, e, f = m
    return [(a * x + c * y + e, b * x + d * y + f) for x, y in pts]


def _ellipse(cx, cy, rx, ry):
    n = max(12, int(2 * math.pi * max(rx, ry) / STEP))
    return [(cx + rx * math.cos(2 * math.pi * i / n), cy + ry * math.sin(2 * math.pi * i / n))
            for i in range(n)]


def subpaths(el):
    """Les sous-tracés d'un élément de dessin, en coordonnées absolues."""
    tag = _tag(el)
    f = lambda k, dflt=0.0: float(el.get(k, dflt))
    if tag == "path":
        subs = parse_d(el.get("d", ""))
    elif tag == "circle":
        subs = [(_ellipse(f("cx"), f("cy"), f("r"), f("r")), True)]
    elif tag == "ellipse":
        subs = [(_ellipse(f("cx"), f("cy"), f("rx"), f("ry")), True)]
    elif tag == "rect":
        x, y, w, h = f("x"), f("y"), f("width"), f("height")
        rx = float(el.get("rx", el.get("ry", 0)))
        ry = float(el.get("ry", el.get("rx", 0)))
        if rx or ry:
            rx, ry = min(rx, w / 2), min(ry, h / 2)
            d = (f"M{x + rx} {y}H{x + w - rx}A{rx} {ry} 0 0 1 {x + w} {y + ry}V{y + h - ry}"
                 f"A{rx} {ry} 0 0 1 {x + w - rx} {y + h}H{x + rx}A{rx} {ry} 0 0 1 {x} {y + h - ry}"
                 f"V{y + ry}A{rx} {ry} 0 0 1 {x + rx} {y}Z")
            subs = parse_d(d)
        else:
            subs = [([(x, y), (x + w, y), (x + w, y + h), (x, y + h)], True)]
    elif tag in ("polygon", "polyline"):
        v = [float(t) for t in re.split(r"[\s,]+", el.get("points", "").strip()) if t]
        subs = [(list(zip(v[0::2], v[1::2])), tag == "polygon")]
    elif tag == "line":
        subs = [([(f("x1"), f("y1")), (f("x2"), f("y2"))], False)]
    else:
        return []
    m = _matrix(el.get("transform"))
    return [(_apply(m, pts), closed) for pts, closed in subs]


def _fill_shape(el):
    subs = subpaths(el)
    polys = []
    for pts, _ in subs:
        if len(pts) >= 3:
            p = Polygon(pts).buffer(0)
            if not p.is_empty:
                polys.append(p)
    if not polys:
        return GeometryCollection()
    if el.get("fill-rule") == "evenodd" and len(polys) > 1:
        shape = polys[0]
        for p in polys[1:]:
            shape = shape.symmetric_difference(p)
        return shape
    return unary_union(polys)


def _stroke_lines(el):
    lines = []
    for pts, closed in subpaths(el):
        if len(pts) >= 2:
            lines.append(LinearRing(pts) if closed and len(pts) >= 3 else LineString(pts))
    return lines


def _f(v):
    s = f"{v:.2f}".rstrip("0").rstrip(".")
    return "0" if s in ("-0", "") else s


def _poly_d(geom):
    out = []
    polys = [geom] if isinstance(geom, Polygon) else [g for g in getattr(geom, "geoms", []) if isinstance(g, Polygon)]
    for p in polys:
        for ring in [p.exterior, *p.interiors]:
            c = list(ring.coords)[:-1]
            if len(c) < 3:
                continue
            out.append("M" + "L".join(f"{_f(x)} {_f(y)}" for x, y in c) + "Z")
    return "".join(out)


def _line_d(geoms):
    out = []
    for g in geoms:
        parts = [g] if isinstance(g, (LineString, LinearRing)) else list(getattr(g, "geoms", []))
        for line in parts:
            if not isinstance(line, (LineString, LinearRing)):
                continue
            c = list(line.coords)
            if len(c) < 2:
                continue
            closed = isinstance(line, LinearRing)
            if closed:
                c = c[:-1]
            out.append("M" + "L".join(f"{_f(x)} {_f(y)}" for x, y in c) + ("Z" if closed else ""))
    return "".join(out)


# ------------------------------------------------------------------ peinture

class Paints:
    """Les dégradés du document : lesquels sont transparents, et leur copie
    en coordonnées absolues quand un morceau coupé en a besoin."""

    def __init__(self, root):
        self.defs = root.find(Q + "defs")
        self.byid = {g.get("id"): g for g in root.iter() if g.tag in (Q + "linearGradient", Q + "radialGradient")}
        self.copies = 0

    def translucent(self, paint):
        m = re.match(r"url\(#([^)]+)\)", paint or "")
        if not m:
            return False
        g = self.byid.get(m.group(1))
        if g is None:
            return False
        stops = g.findall(Q + "stop")
        while not stops and g.get("{http://www.w3.org/1999/xlink}href"):
            g = self.byid.get(g.get("{http://www.w3.org/1999/xlink}href")[1:])
            stops = g.findall(Q + "stop") if g is not None else []
        for s in stops:
            op = s.get("stop-opacity")
            style = s.get("style", "")
            mm = re.search(r"stop-opacity:\s*([\d.]+)", style)
            if (op is not None and float(op) < 0.999) or (mm and float(mm.group(1)) < 0.999):
                return True
        return False

    def opaque_region(self, paint):
        """Là où le dégradé `paint` est tout à fait opaque (une géométrie, en
        coordonnées absolues) : pour un dégradé linéaire en `userSpaceOnUse`,
        sans transformation, opaque à l'un de ses bouts ; None sinon."""
        m = re.match(r"url\(#([^)]+)\)", paint or "")
        g = self.byid.get(m.group(1)) if m else None
        if (g is None or g.tag != Q + "linearGradient" or g.get("gradientUnits") != "userSpaceOnUse"
                or g.get("gradientTransform")):
            return None
        stops = g.findall(Q + "stop")
        while not stops and g.get("{http://www.w3.org/1999/xlink}href"):
            g = self.byid.get(g.get("{http://www.w3.org/1999/xlink}href")[1:])
            stops = g.findall(Q + "stop") if g is not None else []
        if not stops:
            return None

        def offset(st):
            v = st.get("offset", "0").strip()
            return float(v[:-1]) / 100 if v.endswith("%") else float(v)

        def opacity(st):
            mm = re.search(r"stop-opacity:\s*([\d.]+)", st.get("style", ""))
            return float(st.get("stop-opacity", mm.group(1) if mm else 1.0))

        marks = [(offset(st), opacity(st) >= 0.999) for st in stops]
        x1, y1, x2, y2 = (float(g.get(k, "0")) for k in ("x1", "y1", "x2", "y2"))
        dx, dy = x2 - x1, y2 - y1
        n = math.hypot(dx, dy)
        if n < 1e-9:
            return None
        ux, uy = dx / n, dy / n
        far = 1000.0

        def side(t, forward):
            # Le demi-plan au-delà de la position t du dégradé, vers son bout.
            px, py = x1 + dx * t, y1 + dy * t
            sgn = 1.0 if forward else -1.0
            return Polygon([(px - uy * far, py + ux * far), (px + uy * far, py - ux * far),
                            (px + uy * far + sgn * ux * far, py - ux * far + sgn * uy * far),
                            (px - uy * far + sgn * ux * far, py + ux * far + sgn * uy * far)])

        parts = []
        if marks[-1][1]:
            k = len(marks) - 1
            while k > 0 and marks[k - 1][1]:
                k -= 1
            parts.append(side(marks[k][0], True))
        if marks[0][1]:
            k = 0
            while k < len(marks) - 1 and marks[k + 1][1]:
                k += 1
            parts.append(side(marks[k][0], False))
        return unary_union(parts) if parts else None

    def absolute(self, el, attr):
        """Une copie en `userSpaceOnUse` du dégradé de `el` (s'il est en
        `objectBoundingBox`), calée sur la boîte de l'élément entier."""
        paint = el.get(attr)
        m = re.match(r"url\(#([^)]+)\)", paint or "")
        if not m:
            return
        g = self.byid.get(m.group(1))
        if g is None or g.get("gradientUnits") == "userSpaceOnUse":
            return
        xs, ys = [], []
        for pts, _ in subpaths(el):
            xs += [p[0] for p in pts]
            ys += [p[1] for p in pts]
        if not xs:
            return
        bx, by, bw, bh = min(xs), min(ys), max(xs) - min(xs), max(ys) - min(ys)
        self.copies += 1
        copy = ET.SubElement(self.defs, g.tag, dict(g.attrib))
        copy.set("id", f"{g.get('id')}-abs{self.copies}")
        copy.set("gradientUnits", "userSpaceOnUse")
        t = f"matrix({_f(bw)} 0 0 {_f(bh)} {_f(bx)} {_f(by)})"
        if g.get("gradientTransform"):
            t += " " + g.get("gradientTransform")
        copy.set("gradientTransform", t)
        # Les valeurs par défaut de objectBoundingBox (un dégradé de gauche à droite).
        if copy.tag == Q + "linearGradient":
            for k, v in (("x1", "0"), ("y1", "0"), ("x2", "1"), ("y2", "0")):
                copy.set(k, g.get(k, v))
        else:
            for k, v in (("cx", "0.5"), ("cy", "0.5"), ("r", "0.5")):
                copy.set(k, g.get(k, v))
        for s in g:
            ET.SubElement(copy, s.tag, dict(s.attrib))
        self.byid[copy.get("id")] = copy
        el.set(attr, f"url(#{copy.get('id')})")


# Les attributs de présentation qu'un élément hérite de ses groupes.
INHERITED = ("fill", "fill-opacity", "fill-rule", "stroke", "stroke-width", "stroke-opacity",
             "stroke-linecap", "stroke-linejoin", "stroke-miterlimit", "stroke-dasharray")
STROKE_ATTRS = ("stroke", "stroke-width", "stroke-opacity", "stroke-linecap", "stroke-linejoin",
                "stroke-miterlimit", "stroke-dasharray")
GEOMETRY = ("cx", "cy", "r", "rx", "ry", "x", "y", "width", "height", "points", "x1", "y1", "x2", "y2",
            "transform", "d")


def _inherit(el, inherited=None):
    """Les attributs hérités par les enfants du groupe `el`."""
    out = dict(inherited or {})
    for k in INHERITED:
        if el.get(k) is not None:
            out[k] = el.get(k)
    return out


def _style(el, inherited=None):
    """Le style effectif d'un élément : le sien, et ce qu'il hérite."""
    return _inherit(el, inherited)


def _opacity(el, inherited=None):
    v = float(el.get("opacity", 1.0))
    fo = _style(el, inherited).get("fill-opacity")
    if fo is not None:
        v *= float(fo)
    return v


def _has_fill(el, inherited=None):
    return _style(el, inherited).get("fill", "black") != "none"


def _has_stroke(el, inherited=None):
    return _style(el, inherited).get("stroke") not in (None, "none")


def _drawn(group, inherited=None):
    """Les éléments dessinés d'un groupe, avec leur style hérité."""
    for child in group:
        tag = _tag(child)
        if tag == "g":
            yield from _drawn(child, _inherit(child, inherited))
        elif tag in DRAWN:
            yield child, inherited or {}


DRAWN = {"path", "circle", "ellipse", "rect", "polygon", "polyline", "line"}


# ------------------------------------------------------------------ coupe

def region(pivot, normal, radius, far=1000.0):
    """La région qui bouge avec la pièce du dessus : le côté de la droite
    passant par `pivot` vers lequel pointe `normal`, et le disque de rayon
    `radius` centré sur le pivot ; puis l'autre côté, sans le disque."""
    nx, ny = normal
    n = math.hypot(nx, ny)
    nx, ny = nx / n, ny / n
    tx, ty = -ny, nx
    px, py = pivot
    half = Polygon([(px + tx * far, py + ty * far), (px + tx * far + nx * far, py + ty * far + ny * far),
                    (px - tx * far + nx * far, py - ty * far + ny * far), (px - tx * far, py - ty * far)])
    disc = Point(pivot).buffer(radius, quad_segs=64)
    top = half.union(disc)
    world = box(-far, -far, far, far)
    bottom = world.difference(top)
    return top, bottom, disc


def _clip_element(el, area, paints, opaque_extra=None, inherited=None, fill_extra=None):
    """Les éléments qui dessinent `el` dans `area` (une liste, peut-être
    vide) ; `opaque_extra` : ce qu'un remplissage opaque peut déborder ;
    `inherited` : le style que `el` hérite de ses groupes."""
    tag = _tag(el)
    if tag not in DRAWN:
        return [el]
    style = _style(el, inherited)
    out = []
    fill, stroke = _has_fill(el, inherited), _has_stroke(el, inherited)
    paint = style.get("fill", "black")
    opaque = (_opacity(el, inherited) > 0.999 and not paints.translucent(paint)
              and paint not in ("transparent",))
    a = area.union(opaque_extra) if (opaque and opaque_extra is not None) else area
    if fill_extra is not None and not fill_extra.is_empty:
        a = a.union(fill_extra)
    shape = _fill_shape(el) if fill else GeometryCollection()
    lines = _stroke_lines(el) if stroke else []
    geom = unary_union(lines) if lines else GeometryCollection()
    # Tout entier dedans : l'élément tel quel ; tout entier dehors : rien.
    fill_in = shape.is_empty or shape.difference(a).is_empty
    stroke_in = geom.is_empty or geom.difference(area).length < 1e-6
    if fill_in and stroke_in:
        return [_copy(el)]
    if (shape.is_empty or shape.intersection(a).area < 1e-4) and (
            geom.is_empty or geom.intersection(area).length < 1e-3):
        return []
    if fill:
        if not shape.is_empty:
            if fill_in:
                piece = _copy(el)
                if stroke:
                    for k in STROKE_ATTRS:
                        piece.attrib.pop(k, None)
                    piece.set("stroke", "none")
                out.append(piece)
            else:
                cut = shape.intersection(a).simplify(SIMPLIFY)
                if not cut.is_empty and cut.area > 1e-4:
                    piece = _copy(el)
                    paints.absolute(piece, "fill")
                    for k in STROKE_ATTRS + GEOMETRY + ("fill-rule",):
                        piece.attrib.pop(k, None)
                    piece.tag = Q + "path"
                    piece.set("d", _poly_d(cut))
                    piece.set("fill", piece.get("fill", paint))
                    # Un contour hérité du groupe ne doit pas cerner la coupe.
                    piece.set("stroke", "none")
                    out.append(piece)
    if stroke:
        if not geom.is_empty:
            if stroke_in:
                piece = _copy(el)
                if fill:
                    piece.set("fill", "none")
                    for k in ("fill-opacity", "fill-rule"):
                        piece.attrib.pop(k, None)
                out.append(piece)
            else:
                cut = [g.intersection(area).simplify(SIMPLIFY) for g in lines]
                cut = [g for g in cut if not g.is_empty and g.length > 1e-3]
                if cut:
                    piece = _copy(el)
                    for k in GEOMETRY + ("fill-opacity", "fill-rule"):
                        piece.attrib.pop(k, None)
                    for k in STROKE_ATTRS:
                        if k in style and piece.get(k) is None:
                            piece.set(k, style[k])
                    paints.absolute(piece, "stroke")
                    piece.tag = Q + "path"
                    piece.set("fill", "none")
                    piece.set("d", _line_d(cut))
                    out.append(piece)
    return out


def _tag(el):
    """Le nom d'un élément, sans l'espace de noms ("" pour un commentaire)."""
    return el.tag.replace(Q, "") if isinstance(el.tag, str) else ""


def _copy(el):
    c = ET.Element(el.tag, dict(el.attrib))
    c.text, c.tail = el.text, el.tail
    for child in el:
        c.append(_copy(child))
    return c


def _clip_tree(el, area, paints, opaque_extra=None, skip_ids=(), inherited=None):
    """`el` (un groupe) réduit à `area` : ses éléments coupés, ses sous-groupes
    gardés s'ils dessinent encore quelque chose. `inherited` : le style que
    `el` hérite de ses parents (pas le sien : il garde ses attributs)."""
    out = ET.Element(el.tag, dict(el.attrib))
    inner = _inherit(el, inherited)
    for child in el:
        tag = _tag(child)
        if child.get("id", "").startswith("pivot-") or child.get("id") in skip_ids:
            continue
        if tag == "g":
            sub = _clip_tree(child, area, paints, opaque_extra, skip_ids, inner)
            if len(sub):
                # Une partie imbriquée (une mèche dans le corps) garde son pivot.
                if child.get("id"):
                    sub.extend(c for c in child if c.get("id", "").startswith("pivot-"))
                out.append(sub)
        else:
            for piece in _clip_element(child, area, paints, opaque_extra, inner):
                out.append(piece)
    return out


def _pivot(name, p):
    return ET.Element(Q + "circle", {"id": f"pivot-{name}", "cx": _f(p[0]), "cy": _f(p[1]), "r": "0.8"})


def _find(root, gid):
    for parent in root.iter():
        for i, child in enumerate(list(parent)):
            if child.get("id") == gid:
                return parent, i, child
    raise KeyError(gid)


def split_leg(root, paints, name, joint, radius, axis=(0.0, 1.0)):
    """Coupe la patte `name` à son articulation `joint` (le carpe ou le
    jarret) : le haut garde ce qui est au-dessus et le disque ; le bas, posé
    sous le haut dans son groupe, tout le reste. `axis` : la direction de la
    patte, de la hanche vers le sabot."""
    parent, index, leg = _find(root, name)
    upper_area, lower_area, disc = region(joint, (-axis[0], -axis[1]), radius)
    pivots = [c for c in leg if c.get("id", "").startswith("pivot-")]
    lower = _clip_tree(leg, lower_area, paints, opaque_extra=disc)
    lower.set("id", f"{name}-lower")
    lower.append(_pivot(f"{name}-lower", joint))
    upper = _clip_tree(leg, upper_area, paints)
    new = ET.Element(leg.tag, dict(leg.attrib))
    new.text, new.tail = leg.text, leg.tail
    new.append(lower)
    for child in upper:
        new.append(child)
    for p in pivots:
        new.append(p)
    parent.remove(leg)
    parent.insert(index, new)
    return new


def split_neck(root, paints, source, pivot, radius, forward, poll, head="head", muzzle=None,
               skip_ids=(), margin=1.5):
    """Prend dans le groupe `source` (le corps) ce qui est devant la base de
    l'encolure (`pivot`, la droite perpendiculaire à `forward`), hors du
    disque de rayon `radius` : c'est l'encolure, dessinée par-dessus le
    corps, qui porte désormais la tête (posée à la place de celle-ci). Le
    corps garde le disque (l'épaule) : l'encolure s'arrête sur son bord, et
    glisse le long de ce cercle en tournant. `radius` doit couvrir toute la
    coupe du dessin par la droite. `poll` : la nuque, le nouveau pivot de la
    tête ; `muzzle` : ce qui touche l'herbe."""
    front, behind, disc = region(pivot, forward, radius)
    neck_area = front.difference(disc)
    body_area = behind.union(disc)
    sparent, sindex, body = _find(root, source)
    neck = _clip_tree(body, neck_area, paints, skip_ids=skip_ids)
    rest = _clip_tree(body, body_area, paints, opaque_extra=disc.buffer(margin), skip_ids=skip_ids)
    for c in body:
        if c.get("id", "").startswith("pivot-"):
            rest.append(c)
    sparent.remove(body)
    sparent.insert(sindex, rest)
    hparent, hindex, head_el = _find(root, head)
    group = ET.Element(Q + "g", {"id": "neck"})
    for k, v in body.attrib.items():
        if k != "id":
            group.set(k, v)
    for child in neck:
        group.append(child)
    hparent.remove(head_el)
    for c in list(head_el):
        if c.get("id") == f"pivot-{head}":
            head_el.remove(c)
    head_el.append(_pivot(head, poll))
    if muzzle is not None:
        head_el.append(_pivot("muzzle", muzzle))
    group.append(head_el)
    group.append(_pivot("neck", pivot))
    hparent.insert(hindex, group)
    return group


def cut_under(root, paints, source, area, name, pivot, margin=2.0, skip_ids=()):
    """Détache de `source` (le corps) ce qu'il dessine dans `area` (un
    polygone) : une partie `name`, posée juste avant `source` (dessous),
    qui tourne autour de `pivot`. Ses remplissages opaques débordent de
    `margin` sous le corps (aucune jointure ne laisse passer le jour) ; ses
    traits et ses tracés transparents s'arrêtent net. Au repos, le dessin ne
    change pas. Sert à l'encolure et aux mèches de la toison."""
    area = Polygon(area)
    outside = box(-1000, -1000, 1000, 1000).difference(area)
    sparent, sindex, body = _find(root, source)
    piece = _clip_tree(body, area, paints, opaque_extra=area.buffer(margin), skip_ids=skip_ids)
    rest = _clip_tree(body, outside, paints, skip_ids=skip_ids)
    for c in body:
        if c.get("id", "").startswith("pivot-"):
            rest.append(c)
    piece.set("id", name)
    piece.append(_pivot(name, pivot))
    sparent.remove(body)
    sparent.insert(sindex, rest)
    sparent.insert(sindex, piece)
    return piece


def hang_strands(root, paints, cls, cut_y, name, pivot, margin=1.0):
    """Des mèches qui pendent (les tracés de classe `cls`), coupées à la
    hauteur `cut_y` : ce qui pend plus bas devient la partie `name`, qui
    balance autour de `pivot` (sur la ligne de coupe : en se cisaillant, la
    mèche ne la quitte pas). La partie se pose au début du groupe de la
    première mèche, sous les autres ; son remplissage déborde de `margin`
    au-dessus de la coupe, sous le haut de chaque mèche, resté à sa place.
    Au repos, rien ne change (sauf l'ordre des bouts de mèches sous la coupe,
    s'ils croisent des mèches qui ne balancent pas)."""
    below = box(-1000, cut_y, 1000, 1000)
    above = box(-1000, -1000, 1000, cut_y)
    band = box(-1000, cut_y - margin, 1000, cut_y)
    styles = {}

    def walk(g, inherited):
        inner = _inherit(g, inherited)
        styles[id(g)] = inner
        for child in g:
            if _tag(child) == "g":
                walk(child, inner)

    walk(root, None)
    members = [(parent, el) for parent in root.iter() for el in parent if el.get("class") == cls]
    if not members:
        raise KeyError(cls)
    host = members[0][0]
    group = ET.Element(Q + "g", {"id": name})
    for parent, el in members:
        inherited = styles[id(parent)]
        index = list(parent).index(el)
        upper = _clip_element(el, above, paints, inherited=inherited)
        lower = _clip_element(el, below, paints, opaque_extra=band, inherited=inherited)
        parent.remove(el)
        for k, piece in enumerate(upper):
            piece.attrib.pop("class", None)
            parent.insert(index + k, piece)
        for piece in lower:
            piece.attrib.pop("class", None)
            group.append(piece)
    group.append(_pivot(name, pivot))
    host.insert(0, group)
    return group


def split_neck_under(root, paints, source, area, pivot, poll, head="head", muzzle=None,
                     skip_ids=(), margin=2.0, filler=None):
    """L'encolure sous le corps, comme une marionnette découpée : ce que le
    groupe `source` (le corps) dessine dans `area` (un polygone : ce qui
    dépasse devant l'épaule) devient l'encolure (voir `cut_under`). La tête
    reste à sa place dans le document (par-dessus le corps) : le moteur la
    fait suivre l'encolure. `pivot` : la base de l'encolure ; `poll` : la
    nuque, le nouveau pivot de la tête ; `muzzle` : ce qui touche l'herbe ;
    `filler` : (longueur, rayon) d'une épaisseur de laine posée tout au fond
    de l'encolure, de sa base vers la nuque (voir `_fill_neck`) ; elle est
    ajoutée avant que la tête ne change de pivot."""
    neck = cut_under(root, paints, source, area, "neck", pivot, margin, skip_ids)
    if filler:
        _fill_neck(root, paints, source, neck, pivot, poll, filler)
    _, _, head_el = _find(root, head)
    for c in list(head_el):
        if c.get("id") == f"pivot-{head}":
            head_el.remove(c)
    head_el.append(_pivot(head, poll))
    if muzzle is not None:
        head_el.append(_pivot("muzzle", muzzle))
    return neck


def shoulder(withers, chest, bulge, n=16, far=170.0, top=-20.0):
    """La zone de l'encolure, devant une épaule arrondie : un arc du garrot
    (`withers`) au poitrail (`chest`), bombé de `bulge` vers l'avant et le
    haut ; tout ce qui est devant et au-dessus, jusqu'à `far` et `top`.
    Quand l'encolure descend, le corps finit par cet arc : une épaule."""
    (ax, ay), (bx, by) = withers, chest
    dx, dy = bx - ax, by - ay
    length = math.hypot(dx, dy)
    nx, ny = dy / length, -dx / length
    if nx < 0:
        nx, ny = -nx, -ny
    pts = []
    for i in range(n + 1):
        t = i / n
        k = 4.0 * bulge * t * (1.0 - t)
        pts.append((ax + dx * t + nx * k, ay + dy * t + ny * k))
    return pts + [(far, by), (far, top), (ax, top)]


def _fill_neck(root, paints, source, neck, pivot, poll, filler):
    """Une épaisseur de laine au fond de l'encolure : une capsule de la
    couleur de base de la toison (le premier remplissage opaque du corps),
    de la base de l'encolure vers la nuque, sur `filler[0]` de cette
    longueur, de rayon `filler[1]`, rognée à ce que le corps, l'encolure et
    la tête recouvrent : cachée au repos, elle comble l'espace entre le corps
    et la tête quand celle-ci descend."""
    _, _, body = _find(root, source)
    paint = None
    for el, inh in _drawn(body, _inherit(body)):
        style = _style(el, inh)
        fill = style.get("fill", "black")
        if fill != "none" and _opacity(el, inh) > 0.999 and not paints.translucent(fill):
            paint = fill
            break
    if paint is None:
        return
    length, radius = filler
    (ax, ay), (bx, by) = pivot, poll
    end = (ax + (bx - ax) * length, ay + (by - ay) * length)
    capsule = LineString([pivot, end]).buffer(radius, quad_segs=16)
    # Rognée à ce que le corps, l'encolure et la tête recouvrent au repos :
    # elle ne se voit que quand l'encolure bouge.
    cover = []
    for gid in (source, "neck", "head"):
        try:
            _, _, group = _find(root, gid)
        except KeyError:
            continue
        for el, inh in _drawn(group, _inherit(group)):
            if _has_fill(el, inh):
                shape = _fill_shape(el)
                if not shape.is_empty:
                    cover.append(shape)
    if cover:
        capsule = capsule.intersection(unary_union(cover).buffer(-0.3))
    if capsule.is_empty:
        return
    el = ET.Element(Q + "path", {"d": _poly_d(capsule.simplify(SIMPLIFY)), "fill": paint})
    neck.insert(0, el)


def cut_reach(svg_text, source, pivot, forward):
    """La plus grande distance du pivot aux bords du dessin de `source`, le
    long de la coupe : le rayon minimal du disque de l'encolure."""
    root = ET.fromstring(svg_text)
    _, _, body = _find(root, source)
    shapes = [_fill_shape(el) for el, inh in _drawn(body, _inherit(body)) if _has_fill(el, inh)]
    shape = unary_union([s for s in shapes if not s.is_empty])
    nx, ny = forward
    n = math.hypot(nx, ny)
    tx, ty = -ny / n, nx / n
    px, py = pivot
    line = LineString([(px - tx * 1000, py - ty * 1000), (px + tx * 1000, py + ty * 1000)])
    cut = shape.intersection(line)
    pts = [p for g in getattr(cut, "geoms", [cut]) for p in getattr(g, "coords", [])]
    return max((math.dist(p, pivot) for p in pts), default=0.0)


def lift_part(root, paints, source, area, name, pivot, under=()):
    """Détache de `source` (la tête) ce qui est dans `area` (un polygone) :
    une partie `name` dessinée par-dessus, qui tourne autour de `pivot` (la
    lèvre). `source` garde ses remplissages opaques entiers (cachés au repos),
    et `under` (des éléments : la gencive) se pose entre les deux : c'est ce
    qu'on voit quand la partie se soulève. Les groupes sans nom de `source`
    sont coupés comme le reste ; ses parties (un groupe nommé : la mâchoire,
    les yeux, les oreilles) restent entières à leur place."""
    _, _, src = _find(root, source)
    area = Polygon(area)
    outside = box(-1000, -1000, 1000, 1000).difference(area)
    keep, lifted = _split_tree(src, area, outside, paints, None)
    lifted.attrib.clear()
    lifted.set("id", name)
    _cull_hidden(lifted, paints)
    lifted.append(_pivot(name, pivot))
    for child in list(src):
        src.remove(child)
    pivots = [c for c in keep if c.get("id", "").startswith("pivot-")]
    for child in keep:
        if child not in pivots:
            src.append(child)
    for el in under:
        src.append(el)
    src.append(lifted)
    for c in pivots:
        src.append(c)
    return lifted


def _opaque_gradient(el, inherited, paints):
    """Là où le remplissage de `el`, un dégradé en partie transparent, est
    tout à fait opaque (voir `Paints.opaque_region`) ; None si ce n'est pas
    le cas, ou si l'élément est transformé."""
    if not _has_fill(el, inherited) or el.get("transform") or _opacity(el, inherited) < 0.999:
        return None
    paint = _style(el, inherited).get("fill", "black")
    if not paints.translucent(paint):
        return None
    return paints.opaque_region(paint)


def _cull_hidden(group, paints):
    """Ôte d'un groupe les remplissages opaques (sans contour) entièrement
    recouverts par des remplissages opaques posés après eux : ils ne se
    voient pas, et sous une autre couleur, leurs bords laisseraient un liseré
    à l'anticrénelage (le fond clair du crâne sous le masque noir, sur le bord
    d'une lèvre). Le groupe bouge d'un bloc : rien n'y change à l'œil."""
    items = []

    def walk(g, inherited, moved):
        inner = _inherit(g, inherited)
        for child in list(g):
            tag = _tag(child)
            if tag == "g":
                walk(child, inner, moved or child.get("transform") is not None)
            elif tag in DRAWN:
                items.append((g, child, inner, moved or child.get("transform") is not None))

    walk(group, None, False)
    cover = None
    for parent, el, inh, moved in reversed(items):
        if moved or not _has_fill(el, inh):
            continue
        paint = _style(el, inh).get("fill", "black")
        if _opacity(el, inh) < 0.999 or paint == "transparent":
            continue
        shape = _fill_shape(el)
        if shape.is_empty:
            continue
        if paints.translucent(paint):
            region = _opaque_gradient(el, inh, paints)
            if region is not None:
                part = shape.intersection(region)
                if not part.is_empty:
                    cover = part if cover is None else unary_union([cover, part])
            continue
        if (cover is not None and not _has_stroke(el, inh)
                and shape.difference(cover.buffer(0.05)).area < 1e-3):
            parent.remove(el)
            continue
        cover = shape if cover is None else unary_union([cover, shape])


def _split_tree(group, area, outside, paints, inherited):
    """Ce qui reste d'un groupe hors de `area` (ses remplissages opaques
    entiers) et ce qui en est soulevé (ce qu'il dessine dans `area`), dans
    deux copies du groupe. Ses parties (groupes nommés) et ses pivots restent
    entiers dans la première."""
    keep = ET.Element(group.tag, dict(group.attrib))
    lift = ET.Element(group.tag, dict(group.attrib))
    inner = _inherit(group, inherited) if inherited is not None else _inherit(group, None)
    for child in group:
        tag = _tag(child)
        if child.get("id") and (tag == "g" or child.get("id", "").startswith("pivot-")):
            keep.append(child)
        elif tag == "g":
            k, l = _split_tree(child, area, outside, paints, inner)
            if len(k):
                keep.append(k)
            if len(l):
                lift.append(l)
        elif tag in DRAWN:
            region = _opaque_gradient(child, inner, paints)
            keep.extend(_clip_element(child, outside, paints, opaque_extra=area, inherited=inner,
                                      fill_extra=area.intersection(region) if region is not None else None))
            lift.extend(_clip_element(child, area, paints, inherited=inner))
        else:
            keep.append(child)
    return keep, lift


def smooth_d(points, closed=True):
    """Un tracé lisse passant près de `points` (des quadratiques entre les
    milieux des côtés)."""
    pts = list(points)
    if closed:
        mids = [((pts[i][0] + pts[(i + 1) % len(pts)][0]) / 2, (pts[i][1] + pts[(i + 1) % len(pts)][1]) / 2)
                for i in range(len(pts))]
        d = f"M{_f(mids[-1][0])} {_f(mids[-1][1])}"
        for i, p in enumerate(pts):
            d += f"Q{_f(p[0])} {_f(p[1])} {_f(mids[i][0])} {_f(mids[i][1])}"
        return d + "Z"
    d = f"M{_f(pts[0][0])} {_f(pts[0][1])}"
    for i in range(1, len(pts) - 1):
        m = ((pts[i][0] + pts[i + 1][0]) / 2, (pts[i][1] + pts[i + 1][1]) / 2)
        if i == len(pts) - 2:
            m = pts[-1]
        d += f"Q{_f(pts[i][0])} {_f(pts[i][1])} {_f(m[0])} {_f(m[1])}"
    if len(pts) == 2:
        d += f"L{_f(pts[1][0])} {_f(pts[1][1])}"
    return d


def gum(pad, ridge, fill, crease, width=0.35):
    """La gencive qu'une lèvre découvre en se retroussant (le bourrelet du
    haut : le mouton n'a pas d'incisives en haut) : le bourrelet `pad` (un
    polygone, lissé) de couleur `fill`, et son pli `ridge` (une ligne) de
    couleur `crease`. À poser sous la lèvre (`lift_part(..., under=...)`)."""
    return [
        ET.Element(Q + "path", {"d": smooth_d(pad), "fill": fill}),
        ET.Element(Q + "path", {"d": smooth_d(ridge, closed=False), "fill": "none", "stroke": crease,
                                "stroke-width": _f(width), "stroke-linecap": "round"}),
    ]


def clip_away(root, paints, gid, polygon):
    """Ôte du groupe `gid` ce qui est dans `polygon` (ce qu'une autre pièce,
    peinte jusqu'ici par-dessus, cachait) : le groupe garde son id et ses
    pivots."""
    parent, index, group = _find(root, gid)
    outside = box(-1000, -1000, 1000, 1000).difference(Polygon(polygon))
    kept = _clip_tree(group, outside, paints)
    for c in group:
        if c.get("id", "").startswith("pivot-"):
            kept.append(c)
    parent.remove(group)
    parent.insert(index, kept)
    return kept


def articulate_breed(svg_text, carpus_y, hock_y, neck=None, extra=None):
    """Ce qu'un générateur de race appelle avant d'écrire son dessin : les
    quatre pattes coupées au carpe (à la hauteur `carpus_y`) et au jarret
    (`hock_y`) ; l'encolure, si `neck` est donné (les arguments de
    `split_neck_under`, sans les deux premiers) ; `extra(root, paints)`
    pour le reste (une découpe propre à la race)."""
    legs = []
    for name in ("leg-back-far", "leg-front-far", "leg-back-near", "leg-front-near"):
        y = carpus_y if "front" in name else hock_y
        joint, half = leg_joint(svg_text, name, y)
        legs.append((name, joint, half + 0.4, (0.0, 1.0)))
    parser = ET.XMLParser(target=ET.TreeBuilder(insert_comments=True))
    root = ET.fromstring(svg_text, parser=parser)
    paints = Paints(root)
    for name, joint, radius, axis in legs:
        split_leg(root, paints, name, joint, radius, axis)
    if neck:
        split_neck_under(root, paints, **neck)
    if extra:
        extra(root, paints)
    ET.indent(root, space="  ")
    return ET.tostring(root, encoding="unicode") + "\n"


def articulate(svg_text, legs=(), neck=None):
    """Le dessin recoupé. `legs` : [(nom, articulation, rayon, axe)] ; `neck` :
    les arguments de `split_neck` (sauf les deux premiers)."""
    # Les commentaires du document gardés tels quels.
    parser = ET.XMLParser(target=ET.TreeBuilder(insert_comments=True))
    root = ET.fromstring(svg_text, parser=parser)
    paints = Paints(root)
    for name, joint, radius, axis in legs:
        split_leg(root, paints, name, joint, radius, axis)
    if neck:
        split_neck(root, paints, **neck)
    ET.indent(root, space="  ")
    return ET.tostring(root, encoding="unicode") + "\n"


def leg_joint(svg_text, name, y):
    """Le milieu de la patte `name` à la hauteur `y`, et sa demi-largeur : de
    quoi placer une articulation au carpe ou au jarret."""
    root = ET.fromstring(svg_text)
    _, _, leg = _find(root, name)
    shapes = [_fill_shape(el) for el, inh in _drawn(leg, _inherit(leg)) if _has_fill(el, inh)]
    shape = unary_union([s for s in shapes if not s.is_empty])
    cut = shape.intersection(LineString([(-1000, y), (1000, y)]))
    xs = [x for g in getattr(cut, "geoms", [cut]) for x, _ in g.coords]
    return ((min(xs) + max(xs)) / 2, y), (max(xs) - min(xs)) / 2
