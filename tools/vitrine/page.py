"""Écrit la page de présentation de Mouflon : `python3 tools/vitrine/page.py <dossier>`.

<dossier>/races/ vient de rendre_races.py ; la page va dans <dossier>/site/index.html,
avec les images copiées dans <dossier>/site/races/ (troupeau.webp et troupeau.mp4
s'y posent à la main, voir le README)."""
import html
import json
import pathlib
import shutil
import sys

V = pathlib.Path(sys.argv[1])
races = {r["id"]: r for r in json.loads((V / "races/races.json").read_text())}
(V / "site/races").mkdir(parents=True, exist_ok=True)

# id : (origine, trait, garrot du bélier, de la brebis et de l'agneau, en cm),
# d'après la fiche de la race et les proportions de son générateur. De la
# plus petite à la plus grande (le garrot du bélier) : l'ordre de la frise.
FICHES = {
    "ouessant": ("Ouessant, Bretagne", "L'un des plus petits moutons du monde ; noir, toison roussie au soleil.", (47, 43, 26)),
    "skudde": ("Prusse orientale", "Crinière au cou, cornes ambrées en colimaçon.", (58, 52, 31)),
    "landes-de-bretagne": ("Bretagne", "Petite et rustique, tête mouchetée de roux.", (62, 57, 34)),
    "badger-face-welsh": ("Pays de Galles", "Type Torddu : ventre et face soulignés de noir.", (64, 58, 35)),
    "manech-tete-rousse": ("Pays basque", "Tête fauve, longues mèches blanches qui pendent.", (65, 60, 36)),
    "southdown": ("Sussex, Angleterre", "Un tonneau de laine serrée, museau souris ; son agnelle est la mascotte.", (67, 60, 45)),
    "solognote": ("Sologne, France", "Peinte à l'aquarelle, posée sur un squelette ; l'agneau naît roux.", (70, 65, 38)),
    "romanov": ("Volga, Russie", "Toison courte gris bleuté, grandes oreilles dressées.", (70, 65, 37)),
    "herdwick": ("Lake District", "Gris bleuté, tête blanc givré ; montagnard.", (70, 63, 38)),
    "dorper": ("Afrique du Sud", "Poil ras, tête et cou noirs.", (70, 63, 38)),
    "scottish-blackface": ("Écosse", "Face noire, cornes miel en spirale, longue laine.", (70, 63, 38)),
    "heidschnucke": ("Lande de Lunebourg", "Cape noire, mèches argentées.", (72, 67, 40)),
    "swaledale": ("Yorkshire", "Face noire, museau blanc, yeux cerclés.", (72, 62, 37)),
    "texel": ("Texel, Pays-Bas", "Massif, gigots ronds, nez noir.", (72, 68, 41)),
    "racka": ("Puszta, Hongrie", "Cornes droites vrillées, toison en cordelettes.", (72, 66, 40)),
    "bleu-du-maine": ("Maine, France", "Tête nue bleu ardoise ; intégrée à l'app.", (74, 68, 41)),
    "karakul": ("Asie centrale", "Des déserts ; queue grasse, tête busquée.", (75, 66, 41)),
    "awassi": ("Croissant fertile", "Queue grasse en coussin, longues oreilles.", (75, 68, 41)),
    "jacob": ("Parcs anglais", "Quatre cornes, toison pie.", (75, 67, 40)),
    "mouflon-corse": ("Corse", "Sauvage : selle blanche, cornes lourdes.", (75, 65, 39)),
    "thones-et-marthod": ("Savoie", "Lunettes noires, toison bouclée.", (76, 65, 39)),
    "gotland": ("Gotland, Suède", "Boucles gris argenté lustrées.", (78, 70, 42)),
    "lacaune": ("Aveyron et Tarn", "La laitière du roquefort.", (78, 70, 42)),
    "nez-noir": ("Valais, Suisse", "Masque noir sous un toupet blanc.", (80, 75, 45)),
    "merinos-d-arles": ("Crau, Provence", "Le mouton de la transhumance ; le bélier, « banard », porte de grandes cornes en spirale.", (82, 67, 40)),
    "islandais": ("Islande", "Un meneur : longues pattes, mèches roussies.", (84, 73, 44)),
    "suffolk": ("Suffolk, Angleterre", "Grand, sans cornes, tête et pattes noires.", (85, 75, 45)),
    "zwartbles": ("Frise, Pays-Bas", "Noir chocolat, liste blanche, balzanes.", (90, 80, 48)),
}
# Un animal à part : un seul dessin.
SEULS = {
    "floucat": ("Crau, Provence", "Le mâle castré du Mérinos d'Arles qui mène la transhumance, son redon au cou.", "Meneur", 75),
}
DESSINS = (("belier", "bélier"), ("brebis", "brebis"), ("agneau", "agneau"))
PX_PAR_CM = 3.0  # celui de rendre_races.py
SCALE = 1.9  # pixels par centimètre dans la frise
e = html.escape


def image(rid, dessin, largeur=None):
    """L'image d'un dessin ; `largeur` (px) : à l'échelle de la frise."""
    d = races[rid]["dessins"][dessin]
    name = races[rid]["name"]
    alt = f"{name}, {dict(DESSINS).get(dessin, 'le meneur')}"
    taille = ""
    if largeur:
        taille = f' width="{largeur}" height="{round(d["h"] / PX_PAR_CM * SCALE)}" style="width:{largeur}px"'
    return f'<img src="races/{rid}-{dessin}.webp"{taille} alt="{e(alt)}" loading="lazy">'


def frise():
    items = []
    for rid, (_, _, garrots) in FICHES.items():
        betes = []
        for (dessin, nom), garrot in zip(DESSINS, garrots):
            w = round(races[rid]["dessins"][dessin]["w"] / PX_PAR_CM * SCALE)
            betes.append(f'<span class="bete">{image(rid, dessin, w)}<small>{nom}<br>{garrot} cm</small></span>')
        items.append(
            f'<figure class="famille"><div class="betes">{"".join(betes)}</div>'
            f'<figcaption>{e(races[rid]["name"])}</figcaption></figure>'
        )
    return "\n".join(items)


def cartes():
    out = []
    tous = {**{rid: None for rid in FICHES}, **{rid: None for rid in SEULS}}
    for rid in sorted(tous, key=lambda k: races[k]["name"].lower()):
        name = races[rid]["name"]
        if rid in SEULS:
            origine, trait, genre, garrot = SEULS[rid]
            vignette = f'<span style="--p:100">{image(rid, "seul")}</span>'
            mesure = f"{genre} · {garrot} cm au garrot"
        else:
            origine, trait, garrots = FICHES[rid]
            dessins = races[rid]["dessins"]
            total = sum(dessins[d]["w"] for d, _ in DESSINS)
            # Chacun à la même échelle : sa part de la largeur de la vignette.
            vignette = "".join(
                f'<span style="--p:{100 * dessins[d]["w"] / total:.2f}">{image(rid, d)}</span>' for d, _ in DESSINS
            )
            mesure = " · ".join(f"{nom} {g} cm" for (_, nom), g in zip(DESSINS, garrots))
        cls = "carte seule" if rid in SEULS else "carte"
        out.append(
            f'<article class="{cls}"><div class="vignette">{vignette}</div>'
            f'<h3>{e(name)}</h3><p class="mesure">{mesure}</p>'
            f'<p class="origine">{e(origine)}</p><p>{e(trait)}</p></article>'
        )
    return "\n".join(out)


for webp in (V / "races").glob("*.webp"):
    shutil.copy2(webp, V / "site/races" / webp.name)
# L'icône de l'app, pour l'onglet.
from PIL import Image  # noqa: E402

icone = Image.open(pathlib.Path(__file__).resolve().parents[2] / "packaging/macos/AppIcon.png")
icone.resize((180, 180), Image.LANCZOS).save(V / "site/icone.png")

page = (pathlib.Path(__file__).with_name("gabarit.html")).read_text()
# Ce que montre la vidéo du haut (`mouflon-preview pets <dossier> film`, voir
# le README) : les rôles et les instants de `tools/mouflon-preview/src/film.rs`.
# À changer avec elle.
FILM = "le meneur (floucat) du Mérinos d'Arles, un mouflon de Corse, un bélier Jacob, une agnelle Southdown, un agneau d'Ouessant et une brebis Nez noir du Valais"
SCENES = [
    (0.0, "Ils tombent du haut de l'écran : le meneur du Mérinos d'Arles, un mouflon de Corse, un bélier Jacob."),
    (2.6, "Le meneur broute, sa cloche au cou. Le mouflon, lui, a repéré une fenêtre plus haute…"),
    (4.8, "… et s'y hisse d'un bond, comme sur un rocher. En bas, le bélier Jacob recule, et charge."),
    (7.2, "Deux agneaux débarquent : une Southdown et un petit Ouessant."),
    (9.0, "Trois pas de recul, et coup de front !"),
    (12.4, "Puis des cabrioles, parce que pourquoi pas."),
    (13.8, "Une brebis Nez noir du Valais passait par là. On l'attrape…"),
    (16.4, "… et on la lance. Elle se relève, pas rancunière."),
    (18.2, "Un coup de vent : les feuilles volent, le mouflon s'éclipse, les autres font le dos rond."),
    (21.4, "La journée a été longue. Le meneur s'endort."),
]
ALT = f"Six moutons ({FILM}) tombent sur un bureau. L'un broute, le mouflon saute sur une fenêtre plus haute, le bélier charge, les agneaux jouent à coups de front, le curseur attrape une brebis et la lance, un coup de vent passe, le meneur s'endort."
if (V / "site/troupeau.mp4").exists():
    film = f'<video id="film" src="troupeau.mp4" poster="troupeau.webp" autoplay muted loop playsinline aria-label="{e(ALT)}"></video>'
    scene = SCENES[0][1]
    legende = f"Images du vrai moteur, sans retouche. Dans l'ordre d'apparition : {FILM}."
else:
    film = f'<img src="troupeau.webp" width="960" height="600" alt="{e(ALT)}">'
    scene = "Six moutons sur un bureau, chacun à son affaire."
    legende = "Image du vrai moteur, sans retouche."
page = (
    page.replace("{{FRISE}}", frise())
    .replace("{{CARTES}}", cartes())
    .replace("{{FILM}}", film)
    .replace("{{LEGENDE_FILM}}", e(legende))
    .replace("{{SCENE_FILM}}", e(scene))
    .replace("{{SCENES}}", json.dumps(SCENES, ensure_ascii=False).replace("</", "<\\/"))
)
assert "{{" not in page, "un repère du gabarit n'est pas rempli"
(V / "site/index.html").write_text(page)
print(len(page), "octets,", len(list((V / "site/races").glob("*.webp"))), "images")
