use std::io;
use std::error;
use std::fmt;
use std::net::Ipv4Addr;

use rreq::*;
use rrep::*;
use rerr::*;

#[derive(Debug)]
pub struct ParseError;

impl error::Error for ParseError {
    fn description(&self) -> &str {
        "Unable to parse bit message as AODV message"
    }
}

impl fmt::Display for ParseError {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        write!(f, "Unable to parse bit message as AODV message")
    }
}

pub enum AodvMessage {
    Rreq(RREQ),
    Rrep(RREP),
    Rerr(RERR),
    Hello(RREP),
    Ack,
}

impl AodvMessage {
    pub fn parse(b: &[u8]) -> Result<(), ParseError> {
        if b.len() == 0 {
            return Err(ParseError {});
        }
        match (b[0], b.len(), b.len() % 4) {
            (1, 24, 0) => {
                match RREQ::new(b) {
                    Ok(r) => Ok(r.handle_message()),
                    Err(e) => Err(e),
                }
            }
            (2, 20, 0) => {
                match RREP::new(b) {
                    Ok(r) => Ok(r.handle_message()),
                    Err(e) => Err(e),
                }
            }
            (3, _, 0) => {
                match RERR::new(b) {
                    Ok(r) => Ok(r.handle_message()),
                    Err(e) => Err(e),
                }
            }
            (4, 2, 2) => {
                println!("rerr message!");
                Ok(())
            }
            (_, _, _) => Err(ParseError),
        }
    }
}
