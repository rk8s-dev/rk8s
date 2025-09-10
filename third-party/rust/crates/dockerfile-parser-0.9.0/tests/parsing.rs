// (C) Copyright 2019-2020 Hewlett Packard Enterprise Development LP

extern crate dockerfile_parser;

use dockerfile_parser::*;
use indoc::indoc;
use pretty_assertions::assert_eq;

#[test]
fn parse_basic() -> Result<(), dockerfile_parser::Error> {
    let dockerfile = Dockerfile::parse(
        r#"
    FROM alpine:3.10

    RUN apk add --no-cache curl
  "#,
    )?;

    assert_eq!(dockerfile.instructions.len(), 2);

    assert_eq!(
        dockerfile.instructions[0],
        Instruction::From(FromInstruction {
            span: Span { start: 5, end: 21 },
            image: SpannedString {
                span: Span { start: 10, end: 21 },
                content: "alpine:3.10".into(),
            },
            image_parsed: ImageRef {
                registry: None,
                image: "alpine".into(),
                tag: Some("3.10".into()),
                hash: None
            },
            index: 0,
            alias: None,
            flags: vec![],
        })
    );

    assert_eq!(
        &dockerfile.instructions[1]
            .as_run()
            .unwrap()
            .as_shell()
            .unwrap()
            .to_string(),
        "apk add --no-cache curl"
    );

    Ok(())
}

#[test]
fn parse_multiline_shell() -> Result<(), dockerfile_parser::Error> {
    let dockerfile = Dockerfile::parse(indoc!(
        r#"
    RUN apk add --no-cache \
        curl

    RUN foo
  "#
    ))?;

    assert_eq!(dockerfile.instructions.len(), 2);

    // note: 9 spaces due to 1 before the \ + 8 for indent
    assert_eq!(
        &dockerfile.instructions[0]
            .as_run()
            .unwrap()
            .as_shell()
            .unwrap()
            .to_string(),
        "apk add --no-cache     curl"
    );

    assert_eq!(
        &dockerfile.instructions[1]
            .as_run()
            .unwrap()
            .as_shell()
            .unwrap()
            .to_string(),
        "foo"
    );

    Ok(())
}

#[test]
fn parse_multiline_exec() -> Result<(), dockerfile_parser::Error> {
    let dockerfile = Dockerfile::parse(
        r#"
    RUN ["apk", \
         "add", \
         "--no-cache", \
         "curl"]

    RUN foo
  "#,
    )?;

    assert_eq!(dockerfile.instructions.len(), 2);

    // note: 9 spaces due to 1 before the \ + 8 for indent
    assert_eq!(
        dockerfile.instructions[0]
            .as_run()
            .unwrap()
            .as_exec()
            .unwrap()
            .as_str_vec(),
        &["apk", "add", "--no-cache", "curl"]
    );

    assert_eq!(
        &dockerfile.instructions[1]
            .as_run()
            .unwrap()
            .as_shell()
            .unwrap()
            .to_string(),
        "foo"
    );

    Ok(())
}

#[test]
fn parse_label() -> Result<(), dockerfile_parser::Error> {
    let dockerfile = Dockerfile::parse(
        r#"
    LABEL foo=bar

    LABEL "foo"="bar"

    LABEL "foo=bar"=bar

    LABEL foo="bar\
          baz"

    RUN foo
  "#,
    )?;

    assert_eq!(dockerfile.instructions.len(), 5);

    assert_eq!(
        dockerfile.instructions[0].as_label().unwrap(),
        &LabelInstruction {
            span: Span::new(5, 18),
            labels: vec![Label::new(
                Span::new(11, 18),
                SpannedString {
                    span: Span::new(11, 14),
                    content: "foo".to_string(),
                },
                SpannedString {
                    span: Span::new(15, 18),
                    content: "bar".to_string(),
                },
            )]
        }
    );

    assert_eq!(
        dockerfile.instructions[1],
        Instruction::Label(LabelInstruction {
            span: Span::new(24, 41),
            labels: vec![Label::new(
                Span::new(30, 41),
                SpannedString {
                    span: Span::new(30, 35),
                    content: "foo".to_string(),
                },
                SpannedString {
                    span: Span::new(36, 41),
                    content: "bar".to_string(),
                },
            )]
        })
    );

    assert_eq!(
        dockerfile.instructions[2],
        Instruction::Label(LabelInstruction {
            span: Span::new(47, 66),
            labels: vec![Label::new(
                Span::new(53, 66),
                SpannedString {
                    span: Span::new(53, 62),
                    content: "foo=bar".to_string(),
                },
                SpannedString {
                    span: Span::new(63, 66),
                    content: "bar".to_string(),
                },
            )]
        })
    );

    assert_eq!(
        dockerfile.instructions[3],
        Instruction::Label(LabelInstruction {
            span: Span::new(72, 102),
            labels: vec![Label::new(
                Span::new(78, 102),
                SpannedString {
                    span: Span::new(78, 81),
                    content: "foo".to_string(),
                },
                SpannedString {
                    span: Span::new(82, 102),
                    content: "bar          baz".to_string(),
                },
            )]
        })
    );

    assert_eq!(
        &dockerfile.instructions[4]
            .as_run()
            .unwrap()
            .as_shell()
            .unwrap()
            .to_string(),
        "foo"
    );

    // ambiguous line continuation is an error
    assert!(Dockerfile::parse(
        r#"
    LABEL foo="bar\
          baz"\

    RUN foo
  "#
    )
    .is_err());

    Ok(())
}

#[test]
fn parse_comment() -> Result<(), dockerfile_parser::Error> {
    let dockerfile = Dockerfile::parse(
        r#"
    # lorem ipsum
    LABEL foo=bar
    #dolor sit amet
    # consectetur adipiscing elit

    # sed do eiusmod
    # Duis aute irure dolor
    # tempor incididunt ut labore
    LABEL "foo"="bar"
    # et dolore magna aliqua
    LABEL "foo=bar"=bar
    #Ut enim ad minim veniam
    LABEL foo="bar\
          baz"
    # quis nostrud exercitation

    # ullamco laboris nisi

    RUN foo

    ENV foo=a \
      # test comment


      bar=b

    run [ \
      "echo", \
      # hello world
      "hello", \
      "world" \
    ]

    run echo 'hello # world'
  "#,
    )?;

    assert_eq!(dockerfile.instructions.len(), 8);

    assert_eq!(
        &dockerfile.instructions[4]
            .as_run()
            .unwrap()
            .as_shell()
            .unwrap()
            .to_string(),
        "foo"
    );

    assert_eq!(
        dockerfile.instructions[5].as_env().unwrap().vars,
        vec![
            EnvVar::new(
                Span::new(396, 401),
                SpannedString {
                    span: Span::new(396, 399),
                    content: "foo".to_string(),
                },
                ((400, 401), "a")
            ),
            EnvVar::new(
                Span::new(433, 438),
                SpannedString {
                    span: Span::new(433, 436),
                    content: "bar".to_string(),
                },
                ((437, 438), "b")
            ),
        ]
    );

    assert_eq!(
        dockerfile.instructions[6]
            .as_run()
            .unwrap()
            .as_exec()
            .unwrap()
            .as_str_vec(),
        vec!["echo", "hello", "world"]
    );

    assert_eq!(
        dockerfile.instructions[7]
            .as_run()
            .unwrap()
            .as_shell()
            .unwrap()
            .to_string(),
        "echo 'hello # world'"
    );

    Ok(())
}

#[test]
fn parse_from_sha256_digest() -> Result<(), dockerfile_parser::Error> {
    let dockerfile = Dockerfile::parse(
        r#"
    FROM alpine@sha256:074d3636ebda6dd446d0d00304c4454f468237fdacf08fb0eeac90bdbfa1bac7 as foo
  "#,
    )?;

    assert_eq!(dockerfile.instructions.len(), 1);

    assert_eq!(
        dockerfile.instructions[0].as_from(),
        Some(&FromInstruction {
            index: 0,
            span: (5, 95).into(),
            image: SpannedString {
                span: Span { start: 10, end: 88 },
                content:
                    "alpine@sha256:074d3636ebda6dd446d0d00304c4454f468237fdacf08fb0eeac90bdbfa1bac7"
                        .into(),
            },
            image_parsed: ImageRef {
                registry: None,
                image: "alpine".into(),
                tag: None,
                hash: Some(
                    "sha256:074d3636ebda6dd446d0d00304c4454f468237fdacf08fb0eeac90bdbfa1bac7"
                        .into()
                ),
            },
            alias: Some(SpannedString {
                span: Span { start: 92, end: 95 },
                content: "foo".into(),
            }),
            flags: vec![],
        })
    );

    Ok(())
}
