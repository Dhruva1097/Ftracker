//! Bake TELEGRAM_BOT_TOKEN and TELEGRAM_ADMIN_CHAT_ID from `.env` into the
//! binary. The `.env` file itself is never packed into the MSI.

use std::env;
use std::fs;
use std::path::PathBuf;

fn main() {
    println!("cargo:rerun-if-env-changed=FTRACKER_TELEGRAM_BOT_TOKEN");
    println!("cargo:rerun-if-env-changed=FTRACKER_TELEGRAM_ADMIN_CHAT_ID");
    println!("cargo:rerun-if-env-changed=TELEGRAM_BOT_TOKEN");
    println!("cargo:rerun-if-env-changed=TELEGRAM_ADMIN_CHAT_ID");

    let (mut token, mut chat) = read_dotenv();
    if token.is_empty() {
        token = first_env(&[
            "FTRACKER_TELEGRAM_BOT_TOKEN",
            "TELEGRAM_BOT_TOKEN",
        ]);
    }
    if chat.is_empty() {
        chat = first_env(&[
            "FTRACKER_TELEGRAM_ADMIN_CHAT_ID",
            "TELEGRAM_ADMIN_CHAT_ID",
        ]);
    }

    let dest = PathBuf::from(env::var("OUT_DIR").unwrap()).join("baked_telegram.rs");
    let body = format!(
        "pub const BAKED_TELEGRAM_BOT_TOKEN: &str = {:?};\npub const BAKED_TELEGRAM_ADMIN_CHAT_ID: &str = {:?};\n",
        token, chat
    );
    fs::write(dest, body).expect("write baked_telegram.rs");
}

fn first_env(keys: &[&str]) -> String {
    for k in keys {
        if let Ok(v) = env::var(k) {
            let v = v.trim().to_string();
            if !v.is_empty() {
                return v;
            }
        }
    }
    String::new()
}

fn read_dotenv() -> (String, String) {
    let mut dir = PathBuf::from(env::var("CARGO_MANIFEST_DIR").unwrap());
    for _ in 0..8 {
        let file = dir.join(".env");
        println!("cargo:rerun-if-changed={}", file.display());
        if file.is_file() {
            return parse_dotenv(&file);
        }
        if !dir.pop() {
            break;
        }
    }
    (String::new(), String::new())
}

fn parse_dotenv(path: &std::path::Path) -> (String, String) {
    let Ok(text) = fs::read_to_string(path) else {
        return (String::new(), String::new());
    };
    let mut token = String::new();
    let mut chat = String::new();
    for raw in text.lines() {
        let line = raw.trim();
        if line.is_empty() || line.starts_with('#') {
            continue;
        }
        let Some((k, v)) = line.split_once('=') else {
            continue;
        };
        let k = k.trim();
        let v = v.trim().trim_matches('"').trim_matches('\'').to_string();
        if v.is_empty() {
            continue;
        }
        if k == "TELEGRAM_BOT_TOKEN" {
            token = v;
        } else if k == "TELEGRAM_ADMIN_CHAT_ID" {
            chat = v;
        }
    }
    (token, chat)
}
