#!/usr/bin/env python3
"""Un bélier Awassi local de quatre ans, en juillet (fiche docs/races/awassi.md).

Profil droit, repousse de laine de 3–4 cm, 75 cm au garrot ; une unité SVG
vaut un centimètre. Repère de travail : x vers le nez, h au-dessus du sol.
Le grain, le poil et les rides ont des graines fixes. Rien ne vient des
photos : ce sont des références anatomiques, jamais des textures.

Usage : python3 tools/races/awassi/gen_awassi.py pets/awassi/awassi.svg
        python3 tools/races/variantes.py awassi      # la brebis et l'agneau
© 2026 Alejandro San Martin Lamas — MIT OR Apache-2.0
"""
import math
from pathlib import Path
import random
import sys
import xml.etree.ElementTree as ET

HERE = Path(__file__).resolve().parent
sys.path.insert(0, str(HERE.parent))
import articulations as A  # noqa: E402
import variantes  # noqa: E402
from outils import (Doc, Point, Polygon, S, add, sub, mul, lerp, norm, rot,  # noqa: E402
                    clamp, cr, dense, poly, geom_d, poisson, mix, shade,
                    qstroke, lstroke, runs_path)

doc = Doc()
rng = random.Random(1985)  # la monographie d'Epstein, référence de la fiche

# Les autres dessins de la race (tools/races/variantes.py), d'après la fiche :
# la brebis (§ 1.5) et l'agneau (§ 1.6). Sans variante : le bélier.
VARIANTE = sys.argv[2] if len(sys.argv) > 2 else ""
PROPORTIONS = {
    "brebis": dict(  # Brebis Awassi locale : 68 cm au garrot, sans cornes, encolure fine, queue grasse plus petite, mamelle.
        garrot=(75, 68), corps=(1.0, .97), encolure=(1.0, .86), queue=(.74, .68)),
    "agneau": dict(  # Agneau Awassi de deux semaines : 41 cm au garrot, tête brune, robe crème teintée de brun clair, queue mince.
        garrot=(75, 41), corps=(.84, .82), pattes=(1.15, 1.14), encolure=(.9, .85),
        tete=(1.02, 1.18), oreilles=1.08, port=8),
}
POLL = (94.0, 84.0)
NECK_PIVOT = (71.0, 54.0)
TH = math.radians(43.0)
HU, HV = (math.cos(TH), -math.sin(TH)), (-math.sin(TH), -math.cos(TH))


def HL(u, v):
    return add(POLL, add(mul(HU, u), mul(HV, v)))


def HLs(pts):
    return [HL(*p) for p in pts]


def ellipse(c, rx, ry, a=0.0, n=32):
    return [add(c, rot((rx * math.cos(2 * math.pi * k / n),
                        ry * math.sin(2 * math.pi * k / n)), a)) for k in range(n)]


def region(pts):
    return Polygon(dense(pts, True, 8)).buffer(0)


def glow(gid, c, rx, ry, col, alpha, clip=None, angle=0):
    """Une lumière douce, découpée au calcul : aucun masque ni filtre SVG."""
    geom = Polygon(ellipse(c, rx, ry, angle))
    if clip is not None:
        geom = geom.intersection(clip)
    if geom.is_empty:
        return
    g = doc.rad(gid, c, ry, [(0, col, alpha), (0.45, col, alpha * .7),
                            (1, col, 0)], sx=rx / ry, angle=angle)
    doc.path(geom_d(geom), fill=g)


def hairs(area, seed, density=.75, flow=(-.3, -.9), light="#F5EADB", dark="#887563",
          length=(.35, .8), alpha=.35):
    """Le poil ras, par teinte ; les bords ne sortent pas de leur pièce."""
    r = random.Random(seed)
    doc.begin_batch()
    for p in poisson(area.buffer(-.25), density, r, k=12):
        f = norm(flow(p) if callable(flow) else flow)
        L = r.uniform(*length)
        e = add(p, mul(f, L))
        if not area.contains(Point(e)):
            continue
        c = add(lerp(p, e, .5), (r.uniform(-.1, .1), r.uniform(-.08, .08)))
        doc.stroke(qstroke(p, c, e), light if r.random() < .57 else dark,
                   .065, alpha)
    doc.flush()


def wool(area, seed, brown=False, step=1.0, edge=True):
    """La repousse grossière : petits paquets largement ondulés, jamais des
    boucles serrées. Leur orientation varie doucement en nappes de 5–8 cm.
    Le kemp clair, un peu plus raide, est plus présent sur la croupe."""
    r = random.Random(seed)
    pts = sorted(poisson(area.buffer(-.22), step, r, k=14), key=lambda p: p[1])
    doc.begin_batch()
    for x, h in pts:
        ang = .4 * math.sin(x / 5 + math.sin(h / 7)) + r.uniform(-1.25, 1.25)
        u = (math.cos(ang), math.sin(ang))
        v = (-u[1], u[0])
        L, w = r.uniform(.35, 1.9), r.uniform(-.2, .55)
        p = add((x, h), mul(u, -L * .5))
        e = add((x, h), mul(u, L * .5))
        c = add((x, h), mul(v, -w))
        if not all(area.contains(Point(q)) for q in (p, e, c)):
            continue
        if brown:
            dk, lt = "#372017", "#A27153"
        else:
            tv = clamp((h - 36) / 40)
            dk = mix("#8E7B67", "#B5A18A", round(tv * 5) / 5)
            lt = mix("#D4C4AF", "#FFF5E7", round(tv * 5) / 5)
        alpha = r.choice((.12, .16, .2, .26, .3))
        doc.stroke(qstroke(p, c, e), dk, r.choice((.07, .09, .11)), alpha)
        if r.random() < .58:
            # L'éclat n'épouse pas le croissant entier : pas d'écailles.
            q = add((x, h), (r.uniform(-.3, .3), r.uniform(.05, .25)))
            doc.stroke(qstroke(q, add(q, mul(u, L*.2)), add(q, mul(u, L*.42))),
                       lt, .1, r.choice((.2, .28, .4)))
        if r.random() < .25:
            d = (r.uniform(-.25, .25), r.uniform(.4, .9))
            doc.stroke(lstroke((x, h), add((x, h), d)), lt, .065, .45)
    doc.flush()
    if edge:
        # Quelques fibres fines interrompent le bord sans le festonner.
        doc.begin_batch()
        for geom in ([area] if isinstance(area, Polygon) else area.geoms):
            if not isinstance(geom, Polygon):
                continue
            boundary = geom.exterior
            for k in range(int(boundary.length / .65)):
                t = (k + r.uniform(-.4, .4)) * .65
                if r.random() < .3:
                    continue
                p = boundary.interpolate(t)
                a = boundary.interpolate(max(0, t - .12))
                b = boundary.interpolate(min(boundary.length, t + .12))
                n = norm((b.y - a.y, a.x - b.x))
                # Le signe dépend de l'orientation du polygone.
                tip = add((p.x, p.y), mul(n, .4))
                if area.contains(Point(tip)):
                    n = mul(n, -1)
                n = rot(n, r.uniform(-.65, .65))
                L = r.uniform(.16, .5)
                col = "#8C6048" if brown else ("#F4E9D7" if p.y > 60 else "#B4A18A")
                doc.stroke(qstroke(add((p.x, p.y), mul(n, -.2)),
                                   add((p.x, p.y), mul(n, L * .4)),
                                   add((p.x, p.y), mul(n, L))), col, .055, .65)
        doc.flush()


# ================================================================== silhouettes
# 75 cm au garrot ; hanches à 74 cm, puis croupe à 40° vers la queue.
BACK = [(0, 62.8), (3, 66.7), (7, 70.8), (11.5, 73.5), (17, 74.2),
        (25, 74.3), (35, 74.1), (45, 74), (55, 74.1), (61, 74.4), (66, 75)]
def arc(start, h0, h1, radius, left):
    sign = 1 if left else -1
    a = math.radians(h0 + 90*sign)
    c = add(start, (radius*math.cos(a), radius*math.sin(a)))
    n = int(abs(h1-h0)/3)+2
    return [add(c, (radius*math.cos(math.radians(h0+(h1-h0)*k/n-90*sign)),
                    radius*math.sin(math.radians(h0+(h1-h0)*k/n-90*sign)))) for k in range(n+1)]


def ray(start, angle, length):
    d = (math.cos(math.radians(angle)), math.sin(math.radians(angle)))
    n = max(1, int(length))
    return [add(start, mul(d, length*k/n)) for k in range(1, n+1)]


def turn_neck(p, a):
    return add(NECK_PIVOT, rot(sub(p, NECK_PIVOT), -a))


# Comme chez la Lacaune, le bord caché du cou et l'épaule se relaient :
# l'encolure abaissée a un dessus tangent au garrot, sans angle de carton.
GRAZE_TURN = 1.7483  # pose graze : 103,0348° moins 2,8648° du corps
SLOPE = -40.0
_shoulder = arc(BACK[-1], 6.8, SLOPE, 12, False)
SHOULDER_TANGENT = _shoulder[-1]
_shoulder += [p for p in ray(_shoulder[-1], SLOPE, 18) if p[0] < 83]
SHOULDER = _shoulder + arc(_shoulder[-1], SLOPE, -100, 2.2, False)[1:]
SHOULDER += [(83.2, 55), (82.6, 49), (80.5, 44), (77, 41)]
BELLY = [(77, 41), (72, 39.9), (65, 39.5), (56, 39), (47, 38.7),
         (38, 39), (30, 40), (24, 42), (20, 44), (16, 44.8), (11, 44.5),
         (6, 45.2), (2.4, 48), (.5, 53), (0, 58), (0, 62.8)]
BODY = BACK + SHOULDER[1:] + BELLY[1:-1]
BP = region(BODY)
# L'encolure basse, en avant, cachée sous une épaule ronde. Le grand retour
# circulaire autour du pivot fournit le fond quand elle se redresse.
NAPE = HL(-1.5, 0)
CREST = [NAPE, (89.5, 83.5), (85, 82), (80, 80.2), (75, 78), (70, 76), (66, 75)]
def hidden_neck():
    v = sub(BACK[-1], NECK_PIVOT)
    h0 = math.degrees(math.atan2(v[0], -v[1])) % 360
    h1 = SLOPE + 180 + math.degrees(GRAZE_TURN)
    t = math.radians(SLOPE)
    n = (-math.sin(t), math.cos(t))
    target = sum(SHOULDER_TANGENT[i]*n[i] for i in (0, 1)) - .05
    lo, hi = .5, 45
    for _ in range(60):
        radius = (lo+hi)/2
        q = turn_neck(arc(BACK[-1], h0, h1, radius, True)[-1], GRAZE_TURN)
        if sum(q[i]*n[i] for i in (0, 1)) > target:
            hi = radius
        else:
            lo = radius
    a = arc(BACK[-1], h0, h1, radius, True)
    q = turn_neck(a[-1], GRAZE_TURN)
    length = (q[0]-SHOULDER_TANGENT[0])*math.cos(t)+(q[1]-SHOULDER_TANGENT[1])*math.sin(t)-.6
    top = a + ray(a[-1], h1, length)
    dive = arc(top[-1], h1, h1+80, 3, True)[1:]
    low = [(62, 50), (65, 44), (71, 40.6), (77, 40.5), (81, 43)]
    return top+dive+[lerp(dive[-1], low[0], .5)]+low


HIDDEN = hidden_neck()
THROAT = [(81, 43), (84, 47), (86, 51), (88, 56), (90.4, 62),
          (93.3, 68), HL(5.5, 9), HL(8, 6), HL(7, 1), HL(3, -2), NAPE]
NECK = CREST + HIDDEN[1:] + THROAT[1:-1]
NP = region(NECK)
# La transition mêlée fait 6 cm : seul l'avant de l'encolure est brun.
BR = region([(83, 85), (100, 90), (107, 67), (94, 61), (89, 58),
             (84, 60), (80, 68), (79, 77)]).intersection(NP)


def neck():
    doc.open("neck")
    doc.path(cr(NECK), fill="url(#robe)")
    # Le bord nu du cou ne reçoit pas les fibres blanches du fond : elles
    # resteraient visibles hors du brun comme une barbe ou un peigne.
    wool(NP, 16, step=.95, edge=False)
    doc.path(geom_d(BR), fill=doc.lin("cou-brun", (87, 83), (89, 56),
                                    [(0, "#81563E"), (.45, "#5A3427"), (1, "#362017")]))
    wool(BR, 17, brown=True, step=.8)
    # Le bord brun se fond dans le blanc par des poils mêlés irréguliers.
    r = random.Random(18)
    doc.begin_batch()
    transition = NP.buffer(-.25).intersection(
        BR.boundary.difference(NP.boundary.buffer(.35)).buffer(2.5))
    for p in poisson(transition, .3, r, k=10):
        if p[0] > 90:
            continue
        d = rot(norm((-.7, -.8)), r.uniform(-.8, .8))
        e = add(p, mul(d, r.uniform(.25, 1.1)))
        if NP.contains(Point(e)):
            doc.stroke(qstroke(p, add(lerp(p, e, .5), (.2, .1)), e),
                       r.choice(("#846249", "#AB8C70", "#C2A893", "#6B4430")),
                       r.choice((.07, .09, .12)), r.choice((.35, .45, .6)))
    doc.flush()
    glow("cou-muscle", (91, 73), 3, 10, "#BF9270", .28, BR, angle=-.45)
    glow("cou-gorge", (92, 66), 2, 8, "#2E1813", .4, NP, angle=-.4)
    # La gorge nue et le fanon mince du bélier : pas une barbe de chèvre.
    throat = region([(87.8, 57), (90, 61), (94, 68), (95, 72),
                     (91.7, 66), (88, 60), (86.8, 57)]).intersection(BR)
    doc.path(geom_d(throat), fill=doc.lin("gorge-nue", (89, 63), (91.5, 61),
                                      [(0, "#5F3E2B"), (1, "#513023")]))
    hairs(throat, 20, .4, flow=(-.4, -1), light="#91674F", dark="#29170F", length=(.4, .7))
    # La frange discrète ne pend que de 2–3 cm sous la gorge.
    doc.begin_batch()
    for k in range(28):
        t = (k + rng.uniform(-.4, .4)) / 27
        p = (88.1 + 3.5 * t, 57.7 + 8.3 * t)
        if not NP.buffer(.8).contains(Point(p)):
            continue
        e = add(p, (rng.uniform(-.3, .3), -rng.uniform(.3, .9)))
        doc.stroke(qstroke(p, add(lerp(p, e, .5), (.25, 0)), e), "#6D4934", .09, .55)
    doc.flush()
    doc.pivot("neck", NECK_PIVOT)
    doc.close()


def body():
    doc.open("body")
    if not VARIANTE:
        scrotum()
    doc.path(cr(BODY), fill="url(#robe)")
    glow("flanc-lumiere", (38, 64), 33, 15, "#FFF4E2", .45, BP)
    glow("cuisse-muscle", (10, 56), 8, 16, "#F8E9D3", .36, BP, angle=-.2)
    glow("hanche-creux", (21, 64), 5, 10, "#A28C74", .23, BP, angle=.3)
    glow("flanc-creux", (29, 49), 5, 12, "#A38B74", .2, BP, angle=-.7)
    glow("epaule-muscle", (74, 60), 7, 15, "#F7E6CF", .28, BP, angle=-.22)
    glow("ventre-ombre", (44, 39), 31, 6, "#75665B", .34, BP)
    # Quelques nappes de la tonte grossière ; les bords se perdent sous le grain.
    for i, (x, h, a) in enumerate(((22, 67, -.3), (41, 60, .2), (57, 66, -.3), (13, 55, -.6))):
        glow(f"tonte-{i}", (x, h), 7, 2.3, "#AF9B80", .09, BP, angle=a)
    wool(BP, 23, step=.83)
    # Le ventre adulte est peu laineux, plus gris, aux poils couchés.
    belly = region([(21, 41.6), (30, 40.2), (48, 39.8), (66, 40.7), (74, 41.9),
                    (68, 39.2), (48, 38.7), (30, 39.7)]).intersection(BP)
    doc.path(geom_d(belly), fill="#B5A38C", fo=.5)
    hairs(belly, 26, .45, flow=(.2, -1), light="#E6D7C3", dark="#8B7763", length=(.4, 1.1))
    doc.pivot("body", (43, 39))
    doc.close()


# ================================================================== les pattes
# Les paturons sont courts ; 18 cm au carpe, 24 cm au jarret (§ 2.6).
LEGS = [("leg-back-far", "back", 5, True), ("leg-front-far", "front", -5, True),
        ("leg-back-near", "back", 0, False), ("leg-front-near", "front", 0, False)]


def leg(name, kind, dx, far):
    if kind == "front":
        front = [(76, 50), (77, 42), (76, 34), (75, 26), (74.6, 20), (74.7, 18),
                 (74.2, 15), (74, 8), (74.5, 5.4), (76, 2.9)]
        sole = [(76, 2.9), (77.1, .4), (76.9, 0), (72, 0), (71.8, .6)]
        back = [(71.8, .6), (72.3, 2.8), (71.1, 4.6), (71.5, 7), (71.4, 14),
                (71, 18), (71.4, 21), (70.4, 29), (68.7, 35), (68.6, 42), (70, 50)]
        pivot = (72, 49)
        knee = (73, 18.7)
        patch = [(72.8, 21), (74.5, 21.5), (75, 20), (74.4, 18.6), (74.7, 17),
                 (73.8, 16.2), (72.6, 17.4), (72.8, 18.5), (72.1, 19.8)]
    else:
        front = [(18, 53), (19, 46), (17, 40), (14.3, 35), (11, 30), (8.2, 25.5),
                 (6.4, 22), (6, 18), (5.9, 8), (6.7, 4.8), (8, 2.9)]
        sole = [(8, 2.9), (9.1, .4), (8.9, 0), (4, 0), (3.8, .6)]
        back = [(3.8, .6), (4.3, 2.8), (3.1, 4.6), (3.5, 7), (3.4, 17), (2.8, 21),
                (2.1, 23.6), (2.5, 27), (3.5, 34), (4.4, 43), (6, 53)]
        pivot = (12, 52)
        knee = (4.6, 24)
        patch = [(2, 26), (3.5, 28), (6.6, 26), (7.3, 24), (6, 22), (3.2, 22)]
    shift = lambda ps: [add(p, (dx, 0)) for p in ps]
    front, sole, back, patch = map(shift, (front, sole, back, patch))
    LP = region(front + sole[1:] + back)
    x0, _, x1, _ = LP.bounds
    dim = .82 if far else 1
    pal = lambda c: shade(c, dim)
    doc.open(name)
    doc.path(runs_path([front, sole, back + [front[0]]]),
             fill=doc.lin(name + "-poil", (x0, 20), (x1, 20),
                          [(0, pal("#A18D7B")), (.24, pal("#E6DACB")),
                           (.6, pal("#D2C3B2")), (1, pal("#A28D79"))]))
    # Tache du genou et chaussettes, distinctes sur les canons blancs.
    patch_area = region(patch).intersection(LP) if kind == "front" else Polygon()
    if kind == "front":
        doc.path(geom_d(patch_area), fill=pal("#634330"))
        hairs(patch_area, 111 + int(far), .28, flow=(.1, -1),
              light=pal("#906C50"), dark=pal("#513427"), length=(.25, .5), alpha=.35)
    sock = LP.intersection(Polygon([(x0-1, 0), (x1+1, 0), (x1+1, 7),
                                    (x0+1, 8.1), (x0-1, 6.5)]))
    doc.path(geom_d(sock), fill=pal("#593B2E"))
    hairs(LP.difference(sock).difference(patch_area), 30 + len(name) + int(far), .55,
          flow=(-.1 if kind == "front" else -.5, -1), light=pal("#F1E6D6"), dark=pal("#95806C"),
          length=(.45, .9))
    hairs(sock, 39 + int(far), .4, flow=(.1, -1), light=pal("#A17B60"), dark=pal("#372219"),
          length=(.35, .6))
    # Tendons et pointe du jarret : un relief doux, sans dessin d'os noir.
    p = add(knee, (dx, 0))
    doc.path(cr([add(p, (-.8, 4)), add(p, (-1.1, 1)), add(p, (-.6, -2))], False),
             stroke=pal("#8B7562"), sw=.2, so=.45, cap="round")
    # Petits ergots derrière le paturon.
    heel = sole[-1]
    doc.path(cr([add(heel, (-.5, 4)), add(heel, (-.7, 3)), add(heel, (.15, 2.9)),
                 add(heel, (.1, 3.8))]), fill=pal("#4A382B"))
    # Les deux onglons serrés, brun noir, la paroi poussiéreuse et la fente.
    hoof = [sole[0], sole[1], sole[2], sole[3], sole[4], add(sole[-1], (.2, 1.6))]
    doc.path(runs_path([hoof[:3], [hoof[2], hoof[3]], hoof[3:] + [hoof[0]]]),
             fill=doc.lin(name + "-corne", (x0, 1), (x1, 1),
                          [(0, pal("#1E1611")), (.4, pal("#5F5144")), (1, pal("#30221B"))]))
    mid = lerp(sole[3], sole[2], .62)
    doc.path(cr([add(mid, (0, .15)), add(mid, (-.3, 1)), add(mid, (-.65, 2.2))], False),
             stroke=pal("#1E1611"), sw=.16, so=.9, cap="round")
    doc.path(cr([add(sole[0], (-.3, -.15)), add(sole[1], (-.35, .2))], False),
             stroke=pal("#998575"), sw=.2, so=.65, cap="round")
    doc.pivot(name, add(pivot, (dx, 0)))
    doc.close()


def scrotum():
    doc.open("awassi-anatomie")
    # La racine entre largement sous le ventre (44–45 cm ici). Sans ces
    # 4 cm de recouvrement, elle se détachait dès que la cuisse partait en vol.
    pts = [(19.5, 49), (22, 45), (22, 39), (22.5, 34), (21.7, 29), (19.7, 25.1),
           (17.2, 24), (14.6, 25.4), (13.8, 29), (14.5, 34), (16.5, 39), (17, 45)]
    sp = region(pts)
    doc.path(cr(pts), fill=doc.lin("scrotum", (14, 30), (23, 30),
                                  [(0, "#B08072"), (.4, "#DBAF9E"), (.7, "#D3A495"), (1, "#B08072")]))
    doc.path(cr([(18.2, 37), (17.2, 32), (17.5, 28), (18, 25.2)], False),
             stroke="#A97E6C", sw=.14, so=.45, cap="round")
    hairs(sp, 44, .75, flow=(0, -1), light="#EFE2D2", dark="#B1957F", length=(.4, .9), alpha=.38)
    doc.close()


def tail():
    """Coussin de 28 cm, épais de 10 cm de profil : l'autre lobe et la
    gouttière ne sont que suggérés, car la vue est strictement latérale."""
    doc.open("tail")
    pts = [(1.5, 63), (-1.5, 61), (-4.1, 57), (-5.1, 51), (-5.6, 44),
           (-4.8, 38), (-3, 35), (.1, 34.1), (3.3, 35.3), (4.3, 39),
           (4, 45), (3.1, 51), (2.4, 57)]
    tp = region(pts)
    doc.path(cr(pts), fill=doc.lin("queue", (-6, 48), (5, 48),
                                  [(0, "#B5A088"), (.35, "#DFCCB8"), (.7, "#CEBBA6"), (1, "#A8957F")]))
    glow("queue-relief", (-1.8, 49), 3.3, 12, "#F5E3CD", .35, tp)
    wool(tp, 48, step=.65)
    doc.path(cr([(2.4, 58), (3.5, 50), (4.1, 44), (3.5, 38)], False),
             stroke="#7E6C5A", sw=.35, so=.35, cap="round")
    # Seul un étroit retour de peau nue est visible sous le plateau.
    underside = region([(-4, 37), (-2.5, 35), (.2, 34), (3, 35.1), (3.8, 38),
                        (1.5, 37), (-1, 36.6)]).intersection(tp)
    doc.path(geom_d(underside), fill=doc.lin("queue-peau", (-3, 38), (1, 34),
                                         [(0, "#B69781"), (.5, "#C3A18C"), (1, "#C9A291")]))
    doc.path(cr([(-2.8, 35.9), (-.2, 35), (2.4, 35.8)], False),
             stroke="#9C7D69", sw=.16, so=.38, cap="round")
    # La fine pointe remonte de 4–5 cm ; la houppe retombe de son sommet.
    doc.open("tail-tip")
    hook = [(-2.1, 35.7), (-5.5, 36), (-7.7, 38.5), (-7.9, 41),
            (-6.8, 41.6), (-6.7, 39.5), (-5.1, 38), (-2.6, 37.8)]
    doc.path(cr(hook), fill="#D0B59F")
    doc.path(cr([(-7.2, 40.6), (-9.1, 39.8), (-10.1, 37.8), (-10, 35),
                 (-8.7, 32), (-8.2, 32.4), (-8.7, 35.5), (-7.7, 38.4)]), fill="#C9BBA6")
    doc.begin_batch()
    rr = random.Random(52)
    for _ in range(85):
        p = (-7.4 + rr.uniform(-.4, .3), 40.1 + rr.uniform(-.3, .4))
        c = (-10 + rr.uniform(-.4, .4), 38 + rr.uniform(-.4, .4))
        e = (-9.1 + rr.uniform(-.7, .9), 32.5 + rr.uniform(-.6, 1))
        doc.stroke(qstroke(p, c, e), rr.choice(("#E2D6C4", "#8F806D", "#B8A891")), .07, .75)
    doc.flush()
    doc.pivot("tail-tip", (-3.3, 36.7))
    doc.close()
    doc.pivot("tail", (1, 62))
    doc.close()


# ================================================================== tête et cornes
# Le front presque plat, un léger creux, puis le nez busqué. Les joues
# restent minces, comme le profil P6, et le chanfrein est à 43°.
DORSAL = [(-1.5, 0), (-1, -1.4), (1, -2.5), (4, -2.8), (7, -2.6),
          (9, -2.1), (11, -2.2), (14, -2.6), (17, -2.1), (19.5, -.8),
          (21.5, .9), (22.8, 2.5), (23.1, 4), (22.6, 5.1)]
MOUTH = [(22.6, 5.1), (21.3, 5.3), (19.8, 5.5), (18, 5.7)]
NOTCH = [(18, 5.7), (17.3, 7.4), (15.7, 8.6)]
UNDER = [(15.7, 8.6), (12, 9), (8, 9.1), (4.2, 8.6), (1.3, 7), (-.7, 4), (-1.5, 0)]
HP = region(HLs(DORSAL + MOUTH[1:] + NOTCH[1:] + UNDER[1:-1]))
E = HL(11.0, .8)
ALMOND = [(E[0]-1.55, E[1]-.12), (E[0]-1.05, E[1]+.6), (E[0], E[1]+.77),
          (E[0]+1.15, E[1]+.45), (E[0]+1.5, E[1]-.15), (E[0]+.8, E[1]-.65),
          (E[0]-.35, E[1]-.67), (E[0]-1.2, E[1]-.45)]
EP = region(ALMOND)


def ear(name, far=False):
    doc.open(name)
    base = HL(3.5 if not far else 2, 3)
    # Feuille large au bout rond : 16 cm x 8,5 cm, presque verticale.
    F = lambda u, v: add(base, (u + (2 if far else 0), -v))
    shape = [F(0, 0), F(-2, 3), F(-3.7, 8), F(-3.5, 13), F(-1.7, 16),
             F(1.4, 15.7), F(3.8, 12.3), F(4.3, 8), F(2.6, 3.3)]
    area = region(shape)
    doc.path(cr(shape), fill=doc.lin(name + "-brun", F(-3, 8), F(4, 8),
                                    [(0, "#4E3022"), (.35, "#93684B"), (1, "#63402D")]))
    inner = region([F(.1, 2), F(-2.1, 6), F(-2.3, 11), F(-.8, 14), F(1.4, 13.8),
                    F(2.5, 10), F(2, 6)]).intersection(area)
    doc.path(geom_d(inner), fill=doc.lin(name + "-interieur", F(0, 3), F(0, 14),
                                       [(0, "#3E1A0E"), (.45, "#785544"), (1, "#A07A68")]))
    hairs(inner, 61 + int(far), .52, flow=(.2, -1), light="#C3A28D", dark="#60402E", length=(.4, .9))
    doc.path(cr([F(.7, 3), F(-.4, 7), F(.1, 11), F(0, 13.5)], False),
             stroke="#5E3D2B", sw=.18, so=.4, cap="round")
    doc.pivot(name, base)
    doc.close()


# Une grande hélice projetée en C ; le dernier quart s'écarte vers nous,
# ce qui raccourcit la pointe. P6 fournit la silhouette, P5 la section.
HORN_AXIS = [(95.2, 81.8), (91.5, 87.2), (86.8, 89.6), (82, 87.5),
             (78.7, 83.2), (77.4, 77.6), (78.2, 72), (80.8, 66.9),
             (85, 63.5), (90, 62.8), (94.6, 64.1), (97.2, 67), (97.5, 69.7)]


def horn(far=False):
    C = dense(HORN_AXIS, False, 9)
    if far:
        C = [add(POLL, add(mul(sub(p, POLL), .91), (1.2, 1.5))) for p in C]
    n = len(C)
    N, W, acc = [], [], [0.0]
    for i in range(n):
        t = i / (n-1)
        d = norm(sub(C[min(i+1, n-1)], C[max(i-1, 0)]))
        N.append((-d[1], d[0]))
        W.append(6.5 * (1-t) ** .7 + .25)
        if i:
            acc.append(acc[-1] + math.dist(C[i], C[i-1]))
    outer = [add(p, mul(nn, w/2)) for p, nn, w in zip(C, N, W)]
    inner = [add(p, mul(nn, -w/2)) for p, nn, w in zip(C, N, W)]
    # L'autre corne reste derrière la tête ; son arc dépasse au sommet.
    shape = outer + inner[::-1]
    area = Polygon(shape).buffer(0)
    gid = "corne-loin" if far else "corne"
    shape_d = cr(shape[::2]+[shape[-1]], t=.8)
    doc.path(shape_d, fill=doc.rad(gid, (87, 77), 18,
                                      [(0, "#756348"), (.45, "#B39A74"),
                                       (.72, "#DCC6A0"), (1, "#9A8669")]))
    lit = [add(C[i], mul(N[i], W[i]*.38)) for i in range(n)]
    mid = [add(C[i], mul(N[i], -W[i]*.08)) for i in range(n)]
    doc.path(poly(lit + mid[::-1]), fill="#E7D2AF", fo=.38 if not far else .18)
    # Une face étroite à l'ombre souligne la section triangulaire arrondie.
    shadow = [add(C[i], mul(N[i], -W[i]*.4)) for i in range(n)]
    doc.path(poly(inner + shadow[::-1]), fill="#5E4E3B", fo=.4)
    r = random.Random(68 + int(far))
    dark, light = [], []
    s = .35
    while s < acc[-1] - 6:
        i = next(k for k, a in enumerate(acc) if a >= s)
        p, nn, w = C[i], N[i], W[i]
        d = norm(sub(C[min(n-1, i+1)], C[max(0, i-1)]))
        a, b = add(p, mul(nn, w*.48)), add(p, mul(nn, -w*.48))
        c = add(p, mul(d, .55))
        dark.append(qstroke(a, c, b, 2))
        light.append(qstroke(add(a, mul(d, .18)), add(c, mul(d, .18)),
                             add(lerp(a, b, .65), mul(d, .18)), 2))
        s += r.uniform(1.45, 2.2)
    doc.path("".join(dark), stroke="#5E4E3B", sw=.14, so=.6, cap="round")
    doc.path("".join(light), stroke="#F0DBB5", sw=.12, so=.75, cap="round")
    # Les stries longitudinales se perdent dans le dernier quart lisse.
    for f in (-.25, .1, .3):
        for j in range(2, int(n*.76), 21):
            pts = [add(C[i], mul(N[i], W[i]*f)) for i in range(j, min(j+14, int(n*.8)), 2)]
            if len(pts) > 1:
                doc.path(cr(pts, False), stroke="#6B5A43", sw=.08, so=.26, cap="round")
    tip = int(n*.83)
    tip_area = Polygon(outer[tip:] + inner[tip:][::-1]).buffer(0)
    doc.path(geom_d(tip_area), fill=doc.lin(gid + "-pointe", C[tip], C[-1],
                                         [(0, "#7B542E", 0), (1, "#7B542E")]))
    doc.path(shape_d, stroke="#756348", sw=.18, so=.55, join="round")


def jaw():
    doc.open("jaw")
    doc.path(cr(HLs([(15, 5), (18, 4), (21.8, 3.7), (22.7, 4.5),
                     (21.6, 5.1), (18, 5.8), (15, 7.9)])), fill="#2B1511")
    doc.path(cr(HLs([(18, 4.9), (20.4, 4.5), (22, 4.5), (21.2, 4.9), (18.4, 5.2)])), fill="#B57D78")
    doc.path(cr(HLs([(21.1, 4.55), (22.1, 4.45), (22.3, 4.8), (21.2, 4.9)])), fill="#E1D5BE")
    chin = HLs([(17.8, 5.65), (20, 5.35), (22.6, 5.05), (22.8, 5.8),
                (21.5, 7), (19, 7.8), (15.7, 8.6), (13.5, 8.8), (14, 7.5)])
    doc.path(cr(chin), fill=doc.lin("menton", HL(19, 5), HL(19, 8),
                                   [(0, "#876047"), (.5, "#6D503A"), (1, "#43281B")]))
    hairs(region(chin), 70, .35, flow=(-.6, -.5), light="#B18A69", dark="#4D3321", length=(.25, .5))
    doc.pivot("jaw", HL(17, 6.1))
    doc.close()


def eyes():
    # Le trait fermé est caché sous l'œil ouvert, comme sur les deux races
    # récentes ; le moteur le découvre au clignement et pendant le sommeil.
    doc.path(cr([(E[0]-1.4, E[1]), (E[0]-.2, E[1]-.2), (E[0]+1.3, E[1]-.1)], False),
             stroke="#281B13", sw=.18, so=.9, cap="round")
    doc.open("eyes")
    doc.path(cr(ALMOND), fill="#3E2A1E")
    doc.open("pupils")
    ic = add(E, (-.3, 0))
    doc.path(cr(ellipse(ic, 1.15, 1.15)), fill=doc.rad("iris", ic, 1.17,
                  [(0, "#665034"), (.3, "#9C7A52"), (.7, "#7A5A3A"), (1, "#3E2A1E")]))
    for k in range(24):
        a = k * 2 * math.pi / 24
        doc.path(lstroke(add(ic, (.56*math.cos(a), .56*math.sin(a))),
                          add(ic, (1.05*math.cos(a), 1.05*math.sin(a))), 2),
                 stroke="#C4A06C" if k % 2 else "#4A3321", sw=.045, so=.45, cap="round")
    doc.path(cr([add(ic, (-.83, -.15)), add(ic, (-.7, .23)), add(ic, (.68, .24)),
                 add(ic, (.86, -.08)), add(ic, (.6, -.25)), add(ic, (-.6, -.25))]), fill="#0E0B09")
    doc.pivot("pupils", ic)
    doc.close()
    doc.path(cr(ellipse(add(E, (-.6, .35)), .18, .13)), fill="#EDE4D8", fo=.85)
    doc.pivot("eyes", E)
    doc.close()
    # Cache opaque autour de l'amande : l'iris ne sort pas quand il regarde.
    cover = Polygon(ellipse(E, 3.8, 3)).intersection(HP).difference(EP)
    doc.path(geom_d(cover), fill="url(#face)", rule="evenodd")
    doc.path(cr(ALMOND), stroke="#2E1E16", sw=.17, so=.9, cap="round")
    doc.path(cr(ALMOND[:5], False), stroke="#BD926D", sw=.15, so=.5, cap="round")


LIP = HLs([(19.3, 5.4), (19.2, 4.3), (20.2, 3.5), (22.6, 3.3),
            (23.15, 3.8), (23, 4.6), (22.6, 5.12), (21.3, 5.4)])


def head():
    doc.open("head")
    if not VARIANTE:
        horn(far=True)
    ear("ear-far", far=True)
    jaw()
    d = runs_path([HLs(DORSAL), HLs(MOUTH), HLs(NOTCH), HLs(UNDER)])
    doc.path(d, fill=doc.lin("face", HL(8, -3), HL(8, 9),
                            [(0, "#93684B"), (.38, "#765039"), (.7, "#654330"), (1, "#43281B")]))
    glow("chanfrein", HL(16, -1.5), 6, 1.8, "#C59A75", .36, HP, angle=-TH)
    glow("joue", HL(9, 5), 5, 3, "#B68B67", .3, HP, angle=-TH)
    glow("joue-dessous", HL(10, 8), 6, 1.2, "#28170F", .3, HP, angle=-TH)
    glow("arcade", add(E, (0, 1.25)), 2.2, .65, "#CAA17A", .45, HP)
    glow("orbite", add(E, (0, .25)), 2.3, 1.3, "#332015", .3, HP)
    hairs(HP.difference(EP.buffer(.5)), 74, .32,
          flow=lambda p: norm(add(mul(HU, -.9), mul(HV, .1))), light="#B68C68", dark="#3D261A",
          length=(.25, .58), alpha=.32)
    # Le nez charnu, nu près des naseaux, porte un reflet humide discret.
    muzzle = region(HLs([(19, 1.6), (21, .8), (22.6, 2), (23.1, 3.8),
                        (22.6, 5.1), (20.4, 5), (18.8, 3.8)])).intersection(HP)
    doc.path(geom_d(muzzle), fill=doc.rad("mufle", HL(21.5, 3), 3,
                                        [(0, "#8B6850", .7), (1, "#6D503A", 0)]))
    nostril = HLs([(20.4, 2.15), (21.1, 2.2), (22.3, 2.7), (22.7, 3.1),
                   (22.4, 3.4), (21.9, 3.13), (21.2, 2.65)])
    doc.path(cr(nostril), fill="#1A120E")
    doc.path(cr(HLs([(21, 2.7), (21.7, 3.1), (22.25, 3.35)]), False),
             stroke="#AD8C70", sw=.12, so=.55, cap="round")
    doc.path(cr(HLs(MOUTH), False), stroke="#2E1E16", sw=.2, so=.85, cap="round")
    doc.path(cr(HLs([(23, 3.8), (22.9, 4.4), (22.6, 5.1)]), False),
             stroke="#4D3321", sw=.14, so=.6, cap="round")
    # La fosse du larmier et le pli du masséter, sans joues en peluche.
    doc.path(cr([add(E, (1.7, -.4)), add(E, (1.4, -1.4)), add(E, (.6, -2.2))], False),
             stroke="#3D2519", sw=.18, so=.5, cap="round")
    eyes()
    ear("ear-near")
    # Le toupet brun-roux est irrégulier ; la face demeure à poil ras.
    toupet = region(HLs([(-1.2, .8), (-1.8, -1.3), (0, -3), (2.3, -3.3),
                         (4.5, -3), (6, -2.3), (5, -.6), (2.8, .3)]))
    doc.path(geom_d(toupet), fill="#764838")
    wool(toupet, 79, brown=True, step=.45)
    if not VARIANTE:
        horn()
    # Quelques mèches passent devant la naissance de la corne. Sa section
    # s'enfonce dans le crâne, elle ne se termine pas comme un C cassé.
    roots = region(HLs([(-.9, 2.8), (-.8, .5), (.5, -1.9), (2.2, -2.8),
                        (3.6, -2), (3, -.1), (2, 1.8), (.9, 2.6)]))
    doc.path(geom_d(roots), fill=doc.lin("toupet-racine", HL(1, -2), HL(1, 3),
                                       [(0, "#8A5941"), (.6, "#764838"), (1, "#5B3526")]))
    wool(roots, 81, brown=True, step=.32)
    doc.pivot("head", POLL)
    doc.pivot("muzzle", HL(22.5, 5.45))
    doc.close()


def articulate(svg):
    root = ET.fromstring(svg)
    paints = A.Paints(root)
    for name, kind, _, _ in LEGS:
        joint, half = A.leg_joint(svg, name, S((0, 18 if kind == "front" else 24))[1])
        A.split_leg(root, paints, name, joint, half + .4)
    gum = A.gum([S(p) for p in HLs([(19.5, 4.8), (19.6, 4.1), (20.6, 3.8),
                                   (22.4, 3.7), (22.6, 4.1), (21.5, 4.6)])],
                [S(p) for p in HLs([(20, 4.5), (21, 4.3), (22.1, 4.15)])],
                "#BD8480", "#854E4B", .17)
    A.lift_part(root, paints, "head", [S(p) for p in LIP], "lip", S(HL(19.3, 4.2)), under=gum)
    ET.indent(root, space="  ")
    return ET.tostring(root, encoding="unicode") + "\n"


def main(path):
    doc.lin("robe", (0, 76), (0, 37), [(0, "#F1E7DA"), (.3, "#DCCDB7"),
                                       (.7, "#C8B8A2"), (1, "#A89580")])
    # L'anatomie arrière est cachée en partie par les postérieurs proches.
    for name, kind, dx, far in LEGS[:2]:
        leg(name, kind, dx, far)
    for name, kind, dx, far in LEGS[2:]:
        leg(name, kind, dx, far)
    neck()
    body()
    tail()
    head()
    doc.pivot("ground", (42, 0))
    svg = articulate(doc.render(143, 109,
        "  Awassi local : bélier de quatre ans, 75 cm au garrot, juillet après la tonte.\n"
        "  Profil droit ; une unité = un centimètre ; graines fixes.\n"
        "  Générateur tools/races/awassi/gen_awassi.py ; fiche docs/races/awassi.md.\n"
        "  © 2026 Alejandro San Martin Lamas — MIT OR Apache-2.0"))
    if VARIANTE:
        return variante(path, svg)
    Path(path).write_text(svg)
    print(f"{path} : {len(svg)/1024:.0f} Ko, {svg.count('<path')} tracés")


def variante(path, svg):
    """La brebis ou l'agneau : ce qui change du bélier, puis ses proportions."""
    root = ET.fromstring(svg)
    if VARIANTE == "brebis":
        # La grosse mamelle rose-brun de la brebis laitière dépasse sous la
        # toison, devant le postérieur (§ 1.5).
        variantes.mamelle(root, *S((25, 41.5)), largeur=12, hauteur=9.5, peau="#D9AC9B", ombre="#A87868")
    if VARIANTE == "agneau":
        # Né brun clair : la robe crème garde une teinte fauve (§ 1.6) ; la
        # queue, mince et laineuse, n'a pas encore son coussin de graisse.
        variantes.recolorer(root, variantes.teinte("#C79F78", .3), dans=("body", "neck", "tail"),
                    si=lambda c: variantes.clarte(c) > .55)
        variantes.queue_agneau(root, longueur=27, epaisseur=5.5, clair="#E3CDB0", sombre="#A98B6C")
    variantes.ecrire(path, root, **PROPORTIONS[VARIANTE])


if __name__ == "__main__":
    main(sys.argv[1] if len(sys.argv) > 1 else "pets/awassi/awassi.svg")
