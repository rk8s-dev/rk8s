use crate::{ParseError, err::{perr, ParseErrorKind::*}, parse::{hex_digit_value, check_suffix}};


/// Must start with `\`. Returns the unscaped value as `E` and the number of
/// input bytes the escape is long.
///
/// `unicode` and `byte_escapes` specify which types of escapes are
/// supported. [Quote escapes] are always unescaped, [Unicode escapes] only if
/// `unicode` is true. If `byte_escapes` is false, [ASCII escapes] are
/// used, if it's true, [Byte escapes] are (the only difference being that the
/// latter supports \xHH escapes > 0x7f).
///
/// [Quote escapes]: https://doc.rust-lang.org/reference/tokens.html#quote-escapes
/// [Unicode escapes]: https://doc.rust-lang.org/reference/tokens.html#unicode-escapes
/// [Ascii escapes]: https://doc.rust-lang.org/reference/tokens.html#ascii-escapes
/// [Byte escapes]: https://doc.rust-lang.org/reference/tokens.html#byte-escapes
pub(crate) fn unescape<E: Escapee>(
    input: &str,
    offset: usize,
    unicode: bool,
    byte_escapes: bool,
) -> Result<(E, usize), ParseError> {
    let first = input.as_bytes().get(1)
        .ok_or(perr(offset, UnterminatedEscape))?;
    let out = match first {
        // Quote escapes
        b'\'' => (E::from_byte(b'\''), 2),
        b'"' => (E::from_byte(b'"'), 2),

        // Ascii escapes
        b'n' => (E::from_byte(b'\n'), 2),
        b'r' => (E::from_byte(b'\r'), 2),
        b't' => (E::from_byte(b'\t'), 2),
        b'\\' => (E::from_byte(b'\\'), 2),
        b'0' => (E::from_byte(b'\0'), 2),
        b'x' => {
            let hex_string = input.get(2..4)
                .ok_or(perr(offset..offset + input.len(), UnterminatedEscape))?
                .as_bytes();
            let first = hex_digit_value(hex_string[0])
                .ok_or(perr(offset..offset + 4, InvalidXEscape))?;
            let second = hex_digit_value(hex_string[1])
                .ok_or(perr(offset..offset + 4, InvalidXEscape))?;
            let value = second + 16 * first;

            if !byte_escapes && value > 0x7F {
                return Err(perr(offset..offset + 4, NonAsciiXEscape));
            }

            (E::from_byte(value), 4)
        },

        // Unicode escape
        b'u' => {
            if !unicode {
                return Err(perr(offset..offset + 2, UnicodeEscapeInByteLiteral));
            }

            if input.as_bytes().get(2) != Some(&b'{') {
                return Err(perr(offset..offset + 2, UnicodeEscapeWithoutBrace));
            }

            let closing_pos = input.bytes().position(|b| b == b'}')
                .ok_or(perr(offset..offset + input.len(), UnterminatedUnicodeEscape))?;

            let inner = &input[3..closing_pos];
            if inner.as_bytes().first() == Some(&b'_') {
                return Err(perr(4, InvalidStartOfUnicodeEscape));
            }

            let mut v: u32 = 0;
            let mut digit_count = 0;
            for (i, b) in inner.bytes().enumerate() {
                if b == b'_'{
                    continue;
                }

                let digit = hex_digit_value(b)
                    .ok_or(perr(offset + 3 + i, NonHexDigitInUnicodeEscape))?;

                if digit_count == 6 {
                    return Err(perr(offset + 3 + i, TooManyDigitInUnicodeEscape));
                }
                digit_count += 1;
                v = 16 * v + digit as u32;
            }

            let c = std::char::from_u32(v)
                .ok_or(perr(offset..offset + closing_pos + 1, InvalidUnicodeEscapeChar))?;

            (E::from_char(c), closing_pos + 1)
        }

        _ => return Err(perr(offset..offset + 2, UnknownEscape)),
    };

    Ok(out)
}

pub(crate) trait Escapee: Sized {
    type Container: EscapeeContainer<Self>;
    fn from_byte(b: u8) -> Self;
    fn from_char(c: char) -> Self;
}

impl Escapee for u8 {
    type Container = Vec<u8>;
    fn from_byte(b: u8) -> Self {
        b
    }
    fn from_char(_: char) -> Self {
        panic!("bug: `<u8 as Escapee>::from_char` was called");
    }
}

impl Escapee for char {
    type Container = String;
    fn from_byte(b: u8) -> Self {
        b.into()
    }
    fn from_char(c: char) -> Self {
        c
    }
}

pub(crate) trait EscapeeContainer<E: Escapee> {
    fn new() -> Self;
    fn is_empty(&self) -> bool;
    fn push(&mut self, v: E);
    fn push_str(&mut self, s: &str);
}

impl EscapeeContainer<u8> for Vec<u8> {
    fn new() -> Self { Self::new() }
    fn is_empty(&self) -> bool { self.is_empty() }
    fn push(&mut self, v: u8) { self.push(v); }
    fn push_str(&mut self, s: &str) { self.extend_from_slice(s.as_bytes()); }
}

impl EscapeeContainer<char> for String {
    fn new() -> Self { Self::new() }
    fn is_empty(&self) -> bool { self.is_empty() }
    fn push(&mut self, v: char) { self.push(v); }
    fn push_str(&mut self, s: &str) { self.push_str(s); }
}


/// Checks whether the character is skipped after a string continue start
/// (unescaped backlash followed by `\n`).
fn is_string_continue_skipable_whitespace(b: u8) -> bool {
    b == b' ' || b == b'\t' || b == b'\n'
}

/// Unescapes a whole string or byte string.
#[inline(never)]
pub(crate) fn unescape_string<E: Escapee>(
    input: &str,
    offset: usize,
    unicode: bool,
    byte_escapes: bool,
) -> Result<(Option<E::Container>, usize), ParseError> {
    let mut closing_quote_pos = None;
    let mut i = offset;
    let mut end_last_escape = offset;
    let mut value = <E::Container>::new();
    while i < input.len() {
        match input.as_bytes()[i] {
            // Handle "string continue".
            b'\\' if input.as_bytes().get(i + 1) == Some(&b'\n') => {
                value.push_str(&input[end_last_escape..i]);

                // Find the first non-whitespace character.
                let end_escape = input[i + 2..].bytes()
                    .position(|b| !is_string_continue_skipable_whitespace(b))
                    .ok_or(perr(None, UnterminatedString))?;

                i += 2 + end_escape;
                end_last_escape = i;
            }
            b'\\' => {
                let rest = &input[i..input.len() - 1];
                let (c, len) = unescape::<E>(rest, i, unicode, byte_escapes)?;
                value.push_str(&input[end_last_escape..i]);
                value.push(c);
                i += len;
                end_last_escape = i;
            }
            b'\r' => return Err(perr(i, CarriageReturn)),
            b'"' => {
                closing_quote_pos = Some(i);
                break;
            },
            b if !unicode && !b.is_ascii() => return Err(perr(i, NonAsciiInByteLiteral)),
            _ => i += 1,
        }
    }

    let closing_quote_pos = closing_quote_pos.ok_or(perr(None, UnterminatedString))?;

    let start_suffix = closing_quote_pos + 1;
    let suffix = &input[start_suffix..];
    check_suffix(suffix).map_err(|kind| perr(start_suffix, kind))?;

    // `value` is only empty if there was no escape in the input string
    // (with the special case of the input being empty). This means the
    // string value basically equals the input, so we store `None`.
    let value = if value.is_empty() {
        None
    } else {
        // There was an escape in the string, so we need to push the
        // remaining unescaped part of the string still.
        value.push_str(&input[end_last_escape..closing_quote_pos]);
        Some(value)
    };

    Ok((value, start_suffix))
}

/// Reads and checks a raw (byte) string literal. Returns the number of hashes
/// and the index when the suffix starts.
#[inline(never)]
pub(crate) fn scan_raw_string<E: Escapee>(
    input: &str,
    offset: usize,
    unicode: bool,
) -> Result<(u32, usize), ParseError> {
    // Raw string literal
    let num_hashes = input[offset..].bytes().position(|b| b != b'#')
        .ok_or(perr(None, InvalidLiteral))?;

    if input.as_bytes().get(offset + num_hashes) != Some(&b'"') {
        return Err(perr(None, InvalidLiteral));
    }
    let start_inner = offset + num_hashes + 1;
    let hashes = &input[offset..num_hashes + offset];

    let mut closing_quote_pos = None;
    let mut i = start_inner;
    while i < input.len() {
        let b = input.as_bytes()[i];
        if b == b'"' && input[i + 1..].starts_with(hashes) {
            closing_quote_pos = Some(i);
            break;
        }

        // CR are just always disallowed in all (raw) strings. Rust performs
        // a normalization of CR LF to just LF in a pass prior to lexing. But
        // in lexing, it's disallowed.
        if b == b'\r' {
            return Err(perr(i, CarriageReturn));
        }

        if !unicode {
            if !b.is_ascii() {
                return Err(perr(i, NonAsciiInByteLiteral));
            }
        }

        i += 1;
    }

    let closing_quote_pos = closing_quote_pos.ok_or(perr(None, UnterminatedRawString))?;

    let start_suffix = closing_quote_pos + num_hashes + 1;
    let suffix = &input[start_suffix..];
    check_suffix(suffix).map_err(|kind| perr(start_suffix, kind))?;

    Ok((num_hashes as u32, start_suffix))
}
