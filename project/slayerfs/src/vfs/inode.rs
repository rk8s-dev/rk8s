use anyhow::Context;
use std::sync::Arc;
use tokio::sync::watch;

#[derive(Clone)]
pub struct Inode {
    ino: i64,
    length_rx: watch::Receiver<u64>,
    length_tx: watch::Sender<u64>,
}

impl Inode {
    pub fn new(ino: i64, size: u64) -> Arc<Inode> {
        let (tx, rx) = watch::channel(size);

        Arc::new(Self {
            ino,
            length_rx: rx,
            length_tx: tx,
        })
    }

    pub fn ino(&self) -> i64 {
        self.ino
    }

    pub fn file_size(&self) -> u64 {
        *self.length_rx.borrow()
    }

    pub fn update_size(&self, new_size: u64) -> anyhow::Result<()> {
        self.length_tx
            .send(new_size)
            .with_context(|| "Failed to update file size")
    }
}
