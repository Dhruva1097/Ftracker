use std::path::PathBuf;

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
    // Do not link ftracker.res into the exe here: a bad resource section
    // crashes at process start with 0xC0000005 before any Rust log runs.
}
