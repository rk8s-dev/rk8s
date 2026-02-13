use std::time::{SystemTime, UNIX_EPOCH};

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
    pub fn ssh_issue_path(&self) -> Path {
        let backend = self.inner.clone();

        Path::builder()
            .pattern(r"ssh/issue/(?P<role>\w[\w-]+\w)")
            .field(
                "role",
                Field::builder()
                    .field_type(FieldType::Str)
                    .required(true)
                    .description("SSH role name"),
            )
            .field(
                "key_id",
                Field::builder()
                    .field_type(FieldType::Str)
                    .required(true)
                    .description("Key identifier for the certificate"),
            )
            .field(
                "ttl",
                Field::builder()
                    .field_type(FieldType::Str)
                    .description("Requested TTL for the certificate"),
            )
            .operation(Operation::Write, {
                let handler = backend.clone();
                move |backend, req| {
                    let handler = handler.clone();
                    Box::pin(async move { handler.ssh_issue_cert(backend, req).await })
                }
            })
            .help("Generate a new SSH key pair and issue a signed certificate.")
            .build()
    }
}

impl PkiBackendInner {
    pub async fn ssh_issue_cert(
        &self,
        _backend: &dyn Backend,
        req: &mut Request,
    ) -> Result<Option<Response>, RvError> {
        let payload: types::SshIssueCertificateRequest = req.parse_json()?;

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

        // Generate user key pair
        let (user_key, user_nid) = ssh_util::generate_ssh_keypair(&role.key_type, role.key_bits)?;

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

        let cert_type_str = ssh_util::ssh_cert_type_str(&ca_key, ca_nid)?;
        let user_pubkey_data = ssh_util::encode_pubkey_for_cert(&user_key, user_nid)?;

        let cert_bytes = ssh_util::build_ssh_certificate(
            cert_type_str,
            &user_pubkey_data,
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
        let public_key = ssh_util::format_openssh_pubkey(&user_key, user_nid)?;
        let private_key =
            String::from_utf8_lossy(&user_key.private_key_to_pem_pkcs8()?).to_string();

        // Store the certificate
        let serial_hex = format!("{:016x}", serial);
        let cert_entry = StorageEntry {
            key: format!("ssh/certs/{serial_hex}"),
            value: signed_key.clone().into_bytes(),
        };
        req.storage_put(&cert_entry).await?;

        info!(
            role = %role_name,
            key_id = %payload.key_id,
            serial = %serial_hex,
            principals = ?payload.valid_principals,
            cert_type = %role.cert_type,
            key_type = %role.key_type,
            valid_before = valid_before,
            "SSH certificate issued"
        );

        let response = types::SshIssueCertificateResponse {
            signed_key,
            private_key,
            public_key,
            serial_number: serial_hex,
            expiration: valid_before as i64,
        };

        Ok(Some(Response::data_response(response.to_map()?)))
    }
}
