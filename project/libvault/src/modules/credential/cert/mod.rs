//! The `cert` auth method allows authentication using SSL/TLS client certificates which are
//! either signed by a CA or self-signed. SSL/TLS client certificates are defined as having
//! an `ExtKeyUsage` extension with the usage set to either `ClientAuth` or `Any`.
//!
//! The trusted certificates and CAs are configured directly to the auth method using the
//! `certs/` path. This method cannot read trusted certificates from an external source.
//!
//! CA certificates are associated with a role; role names and CRL names are normalized
//! to lower-case.
//!
//! Please note that to use this auth method, `tls_disable` and `tls_disable_client_certs`
//! must be false in the RustyVault configuration. This is because the certificates are
//! sent through TLS communication itself.

use std::{any::Any, sync::Arc};

use dashmap::DashMap;
use derive_more::Deref;

use crate::{
    core::Core,
    errors::RvError,
    logical::{Backend, LogicalBackend},
    modules::{auth::AuthModule, Module},
    new_logical_backend, new_logical_backend_internal,
};

pub mod cli;
pub mod path_certs;
pub mod path_config;
pub mod path_crls;
pub mod path_login;

pub use path_certs::CertEntry;
pub use path_crls::CRLInfo;

static CERT_BACKEND_HELP: &str = r#"
The "cert" credential provider allows authentication using
TLS client certificates. A client connects to RustyVault and uses
the "login" endpoint to generate a client token.

Trusted certificates are configured using the "certs/" endpoint
by a user with root access. A certificate authority can be trusted,
which permits all keys signed by it. Alternatively, self-signed
certificates can be trusted avoiding the need for a CA.
"#;

pub struct CertModule {
    pub name: String,
    pub backend: Arc<CertBackend>,
}

pub struct CertBackendInner {
    pub core: Arc<Core>,
    pub crls: DashMap<String, CRLInfo>,
}

#[derive(Deref)]
pub struct CertBackend {
    #[deref]
    pub inner: Arc<CertBackendInner>,
}

impl CertBackend {
    pub fn new(core: Arc<Core>) -> Self {
        let inner = CertBackendInner { core, crls: DashMap::new() };
        Self { inner: Arc::new(inner) }
    }

    pub fn new_backend(&self) -> LogicalBackend {
        let cert_backend_ref = self.inner.clone();

        let mut backend = new_logical_backend!({
            unauth_paths: ["login"],
            auth_renew_handler: cert_backend_ref.login_renew,
            help: CERT_BACKEND_HELP,
        });

        backend.paths.push(Arc::new(self.config_path()));
        backend.paths.push(Arc::new(self.certs_path()));
        backend.paths.push(Arc::new(self.certs_list_path()));
        backend.paths.push(Arc::new(self.crl_path()));
        backend.paths.push(Arc::new(self.crl_list_path()));
        backend.paths.push(Arc::new(self.login_path()));

        backend
    }
}

impl CertModule {
    pub fn new(core: Arc<Core>) -> Self {
        Self { name: "cert".to_string(), backend: Arc::new(CertBackend::new(core)) }
    }
}

impl Module for CertModule {
    fn name(&self) -> String {
        self.name.clone()
    }

    fn as_any_arc(self: Arc<Self>) -> Arc<dyn Any + Send + Sync> {
        self
    }

    fn setup(&self, core: &Core) -> Result<(), RvError> {
        let cert = self.backend.clone();
        let cert_backend_new_func = move |_c: Arc<Core>| -> Result<Arc<dyn Backend>, RvError> {
            let mut cert_backend = cert.new_backend();
            cert_backend.init()?;
            Ok(Arc::new(cert_backend))
        };

        if let Some(auth_module) = core.module_manager.get_module::<AuthModule>("auth") {
            return auth_module.add_auth_backend("cert", Arc::new(cert_backend_new_func));
        } else {
            log::error!("get auth module failed!");
        }

        Ok(())
    }

    fn cleanup(&self, core: &Core) -> Result<(), RvError> {
        if let Some(auth_module) = core.module_manager.get_module::<AuthModule>("auth") {
            return auth_module.delete_auth_backend("cert");
        } else {
            log::error!("get auth module failed!");
        }

        Ok(())
    }
}

#[cfg(test)]
mod test {
    use std::time::Duration;

    use serde_json::{json, Map, Value};

    use super::*;
    use crate::{
        modules::auth::expiration::DEFAULT_LEASE_DURATION_SECS,
        test_utils::{new_test_cert, new_test_cert_ext, new_test_crl, TestHttpServer, TestTlsClientAuth},
    };

    #[derive(Default)]
    struct Allowed {
        // allowed names in the certificate, looks at common, name, dns, email [depricated]
        names: String,
        // allowed common names in the certificate
        common_names: String,
        // allowed dns names in the SAN extension of the certificate
        dns: String,
        // allowed email names in SAN extension of the certificate
        emails: String,
        // allowed uris in SAN extension of the certificate
        uris: String,
        // allowed OUs in the certificate
        organizational_units: String,
        // required extensions in the certificate
        ext: String,
        // allowed metadata extensions to add to identity alias
        metadata_ext: String,
    }

    impl TestHttpServer {
        fn test_write_cert(
            &self,
            name: &str,
            cert: &str,
            policies: &str,
            test_data: &Allowed,
            extra_data: &Map<String, Value>,
            expect_err: Option<&str>,
        ) {
            let mut data = json!({
                "display_name": name,
                "policies": policies,
                "certificate": cert,
                "lease": 1000,
                "allowed_names": &test_data.names,
                "allowed_common_names": &test_data.common_names,
                "allowed_dns_sans": &test_data.dns,
                "allowed_email_sans": &test_data.emails,
                "allowed_uri_sans": &test_data.uris,
                "allowed_organizational_units": &test_data.organizational_units,
                "required_extensions": &test_data.ext,
                "allowed_metadata_extensions": &test_data.metadata_ext,
            })
            .as_object()
            .unwrap()
            .clone();

            for (key, value) in extra_data.iter() {
                data.insert(key.clone(), value.clone());
            }

            let ret = self.write(&format!("auth/{}/certs/{}", &self.mount_path, name), Some(data), None);
            assert!(ret.is_ok());
            let (status, resp) = ret.unwrap();
            match expect_err {
                Some(err) => {
                    assert_eq!(status, 400);
                    assert_eq!(resp["error"], err);
                }
                _ => {
                    assert!(status == 200 || status == 204);
                }
            }
        }

        fn test_write_cert_lease(&self, name: &str, cert: &str, policies: &str) {
            let data = json!({
                "display_name": name,
                "policies": policies,
                "certificate": cert,
                "lease": 900,
            })
            .as_object()
            .cloned();

            let ret = self.write(&format!("auth/{}/certs/{}", &self.mount_path, name), data, None);
            assert!(ret.is_ok());
            let (status, _) = ret.unwrap();
            assert!(status == 200 || status == 204);
        }

        fn test_write_cert_no_lease(&self, name: &str, cert: &str, policies: &str) {
            let data = json!({
                "display_name": name,
                "policies": policies,
                "certificate": cert,
            })
            .as_object()
            .cloned();

            let ret = self.write(&format!("auth/{}/certs/{}", &self.mount_path, name), data, None);
            assert!(ret.is_ok());
            let (status, _) = ret.unwrap();
            assert!(status == 200 || status == 204);
        }

        fn test_write_cert_ttl(&self, name: &str, cert: &str, policies: &str) {
            let data = json!({
                "display_name": name,
                "policies": policies,
                "certificate": cert,
                "ttl": "900s",
            })
            .as_object()
            .cloned();

            let ret = self.write(&format!("auth/{}/certs/{}", &self.mount_path, name), data, None);
            assert!(ret.is_ok());
            let (status, _) = ret.unwrap();
            assert!(status == 200 || status == 204);
        }

        fn test_write_cert_max_ttl(&self, name: &str, cert: &str, policies: &str) {
            let data = json!({
                "display_name": name,
                "policies": policies,
                "certificate": cert,
                "ttl": "900s",
                "max_ttl": "1200s",
            })
            .as_object()
            .cloned();

            let ret = self.write(&format!("auth/{}/certs/{}", &self.mount_path, name), data, None);
            assert!(ret.is_ok());
            let (status, _) = ret.unwrap();
            assert!(status == 200 || status == 204);
        }

        fn test_read_cert(&self, name: &str) -> Result<(u16, Value), RvError> {
            self.read(&format!("auth/{}/certs/{}", &self.mount_path, name), None)
        }

        fn test_login(
            &self,
            server_ca: &str,
            client_cert: &str,
            client_key: &str,
            expect_err: Option<&str>,
        ) -> Result<(u16, Value), RvError> {
            self.test_login_with_name("", server_ca, client_cert, client_key, expect_err)
        }

        fn test_login_with_name(
            &self,
            name: &str,
            server_ca: &str,
            client_cert: &str,
            client_key: &str,
            expect_err: Option<&str>,
        ) -> Result<(u16, Value), RvError> {
            let tls_client_auth = TestTlsClientAuth {
                ca_pem: server_ca.into(),
                cert_pem: client_cert.into(),
                key_pem: client_key.into(),
            };

            let data = json!({
                "name": name,
            })
            .as_object()
            .cloned();
            let ret = self.login(&format!("auth/{}/login", &self.mount_path), data, Some(tls_client_auth));
            assert!(ret.is_ok());
            let (status, resp) = ret.unwrap();
            match expect_err {
                Some(err) => {
                    assert!(status == 400 || status == 403);
                    assert_eq!(resp["error"], err);
                    assert!(resp["auth"].is_null());
                }
                _ => {
                    assert!(resp["auth"].is_object());
                    assert_ne!(resp["auth"]["client_token"], "");
                }
            }

            Ok((status, resp))
        }

        fn test_login_with_metadata(
            &self,
            name: &str,
            server_ca: &str,
            client_cert: &str,
            client_key: &str,
            meta_data: &Map<String, Value>,
            expect_err: Option<&str>,
        ) -> Result<(u16, Value), RvError> {
            let tls_client_auth = TestTlsClientAuth {
                ca_pem: server_ca.into(),
                cert_pem: client_cert.into(),
                key_pem: client_key.into(),
            };

            let data = json!({
                "metadata": meta_data,
            })
            .as_object()
            .cloned();
            let ret = self.login(&format!("auth/{}/login", &self.mount_path), data, Some(tls_client_auth));
            assert!(ret.is_ok());
            let (status, resp) = ret.unwrap();
            match expect_err {
                Some(err) => {
                    assert_eq!(status, 400);
                    assert_eq!(resp["error"], err);
                    assert!(resp["auth"].is_null());
                }
                _ => {
                    assert!(resp["auth"].is_object());
                    assert_ne!(resp["auth"]["client_token"], "");
                    for (key, expected) in meta_data.iter() {
                        assert_eq!(resp["auth"]["metadata"][key], expected.clone());
                    }
                    assert_eq!(resp["auth"]["metadata"]["cert_name"], name);
                }
            }

            Ok((status, resp))
        }

        fn test_write_crl(&self, revoked_cert: &str, ca_cert: &str, ca_key: &str) {
            let crl_pem_ret = unsafe { new_test_crl(revoked_cert, ca_cert, ca_key) };
            assert!(crl_pem_ret.is_ok());
            let crl_pem = crl_pem_ret.unwrap();
            let crl_data = json!({
                "crl": crl_pem,
            })
            .as_object()
            .cloned();
            let ret = self.write(&format!("auth/{}/crls/test", &self.mount_path), crl_data, None);
            assert!(ret.is_ok());
            let (status, _) = ret.unwrap();
            assert!(status == 200 || status == 204);

            // Ensure the CRL shows up on a list.
            let ret = self.list("auth/cert/crls", None);
            assert!(ret.is_ok());

            let (status, resp_data) = ret.unwrap();
            assert_eq!(status, 200);
            assert_eq!(resp_data["data"]["keys"].as_array().expect("crl list is empty").len(), 1);
        }

        fn test_delete_crl(&self) {
            let ret = self.delete(&format!("auth/{}/crls/test", &self.mount_path), None, None);
            assert!(ret.is_ok());
            let (status, _) = ret.unwrap();
            assert!(status == 200 || status == 204);

            // Ensure the CRL shows up on a list.
            let ret = self.list("auth/cert/crls", None);
            assert!(ret.is_ok());

            let (status, resp_data) = ret.unwrap();
            assert_eq!(status, 200);
            assert_eq!(resp_data["data"]["keys"].as_array().expect("crl list is empty").len(), 0);
        }
    }

    #[maybe_async::test(feature = "sync_handler", async(all(not(feature = "sync_handler")), tokio::test))]
    async fn test_credential_cert_module_permitted_dns_domains_intermediate_ca() {
        let mut test_http_server =
            TestHttpServer::new("test_credential_cert_module_permitted_dns_domains_intermediate_ca", true).await;

        let (intermediate_ca_cert, intermediate_ca_key) = new_test_cert(
            true,
            true,
            true,
            "inter",
            Some(".myrv.com"),
            None,
            None,
            None,
            Some(test_http_server.ca_cert_pem.clone()),
            Some(test_http_server.ca_key_pem.clone()),
        )
        .unwrap();

        let (leaf_cert, leaf_key) = new_test_cert(
            false,
            true,
            true,
            "cert.myrv.com",
            Some("cert.myrv.com"),
            None,
            None,
            Some("10s"),
            Some(intermediate_ca_cert.clone()),
            Some(intermediate_ca_key),
        )
        .unwrap();

        // mount cert auth to path: auth/cert
        let _ = test_http_server.mount_auth("cert", "cert");

        test_http_server.test_write_cert(
            "myrv-dot-com",
            &intermediate_ca_cert,
            "default",
            &Allowed::default(),
            &Map::new(),
            None,
        );

        let _ = test_http_server.test_login(&test_http_server.ca_cert_pem, &leaf_cert, &leaf_key, None);

        // TODO: testing pathLoginRenew for cert auth
    }

    #[maybe_async::test(feature = "sync_handler", async(all(not(feature = "sync_handler")), tokio::test))]
    async fn test_credential_cert_module_non_ca_expiry() {
        let mut test_http_server = TestHttpServer::new("test_credential_cert_module_non_ca_expiry", true).await;

        // mount /pki as a root CA
        let ret = test_http_server.mount("pki", "pki");
        assert!(ret.is_ok());

        let (ca_cert, ca_key) = new_test_cert(true, true, true, "test-ca", None, None, None, None, None, None).unwrap();

        let (issued_cert, issued_key) = new_test_cert(
            false,
            true,
            true,
            "cert.myrv.com",
            Some("cert.myrv.com"),
            None,
            None,
            Some("5s"),
            Some(ca_cert.clone()),
            Some(ca_key),
        )
        .unwrap();

        // mount cert auth to path: auth/cert
        let ret = test_http_server.mount_auth("cert", "cert");
        assert!(ret.is_ok());

        test_http_server.test_write_cert(
            "myrv-dot-com",
            &issued_cert,
            "default",
            &Allowed::default(),
            &Map::new(),
            None,
        );

        // Login when the certificate is still valid. Login should succeed.
        let _ = test_http_server.test_login(&test_http_server.ca_cert_pem, &issued_cert, &issued_key, None);

        // Wait until the certificate expires
        std::thread::sleep(Duration::from_secs(6));

        // Login attempt after certificate expiry should fail
        let _ = test_http_server.test_login(
            &test_http_server.ca_cert_pem,
            &issued_cert,
            &issued_key,
            Some("certificate has expired"),
        );
    }

    #[maybe_async::test(feature = "sync_handler", async(all(not(feature = "sync_handler")), tokio::test))]
    async fn test_credential_cert_module_registered_non_ca_crl() {
        let mut test_http_server = TestHttpServer::new("test_credential_cert_module_registered_non_ca_crl", true).await;

        let (ca_cert, ca_key) = new_test_cert(true, true, true, "test-ca", None, None, None, None, None, None).unwrap();

        let (issued_cert, issued_key) = new_test_cert(
            false,
            true,
            true,
            "cert.myrv.com",
            Some("cert.myrv.com"),
            None,
            None,
            None,
            Some(ca_cert.clone()),
            Some(ca_key.clone()),
        )
        .unwrap();

        // mount cert auth to path: auth/cert
        let ret = test_http_server.mount_auth("cert", "cert");
        assert!(ret.is_ok());

        test_http_server.test_write_cert(
            "myrv-dot-com",
            &issued_cert,
            "default",
            &Allowed::default(),
            &Map::new(),
            None,
        );

        // Login when the certificate is still valid. Login should succeed.
        let _ = test_http_server.test_login(&test_http_server.ca_cert_pem, &issued_cert, &issued_key, None);

        // Register a CRL containing the issued client certificate used above.
        test_http_server.test_write_crl(&issued_cert, &ca_cert, &ca_key);

        // Login attempt after certificate expiry should fail
        let _ = test_http_server.test_login(
            &test_http_server.ca_cert_pem,
            &issued_cert,
            &issued_key,
            Some("invalid certificate or no client certificate supplied"),
        );
    }

    #[maybe_async::test(feature = "sync_handler", async(all(not(feature = "sync_handler")), tokio::test))]
    async fn test_credential_cert_module_crls() {
        let mut test_http_server = TestHttpServer::new("test_credential_cert_module_crls", true).await;

        let (ca_cert, ca_key) = new_test_cert(true, true, true, "test-ca", None, None, None, None, None, None).unwrap();

        let (issued_cert, issued_key) = new_test_cert(
            false,
            true,
            true,
            "cert.myrv.com",
            Some("cert.myrv.com"),
            None,
            None,
            None,
            Some(ca_cert.clone()),
            Some(ca_key.clone()),
        )
        .unwrap();

        // mount cert auth to path: auth/cert
        let ret = test_http_server.mount_auth("cert", "cert");
        assert!(ret.is_ok());

        // Register the CA certificate of the client key pair
        test_http_server.test_write_cert("cert1", &ca_cert, "abc", &Allowed::default(), &Map::new(), None);

        // Login with the CA certificate should be successful.
        let _ = test_http_server.test_login(&test_http_server.ca_cert_pem, &ca_cert, &ca_key, None);

        // Login with a client certificate issued by this CA certificate should also be successful.
        let _ = test_http_server.test_login(&test_http_server.ca_cert_pem, &issued_cert, &issued_key, None);

        // Register a CRL containing the issued client certificate used above.
        test_http_server.test_write_crl(&issued_cert, &ca_cert, &ca_key);

        // Attempt login with the revoked certificate should fail.
        let _ = test_http_server.test_login(
            &test_http_server.ca_cert_pem,
            &issued_cert,
            &issued_key,
            Some("no chain matching all constraints could be found for this login certificate"),
        );

        // Register a different client CA certificate.
        let (ca_cert, ca_key) =
            new_test_cert(true, true, true, "test-ca2", None, None, None, None, None, None).unwrap();
        test_http_server.test_write_cert("cert1", &ca_cert, "abc", &Allowed::default(), &Map::new(), None);

        // Test login using a different client CA cert pair.
        let _ = test_http_server.test_login(&test_http_server.ca_cert_pem, &ca_cert, &ca_key, None);

        // Register a CRL containing the root CA certificate used above.
        test_http_server.test_write_crl(&ca_cert, &ca_cert, &ca_key);

        // Attempt login with the revoked ca certificate should fail.
        let _ = test_http_server.test_login(
            &test_http_server.ca_cert_pem,
            &ca_cert,
            &ca_key,
            Some("no chain matching all constraints could be found for this login certificate"),
        );
    }

    #[maybe_async::test(feature = "sync_handler", async(all(not(feature = "sync_handler")), tokio::test))]
    async fn test_credential_cert_module_cert_writes() {
        let mut test_http_server = TestHttpServer::new("test_credential_cert_module_cert_writes", true).await;

        // CA cert
        let (ca_cert, ca_key) = new_test_cert(true, true, true, "test-ca", None, None, None, None, None, None).unwrap();

        // Non CA cert
        let (non_ca_cert, _) = new_test_cert(
            false,
            true,
            true,
            "non-ca-cert",
            Some("non-ca-cert"),
            None,
            None,
            None,
            Some(ca_cert.clone()),
            Some(ca_key.clone()),
        )
        .unwrap();

        // Non CA cert without TLS web client authentication
        let (non_ca_cert2, _) = new_test_cert(
            false,
            false,
            true,
            "non-ca-cert2",
            Some("non-ca-cert2"),
            None,
            None,
            None,
            Some(ca_cert.clone()),
            Some(ca_key.clone()),
        )
        .unwrap();

        // mount cert auth to path: auth/cert
        let ret = test_http_server.mount_auth("cert", "cert");
        assert!(ret.is_ok());

        test_http_server.test_write_cert("cert1", &ca_cert, "abc", &Allowed::default(), &Map::new(), None);
        test_http_server.test_write_cert("cert1", &non_ca_cert, "abc", &Allowed::default(), &Map::new(), None);
        test_http_server.test_write_cert(
            "cert1",
            &non_ca_cert2,
            "abc",
            &Allowed::default(),
            &Map::new(),
            Some("nonCA certificates should have TLS client authentication set as an extended key usage"),
        );
    }

    #[maybe_async::test(feature = "sync_handler", async(all(not(feature = "sync_handler")), tokio::test))]
    async fn test_credential_cert_module_basic_ca() {
        let mut test_http_server = TestHttpServer::new("test_credential_cert_module_basic_ca", true).await;

        // CA cert
        let (ca_cert, ca_key) = new_test_cert(true, true, true, "test-ca", None, None, None, None, None, None).unwrap();

        let (client_cert, client_key) = new_test_cert(
            false,
            true,
            true,
            "cert.example.com",
            Some("cert.example.com"),
            None,
            None,
            None,
            Some(ca_cert.clone()),
            Some(ca_key.clone()),
        )
        .unwrap();

        // mount cert auth to path: auth/cert
        let ret = test_http_server.mount_auth("cert", "cert");
        assert!(ret.is_ok());

        test_http_server.test_write_cert("web", &ca_cert, "foo", &Allowed::default(), &Map::new(), None);

        // Test a client trusted by a CA
        let _ = test_http_server.test_login(&test_http_server.ca_cert_pem, &client_cert, &client_key, None);
        let (_, resp) =
            test_http_server.test_login(&test_http_server.ca_cert_pem, &client_cert, &client_key, None).unwrap();
        assert_eq!(resp["auth"]["lease_duration"], 1000);
        assert_eq!(resp["auth"]["policies"], json!(["default", "foo"]));

        test_http_server.test_write_cert_lease("web", &ca_cert, "foo");
        test_http_server.test_write_cert_ttl("web", &ca_cert, "foo");
        let (_, resp) =
            test_http_server.test_login(&test_http_server.ca_cert_pem, &client_cert, &client_key, None).unwrap();
        assert_eq!(resp["auth"]["lease_duration"], 900);
        assert_eq!(resp["auth"]["policies"], json!(["default", "foo"]));

        test_http_server.test_write_cert_max_ttl("web", &ca_cert, "foo");
        let _ = test_http_server.test_login(&test_http_server.ca_cert_pem, &client_cert, &client_key, None);
        test_http_server.test_write_cert_no_lease("web", &ca_cert, "foo");
        let (_, resp) =
            test_http_server.test_login(&test_http_server.ca_cert_pem, &client_cert, &client_key, None).unwrap();
        assert_eq!(resp["auth"]["lease_duration"], 900);
        assert_eq!(resp["auth"]["policies"], json!(["default", "foo"]));

        test_http_server.test_write_cert(
            "web",
            &ca_cert,
            "foo",
            &Allowed { names: "*.example.com".into(), ..Default::default() },
            &Map::new(),
            None,
        );
        let _ = test_http_server.test_login(&test_http_server.ca_cert_pem, &client_cert, &client_key, None).unwrap();

        test_http_server.test_write_cert(
            "web",
            &ca_cert,
            "foo",
            &Allowed { names: "*.invalid.com".into(), ..Default::default() },
            &Map::new(),
            None,
        );
        let _ = test_http_server
            .test_login(
                &test_http_server.ca_cert_pem,
                &client_cert,
                &client_key,
                Some("no chain matching all constraints could be found for this login certificate"),
            )
            .unwrap();
    }

    #[maybe_async::test(feature = "sync_handler", async(all(not(feature = "sync_handler")), tokio::test))]
    async fn test_credential_cert_module_basic_crls() {
        let mut test_http_server = TestHttpServer::new("test_credential_cert_module_basic_crls", true).await;

        // CA cert
        let (ca_cert, ca_key) = new_test_cert(true, true, true, "test-ca", None, None, None, None, None, None).unwrap();

        let (client_cert, client_key) = new_test_cert(
            false,
            true,
            true,
            "cert.example.com",
            Some("cert.example.com"),
            None,
            None,
            None,
            Some(ca_cert.clone()),
            Some(ca_key.clone()),
        )
        .unwrap();

        // mount cert auth to path: auth/cert
        let ret = test_http_server.mount_auth("cert", "cert");
        assert!(ret.is_ok());

        test_http_server.test_write_cert_no_lease("web", &ca_cert, "foo");

        let (_, resp) =
            test_http_server.test_login(&test_http_server.ca_cert_pem, &client_cert, &client_key, None).unwrap();
        assert_eq!(resp["auth"]["lease_duration"], DEFAULT_LEASE_DURATION_SECS.as_secs());
        assert_eq!(resp["auth"]["policies"], json!(["default", "foo"]));

        test_http_server.test_write_crl(&ca_cert, &ca_cert, &ca_key);

        let (status, resp) = test_http_server
            .test_login(
                &test_http_server.ca_cert_pem,
                &client_cert,
                &client_key,
                Some("no chain matching all constraints could be found for this login certificate"),
            )
            .unwrap();
        assert_eq!(status, 400);
        assert!(resp["auth"].is_null());

        test_http_server.test_delete_crl();

        let (_, resp) =
            test_http_server.test_login(&test_http_server.ca_cert_pem, &client_cert, &client_key, None).unwrap();
        assert_eq!(resp["auth"]["lease_duration"], DEFAULT_LEASE_DURATION_SECS.as_secs());
        assert_eq!(resp["auth"]["policies"], json!(["default", "foo"]));
    }

    #[maybe_async::test(feature = "sync_handler", async(all(not(feature = "sync_handler")), tokio::test))]
    async fn test_credential_cert_module_basic_single_cert() {
        let mut test_http_server = TestHttpServer::new("test_credential_cert_module_basic_single_cert", true).await;

        // CA cert
        let (ca_cert, ca_key) = new_test_cert(
            true,
            true,
            true,
            "example.com",
            Some("example.com"),
            Some("127.0.0.1"),
            None,
            None,
            None,
            None,
        )
        .unwrap();

        // mount cert auth to path: auth/cert
        let ret = test_http_server.mount_auth("cert", "cert");
        assert!(ret.is_ok());

        test_http_server.test_write_cert("web", &ca_cert, "foo", &Allowed::default(), &Map::new(), None);

        let _ = test_http_server.test_login(&test_http_server.ca_cert_pem, &ca_cert, &ca_key, None).unwrap();

        test_http_server.test_write_cert(
            "web",
            &ca_cert,
            "foo",
            &Allowed { names: "example.com".into(), ..Default::default() },
            &Map::new(),
            None,
        );
        let _ = test_http_server.test_login(&test_http_server.ca_cert_pem, &ca_cert, &ca_key, None).unwrap();

        test_http_server.test_write_cert(
            "web",
            &ca_cert,
            "foo",
            &Allowed { names: "invalid".into(), ..Default::default() },
            &Map::new(),
            None,
        );
        let _ = test_http_server
            .test_login(
                &test_http_server.ca_cert_pem,
                &ca_cert,
                &ca_key,
                Some("no chain matching all constraints could be found for this login certificate"),
            )
            .unwrap();

        test_http_server.test_write_cert(
            "web",
            &ca_cert,
            "foo",
            &Allowed { common_names: "example.com".into(), ..Default::default() },
            &Map::new(),
            None,
        );
        let _ = test_http_server.test_login(&test_http_server.ca_cert_pem, &ca_cert, &ca_key, None).unwrap();

        test_http_server.test_write_cert(
            "web",
            &ca_cert,
            "foo",
            &Allowed { common_names: "invalid".into(), ..Default::default() },
            &Map::new(),
            None,
        );
        let _ = test_http_server
            .test_login(
                &test_http_server.ca_cert_pem,
                &ca_cert,
                &ca_key,
                Some("no chain matching all constraints could be found for this login certificate"),
            )
            .unwrap();

        test_http_server.test_write_cert(
            "web",
            &ca_cert,
            "foo",
            &Allowed { ext: "1.2.3.4:invalid".into(), ..Default::default() },
            &Map::new(),
            None,
        );
        let _ = test_http_server
            .test_login(
                &test_http_server.ca_cert_pem,
                &ca_cert,
                &ca_key,
                Some("no chain matching all constraints could be found for this login certificate"),
            )
            .unwrap();
    }

    #[maybe_async::test(feature = "sync_handler", async(all(not(feature = "sync_handler")), tokio::test))]
    async fn test_credential_cert_module_ext_single_cert() {
        let mut test_http_server = TestHttpServer::new("test_credential_cert_module_ext_single_cert", true).await;

        // CA cert
        let (ca_cert, ca_key) = new_test_cert_ext(
            true,
            true,
            true,
            "example.com",
            Some("example.com"),
            Some("127.0.0.1"),
            None,
            None,
            None,
            None,
        )
        .unwrap();

        // mount cert auth to path: auth/cert
        let ret = test_http_server.mount_auth("cert", "cert");
        assert!(ret.is_ok());

        test_http_server.test_write_cert(
            "web",
            &ca_cert,
            "foo",
            &Allowed { ext: "2.1.1.1:A UTF8String Extension".into(), ..Default::default() },
            &Map::new(),
            None,
        );
        let _ = test_http_server.test_login(&test_http_server.ca_cert_pem, &ca_cert, &ca_key, None).unwrap();

        test_http_server.test_write_cert(
            "web",
            &ca_cert,
            "foo",
            &Allowed { ext: "2.1.1.1:*,2.1.1.2:A UTF8*".into(), ..Default::default() },
            &Map::new(),
            None,
        );
        let _ = test_http_server.test_login(&test_http_server.ca_cert_pem, &ca_cert, &ca_key, None).unwrap();

        test_http_server.test_write_cert(
            "web",
            &ca_cert,
            "foo",
            &Allowed { ext: "1.2.3.45:*".into(), ..Default::default() },
            &Map::new(),
            None,
        );
        let _ = test_http_server
            .test_login(
                &test_http_server.ca_cert_pem,
                &ca_cert,
                &ca_key,
                Some("no chain matching all constraints could be found for this login certificate"),
            )
            .unwrap();

        test_http_server.test_write_cert(
            "web",
            &ca_cert,
            "foo",
            &Allowed { ext: "2.1.1.1:The Wrong Value".into(), ..Default::default() },
            &Map::new(),
            None,
        );
        let _ = test_http_server
            .test_login(
                &test_http_server.ca_cert_pem,
                &ca_cert,
                &ca_key,
                Some("no chain matching all constraints could be found for this login certificate"),
            )
            .unwrap();

        test_http_server.test_write_cert(
            "web",
            &ca_cert,
            "foo",
            &Allowed { ext: "2.1.1.1:*,2.1.1.2:The Wrong Value".into(), ..Default::default() },
            &Map::new(),
            None,
        );
        let _ = test_http_server
            .test_login(
                &test_http_server.ca_cert_pem,
                &ca_cert,
                &ca_key,
                Some("no chain matching all constraints could be found for this login certificate"),
            )
            .unwrap();

        test_http_server.test_write_cert(
            "web",
            &ca_cert,
            "foo",
            &Allowed { ext: "2.1.1.1:".into(), ..Default::default() },
            &Map::new(),
            None,
        );
        let _ = test_http_server
            .test_login(
                &test_http_server.ca_cert_pem,
                &ca_cert,
                &ca_key,
                Some("no chain matching all constraints could be found for this login certificate"),
            )
            .unwrap();

        test_http_server.test_write_cert(
            "web",
            &ca_cert,
            "foo",
            &Allowed { ext: "2.1.1.1:,2.1.1.2:*".into(), ..Default::default() },
            &Map::new(),
            None,
        );
        let _ = test_http_server
            .test_login(
                &test_http_server.ca_cert_pem,
                &ca_cert,
                &ca_key,
                Some("no chain matching all constraints could be found for this login certificate"),
            )
            .unwrap();

        test_http_server.test_write_cert(
            "web",
            &ca_cert,
            "foo",
            &Allowed {
                names: "example.com".into(),
                ext: "2.1.1.1:A UTF8String Extension".into(),
                ..Default::default()
            },
            &Map::new(),
            None,
        );
        let _ = test_http_server.test_login(&test_http_server.ca_cert_pem, &ca_cert, &ca_key, None).unwrap();

        test_http_server.test_write_cert(
            "web",
            &ca_cert,
            "foo",
            &Allowed { names: "example.com".into(), ext: "2.1.1.1:*,2.1.1.2:A UTF8*".into(), ..Default::default() },
            &Map::new(),
            None,
        );
        let _ = test_http_server.test_login(&test_http_server.ca_cert_pem, &ca_cert, &ca_key, None).unwrap();

        test_http_server.test_write_cert(
            "web",
            &ca_cert,
            "foo",
            &Allowed { names: "example.com".into(), ext: "1.2.3.45:*".into(), ..Default::default() },
            &Map::new(),
            None,
        );
        let _ = test_http_server
            .test_login(
                &test_http_server.ca_cert_pem,
                &ca_cert,
                &ca_key,
                Some("no chain matching all constraints could be found for this login certificate"),
            )
            .unwrap();

        test_http_server.test_write_cert(
            "web",
            &ca_cert,
            "foo",
            &Allowed { names: "example.com".into(), ext: "2.1.1.1:The Wrong Value".into(), ..Default::default() },
            &Map::new(),
            None,
        );
        let _ = test_http_server
            .test_login(
                &test_http_server.ca_cert_pem,
                &ca_cert,
                &ca_key,
                Some("no chain matching all constraints could be found for this login certificate"),
            )
            .unwrap();

        test_http_server.test_write_cert(
            "web",
            &ca_cert,
            "foo",
            &Allowed {
                names: "example.com".into(),
                ext: "2.1.1.1:*,2.1.1.2:The Wrong Value".into(),
                ..Default::default()
            },
            &Map::new(),
            None,
        );
        let _ = test_http_server
            .test_login(
                &test_http_server.ca_cert_pem,
                &ca_cert,
                &ca_key,
                Some("no chain matching all constraints could be found for this login certificate"),
            )
            .unwrap();

        test_http_server.test_write_cert(
            "web",
            &ca_cert,
            "foo",
            &Allowed { names: "invalid".into(), ext: "2.1.1.1:A UTF8String Extension".into(), ..Default::default() },
            &Map::new(),
            None,
        );
        let _ = test_http_server
            .test_login(
                &test_http_server.ca_cert_pem,
                &ca_cert,
                &ca_key,
                Some("no chain matching all constraints could be found for this login certificate"),
            )
            .unwrap();

        test_http_server.test_write_cert(
            "web",
            &ca_cert,
            "foo",
            &Allowed { names: "invalid".into(), ext: "2.1.1.1:*,2.1.1.2:A UTF8*".into(), ..Default::default() },
            &Map::new(),
            None,
        );
        let _ = test_http_server
            .test_login(
                &test_http_server.ca_cert_pem,
                &ca_cert,
                &ca_key,
                Some("no chain matching all constraints could be found for this login certificate"),
            )
            .unwrap();

        test_http_server.test_write_cert(
            "web",
            &ca_cert,
            "foo",
            &Allowed { names: "invalid".into(), ext: "1.2.3.45:*".into(), ..Default::default() },
            &Map::new(),
            None,
        );
        let _ = test_http_server
            .test_login(
                &test_http_server.ca_cert_pem,
                &ca_cert,
                &ca_key,
                Some("no chain matching all constraints could be found for this login certificate"),
            )
            .unwrap();

        test_http_server.test_write_cert(
            "web",
            &ca_cert,
            "foo",
            &Allowed { names: "invalid".into(), ext: "2.1.1.1:The Wrong Value".into(), ..Default::default() },
            &Map::new(),
            None,
        );
        let _ = test_http_server
            .test_login(
                &test_http_server.ca_cert_pem,
                &ca_cert,
                &ca_key,
                Some("no chain matching all constraints could be found for this login certificate"),
            )
            .unwrap();

        test_http_server.test_write_cert(
            "web",
            &ca_cert,
            "foo",
            &Allowed { names: "invalid".into(), ext: "2.1.1.1:*,2.1.1.2:The Wrong Value".into(), ..Default::default() },
            &Map::new(),
            None,
        );
        let _ = test_http_server
            .test_login(
                &test_http_server.ca_cert_pem,
                &ca_cert,
                &ca_key,
                Some("no chain matching all constraints could be found for this login certificate"),
            )
            .unwrap();

        test_http_server.test_write_cert(
            "web",
            &ca_cert,
            "foo",
            &Allowed { names: "example.com".into(), ext: "hex:2.5.29.17:*87047F000002*".into(), ..Default::default() },
            &Map::new(),
            None,
        );
        let _ = test_http_server
            .test_login(
                &test_http_server.ca_cert_pem,
                &ca_cert,
                &ca_key,
                Some("no chain matching all constraints could be found for this login certificate"),
            )
            .unwrap();

        test_http_server.test_write_cert(
            "web",
            &ca_cert,
            "foo",
            &Allowed { names: "example.com".into(), ext: "hex:2.5.29.17:*87047F000001*".into(), ..Default::default() },
            &Map::new(),
            None,
        );
        let _ = test_http_server.test_login(&test_http_server.ca_cert_pem, &ca_cert, &ca_key, None).unwrap();

        test_http_server.test_write_cert(
            "web",
            &ca_cert,
            "foo",
            &Allowed { names: "example.com".into(), ext: "2.5.29.17:*".into(), ..Default::default() },
            &Map::new(),
            None,
        );
        let _ = test_http_server.test_login(&test_http_server.ca_cert_pem, &ca_cert, &ca_key, None).unwrap();

        test_http_server.test_write_cert(
            "web",
            &ca_cert,
            "foo",
            &Allowed { metadata_ext: "2.1.1.1,1.2.3.45".into(), ..Default::default() },
            &Map::new(),
            None,
        );
        let _ = test_http_server
            .test_login_with_metadata(
                "web",
                &test_http_server.ca_cert_pem,
                &ca_cert,
                &ca_key,
                json!({"2-1-1-1":"A UTF8String Extension"}).as_object().unwrap(),
                None,
            )
            .unwrap();

        test_http_server.test_write_cert(
            "web",
            &ca_cert,
            "foo",
            &Allowed { metadata_ext: "1.2.3.45".into(), ..Default::default() },
            &Map::new(),
            None,
        );
        let _ = test_http_server
            .test_login_with_metadata("web", &test_http_server.ca_cert_pem, &ca_cert, &ca_key, &Map::new(), None)
            .unwrap();
    }

    #[maybe_async::test(feature = "sync_handler", async(all(not(feature = "sync_handler")), tokio::test))]
    async fn test_credential_cert_module_dns_single_cert() {
        let mut test_http_server = TestHttpServer::new("test_credential_cert_module_dns_single_cert", true).await;

        // CA cert
        let (ca_cert, ca_key) =
            new_test_cert(true, true, true, "localhost", Some("localhost"), Some("127.0.0.1"), None, None, None, None)
                .unwrap();

        let (client_cert, client_key) = new_test_cert(
            false,
            true,
            true,
            "example.com",
            Some("example.com"),
            Some("127.0.0.1"),
            None,
            None,
            Some(ca_cert.clone()),
            Some(ca_key.clone()),
        )
        .unwrap();

        // mount cert auth to path: auth/cert
        let ret = test_http_server.mount_auth("cert", "cert");
        assert!(ret.is_ok());

        test_http_server.test_write_cert(
            "web",
            &ca_cert,
            "foo",
            &Allowed { dns: "example.com".into(), ..Default::default() },
            &Map::new(),
            None,
        );
        let _ = test_http_server.test_login(&test_http_server.ca_cert_pem, &client_cert, &client_key, None).unwrap();

        test_http_server.test_write_cert(
            "web",
            &ca_cert,
            "foo",
            &Allowed { dns: "*ample.com".into(), ..Default::default() },
            &Map::new(),
            None,
        );
        let _ = test_http_server.test_login(&test_http_server.ca_cert_pem, &client_cert, &client_key, None).unwrap();

        test_http_server.test_write_cert(
            "web",
            &ca_cert,
            "foo",
            &Allowed { dns: "notincert.com".into(), ..Default::default() },
            &Map::new(),
            None,
        );
        let _ = test_http_server
            .test_login(
                &test_http_server.ca_cert_pem,
                &client_cert,
                &client_key,
                Some("no chain matching all constraints could be found for this login certificate"),
            )
            .unwrap();

        test_http_server.test_write_cert(
            "web",
            &ca_cert,
            "foo",
            &Allowed { dns: "abc".into(), ..Default::default() },
            &Map::new(),
            None,
        );
        let _ = test_http_server
            .test_login(
                &test_http_server.ca_cert_pem,
                &client_cert,
                &client_key,
                Some("no chain matching all constraints could be found for this login certificate"),
            )
            .unwrap();

        test_http_server.test_write_cert(
            "web",
            &ca_cert,
            "foo",
            &Allowed { dns: "*.example.com".into(), ..Default::default() },
            &Map::new(),
            None,
        );
        let _ = test_http_server
            .test_login(
                &test_http_server.ca_cert_pem,
                &client_cert,
                &client_key,
                Some("no chain matching all constraints could be found for this login certificate"),
            )
            .unwrap();
    }

    #[maybe_async::test(feature = "sync_handler", async(all(not(feature = "sync_handler")), tokio::test))]
    async fn test_credential_cert_module_email_single_cert() {
        let mut test_http_server = TestHttpServer::new("test_credential_cert_module_email_single_cert", true).await;

        // CA cert
        let (ca_cert, ca_key) =
            new_test_cert(true, true, true, "localhost", Some("localhost"), Some("127.0.0.1"), None, None, None, None)
                .unwrap();

        let (client_cert, client_key) = new_test_cert_ext(
            false,
            true,
            true,
            "example.com",
            Some("example.com"),
            Some("127.0.0.1"),
            None,
            None,
            Some(ca_cert.clone()),
            Some(ca_key.clone()),
        )
        .unwrap();

        // mount cert auth to path: auth/cert
        let ret = test_http_server.mount_auth("cert", "cert");
        assert!(ret.is_ok());

        test_http_server.test_write_cert(
            "web",
            &ca_cert,
            "foo",
            &Allowed { emails: "valid@example.com".into(), ..Default::default() },
            &Map::new(),
            None,
        );
        let _ = test_http_server.test_login(&test_http_server.ca_cert_pem, &client_cert, &client_key, None).unwrap();

        test_http_server.test_write_cert(
            "web",
            &ca_cert,
            "foo",
            &Allowed { emails: "*@example.com".into(), ..Default::default() },
            &Map::new(),
            None,
        );
        let _ = test_http_server.test_login(&test_http_server.ca_cert_pem, &client_cert, &client_key, None).unwrap();

        test_http_server.test_write_cert(
            "web",
            &ca_cert,
            "foo",
            &Allowed { emails: "invalid@notincert.com".into(), ..Default::default() },
            &Map::new(),
            None,
        );
        let _ = test_http_server
            .test_login(
                &test_http_server.ca_cert_pem,
                &client_cert,
                &client_key,
                Some("no chain matching all constraints could be found for this login certificate"),
            )
            .unwrap();

        test_http_server.test_write_cert(
            "web",
            &ca_cert,
            "foo",
            &Allowed { emails: "abc".into(), ..Default::default() },
            &Map::new(),
            None,
        );
        let _ = test_http_server
            .test_login(
                &test_http_server.ca_cert_pem,
                &client_cert,
                &client_key,
                Some("no chain matching all constraints could be found for this login certificate"),
            )
            .unwrap();

        test_http_server.test_write_cert(
            "web",
            &ca_cert,
            "foo",
            &Allowed { emails: "*.example.com".into(), ..Default::default() },
            &Map::new(),
            None,
        );
        let _ = test_http_server
            .test_login(
                &test_http_server.ca_cert_pem,
                &client_cert,
                &client_key,
                Some("no chain matching all constraints could be found for this login certificate"),
            )
            .unwrap();
    }

    #[maybe_async::test(feature = "sync_handler", async(all(not(feature = "sync_handler")), tokio::test))]
    async fn test_credential_cert_module_uri_single_cert() {
        let mut test_http_server = TestHttpServer::new("test_credential_cert_module_uri_single_cert", true).await;

        // CA cert
        let (ca_cert, ca_key) =
            new_test_cert(true, true, true, "localhost", Some("localhost"), Some("127.0.0.1"), None, None, None, None)
                .unwrap();

        let (client_cert, client_key) = new_test_cert(
            false,
            true,
            true,
            "example.com",
            Some("example.com"),
            Some("127.0.0.1"),
            Some("spiffe://example.com/host"),
            None,
            Some(ca_cert.clone()),
            Some(ca_key.clone()),
        )
        .unwrap();

        // mount cert auth to path: auth/cert
        let ret = test_http_server.mount_auth("cert", "cert");
        println!("mount ret: {:?}", ret);
        assert!(ret.is_ok());

        test_http_server.test_write_cert(
            "web",
            &ca_cert,
            "foo",
            &Allowed { uris: "spiffe://example.com/*".into(), ..Default::default() },
            &Map::new(),
            None,
        );
        let _ = test_http_server.test_login(&test_http_server.ca_cert_pem, &client_cert, &client_key, None).unwrap();

        test_http_server.test_write_cert(
            "web",
            &ca_cert,
            "foo",
            &Allowed { uris: "spiffe://example.com/host".into(), ..Default::default() },
            &Map::new(),
            None,
        );
        let _ = test_http_server.test_login(&test_http_server.ca_cert_pem, &client_cert, &client_key, None).unwrap();

        test_http_server.test_write_cert(
            "web",
            &ca_cert,
            "foo",
            &Allowed { uris: "spiffe://example.com/invalid".into(), ..Default::default() },
            &Map::new(),
            None,
        );
        let _ = test_http_server
            .test_login(
                &test_http_server.ca_cert_pem,
                &client_cert,
                &client_key,
                Some("no chain matching all constraints could be found for this login certificate"),
            )
            .unwrap();

        test_http_server.test_write_cert(
            "web",
            &ca_cert,
            "foo",
            &Allowed { uris: "abc".into(), ..Default::default() },
            &Map::new(),
            None,
        );
        let _ = test_http_server
            .test_login(
                &test_http_server.ca_cert_pem,
                &client_cert,
                &client_key,
                Some("no chain matching all constraints could be found for this login certificate"),
            )
            .unwrap();

        test_http_server.test_write_cert(
            "web",
            &ca_cert,
            "foo",
            &Allowed { uris: "http://www.google.com".into(), ..Default::default() },
            &Map::new(),
            None,
        );
        let _ = test_http_server
            .test_login(
                &test_http_server.ca_cert_pem,
                &client_cert,
                &client_key,
                Some("no chain matching all constraints could be found for this login certificate"),
            )
            .unwrap();
    }

    #[maybe_async::test(feature = "sync_handler", async(all(not(feature = "sync_handler")), tokio::test))]
    async fn test_credential_cert_module_ou_single_cert() {
        let mut test_http_server = TestHttpServer::new("test_credential_cert_module_ou_single_cert", true).await;

        // CA cert
        let (ca_cert, ca_key) = new_test_cert_ext(
            true,
            true,
            true,
            "localhost",
            Some("localhost"),
            Some("127.0.0.1"),
            None,
            None,
            None,
            None,
        )
        .unwrap();

        // mount cert auth to path: auth/cert
        let ret = test_http_server.mount_auth("cert", "cert");
        assert!(ret.is_ok());

        test_http_server.test_write_cert(
            "web",
            &ca_cert,
            "foo",
            &Allowed { organizational_units: "engineering".into(), ..Default::default() },
            &Map::new(),
            None,
        );
        let _ = test_http_server.test_login(&test_http_server.ca_cert_pem, &ca_cert, &ca_key, None).unwrap();

        test_http_server.test_write_cert(
            "web",
            &ca_cert,
            "foo",
            &Allowed { organizational_units: "eng*".into(), ..Default::default() },
            &Map::new(),
            None,
        );
        let _ = test_http_server.test_login(&test_http_server.ca_cert_pem, &ca_cert, &ca_key, None).unwrap();

        test_http_server.test_write_cert(
            "web",
            &ca_cert,
            "foo",
            &Allowed { organizational_units: "engineering,finance".into(), ..Default::default() },
            &Map::new(),
            None,
        );
        let _ = test_http_server.test_login(&test_http_server.ca_cert_pem, &ca_cert, &ca_key, None).unwrap();

        test_http_server.test_write_cert(
            "web",
            &ca_cert,
            "foo",
            &Allowed { organizational_units: "foo".into(), ..Default::default() },
            &Map::new(),
            None,
        );
        let _ = test_http_server
            .test_login(
                &test_http_server.ca_cert_pem,
                &ca_cert,
                &ca_key,
                Some("no chain matching all constraints could be found for this login certificate"),
            )
            .unwrap();
    }

    #[maybe_async::test(feature = "sync_handler", async(all(not(feature = "sync_handler")), tokio::test))]
    async fn test_credential_cert_module_mixed_constraints() {
        let mut test_http_server = TestHttpServer::new("test_credential_cert_module_mixed_constraints", true).await;

        // CA cert
        let (ca_cert, ca_key) = new_test_cert_ext(
            true,
            true,
            true,
            "localhost",
            Some("localhost"),
            Some("127.0.0.1"),
            None,
            None,
            None,
            None,
        )
        .unwrap();

        let (client_cert, client_key) = new_test_cert(
            false,
            true,
            true,
            "cert.example.com",
            Some("cert.example.com"),
            Some("127.0.0.1"),
            None,
            None,
            Some(ca_cert.clone()),
            Some(ca_key.clone()),
        )
        .unwrap();

        // mount cert auth to path: auth/cert
        let ret = test_http_server.mount_auth("cert", "cert");
        assert!(ret.is_ok());

        test_http_server.test_write_cert("1unconstrained", &ca_cert, "foo", &Allowed::default(), &Map::new(), None);
        test_http_server.test_write_cert(
            "2matching",
            &ca_cert,
            "foo",
            &Allowed { names: "*.example.com,whatever".into(), ..Default::default() },
            &Map::new(),
            None,
        );
        test_http_server.test_write_cert(
            "3invalid",
            &ca_cert,
            "foo",
            &Allowed { names: "invalid".into(), ..Default::default() },
            &Map::new(),
            None,
        );

        let _ = test_http_server.test_login(&test_http_server.ca_cert_pem, &client_cert, &client_key, None).unwrap();
        let _ = test_http_server
            .test_login_with_name("2matching", &test_http_server.ca_cert_pem, &client_cert, &client_key, None)
            .unwrap();
        let _ = test_http_server
            .test_login_with_name(
                "3invalid",
                &test_http_server.ca_cert_pem,
                &client_cert,
                &client_key,
                Some("no chain matching all constraints could be found for this login certificate"),
            )
            .unwrap();
    }

    #[maybe_async::test(feature = "sync_handler", async(all(not(feature = "sync_handler")), tokio::test))]
    async fn test_credential_cert_module_untrusted() {
        let mut test_http_server = TestHttpServer::new("test_credential_cert_module_untrusted", true).await;

        // CA cert
        let (ca_cert, ca_key) = new_test_cert_ext(
            true,
            true,
            true,
            "localhost",
            Some("localhost"),
            Some("127.0.0.1"),
            None,
            None,
            None,
            None,
        )
        .unwrap();

        let (client_cert, client_key) = new_test_cert(
            false,
            true,
            true,
            "cert.example.com",
            Some("cert.example.com"),
            Some("127.0.0.1"),
            None,
            None,
            Some(ca_cert.clone()),
            Some(ca_key.clone()),
        )
        .unwrap();

        // mount cert auth to path: auth/cert
        let ret = test_http_server.mount_auth("cert", "cert");
        assert!(ret.is_ok());

        let _ = test_http_server
            .test_login(
                &test_http_server.ca_cert_pem,
                &client_cert,
                &client_key,
                Some("invalid certificate or no client certificate supplied"),
            )
            .unwrap();
    }

    #[maybe_async::test(feature = "sync_handler", async(all(not(feature = "sync_handler")), tokio::test))]
    async fn test_credential_cert_module_valid_cidr() {
        let mut test_http_server = TestHttpServer::new("test_credential_cert_module_valid_cidr", true).await;

        // CA cert
        let (ca_cert, ca_key) =
            new_test_cert(true, true, true, "localhost", Some("localhost"), Some("127.0.0.1"), None, None, None, None)
                .unwrap();

        let (client_cert, client_key) = new_test_cert(
            false,
            true,
            true,
            "cert.example.com",
            Some("cert.example.com"),
            Some("127.0.0.1"),
            None,
            None,
            Some(ca_cert.clone()),
            Some(ca_key.clone()),
        )
        .unwrap();

        // mount cert auth to path: auth/cert
        let ret = test_http_server.mount_auth("cert", "cert");
        assert!(ret.is_ok());

        test_http_server.test_write_cert(
            "web",
            &ca_cert,
            "foo",
            &Allowed::default(),
            json!({"bound_cidrs": ["127.0.0.1", "128.252.0.0/16"]}).as_object().unwrap(),
            None,
        );
        let (_, resp) = test_http_server.test_read_cert("web").unwrap();
        assert_eq!(resp["data"]["bound_cidrs"], json!(["127.0.0.1", "128.252.0.0/16"]));
        assert_eq!(resp["data"]["token_bound_cidrs"], json!(["127.0.0.1", "128.252.0.0/16"]));
        let _ = test_http_server.test_login(&test_http_server.ca_cert_pem, &client_cert, &client_key, None).unwrap();
    }

    #[maybe_async::test(feature = "sync_handler", async(all(not(feature = "sync_handler")), tokio::test))]
    async fn test_credential_cert_module_invalid_cidr() {
        let mut test_http_server = TestHttpServer::new("test_credential_cert_module_invalid_cidr", true).await;

        // CA cert
        let (ca_cert, ca_key) =
            new_test_cert(true, true, true, "localhost", Some("localhost"), Some("127.0.0.1"), None, None, None, None)
                .unwrap();

        let (client_cert, client_key) = new_test_cert(
            false,
            true,
            true,
            "cert.example.com",
            Some("cert.example.com"),
            Some("127.0.0.1"),
            None,
            None,
            Some(ca_cert.clone()),
            Some(ca_key.clone()),
        )
        .unwrap();

        // mount cert auth to path: auth/cert
        let ret = test_http_server.mount_auth("cert", "cert");
        assert!(ret.is_ok());

        test_http_server.test_write_cert(
            "web",
            &ca_cert,
            "foo",
            &Allowed::default(),
            json!({"bound_cidrs": ["127.0.0.2", "128.252.0.0/16"]}).as_object().unwrap(),
            None,
        );
        let (_, resp) = test_http_server.test_read_cert("web").unwrap();
        assert_eq!(resp["data"]["bound_cidrs"], json!(["127.0.0.2", "128.252.0.0/16"]));
        assert_eq!(resp["data"]["token_bound_cidrs"], json!(["127.0.0.2", "128.252.0.0/16"]));
        let _ = test_http_server
            .test_login(&test_http_server.ca_cert_pem, &client_cert, &client_key, Some("Permission denied."))
            .unwrap();
    }
}
