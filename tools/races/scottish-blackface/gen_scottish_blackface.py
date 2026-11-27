#!/usr/bin/env python3
"""Un bélier Scottish Blackface Lanark de quatre ans, en toison naturelle.

Une unité vaut un centimètre. Le repère de travail est x vers le nez,
h vers le haut, sol à zéro. Les mesures viennent de la fiche de race ;
le dessin, original, suit les photos P2, P9, P10, P20 et P23.
Les photos ne sont jamais nécessaires au générateur ni copiées au dépôt.

Usage : python3 gen_scottish_blackface.py pets/scottish-blackface/scottish-blackface.svg
© 2026 Alejandro San Martin Lamas — MIT OR Apache-2.0
"""
import math
import os
import random
import sys
import xml.etree.ElementTree as ET

HERE = os.path.dirname(os.path.abspath(__file__))
sys.path.insert(0, HERE)
sys.path.insert(0, os.path.dirname(HERE))
import articulations as A
import variantes  # noqa: E402

# Les autres dessins de la race (tools/races/variantes.py), d'après la fiche.
# Sans variante : le bélier.
VARIANTE = sys.argv[2] if len(sys.argv) > 2 else ""
PROPORTIONS = {
    "brebis": dict(  # Brebis Scottish Blackface : 63 cm au garrot, cornes fines en arc vers l'arrière, tête et encolure plus fines.
        garrot=(70, 63), encolure=(1.0, .86), tete=(.96, .93)),
    "agneau": dict(  # Agneau Scottish Blackface de deux semaines : 38 cm au garrot, face et pattes déjà noires et blanches, sans cornes.
        variantes.AGNEAU, garrot=(70, 38), corps=(.84, .8)),
}
from outils import Doc, OY, S, cr, dense, ellipse_pts, mix, num, poly, quad
from shapely.geometry import LineString, Point, Polygon

# Garrot osseux 70 ; le dos de laine est à 83–84. Épaule–fesse : 84.
GARROT, CARPE, JARRET = 70.0, 18.0, 22.0
POLL = (104.0, 91.0)
NECK_BASE = (88.0, 54.0)
HEAD_ANGLE = math.radians(43)
FORE, HIND = 81.0, 7.0
SHOULDER_H, HIP_H = 49.0, 53.0
SEED = 1901  # fondation de l'association de Lanark
D = Doc()
rng = random.Random(SEED)


def hl(u, v):
    """Repère de la tête : u nuque→nez, v du chanfrein→ganache."""
    c, s = math.cos(HEAD_ANGLE), math.sin(HEAD_ANGLE)
    return POLL[0] + u*c - v*s, POLL[1] - u*s - v*c


def hls(points):
    return [hl(*p) for p in points]


def geom(g):
    """Écrit une intersection géométrique réelle, jamais un clip SVG."""
    if g.is_empty:
        return ""
    g = g.simplify(.025, preserve_topology=True)
    if g.geom_type == "Polygon":
        return poly(list(g.exterior.coords)[:-1]) + "".join(poly(list(r.coords)[:-1]) for r in g.interiors)
    return "".join(geom(p) for p in getattr(g, "geoms", []))


def region(points):
    return Polygon(dense(points, True, 6)).buffer(0)


def shape(points, fill, **kw):
    D.el(cr(points), fill=fill, **kw)


def spot(gid, center, rx, ry, col, alpha, area, angle=0):
    pts = ellipse_pts(*center, rx, ry, angle, 30)
    sh = Polygon(pts).intersection(area)
    grad = D.ell(gid, center, rx, ry, -angle, [(0, col, alpha), (.65, col, alpha*.35), (1, col, 0)])
    D.el(geom(sh), fill=grad, rule="evenodd")


# Contour sobre, recouvert de fibres et de pointes indépendantes. La croupe
# est pleine et carrée, le dessous remonte vers le poitrail (à droite).
BODY = [(-6,63), (-6,73), (0,80), (13,83), (34,83), (57,82), (76,84), (81,81),
        (85,75), (88,65), (90,53), (87,40), (74,35), (55,32), (34,31), (15,30), (1,34), (-7,46)]
BP = region(BODY)
NECK = [(68,62), (72,77), (83,86), (94,94), (103,95), (109,89), (110,78), (108,63),
        (109,48), (103,37), (95,34), (85,39), (76,46), (67,50)]
NP = region(NECK)


def wool_color(x, h, dark=0):
    """Lumière continue du volume, indépendante du quadrillage des racines."""
    light = .45 + .28*(h-30)/54 + .07*math.sin(x*.071+h*.037)
    light += .045*math.sin(x*.31+h*.11) - dark
    light = max(0, min(1, light))
    return mix("#9C9384", "#EEEAE1", light)


def lock(x, h, length, width, ang, rand, area=None, dark=0, tip=False):
    """Mèche droite de laine grossière : une grande ondulation lâche, pointe
    vrillée et fibres longitudinales. Pas les tire-bouchons d'un Nez noir."""
    phase = rand.uniform(0, 6.28)
    bend = rand.uniform(-2.4, 2.4)
    n = max(8, int(length/.85))
    axis, left, right = [], [], []
    for k in range(n+1):
        t = k/n
        lateral = bend*math.sin(math.pi*t) + .55*math.sin(phase+7*t)*math.sin(math.pi*t)
        xx = x + math.sin(ang)*length*t + math.cos(ang)*lateral
        hh = h - math.cos(ang)*length*t + math.sin(ang)*lateral
        # La racine s'effile dans le fond : aucune calotte ni ligne de coupe.
        half = width/2 * (.92-.18*t) * (1-t**3.0) * min(1, t/.16)
        half *= 1 + .045*math.sin(phase+12*t)
        axis.append((xx,hh))
        left.append((xx-math.cos(ang)*half,hh-math.sin(ang)*half))
        right.append((xx+math.cos(ang)*half,hh+math.sin(ang)*half))
    sh = Polygon(dense(left+right[::-1], True, 2)).buffer(0)
    if area is not None:
        sh = sh.intersection(area)
    D.el(geom(sh), fill="url(#laine-fibres)")
    D.el(geom(sh), fill="#726856", fo=.025 + dark*.22 + rand.uniform(0,.045))
    # Une bande étroite à l'ombre sépare les mèches sans les cerner.
    for f, col, alpha, sw in [(-.34,"#6B6559",.16,.12),(-.05,"#F7F3E9",.29,.12),(.24,"#FFFDF6",.24,.075)]:
        paths = []
        pts=[]
        for k,(xx,hh) in enumerate(axis):
            t=k/n
            shift = f*width*(1-t**3)
            p=(xx+math.cos(ang)*shift,hh+math.sin(ang)*shift)
            if area is None or area.buffer(-.05).contains(Point(p)):
                pts.append(p)
            elif len(pts)>1:
                paths.append(cr(pts,False));pts=[]
        if len(pts)>1:paths.append(cr(pts,False))
        D.el("".join(paths),stroke=col,sw=sw,so=alpha,cap="round")
    # Les pointes salies et effilées n'ont pas de bord de coupe horizontal.
    if tip:
        p=axis[-1]
        D.line([axis[-3],p,(p[0]+rand.uniform(-.5,.5),p[1]-.65)],"#B6A78B",.1,so=.55)


def coat(area, rand, dark=0, lo=26, hi=86, step=5.3, angle=0, mark=None, sheets=None):
    """Des racines décalées ; l'irrégularité vient des mèches, pas du bruit
    appliqué à une ellipse. Les racines hautes recouvrent les basses.
    `mark(h, x)` : le numéro de la nappe d'encolure d'une mèche (`h` : la
    hauteur de sa rangée, `x` : sa racine), ou rien. Les mèches d'une nappe
    sont mises de côté, dans leur ordre : `sheets(mèches par nappe)` les
    pose, par nappe, après celles qui restent à l'encolure et sous les fibres
    fines."""
    xmin,_,xmax,_=area.bounds
    h=lo
    row=0
    aside={}
    while h<hi:
        x=xmin-1+(row%2)*1.3
        while x<xmax+1:
            xx=x+rand.uniform(-.65,.65)
            hh=h+rand.uniform(-3.8,3.8)
            if area.contains(Point(xx,hh)):
                length=rand.uniform(11,26)
                # Les mèches du dessus s'écartent de la raie avant de pendre.
                a=angle + .14*math.sin(xx*.09) + rand.uniform(-.20,.20)
                a -= .38 * max(0,min(1,(hh-67)/18))
                k=mark(h,xx) if mark else None
                start=len(D.out)
                lock(xx,hh,length,rand.uniform(1.25,3.2),a,rand,area,dark)
                if k:
                    aside.setdefault(k,[]).extend(D.out[start:])
                    del D.out[start:]
            x+=2.15
        row+=1;h+=step
    if sheets:
        sheets(aside)
    # Des fibres fines non alignées avec les bandes de mèches.
    paths_light=[];paths_dark=[]
    for _ in range(int(area.area*.55)):
        x=rand.uniform(xmin,xmax);h=rand.uniform(lo,hi)
        length=rand.uniform(1.4,5.5)
        p=(x,h);q=(x+.16*length*math.sin(x*.13)+rand.uniform(-.18,.18),h-length)
        if area.contains(Point(p)) and area.contains(Point(q)):
            arr=paths_light if rand.random()<.55 else paths_dark
            arr.append(quad(p,((p[0]+q[0])/2+.25,(p[1]+q[1])/2),q,nd=2))
    D.el("".join(paths_dark),stroke="#8A8070",sw=.065,so=.21,cap="round")
    D.el("".join(paths_light),stroke="#FFFDF7",sw=.055,so=.34,cap="round")


def fringe(x0,x1,h0,rand,dark=0):
    x=x0
    while x<x1:
        lock(x,h0+rand.uniform(-2.5,2.5),rand.uniform(8,17),rand.uniform(1.35,2.7),
             rand.uniform(-.22,.22),rand,None,dark,True)
        x+=rand.uniform(1.1,1.65)


def body():
    # Les trois touffes articulées sont prises derrière le bas du corps.
    for k,(xa,xb) in enumerate(((18,35),(35,53),(53,70)),1):
        D.open(f"sway-{k}")
        fringe(xa,xb,36.5,random.Random(900+k),.05)
        D.pivot(f"sway-{k}",(xa+xb)/2,36.5)
        D.close()
    D.open("body")
    base=D.lin("laine-corps",(30,84),(30,30),[(0,"#E9E4DA"),(.45,"#D9D0C1"),(1,"#A69A85")])
    shape(BODY,base)
    coat(BP,random.Random(210),lo=30,hi=85,step=4.5)
    # Creux des faisceaux de laine : des masses longues et irrégulières,
    # distinctes des fibres fines, font lire l'épaisseur de la toison.
    folds=random.Random(953)
    for k in range(29):
        x=folds.uniform(0,88);h=folds.uniform(42,74)
        rx=folds.uniform(1.2,2.7);ry=folds.uniform(8,17)
        ang=folds.uniform(-.18,.18)
        spot(f"pli-laine-{k}",(x,h),rx,ry,"#746957",folds.uniform(.07,.15),BP,angle=ang)
        spot(f"pli-laine-lumiere-{k}",(x+rx*.8,h+2),rx*.8,ry*.9,"#FFFDF5",.09,BP,angle=ang)
    # Quelques volumes larges relient les milliers de fibres au tonneau
    # des côtes : lumière sur le flanc haut, ombre derrière épaule et sous ventre.
    spot("volume-flanc",(38,63),43,24,"#FFFDF4",.19,BP)
    spot("volume-ventre",(45,31),48,14,"#6B6559",.26,BP)
    spot("volume-croupe",(-5,51),18,26,"#7A705E",.19,BP)
    spot("volume-epaule",(85,53),16,28,"#817361",.13,BP)
    # Pointes de la culotte et de la frange, arrière plus bas que l'avant.
    edge_rng=random.Random(70)
    fringe(0,17,37,edge_rng,.06)
    fringe(17,70,36.0,edge_rng,.04)
    fringe(70,88,41,edge_rng,.03)
    for h in range(43,80,3):
        x=-6+.014*(h-62)**2
        lock(x,h,edge_rng.uniform(7,11),1.8,-.32,edge_rng,None,.03,True)
    # Raie dorsale : un sillon irrégulier, des fibres couchées vers les flancs.
    D.line([(2,80.5),(17,82.0),(36,81.8),(58,81.2),(74,82.8)],"#9C9384",.18,so=.3)
    for x in range(3,78,2):
        p=(x,82+.8*math.sin(x*.1))
        D.line([p,(x-1.1,81.8),(x-2.3,79.9)],"#F7F4EB",.16,so=.65)
    # Une frange de jarres rompt la ligne du dos sans former une crête
    # régulière : les fibres partent de part et d'autre de la raie.
    rr=random.Random(297)
    for k in range(150):
        x=rr.uniform(1,83)
        h=BP.intersection(LineString([(x,0),(x,100)])).bounds[3]-.15
        dx=rr.uniform(-.7,.7);length=rr.uniform(.4,1.3)
        D.line([(x,h),(x+dx*.35,h+length*.65),(x+dx,h+length)],
               "#CFC7B8" if k%3 else "#F1EDE3",rr.uniform(.045,.08),so=.56)
    D.pivot("body",42,39)
    D.close()


# Les trois nappes de l'encolure : sa longue laine pend au lieu de tourner
# avec elle. Chaque nappe est un groupe de rangées de mèches entières, de la
# racine à la pointe ; son attache, sur l'axe de l'encolure, suit le cou, et
# la nappe pend de là, à la verticale (docs/rig.md, « Les nappes »).
#   1. le poitrail, sous la base : son attache est la base de l'encolure,
#      elle ne bouge pas quand il broute (c'est la laine du poitrail) ;
#   2. le milieu ; 3. le haut, sous la nuque.
# (rangées jusqu'à cette hauteur, attache). Derrière `NECK_HIDDEN`, sous le
# garrot, la réserve cachée par le corps reste à l'encolure et tourne avec
# elle : elle fait le dessus du cou baissé.
# Le bord du haut de chaque nappe (la ligne d'où elle se balance) : celui de
# sa rangée la plus haute.
NECK_SHEETS=[(57.0,NECK_BASE,58.0),(73.0,(92.8,65.0),74.2),(200.0,(101.0,84.0),95.0)]
NECK_HIDDEN=(86.0,78.0)


def neck_sheet(h,x):
    if x<NECK_HIDDEN[0] and h<NECK_HIDDEN[1]:
        return None
    for k,(top,_,_) in enumerate(NECK_SHEETS,1):
        if h<top:
            return k
    return None


def neck():
    D.open("neck")
    base=D.lin("laine-cou",(87,93),(101,36),[(0,"#E9E4D9"),(.45,"#D8CEC0"),(1,"#A99C87")])
    shape(NECK,base)
    # Les mèches des nappes, mises de côté, puis posées par nappe ; le reste
    # de la toison (réserve, fibres fines) reste à l'encolure.
    def sheets(aside):
        for k,(_,pivot,edge) in enumerate(NECK_SHEETS,1):
            D.open(f"neck-sway-{k}")
            D.out.extend(aside.get(k,[]))
            if k==1:
                # La frange du poitrail pend avec lui.
                fringe(93,105,48,random.Random(612),.05)
            D.pivot(f"neck-sway-{k}",*pivot)
            D.pivot(f"neck-sway-{k}-top",pivot[0],edge)
            D.close()
    coat(NP,random.Random(611),dark=.02,lo=38,hi=95,step=5.4,angle=.04,mark=neck_sheet,sheets=sheets)
    spot("volume-gorge",(106,52),12,26,"#736755",.19,NP)
    spot("volume-cou",(91,79),14,25,"#FFFDF5",.19,NP)
    # Épaisseur complète cachée sous le corps, aucune encoche à la rotation.
    D.pivot("neck",*NECK_BASE)
    D.close()


def hoof(x,far,gid):
    pal="#151618" if far else "#252528"
    pts=[(x-1.9,4.5),(x+1.8,4.3),(x+3.8,1.2),(x+3.75,.2),(x-1.8,.2),(x-2.3,1.0)]
    g=D.lin(gid+"-onglon",(x,4.5),(x,0),[(0,"#4A4A4C" if not far else "#323236"),(.5,pal),(1,"#101112")])
    shape(pts,g)
    D.line([(x+.4,3.8),(x+1.3,1.6),(x+1.1,.35)],"#0B0C0D",.26,so=.9)
    D.line([(x-1.65,.65),(x+1,.4),(x+3.35,.45)],"#77746D",.14,so=.7)
    shape([(x-2.1,6.8),(x-2.8,6.1),(x-2.6,5.0),(x-1.9,5.7)],"#222226")


LEGS=[("leg-back-far",HIND+7,False,True),("leg-front-far",FORE-6,True,True),
      ("leg-back-near",HIND,False,False),("leg-front-near",FORE,True,False)]


def leg(gid,x,front,far):
    D.open(gid)
    if front:
        pts=[(x-4,52),(x+4.6,53),(x+3.0,34),(x+2,19),(x+1.5,8),(x+2,4.1),
             (x-1.6,3.6),(x-2,8),(x-2.3,18),(x-3.3,32)]
        joint_h=CARPE
    else:
        pts=[(x+1,57),(x+10,55),(x+9,43),(x+5,32),(x+1.2,22),(x+.7,9),(x+2,4),
             (x-1.8,3.9),(x-2.3,8),(x-2.3,21),(x-1.4,27),(x+1.5,40)]
        joint_h=JARRET
    p=region(pts)
    g=D.lin(gid+"-peau",(x-2,18),(x+2.5,18),[(0,"#101114"),(.45,"#35373B" if not far else "#24262A"),(1,"#121316")])
    # Le fond de la culotte est laineux : en vol, une articulation peut
    # découvrir le haut de la patte derrière le corps, jamais un grand
    # triangle de peau noire. Le noir ras commence au carpe ou au jarret.
    shape(pts,"#A79D8E" if far else "#D8D0C3")
    bare=p.intersection(Polygon([(x-16,-2),(x+18,-2),(x+18,joint_h+2),(x-16,joint_h+2)]))
    D.el(geom(bare),fill=g)
    # Les taches franches appartiennent au poil ; on ne mélange pas noir/blanc.
    if front:
        patch=[(x-.8,17),(x+1.9,16.5),(x+1.7,13.5),(x+.5,10.7),(x-1,11.4),(x-1.4,14)] if not far else [
            (x-.5,16.4),(x+1.5,15.4),(x+1.7,14),(x+.5,12),(x-1,12.7),(x-1.5,15)]
    else:
        patch=[(x-1.9,14.1),(x+.65,15.2),(x+.9,12.1),(x+.1,9.5),(x-1.7,9.9)] if not far else [
            (x-1.7,12.7),(x+.5,14.6),(x+.9,11.1),(x-.2,8.6),(x-1.8,9.8)]
    D.el(geom(region(patch).intersection(p)),fill="#C6C4BE" if far else "#E6E2DA")
    shape([(x-1.5,5),(x+1.5,5.1),(x+1.8,4.0),(x-1.4,3.8)],"#CDCBC4" if far else "#EFECE5")
    # Lumière des tendons et quelques fibres rases, jamais un bas de patte laineux.
    D.line([(x+.8,18),(x+.45,10),(x+.6,6)],"#55585C",.18,so=.5)
    rr=random.Random(81+int(x))
    hairs=[]
    for _ in range(95):
        px=x+rr.uniform(-2.0,2);h=rr.uniform(5,31)
        if p.buffer(-.25).contains(Point(px,h)):
            hairs.append(cr([(px,h),(px+.12,h-rr.uniform(.3,.65))],False))
    D.el("".join(hairs),stroke="#62656A",sw=.055,so=.33,cap="round")
    # Culotte de laine, dont les pointes s'arrêtent juste au-dessus du joint.
    upper=p.intersection(Polygon([(x-15,joint_h+7),(x-2,joint_h+5),(x,joint_h+7),(x+2,joint_h+5.5),
                                  (x+17,joint_h+7),(x+17,62),(x-15,62)]))
    D.el(geom(upper),fill="#A79D8E" if far else "#D8D0C3")
    coat(upper,random.Random(int(x)+710),dark=.2 if far else .05,lo=joint_h+4,hi=55,step=6.0)
    for k in range(7):
        lock(x-2+k*.75,joint_h+rr.uniform(12,16),rr.uniform(8,11),1.3,rr.uniform(-.12,.12),rr,None,
             .2 if far else .07,True)
    hoof(x,far,gid)
    D.pivot(gid,x+(.5 if front else 5),SHOULDER_H if front else HIP_H)
    D.close()


def tail_piece(gid,pts,seed):
    sh=region(pts)
    shape(pts,"#C8BEAB")
    coat(sh,random.Random(seed),dark=.12,lo=20,hi=76,step=5.5)
    rr=random.Random(seed+20)
    cx=sum(x for x,h in pts)/len(pts);bottom=min(h for x,h in pts)
    for k in range(5):lock(cx-2+k,bottom+9,rr.uniform(8,13),1.45,rr.uniform(-.16,.16),rr,None,.13,True)


def tail():
    D.open("tail")
    tail_piece("queue",[(-1,72),(-6,73),(-9,66),(-9,56),(-6,51),(-1,55),(0,65)],420)
    D.open("tail-mid")
    tail_piece("queue-milieu",[(-7,59),(-11,56),(-12,43),(-9,38),(-4,41),(-3,54)],421)
    D.open("tail-tip")
    tail_piece("queue-bout",[(-10,45),(-13,41),(-14,30),(-11,23),(-6,26),(-5,39)],422)
    D.pivot("tail-tip",-8.5,43);D.close()
    D.pivot("tail-mid",-5,57);D.close()
    D.pivot("tail",-2,71);D.close()


# Le crâne sec et légèrement busqué ; la bouche est une encoche recouverte
# par la mâchoire, pour qu'aucun trait ne demeure quand elle s'ouvre.
DORSAL=[(-2,-2),(3,-4),(8,-3.8),(13,-3.5),(18,-2.7),(23,-1.2),(26.5,.6),(28,2.3),(28.3,4)]
MOUTH=[(28.3,4),(27.3,4.9),(24,5.6),(21.5,6)]
UNDER=[(21.5,6),(19.2,9.7),(14,11.7),(8,11.4),(3,9.2),(-1.5,6),(-3,1)]
HP=region(hls(DORSAL+MOUTH[1:]+UNDER[1:]))
MUZZLE_L=[(20,-1.9),(23,-1.2),(26.5,.6),(28,2.3),(28.3,4),(27.3,5.1),(24.3,6),(21,7.1),(18.7,8.2),(17.9,5.1)]
MP=region(hls(MUZZLE_L))
LIP_L=[(24,3.2),(26,2.8),(28.2,3.2),(28.4,4),(27.2,5.0),(24,5.6),(22.9,5.7),(23,4.1)]


def ear(gid,far=False):
    D.open(gid)
    base=(101.3,85.4) if not far else (102,88)
    pts=[base,(97,86),(91,84.1),(89.4,82.2),(93.3,80.8),(98.2,82.2),(101.3,83.2)]
    if far:pts=[(p[0]+1.2,p[1]+2.0) for p in pts]
    g=D.lin(gid+"-peau",(97,86),(95,81),[(0,"#38363B"),(.5,"#18171A"),(1,"#0C0C0D")])
    shape(pts,g)
    D.line([(99.4,84.4),(95,84),(91.6,82.6)],"#71696B",.38,so=.6)
    D.line([(99,82.8),(94.7,81.9),(91.8,82)],"#BAB6B0",.14,so=.65)
    for k in range(8):
        x=92+k*.8
        D.line([(x,82.0),(x-.4,81.65)],"#DAD6CF",.055,so=.6)
    D.pivot(gid,*base);D.close()


def horn_geom(far=False):
    # Hélice projetée, 1¼ tour : racine basse à plat, boucle largement
    # dégagée de la joue, retour qui se déporte vers le spectateur. Le
    # quart de tour en plus monte vers l'arrière au-dessus de l'oreille et
    # finit pointe en haut et en arrière (fiche § 6.2, point 6) : il ne
    # replonge pas au centre de la spirale, sur l'oreille.
    axis=[(104,91),(100,94),(93,94),(87,90),(83.8,84),(84.2,77),(88,71.7),
          (94.2,69.9),(100.4,72.4),(104.4,78),(104.4,84),(101.7,88.1),
          (98.4,89.6),(95.6,89.9),(93.6,90.6)]
    if VARIANTE=="brebis":
        # La brebis : un arc fin vers l'arrière et le bas, sans tour complet (§ 1.5).
        axis=axis[:6]
    if far:axis=[(104+(x-104)*.92+1.0,91+(h-91)*.92+1.4) for x,h in axis]
    c=dense(axis,False,7)
    normals=[];widths=[];outer=[];inner=[];acc=[0]
    for i,p in enumerate(c):
        a,b=c[max(0,i-1)],c[min(len(c)-1,i+1)]
        dx,dh=b[0]-a[0],b[1]-a[1];l=math.hypot(dx,dh)
        n=(-dh/l,dx/l)
        w=(7.2 if VARIANTE!="brebis" else 3.8)*(1-i/(len(c)-1))**.72+.12
        if far:w*=.9
        normals.append(n);widths.append(w)
        outer.append((p[0]+n[0]*w/2,p[1]+n[1]*w/2))
        inner.append((p[0]-n[0]*w/2,p[1]-n[1]*w/2))
        if i:acc.append(acc[-1]+math.dist(p,c[i-1]))
    return c,normals,widths,outer,inner,acc


def horn(far=False):
    c,n,w,outer,inner,acc=horn_geom(far)
    if far:
        # Arc supérieur et portion arrière seulement, le reste est occulté
        # par le crâne et la corne proche dans le profil strict.
        m=15
        # La fin de l'arc lointain se glisse derrière la corne proche :
        # ses huit derniers points s'amincissent, sans section apparente.
        for i in range(m-8,m):
            t=(i-(m-8))/7
            scale=1-t*t*(3-2*t)
            outer[i]=(c[i][0]+n[i][0]*w[i]*.5*scale,c[i][1]+n[i][1]*w[i]*.5*scale)
            inner[i]=(c[i][0]-n[i][0]*w[i]*.5*scale,c[i][1]-n[i][1]*w[i]*.5*scale)
        outer=outer[:m];inner=inner[:m]
        g=D.lin("corne-lointaine",(99,97),(97,80),[(0,"#C0A98B"),(.6,"#80654E"),(1,"#5D4938")])
        shape(outer+inner[::-1],g)
        return
    g=D.rad("corne",(96,82),17,[(0,"#6E5640"),(.35,"#9E7E5E"),(.68,"#D8C2A4"),(1,"#A8896A")])
    shape(outer+inner[::-1],g)
    # Deux faces arrondies de la section triangulaire.
    strip=[(p[0]+nn[0]*ww*.12,p[1]+nn[1]*ww*.12) for p,nn,ww in zip(c,n,w)]
    shape(outer+strip[::-1],"#E1CEB2",fo=.26)
    strip2=[(p[0]-nn[0]*ww*.15,p[1]-nn[1]*ww*.15) for p,nn,ww in zip(c,n,w)]
    shape(inner+strip2[::-1],"#4A3A2C",fo=.25)
    for f in [-.23,.26]:
        line=[(p[0]+nn[0]*ww*f,p[1]+nn[1]*ww*f) for p,nn,ww in zip(c[2:-8],n[2:-8],w[2:-8])]
        D.line(line,"#5A3A22",.12,so=.18)
    rr=random.Random(1901);s=.35;dark=[];light=[]
    while s<acc[-1]-5:
        i=next(k for k,v in enumerate(acc) if v>=s)
        p=c[i];nn=n[i];ww=w[i]
        a=(p[0]+nn[0]*ww*.47,p[1]+nn[1]*ww*.47)
        b=(p[0]-nn[0]*ww*.47,p[1]-nn[1]*ww*.47)
        tangent=(c[min(i+1,len(c)-1)][0]-p[0],c[min(i+1,len(c)-1)][1]-p[1])
        tl=math.hypot(*tangent) or 1
        off=(tangent[0]/tl*.35,tangent[1]/tl*.35)
        dark.append(quad(a,(p[0]+off[0],p[1]+off[1]),b,2))
        light.append(quad((a[0]+off[0]*.5,a[1]+off[1]*.5),(p[0]+off[0]*1.5,p[1]+off[1]*1.5),
                          (p[0]-nn[0]*ww*.15,p[1]-nn[1]*ww*.15),2))
        s+=rr.uniform(.46,.65)
    D.el("".join(dark),stroke="#4A3A2C",sw=.11,so=.49,cap="round")
    D.el("".join(light),stroke="#F1DFC2",sw=.09,so=.56,cap="round")
    m=next(k for k,v in enumerate(acc) if v>=acc[-1]-5)
    tip=D.lin("corne-pointe",c[m],c[-1],[(0,"#4E4034",0),(1,"#4E4034")])
    shape(outer[m:]+inner[m:][::-1],tip)
    D.line(outer,"#6E5640",.18,so=.75)
    D.line(inner,"#4A3A2C",.18,so=.65)


def jaw():
    D.open("jaw")
    shape(hls([(11,5),(16,4),(22,3.9),(27.8,4.1),(28.2,5.8),(26.7,7.0),(22.2,8.7),
               (18,10.4),(12,11.6),(8,10.8),(7.5,7)]),"#272326")
    shape(hls([(20.3,4.8),(23.8,4.2),(27.2,4.4),(27.4,5.3),(24,6.2),(20,6.6)]),"#AD7376")
    g=D.lin("menton",hl(23,5),hl(23,9),[(0,"#F4F0E9"),(.5,"#D6D0C6"),(1,"#AAA59C")])
    shape(hls([(21.2,6),(24,5.7),(27.8,4.8),(28.3,5.5),(27.2,6.8),(24,8.1),
               (21.0,8.9),(18.7,8.2)]),g)
    D.line(hls([(22.2,5.95),(25.2,5.45),(28.1,4.8)]),"#171517",.29,so=.95)
    D.pivot("jaw",*hl(12,7));D.close()


def eyes():
    e=hl(9.0,1.5)
    win=[(e[0]-1.7,e[1]),(e[0]-.9,e[1]+.88),(e[0]+.2,e[1]+.83),(e[0]+1.5,e[1]+.2),
         (e[0]+1.6,e[1]-.25),(e[0]+.5,e[1]-.8),(e[0]-.7,e[1]-.65)]
    # Le pli fermé reste en dessous du groupe qui cligne.
    D.line([(e[0]-1.6,e[1]),(e[0],e[1]-.18),(e[0]+1.5,e[1]-.12)],"#0A090A",.19)
    D.open("eyes");shape(win,"#5F4D38")
    D.open("pupils")
    iris=D.rad("iris",e,1.3,[(0,"#B08A55"),(.6,"#8B7256"),(1,"#4F3F2E")])
    shape(ellipse_pts(*e,1.34,1.34,0,20),iris)
    for k in range(26):
        a=k*6.283/26
        D.line([(e[0]+.53*math.cos(a),e[1]+.53*math.sin(a)),
                (e[0]+1.2*math.cos(a+.08),e[1]+1.2*math.sin(a+.08))],"#C4A477" if k%2 else "#594632",.045,so=.6)
    X,Y=S(e[0]-.8,e[1]+.23)
    D.emit(f'<rect x="{num(X)}" y="{num(Y)}" width="1.6" height=".46" rx=".21" fill="#0D0C0B"/>')
    D.pivot("pupils",*e);D.close()
    shape(ellipse_pts(e[0]-.45,e[1]+.36,.22,.13,0,10),"#F2EEE6",fo=.9)
    D.pivot("eyes",*e);D.close()
    # Le pourtour réellement découpé cache l'iris au-dehors de l'amande.
    ring=Polygon(ellipse_pts(*e,3.3,2.6,0,28)).difference(Polygon(win))
    D.el(geom(ring),fill="url(#tete)",rule="evenodd")
    D.line(win+[win[0]],"#080809",.25,so=.9)
    D.line([(e[0]-1.4,e[1]+.5),(e[0]-.2,e[1]+1.15),(e[0]+1.3,e[1]+.6)],"#4C4C52",.17,so=.7)
    for k in range(6):
        x=e[0]-1.05+k*.4
        D.line([(x,e[1]+.82),(x-.12,e[1]+1.25)],"#09090A",.08)


def head():
    D.open("head")
    if VARIANTE!="agneau":
        horn(True)
    ear("ear-far",True)
    jaw()
    g=D.lin("tete",hl(9,-4),hl(9,10),[(0,"#3D3E45"),(.4,"#202126"),(.7,"#18181B"),(1,"#0B0B0C")])
    D.el(geom(HP),fill=g)
    # Petit col de poil blanc sous la ganache ; la laine ne monte pas au front.
    throat=hls([(-2,4),(2,8),(7,10),(11,10),(10,11.8),(5,12.5),(0,10),(-3,7)])
    shape(throat,"#E6E2DA")
    mg=D.lin("museau",hl(23,-1),hl(23,8),[(0,"#F7F5F0"),(.5,"#E6E2DA"),(1,"#A9A49B")])
    D.el(geom(MP.intersection(HP)),fill=mg)
    eye_keep=Polygon(ellipse_pts(*hl(9,1.5),3.3,2.6,0,28))
    black=HP.difference(MP).difference(eye_keep)
    spot("front-reflet",hl(7,-2.6),5,1.2,"#6D6D77",.27,black,angle=-HEAD_ANGLE)
    spot("joue-volume",hl(9,6),4.3,3,"#53545C",.22,black,angle=-HEAD_ANGLE)
    spot("chanfrein-reflet",hl(17,-1.8),4.3,1.0,"#74747D",.25,black,angle=-HEAD_ANGLE)
    rr=random.Random(45);hairs=[];whites=[]
    for _ in range(1400):
        u=rr.uniform(-2,28);v=rr.uniform(-4,10)
        p=hl(u,v);q=hl(u-rr.uniform(.25,.65),v+rr.uniform(-.05,.12))
        if HP.buffer(-.12).contains(Point(p)) and HP.buffer(-.12).contains(Point(q)) and not eye_keep.contains(Point(p)):
            arr=whites if MP.buffer(-.12).contains(Point(p)) else hairs
            arr.append(cr([p,q],False))
    D.el("".join(hairs),stroke="#64656C",sw=.052,so=.28,cap="round")
    D.el("".join(whites),stroke="#FFFFFF",sw=.055,so=.44,cap="round")
    # Truffe et grande narine en virgule ; lèvres pigmentées sous poil blanc.
    nose=hls([(25.2,.55),(26.8,1.15),(28,2.4),(28.3,3.7),(27.6,4.7),(26.3,4.6),(25.5,3.3)])
    ng=D.lin("truffe",hl(26.5,1.2),hl(27,4.6),[(0,"#4A4748"),(.5,"#252326"),(1,"#0D0C0D")])
    shape(nose,ng)
    shape(hls([(24.4,1.6),(25.9,1.85),(27.1,2.4),(27.55,3.0),(27.1,3.6),(26.3,3.2),(25.3,2.45)]),"#080808")
    D.line(hls([(25,1.45),(26,1.6),(27,2.1)]),"#B3ABA4",.12,so=.6)
    D.line(hls([(28.1,3.4),(28.1,4),(27.8,4.7)]),"#0A090A",.18)
    D.line(hls(MOUTH),"#111012",.25)
    for k in range(15):
        p=hl(rr.uniform(21.2,25.2),rr.uniform(3.1,5.3))
        shape(ellipse_pts(*p,.055,.055,0,6),"#6A6663",fo=.55)
    eyes()
    ear("ear-near")
    if VARIANTE!="agneau":
        horn()
    # Peau noire entre les racines : le départ plat de la corne rentre dans
    # le crâne au lieu de finir par une tranche de ruban apparente.
    socket=region([(105.7,95.7),(108.3,94.4),(110.2,91.3),(108.8,89.1),(106.9,89.3),(105.9,92.3)])
    D.el(geom(socket.intersection(HP)),fill=g)
    D.pivot("head",*POLL)
    D.pivot("muzzle",*hl(28.15,5.4))
    D.close()


def articulate(svg):
    parser=ET.XMLParser(target=ET.TreeBuilder(insert_comments=True))
    root=ET.fromstring(svg,parser=parser);paints=A.Paints(root)
    for name,_,front,_ in LEGS:
        joint,half=A.leg_joint(svg,name,OY-(CARPE if front else JARRET))
        A.split_leg(root,paints,name,joint,half+.4)
    pad=hls([(23.5,4.5),(24,3.7),(26,3.45),(27.5,3.8),(27.7,4.2),(26.5,4.7),(24,5.2)])
    gum=[ET.Element(A.Q+"path",{"d":cr(pad),"fill":"#B78483"})]
    A.lift_part(root,paints,"head",[S(*p) for p in hls(LIP_L)],"lip",S(*hl(23.5,4.6)),under=gum)
    ET.indent(root,space=" ")
    return ET.tostring(root,encoding="unicode")+"\n"


def main(path):
    D.lin("laine-fibres",(25,90),(25,20),[(0,"#EEEAE1"),(.3,"#DDD6CA"),(.65,"#CEC4B3"),(1,"#AA9B82")])
    for item in LEGS[:2]:leg(*item)
    tail()
    for item in LEGS[2:]:leg(*item)
    neck();body();head()
    D.pivot("ground",44,0)
    svg=('<svg xmlns="http://www.w3.org/2000/svg" viewBox="-13 0 154 108" width="154" height="108">\n'
         '<!-- Scottish Blackface Lanark, bélier de quatre ans ; toison naturelle avant tonte.\n'
         '1 unité = 1 cm ; garrot osseux 70 cm ; dessin original d’après docs/races/scottish-blackface.md.\n'
         '© 2026 Alejandro San Martin Lamas — MIT OR Apache-2.0 -->\n'
         '<defs>'+''.join(D.defs)+'</defs>\n'+'\n'.join(D.out)+'\n</svg>\n')
    svg=articulate(svg)
    if VARIANTE:
        return variante(path, svg)
    with open(path,"w",encoding="utf-8") as f:f.write(svg)
    print(f"{path} : {len(svg.encode())/1024:.0f} Ko, {svg.count('<path')} tracés")


def variante(path, svg):
    """Les autres dessins : ce qui change, puis les proportions."""
    root = ET.fromstring(svg)
    pass
    variantes.ecrire(path, root, **PROPORTIONS[VARIANTE])


if __name__=="__main__":
    main(sys.argv[1] if len(sys.argv)>1 else "scottish-blackface.svg")
