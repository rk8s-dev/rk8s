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

    let mut filtered_sorted_a: Vec<String> =
        a.iter().filter(|s| *s != "default").cloned().collect::<HashSet<_>>().into_iter().collect::<Vec<_>>();

    let mut filtered_sorted_b: Vec<String> =
        b.iter().filter(|s| *s != "default").cloned().collect::<HashSet<_>>().into_iter().collect::<Vec<_>>();

    filtered_sorted_a.sort();
    filtered_sorted_b.sort();

    filtered_sorted_a == filtered_sorted_b
}

#[cfg(test)]
mod test {
    use super::*;

    #[test]
    fn test_policy() {
        let mut policies1 = vec![
            String::from("Root"),
            String::from("root"),
            String::from("Admin"),
            String::from("Default"),
            String::from(""),
            String::from("Admin"),
        ];

        sanitize_policies(&mut policies1, true);
        assert_eq!(policies1, vec!["root".to_string()]);

        let mut policies2 = vec![
            String::from("rooot"),
            String::from("Admin"),
            String::from("Default"),
            String::from(""),
            String::from("Admin"),
        ];

        sanitize_policies(&mut policies2, true);
        assert_eq!(policies2, vec!["admin".to_string(), "default".to_string(), "rooot".to_string()]);

        let mut policies3 = vec![String::from("rooot"), String::from("Admin"), String::from(""), String::from("Admin")];

        sanitize_policies(&mut policies3, true);
        assert_eq!(policies3, vec!["admin".to_string(), "default".to_string(), "rooot".to_string()]);

        let mut policies4 = vec![String::from("")];

        sanitize_policies(&mut policies4, true);
        assert_eq!(policies4, vec!["default".to_string()]);

        let mut policies5 = Vec::new();

        sanitize_policies(&mut policies5, true);
        assert_eq!(policies5, vec!["default".to_string()]);

        let mut policies6 = Vec::new();

        sanitize_policies(&mut policies6, false);
        assert_eq!(policies6.len(), 0);

        let mut policies7 = vec![String::from("rooot"), String::from("Admin"), String::from(""), String::from("Admin")];

        sanitize_policies(&mut policies7, false);
        assert_eq!(policies7, vec!["admin".to_string(), "rooot".to_string()]);
    }

    #[test]
    fn test_equivalent_policies_both_empty() {
        let a: Vec<String> = vec![];
        let b: Vec<String> = vec![];
        assert!(equivalent_policies(&a, &b));
    }

    #[test]
    fn test_equivalent_policies_one_empty_other_default() {
        let a: Vec<String> = vec![];
        let b: Vec<String> = vec![String::from("default")];
        assert!(equivalent_policies(&a, &b));

        let a: Vec<String> = vec![String::from("default")];
        let b: Vec<String> = vec![];
        assert!(equivalent_policies(&a, &b));
    }

    #[test]
    fn test_equivalent_policies_one_empty_other_non_default() {
        let a: Vec<String> = vec![];
        let b: Vec<String> = vec![String::from("apple"), String::from("banana")];
        assert!(!equivalent_policies(&a, &b));

        let a: Vec<String> = vec![String::from("apple"), String::from("banana")];
        let b: Vec<String> = vec![];
        assert!(!equivalent_policies(&a, &b));
    }

    #[test]
    fn test_equivalent_policies_arrays() {
        let a = vec![
            String::from("apple"),
            String::from("default"),
            String::from("banana"),
            String::from("cherry"),
            String::from("default"),
            String::from("date"),
            String::from("apple"), // duplicated
        ];

        let b = vec![
            String::from("banana"),
            String::from("default"),
            String::from("cherry"),
            String::from("apple"),
            String::from("default"),
            String::from("date"),
        ];

        assert!(equivalent_policies(&a, &b));

        let b = vec![
            String::from("banana"),
            String::from("default"),
            String::from("cherry"),
            String::from("apple"),
            String::from("default"),
            String::from("grape"),
        ];

        assert!(!equivalent_policies(&a, &b));
    }

    #[test]
    fn test_equivalent_policies_mixed_elements() {
        let a = vec![
            String::from("apple"),
            String::from("default"),
            String::from("banana"),
            String::from("cherry"),
            String::from("default"),
            String::from("date"),
            String::from("apple"), // duplicated
        ];

        let b = vec![
            String::from("banana"),
            String::from("default"),
            String::from("cherry"),
            String::from("apple"),
            String::from("default"),
            String::from("date"),
            String::from("default"), // dup default
        ];

        assert!(equivalent_policies(&a, &b));
    }
}
