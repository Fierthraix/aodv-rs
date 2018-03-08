use super::std;
use super::std::io;

/// `ParseError` is an `io::Error` specifically for when parsing an aodv message fails
pub struct ParseError;

impl ParseError {
    pub fn new<E>(error: E) -> io::Error where E: Into<Box<std::error::Error + Send + Sync>>{
        io::Error::new(
            io::ErrorKind::InvalidInput,
            error,
            )
    }
}

//TODO: make aodv messages implement BigEndianBytes instaed of `parse` and `bit_message`
/// Convert a type to its byte representation in Big Endian Bytes
pub trait BigEndianBytes {
    //TODO: convert this Vec<u8> to any iterable type (so we can return [u8; 4])
    fn as_be_bytes(self: &Self) -> Vec<u8>;
    fn from_be_bytes(slice: &[u8]) -> Self;
}

impl BigEndianBytes for u32 {
    //TODO try using unsafe transmutation!
    fn as_be_bytes(&self) -> Vec<u8> {
        //[(n >> 24) as u8, (n >> 16) as u8, (n >> 8) as u8, n as u8]
        vec![
            (self >> 24) as u8,
            (self >> 16) as u8,
            (self >> 8) as u8,
            *self as u8,
        ]
    }
    fn from_be_bytes(b: &[u8]) -> Self {
        (u32::from(b[0]) << 24) + (u32::from(b[1]) << 16) + (u32::from(b[2]) << 8) + u32::from(b[3])
    }
}
