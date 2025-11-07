use std::time::{Duration, SystemTime};

use humantime::{parse_duration, parse_rfc3339};
use openssl::x509::X509NameBuilder;

use super::path_roles::RoleEntry;
use crate::{errors::RvError, logical::Request, utils::cert::Certificate};

pub const DEFAULT_MAX_TTL: Duration = Duration::from_secs(365 * 24 * 60 * 60_u64);

pub fn get_role_params(req: &mut Request) -> Result<RoleEntry, RvError> {
    let mut ttl = DEFAULT_MAX_TTL;
    if let Ok(ttl_value) = req.get_data("ttl") {
        let ttl_str = ttl_value.as_str().ok_or(RvError::ErrRequestFieldInvalid)?;
        if !ttl_str.is_empty() {
            ttl = parse_duration(ttl_str)?;
        }
    }
    let not_before_duration_u64 = req
        .get_data_or_default("not_before_duration")?
        .as_u64()
        .ok_or(RvError::ErrRequestFieldInvalid)?;
    let not_before_duration = Duration::from_secs(not_before_duration_u64);
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
        _ => return Err(RvError::ErrPkiKeyTypeInvalid),
    }

    let signature_bits = req
        .get_data_or_default("signature_bits")?
        .as_u64()
        .ok_or(RvError::ErrRequestFieldInvalid)?;
    let use_pss = req
        .get_data_or_default("use_pss")?
        .as_bool()
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
    let not_after = req
        .get_data_or_default("not_after")?
        .as_str()
        .ok_or(RvError::ErrRequestFieldInvalid)?
        .to_string();

    let role_entry = RoleEntry {
        ttl,
        not_before_duration,
        use_pss,
        key_type: key_type.to_string(),
        key_bits: key_bits as u32,
        signature_bits: signature_bits as u32,
        country,
        province,
        locality,
        organization,
        ou,
        street_address,
        postal_code,
        not_after,
        ..Default::default()
    };

    Ok(role_entry)
}

pub fn generate_certificate(
    role_entry: &RoleEntry,
    req: &mut Request,
) -> Result<Certificate, RvError> {
    let mut common_names = Vec::new();

    let common_name_value = req.get_data_or_default("common_name")?;
    let common_name = common_name_value
        .as_str()
        .ok_or(RvError::ErrRequestFieldInvalid)?;
    if !common_name.is_empty() {
        common_names.push(common_name.to_string());
    }

    if let Ok(alt_names_value) = req.get_data("alt_names") {
        let alt_names = alt_names_value
            .as_str()
            .ok_or(RvError::ErrRequestFieldInvalid)?;
        if !alt_names.is_empty() {
            for v in alt_names.split(',') {
                common_names.push(v.to_string());
            }
        }
    }

    let mut ip_sans = Vec::new();
    if let Ok(ip_sans_value) = req.get_data("ip_sans") {
        let ip_sans_str = ip_sans_value
            .as_str()
            .ok_or(RvError::ErrRequestFieldInvalid)?;
        if !ip_sans_str.is_empty() {
            for v in ip_sans_str.split(',') {
                ip_sans.push(v.to_string());
            }
        }
    }

    let not_before = SystemTime::now() - Duration::from_secs(10);
    let not_after: SystemTime;
    if role_entry.not_after.len() > 18 {
        let parsed_time = parse_rfc3339(&role_entry.not_after)?;
        not_after = parsed_time;
    } else if role_entry.ttl != Duration::from_secs(0) {
        not_after = not_before + role_entry.ttl;
    } else {
        not_after = not_before + role_entry.max_ttl;
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
        subject_name.append_entry_by_text("CN", common_name)?;
    }
    let subject = subject_name.build();

    let cert = Certificate {
        not_before,
        not_after,
        subject,
        dns_sans: common_names,
        ip_sans,
        key_type: role_entry.key_type.clone(),
        key_bits: role_entry.key_bits,
        ..Default::default()
    };

    Ok(cert)
}
