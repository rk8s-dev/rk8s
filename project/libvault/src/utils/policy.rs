//! This module is a Rust replica of
//! https://github.com/hashicorp/vault/blob/main/sdk/helper/policyutil/policyutil.go

use std::collections::HashSet;

use super::string::remove_duplicates;

// sanitize_policies performs the common input validation tasks
// which are performed on the list of policies across RustyVault.
// The resulting collection will have no duplicate elements.
// If 'root' policy was present in the list of policies, then
// all other policies will be ignored, the result will contain
// just the 'root'. In cases where 'root' is not present, if
// 'default' policy is not already present, it will be added
// if add_default is set to true.
pub fn sanitize_policies(policies: &mut Vec<String>, add_default: bool) {
    let mut default_found = false;
    for p in policies.iter() {
        let q = p.trim().to_lowercase();
        if q.is_empty() {
            continue;
        }

        // If 'root' policy is present, ignore all other policies.
        if q == "root" {
            policies.clear();
            policies.push("root".to_string());
            default_found = true;
            break;
        }
        if q == "default" {
            default_found = true;
        }
    }

    // Always add 'default' except only if the policies contain 'root'.
    if add_default && (!default_found || policies.is_empty()) {
        policies.push("default".to_string());
    }

    remove_duplicates(policies, false, true)
}

// equivalent_policies checks whether the given policy sets are equivalent, as in,
// they contain the same values. The benefit of this method is that it leaves
// the "default" policy out of its comparisons as it may be added later by core
// after a set of policies has been saved by a backend.
pub fn equivalent_policies(a: &Vec<String>, b: &Vec<String>) -> bool {
    if (a.is_empty() && b.is_empty())
        || (a.is_empty() && b.len() == 1 && b[0] == "default")
        || (b.is_empty() && a.len() == 1 && a[0] == "default")
    {
        return true;
    } else if a.is_empty() || b.is_empty() {
        return false;
    }

    let mut filtered_sorted_a: Vec<String> = a
        .iter()
        .filter(|s| *s != "default")
        .cloned()
        .collect::<HashSet<_>>()
        .into_iter()
        .collect::<Vec<_>>();

    let mut filtered_sorted_b: Vec<String> = b
        .iter()
        .filter(|s| *s != "default")
        .cloned()
        .collect::<HashSet<_>>()
        .into_iter()
        .collect::<Vec<_>>();

    filtered_sorted_a.sort();
    filtered_sorted_b.sort();

    filtered_sorted_a == filtered_sorted_b
}
