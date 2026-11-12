#!/usr/bin/env python3
"""Ce qu'un compositeur recevrait de Mouflon sous X11.

Un écran virtuel (Xvfb) n'a pas de compositeur : la fenêtre transparente de
Mouflon y paraît noire. Ce script lit cette fenêtre à couche alpha
(profondeur 32), compte ses pixels transparents, vérifie que ses couleurs
sont prémultipliées, puis la compose lui-même sur les autres fenêtres : on
voit l'écran tel qu'un vrai bureau le montrerait.

    python3 tools/essai/alpha_x11.py apercu/linux-compose.png

À lancer sur le même écran que Mouflon (la même variable DISPLAY). Il lui faut
python-xlib, numpy et Pillow.
"""
import sys

import numpy as np
from PIL import Image
from Xlib import X, display


def pixels(window, geometry):
    image = window.get_image(0, 0, geometry.width, geometry.height, X.ZPixmap, 0xFFFFFFFF)
    data = np.frombuffer(image.data, dtype=np.uint8)
    return data.reshape(geometry.height, geometry.width, 4)  # B, G, R, A (ou X)


def simulated():
    """Les écrans simulés (`MOUFLON_ECRANS`, voir `screens.rs`) : x, y, largeur,
    hauteur, barre des tâches."""
    import os

    screens = []
    for one in os.environ.get("MOUFLON_ECRANS", "").split(";"):
        try:
            n = [float(v) for v in one.split(",")]
        except ValueError:
            continue
        if len(n) in (4, 5) and n[2] >= 1 and n[3] >= 1:
            screens.append((*[int(round(v)) for v in n[:4]], int(round(n[4])) if len(n) == 5 else 0))
    return screens


def main():
    d = display.Display()
    root = d.screen().root
    screen = root.get_geometry()
    canvas = np.full((screen.height, screen.width, 3), 128, dtype=np.float32)
    # Des écrans simulés : ce qui n'est sur aucun d'eux est assombri, chacun
    # a un fond à lui et sa barre des tâches. On voit ainsi d'un coup d'œil
    # si un animal est coupé au bord d'un écran, ou perdu entre deux.
    screens = simulated()
    if screens:
        canvas[:] = 40
        for k, (x, y, w, h, bar) in enumerate(screens):
            canvas[y : y + h, x : x + w] = (150 + 18 * (k % 3), 170, 190 - 18 * (k % 3))
            if bar:
                canvas[y + h - bar : y + h, x : x + w] = (70, 80, 110)
    overlays = []
    for window in root.query_tree().children:  # du fond vers le dessus
        try:
            attributes = window.get_attributes()
            g = window.get_geometry()
        except Exception:
            continue
        if attributes.map_state != X.IsViewable:
            continue
        print(f"fenêtre {window.id:#x} profondeur {g.depth} {g.width}x{g.height} en {g.x},{g.y}")
        # Les fenêtres de Mouflon : une par écran, à couche alpha.
        if g.depth == 32 and g.width * g.height * 16 >= screen.width * screen.height:
            overlays.append((pixels(window, g), g))
            continue
        if g.depth != 24 or g.x < 0 or g.y < 0:
            continue
        try:
            p = pixels(window, g)
        except Exception as error:
            print("  illisible :", error)
            continue
        h = min(g.height, screen.height - g.y)
        w = min(g.width, screen.width - g.x)
        canvas[g.y : g.y + h, g.x : g.x + w] = p[:h, :w, 2::-1]
    if not overlays:
        sys.exit("pas de fenêtre à couche alpha : Mouflon ne serait pas transparent")
    total = clear = solid = 0
    excess = 0.0
    for overlay, g in overlays:
        h = min(g.height, screen.height - g.y)
        w = min(g.width, screen.width - g.x)
        if g.x < 0 or g.y < 0 or h <= 0 or w <= 0:
            print(f"  fenêtre de Mouflon hors de l'écran : {g.width}x{g.height} en {g.x},{g.y}")
            continue
        alpha = overlay[:h, :w, 3].astype(np.float32)
        rgb = overlay[:h, :w, 2::-1].astype(np.float32)
        total += alpha.size
        clear += int((alpha == 0).sum())
        solid += int((alpha == 255).sum())
        excess = max(excess, float((rgb.max(axis=2) - alpha).max()))
        under = canvas[g.y : g.y + h, g.x : g.x + w]
        canvas[g.y : g.y + h, g.x : g.x + w] = rgb + under * (1.0 - alpha[:, :, None] / 255.0)
    print(f"fenêtres de Mouflon : {len(overlays)}")
    print(f"transparents {clear / total:.1%}, pleins {solid / total:.2%}, entre deux {(total - clear - solid) / total:.2%}")
    print(f"couleur au-delà de l'alpha (prémultiplié : 0 ou 1) : {excess:.0f}")
    Image.fromarray(np.clip(canvas, 0, 255).astype(np.uint8)).save(sys.argv[1])
    print("écrit", sys.argv[1])


main()
