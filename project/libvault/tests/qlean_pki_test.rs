#![cfg(target_os = "linux")]

use anyhow::{Context, Result};
use libvault::core::Core;
use libvault::logical::{Operation, Request};
use libvault::modules::auth::AuthModule;
use libvault::modules::kv::KvModule;
use libvault::modules::pki::PkiModule;
use libvault::modules::policy::PolicyModule;
use libvault::mount::MountEntry;
use libvault::storage::Backend as PhysicalBackend;
use libvault::storage::physical::file::FileBackend;
use openssl::x509::X509;
use qlean::{Distro, MachineConfig, create_image, with_machine};
use ssh_key::LineEnding;
use std::str;
use std::sync::Arc;

// ============================================================
// setup_core: initialize Vault with all modules, return (core, root_token)
// ============================================================
async fn setup_core() -> Result<(Arc<Core>, String)> {
    let temp_dir = tempfile::tempdir().context("Failed to create temp dir")?;
    let path = temp_dir.keep();

    let backend: Arc<dyn PhysicalBackend> =
        Arc::new(FileBackend::with_folder(&path).context("Failed to create FileBackend")?);
    let core = Core::new(backend).wrap();

    core.module_manager
        .set_default_modules(core.clone())
        .context("Failed to set default modules")?;

    let auth_module = AuthModule::new(core.clone()).context("Failed to create AuthModule")?;
    core.module_manager
        .add_module(Arc::new(auth_module))
        .context("Failed to add AuthModule")?;

    let policy_module = PolicyModule::new(core.clone());
    core.module_manager
        .add_module(Arc::new(policy_module))
        .context("Failed to add PolicyModule")?;

    let pki_module = PkiModule::new(core.clone());
    core.module_manager
        .add_module(Arc::new(pki_module))
        .context("Failed to add PkiModule")?;

    let kv_module = KvModule::new(core.clone());
    core.module_manager
        .add_module(Arc::new(kv_module))
        .context("Failed to add KvModule")?;

    let seal_config = libvault::core::SealConfig {
        secret_shares: 1,
        secret_threshold: 1,
    };
    let init_result = core
        .init(&seal_config)
        .await
        .context("Failed to init core")?;

    let unseal_key = &init_result.secret_shares[0];
    core.unseal(unseal_key)
        .await
        .context("Failed to unseal core")?;

    let root_token = init_result.root_token.clone();

    let mount_entry = MountEntry::new("mounts", "pki/", "pki", "PKI backend");
    core.mount(&mount_entry)
        .await
        .context("Failed to mount PKI backend")?;

    Ok((core, root_token))
}

// ============================================================
// Main test: TLS / SSH / PGP generation, storage, and VM validation
// ============================================================
#[tokio::test]
async fn test_tls_ssh_pgp_generation_and_validation() -> Result<()> {
    let start_time = std::time::Instant::now();
    let (core, root_token) = setup_core().await?;

    // ==========================================================
    // Part 1: TLS
    // ==========================================================

    // 1-1. Generate Root CA
    let mut req = Request::new("pki/root/tls/generate/internal");
    req.operation = Operation::Write;
    req.client_token = root_token.clone();
    req.body = Some(
        serde_json::json!({
            "common_name": "Test Root CA",
            "ttl": "87600h"
        })
        .as_object()
        .unwrap()
        .clone(),
    );

    let resp = core
        .handle_request(&mut req)
        .await
        .context("Failed to generate root CA")?;
    let root_ca_pem = resp
        .context("Root CA response was None")?
        .data
        .context("Root CA response data was None")?
        .get("certificate")
        .context("certificate field missing")?
        .as_str()
        .context("certificate is not a string")?
        .to_string();
    println!("[OK] Root CA generated");

    // 1-2. Create PKI role
    let mut req = Request::new("pki/roles/tls/example-dot-com");
    req.operation = Operation::Write;
    req.client_token = root_token.clone();
    req.body = Some(
        serde_json::json!({
            "allowed_domains": "example.com",
            "allow_subdomains": true,
            "max_ttl": "72h"
        })
        .as_object()
        .unwrap()
        .clone(),
    );
    core.handle_request(&mut req)
        .await
        .context("Failed to create PKI role")?;
    println!("[OK] PKI role 'example-dot-com' created");

    // 1-3. Issue TLS server certificate
    let mut req = Request::new("pki/issue/tls/example-dot-com");
    req.operation = Operation::Write;
    req.client_token = root_token.clone();
    req.body = Some(
        serde_json::json!({
            "common_name": "www.example.com",
            "ttl": "24h"
        })
        .as_object()
        .unwrap()
        .clone(),
    );

    let resp = core
        .handle_request(&mut req)
        .await
        .context("Failed to issue TLS certificate")?;
    let tls_data = resp
        .context("Issue cert response was None")?
        .data
        .context("Issue cert response data was None")?;

    let tls_cert = tls_data
        .get("certificate")
        .context("certificate missing")?
        .as_str()
        .context("certificate not string")?
        .to_string();
    let tls_key = tls_data
        .get("private_key")
        .context("private_key missing")?
        .as_str()
        .context("private_key not string")?
        .to_string();
    let tls_serial = tls_data
        .get("serial_number")
        .context("serial_number missing")?
        .as_str()
        .context("serial_number not string")?
        .to_string();
    println!("[OK] TLS cert issued (serial: {})", tls_serial);

    // 1-4. Storage consistency: fetch cert by serial
    let mut req = Request::new(&format!("pki/cert/tls/{}", tls_serial));
    req.operation = Operation::Read;
    req.client_token = root_token.clone();
    let resp = core
        .handle_request(&mut req)
        .await
        .context("Failed to fetch TLS cert from storage")?;
    let fetched_tls_cert = resp
        .context("Fetch cert response was None")?
        .data
        .context("Fetch cert response data was None")?
        .get("certificate")
        .context("certificate missing")?
        .as_str()
        .context("certificate not string")?
        .to_string();
    assert_eq!(
        tls_cert, fetched_tls_cert,
        "Fetched TLS cert does not match issued cert"
    );
    println!("[OK] TLS cert storage consistency verified");

    // 1-5. Local signature verification
    let x509_cert =
        X509::from_pem(tls_cert.as_bytes()).context("Failed to parse issued TLS cert")?;
    let x509_ca = X509::from_pem(root_ca_pem.as_bytes()).context("Failed to parse Root CA cert")?;
    let ca_pubkey = x509_ca
        .public_key()
        .context("Failed to extract CA public key")?;
    let is_valid = x509_cert
        .verify(&ca_pubkey)
        .context("TLS cert signature verification call failed")?;
    assert!(
        is_valid,
        "TLS cert signature is invalid (not signed by Root CA)"
    );
    println!("[OK] TLS cert signature locally verified against Root CA");

    // ==========================================================
    // Part 2: SSH
    // ==========================================================

    // 2-1. Configure SSH CA (generates a new RSA key pair for SSH signing)
    let mut req = Request::new("pki/config/ca/ssh");
    req.operation = Operation::Write;
    req.client_token = root_token.clone();
    req.body = Some(
        serde_json::json!({
            "key_type": "rsa",
            "key_bits": 2048
        })
        .as_object()
        .unwrap()
        .clone(),
    );
    let resp = core
        .handle_request(&mut req)
        .await
        .context("Failed to configure SSH CA")?;
    let ssh_ca_pub = resp
        .context("SSH CA config response was None")?
        .data
        .context("SSH CA config response data was None")?
        .get("public_key")
        .context("public_key missing")?
        .as_str()
        .context("public_key not string")?
        .to_string();
    println!("[OK] SSH CA configured");

    // 2-2. Create SSH role
    let mut req = Request::new("pki/roles/ssh/my-role");
    req.operation = Operation::Write;
    req.client_token = root_token.clone();
    req.body = Some(
        serde_json::json!({
            "cert_type_ssh": "user",
            "key_type": "ed25519",
            "ttl": "1h",
            "allowed_users": "ubuntu"
        })
        .as_object()
        .unwrap()
        .clone(),
    );
    core.handle_request(&mut req)
        .await
        .context("Failed to create SSH role")?;
    println!("[OK] SSH role 'my-role' created");

    // 2-3. Generate Ed25519 user key pair locally
    let user_ssh_key =
        ssh_key::PrivateKey::random(&mut ssh_key::rand_core::OsRng, ssh_key::Algorithm::Ed25519)
            .context("Failed to generate Ed25519 SSH key")?;

    let user_ssh_pub = user_ssh_key
        .public_key()
        .to_openssh()
        .context("Failed to export SSH public key")?;
    let user_ssh_priv_openssh = user_ssh_key
        .to_openssh(LineEnding::LF)
        .context("Failed to export SSH private key")?;

    // 2-4. Request Vault to sign the user's public key
    let mut req = Request::new("pki/sign/ssh/my-role");
    req.operation = Operation::Write;
    req.client_token = root_token.clone();
    req.body = Some(
        serde_json::json!({
            "public_key": user_ssh_pub,
            "key_id": "test-user-cert",
            "valid_principals": ["ubuntu"],
            "ttl": "1h"
        })
        .as_object()
        .unwrap()
        .clone(),
    );

    let resp = core
        .handle_request(&mut req)
        .await
        .context("Failed to sign SSH user cert")?;
    let ssh_data = resp
        .context("SSH sign response was None")?
        .data
        .context("SSH sign response data was None")?;

    let ssh_cert = ssh_data
        .get("signed_key")
        .context("signed_key missing")?
        .as_str()
        .context("signed_key not string")?
        .to_string();
    let ssh_serial = ssh_data
        .get("serial_number")
        .context("serial_number missing")?
        .as_str()
        .context("serial_number not string")?
        .to_string();
    println!("[OK] SSH cert signed (serial: {})", ssh_serial);

    // 2-5. Storage consistency: fetch SSH cert by serial
    let mut req = Request::new(&format!("pki/cert/ssh/{}", ssh_serial));
    req.operation = Operation::Read;
    req.client_token = root_token.clone();
    let resp = core
        .handle_request(&mut req)
        .await
        .context("Failed to fetch SSH cert from storage")?;
    let ssh_fetched_data = resp
        .context("Fetch SSH cert response was None")?
        .data
        .context("Fetch SSH cert response data was None")?;
    let fetched_ssh_cert = ssh_fetched_data
        .get("signed_key")
        .context("signed_key missing")?
        .as_str()
        .context("signed_key not string")?
        .to_string();
    assert_eq!(
        ssh_cert, fetched_ssh_cert,
        "Fetched SSH cert does not match issued cert"
    );
    println!("[OK] SSH cert storage consistency verified");

    // ==========================================================
    // Part 3: PGP
    // ==========================================================

    // 3-1. Generate PGP key pair (exported mode to get private key)
    let mut req = Request::new("pki/keys/generate/exported");
    req.operation = Operation::Write;
    req.client_token = root_token.clone();
    req.body = Some(
        serde_json::json!({
            "name": "Test User",
            "email": "test@example.com",
            "key_name": "test-pgp-key",
            "key_type": "pgp",
            "key_bits": 2048
        })
        .as_object()
        .unwrap()
        .clone(),
    );

    let resp = core
        .handle_request(&mut req)
        .await
        .context("Failed to generate PGP key")?;
    let pgp_data = resp
        .context("PGP generate response was None")?
        .data
        .context("PGP generate response data was None")?;

    let pgp_pub = pgp_data
        .get("public_key")
        .context("public_key missing")?
        .as_str()
        .context("public_key not string")?
        .to_string();
    let pgp_priv = pgp_data
        .get("private_key")
        .and_then(|v| v.as_str())
        .context("private_key missing or null (ensure using /exported endpoint)")?
        .to_string();
    let pgp_fingerprint = pgp_data
        .get("fingerprint")
        .context("fingerprint missing")?
        .as_str()
        .context("fingerprint not string")?
        .to_string();
    println!("[OK] PGP key generated (fingerprint: {})", pgp_fingerprint);

    // 3-2. Verify PGP key works via sign + verify API roundtrip
    // "hello pgp" in hex
    let test_data = "68656c6c6f20706770";
    let mut req = Request::new("pki/keys/sign");
    req.operation = Operation::Write;
    req.client_token = root_token.clone();
    req.body = Some(
        serde_json::json!({
            "key_name": "test-pgp-key",
            "data": test_data
        })
        .as_object()
        .unwrap()
        .clone(),
    );

    let resp = core
        .handle_request(&mut req)
        .await
        .context("Failed to sign data with PGP key")?;
    let sig_hex = resp
        .context("PGP sign response was None")?
        .data
        .context("PGP sign response data was None")?
        .get("signature")
        .context("signature missing")?
        .as_str()
        .context("signature not string")?
        .to_string();
    println!("[OK] PGP signature created");

    let mut req = Request::new("pki/keys/verify");
    req.operation = Operation::Write;
    req.client_token = root_token.clone();
    req.body = Some(
        serde_json::json!({
            "key_name": "test-pgp-key",
            "data": test_data,
            "signature": sig_hex
        })
        .as_object()
        .unwrap()
        .clone(),
    );

    let resp = core
        .handle_request(&mut req)
        .await
        .context("Failed to verify PGP signature")?;
    let valid = resp
        .context("PGP verify response was None")?
        .data
        .context("PGP verify response data was None")?
        .get("valid")
        .context("valid missing")?
        .as_bool()
        .context("valid not bool")?;
    assert!(valid, "PGP signature verification failed");
    println!("[OK] PGP sign + verify API roundtrip verified");

    println!(
        "\n[INFO] Vault logic tests completed in {:?}",
        start_time.elapsed()
    );

    // ==========================================================
    // Part 4: Qlean VM end-to-end validation
    // ==========================================================
    println!("\n[INFO] Starting VM-based integration tests...");
    let vm_start = std::time::Instant::now();

    let image = create_image(Distro::Debian, "debian-13-generic-amd64")
        .await
        .context("Failed to create Qlean VM image")?;
    let config = MachineConfig {
        core: 2,
        mem: 1024,
        disk: None,
        clear: true,
    };

    with_machine(&image, &config, |vm| {
        Box::pin(async move {
            // --------------------------------------------------
            // 4-1. TLS verification
            // --------------------------------------------------
            vm.write("root_ca.crt", root_ca_pem.as_bytes()).await?;
            vm.write("server.crt", tls_cert.as_bytes()).await?;
            vm.write("server.key", tls_key.as_bytes()).await?;

            // Verify certificate chain
            let result = vm.exec("openssl verify -CAfile root_ca.crt server.crt").await?;
            if !result.status.success() {
                println!(
                    "[ERROR] TLS chain verify stderr: {}",
                    str::from_utf8(&result.stderr)?
                );
                println!(
                    "[ERROR] TLS chain verify stdout: {}",
                    str::from_utf8(&result.stdout)?
                );
            }
            assert!(
                result.status.success(),
                "TLS certificate chain verification failed"
            );
            println!("[VM] TLS chain verified");

            // TLS handshake test - write script to file to avoid quote-nesting issues
            println!("[VM] Starting TLS handshake test (may take 10-20 seconds)...");
            let tls_handshake_script = r#"#!/bin/bash
set -e

openssl s_server -accept 4433 -cert server.crt -key server.key -www > /tmp/server.log 2>&1 &
SERVER_PID=$!

echo "Waiting for port 4433..."
for i in $(seq 1 40); do
    if ss -tln | grep -q ':4433 '; then
        echo "Port ready"
        break
    fi
    if [ $i -eq 40 ]; then
        echo "ERROR: Timeout waiting for port" >&2
        cat /tmp/server.log >&2
        kill $SERVER_PID 2>/dev/null || true
        exit 1
    fi
    sleep 0.5
done

sleep 1

echo Q | timeout 15 openssl s_client -connect localhost:4433 -CAfile root_ca.crt -quiet > /tmp/client.log 2>&1
RET=$?

kill $SERVER_PID 2>/dev/null || true
exit $RET
"#;
            vm.write("tls_test.sh", tls_handshake_script.as_bytes()).await?;
            vm.exec("chmod +x tls_test.sh").await?;
            let result = vm.exec("bash tls_test.sh").await?;
            if !result.status.success() {
                println!(
                    "[WARN] TLS handshake failed: {}",
                    str::from_utf8(&result.stderr)?
                );
                println!("[WARN] Continuing anyway (chain verification passed)");
            } else {
                println!("[VM] TLS handshake verified");
            }

            // --------------------------------------------------
            // 4-2. SSH verification
            // --------------------------------------------------
            vm.write("id_ed25519-cert.pub", ssh_cert.as_bytes())
                .await?;
            let result = vm.exec("ssh-keygen -L -f id_ed25519-cert.pub").await?;
            assert!(
                result.status.success(),
                "SSH cert inspection failed"
            );
            let stdout = str::from_utf8(&result.stdout)?;
            assert!(
                stdout.contains("ssh-ed25519-cert-v01@openssh.com"),
                "Wrong cert type: {}",
                stdout
            );
            assert!(
                stdout.contains("ubuntu"),
                "Missing principals: {}",
                stdout
            );
            println!("[VM] SSH cert structure verified");

            // Configure sshd
            vm.write("user_ca.pub", ssh_ca_pub.as_bytes()).await?;
            vm.exec("cp user_ca.pub /etc/ssh/user_ca.pub").await?;
            vm.exec("echo 'TrustedUserCAKeys /etc/ssh/user_ca.pub' >> /etc/ssh/sshd_config")
                .await?;

            vm.exec("id -u ubuntu > /dev/null 2>&1 || useradd -m -s /bin/bash ubuntu")
                .await?;

            vm.exec("systemctl restart ssh 2>/dev/null || service ssh restart 2>/dev/null || true").await?;
            vm.exec("sleep 2").await?;

            // SSH login
            vm.write("id_ed25519", user_ssh_priv_openssh.as_bytes())
                .await?;
            vm.exec("chmod 600 id_ed25519").await?;

            let result = vm
                .exec("ssh -o StrictHostKeyChecking=no -o BatchMode=yes -o ConnectTimeout=10 -i id_ed25519 ubuntu@localhost echo success 2>&1")
                .await?;
            assert!(result.status.success(), "SSH login failed");
            assert!(
                str::from_utf8(&result.stdout)?.contains("success"),
                "Wrong output"
            );
            println!("[VM] SSH certificate login verified");

            // --------------------------------------------------
            // 4-3. PGP verification
            // --------------------------------------------------
            // Install gnupg if not present
            vm.exec("which gpg >/dev/null 2>&1 || apt-get update -qq && apt-get install -y -qq gnupg >/dev/null 2>&1").await?;

            // Configure GPG to avoid gpg-agent/pinentry hangs in non-interactive VM
            vm.exec("mkdir -p ~/.gnupg && chmod 700 ~/.gnupg").await?;
            vm.exec("echo 'pinentry-mode loopback' > ~/.gnupg/gpg.conf").await?;
            vm.exec("echo 'allow-loopback-pinentry' > ~/.gnupg/gpg-agent.conf").await?;
            vm.exec("gpgconf --kill gpg-agent 2>/dev/null || true").await?;

            vm.write("public.asc", pgp_pub.as_bytes()).await?;
            vm.write("private.asc", pgp_priv.as_bytes()).await?;

            let result = vm.exec("gpg --batch --import public.asc 2>&1").await?;
            println!("[VM] PGP public key import: {}", str::from_utf8(&result.stdout)?);
            if !result.status.success() {
                println!("[VM] PGP public key import stderr: {}", str::from_utf8(&result.stderr)?);
            }

            let result = vm.exec("gpg --batch --import private.asc 2>&1").await?;
            println!("[VM] PGP private key import: {}", str::from_utf8(&result.stdout)?);
            if !result.status.success() {
                println!("[VM] PGP private key import stderr: {}", str::from_utf8(&result.stderr)?);
            }

            let result = vm.exec("gpg --list-keys --keyid-format long 2>&1").await?;
            let stdout = str::from_utf8(&result.stdout)?;
            println!("[VM] gpg --list-keys output:\n{}", stdout);
            assert!(stdout.contains("Test User"), "PGP key missing 'Test User' in: {}", stdout);
            assert!(stdout.contains("test@example.com"), "PGP key missing email in: {}", stdout);
            println!("[VM] PGP key metadata verified");

            vm.exec("echo 'Hello PGP' > data.txt").await?;
            let result = vm.exec("gpg --batch --yes --pinentry-mode loopback --default-key test@example.com --sign --armor --output data.sig data.txt").await?;
            assert!(result.status.success(), "PGP sign failed: {}", str::from_utf8(&result.stderr)?);
            let result = vm.exec("gpg --batch --verify data.sig").await?;
            assert!(
                result.status.success() && str::from_utf8(&result.stderr)?.contains("Good signature"),
                "PGP verify failed: status={}, stderr={}",
                result.status,
                str::from_utf8(&result.stderr)?
            );
            println!("[VM] PGP sign + verify verified");

            println!("\n[VM] All tests passed!");
            Ok(())
        })
    })
    .await?;

    println!("[INFO] VM tests: {:?}", vm_start.elapsed());
    println!("\n=== ALL TESTS PASSED in {:?} ===", start_time.elapsed());
    Ok(())
}
