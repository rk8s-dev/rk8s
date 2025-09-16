// (C) Copyright 2019 Hewlett Packard Enterprise Development LP

use std::fmt;

use crate::error::*;
use crate::parser::*;
use crate::splicer::Span;

use enquote::unquote;
use snafu::ResultExt;

/// Given a node ostensibly containing a string array, returns an unescaped
/// array of strings
pub(crate) fn parse_string_array(array: Pair) -> Result<StringArray> {
  let span = Span::from_pair(&array);
  let mut elements = Vec::new();

  for field in array.into_inner() {
    match field.as_rule() {
      Rule::string => {
        elements.push(parse_string(&field)?);
      },
      Rule::comment => continue,
      _ => return Err(unexpected_token(field))
    }
  }

  Ok(StringArray {
    span,
    elements,
  })
}

pub(crate) fn parse_string(field: &Pair) -> Result<SpannedString> {
  let str_span = Span::from_pair(field);
  let field_str = field.as_str();
  let content = if matches!(field_str.chars().next(), Some('"' | '\'' | '`')) {
    unquote(field_str).context(UnescapeError)?
  } else {
    field_str.to_string()
  };

  Ok(SpannedString {
    span: str_span,
    content,
  })
}

/// Removes escaped line breaks (\\\n) from a string
///
/// This should be used to clean any input from the any_breakable rule
pub(crate) fn clean_escaped_breaks(s: &str) -> String {
  s.replace("\\\n", "")
}

/// A string that may be broken across many lines or an array of strings.
#[derive(Debug, PartialEq, Eq, Clone)]
pub enum ShellOrExecExpr {
  Shell(BreakableString),
  Exec(StringArray),
}

impl ShellOrExecExpr {
  /// Unpacks this expression into its inner value if it is a Shell-form
  /// instruction, otherwise returns None.
  pub fn into_shell(self) -> Option<BreakableString> {
    if let ShellOrExecExpr::Shell(s) = self {
      Some(s)
    } else {
      None
    }
  }

  /// Unpacks this expression into its inner value if it is a Shell-form
  /// instruction, otherwise returns None.
  pub fn as_shell(&self) -> Option<&BreakableString> {
    if let ShellOrExecExpr::Shell(s) = self {
      Some(s)
    } else {
      None
    }
  }

  /// Unpacks this expression into its inner value if it is an Exec-form
  /// instruction, otherwise returns None.
  pub fn into_exec(self) -> Option<StringArray> {
    if let ShellOrExecExpr::Exec(s) = self {
      Some(s)
    } else {
      None
    }
  }

  /// Unpacks this expression into its inner value if it is an Exec-form
  /// instruction, otherwise returns None.
  pub fn as_exec(&self) -> Option<&StringArray> {
    if let ShellOrExecExpr::Exec(s) = self {
      Some(s)
    } else {
      None
    }
  }
}

/// A string array (ex. ["executable", "param1", "param2"])
#[derive(Debug, Eq, PartialEq, Ord, PartialOrd, Clone)]
pub struct StringArray {
  pub span: Span,
  pub elements: Vec<SpannedString>,
}

impl StringArray {
  pub fn as_str_vec(&self) -> Vec<&str> {
    self.elements.iter().map(|c| c.as_ref()).collect()
  }
}

/// A comment with a character span.
#[derive(Debug, Eq, PartialEq, Ord, PartialOrd, Clone)]
pub struct SpannedComment {
  pub span: Span,
  pub content: String,
}

/// A string with a character span.
#[derive(Debug, Eq, PartialEq, Ord, PartialOrd, Clone)]
pub struct SpannedString {
  pub span: Span,
  pub content: String,
}

impl AsRef<str> for SpannedString {
  fn as_ref(&self) -> &str {
    &self.content
  }
}

impl fmt::Display for SpannedString {
  fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
    self.content.fmt(f)
  }
}

/// A component of a breakable string.
#[derive(Debug, Eq, PartialEq, Ord, PartialOrd, Clone)]
pub enum BreakableStringComponent {
  String(SpannedString),
  Comment(SpannedComment),
}

impl From<SpannedString> for BreakableStringComponent {
  fn from(s: SpannedString) -> Self {
    BreakableStringComponent::String(s)
  }
}

impl From<((usize, usize), &str)> for BreakableStringComponent {
  fn from(s: ((usize, usize), &str)) -> Self {
    let ((start, end), content) = s;

    BreakableStringComponent::String(SpannedString {
      span: (start, end).into(),
      content: content.to_string(),
    })
  }
}

impl From<SpannedComment> for BreakableStringComponent {
  fn from(c: SpannedComment) -> Self {
    BreakableStringComponent::Comment(c)
  }
}

/// A Docker string that may be broken across several lines, separated by line
/// continuations (`\\\n`), and possibly intermixed with comments.
///
/// These strings have several potentially valid interpretations. As these line
/// continuations match those natively supported by bash, a given multiline
/// `RUN` block can be pasted into a bash shell unaltered and with line
/// continuations included. However, at "runtime" line continuations and
/// comments (*) are stripped from the string handed to the shell.
///
/// To ensure output is correct in all cases, `BreakableString` preserves the
/// user's original AST, including comments, and implements Docker's
/// continuation-stripping behavior in the `Display` implementation.
#[derive(Debug, Eq, PartialEq, Ord, PartialOrd, Clone)]
pub struct BreakableString {
  pub span: Span,
  pub components: Vec<BreakableStringComponent>,
}

/// Formats this breakable string as it will be interpreted by the underlying
/// Docker engine, i.e. on a single line with line continuations removed
impl fmt::Display for BreakableString {
  fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
    for component in &self.components {
      if let BreakableStringComponent::String(s) = &component {
        write!(f, "{}", s.content)?;
      }
    }

    Ok(())
  }
}

impl BreakableString {
  pub fn new(span: impl Into<Span>) -> Self {
    BreakableString {
      span: span.into(),
      components: Vec::new(),
    }
  }

  pub fn add(mut self, c: impl Into<BreakableStringComponent>) -> Self {
    self.components.push(c.into());

    self
  }

  pub fn add_string(mut self, s: impl Into<Span>, c: impl Into<String>) -> Self {
    self.components.push(SpannedString {
      span: s.into(),
      content: c.into(),
    }.into());

    self
  }

  pub fn add_comment(mut self, s: impl Into<Span>, c: impl Into<String>) -> Self {
    self.components.push(SpannedComment {
      span: s.into(),
      content: c.into(),
    }.into());

    self
  }

  pub fn iter_components(&self) -> impl Iterator<Item = &BreakableStringComponent> {
    self.components.iter()
  }
}

impl From<((usize, usize), &str)> for BreakableString {
  fn from(s: ((usize, usize), &str)) -> Self {
    let ((start, end), content) = s;

    BreakableString::new((start, end))
      .add_string((start, end), content)
  }
}

fn parse_any_breakable_inner(pair: Pair) -> Result<Vec<BreakableStringComponent>> {
  let mut components = Vec::new();

  for field in pair.into_inner() {
    match field.as_rule() {
      Rule::any_breakable => components.extend(parse_any_breakable_inner(field)?),
      Rule::comment => components.push(SpannedComment {
        span: (&field).into(),
        content: field.as_str().to_string(),
      }.into()),
      Rule::any_content => components.push(SpannedString {
        span: (&field).into(),
        content: field.as_str().to_string(),
      }.into()),
      _ => return Err(unexpected_token(field))
    }
  }

  Ok(components)
}

pub(crate) fn parse_any_breakable(pair: Pair) -> Result<BreakableString> {
  Ok(BreakableString {
    span: (&pair).into(),
    components: parse_any_breakable_inner(pair)?,
  })
}
