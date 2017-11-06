use std::collections::HashMap;
use std::sync::{Mutex, MutexGuard};
use std::time::Duration;
use std::net::Ipv4Addr;

/// The internal representation of the aodv routing table
pub struct RoutingTable(Mutex<HashMap<Ipv4Addr, Route>>);

impl RoutingTable {
    /// Instantiate the routing table
    pub fn new() -> Self {
        RoutingTable(Mutex::new(HashMap::new()))
    }
    pub fn lock(&self) -> MutexGuard<HashMap<Ipv4Addr, Route>> {
        //self.0.lock().unwrap()
        match self.0.lock() {
            Ok(r) => r,
            Err(_) => panic!("Error locking Routing Table"),
        }
    }
}

// TODO remove this `#allow[]`
#[allow(dead_code)]
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
