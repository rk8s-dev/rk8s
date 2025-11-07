use std::{collections::HashMap, time::Duration};

use openssl::{bn::BigNum, x509::X509Crl};
use serde::{Deserialize, Serialize};
use url::Url;

use super::{CertBackend, CertBackendInner, path_config::Config};
use crate::{
    errors::RvError,
    logical::{Backend, Field, FieldType, Operation, Path, Request, Response},
    storage::StorageEntry,
    utils::{deserialize_duration, serialize_duration},
};

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct RevokedSerialInfo;

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct CDPInfo {
    pub url: String,
    #[serde(
        serialize_with = "serialize_duration",
        deserialize_with = "deserialize_duration"
    )]
    pub valid_until: Duration,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct CRLInfo {
    pub cdp: Option<CDPInfo>,
    pub serials: HashMap<String, RevokedSerialInfo>,
}

impl CertBackend {
    pub fn crl_path(&self) -> Path {
        let cert_backend_ref1 = self.inner.clone();
        let cert_backend_ref2 = self.inner.clone();
        let cert_backend_ref3 = self.inner.clone();

        Path::builder()
            .pattern(r"crls/(?P<name>\w[\w-]+\w)")
            .field(
                "name",
                Field::builder()
                    .field_type(FieldType::Str)
                    .required(true)
                    .description("The name of the certificate."),
            )
            .field(
                "crl",
                Field::builder()
                    .field_type(FieldType::Str)
                    .description(
                        r#"The public CRL that should be trusted to attest to certificates' validity statuses.
    May be DER or PEM encoded. Note: the expiration time
    is ignored; if the CRL is no longer valid, delete it
    using the same name as specified here."#,
                    ),
            )
            .field(
                "url",
                Field::builder()
                    .field_type(FieldType::Str)
                    .description(
                        "The URL of a CRL distribution point.  Only one of 'crl' or 'url' parameters should be specified.",
                    ),
            )
            .operation(Operation::Read, {
                let handler = cert_backend_ref1.clone();
                move |backend, req| {
                    let handler = handler.clone();
                    Box::pin(async move { handler.read_crl(backend, req).await })
                }
            })
            .operation(Operation::Write, {
                let handler = cert_backend_ref2.clone();
                move |backend, req| {
                    let handler = handler.clone();
                    Box::pin(async move { handler.write_crl(backend, req).await })
                }
            })
            .operation(Operation::Delete, {
                let handler = cert_backend_ref3.clone();
                move |backend, req| {
                    let handler = handler.clone();
                    Box::pin(async move { handler.delete_crl(backend, req).await })
                }
            })
            .help(
                r#"
This endpoint allows you to list, create, read, update, and delete the Certificate
Revocation Lists checked during authentication, and/or CRL Distribution Point
URLs.

When any CRLs are in effect, any login will check the trust chains sent by a
client against the submitted or retrieved CRLs. Any chain containing a serial number revoked
by one or more of the CRLs causes that chain to be marked as invalid for the
authentication attempt. Conversely, *any* valid chain -- that is, a chain
in which none of the serials are revoked by any CRL -- allows authentication.
This allows authentication to succeed when interim parts of one chain have been
revoked; for instance, if a certificate is signed by two intermediate CAs due to
one of them expiring.
                "#,
            )
            .build()
    }

    pub fn crl_list_path(&self) -> Path {
        let cert_backend_ref = self.inner.clone();

        Path::builder()
            .pattern(r"crls/?$")
            .operation(Operation::List, {
                let handler = cert_backend_ref.clone();
                move |backend, req| {
                    let handler = handler.clone();
                    Box::pin(async move { handler.list_crl(backend, req).await })
                }
            })
            .help(
                r#"
This endpoint allows you to list, create, read, update, and delete the Certificate
Revocation Lists checked during authentication, and/or CRL Distribution Point
URLs.

When any CRLs are in effect, any login will check the trust chains sent by a
client against the submitted or retrieved CRLs. Any chain containing a serial number revoked
by one or more of the CRLs causes that chain to be marked as invalid for the
authentication attempt. Conversely, *any* valid chain -- that is, a chain
in which none of the serials are revoked by any CRL -- allows authentication.
This allows authentication to succeed when interim parts of one chain have been
revoked; for instance, if a certificate is signed by two intermediate CAs due to
one of them expiring.
                "#,
            )
            .build()
    }
}

impl CertBackendInner {
    pub async fn get_crl(&self, req: &mut Request) -> Result<Option<Config>, RvError> {
        let storage_entry = req.storage_get("crls").await?;
        if storage_entry.is_none() {
            return Ok(None);
        }

        let entry = storage_entry.unwrap();
        let config: Config = serde_json::from_slice(entry.value.as_slice())?;
        Ok(Some(config))
    }

    pub async fn set_crl(
        &self,
        req: &mut Request,
        x509crl: Option<X509Crl>,
        name: &str,
        cdp: Option<CDPInfo>,
    ) -> Result<(), RvError> {
        self.update_crl_cache(req).await?;

        let mut crl_info = CRLInfo {
            cdp,
            ..Default::default()
        };

        if let Some(crl) = x509crl
            && let Some(revoked_stack) = crl.get_revoked()
        {
            for revoked in revoked_stack.iter() {
                let serial = revoked.serial_number().to_bn()?;
                let serial_str = serial.to_dec_str()?;
                crl_info
                    .serials
                    .insert(serial_str.to_lowercase(), RevokedSerialInfo {});
            }
        }

        let entry = StorageEntry::new(format!("crls/{name}").as_str(), &crl_info)?;
        req.storage_put(&entry).await?;

        self.crls.insert(name.to_string(), crl_info);

        Ok(())
    }

    pub async fn fetch_crl(
        &self,
        req: &mut Request,
        name: &str,
        crl: CRLInfo,
    ) -> Result<(), RvError> {
        if crl.cdp.is_none() {
            return Err(RvError::ErrRequestInvalid);
        }

        let url = crl.cdp.as_ref().unwrap().url.as_str();
        let body: String = ureq::get(url).call()?.into_string()?;

        let x509crl = X509Crl::from_pem(body.as_bytes())?;
        self.set_crl(req, Some(x509crl), name, crl.cdp).await?;
        Ok(())
    }

    pub async fn list_crl(
        &self,
        _backend: &dyn Backend,
        req: &mut Request,
    ) -> Result<Option<Response>, RvError> {
        let crls = req.storage_list("crls/").await?;
        let resp = Response::list_response(&crls);
        Ok(Some(resp))
    }

    pub async fn read_crl(
        &self,
        _backend: &dyn Backend,
        req: &mut Request,
    ) -> Result<Option<Response>, RvError> {
        let name = req.get_data_as_str("name")?.to_lowercase();
        if name.is_empty() {
            return Err(RvError::ErrRequestNoDataField);
        }

        self.update_crl_cache(req).await?;

        let crl = self.crls.get(&name);
        if crl.is_none() {
            log::error!("no such CRL {name}");
            return Err(RvError::ErrRequestInvalid);
        };
        let crl_info = crl.unwrap();

        let crl_data = serde_json::to_value(&*crl_info)?;

        Ok(Some(Response::data_response(Some(
            crl_data.as_object().unwrap().clone(),
        ))))
    }

    pub async fn write_crl(
        &self,
        _backend: &dyn Backend,
        req: &mut Request,
    ) -> Result<Option<Response>, RvError> {
        let name = req.get_data_as_str("name")?.to_lowercase();
        if name.is_empty() {
            return Err(RvError::ErrRequestNoDataField);
        }

        if let Ok(crl_value) = req.get_data("crl") {
            let crl = crl_value.as_str().ok_or(RvError::ErrRequestFieldInvalid)?;
            let x509crl = X509Crl::from_pem(crl.as_bytes())?;
            self.set_crl(req, Some(x509crl), &name, None).await?;
        } else if let Ok(url_value) = req.get_data("url") {
            let url = url_value.as_str().ok_or(RvError::ErrRequestFieldInvalid)?;
            if url.is_empty() {
                return Err(RvError::ErrRequestInvalid);
            }
            let _ = Url::parse(url)?;
            let cdp_info = CDPInfo {
                url: url.to_string(),
                ..Default::default()
            };
            let crl_info = CRLInfo {
                cdp: Some(cdp_info),
                ..Default::default()
            };

            self.fetch_crl(req, &name, crl_info).await?;
        } else {
            return Err(RvError::ErrRequestNoDataField);
        }

        Ok(None)
    }

    pub async fn delete_crl(
        &self,
        _backend: &dyn Backend,
        req: &mut Request,
    ) -> Result<Option<Response>, RvError> {
        let name = req.get_data_as_str("name")?.to_lowercase();
        if name.is_empty() {
            return Err(RvError::ErrRequestNoDataField);
        }

        self.update_crl_cache(req).await?;

        if self.crls.get(&name).is_none() {
            log::error!("no such CRL {name}");
            return Err(RvError::ErrRequestInvalid);
        }

        req.storage_delete(format!("crls/{}", name.to_lowercase()).as_str())
            .await?;

        self.crls.remove(&name);

        Ok(None)
    }

    pub fn find_serial_in_crls(
        &self,
        serial: BigNum,
    ) -> Result<HashMap<String, RevokedSerialInfo>, RvError> {
        let serial_str = serial.to_dec_str()?;
        let mut ret: HashMap<String, RevokedSerialInfo> = HashMap::new();
        for item in self.crls.iter() {
            let crl = item.value();
            if let Some(info) = crl.serials.get(&serial_str.to_lowercase()) {
                ret.insert(item.key().clone(), info.clone());
            }
        }

        Ok(ret)
    }

    async fn update_crl_cache(&self, req: &Request) -> Result<(), RvError> {
        if !self.crls.is_empty() {
            return Ok(());
        }

        let keys = req.storage_list("crls/").await?;
        if keys.is_empty() {
            return Ok(());
        }

        for key in &keys {
            let entry = match req.storage_get(&format!("crls/{key}")).await {
                Ok(None) => continue,
                Ok(Some(entry)) => entry,
                Err(err) => {
                    self.crls.clear();
                    return Err(err);
                }
            };

            let crl_info: CRLInfo = match serde_json::from_slice(entry.value.as_slice()) {
                Ok(crl_info) => crl_info,
                Err(err) => {
                    self.crls.clear();
                    return Err(RvError::SerdeJson { source: err });
                }
            };

            self.crls.insert(key.to_string(), crl_info);
        }

        Ok(())
    }
}
