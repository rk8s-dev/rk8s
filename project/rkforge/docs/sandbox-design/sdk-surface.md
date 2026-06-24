# Sandbox SDK Surface

## Goal

The sandbox API should be usable from:

- Rust directly
- future Python bindings
- future TypeScript bindings
- future Go bindings

This means the public contract should be host-side, stable, and not tightly bound to
internal wire protocol details.

## Current Rust SDK Facade

Current public Rust-facing types:

- `SandboxClient`
- `SandboxClientBuilder`
- `SandboxHandle`
- `SandboxCreateOptions`
- `SandboxExecOptions`
- `SandboxExecSpec`
- `SandboxExecTarget`
- `SandboxExecResult`
- `SandboxMetadata`

Relevant code:

- [src/sandbox/sdk.rs](/home/erasernoob/project/rk8s/project/rkforge/src/sandbox/sdk.rs)
- [src/sandbox/types.rs](/home/erasernoob/project/rk8s/project/rkforge/src/sandbox/types.rs)

## Why This Layer Exists

Internal types such as:

- `GuestExecRequest`
- `GuestExecResponse`
- `SandboxBackend`
- `VmBackend`

are not a good long-term external SDK contract because they expose:

- transport assumptions
- guest protocol details
- backend implementation details

The facade layer exists to present a smaller, more stable API.

## Current Design Rule

The CLI should be an adapter over the SDK facade, not the source of truth.

Desired layering:

```text
CLI / Python SDK / TS SDK / Go SDK
                ↓
         SDK facade / host-side API
                ↓
    runtime / backend / guest protocol internals
```

## Language-Neutral Request Model

The current API now includes:

- `SandboxExecSpec`
- `SandboxExecTarget`

This is the first step toward a language-neutral request model.

Example:

```rust
let result = sandbox
    .execute(
        rkforge::SandboxExecSpec::python("print('hello')")
            .timeout_secs(30),
    )
    .await?;
```

This is better for future bindings than only exposing:

- `exec_python(...)`
- `exec(command, args, ...)`

because bindings can map a single request object more naturally.

## Next SDK Steps

1. keep moving CLI usage toward `SandboxClient`
2. avoid exposing protocol structs as public SDK types
3. define error mapping suitable for language bindings
4. decide whether future non-Rust SDKs bind Rust directly or go through a separate controller API
