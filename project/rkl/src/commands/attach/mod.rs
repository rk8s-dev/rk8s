use anyhow::{Error, Result};
use clap::Args;
use nix::sys::termios::{self, LocalFlags, SetArg, Termios};
use std::io::{Read, Write};
use std::os::fd::{AsFd, BorrowedFd, FromRawFd};
use std::os::unix::io::AsRawFd;

use crate::daemon::tty::{TTY_SOCK_PATH, TtyIpcRequest, TtyIpcResponse};

#[derive(Args, Debug, Clone)]
pub struct AttachCommand {
    #[arg(value_name = "CONTAINER_ID")]
    pub container_id: String,
}

// ── Terminal helpers ──────────────────────────────────────────────────────────

fn enter_raw_mode(fd: impl AsFd) -> Result<Termios> {
    let original = termios::tcgetattr(&fd)?;
    let mut raw = original.clone();
    raw.local_flags.remove(
        LocalFlags::ICANON
            | LocalFlags::ECHO
            | LocalFlags::ECHOE
            | LocalFlags::ECHOK
            | LocalFlags::ECHONL
            | LocalFlags::ISIG
            | LocalFlags::IEXTEN,
    );
    raw.input_flags.remove(
        nix::sys::termios::InputFlags::IXON
            | nix::sys::termios::InputFlags::ICRNL
            | nix::sys::termios::InputFlags::BRKINT
            | nix::sys::termios::InputFlags::INPCK
            | nix::sys::termios::InputFlags::ISTRIP,
    );
    raw.output_flags
        .remove(nix::sys::termios::OutputFlags::OPOST);
    raw.control_flags
        .insert(nix::sys::termios::ControlFlags::CS8);
    raw.control_chars[termios::SpecialCharacterIndices::VMIN as usize] = 1;
    raw.control_chars[termios::SpecialCharacterIndices::VTIME as usize] = 0;
    termios::tcsetattr(&fd, SetArg::TCSANOW, &raw)?;
    Ok(original)
}

fn restore_terminal(fd: BorrowedFd<'_>, saved: &Termios) {
    let _ = termios::tcsetattr(fd, SetArg::TCSANOW, saved);
}

// IPC helpers: simple length-prefixed JSON messages over the socket

fn send_json<T: serde::Serialize>(fd: std::os::unix::io::RawFd, value: &T) -> Result<()> {
    let payload = serde_json::to_vec(value)?;
    let len = payload.len() as u32;
    use std::mem::ManuallyDrop;
    let mut file = ManuallyDrop::new(unsafe { std::fs::File::from_raw_fd(fd) });
    file.write_all(&len.to_be_bytes())?;
    file.write_all(&payload)?;
    Ok(())
}

fn recv_json<T: serde::de::DeserializeOwned>(fd: std::os::unix::io::RawFd) -> Result<T> {
    use std::mem::ManuallyDrop;
    let mut file = ManuallyDrop::new(unsafe { std::fs::File::from_raw_fd(fd) });
    let mut len_buf = [0u8; 4];
    file.read_exact(&mut len_buf)?;
    let len = u32::from_be_bytes(len_buf) as usize;
    if len > 4 * 1024 * 1024 {
        anyhow::bail!("[attach] message too large ({} bytes)", len);
    }
    let mut body = vec![0u8; len];
    file.read_exact(&mut body)?;
    Ok(serde_json::from_slice(&body)?)
}

/// print debug info to stderr in raw mode, replace \n with \r\n
macro_rules! raw_debug {
    ($($arg:tt)*) => {{
        let msg = format!($($arg)*);
        let msg = msg.replace('\n', "\r\n");
        let _ = std::io::Write::write_all(&mut std::io::stderr(), format!("[attach-dbg] {}\r\n", msg).as_bytes());
    }};
}

pub fn attach_execute(cmd: AttachCommand) -> Result<(), Error> {
    // 1. Connect to daemon tty-ipc socket and send Attach request.
    let sock = std::os::unix::net::UnixStream::connect(TTY_SOCK_PATH)
        .map_err(|e| anyhow::anyhow!("connect tty-ipc: {e} (is rkl daemon running?)"))?;
    let sock_fd = sock.as_raw_fd();

    send_json(
        sock_fd,
        &TtyIpcRequest::Attach {
            container_id: cmd.container_id.clone(),
        },
    )?;

    if let TtyIpcResponse::Error { message } = recv_json(sock_fd)? {
        anyhow::bail!("attach: {message}");
    }

    eprintln!(
        "[attach-dbg] attached to '{}'. Ctrl-P Ctrl-Q to detach.",
        cmd.container_id
    );

    // 2. Switch local terminal to raw mode.
    let stdin = std::io::stdin();
    let is_tty = nix::unistd::isatty(stdin.as_raw_fd()).unwrap_or(false);
    eprintln!("[attach-dbg] stdin is_tty={is_tty}, entering raw mode");
    let saved_termios = if is_tty {
        Some(enter_raw_mode(&stdin)?)
    } else {
        None
    };

    // 3. stdin → daemon socket (daemon forwards to pty master).
    //    write directly to sock_fd from this thread, and signal detach via shutdown(SHUT_WR)
    //    so the daemon's reader sees EOF without closing the fd (main thread still reads from it).
    let stdin_thread = std::thread::spawn(move || {
        let mut stdin = std::io::stdin().lock();
        let mut buf = [0u8; 256];
        let mut prev_ctrl_p = false;
        let mut writer =
            unsafe { std::mem::ManuallyDrop::new(std::fs::File::from_raw_fd(sock_fd)) };
        loop {
            let n = match stdin.read(&mut buf) {
                Ok(0) | Err(_) => break,
                Ok(n) => n,
            };
            // Detach sequence: Ctrl-P (0x10) then Ctrl-Q (0x11)
            for i in 0..n {
                if prev_ctrl_p && buf[i] == 0x11 {
                    // Shut down the write half of the socket so daemon's
                    // input loop gets EOF, triggering broadcast cleanup
                    // and unblocking the main thread's reader.
                    let _ = nix::sys::socket::shutdown(sock_fd, nix::sys::socket::Shutdown::Write);
                    raw_debug!(
                        "detach sequence detected, shutting down write half of socket and exiting stdin thread"
                    );
                    return; // detach
                }
                prev_ctrl_p = buf[i] == 0x10;
            }
            if writer.write_all(&buf[..n]).is_err() {
                break;
            }
        }
    });

    // 4. daemon socket → stdout (daemon forwards tee output to us).
    //    Runs on the main thread; blocks until daemon closes the connection
    {
        let mut reader =
            unsafe { std::mem::ManuallyDrop::new(std::fs::File::from_raw_fd(sock_fd)) };
        let mut stdout = std::io::stdout();
        let mut buf = [0u8; 4096];
        loop {
            let n = match reader.read(&mut buf) {
                Ok(0) | Err(_) => break,
                Ok(n) => n,
            };
            if stdout.write_all(&buf[..n]).is_err() {
                break;
            }
            let _ = stdout.flush();
        }
        raw_debug!("reader from daemon socket quit");
    }

    let _ = stdin_thread.join();

    // 5. Restore terminal.
    if let Some(ref saved) = saved_termios {
        restore_terminal(stdin.as_fd(), saved);
    }

    eprintln!("\n[rkl] detached from '{}'.", cmd.container_id);
    Ok(())
}
