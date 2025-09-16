// (C) Copyright 2020 Hewlett Packard Enterprise Development LP

use std::fmt;
use std::ops::Index;

use crate::dockerfile_parser::{Dockerfile, Instruction};
use crate::image::ImageRef;

/// The parent image of a Docker build stage
#[derive(Debug, Eq, PartialEq, Clone)]
pub enum StageParent<'a> {
  /// An externally-built image, potentially from a remote registry
  Image(&'a ImageRef),

  /// An index of a previous stage within the current Dockerfile
  Stage(usize),

  /// The empty (scratch) parent image
  Scratch
}

impl<'a> fmt::Display for StageParent<'a> {
  fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
    match self {
      StageParent::Image(image) => image.fmt(f),
      StageParent::Stage(index) => index.fmt(f),
      StageParent::Scratch => write!(f, "scratch")
    }
  }
}

/// A single stage in a [multi-stage build].
///
/// A stage begins with (and includes) a `FROM` instruction and continues until
/// (but does *not* include) the next `FROM` instruction, if any.
///
/// Stages have an index and an optional alias. Later `COPY --from=$index [...]`
/// instructions may copy files between unnamed build stages. The alias, if
/// defined in this stage's `FROM` instruction, may be used as well.
///
/// Note that instructions in a Dockerfile before the first `FROM` are not
/// included in the first stage's list of instructions.
///
/// [multi-stage build]: https://docs.docker.com/develop/develop-images/multistage-build/
#[derive(Debug, Eq)]
pub struct Stage<'a> {
  /// The stage index.
  pub index: usize,

  /// The stage's FROM alias, if any.
  pub name: Option<String>,

  /// An ordered list of instructions in this stage.
  pub instructions: Vec<&'a Instruction>,

  /// The direct parent of this stage.
  ///
  /// If this is the first stage, it will be equal to the root stage.
  pub parent: StageParent<'a>,

  /// The root image of this stage, either an external reference (possibly from
  /// a remote registry) or `scratch`.
  pub root: StageParent<'a>
}

impl<'a> Ord for Stage<'a> {
  fn cmp(&self, other: &Self) -> std::cmp::Ordering {
    self.index.cmp(&other.index)
  }
}

impl<'a> PartialOrd for Stage<'a> {
  fn partial_cmp(&self, other: &Self) -> Option<std::cmp::Ordering> {
    Some(self.cmp(&other))
  }
}

impl<'a> PartialEq for Stage<'a> {
  fn eq(&self, other: &Self) -> bool {
    self.index == other.index
  }
}

impl<'a> Stage<'a> {
  /// Finds the index, relative to this stage, of an ARG instruction defining
  /// the given name. Per the Dockerfile spec, only instructions following the
  /// ARG definition in a particular stage will have the value in scope, even
  /// if it was a defined globally or in a previous stage.
  pub fn arg_index(&self, name: &str) -> Option<usize> {
    self.instructions
      .iter()
      .enumerate()
      .find_map(|(i, ins)| match ins {
        Instruction::Arg(a) => if a.name.content == name { Some(i) } else { None },
        _ => None
      })
  }
}

/// A collection of stages in a [multi-stage build].
///
/// # Example
/// ```
/// use dockerfile_parser::Dockerfile;
///
/// let dockerfile = Dockerfile::parse(r#"
///   FROM alpine:3.12 as build
///   RUN echo "hello world" > /foo
///
///   FROM ubuntu:18.04
///   COPY --from=0 /foo /foo
/// "#).unwrap();
///
/// for stage in dockerfile.stages() {
///   println!("stage #{}, name: {:?}", stage.index, stage.name)
/// }
/// ```
#[derive(Debug)]
pub struct Stages<'a> {
  pub stages: Vec<Stage<'a>>
}

impl<'a> Stages<'a> {
  pub fn new(dockerfile: &'a Dockerfile) -> Stages<'a> {
    // note: instructions before the first FROM are not part of any stage and
    // are not included in the first stage's instruction list

    let mut stages = Stages { stages: vec![] };
    let mut next_stage_index = 0;

    for ins in &dockerfile.instructions {
      if let Instruction::From(from) = ins {
        let image_name = from.image.as_ref().to_ascii_lowercase();
        let parent = if image_name == "scratch" {
          StageParent::Scratch
        } else if let Some(stage) = stages.get_by_name(&image_name) {
          StageParent::Stage(stage.index)
        } else {
          StageParent::Image(&from.image_parsed)
        };

        let root = if let StageParent::Stage(parent_stage) = parent {
          stages.stages[parent_stage].root.clone()
        } else {
          parent.clone()
        };

        stages.stages.push(Stage {
          index: next_stage_index,
          name: from.alias.as_ref().map(|a| a.as_ref().to_ascii_lowercase()),
          instructions: vec![ins],
          parent,
          root
        });

        next_stage_index += 1;
      } else if !stages.stages.is_empty() {
        let len = stages.stages.len();
        if let Some(stage) = stages.stages.get_mut(len - 1) {
          stage.instructions.push(ins);
        }
      }
    }

    stages
  }

  /// Attempts to fetch a stage by its name (`FROM` alias).
  pub fn get_by_name(&'a self, name: &str) -> Option<&'a Stage<'a>> {
    self.stages.iter().find(|s| s.name == Some(name.to_ascii_lowercase()))
  }

  /// Attempts to fetch a stage by its string representation.
  ///
  /// Stages with a valid integer value are retrieved by index, otherwise by
  /// name.
  pub fn get(&'a self, s: &str) -> Option<&'a Stage<'a>> {
    match s.parse::<usize>() {
      Ok(index) => self.stages.get(index),
      Err(_) => self.get_by_name(s)
    }
  }

  /// Returns an iterator over `stages`, wrapping the underlying `Vec::iter()`.
  pub fn iter(&self) -> std::slice::Iter<'_, Stage<'a>> {
    self.stages.iter()
  }
}

impl<'a> Index<usize> for Stages<'a> {
  type Output = Stage<'a>;

  fn index(&self, index: usize) -> &Self::Output {
    &self.stages[index]
  }
}

impl<'a> IntoIterator for Stages<'a> {
  type Item = Stage<'a>;
  type IntoIter = std::vec::IntoIter<Stage<'a>>;

  fn into_iter(self) -> Self::IntoIter {
    self.stages.into_iter()
  }
}

#[cfg(test)]
mod tests {
  use super::*;
  use indoc::indoc;

  #[test]
  fn test_stages() {
    let dockerfile = Dockerfile::parse(indoc!(r#"
      FROM alpine:3.12

      FROM ubuntu:18.04 as build
      RUN echo "hello world"

      FROM build as build2
      COPY /foo /bar
      COPY /bar /baz

      FROM build as build3
    "#)).unwrap();

    let stages = Stages::new(&dockerfile);
    assert_eq!(stages.stages.len(), 4);
    assert_eq!(stages[1], Stage {
      index: 1,
      name: Some("build".into()),
      instructions: vec![&dockerfile.instructions[1], &dockerfile.instructions[2]],
      parent: StageParent::Image(&ImageRef::parse("ubuntu:18.04")),
      root: StageParent::Image(&ImageRef::parse("ubuntu:18.04")),
    });

    assert_eq!(stages[2], Stage {
      index: 2,
      name: Some("build2".into()),
      instructions: dockerfile.instructions[3..5].iter().collect(),
      parent: StageParent::Stage(1),
      root: StageParent::Image(&ImageRef::parse("ubuntu:18.04")),
    });

    assert_eq!(stages[3], Stage {
      index: 3,
      name: Some("build3".into()),
      instructions: vec![&dockerfile.instructions[6]],
      parent: StageParent::Stage(2),
      root: StageParent::Image(&ImageRef::parse("ubuntu:18.04")),
    });
  }

  #[test]
  fn test_stages_get() {
    let dockerfile = Dockerfile::parse(indoc!(r#"
      FROM alpine:3.12

      FROM ubuntu:18.04 as build

      FROM build as build2
    "#)).unwrap();

    let stages = Stages::new(&dockerfile);
    assert_eq!(stages.get("0").unwrap().index, 0);
    assert_eq!(stages.get("1"), stages.get("build"));
    assert_eq!(stages.get("2"), stages.get("build2"));
  }
}
