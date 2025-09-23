use indicatif::ProgressBar;
use std::pin::Pin;
use std::task::Poll;
use tokio::io::{AsyncWrite, BufWriter};

pub struct LayerDownloadWrapper<W: AsyncWrite + Unpin> {
    inner: BufWriter<W>,
    progress_bar: ProgressBar,
}

impl<W: AsyncWrite + Unpin> LayerDownloadWrapper<W> {
    pub fn new(writer: W, progress_bar: ProgressBar) -> Self {
        Self {
            inner: BufWriter::new(writer),
            progress_bar,
        }
    }
}

impl<W: AsyncWrite + Unpin> AsyncWrite for LayerDownloadWrapper<W> {
    fn poll_write(
        mut self: std::pin::Pin<&mut Self>,
        cx: &mut std::task::Context<'_>,
        buf: &[u8],
    ) -> std::task::Poll<std::result::Result<usize, std::io::Error>> {
        let res = Pin::new(&mut self.inner).poll_write(cx, buf);
        if let Poll::Ready(Ok(n)) = &res {
            self.progress_bar.inc(*n as u64);
        }
        res
    }

    fn poll_flush(
        mut self: Pin<&mut Self>,
        cx: &mut std::task::Context<'_>,
    ) -> Poll<std::result::Result<(), std::io::Error>> {
        Pin::new(&mut self.inner).poll_flush(cx)
    }

    fn poll_shutdown(
        mut self: Pin<&mut Self>,
        cx: &mut std::task::Context<'_>,
    ) -> Poll<std::result::Result<(), std::io::Error>> {
        Pin::new(&mut self.inner).poll_shutdown(cx)
    }
}
