//! Persistent config: which apps are "chat", and the last mix value.
//! Stored as JSON at %APPDATA%\chatmix\config.json.

use std::collections::HashSet;
use std::path::PathBuf;

use serde::{Deserialize, Serialize};

#[derive(Serialize, Deserialize, Clone)]
pub struct Config {
    /// Exe basenames (lowercased) treated as Chat.
    pub chat: Vec<String>,
    /// Last mix in [-1.0, 1.0].
    pub mix: f32,
    /// Start with Windows.
    pub autostart: bool,
}

impl Default for Config {
    fn default() -> Self {
        Config {
            chat: vec!["discord.exe".into()],
            mix: 0.0,
            autostart: false,
        }
    }
}

impl Config {
    pub fn path() -> Option<PathBuf> {
        let base = std::env::var_os("APPDATA")?;
        Some(PathBuf::from(base).join("chatmix").join("config.json"))
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

    pub fn chat_set(&self) -> HashSet<String> {
        self.chat.iter().map(|s| s.to_lowercase()).collect()
    }
}
