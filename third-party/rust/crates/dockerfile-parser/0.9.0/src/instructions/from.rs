// (C) Copyright 2019-2020 Hewlett Packard Enterprise Development LP

use std::convert::TryFrom;

use crate::dockerfile_parser::Instruction;
use crate::image::ImageRef;
use crate::parser::{Pair, Rule};
use crate::parse_string;
use crate::SpannedString;
use crate::splicer::*;
use crate::error::*;

use lazy_static::lazy_static;
use regex::Regex;

/// A key/value pair passed to a `FROM` instruction as a flag.
///
/// Examples include: `FROM --platform=linux/amd64 node:lts-alpine`
#[derive(Debug, PartialEq, Eq, Clone)]
pub struct FromFlag {
  pub span: Span,
  pub name: SpannedString,
  pub value: SpannedString,
}

impl FromFlag {
  fn from_record(record: Pair) -> Result<FromFlag> {
    let span = Span::from_pair(&record);
    let mut name = None;
    let mut value = None;

    for field in record.into_inner() {
      match field.as_rule() {
        Rule::from_flag_name => name = Some(parse_string(&field)?),
        Rule::from_flag_value => value = Some(parse_string(&field)?),
        _ => return Err(unexpected_token(field))
      }
    }

    let name = name.ok_or_else(|| Error::GenericParseError {
      message: "from flags require a key".into(),
    })?;

    let value = value.ok_or_else(|| Error::GenericParseError {
      message: "from flags require a value".into()
    })?;

    Ok(FromFlag {
      span, name, value
    })
  }
}


/// A Dockerfile [`FROM` instruction][from].
///
/// Contains spans for the entire instruction, the image, and the alias (if
/// any).
///
/// [from]: https://docs.docker.com/engine/reference/builder/#from
#[derive(Debug, PartialEq, Eq, Clone)]
pub struct FromInstruction {
  pub span: Span,
  pub flags: Vec<FromFlag>,
  pub image: SpannedString,
  pub image_parsed: ImageRef,

  pub index: usize,
  pub alias: Option<SpannedString>,
}

impl FromInstruction {
  pub(crate) fn from_record(record: Pair, index: usize) -> Result<FromInstruction> {
    lazy_static! {
      static ref HEX: Regex =
          Regex::new(r"[0-9a-fA-F]+").unwrap();
    }

    let span = Span::from_pair(&record);
    let mut image_field = None;
    let mut alias_field = None;
    let mut flags = Vec::new();

    for field in record.into_inner() {
      match field.as_rule() {
        Rule::from_flag => flags.push(FromFlag::from_record(field)?),        
        Rule::from_image => image_field = Some(field),
        Rule::from_alias => alias_field = Some(field),
        Rule::comment => continue,
        _ => return Err(unexpected_token(field))
      };
    }

    let image = if let Some(image_field) = image_field {
      parse_string(&image_field)?
    } else {
      return Err(Error::GenericParseError {
        message: "missing from image".into()
      });
    };

    let image_parsed = ImageRef::parse(&image.as_ref());

    if let Some(hash) = &image_parsed.hash {
      let parts: Vec<&str> = hash.split(":").collect();
      if let ["sha256", hexdata] = parts[..] {
        if !HEX.is_match(hexdata) || hexdata.len() != 64 {
          return Err(Error::GenericParseError { message: "image reference digest is invalid".into() });
        }
      } else {
        return Err(Error::GenericParseError { message: "image reference digest is invalid".into() });
      }
    }

    let alias = if let Some(alias_field) = alias_field {
      Some(parse_string(&alias_field)?)
    } else {
      None
    };

    Ok(FromInstruction {
      span, index,
      image, image_parsed,
      flags, alias,
    })
  }

  // TODO: util for converting to an ImageRef while resolving ARG
  // per the docs, ARG instructions are only honored in FROMs if they occur
  // before the *first* FROM (but this should be verified)
  // fn image_ref(&self) -> ImageRef { ... }
}

impl<'a> TryFrom<&'a Instruction> for &'a FromInstruction {
  type Error = Error;

  fn try_from(instruction: &'a Instruction) -> std::result::Result<Self, Self::Error> {
    if let Instruction::From(f) = instruction {
      Ok(f)
    } else {
      Err(Error::ConversionError {
        from: format!("{:?}", instruction),
        to: "FromInstruction".into()
      })
    }
  }
}

#[cfg(test)]
mod tests {
  use core::panic;

use indoc::indoc;
  use pretty_assertions::assert_eq;

  use super::*;
  use crate::test_util::*;

  #[test]
  fn from_bad_digest() {
    let cases = vec![
      "from alpine@sha256:ca5a2eb9b7917e542663152b04c0",
      "from alpine@sha257:ca5a2eb9b7917e542663152b04c0ad0572e0522fcf80ff080156377fc08ea8f8",
      "from alpine@ca5a2eb9b7917e542663152b04c0ad0572e0522fcf80ff080156377fc08ea8f8",
    ];

    for case in cases {
      let result = parse_direct(
        case,
        Rule::from,
        |p| FromInstruction::from_record(p, 0)
      );

      match result {
        Ok(_) => panic!("Expected parse error."),
        Err(Error::GenericParseError { message: _}) => {},
        Err(_) => panic!("Expected GenericParseError"),
      };
    }
  }

  #[test]
  fn from_no_alias() -> Result<()> {
    // pulling the FromInstruction out of the enum is messy, so just parse
    // directly
    let from = parse_direct(
      "from alpine:3.10",
      Rule::from,
      |p| FromInstruction::from_record(p, 0)
    )?;

    assert_eq!(from, FromInstruction {
      span: Span { start: 0, end: 16 },
      index: 0,
      image: SpannedString {
        span: Span { start: 5, end: 16 },
        content: "alpine:3.10".into(),
      },
      image_parsed: ImageRef {
        registry: None,
        image: "alpine".into(),
        tag: Some("3.10".into()),
        hash: None
      },
      alias: None,
      flags: vec![],
    });

    Ok(())
  }

  #[test]
  fn from_no_newline() -> Result<()> {
    // unfortunately we can't use a single rule to test these as individual
    // rules have no ~ EOI requirement to ensure we parse the whole string
    assert!(parse_single(
      "from alpine:3.10 from example",
      Rule::dockerfile,
    ).is_err());

    Ok(())
  }

  #[test]
  fn from_missing_alias() -> Result<()> {
    assert!(parse_single(
      "from alpine:3.10 as",
      Rule::dockerfile,
    ).is_err());

    Ok(())
  }

  #[test]
  fn from_flags() -> Result<()> {
    assert_eq!(
      parse_single(
        "FROM --platform=linux/amd64 alpine:3.10",
        Rule::from
      )?,
      FromInstruction {
        index: 0,
        span: Span { start: 0, end: 39 },
        flags: vec![
          FromFlag {
            span: Span { start: 5, end: 27 },
            name: SpannedString {
              content: "platform".into(),
              span: Span { start: 7, end: 15 },
            },
            value: SpannedString {
              content: "linux/amd64".into(),
              span: Span { start: 16, end: 27 },
            }
          }
        ],
        image: SpannedString {
          span: Span { start: 28, end: 39 },
          content: "alpine:3.10".into(),
        },
        image_parsed: ImageRef {
          registry: None,
          image: "alpine".into(),
          tag: Some("3.10".into()),
          hash: None
        },
        alias: None,
      }.into()
    );

    Ok(())
  }


  #[test]
  fn from_multiline() -> Result<()> {
    let from = parse_direct(
      indoc!(r#"
        from \
          # foo
          alpine:3.10 \

          # test
          # comment

          as \

          test
      "#),
      Rule::from,
      |p| FromInstruction::from_record(p, 0)
    )?;

    assert_eq!(from, FromInstruction {
      span: Span { start: 0, end: 68 },
      index: 0,
      image: SpannedString {
        span: Span { start: 17, end: 28 },
        content: "alpine:3.10".into(),
      },
      image_parsed: ImageRef {
        registry: None,
        image: "alpine".into(),
        tag: Some("3.10".into()),
        hash: None
      },
      alias: Some(SpannedString {
        span: (64, 68).into(),
        content: "test".into(),
      }),
      flags: vec![],
    });

    Ok(())
  }
}
