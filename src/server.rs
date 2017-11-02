use std::io;
use std::net::SocketAddr;

use futures::{Future, Poll};
use tokio_core::net::UdpSocket;
use tokio_core::reactor::Core;

use parse::Config;
use routing::RoutingTable;

pub fn aodv(config: &Config, routing_table: RoutingTable) {

    //TODO: Check port is being used

    //TODO: Handle messages from spun up instances

}

pub fn server(config: &Config) {
    //TODO: bind udp port and listen

    //TODO: handle received aodv control messages
}

struct Server;
