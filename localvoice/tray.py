from __future__ import annotations

from collections.abc import Callable

import pystray
from PIL import Image, ImageDraw


def _icon_image() -> Image.Image:
    image = Image.new("RGBA", (64, 64), (20, 22, 28, 255))
    draw = ImageDraw.Draw(image)
    draw.ellipse((13, 7, 51, 45), fill=(90, 210, 170, 255))
    draw.rounded_rectangle((26, 13, 38, 38), radius=6, fill=(20, 22, 28, 255))
    draw.arc((19, 20, 45, 48), 0, 180, fill=(20, 22, 28, 255), width=4)
    draw.line((32, 44, 32, 55), fill=(90, 210, 170, 255), width=4)
    return image


class Tray:
    def __init__(
        self,
        schedule: Callable[[Callable[[], None]], None],
        *,
        show: Callable[[], None],
        toggle_live: Callable[[], None],
        toggle_mute: Callable[[], None],
        quit_app: Callable[[], None],
    ):
        def item(label: str, action: Callable[[], None], *, default: bool = False):
            return pystray.MenuItem(
                label, lambda _icon, _item: schedule(action), default=default
            )

        self.icon = pystray.Icon(
            "LocalVoice",
            _icon_image(),
            "LocalVoice",
            pystray.Menu(
                item("Open LocalVoice", show, default=True),
                item("Go live / stop", toggle_live),
                item("Mute / unmute", toggle_mute),
                pystray.Menu.SEPARATOR,
                item("Quit", quit_app),
            ),
        )

    def start(self) -> None:
        self.icon.run_detached()

    def notify_hidden(self) -> None:
        self.icon.notify(
            "LocalVoice is still running. Use the tray icon or your hotkeys.",
            "LocalVoice",
        )

    def stop(self) -> None:
        self.icon.stop()
