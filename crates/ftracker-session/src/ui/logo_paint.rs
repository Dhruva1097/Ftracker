// Shared FT mark pixels. Included from `logo.rs` and `build.rs` (no Windows types).

/// PRIMARY blue BGRA (`theme::PRIMARY` / `ON_PRIMARY`).
const FILL: [u8; 4] = [235, 99, 37, 255];
const GLYPH: [u8; 4] = [255, 255, 255, 255];

pub fn paint_bgra(n: i32) -> Vec<u8> {
    let n = n.max(8);
    let r = (n * 3 / 16).max(2);
    let mut out = vec![0u8; (n * n * 4) as usize];
    for y in 0..n {
        for x in 0..n {
            let i = ((y * n + x) * 4) as usize;
            if !in_round_square(x, y, n, r) {
                continue;
            }
            let c = if is_ft(x, y, n) { GLYPH } else { FILL };
            out[i] = c[0];
            out[i + 1] = c[1];
            out[i + 2] = c[2];
            out[i + 3] = c[3];
        }
    }
    out
}

fn in_round_square(x: i32, y: i32, n: i32, r: i32) -> bool {
    if x < 0 || y < 0 || x >= n || y >= n {
        return false;
    }
    let cx0 = r;
    let cy0 = r;
    let cx1 = n - 1 - r;
    let cy1 = n - 1 - r;
    if x < cx0 && y < cy0 {
        return (x - cx0) * (x - cx0) + (y - cy0) * (y - cy0) <= r * r;
    }
    if x > cx1 && y < cy0 {
        return (x - cx1) * (x - cx1) + (y - cy0) * (y - cy0) <= r * r;
    }
    if x < cx0 && y > cy1 {
        return (x - cx0) * (x - cx0) + (y - cy1) * (y - cy1) <= r * r;
    }
    if x > cx1 && y > cy1 {
        return (x - cx1) * (x - cx1) + (y - cy1) * (y - cy1) <= r * r;
    }
    true
}

/// Nearest-neighbour from a 16×16 FT: F on the left, T on the right.
fn is_ft(x: i32, y: i32, n: i32) -> bool {
    let gx = x * 16 / n;
    let gy = y * 16 / n;
    let f_stem = (3..=5).contains(&gx) && (3..=12).contains(&gy);
    let f_top = (3..=8).contains(&gx) && (3..=4).contains(&gy);
    let f_mid = (3..=7).contains(&gx) && (7..=8).contains(&gy);
    let t_top = (9..=13).contains(&gx) && (3..=4).contains(&gy);
    let t_stem = (10..=12).contains(&gx) && (3..=12).contains(&gy);
    f_stem || f_top || f_mid || t_top || t_stem
}

#[allow(dead_code)]
pub fn ico_file(sizes: &[i32]) -> Vec<u8> {
    let images: Vec<(i32, Vec<u8>)> = sizes.iter().copied().map(|n| (n, dib_image(n))).collect();
    let count = images.len() as u16;
    let dir = 6 + 16 * images.len();
    let mut off = dir as u32;
    let mut out = Vec::new();
    out.extend_from_slice(&0u16.to_le_bytes());
    out.extend_from_slice(&1u16.to_le_bytes());
    out.extend_from_slice(&count.to_le_bytes());
    for (n, img) in &images {
        let w = if *n >= 256 { 0u8 } else { *n as u8 };
        out.push(w);
        out.push(w);
        out.push(0);
        out.push(0);
        out.extend_from_slice(&1u16.to_le_bytes());
        out.extend_from_slice(&32u16.to_le_bytes());
        out.extend_from_slice(&(img.len() as u32).to_le_bytes());
        out.extend_from_slice(&off.to_le_bytes());
        off += img.len() as u32;
    }
    for (_, img) in &images {
        out.extend_from_slice(img);
    }
    out
}

#[allow(dead_code)]
fn dib_image(n: i32) -> Vec<u8> {
    let top = paint_bgra(n);
    let xor_len = (n * n * 4) as usize;
    let and_stride = ((n + 31) / 32) * 4;
    let and_len = (and_stride * n) as usize;
    let mut dib = Vec::with_capacity(40 + xor_len + and_len);
    dib.extend_from_slice(&40u32.to_le_bytes());
    dib.extend_from_slice(&n.to_le_bytes());
    dib.extend_from_slice(&(n * 2).to_le_bytes());
    dib.extend_from_slice(&1u16.to_le_bytes());
    dib.extend_from_slice(&32u16.to_le_bytes());
    dib.extend_from_slice(&0u32.to_le_bytes());
    dib.extend_from_slice(&((xor_len + and_len) as u32).to_le_bytes());
    dib.extend_from_slice(&0u32.to_le_bytes());
    dib.extend_from_slice(&0u32.to_le_bytes());
    dib.extend_from_slice(&0u32.to_le_bytes());
    dib.extend_from_slice(&0u32.to_le_bytes());
    for y in (0..n).rev() {
        let row = (y * n * 4) as usize;
        dib.extend_from_slice(&top[row..row + (n * 4) as usize]);
    }
    dib.resize(40 + xor_len + and_len, 0);
    dib
}
