// (C) Copyright 2019-2020 Hewlett Packard Enterprise Development LP

use std::collections::{HashMap, HashSet};
use std::fmt;
use std::iter::FromIterator;

use lazy_static::lazy_static;
use regex::Regex;

use crate::{Dockerfile, Span, Splicer};

/// A parsed docker image reference
///
/// The `Display` impl may be used to convert a parsed image back to a plain
/// string:
/// ```
/// use dockerfile_parser::ImageRef;
///
/// let image = ImageRef::parse("alpine:3.11");
/// assert_eq!(image.registry, None);
/// assert_eq!(image.image, "alpine");
/// assert_eq!(image.tag, Some("3.11".to_string()));
/// assert_eq!(format!("{}", image), "alpine:3.11");
/// ```
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ImageRef {
  /// an optional registry, generally Docker Hub if unset
  pub registry: Option<String>,

  /// an image string, possibly including a user or organization name
  pub image: String,

  /// An optional image tag (after the colon, e.g. `:1.2.3`), generally inferred
  /// to mean `:latest` if unset
  pub tag: Option<String>,

  /// An optional embedded image hash, e.g. `sha256:...`. Conflicts with `tag`.
  pub hash: Option<String>
}

/// Determines if an ImageRef token refers to a registry hostname or not
///
/// Based on rules from https://stackoverflow.com/a/42116190
fn is_registry(token: &str) -> bool {
  token == "localhost" || token.contains('.') || token.contains(':')
}

/// Given a map of key/value pairs, perform variable substitution on a given
/// input string. `max_recursion_depth` controls the maximum allowed recursion
/// depth if variables refer to other strings themselves containing variable
/// references. A small number but reasonable is recommended by default, e.g.
/// 16.
/// If None is returned, substitution was impossible, either because a
/// referenced variable did not exist, or recursion depth was exceeded.
pub fn substitute<'a, 'b>(
  s: &'a str,
  vars: &'b HashMap<&'b str, &'b str>,
  used_vars: &mut HashSet<String>,
  max_recursion_depth: u8
) -> Option<String> {
  lazy_static! {
    static ref VAR: Regex = Regex::new(r"\$(?:([A-Za-z0-9_]+)|\{([A-Za-z0-9_]+)\})").unwrap();
  }

  // note: docker also allows defaults in FROMs, e.g.
  //   ARG tag
  //   FROM alpine:${tag:-3.12}
  // this isn't currently supported.

  let mut splicer = Splicer::from_str(s);

  for caps in VAR.captures_iter(s) {
    if max_recursion_depth == 0 {
      // can't substitute, so give up
      return None;
    }

    let full_range = caps.get(0)?.range();
    let var_name = caps.get(1).or_else(|| caps.get(2))?;
    let var_content = vars.get(var_name.as_str())?;
    let substituted_content = substitute(
      var_content,
      vars,
      used_vars,
      max_recursion_depth.saturating_sub(1)
    )?;
    used_vars.insert(var_name.as_str().to_string());

    // splice the substituted content back into the output string
    splicer.splice(&Span::new(full_range.start, full_range.end), &substituted_content);
  }

  Some(splicer.content)
}

impl ImageRef {
  /// Parses an `ImageRef` from a string.
  ///
  /// This is not fallible, however malformed image strings may return
  /// unexpected results.
  pub fn parse(s: &str) -> ImageRef {
    // tags may be one of:
    // foo (implies registry.hub.docker.com/library/foo:latest)
    // foo:bar (implies registry.hub.docker.com/library/foo:bar)
    // org/foo:bar (implies registry.hub.docker.com/org/foo:bar)

    // per https://stackoverflow.com/a/42116190, some extra rules are needed to
    // disambiguate external registries
    // localhost/foo:bar is allowed (localhost is special)
    // example.com/foo:bar is allowed
    // host/foo:bar is not allowed (conflicts with docker hub)
    // host:443/foo:bar is allowed (':' or '.' make it unambiguous)

    // we don't attempt to actually validate tags otherwise, so invalid
    // characters could slip through

    let parts: Vec<&str> = s.splitn(2, '/').collect();
    let (registry, image_full) = if parts.len() == 2 && is_registry(parts[0]) {
      // some 3rd party registry
      (Some(parts[0].to_string()), parts[1])
    } else {
      // some other image on the default registry; return the original string
      (None, s)
    };

    if let Some(at_pos) = image_full.find('@') {
      // parts length is guaranteed to be at least 1 given an empty string
      let (image, hash) = image_full.split_at(at_pos);

      ImageRef {
        registry,
        image: image.to_string(),
        hash: Some(hash[1..].to_string()),
        tag: None
      }
    } else {
      // parts length is guaranteed to be at least 1 given an empty string
      let parts: Vec<&str> = image_full.splitn(2, ':').collect();
      let image = parts[0].to_string();
      let tag = parts.get(1).map(|p| String::from(*p));

      ImageRef { registry, image, tag, hash: None }
    }
  }

  /// Given a Dockerfile (and its global `ARG`s), perform any necessary
  /// variable substitution to resolve any variable references in this
  /// `ImageRef` and returns a list of variables included in the end result.
  ///
  /// If this `ImageRef` contains any unknown variables or if any references are
  /// excessively recursive, returns None; otherwise, returns the
  /// fully-substituted string.
  pub fn resolve_vars_with_context<'a>(
    &self, dockerfile: &'a Dockerfile
  ) -> Option<(ImageRef, HashSet<String>)> {
    let vars: HashMap<&'a str, &'a str> = HashMap::from_iter(
      dockerfile.global_args
        .iter()
        .filter_map(|a| match a.value.as_ref() {
          Some(v) => Some((a.name.as_ref(), v.as_ref())),
          None => None
        })
    );

    let mut used_vars = HashSet::new();

    if let Some(s) = substitute(&self.to_string(), &vars, &mut used_vars, 16) {
      Some((ImageRef::parse(&s), used_vars))
    } else {
      None
    }
  }

  /// Given a Dockerfile (and its global `ARG`s), perform any necessary
  /// variable substitution to resolve any variable references in this
  /// `ImageRef`.
  ///
  /// If this `ImageRef` contains any unknown variables or if any references are
  /// excessively recursive, returns None; otherwise, returns the
  /// fully-substituted string.
  pub fn resolve_vars(&self, dockerfile: &Dockerfile) -> Option<ImageRef> {
    self.resolve_vars_with_context(dockerfile).map(|(image, _vars)| image)
  }
}

impl fmt::Display for ImageRef {
  fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
    if let Some(registry) = &self.registry {
      write!(f, "{}/", registry)?;
    }

    write!(f, "{}", self.image)?;

    if let Some(tag) = &self.tag {
      write!(f, ":{}", tag)?;
    } else if let Some(hash) = &self.hash {
      write!(f, "@{}", hash)?;
    }

    Ok(())
  }
}

#[cfg(test)]
mod tests {
  use super::*;

  use std::convert::TryInto;
  use indoc::indoc;
  use crate::instructions::*;

  #[test]
  fn test_image_parse_dockerhub() {
    assert_eq!(
      ImageRef::parse("alpine:3.10"),
      ImageRef {
        registry: None,
        image: "alpine".into(),
        tag: Some("3.10".into()),
        hash: None
      }
    );

    assert_eq!(
      ImageRef::parse("foo/bar"),
      ImageRef {
        registry: None,
        image: "foo/bar".into(),
        tag: None,
        hash: None
      }
    );

    assert_eq!(
      ImageRef::parse("clux/muslrust"),
      ImageRef {
        registry: None,
        image: "clux/muslrust".into(),
        tag: None,
        hash: None
      }
    );

    assert_eq!(
      ImageRef::parse("clux/muslrust:1.41.0-stable"),
      ImageRef {
        registry: None,
        image: "clux/muslrust".into(),
        tag: Some("1.41.0-stable".into()),
        hash: None
      }
    );

    assert_eq!(
      ImageRef::parse("fake_project/fake_image@fake_hash"),
      ImageRef {
        registry: None,
        image: "fake_project/fake_image".into(),
        tag: None,
        hash: Some("fake_hash".into())
      }
    );

    // invalid hashes, but should still not panic
    assert_eq!(
      ImageRef::parse("fake_project/fake_image@"),
      ImageRef {
        registry: None,
        image: "fake_project/fake_image".into(),
        tag: None,
        hash: Some("".into())
      }
    );

    assert_eq!(
      ImageRef::parse("fake_project/fake_image@sha256:"),
      ImageRef {
        registry: None,
        image: "fake_project/fake_image".into(),
        tag: None,
        hash: Some("sha256:".into())
      }
    );
  }

  #[test]
  fn test_image_parse_registry() {
    assert_eq!(
      ImageRef::parse("quay.io/prometheus/node-exporter:v0.18.1"),
      ImageRef {
        registry: Some("quay.io".into()),
        image: "prometheus/node-exporter".into(),
        tag: Some("v0.18.1".into()),
        hash: None
      }
    );

    assert_eq!(
      ImageRef::parse("gcr.io/fake_project/fake_image:fake_tag"),
      ImageRef {
        registry: Some("gcr.io".into()),
        image: "fake_project/fake_image".into(),
        tag: Some("fake_tag".into()),
        hash: None
      }
    );

    assert_eq!(
      ImageRef::parse("gcr.io/fake_project/fake_image"),
      ImageRef {
        registry: Some("gcr.io".into()),
        image: "fake_project/fake_image".into(),
        tag: None,
        hash: None
      }
    );

    assert_eq!(
      ImageRef::parse("gcr.io/fake_image"),
      ImageRef {
        registry: Some("gcr.io".into()),
        image: "fake_image".into(),
        tag: None,
        hash: None
      }
    );

    assert_eq!(
      ImageRef::parse("gcr.io/fake_image:fake_tag"),
      ImageRef {
        registry: Some("gcr.io".into()),
        image: "fake_image".into(),
        tag: Some("fake_tag".into()),
        hash: None
      }
    );

    assert_eq!(
      ImageRef::parse("quay.io/fake_project/fake_image@fake_hash"),
      ImageRef {
        registry: Some("quay.io".into()),
        image: "fake_project/fake_image".into(),
        tag: None,
        hash: Some("fake_hash".into())
      }
    );
  }

  #[test]
  fn test_image_parse_localhost() {
    assert_eq!(
      ImageRef::parse("localhost/foo"),
      ImageRef {
        registry: Some("localhost".into()),
        image: "foo".into(),
        tag: None,
        hash: None
      }
    );

    assert_eq!(
      ImageRef::parse("localhost/foo:bar"),
      ImageRef {
        registry: Some("localhost".into()),
        image: "foo".into(),
        tag: Some("bar".into()),
        hash: None
      }
    );

    assert_eq!(
      ImageRef::parse("localhost/foo/bar"),
      ImageRef {
        registry: Some("localhost".into()),
        image: "foo/bar".into(),
        tag: None,
        hash: None
      }
    );

    assert_eq!(
      ImageRef::parse("localhost/foo/bar:baz"),
      ImageRef {
        registry: Some("localhost".into()),
        image: "foo/bar".into(),
        tag: Some("baz".into()),
        hash: None
      }
    );
  }

  #[test]
  fn test_image_parse_registry_port() {
    assert_eq!(
      ImageRef::parse("example.com:1234/foo"),
      ImageRef {
        registry: Some("example.com:1234".into()),
        image: "foo".into(),
        tag: None,
        hash: None
      }
    );

    assert_eq!(
      ImageRef::parse("example.com:1234/foo:bar"),
      ImageRef {
        registry: Some("example.com:1234".into()),
        image: "foo".into(),
        tag: Some("bar".into()),
        hash: None
      }
    );

    assert_eq!(
      ImageRef::parse("example.com:1234/foo/bar"),
      ImageRef {
        registry: Some("example.com:1234".into()),
        image: "foo/bar".into(),
        tag: None,
        hash: None
      }
    );

    assert_eq!(
      ImageRef::parse("example.com:1234/foo/bar:baz"),
      ImageRef {
        registry: Some("example.com:1234".into()),
        image: "foo/bar".into(),
        tag: Some("baz".into()),
        hash: None
      }
    );

    // docker hub doesn't allow it, but other registries can allow arbitrarily
    // nested images
    assert_eq!(
      ImageRef::parse("example.com:1234/foo/bar/baz:qux"),
      ImageRef {
        registry: Some("example.com:1234".into()),
        image: "foo/bar/baz".into(),
        tag: Some("qux".into()),
        hash: None
      }
    );
  }

  #[test]
  fn test_substitute() {
    let mut vars = HashMap::new();
    vars.insert("foo", "bar");
    vars.insert("baz", "qux");
    vars.insert("lorem", "$foo");
    vars.insert("ipsum", "${lorem}");
    vars.insert("recursion1", "$recursion2");
    vars.insert("recursion2", "$recursion1");

    let mut used_vars = HashSet::new();
    assert_eq!(
      substitute("hello world", &vars, &mut used_vars, 16).as_deref(),
      Some("hello world")
    );

    let mut used_vars = HashSet::new();
    assert_eq!(
      substitute("hello $foo", &vars, &mut used_vars, 16).as_deref(),
      Some("hello bar")
    );
    assert_eq!(used_vars, {
      let mut h = HashSet::new();
      h.insert("foo".to_string());
      h
    });

    let mut used_vars = HashSet::new();
    assert_eq!(
      substitute("hello $foo", &vars, &mut used_vars, 0).as_deref(),
      None
    );
    assert!(used_vars.is_empty());

    let mut used_vars = HashSet::new();
    assert_eq!(
      substitute("hello ${foo}", &vars, &mut used_vars, 16).as_deref(),
      Some("hello bar")
    );
    assert_eq!(used_vars, {
      let mut h = HashSet::new();
      h.insert("foo".to_string());
      h
    });

    let mut used_vars = HashSet::new();
    assert_eq!(
      substitute("$baz $foo", &vars, &mut used_vars, 16).as_deref(),
      Some("qux bar")
    );
    assert_eq!(used_vars, {
      let mut h = HashSet::new();
      h.insert("baz".to_string());
      h.insert("foo".to_string());
      h
    });

    let mut used_vars = HashSet::new();
    assert_eq!(
      substitute("hello $lorem", &vars, &mut used_vars, 16).as_deref(),
      Some("hello bar")
    );
    assert_eq!(used_vars, {
      let mut h = HashSet::new();
      h.insert("foo".to_string());
      h.insert("lorem".to_string());
      h
    });

    let mut used_vars = HashSet::new();
    assert_eq!(
      substitute("hello $lorem", &vars, &mut used_vars, 1).as_deref(),
      None
    );
    assert!(used_vars.is_empty());

    let mut used_vars = HashSet::new();
    assert_eq!(
      substitute("hello $ipsum", &vars, &mut used_vars, 16).as_deref(),
      Some("hello bar")
    );
    assert_eq!(used_vars, {
      let mut h = HashSet::new();
      h.insert("foo".to_string());
      h.insert("lorem".to_string());
      h.insert("ipsum".to_string());
      h
    });

    let mut used_vars = HashSet::new();
    assert_eq!(
      substitute("hello $ipsum", &vars, &mut used_vars, 2).as_deref(),
      None
    );
    assert!(used_vars.is_empty());

    let mut used_vars = HashSet::new();
    assert_eq!(
      substitute("hello $recursion1", &vars, &mut used_vars, 16).as_deref(),
      None
    );
    assert!(used_vars.is_empty());
  }

  #[test]
  fn test_resolve_vars() {
    let d = Dockerfile::parse(indoc!(r#"
      ARG image=alpine:3.12
      FROM $image
    "#)).unwrap();

    let from: &FromInstruction = d.instructions
      .get(1).unwrap()
      .try_into().unwrap();

    assert_eq!(
      from.image_parsed.resolve_vars(&d),
      Some(ImageRef::parse("alpine:3.12"))
    );
  }

  #[test]
  fn test_resolve_vars_nested() {
    let d = Dockerfile::parse(indoc!(r#"
      ARG image=alpine
      ARG unnecessarily_nested=${image}
      ARG tag=3.12
      FROM ${unnecessarily_nested}:${tag}
    "#)).unwrap();

    let from: &FromInstruction = d.instructions
      .get(3).unwrap()
      .try_into().unwrap();

    assert_eq!(
      from.image_parsed.resolve_vars(&d),
      Some(ImageRef::parse("alpine:3.12"))
    );
  }

  #[test]
  fn test_resolve_vars_technically_invalid() {
    // docker allows this, but we can't give an answer
    let d = Dockerfile::parse(indoc!(r#"
      ARG image
      FROM $image
    "#)).unwrap();

    let from: &FromInstruction = d.instructions
      .get(1).unwrap()
      .try_into().unwrap();

    assert_eq!(
      from.image_parsed.resolve_vars(&d),
      None
    );
  }

  #[test]
  fn test_resolve_vars_typo() {
    // docker allows this, but we can't give an answer
    let d = Dockerfile::parse(indoc!(r#"
      ARG image="alpine:3.12"
      FROM $foo
    "#)).unwrap();

    let from: &FromInstruction = d.instructions
      .get(1).unwrap()
      .try_into().unwrap();

    assert_eq!(
      from.image_parsed.resolve_vars(&d),
      None
    );
  }

  #[test]
  fn test_resolve_vars_out_of_order() {
    // docker allows this, but we can't give an answer
    let d = Dockerfile::parse(indoc!(r#"
      FROM $image
      ARG image="alpine:3.12"
    "#)).unwrap();

    let from: &FromInstruction = d.instructions
      .get(0).unwrap()
      .try_into().unwrap();

    assert_eq!(
      from.image_parsed.resolve_vars(&d),
      None
    );
  }
}
