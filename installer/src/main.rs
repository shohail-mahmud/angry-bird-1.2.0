// Angry Birds 1.2.0 (Rust rebuild) - single-file Windows installer
// Copyright (c) 2026 Shohail Mahmud. MIT licensed - see LICENSE.
//
// One .exe that does the whole job:
//   * the game binary, the uninstaller and this readme are embedded in the installer;
//   * running it copies everything to %LOCALAPPDATA%\AngryBirds1_2, creates the Desktop and
//     Start Menu shortcuts and registers the app in Settings -> Apps;
//   * the original 1.2.0.ipa is embedded too when the installer was built next to it,
//     otherwise it is picked up from beside setup.exe or asked for once.
//
// Standard library only, so it cross-compiles to Windows with nothing but mingw.

use std::fs;
use std::io::{self, Write};
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};

/// The game itself, baked in at build time.
const GAME: &[u8] = include_bytes!(concat!(env!("CARGO_MANIFEST_DIR"), "/payload/game.exe"));
/// The standalone uninstaller, baked in at build time.
const UNINSTALLER: &[u8] = include_bytes!(concat!(
    env!("CARGO_MANIFEST_DIR"),
    "/payload/uninstall.exe"
));
/// Quick-start notes written next to the installed game.
const README: &str = include_str!(concat!(env!("CARGO_MANIFEST_DIR"), "/../README.txt"));
// `IPA` is Some(..) only in a private build that had the archive available (see build.rs).
include!(concat!(env!("OUT_DIR"), "/payload.rs"));

const GAME_EXE: &str = "AngryBirds_1.2.0.exe";
const UNINSTALL_EXE: &str = "uninstall.exe";
const IPA_FILE: &str = "1.2.0.ipa";
const FOLDER: &str = "AngryBirds1_2";

const REG_UNINSTALL: &str =
    r"HKCU\Software\Microsoft\Windows\CurrentVersion\Uninstall\AngryBirds1_2";
const REG_KEY: &str = r"HKCU\Software\AngryBirds1_2";

fn main() {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let silent = args
        .iter()
        .any(|a| a == "--silent" || a.eq_ignore_ascii_case("/s") || a == "-s");
    let no_launch = silent || args.iter().any(|a| a == "--no-launch");

    banner();
    let dir = install_dir();
    println!("  Installing to  {}", dir.display());
    println!();

    if let Err(e) = fs::create_dir_all(&dir) {
        fail(&format!("could not create {}: {e}", dir.display()));
    }

    // 1) the binaries that make up the installation
    if let Err(e) = write_file(&dir.join(GAME_EXE), GAME) {
        fail(&format!("could not write the game: {e}"));
    }
    println!("  [ok] game            {GAME_EXE}");
    if let Err(e) = write_file(&dir.join(UNINSTALL_EXE), UNINSTALLER) {
        fail(&format!("could not write the uninstaller: {e}"));
    }
    println!("  [ok] uninstaller     {UNINSTALL_EXE}");
    let _ = write_file(&dir.join("README.txt"), README.as_bytes());

    // 2) the original archive, the only thing that cannot be shipped for licensing reasons
    match install_ipa(&dir, silent) {
        Ipa::Copied(how) => println!("  [ok] game assets     {IPA_FILE} ({how})"),
        Ipa::Missing => {
            println!("  [!!] game assets     {IPA_FILE} not found - see the note below")
        }
    }

    // 3) shortcuts, then the entry in Settings -> Apps
    match shortcuts(&dir) {
        Ok(n) => println!("  [ok] shortcuts       {n} created"),
        Err(_) => println!("  [!!] shortcuts       could not be created"),
    }
    let reg_ok = registry(&dir);
    println!(
        "  [{}] Apps & features {}",
        if reg_ok { "ok" } else { "!!" },
        if reg_ok {
            "entry written"
        } else {
            "entry could not be written"
        }
    );

    println!();
    println!("  Installed to {}", dir.display());
    if !dir.join(IPA_FILE).is_file() {
        println!();
        println!("  NOTE: the game needs the original {IPA_FILE} (Rovio's app).");
        println!("        Copy it into the folder above, then start the game again.");
        if !silent {
            println!("        You can also re-run this installer and point it at the file.");
        }
    }
    println!();

    if !no_launch && dir.join(IPA_FILE).is_file() && ask_yes("  Start the game now? [Y/n] ") {
        let _ = Command::new(dir.join(GAME_EXE)).current_dir(&dir).spawn();
    }

    if !silent {
        print!("  Press Enter to close...");
        let _ = io::stdout().flush();
        let mut buf = String::new();
        let _ = io::stdin().read_line(&mut buf);
    }
}

enum Ipa {
    Copied(&'static str),
    Missing,
}

/// Puts 1.2.0.ipa in the install folder, from the binary itself, from beside setup.exe,
/// from the working directory, or from a path the user types in.
fn install_ipa(dir: &Path, silent: bool) -> Ipa {
    let target = dir.join(IPA_FILE);
    if is_real_file(&target) {
        return Ipa::Copied("already installed");
    }

    if let Some(bytes) = IPA {
        if write_file(&target, bytes).is_ok() {
            return Ipa::Copied("embedded in this installer");
        }
    }

    let mut candidates: Vec<PathBuf> = Vec::new();
    if let Some(exe_dir) = std::env::current_exe()
        .ok()
        .and_then(|p| p.parent().map(Path::to_path_buf))
    {
        candidates.push(exe_dir.join(IPA_FILE));
    }
    if let Ok(cwd) = std::env::current_dir() {
        candidates.push(cwd.join(IPA_FILE));
    }
    for candidate in candidates {
        if is_real_file(&candidate) && fs::copy(&candidate, &target).is_ok() {
            return Ipa::Copied("copied next to setup.exe");
        }
    }

    if silent {
        return Ipa::Missing;
    }

    loop {
        println!();
        println!("  The game renders everything from the original {IPA_FILE}.");
        println!("  Enter the full path to it, or press Enter to skip:");
        print!("  > ");
        let _ = io::stdout().flush();
        let mut answer = String::new();
        if io::stdin().read_line(&mut answer).is_err() {
            return Ipa::Missing;
        }
        let answer = answer.trim().trim_matches('"');
        if answer.is_empty() {
            return Ipa::Missing;
        }
        let src = PathBuf::from(answer);
        if !is_real_file(&src) {
            println!(
                "  {} does not look like the archive - try again.",
                src.display()
            );
            continue;
        }
        return match fs::copy(&src, &target) {
            Ok(_) => Ipa::Copied("copied from the path you gave"),
            Err(e) => {
                println!("  copy failed: {e}");
                Ipa::Missing
            }
        };
    }
}

/// Desktop and Start Menu entries, through the shell's own shortcut API.
fn shortcuts(dir: &Path) -> io::Result<usize> {
    let target = dir.join(GAME_EXE);
    let uninstall = dir.join(UNINSTALL_EXE);
    let script = format!(
        r#"$d=[Environment]::GetFolderPath('Desktop'); $p=[Environment]::GetFolderPath('Programs');
           $s=(New-Object -ComObject WScript.Shell).CreateShortcut((Join-Path $d 'Angry Birds 1.2.0.lnk'));
           $s.TargetPath='{game}'; $s.WorkingDirectory='{dir}'; $s.IconLocation='{game},0';
           $s.Description='Angry Birds 1.2.0 (Rust rebuild)'; $s.Save();
           $g=Join-Path $p 'Angry Birds 1.2.0'; New-Item -ItemType Directory -Force -Path $g | Out-Null;
           $t=(New-Object -ComObject WScript.Shell).CreateShortcut((Join-Path $g 'Angry Birds 1.2.0.lnk'));
           $t.TargetPath='{game}'; $t.WorkingDirectory='{dir}'; $t.IconLocation='{game},0'; $t.Save();
           $u=(New-Object -ComObject WScript.Shell).CreateShortcut((Join-Path $g 'Uninstall Angry Birds 1.2.0.lnk'));
           $u.TargetPath='{uninstall}'; $u.WorkingDirectory='{dir}'; $u.Save();
           exit 0"#,
        game = target.display(),
        dir = dir.display(),
        uninstall = uninstall.display(),
    );
    let status = Command::new("powershell")
        .args(["-NoProfile", "-NonInteractive", "-Command", &script])
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .status()?;
    if status.success() {
        Ok(3)
    } else {
        Err(io::Error::other("powershell failed"))
    }
}

/// Everything Settings -> Apps needs, plus the game's own key.
fn registry(dir: &Path) -> bool {
    let game = dir.join(GAME_EXE).display().to_string();
    let uninstall = format!("\"{}\"", dir.join(UNINSTALL_EXE).display());
    let location = dir.display().to_string();
    let entries: [(&str, &str, &str, &str); 11] = [
        (
            REG_UNINSTALL,
            "DisplayName",
            "REG_SZ",
            "Angry Birds 1.2.0 (Rust rebuild)",
        ),
        (REG_UNINSTALL, "DisplayVersion", "REG_SZ", "1.2.0"),
        (REG_UNINSTALL, "Publisher", "REG_SZ", "Shohail Mahmud"),
        (REG_UNINSTALL, "InstallLocation", "REG_SZ", &location),
        (REG_UNINSTALL, "DisplayIcon", "REG_SZ", &game),
        (REG_UNINSTALL, "UninstallString", "REG_SZ", &uninstall),
        (REG_UNINSTALL, "NoModify", "REG_DWORD", "1"),
        (REG_UNINSTALL, "NoRepair", "REG_DWORD", "1"),
        (REG_UNINSTALL, "EstimatedSize", "REG_DWORD", "9000"),
        (REG_KEY, "InstallLocation", "REG_SZ", &location),
        (REG_KEY, "Version", "REG_SZ", "1.2.0"),
    ];
    let mut ok = true;
    for (key, name, kind, value) in entries {
        let status = Command::new("reg")
            .args(["add", key, "/v", name, "/t", kind, "/d", value, "/f"])
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .status();
        ok &= status.map(|s| s.success()).unwrap_or(false);
    }
    ok
}

fn install_dir() -> PathBuf {
    if let Ok(local) = std::env::var("LOCALAPPDATA") {
        return PathBuf::from(local).join(FOLDER);
    }
    // No LOCALAPPDATA (Wine without a profile, or a stripped-down environment):
    // install beside the setup exe rather than failing.
    std::env::current_exe()
        .ok()
        .and_then(|p| p.parent().map(Path::to_path_buf))
        .unwrap_or_else(|| PathBuf::from("."))
        .join(FOLDER)
}

fn is_real_file(path: &Path) -> bool {
    fs::metadata(path)
        .map(|m| m.is_file() && m.len() > 4 * 1024 * 1024)
        .unwrap_or(false)
}

fn write_file(path: &Path, bytes: &[u8]) -> io::Result<()> {
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)?;
    }
    fs::write(path, bytes)
}

fn ask_yes(prompt: &str) -> bool {
    print!("{prompt}");
    let _ = io::stdout().flush();
    let mut answer = String::new();
    if io::stdin().read_line(&mut answer).is_err() {
        return false;
    }
    matches!(
        answer.trim().to_ascii_lowercase().as_str(),
        "" | "y" | "yes"
    )
}

fn banner() {
    println!("==============================================");
    println!("  ANGRY BIRDS 1.2.0 (Rust rebuild)");
    println!("  single-file installer");
    println!("==============================================");
    println!();
    println!("  game          {} bytes", GAME.len());
    println!("  uninstaller   {} bytes", UNINSTALLER.len());
    println!(
        "  game assets   {}",
        if IPA.is_some() {
            format!("{IPA_BYTES} bytes, embedded")
        } else {
            "not embedded - 1.2.0.ipa needed".to_string()
        }
    );
    println!();
}

fn fail(message: &str) -> ! {
    println!();
    println!("  ERROR: {message}");
    println!();
    print!("  Press Enter to close...");
    let _ = io::stdout().flush();
    let mut buf = String::new();
    let _ = io::stdin().read_line(&mut buf);
    std::process::exit(1);
}
