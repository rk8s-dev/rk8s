//! FUSE adapter helper utilities.
//!
//! This module contains small, shared validation helpers that keep request
//! handlers in `fuse/mod.rs` focused on orchestration and errno mapping.

use rfuse3::Errno;

/// Validate rename source/destination names from FUSE request payload.
pub(crate) fn validate_rename_names(name: &str, new_name: &str) -> Result<(), Errno> {
    if name.is_empty() || new_name.is_empty() {
        return Err(libc::EINVAL.into());
    }

    if name.contains('/') || name.contains('\0') || new_name.contains('/') || new_name.contains('\0') {
        return Err(libc::EINVAL.into());
    }

    Ok(())
}

/// Whether a rename request is a same-path no-op.
pub(crate) fn is_same_rename(parent: u64, name: &str, new_parent: u64, new_name: &str) -> bool {
    parent == new_parent && name == new_name
}
