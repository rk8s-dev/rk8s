use serial_test::serial;
use uuid::Uuid;

use rkforge::commands::container::{ContainerCommand, container_execute};

mod test_common;

#[test]
#[serial]
fn test_container_commands() {
    let container_name = format!("rkforge-nonexistent-{}", Uuid::new_v4());

    let _ = container_execute(ContainerCommand::List {
        quiet: None,
        format: None,
    });

    let _ = container_execute(ContainerCommand::State {
        container_name: container_name.clone(),
    });

    let _ = container_execute(ContainerCommand::Delete {
        container_name,
    });
}
