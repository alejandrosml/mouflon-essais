#!/usr/bin/env python3
"""Une brebis Heidschnucke grise cornue de quatre ans, en novembre.

D'après docs/races/heidschnucke.md : garrot osseux 67 cm, toison de cinq
mois, cornes en faucille (pas les spirales du bélier). Repère en cm : x vers
la droite, h vers le haut ; S les transforme en coordonnées SVG. Les fibres
sont peintes, jamais des photos. Graine fixe, aucun masque ni filtre.

Usage : python3 tools/races/heidschnucke/gen_heidschnucke.py [sortie.svg]
© 2026 Alejandro San Martin Lamas — MIT OR Apache-2.0
"""
import math
from pathlib import Path
import random
import sys
import xml.etree.ElementTree as ET

HERE = Path(__file__).resolve().parent
sys.path.insert(0, str(HERE.parent))
import articulations as A
import variantes  # noqa: E402

# Les autres dessins de la race (tools/races/variantes.py), d'après la fiche.
# Sans variante : la brebis.
VARIANTE = sys.argv[2] if len(sys.argv) > 2 else ""
PROPORTIONS = {
    "belier": dict(  # Bélier Heidschnucke : 72 cm au garrot, cornes massives en escargot écartées de la tête, encolure épaisse, tête plus forte.
        garrot=(67, 72), encolure=(1.0, 1.22), tete=(1.05, 1.08)),
    "agneau": dict(  # Agneau Heidschnucke de deux semaines : 40 cm au garrot, né noir en laine bouclée, sans cornes.
        variantes.AGNEAU, garrot=(67, 40), corps=(.84, .8)),
}
from shapely.geometry import Point, Polygon
from outils import Doc, OY, S, cr, dense, mix, poly, shade

D = Doc()
R = random.Random(1967)
POLL = (96.0, 91.0)
ANGLE = math.radians(36)
NECK_PIVOT = (78.0, 49.0)
CARPUS, HOCK = 15.5, 23.0


def hl(u, v):
    """Repère de la tête : u vers le nez, v sous la joue."""
    c, s = math.cos(ANGLE), math.sin(ANGLE)
    return (POLL[0] + c*u - s*v, POLL[1] - s*u - c*v)


def shape(pts, fill, **kw):
    D.el(cr(pts), fill=fill, **kw)


def local(pts, fill, **kw):
    shape([hl(*p) for p in pts], fill, **kw)


def line(pts, color, width, alpha=1):
    D.line(pts, color, width, so=alpha)


def oval(c, rx, ry, fill, **kw):
    shape([(c[0]+rx*math.cos(k*math.tau/24), c[1]+ry*math.sin(k*math.tau/24))
           for k in range(24)], fill, **kw)


def region(pts):
    return Polygon(dense(pts, True, per=6)).buffer(0)


def hairs(area, count, flow, colors, length, width=0.07):
    """Poils en courbes très fines, contenus dans la pièce qu'ils modèlent."""
    x0, h0, x1, h1 = area.bounds
    paths = {c: [] for c in colors}
    for _ in range(count):
        x, h = R.uniform(x0, x1), R.uniform(h0, h1)
        if not area.contains(Point(x, h)):
            continue
        dx, dh = flow(x, h)
        L = R.uniform(*length)
        pts = [(x, h), (x+dx*L*.45+R.uniform(-.14, .14), h+dh*L*.45),
               (x+dx*L, h+dh*L)]
        if area.contains(Point(*pts[-1])):
            paths[R.choice(colors)].append(cr(pts, closed=False))
    for color, paths_ in paths.items():
        D.el("".join(paths_), stroke=color, sw=width, so=.5, cap="round")


# Un tronc profond, long de 92 cm dans sa toison ; frange vers 28–30 cm.
# L'épaule arrondie recouvre une encolure noire complète (pas un col coupé).
BODY = [(0, 61), (3, 68), (10, 72), (22, 74), (40, 74), (58, 73.8),
        (71, 74.5), (77, 74), (82, 70), (86, 61), (88, 49), (87, 38),
        (82, 32), (73, 31), (65, 30), (52, 29), (37, 29), (24, 30),
        (15, 29), (6, 30), (1, 37), (-1, 48)]
BODY_AREA = region(BODY)
# Le bord caché commence bas derrière le pivot. En descendant brouter,
# il vient sous l'arrondi de l'épaule plutôt que de former une bosse devant.
NECK = [(62, 56), (68, 66), (79, 78), (87, 85), (94, 92),
        hl(3, -1), hl(7, 5), hl(7, 7), (99, 75), (97, 67),
        (94, 57), (91, 45), (86, 33), (80, 32), (73, 39), (67, 48)]


def cape(x, h):
    """Le noir avance en diagonale : garrot noir, gris jusque sur le bras."""
    edge = 77 - (h-35)*.31
    return max(0, min(1, (x-edge+5)/15))


def wool_color(x, h, variation=0):
    lit = max(0, min(1, (h-25)/52))
    gray = mix("#656164", "#C9C5BF", .2+.8*lit)
    gray = shade(gray, 1+variation)
    # § 4.1 et détail 8 : une ligne sombre sous le ventre prolonge la
    # bavette ; les mèches du bas du flanc noircissent vers la frange.
    belly = max(0, min(1, (40-h)/8))
    gray = mix(gray, "#28272D", .8*belly*belly*(3-2*belly))
    return mix(gray, mix("#111115", "#35343A", lit), cape(x, h))


def lock(x, h, length, width, black=False):
    """Une mèche sans boucle : quelques jarres filent jusqu'à sa pointe.

    Pas de rang de festons : racines aléatoires, contours sans bord peint,
    pointe divisée et stries de longueurs différentes qui se chevauchent.
    """
    lean = R.uniform(-2.2, .6) + .7*math.sin(x*.13)
    bend = R.uniform(-1.05, 1.05)
    def p(t, f=0):
        return (x+lean*t+bend*math.sin(t*math.pi)+f*width*(1-t*.8), h-length*t)
    base = mix("#222126", "#45434A", R.random()*.55) if black else wool_color(x, h, R.uniform(-.13,.08))
    # Le corps de la mèche se dissout dans les voisins ; sa pointe reste fine.
    shape([p(0,0), p(.36,-.38), p(.78,-.24), p(1,-.05),
           p(.94,.06), p(.99,.14), p(.67,.32), p(.18,.24)], base, fo=.65)
    for _ in range(R.randint(7, 12)):
        f = R.uniform(-.4,.4)
        start, end = R.uniform(0,.3), R.uniform(.72,1.08)
        pts = [p(start,f), p((start+end)/2,f+R.uniform(-.08,.08)), p(end,f*.45)]
        if black:
            color = R.choice(["#100F14", "#48454A", "#343138"])
        else:
            color = mix(wool_color(x,h-length*.45),
                        R.choice(["#E2DED6", "#BEBAB6", "#4A484C", "#343337"]), R.uniform(.2,.65))
            color = mix(color, "#202026", cape(x,h)*.94)
        line(pts, color, R.uniform(.035,.085), R.uniform(.35,.7))


def fleece():
    D.open("body")
    fill = D.lin("toison-fond", (30,77), (35,25),
                 [(0,"#B4B0AA"),(.38,"#969296"),(.8,"#504D53"),(1,"#1B191F")])
    # Le fond lui-même est effiloché. Les fibres qui dépassent ne doivent
    # pas seulement décorer le bord d'un rectangle intact.
    edge=[]
    for x,h in dense(BODY,True,per=10):
        dx,dh=x-43,h-52
        L=math.hypot(dx,dh)
        fuzz=R.uniform(-.25,.65)
        edge.append((x+dx/L*fuzz,h+dh/L*fuzz))
    D.el(poly(edge),fill=fill)
    shade_ = D.lin("epaule-noire", (63,68), (86,53),
                  [(0,"#17161C",0),(.5,"#17161C",.5),(1,"#17161C")])
    D.el(poly(edge),fill=shade_)
    # Le duvet est traversé de jarres ; même entre les grandes mèches,
    # aucun aplat lisse. Regroupement des fibres de même peinture en tracés
    # communs, sans augmenter le nombre de morceaux cuits.
    fibers={}
    for _ in range(12000):
        x,h=R.uniform(-1,88),R.uniform(30,76)
        if not BODY_AREA.contains(Point(x,h)):
            continue
        L=R.uniform(2.5,8)
        dx=R.uniform(-1.6,.8)
        end=(x+dx,h-L)
        if not BODY_AREA.contains(Point(*end)):
            continue
        v=R.choice([-.35,-.17,.04,.16])
        # Palette discrète : les traits voisins se fondent à petite taille.
        col=wool_color(round(x/3)*3,round(h/4)*4,v)
        fibers.setdefault(col,[]).append(cr([(x,h),(x+dx*.4+.3*math.sin(x),h-L*.5),end],False))
    for col,paths in fibers.items():
        D.el("".join(paths),stroke=col,sw=.075,so=.7,cap="round")
    # De vraies nappes longitudinales, pas des boucles superposées : on
    # travaille du ventre au dos, sans grille ni rangées perceptibles.
    roots = []
    for _ in range(950):
        x, h = R.uniform(-1,88), R.uniform(32,77)
        if BODY_AREA.contains(Point(x,h)):
            roots.append((x,h))
    for x,h in sorted(roots, key=lambda p:p[1]):
        L = R.uniform(7,15)
        if h < 40:
            L = min(L, h-27+R.uniform(-1,2))
        lock(x,h,L,R.uniform(1.5,3.4))
    # Au sommet, les racines des mèches se confondent avec le dos et
    # retombent sur le flanc. Leur début n'est pas un trait de contour.
    for _ in range(135):
        x=R.uniform(1,83)
        cut=BODY_AREA.intersection(Polygon([(x-.05,20),(x+.05,20),(x+.05,80),(x-.05,80)]))
        if not cut.is_empty:
            h=cut.bounds[3]-.25
            lock(x,h,R.uniform(7,13),R.uniform(1.1,2.8))
    # Les poils du dos percent la silhouette ; jamais une bordure lisse.
    for _ in range(130):
        x = R.uniform(3,77)
        h = 73.4 + .7*math.sin(x*.12)
        if BODY_AREA.contains(Point(x,h-1)):
            line([(x,h-1.5),(x-.6,h+.2),(x+R.uniform(-1.1,-.3),h+R.uniform(.3,1.4))],
                 wool_color(x,h), R.uniform(.05,.11), .85)
    D.pivot("body",43,30)
    D.close()


def sways():
    # Trois pans de la frange sous le ventre, racines cachées dans la nappe.
    for i,(x0,x1) in enumerate([(23,38),(38,53),(53,68)],1):
        D.open(f"sway-{i}")
        for _ in range(42):
            x = R.uniform(x0,x1)
            lock(x,R.uniform(34,39),R.uniform(8,11),R.uniform(.6,1.5),True)
        D.pivot(f"sway-{i}",(x0+x1)/2,38)
        D.close()


def neck():
    D.open("neck")
    fill = D.lin("cape", (80,83),(96,48),[(0,"#303037"),(.5,"#1C1B21"),(1,"#101015")])
    shape(NECK,fill)
    for i,(x,h) in enumerate(dense(NECK,True,per=7)):
        if x>79 and 36<h<90 and i%2==0:
            lock(x-.25,h+.3,R.uniform(1.5,3.5),R.uniform(.6,1.3),True)
    hairs(region(NECK),2300,lambda x,h:(-.13,-1),
          ["#101014","#444148","#59555A"],(1.5,4.5),.065)
    for _ in range(100):
        x,h = R.uniform(80,94),R.uniform(38,85)
        if region(NECK).contains(Point(x,h)):
            lock(x,h,R.uniform(3,7),R.uniform(.5,1.1),True)
    D.pivot("neck",*NECK_PIVOT)
    D.close()


def leg(name,x,back=False,far=False):
    D.open(name)
    # Grasset et coude sont cachés ; jarret sec tourné en arrière.
    if back:
        pts=[(x-5,47),(x+5,46),(x+7,37),(x+2,28),(x-2,23),
             (x-2.2,9),(x-1,5),(x+1.6,2),(x+1.8,.4),(x-4.8,0),
             (x-6,1.3),(x-5,5),(x-5,11),(x-5,23),(x-2,31),(x-6,39)]
        jx=x-3.6
    else:
        pts=[(x-3.5,47),(x+4,46),(x+3.4,33),(x+1.3,23),(x+1.4,16),
             (x+.6,7),(x+2,4),(x+4.1,1),(x+3.7,.1),(x-2.7,0),
             (x-3.2,1.8),(x-2.6,6),(x-2.6,16),(x-3.5,23),(x-4.3,35)]
        jx=x-.6
    base=D.lin(name+"-poil",(x-4,20),(x+3,20),
               [(0,shade("#15151B",.8 if far else 1)),(.45,"#303039" if not far else "#202027"),(1,"#101015")])
    shape(pts,base)
    hairs(region(pts),550,lambda a,b:(.04,-1),["#55525B","#141319"],(.4,1.3),.045)
    # Deux onglons compacts et l'ergot, plutôt qu'une grosse chaussure.
    toe=x+1.8 if back else x+4
    heel=x-5 if back else x-2.7
    shape([(heel,3.1),(toe-2,3.4),(toe,1.5),(toe-.2,.1),
           (heel+.3,0),(heel-.5,1.1)],"#222120" if not far else "#141315")
    line([(heel+.4,2.7),(toe-2.2,3),(toe-.7,1.5)],"#5A5650",.17,.8)
    line([(toe-1.4,2),(toe-1.5,.3)],"#0D0C10",.2)
    oval((heel-.2,5.7),.55,.9,"#242126")
    joint_h=HOCK if back else CARPUS
    oval((jx,joint_h),1.65 if back else 1.8,2.1,"#25242C",fo=.5)
    D.pivot(name,x,43)
    D.close()


def tail():
    D.open("tail")
    pts=[(1,64),(-2,64),(-3.7,59),(-4.6,53),(-4.4,50),
         (-3.1,49.3),(-1.8,51),(-.7,56),(1.5,61)]
    shape(pts,D.lin("queue",(-4,57),(1,57),[(0,"#0E0E13"),(.6,"#39363C"),(1,"#17161B")]))
    hairs(region(pts),300,lambda x,h:(-.1,-1),["#15141A","#555059"],(.5,1.3))
    D.pivot("tail",0,63)
    D.close()


def horn(far=False):
    # La faucille de P1 (§ 6.2) : monte de 3 cm à 65°, s'incline aussitôt
    # en arrière au-dessus de l'oreille et redescend ; corde de 17,5 cm,
    # arc d'environ 130°, pointe en arrière et en bas, à hauteur de la base
    # de l'oreille, à quelques centimètres derrière elle.
    axis=[(97.8,91.9),(96.7,94.2),(94.6,95.8),(91.6,96.5),(88.4,96.3),
          (85.5,95.2),(83.1,93.4),(81.4,91.3),(80.5,89.4)]
    gros=1.6
    if VARIANTE=="belier":
        # L'escargot du bélier (§ 1.4) : un tour et un huitième autour de
        # l'oreille, massif à la base, la pointe ressortie vers l'avant.
        axis=variantes.spirale((97.8,91.9),(91.6,87.6),1.13,5.0,n=30)
        gros=3.5
    if far:
        axis=[(x+.9,h+.7) for x,h in axis]
    C=dense(axis,False,per=8)
    sides=[[],[]]
    normal=[]
    for i,p in enumerate(C):
        a,b=C[max(0,i-1)],C[min(len(C)-1,i+1)]
        L=math.dist(a,b)
        n=(-(b[1]-a[1])/L,(b[0]-a[0])/L)
        w=gros*(1-i/(len(C)-1))**.7+.025
        normal.append((n,w))
        for side,s in enumerate([-1,1]):
            sides[side].append((p[0]+s*n[0]*w,p[1]+s*n[1]*w))
    shape(sides[0]+sides[1][::-1],D.lin("corne-loin" if far else "corne",
          (86,98),(86,88.5),[(0,"#AAA7A2" if not far else "#767579"),(.35,"#7A7879"),(.8,"#4E4C51"),(1,"#343238")]))
    for i in range(2,len(C)-12,2):
        (nx,nh),w=normal[i]
        p=C[i]
        line([(p[0]-nx*w*.9,p[1]-nh*w*.9),
              (p[0]-.13,p[1]-.13),(p[0]+nx*w*.9,p[1]+nh*w*.9)],"#36343A",.065,.45)
        if not far:
            line([(p[0]-nx*w*.7,p[1]-nh*w*.7+.13),(p[0],p[1]+.13)],"#C0BCB5",.07,.65)
    if not far:
        line(sides[0][3:-6],"#343238",.12,.7)


def ear(far=False):
    name="ear-far" if far else "ear-near"
    D.open(name)
    # Une feuille courte de 8 cm, à peine relevée, sous la corne.
    pts=[(96,90),(93,92),(89,92.5),(87.5,91.3),(90,88.6),(94,88.5)] if not far else \
        [(98,91),(100.5,92.1),(103.5,92.7),(104.2,91.8),(101.3,90.3),(99,89.7)]
    shape(pts,"#1D1C23" if not far else "#111017")
    if not far:
        shape([(94.6,90.2),(91,91.4),(88.7,91.1),(91,89.7)],"#383139")
        line([(94,89.7),(91,89.6),(89,90.6)],"#56505B",.12,.7)
    D.pivot(name,*(pts[0]))
    D.close()


def head():
    D.open("head")
    if VARIANTE!="agneau":
        horn(True)
    ear(True)
    # Mâchoire sous la face ; l'intérieur ne paraît que bouche ouverte.
    D.open("jaw")
    local([(5,4),(11,3.8),(20.6,2.4),(23,3),(23.2,5),(21.6,6.2),
           (16,7.1),(10,7.4),(6,6.1)],"#111116")
    local([(13,4),(21.9,3.1),(22.9,4.5),(18,5.1),(13,5.6)],"#68424A")
    local([(12,5.7),(19,4.8),(22,4.7),(21.4,5.4),(17,6.2)],"#956771")
    local([(7,4.8),(15,4.45),(23,4.15),(22.8,5.5),(20.8,6.1),(12,8.3),(7,8)],"#24232A")
    D.pivot("jaw",*hl(7,5.3))
    D.close()
    face=[(-1,0),(-.5,-2),(3,-2.7),(7,-2.4),(12,-1.7),(17,-.8),
          (21,0),(23,1),(23.5,2.2),(23,4.1),(20,4.5),(16,4.7),
          (14,7.1),(10,8.5),(5,8.2),(1,5.6),(-.5,2)]
    local(face,D.lin("face",hl(7,-2),hl(9,7),[(0,"#33343E"),(.45,"#202128"),(1,"#111116")]))
    # Le modelé froid suit le chanfrein ; aucun poil gris ni toupet.
    fill=D.ell("joue",hl(9,3),8,3,.5,[(0,"#495064",.22),(1,"#495064",0)])
    local(face,fill)
    hairs(region([hl(*p) for p in face]),1700,lambda x,h:(.65,-.72),
          ["#555563","#101117","#3D414C"],(.22,.65),.035)
    # Œil latéral petit, iris paille couvert en haut par la paupière.
    ex,eh=hl(7.8,.3)
    D.open("eyes")
    oval((ex,eh),1.65,1.03,"#09090D")
    oval((ex+.08,eh),1.28,.76,"#5E5446")
    D.open("pupils")
    oval((ex+.1,eh),1.05,.74,"#C8B78E")
    # Coins arrondis, fente rigoureusement horizontale dans le monde.
    shape([(ex-.88,eh+.16),(ex+.96,eh+.15),(ex+1,eh-.12),
           (ex-.86,eh-.16)],"#0E0E10")
    D.pivot("pupils",ex+.1,eh)
    D.close()
    oval((ex-.32,eh+.25),.16,.1,"#F4EAD2")
    line([(ex-1.46,eh+.15),(ex-.6,eh+.7),(ex+.5,eh+.6),(ex+1.35,eh+.1)],"#16151C",.45)
    line([(ex-1.2,eh-.25),(ex,eh-.83),(ex+1.28,eh-.25)],"#57515A",.09,.45)
    D.pivot("eyes",ex,eh)
    D.close()
    # La peau et les paupières recouvrent le pourtour du globe quand iris
    # et pupille suivent le regard. C'est un tracé opaque composé, taillé
    # lors de la génération ; aucun masque ni découpe à exécuter au rendu.
    window=Polygon([(ex+1.3*math.cos(k*math.tau/48),eh+.76*math.sin(k*math.tau/48))
                    for k in range(48)])
    surround=Point(ex,eh).buffer(4.8).intersection(region([hl(*p) for p in face])).difference(window)
    pieces=getattr(surround,"geoms",[surround])
    skin=""
    for piece in pieces:
        skin+=poly(list(piece.exterior.coords)[:-1])
        for hole in piece.interiors:
            skin+=poly(list(hole.coords)[:-1])
    D.el(skin,fill="url(#face)",rule="evenodd")
    D.el(skin,fill="url(#joue)",rule="evenodd")
    hairs(surround,500,lambda x,h:(.65,-.72),
          ["#555563","#101117","#3D414C"],(.22,.65),.035)
    line([(ex-1.25,eh+.08),(ex-.55,eh+.69),(ex+.5,eh+.6),(ex+1.23,eh+.08)],"#101016",.22)
    # Museau étroit, naseau en virgule et sillon de la lèvre supérieure.
    local([(19.3,.1),(21.5,.2),(23.2,1.3),(23.3,3.2),(22.4,4),(19.7,3.7)],"#18191F")
    local([(20.6,.85),(22.1,1.3),(22.5,2.15),(21.5,2.7),(20.9,2.3),(21.4,1.9)],"#05070A")
    line([hl(20.4,.8),hl(21.5,.8),hl(22.4,1.5)],"#5B5B65",.12,.65)
    # Lèvre peinte à part dès l'origine : pas de double coupe du noir.
    local([(19.3,3.2),(22.5,3.1),(23,3.7),(20,4.1),(19.3,4)],"#68404A")
    D.open("lip")
    local([(18.8,2.9),(21,2.7),(23.2,2.9),(23.4,3.8),(22.3,4.3),(19,4.6)],"#202027")
    line([hl(19,4.5),hl(21.7,4.2),hl(23.2,3.8)],"#06070B",.16)
    D.pivot("lip",*hl(19,3.2))
    D.close()
    ear()
    oval((97,92),2.1,1.25,"#22232B")
    if VARIANTE!="agneau":
        horn()
    # Les poils de la nuque recouvrent le pied de la corne : pas de bout
    # scié posé sur le crâne, même lorsqu'on agrandit la tête.
    for _ in range(40):
        x,h=R.uniform(95.5,98.5),R.uniform(90.8,92.1)
        line([(x,h),(x-.25,h+.5),(x-.4,h+R.uniform(.6,1.3))],"#27272E",.09,.85)
    D.pivot("head",*POLL)
    D.pivot("muzzle",*hl(23.2,4.7))
    D.close()


def main(path):
    leg("leg-back-far",20,True,True)
    leg("leg-front-far",82,False,True)
    tail()
    leg("leg-back-near",12,True)
    leg("leg-front-near",77)
    neck()
    sways()
    fleece()
    head()
    D.pivot("ground",45,0)
    svg=('<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 133 109" width="133" height="109">\n'
         '<!-- Heidschnucke : brebis adulte de novembre, 67 cm au garrot osseux.\n'
         ' Une unité = un centimètre. Dessin original généré, photos de référence hors dépôt.\n'
         ' © 2026 Alejandro San Martin Lamas — MIT OR Apache-2.0 -->\n'
         '<defs>'+"".join(D.defs)+'</defs>\n'+"\n".join(D.out)+'</svg>')
    root=ET.fromstring(svg,parser=ET.XMLParser(target=ET.TreeBuilder(insert_comments=True)))
    paints=A.Paints(root)
    for name,h in [("leg-front-near",CARPUS),("leg-front-far",CARPUS),
                   ("leg-back-near",HOCK),("leg-back-far",HOCK)]:
        joint,half=A.leg_joint(svg,name,OY-h)
        A.split_leg(root,paints,name,joint,half+.35,(0,1))
    if VARIANTE:
        return variante(path,ET.tostring(root,encoding="unicode"))
    ET.indent(root,space=" ")
    ET.register_namespace("",A.Q[1:-1])
    output=ET.tostring(root,encoding="unicode")+"\n"
    Path(path).parent.mkdir(parents=True,exist_ok=True)
    Path(path).write_text(output,encoding="utf-8")
    print(f"{path} : {len(output.encode())/1024:.0f} Ko")


def variante(path, svg):
    """Les autres dessins : ce qui change, puis les proportions."""
    root = ET.fromstring(svg)
    if VARIANTE == "belier":
        variantes.bourses(root, *variantes.sous_le_ventre(root, 6, -6), largeur=9, hauteur=15,
                  peau="#3B3A3D", ombre="#18171A")
    if VARIANTE == "agneau":
        # Né noir, en laine bouclée ; il brunit puis grisonne en grandissant.
        variantes.recolorer(root, variantes.rampe("#0D0C0E", "#45424A", bas=.1, haut=.85),
                    dans=("body", "neck", "tail", "sway-1", "sway-2", "sway-3"))
    variantes.ecrire(path, root, **PROPORTIONS[VARIANTE])


if __name__ == "__main__":
    main(sys.argv[1] if len(sys.argv)>1 else HERE.parents[2]/"pets/heidschnucke/heidschnucke.svg")
