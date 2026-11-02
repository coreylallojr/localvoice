import unittest
from types import SimpleNamespace

import numpy as np

from localvoice.audio import AudioEngine, build_board
from localvoice.characters import BUILTINS


class DspTests(unittest.TestCase):
    def test_every_builtin_processes_a_block(self):
        source = np.zeros((1, 2048), dtype=np.float32)
        for character in BUILTINS:
            with self.subTest(character=character.name):
                output = build_board(character)(source, 48_000)
                self.assertEqual(output.shape, source.shape)
                self.assertTrue(np.isfinite(output).all())

    def test_monitor_hears_processed_audio_while_transmission_is_muted(self):
        engine = AudioEngine()
        engine.bypass = True
        engine.muted = True
        engine.monitor_stream = SimpleNamespace(active=True)
        source = np.full((8, 1), 0.25, dtype=np.float32)
        transmitted = np.empty_like(source)

        engine._callback(source, transmitted, 8, None, False)

        np.testing.assert_array_equal(transmitted, 0)
        np.testing.assert_array_equal(engine.monitor_queue.get_nowait(), source[:, 0])

    def test_every_builtin_fills_a_realtime_callback_block(self):
        source = np.random.default_rng(1).normal(0, 0.05, (512, 1)).astype(np.float32)
        for character in BUILTINS:
            with self.subTest(character=character.name):
                engine = AudioEngine()
                engine.sample_rate = 48_000
                engine.board = build_board(character)
                output = np.empty_like(source)

                engine._callback(source, output, 512, None, False)

                self.assertEqual(engine.error, "")
                self.assertTrue(np.isfinite(output).all())


if __name__ == "__main__":
    unittest.main()
