extern crate tokio_core;

use std::io;
use std::io::Error;
use std::net::{Ipv4Addr, SocketAddr};
use std::time::Duration;
use std::collections::HashSet;
use std::collections::hash_map::Entry::{Occupied, Vacant};

use functions::*;
use routing::Route;
use server::client;
use super::*;


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
    pub fn handle_message(&mut self, addr: &SocketAddr) {
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
            return;
        }

        println!("Received new RREQ from {} for {}", addr, self.dest_ip);

        self.hop_count += 1;

        let minimal_lifetime = config.NET_TRAVERSAL_TIME * 2 -
            config.NODE_TRAVERSAL_TIME * (2 * u32::from(self.hop_count));

        //TODO Ensure inserts are in here properly
        // Make a reverse route for the Originating IP address
        match routing_table.lock().entry(self.orig_ip) {
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
    pub fn handle_message(&mut self, addr: &SocketAddr) {
        //TODO: fix this!
        // This is likely a hello message
        if self.dest_ip == self.orig_ip {
            return;
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
            client(
                orig_route.next_hop.to_aodv_sa(),
                &AodvMessage::Rrep(self.clone()),
                );
        }
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

// TODO: remove and use futures and timouts instead
use std::thread;
use std::collections::HashMap;
use std::sync::{Mutex, MutexGuard};

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
                // TODO: use futures or something instead of a thread
                thread::spawn(move || { RreqDatabase::manage_rreq(ip, rreq_id); });
                false
            }
        }
    }

    fn manage_rreq(ip: Ipv4Addr, rreq_id: u32) {

        //TODO replace sleep with a future
        thread::sleep(config.PATH_DISCOVERY_TIME);

        let mut db = rreq_database.lock();

        // Scoped to remove reference to db and allow cleanup code to run
        {
            let v = db.get_mut(&ip).unwrap(); // This unwrap *shoudln't* fail, not 100% sure tho

            // Remove the current rreq_id from the list
            v.retain(|id| id != &rreq_id); // Keep elements that *aren't* equal to rreq_id
        }

        // Clean up empty hash maps
        if db.get_mut(&ip).unwrap().is_empty() {
            db.remove(&ip);
        }
    }

    fn lock(&self) -> MutexGuard<HashMap<Ipv4Addr, Vec<u32>>> {
        match self.0.lock() {
            Ok(r) => r,
            Err(e) => panic!("error locking rreq database: {}", e),
        }
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
