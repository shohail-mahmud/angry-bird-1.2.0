//! Embeds the Angry Birds icon and a proper VERSIONINFO block into the game executable,
//! so Explorer, shortcuts and the Properties dialog show the bird instead of a blank
//! default icon. Only runs for Windows GNU targets (mingw's windres is used there);
//! other targets simply build without resources.

use std::env;
use std::path::PathBuf;
use std::process::Command;

fn main() {
    println!("cargo:rerun-if-changed=assets/angry_birds.rc");
    println!("cargo:rerun-if-changed=installer/icon.ico");

    let target_os = env::var("CARGO_CFG_TARGET_OS").unwrap_or_default();
    let target_env = env::var("CARGO_CFG_TARGET_ENV").unwrap_or_default();
    let target_arch = env::var("CARGO_CFG_TARGET_ARCH").unwrap_or_default();
    if target_os != "windows" || target_env != "gnu" {
        return;
    }

    let windres = match target_arch.as_str() {
        "x86_64" => "x86_64-w64-mingw32-windres",
        "aarch64" => "aarch64-w64-mingw32-windres",
        "i686" => "i686-w64-mingw32-windres",
        other => {
            println!("cargo:warning=no windres mapping for {other}, skipping icon");
            return;
        }
    };

    let out = PathBuf::from(env::var("OUT_DIR").expect("OUT_DIR"));
    let object = out.join("angry_birds_icon.o");
    let status = Command::new(windres)
        .arg("-I")
        .arg("assets")
        .arg("-O")
        .arg("coff")
        .arg("-o")
        .arg(&object)
        .arg("assets/angry_birds.rc")
        .status();

    match status {
        Ok(s) if s.success() => {
            println!("cargo:rustc-link-arg-bins={}", object.display());
        }
        Ok(s) => println!("cargo:warning=windres exited with {s}, building without icon"),
        Err(e) => println!("cargo:warning=windres not found ({e}), building without icon"),
    }
}
