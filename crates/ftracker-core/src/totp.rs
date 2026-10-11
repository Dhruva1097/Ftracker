use anyhow::{anyhow, Result};
use hmac::{Hmac, Mac};
use qrcode::render::unicode;
use qrcode::QrCode;
use rand::{Rng, RngCore};
use sha1::Sha1;

type HmacSha1 = Hmac<Sha1>;

pub fn generate_secret_b32() -> String {
    let mut raw = [0u8; 20];
    rand::thread_rng().fill_bytes(&mut raw);
    base32::encode(base32::Alphabet::RFC4648 { padding: false }, &raw)
}

pub fn otpauth_url(employee: &str, machine: &str, secret_b32: &str) -> String {
    let label = urlencoding_basic(&format!("Ftracker:{employee}@{machine}"));
    format!("otpauth://totp/{label}?secret={secret_b32}&issuer=Ftracker&algorithm=SHA1&digits=6&period=30")
}

fn urlencoding_basic(s: &str) -> String {
    let mut out = String::new();
    for b in s.bytes() {
        match b {
            b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'_' | b'.' | b'~' => {
                out.push(b as char)
            }
            _ => out.push_str(&format!("%{b:02X}")),
        }
    }
    out
}

pub fn generate_code(secret_b32: &str, unix_secs: i64) -> Result<String> {
    let secret = base32::decode(base32::Alphabet::RFC4648 { padding: false }, secret_b32)
        .ok_or_else(|| anyhow!("bad totp secret"))?;
    let timestep = (unix_secs / 30) as u64;
    let mut msg = [0u8; 8];
    msg.copy_from_slice(&timestep.to_be_bytes());
    let mut mac = HmacSha1::new_from_slice(&secret)?;
    mac.update(&msg);
    let hash = mac.finalize().into_bytes();
    let offset = (hash[19] & 0x0f) as usize;
    let bin = ((hash[offset] as u32 & 0x7f) << 24)
        | ((hash[offset + 1] as u32) << 16)
        | ((hash[offset + 2] as u32) << 8)
        | (hash[offset + 3] as u32);
    Ok(format!("{:06}", bin % 1_000_000))
}

pub fn verify(secret_b32: &str, code: &str, unix_secs: i64) -> bool {
    let code = code.trim();
    if code.len() != 6 {
        return false;
    }
    for skew in [-1i64, 0, 1] {
        if let Ok(expected) = generate_code(secret_b32, unix_secs + skew * 30) {
            if expected == code {
                return true;
            }
        }
    }
    false
}

pub fn qr_unicode(otpauth: &str) -> String {
    match QrCode::new(otpauth.as_bytes()) {
        Ok(code) => code.render::<unicode::Dense1x2>().build(),
        Err(_) => otpauth.to_string(),
    }
}

pub fn recovery_codes() -> Vec<String> {
    let mut codes = Vec::new();
    let mut rng = rand::thread_rng();
    for _ in 0..8 {
        let n: u32 = rng.gen_range(0..100_000_000);
        codes.push(format!("{n:08}"));
    }
    codes
}
