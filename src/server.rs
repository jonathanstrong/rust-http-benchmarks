#![allow(unused)] // during dev - remove later

#[macro_use]
extern crate clap;
#[macro_use]
extern crate slog;

use std::thread;
use std::time::*;
use slog::Drain;

fn main() {
    let args: clap::ArgMatches = clap::App::new("server")
        .version(crate_version!())
        .arg(clap::Arg::with_name("hist-dir")
             .long("hist-dir")
             .short("d")
             .help("direcotry to save histogram log files in")
             .takes_value(true)
             .default_value("var/hist/")
             .required(true))
        .arg(clap::Arg::with_name("tokio-server")
             .long("tokio-server")
             .short("m")
             .help("launch the tokio minihttp server, listening on <addr>")
             .takes_value(true)
             .required(true)) // until other server types implemented
        .get_matches();

    let hist_dir = args.value_of("hist-dir").unwrap();

    let decorator = slog_term::TermDecorator::new().stdout().force_color().build();
    let drain = slog_term::CompactFormat::new(decorator).use_utc_timestamp().build().fuse();
    let drain = slog_async::Async::new(drain).chan_size(8192).thread_name("recv".into()).build().fuse();
    let root = slog::Logger::root(drain, o!());
    let logger = root.new(o!("thread" => "main"));

    let tokio_server = args.value_of("tokio-server").map(|addr| {
        info!(logger, "launching tokio minihttp server, listening at {}", addr);
        http_benchmarks::servers::tokio_server(addr, hist_dir, &root)
    });

    info!(logger, "program initialized. press ctrl-c to exit.");

    loop {
        thread::sleep(Duration::from_millis(1));
    }
}

