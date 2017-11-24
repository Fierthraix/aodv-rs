use std::io::Error;
use std::iter::Iterator;
use std::net::{Ipv4Addr, SocketAddr};
use std::collections::HashSet;
use std::collections::hash_map::Entry::Occupied;

use aodv::*;
use super::*;
use functions::*;
use server::client;

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
    //TODO: Implement this!
    pub fn handle_message(&mut self, addr: &SocketAddr){
        println!("Received RERR from {}", addr.to_ipv4());

        // Get unreachable destinations that use this node as the next hop
        let udests: Vec<(Ipv4Addr, u32)> = self.udest_list.iter().filter_map(|&(ip, seq_num)|{
            for route in routing_table.lock().values() {
                if route.next_hop == ip {
                    return Some((ip, seq_num))
                }
            }
            None
        }).collect();

        // Send an RERR if you need to
        if let Some((addr, rerr))=  RERR::generate_rerr(udests) {
            client(addr, &rerr);
        }
    }
    fn generate_rerr(mut udests: Vec<(Ipv4Addr, u32)>) -> Option<(SocketAddr, AodvMessage)>{
        // Sort and remove consecutive duplicates (thus removing all duplicates)
        udests.sort();
        udests.dedup();

        // Don't forward the RERR if you don't need to
        if udests.is_empty() {
            return None;
        }


        // Unicast if only one node needs the RERR, broadcast otherwise
        let mut precursors: HashSet<Ipv4Addr> = HashSet::new();

        let mut latest_ip = Ipv4Addr::new(0,0,0,0);
        for udest in &udests {
            if let Occupied(r) = routing_table.lock().entry(udest.0) {
                for precursor in &r.get().precursors {
                    precursors.insert(*precursor);
                    latest_ip = *precursor;
                }
            }
            // If there is more than one person to send the RERR to, broadcast it!
            if precursors.len() > 1 {
                latest_ip = config.broadcast_address;
                break;
            }
        }
        if precursors.is_empty() {
            None // No one to send the RERR to
        } else {
            Some((latest_ip.to_aodv_sa(), AodvMessage::Rerr(RERR{
                n: false,
                dest_count: udests.len() as u8,
                udest_list: udests,
            })))
        }
    }
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
