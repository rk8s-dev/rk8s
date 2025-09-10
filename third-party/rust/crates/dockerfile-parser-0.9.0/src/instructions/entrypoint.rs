// (C) Copyright 2019-2020 Hewlett Packard Enterprise Development LP

use std::convert::TryFrom;

use crate::Span;
use crate::dockerfile_parser::Instruction;
use crate::error::*;
use crate::util::*;
use crate::parser::*;

/// A Dockerfile [`ENTRYPOINT` instruction][entrypoint].
///
/// An entrypoint may be defined as either a single string (to be run in the
/// default shell), or a list of strings (to be run directly).
///
/// [entrypoint]: https://docs.docker.com/engine/reference/builder/#entrypoint
#[derive(Debug, PartialEq, Eq, Clone)]
pub struct EntrypointInstruction {
  pub span: Span,
  pub expr: ShellOrExecExpr,
}

impl EntrypointInstruction {
  pub(crate) fn from_record(record: Pair) -> Result<EntrypointInstruction> {
    let span = Span::from_pair(&record);
    let field = record.into_inner().next().unwrap();

    match field.as_rule() {
      Rule::entrypoint_exec => Ok(EntrypointInstruction {
        span,
        expr: ShellOrExecExpr::Exec(parse_string_array(field)?),
      }),
      Rule::entrypoint_shell => Ok(EntrypointInstruction {
        span,
        expr: ShellOrExecExpr::Shell(parse_any_breakable(field)?),
      }),
      _ => Err(unexpected_token(field)),
    }
  }

  /// Unpacks this instruction into its inner value if it is a Shell-form
  /// instruction, otherwise returns None.
  pub fn into_shell(self) -> Option<BreakableString> {
    self.expr.into_shell()
  }

  /// Unpacks this instruction into its inner value if it is a Shell-form
  /// instruction, otherwise returns None.
  pub fn as_shell(&self) -> Option<&BreakableString> {
    self.expr.as_shell()
  }

  /// Unpacks this instruction into its inner value if it is an Exec-form
  /// instruction, otherwise returns None.
  pub fn into_exec(self) -> Option<StringArray> {
    self.expr.into_exec()
  }

  /// Unpacks this instruction into its inner value if it is an Exec-form
  /// instruction, otherwise returns None.
  pub fn as_exec(&self) -> Option<&StringArray> {
    self.expr.as_exec()
  }
}

impl TryFrom<Instruction> for EntrypointInstruction {
  type Error = Error;

  fn try_from(instruction: Instruction) -> std::result::Result<Self, Self::Error> {
    if let Instruction::Entrypoint(e) = instruction {
      Ok(e)
    } else {
      Err(Error::ConversionError {
        from: format!("{:?}", instruction),
        to: "EntrypointInstruction".into()
      })
    }
  }
}

#[cfg(test)]
mod tests {
  use indoc::indoc;
  use pretty_assertions::assert_eq;

  use super::*;
  use crate::Span;
  use crate::test_util::*;

  #[test]
  fn entrypoint_basic() -> Result<()> {
    assert_eq!(
      parse_single(r#"entrypoint echo "hello world""#, Rule::entrypoint)?
        .as_entrypoint().unwrap()
        .as_shell().unwrap(),
      &BreakableString::new((11, 29))
        .add_string((11, 29), "echo \"hello world\"")
    );

    assert_eq!(
      parse_single(r#"entrypoint ["echo", "hello world"]"#, Rule::entrypoint)?,
      EntrypointInstruction {
        span: Span::new(0, 34),
        expr: ShellOrExecExpr::Exec(StringArray {
          span: Span::new(11, 34),
          elements: vec![SpannedString {
            span: Span::new(12, 18),
            content: "echo".to_string(),
          }, SpannedString {
            span: Span::new(20, 33),
            content: "hello world".to_string(),
          }]
        })
      }.into()
    );

    Ok(())
  }

  #[test]
  fn entrypoint_multiline_exec() -> Result<()> {
    assert_eq!(
      parse_single(r#"entrypoint\
        [\
        "echo", \
        "hello world"\
        ]"#, Rule::entrypoint)?,
      EntrypointInstruction {
        span: Span::new(0, 73),
        expr: ShellOrExecExpr::Exec(StringArray {
          span: Span::new(20, 73),
          elements: vec![SpannedString {
            span: Span::new(31, 37),
            content: "echo".to_string(),
          }, SpannedString {
            span: Span::new(49, 62),
            content: "hello world".to_string(),
          }]
        }),
      }.into()
    );

    Ok(())
  }

  #[test]
  fn entrypoint_multiline_shell() -> Result<()> {
    assert_eq!(
      parse_single(indoc!(r#"
        entrypoint echo \
          "hello world"
      "#), Rule::entrypoint)?
        .as_entrypoint().unwrap()
        .as_shell().unwrap(),
      &BreakableString::new((11, 33))
        .add_string((11, 16), "echo ")
        .add_string((18, 33), "  \"hello world\"")
    );

    Ok(())
  }

  #[test]
  fn entrypoint_multiline_large() -> Result<()> {
    // note: the trailing `\` at the end is _almost_ nonsense and generates a
    // warning from docker
    let ins = parse_single(
      indoc!(r#"
        entrypoint set -x && \
            # lorem ipsum
            echo "hello world" && \
            # dolor sit amet,
            # consectetur \
            # adipiscing elit, \
            # sed do eiusmod
            # tempor incididunt ut labore
            echo foo && \
            echo 'bar' \
            && echo baz \
            # et dolore magna aliqua."#),
      Rule::entrypoint
    )?.into_entrypoint().unwrap().into_shell().unwrap();

    assert_eq!(
      ins,
      BreakableString::new((11, 273))
        .add_string((11, 21), "set -x && ")
        .add_comment((27, 40), "# lorem ipsum")
        .add_string((41, 67), "    echo \"hello world\" && ")
        .add_comment((73, 90), "# dolor sit amet,")
        .add_comment((95, 110), "# consectetur \\")
        .add_comment((115, 135), "# adipiscing elit, \\")
        .add_comment((140, 156), "# sed do eiusmod")
        .add_comment((161, 190), "# tempor incididunt ut labore")
        .add_string((191, 207), "    echo foo && ")
        .add_string((209, 224), "    echo 'bar' ")
        .add_string((226, 242), "    && echo baz ")
        .add_comment((248, 273), "# et dolore magna aliqua.")
    );

    assert_eq!(
      ins.to_string(),
      r#"set -x &&     echo "hello world" &&     echo foo &&     echo 'bar'     && echo baz "#
    );

    Ok(())
  }
}
