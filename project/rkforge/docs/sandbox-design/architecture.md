# Sandbox Architecture

## Goal

`rkforge sandbox` provides an execution environment built around a microVM lifecycle:

```text
create -> boot -> ready -> exec -> stop -> remove
```

The current primary backend is `libkrun`.

## Layers

The sandbox stack is intentionally split into layers:

```text
CLI / SDK / future language bindings
        ↓
stable host-side sandbox API
        ↓
runtime orchestration
        ↓
VMM backend / guest protocol / helper binaries
        ↓
runtime bundle + prebuilt runtime artifacts
```

## Host-Side Runtime

Core host-side types:

- `SandboxRuntime`
- `SandboxBox`
- `MicroVmSandboxBackend`
- `LibkrunVmBackend`

Responsibilities:

- persist sandbox metadata
- build VM instance spec
- boot the VM
- wait for guest ready
- dispatch exec requests
- track sandbox state transitions

Relevant code:

- [src/sandbox/mod.rs](../../src/sandbox/mod.rs)
- [src/sandbox/vm/mod.rs](../../src/sandbox/vm/mod.rs)
- [src/sandbox/vm/libkrun.rs](../../src/sandbox/vm/libkrun.rs)

## Guest-Side Components

Guest-side pieces currently include:

- sandbox guest agent
- guest init helper
- vsock ready path
- vsock exec transport

Relevant code:

- [src/sandbox/agent.rs](../../src/sandbox/agent.rs)
- [src/sandbox/guest.rs](../../src/sandbox/guest.rs)
- [src/sandbox/protocol.rs](../../src/sandbox/protocol.rs)

## Helper Binary Direction

The sandbox no longer treats helper roles as only hidden subcommands.

Current binary targets:

- `rkforge`
- `rkforge-sandbox-shim`
- `rkforge-sandbox-agent`
- `rkforge-sandbox-guest-init`

This improves long-term SDK and multi-language integration because helper roles are now
first-class runtime assets instead of being only an implementation detail of the main binary.

## Current Runtime Contract

Runtime bundle layout:

```text
runtime/
  bin/
    rkforge
    rkforge-sandbox-shim
    rkforge-sandbox-agent
    rkforge-sandbox-guest-init
  lib/
    libkrun.so*
    libkrunfw.so*
```

This runtime bundle is consumed by:

- VM helper process launching
- guest image preparation
- runtime asset discovery
- embedded runtime extraction

## Current State

What is already in place:

- libkrun end-to-end MVP validation
- internal guest image creation and caching
- runtime asset bundle abstraction
- embedded runtime support
- prebuilt runtime contract
- Rust SDK facade

What remains after the current code state:

- harden prebuilt runtime CI/release flow
- validate clean-machine install path
- stabilize language-neutral host-side contract for Python / TS / Go SDKs
