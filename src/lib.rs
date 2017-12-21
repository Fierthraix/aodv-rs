#[macro_use]
extern crate lazy_static;
extern crate tokio_core;
extern crate futures;

use std::io;
use std::io::Error;
use std::net::{Ipv4Addr, SocketAddr};
use std::time::Duration;
use std::collections::HashSet;
use std::collections::hash_map::Entry::{Occupied, Vacant};

pub mod util;
pub mod routing;
pub mod server;
pub mod config;

use util::*;
use routing::*;
use server::client;
use config::Config;

lazy_static!{
    static ref ROUTING_TABLE: RoutingTable = RoutingTable::new();
    static ref CONFIG: Config = Config::new(&config::get_args());
    static ref RREQ_DATABASE: RreqDatabase = RreqDatabase::new();
    static ref SEQ_NUM: SequenceNumber = SequenceNumber::default();
}

pub const AODV_PORT: u16 = 654;
pub const INSTANCE_PORT: u16 = 15_292;

use tokio_core::net::UdpCodec;

/// `ParseError` is an `io::Error` specifically for when parsing an aodv message fails
pub struct ParseError;

impl ParseError {
    pub fn new<E>(error: E) -> io::Error where E: Into<Box<std::error::Error + Send + Sync>>{
        io::Error::new(
            io::ErrorKind::InvalidInput,
            error,
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
        // Create a reverse route to the sender of the RREQ
        ROUTING_TABLE.set_route(Route {
            dest_ip: addr.to_ipv4(),
            dest_seq_num: 0,
            valid_dest_seq_num: false,
            valid: false,
            interface: CONFIG.interface.clone(),
            hop_count: 1,
            next_hop: addr.to_ipv4(),
            precursors: HashSet::new(),
            lifetime: Duration::from_millis(0),
        });

        // Disregard the RREQ and stop processing it if you've seen it before
        if RREQ_DATABASE.seen_before(addr.to_ipv4(), self.rreq_id) {
            return;
        }

        println!("Received new RREQ from {} for {}", addr, self.dest_ip);

        // Account for this hop
        self.hop_count += 1;

        let minimal_lifetime = CONFIG.NET_TRAVERSAL_TIME * 2 -
            CONFIG.NODE_TRAVERSAL_TIME * (2 * u32::from(self.hop_count));

        //TODO Ensure inserts are in here properly
        // Make or update the reverse route to the Originating IP address
        match ROUTING_TABLE.lock().entry(self.orig_ip) {
            // If we don't already have a route, create one based on what we know
            Vacant(r) => {
                r.insert(Route {
                    dest_ip: self.orig_ip,
                    dest_seq_num: self.dest_seq_num,
                    valid_dest_seq_num: true,
                    valid: true,
                    interface: CONFIG.interface.clone(),
                    hop_count: self.hop_count,
                    next_hop: addr.to_ipv4(),
                    precursors: HashSet::new(),
                    lifetime: minimal_lifetime,
                });
            }
            // Update the route if we need to
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

        // If you should generate a RREP do so, otherwise rebroadcast the RREQ
        if let Some((socket, rrep)) = RREP::generate_rrep(&self, addr) {
            client(socket, rrep.bit_message().as_ref())
        }else {
            client(CONFIG.broadcast_address.to_aodv_sa(), self.bit_message().as_ref());
        }
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
        // This is likely a hello message
        let mut is_hello = false;
        if self.dest_ip == self.orig_ip && *addr == CONFIG.broadcast_address.to_aodv_sa() {
            is_hello = true;
            println!("Received Hello from {}", addr.to_ipv4());
        }else{
            println!("Received RREP from {} for {}", addr.to_ipv4(), self.orig_ip);
        }

        //NOTE: in this section 'destination' refers to the node that created
        //      the RREP, and the 'originating node' is receiving the RREP

        // If we don't already have a route to the previous hop create one based on what we know
        if !ROUTING_TABLE.lock().contains_key(&addr.to_ipv4()) {
            ROUTING_TABLE.set_route(Route {
                dest_ip: self.orig_ip,
                dest_seq_num: self.dest_seq_num,
                valid_dest_seq_num: false,
                valid: false,
                interface: CONFIG.interface.clone(),
                hop_count: self.hop_count,
                next_hop: addr.to_ipv4(),
                precursors: HashSet::new(),
                lifetime: Duration::from_millis(0),
            });
        }

        // Account for this hop
        self.hop_count += 1;

        // If the forward route gets changed we need to modify it later
        let dest_route_changed = match ROUTING_TABLE.lock().entry(self.dest_ip) {
            Vacant(r) => {
                r.insert( Route {
                    dest_ip: self.orig_ip,
                    dest_seq_num: self.dest_seq_num,
                    valid_dest_seq_num: false,
                    valid: false,
                    interface: CONFIG.interface.clone(),
                    hop_count: self.hop_count,
                    next_hop: addr.to_ipv4(),
                    precursors: HashSet::new(),
                    lifetime: Duration::from_millis(0),
                });
                true
            }
            Occupied(mut r) => {
                let dest_route = r.get_mut();
                // Update the Destination Sequence Number if you need to
                if !dest_route.valid_dest_seq_num ||
                    (dest_route.valid_dest_seq_num && self.dest_seq_num > dest_route.dest_seq_num) ||
                        (dest_route.dest_seq_num == self.dest_seq_num && !dest_route.valid) ||
                        (dest_route.dest_seq_num == self.dest_seq_num &&
                         self.hop_count < dest_route.hop_count)
                        {
                            dest_route.valid = true;
                            dest_route.valid_dest_seq_num = true;
                            dest_route.next_hop = addr.to_ipv4();
                            dest_route.hop_count = self.hop_count;
                            dest_route.lifetime = Duration::from_millis(u64::from(self.lifetime));
                            dest_route.dest_seq_num = self.dest_seq_num;

                            println!("Putting changed route {}", dest_route.dest_ip);
                            true
                        } else {
                            false
                        }
            }
        };

        // Don't forward hello messages
        if is_hello {return}

        // If you're not the originator node, then forward the RREP and exit
        if CONFIG.current_ip != self.orig_ip && dest_route_changed {
            let orig_route = ROUTING_TABLE.lock().get(&self.orig_ip).unwrap().clone();

            println!(
                "Forwarding RREP meant for {} to {}",
                self.orig_ip,
                orig_route.next_hop
                );

            //TODO: fix this
            if let Occupied(mut r) = ROUTING_TABLE.lock().entry(self.dest_ip) {
                r.get_mut().precursors.insert(self.dest_ip);
            }

            //TODO: message route used


            //TODO: generalize this!
            client(
                orig_route.next_hop.to_aodv_sa(),
                self.bit_message().as_ref(),
                );
        }
    }
    /// Based on an RREQ return an RREP to send if we need to
    pub fn generate_rrep(rreq: &RREQ, addr: &SocketAddr) -> Option<(SocketAddr, Self)> {
        // If you are the destination send an RREP
        if rreq.dest_ip == CONFIG.current_ip {
            // Increment Sequence number if RREQ SeqNum is one higher
            let seq_num = if rreq.dest_seq_num == SEQ_NUM.get() + 1 {
                SEQ_NUM.increment_then_get()
            } else {
                SEQ_NUM.get()
            };

            // Set RREP values
            //TODO: set the destination value from the next hop!
            return Some((
                    *addr,
                    RREP {
                        r: false,
                        a: false,
                        prefix_size: 0,
                        hop_count: 0,
                        dest_ip: rreq.dest_ip,
                        dest_seq_num: seq_num,
                        orig_ip: rreq.orig_ip,
                        lifetime: CONFIG.MY_ROUTE_TIMEOUT.as_secs() as u32, //TODO: fix durations
                    }
                    ));
        }
        // If you have a valid route and sequence number to the destination send a RREP
        if let Occupied(mut r) = ROUTING_TABLE.lock().entry(rreq.dest_ip) {
            let fr = r.get_mut();
            if fr.valid && fr.valid_dest_seq_num && fr.dest_seq_num >= rreq.dest_seq_num && !rreq.d {
                // Update the forward route
                fr.valid = true;
                fr.precursors.insert(rreq.dest_ip); //TODO: Check this is the right value
                // Return the appropriate RREP
                return Some((
                        fr.dest_ip.to_aodv_sa(),
                        RREP {
                            r: false,
                            a: false,
                            prefix_size: 0,
                            hop_count: fr.hop_count,
                            dest_ip: rreq.dest_ip,
                            dest_seq_num: fr.dest_seq_num,
                            orig_ip: rreq.orig_ip,
                            lifetime: fr.lifetime.as_secs() as u32,
                        }
                        ));
            }
        }
        // Otherwise don't send a RREP
        None
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
    //TODO: Implement this!
    pub fn handle_message(&mut self, addr: &SocketAddr){
        println!("Received RERR from {}", addr.to_ipv4());

        // Get unreachable destinations that use this node as the next hop
        let udests: Vec<(Ipv4Addr, u32)> = self.udest_list.iter().filter_map(|&(ip, seq_num)|{
            for route in ROUTING_TABLE.lock().values() {
                if route.next_hop == ip {
                    return Some((ip, seq_num))
                }
            }
            None
        }).collect();

        // Send an RERR if you need to
        if let Some((addr, rerr))=  RERR::generate_rerr(udests) {
            client(addr, rerr.bit_message().as_ref());
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
            if let Occupied(r) = ROUTING_TABLE.lock().entry(udest.0) {
                for precursor in &r.get().precursors {
                    precursors.insert(*precursor);
                    latest_ip = *precursor;
                }
            }
            // If there is more than one person to send the RERR to, broadcast it!
            if precursors.len() > 1 {
                latest_ip = CONFIG.broadcast_address;
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
}
