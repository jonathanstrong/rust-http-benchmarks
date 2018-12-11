pub mod hist;

pub fn send_strat(n: u16) -> Option<&'static str> {
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
        12 => Some("raw-tcp+tls-via-openssl"),
        _ => None
    }
}

#[cfg(test)]
mod tests {
    #[test]
    fn it_works() {
        assert_eq!(2 + 2, 4);
    }
}
