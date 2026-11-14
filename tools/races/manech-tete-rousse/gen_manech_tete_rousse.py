#!/usr/bin/env python3
"""Un bélier Manech tête rousse adulte en pleine toison, d'après nature.

Voir docs/races/manech-tete-rousse.md : 65 cm au garrot, tête de 23 cm,
toison jarreuse de fin de printemps et une paire de cornes ambrées. Les
photos servent de référence, aucun fragment de photographie n'est incorporé.
Repère en centimètres : x vers le nez, h vers le haut, sol à h = 0.

Usage : python3 tools/races/manech-tete-rousse/gen_manech_tete_rousse.py
        pets/manech-tete-rousse/manech-tete-rousse.svg

© 2026 Alejandro San Martin Lamas — MIT OR Apache-2.0
"""
import math
from pathlib import Path
import random
import sys
import xml.etree.ElementTree as ET

HERE = Path(__file__).resolve().parent
sys.path.insert(0, str(HERE))
sys.path.insert(0, str(HERE.parent))
import articulations as A  # noqa: E402
import variantes  # noqa: E402

# Les autres dessins de la race (tools/races/variantes.py), d'après la fiche.
# Sans variante : le bélier cornu.
VARIANTE = sys.argv[2] if len(sys.argv) > 2 else ""
PROPORTIONS = {
    "brebis": dict(  # Brebis Manech tête rousse : 60 cm au garrot, sans cornes ni toupet, tête longue et étroite, encolure fine, mamelle globuleuse.
        garrot=(65, 60), encolure=(1.0, .86), tete=(1.0, .92)),
    "agneau": dict(  # Agneau Manech tête rousse de deux semaines : 36 cm au garrot, tête et pattes rousses, sans cornes.
        variantes.AGNEAU, garrot=(65, 36), corps=(.84, .8)),
}
from outils import Doc, OY, S, cr, dense, ellipse_pts, inside, mix, num, poly, quad, shade  # noqa: E402
from shapely.geometry import Polygon  # noqa: E402
from toison import body, neck  # noqa: E402

GARROT = 65.0
NUQUE = (92.0, 79.0)
ANGLE = math.radians(60.0)
HU = (math.cos(ANGLE), -math.sin(ANGLE))
HV = (-math.sin(ANGLE), -math.cos(ANGLE))
CARPE, JARRET = 20.5, 25.0
FACE, FACE_LT, FACE_SH = "#CDA272", "#E6C893", "#9A764E"
FRONT, NEZ, NEZ_SH = "#B5834F", "#9C8277", "#695B54"
CORNE, CORNE_LT, CORNE_SH = "#9A7C60", "#B99D7F", "#6F543E"
D = Doc()


def HL(u, v):
    """De la nuque vers le nez, v vers la mâchoire."""
    return (NUQUE[0] + u * HU[0] + v * HV[0], NUQUE[1] + u * HU[1] + v * HV[1])


def HLs(points):
    return [HL(*p) for p in points]


def geometry(g):
    """Écrit les intersections en vrais contours, sans découpe SVG."""
    if g.is_empty:
        return ""
    if g.geom_type == "Polygon":
        return poly(list(g.exterior.coords)) + "".join(poly(list(r.coords)) for r in g.interiors)
    return "".join(geometry(q) for q in g.geoms if q.geom_type in ("Polygon", "MultiPolygon"))


# Le front ne fait qu'une légère convexité ; pas le gros nez d'une
# Basco-béarnaise. La ganache est étroite et le menton en retrait du nez.
DESSUS = [(-1.6, 1.0), (-1.4, -1.0), (0, -2.3), (2.8, -3.0), (5.7, -3.1),
          (8.5, -2.95), (11.5, -2.65), (14.7, -2.2), (17.5, -1.6),
          (20, -0.7), (21.7, 0.2), (22.7, 1.1), (23.0, 2.0), (22.7, 2.9)]
BOUCHE = [(22.7, 2.9), (21.5, 3.4), (20.0, 3.7), (18.4, 4.05)]
DESSOUS = [(18.4, 4.05), (17.3, 5.4), (15.5, 6.7), (12.0, 7.8),
           (8.6, 8.35), (5.6, 7.7), (2.4, 5.8), (0, 3.9), (-1.6, 1)]
CONTOUR = DESSUS + BOUCHE[1:] + DESSOUS[1:-1]
FACE_POLY = Polygon(dense(HLs(CONTOUR), True, 8)).buffer(0)
OEIL = HL(7.8, 0.6)
FENETRE = [(OEIL[0]-1.48, OEIL[1]-.04), (OEIL[0]-1.03, OEIL[1]+.72),
           (OEIL[0], OEIL[1]+.94), (OEIL[0]+1.03, OEIL[1]+.67),
           (OEIL[0]+1.48, OEIL[1]-.03), (OEIL[0]+.7, OEIL[1]-.52),
           (OEIL[0]-.5, OEIL[1]-.59)]
FENETRE = [(OEIL[0]+.84*(x-OEIL[0]), OEIL[1]+.84*(h-OEIL[1])) for x,h in FENETRE]
EYE_POLY = Polygon(dense(FENETRE, True, 8)).buffer(0)
LIP_L = [(19.2, 1.6), (21.1, 1.25), (22.8, 1.5), (23.4, 2.3),
         (23.1, 3.0), (21.7, 3.6), (19.4, 4.0), (18.0, 4.3), (18.7, 2.6)]


def soft(gid, local, rx, ry, color, opacity):
    """Un modelé doux, rogné au contour par géométrie (aucun masque)."""
    c = HL(*local)
    grad = D.ell(gid, c, rx, ry, ANGLE, [(0, color, opacity), (.45, color, opacity*.7), (1, color, 0)])
    shape = Polygon(ellipse_pts(*c, rx*1.05, ry*1.05, -ANGLE, 48))
    D.el(geometry(shape.intersection(FACE_POLY).difference(EYE_POLY)), fill=grad, rule="evenodd")


def eyes():
    D.open("eyes")
    D.el(cr(FENETRE), fill="#6D5138")
    D.open("pupils")
    x, h = OEIL
    g = D.rad("iris", (x, h), 1.04, [(0,"#B07A3E"), (.58,"#A47037"), (.88,"#8C5E2E"), (1,"#604023")])
    D.el(cr(ellipse_pts(x,h,1.02,1.02,0,24)), fill=g)
    rays=[]
    for i in range(32):
        a=2*math.pi*i/32
        rays.append(cr([(x+.55*math.cos(a),h+.55*math.sin(a)),
                        (x+.98*math.cos(a+.04),h+.98*math.sin(a+.04))],False))
    D.el("".join(rays), stroke="#79542D",sw=.047,so=.6)
    X,Y=S(x-.64,h+.17)
    D.emit(f'<rect x="{num(X)}" y="{num(Y)}" width="1.28" height=".34" rx=".14" fill="#141110"/>')
    D.pivot("pupils",x,h)
    D.close()
    D.el(cr(ellipse_pts(x-.3,h+.3,.13,.085,0,12)),fill="#F3EDE2",fo=.78)
    D.pivot("eyes",x,h)
    D.close()


def face():
    g = D.lin("face", HL(9,-3),HL(9,8),[(0,FACE_LT),(.35,FACE),(.8,FACE_SH),(1,"#826544")])
    D.el(geometry(FACE_POLY.difference(EYE_POLY)),fill=g,rule="evenodd")
    soft("front",(3.6,.1),6.2,4.0,FRONT,.7)
    soft("tempe",(7.8,2.6),3.8,3.1,"#9D6E40",.45)
    soft("chanfrein",(14.5,-.8),7.4,1.6,"#F0D7AE",.68)
    soft("joue",(11.3,4.8),5.0,2.3,"#E7C59A",.55)
    # Le poil ras suit les os du chanfrein et s'épanouit sur la joue ; les
    # petits traits restent dans la peau, et dégagent le tour de l'œil.
    rng=random.Random(5304)
    paths={"#F3DDB8":[],"#8D613A":[],"#DAB584":[]}
    face_dense=dense(HLs(CONTOUR),True,5)
    for i in range(2100):
        u,v=rng.uniform(-1,23),rng.uniform(-3.2,7.7)
        du,dv=rng.uniform(.12,.35),rng.uniform(-.07,.10)
        a,b=HL(u,v),HL(u+du,v+dv)
        if not (inside(face_dense,*a) and inside(face_dense,*b)):
            continue
        if math.dist(a,OEIL)<2.1 or u>20.2:
            continue
        paths[list(paths)[i%3]].append(cr([a,b],False))
    for color,paths_ in paths.items():
        D.el("".join(paths_),stroke=color,sw=.038,so=.32,cap="round")
    # Le nez n'est pas noir : une peau grise rosée peu pigmentée, tachée.
    nose=HLs([(19.9,-.6),(21.8,.2),(22.7,1.1),(23,2),(22.7,2.9),(21.5,3.4),(20.2,3.6),(19.5,2)])
    g=D.lin("nez",HL(19.8,1),HL(22.6,2.3),[(0,"#BBA191",0),(.32,"#BBA191",.75),(.7,NEZ),(1,NEZ_SH)])
    D.el(cr(nose),fill=g)
    for i in range(70):
        u,v=rng.uniform(19.8,22.7),rng.uniform(.1,3.1)
        if inside([(19.9,-.6),(21.8,.2),(22.7,1.1),(23,2),(22.7,2.9),(21.5,3.4),(20.2,3.6),(19.5,2)],u,v):
            D.el(cr(ellipse_pts(*HL(u,v),rng.uniform(.025,.08),rng.uniform(.025,.065),0,6)),fill=NEZ_SH,fo=.38)
    nostril=HLs([(21.9,.75),(22.45,1.23),(22.35,1.65),(21.8,1.8),
                  (21.2,1.45),(20.65,.75),(20.25,.32),(20.85,.62),(21.5,.82)])
    D.el(cr(nostril),fill="#4D4139")
    D.line(HLs([(20.5,.48),(21.15,.85),(21.95,1.05)]),"#C4AAA0",.13,so=.85)
    D.line(HLs([(22.55,2.0),(22.45,2.7),(21.6,3.4)]),"#625149",.12,so=.8)
    D.line(HLs(BOUCHE),"#725645",.12,so=.8)
    # Bord des paupières brun roux, sans lunettes noires. Le bord est peint
    # après la face percée : l'iris mobile reste caché sous la paupière.
    D.el(cr(FENETRE),stroke="#88633F",sw=.11)
    D.line([(OEIL[0]-1.24,OEIL[1]),(OEIL[0]-.84,OEIL[1]+.65),
            (OEIL[0],OEIL[1]+.84),(OEIL[0]+.92,OEIL[1]+.56)],"#765337",.13)
    D.line([(OEIL[0]-1.09,OEIL[1]-.1),(OEIL[0]-.34,OEIL[1]-.56),
            (OEIL[0]+.67,OEIL[1]-.44)],"#E5C39A",.10,so=.5)
    for i in range(6):
        x=OEIL[0]-.8+i*.28
        h=OEIL[1]+.72-.16*abs(x-OEIL[0])
        D.line([(x,h),(x-.14,h+.27)],"#805638",.04)


def jaw():
    D.open("jaw")
    chin=HLs([(16.5,5),(18.6,4.1),(21.2,3.4),(22.7,2.9),
              (22.75,3.7),(22.15,4.55),(21.1,5.0),(19.5,5.1),(17.4,5.2),(15.6,6.6)])
    # L'intérieur existe sous la face et le menton, mais n'en déborde pas
    # bouche fermée ; la rotation de jaw le découvre en bâillant ou bêlant.
    covered=FACE_POLY.union(Polygon(dense(chin,True,8))).buffer(-.05)
    for points,color in [
        ([(12,5.8),(17.5,3.1),(21.5,2.4),(22.4,2.7),(21,4.3),(17.5,5.6),(14,7)],"#543935"),
        ([(17.3,4.3),(20.3,3.2),(22,3),(21.4,3.9),(18,5.1)],"#B4817C"),
        ([(21.1,3),(22.2,2.9),(22.1,3.3),(21.2,3.5)],"#E6D6BC"),
    ]:
        interior=Polygon(dense(HLs(points),True,8)).buffer(0)
        D.el(geometry(interior.intersection(covered)),fill=color)
    g=D.lin("menton",HL(20,3.3),HL(19,6),[(0,"#B49470"),(.5,FACE),(1,FACE_SH)])
    D.el(cr(chin),fill=g)
    D.line(HLs([(20.7,4.4),(21.6,4.1),(22.15,3.65)]),"#DEC9AE",.08,so=.4)
    D.pivot("jaw",*HL(15.0,5.6))
    D.close()


def ear(far=False):
    name="ear-far" if far else "ear-near"
    base=(90.1,78.6) if far else (90.5,76.9)
    length=6.9 if far else 8.6  # raccourcissement de profil de l'oreille de 10,5 cm
    def P(u,v):
        return (base[0]-u,base[1]-.17*u+v)
    contour=[(-.4,-.7),(.8,-1.1),(3,-1.7),(5.5,-1.45),(length,-.05),
             (length-.35,.43),(5.5,1.45),(3,1.6),(1,1.1),(-.4,.65)]
    D.open(name)
    k=.82 if far else 1
    g=D.lin(name+"-peau",P(3,1.5),P(3,-1.8),[(0,shade(FACE_LT,k)),(.5,shade(FACE,k)),(1,shade(FACE_SH,k))])
    D.el(cr([P(*p) for p in contour]),fill=g)
    if not far:
        D.el(cr([P(*p) for p in [(.6,-.2),(2.3,-.8),(5,-.8),(7.7,0),(5.3,.85),(2.5,.9),(.6,.4)]]),fill="#AD7D62")
        D.line([P(1,-.1),P(3.2,-.3),P(5.8,-.05),P(7.7,0)],"#876449",.18,so=.7)
        D.line([P(.5,1),P(2.9,1.45),P(5.4,1.25),P(7.8,.4)],"#E8CEAA",.17)
        rng=random.Random(5318)
        hairs=[]
        for _ in range(90):
            u,v=rng.uniform(.5,7),rng.uniform(-1.4,1.3)
            if inside(contour,u,v):
                hairs.append(cr([P(u,v),P(u+.38,v+.09)],False))
        D.el("".join(hairs),stroke="#EFDCBC",sw=.045,so=.65)
    D.pivot(name,*base)
    D.close()


def horn(far=False):
    """Une seule spire, ambrée, autour de l'oreille ; pointe écartée de la
    joue. L'autre corne n'apparaît que par un petit arc au-dessus de la nuque.
    Les anneaux suivent la section arrondie, avec crête éclairée et sillon.
    """
    axis=[(93,80),(89.8,82),(85.5,82),(81.3,79.3),(79.4,75),
          (80,70),(83,66.3),(87.4,64.9),(92.0,65.1),(96.0,66.2),(99.0,68.0)]
    widths=[5.7,5.4,4.9,4.2,3.6,3,2.6,2.1,1.55,.95,.04]
    C=dense(axis,False,12)
    N,W=[],[]
    for i,c in enumerate(C):
        a,b=C[max(0,i-1)],C[min(len(C)-1,i+1)]
        dx,dy=b[0]-a[0],b[1]-a[1]
        length=math.hypot(dx,dy)
        N.append((-dy/length,dx/length))
        t=i/(len(C)-1)*(len(widths)-1)
        j=min(int(t),len(widths)-2)
        W.append(widths[j]*(1-(t-j))+widths[j+1]*(t-j))
    if far:
        C=[(x-.5,h+.55) for x,h in C[:37]]
        N,W=N[:37],W[:37]
    left=[(p[0]+n[0]*w/2,p[1]+n[1]*w/2) for p,n,w in zip(C,N,W)]
    right=[(p[0]-n[0]*w/2,p[1]-n[1]*w/2) for p,n,w in zip(C,N,W)]
    shape=left+right[::-1]
    if not far:
        # Son ombre sur la joue suggère que la pointe est écartée du crâne.
        shadow=Polygon([(x+.65,h-.7) for x,h in shape]).buffer(0)
        D.el(geometry(shadow.intersection(FACE_POLY).difference(EYE_POLY)),fill="#705137",fo=.16)
    k=.82 if far else 1
    gid="corne-loin" if far else "corne"
    g=D.rad(gid,(87,75),13,[(0,shade(CORNE_SH,k)),(.45,shade(CORNE_SH,k)),(.74,shade(CORNE,k)),(1,shade(CORNE_LT,k))])
    D.el(cr(shape,t=.7),fill=g)
    # Une arête plus claire sur la face externe, le creux près du crâne.
    for fraction,color,alpha in [(.27,CORNE_LT,.55),(-.32,"#5B3F2A",.28)]:
        strip=[(p[0]+n[0]*w*fraction,p[1]+n[1]*w*fraction) for p,n,w in zip(C,N,W)]
        edge=left if fraction>0 else right
        D.el(cr(edge+strip[::-1],t=.7),fill=shade(color,k),fo=alpha)
    rng=random.Random(5330)
    acc=[0]
    for i in range(1,len(C)):
        acc.append(acc[-1]+math.dist(C[i],C[i-1]))
    dark,light=[],[]
    s=.4
    while s<acc[-1]-3:
        i=next(i for i,a in enumerate(acc) if a>=s)
        p,n,w=C[i],N[i],W[i]
        a,b=left[i],right[i]
        tangent=(n[1],-n[0])
        bend=.22+rng.uniform(-.08,.08)
        c=(p[0]+tangent[0]*bend,p[1]+tangent[1]*bend)
        ring=[]
        for j in range(7):
            t=j/6
            ripple=math.sin(math.pi*t)*bend+.07*math.sin(t*11+s)
            ring.append((a[0]+(b[0]-a[0])*t+tangent[0]*ripple,
                         a[1]+(b[1]-a[1])*t+tangent[1]*ripple))
        dark.append(cr(ring,False,nd=2,t=.75))
        shift=.15
        aa=(a[0]+tangent[0]*shift,a[1]+tangent[1]*shift)
        bb=(p[0]-n[0]*w*.25+tangent[0]*shift,p[1]-n[1]*w*.25+tangent[1]*shift)
        light.append(quad(aa,(c[0]+tangent[0]*shift,c[1]+tangent[1]*shift),bb,nd=2))
        s+=rng.uniform(.43,.65)
    D.el("".join(dark),stroke=shade("#5B3F2A",k),sw=.115,so=.64)
    D.el("".join(light),stroke=shade("#D0B497",k),sw=.09,so=.64)
    for frac in [-.25,.05,.28]:
        D.line([(p[0]+n[0]*w*frac,p[1]+n[1]*w*frac) for p,n,w in zip(C[2::3],N[2::3],W[2::3])],
               shade("#71543D",k),.08,so=.5)
    D.el(cr(shape,t=.7),stroke=shade("#65482F",k),sw=.13,so=.7)


def toupet():
    rng=random.Random(5307)
    # Petite calotte irrégulière entre les deux bases ; sa laine s'effiloche
    # sur le poil roux, sans bouquets de boucles posés sur le chanfrein.
    crown=[(-2.4,-2.5),(-2.5,-3.25),(-1.6,-3.9),(-.6,-4.15),(.5,-3.9),
           (1.5,-3.5),(2.6,-2.9),(2.7,-1.95),(1.8,-1.9),(.6,-2.4),(-.8,-2.2)]
    D.el(cr(HLs(crown)),fill="#D9CDAE")
    for _ in range(110):
        u,v=rng.uniform(-2.8,3),rng.uniform(-4.3,-1.8)
        if not inside(crown,u,v):
            continue
        width=rng.uniform(.12,.3)
        length=rng.uniform(.35,.8)
        p=[(u-width,v),(u-width*.6,v-length*.6),(u+.1,v-length),
           (u+width,v-length*.3),(u+width*.4,v+.15),(u,v+.28)]
        D.el(cr(HLs(p)),fill=mix("#C1B092","#F1E4C9",rng.uniform(.25,1)))
        D.line(HLs([(u-width*.4,v),(u,v-length*.7),(u+width*.5,v-length*.8)]),
               "#A18B6C",.035,so=.45)


def head():
    D.open("head")
    ear(True)
    if not VARIANTE:
        horn(True)
    jaw()
    eyes()
    face()
    if not VARIANTE:
        horn()
    ear()
    if VARIANTE != "brebis":
        toupet()
    D.pivot("head",*NUQUE)
    D.pivot("muzzle",*HL(22.45,3.85))
    D.close()


def leg(x,front,near):
    name=f'leg-{"front" if front else "back"}-{"near" if near else "far"}'
    k=1 if near else .82
    D.open(name)
    if front:
        contour=[(x-2.7,45),(x+2.8,45),(x+2.1,28),(x+2.25,22),
                 (x+2,19),(x+1.35,15),(x+1.35,8),(x+2.6,3.2),
                 (x-.2,3.2),(x-1.5,7),(x-1.2,15),(x-1.7,19),(x-2.2,21),(x-2,25)]
        pivot=(x,43)
    else:
        contour=[(x+2,51),(x+10,51),(x+7,38),(x+2.2,28),
                 (x+1.45,23),(x+.5,14),(x+.8,8),(x+2.8,3.2),
                 (x,3.2),(x-1.3,7),(x-1.1,14),(x-1.4,22),(x-2.6,25),
                 (x-2.4,27),(x-.8,33)]
        pivot=(x+5,48)
    g=D.lin(name+"-poil",(x-2,0),(x+3,0),[(0,shade("#9A764E",k)),(.3,shade("#D4AE7B",k)),
                                           (.7,shade("#EBCB95",k)),(1,shade("#AD8050",k))])
    D.el(cr(contour,t=.55),fill=g)
    # Poil court roux, le canon plus soutenu devant et le tendon lisible.
    rng=random.Random(round(x*50))
    hairs=[]
    for _ in range(160):
        xx,h=x+rng.uniform(-1.9,1.9),rng.uniform(5,29)
        if inside(contour,xx,h) and inside(contour,xx+.05,h-.45):
            hairs.append(cr([(xx,h),(xx+.05,h-.45)],False))
    D.el("".join(hairs),stroke=shade("#F1D6A9",k),sw=.045,so=.7)
    joint=CARPE if front else JARRET
    D.line([(x-1,9),(x-.8,16),(x-1.1,joint-2),(x-1.65,joint)],shade("#8E6845",k),.22,so=.6)
    if front:
        D.line([(x-.7,19.4),(x+.2,19.1),(x+1.6,19.5)],shade("#A1784D",k),.12,so=.6)
    # Petits ergots et deux onglons serrés, clairs et striés de brun.
    D.el(cr([(x-1.3,6.6),(x-1.95,6.3),(x-1.7,5.2),(x-1,5.7)]),fill=shade("#95724F",k))
    hoof=[(x-.6,3.8),(x+2.5,3.7),(x+3.7,1.6),(x+4,0.12),(x+3.3,0),
          (x-.9,0),(x-1.2,.5),(x-.9,2.2)]
    g=D.lin(name+"-onglon",(x,4),(x+1,0),[(0,shade("#CDA676",k)),(.55,shade("#B78C58",k)),(1,shade("#74583F",k))])
    D.el(cr(hoof,t=.4),fill=g)
    D.line([(x+1.5,3.2),(x+2.2,1.5),(x+2.3,.16)],shade("#74583F",k),.15)
    for i in range(7):
        h=.55+i*.37
        D.line([(x-.9,h),(x+.4,h+.12),(x+2.6,h+.05)],shade("#765739",k),.055,so=.5)
    D.pivot(name,*pivot)
    D.close()


def tail():
    # La queue courte est presque entièrement cachée par la culotte.
    D.open("tail")
    g=D.lin("queue",(1,62),(-2,52),[(0,"#D6C6A3"),(1,"#A99470")])
    D.el(cr([(3,65),(1.5,64),(.4,60),(.2,56),(1,54.5),(2.5,55.5),(3.5,59),(4,63)]),fill=g)
    D.pivot("tail",1,63)
    D.close()


def articulate(text):
    root=ET.fromstring(text)
    paints=A.Paints(root)
    for name,h in [("leg-front-near",CARPE),("leg-front-far",CARPE),
                   ("leg-back-near",JARRET),("leg-back-far",JARRET)]:
        joint,half=A.leg_joint(text,name,OY-h)
        A.split_leg(root,paints,name,joint,half+.35,(0,1))
    lip_shape=Polygon(dense(HLs(LIP_L),True,8)).buffer(-.14)
    gum_shape=Polygon(dense(HLs([(18.6,2.1),(20,1.9),(22,2.1),(22.6,2.6),
                                (21.7,3.0),(19.8,3.4),(18.4,3.6)]),True,8))
    gum=ET.Element(A.Q+"path",{"d":geometry(lip_shape.intersection(gum_shape)),"fill":"#C9938C"})
    A.lift_part(root,paints,"head",[S(*p) for p in HLs(LIP_L)],"lip",S(*HL(19,2.6)),under=[gum])
    ET.indent(root,space=" ")
    return ET.tostring(root,encoding="unicode")+"\n"


def main(path):
    leg(19,False,False)
    leg(64,True,False)
    tail()
    leg(12,False,True)
    leg(71,True,True)
    neck(D)
    body(D)
    head()
    D.pivot("ground",43,0)
    text=('<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 125 98" width="125" height="98">\n'
          '<!-- Manech tête rousse : bélier cornu de 3 à 4 ans, 65 cm au garrot ; 1 unité = 1 cm.\n'
          'Dessin original écrit par tools/races/manech-tete-rousse/gen_manech_tete_rousse.py.\n'
          '© 2026 Alejandro San Martin Lamas — MIT OR Apache-2.0 -->\n'
          '<defs>'+''.join(D.defs)+'</defs>\n'+'\n'.join(D.out)+'\n</svg>\n')
    svg=articulate(text)
    if VARIANTE:
        return variante(path, svg)
    Path(path).parent.mkdir(parents=True,exist_ok=True)
    Path(path).write_text(svg,encoding="utf-8")
    print(f"{path} : {len(svg.encode())/1024:.0f} Ko, {svg.count('<path')} tracés")


def variante(path, svg):
    """Les autres dessins : ce qui change, puis les proportions."""
    root = ET.fromstring(svg)
    if VARIANTE == "brebis":
        # La mamelle « très développée, globuleuse » de la laitière basque.
        variantes.mamelle(root, *variantes.sous_le_ventre(root, 10, -5), largeur=11, hauteur=9,
                  peau="#E0B5A2", ombre="#B08572")
    variantes.ecrire(path, root, **PROPORTIONS[VARIANTE])


if __name__=="__main__":
    main(sys.argv[1] if len(sys.argv)>1 else "manech-tete-rousse.svg")
