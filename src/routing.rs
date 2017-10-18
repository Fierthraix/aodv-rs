use std::collections::HashMap;
use std::sync::{Arc, Mutex, MutexGuard};
use std::time::Duration;
use std::net::Ipv4Addr;

#[derive(Clone)]
pub struct RoutingTable(Arc<Mutex<HashMap<Ipv4Addr, Route>>>);

impl RoutingTable {
    pub fn new() -> Self {
        RoutingTable(Arc::new(Mutex::new(HashMap::new())))
    }
    pub fn lock(&self) -> MutexGuard<HashMap<Ipv4Addr, Route>> {
        //TODO: Handle this unwrap
        self.0.lock().unwrap()
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
