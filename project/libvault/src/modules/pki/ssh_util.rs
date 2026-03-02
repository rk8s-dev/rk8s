use base64::Engine;
use openssl::{
    bn::BigNumRef,
    ec::{EcGroup, EcKey},
    hash::MessageDigest,
    nid::Nid,
    pkey::{PKey, Private},
    rsa::Rsa,
    sign::Signer,
};
use rand::Rng;

use crate::errors::RvError;

/// SSH certificate type constants per PROTOCOL.certkeys.
pub const SSH_CERT_TYPE_USER: u32 = 1;
pub const SSH_CERT_TYPE_HOST: u32 = 2;

/// Encode a byte slice as an SSH string (u32 length prefix + data).
pub fn encode_string(data: &[u8]) -> Result<Vec<u8>, RvError> {
    let len: u32 = data.len().try_into().map_err(|_| RvError::ErrPkiInternal)?;
    let mut buf = Vec::with_capacity(4 + data.len());
    buf.extend_from_slice(&len.to_be_bytes());
    buf.extend_from_slice(data);
    Ok(buf)
}

pub fn encode_u32(v: u32) -> Vec<u8> {
    v.to_be_bytes().to_vec()
}

pub fn encode_u64(v: u64) -> Vec<u8> {
    v.to_be_bytes().to_vec()
}

/// Encode a BigNum as an SSH mpint.
pub fn encode_mpint(bn: &BigNumRef) -> Result<Vec<u8>, RvError> {
    let bytes = bn.to_vec();
    if bytes.is_empty() {
        return encode_string(&[]);
    }
    // If the high bit is set, prepend a zero byte.
    if bytes[0] & 0x80 != 0 {
        let mut padded = Vec::with_capacity(1 + bytes.len());
        padded.push(0);
        padded.extend_from_slice(&bytes);
        encode_string(&padded)
    } else {
        encode_string(&bytes)
    }
}

/// Encode an RSA public key in OpenSSH wire format.
pub fn encode_rsa_pubkey(pkey: &PKey<Private>) -> Result<Vec<u8>, RvError> {
    let rsa = pkey.rsa()?;
    let mut buf = Vec::new();
    buf.extend_from_slice(&encode_string(b"ssh-rsa")?);
    buf.extend_from_slice(&encode_mpint(rsa.e())?);
    buf.extend_from_slice(&encode_mpint(rsa.n())?);
    Ok(buf)
}

/// Encode an EC public key in OpenSSH wire format.
pub fn encode_ec_pubkey(pkey: &PKey<Private>, nid: Nid) -> Result<Vec<u8>, RvError> {
    let ec = pkey.ec_key()?;
    let group = ec.group();
    let mut ctx = openssl::bn::BigNumContext::new()?;
    let point_bytes = ec.public_key().to_bytes(
        group,
        openssl::ec::PointConversionForm::UNCOMPRESSED,
        &mut ctx,
    )?;

    let (key_type, curve_id) = match nid {
        Nid::X9_62_PRIME256V1 => ("ecdsa-sha2-nistp256", "nistp256"),
        Nid::SECP384R1 => ("ecdsa-sha2-nistp384", "nistp384"),
        Nid::SECP521R1 => ("ecdsa-sha2-nistp521", "nistp521"),
        _ => return Err(RvError::ErrPkiKeyTypeInvalid),
    };

    let mut buf = Vec::new();
    buf.extend_from_slice(&encode_string(key_type.as_bytes())?);
    buf.extend_from_slice(&encode_string(curve_id.as_bytes())?);
    buf.extend_from_slice(&encode_string(&point_bytes)?);
    Ok(buf)
}

/// Encode an Ed25519 public key in OpenSSH wire format.
pub fn encode_ed25519_pubkey(pkey: &PKey<Private>) -> Result<Vec<u8>, RvError> {
    let raw = pkey.raw_public_key()?;
    let mut buf = Vec::new();
    buf.extend_from_slice(&encode_string(b"ssh-ed25519")?);
    buf.extend_from_slice(&encode_string(&raw)?);
    Ok(buf)
}

/// Get the SSH key type string for a given key.
pub fn ssh_key_type_str(pkey: &PKey<Private>, nid: Option<Nid>) -> Result<&'static str, RvError> {
    match pkey.id() {
        openssl::pkey::Id::RSA => Ok("ssh-rsa"),
        openssl::pkey::Id::EC => match nid {
            Some(Nid::X9_62_PRIME256V1) => Ok("ecdsa-sha2-nistp256"),
            Some(Nid::SECP384R1) => Ok("ecdsa-sha2-nistp384"),
            Some(Nid::SECP521R1) => Ok("ecdsa-sha2-nistp521"),
            _ => Err(RvError::ErrPkiKeyTypeInvalid),
        },
        openssl::pkey::Id::ED25519 => Ok("ssh-ed25519"),
        _ => Err(RvError::ErrPkiKeyTypeInvalid),
    }
}

/// Get the SSH certificate type string for a given key.
pub fn ssh_cert_type_str(pkey: &PKey<Private>, nid: Option<Nid>) -> Result<&'static str, RvError> {
    match pkey.id() {
        openssl::pkey::Id::RSA => Ok("ssh-rsa-cert-v01@openssh.com"),
        openssl::pkey::Id::EC => match nid {
            Some(Nid::X9_62_PRIME256V1) => Ok("ecdsa-sha2-nistp256-cert-v01@openssh.com"),
            Some(Nid::SECP384R1) => Ok("ecdsa-sha2-nistp384-cert-v01@openssh.com"),
            Some(Nid::SECP521R1) => Ok("ecdsa-sha2-nistp521-cert-v01@openssh.com"),
            _ => Err(RvError::ErrPkiKeyTypeInvalid),
        },
        openssl::pkey::Id::ED25519 => Ok("ssh-ed25519-cert-v01@openssh.com"),
        _ => Err(RvError::ErrPkiKeyTypeInvalid),
    }
}

/// Encode the public key portion of a PKey for embedding in a certificate.
pub fn encode_pubkey_for_cert(pkey: &PKey<Private>, nid: Option<Nid>) -> Result<Vec<u8>, RvError> {
    match pkey.id() {
        openssl::pkey::Id::RSA => {
            let rsa = pkey.rsa()?;
            let mut buf = Vec::new();
            buf.extend_from_slice(&encode_mpint(rsa.e())?);
            buf.extend_from_slice(&encode_mpint(rsa.n())?);
            Ok(buf)
        }
        openssl::pkey::Id::EC => {
            let ec = pkey.ec_key()?;
            let group = ec.group();
            let mut ctx = openssl::bn::BigNumContext::new()?;
            let point_bytes = ec.public_key().to_bytes(
                group,
                openssl::ec::PointConversionForm::UNCOMPRESSED,
                &mut ctx,
            )?;
            let curve_id = match nid {
                Some(Nid::X9_62_PRIME256V1) => "nistp256",
                Some(Nid::SECP384R1) => "nistp384",
                Some(Nid::SECP521R1) => "nistp521",
                _ => return Err(RvError::ErrPkiKeyTypeInvalid),
            };
            let mut buf = Vec::new();
            buf.extend_from_slice(&encode_string(curve_id.as_bytes())?);
            buf.extend_from_slice(&encode_string(&point_bytes)?);
            Ok(buf)
        }
        openssl::pkey::Id::ED25519 => {
            let raw = pkey.raw_public_key()?;
            Ok(encode_string(&raw)?)
        }
        _ => Err(RvError::ErrPkiKeyTypeInvalid),
    }
}

/// Build an SSH certificate binary blob and sign it with the CA key.
///
/// Returns the full certificate in OpenSSH wire format (base64-encodable).
pub fn build_ssh_certificate(
    cert_type_str: &str,
    user_pubkey_data: &[u8],
    serial: u64,
    key_id: &str,
    valid_principals: &[String],
    valid_after: u64,
    valid_before: u64,
    cert_type: u32,
    extensions: &std::collections::HashMap<String, String>,
    ca_key: &PKey<Private>,
    ca_nid: Option<Nid>,
) -> Result<Vec<u8>, RvError> {
    let nonce: [u8; 32] = rand::rng().random();

    let mut cert = Vec::new();
    // cert type string
    cert.extend_from_slice(&encode_string(cert_type_str.as_bytes())?);
    // nonce
    cert.extend_from_slice(&encode_string(&nonce)?);
    // public key data (without key type prefix)
    cert.extend_from_slice(user_pubkey_data);
    // serial
    cert.extend_from_slice(&encode_u64(serial));
    // type
    cert.extend_from_slice(&encode_u32(cert_type));
    // key id
    cert.extend_from_slice(&encode_string(key_id.as_bytes())?);
    // valid principals (packed as a single string containing sub-strings)
    let mut principals_buf = Vec::new();
    for p in valid_principals {
        principals_buf.extend_from_slice(&encode_string(p.as_bytes())?);
    }
    cert.extend_from_slice(&encode_string(&principals_buf)?);
    // valid after / before
    cert.extend_from_slice(&encode_u64(valid_after));
    cert.extend_from_slice(&encode_u64(valid_before));
    // critical options (empty)
    cert.extend_from_slice(&encode_string(&[])?);
    // extensions
    let mut ext_buf = Vec::new();
    let mut sorted_keys: Vec<&String> = extensions.keys().collect();
    sorted_keys.sort();
    for k in sorted_keys {
        let v = &extensions[k];
        ext_buf.extend_from_slice(&encode_string(k.as_bytes())?);
        ext_buf.extend_from_slice(&encode_string(v.as_bytes())?);
    }
    cert.extend_from_slice(&encode_string(&ext_buf)?);
    // reserved
    cert.extend_from_slice(&encode_string(&[])?);
    // signature key (CA public key in wire format)
    let ca_pubkey_wire = encode_ca_pubkey(ca_key, ca_nid)?;
    cert.extend_from_slice(&encode_string(&ca_pubkey_wire)?);

    // Sign the certificate
    let signature = sign_ssh_data(ca_key, &cert)?;
    cert.extend_from_slice(&encode_string(&signature)?);

    Ok(cert)
}

/// Encode the CA public key in SSH wire format (with key type prefix).
fn encode_ca_pubkey(pkey: &PKey<Private>, nid: Option<Nid>) -> Result<Vec<u8>, RvError> {
    match pkey.id() {
        openssl::pkey::Id::RSA => encode_rsa_pubkey(pkey),
        openssl::pkey::Id::EC => encode_ec_pubkey(pkey, nid.unwrap_or(Nid::X9_62_PRIME256V1)),
        openssl::pkey::Id::ED25519 => encode_ed25519_pubkey(pkey),
        _ => Err(RvError::ErrPkiKeyTypeInvalid),
    }
}

/// Sign data using the SSH signing algorithm for the given key type.
fn sign_ssh_data(pkey: &PKey<Private>, data: &[u8]) -> Result<Vec<u8>, RvError> {
    match pkey.id() {
        openssl::pkey::Id::RSA => {
            let mut signer = Signer::new(MessageDigest::sha512(), pkey)?;
            signer.set_rsa_padding(openssl::rsa::Padding::PKCS1)?;
            signer.update(data)?;
            let sig = signer.sign_to_vec()?;
            let mut buf = Vec::new();
            buf.extend_from_slice(&encode_string(b"rsa-sha2-512")?);
            buf.extend_from_slice(&encode_string(&sig)?);
            Ok(buf)
        }
        openssl::pkey::Id::EC => {
            let ec = pkey.ec_key()?;
            let nid = ec.group().curve_name().unwrap_or(Nid::X9_62_PRIME256V1);
            let md = match nid {
                Nid::X9_62_PRIME256V1 => MessageDigest::sha256(),
                Nid::SECP384R1 => MessageDigest::sha384(),
                Nid::SECP521R1 => MessageDigest::sha512(),
                _ => return Err(RvError::ErrPkiKeyTypeInvalid),
            };
            let sig_type = match nid {
                Nid::X9_62_PRIME256V1 => "ecdsa-sha2-nistp256",
                Nid::SECP384R1 => "ecdsa-sha2-nistp384",
                Nid::SECP521R1 => "ecdsa-sha2-nistp521",
                _ => return Err(RvError::ErrPkiKeyTypeInvalid),
            };
            let mut signer = Signer::new(md, pkey)?;
            signer.update(data)?;
            let sig = signer.sign_to_vec()?;
            let mut buf = Vec::new();
            buf.extend_from_slice(&encode_string(sig_type.as_bytes())?);
            buf.extend_from_slice(&encode_string(&sig)?);
            Ok(buf)
        }
        openssl::pkey::Id::ED25519 => {
            let mut signer = Signer::new_without_digest(pkey)?;
            let sig = signer.sign_oneshot_to_vec(data)?;
            let mut buf = Vec::new();
            buf.extend_from_slice(&encode_string(b"ssh-ed25519")?);
            buf.extend_from_slice(&encode_string(&sig)?);
            Ok(buf)
        }
        _ => Err(RvError::ErrPkiKeyTypeInvalid),
    }
}

/// Generate an SSH key pair. Returns (PKey, Option<Nid>).
pub fn generate_ssh_keypair(
    key_type: &str,
    key_bits: u32,
) -> Result<(PKey<Private>, Option<Nid>), RvError> {
    match key_type {
        "rsa" => {
            let bits = if key_bits == 0 { 2048 } else { key_bits };
            let rsa = Rsa::generate(bits)?;
            let pkey = PKey::from_rsa(rsa)?;
            Ok((pkey, None))
        }
        "ec" => {
            let (nid, _bits) = match key_bits {
                0 | 256 => (Nid::X9_62_PRIME256V1, 256),
                384 => (Nid::SECP384R1, 384),
                521 => (Nid::SECP521R1, 521),
                _ => return Err(RvError::ErrPkiKeyBitsInvalid),
            };
            let group = EcGroup::from_curve_name(nid)?;
            let ec = EcKey::generate(&group)?;
            let pkey = PKey::from_ec_key(ec)?;
            Ok((pkey, Some(nid)))
        }
        "ed25519" => {
            let pkey = PKey::generate_ed25519()?;
            Ok((pkey, None))
        }
        _ => Err(RvError::ErrPkiKeyTypeInvalid),
    }
}

/// Format a PKey's public key as an OpenSSH authorized_keys line.
pub fn format_openssh_pubkey(pkey: &PKey<Private>, nid: Option<Nid>) -> Result<String, RvError> {
    let wire = encode_ca_pubkey(pkey, nid)?;
    let key_type = ssh_key_type_str(pkey, nid)?;
    Ok(format!(
        "{} {}",
        key_type,
        base64::engine::general_purpose::STANDARD.encode(&wire)
    ))
}

/// Format a signed certificate as an OpenSSH certificate line.
pub fn format_openssh_cert(cert_type_str: &str, cert_bytes: &[u8]) -> String {
    format!(
        "{} {}",
        cert_type_str,
        base64::engine::general_purpose::STANDARD.encode(cert_bytes)
    )
}
