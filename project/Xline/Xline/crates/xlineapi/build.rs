fn main() {
    // during the xlinerpc migration we still build the proto files with
    // tonic-build. once the generated sources are checked into the
    // repository we can drop the dependency entirely and remove this
    // build script. an environment variable is provided to skip the
    // build step so CI or developers can opt‑out while the transition is
    // in progress.
    if std::env::var("XLINERPC_SKIP_PROTO").is_ok() {
        println!("cargo:warning=skipping proto compilation (XLINERPC_SKIP_PROTO)");
        return;
    }

    tonic_build::configure()
        .type_attribute(".", "#[derive(serde::Deserialize, serde::Serialize)]")
        .compile_protos(
            &[
                "proto/src/kv.proto",
                "proto/src/rpc.proto",
                "proto/src/auth.proto",
                "proto/src/v3lock.proto",
                "proto/src/lease.proto",
                "proto/src/xline-command.proto",
                "proto/src/xline-error.proto",
            ],
            &["./proto/src"],
        )
        .unwrap_or_else(|e| panic!("Failed to compile proto, error is {:?}", e));
}
