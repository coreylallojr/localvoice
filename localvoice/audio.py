from __future__ import annotations

import queue
from dataclasses import dataclass

import numpy as np
import sounddevice as sd
from pedalboard import (
    Bitcrush,
    Chorus,
    Compressor,
    Distortion,
    Gain,
    HighpassFilter,
    Limiter,
    LowpassFilter,
    NoiseGate,
    PeakFilter,
    Pedalboard,
    PitchShift,
    Reverb,
)

from .characters import Character


@dataclass(frozen=True, slots=True)
class AudioDevice:
    index: int
    name: str
    host_api: str
    inputs: int
    outputs: int

    @property
    def label(self) -> str:
        return f"{self.index}: {self.name} [{self.host_api}]"


def list_devices() -> tuple[list[AudioDevice], list[AudioDevice]]:
    host_apis = sd.query_hostapis()
    devices = []
    for index, info in enumerate(sd.query_devices()):
        devices.append(
            AudioDevice(
                index,
                str(info["name"]),
                str(host_apis[info["hostapi"]]["name"]),
                int(info["max_input_channels"]),
                int(info["max_output_channels"]),
            )
        )
    return (
        [item for item in devices if item.inputs],
        [item for item in devices if item.outputs],
    )


def build_board(character: Character) -> Pedalboard:
    plugins = [
        NoiseGate(threshold_db=character.gate_db, ratio=8, attack_ms=2, release_ms=120),
        HighpassFilter(cutoff_frequency_hz=character.highpass_hz),
        Compressor(threshold_db=-18, ratio=3, attack_ms=5, release_ms=100),
    ]
    if character.pitch_semitones:
        plugins.append(PitchShift(semitones=character.pitch_semitones))
    if character.presence_db:
        plugins.append(
            PeakFilter(cutoff_frequency_hz=3_000, gain_db=character.presence_db, q=0.8)
        )
    if character.lowpass_hz < 20_000:
        plugins.append(LowpassFilter(cutoff_frequency_hz=character.lowpass_hz))
    if character.distortion_db:
        plugins.append(Distortion(drive_db=character.distortion_db))
    if character.bit_depth < 32:
        plugins.append(Bitcrush(bit_depth=character.bit_depth))
    if character.chorus_mix:
        plugins.append(
            Chorus(
                rate_hz=1.4,
                depth=0.4,
                centre_delay_ms=7,
                feedback=0.08,
                mix=character.chorus_mix,
            )
        )
    if character.reverb_mix:
        plugins.append(
            Reverb(
                room_size=0.35,
                damping=0.7,
                wet_level=character.reverb_mix,
                dry_level=1.0,
            )
        )
    plugins.extend(
        (Gain(gain_db=character.gain_db), Limiter(threshold_db=-3, release_ms=60))
    )
    return Pedalboard(plugins)


class AudioEngine:
    def __init__(self) -> None:
        self.stream: sd.Stream | None = None
        self.commands: queue.SimpleQueue[tuple[str, object]] = queue.SimpleQueue()
        self.board = build_board(Character("clean", "Clean"))
        self.bypass = False
        self.muted = False
        self.input_level = 0.0
        self.output_level = 0.0
        self.xruns = 0
        self.sample_rate = 0
        self.error = ""

    @property
    def running(self) -> bool:
        return bool(self.stream and self.stream.active)

    def start(self, input_device: int, output_device: int, character: Character) -> int:
        self.stop()
        sample_rate = self._choose_sample_rate(input_device, output_device)
        self.board = build_board(character)
        self.error = ""
        self.xruns = 0
        self.sample_rate = sample_rate
        self.stream = sd.Stream(
            device=(input_device, output_device),
            samplerate=sample_rate,
            blocksize=512,
            dtype="float32",
            channels=(1, 1),
            latency="low",
            callback=self._callback,
        )
        self.stream.start()
        return sample_rate

    @staticmethod
    def _choose_sample_rate(input_device: int, output_device: int) -> int:
        errors = []
        for rate in (48_000, 44_100):
            try:
                sd.check_input_settings(
                    input_device, channels=1, dtype="float32", samplerate=rate
                )
                sd.check_output_settings(
                    output_device, channels=1, dtype="float32", samplerate=rate
                )
                return rate
            except sd.PortAudioError as error:
                errors.append(str(error))
        raise RuntimeError(
            "Input and output do not share a 48 kHz or 44.1 kHz format. "
            + " ".join(errors)
        )

    def stop(self) -> None:
        if self.stream:
            self.stream.stop()
            self.stream.close()
            self.stream = None
        self.input_level = self.output_level = 0.0

    def set_character(self, character: Character) -> None:
        self.commands.put(("character", character))

    def set_bypass(self, enabled: bool) -> None:
        self.commands.put(("bypass", enabled))

    def set_muted(self, enabled: bool) -> None:
        self.commands.put(("muted", enabled))

    def _apply_commands(self) -> None:
        while True:
            try:
                name, value = self.commands.get_nowait()
            except queue.Empty:
                return
            if name == "character":
                self.board = build_board(value)  # type: ignore[arg-type]
            elif name == "bypass":
                self.bypass = bool(value)
            elif name == "muted":
                self.muted = bool(value)

    def _callback(self, indata, outdata, frames, time_info, status) -> None:
        del frames, time_info
        try:
            self._apply_commands()
            source = np.asarray(indata[:, 0], dtype=np.float32)
            self.input_level = float(np.max(np.abs(source), initial=0))
            if status:
                self.xruns += 1
            if self.muted:
                output = np.zeros_like(source)
            elif self.bypass:
                output = source
            else:
                processed = self.board(
                    source[np.newaxis, :], self.sample_rate, reset=False
                )
                output = np.asarray(processed).reshape(-1)[: len(source)]
            np.clip(output, -1, 1, out=output)
            outdata[:, 0] = output
            self.output_level = float(np.max(np.abs(output), initial=0))
        except Exception as error:  # noqa: BLE001 - fail silent so the raw voice never leaks.
            outdata.fill(0)
            self.error = str(error)
            self.xruns += 1
