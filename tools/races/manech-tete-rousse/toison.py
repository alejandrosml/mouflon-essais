"""Toison longue et jarreuse de la Manech tête rousse (fiche, § 2 à 4).

Le flanc est un rideau ouvert de mèches de 20 à 25 cm, et non une nappe de
boucles. Les mèches se chevauchent ; leurs fibres sont dessinées dans le sens
de la longueur. Trois paquets de frange restent séparés du corps pour suivre
les pas, les secousses et le vent. Une unité du monde vaut un centimètre.

© 2026 Alejandro San Martin Lamas — MIT OR Apache-2.0
"""
import math
import random

from outils import cr, dense, ellipse_pts, hspan, mix, shade, vspan


NECK_BASE = (77.0, 46.0)
# Le fond s'arrête au-dessus des longues pointes libres. La croupe est
# plate : les longues mèches donnent le volume, pas un gigot de race à viande.
BODY = [
    (0.5, 60.0), (2.0, 68.0), (7.0, 72.5), (14.0, 74.5),
    (25.0, 75.0), (39.0, 74.8), (54.0, 74.5), (66.0, 73.7),
    (72.0, 73.2), (77.5, 73.1), (81.0, 71.8), (83.0, 67.0), (83.3, 57.0),
    (81.0, 47.0), (78.0, 39.0), (71.0, 34.5), (61.0, 33.1),
    (49.0, 32.8), (36.0, 33.0), (24.0, 33.2), (14.0, 34.0),
    (6.0, 37.0), (1.0, 43.5), (-0.4, 51.0),
]
BODY_D = dense(BODY, True, 8)
# Le corps recouvre largement la base de ce lobe. La nuque (92,79) et le
# dessous de la joue recouvrent l'autre bout : pas de couture au repos.
NECK = [
    (73.5, 36.0), (73.3, 47.0), (73.3, 65.0), (74.0, 73.0), (77.0, 73.5),
    (82.0, 73.1), (86.0, 74.0), (89.0, 81.0), (92.0, 79.5), (96.0, 74.0),
    (98.0, 66.0), (97.0, 57.5), (93.5, 47.0), (89.0, 38.0),
    (84.0, 33.0), (77.0, 33.0), (74.5, 33.3),
]
NECK_D = dense(NECK, True, 8)
# La crête monte seulement devant la boîte du flanc, et le raccord arrière
# reste sous le dos. Le masque d'éclairage commun de la cuisson peut ainsi
# recouvrir ce raccord sans imprimer une pointe claire du cou, immobile sur
# le dos, quand l'animal retourné abaisse l'encolure pour brouter.

# Les trois nappes de l'encolure : sa longue laine pend au lieu de tourner
# avec elle. Chaque nappe est un groupe de rangées de mèches entières, de la
# racine à la pointe ; son attache, sur l'axe de l'encolure, suit le cou, et
# la nappe pend de là, à la verticale (docs/rig.md, « Les nappes »).
#   1. le poitrail, la rangée du bas : son attache est la base de l'encolure,
#      elle ne bouge pas quand il broute (c'est la laine du poitrail) ;
#   2. le milieu ; 3. le haut, sous la nuque (la rangée du haut et la crête).
# (attache de chaque nappe). Derrière `NECK_HIDDEN`, la réserve cachée par le
# flanc reste à l'encolure et tourne avec elle : elle fait le dessus du cou
# baissé.
# Avec le bord du haut de chacune (la ligne d'où elle se balance).
NECK_SHEETS = [(NECK_BASE, 47.0), ((82.0, 57.0), 59.0), ((87.5, 69.0), 81.0)]
NECK_HIDDEN = 82.0


def neck_sheet(k):
    """La nappe `k` pour les mèches d'une rangée, sauf celles de la réserve."""
    return lambda x, h: k if x >= NECK_HIDDEN else None


def _clamp(x):
    return max(0.0, min(1.0, x))


def _light(x, h):
    """La lumière du dessus et les grosses masses souples du flanc."""
    t = _clamp((h - 17.0) / 59.0)
    # Les nappes voisines ne prennent pas toutes la lumière de face : le
    # long flanc a de larges sillons, pas un aplat ivoire uniforme.
    packs = 0.095 * math.sin(x / 6.1 + 0.5 * math.sin(h / 9.0))
    packs += 0.047 * math.sin(x / 2.8 - h / 11.0)
    front = 0.10 * _clamp((x - 58.0) / 25.0)
    return _clamp(0.13 + 0.83 * t ** 0.9 + packs - front)


def _palette(doc):
    """Dégradés partagés : chaque mèche est ronde, avec un sillon étroit.

    Les couleurs sont ramenées à douze niveaux de lumière et quatre degrés
    de roux. Les dégradés en coordonnées relatives restent valables pour les
    mèches de toutes largeurs, sans masque ni découpe dans le SVG livré.
    """
    if "meche-0-0" in doc.ids:
        return
    for warmth in range(4):
        for light in range(12):
            ident = f"meche-{warmth}-{light}"
            k = light / 11.0
            base = mix("#B7AA8D", "#EDE7D6", k)
            base = mix(base, mix("#947451", "#DDC09B", k), warmth * 0.22)
            colors = [shade(base, 0.86), shade(base, 0.97), mix(base, "#FBF7E9", 0.18),
                      base, shade(base, 0.93)]
            stops = "".join(f'<stop offset="{p}" stop-color="{c}"/>'
                            for p, c in zip((0, 0.18, 0.4, 0.7, 1), colors))
            doc.defs.append(f'<linearGradient id="{ident}" x1="0" y1="0" x2="1" y2="0">'
                            f'{stops}</linearGradient>')
            doc.ids.add(ident)
    doc.lin("toison-fond", (0, 75), (0, 25),
            [(0, "#E5DECB"), (0.45, "#C9BDA2"), (1, "#AE9E7E")])


def _lock(doc, x, h, length, width, angle, light, rng, warmth=0.0, free=False, limit=None):
    """Une mèche en pointe, parcourue de fibres longitudinales irrégulières.

    Les deux bords ondulent lentement et indépendamment. La laine ne fait
    aucun tire-bouchon ; son bout se sépare en quelques jarres plus fines.
    `free` ajoute les fibres qui dépassent du bord de la frange. Une mèche
    dont la pointe ne passe pas `limit` se redresse vers la verticale
    jusqu'à ce qu'elle y passe ; ses tirages au hasard ne changent pas.
    """
    light = _clamp(light)
    a = math.sin(angle)
    b = math.cos(angle)
    bend = rng.uniform(-1.2, 1.2)
    phase = rng.uniform(0, 2 * math.pi)
    wave = rng.uniform(0.25, 0.55)
    lean = rng.uniform(-0.12, 0.12)

    def axis(t):
        ds = length * t
        drift = bend * math.sin(math.pi * t * 0.8) + wave * math.sin(t * 7.1 + phase) * t
        return (x + a * ds + b * drift, h - b * ds + a * drift)

    def half(t):
        # Une longue pointe très effilée, les épaules des mèches se fondant
        # sous la rangée du dessus. L'irrégularité reste plus fine qu'un cm.
        taper = (1 - t) ** 0.32 * (1 - 0.18 * t)
        taper *= 0.18 + 0.82 * min(1.0, t / 0.14)
        return width * 0.5 * taper

    def fibre(t, frac):
        cx, ch = axis(t)
        shift = frac * half(t) + lean * math.sin(t * 7.0 + phase) * width * t
        return (cx + b * shift, ch + a * shift)

    turns = 0
    while limit is not None and not limit(axis(1.0)) and turns < 40:
        angle -= 0.04
        a, b = math.sin(angle), math.cos(angle)
        turns += 1
    left, right = [], []
    for i in range(9):
        t = i / 8.0
        cx, ch = axis(t)
        hw = half(t)
        sl = 1 + 0.06 * math.sin(t * 15.0 + phase)
        sr = 1 + 0.065 * math.sin(t * 12.0 + phase + 1.7)
        left.append((cx - hw * b * sl, ch - hw * a * sl))
        right.append((cx + hw * b * sr, ch + hw * a * sr))
    outline = left + right[::-1]
    warm = min(3, max(0, round(warmth * 3)))
    level = min(11, max(0, round(light * 11)))
    doc.el(cr(outline, nd=2, t=0.6), fill=f"url(#meche-{warm}-{level})")
    dark, bright = [], []
    for i in range(rng.randint(5, 8)):
        frac = -0.84 + i * 1.68 / 7.0 + rng.uniform(-0.06, 0.06)
        t0 = rng.uniform(0.04, 0.19)
        t1 = rng.uniform(0.78, 0.97)
        run = [fibre(t0 + (t1 - t0) * j / 6.0, frac) for j in range(7)]
        (bright if i % 3 else dark).append(cr(run, closed=False, nd=2, t=0.7))
    base = mix("#9E8D6A", "#D0C4A7", light)
    base = mix(base, "#AB8054", warmth * 0.3)
    doc.el("".join(dark), stroke=shade(base, 0.88), sw=0.055, so=0.56, cap="round")
    doc.el("".join(bright), stroke=mix(base, "#FFFBEF", 0.75), sw=0.055, so=0.72, cap="round")
    if free:
        strands = []
        for frac in (-0.7, -0.24, 0.33, 0.74):
            t0 = rng.uniform(0.53, 0.72)
            tip = axis(1.0)
            tip = (tip[0] + frac * width * 0.24 + rng.uniform(-0.3, 0.3),
                   tip[1] - rng.uniform(0.2, 1.4))
            strands.append(cr([fibre(t0, frac), fibre(0.9, frac), tip], closed=False, nd=2))
        doc.el("".join(strands), stroke=mix(base, "#FCF5DB", 0.53), sw=0.065, so=0.83, cap="round")


def _row(doc, rng, x0, x1, height, bottom=None, collar=False, angular=0.0,
         spacing=1.65, root_width=(2.0, 3.2), length=(17.0, 24.0), free=False,
         bounds=None, bounds_extra=1.8, limit=None, mark=None, aside=None):
    """Une rangée irrégulière : aucun alignement en damier sur le flanc.
    `mark(x, h)` : le numéro de la nappe d'encolure d'une mèche, d'après sa
    racine, ou rien. Les mèches d'une nappe sont mises de côté dans `aside`
    (par nappe, dans leur ordre) : `neck` les pose ensuite, par nappe."""
    roots = []
    x = x0 + rng.uniform(0, spacing)
    while x < x1:
        ph = height(x) if callable(height) else height
        ph += rng.uniform(-3.7, 3.7)
        ll = rng.uniform(*length)
        if bottom is not None:
            bh = bottom(x) if callable(bottom) else bottom
            ll = max(6.0, ph - bh + rng.uniform(-1.8, 1.8))
        if bounds is not None:
            span = bounds(x)
            if span is None:
                x += spacing
                continue
            ph = min(ph, span[1] - 0.25)
            if ph < span[0] + 1.0:
                x += spacing
                continue
            ll = min(ll, ph - span[0] + bounds_extra)
        angle = angular + rng.uniform(-0.08, 0.08)
        angle += 0.12 * math.sin(x / 6.0 + ph / 14.0)
        angle += 0.06 * math.sin(x / 3.2 - ph / 19.0)
        if x < 13:
            angle -= 0.27 * (13 - x) / 13
            # Le croupion reste rond : l'obliquité vient des mèches couchées
            # du dessus, pas de longues piques qui sortiraient de la fesse.
            angle = max(angle, -0.15 if ll > 12 else -0.45)
        warmth = _clamp((x - 76) / 15) if collar else 0.0
        roots.append((x, ph, ll, rng.uniform(*root_width), angle, warmth))
        x += spacing * rng.uniform(0.73, 1.18)
    rng.shuffle(roots)
    for x, h, ll, ww, angle, warmth in roots:
        k = mark(x, h) if mark else None
        start = len(doc.out)
        _lock(doc, x, h, ll, ww, angle, _light(x, h - 0.5 * ll) + rng.uniform(-0.025, 0.025),
              rng, warmth=warmth, free=free, limit=limit)
        if k:
            aside.setdefault(k, []).extend(doc.out[start:])
            del doc.out[start:]


def body(doc):
    """Corps et frange indépendante, les racines des mèches cachées dessous."""
    _palette(doc)
    rng = random.Random(53065)
    doc.comment("Toison : longue laine jarreuse en mèches ouvertes, sans boucles")
    doc.open("body")
    # La frange pend sous le sternum : elle appartient au corps, jamais aux
    # pattes. Chaque paquet est attaché à une ligne horizontale ; seules ses
    # pointes partent sur le côté lorsque le moteur le cisaille.
    doc.el(cr(BODY, t=0.75), fill="url(#toison-fond)")
    for k, lo, hi in ((1, 21.0, 33.4), (2, 33.0, 46.6), (3, 46.2, 60.2)):
        doc.open(f"sway-{k}")
        _row(doc, rng, lo, hi, 40.0,
             bottom=lambda x: 16.5 + 1.0 * math.sin(x * 0.7),
             root_width=(2.2, 3.3), free=True, spacing=0.9)
        doc.pivot(f"sway-{k}", (lo + hi) / 2.0, 34.0)
        doc.close()
    # Les mèches arrière et celles du poitrail restent solidaires de leurs
    # masses. Les pattes ne se découvrent qu'au-dessous des genoux/jarrets.
    _row(doc, rng, 1.0, 21.0, lambda x: 43.0 + 0.1 * x,
         bottom=lambda x: 25.0 + 1.0 * math.sin(x / 3), free=True)
    _row(doc, rng, 60.0, 78.5, lambda x: 39.0 + (x - 60) * 0.28,
         bottom=lambda x: 22.0 + (x - 60) * 0.35, free=True)
    # Du bas vers le dos : les mèches longues de la rangée suivante masquent
    # le départ de celles du dessous. Pas de traits transversaux de couture.
    for base in (44.0, 54.0, 63.0):
        def height(x, h=base):
            span = vspan(BODY_D, x)
            return min(h + 1.8 * math.sin(x / 8.0 + h), span[1] - 0.8)
        _row(doc, rng, 1.0, 80.0, height, spacing=0.9, free=False,
             length=(20.0, 26.0), root_width=(1.8, 2.8))
    # La raie du dos : elle forme un dessus presque droit. Les racines,
    # couchées et un peu inclinées, se partagent vers les deux flancs.
    _row(doc, rng, 1.7, 77.5, lambda x: vspan(BODY_D, x)[1] - 2.1,
         spacing=0.9, root_width=(1.5, 2.2), length=(19.0, 24.0), angular=-0.22)
    # Le bord de l'échine est couché : de petites mèches souples rejoignent
    # les longues nappes du flanc, au lieu d'une bande nue et raide.
    _row(doc, rng, 3.0, 73.0, lambda x: vspan(BODY_D, x)[1] - 0.6,
         spacing=0.85, root_width=(1.0, 1.5), length=(5.0, 8.5), angular=-0.60,
         bounds=lambda x: vspan(BODY_D, x))
    # Quelques jarres libres accrochent la lumière sur le dessus. Une mèche
    # plus claire et une autre plus sombre indiquent la séparation centrale,
    # sans dessiner une ligne sombre de bout en bout.
    for x in (8, 12, 19, 27, 31, 40, 48, 58, 67, 71):
        h = vspan(BODY_D, x)[1]
        dx = rng.uniform(-2.0, 1.0)
        doc.line([(x - 1.0, h - 0.8), (x, h + 0.25), (x + dx, h + rng.uniform(0.3, 0.9))],
                 "#EDE7D4", 0.07, 0.9)
    # De larges creux de lumière donnent du volume aux paquets de mèches.
    # Les bords s'éteignent dans le flanc : aucune bande sombre découpée,
    # aucun contour transversal, et les fibres restent visibles au travers.
    for k, x, h, rx, ry, alpha in ((1, 23, 45, 3.7, 22, 0.15),
                                  (2, 46, 43, 3.1, 23, 0.13),
                                  (3, 64, 46, 4.4, 20, 0.15)):
        fill = doc.ell(f"creux-toison-{k}", (x, h), rx, ry, -0.06,
                       [(0, "#7C7056", alpha), (0.5, "#8F8062", alpha * 0.65),
                        (1, "#AA9B7E", 0)])
        doc.el(cr(ellipse_pts(x, h, rx, ry, 0.06, 24)), fill=fill)
    doc.pivot("body", 43.0, 33.0)
    doc.close()


def neck(doc):
    """Encolure cachée sous le flanc, roux diffus à l'approche de la tête."""
    _palette(doc)
    rng = random.Random(53066)
    doc.comment("Encolure : longues mèches crème au collier roussâtre, sous la tête et le corps")
    doc.open("neck")
    doc.el(cr(NECK, t=0.7), fill="url(#toison-fond)")
    # Tout le lobe existe, y compris ce que la tête et le flanc cachent :
    # brouter et dormir ne découvrent pas une coupe vide au fond du cou.
    # La gorge est rase (fiche § 3) : une mèche dont la pointe sortirait
    # devant l'encolure pendrait sous le menton comme une barbiche ; elle
    # se redresse le long de la gorge.
    def throat(p):
        sp = hspan(NECK_D, p[1])
        return sp is None or p[0] <= sp[1] + 1.0
    aside = {}
    for sheet, (base, xmin, xmax) in enumerate(((43, 69, 88), (55, 68, 94), (66, 69, 96)), 1):
        def height(x, h=base):
            sp = vspan(NECK_D, x)
            return min(h, sp[1] - 0.7) if sp else h
        def bottom(x):
            sp = vspan(NECK_D, x)
            return (sp[0] - 1.5) if sp else 36
        _row(doc, rng, xmin, xmax, height,
             bottom=bottom if base == 43 else None,
             spacing=0.9, root_width=(1.9, 2.7), length=(14, 18),
             angular=0.13, collar=True, free=False,
             bounds=lambda x: vspan(NECK_D, x), bounds_extra=-0.4, limit=throat,
             mark=neck_sheet(sheet), aside=aside)
    _row(doc, rng, 74, 94, lambda x: vspan(NECK_D, x)[1] - 0.1,
         spacing=0.8, root_width=(1.3, 2.3), length=(16, 21),
         angular=0.09, collar=True, bounds=lambda x: vspan(NECK_D, x), bounds_extra=-0.4,
         limit=throat, mark=neck_sheet(3), aside=aside)
    # Les nappes, par-dessus la réserve qui reste à l'encolure.
    for k, (pivot, edge) in enumerate(NECK_SHEETS, 1):
        doc.open(f"neck-sway-{k}")
        doc.out.extend(aside.get(k, []))
        doc.pivot(f"neck-sway-{k}", *pivot)
        doc.pivot(f"neck-sway-{k}-top", pivot[0], edge)
        doc.close()
    doc.pivot("neck", *NECK_BASE)
    doc.close()
