# Un animal peint, sur un squelette

Un animal à squelette est fait de **morceaux d'images** (le corps, la tête,
chaque patte, chaque oreille, la queue, l'œil…) posés sur des **os**. Le
moteur les anime lui-même : il marche au pas et galope (un pied après
l'autre, les sabots posés sans glisser), se couche, dort, broute la tête au
sol, saute les pattes repliées, se balance quand on le tient, regarde le
curseur, cligne des yeux, respire, bat des oreilles et de la queue. Avec une
encolure, des pattes en deux segments, une lèvre et des mèches (voir « Les
articulations »), l'encolure descend pour brouter, le carpe et le jarret
plient, les pattes se replient sous l'animal couché, la lèvre se retrousse
pour le flehmen, la toison traîne et balance. Tu
peins une seule pose de profil, il fait le reste ; des animations à clés
peuvent s'y ajouter ou le remplacer.

C'est la façon de faire pour des animaux **peints** : à la main, dans
Blender avec Grease Pencil, dans Krita, Procreate ou Photoshop, ou à partir
d'une image générée par une IA, découpée en morceaux. Le dessin garde sa
texture, ses dégradés et ses coups de pinceau : ce sont des images. Il
reste léger : une douzaine de morceaux, réduits à la finesse de l'écran au
chargement, que le GPU pose directement.

`pets/solognote` est un exemple complet : une brebis Solognote peinte à
l'aquarelle (ses morceaux sont peints par `tools/mouflon-peintre`).

## Le dossier

```
solognote/
├── pet.toml          comportements, physique (voir format.md)
├── squelette.toml    les os, les morceaux, les animations
└── morceaux/
    ├── corps.png
    ├── tete.png
    └── …
```

Dans `pet.toml` :

```toml
[appearance]
skeleton = "squelette.toml"
height = 58.0          # hauteur affichée, en points
```

Les comportements sont ceux d'un dessin vectoriel (`motion = "walk"`,
`"sit"`, `"graze"`…) : pars de `pets/solognote/pet.toml` ou de
`pets/bleu-du-maine/pet.toml`.

## Peindre les morceaux

- L'animal est de **profil**, tourné vers la **droite**.
- Une image par morceau, sur fond **transparent** (PNG). Le plus simple :
  toutes les images à la taille de toute la toile, chaque morceau à sa
  place, comme un calque exporté seul. Mouflon recadre chaque image sur ce
  qu'elle montre.
- Les morceaux **se chevauchent** aux articulations : le haut des pattes
  rentre sous le corps, le cou sous la tête, la base des oreilles sous le
  toupet. Peins aussi ce qui est caché au repos (le haut d'une patte qui se
  lève, l'arrière de la tête qui se penche) : sinon les mouvements ouvrent
  des trous.
- Peins plus fin que l'écran : une toile de 1000 à 2000 pixels de large
  convient. Mouflon réduit chaque morceau (filtre de surface) à 2,5 pixels
  par point affiché ; il ne les agrandit jamais.
- Pas d'ombre au sol sous l'animal : le moteur en ajoute une, qui le suit
  en l'air.
- Les pattes du côté caché (`far`) un peu plus sombres que celles du côté
  visible (`near`) : elles passent l'une devant l'autre en marchant.

## `squelette.toml`

Toutes les positions sont en **pixels de la toile** (x vers la droite, y vers
le bas).

```toml
size = [1200, 900]     # la toile
ground = [560, 820]    # le point d'appui : entre les sabots, au sol
muzzle = [944, 538]    # facultatif : ce qui touche l'herbe (le bout du museau)
```

### Les os

```toml
[[bone]]
name = "racine"
at = [560, 820]

[[bone]]
name = "corps"
parent = "racine"
at = [560, 560]        # l'articulation : le point autour duquel il tourne
role = "body"
```

Chaque os a un nom, une articulation (`at`) et un parent, défini avant lui
(seul le premier os n'en a pas). Un os tourne autour de son articulation et
emporte ses enfants : la tête emporte ses oreilles et son œil.

Un `role` dit ce que le moteur fait de l'os de lui-même :

| `role` | Ce que le moteur en fait | Articulation |
|---|---|---|
| `body` | monte et descend à chaque pas, se penche en se couchant, respire | milieu du corps |
| `head` | regarde le curseur, hoche au pas, se baisse pour dormir, va au sol pour brouter | le cou |
| `ear` | bat en tombant, frémit, tombe en dormant | base de l'oreille |
| `eye` | cligne (s'écrase, ou laisse place à une paupière peinte), se ferme en dormant | milieu de l'œil |
| `pupil` | suit le regard, et reste à l'horizontale quand la tête penche (la fente du mouton) ; les reflets, sur l'œil, par-dessus | centre de la pupille |
| `jaw` | s'ouvre pour mâcher, ruminer, bêler, bâiller (enfant de la tête) | la charnière, sous la joue, à hauteur de la bouche |
| `tail` | se balance, bat de temps en temps ; plusieurs os `tail` à la suite (chacun enfant du précédent) font une queue en chaîne : chacun suit le précédent avec un temps de retard, elle ondule ; couchée, si elle touche le sol, chaque os se plie vers l'arrière du même angle et elle se pose sur l'herbe | base de la queue (puis l'attache de chaque morceau) |
| `leg-front-near`, `leg-back-near` | pas, trot, galop, sauts, gestes (gratter le sol, taper du pied, se gratter), pattes repliées couché | hanche ou épaule |
| `leg-front-far`, `leg-back-far` | idem, décalées dans le cycle | hanche ou épaule |
| `neck` | l'encolure (la tête est son enfant) : elle descend pour brouter, se redresse aux aguets, se tend quand il fonce, s'allonge vers l'avant quand il dort la tête au sol | la base de l'encolure, dans l'épaule |
| `leg-front-near-lower`… | le bas d'une patte (enfant de sa patte) : il plie au carpe (devant) ou au jarret (derrière) | le carpe ou le jarret |
| `lip` | la lèvre supérieure (enfant de la tête) : elle se retrousse pour le flehmen | son arrière, sous le naseau |
| `sway` | une mèche de la toison (enfant du corps) : elle traîne quand il court, balance au rythme des pas, penche dans le vent, suit la secousse ; elle penche sans quitter son attache (le moteur la cisaille autour de la ligne horizontale de son pivot) ; plusieurs os `sway`, les suivants un peu plus en retard | le milieu de son attache, en haut |

Il n'y a pas de rôle pour une cloche pendue au collier (la partie `bell`
des rigs, voir `rig.md`) : peinte sur un os enfant de l'encolure, elle
tournerait avec le cou. Le moteur saurait la faire pendre et la poser sur
l'herbe (`pendant.rs`) ; aucun animal peint n'en porte encore. De même,
endormi la tête au sol, l'encolure d'un rig s'arrête sur ce qu'elle porte
s'il touche l'herbe le premier (un collier, une gorge laineuse) ; le moteur
ne lit pas les images des morceaux d'une encolure peinte, qui ne s'arrête
que sur le menton (celle de la Solognote reste loin de l'herbe).

La mâchoire de la Solognote est un morceau posé **derrière** la tête : le
bas du museau, peint comme la tête l'aurait peint, et au-dessus, cachés tant
que la bouche est fermée, la joue qui s'étire et l'intérieur de la bouche,
langue en bas. La tête a l'encoche de la mâchoire (et la recouvre d'un rien
sur ses bords, pour qu'aucune jointure ne laisse passer le jour) : bouche
fermée, on ne voit rien ; ouverte, l'intérieur apparaît entre les lèvres.
Voir `head_and_jaw` dans `tools/mouflon-peintre/src/solognote.rs`.

`near` : le côté qu'on voit ; `far` : l'autre. Une patte balaie juste ce
qu'il faut pour ne pas glisser : sa longueur est la distance de son
articulation au sol, et le pas d'un cycle est `stride` dans `[physics]`.

Les mouvements du moteur sont calculés comme si chaque os avec un rôle était
libre, puis ramenés à son parent : tu peux ranger les os comme tu veux (les
pattes sous le corps, par exemple), les sabots restent posés quand le corps
monte et descend.

### Les articulations

Toutes sont facultatives : un squelette qui ne les a pas bouge exactement
comme avant. `pets/solognote` les a toutes.

```toml
[[bone]]
name = "encolure"
parent = "corps"
at = [690, 560]          # la base de l'encolure, dans l'épaule
role = "neck"

[[bone]]
name = "tete"
parent = "encolure"
at = [782, 470]          # la nuque : la tête y tourne
role = "head"

[[bone]]
name = "patte-avant-pres"
parent = "corps"
at = [742, 668]
role = "leg-front-near"

[[bone]]
name = "patte-avant-pres-bas"
parent = "patte-avant-pres"
at = [750, 744]          # le carpe
role = "leg-front-near-lower"

[[bone]]
name = "levre"
parent = "tete"
at = [910, 522]
role = "lip"

[[bone]]
name = "meche-1"
parent = "corps"
at = [616, 694]
role = "sway"
```

- **L'encolure** : sa base assez bas dans l'épaule pour que l'encolure et
  la tête, bout à bout, atteignent le sol devant les antérieurs ; la tête
  tourne à la nuque. Peins-la en entier (aussi ce qui est caché sous le
  corps et la tête au repos) et pose son morceau **avant** celui du corps :
  le corps la recouvre à sa base, et elle se découvre en descendant. Pour
  brouter, le moteur pose le museau (`muzzle`) au sol, à peu près une
  longueur de tête devant les antérieurs ; seule l'encolure s'étire, la tête
  reste d'une pièce.
- **Les bas de pattes** : le haut va de l'épaule ou de la hanche au carpe ou
  au jarret et se termine en demi-disque autour de l'articulation ; le bas
  (le canon et le sabot), posé **avant** le haut (dessous), commence par
  l'arc du même cercle. Le moteur pose le sabot et plie le genou : droite à
  l'appui, pliée en vol, repliée sous l'animal couché.
- **La lèvre** : le morceau de la tête qui la peint, posé par-dessus la
  tête, et juste avant lui (sur l'os de la tête) la gencive qu'elle découvre.
- **Les mèches** : des festons du bas de la toison, détachés du corps et
  posés juste avant lui, dessous : au repos, rien ne change. Prends chacun
  d'un creux du contour au suivant (la corde entre les deux creux est son
  attache, son milieu le pivot de l'os) : une mèche coupée au milieu d'un
  feston s'ouvrirait en encoche en penchant. Au-dessus de la corde, la
  mèche déborde sous le corps, qui s'y fond ; sous la corde, ses côtés sont
  nets.

Voir `tools/mouflon-peintre` : `split_joint` coupe une patte peinte à son
articulation, `split_lock` détache une mèche ; dans `src/solognote.rs`,
`neck` peint l'encolure (la gorge et la laine de l'avant, avec ce qui reste
caché sous la tête et le corps) et `paint` en tire la lèvre et la gencive.

Le dos ne plie pas : coupé en deux au milieu du dos, la toison s'ouvre ou se
double dès qu'on la plie (voir [rig.md](rig.md#les-articulations)).

### Les morceaux

```toml
[[piece]]
bone = "corps"
image = "morceaux/corps.png"

[[piece]]
bone = "oeil"
image = "morceaux/oeil.png"
eyes = "open"          # seulement les yeux ouverts

[[piece]]
bone = "oeil"
image = "morceaux/oeil-ferme.png"
eyes = "closed"        # la paupière peinte, quand l'œil se ferme
```

De l'**arrière vers l'avant** : le premier est dessiné en premier. Chaque
morceau suit son os. Une image plus petite que la toile se place avec
`at = [x, y]` (son coin en haut à gauche). Avec des paupières peintes
(`eyes = "closed"`), l'œil ne s'écrase plus pour cligner : les deux images
s'échangent.

### Les animations à clés

```toml
[[clip]]
name = "calme"
length = 7.0           # secondes, quand elle tourne sur l'horloge

[clip.bones.oreille-pres]
rotate = [[0, 0], [0.42, 0], [0.45, 14], [0.5, 0]]   # [instant, degrés]

[clip.bones.corps]
move = [[0, 0, 0], [0.5, 0, -6], [1, 0, 0]]         # [instant, x, y]
scale = [[0, 1], [0.5, 1.04, 0.97]]                  # [instant, échelle] ou [instant, largeur, hauteur]

[motions]
idle = "calme"
```

Une animation donne, pour quelques os, des angles (degrés, sens des aiguilles
d'une montre), des déplacements (pixels de la toile) et des échelles à des
instants de 0 à 1 ; entre deux clés, le mouvement part et arrive en douceur.
Elle tourne en boucle.

`[motions]` dit quand la jouer :

| clé | quand | sur quelle horloge |
|---|---|---|
| `idle` | toujours, sous les autres | l'horloge (`length` secondes) |
| `walk`, `run` | au pas, au galop | une boucle par cycle de pas |
| `sit`, `sleep`, `graze` | couché, endormi, en broutant | l'horloge |
| `air`, `dangle` | en l'air, tenu | l'horloge |

Elles se mêlent selon ce que fait l'animal (à mi-chemin entre le pas et le
galop, à moitié l'une, à moitié l'autre) et seuls les os et les mouvements
qu'elles animent changent. Ce qu'une animation joue, le moteur ne le refait
pas : avec une animation `walk`, les pattes et le corps ne bougent plus
d'eux-mêmes au pas, mais la tête regarde toujours le curseur et l'œil cligne
toujours.

## Depuis Blender (Grease Pencil)

1. Dessine l'animal de profil, tourné vers la droite (vue de face de la
   caméra, en orthographique), **un calque par morceau** (ou un objet par
   morceau). Prolonge ce qui sera caché aux articulations.
2. Rendu : *Film → Transparent* coché, PNG en RGBA, une résolution fixe (par
   exemple 1600 × 1200).
3. Rends chaque morceau seul (les autres masqués, ou un *View Layer* par
   morceau) : toutes les images ont la même taille et se superposent
   exactement.
4. Relève les articulations en pixels de l'image (par exemple en ouvrant le
   rendu complet dans un éditeur d'images, curseur sur l'épaule, la
   hanche, le cou…) et écris `squelette.toml`.

## Depuis une image (IA ou dessin d'un seul tenant)

Dans Krita, Photopea, GIMP ou Photoshop : découpe chaque morceau dans son
propre calque, repeins ce qui manque dessous (le haut des pattes sous la
toison, l'arrière de la tête sous l'oreille), puis exporte chaque calque en
PNG à la taille de la toile.

## Vérifier

```sh
cargo run --release -p mouflon-preview -- pets/solognote/pet.toml apercu poses
```

donne une planche de poses (debout, au pas, au galop, en l'air, tenu,
couché, endormi, en broutant) ; `gestes` et `allures` (les allures phase
par phase, le coucher, le saut) montrent les articulations, `toison` les
mèches. `video` à la place de `poses` donne une
scène de 18 secondes, image par image.

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
