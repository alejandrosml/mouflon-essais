#!/usr/bin/env python3
"""Trois évocations sonores originales, sans échantillon ni dépendance externe.

Les WAV produits sont offerts sous CC0-1.0 ; le script suit les licences du
dépôt (MIT OR Apache-2.0). Les fréquences, durées et enveloppes sont des choix
sonores pour un compagnon de bureau, pas des mesures bioacoustiques.
"""

from __future__ import annotations

import argparse
import io
import math
from pathlib import Path
import struct
import wave


RATE = 24_000
PEAK = 0.62
TAU = 2.0 * math.pi
DURATIONS = {
    "mouflon-alarme.wav": 0.34,
    "mouflon-grondement.wav": 0.76,
    "appel-maternel.wav": 0.92,
}
REPOSITORY = Path(__file__).resolve().parents[2]
DESTINATIONS = {
    "mouflon-alarme.wav": Path("pets/mouflon-corse/sons/mouflon-alarme.wav"),
    "mouflon-grondement.wav": Path("pets/mouflon-corse/sons/mouflon-grondement.wav"),
    "appel-maternel.wav": Path("crates/mouflon-app/assets/appel-maternel.wav"),
}


class Noise:
    """Bruit à graine fixe, indépendant du générateur de la simulation."""

    def __init__(self, seed: int) -> None:
        self.state = seed

    def next(self) -> float:
        self.state = (1_664_525 * self.state + 1_013_904_223) & 0xFFFFFFFF
        return self.state / 2_147_483_648.0 - 1.0


def envelope(t: float, duration: float, attack: float, release: float) -> float:
    """Attaque et extinction à pente nulle : pas de clic aux extrémités."""
    fade_in = min(max(t / attack, 0.0), 1.0)
    fade_out = min(max((duration - t) / release, 0.0), 1.0)
    return math.sin(fade_in * math.pi / 2) ** 2 * math.sin(
        fade_out * math.pi / 2
    ) ** 2


def alarm() -> list[float]:
    """Bref souffle chuinté, avec un filet de sifflement peu tonal."""
    duration = DURATIONS["mouflon-alarme.wav"]
    noise = Noise(0x414C4152)
    low = 0.0
    band = 0.0
    phase = 0.0
    result = []
    for i in range(round(duration * RATE)):
        t = i / RATE
        raw = noise.next()
        # Deux filtres du premier ordre délimitent le souffle : rien de
        # tranchant à Nyquist, peu de graves dans le chuintement.
        low += 0.38 * (raw - low)
        band += 0.10 * (low - band)
        breath = low - band
        phase += TAU * (2_150.0 - 620.0 * t / duration) / RATE
        wavering = 0.85 + 0.15 * math.sin(TAU * 31.0 * t)
        signal = breath + 0.11 * wavering * math.sin(phase)
        shape = envelope(t, duration, 0.025, 0.12) * math.exp(-2.2 * t)
        result.append(signal * shape)
    return result


def voice(*, maternal: bool) -> list[float]:
    """Voix sourde fermée ou grondement rugueux ; aucune prise enregistrée."""
    name = "appel-maternel.wav" if maternal else "mouflon-grondement.wav"
    duration = DURATIONS[name]
    noise = Noise(0x4D455245 if maternal else 0x52555421)
    phase = 0.0
    breath = 0.0
    result = []
    for i in range(round(duration * RATE)):
        t = i / RATE
        progress = t / duration
        # Un contour discret, descendant, et une légère instabilité de voix.
        frequency = (170.0 if maternal else 104.0) * (
            1.0 + 0.07 * math.sin(math.pi * progress) - 0.10 * progress
        )
        frequency *= 1.0 + 0.009 * math.sin(TAU * 5.2 * t)
        phase += TAU * frequency / RATE
        breath += 0.16 * (noise.next() - breath)
        if maternal:
            # Bouche fermée : fondamentale et harmoniques graves dominantes,
            # sans la voyelle brillante d'un bêlement à bouche ouverte.
            signal = sum(
                amplitude * math.sin(harmonic * phase)
                for harmonic, amplitude in ((1, 1.0), (2, 0.34), (3, 0.14), (4, 0.045))
            )
            signal += 0.035 * breath
            pulse = 0.93 + 0.07 * math.sin(TAU * 4.0 * t)
        else:
            # Une irrégularité lente et du souffle donnent le grain ; les
            # harmoniques restent très en dessous de la limite de Nyquist.
            signal = sum(
                math.sin(harmonic * phase) / harmonic**1.35
                for harmonic in range(1, 13)
            )
            signal += 0.22 * breath
            pulse = 0.73 + 0.18 * math.sin(TAU * 27.0 * t) + 0.09 * math.sin(TAU * 41.0 * t)
        shape = envelope(t, duration, 0.09 if maternal else 0.05, 0.24)
        result.append(signal * pulse * shape)
    return result


def encode(samples: list[float]) -> bytes:
    """PCM mono 16 bits, crête limitée et deux extrémités exactement nulles."""
    if not samples or not all(math.isfinite(value) for value in samples):
        raise ValueError("le signal doit contenir des nombres finis")
    peak = max(abs(value) for value in samples)
    gain = PEAK / peak if peak else 0.0
    pcm = [round(value * gain * 32_767) for value in samples]
    pcm[0] = pcm[-1] = 0
    output = io.BytesIO()
    with wave.open(output, "wb") as writer:
        writer.setnchannels(1)
        writer.setsampwidth(2)
        writer.setframerate(RATE)
        writer.writeframes(struct.pack(f"<{len(pcm)}h", *pcm))
    return output.getvalue()


def generate() -> dict[str, bytes]:
    return {
        "mouflon-alarme.wav": encode(alarm()),
        "mouflon-grondement.wav": encode(voice(maternal=False)),
        "appel-maternel.wav": encode(voice(maternal=True)),
    }


def main() -> None:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument(
        "destination", nargs="?", type=Path,
        help="dossier d'essai ; sans argument, régénère les fichiers du dépôt",
    )
    args = parser.parse_args()
    for name, content in generate().items():
        path = args.destination / name if args.destination else REPOSITORY / DESTINATIONS[name]
        path.parent.mkdir(parents=True, exist_ok=True)
        path.write_bytes(content)
        print(f"{path} : {len(content)} octets")


if __name__ == "__main__":
    main()
