// (C) Copyright 2019-2020 Hewlett Packard Enterprise Development LP

use std::convert::TryFrom;

use crate::Span;
use crate::dockerfile_parser::Instruction;
use crate::error::*;
use crate::util::*;
use crate::parser::*;

/// A miscellaneous (unsupported) Dockerfile instruction.
///
/// These are instructions that aren't explicitly parsed. They may be invalid,
/// deprecated, or otherwise unsupported by this library.
///
/// Unsupported but valid commands include: `MAINTAINER`, `EXPOSE`, `VOLUME`,
/// `USER`, `WORKDIR`, `ONBUILD`, `STOPSIGNAL`, `HEALTHCHECK`, `SHELL`
#[derive(Debug, PartialEq, Eq, Clone)]
pub struct MiscInstruction {
  pub span: Span,
  pub instruction: SpannedString,
  pub arguments: BreakableString
}

impl MiscInstruction {
  pub(crate) fn from_record(record: Pair) -> Result<MiscInstruction> {
    let span = Span::from_pair(&record);
    let mut instruction = None;
    let mut arguments = None;

    for field in record.into_inner() {
      match field.as_rule() {
        Rule::misc_instruction => instruction = Some(parse_string(&field)?),
        Rule::misc_arguments => arguments = Some(parse_any_breakable(field)?),
        _ => return Err(unexpected_token(field))
      }
    }

    let instruction = instruction.ok_or_else(|| Error::GenericParseError {
      message: "generic instructions require a name".into()
    })?;

    let arguments = arguments.ok_or_else(|| Error::GenericParseError {
      message: "generic instructions require arguments".into()
    })?;

    Ok(MiscInstruction {
      span,
      instruction, arguments
    })
  }
}

impl<'a> TryFrom<&'a Instruction> for &'a MiscInstruction {
  type Error = Error;

  fn try_from(instruction: &'a Instruction) -> std::result::Result<Self, Self::Error> {
    if let Instruction::Misc(m) = instruction {
      Ok(m)
    } else {
      Err(Error::ConversionError {
        from: format!("{:?}", instruction),
        to: "MiscInstruction".into()
      })
    }
  }
}
