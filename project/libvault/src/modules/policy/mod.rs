use std::{any::Any, str::FromStr, sync::Arc};

use arc_swap::ArcSwap;
use base64::{Engine as _, engine::general_purpose::STANDARD};
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
        Self {
            name: "policy".into(),
            core,
            policy_store: ArcSwap::new(Arc::new(PolicyStore::default())),
        }
    }

    pub async fn setup_policy(&self) -> Result<(), RvError> {
        self.policy_store.load().load_default_acl_policy().await
    }

    pub async fn handle_policy_list(
        &self,
        _backend: &dyn Backend,
        req: &mut Request,
    ) -> Result<Option<Response>, RvError> {
        let mut policies = self
            .policy_store
            .load()
            .list_policy(PolicyType::Acl)
            .await?;

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
        if let Some(policy) = self
            .policy_store
            .load()
            .get_policy(&name, PolicyType::Acl)
            .await?
        {
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
        Err(rv_error_response_status!(
            404,
            &format!("No policy named: {name}")
        ))
    }

    pub async fn handle_policy_write(
        &self,
        _backend: &dyn Backend,
        req: &mut Request,
    ) -> Result<Option<Response>, RvError> {
        let name = req.get_data_as_str("name")?;
        let policy_str = req
            .get_data("policy")?
            .as_str()
            .ok_or(RvError::ErrRequestFieldInvalid)?
            .to_string();
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
        self.policy_store
            .load()
            .delete_policy(&name, PolicyType::Acl)
            .await?;
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
    use crate::test_utils::{
        TestHttpServer, new_unseal_test_rusty_vault, test_delete_api, test_list_api, test_read_api,
        test_write_api,
    };

    #[maybe_async::maybe_async]
    async fn test_write_policy(core: &Core, token: &str, name: &str, policy: &str) {
        let data = json!({
            "policy": policy,
        })
        .as_object()
        .cloned();

        let resp = test_write_api(
            core,
            token,
            format!("sys/policy/{}", name).as_str(),
            true,
            data,
        )
        .await;
        assert!(resp.is_ok());
    }

    #[maybe_async::maybe_async]
    async fn test_read_policy(
        core: &Core,
        token: &str,
        name: &str,
    ) -> Result<Option<Response>, RvError> {
        let resp = test_read_api(core, token, format!("sys/policy/{}", name).as_str(), true).await;
        assert!(resp.is_ok());
        resp
    }

    #[maybe_async::maybe_async]
    async fn test_delete_policy(core: &Core, token: &str, name: &str) {
        let resp = test_delete_api(
            core,
            token,
            format!("sys/policy/{}", name).as_str(),
            true,
            None,
        )
        .await;
        assert!(resp.is_ok());
    }

    #[tokio::test]
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
        assert_eq!(
            policies["policies"],
            json!(["default", policy1_name, "root"])
        );

        // Delete
        test_delete_policy(&core, &root_token, policy1_name).await;

        // Read again
        let policy1 = test_read_api(
            &core,
            &root_token,
            format!("sys/policy/{}", policy1_name).as_str(),
            false,
        )
        .await;
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

    #[tokio::test]
    async fn test_policy_http_api() {
        let mut test_http_server = TestHttpServer::new("test_policy_http_api", true).await;

        // set token
        test_http_server.token = test_http_server.root_token.clone();

        // List policies
        let ret = test_http_server.read("sys/policy", None);
        assert!(ret.is_ok());
        assert_eq!(
            ret.unwrap().1,
            json!({"keys": ["default", "root"], "policies": ["default", "root"]})
        );

        // Read default policy
        let ret = test_http_server.read("sys/policy/default", None);
        assert!(ret.is_ok());
        assert_eq!(
            ret.unwrap().1,
            json!({"name": "default", "rules": DEFAULT_POLICY})
        );

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
        assert_eq!(
            ret.unwrap().1,
            json!({"name": "policy1", "rules": policy1_hcl})
        );

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
        assert_eq!(
            ret.unwrap().1,
            json!({"keys": ["default", "root"], "policies": ["default", "root"]})
        );
    }
}
