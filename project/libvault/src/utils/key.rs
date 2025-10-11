use crate::{errors::RvError, utils::generate_uuid};
use better_default::Default;
use openssl::{
    ec::{EcGroup, EcKey},
    hash::MessageDigest,
    nid::Nid,
    pkey::PKey,
    rand::rand_bytes,
    rsa::{Padding, Rsa},
    sign::{Signer, Verifier},
    symm::{Cipher, decrypt, decrypt_aead, encrypt, encrypt_aead},
};
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct KeyBundle {
    #[default(generate_uuid())]
    pub id: String,
    pub name: String,
    pub key_type: String,
    pub key: Vec<u8>,
    //for aes-gcm | aes-cbc
    pub iv: Vec<u8>,
    pub bits: u32,
}

#[derive(Debug, Clone)]
pub enum EncryptExtraData<'a> {
    Aad(&'a [u8]),
    Flag(bool),
}

fn key_bits_default(key_type: &str) -> u32 {
    match key_type {
        "rsa" => 2048,
        "ec" | "sm2" => 256,
        "aes-gcm" | "aes-cbc" | "aes-ecb" | "sm4-gcm" | "sm4-ccm" => 256,
        _ => 0,
    }
}

// TODO: this function needs to be refactored to use crypto adaptors.
fn cipher_from_key_type_and_bits(key_type: &str, bits: u32) -> Result<Cipher, RvError> {
    match (key_type, bits) {
        ("aes-gcm", 128) => Ok(Cipher::aes_128_gcm()),
        ("aes-gcm", 192) => Ok(Cipher::aes_192_gcm()),
        ("aes-gcm", 256) => Ok(Cipher::aes_256_gcm()),
        ("aes-cbc", 128) => Ok(Cipher::aes_128_cbc()),
        ("aes-cbc", 192) => Ok(Cipher::aes_192_cbc()),
        ("aes-cbc", 256) => Ok(Cipher::aes_256_cbc()),
        ("aes-ecb", 128) => Ok(Cipher::aes_128_ecb()),
        ("aes-ecb", 192) => Ok(Cipher::aes_192_ecb()),
        ("aes-ecb", 256) => Ok(Cipher::aes_256_ecb()),
        #[cfg(feature = "crypto_adaptor_tongsuo")]
        ("sm4-gcm", 128) => Ok(Cipher::sm4_gcm()),
        #[cfg(feature = "crypto_adaptor_tongsuo")]
        ("sm4-ccm", 128) => Ok(Cipher::sm4_ccm()),
        _ => Err(RvError::ErrPkiKeyBitsInvalid),
    }
}

impl KeyBundle {
    pub fn new(name: &str, key_type: &str, key_bits: u32) -> Self {
        let bits = if key_bits == 0 {
            key_bits_default(key_type)
        } else {
            key_bits
        };
        Self {
            name: name.to_string(),
            key_type: key_type.to_string(),
            bits,
            ..KeyBundle::default()
        }
    }

    pub fn generate(&mut self) -> Result<(), RvError> {
        let key_bits = self.bits;
        let priv_key = match self.key_type.as_str() {
            "rsa" => match key_bits {
                2048 | 3072 | 4096 => {
                    let rsa_key = Rsa::generate(key_bits)?;
                    PKey::from_rsa(rsa_key)?.private_key_to_pem_pkcs8()?
                }
                _ => return Err(RvError::ErrPkiKeyBitsInvalid),
            },
            "ec" => {
                let curve_name = match key_bits {
                    224 => Nid::SECP224R1,
                    256 => Nid::X9_62_PRIME256V1,
                    384 => Nid::SECP384R1,
                    521 => Nid::SECP521R1,
                    _ => return Err(RvError::ErrPkiKeyBitsInvalid),
                };
                let ec_group = EcGroup::from_curve_name(curve_name)?;
                let ec_key = EcKey::generate(&ec_group)?;
                PKey::from_ec_key(ec_key)?.private_key_to_pem_pkcs8()?
            }
            #[cfg(feature = "crypto_adaptor_tongsuo")]
            "sm2" => {
                self.bits = 256;
                let ec_group = EcGroup::from_curve_name(Nid::SM2)?;
                let ec_key = EcKey::generate(&ec_group)?;
                PKey::from_ec_key(ec_key)?.private_key_to_pem_pkcs8()?
            }
            "aes-gcm" | "aes-cbc" | "aes-ecb" | "sm4-gcm" | "sm4-ccm" => {
                let _ = cipher_from_key_type_and_bits(self.key_type.as_str(), self.bits)?;

                #[cfg(not(feature = "crypto_adaptor_tongsuo"))]
                if self.key_type.starts_with("sm4-") {
                    return Err(RvError::ErrPkiKeyTypeInvalid);
                }

                match self.key_type.as_str() {
                    "aes-ecb" => (),
                    "sm4-ccm" => {
                        self.iv = vec![0u8; 12];
                        rand_bytes(&mut self.iv)?;
                    }
                    _ => {
                        self.iv = vec![0u8; 16];
                        rand_bytes(&mut self.iv)?;
                    }
                }

                let mut key = vec![0u8; key_bits as usize / 8];
                rand_bytes(&mut key)?;
                key
            }
            _ => return Err(RvError::ErrPkiKeyTypeInvalid),
        };

        self.key = priv_key;

        Ok(())
    }

    pub fn sign(&self, data: &[u8]) -> Result<Vec<u8>, RvError> {
        let digest = match self.key_type.as_str() {
            "rsa" | "ec" => MessageDigest::sha256(),
            #[cfg(feature = "crypto_adaptor_tongsuo")]
            "sm2" => MessageDigest::sm3(),
            _ => return Err(RvError::ErrPkiKeyOperationInvalid),
        };

        let pkey = PKey::private_key_from_pem(&self.key)?;

        let mut signer = Signer::new(digest, &pkey)?;
        if self.key_type == "rsa" {
            signer.set_rsa_padding(Padding::PKCS1)?;
        }

        signer.update(data)?;
        signer.sign_to_vec().map_err(From::from)
    }

    pub fn verify(&self, data: &[u8], signature: &[u8]) -> Result<bool, RvError> {
        let digest = match self.key_type.as_str() {
            "rsa" | "ec" => MessageDigest::sha256(),
            #[cfg(feature = "crypto_adaptor_tongsuo")]
            "sm2" => MessageDigest::sm3(),
            _ => return Err(RvError::ErrPkiKeyOperationInvalid),
        };

        let pkey = PKey::private_key_from_pem(&self.key)?;

        let mut verifier = Verifier::new(digest, &pkey)?;
        if self.key_type == "rsa" {
            verifier.set_rsa_padding(Padding::PKCS1)?;
        }

        verifier.update(data)?;
        Ok(verifier.verify(signature).unwrap_or(false))
    }

    pub fn encrypt(
        &self,
        data: &[u8],
        extra: Option<EncryptExtraData>,
    ) -> Result<Vec<u8>, RvError> {
        match self.key_type.as_str() {
            "aes-gcm" | "sm4-gcm" | "sm4-ccm" => {
                let cipher = cipher_from_key_type_and_bits(self.key_type.as_str(), self.bits)?;
                let aad = extra.map_or("".as_bytes(), |ex| match ex {
                    EncryptExtraData::Aad(aad) => aad,
                    _ => "".as_bytes(),
                });
                let mut tag = vec![0u8; 16];
                let mut ciphertext =
                    encrypt_aead(cipher, &self.key, Some(&self.iv), aad, data, &mut tag)?;
                ciphertext.extend_from_slice(&tag);
                Ok(ciphertext)
            }
            "aes-cbc" | "aes-ecb" => {
                let cipher = cipher_from_key_type_and_bits(self.key_type.as_str(), self.bits)?;
                let iv = if self.key_type == "aes-ecb" {
                    None
                } else {
                    Some(self.iv.as_slice())
                };
                Ok(encrypt(cipher, &self.key, iv, data)?)
            }
            "rsa" => {
                let rsa = Rsa::private_key_from_pem(&self.key)?;
                if data.len() > rsa.size() as usize {
                    return Err(RvError::ErrPkiInternal);
                }

                let mut buf: Vec<u8> = vec![0; rsa.size() as usize];

                let flag = extra.map_or(false, |ex| match ex {
                    EncryptExtraData::Flag(flag) => flag,
                    _ => false,
                });
                if !flag {
                    let _ = rsa.private_encrypt(data, &mut buf, Padding::PKCS1)?;
                } else {
                    let _ = rsa.public_encrypt(data, &mut buf, Padding::PKCS1)?;
                }

                Ok(buf)
            }
            _ => Err(RvError::ErrPkiKeyOperationInvalid),
        }
    }

    pub fn decrypt(
        &self,
        data: &[u8],
        extra: Option<EncryptExtraData>,
    ) -> Result<Vec<u8>, RvError> {
        match self.key_type.as_str() {
            "aes-gcm" | "sm4-gcm" | "sm4-ccm" => {
                let cipher = cipher_from_key_type_and_bits(self.key_type.as_str(), self.bits)?;
                let aad = extra.map_or("".as_bytes(), |ex| match ex {
                    EncryptExtraData::Aad(aad) => aad,
                    _ => "".as_bytes(),
                });
                let tag_len = 16;
                if data.len() < tag_len {
                    return Err(RvError::ErrPkiInternal);
                }
                let (ciphertext, tag) = data.split_at(data.len() - tag_len);
                Ok(decrypt_aead(
                    cipher,
                    &self.key,
                    Some(&self.iv),
                    aad,
                    ciphertext,
                    tag,
                )?)
            }
            "aes-cbc" | "aes-ecb" => {
                let cipher = cipher_from_key_type_and_bits(self.key_type.as_str(), self.bits)?;
                let iv = if self.key_type == "aes-ecb" {
                    None
                } else {
                    Some(self.iv.as_slice())
                };
                Ok(decrypt(cipher, &self.key, iv, data)?)
            }
            "rsa" => {
                let rsa = Rsa::private_key_from_pem(&self.key)?;
                if data.len() > rsa.size() as usize {
                    return Err(RvError::ErrPkiDataInvalid);
                }

                let mut buf: Vec<u8> = vec![0; rsa.size() as usize];

                let flag = extra.map_or(false, |ex| match ex {
                    EncryptExtraData::Flag(flag) => flag,
                    _ => false,
                });
                if !flag {
                    let rsa_pub_der = rsa.public_key_to_der()?;
                    let rsa_pub = Rsa::public_key_from_der(&rsa_pub_der)?;
                    let _ = rsa_pub.public_decrypt(data, &mut buf, Padding::PKCS1)?;
                } else {
                    let rsa_pri_der = rsa.private_key_to_der()?;
                    let rsa_pri = Rsa::private_key_from_der(&rsa_pri_der)?;
                    let _ = rsa_pri.private_decrypt(data, &mut buf, Padding::PKCS1)?;
                }

                let pos = buf
                    .iter()
                    .position(|&x| x == 0)
                    .ok_or(RvError::ErrPkiInternal)?;
                buf.truncate(pos);

                Ok(buf)
            }
            _ => Err(RvError::ErrPkiKeyOperationInvalid),
        }
    }
}
