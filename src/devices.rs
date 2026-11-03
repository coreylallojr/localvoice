//! Audio device discovery and the default-picking rules.

use cpal::traits::{DeviceTrait, HostTrait};

#[derive(Clone, Debug, Default, PartialEq)]
pub struct DeviceLists {
    pub inputs: Vec<String>,
    pub virtual_outputs: Vec<String>,
    pub monitors: Vec<String>,
    pub default_input: Option<String>,
    pub default_output: Option<String>,
}

const VIRTUAL_TERMS: [&str; 5] = [
    "virtual audio",
    "virtual mic",
    "cable input",
    "vb-audio",
    "voicemeeter input",
];

pub fn is_virtual(name: &str) -> bool {
    let name = name.to_lowercase();
    VIRTUAL_TERMS.iter().any(|term| name.contains(term))
}

/// Usable virtual-mic outputs, best first.
/// ponytail: name-based. VB-CABLE's "CABLE In 16ch" endpoint fails to open with
/// mono/stereo streams, so it is skipped and plain "CABLE Input" goes first.
pub fn rank_virtual(outputs: &[String]) -> Vec<String> {
    let mut usable: Vec<String> = outputs
        .iter()
        .filter(|name| is_virtual(name) && !name.to_lowercase().replace(' ', "").contains("16ch"))
        .cloned()
        .collect();
    usable.sort_by_key(|name| !name.to_lowercase().starts_with("cable input"));
    usable
}

/// Headphones: Windows' default playback if it is a real device; otherwise the
/// headset that owns the chosen mic, e.g. "(PRO X 2 LIGHTSPEED)"; otherwise the first.
pub fn pick_monitor(
    monitors: &[String],
    default_output: Option<&str>,
    mic: Option<&str>,
) -> Option<String> {
    if let Some(default) = default_output.filter(|d| monitors.iter().any(|m| m == d)) {
        return Some(default.to_string());
    }
    let owner = mic
        .and_then(|mic| mic.split_once('('))
        .map(|(_, rest)| rest.trim_end_matches(')'));
    if let Some(owner) = owner {
        let tag = format!("({owner})");
        if let Some(found) = monitors.iter().find(|m| m.contains(&tag)) {
            return Some(found.clone());
        }
    }
    monitors.first().cloned()
}

fn name(device: &cpal::Device) -> Option<String> {
    device.description().ok().map(|d| d.name().to_string())
}

pub fn list() -> DeviceLists {
    let host = cpal::default_host();
    let inputs: Vec<String> = host
        .input_devices()
        .map(|devices| devices.filter_map(|d| name(&d)).collect())
        .unwrap_or_default();
    let outputs: Vec<String> = host
        .output_devices()
        .map(|devices| devices.filter_map(|d| name(&d)).collect())
        .unwrap_or_default();
    DeviceLists {
        inputs: inputs
            .into_iter()
            .filter(|n| !is_virtual_capture(n))
            .collect(),
        virtual_outputs: rank_virtual(&outputs),
        monitors: outputs.iter().filter(|n| !is_virtual(n)).cloned().collect(),
        default_input: host.default_input_device().and_then(|d| name(&d)),
        default_output: host.default_output_device().and_then(|d| name(&d)),
    }
}

/// "CABLE Output" is the virtual mic other apps record from; never capture it
/// ourselves or LocalVoice would feed back into itself.
fn is_virtual_capture(name: &str) -> bool {
    let name = name.to_lowercase();
    name.contains("cable output")
        || name.contains("vb-audio point")
        || name.contains("voicemeeter out")
}

pub fn find_input(name: &str) -> Option<cpal::Device> {
    cpal::default_host()
        .input_devices()
        .ok()?
        .find(|d| self::name(d).as_deref() == Some(name))
}

pub fn find_output(name: &str) -> Option<cpal::Device> {
    cpal::default_host()
        .output_devices()
        .ok()?
        .find(|d| self::name(d).as_deref() == Some(name))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn names(list: &[&str]) -> Vec<String> {
        list.iter().map(|s| s.to_string()).collect()
    }

    #[test]
    fn skips_16ch_and_prefers_cable_input() {
        let outputs = names(&[
            "CABLE In 16ch (VB-Audio Virtual Cable)",
            "Speakers (PRO X 2 LIGHTSPEED)",
            "Voicemeeter Input (VB-Audio Voicemeeter VAIO)",
            "CABLE Input (VB-Audio Virtual Cable)",
        ]);
        assert_eq!(
            rank_virtual(&outputs),
            names(&[
                "CABLE Input (VB-Audio Virtual Cable)",
                "Voicemeeter Input (VB-Audio Voicemeeter VAIO)"
            ])
        );
    }

    #[test]
    fn monitor_falls_back_to_the_mic_headset_when_default_is_virtual() {
        let monitors = names(&[
            "CU34G2XP (NVIDIA High Definition Audio)",
            "Speakers (PRO X 2 LIGHTSPEED)",
        ]);
        assert_eq!(
            pick_monitor(
                &monitors,
                Some("CABLE Input (VB-Audio Virtual Cable)"),
                Some("Microphone (PRO X 2 LIGHTSPEED)")
            ),
            Some("Speakers (PRO X 2 LIGHTSPEED)".into())
        );
        assert_eq!(
            pick_monitor(
                &monitors,
                Some("CU34G2XP (NVIDIA High Definition Audio)"),
                None
            ),
            Some("CU34G2XP (NVIDIA High Definition Audio)".into())
        );
    }

    #[test]
    fn never_captures_the_virtual_mic() {
        assert!(is_virtual_capture("CABLE Output (VB-Audio Virtual Cable)"));
        assert!(!is_virtual_capture("Microphone (PRO X 2 LIGHTSPEED)"));
    }
}
