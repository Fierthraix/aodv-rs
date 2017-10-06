use std::net::Ipv4Addr;
use std::ops::Deref;
use functions::*;

/*
   RREP Message Format:
   0                   1                   2                   3
   0 1 2 3 4 5 6 7 8 9 0 1 2 3 4 5 6 7 8 9 0 1 2 3 4 5 6 7 8 9 0 1
   +-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+
   |     Type      |R|A|    Reserved     |Prefix Sz|   Hop Count   |
   +-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+
   |                     Destination IP Address                    |
   +-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+
   |                  Destination Sequence Number                  |
   +-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+
   |                    Originator IP Address                      |
   +-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+
   |                           Lifetime                            |
   +-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+
   */

#[derive(Debug, PartialEq)]
pub struct RREP {
    pub r: bool, // Repair flag
    pub a: bool, // Acknowledgment required flag

    pub prefix_size: u8, // 5-bit prefix size
    pub hop_count: u8, // 8-bit Hop Count

    pub dest_ip: Ipv4Addr, //Destination IP
    pub dest_seq_num: u32, //Destination Sequence Number

    pub orig_ip: Ipv4Addr, //Originator IP

    pub lifetime: u32, //Lifetime in milliseconds
}

impl RREP {
    // TODO: recode this to have a parse error or something
    pub fn new(b: &[u8]) -> Result<RREP, &'static str> {
        if b.len() != 20 {
            return Err("This byte message is not the right size");
        }
        if b[0] != 2 {
            return Err("This byte message is not the right type");
        }
        // TODO: Fix prefix size!
        Ok(RREP {
            r: 1 << 7 & b[1] != 0,
            a: 1 << 6 & b[1] != 0,
            prefix_size: b[2],
            hop_count: b[3],
            dest_ip: Ipv4Addr::new(b[4], b[5], b[6], b[7]),
            dest_seq_num: as_u32_be(&b[8..12]),
            orig_ip: Ipv4Addr::new(b[12], b[13], b[14], b[15]),
            lifetime: as_u32_be(&b[16..20]),
        })
    }

    pub fn bit_message(&self) -> [u8; 20] {
        let mut b = [0u8; 20];
        b[0] = 2;
        b[1] = if self.r { 1 << 7 } else { 0 } + if self.a { 1 << 6 } else { 0 };
        // TODO: fix this value!
        b[2] = self.prefix_size;
        b[3] = self.hop_count;

        for i in 0..4 {
            b[i + 4] = self.dest_ip.octets()[i];
            b[i + 8] = u32_as_bytes_be(self.dest_seq_num)[i];
            b[i + 12] = self.orig_ip.octets()[i];
            b[i + 16] = u32_as_bytes_be(self.lifetime)[i];
        }
        b
    }
}

#[test]
fn test_rrep_encoding() {
    let rrep = RREP {
        r: true,
        a: false,
        prefix_size: 31,
        hop_count: 98,
        dest_ip: Ipv4Addr::new(192, 168, 10, 14),
        dest_seq_num: 12,
        orig_ip: Ipv4Addr::new(192, 168, 10, 19),
        lifetime: 32603,
    };

    let bytes: &[u8] = &[
        2,
        128,
        31,
        98,
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
        127,
        91,
    ];

    assert_eq!(bytes, rrep.bit_message());
    assert_eq!(rrep, RREP::new(bytes).unwrap())
}
