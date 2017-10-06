mod parse;
mod server;
mod rrep;
mod rreq;
mod rerr;
mod functions;

fn main() {
    let args = parse::get_args();

    if args.is_present("start_aodv") {
        //go server()
        //go tcpServer()
        server::aodv();
    }

}
