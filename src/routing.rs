use std::collections::{HashMap, HashSet};
use std::collections::hash_map::Entry::{Occupied, Vacant};
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

        match self.lock().entry(ip) {
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
        self.lock().insert(route.dest_ip, route);
    }
    /// Adds a precursor to the precursor list of a route
    pub fn add_precursor(&self, route: Ipv4Addr, precursor: Ipv4Addr) {
        if let Occupied(r) = self.lock().entry(route) {
            let r = r.into_mut();
            r.precursors.insert(precursor);
        }
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
    pub precursors: HashSet<Ipv4Addr>,
    pub lifetime: Duration,
    //lifetimeChannel chan bool
}

//TODO: make this AtomicUsize or RwLock or something
pub struct SequenceNumber(Mutex<u32>);

impl Default for SequenceNumber {
    fn default() -> Self {
        SequenceNumber(Mutex::new(0))
    }
}

impl SequenceNumber {
    pub fn get(&self) -> u32 {
        *self.0.lock().unwrap()
    }
    pub fn increment(&self) {
        *self.0.lock().unwrap() += 1;
    }
    pub fn increment_then_get(&self) -> u32 {
        let mut seq_num = self.0.lock().unwrap();
        *seq_num += 1;
        *seq_num
    }
}

#[cfg(test)]
mod test_sequence_number {
    use super::*;
    lazy_static!{
        static ref SEQ_NUM: SequenceNumber= SequenceNumber::default();
    }

    #[test]
    fn test_sequence_number_methods() {
        let a = SEQ_NUM.get();
        assert_eq!(a, 0);

        let b = SEQ_NUM.increment_then_get();
        assert_eq!(a + 1, b);

        SEQ_NUM.increment();
        assert_eq!(b + 1, SEQ_NUM.get());
    }
}

#[cfg(test)]
mod test_routing_table {
    use super::*;

    lazy_static!{
        static ref ROUTING_TABLE: RoutingTable = RoutingTable::new();
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
            precursors: HashSet::new(),
            lifetime: Duration::from_millis(0),
        };

        ROUTING_TABLE.set_route(r1.clone());

        // Check the route was inserted properly
        assert_eq!(*ROUTING_TABLE.lock().get(&r1.dest_ip).unwrap(), r1);

        // Re-add with invalid dest_seq_num
        let mut r2 = r1.clone();
        r2.valid_dest_seq_num = false;
        r2.dest_seq_num = 0;
        ROUTING_TABLE.set_route(r2.clone());

        // Check this route didn't supersede the old one
        assert_eq!(*ROUTING_TABLE.lock().get(&r2.dest_ip).unwrap(), r2);

        // Add new route with unknown dest_seq_num
        let r3 = Route {
            dest_ip: Ipv4Addr::new(192, 168, 10, 3),
            dest_seq_num: 0,
            valid_dest_seq_num: false,
            valid: true,
            interface: String::from("wlan0"),
            hop_count: 14,
            next_hop: Ipv4Addr::new(192, 168, 10, 4),
            precursors: HashSet::new(),
            lifetime: Duration::from_millis(0),
        };
        ROUTING_TABLE.set_route(r3.clone());

        // Overwrite it with a valid dest_seq_num
        let mut r4 = r3.clone();
        r4.dest_seq_num = 46;
        r4.valid_dest_seq_num = true;
        ROUTING_TABLE.set_route(r4.clone());

        // Check it was overwritten properly
        assert_eq!(*ROUTING_TABLE.lock().get(&r4.dest_ip).unwrap(), r4);

        // Check having a higher dest_seq_num overwrites
        let mut r5 = r4.clone();
        r5.dest_seq_num += 1;
        ROUTING_TABLE.set_route(r5.clone());
        assert_eq!(*ROUTING_TABLE.lock().get(&r5.dest_ip).unwrap(), r5);

        // Check same dest_seq_num, but lower hop count overwrites
        let mut r6 = r5.clone();
        r6.hop_count -= 2;
        ROUTING_TABLE.set_route(r6.clone());
        assert_eq!(*ROUTING_TABLE.lock().get(&r6.dest_ip).unwrap(), r6);
    }
}
