extern crate futures;
extern crate tokio_core;

extern crate aodv;

use std::thread;
use std::env::var;
use std::process::exit;

use aodv::{config, server};


fn main() {
    // Get command line arguments
    let args = config::get_args();

    // Start server
    if args.is_present("start_aodv") {

        // Check user is root
        match var("USER") {
            Ok(s) => {
                if s != "root" {
                    eprintln!("Must be root to run the server!");
                    exit(1);
                }
            }
            Err(e) => panic!(e),
        }

        // Start internal server
        let handle = thread::spawn(|| { server::server(); });

        server::aodv();

        handle.join().unwrap();
    } else {
        println!("{}", args.usage());
    }
}
