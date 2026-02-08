use libvault::core::{Core as VaultCore, SealConfig};
use libvault::logical::{Connection, Request};
use libvault::modules::auth::AuthModule;
use libvault::modules::credential::cert::CertModule;
use libvault::modules::kv::KvModule;
use libvault::modules::openpgp::OpenPgpModule;
use libvault::modules::pki::PkiModule;
#[cfg(feature = "ssh-key")]
use libvault::modules::ssh::SshModule;
use libvault::storage::physical::file::FileBackend;
use openssl::x509::X509;
use serde_json::json;
use std::sync::Arc;
use tempfile::TempDir;

async fn setup_core() -> (Arc<VaultCore>, TempDir, String) {
    let dir = TempDir::new().unwrap();
    let backend = FileBackend::with_folder(dir.path()).unwrap();
    let core = VaultCore::new(Arc::new(backend));
    let core = core.wrap();

    // Register modules
    core.module_manager
        .set_default_modules(core.clone())
        .unwrap();

    let kv = KvModule::new(core.clone());
    core.module_manager.add_module(Arc::new(kv)).unwrap();

    let auth = AuthModule::new(core.clone()).unwrap();
    core.module_manager.add_module(Arc::new(auth)).unwrap();

    let pki = PkiModule::new(core.clone());
    core.module_manager.add_module(Arc::new(pki)).unwrap();

    #[cfg(feature = "ssh-key")]
    {
        let ssh = SshModule::new(core.clone());
        core.module_manager.add_module(Arc::new(ssh)).unwrap();
    }

    let pgp = OpenPgpModule::new(core.clone());
    core.module_manager.add_module(Arc::new(pgp)).unwrap();

    let cert = CertModule::new(core.clone());
    core.module_manager.add_module(Arc::new(cert)).unwrap();

    let seal_config = SealConfig {
        secret_shares: 1,
        secret_threshold: 1,
    };
    let init_result = core.init(&seal_config).await.expect("failed to init core");

    // Unseal
    let unseal_key = &init_result.secret_shares[0];
    core.unseal(unseal_key).await.expect("failed to unseal");

    (core, dir, init_result.root_token.clone())
}

#[tokio::test]
async fn test_comprehensive_integration() {
    let (core, _dir, root_token): (_, _, _) = setup_core().await;
    let token = root_token.as_str();

    // --- 1. Mount Backends ---
    mount_backend(&core, token, "pki", "pki").await;
    #[cfg(feature = "ssh-key")]
    mount_backend(&core, token, "ssh", "ssh").await;
    mount_backend(&core, token, "openpgp", "openpgp").await;
    enable_auth(&core, token, "cert", "cert").await;

    // --- 2. Test SSH Module ---
    #[cfg(feature = "ssh-key")]
    test_ssh_module(&core, token).await;

    // --- 3. Test OpenPGP Module ---
    test_openpgp_module(&core, token).await;

    // --- 4. Test Cert Auth (TLS) ---
    test_cert_auth_with_pki(&core, token).await;
}

async fn mount_backend(core: &Arc<VaultCore>, token: &str, path: &str, _type: &str) {
    let mut req = Request::new_write_request(
        format!("sys/mounts/{}", path),
        Some(json!({ "type": _type }).as_object().unwrap().clone()),
    );
    req.client_token = token.to_string();
    core.handle_request(&mut req).await.expect("mount failed");
}

async fn enable_auth(core: &Arc<VaultCore>, token: &str, path: &str, _type: &str) {
    let mut req = Request::new_write_request(
        format!("sys/auth/{}", path),
        Some(json!({ "type": _type }).as_object().unwrap().clone()),
    );
    req.client_token = token.to_string();
    core.handle_request(&mut req)
        .await
        .expect("enable auth failed");
}

#[cfg(feature = "ssh-key")]
async fn test_ssh_module(core: &Arc<VaultCore>, token: &str) {
    println!("Testing SSH Module...");

    // Configure CA
    let mut req = Request::new_write_request(
        "ssh/config/ca",
        Some(
            json!({
                "generate": true
            })
            .as_object()
            .unwrap()
            .clone(),
        ),
    );
    req.client_token = token.to_string();
    core.handle_request(&mut req)
        .await
        .expect("ssh ca config failed");

    // Create Role
    let mut req = Request::new_write_request(
        "ssh/roles/test-role",
        Some(
            json!({
                "key_type": "ca",
                "allowed_users": "ubuntu",
                "ttl": "1h"
            })
            .as_object()
            .unwrap()
            .clone(),
        ),
    );
    req.client_token = token.to_string();
    core.handle_request(&mut req)
        .await
        .expect("ssh role create failed");

    // Issue Cert (Generate key internally)
    // 1. Generate Key
    let mut req = Request::new_write_request(
        "ssh/keys/generate",
        Some(
            json!({
                "key_name": "test-key"
            })
            .as_object()
            .unwrap()
            .clone(),
        ),
    );
    req.client_token = token.to_string();
    core.handle_request(&mut req)
        .await
        .expect("ssh key gen failed");

    // 2. Sign Key
    let mut req = Request::new_write_request(
        "ssh/cert/sign",
        Some(
            json!({
                "ca_name": "default",
                "key_name": "test-key",
                "role": "test-role",
                "valid_principals": "ubuntu",
                "public_key": "",
                "valid_after": "",
                "valid_before": "",
                "cert_type": "user"
            })
            .as_object()
            .unwrap()
            .clone(),
        ),
    );
    req.client_token = token.to_string();

    let resp = core
        .handle_request(&mut req)
        .await
        .expect("ssh issue failed")
        .unwrap();
    let data = resp.data.expect("no data");

    assert!(data.contains_key("certificate"));
    println!("SSH Cert Issued: {}", data.get("serial").unwrap());

    // 3. Fetch Cert (Test new logic)
    let mut req = Request::new_read_request("ssh/cert/fetch/test-key");
    req.client_token = token.to_string();
    let resp = core
        .handle_request(&mut req)
        .await
        .expect("ssh fetch failed")
        .unwrap();
    let data = resp.data.expect("no data");
    assert!(data.contains_key("certificate"));
    println!("SSH Cert Fetched Successfully");

    // 4. Revoke Cert (Test new logic)
    // A. Revoke by ID
    let mut req = Request::new_write_request(
        "ssh/revoke",
        Some(
            json!({
                "key_id": "test-key"
            })
            .as_object()
            .unwrap()
            .clone(),
        ),
    );
    req.client_token = token.to_string();
    let resp = core
        .handle_request(&mut req)
        .await
        .expect("ssh revoke failed")
        .unwrap();
    assert_eq!(
        resp.data.unwrap().get("revoked").unwrap().as_bool(),
        Some(true)
    );
    println!("SSH Cert Revoked by ID");

    // B. Idempotency Check (Revoke again)
    let mut req = Request::new_write_request(
        "ssh/revoke",
        Some(
            json!({
                "key_id": "test-key"
            })
            .as_object()
            .unwrap()
            .clone(),
        ),
    );
    req.client_token = token.to_string();
    let resp = core
        .handle_request(&mut req)
        .await
        .expect("ssh revoke retry failed")
        .unwrap();
    assert_eq!(
        resp.data.unwrap().get("revoked").unwrap().as_bool(),
        Some(true)
    );

    // C. Negative Serial (Should fail)
    let mut req = Request::new_write_request(
        "ssh/revoke",
        Some(
            json!({
                "serial": -1
            })
            .as_object()
            .unwrap()
            .clone(),
        ),
    );
    req.client_token = token.to_string();
    let result = core.handle_request(&mut req).await;
    assert!(result.is_err(), "Negative serial revocation should fail");
    println!("Negative serial revocation rejected as expected");
}

async fn test_openpgp_module(core: &Arc<VaultCore>, token: &str) {
    println!("Testing OpenPGP Module...");

    // Generate Key
    let mut req = Request::new_write_request(
        "openpgp/keys/test-key",
        Some(
            json!({
                "real_name": "Test User",
                "email": "test@example.com"
            })
            .as_object()
            .unwrap()
            .clone(),
        ),
    );
    req.client_token = token.to_string();
    core.handle_request(&mut req)
        .await
        .expect("pgp key gen failed");

    // Sign Data
    let input_data = "SGVsbG8gV29ybGQ="; // "Hello World" in base64
    let mut req = Request::new_write_request(
        "openpgp/keys/test-key/sign",
        Some(
            json!({
                "input": input_data
            })
            .as_object()
            .unwrap()
            .clone(),
        ),
    );
    req.client_token = token.to_string();

    let resp = core
        .handle_request(&mut req)
        .await
        .expect("pgp sign failed")
        .unwrap();
    let data = resp.data.unwrap();
    let signed_msg = data.get("signed_data").unwrap().as_str().unwrap();

    // Verify Data
    let mut req = Request::new_write_request(
        "openpgp/keys/test-key/verify",
        Some(
            json!({
                "input": signed_msg
            })
            .as_object()
            .unwrap()
            .clone(),
        ),
    );
    req.client_token = token.to_string();

    let resp = core
        .handle_request(&mut req)
        .await
        .expect("pgp verify failed")
        .unwrap();
    let data = resp.data.unwrap();
    assert_eq!(data.get("valid").unwrap().as_bool(), Some(true));
    println!("OpenPGP Signature Verified!");

    let mut req = Request::new_write_request(
        "openpgp/keys/test-key/revoke",
        Some(
            json!({
                "reason": "Compromised"
            })
            .as_object()
            .unwrap()
            .clone(),
        ),
    );
    req.client_token = token.to_string();

    let resp = core
        .handle_request(&mut req)
        .await
        .expect("pgp revoke failed")
        .unwrap();
    let data = resp.data.unwrap();
    assert_eq!(data.get("revoked").unwrap().as_bool(), Some(true));
    println!("OpenPGP Key Revoked!");

    // Verify Sign Fails after Revocation
    let mut req = Request::new_write_request(
        "openpgp/keys/test-key/sign",
        Some(
            json!({
                "input": "Should fail"
            })
            .as_object()
            .unwrap()
            .clone(),
        ),
    );
    req.client_token = token.to_string();

    let result = core.handle_request(&mut req).await;
    assert!(result.is_err(), "Signing should fail for revoked key");
    println!("OpenPGP Signing Blocked for Revoked Key!");
}

async fn test_cert_auth_with_pki(core: &Arc<VaultCore>, token: &str) {
    println!("Testing Cert Auth (using PKI)...");

    // 1. Setup PKI to be our CA
    let mut req = Request::new_write_request(
        "pki/root/generate/internal",
        Some(
            json!({
                "common_name": "Vault Test CA"
            })
            .as_object()
            .unwrap()
            .clone(),
        ),
    );
    req.client_token = token.to_string();
    let resp = core
        .handle_request(&mut req)
        .await
        .expect("pki root gen failed")
        .unwrap();
    let ca_cert_pem = resp
        .data
        .as_ref()
        .unwrap()
        .get("certificate")
        .unwrap()
        .as_str()
        .unwrap();

    // 2. Create PKI Role for Client Certs
    let mut req = Request::new_write_request(
        "pki/roles/client",
        Some(
            json!({
                "allowed_domains": "client.example.com",
                "allow_subdomains": true,
                "client_flag": true
            })
            .as_object()
            .unwrap()
            .clone(),
        ),
    );
    req.client_token = token.to_string();
    core.handle_request(&mut req)
        .await
        .expect("pki role failed");

    // 3. Issue Client Cert
    let mut req = Request::new_write_request(
        "pki/issue/client",
        Some(
            json!({
                "common_name": "client.example.com"
            })
            .as_object()
            .unwrap()
            .clone(),
        ),
    );
    req.client_token = token.to_string();
    let resp = core
        .handle_request(&mut req)
        .await
        .expect("pki issue failed")
        .unwrap();
    let data = resp.data.unwrap();
    let client_cert_pem = data.get("certificate").unwrap().as_str().unwrap();
    let serial_number = data.get("serial_number").unwrap().as_str().unwrap();

    // Verify Certificate Storage (Fetch by Serial)
    let mut req = Request::new_read_request(format!("pki/cert/{}", serial_number));
    req.client_token = token.to_string();
    let resp = core
        .handle_request(&mut req)
        .await
        .expect("fetch cert failed")
        .unwrap();
    let data = resp.data.expect("fetch data empty");
    let fetched_cert = data.get("certificate").unwrap().as_str().unwrap();

    assert_eq!(
        client_cert_pem.trim(),
        fetched_cert.trim(),
        "Fetched certificate does not match issued certificate"
    );

    // 4. Configure Cert Auth Backend
    // Trust the CA we just generated
    let mut req = Request::new_write_request(
        "auth/cert/certs/test-role",
        Some(
            json!({
                "certificate": ca_cert_pem,
                "allowed_common_names": "client.example.com",
                "token_ttl": "1h"
            })
            .as_object()
            .unwrap()
            .clone(),
        ),
    );
    req.client_token = token.to_string();
    core.handle_request(&mut req)
        .await
        .expect("cert auth config failed");

    // 5. Simulate Login
    // Parse PEM to X509 for Connection mock
    let client_cert_x509 =
        X509::from_pem(client_cert_pem.as_bytes()).expect("failed to parse client cert");

    let mut req = Request::new_write_request(
        "auth/cert/login",
        Some(
            json!({
                "name": "test-role"
            })
            .as_object()
            .unwrap()
            .clone(),
        ),
    );

    // Mock TLS Connection
    req.connection = Some(Connection {
        peer_addr: "127.0.0.1:1234".to_string(),
        peer_tls_cert: Some(vec![client_cert_x509]),
    });

    let resp = core
        .handle_request(&mut req)
        .await
        .expect("login request failed")
        .unwrap();

    assert!(resp.auth.is_some());
    println!(
        "Cert Auth Login Successful! Token: {}",
        resp.auth.unwrap().client_token
    );
}
