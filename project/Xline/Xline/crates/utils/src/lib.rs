//! `utils`
#![deny(
    // The following are allowed by default lints according to
    // https://doc.rust-lang.org/rustc/lints/listing/allowed-by-default.html

    absolute_paths_not_starting_with_crate,
    // box_pointers, async trait must use it
    elided_lifetimes_in_paths,
    explicit_outlives_requirements,
    keyword_idents,
    macro_use_extern_crate,
    meta_variable_misuse,
    missing_abi,
    missing_copy_implementations,
    missing_debug_implementations,
    missing_docs,
    // must_not_suspend, unstable
    non_ascii_idents,
    // non_exhaustive_omitted_patterns, unstable
    noop_method_call,
    rust_2021_incompatible_closure_captures,
    rust_2021_incompatible_or_patterns,
    rust_2021_prefixes_incompatible_syntax,
    rust_2021_prelude_collisions,
    single_use_lifetimes,
    trivial_casts,
    trivial_numeric_casts,
    unreachable_pub,
    unsafe_code,
    unsafe_op_in_unsafe_fn,
    unstable_features,
    // unused_crate_dependencies, the false positive case blocks us
    unused_extern_crates,
    unused_import_braces,
    unused_lifetimes,
    unused_qualifications,
    unused_results,
    variant_size_differences,

    warnings, // treat all warns as errors

    clippy::all,
    clippy::pedantic,
    clippy::cargo,


    // The followings are selected restriction lints for rust 1.57
    clippy::as_conversions,
    clippy::clone_on_ref_ptr,
    clippy::create_dir,
    clippy::dbg_macro,
    clippy::decimal_literal_representation,
    // clippy::default_numeric_fallback, too verbose when dealing with numbers
    clippy::disallowed_script_idents,
    clippy::else_if_without_else,
    clippy::exhaustive_enums,
    clippy::exhaustive_structs,
    clippy::exit,
    clippy::expect_used,
    clippy::filetype_is_file,
    clippy::float_arithmetic,
    clippy::float_cmp_const,
    clippy::get_unwrap,
    clippy::if_then_some_else_none,
    // clippy::implicit_return, it's idiomatic Rust code.
    clippy::indexing_slicing,
    // clippy::inline_asm_x86_att_syntax, stick to intel syntax
    clippy::inline_asm_x86_intel_syntax,
    clippy::arithmetic_side_effects,
    // clippy::integer_division, required in the project
    clippy::let_underscore_must_use,
    clippy::lossy_float_literal,
    clippy::map_err_ignore,
    clippy::mem_forget,
    clippy::missing_docs_in_private_items,
    clippy::missing_enforced_import_renames,
    clippy::missing_inline_in_public_items,
    // clippy::mod_module_files, mod.rs file is used
    clippy::modulo_arithmetic,
    clippy::multiple_inherent_impl,
    clippy::panic,
    // clippy::panic_in_result_fn, not necessary as panic is banned
    clippy::pattern_type_mismatch,
    clippy::print_stderr,
    clippy::print_stdout,
    clippy::rc_buffer,
    clippy::rc_mutex,
    clippy::rest_pat_in_fully_bound_structs,
    clippy::same_name_method,
    clippy::self_named_module_files,
    // clippy::shadow_reuse, it’s a common pattern in Rust code
    // clippy::shadow_same, it’s a common pattern in Rust code
    clippy::shadow_unrelated,
    clippy::str_to_string,
    clippy::string_add,
    clippy::todo,
    clippy::unimplemented,
    clippy::unnecessary_self_imports,
    clippy::unneeded_field_pattern,
    // clippy::unreachable, allow unreachable panic, which is out of expectation
    clippy::unwrap_in_result,
    clippy::unwrap_used,
    // clippy::use_debug, debug is allow for debug log
    clippy::verbose_file_reads,
    clippy::wildcard_enum_match_arm,

    // The followings are selected lints from 1.61.0 to 1.67.1
    clippy::as_ptr_cast_mut,
    clippy::derive_partial_eq_without_eq,
    clippy::empty_drop,
    clippy::empty_structs_with_brackets,
    clippy::format_push_string,
    clippy::iter_on_empty_collections,
    clippy::iter_on_single_items,
    clippy::large_include_file,
    clippy::manual_clamp,
    clippy::suspicious_xor_used_as_pow,
    clippy::unnecessary_safety_comment,
    clippy::unnecessary_safety_doc,
    clippy::unused_peekable,
    clippy::unused_rounding,

    // The followings are selected restriction lints from rust 1.68.0 to 1.71.0
    // clippy::allow_attributes, still unstable
    clippy::impl_trait_in_params,
    clippy::let_underscore_untyped,
    clippy::missing_assert_message,
    clippy::semicolon_inside_block,
    // clippy::semicolon_outside_block, already used `semicolon_inside_block`
    clippy::tests_outside_test_module,

    // The followings are selected lints from 1.71.0 to 1.74.0
    clippy::large_stack_frames,
    clippy::tuple_array_conversions,
    clippy::pub_without_shorthand,
    clippy::needless_raw_strings,
    clippy::redundant_type_annotations,
    clippy::host_endian_bytes,
    clippy::big_endian_bytes,
    clippy::error_impl_error,
    clippy::string_lit_chars_any,
    clippy::needless_pass_by_ref_mut,
    clippy::redundant_as_str,
    clippy::missing_asserts_for_indexing,

)]
#![allow(
    clippy::multiple_crate_versions, // caused by the dependency, can't be fixed
)]
#![cfg_attr(
    test,
    allow(
        clippy::indexing_slicing,
        unused_results,
        clippy::unwrap_used,
        clippy::expect_used,
        clippy::as_conversions,
        clippy::shadow_unrelated,
        clippy::arithmetic_side_effects,
        clippy::let_underscore_untyped,
        clippy::too_many_lines,
    )
)]

use std::str::FromStr;

use tonic::transport::ClientTlsConfig;
use tonic::transport::Endpoint;

/// Barrier util
pub mod barrier;
/// configuration
pub mod config;
/// LCA tree implementation
pub mod lca_tree;
/// utils for metrics
pub mod metrics;
/// utils of `parking_lot` lock
pub mod parking_lot_lock;
/// utils for parse config
pub mod parser;
/// utils of `std` lock
#[cfg(feature = "std")]
pub mod std_lock;
/// table names
pub mod table_names;
/// task manager
pub mod task_manager;
/// utils of `tokio` lock
#[cfg(feature = "tokio")]
pub mod tokio_lock;
/// utils for pass span context
pub mod tracing;

use ::tracing::debug;
/// Interval tree implementation
pub use interval_map;
pub use parser::*;
use pbkdf2::{
    Params, Pbkdf2,
    password_hash::{PasswordHasher, SaltString, rand_core::OsRng},
};

/// display all elements for the given vector
#[macro_export]
macro_rules! write_vec {
    ($f:expr_2021, $name:expr_2021, $vector:expr_2021) => {
        write!($f, "{}: [ ", { $name })?;
        let last_idx = if $vector.is_empty() {
            0
        } else {
            $vector.len() - 1
        };
        for (idx, element) in $vector.iter().enumerate() {
            write!($f, "{}", element)?;
            if idx != last_idx {
                write!($f, ",")?;
            }
        }
        write!($f, "]")?;
    };
}

/// Get current timestamp in seconds
#[must_use]
#[inline]
pub fn timestamp() -> u64 {
    let now = std::time::SystemTime::now();
    now.duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_else(|_| unreachable!("Time went backwards"))
        .as_secs()
}

/// Create a new endpoint from addr
/// # Errors
/// Return error if addr or tls config is invalid
#[inline]
#[allow(unused_variables)]
pub fn build_endpoint(
    addr: &str,
    tls_config: Option<&ClientTlsConfig>,
) -> Result<Endpoint, tonic::transport::Error> {
    debug!(
        "connect to {addr}{}",
        if tls_config.is_some() {
            " with tls_config"
        } else {
            ""
        }
    );
    let scheme_str = addr.split_once("://").map(|(scheme, _)| scheme);
    let endpoint = match scheme_str {
        Some(_scheme) => Endpoint::from_str(addr)?,
        None => Endpoint::from_shared(format!("http://{addr}"))?,
    };
    match scheme_str {
        Some("http") | None => {}
        Some("https") => {
            let tls_config = tls_config.cloned().unwrap_or_default();
            return endpoint.tls_config(tls_config);
        }
        _ => {
            if let Some(tls_config) = tls_config {
                return endpoint.tls_config(tls_config.clone());
            }
        }
    }
    Ok(endpoint)
}

/// Hash password
///
/// # Errors
///
/// return `Error` when hash password failed
#[inline]
pub fn hash_password(password: &[u8]) -> Result<String, pbkdf2::password_hash::errors::Error> {
    let salt = SaltString::generate(&mut OsRng);
    let simple_para = Params {
        // The recommended rounds is 600,000 or more
        // [OWASP cheat sheet]: https://cheatsheetseries.owasp.org/cheatsheets/Password_Storage_Cheat_Sheet.html
        rounds: 200_000,
        output_length: 32,
    };
    let hashed_password =
        Pbkdf2.hash_password_customized(password, None, None, simple_para, &salt)?;
    Ok(hashed_password.to_string())
}

// ============================================================================
// QUIC address parsing
// ============================================================================

/// Error type for QUIC address parsing
#[derive(Debug, Clone, PartialEq, Eq)]
#[non_exhaustive]
pub enum QuicAddrError {
    /// Missing port in address
    MissingPort,
    /// Invalid port number
    InvalidPort,
    /// Invalid IPv6 format (mismatched brackets)
    InvalidIpv6,
    /// IP address without domain (when fallback is disabled)
    IpWithoutDomain,
}

impl std::fmt::Display for QuicAddrError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match *self {
            Self::MissingPort => write!(f, "missing port in address"),
            Self::InvalidPort => write!(f, "invalid port number"),
            Self::InvalidIpv6 => write!(f, "invalid IPv6 format"),
            Self::IpWithoutDomain => write!(f, "IP address requires domain name for SNI"),
        }
    }
}

impl std::error::Error for QuicAddrError {}

/// Parse QUIC address into (host, port, server_name)
///
/// # Arguments
///
/// * `addr` - Address string in format "host:port" or "quic://host:port"
/// * `allow_ip_fallback` - If true, IP addresses will use "localhost" as SNI (test only)
///
/// # Returns
///
/// Tuple of (host, port, server_name) where server_name is used for TLS SNI
///
/// # Errors
///
/// Returns error if address format is invalid or IP address is used without fallback
///
/// # Examples
///
/// ```
/// use utils::parse_quic_addr;
///
/// // Domain name
/// let (host, port, sni) = parse_quic_addr("example.com:8443", false).unwrap();
/// assert_eq!(host, "example.com");
/// assert_eq!(port, 8443);
/// assert_eq!(sni, "example.com");
///
/// // With scheme
/// let (host, port, sni) = parse_quic_addr("quic://example.com:8443", false).unwrap();
/// assert_eq!(host, "example.com");
///
/// // IPv4 with fallback (test mode)
/// let (host, port, sni) = parse_quic_addr("127.0.0.1:8443", true).unwrap();
/// assert_eq!(host, "127.0.0.1");
/// assert_eq!(sni, "localhost");
///
/// // IPv6 with fallback (test mode)
/// let (host, port, sni) = parse_quic_addr("[::1]:8443", true).unwrap();
/// assert_eq!(host, "::1");
/// assert_eq!(sni, "localhost");
/// ```
#[inline]
pub fn parse_quic_addr(
    addr: &str,
    allow_ip_fallback: bool,
) -> Result<(String, u16, String), QuicAddrError> {
    // Strip scheme if present
    let addr = addr
        .strip_prefix("quic://")
        .or_else(|| addr.strip_prefix("https://"))
        .or_else(|| addr.strip_prefix("http://"))
        .unwrap_or(addr);

    // Parse host and port
    let (host, port_str) = if addr.starts_with('[') {
        // IPv6 format: [::1]:port
        let bracket_end = addr.find(']').ok_or(QuicAddrError::InvalidIpv6)?;
        let host = &addr[1..bracket_end];
        let rest = &addr[bracket_end + 1..];
        let port_str = rest.strip_prefix(':').ok_or(QuicAddrError::MissingPort)?;
        (host.to_owned(), port_str)
    } else {
        // IPv4 or domain: host:port
        let colon_pos = addr.rfind(':').ok_or(QuicAddrError::MissingPort)?;
        let host = &addr[..colon_pos];
        let port_str = &addr[colon_pos + 1..];
        (host.to_owned(), port_str)
    };

    let port: u16 = port_str.parse().map_err(|_| QuicAddrError::InvalidPort)?;

    // Determine server_name for SNI
    let is_ip = host.parse::<std::net::IpAddr>().is_ok();
    let server_name = if is_ip {
        if allow_ip_fallback {
            debug!(
                "QUIC address {} is an IP, using 'localhost' as SNI (test mode only)",
                host
            );
            "localhost".to_owned()
        } else {
            return Err(QuicAddrError::IpWithoutDomain);
        }
    } else {
        host.clone()
    };

    Ok((host, port, server_name))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_quic_addr_domain() {
        let (host, port, sni) = parse_quic_addr("example.com:8443", false).unwrap();
        assert_eq!(host, "example.com");
        assert_eq!(port, 8443);
        assert_eq!(sni, "example.com");
    }

    #[test]
    fn test_parse_quic_addr_with_scheme() {
        let (host, port, sni) = parse_quic_addr("quic://example.com:8443", false).unwrap();
        assert_eq!(host, "example.com");
        assert_eq!(port, 8443);
        assert_eq!(sni, "example.com");
    }

    #[test]
    fn test_parse_quic_addr_ipv4_with_fallback() {
        let (host, port, sni) = parse_quic_addr("127.0.0.1:8443", true).unwrap();
        assert_eq!(host, "127.0.0.1");
        assert_eq!(port, 8443);
        assert_eq!(sni, "localhost");
    }

    #[test]
    fn test_parse_quic_addr_ipv4_without_fallback() {
        let result = parse_quic_addr("127.0.0.1:8443", false);
        assert_eq!(result, Err(QuicAddrError::IpWithoutDomain));
    }

    #[test]
    fn test_parse_quic_addr_ipv6_with_fallback() {
        let (host, port, sni) = parse_quic_addr("[::1]:8443", true).unwrap();
        assert_eq!(host, "::1");
        assert_eq!(port, 8443);
        assert_eq!(sni, "localhost");
    }

    #[test]
    fn test_parse_quic_addr_ipv6_with_scheme() {
        let (host, port, sni) = parse_quic_addr("quic://[::1]:8443", true).unwrap();
        assert_eq!(host, "::1");
        assert_eq!(port, 8443);
        assert_eq!(sni, "localhost");
    }

    #[test]
    fn test_parse_quic_addr_missing_port() {
        let result = parse_quic_addr("example.com", false);
        assert_eq!(result, Err(QuicAddrError::MissingPort));
    }

    #[test]
    fn test_parse_quic_addr_invalid_port() {
        let result = parse_quic_addr("example.com:abc", false);
        assert_eq!(result, Err(QuicAddrError::InvalidPort));
    }

    #[test]
    fn test_parse_quic_addr_invalid_ipv6() {
        let result = parse_quic_addr("[::1:8443", false);
        assert_eq!(result, Err(QuicAddrError::InvalidIpv6));
    }
}
