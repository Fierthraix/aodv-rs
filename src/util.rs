use super::std::io;

macro_rules! parse_error {
    ($x:expr) => {
        io::Error::new(io::ErrorKind::InvalidInput, $x)
    };
    () => {
        parse_error!("")
    }
}
