#[cfg(feature = "build-server")]
fn should_build_server() -> bool {
    true
}

#[cfg(not(feature = "build-server"))]
fn should_build_server() -> bool {
    false
}

fn main() {
    let proto_root = "proto";
    println!("cargo:rerun-if-changed={proto_root}");

    tonic_build::configure()
        .build_server(should_build_server())
        .compile_protos(
            &[
                "proto/auth.proto",
                "proto/kv.proto",
                "proto/rpc.proto",
                "proto/v3election.proto",
                "proto/v3lock.proto",
            ],
            &[proto_root],
        )
        .expect("Failed to compile proto files");
}
