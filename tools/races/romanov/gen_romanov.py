#!/usr/bin/env python3
"""Une brebis Romanov adulte de trois ans, en février, trois mois et demi
après la tonte : gris bleuté mat, tête noire sèche à petite liste blanche.

Dessin original d'après docs/races/romanov.md, de profil droit. Une unité
vaut un centimètre réel. Repère monde : x vers le nez, h vers le haut,
sol à h=0. Le module local outils.py reprend les outils de la Lacaune.
Les pattes sont recoupées au carpe/jarret ; la lèvre cache son bourrelet.
La toison est courte : aucune mèche pendante à articuler.

Usage : python3 tools/races/romanov/gen_romanov.py pets/romanov/romanov.svg
© 2026 Alejandro San Martin Lamas — MIT OR Apache-2.0
"""
import math
import os
import random
import sys
import xml.etree.ElementTree as ET

HERE = os.path.dirname(os.path.abspath(__file__))
sys.path.insert(0, os.path.dirname(HERE))
sys.path.insert(0, HERE)
import articulations as A
import variantes  # noqa: E402

# Les autres dessins de la race (tools/races/variantes.py), d'après la fiche.
# Sans variante : la brebis.
VARIANTE = sys.argv[2] if len(sys.argv) > 2 else ""
PROPORTIONS = {
    "belier": dict(  # Bélier Romanov : 70 cm au garrot, sans cornes, crinière de jarres noirs sur l'encolure et le poitrail, ossature plus lourde.
        garrot=(65, 70), pattes=(1.08, 1.0), encolure=(1.0, 1.2), tete=(1.05, 1.07)),
    "agneau": dict(  # Agneau Romanov de deux semaines : 37 cm au garrot, noir de la tête aux pieds, tout en pattes.
        variantes.AGNEAU, garrot=(65, 37), pattes=(1.15, 1.2), corps=(.8, .8)),
}
from outils import (Doc, Polygon, S, add, clamp, cr, dense, geom_d,
                    lstroke, mix, poisson, poly, qstroke, rot, runs_path)

doc = Doc()
GARROT = 65.0
CARPE, JARRET = 15.2, 21.2      # § 2.5 : 0,21–0,25 H ; 0,30–0,34 H
POLL = (87.0, 87.0)            # nuque haute, 22 cm au-dessus du garrot
ANGLE = math.radians(31.0)
NECK_PIVOT = (72.0, 51.0)      # assez bas : museau au sol au broutage


def HL(u, v):
    """Repère de la tête : u vers le nez, v vers la mâchoire."""
    return (POLL[0] + u * math.cos(ANGLE) - v * math.sin(ANGLE),
            POLL[1] - u * math.sin(ANGLE) - v * math.cos(ANGLE))


def HLS(pts):
    return [HL(*p) for p in pts]


def ellipse(c, rx, ry, a=0.0, n=24):
    return [add(c, rot((rx * math.cos(t * 2 * math.pi / n), ry * math.sin(t * 2 * math.pi / n)), a))
            for t in range(n)]


def shape(pts):
    return Polygon(dense(pts, True, 8)).buffer(0)


def glow(gid, c, rx, ry, color, alpha, a=0.0, clip=None):
    """Un modelé doux en dégradé, rogné géométriquement avant écriture."""
    g = doc.rad(gid, c, ry, [(0, color, alpha), (1, color, 0)], sx=rx / ry, angle=a)
    p = Polygon(ellipse(c, rx, ry, a, 40))
    if clip is not None:
        p = p.intersection(clip)
    doc.path(geom_d(p), fill=g, rule="evenodd")


# § 2 : tronc osseux 68 cm, laine 6 cm sur le dos ; poitrine à 34,5 cm.
# Le contour en laine mesure 80 cm ; la frange reste à 30–31 cm.
BACK = [(1, 57), (2.5, 63), (7, 67), (13, 69.2), (21, 70.4), (31, 71),
        (43, 71.2), (55, 71.2), (64, 71.2)]
WITHERS = BACK[-1]
# L'épaule et le bord caché du cou partagent un cercle autour du pivot.
# Quand le cou descend, leur recouvrement reste arrondi, même au sommeil :
# aucun bord vertical de la pièce fixe ne dépasse au-dessus de lui.
R_SHOULDER = math.dist(WITHERS,NECK_PIVOT)
A_SHOULDER = math.atan2(WITHERS[1]-NECK_PIVOT[1],WITHERS[0]-NECK_PIVOT[0])


def shoulder_arc(a0,a1):
    n=max(3,int(abs(a1-a0)/0.06))
    return [(NECK_PIVOT[0]+R_SHOULDER*math.cos(a0+(a1-a0)*i/n),
             NECK_PIVOT[1]+R_SHOULDER*math.sin(a0+(a1-a0)*i/n)) for i in range(n+1)]


SHOULDER = shoulder_arc(A_SHOULDER,math.radians(55))+[(83,61),(81,53),(80,46),(78,39),(74,34)]
BELLY = [(74, 34), (68, 32), (59, 31), (49, 30.8), (39, 31.2), (31, 32.5),
         (26, 35.2), (21, 38), (15, 39.8), (10, 41), (6, 43.8), (3, 48), (1, 53), (1, 57)]
BODY = BACK + SHOULDER[1:] + BELLY[1:-1]

# Le cou est une pièce entière, largement prolongée sous le corps et la
# tête. Son bord caché est arrondi pour ne pas créer d'angle au broutage.
CREST = [(85,89),(81,87.3),(77.5,84),(74,80),(70,77),(66.5,73.5),WITHERS]
HIDDEN = shoulder_arc(A_SHOULDER,math.radians(220))+[(61,36.5),(68,34.2),(73,34.5),(77.5,37),(79,40)]
THROAT = [(79,40),(80,45),(82,54),(85,61),(86.2,67),(88.3,72),(91,76),(91,81),(89,86),(85,89)]
# Même sens de parcours que le corps. Le moteur réunit les remplissages
# pour son masque d'éclairage en règle non nulle : un sens inverse dans le
# recouvrement créerait un trou de lumière, visible comme un croissant.
NECK = (CREST+HIDDEN[1:]+THROAT[1:-1])[::-1]


def wool_color(p):
    """Gris à peine chaud en surface ; creux bleutés, plages de jarres."""
    x, h = p
    t = clamp((h - 31) / 41)
    col = mix("#72736F", "#B4B4AE", t)
    patch = (0.40 * math.exp(-((x - 59) / 9) ** 2 - ((h - 51) / 10) ** 2)
             + 0.26 * math.exp(-((x - 31) / 10) ** 2 - ((h - 64) / 5) ** 2)
             + 0.16 * math.exp(-((x - 23) / 7) ** 2 - ((h - 44) / 5) ** 2))
    if x > 74:
        patch += 0.18 * clamp((h - 71) / 17)
    col = mix(col, "#4E555D", patch)
    # De petits amas irréguliers (quelques centimètres) portent davantage
    # de jarres ou de duvet ; ils ne suivent ni colonnes ni côtes répétées.
    mottling = (math.sin(x*0.81+math.sin(h*0.47))*math.sin(h*0.73+math.sin(x*0.36)))
    return mix(col, "#D2D2C8" if mottling>0 else "#4B5158", abs(mottling)*0.065)


def quant(col, step=10):
    """Partager les teintes des fibres, sans des milliers de tracés isolés."""
    return "#%02X%02X%02X" % tuple(min(255, round(int(col[i:i+2],16)/step)*step) for i in (1,3,5))


def wool(region, seed, spacing=0.74, dim=0.0):
    """Petites boucles 6–12 mm : duvet clair au-dessus de jarres sombres.

    Pas de spirales régulières ni mèches longues : les croissants se
    chevauchent et changent d'orientation, les contrastes restent mats.
    Les points de Poisson évitent toute trame visible. `dim` assombrit la
    laine du côté caché (la cuisse du fond).
    """
    r = random.Random(seed)
    pts = poisson(region.buffer(-0.30), spacing, r)
    pts.sort(key=lambda p: (-p[1], p[0]))
    doc.begin_batch()
    for p in pts:
        col = wool_color(p)
        if dim > 0:
            col = mix(col, "#2A2C30", dim)
        a = r.uniform(-math.pi, math.pi)
        w, h = r.uniform(0.30, 0.53), r.uniform(0.21, 0.40)
        def F(u, v):
            return add(p, rot((u, v), a))
        # Le creux noir n'est qu'une virgule fine entre les fibres claires.
        dark = quant(mix(col, "#303238", r.uniform(0.14, 0.30)))
        light = quant(mix(col, "#E0DFD7", r.uniform(0.12, 0.38)))
        # Croissants interrompus, de tailles différentes : jamais l'anneau
        # complet qui ferait une cotte de mailles plutôt qu'un duvet mat.
        doc.stroke(cr([F(-w, 0), F(-0.7*w, -h), F(0.15*w, -h)], False),
                   dark, 0.075, 0.62)
        ww=r.uniform(0.45,0.9)
        doc.stroke(cr([F(-ww*w, h*0.15), F(-0.25*w, h), F(0.5*w, h*0.7)], False),
                   light, 0.10 if r.random()<0.7 else 0.13, 0.72)
        if r.random() < 0.35:
            doc.stroke(qstroke(F(-w*0.3, -h*0.7), F(w*0.4, 0), F(0, h*0.6), 2),
                       quant(mix(col, "#D3D3CE", 0.25)), 0.055, 0.42)
        if r.random()<0.12:
            doc.stroke(qstroke(F(-0.2,-0.4),F(0.15,0),F(0.05,0.45),2),
                       "#CFD0CA",0.045,0.28)
    doc.flush()


def fringe(line, seed, dim=0.0):
    """Le bord grumeleux fin : boucles courtes, pas de festons de peluche."""
    r = random.Random(seed)
    boundary = dense(line, False, 10)
    for k in range(1, len(boundary)-1, 2):
        p = boundary[k]
        prev, nxt = boundary[k-1], boundary[k+1]
        dx, dy = nxt[0]-prev[0], nxt[1]-prev[1]
        length = math.hypot(dx, dy) or 1
        # Sur ce contour horaire en coordonnées monde, l'extérieur est à gauche.
        n = (-dy/length, dx/length)
        if r.random()<0.25:
            continue
        c = add(p, (-n[0]*0.20, -n[1]*0.20))
        w, h = r.uniform(0.28, 0.58), r.uniform(0.20, 0.37)
        a = math.atan2(dy, dx)
        pts = ellipse(c, w, h, a, 12)
        col = mix(wool_color(p), "#C6C5BD", 0.12)
        if dim > 0:
            col = mix(col, "#2A2C30", dim)
        doc.path(cr(pts[::-1]), fill=col)


def gradients():
    doc.lin("toison", (0, 74), (0, 30), [(0, "#A9ABA8"), (0.30, "#9A9B96"),
                                             (0.70, "#858782"), (1, "#656963")])
    doc.lin("tete", HL(7,-3), HL(10,10), [(0, "#3B3E46"), (0.30, "#292C32"),
                                          (0.72, "#202226"), (1, "#121416")])


def neck():
    doc.open("neck")
    region = shape(NECK)
    doc.path(cr(NECK), fill="url(#toison)")
    # § 4.1 : le gris passe au noir de la tête par 2 à 4 cm de gris foncé,
    # en arrière des joues et sur le haut de la nuque.
    glow("cou-ombre", (85.5, 82), 5, 7, "#282F3B", 0.40, clip=region)
    wool(region, 1810)
    fringe(CREST, 1811)
    doc.pivot("neck", NECK_PIVOT)
    doc.close()


def body():
    doc.open("body")
    region = shape(BODY)
    doc.path(cr(BODY), fill="url(#toison)")
    glow("flanc-rond", (32,58), 28, 13, "#CFCEC5", 0.20, clip=region)
    glow("epaule-jarres", (60,52), 10, 13, "#353B43", 0.25, a=-0.25, clip=region)
    glow("dos-jarres", (31,64), 11, 6, "#414750", 0.18, a=-0.25, clip=region)
    glow("flanc-jarres", (23,44), 8, 6, "#42464B", 0.12, a=-0.25, clip=region)
    for i,(x,h,rx,ry) in enumerate(((54,51,3.5,4),(59,58,3,3.8),(66,50,3,4.2),
                                   (32,62,4.5,2.8),(26,42,3.2,2.8),(18,47,2.8,4.2))):
        glow(f"amas-jarres-{i}",(x,h),rx,ry,"#424B54",0.10,a=-0.35,clip=region)
    glow("ventre-creux", (44,33), 31, 6, "#272C31", 0.16, clip=region)
    wool(region, 1802)
    fringe(BACK, 1803)
    fringe(BELLY, 1804)
    doc.pivot("body", (43, 34))
    doc.close()


LEGS = [("leg-back-far", "back", 20, True), ("leg-front-far", "front", 61, True),
        ("leg-back-near", "back", 13, False), ("leg-front-near", "front", 69, False)]


def culotte(name, x, far):
    """§ 2.5 : derrière, la laine descend jusqu'au-dessus du jarret (le bas
    de la cuisse) ; le poil ras noir ne commence qu'à 3–4 cm de sa pointe.

    La laine de la cuisse appartient au haut de la patte : elle la suit au
    pas et quand elle se replie. Son bord arrière prolonge la fesse ; son
    haut reste caché sous le corps, quel que soit l'angle de la hanche.
    """
    front = [(x+6.0,54),(x+7.6,47),(x+7.4,42),(x+6.1,37),(x+4.3,32.5),
             (x+2.5,28.6),(x+1.1,26.0)]
    bottom = [(x+1.1,26.0),(x+0.3,25.0),(x-0.9,24.5),(x-2.3,24.6),
              (x-3.5,25.1),(x-4.2,25.9)]
    back = [(x-4.2,25.9),(x-4.9,28.5),(x-5.4,32),(x-6.0,36.5),(x-6.6,41),
            (x-7.0,44.5),(x-7.4,48),(x-6.0,54)]
    pts = front+bottom[1:]+back[1:-1]
    reg = shape(pts)
    dim = 0.30 if far else 0.0
    top, low = ("#666762", "#4F504B") if far else ("#7E7F7A", "#676863")
    doc.path(cr(pts), fill=doc.lin(name+"-culotte", (x,46), (x,25), [(0,top),(1,low)]))
    glow(name+"-culotte-ombre", (x,40.5), 7.5, 3.4, "#272C31", 0.28 if far else 0.2, clip=reg)
    if not far:
        glow(name+"-culotte-lumiere", (x+1.6,33), 3.2, 4.8, "#C9C9C1", 0.12, a=-0.3, clip=reg)
    wool(reg, 1830+x, dim=dim)
    # Le bord grumeleux du bas, de l'avant vers l'arrière (dehors à gauche).
    fringe(bottom, 1850+x, dim=dim)


def leg(name, kind, x, far):
    """Poil ras noir ; canons de 3 cm, paturons courts, deux onglons."""
    front = kind == "front"
    if front:
        outline = [(x-3.5,49),(x+3.6,49),(x+2.6,37),(x+1.8,27),
                   (x+1.8,17),(x+1.6,14),(x+1.1,8),(x+1.7,4),
                   (x+2.1,2),(x+2.1,0.2),(x-1.6,0.2),(x-1.8,1.8),
                   (x-1.6,4.5),(x-1.9,9),(x-1.7,15),(x-2,19),
                   (x-2.3,28),(x-3.2,38)]
        pivot = (x,47)
        axis = [(x,37),(x+0.2,24),(x-0.1,15.2),(x-0.4,7)]
    else:
        outline = [(x-5.5,55),(x+5,53),(x+7,43),(x+5.5,36),(x+2.2,30),
                   (x-0.5,24),(x-0.3,20.8),(x+0.6,15),(x+0.7,8),
                   (x+1.4,4),(x+2.1,2),(x+2.1,0.2),(x-1.6,0.2),
                   (x-2.4,2),(x-1.7,5),(x-2.1,11),(x-2.5,18),
                   (x-3.5,22),(x-2.5,27),(x+0.1,35),(x-3.5,43)]
        pivot = (x+0.5,53)
        axis = [(x+2.7,37),(x+0.4,29),(x-1.4,21.5),(x-0.8,8)]
    col = "#151719" if far else "#252626"
    light = "#2C2F31" if far else "#4C4E4B"
    doc.open(name)
    g = doc.lin(name+"-poil", (x-3,25),(x+2.8,25),[(0,"#101214"),(0.4,col),(0.72,light),(1,col)])
    doc.path(cr(outline), fill=g)
    reg = shape(outline)
    glow(name+"-tendon", (x+0.2,12), 0.48, 6, light, 0.4, clip=reg)
    doc.path(cr(axis,False), stroke=light, sw=0.22, so=0.45, cap="round")
    # Le relief du genou / jarret se lit sans anneau de contour artificiel.
    jh = CARPE if front else JARRET
    jc = (x+0.3,jh+0.4) if front else (x-1.5,jh+0.2)
    glow(name+"-joint", jc, 1.25, 1.5, light, 0.38, clip=reg)
    # Quelques poils couchés, très fins, suivant les tendons.
    r = random.Random(x*11)
    doc.begin_batch()
    for p in poisson(reg.buffer(-0.35), 0.85, r):
        if p[1] < 5:
            continue
        end = add(p,(r.uniform(-0.1,0.15),-r.uniform(0.35,0.72)))
        doc.stroke(lstroke(p,end,2), light, 0.055, 0.4)
    doc.flush()
    if not front:
        culotte(name, x, far)
    hoof = [(x-1.6,3.3),(x+1.1,3.35),(x+1.4,2.5),(x+2.3,0.9),
            (x+2.2,0),(x-1.8,0),(x-1.9,1.2)]
    doc.path(runs_path([hoof[:4],hoof[3:5],hoof[4:6],hoof[5:]+hoof[:1]]),
             fill=doc.lin(name+"-sabot",(x,3.3),(x,0),[(0,"#383835"),(0.65,"#242422"),(1,"#161715")]))
    doc.path(cr([(x+0.8,2.9),(x+1.15,1.5),(x+1.1,0.15)],False), stroke="#101110",sw=0.16,cap="round")
    doc.path(cr([(x-1.6,0.2),(x+0.3,0.15),(x+2.1,0.2)],False),stroke="#77736B",sw=0.10,so=0.6)
    # L'ergot, derrière le paturon, ne touche pas le sol.
    doc.path(poly([(x-1.3,5.9),(x-2.25,5.25),(x-2.1,4.55),(x-1.35,4.95)]),fill="#1E201E")
    doc.pivot(name,pivot)
    doc.close()


def tail():
    """Queue naturelle 11 cm, sombre, fine, laineuse, très au-dessus du jarret."""
    doc.open("tail")
    pts = [(2.2,62),(-0.2,61),(-1,57),(-1.1,53),(-0.4,50.4),
           (0.8,50),(2,52.1),(2.8,56),(3.5,59.7)]
    reg = shape(pts)
    doc.path(cr(pts),fill=doc.lin("queue-laine",(-2,60),(2,51),[(0,"#4C4D48"),(0.5,"#454642"),(1,"#2F3232")]))
    r=random.Random(38)
    doc.begin_batch()
    for p in poisson(reg.buffer(-0.15),0.42,r):
        doc.stroke(qstroke(add(p,(-0.25,0.2)),add(p,(0.15,0.1)),add(p,(0.2,-0.3)),2),
                   "#777871",0.085,0.6)
    doc.flush()
    doc.pivot("tail",(2,61))
    doc.close()


# Tête de 23,5 cm : chanfrein doucement convexe, museau fin, ganache sèche.
# § 5 : front un peu bombé, puis un chanfrein convexe sur toute sa
# longueur, sans creux devant l'œil ; la bosse culmine entre l'œil et le nez.
DORSAL = [(-1.2,0),(-0.7,-2),(1.4,-3.2),(4.5,-3.7),(7.1,-3.72),
          (10,-3.78),(12.7,-3.84),(15.3,-3.76),(18,-2.7),(20.7,-0.8),
          (22.4,0.6),(23.3,2.1),(23.5,3.6),(22.9,4.8)]
MOUTH = [(22.9,4.8),(21.6,5.1),(19.9,5.3),(18.2,5.5)]
NOTCH = [(18.2,5.5),(17,6.4),(14.9,6.9)]
UNDER = [(14.9,6.9),(13,8.8),(10.1,9.7),(6.7,9.4),(3.2,8.2),
         (0.3,5.3),(-1,2.5),(-1.2,0)]
HEAD = HLS(DORSAL+MOUTH[1:]+NOTCH[1:]+UNDER[1:-1])
HEAD_REG = shape(HEAD)
EYE = HL(8.8,0.4)             # § 5 : environ 40 % depuis la nuque
ALMOND = ellipse(EYE,1.62,1.15,-0.12,24)
LIP_POLY = HLS([(19.1,3.6),(21.1,2.9),(23.9,2.9),(24.2,5.0),
                (22.7,5.45),(20.8,5.5),(18.8,5.7)])


def ear(gid, base, angle, far=False):
    """Oreille en lancette, 10,5 cm, noire dedans aussi, sans étiquette."""
    a=math.radians(angle)
    def F(u,v):
        return add(base,rot((u,v),a))
    doc.open(gid)
    pts=[F(0,0),F(2,1.4),F(5,2.3),F(8.1,1.6),F(10.5,0.3),
         F(9,-0.65),F(6.4,-1.5),F(3.6,-1.4),F(1,-0.65)]
    g=doc.lin(gid+"-peau",F(5,2),F(5,-1.6),[(0,"#44484B"),(0.4,"#292C2D"),(1,"#111416")])
    doc.path(cr(pts),fill=g)
    interior=[F(1.8,0.1),F(4.5,1.45),F(7.6,1.1),F(9.5,0.35),
              F(7.1,-0.45),F(4.5,-0.75),F(2.1,-0.2)]
    doc.path(cr(interior),fill="#171B1E" if far else "#252529")
    doc.path(cr([F(1.7,0.2),F(4.7,0.7),F(8.7,0.4)],False),stroke="#56595A",sw=0.15,so=0.6)
    r=random.Random(129 if far else 130)
    doc.begin_batch()
    for _ in range(60):
        u=r.uniform(2,9)
        v=r.uniform(-0.65,1.2)
        doc.stroke(lstroke(F(u,v),F(u-0.5,v*0.75),2),"#8B8C85",0.047,0.3)
    doc.flush()
    doc.pivot(gid,base)
    doc.close()


def jaw():
    doc.open("jaw")
    doc.path(cr(HLS([(14,5.2),(17,4.2),(20,3.9),(22.6,4.2),
                     (22.7,5.4),(20,6),(16,7.5),(13,8)])),fill="#321F24")
    doc.path(cr(HLS([(17.8,4.8),(20.6,4.2),(22,4.4),(21,5.2),(18.8,5.5)])),fill="#9B6E77")
    doc.path(cr(HLS([(21,4.4),(22.1,4.4),(22.3,4.8),(21.1,5)])),fill="#D8D1C3")
    chin=HLS([(17.8,5.5),(20.5,5.25),(22.9,4.9),(23,5.8),(21.7,7.1),
              (19.3,8.1),(16.2,8.8),(13.4,9.2),(12.9,8),(14.8,6.7)])
    doc.path(cr(chin),fill=doc.lin("menton",HL(19,5),HL(19,9),[(0,"#34383D"),(1,"#14171B")]))
    doc.path(cr(HLS([(18,5.6),(20.5,5.3),(22.7,5.0)]),False),stroke="#5C5E5E",sw=0.18,so=0.65)
    doc.pivot("jaw",HL(14.9,6.9))
    doc.close()


def eyes():
    # Une paupière fermée subsiste sous le groupe que le moteur écrase.
    doc.path(cr(ALMOND),fill="#2D3032")
    doc.path(cr([add(EYE,(-1.5,0)),add(EYE,(0,-0.16)),add(EYE,(1.5,-0.1))],False),
             stroke="#151719",sw=0.2,cap="round")
    doc.open("eyes")
    doc.path(cr(ALMOND),fill="#3B3025")
    doc.open("pupils")
    ic=add(EYE,(-0.2,0))
    iris=doc.rad("iris",ic,1.18,[(0,"#382A1C"),(0.35,"#A07B4D"),(0.75,"#715333"),(1,"#2E241B")])
    doc.path(cr(ellipse(ic,1.15,1.1)),fill=iris)
    doc.begin_batch()
    for i in range(30):
        a=i*2*math.pi/30
        doc.stroke(lstroke(add(ic,(0.65*math.cos(a),0.65*math.sin(a))),
                           add(ic,(1.02*math.cos(a+0.05),1.02*math.sin(a+0.05))),2),
                   "#C1A172" if i%2 else "#42331F",0.047,0.5)
    doc.flush()
    # Fente horizontale dans le monde ; le moteur compense ensuite le regard.
    doc.path(cr([add(ic,(-0.83,0)),add(ic,(-0.70,0.20)),add(ic,(0.66,0.20)),
                 add(ic,(0.84,0)),add(ic,(0.66,-0.20)),add(ic,(-0.70,-0.20))]),fill="#0B0D0E")
    doc.pivot("pupils",ic)
    doc.close()
    doc.path(cr(ellipse(add(EYE,(-0.5,0.38)),0.22,0.15)),fill="#E9EBE7",fo=0.9)
    doc.path(cr(ellipse(add(EYE,(0.6,-0.25)),0.08,0.06)),fill="#D5DAD7",fo=0.65)
    doc.pivot("eyes",EYE)
    doc.close()
    # La garde fixe cache l'iris quand le moteur le décale.
    ring=Polygon(ellipse(EYE,3.3,2.7)).intersection(HEAD_REG).difference(shape(ALMOND))
    doc.path(geom_d(ring),fill="url(#tete)",rule="evenodd")
    doc.path(cr(ALMOND),stroke="#737572",sw=0.21,so=0.85)
    doc.path(cr([add(EYE,(-1.45,0.48)),add(EYE,(-0.6,1.12)),
                 add(EYE,(0.45,1.16)),add(EYE,(1.25,0.65))],False),
             stroke="#9A9C94",sw=0.20,so=0.55,cap="round")
    doc.path(cr([add(EYE,(-1.2,-0.72)),add(EYE,(-0.2,-1.2)),add(EYE,(0.9,-0.92))],False),
             stroke="#92968E",sw=0.15,so=0.5,cap="round")
    top=ALMOND[:13]
    doc.path(cr(top,False),stroke="#131619",sw=0.24,cap="round")


def head():
    doc.open("head")
    ear("ear-far",HL(1,-2),26,True)
    jaw()
    doc.path(runs_path([HLS(DORSAL),HLS(MOUTH),HLS(NOTCH),HLS(UNDER)]),fill="url(#tete)")
    glow("chanfrein-lumiere",HL(13,-1.8),7.8,1.45,"#626B79",0.32,a=-ANGLE,clip=HEAD_REG)
    glow("joue-relief",HL(8.8,5.4),4.3,2.7,"#64686A",0.23,a=-ANGLE,clip=HEAD_REG)
    glow("ganache-ombre",HL(5.2,7),4,1.7,"#080C12",0.28,a=-ANGLE,clip=HEAD_REG)
    # Le poil est ras, orienté vers le museau ; pas de laine au front.
    r=random.Random(1812)
    doc.begin_batch()
    keep=shape(ALMOND).buffer(0.6)
    for p in poisson(HEAD_REG.buffer(-0.18).difference(keep),0.35,r):
        d=rot((r.uniform(0.18,0.4),r.uniform(-0.06,0.06)),-ANGLE)
        doc.stroke(lstroke(p,add(p,d),2),"#78818A" if r.random()<0.45 else "#080C10",0.04,0.16)
    doc.flush()
    # Liste frontale courte, bords nets : arrêt 2 cm avant la hauteur de
    # l'œil (§ 4.2) ; sur la ligne du front, de profil.
    blaze=HLS([(0.4,-2.6),(1.5,-3.12),(2.3,-3.36),(3.5,-3.55),
               (4.4,-3.66),(5.3,-3.66),(6.2,-3.62),(6.9,-3.4),
               (6.35,-3.1),(5.5,-2.7),(4.6,-2.3),(3.4,-2.15),
               (2.9,-1.95),(2.1,-2.08),(1.4,-1.85),(0.6,-2)])
    doc.path(poly(blaze),fill=doc.lin("liste",HL(2,-3),HL(5,-1.8),[(0,"#F6F7F3"),(1,"#C6CCCE")]))
    doc.begin_batch()
    for p in poisson(shape(blaze).buffer(-0.1),0.23,r):
        doc.stroke(lstroke(p,add(p,rot((0.3,0.01),-ANGLE)),2),"#FFFFFF",0.045,0.8)
    doc.flush()
    # Peau nue du mufle, naseau en virgule, philtrum et commissure.
    nose=HLS([(21.1,-0.35),(22.5,0.7),(23.3,2.1),(23.5,3.6),
              (22.9,4.8),(21.7,4.5),(20.8,3),(20.6,1)])
    doc.path(cr(nose),fill=doc.lin("nez",HL(21.7,0),HL(22.4,4.7),[(0,"#454746"),(0.6,"#282A29"),(1,"#181B1C")]))
    doc.path(cr(HLS([(21.9,0.9),(22.65,1.5),(22.6,2.4),(21.8,2.8),
                     (21.25,2.55),(21.7,2.0),(21.5,1.4)])),fill="#090D0E")
    doc.path(cr(HLS([(21.8,1.05),(22.4,1.45),(22.5,1.8)]),False),stroke="#7F8583",sw=0.11,so=0.65)
    doc.path(cr(HLS([(23.1,3.2),(23.0,4),(22.65,4.65)]),False),stroke="#0B0F10",sw=0.16)
    doc.path(cr(HLS(MOUTH),False),stroke="#070B0D",sw=0.16,cap="round")
    eyes()
    # L'oreille visible part de la nuque et pointe vers l'arrière-haut.
    ear("ear-near",HL(2,0.6),154)
    doc.pivot("head",POLL)
    doc.pivot("muzzle",HL(23,5.4))
    doc.close()


def gum():
    Q=A.Q
    p=HLS([(19,4.4),(20.8,4),(22.1,4),(22.4,4.45),(21.7,4.8),(19.4,5.1)])
    return [ET.Element(Q+"path",{"d":cr(p),"fill":"#8B616C"}),
            ET.Element(Q+"path",{"d":cr(HLS([(19.5,4.6),(20.8,4.3),(21.9,4.3)]),False),
                                     "fill":"none","stroke":"#C29A9C","stroke-width":".13"})]


def articulate(svg):
    parser=ET.XMLParser(target=ET.TreeBuilder(insert_comments=True))
    root=ET.fromstring(svg,parser=parser)
    paints=A.Paints(root)
    for name,kind,x,far in LEGS:
        h=CARPE if kind=="front" else JARRET
        joint,half=A.leg_joint(svg,name,S((0,h))[1])
        A.split_leg(root,paints,name,joint,half+0.4)
    A.lift_part(root,paints,"head",[S(p) for p in LIP_POLY],"lip",S(HL(19.1,4.3)),under=gum())
    ET.indent(root,space="  ")
    return ET.tostring(root,encoding="unicode")+"\n"


def main(path):
    gradients()
    for args in LEGS:
        leg(*args)
    tail()
    neck()
    body()
    head()
    doc.pivot("ground",(41,0))
    svg=doc.render(123,104,
        "  Romanov : brebis de trois ans, début février, 3,5 mois après la tonte.\n"
        "  65 cm au garrot ; 1 unité = 1 cm. Toison courte grise, tête noire sans\n"
        "  toupet ni corne, liste frontale courte, oreilles en lancette, queue de 11 cm.\n"
        "  Dessin original d'après docs/races/romanov.md, écrit par\n"
        "  tools/races/romanov/gen_romanov.py. © 2026 Alejandro San Martin Lamas\n"
        "  MIT OR Apache-2.0")
    svg=articulate(svg)
    if VARIANTE:
        return variante(path, svg)
    with open(path,"w",encoding="utf-8") as f:
        f.write(svg)
    print(f"{path} : {len(svg.encode())/1024:.0f} Ko, {svg.count('<path')} tracés")


def variante(path, svg):
    """Les autres dessins : ce qui change, puis les proportions."""
    root = ET.fromstring(svg)
    if VARIANTE == "belier":
        # La crinière noire, nettement séparée de la toison (§ 1.4).
        variantes.recolorer(root, variantes.rampe("#08090A", "#34383C", bas=.05, haut=.8), dans=("neck",))
        variantes.bourses(root, *variantes.sous_le_ventre(root, 6, -3), largeur=8.5, hauteur=13,
                  peau="#3A3D40", ombre="#141618")
    if VARIANTE == "agneau":
        # Noir profond à la naissance : il ne pâlit qu'après quelques semaines.
        variantes.recolorer(root, variantes.rampe("#0A0C0E", "#33363A", bas=.05, haut=.85),
                    dans=("body", "neck", "tail", "leg-back-near", "leg-back-far",
                          "leg-front-near", "leg-front-far"))
    variantes.ecrire(path, root, **PROPORTIONS[VARIANTE])


if __name__=="__main__":
    main(sys.argv[1] if len(sys.argv)>1 else "romanov.svg")
