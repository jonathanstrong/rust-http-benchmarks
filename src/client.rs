#[macro_use]
extern crate clap;
#[macro_use]
extern crate slog;
#[macro_use]
extern crate jetscii;

use std::thread;
use std::mem;
use std::sync::{Arc, Mutex};
use std::sync::atomic::{AtomicBool, Ordering};
use std::net::{TcpListener, TcpStream, Shutdown};
use std::net::{SocketAddr, ToSocketAddrs};
use std::os::unix::io::{AsRawFd, FromRawFd};
use std::time::{Instant, Duration};
use std::str::{self, FromStr};
use std::io::{self, Read, Write};
use std::collections::HashMap;
use slog::{Drain, Logger};
use chrono::{DateTime, Utc};
use pretty_toa::ThousandsSep;

const HEARTBEAT_EVERY: usize = 1_000;

fn raw_tcp_client<A: ToSocketAddrs>(
    addr: A,
    //core: usize,
    //topo: Arc<Mutex<hwloc::Topology>>,
    stop: Arc<AtomicBool>,
    throttle: Option<Duration>,
    logger: &Logger,
) -> thread::JoinHandle<usize> {
    let addr: SocketAddr = addr.to_socket_addrs().unwrap().next().unwrap();
    let logger = logger.new(o!("thread" => "send", "addr" => format!("{}", addr)));//, "core" => core));
    thread::spawn(move || {
        //#[cfg(feature = "affinity")]
        //bind_thread(topo, core, &logger);
        let start = Instant::now();
        let mut loop_time: Instant;
        let mut length: usize = 89;
        let mut n_sent = 0;
        let mut n_sent_stream = 0;
        let mut snd = [0u8; 512];
        let mut rcv = [0u8; 512];
        let headers = format!(
            "POST /rust-http-benchmarks/ HTTP/1.1\r\n\
             Host: {}\r\n\
             User-Agent: rust-http-benchmarks-client/v{}\r\n\
             Connection: keep-alive\r\n\
             Content-Length: 22\r\n\r\n\
             11 ",
             addr, crate_version!());
        let n = headers.as_bytes().len();
        info!(logger, "assembled request headers"; "headers" => format!("\n{}\n", headers), "n" => n, "ip" => %addr.ip());
        //snd[..n].copy_from_slice(&b"POST / HTTP/1.1\r\nHost: bench.mmcxi.com\r\nConnection: keep-alive\r\nContent-Length: 22\r\n\r\n11 "[..]);
        snd[..n].copy_from_slice(headers.as_bytes());
        assert!(jetscii::ByteSubstring::new(b"\r\n\r\n").find(&snd[..n]).is_some());

        'a: while !stop.load(Ordering::Relaxed) {
            loop_time = Instant::now();
            //snd[(length-2)..length].copy_from_slice(&b"\r\n"[..]);
            TcpStream::connect(&addr).map_err(|e| {
                error!(logger, "failed to connect: {:?}", e; "addr" => %addr);
                warn!(logger, "sleeping 1s on connection error before retry");
                thread::sleep(Duration::from_secs(1));
                e
            }).map(|mut stream| {
                stream.set_nonblocking(true).expect("send nonblocking");
                stream.set_nodelay(true).expect("send nodelay");
                trace!(logger, "stream: connected");
                'b: while !stop.load(Ordering::Relaxed) {
                    length = n + 4 + itoa::write(&mut snd[n..], logging::nanos(Utc::now())).unwrap();
                    snd[(length-4)..length].copy_from_slice(&b"\r\n\r\n"[..]);
                    debug!(logger, "sending request:\n{}", unsafe { str::from_utf8_unchecked(&snd[..length]) });
                    let mut bytes_sent = 0;
                    'c: loop {
                        match stream.write(&snd[bytes_sent..length]) {
                            Ok(n) => {
                                bytes_sent += n;
                            }

                            Err(e) => {
                                trace!(logger, "stream.write err: {:?}", e);

                                #[cfg(any(feature = "trace", feature = "debug"))]
                                thread::sleep(Duration::from_millis(100));
                            }
                        }

                        if bytes_sent >= length { break 'c }

                        if stop.load(Ordering::Relaxed) { break 'b }
                    }

                    n_sent_stream += 1;

                    //stream.shutdown(Shutdown::Write).expect("shutdown write");
                    trace!(logger, "awaiting resp");
                    let mut bytes_rcvd = 0;
                    'd: loop {
                        match stream.read(&mut rcv[bytes_rcvd..]) {
                            Ok(n) => {
                                bytes_rcvd += n;
                                trace!(logger, "{} bytes rcvd: {}", bytes_rcvd, str::from_utf8(&rcv[..bytes_rcvd]).unwrap());
                            }

                            Err(e) => {
                                #[cfg(feature = "trace")]
                                {
                                    if e.kind() != io::ErrorKind::WouldBlock {
                                        trace!(logger, "stream.read err: {:?}", e);
                                    }
                                }
                            }
                        }

                        if stop.load(Ordering::Relaxed) {
                            break 'b
                        }

                        //if &rcv[..bytes_rcvd] == HTTP_204 { break 'c }
                        if let Some(i) = jetscii::ByteSubstring::new(b"\r\n\r\n").find(&rcv[..bytes_rcvd]) {
                            trace!(logger, "rcvd resp:\n {}", unsafe { str::from_utf8_unchecked(&rcv[..i]) });
                            n_sent += 1;
                            break 'd
                        }
                    }

                    if n_sent % HEARTBEAT_EVERY == 0 {
                        info!(logger, "sent {} requests in {:?}", n_sent.thousands_sep(), Instant::now() - start);
                    }

                    if let &Some(throttle) = &throttle {
                        thread::sleep(throttle);
                    }

                    if cfg!(any(feature = "trace", feature = "debug")) {
                        thread::sleep(Duration::from_secs(1));
                    }

                    // if server resp does not include text "keep-alive", return
                    // from closure with active `stream`, triggering the creation of
                    // a new connection on the next iteration of `'a` loop.
                    //
                    // this is a quick and dirty check for the server approving the keep-alive
                    // with a 
                    //
                    if jetscii::ByteSubstring::new(b"keep-alive").find(&rcv[..bytes_rcvd]).is_none() { return () }
                }
            }).ok();

            #[cfg(any(feature = "trace", feature = "debug"))]
            thread::sleep(Duration::from_secs(1));
        }
        n_sent
    })
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
             .required(true)) // temporarily
        .arg(clap::Arg::with_name("throttle")
             .long("throttle")
             .short("t")
             .help("sleep <n> milliseconds between requests")
             .takes_value(true)
             .required(false)) // temporarily
        //.arg(clap::Arg::with_name("openssl-client")
        //     .long("openssl-client")
        //     .short("O")
        //     .help("Launch a raw tcp+tls socket using rust bindings to openssl \
        //           sending requests to <addr>")
        //     .takes_value(true)
        //     .required(false))
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
        raw_tcp_client(addr, stop, throttle, &root)
    });

    info!(logger, "program initialized. press enter key to exit.");
    let mut keys = String::new();
    loop {
        if let Ok(_) = io::stdin().read_line(&mut keys) {
            break
        }
        thread::sleep(Duration::from_millis(100));
    }
}
