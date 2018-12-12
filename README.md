# rust http benchmarks

Command line utilities to benchmark various ways of sending http requests in rust, with a focus on latency.

The basic gist is: one or more clients send http(s) requests to a server with a timestamp in the request body.
The server records the lag between the current time (on the server) and the client timestamp to an
HdrHistogram interval log, tagging the entries by client-type. Note: synchronizing clocks (with
[chrony](https://chrony.tuxfamily.org/), for instance) is highly recommended to minimize measurement
noise from clock drift.

Work in progress - more clients and server types planned.

## server

build:

```console
$ cargo build --bin server --release
```

use:

```console
$ ./target/release/server -h
server 0.1.0

USAGE:
    server --hist-dir <hist-dir> --tokio-server <tokio-server>

FLAGS:
    -h, --help       Prints help information
    -V, --version    Prints version information

OPTIONS:
    -d, --hist-dir <hist-dir>            direcotry to save histogram log files in [default: var/hist/]
    -m, --tokio-server <tokio-server>    launch the tokio minihttp server, listening on <addr>
```

## client

build

```
$ cargo build --bin client --release
```

use:

```console
$ ./target/release/client -h
client 0.2.2

USAGE:
    client [OPTIONS]

FLAGS:
    -h, --help       Prints help information
    -V, --version    Prints version information

OPTIONS:
    -O, --openssl-client <openssl-client>    Launch a raw tcp+tls socket using rust bindings to openssl sending requests
                                             to <addr>
    -R, --raw-tcp-client <raw-tcp-client>    Launch a raw tcp client (no TLS, use stunnel bridge if TLS desired),
                                             sending requests to <addr>
    -t, --throttle <throttle>                sleep <n> milliseconds between requests
```

## examples

### server/client on same machine:

terminal 1:

```console
$ ./target/release/server --tokio-server 127.0.0.1:34567
```

terminal 2:

```console
$ ./target/release/client --raw-tcp-client 127.0.0.1:34567 --throttle 1
```

[Allow to run for desired period...]

View resulting HdrHistogram log with [log analyzer](https://hdrhistogram.github.io/HdrHistogramJSDemo/logparser.html).

### nginx proxy

#### server:

1. install nginx
2. configure domain/dns/ssl cert. For example:
  - buy my-domain.com
  - create A record for bench.my-domain.com pointing to server ip (or whatever)
  - run [certbot](https://certbot.eff.org/) (`$ sudo certbot --nginx -d bench.my-domain.com`) and follow the prompts to obtain free ssl cert
3. `proxy_pass` chosen port to `http_benchmarks` server in nginx config (`/etc/nginx/sites-enabled/default` is default location on ubuntu 16.04):
  ```
  server {
      listen 443;

      # [...]

      location /rust-http-benchmarks/ {
          proxy_pass 127.0.0.1:34567;
      }

      # [...]
  }
  ```
4. build server: `/path/to/rust-http-benchmarks$ cargo build --bin server --release`
5. run server: `/path/to/rust-http-benchmarks$ ./target/release/server --tokio-server 127.0.0.1:34567`

#### client:

1. build client: `/path/to/rust-http-benchmarks$ cargo build --bin client --release`
2. run client: `/path/to/rust-http-benchmarks$ ./target/release/client --openssl-client bench.my-domain.com`

#### benchmark analysis

1. run benchmark for desired period (could be 10min or 4h+ -- depends on how thorough you want to be).
2. stop client and server programs
3. retrieve histogram interval log files from server:
  ```console
  /path/to/rust-http-benchmarks$ mkdir var/hist/<server-name> -p
  /path/to/rust-http-benchmarks$ rsync -av <user>@<server-ip>:/remote/path/to/rust-http-benchmarks/var/hist/ var/hist/<server-name>/
  ```
4. open logs with [log analyzer](https://hdrhistogram.github.io/HdrHistogramJSDemo/logparser.html) (note: set units to nanoseconds)

