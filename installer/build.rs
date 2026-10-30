//! Build script for the single-file installer.
//!
//! It makes sure the two binaries the installer carries are present, and decides whether the
//! original `1.2.0.ipa` should be baked in as well. That switch is what turns the installer
//! into a genuinely self-contained single file:
//!
//! * CI / public build — no `.ipa` in the tree, so `IPA = None`. The installer writes the game
//!   and the uninstaller, then asks for the archive (or finds it next to setup.exe).
//! * Private build — drop `1.2.0.ipa` in the repository root before building and it is embedded,
//!   so the one `.exe` installs a fully playable game with nothing else needed.

use std::env;
use std::fs;
use std::path::{Path, PathBuf};

fn main() {
    let manifest = PathBuf::from(env::var("CARGO_MANIFEST_DIR").expect("CARGO_MANIFEST_DIR"));
    let out = PathBuf::from(env::var("OUT_DIR").expect("OUT_DIR"));
    let payload = manifest.join("payload");

    for file in ["game.exe", "uninstall.exe"] {
        let path = payload.join(file);
        if !path.is_file() {
            panic!(
                "missing {}\n\nBuild the game first, then stage its binaries:\n  \
                 cargo build --release --no-default-features --target x86_64-pc-windows-gnu\n  \
                 mkdir -p installer/payload\n  \
                 cp target/x86_64-pc-windows-gnu/release/angry_birds_1_2.exe installer/payload/game.exe\n  \
                 cp target/x86_64-pc-windows-gnu/release/uninstall.exe installer/payload/uninstall.exe",
                path.display()
            );
        }
        println!("cargo:rerun-if-changed={}", path.display());
    }

    // An .ipa is only worth embedding if it is the real thing, not a stub.
    let mut ipa: Option<PathBuf> = None;
    for candidate in [
        manifest.join("payload").join("1.2.0.ipa"),
        manifest.join("..").join("1.2.0.ipa"),
    ] {
        if is_real_ipa(&candidate) {
            ipa = Some(candidate);
            break;
        }
    }

    let generated = match &ipa {
        Some(path) => {
            println!("cargo:rerun-if-changed={}", path.display());
            println!("cargo:rustc-env=AB_IPA_EMBEDDED=1");
            format!(
                "pub const IPA: Option<&[u8]> = Some(include_bytes!({path:?}));\n\
                 pub const IPA_BYTES: usize = {len};\n",
                path = path.to_string_lossy().replace('\\', "/"),
                len = fs::metadata(path).map(|m| m.len()).unwrap_or(0),
            )
        }
        None => {
            println!("cargo:rustc-env=AB_IPA_EMBEDDED=0");
            "pub const IPA: Option<&[u8]> = None;\npub const IPA_BYTES: usize = 0;\n".to_string()
        }
    };

    fs::write(out.join("payload.rs"), generated).expect("write generated payload.rs");
    println!("cargo:rerun-if-changed=build.rs");
    let _ = Path::new(&payload);
}

fn is_real_ipa(path: &Path) -> bool {
    fs::metadata(path)
        .map(|m| m.is_file() && m.len() > 4 * 1024 * 1024)
        .unwrap_or(false)
}
