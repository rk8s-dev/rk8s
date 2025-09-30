pub use crate::utils::string::{ensure_no_leading_slash, ensure_no_trailing_slash, ensure_trailing_slash};

pub fn sanitize_path(s: &str) -> String {
    ensure_no_trailing_slash(&ensure_no_leading_slash(s))
}
