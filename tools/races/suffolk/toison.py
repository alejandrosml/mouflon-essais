"""La toison du Suffolk : quatre mois et demi de repousse (3 à 4 cm), dense,
fermée, à grain fin, « en croûte de pain » (fiche, § 3.1 ; photos P1, P11,
P17) : une surface mate parcourue d'un réseau de fines crevasses, surtout
verticales, ondulées, reliées par de courtes branches, là où la toison
s'ouvre ; entre elles, de petites cellules bombées qui prennent la lumière
en haut à gauche (un bourrelet clair le long de chaque crevasse, son ombre
de l'autre côté) ; et partout un grain serré de pointes de mèches. Rien ne
pend.

La texture est tirée une seule fois, sur l'union de toutes les pièces de
laine du côté visible (le corps, l'encolure, le haut des pattes du côté
proche, y compris ce qui est caché au repos) ; chaque pièce n'en dessine
que ce qui tombe dans sa forme. Au repos, les crevasses passent donc d'une
pièce à l'autre sans rupture : la jointure ne se voit pas. Les ombres et
les lumières de volume sont des dégradés en coordonnées absolues, posés de
même sur chaque pièce.

© 2026 Alejandro San Martin Lamas — MIT OR Apache-2.0
"""
import math
import random

from outils import (LineString, Marks, Noise1D, Noise2D, Point, clamp, lines_of, mix, normals, poisson, prep,
                    resample)

# ------------------------------------------------------------------ palette (fiche, § 4)

T_LIGHT = "#EAE3D3"      # toison au soleil, lumière
T_BASE = "#C9C1AE"       # toison au soleil, base
T_SUN_SHADOW = "#ABA392"
T_SHADE = "#9E9683"      # toison à l'ombre propre (bas des flancs)
T_SHADE_DEEP = "#766F66"
T_HOLLOW = "#6E6756"     # creux entre les mèches, dessous du ventre
T_HOLLOW_DEEP = "#4E4838"
T_HIGH = "#F4EFE4"       # le haut des cellules, au soleil

# La lumière vient d'en haut à gauche (comme pour les autres races).
LIGHT = (-0.6, 0.8)

# Les petites zones plus fermées alternent avec les ouvertures du flanc,
# avec les mêmes valeurs sur toutes les pièces : pas de couture au repos.
RELIEF = Noise2D(1886, [(13.0, 0.26), (4.5, 0.12)])


# ------------------------------------------------------------------ le réseau de crevasses

class Cracks:
    """Le réseau de crevasses, tiré sur une forme (monde). `flow(x, h)` donne
    l'angle des crevasses (radians, 0 : verticales ; positif : le haut part
    vers l'avant) ; `spacing` l'écart moyen entre deux crevasses voisines ;
    `depth(x, h)` combien elles sont ouvertes (0 : fermées, sur le dos ; 1 :
    ouvertes, sur les flancs)."""

    def __init__(self, region, rng, flow, depth, spacing=2.7, length=(5.0, 13.0), step=0.35):
        self.region = region
        self.rng = rng
        self.flow = flow
        self.depth = depth
        self.spacing = spacing
        self.cell = spacing * 0.5
        self.grid = {}
        self.lines = []          # [(points, depth moyenne, sorte)]
        inside = prep(region)
        wob = Noise2D(rng.randrange(1 << 30), [(2.4, 0.6), (0.9, 0.42), (0.5, 0.2)])
        seeds = poisson(region, spacing * 0.95, rng)
        seeds.sort(key=lambda p: (round(p[1] / 4.0), p[0]))
        rng.shuffle(seeds)
        for (x, h) in seeds:
            if self._near(x, h, spacing * 0.62, None):
                continue
            L = rng.uniform(*length)
            pts = self._trace(x, h, L, step, wob, inside)
            if len(pts) >= 4:
                self._add(pts, "crevasse")
        # Les branches : de courts traits de travers, d'une crevasse vers sa voisine.
        main = list(self.lines)
        for pts, _, _ in main:
            n = len(pts)
            k = rng.randint(2, 5)
            while k < n - 2:
                if rng.random() < 0.6:
                    x, h = pts[k]
                    side = rng.choice((-1, 1))
                    self._branch(x, h, side, step, wob, inside, own=id(pts))
                k += rng.randint(5, 9)

    def _key(self, x, h):
        return (int(math.floor(x / self.cell)), int(math.floor(h / self.cell)))

    def _near(self, x, h, d, own):
        gx, gh = self._key(x, h)
        r = int(math.ceil(d / self.cell)) + 1
        for i in range(gx - r, gx + r + 1):
            for j in range(gh - r, gh + r + 1):
                for (px, ph, o) in self.grid.get((i, j), ()):
                    if o != own and (px - x) ** 2 + (ph - h) ** 2 < d * d:
                        return True
        return False

    def _mark(self, pts, own):
        for (x, h) in pts:
            self.grid.setdefault(self._key(x, h), []).append((x, h, own))

    def _trace(self, x0, h0, L, step, wob, inside):
        """Une crevasse : vers le haut puis vers le bas depuis la graine, en
        suivant le sens des crevasses, en ondulant, jusqu'à sa longueur ou
        jusqu'à toucher une voisine."""
        own = object()
        halves = []
        for sgn in (1, -1):
            x, h = x0, h0
            pts = []
            s = 0.0
            while s < L * 0.5:
                a = self.flow(x, h) + 0.32 * wob(x, h)
                dx, dh = math.sin(a) * sgn, math.cos(a) * sgn
                x, h = x + dx * step, h + dh * step
                s += step
                if not inside.contains(Point(x, h)):
                    break
                if self._near(x, h, self.spacing * 0.42, None):
                    break
                pts.append((x, h))
            halves.append(pts)
        pts = halves[1][::-1] + [(x0, h0)] + halves[0]
        del own
        return pts

    def _branch(self, x0, h0, side, step, wob, inside, own):
        """Une branche de travers, qui part d'une crevasse et va rejoindre la
        voisine (ou s'arrête avant), souvent en biais : des fourches en Y."""
        a = self.flow(x0, h0) + side * (math.pi / 2 + self.rng.uniform(-1.0, 0.75))
        maxlen = self.spacing * self.rng.uniform(0.6, 1.4)
        x, h = x0, h0
        pts = [(x, h)]
        s = 0.0
        while s < maxlen:
            a2 = a + 0.5 * wob(x + 7.0, h - 3.0)
            x, h = x + math.sin(a2) * step, h + math.cos(a2) * step
            s += step
            if not inside.contains(Point(x, h)):
                break
            pts.append((x, h))
            if s > self.spacing * 0.3 and self._near(x, h, step * 1.2, None):
                break
        if len(pts) >= 3:
            self._add(pts, "branche")

    def _add(self, pts, kind):
        d = sum(self.depth(x, h) for (x, h) in pts) / len(pts)
        self.lines.append((pts, d, kind))
        self._mark(pts, id(pts))


# ------------------------------------------------------------------ dessiner une pièce

def _shade_side(pts):
    """Le côté (+1 : à gauche du sens de parcours) où la paroi de la crevasse
    est à l'ombre (tournée vers le bas et la droite, loin de la lumière)."""
    if len(pts) < 2:
        return 1
    dx = pts[-1][0] - pts[0][0]
    dh = pts[-1][1] - pts[0][1]
    L = math.hypot(dx, dh) or 1.0
    nx, nh = -dh / L, dx / L        # la normale à gauche
    # La cellule à gauche de la crevasse lui présente sa paroi tournée vers -n :
    # éclairée si -n va vers la lumière.
    lit_left = (-nx) * LIGHT[0] + (-nh) * LIGHT[1] > 0
    return -1 if lit_left else 1


def _offset(pts, d):
    out = []
    ns = normals(pts, closed=False)
    for (x, h), (nx, nh) in zip(pts, ns):
        out.append((x + nx * d, h + nh * d))
    return out


def draw_cracks(doc, piece, cracks, light, dim=0.0, seed=0, scale=1.0):
    """Les crevasses du réseau qui tombent dans `piece` (une forme, monde) :
    un sillon sombre d'épaisseur inégale (la toison s'ouvre plus ou moins),
    une ombre douce sur la paroi tournée vers le bas et la droite, la lèvre
    éclairée de l'autre. `light(x, h)` : la lumière du lieu (0 à 1) ; `dim`
    assombrit tout (les pattes du fond)."""
    clip = piece.buffer(-0.08)
    if clip.is_empty:
        return
    inside = prep(clip)
    m = Marks()
    rng = random.Random(seed)

    def q(v, k):
        return round(v * k) / k

    for pts, depth, kind in cracks.lines:
        line = LineString(pts)
        if not inside.intersects(line):
            continue
        if inside.contains(line):
            parts = [pts]
        else:
            parts = [list(g.coords) for g in lines_of(line.intersection(clip)) if g.length > 0.5]
        for seg in parts:
            seg = resample(seg, 0.5) if len(seg) > 2 else seg
            if len(seg) < 2:
                continue
            dep0 = clamp(depth * (0.8 if kind == "branche" else 1.0))
            if dep0 < 0.08:
                continue
            side = _shade_side(seg)
            # par tronçons de 1 à 3 cm, chacun plus ou moins ouvert
            i = 0
            while i < len(seg) - 1:
                n = rng.randint(2, 6)
                chunk = seg[i:i + n + 1]
                i += n
                if len(chunk) < 2:
                    break
                mx = sum(p[0] for p in chunk) / len(chunk)
                mh = sum(p[1] for p in chunk) / len(chunk)
                lt = q(clamp(light(mx, mh) - dim), 4)
                dep = clamp(dep0 * rng.uniform(0.55, 1.35) * (0.8 + RELIEF(mx, mh)))
                col = mix(T_HOLLOW_DEEP, T_HOLLOW, lt)
                sw = q((0.06 + 0.11 * dep) * scale, 40)
                op = q(clamp(0.12 + 0.26 * dep - 0.06 * lt + 0.16 * dim), 20)
                m.add(col, sw, op, doc_line(chunk))
                # le bourrelet : la paroi à l'ombre, large et douce, et la lèvre
                # éclairée de l'autre côté, plus large encore
                sh = _offset(chunk, side * (0.3 + 0.08 * dep) * scale)
                m.add(T_SHADE_DEEP, q((0.4 + 0.12 * dep) * scale, 10), q(clamp(0.025 + 0.035 * dep + 0.05 * dim), 40),
                      doc_line(sh))
                if lt > 0.15:
                    hi = _offset(chunk, -side * (0.42 + 0.08 * dep) * scale)
                    m.add(T_HIGH, q(0.5 * scale, 10), q(clamp((0.035 + 0.1 * lt) * (0.55 + 0.45 * dep)), 40),
                          doc_line(hi))
    m.emit(doc)


def doc_line(pts):
    # Les pointes de mèches ondulent : une courbe douce évite les angles de
    # polygone qui faisaient lire une terre craquelée plutôt qu'une toison.
    from outils import cr
    return cr(pts, closed=False, nd=2, t=0.7)


def arc_d(cx, ch, r, a0, a1):
    """Un arc de cercle (monde) en une seule courbe de Bézier, au millimètre."""
    from outils import S, nums
    k = 4.0 / 3.0 * math.tan((a1 - a0) / 4.0) * r
    p0 = (cx + r * math.cos(a0), ch + r * math.sin(a0))
    p3 = (cx + r * math.cos(a1), ch + r * math.sin(a1))
    p1 = (p0[0] - k * math.sin(a0), p0[1] + k * math.cos(a0))
    p2 = (p3[0] + k * math.sin(a1), p3[1] - k * math.cos(a1))
    P = [S(*p) for p in (p0, p1, p2, p3)]
    x0, y0 = round(P[0][0], 1), round(P[0][1], 1)
    rel = [P[1][0] - x0, P[1][1] - y0, P[2][0] - x0, P[2][1] - y0, P[3][0] - x0, P[3][1] - y0]
    return f"M{nums((x0, y0), 1)}c{nums(rel, 1)}"


def quad_d(p0, c, p1):
    """Une petite courbe (monde) en une quadratique, au millimètre."""
    from outils import S, nums
    P = [S(*p) for p in (p0, c, p1)]
    x0, y0 = round(P[0][0], 1), round(P[0][1], 1)
    rel = [P[1][0] - x0, P[1][1] - y0, P[2][0] - x0, P[2][1] - y0]
    return f"M{nums((x0, y0), 1)}q{nums(rel, 1)}"


def draw_grain(doc, piece, light, seed, density=1.5, dim=0.0, scale=1.0, flow=None):
    """Le grain serré de la surface : le haut bombé des petites mèches (de
    courts arcs clairs) et les pointes frisées (de petites virgules
    sombres), fins et peu contrastés."""
    rng = random.Random(seed)
    clip = piece.buffer(-0.25)
    if clip.is_empty:
        return
    m = Marks()

    def q(v, k=5):
        return round(v * k) / k

    for (x, h) in poisson(clip, 1.0 / math.sqrt(density) * scale, rng):
        lt = clamp(light(x, h) - dim)
        a0 = flow(x, h) if flow else 0.0
        r = rng.uniform(0.22, 0.42) * scale
        if rng.random() < 0.55:
            # le haut d'une petite mèche, qui prend la lumière : un arc bombé vers le haut-gauche
            mid = math.pi / 2 + 0.45 + a0 + rng.uniform(-0.6, 0.5)
            span = rng.uniform(1.4, 2.4)
            if lt > 0.15:
                m.add(T_HIGH, round(0.11 * scale, 2), q(clamp(0.12 + 0.38 * lt), 10),
                      arc_d(x, h, r, mid - span / 2, mid + span / 2))
        else:
            # une pointe frisée : une petite virgule sombre, plutôt verticale
            L = rng.uniform(0.3, 0.55) * scale
            a = a0 + rng.uniform(-0.6, 0.6)
            ca, sa = math.sin(a), math.cos(a)
            w = 0.2 * scale * rng.choice((-1, 1))
            col = mix(T_HOLLOW, T_SHADE, q(lt, 3))
            m.add(col, round(0.08 * scale, 2), q(clamp(0.1 + 0.16 * (1 - lt) + 0.12 * dim), 10),
                  quad_d((x - ca * L / 2, h - sa * L / 2), (x + sa * w, h - ca * w), (x + ca * L / 2, h + sa * L / 2)))
    m.emit(doc)


def bumpy_outline(pts, seed, amp, wl=3.0, closed=True, weight=None):
    """Un contour un peu bosselé : chaque cellule de la toison fait une petite
    bosse sur le bord. `weight(x, h)` module l'amplitude (0 : lisse)."""
    noise = Noise1D(seed, [(wl, 1.0), (wl * 0.43, 0.35)])
    rs = resample(pts + ([pts[0]] if closed else []), 0.3)
    if closed:
        rs = rs[:-1]
    ns = normals(rs, closed=closed)
    out = []
    s = 0.0
    for i, (x, h) in enumerate(rs):
        if i:
            s += math.dist(rs[i - 1], rs[i])
        w = weight(x, h) if weight else 1.0
        # des bosses, pas des creux : le bruit redressé
        d = amp * w * (abs(noise(s)) - 0.35)
        out.append((x + ns[i][0] * d, h + ns[i][1] * d))
    return out
