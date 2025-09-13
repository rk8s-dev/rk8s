[![crates.io](https://img.shields.io/crates/v/http-auth)](https://crates.io/crates/http-auth)
[![Released API docs](https://docs.rs/http-auth/badge.svg)](https://docs.rs/http-auth/)
[![CI](https://github.com/scottlamb/http-auth/workflows/CI/badge.svg)](https://github.com/scottlamb/http-auth/actions?query=workflow%3ACI)

Rust library for HTTP authentication. Parses challenge lists, responds
to `Basic` and `Digest` challenges. Likely to be extended with server
support and additional auth schemes.

HTTP authentication is described in the following documents and specifications:

*   [MDN documentation](https://developer.mozilla.org/en-US/docs/Web/HTTP/Authentication).
*   [RFC 7235](https://datatracker.ietf.org/doc/html/rfc7235):
    Hypertext Transfer Protocol (HTTP/1.1): Authentication.
*   [RFC 7617](https://datatracker.ietf.org/doc/html/rfc7617):
    The 'Basic' HTTP Authentication Scheme
*   [RFC 7616](https://datatracker.ietf.org/doc/html/rfc7616):
    HTTP Digest Access Authentication

This framework is primarily used with HTTP, as suggested by the name. It is
also used by some other protocols such as RTSP.

## Status

Well-tested, suitable for production. The API may change to improve ergonomics
and functionality. New functionality is likely to be added. PRs welcome!

## Goals

In order:

1.  **sound.** Currently no `unsafe` blocks in `http-auth` itself. All
    dependencies are common, trusted crates.
2.  **correct.** Precisely implements the specifications except where noted.
    Fuzz tests verify the hand-written parser never panics and matches a
    nom-based reference implementation.
3.  **light-weight.** Minimal dependencies; uses Cargo features so callers can
    avoid them when undesired. Simple code that minimizes monomorphization
    bloat. Small data structures; eg `http_auth::DigestClient` currently weighs
    in at 32 bytes plus one allocation for all string fields.
4.  **complete.** Implements both parsing and responding to challenges.
    (Currently only supports the client side and responding to the most common
    `Basic` and `Digest` schemes; future expansion is likely.)
5.  **ergonomic.** Creating a client for responding to a password challenge is
    a one-liner from a string header or a
    [`http::header::GetAll`](https://docs.rs/http/0.2.5/http/header/struct.GetAll.html).
6.  **fast enough.** HTTP authentication is a small part of a real program, and
    `http-auth`'s CPU usage should never be noticeable. For `Digest`'s
    cryptographic operations, it uses popular optimized crates. In other
    respects, `http-auth` is likely at least as efficient as other HTTP
    authentication crates, although I have no reason to believe their
    performance is problematic.

## Author

Scott Lamb &lt;slamb@slamb.org>

## License

SPDX-License-Identifier: [MIT](https://spdx.org/licenses/MIT.html) OR [Apache-2.0](https://spdx.org/licenses/Apache-2.0.html)

See [LICENSE-MIT.txt](LICENSE-MIT.txt) or [LICENSE-APACHE](LICENSE-APACHE.txt),
respectively.
