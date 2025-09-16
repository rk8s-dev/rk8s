#[cfg(feature = "std")]
use std::str::FromStr;

#[cfg(not(feature = "std"))]
use core::str::FromStr;

use assert_matches::assert_matches;

use crate::{MacAddr, MacAddr6, MacAddr8};

#[test]
fn test_parse_v6_upper_case_canonical_format() {
    let addr = MacAddr6::from_str("12-34-56-78-9A-BC");

    assert!(addr.is_ok());
    let addr = addr.unwrap();

    assert_eq!(&[0x12, 0x34, 0x56, 0x78, 0x9A, 0xBC], addr.as_bytes());
}

#[test]
fn test_parse_v6_lower_case_canonical_format() {
    let addr = MacAddr6::from_str("ab-cd-ef-ab-cd-ef");

    assert!(addr.is_ok());
    let addr = addr.unwrap();

    assert_eq!(&[0xAB, 0xCD, 0xEF, 0xAB, 0xCD, 0xEF], addr.as_bytes());
}

#[test]
fn test_parse_v6_mixed_case_canonical_format() {
    let addr = MacAddr6::from_str("AB-cd-Ef-Ab-cD-EF");

    assert!(addr.is_ok());
    let addr = addr.unwrap();

    assert_eq!(&[0xAB, 0xCD, 0xEF, 0xAB, 0xCD, 0xEF], addr.as_bytes());
}

#[test]
fn test_parse_v6_colon_format() {
    let addr = MacAddr6::from_str("12:34:56:78:9A:BC");

    assert!(addr.is_ok());
    let addr = addr.unwrap();

    assert_eq!(&[0x12, 0x34, 0x56, 0x78, 0x9A, 0xBC], addr.as_bytes());
}

#[test]
fn test_parse_v6_cisco_format() {
    let addr = MacAddr6::from_str("1234.5678.9ABC");

    assert!(addr.is_ok());
    let addr = addr.unwrap();

    assert_eq!(&[0x12, 0x34, 0x56, 0x78, 0x9A, 0xBC], addr.as_bytes());
}

#[test]
fn test_parse_v8_canonical_format() {
    let addr = MacAddr8::from_str("12-34-56-78-9A-BC-DE-F0");

    assert!(addr.is_ok());
    let addr = addr.unwrap();

    assert_eq!(&[0x12, 0x34, 0x56, 0x78, 0x9A, 0xBC, 0xDE, 0xF0], addr.as_bytes());
}

#[test]
fn test_parse_v8_colon_format() {
    let addr = MacAddr8::from_str("12:34:56:78:9A:BC:DE:F0");

    assert!(addr.is_ok());
    let addr = addr.unwrap();

    assert_eq!(&[0x12, 0x34, 0x56, 0x78, 0x9A, 0xBC, 0xDE, 0xF0], addr.as_bytes());
}

#[test]
fn test_parse_canonical_format() {
    let addr = MacAddr::from_str("12-34-56-78-9A-BC-DE-F0");

    assert!(addr.is_ok());
    let addr = addr.unwrap();
    assert_matches!(addr, MacAddr::V8(..));
    assert_eq!(&[0x12, 0x34, 0x56, 0x78, 0x9A, 0xBC, 0xDE, 0xF0], addr.as_bytes());
}

#[test]
fn test_parse_colon_format() {
    let addr = MacAddr::from_str("12:34:56:78:9A:BC:DE:F0");

    assert!(addr.is_ok());
    let addr = addr.unwrap();
    assert_matches!(addr, MacAddr::V8(..));
    assert_eq!(&[0x12, 0x34, 0x56, 0x78, 0x9A, 0xBC, 0xDE, 0xF0], addr.as_bytes());
}

#[test]
fn test_parse_v6_empty() {
    let addr = MacAddr6::from_str("");

    assert!(addr.is_err());
}

#[test]
fn test_parse_v8_empty() {
    let addr = MacAddr8::from_str("");

    assert!(addr.is_err());
}

#[test]
fn test_parse_empty() {
    let addr = MacAddr::from_str("");

    assert!(addr.is_err());
}

#[test]
fn test_parse_v6_partial_start() {
    let addr = MacAddr6::from_str("b-cd-ef-12-34-56");

    assert!(addr.is_err());
}

#[test]
fn test_parse_v8_partial_start() {
    let addr = MacAddr8::from_str("b-cd-ef-12-34-56-78-9A");

    assert!(addr.is_err());
}

#[test]
fn test_parse_v6_partial_end() {
    let addr = MacAddr6::from_str("ab-cd-ef-12-34-5");

    assert!(addr.is_err());
}

#[test]
fn test_parse_v8_partial_end() {
    let addr = MacAddr8::from_str("ab-cd-ef-12-34-56-78-9");

    assert!(addr.is_err());
}

#[test]
fn test_parse_v6_invalid_char() {
    let addr = MacAddr6::from_str("ab-Qd-ef-12-34-56");

    assert!(addr.is_err());
}

#[test]
fn test_parse_v8_invalid_char() {
    let addr = MacAddr8::from_str("ab-Qd-ef-12-34-56-78-9A");

    assert!(addr.is_err());
}

#[test]
fn test_parse_v6_different_delimiters() {
    let addr = MacAddr6::from_str("ab-cd:ef-12-34-56");

    assert!(addr.is_err());
}

#[test]
fn test_parse_v8_different_delimiters() {
    let addr = MacAddr8::from_str("ab-cd-ef-12-34-56-78:9A");

    assert!(addr.is_err());
}
