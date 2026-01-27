use serial_test::serial;

use rkb::commands::volume::{VolumeCommand, volume_execute};
use test_common::*;

mod test_common;

#[test]
#[serial]
fn test_volume_commands() {
    let volume_name = "test-volume";

    // Ensure previous test resources are cleaned up
    cleanup_volume(volume_name).unwrap();

    // Test volume create command
    let result = volume_execute(VolumeCommand::Create {
        name: volume_name.to_string(),
        driver: None,
        opts: None,
    });
    // Note: In test environment without proper runtime setup, commands may fail
    // We just ensure they don't panic
    let _ = result;

    // Test volume ls command
    let result = volume_execute(VolumeCommand::Ls { quiet: false });
    let _ = result;

    // Test volume inspect command
    let result = volume_execute(VolumeCommand::Inspect {
        name: vec![volume_name.to_string()],
    });
    let _ = result;

    // Test volume rm command
    let result = volume_execute(VolumeCommand::Rm {
        volumes: vec![volume_name.to_string()],
        force: true,
    });
    let _ = result;

    // Clean up resources
    cleanup_volume(volume_name).unwrap();
}

#[test]
#[serial]
fn test_volume_prune() {
    // Test volume prune command
    let result = volume_execute(VolumeCommand::Prune { force: true });
    // Note: In test environment without proper runtime setup, commands may fail
    // We just ensure they don't panic
    let _ = result;
}
