#[macro_use]
extern crate clap;
#[macro_use]
extern crate slog;
#[macro_use]
extern crate jetscii;

use chrono::{DateTime, Utc};

pub mod servers;
pub mod clients;

pub(crate) const HEARTBEAT_EVERY: usize = 1_000;
pub(crate) const REQUEST_PATH: &str = "rust-http-benchmarks";

/// Maps integer codes to `&'static str` descriptions.
pub fn client_tag(n: u16) -> Option<&'static str> {
    match n {
        0 => Some("test"),
        1 => Some("loop_rw"),
        2 => Some("hyper-tls"),
        3 => Some("chttp-openssl-none"),
        4 => Some("chttp-wolfssl-none"),
        5 => Some("chttp-wolfssl-DES-CBC3-SHA"),
        6 => Some("chttp-wolfssl-AES128-SHA"),
        7 => Some("chttp-wolfssl-AES256-SHA"),
        8 => Some("chttp-wolfssl-ECDHE-RSA-AES128-SHA"),
        9 => Some("chttp-wolfssl-ECDHE-RSA-AES128-SHA"),
        10 => Some("hyper-http-via-stunnel"),
        11 => Some("raw-tcp"),
        12 => Some("raw tcp+tls[openssl]"),
        _ => None
    }
}

#[inline]
pub fn nanos(t: DateTime<Utc>) -> u64 {
    (t.timestamp() as u64) * 1_000_000_000_u64 + (t.timestamp_subsec_nanos() as u64)
}

#[cfg(test)]
mod tests {
    #[test]
    fn it_works() {
        assert_eq!(2 + 2, 4);
    }
}
