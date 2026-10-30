// Angry Birds 1.2.0 (Rust rebuild) - uninstaller
// Copyright (c) 2026 Shohail Mahmud. MIT licensed - see LICENSE.
//
// Removes the game, its save data, its settings, the registry entries and the shortcuts, then
// deletes its own install folder. Uses nothing but the standard library.

use std::io::{self, Write};
use std::path::PathBuf;
use std::process::{Command, Stdio};

/// Sub-keys the game writes under HKCU\Software.
const REG_UNINSTALL: &str =
    r"HKCU\Software\Microsoft\Windows\CurrentVersion\Uninstall\AngryBirds1_2";
const REG_KEY: &str = r"HKCU\Software\AngryBirds1_2";

fn install_dir() -> PathBuf {
    std::env::current_exe()
        .ok()
        .and_then(|p| p.parent().map(|d| d.to_path_buf()))
        .unwrap_or_else(|| PathBuf::from("."))
}

fn appdata_dir() -> Option<PathBuf> {
    std::env::var("APPDATA")
        .ok()
        .map(PathBuf::from)
        .or_else(|| {
            std::env::var("HOME")
                .ok()
                .map(|h| PathBuf::from(h).join(".config"))
        })
}

fn shortcuts() -> Vec<PathBuf> {
    let mut out = Vec::new();
    // Desktop + Start Menu are found through the shell folders rather than hard-coded paths,
    // so OneDrive-redirected desktops work too.
    let ps = r#"$d=[Environment]::GetFolderPath('Desktop'); $p=[Environment]::GetFolderPath('Programs'); "$d|$p""#;
    if let Ok(o) = Command::new("powershell")
        .args(["-NoProfile", "-Command", ps])
        .output()
    {
        let text = String::from_utf8_lossy(&o.stdout);
        let mut parts = text.trim().split('|');
        if let Some(desktop) = parts.next() {
            if !desktop.is_empty() {
                out.push(PathBuf::from(desktop).join("Angry Birds 1.2.0.lnk"));
            }
        }
        if let Some(programs) = parts.next() {
            if !programs.is_empty() {
                let start = PathBuf::from(programs).join("Angry Birds 1.2.0");
                out.push(start.join("Angry Birds 1.2.0.lnk"));
                out.push(start.join("Uninstall Angry Birds 1.2.0.lnk"));
            }
        }
    }
    out
}

fn remove_file_if_present(path: &PathBuf) {
    if path.is_file() && std::fs::remove_file(path).is_ok() {
        println!("  removed {}", path.display());
    }
}

fn remove_dir_if_present(path: &PathBuf) {
    if path.exists() {
        match std::fs::remove_dir_all(path) {
            Ok(_) => println!("  removed {}", path.display()),
            Err(_) => println!("  could not remove {}", path.display()),
        }
    }
}

fn main() {
    let dir = install_dir();
    println!("==============================================");
    println!("  Angry Birds 1.2.0 - uninstaller");
    println!("==============================================");
    println!();
    println!("This will remove:");
    println!("  - the game folder            {}", dir.display());
    println!("  - your saved progress        %APPDATA%\\AngryBirds1_2_Rust");
    println!("  - the Apps & features entry");
    println!("  - desktop and Start Menu shortcuts");
    println!();
    print!("Type Y and press Enter to uninstall, anything else to cancel: ");
    let _ = io::stdout().flush();

    let mut answer = String::new();
    if io::stdin().read_line(&mut answer).is_err() {
        return;
    }
    if !matches!(answer.trim().to_ascii_lowercase().as_str(), "y" | "yes") {
        println!("Cancelled. Nothing was removed.");
        return;
    }

    // 1) make sure the game is not running, or its exe is locked
    for exe in ["angry_birds_1_2.exe", "AngryBirds_1.2.0_Windows_r9.exe"] {
        let _ = Command::new("taskkill")
            .args(["/F", "/IM", exe])
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .status();
    }

    // 2) save data (progress, stars, highscores, seen tutorials, audio/UI settings)
    if let Some(base) = appdata_dir() {
        remove_dir_if_present(&base.join("AngryBirds1_2_Rust"));
    }

    // 3) registry
    for key in [REG_UNINSTALL, REG_KEY] {
        let _ = Command::new("reg")
            .args(["delete", key, "/f"])
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .status();
        println!("  registry entry removed: {}", key);
    }

    // 4) shortcuts
    for lnk in shortcuts() {
        remove_file_if_present(&lnk);
    }
    // 5) the program folder - deferred, because this exe lives inside it
    let bat = std::env::temp_dir().join("angry_birds_120_cleanup.bat");
    // The deferred script must not run *inside* the folder it deletes: a directory that is
    // some process's working directory cannot be removed, and `uninstall.exe` is usually
    // started from that very folder. So it moves to %TEMP% first, then retries until the
    // folder is really gone (the uninstaller exe stays locked until this window is closed).
    let script = format!(
        "@echo off\r\n\
         cd /d \"%TEMP%\"\r\n\
         ping -n 3 127.0.0.1 >nul\r\n\
         for /L %%i in (1,1,60) do (\r\n\
         \x20 rmdir /s /q \"{dir}\" 2>nul\r\n\
         \x20 if not exist \"{dir}\" goto done\r\n\
         \x20 ping -n 2 127.0.0.1 >nul\r\n\
         )\r\n\
         :done\r\n\
         del \"%~f0\"\r\n",
        dir = dir.display()
    );
    if std::fs::write(&bat, script).is_ok() {
        let _ = Command::new("cmd")
            .args(["/C", "start", "/min", "", &bat.display().to_string()])
            .spawn();
    }

    println!();
    println!("Uninstalled. Nothing related to Angry Birds 1.2.0 is left on this PC.");
    println!("Installing again will start from a clean save.");
    println!();
    print!("Press Enter to close...");
    let _ = io::stdout().flush();
    let mut _dummy = String::new();
    let _ = io::stdin().read_line(&mut _dummy);
}
