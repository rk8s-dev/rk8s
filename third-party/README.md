# Third-party Dependency Management

Buckal (a.k.a. `cargo-buckal`) is a Cargo plugin that enables seamless migration of Cargo packages to the Buck2 build system. It automatically resolves Cargo dependencies and generates BUCK files, allowing developers to use Buck2 as effortlessly as they would use `cargo`, with minimal manual configuration required.

## Install Buckal

The rk8s project leverages Buckal for managing third-party dependencies and generating Buck2 build configurations. Install the Buckal plugin using the following command:

```bash
cargo install --git https://github.com/buck2hub/cargo-buckal.git
```

NOTE: Buckal requires Buck2 and Python 3. Please ensure both are installed on your system before proceeding.

## Manage dependencies

You're working away on your code, and you suddenly need to use some third-party crates. With Buckal, you can keep using the familiar Cargo workflow. Simply prepend `buckal` to your usual command, and stay right in your project directory.

To add a dependency, run:

```bash
cargo buckal add <DEP>[@<VERSION>] [--features <FEATURES>]
```

This command adds the specified crate to your `Cargo.toml` and automatically generates or updates the corresponding BUCK build rules for Buck2.

If you prefer editing `Cargo.toml` manually, just make your changes as usual and then synchronize them to Buck2 by running:

```bash
cargo buckal migrate
```

This parses your dependencies and updates only the BUCK files that need to change, keeping your build configuration in sync with your Cargo manifest — automatically and without manual effort.
