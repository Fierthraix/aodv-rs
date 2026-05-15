use std::collections::{BTreeMap, BTreeSet};
use std::io;
use std::net::Ipv4Addr;
use std::time::{Duration, Instant};

use aodv::{Action, Config, Engine, IncomingPacket, Message, RouteState, SendAction, SendTarget};
use tokio::net::UdpSocket;

struct UdpNode {
    socket: UdpSocket,
    engine: Engine,
}

struct UdpMesh {
    now: Instant,
    nodes: BTreeMap<Ipv4Addr, UdpNode>,
    ports: BTreeMap<u16, Ipv4Addr>,
    links: BTreeMap<Ipv4Addr, BTreeSet<Ipv4Addr>>,
    action_log: Vec<(Ipv4Addr, Action)>,
}

impl UdpMesh {
    async fn new(ips: &[Ipv4Addr]) -> io::Result<Self> {
        let mut nodes = BTreeMap::new();
        let mut ports = BTreeMap::new();

        for &ip in ips {
            let socket = UdpSocket::bind("127.0.0.1:0").await?;
            let port = socket.local_addr()?.port();
            ports.insert(port, ip);
            nodes.insert(
                ip,
                UdpNode {
                    socket,
                    engine: Engine::new(test_config(ip)),
                },
            );
        }

        Ok(Self {
            now: Instant::now(),
            nodes,
            ports,
            links: BTreeMap::new(),
            action_log: Vec::new(),
        })
    }

    fn link(&mut self, a: Ipv4Addr, b: Ipv4Addr) {
        self.links.entry(a).or_default().insert(b);
        self.links.entry(b).or_default().insert(a);
    }

    fn unlink(&mut self, a: Ipv4Addr, b: Ipv4Addr) {
        if let Some(neighbors) = self.links.get_mut(&a) {
            neighbors.remove(&b);
        }
        if let Some(neighbors) = self.links.get_mut(&b) {
            neighbors.remove(&a);
        }
    }

    fn node(&self, ip: Ipv4Addr) -> &Engine {
        &self.nodes.get(&ip).unwrap().engine
    }

    async fn start_discovery(&mut self, from: Ipv4Addr, destination: Ipv4Addr) -> io::Result<()> {
        let actions = self
            .nodes
            .get_mut(&from)
            .unwrap()
            .engine
            .start_route_discovery(destination, self.now);
        self.apply_actions(from, actions).await
    }

    async fn handle_link_loss(&mut self, node: Ipv4Addr, next_hop: Ipv4Addr) -> io::Result<()> {
        let actions = self
            .nodes
            .get_mut(&node)
            .unwrap()
            .engine
            .handle_link_loss(next_hop, self.now);
        self.apply_actions(node, actions).await
    }

    async fn run_until_idle(&mut self, max_steps: usize) -> io::Result<()> {
        for _ in 0..max_steps {
            let mut progressed = false;

            for ip in self.nodes.keys().copied().collect::<Vec<_>>() {
                if let Some(deadline) = self.nodes.get(&ip).unwrap().engine.next_deadline(self.now)
                    && deadline <= self.now
                {
                    let actions = self.nodes.get_mut(&ip).unwrap().engine.tick(self.now);
                    self.apply_actions(ip, actions).await?;
                    progressed = true;
                }
            }

            tokio::task::yield_now().await;

            for ip in self.nodes.keys().copied().collect::<Vec<_>>() {
                loop {
                    let recv = {
                        let node = self.nodes.get_mut(&ip).unwrap();
                        let mut buffer = [0_u8; 2048];
                        match node.socket.try_recv_from(&mut buffer) {
                            Ok((size, source_addr)) => Some((buffer, size, source_addr)),
                            Err(error) if error.kind() == io::ErrorKind::WouldBlock => None,
                            Err(error) => return Err(error),
                        }
                    };

                    let Some((buffer, size, source_addr)) = recv else {
                        break;
                    };

                    let source_ip = self.ports[&source_addr.port()];
                    let message = Message::decode(&buffer[..size]).unwrap();
                    let actions = self.nodes.get_mut(&ip).unwrap().engine.handle_incoming(
                        IncomingPacket {
                            source: source_ip,
                            ttl: None,
                            message,
                        },
                        self.now,
                    );
                    self.apply_actions(ip, actions).await?;
                    progressed = true;
                }
            }

            if !progressed {
                break;
            }
        }

        Ok(())
    }

    async fn advance_by(&mut self, duration: Duration) -> io::Result<()> {
        let target = self.now + duration;
        while self.now < target {
            let next_deadline = self
                .nodes
                .values()
                .filter_map(|node| node.engine.next_deadline(self.now))
                .min();
            let Some(deadline) = next_deadline else {
                self.now = target;
                break;
            };
            self.now = deadline.min(target);
            self.run_until_idle(256).await?;
        }
        self.now = target;
        self.run_until_idle(256).await
    }

    async fn apply_actions(&mut self, from: Ipv4Addr, actions: Vec<Action>) -> io::Result<()> {
        for action in actions {
            self.action_log.push((from, action.clone()));
            if let Action::Send(send) = action {
                self.send_datagram(from, &send).await?;
            }
        }
        Ok(())
    }

    async fn send_datagram(&self, from: Ipv4Addr, send: &SendAction) -> io::Result<()> {
        let bytes = send.message.encode();
        let sender = &self.nodes.get(&from).unwrap().socket;
        sender.set_ttl(send.ttl as u32)?;

        match send.target {
            SendTarget::Broadcast => {
                for neighbor in self
                    .links
                    .get(&from)
                    .into_iter()
                    .flat_map(|set| set.iter().copied())
                {
                    let destination = self.nodes.get(&neighbor).unwrap().socket.local_addr()?;
                    sender.send_to(bytes.as_ref(), destination).await?;
                }
            }
            SendTarget::Unicast(target) => {
                if self
                    .links
                    .get(&from)
                    .is_some_and(|set| set.contains(&target))
                {
                    let destination = self.nodes.get(&target).unwrap().socket.local_addr()?;
                    sender.send_to(bytes.as_ref(), destination).await?;
                }
            }
        }

        Ok(())
    }
}

fn test_config(local_ip: Ipv4Addr) -> Config {
    Config {
        local_ip,
        bind_ip: local_ip,
        active_route_timeout: Duration::from_millis(400),
        hello_interval: Duration::from_millis(100),
        allowed_hello_loss: 2,
        ttl_start: 1,
        ttl_increment: 2,
        ttl_threshold: 5,
        net_diameter: 10,
        rreq_retries: 2,
        ..Config::default()
    }
}

fn ip(last: u8) -> Ipv4Addr {
    Ipv4Addr::new(10, 0, 0, last)
}

// Real UDP smoke test for multihop discovery.  The mesh harness sends encoded
// AODV datagrams over loopback sockets while the test-controlled links decide
// which virtual neighbors receive each packet.
#[tokio::test]
async fn udp_mesh_discovers_multihop_route() -> io::Result<()> {
    let mut mesh = UdpMesh::new(&[ip(1), ip(2), ip(3)]).await?;
    mesh.link(ip(1), ip(2));
    mesh.link(ip(2), ip(3));

    mesh.start_discovery(ip(1), ip(3)).await?;
    mesh.advance_by(Duration::from_millis(300)).await?;

    let route = mesh.node(ip(1)).route(ip(3)).unwrap();
    assert_eq!(route.state, RouteState::Valid);
    assert_eq!(route.next_hop, ip(2));
    assert_eq!(route.hop_count, 2);
    Ok(())
}

// Real UDP smoke test for RERR propagation after a link break, proving the
// socket harness and engine agree on invalidating a discovered route.
#[tokio::test]
async fn udp_mesh_propagates_rerr_after_link_break() -> io::Result<()> {
    let mut mesh = UdpMesh::new(&[ip(1), ip(2), ip(3)]).await?;
    mesh.link(ip(1), ip(2));
    mesh.link(ip(2), ip(3));

    mesh.start_discovery(ip(1), ip(3)).await?;
    mesh.advance_by(Duration::from_millis(300)).await?;

    mesh.unlink(ip(2), ip(3));
    mesh.handle_link_loss(ip(2), ip(3)).await?;
    mesh.run_until_idle(128).await?;

    let route = mesh.node(ip(1)).route(ip(3)).unwrap();
    assert_eq!(route.state, RouteState::Invalid);
    Ok(())
}

// Real UDP smoke test for rediscovery over an alternate branch after the first
// discovered route is invalidated.
#[tokio::test]
async fn udp_mesh_rediscovery_finds_alternate_path() -> io::Result<()> {
    let mut mesh = UdpMesh::new(&[ip(1), ip(2), ip(3), ip(4)]).await?;
    mesh.link(ip(1), ip(2));
    mesh.link(ip(2), ip(4));
    mesh.link(ip(1), ip(3));
    mesh.link(ip(3), ip(4));

    mesh.start_discovery(ip(1), ip(4)).await?;
    mesh.advance_by(Duration::from_millis(300)).await?;
    assert_eq!(mesh.node(ip(1)).route(ip(4)).unwrap().next_hop, ip(2));

    mesh.unlink(ip(2), ip(4));
    mesh.handle_link_loss(ip(2), ip(4)).await?;
    mesh.run_until_idle(128).await?;
    mesh.start_discovery(ip(1), ip(4)).await?;
    mesh.advance_by(Duration::from_millis(300)).await?;

    let route = mesh.node(ip(1)).route(ip(4)).unwrap();
    assert_eq!(route.state, RouteState::Valid);
    assert_eq!(route.next_hop, ip(3));
    Ok(())
}
