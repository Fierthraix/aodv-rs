use std::io::Error;
use std::net::{Ipv4Addr, SocketAddr};
use std::time::Duration;
use std::collections::hash_map::Entry::{Occupied, Vacant};

use aodv::*;
use functions::*;
use routing::Route;
use super::*;

/*
   RREQ Message Format:
   0                   1                   2                   3
   0 1 2 3 4 5 6 7 8 9 0 1 2 3 4 5 6 7 8 9 0 1 2 3 4 5 6 7 8 9 0 1
   +-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+
   |     Type      |J|R|G|D|U|   Reserved          |   Hop Count   |
   +-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+
   |                            RREQ ID                            |
   +-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+
   |                    Destination IP Address                     |
   +-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+
   |                  Destination Sequence Number                  |
   +-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+
   |                    Originator IP Address                      |
   +-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+
   |                  Originator Sequence Number                   |
   +-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+
   */

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
            //   return Err("This message is not the right size");
            return Err(ParseError::new());
        }
        if b[0] != 1 {
            //  return Err("This byte message is not the right type");
            return Err(ParseError::new());
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
    pub fn handle_message(&mut self, addr: &SocketAddr) -> Option<(SocketAddr, AodvMessage)> {
        // Create a reverse route to the sender of the RREQ
        routing_table.set_route(Route {
            dest_ip: addr.to_ipv4(),
            dest_seq_num: 0,
            valid_dest_seq_num: false,
            valid: false,
            interface: config.interface.clone(),
            hop_count: 1,
            next_hop: addr.to_ipv4(),
            precursors: Vec::new(),
            lifetime: Duration::from_millis(0),
        });

        // Disregard the RREQ and stop processing it if you've seen it before
        if rreq_database.seen_before(addr.to_ipv4(), self.rreq_id) {
            return None;
        }

        println!("Received new RREQ from {} for {}", addr, self.dest_ip);

        self.hop_count += 1;

        let mut db = routing_table.lock();

        let minimal_lifetime = config.NET_TRAVERSAL_TIME * 2 -
            config.NODE_TRAVERSAL_TIME * (2 * self.hop_count as u32);

        //TODO Ensure inserts are in here properly
        // Make a reverse route for the Originating IP address
        match db.entry(self.orig_ip) {
            Vacant(r) => {
                // If we don't already have a route, create one based on what we know
                r.insert(Route {
                    dest_ip: self.orig_ip,
                    dest_seq_num: self.dest_seq_num,
                    valid_dest_seq_num: true,
                    valid: true,
                    interface: config.interface.clone(),
                    hop_count: self.hop_count,
                    next_hop: addr.to_ipv4(),
                    precursors: Vec::new(),
                    lifetime: minimal_lifetime,
                });
            }
            Occupied(r) => {
                let r = r.into_mut();

                // Set the sequence number for the reverse route from the RREQ if the one in
                // the routing table is smaller or invalid
                if (r.valid_dest_seq_num && self.orig_seq_num > r.dest_seq_num) ||
                    !r.valid_dest_seq_num
                {
                    r.dest_seq_num = self.orig_seq_num
                }
                // Update the route with the latest info from the RREQ
                r.valid_dest_seq_num = true;
                r.next_hop = addr.to_ipv4();
                r.hop_count = self.hop_count;

                if r.lifetime < minimal_lifetime {
                    r.lifetime = minimal_lifetime;
                }
            }
        };

        //TODO Add logic to get decide to send RREP or not
        None
    }
}

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
        1,
        168,
        0,
        144,
        0,
        0,
        56,
        89,
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
        0,
        63,
    ];
    assert_eq!(bytes.to_vec(), rreq.bit_message());
    assert_eq!(rreq, RREQ::new(bytes).unwrap())
}

use std::collections::HashMap;
use std::sync::{Mutex, MutexGuard};

// TODO: remove and use futures and timouts instead
use std::thread;

pub struct RreqDatabase(Mutex<HashMap<Ipv4Addr, Vec<u32>>>);

impl RreqDatabase {
    pub fn new() -> Self {
        RreqDatabase(Mutex::new(HashMap::new()))
    }

    /// Returns a bool for whether or not a particular RREQ ID has been seen before and keeps track
    /// of it for PATH_DISCOVERY_TIME
    pub fn seen_before(&self, ip: Ipv4Addr, rreq_id: u32) -> bool {
        let mut db = self.lock();

        // Try to get the entry for the given ip
        if !db.contains_key(&ip) {
            db.insert(ip, vec![rreq_id]);
            false
        } else {
            let v = db.get_mut(&ip).unwrap(); // Unwrap is ok as we just checked for existence
            if v.contains(&rreq_id) {
                true
            // If you haven't seen an ip then add it and begin it's removal timer
            } else {
                v.push(rreq_id);
                /*
                // TODO: use futures or something instead of a thread
                let _db = self.clone();
                thread::spawn(move || { RreqDatabase::manage_rreq(ip, rreq_id, _db); });
                */
                false
            }
        }
    }

    fn manage_rreq(ip: Ipv4Addr, rreq_id: u32, db: RreqDatabase) {
        //TODO: Replace with lookup in config object
        thread::sleep(Duration::from_millis(500));
        let mut db = db.lock();

        // Scoped to remove reference to db and allow cleanup code to run
        {
            let v = db.get_mut(&ip).unwrap(); // This unwrap *shoudln't* fail, not 100% sure tho

            // Remove the current rreq_id from the list
            v.retain(|id| id != &rreq_id); // Keep elements that *aren't* equal to rreq_id
        }

        // Clean up empty hash maps
        if db.get_mut(&ip).unwrap().len() == 0 {
            db.remove(&ip);
        }
    }

    fn lock(&self) -> MutexGuard<HashMap<Ipv4Addr, Vec<u32>>> {
        match self.0.lock() {
            Ok(r) => r,
            Err(_) => panic!("Error locking rreq database!"),
        }
    }
}
