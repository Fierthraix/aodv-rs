extern crate clap;
use self::clap::{Arg, App, ArgMatches};

use std::net::Ipv4Addr;
use std::str::FromStr;
use std::process;

pub struct Config {}

// start
// destip
// findroute
// printtable
// currentip

//  Parse the command line arguments
pub fn get_args() -> ArgMatches<'static> {
    let matches = App::new("aodv")
        .version("0.0.1")
        .about("Implements the AODV routing protocol as defined in RFC")
        .arg(
            Arg::with_name("port")
                .short("p")
                .long("port")
                .value_name("PORT")
                .help("The port to run the tcp server on.")
                .takes_value(true),
        )
        .arg(
            Arg::with_name("start_aodv")
                .short("s")
                .long("start")
                .value_name("start_aodv"),
        )
        .arg(
            Arg::with_name("current_ip")
                .long("ip")
                .value_name("IP ADDRESS")
                .help("The current IP address of the device")
                .takes_value(true),
        )
        .get_matches();

    // Check inputs are valid

    // TODO: Checkuot Ipv4Addr::is_broadcast -> bool

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

    matches
}
