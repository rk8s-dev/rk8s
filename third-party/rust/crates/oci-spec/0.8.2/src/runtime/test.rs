#[cfg(test)]
use super::*;

#[test]
fn serialize_and_deserialize_spec() {
    let spec: Spec = Default::default();
    let json_string = serde_json::to_string(&spec).unwrap();
    let new_spec = serde_json::from_str(&json_string).unwrap();
    assert_eq!(spec, new_spec);
}

#[test]
fn test_linux_device_cgroup_to_string() {
    let ldc = LinuxDeviceCgroupBuilder::default()
        .allow(true)
        .typ(LinuxDeviceType::B)
        .access("rwm".to_string())
        .build()
        .expect("build device cgroup");
    assert_eq!(ldc.to_string(), "b *:* rwm");

    let ldc = LinuxDeviceCgroupBuilder::default()
        .allow(true)
        .typ(LinuxDeviceType::A)
        .major(1)
        .minor(9)
        .access("rwm".to_string())
        .build()
        .expect("build device cgroup");
    assert_eq!(ldc.to_string(), "a 1:9 rwm");
}

#[test]
fn test_load_sample_spec() {
    let fixture_path = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("src/runtime/test/fixture/sample.json");
    let err = Spec::load(fixture_path);
    assert!(err.is_ok(), "failed to load spec: {err:?}");
}

#[test]
fn test_load_sample_windows_spec() {
    let fixture_path = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("src/runtime/test/fixture/sample_windows.json");
    let err = Spec::load(fixture_path);
    assert!(err.is_ok(), "failed to load spec: {err:?}");
}

#[test]
fn test_linux_netdevice_lifecycle() {
    let fixture_path = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("src/runtime/test/fixture/sample.json");

    let spec =
        Spec::load(fixture_path).unwrap_or_else(|err| panic!("Failed to load spec: {}", err));

    let net_devices = spec.linux.as_ref().unwrap().net_devices().as_ref().unwrap();
    assert_eq!(net_devices.len(), 3);

    // Get eth0
    let eth0 = net_devices.get("eth0").unwrap();
    assert_eq!(eth0.name().as_ref(), Some(&"container_eth0".to_string()));

    // Get ens4
    let ens4 = net_devices.get("ens4").unwrap();
    assert_eq!(ens4.name().as_ref(), None);

    // Get ens5
    let ens5 = net_devices.get("ens5").unwrap();
    assert_eq!(ens5.name().as_ref(), None);

    // Set ens6
    let mut net_devices = net_devices.clone();
    let ens6 = LinuxNetDeviceBuilder::default()
        .name("ens6".to_string())
        .build()
        .unwrap();
    net_devices.insert("ens6".to_string(), ens6);
    assert_eq!(net_devices.len(), 4);

    // Get ens6
    let ens6 = net_devices.get("ens6").unwrap();
    assert_eq!(ens6.name().as_ref(), Some(&"ens6".to_string()));

    // Change name of ens6
    let ens6 = net_devices.get_mut("ens6").unwrap();
    ens6.set_name(Some("ens6_container".to_string()));
    assert_eq!(ens6.name().as_ref(), Some(&"ens6_container".to_string()));
}
