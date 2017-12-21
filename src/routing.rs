extern crate futures;
extern crate tokio_timer;

use std::collections::{HashMap, HashSet};
use std::collections::hash_map::Entry::{Occupied, Vacant};
use std::sync::{Mutex, MutexGuard};
use std::time::Duration;
use std::net::Ipv4Addr;

use super::*;

use futures::prelude::*;
use self::tokio_timer::*;

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
    /// Called when a route is used so we reset the timer that makes it invalid
    pub fn used(&self, route: Ipv4Addr) {
        if let Occupied(r) = self.lock().entry(route) {
            unimplemented!()
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

pub struct RreqDatabase(Mutex<HashMap<Ipv4Addr, Vec<u32>>>);

impl RreqDatabase {
    pub fn new() -> Self {
        RreqDatabase(Mutex::new(HashMap::new()))
    }

    /// Returns a bool for whether or not a particular RREQ ID has been seen before and keeps track
    /// of it for PATH_DISCOVERY_TIME
    pub fn seen_before(&'static self, ip: Ipv4Addr, rreq_id: u32) -> bool {
        let mut seen_before = false;
        let mut need_to_manage = false;
        match self.lock().entry(ip) {
            // If the IP address has never sent a RREQ create an entry for it
            Vacant(r) => {
                r.insert(vec![rreq_id]);
                need_to_manage = true;
            }
            // If the IP address has sent an RREQ before check if it was this one
            Occupied(r) => {
                let r = r.into_mut();
                if r.contains(&rreq_id) {
                    seen_before = true;
                } else {
                    r.push(rreq_id);
                    println!("Sending seen before");
                    need_to_manage = true;
                    println!("Returned form fun with thing running in background");
                    seen_before = false;
                }
            }
        }
        if need_to_manage {
            self.manage_rreq(ip, rreq_id);
        }
        seen_before
    }

    fn manage_rreq(&'static self, ip: Ipv4Addr, rreq_id: u32) {
        //CORE::run(Timer::default().sleep(CONFIG.PATH_DISCOVERY_TIME).and_then(|_| {
        use std::thread;
        thread::spawn(move || {

            thread::sleep(CONFIG.PATH_DISCOVERY_TIME);

            let mut db = self.lock();

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
            //Ok(())
        });
        //})).unwrap();
    }

    fn lock(&self) -> MutexGuard<HashMap<Ipv4Addr, Vec<u32>>> {
        match self.0.lock() {
            Ok(r) => r,
            Err(e) => panic!("error locking rreq database: {}", e),
        }
    }
}

#[cfg(test)]
mod test_routing_table {
    use super::*;
    use config::{self, Config};

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

    #[test]
    fn test_lifetime_management() {
        use std::time::Duration;
        use std::thread::sleep;
        use std::collections::hash_map::Entry::{Occupied, Vacant};

        // Add test route
        let r1 = Route {
            dest_ip: Ipv4Addr::new(192, 168, 10, 42),
            dest_seq_num: 45641,
            valid_dest_seq_num: true,
            valid: true,
            interface: String::from("wlan0"),
            hop_count: 14,
            next_hop: Ipv4Addr::new(192, 168, 10, 44),
            precursors: HashSet::new(),
            lifetime: Duration::from_millis(0),
        };

        ROUTING_TABLE.set_route(r1.clone());

        // Wait to see if it's marked invalid when it should be
        sleep(Duration::from_millis(50));
        match ROUTING_TABLE.lock().entry(r1.dest_ip) {
            Occupied(r) => {
                assert!(!r.get().valid);
            }
            _ => panic!("There should be a routing table entry!"),
        }

        // Add a test route
        let r2 = Route {
            dest_ip: Ipv4Addr::new(192, 168, 10, 43),
            dest_seq_num: 45641,
            valid_dest_seq_num: true,
            valid: true,
            interface: String::from("wlan0"),
            hop_count: 14,
            next_hop: Ipv4Addr::new(192, 168, 10, 44),
            precursors: HashSet::new(),
            lifetime: Duration::from_millis(0),
        };
        ROUTING_TABLE.set_route(r2.clone());

        // Wait 2/3 of it's lifetime
        sleep(CONFIG.ACTIVE_ROUTE_TIMEOUT * 2 / 3);

        // Ping it to keep it alive
        ROUTING_TABLE.used(r2.dest_ip);

        sleep(CONFIG.ACTIVE_ROUTE_TIMEOUT * 2 / 3);

        // Check it is still alive
        match ROUTING_TABLE.lock().entry(r2.dest_ip) {
            Occupied(r) => {
                assert!(r.get().valid);
            }
            _ => panic!("There should be a routing table entry!"),
        }

        // Revive dead route and check it's alive
        ROUTING_TABLE.used(r1.dest_ip);
        match ROUTING_TABLE.lock().entry(r1.dest_ip) {
            Occupied(r) => {
                assert!(r.get().valid);
            }
            _ => panic!("There should be a routing table entry!"),
        }
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
mod test_rreq_database {
    use super::*;
    use std::net::Ipv4Addr;
    use std::time::Duration;
    use std::thread::sleep;

    #[test]
    fn test_methods() {
        let ip1 = Ipv4Addr::new(192, 168, 10, 52);

        // Receive RREQ once
        assert!(!RREQ_DATABASE.seen_before(ip1, 4343));

        // Receive same RREQ
        assert!(RREQ_DATABASE.seen_before(ip1, 4343));

        // Wait for RREQ to self-delete
        sleep(CONFIG.PATH_DISCOVERY_TIME * 3 / 2);

        // Check table was deleted properly
        assert!(RREQ_DATABASE.lock().get(&ip1).is_none());
    }
}
