//! Win32 COLORREF palette (0x00BBGGRR). Use these blues on every Ftracker surface.

pub const fn rgb(r: u8, g: u8, b: u8) -> u32 {
    (r as u32) | ((g as u32) << 8) | ((b as u32) << 16)
}

/// Split a COLORREF (`0x00BBGGRR`) into `(r, g, b)`.
pub const fn split_rgb(c: u32) -> (u8, u8, u8) {
    ((c & 0xff) as u8, ((c >> 8) & 0xff) as u8, ((c >> 16) & 0xff) as u8)
}

pub const BG: u32 = rgb(241, 247, 255);
pub const SURFACE: u32 = rgb(255, 255, 255);
pub const PRIMARY: u32 = rgb(37, 99, 235);
pub const PRIMARY_HOVER: u32 = rgb(29, 78, 216);
pub const PRIMARY_DOWN: u32 = rgb(30, 64, 175);
pub const PRIMARY_SOFT: u32 = rgb(219, 234, 254);
pub const LINE: u32 = rgb(191, 219, 254);
pub const TEXT: u32 = rgb(15, 23, 42);
pub const MUTED: u32 = rgb(71, 85, 105);
pub const ON_PRIMARY: u32 = rgb(255, 255, 255);
#[allow(dead_code)]
pub const CHIP: u32 = rgb(30, 64, 175);
#[allow(dead_code)]
pub const CHIP_IDLE: u32 = rgb(51, 65, 85);
#[allow(dead_code)]
pub const CHIP_TEXT: u32 = rgb(239, 246, 255);
