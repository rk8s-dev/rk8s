//! This file defines structures and implementations for handling security policies.
//!
//! The core components include:
//! - `PolicyType`: An enum representing different types of policies such as ACL, RGP, EGP, and Token.
//! - `Policy`: A struct that represents a security policy, encompassing a name, type, and rules associated with specific paths.
//! - `PolicyPathRules`: A struct that defines rules and permissions for individual paths within a policy.
//! - `Permissions`: A struct managing capabilities and parameter rules for policies, including allowed, denied, and required parameters.
//! - `Capability`: An enum representing various capabilities (e.g., Read, Write, Delete) that can be associated with a policy path.
//!
//! Key functionality:
//! - Parsing policies from strings using the `FromStr` trait, supporting HCL formats.
//! - Checking permissions against requests to determine allowed operations.
//! - Merging permissions from multiple sources, ensuring correct precedence and handling of capabilities.
//! - Managing and querying capabilities as bitmaps, converting between bit representations and string lists.
//! - Handling parameter rules, including merging and checking against allowed and denied lists.

use std::{collections::HashMap, str::FromStr, time::Duration};

use better_default::Default;
use dashmap::DashMap;
use derive_more::Display;
use hcl::{Body, Expression};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use strum::IntoEnumIterator;
use strum_macros::{Display as StrumDisplay, EnumIter, EnumString};

use super::acl::ACLResults;
use crate::{
    errors::RvError,
    logical::{Operation, Request, Response, auth::PolicyInfo},
    rv_error_string,
    utils::{
        deserialize_duration,
        string::{GlobContains, ensure_no_leading_slash},
    },
};

#[derive(Display, Copy, Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum PolicyType {
    #[display(fmt = "acl")]
    Acl,
    #[display(fmt = "rgp")]
    Rgp,
    #[display(fmt = "egp")]
    Egp,
    #[display(fmt = "token")]
    Token,
}

#[derive(Debug, Copy, Clone, Default, Serialize, Deserialize)]
pub struct SentinelPolicy {}

/// The `Policy` struct holds the main policy details including its rules and configurations.
#[derive(Debug, Clone, Default)]
pub struct Policy {
    pub sentinel_policy: SentinelPolicy,
    pub name: String,
    pub paths: Vec<PolicyPathRules>,
    pub raw: String,
    #[default(PolicyType::Acl)]
    pub policy_type: PolicyType,
    pub templated: bool,
}

/// Describes rules associated with specific paths in a policy.
#[derive(Debug, Clone, Default)]
pub struct PolicyPathRules {
    pub path: String,
    pub permissions: Permissions,
    pub capabilities: Vec<Capability>,
    pub is_prefix: bool,
    pub has_segment_wildcards: bool,
    pub min_wrapping_ttl: Duration,
    pub max_wrapping_ttl: Duration,
}

/// Structure holding permissions and associated configurations.
#[derive(Debug, Clone, Default)]
pub struct Permissions {
    pub capabilities_bitmap: u32,
    pub min_wrapping_ttl: Duration,
    pub max_wrapping_ttl: Duration,
    pub allowed_parameters: HashMap<String, Vec<Value>>,
    pub denied_parameters: HashMap<String, Vec<Value>>,
    pub required_parameters: Vec<String>,
    pub granting_policies_map: DashMap<u32, Vec<PolicyInfo>>,
}

// Configuration struct used to parse policy data from HCL.
#[derive(Debug, Default, Deserialize)]
struct PolicyConfig {
    pub name: String,
    pub path: HashMap<String, PolicyPathConfig>,
}

// Path-specific configuration used in policy definitions.
#[derive(Debug, Deserialize)]
struct PolicyPathConfig {
    #[serde(default)]
    pub policy: Option<OldPathPolicy>,
    #[serde(default)]
    pub capabilities: Vec<Capability>,
    #[serde(default, deserialize_with = "deserialize_duration")]
    pub min_wrapping_ttl: Duration,
    #[serde(default, deserialize_with = "deserialize_duration")]
    pub max_wrapping_ttl: Duration,
    #[serde(default)]
    pub allowed_parameters: HashMap<String, Vec<Value>>,
    #[serde(default)]
    pub denied_parameters: HashMap<String, Vec<Value>>,
    #[serde(default)]
    pub required_parameters: Vec<String>,
}

/// Enumeration of backwards capabilities, old-style policies.
#[derive(Debug, StrumDisplay, Copy, Clone, PartialEq, Eq, EnumString, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum OldPathPolicy {
    #[strum(to_string = "deny")]
    Deny,
    #[strum(to_string = "read")]
    Read,
    #[strum(to_string = "write")]
    Write,
    #[strum(to_string = "sudo")]
    Sudo,
}

/// Enumeration of possible capabilities, supporting string conversion and iteration.
#[derive(Debug, StrumDisplay, Copy, Clone, PartialEq, Eq, EnumString, EnumIter, Deserialize)]
#[serde(rename_all = "lowercase")]
#[repr(u32)]
pub enum Capability {
    #[strum(to_string = "deny")]
    Deny = 1 << 0,
    #[strum(to_string = "create")]
    Create = 1 << 1,
    #[strum(to_string = "read")]
    Read = 1 << 2,
    #[strum(to_string = "update")]
    Update = 1 << 3,
    #[strum(to_string = "delete")]
    Delete = 1 << 4,
    #[strum(to_string = "list")]
    List = 1 << 5,
    #[strum(to_string = "sudo")]
    Sudo = 1 << 6,
    #[strum(to_string = "patch")]
    Patch = 1 << 7,
    #[strum(to_string = "root")]
    Root = 1 << 8,
}

impl Capability {
    /// Converts a capability to its bit representation.
    pub fn to_bits(&self) -> u32 {
        *self as u32
    }
}

impl FromStr for Policy {
    type Err = RvError;

    /// Parses a string into a Policy struct. The input string can be in HCL format.
    /// It constructs a `Policy` object by parsing paths and associated configurations from the input.
    ///
    /// # Arguments
    ///
    /// * `s` - A string slice that holds the representation of the policy configuration.
    ///
    /// # Returns
    ///
    /// * `Ok(Policy)` if parsing succeeds.
    /// * `Err(RvError)` if the input string is malformed or if there are invalid configurations.
    ///
    /// # Errors
    ///
    /// * Returns an error if path contains invalid wildcards (`+*`).
    /// * Returns an error if `allowed_parameters` or `denied_parameters` are not arrays.
    ///
    /// # Examples
    ///
    /// ```
    /// use std::str::FromStr;
    /// use libvault::modules::policy::Policy;
    ///
    /// let policy_str = r#"
    /// path "secret/*" {
    ///     capabilities = ["read", "list"]
    ///     min_wrapping_ttl = "1h"
    ///     max_wrapping_ttl = "24h"
    ///     allowed_parameters = {"key1" = ["value1", "value2"]}
    ///     denied_parameters = {"key2" = ["value3", "value4"]}
    ///     required_parameters = ["param1", "param2"]
    /// }
    /// "#;
    ///
    /// let policy = Policy::from_str(policy_str);
    /// assert!(policy.is_ok());
    /// ```
    fn from_str(s: &str) -> Result<Self, RvError> {
        let policy_config = Policy::parse(s)?;

        let mut policy = Policy::default();
        policy.raw = s.to_string();
        policy.name.clone_from(&policy_config.name);

        policy.init(&policy_config)?;

        Ok(policy)
    }
}

impl Policy {
    /// Dummy method to input sentinel policy data; currently does nothing.
    pub fn input_sentinel_policy_data(&mut self, _req: &Request) -> Result<(), RvError> {
        Ok(())
    }

    /// Dummy method to add sentinel policy data; currently does nothing.
    pub fn add_sentinel_policy_data(&self, _resp: &Response) -> Result<(), RvError> {
        Ok(())
    }

    fn parse(s: &str) -> Result<PolicyConfig, RvError> {
        let body: Body = hcl::from_str(s)?;

        let mut policy_config = PolicyConfig::default();

        for attribute in body.attributes() {
            if attribute.key.as_str() == "name"
                && let Expression::String(name) = &attribute.expr
            {
                policy_config.name.clone_from(name);
            }
        }

        for block in body.blocks() {
            if block.identifier() == "path"
                && let Some(path_label) = block.labels().first()
            {
                let path_str = path_label.as_str().to_string();
                if path_str.contains("+*") {
                    return Err(rv_error_string!(&format!(
                        "path {path_str}: invalid use of wildcards ('+*' is forbidden)"
                    )));
                }

                let mut path_config: PolicyPathConfig = hcl::from_body(block.body().clone())?;
                let allowed_parameters: HashMap<String, Vec<Value>> = path_config
                    .allowed_parameters
                    .iter()
                    .map(|(key, value)| (key.to_lowercase(), value.clone()))
                    .collect();
                path_config.allowed_parameters = allowed_parameters;

                let denied_parameters: HashMap<String, Vec<Value>> = path_config
                    .denied_parameters
                    .iter()
                    .map(|(key, value)| (key.to_lowercase(), value.clone()))
                    .collect();
                path_config.denied_parameters = denied_parameters;

                if let Some(existing_path) = policy_config.path.get_mut(&path_str) {
                    // Collect new required parameters to be added
                    let new_params: Vec<String> = path_config
                        .required_parameters
                        .into_iter()
                        .filter(|x| !existing_path.required_parameters.contains(x))
                        .collect();

                    // Extend required_parameters with the new parameters
                    existing_path.required_parameters.extend(new_params);

                    // Merge allowed_parameters
                    for (key, mut value) in path_config.allowed_parameters {
                        if let Some(dst_vec) = existing_path.allowed_parameters.get_mut(&key) {
                            if value.is_empty() {
                                dst_vec.clear();
                            } else if !dst_vec.is_empty() {
                                dst_vec.append(&mut value);
                            }
                        } else {
                            existing_path
                                .allowed_parameters
                                .insert(key.to_lowercase(), value);
                        }
                    }

                    // Merge denied_parameters
                    for (key, mut value) in path_config.denied_parameters {
                        if let Some(dst_vec) = existing_path.denied_parameters.get_mut(&key) {
                            if value.is_empty() {
                                dst_vec.clear();
                            } else if !dst_vec.is_empty() {
                                dst_vec.append(&mut value);
                            }
                        } else {
                            existing_path
                                .denied_parameters
                                .insert(key.to_lowercase(), value);
                        }
                    }
                } else {
                    policy_config.path.insert(path_str, path_config);
                }
            }
        }

        Ok(policy_config)
    }

    fn init(&mut self, policy_config: &PolicyConfig) -> Result<(), RvError> {
        for (path, pc) in policy_config.path.iter() {
            let mut rules = PolicyPathRules::default();
            rules.path = ensure_no_leading_slash(path);
            rules.capabilities.clone_from(&pc.capabilities);
            rules.min_wrapping_ttl = pc.min_wrapping_ttl;
            rules.max_wrapping_ttl = pc.max_wrapping_ttl;

            if rules.path == "+" || rules.path.contains("/+") || rules.path.starts_with("+/") {
                rules.has_segment_wildcards = true;
            }

            // If there are segment wildcards, don't actually strip the
            // trailing asterisk, but don't want to hit the default case
            if rules.path.ends_with('*') && !rules.has_segment_wildcards {
                rules.path = rules.path.trim_end_matches('*').to_string();
                rules.is_prefix = true;
            }

            if let Some(old_path_policy) = pc.policy {
                match old_path_policy {
                    OldPathPolicy::Deny => {
                        rules.capabilities = vec![Capability::Deny];
                    }
                    OldPathPolicy::Read => {
                        rules
                            .capabilities
                            .extend(vec![Capability::Read, Capability::List]);
                    }
                    OldPathPolicy::Write => {
                        rules.capabilities.extend(vec![
                            Capability::Read,
                            Capability::List,
                            Capability::Create,
                            Capability::Update,
                            Capability::Delete,
                        ]);
                    }
                    OldPathPolicy::Sudo => {
                        rules.capabilities.extend(vec![
                            Capability::Read,
                            Capability::List,
                            Capability::Create,
                            Capability::Update,
                            Capability::Delete,
                            Capability::Sudo,
                        ]);
                    }
                }
            }

            let permissions = &mut rules.permissions;
            permissions.capabilities_bitmap = rules
                .capabilities
                .iter()
                .fold(0u32, |acc, cap| acc | cap.to_bits());
            if permissions.capabilities_bitmap & Capability::Deny.to_bits() != 0 {
                // If it's deny, don't include any other capability
                permissions.capabilities_bitmap = Capability::Deny.to_bits();
                rules.capabilities = vec![Capability::Deny];
            }

            permissions.min_wrapping_ttl = pc.min_wrapping_ttl;
            permissions.max_wrapping_ttl = pc.max_wrapping_ttl;

            permissions
                .allowed_parameters
                .clone_from(&pc.allowed_parameters);
            permissions
                .denied_parameters
                .clone_from(&pc.denied_parameters);
            permissions
                .required_parameters
                .clone_from(&pc.required_parameters);

            self.paths.push(rules);
        }

        Ok(())
    }
}

impl Permissions {
    /// Checks the permissions against a request to determine if it is allowed.
    /// Evaluates capabilities, required parameters, and allowed/denied parameters.
    pub fn check(&self, req: &Request, check_only: bool) -> Result<ACLResults, RvError> {
        let mut ret = ACLResults::default();
        let _path = ensure_no_leading_slash(&req.path);

        ret.root_privs = (self.capabilities_bitmap & Capability::Sudo.to_bits()) != 0;

        if check_only {
            ret.capabilities_bitmap = self.capabilities_bitmap;
            return Ok(ret);
        }

        let cap = match req.operation {
            Operation::Read => Capability::Read,
            Operation::List => Capability::List,
            Operation::Write => Capability::Update,
            Operation::Delete => Capability::Delete,
            Operation::Renew | Operation::Revoke | Operation::Rollback => Capability::Update,
            _ => return Ok(ret),
        };

        if self.capabilities_bitmap & cap.to_bits() == 0
            && (req.operation != Operation::Write
                || self.capabilities_bitmap & Capability::Create.to_bits() == 0)
        {
            return Ok(ret);
        }

        if let Some(value) = self.granting_policies_map.get(&cap.to_bits()) {
            ret.granting_policies.clone_from(&value);
        }

        let zero_ttl = Duration::from_secs(0);

        if self.max_wrapping_ttl > zero_ttl {
            // TODO
        }

        if self.min_wrapping_ttl > zero_ttl {
            // TODO
        }

        if self.min_wrapping_ttl != zero_ttl
            && self.max_wrapping_ttl != zero_ttl
            && self.max_wrapping_ttl < self.min_wrapping_ttl
        {
            return Ok(ret);
        }

        match req.operation {
            // Only check parameter permissions for operations that can modify parameters.
            Operation::Read | Operation::Write => {
                for parameter in self.required_parameters.iter() {
                    let key = parameter.to_lowercase();
                    if let Some(data) = &req.data
                        && data.get(key.as_str()).is_some()
                    {
                        continue;
                    }
                    if let Some(body) = &req.body
                        && body.get(key.as_str()).is_some()
                    {
                        continue;
                    }

                    return Ok(ret);
                }

                // If there are no data fields, allow
                if (req.data.is_none() || req.data.as_ref().unwrap().is_empty())
                    && (req.body.is_none() || req.body.as_ref().unwrap().is_empty())
                {
                    ret.capabilities_bitmap = self.capabilities_bitmap;
                    ret.allowed = true;
                    return Ok(ret);
                }

                if self.denied_parameters.contains_key("*") {
                    return Ok(ret);
                }

                for (param_key, param_value) in req.data_iter() {
                    if let Some(denied_parameters) = self
                        .denied_parameters
                        .get(param_key.to_lowercase().as_str())
                        && denied_parameters.glob_contains(param_value)
                    {
                        return Ok(ret);
                    }
                }

                let allowed_all = self.allowed_parameters.contains_key("*");

                if self.allowed_parameters.is_empty()
                    || (allowed_all && self.allowed_parameters.len() == 1)
                {
                    ret.capabilities_bitmap = self.capabilities_bitmap;
                    ret.allowed = true;
                    return Ok(ret);
                }

                for (param_key, param_value) in req.data_iter() {
                    if let Some(allowed_parameters) = self
                        .allowed_parameters
                        .get(param_key.to_lowercase().as_str())
                    {
                        if !allowed_parameters.glob_contains(param_value) {
                            return Ok(ret);
                        }
                    } else if !allowed_all {
                        return Ok(ret);
                    }
                }
            }
            _ => {}
        }

        ret.capabilities_bitmap = self.capabilities_bitmap;
        ret.allowed = true;

        Ok(ret)
    }

    /// Merges another set of permissions into the current set.
    /// Ensures that deny capabilities override others and merges parameter rules.
    pub fn merge(&mut self, other: &Permissions) -> Result<(), RvError> {
        let deny = Capability::Deny.to_bits();
        if self.capabilities_bitmap & deny != 0 {
            // If we are explicitly denied in the existing capability set, don't save anything else
            return Ok(());
        }
        if other.capabilities_bitmap & deny != 0 {
            // If this new policy explicitly denies, only save the deny value
            self.capabilities_bitmap = deny;
            self.allowed_parameters.clear();
            self.denied_parameters.clear();
            return Ok(());
        }

        self.capabilities_bitmap |= other.capabilities_bitmap;

        let zero_ttl = Duration::from_secs(0);

        // If we have an existing max, and we either don't have a current max, or the current is
        // greater than the previous, use the existing.
        if other.max_wrapping_ttl > zero_ttl
            && (self.max_wrapping_ttl == zero_ttl || self.max_wrapping_ttl < other.max_wrapping_ttl)
        {
            self.max_wrapping_ttl = other.max_wrapping_ttl;
        }

        // If we have an existing min, and we either don't have a current min, or the current is
        // greater than the previous, use the existing
        if other.min_wrapping_ttl > zero_ttl
            && (self.min_wrapping_ttl == zero_ttl || self.min_wrapping_ttl < other.min_wrapping_ttl)
        {
            self.min_wrapping_ttl = other.min_wrapping_ttl;
        }

        if !other.allowed_parameters.is_empty() {
            if self.allowed_parameters.is_empty() {
                self.allowed_parameters
                    .clone_from(&other.allowed_parameters);
            } else {
                for (key, value) in other.allowed_parameters.iter() {
                    if let Some(dst_vec) = self.allowed_parameters.get_mut(key) {
                        if value.is_empty() {
                            dst_vec.clear();
                        } else if !dst_vec.is_empty() {
                            dst_vec.extend(value.iter().cloned());
                        }
                    } else {
                        self.allowed_parameters.insert(key.clone(), value.clone());
                    }
                }
            }
        }

        if !other.denied_parameters.is_empty() {
            if self.denied_parameters.is_empty() {
                self.denied_parameters.clone_from(&other.denied_parameters);
            } else {
                for (key, value) in other.denied_parameters.iter() {
                    if let Some(dst_vec) = self.denied_parameters.get_mut(key) {
                        if value.is_empty() {
                            dst_vec.clear();
                        } else if !dst_vec.is_empty() {
                            dst_vec.extend(value.iter().cloned());
                        }
                    } else {
                        self.denied_parameters.insert(key.clone(), value.clone());
                    }
                }
            }
        }

        if !other.required_parameters.is_empty() {
            for param in other.required_parameters.iter() {
                if !self.required_parameters.contains(param) {
                    self.required_parameters.push(param.clone());
                }
            }
        }

        Ok(())
    }

    pub fn add_granting_policy_to_map(
        &mut self,
        policy: &Policy,
        capabilities_bitmap: u32,
    ) -> Result<(), RvError> {
        for cap in Capability::iter() {
            if cap.to_bits() & capabilities_bitmap == 0 {
                continue;
            }

            let pi = PolicyInfo {
                name: policy.name.clone(),
                policy_type: "acl".into(),
                ..Default::default()
            };

            self.granting_policies_map
                .entry(cap.to_bits())
                .or_default()
                .push(pi);
        }

        Ok(())
    }

    pub fn get_granting_capabilities(&self) -> Vec<String> {
        to_granting_capabilities(self.capabilities_bitmap)
    }
}

/// Converts a bitmask to a vector of capability strings.
///
/// This function takes a 32-bit integer representing a bitmask of capabilities and converts it into
/// a vector of string representations of the enabled capabilities. The capabilities are defined in
/// the `Capability` enum, and each capability has a corresponding bit position.
///
/// # Arguments
///
/// * `value` - A 32-bit integer representing the bitmask of capabilities.
///
/// # Returns
///
/// A vector of strings, where each string is the name of an enabled capability.
///
/// # Examples
///
/// ```
/// use libvault::modules::policy::policy::{Capability, to_granting_capabilities};
///
/// let bitmask = Capability::Read.to_bits() | Capability::Update.to_bits();
/// let capabilities = to_granting_capabilities(bitmask);
/// assert_eq!(capabilities, vec!["read", "update"]);
/// ```
pub fn to_granting_capabilities(value: u32) -> Vec<String> {
    let mut ret = Vec::new();
    let deny = Capability::Deny;
    if value & deny.to_bits() != 0 {
        ret.push(deny.to_string());
        return ret;
    }

    for cap in Capability::iter() {
        if cap.to_bits() & value != 0 {
            ret.push(cap.to_string());
        }
    }

    ret
}
