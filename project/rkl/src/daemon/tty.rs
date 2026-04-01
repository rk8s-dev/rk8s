use anyhow::Result;
use tracing::{debug, error};

use std::collections::HashMap;
use std::io::{Read, Write};
use std::os::fd::{AsRawFd, FromRawFd, OwnedFd, RawFd};
use std::sync::mpsc::{SyncSender, sync_channel};
use std::sync::{Mutex, OnceLock};

pub const TTY_SOCK_PATH: &str = "/run/rkl/tty-ipc.sock";

struct SocketPathGuard {
    path: &'static str,
}

impl SocketPathGuard {
    fn new(path: &'static str) -> Self {
        Self { path }
    }
}

impl Drop for SocketPathGuard {
    fn drop(&mut self) {
        if let Err(e) = std::fs::remove_file(self.path)
            && e.kind() != std::io::ErrorKind::NotFound
        {
            error!(
                "[tty-ipc] failed to remove socket path {}: {}",
                self.path, e
            );
        }
    }
}

/// Holds the pty master OwnedFd for each container.
pub static TTY_STORE: OnceLock<Mutex<HashMap<String, OwnedFd>>> = OnceLock::new();

/// Holds the attach broadcast sender for each container.
/// The tee thread in libruntime calls `broadcast_to_attach` to push output
/// chunks here; the attach session reads from the paired receiver.
pub static ATTACH_TX_STORE: OnceLock<Mutex<HashMap<String, SyncSender<Vec<u8>>>>> = OnceLock::new();

/// Called by the libruntime tee thread for every raw chunk from the pty master.
/// Registered via `libruntime::ops::set_attach_broadcast`.
pub fn broadcast_to_attach(container_id: &str, data: &[u8]) {
    if let Some(store) = ATTACH_TX_STORE.get() {
        if let Ok(guard) = store.lock() {
            if let Some(tx) = guard.get(container_id) {
                let _ = tx.try_send(data.to_vec());
            }
        }
    }
}

#[derive(serde::Serialize, serde::Deserialize)]
#[serde(tag = "type")]
pub enum TtyIpcRequest {
    /// Establish a bidirectional attach session.
    /// After Ok response the connection becomes a raw byte tunnel:
    ///   daemon → client : container stdout (from tee broadcast)
    ///   client → daemon : keystrokes forwarded to pty master
    Attach { container_id: String },
}

#[derive(serde::Serialize, serde::Deserialize)]
#[serde(tag = "type")]
pub enum TtyIpcResponse {
    Ok,
    Error { message: String },
}

/// Write a serialized JSON message to fd
/// Message Structure: Len of JSON String(4 Bytes) + JSON String.
fn send_json<T: serde::Serialize>(fd: RawFd, value: &T) -> Result<()> {
    let payload = serde_json::to_vec(value)?;
    let len = payload.len() as u32;

    use std::fs::File; // convert raw fd to File to use write_all()
    use std::mem::ManuallyDrop; // ManuallyDrop, avoid auto drop fd
    let mut file = ManuallyDrop::new(unsafe { File::from_raw_fd(fd) });
    file.write_all(&len.to_be_bytes())?;
    file.write_all(&payload)?;
    Ok(())
}

/// Read a JSON message from fd and deserialize it.
/// Message Structure: Len of JSON String(4 Bytes) + JSON String.
fn recv_json<T: serde::de::DeserializeOwned>(fd: RawFd) -> Result<T> {
    let mut file = std::mem::ManuallyDrop::new(unsafe { std::fs::File::from_raw_fd(fd) });
    let mut len_buf = [0u8; 4];
    file.read_exact(&mut len_buf)?;
    let len = u32::from_be_bytes(len_buf) as usize;
    if len > 4 * 1024 * 1024 {
        // anyhow::bail! = return early with an error
        anyhow::bail!("[tty-ipc]: message too large ({} bytes)", len);
    }
    let mut body = vec![0u8; len];
    file.read_exact(&mut body)?;
    Ok(serde_json::from_slice(&body)?)
}

/// Listen on the TTY socket and serve attach sessions.
pub async fn run_server() -> Result<()> {
    if let Some(parent) = std::path::Path::new(TTY_SOCK_PATH).parent()
        && !parent.exists()
    {
        std::fs::create_dir_all(parent)?;
    }

    if let Err(e) = std::fs::remove_file(TTY_SOCK_PATH)
        && e.kind() != std::io::ErrorKind::NotFound
    {
        return Err(anyhow::anyhow!(
            "[tty-ipc] failed to remove stale socket {}: {}",
            TTY_SOCK_PATH,
            e
        ));
    }

    // Register the broadcast hook so the tee thread in libruntime can reach us.
    libruntime::ops::set_attach_broadcast(broadcast_to_attach);
    // Initialise ATTACH_TX_STORE alongside TTY_STORE
    ATTACH_TX_STORE.get_or_init(|| Mutex::new(HashMap::new()));

    let _socket_guard = SocketPathGuard::new(TTY_SOCK_PATH);
    let listener = tokio::net::UnixListener::bind(TTY_SOCK_PATH)?;

    loop {
        let (stream, _) = listener.accept().await?;
        tokio::spawn(async move {
            if let Err(e) = handle_connection(stream).await {
                error!("[tty-ipc] connection error: {e}");
            }
        });
    }
}

async fn handle_connection(stream: tokio::net::UnixStream) -> Result<()> {
    // The socket protocol here is synchronous, so run it on a blocking thread.
    // tokio sets the underlying fd to O_NONBLOCK; clear it so sync read() blocks properly.
    let std_stream = stream.into_std()?;
    std_stream.set_nonblocking(false)?;
    tokio::task::spawn_blocking(move || handle_connection_sync(std_stream))
        .await
        .map_err(|e| anyhow::anyhow!("[tty-ipc] spawn_blocking panicked: {e}"))?
}

fn handle_connection_sync(stream: std::os::unix::net::UnixStream) -> Result<()> {
    let fd = stream.as_raw_fd();

    let req: TtyIpcRequest = recv_json(fd)?;

    match req {
        TtyIpcRequest::Attach { container_id } => {
            debug!("[tty-ipc] Attach request for container_id='{container_id}', sock_fd={fd}");

            // Verify the container has a PTY and get the master fd (raw, borrowed)
            let master_raw = {
                let store = TTY_STORE
                    .get()
                    .unwrap()
                    .lock()
                    .map_err(|e| anyhow::anyhow!("TTY_STORE lock poisoned: {e}"))?;
                match store.get(&container_id) {
                    Some(owned_fd) => {
                        let raw = owned_fd.as_raw_fd();
                        raw
                    }
                    None => {
                        send_json(
                            fd,
                            &TtyIpcResponse::Error {
                                message: format!("container '{}' has no PTY", container_id),
                            },
                        )?;
                        return Ok(());
                    }
                }
            };

            send_json(fd, &TtyIpcResponse::Ok)?;

            // Register broadcast channel so the tee thread can reach this session
            let (tx, rx) = sync_channel::<Vec<u8>>(64);
            ATTACH_TX_STORE
                .get()
                .unwrap()
                .lock()
                .map_err(|e| anyhow::anyhow!("ATTACH_TX_STORE lock poisoned: {e}"))?
                .insert(container_id.clone(), tx);

            // Output: tee broadcast → attach client
            let container_id_out = container_id.clone();
            let write_fd = nix::unistd::dup(fd)?;
            let output_thread = std::thread::spawn(move || {
                let mut writer =
                    unsafe { std::mem::ManuallyDrop::new(std::fs::File::from_raw_fd(write_fd)) };
                for chunk in rx {
                    debug!("tty output writer send {} bytes", chunk.len());
                    if writer.write_all(&chunk).is_err() {
                        break;
                    }
                }
                if let Some(store) = ATTACH_TX_STORE.get() {
                    let _ = store.lock().map(|mut g| g.remove(&container_id_out));
                }
                // Explicitly close write_fd so the attach client's reader
                // sees EOF and the attach process can exit cleanly.
                let _ = nix::unistd::close(write_fd);
            });

            // Input: attach client → pty master
            {
                let mut reader =
                    unsafe { std::mem::ManuallyDrop::new(std::fs::File::from_raw_fd(fd)) };
                let mut master_write =
                    unsafe { std::mem::ManuallyDrop::new(std::fs::File::from_raw_fd(master_raw)) };
                let mut buf = [0u8; 256];
                loop {
                    let n = match reader.read(&mut buf) {
                        Ok(0) | Err(_) => {
                            debug!("daemon tty input thread: read socket over");
                            break;
                        }
                        Ok(n) => n,
                    };
                    debug!("tty input reader receive {n} bytes");
                    if master_write.write_all(&buf[..n]).is_err() {
                        debug!("[tty-ipc] input loop: write to pty master failed");
                        break;
                    }
                }
            }

            // Client disconnected: stop broadcasting to this session
            if let Some(store) = ATTACH_TX_STORE.get() {
                let _ = store.lock().map(|mut g| g.remove(&container_id));
            }
            let _ = output_thread.join();
        }
    }
    Ok(())
}

/// Register a PTY master fd directly into TTY_STORE without IPC.
/// Use this when the caller is running inside the daemon process.
/// `master_fd` must be a freshly-owned fd (will be wrapped in OwnedFd).
pub fn register_tty_local(container_id: &str, master_fd: RawFd) -> Result<()> {
    let owned = unsafe { OwnedFd::from_raw_fd(master_fd) };
    TTY_STORE
        .get()
        .ok_or_else(|| anyhow::anyhow!("TTY_STORE not initialised"))?
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
