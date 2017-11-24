extern crate tokio_core;

use std::io;
use std::net::SocketAddr;

use rreq::*;
use rrep::*;
use rerr::*;

use tokio_core::net::UdpCodec;

/// `ParseError` is an `io::Error` specifically for when parsing an aodv message fails
pub struct ParseError;

impl ParseError {
    pub fn new() -> io::Error {
        io::Error::new(
            io::ErrorKind::InvalidInput,
            "Unable to parse bit message as AODV message",
        )
    }
}

/// The enum for every sort of aodv control message
pub enum AodvMessage {
    Rreq(RREQ),
    Rrep(RREP),
    Rerr(RERR),
    Hello(RREP),
    Ack,
}

/// This mostly just uses pattern matching to call the struct method corresponding to its enum
impl AodvMessage {
    /// Try to convert bytes into an aodv message struct or return a ParseError
    pub fn parse(b: &[u8]) -> Result<Self, io::Error> {
        if b.is_empty() {
            return Err(ParseError::new());
        }
        use self::AodvMessage::*;
        // Type, Length, Multiple of 4 or not
        match (b[0], b.len(), b.len() % 4) {
            (1, 24, 0) => Ok(Rreq(RREQ::new(b)?)),
            (2, 20, 0) => Ok(Rrep(RREP::new(b)?)),
            (3, _, 0) => Ok(Rerr(RERR::new(b)?)),
            (4, 2, 2) => Ok(Ack),
            (_, _, _) => Err(ParseError::new()),
        }
    }
    /// Convert an aodv control message into its representation as a bitfield
    pub fn bit_message(&self) -> Vec<u8> {
        use self::AodvMessage::*;
        match *self {
            Rreq(ref r) => r.bit_message(),
            Rrep(ref r) => r.bit_message(),
            Rerr(ref r) => r.bit_message(),
            Hello(ref r) => r.bit_message(),
            Ack => vec![4, 0],
        }
    }

    /// Handle a given aodv control message according to the protocol
    pub fn handle_message(self, addr: &SocketAddr) {
        use self::AodvMessage::*;
        match self {
            Rreq(mut r) => r.handle_message(addr),
            Rrep(mut r) => r.handle_message(addr),
            Rerr(mut r) => r.handle_message(addr),
            Hello(mut r) => r.handle_message(addr),
            Ack => {
                println!("Received Ack from {}", addr);
            }
        }
    }
}

/// The `UdpCodec` for handling aodv control message through tokio
pub struct AodvCodec;

impl UdpCodec for AodvCodec {
    //TODO: Find out why codec user crashes when error sent up
    type In = Option<(SocketAddr, AodvMessage)>;
    type Out = (SocketAddr, AodvMessage);

    fn decode(&mut self, addr: &SocketAddr, buf: &[u8]) -> Result<Self::In, io::Error> {
        match AodvMessage::parse(buf) {
            Ok(msg) => Ok(Some((*addr, msg))),
            Err(_) => Ok(None),
        }
        //Ok(Some((*addr, AodvMessage::parse(buf)?)))
    }

    fn encode(&mut self, (addr, msg): Self::Out, into: &mut Vec<u8>) -> SocketAddr {
        into.extend(msg.bit_message());
        addr
    }
}
