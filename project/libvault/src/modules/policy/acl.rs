//! This Rust file defines the implementation of Access Control Lists (ACLs) for handling
//! authorization and permissions within a security context.
//!
//! The primary structures include:
//! - `AuthResults`: Stores the results of authentication checks, including ACL and sentinel results.
//! - `ACLResults`: Contains detailed results from ACL checks, such as permissions and capabilities.
//! - `SentinelResults`: Holds information about granting policies determined by sentinels.
//! - `ACL`: Manages rules and policies related to access control, using data structures like `Trie`
//!   and `DashMap` for efficient storage and retrieval.
//!
//! Key Functionality:
//! - Constructing an ACL from a list of policies.
//! - Checking if a requested operation is allowed based on the defined ACL rules.
//! - Managing permission rules with support for exact, prefix, and segment wildcard path matching.
//! - Storing and retrieving permissions with efficiency using data structures optimized for this purpose.
//!
//! External Dependencies:
//! - Uses `radix_trie` for efficient storage and retrieval of path rules.
//! - Relies on `dashmap` for concurrent access to wildcard path permissions.

use std::sync::Arc;

use better_default::Default;
use dashmap::DashMap;
use radix_trie::{Trie, TrieCommon};

use super::{
    Permissions, Policy, PolicyPathRules, PolicyType,
    policy::{Capability, to_granting_capabilities},
};
use crate::{
    errors::RvError,
    logical::{Operation, Request, auth::PolicyInfo},
    rv_error_string,
    utils::string::ensure_no_leading_slash,
};

/// Stores the results of an authentication check, including ACL and sentinel results.
#[derive(Debug, Clone, Default)]
pub struct AuthResults {
    pub acl_results: ACLResults,
    pub sentinel_results: SentinelResults,
    pub allowed: bool,
    pub root_privs: bool,
    pub denied_error: bool,
}

/// Contains the outcome of an ACL check, including capabilities and granting policies.
#[derive(Debug, Clone, Default)]
pub struct ACLResults {
    pub allowed: bool,
    pub root_privs: bool,
    pub is_root: bool,
    pub capabilities_bitmap: u32,
    pub granting_policies: Vec<PolicyInfo>,
}

/// Stores results specific to sentinel checks.
#[derive(Debug, Clone, Default)]
pub struct SentinelResults {
    pub granting_policies: Vec<PolicyInfo>,
}

/// Represents an ACL system, containing rules for exact matches, prefixes, and segment wildcards.
#[derive(Debug, Clone, Default)]
pub struct ACL {
    pub exact_rules: Trie<String, Permissions>,
    pub prefix_rules: Trie<String, Permissions>,
    pub segment_wildcard_paths: DashMap<String, Permissions>,
    pub rgp_policies: Vec<Arc<Policy>>,
    #[default(false)]
    pub root: bool,
}

#[derive(Debug, Clone, Default)]
struct WcPathDescr {
    first_wc_or_glob: isize,
    wc_path: String,
    is_prefix: bool,
    wildcards: usize,
    perms: Option<Permissions>,
}

impl PartialEq for WcPathDescr {
    fn eq(&self, other: &Self) -> bool {
        self.first_wc_or_glob == other.first_wc_or_glob
            && self.wc_path == other.wc_path
            && self.is_prefix == other.is_prefix
            && self.wildcards == other.wildcards
    }
}

impl Eq for WcPathDescr {}

impl PartialOrd for WcPathDescr {
    fn partial_cmp(&self, other: &Self) -> Option<std::cmp::Ordering> {
        Some(self.cmp(other))
    }
}

impl Ord for WcPathDescr {
    fn cmp(&self, other: &Self) -> std::cmp::Ordering {
        self.first_wc_or_glob
            .cmp(&other.first_wc_or_glob)
            .then_with(|| other.is_prefix.cmp(&self.is_prefix))
            .then_with(|| other.wildcards.cmp(&self.wildcards))
            .then_with(|| self.wc_path.len().cmp(&other.wc_path.len()))
            .then_with(|| self.wc_path.cmp(&other.wc_path))
    }
}

impl ACL {
    /// Constructs a new `ACL` from a slice of policies.
    ///
    /// This method processes each policy, checking for `Rgp` and `Acl` types. It inserts rules into
    /// appropriate structures based on the path rules, managing exact matches, prefixes, and segment wildcards.
    ///
    /// # Arguments
    ///
    /// * `policies` - A slice of shared policies to initialize the ACL with.
    ///
    /// # Returns
    ///
    /// * `Result<Self, RvError>` - Returns an initialized `ACL` or an error if a policy type is incorrect.
    pub fn new(policies: &[Arc<Policy>]) -> Result<Self, RvError> {
        let mut acl = ACL::default();
        for policy in policies.iter() {
            if policy.policy_type == PolicyType::Rgp {
                acl.rgp_policies.push(policy.clone());
                continue;
            } else if policy.policy_type != PolicyType::Acl {
                return Err(rv_error_string!("unable to parse policy (wrong type)"));
            }

            if policy.name == "root" {
                if policies.len() != 1 {
                    return Err(rv_error_string!("other policies present along with root"));
                }
                acl.root = true;
            }

            for pr in policy.paths.iter() {
                if let Some(mut existing_perms) = acl.get_permissions(pr)? {
                    let deny = Capability::Deny.to_bits();
                    if existing_perms.capabilities_bitmap & deny != 0 {
                        // If we are explicitly denied in the existing capability set, don't save anything else
                        continue;
                    }

                    existing_perms.merge(&pr.permissions)?;
                    existing_perms
                        .add_granting_policy_to_map(policy, pr.permissions.capabilities_bitmap)?;
                    acl.insert_permissions(pr, existing_perms)?;
                } else {
                    let mut cloned_perms = pr.permissions.clone();
                    cloned_perms
                        .add_granting_policy_to_map(policy, pr.permissions.capabilities_bitmap)?;
                    acl.insert_permissions(pr, cloned_perms)?;
                }
            }
        }

        Ok(acl)
    }

    /// Retrieves permissions for a given path rule.
    ///
    /// This method checks for both segment wildcard paths and exact/prefix rules, returning the permissions
    /// if they exist for the given path.
    ///
    /// # Arguments
    ///
    /// * `pr` - A reference to a `PolicyPathRules` to find permissions for.
    ///
    /// # Returns
    ///
    /// * `Result<Option<Permissions>, RvError>` - Returns the permissions if found, otherwise `None`.
    pub fn get_permissions(&self, pr: &PolicyPathRules) -> Result<Option<Permissions>, RvError> {
        if pr.has_segment_wildcards {
            if let Some(existing_perms) = self.segment_wildcard_paths.get(&pr.path) {
                return Ok(Some(existing_perms.value().clone()));
            }
        } else {
            let tree = if pr.is_prefix {
                &self.prefix_rules
            } else {
                &self.exact_rules
            };

            if let Some(existing_perms) = tree.get(&pr.path) {
                return Ok(Some(existing_perms.clone()));
            }
        }

        Ok(None)
    }

    /// Inserts permissions into the appropriate rule set based on path rules.
    ///
    /// Depending on whether the path uses segment wildcards or is an exact/prefix rule, it inserts the
    /// permissions into the respective storage structure.
    ///
    /// # Arguments
    ///
    /// * `pr` - A reference to `PolicyPathRules` providing path info.
    /// * `perm` - The `Permissions` to insert.
    ///
    /// # Returns
    ///
    /// * `Result<(), RvError>` - Returns an error if insertion fails.
    pub fn insert_permissions(
        &mut self,
        pr: &PolicyPathRules,
        perm: Permissions,
    ) -> Result<(), RvError> {
        if pr.has_segment_wildcards {
            self.segment_wildcard_paths.insert(pr.path.clone(), perm);
        } else {
            let tree = if pr.is_prefix {
                &mut self.prefix_rules
            } else {
                &mut self.exact_rules
            };

            tree.insert(pr.path.clone(), perm);
        }

        Ok(())
    }

    /// Checks if an operation is allowed based on the ACL rules.
    ///
    /// This function checks various rules (exact matches, lists, prefixes, and wildcards) to determine
    /// if the operation specified in the request is allowed.
    ///
    /// # Arguments
    ///
    /// * `req` - A reference to the `Request` being checked.
    /// * `check_only` - A boolean indicating if the function should only perform a check without modifying state.
    ///
    /// # Returns
    ///
    /// * `Result<ACLResults, RvError>` - The result of the ACL check, indicating allowed operations and other details.
    pub fn allow_operation(&self, req: &Request, check_only: bool) -> Result<ACLResults, RvError> {
        if self.root {
            return Ok(ACLResults {
                allowed: true,
                root_privs: true,
                is_root: true,
                granting_policies: vec![PolicyInfo {
                    name: "root".into(),
                    namespace_id: "root".into(),
                    policy_type: "acl".into(),
                    ..Default::default()
                }],
                ..Default::default()
            });
        }

        if req.operation == Operation::Help {
            return Ok(ACLResults {
                allowed: true,
                ..Default::default()
            });
        }

        let path = ensure_no_leading_slash(&req.path);

        if let Some(perm) = self.exact_rules.get(&path) {
            return perm.check(req, check_only);
        }

        if req.operation == Operation::List
            && let Some(perm) = self.exact_rules.get(path.trim_end_matches('/'))
        {
            return perm.check(req, check_only);
        }

        if let Some(perm) = self.get_none_exact_paths_permissions(&path, false) {
            return perm.check(req, check_only);
        }

        Ok(ACLResults::default())
    }

    /// Retrieves permissions for a path that does not have an exact match.
    /// This function checks the prefix rules and segment wildcard paths to determine
    /// if any permissions apply to the given path.
    ///
    /// # Arguments
    ///
    /// * `path` - The path to check for permissions.
    /// * `bare_mount` - A flag indicating whether to use bare mount logic, affecting path matching.
    ///
    /// # Returns
    ///
    /// * `Option<Permissions>` - Returns permissions if found, otherwise `None`.
    pub fn get_none_exact_paths_permissions(
        &self,
        path: &str,
        bare_mount: bool,
    ) -> Option<Permissions> {
        let mut wc_path_descrs = Vec::with_capacity(self.segment_wildcard_paths.len() + 1);

        if let Some(item) = self.prefix_rules.get_ancestor(path) {
            if self.segment_wildcard_paths.is_empty() {
                return Some(item.value().unwrap().clone());
            }

            let prefix = item.key().unwrap().clone();
            wc_path_descrs.push(WcPathDescr {
                first_wc_or_glob: prefix.len() as isize,
                wc_path: prefix,
                is_prefix: true,
                perms: item.value().cloned(),
                ..Default::default()
            });
        }

        if self.segment_wildcard_paths.is_empty() {
            return None;
        }

        if self.segment_wildcard_paths.is_empty() {
            return None;
        }

        let path_parts: Vec<&str> = path.split('/').collect();

        for item in self.segment_wildcard_paths.iter() {
            let (full_wc_path, permissions) = (item.key(), item.value());

            if full_wc_path.is_empty() {
                continue;
            }

            let mut pd = WcPathDescr {
                first_wc_or_glob: full_wc_path.find('+').map(|i| i as isize).unwrap_or(-1),
                ..Default::default()
            };

            let mut curr_wc_path = full_wc_path.as_str();
            if curr_wc_path.ends_with('*') {
                pd.is_prefix = true;
                curr_wc_path = &curr_wc_path[..curr_wc_path.len() - 1];
            }
            pd.wc_path = curr_wc_path.to_string();

            let split_curr_wc_path: Vec<&str> = curr_wc_path.split('/').collect();

            if !bare_mount && path_parts.len() < split_curr_wc_path.len() {
                continue;
            }

            if !bare_mount && !pd.is_prefix && split_curr_wc_path.len() != path_parts.len() {
                continue;
            }

            let mut skip = false;
            let mut segments = Vec::with_capacity(split_curr_wc_path.len());

            for (i, acl_part) in split_curr_wc_path.iter().enumerate() {
                match *acl_part {
                    "+" => {
                        pd.wildcards += 1;
                        segments.push(path_parts[i]);
                    }
                    _ if *acl_part == path_parts[i] => {
                        segments.push(path_parts[i]);
                    }
                    _ if pd.is_prefix
                        && i == split_curr_wc_path.len() - 1
                        && path_parts[i].starts_with(acl_part) =>
                    {
                        segments.extend_from_slice(&path_parts[i..]);
                    }
                    _ if !bare_mount => {
                        skip = true;
                        break;
                    }
                    _ => {}
                }

                if bare_mount && i == path_parts.len() - 2 {
                    let joined_path = segments.join("/") + "/";
                    if joined_path.starts_with(path)
                        && permissions.capabilities_bitmap & Capability::Deny.to_bits() == 0
                        && permissions.capabilities_bitmap > 0
                    {
                        return Some(permissions.clone());
                    }
                    skip = true;
                    break;
                }
            }

            if !skip {
                pd.perms = Some(permissions.clone());
                wc_path_descrs.push(pd);
            }
        }

        if bare_mount || wc_path_descrs.is_empty() {
            return None;
        }

        wc_path_descrs.sort();

        wc_path_descrs
            .into_iter()
            .next_back()
            .and_then(|pd| pd.perms)
    }

    pub fn capabilities<S: Into<String>>(&self, path: S) -> Vec<String> {
        let mut req = Request::new(path);
        req.operation = Operation::List;

        let deny_response: Vec<String> = vec![Capability::Deny.to_string()];
        let res = match self.allow_operation(&req, true) {
            Ok(result) => result,
            Err(_) => return deny_response.clone(),
        };

        if res.is_root {
            return vec![Capability::Root.to_string()];
        }

        let capabilities = res.capabilities_bitmap;

        if capabilities & Capability::Deny.to_bits() > 0 {
            return deny_response.clone();
        }

        let path_capabilities = to_granting_capabilities(capabilities);

        if path_capabilities.is_empty() {
            return deny_response.clone();
        }

        path_capabilities
    }

    pub fn has_mount_access(&self, path: &str) -> bool {
        // If a policy is giving us direct access to the mount path then we can do a fast return.
        let capabilities = self.capabilities(path);
        if !capabilities.contains(&Capability::Deny.to_string()) {
            return true;
        }

        let mut acl_cap_given = check_path_capability(&self.exact_rules, path);
        if !acl_cap_given {
            acl_cap_given = check_path_capability(&self.prefix_rules, path);
        }

        if !acl_cap_given && self.get_none_exact_paths_permissions(path, true).is_some() {
            return true;
        }

        acl_cap_given
    }
}

fn check_path_capability(rules: &Trie<String, Permissions>, path: &str) -> bool {
    !path.is_empty()
        && rules
            .iter()
            .filter(|(p, perms)| {
                p.starts_with(path) && perms.capabilities_bitmap & Capability::Deny.to_bits() == 0
            })
            .any(|(_key, perms)| {
                perms.capabilities_bitmap
                    & (Capability::Create.to_bits()
                        | Capability::Delete.to_bits()
                        | Capability::List.to_bits()
                        | Capability::Read.to_bits()
                        | Capability::Sudo.to_bits()
                        | Capability::Update.to_bits()
                        | Capability::Patch.to_bits())
                    > 0
            })
}
