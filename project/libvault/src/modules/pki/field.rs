use std::{collections::HashMap, sync::Arc};

use crate::{
    logical::{Field, FieldType},
    new_fields, new_fields_internal,
};

pub fn ca_common_fields() -> HashMap<String, Arc<Field>> {
    let fields = new_fields!({
        "alt_names": {
            field_type: FieldType::Str,
            default: "",
            description: r#"The requested Subject Alternative Names, if any,
    in a comma-delimited list. May contain both DNS names and email addresses."#
        },
        "common_name": {
            field_type: FieldType::Str,
            required: true,
            description: r#"The requested common name; if you want more than
one, specify the alternative names in the alt_names map. If not specified when
signing, the common name will be taken from the CSR; other names must still be
specified in alt_names or ip_sans.
"#
        },
        "ttl": {
            field_type: FieldType::Str,
            description: r#"The requested Time To Live for the certificate;
sets the expiration date. If not specified the role default, backend default,
or system default TTL is used, in that order. Cannot be larger than the mount
max TTL. Note: this only has an effect when generating a CA cert or signing a
CA cert, not when generating a CSR for an intermediate CA.
"#
        },
        "not_before_duration": {
            field_type: FieldType::Int,
            default: 30,
            description: r#"
    The duration before now which the certificate needs to be backdated by."#
        },
        "not_after": {
            field_type: FieldType::Str,
            default: "",
            description: r#"
Set the not after field of the certificate with specified date value.
The value format should be given in UTC format YYYY-MM-ddTHH:MM:SSZ."#
        },
        "ou": {
            field_type: FieldType::Str,
            default: "",
            description: r#"If set, OU (OrganizationalUnit) will be set to this value."#
        },
        "organization": {
            field_type: FieldType::Str,
            default: "",
            description: r#"If set, O (Organization) will be set to this value."#
        },
        "country": {
            field_type: FieldType::Str,
            default: "",
            description: r#"If set, Country will be set to this value."#
        },
        "locality": {
            field_type: FieldType::Str,
            default: "",
            description: r#"If set, Locality will be set to this value in certificates issued by this role."#
        },
        "province": {
            field_type: FieldType::Str,
            default: "",
            description: r#"If set, Province will be set to this value."#
        },
        "street_address": {
            field_type: FieldType::Str,
            default: "",
            description: r#"If set, Street Address will be set to this value."#
        },
        "postal_code": {
            field_type: FieldType::Str,
            default: "",
            description: r#"If set, Postal Code will be set to this value."#
        },
        "serial_number": {
            field_type: FieldType::Str,
            default: "",
            description: r#"The Subject's requested serial number, if any.
See RFC 4519 Section 2.31 'serialNumber' for a description of this field.
If you want more than one, specify alternative names in the alt_names
map using OID 2.5.4.5. This has no impact on the final certificate's
Serial Number field.
"#
        }
    });

    fields
}

pub fn ca_key_generation_fields() -> HashMap<String, Arc<Field>> {
    let fields = new_fields!({
        "exported": {
            field_type: FieldType::Str,
            default: "internal",
            description: r#"Must be "internal", "exported" or "kms". If set to
"exported", the generated private key will be returned. This is your *only*
chance to retrieve the private key!"#
        },
        "key_type": {
            field_type: FieldType::Str,
            default: "rsa",
            description: r#"The type of key to use; defaults to RSA. "rsa" "ec",
    "ed25519" and "any" are the only valid values."#
        },
        "key_bits": {
            field_type: FieldType::Int,
            default: 0,
            description: r#"The number of bits to use. Allowed values are 0 (universal default);
with rsa key_type: 2048 (default), 3072, or 4096; with ec key_type: 224, 256 (default),
384, or 521; ignored with ed25519."#
        },
        "signature_bits": {
            field_type: FieldType::Int,
            default: 0,
            description: r#"The number of bits to use in the signature algorithm;
accepts 256 for SHA-2-256, 384 for SHA-2-384, and 512 for SHA-2-512. defaults to 0
to automatically detect based on key length (SHA-2-256 for RSA keys, and matching
the curve size for NIST P-Curves)."#
        },
        "use_pss": {
            field_type: FieldType::Bool,
            default: false,
            description: r#"Whether or not to use PSS signatures when using a
    RSA key-type issuer. Defaults to false."#
        }
    });

    fields
}

pub fn ca_issue_fields() -> HashMap<String, Arc<Field>> {
    let fields = new_fields!({
        "permitted_dns_domains": {
            field_type: FieldType::Str,
            default: "rsa",
            description: r#"Domains for which this certificate is allowed to
sign or issue child certificates. If set, all DNS names (subject and alt) on
child certs must be exact matches or subsets of the given domains
(see https://tools.ietf.org/html/rfc5280#section-4.2.1.10)."#
        },
        "max_path_length": {
            field_type: FieldType::Int,
            default: -1,
            description: r#"The maximum allowable path length"#
        }
    });

    fields
}
