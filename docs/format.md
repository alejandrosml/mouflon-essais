# Le format `pet.toml`

Un animal Mouflon tient dans un dossier : un `pet.toml` et son dessin.
`pets/bleu-du-maine/pet.toml` est un exemple complet.

## Identité et apparence

```toml
[pet]
id = "mouton"                 # identifiant unique
name = "Mouton"               # nom affiché dans le menu
author = "Alejandro San Martin Lamas"

[appearance]
rig = "mouton.svg"            # dessin vectoriel découpé en calques (voir rig.md)
height = 64.0                 # hauteur affichée, en points
```

Ou des morceaux d'images peints posés sur un squelette (voir
[squelette.md](squelette.md) et `pets/solognote`) :

```toml
[appearance]
skeleton = "squelette.toml"   # les os, les morceaux, les animations
height = 58.0
```

Une planche de sprites façon desktopPet marche aussi :

```toml
[appearance]
sprites = "planche.png"
tiles = [16, 11]              # colonnes, lignes
key_color = "#ff00ff"         # couleur rendue transparente
```

### Famille et région

Dans `[pet]`, `category` dit la famille de l'animal (`sheep`, `farm`,
`cats`, `dogs`, `birds`, `wild`, `fantasy`, `others`, ou une autre, montrée
telle quelle) : le menu et la galerie rangent les animaux par familles, et
ceux d'une même famille s'imitent. Sans elle, le nom et la description la
trahissent souvent (un « mouton »), sinon `others`.

`region` dit d'où vient sa race : `france`, `british-isles`, `europe` (le
reste de l'Europe), `asia-africa`, `americas`, `oceania`, ou une autre,
montrée telle quelle. Une famille qui compte dix espèces ou plus, venues
d'au moins deux régions (les races de moutons du dépôt), se range par
régions dans le menu et dans la galerie ; celles qui ne disent rien vont
sous « Ailleurs ». Une petite famille reste d'un seul tenant.

```toml
[pet]
id = "herdwick"
name = "Herdwick"
category = "sheep"
region = "british-isles"
```

### Sexe et âge

Dans `[pet]`, `sex = "male"` ou `sex = "female"` fixe le sexe. Pour un
troupeau mixte, `sex = { male = 0.3 }` donne à chaque animal 30 % de chances
d'être mâle à l'apparition ; la valeur doit être finie, entre 0 et 1.
Sans clé, les deux sexes ont la même probabilité. Le tirage est déterministe
et indépendant des tirages de mouvement : modifier cette proportion ne
décale pas les pas, regards ou
clignements d'un animal qui ne joue pas de comportement sexué.

`age = "adult"` est la valeur par défaut. `age = "lamb"` déclare un
agneau ; `age_weeks = 2.0` donne son âge en semaines (2 par défaut, fini,
strictement positif et au plus 52). `age_weeks` n'est accepté qu'avec
`age = "lamb"`.
Cet âge est fixe : le temps passé sur le bureau ne fait pas vieillir l'animal.
Ces clés ne redessinent ni ne redimensionnent à elles seules son apparence :
pour donner au bélier, à la brebis et à l'agneau chacun son dessin, voir
[Un dessin par sexe et par âge](#un-dessin-par-sexe-et-par-âge).

```toml
[pet]
id = "agneau"
name = "Agneau"
category = "sheep"
sex = { male = 0.5 }
age = "lamb"
age_weeks = 2.0
```

Les conditions `male`, `female`, `adult` et `lamb` permettent de réserver
une transition au sexe ou à l'âge voulu. Les deux conditions de sexe
s'appliquent aussi aux agneaux ; utiliser `when = ["male", "adult"]` pour
un mâle adulte.

### Un dessin par sexe et par âge

Le dessin de `[appearance]` est le dessin de base. Un animal dessiné en
vecteurs ou peint sur un squelette peut en avoir d'autres, un par sexe et
par âge, chacun dans sa section :

```toml
[pet]
sex = { male = 0.4 }          # des béliers et des brebis

[appearance]
rig = "belier.svg"            # le dessin de base : ici, le bélier
height = 47.0

[appearance.female]           # la brebis
rig = "brebis.svg"
height = 41.5                 # sa hauteur affichée (0,5 pt par centimètre)
width = 56.0                  # sa largeur et sa foulée, comme dans [physics]
stride = 36.8

[appearance.lamb]             # l'agneau, mâle ou femelle
rig = "agneau.svg"
height = 27.0
width = 30.0
stride = 27.2

[physics]
width = 60.0                  # celles du dessin de base
stride = 39.2
```

- Trois sections : `[appearance.male]`, le mâle adulte (le bélier) ;
  `[appearance.female]`, la femelle adulte (la brebis) ;
  `[appearance.lamb]`, le jeune, des deux sexes (l'agneau).
- Chaque animal porte, dès son apparition, le dessin de son sexe et de son
  âge s'il existe, sinon le dessin de base. Avec un bélier pour dessin de
  base et une brebis dans `[appearance.female]`, les mâles portent le
  bélier, les femelles la brebis. Le sexe se tire comme sans ces dessins :
  ils ne changent ni la proportion de mâles ni aucun tirage.
- Chaque section donne un dessin vectoriel (`rig`) ou un squelette
  (`skeleton`), et sa hauteur affichée (`height`, obligatoire). `width` et
  `stride` disent sa largeur et sa foulée, comme dans `[physics]` ; sans
  elles, celles du dessin de base, au rapport des tailles.
- Chaque dessin a sa taille, et l'animal qui le porte bouge à sa taille :
  ses vitesses, ses sauts et sa pesanteur suivent la longueur de ses
  pattes (le rapport de sa foulée à celle du dessin de base, ou des
  hauteurs sans `stride`), comme un animal agrandi dans les réglages. La
  brebis un peu plus petite marche un peu moins vite ; l'agneau trottine
  pour suivre sa mère. Le reste est celui du fichier : comportements,
  réflexes, sons, physique (les conditions `male`, `female`, `adult` et
  `lamb` y choisissent ce qui revient à chacun).
- L'agneau de la race (celui d'« Ajouter son agneau », celui qui suit sa
  mère à son arrivée, celui qu'on demande dans la galerie : voir
  [Mères et agneaux](#mères-et-agneaux)) porte `[appearance.lamb]`, à la
  taille du dessin. Il a deux semaines ; dessiné à un autre âge, la section
  le dit avec `weeks` (`weeks = 8` pour l'agnelle Southdown de huit
  semaines, plus de 0 et au plus 52) : l'agneau a alors cet âge, qui règle
  sa tétée. Sans dessin d'agneau, il porte le dessin de sa mère (celui de la
  brebis, sinon le dessin de base), réduit : ce n'est pas une anatomie
  d'agneau.
- Un animal déclaré agneau dans son fichier (`age = "lamb"`) porte
  `[appearance.lamb]` s'il l'a ; les dessins d'adultes ne lui servent pas.
- Une race qui a le dessin de chaque sexe tire le sexe de chacun
  (`sex = { male = 0.4 }`, ou sans clé). Sinon, son `sex` fixe celui de son
  dessin : un bélier dessiné ne doit pas être tiré brebis.
- Chaque dessin respecte le contrat des parties ([rig.md](rig.md),
  [squelette.md](squelette.md)) : le test `troupeau` les vérifie tous,
  comme le dessin de base. Les animaux en images (planche, bandes,
  desktopPet) n'ont pas d'autres dessins.

Pour les voir côte à côte, à la même échelle, debout et broutant, avec leur
largeur et leur foulée (et `dessins.md`, la légende) :

```sh
cargo run -p mouflon-preview --release -- pets/awassi/pet.toml apercu dessins
```

Toute autre planche de l'aperçu montre un dessin choisi, ajouté au chemin :
`pets/awassi/pet.toml#female` (ou `#male`, `#lamb`).

## Physique (facultatif)

Toutes les valeurs sont en points et en secondes. Tous les nombres d'une
fiche (ceux-ci, les vitesses, les sauts, les durées et les poids des
comportements) doivent être de vrais nombres, d'une grandeur possible : au
plus 100 000 en valeur absolue, et jamais négatifs pour ce qui ne peut pas
l'être (la pesanteur, les frottements, les tailles, les durées, les poids).
Une fiche où l'un d'eux est abîmé (`inf`, `nan`, un zéro de trop) est
refusée au chargement, avec le nom de la valeur.

```toml
[physics]
gravity = 2200.0              # pt/s²
terminal_velocity = 1700.0
bounce = 0.22                 # rebond à l'atterrissage
hard_landing_speed = 1150.0   # au-delà : réflexe land_hard
width = 64.0                  # largeur utilisée pour les bords et les murs
stride = 29.0                 # distance d'un cycle de marche (défaut : 0,9 × la hauteur)
fleece = 23                   # longueur des mèches qui se balancent, en centimètres (facultatif)
```

`fleece` est la longueur des mèches de la toison qui se balancent (les
parties `sway-*` et `neck-sway-*` du dessin), en centimètres de l'animal,
d'après la fiche de la race : une longue mèche se balance plus lentement
qu'une courte, comme un pendule (2 Hz pour 9 cm, le rythme de base, celui
d'un animal qui ne le dit pas ; 1,25 Hz pour les cordes de 23 cm de la
Racka). Les autres dessins de la race (`[appearance.lamb]`…) ont des mèches
à leur taille : celles de l'agneau battent plus vite. `mouflon-preview
<dessin.svg> - mesure` écrit, sous `fleece`, jusqu'où pend la plus longue
mèche du dessin sous son attache : un repère, pas la longueur d'une mèche
(une nappe compte plusieurs rangées).

Pour un dessin vectoriel ou un squelette, `stride` règle la cadence des pas :
les pattes balaient juste ce qu'il faut pour que les sabots ne glissent pas,
et une foulée d'environ 1,6 fois la longueur des pattes donne un pas naturel.
L'allure se choisit d'elle-même d'après la vitesse rapportée à la longueur
des pattes, comme chez les vrais quadrupèdes (voir
[animation-animale.md](animation-animale.md)) : le pas jusqu'à 2,6 longueurs
de patte par seconde, le trot (pattes par paires diagonales, foulée à peine
plus longue) vers 3,4, le galop (le galop transverse des moutons, foulée
presque deux fois plus longue) au-delà de 4,6. Un comportement `trot` n'est
donc qu'un `walk` plus rapide.

## Comportements

Chaque `[[behavior]]` a un nom, un mouvement, une durée et des suites possibles.

```toml
[[behavior]]
name = "walk"
motion = "walk"               # voir la liste ci-dessous
speed = 46.0                  # pt/s, pour walk et run
duration = [3.0, 9.0]         # au hasard entre 3 et 9 s ; ou un nombre ; ou "forever"
next = [
    { to = "stand", weight = 4 },
    { to = "sit", weight = 1 },
    { to = "sleep", weight = 6, when = "night" },
]
on_edge = "turn"              # au bord d'une fenêtre (sinon : réflexe edge)
on_wall = "turn"              # contre le bord de l'écran (sinon : réflexe wall)
on_fall = "fall"              # si le sol disparaît (sinon : réflexe fall)
on_meet = "surprise"          # en rencontrant un autre animal (sinon : réflexe meet)
on_window_side = "turn"       # le côté d'une fenêtre barre le chemin (sinon : réflexe window_side)
```

Mouvements : `stand`, `walk`, `run`, `turn` (demi-tour), `sit` (couché pour
un dessin vectoriel), `sleep`, `look` (regarde autour de lui), `graze` (broute,
la tête au sol, une bouchée par seconde, et de temps en temps un pas vers
l'herbe suivante), `jump` (avec `jump = [vers l'avant, vers le haut]` en pt/s),
`fall`, `land` (récupération après atterrissage), `held` (tenu par le curseur),
`charge` (il fonce, tête basse, oreilles couchées ; `speed`, 175 pt/s par
défaut), `pronk` (saut d'agneau : droit en l'air, les quatre pattes raides ;
`jump`, `[25, 420]` par défaut) et `back` (il recule sans se retourner, le
bélier qui prend son élan avant de charger ; `speed`, 28 pt/s par défaut ; il
s'arrête au bout d'un rebord plutôt que d'en tomber à reculons, et ne réagit
pas au bord de l'écran), et `leap` (il saute sur un rebord plus haut, tout
proche, comme un mouflon sur un rocher ; `reach`, la hauteur du plus haut
rebord qu'il atteint d'un bond, en longueurs de patte, 2 par défaut ; voir
plus bas).

Un animal dessiné (vecteurs ou squelette) se ramasse un dixième de seconde
avant de sauter (`jump`, `pronk`) : l'anticipation.

Un saut sur un rebord (`motion = "leap"`) se choisit dans les réflexes
`window_side` (le côté d'une fenêtre plus haute lui barre le chemin) et
`edge` (au bout de son rebord, un rebord plus haut juste devant) ; le
moteur ne le propose que si ce rebord est à sa portée, et les autres
réactions gardent alors leurs chances, comme sans saut. L'animal mesure le
rebord du regard, recule pour prendre son élan s'il est trop près, se
ramasse, bondit les pattes repliées, et se pose les sabots plantés ;
`on_land` dit ce qu'il fait ensuite :

```toml
[reflexes]
window_side = [{ to = "turn", weight = 2 }, { to = "leap", weight = 3 }]
edge = [{ to = "turn", weight = 4 }, { to = "hop_down", weight = 2 }, { to = "leap", weight = 3 }]

[[behavior]]
name = "leap"
motion = "leap"
reach = 2.4                   # longueurs de patte (stride / 1,6) : ici 1,5 fois le garrot
duration = "forever"
on_land = "leap_land"

[[behavior]]
name = "leap_land"            # la réception, sabots plantés
motion = "stand"
duration = 0.3
sound = { name = "pose", chance = 0.6 }
next = [{ to = "walk", weight = 3 }, { to = "look", weight = 2 }]
```

Les portées et les envies des vingt-huit races, et ce que disent les
études, sont dans [animation-animale.md](animation-animale.md) (« Sauter
sur un rebord plus haut »). Endormi (`sleep`), il pose
une fois sur deux la tête au sol, le cou tendu ; une oreille ou une patte
tressaille de temps en temps. Un animal en images a ses propres images pour
tout cela.

Gestes sur place, que le moteur anime de lui-même pour un dessin vectoriel
ou un squelette (`duration` dit combien de temps ils durent) : `shake`
(s'ébroue : une secousse six fois par seconde, de la tête à la queue),
`stretch` (s'étire : le cou tendu, puis une patte arrière ; 2,4 s),
`rear` (se cabre : l'avant se dresse sur les pattes de derrière, celles de
devant repliées, puis retombe d'un coup ; 1,15 s ; le bélier avant de
charger), `invite` (l'invitation au jeu des agneaux : tête et avant
baissés, il fixe l'autre en sautillant),
`scratch` (se gratte d'un sabot arrière), `paw` (gratte le sol d'un sabot de
devant, tête basse), `stamp` (tape du pied, tête haute : l'alarme), `sniff`
(renifle, le museau tendu), `daze` (sonné : la tête tourne, les yeux
mi-clos), `brace` (contre le vent : tête basse, oreilles couchées, serré sur
ses pattes), `yawn` (bâille ; 1,6 s), `bleat` (bêle, la bouche ouverte),
`flehmen` (le bélier qui a flairé une brebis : l'encolure tendue, la tête
relevée, la lèvre supérieure retroussée, la bouche entrouverte ; 7 à 12 s
dans les animaux du dépôt, 7 à 19 s en moyenne chez de jeunes béliers,
parfois une minute ou deux).
`pets/bleu-du-maine/pet.toml` les montre tous.

La rencontre mère–agneau pilote en plus `nurse` (tête levée et queue
frétillante) et `maternal_call` (appel maternel), sans demander de nouveaux
comportements au fichier ; voir [Mères et agneaux](#mères-et-agneaux).

D'autres clés d'un comportement :

```toml
[[behavior]]
name = "ruminate"
motion = "sit"
chew = true                   # il mâche : couché, il rumine bol après bol
duration = [10.0, 24.0]

[[behavior]]
name = "walk"
motion = "walk"
speed = 46.0
contagious = 0.25             # un voisin de la même famille l'imite une fois sur quatre

[[behavior]]
name = "fonce"
strip = "course"
tilt = 0.12                   # penché en avant (radians) : un animal en images qui fonce

[[behavior]]
name = "gambol"
motion = "pronk"
twist = true                  # en l'air, le corps se tord et les pattes de derrière ruent
jump = [40.0, 380.0]
duration = "forever"
on_land = [{ to = "land", weight = 3 }, { to = "gambol", weight = 2 }]   # il en enchaîne

[[behavior]]
name = "pivot"
motion = "pronk"
spin = true                   # il se retourne en l'air (un petit bond sur place)
jump = [0.0, 300.0]
duration = "forever"
```

`on_land` dit ce qu'il fait en se posant (sinon : réflexe `land`, ou
`land_hard` après une grosse chute).

Un comportement contagieux (`contagious`, de 0 à 1) se propage : chaque
voisin de la même famille (`category`), à moins de quatre largeurs et à la
même hauteur, qui a un comportement du même nom l'imite avec cette chance,
dans les cinq secondes s'il est debout, à marcher, à brouter ou à regarder
autour de lui, sinon à la fin de ce qu'il fait (un animal en images ne coupe
pas sa séquence ; il attend au plus huit secondes). Il part alors du même
côté : un mouton qui part entraîne les autres, un bâillement en appelle un
autre, l'alerte gagne le troupeau. Celui qui vient d'imiter n'imite plus
pendant 25 secondes, et ce qu'il imite ne se propage pas plus loin. Les
animaux desktopPet, dont les scènes s'enchaînent d'elles-mêmes, n'imitent
pas.

Les poids sont relatifs : `weight = 4` contre `weight = 1` donne quatre
chances sur cinq. Une transition dont la condition est fausse est ignorée.

Conditions (`when`, une seule ou une liste) : `on_window`, `on_floor`,
`night` (22 h – 6 h), `day`, `morning` (6 h – 11 h), `evening` (18 h – 22 h),
`cursor_near:220`, `cursor_far:220` (rayon en points), `typing` (l'utilisateur
tape au clavier : une touche il y a moins de 3 s), `away` (personne au clavier,
à la souris ni au trackpad depuis 3 minutes), `present` (le contraire),
`into_wind` (un coup de vent lui souffle dans le museau), `male`, `female`,
`adult`, `lamb` (sexe et âge ; voir plus haut), et le tempérament
de l'animal, réglé dans les réglages : `calm`, `lively` (actif), `rowdy`
(énervé), `even` (tel que son auteur l'a fait).

```toml
next = [
    { to = "walk", weight = 6 },
    { to = "paw", weight = 4, when = "rowdy" },      # un énervé gratte le sol avant de foncer
    { to = "pronk", weight = 3, when = "lively" },   # un actif saute comme un agneau
    { to = "ruminate", weight = 3, when = "calm" },  # un calme se couche pour ruminer
]
```

Le tempérament change aussi, de lui-même, ce que l'animal choisit le plus
volontiers (le repos pour un calme, l'élan pour un actif ou un énervé) et
combien de temps il y reste ; il change sa posture (oreilles dressées et
tête basse d'un énervé, oreilles pendantes d'un calme) et le rythme de ses
oreilles et de sa queue.
Mouflon sait *quand* une touche est frappée, jamais *laquelle* : le système le
dit à tous les programmes, sans autorisation.

Un comportement où l'animal dort (ou s'endort) le dit avec `sleep = true`
(implicite pour `motion = "sleep"`) : l'utilisateur qui revient le réveille, et
un pointeur qui file ne le fait pas sursauter.

## Réflexes

Ce qui se passe lors des événements physiques :

```toml
[reflexes]
spawn = "fall"                # à l'apparition, en haut de l'écran
fall = "fall"                 # le sol disparaît
land = "land"                 # atterrissage
land_hard = "land_hard"       # atterrissage violent
held = "held"                 # attrapé par le curseur
thrown = "fall"               # relâché
edge = [{ to = "turn", weight = 5 }, { to = "hop_down", weight = 1 }, { to = "leap", weight = 1 }]
wall = "turn"
poke = "hop"                  # simple clic
bump = "tumble"               # heurté par un autre animal (mode collisions) ; sinon : thrown
meet = "greet"                # rencontre un autre animal en chemin (hors mode collisions)
window_side = [{ to = "turn", weight = 3 }, { to = "leap", weight = 1 }]  # le côté d'une fenêtre lui barre le chemin
leave = "wave"                # au revoir (« Tout retirer ») : il le joue, puis s'efface
together = "hop"              # « Tous ensemble » : tous les animaux le jouent en même temps
startle = "surprise"          # le pointeur file tout près de lui (plus de 1 400 pt/s)
sleepy = "yawn"               # l'utilisateur est parti : il s'installe pour dormir
wake = "wake_up"              # l'utilisateur revient pendant qu'il dort (sinon : la suite de son sommeil)
wind = "look"                 # un coup de vent se lève (une surprise)
```

Un mouton tourne le dos au vent : `wind = [{ to = "turn_away", weight = 4,
when = "into_wind" }, { to = "brace", weight = 1 }]`.

Les animaux s'endorment l'un après l'autre (dans les 40 s) quand
l'utilisateur part, se rendorment s'ils se réveillent avant son retour, et
se réveillent dans les deux secondes quand il revient.

Sans `meet`, deux animaux qui se croisent passent l'un devant l'autre ;
sans `window_side`, l'animal passe devant la fenêtre ; sans `leave`, il
s'efface aussitôt.

### Rencontres

Hors mode collisions, un animal qui en atteint un autre en marchant (au
même niveau, en allant vers lui) réagit, et l'autre parfois avec lui, comme
dans desktopPet :

1. s'ils ont des réactions en commun (même nom, poids non nul), les deux
   font la même chose : deux Lainou se saluent ensemble ;
2. sinon, si l'autre joue une réaction que le premier connaît, même de poids
   nul, le premier l'imite une fois sur deux (`{ to = "dort", weight = 0 }` :
   il ne s'endort jamais de lui-même en rencontrant quelqu'un, mais peut
   rejoindre un animal endormi) ;
3. sinon, chacun réagit de son côté, l'autre seulement s'il lui fait face.

Un animal en chemin prend le réflexe `meet` ; au repos (assis, endormi…),
il ne réagit que si son comportement a un `on_meet`. Après une rencontre,
chacun attend un peu avant la suivante, sauf s'il entre dans un
comportement qui a son propre `on_meet` : son auteur a prévu la rencontre.
Un animal qui fonce (`charge`) et réagit en atteignant l'autre le cogne :
le choc les tasse tous les deux. C'est ainsi que deux agneaux jouent à se
cogner le front (voir `play_back`, `play_lunge` et `play_butt` dans
`pets/bleu-du-maine/pet.toml`) :

```toml
[reflexes]
meet = [{ to = "sniff", weight = 4 }, { to = "play_back", weight = 4, when = "lively" }]

[[behavior]]
name = "play_back"            # tous deux reculent, aussi longtemps l'un que l'autre
motion = "back"
duration = 1.1
next = "play_lunge"

[[behavior]]
name = "play_lunge"           # s'élancent…
motion = "charge"
speed = 110.0
duration = 0.8
on_meet = "play_butt"         # … et se cognent
```

### Mères et agneaux

Une race dont le fichier tire le sexe (`sex = { male = 0.25 }`) donne
surtout des brebis, parfois un bélier ; la galerie laisse aussi demander
précisément un bélier, une brebis ou un agneau. Une brebis sur quatre
arrive suivie de son agneau, qui tombe du haut de l'écran derrière elle.

Le menu d'une brebis native (rig ou squelette, famille `sheep`) permet
« Ajouter son agneau » dès qu'elle est posée, même couchée ou endormie (la
tétée attend qu'elle soit debout) ; une brebis en images ou desktopPet n'a
pas cet article. Sa définition doit prévoir les mouvements `stand` et
`walk`, ce dernier à vitesse positive ; les noms des comportements restent
libres. Le menu prépare en arrière-plan une définition âgée de deux
semaines (ou de l'âge de son dessin d'agneau, `weeks`) et lie le jeune à
cette mère. Le jeune est mâle ou femelle à
chances égales, quel que soit le `sex` fixé pour le dessin de sa mère. Le
lien repose sur les identifiants des individus, pas sur leur rang ni sur le
voisin le plus proche. Retirer la mère libère le jeune.

Le jeune suit sa mère sur leur support commun ; lors d'une tétée, elle
attend et l'appelle bouche fermée, il marche sous son flanc et se tourne à
l'inverse d'elle. En mode collisions, une mère et ses agneaux ne se
heurtent jamais : ils se quittent après la tétée, encore l'un sous l'autre,
sans se renverser. Les durées et intervalles viennent des moyennes de
Gordon & Siegmann 1991 (voir
[animation-animale.md](animation-animale.md#la-mère-et-son-agneau)). L'âge
ne change pas pendant la session. Le jeune porte le dessin d'agneau de sa
race (`[appearance.lamb]`, voir
[Un dessin par sexe et par âge](#un-dessin-par-sexe-et-par-âge)) ; sans lui,
le dessin de sa mère réduit, qui n'est pas une anatomie de jeune. Les
fichiers en images et desktopPet gardent leurs propres scènes.

Un agneau est un cran plus vif que le troupeau : avec le tempérament
« normal », il vit comme un actif (les jeux que chaque fichier réserve aux
actifs sont les siens) ; dans un troupeau calme, comme son fichier l'a
fait. Il a aussi sa voix, si le fichier la donne (voir [Sons](#sons)).

Pour les outils, `PetDef::as_lamb(weeks)` prépare la définition
(`PetDef::lamb_weeks` dit l'âge de son dessin) ; `Flock::spawn_lamb_with`
ajoute le jeune auprès de sa mère posée, `Flock::spawn_lamb_arriving` le
fait tomber derrière elle où qu'elle soit, `Flock::spawn_as` ajoute un
animal du sexe voulu, et `Flock::bind_mother` peut lier deux individus
existants compatibles. Déclarer `age = "lamb"` dans un fichier ne choisit
pas une mère au hasard.

### Invitations à jouer

Un animal qui a les comportements `play_invite` (le geste `invite`) et
`play_lunge` (un `charge` dont l'`on_meet` mène au coup de front) invite de
lui-même un voisin à jouer, toutes les 20 à 60 secondes s'il en a envie
(toujours s'il est actif, une fois sur trois tel quel, rarement énervé,
jamais calme) : un voisin de sa famille, libre, sur le même rebord, à une ou
trois longueurs. Il se tourne vers lui et joue `play_invite` ; une seconde
plus tard, l'autre accepte (le plus souvent s'il est actif, rarement s'il
est calme) : tous deux se font face et jouent `play_lunge`. Sinon,
l'invitation finit comme son comportement le prévoit.

### Regroupements

De temps en temps (toutes les cinq à douze minutes), le troupeau se
regroupe, comme les moutons au pré : sur un même rebord ou sur le sol, au
moins trois animaux d'une même famille, libres (debout, à brouter, à
marcher…) et bien dispersés, se rapprochent de leur milieu en ordre serré,
sans changer d'ordre. Le plus loin de sa place part le premier, les autres
suivent comme une vague. Chacun y va avec son comportement `walk` ou
`marche` (`trot` s'il est loin), puis s'arrête (le comportement au sol par
défaut) ; en chemin, il n'imite personne. Les animaux desktopPet ne se
regroupent pas.

## Listes de suites nommées

Une liste qui revient souvent se nomme une fois dans `[choices]`, puis
s'emploie comme un comportement : `next = "envie"`.

```toml
[choices]
envie = [
    { to = "marche", weight = 6 },
    { to = "repos", weight = 3 },
    { to = "baille", weight = 4, when = "night" },
]
```

## Animaux en images

Un personnage dessiné image par image a une bande d'images par animation,
dans chaque sens : `planches/gauche/marche.png`, `planches/droite/marche.png`…
Chaque comportement joue la bande qui porte son nom et avance du déplacement
prévu à chaque image : les pattes ne glissent jamais. Lainou, gardé comme
donnée de test (`tests/donnees/lainou`), en est un exemple complet.

```toml
[appearance]
strips_left = "planches/gauche/{name}.png"    # {name} : nom de l'animation
strips_right = "planches/droite/{name}.png"   # facultatif : sinon, miroir des images de gauche
frame = [256, 256]            # taille d'une image dans les bandes (pixels)
scale = 0.375                 # points par pixel : 256 px font 96 points
anchor = [128, 226]           # milieu des pattes, posé sur le sol (pixels)
drawn = ["shadow", "dust", "squash"]   # effets déjà dessinés : Mouflon ne les ajoute pas

[physics]
width = 78.0                  # largeur du corps : il se retourne quand son nez touche le bord
height = 55.0                 # hauteur du corps au-dessus du point d'appui

[[behavior]]
name = "marche"
fps = 10                      # images par seconde
loops = [3, 6]                # passages : un nombre, ou [min, max] au hasard
move = [5.5, 0]               # pixels par image : vers l'avant, vers le bas
on_wall = "demi_tour"
next = "envie"

[[behavior]]
name = "demi_tour"
fps = 12
turns = true                  # les images montrent le demi-tour : il regarde de l'autre côté à la fin
next = "marche"

[[behavior]]
name = "se_cogne"
fps = 12
move = [[0, 0], [0, 0], [-5, 0], [-7, 0], [-4, 0], [-1.5, 0], [0, 0], [0, 0], [0, 0]]  # un déplacement par image
next = "demi_tour"

[[behavior]]
name = "aventure"
strip = "marche"              # rejoue une autre bande
frames = [0, 1, 2, 3]         # images choisies dans la bande (toutes par défaut)
move = [5.5, 0]
walk_off_edges = true         # au bout d'une fenêtre, il continue et saute dans le vide

[[behavior]]
name = "tombe"
motion = "fall"               # chute physique (lancer, sol qui disparaît) ; images en boucle
fps = 10
duration = "forever"
```

Sans `motion`, un comportement avance image par image ; avec `motion =
"fall"` ou `"held"`, ses images tournent en boucle pendant la chute ou
pendant qu'on le tient. La séquence dure ses images fois ses passages, puis
l'animal passe à la suite. `fps = [20, 8]` change de rythme le long de la
séquence : un atterrissage vif au début, plus posé à la fin. `loop_from = 3` fait reprendre les passages
suivants à la quatrième image (une entrée, puis une boucle).

Un animal en images vérifie à chaque pas qu'il a encore un appui (sinon il
tombe, réflexe `fall`). Sans réaction prévue au bord de l'écran ou d'une
fenêtre, il s'arrête et se retourne ; avec `walk_off_edges = true`, il
continue et tombe du rebord.

Effets de `drawn` : `shadow` (l'ombre au sol est dans les images ; Mouflon
dessine la sienne seulement en l'air, sur ce qui est en dessous), `dust`
(poussière de course et d'atterrissage), `squash` (écrasement et étirement),
`roll` (les images de chute font déjà tourner l'animal sur lui-même).

Une planche unique marche aussi : `sprites = "planche.png"`, `tiles =
[colonnes, lignes]`, `facing = "right"` si les images regardent vers la
droite, et `frames = [numéros des cases]` dans chaque comportement.

## Images intermédiaires

Pour un mouvement plus fluide, un animal en images peut avoir des images
intermédiaires entre deux images qui se suivent. Vers la fin de chaque pas,
Mouflon montre celles qui mènent à l'image suivante : une pose longue reste
tenue, puis l'animal change de pose en douceur (80 ms au plus par image
intermédiaire). Elles sont décrites par un fichier à part :

```toml
[appearance]
inbetweens = "entre-deux.toml"
```

L'outil `tools/inbetween` les calcule et écrit ce fichier (avec le réseau
RIFE, qui estime le mouvement entre deux images ; il laisse sans image
intermédiaire les paires trop différentes, comme un demi-tour de profil à
face). Pour des bandes, chaque bande a la sienne, avec `count` images après
chacune de ses images (après la dernière, vers la première) :

```toml
count = 1
strips_left = "entre-deux/gauche/{name}.png"
strips_right = "entre-deux/droite/{name}.png"

[counts]
tombe = 3                     # une chute rapide : trois images après chaque image

[skip]
demi_tour = [0, 1, 2]         # pas d'image intermédiaire après ces images
```

Pour une planche unique (un animal desktopPet redessiné, par exemple), une
planche à part, de mêmes cases, et la liste des paires d'images :

```toml
sprites = "entre-deux.png"
tiles = [12, 23]
# [image de départ, image d'arrivée, première case, nombre de cases]
pairs = [
    [1, 0, 0, 3],
    [0, 1, 3, 3],
]
```

Une bande sans la sienne, ou une paire absente de la liste, reste telle
qu'elle est dessinée.

## Sons

Les sons d'un animal ont un nom ; chacun est un fichier audio (MP3, AAC, WAV),
ou plusieurs prises dont l'une est tirée au hasard à chaque fois, avec un
volume de 0 à 1. Un comportement fait entendre un son au début, ou sur une
image de sa séquence à chaque passage, selon une chance de 0 à 1 :

```toml
[sounds]
bele = { files = ["sons/agneau-1.mp3", "sons/agneau-2.mp3"], volume = 0.8 }
clochette = { files = ["sons/clochette-1.mp3", "sons/clochette-2.mp3"], volume = 0.5 }
broute = "sons/broute.mp3"

[[behavior]]
name = "bele"
sound = "bele"                # au début du comportement
fps = 10

[[behavior]]
name = "course"
sound = [{ name = "clochette", frame = 2, chance = 0.15 }]   # sur la troisième image, une fois sur sept environ
fps = 14
```

Un son qui est la voix de l'animal le dit avec `mouth = true` : il ouvre la
bouche en le faisant entendre (s'il a une mâchoire, voir rig.md et
squelette.md).

```toml
[sounds]
bele = { files = ["sons/mouton-1.mp3", "sons/mouton-2.mp3"], volume = 0.7, mouth = true }
```

Un agneau n'a pas la voix de ses parents : `lamb` donne les prises qu'il
fait entendre à la place de `files`, pour ce son seulement (l'agneau de la
race, avec ou sans dessin d'agneau, et un fichier qui déclare
`age = "lamb"`). Sans `lamb`, il garde celles des adultes.

```toml
[sounds]
bele = { files = ["sons/mouton-1.mp3", "sons/mouton-2.mp3"], lamb = ["sons/agneau-1.mp3"], volume = 0.7, mouth = true }
```

Les sons se coupent dans le menu (« Sons »).

## Animaux desktopPet

Mouflon lit directement les fichiers `animations.xml` de
[desktopPet](https://github.com/Adrianotiger/desktopPet) : dépose le fichier,
ou tout le dossier de l'animal, dans le dossier des animaux.

Tout le moteur d'animation d'origine est repris : nombre de pas et
répétitions (y compris tirées au hasard), déplacements interpolés,
demi-tours `flip`, réactions aux bords (`border`, avec les restrictions
`only`, dont `windowSide` pour le côté d'une fenêtre et `petSide` pour la
rencontre d'un autre animal), gravité, points d'apparition, personnages
secondaires (`child` : la fleur que mange le mouton, le bain, le mouton
noir…), variantes de couleur (`skins`), expressions (`screenW`, `areaH`,
`imageW`, `random`, `randS`, `Convert(…, System.Int32)`…) et animations
réservées (`drag`, `fall`, `toss`, `fall soft`, `fall hard`, `kill` pour
l'au revoir, `sync` pour « Tous ensemble »).

Ce que Mouflon fait mieux :

- le déplacement de chaque pas est réparti sur sa durée : l'animal glisse à
  120 Hz au lieu d'avancer par à-coups ;
- un lancer suit une vraie trajectoire (élan, rebond léger), et l'animal se
  pose tourné vers où il allait ;
- sans réaction prévue au bout d'une fenêtre, il ne tombe que lorsque son
  centre a dépassé le bord, pas dès que le bord de son image le dépasse ;
- une fenêtre fermée sous un animal le fait tomber tout de suite ;
- il se promène d'un écran à l'autre.

Les sons (`<sounds>`, en MP3) sont joués au début de leur animation, selon
leur probabilité ; la répétition (`<loop>`) n'est pas encore reprise. En
mode collisions, Mouflon fait culbuter les animaux qui se rentrent dedans
au lieu de jouer leurs rencontres `petSide`.

### Redessiner un animal desktopPet

Pour garder les comportements d'origine avec une planche redessinée en
haute définition, place à côté du fichier XML un `pet.toml` :

```toml
[pet]
id = "mouton"
name = "Mouton"
author = "Alejandro San Martin Lamas"

[appearance]
sprites = "mouton-4k.png"     # même disposition que la planche d'origine
tiles = [16, 11]              # colonnes, lignes (16 × 11 pour eSheep 64)
scale = 0.25                  # cases de 160 px affichées à 40 points

[desktoppet]
animations = "animations.xml"
```

La planche doit garder la disposition d'origine (une case par image, dans le
même ordre) ; sa transparence est celle du PNG. La taille affichée d'une case
vaut sa taille en pixels multipliée par `scale` : à la taille d'origine
(40 points pour eSheep 64), une planche quatre fois plus fine se lisse et
reste nette sur les écrans Retina. Plus grand que l'original, l'animal se
déplace d'autant plus vite : ses pas restent en proportion.

Sans `[appearance]`, le `pet.toml` ne fait que renommer l'animal et garde
l'image du fichier XML.

Le fichier XML peut aussi venir sans son image (balise `<png>` vide) : il
faut alors dire pour quelle taille de case ses déplacements sont réglés.

```toml
[desktoppet]
animations = "animations.xml"
tile = [40, 40]               # pixels d'une case de la planche d'origine
```

C'est le cas de Pelote (`tests/donnees/pelote`, une donnée de test, réglé pour
des cases de 80 pixels) et de Poete, la mascotte (réglé pour les cases de 40
pixels d'eSheep 64).

Le `pet.toml` peut ajouter des sons aux animations du fichier, par leur
nom, et des images intermédiaires (`inbetweens` dans `[appearance]`) :

```toml
[sounds]
bele = { files = ["sons/mouton-1.mp3", "sons/mouton-2.mp3"], volume = 0.7 }
broute = "sons/broute.mp3"

[desktoppet.sounds]
eat = "broute"
drag = { name = "bele", chance = 0.5 }
```

Il peut aussi donner aux animations du fichier les réactions de Mouflon
que desktopPet ne connaît pas (`startle`, `sleepy`, `wake`, `wind`, `poke`,
voir les réflexes) :

```toml
[desktoppet.reactions]
startle = "jump"              # le pointeur file tout près : il bondit
sleepy = "sleep1a"            # l'utilisateur est parti : il s'endort
# Une liste, comme ailleurs : le vent dans le museau, il lui tourne le dos.
wind = [{ to = "rotate1a", weight = 4, when = "into_wind" }, { to = "look_down", weight = 1 }]
```

Sans `sleepy`, un animal desktopPet s'endort avec sa première animation de
sommeil (`sleep1a`) ; réveillé, il passe à la suite (`sleep1b`).
