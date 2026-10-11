/// 64-bit difference hash. Hamming distance below `threshold` => similar.
pub fn dhash_gray(width: u32, height: u32, pixels: &[u8]) -> u64 {
    if width < 9 || height < 8 || pixels.len() < (width * height) as usize {
        return 0;
    }
    let mut hash = 0u64;
    let mut bit = 0;
    for y in 0..8 {
        for x in 0..8 {
            let i = (y * width + x) as usize;
            let j = (y * width + x + 1) as usize;
            if pixels[j] > pixels[i] {
                hash |= 1u64 << bit;
            }
            bit += 1;
        }
    }
    hash
}

pub fn hamming(a: u64, b: u64) -> u32 {
    (a ^ b).count_ones()
}

pub fn similar(a: u64, b: u64, threshold: u32) -> bool {
    a != 0 && b != 0 && hamming(a, b) <= threshold
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn identical_frames_are_similar() {
        let mut px = vec![0u8; 9 * 8];
        for i in 0..px.len() {
            px[i] = (i % 17) as u8;
        }
        let h = dhash_gray(9, 8, &px);
        assert_ne!(h, 0);
        assert!(similar(h, h, 10));
        assert!(!similar(h, 0, 10));
        assert_eq!(hamming(h, h), 0);
    }
}
