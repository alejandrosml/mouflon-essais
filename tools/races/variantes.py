#!/usr/bin/env python3
"""Le bélier, la brebis et l'agneau d'une race, tirés du dessin de son générateur.

Chaque générateur dessine un animal : le dessin de base de sa race. Avec un
deuxième argument (`belier`, `brebis` ou `agneau`), il dessine l'autre sexe
ou le jeune. Il retire ou ajoute lui-même ce qui change (cornes, bourses,
mamelle, queue d'agneau, couleurs de naissance), puis `ecrire` met chaque
partie à ses proportions, d'après la fiche de la race :

    python3 tools/races/awassi/gen_awassi.py pets/awassi/awassi-brebis.svg brebis

Les proportions ne redessinent rien : chaque partie du rig (`body`, `neck`,
`head`, les pattes, `tail`) reçoit une transformation (`transform`) qui
l'allonge, l'épaissit ou la déplace, et les articulations restent jointes
(l'encolure suit l'épaule, la tête suit la nuque, les sabots restent au
sol). Le moteur lit ces transformations comme le reste du dessin.

`python3 tools/races/variantes.py <race>` relance le générateur de la race
pour chacun de ses autres dessins et met à jour son `pet.toml`.

© 2026 Alejandro San Martin Lamas — MIT OR Apache-2.0
"""
import colorsys
import math
from pathlib import Path
import re
import subprocess
import sys
import xml.etree.ElementTree as ET

NS = "http://www.w3.org/2000/svg"
ET.register_namespace("", NS)
RACINE = Path(__file__).resolve().parents[2]
APERCU = RACINE / "target" / "release" / "mouflon-preview"

# Les sections de `pet.toml` et le nom des fichiers.
SECTIONS = {"belier": "male", "brebis": "female", "agneau": "lamb"}

# Les proportions d'un agneau de deux semaines, par rapport à l'adulte ramené
# à sa taille (voir `transformer`) : le tronc plus court et moins profond,
# les pattes longues et un peu fortes, l'encolure courte et portée haut, la
# tête grosse au museau court, les oreilles grandes. Chaque race y ajoute sa
# hauteur au garrot (environ 0,6 fois celle de la brebis) et ses retouches.
AGNEAU = dict(corps=(.84, .82), pattes=(1.15, 1.14), encolure=(.9, .85),
              tete=(1.04, 1.18), oreilles=1.1, port=8)


def q(tag):
    return f"{{{NS}}}{tag}"


# ------------------------------------------------------------ transformations
# Une transformation affine (a, b, c, d, e, f), comme `matrix()` en SVG :
# x' = a·x + c·y + e ; y' = b·x + d·y + f.
ID = (1.0, 0.0, 0.0, 1.0, 0.0, 0.0)


def mul(m, n):
    """m ∘ n : d'abord n, puis m."""
    a, b, c, d, e, f = m
    A, B, C, D, E, F = n
    return (a * A + c * B, b * A + d * B, a * C + c * D, b * C + d * D,
            a * E + c * F + e, b * E + d * F + f)


def inv(m):
    a, b, c, d, e, f = m
    det = a * d - b * c
    A, B, C, D = d / det, -b / det, -c / det, a / det
    return (A, B, C, D, -(A * e + C * f), -(B * e + D * f))


def app(m, p):
    a, b, c, d, e, f = m
    return (a * p[0] + c * p[1] + e, b * p[0] + d * p[1] + f)


def vers(p, cible, lin):
    """Envoie `p` sur `cible`, avec la partie linéaire `lin` (a, b, c, d)."""
    a, b, c, d = lin
    return (a, b, c, d, cible[0] - a * p[0] - c * p[1], cible[1] - b * p[0] - d * p[1])


def rot(angle):
    co, si = math.cos(angle), math.sin(angle)
    return (co, si, -si, co)


def lin_mul(m, n):
    a, b, c, d = m
    A, B, C, D = n
    return (a * A + c * B, b * A + d * B, a * C + c * D, b * C + d * D)


def etire(angle, le_long, en_travers):
    """Étire le long d'un axe incliné de `angle` et en travers de lui."""
    r, ri = rot(angle), rot(-angle)
    return lin_mul(r, lin_mul((le_long, 0.0, 0.0, en_travers), ri))


def texte(m):
    return "matrix(" + " ".join(f"{v:.5f}".rstrip("0").rstrip(".") for v in m) + ")"


# -------------------------------------------------------------------- le rig
def pivots(root):
    """Les pivots du dessin : nom de la partie -> (x, y)."""
    out = {}
    for c in root.iter(q("circle")):
        ident = c.get("id", "")
        if ident.startswith("pivot-"):
            out[ident[6:]] = (float(c.get("cx")), float(c.get("cy")))
    return out


def partie(root, ident):
    for el in root.iter():
        if el.get("id") == ident:
            return el
    return None


def parent_de(root, enfant):
    for el in root.iter():
        if enfant in list(el):
            return el
    return None


def retirer(root, *, ident=None, classe=None):
    """Retire les éléments de cet `id` ou de cette classe ; rend leur nombre."""
    n = 0
    for el in list(root.iter()):
        for ch in list(el):
            if (ident and ch.get("id") == ident) or (
                    classe and classe in (ch.get("class") or "").split()):
                el.remove(ch)
                n += 1
    return n


def vider(groupe):
    """Retire tout d'une partie, sauf ses pivots et ses sous-parties nommées."""
    for ch in list(groupe):
        if ch.tag == q("circle") and (ch.get("id") or "").startswith("pivot-"):
            continue
        groupe.remove(ch)


def _poser(el, m):
    """Ajoute une transformation à un élément (avant celle qu'il a déjà)."""
    if all(abs(v - w) < 1e-9 for v, w in zip(m, ID)):
        return
    avant = el.get("transform")
    el.set("transform", texte(m) + (" " + avant if avant else ""))


def transformer(root, *, echelle=1.0, garrot=None, corps=(1.0, 1.0), pattes=(1.0, 1.0),
                encolure=(1.0, 1.0), tete=(1.0, 1.0), oreilles=1.0, queue=(1.0, 1.0),
                port=0.0, port_tete=0.0, avant=None, arriere=None, enfonce=0.0):
    """Met chaque partie du rig à ses proportions.

    - `echelle` : la taille de l'ensemble ; ou `garrot=(base, cible)` : la
      hauteur au garrot du dessin de base et celle voulue, en centimètres
      (l'échelle s'en déduit, pattes allongées comprises) ;
    - `corps=(longueur, hauteur)` : le tronc, autour de l'attache des pattes ;
    - `pattes=(épaisseur, longueur)` : les quatre pattes, sabots au sol ; le
      tronc monte d'autant ; `avant` et `arriere` remplacent `pattes` pour
      une paire ;
    - `encolure=(longueur, épaisseur)`, le long de l'encolure ; `port` la
      relève (degrés, positif vers le haut) autour de sa base ;
    - `tete=(longueur, hauteur)` autour de la nuque, `port_tete` la relève ;
    - `oreilles` autour de leur base ; `queue=(épaisseur, longueur)` ;
    - `enfonce` : de combien le tronc descend sur ses pattes (unités du
      dessin de base), leur haut rentrant d'autant dans la toison. Pour un
      adulte trapu tiré d'un dessin haut sur pattes : les pattes gardent
      alors la même échelle en large et en long, et leurs articulations
      restent rondes (étirées d'un seul côté, elles s'ouvriraient en pliant).

    Tous les facteurs sont relatifs à l'échelle d'ensemble."""
    pv = pivots(root)
    sol = pv["ground"]
    jambes = [n for n in pv if re.fullmatch(r"leg-(front|back)-(near|far)", n)]
    xm = sum(pv[n][0] for n in jambes) / len(jambes)
    ya = sum(pv[n][1] for n in jambes) / len(jambes)
    devant = avant or pattes
    derriere = arriere or pattes
    longueur = (devant[1] + derriere[1]) / 2
    monte = (longueur - 1.0) * (sol[1] - ya) - enfonce

    def corps0(p):
        return (xm + corps[0] * (p[0] - xm), ya + corps[1] * (p[1] - ya) - monte)

    if garrot:
        base, cible = garrot
        haut = sol[1] - corps0((xm, sol[1] - base))[1]
        echelle = cible / haut
    s = echelle
    globale = (s, 0.0, 0.0, s, sol[0] * (1 - s), sol[1] * (1 - s))
    m_corps = mul(globale, (corps[0], 0.0, 0.0, corps[1],
                            xm * (1 - corps[0]), ya * (1 - corps[1]) - monte))

    maps = {}
    for n in jambes:
        ep, lo = devant if "front" in n else derriere
        px = pv[n][0]
        # L'épaisseur autour de l'axe de la patte, la longueur depuis le sol.
        m = (ep, 0.0, 0.0, lo, corps0((px, 0))[0] - ep * px, sol[1] * (1 - lo))
        maps[n] = mul(globale, m)
    if "neck" in pv and "head" in pv:
        pn, ph = pv["neck"], pv["head"]
        angle = math.atan2(ph[1] - pn[1], ph[0] - pn[0])
        lin = lin_mul(rot(-math.radians(port)), etire(angle, encolure[0], encolure[1]))
        lin = tuple(s * v for v in lin)
        maps["neck"] = vers(pn, app(m_corps, pn), lin)
        base_tete = app(maps["neck"], ph)
    elif "head" in pv:
        base_tete = app(m_corps, pv["head"])
    if "head" in pv:
        lin = lin_mul(rot(-math.radians(port_tete)), (tete[0], 0.0, 0.0, tete[1]))
        maps["head"] = vers(pv["head"], base_tete, tuple(s * v for v in lin))
    if "tail" in pv:
        maps["tail"] = vers(pv["tail"], app(m_corps, pv["tail"]),
                            (s * queue[0], 0.0, 0.0, s * queue[1]))

    for el in list(root):
        ident = el.get("id") or ""
        if el.tag == q("defs") or ident == "pivot-ground":
            continue
        _poser(el, maps.get(ident, m_corps))
    # Une partie rangée dans une autre (la tête dans l'encolure) : sa
    # transformation propre, défaite de celle qui la porte.
    for ident, m in maps.items():
        el = partie(root, ident)
        porte = parent_de(root, el)
        if porte is root or el is None:
            continue
        haut = porte
        while haut is not root and haut.get("id") not in maps:
            haut = parent_de(root, haut)
        dessus = maps.get(haut.get("id"), m_corps) if haut is not root else ID
        _poser(el, mul(inv(dessus), m))
    if abs(oreilles - 1.0) > 1e-9:
        for n in ("ear-near", "ear-far"):
            el = partie(root, n)
            if el is not None and n in pv:
                _poser(el, vers(pv[n], pv[n], (oreilles, 0.0, 0.0, oreilles)))
    return echelle


# ------------------------------------------------------------------ couleurs
def _hex(c):
    c = c.strip()
    if re.fullmatch(r"#[0-9a-fA-F]{3}", c):
        c = "#" + "".join(ch * 2 for ch in c[1:])
    if not re.fullmatch(r"#[0-9a-fA-F]{6}", c):
        return None
    return tuple(int(c[i:i + 2], 16) / 255 for i in (1, 3, 5))


def _vers_hex(rgb):
    return "#" + "".join(f"{round(max(0.0, min(1.0, v)) * 255):02X}" for v in rgb)


def clarte(rgb):
    return 0.2126 * rgb[0] + 0.7152 * rgb[1] + 0.0722 * rgb[2]


def melange(a, b, t):
    return tuple(x + (y - x) * t for x, y in zip(a, b))


def rampe(sombre, clair, bas=0.0, haut=1.0, garde=0.0):
    """Une fonction de couleur : la clarté d'origine (de `bas` à `haut`)
    choisit un ton entre `sombre` et `clair` ; `garde` conserve une part de
    la teinte d'origine."""
    s, c = _hex(sombre), _hex(clair)

    def f(rgb):
        t = (clarte(rgb) - bas) / max(1e-6, haut - bas)
        t = max(0.0, min(1.0, t))
        return melange(melange(s, c, t), rgb, garde)
    return f


def teinte(vers_couleur, force):
    """Une fonction de couleur : tire chaque ton vers une couleur."""
    cible = _hex(vers_couleur)
    return lambda rgb: melange(rgb, cible, force)


def eclaircir(force):
    return lambda rgb: melange(rgb, (1.0, 1.0, 1.0), force)


def saturer(k):
    def f(rgb):
        h, l, s = colorsys.rgb_to_hls(*rgb)
        return colorsys.hls_to_rgb(h, l, max(0.0, min(1.0, s * k)))
    return f


def recolorer(root, fonction, dans=None, sauf=(), si=None):
    """Change les couleurs (remplissages, traits, dégradés) des éléments de
    `dans` (des `id` de parties ; tout le dessin sinon), sauf ceux rangés
    sous un `id` ou une classe de `sauf`. `si(rgb)` choisit les couleurs à
    changer. Un dégradé employé ailleurs aussi est dédoublé."""
    racines = [partie(root, i) for i in dans] if dans else [root]
    racines = [r for r in racines if r is not None]
    defs = root.find(q("defs"))
    touche, ailleurs = set(), set()

    def change(valeur):
        rgb = _hex(valeur or "")
        if rgb is None or (si and not si(rgb)):
            return valeur
        return _vers_hex(fonction(rgb))

    def exclu(el):
        return el.get("id") in sauf or any(c in sauf for c in (el.get("class") or "").split())

    dedans = set()

    def visite(el):
        if exclu(el):
            return
        dedans.add(el)
        for ch in el:
            visite(ch)

    for r in racines:
        visite(r)
    for el in root.iter():
        if defs is not None and (el is defs or el in list(defs.iter())):
            continue
        for attr in ("fill", "stroke"):
            v = el.get(attr) or ""
            m = re.fullmatch(r"url\(#(.+)\)", v)
            if m:
                (touche if el in dedans else ailleurs).add(m.group(1))
            elif el in dedans and v:
                el.set(attr, change(v))
    if defs is None:
        return
    par_id = {g.get("id"): g for g in defs}
    for ident in sorted(touche):
        g = par_id.get(ident)
        if g is None:
            continue
        if ident in ailleurs:
            # Employé hors de la zone : une copie pour la zone.
            copie = ET.fromstring(ET.tostring(g))
            neuf = ident + "-v"
            copie.set("id", neuf)
            defs.append(copie)
            for el in dedans:
                for attr in ("fill", "stroke"):
                    if el.get(attr) == f"url(#{ident})":
                        el.set(attr, f"url(#{neuf})")
            g = copie
        for stop in g.iter(q("stop")):
            stop.set("stop-color", change(stop.get("stop-color")))


def corne(root, axe, gros, clair="#CDBDA2", moyen="#9C876B", sombre="#4B3B2D", loin=False,
          avant=None, anneaux=1.0, dans="head", nom="corne"):
    """Une corne annelée le long d'un axe (des points du repère du dessin,
    de la base à la pointe), épaisse de `gros` à la base (demi-largeur) et
    effilée. Elle se range dans `dans` : tout au fond si `loin` (celle de
    l'autre côté, assombrie), sinon juste avant l'élément d'`id` `avant`
    (l'oreille qui la recouvre), ou par-dessus tout."""
    g = partie(root, dans)
    grp = ET.Element(q("g"), {"class": nom})
    # Un axe régulier, par longueur d'arc.
    fin = [axe[0]]
    for a, b in zip(axe, axe[1:]):
        d = math.hypot(b[0] - a[0], b[1] - a[1])
        k = max(1, int(d / .45))
        fin += [(a[0] + (b[0] - a[0]) * i / k, a[1] + (b[1] - a[1]) * i / k) for i in range(1, k + 1)]
    n = len(fin) - 1
    gauche, droite, normales = [], [], []
    for i, p in enumerate(fin):
        a, b = fin[max(0, i - 1)], fin[min(n, i + 1)]
        d = math.hypot(b[0] - a[0], b[1] - a[1]) or 1.0
        nx, ny = -(b[1] - a[1]) / d, (b[0] - a[0]) / d
        w = gros * (1 - i / n) ** .8 + .12
        normales.append((nx, ny, w))
        gauche.append((p[0] + nx * w, p[1] + ny * w))
        droite.append((p[0] - nx * w, p[1] - ny * w))
    ys = [p[1] for p in gauche + droite]
    xs = [p[0] for p in gauche + droite]
    if loin:
        clair, moyen, sombre = (_vers_hex(melange(_hex(c), (.08, .07, .07), .35))
                                for c in (clair, moyen, sombre))
    fill = _degrade(root, nom + ("-loin" if loin else "-pres") + f"-{len(list(g))}",
                    (min(xs), min(ys)), (min(xs), max(ys)),
                    [(0, clair), (.4, moyen), (1, sombre)])
    contour = gauche + droite[::-1]
    _chemin(grp, "M" + " L".join(f"{x:.2f},{y:.2f}" for x, y in contour) + " Z", fill=fill)
    # Les anneaux : des sillons en travers, serrés ; un reflet sur leur crête.
    pas = max(2, int(round(1.1 / .45 / max(.2, anneaux))))
    for i in range(2, n - 3, pas):
        (nx, ny, w), p = normales[i], fin[i]
        _chemin(grp, f"M{p[0] + nx * w * .92:.2f},{p[1] + ny * w * .92:.2f} "
                     f"L{p[0] - nx * w * .92:.2f},{p[1] - ny * w * .92:.2f}",
                fill="none", stroke=sombre, stroke_width=".14", stroke_opacity=".5",
                stroke_linecap="round")
        if not loin:
            _chemin(grp, f"M{p[0] + nx * w * .85 + .12:.2f},{p[1] + ny * w * .85 + .12:.2f} "
                         f"L{p[0] + nx * w * .2 + .12:.2f},{p[1] + ny * w * .2 + .12:.2f}",
                    fill="none", stroke=clair, stroke_width=".1", stroke_opacity=".55",
                    stroke_linecap="round")
    if loin:
        g.insert(0, grp)
    elif avant is not None:
        enfants = list(g)
        cible = [k for k, el in enumerate(enfants) if el.get("id") == avant]
        g.insert(cible[0] if cible else len(enfants), grp)
    else:
        g.append(grp)
    return grp


def spirale(base, centre, tours, rayon_fin, sens=1.0, n=28):
    """L'axe d'une corne en spirale, pour les générateurs qui dessinent une
    corne le long d'un axe : de `base` (à la tête), autour de `centre`, sur
    `tours` tours, le rayon passant de sa valeur de départ à `rayon_fin`.
    `sens` = 1 : sens trigonométrique du repère employé (dans un repère où
    x va vers le nez et h vers le haut : vers l'arrière, puis le bas)."""
    a0 = math.atan2(base[1] - centre[1], base[0] - centre[0])
    r0 = math.hypot(base[0] - centre[0], base[1] - centre[1])
    pts = []
    for i in range(n + 1):
        t = i / n
        a = a0 + sens * 2 * math.pi * tours * t
        r = r0 + (rayon_fin - r0) * t
        pts.append((centre[0] + r * math.cos(a), centre[1] + r * math.sin(a)))
    return pts


# ---------------------------------------------------------- ce qu'on ajoute
_NOMBRE = re.compile(r"[-+]?(?:\d*\.\d+|\d+\.?)(?:[eE][-+]?\d+)?")
_ARGS = {"M": 2, "L": 2, "H": 1, "V": 1, "C": 6, "S": 4, "Q": 4, "T": 2, "A": 7, "Z": 0}


def _points(d):
    """Les sommets d'un tracé (le bout de chaque segment), commandes
    absolues ou relatives."""
    pts = []
    x = y = x0 = y0 = 0.0
    for cmd, reste in re.findall(r"([MmLlHhVvCcSsQqTtAaZz])([^MmLlHhVvCcSsQqTtAaZz]*)", d):
        n = _ARGS[cmd.upper()]
        vals = [float(v) for v in _NOMBRE.findall(reste)]
        if n == 0:
            x, y = x0, y0
            continue
        rel = cmd.islower()
        for k in range(0, len(vals) - n + 1, n):
            a = vals[k:k + n]
            c = cmd.upper()
            if c == "H":
                x = x + a[0] if rel else a[0]
            elif c == "V":
                y = y + a[0] if rel else a[0]
            else:
                x, y = (x + a[-2], y + a[-1]) if rel else (a[-2], a[-1])
            if c == "M" and k == 0:
                x0, y0 = x, y
            pts.append((x, y))
    return pts


def ventre(root, x, large=2.5, dans="body"):
    """Le bas de ce que dessine `dans` autour de l'abscisse `x` (le dessous
    du ventre, laine comprise), dans le repère du dessin."""
    def plus_bas(traits):
        bas = None
        for el in partie(root, dans).iter(q("path")):
            if ((el.get("fill") or "none") == "none") != traits:
                continue
            for px, py in _points(el.get("d") or ""):
                if abs(px - x) <= large and (bas is None or py > bas):
                    bas = py
        return bas

    # Les surfaces d'abord ; un dessin tout en traits (le Zwartbles) n'a que
    # ses coups de pinceau.
    bas = plus_bas(False)
    if bas is None:
        bas = plus_bas(True)
    if bas is None:
        sys.exit(f"pas de ventre trouvé autour de x = {x}")
    return bas


def sous_le_ventre(root, dx, dy=0.0):
    """Un point sous le ventre, à `dx` devant le haut du postérieur du côté
    qu'on voit, `dy` sous la laine (négatif : rentré dedans)."""
    x = pivots(root)["leg-back-near"][0] + dx
    return (x, ventre(root, x) + dy)


def _degrade(root, ident, p0, p1, stops):
    defs = root.find(q("defs"))
    if defs is None:
        defs = ET.Element(q("defs"))
        root.insert(0, defs)
    g = ET.SubElement(defs, q("linearGradient"), {
        "id": ident, "gradientUnits": "userSpaceOnUse",
        "x1": f"{p0[0]:.2f}", "y1": f"{p0[1]:.2f}", "x2": f"{p1[0]:.2f}", "y2": f"{p1[1]:.2f}"})
    for off, col in stops:
        ET.SubElement(g, q("stop"), {"offset": f"{off:g}", "stop-color": col})
    return f"url(#{ident})"


def _lisse(pts, ferme=True):
    """Une courbe lisse (Catmull-Rom) par ces points, en tracé SVG."""
    n = len(pts)
    if n < 3:
        return "M" + " L".join(f"{x:.2f},{y:.2f}" for x, y in pts)
    def P(i):
        return pts[i % n] if ferme else pts[max(0, min(n - 1, i))]
    d = [f"M{pts[0][0]:.2f},{pts[0][1]:.2f}"]
    for i in range(n if ferme else n - 1):
        p0, p1, p2, p3 = P(i - 1), P(i), P(i + 1), P(i + 2)
        c1 = (p1[0] + (p2[0] - p0[0]) / 6, p1[1] + (p2[1] - p0[1]) / 6)
        c2 = (p2[0] - (p3[0] - p1[0]) / 6, p2[1] - (p3[1] - p1[1]) / 6)
        d.append(f"C{c1[0]:.2f},{c1[1]:.2f} {c2[0]:.2f},{c2[1]:.2f} {p2[0]:.2f},{p2[1]:.2f}")
    return " ".join(d) + (" Z" if ferme else "")


def _chemin(parent, d, index=None, **attrs):
    el = ET.Element(q("path"), {"d": d, **{k.replace("_", "-"): str(v) for k, v in attrs.items()}})
    if index is None:
        parent.append(el)
    else:
        parent.insert(index, el)
    return el


def mamelle(root, x, y, largeur=11.0, hauteur=8.0, peau="#E3B7A6", ombre="#B78673",
            trayon=True, dans="body"):
    """Une mamelle sous le ventre, devant le postérieur : `(x, y)` est le
    milieu de son attache (en haut), dans le repère du dessin. Elle se range
    au fond de `dans`, sous la laine du ventre."""
    g = partie(root, dans)
    grp = ET.Element(q("g"), {"class": "mamelle"})
    w, h = largeur / 2, hauteur
    pts = [(x - w, y - 1.5), (x - w * 1.05, y + h * .45), (x - w * .55, y + h * .92),
           (x + w * .1, y + h), (x + w * .75, y + h * .78), (x + w * 1.05, y + h * .3),
           (x + w, y - 1.5)]
    fill = _degrade(root, "mamelle-peau", (x - w, y), (x + w, y + h),
                    [(0, ombre), (.45, peau), (1, ombre)])
    _chemin(grp, _lisse(pts), fill=fill)
    if trayon:
        tx, ty = x + w * .5, y + h * .82
        t = [(tx - 1.0, ty - .6), (tx - .2, ty + 1.9), (tx + .9, ty + 2.5), (tx + 1.5, ty + 1.5),
             (tx + 1.3, ty - .4)]
        _chemin(grp, _lisse(t), fill=ombre)
    _chemin(grp, _lisse([(x - w * .2, y + h * .2), (x + w * .1, y + h * .55), (x + w * .15, y + h * .9)],
                        False), fill="none", stroke=ombre, stroke_width=".25",
            stroke_opacity=".55", stroke_linecap="round")
    g.insert(0, grp)
    return grp


def bourses(root, x, y, largeur=9.0, hauteur=17.0, peau="#D9AE9D", ombre="#AD7F6F",
            laine=None, dans="body"):
    """Les bourses du bélier, pendues entre les postérieurs : `(x, y)` est le
    milieu de leur attache sous le ventre. `laine` : une couleur de toison,
    si elles sont laineuses (elle remplace la peau)."""
    g = partie(root, dans)
    grp = ET.Element(q("g"), {"class": "bourses"})
    w, h = largeur / 2, hauteur
    pts = [(x - w * .55, y - 2), (x - w * .5, y + h * .3), (x - w * .95, y + h * .62),
           (x - w * .8, y + h * .9), (x, y + h), (x + w * .8, y + h * .9),
           (x + w * .95, y + h * .62), (x + w * .5, y + h * .3), (x + w * .55, y - 2)]
    clair = laine or peau
    fill = _degrade(root, "bourses-peau", (x - w, y), (x + w, y),
                    [(0, ombre), (.4, clair), (.7, clair), (1, ombre)])
    _chemin(grp, _lisse(pts), fill=fill)
    _chemin(grp, _lisse([(x + .2, y + h * .35), (x - .2, y + h * .65), (x + .1, y + h * .95)], False),
            fill="none", stroke=ombre, stroke_width=".22", stroke_opacity=".5",
            stroke_linecap="round")
    g.insert(0, grp)
    return grp


def queue_agneau(root, longueur, epaisseur=3.2, clair="#EFE6D8", sombre="#B9A892",
                 courbe=2.0, graine=7, vers_avant=1.5):
    """Remplace la queue par celle d'un agneau : longue, mince, laineuse,
    pendue de son attache (`pivot-tail`) vers les jarrets, contre la fesse.
    `courbe` : de combien son bout part vers l'arrière ; `vers_avant` : de
    combien son attache rentre sous la croupe. Mesures dans le repère du
    dessin de base (avant la mise à l'échelle)."""
    import random
    rng = random.Random(graine)
    g = partie(root, "tail")
    px, py = pivots(root)["tail"]
    vider(g)
    for sous in ("tail-mid", "tail-tip"):
        retirer(root, ident=sous)
    w = epaisseur / 2

    def axe(t):
        return (px + vers_avant * (1 - t) ** 2 - courbe * t ** 1.6 + .35 * math.sin(t * 5.0),
                py - 1.5 + (longueur + 1.5) * t)

    def demi(t):
        # Un peu plus forte sous l'attache, effilée ensuite, le bout arrondi.
        forme = (.82 + .18 * math.sin(math.pi * min(1.0, t / .35))) * (1.0 - .5 * t)
        bout = math.sqrt(max(0.0, 1.0 - max(0.0, (t - .9) / .1) ** 2))
        return w * forme * (.25 + .75 * bout)

    n = 14
    gauche = [(axe(i / n)[0] - demi(i / n), axe(i / n)[1]) for i in range(n + 1)]
    droite = [(axe(i / n)[0] + demi(i / n), axe(i / n)[1]) for i in range(n + 1)]
    contour = gauche + droite[::-1]
    fill = _degrade(root, "queue-agneau", (px - w, py), (px + w, py),
                    [(0, sombre), (.4, clair), (.75, clair), (1, sombre)])
    _chemin(g, _lisse(contour), 0, fill=fill)
    # La laine : de petites boucles en travers, plus serrées vers le bout.
    k = 1
    for i in range(int(longueur * 1.6)):
        t = (i + rng.random()) / (longueur * 1.6)
        cx, cy = axe(t)
        ww = demi(t) * .8
        x0 = cx + rng.uniform(-ww, ww)
        _chemin(g, f"M{x0:.2f},{cy:.2f} q{rng.uniform(-.6, .6):.2f},{rng.uniform(.3, .8):.2f} "
                   f"{rng.uniform(-.9, .9):.2f},{rng.uniform(.6, 1.3):.2f}", k, fill="none",
                stroke=sombre if rng.random() < .6 else clair,
                stroke_width=f"{rng.uniform(.14, .24):.2f}", stroke_opacity=".5",
                stroke_linecap="round")
        k += 1
    return g


# ------------------------------------------------------------------- écrire
def mesurer(chemin):
    """Les mesures du moteur pour ce dessin (`mouflon-preview … mesure`)."""
    if not APERCU.exists():
        sys.exit("il manque target/release/mouflon-preview : "
                 "cargo build --release -p mouflon-preview")
    out = subprocess.run([str(APERCU), str(chemin), "-", "mesure"], check=True,
                         capture_output=True, text=True).stdout
    m = {}
    for ligne in out.splitlines():
        nom, *vals = ligne.split()
        m[nom] = [float(v) for v in vals]
    return m


def verifier_pivots(chemin, root):
    """Une variante garde les articulations du dessin de base, au même
    endroit : un pivot déplacé par mégarde (une bascule du générateur qui
    change aussi la partie qu'il dessine) ferait tourner la partie autour
    d'un autre point, ce qui ne se voit pas au repos. Compare avec le dessin
    de base du dépôt, quand il sort tel quel du générateur."""
    base = chemin.with_name(chemin.name.rsplit("-", 1)[0] + ".svg")
    if not base.exists() or base == chemin:
        return
    racine = ET.parse(base).getroot()
    if any(g.get("transform") for g in racine.iter(q("g"))):
        return  # le dessin de base est lui-même une variante (Southdown, Mérinos d'Arles)
    attendus, trouves = pivots(racine), pivots(root)
    for nom, (x, y) in attendus.items():
        if nom == "ground" or nom not in trouves:
            continue
        if abs(trouves[nom][0] - x) > 1e-6 or abs(trouves[nom][1] - y) > 1e-6:
            sys.exit(f"{chemin.name} : le pivot « {nom} » est en {trouves[nom]}, "
                     f"en ({x:g}, {y:g}) dans le dessin de base")


def ecrire(chemin, svg, marges=(6.0, 6.0, 6.0, 5.0), **proportions):
    """Met le dessin à ses proportions (voir `transformer`), le cadre au plus
    juste (marges : gauche, haut, droite, bas) et l'écrit. Rend ses mesures :
    `height`, `width` et `stride` pour son `pet.toml`."""
    root = ET.fromstring(svg) if isinstance(svg, str) else svg
    chemin = Path(chemin)
    verifier_pivots(chemin, root)
    echelle = transformer(root, **proportions)
    ET.indent(root, space="  ")
    chemin.write_text(ET.tostring(root, encoding="unicode") + "\n")
    m = mesurer(chemin)
    x0, y0, x1, y1 = m["boite"]
    dx, dy = marges[0] - x0, marges[1] - y0
    for el in list(root):
        ident = el.get("id") or ""
        if el.tag == q("defs"):
            continue
        if ident == "pivot-ground":
            el.set("cx", f"{float(el.get('cx')) + dx:.3f}")
            el.set("cy", f"{float(el.get('cy')) + dy:.3f}")
            continue
        _poser(el, (1.0, 0.0, 0.0, 1.0, dx, dy))
    w = x1 - x0 + marges[0] + marges[2]
    h = y1 - y0 + marges[1] + marges[3]
    for attr, v in (("viewBox", f"0 0 {w:.2f} {h:.2f}"), ("width", f"{w:.2f}"), ("height", f"{h:.2f}")):
        root.set(attr, v)
    ET.indent(root, space="  ")
    texte_svg = ET.tostring(root, encoding="unicode") + "\n"
    chemin.write_text(texte_svg)
    m = mesurer(chemin)
    print(f"{chemin} : {len(texte_svg) / 1024:.0f} Ko, échelle {echelle:.3f}, "
          f"height {m['height'][0]:.3f}, width {m['width'][0]:.3f}, stride {m['stride'][0]:.3f}")
    return m


# ------------------------------------------------------------- le pet.toml
def section(nom, fichier, m, commentaire="", semaines=None, cle="rig"):
    lignes = [f"[appearance.{nom}]"]
    if commentaire:
        lignes.insert(0, f"# {commentaire}")
    lignes += [f'{cle} = "{fichier}"',
               f"height = {m['height'][0]:.3f}",
               f"width = {m['width'][0]:.3f}",
               f"stride = {m['stride'][0]:.3f}"]
    if semaines:
        lignes.append(f"weeks = {semaines:g}")
    return "\n".join(lignes) + "\n"


def maj_pet_toml(race, dessins, semaines=None, cle="rig"):
    """Écrit les sections `[appearance.male|female|lamb]` du `pet.toml` de la
    race (entre `[appearance]` et la section suivante), en remplaçant celles
    qui y sont. `dessins` : [(variante, fichier, mesures, commentaire)] ;
    `semaines` : l'âge de l'agneau dessiné, s'il n'a pas deux semaines ;
    `cle` : `rig` pour un dessin vectoriel, `skeleton` pour un squelette."""
    chemin = RACINE / "pets" / race / "pet.toml"
    texte_toml = chemin.read_text()
    for variante, _, _, _ in dessins:
        nom = SECTIONS[variante]
        texte_toml = re.sub(
            rf"(?ms)^(#[^\n]*\n)?\[appearance\.{nom}\]\n.*?(?=^\[|^# [^\n]*\n\[|\Z)", "", texte_toml)
    m = re.search(r"(?ms)^\[appearance\]\n.*?(?=^(?:#[^\n]*\n)*\[)", texte_toml)
    if not m:
        sys.exit(f"{chemin} : pas de section [appearance]")
    bloc = m.group(0).rstrip("\n") + "\n\n"
    for variante, fichier, mesures, commentaire in dessins:
        bloc += section(SECTIONS[variante], fichier, mesures, commentaire,
                        semaines if variante == "agneau" else None, cle) + "\n"
    texte_toml = texte_toml[:m.start()] + bloc + texte_toml[m.end():]
    chemin.write_text(texte_toml)


def generateur(race):
    dossier = RACINE / "tools" / "races" / race
    scripts = sorted(dossier.glob("gen_*.py"))
    if len(scripts) != 1:
        sys.exit(f"{dossier} : un seul gen_*.py attendu")
    return scripts[0]


def main(argv):
    if len(argv) < 2:
        sys.exit("usage : variantes.py <race> [belier] [brebis] [agneau]")
    race = argv[1]
    script = generateur(race)
    source = script.read_text()
    voulues = argv[2:] or [v for v in SECTIONS if re.search(rf'"{v}"\s*:', source)]
    dessins = []
    for variante in voulues:
        fichier = f"{race}-{variante}.svg"
        sortie = RACINE / "pets" / race / fichier
        subprocess.run([sys.executable, str(script), str(sortie), variante], check=True)
        commentaire = re.search(rf'"{variante}"\s*:\s*dict\(\s*#\s*([^\n]+)', source)
        dessins.append((variante, fichier, mesurer(sortie),
                        commentaire.group(1).strip() if commentaire else ""))
    # Un agneau dessiné à un autre âge que deux semaines le dit dans son
    # générateur : `AGE_AGNEAU = 8.0`.
    age = re.search(r"(?m)^AGE_AGNEAU\s*=\s*([\d.]+)", source)
    maj_pet_toml(race, dessins, float(age.group(1)) if age else None)
    print(f"pets/{race}/pet.toml : {', '.join(SECTIONS[v] for v, *_ in dessins)}")


if __name__ == "__main__":
    main(sys.argv)
