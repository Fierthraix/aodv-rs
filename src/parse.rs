extern crate clap;
extern crate yaml_rust;

use self::clap::{Arg, App, ArgMatches};
use self::yaml_rust::YamlLoader;

use std::fs::File;
use std::io::BufReader;
use std::io::prelude::*;
use std::time::Duration;
use std::net::Ipv4Addr;
use std::str::FromStr;

//TODO: add lazy static to see if that helps

/// The object that holds both user-set variables and aodv constants
#[allow(non_snake_case)]
#[derive(Debug, PartialEq)]
pub struct Config {
    pub current_ip: Ipv4Addr,
    pub interface: String,
    pub broadcast_address: Ipv4Addr,
    pub port: u16,

    pub ACTIVE_ROUTE_TIMEOUT: Duration,
    pub ALLOWED_HELLO_LOSS: u32,
    pub BLACKLIST_TIMEOUT: Duration,
    pub DELETE_PERIOD: Duration,
    pub HELLO_INTERVAL: Duration,
    pub LOCAL_ADD_TTL: usize,
    pub MAX_REPAIR_TTL: f64,
    pub MIN_REPAIR_TTL: usize,
    pub MY_ROUTE_TIMEOUT: Duration,
    pub NET_DIAMETER: usize,
    pub NET_TRAVERSAL_TIME: Duration,
    pub NEXT_HOP_WAIT: Duration,
    pub NODE_TRAVERSAL_TIME: Duration,
    pub PATH_DISCOVERY_TIME: Duration,
    pub RERR_RATELIMIT: usize,
    pub RING_TRAVERSAL_TIME: Duration,
    pub RREQ_RETRIES: usize,
    pub RREQ_RATELIMIT: usize,
    pub TIMEOUT_BUFFER: usize,
    pub TTL_START: usize,
    pub TTL_INCREMENT: usize,
    pub TTL_THRESHOLD: usize,
    pub TTL_VALUE: usize,
}

//TODO: add tests for everything
impl Config {
    /// Get the global config using both a .yaml file and the command line input
    pub fn new(args: &ArgMatches) -> Self {
        // Load the default config
        let mut config = Config::default_config();

        // Change elements from a config file if supplied
        if let Some(file) = args.value_of("config_file") {
            config.read_config(File::open(file).unwrap());
        }

        // Change any arguments from stdin
        config.read_args(args);

        config
    }

    fn default_config() -> Self {
        Config {
            current_ip: Ipv4Addr::new(0, 0, 0, 0),
            interface: "wlano".parse().unwrap(),
            broadcast_address: Ipv4Addr::new(255, 255, 255, 255),
            port: 1200,

            ACTIVE_ROUTE_TIMEOUT: Duration::from_millis(3000),
            ALLOWED_HELLO_LOSS: 2,
            BLACKLIST_TIMEOUT: Duration::from_millis(5000),
            DELETE_PERIOD: Duration::from_millis(15000),
            HELLO_INTERVAL: Duration::from_millis(1000),
            LOCAL_ADD_TTL: 2,
            MAX_REPAIR_TTL: 0.3 * 35.,
            MIN_REPAIR_TTL: 0,
            MY_ROUTE_TIMEOUT: Duration::from_millis(6000),
            NET_DIAMETER: 35,
            NET_TRAVERSAL_TIME: Duration::from_millis(2800),
            NEXT_HOP_WAIT: Duration::from_millis(50),
            NODE_TRAVERSAL_TIME: Duration::from_millis(40),
            PATH_DISCOVERY_TIME: Duration::from_millis(5600),
            RERR_RATELIMIT: 10,
            RING_TRAVERSAL_TIME: Duration::from_millis(160),
            RREQ_RETRIES: 2,
            RREQ_RATELIMIT: 10,
            TIMEOUT_BUFFER: 2,
            TTL_START: 1,
            TTL_INCREMENT: 2,
            TTL_THRESHOLD: 7,
            TTL_VALUE: 0,
        }
    }

    fn read_config(&mut self, file: File) {
        // Read file into string with buffered reader
        let mut buf_reader = BufReader::new(file);
        let mut contents = String::new();
        // TODO handle the unwrap properly
        buf_reader.read_to_string(&mut contents).unwrap();

        // Read string file into Yaml file
        // TODO: get rid of this unwrap
        let yaml_file = YamlLoader::load_from_str(&contents).unwrap();
        // First doc (there is multi-document support)
        let doc = &yaml_file[0];

        // TODO use an iter() here or something more generic
        // TODO: Use log error reporting when something fails

        // Replace appropriate arguments
        doc["Interface"].as_str().map(|x| {
            self.interface = String::from(x)
        });
        doc["BroadcastAddress"].as_str().map(|x| {
            if Ipv4Addr::from_str(x).is_ok() {
                self.broadcast_address = Ipv4Addr::from_str(x).unwrap();
            }
        });
        doc["Port"].as_i64().map(|x| self.port = x as u16);
        doc["ACTIVE_ROUTE_TIMEOUT"].as_i64().map(|x| {
            self.ACTIVE_ROUTE_TIMEOUT = Duration::from_millis(x as u64)
        });
        doc["ALLOWED_HELLO_LOSS"].as_i64().map(|x| {
            self.ALLOWED_HELLO_LOSS = x as u32
        });
        doc["HELLO_INTERVAL"].as_i64().map(|x| {
            self.HELLO_INTERVAL = Duration::from_millis(x as u64)
        });
        doc["LOCAL_ADD_TTL"].as_i64().map(|x| {
            self.LOCAL_ADD_TTL = x as usize
        });
        doc["NET_DIAMETER"].as_i64().map(|x| {
            self.NET_DIAMETER = x as usize
        });
        doc["NODE_TRAVERSAL_TIME"].as_i64().map(|x| {
            self.NODE_TRAVERSAL_TIME = Duration::from_millis(x as u64)
        });
        doc["RERR_RATELIMIT"].as_i64().map(|x| {
            self.RERR_RATELIMIT = x as usize
        });
        doc["RREQ_RETRIES"].as_i64().map(|x| {
            self.RREQ_RETRIES = x as usize
        });
        doc["RREQ_RATELIMIT"].as_i64().map(|x| {
            self.RREQ_RATELIMIT = x as usize
        });
        doc["TIMEOUT_BUFFER"].as_i64().map(|x| {
            self.TIMEOUT_BUFFER = x as usize
        });
        doc["TTL_START"].as_i64().map(
            |x| self.TTL_START = x as usize,
        );
        doc["TTL_INCREMENT"].as_i64().map(|x| {
            self.TTL_INCREMENT = x as usize
        });
        doc["TTL_THRESHOLD"].as_i64().map(|x| {
            self.TTL_THRESHOLD = x as usize
        });

        self.compute_values();
    }

    fn read_args(&mut self, args: &ArgMatches) {
        args.value_of("current_ip").map(
            |x| match Ipv4Addr::from_str(x) {
                Ok(ip) => self.current_ip = ip,
                _ => {}
            },
        );
        args.value_of("port").map(|x| match x.parse::<u16>() {
            Ok(port) => self.port = port,
            _ => {}
        });
    }

    fn compute_values(&mut self) {
        // Arbitrary value; see Section 10.
        let k = 5;

        //TODO fix these unwraps
        if self.ACTIVE_ROUTE_TIMEOUT > self.HELLO_INTERVAL {
            self.DELETE_PERIOD = self.ACTIVE_ROUTE_TIMEOUT.checked_mul(k).unwrap();
        } else {
            self.DELETE_PERIOD = self.HELLO_INTERVAL.checked_mul(k).unwrap();
        }
        self.MAX_REPAIR_TTL = 0.3 * self.NET_DIAMETER as f64;
        self.MY_ROUTE_TIMEOUT = self.ACTIVE_ROUTE_TIMEOUT.checked_mul(2).unwrap();
        self.NET_TRAVERSAL_TIME = self.NODE_TRAVERSAL_TIME
            .checked_mul(2 * self.NET_DIAMETER as u32)
            .unwrap();
        self.BLACKLIST_TIMEOUT = self.NET_TRAVERSAL_TIME
            .checked_mul(self.RREQ_RETRIES as u32)
            .unwrap();
        self.NEXT_HOP_WAIT = self.NODE_TRAVERSAL_TIME
            .checked_add(Duration::from_millis(10))
            .unwrap();
        self.PATH_DISCOVERY_TIME = self.NET_TRAVERSAL_TIME.checked_mul(2).unwrap();
        self.RING_TRAVERSAL_TIME = self.NODE_TRAVERSAL_TIME *
            (2 * (self.TTL_VALUE + self.TIMEOUT_BUFFER)) as u32;
    }
}

///  Parse the command line arguments or print help/usage information
pub fn get_args() -> ArgMatches<'static> {
    let matches = App::new("aodv")
        .version("0.0.1")
        .about(
            "Implements the AODV routing protocol as defined in RFC 3561",
        )
        .arg(
            Arg::with_name("port")
                .short("p")
                .long("port")
                .value_name("PORT")
                .help("The port to run the tcp server on.")
                .takes_value(true),
        )
        .arg(Arg::with_name("start_aodv").short("s").long("start").help(
            "Start the aodv daemon",
        ))
        .arg(
            Arg::with_name("current_ip")
                .long("ip")
                .value_name("IP ADDRESS")
                .help("The current IP address of the device")
                .takes_value(true),
        )
        .arg(
            Arg::with_name("config_file")
                .short("c")
                .long("config")
                .value_name("CONFIG FILE")
                .help("Alternate config file")
                .takes_value(true),
        )
        .get_matches();

    // Validate submitted Ipv4Addr
    if let Some(ip_str) = matches.value_of("current_ip") {
        if let Err(e) = Ipv4Addr::from_str(ip_str) {
            eprintln!("incorrectly formatted ip address: {}", e);
        }
    }

    matches
}

#[test]
fn test_parse_config() {

    let config = r#"Interface: "wlan1"
BroadcastAddress: "192.168.10.251"
Port: 1201
ACTIVE_ROUTE_TIMEOUT: 3001 # milliseconds
ALLOWED_HELLO_LOSS: 3
HELLO_INTERVAL: 1001 # milliseconds
LOCAL_ADD_TTL: 3
NET_DIAMETER: 36
NODE_TRAVERSAL_TIME: 41 # milliseconds
RERR_RATELIMIT: 11 # messages per second
RREQ_RETRIES: 3
RREQ_RATELIMIT: 11 # messages per second
TIMEOUT_BUFFER: 3
TTL_START: 2
TTL_INCREMENT: 3
TTL_THRESHOLD: 8
"#;

    use std::env::temp_dir;
    use std::fs::{File, remove_file};

    // Save modified config file to tmp file

    let mut tmp = temp_dir();
    tmp.push("config.yaml");

    // Scope the creation of `c` to automatically close it
    {
        let mut c = File::create(&tmp).unwrap();
        c.write_all(config.as_bytes()).unwrap();
    }

    // Create default config
    let mut config1 = Config::default_config();
    let mut config2 = Config::default_config();

    // Change config1 based on the `.yaml` file
    config1.read_config(File::open(&tmp).unwrap());

    // Manually calculated chagnes
    let config2 = Config {
        interface: String::from("wlan1"),
        broadcast_address: Ipv4Addr::new(192, 168, 10, 251),
        current_ip: config1.current_ip,
        port: 1201,
        ACTIVE_ROUTE_TIMEOUT: Duration::from_millis(3001),
        ALLOWED_HELLO_LOSS: 3,
        BLACKLIST_TIMEOUT: Duration::from_millis(8856),
        DELETE_PERIOD: Duration::from_millis(15005),
        HELLO_INTERVAL: Duration::from_millis(1001),
        LOCAL_ADD_TTL: 3,
        MIN_REPAIR_TTL: 0,
        MAX_REPAIR_TTL: config1.MAX_REPAIR_TTL, // Float, subject to error
        MY_ROUTE_TIMEOUT: Duration::from_millis(6002),
        NET_DIAMETER: 36,
        NET_TRAVERSAL_TIME: Duration::from_millis(2952),
        NEXT_HOP_WAIT: Duration::from_millis(51),
        NODE_TRAVERSAL_TIME: Duration::from_millis(41),
        PATH_DISCOVERY_TIME: Duration::from_millis(5904),
        RERR_RATELIMIT: 11,
        RING_TRAVERSAL_TIME: Duration::from_millis(246),
        RREQ_RETRIES: 3,
        RREQ_RATELIMIT: 11,
        TIMEOUT_BUFFER: 3,
        TTL_START: 2,
        TTL_INCREMENT: 3,
        TTL_THRESHOLD: 8,
        TTL_VALUE: 0,
    };

    assert_eq!(config1, config2);

    // Clean up tmp file
    remove_file(tmp);
}
