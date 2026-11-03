//! Voices (characters): parameters, built-ins, persistence and `.voicepack` files.
//! JSON stays field-compatible with LocalVoice v0.2 `characters.json` and packs.

use serde::{Deserialize, Serialize};
use std::io::{Read, Write};
use std::path::{Component, Path, PathBuf};

pub const SCHEMA_VERSION: u32 = 1;
const MAX_PACK_BYTES: u64 = 25 * 1024 * 1024;

#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct Params {
    pub pitch_semitones: f32,
    /// Formant shift relative to the speaker; 0 keeps the voice natural.
    pub formant_semitones: f32,
    pub gate_db: f32,
    pub denoise: bool,
    pub highpass_hz: f32,
    pub lowpass_hz: f32,
    pub presence_db: f32,
    pub distortion_db: f32,
    pub bit_depth: f32,
    pub ring_hz: f32,
    pub chorus_mix: f32,
    pub reverb_mix: f32,
    pub gain_db: f32,
}

impl Default for Params {
    fn default() -> Self {
        Self {
            pitch_semitones: 0.0,
            formant_semitones: 0.0,
            gate_db: -55.0,
            denoise: false,
            highpass_hz: 80.0,
            lowpass_hz: 20_000.0,
            presence_db: 0.0,
            distortion_db: 0.0,
            bit_depth: 32.0,
            ring_hz: 0.0,
            chorus_mix: 0.0,
            reverb_mix: 0.0,
            gain_db: 0.0,
        }
    }
}

/// (field, min, max) — shared by validation and the editor UI.
pub const LIMITS: &[(&str, f32, f32)] = &[
    ("pitch_semitones", -12.0, 12.0),
    ("formant_semitones", -12.0, 12.0),
    ("gate_db", -100.0, 0.0),
    ("highpass_hz", 20.0, 1_000.0),
    ("lowpass_hz", 1_000.0, 22_000.0),
    ("presence_db", -12.0, 12.0),
    ("distortion_db", 0.0, 30.0),
    ("bit_depth", 2.0, 32.0),
    ("ring_hz", 0.0, 500.0),
    ("chorus_mix", 0.0, 1.0),
    ("reverb_mix", 0.0, 1.0),
    ("gain_db", -24.0, 24.0),
];

impl Params {
    pub fn get(&self, field: &str) -> f32 {
        match field {
            "pitch_semitones" => self.pitch_semitones,
            "formant_semitones" => self.formant_semitones,
            "gate_db" => self.gate_db,
            "highpass_hz" => self.highpass_hz,
            "lowpass_hz" => self.lowpass_hz,
            "presence_db" => self.presence_db,
            "distortion_db" => self.distortion_db,
            "bit_depth" => self.bit_depth,
            "ring_hz" => self.ring_hz,
            "chorus_mix" => self.chorus_mix,
            "reverb_mix" => self.reverb_mix,
            "gain_db" => self.gain_db,
            _ => unreachable!("unknown field {field}"),
        }
    }

    pub fn validate(&self) -> Result<(), String> {
        for &(field, min, max) in LIMITS {
            let value = self.get(field);
            if !(min..=max).contains(&value) {
                return Err(format!("{field} must be between {min} and {max}."));
            }
        }
        if self.highpass_hz >= self.lowpass_hz {
            return Err("Low cut must be below high cut.".into());
        }
        Ok(())
    }
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Voice {
    #[serde(default = "new_id")]
    pub id: String,
    pub name: String,
    #[serde(default)]
    pub description: String,
    #[serde(flatten)]
    pub params: Params,
    #[serde(default, skip_serializing)]
    pub builtin: bool,
}

pub fn new_id() -> String {
    use std::time::{SystemTime, UNIX_EPOCH};
    let nanos = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_nanos())
        .unwrap_or(0);
    format!("{nanos:x}")
}

impl Voice {
    pub fn validated(mut self) -> Result<Self, String> {
        self.name = self.name.trim().to_string();
        if self.name.is_empty() || self.name.chars().count() > 60 {
            return Err("Name must be between 1 and 60 characters.".into());
        }
        self.params.validate()?;
        Ok(self)
    }
}

fn builtin(id: &str, name: &str, description: &str, params: Params) -> Voice {
    Voice {
        id: id.into(),
        name: name.into(),
        description: description.into(),
        params,
        builtin: true,
    }
}

pub fn builtins() -> Vec<Voice> {
    let base = Params::default();
    vec![
        builtin(
            "clean",
            "Clean",
            "Noise removal, clarity and leveling",
            Params {
                denoise: true,
                ..base
            },
        ),
        builtin(
            "deep",
            "Deep",
            "Lower, natural-sounding voice",
            Params {
                pitch_semitones: -4.0,
                lowpass_hz: 14_000.0,
                presence_db: 1.0,
                denoise: true,
                ..base
            },
        ),
        builtin(
            "bright",
            "Bright",
            "Higher, natural-sounding voice",
            Params {
                pitch_semitones: 3.0,
                highpass_hz: 110.0,
                presence_db: 2.5,
                denoise: true,
                ..base
            },
        ),
        builtin(
            "demon",
            "Demon",
            "Low pitch, huge throat, grit",
            Params {
                pitch_semitones: -7.0,
                formant_semitones: -4.0,
                lowpass_hz: 9_000.0,
                distortion_db: 8.0,
                chorus_mix: 0.18,
                reverb_mix: 0.08,
                ..base
            },
        ),
        builtin(
            "robot",
            "Robot",
            "Metallic ring-modulated machine voice",
            Params {
                ring_hz: 55.0,
                bit_depth: 10.0,
                chorus_mix: 0.15,
                presence_db: 3.0,
                gain_db: 3.0,
                ..base
            },
        ),
        builtin(
            "radio",
            "Radio",
            "Narrow-band broadcast effect",
            Params {
                highpass_hz: 300.0,
                lowpass_hz: 3_400.0,
                distortion_db: 4.0,
                bit_depth: 12.0,
                ..base
            },
        ),
        builtin(
            "giant",
            "Giant",
            "Very low cinematic voice",
            Params {
                pitch_semitones: -10.0,
                formant_semitones: -5.0,
                lowpass_hz: 7_000.0,
                presence_db: -2.0,
                reverb_mix: 0.12,
                ..base
            },
        ),
        builtin(
            "sprite",
            "Sprite",
            "High, playful character",
            Params {
                pitch_semitones: 7.0,
                formant_semitones: 4.0,
                highpass_hz: 160.0,
                presence_db: 3.0,
                ..base
            },
        ),
    ]
}

pub fn data_dir() -> PathBuf {
    let root = std::env::var_os("APPDATA")
        .map(PathBuf::from)
        .unwrap_or_else(|| ".".into());
    root.join("LocalVoice")
}

pub struct Store {
    path: PathBuf,
}

impl Store {
    pub fn new(root: &Path) -> Self {
        let _ = std::fs::create_dir_all(root);
        Self {
            path: root.join("characters.json"),
        }
    }

    /// Built-ins followed by saved customs. A corrupt file loads as no customs.
    pub fn load(&self) -> Vec<Voice> {
        let custom: Vec<Voice> = read_json_text(&self.path)
            .and_then(|text| serde_json::from_str::<Vec<Voice>>(&text).ok())
            .unwrap_or_default()
            .into_iter()
            .filter_map(|voice| {
                Voice {
                    builtin: false,
                    ..voice
                }
                .validated()
                .ok()
            })
            .collect();
        builtins().into_iter().chain(custom).collect()
    }

    pub fn save(&self, voices: &[Voice]) -> std::io::Result<()> {
        let custom: Vec<&Voice> = voices.iter().filter(|v| !v.builtin).collect();
        write_atomic(
            &self.path,
            serde_json::to_string_pretty(&custom)?.as_bytes(),
        )
    }
}

/// Reads a JSON text file, ignoring a UTF-8 BOM (Notepad and Windows PowerShell add one).
pub fn read_json_text(path: &Path) -> Option<String> {
    let text = std::fs::read_to_string(path).ok()?;
    Some(text.strip_prefix('\u{feff}').unwrap_or(&text).to_string())
}

pub fn write_atomic(path: &Path, bytes: &[u8]) -> std::io::Result<()> {
    let temporary = path.with_extension("tmp");
    std::fs::write(&temporary, bytes)?;
    std::fs::rename(temporary, path)
}

#[derive(Serialize, Deserialize)]
struct Manifest {
    schema: u32,
    engine: String,
    character: Voice,
}

pub fn import_pack(path: &Path) -> Result<Voice, String> {
    let file = std::fs::File::open(path).map_err(|e| e.to_string())?;
    if file.metadata().map_err(|e| e.to_string())?.len() > MAX_PACK_BYTES {
        return Err("Voice pack is larger than 25 MB.".into());
    }
    let mut archive = zip::ZipArchive::new(file).map_err(|e| format!("Not a voice pack: {e}"))?;
    if archive.len() > 20 {
        return Err("Voice pack has too many files.".into());
    }
    let mut total = 0u64;
    for index in 0..archive.len() {
        let entry = archive.by_index(index).map_err(|e| e.to_string())?;
        total += entry.size();
        let name = entry
            .name()
            .map_err(|_| "Voice pack contains an unsafe path.".to_string())?;
        let unsafe_path = Path::new(name.as_ref())
            .components()
            .any(|c| !matches!(c, Component::Normal(_) | Component::CurDir));
        if unsafe_path || name.starts_with('/') || name.contains('\\') {
            return Err("Voice pack contains an unsafe path.".into());
        }
    }
    if total > MAX_PACK_BYTES {
        return Err("Voice pack is too large.".into());
    }
    let mut text = String::new();
    archive
        .by_name("manifest.json")
        .map_err(|_| "Voice pack does not contain manifest.json.".to_string())?
        .take(1024 * 1024)
        .read_to_string(&mut text)
        .map_err(|e| e.to_string())?;
    let manifest: Manifest =
        serde_json::from_str(&text).map_err(|e| format!("Bad manifest: {e}"))?;
    if manifest.schema != SCHEMA_VERSION || manifest.engine != "dsp" {
        return Err("This version imports LocalVoice DSP packs only.".into());
    }
    Voice {
        id: new_id(),
        builtin: false,
        ..manifest.character
    }
    .validated()
}

pub fn export_pack(voice: &Voice, path: &Path) -> Result<(), String> {
    let manifest = Manifest {
        schema: SCHEMA_VERSION,
        engine: "dsp".into(),
        character: Voice {
            builtin: false,
            ..voice.clone()
        },
    };
    let file = std::fs::File::create(path).map_err(|e| e.to_string())?;
    let mut archive = zip::ZipWriter::new(file);
    archive
        .start_file("manifest.json", zip::write::SimpleFileOptions::default())
        .map_err(|e| e.to_string())?;
    archive
        .write_all(
            serde_json::to_string_pretty(&manifest)
                .map_err(|e| e.to_string())?
                .as_bytes(),
        )
        .map_err(|e| e.to_string())?;
    archive.finish().map_err(|e| e.to_string())?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn temp_dir(name: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!("localvoice-test-{name}-{}", new_id()));
        std::fs::create_dir_all(&dir).unwrap();
        dir
    }

    #[test]
    fn builtins_are_valid() {
        for voice in builtins() {
            voice
                .clone()
                .validated()
                .unwrap_or_else(|e| panic!("{}: {e}", voice.name));
        }
    }

    #[test]
    fn loads_v02_characters_json() {
        let dir = temp_dir("v02");
        std::fs::write(
            dir.join("characters.json"),
            r#"[{"id":"abc","name":"Morgan","description":"","pitch_semitones":-3.0,
                "gate_db":-55.0,"highpass_hz":80.0,"lowpass_hz":20000.0,"presence_db":0.0,
                "distortion_db":0.0,"bit_depth":32.0,"chorus_mix":0.0,"reverb_mix":0.0,
                "gain_db":0.0,"builtin":false}]"#,
        )
        .unwrap();
        let voices = Store::new(&dir).load();
        let morgan = voices.last().unwrap();
        assert_eq!(morgan.name, "Morgan");
        assert_eq!(morgan.params.pitch_semitones, -3.0);
        assert_eq!(morgan.params.formant_semitones, 0.0);
        assert!(!morgan.builtin);
    }

    #[test]
    fn store_and_pack_round_trip() {
        let dir = temp_dir("roundtrip");
        let store = Store::new(&dir);
        let mut voices = store.load();
        let custom = Voice {
            id: "x".into(),
            name: "Mine".into(),
            description: String::new(),
            params: Params {
                pitch_semitones: 2.0,
                ring_hz: 40.0,
                ..Params::default()
            },
            builtin: false,
        };
        voices.push(custom.clone());
        store.save(&voices).unwrap();
        assert_eq!(store.load().last().unwrap(), &custom);

        let pack = dir.join("mine.voicepack");
        export_pack(&custom, &pack).unwrap();
        let imported = import_pack(&pack).unwrap();
        assert_eq!(imported.params, custom.params);
        assert_ne!(imported.id, custom.id);
    }

    #[test]
    fn rejects_unsafe_and_invalid_packs() {
        let dir = temp_dir("unsafe");
        let pack = dir.join("bad.voicepack");
        let mut archive = zip::ZipWriter::new(std::fs::File::create(&pack).unwrap());
        archive
            .start_file("../manifest.json", zip::write::SimpleFileOptions::default())
            .unwrap();
        archive.write_all(b"{}").unwrap();
        archive.finish().unwrap();
        assert!(import_pack(&pack).unwrap_err().contains("unsafe path"));

        std::fs::write(&pack, b"not a zip").unwrap();
        assert!(import_pack(&pack).is_err());
        assert!(
            Params {
                pitch_semitones: 30.0,
                ..Params::default()
            }
            .validate()
            .is_err()
        );
    }
}
