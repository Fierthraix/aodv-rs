use aodv::*;

use std::io::Error;
use std::net::{Ipv4Addr, SocketAddr};
use functions::*;

/*
   RERR Message Format:
   0                   1                   2                   3
   0 1 2 3 4 5 6 7 8 9 0 1 2 3 4 5 6 7 8 9 0 1 2 3 4 5 6 7 8 9 0 1
   +-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+
   |     Type      |N|          Reserved           |   DestCount   |
   +-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+
   |            Unreachable Destination IP Address (1)             |
   +-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+
   |         Unreachable Destination Sequence Number (1)           |
   +-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+
   |  Additional Unreachable Destination IP Addresses (if needed)  |
   +-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+
   |Additional Unreachable Destination Sequence Numbers (if needed)|
   +-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+
   */

#[derive(Debug, PartialEq)]
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
            //return Err("This byte message is not the right size");
            return Err(ParseError::new());
        }
        if b[0] != 3{
            //return Err("This byte message is not the right type");
            return Err(ParseError::new());
        }

        let mut udest_list = Vec::new();
        let mut i = 4;
        while i < b.len(){
            udest_list.push((Ipv4Addr::new(b[i],b[i+1],b[i+2],b[i+3]),
            bytes_as_u32_be(&b[i+4..i+8])));
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
            for bit in self.udest_list[i].0.octets().iter() {
                b.push(*bit);
            }
            // Add its sequence number
            for bit in u32_as_bytes_be(self.udest_list[i].1).iter() {
                b.push(*bit)
            }
        }
        b
    }
    //TODO: Implement this!
    pub fn handle_message(&self, addr: &SocketAddr)->Option<(SocketAddr, AodvMessage)>{None}
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
