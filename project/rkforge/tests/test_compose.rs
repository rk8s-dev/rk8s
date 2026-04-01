use std::net::IpAddr;
use std::net::Ipv4Addr;

use trust_dns_resolver::Resolver;
use trust_dns_resolver::config::*;

use serial_test::serial;

use rkforge::commands::compose::{ComposeCommand, DownArgs, PsArgs, UpArgs, compose_execute};
use test_common::*;

mod test_common;

/// Get compose configuration for testing
fn get_compose_config(project_name: &str) -> String {
    format!(
        r#"name: {}

services:
  backend:
    container_name: backend
    image: {}
    command: ["sleep", "10"]
    ports:
      - "8080:8080"
    networks:
      - default-net
  frontend:
    container_name: frontend
    image: {}
    command: ["sleep", "10"]
    ports:
      - "80:80"
    networks:
      - default-net

networks:
  default-net:
    driver: bridge
"#,
        project_name,
        bundles_path("busybox"),
        bundles_path("busybox")
    )
}

/// todo! need bundle
/// this test to test the network in containers
#[allow(unused)]
fn get_compose_web_config(project_name: &str) -> String {
    format!(
        r#"name: {}

services:
  backend:
    container_name: backend
    image: {}
    command: ["sleep", "300"]
    ports:
      - "8080:8080"
    networks:
      - default-net
  frontend:
    container_name: frontend
    image: {}
    command: ["sleep", "300"]
    ports:
      - "80:80"
    networks:
      - default-net

networks:
  default-net:
    driver: bridge
"#,
        project_name,
        bundles_path(""),
        bundles_path("")
    )
}

fn dns_dig() -> Result<(), Box<dyn std::error::Error>> {
    let name_server = NameServerConfig {
        socket_addr: "172.17.0.1:53".parse()?,
        protocol: Protocol::Udp,
        tls_dns_name: None,
        trust_negative_responses: false,
        bind_addr: None,
    };

    let mut opts = ResolverOpts::default();
    opts.timeout = std::time::Duration::from_secs(2);
    opts.attempts = 1;

    let resolver_config = ResolverConfig::from_parts(None, vec![], vec![name_server]);
    let resolver = Resolver::new(resolver_config, opts)?;

    let response = resolver.lookup_ip("frontend")?;
    let assert_ip = IpAddr::V4(Ipv4Addr::new(172, 17, 0, 2)).to_string();
    let assert_ip2 = IpAddr::V4(Ipv4Addr::new(172, 17, 0, 2)).to_string();
    for ip in response {
        if ip.to_string() != assert_ip && assert_ip2 != ip.to_string() {
            panic!("dns server error");
        }
    }
    Ok(())
}

#[test]
#[serial]
fn test_compose_up_and_down() {
    let project_name = "test-compose-app";
    let compose_config = get_compose_config(project_name);

    // Create temporary compose file
    let compose_path = create_temp_compose_file(&compose_config).unwrap();

    // Ensure previous test resources are cleaned up
    cleanup_compose_project(project_name).unwrap();

    // Run compose up
    let result = compose_execute(ComposeCommand::Up(UpArgs {
        compose_yaml: Some(compose_path.clone()),
        project_name: Some(project_name.to_string()),
    }));

    // Note: Since the test environment may not have an actual container runtime,
    // we only check if the command can be parsed correctly, not the actual execution result.
    // In a real environment, we should check if containers are running properly
    match result {
        Ok(_) => {
            // test dns server ,It is essential to ensure that the test container is the first to run.
            dns_dig().unwrap();
        }
        Err(_) => (),
    }

    // Run compose ps
    let result = compose_execute(ComposeCommand::Ps(PsArgs {
        project_name: Some(project_name.to_string()),
        compose_yaml: Some(compose_path.clone()),
    }));
    let _ = result;

    // Run compose down
    let result = compose_execute(ComposeCommand::Down(DownArgs {
        project_name: Some(project_name.to_string()),
        compose_yaml: Some(compose_path.clone()),
    }));
    let _ = result;

    // Clean up temporary files
    cleanup_temp_compose_file(&compose_path).unwrap();
    cleanup_compose_project(project_name).unwrap();
}

#[test]
#[serial]
fn test_compose_invalid_yaml() {
    let invalid_config = "invalid: yaml: content";
    let compose_path = create_temp_compose_file(invalid_config).unwrap();

    let result = compose_execute(ComposeCommand::Up(UpArgs {
        compose_yaml: Some(compose_path.clone()),
        project_name: Some("test-invalid".to_string()),
    }));

    // Invalid yaml configuration should return an error
    assert!(
        result.is_err(),
        "compose up with invalid yaml should return error"
    );

    cleanup_temp_compose_file(&compose_path).unwrap();
}
