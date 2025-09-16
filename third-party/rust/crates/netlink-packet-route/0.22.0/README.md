# Rust crate for Netlink Route Protocol

The `netlink-packet-route` crate is designed to abstract Netlink route
protocol([`rtnetlink`][rtnetlink_man]) packet into Rust data types. The goal of
this crate is saving netlink user from reading Kernel Netlink codes.

This crate grouped Netlink route protocol into these modules:
 * `link`: NIC interface, similar to to `ip link` command.
 * `address`: IP address, similar to `ip address` command.
 * `route`: Route, similar to `ip route` command.
 * `rule`: Route rule, similar to `ip rule` command.
 * `tc`: Traffic control, similar to `tc` command.
 * `neighbour`: Neighbour, similar to `ip neighbour` command.
 * `neighbour_table`: Neighbour table, similar to `ip ntable` command.
 * `nsid`: Namespace, similar to `ip netns` command.

Normally, you should use [`rtnetlink`][rtnetlink_url] instead of using this
crate directly.

## Development
 * Please use `git commit --signoff` to append Signed-off-by trailer to commit
   message.

 * For new source file, please append `// SPDX-License-Identifier: MIT` at
   the beginning with content of license new code to MIT license.

 * No panic is allowed, please use `Result<>` instead of `unwrap()` or
   `expect()`.

 * Please run `cargo fmt` and `cargo clippy` before creating pull request.

 * All struct/enum should be decorated by
   [`#[non_exhaustive]`][non_exhaustive_doc] to preserve
   API backwards compatibility unless explicit approval from maintainer.

 * Unknown netlink attribute should be stored into `Other(DefaultNla)` instead
   of breaking decoding.

 * Please use unit test case to cover the serialize and deserialize of the
   changed netlink attribute. To capture the netlink raw bytes, you may use
   tcpdump/wireshark again the `nlmon` interface. For example:

```bash
modprobe nlmon
ip link add nl0 type nlmon
ip link set nl0 up
tcpdump -i nl0 -w netlink_capture_file.cap
# Then use wireshark to open this `netlink_capture_file.cap`
# Find out the packet you are interested,
# right click -> "Copy" -> "...as Hex Dump".
# You may use https://github.com/cathay4t/hex_to_rust to convert this
# hexdump to rust u8 array
```
 * The integration test(play with linux kernel netlink interface) should be
   placed into `rtnetlink` crate. Current(netlink-packet-route) crate should
   only unit test case only.


 * For certain netlink message which cannot captured by nlmon, please use
   Rust Debug and explain every bits in comment.

 * Optionally, please run `tools/test_cross_build.sh` to make sure your
   code is compilable on other platforms.

[rtnetlink_man]: https://www.man7.org/linux/man-pages/man7/rtnetlink.7.html
[rtnetlink_url]: https://docs.rs/rtnetlink
[non_exhaustive_doc]: https://doc.rust-lang.org/stable/reference/attributes/type_system.html#the-non_exhaustive-attribute
