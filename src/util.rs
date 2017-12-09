use super::*;

//TODO: make aodv messages implement BigEndianBytes instaed of `parse` and `bit_message`
/// Convert a type to its byte representation in Big Endian Bytes
pub trait BigEndianBytes {
    //TODO: convert this Vec<u8> to any iterable type (so we can return [u8; 4])
    fn as_be_bytes(self: &Self) -> Vec<u8>;
    fn from_be_bytes(slice: &[u8]) -> Self;
}

impl BigEndianBytes for u32 {
    //TODO try using unsafe transmutation!
    fn as_be_bytes(&self) -> Vec<u8> {
        //[(n >> 24) as u8, (n >> 16) as u8, (n >> 8) as u8, n as u8]
        vec![
            (self >> 24) as u8,
            (self >> 16) as u8,
            (self >> 8) as u8,
            *self as u8,
        ]
    }
    fn from_be_bytes(b: &[u8]) -> Self {
        (u32::from(b[0]) << 24) + (u32::from(b[1]) << 16) + (u32::from(b[2]) << 8) + u32::from(b[3])
    }
}

use std::net::{IpAddr, Ipv4Addr, SocketAddr};

/// Get either the underlying ipv4 address associated with a struct, or 0.0.0.0
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
        match *self {
            IpAddr::V4(ip) => ip,
            IpAddr::V6(_) => Ipv4Addr::new(0, 0, 0, 0),
        }
    }
}

pub trait ToAodvSocketAddr {
    fn to_aodv_sa(self: Self) -> SocketAddr;
}

impl ToAodvSocketAddr for Ipv4Addr {
    fn to_aodv_sa(self: Self) -> SocketAddr {
        SocketAddr::new(IpAddr::V4(self), AODV_PORT)
    }
}

#[test]
fn test_conversions() {
    // Test u32 and byte conversions
    let b: u32 = 19381837;
    assert_eq!(u32::from_be_bytes(b.as_be_bytes().as_ref()), b);

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
