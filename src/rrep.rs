use std::io::Error;
use std::net::{IpAddr, Ipv4Addr, SocketAddr};
use std::time::Duration;
use std::collections::hash_map::Entry::{Occupied, Vacant};

use aodv::*;
use super::*;
use rreq::*;
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

        // If we don't already have a route to the previous hop create one based on what we know
        if !routing_table.lock().contains_key(&addr.to_ipv4()) {
            routing_table.set_route(Route {
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

        let mut dest_route = match routing_table.lock().entry(self.dest_ip) {
            Vacant(_) => {
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

        routing_table.put_route(dest_route);

        // If you're not the originator node, then forward the RREP and exit
        if config.current_ip != self.orig_ip && dest_route_changed {
            let orig_route = routing_table.lock().get(&self.orig_ip).unwrap().clone();

            println!(
                "Forwarding RREP meant for {} to {}",
                self.orig_ip,
                orig_route.next_hop
            );

            //TODO: fix this
            if let Occupied(mut r) = routing_table.lock().entry(self.dest_ip) {
                r.get_mut().precursors.push(self.dest_ip);
            }

            //TODO: message route used


            //TODO: generalize this!
            return Some((
                orig_route.next_hop.to_aodv_sa(),
                AodvMessage::Rrep(self.clone()),
            ));
        }

        None
    }
    pub fn generate_rrep(rreq: &RREQ) -> Option<(SocketAddr, AodvMessage)> {
        // If you are the destination send an RREP
        if rreq.dest_ip == config.current_ip {
            return Some((
                config.broadcast_address.to_aodv_sa(),
                AodvMessage::Rrep(RREP::create_rrep(rreq)),
            ));
        }
        // If you have a valid route and sequence number to the destination
        // send and RREP
        if let Occupied(r) = routing_table.lock().entry(rreq.dest_ip) {
            let r = r.get();
            if r.valid && r.valid_dest_seq_num && r.dest_seq_num >= rreq.dest_seq_num && !rreq.d {
                return Some((
                    config.broadcast_address.to_aodv_sa(),
                    //TODO: change this to the actual message
                    AodvMessage::Rrep(RREP::create_rrep(rreq)),
                ));
            }
        }
        // Otherwise don't send a RREP
        None
    }

    //TODO: Add all the proper generation logic
    fn create_rrep(rreq: &RREQ) -> RREP {
        let mut rrep = RREP {
            r: false,
            a: false,
            prefix_size: 0,
            hop_count: 0,
            dest_ip: Ipv4Addr::new(0, 0, 0, 0),
            dest_seq_num: 0,
            orig_ip: Ipv4Addr::new(0, 0, 0, 0),
            lifetime: 0,
        };

        //TODO: change this to be from the input
        let mut forward_route = Route {
            dest_ip: Ipv4Addr::new(192, 168, 10, 2),
            dest_seq_num: 45641,
            valid_dest_seq_num: true,
            valid: true,
            interface: String::from("wlan0"),
            hop_count: 14,
            next_hop: Ipv4Addr::new(192, 168, 10, 4),
            precursors: Vec::new(),
            lifetime: Duration::from_millis(0),
        };

        // If the current ip is one generating the RREP:
        if config.current_ip == rreq.dest_ip {
            let mut curr_seq_num = 17; // TODO: implement sequence number counting!
            // Increment Sequence number if RREQ SeqNum is one higher
            if rreq.dest_seq_num == curr_seq_num + 1 {
                curr_seq_num = 17 + 1; //TODO: increment_and_get
            }
            // Set RREP values
            rrep.dest_seq_num = curr_seq_num;
            rrep.lifetime = 94; //TODO: config.MY_ROUTE_TIMEOUT;
        } else {
            // Set the RREP values
            rrep.dest_seq_num = forward_route.dest_seq_num;
            rrep.hop_count = forward_route.hop_count;
            // Lifetime is how long until the forward route expires
            rrep.lifetime = 37;

            // Add next_hop to reverse route precursors
            routing_table.add_precursor(rreq.dest_ip, forward_route.next_hop);

            // Add node the RREQ just crome from to forward route precursors
            forward_route.precursors.push(Ipv4Addr::new(0, 1, 2, 3)); //TODO: addr
            forward_route.valid = true;

            routing_table.put_route(forward_route);
        }

        rrep
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
