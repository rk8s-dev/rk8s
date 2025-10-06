use std::time::Duration;

use derive_more::{Deref, DerefMut};
use openssl::x509::X509;
use openssl_sys::XKU_SSL_CLIENT;
use serde::{Deserialize, Serialize};

use super::{CertBackend, CertBackendInner};
use crate::{
    errors::RvError,
    logical::{Backend, Field, FieldType, Operation, Path, Request, Response, field::FieldTrait},
    rv_error_response,
    storage::StorageEntry,
    utils::{
        cert::{
            deserialize_vec_x509, has_x509_ext_key_usage, has_x509_ext_key_usage_flag, is_ca_cert,
            serialize_vec_x509,
        },
        deserialize_duration, serialize_duration,
        sock_addr::SockAddrMarshaler,
        token_util::{TokenParams, token_fields},
    },
};

//const DEFAULT_MAX_TTL: Duration = Duration::from_secs(365*24*60*60 as u64);

#[derive(Debug, Clone, Default, Serialize, Deserialize, Deref, DerefMut)]
pub struct CertEntry {
    pub name: String,
    pub display_name: String,
    #[serde(
        serialize_with = "serialize_vec_x509",
        deserialize_with = "deserialize_vec_x509"
    )]
    pub certificate: Vec<X509>,
    pub policies: Vec<String>,
    #[serde(
        serialize_with = "serialize_duration",
        deserialize_with = "deserialize_duration"
    )]
    pub ttl: Duration,
    #[serde(
        serialize_with = "serialize_duration",
        deserialize_with = "deserialize_duration"
    )]
    pub max_ttl: Duration,
    #[serde(
        serialize_with = "serialize_duration",
        deserialize_with = "deserialize_duration"
    )]
    pub period: Duration,
    pub bound_cidrs: Vec<SockAddrMarshaler>,
    pub allowed_names: Vec<String>,
    pub allowed_common_names: Vec<String>,
    pub allowed_dns_sans: Vec<String>,
    pub allowed_email_sans: Vec<String>,
    pub allowed_uri_sans: Vec<String>,
    pub allowed_organizational_units: Vec<String>,
    pub allowed_metadata_extensions: Vec<String>,
    pub required_extensions: Vec<String>,
    pub ocsp_enabled: bool,
    #[serde(
        serialize_with = "serialize_vec_x509",
        deserialize_with = "deserialize_vec_x509"
    )]
    pub ocsp_ca_certificates: Vec<X509>,
    pub ocsp_servers_override: Vec<String>,
    pub ocsp_fail_open: bool,
    pub ocsp_query_all_servers: bool,
    #[serde(flatten)]
    #[deref]
    #[deref_mut]
    pub token_params: TokenParams,
}

impl CertBackend {
    pub fn certs_path(&self) -> Path {
        let backend_read = self.inner.clone();
        let backend_write = self.inner.clone();
        let backend_delete = self.inner.clone();

        let mut path = Path::builder()
            .pattern(r"certs/(?P<name>\w[\w-]+\w)")
            .field(
                "name",
                Field::builder()
                    .field_type(FieldType::Str)
                    .required(true)
                    .description("The name of the certificate."),
            )
            .field(
                "certificate",
                Field::builder()
                    .field_type(FieldType::Str)
                    .required(true)
                    .description(
                        "The public certificate that should be trusted. Must be x509 PEM encoded.",
                    ),
            )
            .field(
                "ocsp_enabled",
                Field::builder()
                    .field_type(FieldType::Bool)
                    .default_value(false)
                    .description("Whether to attempt OCSP verification of certificates at login"),
            )
            .field(
                "ocsp_ca_certificates",
                Field::builder()
                    .field_type(FieldType::Str)
                    .description("Any additional CA certificates needed to communicate with OCSP servers"),
            )
            .field(
                "ocsp_servers_override",
                Field::builder()
                    .field_type(FieldType::CommaStringSlice)
                    .description(
                        "A comma-separated list of OCSP server addresses.\nIf unset, the OCSP server is determined from the AuthorityInformationAccess extension on\nthe certificate being inspected.",
                    ),
            )
            .field(
                "ocsp_fail_open",
                Field::builder()
                    .field_type(FieldType::Bool)
                    .default_value(false)
                    .description(
                        "If set to true, if an OCSP revocation cannot\nbe made successfully, login will proceed rather than failing.  If false, failing\nto get an OCSP status fails the request.",
                    ),
            )
            .field(
                "ocsp_query_all_servers",
                Field::builder()
                    .field_type(FieldType::Bool)
                    .default_value(false)
                    .description(
                        "If set to true, rather than accepting the first\nsuccessful OCSP response, query all servers and consider the certificate valid\nonly if all servers agree.",
                    ),
            )
            .field(
                "allowed_names",
                Field::builder()
                    .field_type(FieldType::CommaStringSlice)
                    .description(
                        "A comma-separated list of names.\nAt least one must exist in either the Common Name or SANs. Supports globbing.\nThis parameter is deprecated, please use allowed_common_names, allowed_dns_sans,\nallowed_email_sans, allowed_uri_sans.",
                    ),
            )
            .field(
                "allowed_common_names",
                Field::builder()
                    .field_type(FieldType::CommaStringSlice)
                    .description(
                        "A comma-separated list of names.\n        At least one must exist in the Common Name. Supports globbing.",
                    ),
            )
            .field(
                "allowed_dns_sans",
                Field::builder()
                    .field_type(FieldType::CommaStringSlice)
                    .description(
                        "A comma-separated list of DNS names.\n        At least one must exist in the SANs. Supports globbing.",
                    ),
            )
            .field(
                "allowed_email_sans",
                Field::builder()
                    .field_type(FieldType::CommaStringSlice)
                    .description(
                        "A comma-separated list of Email Addresses.\n        At least one must exist in the SANs. Supports globbing.",
                    ),
            )
            .field(
                "allowed_uri_sans",
                Field::builder()
                    .field_type(FieldType::CommaStringSlice)
                    .description(
                        "A comma-separated list of URIs.\n        At least one must exist in the SANs. Supports globbing.",
                    ),
            )
            .field(
                "allowed_organizational_units",
                Field::builder()
                    .field_type(FieldType::CommaStringSlice)
                    .description(
                        "A comma-separated list of Organizational Units names.\n        At least one must exist in the OU field.",
                    ),
            )
            .field(
                "required_extensions",
                Field::builder()
                    .field_type(FieldType::CommaStringSlice)
                    .description(
                        "A comma-separated string or array of extensions\nformatted as \"oid:value\". Expects the extension value to be some type of ASN1 encoded string.\nAll values much match. Supports globbing on \"value\".",
                    ),
            )
            .field(
                "allowed_metadata_extensions",
                Field::builder()
                    .field_type(FieldType::CommaStringSlice)
                    .description(
                        "A comma-separated string or array of oid extensions.\nUpon successful authentication, these extensions will be added as metadata if they are present\nin the certificate. The metadata key will be the string consisting of the oid numbers\nseparated by a dash (-) instead of a dot (.) to allow usage in ACL templates.",
                    ),
            )
            .field(
                "policies",
                Field::builder()
                    .field_type(FieldType::CommaStringSlice)
                    .description(
                        "Use token_policies instead. If this and token_policies are both speicified, only token_policies will be used.",
                    ),
            )
            .field(
                "lease",
                Field::builder()
                    .field_type(FieldType::Int)
                    .description(
                        "Use token_ttl instead. If this and token_ttl are both speicified, only token_ttl will be used.",
                    ),
            )
            .field(
                "ttl",
                Field::builder()
                    .field_type(FieldType::DurationSecond)
                    .description(
                        "Use token_ttl instead. If this and token_ttl are both speicified, only token_ttl will be used.",
                    ),
            )
            .field(
                "max_ttl",
                Field::builder()
                    .field_type(FieldType::DurationSecond)
                    .description(
                        "Use token_max_ttl instead. If this and token_max_ttl are both speicified, only token_max_ttl will be used.",
                    ),
            )
            .field(
                "period",
                Field::builder()
                    .field_type(FieldType::DurationSecond)
                    .default_value(0)
                    .description(
                        "Use token_period instead. If this and token_period are both speicified, only token_period will be used.",
                    ),
            )
            .field(
                "bound_cidrs",
                Field::builder()
                    .field_type(FieldType::CommaStringSlice)
                    .description(
                        "Use token_bound_cidrs instead. If this and token_bound_cidrs are both speicified, only token_bound_cidrs will be used.",
                    ),
            )
            .field(
                "display_name",
                Field::builder()
                    .field_type(FieldType::Str)
                    .description("The display name to use for clients using this certificate."),
            )
            .operation(Operation::Read, {
                let handler = backend_read.clone();
                move |backend, req| {
                    let handler = handler.clone();
                    Box::pin(async move { handler.read_cert(backend, req).await })
                }
            })
            .operation(Operation::Write, {
                let handler = backend_write.clone();
                move |backend, req| {
                    let handler = handler.clone();
                    Box::pin(async move { handler.write_cert(backend, req).await })
                }
            })
            .operation(Operation::Delete, {
                let handler = backend_delete.clone();
                move |backend, req| {
                    let handler = handler.clone();
                    Box::pin(async move { handler.delete_cert(backend, req).await })
                }
            })
            .help(
                "\nThis endpoint allows you to create, read, update, and delete trusted certificates\nthat are allowed to authenticate.\n\nDeleting a certificate will not revoke auth for prior authenticated connections.\nTo do this, do a revoke on \"login\". If you don't need to revoke login immediately,\nthen the next renew will cause the lease to expire.\n                ",
            )
            .build();

        path.fields.extend(token_fields());

        path
    }

    pub fn certs_list_path(&self) -> Path {
        let cert_backend_ref = self.inner.clone();

        Path::builder()
            .pattern(r"certs/?")
            .operation(Operation::List, {
                let handler = cert_backend_ref.clone();
                move |backend, req| {
                    let handler = handler.clone();
                    Box::pin(async move { handler.list_cert(backend, req).await })
                }
            })
            .help("This endpoint allows you to list certs")
            .build()
    }
}

#[allow(clippy::assigning_clones)]
impl CertBackendInner {
    pub async fn get_cert(&self, req: &Request, name: &str) -> Result<Option<CertEntry>, RvError> {
        let key = format!("cert/{}", name.to_lowercase());
        let storage_entry = req.storage_get(&key).await?;
        if storage_entry.is_none() {
            return Ok(None);
        }

        let entry = storage_entry.unwrap();
        let mut cert_entry: CertEntry = serde_json::from_slice(entry.value.as_slice())?;

        if cert_entry.token_ttl.as_secs() == 0 && cert_entry.ttl.as_secs() > 0 {
            cert_entry.token_ttl = cert_entry.ttl;
        }
        if cert_entry.token_max_ttl.as_secs() == 0 && cert_entry.max_ttl.as_secs() > 0 {
            cert_entry.token_max_ttl = cert_entry.max_ttl;
        }
        if cert_entry.token_period.as_secs() == 0 && cert_entry.period.as_secs() > 0 {
            cert_entry.token_period = cert_entry.period;
        }
        if cert_entry.token_policies.is_empty() && !cert_entry.policies.is_empty() {
            cert_entry.token_policies = cert_entry.policies.clone();
        }
        if cert_entry.token_bound_cidrs.is_empty() && !cert_entry.bound_cidrs.is_empty() {
            cert_entry.token_bound_cidrs = cert_entry.bound_cidrs.clone();
        }

        Ok(Some(cert_entry))
    }

    pub async fn set_cert(
        &self,
        req: &Request,
        name: &str,
        cert_entry: &CertEntry,
    ) -> Result<(), RvError> {
        let entry = StorageEntry::new(format!("cert/{name}").as_str(), cert_entry)?;

        req.storage_put(&entry).await
    }

    pub async fn read_cert(
        &self,
        _backend: &dyn Backend,
        req: &Request,
    ) -> Result<Option<Response>, RvError> {
        let name = req.get_data_as_str("name")?.to_lowercase();

        let entry = self.get_cert(req, &name).await?;
        if entry.is_none() {
            return Ok(None);
        }

        let cert_entry = entry.unwrap();
        let mut cert_entry_data = serde_json::to_value(&cert_entry)?;
        let data = cert_entry_data.as_object_mut().unwrap();

        if cert_entry.ttl.as_secs() == 0 {
            data.remove("ttl");
        }

        if cert_entry.max_ttl.as_secs() == 0 {
            data.remove("max_ttl");
        }

        if !cert_entry.policies.is_empty() {
            data["policies"] = data["token_policies"].clone();
        }

        if !cert_entry.bound_cidrs.is_empty() {
            data["bound_cidrs"] = data["token_bound_cidrs"].clone();
        }

        Ok(Some(Response::data_response(Some(data.clone()))))
    }

    pub async fn write_cert(
        &self,
        _backend: &dyn Backend,
        req: &Request,
    ) -> Result<Option<Response>, RvError> {
        let name = req.get_data_as_str("name")?.to_lowercase();

        let mut cert_entry = CertEntry::default();

        let entry = self.get_cert(req, &name).await?;
        if entry.is_some() {
            cert_entry = entry.unwrap();
        } else {
            cert_entry.name.clone_from(&name);
        }

        if let Ok(certificate_raw) = req.get_data("certificate") {
            let certificate = certificate_raw
                .as_str()
                .ok_or(RvError::ErrRequestFieldInvalid)?;
            cert_entry.certificate = X509::stack_from_pem(certificate.as_bytes())?;
        }

        if let Ok(ocsp_ca_certificates_raw) = req.get_data("ocsp_ca_certificates") {
            let ocsp_ca_certificates = ocsp_ca_certificates_raw
                .as_str()
                .ok_or(RvError::ErrRequestFieldInvalid)?;
            cert_entry.ocsp_ca_certificates =
                X509::stack_from_pem(ocsp_ca_certificates.as_bytes())?;
        }

        if let Ok(ocsp_enabled_raw) = req.get_data("ocsp_enabled") {
            cert_entry.ocsp_enabled = ocsp_enabled_raw
                .as_bool()
                .ok_or(RvError::ErrRequestFieldInvalid)?;
        }

        if let Ok(ocsp_servers_override_raw) = req.get_data("ocsp_servers_override") {
            cert_entry.ocsp_servers_override = ocsp_servers_override_raw
                .as_comma_string_slice()
                .ok_or(RvError::ErrRequestFieldInvalid)?;
        }

        if let Ok(ocsp_fail_open_raw) = req.get_data("ocsp_fail_open") {
            cert_entry.ocsp_fail_open = ocsp_fail_open_raw
                .as_bool()
                .ok_or(RvError::ErrRequestFieldInvalid)?;
        }

        if let Ok(ocsp_query_all_servers_raw) = req.get_data("ocsp_query_all_servers") {
            cert_entry.ocsp_query_all_servers = ocsp_query_all_servers_raw
                .as_bool()
                .ok_or(RvError::ErrRequestFieldInvalid)?;
        }

        if let Ok(display_name_raw) = req.get_data("display_name") {
            cert_entry.display_name = display_name_raw
                .as_str()
                .ok_or(RvError::ErrRequestFieldInvalid)?
                .to_string();
        }

        if let Ok(allowed_names_raw) = req.get_data("allowed_names") {
            cert_entry.allowed_names = allowed_names_raw
                .as_comma_string_slice()
                .ok_or(RvError::ErrRequestFieldInvalid)?;
        }

        if let Ok(allowed_common_names_raw) = req.get_data("allowed_common_names") {
            cert_entry.allowed_common_names = allowed_common_names_raw
                .as_comma_string_slice()
                .ok_or(RvError::ErrRequestFieldInvalid)?;
        }

        if let Ok(allowed_dns_sans_raw) = req.get_data("allowed_dns_sans") {
            cert_entry.allowed_dns_sans = allowed_dns_sans_raw
                .as_comma_string_slice()
                .ok_or(RvError::ErrRequestFieldInvalid)?;
        }

        if let Ok(allowed_email_sans_raw) = req.get_data("allowed_email_sans") {
            cert_entry.allowed_email_sans = allowed_email_sans_raw
                .as_comma_string_slice()
                .ok_or(RvError::ErrRequestFieldInvalid)?;
        }

        if let Ok(allowed_uri_sans_raw) = req.get_data("allowed_uri_sans") {
            cert_entry.allowed_uri_sans = allowed_uri_sans_raw
                .as_comma_string_slice()
                .ok_or(RvError::ErrRequestFieldInvalid)?;
        }

        if let Ok(allowed_organizational_units_raw) = req.get_data("allowed_organizational_units") {
            cert_entry.allowed_organizational_units = allowed_organizational_units_raw
                .as_comma_string_slice()
                .ok_or(RvError::ErrRequestFieldInvalid)?;
        }

        if let Ok(required_extensions_raw) = req.get_data("required_extensions") {
            cert_entry.required_extensions = required_extensions_raw
                .as_comma_string_slice()
                .ok_or(RvError::ErrRequestFieldInvalid)?;
        }

        if let Ok(allowed_metadata_extensions_raw) = req.get_data("allowed_metadata_extensions") {
            cert_entry.allowed_metadata_extensions = allowed_metadata_extensions_raw
                .as_comma_string_slice()
                .ok_or(RvError::ErrRequestFieldInvalid)?;
        }

        let old_token_policies = cert_entry.token_policies.clone();
        let old_token_period = cert_entry.token_period;
        let old_token_ttl = cert_entry.token_ttl;
        let old_token_max_ttl = cert_entry.token_max_ttl;
        let old_token_bound_cidrs = cert_entry.token_bound_cidrs.clone();

        cert_entry.token_params.parse_token_fields(req)?;

        if old_token_policies != cert_entry.token_policies {
            cert_entry.policies = cert_entry.token_policies.clone();
        } else if let Ok(policies_value) = req.get_data("policies") {
            let policies = policies_value
                .as_comma_string_slice()
                .ok_or(RvError::ErrRequestFieldInvalid)?;
            cert_entry.policies.clone_from(&policies);
            cert_entry.token_policies = policies;
        }

        if old_token_period != cert_entry.token_period {
            cert_entry.period = cert_entry.token_period;
        } else if let Ok(period_value) = req.get_data("period") {
            let period = period_value
                .as_duration()
                .ok_or(RvError::ErrRequestFieldInvalid)?;
            cert_entry.period = period;
            cert_entry.token_period = period;
        }

        if old_token_ttl != cert_entry.token_ttl {
            cert_entry.ttl = cert_entry.token_ttl;
        } else if let Ok(ttl_value) = req.get_data("ttl") {
            let ttl = ttl_value
                .as_duration()
                .ok_or(RvError::ErrRequestFieldInvalid)?;
            cert_entry.ttl = ttl;
            cert_entry.token_ttl = ttl;
        } else if let Ok(lease_value) = req.get_data("lease") {
            let lease = lease_value
                .as_u64()
                .ok_or(RvError::ErrRequestFieldInvalid)?;
            cert_entry.ttl = Duration::from_secs(lease);
            cert_entry.token_ttl = cert_entry.ttl;
        }

        if old_token_max_ttl != cert_entry.token_max_ttl {
            cert_entry.max_ttl = cert_entry.token_max_ttl;
        } else if let Ok(max_ttl_value) = req.get_data("max_ttl") {
            let max_ttl = max_ttl_value
                .as_duration()
                .ok_or(RvError::ErrRequestFieldInvalid)?;
            cert_entry.max_ttl = max_ttl;
            cert_entry.token_max_ttl = max_ttl;
        }

        if old_token_bound_cidrs != cert_entry.token_bound_cidrs {
            cert_entry.bound_cidrs = cert_entry.token_bound_cidrs.clone();
        } else if let Ok(bound_cidrs_value) = req.get_data("bound_cidrs") {
            let bound_cidrs = bound_cidrs_value
                .as_comma_string_slice()
                .ok_or(RvError::ErrRequestFieldInvalid)?;
            cert_entry.bound_cidrs = bound_cidrs
                .iter()
                .map(|s| SockAddrMarshaler::from_str(s))
                .collect::<Result<Vec<SockAddrMarshaler>, _>>()?;
            cert_entry.token_bound_cidrs = cert_entry.bound_cidrs.clone();
        }

        if cert_entry.display_name.is_empty() {
            cert_entry.display_name.clone_from(&name);
        }

        //TODO: TTL check

        //If the certificate is not a CA cert, then ensure that x509.ExtKeyUsageClientAuth is set
        let cert = &cert_entry.certificate[0];
        if !is_ca_cert(cert)
            && has_x509_ext_key_usage(cert)
            && !has_x509_ext_key_usage_flag(cert, XKU_SSL_CLIENT)
        {
            return Err(rv_error_response!(
                "nonCA certificates should have TLS client authentication set as an extended key usage"
            ));
        }

        self.set_cert(req, &name, &cert_entry).await?;

        Ok(None)
    }

    pub async fn delete_cert(
        &self,
        _backend: &dyn Backend,
        req: &Request,
    ) -> Result<Option<Response>, RvError> {
        let name = req.get_data_as_str("name")?.to_lowercase();

        req.storage_delete(format!("cert/{name}").as_str()).await?;
        Ok(None)
    }

    pub async fn list_cert(
        &self,
        _backend: &dyn Backend,
        req: &Request,
    ) -> Result<Option<Response>, RvError> {
        let certs = req.storage_list("cert/".to_string().as_str()).await?;
        let resp = Response::list_response(&certs);
        Ok(Some(resp))
    }
}
