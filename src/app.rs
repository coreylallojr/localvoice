//! App controller: settings, voices and device choices behind one lock, with
//! actions callable from the UI, hotkey and tray threads alike.

use crate::devices::{self, DeviceLists};
use crate::engine::{self, Command, Mode, Shared, StartRequest};
use crate::voices::{self, Params, Store, Voice};
use serde::{Deserialize, Serialize};
use std::sync::atomic::Ordering::Relaxed;
use std::sync::mpsc::Sender;
use std::sync::{Arc, Mutex, MutexGuard};

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(default)]
pub struct Settings {
    pub settings_version: u32,
    pub input: String,
    pub output: String,
    pub monitor: String,
    pub character: String,
    pub ptt_enabled: bool,
    pub ptt_key: String,
    pub ptt_mode: String,
    pub close_to_tray: bool,
    pub auto_live: bool,
    pub monitor_enabled: bool,
    pub monitor_warning_seen: bool,
    pub input_gain_db: f32,
    pub monitor_volume: f32,
}

impl Default for Settings {
    fn default() -> Self {
        Self {
            settings_version: 3,
            input: String::new(),
            output: String::new(),
            monitor: String::new(),
            character: "Clean".into(),
            ptt_enabled: false,
            ptt_key: "f8".into(),
            ptt_mode: "hold".into(),
            close_to_tray: true,
            auto_live: false,
            monitor_enabled: false,
            monitor_warning_seen: false,
            input_gain_db: 0.0,
            monitor_volume: 0.8,
        }
    }
}

impl Settings {
    fn load(path: &std::path::Path) -> Self {
        let mut settings: Settings = voices::read_json_text(path)
            .and_then(|text| serde_json::from_str(&text).ok())
            .unwrap_or_default();
        // v0.2 stored labels like "Microphone (X) [Windows WASAPI]".
        for name in [
            &mut settings.input,
            &mut settings.output,
            &mut settings.monitor,
        ] {
            if let Some((device, _)) = name.rsplit_once(" [") {
                *name = device.to_string();
            }
        }
        settings.settings_version = 3;
        settings
    }
}

pub struct Session {
    pub settings: Settings,
    pub voices: Vec<Voice>,
    pub current: usize,
    pub devices: DeviceLists,
    /// Key-capture mode for the push-to-talk button.
    pub capturing_key: bool,
}

impl Session {
    pub fn voice(&self) -> &Voice {
        &self.voices[self.current]
    }
}

pub struct Controller {
    pub shared: Arc<Shared>,
    engine: Sender<Command>,
    session: Mutex<Session>,
    store: Store,
    settings_path: std::path::PathBuf,
    notify: Arc<dyn Fn() + Send + Sync>,
}

impl Controller {
    pub fn new(notify: Arc<dyn Fn() + Send + Sync>) -> Arc<Self> {
        let root = voices::data_dir();
        let store = Store::new(&root);
        let settings_path = root.join("settings.json");
        let settings = Settings::load(&settings_path);
        let voices = store.load();
        let current = voices
            .iter()
            .position(|v| v.name == settings.character)
            .unwrap_or(0);
        let shared = Shared::new();
        let engine = engine::spawn(shared.clone(), notify.clone());
        let controller = Arc::new(Self {
            shared,
            engine,
            session: Mutex::new(Session {
                settings,
                voices,
                current,
                devices: DeviceLists::default(),
                capturing_key: false,
            }),
            store,
            settings_path,
            notify,
        });
        controller.refresh_devices();
        {
            let session = controller.session();
            let s = &controller.shared;
            s.input_gain.set(crate::dsp::effects::db_to_gain(
                session.settings.input_gain_db,
            ));
            s.monitor_volume.set(session.settings.monitor_volume);
            s.monitor_on
                .store(session.settings.monitor_enabled, Relaxed);
            // Push-to-talk switches the voice effect: off (real voice) until the key is used.
            s.bypass.store(session.settings.ptt_enabled, Relaxed);
        }
        controller.save_settings();
        controller
    }

    pub fn session(&self) -> MutexGuard<'_, Session> {
        self.session.lock().unwrap_or_else(|e| e.into_inner())
    }

    pub fn mode(&self) -> Mode {
        self.shared.status().mode
    }

    pub fn refresh_devices(&self) {
        let lists = devices::list();
        let mut session = self.session();
        let s = &mut session.settings;
        if !lists.inputs.contains(&s.input) {
            s.input = lists
                .default_input
                .clone()
                .filter(|d| lists.inputs.contains(d))
                .or_else(|| lists.inputs.first().cloned())
                .unwrap_or_default();
        }
        if !lists.virtual_outputs.contains(&s.output) {
            s.output = lists.virtual_outputs.first().cloned().unwrap_or_default();
        }
        if !lists.monitors.contains(&s.monitor) {
            s.monitor = devices::pick_monitor(
                &lists.monitors,
                lists.default_output.as_deref(),
                Some(&s.input),
            )
            .unwrap_or_default();
        }
        session.devices = lists;
        drop(session);
        self.save_settings();
    }

    pub fn request(&self, mode: Mode) -> StartRequest {
        let session = self.session();
        let s = &session.settings;
        let some = |name: &String| (!name.is_empty()).then(|| name.clone());
        StartRequest {
            mode,
            input: s.input.clone(),
            virtual_mic: some(&s.output),
            headphones: some(&s.monitor),
            params: session.voice().params,
            test_tone: false,
        }
    }

    pub fn go_live(&self) {
        if self.session().settings.ptt_enabled && self.session().settings.ptt_mode == "hold" {
            self.shared.bypass.store(true, Relaxed);
        }
        let _ = self.engine.send(Command::Start(self.request(Mode::Live)));
    }

    pub fn start(&self, request: StartRequest) {
        let _ = self.engine.send(Command::Start(request));
    }

    pub fn stop(&self) {
        let _ = self.engine.send(Command::Stop);
    }

    pub fn toggle_live(&self) {
        if self.mode() == Mode::Live {
            self.stop()
        } else {
            self.go_live()
        }
    }

    /// Headphone button: while live it toggles monitoring; otherwise a private test.
    pub fn toggle_headphones(&self) {
        match self.mode() {
            Mode::Live => {
                let on = !self.shared.monitor_on.load(Relaxed);
                self.shared.monitor_on.store(on, Relaxed);
                self.session().settings.monitor_enabled = on;
                self.save_settings();
            }
            Mode::Test => self.stop(),
            Mode::Stopped => {
                let _ = self.engine.send(Command::Start(self.request(Mode::Test)));
            }
        }
    }

    pub fn select_voice(&self, index: usize) {
        let mut session = self.session();
        if index >= session.voices.len() {
            return;
        }
        session.current = index;
        session.settings.character = session.voices[index].name.clone();
        self.shared.set_params(session.voices[index].params);
        drop(session);
        self.save_settings();
        (self.notify)();
    }

    pub fn step_voice(&self, delta: i32) {
        let (current, count) = {
            let session = self.session();
            (session.current as i32, session.voices.len() as i32)
        };
        self.select_voice((current + delta).rem_euclid(count) as usize);
    }

    /// Editor sliders call this on every change: the sound updates immediately.
    pub fn preview(&self, params: Params) {
        self.shared.set_params(params);
    }

    pub fn revert_preview(&self) {
        let params = self.session().voice().params;
        self.shared.set_params(params);
    }

    pub fn save_voice(&self, mut voice: Voice, as_new: bool) -> Result<(), String> {
        let mut session = self.session();
        let current = session.current;
        let replacing = !as_new && !session.voices[current].builtin;
        if !replacing {
            voice.id = voices::new_id();
            voice.builtin = false;
        } else {
            voice.id = session.voices[current].id.clone();
        }
        let voice = voice.validated()?;
        let taken =
            session.voices.iter().enumerate().any(|(i, v)| {
                v.name.eq_ignore_ascii_case(&voice.name) && !(replacing && i == current)
            });
        if taken {
            return Err(format!("A voice named \"{}\" already exists.", voice.name));
        }
        let index = if replacing {
            session.voices[current] = voice;
            current
        } else {
            session.voices.push(voice);
            session.voices.len() - 1
        };
        self.store
            .save(&session.voices)
            .map_err(|e| e.to_string())?;
        drop(session);
        self.select_voice(index);
        Ok(())
    }

    pub fn delete_voice(&self) -> Result<(), String> {
        let mut session = self.session();
        let current = session.current;
        if session.voices[current].builtin {
            return Err("Built-in voices cannot be deleted.".into());
        }
        session.voices.remove(current);
        self.store
            .save(&session.voices)
            .map_err(|e| e.to_string())?;
        drop(session);
        self.select_voice(0);
        Ok(())
    }

    pub fn import_voice(&self, path: &std::path::Path) -> Result<String, String> {
        let mut voice = voices::import_pack(path)?;
        let mut session = self.session();
        let base = voice.name.clone();
        let mut number = 2;
        while session
            .voices
            .iter()
            .any(|v| v.name.eq_ignore_ascii_case(&voice.name))
        {
            voice.name = format!("{base} ({number})");
            number += 1;
        }
        let name = voice.name.clone();
        session.voices.push(voice);
        let index = session.voices.len() - 1;
        self.store
            .save(&session.voices)
            .map_err(|e| e.to_string())?;
        drop(session);
        self.select_voice(index);
        Ok(name)
    }

    pub fn toggle_mute(&self) {
        self.shared.muted.fetch_xor(true, Relaxed);
        (self.notify)();
    }

    pub fn toggle_bypass(&self) {
        self.shared.bypass.fetch_xor(true, Relaxed);
        (self.notify)();
    }

    pub fn set_ptt_enabled(&self, enabled: bool) {
        self.session().settings.ptt_enabled = enabled;
        self.shared.bypass.store(enabled, Relaxed);
        self.save_settings();
    }

    /// Push-to-talk edge from the hotkey thread. It controls the voice effect, not the
    /// mic: hold = effect only while held; toggle = each press switches it on/off.
    pub fn ptt(&self, down: bool) {
        let (enabled, hold) = {
            let s = &self.session().settings;
            (s.ptt_enabled, s.ptt_mode == "hold")
        };
        if !enabled {
            return;
        }
        if hold {
            self.shared.bypass.store(!down, Relaxed);
        } else if down {
            self.shared.bypass.fetch_xor(true, Relaxed);
        }
        (self.notify)();
    }

    pub fn captured_key(&self, name: String) {
        let mut session = self.session();
        session.capturing_key = false;
        session.settings.ptt_key = name;
        drop(session);
        self.save_settings();
        (self.notify)();
    }

    pub fn set_input_gain_db(&self, db: f32) {
        self.session().settings.input_gain_db = db;
        self.shared
            .input_gain
            .set(crate::dsp::effects::db_to_gain(db));
    }

    pub fn set_monitor_volume(&self, volume: f32) {
        self.session().settings.monitor_volume = volume;
        self.shared.monitor_volume.set(volume);
    }

    pub fn save_settings(&self) {
        let settings = self.session().settings.clone();
        if let Ok(text) = serde_json::to_string_pretty(&settings) {
            let _ = voices::write_atomic(&self.settings_path, text.as_bytes());
        }
    }

    pub fn quit(&self) {
        self.save_settings();
        let _ = self.engine.send(Command::Quit);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn loads_v02_settings_and_strips_host_api_labels() {
        let dir = std::env::temp_dir().join(format!("localvoice-settings-{}", voices::new_id()));
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join("settings.json");
        // Starts with a UTF-8 BOM, as written by Notepad / Windows PowerShell.
        let json = r#"{"settings_version":2,"ptt_enabled":true,"ptt_key":"f8","ptt_mode":"toggle",
               "input":"Microphone (PRO X 2 LIGHTSPEED) [Windows WASAPI]",
               "output":"Virtual microphone not installed","character":"Deep"}"#;
        std::fs::write(&path, format!("\u{feff}{json}")).unwrap();
        let settings = Settings::load(&path);
        assert_eq!(settings.input, "Microphone (PRO X 2 LIGHTSPEED)");
        assert_eq!(settings.ptt_mode, "toggle");
        assert_eq!(settings.character, "Deep");
        assert_eq!(settings.monitor_volume, 0.8);
    }
}
