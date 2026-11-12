#!/usr/bin/env python3
"""Bélier meneur islandais de trois ans, noir à tête d'aigle, en septembre.

Une unité = un centimètre ; x vers l'avant, h vers le haut, sol à h=0.
Fiche : docs/races/islandais.md. Robe de Jökull 22-978 (RML), proportions
de Strumpur 14-815 (ISBONA). Les photos restent hors du dépôt.
Le tog est une nappe de mèches effilées, sans les bouclettes du Gotland.
Les pattes sont recoupées au carpe et au jarret par articulations.py.

Usage : python3 tools/races/islandais/gen_islandais.py pets/islandais/islandais.svg
© 2026 Alejandro San Martin Lamas — MIT OR Apache-2.0
"""
import math
import random
import sys
import xml.etree.ElementTree as ET
from pathlib import Path

HERE = Path(__file__).resolve().parent
sys.path.insert(0, str(HERE.parent))
from shapely.geometry import Point, Polygon
import articulations as A
import variantes  # noqa: E402

# Les autres dessins de la race (tools/races/variantes.py), d'après la fiche.
# Sans variante : le bélier meneur.
VARIANTE = sys.argv[2] if len(sys.argv) > 2 else ""
PROPORTIONS = {
    "brebis": dict(  # Brebis meneuse islandaise : 73 cm au garrot en toison, cornes fines en demi-spirale, cou plus fin, tête plus étroite.
        garrot=(84, 73), encolure=(1.0, .85), tete=(.97, .92)),
    "agneau": dict(  # Agneau islandais de deux semaines : 44 cm au garrot, même robe en bouclettes serrées, sans cornes.
        variantes.AGNEAU, garrot=(84, 44), corps=(.84, .8)),
}
from outils import Doc, S, OY, cr, dense, quad, mix

D = Doc()
R = random.Random(22978)
POLL = (111.0, 101.0)
NECK = (89.0, 55.0)
TH = math.radians(43)


def hl(u, v):
    """Repère de la tête : u nuque→nez, v vers la joue."""
    return (POLL[0] + u * math.cos(TH) - v * math.sin(TH),
            POLL[1] - u * math.sin(TH) - v * math.cos(TH))


def shape(pts, fill, **kw):
    D.el(cr(pts), fill=fill, **kw)


def ellipse(c, rx, ry, fill, angle=0, **kw):
    pts = [(c[0] + rx * math.cos(t) * math.cos(angle) - ry * math.sin(t) * math.sin(angle),
            c[1] + rx * math.cos(t) * math.sin(angle) + ry * math.sin(t) * math.cos(angle))
           for t in [i * math.tau / 24 for i in range(24)]]
    shape(pts, fill, **kw)


def gradient(name, top, bottom, light, base, dark):
    return D.lin(name, top, bottom, [(0, light), (0.46, base), (1, dark)])


def geometry(pts):
    return Polygon(dense(pts, per=5)).buffer(0)


def hair(region, n, color, length=(0.3, 0.9), angle=-1.0, width=0.08, alpha=0.4):
    """Poil ras dans un contour, gardé géométriquement sans clip SVG."""
    x0, h0, x1, h1 = region.bounds
    strokes = []
    for _ in range(n):
        p = (R.uniform(x0, x1), R.uniform(h0, h1))
        a = angle + R.uniform(-0.24, 0.24)
        le = R.uniform(*length)
        q = (p[0] + le * math.cos(a), p[1] + le * math.sin(a))
        if region.contains(Point(p)) and region.contains(Point(q)):
            strokes.append(quad(p, ((p[0]+q[0])/2+0.08, (p[1]+q[1])/2), q, nd=2))
    D.el(''.join(strokes), stroke=color, sw=width, so=alpha, cap='round')


def lock(x, h, length, width, light, white=False, lean=0):
    """Une mèche de tog : racine large, pointe fine, une ondulation lâche.

    Le modelé longitudinal distingue les fibres longues du tog du fond de þel.
    Les variations de longueur évitent les rangées régulières de boucles.
    """
    phase = R.uniform(-1.2, 1.2)
    bend = R.uniform(-1.5, 1.5)

    def axis(t):
        return (x + lean * t + bend * (math.sin(t * 5.8 + phase) - math.sin(phase)) * t,
                h - length * t)

    def side(t, f):
        a, b = axis(t)
        w = width * (1 - t) ** 0.65 * (0.32 + 0.13 * math.sin(t * 11 + phase)) * min(1.0, 0.3+t*8)
        return (a + f * w, b)

    ts = [i/12 for i in range(13)]
    outline = [side(t, -1) for t in ts] + [side(t, 1) for t in ts[::-1]]
    if white:
        col = mix('#867A69', '#E3D8C2', light)
        hi, lo, tip = '#F0E7D1', '#62594F', '#AA9676'
    else:
        col = mix('#19171A', '#4C4447', light)
        hi, lo, tip = '#8C8288', '#100F12', '#5A4636'
    # La racine se fond dans le þel ; aucun bord de « feuille » plaqué.
    gid = 'meche-' + str(len(D.ids))
    D.defs.append(f'<linearGradient id="{gid}" x1="0" y1="0" x2="0" y2="1" gradientUnits="objectBoundingBox">'
                  f'<stop offset="0" stop-color="{col}" stop-opacity="0"/>'
                  f'<stop offset=".27" stop-color="{col}"/>'
                  f'<stop offset=".78" stop-color="{col}"/>'
                  f'<stop offset="1" stop-color="{tip}"/></linearGradient>')
    D.ids.add(gid)
    shape(outline, f'url(#{gid})')
    # Face éclairée et creux : deux rubans qui s'éteignent vers la pointe.
    shape([side(t, -0.7) for t in ts[:-1]] + [side(t, -0.05) for t in ts[-2::-1]], hi, fo=0.025)
    D.line([side(t, 0.8) for t in ts[1:-1]], lo, 0.12, so=0.6)
    # Le soleil roussit les pointes seulement, le noir reste franc à la base.
    if not white:
        shape([side(t, -0.75) for t in ts[7:]] + [side(t, 0.7) for t in ts[:6:-1]],
              tip, fo=R.uniform(0.10, 0.27))
    for f in [-0.5, -0.15, 0.3]:
        D.line([side(t, f) for t in ts[1:-1]], hi, R.uniform(0.045, 0.085), so=R.uniform(0.09, 0.24))


BODY_PTS = [(1, 69), (5, 77), (16, 81.5), (37, 82), (60, 82.4), (80, 84),
            (91, 82), (100, 74), (104, 58), (98, 43), (82, 38.5), (63, 38),
            (43, 38), (24, 39), (9, 43), (2, 53)]
BODY = geometry(BODY_PTS)


def body():
    # Les racines des trois franges sont cachées dans une nappe continue.
    # Rien n'est coupé à travers une mèche : chaque bouquet a sa propre racine.
    for j, (xa, xb) in enumerate([(27, 44), (47, 65), (68, 84)], 1):
        D.open(f'sway-{j}')
        for k in range(23):
            x = xa + (xb-xa) * k/22
            lock(x, 43 + R.uniform(-1, 1), R.uniform(14, 18), R.uniform(3.5, 4.5), R.uniform(0.15, 0.5))
        D.pivot(f'sway-{j}', (xa+xb)/2, 43)
        D.close()
    D.open('body')
    shape(BODY_PTS, gradient('toison', (40, 85), (50, 35), '#4C4447', '#2B282A', '#19171A'))
    # Étagées du ventre vers la raie dorsale : les racines du rang inférieur
    # disparaissent sous le rang suivant, comme le manteau de tog réel.
    for h in [43, 50, 57, 64, 71, 78, 82]:
        x = R.uniform(0, 2)
        while x < 105:
            root = (x, h + R.uniform(-3.2, 3.2))
            if BODY.contains(Point(root)):
                le = R.uniform(10, 17)
                # La bordure du ventre est fournie par sway-* au milieu.
                if root[1] < 46 and 24 < x < 87:
                    le = R.uniform(4.0, 5.5)
                lock(*root, le, R.uniform(3.3, 4.8),
                     min(0.86, 0.12 + root[1]/170 + R.uniform(-0.2, 0.15)), lean=R.uniform(-1.2, 0.7))
            x += R.uniform(0.9, 1.6)
    # Raie dorsale : mèches courtes couchées en arrière, pas un liseré clair.
    for _ in range(180):
        x = R.uniform(9, 88)
        h = 81.3 + 0.023*x + R.uniform(-.6, .3)
        D.line([(x, h), (x-0.8, h+0.15), (x-2.0, h-0.2)], '#8C8288', 0.08, so=R.uniform(0.1, 0.3))
    D.close()


def neck():
    D.open('neck')
    pts = [(72, 76), (88, 82), (99, 94), (106, 103), (114, 102), (117, 91),
           (115, 75), (109, 55), (98, 42), (78, 43), (72, 57)]
    shape(pts, gradient('encolure', (97, 101), (91, 43), '#464044', '#2B282A', '#19171A'))
    for h in [51, 59, 67, 75, 83, 91, 99]:
        for x in [86, 89, 92, 95, 98, 101, 104, 107, 110]:
            if geometry(pts).contains(Point(x, h)):
                lock(x+R.uniform(-1, 1), h, R.uniform(10, 16), 4.2, R.uniform(0.3, 0.7))
    # Les mèches de la crête chevauchent son bord : elle reste poilue
    # jusqu'à la nuque, même dans l'espace découvert en broutant.
    for i in range(48):
        x = 88 + i*18/47
        h = 82 + (x-88)*1.16
        lock(x, h, R.uniform(8, 13), R.uniform(2.5, 3.4), R.uniform(.22, .47), lean=-1)
    # La bavette blanche part de la gorge et s'élargit légèrement au poitrail.
    bib = [(109, 94), (117, 88), (117, 78), (113, 65), (109, 51),
           (102, 45), (101, 57), (104, 75)]
    shape(bib, gradient('bavette', (112, 92), (107, 45), '#D8CEB8', '#BAAD94', '#867A69'))
    hair(geometry(bib), 950, '#EFE4CE', length=(1.5, 5.5), angle=-1.72, width=.085, alpha=.47)
    for h, xa, xb in [(51, 102, 108), (55, 103, 109), (63, 104, 111), (71, 106, 113), (79, 108, 115), (86, 109, 116), (93, 110, 115)]:
        for i in range(18):
            lock(R.uniform(xa, xb), h+R.uniform(-2, 2), R.uniform(11, 18), R.uniform(1.4, 2.4),
                 R.uniform(0.45, 0.9), white=True, lean=R.uniform(-1.5, 0.7))
    D.pivot('neck', *NECK)
    D.close()


def leg(name, x, front, far):
    D.open(name)
    # Épaule/cuisse à 49 cm ; carpe à 19,3, jarret à 25,2 cm.
    jh = 19.3 if front else 25.2
    jx = x if front else x-5.0
    pts = [(x-3.5, 51), (x+4.5, 51), (x+3.0, 36), (jx+2, jh+1),
           (jx+1.6, 7), (jx+3.8, 2.4), (jx+3.7, 0.9), (jx-2.6, 1),
           (jx-2, 5.5), (jx-2.4, jh), (x-4.7, 36)]
    col = gradient(name+'-poil', (x-3, 25), (x+3, 25), '#373338' if not far else '#242229', '#1A191D', '#111115')
    shape(pts, col)
    region = geometry(pts)
    hair(region, 480, '#746B69', length=(0.3, 1), angle=-1.5, alpha=0.2)
    if not front:
        # Balzanes irrégulières : les deux postérieurs, antérieurs noirs.
        white = [(jx-1.75, 14.5), (jx-0.7, 13.5), (jx+0.2, 14.8), (jx+1.9, 15.3),
                 (jx+1.6, 7), (jx+3.6, 3), (jx-2.3, 2.8), (jx-1.9, 6)]
        shape(white, gradient(name+'-blanc', (jx-2, 10), (jx+2, 10), '#F5F0E6' if not far else '#B5AFA3', '#D9D3C7', '#9A978E'))
        hair(geometry(white), 260, '#7D7A74', length=(0.25, 0.7), angle=-1.5, alpha=0.3)
    hoof = [(jx-2.3, 3.2), (jx+1.4, 3.5), (jx+3.9, 1.3), (jx+4.1, 0.3),
            (jx+3.2, 0), (jx-2.4, 0), (jx-2.7, 1.3)]
    shape(hoof, gradient(name+'-onglon', (jx, 4), (jx, 0), '#545253', '#28272B', '#111115'))
    D.line([(jx-2.3, 3), (jx, 3.1), (jx+1.8, 2.3)], '#89817A', 0.12, so=0.65)
    D.line([(jx+2.5, 2.4), (jx+2.3, 0.15)], '#09090C', 0.25)
    # Deux ergots derrière le boulet, très discrets.
    shape([(jx-2.1, 6), (jx-1.5, 5.8), (jx-1.9, 4.5), (jx-2.5, 4.8)], '#302C2B')
    D.pivot(name, x, 49)
    D.close()


def tail():
    D.open('tail')
    # 18 cm de longueur ; base large et bout aplati en fluke. La culotte
    # recouvre l'essentiel au repos, elle se découvre au battement.
    pts = [(6, 69), (2, 68), (0, 59), (0, 52), (2.5, 50.5),
           (5, 53), (6.5, 61)]
    shape(pts, '#272327')
    hair(geometry(pts), 280, '#756B65', length=(0.4, 1.2), angle=-1.5, alpha=0.25)
    D.pivot('tail', 2.5, 68)
    D.close()


def horn(far=False):
    # Un tour ouvert ; la pointe revient vers l'avant et le dehors. Le
    # déport latéral n'est visible qu'en projection, d'où ce bout allongé.
    axis = [(112, 101), (106, 104.5), (101, 102), (98.8, 97), (99.6, 92),
            (103.5, 88.8), (109, 88.5), (113.3, 91.5), (114.3, 95.3),
            (112.5, 99), (110, 100.7), (114, 99.8), (118.1, 96.7)]
    if VARIANTE == "brebis":
        # La brebis : une demi-spirale fine, sans tour complet (§ 1.5).
        axis = axis[:6]
    c = dense(axis, closed=False, per=7)
    if far:
        c = [(x+0.8, h+1.4) for x, h in c]
    left, right, normals, widths = [], [], [], []
    for i, p in enumerate(c):
        a, b = c[max(0, i-1)], c[min(len(c)-1, i+1)]
        dx, dh = b[0]-a[0], b[1]-a[1]
        le = math.hypot(dx, dh)
        n = (-dh/le, dx/le)
        w = (6.4*(1-i/(len(c)-1))**0.85+0.12) if not far else 5.6*(1-i/(len(c)-1))**0.85+0.12
        if VARIANTE == "brebis":
            w *= .55
        normals.append(n)
        widths.append(w)
        left.append((p[0]+n[0]*w/2, p[1]+n[1]*w/2))
        right.append((p[0]-n[0]*w/2, p[1]-n[1]*w/2))
    suffix = 'loin' if far else 'pres'
    shape(left+right[::-1]+[(c[0][0]+2.5, c[0][1]-1.5)], gradient('corne-'+suffix, (103, 105), (109, 88), '#C2B8AE', '#978672', '#403127'))
    ridge = [(p[0]+n[0]*w*0.04, p[1]+n[1]*w*0.04) for p, n, w in zip(c, normals, widths)]
    shape(left+ridge[::-1], '#D1C6B4', fo=0.32)
    D.line(ridge, '#E2D5BC', 0.17, so=0.65)
    dist = 0
    for i in range(1, len(c)-1):
        dist += math.dist(c[i], c[i-1])
        if dist < 0.64+0.12*math.sin(i*1.7) or (not far and i > len(c)*0.88):
            continue
        dist = 0
        p, n, w = c[i], normals[i], widths[i]
        a = (p[0]-n[0]*w*0.47, p[1]-n[1]*w*0.47)
        b = (p[0]+n[0]*w*0.47, p[1]+n[1]*w*0.47)
        mid = (p[0]+0.18, p[1]+0.18)
        D.el(quad(a, mid, b, nd=2), stroke='#403127', sw=0.14, so=0.63, cap='round')
        D.el(quad((a[0]-.15,a[1]+.15), (mid[0]-.15,mid[1]+.15),
                      (b[0]-.15,b[1]+.15), nd=2), stroke='#E1D4C0', sw=0.10, so=0.55, cap='round')
    D.line(left, '#554536', 0.17, so=0.8)
    D.line(right, '#554536', 0.17, so=0.8)


def ear(far=False):
    name = 'ear-far' if far else 'ear-near'
    D.open(name)
    base = (107, 96.3) if not far else (111, 101)
    x, h = base
    pts = [(x+1,h+1), (x-3,h+2), (x-9,h+0.5), (x-11,h-1),
           (x-8,h-3), (x-3,h-2.5), (x,h-1)]
    shape(pts, gradient(name+'-peau', (x-7,h+2), (x-7,h-3), '#6F6455', '#2B2420', '#1E1916'))
    shape([(x-1,h), (x-5,h+.1), (x-9,h-1), (x-6,h-1.8), (x-2,h-1.5)], '#3A2E2E')
    hair(geometry(pts), 160, '#B5A58A', length=(0.25,.6), angle=.1, alpha=.3)
    D.pivot(name, *base)
    D.close()


def head():
    D.open('head')
    if VARIANTE != "agneau":
        horn(True)
    ear(True)
    # Mâchoire sous le profil : elle s'ouvre sans déplacer le nez.
    D.open('jaw')
    jaw = [hl(*p) for p in [(8,6), (17,4.8), (28,3.8), (28,6.5), (22,9), (13,10), (6,8)]]
    shape(jaw, '#3B2426')
    shape([hl(*p) for p in [(15,7), (26,5.5), (26,6.8), (20,8)]], '#A66E70')
    shape([hl(*p) for p in [(7,8), (17,6.9), (28,5.6), (27.6,7), (21,9.2), (12,10)]],
          gradient('mandibule', hl(15,7), hl(15,10), '#E4E0D6', '#CFC4AE', '#9D8D76'))
    shape([hl(*p) for p in [(23,6.2), (28,5.6), (27.6,7), (24.5,8)]], '#2B2420')
    D.pivot('jaw', *hl(8,7))
    D.close()
    outline = [(-1,0), (0,-2), (3,-3.3), (7,-3.3), (12,-2.6), (18,-1.9),
               (24,-.8), (28,1), (29,3), (28.5,5.4), (24,6.5), (20,7),
               (17,8.6), (10,9.5), (4,8), (0,5)]
    facepts = [hl(*p) for p in outline]
    face = geometry(facepts)
    shape(facepts, gradient('face', hl(13,-2), hl(13,9), '#F4F1EA', '#E4E0D6', '#B6A58C'))
    hair(face, 1600, '#938675', angle=-.75, alpha=.24)
    # Marque noire œil/joue en goutte : un vrai masque, pas des lunettes rondes.
    mask = [(1,2), (5,-.4), (10,-.2), (14,2.8), (15,6.2), (11,8.8), (5,7.8), (2,5)]
    maskpts = [hl(*p) for p in mask]
    shape(maskpts, gradient('masque', hl(8,0), hl(9,9), '#52463C', '#2B2420', '#1E1916'))
    hair(geometry(maskpts), 800, '#9B8871', length=(.15,.55), angle=-.6, alpha=.28)
    # Museau séparé du masque par une plage blanche franche.
    muzzle = [(23,-.9), (27,.2), (29,2), (28.5,5.4), (24,6.5), (22,5.1), (23,3)]
    shape([hl(*p) for p in muzzle], gradient('museau', hl(24,0), hl(27,6), '#5F5E59', '#2E2C2A', '#171719'))
    shape([hl(*p) for p in [(26,1.3), (27.5,1.1), (28,2.2), (27.4,3.4), (26.2,3.5), (26.8,2.3)]], '#08090B')
    D.line([hl(26,.9), hl(27.5,.8), hl(28.2,1.8)], '#999185', .15, so=.7)
    D.line([hl(23.5,6.3), hl(26,5.9), hl(28.2,5.4)], '#101012', .18)
    D.open('eyes')
    ec = hl(9.5,1.6)
    ellipse(ec, 1.8, 1.2, '#100F0D', angle=-.12)
    D.open('pupils')
    ellipse(ec, 1.24, 1.03, gradient('iris', (ec[0],ec[1]+1), (ec[0],ec[1]-1), '#9A753D', '#5C3F1C', '#2A1E0D'))
    # La fente reste horizontale dans le repère du monde.
    shape([(ec[0]-1.08,ec[1]+.25), (ec[0]+1.1,ec[1]+.23),
           (ec[0]+1.08,ec[1]-.22), (ec[0]-1.08,ec[1]-.24)], '#0E0C0B')
    D.pivot('pupils', *ec)
    D.close()
    ellipse((ec[0]-.4,ec[1]+.48), .23, .16, '#F1EAD9', fo=.9)
    D.pivot('eyes', *ec)
    D.close()
    D.line([(ec[0]-1.9,ec[1]+.1), (ec[0]-.6,ec[1]+1.35), (ec[0]+1,ec[1]+1.1)], '#A0917B', .15, so=.75)
    if VARIANTE != "agneau":
        horn()
    ear()
    # Toupet blanc court entre les bases ; barbe blanche sous la ganache.
    shape([(109,103.5), (111,104), (113,103), (115,101), (114,99), (111,99.5), (109,101)], '#D8CEB9')
    for i in range(42):
        lock(110+R.uniform(-1.5,3), 103+R.uniform(-.5,1.1), R.uniform(2,4.5), .8, R.uniform(.5,.9), True, lean=1)
    for i in range(21):
        p = hl(R.uniform(8,14), R.uniform(8.8,9.6))
        lock(*p, R.uniform(5,10), R.uniform(.6,1.3), R.uniform(.6,.95), True)
    D.pivot('head', *POLL)
    D.pivot('muzzle', *hl(28.6,5.8))
    D.close()


def articulate(svg):
    def extra(root, paints):
        # Lèvre de 4 cm, avec bourrelet dentaire rose caché au repos.
        area = [S(*hl(*p)) for p in [(24,3.9), (30,3.7), (30,6.6), (23,7.3)]]
        under = A.gum([S(*hl(*p)) for p in [(24,4.3), (28.5,4), (28.3,5.8), (24,6.7)]],
                      [S(*hl(*p)) for p in [(24,6.3), (26,6), (28.2,5.6)]], '#A16F71', '#513437', .15)
        A.lift_part(root, paints, 'head', area, 'lip', S(*hl(24,4.5)), under=under)
    return A.articulate_breed(svg, OY-19.3, OY-25.2, extra=extra)


def main(path):
    leg('leg-back-far', 23, False, True)
    leg('leg-front-far', 89, True, True)
    tail()
    leg('leg-back-near', 15, False, False)
    leg('leg-front-near', 96, True, False)
    neck()
    body()
    head()
    D.pivot('ground', 56, 0)
    text = ('<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 158 120" width="158" height="120">\n'
            '<!-- Islandais : bélier meneur, 3 ans, robe noire à tête d’aigle, septembre.\n'
            '1 unité = 1 cm ; garrot 80 cm, 84 cm avec la laine.\n'
            'D’après docs/races/islandais.md ; généré par tools/races/islandais/gen_islandais.py.\n'
            '© 2026 Alejandro San Martin Lamas — MIT OR Apache-2.0 -->\n'
            '<defs>'+''.join(D.defs)+'</defs>\n'+'\n'.join(D.out)+'\n</svg>\n')
    text = articulate(text)
    if VARIANTE:
        return variante(path, text)
    Path(path).write_text(text, encoding='utf-8')
    print(f'{path} : {len(text.encode())/1024:.0f} Ko, {text.count("<path")} tracés')


def variante(path, svg):
    """Les autres dessins : ce qui change, puis les proportions."""
    root = ET.fromstring(svg)
    pass
    variantes.ecrire(path, root, **PROPORTIONS[VARIANTE])


if __name__ == '__main__':
    main(sys.argv[1] if len(sys.argv)>1 else HERE.parents[2]/'pets/islandais/islandais.svg')
