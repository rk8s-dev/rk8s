// (C) Copyright 2019 Hewlett Packard Enterprise Development LP

use std::convert::TryFrom;

use pest::Parser;
use snafu::ResultExt;

use crate::dockerfile_parser::Instruction;
use crate::error::*;
use crate::parser::{DockerfileParser, Pair, Rule};

/// Parses a string into a single instruction using a particular syntax rule.
///
/// This is technically over-constrained as we could just parse any single
/// instruction using `Rule::step`, however doing so isn't ideal for
/// per-instruction unit tests.
pub fn parse_single(input: &str, rule: Rule) -> Result<Instruction> {
  let record = DockerfileParser::parse(rule, input)
    .context(ParseError)?
    .next()
    .ok_or(Error::UnknownParseError)?;

  Instruction::try_from(record)
}

pub fn parse_direct<T, F>(input: &str, rule: Rule, func: F) -> Result<T>
where
  F: Fn(Pair) -> Result<T>
{
  let pair = DockerfileParser::parse(rule, input)
    .context(ParseError)?
    .next()
    .ok_or(Error::UnknownParseError)?;

  func(pair)
}
