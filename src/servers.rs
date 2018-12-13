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
use histlog::HistLog;

use crate::client_tag;

#[derive(Clone)]
struct TokioServer {
    by_client: Arc<Mutex<HashMap<&'static str, HistLog>>>,
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
        let time = crate::nanos(utc) as i64;

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
                   client_tag(n)
                       .or_else(|| {
                           error!(self.logger, "strat not found"; "n" => n);
                           None
                       })
                }).map(|key| {
                    let mut map = self.by_client.lock().unwrap();

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


pub fn tokio_server<A: ToSocketAddrs>(
    addr: A,
    hist_dir: &str,
    interval: Duration,
    logger: &Logger,
) -> thread::JoinHandle<()> {
    let hist = HistLog::new(hist_dir, "tokio_server", "master", interval).unwrap();
    let mut by_client: HashMap<&'static str, HistLog> = Default::default();
    by_client.insert("master", hist);
    let by_client = Arc::new(Mutex::new(by_client));
    let addr: SocketAddr = addr.to_socket_addrs().unwrap().next().unwrap();
    let logger = logger.new(o!("thread" => "tokio-server"));

    thread::spawn(move || {
        info!(logger, "spawning TcpServer thread");
        TcpServer::new(Http, addr)
            .serve(move || {
                let server = TokioServer { by_client: by_client.clone(), logger: logger.clone() };
                Ok(server)
            });
    })
}
