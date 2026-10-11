//! Anti-aliased rounded rectangles. Do not use GDI RoundRect — it is aliased and looks muddy.

#[cfg(windows)]
use windows::Win32::Foundation::RECT;
#[cfg(windows)]
use windows::Win32::Graphics::Gdi::HDC;

fn unpack(c: u32) -> (u8, u8, u8) {
    ((c & 0xff) as u8, ((c >> 8) & 0xff) as u8, ((c >> 16) & 0xff) as u8)
}

fn mix(a: u8, b: u8, t: f32) -> u8 {
    (a as f32 + (b as f32 - a as f32) * t).round() as u8
}

fn sd_round_box(px: f32, py: f32, hx: f32, hy: f32, r: f32) -> f32 {
    let r = r.min(hx.min(hy)).max(0.0);
    let qx = px.abs() - hx + r;
    let qy = py.abs() - hy + r;
    let outside = (qx.max(0.0) * qx.max(0.0) + qy.max(0.0) * qy.max(0.0)).sqrt();
    let inside = qx.max(qy).min(0.0);
    outside + inside - r
}

fn cover(sd: f32) -> f32 {
    (0.5 - sd).clamp(0.0, 1.0)
}

fn raster(w: i32, h: i32, radius: i32, fill: u32, border: u32, border_px: i32, backdrop: u32) -> Vec<u8> {
    let w = w.max(1);
    let h = h.max(1);
    let hx = w as f32 * 0.5;
    let hy = h as f32 * 0.5;
    let r = radius as f32;
    let bw = border_px.max(0) as f32;
    let (fr, fg, fb) = unpack(fill);
    let (er, eg, eb) = unpack(border);
    let (br, bg, bb) = unpack(backdrop);
    let mut out = vec![0u8; (w * h * 4) as usize];
    for y in 0..h {
        for x in 0..w {
            let px = x as f32 + 0.5 - hx;
            let py = y as f32 + 0.5 - hy;
            let outer = sd_round_box(px, py, hx, hy, r);
            let oc = cover(outer);
            let (cr, cg, cb) = if bw <= 0.0 {
                (
                    mix(br, fr, oc),
                    mix(bg, fg, oc),
                    mix(bb, fb, oc),
                )
            } else {
                let inner = sd_round_box(
                    px,
                    py,
                    (hx - bw).max(0.5),
                    (hy - bw).max(0.5),
                    (r - bw).max(0.0),
                );
                let ic = cover(inner);
                let bc = (oc - ic).max(0.0);
                let t0r = mix(br, er, bc);
                let t0g = mix(bg, eg, bc);
                let t0b = mix(bb, eb, bc);
                (
                    mix(t0r, fr, ic),
                    mix(t0g, fg, ic),
                    mix(t0b, fb, ic),
                )
            };
            let i = ((y * w + x) * 4) as usize;
            out[i] = cb;
            out[i + 1] = cg;
            out[i + 2] = cr;
            out[i + 3] = 255;
        }
    }
    out
}

#[cfg(windows)]
pub fn paint(
    hdc: HDC,
    rc: RECT,
    radius: i32,
    fill: u32,
    border: u32,
    border_px: i32,
    backdrop: u32,
) {
    use windows::Win32::Graphics::Gdi::{
        SetDIBitsToDevice, BITMAPINFO, BITMAPINFOHEADER, DIB_RGB_COLORS,
    };
    let w = (rc.right - rc.left).max(1);
    let h = (rc.bottom - rc.top).max(1);
    let bits = raster(w, h, radius, fill, border, border_px, backdrop);
    let mut info = BITMAPINFO::default();
    info.bmiHeader = BITMAPINFOHEADER {
        biSize: std::mem::size_of::<BITMAPINFOHEADER>() as u32,
        biWidth: w,
        biHeight: -h,
        biPlanes: 1,
        biBitCount: 32,
        biCompression: 0,
        biSizeImage: (w * h * 4) as u32,
        ..Default::default()
    };
    unsafe {
        SetDIBitsToDevice(
            hdc,
            rc.left,
            rc.top,
            w as u32,
            h as u32,
            0,
            0,
            0,
            h as u32,
            bits.as_ptr() as *const core::ffi::c_void,
            &info,
            DIB_RGB_COLORS,
        );
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rounded_corners_are_blended() {
        let px = raster(40, 24, 8, 0x00EB6325, 0x00EB6325, 0, 0x00FFF7F1);
        let corner = (px[0], px[1], px[2]);
        let mid = {
            let i = ((12 * 40 + 20) * 4) as usize;
            (px[i], px[i + 1], px[i + 2])
        };
        assert_ne!(corner, mid, "corner must not be a hard fill");
    }
}
