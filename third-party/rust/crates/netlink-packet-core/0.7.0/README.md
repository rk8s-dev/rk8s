# Rust crate for common netlink packet parsing

The `netlink-packet-core` is the glue for all the other netlink-packet-*
crates. It provides a `NetlinkMessage<T>` type that represent any netlink
message for any sub-protocol.
