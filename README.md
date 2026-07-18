# ChatMix

A lightweight, **driver-free ChatMix for Windows** — one dial balances your game
audio against your voice-chat apps. Think SteelSeries Sonar's ChatMix, without the
virtual audio driver, the installer, or the bloat.

- 🪶 **Tiny** — a single self-contained `.exe` (~3 MB). No installer, no drivers, no .NET.
- 🎚️ **One dial** — slide from Game to Chat; the other side fades down, center = both full.
- 🎧 **Per-app** — tag which apps are voice chat (Discord, etc.); everything else is "game".
- ⌨️ **Global hotkeys** — adjust the mix without leaving your game.
- 🧹 **Clean** — no background driver; restores every app to full volume on quit.

## Install

1. Download the latest `ChatMix-vX.Y-win64.zip` from the [**Releases**](../../releases) page.
2. Unzip and run `chatmix.exe`.
3. The app isn't code-signed, so **Windows SmartScreen** will warn on first run —
   click **More info → Run anyway**. Your **antivirus may also flag/quarantine** an
   unknown unsigned exe; whitelist the folder if needed.

Requires 64-bit Windows 10/11. Nothing else to install.

## Usage

- **Drag the slider** toward **Game** to fade voice chat down, toward **Chat** to fade
  the game down. **Center** = both at full volume.
- In the **Apps** list, tick the apps that are voice chat. You can also add one by name.
- **Tray icon:** left-click opens the window; right-click gives a quick tag list + Quit.
- **Global hotkeys** (work in-game):
  - `Ctrl+Alt+←` — toward Chat
  - `Ctrl+Alt+→` — toward Game
  - `Ctrl+Alt+↓` — re-center
- Closing the window hides it to the tray; quit from the tray menu.
- **Start with Windows** and your settings are remembered between runs.

Settings are stored at `%APPDATA%\chatmix\config.json`.

## How it works

Windows gives every app that plays sound its own volume via the Core Audio API.
ChatMix simply **ducks** those per-app volumes: it classifies each app as Chat or
Game and, as you move the dial, attenuates the opposite group using a smooth
decibel taper. No virtual audio device is created — which is why it's a single small
exe with nothing to install. Everything is restored to full volume when you quit.

## Build from source

Requires the Rust MSVC toolchain (`rustup`) and the MSVC build tools + Windows SDK.

```powershell
cargo build --release
# output: target\release\chatmix.exe
```

The release profile links the CRT statically (`.cargo/config.toml`), so the exe has
no external runtime dependency. See [`CLAUDE.md`](CLAUDE.md) for the full architecture
notes and build gotchas.

## Roadmap

- [x] Per-app volume + mute (v1.6)
- [x] Chat/Game sections with live re-grouping; resizable window; compact footer; start-in-tray autostart (v1.7)
- [ ] MIDI / hardware-knob binding for a physical dial

## License

Personal project — all rights reserved for now.
