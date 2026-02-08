use anyhow::anyhow;
use humantime::parse_duration;
use rand::rngs::OsRng;
use serde_json::json;
use ssh_key::{
    Fingerprint, HashAlg, PrivateKey, PublicKey,
    certificate::{Builder, CertType},
};
use std::time::{SystemTime, UNIX_EPOCH};

use crate::{
    errors::RvError,
    logical::{Backend, Field, FieldType, Operation, Path, Request, Response},
    modules::RequestExt,
    storage::StorageEntry,
    utils::seal::SealedSecret,
};

use super::{
    SSH_CA_PREFIX, SSH_CERTS_PREFIX, SSH_KEYS_PREFIX, SSH_REVOKED_PREFIX, SSH_ROLES_PREFIX,
    SSH_SERIAL_PATH, SshBackend, SshBackendInner,
    types::{SshCertSignRequest, SshRole},
};

impl SshBackend {
    pub fn cert_sign_path(&self) -> Path {
        let backend = self.inner.clone();
        Path::builder()
            .pattern("cert/sign")
            .operation(Operation::Write, {
                let handler = backend.clone();
                move |backend, req| {
                    let handler = handler.clone();
                    Box::pin(async move { handler.sign_cert(backend, req).await })
                }
            })
            .field(
                "ca_name",
                Field::builder()
                    .field_type(FieldType::Str)
                    .required(true)
                    .description("CA key name"),
            )
            .field(
                "key_name",
                Field::builder()
                    .field_type(FieldType::Str)
                    .required(true)
                    .description("User key name to sign"),
            )
            .field(
                "principals",
                Field::builder()
                    .field_type(FieldType::CommaStringSlice)
                    .required(false)
                    .description("Comma-separated list of principals"),
            )
            .field(
                "ttl",
                Field::builder()
                    .field_type(FieldType::Str)
                    .required(false)
                    .default_value("30d")
                    .description("Certificate validity duration, e.g., 30d"),
            )
            .field(
                "cert_type",
                Field::builder()
                    .field_type(FieldType::Str)
                    .required(false)
                    .default_value("user")
                    .description("Certificate type: user or host"),
            )
            .help("Sign an SSH public key with the CA and store the certificate")
            .build()
    }
}

impl SshBackendInner {
    pub async fn sign_cert(
        &self,
        _backend: &dyn Backend,
        req: &mut Request,
    ) -> Result<Option<Response>, RvError> {
        let payload: SshCertSignRequest = req.parse_json()?;

        // 1. Audit Logging: Start
        let user_display = req
            .auth
            .as_ref()
            .map(|a| a.display_name.as_str())
            .unwrap_or("unknown");
        log::info!(
            "SSH Cert Issue Request: user={}, key_name={}, ca_name={}",
            user_display,
            payload.key_name,
            payload.ca_name
        );

        // 2. Check Revocation (ID and Serial) - Pre-check
        let revoked_id = format!("id:{}", payload.key_name);
        if req
            .storage_get(&format!("{SSH_REVOKED_PREFIX}{}", revoked_id))
            .await?
            .is_some()
        {
            log::warn!(
                "SSH Cert Issue Blocked: Key ID {} is revoked",
                payload.key_name
            );
            return Err(RvError::ErrRequestInvalid);
        }

        // 3. Retrieve and Parse CA Key (No unwrap)
        let ca_entry = req
            .storage_get(format!("{SSH_CA_PREFIX}{}", payload.ca_name).as_str())
            .await?
            .ok_or(RvError::ErrPkiCertNotFound)?;

        let ca_key_str = if let Ok(secret) = serde_json::from_slice::<SealedSecret>(&ca_entry.value)
        {
            let key_bytes = secret
                .unwrap_key()
                .map_err(|e| RvError::ErrOther(anyhow!("Failed to unwrap CA key: {}", e)))?;
            let mut sb = secret.box_data;
            sb.unseal_with_key(&key_bytes)
                .map_err(|e| RvError::ErrOther(anyhow!("Failed to unseal CA key: {}", e)))?;
            String::from_utf8(
                sb.get()
                    .map_err(|e| RvError::ErrOther(anyhow!("Failed to get sealbox data: {}", e)))?
                    .clone(),
            )?
        } else {
            String::from_utf8(ca_entry.value)?
        };

        let ca_key = PrivateKey::from_openssh(&ca_key_str)?;

        // 4. Retrieve and Parse User Public Key (No unwrap)
        let public = if !payload.public_key.is_empty() {
            PublicKey::from_openssh(&payload.public_key)?
        } else {
            let user_entry = req
                .storage_get(format!("{SSH_KEYS_PREFIX}{}", payload.key_name).as_str())
                .await?
                .ok_or(RvError::ErrPkiCertNotFound)?;

            let user_key_str =
                if let Ok(secret) = serde_json::from_slice::<SealedSecret>(&user_entry.value) {
                    let key_bytes = secret
                        .unwrap_key()
                        .map_err(|e| RvError::ErrOther(anyhow!("Failed to unwrap key: {}", e)))?;
                    let mut sb = secret.box_data;
                    sb.unseal_with_key(&key_bytes)
                        .map_err(|e| RvError::ErrOther(anyhow!("Failed to unseal key: {}", e)))?;
                    String::from_utf8(
                        sb.get()
                            .map_err(|e| {
                                RvError::ErrOther(anyhow!("Failed to get sealbox data: {}", e))
                            })?
                            .clone(),
                    )?
                } else {
                    String::from_utf8(user_entry.value)?
                };

            let user_key = if let Some(pass) = payload.passphrase.as_ref() {
                PrivateKey::from_openssh(&user_key_str)?
                    .decrypt(pass)
                    .map_err(|e| RvError::ErrOther(anyhow!("Failed to decrypt key: {}", e)))?
            } else {
                PrivateKey::from_openssh(&user_key_str)?
            };
            user_key.public_key().clone()
        };

        // 5. Serial Management
        // Note: Without explicit locking supported by the storage trait (missing Send bound on lock object),
        // we accept a small race condition window here. In a real distributed setup, backend should handle CAS or atomic increments.
        // let _lock = req.storage_lock(SSH_SERIAL_PATH).await?;

        let serial_entry = req.storage_get(SSH_SERIAL_PATH).await?;
        let current_serial: u64 = if let Some(entry) = serial_entry {
            String::from_utf8(entry.value)?.parse().unwrap_or(0)
        } else {
            0
        };
        let serial = current_serial + 1;

        // Check Revocation (Serial)
        let revoked_serial = format!("serial:{}", serial);
        if req
            .storage_get(&format!("{SSH_REVOKED_PREFIX}{}", revoked_serial))
            .await?
            .is_some()
        {
            return Err(RvError::ErrRequestInvalid);
        }

        let entry = StorageEntry {
            key: SSH_SERIAL_PATH.to_string(),
            value: serial.to_string().into_bytes(),
        };
        req.storage_put(&entry).await?;
        // drop(_lock);

        // 6. Role and Parameter Validation
        let cert_type = match payload.cert_type.as_str() {
            "host" => CertType::Host,
            _ => CertType::User,
        };

        let principals: Vec<String> = payload
            .valid_principals
            .split(',')
            .map(|s| s.trim().to_string())
            .filter(|s| !s.is_empty())
            .collect();

        // Validate principals are not empty if provided (unless the role allows empty? usually SSH certs need principals)
        if principals.is_empty() && !payload.valid_principals.is_empty() {
            // If input was provided but resulted in empty list (e.g. ",,"), that's invalid
            return Err(RvError::ErrRequestInvalid);
        }

        let mut ttl = 3600 * 24 * 30; // default 30 days

        if let Some(role_name) = payload.role {
            let role_entry = req
                .storage_get(&format!("{}{}", SSH_ROLES_PREFIX, role_name))
                .await?;
            if let Some(entry) = role_entry {
                let role: SshRole = serde_json::from_slice(&entry.value)?;

                if cert_type == CertType::User && !role.allow_user_certificates {
                    return Err(RvError::ErrRequestInvalid);
                }
                if cert_type == CertType::Host && !role.allow_host_certificates {
                    return Err(RvError::ErrRequestInvalid);
                }

                if role.allowed_users != "*" {
                    let allowed: Vec<&str> =
                        role.allowed_users.split(',').map(|s| s.trim()).collect();
                    for p in &principals {
                        if !allowed.contains(&p.as_str()) {
                            return Err(RvError::ErrRequestInvalid);
                        }
                    }
                }

                if let Ok(dur) = parse_duration(&role.ttl) {
                    ttl = dur.as_secs();
                }
                if let Ok(max_dur) = parse_duration(&role.max_ttl) {
                    ttl = ttl.min(max_dur.as_secs());
                }
            } else {
                return Err(RvError::ErrRequestInvalid);
            }
        }

        // 7. Time Calculation (Overflow Protection)
        let now = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap_or_default()
            .as_secs();

        let valid_after = if !payload.valid_after.is_empty() {
            parse_duration(&payload.valid_after)
                .map(|d| now.saturating_add(d.as_secs()))
                .unwrap_or(now)
        } else {
            now
        };

        let valid_before = if !payload.valid_before.is_empty() {
            parse_duration(&payload.valid_before)
                .map(|d| now.saturating_add(d.as_secs()))
                .unwrap_or_else(|_| now.saturating_add(ttl))
        } else {
            now.saturating_add(ttl)
        };

        if valid_before < valid_after {
            return Err(RvError::ErrRequestInvalid);
        }

        // 8. Build and Sign
        let mut builder =
            Builder::new_with_random_nonce(&mut OsRng, public.clone(), valid_after, valid_before)?;
        builder
            .cert_type(cert_type)?
            .key_id(format!("{}:{}", payload.ca_name, payload.key_name))?
            .serial(serial)?
            .comment(payload.key_name.clone())?;

        for p in principals {
            builder.valid_principal(p)?;
        }

        let cert = builder.sign(&ca_key)?;
        let cert_str = cert.to_openssh()?;

        // Store the certificate
        let entry = StorageEntry {
            key: format!("{SSH_CERTS_PREFIX}{}", payload.key_name),
            value: cert_str.as_bytes().to_vec(),
        };
        req.storage_put(&entry).await?;

        // 9. Response and Audit
        let fp = Fingerprint::new(HashAlg::Sha256, public.key_data());
        let pubkey = public.to_openssh()?;

        log::info!(
            "SSH Cert Issued: serial={}, key={}, fp={}",
            serial,
            payload.key_name,
            fp
        );

        let resp = json!({
            "name": payload.key_name,
            "certificate": cert_str,
            "public_key": pubkey,
            "fingerprint": fp.to_string(),
            "expiration": valid_before,
            "serial": serial,
        })
        .as_object()
        .ok_or(RvError::ErrPkiInternal)?
        .clone();

        Ok(Some(Response::data_response(Some(resp))))
    }
}
