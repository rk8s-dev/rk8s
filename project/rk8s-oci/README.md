# rk8s-oci

Rust-friendly OCI image primitives for rk8s.

This crate intentionally stays independent from any concrete runtime such as
youki, containerd, or WSLC. It provides small core types that higher-level rk8s
components can reuse:

- image reference parsing and normalization;
- registry mirror rewriting;
- OCI descriptors and digests;
- OCI platform metadata;
- common OCI and Docker media types.

Network registry clients, content stores, archive unpacking, and runtime bundle
generation are left to higher-level crates.
