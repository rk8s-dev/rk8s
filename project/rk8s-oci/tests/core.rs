use rk8s_oci::{
    Descriptor, Digest, ImageReference, MediaType, Platform, ReferenceKind, RegistryMirrorConfig,
};

#[test]
fn parses_and_normalizes_docker_hub_references() {
    let reference: ImageReference = "alpine:3.20".parse().unwrap();
    assert_eq!(reference.registry(), "docker.io");
    assert_eq!(reference.repository(), "library/alpine");
    assert_eq!(reference.tag(), Some("3.20"));
    assert_eq!(reference.digest(), None);
    assert_eq!(reference.kind(), ReferenceKind::Tag);
    assert_eq!(reference.to_string(), "docker.io/library/alpine:3.20");

    let reference: ImageReference = "busybox".parse().unwrap();
    assert_eq!(reference.to_string(), "docker.io/library/busybox:latest");
}

#[test]
fn parses_registry_and_digest_references() {
    let reference: ImageReference =
        "ghcr.io/rk8s-dev/demo@sha256:aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa"
            .parse()
            .unwrap();

    assert_eq!(reference.registry(), "ghcr.io");
    assert_eq!(reference.repository(), "rk8s-dev/demo");
    assert_eq!(
        reference.digest().unwrap().as_str(),
        "sha256:aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa"
    );
    assert_eq!(reference.kind(), ReferenceKind::Digest);
    assert_eq!(
        reference.to_string(),
        "ghcr.io/rk8s-dev/demo@sha256:aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa"
    );
}

#[test]
fn rejects_invalid_references() {
    assert!("".parse::<ImageReference>().is_err());
    assert!("docker.io/".parse::<ImageReference>().is_err());
    assert!(
        "docker.io/library/alpine:"
            .parse::<ImageReference>()
            .is_err()
    );
    assert!(
        "docker.io/library/alpine@"
            .parse::<ImageReference>()
            .is_err()
    );
    assert!(
        "docker.io/library/alpine:latest@sha256:aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa"
            .parse::<ImageReference>()
            .is_err()
    );
}

#[test]
fn mirror_config_rewrites_matching_registries() {
    let config = RegistryMirrorConfig::new()
        .with_docker_hub_mirror("mirror.example.com")
        .unwrap()
        .with_registry_mirror("ghcr.io", "ghcr-mirror.example.com")
        .unwrap();

    let docker_hub: ImageReference = "alpine:latest".parse().unwrap();
    assert_eq!(
        config.rewrite(&docker_hub).unwrap().to_string(),
        "mirror.example.com/library/alpine:latest"
    );

    let ghcr: ImageReference = "ghcr.io/rk8s-dev/demo:v1".parse().unwrap();
    assert_eq!(
        config.rewrite(&ghcr).unwrap().to_string(),
        "ghcr-mirror.example.com/rk8s-dev/demo:v1"
    );
}

#[test]
fn descriptor_and_platform_serialize_like_oci_json() {
    let descriptor = Descriptor::new(
        MediaType::OciImageManifest,
        Digest::parse("sha256:bbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb")
            .unwrap(),
        1234,
    )
    .with_platform(Platform::linux_amd64());

    let value = serde_json::to_value(&descriptor).unwrap();
    assert_eq!(value["mediaType"], MediaType::OciImageManifest.as_str());
    assert_eq!(value["size"], 1234);
    assert_eq!(value["platform"]["os"], "linux");
    assert_eq!(value["platform"]["architecture"], "amd64");
}
