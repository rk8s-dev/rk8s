[![Latest Version]][crates.io] [![Documentation]][docs.rs] [![GHA Status]][GitHub Actions] ![License]

fork from https://github.com/rust-lang/impl-trait-utils

`trait_variant` is a good lib, but it still doesn't support default method, so fork it and create a new lib `trait-make`
to support default method

users can replace `trait_variant` with a few change

## `trait_make`

`trait_make` generates a specialized version of a base trait that uses `async fn` and/or `-> impl Trait`.

For example, if you want a [`Send`][rust-std-send]able version of your trait, you'd write:

```rust
#[trait_make::make(IntFactory: Send)]
trait LocalIntFactory {
    async fn make(&self) -> i32;
    fn stream(&self) -> impl Iterator<Item = i32>;
    fn call(&self) -> u32;
}
```

The `trait_make::make` would generate an additional trait called `IntFactory`:

```rust
use core::future::Future;

trait IntFactory: Send {
   fn make(&self) -> impl Future<Output = i32> + Send;
   fn stream(&self) -> impl Iterator<Item = i32> + Send;
   fn call(&self) -> u32;
}
```

Implementers can choose to implement either `LocalIntFactory` or `IntFactory` as appropriate.

For more details, see the docs for [`trait_make::make`].

[`trait_make::make`]: https://docs.rs/trait-make/latest/trait_make/attr.make.html

#### License and usage notes

Licensed under either of [Apache License, Version 2.0](LICENSE-APACHE) or
[MIT license](LICENSE-MIT) at your option.

[GitHub Actions]: https://github.com/Sherlock-Holo/impl-trait-utils/actions

[GHA Status]: https://github.com/Sherlock-Holo/impl-trait-utils/actions/workflows/rust.yml/badge.svg

[crates.io]: https://crates.io/crates/trait-make

[Latest Version]: https://img.shields.io/crates/v/trait-make.svg

[Documentation]: https://img.shields.io/docsrs/trait-make

[docs.rs]: https://docs.rs/trait-make

[License]: https://img.shields.io/crates/l/trait-make.svg
[rust-std-send]: https://doc.rust-lang.org/std/marker/trait.Send.html
