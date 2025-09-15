use const_format::formatcp;

/// API incompatible changes.
pub const VERSION_MAJOR: u32 = 1;

/// Changing functionality in a backwards-compatible manner
pub const VERSION_MINOR: u32 = 0;

/// Backwards-compatible bug fixes.
pub const VERSION_PATCH: u32 = 0;

/// Indicates development branch. Releases will be empty string.
pub const VERSION_DEV: &str = "";

/// Retrieve the version as static str representation.
pub const VERSION: &str = formatcp!("{VERSION_MAJOR}.{VERSION_MINOR}.{VERSION_PATCH}{VERSION_DEV}");

/// Retrieve the version as string representation.
///
/// Use [`VERSION`] instead.
#[deprecated]
pub fn version() -> String {
    VERSION.to_owned()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    #[allow(deprecated)]
    fn version_test() {
        assert_eq!(version(), "1.0.0".to_string())
    }
}
