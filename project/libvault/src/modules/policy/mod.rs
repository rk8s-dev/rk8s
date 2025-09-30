use std::{any::Any, str::FromStr, sync::Arc};

use arc_swap::ArcSwap;
use base64::{engine::general_purpose::STANDARD, Engine as _};
use better_default::Default;
use serde_json::{Map, Value};

use super::Module;
use crate::{
    core::Core,
    errors::RvError,
    handler::AuthHandler,
    logical::{Backend, Request, Response},
    rv_error_response_status,
};

#[allow(clippy::module_inception)]
pub mod policy;
pub use policy::{Permissions, Policy, PolicyPathRules, PolicyType};

pub mod policy_store;
pub use policy_store::PolicyStore;

pub mod acl;

#[derive(Default)]
pub struct PolicyModule {
    #[default("policy".into())]
    pub name: String,
    pub core: Arc<Core>,
    pub policy_store: ArcSwap<PolicyStore>,
}

#[maybe_async::maybe_async]
impl PolicyModule {
    pub fn new(core: Arc<Core>) -> Self {
        Self { name: "policy".into(), core, policy_store: ArcSwap::new(Arc::new(PolicyStore::default())) }
    }

    pub async fn setup_policy(&self) -> Result<(), RvError> {
        self.policy_store.load().load_default_acl_policy().await
    }

    pub async fn handle_policy_list(
        &self,
        _backend: &dyn Backend,
        req: &mut Request,
    ) -> Result<Option<Response>, RvError> {
        let mut policies = self.policy_store.load().list_policy(PolicyType::Acl).await?;

        // TODO: After the "namespace" feature is added here, it is necessary to determine whether it is the root
        // namespace before the root can be added.
        policies.push("root".into());

        let mut resp = Response::list_response(&policies);

        if req.path.starts_with("policy") {
            let data = resp.data.as_mut().unwrap();
            data.insert("policies".into(), data["keys"].clone());
        }
        Ok(Some(resp))
    }

    pub async fn handle_policy_read(
        &self,
        _backend: &dyn Backend,
        req: &mut Request,
    ) -> Result<Option<Response>, RvError> {
        let name = req.get_data_as_str("name")?;
        if let Some(policy) = self.policy_store.load().get_policy(&name, PolicyType::Acl).await? {
            let mut resp_data = Map::new();
            resp_data.insert("name".into(), Value::String(name));

            // If the request is from sys/policy/ we handle backwards compatibility
            if req.path.starts_with("policy") {
                resp_data.insert("rules".into(), Value::String(policy.raw.clone()));
            } else {
                resp_data.insert("policy".into(), Value::String(policy.raw.clone()));
            }

            let resp = Response::data_response(Some(resp_data));
            if policy.policy_type == PolicyType::Egp || policy.policy_type == PolicyType::Rgp {
                policy.add_sentinel_policy_data(&resp)?;
            }

            return Ok(Some(resp));
        }
        Err(rv_error_response_status!(404, &format!("No policy named: {name}")))
    }

    pub async fn handle_policy_write(
        &self,
        _backend: &dyn Backend,
        req: &mut Request,
    ) -> Result<Option<Response>, RvError> {
        let name = req.get_data_as_str("name")?;
        let policy_str = req.get_data("policy")?.as_str().ok_or(RvError::ErrRequestFieldInvalid)?.to_string();
        let policy_raw = if let Ok(policy_bytes) = STANDARD.decode(&policy_str) {
            String::from_utf8_lossy(&policy_bytes).to_string()
        } else {
            policy_str
        };

        let mut policy = Policy::from_str(&policy_raw)?;
        policy.name = name;

        if policy.policy_type == PolicyType::Egp || policy.policy_type == PolicyType::Rgp {
            policy.input_sentinel_policy_data(req)?;
        }

        self.policy_store.load().set_policy(policy).await?;

        Ok(None)
    }

    pub async fn handle_policy_delete(
        &self,
        _backend: &dyn Backend,
        req: &mut Request,
    ) -> Result<Option<Response>, RvError> {
        let name = req.get_data_as_str("name")?;
        self.policy_store.load().delete_policy(&name, PolicyType::Acl).await?;
        Ok(None)
    }
}

#[maybe_async::maybe_async]
impl Module for PolicyModule {
    fn name(&self) -> String {
        self.name.clone()
    }

    fn as_any_arc(self: Arc<Self>) -> Arc<dyn Any + Send + Sync> {
        self
    }

    fn setup(&self, _core: &Core) -> Result<(), RvError> {
        Ok(())
    }

    async fn init(&self, core: &Core) -> Result<(), RvError> {
        let policy_store = PolicyStore::new(core).await?;
        self.policy_store.store(policy_store.clone());

        self.setup_policy().await?;

        core.add_auth_handler(policy_store as Arc<dyn AuthHandler>)?;

        Ok(())
    }

    fn cleanup(&self, core: &Core) -> Result<(), RvError> {
        core.delete_auth_handler(self.policy_store.load().clone() as Arc<dyn AuthHandler>)?;
        let policy_store = Arc::new(PolicyStore::default());
        self.policy_store.swap(policy_store);
        Ok(())
    }
}

#[cfg(test)]
mod mod_policy_tests {
    use policy_store::DEFAULT_POLICY;
    use serde_json::json;

    use super::*;
    use crate::{
        logical::{Operation, Request},
        test_utils::{
            new_unseal_test_rusty_vault, test_delete_api, test_list_api, test_mount_api, test_mount_auth_api,
            test_read_api, test_write_api, TestHttpServer,
        },
    };

    #[maybe_async::maybe_async]
    async fn test_write_policy(core: &Core, token: &str, name: &str, policy: &str) {
        let data = json!({
            "policy": policy,
        })
        .as_object()
        .cloned();

        let resp = test_write_api(core, token, format!("sys/policy/{}", name).as_str(), true, data).await;
        assert!(resp.is_ok());
    }

    #[maybe_async::maybe_async]
    async fn test_read_policy(core: &Core, token: &str, name: &str) -> Result<Option<Response>, RvError> {
        let resp = test_read_api(core, token, format!("sys/policy/{}", name).as_str(), true).await;
        assert!(resp.is_ok());
        resp
    }

    #[maybe_async::maybe_async]
    async fn test_delete_policy(core: &Core, token: &str, name: &str) {
        let resp = test_delete_api(core, token, format!("sys/policy/{}", name).as_str(), true, None).await;
        assert!(resp.is_ok());
    }

    #[maybe_async::maybe_async]
    async fn test_write_user(
        core: &Core,
        token: &str,
        path: &str,
        username: &str,
        password: &str,
        policy: &str,
        ttl: i32,
    ) {
        let user_data = json!({
            "password": password,
            "token_policies": policy,
            "ttl": ttl,
        })
        .as_object()
        .cloned();

        let resp =
            test_write_api(core, token, format!("auth/{}/users/{}", path, username).as_str(), true, user_data).await;
        assert!(resp.is_ok());
    }

    #[maybe_async::maybe_async]
    async fn test_user_login(
        core: &Core,
        path: &str,
        username: &str,
        password: &str,
        is_ok: bool,
    ) -> Result<Option<Response>, RvError> {
        let login_data = json!({
            "password": password,
        })
        .as_object()
        .cloned();

        let mut req = Request::new(format!("auth/{}/login/{}", path, username).as_str());
        req.operation = Operation::Write;
        req.body = login_data;

        let resp = core.handle_request(&mut req).await;
        assert!(resp.is_ok());
        if is_ok {
            let resp = resp.as_ref().unwrap();
            assert!(resp.is_some());
        }
        resp
    }

    #[maybe_async::test(feature = "sync_handler", async(all(not(feature = "sync_handler")), tokio::test))]
    async fn test_policy_curd_api() {
        let (_rvault, core, root_token) = new_unseal_test_rusty_vault("test_policy_curd_api").await;

        let policy1_name = "policy1";
        let policy1_hcl = r#"
            path "path1/" {
                capabilities = ["read"]
            }
        "#;

        // Write
        test_write_policy(&core, &root_token, policy1_name, policy1_hcl).await;

        // Read
        let policy1 = test_read_policy(&core, &root_token, policy1_name).await;
        assert!(policy1.is_ok());
        let policy1 = policy1.unwrap();
        assert!(policy1.is_some());
        let policy1 = policy1.unwrap();
        assert!(policy1.data.is_some());
        let policy1 = policy1.data.unwrap();
        assert_eq!(policy1["name"], policy1_name);
        assert_eq!(policy1["rules"], policy1_hcl);

        // List
        let policies = test_list_api(&core, &root_token, "sys/policy", true).await;
        assert!(policies.is_ok());
        let policies = policies.unwrap();
        assert!(policies.is_some());
        let policies = policies.unwrap();
        assert!(policies.data.is_some());
        let policies = policies.data.unwrap();
        assert_eq!(policies["keys"], json!(["default", policy1_name, "root"]));
        assert_eq!(policies["policies"], json!(["default", policy1_name, "root"]));

        // Delete
        test_delete_policy(&core, &root_token, policy1_name).await;

        // Read again
        let policy1 = test_read_api(&core, &root_token, format!("sys/policy/{}", policy1_name).as_str(), false).await;
        let policy1 = policy1.unwrap_err();
        assert!(policy1.to_string().contains("status: 404,"));
        assert!(policy1.to_string().contains("No policy named: "));
        assert!(policy1.to_string().contains(policy1_name));

        // List again
        let policies = test_list_api(&core, &root_token, "sys/policy", true).await;
        let policies = policies.unwrap().unwrap().data.unwrap();
        assert_eq!(policies["keys"], json!(["default", "root"]));
        assert_eq!(policies["policies"], json!(["default", "root"]));
    }

    #[maybe_async::test(feature = "sync_handler", async(all(not(feature = "sync_handler")), tokio::test))]
    async fn test_policy_http_api() {
        let mut test_http_server = TestHttpServer::new("test_policy_http_api", true).await;

        // set token
        test_http_server.token = test_http_server.root_token.clone();

        // List policies
        let ret = test_http_server.read("sys/policy", None);
        assert!(ret.is_ok());
        assert_eq!(ret.unwrap().1, json!({"keys": ["default", "root"], "policies": ["default", "root"]}));

        // Read default policy
        let ret = test_http_server.read("sys/policy/default", None);
        assert!(ret.is_ok());
        assert_eq!(ret.unwrap().1, json!({"name": "default", "rules": DEFAULT_POLICY}));

        // Write policy1
        let policy1_hcl = r#"
            path "path1/" {
                capabilities = ["read"]
            }
        "#;
        let data = json!({
            "policy": policy1_hcl,
        })
        .as_object()
        .cloned();
        let ret = test_http_server.write("sys/policy/policy1", data, None);
        assert!(ret.is_ok());

        // Read policy1
        let ret = test_http_server.read("sys/policy/policy1", None);
        assert!(ret.is_ok());
        assert_eq!(ret.unwrap().1, json!({"name": "policy1", "rules": policy1_hcl}));

        // List policies again
        let ret = test_http_server.read("sys/policy", None);
        assert!(ret.is_ok());
        assert_eq!(
            ret.unwrap().1,
            json!({"keys": ["default", "policy1", "root"], "policies": ["default", "policy1", "root"]})
        );

        // Delete policy1
        let ret = test_http_server.delete("sys/policy/policy1", None, None);
        assert!(ret.is_ok());

        // List policies again
        let ret = test_http_server.read("sys/policy", None);
        assert!(ret.is_ok());
        assert_eq!(ret.unwrap().1, json!({"keys": ["default", "root"], "policies": ["default", "root"]}));
    }

    #[maybe_async::test(feature = "sync_handler", async(all(not(feature = "sync_handler")), tokio::test))]
    async fn test_policy_acl_check() {
        let (_rvault, core, root_token) = new_unseal_test_rusty_vault("test_policy_acl_check").await;

        let policy1_name = "policy1";
        let policy1_hcl = r#"
            path "path1/*" {
                capabilities = ["read"]
            }

            path "path1/kv1" {
                capabilities = ["read", "list", "create", "update", "delete"]
            }
        "#;
        let policy2_name = "policy2";
        let policy2_hcl = r#"
            path "path1/*" {
                capabilities = ["read", "list", "create", "update"]
            }
        "#;

        // Write
        test_write_policy(&core, &root_token, policy1_name, policy1_hcl).await;
        test_write_policy(&core, &root_token, policy2_name, policy2_hcl).await;

        // Mount userpass auth
        test_mount_auth_api(&core, &root_token, "userpass", "up1").await;

        // Add user xxx with policy1, add user yyy with policy2
        test_write_user(&core, &root_token, "up1", "xxx", "123qwe!@#", policy1_name, 0).await;
        let resp = test_user_login(&core, "up1", "xxx", "123qwe!@#", true).await;
        assert!(resp.is_ok());
        let xxx_token = resp.unwrap().unwrap().auth.unwrap().client_token;
        test_write_user(&core, &root_token, "up1", "yyy", "123456", policy2_name, 0).await;
        let resp = test_user_login(&core, "up1", "yyy", "123456", true).await;
        assert!(resp.is_ok());
        let yyy_token = resp.unwrap().unwrap().auth.unwrap().client_token;

        // Mount kv to path1/ and path2/
        test_mount_api(&core, &root_token, "kv", "path1/").await;
        test_mount_api(&core, &root_token, "kv", "path2/").await;

        // User xxx write path path1/kv1 should succeed
        let data = json!({
            "aa": "bb",
        })
        .as_object()
        .cloned();
        let resp = test_write_api(&core, &xxx_token, "path1/kv1", true, data.clone()).await;
        assert!(resp.is_ok());

        // User xxx write path1/kv2 should fail
        let resp = test_write_api(&core, &xxx_token, "path1/kv2", false, data.clone()).await;
        assert!(resp.is_err());

        // User yyy write path1/kv2 should succeed
        let resp = test_write_api(&core, &yyy_token, "path1/kv2", true, data).await;
        assert!(resp.is_ok());

        // User xxx read path1/kv1 should succeed
        let resp = test_read_api(&core, &xxx_token, "path1/kv1", true).await;
        assert!(resp.is_ok());

        // User xxx read path1/kv2 should succeed
        let resp = test_read_api(&core, &xxx_token, "path1/kv2", true).await;
        assert!(resp.is_ok());

        // User yyy read path1/kv1 should succeed
        let resp = test_read_api(&core, &yyy_token, "path1/kv1", true).await;
        assert!(resp.is_ok());

        // User yyy read path1/kv2 should succeed
        let resp = test_read_api(&core, &yyy_token, "path1/kv2", true).await;
        assert!(resp.is_ok());

        // User xxx list path1/ should fail
        let resp = test_list_api(&core, &xxx_token, "path1", false).await;
        assert!(resp.is_err());

        // User yyy list path1/ should fail
        let resp = test_list_api(&core, &yyy_token, "path1", false).await;
        assert!(resp.is_err());

        // User yyy delete path1/kv1 should fail
        let resp = test_delete_api(&core, &yyy_token, "path1/kv1", false, None).await;
        assert!(resp.is_err());

        // User yyy delete path1/kv2 should fail
        let resp = test_delete_api(&core, &yyy_token, "path1/kv2", false, None).await;
        assert!(resp.is_err());
    }

    #[maybe_async::test(feature = "sync_handler", async(all(not(feature = "sync_handler")), tokio::test))]
    async fn test_policy_acl_check_with_policy_parameters() {
        let (_rvault, core, root_token) =
            new_unseal_test_rusty_vault("test_policy_acl_check_with_policy_parameters").await;

        let policy1_name = "policy1";
        let policy1_hcl = r#"
            path "path1/*" {
                capabilities = ["read", "list"]
            }

            path "path1/kv1" {
                capabilities = ["read", "list", "create", "update", "delete"]
                allowed_parameters = {"key1" = ["value1", "value2"], "key2" = ["value3", "value4"]}
                required_parameters = ["key1"]
            }

            path "path1/kv2" {
                capabilities = ["read", "list", "create", "update"]
                required_parameters = ["key1", "key2", "key3"]
            }

            path "path1/kv3" {
                capabilities = ["read", "list", "create", "update"]
                denied_parameters = {"*" = []}
            }

            path "path1/kv4" {
                capabilities = ["read", "list", "create", "update"]
                denied_parameters = {"key2" = ["value3", "value4"]}
            }
        "#;

        // Write
        test_write_policy(&core, &root_token, policy1_name, policy1_hcl).await;

        // Mount userpass auth
        test_mount_auth_api(&core, &root_token, "userpass", "up1").await;

        // Add user xxx with policy1
        test_write_user(&core, &root_token, "up1", "xxx", "123qwe!@#", policy1_name, 0).await;
        let resp = test_user_login(&core, "up1", "xxx", "123qwe!@#", true).await;
        assert!(resp.is_ok());
        let xxx_token = resp.unwrap().unwrap().auth.unwrap().client_token;

        // Mount kv to path1/ and path2/
        test_mount_api(&core, &root_token, "kv", "path1/").await;

        // User xxx write path path1/kv1 with parameters key1=value1 should succeed
        let data = json!({
            "key1": "value1",
        })
        .as_object()
        .cloned();
        let _ = test_write_api(&core, &xxx_token, "path1/kv1", true, data).await;

        // User xxx write path path1/kv1 with parameters key1=value2 should succeed
        let data = json!({
            "key1": "value2",
        })
        .as_object()
        .cloned();
        let _ = test_write_api(&core, &xxx_token, "path1/kv1", true, data.clone()).await;

        // User xxx write path1/kv2 should fail
        let _ = test_write_api(&core, &xxx_token, "path1/kv2", false, data).await;

        // User xxx write path path1/kv1 with parameters key1=value1 and key2=value3 should succeed
        let data = json!({
            "key1": "value1",
            "key2": "value3",
        })
        .as_object()
        .cloned();
        let _ = test_write_api(&core, &xxx_token, "path1/kv1", true, data).await;

        // User xxx write path path1/kv1 with parameters key1=value1 and key2=value4 should succeed
        let data = json!({
            "key1": "value1",
            "key2": "value4",
        })
        .as_object()
        .cloned();
        let _ = test_write_api(&core, &xxx_token, "path1/kv1", true, data).await;

        // User xxx write path path1/kv1 with parameters key1=value2 and key2=value22 should fail
        let data = json!({
            "key1": "value2",
            "key2": "value22",
        })
        .as_object()
        .cloned();
        let _ = test_write_api(&core, &xxx_token, "path1/kv1", false, data).await;

        // User xxx read path1/kv1 without parameters should fail
        let _ = test_read_api(&core, &xxx_token, "path1/kv1", false).await;

        // User xxx list path1/ should fail
        let _ = test_list_api(&core, &xxx_token, "path1", false).await;

        // User xxx write path path1/kv1 with parameters key1=value3 should fail
        let data = json!({
            "key1": "value3",
        })
        .as_object()
        .cloned();
        let _ = test_write_api(&core, &xxx_token, "path1/kv1", false, data).await;

        // User xxx write path path1/kv1 with parameters key2=value3 (missing key1) should fail
        let data = json!({
            "key2": "value3",
        })
        .as_object()
        .cloned();
        let _ = test_write_api(&core, &xxx_token, "path1/kv1", false, data).await;

        // User xxx write path path1/kv2 with parameters key1 (missing key2 and key3) should fail
        let data = json!({
            "key1": "xx",
        })
        .as_object()
        .cloned();
        let _ = test_write_api(&core, &xxx_token, "path1/kv2", false, data).await;

        // User xxx write path path1/kv2 with parameters key1 and key2 (missing key3) should fail
        let data = json!({
            "key1": "xx",
            "key2": "yy",
        })
        .as_object()
        .cloned();
        let _ = test_write_api(&core, &xxx_token, "path1/kv2", false, data).await;

        // User xxx write path path1/kv2 with parameters key1、key2 and key3 should succeed
        let data = json!({
            "key1": "xx",
            "key2": "yy",
            "key3": "zz",
        })
        .as_object()
        .cloned();
        let _ = test_write_api(&core, &xxx_token, "path1/kv2", true, data).await;

        // User xxx write path path1/kv2 with parameters key1、key2、key3 and other param should succeed
        let data = json!({
            "key1": "xx",
            "key2": "yy",
            "key3": "zz",
            "key4": "vv",
        })
        .as_object()
        .cloned();
        let _ = test_write_api(&core, &xxx_token, "path1/kv2", true, data).await;

        // User xxx write path path1/kv3 with parameters key1 should fail
        let data = json!({
            "key1": "xx",
        })
        .as_object()
        .cloned();
        let _ = test_write_api(&core, &xxx_token, "path1/kv3", false, data).await;

        // User xxx write path path1/kv3 with parameters key1 should fail
        let data = json!({
            "key1": "xx",
            "key2": "yy"
        })
        .as_object()
        .cloned();
        let _ = test_write_api(&core, &xxx_token, "path1/kv3", false, data).await;

        // User xxx write path path1/kv4 with parameters key1 should succeed
        let data = json!({
            "key1": "xx"
        })
        .as_object()
        .cloned();
        let _ = test_write_api(&core, &xxx_token, "path1/kv4", true, data).await;

        // User xxx write path path1/kv4 with parameters key1 and key2=yy should succeed
        let data = json!({
            "key1": "xx",
            "key2": "yy"
        })
        .as_object()
        .cloned();
        let _ = test_write_api(&core, &xxx_token, "path1/kv4", true, data).await;

        // User xxx write path path1/kv4 with parameters key1 and key2=value3 should succeed
        let data = json!({
            "key1": "xx",
            "key2": "value3"
        })
        .as_object()
        .cloned();
        let _ = test_write_api(&core, &xxx_token, "path1/kv4", false, data).await;

        // User xxx write path path1/kv4 with parameters key1 and key2=value4 should succeed
        let data = json!({
            "key1": "xx",
            "key2": "value4"
        })
        .as_object()
        .cloned();
        let _ = test_write_api(&core, &xxx_token, "path1/kv4", false, data).await;
    }
}
