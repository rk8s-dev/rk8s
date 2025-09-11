use crate::{CalculatorSelector, TrySha256Digest};
use bytes::BytesMut;
use std::io;

/// sha256 digest file
///
/// # Examples
///
/// ```rust
/// use sha256::{try_async_digest};
/// use std::path::Path;
/// let input = Path::new("./foo.file");
/// tokio_test::block_on(async{
/// let val = try_async_digest(input).await.unwrap();
/// assert_eq!(val,"433855b7d2b96c23a6f60e70c655eb4305e8806b682a9596a200642f947259b1")
/// });
///
/// ```
pub async fn try_async_digest<D: TrySha256Digest>(input: D) -> Result<String, D::Error> {
    input.async_digest().await
}

/// sha256 digest file
///
/// # Examples
///
/// ```rust
/// use sha256::{try_async_openssl_digest};
/// use std::path::Path;
/// let input = Path::new("./foo.file");
/// tokio_test::block_on(async{
/// let val = try_async_openssl_digest(input).await.unwrap();
/// assert_eq!(val,"433855b7d2b96c23a6f60e70c655eb4305e8806b682a9596a200642f947259b1")
/// });
/// ```
#[cfg(feature = "native_openssl")]
pub async fn try_async_openssl_digest<D: TrySha256Digest>(input: D) -> Result<String, D::Error> {
    input.async_openssl_digest().await
}

#[async_trait::async_trait]
pub trait AsyncCalculatorInput {
    async fn read_inner(&mut self, buf: &mut BytesMut) -> io::Result<usize>;
}

pub async fn async_calc<I, S>(mut input: I, mut selector: S) -> io::Result<String>
where
    I: AsyncCalculatorInput,
    S: CalculatorSelector,
{
    let mut buf = BytesMut::with_capacity(1024);
    loop {
        buf.clear();
        let len = input.read_inner(&mut buf).await?;
        if len == 0 {
            break;
        }
        selector.update_inner(&buf[0..len]);
    }
    let hash = selector.finish_inner();
    Ok(hex::encode(hash))
}

#[async_trait::async_trait]
impl<R> AsyncCalculatorInput for tokio::io::BufReader<R>
where
    R: tokio::io::AsyncRead + Unpin + Send,
{
    async fn read_inner(&mut self, buf: &mut BytesMut) -> io::Result<usize> {
        use tokio::io::AsyncReadExt;

        self.read_buf(buf).await
    }
}
