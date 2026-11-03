#![cfg_attr(not(test), windows_subsystem = "windows")]

mod app;
mod devices;
mod dsp;
mod engine;
mod hotkeys;
mod soak;
mod ui;
mod voices;

use app::Controller;
use eframe::egui;
use std::sync::Arc;
use std::sync::mpsc::{Receiver, Sender};
use tray_icon::menu::{Menu, MenuEvent, MenuItem, PredefinedMenuItem};
use tray_icon::{MouseButton, TrayIcon, TrayIconBuilder, TrayIconEvent};
use windows::Win32::Foundation::{CloseHandle, ERROR_ALREADY_EXISTS, GetLastError, HWND};
use windows::Win32::System::Threading::{
    CreateMutexW, OpenMutexW, OpenProcess, PROCESS_NAME_WIN32, PROCESS_QUERY_LIMITED_INFORMATION,
    QueryFullProcessImageNameW, SYNCHRONIZATION_SYNCHRONIZE,
};
use windows::Win32::UI::WindowsAndMessaging::{
    FindWindowExW, GetWindowThreadProcessId, MB_ICONWARNING, MB_OK, MessageBoxW, SW_RESTORE,
    SW_SHOW, SetForegroundWindow, ShowWindow,
};
use windows::core::{PWSTR, w};

const TITLE: &str = "LocalVoice";

enum TrayAction {
    Show,
    Quit,
}

struct Shell {
    ui: ui::Ui,
    controller: Arc<Controller>,
    tray_actions: Receiver<TrayAction>,
    _tray: Option<TrayIcon>,
    quitting: bool,
}

impl eframe::App for Shell {
    fn logic(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        while let Ok(action) = self.tray_actions.try_recv() {
            match action {
                TrayAction::Show => {
                    ctx.send_viewport_cmd(egui::ViewportCommand::Visible(true));
                    ctx.send_viewport_cmd(egui::ViewportCommand::Minimized(false));
                    ctx.send_viewport_cmd(egui::ViewportCommand::Focus);
                }
                TrayAction::Quit => {
                    self.quitting = true;
                    ctx.send_viewport_cmd(egui::ViewportCommand::Visible(true));
                    ctx.send_viewport_cmd(egui::ViewportCommand::Close);
                }
            }
        }
        if ctx.input(|i| i.viewport().close_requested()) && !self.quitting {
            let to_tray = self.controller.session().settings.close_to_tray && self._tray.is_some();
            if to_tray {
                ctx.send_viewport_cmd(egui::ViewportCommand::CancelClose);
                ctx.send_viewport_cmd(egui::ViewportCommand::Visible(false));
            }
        }
    }

    fn ui(&mut self, ui: &mut egui::Ui, _frame: &mut eframe::Frame) {
        self.ui.show(ui);
    }

    fn on_exit(&mut self, _gl: Option<&eframe::glow::Context>) {
        self.controller.quit();
    }
}

/// Teal circle with a dark microphone — drawn in code so there is no asset file.
fn icon_rgba(size: u32) -> Vec<u8> {
    let mut pixels = vec![0u8; (size * size * 4) as usize];
    let s = size as f32;
    for y in 0..size {
        for x in 0..size {
            let (fx, fy) = ((x as f32 + 0.5) / s, (y as f32 + 0.5) / s);
            let circle = ((fx - 0.5).powi(2) + (fy - 0.5).powi(2)).sqrt() < 0.48;
            let capsule = (fx - 0.5).abs() < 0.11 && fy > 0.2 && fy < 0.56
                || ((fx - 0.5).powi(2) + (fy - 0.2).powi(2)).sqrt() < 0.11
                || ((fx - 0.5).powi(2) + (fy - 0.56).powi(2)).sqrt() < 0.11;
            let ring = {
                let d = ((fx - 0.5).powi(2) + (fy - 0.5).powi(2)).sqrt();
                (0.19..0.24).contains(&d) && fy > 0.5
            };
            let stem = (fx - 0.5).abs() < 0.03 && (0.72..0.82).contains(&fy);
            let i = ((y * size + x) * 4) as usize;
            let color = if !circle {
                [0, 0, 0, 0]
            } else if capsule || ring || stem {
                [20, 22, 28, 255]
            } else {
                [90, 210, 170, 255]
            };
            pixels[i..i + 4].copy_from_slice(&color);
        }
    }
    pixels
}

fn build_tray(
    actions: Sender<TrayAction>,
    controller: Arc<Controller>,
    ctx: egui::Context,
) -> Option<TrayIcon> {
    let open = MenuItem::with_id("open", "Open LocalVoice", true, None);
    let live = MenuItem::with_id("live", "Go live / stop", true, None);
    let mute = MenuItem::with_id("mute", "Mute / unmute", true, None);
    let quit = MenuItem::with_id("quit", "Quit", true, None);
    let menu =
        Menu::with_items(&[&open, &live, &mute, &PredefinedMenuItem::separator(), &quit]).ok()?;
    let icon = tray_icon::Icon::from_rgba(icon_rgba(32), 32, 32).ok()?;

    let menu_actions = actions.clone();
    let menu_ctx = ctx.clone();
    MenuEvent::set_event_handler(Some(move |event: MenuEvent| {
        match event.id.0.as_str() {
            "open" => {
                let _ = menu_actions.send(TrayAction::Show);
            }
            "live" => controller.toggle_live(),
            "mute" => controller.toggle_mute(),
            "quit" => {
                let _ = menu_actions.send(TrayAction::Quit);
            }
            _ => {}
        }
        menu_ctx.request_repaint();
    }));
    TrayIconEvent::set_event_handler(Some(move |event: TrayIconEvent| {
        if let TrayIconEvent::DoubleClick {
            button: MouseButton::Left,
            ..
        }
        | TrayIconEvent::Click {
            button: MouseButton::Left,
            button_state: tray_icon::MouseButtonState::Up,
            ..
        } = event
        {
            let _ = actions.send(TrayAction::Show);
            ctx.request_repaint();
        }
    }));
    TrayIconBuilder::new()
        .with_menu(Box::new(menu))
        .with_menu_on_left_click(false)
        .with_tooltip(TITLE)
        .with_icon(icon)
        .build()
        .ok()
}

/// The top-level "LocalVoice" window owned by a v0.3+ LocalVoice.exe (not the old v0.2 app,
/// which uses the same title).
fn existing_window() -> Option<HWND> {
    let mut after: Option<HWND> = None;
    // SAFETY: read-only window and process queries.
    unsafe {
        while let Ok(window) = FindWindowExW(None, after, None, w!("LocalVoice")) {
            after = Some(window);
            let mut pid = 0u32;
            GetWindowThreadProcessId(window, Some(&mut pid));
            let Ok(process) = OpenProcess(PROCESS_QUERY_LIMITED_INFORMATION, false, pid) else {
                continue;
            };
            let mut buffer = [0u16; 512];
            let mut length = buffer.len() as u32;
            let ok = QueryFullProcessImageNameW(
                process,
                PROCESS_NAME_WIN32,
                PWSTR(buffer.as_mut_ptr()),
                &mut length,
            )
            .is_ok();
            let _ = CloseHandle(process);
            let path = String::from_utf16_lossy(&buffer[..length as usize]).to_lowercase();
            if ok && path.ends_with(r"\localvoice.exe") {
                return Some(window);
            }
        }
    }
    None
}

/// Returns false (after focusing the existing window) if LocalVoice is already running.
fn single_instance() -> bool {
    // SAFETY: plain Win32 calls; the mutex handle intentionally lives until exit.
    unsafe {
        if OpenMutexW(
            SYNCHRONIZATION_SYNCHRONIZE,
            false,
            w!("LocalVoice.SingleInstance"),
        )
        .is_ok()
        {
            MessageBoxW(
                None,
                w!(
                    "The old LocalVoice (v0.2) is still running and uses the same hotkeys.\n\nRight-click its tray icon and choose Quit, or it will fight this version for Ctrl+Shift shortcuts."
                ),
                w!("LocalVoice"),
                MB_OK | MB_ICONWARNING,
            );
        }
        let _mutex = CreateMutexW(None, false, w!("LocalVoice.v3.SingleInstance"));
        if GetLastError() != ERROR_ALREADY_EXISTS {
            return true;
        }
        if let Some(window) = existing_window() {
            let _ = ShowWindow(window, SW_SHOW);
            let _ = ShowWindow(window, SW_RESTORE);
            let _ = SetForegroundWindow(window);
        }
        false
    }
}

fn main() -> eframe::Result {
    let args: Vec<String> = std::env::args().collect();
    if args.iter().any(|a| a == "--self-check") {
        std::process::exit(soak::self_check());
    }
    if let Some(index) = args.iter().position(|a| a == "--soak") {
        let seconds = args
            .get(index + 1)
            .and_then(|s| s.parse().ok())
            .unwrap_or(30);
        std::process::exit(soak::run(seconds));
    }
    if !single_instance() {
        return Ok(());
    }
    let options = eframe::NativeOptions {
        viewport: egui::ViewportBuilder::default()
            .with_title(TITLE)
            .with_inner_size([1020.0, 720.0])
            .with_min_inner_size([860.0, 600.0])
            .with_icon(egui::IconData {
                rgba: icon_rgba(64),
                width: 64,
                height: 64,
            }),
        persist_window: false,
        ..Default::default()
    };
    eframe::run_native(
        TITLE,
        options,
        Box::new(|cc| {
            ui::Ui::style(&cc.egui_ctx);
            let ctx = cc.egui_ctx.clone();
            let controller = Controller::new(Arc::new(move || ctx.request_repaint()));
            hotkeys::spawn(controller.clone());
            let (actions, tray_actions) = std::sync::mpsc::channel();
            let tray = build_tray(actions, controller.clone(), cc.egui_ctx.clone());
            if controller.session().settings.auto_live {
                controller.go_live();
            }
            Ok(Box::new(Shell {
                ui: ui::Ui::new(controller.clone()),
                controller,
                tray_actions,
                _tray: tray,
                quitting: false,
            }))
        }),
    )
}
