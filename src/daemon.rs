use std::io;
use std::net::{IpAddr, Ipv4Addr, SocketAddr, SocketAddrV4};
use std::time::Instant;

use socket2::{Domain, Protocol, Socket, Type};
use tokio::net::UdpSocket;
use tokio::signal;
use tracing::{debug, info, warn};

#[cfg(any(target_os = "linux", target_os = "macos"))]
use std::ffi::CString;
#[cfg(any(target_os = "linux", target_os = "macos"))]
use std::mem::{size_of, zeroed};
#[cfg(any(target_os = "linux", target_os = "macos"))]
use std::os::fd::AsRawFd;
#[cfg(target_os = "linux")]
use std::os::fd::RawFd;
#[cfg(any(target_os = "linux", target_os = "macos"))]
use std::ptr::addr_of_mut;
#[cfg(target_family = "unix")]
use std::ptr::null_mut;
#[cfg(windows)]
use std::{ffi::CStr, os::windows::io::AsRawSocket, ptr::null_mut as win_null_mut};

#[cfg(windows)]
use windows_sys::Win32::{
    Foundation::{ERROR_BUFFER_OVERFLOW, ERROR_SUCCESS},
    NetworkManagement::IpHelper::{
        GAA_FLAG_INCLUDE_PREFIX, GetAdaptersAddresses, IP_ADAPTER_ADDRESSES_LH,
    },
    Networking::WinSock::{
        AF_INET, IP_UNICAST_IF, IPPROTO_IP, SOCKADDR, SOCKADDR_IN, SOCKET_ERROR, setsockopt,
    },
};

use crate::config::Config;
use crate::engine::{Action, Engine, IncomingPacket, SendAction, SendTarget};
use crate::message::Message;

pub async fn run(config: Config) -> io::Result<()> {
    let config = finalize_runtime_config(config)?;
    let socket = bind_socket(&config)?;
    let bind_addr = socket.local_addr()?;

    info!(
        %bind_addr,
        local_ip = %config.local_ip,
        interface = ?config.interface,
        "aodv daemon started"
    );

    let mut engine = Engine::new(config.clone());
    let mut buffer = [0_u8; 2048];

    loop {
        let now = Instant::now();
        let deadline = engine.next_deadline(now);

        tokio::select! {
            biased;
            result = signal::ctrl_c() => {
                result?;
                info!("received shutdown signal");
                return Ok(());
            }
            result = recv_datagram(&socket, &mut buffer) => {
                let (length, source_addr, ttl) = result?;
                let source = match source_addr.ip() {
                    IpAddr::V4(ipv4) => ipv4,
                    IpAddr::V6(_) => {
                        warn!(%source_addr, "ignoring ipv6 sender for ipv4 AODV daemon");
                        continue;
                    }
                };

                match Message::decode(&buffer[..length]) {
                    Ok(message) => {
                        debug!(%source, length, ttl, "received AODV datagram");
                        let actions = engine.handle_incoming(
                            IncomingPacket {
                                source,
                                ttl,
                                message,
                            },
                            Instant::now(),
                        );
                        execute_actions(&socket, &config, actions).await?;
                    }
                    Err(error) => warn!(%source, %error, "dropping invalid datagram"),
                }
            }
            _ = sleep_until_deadline(deadline), if deadline.is_some() => {
                let actions = engine.tick(Instant::now());
                execute_actions(&socket, &config, actions).await?;
            }
        }
    }
}

fn finalize_runtime_config(mut config: Config) -> io::Result<Config> {
    if config.local_ip == Ipv4Addr::UNSPECIFIED {
        if let Some(interface) = &config.interface {
            config.local_ip = interface_ipv4_addr(interface)?;
        } else if config.bind_ip != Ipv4Addr::UNSPECIFIED {
            config.local_ip = config.bind_ip;
        } else {
            return Err(io::Error::new(
                io::ErrorKind::InvalidInput,
                "local_ip is unspecified; pass --local-ip, --bind-ip, or --interface for real-device operation",
            ));
        }
    }

    Ok(config)
}

fn bind_socket(config: &Config) -> io::Result<UdpSocket> {
    let socket = Socket::new(Domain::IPV4, Type::DGRAM, Some(Protocol::UDP))?;
    socket.set_reuse_address(true)?;
    socket.set_broadcast(true)?;
    socket.set_nonblocking(true)?;

    if let Some(interface) = &config.interface {
        bind_socket_to_interface(&socket, interface)?;
    }

    enable_recv_ttl(&socket)?;

    let bind_addr = SocketAddrV4::new(config.bind_ip, config.aodv_port());
    let bind_result = socket.bind(&bind_addr.into());
    if let Err(error) = bind_result {
        if error.kind() == io::ErrorKind::PermissionDenied && config.aodv_port() < 1024 {
            return Err(io::Error::new(
                io::ErrorKind::PermissionDenied,
                format!(
                    "binding UDP port {} requires root or CAP_NET_BIND_SERVICE: {error}",
                    config.aodv_port()
                ),
            ));
        }
        return Err(error);
    }

    let std_socket: std::net::UdpSocket = socket.into();
    UdpSocket::from_std(std_socket)
}

async fn recv_datagram(
    socket: &UdpSocket,
    buffer: &mut [u8],
) -> io::Result<(usize, SocketAddr, Option<u8>)> {
    loop {
        socket.readable().await?;
        match try_recv_datagram(socket, buffer) {
            Ok(result) => return Ok(result),
            Err(error) if error.kind() == io::ErrorKind::WouldBlock => continue,
            Err(error) => return Err(error),
        }
    }
}

fn try_recv_datagram(
    socket: &UdpSocket,
    buffer: &mut [u8],
) -> io::Result<(usize, SocketAddr, Option<u8>)> {
    #[cfg(any(target_os = "linux", target_os = "macos"))]
    {
        try_recv_datagram_with_ttl(socket, buffer)
    }

    #[cfg(not(any(target_os = "linux", target_os = "macos")))]
    {
        socket
            .try_recv_from(buffer)
            .map(|(size, source_addr)| (size, source_addr, None))
    }
}

#[cfg(any(target_os = "linux", target_os = "macos"))]
fn try_recv_datagram_with_ttl(
    socket: &UdpSocket,
    buffer: &mut [u8],
) -> io::Result<(usize, SocketAddr, Option<u8>)> {
    let raw_fd = socket.as_raw_fd();
    let mut source_addr: libc::sockaddr_storage = unsafe { zeroed() };
    let mut iov = libc::iovec {
        iov_base: buffer.as_mut_ptr().cast(),
        iov_len: buffer.len(),
    };
    let mut control = [0_u8; 64];
    let mut message: libc::msghdr = unsafe { zeroed() };
    message.msg_name = addr_of_mut!(source_addr).cast();
    message.msg_namelen = size_of::<libc::sockaddr_storage>() as libc::socklen_t;
    message.msg_iov = addr_of_mut!(iov);
    message.msg_iovlen = 1;
    message.msg_control = control.as_mut_ptr().cast();
    message.msg_controllen = control.len() as _;

    let received = unsafe { libc::recvmsg(raw_fd, &mut message, 0) };
    if received < 0 {
        return Err(io::Error::last_os_error());
    }

    let address = socket_addr_from_storage(&source_addr, message.msg_namelen)?;
    let ttl = unsafe { ttl_from_cmsgs(&message) };
    Ok((received as usize, address, ttl))
}

async fn sleep_until_deadline(deadline: Option<Instant>) {
    if let Some(deadline) = deadline {
        tokio::time::sleep_until(tokio::time::Instant::from_std(deadline)).await;
    }
}

async fn execute_actions(
    socket: &UdpSocket,
    config: &Config,
    actions: Vec<Action>,
) -> io::Result<()> {
    for action in actions {
        match action {
            Action::Send(send) => send_action(socket, config, &send).await?,
            Action::ForwardBufferedPackets {
                destination,
                next_hop,
                packets,
            } => info!(
                %destination,
                %next_hop,
                count = packets.len(),
                "buffered packets ready to forward"
            ),
            Action::DropBufferedPackets {
                destination,
                packets,
            } => warn!(
                %destination,
                count = packets.len(),
                "dropping buffered packets"
            ),
            Action::RouteDiscovered {
                destination,
                next_hop,
                hop_count,
            } => info!(%destination, %next_hop, hop_count, "route discovered"),
            Action::RouteInvalidated {
                destination,
                next_hop,
            } => {
                info!(%destination, %next_hop, "route invalidated");
            }
            Action::RouteDiscoveryFailed { destination } => {
                warn!(%destination, "route discovery failed");
            }
            Action::LocalRepairStarted { destination, ttl } => {
                info!(%destination, ttl, "local repair started");
            }
            Action::LocalRepairFailed { destination } => {
                warn!(%destination, "local repair failed");
            }
        }
    }
    Ok(())
}

pub(crate) async fn send_action(
    socket: &UdpSocket,
    config: &Config,
    action: &SendAction,
) -> io::Result<()> {
    socket.set_ttl(action.ttl as u32)?;
    let ip = match action.target {
        SendTarget::Unicast(ip) => ip,
        SendTarget::Broadcast => config.broadcast_ip,
    };
    let destination = SocketAddr::new(IpAddr::V4(ip), config.aodv_port());
    let bytes = action.message.encode();
    socket.send_to(bytes.as_ref(), destination).await?;
    Ok(())
}

#[cfg(target_family = "unix")]
fn interface_ipv4_addr(interface: &str) -> io::Result<Ipv4Addr> {
    let mut ifaddrs = null_mut();
    let result = unsafe { libc::getifaddrs(&mut ifaddrs) };
    if result != 0 {
        return Err(io::Error::last_os_error());
    }

    let mut current = ifaddrs;
    let mut found = None;

    while !current.is_null() {
        let entry = unsafe { &*current };
        if !entry.ifa_name.is_null() && !entry.ifa_addr.is_null() {
            let name = unsafe { std::ffi::CStr::from_ptr(entry.ifa_name) };
            if name.to_string_lossy() == interface {
                let family = unsafe { (*entry.ifa_addr).sa_family as i32 };
                if family == libc::AF_INET {
                    let sockaddr = unsafe { &*(entry.ifa_addr as *const libc::sockaddr_in) };
                    let ip = Ipv4Addr::from(u32::from_be(sockaddr.sin_addr.s_addr));
                    found = Some(ip);
                    break;
                }
            }
        }
        current = unsafe { (*current).ifa_next };
    }

    unsafe { libc::freeifaddrs(ifaddrs) };

    found.ok_or_else(|| {
        io::Error::new(
            io::ErrorKind::NotFound,
            format!("no IPv4 address found for interface {interface}"),
        )
    })
}

#[cfg(windows)]
fn interface_ipv4_addr(interface: &str) -> io::Result<Ipv4Addr> {
    windows_interface(interface).and_then(|win_interface| {
        win_interface.ipv4.ok_or_else(|| {
            io::Error::new(
                io::ErrorKind::NotFound,
                format!("no IPv4 address found for interface {interface}"),
            )
        })
    })
}

#[cfg(target_os = "linux")]
fn bind_socket_to_interface(socket: &Socket, interface: &str) -> io::Result<()> {
    bind_to_device(socket.as_raw_fd(), interface)
}

#[cfg(target_os = "macos")]
fn bind_socket_to_interface(socket: &Socket, interface: &str) -> io::Result<()> {
    let index = unix_interface_index(interface)?;
    let result = unsafe {
        libc::setsockopt(
            socket.as_raw_fd(),
            libc::IPPROTO_IP,
            libc::IP_BOUND_IF,
            (&index as *const libc::c_uint).cast(),
            size_of::<libc::c_uint>() as libc::socklen_t,
        )
    };
    if result == 0 {
        Ok(())
    } else {
        Err(io::Error::new(
            io::Error::last_os_error().kind(),
            format!(
                "failed to bind socket to interface {interface}: {}",
                io::Error::last_os_error()
            ),
        ))
    }
}

#[cfg(windows)]
fn bind_socket_to_interface(socket: &Socket, interface: &str) -> io::Result<()> {
    let index = windows_interface(interface)?.index.to_be();
    let result = unsafe {
        setsockopt(
            socket.as_raw_socket() as usize,
            IPPROTO_IP,
            IP_UNICAST_IF,
            (&index as *const u32).cast(),
            std::mem::size_of::<u32>() as i32,
        )
    };
    if result == SOCKET_ERROR {
        Err(io::Error::new(
            io::Error::last_os_error().kind(),
            format!(
                "failed to bind socket to interface {interface}: {}",
                io::Error::last_os_error()
            ),
        ))
    } else {
        Ok(())
    }
}

#[cfg(target_os = "linux")]
fn bind_to_device(fd: RawFd, interface: &str) -> io::Result<()> {
    let device = CString::new(interface)
        .map_err(|_| io::Error::new(io::ErrorKind::InvalidInput, "interface contains NUL"))?;
    let result = unsafe {
        libc::setsockopt(
            fd,
            libc::SOL_SOCKET,
            libc::SO_BINDTODEVICE,
            device.as_ptr().cast(),
            (device.as_bytes_with_nul().len()) as libc::socklen_t,
        )
    };
    if result == 0 {
        Ok(())
    } else {
        Err(io::Error::new(
            io::Error::last_os_error().kind(),
            format!(
                "failed to bind socket to interface {interface}: {}",
                io::Error::last_os_error()
            ),
        ))
    }
}

#[cfg(any(target_os = "linux", target_os = "macos"))]
fn enable_recv_ttl(socket: &Socket) -> io::Result<()> {
    let enabled: libc::c_int = 1;
    #[cfg(target_os = "linux")]
    let option = libc::IP_RECVTTL;
    #[cfg(target_os = "macos")]
    let option = libc::IP_RECVTTL;

    let result = unsafe {
        libc::setsockopt(
            socket.as_raw_fd(),
            libc::IPPROTO_IP,
            option,
            (&enabled as *const libc::c_int).cast(),
            size_of::<libc::c_int>() as libc::socklen_t,
        )
    };
    if result == 0 {
        Ok(())
    } else {
        Err(io::Error::last_os_error())
    }
}

#[cfg(windows)]
fn enable_recv_ttl(_socket: &Socket) -> io::Result<()> {
    Ok(())
}

#[cfg(any(target_os = "linux", target_os = "macos"))]
fn socket_addr_from_storage(
    storage: &libc::sockaddr_storage,
    length: libc::socklen_t,
) -> io::Result<SocketAddr> {
    if storage.ss_family as i32 != libc::AF_INET
        || length < size_of::<libc::sockaddr_in>() as libc::socklen_t
    {
        return Err(io::Error::new(
            io::ErrorKind::InvalidData,
            "received non-IPv4 UDP datagram",
        ));
    }

    let address =
        unsafe { &*(storage as *const libc::sockaddr_storage as *const libc::sockaddr_in) };
    let ip = Ipv4Addr::from(u32::from_be(address.sin_addr.s_addr));
    let port = u16::from_be(address.sin_port);
    Ok(SocketAddr::V4(SocketAddrV4::new(ip, port)))
}

#[cfg(any(target_os = "linux", target_os = "macos"))]
unsafe fn ttl_from_cmsgs(message: &libc::msghdr) -> Option<u8> {
    let mut cursor = unsafe { libc::CMSG_FIRSTHDR(message) };
    while !cursor.is_null() {
        let header = unsafe { &*cursor };
        #[cfg(target_os = "linux")]
        let ttl_type = libc::IP_TTL;
        #[cfg(target_os = "macos")]
        let ttl_type = libc::IP_RECVTTL;

        if header.cmsg_level == libc::IPPROTO_IP && header.cmsg_type == ttl_type {
            let value = unsafe { libc::CMSG_DATA(cursor) as *const libc::c_int };
            return unsafe { (*value).try_into().ok() };
        }
        cursor = unsafe { libc::CMSG_NXTHDR(message, cursor) };
    }
    None
}

#[cfg(target_os = "macos")]
fn unix_interface_index(interface: &str) -> io::Result<libc::c_uint> {
    let name = CString::new(interface)
        .map_err(|_| io::Error::new(io::ErrorKind::InvalidInput, "interface contains NUL"))?;
    let index = unsafe { libc::if_nametoindex(name.as_ptr()) };
    if index == 0 {
        Err(io::Error::new(
            io::ErrorKind::NotFound,
            format!("no interface found named {interface}"),
        ))
    } else {
        Ok(index)
    }
}

#[cfg(windows)]
#[derive(Debug, Clone, Copy)]
struct WindowsInterface {
    index: u32,
    ipv4: Option<Ipv4Addr>,
}

#[cfg(windows)]
fn windows_interface(interface: &str) -> io::Result<WindowsInterface> {
    let adapters = windows_adapters()?;
    let requested_index = interface.parse::<u32>().ok();
    let mut current: *const IP_ADAPTER_ADDRESSES_LH = adapters.as_ptr().cast();

    while !current.is_null() {
        let adapter = unsafe { &*current };
        let index = unsafe { adapter.Anonymous1.Anonymous.IfIndex };
        if requested_index == Some(index) || windows_adapter_name_matches(adapter, interface) {
            return Ok(WindowsInterface {
                index,
                ipv4: windows_adapter_ipv4(adapter),
            });
        }
        current = adapter.Next;
    }

    Err(io::Error::new(
        io::ErrorKind::NotFound,
        format!("no interface found named {interface}"),
    ))
}

#[cfg(windows)]
fn windows_adapters() -> io::Result<Vec<u8>> {
    let mut size = 15 * 1024;
    loop {
        let mut buffer = vec![0_u8; size as usize];
        let result = unsafe {
            GetAdaptersAddresses(
                AF_INET as u32,
                GAA_FLAG_INCLUDE_PREFIX,
                win_null_mut(),
                buffer.as_mut_ptr().cast::<IP_ADAPTER_ADDRESSES_LH>(),
                &mut size,
            )
        };

        match result {
            ERROR_SUCCESS => return Ok(buffer),
            ERROR_BUFFER_OVERFLOW => continue,
            error => {
                return Err(io::Error::other(format!(
                    "GetAdaptersAddresses failed with error {error}"
                )));
            }
        }
    }
}

#[cfg(windows)]
fn windows_adapter_name_matches(adapter: &IP_ADAPTER_ADDRESSES_LH, interface: &str) -> bool {
    let adapter_name = unsafe { c_string_lossy(adapter.AdapterName.cast_const().cast()) };
    let friendly_name = unsafe { wide_string_lossy(adapter.FriendlyName.cast_const()) };
    let description = unsafe { wide_string_lossy(adapter.Description.cast_const()) };

    [adapter_name, friendly_name, description]
        .into_iter()
        .flatten()
        .any(|name| name == interface)
}

#[cfg(windows)]
fn windows_adapter_ipv4(adapter: &IP_ADAPTER_ADDRESSES_LH) -> Option<Ipv4Addr> {
    let mut current = adapter.FirstUnicastAddress;
    while !current.is_null() {
        let address = unsafe { &(*current).Address };
        if let Some(ip) = unsafe { ipv4_from_sockaddr(address.lpSockaddr, address.iSockaddrLength) }
        {
            return Some(ip);
        }
        current = unsafe { (*current).Next };
    }
    None
}

#[cfg(windows)]
unsafe fn ipv4_from_sockaddr(sockaddr: *const SOCKADDR, length: i32) -> Option<Ipv4Addr> {
    if sockaddr.is_null()
        || length < std::mem::size_of::<SOCKADDR_IN>() as i32
        || unsafe { (*sockaddr).sa_family } != AF_INET
    {
        return None;
    }

    let address = unsafe { &*(sockaddr.cast::<SOCKADDR_IN>()) };
    let raw = unsafe { address.sin_addr.S_un.S_addr };
    Some(Ipv4Addr::from(u32::from_be(raw)))
}

#[cfg(windows)]
unsafe fn c_string_lossy(value: *const i8) -> Option<String> {
    (!value.is_null()).then(|| unsafe { CStr::from_ptr(value).to_string_lossy().into_owned() })
}

#[cfg(windows)]
unsafe fn wide_string_lossy(value: *const u16) -> Option<String> {
    if value.is_null() {
        return None;
    }

    let mut length = 0;
    while unsafe { *value.add(length) } != 0 {
        length += 1;
    }
    Some(String::from_utf16_lossy(unsafe {
        std::slice::from_raw_parts(value, length)
    }))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::message::{Rrep, Rreq};

    fn loopback_config(port: u16) -> Config {
        Config {
            local_ip: Ipv4Addr::new(127, 0, 0, 1),
            bind_ip: Ipv4Addr::new(127, 0, 0, 1),
            port,
            ..Config::default()
        }
    }

    #[tokio::test]
    async fn send_action_writes_udp_datagram() {
        let receiver = UdpSocket::bind("127.0.0.1:0").await.unwrap();
        let sender = UdpSocket::bind("127.0.0.1:0").await.unwrap();

        let receiver_addr = receiver.local_addr().unwrap();
        let config = loopback_config(receiver_addr.port());

        let action = SendAction {
            target: SendTarget::Unicast(Ipv4Addr::new(127, 0, 0, 1)),
            ttl: 4,
            message: Message::Rreq(Rreq {
                join: false,
                repair: false,
                gratuitous_rrep: false,
                destination_only: false,
                unknown_sequence_number: true,
                hop_count: 0,
                rreq_id: 9,
                destination_ip: Ipv4Addr::new(10, 0, 0, 9),
                destination_sequence_number: 0,
                originator_ip: Ipv4Addr::new(10, 0, 0, 1),
                originator_sequence_number: 1,
            }),
        };

        send_action(&sender, &config, &action).await.unwrap();

        let mut buffer = [0_u8; 128];
        let (size, _) = receiver.recv_from(&mut buffer).await.unwrap();
        assert!(matches!(
            Message::decode(&buffer[..size]).unwrap(),
            Message::Rreq(_)
        ));

        let hello = SendAction {
            target: SendTarget::Unicast(Ipv4Addr::new(127, 0, 0, 1)),
            ttl: 1,
            message: Message::Rrep(Rrep::hello(Ipv4Addr::new(127, 0, 0, 1), 4, 2_000, 1_000)),
        };
        send_action(&sender, &config, &hello).await.unwrap();
        let (size, _) = receiver.recv_from(&mut buffer).await.unwrap();
        assert!(matches!(
            Message::decode(&buffer[..size]).unwrap(),
            Message::Rrep(_)
        ));
    }

    #[cfg(target_os = "linux")]
    #[tokio::test]
    async fn recv_datagram_reports_inbound_ttl() {
        let socket = bind_socket(&loopback_config(0)).unwrap();
        let local_addr = socket.local_addr().unwrap();
        let sender = std::net::UdpSocket::bind("127.0.0.1:0").unwrap();
        sender.set_ttl(3).unwrap();

        let payload = Rreq {
            join: false,
            repair: false,
            gratuitous_rrep: false,
            destination_only: false,
            unknown_sequence_number: true,
            hop_count: 0,
            rreq_id: 4,
            destination_ip: Ipv4Addr::new(10, 0, 0, 4),
            destination_sequence_number: 0,
            originator_ip: Ipv4Addr::new(10, 0, 0, 1),
            originator_sequence_number: 1,
        }
        .encode();
        sender.send_to(payload.as_ref(), local_addr).unwrap();

        let mut buffer = [0_u8; 128];
        let (size, source_addr, ttl) = recv_datagram(&socket, &mut buffer).await.unwrap();

        assert_eq!(size, payload.len());
        assert_eq!(source_addr.ip(), IpAddr::V4(Ipv4Addr::new(127, 0, 0, 1)));
        assert_eq!(ttl, Some(3));
        assert!(matches!(
            Message::decode(&buffer[..size]).unwrap(),
            Message::Rreq(_)
        ));
    }

    #[test]
    fn finalize_runtime_config_uses_bind_ip_as_local_ip() {
        let config = Config {
            local_ip: Ipv4Addr::UNSPECIFIED,
            bind_ip: Ipv4Addr::new(192, 0, 2, 4),
            ..Config::default()
        };

        let finalized = finalize_runtime_config(config).unwrap();
        assert_eq!(finalized.local_ip, Ipv4Addr::new(192, 0, 2, 4));
    }
}
