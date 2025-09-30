---
sidebar_position: 2
title: Install
---

# Install RustyVault

RustyVault must be installed properly in your environment before it actually works. Currently RustyVault is only available by source code. RustyVault can be used as an application or a library, thus:

1. RustyVault is available to compile from source code only, or
2. RustyVault is availabe on [crates.io](https://crates.io/crates/rusty_vault) for other Rust projects.

This document is about how to build and install RustyVault in the application form. For the library form, please go to [docs.rs](https://docs.rs/rusty_vault/latest/rusty_vault) for more information.

## Operating System

RustyVault is supposed to work on the following operating systems:

* Linux
* macOS
* Windows (experimental)

In this document, macOS is used as the demonstration operating system.

## Prerequisite

RustyVault is developed in [Rust](https://rust-lang.org) programming language, so Rust must be properly installed in your environment before building RustyVault.

Read [this](https://www.rust-lang.org/tools/install) to make Rust work for you.

## Build from Source

Clone the latest RustyVault source code from Github:

~~~bash
git clone https://github.com/Tongsuo-Project/RustyVault.git
~~~

Then you have a directory called RustyVault now. Change directory into it.

~~~bash
cd RustyVault
~~~

Simply build the binary by using the tool Cargo.

~~~bash
cargo build
~~~

Rust toolchain is responsible for taking care of almost everything during the build process. After RustyVault is successfully built, you get a bundle of files in the `RustyVault/target/debug` directory. There will be a executable file called `rvault`, which is the application of RustyVault.

## Verify RustyVault

Simply run the following command:

~~~bash
target/debug/rvault --help
~~~

And you will get a response similar to:

~~~bash
A secure and high performance secret management software that is compatible with Hashicorp Vault.

Usage: rvault [COMMAND]

Commands:
  server  Start a rusty_vault server
  status  Print seal and HA status
  help    Print this message or the help of the given subcommand(s)

Options:
  -h, --help     Print help
  -V, --version  Print version
~~~

That means you now have a ready-to-use RustyVault binary.