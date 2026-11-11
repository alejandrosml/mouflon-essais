"""Rend chaque dessin de chaque race (son bélier, sa brebis, son agneau) à la
même échelle (PX_PAR_CM), sur fond transparent, rogné à son dessin :
`python3 tools/vitrine/rendre_races.py <dossier>`.

C'est le moteur qui dessine (`mouflon-preview <pet.toml> <dossier> portraits`),
comme dans l'app : il faut `target/release/mouflon-preview`
(`cargo build --release -p mouflon-preview`) et Pillow."""
import json
import pathlib
import subprocess
import sys
import tomllib

from PIL import Image

REPO = pathlib.Path(__file__).resolve().parents[2]
APERCU = REPO / "target/release/mouflon-preview"
OUT = pathlib.Path(sys.argv[1])
OUT.mkdir(parents=True, exist_ok=True)
PX_PAR_CM = 3.0
# Le sol est à tant de pixels du bas de chaque portrait (voir `render_portraits`).
SOL = 24
# Le nom de chaque dessin dans la page ; `base` : un animal qui n'en a qu'un.
NOMS = {"male": "belier", "female": "brebis", "lamb": "agneau", "base": "seul"}

if not APERCU.exists():
    sys.exit("il manque target/release/mouflon-preview : cargo build --release -p mouflon-preview")

infos = []
for dossier in sorted((REPO / "pets").iterdir()):
    toml = dossier / "pet.toml"
    if not toml.exists():
        continue
    meta = tomllib.loads(toml.read_text())
    brut = OUT / "brut" / dossier.name
    brut.mkdir(parents=True, exist_ok=True)
    subprocess.run([str(APERCU), str(toml), str(brut), "portraits", str(PX_PAR_CM)],
                   check=True, capture_output=True)
    dessins = {}
    for png in sorted(brut.glob("*.png")):
        nom = NOMS[png.stem]
        image = Image.open(png)
        gauche, haut, droite, _ = image.getbbox()
        # Rogné à son dessin, mais le bas reste le sol : les animaux d'une
        # famille se posent sur la même ligne.
        image = image.crop((gauche, haut, droite, image.height - SOL))
        image.save(OUT / f"{dossier.name}-{nom}.webp", "WEBP", quality=86, method=6)
        dessins[nom] = {"w": image.width, "h": image.height}
    infos.append({"id": dossier.name, "name": meta["pet"]["name"], "dessins": dessins,
                  "description": meta["pet"].get("description", "")})
(OUT / "races.json").write_text(json.dumps(infos, ensure_ascii=False, indent=1))
print(len(infos), "animaux,", sum(len(i["dessins"]) for i in infos), "dessins")
