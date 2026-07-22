//! Embeds a Windows VERSIONINFO resource in the exe, so `bilano.exe` reports its
//! version to Explorer's Details tab, `Get-Item .VersionInfo`, and any antivirus
//! or support tool that reads it. Without it the file claims no version at all,
//! and "which version are you running?" has no answer outside the app's own UI.
//!
//! Version strings come from Cargo, so `Cargo.toml` stays the single place a
//! release version is written.

fn main() {
    // Only Windows targets have a resource section; skip elsewhere so the crate
    // still `cargo check`s on other platforms.
    if std::env::var_os("CARGO_CFG_WINDOWS").is_none() {
        return;
    }

    println!("cargo:rerun-if-changed=build.rs");
    println!("cargo:rerun-if-changed=Cargo.toml");

    let mut res = winresource::WindowsResource::new();
    res.set("ProductName", "Bilano")
        .set("FileDescription", "Bilano - game vs chat volume balance")
        .set("OriginalFilename", "bilano.exe")
        .set("LegalCopyright", "Copyright (c) 2026 Tiago Neto - MIT");

    // FileVersion/ProductVersion are derived from CARGO_PKG_VERSION by winresource.
    res.compile()
        .expect("failed to compile the Windows version resource (is the Windows SDK's rc.exe available?)");
}
