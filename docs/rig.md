# Dessiner un personnage pour Mouflon

Un personnage Mouflon est un **dessin SVG découpé en calques nommés**. Le
moteur anime ces calques lui-même : marche au pas (un pied après l'autre, les
sabots posés sans glisser), trot par paires diagonales et galop transverse,
couché (l'avant se plie d'abord, il se relève par l'arrière), sommeil (la
tête posée au sol, une patte qui tressaille), broutage la tête au sol,
rumination, sauts les pattes repliées, raides ou en ruant (il se ramasse
avant de bondir),
atterrissage, regard, respiration, clignements, oreilles qui frémissent et
queue qui bat de temps en temps, et une dizaine de gestes (s'ébrouer,
s'étirer, se gratter, bâiller, bêler, taper du pied, le flehmen…). Tu
dessines une seule pose de profil, il fait le reste.

Avec ses **articulations** (voir plus bas), il fait mieux encore : l'encolure
descend pour brouter et pose le museau dans l'herbe, se redresse aux aguets ;
le carpe et le jarret plient en marchant, la patte d'appui reste droite ;
couché, les antérieurs se replient sous le poitrail et les postérieurs le
long du ventre ; la lèvre se retrousse ; la toison traîne et balance ; une
cloche pend de son collier et se pose sur l'herbe.

Le vectoriel n'a pas de résolution : dessine à la taille qui te convient, le
moteur le met à l'échelle (`height` dans `pet.toml`) et il reste net sur
n'importe quel écran Retina, même agrandi.

## Règles de base

- Le personnage regarde vers la **droite**.
- L'ordre des calques est l'ordre de dessin : ce qui est en bas de la liste
  est dessiné en premier (derrière).
- Les articulations se chevauchent : le haut des pattes rentre sous le corps,
  le cou sous la tête. Sinon, les rotations ouvrent des trous.
- L'encolure se dessine dans `neck` (voir « Les articulations ») : elle porte
  la tête et descend avec elle pour brouter (voir `pets/mouflon-corse`, dont
  le collier est l'encolure).
- Pas d'ombre portée ni de reflet « collés » sous le personnage : le moteur
  ajoute une ombre douce au sol et une lumière dynamique sur le corps. Elle
  éclaire toute la toison (le corps, l'encolure, les mèches) comme une seule
  silhouette au repos ; chaque suite de tracés de la toison en garde sa
  part, cuite avec elle ou posée avec elle dans l'aperçu, et elle ne tombe
  que sur ce que la suite dessine : une encolure qui descend brouter emporte
  la sienne, sans laisser de voile au garrot.

## Calques animés

Chaque partie est un groupe (`<g>`) dont l'`id` est un des noms ci-dessous.
Toutes sont facultatives.

| `id` du groupe | Ce que le moteur en fait | Pivot attendu |
|---|---|---|
| `body` | écrasement, respiration, rebond de la marche, éclairage dynamique | bas du corps |
| `head` | regard, hochement au pas, tête basse en dormant, au sol pour brouter | le cou ; la nuque s'il y a une encolure |
| `ear-near`, `ear-far` | battent en tombant, frémissent, tombent en dormant | base de l'oreille |
| `eyes` (dans `head`) | clignements, yeux fermés en dormant | centre des yeux |
| `pupils` (dans `eyes`) | suivent le regard, et restent à l'horizontale quand la tête penche (la pupille du mouton est une fente horizontale ; son œil tourne d'une cinquantaine de degrés en broutant) : mets-y l'iris et la fente, et les reflets après le groupe | centre de la pupille |
| `jaw` (dans `head`) | s'ouvre pour mâcher, ruminer, bêler, bâiller | la charnière, sous la joue, à hauteur de la bouche |
| `tail` | balancement, coups de queue | base de la queue |
| `tail-mid` (dans `tail`), `tail-tip` (dans `tail-mid`) | une longue queue en morceaux : chacun suit le précédent avec un temps de retard, elle ondule ; couché, si elle touche le sol, chaque morceau se plie vers l'arrière du même angle et elle se pose sur l'herbe | l'attache de chaque morceau |
| `leg-front-near`, `leg-back-near` | pas, galop, sauts, pattes repliées couché | hanche ou épaule |
| `leg-front-far`, `leg-back-far` | idem, décalées dans le cycle | hanche ou épaule |
| `neck` | l'encolure, qui porte la tête (`head` dedans) : elle descend pour brouter, se redresse aux aguets, se tend quand il fonce, s'allonge vers l'avant quand il dort la tête au sol | la base de l'encolure, devant le garrot, dans l'épaule |
| `leg-front-near-lower`… (dans sa patte) | le bas de chaque patte (le canon et le sabot) : il plie au carpe (devant) ou au jarret (derrière) | le carpe ou le jarret |
| `lip` (dans `head`) | la lèvre supérieure : elle se retrousse pour le flehmen | son arrière, sous le naseau |
| `sway-1`, `sway-2`, `sway-3` (dans `body`, ou juste avant lui) | des mèches de la toison : elles traînent quand il court, balancent au rythme des pas, penchent dans le vent, suivent la secousse quand il s'ébroue | le milieu de leur attache, en haut |
| `neck-sway-1`, `neck-sway-2`, `neck-sway-3` (dans `neck`) | de petites touffes d'encolure, ou des nappes de longue laine : leur attache suit le cou, leurs fibres pendent quand il broute et balancent avec la toison (voir « Les touffes d'encolure » et « Les nappes ») | un point à la racine de chaque touffe, recouvert par la laine courte ; pour une nappe, sur l'axe de l'encolure, en son milieu |
| `bell` (dans `neck`, à sa place dans le dessin) | une cloche pendue au collier : son anse suit le collier, mais elle pend à la verticale au lieu de tourner avec le cou, se balance avec le mouvement, et se pose sur l'herbe quand le cou descend (voir « Une cloche ») | là où elle pend de sa courroie, au milieu de l'anse |
| `clapper` (dans `bell`) | le battant de la cloche : pendu dedans, il reste à la verticale quand elle penche ou se balance, et bute contre sa paroi | là où il est pendu, haut dans la cloche |

Et un point qui n'est pas une partie : `pivot-muzzle`, ce qui touche l'herbe
quand il broute (le bout du museau, sous les naseaux). Sans lui, le moteur
le devine près du coin du bas et de l'avant de la tête.

`near` = côté visible (devant le corps), `far` = côté caché (derrière le corps).

La mâchoire se dessine **sous** la tête (avant elle dans le groupe `head`) :
le bas du museau, et au-dessus, cachés par la tête tant que la bouche est
fermée, l'intérieur de la bouche (langue en bas) et la joue qui s'étire. Le
tracé de la tête a l'encoche de la mâchoire, sans contour le long de
l'encoche : bouche fermée, on ne voit rien ; ouverte, l'intérieur apparaît
entre les lèvres. Voir le groupe `jaw` de `pets/bleu-du-maine/bleu-du-maine.svg`.

## Les articulations

Toutes sont facultatives : **un dessin qui ne les a pas bouge exactement
comme avant** (le test `sans_changement` le vérifie à 10⁻⁷ près, et
que ses images cuites restent les mêmes). Le Bleu du Maine
(`pets/bleu-du-maine/bleu-du-maine.svg`) les a toutes : c'est l'exemple à suivre.

**L'encolure** (`neck`). Deux façons de la ranger, le moteur fait suivre la
tête dans les deux cas :

- `neck` porte `head` (`<g id="neck">…<g id="head">…</g></g>`), à la place
  qu'avait la tête dans le document : l'encolure est dessinée par-dessus le
  corps. C'est le plus simple quand le cou est une pièce à part, au contour
  fini (le collier du mouflon).
- ou bien `neck` avant `body`, **sous** le corps, et `head` à part, après le
  corps comme avant : le corps recouvre la base de l'encolure, comme une
  marionnette découpée. C'est ce qu'il faut quand l'encolure sort de la
  toison (le Bleu du Maine, le Nez noir, le Jacob, l'Ouessant).

`pivot-neck` est la base de l'encolure : un point dans l'épaule, assez bas
pour que l'encolure et la tête, mises bout à bout, atteignent le sol devant
les antérieurs (le test `troupeau` le vérifie). `pivot-head` est alors la
**nuque** (derrière les oreilles, à la base des cornes), et non plus la base
du cou : la tête y tourne. Le moteur répartit entre les deux ce que faisait
la tête d'une pièce : pour brouter, une cinématique inverse à deux
articulations pose le museau au sol, à peu près une longueur de tête devant
les antérieurs (moins si l'encolure est courte), et c'est surtout
l'encolure qui descend. Seule
l'encolure s'étire (jusqu'à 12 %) ; la tête reste d'une pièce.

Dessine l'encolure plus longue que ce qui se voit au repos : ce qui sera
découvert quand elle bouge doit exister, caché sous le corps ou la tête
(les races du dépôt ont une épaisseur de laine cachée au fond de
l'encolure).

Endormi la tête au sol, l'encolure s'allonge vers l'avant et le menton se
pose sur l'herbe. Ce qu'elle dessine elle-même au-delà du premier tiers de
sa longueur (de `pivot-neck` vers `pivot-head`) : un collier, une gorge
laineuse, une barbe, ne passe pas sous l'herbe : s'il y touche le premier,
la tête s'arrête là, posée dessus. Ce qu'elle dessine derrière sa base ou
dans son premier tiers (la laine qui recouvre son raccord au poitrail)
n'arrête pas la tête : une vraie encolure y plie au lieu de tourner. Cette
laine-là ne passe pas sous l'herbe non plus : l'encolure, qui tourne d'une
pièce, la balaierait vers le sol en descendant, alors le corps se couche
un peu plus haut que sur son ventre (de ce qu'il faut, mesuré au
chargement sur les poses du mouton couché : rien pour presque tous les
dessins, 2 centièmes de la hauteur pour le Texel), et, endormi la tête au
sol, l'animal entier remonte d'un rien (au plus 1,5 centième) si son
premier tiers y touche. Le moteur lit ce dessin une fois, au chargement ;
les touffes et les nappes (`neck-sway-*`) n'en font pas partie, elles se
tassent sur l'herbe ; ni la cloche (`bell`), qui se pose seule.

**Les bas de pattes** (`leg-front-near-lower` dans `leg-front-near`, et de
même pour les trois autres). Le haut de la patte va de l'épaule ou de la
hanche au carpe ou au jarret ; le bas, du carpe ou du jarret au sabot, avec
le sabot. Pour que rien ne s'ouvre au genou :

- le bas se dessine **en premier** dans le groupe de la patte (dessous), et
  son `pivot-…-lower` est au centre de l'articulation ;
- le haut se termine en **demi-disque** autour de ce pivot, un peu plus
  large que la patte, sans trait sur ce bord ; le bas commence par l'arc du
  même cercle (il glisse sous le haut en tournant) ;
- dessine la patte droite ou à peine pliée au repos : le carpe pointe vers
  l'avant (le canon part en arrière en pliant), le jarret vers l'arrière (le
  canon part vers l'avant).

Le moteur pose chaque sabot là où il doit être (posé, il recule exactement
de ce que le corps avance : il ne glisse pas) et plie le genou de lui-même ;
à l'appui, la patte reste droite (elle se tasse de 7 % avant de plier) ; en
vol, le carpe et le jarret plient ; couché, les antérieurs se replient sous
le poitrail et les postérieurs le long du ventre (voir plus bas) ; en l'air,
les pattes se replient ; à l'atterrissage, le corps descend et les genoux
amortissent ; chaque geste plie les bonnes articulations (gratter le sol,
taper du pied, se cabrer, se gratter d'un postérieur, s'étirer la patte
tendue).

**Couché**, le carpe se pose au sol sous le poitrail, son bord au plus au
devant de la poitrine (la boîte du `body`), le jarret sous la fesse ; le
canon se replie dessous, vers l'arrière ou vers l'avant, le sabot un peu
relevé. Le haut de la patte, plus long que l'épaule ou la hanche n'est haute
au-dessus du sol, rentre sous le corps : il glisse le long de lui-même, et
le corps le cache, tant que son bord du haut reste sous le dos (avec 2 % de
la hauteur du dessin de marge) ; au-delà, il se raccourcit. Pour que ce soit
juste :

- dessine les quatre pattes **avant** `body` dans le document (dessous) ;
  une patte dessinée par-dessus le corps (celles du côté qu'on voit de
  l'Ouessant, dont la laine monte sur le flanc) se raccourcit au lieu de
  glisser ;
- ce que le haut dessine sous le pivot de l'articulation (son demi-disque)
  donne la taille du genou : couché, il repose dessus ;
- le bas de la boîte du bas de la patte est la semelle : le canon replié
  remonte assez pour qu'elle reste au-dessus du sol (la pince tournée vers
  le sol devant, le talon derrière).

L'aperçu `couche` (`mouflon-preview <pet.toml> <dossier> couche`) montre le
coucher de près, tourné à droite puis à gauche, avec la ligne du sol.

**La lèvre** (`lip`, dans `head`, dessinée par-dessus le reste de la tête) :
la lèvre supérieure, du naseau à la bouche, qui recouvre exactement la tête
au repos. Juste avant elle, dans la tête, dessine ce qu'elle découvre en se
retroussant : la gencive (le mouton n'a pas d'incisives en haut, seulement
un bourrelet rose). `pivot-lip` à son arrière.

**Les mèches** (`sway-1`, `sway-2`, `sway-3`) : des boucles ou des mèches qui
pendent. Le plus sûr est de les prendre dans la toison elle-même (des
festons du bas du ventre, posés juste avant le corps, dessous) : au repos,
rien ne change. Une mèche **penche** sans quitter son attache : le moteur la
cisaille autour de la ligne horizontale de son pivot, qui ne bouge pas, et
seul son bas part vers l'arrière ou vers l'avant. Coupe donc chaque feston
d'un creux du contour au suivant, et pose son pivot au milieu de la corde
entre les deux creux : seuls les creux touchent les voisins, et ils bougent
à peine ; un bord coupé au milieu d'un feston s'ouvrirait en encoche dès
que la mèche penche. Elles sont éclairées comme le corps.

**Les touffes d'encolure** (`neck-sway-1`, `neck-sway-2`,
`neck-sway-3`) ont une attache ponctuelle, étroite et cachée sous la laine
du cou. Cette racine suit l'encolure ; le moteur compense sa rotation et
son étirement pour que les fibres pendent à la verticale, puis leur
applique le même balancement qu'aux mèches du ventre. Elles ne pendent pas
à travers l'herbe : quand leur attache descend (il broute, il dort la
tête au sol), elles s'y tassent, raccourcies de ce qu'il faut. Le miroir et
les rotations de l'animal entier restent communs à tout le dessin. Ces
parties sont facultatives ; sans `neck`, leur attache suit `body`.

Dessine plusieurs petites touffes, avec un recouvrement généreux sous la
laine courte. Une large frange cousue sur toute sa largeur ne convient
pas : lorsque le cou passe à l'horizontale, conserver cette ligne
d'attache tout en faisant pendre les fibres aplatirait l'image. Le Nez
noir utilise trois touffes et 3 unités de recouvrement dans son
générateur. Les mèches du ventre conservent leur attache horizontale et
leur mouvement existant.

**Les nappes** : la longue laine d'une encolure (la bavette de cordes de la
Racka, les mèches de 20 cm du Scottish Blackface et de la Manech tête
rousse) ne tourne pas avec le cou, elle pend. Une encolure qui tourne d'une
pièce l'emporterait comme un drapeau raide : à l'horizontale quand il
broute, et, pour ce qui est sous sa base, balayée vers l'arrière puis sous
l'herbe. Ces trois races rangent donc leurs mèches d'encolure dans les
trois mêmes parties `neck-sway-*`, en trois nappes le long du cou :

1. le poitrail, sous la base de l'encolure : `pivot-neck-sway-1` est
   `pivot-neck`, la nappe ne bouge pas quand il broute (c'est la laine du
   poitrail, qui tient au corps) ;
2. le milieu du cou ; 3. le haut, sous la nuque : leur pivot est sur l'axe
   de l'encolure (de `pivot-neck` à `pivot-head`), au milieu de leurs
   rangées. Il suit le cou ; la nappe entière se déplace avec lui sans
   tourner, ses mèches à la verticale.

Une nappe est faite de mèches entières, de la racine à la pointe, sur
plusieurs rangées (aucune coupe, donc aucun bord à cacher), posées dans
l'ordre du dessin : au repos, rien ne change. Deux choses restent à
l'encolure et tournent avec elle : son fond et sa laine courte (le feutre),
et la réserve cachée sous le corps (ses mèches font le dessus du cou
baissé). Si le fond de l'encolure descend sous sa base (la bavette de la
Racka), coupe-le en deux : le haut à l'encolure, avec un disque autour de
la base pour qu'aucune encoche ne s'ouvre, le bas dans la nappe du
poitrail ; sinon l'encolure balaie un fond nu sous le poitrail en
descendant.

Ajoute à chaque nappe `pivot-neck-sway-N-top`, sur son bord du haut : elle
se balance depuis cette ligne (son bord du haut reste en place, tout ce
qui pend dessous penche du même côté), et non autour de son pivot, qui est
en son milieu. Sans ce point, la partie est une touffe coupée, qui ne
quitte pas sa ligne de coupe.

**Une cloche** (`bell`) pend de son collier, comme le redon du floucat, le
meneur du Mérinos d'Arles (`pets/floucat`). Range-la dans un groupe `bell`,
avec ce qui fait corps avec elle : l'anse rivée dessus. Le battant, pendu
dedans, peut avoir sa partie, `clapper`, dans `bell` (entre le fond de la
bouche et son ourlet de devant, pour qu'il en sorte) : `pivot-clapper` est
là où il est pendu, haut dans la cloche. Il reste à la verticale quand la
cloche penche ou se balance (il tourne à rebours dedans, d'un tiers de
radian au plus : il bute contre la paroi, et repose dessus quand elle est
couchée sur l'herbe), avec un temps de retard, celui des mèches du bout :
il bat d'un bord à l'autre à chaque pas.
La courroie et le collier restent dans `neck` : ils tournent avec le cou.
Le groupe se place dans `neck` à l'endroit même où la cloche se dessine
(après la laine de la gorge et la courroie qu'elle recouvre, avant ce qui
recouvre son haut, comme le bras du collier) : au repos, rien ne change.
Sans encolure, mets-la dans la partie qui porte son collier (`head` ou
`body`) : son anse suivra celle-là.

Dessine l'encolure qui porte une cloche avant les pattes qu'on voit (les
pattes de l'autre côté, l'encolure, puis celles-ci) : l'encolure est au
milieu du corps, et la cloche basculée sur l'herbe quand il broute passe
alors derrière l'antérieur au lieu de le recouvrir.

`pivot-bell` est là où elle pend de sa courroie : au milieu de l'anse, là
où la courroie la traverse. Dessine-la pendue droite : c'est sa pose de
repos. Et dessine-la entière, même ce que cache le collier : en se
balançant, en se couchant, elle se découvre.

Le moteur fait suivre ce pivot au collier, mais la fait pendre à la
verticale de l'écran, quoi que fassent le cou et l'animal (il broute, se
cabre, se penche dans l'élan, on le tient en l'air ; seule une culbute la
fait tourner avec lui). Elle se balance comme les mèches du bout de la
toison, en plus lourd : elle traîne un peu au galop, part vers l'avant
quand il s'arrête net, suit la secousse quand il s'ébroue, penche dans le
vent. Le moteur lit son contour une fois (l'enveloppe de ses tracés), pour
savoir jusqu'où elle descend à chaque angle. Près de l'herbe, son
balancement se calme ; quand elle passerait sous le sol, elle bascule vers
l'arrière (le cou avance et descend : son bas traîne), juste assez pour
s'y poser, en commençant un peu avant de toucher, comme poussée par
l'herbe ; l'anse trop bas, elle se couche sur le flanc, sur l'herbe,
soulevée s'il le faut. Endormi la tête au sol, un collier qui touche
l'herbe arrête la tête (voir « L'encolure ») : le redon du Mérinos d'Arles
bascule alors presque sur le flanc, à côté de la gorge, sans quitter sa
courroie. Tout cela change sans à-coup d'une image à l'autre. Les squelettes peints n'ont pas
de cloche (voir `squelette.md`).

Un exemple, réduit à l'essentiel :

```xml
<g id="leg-front-near">
  <g id="leg-front-near-lower">          <!-- dessous : le canon et le sabot -->
    <path d="…"/>
    <circle id="pivot-leg-front-near-lower" cx="101.2" cy="100.6" r="0.8"/>
  </g>
  <path d="…"/>                          <!-- le haut, fini en demi-disque -->
  <circle id="pivot-leg-front-near" cx="100.5" cy="88" r="0.8"/>
</g>
<g id="neck">                            <!-- sous le corps -->
  <path d="…"/>
  <path d="…"/>                          <!-- la courroie -->
  <g id="bell">                          <!-- l'anse, la cloche -->
    <path d="…"/>
    <g id="clapper">                     <!-- le battant, pendu dedans -->
      <path d="…"/>
      <circle id="pivot-clapper" cx="131.3" cy="50" r="0.8"/>
    </g>
    <path d="…"/>                        <!-- l'ourlet de la bouche, devant lui -->
    <circle id="pivot-bell" cx="131.4" cy="46.4" r="0.8"/>   <!-- au milieu de l'anse -->
  </g>
  <path d="…"/>                          <!-- le bras du collier, par-dessus -->
  <circle id="pivot-neck" cx="92" cy="66" r="0.8"/>
</g>
<g id="body">…</g>
<g id="head">                            <!-- par-dessus, à part -->
  …
  <path d="…"/>                          <!-- la gencive -->
  <g id="lip"><path d="…"/><circle id="pivot-lip" cx="130.2" cy="62.4" r="0.8"/></g>
  <circle id="pivot-head" cx="103.5" cy="51.5" r="0.8"/>   <!-- la nuque -->
  <circle id="pivot-muzzle" cx="134" cy="71" r="0.8"/>
</g>
```

**Couper un dessin existant.** `tools/races/articulations.py` le fait pour
un SVG généré par un script (voir `tools/races/README.md`) : chaque patte
coupée au carpe et au jarret, l'encolure prise dans le corps devant une
épaule arrondie, la lèvre détachée de la tête avec sa gencive, des festons
détachés en mèches, ou le bout de mèches qui pendent coupé sous la toison ;
au repos, le dessin reste le même, à l'anticrénelage près.

**Le dos ne plie pas.** Un arrière-train (`rump`) coupé au milieu du dos et
tournant autour de lui a été essayé sur le Mannequin : dès 0,12 rad (7°), le
ventre s'ouvre d'un côté (une boucle coupée en deux, le contour interrompu)
et se double de l'autre. Une toison est d'un seul tenant : pour la plier, il
faudrait déformer le dessin, pas faire tourner des morceaux cuits en
images. Le moteur incline le corps entier (le coucher, le galop, le
cabré) ; il ne le plie pas.

## Pivots

Un pivot est un **petit cercle** (rayon 1, n'importe quelle couleur) dont
l'`id` est `pivot-` suivi du nom de la partie : `pivot-head`,
`pivot-leg-front-near`, etc. Place-le à l'articulation. Il n'est jamais
affiché. S'il manque, le moteur en devine un raisonnable.

Le pivot **`pivot-ground`** est obligatoire en pratique : c'est le point où
le personnage touche le sol, entre ses pattes.

Un dessin mesure au plus 4 096 unités de côté, et ses pivots ne s'en
éloignent pas de plus que sa propre taille : au-delà (une forme oubliée au
loin, un nombre abîmé dans le fichier), il est refusé au chargement, avec
la raison.

## Ce que le SVG peut contenir

Pris en charge : tracés et formes, remplissages et contours, dégradés
linéaires et radiaux, opacité de groupe.

Le moteur cuit le dessin en images, une par suite de tracés d'une même
partie, et ne pose plus que ces images à l'écran : un dessin de milliers de
tracés ne coûte pas plus cher à animer qu'un dessin simple (les dessins des
races du dépôt pèsent de 370 Ko à 3,5 Mo, cuits en une trentaine d'images au plus). Un
groupe semi-transparent empêche cette cuisson (le dessin est alors redessiné
à chaque image) : mets plutôt l'opacité sur chaque tracé.

Pas encore : masques, découpes (`clip-path`), filtres (flou…), motifs,
texte (convertis-le en tracés), images bitmap intégrées.

## Depuis ton logiciel de dessin

- **Figma** : le nom d'un calque devient son `id` si tu coches « Include “id”
  attribute » à l'export SVG.
- **Illustrator** : export SVG avec « Object IDs : Layer Names ».
- **Affinity Designer** : le nom du calque devient l'`id` à l'export SVG.
- **Inkscape** : propriétés de l'objet → champ « ID ».

## Le fichier `pet.toml`

Pose ton SVG et un `pet.toml` dans un dossier, par exemple `mouton/`, en
partant de `pets/bleu-du-maine/pet.toml` qui sert de modèle complet :

```toml
[pet]
id = "mouton"
name = "Mouton"
author = "Alejandro San Martin Lamas"

[appearance]
rig = "mouton.svg"
height = 64.0   # hauteur affichée, en points
```

Les comportements (marcher, s'asseoir, dormir, sauter…) se règlent dans le
même fichier, avec leurs durées et leurs enchaînements pondérés.

## Vérifier sans lancer l'app

```sh
cargo run -p mouflon-preview --release -- mouton/pet.toml apercu poses
```

produit `apercu/poses.png` : debout, marche, atterrissage, saut, tenu,
sommeil, couché, broute, galop. Avec `gestes`, les gestes (le flehmen
compris) ; avec `allures`, le pas, le trot et le galop phase par phase, le
coucher et le saut : c'est là qu'on voit plier le carpe et le jarret.
Avec `cloche`, la tête et ce qui pend sous la gorge en gros plan, une ligne
sur le sol : `cloche.png` (broutage, coucher, sommeil la tête au sol,
allures, ébrouement, tenu, saut, demi-tour) et `cloche-mouvement.png` (un
mouton simulé qui part, galope, s'arrête net, s'ébroue, puis prend un coup
de vent). Avec `video` à la place de `poses`, tu obtiens les images d'une
petite scène de bureau à 60 i/s (à assembler avec ffmpeg).

Pour l'utiliser dans l'app : menu de la barre des menus → « Ouvrir le dossier
des animaux », copie ton dossier dedans, puis relance Mouflon.

## La tétée et l'appel maternel

La rencontre familiale du moteur n'exige pas de nouvelle partie dessinée.
Le rendu fournit au cœur deux repères normalisés par la hauteur : la région
mammaire implicite sous le flanc près des postérieurs, et le museau dans la
pose de tétée. Le jeune rejoint sa place en marchant, puis `nurse` plie
l'encolure et relève la tête vers la cible, tandis que ses quatre sabots
restent fixes. La queue frétille ; `maternal_call` garde la bouche de la
mère fermée. Ces gestes viennent de l'état familial, qui s'interrompt si
les partenaires perdent leur support ou sont pris.

Le repère mammaire est une convention graphique pour les dessins actuels,
pas une mesure vétérinaire ni une mamelle ajoutée au dessin. La frange de
laine peut descendre sous ce repère. Un dessin sans encolure conserve un
simple geste de tête ; ses autres poses et ses images cuites restent
inchangées. L'aperçu `famille` montre la vraie simulation à deux âges et
dans les deux sens, accompagné de sa légende `famille.md`.
