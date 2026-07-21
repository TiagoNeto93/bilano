//! Persistent config: group membership (chat), per-app volume trims and mutes,
//! the mix, and autostart. Stored as JSON at %APPDATA%\bilano\config.json.

use std::collections::{HashMap, HashSet};
use std::path::PathBuf;

use serde::{Deserialize, Serialize};

const APP_DIR: &str = "bilano";
/// Pre-2.0, when the app was called ChatMix. Only read once, by `migrate_legacy`.
const LEGACY_APP_DIR: &str = "chatmix";

#[derive(Serialize, Deserialize, Clone)]
pub struct Config {
    /// Exe basenames (lowercased) treated as Chat; everything else is Game.
    pub chat: Vec<String>,
    /// Per-app volume trim, exe(lowercased) -> [0.0, 1.0]. Absent = 1.0 (full).
    /// Only non-default trims are stored, so old configs load unchanged.
    #[serde(default)]
    pub volume: HashMap<String, f32>,
    /// Exe basenames (lowercased) that are muted.
    #[serde(default)]
    pub muted: Vec<String>,
    /// Last mix in [-1.0, 1.0].
    pub mix: f32,
    /// Start with Windows.
    pub autostart: bool,
}

impl Default for Config {
    fn default() -> Self {
        Config {
            chat: vec!["discord.exe".into()],
            volume: HashMap::new(),
            muted: Vec::new(),
            mix: 0.0,
            autostart: false,
        }
    }
}

fn appdata_dir(name: &str) -> Option<PathBuf> {
    let base = std::env::var_os("APPDATA")?;
    Some(PathBuf::from(base).join(name))
}

impl Config {
    pub fn path() -> Option<PathBuf> {
        Some(appdata_dir(APP_DIR)?.join("config.json"))
    }

    /// One-shot migration of a pre-2.0 ChatMix config into `%APPDATA%\bilano`.
    /// Without it the rename would silently reset every tag, trim and mute.
    ///
    /// Returns whether a legacy config was actually migrated — the caller uses
    /// that to also fix up the autostart entry, which still points at the old
    /// exe name. The legacy directory is removed so this only ever runs once.
    /// Call before `load()`.
    pub fn migrate_legacy() -> bool {
        let (Some(new), Some(legacy_dir)) = (Self::path(), appdata_dir(LEGACY_APP_DIR)) else {
            return false;
        };
        let legacy = legacy_dir.join("config.json");
        if new.exists() || !legacy.exists() {
            return false;
        }
        let Some(dir) = new.parent() else { return false };
        if std::fs::create_dir_all(dir).is_err() || std::fs::copy(&legacy, &new).is_err() {
            return false; // leave the old config alone so a later run can retry
        }
        let _ = std::fs::remove_dir_all(&legacy_dir);
        true
    }

    pub fn load() -> Config {
        Self::path()
            .and_then(|p| std::fs::read_to_string(p).ok())
            .and_then(|s| serde_json::from_str(&s).ok())
            .unwrap_or_default()
    }

    pub fn save(&self) {
        if let Some(p) = Self::path() {
            if let Some(dir) = p.parent() {
                let _ = std::fs::create_dir_all(dir);
            }
            if let Ok(s) = serde_json::to_string_pretty(self) {
                let _ = std::fs::write(p, s);
            }
        }
    }

    // --- group membership ---

    pub fn chat_set(&self) -> HashSet<String> {
        self.chat.iter().map(|s| s.to_lowercase()).collect()
    }

    pub fn is_chat(&self, exe: &str) -> bool {
        self.chat.iter().any(|c| c.eq_ignore_ascii_case(exe))
    }

    pub fn set_chat(&mut self, exe: &str, on: bool) {
        let e = exe.to_lowercase();
        let has = self.is_chat(&e);
        if on && !has {
            self.chat.push(e);
        } else if !on && has {
            self.chat.retain(|c| !c.eq_ignore_ascii_case(&e));
        }
    }

    // --- per-app volume trim ---

    pub fn volume_of(&self, exe: &str) -> f32 {
        self.volume
            .get(&exe.to_lowercase())
            .copied()
            .unwrap_or(1.0)
            .clamp(0.0, 1.0)
    }

    pub fn set_volume(&mut self, exe: &str, v: f32) {
        let key = exe.to_lowercase();
        let v = v.clamp(0.0, 1.0);
        if v >= 0.999 {
            self.volume.remove(&key); // default (full) — don't persist
        } else {
            self.volume.insert(key, v);
        }
    }

    // --- per-app mute ---

    pub fn is_muted(&self, exe: &str) -> bool {
        self.muted.iter().any(|m| m.eq_ignore_ascii_case(exe))
    }

    pub fn muted_set(&self) -> HashSet<String> {
        self.muted.iter().map(|s| s.to_lowercase()).collect()
    }

    pub fn set_muted(&mut self, exe: &str, on: bool) {
        let e = exe.to_lowercase();
        self.muted.retain(|m| !m.eq_ignore_ascii_case(&e));
        if on {
            self.muted.push(e);
        }
    }

    /// Every exe that has any saved setting (for showing not-currently-running apps).
    pub fn known_apps(&self) -> HashSet<String> {
        let mut s = self.chat_set();
        s.extend(self.volume.keys().cloned());
        s.extend(self.muted_set());
        s
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn default_is_centered_with_discord() {
        let c = Config::default();
        assert_eq!(c.mix, 0.0);
        assert!(!c.autostart);
        assert!(c.chat.iter().any(|x| x == "discord.exe"));
        assert!(c.volume.is_empty());
        assert!(c.muted.is_empty());
    }

    #[test]
    fn chat_set_lowercases_and_dedups() {
        let mut c = Config::default();
        c.chat = vec!["Discord.EXE".into(), "discord.exe".into(), "Steam.exe".into()];
        let s = c.chat_set();
        assert!(s.contains("discord.exe"));
        assert!(s.contains("steam.exe"));
        assert_eq!(s.len(), 2);
    }

    #[test]
    fn volume_defaults_to_full_and_is_case_insensitive() {
        let mut c = Config::default();
        assert_eq!(c.volume_of("game.exe"), 1.0);
        c.set_volume("Game.exe", 0.5);
        assert_eq!(c.volume_of("game.exe"), 0.5); // stored lowercased
        // setting back to full removes the key (keeps config minimal)
        c.set_volume("game.exe", 1.0);
        assert_eq!(c.volume_of("game.exe"), 1.0);
        assert!(c.volume.is_empty());
    }

    #[test]
    fn mute_toggles_case_insensitively() {
        let mut c = Config::default();
        assert!(!c.is_muted("spotify.exe"));
        c.set_muted("Spotify.exe", true);
        assert!(c.is_muted("spotify.exe"));
        c.set_muted("spotify.exe", false);
        assert!(!c.is_muted("spotify.exe"));
    }

    #[test]
    fn old_format_json_migrates() {
        // A pre-v1.6 config with no volume/muted fields must still load.
        let json = r#"{"chat":["discord.exe"],"mix":-0.2,"autostart":true}"#;
        let c: Config = serde_json::from_str(json).unwrap();
        assert!(c.is_chat("discord.exe"));
        assert!((c.mix + 0.2).abs() < 1e-6);
        assert!(c.autostart);
        assert!(c.volume.is_empty());
        assert!(c.muted.is_empty());
        assert_eq!(c.volume_of("discord.exe"), 1.0);
    }

    #[test]
    fn json_round_trips_with_new_fields() {
        let mut c = Config::default();
        c.mix = -0.3;
        c.set_volume("game.exe", 0.4);
        c.set_muted("spotify.exe", true);
        let json = serde_json::to_string(&c).unwrap();
        let back: Config = serde_json::from_str(&json).unwrap();
        assert_eq!(back.volume_of("game.exe"), 0.4);
        assert!(back.is_muted("spotify.exe"));
        assert!((back.mix - c.mix).abs() < 1e-6);
    }
}
