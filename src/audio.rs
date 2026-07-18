//! Core Audio engine: ChatMix by per-application session ducking.
//!
//! No virtual driver. We enumerate the audio *sessions* on the default render
//! endpoint, classify each app as Chat or Game, and set each session's volume
//! via `ISimpleAudioVolume`. One "mix" value from -1.0 (all Chat) to +1.0 (all
//! Game); 0.0 = both at full. The COM work all happens on the engine thread.

use std::collections::HashSet;
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
    OpenProcess, QueryFullProcessImageNameW, PROCESS_NAME_WIN32,
    PROCESS_QUERY_LIMITED_INFORMATION,
};
use windows::core::PWSTR;

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Group {
    Chat,
    Game,
}

/// A currently-detected audio app (deduped by exe name).
#[derive(Clone, Debug)]
pub struct AppInfo {
    pub exe: String, // lowercased basename, e.g. "discord.exe"
    #[allow(dead_code)]
    pub group: Group,
    pub active: bool,
    pub vol: f32, // dial level [0,1] for this app, for the UI meter
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

/// Commands the UI sends to the engine thread.
pub enum Cmd {
    /// mix in [-1.0, 1.0]: -1 = favor Chat, +1 = favor Game, 0 = both full.
    SetMix(f32),
    /// Full set of exe names (lowercased) treated as Chat.
    SetChat(HashSet<String>),
    #[allow(dead_code)]
    Refresh,
    Quit,
}

struct Cfg {
    chat: HashSet<String>,
    mix: f32,
}

impl Cfg {
    /// Perceptual "level" [0,1] for a group given the current mix.
    /// 1.0 = full, 0.0 = off. This is the dial *intent* (linear), which the
    /// UI shows on the level bars; the actual amplitude is tapered below.
    fn level(&self, g: Group) -> f32 {
        match g {
            // mix > 0 favors Game -> Game stays full, Chat attenuates.
            Group::Game => (1.0 + self.mix).clamp(0.0, 1.0),
            Group::Chat => (1.0 - self.mix).clamp(0.0, 1.0),
        }
    }
}

/// Floor of the fade in decibels. A dial level of 0 maps to full mute; a level
/// of 1 maps to 0 dB (unchanged). In between we fade *linearly in dB* so equal
/// dial movement produces an equal perceived-loudness change — a smooth fade
/// instead of the "nothing then sudden drop" of a linear-amplitude curve.
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

/// Start the engine thread. Returns a sender for commands.
pub fn spawn(shared: Arc<Shared>) -> Sender<Cmd> {
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

        let mut cfg = Cfg {
            chat: HashSet::new(),
            mix: 0.0,
        };

        loop {
            match rx.recv_timeout(Duration::from_millis(1000)) {
                Ok(Cmd::SetMix(m)) => cfg.mix = m.clamp(-1.0, 1.0),
                Ok(Cmd::SetChat(s)) => cfg.chat = s,
                Ok(Cmd::Refresh) => {}
                Ok(Cmd::Quit) => {
                    // Restore every session to full before we leave.
                    let restore = Cfg {
                        chat: cfg.chat.clone(),
                        mix: 0.0,
                    };
                    let _ = apply(&enumr, &restore, true);
                    break;
                }
                Err(RecvTimeoutError::Timeout) => {} // periodic refresh below
                Err(RecvTimeoutError::Disconnected) => break,
            }

            if let Ok(apps) = apply(&enumr, &cfg, false) {
                if let Ok(mut guard) = shared.apps.lock() {
                    *guard = apps;
                }
            }
        }

        CoUninitialize();
    });
    tx
}

/// Enumerate sessions, apply gains, and return the deduped app list.
/// When `force_full` is set, every session is driven to 1.0 (used on quit).
unsafe fn apply(
    enumr: &IMMDeviceEnumerator,
    cfg: &Cfg,
    force_full: bool,
) -> Result<Vec<AppInfo>> {
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
        // pid 0 is the system-sounds session; leave it untouched.
        if pid == 0 {
            continue;
        }

        let exe = match process_exe(pid) {
            Some(name) => name.to_lowercase(),
            None => continue,
        };

        let group = if cfg.chat.contains(&exe) {
            Group::Chat
        } else {
            Group::Game
        };

        let level = if force_full { 1.0 } else { cfg.level(group) };
        let amplitude = if force_full { 1.0 } else { taper(level) };

        if let Ok(vol) = ctrl.cast::<ISimpleAudioVolume>() {
            let _ = vol.SetMasterVolume(amplitude, std::ptr::null());
        }

        let active = matches!(ctrl.GetState(), Ok(s) if s != AudioSessionStateExpired);
        // Dedup: an app can own several sessions; show it once.
        if let Some(existing) = apps.iter_mut().find(|a| a.exe == exe) {
            existing.active |= active;
            existing.vol = level;
        } else {
            apps.push(AppInfo {
                exe,
                group,
                active,
                vol: level,
            });
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
    let ok = QueryFullProcessImageNameW(
        handle,
        PROCESS_NAME_WIN32,
        PWSTR(buf.as_mut_ptr()),
        &mut len,
    );
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
    use std::collections::HashSet;

    fn cfg(mix: f32) -> Cfg {
        Cfg {
            chat: HashSet::new(),
            mix,
        }
    }

    #[test]
    fn center_keeps_both_full() {
        let c = cfg(0.0);
        assert_eq!(c.level(Group::Game), 1.0);
        assert_eq!(c.level(Group::Chat), 1.0);
    }

    #[test]
    fn full_game_mutes_chat_keeps_game() {
        let c = cfg(1.0);
        assert_eq!(c.level(Group::Game), 1.0);
        assert_eq!(c.level(Group::Chat), 0.0);
    }

    #[test]
    fn full_chat_mutes_game_keeps_chat() {
        let c = cfg(-1.0);
        assert_eq!(c.level(Group::Game), 0.0);
        assert_eq!(c.level(Group::Chat), 1.0);
    }

    #[test]
    fn half_toward_game_halves_chat_only() {
        let c = cfg(0.5);
        assert_eq!(c.level(Group::Game), 1.0);
        assert_eq!(c.level(Group::Chat), 0.5);
    }

    #[test]
    fn levels_stay_in_range() {
        for &m in &[-1.0, -0.3, 0.0, 0.25, 1.0] {
            let c = cfg(m);
            for g in [Group::Game, Group::Chat] {
                let l = c.level(g);
                assert!((0.0..=1.0).contains(&l), "level {} out of range at mix {}", l, m);
            }
        }
    }

    #[test]
    fn taper_endpoints_and_floor() {
        assert_eq!(taper(1.0), 1.0);
        assert_eq!(taper(0.0), 0.0);
        assert_eq!(taper(0.0005), 0.0); // below the mute floor
    }

    #[test]
    fn taper_is_monotonic_increasing() {
        let mut prev = -1.0f32;
        let mut x = 0.0f32;
        while x <= 1.0 {
            let g = taper(x);
            assert!(g >= prev, "taper not monotonic at {} ({} < {})", x, g, prev);
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
    fn basename_extracts_exe_name() {
        assert_eq!(basename(r"C:\Program Files\Discord\Discord.exe"), "Discord.exe");
        assert_eq!(basename("/usr/bin/foo"), "foo");
        assert_eq!(basename("bare.exe"), "bare.exe");
        assert_eq!(basename(""), "");
    }
}
