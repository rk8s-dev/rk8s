# context-rs

[![Build Status](https://travis-ci.org/zonyitoo/context-rs.svg?branch=master)](https://travis-ci.org/zonyitoo/context-rs)
[![Build status](https://ci.appveyor.com/api/projects/status/ce622ulw4pil7vle?svg=true)](https://ci.appveyor.com/project/zonyitoo/context-rs)
[![License](https://img.shields.io/crates/l/context.svg)](https://github.com/zonyitoo/context-rs)

This project offers an easy interface to the famous
[Boost.Context](http://www.boost.org/doc/libs/1_60_0/libs/context/doc/html/context/overview.html)
library and thus provides _the building blocks for higher-level abstractions,
like coroutines, cooperative threads (userland threads) or
an equivalent to C# keyword yield in C++._

[**API documentation**](https://crates.fyi/crates/context/1.0.0/)

## Usage

To use `context`, first add this to your `Cargo.toml`:

```toml
[dependencies]
context = "3.0"
```

And then add this to your source files:

```rust
extern crate context;
```

## Performance

The performance heavily depends on the architecture and even on the operating
system. A context switch itself is usually as fast as a regular function call
though and can thus be viewed as one.

To see this for yourself run `cargo bench resume`. You can then compare the
results of the `resume` benchmarks (which uses `Context::resume()`) to the
results of `resume_reference_perf` (which uses regular function calls).

## Platforms

Architecture  | Linux (UNIX)      | Windows    | MacOS X       | iOS
--------------|-------------------|------------|---------------|---------------
i386          | SYSV (ELF)        | MS (PE)    | SYSV (MACH-O) | -
x86_64        | SYSV, X32 (ELF)   | MS (PE)    | SYSV (MACH-O) | -
arm (aarch32) | AAPCS (ELF)       | AAPCS (PE) | -             | AAPCS (MACH-O)
arm (aarch64) | AAPCS (ELF)       | -          | -             | AAPCS (MACH-O)
mips1         | O32 (ELF)         | -          | -             | -
ppc32         | SYSV (ELF), XCOFF | -          | SYSV (MACH-O) | -
ppc64         | SYSV (ELF), XCOFF | -          | SYSV (MACH-O) | -

Format: `ABI (binary format)`.
Source: [Boost.Context](http://www.boost.org/doc/libs/1_60_0/libs/context/doc/html/context/architectures.html)
