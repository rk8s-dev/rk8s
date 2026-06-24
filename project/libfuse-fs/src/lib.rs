// #[macro_use]
// extern crate log;

pub mod context;
pub mod overlayfs;
pub mod passthrough;
mod server;
pub mod unionfs;
pub mod util;

// Test utilities (only compiled during tests)
#[cfg(test)]
pub mod test_utils {
    pub fn privileged_tests_enabled() -> bool {
        std::env::var("RUN_PRIVILEGED_TESTS").ok().as_deref() == Some("1")
    }

    pub fn macfuse_tests_enabled() -> bool {
        std::env::var("RUN_MACFUSE_TESTS").ok().as_deref() == Some("1")
    }

    pub fn is_skippable_mount_error(err: &std::io::Error) -> bool {
        matches!(
            err.raw_os_error(),
            Some(libc::EPERM)
                | Some(libc::EACCES)
                | Some(libc::ENXIO)
                | Some(libc::ENODEV)
                | Some(libc::ENOTCONN)
        ) || matches!(
            err.kind(),
            std::io::ErrorKind::PermissionDenied
                | std::io::ErrorKind::NotFound
                | std::io::ErrorKind::TimedOut
        )
    }

    /// Macro: unwrap result or skip test when encountering EPERM (Permission denied).
    ///
    /// Behavior:
    /// - On Ok(v): returns v
    /// - On Err(e) where e -> io::Error has raw_os_error()==EPERM (or PermissionDenied):
    ///     * If env RUN_PRIVILEGED_TESTS=1 -> panic (treat as hard failure)
    ///     * Else: print a line indicating skip and `return` from test (so test counted as ignored when used with #[ignore]).
    /// - On Err(e) other than EPERM -> panic with diagnostic.
    ///
    /// Usage examples:
    /// let handle = unwrap_or_skip_eperm!(some_async_call.await, "mount session");
    #[macro_export]
    macro_rules! unwrap_or_skip_eperm {
        ($expr:expr, $ctx:expr) => {{
            match $expr {
                Ok(v) => v,
                Err(e) => {
                    let ioerr: std::io::Error = e.into();
                    let is_eperm = ioerr.raw_os_error() == Some(libc::EPERM)
                        || ioerr.kind() == std::io::ErrorKind::PermissionDenied;
                    if is_eperm {
                        if $crate::test_utils::privileged_tests_enabled() {
                            panic!(
                                "{} failed with EPERM while RUN_PRIVILEGED_TESTS=1: {:?}",
                                $ctx, ioerr
                            );
                        } else {
                            eprintln!("skip (EPERM) {}: {:?}", $ctx, ioerr);
                            return;
                        }
                    }
                    panic!("{} unexpected error: {:?}", $ctx, ioerr);
                }
            }
        }};
    }

    #[macro_export]
    macro_rules! unwrap_or_skip_mount_error {
        ($expr:expr, $ctx:expr) => {{
            match $expr {
                Ok(v) => v,
                Err(e) => {
                    let ioerr: std::io::Error = e.into();
                    if $crate::test_utils::is_skippable_mount_error(&ioerr) {
                        if $crate::test_utils::macfuse_tests_enabled() {
                            panic!("{} failed while RUN_MACFUSE_TESTS=1: {:?}", $ctx, ioerr);
                        } else {
                            eprintln!("skip (mount environment) {}: {:?}", $ctx, ioerr);
                            return;
                        }
                    }
                    panic!("{} unexpected error: {:?}", $ctx, ioerr);
                }
            }
        }};
    }
}
