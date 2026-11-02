from __future__ import annotations

from collections.abc import Callable

from pynput import keyboard


class Hotkeys:
    def __init__(
        self,
        schedule: Callable[[Callable[[], None]], None],
        actions: dict[str, Callable[[], None]],
    ):
        self.listener = keyboard.GlobalHotKeys(
            {
                key: lambda action=action: schedule(action)
                for key, action in actions.items()
            }
        )

    def start(self) -> None:
        self.listener.start()

    def stop(self) -> None:
        self.listener.stop()
