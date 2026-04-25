use anyhow::Result;
use nix::libc;
use tokio::sync::mpsc;

use std::collections::HashMap;
use std::io::Write;
use std::os::fd::{AsRawFd, FromRawFd, OwnedFd, RawFd};
use std::sync::{Mutex, OnceLock};

/// Holds the pty master OwnedFd for each container.
pub static TTY_STORE: OnceLock<Mutex<HashMap<String, OwnedFd>>> = OnceLock::new();

/// Holds the attach broadcast sender for each container.
/// The daemon-owned PTY tee thread pushes output chunks here; active attach
/// sessions read from the paired receiver.
pub static ATTACH_TX_STORE: OnceLock<Mutex<HashMap<String, mpsc::Sender<Vec<u8>>>>> =
    OnceLock::new();

pub struct LocalAttachSession {
    container_id: String,
    master_fd: OwnedFd,
    output_rx: mpsc::Receiver<Vec<u8>>,
}

impl LocalAttachSession {
    pub fn into_parts(self) -> (String, OwnedFd, mpsc::Receiver<Vec<u8>>) {
        (self.container_id, self.master_fd, self.output_rx)
    }
}

/// Called by the daemon-owned PTY tee thread for every raw chunk read from the
/// pty master.
pub fn broadcast_to_attach(container_id: &str, data: &[u8]) {
    if let Some(store) = ATTACH_TX_STORE.get()
        && let Ok(guard) = store.lock()
        && let Some(tx) = guard.get(container_id)
    {
        let _ = tx.try_send(data.to_vec());
    }
}

fn close_attach_channel(container_id: &str) -> Result<()> {
    if let Some(store) = ATTACH_TX_STORE.get() {
        store
            .lock()
            .map_err(|e| anyhow::anyhow!("ATTACH_TX_STORE lock poisoned: {e}"))?
            .remove(container_id);
    }
    Ok(())
}

pub fn close_attach_session(container_id: &str) -> Result<()> {
    close_attach_channel(container_id)
}

pub fn write_attach_input(master_fd: &OwnedFd, data: &[u8]) -> Result<()> {
    use std::fs::File;
    use std::mem::ManuallyDrop;

    let mut file = ManuallyDrop::new(unsafe { File::from_raw_fd(master_fd.as_raw_fd()) });
    file.write_all(data)?;
    Ok(())
}

pub fn resize_attach_pty(master_fd: &OwnedFd, rows: u16, cols: u16) -> Result<()> {
    let winsize = libc::winsize {
        ws_row: rows,
        ws_col: cols,
        ws_xpixel: 0,
        ws_ypixel: 0,
    };

    let rc = unsafe { libc::ioctl(master_fd.as_raw_fd(), libc::TIOCSWINSZ, &winsize) };
    if rc == -1 {
        return Err(anyhow::anyhow!(
            "failed to resize PTY: {}",
            std::io::Error::last_os_error()
        ));
    }

    Ok(())
}

pub fn open_attach_session(container_id: &str) -> Result<LocalAttachSession> {
    let master_dup = {
        let store = TTY_STORE
            .get_or_init(|| Mutex::new(HashMap::new()))
            .lock()
            .map_err(|e| anyhow::anyhow!("TTY_STORE lock poisoned: {e}"))?;
        let owned_fd = store
            .get(container_id)
            .ok_or_else(|| anyhow::anyhow!("container '{}' has no PTY", container_id))?;
        nix::unistd::dup(owned_fd.as_raw_fd())?
    };

    let (tx, rx) = mpsc::channel::<Vec<u8>>(64);
    ATTACH_TX_STORE
        .get_or_init(|| Mutex::new(HashMap::new()))
        .lock()
        .map_err(|e| anyhow::anyhow!("ATTACH_TX_STORE lock poisoned: {e}"))?
        .insert(container_id.to_string(), tx);

    Ok(LocalAttachSession {
        container_id: container_id.to_string(),
        master_fd: unsafe { OwnedFd::from_raw_fd(master_dup) },
        output_rx: rx,
    })
}

/// Register a PTY master fd directly into TTY_STORE without IPC.
/// Use this when the caller is running inside the daemon process.
/// `master_fd` must be a freshly-owned fd (will be wrapped in OwnedFd).
pub fn register_tty_local(container_id: &str, master_fd: RawFd) -> Result<()> {
    let owned = unsafe { OwnedFd::from_raw_fd(master_fd) };
    TTY_STORE
        .get_or_init(|| Mutex::new(HashMap::new()))
        .lock()
        .map_err(|e| anyhow::anyhow!("TTY_STORE lock poisoned: {e}"))?
        .insert(container_id.to_string(), owned);
    Ok(())
}

/// Drop the stored PTY master fd and any attach sender for this container.
pub fn unregister_tty_local(container_id: &str) -> Result<()> {
    if let Some(store) = TTY_STORE.get() {
        store
            .lock()
            .map_err(|e| anyhow::anyhow!("TTY_STORE lock poisoned: {e}"))?
            .remove(container_id);
    }

    if let Some(store) = ATTACH_TX_STORE.get() {
        store
            .lock()
            .map_err(|e| anyhow::anyhow!("ATTACH_TX_STORE lock poisoned: {e}"))?
            .remove(container_id);
    }

    Ok(())
}
