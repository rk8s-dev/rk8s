// Copyright (C) 2021 Scott Lamb <slamb@slamb.org>
// SPDX-License-Identifier: MIT OR Apache-2.0

//! Builds and offers lookup on a table of byte values to the character
//! classes the respective bytes are part of. Most classes are referenced from
//! [RFC 7235 Appendix B: Imported ABNF](https://datatracker.ietf.org/doc/html/rfc7235#appendix-B)
//! or [RFC 7235 Appendix C: Collected ABNF](https://datatracker.ietf.org/doc/html/rfc7235#appendix-C).

pub(crate) const C_TCHAR: u8 = 1;
pub(crate) const C_QDTEXT: u8 = 2;
pub(crate) const C_ESCAPABLE: u8 = 4;
pub(crate) const C_OWS: u8 = 8;
pub(crate) const C_ATTR: u8 = 16;

static TABLE: [u8; 128] = build_table();

pub(crate) fn char_classes(b: u8) -> u8 {
    *TABLE.get(usize::from(b)).unwrap_or(&0)
}

const fn build_table() -> [u8; 128] {
    // It'd be nice to use array::from_fn here, but it wasn't stablized until Rust 1.63.
    let mut table = [0u8; 128];
    let mut i = 0;
    while i < 128 {
        let b = i as u8;
        let mut classes = 0;
        if is_tchar(b) {
            classes |= C_TCHAR;
        }
        if is_qdtext(b) {
            classes |= C_QDTEXT;
        }
        if is_escapable(b) {
            classes |= C_ESCAPABLE;
        }
        if is_ows(b) {
            classes |= C_OWS;
        }
        if is_attr(b) {
            classes |= C_ATTR;
        }
        table[i] = classes;
        i += 1;
    }
    table
}

/// Returns if the byte is a `tchar` as defined in
/// [RFC 7230 section 3.2.6](https://datatracker.ietf.org/doc/html/rfc7230#section-3.2.6).
const fn is_tchar(b: u8) -> bool {
    // tchar          = "!" / "#" / "$" / "%" / "&" / "'" / "*"
    //                / "+" / "-" / "." / "^" / "_" / "`" / "|" / "~"
    //                / DIGIT / ALPHA
    //                ; any VCHAR, except delimiters
    matches!(b,
        b'!'
        | b'#'
        | b'$'
        | b'%'
        | b'&'
        | b'\''
        | b'*'
        | b'+'
        | b'-'
        | b'.'
        | b'^'
        | b'_'
        | b'`'
        | b'|'
        | b'~'
        | b'0'..=b'9'
        | b'a'..=b'z'
        | b'A'..=b'Z')
}

/// Returns true if the byte is a valid `qdtext` (excluding `obs-text`), as defined in
/// [RFC 7230 section 3.2.6](https://datatracker.ietf.org/doc/html/rfc7230#section-3.2.6).
///
/// ```text
/// quoted-string  = DQUOTE *( qdtext / quoted-pair ) DQUOTE
/// qdtext         = HTAB / SP /%x21 / %x23-5B / %x5D-7E / obs-text
/// obs-text       = %x80-FF
/// quoted-pair    = "\" ( HTAB / SP / VCHAR / obs-text )
/// VCHAR          =  %x21-7E
///                ; visible (printing) characters
/// ```
const fn is_qdtext(b: u8) -> bool {
    matches!(b, b'\t' | b' ' | 0x21 | 0x23..=0x5B | 0x5D..=0x7E)
}

/// Returns true if the byte is a valid end of a `quoted-pair`, as defined in
/// [RFC 7230 section 3.2.6](https://datatracker.ietf.org/doc/html/rfc7230#section-3.2.6).
const fn is_escapable(b: u8) -> bool {
    matches!(b, b'\t' | b' ' | 0x21..=0x7E | 0x80..=0xFF)
}

/// Returns true if the byte is a valid `attr-char` as defined in
/// [RFC 5987 section 3.2.1](https://datatracker.ietf.org/doc/html/rfc5987#section-3.2.1).
///
/// ```text
///  attr-char     = ALPHA / DIGIT
///                / "!" / "#" / "$" / "&" / "+" / "-" / "."
///                / "^" / "_" / "`" / "|" / "~"
///                ; token except ( "*" / "'" / "%" )
/// ```
const fn is_attr(b: u8) -> bool {
    matches!(b,
        b'a'..=b'z'
        | b'A'..=b'Z'
        | b'0'..=b'9'
        | b'!'
        | b'#'
        | b'$'
        | b'&'
        | b'+'
        | b'-'
        | b'.'
        | b'^'
        | b'_'
        | b'`'
        | b'|'
        | b'~')
}

/// Returns true if the byte is valid optional whitespace as in [RFC 7230 section
/// 3.2.3](https://datatracker.ietf.org/doc/html/rfc7230#section-3.2.3).
///
/// ```text
///      OWS            = *( SP / HTAB )
///                     ; optional whitespace
/// ```
const fn is_ows(b: u8) -> bool {
    matches!(b, b' ' | b'\t')
}
