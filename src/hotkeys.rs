//! Global hotkeys by polling key state every 5 ms.
//!
//! ponytail: polling instead of RegisterHotKey/hooks: it works while games have
//! focus, never swallows the key from the game, and supports mouse side buttons.
//! Ceiling: ~5 ms PTT reaction and a tiny constant CPU cost.

use crate::app::Controller;
use std::sync::Arc;
use std::time::Duration;
use windows::Win32::UI::Input::KeyboardAndMouse::GetAsyncKeyState;

const VK_SHIFT: i32 = 0x10;
const VK_CONTROL: i32 = 0x11;
const VK_SPACE: i32 = 0x20;
const VK_LEFT: i32 = 0x25;
const VK_RIGHT: i32 = 0x27;
const VK_B: i32 = 0x42;
const VK_M: i32 = 0x4D;

fn down(vk: i32) -> bool {
    // SAFETY: GetAsyncKeyState only reads global key state.
    (unsafe { GetAsyncKeyState(vk) } as u16 & 0x8000) != 0
}

/// Stable names stored in settings (v0.2 used "f8", "caps_lock", "v", ...).
pub fn vk_from_name(name: &str) -> Option<i32> {
    let name = name.trim().to_lowercase().replace(' ', "_");
    let named = match name.as_str() {
        "space" => Some(0x20),
        "caps_lock" => Some(0x14),
        "scroll_lock" => Some(0x91),
        "insert" => Some(0x2D),
        "pause" => Some(0x13),
        "tab" => Some(0x09),
        "left_alt" => Some(0xA4),
        "mouse_middle" => Some(0x04),
        "mouse_4" => Some(0x05),
        "mouse_5" => Some(0x06),
        _ => None,
    };
    if named.is_some() {
        return named;
    }
    if let Some(number) = name.strip_prefix('f').and_then(|n| n.parse::<i32>().ok()) {
        return (1..=24).contains(&number).then_some(0x6F + number);
    }
    if let Some(number) = name.strip_prefix("numpad_").and_then(|n| n.parse::<i32>().ok()) {
        return (0..=9).contains(&number).then_some(0x60 + number);
    }
    if let Some(code) = name.strip_prefix("vk_").and_then(|n| n.parse::<i32>().ok()) {
        return (1..=254).contains(&code).then_some(code);
    }
    let mut chars = name.chars();
    match (chars.next(), chars.next()) {
        (Some(c), None) if c.is_ascii_alphanumeric() => Some(c.to_ascii_uppercase() as i32),
        _ => None,
    }
}

pub fn name_from_vk(vk: i32) -> String {
    match vk {
        0x20 => "space".into(),
        0x14 => "caps_lock".into(),
        0x91 => "scroll_lock".into(),
        0x2D => "insert".into(),
        0x13 => "pause".into(),
        0x09 => "tab".into(),
        0xA4 => "left_alt".into(),
        0x04 => "mouse_middle".into(),
        0x05 => "mouse_4".into(),
        0x06 => "mouse_5".into(),
        0x70..=0x87 => format!("f{}", vk - 0x6F),
        0x60..=0x69 => format!("numpad_{}", vk - 0x60),
        0x30..=0x39 | 0x41..=0x5A => ((vk as u8) as char).to_ascii_lowercase().to_string(),
        _ => format!("vk_{vk}"),
    }
}

pub fn display_name(name: &str) -> String {
    match name {
        "mouse_4" => "Mouse 4 (back)".into(),
        "mouse_5" => "Mouse 5 (forward)".into(),
        "mouse_middle" => "Middle mouse".into(),
        other => other.replace('_', " ").to_uppercase(),
    }
}

pub fn spawn(controller: Arc<Controller>) {
    std::thread::Builder::new()
        .name("localvoice-hotkeys".into())
        .spawn(move || run(controller))
        .expect("spawn hotkey thread");
}

fn run(controller: Arc<Controller>) {
    let mut previous = [false; 256];
    let mut ptt_down = false;
    loop {
        std::thread::sleep(Duration::from_millis(5));
        let (capturing, ptt_vk) = {
            let session = controller.session();
            (session.capturing_key, vk_from_name(&session.settings.ptt_key))
        };
        // Only scan every key while capturing; otherwise poll the handful we use.
        let mut now = [false; 256];
        if capturing {
            for (vk, state) in now.iter_mut().enumerate().skip(1) {
                *state = down(vk as i32);
            }
        } else {
            for vk in [VK_SHIFT, VK_CONTROL, VK_SPACE, VK_LEFT, VK_RIGHT, VK_B, VK_M]
                .into_iter()
                .chain(ptt_vk)
            {
                now[vk as usize] = down(vk);
            }
        }
        let pressed = |vk: i32| now[vk as usize] && !previous[vk as usize];

        if capturing {
            // Skip left/right mouse (clicking the button) and bare modifiers.
            let skip = [0x01, 0x02, 0x10, 0x11, 0x12, 0xA0, 0xA1, 0xA2, 0xA3, 0x5B, 0x5C];
            if let Some(vk) = (1..255).find(|&vk| pressed(vk) && !skip.contains(&vk)) {
                controller.captured_key(name_from_vk(vk));
            }
        } else if let Some(vk) = ptt_vk {
            let is_down = now[vk as usize];
            if is_down != ptt_down {
                ptt_down = is_down;
                controller.ptt(is_down);
            }
        }

        if now[VK_CONTROL as usize] && now[VK_SHIFT as usize] {
            if pressed(VK_SPACE) {
                controller.toggle_live();
            }
            if pressed(VK_M) {
                controller.toggle_mute();
            }
            if pressed(VK_B) {
                controller.toggle_bypass();
            }
            if pressed(VK_LEFT) {
                controller.step_voice(-1);
            }
            if pressed(VK_RIGHT) {
                controller.step_voice(1);
            }
        }
        previous = now;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn key_names_round_trip_and_accept_v02_names() {
        for name in ["f8", "caps_lock", "v", "7", "space", "mouse_4", "numpad_3", "f24"] {
            let vk = vk_from_name(name).unwrap_or_else(|| panic!("{name}"));
            assert_eq!(name_from_vk(vk), name);
        }
        assert_eq!(vk_from_name("F8"), Some(0x77));
        assert_eq!(vk_from_name("Caps Lock"), Some(0x14));
        assert_eq!(vk_from_name("ctrl+v"), None);
    }
}
