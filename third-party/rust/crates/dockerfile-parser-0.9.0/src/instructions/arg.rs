// (C) Copyright 2019-2020 Hewlett Packard Enterprise Development LP

use std::convert::TryFrom;

use crate::dockerfile_parser::Instruction;
use crate::SpannedString;
use crate::error::*;
use crate::parse_string;
use crate::parser::{Pair, Rule};
use crate::splicer::Span;

/// A Dockerfile [`ARG` instruction][arg].
///
/// [arg]: https://docs.docker.com/engine/reference/builder/#arg
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ArgInstruction {
  pub span: Span,

  /// The argument key
  pub name: SpannedString,

  /// An optional argument value.
  ///
  /// This may be unset when passing arguments through to later stages in a
  /// [multi-stage build][build].
  ///
  /// [build]: https://docs.docker.com/develop/develop-images/multistage-build/
  pub value: Option<SpannedString>,
}

impl ArgInstruction {
  pub(crate) fn from_record(record: Pair) -> Result<ArgInstruction> {
    let span = Span::from_pair(&record);
    let mut name = None;
    let mut value = None;

    for field in record.into_inner() {
      match field.as_rule() {
        Rule::arg_name => name = Some(parse_string(&field)?),
        Rule::arg_quoted_value => value = Some(parse_string(&field)?),
        Rule::arg_value => value = Some(parse_string(&field)?),
        Rule::comment => continue,
        _ => return Err(unexpected_token(field))
      }
    }

    let name = match name {
      Some(name) => name,
      _ => return Err(Error::GenericParseError {
        message: "arg name is required".into()
      })
    };

    Ok(ArgInstruction {
      span,
      name,
      value,
    })
  }
}

impl<'a> TryFrom<&'a Instruction> for &'a ArgInstruction {
 type Error = Error;

 fn try_from(instruction: &'a Instruction) -> std::result::Result<Self, Self::Error> {
   if let Instruction::Arg(a) = instruction {
     Ok(a)
   } else {
     Err(Error::ConversionError {
       from: format!("{:?}", instruction),
       to: "ArgInstruction".into()
     })
   }
 }
}

#[cfg(test)]
mod tests {
  use pretty_assertions::assert_eq;

  use super::*;
  use crate::Dockerfile;
  use crate::test_util::*;

  #[test]
  fn arg_strings() -> Result<()> {
    assert_eq!(
      parse_single(r#"arg foo=bar"#, Rule::arg)?,
      ArgInstruction {
        span: Span::new(0, 11),
        name: SpannedString {
          span: Span::new(4, 7),
          content: "foo".into(),
        },
        value: Some(SpannedString {
          span: Span::new(8, 11),
          content: "bar".into(),
        }),
      }.into()
    );

    assert_eq!(
      parse_single(r#"arg foo="bar""#, Rule::arg)?,
      ArgInstruction {
        span: Span::new(0, 13),
        name: SpannedString {
          span: Span::new(4, 7),
          content: "foo".into(),
        },
        value: Some(SpannedString {
          span: Span::new(8, 13),
          content: "bar".into(),
        }),
      }.into()
    );

    assert_eq!(
      parse_single(r#"arg foo='bar'"#, Rule::arg)?,
      ArgInstruction {
        span: Span::new(0, 13),
        name: SpannedString {
          span: Span::new(4, 7),
          content: "foo".into(),
        },
        value: Some(SpannedString {
          span: Span::new(8, 13),
          content: "bar".into(),
        }),
      }.into()
    );

    assert!(Dockerfile::parse(r#"arg foo="bar"bar"#).is_err());
    assert!(Dockerfile::parse(r#"arg foo='bar'bar"#).is_err());

    Ok(())
  }
}
