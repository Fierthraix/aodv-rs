use std::io;
use std::net::Ipv4Addr;

use crate::config::{Config, DataPlaneMode};
use crate::engine::{Action, BufferedPacket};

#[derive(Debug)]
pub enum DataPlaneEvent {
    Packet {
        destination: Ipv4Addr,
        packet: BufferedPacket,
    },
    LocalDelivery {
        packet: BufferedPacket,
    },
}

pub enum DataPlane {
    // ControlOnly is useful for protocol testing and route visibility: the
    // daemon runs the RFC control plane but only logs data-plane actions.
    ControlOnly,
    #[cfg(target_os = "linux")]
    TunOverlay(linux::TunOverlay),
    #[cfg(target_os = "linux")]
    KernelRoutes(linux::KernelRoutes),
}

impl DataPlane {
    pub async fn new(config: &Config) -> io::Result<Self> {
        match config.data_plane {
            DataPlaneMode::ControlOnly => Ok(Self::ControlOnly),
            DataPlaneMode::TunOverlay => {
                #[cfg(target_os = "linux")]
                {
                    linux::TunOverlay::new(config).await.map(Self::TunOverlay)
                }
                #[cfg(not(target_os = "linux"))]
                {
                    unsupported("tun-overlay")
                }
            }
            DataPlaneMode::KernelRoutes => {
                #[cfg(target_os = "linux")]
                {
                    linux::KernelRoutes::new(config)
                        .await
                        .map(Self::KernelRoutes)
                }
                #[cfg(not(target_os = "linux"))]
                {
                    unsupported("kernel-routes")
                }
            }
        }
    }

    pub fn has_events(&self) -> bool {
        #[cfg(target_os = "linux")]
        {
            matches!(self, Self::TunOverlay(_))
        }

        #[cfg(not(target_os = "linux"))]
        {
            false
        }
    }

    pub async fn next_event(&mut self) -> io::Result<DataPlaneEvent> {
        match self {
            // No payload source exists in control-only mode, so this branch is
            // never selected unless has_events() is changed incorrectly.
            Self::ControlOnly => std::future::pending().await,
            #[cfg(target_os = "linux")]
            Self::TunOverlay(tun) => tun.next_event().await,
            #[cfg(target_os = "linux")]
            Self::KernelRoutes(_) => std::future::pending().await,
        }
    }

    pub async fn handle_action(&mut self, action: Action) -> io::Result<()> {
        match self {
            Self::ControlOnly => log_control_action(action),
            #[cfg(target_os = "linux")]
            Self::TunOverlay(tun) => tun.handle_action(action).await,
            #[cfg(target_os = "linux")]
            Self::KernelRoutes(routes) => routes.handle_action(action).await,
        }
    }

    pub async fn deliver_local(&mut self, packet: BufferedPacket) -> io::Result<()> {
        match self {
            Self::ControlOnly => {
                tracing::debug!(
                    id = packet.id,
                    length = packet.payload.len(),
                    "local data packet"
                );
                Ok(())
            }
            #[cfg(target_os = "linux")]
            Self::TunOverlay(tun) => tun.deliver_local(packet).await,
            #[cfg(target_os = "linux")]
            Self::KernelRoutes(_) => Ok(()),
        }
    }
}

fn log_control_action(action: Action) -> io::Result<()> {
    match action {
        Action::ForwardBufferedPackets {
            destination,
            next_hop,
            packets,
        } => tracing::info!(
            %destination,
            %next_hop,
            count = packets.len(),
            "buffered packets ready to forward"
        ),
        Action::DropBufferedPackets {
            destination,
            packets,
        } => tracing::warn!(
            %destination,
            count = packets.len(),
            "dropping buffered packets"
        ),
        Action::RouteDiscovered {
            destination,
            next_hop,
            hop_count,
        } => tracing::info!(%destination, %next_hop, hop_count, "route discovered"),
        Action::RouteInvalidated {
            destination,
            next_hop,
        } => tracing::info!(%destination, %next_hop, "route invalidated"),
        Action::RouteDiscoveryFailed { destination } => {
            tracing::warn!(%destination, "route discovery failed");
        }
        Action::LocalRepairStarted { destination, ttl } => {
            tracing::info!(%destination, ttl, "local repair started");
        }
        Action::LocalRepairFailed { destination } => {
            tracing::warn!(%destination, "local repair failed");
        }
        Action::Send(_) => {}
    }
    Ok(())
}

#[cfg(not(target_os = "linux"))]
fn unsupported(mode: &str) -> io::Result<DataPlane> {
    Err(io::Error::new(
        io::ErrorKind::Unsupported,
        format!("{mode} data plane is currently supported only on Linux"),
    ))
}

#[cfg(target_os = "linux")]
mod linux {
    use std::collections::BTreeSet;
    use std::io;
    use std::net::{IpAddr, Ipv4Addr, SocketAddr};

    use etherparse::Ipv4HeaderSlice;
    use futures_util::TryStreamExt;
    use rtnetlink::{Handle, RouteMessageBuilder, new_connection};
    use tokio::net::UdpSocket;
    use tun_rs::{AsyncDevice, DeviceBuilder};

    use super::{DataPlaneEvent, log_control_action};
    use crate::config::Config;
    use crate::engine::{Action, BufferedPacket};

    pub struct TunOverlay {
        // TUN overlay keeps user IPv4 packets in userspace and forwards them to
        // the next hop over UDP after the AODV engine resolves a route.
        device: AsyncDevice,
        data_socket: UdpSocket,
        data_port: u16,
        mtu: usize,
        routes: LinuxRoutes,
    }

    #[derive(Debug)]
    pub struct KernelRoutes {
        routes: LinuxRoutes,
    }

    #[derive(Debug)]
    struct LinuxRoutes {
        handle: Handle,
        interface: String,
        route_metric: u32,
        managed: BTreeSet<Ipv4Addr>,
    }

    impl TunOverlay {
        pub async fn new(config: &Config) -> io::Result<Self> {
            let device = DeviceBuilder::new()
                .name(config.tun_name.clone())
                .mtu(config.tun_mtu)
                .build_async()
                .map_err(|error| {
                    io::Error::new(
                        error.kind(),
                        format!(
                            "failed to create/open TUN device {}: {error}",
                            config.tun_name
                        ),
                    )
                })?;
            let actual_name = device.name()?;
            let data_socket = UdpSocket::bind((config.bind_ip, config.data_port())).await?;
            let routes = LinuxRoutes::new(actual_name, config.route_metric).await?;

            tracing::info!(
                tun = %config.tun_name,
                data_port = config.data_port(),
                mtu = config.tun_mtu,
                "TUN overlay data plane started"
            );

            Ok(Self {
                device,
                data_socket,
                data_port: config.data_port(),
                mtu: config.tun_mtu as usize,
                routes,
            })
        }

        pub async fn next_event(&mut self) -> io::Result<DataPlaneEvent> {
            let mut tun_buffer = vec![0_u8; self.mtu.max(1500)];
            let mut udp_buffer = vec![0_u8; self.mtu.max(1500)];

            tokio::select! {
                result = self.device.recv(&mut tun_buffer) => {
                    // Packets from the TUN device are original IPv4 payloads
                    // that need route discovery if the destination is unknown.
                    let length = result?;
                    tun_buffer.truncate(length);
                    let destination = ipv4_destination(&tun_buffer)?;
                    Ok(DataPlaneEvent::Packet {
                        destination,
                        packet: BufferedPacket {
                            id: 0,
                            payload: tun_buffer,
                        },
                    })
                }
                result = self.data_socket.recv_from(&mut udp_buffer) => {
                    // Overlay UDP packets have already been forwarded by an
                    // upstream AODV next hop; they either continue through the
                    // engine or are delivered locally.
                    let (length, source) = result?;
                    udp_buffer.truncate(length);
                    let destination = ipv4_destination(&udp_buffer)?;
                    let source = match source.ip() {
                        IpAddr::V4(ip) => ip,
                        IpAddr::V6(_) => Ipv4Addr::UNSPECIFIED,
                    };
                    tracing::debug!(%source, %destination, length, "received overlay data packet");
                    Ok(DataPlaneEvent::Packet {
                        destination,
                        packet: BufferedPacket {
                            id: 0,
                            payload: udp_buffer,
                        },
                    })
                }
            }
        }

        pub async fn handle_action(&mut self, action: Action) -> io::Result<()> {
            match action {
                Action::ForwardBufferedPackets {
                    destination,
                    next_hop,
                    packets,
                } => {
                    for packet in packets {
                        self.forward_packet(destination, next_hop, packet).await?;
                    }
                    Ok(())
                }
                Action::RouteDiscovered { destination, .. } => {
                    // The TUN mode installs a host route to the virtual device
                    // so the kernel sends future packets for that destination
                    // into this daemon.
                    self.routes.install_dev_route(destination).await
                }
                Action::RouteInvalidated { destination, .. }
                | Action::RouteDiscoveryFailed { destination } => {
                    self.routes.remove_route(destination).await
                }
                other => log_control_action(other),
            }
        }

        pub async fn deliver_local(&self, packet: BufferedPacket) -> io::Result<()> {
            self.device.send(&packet.payload).await?;
            Ok(())
        }

        async fn forward_packet(
            &self,
            destination: Ipv4Addr,
            next_hop: Ipv4Addr,
            packet: BufferedPacket,
        ) -> io::Result<()> {
            let target = SocketAddr::new(IpAddr::V4(next_hop), self.data_port);
            self.data_socket.send_to(&packet.payload, target).await?;
            tracing::debug!(
                %destination,
                %next_hop,
                id = packet.id,
                length = packet.payload.len(),
                "forwarded overlay data packet"
            );
            Ok(())
        }
    }

    impl KernelRoutes {
        pub async fn new(config: &Config) -> io::Result<Self> {
            let interface = config.interface.clone().ok_or_else(|| {
                io::Error::new(
                    io::ErrorKind::InvalidInput,
                    "--interface is required for kernel-routes data plane",
                )
            })?;
            Ok(Self {
                routes: LinuxRoutes::new(interface, config.route_metric).await?,
            })
        }

        pub async fn handle_action(&mut self, action: Action) -> io::Result<()> {
            match action {
                Action::RouteDiscovered {
                    destination,
                    next_hop,
                    ..
                } => {
                    // Kernel-routes mode lets the OS forward packets directly
                    // to the AODV next hop once the control plane discovers it.
                    self.routes
                        .install_gateway_route(destination, next_hop)
                        .await
                }
                Action::RouteInvalidated { destination, .. }
                | Action::RouteDiscoveryFailed { destination } => {
                    self.routes.remove_route(destination).await
                }
                other => log_control_action(other),
            }
        }
    }

    impl LinuxRoutes {
        async fn new(interface: String, route_metric: u32) -> io::Result<Self> {
            let (connection, handle, _) =
                new_connection().map_err(|error| io::Error::other(error.to_string()))?;
            tokio::spawn(connection);
            Ok(Self {
                handle,
                interface,
                route_metric,
                managed: BTreeSet::new(),
            })
        }

        async fn install_dev_route(&mut self, destination: Ipv4Addr) -> io::Result<()> {
            let ifindex = self.interface_index().await?;
            self.remove_route(destination).await?;
            let mut route = RouteMessageBuilder::<Ipv4Addr>::new()
                .destination_prefix(destination, 32)
                .output_interface(ifindex)
                .build();
            route
                .attributes
                .push(rtnetlink::packet_route::route::RouteAttribute::Priority(
                    self.route_metric,
                ));
            self.handle
                .route()
                .add(route)
                .execute()
                .await
                .map_err(|error| io::Error::other(error.to_string()))?;
            self.managed.insert(destination);
            Ok(())
        }

        async fn install_gateway_route(
            &mut self,
            destination: Ipv4Addr,
            next_hop: Ipv4Addr,
        ) -> io::Result<()> {
            let ifindex = self.interface_index().await?;
            self.remove_route(destination).await?;
            let mut route = RouteMessageBuilder::<Ipv4Addr>::new()
                .destination_prefix(destination, 32)
                .gateway(next_hop)
                .output_interface(ifindex)
                .build();
            route
                .attributes
                .push(rtnetlink::packet_route::route::RouteAttribute::Priority(
                    self.route_metric,
                ));
            self.handle
                .route()
                .add(route)
                .execute()
                .await
                .map_err(|error| io::Error::other(error.to_string()))?;
            self.managed.insert(destination);
            Ok(())
        }

        async fn remove_route(&mut self, destination: Ipv4Addr) -> io::Result<()> {
            if !self.managed.remove(&destination) {
                return Ok(());
            }
            let route = RouteMessageBuilder::<Ipv4Addr>::new()
                .destination_prefix(destination, 32)
                .build();
            match self.handle.route().del(route).execute().await {
                Ok(()) => Ok(()),
                Err(error) => {
                    tracing::debug!(%destination, %error, "managed route removal failed");
                    Ok(())
                }
            }
        }

        async fn interface_index(&self) -> io::Result<u32> {
            let link = self
                .handle
                .link()
                .get()
                .match_name(self.interface.clone())
                .execute()
                .try_next()
                .await
                .map_err(|error| io::Error::other(error.to_string()))?
                .ok_or_else(|| {
                    io::Error::new(
                        io::ErrorKind::NotFound,
                        format!("no interface found named {}", self.interface),
                    )
                })?;
            Ok(link.header.index)
        }
    }

    fn ipv4_destination(packet: &[u8]) -> io::Result<Ipv4Addr> {
        let header = Ipv4HeaderSlice::from_slice(packet).map_err(|error| {
            io::Error::new(
                io::ErrorKind::InvalidData,
                format!("data plane packet is not valid IPv4: {error}"),
            )
        })?;
        Ok(header.destination_addr())
    }
}
