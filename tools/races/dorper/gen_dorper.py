#!/usr/bin/env python3
"""Le Dorper : bélier adulte de 105 kg, après sa mue de fin d'été.

Dessin original d'après docs/races/dorper.md et les profils du Kalahari et
les béliers de concours cités dans le README. Le repère est en centimètres,
x vers la droite, h vers le haut ; le garrot est à 70 cm. Aucun élément
photographique n'entre dans le dessin. La graine fixe ne sert qu'au grain
fin du poil. La silhouette et chaque détail anatomique sont construits.

L'encolure est une pièce complète sous le corps ; la tête est indépendante
et la suit. Les pattes sont découpées par articulations.py ; la lèvre
supérieure, son bourrelet et la mâchoire sont dessinés séparément.
Pas de mèches mobiles : le sujet a fini sa mue, et porte un poil ras.

Usage : python3 tools/races/dorper/gen_dorper.py pets/dorper/dorper.svg
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

# Les autres dessins de la race (tools/races/variantes.py), d'après la fiche.
# Sans variante : le bélier.
VARIANTE = sys.argv[2] if len(sys.argv) > 2 else ""
PROPORTIONS = {
    "brebis": dict(  # Brebis Dorper : 63 cm au garrot, sans moignon de corne, tête plus fine au profil droit, mamelle pigmentée.
        garrot=(70, 63), encolure=(1.0, .88), tete=(.97, .93)),
    "agneau": dict(  # Agneau Dorper de deux semaines : 38 cm au garrot, tête noire ronde et courte, grandes oreilles, queue entière.
        variantes.AGNEAU, garrot=(70, 38), tete=(1.0, 1.18), oreilles=1.2),
}
from outils import (Doc, Polygon, S, add, cr, dense, geom_d, lerp,  # noqa: E402
                    lstroke, mul, poisson, poly, qstroke, region,
                    rot, runs_path, sub)
from shapely.geometry import LineString, Point  # noqa: E402

GARROT, STERNUM, VENTRE = 70.0, 31.0, 24.0
CARPE, JARRET = 19.5, 21.5
POLL = (112.0, 88.0)
NECK_PIVOT = (80.0, 48.0)
# Le repère u penche de 22° : le contour convexe du front au nez ajoute
# environ 18°, donnant le chanfrein à 40° de la photo du Kalahari.
HEAD_ANGLE = math.radians(22)
HU, HV = (math.cos(HEAD_ANGLE), -math.sin(HEAD_ANGLE)), (-math.sin(HEAD_ANGLE), -math.cos(HEAD_ANGLE))
rng = random.Random(1950)  # fondation de la société de race, fiche § 10
D = Doc()


def HL(u, v):
    """Repère de la tête : u vers le mufle, v vers la ganache."""
    return add(POLL, add(mul(HU, u), mul(HV, v)))


def HH(ps):
    return [HL(*p) for p in ps]


# La fesse est x=0, le poitrail x=98 ; la profondeur du coffre est de
# 39 cm, celle du ventre de 46 cm. Dos long, droit ; pas de toison ajoutée.
BACK = [(0.4, 57), (0.5, 63), (2.5, 67), (7, 69.3), (16, 70.3), (28, 70.6),
        (41, 70.5), (54, 70.2), (67, 69.9), (78, 70)]
# Une épaule arrondie puis une tangente à -43° ; le bord caché du cou
# vient exactement sous cette tangente quand l'animal broute (Lacaune).
# Rotation de l'encolure relativement au corps, mesurée dans le moteur
# pour graze=1 : 83,922° - 2,865° de bascule du coffre.
GRAZE_TURN = math.radians(81.05737)
SHOULDER_SLOPE = -43.0


def arc(start, h0, h1, radius, left):
    sign = 1 if left else -1
    a = math.radians(h0 + 90 * sign)
    center = add(start, (radius*math.cos(a), radius*math.sin(a)))
    n = max(2, int(abs(h1-h0)/3)+1)
    return [add(center, (radius*math.cos(math.radians(h0+(h1-h0)*k/n-90*sign)),
                         radius*math.sin(math.radians(h0+(h1-h0)*k/n-90*sign)))) for k in range(n+1)]


def ray(start, angle, length):
    n = max(1, int(abs(length))+1)
    a = math.radians(angle)
    return [add(start, (length*math.cos(a)*k/n, length*math.sin(a)*k/n)) for k in range(1,n+1)]


def neck_turn(p, angle):
    v=sub(p,NECK_PIVOT)
    return add(NECK_PIVOT,(v[0]*math.cos(angle)+v[1]*math.sin(angle),
                          -v[0]*math.sin(angle)+v[1]*math.cos(angle)))


shoulder_arc = arc((78,70), 0, SHOULDER_SLOPE, 15, False)
SHOULDER_TANGENT = shoulder_arc[-1]
SHOULDER = shoulder_arc + ray(SHOULDER_TANGENT, SHOULDER_SLOPE, 11)
SHOULDER += [(97.5, 52), (98.2, 47), (97.7,42), (95,36), (90.5,32.7), (86,STERNUM)]

BELLY = [(86, STERNUM), (77, 28.7), (65, 25.2), (53, VENTRE), (43, 24.5), (33, 26),
         (26, 28.5), (22, 31.7), (18, 33), (12, 34.9), (7.1, 38.3), (3.8, 43.7), (1.2, 50), (0.4, 57)]
BODY = BACK + SHOULDER[1:] + BELLY[1:-1]
BODY_POLY = region(BODY)
# Le cou garde une réserve arrondie, derrière l'épaule au repos, afin de
# recouvrir les articulations quand il descend. Le noir s'arrête avant
# l'épaule : crête aux deux tiers depuis la nuque, bas plus en avant.
CREST = [(78,70), (84,72.9), (90,76), (97,80.2), (104,84.5), (111.5,88.6)]


def hidden_neck_top():
    v = sub((78,70), NECK_PIVOT)
    h0 = math.degrees(math.atan2(v[0], -v[1])) % 360
    h1 = SHOULDER_SLOPE + 180 + math.degrees(GRAZE_TURN)
    a=math.radians(SHOULDER_SLOPE)
    normal=(-math.sin(a),math.cos(a))
    target=sum(p*q for p,q in zip(SHOULDER_TANGENT,normal))-.05
    lo,hi=.5,40
    for _ in range(60):
        radius=(lo+hi)/2
        q=neck_turn(arc((78,70),h0,h1,radius,True)[-1],GRAZE_TURN)
        if sum(p*q for p,q in zip(q,normal))>target:hi=radius
        else:lo=radius
    # Une réserve inadéquate ne doit pas passer silencieusement : elle
    # ferait une bosse quand le cou tourne. Le raccord doit être tangent.
    q=neck_turn(arc((78,70),h0,h1,radius,True)[-1],GRAZE_TURN)
    assert abs(sum(p*q for p,q in zip(q,normal))-target)<1e-6, "Raccord d'encolure hors de la tangente"
    aa=arc((78,70),h0,h1,radius,True)
    q=neck_turn(aa[-1],GRAZE_TURN)
    length=(q[0]-SHOULDER_TANGENT[0])*math.cos(a)+(q[1]-SHOULDER_TANGENT[1])*math.sin(a)-.6
    top=aa+ray(aa[-1],h1,length)
    return top+arc(top[-1],h1,h1+85,2.5,True)[1:]


# Le contour va du haut de la nuque vers le garrot puis sous l'épaule.
# Aucun coin caché ne peut dépasser en bosse lorsque le cou descend.
NECK_HIDDEN = hidden_neck_top()
NECK = list(reversed(CREST)) + NECK_HIDDEN[1:] + [(75,42),(83,36.5),(90,36),(97,41),(102.2,49.3),
            (105,57),(110,65),(115,70),(120,73),(124,78),(120,85),(115,88)]

NECK_POLY = region(NECK)
BLACK_LIMIT = [(93.8, 78.6), (95, 75.3), (96.2, 71), (97.4, 66), (98.9, 60), (100.5, 54), (102.2, 49.3)]
BLACK_POLY = NECK_POLY.intersection(Polygon(BLACK_LIMIT + [(160, 30), (160, 110), (93.8, 110)]))

# Un triangle long de 24 cm, ganache profonde, front convexe sans creux.
DORSAL = [(-2, 0), (-1.9, -1.5), (-0.5, -3), (2, -3.8), (5, -4.15), (8.5, -4.05),
          (12, -3.55), (15.5, -2.7), (18.8, -1.35), (21, 0.0), (22.6, 1.3), (23.6, 2.7), (23.9, 4.0), (23.7, 5.0)]
MOUTH = [(23.7, 5.0), (22.4, 5.3), (20.7, 5.65), (18.7, 6.0), (17.5, 6.1)]
NOTCH = [(17.5, 6.1), (16.8, 7.4), (15.8, 11.3)]
UNDER = [(15.8, 11.3), (13, 12.4), (9.6, 13.0), (6.5, 12.5), (3.7, 11.0), (1.4, 9.0), (-0.2, 6.0),
         (-1.3, 3.5), (-2, 0)]
HEAD_POLY = region(HH(DORSAL + MOUTH[1:] + NOTCH[1:] + UNDER[1:-1]))
EYE = HL(8.5, 0.6)
LIP_PIVOT = HL(20, 4.15)
MUZZLE = HL(23.45, 5.9)


def ellipse(c, rx, ry, angle=0, n=24):
    return [add(c, rot((rx * math.cos(i * math.tau / n), ry * math.sin(i * math.tau / n)), angle)) for i in range(n)]


def glow(gid, c, rx, ry, color, alpha, clip, angle=0):
    """Modelé doux, rogné au calcul ; jamais de masque SVG."""
    shape = region(ellipse(c, rx, ry, angle))
    geom = shape.intersection(clip)
    if geom.is_empty:
        return
    grad = D.rad(gid, c, ry, [(0, color, alpha), (.42, color, alpha * .75), (1, color, 0)], sx=rx / ry, angle=angle)
    D.path(geom_d(geom), fill=grad)


def grain(area, spacing, flow, colors, lengths, alpha=.35, seed=None):
    """Jarre courte : fines marques courbes suivant les muscles, pas de boucles."""
    rr = random.Random(seed) if seed is not None else rng
    D.begin_batch()
    for p in poisson(area.buffer(-.16), spacing, rr, limit=7000):
        angle = flow(p) + rr.uniform(-.2, .2)
        le = rr.uniform(*lengths)
        v = (math.cos(angle), math.sin(angle))
        e = add(p, mul(v, le))
        if not area.contains(Point(e)):
            continue
        c = add(lerp(p, e, .5), mul((-v[1], v[0]), rr.uniform(-.08, .08)))
        D.stroke(qstroke(p, c, e, 2), colors[rr.randrange(len(colors))], .065, alpha)
    D.flush()


def line(ps, color, width, alpha=1):
    D.path(cr(ps, closed=False), stroke=color, sw=width, so=alpha, cap="round")


def line_in(ps, color, width, alpha, area):
    """Rogne un pli au calcul, y compris son épaisseur : aucun trait ne
    dépasse du poil court quand l'encolure tourne."""
    clipped=LineString(dense(ps,False,per=10)).intersection(area.buffer(-width*.55))
    for run in getattr(clipped,"geoms",[clipped]):
        if run.is_empty or run.geom_type!="LineString":
            continue
        D.path(poly(list(run.coords),False),stroke=color,sw=width,so=alpha,cap="round")


def gradients():
    # Une lumière neutre : on ne reprend pas les reflets rouges du sable.
    D.lin("robe", (0, 72), (0, 23), [(0, "#F7F0E4"), (.28, "#F1E9DC"), (.55, "#EDE3D2"),
                                      (.80, "#D4CCBF"), (1, "#A9A196")])
    D.lin("noir", (0, 89), (0, 47), [(0, "#332C29"), (.32, "#28221F"), (.65, "#221C19"), (1, "#100D0D")])
    D.lin("tete", HL(8, -4), HL(8, 10), [(0, "#3A302B"), (.28, "#2B2522"), (.63, "#221C19"), (1, "#100D0D")])
    D.lin("scur", (109, 94), (111, 89), [(0, "#9A825E"), (.4, "#7F6644"), (1, "#574330")])


def body():
    D.open("body")
    if not VARIANTE:
        scrotum()
    D.path(cr(BODY), fill="url(#robe)")
    # Les reliefs sont larges et légers ; pas de trait artificiel au milieu
    # du flanc : grand dorsal, côte, épaule, culotte et pli de l'aine.
    glow("flanc-volume", (49, 54), 35, 15, "#FFFDF5", .31, BODY_POLY)
    glow("epaule-volume", (83, 51), 8, 17, "#FFF9EB", .32, BODY_POLY, -.18)
    glow("epaule-creux", (77, 44), 3.2, 15, "#8F8473", .20, BODY_POLY, -.20)
    glow("epaule-avant", (87.2, 43.5), 5.5, 9, "#FFF7E9", .29, BODY_POLY, .25)
    glow("gigot-lumiere", (12, 51), 10, 16, "#FFF9EF", .28, BODY_POLY, -.15)
    glow("gigot-pli", (22, 39), 3.3, 12, "#968776", .26, BODY_POLY, -.55)
    glow("rein", (22, 65), 19, 3.5, "#FFFDF5", .19, BODY_POLY)
    # La peau rasée du bas du ventre reste pigmentée sous un poil fin.
    glow("aine", (28, 28.5), 10, 3.2, "#8D8073", .25, BODY_POLY)
    for k in range(4):
        ps = [(20+k*.7, 37-k*1.8), (23+k*.8, 33.5-k*1.4), (28+k, 31.5-k*1.0)]
        line(ps, "#A79B8A", .13, .22)
    def flow(p):
        x, h = p
        if x < 27:
            return -1.6 + .012*(h-45)
        if x > 73:
            return -1.4 + .011*(h-45)
        return -.9 + .008*(h-40)
    grain(BODY_POLY, .66, flow, ["#FCF7EB", "#B4A997", "#E3D8C7", "#FFFDF7"], (.35, .85), .27)
    line(BACK, "#9D9487", .17, .42)
    line(BELLY, "#8E857A", .18, .42)
    D.pivot("body", (49, 30))
    D.close()


def neck():
    D.open("neck")
    D.path(cr(NECK), fill="url(#robe)")
    # Blanc du dernier tiers de l'encolure, même dégradé que l'épaule.
    white = NECK_POLY.difference(BLACK_POLY)
    glow("cou-blanc-volume", (90, 65), 6, 17, "#FFFCF1", .25, white, .3)
    for k in range(5):
        x, h = 80.5+k*2.35, 71+k*1.3
        ps = [(x-.4, h-.1), (x+.5, h-1.8), (x+1.0, h-3.9)]
        line_in(ps, "#BAAC93", .22, .4, white)
        line_in([(a-.28,b+.2) for a,b in ps], "#FFF7E8", .18, .5, white)
    D.path(geom_d(BLACK_POLY), fill="url(#noir)")
    glow("cou-noir-muscle", (108, 70), 7, 17, "#554942", .11, BLACK_POLY, -.35)
    glow("cou-gorge", (112, 72), 8, 3.5, "#080606", .32, BLACK_POLY, -.25)
    grain(white, .58, lambda p:-1.15, ["#FDF7EC", "#B6A994"], (.32,.72), .25)
    grain(BLACK_POLY, .50, lambda p:-1.35, ["#48403B", "#110E0D", "#322B28"], (.30,.65), .33)
    # Petits poils à la frontière, limités à une fraction de centimètre.
    for k in range(58):
        t=k/57
        i=min(5,int(t*6)); tt=t*6-i
        p=lerp(BLACK_LIMIT[i],BLACK_LIMIT[i+1],tt)
        line([add(p,(-.09,.14)),add(p,(.12,-.18))],"#2C2420",.07,.4)
    D.pivot("neck", NECK_PIVOT)
    D.close()


LEGS = [("leg-back-far", "back", 6, True), ("leg-front-far", "front", -6, True),
        ("leg-back-near", "back", 0, False), ("leg-front-near", "front", 0, False)]


def leg(name, kind, dx, far):
    """Les quatre aplombs carrés : jarret à 145°, avant-bras musclé et
    canon court ; sabot de 6 cm, deux onglons serrés."""
    if kind == "front":
        up=[(78,49),(89,49),(92,42),(91,34),(89,27),(88.6,22),(89,20),(88.6,18),
            (88.2,12),(88.2,7),(89,4.0),(90,2.8)]
        sole=[(90,2.8),(91.5,.45),(91.3,0),(85.2,0),(84.7,.65)]
        back=[(84.7,.65),(84.8,2.4),(83.8,4.2),(84.0,6.6),(84.2,11),(84.0,16.5),
              (83.3,18.7),(83.4,20.4),(82.4,25),(80,30),(77.5,35),(76.7,41),(78,49)]
        piv=(84,47); jt=CARPE; dew=(83.6,4.8)
    else:
        up=[(6,55),(25,55),(29,44),(26,36),(22,31),(18,27),(14.9,23),(13.9,20.8),
            (14.2,14),(14.6,8),(15.5,4.0),(16.7,2.8)]
        sole=[(16.7,2.8),(18,.45),(17.8,0),(11.6,0),(11.0,.65)]
        back=[(11,.65),(11.2,2.4),(10.2,4.2),(10.4,7.2),(10.3,12),(10,18),
              (8.5,20.8),(8.4,22.3),(8.9,26),(9.3,31),(8.1,38),(5.1,47),(6,55)]
        piv=(14,52); jt=JARRET; dew=(10,4.8)
    tr=lambda ps:[(x+dx,h) for x,h in ps]
    up,sole,back=tr(up),tr(sole),tr(back)
    contour=up+sole[1:]+back[1:-1]
    lp=region(contour)
    x0,h0,x1,h1=lp.bounds
    base,lt,sh=("#C4BDB0","#DBD4C8","#999183") if far else ("#E8DECD","#F4EDE2","#ACA394")
    D.open(name)
    grad=D.lin(name+"-poil",(x0,jt),(x1,jt),[(0,sh),(.24,lt),(.57,base),(1,sh)])
    D.path(runs_path([up,sole,back]),fill=grad)
    glow(name+"-haut",(piv[0]+dx,31 if kind=="front" else 39),7,9,"#8D8373",.18,lp)
    glow(name+"-joint",((86.0 if kind=="front" else 11.4)+dx,jt+.7),2,2.5,lt,.5,lp)
    if kind=="front":
        line(tr([(83.7,17),(84.4,12),(84.4,7)]),sh,.25,.38)
        line(tr([(88.4,22),(88.6,20),(88.2,18.1)]),sh,.24,.5)
    else:
        line(tr([(9.3,31),(8.8,26),(8.7,22)]),sh,.25,.5)
        line(tr([(21,32),(17,27),(14,23)]),sh,.5,.25)
        line(tr([(11,17),(11.4,12),(11.6,7)]),sh,.25,.4)
    grain(lp,.51 if not far else .66,lambda p:-1.55,[lt,sh,base],(.28,.65),.3)
    line(up[3:]+sole[1:]+back[1:-3],"#847A6B" if far else "#A59883",.15,.55)
    # Sabots noirs : talon arrondi, pince carrée, fente et usure grise.
    toe,heel=sole[0],sole[-1]
    hp=[toe,sole[1],sole[2],sole[3],sole[4],add(heel,(.25,1.65)),lerp(add(heel,(.25,1.65)),toe,.5)]
    hg=D.lin(name+"-onglon",(heel[0],1),(toe[0],2),[(0,"#0A0909"),(.4,"#1C1817"),(.72,"#403A35"),(1,"#1B1716")])
    D.path(cr(hp,t=.5),fill=hg)
    line([add(sole[3],(.3,.22)),lerp(sole[3],sole[2],.6),add(sole[2],(-.25,.22))],"#4E4642",.15,.65)
    mid=lerp(sole[3],sole[2],.61)
    line([add(mid,(0,.1)),add(mid,(-.2,1.0)),lerp(heel,toe,.57)],"#090808",.17,.9)
    line([add(toe,(-.25,-.2)),lerp(toe,sole[1],.65)],"#70665B",.16,.45)
    de=(dew[0]+dx,dew[1])
    D.path(cr(ellipse(de,.43,.55,.2,10)),fill="#292320")
    D.pivot(name,(piv[0]+dx,piv[1]))
    D.close()


def scrotum():
    # Pigmentation voulue, pas de mamelle ; le bas ne dépasse pas le jarret.
    p=[(24,32),(31,31),(33,28),(32.5,25),(30.5,22),(27.5,21.8),(25,24),(24,28)]
    D.path(cr(p),fill=D.lin("scrotum",(25,28),(32,24),[(0,"#7E6D60"),(.45,"#A79A87"),(1,"#5E4A42")]))
    line([(28.3,29),(29.2,26),(29.5,22.4)],"#66544A",.17,.55)
    grain(region(p),.55,lambda p:-1.4,["#CDC0A8","#53473E"],(.3,.6),.25)


def tail():
    D.open("tail")
    # La racine rentre dans la croupe ; son disque reste solidaire de
    # l'attache quand la queue bat pendant les sauts et l'ébrouement.
    pivot=(9.0,56.0)
    p=[(10.5,58),(5.5,65),(2.4,66),(-.5,65.7),(-3.5,63.7),(-4.2,61.6),(-3.3,60),(-1.2,60.3),(1.8,62.8),(7,54.5)]
    D.path(cr(p),fill="url(#robe)")
    D.path(cr(ellipse(pivot,2.1,2.1)),fill="url(#robe)")
    # La réserve diagonale jusqu'au pivot est cachée par la culotte ;
    # elle garde le moignon attaché quand le coffre pivote dans un bond.
    grain(region(p),.43,lambda p:-2.2,["#FFFAEF","#AA9D88"],(.3,.6),.35)
    line(p[3:8],"#A79A86",.17,.5)
    D.pivot("tail",pivot)
    D.close()


def ear(base, name, far=False):
    D.open(name)
    # Oreille raccourcie par la perspective : longueur anatomique 14 cm,
    # projection de profil 10.8 ; largement arrondie, un peu tombante.
    angle=math.radians(12 if not far else 5)
    length=10.8 if not far else 6.3
    u=(-math.cos(angle),-math.sin(angle));v=(u[1],-u[0])
    F=lambda a,b:add(base,add(mul(u,a),mul(v,b)))
    ps=[(-.6,0),(1.3,1.6),(4.0,2.4),(7.0,2.25),(length,1.0),(length+.25,-.1),
        (length-1.0,-1.5),(6,-2),(2,-1.7),(-.6,0)]
    if far:ps=[(a*.7,b*.65) for a,b in ps]
    out=[F(a,b*(1.4 if not far else 1)) for a,b in ps]
    D.path(cr(out),fill=D.lin(name+"-noir",F(3,2.3),F(3,-2),[(0,"#3C3330"),(.4,"#211B19"),(1,"#100D0D")]))
    inn=[F(a,b*1.4) for a,b in [(1,0),(3,1.1),(7,1.3),(length-.8,.2),(7,-1),(3,-.9)]]
    if far:inn=[]
    if inn:
        D.path(cr(inn),fill=D.lin(name+"-peau",F(4,1),F(4,-1),[(0,"#3C2D2A"),(1,"#181211")]))
        line([F(1,0),F(4,-.65),F(8,-.5)],"#5A4239",.15,.4)
        grain(region(out),.39,lambda p:-.4,["#4B4039","#171312"],(.25,.48),.45)
    line(out[1:6],"#50443D",.13,.65)
    D.pivot(name,base)
    D.close()


def jaw():
    D.open("jaw")
    # La bouche et sa langue sont cachées par le chanfrein au repos.
    D.path(cr(HH([(16.4,5),(18.4,4.2),(21.1,4.2),(23.6,4.8),(23,6.6),(20,8.4),(16.3,8.9)])),fill="#2F171B")
    D.path(cr(HH([(19,5.8),(20.5,5.1),(22.8,5),(23.1,5.6),(21.1,6.3)])),fill="#9E646B")
    D.path(cr(HH([(21.7,5.4),(22.8,5.2),(22.9,5.6),(21.8,5.8)])),fill="#DDD5C7")
    chin=HH([(17.5,6.1),(20.7,5.65),(23.7,5),(24,5.6),(23.6,8.0),(22.7,9.4),(20.7,10.7),
             (18.1,11.5),(15.7,11.6),(13.9,11.9),(14,7.7),(16,6.5)])
    D.path(cr(chin),fill=D.lin("menton",HL(20,5),HL(20,9.5),[(0,"#282120"),(.5,"#221C19"),(1,"#100D0D")]))
    line(HH([(17.6,6.1),(20.3,5.75),(22.4,5.3),(23.7,5.15)]),"#090707",.23,.9)
    line(HH([(20.5,8.7),(22,8.2),(23.1,7.35)]),"#4C433D",.12,.4)
    grain(region(chin),.36,lambda p:-2.4,["#4A403A","#0E0B0B"],(.22,.45),.32)
    D.pivot("jaw",HL(16.5,6.9))
    D.close()


def eye():
    def E(a,b):return add(EYE,(a,-b))
    almond=[E(*p) for p in [(-1.5,0),(-1.1,-.65),(-.2,-.88),(.8,-.7),(1.5,-.1),(1.15,.55),(.1,.72),(-1,.52)]]
    # Paupière fermée : dessin permanent sous le groupe qui cligne.
    D.path(cr(almond),fill="#292322")
    line([E(-1.4,.1),E(0,.25),E(1.4,.05)],"#0D0B0B",.2,.8)
    D.open("eyes")
    D.path(cr(almond),fill="#624635")
    D.open("pupils")
    ic=add(EYE,(-.15,.02))
    D.path(cr(ellipse(ic,1.24,.84,0,20)),fill=D.rad("iris",ic,1.3,[(0,"#DEA273"),(.45,"#CE9464"),(.78,"#B97F55"),(1,"#784B2D")]))
    for k in range(28):
        a=k*math.tau/28
        p=add(ic,(.62*math.cos(a),.4*math.sin(a)))
        q=add(ic,(1.13*math.cos(a+.03),.74*math.sin(a+.03)))
        D.path(lstroke(p,q,2),stroke="#8A593B" if k%2 else "#F0B780",sw=.045,so=.6,cap="round")
    # Rectangle horizontal arrondi : aucune rotation avec l'axe du crâne.
    pw,ph=1.65,.44
    ps=[(-pw/2+.15,-ph/2),(pw/2-.15,-ph/2),(pw/2,0),(pw/2-.15,ph/2),
        (-pw/2+.15,ph/2),(-pw/2,0)]
    D.path(cr([add(ic,p) for p in ps],t=.45),fill="#17232A")
    D.pivot("pupils",ic)
    D.close()
    D.path(cr(ellipse(E(-.45,-.28),.17,.12,0,12)),fill="#EDEBE6",fo=.85)
    line(almond[:5],"#110D0C",.25,.95)
    line(almond[4:]+almond[:1],"#6E727D",.13,.8)
    # Les cils noirs protègent l'œil ; pas de tour roux ou de blanc.
    for k in range(11):
        t=k/10
        p=E(-1.2+2.4*t,-.5-.28*math.sin(t*math.pi))
        line([p,add(p,(.12,-.35))],"#0A0808",.065,.8)
    D.pivot("eyes",EYE)
    D.close()


def head():
    D.open("head")
    ear(HL(-.8,-1.9),"ear-far",True)
    # Scur court de 3,6 cm, pointe en haut et en arrière ; aucun anneau.
    sc=[HL(-.5,-2.5),HL(-1.7,-3.6),HL(-1.8,-5.9),HL(-.6,-6.3),HL(.4,-5.0),HL(1.7,-3.2)]
    if not VARIANTE:
        D.path(cr(sc,t=.7),fill="url(#scur)")
        line(sc[1:4],"#B59A71",.16,.6)
    jaw()
    D.path(runs_path([HH(DORSAL),HH(MOUTH),HH(NOTCH),HH(UNDER)]),fill="url(#tete)")
    glow("joue",HL(7.2,6),5,4,"#544840",.13,HEAD_POLY,-HEAD_ANGLE)
    glow("arcade",HL(8,-1.35),3,1.2,"#564C45",.18,HEAD_POLY,-HEAD_ANGLE)
    glow("chanfrein",HL(15.8,-1.5),5,1.0,"#564940",.15,HEAD_POLY,-HEAD_ANGLE)
    grain(HEAD_POLY,.37,lambda p:-HEAD_ANGLE-.10,["#4A403A","#151110","#342A25"],(.22,.5),.35)
    # Deux rides du front et les plis plus courts sur le large mufle.
    for ps in [[(4.3,-2.4),(6.2,-1.8),(7.4,-.9)],[(7,-3),(8.8,-2.2),(9.9,-1.1)],
               [(13,-2.0),(14.1,-1.0),(14.6,.2)],[(17,-.7),(18.2,.3),(18.5,1.4)]]:
        line(HH(ps),"#100D0D",.22,.85)
        line(HH([(u-.18,v-.18) for u,v in ps]),"#493B33",.11,.3)
    # Lèvre supérieure et mufle nus, jamais brillants comme du plastique.
    nose=HH([(19.6,2.1),(21.6,1.8),(23.2,2.7),(23.8,4.1),(23.7,5),(21.1,5.6),(19.7,5.3)])
    D.path(cr(nose),fill=D.lin("truffe",HL(21,2),HL(21,5.8),[(0,"#302927"),(.55,"#1B1716"),(1,"#151110")]))
    # Narine virgule, légèrement oblique vers le haut du nez.
    nost=HH([(21.1,2.8),(22.2,2.65),(22.9,3.1),(22.7,3.7),(21.5,4.0),(21.5,3.5),(20.7,3.2)])
    D.path(cr(nost),fill="#0A0808")
    line(HH([(21,2.75),(22,2.5),(22.8,2.95)]),"#4C423C",.15,.4)
    for k in range(18):
        u=rng.uniform(19.9,23.2);v=rng.uniform(4.2,5.1)
        D.path(cr(ellipse(HL(u,v),.055,.04,0,8)),fill="#63554A",fo=.3)
    # Gencive cachée sous la lèvre : bourrelet, aucune incisive supérieure.
    gum=HH([(20.1,4.4),(21.8,4.1),(23.4,4.5),(23.4,5.0),(21.8,5.3),(20.2,5.5)])
    D.path(cr(gum),fill="#88575B")
    D.open("lip")
    lip=HH([(19.9,4.1),(21.5,3.9),(23.6,4.1),(23.9,4.9),(23.5,5.1),(21.8,5.45),(20,5.8)])
    D.path(cr(lip),fill=D.lin("levre-haute",HL(21,4),HL(21,5.7),[(0,"#302725"),(.6,"#1B1716"),(1,"#110D0C")]))
    line(HH([(23.6,4.7),(23.3,5.05),(22.4,5.15)]),"#4D423D",.12,.45)
    D.pivot("lip",LIP_PIVOT)
    D.close()
    eye()
    # L'angle de la ganache est net, sans fanon ni barbe sous la gorge.
    line(HH([(1.3,9.0),(3.7,10.9),(6.4,12.4),(10,12.8)]),"#0B0908",.16,.6)
    ear(HL(1.6,-.5),"ear-near")
    D.pivot("head",POLL)
    D.pivot("muzzle",MUZZLE)
    D.close()


def articulate(svg):
    root=ET.fromstring(svg)
    paints=A.Paints(root)
    for name,kind,dx,far in LEGS:
        joint,half=A.leg_joint(svg,name,S((0,CARPE if kind=="front" else JARRET))[1])
        A.split_leg(root,paints,name,joint,half+.4)
    ET.indent(root,space="  ")
    return ET.tostring(root,encoding="unicode")+"\n"


def main(path):
    gradients()
    for spec in LEGS[:2]:leg(*spec)
    for spec in LEGS[2:]:leg(*spec)
    neck()
    tail()
    body()
    head()
    D.pivot("ground",(50,0))
    comment=("  Dorper : bélier de trois à quatre ans, après sa mue naturelle de fin d'été.\n"
             "  1 unité = 1 cm ; garrot 70 cm. Profil droit, tête et cou noirs, poil court.\n"
             "  Généré par tools/races/dorper/gen_dorper.py, d'après docs/races/dorper.md.\n"
             "  © 2026 Alejandro San Martin Lamas — MIT OR Apache-2.0")
    svg=articulate(D.render(148,104,comment))
    if VARIANTE:
        return variante(path, svg)
    Path(path).parent.mkdir(parents=True,exist_ok=True)
    Path(path).write_text(svg,encoding="utf-8")
    print(f"{path} : {len(svg)/1024:.0f} Ko, {svg.count('<path')} tracés")


def variante(path, svg):
    """Les autres dessins : ce qui change, puis les proportions."""
    root = ET.fromstring(svg)
    if VARIANTE == "brebis":
        # La mamelle et les trayons pigmentés que demande le standard.
        variantes.mamelle(root, *variantes.sous_le_ventre(root, 12, -1.5), largeur=11, hauteur=7.5,
                  peau="#8A6A60", ombre="#4A3530")
    if VARIANTE == "agneau":
        # La queue entière, mince et à poil, avant qu'on ne la coupe.
        variantes.queue_agneau(root, longueur=24, epaisseur=3.6, clair="#EFE8DC", sombre="#B5AA9B")
    variantes.ecrire(path, root, **PROPORTIONS[VARIANTE])


if __name__=="__main__":
    main(sys.argv[1] if len(sys.argv)>1 else HERE.parents[2]/"pets/dorper/dorper.svg")
