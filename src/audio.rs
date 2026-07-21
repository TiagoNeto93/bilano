//! Core Audio engine: game/chat balance by per-application session ducking.
//!
//! No virtual driver. We enumerate the audio *sessions* on the default render
//! endpoint and set each session's volume via `ISimpleAudioVolume`. Each app's
//! final volume is a gain chain:
//!
//!   effective = taper(group_level(mix)) * per-app-trim * (muted ? 0 : 1)
//!
//! The engine reads the shared `Config` and re-applies on an `Apply` wake (sent
//! whenever the UI/tray/hotkeys change something) or once a second to catch
//! newly-started apps. All COM work happens on the engine thread.

use std::collections::{HashMap, HashSet};
use std::sync::mpsc::{channel, RecvTimeoutError, Sender};
use std::sync::{Arc, Mutex};
use std::thread;
use std::time::Duration;

use windows::core::{Interface, Result};
use windows::Win32::Foundation::CloseHandle;
use windows::Win32::Media::Audio::{
    eConsole, eRender, AudioSessionStateExpired, IAudioSessionControl2, IAudioSessionManager2,
    IMMDeviceEnumerator, ISimpleAudioVolume, MMDeviceEnumerator,
};
use windows::Win32::System::Com::{
    CoCreateInstance, CoInitializeEx, CoUninitialize, CLSCTX_ALL, COINIT_MULTITHREADED,
};
use windows::Win32::System::Threading::{
    OpenProcess, QueryFullProcessImageNameW, PROCESS_NAME_WIN32, PROCESS_QUERY_LIMITED_INFORMATION,
};
use windows::core::PWSTR;

use crate::config::Config;

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Group {
    Chat,
    Game,
}

/// A currently-detected audio app (deduped by exe name).
#[derive(Clone, Debug)]
pub struct AppInfo {
    pub exe: String, // lowercased basename, e.g. "discord.exe"
    pub active: bool,
}

/// Shared read-side state the UI can observe.
pub struct Shared {
    pub apps: Mutex<Vec<AppInfo>>,
}

impl Shared {
    pub fn new() -> Arc<Self> {
        Arc::new(Shared {
            apps: Mutex::new(Vec::new()),
        })
    }
}

/// Commands the UI/tray send to the engine thread.
pub enum Cmd {
    /// Re-read the config and apply immediately.
    Apply,
    /// Restore every app to full volume and stop.
    Quit,
}

/// Perceptual "level" [0,1] for a group at a given mix. 1 = full, 0 = off.
fn group_level(mix: f32, g: Group) -> f32 {
    match g {
        // mix > 0 favors Game -> Game stays full, Chat attenuates.
        Group::Game => (1.0 + mix).clamp(0.0, 1.0),
        Group::Chat => (1.0 - mix).clamp(0.0, 1.0),
    }
}

/// Floor of the dial fade in decibels; equal dial movement = equal perceived
/// loudness change (a smooth fade instead of a sudden drop near the end).
const MIN_DB: f32 = -40.0;

/// Map a perceptual level [0,1] to a linear amplitude [0,1] with a dB taper.
fn taper(level: f32) -> f32 {
    if level <= 0.001 {
        0.0
    } else if level >= 0.999 {
        1.0
    } else {
        let db = MIN_DB * (1.0 - level);
        10f32.powf(db / 20.0)
    }
}

/// The full per-app gain chain: dial group level (tapered) x per-app trim,
/// forced to 0 when muted. `trim` is a gentle linear multiplier [0,1].
fn effective(mix: f32, g: Group, trim: f32, muted: bool) -> f32 {
    if muted {
        return 0.0;
    }
    taper(group_level(mix, g)) * trim.clamp(0.0, 1.0)
}

/// A cheap snapshot of the config, taken under lock and used lock-free while the
/// (slower) COM enumeration runs.
struct Snapshot {
    mix: f32,
    chat: HashSet<String>,
    volume: HashMap<String, f32>,
    muted: HashSet<String>,
}

impl Snapshot {
    fn of(cfg: &Config) -> Self {
        Snapshot {
            mix: cfg.mix,
            chat: cfg.chat_set(),
            volume: cfg.volume.clone(),
            muted: cfg.muted_set(),
        }
    }
}

/// Start the engine thread. Returns a sender for wake/quit commands.
pub fn spawn(shared: Arc<Shared>, cfg: Arc<Mutex<Config>>) -> Sender<Cmd> {
    let (tx, rx) = channel::<Cmd>();
    thread::spawn(move || unsafe {
        let _ = CoInitializeEx(None, COINIT_MULTITHREADED);
        let enumr: IMMDeviceEnumerator =
            match CoCreateInstance(&MMDeviceEnumerator, None, CLSCTX_ALL) {
                Ok(e) => e,
                Err(_) => {
                    CoUninitialize();
                    return;
                }
            };

        loop {
            let quitting = match rx.recv_timeout(Duration::from_millis(1000)) {
                Ok(Cmd::Apply) => false,
                Ok(Cmd::Quit) => true,
                Err(RecvTimeoutError::Timeout) => false,
                Err(RecvTimeoutError::Disconnected) => break,
            };

            let snap = match cfg.lock() {
                Ok(c) => Snapshot::of(&c),
                Err(_) => continue,
            };

            if let Ok(apps) = apply(&enumr, &snap, quitting) {
                if let Ok(mut guard) = shared.apps.lock() {
                    *guard = apps;
                }
            }

            if quitting {
                break;
            }
        }

        CoUninitialize();
    });
    tx
}

/// Enumerate sessions, apply the gain chain, and return the deduped app list.
/// When `force_full` is set, every session is driven to 1.0 (used on quit).
unsafe fn apply(enumr: &IMMDeviceEnumerator, snap: &Snapshot, force_full: bool) -> Result<Vec<AppInfo>> {
    let device = enumr.GetDefaultAudioEndpoint(eRender, eConsole)?;
    let manager: IAudioSessionManager2 = device.Activate(CLSCTX_ALL, None)?;
    let sessions = manager.GetSessionEnumerator()?;
    let count = sessions.GetCount()?;

    let mut apps: Vec<AppInfo> = Vec::new();

    for i in 0..count {
        let ctrl = match sessions.GetSession(i) {
            Ok(c) => c,
            Err(_) => continue,
        };
        let ctrl2: IAudioSessionControl2 = match ctrl.cast() {
            Ok(c) => c,
            Err(_) => continue,
        };

        let expired = matches!(ctrl.GetState(), Ok(s) if s == AudioSessionStateExpired);
        if expired {
            continue;
        }

        let pid = ctrl2.GetProcessId().unwrap_or(0);
        if pid == 0 {
            continue; // system-sounds session; leave it alone
        }

        let exe = match process_exe(pid) {
            Some(name) => name.to_lowercase(),
            None => continue,
        };

        let amplitude = if force_full {
            1.0
        } else {
            let group = if snap.chat.contains(&exe) {
                Group::Chat
            } else {
                Group::Game
            };
            let trim = snap.volume.get(&exe).copied().unwrap_or(1.0);
            let muted = snap.muted.contains(&exe);
            effective(snap.mix, group, trim, muted)
        };

        if let Ok(vol) = ctrl.cast::<ISimpleAudioVolume>() {
            let _ = vol.SetMasterVolume(amplitude, std::ptr::null());
        }

        let active = matches!(ctrl.GetState(), Ok(s) if s != AudioSessionStateExpired);
        if let Some(existing) = apps.iter_mut().find(|a| a.exe == exe) {
            existing.active |= active;
        } else {
            apps.push(AppInfo { exe, active });
        }
    }

    apps.sort_by(|a, b| a.exe.cmp(&b.exe));
    Ok(apps)
}

/// Resolve a PID to its executable basename (e.g. "Discord.exe").
unsafe fn process_exe(pid: u32) -> Option<String> {
    let handle = OpenProcess(PROCESS_QUERY_LIMITED_INFORMATION, false, pid).ok()?;
    let mut buf = [0u16; 260];
    let mut len = buf.len() as u32;
    let ok = QueryFullProcessImageNameW(handle, PROCESS_NAME_WIN32, PWSTR(buf.as_mut_ptr()), &mut len);
    let _ = CloseHandle(handle);
    ok.ok()?;
    let full = String::from_utf16_lossy(&buf[..len as usize]);
    Some(basename(&full).to_string())
}

/// Last path component (executable file name) of a Windows or Unix-style path.
fn basename(path: &str) -> &str {
    path.rsplit(['\\', '/']).next().unwrap_or(path)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn group_level_center_and_extremes() {
        assert_eq!(group_level(0.0, Group::Game), 1.0);
        assert_eq!(group_level(0.0, Group::Chat), 1.0);
        assert_eq!(group_level(1.0, Group::Game), 1.0);
        assert_eq!(group_level(1.0, Group::Chat), 0.0);
        assert_eq!(group_level(-1.0, Group::Game), 0.0);
        assert_eq!(group_level(-1.0, Group::Chat), 1.0);
        assert_eq!(group_level(0.5, Group::Chat), 0.5);
    }

    #[test]
    fn taper_endpoints_and_floor() {
        assert_eq!(taper(1.0), 1.0);
        assert_eq!(taper(0.0), 0.0);
        assert_eq!(taper(0.0005), 0.0);
    }

    #[test]
    fn taper_is_monotonic_increasing() {
        let mut prev = -1.0f32;
        let mut x = 0.0f32;
        while x <= 1.0 {
            let g = taper(x);
            assert!(g >= prev, "taper not monotonic at {}", x);
            prev = g;
            x += 0.05;
        }
    }

    #[test]
    fn taper_midpoint_matches_db_formula() {
        let expected = 10f32.powf(MIN_DB * 0.5 / 20.0);
        assert!((taper(0.5) - expected).abs() < 1e-6);
    }

    #[test]
    fn effective_full_trim_matches_dial_only() {
        // trim=1.0, unmuted -> identical to the old group-only behavior.
        assert_eq!(effective(0.0, Group::Game, 1.0, false), 1.0);
        assert_eq!(effective(1.0, Group::Chat, 1.0, false), 0.0);
        assert!((effective(0.0, Group::Game, 1.0, false) - taper(group_level(0.0, Group::Game))).abs() < 1e-6);
    }

    #[test]
    fn effective_trim_scales_linearly_and_cannot_exceed_group() {
        // Cut-only: trim scales the (tapered) group level down, never above it.
        let full = effective(0.0, Group::Game, 1.0, false); // = 1.0 at center
        let half = effective(0.0, Group::Game, 0.5, false);
        assert!((half - full * 0.5).abs() < 1e-6);
        assert!(half <= full);
    }

    #[test]
    fn muted_is_always_silent() {
        assert_eq!(effective(0.0, Group::Game, 1.0, true), 0.0);
        assert_eq!(effective(-1.0, Group::Chat, 1.0, true), 0.0);
    }

    #[test]
    fn basename_extracts_exe_name() {
        assert_eq!(basename(r"C:\Program Files\Discord\Discord.exe"), "Discord.exe");
        assert_eq!(basename("/usr/bin/foo"), "foo");
        assert_eq!(basename("bare.exe"), "bare.exe");
        assert_eq!(basename(""), "");
    }
}
