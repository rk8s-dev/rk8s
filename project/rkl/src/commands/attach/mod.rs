use anyhow::{Error, Result, anyhow};
use clap::Args;
use common::quic::{recv_frame, send_frame};
use common::{AttachControlMessage, AttachTarget};
use nix::libc;
use nix::sys::termios::{self, LocalFlags, SetArg, Termios};
use std::io::Write;
use std::os::fd::{AsFd, BorrowedFd};
use std::os::unix::io::AsRawFd;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::Duration;
use tokio::sync::mpsc;

use crate::commands::pod::TLSConnectionArgs;
use crate::quic::client::{Cli, QUICClient};

#[derive(Args, Debug, Clone)]
pub struct AttachCommand {
    #[arg(value_name = "POD_NAME")]
    pub pod_name: String,

    #[arg(short = 'c', long, value_name = "CONTAINER")]
    pub container: Option<String>,

    #[arg(short = 'n', long, value_name = "NAMESPACE", default_value = "default")]
    pub namespace: String,

    #[arg(long, value_name = "RKS_ADDRESS", env = "RKS_ADDRESS")]
    pub cluster: String,

    #[clap(flatten)]
    pub tls_cfg: TLSConnectionArgs,
}

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

fn current_winsize(fd: i32) -> Option<(u16, u16)> {
    let mut winsize = libc::winsize {
        ws_row: 0,
        ws_col: 0,
        ws_xpixel: 0,
        ws_ypixel: 0,
    };
    let rc = unsafe { libc::ioctl(fd, libc::TIOCGWINSZ, &mut winsize) };
    if rc == -1 || winsize.ws_row == 0 || winsize.ws_col == 0 {
        return None;
    }
    Some((winsize.ws_row, winsize.ws_col))
}

pub fn attach_execute(cmd: AttachCommand) -> Result<(), Error> {
    let stdin = std::io::stdin();
    let is_tty = nix::unistd::isatty(stdin.as_raw_fd()).unwrap_or(false);
    let saved_termios = if is_tty {
        Some(enter_raw_mode(&stdin)?)
    } else {
        None
    };

    let result = tokio::runtime::Runtime::new()?.block_on(async move { attach_cluster(cmd).await });

    if let Some(ref saved) = saved_termios {
        restore_terminal(stdin.as_fd(), saved);
    }

    result
}

async fn attach_cluster(cmd: AttachCommand) -> Result<(), Error> {
    let cli = QUICClient::<Cli>::connect(&cmd.cluster, &cmd.tls_cfg).await?;
    let mut stream = cli.open_bi().await?;

    stream
        .send_frame(&AttachControlMessage::Open(AttachTarget {
            namespace: cmd.namespace.clone(),
            pod_name: cmd.pod_name.clone(),
            container_name: cmd.container.clone(),
        }))
        .await?;

    match stream.recv_frame::<AttachControlMessage>().await? {
        AttachControlMessage::Ack => {}
        AttachControlMessage::Error(message) => anyhow::bail!("attach: {message}"),
        other => anyhow::bail!("attach: unexpected response: {other:?}"),
    }

    eprintln!(
        "[rkl] attached to {}/{}{}. Ctrl-P Ctrl-Q to detach.",
        cmd.namespace,
        cmd.pod_name,
        cmd.container
            .as_deref()
            .map(|container| format!(" (container {container})"))
            .unwrap_or_default()
    );

    let (mut send_stream, mut recv_stream) = stream.into_inner();
    let (tx, mut rx) = mpsc::unbounded_channel::<AttachControlMessage>();
    let stop = Arc::new(AtomicBool::new(false));

    if let Some((rows, cols)) = current_winsize(std::io::stdin().as_raw_fd()) {
        let _ = tx.send(AttachControlMessage::Resize { rows, cols });
    }

    let writer_stop = stop.clone();
    let writer = tokio::spawn(async move {
        while let Some(msg) = rx.recv().await {
            send_frame(&mut send_stream, &msg).await?;
            if matches!(
                msg,
                AttachControlMessage::Close | AttachControlMessage::Error(_)
            ) {
                break;
            }
        }
        writer_stop.store(true, Ordering::SeqCst);
        let _ = send_stream.finish();
        Ok::<(), anyhow::Error>(())
    });

    let stdin_stop = stop.clone();
    let stdin_tx = tx.clone();
    let stdin_thread = std::thread::spawn(move || {
        let mut stdin = std::io::stdin().lock();
        let mut buf = [0u8; 256];
        let mut prev_ctrl_p = false;
        while !stdin_stop.load(Ordering::SeqCst) {
            let n = match std::io::Read::read(&mut stdin, &mut buf) {
                Ok(0) | Err(_) => break,
                Ok(n) => n,
            };
            for &byte in buf.iter().take(n) {
                if prev_ctrl_p && byte == 0x11 {
                    let _ = stdin_tx.send(AttachControlMessage::Close);
                    stdin_stop.store(true, Ordering::SeqCst);
                    return;
                }
                prev_ctrl_p = byte == 0x10;
            }
            if stdin_tx
                .send(AttachControlMessage::Data(buf[..n].to_vec()))
                .is_err()
            {
                break;
            }
        }
    });

    let resize_stop = stop.clone();
    let resize_tx = tx.clone();
    let resize_thread = std::thread::spawn(move || {
        let stdin = std::io::stdin();
        let fd = stdin.as_raw_fd();
        let mut last_size = current_winsize(fd);
        while !resize_stop.load(Ordering::SeqCst) {
            std::thread::sleep(Duration::from_millis(250));
            let size = current_winsize(fd);
            if size.is_some() && size != last_size {
                last_size = size;
                if let Some((rows, cols)) = size
                    && resize_tx
                        .send(AttachControlMessage::Resize { rows, cols })
                        .is_err()
                {
                    break;
                }
            }
        }
    });

    let reader_stop = stop.clone();
    let reader = tokio::spawn(async move {
        let mut stdout = std::io::stdout();
        loop {
            let msg = recv_frame::<AttachControlMessage>(&mut recv_stream).await?;
            match msg {
                AttachControlMessage::Data(data) => {
                    stdout.write_all(&data)?;
                    stdout.flush()?;
                }
                AttachControlMessage::Close => {
                    break;
                }
                AttachControlMessage::Error(message) => {
                    return Err(anyhow!("attach: {message}"));
                }
                AttachControlMessage::Resize { .. }
                | AttachControlMessage::Open(_)
                | AttachControlMessage::Ack => {}
            }
        }
        reader_stop.store(true, Ordering::SeqCst);
        Ok::<(), anyhow::Error>(())
    });

    let reader_result = reader
        .await
        .map_err(|e| anyhow!("attach reader task failed: {e}"))?;
    stop.store(true, Ordering::SeqCst);
    drop(tx);
    let _ = stdin_thread.join();
    let _ = resize_thread.join();
    writer
        .await
        .map_err(|e| anyhow!("attach writer task failed: {e}"))??;
    reader_result?;

    eprintln!("\n[rkl] detached from '{}'.", cmd.pod_name);
    Ok(())
}
