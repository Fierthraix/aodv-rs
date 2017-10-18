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

#[allow(non_snake_case)]
pub struct Config {
    current_ip: Ipv4Addr,
    interface: String,
    broadcast_address: Ipv4Addr,
    port: u16,

    ACTIVE_ROUTE_TIMEOUT: Duration,
    ALLOWED_HELLO_LOSS: u32,
    BLACKLIST_TIMEOUT: Duration,
    DELETE_PERIOD: Duration,
    HELLO_INTERVAL: Duration,
    LOCAL_ADD_TTL: usize,
    MAX_REPAIR_TTL: f64,
    MIN_REPAIR_TTL: usize,
    MY_ROUTE_TIMEOUT: Duration,
    NET_DIAMETER: usize,
    NET_TRAVERSAL_TIME: Duration,
    NEXT_HOP_WAIT: Duration,
    NODE_TRAVERSAL_TIME: Duration,
    PATH_DISCOVERY_TIME: Duration,
    RERR_RATELIMIT: usize,
    RING_TRAVERSAL_TIME: Duration,
    RREQ_RETRIES: usize,
    RREQ_RATELIMIT: usize,
    TIMEOUT_BUFFER: usize,
    TTL_START: usize,
    TTL_INCREMENT: usize,
    TTL_THRESHOLD: usize,
    TTL_VALUE: usize,
}

impl Config {
    pub fn new(args: &ArgMatches) -> Self {
        // Load the default config
        let mut config = Config::default_config();

        // Change elements from a config file if supplied
        match args.value_of("config_file") {
            Some(file) => {
                config.read_config(File::open(file).unwrap());
            },
            None=>(),
        }

        // Change any arguments from stdin
        config.read_args(args);

        config
    }

    fn default_config() -> Self {
        //TODO: add option for external config file
        let config_file = YamlLoader::load_from_str(DEFAULT_CONFIG)
            .unwrap(); // Built in config file shouldn't fail

        Config {
            current_ip: Ipv4Addr::new(0,0,0,0),
            interface: "wlano".parse().unwrap(),
            broadcast_address: Ipv4Addr::new(255,255,255,255),
            port: 1200,

            ACTIVE_ROUTE_TIMEOUT: Duration::from_millis(3000),
            ALLOWED_HELLO_LOSS: 2,
            BLACKLIST_TIMEOUT: Duration::from_millis(5000),
            DELETE_PERIOD: Duration::from_millis(15000),
            HELLO_INTERVAL: Duration::from_millis(1000),
            LOCAL_ADD_TTL: 2,
            MAX_REPAIR_TTL: 0.3*35.,
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
        // Replace appropriate arguments
        // TODO: Use log error reporting when somethign fails

        doc["Interface"].as_str().map(|x| self.interface = String::from(x));
        doc["BroadcastAddress"].as_str().map(|x|{if Ipv4Addr::from_str(x).is_ok(){
            self.broadcast_address = Ipv4Addr::from_str(x).unwrap();
        }});
        doc["Port"].as_i64().map(|x| self.port = x as u16);
        doc["ACTIVE_ROUTE_TIMEOUT"].as_i64().map(|x|self.ACTIVE_ROUTE_TIMEOUT =
                                                 Duration::from_millis(x as u64));
        doc["ALLOWED_HELLO_LOSS"].as_i64().map(|x| self.ALLOWED_HELLO_LOSS = x as u32);
        doc["HELLO_INTERVAL"].as_i64().map(|x| self.HELLO_INTERVAL =
                                           Duration::from_millis(x as u64));
        doc["LOCAL_ADD_TTL"].as_i64().map(|x| self.LOCAL_ADD_TTL = x as usize);
        doc["NET_DIAMETER"].as_i64().map(|x| self.NET_DIAMETER = x as usize);
        doc["NODE_TRAVERSAL_TIME"].as_i64().map(|x| self.NODE_TRAVERSAL_TIME =
                                                Duration::from_millis(x as u64 ));
        doc["RERR_RATELIMIT"].as_i64().map(|x| self.RERR_RATELIMIT = x as usize);
        doc["RREQ_RETRIES"].as_i64().map(|x| self.RREQ_RETRIES = x as usize);
        doc["RREQ_RATELIMIT"].as_i64().map(|x| self.RREQ_RATELIMIT = x as usize);
        doc["TIMEOUT_BUFFER"].as_i64().map(|x| self.TIMEOUT_BUFFER = x as usize);
        doc["TTL_START"].as_i64().map(|x| self.TTL_START = x as usize);
        doc["TTL_INCREMENT"].as_i64().map(|x| self.TTL_INCREMENT = x as usize);
        doc["TTL_THRESHOLD"].as_i64().map(|x| self.TTL_THRESHOLD = x as usize);

        self.compute_values();
    }

    fn read_args(&mut self, args: &ArgMatches) {
        args.value_of("current_ip").map(|x| { if Ipv4Addr::from_str(x).is_ok(){
            self.current_ip = Ipv4Addr::from_str(x).unwrap();}
        });
        args.value_of("port").map(|x| if x.parse::<u16>().is_ok() {
            self.port = x.parse::<u16>().unwrap();
        });
    }

    fn compute_values(&mut self){
        // Arbitrary value; see Section 10.
        let K = 5;

        //TODO fix these unwraps
        if self.ACTIVE_ROUTE_TIMEOUT > self.HELLO_INTERVAL{
            self.DELETE_PERIOD = self.ACTIVE_ROUTE_TIMEOUT.checked_mul(K).unwrap();
        }else{
            self.DELETE_PERIOD = self.HELLO_INTERVAL.checked_mul(K).unwrap();
        }
        self.MAX_REPAIR_TTL = 0.3*self.NET_DIAMETER as f64;
        self.MY_ROUTE_TIMEOUT = self.ACTIVE_ROUTE_TIMEOUT.checked_mul(2).unwrap();
        self.NET_TRAVERSAL_TIME = self.NODE_TRAVERSAL_TIME.checked_mul(2*self.NET_DIAMETER as u32).unwrap();
        self.BLACKLIST_TIMEOUT = self.NET_TRAVERSAL_TIME.checked_mul(self.RREQ_RETRIES as u32).unwrap();
        self.NEXT_HOP_WAIT = self.NODE_TRAVERSAL_TIME.checked_add(Duration::from_millis(10)).unwrap();
        self.PATH_DISCOVERY_TIME = self.NET_TRAVERSAL_TIME.checked_mul(2).unwrap();
        self.RING_TRAVERSAL_TIME = self.NODE_TRAVERSAL_TIME;
    }
}

//  Parse the command line arguments
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
            .help("Alternate config file")
            .takes_value(true),
            )
        .get_matches();

    // Check inputs are valid

    // TODO: Checkout Ipv4Addr::is_broadcast -> bool

    /*
     * Validate input before returning
     */

    // Run this if value is not None
    if let Some(ip_str) = matches.value_of("current_ip") {
        let current_ip;
        // Check for valid ipv4 address
        match Ipv4Addr::from_str(ip_str) {
            Ok(ip) => current_ip = ip,
            Err(e) => eprintln!("Error getting IP address: {}", e),
        }
    }

    println!("{}", DEFAULT_CONFIG);

    matches
}

//10.
const DEFAULT_CONFIG: &'static str = r#"
Interface: "wlan0"
BroadcastAddress: "192.168.10.255"
Port: 1200
ACTIVE_ROUTE_TIMEOUT: 3000 # milliseconds
ALLOWED_HELLO_LOSS: 2
HELLO_INTERVAL: 1000 # milliseconds
LOCAL_ADD_TTL: 2
NET_DIAMETER: 35
NODE_TRAVERSAL_TIME: 40 # milliseconds
RERR_RATELIMIT: 10 # messages per second
RREQ_RETRIES: 2
RREQ_RATELIMIT: 10 # messages per second
TIMEOUT_BUFFER: 2
TTL_START: 1
TTL_INCREMENT: 2
TTL_THRESHOLD: 7
"#;
