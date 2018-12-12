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
use openssl::ssl::{SslMethod, SslConnector, HandshakeError};

pub fn raw_tcp_client<A: ToSocketAddrs>(
    addr: A,
    //core: usize,
    //topo: Arc<Mutex<hwloc::Topology>>,
    stop: Arc<AtomicBool>,
    throttle: Option<Duration>,
    logger: &Logger,
) -> thread::JoinHandle<usize> {
    let addr: SocketAddr = addr.to_socket_addrs().unwrap().next().unwrap();
    let logger = logger.new(o!("thread" => "raw tcp", "addr" => format!("{}", addr)));//, "core" => core));
    thread::spawn(move || {
        //#[cfg(feature = "affinity")]
        //bind_thread(topo, core, &logger);
        let start = Instant::now();
        let client_code = 11;
        let mut loop_time: Instant;
        let mut length: usize = 89;
        let mut n_sent = 0;
        let mut n_sent_stream = 0;
        let mut snd = [0u8; 512];
        let mut rcv = [0u8; 512];
        let headers = format!(
            "POST /{path}/ HTTP/1.1\r\n\
             Host: {host}\r\n\
             User-Agent: rust-http-benchmarks-client/v{version}\r\n\
             Connection: keep-alive\r\n\
             Content-Length: 22\r\n\r\n\
             {client_code} ",
             path = crate::REQUEST_PATH,
             host = addr,
             version = crate_version!(),
             client_code = client_code);
        let n = headers.as_bytes().len();
        info!(logger, "assembled request headers"; "headers" => format!("\n{}\n", headers), "n" => n, "ip" => %addr.ip());
        //snd[..n].copy_from_slice(&b"POST / HTTP/1.1\r\nHost: bench.mmcxi.com\r\nConnection: keep-alive\r\nContent-Length: 22\r\n\r\n11 "[..]);
        snd[..n].copy_from_slice(headers.as_bytes());
        assert!(jetscii::ByteSubstring::new(b"\r\n\r\n").find(&snd[..n]).is_some());

        'a: while !stop.load(Ordering::Relaxed) {
            loop_time = Instant::now();
            //snd[(length-2)..length].copy_from_slice(&b"\r\n"[..]);
            TcpStream::connect(&addr).map_err(|e| {
                error!(logger, "failed to connect: {:?}", e);
                warn!(logger, "sleeping 1s on connection error before retry");
                thread::sleep(Duration::from_secs(1));
                e
            }).map(|mut stream| {
                stream.set_nonblocking(true).expect("send nonblocking");
                stream.set_nodelay(true).expect("send nodelay");
                trace!(logger, "stream: connected");
                'b: while !stop.load(Ordering::Relaxed) {
                    length = n + 4 + itoa::write(&mut snd[n..], crate::nanos(Utc::now())).unwrap();
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

                    if n_sent % crate::HEARTBEAT_EVERY == 0 {
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

pub fn openssl_client(
    addr: http::Uri,
    // core: usize,
    // topo: Arc<Mutex<hwloc::Topology>>,
    stop: Arc<AtomicBool>,
    throttle: Option<Duration>,
    logger: &Logger,
) -> thread::JoinHandle<usize> {
    let logger = logger.new(o!(
        "thread" => "client[raw tcp+tls[openssl]]",
        "addr" => addr.to_string(),
        "host" => addr.host().unwrap().to_string(),
    ));
    thread::spawn(move || {
        // #[cfg(feature = "affinity")]
        // bind_thread(topo, core, &logger);
        let start = Instant::now();
        let client_code = 12;
        let mut loop_time: Instant;
        let mut length: usize = 89;
        let mut n_sent = 0;
        let mut n_sent_stream = 0;
        let mut snd = [0u8; 512];
        let mut rcv = [0u8; 512];

        let headers = format!(
            "POST /{path}/ HTTP/1.1\r\n\
             Host: {host}\r\n\
             User-Agent: rust-http-benchmarks-client/v{version}\r\n\
             Connection: keep-alive\r\n\
             Content-Length: 22\r\n\r\n\
             {client_code} ",
             path = crate::REQUEST_PATH,
             host = addr.host().unwrap(),
             version = crate_version!(),
             client_code = client_code);
        let n = headers.as_bytes().len();
        info!(logger, "assembled request headers"; "headers" => format!("\n{}\n", headers), "n" => n);
        snd[..n].copy_from_slice(headers.as_bytes());
        assert!(jetscii::ByteSubstring::new(b"\r\n\r\n").find(&snd[..n]).is_some());

        let connector = SslConnector::builder(SslMethod::tls())
            .map_err(|e| {
                error!(logger, "failed to build SslConnector: {:?}", e);
            }).expect("SslConnector::builder(SslMethod::tls())").build();

        'a: while !stop.load(Ordering::Relaxed) {
            loop_time = Instant::now();
            //snd[(length-2)..length].copy_from_slice(&b"\r\n"[..]);
            let conn: &str = addr.authority_part().unwrap().as_str();
            TcpStream::connect(conn).map_err(|e| {
                error!(logger, "failed to connect: {:?}", e; "addr" => %conn, "uri" => %addr);
                warn!(logger, "waiting 1s until retry on failed connection attempt");
                thread::sleep(Duration::from_secs(1));
            }).and_then(|mut stream| {
                stream.set_nonblocking(true).expect("send nonblocking");
                stream.set_nodelay(true).expect("send nodelay");

                trace!(logger, "stream: connected, initializing tls...");

                match connector.connect("bench.mmcxi.com", stream) {
                    Ok(stream) => Ok(stream),

                    Err(HandshakeError::WouldBlock(mut handshake)) => {
                        loop {
                            handshake = match handshake.handshake() {
                                Ok(stream) => return Ok(stream),
                                Err(HandshakeError::WouldBlock(handshake)) => handshake,
                                Err(e) => {
                                    error!(logger, "error calling connector.connect: {:?}", e);
                                    return Err(())
                                }
                            };
                        }
                    }

                    Err(e) => {
                        error!(logger, "error calling connector.connect: {:?}", e);
                        Err(())
                    }
                }
            }).map(|mut stream| {
                'b: while !stop.load(Ordering::Relaxed) {
                    length = n + 4 + itoa::write(&mut snd[n..], crate::nanos(Utc::now())).unwrap();
                    snd[(length-4)..length].copy_from_slice(&b"\r\n\r\n"[..]);
                    trace!(logger, "sending request:\n{}", unsafe { str::from_utf8_unchecked(&snd[..length]) });
                    let mut bytes_sent = 0;
                    'c: loop {
                        match stream.write(&snd[bytes_sent..length]) {
                            Ok(n) => {
                                bytes_sent += n;
                            }

                            Err(e) => {
                                trace!(logger, "stream.write err: {:?}", e);
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

                        if stop.load(Ordering::Relaxed) { break 'b }

                        //if &rcv[..bytes_rcvd] == HTTP_204 { break 'c }
                        if let Some(i) = jetscii::ByteSubstring::new(b"\r\n\r\n").find(&rcv[..bytes_rcvd]) {
                            trace!(logger, "rcvd resp:\n {}", unsafe { str::from_utf8_unchecked(&rcv[..i]) });
                            n_sent += 1;
                            break 'd
                        }
                    }

                    if jetscii::ByteSubstring::new(b"keep-alive").find(&rcv[..bytes_rcvd]).is_none() { return () }

                    //thread::sleep(Duration::from_millis(1));

                    //if n_sent_stream > 75 { return () }

                    #[cfg(any(feature = "trace", feature = "debug"))]
                    thread::sleep(Duration::from_secs(1));
                }
            }).ok();
        }
        n_sent
    })
}


