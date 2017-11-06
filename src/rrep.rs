use aodv::*;

use std::io::Error;
use std::net::{Ipv4Addr, SocketAddr};
use functions::*;

/*
   RREP Message Format:
   0                   1                   2                   3
   0 1 2 3 4 5 6 7 8 9 0 1 2 3 4 5 6 7 8 9 0 1 2 3 4 5 6 7 8 9 0 1
   +-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+
   |     Type      |R|A|    Reserved     |Prefix Sz|   Hop Count   |
   +-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+
   |                     Destination IP Address                    |
   +-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+
   |                  Destination Sequence Number                  |
   +-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+
   |                    Originator IP Address                      |
   +-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+
   |                           Lifetime                            |
   +-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+
   */

#[derive(Debug, PartialEq)]
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
            //return Err("This byte message is not the right size");
            return Err(ParseError::new());
        }
        if b[0] != 2 {
            //return Err("This byte message is not the right type");
            return Err(ParseError::new());
        }
        Ok(RREP {
            r: 1 << 7 & b[1] != 0,
            a: 1 << 6 & b[1] != 0,
            // TODO: Fix prefix size!
            prefix_size: b[2],
            hop_count: b[3],
            dest_ip: Ipv4Addr::new(b[4], b[5], b[6], b[7]),
            dest_seq_num: bytes_as_u32_be(&b[8..12]),
            orig_ip: Ipv4Addr::new(b[12], b[13], b[14], b[15]),
            lifetime: bytes_as_u32_be(&b[16..20]),
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
        b.push(self.prefix_size);
        b.push(self.hop_count);

        b.extend(self.dest_ip.octets().iter());
        b.extend(u32_as_bytes_be(self.dest_seq_num).iter());
        b.extend(self.orig_ip.octets().iter());
        b.extend(u32_as_bytes_be(self.lifetime).iter());

        b
    }

    //TODO: Implement this!
    pub fn handle_message(&self, addr: &SocketAddr) -> Option<(SocketAddr, AodvMessage)> {
        None
    }
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
        2,
        128,
        31,
        98,
        192,
        168,
        10,
        14,
        0,
        0,
        0,
        12,
        192,
        168,
        10,
        19,
        0,
        0,
        127,
        91,
    ];

    assert_eq!(bytes.to_vec(), rrep.bit_message());
    assert_eq!(rrep, RREP::new(bytes).unwrap())
}
