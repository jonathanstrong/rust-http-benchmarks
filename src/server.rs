#![allow(unused)] // during dev - remove later

#[macro_use]
extern crate clap;
#[macro_use]
extern crate slog;
#[macro_use]
extern crate jetscii;

use futures::future;
use tokio_minihttp::{Request, Response, Http};
use tokio_proto::TcpServer;
use tokio_service::Service;

use std::thread;
use std::mem;
use std::sync::{Arc, Mutex};
use std::sync::atomic::{AtomicBool, Ordering};
use std::net::{TcpListener, TcpStream, Shutdown};
use std::net::{SocketAddr, ToSocketAddrs};
use std::os::unix::io::{AsRawFd, FromRawFd};
use std::time::{Instant, Duration};
use std::str;
use std::io::{self, Read, Write};
use std::collections::HashMap;
use slog::{Drain, Logger};
use chrono::{DateTime, Utc};

use http_benchmarks::{send_strat, hist::HistLog};

#[derive(Clone)]
struct TokioServer {
    by_strat: Arc<Mutex<HashMap<&'static str, HistLog>>>,
    logger: Logger,
}

impl Service for TokioServer {
    type Request = Request;
    type Response = Response;
    type Error = io::Error;
    type Future = future::Ok<Response, io::Error>;

    fn call(&self, req: Request) -> Self::Future {
        let loop_time = Instant::now();
        let utc = Utc::now();
        let time = logging::inanos(utc);

        trace!(self.logger, "new {} request to {}", req.method(), req.path());

        req.body().or_else(|| {
            error!(self.logger, "no body found"; "slice" => String::from_utf8(req.data().to_vec()).unwrap());
            None
        }).map(|body| {
            bytes!(b' ').find(body).or_else(|| {
                error!(self.logger, "no space found in body";
                       "slice" => String::from_utf8(body[..].to_vec()).unwrap(),
                       "data" => String::from_utf8(req.data().to_vec()).unwrap());
                None
            }).map(|i| {
                // try to parse the code as u16
                atoi::atoi::<u16>(&body[..i]).or_else(|| {
                    error!(self.logger, "failed to parse strat code"; "slice" => String::from_utf8(body[..i].to_vec()).unwrap());
                    None
                }).and_then(|n| {
                   send_strat(n)
                       .or_else(|| {
                           error!(self.logger, "strat not found"; "n" => n);
                           None
                       })
                }).map(|key| {
                    let mut map = self.by_strat.lock().unwrap();

                    if !map.contains_key(&key) {
                        info!(self.logger, "inserting new key"; "key" => key);
                        let hist = map.get("master").unwrap().clone_with_tag(key);
                        map.insert(key, hist);
                    }

                    let hist = map.get_mut(key).unwrap();

                    atoi::atoi::<i64>(&body[(i+1)..]).or_else(|| {
                        error!(self.logger, "failed to parse timestamp"; "slice" => String::from_utf8(body[(i+1)..].to_vec()).unwrap());
                        None
                    }).map(|sent| {
                        hist.record((time - sent).max(0) as u64);
                        debug!(self.logger, "successfully recorded request"; "sent" => sent, "nanos" => (time - sent).max(0) as u64);
                    });

                    hist.check_send(loop_time);
                });
            });

        });
        let mut resp = Response::new();
        resp.status_code(204, "No Content")
            .body("");
        trace!(self.logger, "sending (future) 204 resp");
        future::ok(resp)
    }
}


fn tokio_server<A: ToSocketAddrs>(
    addr: A,
    hist_dir: &str,
    logger: &Logger,
) -> thread::JoinHandle<()> {
    let hist = HistLog::with_path(hist_dir, "tokio_server", "master", Duration::from_secs(1));
    let mut by_strat: HashMap<&'static str, HistLog> = Default::default();
    by_strat.insert("master", hist);
    let by_strat = Arc::new(Mutex::new(by_strat));
    let addr: SocketAddr = addr.to_socket_addrs().unwrap().next().unwrap();
    let logger = logger.new(o!("thread" => "tokio-server"));

    thread::spawn(move || {
        info!(logger, "spawning TcpServer thread");
        TcpServer::new(Http, addr)
            .serve(move || {
                let server = TokioServer { by_strat: by_strat.clone(), logger: logger.clone() };
                Ok(server)
            });
    })
}

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
        tokio_server(addr, hist_dir, &root)
    });

    info!(logger, "program initialized. press ctrl-c to exit.");

    loop {
        thread::sleep(Duration::from_millis(1));
    }
}

