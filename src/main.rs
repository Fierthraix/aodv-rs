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
    let args = parse::get_args();

    let config = Config::new(&args);

    if args.is_present("start_aodv") {

        // Initialize routing table here; clone for each function/thread it's needed in
        let routing_table = RoutingTable::new();

        //go server()
        //go tcpServer()
        //server::aodv();
    }
}
