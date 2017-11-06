use std::net::{Ipv4Addr, SocketAddr};

use futures::{Future, Poll, Sink, Stream};
use tokio_core::net::UdpSocket;
use tokio_core::reactor::Core;

use aodv::*;
use super::*;

const AODV_PORT: u16 = 654;
const INSTANCE_PORT: u16 = 15292;

/// Outward AODV server
pub fn aodv() {

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

    //TODO: find out who server crashes on non-aodv messages

    // Handle incoming AODV messages
    let stream = stream
        .map(|(addr, msg)| {
            msg.handle_message(&addr)
        })
    // Send a reply if need be
    .filter(|ref outgoing_msg| outgoing_msg.is_some()) //TODO: get this using better iterator
    // Unwrap the option (which we know is some because of the filter)
    .map(|outgoing_msg| outgoing_msg.unwrap());

    let _server = core.run(stream.forward(sink).and_then(|_| Ok(())));
}

/// Internal instance server
pub fn server() {

    // Get address
    let addr = SocketAddr::new("0.0.0.0".parse().unwrap(), INSTANCE_PORT);

    // Get new core/handle
    let mut core = Core::new().unwrap();
    let handle = core.handle();

    //TODO: Handle messages from spun up instances
}
