use anyhow::Result;
use ftracker_core::domain::dhash;
use image::{DynamicImage, ImageBuffer, Rgba};

#[cfg(windows)]
pub fn capture_jpeg(max_width: u32, quality: u8) -> Result<Option<(Vec<u8>, u64)>> {
    use windows::Win32::Foundation::HWND;
    use windows::Win32::Graphics::Gdi::{
        BitBlt, CreateCompatibleBitmap, CreateCompatibleDC, DeleteDC, DeleteObject, GetDC,
        GetDIBits, ReleaseDC, SelectObject, BITMAPINFO, BITMAPINFOHEADER, BI_RGB, DIB_RGB_COLORS,
        HGDIOBJ, SRCCOPY,
    };
    use windows::Win32::UI::WindowsAndMessaging::{GetSystemMetrics, SM_CXSCREEN, SM_CYSCREEN};

    if is_locked_or_saver() {
        return Ok(None);
    }
    let w = unsafe { GetSystemMetrics(SM_CXSCREEN) };
    let h = unsafe { GetSystemMetrics(SM_CYSCREEN) };
    if w <= 0 || h <= 0 {
        return Ok(None);
    }
    let nbytes = (w as usize)
        .saturating_mul(h as usize)
        .saturating_mul(4);
    const MAX_SHOT: usize = 64 * 1024 * 1024;
    if nbytes == 0 || nbytes > MAX_SHOT {
        return Ok(None);
    }
    let hwnd = HWND(0);
    let hdc = unsafe { GetDC(hwnd) };
    if hdc.0 == 0 {
        return Ok(None);
    }
    unsafe {
        let mem = CreateCompatibleDC(hdc);
        let bmp = CreateCompatibleBitmap(hdc, w, h);
        let old = SelectObject(mem, HGDIOBJ(bmp.0));
        let _ = BitBlt(mem, 0, 0, w, h, hdc, 0, 0, SRCCOPY);
        let mut info = BITMAPINFO {
            bmiHeader: BITMAPINFOHEADER {
                biSize: std::mem::size_of::<BITMAPINFOHEADER>() as u32,
                biWidth: w,
                biHeight: -h,
                biPlanes: 1,
                biBitCount: 32,
                biCompression: BI_RGB.0 as u32,
                ..Default::default()
            },
            ..Default::default()
        };
        let mut buf = vec![0u8; nbytes];
        GetDIBits(
            mem,
            bmp,
            0,
            h as u32,
            Some(buf.as_mut_ptr() as *mut _),
            &mut info,
            DIB_RGB_COLORS,
        );
        SelectObject(mem, old);
        let _ = DeleteObject(HGDIOBJ(bmp.0));
        let _ = DeleteDC(mem);
        ReleaseDC(hwnd, hdc);
        let img: ImageBuffer<Rgba<u8>, Vec<u8>> =
            ImageBuffer::from_raw(w as u32, h as u32, bgra_to_rgba(&buf, w as u32, h as u32))
                .ok_or_else(|| anyhow::anyhow!("screenshot buffer"))?;
        let dynimg = DynamicImage::ImageRgba8(img);
        encode(dynimg, max_width, quality)
    }
}

fn bgra_to_rgba(bgra: &[u8], w: u32, h: u32) -> Vec<u8> {
    let n = (w as usize)
        .saturating_mul(h as usize)
        .saturating_mul(4)
        .min(bgra.len());
    let mut out = vec![0u8; n];
    let pixels = n / 4;
    for i in 0..pixels {
        let o = i * 4;
        out[o] = bgra[o + 2];
        out[o + 1] = bgra[o + 1];
        out[o + 2] = bgra[o];
        out[o + 3] = 255;
    }
    out
}

fn encode(img: DynamicImage, max_width: u32, quality: u8) -> Result<Option<(Vec<u8>, u64)>> {
    let img = if img.width() > max_width {
        img.resize(
            max_width,
            max_width * img.height() / img.width().max(1),
            image::imageops::FilterType::Lanczos3,
        )
    } else {
        img
    };
    let gray = img
        .resize_exact(9, 8, image::imageops::FilterType::Triangle)
        .to_luma8();
    let hash = dhash::dhash_gray(9, 8, gray.as_raw());
    let mut jpeg = Vec::new();
    let mut enc = image::codecs::jpeg::JpegEncoder::new_with_quality(&mut jpeg, quality);
    enc.encode(
        img.to_rgb8().as_raw(),
        img.width(),
        img.height(),
        image::ColorType::Rgb8,
    )?;
    Ok(Some((jpeg, hash)))
}

#[cfg(windows)]
fn is_locked_or_saver() -> bool {
    use windows::Win32::UI::WindowsAndMessaging::{
        SystemParametersInfoW, SPI_GETSCREENSAVERRUNNING, SYSTEM_PARAMETERS_INFO_UPDATE_FLAGS,
    };
    let mut running: u32 = 0;
    unsafe {
        let _ = SystemParametersInfoW(
            SPI_GETSCREENSAVERRUNNING,
            0,
            Some((&mut running as *mut u32).cast()),
            SYSTEM_PARAMETERS_INFO_UPDATE_FLAGS(0),
        );
    }
    if running != 0 {
        return true;
    }
    false
}

#[cfg(not(windows))]
pub fn capture_jpeg(_max_width: u32, _quality: u8) -> Result<Option<(Vec<u8>, u64)>> {
    Ok(None)
}
