# ChatMix — project guide for Claude

A lightweight, driver-free **ChatMix** for Windows: one dial balances game audio
against voice-chat audio. A hand-built alternative to SteelSeries Sonar, without
the virtual audio driver, the install, or the bloat. Single self-contained
`.exe` (~3 MB), starts instantly, clean to remove.

Status: **working v1.4**. Personal/SFW tooling. Owner uses it and shares the exe
with friends.

## What it does & how (the core idea)

Windows exposes a per-application volume for every audio *session* via Core Audio
(`ISimpleAudioVolume`). We don't need a virtual driver — we just **duck** volumes:

- The user tags some apps as **Chat** (e.g. `discord.exe`); everything else is **Game**.
- One `mix` value in `[-1.0, +1.0]`: `-1` = all Chat, `+1` = all Game, `0` = both full.
- Moving toward Game attenuates the Chat sessions (and vice-versa). At center both are full.
- The fade is **logarithmic (dB-linear)**, floor `MIN_DB = -40 dB` in `audio.rs`, so
  equal dial movement = equal perceived-loudness change (smooth, not "sudden drop at the end").
- On quit the engine restores every app to full volume.

Trade-off vs. Sonar: this is ducking (it moves the apps' real Volume-Mixer levels),
not separate audio buses. Audibly identical for the ChatMix use case.

## Architecture — READ THIS BEFORE TOUCHING THREADING

**The single most important lesson:** `eframe` STOPS calling `App::update()` while
the window is hidden. Proven empirically (a per-frame heartbeat froze the exact
frame the window hid). So **anything that must work while minimized to the tray —
tray Quit, tray Show, global hotkeys, tray tagging — must NOT be handled inside
`update()`.** They silently queue and never run while the window is hidden (e.g.
mid-game). This caused two real bugs (unkillable-from-tray, dead hotkeys) before
the current design.

Threads (all long-lived; process exits via `process::exit` from the tray thread):

- **UI thread** — `main.rs`, eframe/egui window. ONLY draws the UI and handles
  in-window interaction. Reads/writes shared state. Frozen while hidden (that's fine —
  nothing to draw). Hides on window-close via `CancelClose` + Win32 `SW_HIDE`
  (NOT `ViewportCommand::Visible(false)`, which desyncs from our Win32 show/hide).
- **Audio engine thread** — `audio.rs`. Owns COM (`CoInitializeEx` MULTITHREADED),
  enumerates sessions, applies the mix. Driven by a `Sender<Cmd>` channel
  (`SetMix`, `SetChat`, `Quit`). Re-applies every ~1s to catch newly-started apps.
- **Tray/hotkey thread** — `tray.rs`. Owns the `TrayIcon` + `GlobalHotKeyManager`
  and runs its own Win32 `PeekMessageW`/`DispatchMessageW` loop, so it works
  regardless of window state. Handles: left-click → show; right-click → menu
  (menu-on-left-click disabled); menu Open/Quit/tag-checkboxes; hotkeys → adjust mix.
  Quit = `send(Cmd::Quit)` → sleep 250 ms (let engine restore) → drop `TrayIcon`
  (its `Drop` does `NIM_DELETE`, avoiding a ghost icon) → `process::exit(0)`.

Show/hide/find the window from any thread via
`FindWindowW(null, "ChatMix")` + `ShowWindow(SW_SHOW/SW_HIDE)`.
**The window title MUST stay exactly `"ChatMix"`** or find/show/hide/single-instance break.

Shared state:
- `Arc<Mutex<Config>>` — chat set + mix + autostart, the single source of truth
  (UI, tray, hotkeys all read/write it and call `Config::save()`).
- `Arc<Shared>` (`audio::Shared`) — live detected-apps snapshot the engine publishes
  and the UI/tray read.
- `Arc<AtomicBool>` quit_flag — UI "Quit" button sets it; the tray thread performs the exit.

## Module map

| File | Responsibility |
|------|----------------|
| `src/main.rs` | eframe app, UI, custom gradient slider, app rows, style/fonts, wiring |
| `src/audio.rs` | Core Audio engine, session ducking, dB taper, restore-on-quit |
| `src/tray.rs`  | Background thread: tray icon, dynamic tagging menu, hotkeys, quit/show/hide |
| `src/single.rs`| Named-mutex single instance; 2nd launch surfaces the running window |
| `src/config.rs`| `Config` (serde), `%APPDATA%\chatmix\config.json` |
| `src/icon.rs`  | Procedural anti-aliased app/tray icon (4× supersample), no bundled asset |

Hotkeys: `Ctrl+Alt+←` Chat · `Ctrl+Alt+→` Game · `Ctrl+Alt+↓` Center (step 0.1).

## UI / fonts

egui's bundled fonts lack arrow glyphs and can't render color emoji → they showed
as tofu boxes. Fixed by loading Windows' own `C:\Windows\Fonts\segoeui.ttf` +
`seguisym.ttf` at startup (real arrows, native look, zero bundled font bloat).
**Don't use color emoji** (🎧🎮) in UI text — egui renders monochrome only; use text
+ arrows (◀ ▶ ← → ↓ ↔) which Segoe provides.

## Building (Windows-specific gotchas)

Toolchain: Rust stable MSVC (installed via winget `Rustlang.Rustup`). `cargo`/`rustc`
live in `%USERPROFILE%\.cargo\bin` and are often NOT on PATH in tool shells — prepend it.

```powershell
$env:Path = "$env:USERPROFILE\.cargo\bin;$env:Path"
$env:CARGO_HTTP_CHECK_REVOKE = "false"   # REQUIRED: this machine has TLS cert interception;
                                          # without it cargo fails to reach crates.io (schannel
                                          # CRYPT_E_NO_REVOCATION_CHECK). See global memory.
cargo build            # debug: keeps a console window (handy for panics)
cargo build --release  # release: windowless (windows_subsystem="windows"), LTO, ~3 MB
```

- `.cargo/config.toml` sets **`crt-static`** so the shipped exe needs no VC++ Redistributable.
- winget on this machine: pass `--source winget` (the msstore source has a cert mismatch).
- Release LTO build + antivirus scan can be slow; if a build command "times out" it
  usually finished — check the task output; the hang is typically AV scanning the new exe.

## Running / packaging / sharing

```powershell
# always kill first — a running exe is locked and blocks rebuild
Stop-Process -Name chatmix -Force -ErrorAction SilentlyContinue
Start-Process .\target\release\chatmix.exe
```

Distribution lives in `dist/`: `chatmix.exe` + `README.txt`, zipped as
`ChatMix-vX.Y-win64.zip`. Sharing = send the zip; friend does More info → Run anyway
on SmartScreen (unsigned). **Antivirus (Norton here) may block/quarantine the unsigned
exe on first run** — whitelist the folder/exe. This looked like "won't launch / weird
leftover 1-thread process" during dev until the folder was added to Norton exceptions.

## Testing tips (no human clicks needed for most of it)

- Single-instance / show path: launch twice → exactly 1 process, no message box.
- Clean-quit path: there WAS an env-gated self-quit hook (`CHATMIX_SELFQUIT`) used to
  prove `process::exit` runs from the tray loop; removed after verifying. Re-add
  temporarily if you need to re-verify the exit path.
- "Is it really quitting?": check `Get-Process chatmix` — don't trust the tray icon
  alone (Windows keeps a ghost icon until hover if a process dies without `NIM_DELETE`;
  our clean-drop avoids that).
- A healthy instance has ~15 threads + a `ChatMix` window title; 1-thread no-window
  processes are AV-blocked corpses (clear on reboot).

## Decisions & future work

- Chosen: session-volume ducking (no driver) + Rust/windows-rs + eframe. Deliberately
  lightweight over "true separate buses."
- Parked for later: **MIDI / hardware-knob binding** for a physical dial;
  **GitHub Releases** page (repo + `.gitignore` + CI build) for nicer sharing than a raw zip.
- If asked to make the fade more/less aggressive, tune `MIN_DB` in `audio.rs`.
