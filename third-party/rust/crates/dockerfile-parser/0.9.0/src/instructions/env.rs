// (C) Copyright 2019-2020 Hewlett Packard Enterprise Development LP

use std::convert::TryFrom;

use crate::dockerfile_parser::Instruction;
use crate::Span;
use crate::error::*;
use crate::parser::{Pair, Rule};
use crate::util::*;

use enquote::unquote;
use snafu::ResultExt;

/// An environment variable key/value pair
#[derive(Debug, PartialEq, Eq, Clone)]
pub struct EnvVar {
  pub span: Span,
  pub key: SpannedString,
  pub value: BreakableString,
}

impl EnvVar {
  pub fn new(span: Span, key: SpannedString, value: impl Into<BreakableString>) -> Self {
    EnvVar {
      span,
      key: key,
      value: value.into(),
    }
  }
}

/// A Dockerfile [`ENV` instruction][env].
///
/// [env]: https://docs.docker.com/engine/reference/builder/#env
#[derive(Debug, PartialEq, Eq, Clone)]
pub struct EnvInstruction {
  pub span: Span,
  pub vars: Vec<EnvVar>
}

/// Parses an env pair token, e.g. key=value or key="value"
fn parse_env_pair(record: Pair) -> Result<EnvVar> {
  let span = Span::from_pair(&record);
  let mut key = None;
  let mut value = None;

  for field in record.into_inner() {
    match field.as_rule() {
      Rule::env_name => key = Some(parse_string(&field)?),
      Rule::env_pair_value => {
        value = Some(
          BreakableString::new(&field).add_string(&field, field.as_str())
        );
      },
      Rule::env_pair_quoted_value => {
        let v = unquote(field.as_str()).context(UnescapeError)?;

        value = Some(
          BreakableString::new(&field).add_string(&field, v)
        );
      },
      _ => return Err(unexpected_token(field))
    }
  }

  let key = key.ok_or_else(|| Error::GenericParseError {
    message: "env pair requires a key".into()
  })?;

  let value = value.ok_or_else(|| Error::GenericParseError {
    message: "env pair requires a value".into()
  })?;

  Ok(EnvVar {
    span,
    key,
    value,
  })
}

impl EnvInstruction {
  pub(crate) fn from_record(record: Pair) -> Result<EnvInstruction> {
    let span = Span::from_pair(&record);
    let field = record.into_inner().next().unwrap();

    match field.as_rule() {
      Rule::env_single => EnvInstruction::from_single_record(span, field),
      Rule::env_pairs => EnvInstruction::from_pairs_record(span, field),
      _ => Err(unexpected_token(field)),
    }
  }

  fn from_pairs_record(span: Span, record: Pair) -> Result<EnvInstruction> {
    let mut vars = Vec::new();

    for field in record.into_inner() {
      match field.as_rule() {
        Rule::env_pair => vars.push(parse_env_pair(field)?),
        Rule::comment => continue,
        _ => return Err(unexpected_token(field))
      }
    }

    Ok(EnvInstruction {
      span,
      vars,
    })
  }

  fn from_single_record(span: Span, record: Pair) -> Result<EnvInstruction> {
    let mut key = None;
    let mut value = None;

    for field in record.into_inner() {
      match field.as_rule() {
        Rule::env_name => key = Some(parse_string(&field)?),
        Rule::env_single_value => value = Some(parse_any_breakable(field)?),
        Rule::env_single_quoted_value => {
          let v = unquote(field.as_str()).context(UnescapeError)?;

          value = Some(
            BreakableString::new(&field).add_string(&field, v)
          );
        },
        Rule::comment => continue,
        _ => return Err(unexpected_token(field))
      }
    }

    let key = key.ok_or_else(|| Error::GenericParseError {
      message: "env requires a key".into()
    })?;

    let value = value.ok_or_else(|| Error::GenericParseError {
        message: "env requires a value".into()
    })?;

    Ok(EnvInstruction {
      span,
      vars: vec![EnvVar {
        span: Span::new(key.span.start, value.span.end),
        key,
        value,
      }],
    })
  }
}

impl<'a> TryFrom<&'a Instruction> for &'a EnvInstruction {
  type Error = Error;

  fn try_from(instruction: &'a Instruction) -> std::result::Result<Self, Self::Error> {
    if let Instruction::Env(e) = instruction {
      Ok(e)
    } else {
      Err(Error::ConversionError {
        from: format!("{:?}", instruction),
        to: "EnvInstruction".into()
      })
    }
  }
}

#[cfg(test)]
mod tests {
  use indoc::indoc;
  use pretty_assertions::assert_eq;

  use super::*;
  use crate::Dockerfile;
  use crate::test_util::*;

  #[test]
  fn env() -> Result<()> {
    assert_eq!(
      parse_single(r#"env foo=bar"#, Rule::env)?.into_env().unwrap(),
      EnvInstruction {
        span: Span::new(0, 11),
        vars: vec![EnvVar::new(
          Span::new(4, 11),
          SpannedString {
            span: Span::new(4, 7),
            content: "foo".to_string(),
          },
          ((8, 11), "bar"),
        )],
      }
    );

    assert_eq!(
      parse_single(r#"env FOO_BAR="baz""#, Rule::env)?,
      EnvInstruction {
        span: Span::new(0, 17),
        vars: vec![EnvVar::new(
          Span::new(4, 17),
          SpannedString {
            span: Span::new(4, 11),
            content: "FOO_BAR".to_string(),
          },
          ((12, 17), "baz"),
        )],
      }.into()
    );

    assert_eq!(
      parse_single(r#"env FOO_BAR "baz""#, Rule::env)?,
      EnvInstruction {
        span: Span::new(0, 17),
        vars: vec![EnvVar::new(
          Span::new(4, 17),
          SpannedString {
            span: Span::new(4, 11),
            content: "FOO_BAR".to_string(),
          },
          ((12, 17), "baz")),
        ],
      }.into()
    );

    assert_eq!(
      parse_single(r#"env foo="bar\"baz""#, Rule::env)?,
      EnvInstruction {
        span: Span::new(0, 18),
        vars: vec![EnvVar::new(
          Span::new(4, 18),
          SpannedString {
            span: Span::new(4, 7),
            content: "foo".to_string(),
          },
          ((8, 18), "bar\"baz"),
        )],
      }.into()
    );

    assert_eq!(
      parse_single(r#"env foo='bar'"#, Rule::env)?,
      EnvInstruction {
        span: Span::new(0, 13),
        vars: vec![EnvVar::new(
          Span::new(4, 13),
          SpannedString {
            span: Span::new(4, 7),
            content: "foo".to_string(),
          },
          ((8, 13), "bar"),
        )],
      }.into()
    );

    assert_eq!(
      parse_single(r#"env foo='bar\'baz'"#, Rule::env)?,
      EnvInstruction {
        span: Span::new(0, 18),
        vars: vec![EnvVar::new(
          Span::new(4, 18),
          SpannedString {
            span: Span::new(4, 7),
            content: "foo".to_string(),
          },
          ((8, 18), "bar'baz"),
        )],
      }.into()
    );

    assert_eq!(
      parse_single(r#"env foo="123" bar='456' baz=789"#, Rule::env)?,
      EnvInstruction {
        span: Span::new(0, 31),
        vars: vec![
          EnvVar::new(
            Span::new(4, 13),
            SpannedString {
              span: Span::new(4, 7),
              content: "foo".to_string(),
            },
            ((8, 13), "123")
          ),
          EnvVar::new(
            Span::new(14, 23),
            SpannedString {
              span: Span::new(14, 17),
              content: "bar".to_string(),
            },
            ((18, 23), "456")
          ),
          EnvVar::new(
            Span::new(24, 31),
            SpannedString {
              span: Span::new(24, 27),
              content: "baz".to_string(),
            },
            ((28, 31), "789")
          ),
        ],
      }.into()
    );

    assert!(Dockerfile::parse(r#"env foo="bar"bar"#).is_err());
    assert!(Dockerfile::parse(r#"env foo='bar'bar"#).is_err());

    Ok(())
  }

  #[test]
  fn test_multiline_pairs() -> Result<()> {
    // note: docker allows empty line continuations (but may print a warning)
    assert_eq!(
      parse_single(
        indoc!(r#"
          env foo=a \
            bar=b \
            baz=c \

        "#),
        Rule::env
      )?.into_env().unwrap().vars,
      vec![
        EnvVar::new(
          Span::new(4, 9),
          SpannedString {
            span: Span::new(4, 7),
            content: "foo".to_string(),
          },
          ((8, 9), "a")
        ),
        EnvVar::new(
          Span::new(14, 19),
          SpannedString {
            span: Span::new(14, 17),
            content: "bar".to_string(),
          },
          ((18, 19), "b")
        ),
        EnvVar::new(
          Span::new(24, 29),
          SpannedString {
            span: Span::new(24, 27),
            content: "baz".to_string(),
          },
          ((28, 29), "c")
        )
      ]
    );

    Ok(())
  }

  #[test]
  fn test_multiline_single_env() -> Result<()> {
    assert_eq!(
      parse_single(
        indoc!(r#"
          env foo Lorem ipsum dolor sit amet, \
            consectetur adipiscing elit, \
            sed do eiusmod tempor incididunt ut \
            labore et dolore magna aliqua.
        "#),
        Rule::env
      )?.into_env().unwrap().vars,
      vec![
        EnvVar::new(
          Span::new(4, 143),
          SpannedString {
            span: Span::new(4, 7),
            content: "foo".to_string(),
          },
          BreakableString::new((8, 143))
            .add_string((8, 36), "Lorem ipsum dolor sit amet, ")
            .add_string((38, 69), "  consectetur adipiscing elit, ")
            .add_string((71, 109), "  sed do eiusmod tempor incididunt ut ")
            .add_string((111, 143), "  labore et dolore magna aliqua.")
        )
      ]
    );

    // note: maybe a small bug here, leading whitespace on the first value line
    // is eaten (this will hopefully never matter...)
    assert_eq!(
      parse_single(
        indoc!(r#"
          env \
            foo \
            Lorem ipsum dolor sit amet, \
            consectetur adipiscing elit
        "#),
        Rule::env
      )?.into_env().unwrap().vars,
      vec![
        EnvVar::new(
          Span::new(8, 75),
          SpannedString {
            span: Span::new(8, 11),
            content: "foo".to_string(),
          },
          BreakableString::new((16, 75))
            .add_string((16, 44), "Lorem ipsum dolor sit amet, ")
            .add_string((46, 75), "  consectetur adipiscing elit")
        )
      ]
    );

    assert_eq!(
      parse_single(
        indoc!(r#"
          env \
            foo \
            # bar
            Lorem ipsum dolor sit amet, \
            # baz
            consectetur adipiscing elit
        "#),
        Rule::env
      )?.into_env().unwrap().vars,
      vec![
        EnvVar::new(
          Span::new(8, 91),
          SpannedString {
            span: Span::new(8, 11),
            content: "foo".to_string(),
          },
          BreakableString::new((24, 91))
            .add_string((24, 52), "Lorem ipsum dolor sit amet, ")
            .add_comment((56, 61), "# baz")
            .add_string((62, 91), "  consectetur adipiscing elit")
        )
      ]
    );

    Ok(())
  }
}
