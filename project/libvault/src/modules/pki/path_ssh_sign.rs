use std::time::{SystemTime, UNIX_EPOCH};

use base64::Engine;
use humantime::parse_duration;
use openssl::pkey::PKey;
use rand::Rng;
use tracing::info;

use super::{PkiBackend, PkiBackendInner, ssh_util, types};
use crate::{
    errors::RvError,
    logical::{Backend, Field, FieldType, Operation, Path, Request, Response},
    modules::{RequestExt, ResponseExt},
    storage::StorageEntry,
};

impl PkiBackend {
    pub fn ssh_sign_path(&self) -> Path {
        let backend = self.inner.clone();

        Path::builder()
            .pattern(r"ssh/sign/(?P<role>\w[\w-]+\w)")
            .field(
                "role",
                Field::builder()
                    .field_type(FieldType::Str)
                    .required(true)
                    .description("SSH role name"),
            )
            .field(
                "public_key",
                Field::builder()
                    .field_type(FieldType::Str)
                    .required(true)
                    .description("OpenSSH public key to sign"),
            )
            .field(
                "key_id",
                Field::builder()
                    .field_type(FieldType::Str)
                    .required(true)
                    .description("Key identifier"),
            )
            .field(
                "ttl",
                Field::builder()
                    .field_type(FieldType::Str)
                    .description("Requested TTL"),
            )
            .operation(Operation::Write, {
                let handler = backend.clone();
                move |backend, req| {
                    let handler = handler.clone();
                    Box::pin(async move { handler.ssh_sign_key(backend, req).await })
                }
            })
            .help("Sign an existing SSH public key with the CA.")
            .build()
    }
}

impl PkiBackendInner {
    pub async fn ssh_sign_key(
        &self,
        _backend: &dyn Backend,
        req: &mut Request,
    ) -> Result<Option<Response>, RvError> {
        let payload: types::SshSignKeyRequest = req.parse_json()?;

        let role_name = req
            .get_data("role")?
            .as_str()
            .ok_or(RvError::ErrRequestFieldInvalid)?
            .to_string();
        let role = self
            .get_ssh_role(req, &role_name)
            .await?
            .ok_or(RvError::ErrPkiSshRoleNotFound)?;

        let ca_bundle = self.fetch_ssh_ca_bundle(req).await?;
        let ca_key = PKey::private_key_from_pem(ca_bundle.private_key_pem.as_bytes())?;
        let ca_nid = if ca_key.id() == openssl::pkey::Id::EC {
            ca_key.ec_key()?.group().curve_name()
        } else {
            None
        };

        // Parse the user's OpenSSH public key
        let parts: Vec<&str> = payload.public_key.split_whitespace().collect();
        if parts.len() < 2 {
            return Err(RvError::ErrPkiSshPublicKeyInvalid);
        }
        let key_type_str = parts[0];
        let key_data = base64::engine::general_purpose::STANDARD
            .decode(parts[1])
            .map_err(|_| RvError::ErrPkiSshPublicKeyInvalid)?;

        // Validate key type early
        match key_type_str {
            "ssh-rsa" | "ecdsa-sha2-nistp256" | "ecdsa-sha2-nistp384"
            | "ecdsa-sha2-nistp521" | "ssh-ed25519" => {}
            _ => return Err(RvError::ErrPkiSshPublicKeyInvalid),
        }

        // Extract the public key data (skip the key type string prefix in wire format)
        // Wire format: [u32 len][key_type_bytes][...pubkey_data...]
        if key_data.len() < 4 {
            return Err(RvError::ErrPkiSshPublicKeyInvalid);
        }
        let type_len = u32::from_be_bytes(
            key_data[0..4]
                .try_into()
                .map_err(|_| RvError::ErrPkiSshPublicKeyInvalid)?,
        ) as usize;
        if 4 + type_len > key_data.len() {
            return Err(RvError::ErrPkiSshPublicKeyInvalid);
        }
        let pubkey_data = &key_data[4 + type_len..];

        let cert_type = match role.cert_type.as_str() {
            "user" => ssh_util::SSH_CERT_TYPE_USER,
            "host" => ssh_util::SSH_CERT_TYPE_HOST,
            _ => return Err(RvError::ErrPkiSshCertTypeInvalid),
        };

        // Validate principals against role's allowed_users
        if payload.valid_principals.is_empty() {
            return Err(RvError::ErrPkiSshPrincipalNotAllowed);
        }
        if !role.allowed_users.is_empty() {
            for principal in &payload.valid_principals {
                if !role.allowed_users.contains(principal) {
                    return Err(RvError::ErrPkiSshPrincipalNotAllowed);
                }
            }
        }

        let ttl = if let Some(ref ttl_str) = payload.ttl {
            parse_duration(ttl_str)?
        } else {
            role.ttl
        };

        let now = SystemTime::now().duration_since(UNIX_EPOCH)?.as_secs();
        let valid_after = now - 10;
        let valid_before = now + ttl.as_secs();

        let serial: u64 = rand::rng().random();

        let mut extensions = payload.extensions.unwrap_or_default();
        if extensions.is_empty() && cert_type == ssh_util::SSH_CERT_TYPE_USER {
            extensions.insert("permit-pty".to_string(), String::new());
            extensions.insert("permit-user-rc".to_string(), String::new());
        }

        // Derive cert type string from the user's key type
        let cert_type_str = match key_type_str {
            "ssh-rsa" => "ssh-rsa-cert-v01@openssh.com",
            "ecdsa-sha2-nistp256" => "ecdsa-sha2-nistp256-cert-v01@openssh.com",
            "ecdsa-sha2-nistp384" => "ecdsa-sha2-nistp384-cert-v01@openssh.com",
            "ecdsa-sha2-nistp521" => "ecdsa-sha2-nistp521-cert-v01@openssh.com",
            "ssh-ed25519" => "ssh-ed25519-cert-v01@openssh.com",
            _ => return Err(RvError::ErrPkiSshPublicKeyInvalid),
        };

        let cert_bytes = ssh_util::build_ssh_certificate(
            cert_type_str,
            pubkey_data,
            serial,
            &payload.key_id,
            &payload.valid_principals,
            valid_after,
            valid_before,
            cert_type,
            &extensions,
            &ca_key,
            ca_nid,
        )?;

        let signed_key = ssh_util::format_openssh_cert(cert_type_str, &cert_bytes);

        let serial_hex = format!("{:016x}", serial);
        let cert_entry =
            StorageEntry::new(format!("ssh/certs/{serial_hex}").as_str(), &signed_key)?;
        req.storage_put(&cert_entry).await?;

        info!(
            role = %role_name,
            key_id = %payload.key_id,
            serial = %serial_hex,
            principals = ?payload.valid_principals,
            cert_type = %role.cert_type,
            valid_before = valid_before,
            "SSH certificate signed"
        );

        let response = types::SshSignKeyResponse {
            signed_key,
            serial_number: serial_hex,
            expiration: valid_before as i64,
        };

        Ok(Some(Response::data_response(response.to_map()?)))
    }
}
