use std::net::SocketAddr;

use futures::{future, Stream};
use tokio_core::net::UdpSocket;
use tokio_core::reactor::Core;

use super::*;

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
    let (_, stream) = socket.framed(AodvCodec).split();

    // Handle incoming AODV messages
    let stream = stream.filter_map(|msg| msg).for_each(|(addr, msg)| {
        msg.handle_message(&addr);
        future::ok(())
    });

    let _server = core.run(stream);
}

/// Internal instance server
pub fn server() {

    // Get address
    let _addr = SocketAddr::new("0.0.0.0".parse().unwrap(), INSTANCE_PORT);

    // Get new core/handle
    let _core = Core::new().unwrap();
    let _handle = _core.handle();

    //TODO: Handle messages from spun up instances
}

/// Send an aodv message on a socket address
pub fn client(s: SocketAddr, msg: &AodvMessage) {
    use std::net;
    let socket = net::UdpSocket::bind("0.0.0.0:0").unwrap();

    socket.send_to(msg.bit_message().as_ref(), s).unwrap();
}
