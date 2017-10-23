extern crate futures;
extern crate tokio_core;

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
    let config = Config::new(&args);

    // Start server
    if args.is_present("start_aodv") {

        // Initialize routing table here; clone for each function/thread it's needed in
        let routing_table = RoutingTable::new();

        server::server(&config);
        //go tcpServer()

        server::aodv(&config, routing_table.clone());
    }
}
