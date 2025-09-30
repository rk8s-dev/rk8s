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
    policy::{to_granting_capabilities, Capability},
    Permissions, Policy, PolicyPathRules, PolicyType,
};
use crate::{
    errors::RvError,
    logical::{auth::PolicyInfo, Operation, Request},
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
                    existing_perms.add_granting_policy_to_map(policy, pr.permissions.capabilities_bitmap)?;
                    acl.insert_permissions(pr, existing_perms)?;
                } else {
                    let mut cloned_perms = pr.permissions.clone();
                    cloned_perms.add_granting_policy_to_map(policy, pr.permissions.capabilities_bitmap)?;
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
            let tree = if pr.is_prefix { &self.prefix_rules } else { &self.exact_rules };

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
    pub fn insert_permissions(&mut self, pr: &PolicyPathRules, perm: Permissions) -> Result<(), RvError> {
        if pr.has_segment_wildcards {
            self.segment_wildcard_paths.insert(pr.path.clone(), perm);
        } else {
            let tree = if pr.is_prefix { &mut self.prefix_rules } else { &mut self.exact_rules };

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
            return Ok(ACLResults { allowed: true, ..Default::default() });
        }

        let path = ensure_no_leading_slash(&req.path);

        if let Some(perm) = self.exact_rules.get(&path) {
            return perm.check(req, check_only);
        }

        if req.operation == Operation::List {
            if let Some(perm) = self.exact_rules.get(path.trim_end_matches('/')) {
                return perm.check(req, check_only);
            }
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
    pub fn get_none_exact_paths_permissions(&self, path: &str, bare_mount: bool) -> Option<Permissions> {
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
                    _ if pd.is_prefix && i == split_curr_wc_path.len() - 1 && path_parts[i].starts_with(acl_part) => {
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

        wc_path_descrs.into_iter().next_back().and_then(|pd| pd.perms)
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
            .filter(|(p, perms)| p.starts_with(path) && perms.capabilities_bitmap & Capability::Deny.to_bits() == 0)
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

#[cfg(test)]
mod mod_policy_acl_tests {
    use std::{
        str::FromStr,
        sync::Arc,
        thread,
        time::{Duration, Instant},
    };

    use serde_json::{json, Map, Value};

    use super::*;
    use crate::logical::{Operation, Request};

    static TEST_ACL_POLICY: &str = r#"
name = "DeV"
path "dev/*" {
    policy = "sudo"
}
path "stage/*" {
    policy = "write"
}
path "stage/aws/*" {
    policy = "read"
    capabilities = ["update", "sudo"]
}
path "stage/aws/policy/*" {
    policy = "sudo"
}
path "prod/*" {
    policy = "read"
}
path "prod/aws/*" {
    policy = "deny"
}
path "sys/*" {
    policy = "deny"
}
path "foo/bar" {
    capabilities = ["read", "create", "sudo"]
}
path "baz/quux" {
    capabilities = ["read", "create", "patch"]
}
path "test/+/segment" {
    capabilities = ["read"]
}
path "+/segment/at/front" {
    capabilities = ["read"]
}
path "test/segment/at/end/+" {
    capabilities = ["read"]
}
path "test/segment/at/end/v2/+/" {
    capabilities = ["read"]
}
path "test/+/wildcard/+/*" {
    capabilities = ["read"]
}
path "test/+/wildcardglob/+/end*" {
    capabilities = ["read"]
}
path "1/2/*" {
    capabilities = ["create"]
}
path "1/2/+" {
    capabilities = ["read"]
}
path "1/2/+/+" {
    capabilities = ["update"]
}
    "#;

    static TEST_ACL_POLICY2: &str = r#"
name = "OpS"
path "dev/hide/*" {
    policy = "deny"
}
path "stage/aws/policy/*" {
    policy = "deny"
    # This should have no effect
    capabilities = ["read", "update", "sudo"]
}
path "prod/*" {
    policy = "write"
}
path "sys/seal" {
    policy = "sudo"
}
path "foo/bar" {
    capabilities = ["deny"]
}
path "baz/quux" {
    capabilities = ["deny"]
}
    "#;

    static TEST_MERGING_POLICIES: &str = r#"
name = "ops"
path "foo/bar" {
    policy = "write"
    denied_parameters = {
        "baz" = []
    }
    required_parameters = ["baz"]
}
path "foo/bar" {
    policy = "write"
    denied_parameters = {
        "zip" = []
    }
}
path "hello/universe" {
    policy = "write"
    allowed_parameters = {
        "foo" = []
    }
    required_parameters = ["foo"]
    max_wrapping_ttl = 300
    min_wrapping_ttl = 100
}
path "hello/universe" {
    policy = "write"
    allowed_parameters = {
        "bar" = []
    }
    required_parameters = ["bar"]
    max_wrapping_ttl = 200
    min_wrapping_ttl = 50
}
path "allow/all" {
    policy = "write"
    allowed_parameters = {
        "test" = []
        "test1" = ["foo"]
    }
}
path "allow/all" {
    policy = "write"
    allowed_parameters = {
        "*" = []
    }
}
path "allow/all1" {
    policy = "write"
    allowed_parameters = {
        "*" = []
    }
}
path "allow/all1" {
    policy = "write"
    allowed_parameters = {
        "test" = []
        "test1" = ["foo"]
    }
}
path "deny/all" {
    policy = "write"
    denied_parameters = {
        "test" = []
    }
}
path "deny/all" {
    policy = "write"
    denied_parameters = {
        "*" = []
    }
}
path "deny/all1" {
    policy = "write"
    denied_parameters = {
        "*" = []
    }
}
path "deny/all1" {
    policy = "write"
    denied_parameters = {
        "test" = []
    }
}
path "value/merge" {
    policy = "write"
    allowed_parameters = {
        "test" = [1, 2]
    }
    denied_parameters = {
        "test" = [1, 2]
    }
}
path "value/merge" {
    policy = "write"
    allowed_parameters = {
        "test" = [3, 4]
    }
    denied_parameters = {
        "test" = [3, 4]
    }
}
path "value/empty" {
    policy = "write"
    allowed_parameters = {
        "empty" = []
    }
    denied_parameters = {
        "empty" = [1]
    }
}
path "value/empty" {
    policy = "write"
    allowed_parameters = {
        "empty" = [1]
    }
    denied_parameters = {
        "empty" = []
    }
}
    "#;

    static TEST_PERMISSIONS_POLICY: &str = r#"
name = "dev"
path "dev/*" {
    policy = "write"
    allowed_parameters = {
        "zip" = []
    }
}
path "foo/bar" {
    policy = "write"
    denied_parameters = {
        "zap" = []
    }
    min_wrapping_ttl = 300
    max_wrapping_ttl = 400
}
path "foo/baz" {
    policy = "write"
    allowed_parameters = {
        "hello" = []
    }
    denied_parameters = {
        "zap" = []
    }
    min_wrapping_ttl = 300
}
path "working/phone" {
    policy = "write"
    max_wrapping_ttl = 400
}
path "broken/phone" {
    policy = "write"
    allowed_parameters = {
      "steve" = []
    }
    denied_parameters = {
      "steve" = []
    }
}
path "hello/world" {
    policy = "write"
    allowed_parameters = {
        "*" = []
    }
    denied_parameters = {
        "*" = []
    }
}
path "tree/fort" {
    policy = "write"
    allowed_parameters = {
        "*" = []
    }
    denied_parameters = {
        "foo" = []
    }
}
path "fruit/apple" {
    policy = "write"
    allowed_parameters = {
        "pear" = []
    }
    denied_parameters = {
        "*" = []
    }
}
path "cold/weather" {
    policy = "write"
    allowed_parameters = {}
    denied_parameters = {}
}
path "var/aws" {
    policy = "write"
    allowed_parameters = {
        "*" = []
    }
    denied_parameters = {
        "soft" = []
        "warm" = []
        "kitty" = []
    }
}
path "var/req" {
    policy = "write"
    required_parameters = ["foo"]
}
    "#;

    static TEST_VALUE_PERMISSIONS_POLICY: &str = r#"
name = "op"
path "dev/*" {
    policy = "write"
    allowed_parameters = {
        "allow" = ["good"]
    }
}
path "foo/bar" {
    policy = "write"
    denied_parameters = {
        "deny" = ["bad*"]
    }
}
path "foo/baz" {
    policy = "write"
    allowed_parameters = {
        "ALLOW" = ["good"]
    }
    denied_parameters = {
        "dEny" = ["bad"]
    }
}
path "fizz/buzz" {
    policy = "write"
    allowed_parameters = {
        "allow_multi" = ["good", "good1", "good2", "*good3"]
        "allow" = ["good"]
    }
    denied_parameters = {
        "deny_multi" = ["bad", "bad1", "bad2"]
    }
}
path "test/types" {
    policy = "write"
    allowed_parameters = {
        "map" = [{"good" = "one"}]
        "int" = [1, 2]
        "bool" = [false]
    }
    denied_parameters = {
    }
}
path "test/star" {
    policy = "write"
    allowed_parameters = {
        "*" = []
        "foo" = []
        "bar" = [false]
    }
    denied_parameters = {
    }
}
    "#;

    static TEST_GRANTING_TEST_POLICY: &str = r#"
name = "granting_policy"
path "kv/foo" {
    capabilities = ["update", "read"]
}

path "kv/path/*" {
    capabilities = ["read"]
}

path "kv/path/longer" {
    capabilities = ["update", "read"]
}

path "kv/path/longer2" {
    capabilities = ["update"]
}

path "kv/deny" {
    capabilities = ["deny"]
}

path "ns1/kv/foo" {
    capabilities = ["update", "read"]
}
    "#;

    static TEST_GRANTING_TEST_POLICY_MERGED: &str = r#"
name = "granting_policy_merged"
path "kv/foo" {
    capabilities = ["update", "read"]
}

path "kv/bar" {
    capabilities = ["update", "read"]
}

path "kv/path/*" {
    capabilities = ["read"]
}

path "kv/path/longer" {
    capabilities = ["read"]
}

path "kv/path/longer3" {
    capabilities = ["read"]
}

path "kv/deny" {
    capabilities = ["update"]
}
    "#;

    fn create_test_policy(name: &str, policy_str: &str) -> Policy {
        let policy: Result<Policy, RvError> = Policy::from_str(policy_str);
        assert!(policy.is_ok());
        let mut policy = policy.unwrap();
        if !name.is_empty() {
            policy.name = name.into();
        }
        policy
    }

    #[derive(Debug)]
    struct BatchTestCase(Operation, &'static str, bool, bool);

    fn acl_batch_test(acl: &ACL, cases: &[BatchTestCase]) {
        for case in cases.iter() {
            let req = Request { operation: case.0, path: case.1.to_string(), ..Default::default() };

            let result = acl.allow_operation(&req, false).unwrap();
            assert_eq!(case.2, result.allowed);
            assert_eq!(case.3, result.root_privs);
        }
    }

    #[test]
    fn test_sort_wc_path_descrs() {
        let mut wc_path_descrs = vec![
            WcPathDescr {
                first_wc_or_glob: 3,
                wc_path: String::from("path/a"),
                is_prefix: false,
                wildcards: 1,
                perms: None,
            },
            WcPathDescr {
                first_wc_or_glob: 1,
                wc_path: String::from("path/b"),
                is_prefix: true,
                wildcards: 0,
                perms: None,
            },
            WcPathDescr {
                first_wc_or_glob: 1,
                wc_path: String::from("path/c"),
                is_prefix: false,
                wildcards: 2,
                perms: None,
            },
            WcPathDescr {
                first_wc_or_glob: 2,
                wc_path: String::from("path/d"),
                is_prefix: true,
                wildcards: 0,
                perms: None,
            },
            WcPathDescr {
                first_wc_or_glob: 4,
                wc_path: String::from("path/e"),
                is_prefix: true,
                wildcards: 0,
                perms: None,
            },
            WcPathDescr {
                first_wc_or_glob: 4,
                wc_path: String::from("path/f"),
                is_prefix: true,
                wildcards: 1,
                perms: None,
            },
        ];

        wc_path_descrs.sort();

        let expected = vec![
            WcPathDescr {
                first_wc_or_glob: 1,
                wc_path: String::from("path/b"),
                is_prefix: true,
                wildcards: 0,
                perms: None,
            },
            WcPathDescr {
                first_wc_or_glob: 1,
                wc_path: String::from("path/c"),
                is_prefix: false,
                wildcards: 2,
                perms: None,
            },
            WcPathDescr {
                first_wc_or_glob: 2,
                wc_path: String::from("path/d"),
                is_prefix: true,
                wildcards: 0,
                perms: None,
            },
            WcPathDescr {
                first_wc_or_glob: 3,
                wc_path: String::from("path/a"),
                is_prefix: false,
                wildcards: 1,
                perms: None,
            },
            WcPathDescr {
                first_wc_or_glob: 4,
                wc_path: String::from("path/f"),
                is_prefix: true,
                wildcards: 1,
                perms: None,
            },
            WcPathDescr {
                first_wc_or_glob: 4,
                wc_path: String::from("path/e"),
                is_prefix: true,
                wildcards: 0,
                perms: None,
            },
        ];

        assert_eq!(wc_path_descrs, expected);
    }

    #[test]
    fn test_new_acl() {
        let policy1 = create_test_policy(
            "policy1",
            r#"
            path "path1/" {
                capabilities = ["read", "list"]
            }
            "#,
        );

        let policy2 = create_test_policy(
            "policy2",
            r#"
            path "path2/*" {
                capabilities = ["update", "delete"]
            }
            "#,
        );

        let acl = ACL::new(&[Arc::new(policy1), Arc::new(policy2)]).unwrap();

        assert_eq!(acl.root, false);
        assert_eq!(
            acl.exact_rules.get("path1/").unwrap().capabilities_bitmap,
            Capability::Read.to_bits() | Capability::List.to_bits()
        );
        assert_eq!(
            acl.prefix_rules.get_ancestor_value("path2/kk").unwrap().capabilities_bitmap,
            Capability::Update.to_bits() | Capability::Delete.to_bits()
        );
    }

    #[test]
    fn test_get_permissions() {
        let policy1 = create_test_policy(
            "policy1",
            r#"
            path "path1/" {
                capabilities = ["read", "list"]
            }

            path "path2*" {
                capabilities = ["update", "delete"]
            }
            "#,
        );

        let policy2 = create_test_policy(
            "policy2",
            r#"
            path "path2/*" {
                capabilities = ["update", "delete"]
            }
            "#,
        );

        let (policy1, policy2) = (Arc::new(policy1), Arc::new(policy2));

        let acl = ACL::new(&[policy1.clone(), policy2.clone()]).unwrap();

        let perm1 = acl.get_permissions(&policy1.paths[0]).unwrap().unwrap();
        if policy1.paths[0].path == "path1/" {
            assert_eq!(perm1.capabilities_bitmap, Capability::Read.to_bits() | Capability::List.to_bits());
        } else {
            assert_eq!(perm1.capabilities_bitmap, Capability::Update.to_bits() | Capability::Delete.to_bits());
        }

        let perm2 = acl.get_permissions(&policy2.paths[0]).unwrap().unwrap();
        assert_eq!(perm2.capabilities_bitmap, Capability::Update.to_bits() | Capability::Delete.to_bits());
    }

    #[test]
    fn test_insert_permissions() {
        let mut acl = ACL::default();
        let perm = Permissions { capabilities_bitmap: Capability::Read.to_bits(), ..Default::default() };

        acl.insert_permissions(
            &PolicyPathRules {
                path: "path1/".to_string(),
                is_prefix: false,
                has_segment_wildcards: false,
                ..Default::default()
            },
            perm.clone(),
        )
        .unwrap();

        assert_eq!(acl.exact_rules.get("path1/").unwrap().capabilities_bitmap, Capability::Read.to_bits());

        acl.insert_permissions(
            &PolicyPathRules {
                path: "path2".to_string(),
                is_prefix: true,
                has_segment_wildcards: false,
                ..Default::default()
            },
            perm,
        )
        .unwrap();

        assert_eq!(
            acl.prefix_rules.get_ancestor_value("path222").unwrap().capabilities_bitmap,
            Capability::Read.to_bits()
        );
    }

    #[test]
    fn test_get_none_exact_paths_permissions() {
        let policy = create_test_policy(
            "policy1",
            r#"
            path "path1/*" {
                capabilities = ["read"]
            }
            "#,
        );

        let acl = ACL::new(&[Arc::new(policy)]).unwrap();

        let result = acl.get_none_exact_paths_permissions("path1/subpath", false);
        assert!(result.is_some());
        assert_eq!(result.unwrap().capabilities_bitmap, Capability::Read.to_bits());
    }

    #[test]
    fn test_simple_allow_operation() {
        let policy = create_test_policy(
            "policy1",
            r#"
            path "path1/" {
                capabilities = ["read"]
            }
            "#,
        );

        let acl = ACL::new(&[Arc::new(policy)]).unwrap();

        let mut req = Request { operation: Operation::Read, path: "path1/".to_string(), ..Default::default() };

        let result = acl.allow_operation(&req, false).unwrap();
        assert!(result.allowed);
        assert!(!result.root_privs);
        assert!(!result.is_root);
        assert_eq!(result.capabilities_bitmap, Capability::Read.to_bits());

        req.path = "path2/".to_string();
        let result = acl.allow_operation(&req, false).unwrap();
        assert!(!result.allowed);
        assert!(!result.root_privs);
        assert!(!result.is_root);
        assert_eq!(result.capabilities_bitmap, 0);
    }

    #[test]
    fn test_acl_root() {
        let acl = ACL::new(&[Arc::new(Policy { name: "root".into(), ..Default::default() })]).unwrap();

        let req = Request { operation: Operation::Write, path: "sys/mount/foo".to_string(), ..Default::default() };

        let result = acl.allow_operation(&req, false).unwrap();
        assert!(result.allowed);
        assert!(result.root_privs);
        assert!(result.is_root);
    }

    #[test]
    fn test_acl_single() {
        let policy = create_test_policy("", TEST_ACL_POLICY);
        let acl = ACL::new(&[Arc::new(policy)]).unwrap();

        let cases = [
            BatchTestCase(Operation::Read, "root", false, false),
            BatchTestCase(Operation::Help, "root", true, false),
            BatchTestCase(Operation::Read, "dev/foo", true, true),
            BatchTestCase(Operation::Write, "dev/foo", true, true),
            BatchTestCase(Operation::Delete, "stage/foo", true, false),
            BatchTestCase(Operation::List, "stage/aws/foo", true, true),
            BatchTestCase(Operation::Write, "stage/aws/foo", true, true),
            BatchTestCase(Operation::Write, "stage/aws/policy/foo", true, true),
            BatchTestCase(Operation::Delete, "prod/foo", false, false),
            BatchTestCase(Operation::Write, "prod/foo", false, false),
            BatchTestCase(Operation::Read, "prod/foo", true, false),
            BatchTestCase(Operation::List, "prod/foo", true, false),
            BatchTestCase(Operation::Read, "prod/aws/foo", false, false),
            BatchTestCase(Operation::Read, "foo/bar", true, true),
            BatchTestCase(Operation::List, "foo/bar", false, true),
            //TODO
            //BatchTestCase(Operation::Update, "foo/bar", true, true),
            //BatchTestCase(Operation::Create, "foo/bar", false, true),
            BatchTestCase(Operation::Write, "foo/bar", true, true),
            BatchTestCase(Operation::Read, "baz/quux", true, false),
            //TODO
            //BatchTestCase(Operation::Patch, "baz/quux", true, false),
            BatchTestCase(Operation::List, "baz/quux", false, false),
            BatchTestCase(Operation::Write, "baz/quux", true, false),
            //TODO
            //BatchTestCase(Operation::Create, "baz/quux", true, false),
            //BatchTestCase(Operation::Update, "baz/quux", false, false),

            // Path segment wildcards
            BatchTestCase(Operation::Read, "test/foo/bar/segment", false, false),
            BatchTestCase(Operation::Read, "test/foo/segment", true, false),
            BatchTestCase(Operation::Read, "test/bar/segment", true, false),
            BatchTestCase(Operation::Read, "test/segment/at/frond", false, false),
            BatchTestCase(Operation::Read, "test/segment/at/front", true, false),
            BatchTestCase(Operation::Read, "test/segment/at/end/foo", true, false),
            BatchTestCase(Operation::Read, "test/segment/at/end/foo/", false, false),
            BatchTestCase(Operation::Read, "test/segment/at/end/v2/foo/", true, false),
            BatchTestCase(Operation::Read, "test/segment/wildcard/at/foo/", true, false),
            BatchTestCase(Operation::Read, "test/segment/wildcard/at/end", true, false),
            BatchTestCase(Operation::Read, "test/segment/wildcard/at/end/", true, false),
            // Path segment wildcards vs glob
            BatchTestCase(Operation::Read, "1/2/3/4", false, false),
            BatchTestCase(Operation::Read, "1/2/3", true, false),
            BatchTestCase(Operation::Write, "1/2/3", false, false),
            BatchTestCase(Operation::Write, "1/2/3/4", true, false),
            BatchTestCase(Operation::Write, "1/2/3/4/5", true, false),
        ];

        acl_batch_test(&acl, &cases);
    }

    #[test]
    fn test_acl_layered() {
        let policy1 = create_test_policy("", TEST_ACL_POLICY);
        let policy2 = create_test_policy("", TEST_ACL_POLICY2);
        let acl = ACL::new(&[Arc::new(policy1), Arc::new(policy2)]).unwrap();

        let cases = [
            BatchTestCase(Operation::Read, "root", false, false),
            BatchTestCase(Operation::Help, "root", true, false),
            BatchTestCase(Operation::Read, "dev/foo", true, true),
            BatchTestCase(Operation::Write, "dev/foo", true, true),
            BatchTestCase(Operation::Read, "dev/hide/foo", false, false),
            BatchTestCase(Operation::Write, "dev/hide/foo", false, false),
            BatchTestCase(Operation::Delete, "stage/foo", true, false),
            BatchTestCase(Operation::List, "stage/aws/foo", true, true),
            BatchTestCase(Operation::Write, "stage/aws/foo", true, true),
            BatchTestCase(Operation::Write, "stage/aws/policy/foo", false, false),
            BatchTestCase(Operation::Delete, "prod/foo", true, false),
            BatchTestCase(Operation::Write, "prod/foo", true, false),
            BatchTestCase(Operation::Read, "prod/foo", true, false),
            BatchTestCase(Operation::List, "prod/foo", true, false),
            BatchTestCase(Operation::Read, "prod/aws/foo", false, false),
            BatchTestCase(Operation::Read, "sys/status", false, false),
            BatchTestCase(Operation::Write, "sys/seal", true, true),
            BatchTestCase(Operation::Read, "foo/bar", false, false),
            BatchTestCase(Operation::List, "foo/bar", false, false),
            BatchTestCase(Operation::Write, "foo/bar", false, false),
            //TODO
            //BatchTestCase(Operation::Create, "foo/bar", false, false),
            BatchTestCase(Operation::Read, "baz/quux", false, false),
            BatchTestCase(Operation::List, "baz/quux", false, false),
            BatchTestCase(Operation::Write, "baz/quux", false, false),
            //TODO
            //BatchTestCase(Operation::Write, "baz/quux", false, false),
            //BatchTestCase(Operation::Patch, "baz/quux", false, false),
        ];

        acl_batch_test(&acl, &cases);
    }

    #[test]
    fn test_acl_policy_merge() {
        let policy = create_test_policy("", TEST_MERGING_POLICIES);
        let acl = ACL::new(&[Arc::new(policy)]).unwrap();

        #[derive(Debug)]
        struct Case(&'static str, Option<Value>, Option<Value>, Vec<&'static str>);

        let cases = [
            Case("foo/bar", None, Some(json!({"zip": [], "baz": []})), ["baz"].to_vec()),
            Case("hello/universe", Some(json!({"foo": [], "bar": []})), None, ["foo", "bar"].to_vec()),
            Case("allow/all", Some(json!({"*": [], "test": [], "test1": ["foo"]})), None, vec![]),
            Case("allow/all1", Some(json!({"*": [], "test": [], "test1": ["foo"]})), None, vec![]),
            Case("deny/all", None, Some(json!({"*": [], "test": []})), vec![]),
            Case("deny/all1", None, Some(json!({"*": [], "test": []})), vec![]),
            Case("value/merge", Some(json!({"test": [1, 2, 3, 4]})), Some(json!({"test": [1, 2, 3, 4]})), vec![]),
            Case("value/empty", Some(json!({"empty": []})), Some(json!({"empty": []})), vec![]),
        ];

        for case in cases.iter() {
            let result = acl.exact_rules.get(case.0);
            assert!(result.is_some());
            let result = result.unwrap();
            if let Some(allow) = &case.1 {
                let allowed_parameters = result
                    .allowed_parameters
                    .iter()
                    .map(|(key, value)| {
                        let array: Vec<Value> = value.iter().map(|s| s.clone()).collect();
                        (key.clone(), Value::Array(array))
                    })
                    .collect();
                assert_eq!(allow.as_object().unwrap(), &allowed_parameters);
            }
            if let Some(deny) = &case.2 {
                let denied_parameters = result
                    .denied_parameters
                    .iter()
                    .map(|(key, value)| {
                        let array: Vec<Value> = value.iter().map(|s| s.clone()).collect();
                        (key.clone(), Value::Array(array))
                    })
                    .collect();
                assert_eq!(deny.as_object().unwrap(), &denied_parameters);
            }
            if !case.3.is_empty() {
                let required: Vec<String> = case.3.iter().map(|s| s.to_string()).collect();
                assert_eq!(&required, &result.required_parameters);
            }
        }
    }

    #[test]
    fn test_acl_allow_operation() {
        let policy = create_test_policy("", TEST_PERMISSIONS_POLICY);
        let acl = ACL::new(&[Arc::new(policy)]).unwrap();

        #[derive(Debug)]
        struct Case(&'static str, Vec<&'static str>, bool);

        let cases = [
            Case("dev/ops", ["zip"].to_vec(), true),
            Case("foo/bar", ["zap"].to_vec(), false),
            Case("foo/bar", ["zip"].to_vec(), true),
            //TODO: add req ttl test
            //Case("foo/bar", ["zip"].to_vec(), false),
            //Case("foo/bar", ["zip"].to_vec(), false),
            //Case("foo/bar", ["zip"].to_vec(), true),
            Case("foo/baz", ["hello"].to_vec(), true),
            //TODO: add req ttl test
            //Case("foo/baz", ["hello"].to_vec(), false),
            //Case("foo/baz", ["hello"].to_vec(), true),
            //Case("foo/baz", ["zap"].to_vec(), false),
            Case("broken/phone", ["steve"].to_vec(), false),
            Case("working/phone", [""].to_vec(), true),
            //TODO: add req ttl test
            //Case("working/phone", [""].to_vec(), false),
            //Case("working/phone", [""].to_vec(), true),
            Case("hello/world", ["one"].to_vec(), false),
            Case("tree/fort", ["one"].to_vec(), true),
            Case("tree/fort", ["foo"].to_vec(), false),
            Case("fruit/apple", ["pear"].to_vec(), false),
            Case("fruit/apple", ["one"].to_vec(), false),
            Case("cold/weather", ["four"].to_vec(), true),
            Case("var/aws", ["cold", "warm", "kitty"].to_vec(), false),
            Case("var/req", ["cold", "warm", "kitty"].to_vec(), false),
            Case("var/req", ["cold", "warm", "kitty", "foo"].to_vec(), true),
        ];

        for case in cases.iter() {
            let mut req = Request { operation: Operation::Write, path: case.0.to_string(), ..Default::default() };

            let mut data: Map<String, Value> = Map::new();
            for parameter in case.1.iter() {
                data.insert(parameter.to_string(), Value::String("".into()));
            }

            req.body = Some(data);

            let result = acl.allow_operation(&req, false).unwrap();
            assert_eq!(case.2, result.allowed);
        }
    }

    #[test]
    fn test_acl_value_permissions() {
        let policy = create_test_policy("", TEST_VALUE_PERMISSIONS_POLICY);
        let acl = ACL::new(&[Arc::new(policy)]).unwrap();

        #[derive(Debug)]
        struct Case(&'static str, Vec<&'static str>, Vec<Value>, bool);

        let cases = [
            Case("dev/ops", ["allow"].to_vec(), [json!("good")].to_vec(), true),
            Case("dev/ops", ["allow"].to_vec(), [json!("bad")].to_vec(), false),
            Case("foo/bar", ["deny"].to_vec(), [json!("bad")].to_vec(), false),
            Case("foo/bar", ["deny"].to_vec(), [json!("bad glob")].to_vec(), false),
            Case("foo/bar", ["deny"].to_vec(), [json!("good")].to_vec(), true),
            Case("foo/bar", ["allow"].to_vec(), [json!("good")].to_vec(), true),
            Case("foo/bar", ["deny"].to_vec(), [Value::Null].to_vec(), true),
            Case("foo/bar", ["allow"].to_vec(), [Value::Null].to_vec(), true),
            Case("foo/baz", ["aLLow"].to_vec(), [json!("good")].to_vec(), true),
            Case("foo/baz", ["deny"].to_vec(), [json!("bad")].to_vec(), false),
            Case("foo/baz", ["deny"].to_vec(), [json!("good")].to_vec(), false),
            Case("foo/baz", ["allow", "deny"].to_vec(), [json!("good"), json!("bad")].to_vec(), false),
            Case("foo/baz", ["deny", "allow"].to_vec(), [json!("good"), json!("bad")].to_vec(), false),
            Case("foo/baz", ["deNy", "allow"].to_vec(), [json!("bad"), json!("good")].to_vec(), false),
            Case("foo/baz", ["aLLow"].to_vec(), [json!("bad")].to_vec(), false),
            Case("foo/baz", ["Neither"].to_vec(), [json!("bad")].to_vec(), false),
            Case("foo/baz", ["allow"].to_vec(), [Value::Null].to_vec(), false),
            Case("fizz/buzz", ["allow_multi"].to_vec(), [json!("good")].to_vec(), true),
            Case("fizz/buzz", ["allow_multi"].to_vec(), [json!("good1")].to_vec(), true),
            Case("fizz/buzz", ["allow_multi"].to_vec(), [json!("good2")].to_vec(), true),
            Case("fizz/buzz", ["allow_multi"].to_vec(), [json!("glob good2")].to_vec(), false),
            Case("fizz/buzz", ["allow_multi"].to_vec(), [json!("glob good3")].to_vec(), true),
            Case("fizz/buzz", ["allow_multi"].to_vec(), [json!("bad")].to_vec(), false),
            Case("fizz/buzz", ["allow_multi"].to_vec(), [json!("bad")].to_vec(), false),
            Case("fizz/buzz", ["allow_multi", "allow"].to_vec(), [json!("good1"), json!("good")].to_vec(), true),
            Case("fizz/buzz", ["deny_multi"].to_vec(), [json!("bad2")].to_vec(), false),
            Case("fizz/buzz", ["deny_multi", "allow_multi"].to_vec(), [json!("good"), json!("good2")].to_vec(), false),
            Case("test/types", ["map"].to_vec(), [json!({"good": "one"})].to_vec(), true),
            Case("test/types", ["map"].to_vec(), [json!({"bad": "one"})].to_vec(), false),
            Case("test/types", ["int"].to_vec(), [json!(1)].to_vec(), true),
            Case("test/types", ["int"].to_vec(), [json!(3)].to_vec(), false),
            Case("test/types", ["bool"].to_vec(), [json!(false)].to_vec(), true),
            Case("test/types", ["bool"].to_vec(), [json!(true)].to_vec(), false),
            Case("test/star", ["anything"].to_vec(), [json!(true)].to_vec(), true),
            Case("test/star", ["foo"].to_vec(), [json!(true)].to_vec(), true),
            Case("test/star", ["bar"].to_vec(), [json!(false)].to_vec(), true),
            Case("test/star", ["bar"].to_vec(), [json!(true)].to_vec(), false),
        ];

        for case in cases.iter() {
            let mut req = Request { operation: Operation::Write, path: case.0.to_string(), ..Default::default() };

            let mut data: Map<String, Value> = Map::new();
            let mut i = 0;
            for parameter in case.1.iter() {
                data.insert(parameter.to_string(), case.2[i].clone());
                i += 1;
            }

            req.body = Some(data);

            let result = acl.allow_operation(&req, false).unwrap();
            assert_eq!(case.3, result.allowed);
        }
    }

    #[test]
    fn test_acl_segment_wildcard_priority() {
        #[derive(Debug)]
        struct Case(&'static str, &'static str);

        // These test cases should each have a read rule and an update rule, where
        // the update rule wins out due to being more specific.
        let cases = [
            Case(
                // Verify edge conditions.  Here '*' is more specific both because
                // of first wildcard position (0 vs -1/infinity) and #wildcards.
                r#"
                    path "+/*" { capabilities = ["read"] }
                    path "*" { capabilities = ["update"] }
                "#,
                "foo/bar/bar/baz",
            ),
            Case(
                // Verify edge conditions.  Here '+/*' is less specific because of
                // first wildcard position.
                r#"
                    path "+/*" { capabilities = ["read"] }
                    path "foo/+/*" { capabilities = ["update"] }
                "#,
                "foo/bar/bar/baz",
            ),
            Case(
                // Verify that more wildcard segments is lower priority.
                r#"
                    path "foo/+/+/*" { capabilities = ["read"] }
                    path "foo/+/bar/baz" { capabilities = ["update"] }
                "#,
                "foo/bar/bar/baz",
            ),
            Case(
                // Verify that more wildcard segments is lower priority.
                r#"
                    path "foo/+/+/baz" { capabilities = ["read"] }
                    path "foo/+/bar/baz" { capabilities = ["update"] }
                "#,
                "foo/bar/bar/baz",
            ),
            Case(
                // Verify that first wildcard position is lower priority.
                // '(' is used here because it is lexicographically smaller than "+"
                r#"
                    path "foo/+/(ar/baz" { capabilities = ["read"] }
                    path "foo/(ar/+/baz" { capabilities = ["update"] }
                "#,
                "foo/(ar/(ar/baz",
            ),
            Case(
                // Verify that a glob has lower priority, even if the prefix is the
                // same otherwise.
                r#"
                    path "foo/bar/+/baz*" { capabilities = ["read"] }
                    path "foo/bar/+/baz" { capabilities = ["update"] }
                "#,
                "foo/bar/bar/baz",
            ),
            Case(
                // Verify that a shorter prefix has lower priority.
                r#"
                    path "foo/bar/+/b*" { capabilities = ["read"] }
                    path "foo/bar/+/ba*" { capabilities = ["update"] }
                "#,
                "foo/bar/bar/baz",
            ),
        ];

        for case in cases.iter() {
            let policy = create_test_policy("", case.0);
            let acl = ACL::new(&[Arc::new(policy)]).unwrap();

            let mut req = Request { operation: Operation::Write, path: case.1.to_string(), ..Default::default() };

            let result = acl.allow_operation(&req, false).unwrap();
            assert!(result.allowed);

            req.operation = Operation::Read;
            let result = acl.allow_operation(&req, false).unwrap();
            assert!(!result.allowed);
        }
    }

    #[test]
    fn test_acl_segment_wildcard_priority_bare_mount() {
        #[derive(Debug)]
        struct Case(&'static str, &'static str, bool);

        // These test cases should have one or more rules and a mount prefix.
        // hasperms should be true if there are non-deny perms that apply
        // to the mount prefix or something below it.
        let cases = [
            Case(r#"path "+" { capabilities = ["read"] }"#, "foo/", true),
            Case(r#"path "+/*" { capabilities = ["read"] }"#, "foo/", true),
            Case(r#"path "foo/+/+/*" { capabilities = ["read"] }"#, "foo/", true),
            Case(r#"path "foo/+/+/*" { capabilities = ["read"] }"#, "foo/bar/", true),
            Case(r#"path "foo/+/+/*" { capabilities = ["read"] }"#, "foo/bar/bar/", true),
            Case(r#"path "foo/+/+/*" { capabilities = ["read"] }"#, "foo/bar/bar/baz/", true),
            Case(r#"path "foo/+/+/baz" { capabilities = ["read"] }"#, "foo/bar/bar/baz/", true),
            Case(r#"path "foo/+/bar/baz" { capabilities = ["read"] }"#, "foo/bar/bar/baz/", true),
            Case(r#"path "foo/bar/+/baz*" { capabilities = ["read"] }"#, "foo/bar/bar/baz/", true),
            Case(r#"path "foo/bar/+/b*" { capabilities = ["read"] }"#, "foo/bar/bar/baz/", true),
            Case(r#"path "foo/+" { capabilities = ["read"] }"#, "foo/", true),
        ];

        for case in cases.iter() {
            let policy = create_test_policy("", case.0);
            let acl = ACL::new(&[Arc::new(policy)]).unwrap();

            let result = acl.get_none_exact_paths_permissions(case.1, true);
            println!("case: {:?}", case);
            println!("result: {:?}", result);
            assert_eq!(result.is_some(), case.2);
        }
    }

    #[test]
    fn test_acl_creation_race() {
        let policy = Arc::new(create_test_policy("", TEST_VALUE_PERMISSIONS_POLICY));

        let mut threads = Vec::new();
        let stop_time = Instant::now() + Duration::from_secs(20);

        for _i in 0..50 {
            let p = policy.clone();
            threads.push(thread::spawn(move || loop {
                if Instant::now() >= stop_time {
                    break;
                }
                assert!(ACL::new(&[p.clone()]).is_ok());
            }));
        }

        for thread in threads {
            thread.join().unwrap();
        }
    }

    #[test]
    fn test_acl_granting_policies() {
        let policy = Arc::new(create_test_policy("", TEST_GRANTING_TEST_POLICY));
        let merged = Arc::new(create_test_policy("", TEST_GRANTING_TEST_POLICY_MERGED));

        let policy_info = PolicyInfo {
            name: "granting_policy".into(),
            namespace_id: "".into(),
            namespace_path: "".into(),
            policy_type: "acl".into(),
        };
        let merged_info = PolicyInfo {
            name: "granting_policy_merged".into(),
            namespace_id: "".into(),
            namespace_path: "".into(),
            policy_type: "acl".into(),
        };

        #[derive(Debug)]
        struct Case(&'static str, Operation, Vec<Arc<Policy>>, Vec<PolicyInfo>, bool);

        let cases = [
            Case("kv/foo", Operation::Read, [policy.clone()].to_vec(), [policy_info.clone()].to_vec(), true),
            Case("kv/foo", Operation::Write, [policy.clone()].to_vec(), [policy_info.clone()].to_vec(), true),
            Case("kv/bad", Operation::Read, [policy.clone()].to_vec(), vec![], false),
            Case("kv/deny", Operation::Read, [policy.clone()].to_vec(), vec![], false),
            Case("kv/path/foo", Operation::Read, [policy.clone()].to_vec(), [policy_info.clone()].to_vec(), true),
            Case("kv/path/longer", Operation::Read, [policy.clone()].to_vec(), [policy_info.clone()].to_vec(), true),
            Case(
                "kv/foo",
                Operation::Read,
                [policy.clone(), merged.clone()].to_vec(),
                [policy_info.clone(), merged_info.clone()].to_vec(),
                true,
            ),
            Case(
                "kv/path/longer3",
                Operation::Read,
                [policy.clone(), merged.clone()].to_vec(),
                [merged_info.clone()].to_vec(),
                true,
            ),
            Case(
                "kv/bar",
                Operation::Read,
                [policy.clone(), merged.clone()].to_vec(),
                [merged_info.clone()].to_vec(),
                true,
            ),
            Case("kv/deny", Operation::Read, [policy.clone(), merged.clone()].to_vec(), vec![], false),
            Case(
                "kv/path/longer",
                Operation::Write,
                [policy.clone(), merged.clone()].to_vec(),
                [policy_info.clone()].to_vec(),
                true,
            ),
            Case(
                "kv/path/foo",
                Operation::Read,
                [policy.clone(), merged.clone()].to_vec(),
                [policy_info.clone(), merged_info.clone()].to_vec(),
                true,
            ),
        ];
        for case in cases.iter() {
            let acl = ACL::new(&case.2).unwrap();

            let req = Request { operation: case.1, path: case.0.to_string(), ..Default::default() };

            let result = acl.allow_operation(&req, false).unwrap();
            assert_eq!(case.3, result.granting_policies);
            assert_eq!(case.4, result.allowed);
        }
    }

    #[test]
    fn test_acl_capabilities() {
        let policy = create_test_policy("", TEST_ACL_POLICY);
        let acl = ACL::new(&[Arc::new(policy)]).unwrap();

        let caps = acl.capabilities("dev");
        assert_eq!(caps, vec![Capability::Deny.to_string()]);

        let caps = acl.capabilities("dev/");
        assert_eq!(
            caps,
            vec![
                Capability::Create.to_string(),
                Capability::Read.to_string(),
                Capability::Update.to_string(),
                Capability::Delete.to_string(),
                Capability::List.to_string(),
                Capability::Sudo.to_string()
            ]
        );

        let caps = acl.capabilities("stage/aws/foo");
        assert_eq!(
            caps,
            vec![
                Capability::Read.to_string(),
                Capability::Update.to_string(),
                Capability::List.to_string(),
                Capability::Sudo.to_string()
            ]
        );

        let caps = acl.capabilities("prod/foo");
        assert_eq!(caps, vec![Capability::Read.to_string(), Capability::List.to_string()]);

        let caps = acl.capabilities("sys/mount");
        assert_eq!(caps, vec![Capability::Deny.to_string()]);
    }

    #[test]
    fn test_check_path_capability() {
        let mut rules = Trie::new();
        rules.insert(
            "/api".to_string(),
            Permissions { capabilities_bitmap: Capability::Deny.to_bits(), ..Default::default() },
        );
        rules.insert(
            "/api/v1".to_string(),
            Permissions {
                capabilities_bitmap: Capability::Read.to_bits() | Capability::List.to_bits(),
                ..Default::default()
            },
        );
        rules.insert(
            "/api/v2".to_string(),
            Permissions { capabilities_bitmap: Capability::Create.to_bits(), ..Default::default() },
        );
        rules.insert(
            "/api/v3".to_string(),
            Permissions { capabilities_bitmap: Capability::Deny.to_bits(), ..Default::default() },
        );
        rules.insert(
            "/admin".to_string(),
            Permissions { capabilities_bitmap: Capability::Sudo.to_bits(), ..Default::default() },
        );
        rules.insert(
            "/root".to_string(),
            Permissions { capabilities_bitmap: Capability::Deny.to_bits(), ..Default::default() },
        );
        rules.insert(
            "".to_string(),
            Permissions { capabilities_bitmap: Capability::Read.to_bits(), ..Default::default() },
        );

        assert!(check_path_capability(&rules, "/api/v1"));
        assert!(check_path_capability(&rules, "/api/v2"));
        assert!(!check_path_capability(&rules, "/api/v3"));
        assert!(!check_path_capability(&rules, "/unknown/path"));
        assert!(check_path_capability(&rules, "/admin"));
        assert!(!check_path_capability(&rules, "/root"));
        assert!(!check_path_capability(&rules, ""));
        assert!(check_path_capability(&rules, "/api"));
    }
}
