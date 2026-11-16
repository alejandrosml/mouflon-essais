#!/usr/bin/env python3
"""Le redon du Mérinos d'Arles : trois coups de cloche coupés dans un
enregistrement de troupeau transhumant.

La source est « Herd of 300 sheep #1 » de BigSoundBank (n° 2971, Joseph
SARDIN, CC0 1.0) : 300 moutons réunis dans un pré avant la transhumance, sans
un bêlement, seulement leurs sonnailles. Elle n'est pas dans le dépôt :

    https://bigsoundbank.com/herd-of-300-sheep-1-s2971.html

Dans ce concert, la plus grosse cloche (son fondamental vers 388 Hz, des
partiels vers 1 012, 1 760, 2 530 et 3 140 Hz) frappe souvent seule. On garde
trois de ses coups, chacun avec le choc du battant intact ; ensuite, un masque
doux garde les partiels de cette cloche et baisse de 14 dB tout le reste (les
autres cloches qui sonnent derrière). Puis un fondu, et le MP3.

    python3 tools/audio/sonnaille.py 2971.flac [dossier]

Sans dossier, les trois fichiers sont réécrits dans
`pets/merinos-d-arles/sons/`. Il faut numpy, scipy et ffmpeg.
"""

import subprocess
import sys
import tempfile
import wave
from pathlib import Path

import numpy as np
from scipy.signal import butter, find_peaks, sosfiltfilt

SR = 44100
# Les trois coups retenus (secondes dans l'enregistrement) : ceux où la grosse
# cloche frappe le plus nettement au-dessus des autres.
COUPS = (4.27, 49.08, 26.96)
DUREE = 0.8
# Ce que perdent les autres cloches, hors des partiels de la grosse.
ATTENUATION_DB = 14.0


def lire(source: Path) -> np.ndarray:
    """L'enregistrement, en mono à 44,1 kHz, en flottants."""
    brut = subprocess.run(
        ["ffmpeg", "-v", "error", "-i", str(source), "-ac", "1", "-ar", str(SR),
         "-f", "f32le", "-"],
        check=True, capture_output=True,
    ).stdout
    return np.frombuffer(brut, dtype="<f4").astype(np.float64)


def attaque(x: np.ndarray, t: float) -> int:
    """L'instant du choc : la plus forte montée de l'enveloppe (2 ms) près de
    `t`, puis en arrière jusqu'à 20 % de son sommet."""
    a0, a1 = int((t - 0.04) * SR), int((t + 0.12) * SR)
    env = np.sqrt(np.convolve(x[a0:a1] ** 2, np.ones(88) / 88, mode="same"))
    montee = int(np.argmax(np.diff(env)))
    sommet = montee + int(np.argmax(env[montee:montee + int(0.02 * SR)]))
    i = montee
    while i > 0 and env[i] > 0.2 * env[sommet]:
        i -= 1
    return a0 + i


def coup(x: np.ndarray, t: float) -> np.ndarray:
    """Un coup de la grosse cloche, les autres atténuées derrière."""
    debut_choc = attaque(x, t)
    debut = debut_choc - int(0.004 * SR)
    fin = debut + int(DUREE * SR)
    fen, pas = 2048, 256
    hann = np.hanning(fen)
    freqs = np.fft.rfftfreq(fen, 1 / SR)

    def spectre(a: int, b: int) -> np.ndarray:
        trames = np.lib.stride_tricks.sliding_window_view(x[a:b], fen)[::pas] * hann
        return np.abs(np.fft.rfft(trames, axis=1)).mean(axis=0)

    # Les partiels de cette cloche : ce qui monte au choc, par rapport aux
    # 0,6 s d'avant.
    apres = spectre(debut_choc, debut_choc + int(0.25 * SR))
    avant = spectre(debut_choc - int(0.6 * SR), debut_choc - int(0.01 * SR))
    gain = 20 * np.log10((apres + 1e-9) / (avant + 1e-9))
    niveau = 20 * np.log10(apres + 1e-9)
    pics, _ = find_peaks(niveau, prominence=4, distance=3)
    pics = [p for p in pics if gain[p] > 6 and niveau[p] > niveau.max() - 40 and freqs[p] > 180]
    masque = np.full(len(freqs), 10 ** (-ATTENUATION_DB / 20))
    for p in pics:
        masque[max(0, p - 3):p + 4] = 1.0
    masque = np.convolve(masque, np.ones(3) / 3, mode="same")

    # Le masque s'applique après les 15 premières ms : le choc reste entier.
    morceau = x[debut - fen:fin + fen]
    n = (len(morceau) - fen) // pas + 1
    trames = np.stack([morceau[i * pas:i * pas + fen] * hann for i in range(n)])
    S = np.fft.rfft(trames, axis=1)
    centre = (np.arange(n) * pas + fen / 2 - fen) / SR
    k = np.clip((centre - 0.015) / 0.02, 0, 1)[:, None]
    S *= (1 - k) + k * masque[None, :]
    y = np.zeros(len(morceau))
    poids = np.zeros(len(morceau))
    for i, trame in enumerate(np.fft.irfft(S, n=fen, axis=1)):
        y[i * pas:i * pas + fen] += trame * hann
        poids[i * pas:i * pas + fen] += hann ** 2
    y = (y / np.maximum(poids, 1e-6))[fen:fen + (fin - debut)]

    # Fondus : 3 ms à l'entrée, un demi-cosinus sur les 45 % de la fin.
    entree = int(0.003 * SR)
    y[:entree] *= np.linspace(0, 1, entree)
    sortie = int(0.45 * len(y))
    y[-sortie:] *= 0.5 * (1 + np.cos(np.linspace(0, np.pi, sortie)))
    return y * (0.7 / np.abs(y).max())


def ecrire_mp3(y: np.ndarray, chemin: Path) -> None:
    with tempfile.TemporaryDirectory() as dossier:
        wav = Path(dossier) / "coup.wav"
        with wave.open(str(wav), "wb") as w:
            w.setnchannels(1)
            w.setsampwidth(2)
            w.setframerate(SR)
            w.writeframes((np.clip(y, -1, 1) * 32767).astype("<i2").tobytes())
        subprocess.run(
            ["ffmpeg", "-v", "error", "-y", "-i", str(wav), "-ac", "1", "-ar", str(SR),
             "-c:a", "libmp3lame", "-q:a", "3", str(chemin)],
            check=True,
        )


def main() -> None:
    if len(sys.argv) < 2:
        sys.exit(__doc__)
    source = Path(sys.argv[1])
    racine = Path(__file__).resolve().parents[2]
    dossier = Path(sys.argv[2]) if len(sys.argv) > 2 else racine / "pets/merinos-d-arles/sons"
    dossier.mkdir(parents=True, exist_ok=True)
    x = sosfiltfilt(butter(4, 180, "highpass", fs=SR, output="sos"), lire(source))
    for i, t in enumerate(COUPS, start=1):
        chemin = dossier / f"sonnaille-{i}.mp3"
        ecrire_mp3(coup(x, t), chemin)
        print(chemin)


if __name__ == "__main__":
    main()
