# Import Cargo packages with Reindeer

## Install Reindeer

Reindeer builds with Cargo in the normal way. It has no unusual build-time dependencies. Therefore, you can use Cargo to not only build Reindeer, but to install it as well.

```
cargo install --locked --git https://github.com/facebookincubator/reindeer reindeer
```

## Manage dependencies

You're working away on your code, and you suddenly need to use some third-party crates. You might want to follow the workflow below.

1. Add the specification to `[dependencies]` in `third-party/Cargo.toml`, as you would if this were a Cargo project. You can use all the usual options, such as adding features, defining a local name, and so on.
2. Run `reindeer --third-party-dir third-party vendor`. This will resolve the new dependencies (creating or updating `Cargo.lock`), vendor all the new code in the `third-party/vendor` directory (also deleting unused code).
3. Run `reindeer --third-party-dir third-party buckify`. This will analyze the Cargo dependencies and (re)generate the BUCK file accordingly. If this succeeds silently then there's a good chance that nothing more is needed.
4. Do a test build with `buck build //third-party:new-package#check` to make sure it is basically buildable.

