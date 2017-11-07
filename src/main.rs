extern crate futures;
extern crate tokio_core;
#[macro_use]
extern crate lazy_static;

use std::thread;
use std::env::var;
use std::sync::Arc;
use std::process::exit;

use parse::Config;
use routing::RoutingTable;
use rreq::RreqDatabase;

mod aodv;
mod parse;
mod server;
mod rreq;
mod rrep;
mod rerr;
mod functions;
mod routing;

#[allow(non_upper_case_globals)]
lazy_static!{
    static ref routing_table: RoutingTable = RoutingTable::new();
    static ref rreq_database: RreqDatabase = RreqDatabase::new();
    static ref config: Config = Config::new(&parse::get_args());
}

fn main() {
    // Get command line arguments
    let args = parse::get_args();

    // Start server
    if args.is_present("start_aodv") {

        // Check user is root
        match var("USER") {
            Ok(s) => {
                if s != "root" {
                    eprintln!("Must be root to run the server!");
                    exit(1);
                }
            }
            Err(e) => panic!(e),
        }

        // Start internal server
        let handle = thread::spawn(|| { server::server(); });

        //go tcpServer()

        server::aodv();

        handle.join().unwrap();
    }
}
