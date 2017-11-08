//TODO: change these conversion functions to traits

/// Convert big endian bytes to a u32
#[inline]
pub fn bytes_as_u32_be(slice: &[u8]) -> u32 {
    ((slice[0] as u32) << 24) + ((slice[1] as u32) << 16) + ((slice[2] as u32) << 8) +
        ((slice[3] as u32) << 0)
}

/// Convert u32 to byte array
#[inline]
pub fn u32_as_bytes_be(n: u32) -> [u8; 4] {
    [(n >> 24) as u8, (n >> 16) as u8, (n >> 8) as u8, n as u8]
}

use std::net::{IpAddr, Ipv4Addr, SocketAddr};

/// Get either the underlying ipv4 address of a struct, or 0.0.0.0
pub trait ToIpv4 {
    fn to_ipv4(self: &Self) -> Ipv4Addr;
}

impl ToIpv4 for SocketAddr {
    fn to_ipv4(&self) -> Ipv4Addr {
        match self.ip() {
            IpAddr::V4(ip) => ip,
            IpAddr::V6(_) => Ipv4Addr::new(0, 0, 0, 0),
        }
    }
}

impl ToIpv4 for IpAddr {
    fn to_ipv4(&self) -> Ipv4Addr {
        match self {
            &IpAddr::V4(ip) => ip,
            &IpAddr::V6(_) => Ipv4Addr::new(0, 0, 0, 0),
        }
    }
}

#[test]
fn test_conversions() {
    // Test u32 and byte conversions
    let b = 19381837;
    assert_eq!(bytes_as_u32_be(&u32_as_bytes_be(b)), b);

    // Test ipv4 conversions
    use std::net::{IpAddr, Ipv4Addr, Ipv6Addr, SocketAddr};

    // Ipv4 socket
    let socket: SocketAddr = "127.0.0.1:8080".parse().unwrap();
    assert_eq!(socket.to_ipv4(), Ipv4Addr::new(127, 0, 0, 1));

    //Ipv6 socket
    let socket = SocketAddr::new(IpAddr::V6(Ipv6Addr::new(0, 0, 0, 0, 0, 0, 0, 1)), 8080);
    assert_eq!(socket.to_ipv4(), Ipv4Addr::new(0, 0, 0, 0));

    // Ipv4 addres
    let ip = IpAddr::V4(Ipv4Addr::new(192, 168, 10, 253));
    assert_eq!(ip.to_ipv4(), Ipv4Addr::new(192, 168, 10, 253));

    //Ipv6 address
    let ip = SocketAddr::new(IpAddr::V4(Ipv4Addr::new(70, 69, 68, 67)), 8080);
    assert_eq!(ip.to_ipv4(), Ipv4Addr::new(70, 69, 68, 67));
}
