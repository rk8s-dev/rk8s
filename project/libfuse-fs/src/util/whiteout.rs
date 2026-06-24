//! Whiteout format selection and helpers shared by overlayfs/unionfs.
//!
//! Two formats are supported:
//!
//! - [`WhiteoutFormat::CharDev`] — Linux overlayfs convention. A whiteout is a
//!   character device with major/minor 0/0 created via `mknod`. Requires
//!   `CAP_MKNOD` on Linux and root on macOS, so unprivileged macOS use needs
//!   the OCI form below.
//! - [`WhiteoutFormat::OciWhiteout`] — OCI image-spec convention. A whiteout is
//!   an empty regular file named `.wh.<base>`; opaque-directory marker is the
//!   special name `.wh..wh..opq`. Works without any elevated privileges.

use std::ffi::{OsStr, OsString};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum WhiteoutFormat {
    CharDev,
    OciWhiteout,
}

impl Default for WhiteoutFormat {
    /// macOS defaults to the OCI form because creating char devices via
    /// `mknod` requires root; Linux keeps the kernel-overlayfs convention.
    fn default() -> Self {
        if cfg!(target_os = "macos") {
            WhiteoutFormat::OciWhiteout
        } else {
            WhiteoutFormat::CharDev
        }
    }
}

/// File-name prefix that marks a whiteout in the OCI form.
pub const OCI_WHITEOUT_PREFIX: &str = ".wh.";

/// Special file name (also under the same prefix) that marks a directory as
/// opaque in the OCI form.
pub const OCI_OPAQUE_MARKER: &str = ".wh..wh..opq";

/// Returns `true` if `name` matches the OCI whiteout form `.wh.<base>` and is
/// **not** the opaque-directory marker.
pub fn is_oci_whiteout_name(name: &OsStr) -> bool {
    let bytes = name.as_encoded_bytes();
    bytes.starts_with(OCI_WHITEOUT_PREFIX.as_bytes()) && bytes != OCI_OPAQUE_MARKER.as_bytes()
}

/// Returns `true` if `name` is the OCI opaque-directory marker.
pub fn is_oci_opaque_marker(name: &OsStr) -> bool {
    name.as_encoded_bytes() == OCI_OPAQUE_MARKER.as_bytes()
}

/// If `raw` is an OCI whiteout name `.wh.<base>`, return `<base>`; otherwise
/// `None` (also `None` for the opaque marker).
pub fn oci_whiteout_target(raw: &OsStr) -> Option<&OsStr> {
    if !is_oci_whiteout_name(raw) {
        return None;
    }
    let bytes = raw.as_encoded_bytes();
    let target = &bytes[OCI_WHITEOUT_PREFIX.len()..];
    // SAFETY: bytes came from a valid OsStr; trimming a known UTF-8 prefix
    // leaves a valid OsStr encoding behind.
    Some(unsafe { OsStr::from_encoded_bytes_unchecked(target) })
}

/// Build the OCI whiteout name `.wh.<base>` for the given target.
pub fn oci_whiteout_name(base: &OsStr) -> OsString {
    use std::os::unix::ffi::OsStringExt;
    let mut bytes = OCI_WHITEOUT_PREFIX.as_bytes().to_vec();
    bytes.extend_from_slice(base.as_encoded_bytes());
    OsString::from_vec(bytes)
}

/// Returns `true` if the user is allowed to create a file with this name under
/// the given format. Used to reject `.wh.*` names from user-facing operations
/// when the OCI format is in use.
pub fn is_user_creatable_name(format: WhiteoutFormat, name: &OsStr) -> bool {
    match format {
        WhiteoutFormat::CharDev => true,
        WhiteoutFormat::OciWhiteout => {
            let bytes = name.as_encoded_bytes();
            !bytes.starts_with(OCI_WHITEOUT_PREFIX.as_bytes())
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn detects_whiteout_names() {
        assert!(is_oci_whiteout_name(OsStr::new(".wh.foo")));
        assert!(is_oci_whiteout_name(OsStr::new(".wh..hidden")));
        assert!(!is_oci_whiteout_name(OsStr::new("foo")));
        // Opaque marker is reported separately, not as a normal whiteout.
        assert!(!is_oci_whiteout_name(OsStr::new(OCI_OPAQUE_MARKER)));
    }

    #[test]
    fn detects_opaque_marker() {
        assert!(is_oci_opaque_marker(OsStr::new(OCI_OPAQUE_MARKER)));
        assert!(!is_oci_opaque_marker(OsStr::new(".wh.foo")));
    }

    #[test]
    fn target_extraction() {
        assert_eq!(
            oci_whiteout_target(OsStr::new(".wh.foo")),
            Some(OsStr::new("foo"))
        );
        assert_eq!(oci_whiteout_target(OsStr::new("foo")), None);
        assert_eq!(oci_whiteout_target(OsStr::new(OCI_OPAQUE_MARKER)), None);
    }

    #[test]
    fn name_construction_roundtrips() {
        let made = oci_whiteout_name(OsStr::new("payload"));
        assert_eq!(made, OsString::from(".wh.payload"));
        assert_eq!(oci_whiteout_target(&made), Some(OsStr::new("payload")));
    }

    #[test]
    fn user_creation_rejected_in_oci() {
        assert!(!is_user_creatable_name(
            WhiteoutFormat::OciWhiteout,
            OsStr::new(".wh.evil"),
        ));
        assert!(is_user_creatable_name(
            WhiteoutFormat::OciWhiteout,
            OsStr::new("normal"),
        ));
        // CharDev format places no name restriction.
        assert!(is_user_creatable_name(
            WhiteoutFormat::CharDev,
            OsStr::new(".wh.allowed"),
        ));
    }

    #[test]
    fn default_per_platform() {
        let want = if cfg!(target_os = "macos") {
            WhiteoutFormat::OciWhiteout
        } else {
            WhiteoutFormat::CharDev
        };
        assert_eq!(WhiteoutFormat::default(), want);
    }
}
