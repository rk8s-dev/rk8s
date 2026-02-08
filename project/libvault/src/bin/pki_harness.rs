use libvault::core::{Core as VaultCore, SealConfig};
use libvault::logical::Request;
use libvault::modules::kv::KvModule;
use libvault::modules::pki::PkiModule;
use libvault::storage::physical::file::FileBackend;
use serde_json::json;
use std::sync::Arc;
use tempfile::TempDir;

#[tokio::main]
async fn main() {
    println!("Starting PKI Harness...");
    let result = run_test().await;
    match result {
        Ok(_) => {
            println!("PKI Harness Test Passed");
            std::process::exit(0);
        }
        Err(e) => {
            eprintln!("PKI Harness Test Failed: {:?}", e);
            std::process::exit(1);
        }
    }
}

async fn run_test() -> Result<(), Box<dyn std::error::Error>> {
    let dir = TempDir::new()?;
    let backend = FileBackend::with_folder(dir.path())?;
    let core = VaultCore::new(Arc::new(backend));
    let core = core.wrap();

    // Register modules
    core.module_manager.set_default_modules(core.clone())?;

    let pki_module = PkiModule::new(core.clone());
    core.module_manager.add_module(Arc::new(pki_module))?;

    let kv_module = KvModule::new(core.clone());
    core.module_manager.add_module(Arc::new(kv_module))?;

    let seal_config = SealConfig {
        secret_shares: 1,
        secret_threshold: 1,
    };
    let init_result = core.init(&seal_config).await?;

    // Unseal
    let unseal_key = &init_result.secret_shares[0];
    core.unseal(unseal_key).await?;

    // 1. Mount PKI Backend
    let mut mount_req = Request::new_write_request(
        "sys/mounts/pki",
        Some(json!({ "type": "pki" }).as_object().unwrap().clone()),
    );
    mount_req.client_token = init_result.root_token.clone();

    core.handle_request(&mut mount_req)
        .await
        .map_err(|e| format!("Mount failed: {:?}", e))?;

    // 2. Generate Root CA
    let mut req = Request::new_write_request(
        "pki/root/generate/internal",
        Some(
            json!({
                "common_name": "Test Root CA",
                "ttl": "87600h"
            })
            .as_object()
            .unwrap()
            .clone(),
        ),
    );
    req.client_token = init_result.root_token.clone();

    let resp = core
        .handle_request(&mut req)
        .await
        .map_err(|e| format!("Generate root failed: {:?}", e))?;
    if resp.is_none() {
        return Err("Generate root response is empty".into());
    }

    // 3. Create a Role
    let mut req = Request::new_write_request(
        "pki/roles/web-server",
        Some(
            json!({
                "allowed_domains": "example.com",
                "allow_subdomains": true,
                "max_ttl": "72h"
            })
            .as_object()
            .unwrap()
            .clone(),
        ),
    );
    req.client_token = init_result.root_token.clone();
    core.handle_request(&mut req)
        .await
        .map_err(|e| format!("Create role failed: {:?}", e))?;

    // 4. Issue a Certificate
    let mut req = Request::new_write_request(
        "pki/issue/web-server",
        Some(
            json!({
                "common_name": "www.example.com",
                "ttl": "24h"
            })
            .as_object()
            .unwrap()
            .clone(),
        ),
    );
    req.client_token = init_result.root_token.clone();

    let resp = core
        .handle_request(&mut req)
        .await
        .map_err(|e| format!("Issue failed: {:?}", e))?
        .unwrap();

    // 5. Assertions
    let data = resp.data.ok_or("Response data empty")?;
    if !data.contains_key("certificate") {
        return Err("Missing certificate".into());
    }
    if !data.contains_key("private_key") {
        return Err("Missing private_key".into());
    }
    if !data.contains_key("serial_number") {
        return Err("Missing serial_number".into());
    }

    println!("Certificate Serial: {}", data.get("serial_number").unwrap());

    Ok(())
}
