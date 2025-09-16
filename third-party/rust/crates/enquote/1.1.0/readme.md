# enquote [![crate](https://img.shields.io/crates/v/enquote.svg)](https://crates.io/crates/enquote) [![docs](https://docs.rs/enquote/badge.svg)](https://docs.rs/enquote)
This Rust library quotes, unquotes, and unescapes strings.

## Example
```rust
extern crate enquote;

fn main() {
    assert_eq!(enquote::enquote('\'', "foo'bar"), "'foo\\'bar'");
    assert_eq!(enquote::unquote("'foo\\'bar\\n'").unwrap(), "foo'bar\n");
    assert_eq!(enquote::unescape("\\n", None).unwrap(), "\n");
}
```
