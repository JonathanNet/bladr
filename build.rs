//! Embed the application icon and version information into the Windows exe.
//!
//! `eframe` can set the icon of the *window*, but Explorer and the taskbar read
//! the icon resource from the PE file itself. That resource has to be compiled
//! with the Windows resource compiler (`rc.exe`) and linked in.

use std::env;
use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;

fn main() {
    println!("cargo:rerun-if-changed=assets/icon.ico");
    println!("cargo:rerun-if-changed=assets/app.manifest");
    println!("cargo:rerun-if-changed=build.rs");

    let target_os = env::var("CARGO_CFG_TARGET_OS").unwrap_or_default();
    if target_os != "windows" {
        return;
    }

    let out_dir = PathBuf::from(env::var("OUT_DIR").expect("OUT_DIR"));
    let icon = Path::new("assets/icon.ico");
    if !icon.exists() {
        println!("cargo:warning=assets/icon.ico mangler - exe-filen får intet ikon");
        return;
    }

    let Some(rc) = find_rc() else {
        println!(
            "cargo:warning=rc.exe blev ikke fundet - exe-filen får intet ikon. \
             Installér Visual Studio Build Tools eller Windows SDK."
        );
        return;
    };

    let icon_copy = out_dir.join("icon.ico");
    if let Err(err) = fs::copy(icon, &icon_copy) {
        println!("cargo:warning=kunne ikke kopiere ikonet: {err}");
        return;
    }

    let manifest = Path::new("assets/app.manifest");
    if manifest.exists() {
        if let Err(err) = fs::copy(manifest, out_dir.join("app.manifest")) {
            println!("cargo:warning=kunne ikke kopiere manifestet: {err}");
        }
    }

    let version = env::var("CARGO_PKG_VERSION").unwrap_or_else(|_| "0.1.0".to_owned());
    let version_tuple = version
        .split('.')
        .map(|part| part.parse::<u16>().unwrap_or(0))
        .collect::<Vec<_>>();
    let v = |index: usize| version_tuple.get(index).copied().unwrap_or(0);

    let rc_text = format!(
        r#"1 ICON "icon.ico"
1 24 "app.manifest"

1 VERSIONINFO
FILEVERSION {maj},{min},{pat},0
PRODUCTVERSION {maj},{min},{pat},0
FILEFLAGSMASK 0x3fL
FILEFLAGS 0x0L
FILEOS 0x40004L
FILETYPE 0x1L
FILESUBTYPE 0x0L
BEGIN
    BLOCK "StringFileInfo"
    BEGIN
        BLOCK "040904b0"
        BEGIN
            VALUE "CompanyName", "Bladr"
            VALUE "FileDescription", "Bladr - hurtig billedfremviser"
            VALUE "FileVersion", "{version}"
            VALUE "InternalName", "bladr"
            VALUE "LegalCopyright", "MIT"
            VALUE "OriginalFilename", "Bladr.exe"
            VALUE "ProductName", "Bladr"
            VALUE "ProductVersion", "{version}"
        END
    END
    BLOCK "VarFileInfo"
    BEGIN
        VALUE "Translation", 0x409, 1200
    END
END
"#,
        maj = v(0),
        min = v(1),
        pat = v(2),
        version = version,
    );

    let rc_path = out_dir.join("bladr.rc");
    if let Err(err) = fs::write(&rc_path, rc_text) {
        println!("cargo:warning=kunne ikke skrive .rc-filen: {err}");
        return;
    }

    let res_path = out_dir.join("bladr.res");
    let output = Command::new(&rc)
        .current_dir(&out_dir)
        .arg("/nologo")
        .arg("/fo")
        .arg(&res_path)
        .arg(&rc_path)
        .output();

    match output {
        Ok(output) if output.status.success() => {
            println!("cargo:rustc-link-arg-bins={}", res_path.display());
        }
        Ok(output) => {
            let stderr = String::from_utf8_lossy(&output.stderr);
            let stdout = String::from_utf8_lossy(&output.stdout);
            println!(
                "cargo:warning=rc.exe fejlede (status {}): {}{}",
                output.status, stdout, stderr
            );
        }
        Err(err) => {
            println!("cargo:warning=kunne ikke starte rc.exe: {err}");
        }
    }
}

fn find_rc() -> Option<PathBuf> {
    if let Ok(path) = env::var("RC_EXE") {
        let path = PathBuf::from(path);
        if path.is_file() {
            return Some(path);
        }
    }

    // Visual Studio sets this for its own developer shells.
    if let Ok(dir) = env::var("VCToolsInstallDir") {
        let candidate = Path::new(&dir).join("bin/Hostx64/x64/rc.exe");
        if candidate.is_file() {
            return Some(candidate);
        }
    }

    // Windows SDK: pick the newest installed version.
    let kits = Path::new(r"C:\Program Files (x86)\Windows Kits\10\bin");
    let mut best: Option<(Vec<u32>, PathBuf)> = None;
    if let Ok(entries) = fs::read_dir(kits) {
        for entry in entries.flatten() {
            let candidate = entry.path().join("x64/rc.exe");
            if !candidate.is_file() {
                continue;
            }
            let version = entry
                .file_name()
                .to_string_lossy()
                .split('.')
                .filter_map(|part| part.parse::<u32>().ok())
                .collect::<Vec<_>>();
            if best
                .as_ref()
                .map(|(current, _)| version > *current)
                .unwrap_or(true)
            {
                best = Some((version, candidate));
            }
        }
    }

    best.map(|(_, path)| path)
}
