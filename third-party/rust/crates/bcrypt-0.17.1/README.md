# bcrypt

[![Safety Dance](https://img.shields.io/badge/unsafe-forbidden-success.svg)](https://github.com/rust-secure-code/safety-dance/)
[![Build Status](https://travis-ci.org/Keats/rust-bcrypt.svg)](https://travis-ci.org/Keats/rust-bcrypt)
[![Documentation](https://docs.rs/bcrypt/badge.svg)](https://docs.rs/bcrypt)

## Installation
Add the following to Cargo.toml:

```toml
bcrypt = "0.17"
```

The minimum Rust version is 1.63.0.

## Usage
The crate makes 3 things public: `DEFAULT_COST`, `hash`, `verify`.

```rust
extern crate bcrypt;

use bcrypt::{DEFAULT_COST, hash, verify};

let hashed = hash("hunter2", DEFAULT_COST)?;
let valid = verify("hunter2", &hashed)?;
```

The cost needs to be an integer between 4 and 31 (see benchmarks to have an idea of the speed for each), the `DEFAULT_COST` is 12.

## Error on truncation
Most if not all bcrypt implementation truncates the password after 72 bytes. In specific use cases this can break 2nd pre-image resistance. 
One can enforce the 72-bytes limit on input by using `non_truncating_hash`, `non_truncating_hash_with_result`, `non_truncating_hash_with_salt`, and `non_truncating_verify`. 
The `non_truncating_*` functions behave identically to their truncating counterparts unless the input is longer than 72 bytes, in which case they will return `BcryptError::Truncation`.

If you are generating hashes from other libraries/languages, do not use the `non_truncating_verify` function.

## `no_std`

`bcrypt` crate supports `no_std` platforms. When `alloc` feature is enabled,
all crate functionality is available.  When `alloc` is not enabled only the
raw `bcrypt()` function is usable.

## Benchmarks
Speed depends on the cost used: the highest the slowest.
Here are some benchmarks on a 2019 Macbook Pro to give you some ideas on the cost/speed ratio.
Note that I don't go above 14 as it takes too long.

```
test bench_cost_10      ... bench:  51,474,665 ns/iter (+/- 16,006,581)
test bench_cost_14      ... bench: 839,109,086 ns/iter (+/- 274,507,463)
test bench_cost_4       ... bench:     795,814 ns/iter (+/- 42,838)
test bench_cost_default ... bench: 195,344,338 ns/iter (+/- 8,329,675)
```

## Acknowledgments
This [gist](https://gist.github.com/rgdmarshall/ae3dc072445ed88b357a) for the hash splitting and the null termination.

## Recommendations
While bcrypt works well as an algorithm, using something like [Argon2](https://en.wikipedia.org/wiki/Argon2) is recommended
for new projects.

## Changelog

* 0.17.1: implements `Display` for HashParts instead of `ToString`
* 0.17.0: update getrandom and remove `js` featuree
* 0.16.0: add `non_truncating_*` functions
* 0.15.1: update base64 dependency
* 0.15.0: add an `alloc` feature that can be disabled.
* 0.14.0: use `subtle` crate for constant time comparison, update base64 and bump to 2021 edition
* 0.13.0: make zeroize dep opt-out and use fixed salt length
* 0.12.1: zero vec containing password in the hashing function before returning the hash
* 0.12.0: allow null bytes in password
* 0.11.0: update deps causing big bump in MSRV
* 0.10.1: fix panic with invalid hashes and allow `2x`
* 0.10.0: update blowfish to 0.8 and minimum Rust version to 1.43.0.
* 0.9.0: update base64 to 0.13 and getrandom to 0.2
* 0.8.2: fix no-std build
* 0.8.0: constant time verification for hash, remove custom base64 code from repo and add `std` feature
* 0.7.0: add HashParts::from_str and remove Error::description impl, it's deprecated
* 0.6.3: add `hash_with_salt` function and make `Version::format_for_version` public
* 0.6.2: update base64 to 0.12
* 0.6.1: update base64 to 0.11
* 0.6.0: allow users to choose the bcrypt version and default to 2b instead of 2y
* 0.5.0: expose the inner `bcrypt` function + edition 2018
* 0.4.0: make DEFAULT_COST const instead of static
* 0.3.0: forbid NULL bytes in passwords & update dependencies
* 0.2.2: update rand
* 0.2.1: update rand
* 0.2.0: replace rust-crypto with blowfish, use some more modern Rust things like `?` and handle more errors
* 0.1.6: update rand and base64 deps
* 0.1.5: update lazy-static to 1.0
* 0.1.4: Replace rustc-serialize dependency with bcrypt
* 0.1.3: Fix panic when password > 72 chars
* 0.1.1: make BcryptResult, BcryptError public and update dependencies
* 0.1.0: initial release
