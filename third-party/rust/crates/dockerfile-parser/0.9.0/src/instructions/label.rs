// (C) Copyright 2019-2020 Hewlett Packard Enterprise Development LP

use std::convert::TryFrom;

use crate::dockerfile_parser::Instruction;
use crate::parser::{Pair, Rule};
use crate::Span;
use crate::util::*;
use crate::error::*;

use enquote::unquote;
use snafu::ResultExt;

/// A single label key/value pair.
#[derive(Debug, PartialEq, Eq, Clone)]
pub struct Label {
  pub span: Span,
  pub name: SpannedString,
  pub value: SpannedString
}

impl Label {
  pub fn new(span: Span, name: SpannedString, value: SpannedString) -> Label
  {
    Label {
      span,
      name,
      value,
    }
  }

  pub(crate) fn from_record(record: Pair) -> Result<Label> {
    let span = Span::from_pair(&record);
    let mut name = None;
    let mut value = None;

    for field in record.into_inner() {
      match field.as_rule() {
        Rule::label_name | Rule::label_single_name => name = Some(parse_string(&field)?),
        Rule::label_quoted_name | Rule::label_single_quoted_name => {
          // label seems to be uniquely able to span multiple lines when quoted
          let v = unquote(&clean_escaped_breaks(field.as_str()))
            .context(UnescapeError)?;

          name = Some(SpannedString {
            content: v,
            span: Span::from_pair(&field),
          });
        },

        Rule::label_value => value = Some(parse_string(&field)?),
        Rule::label_quoted_value => {
          let v = unquote(&clean_escaped_breaks(field.as_str()))
            .context(UnescapeError)?;

          value = Some(SpannedString {
            content: v,
            span: Span::from_pair(&field),
          });
        },
        Rule::comment => continue,
        _ => return Err(unexpected_token(field))
      }
    }

    let name = name.ok_or_else(|| Error::GenericParseError {
      message: "label name is required".into()
    })?;

    let value = value.ok_or_else(|| Error::GenericParseError {
      message: "label value is required".into()
    })?;

    Ok(Label::new(span, name, value))
  }
}

/// A Dockerfile [`LABEL` instruction][label].
///
/// A single `LABEL` instruction may set many labels.
///
/// [label]: https://docs.docker.com/engine/reference/builder/#label
#[derive(Debug, PartialEq, Eq, Clone)]
pub struct LabelInstruction {
  pub span: Span,
  pub labels: Vec<Label>,
}

impl LabelInstruction {
  pub(crate) fn from_record(record: Pair) -> Result<LabelInstruction> {
    let span = Span::from_pair(&record);
    let mut labels = Vec::new();

    for field in record.into_inner() {
      match field.as_rule() {
        Rule::label_pair => labels.push(Label::from_record(field)?),
        Rule::label_single => labels.push(Label::from_record(field)?),
        Rule::comment => continue,
        _ => return Err(unexpected_token(field))
      }
    }

    Ok(LabelInstruction {
      span,
      labels,
    })
  }
}

impl<'a> TryFrom<&'a Instruction> for &'a LabelInstruction {
  type Error = Error;

  fn try_from(instruction: &'a Instruction) -> std::result::Result<Self, Self::Error> {
    if let Instruction::Label(l) = instruction {
      Ok(l)
    } else {
      Err(Error::ConversionError {
        from: format!("{:?}", instruction),
        to: "LabelInstruction".into()
      })
    }
  }
}

#[cfg(test)]
mod tests {
  use indoc::indoc;
  use pretty_assertions::assert_eq;

  use super::*;
  use crate::test_util::*;

  #[test]
  fn label_basic() -> Result<()> {
    assert_eq!(
      parse_single("label foo=bar", Rule::label)?,
      LabelInstruction {
        span: Span::new(0, 13),
        labels: vec![
          Label::new(
            Span::new(6, 13),
            SpannedString {
              span: Span::new(6, 9),
              content: "foo".to_string(),
            }, SpannedString {
              span: Span::new(10, 13),
              content: "bar".to_string()
            },
          )
        ]
      }.into()
    );

    assert_eq!(
      parse_single("label foo.bar=baz", Rule::label)?,
      LabelInstruction {
        span: Span::new(0, 17),
        labels: vec![
          Label::new(
            Span::new(6, 17),
            SpannedString {
              span: Span::new(6, 13),
              content: "foo.bar".to_string(),
            },
            SpannedString {
              span: Span::new(14, 17),
              content: "baz".to_string()
            }
          )
        ]
      }.into()
    );

    assert_eq!(
      parse_single(r#"label "foo.bar"="baz qux""#, Rule::label)?,
      LabelInstruction {
        span: Span::new(0, 25),
        labels: vec![
          Label::new(
            Span::new(6, 25),
            SpannedString {
              span: Span::new(6, 15),
              content: "foo.bar".to_string(),
            }, SpannedString {
              span: Span::new(16, 25),
              content: "baz qux".to_string(),
            },
          )
        ]
      }.into()
    );

    // this is undocumented but supported :(
    assert_eq!(
      parse_single(r#"label foo.bar baz"#, Rule::label)?,
      LabelInstruction {
        span: Span::new(0, 17),
        labels: vec![
          Label::new(
            Span::new(5, 17),
            SpannedString {
              span: Span::new(6, 13),
              content: "foo.bar".to_string(),
            },
            SpannedString {
              span: Span::new(14, 17),
              content: "baz".to_string(),
            }
          )
        ]
      }.into()
    );
    assert_eq!(
      parse_single(r#"label "foo.bar" "baz qux""#, Rule::label)?,
      LabelInstruction {
        span: Span::new(0, 25),
        labels: vec![
          Label::new(
            Span::new(5, 25),
            SpannedString {
              span: Span::new(6, 15),
              content: "foo.bar".to_string(),
            },
            SpannedString {
              span: Span::new(16, 25),
              content: "baz qux".to_string(),
            },
          )
        ]
      }.into()
    );

    Ok(())
  }

  #[test]
  fn label_multi() -> Result<()> {
    assert_eq!(
      parse_single(r#"label foo=bar baz="qux" "quux quuz"="corge grault""#, Rule::label)?,
      LabelInstruction {
        span: Span::new(0, 50),
        labels: vec![
          Label::new(
            Span::new(6, 13),
            SpannedString {
              span: Span::new(6, 9),
              content: "foo".to_string(),
            },
            SpannedString {
              span: Span::new(10, 13),
              content: "bar".to_string(),
            },
          ),
          Label::new(
            Span::new(14, 23),
            SpannedString {
              span: Span::new(14, 17),
              content: "baz".to_string(),
            },
            SpannedString {
              span: Span::new(18, 23),
              content: "qux".to_string(),
            },
          ),
          Label::new(
            Span::new(24, 50),
            SpannedString {
              span: Span::new(24, 35),
              content: "quux quuz".to_string(),
            },
            SpannedString {
              span: Span::new(36, 50),
              content: "corge grault".to_string(),
            },
          )
        ]
      }.into()
    );

    assert_eq!(
      parse_single(
        r#"label foo=bar \
          baz="qux" \
          "quux quuz"="corge grault""#,
        Rule::label
      )?,
      LabelInstruction {
        span: Span::new(0, 74),
        labels: vec![
          Label::new(
            Span::new(6, 13),
            SpannedString {
              span: Span::new(6, 9),
              content: "foo".to_string(),
            },
            SpannedString {
              span: Span::new(10, 13),
              content: "bar".to_string(),
            },
          ),
          Label::new(
            Span::new(26, 35),
            SpannedString {
              span: Span::new(26, 29),
              content: "baz".to_string(),
            },
            SpannedString {
              span: Span::new(30, 35),
              content: "qux".to_string(),
            },
          ),
          Label::new(
            Span::new(48, 74),
            SpannedString {
              span: Span::new(48, 59),
              content: "quux quuz".to_string(),
            },
            SpannedString {
              span: Span::new(60, 74),
              content: "corge grault".to_string(),
            },
          )
        ]
      }.into()
    );

    Ok(())
  }

  #[test]
  fn label_multiline() -> Result<()> {
    assert_eq!(
      parse_single(r#"label "foo.bar"="baz\n qux""#, Rule::label)?,
      LabelInstruction {
        span: Span::new(0, 27),
        labels: vec![
          Label::new(
            Span::new(6, 27),
            SpannedString {
              span: Span::new(6, 15),
              content: "foo.bar".to_string(),
            },
            SpannedString {
              span: Span::new(16, 27),
              content: "baz\n qux".to_string(),
            },
          )
        ]
      }.into()
    );

    assert_eq!(
      parse_single(r#"label "foo\nbar"="baz\n qux""#, Rule::label)?,
      LabelInstruction {
        span: Span::new(0, 28),
        labels: vec![
          Label::new(
            Span::new(6, 28),
            SpannedString {
              span: Span::new(6, 16),
              content: "foo\nbar".to_string(),
            },
            SpannedString {
              span: Span::new(17, 28),
              content: "baz\n qux".to_string(),
            },
          )
        ]
      }.into()
    );

    Ok(())
  }

  #[test]
  fn label_multi_multiline() -> Result<()> {
    assert_eq!(
      parse_single(
        r#"label foo=bar \
          "lorem ipsum
          dolor
          "="sit
          amet" \
          baz=qux"#,
        Rule::label
      )?,
      LabelInstruction {
        span: Span::new(0, 107),
        labels: vec![
          Label::new(
            Span::new(6, 13),
            SpannedString {
              span: Span::new(6, 9),
              content: "foo".to_string(),
            },
            SpannedString {
              span: Span::new(10, 13),
              content: "bar".to_string(),
            },
          ),
          Label::new(
            Span::new(26, 87),
            SpannedString {
              span: Span::new(26, 66),
              content: "lorem ipsum\n          dolor\n          ".to_string(),
            },
            SpannedString {
              span: Span::new(67, 87),
              content: "sit\n          amet".to_string(),
            },
          ),
          Label::new(
            Span::new(100, 107),
            SpannedString {
              span: Span::new(100, 103),
              content: "baz".to_string(),
            },
            SpannedString {
              span: Span::new(104, 107),
              content: "qux".to_string(),
            },
          )
        ]
      }.into()
    );

    Ok(())
  }

  #[test]
  fn label_multiline_improper_continuation() -> Result<()> {
    // note: docker allows empty line continuations (but may print a warning)
    assert_eq!(
      parse_single(
        indoc!(r#"
          label foo=a \
            bar=b \
            baz=c \

        "#),
        Rule::label
      )?.into_label().unwrap().labels,
      vec![
        Label::new(
          Span::new(6, 11),
          SpannedString {
          span: Span::new(6, 9),
            content: "foo".to_string(),
          },
          SpannedString {
            span: Span::new(10, 11),
            content: "a".to_string(),
          },
        ),
        Label::new(
          Span::new(16, 21),
          SpannedString {
            span: Span::new(16, 19),
            content: "bar".to_string(),
          },
          SpannedString {
            span: Span::new(20, 21),
            content: "b".to_string(),
          },
        ),
        Label::new(
          Span::new(26, 31),
          SpannedString {
            span: Span::new(26, 29),
            content: "baz".to_string(),
          },
          SpannedString {
            span: Span::new(30, 31),
            content: "c".to_string(),
          },
        ),
      ]
    );

    Ok(())
  }
}
