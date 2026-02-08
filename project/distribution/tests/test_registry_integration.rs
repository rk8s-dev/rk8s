//! Integration tests for the OCI Distribution registry.
//!
//! These tests run inside a QEMU/KVM virtual machine using the qlean crate,
//! following the same test flow as `project/distribution/test_registry.sh` but using HTTP APIs
//! instead of Docker commands.

use std::path::{Path, PathBuf};
use std::sync::Once;
use std::time::Duration;

use anyhow::{Context, Result};
use qlean::{Distro, Machine, MachineConfig, create_image, with_machine};
use tracing_subscriber::EnvFilter;

const REGISTRY_HOST: &str = "127.0.0.1";
const REGISTRY_PORT: u16 = 8968;
const POSTGRES_USER: &str = "postgres";
const POSTGRES_PASSWORD: &str = "password";
const POSTGRES_DB: &str = "postgres";

fn tracing_subscriber_init() {
    static INIT: Once = Once::new();
    INIT.call_once(|| {
        tracing_subscriber::fmt()
            .with_env_filter(EnvFilter::from_default_env())
            .init();
    });
}

/// Helper to run a command and check its exit status
async fn exec_check(vm: &mut Machine, cmd: &str) -> Result<String> {
    let result = vm.exec(cmd).await?;
    if !result.status.success() {
        let stderr = String::from_utf8_lossy(&result.stderr);
        let stdout = String::from_utf8_lossy(&result.stdout);
        anyhow::bail!(
            "Command '{}' failed with exit code {:?}\nstdout: {}\nstderr: {}",
            cmd,
            result.status.code(),
            stdout,
            stderr
        );
    }
    Ok(String::from_utf8_lossy(&result.stdout).to_string())
}

/// Helper to run a command and expect it to fail
#[allow(dead_code)]
async fn exec_expect_fail(vm: &mut Machine, cmd: &str) -> Result<()> {
    let result = vm.exec(cmd).await?;
    if result.status.success() {
        anyhow::bail!("Command '{}' succeeded but was expected to fail", cmd);
    }
    Ok(())
}

/// Wait for a service to be ready by polling a URL
/// The /v2/ endpoint returns 401 without auth, which is still a valid response indicating the service is up
async fn wait_for_service(vm: &mut Machine, url: &str, timeout_secs: u64) -> Result<()> {
    let start = std::time::Instant::now();
    let timeout = Duration::from_secs(timeout_secs);

    loop {
        // Use -w to get the HTTP status code, accept 200, 401, 403 as valid (service is running)
        let result = vm
            .exec(&format!(
                "curl -s -o /dev/null -w '%{{http_code}}' '{}'",
                url
            ))
            .await?;

        let status_code = String::from_utf8_lossy(&result.stdout).trim().to_string();
        tracing::debug!("Service check returned status: {}", status_code);

        // 200, 401, 403 all indicate the service is running
        if status_code == "200" || status_code == "401" || status_code == "403" {
            tracing::info!("Service is ready (status: {})", status_code);
            return Ok(());
        }

        if start.elapsed() > timeout {
            // Get more details about the failure
            let debug_result = vm.exec(&format!("curl -v '{}' 2>&1 || true", url)).await?;
            let debug_output = String::from_utf8_lossy(&debug_result.stdout);
            tracing::error!("Service check debug output:\n{}", debug_output);
            anyhow::bail!(
                "Timeout waiting for service at {} (last status: {})",
                url,
                status_code
            );
        }

        tokio::time::sleep(Duration::from_secs(1)).await;
    }
}

/// Setup PostgreSQL in the VM
async fn setup_postgres(vm: &mut Machine) -> Result<()> {
    tracing::info!("Installing PostgreSQL...");
    exec_check(vm, "apt-get update -qq").await?;
    exec_check(vm, "DEBIAN_FRONTEND=noninteractive apt-get install -y -qq postgresql postgresql-contrib curl jq").await?;

    // Start PostgreSQL
    tracing::info!("Starting PostgreSQL...");
    exec_check(vm, "service postgresql start").await?;

    // Wait for PostgreSQL to be ready
    tokio::time::sleep(Duration::from_secs(2)).await;

    // Setup PostgreSQL user and database
    tracing::info!("Configuring PostgreSQL...");
    exec_check(
        vm,
        &format!(
            "su - postgres -c \"psql -c \\\"ALTER USER {} WITH PASSWORD '{}';\\\"\"",
            POSTGRES_USER, POSTGRES_PASSWORD
        ),
    )
    .await?;

    // Configure pg_hba.conf for password authentication
    exec_check(
        vm,
        "echo 'host all all 127.0.0.1/32 md5' >> /etc/postgresql/*/main/pg_hba.conf",
    )
    .await?;
    exec_check(vm, "service postgresql restart").await?;
    tokio::time::sleep(Duration::from_secs(2)).await;

    tracing::info!("PostgreSQL setup complete.");
    Ok(())
}

/// Setup and start the distribution service
async fn setup_distribution(vm: &mut Machine, binary_path: &Path) -> Result<()> {
    // Create registry root directory
    tracing::info!("Creating registry directory...");
    exec_check(vm, "mkdir -p /var/lib/oci-registry").await?;

    // Upload the distribution binary
    tracing::info!("Uploading distribution binary...");
    vm.upload(binary_path, Path::new("/usr/local/bin")).await?;
    exec_check(vm, "chmod +x /usr/local/bin/distribution").await?;

    // Set environment variables and start the service
    tracing::info!("Starting distribution service...");

    // Create env file
    let env_file_content = format!(
        r#"OCI_REGISTRY_URL=0.0.0.0
OCI_REGISTRY_PORT={}
OCI_REGISTRY_STORAGE=FILESYSTEM
OCI_REGISTRY_ROOTDIR=/var/lib/oci-registry
OCI_REGISTRY_PUBLIC_URL=http://{}:{}
POSTGRES_HOST=127.0.0.1
POSTGRES_PORT=5432
POSTGRES_USER={}
POSTGRES_PASSWORD={}
POSTGRES_DB={}
JWT_SECRET=secret
JWT_LIFETIME_SECONDS=3600
RUST_LOG=info
GITHUB_CLIENT_ID=test_client_id
GITHUB_CLIENT_SECRET=test_client_secret"#,
        REGISTRY_PORT, REGISTRY_HOST, REGISTRY_PORT, POSTGRES_USER, POSTGRES_PASSWORD, POSTGRES_DB
    );

    vm.write(
        std::path::Path::new("/etc/distribution.env"),
        env_file_content.as_bytes(),
    )
    .await?;

    // Create startup script
    let startup_script = r#"#!/bin/bash
set -a
source /etc/distribution.env
set +a
exec /usr/local/bin/distribution
"#;
    vm.write(
        std::path::Path::new("/usr/local/bin/start-distribution.sh"),
        startup_script.as_bytes(),
    )
    .await?;
    exec_check(vm, "chmod +x /usr/local/bin/start-distribution.sh").await?;

    // Start distribution in background using the script
    exec_check(
        vm,
        "nohup /usr/local/bin/start-distribution.sh > /var/log/distribution.log 2>&1 &",
    )
    .await?;

    // Give it a moment to start
    tokio::time::sleep(Duration::from_secs(3)).await;

    // Check if process is running
    let ps_output = exec_check(vm, "ps aux | grep distribution || true").await?;
    tracing::info!("Process status: {}", ps_output);

    // Check logs for errors
    let log_output = exec_check(
        vm,
        "cat /var/log/distribution.log 2>/dev/null || echo 'No log file'",
    )
    .await?;
    tracing::info!("Distribution log:\n{}", log_output);

    // Check if the binary can run
    let file_output = exec_check(vm, "file /usr/local/bin/distribution").await?;
    tracing::info!("Binary file type: {}", file_output);

    // Wait for service to be ready
    let api_url = format!("http://{}:{}/v2/", REGISTRY_HOST, REGISTRY_PORT);
    tracing::info!(
        "Waiting for distribution service to be ready at {}...",
        api_url
    );
    wait_for_service(vm, &api_url, 60).await?;

    tracing::info!("Distribution service is ready.");
    Ok(())
}

/// Get auth token for a user
async fn get_auth_token(vm: &mut Machine, username: &str, password: &str) -> Result<String> {
    let auth_url = format!("http://{}:{}/auth/token", REGISTRY_HOST, REGISTRY_PORT);
    let output = exec_check(
        vm,
        &format!(
            "curl -sf -u '{}:{}' '{}' | jq -r .token",
            username, password, auth_url
        ),
    )
    .await?;
    Ok(output.trim().to_string())
}

/// Register a new user (debug endpoint)
async fn register_user(vm: &mut Machine, username: &str, password: &str) -> Result<()> {
    let debug_url = format!("http://{}:{}/debug/users", REGISTRY_HOST, REGISTRY_PORT);
    exec_check(
        vm,
        &format!(
            r#"curl -sf -X POST -H "Content-Type: application/json" -d '{{"username": "{}", "password": "{}"}}' '{}'"#,
            username, password, debug_url
        ),
    ).await?;
    Ok(())
}

/// Test anonymous user permissions
async fn test_anonymous_user(vm: &mut Machine) -> Result<()> {
    tracing::info!("--- Running Test Case 1: Anonymous User Permissions ---");

    let api_url = format!("http://{}:{}", REGISTRY_HOST, REGISTRY_PORT);

    // Get anonymous token
    let anon_token = exec_check(
        vm,
        &format!("curl -sf '{}/auth/token' | jq -r .token", api_url),
    )
    .await?;
    let anon_token = anon_token.trim();

    // Try to initiate a blob upload as anonymous (should fail with 401 or 403)
    tracing::info!("Attempting to start blob upload as anonymous user (should fail)...");
    let result = vm.exec(&format!(
        r#"curl -s -w "%{{http_code}}" -o /dev/null -X POST -H "Authorization: Bearer {}" '{}/v2/anonymous/test/blobs/uploads/'"#,
        anon_token, api_url
    )).await?;

    let status_code = String::from_utf8_lossy(&result.stdout).trim().to_string();
    tracing::info!("Anonymous push attempt returned status: {}", status_code);

    // Anonymous users should not be able to push (expect 401 or 403)
    if status_code == "202" || status_code == "200" {
        anyhow::bail!("SECURITY RISK: Anonymous user was able to initiate blob upload!");
    }

    tracing::info!("[SUCCESS] Anonymous user correctly denied push access.");
    Ok(())
}

/// Test cross-namespace push permissions
async fn test_cross_namespace_push(
    vm: &mut Machine,
    user_a: &str,
    pass_a: &str,
    user_b: &str,
    _pass_b: &str,
) -> Result<()> {
    tracing::info!("--- Running Test Case 2: Cross-Namespace Push Permissions ---");

    let api_url = format!("http://{}:{}", REGISTRY_HOST, REGISTRY_PORT);

    // Get token for User A
    let token_a = get_auth_token(vm, user_a, pass_a).await?;

    // User A tries to initiate blob upload in their own namespace (should succeed)
    tracing::info!("User A attempting to push to their own namespace...");
    let result = vm.exec(&format!(
        r#"curl -s -w "%{{http_code}}" -o /dev/null -X POST -H "Authorization: Bearer {}" '{}/v2/{}/test-image/blobs/uploads/'"#,
        token_a, api_url, user_a
    )).await?;

    let status_code = String::from_utf8_lossy(&result.stdout).trim().to_string();
    tracing::info!(
        "User A push to own namespace returned status: {}",
        status_code
    );

    if status_code != "202" {
        anyhow::bail!(
            "User A failed to initiate blob upload to their own namespace (status: {})",
            status_code
        );
    }
    tracing::info!("[SUCCESS] User A successfully initiated upload to their own namespace.");

    // User A tries to initiate blob upload in User B's namespace (should fail)
    tracing::info!("User A attempting to push to User B's namespace (should fail)...");
    let result = vm.exec(&format!(
        r#"curl -s -w "%{{http_code}}" -o /dev/null -X POST -H "Authorization: Bearer {}" '{}/v2/{}/illegal-push/blobs/uploads/'"#,
        token_a, api_url, user_b
    )).await?;

    let status_code = String::from_utf8_lossy(&result.stdout).trim().to_string();
    tracing::info!(
        "User A push to User B namespace returned status: {}",
        status_code
    );

    if status_code == "202" || status_code == "200" {
        anyhow::bail!("SECURITY RISK: User A was able to push to User B's namespace!");
    }

    tracing::info!("[SUCCESS] User A correctly denied push access to User B's namespace.");
    Ok(())
}

/// Push a minimal blob and manifest to create a repository
async fn push_minimal_image(
    vm: &mut Machine,
    token: &str,
    namespace: &str,
    repo: &str,
    tag: &str,
) -> Result<()> {
    let api_url = format!("http://{}:{}", REGISTRY_HOST, REGISTRY_PORT);

    // Create a minimal blob (empty tar)
    exec_check(vm, "echo -n '' > /tmp/empty_blob").await?;
    let blob_digest = "sha256:e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855"; // sha256 of empty string

    // Initiate blob upload
    tracing::info!("Initiating blob upload for {}/{}...", namespace, repo);
    let location = exec_check(
        vm,
        &format!(
            r#"curl -s -D - -X POST -H "Authorization: Bearer {}" '{}/v2/{}/{}/blobs/uploads/' | grep -i '^location:' | tr -d '\r' | cut -d' ' -f2"#,
            token, api_url, namespace, repo
        ),
    ).await?;
    let location = location.trim();

    if location.is_empty() {
        anyhow::bail!("Failed to get upload location");
    }

    // Complete blob upload
    tracing::info!("Completing blob upload...");
    // Determine the separator: use '?' if location doesn't have query params, '&' otherwise
    let separator = if location.contains('?') { "&" } else { "?" };
    let upload_url = if location.starts_with("http") {
        format!("{}{}digest={}", location, separator, blob_digest)
    } else {
        format!("{}{}{}digest={}", api_url, location, separator, blob_digest)
    };
    tracing::debug!("Upload URL: {}", upload_url);

    exec_check(
        vm,
        &format!(
            r#"curl -sf -X PUT -H "Authorization: Bearer {}" -H "Content-Type: application/octet-stream" --data-binary @/tmp/empty_blob '{}'"#,
            token, upload_url
        ),
    ).await?;

    // Create a minimal manifest
    let config_digest = blob_digest;
    let manifest = format!(
        r#"{{
  "schemaVersion": 2,
  "mediaType": "application/vnd.docker.distribution.manifest.v2+json",
  "config": {{
    "mediaType": "application/vnd.docker.container.image.v1+json",
    "size": 0,
    "digest": "{}"
  }},
  "layers": []
}}"#,
        config_digest
    );

    // Push the manifest
    tracing::info!("Pushing manifest for {}:{}", repo, tag);
    exec_check(
        vm,
        &format!(
            r#"curl -sf -X PUT -H "Authorization: Bearer {}" -H "Content-Type: application/vnd.docker.distribution.manifest.v2+json" -d '{}' '{}/v2/{}/{}/manifests/{}'"#,
            token, manifest, api_url, namespace, repo, tag
        ),
    ).await?;

    tracing::info!("Successfully pushed {}/{}:{}", namespace, repo, tag);
    Ok(())
}

/// Test private/public pull permissions
async fn test_visibility_permissions(
    vm: &mut Machine,
    user_a: &str,
    pass_a: &str,
    user_b: &str,
    pass_b: &str,
) -> Result<()> {
    tracing::info!("--- Running Test Case 3: Private/Public Pull Permissions ---");

    let api_url = format!("http://{}:{}", REGISTRY_HOST, REGISTRY_PORT);

    // Get tokens
    let token_b = get_auth_token(vm, user_b, pass_b).await?;
    let token_a = get_auth_token(vm, user_a, pass_a).await?;

    // User B creates a private repository
    tracing::info!("User B creating private repository...");
    push_minimal_image(vm, &token_b, user_b, "private-repo", "v1").await?;

    // User B creates a public repository
    tracing::info!("User B creating public repository...");
    push_minimal_image(vm, &token_b, user_b, "public-repo", "v1").await?;

    // Set public-repo to public
    tracing::info!("User B setting repository to public...");
    exec_check(
        vm,
        &format!(
            r#"curl -sf -X PUT -H "Authorization: Bearer {}" -H "Content-Type: application/json" -d '{{"visibility": "public"}}' '{}/api/v1/{}/public-repo/visibility'"#,
            token_b, api_url, user_b
        ),
    ).await?;

    // User A tries to pull User B's private repo (should fail)
    tracing::info!("User A attempting to pull User B's private repo (should fail)...");
    let result = vm.exec(&format!(
        r#"curl -s -w "%{{http_code}}" -o /dev/null -H "Authorization: Bearer {}" '{}/v2/{}/private-repo/manifests/v1'"#,
        token_a, api_url, user_b
    )).await?;

    let status_code = String::from_utf8_lossy(&result.stdout).trim().to_string();
    tracing::info!("User A pull private repo returned status: {}", status_code);

    if status_code == "200" {
        anyhow::bail!("SECURITY RISK: User A was able to pull User B's private repo!");
    }
    tracing::info!("[SUCCESS] User A correctly denied access to User B's private repo.");

    // User A tries to pull User B's public repo (should succeed)
    tracing::info!("User A attempting to pull User B's public repo...");
    let result = vm.exec(&format!(
        r#"curl -s -w "%{{http_code}}" -o /dev/null -H "Authorization: Bearer {}" '{}/v2/{}/public-repo/manifests/v1'"#,
        token_a, api_url, user_b
    )).await?;

    let status_code = String::from_utf8_lossy(&result.stdout).trim().to_string();
    tracing::info!("User A pull public repo returned status: {}", status_code);

    if status_code != "200" {
        anyhow::bail!(
            "User A failed to pull User B's public repo (status: {})",
            status_code
        );
    }
    tracing::info!("[SUCCESS] User A successfully pulled User B's public repo.");

    Ok(())
}

/// Get the path to the distribution binary (debug build only)
///
/// Note: Only debug builds are supported because the /debug/users route
/// (used for user registration in tests) is only available when compiled
/// with debug_assertions enabled.
fn get_distribution_binary_path() -> Result<PathBuf> {
    // First, try CARGO_BIN_EXE_distribution which Cargo sets for integration tests
    if let Ok(path) = std::env::var("CARGO_BIN_EXE_distribution") {
        let path = PathBuf::from(path);
        if path.exists() {
            return Ok(path);
        }
    }

    // Fall back to manual path resolution, respecting CARGO_TARGET_DIR
    let manifest_dir = std::env::var("CARGO_MANIFEST_DIR").context("CARGO_MANIFEST_DIR not set")?;

    let target_dir = std::env::var("CARGO_TARGET_DIR")
        .map(PathBuf::from)
        .unwrap_or_else(|_| {
            PathBuf::from(&manifest_dir)
                .parent()
                .unwrap()
                .join("target")
        });

    // Only use debug build because /debug/users route is only available in debug mode
    let debug_path = target_dir.join("debug/distribution");

    if debug_path.exists() {
        return Ok(debug_path);
    }

    anyhow::bail!(
        "Distribution debug binary not found at {:?}. \
        Please build it with 'cargo build -p distribution'. \
        Note: Release builds are not supported because the /debug/users route \
        is only available in debug mode.",
        debug_path
    );
}

#[tokio::test]
async fn test_registry_integration() -> Result<()> {
    tracing_subscriber_init();

    // Load .env file
    let manifest_dir = std::env::var("CARGO_MANIFEST_DIR").context("CARGO_MANIFEST_DIR not set")?;
    let env_path = PathBuf::from(&manifest_dir).join(".env");
    if env_path.exists() {
        dotenvy::from_path(&env_path).ok();
        tracing::info!("Loaded .env from {:?}", env_path);
    }

    // Get distribution binary path
    let binary_path = get_distribution_binary_path()?;
    tracing::info!("Using distribution binary at {:?}", binary_path);

    // Create VM image and config
    tracing::info!("Creating VM image...");
    let image = create_image(Distro::Debian, "debian-13-generic-amd64").await?;
    let config = MachineConfig {
        core: 2,
        mem: 2048,
        disk: Some(10),
        clear: true,
    };

    // Execute tests in the virtual machine
    with_machine(&image, &config, |vm| {
        Box::pin(async move {
            tracing::info!("VM started successfully");

            // Setup PostgreSQL
            setup_postgres(vm).await?;

            // Setup and start distribution
            setup_distribution(vm, &binary_path).await?;

            // Generate test users
            let timestamp = std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_secs();
            let user_a = format!("usera-{}", timestamp);
            let user_b = format!("userb-{}", timestamp);
            let pass_a = "passwordA";
            let pass_b = "passwordB";

            // Register users
            tracing::info!("Registering test users...");
            register_user(vm, &user_a, pass_a).await?;
            tracing::info!("Registered user: {}", user_a);
            register_user(vm, &user_b, pass_b).await?;
            tracing::info!("Registered user: {}", user_b);

            // Run test cases
            test_anonymous_user(vm).await?;
            test_cross_namespace_push(vm, &user_a, pass_a, &user_b, pass_b).await?;
            test_visibility_permissions(vm, &user_a, pass_a, &user_b, pass_b).await?;

            tracing::info!("");
            tracing::info!("=================================================");
            tracing::info!("[SUCCESS] All integration tests passed!");
            tracing::info!("=================================================");

            Ok(())
        })
    })
    .await?;

    Ok(())
}
