//! This Rust library quotes, unquotes, and unescapes strings.
//!
//! # Examples
//! ```
//! extern crate enquote;
//!
//! fn main() {
//!     assert_eq!(enquote::enquote('\'', "foo'bar"), "'foo\\'bar'");
//!     assert_eq!(enquote::unquote("'foo\\'bar\\n'").unwrap(), "foo'bar\n");
//!     assert_eq!(enquote::unescape("\\n", None).unwrap(), "\n");
//! }
//! ```

mod error;

pub use error::Error;

/// Enquotes `s` with `quote`.
pub fn enquote(quote: char, s: &str) -> String {
    // escapes any `quote` in `s`
    let escaped = s
        .chars()
        .map(|c| match c {
            // escapes the character if it's the quote
            _ if c == quote => format!("\\{}", quote),
            // escapes backslashes
            '\\' => "\\\\".into(),
            // no escape required
            _ => c.to_string(),
        })
        .collect::<String>();

    // enquotes escaped string
    quote.to_string() + &escaped + &quote.to_string()
}

/// Unquotes `s`.
pub fn unquote(s: &str) -> Result<String, Error> {
    if s.chars().count() < 2 {
        return Err(Error::NotEnoughChars);
    }

    let quote = s.chars().next().unwrap();

    if quote != '"' && quote != '\'' && quote != '`' {
        return Err(Error::UnrecognizedQuote);
    }

    if s.chars().last().unwrap() != quote {
        return Err(Error::UnexpectedEOF);
    }

    // removes quote characters
    // the sanity checks performed above ensure that the quotes will be ASCII and this will not
    // panic
    let s = &s[1..s.len() - 1];

    unescape(s, Some(quote))
}

/// Returns `s` after processing escapes such as `\n` and `\x00`.
pub fn unescape(s: &str, illegal: Option<char>) -> Result<String, Error> {
    let mut chars = s.chars();
    let mut unescaped = String::new();
    loop {
        match chars.next() {
            None => break,
            Some(c) => unescaped.push(match c {
                _ if Some(c) == illegal => return Err(Error::IllegalChar),
                '\\' => match chars.next() {
                    None => return Err(Error::UnexpectedEOF),
                    Some(c) => match c {
                        _ if c == '\\' || c == '"' || c == '\'' || c == '`' => c,
                        'a' => '\x07',
                        'b' => '\x08',
                        'f' => '\x0c',
                        'n' => '\n',
                        'r' => '\r',
                        't' => '\t',
                        'v' => '\x0b',
                        // octal
                        '0'..='9' => {
                            let octal = c.to_string() + &take(&mut chars, 2);
                            u8::from_str_radix(&octal, 8).map_err(|_| Error::UnrecognizedEscape)?
                                as char
                        }
                        // hex
                        'x' => {
                            let hex = take(&mut chars, 2);
                            u8::from_str_radix(&hex, 16).map_err(|_| Error::UnrecognizedEscape)?
                                as char
                        }
                        // unicode
                        'u' => decode_unicode(&take(&mut chars, 4))?,
                        'U' => decode_unicode(&take(&mut chars, 8))?,
                        _ => return Err(Error::UnrecognizedEscape),
                    },
                },
                _ => c,
            }),
        }
    }

    Ok(unescaped)
}

#[inline]
// Iterator#take cannot be used because it consumes the iterator
fn take<I: Iterator<Item = char>>(iterator: &mut I, n: usize) -> String {
    let mut s = String::with_capacity(n);
    for _ in 0..n {
        s.push(iterator.next().unwrap_or_default());
    }
    s
}

fn decode_unicode(code_point: &str) -> Result<char, Error> {
    match u32::from_str_radix(code_point, 16) {
        Err(_) => return Err(Error::UnrecognizedEscape),
        Ok(n) => std::char::from_u32(n).ok_or(Error::InvalidUnicode),
    }
}
