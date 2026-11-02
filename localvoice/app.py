from __future__ import annotations

import ctypes
import json
import os
import tkinter as tk
import webbrowser
import zipfile
from dataclasses import replace
from pathlib import Path
from tkinter import filedialog, messagebox, ttk

import sounddevice as sd
from sounddevice import PortAudioError

from .audio import AudioDevice, AudioEngine, list_devices
from .characters import Character, CharacterStore, export_voicepack, import_voicepack
from .hotkeys import Hotkeys, normalize_ptt_key
from .tray import Tray

DEFAULT_SETTINGS = {
    "settings_version": 2,
    "ptt_enabled": False,
    "ptt_key": "f8",
    "ptt_mode": "hold",
    "close_to_tray": True,
    "auto_live": False,
    "monitor_enabled": False,
    "onboarding_complete": False,
    "monitor_warning_seen": False,
}


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


class SettingsDialog(tk.Toplevel):
    def __init__(self, parent: tk.Tk, settings: dict):
        super().__init__(parent)
        self.title("LocalVoice settings")
        self.geometry("480x390")
        self.resizable(False, False)
        self.transient(parent)
        self.grab_set()
        self.result: dict | None = None
        self.ptt_enabled = tk.BooleanVar(value=settings["ptt_enabled"])
        self.ptt_key = tk.StringVar(value=settings["ptt_key"])
        self.ptt_mode = tk.StringVar(value=settings["ptt_mode"])
        self.close_to_tray = tk.BooleanVar(value=settings["close_to_tray"])
        self.auto_live = tk.BooleanVar(value=settings["auto_live"])

        frame = ttk.Frame(self, padding=20)
        frame.pack(fill="both", expand=True)
        frame.columnconfigure(1, weight=1)
        ttk.Label(frame, text="PUSH TO TALK", font=("Segoe UI", 12, "bold")).grid(
            row=0, column=0, columnspan=2, sticky="w", pady=(0, 12)
        )
        ttk.Checkbutton(
            frame, text="Enable push to talk", variable=self.ptt_enabled
        ).grid(row=1, column=0, columnspan=2, sticky="w", pady=5)
        ttk.Label(frame, text="Key").grid(row=2, column=0, sticky="w", pady=5)
        ttk.Entry(frame, textvariable=self.ptt_key, width=18).grid(
            row=2, column=1, sticky="w", padx=12
        )
        ttk.Label(
            frame, text="Examples: F8, Caps Lock, Space, V", foreground="#555"
        ).grid(row=3, column=1, sticky="w", padx=12)
        ttk.Label(frame, text="Behavior").grid(row=4, column=0, sticky="w", pady=10)
        ttk.Combobox(
            frame,
            textvariable=self.ptt_mode,
            values=("hold", "toggle"),
            state="readonly",
            width=15,
        ).grid(row=4, column=1, sticky="w", padx=12)
        ttk.Label(
            frame,
            text="Hold: live only while pressed. Toggle: each press mutes/unmutes.",
            foreground="#555",
            wraplength=330,
        ).grid(row=5, column=1, sticky="w", padx=12)

        ttk.Separator(frame).grid(row=6, column=0, columnspan=2, sticky="ew", pady=18)
        ttk.Checkbutton(
            frame,
            text="Keep running in the system tray when closed",
            variable=self.close_to_tray,
        ).grid(row=7, column=0, columnspan=2, sticky="w", pady=5)
        ttk.Checkbutton(
            frame,
            text="Go live automatically when LocalVoice starts",
            variable=self.auto_live,
        ).grid(row=8, column=0, columnspan=2, sticky="w", pady=5)

        buttons = ttk.Frame(frame)
        buttons.grid(row=9, column=0, columnspan=2, sticky="e", pady=(24, 0))
        ttk.Button(buttons, text="Cancel", command=self.destroy).pack(
            side="left", padx=6
        )
        ttk.Button(buttons, text="Save", command=self.save).pack(side="left")
        self.bind("<Escape>", lambda _event: self.destroy())

    def save(self) -> None:
        try:
            key = normalize_ptt_key(self.ptt_key.get())
        except ValueError as error:
            messagebox.showerror("Invalid push-to-talk key", str(error), parent=self)
            return
        self.result = {
            "ptt_enabled": self.ptt_enabled.get(),
            "ptt_key": key,
            "ptt_mode": self.ptt_mode.get(),
            "close_to_tray": self.close_to_tray.get(),
            "auto_live": self.auto_live.get(),
        }
        self.destroy()


class LocalVoiceApp:
    def __init__(self, root: tk.Tk):
        self.root = root
        self.root.title("LocalVoice")
        self.root.geometry("820x700")
        self.root.minsize(720, 620)
        self.store = CharacterStore()
        self.settings_path = self.store.root / "settings.json"
        loaded_settings = self._load_settings()
        if loaded_settings.get("settings_version", 0) < 2:
            loaded_settings.update(
                settings_version=2, ptt_enabled=False, monitor_enabled=False
            )
        self.settings = DEFAULT_SETTINGS | loaded_settings
        self.characters = self.store.load()
        self.engine = AudioEngine()
        self.inputs: list[AudioDevice] = []
        self.outputs: list[AudioDevice] = []
        self.input_choices: list[AudioDevice] = []
        self.virtual_outputs: list[AudioDevice] = []
        self.monitor_outputs: list[AudioDevice] = []
        self.tray: Tray | None = None
        self.hotkeys: Hotkeys | None = None
        self.quitting = False
        self.auditioning = False

        self.input_var = tk.StringVar()
        self.output_var = tk.StringVar()
        self.monitor_var = tk.StringVar()
        self.character_var = tk.StringVar(value=self.characters[0].name)
        self.status_var = tk.StringVar(
            value="Ready. Choose your microphone, virtual cable, and headphones."
        )
        self.live_var = tk.BooleanVar()
        self.bypass_var = tk.BooleanVar()
        self.mute_var = tk.BooleanVar()
        self.ptt_enabled_var = tk.BooleanVar(value=self.settings["ptt_enabled"])
        self.monitor_enabled_var = tk.BooleanVar(value=self.settings["monitor_enabled"])
        self.ptt_status_var = tk.StringVar()
        self.setup_status_var = tk.StringVar()

        self._build_ui()
        self.refresh_devices()
        self._restore_selections()
        self._save_settings()
        self.start_hotkeys()
        self.tray = Tray(
            lambda action: self.root.after(0, action),
            show=self.show_window,
            toggle_live=self.toggle_live,
            toggle_mute=self.toggle_mute,
            quit_app=self.quit_app,
        )
        try:
            self.tray.start()
        except RuntimeError as error:
            self.tray = None
            self.status_var.set(f"Tray unavailable: {error}")
        self.root.protocol("WM_DELETE_WINDOW", self.window_close)
        self.root.after(80, self.update_meters)
        self.root.after(300, self.first_run_help)
        if self.settings["auto_live"]:
            self.root.after(700, self.start_live)

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
        ttk.Label(
            frame, text="Your voice, processed locally, in every Windows app"
        ).grid(row=1, column=0, columnspan=3, sticky="w", pady=(0, 18))

        ttk.Label(frame, text="1. Mic you speak into").grid(
            row=2, column=0, sticky="w", pady=5
        )
        self.input_box = ttk.Combobox(
            frame, textvariable=self.input_var, state="readonly"
        )
        self.input_box.grid(row=2, column=1, sticky="ew", padx=8)
        ttk.Button(frame, text="Refresh", command=self.refresh_devices).grid(
            row=2, column=2
        )

        ttk.Label(frame, text="2. Send changed voice to").grid(
            row=3, column=0, sticky="w", pady=5
        )
        output_panel = ttk.Frame(frame)
        output_panel.grid(row=3, column=1, sticky="ew", padx=8)
        self.output_box = ttk.Combobox(
            output_panel, textvariable=self.output_var, state="readonly"
        )
        self.output_box.pack(fill="x")
        ttk.Label(
            output_panel,
            textvariable=self.setup_status_var,
            foreground="#9c2f00",
            wraplength=470,
        ).pack(fill="x", pady=(3, 0))
        ttk.Button(frame, text="Set up", command=self.show_cable_help).grid(
            row=3, column=2
        )

        ttk.Label(frame, text="3. Hear yourself through").grid(
            row=4, column=0, sticky="w", pady=5
        )
        self.monitor_box = ttk.Combobox(
            frame, textvariable=self.monitor_var, state="readonly"
        )
        self.monitor_box.grid(row=4, column=1, sticky="ew", padx=8)
        self.monitor_button = ttk.Button(
            frame, text="TEST IN HEADPHONES", command=self.test_my_voice
        )
        self.monitor_button.grid(row=4, column=2)

        ttk.Separator(frame).grid(row=5, column=0, columnspan=3, sticky="ew", pady=16)

        ttk.Label(frame, text="Character").grid(row=6, column=0, sticky="w")
        self.character_box = ttk.Combobox(
            frame,
            textvariable=self.character_var,
            values=[item.name for item in self.characters],
            state="readonly",
        )
        self.character_box.grid(row=6, column=1, sticky="ew", padx=8)
        self.character_box.bind(
            "<<ComboboxSelected>>", lambda _event: self.select_character()
        )
        ttk.Button(frame, text="New", command=self.new_character).grid(row=6, column=2)

        buttons = ttk.Frame(frame)
        buttons.grid(row=7, column=1, sticky="w", padx=8, pady=8)
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
        live.grid(row=8, column=0, columnspan=3, sticky="ew", pady=(14, 10), ipady=10)
        self.live_button = live

        toggles = ttk.Frame(frame)
        toggles.grid(row=9, column=0, columnspan=3)
        ttk.Checkbutton(
            toggles,
            text="Push to talk",
            variable=self.ptt_enabled_var,
            command=self.toggle_ptt_enabled,
        ).pack(side="left", padx=12)
        ttk.Checkbutton(
            toggles,
            text="Bypass effects",
            variable=self.bypass_var,
            command=self.apply_bypass,
        ).pack(side="left", padx=12)
        ttk.Checkbutton(
            toggles, text="Mute output", variable=self.mute_var, command=self.apply_mute
        ).pack(side="left", padx=12)
        ttk.Button(
            toggles, text="Hotkey settings", command=self.configure_settings
        ).pack(side="left", padx=12)

        ttk.Label(frame, text="Input").grid(row=10, column=0, sticky="e", pady=(18, 3))
        self.input_meter = ttk.Progressbar(frame, maximum=1)
        self.input_meter.grid(
            row=10, column=1, columnspan=2, sticky="ew", padx=8, pady=(18, 3)
        )
        ttk.Label(frame, text="Changed voice").grid(
            row=11, column=0, sticky="e", pady=3
        )
        self.output_meter = ttk.Progressbar(frame, maximum=1)
        self.output_meter.grid(
            row=11, column=1, columnspan=2, sticky="ew", padx=8, pady=3
        )

        ttk.Separator(frame).grid(row=12, column=0, columnspan=3, sticky="ew", pady=16)
        ttk.Label(frame, textvariable=self.status_var, wraplength=680).grid(
            row=13, column=0, columnspan=3, sticky="w"
        )
        ttk.Label(frame, textvariable=self.ptt_status_var, foreground="#087f5b").grid(
            row=14, column=0, columnspan=3, sticky="w", pady=(6, 0)
        )
        ttk.Label(
            frame,
            text="Other hotkeys: Ctrl+Shift+Space live | Ctrl+Shift+M mute | Ctrl+Shift+B bypass | Ctrl+Shift+Left/Right character",
            foreground="#555",
        ).grid(row=15, column=0, columnspan=3, sticky="w", pady=(8, 0))

        management = ttk.Frame(frame)
        management.grid(row=16, column=0, columnspan=3, sticky="e", pady=(14, 0))
        ttk.Button(management, text="Hide to tray", command=self.hide_window).pack(
            side="left", padx=5
        )
        ttk.Button(management, text="Settings", command=self.configure_settings).pack(
            side="left", padx=5
        )
        ttk.Button(management, text="Quit", command=self.quit_app).pack(
            side="left", padx=5
        )
        self.update_ptt_status()

    def refresh_devices(self) -> None:
        try:
            self.inputs, self.outputs = list_devices()
            self.input_choices = self._prefer_wasapi(self.inputs)
            self.virtual_outputs = self._prefer_wasapi(
                [item for item in self.outputs if self._is_virtual(item)]
            )
            self.monitor_outputs = self._prefer_wasapi(
                [item for item in self.outputs if not self._is_virtual(item)]
            )
            self.input_box["values"] = [item.label for item in self.input_choices]
            self.output_box["values"] = [item.label for item in self.virtual_outputs]
            self.monitor_box["values"] = [item.label for item in self.monitor_outputs]
            default_input, default_output = sd.default.device
            if (
                self.input_var.get() not in self.input_box["values"]
                and self.input_choices
            ):
                selected = self._matching_default(
                    self.input_choices, self.inputs, default_input
                )
                self.input_var.set(selected.label)
            if self.virtual_outputs:
                if self.output_var.get() not in self.output_box["values"]:
                    self.output_var.set(self.virtual_outputs[0].label)
                self.setup_status_var.set(
                    "Virtual microphone ready. Select its recording side in your app."
                )
                self.live_button.configure(text="GO LIVE")
            else:
                self.output_var.set("Virtual microphone not installed")
                self.setup_status_var.set(
                    "Required for Discord, games, OBS, and calls. Click Set up."
                )
                self.live_button.configure(text="SET UP VIRTUAL MIC TO GO LIVE")
            if (
                self.monitor_var.get() not in self.monitor_box["values"]
                and self.monitor_outputs
            ):
                monitor = self._matching_default(
                    self.monitor_outputs, self.outputs, default_output
                )
                self.monitor_var.set(monitor.label)
        except PortAudioError as error:
            self.status_var.set(f"Could not enumerate audio devices: {error}")

    @staticmethod
    def _prefer_wasapi(devices: list[AudioDevice]) -> list[AudioDevice]:
        wasapi = [item for item in devices if "WASAPI" in item.host_api.upper()]
        return wasapi or devices

    @staticmethod
    def _matching_default(
        choices: list[AudioDevice], all_devices: list[AudioDevice], default_index: int
    ) -> AudioDevice:
        default = next(
            (item for item in all_devices if item.index == default_index), None
        )
        if default:
            match = next(
                (
                    item
                    for item in choices
                    if item.name.casefold() == default.name.casefold()
                ),
                None,
            )
            if match:
                return match
        return choices[0]

    @staticmethod
    def _is_virtual(device: AudioDevice) -> bool:
        return any(
            term in device.name.casefold()
            for term in (
                "virtual audio",
                "virtual mic",
                "cable input",
                "vb-audio",
                "voicemeeter input",
            )
        )

    def _restore_selections(self) -> None:
        for variable, key, devices in (
            (self.input_var, "input", self.input_choices),
            (self.output_var, "output", self.virtual_outputs),
            (self.monitor_var, "monitor", self.monitor_outputs),
        ):
            wanted = self.settings.get(key)
            match = next((item.label for item in devices if item.label == wanted), None)
            if match:
                variable.set(match)
        name = self.settings.get("character")
        if any(item.name == name for item in self.characters):
            self.character_var.set(name)
        self.update_ptt_status()

    def current_character(self) -> Character:
        return next(
            item for item in self.characters if item.name == self.character_var.get()
        )

    @staticmethod
    def _device_index(label: str, devices: list[AudioDevice], description: str) -> int:
        selected = next((item for item in devices if item.label == label), None)
        if not selected:
            raise ValueError(f"Choose {description} first.")
        return selected.index

    def start_hotkeys(self) -> None:
        if self.hotkeys:
            self.hotkeys.stop()
        self.hotkeys = Hotkeys(
            lambda action: self.root.after(0, action),
            {
                "<ctrl>+<shift>+<space>": self.toggle_live,
                "<ctrl>+<shift>+m": self.toggle_mute,
                "<ctrl>+<shift>+b": self.toggle_bypass,
                "<ctrl>+<shift>+<left>": lambda: self.step_character(-1),
                "<ctrl>+<shift>+<right>": lambda: self.step_character(1),
            },
            ptt_key=self.settings["ptt_key"],
            ptt_press=self.ptt_press,
            ptt_release=self.ptt_release,
        )
        try:
            self.hotkeys.start()
        except RuntimeError as error:
            self.status_var.set(f"Hotkeys unavailable: {error}")

    def update_ptt_status(self) -> None:
        if self.ptt_enabled_var.get():
            mode = "hold" if self.settings["ptt_mode"] == "hold" else "press to toggle"
            self.ptt_status_var.set(
                f"Push to talk: {self.settings['ptt_key'].upper()} ({mode})"
            )
        else:
            self.ptt_status_var.set("Push to talk: off")

    def toggle_ptt_enabled(self) -> None:
        self.settings["ptt_enabled"] = self.ptt_enabled_var.get()
        if self.ptt_enabled_var.get():
            self.mute_var.set(True)
        else:
            self.mute_var.set(False)
        self.apply_mute()
        self.update_ptt_status()
        self._save_settings()

    def ptt_press(self) -> None:
        if self.auditioning or not self.ptt_enabled_var.get():
            return
        if not self.engine.running and not self.start_live():
            return
        if self.settings["ptt_mode"] == "toggle":
            self.toggle_mute()
        else:
            self.mute_var.set(False)
            self.apply_mute()
            self.status_var.set("TRANSMITTING - release the push-to-talk key to mute.")

    def ptt_release(self) -> None:
        if (
            not self.auditioning
            and self.ptt_enabled_var.get()
            and self.settings["ptt_mode"] == "hold"
        ):
            self.mute_var.set(True)
            self.apply_mute()
            self.status_var.set("Push to talk ready - output muted.")

    def toggle_live(self) -> None:
        if self.auditioning:
            self.stop_preview()
        if self.engine.running:
            self.stop_live()
        else:
            self.start_live()

    def start_live(self) -> bool:
        if not self.virtual_outputs:
            self.status_var.set(
                "A virtual microphone is required to send audio to other apps. "
                "Headphone testing works without it."
            )
            self.show_cable_help()
            return False
        try:
            output_index = self._device_index(
                self.output_var.get(), self.virtual_outputs, "a virtual microphone"
            )
            monitor_index = (
                self._device_index(
                    self.monitor_var.get(), self.monitor_outputs, "your headphones"
                )
                if self.monitor_var.get()
                else None
            )
            use_monitor = (
                self.monitor_enabled_var.get() and monitor_index != output_index
            )
            if self.ptt_enabled_var.get():
                self.mute_var.set(True)
            rate = self.engine.start(
                self._device_index(
                    self.input_var.get(), self.input_choices, "the microphone you use"
                ),
                output_index,
                self.current_character(),
                monitor_index if use_monitor else None,
                muted=self.mute_var.get(),
                bypass=self.bypass_var.get(),
            )
            self.auditioning = False
            self.live_var.set(True)
            self.live_button.configure(text="STOP")
            self.monitor_button.configure(
                text="STOP HEADPHONE MONITOR"
                if self.monitor_enabled_var.get()
                else "TEST IN HEADPHONES"
            )
            self.status_var.set(
                f"LIVE at {rate:,} Hz. "
                + (
                    "Hold your push-to-talk key to transmit."
                    if self.ptt_enabled_var.get()
                    else "Your changed voice is transmitting."
                )
            )
            self._save_settings()
            return True
        except (PortAudioError, RuntimeError, ValueError) as error:
            self.status_var.set(f"Could not start audio: {error}")
            messagebox.showerror("LocalVoice", str(error), parent=self.root)
            return False

    def stop_live(self) -> None:
        self.engine.stop()
        self.auditioning = False
        self.live_var.set(False)
        self.monitor_enabled_var.set(False)
        self.settings["monitor_enabled"] = False
        self.live_button.configure(
            text="GO LIVE" if self.virtual_outputs else "SET UP VIRTUAL MIC TO GO LIVE"
        )
        self.monitor_button.configure(text="TEST IN HEADPHONES")
        self.status_var.set("Stopped.")
        self._save_settings()

    def toggle_mute(self) -> None:
        self.mute_var.set(not self.mute_var.get())
        self.apply_mute()

    def apply_mute(self) -> None:
        if not self.auditioning:
            self.engine.set_muted(self.mute_var.get())

    def toggle_bypass(self) -> None:
        self.bypass_var.set(not self.bypass_var.get())
        self.apply_bypass()

    def apply_bypass(self) -> None:
        self.engine.set_bypass(self.bypass_var.get())

    def test_my_voice(self) -> None:
        if self.auditioning:
            self.stop_preview()
            return
        if not self.settings["monitor_warning_seen"]:
            if not messagebox.askokcancel(
                "Use headphones",
                "Wear headphones before monitoring your voice. Speakers can create loud feedback. Continue?",
                parent=self.root,
            ):
                return
            self.settings["monitor_warning_seen"] = True
        if not self.engine.running:
            self.start_preview()
            return
        enabling = not self.monitor_enabled_var.get()
        try:
            if enabling:
                self.engine.enable_monitor(
                    self._device_index(
                        self.monitor_var.get(),
                        self.monitor_outputs,
                        "your headphones",
                    )
                )
            else:
                self.engine.disable_monitor()
            self.monitor_enabled_var.set(enabling)
            self.settings["monitor_enabled"] = enabling
            self.status_var.set(
                "You can hear the changed voice; transmission mute is unchanged."
                if enabling
                else "Headphone monitoring is off."
            )
            self.monitor_button.configure(
                text="STOP HEADPHONE MONITOR" if enabling else "TEST IN HEADPHONES"
            )
        except (PortAudioError, RuntimeError, ValueError) as error:
            self.monitor_enabled_var.set(False)
            self.settings["monitor_enabled"] = False
            messagebox.showerror("Headphone test failed", str(error), parent=self.root)
        self._save_settings()

    def start_preview(self) -> None:
        try:
            input_index = self._device_index(
                self.input_var.get(), self.input_choices, "the microphone you use"
            )
            monitor_index = self._device_index(
                self.monitor_var.get(), self.monitor_outputs, "your headphones"
            )
            if self.virtual_outputs:
                output_index = self._device_index(
                    self.output_var.get(), self.virtual_outputs, "a virtual microphone"
                )
                rate = self.engine.start(
                    input_index,
                    output_index,
                    self.current_character(),
                    monitor_index,
                    muted=True,
                    bypass=self.bypass_var.get(),
                )
            else:
                rate = self.engine.start(
                    input_index,
                    monitor_index,
                    self.current_character(),
                    muted=False,
                    bypass=self.bypass_var.get(),
                )
            self.auditioning = True
            self.monitor_enabled_var.set(True)
            self.monitor_button.configure(text="STOP TEST")
            self.status_var.set(
                f"HEADPHONE TEST at {rate:,} Hz — speak normally. "
                "Nothing is being sent to other apps."
            )
        except (PortAudioError, RuntimeError, ValueError) as error:
            self.engine.stop()
            self.auditioning = False
            self.monitor_enabled_var.set(False)
            messagebox.showerror("Headphone test failed", str(error), parent=self.root)

    def stop_preview(self) -> None:
        self.engine.stop()
        self.auditioning = False
        self.monitor_enabled_var.set(False)
        self.settings["monitor_enabled"] = False
        self.monitor_button.configure(text="TEST IN HEADPHONES")
        self.status_var.set("Headphone test stopped.")
        self._save_settings()

    def configure_settings(self) -> None:
        dialog = SettingsDialog(self.root, self.settings)
        self.root.wait_window(dialog)
        if not dialog.result:
            return
        self.settings.update(dialog.result)
        self.ptt_enabled_var.set(self.settings["ptt_enabled"])
        self.update_ptt_status()
        self.start_hotkeys()
        if self.ptt_enabled_var.get():
            self.mute_var.set(True)
        else:
            self.mute_var.set(False)
        self.apply_mute()
        self._save_settings()
        self.status_var.set("Settings saved.")

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
        self.settings.update(
            {
                "input": self.input_var.get(),
                "output": self.output_var.get(),
                "monitor": self.monitor_var.get(),
                "character": self.character_var.get(),
                "monitor_enabled": self.monitor_enabled_var.get(),
                "ptt_enabled": self.ptt_enabled_var.get(),
            }
        )
        temporary = self.settings_path.with_suffix(".tmp")
        temporary.write_text(json.dumps(self.settings, indent=2), encoding="utf-8")
        temporary.replace(self.settings_path)

    def show_cable_help(self) -> None:
        open_page = messagebox.askyesno(
            "Virtual microphone setup",
            "To send your changed voice to other apps:\n\n"
            "1. Download VB-CABLE from its official site.\n"
            "2. Extract the ZIP and run VBCABLE_Setup_x64.exe as administrator.\n"
            "3. Restart Windows, launch LocalVoice, and click Refresh.\n"
            "4. LocalVoice uses CABLE Input. Select CABLE Output as the microphone in Discord, games, OBS, or your browser.\n\n"
            "You can test through headphones before installing it.\n\n"
            "Open the official VB-CABLE download page now?",
            parent=self.root,
        )
        if open_page:
            webbrowser.open("https://vb-audio.com/Cable/")

    def first_run_help(self) -> None:
        if self.settings["onboarding_complete"]:
            return
        self.status_var.set(
            "Start with TEST IN HEADPHONES. Set up the virtual mic only when you "
            "are ready to use LocalVoice in another app."
        )
        self.settings["onboarding_complete"] = True
        self._save_settings()

    def show_window(self) -> None:
        self.root.deiconify()
        self.root.lift()
        self.root.focus_force()

    def hide_window(self) -> None:
        self.root.withdraw()
        if self.tray:
            self.tray.notify_hidden()

    def window_close(self) -> None:
        if self.settings["close_to_tray"] and self.tray:
            self.hide_window()
        else:
            self.quit_app()

    def update_meters(self) -> None:
        self.input_meter["value"] = min(1, self.engine.input_level * 2)
        self.output_meter["value"] = min(1, self.engine.output_level * 2)
        if self.engine.error:
            self.status_var.set(f"Audio error (output muted): {self.engine.error}")
        elif self.engine.running and self.engine.xruns:
            self.status_var.set(
                f"LIVE at {self.engine.sample_rate:,} Hz | {self.engine.xruns} audio warning(s)"
            )
        self.root.after(80, self.update_meters)

    def quit_app(self) -> None:
        if self.quitting:
            return
        self.quitting = True
        self.engine.stop()
        if self.hotkeys:
            self.hotkeys.stop()
        if self.tray:
            self.tray.stop()
        self.root.destroy()


def main() -> None:
    mutex = None
    if os.name == "nt":
        mutex = ctypes.windll.kernel32.CreateMutexW(
            None, False, "LocalVoice.SingleInstance"
        )
        if ctypes.windll.kernel32.GetLastError() == 183:
            window = ctypes.windll.user32.FindWindowW(None, "LocalVoice")
            if window:
                ctypes.windll.user32.ShowWindow(window, 9)
                ctypes.windll.user32.SetForegroundWindow(window)
            else:
                ctypes.windll.user32.MessageBoxW(
                    None,
                    "LocalVoice is already running. Use its system-tray icon to open it.",
                    "LocalVoice",
                    0x40,
                )
            return
    root = tk.Tk()
    LocalVoiceApp(root)
    root.mainloop()
    if mutex:
        ctypes.windll.kernel32.CloseHandle(mutex)


if __name__ == "__main__":
    main()
