use aodv::*;

use std::io::Error;
use std::net::{Ipv4Addr, SocketAddr};
use functions::*;

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
            rreq_id: bytes_as_u32_be(&b[4..8]),
            dest_ip: Ipv4Addr::new(b[8], b[9], b[10], b[11]),
            dest_seq_num: bytes_as_u32_be(&b[12..16]),
            orig_ip: Ipv4Addr::new(b[16], b[17], b[18], b[19]),
            orig_seq_num: bytes_as_u32_be(&b[20..24]),
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

        b.extend(u32_as_bytes_be(self.rreq_id).iter());

        b.extend(self.dest_ip.octets().iter());
        b.extend(u32_as_bytes_be(self.dest_seq_num).iter());

        b.extend(self.orig_ip.octets().iter());
        b.extend(u32_as_bytes_be(self.orig_seq_num).iter());

        b
    }
    //TODO: Implement this!
    pub fn handle_message(&self, addr: &SocketAddr) -> Option<(SocketAddr, AodvMessage)> {
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
use std::sync::{Arc, Mutex, MutexGuard};

// TODO: remove and use futures and timouts instead
use std::thread;
use std::time::Duration;

#[derive(Clone)]
pub struct RreqDatabase(Arc<Mutex<HashMap<Ipv4Addr, Vec<u32>>>>);

impl RreqDatabase {
    pub fn new() -> Self {
        RreqDatabase(Arc::new(Mutex::new(HashMap::new())))
    }

    /// Returns a bool for whether or not a particular RREQ ID has been seen before and keeps track
    /// of it for PATH_DISCOVERY_TIME
    pub fn seen_before(&mut self, ip: Ipv4Addr, rreq_id: u32) -> bool {
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
                // TODO: use futures or something instead of a thread
                let _db = self.clone();
                thread::spawn(move || { RreqDatabase::manage_rreq(ip, rreq_id, _db); });
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
