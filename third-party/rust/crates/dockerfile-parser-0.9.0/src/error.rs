// (C) Copyright 2019-2020 Hewlett Packard Enterprise Development LP

use pest::iterators::Pair;
use snafu::Snafu;

use crate::parser::*;

/// A Dockerfile parsing error.
#[derive(Debug, Snafu)]
#[snafu(visibility(pub(crate)))]
pub enum Error {
  #[snafu(display(
    "could not parse Dockerfile: {}", source
  ))]
  ParseError {
    source: pest::error::Error<Rule>
  },

  #[snafu(display(
    "unable to parse Dockerfile: {}", message
  ))]
  GenericParseError {
    message: String
  },

  #[snafu(display(
    "error unescaping string: {:?}", source
  ))]
  UnescapeError {
    source: enquote::Error
  },

  #[snafu(display(
    "unable to parse Dockerfile"
  ))]
  UnknownParseError,

  #[snafu(display(
    "could not read Dockerfile: {}", source
  ))]
  ReadError {
    source: std::io::Error
  },

  #[snafu(display(
    "could not convert instruction '{:?}' to desired type '{}'", from, to
  ))]
  ConversionError {
    from: String,
    to: String
  }
}

/// A Dockerfile parsing Result.
pub type Result<T, E = Error> = std::result::Result<T, E>;

/// Helper to create an unexpected token error.
pub(crate) fn unexpected_token(record: Pair<Rule>) -> Error {
  Error::GenericParseError {
    message: format!("unexpected token {:?}", record.as_rule())
  }
}
