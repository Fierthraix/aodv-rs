use std::collections::{BTreeMap, BTreeSet};
use std::net::Ipv4Addr;
use std::time::{Duration, Instant};

use aodv::{
    Action, BufferedPacket, Config, Engine, IncomingPacket, Message, RouteState, SendAction,
    SendTarget,
};

#[derive(Debug)]
struct SimNode {
    engine: Engine,
}

#[derive(Debug, Clone)]
struct ScheduledDelivery {
    at: Instant,
    from: Ipv4Addr,
    to: Ipv4Addr,
    ttl: u8,
    message: Message,
}

#[derive(Debug, Clone)]
struct ScheduledDataDelivery {
    at: Instant,
    from: Ipv4Addr,
    to: Ipv4Addr,
    destination: Ipv4Addr,
    packet: BufferedPacket,
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct DeliveredPacket {
    source: Ipv4Addr,
    destination: Ipv4Addr,
    payload: Vec<u8>,
    path: Vec<Ipv4Addr>,
}

#[derive(Debug, Clone, Copy)]
enum DueEvent {
    Control(usize),
    Data(usize),
    Tick(Ipv4Addr),
}

#[derive(Debug)]
struct SimNetwork {
    now: Instant,
    nodes: BTreeMap<Ipv4Addr, SimNode>,
    links: BTreeMap<Ipv4Addr, BTreeSet<Ipv4Addr>>,
    deliveries: Vec<ScheduledDelivery>,
    data_deliveries: Vec<ScheduledDataDelivery>,
    delivered_packets: Vec<DeliveredPacket>,
    packet_sources: BTreeMap<u64, Ipv4Addr>,
    packet_paths: BTreeMap<u64, Vec<Ipv4Addr>>,
    next_packet_id: u64,
    action_log: Vec<(Ipv4Addr, Action)>,
}

impl SimNetwork {
    fn new() -> Self {
        Self {
            now: Instant::now(),
            nodes: BTreeMap::new(),
            links: BTreeMap::new(),
            deliveries: Vec::new(),
            data_deliveries: Vec::new(),
            delivered_packets: Vec::new(),
            packet_sources: BTreeMap::new(),
            packet_paths: BTreeMap::new(),
            next_packet_id: 1,
            action_log: Vec::new(),
        }
    }

    fn add_node(&mut self, ip: Ipv4Addr) {
        let config = test_config(ip);
        self.nodes.insert(
            ip,
            SimNode {
                engine: Engine::new(config),
            },
        );
        self.links.entry(ip).or_default();
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

    fn node_mut(&mut self, ip: Ipv4Addr) -> &mut Engine {
        &mut self.nodes.get_mut(&ip).unwrap().engine
    }

    fn start_discovery(&mut self, from: Ipv4Addr, destination: Ipv4Addr) {
        let now = self.now;
        let actions = self.node_mut(from).start_route_discovery(destination, now);
        self.process_actions(from, actions);
    }

    fn send_payload(&mut self, from: Ipv4Addr, destination: Ipv4Addr, payload: Vec<u8>) -> u64 {
        let id = self.next_packet_id;
        self.next_packet_id += 1;
        self.packet_sources.insert(id, from);
        self.packet_paths.insert(id, vec![from]);

        let now = self.now;
        let actions = self.node_mut(from).submit_data_packet(
            destination,
            BufferedPacket { id, payload },
            now,
        );
        self.process_actions(from, actions);
        id
    }

    fn handle_link_loss(&mut self, at: Ipv4Addr, lost_next_hop: Ipv4Addr) {
        let now = self.now;
        let actions = self.node_mut(at).handle_link_loss(lost_next_hop, now);
        self.process_actions(at, actions);
    }

    fn advance_by(&mut self, duration: Duration) {
        let target = self.now + duration;
        while self.step_until(target) {}
        self.now = target;
        self.tick_due_nodes();
    }

    fn run_until_idle(&mut self, max_steps: usize) {
        for _ in 0..max_steps {
            if !self.step_due_now() {
                break;
            }
        }
    }

    fn count_send_actions<F>(&self, predicate: F) -> usize
    where
        F: Fn(Ipv4Addr, &SendAction) -> bool,
    {
        self.action_log
            .iter()
            .filter_map(|(node, action)| match action {
                Action::Send(send) if predicate(*node, send) => Some(()),
                _ => None,
            })
            .count()
    }

    fn step_until(&mut self, limit: Instant) -> bool {
        let next_control = self
            .deliveries
            .iter()
            .enumerate()
            .filter(|(_, delivery)| delivery.at <= limit)
            .min_by_key(|(_, delivery)| delivery.at)
            .map(|(index, delivery)| (delivery.at, DueEvent::Control(index)));

        let next_data = self
            .data_deliveries
            .iter()
            .enumerate()
            .filter(|(_, delivery)| delivery.at <= limit)
            .min_by_key(|(_, delivery)| delivery.at)
            .map(|(index, delivery)| (delivery.at, DueEvent::Data(index)));

        let next_tick = self
            .nodes
            .iter()
            .filter_map(|(ip, node)| {
                node.engine
                    .next_deadline(self.now)
                    .map(|deadline| (*ip, deadline))
            })
            .filter(|(_, deadline)| *deadline <= limit)
            .min_by_key(|(_, deadline)| *deadline)
            .map(|(ip, deadline)| (deadline, DueEvent::Tick(ip)));

        let mut next = next_control;
        for candidate in [next_data, next_tick].into_iter().flatten() {
            if next.is_none_or(|(at, _)| candidate.0 < at) {
                next = Some(candidate);
            }
        }

        match next {
            None => false,
            Some((_, DueEvent::Control(index))) => {
                self.process_delivery(index);
                true
            }
            Some((_, DueEvent::Data(index))) => {
                self.process_data_delivery(index);
                true
            }
            Some((deadline, DueEvent::Tick(ip))) => {
                self.process_tick(ip, deadline);
                true
            }
        }
    }

    fn step_due_now(&mut self) -> bool {
        let next_delivery_index = self
            .deliveries
            .iter()
            .enumerate()
            .filter(|(_, delivery)| delivery.at <= self.now)
            .min_by_key(|(_, delivery)| delivery.at)
            .map(|(index, _)| index);

        if let Some(index) = next_delivery_index {
            self.process_delivery(index);
            return true;
        }

        let next_data_delivery_index = self
            .data_deliveries
            .iter()
            .enumerate()
            .filter(|(_, delivery)| delivery.at <= self.now)
            .min_by_key(|(_, delivery)| delivery.at)
            .map(|(index, _)| index);

        if let Some(index) = next_data_delivery_index {
            self.process_data_delivery(index);
            return true;
        }

        let next_tick = self
            .nodes
            .iter()
            .filter_map(|(ip, node)| {
                node.engine
                    .next_deadline(self.now)
                    .map(|deadline| (*ip, deadline))
            })
            .find(|(_, deadline)| *deadline <= self.now);
        if let Some((ip, deadline)) = next_tick {
            self.process_tick(ip, deadline);
            return true;
        }

        false
    }

    fn tick_due_nodes(&mut self) {
        loop {
            let due = self
                .nodes
                .iter()
                .filter_map(|(ip, node)| {
                    node.engine
                        .next_deadline(self.now)
                        .map(|deadline| (*ip, deadline))
                })
                .find(|(_, deadline)| *deadline <= self.now);
            let Some((ip, deadline)) = due else {
                break;
            };
            self.process_tick(ip, deadline);
        }
    }

    fn process_tick(&mut self, ip: Ipv4Addr, when: Instant) {
        self.now = when;
        let now = self.now;
        let actions = self.node_mut(ip).tick(now);
        self.process_actions(ip, actions);
    }

    fn process_delivery(&mut self, index: usize) {
        let delivery = self.deliveries.remove(index);
        self.now = delivery.at;
        let now = self.now;
        let actions = self.node_mut(delivery.to).handle_incoming(
            IncomingPacket {
                source: delivery.from,
                ttl: Some(delivery.ttl),
                message: delivery.message,
            },
            now,
        );
        self.process_actions(delivery.to, actions);
    }

    fn process_data_delivery(&mut self, index: usize) {
        let delivery = self.data_deliveries.remove(index);
        self.now = delivery.at;
        let path = self.packet_paths.entry(delivery.packet.id).or_default();
        if path.last().copied() != Some(delivery.to) {
            path.push(delivery.to);
        }

        if delivery.to == delivery.destination {
            let source = self
                .packet_sources
                .remove(&delivery.packet.id)
                .unwrap_or(delivery.from);
            let path = self
                .packet_paths
                .remove(&delivery.packet.id)
                .unwrap_or_else(|| vec![source, delivery.to]);
            self.delivered_packets.push(DeliveredPacket {
                source,
                destination: delivery.destination,
                payload: delivery.packet.payload,
                path,
            });
            return;
        }

        self.forward_payload(delivery.to, delivery.destination, delivery.packet);
    }

    fn forward_payload(
        &mut self,
        current: Ipv4Addr,
        destination: Ipv4Addr,
        packet: BufferedPacket,
    ) {
        let route = self.node(current).route(destination).cloned();
        let Some(route) = route.filter(|route| route.state == RouteState::Valid) else {
            let now = self.now;
            let actions = self
                .node_mut(current)
                .submit_data_packet(destination, packet, now);
            self.process_actions(current, actions);
            return;
        };

        self.schedule_data_send(current, destination, route.next_hop, packet);
    }

    fn process_actions(&mut self, from: Ipv4Addr, actions: Vec<Action>) {
        for action in actions {
            self.action_log.push((from, action.clone()));
            match action {
                Action::Send(send) => self.schedule_send(from, send),
                Action::ForwardBufferedPackets {
                    destination,
                    next_hop,
                    packets,
                } => {
                    for packet in packets {
                        self.schedule_data_send(from, destination, next_hop, packet);
                    }
                }
                Action::DropBufferedPackets { packets, .. } => {
                    for packet in packets {
                        self.packet_sources.remove(&packet.id);
                        self.packet_paths.remove(&packet.id);
                    }
                }
                _ => {}
            }
        }
    }

    fn schedule_send(&mut self, from: Ipv4Addr, send: SendAction) {
        let recipients: Vec<Ipv4Addr> = match send.target {
            SendTarget::Broadcast => self
                .links
                .get(&from)
                .into_iter()
                .flat_map(|neighbors| neighbors.iter().copied())
                .collect(),
            SendTarget::Unicast(target) => {
                if self.is_linked(from, target) {
                    vec![target]
                } else {
                    Vec::new()
                }
            }
        };

        for recipient in recipients {
            self.deliveries.push(ScheduledDelivery {
                at: self.now,
                from,
                to: recipient,
                ttl: send.ttl,
                message: send.message.clone(),
            });
        }
    }

    fn schedule_data_send(
        &mut self,
        from: Ipv4Addr,
        destination: Ipv4Addr,
        next_hop: Ipv4Addr,
        packet: BufferedPacket,
    ) {
        if self.is_linked(from, next_hop) {
            self.data_deliveries.push(ScheduledDataDelivery {
                at: self.now,
                from,
                to: next_hop,
                destination,
                packet,
            });
            return;
        }

        let now = self.now;
        let actions =
            self.node_mut(from)
                .handle_forwarding_failure(destination, next_hop, Some(packet), now);
        self.process_actions(from, actions);
    }

    fn is_linked(&self, a: Ipv4Addr, b: Ipv4Addr) -> bool {
        self.links
            .get(&a)
            .is_some_and(|neighbors| neighbors.contains(&b))
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

// Verifies RREQ/RREP propagation across a simple chain and checks that the
// originator installs the first hop and total hop count for a multihop route.
#[test]
fn line_topology_discovers_multihop_route() {
    let mut network = SimNetwork::new();
    for node in [ip(1), ip(2), ip(3), ip(4)] {
        network.add_node(node);
    }
    network.link(ip(1), ip(2));
    network.link(ip(2), ip(3));
    network.link(ip(3), ip(4));

    network.start_discovery(ip(1), ip(4));
    network.advance_by(Duration::from_millis(300));

    let route = network.node(ip(1)).route(ip(4)).unwrap();
    assert_eq!(route.state, RouteState::Valid);
    assert_eq!(route.next_hop, ip(2));
    assert_eq!(route.hop_count, 3);
}

// Verifies the engine buffers payloads while discovery is in flight, then
// delivers the original payload once the route has been learned.
#[test]
fn payload_sent_before_route_exists_is_delivered_after_discovery() {
    let mut network = SimNetwork::new();
    for node in [ip(1), ip(2), ip(3), ip(4)] {
        network.add_node(node);
    }
    network.link(ip(1), ip(2));
    network.link(ip(2), ip(3));
    network.link(ip(3), ip(4));

    network.send_payload(ip(1), ip(4), b"hello".to_vec());
    network.advance_by(Duration::from_millis(300));

    assert_eq!(
        network.delivered_packets,
        vec![DeliveredPacket {
            source: ip(1),
            destination: ip(4),
            payload: b"hello".to_vec(),
            path: vec![ip(1), ip(2), ip(3), ip(4)],
        }]
    );
    let route = network.node(ip(1)).route(ip(4)).unwrap();
    assert_eq!(route.state, RouteState::Valid);
    assert_eq!(route.next_hop, ip(2));
    assert_eq!(route.hop_count, 3);
}

// Verifies an already-known valid route forwards payloads immediately without
// starting another RREQ flood.
#[test]
fn payload_uses_existing_route_without_new_discovery() {
    let mut network = SimNetwork::new();
    for node in [ip(1), ip(2), ip(3), ip(4)] {
        network.add_node(node);
    }
    network.link(ip(1), ip(2));
    network.link(ip(2), ip(3));
    network.link(ip(3), ip(4));

    network.start_discovery(ip(1), ip(4));
    network.advance_by(Duration::from_millis(300));
    network.action_log.clear();

    network.send_payload(ip(1), ip(4), b"already-routed".to_vec());
    network.run_until_idle(64);

    assert_eq!(
        network.delivered_packets,
        vec![DeliveredPacket {
            source: ip(1),
            destination: ip(4),
            payload: b"already-routed".to_vec(),
            path: vec![ip(1), ip(2), ip(3), ip(4)],
        }]
    );
    let new_rreq_count = network
        .count_send_actions(|node, send| node == ip(1) && matches!(send.message, Message::Rreq(_)));
    assert_eq!(new_rreq_count, 0);
}

// Verifies unreachable destinations eventually fail discovery and drop buffered
// payloads instead of keeping them forever.
#[test]
fn payload_is_not_delivered_when_destination_unreachable() {
    let mut network = SimNetwork::new();
    network.add_node(ip(1));
    network.add_node(ip(2));

    network.send_payload(ip(1), ip(2), b"undeliverable".to_vec());
    network.advance_by(Duration::from_secs(20));

    assert!(network.delivered_packets.is_empty());
    assert!(network.action_log.iter().any(|(node, action)| {
        *node == ip(1)
            && matches!(
                action,
                Action::DropBufferedPackets { destination, .. }
                    | Action::RouteDiscoveryFailed { destination }
                    if *destination == ip(2)
            )
    }));
}

// Verifies a buffered payload can still be delivered when topology changes make
// the destination reachable before discovery retries are exhausted.
#[test]
fn buffered_payload_is_delivered_after_topology_connects() {
    let mut network = SimNetwork::new();
    for node in [ip(1), ip(2), ip(3)] {
        network.add_node(node);
    }
    network.link(ip(1), ip(2));

    network.send_payload(ip(1), ip(3), b"late-link".to_vec());
    network.advance_by(Duration::from_millis(150));
    assert!(network.delivered_packets.is_empty());

    network.link(ip(2), ip(3));
    network.advance_by(Duration::from_millis(200));

    assert_eq!(
        network.delivered_packets,
        vec![DeliveredPacket {
            source: ip(1),
            destination: ip(3),
            payload: b"late-link".to_vec(),
            path: vec![ip(1), ip(2), ip(3)],
        }]
    );
    let route = network.node(ip(1)).route(ip(3)).unwrap();
    assert_eq!(route.state, RouteState::Valid);
    assert_eq!(route.next_hop, ip(2));
}

// Verifies payload forwarding after a primary path break: RERR invalidates the
// old route and rediscovery finds the alternate branch.
#[test]
fn payload_rediscovery_uses_alternate_path_after_topology_break() {
    let mut network = SimNetwork::new();
    for node in [ip(1), ip(2), ip(3), ip(4)] {
        network.add_node(node);
    }
    network.link(ip(1), ip(2));
    network.link(ip(2), ip(4));
    network.link(ip(1), ip(3));
    network.link(ip(3), ip(4));

    network.send_payload(ip(1), ip(4), b"before-break".to_vec());
    network.advance_by(Duration::from_millis(300));
    assert_eq!(
        network.delivered_packets,
        vec![DeliveredPacket {
            source: ip(1),
            destination: ip(4),
            payload: b"before-break".to_vec(),
            path: vec![ip(1), ip(2), ip(4)],
        }]
    );
    assert_eq!(network.node(ip(1)).route(ip(4)).unwrap().next_hop, ip(2));

    network.unlink(ip(2), ip(4));
    network.handle_link_loss(ip(2), ip(4));
    network.run_until_idle(128);
    assert_eq!(
        network.node(ip(1)).route(ip(4)).unwrap().state,
        RouteState::Invalid
    );

    network.send_payload(ip(1), ip(4), b"after-break".to_vec());
    network.advance_by(Duration::from_millis(300));

    assert_eq!(
        network.delivered_packets,
        vec![
            DeliveredPacket {
                source: ip(1),
                destination: ip(4),
                payload: b"before-break".to_vec(),
                path: vec![ip(1), ip(2), ip(4)],
            },
            DeliveredPacket {
                source: ip(1),
                destination: ip(4),
                payload: b"after-break".to_vec(),
                path: vec![ip(1), ip(3), ip(4)],
            },
        ]
    );
    let route = network.node(ip(1)).route(ip(4)).unwrap();
    assert_eq!(route.state, RouteState::Valid);
    assert_eq!(route.next_hop, ip(3));
}

// Verifies the destination learns a reverse route to the originator while
// processing the incoming RREQ.
#[test]
fn destination_learns_reverse_route_from_discovery() {
    let mut network = SimNetwork::new();
    for node in [ip(1), ip(2), ip(3)] {
        network.add_node(node);
    }
    network.link(ip(1), ip(2));
    network.link(ip(2), ip(3));

    network.start_discovery(ip(1), ip(3));
    network.advance_by(Duration::from_millis(300));

    let route = network.node(ip(3)).route(ip(1)).unwrap();
    assert_eq!(route.state, RouteState::Valid);
    assert_eq!(route.next_hop, ip(2));
    assert_eq!(route.hop_count, 2);
}

// Verifies pure route discovery failure without payload buffering still emits a
// RouteDiscoveryFailed action at the originator.
#[test]
fn disconnected_network_eventually_fails_route_discovery() {
    let mut network = SimNetwork::new();
    network.add_node(ip(1));
    network.add_node(ip(2));

    network.start_discovery(ip(1), ip(2));
    network.advance_by(Duration::from_secs(20));

    assert!(network.node(ip(1)).route(ip(2)).is_none());
    assert!(network.action_log.iter().any(|(node, action)| {
        *node == ip(1)
            && matches!(
                action,
                Action::RouteDiscoveryFailed { destination } if *destination == ip(2)
            )
    }));
}

// Verifies discovery retries are useful: a route can succeed after a new link
// appears between the first RREQ and later retry.
#[test]
fn retry_succeeds_after_link_is_added() {
    let mut network = SimNetwork::new();
    for node in [ip(1), ip(2), ip(3)] {
        network.add_node(node);
    }
    network.link(ip(1), ip(2));

    network.start_discovery(ip(1), ip(3));
    network.advance_by(Duration::from_millis(150));
    network.link(ip(2), ip(3));
    network.advance_by(Duration::from_millis(200));

    let route = network.node(ip(1)).route(ip(3)).unwrap();
    assert_eq!(route.state, RouteState::Valid);
    assert_eq!(route.next_hop, ip(2));
}

// Verifies RERR propagation from the node adjacent to a broken link back to the
// route originator that had the broken next hop as a dependency.
#[test]
fn link_break_propagates_rerr_to_originator() {
    let mut network = SimNetwork::new();
    for node in [ip(1), ip(2), ip(3)] {
        network.add_node(node);
    }
    network.link(ip(1), ip(2));
    network.link(ip(2), ip(3));

    network.start_discovery(ip(1), ip(3));
    network.advance_by(Duration::from_millis(300));

    network.unlink(ip(2), ip(3));
    network.handle_link_loss(ip(2), ip(3));
    network.run_until_idle(128);

    let route = network.node(ip(1)).route(ip(3)).unwrap();
    assert_eq!(route.state, RouteState::Invalid);
}

// Verifies periodic HELLO traffic refreshes neighbor-derived routes beyond
// ACTIVE_ROUTE_TIMEOUT while the link remains present.
#[test]
fn hello_messages_keep_routes_alive_past_active_route_timeout() {
    let mut network = SimNetwork::new();
    network.add_node(ip(1));
    network.add_node(ip(2));
    network.link(ip(1), ip(2));

    network.start_discovery(ip(1), ip(2));
    network.run_until_idle(64);
    network.advance_by(Duration::from_millis(900));

    let route = network.node(ip(1)).route(ip(2)).unwrap();
    assert_eq!(route.state, RouteState::Valid);
}

// Verifies missed HELLOs after a partition invalidate the neighbor route once
// the allowed loss window expires.
#[test]
fn hello_timeout_invalidates_routes_after_partition() {
    let mut network = SimNetwork::new();
    network.add_node(ip(1));
    network.add_node(ip(2));
    network.link(ip(1), ip(2));

    network.start_discovery(ip(1), ip(2));
    network.run_until_idle(64);
    network.advance_by(Duration::from_millis(200));
    network.unlink(ip(1), ip(2));
    network.advance_by(Duration::from_millis(800));

    let route = network.node(ip(1)).route(ip(2)).unwrap();
    assert_eq!(route.state, RouteState::Invalid);
}

// Verifies duplicate RREQ suppression in a diamond topology: the destination
// sees two paths for one RREQ ID but answers only once.
#[test]
fn diamond_topology_suppresses_duplicate_rreq_processing_at_destination() {
    let mut network = SimNetwork::new();
    for node in [ip(1), ip(2), ip(3), ip(4)] {
        network.add_node(node);
    }
    network.link(ip(1), ip(2));
    network.link(ip(1), ip(3));
    network.link(ip(2), ip(4));
    network.link(ip(3), ip(4));

    network.start_discovery(ip(1), ip(4));
    network.advance_by(Duration::from_millis(300));

    let rrep_count = network.count_send_actions(|node, send| {
        node == ip(4)
            && matches!(
                &send.message,
                Message::Rrep(rrep) if rrep.destination_ip == ip(4) && rrep.originator_ip == ip(1)
            )
    });
    assert_eq!(rrep_count, 1);
}

// Verifies expanding-ring search TTLs increase from TTL_START through
// TTL_THRESHOLD and then use NET_DIAMETER retries.
#[test]
fn expanding_ring_search_increases_ttl_across_retries() {
    let mut network = SimNetwork::new();
    network.add_node(ip(1));
    network.add_node(ip(9));

    network.start_discovery(ip(1), ip(9));
    network.advance_by(Duration::from_secs(20));

    let ttls: Vec<u8> = network
        .action_log
        .iter()
        .filter_map(|(node, action)| match action {
            Action::Send(send) if *node == ip(1) && matches!(send.message, Message::Rreq(_)) => {
                Some(send.ttl)
            }
            _ => None,
        })
        .collect();
    assert_eq!(ttls, vec![1, 3, 10, 10, 10]);
}

// Verifies rediscovery alone can replace an invalidated primary route with an
// alternate next hop after the original branch breaks.
#[test]
fn rediscovery_uses_alternate_path_after_primary_break() {
    let mut network = SimNetwork::new();
    for node in [ip(1), ip(2), ip(3), ip(4)] {
        network.add_node(node);
    }
    network.link(ip(1), ip(2));
    network.link(ip(2), ip(4));
    network.link(ip(1), ip(3));
    network.link(ip(3), ip(4));

    network.start_discovery(ip(1), ip(4));
    network.advance_by(Duration::from_millis(300));
    assert_eq!(network.node(ip(1)).route(ip(4)).unwrap().next_hop, ip(2));

    network.unlink(ip(2), ip(4));
    network.handle_link_loss(ip(2), ip(4));
    network.run_until_idle(128);
    network.start_discovery(ip(1), ip(4));
    network.advance_by(Duration::from_millis(300));

    let route = network.node(ip(1)).route(ip(4)).unwrap();
    assert_eq!(route.state, RouteState::Valid);
    assert_eq!(route.next_hop, ip(3));
}

// Verifies RERR messages are ignored when they arrive from a node that is not
// the next hop for the affected route.
#[test]
fn rerr_from_non_next_hop_is_ignored() {
    let mut network = SimNetwork::new();
    for node in [ip(1), ip(2), ip(3), ip(4)] {
        network.add_node(node);
    }
    network.link(ip(1), ip(2));
    network.link(ip(2), ip(4));
    network.link(ip(1), ip(3));

    network.start_discovery(ip(1), ip(4));
    network.advance_by(Duration::from_millis(300));

    let bogus_rerr = aodv::Rerr {
        no_delete: false,
        unreachable_destinations: vec![aodv::UnreachableDestination {
            destination_ip: ip(4),
            destination_sequence_number: 99,
        }],
    };
    let now = network.now;
    let actions = network.node_mut(ip(1)).handle_incoming(
        IncomingPacket {
            source: ip(3),
            ttl: Some(1),
            message: Message::Rerr(bogus_rerr),
        },
        now,
    );
    network.process_actions(ip(1), actions);

    let route = network.node(ip(1)).route(ip(4)).unwrap();
    assert_eq!(route.state, RouteState::Valid);
    assert_eq!(route.next_hop, ip(2));
}
