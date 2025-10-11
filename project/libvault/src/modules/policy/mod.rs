use std::{any::Any, str::FromStr, sync::Arc};

use arc_swap::ArcSwap;
use async_trait::async_trait;
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

#[async_trait]
impl Module for PolicyModule {
    fn name(&self) -> String {
        self.name.clone()
    }

    fn as_any_arc(self: Arc<Self>) -> Arc<dyn Any + Send + Sync> {
        self
    }

    async fn init(&self, core: &Core) -> Result<(), RvError> {
        let policy_store = PolicyStore::new(core).await?;
        self.policy_store.store(policy_store.clone());

        self.setup_policy().await?;

        core.add_auth_handler(policy_store as Arc<dyn AuthHandler>)?;

        Ok(())
    }

    fn setup(&self, _core: &Core) -> Result<(), RvError> {
        Ok(())
    }

    fn cleanup(&self, core: &Core) -> Result<(), RvError> {
        core.delete_auth_handler(self.policy_store.load().clone() as Arc<dyn AuthHandler>)?;
        let policy_store = Arc::new(PolicyStore::default());
        self.policy_store.swap(policy_store);
        Ok(())
    }
}
