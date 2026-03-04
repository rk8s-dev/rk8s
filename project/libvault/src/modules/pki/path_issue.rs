use std::time::{Duration, SystemTime, UNIX_EPOCH};

use base64::Engine;
use humantime::parse_duration;
use openssl::{asn1::Asn1Time, pkey::PKey, x509::X509NameBuilder};
use rand::Rng;
use serde_json::{Map, Value};
use tracing::info;

use super::{PkiBackend, PkiBackendInner, ssh_util, types};
use crate::{
    errors::RvError,
    logical::{Backend, Field, FieldType, Operation, Path, Request, Response},
    modules::{RequestExt, ResponseExt},
    utils,
    utils::cert,
};

impl PkiBackend {
    pub fn issue_path(&self) -> Path {
        let backend = self.inner.clone();

        Path::builder()
            .pattern(r"issue/(?P<cert_type>tls|ssh)/(?P<role>\w[\w-]*)")
            .field(
                "cert_type",
                Field::builder()
                    .field_type(FieldType::Str)
                    .required(true)
                    .description("Certificate type: tls or ssh"),
            )
            .field(
                "role",
                Field::builder()
                    .field_type(FieldType::Str)
                    .required(true)
                    .description("The desired role with configuration for this request"),
            )
            // TLS fields
            .field(
                "common_name",
                Field::builder()
                    .field_type(FieldType::Str)
                    .description("The requested common name"),
            )
            .field(
                "alt_names",
                Field::builder()
                    .field_type(FieldType::Str)
                    .description("Subject Alternative Names, comma-delimited"),
            )
            .field(
                "ip_sans",
                Field::builder()
                    .field_type(FieldType::Str)
                    .description("IP SANs, comma-delimited"),
            )
            .field(
                "ttl",
                Field::builder()
                    .field_type(FieldType::Str)
                    .description("Requested Time To Live"),
            )
            // SSH fields
            .field(
                "key_id",
                Field::builder()
                    .field_type(FieldType::Str)
                    .description("Key identifier for the SSH certificate"),
            )
            .operation(Operation::Write, {
                let handler = backend.clone();
                move |backend, req| {
                    let handler = handler.clone();
                    Box::pin(async move { handler.dispatch_issue(backend, req).await })
                }
            })
            .help("Issue a new certificate (TLS or SSH).")
            .build()
    }

    pub fn sign_path(&self) -> Path {
        let backend = self.inner.clone();

        Path::builder()
            .pattern(r"sign/(?P<cert_type>tls|ssh)/(?P<role>\w[\w-]*)")
            .field(
                "cert_type",
                Field::builder()
                    .field_type(FieldType::Str)
                    .required(true)
                    .description("Certificate type: tls or ssh"),
            )
            .field(
                "role",
                Field::builder()
                    .field_type(FieldType::Str)
                    .required(true)
                    .description("Role name"),
            )
            // SSH sign fields
            .field(
                "public_key",
                Field::builder()
                    .field_type(FieldType::Str)
                    .description("OpenSSH public key to sign (SSH)"),
            )
            .field(
                "key_id",
                Field::builder()
                    .field_type(FieldType::Str)
                    .description("Key identifier"),
            )
            .field(
                "ttl",
                Field::builder()
                    .field_type(FieldType::Str)
                    .description("Requested TTL"),
            )
            // TLS sign fields (CSR)
            .field(
                "csr",
                Field::builder()
                    .field_type(FieldType::Str)
                    .description("PEM-encoded CSR (TLS)"),
            )
            .operation(Operation::Write, {
                let handler = backend.clone();
                move |backend, req| {
                    let handler = handler.clone();
                    Box::pin(async move { handler.dispatch_sign(backend, req).await })
                }
            })
            .help("Sign a CSR (TLS) or public key (SSH).")
            .build()
    }
}

impl PkiBackendInner {
    // ── Dispatch ──

    pub async fn dispatch_issue(
        &self,
        backend: &dyn Backend,
        req: &mut Request,
    ) -> Result<Option<Response>, RvError> {
        let ct = req.get_data("cert_type")?;
        let ct = ct.as_str().ok_or(RvError::ErrRequestFieldInvalid)?;
        match ct {
            "tls" => self.issue_cert(backend, req).await,
            "ssh" => self.ssh_issue_cert(backend, req).await,
            _ => Err(RvError::ErrRequestFieldInvalid),
        }
    }

    pub async fn dispatch_sign(
        &self,
        backend: &dyn Backend,
        req: &mut Request,
    ) -> Result<Option<Response>, RvError> {
        let ct = req.get_data("cert_type")?;
        let ct = ct.as_str().ok_or(RvError::ErrRequestFieldInvalid)?;
        match ct {
            "tls" => {
                // TLS CSR signing - not yet implemented
                Err(RvError::ErrPkiKeyOperationInvalid)
            }
            "ssh" => self.ssh_sign_key(backend, req).await,
            _ => Err(RvError::ErrRequestFieldInvalid),
        }
    }

    // ── TLS issue ──

    pub async fn issue_cert(
        &self,
        backend: &dyn Backend,
        req: &mut Request,
    ) -> Result<Option<Response>, RvError> {
        let payload: types::IssueCertificateRequest = req.parse_json()?;

        let mut common_names = Vec::new();

        let common_name = payload.common_name.unwrap_or_default();
        if !common_name.is_empty() {
            common_names.push(common_name.clone());
        }

        if let Some(alt_names) = payload.alt_names
            && !alt_names.is_empty()
        {
            for v in alt_names.split(',') {
                common_names.push(v.to_string());
            }
        }

        let role = self
            .get_role(
                req,
                req.get_data("role")?
                    .as_str()
                    .ok_or(RvError::ErrRequestFieldInvalid)?,
            )
            .await?;
        if role.is_none() {
            return Err(RvError::ErrPkiRoleNotFound);
        }

        let role_entry = role.unwrap();

        let mut ip_sans = Vec::new();
        if let Some(ip_sans_str) = payload.ip_sans
            && !ip_sans_str.is_empty()
        {
            for v in ip_sans_str.split(',') {
                ip_sans.push(v.to_string());
            }
        }

        let ca_bundle = self.fetch_ca_bundle(req).await?;
        let not_before = SystemTime::now() - Duration::from_secs(10);
        let mut not_after = not_before + parse_duration("30d").unwrap();

        if let Some(ttl) = payload.ttl {
            let ttl_dur = parse_duration(ttl.as_str())?;
            let req_ttl_not_after_dur = SystemTime::now() + ttl_dur;
            let req_ttl_not_after = Asn1Time::from_unix(
                req_ttl_not_after_dur.duration_since(UNIX_EPOCH)?.as_secs() as i64,
            )?;
            let ca_not_after = ca_bundle.certificate.not_after();
            match ca_not_after.compare(&req_ttl_not_after) {
                Ok(ret) => {
                    if ret == std::cmp::Ordering::Less {
                        return Err(RvError::ErrRequestInvalid);
                    }
                    not_after = req_ttl_not_after_dur;
                }
                Err(err) => {
                    return Err(RvError::OpenSSL { source: err });
                }
            }
        }

        let mut subject_name = X509NameBuilder::new().unwrap();
        if !role_entry.country.is_empty() {
            subject_name.append_entry_by_text("C", &role_entry.country)?;
        }
        if !role_entry.province.is_empty() {
            subject_name.append_entry_by_text("ST", &role_entry.province)?;
        }
        if !role_entry.locality.is_empty() {
            subject_name.append_entry_by_text("L", &role_entry.locality)?;
        }
        if !role_entry.organization.is_empty() {
            subject_name.append_entry_by_text("O", &role_entry.organization)?;
        }
        if !role_entry.ou.is_empty() {
            subject_name.append_entry_by_text("OU", &role_entry.ou)?;
        }
        if !common_name.is_empty() {
            subject_name.append_entry_by_text("CN", &common_name)?;
        }
        let subject = subject_name.build();

        let mut cert_obj = cert::Certificate {
            not_before,
            not_after,
            subject,
            dns_sans: common_names,
            ip_sans,
            key_type: role_entry.key_type.clone(),
            key_bits: role_entry.key_bits,
            ..cert::Certificate::default()
        };

        let cert_bundle =
            cert_obj.to_cert_bundle(Some(&ca_bundle.certificate), Some(&ca_bundle.private_key))?;

        if !role_entry.no_store {
            let serial_number_hex = cert_bundle.serial_number.replace(':', "-").to_lowercase();
            self.store_cert(req, &serial_number_hex, &cert_bundle.certificate)
                .await?;
        }

        let cert_expiration =
            utils::asn1time_to_timestamp(cert_bundle.certificate.not_after().to_string().as_str())?;
        let ca_chain_pem: String = cert_bundle
            .ca_chain
            .iter()
            .map(|x509| x509.to_pem().unwrap())
            .map(|pem| String::from_utf8_lossy(&pem).to_string())
            .collect::<Vec<String>>()
            .join("");

        let response = types::IssueCertificateResponse {
            certificate: String::from_utf8_lossy(&cert_bundle.certificate.to_pem()?).to_string(),
            private_key: String::from_utf8_lossy(
                &cert_bundle.private_key.private_key_to_pem_pkcs8()?,
            )
            .to_string(),
            private_key_type: cert_bundle.private_key_type.clone(),
            serial_number: cert_bundle.serial_number.clone(),
            issuing_ca: String::from_utf8_lossy(&ca_bundle.certificate.to_pem()?).to_string(),
            ca_chain: ca_chain_pem,
            expiration: cert_expiration,
        };

        if role_entry.generate_lease {
            let mut secret_data: Map<String, Value> = Map::new();
            secret_data.insert(
                "serial_number".to_string(),
                Value::String(cert_bundle.serial_number.clone()),
            );

            let mut resp = backend
                .secret("pki")
                .unwrap()
                .response(response.to_map()?, Some(secret_data));
            let secret = resp.secret.as_mut().unwrap();

            let now_timestamp = SystemTime::now().duration_since(UNIX_EPOCH)?;

            secret.lease.ttl = Duration::from_secs(cert_expiration as u64) - now_timestamp;
            secret.lease.renewable = true;

            Ok(Some(resp))
        } else {
            Ok(Some(Response::data_response(response.to_map()?)))
        }
    }

    // ── SSH issue ──

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

        let (user_key, user_nid) = ssh_util::generate_ssh_keypair(&role.key_type, role.key_bits)?;

        let cert_type = match role.cert_type.as_str() {
            "user" => ssh_util::SSH_CERT_TYPE_USER,
            "host" => ssh_util::SSH_CERT_TYPE_HOST,
            _ => return Err(RvError::ErrPkiSshCertTypeInvalid),
        };

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

        let cert_type_str = ssh_util::ssh_cert_type_str(&user_key, user_nid)?;
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

        let serial_hex = format!("{:016x}", serial);
        self.store_ssh_cert(req, &serial_hex, &signed_key).await?;

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

    // ── SSH sign ──

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

        let parts: Vec<&str> = payload.public_key.split_whitespace().collect();
        if parts.len() < 2 {
            return Err(RvError::ErrPkiSshPublicKeyInvalid);
        }
        let key_type_str = parts[0];
        let key_data = base64::engine::general_purpose::STANDARD
            .decode(parts[1])
            .map_err(|_| RvError::ErrPkiSshPublicKeyInvalid)?;

        match key_type_str {
            "ssh-rsa"
            | "ecdsa-sha2-nistp256"
            | "ecdsa-sha2-nistp384"
            | "ecdsa-sha2-nistp521"
            | "ssh-ed25519" => {}
            _ => return Err(RvError::ErrPkiSshPublicKeyInvalid),
        }

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

        let cert_type_str_out = match key_type_str {
            "ssh-rsa" => "ssh-rsa-cert-v01@openssh.com",
            "ecdsa-sha2-nistp256" => "ecdsa-sha2-nistp256-cert-v01@openssh.com",
            "ecdsa-sha2-nistp384" => "ecdsa-sha2-nistp384-cert-v01@openssh.com",
            "ecdsa-sha2-nistp521" => "ecdsa-sha2-nistp521-cert-v01@openssh.com",
            "ssh-ed25519" => "ssh-ed25519-cert-v01@openssh.com",
            _ => return Err(RvError::ErrPkiSshPublicKeyInvalid),
        };

        let cert_bytes = ssh_util::build_ssh_certificate(
            cert_type_str_out,
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

        let signed_key = ssh_util::format_openssh_cert(cert_type_str_out, &cert_bytes);

        let serial_hex = format!("{:016x}", serial);
        self.store_ssh_cert(req, &serial_hex, &signed_key).await?;

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
