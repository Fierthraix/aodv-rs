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
