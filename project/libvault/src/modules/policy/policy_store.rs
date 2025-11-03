//! The `policy_store.rs` file manages the storage and retrieval of security policies. It provides
//! mechanisms for setting, getting, listing, and deleting policies. This is crucial in systems
//! that rely on policy-based access controls.
//!
//! The main components include:
//! - PolicyEntry: Represents an individual policy with metadata.
//! - PolicyStore: Manages the lifecycle of policies, including caching and storage.
//!
//! Key functionality includes:
//! - Creation and management of ACL (Access Control List), RGP, and EGP policies.
//! - Policy caching to improve access speed.
//! - Methods to handle CRUD operations on policies.
//!
//! External dependencies:
//! - Uses `stretto` for caching and `dashmap` for concurrent collections.
//!
//! Note:
//! - The code includes placeholder functions (e.g., `handle_sentinel_policy`) intended for future implementation.
//! - The design assumes a highly concurrent environment, where caching is critical.

use std::{
    str::FromStr,
    sync::{Arc, Weak},
};

use better_default::Default;
use dashmap::DashMap;
use lazy_static::lazy_static;
use serde::{Deserialize, Serialize};
use stretto::Cache;

use super::{
    Policy, PolicyType,
    acl::{ACL, ACLResults},
    policy::SentinelPolicy,
};
use crate::{
    core::Core,
    errors::RvError,
    handler::AuthHandler,
    logical::{Operation, Request, auth::PolicyResults},
    router::Router,
    rv_error_response_status, rv_error_string,
    storage::{Storage, StorageEntry, barrier_view::BarrierView},
};

// POLICY_ACL_SUB_PATH is the sub-path used for the policy store view. This is
// nested under the system view. POLICY_RGP_SUB_PATH/POLICY_EGP_SUB_PATH are
// similar but for RGPs/EGPs.
const POLICY_ACL_SUB_PATH: &str = "policy/";
const POLICY_RGP_SUB_PATH: &str = "policy-rgp/";
const POLICY_EGP_SUB_PATH: &str = "policy-egp/";

// POLICY_CACHE_SIZE is the number of policies that are kept cached
const POLICY_CACHE_SIZE: usize = 1024;

// DEFAULT_POLICY_NAME is the name of the default policy
const DEFAULT_POLICY_NAME: &str = "default";
pub static DEFAULT_POLICY: &str = r#"
# Allow tokens to look up their own properties
path "auth/token/lookup-self" {
    capabilities = ["read"]
}

# Allow tokens to renew themselves
path "auth/token/renew-self" {
    capabilities = ["update"]
}

# Allow tokens to revoke themselves
path "auth/token/revoke-self" {
    capabilities = ["update"]
}

# Allow a token to look up its own capabilities on a path
path "sys/capabilities-self" {
    capabilities = ["update"]
}

# Allow a token to look up its own entity by id or name
path "identity/entity/id/{{identity.entity.id}}" {
  capabilities = ["read"]
}
path "identity/entity/name/{{identity.entity.name}}" {
  capabilities = ["read"]
}


# Allow a token to look up its resultant ACL from all policies. This is useful
# for UIs. It is an internal path because the format may change at any time
# based on how the internal ACL features and capabilities change.
path "sys/internal/ui/resultant-acl" {
    capabilities = ["read"]
}

# Allow a token to renew a lease via lease_id in the request body; old path for
# old clients, new path for newer
path "sys/renew" {
    capabilities = ["update"]
}
path "sys/leases/renew" {
    capabilities = ["update"]
}

# Allow looking up lease properties. This requires knowing the lease ID ahead
# of time and does not divulge any sensitive information.
path "sys/leases/lookup" {
    capabilities = ["update"]
}

# Allow a token to manage its own cubbyhole
path "cubbyhole/*" {
    capabilities = ["create", "read", "update", "delete", "list"]
}

# Allow a token to wrap arbitrary values in a response-wrapping token
path "sys/wrapping/wrap" {
    capabilities = ["update"]
}

# Allow a token to look up the creation time and TTL of a given
# response-wrapping token
path "sys/wrapping/lookup" {
    capabilities = ["update"]
}

# Allow a token to unwrap a response-wrapping token. This is a convenience to
# avoid client token swapping since this is also part of the response wrapping
# policy.
path "sys/wrapping/unwrap" {
    capabilities = ["update"]
}

# Allow general purpose tools
path "sys/tools/hash" {
    capabilities = ["update"]
}
path "sys/tools/hash/*" {
    capabilities = ["update"]
}

# Allow checking the status of a Control Group request if the user has the
# accessor
path "sys/control-group/request" {
    capabilities = ["update"]
}
"#;

static RESPONSE_WRAPPING_POLICY_NAME: &str = "response-wrapping";
static RESPONSE_WRAPPING_POLICY: &str = r#"
path "cubbyhole/response" {
    capabilities = ["create", "read"]
}

path "sys/wrapping/unwrap" {
    capabilities = ["update"]
}
"#;

static CONTROL_GROUP_POLICY_NAME: &str = "control-group";
static CONTROL_GROUP_POLICY: &str = r#"
path "cubbyhole/control-group" {
    capabilities = ["update", "create", "read"]
}

path "sys/wrapping/unwrap" {
    capabilities = ["update"]
}
"#;

static _POLICY_STORE_HELP: &str = r#"
TODO
"#;

lazy_static! {
    pub static ref IMMUTABLE_POLICIES: Vec<&'static str> = vec![
        "root",
        RESPONSE_WRAPPING_POLICY_NAME,
        CONTROL_GROUP_POLICY_NAME,
    ];
    pub static ref NON_ASSIGNABLE_POLICIES: Vec<&'static str> =
        vec![RESPONSE_WRAPPING_POLICY_NAME, CONTROL_GROUP_POLICY_NAME,];
}

/// Represents a policy entry in the policy store.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct PolicyEntry {
    pub version: u32,
    pub raw: String,
    pub templated: bool,
    #[default(PolicyType::Acl)]
    #[serde(rename = "type")]
    pub policy_type: PolicyType,
    #[serde(default)]
    pub sentinel_policy: SentinelPolicy,
}

/// The main policy store structure.
#[derive(Default)]
pub struct PolicyStore {
    pub router: Arc<Router>,
    pub acl_view: Option<Arc<BarrierView>>,
    pub rgp_view: Option<Arc<BarrierView>>,
    pub egp_view: Option<Arc<BarrierView>>,
    pub token_policies_lru: Option<Cache<String, Arc<Policy>>>,
    pub egp_lru: Option<Cache<String, Arc<Policy>>>,
    // Stores whether a token policy is ACL or RGP
    pub policy_type_map: DashMap<String, PolicyType>,
    pub self_ptr: Weak<PolicyStore>,
}

impl PolicyStore {
    /// Creates a new `PolicyStore` with initial setup based on the given `Core`.
    ///
    /// This function initializes views and caches necessary for policy management.
    ///
    /// # Arguments
    ///
    /// * `core` - A reference to the `Core` struct used for initializing views and caches.
    ///
    /// # Returns
    ///
    /// * `Result<Arc<PolicyStore>, RvError>` - An Arc-wrapped `PolicyStore` instance or an error.
    pub async fn new(core: &Core) -> Result<Arc<PolicyStore>, RvError> {
        let Some(system_view) = core.state.load().system_view.as_ref().cloned() else {
            return Err(RvError::ErrBarrierSealed);
        };

        let acl_view = system_view.new_sub_view(POLICY_ACL_SUB_PATH);
        let rgp_view = system_view.new_sub_view(POLICY_RGP_SUB_PATH);
        let egp_view = system_view.new_sub_view(POLICY_EGP_SUB_PATH);

        let keys = acl_view.get_keys().await?;

        let mut policy_store = PolicyStore {
            router: core.router.clone(),
            acl_view: Some(Arc::new(acl_view)),
            rgp_view: Some(Arc::new(rgp_view)),
            egp_view: Some(Arc::new(egp_view)),
            self_ptr: Weak::default(),
            ..Default::default()
        };

        policy_store.token_policies_lru = Some(
            Cache::builder(POLICY_CACHE_SIZE * 10, POLICY_CACHE_SIZE as i64)
                .set_ignore_internal_cost(true)
                .finalize()
                .unwrap(),
        );
        policy_store.egp_lru = Some(
            Cache::builder(POLICY_CACHE_SIZE * 10, POLICY_CACHE_SIZE as i64)
                .set_ignore_internal_cost(true)
                .finalize()
                .unwrap(),
        );

        for key in keys.iter() {
            policy_store
                .policy_type_map
                .insert(policy_store.cache_key(key.as_str()), PolicyType::Acl);
        }

        // Special-case root; doesn't exist on disk but does need to be found
        policy_store
            .policy_type_map
            .insert(policy_store.cache_key("root"), PolicyType::Acl);

        Ok(policy_store.wrap())
    }

    pub fn wrap(self) -> Arc<Self> {
        let mut wrap_self = Arc::new(self);
        let weak_self = Arc::downgrade(&wrap_self);
        unsafe {
            let ptr_self = Arc::into_raw(wrap_self) as *mut Self;
            (*ptr_self).self_ptr = weak_self;
            wrap_self = Arc::from_raw(ptr_self);
        }

        wrap_self
    }

    /// Set a policy in the policy store.
    /// This function validates the policy name, checks for immutability, and inserts the policy into the appropriate view.
    pub async fn set_policy(&self, policy: Policy) -> Result<(), RvError> {
        if policy.name.is_empty() {
            return Err(rv_error_string!("policy name missing"));
        }

        let name = self.sanitize_name(&policy.name);
        if IMMUTABLE_POLICIES.contains(&name.as_str()) {
            return Err(rv_error_string!(format!("cannot update {} policy", name)));
        }

        if name != policy.name {
            let mut p = policy.clone();
            p.name = name;
            return self.set_policy_internal(Arc::new(p)).await;
        }

        self.set_policy_internal(Arc::new(policy)).await
    }

    // Get a policy from the policy store.
    // This function retrieves the policy from the appropriate view, checks the cache, and handles policy type mapping.
    pub async fn get_policy(
        &self,
        name: &str,
        policy_type: PolicyType,
    ) -> Result<Option<Arc<Policy>>, RvError> {
        let name = self.sanitize_name(name);
        let index = self.cache_key(&name);
        let mut policy_type = policy_type;
        let (view, cache) = match policy_type {
            PolicyType::Acl => (Some(self.get_acl_view()?), &self.token_policies_lru),
            PolicyType::Rgp => (Some(self.get_rgp_view()?), &self.token_policies_lru),
            PolicyType::Egp => (Some(self.get_egp_view()?), &self.egp_lru),
            PolicyType::Token => {
                let (v, c) = if let Some(val) = self.policy_type_map.get(&index) {
                    policy_type = *val;
                    match *val {
                        PolicyType::Acl => (Some(self.get_acl_view()?), &self.token_policies_lru),
                        PolicyType::Rgp => (Some(self.get_rgp_view()?), &self.token_policies_lru),
                        _ => {
                            return Err(rv_error_string!(format!(
                                "invalid type of policy in type map: {}",
                                policy_type
                            )));
                        }
                    }
                } else {
                    (None, &None)
                };

                (v, c)
            }
        };

        if let Some(lru) = cache
            && let Some(p) = lru.get(&index)
        {
            return Ok(Some(p.value().clone()));
        }

        if policy_type == PolicyType::Acl && name == "root" {
            let p = Arc::new(Policy {
                name: "root".into(),
                ..Default::default()
            });
            if let Some(lru) = cache {
                lru.insert(index.clone(), p.clone(), 1);
            }
            return Ok(Some(p));
        }

        if view.is_none() {
            return Err(rv_error_string!(format!(
                "unable to get the barrier subview for policy type {}",
                policy_type
            )));
        }

        let view = view.unwrap();

        let entry = view.get(&name).await?;
        if entry.is_none() {
            return Ok(None);
        }

        let entry = entry.unwrap();

        let policy_entry: PolicyEntry = serde_json::from_slice(entry.value.as_slice())?;

        let mut policy = match policy_type {
            PolicyType::Acl => {
                let p = Policy::from_str(&policy_entry.raw)?;
                self.policy_type_map.insert(index.clone(), PolicyType::Acl);
                p
            }
            PolicyType::Rgp => {
                let p = Policy::default();
                self.handle_sentinel_policy(&p, view, &entry)?;
                self.policy_type_map.insert(index.clone(), PolicyType::Rgp);
                p
            }
            PolicyType::Egp => {
                let p = Policy::default();
                self.handle_sentinel_policy(&p, view, &entry)?;
                p
            }
            _ => {
                return Err(rv_error_string!("invalid type of policy"));
            }
        };

        policy.name = name.to_string();
        policy.policy_type = policy_entry.policy_type;
        policy.templated = policy_entry.templated;

        let p = Arc::new(policy);

        if let Some(lru) = cache {
            lru.insert(index.clone(), p.clone(), 1);
        }

        Ok(Some(p))
    }

    /// List policies of a specific type in the policy store.
    /// This function retrieves the keys from the appropriate view and filters out non-assignable policies for ACLs.
    pub async fn list_policy(&self, policy_type: PolicyType) -> Result<Vec<String>, RvError> {
        let view = self.get_barrier_view(policy_type)?;
        match policy_type {
            PolicyType::Acl => {
                let mut keys = view.get_keys().await?;
                keys.retain(|s| !NON_ASSIGNABLE_POLICIES.iter().any(|&x| s == x));
                Ok(keys)
            }
            PolicyType::Rgp | PolicyType::Egp => view.get_keys().await,
            _ => Err(rv_error_string!("invalid type of policy")),
        }
    }

    /// Delete a policy from the policy store.
    /// This function removes the policy from the appropriate view, updates the cache, and handles sentinel policy invalidation.
    pub async fn delete_policy(&self, name: &str, policy_type: PolicyType) -> Result<(), RvError> {
        let name = self.sanitize_name(name);
        let view = self.get_barrier_view(policy_type)?;
        let index = self.cache_key(&name);
        match policy_type {
            PolicyType::Acl => {
                if IMMUTABLE_POLICIES.contains(&name.as_str()) {
                    return Err(rv_error_response_status!(
                        400,
                        format!("cannot delete {} policy", name)
                    ));
                }
                if name == "default" {
                    return Err(rv_error_response_status!(
                        400,
                        "cannot delete default policy"
                    ));
                }
                view.delete(&name).await?;
                self.remove_token_policy_cache(&index)?;
                self.policy_type_map.remove(&index);
            }
            PolicyType::Rgp => {
                view.delete(&name).await?;
                self.remove_token_policy_cache(&index)?;
                self.policy_type_map.remove(&index);
                self.invalidate_sentinel_policy(policy_type, "")?;
            }
            PolicyType::Egp => {
                view.delete(&name).await?;
                self.remove_egp_cache(&index)?;
                self.invalidate_egp_tree_path("")?;
                self.invalidate_sentinel_policy(policy_type, "")?;
            }
            _ => {
                return Err(rv_error_string!("unknown policy type, cannot set"));
            }
        }
        Ok(())
    }

    /// Load an ACL policy into the policy store.
    /// This function retrieves the policy if it exists, validates immutability, and sets the policy.
    pub async fn load_acl_policy(
        &self,
        policy_name: &str,
        policy_text: &str,
    ) -> Result<(), RvError> {
        let name = self.sanitize_name(policy_name);
        let policy = self.get_policy(&name, PolicyType::Acl).await?;
        if policy.is_some()
            && (!IMMUTABLE_POLICIES.contains(&name.as_str()) || policy_text == policy.unwrap().raw)
        {
            return Ok(());
        }

        let mut policy = Policy::from_str(policy_text)?;
        policy.name.clone_from(&name);
        policy.policy_type = PolicyType::Acl;

        self.set_policy_internal(Arc::new(policy)).await
    }

    /// Load default ACL policies into the policy store.
    pub async fn load_default_acl_policy(&self) -> Result<(), RvError> {
        self.load_acl_policy(DEFAULT_POLICY_NAME, DEFAULT_POLICY)
            .await?;
        self.load_acl_policy(RESPONSE_WRAPPING_POLICY_NAME, RESPONSE_WRAPPING_POLICY)
            .await?;
        self.load_acl_policy(CONTROL_GROUP_POLICY_NAME, CONTROL_GROUP_POLICY)
            .await?;
        Ok(())
    }

    /// Create a new ACL instance from a list of policy names and additional policies.
    /// This function retrieves policies by name, combines them with additional policies, and creates an ACL.
    pub async fn new_acl(
        &self,
        policy_names: &[String],
        additional_policies: Option<Vec<Arc<Policy>>>,
    ) -> Result<ACL, RvError> {
        let mut all_policies: Vec<Arc<Policy>> = vec![];
        for policy_name in policy_names.iter() {
            if let Some(policy) = self
                .get_policy(policy_name.as_str(), PolicyType::Token)
                .await?
            {
                all_policies.push(policy);
            }
        }

        if let Some(ap) = additional_policies {
            all_policies.extend(ap);
        }

        ACL::new(&all_policies)
    }

    async fn set_policy_internal(&self, policy: Arc<Policy>) -> Result<(), RvError> {
        let view = self.get_barrier_view(policy.policy_type)?;
        let pe = PolicyEntry {
            version: 2,
            templated: policy.templated,
            raw: policy.raw.clone(),
            policy_type: policy.policy_type,
            sentinel_policy: policy.sentinel_policy,
        };

        let entry = StorageEntry::new(&policy.name, &pe)?;

        let index = self.cache_key(&policy.name);

        match policy.policy_type {
            PolicyType::Acl => {
                let rgp_view = self.get_rgp_view()?;
                let rgp = rgp_view.get(&policy.name).await?;
                if rgp.is_some() {
                    return Err(rv_error_string!(
                        "cannot reuse policy names between ACLs and RGPs"
                    ));
                }

                view.put(&entry).await?;

                self.policy_type_map.insert(index.clone(), PolicyType::Acl);

                self.save_token_policy_cache(index.clone(), policy.clone())?;
            }
            PolicyType::Rgp => {
                let acl_view = self.get_acl_view()?;
                let acl = acl_view.get(&policy.name).await?;
                if acl.is_some() {
                    return Err(rv_error_string!(
                        "cannot reuse policy names between ACLs and RGPs"
                    ));
                }

                self.handle_sentinel_policy(policy.as_ref(), view, &entry)?;

                self.policy_type_map.insert(index.clone(), PolicyType::Rgp);

                self.save_token_policy_cache(index.clone(), policy.clone())?;
            }
            PolicyType::Egp => {
                self.handle_sentinel_policy(policy.as_ref(), view, &entry)?;
                self.save_egp_cache(index.clone(), policy.clone())?;
            }
            _ => {
                return Err(rv_error_string!("unknown policy type, cannot set"));
            }
        }

        Ok(())
    }

    fn get_barrier_view(&self, _policy_type: PolicyType) -> Result<Arc<BarrierView>, RvError> {
        self.get_acl_view()
    }

    fn get_acl_view(&self) -> Result<Arc<BarrierView>, RvError> {
        match &self.acl_view {
            Some(view) => Ok(view.clone()),
            None => Err(rv_error_string!(
                "unable to get the barrier subview for policy type acl"
            )),
        }
    }

    fn get_rgp_view(&self) -> Result<Arc<BarrierView>, RvError> {
        match &self.rgp_view {
            Some(view) => Ok(view.clone()),
            None => Err(rv_error_string!(
                "unable to get the barrier subview for policy type rgp"
            )),
        }
    }

    fn get_egp_view(&self) -> Result<Arc<BarrierView>, RvError> {
        match &self.egp_view {
            Some(view) => Ok(view.clone()),
            None => Err(rv_error_string!(
                "unable to get the barrier subview for policy type egp"
            )),
        }
    }

    fn save_token_policy_cache(&self, index: String, policy: Arc<Policy>) -> Result<(), RvError> {
        if let Some(lru) = &self.token_policies_lru
            && !lru.insert(index, policy, 1)
        {
            return Err(rv_error_string!("save token policy cache failed!"));
        }

        Ok(())
    }

    fn remove_token_policy_cache(&self, index: &String) -> Result<(), RvError> {
        if let Some(lru) = &self.token_policies_lru {
            lru.remove(index);
        }

        Ok(())
    }

    fn save_egp_cache(&self, index: String, policy: Arc<Policy>) -> Result<(), RvError> {
        if let Some(lru) = &self.egp_lru
            && !lru.insert(index, policy, 1)
        {
            return Err(rv_error_string!("save token policy cache failed!"));
        }

        Ok(())
    }

    fn remove_egp_cache(&self, index: &String) -> Result<(), RvError> {
        if let Some(lru) = &self.egp_lru {
            lru.remove(index);
        }

        Ok(())
    }

    fn handle_sentinel_policy(
        &self,
        _policy: &Policy,
        _view: Arc<BarrierView>,
        _entry: &StorageEntry,
    ) -> Result<(), RvError> {
        Ok(())
    }

    fn invalidate_sentinel_policy(
        &self,
        _policy_type: PolicyType,
        _index: &str,
    ) -> Result<(), RvError> {
        Ok(())
    }

    fn invalidate_egp_tree_path(&self, _index: &str) -> Result<(), RvError> {
        Ok(())
    }

    /// Sanitize a policy name by converting it to lowercase.
    fn sanitize_name(&self, name: &str) -> String {
        name.to_lowercase().to_string()
    }

    /// Generate a cache key for a given policy name.
    fn cache_key(&self, name: &str) -> String {
        name.to_string()
    }
}

#[async_trait::async_trait]
impl AuthHandler for PolicyStore {
    fn name(&self) -> String {
        "policy_store".to_string()
    }

    /// Handle authentication for a given request.
    /// This function checks the request path, performs capability checks, and updates authentication results.
    async fn post_auth(&self, req: &mut Request) -> Result<(), RvError> {
        let is_root_path = self.router.is_root_path(&req.path)?;

        if req.auth.is_none() && is_root_path {
            return Err(rv_error_string!(
                "cannot access root path in unauthenticated request"
            ));
        }

        let mut acl_result = ACLResults::default();

        if let Some(auth) = &req.auth {
            if auth.policies.is_empty() {
                return Ok(());
            }

            let acl = self.new_acl(&auth.policies, None).await?;
            acl_result = acl.allow_operation(req, false)?;
        }

        if let Some(auth) = &mut req.auth {
            if is_root_path && !acl_result.root_privs && req.operation != Operation::Help {
                return Err(rv_error_string!(
                    "cannot access root path in unauthenticated request"
                ));
            }

            let allowed = acl_result.allowed;

            auth.policy_results = Some(PolicyResults {
                allowed,
                granting_policies: acl_result.granting_policies,
            });

            if !allowed {
                log::warn!(
                    "preflight capability check returned 403, please ensure client's policies grant access to path \
                     \"{}\"",
                    req.path
                );
                return Err(RvError::ErrPermissionDenied);
            }
        }

        Ok(())
    }
}
