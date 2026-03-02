# aardvark-dns

Aardvark-dns is an authoritative dns server for `A/AAAA` container records. It can forward other requests
to configured resolvers.

Read more about configuration in `src/backend/mod.rs`. It is mostly intended to be used with
[Netavark](https://github.com/containers/netavark/) which will launch it automatically if both are
installed.

```console
aardvark-dns 0.1.0

USAGE:
    aardvark-dns [OPTIONS] <SUBCOMMAND>

FLAGS:
    -h, --help       Print help information
    -V, --version    Print version information

OPTIONS:
    -c, --config <CONFIG>    Path to configuration directory
    -p, --port <PORT>        Host port for aardvark servers, defaults to 5533

SUBCOMMANDS:
    help    Print this message or the help of the given subcommand(s)
    run     Runs the aardvark dns server with the specified configuration directory
```

### MSRV (Minimum Supported Rust Version)

v1.86

We test that Netavark can be build on this Rust version and on some newer versions.
All newer versions should also build, and if they do not, the issue should be
reported and will be fixed. Older versions are not guaranteed to build and issues
will not be fixed.

### Build

```console
make
```

### Run Example

```console
RUST_LOG=trace ./bin/aardvark-dns --config src/test/config/podman/ --port 5533 run
```

### [Configuration file format](./config.md)

### [Contributing](./CONTRIBUTING.md)
