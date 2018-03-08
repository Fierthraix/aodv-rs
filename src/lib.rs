use std::net::{Ipv4Addr, SocketAddr};

use std::io::{self, Error};

pub mod config;
mod util;

use util::*;

pub const AODV_PORT: u16 = 654;
pub const INSTANCE_PORT: u16 = 15_292;

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
            return Err(ParseError::new("Buffer is empty"));
        }
        use self::AodvMessage::*;
        // Type, Length, Multiple of 4 or not
        match (b[0], b.len(), b.len() % 4) {
            (1, 24, 0) => Ok(Rreq(RREQ::new(b)?)),
            (2, 20, 0) => Ok(Rrep(RREP::new(b)?)),
            (3, _, 0) => Ok(Rerr(RERR::new(b)?)),
            (4, 2, 2) => Ok(Ack),
            (_, _, _) => Err(ParseError::new("Wrong length or type bit")),
        }
    }
    /// Convert an aodv control message into its representation as a bitfield
    pub fn bit_message(&self) -> Vec<u8> {
        use self::AodvMessage::*;
        match *self {
            Rreq(ref r) => r.bit_message(),
            Rrep(ref r) | Hello(ref r) => r.bit_message(),
            Rerr(ref r) => r.bit_message(),
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

///RREQ Message Format:
///0                   1                   2                   3
///0 1 2 3 4 5 6 7 8 9 0 1 2 3 4 5 6 7 8 9 0 1 2 3 4 5 6 7 8 9 0 1
///+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+
///|     Type      |J|R|G|D|U|   Reserved          |   Hop Count   |
///+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+
///|                            RREQ ID                            |
///+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+
///|                    Destination IP Address                     |
///+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+
///|                  Destination Sequence Number                  |
///+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+
///|                    Originator IP Address                      |
///+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+
///|                  Originator Sequence Number                   |
///+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+
#[derive(Debug, PartialEq)]
pub struct RREQ {
    pub j: bool, // Join flag
    pub r: bool, // Repair flag
    pub g: bool, // Gratuitous RREP flag
    pub d: bool, // Destination Only flag
    pub u: bool, // Unknown Sequence number

    pub hop_count: u8, // 8-bit Hop Count
    pub rreq_id: u32, // 32-bit RREQ ID

    pub dest_ip: Ipv4Addr, // Destination IP Address
    pub dest_seq_num: u32, // Destination Sequence Number

    pub orig_ip: Ipv4Addr, // Originator IP Address
    pub orig_seq_num: u32, // Originator Sequence Number
}

impl RREQ {
    /// Return a RREQ message from a byte slice
    pub fn new(b: &[u8]) -> Result<RREQ, Error> {
        if b.len() != 24 {
            return Err(ParseError::new(format!("RREQ messages are 24 bytes, not {}", b.len())));
        }
        if b[0] != 1 {
            return Err(ParseError::new("This is not a RREQ message"));
        }
        Ok(RREQ {
            j: 1 << 7 & b[1] != 0,
            r: 1 << 6 & b[1] != 0,
            g: 1 << 5 & b[1] != 0,
            d: 1 << 4 & b[1] != 0,
            u: 1 << 3 & b[1] != 0,
            hop_count: b[3],
            rreq_id: u32::from_be_bytes(&b[4..8]),
            dest_ip: Ipv4Addr::new(b[8], b[9], b[10], b[11]),
            dest_seq_num: u32::from_be_bytes(&b[12..16]),
            orig_ip: Ipv4Addr::new(b[16], b[17], b[18], b[19]),
            orig_seq_num: u32::from_be_bytes(&b[20..24]),
        })
    }
    /// Return the bit field representation of a RREQ message
    pub fn bit_message(&self) -> Vec<u8> {
        let mut b = Vec::with_capacity(24);
        b.push(1);
        b.push(
            if self.j { 1 << 7 } else { 0 } + if self.r { 1 << 6 } else { 0 } +
            if self.g { 1 << 5 } else { 0 } + if self.d { 1 << 4 } else { 0 } +
            if self.u { 1 << 3 } else { 0 },
            );
        b.push(0); // Reserved space

        b.push(self.hop_count);

        b.extend(self.rreq_id.as_be_bytes().iter());

        b.extend(self.dest_ip.octets().iter());
        b.extend(self.dest_seq_num.as_be_bytes().iter());

        b.extend(self.orig_ip.octets().iter());
        b.extend(self.orig_seq_num.as_be_bytes().iter());

        b
    }
    pub fn handle_message(&mut self, addr: &SocketAddr) {
        unimplemented!();
    }
}

///RREP Message Format:
///0                   1                   2                   3
///0 1 2 3 4 5 6 7 8 9 0 1 2 3 4 5 6 7 8 9 0 1 2 3 4 5 6 7 8 9 0 1
///+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+
///|     Type      |R|A|    Reserved     |Prefix Sz|   Hop Count   |
///+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+
///|                     Destination IP Address                    |
///+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+
///|                  Destination Sequence Number                  |
///+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+
///|                    Originator IP Address                      |
///+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+
///|                           Lifetime                            |
///+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+
#[derive(Clone, Debug, PartialEq)]
pub struct RREP {
    pub r: bool, // Repair flag
    pub a: bool, // Acknowledgment required flag

    pub prefix_size: u8, // 5-bit prefix size
    pub hop_count: u8, // 8-bit Hop Count

    pub dest_ip: Ipv4Addr, //Destination IP
    pub dest_seq_num: u32, //Destination Sequence Number

    pub orig_ip: Ipv4Addr, //Originator IP

    pub lifetime: u32, //Lifetime in milliseconds
}

impl RREP {
    /// Return a RREP message from a byte slice
    pub fn new(b: &[u8]) -> Result<RREP, Error> {
        if b.len() != 20 {
            return Err(ParseError::new(format!("RREP messages are 20 bytes, not {}",b.len())));
        }
        if b[0] != 2 {
            return Err(ParseError::new("This is not a RREP message"));
        }
        Ok(RREP {
            r: 1 << 7 & b[1] != 0,
            a: 1 << 6 & b[1] != 0,
            prefix_size: b[2] % 32,
            hop_count: b[3],
            dest_ip: Ipv4Addr::new(b[4], b[5], b[6], b[7]),
            dest_seq_num: u32::from_be_bytes(&b[8..12]),
            orig_ip: Ipv4Addr::new(b[12], b[13], b[14], b[15]),
            lifetime: u32::from_be_bytes(&b[16..20]),
        })
    }
    /// Return the bit field representation of a RREP message
    pub fn bit_message(&self) -> Vec<u8> {
        let mut b = Vec::with_capacity(20);
        b.push(2);
        b.push(
            if self.r { 1 << 7 } else { 0 } + if self.a { 1 << 6 } else { 0 },
            );
        // TODO: fix this value!
        b.push(self.prefix_size % 32);
        b.push(self.hop_count);

        b.extend(self.dest_ip.octets().iter());
        b.extend(self.dest_seq_num.as_be_bytes().iter());
        b.extend(self.orig_ip.octets().iter());
        b.extend(self.lifetime.as_be_bytes().iter());

        b
    }
    pub fn handle_message(&mut self, addr: &SocketAddr) {
        unimplemented!();
    }
}

///RERR Message Format:
///0                   1                   2                   3
///0 1 2 3 4 5 6 7 8 9 0 1 2 3 4 5 6 7 8 9 0 1 2 3 4 5 6 7 8 9 0 1
///+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+
///|     Type      |N|          Reserved           |   DestCount   |
///+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+
///|            Unreachable Destination IP Address (1)             |
///+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+
///|         Unreachable Destination Sequence Number (1)           |
///+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+
///|  Additional Unreachable Destination IP Addresses (if needed)  |
///+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+
///|Additional Unreachable Destination Sequence Numbers (if needed)|
///+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+
#[derive(Clone, Debug, PartialEq)]
pub struct RERR {
    pub n: bool, // No delete flag

    pub dest_count: u8, // 8-bit Destination Count

    pub udest_list: Vec<(Ipv4Addr, // Unreachable Destination IP Address
                         u32)>, // Unreachable Destination Sequence Number
}

impl RERR {
    /// Return a RERR message from a byte slice
    pub fn new(b: &[u8]) -> Result<RERR, Error> {
        if (b.len()-4) % 8 != 0 || b.len() <12 {
            return Err(ParseError::new("This is not the right size for a RERR message"));
        }
        if b[0] != 3{
            return Err(ParseError::new("This message is not a RERR message"));
        }

        let mut udest_list = Vec::new();
        let mut i = 4;
        while i < b.len(){
            udest_list.push((Ipv4Addr::new(b[i],b[i+1],b[i+2],b[i+3]),
            u32::from_be_bytes(&b[i+4..i+8])));
            i+=8;
        }

        Ok(RERR{
            n: 1<<7&b[1]!=0,
            dest_count: udest_list.len() as u8,
            udest_list: udest_list,
        })
    }
    /// Return the bit field representation of a RERR message
    pub fn bit_message(&self) -> Vec<u8> {
        let mut b = Vec::with_capacity(4+8*self.dest_count as usize);
        b.push(3);
        b.push(if self.n {1<<7} else {0});
        b.push(0);
        b.push(self.dest_count);

        for i in 0..self.udest_list.len() as usize{
            // Add each ip address
            for bit in &self.udest_list[i].0.octets() {
                b.push(*bit);
            }
            // Add its sequence number
            for bit in &self.udest_list[i].1.as_be_bytes() {
                b.push(*bit)
            }
        }
        b
    }
    pub fn handle_message(&mut self, addr: &SocketAddr){
        unimplemented!();
    }
}

#[cfg(test)]
mod test_encoding {
    use super::*;

    #[test]
    fn test_rreq_encoding() {
        let rreq = RREQ {
            j: true,
            r: false,
            g: true,
            d: false,
            u: true,
            hop_count: 144,
            rreq_id: 14425,
            dest_ip: Ipv4Addr::new(192, 168, 10, 14),
            dest_seq_num: 12,
            orig_ip: Ipv4Addr::new(192, 168, 10, 19),
            orig_seq_num: 63,
        };

        let bytes: &[u8] = &[
                1, 168, 0, 144, 0, 0, 56, 89, 192, 168, 10, 
                14, 0, 0, 0, 12, 192, 168, 10, 19, 0, 0, 0, 63,
            ];
        assert_eq!(bytes.to_vec(), rreq.bit_message());
        assert_eq!(rreq, RREQ::new(bytes).unwrap())
    }

    #[test]
    fn test_rrep_encoding() {
        let rrep = RREP {
            r: true,
            a: false,
            prefix_size: 31,
            hop_count: 98,
            dest_ip: Ipv4Addr::new(192, 168, 10, 14),
            dest_seq_num: 12,
            orig_ip: Ipv4Addr::new(192, 168, 10, 19),
            lifetime: 32603,
        };

        let bytes: &[u8] = &[
                2, 128, 31, 98, 192, 168, 10, 14, 0, 0, 0, 12, 192, 168, 10, 19, 0, 0, 127, 91,
            ];

        assert_eq!(bytes.to_vec(), rrep.bit_message());
        assert_eq!(rrep, RREP::new(bytes).unwrap())
    }

    #[test]
    fn test_rerr_encoding() {
        let mut udest_list = Vec::with_capacity(3);
        udest_list.push((Ipv4Addr::new(192,168,10,18), 482755));
        udest_list.push((Ipv4Addr::new(255,255,255,255), 0));
        let rerr = RERR {
            n: false,
            dest_count: 2,
            udest_list: udest_list,
        };
        let bytes: &[u8] = &[
            3, 0, 0, 2, 192, 168, 10, 18, 0, 7,
            93, 195, 255, 255, 255, 255, 0, 0, 0, 0
        ];
        assert_eq!(bytes, rerr.bit_message().as_slice());
        assert_eq!(rerr, RERR::new(bytes).unwrap());

        let mut udest_list = Vec::with_capacity(3);
        udest_list.push((Ipv4Addr::new(192,168,10,18), 482755));
        udest_list.push((Ipv4Addr::new(255,255,255,255), 0));
        udest_list.push((Ipv4Addr::new(192,168,10,15), 58392910));
        let rerr = RERR {
            n: false,
            dest_count: 3,
            udest_list: udest_list,
        };
        let bytes: &[u8] = &[
            3, 0, 0, 3, 192, 168, 10, 18, 0, 7,
            93, 195, 255, 255, 255, 255, 0, 0, 0, 0, 192, 168, 10, 15, 3, 123, 1, 78
        ];

        assert_eq!(bytes, rerr.bit_message().as_slice());
        assert_eq!(rerr, RERR::new(bytes).unwrap());
    }
}
