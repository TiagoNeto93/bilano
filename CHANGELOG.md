# Changelog

Notable changes to Bilano, newest first. Each release on GitHub uses the section
below it as its description, so write these for the person deciding whether to
download — what changed for them, not which commits landed.

## [Unreleased]

### Added

- `bilano.exe` now carries its version, so Explorer's Details tab and
  `(Get-Item bilano.exe).VersionInfo` report it. Previously the file claimed no
  version at all.
- The version is shown in the app header and the tray tooltip, so "which version
  are you on?" is answerable without hunting for the zip you downloaded.

## [2.0.1] - 2026-07-22

The app itself is unchanged from 2.0.0 — this release exists so downloads can be
verified.

### Added

- **`SHA256SUMS.txt` is published with every release.** Bilano isn't code-signed,
  so Windows and antivirus software will warn about it. Now you can at least
  confirm the file you downloaded is the one the build produced:
  `Get-FileHash .\Bilano-v2.0.1-win64.zip -Algorithm SHA256`.
- A [security policy](SECURITY.md) covering what's in scope, how to report
  something privately, and how release builds are produced.
- Issue templates, so bug reports arrive with the details that actually decide
  whether a problem can be reproduced.

## [2.0.0] - 2026-07-21

### Changed

- **Renamed from ChatMix to Bilano** — from Latin *bilanx*, "two scale-pans", the
  root of the word *balance*. Same app, new name, first public release.

### Upgrading from 1.x

Your settings carry over. On first launch Bilano moves the old
`%APPDATA%\chatmix` config to `%APPDATA%\bilano`, keeping your tags, per-app
volumes and mutes, and repoints "Start with Windows" at the new exe.

Two things to do by hand:

- **Quit the old version first.** The two use different single-instance locks, so
  a running 1.7 won't notice 2.0 starting and both will fight over app volumes.
- **Delete the old `chatmix.exe`** once you're happy — nothing removes it for you.

## [1.7.0] - 2026-07-18

### Added

- The app list is now split into **Chat** and **Game** sections. Click a row's
  chip to move an app to the other group; it keeps its volume and mute setting.
- The window is resizable.
- "Start with Windows" now starts Bilano minimised to the tray, instead of
  opening the window on every boot.

### Changed

- Compact footer, so the app list gets the space instead.

## [1.6.0] - 2026-07-18

### Added

- **Per-app volume and mute.** Each app gets its own level that rides *under* the
  dial rather than overriding it, so one loud app can be turned down without
  disturbing the balance. Mute silences an app without losing its level.

## [1.5.0] - 2026-07-18

### Added

- Unit tests for the volume maths and config handling, run in CI on every push,
  so a broken build can't be tagged and published.

## Earlier

1.4.1 and before predate the public repository: the balance dial itself, global
hotkeys, the tray icon, and "Start with Windows".

[Unreleased]: https://github.com/TiagoNeto93/bilano/compare/v2.0.1...HEAD
[2.0.1]: https://github.com/TiagoNeto93/bilano/compare/v2.0.0...v2.0.1
[2.0.0]: https://github.com/TiagoNeto93/bilano/compare/v1.7.0...v2.0.0
[1.7.0]: https://github.com/TiagoNeto93/bilano/compare/v1.6.0...v1.7.0
[1.6.0]: https://github.com/TiagoNeto93/bilano/compare/v1.5.0...v1.6.0
[1.5.0]: https://github.com/TiagoNeto93/bilano/compare/v1.4.1...v1.5.0
