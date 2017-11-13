use std::collections::HashMap;
use std::collections::hash_map::Entry::{Occupied, Vacant};
use std::sync::{Mutex, MutexGuard};
use std::time::Duration;
use std::net::Ipv4Addr;

use super::*;

/// The internal representation of the aodv routing table
pub struct RoutingTable(Mutex<HashMap<Ipv4Addr, Route>>);

impl RoutingTable {
    /// Instantiate the routing table
    pub fn new() -> Self {
        RoutingTable(Mutex::new(HashMap::new()))
    }
    /// Lock the routing table for writing
    pub fn lock(&self) -> MutexGuard<HashMap<Ipv4Addr, Route>> {
        //self.0.lock().unwrap()
        match self.0.lock() {
            Ok(r) => r,
            Err(e) => panic!("error locking routing table: {}", e),
        }
    }
    /// Adds or updates the route according to the rules in section 6.2
    pub fn set_route(&self, route: Route) {
        let ip = route.dest_ip;
        // Don't add a route to yourself
        if config.current_ip == ip {
            return;
        }
        let mut db = self.lock();

        match db.entry(ip) {
            // Insert route if it doesn't exist
            Vacant(r) => {
                r.insert(route);
            }
            Occupied(r) => {
                let old_route = r.into_mut();
                // If it does exist, make sure none of these are true before replacing
                if !(!old_route.valid_dest_seq_num && route.dest_seq_num > old_route.dest_seq_num &&
                         (old_route.dest_seq_num == route.dest_seq_num &&
                              route.hop_count + 1 < old_route.hop_count))
                {
                    *old_route = route;
                };
            }
        };
    }
    /// Adds the route to the routing table, superseding the old one if it exists
    pub fn put_route(&self, route: Route) {
        // Don't add a route to yourself
        if config.current_ip == route.dest_ip {
            return;
        }
        let mut db = self.lock();
        db.insert(route.dest_ip, route);
    }
}

// TODO remove this `#allow[]`
#[allow(dead_code)]
#[derive(Clone, Debug, PartialEq)]
pub struct Route {
    pub dest_ip: Ipv4Addr,
    pub dest_seq_num: u32,
    pub valid_dest_seq_num: bool,
    pub valid: bool,
    pub interface: String,
    pub hop_count: u8,
    pub next_hop: Ipv4Addr,
    pub precursors: Vec<Ipv4Addr>,
    pub lifetime: Duration,
    //lifetimeChannel chan bool
}

#[test]
fn test_routing_table_methods() {
    let r1 = Route {
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

    routing_table.set_route(r1.clone());

    // Check the route was inserted properly
    {
        let db = routing_table.lock();
        assert_eq!(*db.get(&r1.dest_ip).unwrap(), r1);
    }

    // Re-add with invalid dest_seq_num
    let mut r2 = r1.clone();
    r2.valid_dest_seq_num = false;
    r2.dest_seq_num = 0;
    routing_table.set_route(r2.clone());

    // Check this route didn't supersede the old one
    {
        let db = routing_table.lock();
        assert_eq!(*db.get(&r2.dest_ip).unwrap(), r2);
    }

    // Add new route with unknown dest_seq_num
    let r3 = Route {
        dest_ip: Ipv4Addr::new(192, 168, 10, 3),
        dest_seq_num: 0,
        valid_dest_seq_num: false,
        valid: true,
        interface: String::from("wlan0"),
        hop_count: 14,
        next_hop: Ipv4Addr::new(192, 168, 10, 4),
        precursors: Vec::new(),
        lifetime: Duration::from_millis(0),
    };
    routing_table.set_route(r3.clone());

    // Overwrite it with a valid dest_seq_num
    let mut r4 = r3.clone();
    r4.dest_seq_num = 46;
    r4.valid_dest_seq_num = true;
    routing_table.set_route(r4.clone());

    // Check it was overwritten properly
    {
        let db = routing_table.lock();
        assert_eq!(*db.get(&r4.dest_ip).unwrap(), r4);
    }

    // Check having a higher dest_seq_num overwrites
    let mut r5 = r4.clone();
    r5.dest_seq_num += 1;
    routing_table.set_route(r5.clone());
    {
        let db = routing_table.lock();
        assert_eq!(*db.get(&r5.dest_ip).unwrap(), r5);
    }

    // Check same dest_seq_num, but lower hop count overwrites
    let mut r6 = r5.clone();
    r6.hop_count -= 2;
    routing_table.set_route(r6.clone());
    {
        let db = routing_table.lock();
        assert_eq!(*db.get(&r6.dest_ip).unwrap(), r6);
    }
}
