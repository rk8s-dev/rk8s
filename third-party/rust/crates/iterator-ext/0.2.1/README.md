# iterator-ext: An extension to Rust's `Iterator` trait.

## Usage

The crate provides the `IteratorExt` trait extends the capability of those types
that implements `Iterator`. It provides `try_filter()`, `try_flatten()`
and more fallible adaptors that are analogous to those of `Iterator`.

The example demonstrates the usage of the adaptors. It accumulates the values from
0 to 9, and keeps only even outcomes. It raises error when the accumulation exceeds 10.

```rust
use iterator_ext::IteratorExt;
//!
let results: Vec<_> = (0..10)
    .map(Ok)
    .try_scan(0, |acc, val| {
        *acc += val;
        if *acc <= 10 {
            Ok(Some(*acc))
        } else {
            Err("exceed limit")
        }
    })
    .try_filter(|val| Ok(val % 2 == 0))
    .collect();
//!
assert_eq!(results, vec![Ok(0), Ok(6), Ok(10), Err("exceed limit")]);
```

## License

MIT license. See [LICENSE.txt](LICENSE.txt) file.
