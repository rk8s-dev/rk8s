//! This module implements a token storage system for managing token creation, lookup,
//! revocation, and renewal. The system supports various operations and provides the
//! logical backend interface through the `TokenStore` struct. Additionally, it implements
//! the `Handler` trait to provide authentication and authorization functionality at
//! different stages of request handling, such as pre-routing and post-routing.
#[cfg(not(feature = "sync_handler"))]
use std::future::Future;
#[cfg(not(feature = "sync_handler"))]
use std::pin::Pin;

use std::{
    collections::HashMap,
    sync::{Arc, Weak},
    time::{Duration, SystemTime, UNIX_EPOCH},
};

use arc_swap::ArcSwap;
use better_default::Default;
use humantime::parse_duration;
use lazy_static::lazy_static;
use regex::Regex;
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};

use super::{
    expiration::{ExpirationManager, DEFAULT_LEASE_DURATION_SECS, MAX_LEASE_DURATION_SECS},
    AUTH_ROUTER_PREFIX,
};
use crate::{
    context::Context,
    core::Core,
    errors::RvError,
    handler::{AuthHandler, HandlePhase, Handler},
    logical::{
        lease::calculate_ttl, Auth, Backend, Field, FieldType, Lease, LogicalBackend, Operation, Path, PathOperation,
        Request, Response,
    },
    modules::policy::policy_store::NON_ASSIGNABLE_POLICIES,
    new_fields, new_fields_internal, new_logical_backend, new_logical_backend_internal, new_path, new_path_internal,
    router::Router,
    rv_error_response, rv_error_string,
    storage::{Storage, StorageEntry},
    utils::{
        default_system_time, deserialize_duration, deserialize_system_time, generate_uuid, is_str_subset,
        policy::sanitize_policies,
        serialize_duration, serialize_system_time, sha1,
        token_util::{DEFAULT_LEASE_TTL, MAX_LEASE_TTL},
    },
};

const TOKEN_LOOKUP_PREFIX: &str = "id/";
const TOKEN_PARENT_PREFIX: &str = "parent/";
const TOKEN_SALT_LOCATION: &str = "salt";
const TOKEN_SUB_PATH: &str = "token/";

static AUTH_TOKEN_HELP: &str = r#"
TODO
"#;

lazy_static! {
    static ref DISPLAY_NAME_SANITIZE: Regex = Regex::new(r"[^a-zA-Z0-9-]").unwrap();
}

#[derive(Serialize, Deserialize)]
struct TokenReqData {
    #[serde(default)]
    id: String,
    #[serde(default)]
    policies: Vec<String>,
    #[serde(default)]
    meta: HashMap<String, String>,
    #[serde(default)]
    no_parent: bool,
    #[serde(default)]
    lease: String,
    #[serde(default)]
    ttl: String,
    #[serde(default)]
    display_name: String,
    #[serde(default)]
    num_uses: u32,
    #[serde(default)]
    renewable: bool,
    #[serde(default, deserialize_with = "deserialize_duration")]
    period: Duration,
    #[serde(default, deserialize_with = "deserialize_duration")]
    explicit_max_ttl: Duration,
}

/// Data structure representing a stored token entry.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct TokenEntry {
    #[default(generate_uuid())]
    pub id: String,
    pub parent: String,
    pub policies: Vec<String>,
    pub path: String,
    pub meta: HashMap<String, String>,
    pub display_name: String,
    pub num_uses: u32,
    pub ttl: u64,
    #[default(SystemTime::now())]
    #[serde(
        default = "default_system_time",
        serialize_with = "serialize_system_time",
        deserialize_with = "deserialize_system_time"
    )]
    pub creation_time: SystemTime,
    #[serde(default, serialize_with = "serialize_duration", deserialize_with = "deserialize_duration")]
    pub period: Duration,
    #[serde(default, serialize_with = "serialize_duration", deserialize_with = "deserialize_duration")]
    pub explicit_max_ttl: Duration,
}

/// Manages the storage and handling of tokens.
pub struct TokenStore {
    pub self_ptr: Weak<Self>,
    pub router: Arc<Router>,
    pub view: Option<Arc<dyn Storage + Send + Sync>>,
    pub salt: String,
    pub expiration: Arc<ExpirationManager>,
    pub auth_handlers: ArcSwap<Vec<Arc<dyn AuthHandler>>>,
}

#[maybe_async::maybe_async]
impl TokenStore {
    /// Wraps the `TokenStore` instance in an `Arc` and sets its weak pointer reference.
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

    /// Creates a new `TokenStore` and initializes it with the necessary components.
    pub async fn new(core: &Core, expiration: Arc<ExpirationManager>) -> Result<TokenStore, RvError> {
        let Some(system_view) = core.state.load().system_view.as_ref().cloned() else {
            return Err(RvError::ErrBarrierSealed);
        };

        let view = system_view.new_sub_view(TOKEN_SUB_PATH);
        let salt = view.get(TOKEN_SALT_LOCATION).await?;

        let mut token_store = TokenStore {
            self_ptr: Weak::new(),
            router: core.router.clone(),
            view: None,
            salt: String::new(),
            auth_handlers: ArcSwap::new(core.auth_handlers.load().clone()),
            expiration,
        };

        if salt.is_some() {
            token_store.salt = String::from_utf8_lossy(&salt.unwrap().value).to_string();
        }

        if token_store.salt.is_empty() {
            token_store.salt = generate_uuid();
            let raw =
                StorageEntry { key: TOKEN_SALT_LOCATION.to_string(), value: token_store.salt.as_bytes().to_vec() };
            view.put(&raw).await?;
        }

        token_store.view = Some(Arc::new(view));

        Ok(token_store)
    }

    /// Creates a new logical backend for token operations.
    pub fn new_backend(&self) -> LogicalBackend {
        let ts_inner_arc1 = self.self_ptr.upgrade().unwrap().clone();
        let ts_inner_arc2 = self.self_ptr.upgrade().unwrap().clone();
        let ts_inner_arc3 = self.self_ptr.upgrade().unwrap().clone();
        let ts_inner_arc4 = self.self_ptr.upgrade().unwrap().clone();
        let ts_inner_arc5 = self.self_ptr.upgrade().unwrap().clone();
        let ts_inner_arc6 = self.self_ptr.upgrade().unwrap().clone();
        let ts_inner_arc7 = self.self_ptr.upgrade().unwrap().clone();

        let backend = new_logical_backend!({
            paths: [
                {
                    pattern: "create*",
                    fields: {
                        "num_uses": {
                            field_type: FieldType::Int,
                            description: "Max number of uses for this token"
                        },
                        "period": {
                            field_type: FieldType::Str,
                            description: "Renew period"
                        },
                        "ttl": {
                            field_type: FieldType::DurationSecond,
                            description: "Time to live for this token"
                        },
                        "renewable": {
                            field_type: FieldType::Bool,
                            default: true,
                            description: "Allow token to be renewed past its initial TTL up to system/mount maximum TTL"
                        },
                        "policies": {
                            field_type: FieldType::Array,
                            description: "List of policies for the token"
                        }
                    },
                    operations: [
                        {op: Operation::Write, handler: ts_inner_arc1.handle_create}
                    ],
                    help: "The token create path is used to create new tokens."
                },
                {
                    pattern: "lookup/(?P<token>.+)",
                    fields: {
                        "token": {
                            field_type: FieldType::Str,
                            description: "Token to lookup"
                        }
                    },
                    operations: [
                        {op: Operation::Read, handler: ts_inner_arc2.handle_lookup}
                    ],
                    help: "This endpoint will lookup a token and its properties."
                },
                {
                    pattern: "lookup-self$",
                    fields: {
                        "token": {
                            field_type: FieldType::Str,
                            description: "Token to lookup"
                        }
                    },
                    operations: [
                        {op: Operation::Read, handler: ts_inner_arc3.handle_lookup_self}
                    ],
                    help: "This endpoint will lookup a token and its properties."
                },
                {
                    pattern: "revoke/(?P<token>.+)",
                    fields: {
                        "token": {
                            field_type: FieldType::Str,
                            description: "Token to revoke"
                        }
                    },
                    operations: [
                        {op: Operation::Write, handler: ts_inner_arc4.handle_revoke_tree}
                    ],
                    help: "This endpoint will delete the token and all of its child tokens."
                },
                {
                    pattern: "revoke-orphan/(?P<token>.+)",
                    fields: {
                        "token": {
                            field_type: FieldType::Str,
                            description: "Token to revoke (request body)"
                        }
                    },
                    operations: [
                        {op: Operation::Write, handler: ts_inner_arc5.handle_revoke_orphan}
                    ],
                    help: "This endpoint will delete the token and orphan its child tokens."
                },
                {
                    pattern: "renew/(?P<token>.+)",
                    fields: {
                        "token": {
                            field_type: FieldType::Str,
                            description: "Token to renew (request body)"
                        },
                        "increment": {
                            field_type: FieldType::Int,
                            description: "The desired increment in seconds to the token expiration"
                        }
                    },
                    operations: [
                        {op: Operation::Write, handler: ts_inner_arc6.handle_renew}
                    ],
                    help: "This endpoint will renew the token and prevent expiration."
                }
            ],
            auth_renew_handler: ts_inner_arc7.auth_renew,
            root_paths: ["revoke-orphan/*"],
            help: AUTH_TOKEN_HELP,
        });

        backend
    }

    /// Returns a salted hash of a token ID.
    pub fn salt_id(&self, id: &str) -> String {
        let salted_id = format!("{}{}", self.salt, id);
        sha1(salted_id.as_bytes())
    }

    /// Generates a root token with 'root' policy.
    pub async fn root_token(&self) -> Result<TokenEntry, RvError> {
        let mut te = TokenEntry {
            policies: vec!["root".to_string()],
            path: "auth/token/root".to_string(),
            display_name: "root".to_string(),
            ..TokenEntry::default()
        };

        self.create(&mut te).await?;

        Ok(te)
    }

    /// Creates a token entry in the storage.
    pub async fn create(&self, entry: &mut TokenEntry) -> Result<(), RvError> {
        let Some(view) = self.view.as_ref() else {
            return Err(RvError::ErrModuleNotInit);
        };

        if entry.id.is_empty() {
            entry.id = generate_uuid();
        }

        let salted_id = self.salt_id(&entry.id);

        let value = serde_json::to_string(&entry)?;

        if !entry.parent.is_empty() {
            let parent = self.lookup(&entry.parent).await?;
            if parent.is_none() {
                return Err(RvError::ErrAuthTokenNotFound);
            }

            let path = format!("{}{}/{}", TOKEN_PARENT_PREFIX, self.salt_id(&entry.parent), salted_id);
            let entry = StorageEntry { key: path, ..StorageEntry::default() };

            view.put(&entry).await?;
        }

        view.put(&StorageEntry { key: format!("{TOKEN_LOOKUP_PREFIX}{salted_id}"), value: value.as_bytes().to_vec() })
            .await
    }

    /// Uses the token and decrements its use count.
    pub async fn use_token(&self, entry: &mut TokenEntry) -> Result<(), RvError> {
        let Some(view) = self.view.as_ref() else {
            return Err(RvError::ErrModuleNotInit);
        };

        if entry.num_uses == 0 {
            return Ok(());
        }

        entry.num_uses -= 1;

        if entry.num_uses == 0 {
            return self.revoke(&entry.id).await;
        }

        let salted_id = self.salt_id(&entry.id);
        let value = serde_json::to_string(&entry)?;

        let path = format!("{TOKEN_LOOKUP_PREFIX}{salted_id}");
        let entry = StorageEntry { key: path, value: value.as_bytes().to_vec() };

        view.put(&entry).await
    }

    /// Checks the validity of a token and returns the associated authentication data.
    pub async fn check_token(&self, _path: &str, token: &str) -> Result<Option<Auth>, RvError> {
        if token.is_empty() {
            return Err(RvError::ErrRequestClientTokenMissing);
        }

        log::debug!("check token: {token}");
        let te = self.lookup(token).await?;
        if te.is_none() {
            return Err(RvError::ErrPermissionDenied);
        }

        let mut entry = te.unwrap();

        self.use_token(&mut entry).await?;

        let mut auth = Auth {
            client_token: token.to_string(),
            display_name: entry.display_name,
            token_policies: entry.policies.clone(),
            policies: entry.policies.clone(),
            metadata: entry.meta,
            ..Auth::default()
        };

        sanitize_policies(&mut auth.policies, false);

        Ok(Some(auth))
    }

    /// Looks up the token entry with the given ID.
    pub async fn lookup(&self, id: &str) -> Result<Option<TokenEntry>, RvError> {
        if id.is_empty() {
            return Err(RvError::ErrAuthTokenIdInvalid);
        }

        self.lookup_salted(self.salt_id(id).as_str()).await
    }

    pub async fn lookup_salted(&self, salted_id: &str) -> Result<Option<TokenEntry>, RvError> {
        let Some(view) = self.view.as_ref() else {
            return Err(RvError::ErrModuleNotInit);
        };

        let path = format!("{TOKEN_LOOKUP_PREFIX}{salted_id}");
        let raw = view.get(&path).await?;
        if raw.is_none() {
            return Ok(None);
        }

        let entry: TokenEntry = serde_json::from_slice(raw.unwrap().value.as_slice())?;

        Ok(Some(entry))
    }

    pub async fn revoke(&self, id: &str) -> Result<(), RvError> {
        if id.is_empty() {
            return Err(RvError::ErrAuthTokenIdInvalid);
        }

        self.revoke_salted(self.salt_id(id).as_str()).await
    }

    pub async fn revoke_salted(&self, salted_id: &str) -> Result<(), RvError> {
        let Some(view) = self.view.as_ref() else {
            return Err(RvError::ErrModuleNotInit);
        };

        let entry = self.lookup_salted(salted_id).await?;

        let path = format!("{TOKEN_LOOKUP_PREFIX}{salted_id}");

        view.delete(&path).await?;

        if entry.is_some() {
            let entry = entry.unwrap();
            if entry.parent.as_str() != "" {
                let path = format!("{}{}/{}", TOKEN_PARENT_PREFIX, self.salt_id(&entry.parent), salted_id);
                view.delete(&path).await?;
            }
            //Revoke all secrets under this token
            self.expiration.revoke_by_token(&entry).await?;
        }

        Ok(())
    }

    /// Revokes the token with the given ID and all its child tokens.
    ///
    /// # Arguments
    /// - id: The ID of the token to revoke.
    ///
    /// # Returns
    /// - Result<(), RvError>: Ok(()) if successful, or an error if not.
    pub async fn revoke_tree(&self, id: &str) -> Result<(), RvError> {
        if id.is_empty() {
            return Err(RvError::ErrAuthTokenIdInvalid);
        }

        self.revoke_tree_salted(self.salt_id(id).as_str()).await
    }

    #[cfg(not(feature = "sync_handler"))]
    pub fn revoke_tree_salted(
        &self,
        salted_id: &str,
    ) -> Pin<Box<dyn Future<Output = Result<(), RvError>> + Send + '_>> {
        let self_ref = self;
        let salted_id = salted_id.to_owned();

        Box::pin(async move {
            let view = self_ref.view.as_ref().ok_or(RvError::ErrModuleNotInit)?;
            let path = format!("{TOKEN_PARENT_PREFIX}{}", &salted_id);
            let children = view.list(&path).await?;

            for child in children {
                self_ref.revoke_tree_salted(&child).await?;
            }

            self_ref.revoke_salted(&salted_id).await?;

            Ok(())
        })
    }

    #[cfg(feature = "sync_handler")]
    pub fn revoke_tree_salted(&self, salted_id: &str) -> Result<(), RvError> {
        let Some(view) = self.view.as_ref() else {
            return Err(RvError::ErrModuleNotInit);
        };

        let path = format!("{TOKEN_PARENT_PREFIX}{salted_id}/");

        let children = view.list(&path)?;
        for child in children.iter() {
            self.revoke_tree_salted(child)?;
        }

        self.revoke_salted(salted_id)
    }

    pub async fn handle_create(&self, _backend: &dyn Backend, req: &mut Request) -> Result<Option<Response>, RvError> {
        if req.body.is_none() {
            return Err(RvError::ErrRequestInvalid);
        }

        let parent = self.lookup(&req.client_token).await?;
        if parent.is_none() {
            return Err(RvError::ErrRequestInvalid);
        }

        let parent = parent.unwrap();
        if parent.num_uses > 0 {
            return Err(RvError::ErrRequestInvalid);
        }

        let is_root = parent.policies.iter().any(|s| s.as_str() == "root");

        let mut data: TokenReqData = serde_json::from_value(Value::Object(req.body.as_ref().unwrap().clone()))?;

        let mut te = TokenEntry {
            parent: req.client_token.clone(),
            path: "auth/token/create".into(),
            meta: data.meta.clone(),
            display_name: "token".into(),
            num_uses: data.num_uses,
            ..TokenEntry::default()
        };

        let mut renewable = data.renewable;

        if !data.display_name.is_empty() {
            let mut full = format!("token-{}", data.display_name);
            full = DISPLAY_NAME_SANITIZE.replace_all(&full, "-").to_string();
            full = full.trim_end_matches('-').to_string();
            te.display_name = full;
        }

        if !data.id.is_empty() {
            if !is_root {
                return Err(RvError::ErrRequestInvalid);
            }
            te.id.clone_from(&data.id);
        }

        if data.policies.is_empty() {
            data.policies.clone_from(&parent.policies);
            sanitize_policies(&mut data.policies, false);
        }

        if !is_root && !is_str_subset(&data.policies, &parent.policies) {
            return Err(RvError::ErrRequestInvalid);
        }

        te.policies.clone_from(&data.policies);

        for policy in te.policies.iter() {
            if NON_ASSIGNABLE_POLICIES.contains(&policy.as_str()) {
                return Err(rv_error_response!(&format!("cannot assign policy {policy}")));
            }
        }

        if te.policies.contains(&"root".into()) && !parent.policies.contains(&"root".into()) {
            return Err(rv_error_response!("root tokens may not be created without parent token being root"));
        }

        if data.no_parent {
            if !is_root {
                return Err(RvError::ErrRequestInvalid);
            }
            te.parent = "".into();
        }

        if !data.ttl.is_empty() {
            let dur = parse_duration(&data.ttl)?;
            te.ttl = dur.as_secs();
        } else if !data.lease.is_empty() {
            let dur = parse_duration(&data.lease)?;
            te.ttl = dur.as_secs();
        }

        te.period = data.period;
        te.explicit_max_ttl = data.explicit_max_ttl;

        if te.period.as_secs() > 0 || te.ttl > 0 || (te.ttl == 0 && !te.policies.contains(&"root".to_string())) {
            te.ttl = calculate_ttl(
                MAX_LEASE_TTL,
                DEFAULT_LEASE_TTL,
                Duration::ZERO,
                te.period,
                Duration::from_secs(te.ttl),
                Duration::ZERO,
                te.explicit_max_ttl,
                te.creation_time,
            )?
            .as_secs();
        }

        if te.ttl == 0 && te.explicit_max_ttl.as_secs() > 0 {
            te.ttl = te.explicit_max_ttl.as_secs();
        }

        if data.no_parent {
            // TODO: Only allow an orphan token if the client has sudo policy
            te.parent.clear();
        }

        if te.ttl == 0 {
            if parent.ttl != 0 {
                return Err(rv_error_response!("expiring root tokens cannot create non-expiring root tokens"));
            }
            renewable = false;
        }

        self.create(&mut te).await?;

        let auth = Auth {
            lease: Lease { ttl: Duration::from_secs(te.ttl), renewable, ..Lease::default() },
            client_token: te.id.clone(),
            display_name: te.display_name.clone(),
            policies: te.policies.clone(),
            period: te.period,
            explicit_max_ttl: te.explicit_max_ttl,
            metadata: te.meta.clone(),
            ..Default::default()
        };
        let resp = Response { auth: Some(auth), ..Response::default() };

        Ok(Some(resp))
    }

    pub async fn handle_revoke_tree(
        &self,
        _backend: &dyn Backend,
        req: &mut Request,
    ) -> Result<Option<Response>, RvError> {
        let id = req.get_data_as_str("token")?;
        if id.is_empty() {
            return Err(RvError::ErrRequestInvalid);
        }

        self.revoke_tree(&id).await?;

        Ok(None)
    }

    pub async fn handle_revoke_orphan(
        &self,
        _backend: &dyn Backend,
        req: &mut Request,
    ) -> Result<Option<Response>, RvError> {
        let id = req.get_data_as_str("token")?;
        if id.is_empty() {
            return Err(RvError::ErrRequestInvalid);
        }

        self.revoke(&id).await?;

        Ok(None)
    }

    pub async fn handle_lookup_self(
        &self,
        backend: &dyn Backend,
        req: &mut Request,
    ) -> Result<Option<Response>, RvError> {
        if let Some(data) = req.data.as_mut() {
            data.insert("token".to_string(), Value::String(req.client_token.clone()));
        } else {
            req.data = json!({
                "token": req.client_token.clone(),
            })
            .as_object()
            .cloned();
        }

        self.handle_lookup(backend, req).await
    }

    pub async fn handle_lookup(&self, _backend: &dyn Backend, req: &mut Request) -> Result<Option<Response>, RvError> {
        log::debug!("lookup token");
        let mut id = req.get_data_as_str("token")?;
        if id.is_empty() {
            id.clone_from(&req.client_token);
        }

        if id.is_empty() {
            return Err(RvError::ErrRequestInvalid);
        }

        let te = self.lookup(&id).await?;
        if te.is_none() {
            return Ok(None);
        }

        let te = te.unwrap();

        let meta = serde_json::to_value(&te.meta)?;

        let mut data = serde_json::json!({
            "id": te.id.clone(),
            "policies": te.policies.clone(),
            "path": te.path.clone(),
            "meta": meta,
            "display_name": te.display_name.clone(),
            "num_uses": te.num_uses,
            "ttl": 0,
            "creation_time": te.creation_time.duration_since(UNIX_EPOCH).map(|d| d.as_secs()).unwrap_or(0),
            "creation_ttl": te.ttl,
            "explicit_max_ttl": te.explicit_max_ttl.as_secs(),
        })
        .as_object()
        .unwrap()
        .clone();

        if te.period.as_secs() > 0 {
            data.insert("period".to_string(), json!(te.period.as_secs()));
        }

        Ok(Some(Response::data_response(Some(data))))
    }

    pub async fn handle_renew(&self, _backend: &dyn Backend, req: &mut Request) -> Result<Option<Response>, RvError> {
        let id = req.get_data_as_str("token")?;
        if id.is_empty() {
            return Err(RvError::ErrRequestInvalid);
        }

        let te = self.lookup(&id).await?.ok_or(RvError::ErrRequestInvalid)?;

        let increment_raw: i32 = serde_json::from_value(req.get_data("increment")?)?;
        let increment = Duration::from_secs(increment_raw as u64);

        self.expiration.renew_token(req, &te, increment).await
    }

    pub async fn auth_renew(&self, _backend: &dyn Backend, req: &mut Request) -> Result<Option<Response>, RvError> {
        if req.auth.is_none() {
            return Err(rv_error_string!("request auth is nil"));
        }

        let id = &req.auth.as_ref().unwrap().client_token;
        let te = self.lookup(id).await?.ok_or(rv_error_string!("no token entry found during lookup"))?;

        let auth = req.auth.as_mut().unwrap();
        auth.period = te.period;
        auth.explicit_max_ttl = te.explicit_max_ttl;

        Ok(Some(Response { auth: Some(auth.clone()), ..Default::default() }))
    }
}

#[maybe_async::maybe_async]
impl Handler for TokenStore {
    fn name(&self) -> String {
        "auth_token".to_string()
    }

    /// Process the request before routing. If the module has registered the pre_auth phase, execute it.
    /// It can handle custom tokens. If pre_auth returns Auth, skip the default check_token operation.
    /// If pre_auth returns None or is not registered, perform the default check_token operation.
    /// After check_token, there's the post_auth phase where the registered post_auth function of the
    /// module runs. For example, the policy module does ACL checks in the post_auth phase.
    async fn pre_route(&self, req: &mut Request) -> Result<Option<Response>, RvError> {
        let is_unauth_path = self.router.is_unauth_path(&req.path)?;
        if is_unauth_path {
            return Ok(None);
        }

        let mut auth: Option<Auth> = None;

        req.handle_phase = HandlePhase::PreAuth;

        let auth_handlers = self.auth_handlers.load();

        for auth_handler in auth_handlers.iter() {
            match auth_handler.pre_auth(req).await {
                Ok(Some(ret)) => {
                    auth = Some(ret);
                    break;
                }
                Ok(None) | Err(RvError::ErrHandlerDefault) => continue,
                Err(e) => return Err(e),
            }
        }

        if auth.is_none() {
            auth = self.check_token(&req.path, &req.client_token).await?;
        }

        if auth.is_none() {
            return Err(RvError::ErrPermissionDenied);
        }

        req.name.clone_from(&auth.as_ref().unwrap().display_name);
        req.auth = auth;

        req.handle_phase = HandlePhase::PostAuth;

        for auth_handler in auth_handlers.iter() {
            match auth_handler.post_auth(req).await {
                Ok(()) | Err(RvError::ErrHandlerDefault) => continue,
                Err(e) => return Err(e),
            }
        }

        Ok(None)
    }

    /// Handles post-routing logic after routing a request. The main operation here is the expiration
    /// time management of secrets and tokens.
    async fn post_route(&self, req: &mut Request, resp: &mut Option<Response>) -> Result<(), RvError> {
        if resp.is_none() {
            return Ok(());
        }

        let is_unauth_path = self.router.is_unauth_path(&req.path)?;

        let resp = resp.as_mut().unwrap();

        if !is_unauth_path && resp.secret.is_some() && !req.path.starts_with("/sys/renew") {
            let mut register_lease = true;
            let me = self.router.matching_mount_entry(&req.path)?;
            if me.is_none() {
                register_lease = false;
            }

            {
                let mount_entry = me.as_ref().unwrap().read()?;

                if let Some(ref options) = mount_entry.options {
                    if let Some(leased_passthrough) = options.get("leased_passthrough") {
                        if leased_passthrough != "true" {
                            register_lease = false;
                        }
                    } else {
                        register_lease = false;
                    }
                } else {
                    register_lease = false;
                }
            }

            if register_lease {
                self.expiration.register_secret(req, resp).await?;
            }
        }

        if let Some(auth) = resp.auth.as_mut() {
            if is_unauth_path {
                let source = self.router.matching_mount(&req.path)?;
                let source = source.as_str().trim_start_matches(AUTH_ROUTER_PREFIX).replace('/', "-");
                auth.display_name = (source + &auth.display_name).trim_end_matches('-').to_string();
                req.name.clone_from(&auth.display_name);
            } else if !req.path.starts_with("auth/token/") {
                return Err(RvError::ErrPermissionDenied);
            }

            if auth.ttl.as_secs() == 0 {
                auth.ttl = DEFAULT_LEASE_DURATION_SECS;
            }

            if auth.ttl > MAX_LEASE_DURATION_SECS {
                auth.ttl = MAX_LEASE_DURATION_SECS;
            }

            let token_ttl = calculate_ttl(
                MAX_LEASE_TTL,
                DEFAULT_LEASE_TTL,
                Duration::ZERO,
                auth.period,
                auth.ttl,
                auth.max_ttl,
                auth.explicit_max_ttl,
                SystemTime::now(),
            )?;

            auth.token_policies.clone_from(&auth.policies);
            sanitize_policies(&mut auth.token_policies, !auth.no_default_policy);

            let all_policies = auth.token_policies.clone();

            // TODO: add identity_policies to all_policies

            if all_policies.contains(&"root".to_string()) {
                return Err(rv_error_response!("auth methods cannot create root tokens"));
            }

            let mut te = TokenEntry {
                path: req.path.clone(),
                meta: auth.metadata.clone(),
                display_name: auth.display_name.clone(),
                ttl: token_ttl.as_secs(),
                policies: auth.token_policies.clone(),
                explicit_max_ttl: auth.explicit_max_ttl,
                period: auth.period,
                ..Default::default()
            };

            self.create(&mut te).await?;

            auth.client_token.clone_from(&te.id);
            auth.ttl = Duration::from_secs(te.ttl);

            self.expiration.register_auth(&te, auth).await?;

            auth.policies = all_policies;
        }

        Ok(())
    }
}

#[cfg(test)]
mod mod_token_store_tests {
    use super::*;
    use crate::{
        context::Context,
        logical::{Backend, Request, Response, Secret},
        test_utils::new_unseal_test_rusty_vault,
    };

    macro_rules! mock_token_store {
        () => {{
            let name = format!("{}_{}", file!(), line!()).replace("/", "_").replace("\\", "_").replace(".", "_");
            println!("init_test_rusty_vault, name: {}", name);
            #[cfg(not(feature = "sync_handler"))]
            let (_, core, _) = new_unseal_test_rusty_vault(&name).await;
            #[cfg(feature = "sync_handler")]
            let (_, core, _) = new_unseal_test_rusty_vault(&name);

            let expiration = ExpirationManager::new(&core).unwrap().wrap();
            #[cfg(not(feature = "sync_handler"))]
            let token_store = TokenStore::new(&core, expiration.clone()).await.unwrap().wrap();
            #[cfg(feature = "sync_handler")]
            let token_store = TokenStore::new(&core, expiration.clone()).unwrap().wrap();

            expiration.set_token_store(&token_store).unwrap();

            token_store
        }};
    }

    pub struct MockBackend(());

    #[maybe_async::maybe_async]
    impl Backend for MockBackend {
        fn init(&mut self) -> Result<(), RvError> {
            Ok(())
        }
        fn setup(&self, _key: &str) -> Result<(), RvError> {
            Ok(())
        }
        fn cleanup(&self) -> Result<(), RvError> {
            Ok(())
        }
        fn get_unauth_paths(&self) -> Option<Arc<Vec<String>>> {
            None
        }
        fn get_root_paths(&self) -> Option<Arc<Vec<String>>> {
            None
        }
        fn get_ctx(&self) -> Option<Arc<Context>> {
            None
        }
        async fn handle_request(&self, _req: &mut Request) -> Result<Option<Response>, RvError> {
            Ok(None)
        }
        fn secret(&self, _key: &str) -> Option<&Arc<Secret>> {
            None
        }
    }

    #[maybe_async::test(feature = "sync_handler", async(all(not(feature = "sync_handler")), tokio::test))]
    async fn test_token_create_and_lookup() {
        let token_store = mock_token_store!();

        let mut entry = TokenEntry {
            policies: vec!["default".to_string()],
            path: "auth/token/create".to_string(),
            display_name: "test-token".to_string(),
            ..TokenEntry::default()
        };

        token_store.create(&mut entry).await.unwrap();

        let result = token_store.lookup(&entry.id).await.unwrap();
        assert!(result.is_some());
        let looked_up_entry = result.unwrap();
        assert_eq!(looked_up_entry.id, entry.id);
        assert_eq!(looked_up_entry.policies, entry.policies);
    }

    #[maybe_async::test(feature = "sync_handler", async(all(not(feature = "sync_handler")), tokio::test))]
    async fn test_token_revoke() {
        let token_store = mock_token_store!();

        let mut entry = TokenEntry {
            policies: vec!["default".to_string()],
            path: "auth/token/create".to_string(),
            display_name: "test-token".to_string(),
            ..TokenEntry::default()
        };

        token_store.create(&mut entry).await.unwrap();

        token_store.revoke(&entry.id).await.unwrap();

        let result = token_store.lookup(&entry.id).await.unwrap();
        assert!(result.is_none());
    }

    #[maybe_async::test(feature = "sync_handler", async(all(not(feature = "sync_handler")), tokio::test))]
    async fn test_token_revoke_tree() {
        let token_store = mock_token_store!();

        let mut parent_entry = TokenEntry {
            policies: vec!["default".to_string()],
            path: "auth/token/create".to_string(),
            display_name: "parent-token".to_string(),
            ..TokenEntry::default()
        };
        token_store.create(&mut parent_entry).await.unwrap();

        let mut child_entry = TokenEntry {
            parent: parent_entry.id.clone(),
            policies: vec!["default".to_string()],
            path: "auth/token/create".to_string(),
            display_name: "child-token".to_string(),
            ..TokenEntry::default()
        };
        token_store.create(&mut child_entry).await.unwrap();

        let result = token_store.revoke_tree(&parent_entry.id).await;
        assert!(result.is_ok());

        let parent_result = token_store.lookup(&parent_entry.id).await.unwrap();
        let child_result = token_store.lookup(&child_entry.id).await.unwrap();
        assert!(parent_result.is_none());
        assert!(child_result.is_none());
    }

    #[maybe_async::test(feature = "sync_handler", async(all(not(feature = "sync_handler")), tokio::test))]
    async fn test_token_handle_create_request() {
        let token_store = mock_token_store!();
        let mock_backend = MockBackend(());

        let mut entry = TokenEntry {
            policies: vec!["default".to_string()],
            path: "auth/token/create".to_string(),
            display_name: "test-token".to_string(),
            ..TokenEntry::default()
        };

        token_store.create(&mut entry).await.unwrap();

        let result = token_store.lookup(&entry.id).await.unwrap();
        assert!(result.is_some());

        let mut req = Request {
            client_token: entry.id.clone(),
            body: json!({
                "policies": ["default"],
                "display_name": "test-token",
            })
            .as_object()
            .cloned(),
            ..Request::default()
        };

        let response = token_store.handle_create(&mock_backend, &mut req).await.unwrap();
        assert!(response.is_some());
        let resp = response.unwrap();
        assert!(resp.auth.is_some());
        let auth = resp.auth.unwrap();
        assert_eq!(auth.display_name, "token-test-token");
        assert_eq!(auth.policies, vec!["default".to_owned()]);
    }
}
