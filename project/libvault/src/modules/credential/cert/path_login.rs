use std::collections::HashMap;

use base64::{Engine as _, engine::general_purpose::STANDARD};
use foreign_types::ForeignType;
use glob::Pattern;
use openssl::{
    asn1::{Asn1OctetString, Asn1Time},
    nid::Nid,
    stack::Stack,
    x509::{
        X509, X509StoreContext, X509VerifyResult,
        store::{X509Store, X509StoreBuilder},
        verify::X509VerifyFlags,
    },
};
use openssl_sys::{
    ASN1_STRING_get0_data, ASN1_STRING_length, OBJ_obj2txt, X509_EXTENSION_get_data,
    X509_EXTENSION_get_object, X509_V_ERR_CERT_HAS_EXPIRED, X509_V_ERR_UNABLE_TO_GET_ISSUER_CERT,
    X509_V_ERR_UNABLE_TO_GET_ISSUER_CERT_LOCALLY, X509_get_ext, X509_get_ext_count, XKU_ANYEKU,
    XKU_SSL_CLIENT,
};
use serde::{Deserialize, Serialize};

use super::{CertBackend, CertBackendInner, CertEntry};
use crate::{
    errors::RvError,
    logical::{Auth, Backend, Field, FieldType, Operation, Path, Request, Response},
    rv_error_response, rv_error_string,
    utils::{
        self,
        cert::{
            deserialize_vec_x509, has_x509_ext_key_usage, has_x509_ext_key_usage_flag, is_ca_cert,
            serialize_vec_x509,
        },
        cidr::remote_addr_is_ok,
        ocsp::{self, OcspConfig},
        policy::equivalent_policies,
        sock_addr::SockAddr,
    },
};

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct ParsedCert {
    pub entry: CertEntry,
    #[serde(
        serialize_with = "serialize_vec_x509",
        deserialize_with = "deserialize_vec_x509"
    )]
    pub certs: Vec<X509>,
}

impl CertBackend {
    pub fn login_path(&self) -> Path {
        let backend = self.inner.clone();

        Path::builder()
            .pattern(r"login")
            .field(
                "name",
                Field::builder()
                    .field_type(FieldType::Str)
                    .required(true)
                    .description("The name of the certificate role to authenticate against."),
            )
            .operation(Operation::Write, {
                let handler = backend.clone();
                move |backend, req| {
                    let handler = handler.clone();
                    Box::pin(async move { handler.login(backend, req).await })
                }
            })
            .build()
    }
}

impl CertBackendInner {
    pub async fn login(
        &self,
        _backend: &dyn Backend,
        req: &mut Request,
    ) -> Result<Option<Response>, RvError> {
        let config = self.get_config(req).await?;
        if config.is_none() {
            return Err(RvError::ErrCredentailNotConfig);
        }

        if req.connection.is_none() {
            return Err(rv_error_response!("tls connection required"));
        }

        let conn = req.connection.as_ref().ok_or(RvError::ErrRequestNotReady)?;

        let client_cert = conn
            .peer_tls_cert
            .as_ref()
            .filter(|cert| !cert.is_empty())
            .and_then(|cert| cert.first())
            .ok_or(rv_error_response!("no client certificate found"))?;

        let common_name = client_cert
            .subject_name()
            .entries_by_nid(Nid::COMMONNAME)
            .next()
            .map(|c| c.data().as_utf8().map(|s| s.to_owned()))
            .transpose()?
            .unwrap_or_else(|| "".into());

        let serial_number = client_cert
            .serial_number()
            .to_bn()
            .and_then(|bn| bn.to_dec_str())?;
        let sn = String::from_utf8_lossy(serial_number.as_bytes()).to_string();

        let subject_key_id = client_cert
            .subject_key_id()
            .map(|asn1_ref| asn1_ref.as_slice())
            .unwrap_or(b"");
        let authority_key_id = client_cert
            .authority_key_id()
            .map(|asn1_ref| asn1_ref.as_slice())
            .unwrap_or(b"");

        let skid_base64 = STANDARD.encode(subject_key_id);
        let akid_base64 = STANDARD.encode(authority_key_id);
        let skid_hex = utils::hex_encode_with_colon(subject_key_id);
        let akid_hex = utils::hex_encode_with_colon(authority_key_id);

        let matched = self.verify_credentials(req).await?;

        if !matched.entry.token_bound_cidrs.is_empty() {
            let token_bound_cidrs: Vec<Box<dyn SockAddr>> = matched
                .entry
                .token_bound_cidrs
                .iter()
                .map(|s| s.sock_addr.clone())
                .collect();
            if !remote_addr_is_ok(&conn.peer_addr, &token_bound_cidrs) {
                return Err(RvError::ErrPermissionDenied);
            }
        }

        let mut auth = Auth {
            display_name: matched.entry.display_name.clone(),
            ..Default::default()
        };

        auth.metadata
            .insert("cert_name".into(), matched.entry.name.clone());
        auth.metadata.insert("common_name".into(), common_name);
        auth.metadata.insert("serial_number".into(), sn);
        auth.metadata.insert("subject_key_id".into(), skid_hex);
        auth.metadata.insert("authority_key_id".into(), akid_hex);

        auth.metadata
            .extend(self.certificate_extensions_metadata(client_cert, &matched));

        auth.internal_data
            .insert("subject_key_id".into(), skid_base64);
        auth.internal_data
            .insert("authority_key_id".into(), akid_base64);

        matched.entry.populate_token_auth(&mut auth);

        let resp = Response {
            auth: Some(auth),
            ..Response::default()
        };

        Ok(Some(resp))
    }

    pub async fn login_renew(
        &self,
        _backend: &dyn Backend,
        req: &mut Request,
    ) -> Result<Option<Response>, RvError> {
        let config = self
            .get_config(req)
            .await?
            .ok_or(RvError::ErrCredentailNotConfig)?;

        if req.connection.is_none() {
            return Err(rv_error_response!("tls connection required"));
        }

        if req.auth.is_none() {
            return Err(rv_error_response!("invalid request"));
        }
        let mut auth = req.auth.clone().unwrap();

        if !config.disable_binding {
            let subject_key_id = auth
                .metadata
                .get("subject_key_id")
                .ok_or(rv_error_response!(
                    "invalid request, not found subject_key_id"
                ))?;
            let authority_key_id =
                auth.metadata
                    .get("authority_key_id")
                    .ok_or(rv_error_response!(
                        "invalid request, not found authority_key_id"
                    ))?;

            let _matched = self.verify_credentials(req).await?;

            let conn = req.connection.as_ref().ok_or(RvError::ErrRequestNotReady)?;

            let client_cert = conn
                .peer_tls_cert
                .as_ref()
                .filter(|cert| !cert.is_empty())
                .and_then(|cert| cert.first())
                .ok_or(rv_error_response!("no client certificate found"))?;

            let cert_subject_key_id = client_cert
                .subject_key_id()
                .map(|asn1_ref| asn1_ref.as_slice())
                .unwrap_or(b"");
            let cert_authority_key_id = client_cert
                .authority_key_id()
                .map(|asn1_ref| asn1_ref.as_slice())
                .unwrap_or(b"");

            let skid_hex = utils::hex_encode_with_colon(cert_subject_key_id);
            let akid_hex = utils::hex_encode_with_colon(cert_authority_key_id);

            // Certificate should not only match a registered certificate policy.
            // Also, the identity of the certificate presented should match the identity of the certificate used during login
            if *subject_key_id != skid_hex && *authority_key_id != akid_hex {
                return Err(rv_error_response!(
                    "client identity during renewal not matching client identity used during login"
                ));
            }
        }

        let cert_name = auth
            .metadata
            .get("cert_name")
            .ok_or(rv_error_response!("invalid request, not found cert_name"))?;

        let cert = self.get_cert(req, cert_name.as_str()).await?;
        if cert.is_none() {
            return Ok(None);
        }

        let cert = cert.unwrap();

        if !equivalent_policies(&cert.policies, &auth.policies) {
            return Err(rv_error_string!("policies have changed, not renewing"));
        }

        auth.period = cert.token_period;
        auth.ttl = cert.token_ttl;
        auth.max_ttl = cert.token_max_ttl;

        Ok(Some(Response {
            auth: Some(auth),
            ..Response::default()
        }))
    }

    async fn verify_credentials(&self, req: &Request) -> Result<ParsedCert, RvError> {
        let peer_tls_cert = req
            .connection
            .as_ref()
            .and_then(|conn| conn.peer_tls_cert.as_ref())
            .filter(|cert| !cert.is_empty())
            .ok_or_else(|| rv_error_response!("client certificate must be supplied"))?;

        let client_cert = &peer_tls_cert[0];

        let cert_name = req
            .auth
            .as_ref()
            .and_then(|auth| auth.metadata.get("cert_name").cloned())
            .or_else(|| {
                req.get_data("name")
                    .ok()
                    .and_then(|name| name.as_str().map(|s| s.to_string()))
            })
            .unwrap_or_default();

        let (roots, trusted, trusted_non_ca, ocsp_config) =
            self.load_trusted_certs(req, &cert_name).await?;

        let trusted_chains = self.validate_cert(&roots, peer_tls_cert)?;

        let mut ret_err = Vec::new();

        for trust in trusted_non_ca.iter() {
            let crt = &trust.certs[0];
            let crt_key_id = crt.authority_key_id();
            let client_key_id = client_cert.authority_key_id();
            if crt_key_id.is_none() || client_key_id.is_none() {
                continue;
            }

            if crt.serial_number() == client_cert.serial_number()
                && crt_key_id.unwrap().as_slice() == client_key_id.unwrap().as_slice()
            {
                match self.matches_constraints(client_cert, &trust.certs, trust, &ocsp_config) {
                    Ok(true) => return Ok(trust.clone()),
                    Err(e) => ret_err.push(e),
                    _ => {}
                }
            }
        }

        if trusted_chains.is_empty() {
            if !ret_err.is_empty() {
                return Err(rv_error_response!(&format!(
                    "invalid certificate or no client certificate supplied; additionally got errors during \
                     verification:: {ret_err:?}"
                )));
            }
            return Err(rv_error_response!(
                "invalid certificate or no client certificate supplied"
            ));
        }

        for trust in trusted.iter() {
            if trust.certs.iter().any(|crt| trusted_chains.contains(crt)) {
                match self.matches_constraints(client_cert, &trusted_chains, trust, &ocsp_config) {
                    Ok(true) => return Ok(trust.clone()),
                    Err(e) => ret_err.push(e),
                    _ => {}
                }
            }
        }

        if !ret_err.is_empty() {
            return Err(rv_error_response!(&format!(
                "no chain matching all constraints could be found for this login certificate; additionally got errors \
                 during verification: {ret_err:?}"
            )));
        }

        Err(rv_error_response!(
            "no chain matching all constraints could be found for this login certificate"
        ))
    }

    async fn load_trusted_certs(
        &self,
        req: &Request,
        cert_name: &str,
    ) -> Result<(X509Store, Vec<ParsedCert>, Vec<ParsedCert>, OcspConfig), RvError> {
        let names: Vec<String> = if !cert_name.is_empty() {
            vec![cert_name.to_string()]
        } else {
            req.storage_list("cert/").await?
        };

        let mut trusted: Vec<ParsedCert> = Vec::new();
        let mut trusted_non_ca: Vec<ParsedCert> = Vec::new();
        let mut root_store_builder = X509StoreBuilder::new()?;
        let mut ocsp_config: OcspConfig = Default::default();

        root_store_builder.set_flags(X509VerifyFlags::PARTIAL_CHAIN)?;

        for name in names.iter() {
            if let Some(entry) = self.get_cert(req, name.trim_start_matches("cert/")).await? {
                if entry.certificate.is_empty() {
                    log::error!("failed to parse certificate, name: {name}");
                    continue;
                }

                if entry.ocsp_enabled {
                    ocsp_config.enable = true;
                    ocsp_config
                        .servers_override
                        .extend(entry.ocsp_servers_override.iter().cloned());
                    ocsp_config.failure_mode = if entry.ocsp_fail_open {
                        ocsp::FailureMode::FailOpenTrue
                    } else {
                        ocsp::FailureMode::FailOpenFalse
                    };
                    ocsp_config.query_all_servers |= entry.ocsp_query_all_servers;
                }

                let mut certs = entry.certificate.clone();
                certs.extend(entry.ocsp_ca_certificates.clone());

                if is_ca_cert(&certs[0]) {
                    for cert in certs.iter() {
                        root_store_builder.add_cert(cert.clone())?;
                    }
                    trusted.push(ParsedCert { entry, certs });
                } else {
                    trusted_non_ca.push(ParsedCert { entry, certs });
                }
            }
        }

        Ok((
            root_store_builder.build(),
            trusted,
            trusted_non_ca,
            ocsp_config,
        ))
    }

    fn validate_cert(&self, roots: &X509Store, peer_certs: &[X509]) -> Result<Vec<X509>, RvError> {
        if peer_certs.is_empty() {
            return Ok(Vec::new());
        }
        let mut stack = Stack::<X509>::new()?;
        peer_certs
            .iter()
            .skip(1)
            .try_for_each(|crt| stack.push(crt.clone()))?;

        let mut context = X509StoreContext::new()?;
        let (verified_res, verified_error, verified_chains) =
            context.init(roots, &peer_certs[0], &stack, |ctx| {
                let ret = ctx.verify_cert()?;
                let verified_chains: Vec<X509> = ctx
                    .chain()
                    .map(|chain| {
                        chain
                            .iter()
                            .map(|crt| crt.to_owned())
                            .filter(|crt| {
                                !has_x509_ext_key_usage(crt)
                                    || has_x509_ext_key_usage_flag(crt, XKU_ANYEKU | XKU_SSL_CLIENT)
                            })
                            .collect()
                    })
                    .unwrap_or_default();

                Ok((ret, ctx.error(), verified_chains))
            })?;

        if !verified_res {
            return match verified_error.as_raw() {
                X509_V_ERR_UNABLE_TO_GET_ISSUER_CERT
                | X509_V_ERR_UNABLE_TO_GET_ISSUER_CERT_LOCALLY => {
                    if peer_certs[0].not_after() <= Asn1Time::days_from_now(0)? {
                        Err(rv_error_response!(&unsafe {
                            X509VerifyResult::from_raw(X509_V_ERR_CERT_HAS_EXPIRED).to_string()
                        }))
                    } else {
                        Ok(Vec::new())
                    }
                }
                _ => Err(rv_error_response!(&verified_error.to_string())),
            };
        }

        Ok(verified_chains)
    }

    fn matches_constraints(
        &self,
        client_cert: &X509,
        trusted_chain: &[X509],
        config: &ParsedCert,
        ocsp_config: &ocsp::OcspConfig,
    ) -> Result<bool, RvError> {
        let mut ret = !self.check_for_chain_in_crls(trusted_chain)
            && self.matches_names(client_cert, config)
            && self.matches_common_name(client_cert, config)
            && self.matches_dns_sans(client_cert, config)
            && self.matches_email_sans(client_cert, config)
            && self.matches_uri_sans(client_cert, config)
            && self.matches_organizational_units(client_cert, config)
            && self.matches_certificate_extensions(client_cert, config);

        if config.entry.ocsp_enabled {
            let ocsp_ret = self.check_for_cert_in_ocsp(client_cert, trusted_chain, ocsp_config)?;
            ret = ret && ocsp_ret;
        }

        Ok(ret)
    }

    fn matches_names(&self, client_cert: &X509, config: &ParsedCert) -> bool {
        if config.entry.allowed_names.is_empty() {
            return true;
        }

        let common_name = match client_cert
            .subject_name()
            .entries_by_nid(Nid::COMMONNAME)
            .next()
        {
            Some(entry) => match entry.data().as_utf8() {
                Ok(cn_utf8) => cn_utf8.to_string(),
                Err(_) => return false,
            },
            None => return false,
        };

        let subject_alt_names = client_cert.subject_alt_names();

        for allowed_name in &config.entry.allowed_names {
            match Pattern::new(allowed_name) {
                Ok(pattern) => {
                    if pattern.matches(&common_name) {
                        return true;
                    }

                    if let Some(sans) = &subject_alt_names {
                        for san in sans {
                            if let Some(dnsname) = san.dnsname()
                                && pattern.matches(dnsname)
                            {
                                return true;
                            }
                            if let Some(email) = san.email()
                                && pattern.matches(email)
                            {
                                return true;
                            }
                        }
                    }
                }
                Err(_) => return false,
            }
        }

        false
    }

    fn matches_common_name(&self, client_cert: &X509, config: &ParsedCert) -> bool {
        if config.entry.allowed_common_names.is_empty() {
            return true;
        }

        let common_name = match client_cert
            .subject_name()
            .entries_by_nid(Nid::COMMONNAME)
            .next()
        {
            Some(entry) => match entry.data().as_utf8() {
                Ok(cn_utf8) => cn_utf8.to_string(),
                Err(_) => return false,
            },
            None => return false,
        };

        for allowed_common_name in &config.entry.allowed_common_names {
            match Pattern::new(allowed_common_name) {
                Ok(pattern) => {
                    if pattern.matches(&common_name) {
                        return true;
                    }
                }
                Err(_) => return false,
            }
        }

        false
    }

    fn matches_dns_sans(&self, client_cert: &X509, config: &ParsedCert) -> bool {
        if config.entry.allowed_dns_sans.is_empty() {
            return true;
        }

        let subject_alt_names = client_cert.subject_alt_names();

        for allowed_dns in &config.entry.allowed_dns_sans {
            match Pattern::new(allowed_dns) {
                Ok(pattern) => {
                    if let Some(sans) = &subject_alt_names {
                        for san in sans {
                            if let Some(dnsname) = san.dnsname()
                                && pattern.matches(dnsname)
                            {
                                return true;
                            }
                        }
                    }
                }
                Err(_) => return false,
            }
        }

        false
    }

    fn matches_email_sans(&self, client_cert: &X509, config: &ParsedCert) -> bool {
        if config.entry.allowed_email_sans.is_empty() {
            return true;
        }

        let subject_alt_names = client_cert.subject_alt_names();

        for allowed_email in config.entry.allowed_email_sans.iter() {
            match Pattern::new(allowed_email) {
                Ok(pattern) => {
                    if let Some(sans) = &subject_alt_names {
                        for san in sans {
                            if let Some(email) = san.email()
                                && pattern.matches(email)
                            {
                                return true;
                            }
                        }
                    }
                }
                Err(_) => return false,
            }
        }

        false
    }

    fn matches_uri_sans(&self, client_cert: &X509, config: &ParsedCert) -> bool {
        if config.entry.allowed_uri_sans.is_empty() {
            return true;
        }

        let subject_alt_names = client_cert.subject_alt_names();

        for allowed_uri in &config.entry.allowed_uri_sans {
            if let Ok(pattern) = Pattern::new(allowed_uri) {
                if let Some(sans) = &subject_alt_names
                    && sans
                        .iter()
                        .any(|san| san.uri().map_or(false, |uri| pattern.matches(uri)))
                {
                    return true;
                }
            } else {
                return false;
            }
        }

        false
    }

    fn matches_organizational_units(&self, client_cert: &X509, config: &ParsedCert) -> bool {
        if config.entry.allowed_organizational_units.is_empty() {
            return true;
        }

        let ou_name = match client_cert
            .subject_name()
            .entries_by_nid(Nid::ORGANIZATIONALUNITNAME)
            .next()
        {
            Some(entry) => match entry.data().as_utf8() {
                Ok(ou_utf8) => ou_utf8.to_string(),
                Err(_) => return false,
            },
            None => return false,
        };

        for allowed_ou in config.entry.allowed_organizational_units.iter() {
            match Pattern::new(allowed_ou) {
                Ok(pattern) => {
                    if pattern.matches(&ou_name) {
                        return true;
                    }
                }
                Err(_) => return false,
            }
        }

        false
    }

    fn matches_certificate_extensions(&self, client_cert: &X509, config: &ParsedCert) -> bool {
        if config.entry.required_extensions.is_empty() {
            return true;
        }

        let mut client_ext_map: HashMap<String, String> = HashMap::new();
        let mut hex_ext_map: HashMap<String, String> = HashMap::new();

        unsafe {
            let ext_count = X509_get_ext_count(client_cert.as_ptr());
            for i in 0..ext_count {
                let ext = X509_get_ext(client_cert.as_ptr(), i);

                let obj = X509_EXTENSION_get_object(ext);
                let mut oid_buf = [0; 128];
                let oid_len =
                    OBJ_obj2txt(oid_buf.as_mut_ptr() as *mut _, oid_buf.len() as i32, obj, 1);
                let oid = std::str::from_utf8(&oid_buf[..oid_len as usize]).unwrap();
                let ext_data = X509_EXTENSION_get_data(ext);
                let ext_value = ASN1_STRING_get0_data(ext_data as *mut _);
                let ext_len = ASN1_STRING_length(ext_data as *mut _);
                let ext_slice = std::slice::from_raw_parts(ext_value, ext_len as usize);
                let ext_str = Asn1OctetString::new_from_bytes(ext_slice)
                    .map(|ext_der| String::from_utf8_lossy(ext_der.as_slice()).to_string())
                    .unwrap_or_else(|_| String::new());
                client_ext_map.insert(oid.to_string(), ext_str);
                hex_ext_map.insert(oid.to_string(), hex::encode(ext_slice));
            }
        }

        for required_ext in config.entry.required_extensions.iter() {
            let req_ext: Vec<&str> = required_ext.splitn(2, ':').collect();
            if req_ext.len() != 2 {
                return false;
            }

            if req_ext[0] == "hex" {
                let req_hex_ext: Vec<&str> = req_ext[1].splitn(2, ':').collect();
                if req_hex_ext.len() != 2 {
                    return false;
                }

                let is_match = hex_ext_map
                    .get(req_hex_ext[0])
                    .and_then(|client_ext_value| {
                        Pattern::new(&req_hex_ext[1].to_lowercase())
                            .ok()
                            .filter(|pattern| pattern.matches(client_ext_value))
                    })
                    .is_some();

                if !is_match {
                    return false;
                }
            } else {
                let is_match = client_ext_map
                    .get(req_ext[0])
                    .and_then(|client_ext_value| {
                        Pattern::new(req_ext[1])
                            .ok()
                            .filter(|pattern| pattern.matches(client_ext_value))
                    })
                    .is_some();

                if !is_match {
                    return false;
                }
            }
        }

        true
    }

    fn certificate_extensions_metadata(
        &self,
        client_cert: &X509,
        config: &ParsedCert,
    ) -> HashMap<String, String> {
        let mut metadata_map: HashMap<String, String> = HashMap::new();
        if config.entry.allowed_metadata_extensions.is_empty() {
            return metadata_map;
        }

        let mut allowed_oid_map: HashMap<String, String> = HashMap::new();

        for oid_string in config.entry.allowed_metadata_extensions.iter() {
            allowed_oid_map.insert(oid_string.clone(), oid_string.replace('.', "-"));
        }

        unsafe {
            let ext_count = X509_get_ext_count(client_cert.as_ptr());
            for i in 0..ext_count {
                let ext = X509_get_ext(client_cert.as_ptr(), i);

                let obj = X509_EXTENSION_get_object(ext);
                let mut oid_buf = [0; 128];
                let oid_len =
                    OBJ_obj2txt(oid_buf.as_mut_ptr() as *mut _, oid_buf.len() as i32, obj, 1);
                let oid = std::str::from_utf8(&oid_buf[..oid_len as usize]).unwrap();
                if let Some(metadata_key) = allowed_oid_map.get(oid) {
                    let ext_data = X509_EXTENSION_get_data(ext);
                    let ext_value = ASN1_STRING_get0_data(ext_data as *mut _);
                    let ext_len = ASN1_STRING_length(ext_data as *mut _);
                    let ext_slice = std::slice::from_raw_parts(ext_value, ext_len as usize);
                    let ext_str = Asn1OctetString::new_from_bytes(ext_slice)
                        .map(|ext_der| String::from_utf8_lossy(ext_der.as_slice()).to_string())
                        .unwrap_or_else(|_| String::new());
                    metadata_map.insert(metadata_key.clone(), ext_str);
                }
            }
        }

        metadata_map
    }

    fn check_for_cert_in_ocsp(
        &self,
        _client_cert: &X509,
        chain: &[X509],
        ocsp_config: &OcspConfig,
    ) -> Result<bool, RvError> {
        if !ocsp_config.enable || chain.len() < 2 {
            return Ok(true);
        }

        //TODO
        //let err = self.ocsp_client.verify_leaf_certificate(client_cert, chain, ocsp_config)?;
        Ok(true)
    }

    fn check_for_chain_in_crls(&self, chain: &[X509]) -> bool {
        for cert in chain {
            let serial = cert.serial_number().to_bn();
            if serial.is_err() {
                return false;
            }

            if let Ok(bad_crls) = self.find_serial_in_crls(serial.unwrap())
                && !bad_crls.is_empty()
            {
                return true;
            }
        }

        false
    }
}
