#!/usr/bin/env python3
"""Le bélier et l'agneau de la Solognote, d'après la brebis peinte.

La Solognote n'est pas un dessin vectoriel : c'est un squelette
(`pets/solognote/squelette.toml`) et ses 27 morceaux peints à l'aquarelle par
`tools/mouflon-peintre`. Ce script en tire les deux autres dessins de la
race, comme `tools/races/variantes.py` le fait pour un rig : chaque partie
(le tronc, les pattes, l'encolure, la tête, les oreilles, la queue) est mise
à ses proportions autour de son articulation, dans l'image cette fois, et le
squelette suit.

- le bélier (fiche, § 1.4) : 70 cm au garrot, sans cornes comme la brebis,
  plus lourd, l'encolure plus épaisse, la tête plus forte, les bourses ;
- l'agneau de deux semaines (§ 4 : « roux de la tête aux pieds ») : 38 cm au
  garrot, haut sur pattes, la tête grosse, la laine roux-chocolat.

Usage, depuis la racine du dépôt :

    python3 tools/races/solognote/variantes_solognote.py [belier] [agneau]

Écrit `pets/solognote/squelette-<variante>.toml`, ses morceaux dans
`pets/solognote/morceaux-<variante>/`, et les sections
`[appearance.male]` et `[appearance.lamb]` de `pets/solognote/pet.toml`.
Repeindre la brebis (`mouflon-peintre --solognote`) demande de le relancer.

© 2026 Alejandro San Martin Lamas — MIT OR Apache-2.0
"""

import math
import re
import sys
import tomllib
from pathlib import Path

import numpy as np
from PIL import Image

sys.path.insert(0, str(Path(__file__).resolve().parents[1]))
import variantes as V  # noqa: E402

RACINE = Path(__file__).resolve().parents[3]
PACK = RACINE / "pets" / "solognote"
PX_PAR_CM = 7.0
GARROT = 65.0  # la brebis peinte, en centimètres
MARGE = 40  # pixels de toile autour du dessin

# Les proportions de chaque dessin, relatives à l'échelle d'ensemble : celles
# de `variantes.transformer`.
PROPORTIONS = {
    "belier": dict(  # Bélier Solognot : 70 cm au garrot, sans cornes, plus lourd, encolure plus épaisse, tête plus forte au chanfrein plus bombé.
        garrot=70.0, corps=(1.03, 1.07), encolure=(1.0, 1.22), tete=(1.05, 1.1)),
    "agneau": dict(  # Agneau Solognot de deux semaines : 38 cm au garrot, roux-chocolat de la tête aux pieds, laine courte.
        V.AGNEAU, garrot=38.0),
}

# La laine de l'agneau (fiche, § 4.3) : sombre, moyenne, claire.
ROUX = ("#4E2C20", "#7A4636", "#9E6E5E")
TOISON = ("corps", "encolure", "meche-1", "meche-2", "meche-3", "queue", "queue-milieu", "queue-bout")


def cartes(os_, sol, *, garrot, corps=(1.0, 1.0), pattes=(1.0, 1.0), encolure=(1.0, 1.0),
           tete=(1.0, 1.0), oreilles=1.0, queue=(1.0, 1.0), port=0.0, port_tete=0.0):
    """La transformation de chaque partie, par le nom de son os (et celle du
    tronc, sous `None`) : le calcul de `variantes.transformer`, sur les os."""
    role = {nom: o.get("role", "") for nom, o in os_.items()}
    en = {nom: tuple(o["at"]) for nom, o in os_.items()}
    jambes = [n for n in os_ if re.fullmatch(r"leg-(front|back)-(near|far)", role[n])]
    xm = sum(en[n][0] for n in jambes) / len(jambes)
    ya = sum(en[n][1] for n in jambes) / len(jambes)
    monte = (pattes[1] - 1.0) * (sol[1] - ya)

    def corps0(p):
        return (xm + corps[0] * (p[0] - xm), ya + corps[1] * (p[1] - ya) - monte)

    haut = sol[1] - corps0((xm, sol[1] - GARROT * PX_PAR_CM))[1]
    s = garrot * PX_PAR_CM / haut
    globale = (s, 0.0, 0.0, s, sol[0] * (1 - s), sol[1] * (1 - s))
    m_corps = V.mul(globale, (corps[0], 0.0, 0.0, corps[1],
                              xm * (1 - corps[0]), ya * (1 - corps[1]) - monte))
    # Sous `None` : le tronc, et ce qu'il porte ; sous "sol" : ce qui reste au
    # sol (le point d'appui, l'os de la racine).
    maps = {None: m_corps, "sol": globale}
    for n in jambes:
        ep, lo = pattes
        px = en[n][0]
        maps[n] = V.mul(globale, (ep, 0.0, 0.0, lo, corps0((px, 0))[0] - ep * px, sol[1] * (1 - lo)))
    cou = next(n for n in os_ if role[n] == "neck")
    crane = next(n for n in os_ if role[n] == "head")
    pn, ph = en[cou], en[crane]
    angle = math.atan2(ph[1] - pn[1], ph[0] - pn[0])
    lin = V.lin_mul(V.rot(-math.radians(port)), V.etire(angle, encolure[0], encolure[1]))
    maps[cou] = V.vers(pn, V.app(m_corps, pn), tuple(s * v for v in lin))
    lin = V.lin_mul(V.rot(-math.radians(port_tete)), (tete[0], 0.0, 0.0, tete[1]))
    maps[crane] = V.vers(ph, V.app(maps[cou], ph), tuple(s * v for v in lin))
    for n in os_:
        if role[n] == "tail" and role.get(os_[n].get("parent"), "") != "tail":
            maps[n] = V.vers(en[n], V.app(m_corps, en[n]), (s * queue[0], 0.0, 0.0, s * queue[1]))
        if role[n] == "ear":
            maps[n] = V.mul(maps[crane], V.vers(en[n], en[n], (oreilles, 0.0, 0.0, oreilles)))
    return maps, s


def carte_de(nom, os_, maps):
    """La transformation de la partie qui porte cet os (la racine, sans
    parent, reste au sol)."""
    if nom == "sol" or (nom in os_ and not os_[nom].get("parent")):
        return maps["sol"]
    while nom and nom not in maps:
        nom = os_[nom].get("parent")
    return maps[nom if nom in maps else None]


def premultiplie(im):
    a = np.asarray(im.convert("RGBA"), dtype=np.float32) / 255.0
    a[..., :3] *= a[..., 3:4]
    return a


def image_de(a):
    a = a.copy()
    alpha = np.clip(a[..., 3:4], 1e-6, 1.0)
    a[..., :3] = np.where(a[..., 3:4] > 1e-4, a[..., :3] / alpha, 0.0)
    return Image.fromarray((np.clip(a, 0.0, 1.0) * 255.0 + 0.5).astype(np.uint8), "RGBA")


def transforme(a, m, taille):
    """L'image (prémultipliée) vue par la transformation `m`, sur une toile
    de `taille` : rendue deux fois plus fin, puis réduite, pour un bord net
    quand le dessin rétrécit."""
    k = 2
    grand = V.mul((k, 0.0, 0.0, k, 0.0, 0.0), m)
    ia, ib, ic, id_, ie, if_ = V.inv(grand)
    w, h = taille
    canaux = []
    for c in range(4):
        plan = Image.fromarray(a[..., c], "F")
        plan = plan.transform((w * k, h * k), Image.AFFINE, (ia, ic, ie, ib, id_, if_),
                              resample=Image.BICUBIC)
        canaux.append(np.asarray(plan.resize((w, h), Image.LANCZOS), dtype=np.float32))
    out = np.clip(np.stack(canaux, axis=-1), 0.0, 1.0)
    out[..., :3] = np.minimum(out[..., :3], out[..., 3:4])
    return out


def hexa(c):
    return np.array([int(c[i:i + 2], 16) / 255.0 for i in (1, 3, 5)], dtype=np.float32)


def bruit(forme, maille, graine):
    """Un bruit doux, de 0 à 1 : les nuages d'un lavis."""
    rng = np.random.default_rng(graine)
    h, w = forme
    petit = rng.random((h // maille + 2, w // maille + 2)).astype(np.float32)
    return np.asarray(Image.fromarray(petit, "F").resize((w, h), Image.BICUBIC), dtype=np.float32)


def flou(a, sigma):
    """Un flou gaussien, par lignes puis par colonnes."""
    r = max(1, int(math.ceil(sigma * 3)))
    noyau = np.exp(-0.5 * (np.arange(-r, r + 1, dtype=np.float32) / sigma) ** 2)
    noyau /= noyau.sum()
    a = np.apply_along_axis(lambda v: np.convolve(v, noyau, mode="same"), 1, a)
    return np.apply_along_axis(lambda v: np.convolve(v, noyau, mode="same"), 0, a).astype(np.float32)


def roussit(morceaux):
    """La laine bise devient roux-chocolat : la clarté de chaque touche choisit
    son ton entre le sombre, le moyen et le clair de la fiche."""
    sombre, moyen, clair = (hexa(c) for c in ROUX)
    lum = lambda a: (a[..., :3] @ np.array([0.299, 0.587, 0.114], dtype=np.float32)) / np.clip(a[..., 3], 1e-6, 1)
    toutes = np.concatenate([lum(morceaux[n])[morceaux[n][..., 3] > 0.5] for n in TOISON])
    bas, milieu, haut = np.percentile(toutes, [4, 50, 97])
    for n in TOISON:
        a = morceaux[n]
        l = lum(a)
        t1 = np.clip((l - bas) / (milieu - bas), 0, 1)[..., None]
        t2 = np.clip((l - milieu) / (haut - milieu), 0, 1)[..., None]
        ton = np.where(l[..., None] < milieu, sombre + (moyen - sombre) * t1, moyen + (clair - moyen) * t2)
        a[..., :3] = ton * a[..., 3:4]


def bourses(morceaux):
    """Les bourses du bélier, peintes sur la bande de ventre nue : une poire
    de peau châtain entre les postérieurs, jusqu'au jarret, au bord plus
    foncé comme un lavis qui sèche. Le haut reste sous la toison."""
    a = morceaux["ventre"]
    h, w = a.shape[:2]
    peau = a[..., :3][a[..., 3] > 0.9] if (a[..., 3] > 0.9).any() else a[..., :3][a[..., 3] > 0.5]
    base = np.median(peau, axis=0)
    yy, xx = np.mgrid[0:h, 0:w].astype(np.float32)
    cx, haut, bas, large = 381.0, 606.0, 704.0, 29.0
    t = (yy - haut) / (bas - haut)
    # Une poire : le col étroit sous le ventre, le plus large aux deux tiers,
    # le dessous arrondi.
    monte = np.clip(t / 0.62, 0, 1)
    col = 0.46 + 0.54 * monte * monte * (3 - 2 * monte)
    rond = np.sqrt(np.clip(1 - ((t - 0.62) / 0.38) ** 2, 0, 1))
    demi = large * np.where(t < 0.62, col, rond)
    axe = cx + 2.5 * np.sin(t * 2.4)
    dans = np.where((t >= 0) & (t <= 1), demi - np.abs(xx - axe), -1.0)
    masque = np.clip(dans / 1.6 + (bruit((h, w), 7, 71) - 0.5) * 1.2, 0, 1)
    # Seul le voisinage des bourses est peint.
    zone = (slice(int(haut) - 20, int(bas) + 20), slice(int(cx - large) - 20, int(cx + large) + 20))
    doux = np.zeros_like(masque)
    doux[zone] = flou(masque[zone], 0.9)
    large_flou = np.zeros_like(masque)
    large_flou[zone] = flou(masque[zone], 5.0)
    masque, bord = doux, doux - large_flou
    nuage = bruit((h, w), 14, 72)
    ton = base[None, None, :] * (0.93 + 0.14 * nuage[..., None]) * (1.0 - 0.34 * np.clip(bord, 0, 1)[..., None])
    # Le pli du milieu, et l'ombre du dessous.
    pli = np.exp(-((xx - cx - 1.0) / 2.0) ** 2) * np.clip((t - 0.35) / 0.4, 0, 1) * 0.16
    ombre = np.clip((t - 0.6) / 0.4, 0, 1) * 0.16
    ton = ton * (1.0 - pli - ombre)[..., None]
    alpha = masque * 0.97
    a[..., :3] = ton * alpha[..., None] + a[..., :3] * (1 - alpha[..., None])
    a[..., 3] = alpha + a[..., 3] * (1 - alpha)


def ecrire(variante):
    proportions = dict(PROPORTIONS[variante])
    texte = (PACK / "squelette.toml").read_text()
    squelette = tomllib.loads(texte)
    os_ = {o["name"]: o for o in squelette["bone"]}
    sol = tuple(squelette["ground"])
    maps, s = cartes(os_, sol, **proportions)

    morceaux = {}
    for piece in squelette["piece"]:
        nom = Path(piece["image"]).stem
        morceaux[nom] = premultiplie(Image.open(PACK / piece["image"]))
    if variante == "belier":
        bourses(morceaux)
    if variante == "agneau":
        roussit(morceaux)

    # Le cadre : les limites de chaque morceau, vues par sa transformation.
    cadre = [math.inf, math.inf, -math.inf, -math.inf]
    for piece in squelette["piece"]:
        a = morceaux[Path(piece["image"]).stem]
        ys, xs = np.nonzero(a[..., 3] > 0.02)
        m = carte_de(piece["bone"], os_, maps)
        for x in (xs.min(), xs.max() + 1):
            for y in (ys.min(), ys.max() + 1):
                px, py = V.app(m, (float(x), float(y)))
                cadre = [min(cadre[0], px), min(cadre[1], py), max(cadre[2], px), max(cadre[3], py)]
    dx, dy = round(MARGE - cadre[0]), round(MARGE - cadre[1])
    taille = (int(math.ceil(cadre[2] + dx + MARGE)), int(math.ceil(cadre[3] + dy + MARGE)))
    pose = (1.0, 0.0, 0.0, 1.0, float(dx), float(dy))

    dossier = PACK / f"morceaux-{variante}"
    dossier.mkdir(exist_ok=True)
    limites = [math.inf, math.inf, -math.inf, -math.inf]
    for piece in squelette["piece"]:
        nom = Path(piece["image"]).stem
        m = V.mul(pose, carte_de(piece["bone"], os_, maps))
        image = image_de(transforme(morceaux[nom], m, taille))
        image.save(dossier / f"{nom}.png", optimize=True)
        if piece.get("eyes") != "closed":
            alpha = np.asarray(image)[..., 3]
            ys, xs = np.nonzero(alpha > 8)
            limites = [min(limites[0], xs.min()), min(limites[1], ys.min()),
                       max(limites[2], xs.max() + 1), max(limites[3], ys.max() + 1)]

    def point(nom, p):
        x, y = V.app(V.mul(pose, carte_de(nom, os_, maps)), p)
        return f"[{x:.1f}, {y:.1f}]"

    # Le squelette : le même texte, chaque point à sa nouvelle place.
    lignes, courant = [], None
    for ligne in texte.splitlines():
        if m := re.match(r'name = "([^"]+)"', ligne):
            courant = m.group(1)
        if ligne.startswith("[[piece]]"):
            courant = None
        if ligne.startswith("at = ") and courant:
            ligne = f"at = {point(courant, os_[courant]['at'])}"
        elif ligne.startswith("size = "):
            ligne = f"size = [{taille[0]}, {taille[1]}]"
        elif ligne.startswith("ground = "):
            ligne = f"ground = {point('sol', sol)}"
        elif ligne.startswith("muzzle = "):
            crane = next(n for n, o in os_.items() if o.get("role") == "head")
            ligne = f"muzzle = {point(crane, squelette['muzzle'])}"
        elif ligne.startswith("image = "):
            ligne = ligne.replace("morceaux/", f"morceaux-{variante}/")
        lignes.append(ligne)
    corps = "\n".join(lignes)
    corps = corps[corps.index("size = "):]
    commentaire = re.search(rf'"{variante}"\s*:\s*dict\(\s*#\s*([^\n]+)', Path(__file__).read_text()).group(1).strip()
    entete = (f"# {commentaire}\n"
              "# Tiré de la brebis peinte (squelette.toml et morceaux/) par\n"
              "# tools/races/solognote/variantes_solognote.py : ne pas le modifier à la main.\n"
              f"# Sept pixels par centimètre ; le sol est à y = {sol[1] + dy:.0f}.\n")
    (PACK / f"squelette-{variante}.toml").write_text(entete + corps + "\n")

    sol_y = sol[1] + dy
    avant = next(n for n, o in os_.items() if o.get("role") == "leg-front-near")
    patte = sol_y - (V.app(carte_de(avant, os_, maps), os_[avant]["at"])[1] + dy)
    # La hauteur affichée est celle de la boîte du dessin, comme le moteur
    # la mesure (du haut des oreilles au bas des onglons).
    mesures = {"height": [(limites[3] - limites[1]) / PX_PAR_CM * 0.5],
               "width": [(limites[2] - limites[0]) / PX_PAR_CM * 0.5],
               "stride": [patte / PX_PAR_CM * 0.8]}
    print(f"squelette-{variante}.toml : toile {taille[0]} × {taille[1]}, échelle {s:.3f}, "
          f"height {mesures['height'][0]:.3f}, width {mesures['width'][0]:.3f}, "
          f"stride {mesures['stride'][0]:.3f}")
    return mesures, commentaire


def main(argv):
    dessins = []
    for variante in argv[1:] or list(PROPORTIONS):
        mesures, commentaire = ecrire(variante)
        dessins.append((variante, f"squelette-{variante}.toml", mesures, commentaire))
    V.maj_pet_toml("solognote", dessins, cle="skeleton")


if __name__ == "__main__":
    main(sys.argv)
