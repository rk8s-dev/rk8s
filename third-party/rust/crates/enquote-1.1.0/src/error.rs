use thiserror::Error;

#[derive(Error, Debug, PartialEq)]
pub enum Error {
    #[error("not enough chars")]
    NotEnoughChars,
    #[error("unrecognized quote character")]
    UnrecognizedQuote,
    #[error("unexpected eof")]
    UnexpectedEOF,
    #[error("illegal character")]
    IllegalChar,
    #[error("unrecognized escape sequence")]
    UnrecognizedEscape,
    #[error("invalid unicode code point")]
    InvalidUnicode,
}
