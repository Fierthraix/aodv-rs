use std::io::Error;
use std::net::{IpAddr, Ipv4Addr, SocketAddr};
use std::time::Duration;
use std::collections::hash_map::Entry::{Occupied, Vacant};

use aodv::*;
use super::*;
use functions::*;
use routing::Route;

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
        b.push(self.prefix_size);
        b.push(self.hop_count);

        b.extend(self.dest_ip.octets().iter());
        b.extend(self.dest_seq_num.as_be_bytes().iter());
        b.extend(self.orig_ip.octets().iter());
        b.extend(self.lifetime.as_be_bytes().iter());

        b
    }
    pub fn handle_message(&mut self, addr: &SocketAddr) -> Option<(SocketAddr, AodvMessage)> {
        //TODO: fix this!
        // This is likely a hello message
        if self.dest_ip == self.orig_ip {
            return None;
        }

        println!("Received RREP from {} for {}", addr.to_ipv4(), self.orig_ip);

        //TODO: if addr == config.BroadcastAddress we know this is a hello
        //      message and should handle it appropriately

        //NOTE: in this section 'destination' refers to the node that created
        //      the RREP, and the 'originating node' is receiving the RREP

        let mut db = routing_table.lock();

        // If we don't already have a route, create one based on what we know
        //TODO: this is wrong, fix it! (use `.set_route()`)
        if let Vacant(r) = db.entry(addr.to_ipv4()) {
            //TODO: make sure all this is right
            r.insert(Route {
                dest_ip: self.orig_ip,
                dest_seq_num: self.dest_seq_num,
                valid_dest_seq_num: false,
                valid: false,
                interface: config.interface.clone(),
                hop_count: self.hop_count,
                next_hop: addr.to_ipv4(),
                precursors: Vec::new(),
                lifetime: Duration::from_millis(0),
            });
        }

        self.hop_count += 1;

        let mut dest_route_changed = false;

        let mut dest_route = match db.entry(self.dest_ip) {
            Vacant(r) => {
                dest_route_changed = true;
                Route {
                    dest_ip: self.orig_ip,
                    dest_seq_num: self.dest_seq_num,
                    valid_dest_seq_num: false,
                    valid: false,
                    interface: config.interface.clone(),
                    hop_count: self.hop_count,
                    next_hop: addr.to_ipv4(),
                    precursors: Vec::new(),
                    lifetime: Duration::from_millis(0),
                }
            }
            Occupied(r) => r.get().clone(),
        };

        // Update the Destination Sequence Number if you need to
        if !dest_route.valid_dest_seq_num ||
            (dest_route.valid_dest_seq_num && self.dest_seq_num > dest_route.dest_seq_num) ||
            (dest_route.dest_seq_num == self.dest_seq_num && !dest_route.valid) ||
            (dest_route.dest_seq_num == self.dest_seq_num &&
                self.hop_count < dest_route.hop_count)
        {
            dest_route.dest_seq_num = self.dest_seq_num;
            dest_route_changed = true;
        }

        // If the forward route is created/modified, run this:
        if dest_route_changed {
            dest_route.valid = true;
            dest_route.valid_dest_seq_num = true;
            dest_route.next_hop = addr.to_ipv4();
            dest_route.hop_count = self.hop_count;
            dest_route.lifetime = Duration::from_millis(u64::from(self.lifetime));
            dest_route.dest_seq_num = self.dest_seq_num;

            println!("Putting changed route {}", dest_route.dest_ip);
        }

        drop(db);

        routing_table.put_route(dest_route);

        // If you're not the originator node, then forward the RREP and exit
        if config.current_ip != self.orig_ip && dest_route_changed {
            let db = routing_table.lock();

            let orig_route = db.get(&self.orig_ip).unwrap().clone();
            drop(db);

            println!(
                "Forwarding RREP meant for {} to {}",
                self.orig_ip,
                orig_route.next_hop
            );

            let mut db = routing_table.lock();
            //TODO: fix this
            if let Occupied(mut r) = db.entry(self.dest_ip) {
                r.get_mut().precursors.push(self.dest_ip);
            }

            //TODO: message route used


            //TODO: generalize this!
            return Some((
                SocketAddr::new(IpAddr::V4(orig_route.next_hop), 654),
                AodvMessage::Rrep(self.clone()),
            ));
        }

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
