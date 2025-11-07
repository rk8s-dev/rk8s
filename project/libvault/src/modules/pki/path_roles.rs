use std::time::Duration;

use better_default::Default;
use humantime::parse_duration;
use serde::{Deserialize, Serialize};

use super::{PkiBackend, PkiBackendInner, util::DEFAULT_MAX_TTL};
use crate::{
    errors::RvError,
    logical::{Backend, Field, FieldType, Operation, Path, Request, Response, field::FieldTrait},
    storage::StorageEntry,
    utils::{deserialize_duration, serialize_duration},
};

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct RoleEntry {
    #[serde(
        serialize_with = "serialize_duration",
        deserialize_with = "deserialize_duration"
    )]
    pub ttl: Duration,
    #[serde(
        serialize_with = "serialize_duration",
        deserialize_with = "deserialize_duration"
    )]
    #[default(DEFAULT_MAX_TTL)]
    pub max_ttl: Duration,
    #[serde(
        serialize_with = "serialize_duration",
        deserialize_with = "deserialize_duration"
    )]
    pub not_before_duration: Duration,
    #[default("rsa".to_string())]
    pub key_type: String,
    #[default(2048)]
    pub key_bits: u32,
    #[default(256)]
    pub signature_bits: u32,
    pub use_pss: bool,
    #[default(true)]
    pub allow_localhost: bool,
    #[default(true)]
    pub allow_bare_domains: bool,
    #[default(true)]
    pub allow_subdomains: bool,
    #[default(true)]
    pub allow_any_name: bool,
    #[default(true)]
    pub allow_ip_sans: bool,
    pub server_flag: bool,
    pub client_flag: bool,
    #[default(true)]
    pub use_csr_sans: bool,
    #[default(true)]
    pub use_csr_common_name: bool,
    pub key_usage: Vec<String>,
    pub ext_key_usage: Vec<String>,
    pub country: String,
    pub province: String,
    pub locality: String,
    pub organization: String,
    pub ou: String,
    pub street_address: String,
    pub postal_code: String,
    #[default(true)]
    pub no_store: bool,
    pub generate_lease: bool,
    pub not_after: String,
}

impl PkiBackend {
    pub fn roles_path(&self) -> Path {
        let backend_read = self.inner.clone();
        let backend_write = self.inner.clone();
        let backend_delete = self.inner.clone();

        Path::builder()
            .pattern(r"roles/(?P<name>\w[\w-]+\w)")
            .field(
                "name",
                Field::builder()
                    .field_type(FieldType::Str)
                    .required(true)
                    .description("Name of the role."),
            )
            .field(
                "ttl",
                Field::builder()
                    .field_type(FieldType::Str)
                    .description(
                        r#"
The lease duration (validity period of the certificate) if no specific lease
duration is requested. The lease duration controls the expiration of certificates
issued by this backend. defaults to the system default value or the value of
max_ttl, whichever is shorter."#,
                    ),
            )
            .field(
                "max_ttl",
                Field::builder()
                    .field_type(FieldType::Str)
                    .description(
                        r#"
        The maximum allowed lease duration. If not set, defaults to the system maximum lease TTL."#,
                    ),
            )
            .field(
                "use_pss",
                Field::builder()
                    .field_type(FieldType::Bool)
                    .default_value(false)
                    .description(
                        r#"
        Whether or not to use PSS signatures when using a RSA key-type issuer. Defaults to false."#,
                    ),
            )
            .field(
                "allow_localhost",
                Field::builder()
                    .field_type(FieldType::Bool)
                    .default_value(true)
                    .description(
                        r#"
Whether to allow "localhost" and "localdomain" as a valid common name in a request,
independent of allowed_domains value."#,
                    ),
            )
            .field(
                "allowed_domains",
                Field::builder()
                    .field_type(FieldType::Str)
                    .description(
                        r#"
Specifies the domains this role is allowed to issue certificates for.
This is used with the allow_bare_domains, allow_subdomains, and allow_glob_domains
to determine matches for the common name, DNS-typed SAN entries, and Email-typed
SAN entries of certificates. See the documentation for more information.
This parameter accepts a comma-separated string or list of domains."#,
                    ),
            )
            .field(
                "allow_bare_domains",
                Field::builder()
                    .field_type(FieldType::Bool)
                    .default_value(false)
                    .description(
                        r#"
If set, clients can request certificates for the base domains themselves,
e.g. "example.com" of domains listed in allowed_domains. This is a separate
option as in some cases this can be considered a security threat.
See the documentation for more information."#,
                    ),
            )
            .field(
                "allow_subdomains",
                Field::builder()
                    .field_type(FieldType::Bool)
                    .default_value(false)
                    .description(
                        r#"
If set, clients can request certificates for subdomains of domains listed in
allowed_domains, including wildcard subdomains. See the documentation for more information."#,
                    ),
            )
            .field(
                "allow_any_name",
                Field::builder()
                    .field_type(FieldType::Bool)
                    .default_value(false)
                    .description(
                        r#"
If set, clients can request certificates for any domain, regardless of allowed_domains restrictions.
See the documentation for more information."#,
                    ),
            )
            .field(
                "allow_ip_sans",
                Field::builder()
                    .field_type(FieldType::Bool)
                    .default_value(true)
                    .description(
                        r#"
        If set, IP Subject Alternative Names are allowed. Any valid IP is accepted and No authorization checking is performed."#,
                    ),
            )
            .field(
                "server_flag",
                Field::builder()
                    .field_type(FieldType::Bool)
                    .default_value(true)
                    .description(
                        r#"
        If set, certificates are flagged for server auth use. defaults to true. See also RFC 5280 Section 4.2.1.12."#,
                    ),
            )
            .field(
                "client_flag",
                Field::builder()
                    .field_type(FieldType::Bool)
                    .default_value(true)
                    .description(
                        r#"
        If set, certificates are flagged for client auth use. defaults to true. See also RFC 5280 Section 4.2.1.12."#,
                    ),
            )
            .field(
                "code_signing_flag",
                Field::builder()
                    .field_type(FieldType::Bool)
                    .description(
                        r#"
        If set, certificates are flagged for code signing use. defaults to false. See also RFC 5280 Section 4.2.1.12."#,
                    ),
            )
            .field(
                "key_type",
                Field::builder()
                    .field_type(FieldType::Str)
                    .default_value("rsa")
                    .description(
                        r#"
        The type of key to use; defaults to RSA. "rsa" "ec", "ed25519" and "any" are the only valid values."#,
                    ),
            )
            .field(
                "key_bits",
                Field::builder()
                    .field_type(FieldType::Int)
                    .default_value(0)
                    .description(
                        r#"
The number of bits to use. Allowed values are 0 (universal default); with rsa
key_type: 2048 (default), 3072, or 4096; with ec key_type: 224, 256 (default),
384, or 521; ignored with ed25519."#,
                    ),
            )
            .field(
                "signature_bits",
                Field::builder()
                    .field_type(FieldType::Int)
                    .default_value(0)
                    .description(
                        r#"
The number of bits to use in the signature algorithm; accepts 256 for SHA-2-256,
384 for SHA-2-384, and 512 for SHA-2-512. defaults to 0 to automatically detect
based on key length (SHA-2-256 for RSA keys, and matching the curve size for NIST P-Curves)."#,
                    ),
            )
            .field(
                "key_usage",
                Field::builder()
                    .field_type(FieldType::CommaStringSlice)
                    .default_value("DigitalSignature,KeyAgreement,KeyEncipherment")
                    .description(
                        r#"
A comma-separated string or list of key usages (not extended key usages).
Valid values can be found at https://golang.org/pkg/crypto/x509/#KeyUsage
-- simply drop the "KeyUsage" part of the name.
To remove all key usages from being set, set this value to an empty list. See also RFC 5280 Section 4.2.1.3.
                    "#,
                    ),
            )
            .field(
                "ext_key_usage",
                Field::builder()
                    .field_type(FieldType::CommaStringSlice)
                    .description(
                        r#"
A comma-separated string or list of extended key usages. Valid values can be found at
https://golang.org/pkg/crypto/x509/#ExtKeyUsage -- simply drop the "ExtKeyUsage" part of the name.
To remove all key usages from being set, set this value to an empty list. See also RFC 5280 Section 4.2.1.12.
                    "#,
                    ),
            )
            .field(
                "not_before_duration",
                Field::builder()
                    .field_type(FieldType::Int)
                    .default_value(30)
                    .description(
                        r#"
        The duration before now which the certificate needs to be backdated by."#,
                    ),
            )
            .field(
                "not_after",
                Field::builder()
                    .field_type(FieldType::Str)
                    .default_value("")
                    .description(
                        r#"
Set the not after field of the certificate with specified date value.
The value format should be given in UTC format YYYY-MM-ddTHH:MM:SSZ."#,
                    ),
            )
            .field(
                "ou",
                Field::builder()
                    .field_type(FieldType::Str)
                    .description(
                        r#"
        If set, OU (OrganizationalUnit) will be set to this value in certificates issued by this role."#,
                    ),
            )
            .field(
                "organization",
                Field::builder()
                    .field_type(FieldType::Str)
                    .description(
                        r#"
        If set, O (Organization) will be set to this value in certificates issued by this role."#,
                    ),
            )
            .field(
                "country",
                Field::builder()
                    .field_type(FieldType::Str)
                    .description(
                        r#"
        If set, Country will be set to this value in certificates issued by this role."#,
                    ),
            )
            .field(
                "locality",
                Field::builder()
                    .field_type(FieldType::Str)
                    .description(
                        r#"
        If set, Locality will be set to this value in certificates issued by this role."#,
                    ),
            )
            .field(
                "province",
                Field::builder()
                    .field_type(FieldType::Str)
                    .description(
                        r#"
        If set, Province will be set to this value in certificates issued by this role."#,
                    ),
            )
            .field(
                "street_address",
                Field::builder()
                    .field_type(FieldType::Str)
                    .description(
                        r#"
        If set, Street Address will be set to this value."#,
                    ),
            )
            .field(
                "postal_code",
                Field::builder()
                    .field_type(FieldType::Str)
                    .description(
                        r#"
        If set, Postal Code will be set to this value."#,
                    ),
            )
            .field(
                "use_csr_common_name",
                Field::builder()
                    .field_type(FieldType::Bool)
                    .default_value(true)
                    .description(
                        r#"
If set, when used with a signing profile, the common name in the CSR will be used. This
does *not* include any requested Subject Alternative Names; use use_csr_sans for that. defaults to true."#,
                    ),
            )
            .field(
                "use_csr_sans",
                Field::builder()
                    .field_type(FieldType::Bool)
                    .default_value(true)
                    .description(
                        r#"
If set, when used with a signing profile, the SANs in the CSR will be used. This does *not*
include the Common Name (cn); use use_csr_common_name for that. defaults to true."#,
                    ),
            )
            .field(
                "generate_lease",
                Field::builder()
                    .field_type(FieldType::Bool)
                    .default_value(false)
                    .description(
                        r#"
If set, certificates issued/signed against this role will have RustyVault leases
attached to them. Defaults to "false". Certificates can be added to the CRL by
"vault revoke <lease_id>" when certificates are associated with leases.  It can
also be done using the "pki/revoke" endpoint. However, when lease generation is
disabled, invoking "pki/revoke" would be the only way to add the certificates
to the CRL.  When large number of certificates are generated with long
lifetimes, it is recommended that lease generation be disabled, as large amount of
leases adversely affect the startup time of RustyVault."#,
                    ),
            )
            .field(
                "no_store",
                Field::builder()
                    .field_type(FieldType::Bool)
                    .default_value(false)
                    .description(
                        r#"
If set, certificates issued/signed against this role will not be stored in the
storage backend. This can improve performance when issuing large numbers of
certificates. However, certificates issued in this way cannot be enumerated
or revoked, so this option is recommended only for certificates that are
non-sensitive, or extremely short-lived. This option implies a value of "false"
for "generate_lease"."#,
                    ),
            )
            .operation(Operation::Read, {
                let handler = backend_read.clone();
                move |backend, req| {
                    let handler = handler.clone();
                    Box::pin(async move { handler.read_path_role(backend, req).await })
                }
            })
            .operation(Operation::Write, {
                let handler = backend_write.clone();
                move |backend, req| {
                    let handler = handler.clone();
                    Box::pin(async move { handler.create_path_role(backend, req).await })
                }
            })
            .operation(Operation::Delete, {
                let handler = backend_delete.clone();
                move |backend, req| {
                    let handler = handler.clone();
                    Box::pin(async move { handler.delete_path_role(backend, req).await })
                }
            })
            .help("This path lets you manage the roles that can be created with this backend.")
            .build()
    }
}

impl PkiBackendInner {
    pub async fn get_role(
        &self,
        req: &mut Request,
        name: &str,
    ) -> Result<Option<RoleEntry>, RvError> {
        let key = format!("role/{name}");
        let storage_entry = req.storage_get(&key).await?;
        if storage_entry.is_none() {
            return Ok(None);
        }

        let entry = storage_entry.unwrap();
        let role_entry: RoleEntry = serde_json::from_slice(entry.value.as_slice())?;
        Ok(Some(role_entry))
    }

    pub async fn read_path_role(
        &self,
        _backend: &dyn Backend,
        req: &mut Request,
    ) -> Result<Option<Response>, RvError> {
        let role_entry = self
            .get_role(
                req,
                req.get_data("name")?
                    .as_str()
                    .ok_or(RvError::ErrRequestFieldInvalid)?,
            )
            .await?;
        let data = serde_json::to_value(role_entry)?;
        Ok(Some(Response::data_response(Some(
            data.as_object().unwrap().clone(),
        ))))
    }

    pub async fn create_path_role(
        &self,
        _backend: &dyn Backend,
        req: &mut Request,
    ) -> Result<Option<Response>, RvError> {
        let name_value = req.get_data("name")?;
        let name = name_value.as_str().ok_or(RvError::ErrRequestFieldInvalid)?;
        let mut ttl = DEFAULT_MAX_TTL;
        if let Ok(ttl_value) = req.get_data("ttl") {
            let ttl_str = ttl_value.as_str().ok_or(RvError::ErrRequestFieldInvalid)?;
            if !ttl_str.is_empty() {
                ttl = parse_duration(ttl_str)?;
            }
        }
        let mut max_ttl = DEFAULT_MAX_TTL;
        if let Ok(max_ttl_value) = req.get_data("max_ttl") {
            let max_ttl_str = max_ttl_value
                .as_str()
                .ok_or(RvError::ErrRequestFieldInvalid)?;
            if !max_ttl_str.is_empty() {
                max_ttl = parse_duration(max_ttl_str)?;
            }
        }
        let key_type_value = req.get_data_or_default("key_type")?;
        let key_type = key_type_value
            .as_str()
            .ok_or(RvError::ErrRequestFieldInvalid)?;
        let mut key_bits = req
            .get_data_or_default("key_bits")?
            .as_u64()
            .ok_or(RvError::ErrRequestFieldInvalid)?;
        match key_type {
            "rsa" => {
                if key_bits == 0 {
                    key_bits = 2048;
                }

                if key_bits != 2048 && key_bits != 3072 && key_bits != 4096 {
                    return Err(RvError::ErrPkiKeyBitsInvalid);
                }
            }
            "ec" => {
                if key_bits == 0 {
                    key_bits = 256;
                }

                if key_bits != 224 && key_bits != 256 && key_bits != 384 && key_bits != 512 {
                    return Err(RvError::ErrPkiKeyBitsInvalid);
                }
            }
            #[cfg(feature = "crypto_adaptor_tongsuo")]
            "sm2" => {
                if key_bits == 0 {
                    key_bits = 256;
                }

                if key_bits != 256 {
                    return Err(RvError::ErrPkiKeyBitsInvalid);
                }
            }
            _ => {
                return Err(RvError::ErrPkiKeyTypeInvalid);
            }
        }

        let signature_bits = req
            .get_data_or_default("signature_bits")?
            .as_u64()
            .ok_or(RvError::ErrRequestFieldInvalid)?;
        let allow_localhost = req
            .get_data_or_default("allow_localhost")?
            .as_bool()
            .ok_or(RvError::ErrRequestFieldInvalid)?;
        let allow_bare_domains = req
            .get_data_or_default("allow_bare_domains")?
            .as_bool()
            .ok_or(RvError::ErrRequestFieldInvalid)?;
        let allow_subdomains = req
            .get_data_or_default("allow_subdomains")?
            .as_bool()
            .ok_or(RvError::ErrRequestFieldInvalid)?;
        let allow_any_name = req
            .get_data_or_default("allow_any_name")?
            .as_bool()
            .ok_or(RvError::ErrRequestFieldInvalid)?;
        let allow_ip_sans = req
            .get_data_or_default("allow_ip_sans")?
            .as_bool()
            .ok_or(RvError::ErrRequestFieldInvalid)?;
        let server_flag = req
            .get_data_or_default("server_flag")?
            .as_bool()
            .ok_or(RvError::ErrRequestFieldInvalid)?;
        let client_flag = req
            .get_data_or_default("client_flag")?
            .as_bool()
            .ok_or(RvError::ErrRequestFieldInvalid)?;
        let use_csr_sans = req
            .get_data_or_default("use_csr_sans")?
            .as_bool()
            .ok_or(RvError::ErrRequestFieldInvalid)?;
        let use_csr_common_name = req
            .get_data_or_default("use_csr_common_name")?
            .as_bool()
            .ok_or(RvError::ErrRequestFieldInvalid)?;
        let key_usage = req
            .get_data_or_default("key_usage")?
            .as_comma_string_slice()
            .ok_or(RvError::ErrRequestFieldInvalid)?;
        let ext_key_usage = req
            .get_data_or_default("ext_key_usage")?
            .as_comma_string_slice()
            .ok_or(RvError::ErrRequestFieldInvalid)?;
        let country = req
            .get_data_or_default("country")?
            .as_str()
            .ok_or(RvError::ErrRequestFieldInvalid)?
            .to_string();
        let province = req
            .get_data_or_default("province")?
            .as_str()
            .ok_or(RvError::ErrRequestFieldInvalid)?
            .to_string();
        let locality = req
            .get_data_or_default("locality")?
            .as_str()
            .ok_or(RvError::ErrRequestFieldInvalid)?
            .to_string();
        let organization = req
            .get_data_or_default("organization")?
            .as_str()
            .ok_or(RvError::ErrRequestFieldInvalid)?
            .to_string();
        let ou = req
            .get_data_or_default("ou")?
            .as_str()
            .ok_or(RvError::ErrRequestFieldInvalid)?
            .to_string();
        let street_address = req
            .get_data_or_default("street_address")?
            .as_str()
            .ok_or(RvError::ErrRequestFieldInvalid)?
            .to_string();
        let postal_code = req
            .get_data_or_default("postal_code")?
            .as_str()
            .ok_or(RvError::ErrRequestFieldInvalid)?
            .to_string();
        let no_store = req
            .get_data_or_default("no_store")?
            .as_bool()
            .ok_or(RvError::ErrRequestFieldInvalid)?;
        let generate_lease = req
            .get_data_or_default("generate_lease")?
            .as_bool()
            .ok_or(RvError::ErrRequestFieldInvalid)?;
        let not_after = req
            .get_data_or_default("not_after")?
            .as_str()
            .ok_or(RvError::ErrRequestFieldInvalid)?
            .to_string();
        let not_before_duration_u64 = req
            .get_data_or_default("not_before_duration")?
            .as_u64()
            .ok_or(RvError::ErrRequestFieldInvalid)?;
        let not_before_duration = Duration::from_secs(not_before_duration_u64);

        let role_entry = RoleEntry {
            ttl,
            max_ttl,
            key_type: key_type.to_string(),
            key_bits: key_bits as u32,
            signature_bits: signature_bits as u32,
            allow_localhost,
            allow_bare_domains,
            allow_subdomains,
            allow_any_name,
            allow_ip_sans,
            server_flag,
            client_flag,
            use_csr_sans,
            use_csr_common_name,
            key_usage,
            ext_key_usage,
            country,
            province,
            locality,
            organization,
            ou,
            no_store,
            generate_lease,
            not_after,
            not_before_duration,
            street_address,
            postal_code,
            ..Default::default()
        };

        let entry = StorageEntry::new(format!("role/{name}").as_str(), &role_entry)?;

        req.storage_put(&entry).await?;

        Ok(None)
    }

    pub async fn delete_path_role(
        &self,
        _backend: &dyn Backend,
        req: &mut Request,
    ) -> Result<Option<Response>, RvError> {
        let name_value = req.get_data("name")?;
        let name = name_value.as_str().ok_or(RvError::ErrRequestFieldInvalid)?;
        if name.is_empty() {
            return Err(RvError::ErrRequestNoDataField);
        }

        req.storage_delete(format!("role/{name}").as_str()).await?;
        Ok(None)
    }
}
