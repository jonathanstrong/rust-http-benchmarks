#![allow(unused_imports)]

#[macro_use]
extern crate clap;
#[macro_use]
extern crate slog;

use std::thread;
use std::sync::{Arc, Mutex};
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::{Instant, Duration};
use std::str::{self, FromStr};
use std::io;
use slog::{Drain, Logger};
use pretty_toa::ThousandsSep;

fn parse_uri(s: &str) -> Result<http::Uri, http::uri::InvalidUri> {
    let s = if s.starts_with("http") { s.to_string() } else { format!("https://{}", s) };
    let s = if s.ends_with(":443") { s } else { format!("{}:443", s) };
    http::Uri::from_str(&s)
}

fn main() {
    let args: clap::ArgMatches = clap::App::new("client")
        .version(crate_version!())
        .arg(clap::Arg::with_name("raw-tcp-client")
             .long("raw-tcp-client")
             .short("R")
             .help("Launch a raw tcp client (no TLS, use stunnel bridge if TLS desired), \
                   sending requests to <addr>")
             .takes_value(true)
             .required(false))
        .arg(clap::Arg::with_name("throttle")
             .long("throttle")
             .short("t")
             .help("sleep <n> milliseconds between requests")
             .takes_value(true)
             .required(false))
        .arg(clap::Arg::with_name("openssl-client")
             .long("openssl-client")
             .short("O")
             .help("Launch a raw tcp+tls socket using rust bindings to openssl \
                   sending requests to <addr>")
             .takes_value(true)
             .required(false))
        .get_matches();

    let stop = Arc::new(AtomicBool::new(false));
    //let topo = Arc::new(Mutex::new(hwloc::Topology::new()));

    let decorator = slog_term::TermDecorator::new().stdout().force_color().build();
    let drain = slog_term::CompactFormat::new(decorator).use_utc_timestamp().build().fuse();
    let drain = slog_async::Async::new(drain).chan_size(8192).thread_name("recv".into()).build().fuse();
    let root = slog::Logger::root(drain, o!());
    let logger = root.new(o!("thread" => "main"));

    let throttle =
        args.value_of("throttle")
            .and_then(|millis| {
                u64::from_str(millis).ok()
            }).map(|millis| {
                Duration::from_millis(millis)
            });

    let raw_tcp_client = args.value_of("raw-tcp-client").map(|addr| {
        info!(logger, "launching raw tcp client, sending requests to {}", addr);
        //let topo = Arc::clone(&topo);
        let stop = Arc::clone(&stop);
        //raw_tcp_client(addr, 0, topo, stop, &logger)
        http_benchmarks::raw_tcp_client(addr, stop, throttle, &root)
    });

    let openssl_client = args.value_of("openssl-client").map(|addr_arg| {
        let addr = parse_uri(addr_arg).expect(&format!("failed to parse --openssl-client uri ('{}')", addr_arg));
        assert!(addr.authority_part().is_some());
        info!(logger, "launching raw tcp+tls[openssl] client, sending requests to {}", addr);
        //let topo = Arc::clone(&topo);
        let stop = Arc::clone(&stop);
        //raw_tcp_client(addr, 0, topo, stop, &logger)
        http_benchmarks::openssl_client(addr, stop, throttle, &root)
    });

    info!(logger, "program initialized. press enter key to exit.");
    let mut keys = String::new();
    loop {
        if let Ok(_) = io::stdin().read_line(&mut keys) {
            break
        }
        thread::sleep(Duration::from_millis(100));
    }

    info!(logger, "sending terminate signal to worker threads");
    stop.store(true, Ordering::Relaxed);

    if let Some(client) = raw_tcp_client {
        let n_sent = client.join().unwrap();
        info!(logger, "joined raw tcp client"; "n_sent" => n_sent.thousands_sep());
    }

    if let Some(client) = openssl_client {
        let n_sent = client.join().unwrap();
        info!(logger, "joined raw tcp+tls[openssl] client"; "n_sent" => n_sent.thousands_sep());
    }
}
