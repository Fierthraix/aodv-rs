use std::{env, io};
use std::net::SocketAddr;

use futures::{Future, Poll};
use tokio_core::net::UdpSocket;
use tokio_core::reactor::Core;

use parse::Config;
use routing::RoutingTable;

pub fn aodv(config: &Config, routing_table: RoutingTable) {}

pub fn server(config: &Config) {}

struct Server;
