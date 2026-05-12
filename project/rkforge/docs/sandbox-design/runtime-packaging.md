# Sandbox Runtime Packaging

## Why Runtime Packaging Exists

The sandbox path depends on more than a single Rust binary.

Required runtime assets include:

- `rkforge`
- `rkforge-sandbox-shim`
- `rkforge-sandbox-agent`
- `rkforge-sandbox-guest-init`
- `libkrun`
- `libkrunfw`

The packaging goal is to hide these details from end users.

## Runtime Modes

Current build-time modes:

- `Source`
- `Prebuilt`
- `Stub`
- `System`

Feature-oriented intent:

- SDK / released builds should prefer `Prebuilt`
- internal development can use `Source`
- `System` is a fallback, not the preferred end-user story

## Prebuilt Runtime Artifact

Artifact name:

```text
rkforge-sandbox-runtime-v{version}-{target}.tar.gz
```

Expected extracted structure:

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

For prebuilt runtime artifacts, helper binaries must be real files, not only symlink aliases.

## Build-Time Resolution Order

Current prebuilt/runtime resolution order:

1. `RKFORGE_RUNTIME_LIB_DIR`
2. `runtime/current/lib`
3. `RKFORGE_RUNTIME_TARBALL`
4. `RKFORGE_RUNTIME_URL`
5. default runtime URL template
6. `runtime/current.tar.gz`
7. `runtime/prebuilt/runtime.tar.gz`
8. vendored source mode
9. system-installed libraries

## Default Download Template

Current default template:

```text
https://download.rk8s.dev/rk8s/releases/v{version}/rkforge-sandbox-runtime-v{version}-{target}.tar.gz
```

This aligns with the existing rk8s binary release base URL convention.

## Local Assembly

Runtime bundle:

```bash
tools/build-sandbox-runtime.sh --output runtime/current
```

Runtime tarball:

```bash
tools/build-sandbox-runtime.sh \
  --output runtime/current \
  --archive runtime/current.tar.gz
```

Formal script entry:

```bash
scripts/build/build-runtime.sh
```

## Release Flow

Current CI direction:

- build release `rkforge`
- build release helper binaries
- build `libkrunfw`
- build `libkrun`
- assemble runtime bundle
- archive runtime
- upload to Cloudflare R2

Relevant workflow:

- [rkforge-sandbox-runtime-release.yml](../../../../.github/workflows/rkforge-sandbox-runtime-release.yml)

## Installer Integration

The root installer [install.sh](../../../../install.sh) now attempts to install
the matching rkforge sandbox runtime on Linux when installing `rkforge`.

Installer behavior:

1. download `rkforge`
2. detect target triple from host platform
3. download matching sandbox runtime tarball
4. extract:
   - `runtime/bin/*` into the chosen install bin directory
   - `runtime/lib/*` into the sibling `lib/` directory

Example:

- install dir: `/usr/local/bin`
- runtime lib dir: `/usr/local/lib`

Opt-out:

```sh
RK8S_INSTALL_SANDBOX_RUNTIME=0
```

## Remaining Work

- validate runtime artifact on a clean host
- finalize installer integration
- ensure released crates/binaries default to prebuilt runtime path for end users
