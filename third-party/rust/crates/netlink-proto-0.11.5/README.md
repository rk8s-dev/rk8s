# Rust async netlink protocol

The `netlink-proto` crate is an asynchronous implementation of the netlink
protocol. It only depends on [`netlink-packet-core`][netlink_core_url] for the
`NetlinkMessage` type and [`netlink-sys`][netlink_sys_url] for the socket.

[netlink_core_url]: https://github.com/rust-netlink/netlink-packet-core
[netlink_sys_url]: https://github.com/rust-netlink/netlink-sys
