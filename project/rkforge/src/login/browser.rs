use std::process::{Command, Stdio};

/// Best-effort attempt to open a URL in the default browser.
///
/// Failures are silently ignored – the terminal always prints
/// the URL so the user can open it manually.
pub fn open(url: &str) {
    let result = {
        #[cfg(target_os = "linux")]
        {
            Command::new("xdg-open")
                .arg(url)
                .stdout(Stdio::null())
                .stderr(Stdio::null())
                .spawn()
        }

        #[cfg(target_os = "macos")]
        {
            Command::new("open")
                .arg(url)
                .stdout(Stdio::null())
                .stderr(Stdio::null())
                .spawn()
        }

        #[cfg(target_os = "windows")]
        {
            // `cmd /C start` treats `&` as a command separator unless quoted.
            // Our login URL always contains query params, so quote it explicitly.
            let quoted_url = format!("\"{url}\"");
            Command::new("cmd")
                .args(["/C", "start", "", quoted_url.as_str()])
                .stdout(Stdio::null())
                .stderr(Stdio::null())
                .spawn()
        }

        #[cfg(not(any(target_os = "linux", target_os = "macos", target_os = "windows")))]
        {
            Err(std::io::Error::new(
                std::io::ErrorKind::Unsupported,
                "unsupported platform",
            ))
        }
    };

    if let Err(e) = result {
        tracing::debug!("failed to open browser: {e}");
    }
}
