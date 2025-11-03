use std::{collections::HashMap, sync::Arc};

use crate::logical::{Field, FieldType, FieldsBuilder};

pub fn ca_common_fields() -> HashMap<String, Arc<Field>> {
    FieldsBuilder::new()
        .field(
            "alt_names",
            Field::builder()
                .field_type(FieldType::Str)
                .default_value("")
                .description(
                    r#"The requested Subject Alternative Names, if any,
    in a comma-delimited list. May contain both DNS names and email addresses."#,
                ),
        )
        .field(
            "common_name",
            Field::builder()
                .field_type(FieldType::Str)
                .required(true)
                .description(
                    r#"The requested common name; if you want more than
one, specify the alternative names in the alt_names map. If not specified when
signing, the common name will be taken from the CSR; other names must still be
specified in alt_names or ip_sans.
"#,
                ),
        )
        .field(
            "ttl",
            Field::builder()
                .field_type(FieldType::Str)
                .description(
                    r#"The requested Time To Live for the certificate;
sets the expiration date. If not specified the role default, backend default,
or system default TTL is used, in that order. Cannot be larger than the mount
max TTL. Note: this only has an effect when generating a CA cert or signing a
CA cert, not when generating a CSR for an intermediate CA.
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
                .default_value("")
                .description("If set, OU (OrganizationalUnit) will be set to this value."),
        )
        .field(
            "organization",
            Field::builder()
                .field_type(FieldType::Str)
                .default_value("")
                .description("If set, O (Organization) will be set to this value."),
        )
        .field(
            "country",
            Field::builder()
                .field_type(FieldType::Str)
                .default_value("")
                .description("If set, Country will be set to this value."),
        )
        .field(
            "locality",
            Field::builder()
                .field_type(FieldType::Str)
                .default_value("")
                .description(
                    "If set, Locality will be set to this value in certificates issued by this role.",
                ),
        )
        .field(
            "province",
            Field::builder()
                .field_type(FieldType::Str)
                .default_value("")
                .description("If set, Province will be set to this value."),
        )
        .field(
            "street_address",
            Field::builder()
                .field_type(FieldType::Str)
                .default_value("")
                .description("If set, Street Address will be set to this value."),
        )
        .field(
            "postal_code",
            Field::builder()
                .field_type(FieldType::Str)
                .default_value("")
                .description("If set, Postal Code will be set to this value."),
        )
        .field(
            "serial_number",
            Field::builder()
                .field_type(FieldType::Str)
                .default_value("")
                .description(
                    r#"The Subject's requested serial number, if any.
See RFC 4519 Section 2.31 'serialNumber' for a description of this field.
If you want more than one, specify alternative names in the alt_names
map using OID 2.5.4.5. This has no impact on the final certificate's
Serial Number field.
"#,
                ),
        )
        .build()
}

pub fn ca_key_generation_fields() -> HashMap<String, Arc<Field>> {
    FieldsBuilder::new()
        .field(
            "exported",
            Field::builder()
                .field_type(FieldType::Str)
                .default_value("internal")
                .description(
                    r#"Must be "internal", "exported" or "kms". If set to
"exported", the generated private key will be returned. This is your *only*
chance to retrieve the private key!"#,
                ),
        )
        .field(
            "key_type",
            Field::builder()
                .field_type(FieldType::Str)
                .default_value("rsa")
                .description(
                    r#"The type of key to use; defaults to RSA. "rsa" "ec",
    "ed25519" and "any" are the only valid values."#,
                ),
        )
        .field(
            "key_bits",
            Field::builder()
                .field_type(FieldType::Int)
                .default_value(0)
                .description(
                    r#"The number of bits to use. Allowed values are 0 (universal default);
with rsa key_type: 2048 (default), 3072, or 4096; with ec key_type: 224, 256 (default),
384, or 521; ignored with ed25519."#,
                ),
        )
        .field(
            "signature_bits",
            Field::builder()
                .field_type(FieldType::Int)
                .default_value(0)
                .description(
                    r#"The number of bits to use in the signature algorithm;
accepts 256 for SHA-2-256, 384 for SHA-2-384, and 512 for SHA-2-512. defaults to 0
to automatically detect based on key length (SHA-2-256 for RSA keys, and matching
the curve size for NIST P-Curves)."#,
                ),
        )
        .field(
            "use_pss",
            Field::builder()
                .field_type(FieldType::Bool)
                .default_value(false)
                .description(
                    r#"Whether or not to use PSS signatures when using a
    RSA key-type issuer. Defaults to false."#,
                ),
        )
        .build()
}

pub fn ca_issue_fields() -> HashMap<String, Arc<Field>> {
    FieldsBuilder::new()
        .field(
            "permitted_dns_domains",
            Field::builder()
                .field_type(FieldType::Str)
                .default_value("")
                .description(
                    r#"Domains for which this certificate is allowed to
sign or issue child certificates. If set, all DNS names (subject and alt) on
child certs must be exact matches or subsets of the given domains
(see https://tools.ietf.org/html/rfc5280#section-4.2.1.10)."#,
                ),
        )
        .field(
            "max_path_length",
            Field::builder()
                .field_type(FieldType::Int)
                .default_value(-1)
                .description("The maximum allowable path length"),
        )
        .build()
}
