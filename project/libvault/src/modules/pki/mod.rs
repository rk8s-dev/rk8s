//! The `rusty_vault::pki` module implements public key cryptography features, including
//! manipulating certificates as a CA or encrypting a piece of data by using a public key.

use std::{
    any::Any,
    sync::{atomic::AtomicU64, Arc},
    time::Duration,
};

use derive_more::Deref;

use crate::{
    core::Core,
    errors::RvError,
    logical::{secret::Secret, Backend, LogicalBackend, Request, Response},
    modules::Module,
    new_logical_backend, new_logical_backend_internal, new_secret, new_secret_internal,
};

pub mod field;
pub mod path_config_ca;
pub mod path_config_crl;
pub mod path_fetch;
pub mod path_issue;
pub mod path_keys;
pub mod path_revoke;
pub mod path_roles;
pub mod path_root;
pub mod util;

static PKI_BACKEND_HELP: &str = r#"
The PKI backend dynamically generates X509 server and client certificates.

After mounting this backend, configure the CA using the "pem_bundle" endpoint within
the "config/" path.
"#;
const _DEFAULT_LEASE_TTL: Duration = Duration::from_secs(3600_u64);

pub struct PkiModule {
    pub name: String,
    pub backend: Arc<PkiBackend>,
}

pub struct PkiBackendInner {
    pub core: Arc<Core>,
    pub cert_count: AtomicU64,
    pub revoked_cert_count: AtomicU64,
}

#[derive(Deref)]
pub struct PkiBackend {
    #[deref]
    pub inner: Arc<PkiBackendInner>,
}

impl PkiBackend {
    pub fn new(core: Arc<Core>) -> Self {
        Self {
            inner: Arc::new(PkiBackendInner {
                core,
                cert_count: AtomicU64::new(0),
                revoked_cert_count: AtomicU64::new(0),
            }),
        }
    }

    pub fn new_backend(&self) -> LogicalBackend {
        let pki_backend_ref1 = self.inner.clone();
        let pki_backend_ref2 = self.inner.clone();

        let mut backend = new_logical_backend!({
            root_paths: ["config/*", "revoke/*", "crl/rotate"],
            unauth_paths: ["cert/*", "ca/pem", "ca", "crl", "crl/pem"],
            secrets: [{
                secret_type: "pki",
                revoke_handler: pki_backend_ref1.revoke_secret_creds,
                renew_handler: pki_backend_ref2.renew_secret_creds,
            }],
            help: PKI_BACKEND_HELP,
        });

        backend.paths.push(Arc::new(self.roles_path()));
        backend.paths.push(Arc::new(self.config_ca_path()));
        backend.paths.push(Arc::new(self.root_generate_path()));
        backend.paths.push(Arc::new(self.root_delete_path()));
        backend.paths.push(Arc::new(self.fetch_ca_path()));
        backend.paths.push(Arc::new(self.fetch_crl_path()));
        backend.paths.push(Arc::new(self.fetch_cert_path()));
        backend.paths.push(Arc::new(self.fetch_cert_crl_path()));
        backend.paths.push(Arc::new(self.issue_path()));
        backend.paths.push(Arc::new(self.revoke_path()));
        backend.paths.push(Arc::new(self.crl_rotate_path()));
        backend.paths.push(Arc::new(self.keys_generate_path()));
        backend.paths.push(Arc::new(self.keys_import_path()));
        backend.paths.push(Arc::new(self.keys_sign_path()));
        backend.paths.push(Arc::new(self.keys_verify_path()));
        backend.paths.push(Arc::new(self.keys_encrypt_path()));
        backend.paths.push(Arc::new(self.keys_decrypt_path()));

        backend
    }
}

#[maybe_async::maybe_async]
impl PkiBackendInner {
    pub async fn revoke_secret_creds(
        &self,
        _backend: &dyn Backend,
        _req: &mut Request,
    ) -> Result<Option<Response>, RvError> {
        Ok(None)
    }
    pub async fn renew_secret_creds(
        &self,
        _backend: &dyn Backend,
        _req: &mut Request,
    ) -> Result<Option<Response>, RvError> {
        Ok(None)
    }
}

impl PkiModule {
    pub fn new(core: Arc<Core>) -> Self {
        Self { name: "pki".to_string(), backend: Arc::new(PkiBackend::new(core)) }
    }
}

impl Module for PkiModule {
    fn name(&self) -> String {
        self.name.clone()
    }

    fn as_any_arc(self: Arc<Self>) -> Arc<dyn Any + Send + Sync> {
        self
    }

    fn setup(&self, core: &Core) -> Result<(), RvError> {
        let pki = self.backend.clone();
        let pki_backend_new_func = move |_c: Arc<Core>| -> Result<Arc<dyn Backend>, RvError> {
            let mut pki_backend = pki.new_backend();
            pki_backend.init()?;
            Ok(Arc::new(pki_backend))
        };
        core.add_logical_backend("pki", Arc::new(pki_backend_new_func))
    }

    fn cleanup(&self, core: &Core) -> Result<(), RvError> {
        core.delete_logical_backend("pki")
    }
}

#[cfg(test)]
mod test {
    use std::time::{SystemTime, UNIX_EPOCH};

    use openssl::{asn1::Asn1Time, ec::EcKey, nid::Nid, pkey::PKey, rsa::Rsa, x509::X509};
    use serde_json::{json, Value};

    use super::*;
    use crate::{
        core::Core,
        test_utils::{new_unseal_test_rusty_vault, test_delete_api, test_mount_api, test_read_api, test_write_api},
    };

    #[maybe_async::maybe_async]
    async fn config_ca(core: &Core, token: &str, path: &str) {
        let ca_pem_bundle = format!("{}{}", CA_CERT_PEM, CA_KEY_PEM);

        let ca_data = json!({
            "pem_bundle": ca_pem_bundle,
        })
        .as_object()
        .cloned();

        // config ca
        let resp = test_write_api(core, token, format!("{}config/ca", path).as_str(), true, ca_data).await;
        assert!(resp.is_ok());
    }

    #[maybe_async::maybe_async]
    async fn config_role(core: &Core, token: &str, path: &str, role_name: &str, key_type: &str, key_bits: u32) {
        let role_data = json!({
            "ttl": "60d",
            "max_ttl": "365d",
            "key_type": key_type,
            "key_bits": key_bits,
            "country": "CN",
            "province": "Beijing",
            "locality": "Beijing",
            "organization": "OpenAtom",
            "no_store": false,
        })
        .as_object()
        .cloned();

        // config role
        let resp = test_write_api(&core, token, format!("{}roles/{}", path, role_name).as_str(), true, role_data).await;
        assert!(resp.is_ok());
    }

    #[maybe_async::maybe_async]
    async fn generate_root(
        core: &Core,
        token: &str,
        path: &str,
        exported: bool,
        key_type: &str,
        key_bits: u32,
        is_ok: bool,
    ) {
        let common_name = "test-ca";
        let req_data = json!({
            "common_name": common_name,
            "ttl": "365d",
            "country": "cn",
            "key_type": key_type,
            "key_bits": key_bits,
        })
        .as_object()
        .cloned();
        println!("generate root req_data: {:?}, is_ok: {}", req_data, is_ok);
        let resp = test_write_api(
            core,
            token,
            format!("{}root/generate/{}", path, if exported { "exported" } else { "internal" }).as_str(),
            is_ok,
            req_data,
        )
        .await;
        if !is_ok {
            return;
        }
        let resp_body = resp.unwrap();
        assert!(resp_body.is_some());
        let data = resp_body.unwrap().data;
        assert!(data.is_some());
        let key_data = data.unwrap();
        println!("generate root result: {:?}", key_data);

        let resp_ca_pem = test_read_api(core, token, format!("{}ca/pem", path).as_str(), true).await;
        let resp_ca_pem_cert_data = resp_ca_pem.unwrap().unwrap().data.unwrap();

        let ca_cert = X509::from_pem(resp_ca_pem_cert_data["certificate"].as_str().unwrap().as_bytes()).unwrap();
        let subject = ca_cert.subject_name();
        let cn = subject.entries_by_nid(Nid::COMMONNAME).next().unwrap();
        assert_eq!(cn.data().as_slice(), common_name.as_bytes());

        let not_after = Asn1Time::days_from_now(365).unwrap();
        let ttl_diff = ca_cert.not_after().diff(&not_after);
        assert!(ttl_diff.is_ok());
        let ttl_diff = ttl_diff.unwrap();
        assert_eq!(ttl_diff.days, 0);

        if exported {
            assert!(key_data["private_key_type"].as_str().is_some());
            assert_eq!(key_data["private_key_type"].as_str().unwrap(), key_type);
            assert!(key_data["private_key"].as_str().is_some());
            let private_key_pem = key_data["private_key"].as_str().unwrap();
            match key_type {
                "rsa" => {
                    let rsa_key = Rsa::private_key_from_pem(private_key_pem.as_bytes());
                    assert!(rsa_key.is_ok());
                    assert_eq!(rsa_key.unwrap().size() * 8, key_bits);
                }
                "ec" => {
                    let ec_key = EcKey::private_key_from_pem(private_key_pem.as_bytes());
                    assert!(ec_key.is_ok());
                    assert_eq!(ec_key.unwrap().group().degree(), key_bits);
                }
                _ => {}
            }
        } else {
            assert!(key_data.get("private_key").is_none());
        }
    }

    #[maybe_async::maybe_async]
    async fn delete_root(core: &Core, token: &str, path: &str, is_ok: bool) {
        let resp = test_delete_api(core, token, format!("{}root", path).as_str(), is_ok, None).await;
        if !is_ok {
            return;
        }
        assert!(resp.is_ok());

        let resp_ca_pem = test_read_api(core, token, format!("{}ca/pem", path).as_str(), false).await;
        assert_eq!(resp_ca_pem.unwrap_err(), RvError::ErrPkiCaNotConfig);
    }

    #[maybe_async::maybe_async]
    async fn issue_cert_by_generate_root(
        core: &Core,
        token: &str,
        path: &str,
        role_name: &str,
        key_type: &str,
        key_bits: u32,
    ) {
        let dns_sans = vec!["test.com", "a.test.com", "b.test.com"];
        let issue_data = json!({
            "ttl": "10d",
            "common_name": "test.com",
            "alt_names": "a.test.com,b.test.com",
        })
        .as_object()
        .cloned();

        // issue cert
        let now_timestamp = SystemTime::now().duration_since(UNIX_EPOCH).unwrap().as_secs();
        let resp = test_write_api(core, token, format!("{}issue/{}", path, role_name).as_str(), true, issue_data).await;
        assert!(resp.is_ok());
        let resp_body = resp.unwrap();
        assert!(resp_body.is_some());
        let data = resp_body.unwrap().data;
        assert!(data.is_some());
        let cert_data = data.unwrap();
        let cert = X509::from_pem(cert_data["certificate"].as_str().unwrap().as_bytes()).unwrap();
        let alt_names = cert.subject_alt_names();
        assert!(alt_names.is_some());
        let alt_names = alt_names.unwrap();
        assert_eq!(alt_names.len(), dns_sans.len());
        for alt_name in alt_names {
            assert!(dns_sans.contains(&alt_name.dnsname().unwrap()));
        }
        assert_eq!(cert_data["private_key_type"].as_str().unwrap(), key_type);
        let priv_key = PKey::private_key_from_pem(cert_data["private_key"].as_str().unwrap().as_bytes()).unwrap();
        assert_eq!(priv_key.bits(), key_bits);
        assert!(priv_key.public_eq(&cert.public_key().unwrap()));
        let serial_number = cert.serial_number().to_bn().unwrap();
        let serial_number_hex = serial_number.to_hex_str().unwrap();
        assert_eq!(
            cert_data["serial_number"].as_str().unwrap().replace(":", "").to_lowercase().as_str(),
            serial_number_hex.to_lowercase().as_str()
        );
        let expiration_time = Asn1Time::from_unix(cert_data["expiration"].as_i64().unwrap()).unwrap();
        let ttl_compare = cert.not_after().compare(&expiration_time);
        assert!(ttl_compare.is_ok());
        assert_eq!(ttl_compare.unwrap(), std::cmp::Ordering::Equal);
        let expiration_ttl = cert_data["expiration"].as_u64().unwrap();
        let ttl = expiration_ttl - now_timestamp;
        let expect_ttl = 10 * 24 * 60 * 60;
        println!("ttl: {}, expect_ttl: {}", ttl, expect_ttl);
        //assert!(ttl <= expect_ttl);

        let authority_key_id = cert.authority_key_id();
        assert!(authority_key_id.is_some());

        println!("authority_key_id: {:?}", authority_key_id.unwrap().as_slice());

        let resp_ca_pem = test_read_api(core, token, format!("{}ca/pem", path).as_str(), true).await;
        let resp_ca_pem_cert_data = resp_ca_pem.unwrap().unwrap().data.unwrap();

        let ca_cert = X509::from_pem(resp_ca_pem_cert_data["certificate"].as_str().unwrap().as_bytes()).unwrap();
        let subject = ca_cert.subject_name();
        let cn = subject.entries_by_nid(Nid::COMMONNAME).next().unwrap();
        assert_eq!(cn.data().as_slice(), "test-ca".as_bytes());
        println!("ca subject_key_id: {:?}", ca_cert.subject_key_id().unwrap().as_slice());
        assert_eq!(ca_cert.subject_key_id().unwrap().as_slice(), authority_key_id.unwrap().as_slice());
    }

    const CA_CERT_PEM: &str = r#"
-----BEGIN CERTIFICATE-----
MIIC/DCCAeSgAwIBAgIBAjANBgkqhkiG9w0BAQsFADASMRAwDgYDVQQDDAdSb290
IENBMCAXDTIwMTIxMjIwMTY1MFoYDzIxMjAxMjEzMjAxNjUwWjANMQswCQYDVQQD
DAJDQTCCASIwDQYJKoZIhvcNAQEBBQADggEPADCCAQoCggEBAJadpD0ASxxfxsvd
j9IxsogVzMSGLFziaYuE9KejU9+R479RifvwfBANO62sNWJ19X//9G5UjwWmkiOz
n1k50DkYsBBA3mJzik6wjt/c58lBIlSEgAgpvDU8ht8w3t20JP9+YqXAeugqFj/W
l9rFQtsvaWSRywjXVlp5fxuEQelNnXcJEKhsKTNExsBUZebo4/J1BWpklWzA9P0l
YW5INvDAAwcF1nzlEf0Y6Eot03IMNyg2MTE4hehxjdgCSci8GYnFirE/ojXqqpAc
ZGh7r2dqWgZUD1Dh+bT2vjrUzj8eTH3GdzI+oljt29102JIUaqj3yzRYkah8FLF9
CLNNsUcCAwEAAaNgMF4wDwYDVR0TAQH/BAUwAwEB/zALBgNVHQ8EBAMCAQYwHQYD
VR0OBBYEFLQRM/HX4l73U54gIhBPhga/H8leMB8GA1UdIwQYMBaAFI71Ja8em2uE
PXyAmslTnE1y96NSMA0GCSqGSIb3DQEBCwUAA4IBAQDacg5HHo+yaApPb6mk/SP8
J3CjQWhRzv91kwsGLnhPgZI4HcspdJgTaznrstiiA1VRjkQ/kwzd29Sftb1kBio0
pAyblmravufRdojfTgkMnFyRSaj4FHuOQq8lnX3gwlKn5hBtEF6Qd+U79MkpMALa
cxPdyJs2tgDOpP1jweubOawqsKlxhAjwgdeX0Qp8iUj4BrY0zg4Q5im0mEKo4hij
49dQQqoWakCejH4QP2+T1urJsRGn9rXk/nkW9daNYaQDyoAPlnhr5oU+pP3+hSec
Ol83n08VZ8BizTSPkG0J66sZGC5jvsf5rX8YHURv0jNxHcG8QVEmyCwPqfDTI4fz
-----END CERTIFICATE-----"#;
    const CA_KEY_PEM: &str = r#"
-----BEGIN PRIVATE KEY-----
MIIEvgIBADANBgkqhkiG9w0BAQEFAASCBKgwggSkAgEAAoIBAQCWnaQ9AEscX8bL
3Y/SMbKIFczEhixc4mmLhPSno1PfkeO/UYn78HwQDTutrDVidfV///RuVI8FppIj
s59ZOdA5GLAQQN5ic4pOsI7f3OfJQSJUhIAIKbw1PIbfMN7dtCT/fmKlwHroKhY/
1pfaxULbL2lkkcsI11ZaeX8bhEHpTZ13CRCobCkzRMbAVGXm6OPydQVqZJVswPT9
JWFuSDbwwAMHBdZ85RH9GOhKLdNyDDcoNjExOIXocY3YAknIvBmJxYqxP6I16qqQ
HGRoe69naloGVA9Q4fm09r461M4/Hkx9xncyPqJY7dvddNiSFGqo98s0WJGofBSx
fQizTbFHAgMBAAECggEABdXHpiFbx5aiUgWca81HGGSX0UlNcK/I3QHipJf8SN4T
D7dt/Be+BrUsibbxPoZJY5Mb+iZGgDaK1N1BoChQO9YMBCUvOGs3gYLvlhat2Csw
1Etp1mcfhoR4yS7Qg5BWGpvf4IILgPEYeZKrwWsBAxLcJ2xKjGYjT1ADr6I5F3u+
FYN+bvlXxr07GccfS+UHt04oT0dHwxQzFaJj+yqKWGo2IFtPqtr6Sgoh9a+yFYIi
8a9MigTTt+IyJ55OuC/FHRf1PofprftADFts78k43qxWtrxSrQVdlNXp1lpZOtuR
7gvB/r3a2byDYxCxYVu98tQuOfW909TdDgPmEJjcAQKBgQDHcTYi+zcGKooN3tfK
Oc6hnFXAYTNpYp074NfIYB8i10CwbvWta1FDoi3iRqlQFwg+pu12UefZsj21F+aF
v2eGP33kQ6yiXJQ3j7jam7dY+tZ6xb0dthm+X/INuHp/HbSb1qKFmSO2rmMDQg+e
Crqts9+t5Xk04ewTgpySLZjvRwKBgQDBU85Ls3s8osre5EmVBRd5qBt6ILnjtdoa
UxrrrWopRx2q3HsI41VhKFx0PGs6ia0c6+9GFR6wX/Qevj85DADbzHDA5XEZq98q
8yH4lme2Uj2gOlWqyhDeC/g4S+MsbNoIaUOZbMGg/phyAe20HvtvD7MUhZ/2rkta
U5UjFpouAQKBgQC/+vU+tQ0hTV94vJKBoiWKIX/V4HrprbhmxCdSRVyTYBpv+09X
8J7X+MwsLRKb+p/AF1UreOox/sYxhOEsy7MuYf2f9Zi+7VjrJtis7gmOiF5e7er+
J6UeQSMyG+smY4TQIcptyZy8I59Bqpx36CIMRMJClUqYIgTqPubSOzwkzwKBgENB
9LNBbc5alFmW8kJ10wTwBx8l44Xk7kvaPbNgUV6q7xdSPTuKW1nBwOhvXJ6w5xj4
u/WVw2d4+mT3qucd1e6h4Vg6em6D7M/0Zg0lxk8XrXjg0ozoX5XgdCqhvBboh7IF
bQ8jVvm7mS2QnjHb1X196L9q/YvEd1KlYW0jn+ABAoGBAKwArjjmr3zRhJurujA5
x/+V28hUf8m8P2NxP5ALaDZagdaMfzjGZo3O3wDv33Cds0P5GMGQYnRXDxcZN/2L
/453f0uUObRwFepuv9HzuvPgkTRGpcLFiIHCThiKdyBgPKoq39qjbAyWQcfmW8+S
2k24wuH7oUtLlvf05p4cqfEx
-----END PRIVATE KEY-----"#;

    #[maybe_async::test(feature = "sync_handler", async(all(not(feature = "sync_handler")), tokio::test))]
    async fn test_pki_config_ca() {
        let (_rvault, core, root_token) = new_unseal_test_rusty_vault("test_pki_config_ca").await;
        let token = &root_token;
        let path = "pki/";

        // mount pki backend to path: pki/
        test_mount_api(&core, token, "pki", path).await;

        // config ca
        config_ca(&core, token, path).await;

        let resp_ca = test_read_api(&core, token, format!("{}ca", path).as_str(), true).await;
        let resp_ca_pem = test_read_api(&core, token, format!("{}ca/pem", path).as_str(), true).await;
        let resp_ca_cert_data = resp_ca.unwrap().unwrap().data.unwrap();
        let resp_ca_pem_cert_data = resp_ca_pem.unwrap().unwrap().data.unwrap();
        assert!(resp_ca_cert_data.get("private_key").is_none());
        assert!(resp_ca_pem_cert_data.get("private_key").is_none());
        assert_eq!(
            resp_ca_cert_data["certificate"].as_str().unwrap(),
            resp_ca_pem_cert_data["certificate"].as_str().unwrap()
        );
        assert_eq!(
            resp_ca_cert_data["serial_number"].as_str().unwrap(),
            resp_ca_pem_cert_data["serial_number"].as_str().unwrap()
        );
        assert_eq!(resp_ca_cert_data["certificate"].as_str().unwrap().trim(), CA_CERT_PEM.trim());
    }

    #[maybe_async::test(feature = "sync_handler", async(all(not(feature = "sync_handler")), tokio::test))]
    async fn test_pki_config_role() {
        let (_rvault, core, root_token) = new_unseal_test_rusty_vault("test_pki_config_role").await;
        let token = &root_token;
        let path = "pki/";
        let role_name = "test";

        // mount pki backend to path: pki/
        test_mount_api(&core, token, "pki", path).await;

        // config ca
        config_ca(&core, token, path).await;

        // config role
        config_role(&core, token, path, role_name, "rsa", 4096).await;

        let resp = test_read_api(&core, token, &format!("{}roles/{}", path, role_name), true).await;
        assert!(resp.as_ref().unwrap().is_some());
        let resp = resp.unwrap();
        assert!(resp.is_some());
        let data = resp.unwrap().data;
        assert!(data.is_some());
        let role_data = data.unwrap();
        println!("role_data: {:?}", role_data);

        assert_eq!(role_data["ttl"].as_u64().unwrap(), 60 * 24 * 60 * 60);
        assert_eq!(role_data["max_ttl"].as_u64().unwrap(), 365 * 24 * 60 * 60);
        assert_eq!(role_data["not_before_duration"].as_u64().unwrap(), 30);
        assert_eq!(role_data["key_type"].as_str().unwrap(), "rsa");
        assert_eq!(role_data["key_bits"].as_u64().unwrap(), 4096);
        assert_eq!(role_data["country"].as_str().unwrap(), "CN");
        assert_eq!(role_data["province"].as_str().unwrap(), "Beijing");
        assert_eq!(role_data["locality"].as_str().unwrap(), "Beijing");
        assert_eq!(role_data["organization"].as_str().unwrap(), "OpenAtom");
        assert_eq!(role_data["no_store"].as_bool().unwrap(), false);
    }

    #[maybe_async::test(feature = "sync_handler", async(all(not(feature = "sync_handler")), tokio::test))]
    async fn test_pki_issue_cert() {
        let (_rvault, core, root_token) = new_unseal_test_rusty_vault("test_pki_issue_cert").await;
        let token = &root_token;
        let path = "pki/";
        let role_name = "test";

        // mount pki backend to path: pki/
        test_mount_api(&core, token, "pki", path).await;

        // config ca
        config_ca(&core, token, path).await;

        // config role
        config_role(&core, token, path, role_name, "rsa", 4096).await;

        let dns_sans = vec!["test.com", "a.test.com", "b.test.com"];
        let issue_data = json!({
            "ttl": "10d",
            "common_name": "test.com",
            "alt_names": "a.test.com,b.test.com",
        })
        .as_object()
        .cloned();

        // issue cert
        let now_timestamp = SystemTime::now().duration_since(UNIX_EPOCH).unwrap().as_secs();
        let resp = test_write_api(&core, token, &format!("{}issue/{}", path, role_name), true, issue_data).await;
        assert!(resp.is_ok());
        let resp_body = resp.unwrap();
        assert!(resp_body.is_some());
        let data = resp_body.unwrap().data;
        assert!(data.is_some());
        let cert_data = data.unwrap();
        let cert = X509::from_pem(cert_data["certificate"].as_str().unwrap().as_bytes()).unwrap();
        let alt_names = cert.subject_alt_names();
        assert!(alt_names.is_some());
        let alt_names = alt_names.unwrap();
        assert_eq!(alt_names.len(), dns_sans.len());
        for alt_name in alt_names {
            assert!(dns_sans.contains(&alt_name.dnsname().unwrap()));
        }
        assert_eq!(cert_data["private_key_type"].as_str().unwrap(), "rsa");
        let priv_key = PKey::private_key_from_pem(cert_data["private_key"].as_str().unwrap().as_bytes()).unwrap();
        assert_eq!(priv_key.bits(), 4096);
        assert!(priv_key.public_eq(&cert.public_key().unwrap()));
        let serial_number = cert.serial_number().to_bn().unwrap();
        let serial_number_hex = serial_number.to_hex_str().unwrap();
        assert_eq!(
            cert_data["serial_number"].as_str().unwrap().replace(":", "").to_lowercase().as_str(),
            serial_number_hex.to_lowercase().as_str()
        );
        let expiration_time = Asn1Time::from_unix(cert_data["expiration"].as_i64().unwrap()).unwrap();
        let ttl_compare = cert.not_after().compare(&expiration_time);
        assert!(ttl_compare.is_ok());
        assert_eq!(ttl_compare.unwrap(), std::cmp::Ordering::Equal);
        let expiration_ttl = cert_data["expiration"].as_u64().unwrap();
        let ttl = expiration_ttl - now_timestamp;
        let expect_ttl = 10 * 24 * 60 * 60;
        println!("ttl: {}, expect_ttl: {}", ttl, expect_ttl);
        //assert!(ttl <= expect_ttl);

        //test fetch cert
        let serial_number_hex = cert_data["serial_number"].as_str().unwrap();
        let resp_lowercase = test_read_api(
            &core,
            token,
            format!("{}cert/{}", path, serial_number_hex.to_lowercase().as_str()).as_str(),
            true,
        )
        .await;
        let resp_uppercase = test_read_api(
            &core,
            token,
            format!("{}cert/{}", path, serial_number_hex.to_uppercase().as_str()).as_str(),
            true,
        )
        .await;
        println!("resp_uppercase: {:?}", resp_uppercase);
        let resp_lowercase_cert_data = resp_lowercase.unwrap().unwrap().data.unwrap();
        let resp_uppercase_cert_data = resp_uppercase.unwrap().unwrap().data.unwrap();
        assert!(resp_lowercase_cert_data.get("private_key").is_none());
        assert!(resp_uppercase_cert_data.get("private_key").is_none());
        assert_eq!(
            resp_lowercase_cert_data["certificate"].as_str().unwrap(),
            resp_uppercase_cert_data["certificate"].as_str().unwrap()
        );
        assert_eq!(
            cert_data["certificate"].as_str().unwrap(),
            resp_uppercase_cert_data["certificate"].as_str().unwrap()
        );
        assert_eq!(
            cert_data["serial_number"].as_str().unwrap(),
            resp_lowercase_cert_data["serial_number"].as_str().unwrap()
        );
    }

    #[maybe_async::test(feature = "sync_handler", async(all(not(feature = "sync_handler")), tokio::test))]
    async fn test_pki_generate_root() {
        let (_rvault, core, root_token) = new_unseal_test_rusty_vault("test_pki_generate_root").await;
        let token = &root_token;
        let path = "pki/";
        let role_name = "test";

        // mount pki backend to path: pki/
        test_mount_api(&core, token, "pki", path).await;

        // config ca
        config_ca(&core, token, path).await;

        // config role
        config_role(&core, token, path, role_name, "rsa", 4096).await;

        generate_root(&core, token, path, true, "rsa", 4096, true).await;
        generate_root(&core, token, path, false, "rsa", 4096, true).await;

        issue_cert_by_generate_root(&core, token, path, role_name, "rsa", 4096).await;

        delete_root(&core, token, path, true).await;
    }

    #[cfg(feature = "crypto_adaptor_tongsuo")]
    #[maybe_async::test(feature = "sync_handler", async(all(not(feature = "sync_handler")), tokio::test))]
    async fn test_pki_sm2_generate_root() {
        let (_rvault, core, root_token) = new_unseal_test_rusty_vault("test_pki_sm2_generate_root").await;
        let token = &root_token;
        let path = "sm2pki/";
        let role_name = "test";

        // mount pki backend to path: pki/
        test_mount_api(&core, token, "pki", path).await;

        // config ca
        config_ca(&core, token, path).await;

        // config role
        config_role(&core, token, path, role_name, "sm2", 256).await;

        generate_root(&core, token, path, true, "sm2", 256, true).await;
        generate_root(&core, token, path, false, "sm2", 256, true).await;

        issue_cert_by_generate_root(&core, token, path, role_name, "sm2", 256).await;

        delete_root(&core, token, path, true).await;
    }

    #[maybe_async::maybe_async]
    async fn test_pki_generate_key_case(
        core: &Core,
        token: &str,
        path: &str,
        key_name: &str,
        key_type: &str,
        key_bits: u32,
        exported: bool,
        is_ok: bool,
    ) {
        let req_data = json!({
            "key_name": key_name.to_string(),
            "key_type": key_type.to_string(),
            "key_bits": key_bits,
        })
        .as_object()
        .cloned();
        println!("generate req_data: {:?}, is_ok: {}", req_data, is_ok);
        let resp = test_write_api(
            core,
            token,
            &format!("{}/keys/generate/{}", path, if exported { "exported" } else { "internal" }),
            is_ok,
            req_data,
        )
        .await;
        if !is_ok {
            return;
        }
        let resp_body = resp.unwrap();
        assert!(resp_body.is_some());
        let data = resp_body.unwrap().data;
        assert!(data.is_some());
        let key_data = data.unwrap();
        println!("generate key result: {:?}", key_data);
        assert_eq!(key_data["key_name"].as_str().unwrap(), key_name);
        assert_eq!(key_data["key_type"].as_str().unwrap(), key_type);
        assert_eq!(key_data["key_bits"].as_u64().unwrap(), key_bits as u64);
        if exported {
            assert!(key_data["private_key"].as_str().is_some());
            let private_key_pem = key_data["private_key"].as_str().unwrap();
            match key_type {
                "rsa" => {
                    let rsa_key = Rsa::private_key_from_pem(private_key_pem.as_bytes());
                    assert!(rsa_key.is_ok());
                    assert_eq!(rsa_key.unwrap().size() * 8, key_bits);
                }
                "ec" => {
                    let ec_key = EcKey::private_key_from_pem(private_key_pem.as_bytes());
                    assert!(ec_key.is_ok());
                    assert_eq!(ec_key.unwrap().group().degree(), key_bits);
                }
                "aes-gcm" | "aes-cbc" | "aes-ecb" => {
                    let aes_key = hex::decode(private_key_pem.as_bytes());
                    assert!(aes_key.is_ok());
                    assert_eq!(aes_key.unwrap().len() as u32 * 8, key_bits);
                }
                _ => {}
            }
        } else {
            assert!(key_data.get("private_key").is_none());
        }
    }

    #[maybe_async::maybe_async]
    async fn test_pki_import_key_case(
        core: &Core,
        token: &str,
        path: &str,
        key_name: &str,
        key_type: &str,
        key_bits: u32,
        iv: &str,
        data: &str,
        is_ok: bool,
    ) {
        let mut req_data = json!({
            "key_name": key_name.to_string(),
            "key_type": key_type.to_string(),
        })
        .as_object()
        .unwrap()
        .clone();

        match key_type {
            "rsa" | "ec" => {
                req_data.insert("pem_bundle".to_string(), Value::String(data.to_string()));
            }
            "aes-gcm" | "aes-cbc" | "aes-ecb" => {
                req_data.insert("hex_bundle".to_string(), Value::String(data.to_string()));
                req_data.insert("iv".to_string(), Value::String(iv.to_string()));
            }
            _ => {}
        }

        println!("import req_data: {:?}, is_ok: {}", req_data, is_ok);
        let resp = test_write_api(core, token, &format!("{}/keys/import", path), is_ok, Some(req_data)).await;
        if !is_ok {
            return;
        }
        let resp_body = resp.unwrap();
        assert!(resp_body.is_some());
        let data = resp_body.unwrap().data;
        assert!(data.is_some());
        let key_data = data.unwrap();
        println!("import key result: {:?}", key_data);
        assert_eq!(key_data["key_name"].as_str().unwrap(), key_name);
        assert_eq!(key_data["key_type"].as_str().unwrap(), key_type);
        assert_eq!(key_data["key_bits"].as_u64().unwrap(), key_bits as u64);
    }

    #[maybe_async::maybe_async]
    async fn test_pki_sign_verify(core: &Core, token: &str, path: &str, key_name: &str, data: &[u8], is_ok: bool) {
        let req_data = json!({
            "key_name": key_name.to_string(),
            "data": hex::encode(data),
        })
        .as_object()
        .cloned();
        println!("sign req_data: {:?}, is_ok: {}", req_data, is_ok);
        let resp = test_write_api(core, token, &format!("{}/keys/sign", path), is_ok, req_data).await;
        if !is_ok {
            return;
        }
        let resp_body = resp.unwrap();
        assert!(resp_body.is_some());
        let resp_raw_data = resp_body.unwrap().data;
        assert!(resp_raw_data.is_some());
        let resp_data = resp_raw_data.unwrap();
        println!("sign resp_data: {:?}", resp_data);

        let signature = resp_data["result"].as_str().unwrap();

        let req_data = json!({
            "key_name": key_name.to_string(),
            "data": hex::encode(data),
            "signature": signature,
        })
        .as_object()
        .cloned();
        println!("verify req_data: {:?}, is_ok: {}", req_data, is_ok);
        let resp = test_write_api(core, token, &format!("{}/keys/verify", path), is_ok, req_data).await;
        let resp_body = resp.unwrap();
        assert!(resp_body.is_some());
        let resp_raw_data = resp_body.unwrap().data;
        assert!(resp_raw_data.is_some());
        let resp_data = resp_raw_data.unwrap();
        println!("verify resp_data: {:?}", resp_data);
        assert_eq!(resp_data["result"].as_bool().unwrap(), true);

        //test bad data
        let req_data = json!({
            "key_name": key_name.to_string(),
            "data": hex::encode("bad data".as_bytes()),
            "signature": signature,
        })
        .as_object()
        .cloned();
        println!("verify bad req_data: {:?}, is_ok: {}", req_data, is_ok);
        let resp = test_write_api(core, token, &format!("{}/keys/verify", path), true, req_data).await;
        let resp_body = resp.unwrap();
        assert!(resp_body.is_some());
        let resp_raw_data = resp_body.unwrap().data;
        assert!(resp_raw_data.is_some());
        let resp_data = resp_raw_data.unwrap();
        assert_eq!(resp_data["result"].as_bool().unwrap(), false);

        //test bad signature
        let req_data = json!({
            "key_name": key_name.to_string(),
            "data": hex::encode(data),
            "signature": signature[2..],
        })
        .as_object()
        .cloned();
        println!("verify bad signatue req_data: {:?}, is_ok: {}", req_data, is_ok);
        let resp = test_write_api(core, token, &format!("{}/keys/verify", path), true, req_data).await;
        let resp_body = resp.unwrap();
        assert!(resp_body.is_some());
        let resp_raw_data = resp_body.unwrap().data;
        assert!(resp_raw_data.is_some());
        let resp_data = resp_raw_data.unwrap();
        assert_eq!(resp_data["result"].as_bool().unwrap(), false);

        //test bad signature len
        let req_data = json!({
            "key_name": key_name.to_string(),
            "data": hex::encode(data),
            "signature": signature[1..],
        })
        .as_object()
        .cloned();
        let resp = test_write_api(core, token, &format!("{}/keys/verify", path), false, req_data).await;
        assert!(resp.is_err());
    }

    #[maybe_async::maybe_async]
    async fn test_pki_encrypt_decrypt(core: &Core, token: &str, path: &str, key_name: &str, data: &[u8], is_ok: bool) {
        let origin_data = hex::encode(data);
        let req_data = json!({
            "key_name": key_name.to_string(),
            "data": origin_data.clone(),
        })
        .as_object()
        .cloned();
        println!("encrypt req_data: {:?}, is_ok: {}", req_data, is_ok);
        let resp = test_write_api(core, token, &format!("{}/keys/encrypt", path), is_ok, req_data).await;
        if !is_ok {
            return;
        }
        let resp_body = resp.unwrap();
        assert!(resp_body.is_some());
        let resp_raw_data = resp_body.unwrap().data;
        assert!(resp_raw_data.is_some());
        let resp_data = resp_raw_data.unwrap();
        println!("encrypt resp_data: {:?}", resp_data);

        let encrypted_data = resp_data["result"].as_str().unwrap();

        let req_data = json!({
            "key_name": key_name.to_string(),
            "data": encrypted_data,
        })
        .as_object()
        .cloned();
        println!("decrypt req_data: {:?}, is_ok: {}", req_data, is_ok);
        let resp = test_write_api(core, token, &format!("{}/keys/decrypt", path), is_ok, req_data).await;
        let resp_body = resp.unwrap();
        assert!(resp_body.is_some());
        let resp_raw_data = resp_body.unwrap().data;
        assert!(resp_raw_data.is_some());
        let resp_data = resp_raw_data.unwrap();
        println!("decrypt resp_data: {:?}", resp_data);
        assert_eq!(resp_data["result"].as_str().unwrap(), origin_data.as_str());

        //test bad data
        let req_data = json!({
            "key_name": key_name.to_string(),
            "data": encrypted_data[1..],
        })
        .as_object()
        .cloned();
        let resp = test_write_api(core, token, &format!("{}/keys/decrypt", path), false, req_data).await;
        assert!(resp.is_err());
    }

    #[maybe_async::test(feature = "sync_handler", async(all(not(feature = "sync_handler")), tokio::test))]
    async fn test_pki_generate_key() {
        let (_rvault, core, root_token) = new_unseal_test_rusty_vault("test_pki_generate_key").await;
        let token = &root_token;
        let path = "pki";

        // mount pki backend to path: pki/
        test_mount_api(&core, token, "pki", path).await;

        //test generate rsa key
        test_pki_generate_key_case(&core, token, path, "rsa-2048", "rsa", 2048, true, true).await;
        test_pki_generate_key_case(&core, token, path, "rsa-3072", "rsa", 3072, true, true).await;
        test_pki_generate_key_case(&core, token, path, "rsa-4096", "rsa", 4096, true, true).await;
        test_pki_generate_key_case(&core, token, path, "rsa-2048-internal", "rsa", 2048, false, true).await;
        test_pki_generate_key_case(&core, token, path, "rsa-3072-internal", "rsa", 3072, false, true).await;
        test_pki_generate_key_case(&core, token, path, "rsa-4096-internal", "rsa", 4096, false, true).await;
        test_pki_generate_key_case(&core, token, path, "rsa-2048", "rsa", 2048, true, false).await;
        test_pki_generate_key_case(&core, token, path, "rsa-2048-bad-type", "rsaa", 2048, true, false).await;
        test_pki_generate_key_case(&core, token, path, "rsa-2048-bad-bits", "rsa", 2049, true, false).await;

        //test rsa sign and verify
        test_pki_sign_verify(&core, token, path, "rsa-2048", "rusty_vault test".as_bytes(), true).await;
        test_pki_sign_verify(&core, token, path, "rsa-3072", "rusty_vault test".as_bytes(), true).await;
        test_pki_sign_verify(&core, token, path, "rsa-4096", "rusty_vault test".as_bytes(), true).await;
        test_pki_sign_verify(&core, token, path, "rsa-4096-bad-key-name", "rusty_vault test".as_bytes(), false).await;

        //test generate ec key
        test_pki_generate_key_case(&core, token, path, "ec-224", "ec", 224, true, true).await;
        test_pki_generate_key_case(&core, token, path, "ec-256", "ec", 256, true, true).await;
        test_pki_generate_key_case(&core, token, path, "ec-384", "ec", 384, true, true).await;
        test_pki_generate_key_case(&core, token, path, "ec-521", "ec", 521, true, true).await;
        test_pki_generate_key_case(&core, token, path, "ec-224-internal", "ec", 224, false, true).await;
        test_pki_generate_key_case(&core, token, path, "ec-256-internal", "ec", 256, false, true).await;
        test_pki_generate_key_case(&core, token, path, "ec-384-internal", "ec", 384, false, true).await;
        test_pki_generate_key_case(&core, token, path, "ec-521-internal", "ec", 521, false, true).await;
        test_pki_generate_key_case(&core, token, path, "ec-224", "ec", 224, true, false).await;
        test_pki_generate_key_case(&core, token, path, "ec-224-bad_type", "ecc", 224, true, false).await;
        test_pki_generate_key_case(&core, token, path, "ec-224-bad_bits", "ec", 250, true, false).await;

        //test ec sign and verify
        test_pki_sign_verify(&core, token, path, "ec-224", "rusty_vault test".as_bytes(), true).await;
        test_pki_sign_verify(&core, token, path, "ec-256", "rusty_vault test".as_bytes(), true).await;
        test_pki_sign_verify(&core, token, path, "ec-384", "rusty_vault test".as_bytes(), true).await;
        test_pki_sign_verify(&core, token, path, "ec-521", "rusty_vault test".as_bytes(), true).await;
        test_pki_sign_verify(&core, token, path, "ec-224-bad-key-name", "rusty_vault test".as_bytes(), false).await;

        //test generate aes-gcm key
        test_pki_generate_key_case(&core, token, path, "aes-gcm-128", "aes-gcm", 128, true, true).await;
        test_pki_generate_key_case(&core, token, path, "aes-gcm-192", "aes-gcm", 192, true, true).await;
        test_pki_generate_key_case(&core, token, path, "aes-gcm-256", "aes-gcm", 256, true, true).await;
        test_pki_generate_key_case(&core, token, path, "aes-gcm-128-internal", "aes-gcm", 128, false, true).await;
        test_pki_generate_key_case(&core, token, path, "aes-gcm-192-internal", "aes-gcm", 192, false, true).await;
        test_pki_generate_key_case(&core, token, path, "aes-gcm-256-internal", "aes-gcm", 256, false, true).await;
        test_pki_generate_key_case(&core, token, path, "aes-gcm-128", "aes-gcm", 128, true, false).await;
        test_pki_generate_key_case(&core, token, path, "aes-gcm-128-bad-type", "aes-gcmm", 128, true, false).await;
        test_pki_generate_key_case(&core, token, path, "aes-gcm-128-bad-bits", "aes-gcm", 129, true, false).await;

        //test aes-gcm encrypt and decrypt
        test_pki_encrypt_decrypt(&core, token, path, "aes-gcm-128", "rusty_vault test".as_bytes(), true).await;
        test_pki_encrypt_decrypt(&core, token, path, "aes-gcm-192", "rusty_vault test".as_bytes(), true).await;
        test_pki_encrypt_decrypt(&core, token, path, "aes-gcm-256", "rusty_vault test".as_bytes(), true).await;
        test_pki_encrypt_decrypt(&core, token, path, "aes-gcm-256-bad-key-name", "rusty_vault test".as_bytes(), false)
            .await;

        //test generate aes-cbc key
        test_pki_generate_key_case(&core, token, path, "aes-cbc-128", "aes-cbc", 128, true, true).await;
        test_pki_generate_key_case(&core, token, path, "aes-cbc-192", "aes-cbc", 192, true, true).await;
        test_pki_generate_key_case(&core, token, path, "aes-cbc-256", "aes-cbc", 256, true, true).await;
        test_pki_generate_key_case(&core, token, path, "aes-cbc-128-internal", "aes-cbc", 128, false, true).await;
        test_pki_generate_key_case(&core, token, path, "aes-cbc-192-internal", "aes-cbc", 192, false, true).await;
        test_pki_generate_key_case(&core, token, path, "aes-cbc-256-internal", "aes-cbc", 256, false, true).await;
        test_pki_generate_key_case(&core, token, path, "aes-cbc-128", "aes-cbc", 128, true, false).await;
        test_pki_generate_key_case(&core, token, path, "aes-cbc-128-bad-type", "aes-cbcc", 128, true, false).await;
        test_pki_generate_key_case(&core, token, path, "aes-cbc-128-bad-bits", "aes-cbc", 129, true, false).await;

        //test aes-cbc encrypt and decrypt
        test_pki_encrypt_decrypt(&core, token, path, "aes-cbc-128", "rusty_vault test".as_bytes(), true).await;
        test_pki_encrypt_decrypt(&core, token, path, "aes-cbc-192", "rusty_vault test".as_bytes(), true).await;
        test_pki_encrypt_decrypt(&core, token, path, "aes-cbc-256", "rusty_vault test".as_bytes(), true).await;
        test_pki_encrypt_decrypt(&core, token, path, "aes-cbc-256-bad-key-name", "rusty_vault test".as_bytes(), false)
            .await;

        //test generate aes-ecb key
        test_pki_generate_key_case(&core, token, path, "aes-ecb-128", "aes-ecb", 128, true, true).await;
        test_pki_generate_key_case(&core, token, path, "aes-ecb-192", "aes-ecb", 192, true, true).await;
        test_pki_generate_key_case(&core, token, path, "aes-ecb-256", "aes-ecb", 256, true, true).await;
        test_pki_generate_key_case(&core, token, path, "aes-ecb-128-internal", "aes-ecb", 128, false, true).await;
        test_pki_generate_key_case(&core, token, path, "aes-ecb-192-internal", "aes-ecb", 192, false, true).await;
        test_pki_generate_key_case(&core, token, path, "aes-ecb-256-internal", "aes-ecb", 256, false, true).await;
        test_pki_generate_key_case(&core, token, path, "aes-ecb-128", "aes-ecb", 128, true, false).await;
        test_pki_generate_key_case(&core, token, path, "aes-ecb-128-bad-type", "aes-ecbb", 128, true, false).await;
        test_pki_generate_key_case(&core, token, path, "aes-ecb-128-bad-bits", "aes-ecb", 129, true, false).await;

        //test aes-ecb encrypt and decrypt
        test_pki_encrypt_decrypt(&core, token, path, "aes-ecb-128", "rusty_vault test".as_bytes(), true).await;
        test_pki_encrypt_decrypt(&core, token, path, "aes-ecb-192", "rusty_vault test".as_bytes(), true).await;
        test_pki_encrypt_decrypt(&core, token, path, "aes-ecb-256", "rusty_vault test".as_bytes(), true).await;
        test_pki_encrypt_decrypt(&core, token, path, "aes-ecb-256-bad-key-name", "rusty_vault test".as_bytes(), false)
            .await;
    }

    #[maybe_async::test(feature = "sync_handler", async(all(not(feature = "sync_handler")), tokio::test))]
    async fn test_pki_import_key() {
        let (_rvault, core, root_token) = new_unseal_test_rusty_vault("test_pki_import_key").await;
        let token = &root_token;
        let path = "pki";

        // mount pki backend to path: pki/
        test_mount_api(&core, token, "pki", path).await;

        //test import rsa key
        test_pki_import_key_case(
            &core,
            token,
            path,
            "rsa-2048-import",
            "rsa",
            2048,
            "",
            r#"
-----BEGIN PRIVATE KEY-----
MIIEvgIBADANBgkqhkiG9w0BAQEFAASCBKgwggSkAgEAAoIBAQC/K85aluWn5Pkl
HZ5Ye59UkY7S5dLLBSOXpVyECniTpYi08jK0mwP3D+xqgDvS7OBEK2OVO+eUSWCe
tNHGA/u00HeeADVTNaZK7f2+1KQPkmernOecRU0xbl59ScSOzDDYXMKIhoRs6Neu
qw+jRTuW0t9/UOmni1pN+w9i5z9Lmz0qMsSaPDoy1JqajZoTyzJz30ftN/kEg75T
uhwczIzyKPib/IzvsgoPq6ZtVFx9hVEU6SkaKu3jLrxEIpwAROn0fIrcNuOE+VxY
tpGrBFheD0qbEqOLtMgUYAMWWG86tqWBOBxEnRSEmxhPDLqhu6a4yfBKtwL1JA1e
PeuiEEKJAgMBAAECggEAFHIZx0bajVrSqf1hc+LWLGQcQNezSY2lUVuDqgbj/3KA
TPiW+LRC4ne8WBBFQFlKNlrnncyC3Nv+LpXLK7Y9rjMaNUvzaBCrANo0PbvInMu9
NQr6cGmvCFQ0BzVOWtwMIKUcacqX5if+9/Tenskm8YoLEjbz+RHRLi7lkIqH5/d6
lIJAss5Q/u3D9uTP0ngmztG65IV0vHacn0S3zyOZ7DD+MJwk4GUpYxTtgkFIzuDH
aQgkYcjJeNNWcOesEHs0u1Nqt9GlPyScde/jcblNPMdkBuu1vP0gxjCNdRVu9ZE5
x7V9w2buKFwPIS+Hpv35t/0qvcoYDq1Vg1wj6VUVewKBgQDgv0pq1gwkvcZCttEb
EIitqlQ2y0HH7TdiMB317U2wmLwPmVQ2b1gTD+cHNWE9y1F9rSVeDUfcizm9qvDk
kjNOAfXRt5aFi2a03DKlGY57k6o9sp3qqvESEoryzUOUTUvYe9S7nXZ7B8/Pv0OE
2yyEiCg4XtHTRYPLMqbGp359OwKBgQDZwT/ahzYM7RZex9i3BHpuqs6m9ig7W2oM
7H1Qd4FOOa1lnnq5+/CXDH258OmqANvie/wcD/eQ/tvKIrUfm6DRBvSul2Bbae5F
GJxLttPFqxCiGgWhPW4EWdFgHXCTmMd3gOByklfw1dMZkjor2kJJSi8kPvfWUKgM
oCyZ7aiTCwKBgFmnFSl/D0MMzOzJ/qocM1mLi6J7/FajYydw6FK1AfvDQam7UWOR
kQGxo0g12/+Jfo1yp2hYReVNSJBHg2a6h2rDz2qEILBPBn55JF7FzhevtQZ9nQ8C
d73s1a67gQzEtM+7vgXFb4DugdBujKGPyLdplRm/gVYU8dj58JtoL0YHAoGBAMyi
QvOGJVE4bNFdVVeIqdXeRp24rk45tgu3InzAEZAFu+HHcOXe0VXhszVOJQhSDlFk
2qM0jh6AouPuge/WPOaydrasIy1E+1mLqzWr9o/IFrV/ZtMD+6OzFIQSpnzOEoVH
Y6XHyUTWbK+XL3uOfMSLJooVcqrA2WwkCkYNhWHJAoGBANRT1KPQP0+Tlc+8FoGa
q2Kt71bpNXUzj5Vi/Ikgqm0z943hAvBKIvxY2SPdybvSxk9YeUXhB88cApdepRzc
4hNAvCtpiAQHbH5P9dpDXx6xbr1kT/z5iKe3VzxnEyLlm6yEItoq1k0ZvpyQO+W4
bwtnhIcuKu7aG0qI2abuLtNI
-----END PRIVATE KEY-----
"#,
            true,
        )
        .await;
        test_pki_import_key_case(
            &core,
            token,
            path,
            "rsa-3072-import",
            "rsa",
            3072,
            "",
            r#"
-----BEGIN PRIVATE KEY-----
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
Do5o32GEcxbLo+woXez/9og=
-----END PRIVATE KEY-----
"#,
            true,
        )
        .await;
        test_pki_import_key_case(
            &core,
            token,
            path,
            "rsa-4096-import",
            "rsa",
            4096,
            "",
            r#"
-----BEGIN PRIVATE KEY-----
MIIJQwIBADANBgkqhkiG9w0BAQEFAASCCS0wggkpAgEAAoICAQDonanogPRAGLwJ
Bg1EWDgaqJlQLnb7TrvtBzKCLjtw6ssi/1Q1Oldsw6+QEcDOUl5Z+p2SgyT9Ciq+
eXZQ31TxSppmjyQ1E7xsUk33+TBt8Clcw7sXG0MT7CI+q4s356bJ5IF7O5Sz38Lj
RC2svmE6ptWO+lGtZZTUM4nKs6TWOo/uwfg1OzmwIbFhZsy0l1cF6DO8XKI0L2I8
MjNCZ6mUh7fYfREuXzX/rnZI8Kc4cQWaGfxHGXkXzmW0WQ3K7EfdXgdP9xAeTmvJ
sSWlUVsrxo3+IHGjxExrAjB7xLCl778Nff+MVErt+4aveWocrNHOyXgfYHijyjxl
x0BbjNmPiiBmtaApE950VNzmaoj7B+OrC4SY2yxVjnSOoGo0i08wJIIwYVZQBSwI
yFECMzQupSQfo7/AeIhtXzKRtVHgI6M08IqRIiirqA0x62HQmFmjny3cWhUSHSaJ
fcO0YLThCIjbyw8HtfMyHH3whFrX+hOvgEpC5yabwoqs5PseEAJ7vkOrsfo5/61R
kRrwqonXD68CBwRXWwuGWlxPGRjBt9EMRh7tPTAQD5v5u4ZI1jDsAF24pIPMDAQs
AhqahX+5zsNgIaY2YIBhMcu90eCqqUc9oHQ3l5jOYoGyfI58Vs3N5TEyGYPCu2Es
FJKXNoU8NhH77Y+yWSkxCA20MB6lsQIDAQABAoICABoREOktCjHkk62vL+1otWDH
Y+bITiz7NbPqCo7r/1038muJkTYlw4Y8p3/NfX2hXVFX9DBm4c45RXDyy39kh3BC
m+rCP5xzCbpZvsL6+aJYN0pd5KlCNNIWs/+x2Xf6TYZyRNA6bP97I6u0CCpDa0HX
UDcldeGocHUXEWnVl7Mp/NcUhWvxpxVFsUro6ieSjf3rd2C0QLj4VlnIhlX6p9Yt
HzzzRumbYcG1GywxS4vXnnkWUF7nS9qPFnaPRCxpLeRwlQEw/m1m/E0tvLo5062k
ImVH3XQsmyOiqywzblgp9Y7df7WJ/JuOhBlF0c5Ez34MtZlOhjZUg1Akc+HOdtKY
RHPBk7Ixtg/PHRK/+nS/+7JqQ6SlDdwq6yarm0nghwcgmp+xC9Z4PgUpXO0RCoMi
zwMSKtvlH1C8+dtaVIocPco9SRINV8WeiLcIM6IRkdvS9O+VqgBvjs/79r4iulqZ
CroRUwaFVzrwJ/HDSIJMJDINdBnknPoETCW5fJKw7nO+sjp2W95Y8gtUT/Z2zv+u
Ph5yUvFIuf9Wetl9PxAd4XkWZXUzSNKpxMbOCA1PofpLL3i9AB4cYSGpiUPFFoHO
T/j2bEbswxGARzPe20JCmufqf22c3z8ulgusVLS67ds7vh6m29mvZqyi6uvmF5QB
17Ji53b/lHrLJg/kkwjBAoIBAQD4iusLH+KhrE34f/FjkUQOMXqXy9gYZPlS3PpY
frYdvLw6K2NXb15fV+OfwH7cdNBw5kQtx4odasw2cKxcbgyS4eoQLWnxLCTWQuOo
RnGEvQWnUefccMWXsjdmvJQlbCB0WhWGgVorEGrN2W3d4vaVA6zahSQ7m8GvT5wz
1h6ahQylOhAzAzdpewymET5QlAsyX54pAjTAUOXQzbxTabbPNli0mVa1xi/a1LKv
J0GngUP/rXFWAvnDjbZsfsyRa5947HRt5yvwGgSj+3/8q6CMlSA8IjRgFVEJAtUS
t/OkLBzXZ7AdRgD1EzSpI3YXFdvkMgMJQQxr5qmRsSshP7RXAoIBAQDvmGkM9dNd
RZ4X3tgguIaldm2YUir8I1gFy2SjUe0BVxDuLqI+fCebE0J2BEwxrEJpJskDtMtF
h/PVSSKNUhpe12A98qYhlczfyqtfLfnyAsX1C0YbtEK041NjM/NIX7Zr1xoQyRT9
TL0CsLavl3oNRZ2j4d2rTYyBcn1A9blFfCfOppG3E0pd2f+JmTdp8ap/ITas4Rpj
rYSiTMMDS0zT6agjsur/8/yPbgUjRzU02JUjfEXBpT6bCKdom+X+UTacnBri+JRx
Kr6ZOPNxZzJX6hRHnrJ5b4x75JKYHeMQPaRy9/6+hj5QnC/5ZJLdgFBf986xNcM9
uzIkWD8Q//E3AoIBAQCp/pI+/KMNA4Yc9p2LBXuv3YXFN2ZpYP7q/zu3tYsokcOI
Yc7Dqee5fKqyxH3AmaFL5yMw0K8V6csdXTl2ysqM2Do4sGcqzo+vgPanTO8t4/9u
7uWQcA2l8P5VpZwKcIdOLaNVaTncBJGYlCPCRQ904puiprgekS0LlH75MXWjKGd6
x1j3GzcWTVRcbaTahjeWT7IkyF5+P5bAl0c9IiwoVDqd49db4t8uZJaGmGoegJqa
0O2Y79YXO+FPGfcfa6YallgYJ6p0wcb0xftHPbhFD2aJ2rdKFKplaGuGLw1U99sO
NdxOWWgkN+un2BpYNdo9nTtYZAZz8sN+Y9hlGGZnAoIBAAGqxdBZRYc4nMj9u/M+
VXGBSXHt4G6wsEQaDjE0uLlxqaR+npJgusXRdHzturn9wNiKdIuaDnAFaiTpxVvG
Xniadwj3T0CckjhcaKTY5QxSCJ6T3YED2BL6IfJmwiKWx+YoMP485/B7QDVslVjT
bP36pgYl5Cz09S1RZp21F/zryDsf3ZOwhqvwgF6suj36eH059e9uAYkABBQ9BH5Z
X8d5sLnO3OO7Bt7YnSCJtk0P1LnSe4nFZJIflUqdCxSh7Ada7rT1ldLTwU+/nbIE
Tc1ey5VT/Vnq9MdH5903GAVc6HAEWblppbVZ4NuTX5I6+lQwnTeOcDVVwBuQoZ+0
qDECggEBAIwdjxe5PVCk4dNZh5tiGta+IEeiF7nTrqFJrlGYQ5HUPUcD8chupAAB
LdHdzlsJUyCsqAHUxk74CYPMmG2w4+Lyr8siTfDvTW5f9Q43vGAGzRF0oGyQNBHv
VTNPCI2QzRBXneLn+hWcDda3Lgg0IMpPQEXJKOak3kOFnboSwvsN8aP2/LrLBKaV
V6B7Y5GlEch1VTZMb8tyAeLa1PIFFGoJb7mfiZqIfRqrRbQ9kzVBzyeiHAc06VvJ
CMWOmQT9pmXTLLmS4KDU+ktQao+U+LXvgYzhzFo9KqkVeNkifppVFZBW5sC/DQbd
srlAra2xKovU8At81EhC3oarMYLbY9w=
-----END PRIVATE KEY-----
"#,
            true,
        )
        .await;
        test_pki_import_key_case(
            &core,
            token,
            path,
            "rsa-4096-import",
            "rsa",
            4096,
            "",
            r#"
-----BEGIN PRIVATE KEY-----
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
srlAra2xKovU8At81EhC3oarMYLbY9w=
-----END PRIVATE KEY-----
"#,
            false,
        )
        .await;
        test_pki_import_key_case(
            &core,
            token,
            path,
            "rsa-4096-import-bad-type",
            "rsaa",
            4096,
            "",
            r#"
-----BEGIN PRIVATE KEY-----
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
srlAra2xKovU8At81EhC3oarMYLbY9w=
-----END PRIVATE KEY-----
"#,
            false,
        )
        .await;
        test_pki_import_key_case(
            &core,
            token,
            path,
            "rsa-4096-import-bad-pem",
            "rsaa",
            4096,
            "",
            r#"
-----BEGIN PRIVATE KEY-----
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
srlAra2xKovU8At81EhC3oarMYLbY9w=
-----END PRIVATE KEY-----
"#,
            false,
        )
        .await;

        //test rsa sign and verify
        test_pki_sign_verify(&core, token, path, "rsa-2048-import", "rusty_vault test".as_bytes(), true).await;
        test_pki_sign_verify(&core, token, path, "rsa-3072-import", "rusty_vault test".as_bytes(), true).await;
        test_pki_sign_verify(&core, token, path, "rsa-4096-import", "rusty_vault test".as_bytes(), true).await;

        //test import ec key
        test_pki_import_key_case(
            &core,
            token,
            path,
            "ec-224-import",
            "ec",
            224,
            "",
            r#"
-----BEGIN PRIVATE KEY-----
MHgCAQAwEAYHKoZIzj0CAQYFK4EEACEEYTBfAgEBBBzsiBoYW2wy95WsH51cIW90
l5jP3LyA//F/qHE5oTwDOgAEasjtLNpFz6+08WsxkDppMANKXPfaiIzvSfLMFIZU
K9bNL/xrK2WENeATjX1eZE9JZtjDwnAqlJM=
-----END PRIVATE KEY-----
                                "#,
            true,
        )
        .await;
        test_pki_import_key_case(
            &core,
            token,
            path,
            "ec-256-import",
            "ec",
            256,
            "",
            r#"
-----BEGIN PRIVATE KEY-----
MIGEAgEAMBAGByqGSM49AgEGBSuBBAAKBG0wawIBAQQgfSJ3DnUokwFD0QtnEE1f
e0Y20qDAjcYbwFwkWBkWcy+hRANCAATKrAXdOc0ufhMk8225jX+C9a/WfjNIp7lu
AAOYNTNA2jpy34lQ2zlBLIoaTuxXtg6mWvfITYPGrpWorcPTYzG+
-----END PRIVATE KEY-----
"#,
            true,
        )
        .await;
        test_pki_import_key_case(
            &core,
            token,
            path,
            "ec-384-import",
            "ec",
            384,
            "",
            r#"
-----BEGIN PRIVATE KEY-----
MIG2AgEAMBAGByqGSM49AgEGBSuBBAAiBIGeMIGbAgEBBDDY0x5JtPPUfipvnd7P
C6vZfNzkyBRCiwzGbFY1MH39ZC4TfNx0t5SiADPDNv4g1y6hZANiAASMgIt8fVVY
TKSYqB3QPPoSWhfvlq1iSdarRYfH+6S9dRpeaf+xnnVVMD8iqmUBOdl0UZZHOOt6
+JJpUl0cZF9t6E92N4SaXaFI3ZLzYziaMZU1MSTWJZyJvi3vswqHEYU=
-----END PRIVATE KEY-----
"#,
            true,
        )
        .await;
        test_pki_import_key_case(
            &core,
            token,
            path,
            "ec-521-import",
            "ec",
            521,
            "",
            r#"
-----BEGIN PRIVATE KEY-----
MIHuAgEAMBAGByqGSM49AgEGBSuBBAAjBIHWMIHTAgEBBEIA/PzzOHksK0r/Z/82
43IOFCYFjhOL08+cFGElJjHfubjTGhzr1jDHwwnMnd7LAOk+M9395uJjwXMrW5GN
qWeY8cWhgYkDgYYABAG44vWoqZdKP+nGTDNcmK2phS9/TWfHrCqxJAckyINLYwuE
UdkF6MbAwJJOPnBntqZOt83iUtFKUWxy0iFPQVn49QHP/yT+G/cz3qjx7TkFP+4W
jmQbXbxLGIvSIZoscho/LSWyyct4CBPbPplopiMTgDN1MA7mFvT2TYAxFJA0rVWk
Fw==
-----END PRIVATE KEY-----
"#,
            true,
        )
        .await;
        test_pki_import_key_case(&core, token, path, "ec-521-import", "ec", 521, "", "same key name", false).await;
        test_pki_import_key_case(
            &core,
            token,
            path,
            "ec-521-import-bad-type",
            "ecc",
            521,
            "",
            r#"
-----BEGIN PRIVATE KEY-----
MIHuAgEAMBAGByqGSM49AgEGBSuBBAAjBIHWMIHTAgEBBEIA/PzzOHksK0r/Z/82
43IOFCYFjhOL08+cFGElJjHfubjTGhzr1jDHwwnMnd7LAOk+M9395uJjwXMrW5GN
qWeY8cWhgYkDgYYABAG44vWoqZdKP+nGTDNcmK2phS9/TWfHrCqxJAckyINLYwuE
UdkF6MbAwJJOPnBntqZOt83iUtFKUWxy0iFPQVn49QHP/yT+G/cz3qjx7TkFP+4W
jmQbXbxLGIvSIZoscho/LSWyyct4CBPbPplopiMTgDN1MA7mFvT2TYAxFJA0rVWk
Fw==
-----END PRIVATE KEY-----
"#,
            false,
        )
        .await;
        test_pki_import_key_case(
            &core,
            token,
            path,
            "ec-521-import-bad-pem",
            "ec",
            521,
            "",
            r#"
-----BEGIN PRIVATE KEY-----
MIHuAgEAMBAGByqGSM49AgEGBSuBBAAjBIHWMIHTAgEBBEIA/PzzOHksK0r/Z/82
43IOFCYFjhOL08+cFGElJjHfubjTGhzr1jDHwwnMnd7LAOk+M9395uJjwXMrW5GN
qWeY8cWhgYkDgYYABAG44vWoqZdKP+nGTDNcmK2phS9/TWfHrCqxJAckyINLYwuE
UdkF6MbAwJJOPnBntqZOt83iUtFKUWxy0iFPQVn49QHP/yT+G/cz3qjx7TkFP+4W
jmQbXbxLGIvSIZoscho/LSWyyct4CBPbPplopiMTgDN1MA7mFvT2TYAxFJA0rVWkaabb
Fw==
xxxxxxxxxxxxxx
-----END PRIVATE KEY-----
        "#,
            false,
        )
        .await;

        //test ec sign and verify
        test_pki_sign_verify(&core, token, path, "ec-224-import", "rusty_vault test".as_bytes(), true).await;
        test_pki_sign_verify(&core, token, path, "ec-256-import", "rusty_vault test".as_bytes(), true).await;
        test_pki_sign_verify(&core, token, path, "ec-384-import", "rusty_vault test".as_bytes(), true).await;
        test_pki_sign_verify(&core, token, path, "ec-521-import", "rusty_vault test".as_bytes(), true).await;

        //test import aes-gcm key
        test_pki_import_key_case(
            &core,
            token,
            path,
            "aes-gcm-128-import",
            "aes-gcm",
            128,
            "1c499088cddd0382918bd5650718533d",
            "cfe0f571fe695c6a4c5e34339d32eb3c",
            true,
        )
        .await;
        test_pki_import_key_case(
            &core,
            token,
            path,
            "aes-gcm-192-import",
            "aes-gcm",
            192,
            "1c499088cddd0382918bd5650718533d",
            "3077fdca16350c85c354a700bbc127972dafe2138874cdea",
            true,
        )
        .await;
        test_pki_import_key_case(
            &core,
            token,
            path,
            "aes-gcm-256-import",
            "aes-gcm",
            256,
            "1c499088cddd0382918bd5650718533d",
            "6349e3032b690f2fe61a824746ac3ab05c1829a4147f4891f595dfb19cddfd06",
            true,
        )
        .await;
        test_pki_import_key_case(
            &core,
            token,
            path,
            "aes-gcm-256-import",
            "aes-gcm",
            256,
            "1c499088cddd0382918bd5650718533d",
            "same key name",
            false,
        )
        .await;
        test_pki_import_key_case(
            &core,
            token,
            path,
            "aes-gcm-256-import-bad-type",
            "aes-gcmm",
            256,
            "1c499088cddd0382918bd5650718533d",
            "6349e3032b690f2fe61a824746ac3ab05c1829a4147f4891f595dfb19cddfd06",
            false,
        )
        .await;
        test_pki_import_key_case(
            &core,
            token,
            path,
            "aes-gcm-256-import-bad-hex",
            "aes-gcm",
            256,
            "1c499088cddd0382918bd5650718533d",
            "aa6349e3032b690f2fe61a824746ac3ab05c1829a4147f4891f595dfb19cddfd06",
            false,
        )
        .await;

        //test aes-gcm encrypt and decrypt
        test_pki_encrypt_decrypt(&core, token, path, "aes-gcm-128-import", "rusty_vault test".as_bytes(), true).await;
        test_pki_encrypt_decrypt(&core, token, path, "aes-gcm-192-import", "rusty_vault test".as_bytes(), true).await;
        test_pki_encrypt_decrypt(&core, token, path, "aes-gcm-256-import", "rusty_vault test".as_bytes(), true).await;

        //test import aes-cbc key
        test_pki_import_key_case(
            &core,
            token,
            path,
            "aes-cbc-128-import",
            "aes-cbc",
            128,
            "1c499088cddd0382918bd5650718533d",
            "77628ff2c35adc7efdecfb0e86a4576f",
            true,
        )
        .await;
        test_pki_import_key_case(
            &core,
            token,
            path,
            "aes-cbc-192-import",
            "aes-cbc",
            192,
            "1c499088cddd0382918bd5650718533d",
            "807f5f15d2924f104700f058030298c8591d0f6b5163b333",
            true,
        )
        .await;
        test_pki_import_key_case(
            &core,
            token,
            path,
            "aes-cbc-256-import",
            "aes-cbc",
            256,
            "1c499088cddd0382918bd5650718533d",
            "521fc4bb8ee6015ac5a6e3e611854aa7608a17413f72ee007e799dac303853e1",
            true,
        )
        .await;
        test_pki_import_key_case(
            &core,
            token,
            path,
            "aes-cbc-256-import",
            "aes-cbc",
            256,
            "1c499088cddd0382918bd5650718533d",
            "same key name",
            false,
        )
        .await;
        test_pki_import_key_case(
            &core,
            token,
            path,
            "aes-cbc-256-import-bad-type",
            "aes-cbcc",
            256,
            "1c499088cddd0382918bd5650718533d",
            "521fc4bb8ee6015ac5a6e3e611854aa7608a17413f72ee007e799dac303853e1",
            false,
        )
        .await;
        test_pki_import_key_case(
            &core,
            token,
            path,
            "aes-cbc-256-import-bad-hex",
            "aes-cbc",
            256,
            "1c499088cddd0382918bd5650718533d",
            "21521fc4bb8ee6015ac5a6e3e611854aa7608a17413f72ee007e799dac303853e1",
            false,
        )
        .await;

        //test aes-cbc encrypt and decrypt
        test_pki_encrypt_decrypt(&core, token, path, "aes-cbc-128-import", "rusty_vault test".as_bytes(), true).await;
        test_pki_encrypt_decrypt(&core, token, path, "aes-cbc-192-import", "rusty_vault test".as_bytes(), true).await;
        test_pki_encrypt_decrypt(&core, token, path, "aes-cbc-256-import", "rusty_vault test".as_bytes(), true).await;

        //test import aes-ecb key
        test_pki_import_key_case(
            &core,
            token,
            path,
            "aes-ecb-128-import",
            "aes-ecb",
            128,
            "",
            "38a1f9ad74562db696872cbfa10cc46e",
            true,
        )
        .await;
        test_pki_import_key_case(
            &core,
            token,
            path,
            "aes-ecb-192-import",
            "aes-ecb",
            192,
            "",
            "b80f65a5a334e583bafd18d2e86667384ae16cb0467982de",
            true,
        )
        .await;
        test_pki_import_key_case(
            &core,
            token,
            path,
            "aes-ecb-256-import",
            "aes-ecb",
            256,
            "",
            "95b622ebf838b0b8b4cc60635333f87f9b10bcbe340b710020a6e9789156c052",
            true,
        )
        .await;
        test_pki_import_key_case(&core, token, path, "aes-ecb-256-import", "aes-ecb", 256, "", "same key name", false)
            .await;
        test_pki_import_key_case(
            &core,
            token,
            path,
            "aes-ecb-256-import-bad-type",
            "aes-ecbb",
            256,
            "",
            "95b622ebf838b0b8b4cc60635333f87f9b10bcbe340b710020a6e9789156c052",
            false,
        )
        .await;
        test_pki_import_key_case(
            &core,
            token,
            path,
            "aes-ecb-256-import-bad-hex",
            "aes-ecb",
            256,
            "",
            "2295b622ebf838b0b8b4cc60635333f87f9b10bcbe340b710020a6e9789156c052",
            false,
        )
        .await;

        //test aes-gcm encrypt and decrypt
        test_pki_encrypt_decrypt(&core, token, path, "aes-ecb-128-import", "rusty_vault test".as_bytes(), true).await;
        test_pki_encrypt_decrypt(&core, token, path, "aes-ecb-192-import", "rusty_vault test".as_bytes(), true).await;
        test_pki_encrypt_decrypt(&core, token, path, "aes-ecb-256-import", "rusty_vault test".as_bytes(), true).await;
    }
}
