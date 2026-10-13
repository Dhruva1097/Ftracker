use std::path::{Path, PathBuf};
use std::process::Command;

include!("src/ui/logo_paint.rs");

fn main() {
    println!("cargo:rerun-if-changed=app.manifest");
    println!("cargo:rerun-if-changed=src/ui/logo_paint.rs");
    if std::env::var("CARGO_CFG_WINDOWS").is_err() {
        return;
    }

    let ico = ico_file(&[16, 32, 48]);
    let manifest_dir = PathBuf::from(std::env::var("CARGO_MANIFEST_DIR").unwrap());
    let ico_path = manifest_dir.join("ftracker.ico");
    std::fs::write(&ico_path, &ico).expect("write ftracker.ico");
    if let Some(res_dir) = manifest_dir
        .parent()
        .and_then(|p| p.parent())
        .map(|root| root.join("packaging").join("res"))
    {
        let _ = std::fs::create_dir_all(&res_dir);
        let _ = std::fs::write(res_dir.join("ftracker.ico"), &ico);
    }

    if std::env::var("CARGO_CFG_TARGET_ENV").as_deref() == Ok("msvc") {
        let path = std::fs::canonicalize("app.manifest").expect("app.manifest");
        println!("cargo:rustc-link-arg=/MANIFEST:EMBED");
        println!("cargo:rustc-link-arg=/MANIFESTINPUT:{}", path.display());
    }

    if let Some(res) = compile_icon_res(&ico_path) {
        println!("cargo:rustc-link-arg={}", res.display());
    }
}

fn compile_icon_res(ico: &Path) -> Option<PathBuf> {
    let out = PathBuf::from(std::env::var("OUT_DIR").ok()?);
    let rc_path = out.join("ftracker.rc");
    let res_path = out.join("ftracker.res");
    let ico_s = ico.display().to_string().replace('\\', "/");
    std::fs::write(&rc_path, format!("1 ICON \"{ico_s}\"\r\n")).ok()?;
    let rc = find_rc()?;
    let status = Command::new(rc)
        .args([
            "/nologo",
            "/fo",
            res_path.to_str()?,
            rc_path.to_str()?,
        ])
        .status()
        .ok()?;
    if status.success() && res_path.exists() {
        Some(res_path)
    } else {
        None
    }
}

fn find_rc() -> Option<PathBuf> {
    if let Ok(out) = Command::new("where").arg("rc.exe").output() {
        if out.status.success() {
            let line = String::from_utf8_lossy(&out.stdout)
                .lines()
                .next()
                .unwrap_or("")
                .trim()
                .to_string();
            if !line.is_empty() {
                return Some(PathBuf::from(line));
            }
        }
    }
    let kits = PathBuf::from(r"C:\Program Files (x86)\Windows Kits\10\bin");
    let entries = std::fs::read_dir(kits).ok()?;
    let mut vers: Vec<PathBuf> = entries.filter_map(|e| e.ok().map(|e| e.path())).collect();
    vers.sort();
    for ver in vers.into_iter().rev() {
        let p = ver.join("x64").join("rc.exe");
        if p.is_file() {
            return Some(p);
        }
    }
    None
}
