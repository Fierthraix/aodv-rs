extern crate futures;
extern crate tokio_core;

use std::thread;
use std::sync::Arc;

use parse::Config;
use routing::RoutingTable;

mod aodv;
mod parse;
mod server;
mod rreq;
mod rrep;
mod rerr;
mod functions;
mod routing;


fn main() {
    // Get command line arguments
    let args = parse::get_args();

    // Generate config object based on those
    let config = Arc::new(Config::new(&args));

    // Start server
    if args.is_present("start_aodv") {

        // Initialize routing table here; clone for each function/thread it's needed in
        let routing_table = RoutingTable::new();

        //TODO: use tokio or something
        // Start internal server
        let _config = Arc::clone(&config);
        let handle = thread::spawn(move || {
            let config = _config;
            server::server(&config);
        });

        //go tcpServer()

        server::aodv(&config, routing_table.clone());

        handle.join().unwrap();
    }
}
