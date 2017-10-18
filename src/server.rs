use std::{env, io};
use std::net::SocketAddr;

use futures::{Future, Poll};
use tokio_core::net::UdpSocket;
use tokio_core::reactor::Core;

pub fn aodv() {}

pub fn server() {}

struct Server;

/*
impl Future for Server {
    type Item = ();
    type Error = io::Error;

    fn poll(&mut self) -> Poll<(), io::Error> {
        Ok(())
    }
}
*/
