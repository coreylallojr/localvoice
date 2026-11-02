from __future__ import annotations

import json
import os
import uuid
import zipfile
from dataclasses import asdict, dataclass, fields, replace
from pathlib import Path, PurePosixPath

SCHEMA_VERSION = 1
MAX_PACK_BYTES = 25 * 1024 * 1024


@dataclass(frozen=True, slots=True)
class Character:
    id: str
    name: str
    description: str = ""
    pitch_semitones: float = 0.0
    gate_db: float = -55.0
    highpass_hz: float = 80.0
    lowpass_hz: float = 20_000.0
    presence_db: float = 0.0
    distortion_db: float = 0.0
    bit_depth: float = 32.0
    chorus_mix: float = 0.0
    reverb_mix: float = 0.0
    gain_db: float = 0.0
    builtin: bool = False

    def validated(self) -> Character:
        name = self.name.strip()
        if not name or len(name) > 60:
            raise ValueError("Character name must be between 1 and 60 characters.")
        limits = {
            "pitch_semitones": (-12, 12),
            "gate_db": (-100, 0),
            "highpass_hz": (20, 1_000),
            "lowpass_hz": (1_000, 22_000),
            "presence_db": (-12, 12),
            "distortion_db": (0, 30),
            "bit_depth": (2, 32),
            "chorus_mix": (0, 1),
            "reverb_mix": (0, 1),
            "gain_db": (-24, 24),
        }
        for key, (minimum, maximum) in limits.items():
            value = float(getattr(self, key))
            if not minimum <= value <= maximum:
                raise ValueError(f"{key} must be between {minimum} and {maximum}.")
        if self.highpass_hz >= self.lowpass_hz:
            raise ValueError("High-pass frequency must be below low-pass frequency.")
        return replace(self, name=name)


BUILTINS = (
    Character("clean", "Clean", "Noise gate, clarity and leveling", builtin=True),
    Character(
        "deep",
        "Deep",
        "Lower, warmer voice",
        pitch_semitones=-4,
        lowpass_hz=12_000,
        presence_db=-2,
        builtin=True,
    ),
    Character(
        "bright",
        "Bright",
        "Lighter, more present voice",
        pitch_semitones=3,
        highpass_hz=110,
        presence_db=4,
        builtin=True,
    ),
    Character(
        "demon",
        "Demon",
        "Low pitch with grit and width",
        pitch_semitones=-7,
        lowpass_hz=9_000,
        distortion_db=8,
        chorus_mix=0.18,
        builtin=True,
    ),
    Character(
        "robot",
        "Robot",
        "Digital, modulated character",
        bit_depth=6,
        chorus_mix=0.65,
        presence_db=5,
        builtin=True,
    ),
    Character(
        "radio",
        "Radio",
        "Narrow-band broadcast effect",
        highpass_hz=300,
        lowpass_hz=3_400,
        distortion_db=4,
        bit_depth=12,
        builtin=True,
    ),
    Character(
        "giant",
        "Giant",
        "Very low cinematic voice",
        pitch_semitones=-10,
        lowpass_hz=7_000,
        presence_db=-3,
        reverb_mix=0.1,
        builtin=True,
    ),
    Character(
        "sprite",
        "Sprite",
        "High, playful character",
        pitch_semitones=7,
        highpass_hz=160,
        presence_db=3,
        builtin=True,
    ),
)


class CharacterStore:
    def __init__(self, root: Path | None = None):
        appdata = Path(os.environ.get("APPDATA", Path.home()))
        self.root = root or appdata / "LocalVoice"
        self.path = self.root / "characters.json"
        self.root.mkdir(parents=True, exist_ok=True)

    def load(self) -> list[Character]:
        custom: list[Character] = []
        if self.path.exists():
            try:
                payload = json.loads(self.path.read_text(encoding="utf-8"))
                custom = [character_from_dict(item, builtin=False) for item in payload]
            except (OSError, ValueError, TypeError, json.JSONDecodeError):
                custom = []
        return [*BUILTINS, *custom]

    def save_custom(self, characters: list[Character]) -> None:
        payload = [
            asdict(item) | {"builtin": False} for item in characters if not item.builtin
        ]
        temporary = self.path.with_suffix(".tmp")
        temporary.write_text(json.dumps(payload, indent=2), encoding="utf-8")
        temporary.replace(self.path)


def character_from_dict(payload: dict, *, builtin: bool = False) -> Character:
    allowed = {field.name for field in fields(Character)}
    values = {key: value for key, value in payload.items() if key in allowed}
    values["builtin"] = builtin
    values.setdefault("id", uuid.uuid4().hex)
    return Character(**values).validated()


def import_voicepack(path: Path) -> Character:
    if path.stat().st_size > MAX_PACK_BYTES:
        raise ValueError("Voice pack is larger than 25 MB.")
    with zipfile.ZipFile(path) as archive:
        infos = archive.infolist()
        if len(infos) > 20 or sum(item.file_size for item in infos) > MAX_PACK_BYTES:
            raise ValueError("Voice pack is too large.")
        for item in infos:
            member = PurePosixPath(item.filename)
            if member.is_absolute() or ".." in member.parts:
                raise ValueError("Voice pack contains an unsafe path.")
        try:
            manifest = json.loads(archive.read("manifest.json"))
        except KeyError as error:
            raise ValueError("Voice pack does not contain manifest.json.") from error
    if manifest.get("schema") != SCHEMA_VERSION or manifest.get("engine") != "dsp":
        raise ValueError("This release imports LocalVoice DSP packs only.")
    character = character_from_dict(manifest.get("character", {}))
    return replace(character, id=uuid.uuid4().hex, builtin=False)


def export_voicepack(character: Character, path: Path) -> None:
    manifest = {
        "schema": SCHEMA_VERSION,
        "engine": "dsp",
        "character": asdict(replace(character, builtin=False)),
    }
    with zipfile.ZipFile(path, "w", compression=zipfile.ZIP_DEFLATED) as archive:
        archive.writestr("manifest.json", json.dumps(manifest, indent=2))
