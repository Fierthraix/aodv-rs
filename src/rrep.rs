use std::net::Ipv4Addr;
use std::error::Error;
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
        if b[0] != 1 {
            return Err("This byte message is not the right type");
        }
        let r = 1 << 7 & b[1] != 0;
        let a = 1 << 6 & b[1] != 0;
        // TODO: Fix prefix size!
        Ok(RREP {
            r: r,
            a: a,
            prefix_size: b[2],
            hop_count: b[3],
            dest_ip: Ipv4Addr::new(b[4], b[5], b[6], b[7]),
            dest_seq_num: as_u32_be(&b[8..12]).unwrap(),
            orig_ip: Ipv4Addr::new(b[12], b[13], b[14], b[15]),
            lifetime: as_u32_be(&b[16..30]).unwrap(),
        })
    }

    pub fn bit_message<'a>(&'a self) -> &'a[u8] {
        let 'a bytes= &[
            2,
            if self.r { 1 << 7 } else { 0 } + if self.a { 1 << 6 } else { 0 },
            self.prefix_size % 32,
            self.hop_count,

        ]
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
        4,
        250,
        59,
        ];

    assert_eq!(bytes, rrep.bitMessage());
    //assert_eq!(rrep, RREP::new(bytes).unwrap())
}
