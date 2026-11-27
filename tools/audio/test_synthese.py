"""Contrat des sons livrés : format lisible, niveau discret et génération stable."""

import hashlib
import io
import math
import struct
import tomllib
import unittest
import wave

import synthese


class SyntheticCalls(unittest.TestCase):
    @classmethod
    def setUpClass(cls):
        cls.clips = synthese.generate()

    def test_the_same_synthesis_produces_the_same_audio_bytes(self):
        self.assertEqual(self.clips, synthese.generate())
        self.assertEqual(len(set(hashlib.sha256(data).digest() for data in self.clips.values())), 3)

    def test_checked_in_calls_are_exactly_the_generated_audio(self):
        for name, content in self.clips.items():
            with self.subTest(call=name):
                self.assertEqual((synthese.REPOSITORY / synthese.DESTINATIONS[name]).read_bytes(), content)

    def test_the_mouflon_pack_keeps_its_calls_and_other_sounds_inside_its_folder(self):
        folder = synthese.REPOSITORY / "pets/mouflon-corse"
        pet = tomllib.loads((folder / "pet.toml").read_text())
        for sound in pet["sounds"].values():
            files = [sound] if isinstance(sound, str) else sound["files"]
            for name in files:
                with self.subTest(file=name):
                    path = (folder / name).resolve()
                    self.assertTrue(path.is_relative_to(folder))
                    self.assertGreater(len(path.read_bytes()), 44)
        behaviors = {behavior["name"]: behavior for behavior in pet["behavior"]}
        for behavior, sound in (("stamp", "alarme"), ("flehmen", "grondement")):
            self.assertEqual(behaviors[behavior]["sound"], sound)
            self.assertFalse(pet["sounds"][sound]["mouth"])

    def test_each_call_is_short_mono_pcm_with_the_declared_duration(self):
        for name, content in self.clips.items():
            with self.subTest(call=name), wave.open(io.BytesIO(content), "rb") as reader:
                self.assertEqual(reader.getnchannels(), 1)
                self.assertEqual(reader.getsampwidth(), 2)
                self.assertEqual(reader.getframerate(), 24_000)
                self.assertEqual(reader.getcomptype(), "NONE")
                self.assertEqual(reader.getnframes(), round(synthese.DURATIONS[name] * 24_000))
                self.assertLess(len(content), 48_044)

    def test_calls_are_non_silent_without_clipping_and_fade_to_silence(self):
        for name, content in self.clips.items():
            with self.subTest(call=name), wave.open(io.BytesIO(content), "rb") as reader:
                frames = reader.readframes(reader.getnframes())
                values = struct.unpack(f"<{len(frames) // 2}h", frames)
                peak = max(abs(value) for value in values) / 32_767
                rms = math.sqrt(sum(value**2 for value in values) / len(values)) / 32_767
                self.assertGreater(peak, 0.59)
                self.assertLess(peak, 0.65)
                self.assertGreater(rms, 0.04)
                self.assertLess(rms, 0.35)
                self.assertEqual(values[0], 0)
                self.assertEqual(values[-1], 0)
                # Pas de saut d'amplitude au début ni à la fin du fichier.
                self.assertLess(max(abs(value) for value in values[:24]), 200)
                self.assertLess(max(abs(value) for value in values[-24:]), 200)
                self.assertLess(abs(sum(values) / len(values)), 25)


if __name__ == "__main__":
    unittest.main()
