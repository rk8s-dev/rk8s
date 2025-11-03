use std::time::{Duration, SystemTime, UNIX_EPOCH};

use humantime::parse_duration;
use openssl::{asn1::Asn1Time, x509::X509NameBuilder};
use serde_json::{Map, Value};

use super::{PkiBackend, PkiBackendInner, types};
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
            .pattern(r"issue/(?P<role>\w[\w-]+\w)")
            .field(
                "role",
                Field::builder()
                    .field_type(FieldType::Str)
                    .description("The desired role with configuration for this request"),
            )
            .field(
                "common_name",
                Field::builder()
                    .field_type(FieldType::Str)
                    .description(
                        r#"
        The requested common name; if you want more than one, specify the alternative names in the alt_names map"#,
                    ),
            )
            .field(
                "alt_names",
                Field::builder()
                    .field_type(FieldType::Str)
                    .description(
                        r#"
        The requested Subject Alternative Names, if any, in a comma-delimited list"#,
                    ),
            )
            .field(
                "ip_sans",
                Field::builder()
                    .field_type(FieldType::Str)
                    .description(
                        r#"The requested IP SANs, if any, in a comma-delimited list"#,
                    ),
            )
            .field(
                "ttl",
                Field::builder()
                    .field_type(FieldType::Str)
                    .description("Specifies requested Time To Live"),
            )
            .operation(Operation::Write, {
                let handler = backend.clone();
                move |backend, req| {
                    let handler = handler.clone();
                    Box::pin(async move { handler.issue_cert(backend, req).await })
                }
            })
            .help(
                r#"
This path allows requesting certificates to be issued according to the
policy of the given role. The certificate will only be issued if the
requested common name is allowed by the role policy.
                "#,
            )
            .build()
    }
}

impl PkiBackendInner {
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

        let mut cert = cert::Certificate {
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
            cert.to_cert_bundle(Some(&ca_bundle.certificate), Some(&ca_bundle.private_key))?;

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
}
