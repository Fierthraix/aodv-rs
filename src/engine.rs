use std::collections::{BTreeSet, HashMap, VecDeque};
use std::net::Ipv4Addr;
use std::time::{Duration, Instant};

use crate::config::{Config, duration_mul};
use crate::message::{Message, Rerr, Rrep, Rreq, UnreachableDestination};

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct IncomingPacket {
    pub source: Ipv4Addr,
    pub ttl: Option<u8>,
    pub message: Message,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Action {
    Send(SendAction),
    RouteDiscovered {
        destination: Ipv4Addr,
        next_hop: Ipv4Addr,
        hop_count: u8,
    },
    RouteInvalidated {
        destination: Ipv4Addr,
        next_hop: Ipv4Addr,
    },
    RouteDiscoveryFailed {
        destination: Ipv4Addr,
    },
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SendAction {
    pub target: SendTarget,
    pub ttl: u8,
    pub message: Message,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SendTarget {
    Unicast(Ipv4Addr),
    Broadcast,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum RouteState {
    Valid,
    Invalid,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RouteEntry {
    pub destination: Ipv4Addr,
    pub sequence_number: u32,
    pub sequence_number_valid: bool,
    pub state: RouteState,
    pub hop_count: u8,
    pub next_hop: Ipv4Addr,
    pub precursors: BTreeSet<Ipv4Addr>,
    pub lifetime: Instant,
    pub created_by_hello: bool,
}

#[derive(Debug, Clone)]
pub struct Engine {
    config: Config,
    local_sequence_number: u32,
    next_rreq_id: u32,
    routes: HashMap<Ipv4Addr, RouteEntry>,
    seen_rreqs: HashMap<(Ipv4Addr, u32), Instant>,
    pending_discoveries: HashMap<Ipv4Addr, PendingDiscovery>,
    neighbors: HashMap<Ipv4Addr, NeighborState>,
    recent_rreq_emissions: VecDeque<Instant>,
    recent_rerr_emissions: VecDeque<Instant>,
    last_broadcast_at: Option<Instant>,
}

#[derive(Debug, Clone)]
struct PendingDiscovery {
    last_ttl: u8,
    retries_at_net_diameter: usize,
    deadline: Instant,
}

#[derive(Debug, Clone)]
struct NeighborState {
    last_heard: Instant,
    hello_timeout: Option<Duration>,
}

impl Engine {
    pub fn new(config: Config) -> Self {
        Self {
            config,
            local_sequence_number: 0,
            next_rreq_id: 0,
            routes: HashMap::new(),
            seen_rreqs: HashMap::new(),
            pending_discoveries: HashMap::new(),
            neighbors: HashMap::new(),
            recent_rreq_emissions: VecDeque::new(),
            recent_rerr_emissions: VecDeque::new(),
            last_broadcast_at: None,
        }
    }

    pub fn config(&self) -> &Config {
        &self.config
    }

    pub fn route(&self, destination: Ipv4Addr) -> Option<&RouteEntry> {
        self.routes.get(&destination)
    }

    pub fn start_route_discovery(&mut self, destination: Ipv4Addr, now: Instant) -> Vec<Action> {
        self.prune_caches(now);
        if self.has_active_route(destination, now) {
            let route = self.routes.get(&destination).unwrap();
            return vec![Action::RouteDiscovered {
                destination,
                next_hop: route.next_hop,
                hop_count: route.hop_count,
            }];
        }

        let ttl = self.initial_ttl_for(destination);
        let message = self.build_rreq(destination);
        self.seen_rreqs.insert(
            (self.config.local_ip, self.next_rreq_id),
            now + self.config.path_discovery_time(),
        );

        let wait_duration = self.discovery_wait_duration(ttl, 0);
        self.pending_discoveries.insert(
            destination,
            PendingDiscovery {
                last_ttl: ttl,
                retries_at_net_diameter: usize::from(ttl >= self.config.net_diameter),
                deadline: now + wait_duration,
            },
        );

        self.send_rreq(message, ttl, now).into_iter().collect()
    }

    pub fn handle_incoming(&mut self, packet: IncomingPacket, now: Instant) -> Vec<Action> {
        self.prune_caches(now);
        self.note_neighbor_contact(packet.source, now, None);

        match packet.message {
            Message::Rreq(message) => self.handle_rreq(packet.source, packet.ttl, message, now),
            Message::Rrep(message) if message.is_hello(packet.source, packet.ttl) => {
                self.handle_hello(packet.source, message, now)
            }
            Message::Rrep(message) => self.handle_rrep(packet.source, message, now),
            Message::Rerr(message) => self.handle_rerr(packet.source, message, now),
            Message::RrepAck => Vec::new(),
        }
    }

    pub fn handle_link_loss(&mut self, next_hop: Ipv4Addr, now: Instant) -> Vec<Action> {
        self.prune_caches(now);
        self.neighbors.remove(&next_hop);
        self.invalidate_routes_using_next_hop(next_hop, now, None)
    }

    pub fn tick(&mut self, now: Instant) -> Vec<Action> {
        self.prune_caches(now);
        let mut actions = Vec::new();

        let lost_neighbors: Vec<Ipv4Addr> = self
            .neighbors
            .iter()
            .filter_map(|(neighbor, state)| match state.hello_timeout {
                Some(timeout) if state.last_heard + timeout <= now => Some(*neighbor),
                _ => None,
            })
            .collect();
        for neighbor in lost_neighbors {
            actions.extend(self.handle_link_loss(neighbor, now));
        }

        let expiring: Vec<Ipv4Addr> = self
            .routes
            .iter()
            .filter_map(|(destination, route)| {
                if route.state == RouteState::Valid && route.lifetime <= now {
                    Some(*destination)
                } else {
                    None
                }
            })
            .collect();

        for destination in expiring {
            if let Some(route) = self.routes.get_mut(&destination) {
                route.state = RouteState::Invalid;
                route.lifetime = now + self.config.delete_period();
                actions.push(Action::RouteInvalidated {
                    destination,
                    next_hop: route.next_hop,
                });
            }
        }

        self.routes.retain(|destination, route| {
            *destination == self.config.local_ip || route.lifetime > now
        });

        let timed_out_discoveries: Vec<Ipv4Addr> = self
            .pending_discoveries
            .iter()
            .filter_map(|(destination, pending)| (pending.deadline <= now).then_some(*destination))
            .collect();

        for destination in timed_out_discoveries {
            actions.extend(self.retry_route_discovery(destination, now));
        }

        if self.should_emit_hello(now) {
            let hello = Rrep::hello(
                self.config.local_ip,
                self.local_sequence_number,
                self.config.hello_timeout(None).as_millis() as u32,
                self.config.hello_interval.as_millis() as u32,
            );
            actions.push(self.broadcast_action(Message::Rrep(hello), 1, now));
        }

        actions
    }

    pub fn next_deadline(&self, now: Instant) -> Option<Instant> {
        let mut deadlines = Vec::new();

        deadlines.extend(
            self.pending_discoveries
                .values()
                .map(|pending| pending.deadline),
        );
        deadlines.extend(
            self.routes
                .values()
                .filter(|route| route.destination != self.config.local_ip)
                .map(|route| route.lifetime),
        );
        deadlines.extend(self.neighbors.values().filter_map(|state| {
            state
                .hello_timeout
                .map(|timeout| state.last_heard + timeout)
        }));

        if self.should_emit_hello(now) {
            deadlines.push(now);
        } else if self.config.enable_hello && self.has_active_routes(now) {
            let next = self
                .last_broadcast_at
                .map(|instant| instant + self.config.hello_interval)
                .unwrap_or(now + self.config.hello_interval);
            deadlines.push(next);
        }

        deadlines.into_iter().min()
    }

    fn handle_rreq(
        &mut self,
        source: Ipv4Addr,
        ttl: Option<u8>,
        mut message: Rreq,
        now: Instant,
    ) -> Vec<Action> {
        self.ensure_neighbor_route(source, now);

        let key = (message.originator_ip, message.rreq_id);
        if self
            .seen_rreqs
            .get(&key)
            .is_some_and(|deadline| *deadline > now)
        {
            return Vec::new();
        }
        self.seen_rreqs
            .insert(key, now + self.config.path_discovery_time());

        let new_hop_count = message.hop_count.saturating_add(1);
        let minimal_lifetime = now
            + self
                .config
                .net_traversal_time()
                .saturating_mul(2)
                .saturating_sub(duration_mul(
                    self.config.node_traversal_time,
                    2 * new_hop_count as u32,
                ));

        self.update_route(RouteUpdate {
            destination: message.originator_ip,
            next_hop: source,
            sequence_number: Some(message.originator_sequence_number),
            sequence_number_valid: true,
            hop_count: new_hop_count,
            lifetime: minimal_lifetime,
            state: RouteState::Valid,
            created_by_hello: false,
        });

        if message.destination_ip == self.config.local_ip {
            return self.reply_as_destination(&message, now);
        }

        if let Some(actions) = self.reply_as_intermediate(source, &message, now) {
            return actions;
        }

        let inbound_ttl = ttl.unwrap_or(self.config.net_diameter);
        if inbound_ttl <= 1 {
            return Vec::new();
        }

        message.hop_count = new_hop_count;
        if let Some(route) = self.routes.get(&message.destination_ip) {
            if route.sequence_number_valid
                && seq_ge(route.sequence_number, message.destination_sequence_number)
            {
                message.destination_sequence_number = route.sequence_number;
                message.unknown_sequence_number = false;
            }
        }

        vec![self.broadcast_action(Message::Rreq(message), inbound_ttl.saturating_sub(1), now)]
    }

    fn reply_as_destination(&mut self, message: &Rreq, _now: Instant) -> Vec<Action> {
        self.local_sequence_number = self
            .local_sequence_number
            .max(message.destination_sequence_number);
        if self.local_sequence_number == message.destination_sequence_number {
            self.local_sequence_number = self.local_sequence_number.wrapping_add(1);
        }

        let reverse_route = match self.routes.get(&message.originator_ip) {
            Some(route) if route.state == RouteState::Valid => route.clone(),
            _ => return Vec::new(),
        };

        let reply = Rrep {
            repair: false,
            acknowledgement_required: false,
            prefix_size: 0,
            hop_count: 0,
            destination_ip: self.config.local_ip,
            destination_sequence_number: self.local_sequence_number,
            originator_ip: message.originator_ip,
            lifetime_ms: self.config.my_route_timeout().as_millis() as u32,
            hello_interval_ms: None,
        };

        vec![self.unicast_action(
            reverse_route.next_hop,
            Message::Rrep(reply),
            self.config.net_diameter,
        )]
    }

    fn reply_as_intermediate(
        &mut self,
        source: Ipv4Addr,
        message: &Rreq,
        now: Instant,
    ) -> Option<Vec<Action>> {
        let forward_route = self.routes.get(&message.destination_ip)?.clone();
        if forward_route.state != RouteState::Valid
            || !forward_route.sequence_number_valid
            || !seq_ge(
                forward_route.sequence_number,
                message.destination_sequence_number,
            )
            || message.destination_only
        {
            return None;
        }

        let reverse_route = self.routes.get(&message.originator_ip)?.clone();
        let lifetime_ms = forward_route
            .lifetime
            .saturating_duration_since(now)
            .as_millis() as u32;

        if let Some(route) = self.routes.get_mut(&message.destination_ip) {
            route.precursors.insert(source);
        }
        if let Some(route) = self.routes.get_mut(&message.originator_ip) {
            route.precursors.insert(forward_route.next_hop);
        }

        let reply = Rrep {
            repair: false,
            acknowledgement_required: false,
            prefix_size: 0,
            hop_count: forward_route.hop_count,
            destination_ip: message.destination_ip,
            destination_sequence_number: forward_route.sequence_number,
            originator_ip: message.originator_ip,
            lifetime_ms,
            hello_interval_ms: None,
        };

        let mut actions = vec![self.unicast_action(
            reverse_route.next_hop,
            Message::Rrep(reply),
            self.config.net_diameter,
        )];

        if message.gratuitous_rrep {
            let gratuitous = Rrep {
                repair: false,
                acknowledgement_required: false,
                prefix_size: 0,
                hop_count: reverse_route.hop_count,
                destination_ip: message.originator_ip,
                destination_sequence_number: message.originator_sequence_number,
                originator_ip: message.destination_ip,
                lifetime_ms: reverse_route
                    .lifetime
                    .saturating_duration_since(now)
                    .as_millis() as u32,
                hello_interval_ms: None,
            };
            actions.push(self.unicast_action(
                forward_route.next_hop,
                Message::Rrep(gratuitous),
                self.config.net_diameter,
            ));
        }

        Some(actions)
    }

    fn handle_rrep(&mut self, source: Ipv4Addr, mut message: Rrep, now: Instant) -> Vec<Action> {
        self.ensure_neighbor_route(source, now);

        let new_hop_count = message.hop_count.saturating_add(1);
        let route_updated = self.update_route(RouteUpdate {
            destination: message.destination_ip,
            next_hop: source,
            sequence_number: Some(message.destination_sequence_number),
            sequence_number_valid: true,
            hop_count: new_hop_count,
            lifetime: now + Duration::from_millis(message.lifetime_ms as u64),
            state: RouteState::Valid,
            created_by_hello: false,
        });

        if !route_updated {
            return Vec::new();
        }

        if message.originator_ip == self.config.local_ip {
            self.pending_discoveries.remove(&message.destination_ip);
            let route = self.routes.get(&message.destination_ip).unwrap();
            return vec![Action::RouteDiscovered {
                destination: message.destination_ip,
                next_hop: route.next_hop,
                hop_count: route.hop_count,
            }];
        }

        let reverse_route = match self.routes.get_mut(&message.originator_ip) {
            Some(route) if route.state == RouteState::Valid => {
                route.lifetime = route.lifetime.max(now + self.config.active_route_timeout);
                route.clone()
            }
            _ => return Vec::new(),
        };

        if let Some(route) = self.routes.get_mut(&message.destination_ip) {
            route.precursors.insert(reverse_route.next_hop);
        }
        if let Some(route) = self.routes.get_mut(&message.originator_ip) {
            route.precursors.insert(source);
        }

        message.hop_count = new_hop_count;
        vec![self.unicast_action(
            reverse_route.next_hop,
            Message::Rrep(message),
            self.config.net_diameter,
        )]
    }

    fn handle_hello(&mut self, source: Ipv4Addr, message: Rrep, now: Instant) -> Vec<Action> {
        self.note_neighbor_contact(
            source,
            now,
            Some(self.config.hello_timeout(message.hello_interval_ms)),
        );
        self.update_route(RouteUpdate {
            destination: source,
            next_hop: source,
            sequence_number: Some(message.destination_sequence_number),
            sequence_number_valid: true,
            hop_count: 1,
            lifetime: now + self.config.hello_timeout(message.hello_interval_ms),
            state: RouteState::Valid,
            created_by_hello: true,
        });
        Vec::new()
    }

    fn handle_rerr(&mut self, source: Ipv4Addr, message: Rerr, now: Instant) -> Vec<Action> {
        let mut affected = Vec::new();
        let mut precursors = BTreeSet::new();

        for unreachable in message.unreachable_destinations {
            let Some(route) = self.routes.get_mut(&unreachable.destination_ip) else {
                continue;
            };
            if route.next_hop != source || route.state != RouteState::Valid {
                continue;
            }

            if message.no_delete {
                if !route.precursors.is_empty() {
                    affected.push(unreachable);
                    precursors.extend(route.precursors.iter().copied());
                }
                continue;
            }

            route.sequence_number = unreachable.destination_sequence_number;
            route.sequence_number_valid = true;
            route.state = RouteState::Invalid;
            route.lifetime = now + self.config.delete_period();
            precursors.extend(route.precursors.iter().copied());
            affected.push(unreachable.clone());
        }

        if affected.is_empty() || precursors.is_empty() || !self.allow_rerr(now) {
            return Vec::new();
        }

        let rerr = Rerr {
            no_delete: message.no_delete,
            unreachable_destinations: affected,
        };
        vec![self.rerr_send_action(
            precursors.iter().next().copied(),
            precursors.len(),
            Message::Rerr(rerr),
        )]
    }

    fn retry_route_discovery(&mut self, destination: Ipv4Addr, now: Instant) -> Vec<Action> {
        let Some(pending) = self.pending_discoveries.get(&destination).cloned() else {
            return Vec::new();
        };

        let (next_ttl, retries_at_net_diameter) = if pending.last_ttl < self.config.ttl_threshold {
            let incremented = pending.last_ttl.saturating_add(self.config.ttl_increment);
            if incremented >= self.config.ttl_threshold {
                (self.config.net_diameter, 1)
            } else {
                (incremented, 0)
            }
        } else if pending.retries_at_net_diameter <= self.config.rreq_retries {
            (
                self.config.net_diameter,
                pending.retries_at_net_diameter + 1,
            )
        } else {
            self.pending_discoveries.remove(&destination);
            return vec![Action::RouteDiscoveryFailed { destination }];
        };

        let wait_duration = self.discovery_wait_duration(next_ttl, retries_at_net_diameter);
        self.pending_discoveries.insert(
            destination,
            PendingDiscovery {
                last_ttl: next_ttl,
                retries_at_net_diameter,
                deadline: now + wait_duration,
            },
        );

        let message = self.build_rreq(destination);
        self.send_rreq(message, next_ttl, now).into_iter().collect()
    }

    fn invalidate_routes_using_next_hop(
        &mut self,
        next_hop: Ipv4Addr,
        now: Instant,
        no_delete: Option<bool>,
    ) -> Vec<Action> {
        let mut unreachable = Vec::new();
        let mut precursors = BTreeSet::new();
        let mut actions = Vec::new();

        for route in self
            .routes
            .values_mut()
            .filter(|route| route.state == RouteState::Valid && route.next_hop == next_hop)
        {
            if route.sequence_number_valid {
                route.sequence_number = route.sequence_number.wrapping_add(1);
            }
            route.sequence_number_valid = true;
            route.state = RouteState::Invalid;
            route.lifetime = now + self.config.delete_period();
            precursors.extend(route.precursors.iter().copied());
            unreachable.push(UnreachableDestination {
                destination_ip: route.destination,
                destination_sequence_number: route.sequence_number,
            });
            actions.push(Action::RouteInvalidated {
                destination: route.destination,
                next_hop,
            });
        }

        if unreachable.is_empty() || precursors.is_empty() || !self.allow_rerr(now) {
            return actions;
        }

        actions.push(self.rerr_send_action(
            precursors.iter().next().copied(),
            precursors.len(),
            Message::Rerr(Rerr {
                no_delete: no_delete.unwrap_or(false),
                unreachable_destinations: unreachable,
            }),
        ));
        actions
    }

    fn update_route(&mut self, update: RouteUpdate) -> bool {
        match self.routes.get_mut(&update.destination) {
            Some(existing) => {
                if !should_update_route(existing, &update) {
                    return false;
                }
                existing.next_hop = update.next_hop;
                if let Some(sequence_number) = update.sequence_number {
                    existing.sequence_number = sequence_number;
                }
                existing.sequence_number_valid = update.sequence_number_valid;
                existing.state = update.state;
                existing.hop_count = update.hop_count;
                existing.lifetime = existing.lifetime.max(update.lifetime);
                existing.created_by_hello = update.created_by_hello;
                true
            }
            None => {
                self.routes.insert(
                    update.destination,
                    RouteEntry {
                        destination: update.destination,
                        sequence_number: update.sequence_number.unwrap_or_default(),
                        sequence_number_valid: update.sequence_number_valid,
                        state: update.state,
                        hop_count: update.hop_count,
                        next_hop: update.next_hop,
                        precursors: BTreeSet::new(),
                        lifetime: update.lifetime,
                        created_by_hello: update.created_by_hello,
                    },
                );
                true
            }
        }
    }

    fn initial_ttl_for(&self, destination: Ipv4Addr) -> u8 {
        match self.routes.get(&destination) {
            Some(route) if route.state == RouteState::Invalid => {
                let ttl = route.hop_count.saturating_add(self.config.ttl_increment);
                if ttl >= self.config.ttl_threshold {
                    self.config.net_diameter
                } else {
                    ttl
                }
            }
            _ => self.config.ttl_start,
        }
    }

    fn build_rreq(&mut self, destination: Ipv4Addr) -> Rreq {
        self.local_sequence_number = self.local_sequence_number.wrapping_add(1);
        self.next_rreq_id = self.next_rreq_id.wrapping_add(1);
        let existing = self.routes.get(&destination);
        let sequence_number = existing
            .map(|route| route.sequence_number)
            .unwrap_or_default();
        let known = existing.is_some_and(|route| route.sequence_number_valid);

        Rreq {
            join: false,
            repair: false,
            gratuitous_rrep: true,
            destination_only: false,
            unknown_sequence_number: !known,
            hop_count: 0,
            rreq_id: self.next_rreq_id,
            destination_ip: destination,
            destination_sequence_number: sequence_number,
            originator_ip: self.config.local_ip,
            originator_sequence_number: self.local_sequence_number,
        }
    }

    fn discovery_wait_duration(&self, ttl: u8, retries_at_net_diameter: usize) -> Duration {
        if ttl >= self.config.net_diameter {
            duration_mul(
                self.config.net_traversal_time(),
                1u32 << retries_at_net_diameter.min(31),
            )
        } else {
            self.config.ring_traversal_time(ttl)
        }
    }

    fn send_rreq(&mut self, message: Rreq, ttl: u8, now: Instant) -> Option<Action> {
        if !self.allow_rreq(now) {
            return None;
        }
        Some(self.broadcast_action(Message::Rreq(message), ttl, now))
    }

    fn unicast_action(&self, target: Ipv4Addr, message: Message, ttl: u8) -> Action {
        Action::Send(SendAction {
            target: SendTarget::Unicast(target),
            ttl,
            message,
        })
    }

    fn broadcast_action(&mut self, message: Message, ttl: u8, now: Instant) -> Action {
        self.last_broadcast_at = Some(now);
        Action::Send(SendAction {
            target: SendTarget::Broadcast,
            ttl,
            message,
        })
    }

    fn rerr_send_action(
        &self,
        single_precursor: Option<Ipv4Addr>,
        precursor_count: usize,
        message: Message,
    ) -> Action {
        let target = if precursor_count <= 1 {
            single_precursor
                .map(SendTarget::Unicast)
                .unwrap_or(SendTarget::Broadcast)
        } else {
            SendTarget::Broadcast
        };
        Action::Send(SendAction {
            target,
            ttl: 1,
            message,
        })
    }

    fn allow_rreq(&mut self, now: Instant) -> bool {
        prune_rate_limiter(&mut self.recent_rreq_emissions, now);
        if self.recent_rreq_emissions.len() >= self.config.rreq_ratelimit {
            return false;
        }
        self.recent_rreq_emissions.push_back(now);
        true
    }

    fn allow_rerr(&mut self, now: Instant) -> bool {
        prune_rate_limiter(&mut self.recent_rerr_emissions, now);
        if self.recent_rerr_emissions.len() >= self.config.rerr_ratelimit {
            return false;
        }
        self.recent_rerr_emissions.push_back(now);
        true
    }

    fn ensure_neighbor_route(&mut self, source: Ipv4Addr, now: Instant) {
        self.update_route(RouteUpdate {
            destination: source,
            next_hop: source,
            sequence_number: None,
            sequence_number_valid: false,
            hop_count: 1,
            lifetime: now + self.config.active_route_timeout,
            state: RouteState::Valid,
            created_by_hello: false,
        });
    }

    fn note_neighbor_contact(
        &mut self,
        source: Ipv4Addr,
        now: Instant,
        hello_timeout: Option<Duration>,
    ) {
        self.neighbors
            .entry(source)
            .and_modify(|entry| {
                entry.last_heard = now;
                if hello_timeout.is_some() {
                    entry.hello_timeout = hello_timeout;
                }
            })
            .or_insert(NeighborState {
                last_heard: now,
                hello_timeout,
            });
    }

    fn has_active_route(&self, destination: Ipv4Addr, now: Instant) -> bool {
        self.routes
            .get(&destination)
            .is_some_and(|route| route.state == RouteState::Valid && route.lifetime > now)
    }

    fn has_active_routes(&self, now: Instant) -> bool {
        self.routes.values().any(|route| {
            route.destination != self.config.local_ip
                && route.state == RouteState::Valid
                && route.lifetime > now
        })
    }

    fn should_emit_hello(&self, now: Instant) -> bool {
        self.config.enable_hello
            && self.has_active_routes(now)
            && self
                .last_broadcast_at
                .map(|last| last + self.config.hello_interval <= now)
                .unwrap_or(true)
    }

    fn prune_caches(&mut self, now: Instant) {
        self.seen_rreqs.retain(|_, deadline| *deadline > now);
        prune_rate_limiter(&mut self.recent_rreq_emissions, now);
        prune_rate_limiter(&mut self.recent_rerr_emissions, now);
    }
}

#[derive(Debug, Clone)]
struct RouteUpdate {
    destination: Ipv4Addr,
    next_hop: Ipv4Addr,
    sequence_number: Option<u32>,
    sequence_number_valid: bool,
    hop_count: u8,
    lifetime: Instant,
    state: RouteState,
    created_by_hello: bool,
}

fn should_update_route(existing: &RouteEntry, update: &RouteUpdate) -> bool {
    if update.sequence_number_valid {
        if !existing.sequence_number_valid {
            return true;
        }
        if let Some(sequence_number) = update.sequence_number {
            if seq_gt(sequence_number, existing.sequence_number) {
                return true;
            }
            if sequence_number == existing.sequence_number {
                return existing.state == RouteState::Invalid
                    || update.state == RouteState::Valid && update.hop_count < existing.hop_count;
            }
        }
        false
    } else {
        existing.state == RouteState::Invalid || update.hop_count < existing.hop_count
    }
}

fn prune_rate_limiter(queue: &mut VecDeque<Instant>, now: Instant) {
    while queue
        .front()
        .is_some_and(|instant| *instant + Duration::from_secs(1) <= now)
    {
        queue.pop_front();
    }
}

fn seq_gt(incoming: u32, current: u32) -> bool {
    (incoming.wrapping_sub(current) as i32) > 0
}

fn seq_ge(incoming: u32, current: u32) -> bool {
    (incoming.wrapping_sub(current) as i32) >= 0
}

#[cfg(test)]
mod tests {
    use super::*;

    fn test_config(local_ip: Ipv4Addr) -> Config {
        Config {
            local_ip,
            bind_ip: local_ip,
            port: 10_654,
            ..Config::default()
        }
    }

    #[test]
    fn route_discovery_emits_rreq_and_tracks_pending_state() {
        let local_ip = Ipv4Addr::new(10, 0, 0, 1);
        let destination = Ipv4Addr::new(10, 0, 0, 99);
        let mut engine = Engine::new(test_config(local_ip));
        let now = Instant::now();

        let actions = engine.start_route_discovery(destination, now);

        assert_eq!(actions.len(), 1);
        let Action::Send(send) = &actions[0] else {
            panic!("expected send action");
        };
        assert_eq!(send.target, SendTarget::Broadcast);
        assert_eq!(send.ttl, engine.config().ttl_start);
        let Message::Rreq(rreq) = &send.message else {
            panic!("expected rreq");
        };
        assert_eq!(rreq.destination_ip, destination);
        assert!(engine.pending_discoveries.contains_key(&destination));
    }

    #[test]
    fn duplicate_rreq_is_suppressed() {
        let local_ip = Ipv4Addr::new(10, 0, 0, 2);
        let source = Ipv4Addr::new(10, 0, 0, 1);
        let destination = Ipv4Addr::new(10, 0, 0, 3);
        let mut engine = Engine::new(test_config(local_ip));
        let now = Instant::now();
        let packet = IncomingPacket {
            source,
            ttl: Some(4),
            message: Message::Rreq(Rreq {
                join: false,
                repair: false,
                gratuitous_rrep: false,
                destination_only: false,
                unknown_sequence_number: true,
                hop_count: 0,
                rreq_id: 7,
                destination_ip: destination,
                destination_sequence_number: 0,
                originator_ip: source,
                originator_sequence_number: 10,
            }),
        };

        let first = engine.handle_incoming(packet.clone(), now);
        let second = engine.handle_incoming(packet, now + Duration::from_millis(5));

        assert_eq!(first.len(), 1);
        assert!(second.is_empty());
    }

    #[test]
    fn destination_generates_rrep_for_rreq() {
        let local_ip = Ipv4Addr::new(10, 0, 0, 5);
        let source = Ipv4Addr::new(10, 0, 0, 1);
        let mut engine = Engine::new(test_config(local_ip));
        let now = Instant::now();

        let actions = engine.handle_incoming(
            IncomingPacket {
                source,
                ttl: Some(5),
                message: Message::Rreq(Rreq {
                    join: false,
                    repair: false,
                    gratuitous_rrep: false,
                    destination_only: false,
                    unknown_sequence_number: true,
                    hop_count: 0,
                    rreq_id: 1,
                    destination_ip: local_ip,
                    destination_sequence_number: 0,
                    originator_ip: source,
                    originator_sequence_number: 2,
                }),
            },
            now,
        );

        assert_eq!(actions.len(), 1);
        let Action::Send(send) = &actions[0] else {
            panic!("expected send action");
        };
        assert_eq!(send.target, SendTarget::Unicast(source));
        let Message::Rrep(rrep) = &send.message else {
            panic!("expected rrep");
        };
        assert_eq!(rrep.destination_ip, local_ip);
        assert_eq!(rrep.originator_ip, source);
    }

    #[test]
    fn intermediate_route_generates_rrep() {
        let local_ip = Ipv4Addr::new(10, 0, 0, 2);
        let source = Ipv4Addr::new(10, 0, 0, 1);
        let destination = Ipv4Addr::new(10, 0, 0, 9);
        let now = Instant::now();
        let mut engine = Engine::new(test_config(local_ip));

        engine.update_route(RouteUpdate {
            destination,
            next_hop: Ipv4Addr::new(10, 0, 0, 4),
            sequence_number: Some(7),
            sequence_number_valid: true,
            hop_count: 2,
            lifetime: now + Duration::from_secs(30),
            state: RouteState::Valid,
            created_by_hello: false,
        });

        let actions = engine.handle_incoming(
            IncomingPacket {
                source,
                ttl: Some(4),
                message: Message::Rreq(Rreq {
                    join: false,
                    repair: false,
                    gratuitous_rrep: false,
                    destination_only: false,
                    unknown_sequence_number: false,
                    hop_count: 0,
                    rreq_id: 7,
                    destination_ip: destination,
                    destination_sequence_number: 7,
                    originator_ip: source,
                    originator_sequence_number: 3,
                }),
            },
            now,
        );

        assert_eq!(actions.len(), 1);
        let Action::Send(send) = &actions[0] else {
            panic!("expected send action");
        };
        assert_eq!(send.target, SendTarget::Unicast(source));
        let Message::Rrep(rrep) = &send.message else {
            panic!("expected rrep");
        };
        assert_eq!(rrep.destination_ip, destination);
        assert_eq!(rrep.destination_sequence_number, 7);
    }

    #[test]
    fn incoming_rrep_resolves_pending_discovery() {
        let local_ip = Ipv4Addr::new(10, 0, 0, 1);
        let source = Ipv4Addr::new(10, 0, 0, 2);
        let destination = Ipv4Addr::new(10, 0, 0, 99);
        let now = Instant::now();
        let mut engine = Engine::new(test_config(local_ip));

        engine.start_route_discovery(destination, now);

        let actions = engine.handle_incoming(
            IncomingPacket {
                source,
                ttl: Some(4),
                message: Message::Rrep(Rrep {
                    repair: false,
                    acknowledgement_required: false,
                    prefix_size: 0,
                    hop_count: 0,
                    destination_ip: destination,
                    destination_sequence_number: 10,
                    originator_ip: local_ip,
                    lifetime_ms: 5_000,
                    hello_interval_ms: None,
                }),
            },
            now + Duration::from_millis(50),
        );

        assert_eq!(actions.len(), 1);
        assert!(matches!(
            &actions[0],
            Action::RouteDiscovered {
                destination: d,
                next_hop,
                hop_count: 1
            } if *d == destination && *next_hop == source
        ));
    }

    #[test]
    fn hello_refreshes_neighbor_route() {
        let local_ip = Ipv4Addr::new(10, 0, 0, 4);
        let neighbor = Ipv4Addr::new(10, 0, 0, 5);
        let now = Instant::now();
        let mut engine = Engine::new(test_config(local_ip));

        let actions = engine.handle_incoming(
            IncomingPacket {
                source: neighbor,
                ttl: Some(1),
                message: Message::Rrep(Rrep::hello(neighbor, 4, 2_000, 1_000)),
            },
            now,
        );

        assert!(actions.is_empty());
        let route = engine.route(neighbor).unwrap();
        assert_eq!(route.state, RouteState::Valid);
        assert_eq!(route.next_hop, neighbor);
        assert_eq!(route.hop_count, 1);
        assert!(route.created_by_hello);
    }

    #[test]
    fn link_loss_generates_rerr() {
        let local_ip = Ipv4Addr::new(10, 0, 0, 7);
        let next_hop = Ipv4Addr::new(10, 0, 0, 8);
        let destination = Ipv4Addr::new(10, 0, 0, 9);
        let precursor = Ipv4Addr::new(10, 0, 0, 1);
        let now = Instant::now();
        let mut engine = Engine::new(test_config(local_ip));

        engine.routes.insert(
            destination,
            RouteEntry {
                destination,
                sequence_number: 5,
                sequence_number_valid: true,
                state: RouteState::Valid,
                hop_count: 2,
                next_hop,
                precursors: BTreeSet::from([precursor]),
                lifetime: now + Duration::from_secs(5),
                created_by_hello: false,
            },
        );

        let actions = engine.handle_link_loss(next_hop, now);

        assert!(actions.iter().any(|action| matches!(
            action,
            Action::RouteInvalidated {
                destination: d,
                next_hop: n
            } if *d == destination && *n == next_hop
        )));
        assert!(
            actions
                .iter()
                .any(|action| matches!(action, Action::Send(_)))
        );
    }
}
