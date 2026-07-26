# rk8s-runtime-api

Runtime provider traits and portable request types for rk8s.

This crate is intentionally independent from concrete runtimes such as youki,
containerd, WSLC, or microVM backends. Runtime adapters can implement the
`ContainerRuntime` trait and share the same OCI-oriented request and status
types with higher-level rk8s components.
