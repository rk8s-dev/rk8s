# dockerfile-parser-rs

[![docs.rs](https://docs.rs/dockerfile-parser/badge.svg)](https://docs.rs/dockerfile-parser/)

A pure Rust library for parsing and inspecting Dockerfiles, useful for
performing static analysis, writing linters, and creating automated tooling
around Dockerfiles. It uses a proper grammar and can provide useful syntax
errors in addition to a full syntax tree.

## Limitations

 * Buildkit parser directives are not handled at all.
 * Unknown instructions are parsed as `MiscInstruction` rather than producing
   an explicit error. A number of valid but less interesting Docker instructions
   are handled this way, e.g. `ONBUILD`, `MAINTAINER`, etc. See notes in
   [the grammar](./src/dockerfile_parser.pest) for details.

## Usage

See [`./examples`](./examples) for a few usage examples, including a small
utility to dump a Dockerfile's structure:

```bash
$ cargo run --example stages Dockerfile.test
    Finished dev [unoptimized + debuginfo] target(s) in 0.03s
     Running `target/debug/dockerfile Dockerfile.test`
global arg: ArgInstruction { name: "foo", value: None }
stages:
  stage #0
    From(FromInstruction { image: "foo:443/bar", index: 0, alias: None })
  stage #1
    From(FromInstruction { image: "localhost/foo", index: 1, alias: None })
  stage #2
    From(FromInstruction { image: "example.com/foo:bar", index: 2, alias: None })
  stage #3
    From(FromInstruction { image: "alpine:3.10", index: 3, alias: None })
  stage #4
    From(FromInstruction { image: "foo/bar", index: 4, alias: None })
  stage #5
    From(FromInstruction { image: "foo/bar:baz", index: 5, alias: None })
  stage #6
    From(FromInstruction { image: "hello-world:test", index: 6, alias: Some("foo") })
  stage #7
    From(FromInstruction { image: "fooasdf", index: 7, alias: Some("bar-baz") })
    Run(Exec(["foo", "bar", "echo \"hello $world\""]))
    Run(Shell("foo bar baz"))
    Arg(ArgInstruction { name: "image", value: Some("alpine:3.10") })
  stage #8
    From(FromInstruction { image: "$image", index: 8, alias: None })
  stage #9
    From(FromInstruction { image: "alpine:3.10", index: 9, alias: Some("foo") })
    Run(Exec(["foo", "bar"]))
    Run(Shell("foo bar baz     qux     qup"))
    Copy(CopyInstruction { flags: [CopyFlag { name: "from", value: "foo" }], sources: ["/foo/bar", "/foo/baz"], destination: "/qux/" })
    Entrypoint(Shell("foo bar baz"))
    Entrypoint(Exec(["foo", "bar", "baz"]))
    Cmd(Shell("foo bar"))
    Cmd(Exec(["foo", "bar"]))
    Copy(CopyInstruction { flags: [], sources: ["foo"], destination: "bar" })
    Copy(CopyInstruction { flags: [CopyFlag { name: "from", value: "0" }], sources: ["/foo"], destination: "/bar" })
    Misc(MiscInstruction { instruction: "other", arguments: "foo bar" })
    Misc(MiscInstruction { instruction: "other", arguments: "foo bar" })
    Env(EnvInstruction([EnvVar { key: "foo", value: "bar baz   qux" }]))
    Env(EnvInstruction([EnvVar { key: "foo", value: "bar" }, EnvVar { key: "baz", value: "qux" }]))
    Env(EnvInstruction([EnvVar { key: "zxcv", value: "asdf" }]))
    Env(EnvInstruction([EnvVar { key: "foo", value: "bar zxcv" }, EnvVar { key: "baz", value: "qux" }, EnvVar { key: "zxcv", value: "asdf\"qwerty" }, EnvVar { key: "zxcv", value: "zxcvzxvb" }]))
    Label(LabelInstruction([Label { name: "foo", value: "bar" }]))
    Label(LabelInstruction([Label { name: "foo", value: "bar" }]))
    Label(LabelInstruction([Label { name: "foo bar", value: "baz qux" }]))
    Label(LabelInstruction([Label { name: "foo  bar", value: "baz\"  qux" }]))
    Misc(MiscInstruction { instruction: "foo", arguments: "bar" })
```

### Splicing

Some instruction structs also include character spans for various attributes (or
the entire instruction). The included splicing utility can be used to rewrite
these spans while preserving other user formatting within the file. For example,
this can be used to implement a utility that automatically updates image
versions, or to provide automated fixes for detected lints.

See [`examples/splice.rs`](./examples/splice.rs) for an example that rewrites
image references.

## Contributing

Bug reports, feature requests, and pull requests are welcome! Be sure to read
though the [code of conduct] for some pointers to get started.

Note that - as mentioned in the code of conduct - code contributions must
indicate that you accept the [Developer Certificate of Origin][dco],
essentially asserting you have the necessary rights to submit the code you're
contributing under the project's license (MIT). If you agree, simply pass `-s`
to `git commit`:

```bash
git commit -s [...]
```

... and Git will automatically append the required `Signed-off-by: ...` to the
end of your commit message.

[code of conduct]: ./CODE_OF_CONDUCT.md
[dco]: https://developercertificate.org/
