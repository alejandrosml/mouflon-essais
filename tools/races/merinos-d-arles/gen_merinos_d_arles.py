#!/usr/bin/env python3
"""Dessine le Mérinos d'Arles de Mouflon : un floucat blanc de cinq à six ans,
au départ de la transhumance, tondu en velours, avec quatre flocs et son redon.

D'après docs/races/merinos-d-arles.md, surtout les photos P5 (tête), P10
(floucat, silhouette et flocs), P11 (velours et marque), P13, P16 et P25
(cornes, collier et redon). Les photographies restent hors du dépôt.
Une unité = un centimètre ; repère x vers le nez, h vers le haut, sol h=0.
La graine fixe garde exactement les mêmes fibres et les mêmes stries.

Usage : python3 tools/races/merinos-d-arles/gen_merinos_d_arles.py [sortie.svg]
Dépendance : shapely, comme les autres générateurs de tools/races.
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
import articulations as A  # noqa: E402
import variantes  # noqa: E402

# Les dessins que ce script écrit (tools/races/variantes.py), d'après la
# fiche (§ 1.4) : sans variante, la brebis, le dessin de base de la race ;
# `belier`, le bélier entier, banard (cornu) ; `agneau` ; et `floucat`, le
# mâle castré à redon qui mène le troupeau (pets/floucat), le premier dessiné,
# dont les trois autres sont tirés.
VARIANTE = sys.argv[2] if len(sys.argv) > 2 else ""
FLOUCAT = VARIANTE == "floucat"
CORNU = FLOUCAT or VARIANTE == "belier"
BREBIS = dict(garrot=(75, 67), encolure=(1.0, .88), tete=(.97, .94))
PROPORTIONS = {
    "belier": dict(  # Bélier Mérinos d'Arles entier, banard : 82 cm au garrot, grandes cornes striées en spirale régulière, chanfrein busqué, encolure épaisse.
        garrot=(75, 82), encolure=(1.0, 1.14), tete=(1.05, 1.08)),
    "agneau": dict(  # Agneau Mérinos d'Arles de deux semaines : 40 cm au garrot, né à l'automne au retour d'estive, blanc, sans cornes.
        variantes.AGNEAU, garrot=(75, 40)),
}
from outils import (Doc, S, OY, add, sub, mul, norm, lerp, cr, dense, poly,  # noqa: E402
                    geom_d, Polygon, Point, mix, shade, qstroke, lstroke, clamp, runs_path)

D = Doc()
R = random.Random(1804)
POLL = (108.0, 94.0)
ANGLE = math.radians(14)
HU, HV = (math.cos(ANGLE), -math.sin(ANGLE)), (-math.sin(ANGLE), -math.cos(ANGLE))
BODY = [(0, 66), (2, 72), (9, 75.0), (23, 75.7), (43, 75.4), (62, 75.8), (77, 76), (86, 73),
        (91, 66), (93, 55), (90, 44), (84, 39), (74, 38), (57, 36.6), (40, 36.4), (25, 38),
        (13, 41), (5, 48), (0, 57)]
# L'épaule laisse à l'encolure une large embase cachée (découverte en broutant).
NECK = [(65, 68), (72, 75), (82, 76), (89, 82), (96, 90), (101, 96), (108, 97), (114, 92),
        (117, 85), (115, 78), (109, 70), (103, 61), (97, 49), (92, 41), (82, 39), (72, 48)]
BG, NG = Polygon(dense(BODY)).buffer(0), Polygon(dense(NECK)).buffer(0)


def HL(u, v):
    return add(POLL, add(mul(HU, u), mul(HV, v)))


def HLS(points):
    return [HL(*p) for p in points]


def ellipse(c, rx, ry, a=0, n=22):
    return [(c[0] + rx * math.cos(t) * math.cos(a) - ry * math.sin(t) * math.sin(a),
             c[1] + rx * math.cos(t) * math.sin(a) + ry * math.sin(t) * math.cos(a))
            for t in [i * 2 * math.pi / n for i in range(n)]]


def shape(points, fill, **kw):
    D.path(cr(points), fill=fill, **kw)


def line(points, col, width, alpha=1):
    D.path(cr(points, closed=False), stroke=col, sw=width, so=alpha, cap="round")


def soft(region, gid, c, rx, ry, col, alpha):
    """Un modelé fondu, borné géométriquement ; jamais de filtre ou masque SVG."""
    disk = Polygon(ellipse(c, rx, ry, n=60)).intersection(region)
    g = D.rad(gid, c, ry, [(0, col, alpha), (1, col, 0)], sx=rx / ry)
    D.path(geom_d(disk), fill=g)


def short_wool(region, seed, step=0.72, white=False):
    """Velours serré : grains irréguliers, avec des vides, jamais des rangs de points."""
    rr = random.Random(seed)
    x0,y0,x1,y1=region.bounds
    D.begin_batch()
    for i in range(int((x1-x0)*(y1-y0)/step**2*1.05)):
        p=(rr.uniform(x0,x1),rr.uniform(y0,y1))
        if not region.contains(Point(p)):continue
        L=rr.uniform(.12,.40)
        a=rr.uniform(-1.5,1.5)
        d=(math.cos(a)*L,math.sin(a)*L)
        pts=[p,add(p,(d[0]*.38-.04,d[1]*.38+.07)),add(p,d)]
        col='#F8F2E5' if rr.random()<.5 else '#AD9E83' if not white else '#BCAE95'
        D.stroke(cr(pts,closed=False),col,rr.choice([.045,.06,.08]),rr.choice([.18,.25,.32]))
    D.flush()


def hair(region, seed, direction=(0, -1), step=0.5, length=0.5):
    rr = random.Random(seed)
    x0, y0, x1, y1 = region.bounds
    D.begin_batch()
    for i in range(int((x1 - x0) * (y1 - y0) / step**2)):
        p = (rr.uniform(x0, x1), rr.uniform(y0, y1))
        if not region.contains(Point(p)):
            continue
        t = length * rr.uniform(0.5, 1)
        end = add(p, mul(direction, t))
        if region.contains(Point(end)):
            D.stroke(qstroke(p, add(lerp(p, end, 0.5), (0.06, 0)), end),
                     '#FFFDF6' if i % 2 else '#B1A691', 0.045, 0.4)
    D.flush()


def tuft(points, gid, seed, grain=0.75):
    """Laine fermée : petits blocs irréguliers à dessus aplati et fentes étroites.
    La frisure suit chaque bloc ; elle ne forme pas des colonnes de longues boucles."""
    region=Polygon(dense(points)).buffer(0)
    x0,y0,x1,y1=region.bounds
    grad=D.lin(gid,(x0,y0),(x0,y1),[(0,'#D0BD9B'),(.45,'#E5D8BD'),(1,'#B9A27F')])
    shape(points,grad)
    rr=random.Random(seed)
    fills={}
    strokes={}
    def wool_line(points,col,width,alpha):
        key=(col,width,round(alpha*10)/10)
        strokes.setdefault(key,[]).append(cr(points,closed=False))
    # Des blocs décalés : les sommets d'une ligne ne reprennent pas ceux de
    # la précédente. Chaque plateau a sa propre direction de frisure.
    yy=y0-grain
    row=0
    while yy<y1+grain:
        xx=x0-grain+(row%2)*grain*.45
        while xx<x1+grain:
            w=grain*rr.uniform(.72,1.34)
            hh=grain*rr.uniform(.72,1.23)
            x=xx+rr.uniform(-.23,.23)*grain
            y=yy+rr.uniform(-.24,.24)*grain
            raw=[(x-w*.53,y-hh*.47),(x-w*.47,y+hh*.45),(x-w*.08,y+hh*.61),
                 (x+w*.5,y+hh*.43),(x+w*.57,y-hh*.31),(x+w*.1,y-hh*.53)]
            cell=Polygon(dense(raw,True,5)).buffer(0).intersection(region)
            if not cell.is_empty:
                t=clamp((y-y0)/(y1-y0))
                u=abs((x-(x0+x1)/2)/max(.5,(x1-x0)/2))
                lit=.72-.22*t-.24*u*u+rr.uniform(-.10,.10)
                tone=mix('#A9916C','#F4EAD4',round(clamp(lit)*14)/14)
                fills.setdefault(tone,[]).append(geom_d(cell))
                # Des fentes brèves bordent deux côtés du bloc, interrompues
                # et variées pour ne pas fabriquer un quadrillage.
                a,b=raw[0],raw[1]
                if region.contains(Point(a)) and region.contains(Point(b)):
                    wool_line([a,lerp(a,b,.35),b], '#806B4B',.065,rr.uniform(.16,.30))
                top=[raw[1],raw[2],raw[3]]
                if all(region.contains(Point(q)) for q in top):
                    wool_line(top,'#FAF2DF',.08,.30)
                # Une petite ondulation fortement serrée au sein de la mèche.
                ang=rr.uniform(-1.2,1.2)
                u=(math.cos(ang),math.sin(ang));v=(-u[1],u[0])
                for j in range(2):
                    path=[]
                    for k in range(9):
                        q=add((x,y),add(mul(u,(k/8-.5)*w*.82),mul(v,(j-.5)*hh*.35+.05*math.sin(k*2.6+j))))
                        if region.contains(Point(q)):path.append(q)
                    if len(path)>1:wool_line(path,'#806B4B' if j else '#FAF1DD',.05,.25)
            xx+=grain*rr.uniform(.94,1.20)
        yy+=grain*.87
        row+=1
    # Les remplissages par teinte, puis leurs fibres, sont regroupés pour
    # garder une cuisson légère. La petite palette conserve le modelé.
    for col,paths in fills.items():D.path(''.join(paths),fill=col)
    for (col,width,alpha),paths in strokes.items():
        D.path(''.join(paths),stroke=col,sw=width,so=alpha,cap='round')
    # Le contour se perd en petits blocs de laine mates, jamais en filaments
    # réguliers ; les racines et pointes sont crêpelées de la même manière.
    border=dense(points,True,5)
    for j,p in enumerate(border):
        if j%2:continue
        r=grain*rr.uniform(.23,.43)
        block=[(p[0]-r,p[1]-r*.45),(p[0]-r*.92,p[1]+r*.65),
               (p[0]+r*.35,p[1]+r*.77),(p[0]+r,p[1]-r*.3)]
        tone=mix('#D7C6A5','#BCA47D',clamp((p[1]-y0)/(y1-y0)))
        D.path(cr(block,t=.65),fill=tone)
        line([block[1],block[2]],'#F8EDDA',.07,.50)


def ribbon(x, h, gid, scale=1):
    """Un ruban mat noué à la base : nœud et deux bouts libres, sans gros nœud décoratif."""
    F = lambda p: (x + p[0]*scale,h+p[1]*scale)
    shape([F(p) for p in [(-3,.7),(-.4,1),(2.3,.6),(2.7,-.4),(.2,-.8),(-3.2,-.3)]], '#B3322B')
    shape([F(p) for p in [(-.3,-.1),(-1,-2.3),(-3,-5.4),(-1.8,-4.8),(-1.1,-5.2),(.5,-2),(.8,.2)]], '#B3322B')
    shape([F(p) for p in [(.4,.1),(1.6,-1.7),(4.5,-3.8),(4.3,-2.3),(5.1,-2.7),(2.3,-.4),(1,1)]], '#C44035')
    line([F(p) for p in [(-2.8,.65),(-.7,.95),(1.7,.7)]], '#E76C5B', .18, .7)
    line([F(p) for p in [(.2,-.6),(-.9,-2.7),(-1.9,-4.4)]], '#741C18', .22, .65)
    shape([F(p) for p in [(-.6,.7),(.3,1.1),(1,.4),(.5,-.6),(-.4,-.5)]], '#9D2B25')


def floc(x, h, w, height, lean, name, seed):
    """Touffes laissées par la tonte : larges racines et bouquets de mèches
    cassées, couchées à la croupe ; bouts salis de poussière et de suint."""
    D.open(name)
    # Chaque faisceau possède une pointe émoussée, d'une hauteur différente.
    # Les petites pointes du contour ne sont pas symétriques comme une couronne.
    top=[(-.47,.42),(-.47,.60),(-.38,.61),(-.34,.78),(-.27,.74),(-.22,.90),
         (-.15,.88),(-.12,.81),(-.03,.86),(.00,1.0),(.09,.99),(.12,.87),
         (.20,.89),(.23,.72),(.32,.76),(.35,.61),(.44,.62),(.46,.41)]
    pts=[(x-w*.55,h-1),(x-w*.55,h+2)]
    pts += [(x+a*w+lean*b,h+b*height) for a,b in top]
    pts += [(x+w*.53,h+1),(x+w*.51,h-1)]
    tuft(pts,'laine-'+name,seed,.84)
    ribbon(x,h+.7,name,.62)
    D.pivot(name,(x,h))
    D.close()


def body():
    D.open('body')
    shape(BODY,'url(#velours)')
    soft(BG,'cote-lumiere',(32,65),39,16,'#FFFDF5',.36)
    soft(BG,'ventre-ombre',(51,38),39,11,'#8C795A',.19)
    soft(BG,'epaule-ombre',(85,50),10,23,'#A48F6E',.25)
    soft(BG,'cuisse-lumiere',(10,58),15,18,'#FFFBEE',.32)
    soft(BG,'hanche-creux',(18,59),8,12,'#907A58',.10)
    soft(BG,'epaule-reflet',(79,60),7,14,'#FFFBEF',.25)
    soft(BG,'flanc-creux',(29,46),9,7,'#9B8661',.12)
    short_wool(BG.buffer(-.24),1804)
    # Relief sobre de la hanche, du creux de flanc et de l'omoplate.
    for pts, c, sw, alpha in [([(11,68),(17,63),(20,56)],'#AA997D',.26,.15),
                               ([(24,48),(31,46),(35,42)],'#AA997D',.24,.14),
                               ([(78,68),(74,58),(77,47)],'#A99675',.25,.15)]:
        line(pts,c,sw,alpha)
    # Une marque simple à la peinture, irrégulière comme celle des troupeaux :
    # un rond barré (Φ), vu sur les brebis de Saint-Rémy (P11). Symétrique,
    # elle se lit pareil quand il se tourne vers la gauche, dessin retourné
    # (une lettre s'y lirait à l'envers). Le pinceau referme le rond.
    mark=[(11.64,63.27),(10.36,63.5),(9.1,62.88),(8.37,61.31),(8.38,59.52),
          (8.87,57.88),(9.9,56.69),(11.25,56.61),(12.38,57.52),(13.16,58.92),
          (13.41,60.73),(12.78,62.38),(11.64,63.27),(10.38,63.4)]
    line(mark,'#B03A2E',1.0,.78)
    line([(10.9,54.9),(10.75,60.0),(10.6,65.3)],'#BA4031',.9,.74)
    # Le bord du velours, presque lisse, garde un grain très fin.
    line(BODY[:11],'#918167',.20,.48)
    line(BODY[11:],'#918167',.22,.48)
    if FLOUCAT:
        floc(75,75.6,14,11,-1,'sway-1',9)
        floc(44,75.3,10,15,0.2,'sway-2',11)
        floc(13,74.7,13,11,-4.0,'sway-3',13)
    D.pivot('body',(46,36.4))
    D.close()


def leg(name, back, far):
    dx = 5 if far and back else -5 if far else 0
    off = lambda pts: [(x+dx,h) for x,h in pts]
    if back:
        outline = [(9,53),(20,49),(23,42),(18,35),(11,28),(8.5,24.5),(8.2,18),(8.0,8),
                   (8.8,4),(11.4,.4),(11.2,0),(5.0,0),(4.9,1.3),(5.4,3),(4.4,5),(4.5,8),
                   (4.8,19),(3.7,23),(4.0,26),(5,32),(4.4,41),(3.2,48)]
        pivot, joint=(12+dx,49),(6.3+dx,25)
        hoof=[(5,0),(11.2,0),(11.4,.4),(9.7,3),(8.9,3.7),(5.4,3),(4.9,1.3)]
    else:
        outline=[(79,50),(88,49),(89.2,41),(88.0,34),(86.6,27),(85.6,22),(85.3,18),(85.3,8),
                 (86.2,4),(89.0,.4),(88.8,0),(82.6,0),(82.4,1.2),(83,3),(81.4,5),(81.4,8),
                 (81.5,17),(81.4,20.6),(81.0,23),(80.8,28),(79.1,34),(77.2,39),(77.0,45)]
        pivot,joint=(83+dx,46),(83.4+dx,21)
        hoof=[(82.6,0),(88.8,0),(89,.4),(87.2,3),(86.3,3.7),(83,3),(82.4,1.2)]
    pts=off(outline); region=Polygon(dense(pts)).buffer(0)
    k=.82 if far else 1
    D.open(name)
    g=D.lin(name+'-poil',(0,52),(0,0),[(0,shade('#E3D8C3',k)),(.50,shade('#E6DFD0',k)),(1,shade('#F5F0E6',k))])
    shape(pts,g)
    soft(region,name+'-ombre',(joint[0]-1,19),3,23,shade('#998D75',k),.28)
    upper=region.intersection(Polygon([(-10,21 if not back else 25),(160,21 if not back else 25),(160,60),(-10,60)]))
    short_wool(upper.buffer(-.18),54+int(back)+int(far)*10,.61,True)
    # De petits bouts émoussés marquent la laine gardée au-dessus du canon.
    jh=25 if back else 21
    jx=joint[0]
    for q in range(7):
        xx=jx-1.9+q*.56
        hh=jh+.5+.32*math.sin(q*1.9)
        if region.contains(Point((xx,hh))):
            shape([(xx-.25,hh+.6),(xx-.30,hh-.12),(xx-.08,hh-.46),(xx+.23,hh-.17),(xx+.28,hh+.6)],
                  shade('#E6DCC6',k))
            line([(xx-.18,hh+.3),(xx+.06,hh-.21)],shade('#FFF5DF',k),.07,.65)
    lower=region.difference(upper).buffer(-.15)
    hair(lower,66+int(back)+int(far)*10,step=.44,length=.55)
    shape(off(hoof),D.lin(name+'-onglon',(0,4),(0,0),[(0,shade('#D8CFBC',k)),(.5,shade('#B9AE96',k)),(1,shade('#817965',k))]))
    toe_x=8 if back else 85.7
    line(off([(toe_x,0.1),(toe_x-.15,1),(toe_x-.8,2.8)]),shade('#6E6555',k),.15,.85)
    # Ergots derrière le paturon, petits et gris.
    er=4.4 if back else 81.4
    shape(off([(er,6.5),(er-.7,6.0),(er-.5,4.7),(er+.3,5.0)]),shade('#A3977D',k))
    line(off(outline[7:17] if back else outline[8:17]),shade('#8C826F',k),.15,.6)
    D.pivot(name,pivot)
    D.close()


def tail():
    """Queue naturelle jusqu'au jarret, en trois segments à attaches arrondies.
    Chaque segment enfant passe sous son parent : la rotation ne crée pas de fente."""
    centers=[(-.2,67),(-2.9,60),(-4.4,51),(-4.7,43),(-5.0,35),(-5.8,30)]
    widths=[5.8,5.3,4.4,3.8,3.2,1.5]
    def piece(i,j,name):
        pts=[(centers[k][0]-widths[k]/2,centers[k][1]) for k in range(i,j+1)]
        pts += [(centers[k][0]+widths[k]/2,centers[k][1]) for k in range(j,i-1,-1)]
        reg=Polygon(dense(pts)).buffer(0)
        if i:
            # Un disque plein, centré au pivot, glisse sous le segment parent.
            reg=reg.union(Point(centers[i]).buffer(widths[i]/2+.18,resolution=20))
        D.path(geom_d(reg),fill='url(#velours)')
        short_wool(reg.buffer(-.16),170+i,.56)
        D.pivot(name,centers[i])
    D.open('tail')
    D.open('tail-mid')
    D.open('tail-tip');piece(4,5,'tail-tip');D.close()
    piece(2,4,'tail-mid');D.close()
    piece(0,2,'tail');D.close()


def neck():
    D.open('neck')
    shape(NECK,'url(#velours)')
    short_wool(NG.buffer(-.3),75,.66)
    soft(NG,'cou-modelé',(108,78),8,18,'#B1A082',.35)
    line(NECK[3:13],'#A4967B',.2,.46)
    if FLOUCAT:
        collar()
    D.pivot('neck',(84,52))
    D.close()


def collar():
    """Coulas en cytise : un bras de profil, évasé en aile ; conjuscle et clavette.
    Le collier et la courroie suivent l'encolure ; le redon pend de la
    courroie (voir `bell`)."""
    # Le petit bout du bras caché, de l'autre côté de la gorge.
    shape([(103,78),(106,77),(119,67),(121,63),(117,64),(111,70)],'#6E482B')
    # Courroie de cuir : le redon y est suspendu par son anse.
    line([(111,71),(115,70),(120,69.5)],'#2E2016',2.3)
    bell()
    arm_top=[(94.7,95),(98.5,96),(99.8,91),(105,83),(110.5,76)]
    wing=[(110.5,76),(118.8,71.0),(118.4,61.7),(110.2,65.1)]
    arm_back=[(110.2,65.1),(105.7,71),(102,78),(97,86),(94.5,90),(94.7,95)]
    arm_d=runs_path([arm_top,wing[:2],wing[1:3],wing[2:4],arm_back])
    D.path(arm_d,fill=D.lin('cytise',(95,91),(116,66),[(0,'#B78F63'),(.25,'#845934'),(.65,'#5E3A22'),(1,'#9A6B44')]))
    # Le chant clair de la planche suit deux arêtes franches, et son revers
    # sombre apparaît sous l'aile (ni sangle de cuir, ni cloche suisse).
    shape([(118.4,61.7),(119.1,61.4),(119.5,70.8),(118.8,71.0)],'#D0A372')
    shape([(110.2,65.1),(118.4,61.7),(119.1,61.4),(110.6,64.4)],'#4A301D')
    # L'épaisseur polie et les veines, irrégulières et parallèles au fil.
    line([(95.4,94),(97.6,92),(99.3,87),(104.2,79),(109,71),(113,65.5),(118.3,62.2)],'#D3AD7B',.62,.8)
    line([(97.8,94.5),(99.1,90),(103.5,83),(107.5,76),(112,70)],'#3E2919',.25,.65)
    for i in range(7):
        x=98.4+i*.32
        line([(x,91.5),(x+1.7,87),(x+6.9,79),(x+11.6,70)],'#4A2A14' if i%2 else '#BD9364',.08,.6)
    # Clavette transversale en buis, sculptée de deux rosaces.
    pin=[(109,71),(109.6,73.2),(117,73.9),(119,72.5),(118.2,69.8),(111.2,69.4)]
    shape(pin,D.lin('buis',(114,74),(114,69),[(0,'#EFE2C4'),(.4,'#D9C49A'),(1,'#9A8058')]))
    line(pin+[pin[0]],'#7E643D',.15,.7)
    for cx in (112.3,116.2):
        line(ellipse((cx,71.8),1.25,1.05)+[ellipse((cx,71.8),1.25,1.05)[0]],'#987F53',.12,.9)
        for a in range(6):
            aa=a*math.pi/3
            line([(cx+.23*math.cos(aa),71.8+.23*math.sin(aa)),
                  (cx+.85*math.cos(aa+.24),71.8+.75*math.sin(aa+.24))],'#957A4D',.12,.8)
        shape(ellipse((cx,71.8),.21,.18),'#AA8B57')


# Là où le redon pend de sa courroie : le milieu de la courroie, dans l'anse.
BELL_PIVOT = (119.4, 69.6)


def bell():
    """Redon ventru, de 18 cm, bouche resserrée de 12 cm, patine brun bronze.

    Il forme la partie `bell`, que le moteur fait pendre de la courroie
    (le pivot, au milieu de l'anse) : il se balance, et se pose sur l'herbe
    quand le cou descend. L'anse de tôle, rivée à la cloche, va avec lui ;
    la courroie et le collier restent dans l'encolure. Le groupe garde sa
    place entre la courroie et l'aile du collier.

    Le battant, pendu dedans, forme la partie `clapper` (son pivot, haut dans
    la cloche) : il reste à la verticale quand elle penche et bat contre sa
    paroi. Il sort de la bouche derrière l'ourlet de devant."""
    D.open('bell')
    # L'anse de tôle, que traverse la courroie.
    shape([(117.5,70),(117.8,73),(121,73),(121.4,69.8)],'#574126')
    line([(118.6,70),(118.6,72.2),(120,72.2),(120.2,70)],'#BA986A',.4,.65)
    pts=[(115.7,70),(119.7,71),(123,69.5),(125.6,65.7),(126.2,61.5),(125.2,57.4),
         (124.6,53.0),(123.8,52),(113,52),(112.5,53),(112,57),(111.5,61),(112.2,65),(113.6,68)]
    region=Polygon(dense(pts)).buffer(0)
    shape(pts,D.lin('redon',(111,0),(126,0),[(0,'#453221'),(.25,'#8D6A42'),(.5,'#A58050'),(.65,'#6B4E31'),(1,'#36271A')]))
    soft(region,'redon-reflet',(115.4,64),3.4,6.4,'#D0B687',.32)
    soft(region,'redon-bas',(119,53.5),7,5,'#241C15',.5)
    # Martelage, laiton irrégulier et usure : ponctuations mates, jamais jaune vif.
    rr=random.Random(151)
    for i in range(160):
        p=(rr.uniform(112,126),rr.uniform(52,70))
        if region.buffer(-.4).contains(Point(p)):
            r=rr.uniform(.08,.35)
            shape(ellipse(p,r,r*.53),rr.choice(['#D2B27C','#362A20','#92794D']),fo=.19)
    line([(123.8,67.7),(124.6,64),(124.0,59),(123.9,54)],'#2F251B',.20,.7)
    # La bouche est ouverte et ourlée d'une cornière, avec battant pâle.
    shape(ellipse((118.6,52.0),5.9,1.6),'#1E150D')
    D.open('clapper')
    shape([(118,53),(119.3,53),(119.7,50.8),(119.2,49.5),(118.2,49.4),(117.9,50.7)],
          D.lin('battant',(118,51),(120,51),[(0,'#A89C84'),(.45,'#F4EEE2'),(1,'#C3B79D')]))
    D.pivot('clapper',(118.7,66.0))
    D.close()
    line(ellipse((118.6,52.2),6.0,1.65)+[ellipse((118.6,52.2),6.0,1.65)[0]],'#5C4128',.55,.95)
    line([(113.5,52.8),(116,52.0),(120,51.8),(123.8,52.5)],'#B1956B',.23,.8)
    D.pivot('bell',BELL_PIVOT)
    D.close()


# Tête large et légèrement busquée, sans plis en accordéon. L'encoche du
# museau laisse apparaître la mâchoire seulement lorsqu'elle s'ouvre.
HEAD=[(-3,1),(-1,-3),(5,-4.5),(11,-4.4),(16,-3.3),(21,-1.2),(26,1.0),(29,3.0),
      (30,4.6),(29.8,6.5),(28.8,7.2),(26,7.5),(22,8.1),(19,8.3),
      (17.2,9.3),(13,11.2),(7,12),(1.8,10),(-2,6)]
HG=Polygon(dense(HLS(HEAD))).buffer(0)


def ear(name, far=False):
    base=HL(2.7 if far else 3.3,2.0)
    scale=.82 if far else 1
    F=lambda u,v:(base[0]-u*scale,base[1]+v*scale)
    D.open(name)
    pts=[F(0,0),F(1.2,1.8),F(5.5,2),F(9.5,.6),F(10.2,0),F(8.2,-1.3),F(4,-1.9),F(.6,-1)]
    g=D.lin(name+'-peau',F(0,2),F(0,-2),[(0,'#F7F1E8'),(.45,'#E9E0D3'),(1,'#B3A590')])
    shape(pts,g)
    if not far:
        inner=[F(1.3,.3),F(3.2,1.0),F(6.4,.8),F(8.4,.1),F(6,-.9),F(3,-1)]
        shape(inner,D.lin('oreille-rose',F(0,1),F(0,-1),[(0,'#EBC3BA'),(.6,'#D19A90'),(1,'#A67568')]))
        hair(Polygon(dense(pts)).buffer(-.3),189,direction=(-1,0),step=.4,length=.6)
        line([F(1.2,-.7),F(4,-1.1),F(7.8,-.2)],'#9A7869',.13,.6)
    D.pivot(name,base);D.close()


def horn(far=False):
    """¾ de tour ouvert, la pointe libre vers l'avant ; section triangulaire striée."""
    # Arc mesuré sur les floucas P10/P13 ; le bélier P5 ne donne que la surface.
    axis=[(111,98),(106,99),(101,97),(97.5,92),(96.7,86),(98.7,80.4),(103,76.5),
          (109,75),(114.8,76.6),(118.4,80),(119.9,83.5)]
    gros=6.2
    if VARIANTE=="belier":
        # Le bélier banard (§ 1.4) : une spirale régulière d'un tour un
        # tiers, forte à la base, qui se resserre vers la pointe.
        axis=variantes.spirale((111,98),(107.8,87.2),1.33,6.2,n=36)
        gros=8.8
    if far:
        axis=[add(p,(-.7,.35)) for p in axis[:5 if VARIANTE!="belier" else 15]]
    C=dense(axis,closed=False,per=10)
    L=len(C); normals=[]; widths=[]
    for i,p in enumerate(C):
        t=norm(sub(C[min(i+1,L-1)],C[max(0,i-1)]))
        normals.append((-t[1],t[0]))
        t=i/(L-1)
        widths.append((gros*(1-t)**.7+.1)*(1+.20*math.sin(math.pi*t)) if not far else gros*4.5/6.2)
    out=[add(p,mul(n,w/2)) for p,n,w in zip(C,normals,widths)]
    inn=[add(p,mul(n,-w/2)) for p,n,w in zip(C,normals,widths)]
    k=.83 if far else 1
    shape(out+inn[::-1],D.lin('corne-loin' if far else 'corne',(0,100),(0,74),
          [(0,shade('#E6D7C2',k)),(.3,shade('#CDB89E',k)),(.7,shade('#9F866B',k)),(1,shade('#D8C7AE',k))]))
    if far:return
    # Face supérieure de la section, distincte du pan tourné vers la joue.
    ridge=[add(p,mul(n,w*.08)) for p,n,w in zip(C,normals,widths)]
    shape(out+ridge[::-1],'#E6D7C2',fo=.30)
    inner_plane=[add(p,mul(n,-w*.12)) for p,n,w in zip(C,normals,widths)]
    shape(inn+inner_plane[::-1],'#7A6250',fo=.25)
    line(out,'#F2E5D1',.23,.7)
    line(inn,'#7A6250',.23,.65)
    acc=0; next_ring=.1
    rr=random.Random(41)
    for i in range(1,L-3):
        acc += math.dist(C[i],C[i-1])
        if acc<next_ring:continue
        next_ring=acc+rr.uniform(.25,.42)
        if i>L*.90:continue
        t=norm(sub(C[i+1],C[i-1])); n=normals[i]; w=widths[i]
        a=add(C[i],mul(n,w*.48));b=add(C[i],mul(n,-w*.47));m=add(C[i],mul(t,.3))
        D.path(qstroke(a,m,b),stroke='#7A6250',sw=.10,so=.65,cap='round')
        D.path(qstroke(add(a,mul(t,.13)),add(m,mul(t,.13)),add(b,mul(t,.13))),
               stroke='#F0E2CC',sw=.08,so=.75,cap='round')
    line([add(p,mul(n,w*.1)) for p,n,w in zip(C,normals,widths)],'#C8B393',.18,.55)


def eye():
    c=HL(11.1,1.5)
    # La fente reste horizontale dans le repère de l'animal.
    almond=[add(c,p) for p in [(-1.6,0),(-.9,.8),(.2,1),(1.3,.55),(1.7,0),(.8,-.7),(-.4,-.7)]]
    shape(almond,'#C6B49A')
    line([add(c,(-1.5,0)),add(c,(-.3,-.1)),add(c,(1.5,0))],'#8A7A68',.18,.8)
    D.open('eyes')
    shape(almond,'#4D3824')
    D.open('pupils')
    shape(ellipse(c,1.14,.9),D.rad('iris',c,1.3,[(0,'#D1AB6F'),(.45,'#9A7442'),(1,'#5E4424')]))
    for i in range(22):
        a=2*math.pi*i/22
        line([add(c,(.55*math.cos(a),.4*math.sin(a))),add(c,(1.0*math.cos(a),.8*math.sin(a)))],
             '#D1AD72' if i%2 else '#60431F',.045,.55)
    # Rectangle allongé aux extrémités arrondies.
    shape([add(c,p) for p in [(-.9,-.17),(-.8,.18),(.75,.18),(.9,0),(.8,-.18)]],'#0F0C0A')
    D.pivot('pupils',c);D.close()
    shape(ellipse(add(c,(-.45,.43)),.20,.12),'#FFFFFF',fo=.85)
    line(almond[:5],'#8A7A68',.2,.9)
    for i in range(13):
        x=-1.2+i*.19
        p=add(c,(x,.65+.2*math.sin(i/12*math.pi)))
        line([p,add(p,(.06,-.32))],'#F8F1E2',.06,.85)
    D.pivot('eyes',c);D.close()


def jaw():
    D.open('jaw')
    shape(HLS([(18,7.1),(20,5.8),(24,5.6),(28,5.7),(29,6.5),(28.4,8),(25,8.5),(21,8.9),(18,9)]),'#5D3832')
    shape(HLS([(20,7.3),(23,6.8),(26,6.9),(27.7,7.3),(25,8.1),(21,8.2)]),'#C78F87')
    shape(HLS([(17.7,8),(21,8.3),(25,8.0),(28.8,7.0),(29.1,8.0),(27.6,9.2),(23,10.2),(19,10.2),(16.3,9.3)]),
          D.lin('menton',HL(22,7),HL(22,10),[(0,'#EDE6DA'),(1,'#B5A895')]))
    line(HLS([(20,8.6),(24,8.4),(28.4,7.5)]),'#9A786C',.18,.8)
    D.pivot('jaw',HL(17.5,8.5));D.close()


def head():
    D.open('head')
    if CORNU:
        horn(True)
    ear('ear-far',True);jaw()
    shape(HLS(HEAD),D.lin('face',HL(12,-4),HL(12,12),[(0,'#FBF7F1'),(.35,'#EDE6DA'),(1,'#BCAF98')]))
    soft(HG,'joue-modelé',HL(8,7),9,8,'#AA9B83',.25)
    hair(HG.buffer(-.3),71,direction=HU,step=.43,length=.6)
    # Deux plis transversaux très fins du haut du chanfrein, jamais un accordéon.
    line(HLS([(16.4,-2.2),(15.8,-.4),(16.2,1.2)]),'#B2A48E',.13,.5)
    line(HLS([(18.6,-1.5),(18.2,.1),(18.7,1.5)]),'#C3B39A',.11,.5)
    # Laine des bajoues puis calotte : à l'arrière et un peu au-dessous des yeux.
    tuft(HLS([(-3,-1),(4,-1),(8.0,1),(8.5,4),(9,7),(12,10),(10,12),(5,13),(0,11),(-3,7)]),'joue-laine',103,.65)
    tuft(HLS([(-4,-.5),(-3,-5),(-1,-7),(3,-7.6),(7,-7.1),(10,-6.2),(13,-4.2),(13.8,-2.5),
              (12.5,-1.3),(10,-1.0),(7,-.6),(4,.1),(1,.5)]),'toupet',104,.69)
    if FLOUCAT:
        ribbon(*HL(1,-4),'ruban-toupet',.37)
    eye()
    # Museau nu rose discret, naseau en virgule oblique et philtrum.
    nose=HLS([(27.1,2.2),(28.6,2.7),(29.7,3.8),(30.2,5),(29.9,6.3),(29,7.0),(27.9,6.4),
              (27.6,5.0),(26.9,4.1),(26.5,3.3)])
    shape(nose,D.lin('nez',HL(28,2),HL(29,7),[(0,'#E4C6BE'),(.45,'#C99A90'),(1,'#93685E')]))
    shape(HLS([(27.2,3.5),(28.2,3.8),(28.8,4.5),(28.5,5),(27.8,4.4),(27.1,4.1)]),'#4A302B')
    line(HLS([(27.2,3.4),(28.1,3.5),(28.7,4.0)]),'#ECCEC2',.17,.8)
    line(HLS([(29.8,5.6),(29.4,6.2),(29.1,7)]),'#93685E',.18,.85)
    line(HLS([(19.4,8.3),(23,8.0),(26.2,7.7),(29,7)]),'#8B695D',.19,.75)
    if CORNU:
        horn()
    ear('ear-near')
    # Quelques blocs de laine noient la racine de la corne dans le toupet.
    if CORNU:
        tuft(HLS([(-.1,-5.4),(.2,-7.0),(2,-7.7),(4,-7.3),(5.8,-6.2),(5,-4.6),(2,-4.3)]),'racine-corne',105,.6)
    D.pivot('head',POLL)
    D.pivot('muzzle',HL(29.1,8.1))
    D.close()


def gum():
    pts=HLS([(25,6.2),(27,5.8),(29,5.8),(29.7,6.3),(29.0,7),(26.8,7.5),(24.7,7.5)])
    return [ET.Element(A.Q+'path',{'d':cr(pts),'fill':'#D3A299'})]


def articulate(svg):
    root=ET.fromstring(svg,parser=ET.XMLParser(target=ET.TreeBuilder(insert_comments=True)))
    paints=A.Paints(root)
    for name,h in [('leg-back-far',25),('leg-front-far',21),('leg-back-near',25),('leg-front-near',21)]:
        joint,half=A.leg_joint(svg,name,OY-h)
        A.split_leg(root,paints,name,joint,half+.4)
    lip=HLS([(25.0,5.3),(28,5.2),(30.3,5.7),(30.4,7.1),(29,7.6),(26.5,8),(24.9,8.2)])
    A.lift_part(root,paints,'head',[S(p) for p in lip],'lip',S(HL(25.1,6.5)),under=gum())
    ET.indent(root,space='  ')
    return ET.tostring(root,encoding='unicode')+'\n'


def main(path):
    D.lin('velours',(0,76),(0,35),[(0,'#F3ECDF'),(.3,'#E3D8C3'),(.75,'#D8C9AE'),(1,'#B5A487')])
    # L'encolure (et la cloche qu'elle porte) passe derrière les pattes qu'on
    # voit : baissée pour brouter, elle est au milieu du corps, et la cloche
    # posée sur l'herbe ne recouvre pas l'antérieur.
    leg('leg-back-far',True,True);leg('leg-front-far',False,True)
    neck()
    leg('leg-back-near',True,False);leg('leg-front-near',False,False)
    tail();body();head()
    D.pivot('ground',(46,0))
    svg=D.render(160,120,"""  Mérinos d'Arles : floucat blanc cornu, tondu au départ de la transhumance.
  1 unité = 1 cm ; garrot osseux 75 cm ; quatre flocs, rubans rouges,
  collier-lyre de cytise et redon brun bronze. D'après la fiche de la race.
  Écrit par tools/races/merinos-d-arles/gen_merinos_d_arles.py.
  © 2026 Alejandro San Martin Lamas — MIT OR Apache-2.0""")
    svg=articulate(svg)
    if not FLOUCAT:
        return variante(path,svg)
    with open(path,'w',encoding='utf-8') as f:f.write(svg)
    print(f'{path} : {len(svg.encode())/1024:.0f} Ko, {svg.count("<path")} tracés')


def variante(path,svg):
    """La brebis, le bélier ou l'agneau : ce qui change du floucat, puis ses proportions."""
    root=ET.fromstring(svg)
    if VARIANTE=="":
        variantes.mamelle(root,*variantes.sous_le_ventre(root,11,-1.5),largeur=9,hauteur=6,
                          peau="#E7C3B6",ombre="#B88C7E")
        return variantes.ecrire(path,root,**BREBIS)
    if VARIANTE=="belier":
        variantes.bourses(root,*variantes.sous_le_ventre(root,7,-3),largeur=10,hauteur=16,
                          peau="#E9DECB",ombre="#A8987E")
    variantes.ecrire(path,root,**PROPORTIONS[VARIANTE])


if __name__=='__main__':
    main(sys.argv[1] if len(sys.argv)>1 else 'pets/merinos-d-arles/merinos-d-arles.svg')
