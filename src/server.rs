use std::io;
use std::net::{Ipv4Addr, SocketAddr};

use futures::{Future, Poll, Sink, Stream};
use tokio_core::net::UdpSocket;
use tokio_core::reactor::Core;

use aodv::*;
use parse::Config;
use routing::RoutingTable;

const AODV_PORT: u16 = 654;
const INSTANCE_PORT: u16 = 15292;

/// Outward AOV server
pub fn aodv(config: &Config, routing_table: RoutingTable) {

    // Get address
    let addr = SocketAddr::new("0.0.0.0".parse().unwrap(), AODV_PORT);

    // Get new core/handle
    let mut core = Core::new().unwrap();
    let handle = core.handle();

    //  Bind to Address
    let socket = UdpSocket::bind(&addr, &handle).unwrap();
    println!("Started listening on {}", AODV_PORT);

    // Get sink/stream for AODV codec
    let (sink, stream) = socket.framed(AodvCodec).split();

    // Handle incoming AODV messages
    let stream = stream
        .map(|(addr, msg)| { msg.handle_message(addr); })
        .and_then(|_| Ok(()));

    // Start server
    //TODO: Get this working!
    //let server = core.run(stream);
}

/// Internal instance server
pub fn server(config: &Config) {

    // Get address
    let addr = SocketAddr::new("0.0.0.0".parse().unwrap(), INSTANCE_PORT);

    // Get new core/handle
    let mut core = Core::new().unwrap();
    let handle = core.handle();

    //TODO: Handle messages from spun up instances
}
