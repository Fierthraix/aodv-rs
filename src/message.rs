use std::net::Ipv4Addr;

use bytes::{BufMut, Bytes, BytesMut};
use thiserror::Error;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Message {
    Rreq(Rreq),
    Rrep(Rrep),
    Rerr(Rerr),
    RrepAck,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Rreq {
    pub join: bool,
    pub repair: bool,
    pub gratuitous_rrep: bool,
    pub destination_only: bool,
    pub unknown_sequence_number: bool,
    pub hop_count: u8,
    pub rreq_id: u32,
    pub destination_ip: Ipv4Addr,
    pub destination_sequence_number: u32,
    pub originator_ip: Ipv4Addr,
    pub originator_sequence_number: u32,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Rrep {
    pub repair: bool,
    pub acknowledgement_required: bool,
    pub prefix_size: u8,
    pub hop_count: u8,
    pub destination_ip: Ipv4Addr,
    pub destination_sequence_number: u32,
    pub originator_ip: Ipv4Addr,
    pub lifetime_ms: u32,
    pub hello_interval_ms: Option<u32>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Rerr {
    pub no_delete: bool,
    pub unreachable_destinations: Vec<UnreachableDestination>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct UnreachableDestination {
    pub destination_ip: Ipv4Addr,
    pub destination_sequence_number: u32,
}

#[derive(Debug, Error, PartialEq, Eq)]
pub enum MessageError {
    #[error("empty datagram")]
    Empty,
    #[error("invalid length for message type {message_type}: {length}")]
    InvalidLength { message_type: u8, length: usize },
    #[error("invalid message type {0}")]
    InvalidType(u8),
    #[error("invalid extension type {extension_type} length {length}")]
    InvalidExtension { extension_type: u8, length: usize },
}

impl Message {
    pub fn decode(bytes: &[u8]) -> Result<Self, MessageError> {
        let message_type = *bytes.first().ok_or(MessageError::Empty)?;
        match message_type {
            1 if bytes.len() == 24 => Ok(Self::Rreq(Rreq::decode(bytes))),
            2 if bytes.len() >= 20 => Ok(Self::Rrep(Rrep::decode(bytes)?)),
            3 if bytes.len() >= 12 && (bytes.len() - 4).is_multiple_of(8) => {
                Ok(Self::Rerr(Rerr::decode(bytes)))
            }
            4 if bytes.len() == 2 => Ok(Self::RrepAck),
            1..=4 => Err(MessageError::InvalidLength {
                message_type,
                length: bytes.len(),
            }),
            _ => Err(MessageError::InvalidType(message_type)),
        }
    }

    pub fn encode(&self) -> Bytes {
        match self {
            Self::Rreq(message) => message.encode(),
            Self::Rrep(message) => message.encode(),
            Self::Rerr(message) => message.encode(),
            Self::RrepAck => Bytes::from_static(&[4, 0]),
        }
    }
}

impl Rreq {
    pub fn decode(bytes: &[u8]) -> Self {
        Self {
            join: bytes[1] & (1 << 7) != 0,
            repair: bytes[1] & (1 << 6) != 0,
            gratuitous_rrep: bytes[1] & (1 << 5) != 0,
            destination_only: bytes[1] & (1 << 4) != 0,
            unknown_sequence_number: bytes[1] & (1 << 3) != 0,
            hop_count: bytes[3],
            rreq_id: read_u32(&bytes[4..8]),
            destination_ip: read_ipv4(&bytes[8..12]),
            destination_sequence_number: read_u32(&bytes[12..16]),
            originator_ip: read_ipv4(&bytes[16..20]),
            originator_sequence_number: read_u32(&bytes[20..24]),
        }
    }

    pub fn encode(&self) -> Bytes {
        let mut buffer = BytesMut::with_capacity(24);
        buffer.put_u8(1);
        buffer.put_u8(
            (u8::from(self.join) << 7)
                | (u8::from(self.repair) << 6)
                | (u8::from(self.gratuitous_rrep) << 5)
                | (u8::from(self.destination_only) << 4)
                | (u8::from(self.unknown_sequence_number) << 3),
        );
        buffer.put_u8(0);
        buffer.put_u8(self.hop_count);
        buffer.put_u32(self.rreq_id);
        buffer.extend_from_slice(&self.destination_ip.octets());
        buffer.put_u32(self.destination_sequence_number);
        buffer.extend_from_slice(&self.originator_ip.octets());
        buffer.put_u32(self.originator_sequence_number);
        buffer.freeze()
    }
}

impl Rrep {
    pub fn decode(bytes: &[u8]) -> Result<Self, MessageError> {
        if bytes.len() < 20 {
            return Err(MessageError::InvalidLength {
                message_type: 2,
                length: bytes.len(),
            });
        }

        let mut hello_interval_ms = None;
        let mut index = 20;
        while index < bytes.len() {
            if index + 2 > bytes.len() {
                return Err(MessageError::InvalidExtension {
                    extension_type: 0,
                    length: bytes.len() - index,
                });
            }
            let extension_type = bytes[index];
            let length = bytes[index + 1] as usize;
            let start = index + 2;
            let end = start + length;
            if end > bytes.len() {
                return Err(MessageError::InvalidExtension {
                    extension_type,
                    length,
                });
            }
            match (extension_type, length) {
                (1, 4) => hello_interval_ms = Some(read_u32(&bytes[start..end])),
                _ => {
                    return Err(MessageError::InvalidExtension {
                        extension_type,
                        length,
                    });
                }
            }
            index = end;
        }

        Ok(Self {
            repair: bytes[1] & (1 << 7) != 0,
            acknowledgement_required: bytes[1] & (1 << 6) != 0,
            prefix_size: bytes[2] & 0b1_1111,
            hop_count: bytes[3],
            destination_ip: read_ipv4(&bytes[4..8]),
            destination_sequence_number: read_u32(&bytes[8..12]),
            originator_ip: read_ipv4(&bytes[12..16]),
            lifetime_ms: read_u32(&bytes[16..20]),
            hello_interval_ms,
        })
    }

    pub fn encode(&self) -> Bytes {
        let mut buffer = BytesMut::with_capacity(26);
        buffer.put_u8(2);
        buffer
            .put_u8((u8::from(self.repair) << 7) | (u8::from(self.acknowledgement_required) << 6));
        buffer.put_u8(self.prefix_size & 0b1_1111);
        buffer.put_u8(self.hop_count);
        buffer.extend_from_slice(&self.destination_ip.octets());
        buffer.put_u32(self.destination_sequence_number);
        buffer.extend_from_slice(&self.originator_ip.octets());
        buffer.put_u32(self.lifetime_ms);

        if let Some(hello_interval_ms) = self.hello_interval_ms {
            buffer.put_u8(1);
            buffer.put_u8(4);
            buffer.put_u32(hello_interval_ms);
        }

        buffer.freeze()
    }

    pub fn is_hello(&self, sender: Ipv4Addr, ttl: Option<u8>) -> bool {
        self.hop_count == 0
            && self.destination_ip == sender
            && self.originator_ip == sender
            && ttl == Some(1)
    }

    pub fn hello(
        sender: Ipv4Addr,
        sequence_number: u32,
        lifetime_ms: u32,
        hello_interval_ms: u32,
    ) -> Self {
        Self {
            repair: false,
            acknowledgement_required: false,
            prefix_size: 0,
            hop_count: 0,
            destination_ip: sender,
            destination_sequence_number: sequence_number,
            originator_ip: sender,
            lifetime_ms,
            hello_interval_ms: Some(hello_interval_ms),
        }
    }
}

impl Rerr {
    pub fn decode(bytes: &[u8]) -> Self {
        let mut unreachable_destinations = Vec::with_capacity(bytes[3] as usize);
        let mut index = 4;
        while index < bytes.len() {
            unreachable_destinations.push(UnreachableDestination {
                destination_ip: read_ipv4(&bytes[index..index + 4]),
                destination_sequence_number: read_u32(&bytes[index + 4..index + 8]),
            });
            index += 8;
        }
        Self {
            no_delete: bytes[1] & (1 << 7) != 0,
            unreachable_destinations,
        }
    }

    pub fn encode(&self) -> Bytes {
        let mut buffer = BytesMut::with_capacity(4 + self.unreachable_destinations.len() * 8);
        buffer.put_u8(3);
        buffer.put_u8(u8::from(self.no_delete) << 7);
        buffer.put_u8(0);
        buffer.put_u8(self.unreachable_destinations.len() as u8);
        for destination in &self.unreachable_destinations {
            buffer.extend_from_slice(&destination.destination_ip.octets());
            buffer.put_u32(destination.destination_sequence_number);
        }
        buffer.freeze()
    }
}

fn read_u32(bytes: &[u8]) -> u32 {
    u32::from_be_bytes(bytes.try_into().expect("u32 requires four bytes"))
}

fn read_ipv4(bytes: &[u8]) -> Ipv4Addr {
    let octets: [u8; 4] = bytes.try_into().expect("ipv4 requires four octets");
    Ipv4Addr::from(octets)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rreq_round_trip() {
        let rreq = Rreq {
            join: true,
            repair: false,
            gratuitous_rrep: true,
            destination_only: false,
            unknown_sequence_number: true,
            hop_count: 144,
            rreq_id: 14425,
            destination_ip: Ipv4Addr::new(192, 168, 10, 14),
            destination_sequence_number: 12,
            originator_ip: Ipv4Addr::new(192, 168, 10, 19),
            originator_sequence_number: 63,
        };

        let bytes = rreq.encode();
        assert_eq!(
            bytes.as_ref(),
            &[
                1, 168, 0, 144, 0, 0, 56, 89, 192, 168, 10, 14, 0, 0, 0, 12, 192, 168, 10, 19, 0,
                0, 0, 63,
            ]
        );
        assert_eq!(Message::decode(&bytes).unwrap(), Message::Rreq(rreq));
    }

    #[test]
    fn rrep_round_trip_with_hello_extension() {
        let rrep = Rrep {
            repair: true,
            acknowledgement_required: false,
            prefix_size: 31,
            hop_count: 98,
            destination_ip: Ipv4Addr::new(192, 168, 10, 14),
            destination_sequence_number: 12,
            originator_ip: Ipv4Addr::new(192, 168, 10, 19),
            lifetime_ms: 32603,
            hello_interval_ms: Some(1_000),
        };

        let bytes = rrep.encode();
        assert_eq!(bytes.len(), 26);
        assert_eq!(Message::decode(&bytes).unwrap(), Message::Rrep(rrep));
    }

    #[test]
    fn rerr_round_trip() {
        let rerr = Rerr {
            no_delete: false,
            unreachable_destinations: vec![
                UnreachableDestination {
                    destination_ip: Ipv4Addr::new(192, 168, 10, 18),
                    destination_sequence_number: 482_755,
                },
                UnreachableDestination {
                    destination_ip: Ipv4Addr::new(255, 255, 255, 255),
                    destination_sequence_number: 0,
                },
            ],
        };

        let bytes = rerr.encode();
        assert_eq!(
            bytes.as_ref(),
            &[
                3, 0, 0, 2, 192, 168, 10, 18, 0, 7, 93, 195, 255, 255, 255, 255, 0, 0, 0, 0,
            ]
        );
        assert_eq!(Message::decode(&bytes).unwrap(), Message::Rerr(rerr));
    }
}
