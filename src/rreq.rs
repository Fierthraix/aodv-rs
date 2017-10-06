use std::net::Ipv4Addr;
use std::ops::Deref;
use functions::*;

/*
   RREQ Message Format:
   0                   1                   2                   3
   0 1 2 3 4 5 6 7 8 9 0 1 2 3 4 5 6 7 8 9 0 1 2 3 4 5 6 7 8 9 0 1
   +-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+
   |     Type      |J|R|G|D|U|   Reserved          |   Hop Count   |
   +-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+
   |                            RREQ ID                            |
   +-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+
   |                    Destination IP Address                     |
   +-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+
   |                  Destination Sequence Number                  |
   +-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+
   |                    Originator IP Address                      |
   +-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+
   |                  Originator Sequence Number                   |
   +-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+
   */

#[derive(Debug, PartialEq)]
pub struct RREQ {
    pub j: bool, // Join flag
    pub r: bool, // Repair flag
    pub g: bool, // Gratuitous RREP flag
    pub d: bool, // Destination Only flag
    pub u: bool, // Unknown Sequence number

    pub hop_count: u8, // 8-bit Hop Count
    pub rreq_id: u32, // 32-bit RREQ ID

    pub dest_ip: Ipv4Addr, // Destination IP Address
    pub dest_seq_num: u32, // Destination Sequence Number

    pub orig_ip: Ipv4Addr, // Originator IP Address
    pub orig_seq_num: u32, // Originator Sequence Number
}

impl RREQ {
    pub fn new(b: &[u8]) -> Result<RREQ, &'static str> {
        if b.len() != 24 {
            return Err("This message is not the right size");
        }
        if b[0] != 1 {
            return Err("This byte message is not the right type");
        }
        Ok(RREQ {
            j: 1 << 7 & b[1] != 0,
            r: 1 << 6 & b[1] != 0,
            g: 1 << 5 & b[1] != 0,
            d: 1 << 4 & b[1] != 0,
            u: 1 << 3 & b[1] != 0,
            hop_count: b[3],
            rreq_id: as_u32_be(&b[4..8]),
            dest_ip: Ipv4Addr::new(b[8], b[9], b[10], b[11]),
            dest_seq_num: as_u32_be(&b[12..16]),
            orig_ip: Ipv4Addr::new(b[16], b[17], b[18], b[19]),
            orig_seq_num: as_u32_be(&b[20..24]),
        })
    }
    pub fn bit_message(&self) -> [u8; 24] {
        let mut b = [0u8; 24];
        b[0] = 1;
        b[1] = if self.j { 1 << 7 } else { 0 } + if self.r { 1 << 6 } else { 0 } +
            if self.g { 1 << 5 } else { 0 } + if self.d { 1 << 4 } else { 0 } +
            if self.u { 1 << 3 } else { 0 };

        b[3] = self.hop_count;

        for i in 0..4 {
            b[i + 4] = u32_as_bytes_be(self.rreq_id)[i];
            b[i + 8] = self.dest_ip.octets()[i];
            b[i + 12] = u32_as_bytes_be(self.dest_seq_num)[i];
            b[i + 16] = self.orig_ip.octets()[i];
            b[i + 20] = u32_as_bytes_be(self.orig_seq_num)[i];
        }
        b
    }
}

#[test]
fn test_rreq_encoding() {
    let rreq = RREQ {
        j: true,
        r: false,
        g: true,
        d: false,
        u: true,
        hop_count: 144,
        rreq_id: 14425,
        dest_ip: Ipv4Addr::new(192, 168, 10, 14),
        dest_seq_num: 12,
        orig_ip: Ipv4Addr::new(192, 168, 10, 19),
        orig_seq_num: 63,
    };

    let bytes: &[u8] = &[
        1,
        168,
        0,
        144,
        0,
        0,
        56,
        89,
        192,
        168,
        10,
        14,
        0,
        0,
        0,
        12,
        192,
        168,
        10,
        19,
        0,
        0,
        0,
        63,
    ];
    assert_eq!(bytes, rreq.bit_message());
    assert_eq!(rreq, RREQ::new(bytes).unwrap())
}
