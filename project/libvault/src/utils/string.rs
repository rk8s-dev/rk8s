use std::collections::HashSet;

use serde_json::Value;

/// Removes duplicates from the given vector of strings.
///
/// # Parameters:
/// - `strings`: A mutable reference to a vector of strings from which duplicates will be removed.
/// - `stable`: A boolean indicating whether the order should be preserved.
/// - `lowercase`: A boolean indicating whether all strings should be converted to lowercase before processing.
///
/// If `stable` is true, the original order of the elements is maintained. If `lowercase` is true, comparison is case-insensitive.
///
/// # Example:
/// ```
/// use rusty_vault::utils::string::remove_duplicates;
///
/// let mut strings = vec![String::from("Apple"), String::from("apple"), String::from("Banana"), String::from("")];
/// remove_duplicates(&mut strings, true, true);
/// assert_eq!(strings, vec![String::from("apple"), String::from("banana")]);
/// ```
pub fn remove_duplicates(strings: &mut Vec<String>, stable: bool, lowercase: bool) {
    if stable {
        let mut seen = HashSet::new();
        let mut i = 0;
        while i < strings.len() {
            if lowercase {
                strings[i].make_ascii_lowercase();
            }
            if strings[i].trim().is_empty() || !seen.insert(strings[i].clone()) {
                strings.remove(i);
            } else {
                i += 1;
            }
        }
    } else {
        if lowercase {
            strings.iter_mut().for_each(|s| s.make_ascii_lowercase());
        }
        strings.retain(|s| !s.trim().is_empty());
        strings.sort();
        strings.dedup();
    }
}

/// Ensures that a given string ends with a trailing slash.
///
/// # Parameters:
/// - `s`: A string slice to be processed.
///
/// # Returns:
/// A new `String` that guarantees a trailing slash.
///
/// # Example:
/// ```
/// use rusty_vault::utils::string::ensure_trailing_slash;
///
/// assert_eq!(ensure_trailing_slash("example/path"), "example/path/");
/// assert_eq!(ensure_trailing_slash("example/path/"), "example/path/");
/// assert_eq!(ensure_trailing_slash(""), "");
/// ```
pub fn ensure_trailing_slash(s: &str) -> String {
    let s = s.trim();
    if s.is_empty() {
        return String::new();
    }

    let mut result = s.to_string();
    while !result.is_empty() && !result.ends_with('/') {
        result.push('/');
    }
    result
}

/// Ensures that a given string does not end with a trailing slash.
///
/// # Parameters:
/// - `s`: A string slice to be processed.
///
/// # Returns:
/// A new `String` that does not have a trailing slash.
///
/// # Example:
/// ```
/// use rusty_vault::utils::string::ensure_no_trailing_slash;
///
/// assert_eq!(ensure_no_trailing_slash("example/path/"), "example/path");
/// assert_eq!(ensure_no_trailing_slash("example/path"), "example/path");
/// assert_eq!(ensure_no_trailing_slash(""), "");
/// ```
pub fn ensure_no_trailing_slash(s: &str) -> String {
    let s = s.trim();
    if s.is_empty() {
        return String::new();
    }

    let mut result = s.to_string();
    while !result.is_empty() && result.ends_with('/') {
        result.pop();
    }
    result
}

/// Ensures that a given string does not start with a leading slash.
///
/// # Parameters:
/// - `s`: A string slice to be processed.
///
/// # Returns:
/// A new `String` without leading slashes.
///
/// # Example:
/// ```
/// use rusty_vault::utils::string::ensure_no_leading_slash;
///
/// assert_eq!(ensure_no_leading_slash("/example/path"), "example/path");
/// assert_eq!(ensure_no_leading_slash("example/path"), "example/path");
/// assert_eq!(ensure_no_leading_slash(""), "");
/// ```
pub fn ensure_no_leading_slash(s: &str) -> String {
    let s = s.trim();
    if s.is_empty() {
        return String::new();
    }

    let mut result = s.to_string();
    while !result.is_empty() && result.starts_with('/') {
        result.remove(0);
    }
    result
}

/// Matches a string `val` against a glob pattern `item`.
///
/// A glob pattern is a string that can start and/or end with an asterisk (*) and matches substrings.
/// - `*abc*` matches any string containing "abc".
/// - `*abc` matches any string ending with "abc".
/// - `abc*` matches any string starting with "abc".
///
/// # Parameters:
/// - `item`: The glob pattern.
/// - `val`: The string to match against the glob pattern.
///
/// # Returns:
/// A boolean indicating if `val` matches the `item` pattern.
///
/// # Example:
/// ```
/// use rusty_vault::utils::string::globbed_strings_match;
///
/// assert_eq!(globbed_strings_match("*abc*", "xabcx"), true);
/// assert_eq!(globbed_strings_match("*abc", "xabc"), true);
/// assert_eq!(globbed_strings_match("abc*", "abcx"), true);
/// assert_eq!(globbed_strings_match("abc", "abc"), true);
/// assert_eq!(globbed_strings_match("abc", "abcd"), false);
/// ```
pub fn globbed_strings_match(item: &str, val: &str) -> bool {
    if item.len() < 2 {
        return item == val;
    }

    let has_prefix = item.starts_with('*');
    let has_suffix = item.ends_with('*');

    if has_prefix && has_suffix {
        return val.contains(&item[1..item.len() - 1]);
    } else if has_prefix {
        return val.ends_with(&item[1..]);
    } else if has_suffix {
        return val.starts_with(&item[..item.len() - 1]);
    }

    item == val
}

pub trait GlobContains {
    fn glob_contains(&self, val: &Value) -> bool;
}

impl GlobContains for &Vec<Value> {
    fn glob_contains(&self, val: &Value) -> bool {
        if self.is_empty() {
            return true;
        }

        for item in self.iter() {
            if item.is_string() {
                if globbed_strings_match(item.as_str().unwrap(), val.as_str().unwrap_or_default()) {
                    return true;
                }
            } else {
                return self.contains(val);
            }
        }

        false
    }
}

impl GlobContains for Vec<Value> {
    fn glob_contains(&self, val: &Value) -> bool {
        (&self).glob_contains(val)
    }
}

#[cfg(test)]
mod mod_string_tests {
    use serde_json::json;

    use super::*;

    #[test]
    fn test_remove_duplicates() {
        let strings = vec![
            String::from("Orange"),
            String::from("Apple"),
            String::from("banana"),
            String::from(""),
            String::from("banana"),
            String::from(""),
            String::from(""),
            String::from(""),
            String::from("orange"),
            String::from(""),
        ];

        let mut strings1 = strings.clone();
        let mut strings2 = strings.clone();
        let mut strings3 = strings.clone();
        let mut strings4 = strings.clone();

        remove_duplicates(&mut strings1, true, true);
        assert_eq!(strings1, vec!["orange".to_string(), "apple".to_string(), "banana".to_string()]);

        remove_duplicates(&mut strings2, true, false);
        assert_eq!(
            strings2,
            vec!["Orange".to_string(), "Apple".to_string(), "banana".to_string(), "orange".to_string()]
        );

        remove_duplicates(&mut strings3, false, true);
        assert_eq!(strings3, vec!["apple".to_string(), "banana".to_string(), "orange".to_string()]);

        remove_duplicates(&mut strings4, false, false);
        assert_eq!(
            strings4,
            vec!["Apple".to_string(), "Orange".to_string(), "banana".to_string(), "orange".to_string()]
        );
    }

    #[test]
    fn test_glob_contains() {
        let patterns: Vec<Value> = vec![json!("*abc*"), json!("*def"), json!("ghi*"), json!("jkl"), json!("m*n*o")];
        let empty_patterns: Vec<Value> = vec![];

        assert_eq!(empty_patterns.glob_contains(&json!("any_string")), true);

        assert_eq!(patterns.glob_contains(&json!("defabcghi")), true); // *abc*
        assert_eq!(patterns.glob_contains(&json!("def")), true); // *def
        assert_eq!(patterns.glob_contains(&json!("ghijkl")), true); // ghi*
        assert_eq!(patterns.glob_contains(&json!("jkl")), true); // jkl

        assert_eq!(patterns.glob_contains(&json!("mnop")), false);
        assert_eq!(patterns.glob_contains(&json!("xyz")), false);
        assert_eq!(patterns.glob_contains(&json!("ab")), false);
        assert_eq!(patterns.glob_contains(&json!("efg")), false);
        assert_eq!(patterns.glob_contains(&json!("hij")), false);
        assert_eq!(patterns.glob_contains(&json!("k")), false);
        assert_eq!(patterns.glob_contains(&json!("lmn")), false);

        assert_eq!(patterns.glob_contains(&json!(42)), false);
        assert_eq!(patterns.glob_contains(&json!(true)), false);
        assert_eq!(patterns.glob_contains(&json!({"key": "value"})), false);
        assert_eq!(patterns.glob_contains(&json!([1, 2, 3])), false);

        let mixed_patterns: Vec<Value> =
            vec![json!("*abc*"), json!(42), json!(true), json!({"key": "value"}), json!([1, 2, 3])];

        assert_eq!(mixed_patterns.glob_contains(&json!("defabcghi")), true); // *abc*
        assert_eq!(mixed_patterns.glob_contains(&json!(42)), true); // 42
        assert_eq!(mixed_patterns.glob_contains(&json!(true)), true); // true
        assert_eq!(mixed_patterns.glob_contains(&json!({"key": "value"})), true); // {"key": "value"}
        assert_eq!(mixed_patterns.glob_contains(&json!([1, 2, 3])), true); // [1, 2, 3]
    }

    #[test]
    fn test_ensure_trailing_slash() {
        assert_eq!(ensure_trailing_slash("example"), "example/");
        assert_eq!(ensure_trailing_slash("example/"), "example/");
        assert_eq!(ensure_trailing_slash(""), "");
    }

    #[test]
    fn test_ensure_no_trailing_slash() {
        assert_eq!(ensure_no_trailing_slash("example/"), "example");
        assert_eq!(ensure_no_trailing_slash("example"), "example");
        assert_eq!(ensure_no_trailing_slash(""), "");
    }

    #[test]
    fn test_ensure_no_leading_slash() {
        assert_eq!(ensure_no_leading_slash("/example"), "example");
        assert_eq!(ensure_no_leading_slash("example"), "example");
        assert_eq!(ensure_no_leading_slash(""), "");
    }

    #[test]
    fn test_globbed_strings_match() {
        assert!(globbed_strings_match("exact", "exact"));
        assert!(!globbed_strings_match("exact", "notexact"));

        assert!(globbed_strings_match("pre*", "prefix"));
        assert!(!globbed_strings_match("pre*", "noprefix"));

        assert!(globbed_strings_match("*suf", "endsuf"));
        assert!(!globbed_strings_match("*suf", "nosuffix"));

        assert!(globbed_strings_match("*mid*", "inmiddle"));
        assert!(!globbed_strings_match("*mid*", "none"));

        assert!(globbed_strings_match("", ""));
        assert!(!globbed_strings_match("", "nonempty"));

        assert!(globbed_strings_match("a", "a"));
        assert!(!globbed_strings_match("b", "a"));
        assert!(!globbed_strings_match("a", "b"));

        assert!(globbed_strings_match("nowild", "nowild"));
        assert!(!globbed_strings_match("nowild", "wildhere"));
    }
}
