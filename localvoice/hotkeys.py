from __future__ import annotations

from collections.abc import Callable

from pynput import keyboard

KEY_NAMES = {
    "space": keyboard.Key.space,
    "caps_lock": keyboard.Key.caps_lock,
    "scroll_lock": keyboard.Key.scroll_lock,
    "insert": keyboard.Key.insert,
    "pause": keyboard.Key.pause,
    **{f"f{number}": getattr(keyboard.Key, f"f{number}") for number in range(1, 21)},
}


def normalize_ptt_key(value: str) -> str:
    name = value.strip().lower().replace(" ", "_")
    if len(name) == 1 and name.isalnum():
        return name
    if name in KEY_NAMES:
        return name
    raise ValueError(
        "Use one letter, number, F1-F20, Space, Caps Lock, Scroll Lock, Insert, or Pause."
    )


def event_matches(event: keyboard.Key | keyboard.KeyCode, name: str) -> bool:
    if len(name) == 1:
        return (
            isinstance(event, keyboard.KeyCode)
            and event.char is not None
            and event.char.lower() == name
        )
    return event == KEY_NAMES[name]


class Hotkeys:
    def __init__(
        self,
        schedule: Callable[[Callable[[], None]], None],
        actions: dict[str, Callable[[], None]],
        *,
        ptt_key: str,
        ptt_press: Callable[[], None],
        ptt_release: Callable[[], None],
    ):
        self.schedule = schedule
        self.ptt_key = normalize_ptt_key(ptt_key)
        self.ptt_press = ptt_press
        self.ptt_release = ptt_release
        self.ptt_down = False
        self.shortcuts = keyboard.GlobalHotKeys(
            {
                key: lambda action=action: schedule(action)
                for key, action in actions.items()
            }
        )
        self.ptt_listener = keyboard.Listener(
            on_press=self._on_press, on_release=self._on_release
        )

    def _on_press(self, event: keyboard.Key | keyboard.KeyCode) -> None:
        if not self.ptt_down and event_matches(event, self.ptt_key):
            self.ptt_down = True
            self.schedule(self.ptt_press)

    def _on_release(self, event: keyboard.Key | keyboard.KeyCode) -> None:
        if self.ptt_down and event_matches(event, self.ptt_key):
            self.ptt_down = False
            self.schedule(self.ptt_release)

    def start(self) -> None:
        self.shortcuts.start()
        self.ptt_listener.start()

    def stop(self) -> None:
        self.shortcuts.stop()
        self.ptt_listener.stop()
