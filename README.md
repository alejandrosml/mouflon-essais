# Mouflon

*English first, [français plus bas](#français).*

## English

Mouflon brings little companions to your desktop, on the **Mac**, on
**Windows**, and on **Linux** as a preview. Sheep of real breeds drop from the
top of the screen, stroll along your windows, graze, meet, play, doze off, and
let you catch them and throw them across the desktop. Mouflon also reads the
pets of desktopPet (eSheep).

This archive is the source code of one version of Mouflon, as it was
published. Presentation and downloads:
<https://alejandrosml.github.io/Mouflon/>.

### Rebuild it

You need [Rust](https://rustup.rs) 1.90 or later and, on the Mac, the Xcode
command line tools (`xcode-select --install`).

```sh
cargo run --release             # run Mouflon from this folder
cargo test --workspace          # its tests
bash scripts/bundle-macos.sh    # Mac: dist/Mouflon.app (ad hoc signature)
bash scripts/bundle-linux.sh    # Linux: dist/Mouflon-<version>-linux-x86_64.tar.gz
```

On Windows:

```
powershell -ExecutionPolicy Bypass -File scripts\bundle-windows.ps1
```

The Linux package of each published version is built from this very archive,
and from nothing else.

### Make your own pet

Three guides, in French: [`docs/format.md`](docs/format.md) (the `pet.toml`
format), [`docs/rig.md`](docs/rig.md) (drawing a character in SVG) and
[`docs/squelette.md`](docs/squelette.md) (a painted animal on a skeleton).
What was read and looked at for each breed, for the animation and for the
sounds is listed in [`docs/sources.md`](docs/sources.md).
`pets/bleu-du-maine` is a complete example.

### License

© 2026 **Alejandro San Martin Lamas**
([github.com/alejandrosml](https://github.com/alejandrosml)). Mouflon is free
software, available under the [MIT License](LICENSE-MIT) or the
[Apache License 2.0](LICENSE-APACHE), at your option: you may use, change and
share it freely, including commercially, as long as the copyright notice with
the author's name stays. The pets of this archive are under the same
licenses. Credits and third-party licenses: [CREDITS.md](CREDITS.md).

Questions: asml@laposte.net.

## Français

Mouflon amène de petits compagnons sur ton bureau, sur **Mac**, sur
**Windows**, et sur **Linux** en aperçu. Des moutons de vraies races tombent
du haut de l'écran, se promènent sur tes fenêtres, broutent, se rencontrent,
jouent, s'endorment, et se laissent attraper et lancer à travers le bureau.
Mouflon lit aussi les animaux de desktopPet (eSheep).

Cette archive est le code source d'une version de Mouflon, telle qu'elle a
été publiée. Présentation et téléchargements :
<https://alejandrosml.github.io/Mouflon/>.

### Le reconstruire

Il faut [Rust](https://rustup.rs) 1.90 ou plus récent et, sur Mac, les outils
en ligne de commande d'Xcode (`xcode-select --install`).

```sh
cargo run --release             # lancer Mouflon depuis ce dossier
cargo test --workspace          # ses essais
bash scripts/bundle-macos.sh    # Mac : dist/Mouflon.app (signature ad hoc)
bash scripts/bundle-linux.sh    # Linux : dist/Mouflon-<version>-linux-x86_64.tar.gz
```

Sous Windows :

```
powershell -ExecutionPolicy Bypass -File scripts\bundle-windows.ps1
```

Le paquet Linux de chaque version publiée est construit à partir de cette
archive même, et de rien d'autre.

### Créer ton animal

Trois modes d'emploi : [`docs/format.md`](docs/format.md) (le format
`pet.toml`), [`docs/rig.md`](docs/rig.md) (dessiner un personnage en SVG) et
[`docs/squelette.md`](docs/squelette.md) (un animal peint, sur un squelette).
Ce qui a été lu et regardé pour chaque race, pour l'animation et pour les
sons est dans [`docs/sources.md`](docs/sources.md).
`pets/bleu-du-maine` est un exemple complet.

### Licence

© 2026 **Alejandro San Martin Lamas**
([github.com/alejandrosml](https://github.com/alejandrosml)). Mouflon est un
logiciel libre, disponible sous [licence MIT](LICENSE-MIT) ou
[licence Apache 2.0](LICENSE-APACHE), au choix : tu peux l'utiliser, le
modifier et le partager librement, y compris pour un usage commercial, tant
que la mention de copyright avec le nom de l'auteur reste. Les animaux de
cette archive sont sous les mêmes licences. Crédits et licences des tiers :
[CREDITS.md](CREDITS.md).

Questions : asml@laposte.net.
