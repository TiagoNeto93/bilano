# ChatMix — project guide for Claude

A lightweight, driver-free **ChatMix** for Windows: one dial balances game audio
against voice-chat audio. A hand-built alternative to SteelSeries Sonar, without
the virtual audio driver, the install, or the bloat. Single self-contained
`.exe` (~3 MB), starts instantly, clean to remove.

Status: **working v1.7**. Personal/SFW tooling. Owner uses it and shares the exe
with friends. (Cargo `version` is kept aligned with the product/tag version.)

## What it does & how (the core idea)

Windows exposes a per-application volume for every audio *session* via Core Audio
(`ISimpleAudioVolume`). We don't need a virtual driver — we just **duck** volumes.

The full per-app gain chain (in `audio.rs::effective`):

```
app volume = taper(group_level(mix, group)) × per-app-trim × (muted ? 0 : 1)
```

- The user tags some apps as **Chat** (e.g. `discord.exe`); everything else is **Game**.
- One `mix` value in `[-1.0, +1.0]`: `-1` = all Chat, `+1` = all Game, `0` = both full.
  Moving toward Game attenuates the Chat group (and vice-versa).
- The fade is **logarithmic (dB-linear)**, floor `MIN_DB = -40 dB`, so equal dial
  movement = equal perceived-loudness change (smooth, not "sudden drop at the end").
- **Per-app trim** (v1.6): a linear `[0,1]` multiplier per app, default 1.0. It's
  **cut-only** — an app rides *under* its group level, never above it (Windows session
  volume can't amplify above an app's own 100% anyway). **Mute** forces 0 without
  losing the trim. Verified end-to-end (mute→0, trim scales, rides the dial).
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
  enumerates sessions, applies the gain chain. Reads the shared `Arc<Mutex<Config>>`;
  driven by a tiny `Sender<Cmd>` wake channel (`Apply` = re-read config & apply now,
  `Quit` = restore full & stop). UI/tray/hotkeys mutate the config then send `Apply`.
  Re-applies every ~1s anyway to catch newly-started apps.
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
# On Windows, cargo/rustc may not be on PATH in non-login shells:
$env:Path = "$env:USERPROFILE\.cargo\bin;$env:Path"
cargo build            # debug: keeps a console window (handy for panics)
cargo build --release  # release: windowless (windows_subsystem="windows"), LTO, ~3 MB
```

- `.cargo/config.toml` sets **`crt-static`** so the shipped exe needs no VC++ Redistributable.
- If cargo can't reach crates.io with a schannel revocation error
  (`CRYPT_E_NO_REVOCATION_CHECK`) — common behind TLS-inspecting proxies/AV — set
  `CARGO_HTTP_CHECK_REVOKE=false` for the build.
- If installing the toolchain via winget and the msstore source errors, pass `--source winget`.
- Release LTO build + antivirus scan can be slow; if a build command "times out" it
  usually finished — check the output; the hang is typically AV scanning the new exe.

## Running / packaging / sharing

```powershell
# always kill first — a running exe is locked and blocks rebuild
Stop-Process -Name chatmix -Force -ErrorAction SilentlyContinue
Start-Process .\target\release\chatmix.exe
```

Distribution lives in `dist/`: `chatmix.exe` + `README.txt`, zipped as
`ChatMix-vX.Y-win64.zip`. Sharing = send the zip; the user does More info → Run anyway
on SmartScreen (unsigned). **Antivirus (e.g. Norton) may block/quarantine the unsigned
exe on first run** — whitelist the folder/exe. When blocked, it can look like "won't
launch / weird leftover 1-thread process" rather than an outright error.

## Tests & CI

`cargo test` covers the **pure logic** (the part where automated tests pay off):
the gain chain (`group_level` + `taper` curve, and `effective` = mute→0, trim scales
linearly, cut-only can't exceed group, trim=1 matches dial-only), `basename`
extraction, and config defaults / `volume_of` / `is_muted` / JSON round-trip /
**old-format migration**. Tests live in `#[cfg(test)]` modules in `audio.rs` and `config.rs`.

Deliberately **not** unit-tested: the eframe UI, Core Audio session control, and the
tray/hotkey/window behavior — they need a real desktop + audio sessions and are best
verified by running the app (every real bug we hit was found that way, see below).

CI: `.github/workflows/ci.yml` runs build+test+clippy on push to `main` and on PRs.
`release.yml` also runs `cargo test` before building, so a broken tag never publishes.

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
- Per-app control (v1.6): decided **cut-only trim** (rides the dial, can't exceed group)
  over override/pin/absolute models — keeps the dial as master. Two-line rows.
- UI (v1.7): app list **sectioned by Chat/Game** (group toggled by clicking the row's
  chip; re-derived each frame so re-tagging moves an app and keeps its trim/mute).
  `TopBottomPanel::bottom` footer (Add app / Startup / Hide / Quit) so the central
  `ScrollArea` (auto_shrink false) fills a **resizable** window.
- Autostart (v1.7): the HKCU Run entry launches `chatmix.exe --tray`. **Gotcha:** eframe
  force-shows the window in `post_rendering` after the first painted frame (see
  `epi_integration.rs`), so `with_visible(false)` can't keep it hidden — instead we
  `tray::hide_window()` (Win32 SW_HIDE) over the first few frames (`hide_ticks`,
  spinning `request_repaint`) to send it to the tray with minimal flash. The Run value
  stores the exe's path at toggle-time, so moving the exe breaks autostart until re-toggled.
- Done: private GitHub repo `TiagoNeto93/chatmix` + Releases + CI (build/test/clippy on
  push, auto-build+publish on `vX.Y.Z` tag). Unit tests cover the pure logic.
- Parked for later (**v1.7 ideas from the owner**):
  - Group/section the app list by **Chat vs Game** (or sort by group) — must handle an
    app being re-tagged live (moves between sections without losing its trim/mute).
  - Roomier app list; make **"Add app"** and **"Start with Windows"** more compact.
  - **User-resizable** window / layout to taste.
  - **MIDI / hardware-knob binding** for a physical dial.
- If asked to make the fade more/less aggressive, tune `MIN_DB` in `audio.rs`.
