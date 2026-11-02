import unittest

import numpy as np

from localvoice.audio import build_board
from localvoice.characters import BUILTINS


class DspTests(unittest.TestCase):
    def test_every_builtin_processes_a_block(self):
        source = np.zeros((1, 2048), dtype=np.float32)
        for character in BUILTINS:
            with self.subTest(character=character.name):
                output = build_board(character)(source, 48_000)
                self.assertEqual(output.shape, source.shape)
                self.assertTrue(np.isfinite(output).all())


if __name__ == "__main__":
    unittest.main()
