from __future__ import annotations

import json
import os
import tkinter as tk
import webbrowser
import zipfile
from dataclasses import replace
from pathlib import Path
from tkinter import filedialog, messagebox, ttk

from sounddevice import PortAudioError

from .audio import AudioDevice, AudioEngine, list_devices
from .characters import Character, CharacterStore, export_voicepack, import_voicepack
from .hotkeys import Hotkeys


class CharacterEditor(tk.Toplevel):
    CONTROLS = (
        ("pitch_semitones", "Pitch (semitones)", -12, 12, 0.5),
        ("gate_db", "Noise gate (dB)", -100, 0, 1),
        ("highpass_hz", "Low cut (Hz)", 20, 1_000, 10),
        ("lowpass_hz", "High cut (Hz)", 1_000, 22_000, 100),
        ("presence_db", "Presence (dB)", -12, 12, 0.5),
        ("distortion_db", "Grit (dB)", 0, 30, 0.5),
        ("bit_depth", "Bit depth", 2, 32, 1),
        ("chorus_mix", "Chorus mix", 0, 1, 0.01),
        ("reverb_mix", "Reverb mix", 0, 1, 0.01),
        ("gain_db", "Output gain (dB)", -24, 24, 0.5),
    )

    def __init__(self, parent: tk.Tk, character: Character, *, title: str):
        super().__init__(parent)
        self.title(title)
        self.geometry("620x650")
        self.resizable(False, False)
        self.transient(parent)
        self.grab_set()
        self.result: Character | None = None
        self.character = character
        self.name_var = tk.StringVar(value=character.name)
        self.description_var = tk.StringVar(value=character.description)
        self.values = {
            key: tk.DoubleVar(value=getattr(character, key))
            for key, *_rest in self.CONTROLS
        }

        frame = ttk.Frame(self, padding=18)
        frame.pack(fill="both", expand=True)
        frame.columnconfigure(1, weight=1)
        ttk.Label(frame, text="Name").grid(row=0, column=0, sticky="w", pady=4)
        ttk.Entry(frame, textvariable=self.name_var).grid(
            row=0, column=1, columnspan=2, sticky="ew", padx=(12, 0)
        )
        ttk.Label(frame, text="Description").grid(row=1, column=0, sticky="w", pady=4)
        ttk.Entry(frame, textvariable=self.description_var).grid(
            row=1, column=1, columnspan=2, sticky="ew", padx=(12, 0)
        )

        for row, (key, label, minimum, maximum, resolution) in enumerate(
            self.CONTROLS, start=2
        ):
            ttk.Label(frame, text=label).grid(row=row, column=0, sticky="w", pady=4)
            scale = tk.Scale(
                frame,
                from_=minimum,
                to=maximum,
                resolution=resolution,
                variable=self.values[key],
                orient="horizontal",
                showvalue=True,
                length=360,
            )
            scale.grid(row=row, column=1, columnspan=2, sticky="ew", padx=(12, 0))

        buttons = ttk.Frame(frame)
        buttons.grid(
            row=len(self.CONTROLS) + 2, column=0, columnspan=3, sticky="e", pady=(16, 0)
        )
        ttk.Button(buttons, text="Cancel", command=self.destroy).pack(
            side="left", padx=6
        )
        ttk.Button(buttons, text="Save character", command=self.save).pack(side="left")
        self.bind("<Escape>", lambda _event: self.destroy())
        self.protocol("WM_DELETE_WINDOW", self.destroy)

    def save(self) -> None:
        try:
            values = {key: variable.get() for key, variable in self.values.items()}
            self.result = replace(
                self.character,
                name=self.name_var.get(),
                description=self.description_var.get().strip(),
                builtin=False,
                **values,
            ).validated()
            self.destroy()
        except ValueError as error:
            messagebox.showerror("Invalid character", str(error), parent=self)


class LocalVoiceApp:
    def __init__(self, root: tk.Tk):
        self.root = root
        self.root.title("LocalVoice")
        self.root.geometry("760x560")
        self.root.minsize(680, 500)
        self.store = CharacterStore()
        self.settings_path = self.store.root / "settings.json"
        self.settings = self._load_settings()
        self.characters = self.store.load()
        self.engine = AudioEngine()
        self.inputs: list[AudioDevice] = []
        self.outputs: list[AudioDevice] = []

        self.input_var = tk.StringVar()
        self.output_var = tk.StringVar()
        self.character_var = tk.StringVar(value=self.characters[0].name)
        self.status_var = tk.StringVar(
            value="Ready. Select an output cable, then go live."
        )
        self.live_var = tk.BooleanVar()
        self.bypass_var = tk.BooleanVar()
        self.mute_var = tk.BooleanVar()

        self._build_ui()
        self.refresh_devices()
        self._restore_selections()
        self.hotkeys = Hotkeys(
            lambda action: self.root.after(0, action),
            {
                "<ctrl>+<shift>+<space>": self.toggle_live,
                "<ctrl>+<shift>+m": self.toggle_mute,
                "<ctrl>+<shift>+b": self.toggle_bypass,
                "<ctrl>+<shift>+<left>": lambda: self.step_character(-1),
                "<ctrl>+<shift>+<right>": lambda: self.step_character(1),
            },
        )
        try:
            self.hotkeys.start()
        except RuntimeError as error:
            self.status_var.set(f"Hotkeys unavailable: {error}")
        self.root.protocol("WM_DELETE_WINDOW", self.close)
        self.root.after(80, self.update_meters)

    def _build_ui(self) -> None:
        style = ttk.Style()
        if "vista" in style.theme_names():
            style.theme_use("vista")
        frame = ttk.Frame(self.root, padding=18)
        frame.pack(fill="both", expand=True)
        frame.columnconfigure(1, weight=1)

        ttk.Label(frame, text="LOCALVOICE", font=("Segoe UI", 20, "bold")).grid(
            row=0, column=0, columnspan=3, sticky="w"
        )
        ttk.Label(frame, text="Local microphone processing for every Windows app").grid(
            row=1, column=0, columnspan=3, sticky="w", pady=(0, 18)
        )

        ttk.Label(frame, text="Microphone").grid(row=2, column=0, sticky="w", pady=5)
        self.input_box = ttk.Combobox(
            frame, textvariable=self.input_var, state="readonly"
        )
        self.input_box.grid(row=2, column=1, sticky="ew", padx=8)
        ttk.Button(frame, text="Refresh", command=self.refresh_devices).grid(
            row=2, column=2
        )

        ttk.Label(frame, text="Output / cable").grid(
            row=3, column=0, sticky="w", pady=5
        )
        self.output_box = ttk.Combobox(
            frame, textvariable=self.output_var, state="readonly"
        )
        self.output_box.grid(row=3, column=1, sticky="ew", padx=8)
        ttk.Button(frame, text="Cable help", command=self.show_cable_help).grid(
            row=3, column=2
        )

        ttk.Separator(frame).grid(row=4, column=0, columnspan=3, sticky="ew", pady=16)

        ttk.Label(frame, text="Character").grid(row=5, column=0, sticky="w")
        self.character_box = ttk.Combobox(
            frame,
            textvariable=self.character_var,
            values=[item.name for item in self.characters],
            state="readonly",
        )
        self.character_box.grid(row=5, column=1, sticky="ew", padx=8)
        self.character_box.bind(
            "<<ComboboxSelected>>", lambda _event: self.select_character()
        )
        ttk.Button(frame, text="New", command=self.new_character).grid(row=5, column=2)

        buttons = ttk.Frame(frame)
        buttons.grid(row=6, column=1, sticky="w", padx=8, pady=8)
        ttk.Button(buttons, text="Edit", command=self.edit_character).pack(
            side="left", padx=(0, 6)
        )
        ttk.Button(buttons, text="Import pack", command=self.import_pack).pack(
            side="left"
        )
        ttk.Button(buttons, text="Export pack", command=self.export_pack).pack(
            side="left", padx=6
        )
        ttk.Button(buttons, text="Delete", command=self.delete_character).pack(
            side="left"
        )

        live = ttk.Button(frame, text="GO LIVE", command=self.toggle_live)
        live.grid(row=7, column=0, columnspan=3, sticky="ew", pady=(14, 10), ipady=10)
        self.live_button = live

        toggles = ttk.Frame(frame)
        toggles.grid(row=8, column=0, columnspan=3)
        ttk.Checkbutton(
            toggles,
            text="Bypass effects",
            variable=self.bypass_var,
            command=self.apply_bypass,
        ).pack(side="left", padx=12)
        ttk.Checkbutton(
            toggles, text="Mute output", variable=self.mute_var, command=self.apply_mute
        ).pack(side="left", padx=12)

        ttk.Label(frame, text="Input").grid(row=9, column=0, sticky="e", pady=(18, 3))
        self.input_meter = ttk.Progressbar(frame, maximum=1)
        self.input_meter.grid(
            row=9, column=1, columnspan=2, sticky="ew", padx=8, pady=(18, 3)
        )
        ttk.Label(frame, text="Output").grid(row=10, column=0, sticky="e", pady=3)
        self.output_meter = ttk.Progressbar(frame, maximum=1)
        self.output_meter.grid(
            row=10, column=1, columnspan=2, sticky="ew", padx=8, pady=3
        )

        ttk.Separator(frame).grid(row=11, column=0, columnspan=3, sticky="ew", pady=16)
        ttk.Label(frame, textvariable=self.status_var, wraplength=680).grid(
            row=12, column=0, columnspan=3, sticky="w"
        )
        ttk.Label(
            frame,
            text="Hotkeys: Ctrl+Shift+Space live · Ctrl+Shift+M mute · Ctrl+Shift+B bypass · Ctrl+Shift+←/→ character",
            foreground="#555",
        ).grid(row=13, column=0, columnspan=3, sticky="w", pady=(12, 0))

    def refresh_devices(self) -> None:
        try:
            self.inputs, self.outputs = list_devices()
            self.input_box["values"] = [item.label for item in self.inputs]
            self.output_box["values"] = [item.label for item in self.outputs]
            if not self.input_var.get() and self.inputs:
                self.input_var.set(self.inputs[0].label)
            if not self.output_var.get() and self.outputs:
                virtual = next(
                    (
                        item
                        for item in self.outputs
                        if any(
                            term in item.name.casefold()
                            for term in ("virtual audio", "cable input", "vb-audio")
                        )
                    ),
                    self.outputs[0],
                )
                self.output_var.set(virtual.label)
        except PortAudioError as error:
            self.status_var.set(f"Could not enumerate audio devices: {error}")

    def _restore_selections(self) -> None:
        for variable, key, devices in (
            (self.input_var, "input", self.inputs),
            (self.output_var, "output", self.outputs),
        ):
            wanted = self.settings.get(key)
            match = next((item.label for item in devices if item.label == wanted), None)
            if match:
                variable.set(match)
        name = self.settings.get("character")
        if any(item.name == name for item in self.characters):
            self.character_var.set(name)

    def current_character(self) -> Character:
        return next(
            item for item in self.characters if item.name == self.character_var.get()
        )

    @staticmethod
    def _device_index(label: str) -> int:
        return int(label.split(":", 1)[0])

    def toggle_live(self) -> None:
        if self.engine.running:
            self.engine.stop()
            self.live_var.set(False)
            self.live_button.configure(text="GO LIVE")
            self.status_var.set("Stopped.")
            return
        try:
            rate = self.engine.start(
                self._device_index(self.input_var.get()),
                self._device_index(self.output_var.get()),
                self.current_character(),
            )
            self.engine.set_bypass(self.bypass_var.get())
            self.engine.set_muted(self.mute_var.get())
            self.live_var.set(True)
            self.live_button.configure(text="STOP")
            self.status_var.set(
                f"LIVE at {rate:,} Hz. Select the cable's recording endpoint as the microphone in your app."
            )
            self._save_settings()
        except (PortAudioError, RuntimeError, ValueError) as error:
            self.status_var.set(f"Could not start audio: {error}")
            messagebox.showerror("LocalVoice", str(error), parent=self.root)

    def toggle_mute(self) -> None:
        self.mute_var.set(not self.mute_var.get())
        self.apply_mute()

    def apply_mute(self) -> None:
        self.engine.set_muted(self.mute_var.get())

    def toggle_bypass(self) -> None:
        self.bypass_var.set(not self.bypass_var.get())
        self.apply_bypass()

    def apply_bypass(self) -> None:
        self.engine.set_bypass(self.bypass_var.get())

    def select_character(self) -> None:
        self.engine.set_character(self.current_character())
        self.status_var.set(f"Character: {self.character_var.get()}")
        self._save_settings()

    def step_character(self, amount: int) -> None:
        index = next(
            (
                i
                for i, item in enumerate(self.characters)
                if item.name == self.character_var.get()
            ),
            0,
        )
        self.character_var.set(
            self.characters[(index + amount) % len(self.characters)].name
        )
        self.select_character()

    def new_character(self) -> None:
        base = self.current_character()
        editor = CharacterEditor(
            self.root,
            replace(
                base, id=os.urandom(16).hex(), name=f"{base.name} Custom", builtin=False
            ),
            title="Create character",
        )
        self.root.wait_window(editor)
        if not editor.result:
            return
        if self._name_taken(editor.result.name):
            messagebox.showerror(
                "Duplicate character",
                "A character with that name already exists.",
                parent=self.root,
            )
            return
        self.characters.append(editor.result)
        self._save_characters()
        self._refresh_characters(editor.result.name)

    def edit_character(self) -> None:
        original = self.current_character()
        draft = replace(
            original,
            id=os.urandom(16).hex() if original.builtin else original.id,
            name=f"{original.name} Custom" if original.builtin else original.name,
            builtin=False,
        )
        editor = CharacterEditor(self.root, draft, title="Edit character")
        self.root.wait_window(editor)
        if not editor.result:
            return
        if self._name_taken(
            editor.result.name, excluding_id=None if original.builtin else original.id
        ):
            messagebox.showerror(
                "Duplicate character",
                "A character with that name already exists.",
                parent=self.root,
            )
            return
        if not original.builtin:
            self.characters.remove(original)
        self.characters.append(editor.result)
        self._save_characters()
        self._refresh_characters(editor.result.name)

    def import_pack(self) -> None:
        filename = filedialog.askopenfilename(
            filetypes=[("LocalVoice pack", "*.voicepack"), ("ZIP file", "*.zip")],
            parent=self.root,
        )
        if not filename:
            return
        try:
            character = import_voicepack(Path(filename))
            character = replace(character, name=self._unique_name(character.name))
            self.characters.append(character)
            self._save_characters()
            self._refresh_characters(character.name)
            self.status_var.set(f"Imported {character.name}.")
        except (OSError, ValueError, zipfile.BadZipFile) as error:
            messagebox.showerror("Import failed", str(error), parent=self.root)

    def export_pack(self) -> None:
        filename = filedialog.asksaveasfilename(
            defaultextension=".voicepack",
            initialfile=f"{self.current_character().name}.voicepack",
            parent=self.root,
        )
        if filename:
            export_voicepack(self.current_character(), Path(filename))
            self.status_var.set(f"Exported {Path(filename).name}.")

    def delete_character(self) -> None:
        character = self.current_character()
        if character.builtin:
            messagebox.showinfo(
                "LocalVoice", "Built-in characters cannot be deleted.", parent=self.root
            )
            return
        if messagebox.askyesno(
            "Delete character", f"Delete {character.name}?", parent=self.root
        ):
            self.characters.remove(character)
            self._save_characters()
            self._refresh_characters(self.characters[0].name)

    def _refresh_characters(self, selected: str) -> None:
        self.character_box["values"] = [item.name for item in self.characters]
        self.character_var.set(selected)
        self.select_character()

    def _name_taken(self, name: str, excluding_id: str | None = None) -> bool:
        return any(
            item.id != excluding_id and item.name.casefold() == name.casefold()
            for item in self.characters
        )

    def _unique_name(self, name: str) -> str:
        if not self._name_taken(name):
            return name
        number = 2
        while self._name_taken(f"{name} ({number})"):
            number += 1
        return f"{name} ({number})"

    def _save_characters(self) -> None:
        self.store.save_custom(self.characters)

    def _load_settings(self) -> dict:
        try:
            return json.loads(self.settings_path.read_text(encoding="utf-8"))
        except (OSError, ValueError, json.JSONDecodeError):
            return {}

    def _save_settings(self) -> None:
        settings = {
            "input": self.input_var.get(),
            "output": self.output_var.get(),
            "character": self.character_var.get(),
        }
        temporary = self.settings_path.with_suffix(".tmp")
        temporary.write_text(json.dumps(settings, indent=2), encoding="utf-8")
        temporary.replace(self.settings_path)

    def show_cable_help(self) -> None:
        open_page = messagebox.askyesno(
            "Virtual microphone setup",
            "LocalVoice sends processed audio to the output you select. Install a virtual audio cable, select its playback/input side here, then select its recording/output side as the microphone in Discord, OBS, a game, or a browser.\n\n"
            "If no cable appears in the device list, install one and press Refresh.\n\nOpen the signed, open-source Virtual Audio Driver download page now?",
            parent=self.root,
        )
        if open_page:
            webbrowser.open(
                "https://github.com/VirtualDrivers/Virtual-Audio-Driver/releases/latest"
            )

    def update_meters(self) -> None:
        self.input_meter["value"] = min(1, self.engine.input_level * 2)
        self.output_meter["value"] = min(1, self.engine.output_level * 2)
        if self.engine.error:
            self.status_var.set(f"Audio error (output muted): {self.engine.error}")
        elif self.engine.running and self.engine.xruns:
            self.status_var.set(
                f"LIVE at {self.engine.sample_rate:,} Hz · {self.engine.xruns} audio warning(s)"
            )
        self.root.after(80, self.update_meters)

    def close(self) -> None:
        self.engine.stop()
        self.hotkeys.stop()
        self.root.destroy()


def main() -> None:
    root = tk.Tk()
    LocalVoiceApp(root)
    root.mainloop()


if __name__ == "__main__":
    main()
