#!/usr/bin/env python3
"""Une brebis Landes de Bretagne de trois à cinq ans, blanche et mouchetée,
en pleine toison de mai, juste avant la tonte. D'après la fiche de la race,
docs/races/landes-de-bretagne.md, et les photographies citées dans README.md.

Repère en centimètres : x vers le nez, h vers le haut, sol à h = 0.
Une unité SVG vaut un centimètre. Les pattes et la lèvre sont recoupées par
l'outil commun ; l'encolure a son propre dessin sous le corps. Les longues
mèches du ventre sont accrochées par leur racine, la queue en trois pièces.
Toutes les couleurs et les poils sont des tracés vectoriels originaux.
Aucune photo, découpe SVG, image incorporée, filtre ou masque.

Usage : python3 tools/races/landes-de-bretagne/gen_landes_de_bretagne.py <sortie.svg>
© 2026 Alejandro San Martin Lamas — MIT OR Apache-2.0
"""
import math
from pathlib import Path
import random
import sys
import xml.etree.ElementTree as ET

HERE = Path(__file__).resolve().parent
sys.path.insert(0, str(HERE.parent))
sys.path.insert(0, str(HERE))
import articulations as A
import variantes  # noqa: E402

# Les autres dessins de la race (tools/races/variantes.py), d'après la fiche.
# Sans variante : la brebis.
VARIANTE = sys.argv[2] if len(sys.argv) > 2 else ""
PROPORTIONS = {
    "belier": dict(  # Bélier des Landes de Bretagne : 62 cm au garrot, sans cornes comme la plupart, tête plus large, encolure plus épaisse.
        garrot=(57, 62), encolure=(1.0, 1.14), tete=(1.05, 1.07)),
    "agneau": dict(  # Agneau des Landes de Bretagne de deux semaines : 34 cm au garrot, petites boucles serrées plus blanches que sa mère, longue queue en crosse.
        variantes.AGNEAU, garrot=(57, 34), corps=(.84, .8)),
}
from outils import (Doc, OY, S, Point, Polygon, add, clamp, cr, dense, geom_d,
                    lerp, lstroke, mix, norm, poisson, poly, qstroke, rot, shade, sub)

# Les mesures estimées de la fiche (§ 2). Le garrot osseux (57 cm) est caché
# par huit centimètres de laine couchée ; le ventre laineux reste à 21 cm.
GARROT, CARPE, JARRET = 57.0, 14.5, 18.5
POLL = (77.0, 76.7)
NECK_BASE = (66.0, 44.5)
FORE, HIND = 61.0, 8.0
SHOULDER, HIP = 38.0, 42.0
HEAD_ANGLE = math.radians(39.0)
HU = (math.cos(HEAD_ANGLE), -math.sin(HEAD_ANGLE))
HV = (-math.sin(HEAD_ANGLE), -math.cos(HEAD_ANGLE))
FLEECE, LIGHT, SHADOW, DEEP = '#DDD3BE', '#F2E9DB', '#A8997C', '#6C5F45'
HAIR, HAIR_LT, HAIR_SH = '#E6E1D6', '#F5F2EA', '#C9BBA6'
RUST, RUST_LT, RUST_SH = '#7A5B48', '#A07A5C', '#55402F'
D = Doc()


def HL(u, v):
    """Repère de la tête : u de la nuque au nez, v vers la mâchoire."""
    return (POLL[0] + HU[0] * u + HV[0] * v,
            POLL[1] + HU[1] * u + HV[1] * v)


def HLs(pts):
    return [HL(*p) for p in pts]


def shape(pts):
    return Polygon(dense(pts, True, 6)).buffer(0)


def ell(c, rx, ry, angle=0, n=22):
    return [add(c, rot((rx * math.cos(k * math.tau / n), ry * math.sin(k * math.tau / n)), angle))
            for k in range(n)]


def spot(c, rx, ry, col, rng, clip=None, opacity=1):
    """Petite tache à bord irrégulier, toujours découpée géométriquement."""
    pts = []
    for k in range(14):
        a = k * math.tau / 14
        r = rng.uniform(.78, 1.18)
        pts.append(add(c, (rx * r * math.cos(a), ry * r * math.sin(a))))
    if clip is None:
        D.path(cr(pts), fill=col, fo=opacity)
    else:
        D.path(geom_d(shape(pts).intersection(clip), simplify=.01), fill=col, fo=opacity)


def glow(gid, c, rx, ry, col, alpha, clip):
    pts = ell(c, rx, ry)
    D.path(geom_d(shape(pts).intersection(clip)),
           fill=D.rad(gid, c, ry, [(0, col, alpha), (1, col, 0)], sx=rx/ry))


def fleece_light(x, h):
    """Lumière large, à laquelle s'ajoutent des nappes de laine irrégulières."""
    t = clamp((h - 20) / 45)
    v = .22 + .71 * t
    v += .055 * math.sin(x * .23 + h * .31) + .035 * math.sin(x * .59 - h * .17)
    v -= .12 * max(0, (x - 59) / 20)
    return clamp(v)


lock_number = 0


def lock(root, length, width, angle, light, rng, clip=None, dirt=.16):
    """Une mèche grossière et ondulée, non une spirale régulière. Un ruban
    effilé, ses fibres longitudinales, quelques cassures transversales ; le
    pas, la torsion et la largeur varient. Les pointes sont moins claires.
    `clip` sert seulement au calcul des tracés, jamais à un clip-path SVG."""
    global lock_number
    lock_number += 1
    ident = str(lock_number)
    wave = rng.uniform(.055, .12) * width
    phase = rng.uniform(0, math.tau)
    pitch = rng.uniform(2.8, 5.8)
    bend = rng.uniform(-.12, .12)
    u = (math.sin(angle), -math.cos(angle))
    n = (math.cos(angle), math.sin(angle))

    def axis(t, side=0):
        s = t * length
        wav = wave * (math.sin(s * math.tau / pitch + phase) - math.sin(phase)) * min(1, t * 5)
        wav += bend * s * t
        hw = width * .5 * (1 - .38 * t ** 1.7)
        hw *= .92 + .09 * math.sin(s * 2.2 + phase) + .035 * math.sin(s * 5.1)
        if t > .89:
            hw *= .6 + .4 * max(0, (1-t)/.11)
        return add(root, add((u[0] * s, u[1] * s), (n[0] * (wav + side * hw), n[1] * (wav + side * hw))))

    steps = max(12, int(length * 2))
    cap = add(root, (-u[0]*width*.34,-u[1]*width*.34))
    tip = add(axis(1),(u[0]*width*.14,u[1]*width*.14))
    pts = [cap] + [axis(k / steps, 1) for k in range(steps+1)] + [tip] + [axis(k / steps, -1) for k in range(steps, -1, -1)]
    region = shape(pts)
    if clip is not None:
        region = region.intersection(clip)
    if region.is_empty:
        return
    base = mix(SHADOW, LIGHT, light)
    if dirt:
        base = mix(base, '#B8A27E', dirt * rng.uniform(.3, 1))
    a, b = axis(.4, -1), axis(.4, 1)
    fill = D.lin('meche-' + ident, a, b,
                 [(0, shade(base, .92)), (.23, base), (.5, mix(base, LIGHT, .12)),
                  (.73, mix(base, '#A8997C', .08)), (1, shade(base, .9))])
    D.path(geom_d(region, nd=1, simplify=.075), fill=fill)
    # Les pointes jaunies : un voile dans le même contour, sans bord dur.
    tipfill = D.lin('pointe-' + ident, axis(.45), axis(1),
                    [(0, '#8D724C', 0), (.65, '#B8A27E', .10), (1, '#8D724C', dirt * .68)])
    D.path(geom_d(region, nd=1, simplify=.075), fill=tipfill)
    # Fibres discontinues et frisures : pas de grands reflets continus
    # qui feraient lire la laine comme une lame brillante.
    from shapely.geometry import LineString
    D.begin_batch()
    for j in range(rng.randint(15, 25)):
        side = rng.uniform(-.85, .85)
        start = rng.uniform(.02,.82)
        end = min(.98,start+rng.uniform(.1,.25))
        pathpts = [axis(start + (end-start)*k/5, side) for k in range(6)]
        line = LineString(dense(pathpts,False,2)).intersection(region).simplify(.05)
        lines = list(line.geoms) if hasattr(line, 'geoms') else [line]
        ds = ''.join(poly(list(g.coords),closed=False,nd=1) for g in lines if hasattr(g,'coords') and len(g.coords)>1)
        col = mix(base,'#FFF9EE',.45) if j%3 else shade(base,.78)
        D.stroke(ds,col,.055,.55)
    # De courtes frisures rompues, autour de faisceaux irréguliers. Aucun
    # arc ne traverse toute la largeur comme une écaille ou un pli de papier.
    for _ in range(rng.randint(5,10)):
        t=rng.uniform(.13,.92)
        center=axis(t,rng.uniform(-.55,.55))
        radius=rng.uniform(.16,.37)
        turn=rng.uniform(-.65,.65)
        pts=[add(center,rot((radius*math.cos(a),radius*.62*math.sin(a)),turn))
             for a in (0,.8,1.7,2.7,3.5)]
        line=LineString(dense(pts,False,3)).intersection(region)
        lines=list(line.geoms) if hasattr(line,'geoms') else [line]
        ds=''.join(poly(list(g.coords),closed=False,nd=1) for g in lines if hasattr(g,'coords') and len(g.coords)>1)
        D.stroke(ds,shade(base,.76),.07,.45)
    D.flush()


def wool_edge(points,amp):
    """Bosses de laine de largeur et de hauteur irrégulières, sans motif
    périodique. Les raccords se superposent sur une ligne de dos calme."""
    contour=dense(points,True,9)
    distances=[]
    distance=0
    for i,p in enumerate(contour):
        if i:
            distance+=math.dist(p,contour[i-1])
        distances.append(distance)
    total=distance+math.dist(contour[-1],contour[0])
    rng=random.Random(462)
    centers=[]
    at=0
    while at<total:
        centers.append((at,rng.uniform(.5,1.35),amp*rng.uniform(.35,1.35)))
        at+=rng.uniform(1.3,4.8)
    out=[]
    for i,p in enumerate(contour):
        tangent=norm(sub(contour[(i+1)%len(contour)],contour[i-1]))
        n=(-tangent[1],tangent[0])
        d=-amp*.2
        for center,radius,height in centers:
            delta=abs(distances[i]-center)
            delta=min(delta,total-delta)
            d+=height*math.exp(-(delta/radius)**2)
        out.append(add(p,(n[0]*d,n[1]*d)))
    return out


BODY = [(1, 49), (2, 55), (8, 61.5), (17, 64), (30, 64.5), (44, 64),
        (57, 64.5), (66, 61.5), (72, 57), (75, 49), (75, 39), (71, 30),
        (64, 25), (54, 23), (41, 22), (28, 22), (15, 24), (7, 28), (2, 37)]
BODY=wool_edge(BODY,.36)
BODY_POLY = shape(BODY)
NECK = [(57, 44), (58, 57), (62, 65), (67, 71), (72, 76), (76.5, 78),
        (80, 76), (82, 71), (83, 65), (82.5, 58), (80, 50), (77, 42),
        (74, 35), (69, 32), (64, 33), (60, 37)]
NECK=wool_edge(NECK,.26)
NECK_POLY = shape(NECK)


def fleece(name, pts, seed, dx, dh, lengths, neck=False):
    region = shape(pts)
    rng = random.Random(seed)
    D.open(name)
    D.path(cr(pts), fill=D.lin('fond-' + name, (0, 70), (0, 20),
                             [(0, '#DFD3BC'), (.5, '#C7B89D'), (1, '#97866B')]))
    # Les mèches les plus basses d'abord ; leurs racines disparaissent sous
    # les suivantes. Semis de Poisson : aucun quadrillage de rangées.
    roots = poisson(region.buffer(-1.1), dx, rng)
    roots.sort(key=lambda p: p[1])
    for x, h in roots:
        L = rng.uniform(*lengths)*(1+.19*math.sin(x*.47+h*.29))
        if neck:
            ang = rng.uniform(-.7, .7) + .22
            light = .48 + .4 * clamp((h-35)/43) - .1 * clamp((x-75)/8)
        else:
            ang = rng.uniform(-.25, .25) + .18 * math.sin(x/8+h/9)
            if x < 12:
                ang -= .3
            light = fleece_light(x, h-L*.3)
        lock((x, h), L, rng.uniform(dx*1.5, dx*2.4), ang,
             clamp(light + rng.uniform(-.09, .09)), rng, clip=region, dirt=.16 if h>35 else .28)
    # Le contour vivant : petites pointes couchées sur le dos, mèches
    # longues sur le ventre et la croupe. Le fond n'est jamais un ovale nu.
    contour = dense(pts, True, 10)
    prev = contour[0]
    accum = 0
    for i, p in enumerate(contour):
        accum += math.dist(p, prev)
        prev = p
        if accum < rng.uniform(1.5, 2.2):
            continue
        accum = 0
        x,h = p
        if not neck and h>54:
            continue
        tangent = norm(sub(contour[(i+2)%len(contour)], contour[(i-2)%len(contour)]))
        outward = (-tangent[1], tangent[0])
        # Le contour est horaire dans le monde ; la normale gauche sort.
        root = add(p, (-outward[0]*2.3, -outward[1]*2.3))
        angle = math.atan2(outward[0], -outward[1])
        L = rng.uniform(2.25, 2.65)
        if h < 31 and not neck:
            # la frange mobile est dessinée séparément, sous le corps
            continue
        lock(root, L, rng.uniform(2.3, 3.0), angle, fleece_light(x,h), rng, dirt=.2)
    # Quelques jarres au bord supérieur : elles partent de la surface
    # et restent fines, séparées par des distances irrégulières.
    if not neck:
        D.begin_batch()
        edge=dense(pts,True,3)
        for i in range(0,len(edge),5):
            p=edge[i]
            if p[1]<59 or rng.random()>.13:
                continue
            tangent=norm(sub(edge[(i+1)%len(edge)],edge[i-1]))
            n=(-tangent[1],tangent[0])
            a=add(p,(-n[0]*.2,-n[1]*.2))
            b=add(p,(n[0]*rng.uniform(.25,.6),n[1]*rng.uniform(.25,.6)))
            D.stroke(qstroke(a,lerp(a,b,.6),b),LIGHT,.045,.65)
        D.flush()
    # Le grain feutré au-dessus des mèches : de petits faisceaux et des
    # frisures orientés par nappes, regroupés pour garder le SVG compact.
    D.begin_batch()
    for x,h in poisson(region.buffer(-.6),.65,rng,limit=1500):
        a=rng.uniform(-.8,.8)
        radius=rng.uniform(.13,.35)
        grain=[add((x,h),rot((radius*math.cos(t),radius*.7*math.sin(t)),a)) for t in (0,.8,1.7,2.6,3.5)]
        light=round(fleece_light(x,h)*5)/5
        base=mix(SHADOW,LIGHT,light)
        D.stroke(cr(grain,closed=False,nd=1),shade(base,.73) if rng.random()<.5 else mix(base,LIGHT,.7),.06,.48)
    D.flush()
    # Jarres fines irrégulières sur les nappes et aux bords du dos.
    D.begin_batch()
    for x,h in poisson(region.buffer(-.4), .8, rng, limit=850):
        if rng.random() < .5:
            continue
        L = rng.uniform(.25, .65)
        end=(x+rng.uniform(-.1,.15),h-L)
        if not region.contains(Point(end)):
            continue
        D.stroke(qstroke((x,h), (x+.1,h-L*.5), end),
                 '#FFF7E9' if rng.random()<.65 else '#8F8065', .045, .45)
    D.flush()
    # Une ombre large décrit le volume entier : le ventre et la gorge
    # rentrent dans la lumière chaude, sans juxtaposer des panneaux plats.
    volume=D.lin('volume-'+name,(0,78 if neck else 65),(0,32 if neck else 20),
                 [(0,'#6C593D',0),(.3,'#6C593D',0),(.65,'#6C593D',.12),(1,'#6C593D',.42)])
    D.path(cr(pts),fill=volume)
    D.pivot(name, NECK_BASE if neck else (39,26))
    D.close()


def fringe():
    """Trois nappes pendantes, enracinées sous le ventre. Les mèches
    s'écartent en longueurs inégales : des touffes larges courtes et quelques
    cordes plus fines, sans ourlet de pointes toutes à la même hauteur."""
    for j,(a,b) in enumerate(((18,30),(31,43),(44,56)),1):
        rng=random.Random(610+j)
        D.open('sway-'+str(j))
        n=rng.randint(5,7)
        for i in range(n):
            x=a+(b-a)*(i+rng.uniform(.15,.85))/n
            h=27+rng.uniform(-1.1,1.1)
            long=i in (1,n-2)
            length=rng.uniform(8.1,10) if long else rng.uniform(4.8,7.7)
            width=rng.uniform(1.6,2.2) if long else rng.uniform(2.5,3.5)
            lock((x,h),length,width,rng.uniform(-.3,.3),.3+rng.uniform(-.06,.08),rng,dirt=.3)
        D.pivot('sway-'+str(j),((a+b)/2,26.8))
        D.close()


def hair(region, rng, count, flow=(0,-1), length=(.3,.75), far=False):
    D.begin_batch()
    x0,h0,x1,h1 = region.bounds
    for _ in range(count):
        p = (rng.uniform(x0,x1),rng.uniform(h0,h1))
        if not region.contains(Point(p)):
            continue
        d = rng.uniform(*length)
        e = add(p, (flow[0]*d,flow[1]*d))
        if not region.contains(Point(e)):
            continue
        D.stroke(lstroke(p,e), shade('#FFF8EC',.84 if far else 1) if rng.random()<.65 else '#9A8E79', .045, .55)
    D.flush()


def hoof(cx, back, far, gid):
    k = .83 if far else 1
    heel, toe = cx-.2, cx+3.5
    pts = [(heel,2.7),(toe-.6,2.7),(toe+.2,.65),(toe+.1,.08),
           (heel-.45,.08),(heel-.6,.8)]
    reg = shape(pts)
    D.path(cr(pts, t=.6), fill=D.lin('onglon-'+gid,(heel,3),(toe,0),
                                   [(0,shade('#B8AA92',k)),(.25,shade('#6A615A',k)),(1,shade('#2E2A27',k))]))
    D.path(qstroke((heel,.3),(cx+1.2,.25),(toe,.22)), stroke=shade('#262420',k), sw=.14, so=.8)
    D.path(qstroke((toe-.55,2.25),(toe-.2,1.2),(toe-.4,.2)), stroke='#211E1B', sw=.13, so=.8)
    for y in (.8,1.25,1.7,2.1):
        D.path(qstroke((heel+.15,y),(cx+1.4,y+.1),(toe-.7,y-.1)), stroke=shade('#B8AA92',k), sw=.06, so=.5)
    D.path(cr([(cx-1.0,5.4),(cx-.6,4.3),(cx-.05,4.0),(cx+.15,4.8),(cx-.25,5.65)]), fill=shade('#51463D',k))


def leg(cx, front, far):
    name = 'leg-'+('front-' if front else 'back-')+('far' if far else 'near')
    rng = random.Random(int(cx*13)+(3 if front else 7))
    k = .82 if far else 1
    D.open(name)
    if front:
        pts = [(cx-2.9,SHOULDER+2),(cx+2.9,SHOULDER+2),(cx+2.0,26),(cx+1.5,18),
               (cx+1.7,15.3),(cx+1.45,13),(cx+1.05,10),(cx+1.0,7),
               (cx+1.6,4.2),(cx+2.7,2.5),(cx+.1,2.5),(cx-.7,4.5),
               (cx-1.2,6.5),(cx-1.0,10),(cx-1.1,12.6),(cx-1.5,14.1),
               (cx-1.6,16),(cx-1.6,20),(cx-2.5,30)]
        joint = CARPE
    else:
        pts = [(cx+4.5,HIP+2),(cx+10,HIP),(cx+8,34),(cx+4,27),
               (cx+1.5,21),(cx+1.2,17.6),(cx+1,11),(cx+1,7.5),
               (cx+1.8,4.5),(cx+3,2.5),(cx+.4,2.5),(cx-.5,4.6),
               (cx-1.1,6.7),(cx-.9,11),(cx-1.0,15.5),(cx-1.8,17.8),
               (cx-2.0,19.4),(cx-.7,23),(cx+1.4,30),(cx+2,36)]
        joint = JARRET
    reg = shape(pts)
    D.path(cr(pts,t=.7), fill=D.lin('poil-'+name,(cx-2,0),(cx+2.2,0),
                                  [(0,shade('#A59B87',k)),(.23,shade(HAIR_LT,k)),(.6,shade(HAIR,k)),(1,shade('#A59B87',k))]))
    # Les taches des carpes et des pointes des jarrets sont séparées ; pas
    # de chaussette noire continue. Leur liseré est fait de poils courts.
    center = (cx+(.8 if front else -1),joint+.4)
    spot(center,1.05,1.4,shade(RUST,k),rng,reg,.7)
    for _ in range(35):
        p=add(center,(rng.uniform(-1.1,1.1),rng.uniform(-1.8,1.8)))
        spot(p,rng.uniform(.06,.18),rng.uniform(.13,.34),shade(RUST_LT,k),rng,reg,.75)
    for h in (4.3,8,11.5):
        spot((cx+rng.uniform(-.6,.8),h),rng.uniform(.25,.5),rng.uniform(.4,.7),shade(RUST,k),rng,reg,.85)
    hair(reg,rng,450,far=far)
    D.path(cr([(cx-.7,5.9),(cx-.6,10),(cx-.8,joint-1)],closed=False), stroke=shade('#8D806C',k), sw=.2, so=.4)
    hoof(cx,not front,far,name)
    # Laine du haut du membre, cachée par le corps puis mêlée à sa frange.
    sleeve = shape([(cx-3,39),(cx+4,40),(cx+4,29),(cx+2,26.5),(cx-2,27),(cx-3.5,31)]) if front else shape([(cx-1,43),(cx+11,43),(cx+8,30),(cx+4,26),(cx-.3,28)])
    roots=poisson(sleeve.buffer(-.7),1.6,rng)
    roots.sort(key=lambda p:p[1])
    D.path(geom_d(sleeve),fill=shade('#C0B295',k))
    for x,h in roots:
        lock((x,h),rng.uniform(4,6),rng.uniform(2,2.8),rng.uniform(-.2,.2),.4 if far else .53,rng,clip=sleeve,dirt=.25)
    D.pivot(name,(cx,SHOULDER) if front else (cx+4,HIP))
    D.close()


def tail():
    """Longue corde laineuse, crosse vers l'arrière, en chaîne de trois
    pièces. Les capuchons ronds se recouvrent autour de chaque pivot."""
    rng=random.Random(1985)
    axis=[(1,54),(-1,49),(-3,42),(-4,34),(-4.3,26),(-4.8,21),(-6.3,18.7),(-7.7,18.3),(-8.0,19.5)]
    C=dense(axis,False,8)
    # Chaque sous-ruban est continu et porte toute sa laine à l'articulation.
    def piece(gid,lo,hi):
        seg=C[lo:hi+1]
        normals=[]
        for i,p in enumerate(seg):
            tangent=norm(sub(seg[min(i+1,len(seg)-1)],seg[max(i-1,0)]))
            normals.append((-tangent[1],tangent[0]))
        w=2.4
        pts=[add(p,(n[0]*w,n[1]*w)) for p,n in zip(seg,normals)]
        pts += [add(p,(-n[0]*w,-n[1]*w)) for p,n in zip(seg[::-1],normals[::-1])]
        region=shape(pts).union(Point(seg[0]).buffer(w)).union(Point(seg[-1]).buffer(w))
        D.path(geom_d(region),fill=D.lin('fond-'+gid,(-8,50),(1,30),[(0,'#DDCCAA'),(.6,'#C8B796'),(1,'#BEB093')]))
        roots=poisson(region.buffer(-.4),1.25,rng)
        roots.sort(key=lambda p:p[1])
        for x,h in roots:
            nearest=min(range(len(seg)),key=lambda j:math.dist(seg[j],(x,h)))
            t=norm(sub(seg[min(nearest+2,len(seg)-1)],seg[max(nearest-2,0)]))
            angle=math.atan2(t[0],-t[1])
            lock((x,h),rng.uniform(3.4,5.3),rng.uniform(1.6,2.1),angle,.45+rng.uniform(-.05,.12),rng,clip=region,dirt=.3)
    # Comme pour les pattes : les parties basses sont peintes d'abord,
    # sous les capuchons des parties hautes. Les racines ne font aucun
    # bouchon visible à la crosse quand la queue est au repos.
    D.open('tail')
    D.open('tail-mid')
    D.open('tail-tip')
    piece('queue-crosse',39,len(C)-1)
    D.pivot('tail-tip',C[41])
    D.close()
    piece('queue-milieu',20,42)
    D.pivot('tail-mid',C[23])
    D.close()
    piece('queue',0,24)
    D.pivot('tail',axis[0])
    D.close()



# Profil rectiligne, tête nue, yeux hauts, museau fin, mâchoire en coin.
DORSAL=[(-1,0),(-.6,-1.1),(1,-1.7),(3,-1.7),(6,-1.3),(10,-.9),(14,-.4),(18,.1),(20.4,.6),(21.2,1.3),(21.5,2.1),(21.25,3)]
MOUTH=[(21.25,3),(20.6,3.3),(19,3.7),(17,4.1)]
UNDER=[(17,4.1),(15.9,6.2),(13.7,7.2),(10.3,7.8),(7,8),(4,7.4),(1.3,6),(-.6,3.5),(-1,0)]
HEAD_PTS=HLs(DORSAL+MOUTH[1:]+UNDER[1:-1])
HEAD_POLY=shape(HEAD_PTS)
E=HL(6.2,.7)


def ear(far=False):
    rng=random.Random(705 if far else 704)
    base=HL(1.4,-.2) if far else HL(1.8,.4)
    angle=math.radians(14 if far else 174)
    length=7.4 if far else 9.8  # oreille éloignée raccourcie de profil
    width=3.0 if far else 3.8
    u=(math.cos(angle),math.sin(angle))
    v=(-u[1],u[0])
    def F(a,b):
        return add(base,(u[0]*a+v[0]*b,u[1]*a+v[1]*b))
    out=[F(0,0),F(2,-width*.35),F(length*.7,-width*.52),F(length,-.15),F(length*.8,width*.3),F(3,width*.5)]
    reg=shape(out)
    gid='ear-far' if far else 'ear-near'
    k=.85 if far else 1
    D.open(gid)
    D.path(cr(out),fill=D.lin('oreille-'+gid,F(4,-2),F(4,2),[(0,shade('#BFB3A3',k)),(.4,shade('#F2EDE3',k)),(1,shade('#D2C4AF',k))]))
    inside=[F(.8,.1),F(3,-width*.18),F(length*.78,-width*.26),F(length-.8,-.05),F(length*.72,width*.15),F(3,width*.24)]
    D.path(cr(inside),fill=D.lin('oreille-rose-'+gid,F(4,-1),F(4,1),[(0,shade('#8F5A48',k)),(.55,shade('#C98A74',k)),(1,shade('#E8B9A6',k))]))
    D.path(cr([F(1.2,0),F(4,.2),F(length-1,.1)],closed=False),stroke=shade('#8F5A48',k),sw=.13,so=.5)
    for _ in range(14):
        a=rng.uniform(1,length-.5)
        spot(F(a,rng.choice((-1,1))*width*rng.uniform(.25,.45)),rng.uniform(.07,.18),rng.uniform(.08,.2),RUST,rng,reg,.75)
    hair(reg,rng,180,flow=u,length=(.15,.4),far=far)
    # Pas de numéro d'élevage inventé : une simple petite boucle jaune.
    anchor=F(5.7,0)
    tag=[add(anchor,(-.55,.25)),add(anchor,(.55,.2)),add(anchor,(.5,-1.8)),add(anchor,(-.45,-1.9))]
    D.path(cr(tag,t=.3),fill=shade('#E7C743',k),stroke=shade('#9F882F',k),sw=.09)
    D.path(cr(ell(anchor,.22,.2)),fill=shade('#887537',k))
    D.pivot(gid,base)
    D.close()


def jaw():
    D.open('jaw')
    D.path(cr(HLs([(16,4.2),(17.5,3.6),(19,3.1),(21,2.8),(21.3,3.25),(20.3,3.8),(18,4.3),(16.6,4.6)])),fill='#4F3530')
    D.path(cr(HLs([(17,4.3),(19,3.7),(20.8,3.5),(20.1,4),(18.3,4.5)])),fill='#C18C85')
    chin=HLs([(17,4.1),(19,3.7),(20.6,3.3),(21.25,3),(21.4,3.8),(20.7,4.6),(19.4,5.2),(17.3,6.1),(15.9,6.2),(14,6.7),(13.5,5.8)])
    D.path(cr(chin),fill=D.lin('menton',HL(18,3.4),HL(18,6.4),[(0,HAIR),(.5,HAIR_SH),(1,'#A6A79B')]))
    D.path(cr(HLs([(17.3,4.1),(19,3.7),(20.7,3.35),(21.2,3.4)]),closed=False),stroke='#9E7D73',sw=.22)
    D.pivot('jaw',HL(14.5,5.0))
    D.close()


def eyes():
    eyepts=[add(E,(-1.4,-.12)),add(E,(-.65,.64)),add(E,(.3,.72)),add(E,(1.3,.25)),add(E,(1.4,-.15)),add(E,(.6,-.6)),add(E,(-.5,-.55))]
    # Paupière fermée sous l'œil : visible seulement au clignement.
    D.path(cr(eyepts),fill=HAIR_SH)
    D.path(qstroke(add(E,(-1.2,0)),add(E,(0,-.3)),add(E,(1.2,0))),stroke='#776354',sw=.12)
    D.open('eyes')
    D.path(cr(eyepts),fill='#55432D')
    D.open('pupils')
    D.path(cr(ell(E,.92,.83)),fill=D.rad('iris',add(E,(-.2,.15)),1.05,[(0,'#C9A465'),(.5,'#A8803F'),(.86,'#826330'),(1,'#5C4424')]))
    for k in range(24):
        a=k*math.tau/24
        D.path(lstroke(add(E,(.45*math.cos(a),.45*math.sin(a))),add(E,(.82*math.cos(a+.06),.75*math.sin(a+.06)))),stroke='#D4B579' if k%3 else '#6A4D2C',sw=.04,so=.7)
    # Pupille réellement horizontale dans le monde (elle contre-tourne dans
    # le moteur quand la tête plonge), coins arrondis, ni disque ni fente verticale.
    pts=[add(E,(-.72,.18)),add(E,(.72,.18)),add(E,(.81,0)),add(E,(.72,-.18)),add(E,(-.72,-.18)),add(E,(-.81,0))]
    D.path(cr(pts,t=.35),fill='#121010')
    D.pivot('pupils',E)
    D.close()
    D.path(cr(ell(add(E,(-.35,.34)),.18,.11)),fill='#FFFDF5',fo=.9)
    D.path(cr(eyepts[:4],closed=False),stroke='#5C4939',sw=.18)
    rng=random.Random(811)
    for k in range(15):
        p=add(E,(-1.15+k*.155,.48+.18*math.sin(k/15*math.pi)))
        D.path(qstroke(p,add(p,(.04,.2)),add(p,(.15,rng.uniform(.3,.5)))),stroke='#EEE6D5',sw=.045,so=.85)
    D.pivot('eyes',E)
    D.close()


def head():
    rng=random.Random(570)
    D.open('head')
    ear(True)
    jaw()
    D.path(cr(HEAD_PTS,t=.7),fill=D.lin('face',HL(6,-2),HL(9,8),[(0,HAIR_LT),(.35,HAIR),(.7,HAIR_SH),(1,'#9A9C92')]))
    glow('chanfrein-lumiere',HL(11,0),8,2.4,HAIR_LT,.5,HEAD_POLY)
    glow('masseter',HL(6,4.4),4,3.3,'#F6F0E4',.42,HEAD_POLY)
    glow('creux-joue',HL(11,5.3),4.4,1.6,'#9A9C92',.36,HEAD_POLY)
    glow('cerne',E,2.1,1.35,RUST,.38,HEAD_POLY)
    # Mouchetures : petites et distinctes sur le chanfrein, plus fournies
    # autour du nez. Le fond blanc domine, sans masque ni tête rousse unie.
    for u,v,rx,ry in ((8.5,-.1,.38,.27),(11.4,.4,.26,.45),(14,-.05,.38,.25),(16.3,1.2,.3,.25),(12.8,2.8,.23,.4),(4.4,3.0,.22,.18)):
        spot(HL(u,v),rx,ry,RUST,rng,HEAD_POLY,.8)
    for _ in range(70):
        u,v=rng.uniform(17.1,21.6),rng.uniform(.1,4.3)
        spot(HL(u,v),rng.uniform(.05,.22),rng.uniform(.06,.24),rng.choice((RUST,RUST_LT,RUST_SH)),rng,HEAD_POLY,rng.uniform(.4,.85))
    hair(HEAD_POLY,rng,1500,flow=HU,length=(.18,.45))
    # Nez nu rose marbré et narine oblique en virgule.
    nose=shape(HLs([(20.2,.5),(21.1,1),(21.5,2),(21.25,3),(20.7,3.25),(20.1,2.3)]))
    D.path(geom_d(nose),fill=D.lin('nez',HL(20.5,.5),HL(21.3,3),[(0,'#DDBDB2'),(.55,'#C29A8E'),(1,'#9C7468')]))
    for _ in range(17):
        spot(HL(rng.uniform(20.1,21.5),rng.uniform(.7,3.1)),rng.uniform(.06,.2),rng.uniform(.06,.18),'#7B5F4A',rng,nose,.72)
    D.path(cr(HLs([(20.55,1.2),(20.65,1.75),(20.32,2.25),(19.94,2.25),(20.05,1.9)])),fill='#3A2A26')
    D.path(qstroke(HL(20.35,1.15),HL(20.2,1.6),HL(19.85,1.85)),stroke='#E0BDA8',sw=.13,so=.65)
    D.path(cr(HLs(MOUTH),closed=False),stroke='#614D47',sw=.14,so=.8)
    eyes()
    ear(False)
    D.pivot('head',POLL)
    D.pivot('muzzle',HL(21.3,3.6))
    D.close()


def articulate(svg):
    parser=ET.XMLParser(target=ET.TreeBuilder(insert_comments=True))
    root=ET.fromstring(svg,parser=parser)
    paints=A.Paints(root)
    for name,h in (('leg-front-near',CARPE),('leg-front-far',CARPE),('leg-back-near',JARRET),('leg-back-far',JARRET)):
        joint,half=A.leg_joint(svg,name,OY-h)
        A.split_leg(root,paints,name,joint,half+.3,(0,1))
    lip=HLs([(18.7,2.5),(20,2.15),(21.7,2.1),(21.6,3.5),(19,4.1),(18.5,3.4)])
    pad=HLs([(18.9,2.8),(20.4,2.4),(21.3,2.45),(21.2,3),(20,3.4),(18.9,3.55)])
    gum=[ET.Element(A.Q+'path',{'d':cr(pad),'fill':'#C98B87'})]
    A.lift_part(root,paints,'head',[S(p) for p in lip],'lip',S(HL(18.9,3)),under=gum)
    ET.indent(root,space=' ')
    return ET.tostring(root,encoding='unicode')+'\n'


def main(path):
    leg(HIND+6,False,True)
    leg(FORE-5.6,True,True)
    tail()
    leg(HIND,False,False)
    leg(FORE,True,False)
    fleece('neck',NECK,1988,1.65,3.1,(4.5,8),neck=True)
    fringe()
    fleece('body',BODY,1985,2.15,4,(8,14))
    head()
    D.pivot('ground',(35,0))
    comment=('Landes de Bretagne : brebis blanche sans cornes, en pleine toison de mai.\n'
             '57 cm au garrot, tête nue droite et mouchetée, longues mèches crème,\n'
             'queue laineuse en crosse jusqu’au jarret, articulée en trois pièces.\n'
             'Dessin vectoriel original, écrit par tools/races/landes-de-bretagne/gen_landes_de_bretagne.py.\n'
             '© 2026 Alejandro San Martin Lamas — MIT OR Apache-2.0')
    svg=articulate(D.render(114,87,comment))
    if VARIANTE:
        return variante(path, svg)
    Path(path).parent.mkdir(parents=True,exist_ok=True)
    Path(path).write_text(svg,encoding='utf-8')
    print(f'{path} : {len(svg.encode())/1024:.0f} Ko, {svg.count("<path")} tracés')


def variante(path, svg):
    """Les autres dessins : ce qui change, puis les proportions."""
    root = ET.fromstring(svg)
    if VARIANTE == "belier":
        variantes.bourses(root, *variantes.sous_le_ventre(root, 6, -5), largeur=8, hauteur=13,
                  peau="#E2D3BF", ombre="#A18F79")
    if VARIANTE == "agneau":
        # Plus blanc que sa mère : la toison n'est pas encore salie aux pointes.
        variantes.recolorer(root, variantes.eclaircir(.3), dans=("body", "neck", "tail"),
                    si=lambda c: variantes.clarte(c) > .45)
    variantes.ecrire(path, root, **PROPORTIONS[VARIANTE])


if __name__=='__main__':
    main(sys.argv[1] if len(sys.argv)>1 else 'landes-de-bretagne.svg')
