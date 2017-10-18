extern crate futures;
#[macro_use]
extern crate tokio_core;

use std::{env, io};
use std::net::SocketAddr;
use std::net::Ipv4Addr;
use std::sync::{Mutex, Arc};
use std::collections::HashMap;

use futures::{Future, Poll};
use tokio_core::net::UdpSocket;
use tokio_core::reactor::Core;


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

    if args.is_present("start_aodv") {

        // Initialize routing table here; clone for each function/thread it's needed in
        let routing_table = routing::RoutingTable::new();

        //go server()
        //go tcpServer()
        //server::aodv();
    }
}
