# SlayerFS Control Plane

## Overview

SlayerFS now exposes a local control plane so CLI commands can talk to a mounted instance instead of rebuilding filesystem state out of process.

The current control plane is:

- local-only
- based on Unix Domain Socket
- instance-scoped
- managed by `MetaClient`

Today it supports:

- `slayerfs info`
- `slayerfs gc --dry-run`

`gc` currently proves the control path and job lifecycle. The real orphan scan/delete executor is still a follow-up step.

## How It Works

When a mount starts, `MetaClient` starts a control-plane server and writes one runtime record for that mounted instance.

That runtime record contains:

- `pid`
- `mount_point`
- `socket_path`
- `started_at`

CLI commands such as `slayerfs info` and `slayerfs gc` first consult the runtime registry to locate the target instance, then connect to its socket and send one JSON request.

The transport model is intentionally simple:

- one connection
- one request
- one response

## Main Components

### `src/control/runtime.rs`

Implements the runtime registry.

Responsibilities:

- choose the runtime root directory
- compute record and socket paths
- persist `InstanceRecord`
- auto-select a single running instance
- reject ambiguous multi-instance selection
- clean stale records when the recorded process is gone

### `src/control/server.rs`

Implements the Unix Domain Socket server.

Responsibilities:

- create parent directories for the socket
- bind the listener
- read one JSON `ControlRequest`
- dispatch to a handler
- write one JSON `ControlResponse`

### `src/control/client.rs`

Implements the CLI-side request helper.

Responsibilities:

- connect to the socket
- serialize one request
- read the full response
- deserialize the response

### `src/control/job.rs`

Implements generic job tracking.

Responsibilities:

- create job ids
- track `Pending/Running/Succeeded/Failed`
- store optional typed outcomes

The job layer is generic. `JobStatus` no longer hardcodes GC-specific counters at the top level. Instead it returns a generic `outcome`, and GC currently uses `JobOutcome::Gc`.

### `src/meta/client/mod.rs`

`MetaClient` owns the instance-scoped runtime services.

Responsibilities:

- start the control plane
- register the running instance
- own the `JobManager`
- handle `Ping`, `GetInfo`, `RunGc`, and `GetJob`
- clean up socket and runtime record on shutdown

## Supported Requests

The current request set is:

- `Ping`
- `GetInfo`
- `RunGc { dry_run }`
- `GetJob { job_id }`

The current response set is:

- `Pong`
- `Info`
- `Accepted`
- `JobStatus`
- `Error`

## CLI Usage

### `slayerfs info`

Print basic information for a mounted instance.

```bash
slayerfs info /path/to/mount
```

Example output:

```text
mount_point: /tmp/slayerfs
pid: 12345
started_at: 2026-03-23T10:16:18.157+00:00
version: 0.1.0
```

If only one SlayerFS instance is running locally, the mount point may be omitted:

```bash
slayerfs info
```

### `slayerfs gc --dry-run`

Submit a GC job through the control plane.

```bash
slayerfs gc --dry-run /path/to/mount
```

Current behavior:

- creates a GC job
- returns job status through the control plane
- reports placeholder GC output

This already validates:

- instance lookup
- socket communication
- async job creation
- job polling

The actual orphan scan/delete implementation is still pending.

## Instance Selection Rules

Commands that target a mounted instance use these rules:

1. If a mount point is given, select that instance.
2. If no mount point is given and exactly one instance exists, auto-select it.
3. If no mount point is given and multiple instances exist, return an error.

## Shutdown Behavior

On shutdown, the mounted instance removes:

- its runtime record
- its socket file

This keeps CLI discovery clean and avoids stale instance entries after normal unmount.

## Current Limitations

- `RunGc` is still a control-path skeleton; it does not yet perform orphan scanning.
- Session startup currently logs a duplicate-session warning in some mount flows. That is separate from the control plane and should be cleaned up later.
- The control plane is local-only and intentionally not designed for remote management.

## Verification

Useful commands:

```bash
cargo test control:: --lib
cargo test meta::client::tests::test_control_plane_ --lib
cargo run --quiet -- info --help
cargo run --quiet -- gc --help
```
