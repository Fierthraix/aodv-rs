// Convert big endian bytes to a u32
#[inline]
pub fn bytes_as_u32_be(slice: &[u8]) -> u32 {
    ((slice[0] as u32) << 24) + ((slice[1] as u32) << 16) + ((slice[2] as u32) << 8) +
        ((slice[3] as u32) << 0)
}

// Convert u32 to byte array
#[inline]
pub fn u32_as_bytes_be(n: u32) -> [u8; 4] {
    [(n >> 24) as u8, (n >> 16) as u8, (n >> 8) as u8, n as u8]
}

#[test]
fn test_conversions() {
    let b = 19381837;
    assert_eq!(bytes_as_u32_be(&u32_as_bytes_be(b)), b)
}
