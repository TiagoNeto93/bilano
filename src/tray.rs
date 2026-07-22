//! Background thread that owns the tray icon and the global hotkeys, and runs
//! its own Win32 message loop. This is deliberately independent of the eframe
//! window: eframe stops calling `update()` whenever the window is hidden, so
//! Quit / Show / hotkeys / tray-tagging must all live here to work while the
//! app is tucked in the tray (e.g. mid-game).

use std::collections::BTreeSet;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc::Sender;
use std::sync::{Arc, Mutex};
use std::thread;
use std::time::Duration;

use global_hotkey::hotkey::{Code, HotKey, Modifiers};
use global_hotkey::{GlobalHotKeyEvent, GlobalHotKeyManager, HotKeyState};
use tray_icon::menu::{CheckMenuItem, Menu, MenuEvent, MenuId, MenuItem, PredefinedMenuItem};
use tray_icon::{MouseButton, MouseButtonState, TrayIcon, TrayIconBuilder, TrayIconEvent};
use windows::core::{w, PCWSTR};
use windows::Win32::Foundation::HWND;
use windows::Win32::UI::WindowsAndMessaging::{
    DispatchMessageW, FindWindowW, PeekMessageW, SetForegroundWindow, ShowWindow, TranslateMessage,
    MSG, PM_REMOVE, SW_HIDE, SW_SHOW,
};

use crate::audio::{Cmd, Shared};
use crate::config::Config;
use crate::icon;

const STEP: f32 = 0.1;
const WINDOW_TITLE: PCWSTR = w!("Bilano");

/// Reveal the main window (find it by title — works across processes and while
/// hidden). Returns whether the window was found.
pub fn show_window() -> bool {
    unsafe {
        if let Ok(hwnd) = FindWindowW(PCWSTR::null(), WINDOW_TITLE) {
            let _ = ShowWindow(hwnd, SW_SHOW);
            let _ = SetForegroundWindow(hwnd);
            true
        } else {
            false
        }
    }
}

/// Hide the main window to the tray (Win32, so winit keeps its own state and a
/// later SW_SHOW cleanly repaints).
pub fn hide_window() {
    unsafe {
        if let Ok(hwnd) = FindWindowW(PCWSTR::null(), WINDOW_TITLE) {
            let _ = ShowWindow(hwnd, SW_HIDE);
        }
    }
}

struct MenuState {
    show: MenuId,
    quit: MenuId,
    tags: Vec<(MenuId, String)>,
    sig: String,
}

pub fn spawn(
    tx: Sender<Cmd>,
    cfg: Arc<Mutex<Config>>,
    shared: Arc<Shared>,
    quit_flag: Arc<AtomicBool>,
) {
    thread::spawn(move || unsafe {
        let mut tray = Some(
            TrayIconBuilder::new()
                // Tooltip only — the *window* title must stay exactly "Bilano",
                // since find/show/hide and single-instance match on it.
                .with_tooltip(concat!("Bilano v", env!("CARGO_PKG_VERSION"), " — game vs chat balance"))
                .with_menu_on_left_click(false) // left-click opens UI; right-click shows menu
                .with_icon(tray_icon::Icon::from_rgba(icon::rgba(32), 32, 32).expect("icon"))
                .build()
                .expect("failed to build tray icon"),
        );

        // Hotkeys are created here so this thread's message loop delivers WM_HOTKEY.
        let hk = GlobalHotKeyManager::new().expect("hotkey manager");
        let k_chat = HotKey::new(Some(Modifiers::CONTROL | Modifiers::ALT), Code::ArrowLeft);
        let k_game = HotKey::new(Some(Modifiers::CONTROL | Modifiers::ALT), Code::ArrowRight);
        let k_center = HotKey::new(Some(Modifiers::CONTROL | Modifiers::ALT), Code::ArrowDown);
        hk.register(k_chat).ok();
        hk.register(k_game).ok();
        hk.register(k_center).ok();

        let mut ms = rebuild(tray.as_ref().unwrap(), &shared, &cfg);
        let mut msg = MSG::default();

        loop {
            while PeekMessageW(&mut msg, HWND::default(), 0, 0, PM_REMOVE).as_bool() {
                let _ = TranslateMessage(&msg);
                DispatchMessageW(&msg);
            }

            let mut want_quit = quit_flag.load(Ordering::SeqCst);

            while let Ok(ev) = MenuEvent::receiver().try_recv() {
                if ev.id == ms.show {
                    show_window();
                } else if ev.id == ms.quit {
                    want_quit = true;
                } else if let Some((_, exe)) =
                    ms.tags.iter().find(|(id, _)| *id == ev.id).cloned()
                {
                    toggle_chat(&cfg, &tx, &exe);
                    ms = rebuild(tray.as_ref().unwrap(), &shared, &cfg);
                }
            }

            while let Ok(ev) = TrayIconEvent::receiver().try_recv() {
                if let TrayIconEvent::Click {
                    button: MouseButton::Left,
                    button_state: MouseButtonState::Up,
                    ..
                } = ev
                {
                    show_window();
                }
            }

            while let Ok(ev) = GlobalHotKeyEvent::receiver().try_recv() {
                if ev.state != HotKeyState::Pressed {
                    continue;
                }
                if ev.id == k_chat.id() {
                    adjust_mix(&cfg, &tx, Some(-STEP));
                } else if ev.id == k_game.id() {
                    adjust_mix(&cfg, &tx, Some(STEP));
                } else if ev.id == k_center.id() {
                    adjust_mix(&cfg, &tx, None);
                }
            }

            if want_quit {
                tx.send(Cmd::Quit).ok(); // engine restores every app to full
                thread::sleep(Duration::from_millis(250));
                drop(tray.take()); // Drop -> NIM_DELETE removes the tray icon cleanly
                std::process::exit(0);
            }

            // Keep the tray tagging menu in sync as apps start/stop.
            let sig = signature(&shared, &cfg);
            if sig != ms.sig {
                ms = rebuild(tray.as_ref().unwrap(), &shared, &cfg);
            }

            thread::sleep(Duration::from_millis(30));
        }
    });
}

fn merged_names(shared: &Arc<Shared>, cfg: &Arc<Mutex<Config>>) -> Vec<String> {
    let mut set: BTreeSet<String> = shared
        .apps
        .lock()
        .map(|g| g.iter().map(|a| a.exe.clone()).collect())
        .unwrap_or_default();
    if let Ok(c) = cfg.lock() {
        for exe in &c.chat {
            set.insert(exe.to_lowercase());
        }
    }
    set.into_iter().collect()
}

fn signature(shared: &Arc<Shared>, cfg: &Arc<Mutex<Config>>) -> String {
    let chat = cfg.lock().map(|c| c.chat_set()).unwrap_or_default();
    let mut s = String::new();
    for n in merged_names(shared, cfg) {
        s.push_str(&n);
        s.push(if chat.contains(&n) { '1' } else { '0' });
        s.push('|');
    }
    s
}

fn rebuild(tray: &TrayIcon, shared: &Arc<Shared>, cfg: &Arc<Mutex<Config>>) -> MenuState {
    let names = merged_names(shared, cfg);
    let chat = cfg.lock().map(|c| c.chat_set()).unwrap_or_default();

    let menu = Menu::new();
    let show = MenuItem::new("Open Bilano", true, None);
    menu.append(&show).ok();
    menu.append(&PredefinedMenuItem::separator()).ok();
    menu.append(&MenuItem::new("Tag voice-chat apps:", false, None))
        .ok();

    let mut tags = Vec::new();
    for exe in &names {
        let item = CheckMenuItem::new(exe.as_str(), true, chat.contains(exe), None);
        tags.push((item.id().clone(), exe.clone()));
        menu.append(&item).ok();
    }

    menu.append(&PredefinedMenuItem::separator()).ok();
    let quit = MenuItem::new("Quit", true, None);
    menu.append(&quit).ok();

    let ms = MenuState {
        show: show.id().clone(),
        quit: quit.id().clone(),
        tags,
        sig: signature(shared, cfg),
    };
    tray.set_menu(Some(Box::new(menu)));
    ms
}

fn toggle_chat(cfg: &Arc<Mutex<Config>>, tx: &Sender<Cmd>, exe: &str) {
    if let Ok(mut c) = cfg.lock() {
        let on = !c.is_chat(exe);
        c.set_chat(exe, on);
        c.save();
    }
    tx.send(Cmd::Apply).ok();
}

/// `Some(delta)` nudges the mix; `None` re-centers.
fn adjust_mix(cfg: &Arc<Mutex<Config>>, tx: &Sender<Cmd>, delta: Option<f32>) {
    if let Ok(mut c) = cfg.lock() {
        c.mix = match delta {
            Some(d) => (c.mix + d).clamp(-1.0, 1.0),
            None => 0.0,
        };
        c.save();
    }
    tx.send(Cmd::Apply).ok();
}
