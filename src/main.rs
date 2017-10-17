mod aodv;
mod parse;
mod server;
mod rreq;
mod rrep;
mod rerr;
mod functions;

use aodv::*;

fn main() {
    let args = parse::get_args();

    if args.is_present("start_aodv") {
        //go server()
        //go tcpServer()
        server::aodv();
    }
}
