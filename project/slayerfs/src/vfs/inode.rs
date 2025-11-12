use std::sync::Arc;
use tokio::sync::watch;

/// The `Inode`, which holds file attribute state, as a local cache.
/// Slayerfs ensure `close-to-open` semantics, that is to say, each `open` operation must see
/// the newest file states. Otherwise, it is permitted to see stale states.
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

    pub fn update_size(&self, new_size: u64) {
        self.length_tx
            .send(new_size)
            .expect("Inode invariant violated: all receivers dropped in update_size");
    }
}
