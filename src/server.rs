extern crate tokio;
extern crate tokio_io;
extern crate futures;

use std::net::SocketAddr;

use self::tokio::executor::current_thread;
use self::tokio::net::{UdpSocket, UdpFramed};
use self::futures::stream::Stream;
use self::futures::future;

use super::{AodvCodec, AODV_PORT};

pub fn aodv() {
    // Bind to the AODV port
    let addr = SocketAddr::new("0.0.0.0".parse().unwrap(), AODV_PORT);
    let socket = UdpSocket::bind(&addr).unwrap();
    println!("Started listening on {}", AODV_PORT);

    let (_sink, stream) = UdpFramed::new(socket, AodvCodec).split();

    let stream = stream
        .map_err(|err| eprintln!("{}", err)) // BUG: Crashes when malformed packet is sent
        .for_each(|(addr, msg)| {
            println!("{:?}", addr);
            println!("{:?}", msg);
            future::ok(())
        });

    current_thread::run(|_| {
        current_thread::spawn(stream);
    })
}
