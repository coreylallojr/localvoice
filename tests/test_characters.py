import tempfile
import unittest
import zipfile
from pathlib import Path

from localvoice.characters import (
    Character,
    CharacterStore,
    export_voicepack,
    import_voicepack,
)


class CharacterTests(unittest.TestCase):
    def test_store_and_voicepack_round_trip(self):
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            character = Character("custom", "Morgan", pitch_semitones=-3).validated()
            store = CharacterStore(root)
            store.save_custom([character])
            self.assertEqual(store.load()[-1].name, "Morgan")

            pack = root / "morgan.voicepack"
            export_voicepack(character, pack)
            imported = import_voicepack(pack)
            self.assertEqual(imported.name, "Morgan")
            self.assertNotEqual(imported.id, character.id)

    def test_rejects_unsafe_archive_paths(self):
        with tempfile.TemporaryDirectory() as directory:
            pack = Path(directory) / "bad.voicepack"
            with zipfile.ZipFile(pack, "w") as archive:
                archive.writestr("../manifest.json", "{}")
            with self.assertRaisesRegex(ValueError, "unsafe path"):
                import_voicepack(pack)

    def test_validates_ranges(self):
        with self.assertRaisesRegex(ValueError, "pitch_semitones"):
            Character("bad", "Bad", pitch_semitones=30).validated()


if __name__ == "__main__":
    unittest.main()
