use std::collections::HashMap;
use std::sync::{Mutex, Arc};
use std::time::Duration;
use std::net::Ipv4Addr;
use std::ops::Deref;
use functions::*;

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

pub struct RoutingTable(Arc<Mutex<HashMap<Ipv4Addr, String>>>);

impl RoutingTable {
    pub fn new() -> Self {
        RoutingTable(Arc::new(Mutex::new(HashMap::new())))
    }
}

pub struct Route {
    dest_ip: Ipv4Addr,
    dest_seq_num: u32,
    valid_dest_seq_num: bool,
    valid: bool,
    interface: String,
    hop_count: u8,
    next_hop: Ipv4Addr,
    precursors: Vec<Ipv4Addr>,
    lifetime: Duration,
    //lifetimeChannel chan bool
}
