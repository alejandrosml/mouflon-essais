#!/usr/bin/env python3
"""Images intermédiaires pour les animaux Mouflon.

Outil de préparation des images, à lancer une fois par animal : il ne fait
pas partie de l'app (écrite entièrement en Rust), qui lit seulement les
images qu'il produit.

Entre deux images qui se suivent dans une animation, le réseau RIFE (Huang et
al., licence MIT) estime le mouvement ; l'outil applique ce mouvement aux
quatre canaux de l'image (couleur et transparence ensemble), pour une ou
plusieurs images intermédiaires. Les paires trop différentes (un demi-tour
de profil à face, une transformation) donneraient un fondu à deux têtes :
elles restent sans image intermédiaire. Une culbute dessinée (chaque image
est la précédente tournée) se dédoublerait aussi : l'outil la reconnaît et
fait tourner le dessin lui-même.

    pip install numpy pillow opencv-python-headless onnx onnxruntime
    # modèle : https://huggingface.co/walterlow/RIFE_fp32_timestep (MIT)
    python3 tools/inbetween/inbetween.py tests/donnees/lainou --model RIFE_fp32_timestep.onnx
    python3 tools/inbetween/inbetween.py tests/donnees/pelote --model RIFE_fp32_timestep.onnx

Il écrit `entre-deux.toml` et les images à côté du `pet.toml`, puis il reste
à ajouter `inbetweens = "entre-deux.toml"` dans la section [appearance].
Avec `--review planche.png`, une planche de contrôle montre chaque paire, ses
images intermédiaires et sa note de « fondu ». Pour des bandes, `--count 1`
donne une image intermédiaire après chaque image, et `--count-for tombe=3`
en donne davantage à une animation rapide (une chute, une culbute).
"""

import argparse
import math
import re
import sys
import tomllib
import xml.etree.ElementTree as ET
from pathlib import Path

import cv2
import numpy as np
import onnx
import onnxruntime as ort
from onnx import TensorProto, helper
from PIL import Image, ImageDraw

# Fond sur lequel le réseau voit l'animal : neutre, pour que les contours
# clairs comme sombres se détachent.
BACKGROUND = np.array([0.5, 0.5, 0.5], dtype=np.float32)
# Durée visée par image intermédiaire dans une planche desktopPet (secondes).
TARGET_STEP = 0.04
MAX_PER_PAIR = 3


# ---------------------------------------------------------------- réseau RIFE

class Rife:
    """RIFE, avec en plus le mouvement de sa dernière étape : les grilles
    d'échantillonnage des deux images et le masque qui les mêle."""

    def __init__(self, path):
        model = onnx.load(path)
        graph = model.graph
        pads = {n.output[0] for n in graph.node if n.op_type == 'Pad'}
        samples = [n for n in graph.node if n.op_type == 'GridSample' and n.input[0] in pads]
        sigmoids = [n for n in graph.node if n.op_type == 'Sigmoid']
        if len(samples) < 2 or not sigmoids:
            sys.exit(f"{path} : ce n'est pas le modèle RIFE attendu")
        self.names = [samples[-2].input[1], samples[-1].input[1], sigmoids[-1].output[0], samples[-2].input[0]]
        for name in self.names:
            graph.output.append(helper.make_tensor_value_info(name, TensorProto.FLOAT, ['n', 'c', 'h', 'w']))
        opts = ort.SessionOptions()
        opts.intra_op_num_threads = 4
        self.session = ort.InferenceSession(model.SerializeToString(), opts, providers=['CPUExecutionProvider'])
        self.checked = False

    def motion(self, a, b, t):
        inp = np.concatenate([over(a).transpose(2, 0, 1), over(b).transpose(2, 0, 1)])[None]
        out, grid0, grid1, mask, padded = self.session.run(
            None, {'input': inp.astype(np.float32), 'timestep': np.array(t, dtype=np.float32)})
        return out[0].transpose(1, 2, 0), grid0[0], grid1[0], mask[0, 0][..., None], padded.shape[2:]

    def tween(self, a, b, t):
        """Image RGBA au temps t (0 < t < 1) entre a et b, et sa note de fondu."""
        rgb, grid0, grid1, mask, shape = self.motion(a, b, t)
        h, w = a.shape[:2]
        w0 = sample(pad(premul(a), shape), grid0)
        w1 = sample(pad(premul(b), shape), grid1)
        if not self.checked:
            # Le même calcul sur l'image que voit le réseau doit redonner sa sortie.
            mine = sample(pad(over(a), shape), grid0) * mask + sample(pad(over(b), shape), grid1) * (1 - mask)
            gap = float(np.abs(mine[:h, :w] - rgb).max())
            if gap > 1e-3:
                sys.exit(f"le mouvement lu dans le modèle ne redonne pas son image (écart {gap:.4f})")
            self.checked = True
        out = (w0 * mask + w1 * (1 - mask))[:h, :w]
        out[..., :3] = np.minimum(out[..., :3], out[..., 3:4])
        return unpremul(out), Ghost(w0[:h, :w], w1[:h, :w], mask[:h, :w])


def premul(rgba):
    return np.concatenate([rgba[..., :3] * rgba[..., 3:4], rgba[..., 3:4]], axis=-1)


def unpremul(p):
    a = p[..., 3:4]
    rgb = np.where(a > 1e-3, p[..., :3] / np.maximum(a, 1e-3), 0)
    return np.concatenate([np.clip(rgb, 0, 1), np.clip(a, 0, 1)], axis=-1)


def over(rgba):
    a = rgba[..., 3:4]
    return rgba[..., :3] * a + BACKGROUND * (1 - a)


def pad(img, shape):
    h, w = img.shape[:2]
    return np.pad(img, ((0, shape[0] - h), (0, shape[1] - w), (0, 0)))


def sample(img, grid):
    """GridSample d'ONNX : bilinéaire, coins alignés, bord répété."""
    h, w = img.shape[:2]
    mx = ((grid[..., 0] + 1) * 0.5 * (w - 1)).astype(np.float32)
    my = ((grid[..., 1] + 1) * 0.5 * (h - 1)).astype(np.float32)
    out = cv2.remap(img.astype(np.float32), mx, my, cv2.INTER_LINEAR, borderMode=cv2.BORDER_REPLICATE)
    return out if out.ndim == 3 else out[..., None]


class Ghost:
    """Notes de « fondu » d'une image intermédiaire : là où les deux images
    déplacées ne s'accordent pas et se mêlent à parts égales, on verrait deux
    têtes ou deux pattes plutôt qu'un mouvement.

    - `overall` : part de l'animal en désaccord (sensible à la texture : un
      dessin peint en a toujours un peu) ;
    - `silhouette` : désaccord de la seule transparence (une forme qui se
      dédouble) ;
    - `local` : le désaccord le plus fort sur une petite zone (une tête qui
      apparaît au milieu du corps), le grain de la texture s'y moyenne."""

    def __init__(self, w0, w1, mask):
        alpha = np.maximum(w0[..., 3], w1[..., 3])
        area = max(float(alpha.sum()), 1.0)
        mix = np.minimum(mask[..., 0], 1 - mask[..., 0]) * 2
        self.overall = float((np.abs(w0 - w1).max(axis=-1) * mix * alpha).sum() / area)
        self.silhouette = float((np.abs(w0[..., 3] - w1[..., 3]) * mix).sum() / area)
        colour = (np.abs(w0[..., :3] - w1[..., :3]).max(axis=-1) * mix).astype(np.float32)
        self.local = float(cv2.GaussianBlur(colour, (0, 0), w0.shape[0] * 0.035).max())

    def worst(self, other):
        if other is None:
            return self
        merged = Ghost.__new__(Ghost)
        merged.overall = max(self.overall, other.overall)
        merged.silhouette = max(self.silhouette, other.silhouette)
        merged.local = max(self.local, other.local)
        return merged

    def too_much(self, args):
        return (self.overall > args.max_ghost or self.silhouette > args.max_silhouette
                or self.local > args.max_local)

    def __str__(self):
        return f"fondu {self.overall:.3f} / {self.silhouette:.3f} / {self.local:.2f}"


def inbetweens(rife, a, b, count, args):
    """`count` images de a vers b, ou rien si le passage est un fondu."""
    if np.array_equal(a, b):
        return None, None
    images, worst = [], None
    for j in range(1, count + 1):
        image, ghost = rife.tween(a, b, j / (count + 1))
        images.append(image)
        worst = ghost.worst(worst)
    if not worst.too_much(args):
        return images, worst
    # Le réseau ne sait pas faire tourner un dessin : une culbute dessinée
    # image par image se dédouble. On essaie alors de la tourner nous-mêmes.
    turned, turn = rotation_tweens(a, b, count, args)
    if turned is not None:
        return turned, turn
    return None, worst


# ---------------------------------------------------------------- culbutes

# Une image suivante est une rotation de la précédente quand, tournée,
# l'image de départ en explique l'essentiel : il en reste au plus cette part
# de différence, pour un quart de tour au moins.
MAX_ROTATION_RESIDUE = 0.35
MIN_TURN = 10.0


def centroid(rgba):
    alpha = rgba[..., 3]
    total = max(float(alpha.sum()), 1e-6)
    ys, xs = np.mgrid[0:alpha.shape[0], 0:alpha.shape[1]]
    return np.array([(xs * alpha).sum() / total, (ys * alpha).sum() / total])


def turn(rgba, degrees, centre, to):
    """L'image (prémultipliée) tournée autour de `centre`, posée en `to`."""
    m = cv2.getRotationMatrix2D((float(centre[0]), float(centre[1])), float(degrees), 1.0)
    m[:, 2] += to - centre
    return cv2.warpAffine(premul(rgba).astype(np.float32), m, (rgba.shape[1], rgba.shape[0]),
                          flags=cv2.INTER_LINEAR, borderMode=cv2.BORDER_CONSTANT, borderValue=0)


def disagreement(p, q):
    alpha = np.maximum(p[..., 3], q[..., 3])
    return float((np.abs(p - q).max(axis=-1) * alpha).sum() / max(float(alpha.sum()), 1.0))


def rotation_tweens(a, b, count, args):
    """Images intermédiaires d'une culbute dessinée : b est a tourné (et un peu
    déplacé). Chaque image intermédiaire tourne a, et b en sens inverse, de la
    part voulue de l'angle ; si les deux dessins ne sont pas tout à fait le
    même tourné, on prend le plus proche, tourné, sans fondu."""
    ca, cb = centroid(a), centroid(b)
    pb = premul(b)
    base = disagreement(turn(a, 0.0, ca, cb), pb)
    if base <= 1e-6:
        return None, None
    angle, error = 0.0, base
    for coarse in np.arange(-180.0, 180.0, 3.0):
        e = disagreement(turn(a, coarse, ca, cb), pb)
        if e < error:
            angle, error = coarse, e
    for fine in np.arange(angle - 3.0, angle + 3.0, 0.25):
        e = disagreement(turn(a, fine, ca, cb), pb)
        if e < error:
            angle, error = fine, e
    if abs(angle) < MIN_TURN or error / base > MAX_ROTATION_RESIDUE:
        return None, None
    pairs, worst = [], None
    for j in range(1, count + 1):
        t = j / (count + 1)
        at = ca + (cb - ca) * t
        from_a = turn(a, angle * t, ca, at)
        from_b = turn(b, -angle * (1 - t), cb, at)
        worst = Ghost(from_a, from_b, np.full(from_a.shape[:2] + (1,), 1 - t, np.float32)).worst(worst)
        pairs.append((from_a, from_b, t))
    if worst.too_much(args):
        images = [unpremul(from_a if t <= 0.5 else from_b) for from_a, from_b, t in pairs]
    else:
        images = [unpremul(from_a * (1 - t) + from_b * t) for from_a, from_b, t in pairs]
    return images, worst


# ---------------------------------------------------------------- images

def load(path):
    return np.asarray(Image.open(path).convert('RGBA'), dtype=np.float32) / 255


def save(path, rgba):
    path.parent.mkdir(parents=True, exist_ok=True)
    Image.fromarray((np.clip(rgba, 0, 1) * 255).round().astype(np.uint8), 'RGBA').save(path, optimize=True)


def tiles_of(image, tile_w, tile_h):
    rows, cols = image.shape[0] // tile_h, image.shape[1] // tile_w
    return [image[y * tile_h:(y + 1) * tile_h, x * tile_w:(x + 1) * tile_w]
            for y in range(rows) for x in range(cols)]


class Review:
    """Planche de contrôle : une ligne par paire, réduite de moitié."""

    def __init__(self):
        self.rows = []

    def add(self, label, a, images, b, ghost, kept):
        cells = [a] + (images or []) + [b]
        row = np.concatenate(cells, axis=1)
        img = Image.fromarray((row * 255).round().astype(np.uint8), 'RGBA')
        bg = Image.new('RGBA', img.size, (236, 234, 226, 255) if kept else (250, 214, 214, 255))
        bg.alpha_composite(img)
        bg = bg.convert('RGB').resize((img.width // 2, img.height // 2), Image.LANCZOS)
        ImageDraw.Draw(bg).text((4, 2), f"{label}  {ghost or 'identiques'}", fill=(60, 60, 60))
        self.rows.append(bg)

    def save(self, path):
        if not self.rows:
            return
        width = max(r.width for r in self.rows)
        sheet = Image.new('RGB', (width, sum(r.height for r in self.rows)), (255, 255, 255))
        y = 0
        for r in self.rows:
            sheet.paste(r, (0, y))
            y += r.height
        sheet.save(path)


# ---------------------------------------------------------------- bandes

def strips_mode(folder, appearance, rife, args, review):
    """Une bande d'images intermédiaires par animation et par sens."""
    frame_w, frame_h = appearance['frame']
    patterns = [appearance[k] for k in ('strips_left', 'strips_right') if k in appearance]
    outputs = [re.sub(r'^[^/]+/', 'entre-deux/', p, count=1) if '/' in p else 'entre-deux/' + p for p in patterns]
    first = folder / patterns[0]
    names = sorted(p.stem for p in first.parent.glob(first.name.replace('{name}', '*')))
    skip = {}
    counts = dict(args.count_for or [])
    for name in names:
        sides = [tiles_of(load(folder / p.replace('{name}', name)), frame_w, frame_h) for p in patterns]
        n = len(sides[0])
        if n < 2:
            continue
        count = counts.get(name, args.count)
        strips = [[] for _ in sides]
        for i in range(n):
            results = []
            for frames in sides:
                images, ghost = inbetweens(rife, frames[i], frames[(i + 1) % n], count, args)
                results.append((images, ghost))
            kept = all(images is not None for images, _ in results)
            for side, (frames, (images, ghost)) in enumerate(zip(sides, results)):
                if review and side == 0:
                    review.add(f"{name} {i}→{(i + 1) % n}", frames[i], images, frames[(i + 1) % n], ghost, kept)
                empty = np.zeros_like(frames[i])
                strips[side].extend(images if kept else [empty] * count)
            if not kept:
                skip.setdefault(name, []).append(i)
        for out, strip in zip(outputs, strips):
            save(folder / out.replace('{name}', name), np.concatenate(strip, axis=1))
        print(f"{name} : {n} images, {n - len(skip.get(name, []))} passages adoucis", flush=True)

    lines = ["# Images intermédiaires, calculées par tools/inbetween (RIFE) : à refaire",
             "# avec l'outil si les images de l'animal changent.",
             f"count = {args.count}"]
    for key, out in zip(('strips_left', 'strips_right'), outputs):
        lines.append(f'{key} = "{out}"')
    own = {name: c for name, c in sorted(counts.items()) if name in names and c != args.count}
    if own:
        lines.append("")
        lines.append("# Animations rapides : plus d'images intermédiaires après chaque image.")
        lines.append("[counts]")
        lines.extend(f"{name} = {c}" for name, c in own.items())
    if skip:
        lines.append("")
        lines.append("# Images trop différentes de la suivante : pas d'image intermédiaire après elles.")
        lines.append("[skip]")
        lines.extend(f"{name} = {sorted(indices)}" for name, indices in sorted(skip.items()))
    return lines


def count_for(text):
    """« tombe=3 » → ("tombe", 3)."""
    name, _, value = text.partition('=')
    if not name or not value.isdigit() or int(value) < 1:
        raise argparse.ArgumentTypeError(f"« {text} » : il faut NOM=N, N au moins 1")
    return name, int(value)


# ---------------------------------------------------------------- planche desktopPet

def number(text, default):
    try:
        return float((text or '').strip())
    except ValueError:
        return default


def desktoppet_pairs(xml_path):
    """Paires d'images qui se suivent dans les animations, avec la plus longue
    tenue de la première avant la seconde (secondes). desktopPet double
    souvent ses images (66, 66, 67, 67…) : c'est la tenue entière qui compte."""
    root = ET.parse(xml_path).getroot()
    for el in root.iter():
        el.tag = el.tag.split('}')[-1]
    pairs = {}
    for anim in root.iter('animation'):
        seq = anim.find('sequence')
        if seq is None:
            continue
        frames = [int(f.text) for f in seq.findall('frame') if f.text and f.text.strip().isdigit()]
        if len(frames) < 2:
            continue
        start = anim.find('start')
        end = anim.find('end')
        interval = max(number(start.findtext('interval') if start is not None else None, 0.1 * 1000),
                       number(end.findtext('interval') if end is not None else None, 0.1 * 1000)) / 1000
        repeat_from = int(number(seq.get('repeatfrom'), 0))
        # Une animation qui boucle (ou qu'on tient) revient de la dernière image à sa reprise.
        following = frames[1:] + [frames[min(repeat_from, len(frames) - 1)]]
        held = 1
        for a, b in zip(frames, following):
            if a == b:
                held += 1
                continue
            pairs[(a, b)] = max(pairs.get((a, b), 0.0), interval * held)
            held = 1
    return pairs


def sheet_mode(folder, pet, rife, args, review):
    appearance = pet['appearance']
    tiles_x, tiles_y = appearance['tiles']
    sheet = load(folder / appearance['sprites'])
    tile_w, tile_h = sheet.shape[1] // tiles_x, sheet.shape[0] // tiles_y
    tiles = tiles_of(sheet, tile_w, tile_h)
    pairs = desktoppet_pairs(folder / pet['desktoppet']['animations'])
    images, table = [], []
    for (a, b), interval in sorted(pairs.items()):
        if a >= len(tiles) or b >= len(tiles):
            continue
        count = max(0, min(MAX_PER_PAIR, round(interval / TARGET_STEP) - 1))
        if args.count_max is not None:
            count = min(count, args.count_max)
        if count == 0:
            continue
        made, ghost = inbetweens(rife, tiles[a], tiles[b], count, args)
        if review:
            review.add(f"{a}→{b} ({interval * 1000:.0f} ms)", tiles[a], made, tiles[b], ghost, made is not None)
        if made is None:
            continue
        table.append((a, b, len(images), count))
        images.extend(made)
    columns = tiles_x
    rows = math.ceil(len(images) / columns) if images else 1
    out = np.zeros((rows * tile_h, columns * tile_w, 4), dtype=np.float32)
    for i, image in enumerate(images):
        x, y = i % columns, i // columns
        out[y * tile_h:(y + 1) * tile_h, x * tile_w:(x + 1) * tile_w] = image
    save(folder / 'entre-deux.png', out)
    print(f"{len(table)} paires adoucies sur {len(pairs)}, {len(images)} images", flush=True)
    lines = ["# Images intermédiaires, calculées par tools/inbetween (RIFE) : à refaire",
             "# avec l'outil si les images de l'animal changent.",
             'sprites = "entre-deux.png"',
             f"tiles = [{columns}, {rows}]",
             "# [image de départ, image d'arrivée, première case, nombre de cases]",
             "pairs = ["]
    lines.extend(f"    [{a}, {b}, {first}, {count}]," for a, b, first, count in table)
    lines.append("]")
    return lines


def main():
    parser = argparse.ArgumentParser(description=__doc__.split('\n\n')[0])
    parser.add_argument('folder', type=Path, help="dossier de l'animal (avec son pet.toml)")
    parser.add_argument('--model', required=True, help='RIFE_fp32_timestep.onnx')
    parser.add_argument('--count', type=int, default=1, help="bandes : images intermédiaires après chaque image")
    parser.add_argument('--count-for', type=count_for, action='append', metavar='NOM=N',
                        help="bandes : autant d'images intermédiaires pour cette animation (plusieurs fois possible)")
    parser.add_argument('--count-max', type=int, default=None, help="planche : au plus tant d'images par paire")
    parser.add_argument('--max-ghost', type=float, default=0.018,
                        help="note de fondu d'ensemble au-delà de laquelle une paire est sautée "
                             "(plus haut pour un dessin peint, dont la texture change d'une image à l'autre)")
    parser.add_argument('--max-silhouette', type=float, default=0.02, help='note de fondu de la silhouette')
    parser.add_argument('--max-local', type=float, default=0.15, help='note de fondu la plus forte sur une petite zone')
    parser.add_argument('--review', type=Path, help='planche de contrôle à écrire')
    args = parser.parse_args()

    pet = tomllib.loads((args.folder / 'pet.toml').read_text(encoding='utf-8'))
    appearance = pet.get('appearance', {})
    rife = Rife(args.model)
    review = Review() if args.review else None
    if 'strips_left' in appearance or 'strips_right' in appearance:
        lines = strips_mode(args.folder, appearance, rife, args, review)
    elif 'sprites' in appearance and 'desktoppet' in pet:
        lines = sheet_mode(args.folder, pet, rife, args, review)
    else:
        sys.exit("il faut un animal en bandes d'images, ou une planche avec un fichier desktopPet")
    (args.folder / 'entre-deux.toml').write_text('\n'.join(lines) + '\n', encoding='utf-8')
    if review:
        review.save(args.review)
    print('écrit : entre-deux.toml ; à ajouter dans [appearance] : inbetweens = "entre-deux.toml"')


if __name__ == '__main__':
    main()
