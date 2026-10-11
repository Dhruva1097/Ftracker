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

/// Square BGRA bitmap (top-down) with a 4-module quiet zone. `dim` is width and height.
pub fn qr_bgra(otpauth: &str) -> Result<(u32, Vec<u8>)> {
    let code = QrCode::new(otpauth.as_bytes()).map_err(|e| anyhow!("{e}"))?;
    let modules = code.width() as u32;
    let quiet = 4u32;
    let inner = modules + quiet * 2;
    let module_px = (248 / inner).max(4);
    let dim = inner * module_px;
    let mut buf = vec![255u8; (dim as usize) * (dim as usize) * 4];
    for y in 0..modules {
        for x in 0..modules {
            if code[(x as usize, y as usize)] != qrcode::Color::Dark {
                continue;
            }
            let x0 = (x + quiet) * module_px;
            let y0 = (y + quiet) * module_px;
            for dy in 0..module_px {
                for dx in 0..module_px {
                    let i = (((y0 + dy) * dim + (x0 + dx)) * 4) as usize;
                    buf[i] = 0;
                    buf[i + 1] = 0;
                    buf[i + 2] = 0;
                    buf[i + 3] = 255;
                }
            }
        }
    }
    Ok((dim, buf))
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn totp_roundtrip_and_skew() {
        let secret = generate_secret_b32();
        let ts = 1_700_000_000i64;
        let code = generate_code(&secret, ts).unwrap();
        assert_eq!(code.len(), 6);
        assert!(verify(&secret, &code, ts));
        assert!(verify(&secret, &code, ts + 29));
        assert!(!verify(&secret, "12", ts));
        assert!(!verify(&secret, "abcdef", ts));
        let other = generate_secret_b32();
        assert_ne!(secret, other);
        assert!(!verify(&other, &code, ts));
    }

    #[test]
    fn otpauth_contains_secret_not_token() {
        let url = otpauth_url("Alice", "PC1", "MFRGGZDFMZTWQ2LK");
        assert!(url.starts_with("otpauth://totp/"));
        assert!(url.contains("secret=MFRGGZDFMZTWQ2LK"));
        assert!(!url.contains("bot"));
        assert!(!qr_unicode(&url).is_empty());
        let (dim, px) = qr_bgra(&url).unwrap();
        assert!(dim >= 160);
        assert_eq!(px.len(), dim as usize * dim as usize * 4);
        assert!(px.iter().any(|b| *b == 0), "QR must contain black modules");
    }
}
