
// Convert big endian bytes to a u32
pub fn as_u32_be(slice: &[u8]) -> Option<u32> {
    if slice.len() != 4 {
        return None;
    }
    Some(
        ((slice[0] as u32) << 24) + ((slice[0] as u32) << 16) + ((slice[0] as u32) << 8) +
            ((slice[0] as u32) << 0),
    )
}
