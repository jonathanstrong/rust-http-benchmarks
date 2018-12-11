# rust http benchmarks

Command line utilities to benchmark various ways of sending http requests in rust, with a focus on latency. Wip.

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
client 0.1.0

USAGE:
    client [OPTIONS] --raw-tcp-client <raw-tcp-client>

FLAGS:
    -h, --help       Prints help information
    -V, --version    Prints version information

OPTIONS:
    -R, --raw-tcp-client <raw-tcp-client>    Launch a raw tcp client (no TLS, use stunnel bridge if TLS desired),
                                             sending requests to <addr>
    -t, --throttle <throttle>                sleep <n> milliseconds between requests
```

## example

### Server/client on same machine:

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



