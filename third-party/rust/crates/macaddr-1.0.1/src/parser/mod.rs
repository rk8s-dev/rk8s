#[cfg(feature = "std")]
use std::{error::Error, fmt};

#[cfg(not(feature = "std"))]
use core::fmt;

use crate::{MacAddr, MacAddr6, MacAddr8};

/// An error which can be returned when parsing MAC address.
///
/// This error is used as the error type for the `FromStr` implementation
/// for [MacAddr6] and [MacAddr8].
///
/// [MacAddr6]: ./struct.MacAddr6.html
/// [MacAddr8]: ./struct.MacAddr8.html
#[derive(Debug, Hash, Eq, PartialEq, Ord, PartialOrd, Copy, Clone)]
pub enum ParseError {
    /// Provided string can't be parsed into the given type,
    /// because it is either too short or too long.
    ///
    /// For example, any trailing symbols will result in the error,
    /// as in `"12-34-56-78-9A-BC\n"`.
    ///
    /// This enum member will contain the provided string length when returned.
    InvalidLength(usize),

    /// Invalid character occurred in the provided string.
    ///
    /// Allowed characters are `0123456789abcdefABCDEF-:.`.
    ///
    /// This enum member will contain the wrong char and it's position when returned.
    InvalidCharacter(char, usize),
}

impl fmt::Display for ParseError {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        match self {
            ParseError::InvalidLength(len) => f.write_fmt(format_args!("Invalid length of {} characters", len,)),
            ParseError::InvalidCharacter(chr, pos) => {
                f.write_fmt(format_args!("Unexpected character '{}' at position {}", chr, pos,))
            }
        }
    }
}

#[cfg(feature = "std")]
impl Error for ParseError {}

#[derive(Debug, Eq, PartialEq)]
enum Delimiter {
    Hyphen,
    Colon,
    Dot,
}

// Heavily based on the Rust' `std/net/parser.rs` sources.
#[derive(Debug)]
pub struct Parser<'a> {
    source: &'a [u8],
    pos: usize,
    delimiter: Option<Delimiter>,
}

impl<'a> Parser<'a> {
    pub fn new(s: &'a str) -> Parser<'a> {
        Parser {
            source: s.as_bytes(),
            pos: 0,
            delimiter: None,
        }
    }

    fn is_eof(&self) -> bool {
        self.pos == self.source.len()
    }

    fn move_next(&mut self) {
        if !self.is_eof() {
            self.pos += 1;
        }
    }

    fn peek_char(&mut self) -> Option<char> {
        if self.is_eof() {
            None
        } else {
            Some(self.source[self.pos] as char)
        }
    }

    fn read_char(&mut self) -> Result<char, ParseError> {
        if self.is_eof() {
            Err(ParseError::InvalidLength(self.pos))
        } else {
            let r = self.source[self.pos] as char;
            self.pos += 1;
            Ok(r)
        }
    }

    fn read_digit(&mut self) -> Result<u8, ParseError> {
        let chr = self.read_char()?;

        match chr as u8 {
            byte @ b'0'..=b'9' => Ok(byte - b'0'),
            byte @ b'a'..=b'f' => Ok(byte - b'a' + 10),
            byte @ b'A'..=b'F' => Ok(byte - b'A' + 10),
            _ => Err(ParseError::InvalidCharacter(chr, self.pos)),
        }
    }

    fn probe_delimiter(&mut self) -> Result<Option<()>, ParseError> {
        match self.peek_char() {
            Some('-') if self.delimiter.is_none() => {
                self.delimiter = Some(Delimiter::Hyphen);
                Ok(Some(()))
            }
            Some('-') if self.delimiter != Some(Delimiter::Hyphen) => Err(ParseError::InvalidCharacter('-', self.pos)),
            Some('-') => Ok(Some(())),

            Some(':') if self.delimiter.is_none() => {
                self.delimiter = Some(Delimiter::Colon);
                Ok(Some(()))
            }
            Some(':') if self.delimiter != Some(Delimiter::Colon) => Err(ParseError::InvalidCharacter(':', self.pos)),
            Some(':') => Ok(Some(())),

            Some('.') if self.delimiter.is_none() => {
                self.delimiter = Some(Delimiter::Dot);
                Ok(Some(()))
            }
            Some('.') if self.delimiter != Some(Delimiter::Dot) => Err(ParseError::InvalidCharacter('.', self.pos)),
            Some('.') => Ok(Some(())),
            _ => Ok(None),
        }
    }

    pub fn read_v6_addr(&mut self) -> Result<MacAddr6, ParseError> {
        let mut bytes = [0; 6];
        let mut i = 0;

        while i < 6 {
            if self.probe_delimiter()?.is_some() {
                self.move_next();
            }

            let mut digit = self.read_digit()? * 16;
            digit += self.read_digit()?;

            bytes[i] = digit;

            i += 1;
        }

        if self.is_eof() {
            Ok(MacAddr6::from(bytes))
        } else {
            Err(ParseError::InvalidLength(self.source.len()))
        }
    }

    pub fn read_v8_addr(&mut self) -> Result<MacAddr8, ParseError> {
        let mut bytes = [0; 8];
        let mut i = 0;

        while i < 8 {
            if self.probe_delimiter()?.is_some() {
                self.move_next();
            }

            let mut digit = self.read_digit()? * 16;
            digit += self.read_digit()?;

            bytes[i] = digit;

            i += 1;
        }

        if self.is_eof() {
            Ok(MacAddr8::from(bytes))
        } else {
            Err(ParseError::InvalidLength(self.source.len()))
        }
    }

    pub fn read_addr(&mut self) -> Result<MacAddr, ParseError> {
        match self.read_v6_addr() {
            Ok(addr) => return Ok(addr.into()),
            Err(err @ ParseError::InvalidCharacter(..)) => return Err(err),
            Err(ParseError::InvalidLength(..)) => {}
        }

        // Rolling back to the start.
        self.pos = 0;

        self.read_v8_addr().map(Into::into)
    }
}

#[cfg(test)]
mod tests;
