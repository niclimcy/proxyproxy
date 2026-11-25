//! Bounds-checked integer reads shared by the binary-format parsers; `None` on any overrun.

fn array<const N: usize>(b: &[u8], at: usize) -> Option<[u8; N]> {
    b.get(at..at.checked_add(N)?)?.try_into().ok()
}

pub fn le16(b: &[u8], at: usize) -> Option<u16> {
    array(b, at).map(u16::from_le_bytes)
}

pub fn le32(b: &[u8], at: usize) -> Option<u32> {
    array(b, at).map(u32::from_le_bytes)
}

pub fn be16(b: &[u8], at: usize) -> Option<u16> {
    array(b, at).map(u16::from_be_bytes)
}

pub fn be32(b: &[u8], at: usize) -> Option<u32> {
    array(b, at).map(u32::from_be_bytes)
}

// Offsets and sizes read from a file are used as indices; `try_from` keeps 32-bit targets honest.
pub fn le16_usize(b: &[u8], at: usize) -> Option<usize> {
    Some(usize::from(le16(b, at)?))
}

pub fn be16_usize(b: &[u8], at: usize) -> Option<usize> {
    Some(usize::from(be16(b, at)?))
}

pub fn le32_usize(b: &[u8], at: usize) -> Option<usize> {
    usize::try_from(le32(b, at)?).ok()
}

pub fn be32_usize(b: &[u8], at: usize) -> Option<usize> {
    usize::try_from(be32(b, at)?).ok()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn reads_both_endians_and_rejects_overruns() {
        let b = [1, 2, 3, 4];
        assert_eq!(le16(&b, 0), Some(0x0201));
        assert_eq!(be16(&b, 2), Some(0x0304));
        assert_eq!(le32(&b, 0), Some(0x0403_0201));
        assert_eq!(be32_usize(&b, 0), Some(0x0102_0304));
        assert_eq!(le32(&b, 1), None);
        assert_eq!(le16(&b, usize::MAX), None);
    }
}
