//! sha256 crypto digest util
//!
//! ```rust
//!
//! use sha256::{digest, try_digest};
//!
//! //sha256 digest String
//! let input = String::from("hello");
//! let val = digest(input);
//! assert_eq!(val,"2cf24dba5fb0a30e26e83b2ac5b9e29e1b161e5c1fa7425e73043362938b9824");
//!
//! //sha256 digest &str
//! let input = "hello";
//! let val = digest(input);
//! assert_eq!(val,"2cf24dba5fb0a30e26e83b2ac5b9e29e1b161e5c1fa7425e73043362938b9824");
//!
//! //sha256 digest &mut &str
//! let mut input = "hello";
//! let val = digest(&mut input);
//! assert_eq!(val,"2cf24dba5fb0a30e26e83b2ac5b9e29e1b161e5c1fa7425e73043362938b9824");
//!
//! //sha256 digest char
//! let mut input = 'π';
//! let val = digest(input);
//! assert_eq!(val,"2617fcb92baa83a96341de050f07a3186657090881eae6b833f66a035600f35a");
//!
//! //sha256 digest bytes
//! let input = b"hello";
//! let val = digest(input);
//! assert_eq!(val,"2cf24dba5fb0a30e26e83b2ac5b9e29e1b161e5c1fa7425e73043362938b9824");
//!
//! //sha256 digest file
//! use std::path::Path;
//! let input = Path::new("./foo.file");
//! let val = try_digest(input).unwrap();
//! assert_eq!(val,"433855b7d2b96c23a6f60e70c655eb4305e8806b682a9596a200642f947259b1");
//!
//! ```

#[cfg(feature = "async")]
pub mod async_digest;
#[cfg(feature = "native_openssl")]
mod openssl_sha256;

#[cfg(feature = "native_openssl")]
use crate::openssl_sha256::OpenSslSha256;

#[cfg(feature = "async")]
pub use async_digest::*;

use sha2::digest::Output;
use sha2::{Digest, Sha256};
use std::fmt::Debug;
use std::fs;
use std::io;
use std::io::{BufReader, Read};
use std::path::Path;

#[cfg(test)]
mod tests;

/// sha256 digest string
///
/// # Examples
///
/// ```rust
/// use sha256::digest;
/// let input = "hello";
/// let val = digest(input);
/// assert_eq!(val,"2cf24dba5fb0a30e26e83b2ac5b9e29e1b161e5c1fa7425e73043362938b9824")
/// ```
///
pub fn digest<D: Sha256Digest>(input: D) -> String {
    input.digest()
}

/// sha256 digest file
///
/// # Examples
///
/// ```rust
/// use sha256::try_digest;
/// use std::path::Path;
/// let input = Path::new("./foo.file");
/// let val = try_digest(input).unwrap();
/// assert_eq!(val,"433855b7d2b96c23a6f60e70c655eb4305e8806b682a9596a200642f947259b1")
/// ```
pub fn try_digest<D: TrySha256Digest>(input: D) -> Result<String, D::Error> {
    input.digest()
}

/// sha256 digest bytes
///
/// # Examples
///
/// ```rust
/// use sha256::digest_bytes;
/// let input = b"hello";
/// let val = digest_bytes(input);
/// assert_eq!(val,"2cf24dba5fb0a30e26e83b2ac5b9e29e1b161e5c1fa7425e73043362938b9824")
/// ```
///
#[deprecated(since = "1.1.0", note = "Use new function `digest()` instead")]
pub fn digest_bytes(input: &[u8]) -> String {
    __digest__(input)
}

/// sha256 digest file
///
/// # Examples
///
/// ```rust
/// use sha256::digest_file;
/// use std::path::Path;
/// let input = Path::new("./foo.file");
/// let val = digest_file(input).unwrap();
/// assert_eq!(val,"433855b7d2b96c23a6f60e70c655eb4305e8806b682a9596a200642f947259b1")
/// ```
#[deprecated(since = "1.1.0", note = "Use new function `try_digest()` instead")]
pub fn digest_file<P: AsRef<Path>>(path: P) -> Result<String, io::Error> {
    let bytes = fs::read(path)?;
    Ok(__digest__(&bytes))
}

pub trait Sha256Digest {
    fn digest(self) -> String;
}

#[async_trait::async_trait]
pub trait TrySha256Digest {
    type Error: Debug;

    fn digest(self) -> Result<String, Self::Error>;

    #[cfg(feature = "async")]
    async fn async_digest(self) -> Result<String, Self::Error>;

    #[cfg(feature = "native_openssl")]
    async fn async_openssl_digest(self) -> Result<String, Self::Error>;
}

impl<const N: usize> Sha256Digest for &[u8; N] {
    fn digest(self) -> String {
        __digest__(self)
    }
}

impl Sha256Digest for &[u8] {
    fn digest(self) -> String {
        __digest__(self)
    }
}

impl Sha256Digest for &Vec<u8> {
    fn digest(self) -> String {
        __digest__(self)
    }
}

impl Sha256Digest for Vec<u8> {
    fn digest(self) -> String {
        __digest__(&self)
    }
}

impl Sha256Digest for String {
    fn digest(self) -> String {
        __digest__(self.as_bytes())
    }
}

impl Sha256Digest for &str {
    fn digest(self) -> String {
        __digest__(self.as_bytes())
    }
}

impl Sha256Digest for char {
    fn digest(self) -> String {
        __digest__(self.encode_utf8(&mut [0; 4]).as_bytes())
    }
}

impl Sha256Digest for &mut &str {
    fn digest(self) -> String {
        __digest__(self.as_bytes())
    }
}

impl Sha256Digest for &String {
    fn digest(self) -> String {
        __digest__(self.as_bytes())
    }
}

#[async_trait::async_trait]
impl<P> TrySha256Digest for P
where
    P: AsRef<Path> + Send,
{
    type Error = io::Error;

    fn digest(self) -> Result<String, Self::Error> {
        let f = fs::File::open(self)?;
        let reader = BufReader::new(f);
        let sha = Sha256::new();
        calc(reader, sha)
    }

    #[cfg(feature = "async")]
    async fn async_digest(self) -> Result<String, Self::Error> {
        let f = tokio::fs::File::open(self).await?;
        let reader = tokio::io::BufReader::new(f);
        let sha = Sha256::new();
        async_calc(reader, sha).await
    }

    #[cfg(all(feature = "async", feature = "native_openssl"))]
    async fn async_openssl_digest(self) -> Result<String, Self::Error> {
        let f = tokio::fs::File::open(self).await?;
        let reader = tokio::io::BufReader::new(f);
        let sha = OpenSslSha256::new();
        async_calc(reader, sha).await
    }
}

fn __digest__(data: &[u8]) -> String {
    hex::encode(Sha256::digest(data))
}

trait CalculatorInput {
    fn read_inner(&mut self, buf: &mut [u8]) -> std::io::Result<usize>;
}

impl<T> CalculatorInput for T
where
    T: Read,
{
    fn read_inner(&mut self, buf: &mut [u8]) -> io::Result<usize> {
        self.read(buf)
    }
}

pub trait CalculatorSelector {
    type FinishType: AsRef<[u8]>;
    fn update_inner(&mut self, data: &[u8]);
    fn finish_inner(self) -> Self::FinishType;
}

impl CalculatorSelector for Sha256 {
    type FinishType = Output<Sha256>;

    fn update_inner(&mut self, data: &[u8]) {
        self.update(data)
    }

    fn finish_inner(self) -> Self::FinishType {
        self.finalize()
    }
}

fn calc<I, S>(mut input: I, mut selector: S) -> io::Result<String>
where
    I: CalculatorInput,
    S: CalculatorSelector,
{
    let mut buf = [0u8; 1024];
    loop {
        let len = input.read_inner(&mut buf)?;
        if len == 0 {
            break;
        }
        selector.update_inner(&buf[0..len]);
    }
    let hash = selector.finish_inner();
    Ok(hex::encode(hash))
}
