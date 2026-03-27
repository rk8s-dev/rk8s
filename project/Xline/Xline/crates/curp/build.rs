fn main() {
    let mut config = prost_build::Config::new();
    // NOTE: prost_build requires the full proto path with package prefix
    config.type_attribute(
        ".commandpb.ProposeConfChangeRequest.ConfChange",
        "#[derive(serde::Deserialize, serde::Serialize)]",
    );
    config
        .compile_protos(
            &["./proto/common/src/curp-command.proto"],
            &["./proto/common/src"],
        )
        .unwrap_or_else(|e| panic!("Failed to compile proto: {e:?}"));

    let mut inner_config = prost_build::Config::new();
    inner_config.bytes([".inner_messagepb.InstallSnapshotRequest"]);
    inner_config
        .compile_protos(&["./proto/inner_message.proto"], &["./proto/"])
        .unwrap_or_else(|e| panic!("Failed to compile proto: {e:?}"));
}
