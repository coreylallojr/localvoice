import unittest

from pynput import keyboard

from localvoice.hotkeys import event_matches, normalize_ptt_key


class HotkeyTests(unittest.TestCase):
    def test_normalizes_supported_keys(self):
        self.assertEqual(normalize_ptt_key(" F8 "), "f8")
        self.assertEqual(normalize_ptt_key("Caps Lock"), "caps_lock")
        self.assertEqual(normalize_ptt_key("v"), "v")

    def test_rejects_unsupported_chord(self):
        with self.assertRaisesRegex(ValueError, "Use one"):
            normalize_ptt_key("ctrl+v")

    def test_matches_key_events(self):
        self.assertTrue(event_matches(keyboard.Key.f8, "f8"))
        self.assertTrue(event_matches(keyboard.KeyCode.from_char("V"), "v"))
        self.assertFalse(event_matches(keyboard.Key.f7, "f8"))


if __name__ == "__main__":
    unittest.main()
