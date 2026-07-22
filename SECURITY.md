# Security policy

Bilano is a small desktop utility maintained by one person in their spare time.
This policy tells you what that means in practice, so you know what to expect
before you report something.

## Supported versions

Only the **latest release** is supported. There are no backports — if a fix is
needed, it ships in the next version.

## Reporting a vulnerability

Use **[private vulnerability reporting](https://github.com/TiagoNeto93/bilano/security/advisories/new)**
(the "Report a vulnerability" button on the Security tab). That opens a private
thread visible only to the maintainer.

**Please don't open a public issue for anything exploitable.** For ordinary bugs,
a public [issue](https://github.com/TiagoNeto93/bilano/issues) is exactly right.

Helpful things to include: the version, your Windows build, what an attacker
would gain, and the steps to reproduce it.

**What to expect:** this is a hobby project, so there is no response-time
guarantee. Realistically you'll hear back within a couple of weeks. If a report
is valid you'll be credited in the release notes unless you'd rather not be.

## Scope

Bilano runs locally with your own user's privileges, has no network code, no
telemetry, and no update mechanism. That makes the realistic attack surface
small, but these are in scope:

- Anything that lets another process or user **escalate privileges** or run code
  through Bilano.
- Mishandling of `%APPDATA%\bilano\config.json` — it's parsed at startup, so a
  crafted config that does more than fail to load is worth reporting.
- The **`HKCU\...\Run` autostart entry** pointing somewhere it shouldn't.
- The **release pipeline** — anything that could get a binary published that
  wasn't built from this repository's source.

Out of scope, because they're expected behaviour rather than defects:

- **SmartScreen warnings and antivirus flags.** The exe isn't code-signed (a
  certificate costs more per year than this project will ever make), and unsigned
  binaries get flagged. See below for how to verify what you downloaded instead.
- **Bilano changing other applications' volumes.** That is the entire feature —
  it adjusts per-app levels through the Windows Core Audio API and restores them
  on quit.
- Findings from an automated scanner with no accompanying explanation of impact.

## Verifying what you downloaded

Every release is built by **GitHub Actions from a version tag**, on a clean
runner, using [`.github/workflows/release.yml`](.github/workflows/release.yml).
Nothing is uploaded from a personal machine. You can open any release's build run
from the [Actions tab](https://github.com/TiagoNeto93/bilano/actions/workflows/release.yml)
and read exactly what produced the zip.

Since v2.0.1 each release also publishes a **`SHA256SUMS.txt`**. To check your
download in PowerShell:

```powershell
Get-FileHash .\Bilano-vX.Y.Z-win64.zip -Algorithm SHA256
```

Compare that against the file on the release page. If you'd rather trust nothing
at all, [build it from source](README.md#build-from-source) — it's one
`cargo build --release`.
