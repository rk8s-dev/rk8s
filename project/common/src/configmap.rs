//! ConfigMap objects and pure validation, based on Kubernetes v1.34 semantics.
//!
//! Deserialization is not admission: callers must validate before storing an
//! object. The API must also enforce its metadata policy, request namespace,
//! server-owned identity fields and resourceVersion preconditions. These types
//! do not enable ConfigMap consumption by Pods.

use crate::ObjectMeta;
use serde::{Deserialize, Deserializer, Serialize, Serializer};
use std::collections::BTreeMap;
use std::fmt;

/// Combined UTF-8 data values and decoded binaryData values, excluding metadata.
pub const CONFIG_MAP_MAX_SIZE: usize = 1_048_576;

#[derive(Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ConfigMap {
    pub api_version: String,
    pub kind: String,
    pub metadata: ObjectMeta,
    #[serde(
        default,
        deserialize_with = "deserialize_data",
        skip_serializing_if = "BTreeMap::is_empty"
    )]
    pub data: BTreeMap<String, String>,
    #[serde(
        default,
        with = "binary_data",
        skip_serializing_if = "BTreeMap::is_empty"
    )]
    pub binary_data: BTreeMap<String, Vec<u8>>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub immutable: Option<bool>,
}

impl ConfigMap {
    /// Total value bytes, not character count or base64/JSON wire length.
    pub fn data_size(&self) -> usize {
        self.data
            .values()
            .map(String::len)
            .chain(self.binary_data.values().map(Vec::len))
            .sum()
    }

    /// Validate identity syntax, map keys and the combined value size.
    ///
    /// This is deliberately independent of storage and does not assign a UID,
    /// timestamp or version, or validate all ObjectMeta admission constraints.
    pub fn validate(&self) -> Result<(), ConfigMapValidationError> {
        if self.api_version != "v1" {
            return Err(invalid("apiVersion", "must be v1"));
        }
        if self.kind != "ConfigMap" {
            return Err(invalid("kind", "must be ConfigMap"));
        }
        if !is_dns_subdomain(&self.metadata.name) {
            return Err(invalid(
                "metadata.name",
                "must be a DNS subdomain of at most 253 bytes",
            ));
        }
        if self.metadata.namespace.len() > 63 || !is_dns_label(&self.metadata.namespace) {
            return Err(invalid(
                "metadata.namespace",
                "must be a DNS label of at most 63 bytes",
            ));
        }
        for key in self.data.keys() {
            validate_key("data", key)?;
            if self.binary_data.contains_key(key) {
                return Err(invalid(
                    format!("data[{key:?}]"),
                    "key must not also appear in binaryData",
                ));
            }
        }
        for key in self.binary_data.keys() {
            validate_key("binaryData", key)?;
        }
        if self.data_size() > CONFIG_MAP_MAX_SIZE {
            return Err(invalid(
                "data/binaryData",
                "combined value size must not exceed 1048576 bytes",
            ));
        }
        Ok(())
    }

    /// Validate a replacement after the API has preserved server-owned fields.
    /// CAS and resourceVersion checks belong to the storage transaction.
    pub fn validate_update(&self, old: &Self) -> Result<(), ConfigMapValidationError> {
        self.validate()?;
        if self.metadata.name != old.metadata.name {
            return Err(invalid("metadata.name", "must not change during an update"));
        }
        if self.metadata.namespace != old.metadata.namespace {
            return Err(invalid(
                "metadata.namespace",
                "must not change during an update",
            ));
        }
        if self.metadata.uid != old.metadata.uid {
            return Err(invalid("metadata.uid", "must not change during an update"));
        }
        if old.immutable == Some(true) {
            if self.immutable != Some(true) {
                return Err(invalid("immutable", "cannot be unset once true"));
            }
            if self.data != old.data {
                return Err(invalid("data", "cannot change when immutable is true"));
            }
            if self.binary_data != old.binary_data {
                return Err(invalid(
                    "binaryData",
                    "cannot change when immutable is true",
                ));
            }
        }
        Ok(())
    }
}

// ConfigMap values (and annotations) must not leak through diagnostic logging.
impl fmt::Debug for ConfigMap {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("ConfigMap")
            .field("name", &self.metadata.name)
            .field("namespace", &self.metadata.namespace)
            .field("uid", &self.metadata.uid)
            .field("resource_version", &self.metadata.resource_version)
            .field("data_entries", &self.data.len())
            .field("binary_data_entries", &self.binary_data.len())
            .field("data_size", &self.data_size())
            .field("immutable", &self.immutable)
            .finish()
    }
}

/// List snapshot version, distinct from each item's storage version.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ListMeta {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub resource_version: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ConfigMapList {
    pub api_version: String,
    pub kind: String,
    pub metadata: ListMeta,
    pub items: Vec<ConfigMap>,
}

/// An input error with a field path and a reason, never a configuration value.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ConfigMapValidationError {
    pub field: String,
    pub reason: &'static str,
}

impl fmt::Display for ConfigMapValidationError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}: {}", self.field, self.reason)
    }
}

impl std::error::Error for ConfigMapValidationError {}

fn invalid(field: impl Into<String>, reason: &'static str) -> ConfigMapValidationError {
    ConfigMapValidationError {
        field: field.into(),
        reason,
    }
}

fn is_dns_label(value: &str) -> bool {
    let is_alphanumeric = |b: u8| b.is_ascii_lowercase() || b.is_ascii_digit();
    !value.is_empty()
        && value.bytes().all(|b| is_alphanumeric(b) || b == b'-')
        && is_alphanumeric(value.as_bytes()[0])
        && is_alphanumeric(value.as_bytes()[value.len() - 1])
}

fn is_dns_subdomain(value: &str) -> bool {
    // Kubernetes IsDNS1123Subdomain limits the whole name, not each component.
    value.len() <= 253 && value.split('.').all(is_dns_label)
}

fn validate_key(field: &str, key: &str) -> Result<(), ConfigMapValidationError> {
    if key.is_empty()
        || key.len() > 253
        || key == "."
        || key.starts_with("..")
        || !key
            .bytes()
            .all(|b| b.is_ascii_alphanumeric() || matches!(b, b'.' | b'-' | b'_'))
    {
        return Err(invalid(
            format!("{field}[{key:?}]"),
            "key must contain 1-253 ASCII letters, digits, '.', '-' or '_', and must not be '.' or start with '..'",
        ));
    }
    Ok(())
}

fn deserialize_data<'de, D>(deserializer: D) -> Result<BTreeMap<String, String>, D::Error>
where
    D: Deserializer<'de>,
{
    deserialize_string_map(deserializer, "data")
}

fn deserialize_string_map<'de, D>(
    deserializer: D,
    field: &'static str,
) -> Result<BTreeMap<String, String>, D::Error>
where
    D: Deserializer<'de>,
{
    struct StringMapVisitor(&'static str);

    impl<'de> serde::de::Visitor<'de> for StringMapVisitor {
        type Value = BTreeMap<String, String>;

        fn expecting(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
            write!(f, "a string map or null for {}", self.0)
        }

        fn visit_unit<E: serde::de::Error>(self) -> Result<Self::Value, E> {
            Ok(BTreeMap::new())
        }

        fn visit_map<M: serde::de::MapAccess<'de>>(
            self,
            mut map: M,
        ) -> Result<Self::Value, M::Error> {
            let mut values = BTreeMap::new();
            while let Some(key) = map.next_key::<String>()? {
                let value = map.next_value::<StrictString>().map_err(|_| {
                    serde::de::Error::custom(format!("{}[{key:?}]: value must be a string", self.0))
                })?;
                values.insert(key, value.0);
            }
            Ok(values)
        }
    }

    deserializer.deserialize_any(StringMapVisitor(field))
}

// serde_yaml's String deserializer also accepts numbers and booleans. ConfigMap
// values must retain JSON string semantics rather than silently coerce them.
struct StrictString(String);

impl<'de> Deserialize<'de> for StrictString {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        struct StringVisitor;

        impl serde::de::Visitor<'_> for StringVisitor {
            type Value = StrictString;

            fn expecting(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
                f.write_str("a string")
            }

            fn visit_str<E: serde::de::Error>(self, value: &str) -> Result<Self::Value, E> {
                Ok(StrictString(value.to_owned()))
            }

            fn visit_string<E: serde::de::Error>(self, value: String) -> Result<Self::Value, E> {
                Ok(StrictString(value))
            }
        }

        deserializer.deserialize_any(StringVisitor)
    }
}

mod binary_data {
    use super::*;
    use base64::Engine;
    use base64::alphabet;
    use base64::engine::{GeneralPurpose, GeneralPurposeConfig};
    use serde::ser::SerializeMap;

    // Match Go's base64.StdEncoding: padded standard alphabet, permissive pad
    // bits, with CR/LF ignored on decode. Always emit canonical base64.
    const BASE64: GeneralPurpose = GeneralPurpose::new(
        &alphabet::STANDARD,
        GeneralPurposeConfig::new().with_decode_allow_trailing_bits(true),
    );

    pub fn serialize<S>(
        values: &BTreeMap<String, Vec<u8>>,
        serializer: S,
    ) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        let mut map = serializer.serialize_map(Some(values.len()))?;
        for (key, value) in values {
            map.serialize_entry(key, &BASE64.encode(value))?;
        }
        map.end()
    }

    pub fn deserialize<'de, D>(deserializer: D) -> Result<BTreeMap<String, Vec<u8>>, D::Error>
    where
        D: Deserializer<'de>,
    {
        let encoded = deserialize_string_map(deserializer, "binaryData")?;
        encoded
            .into_iter()
            .map(|(key, value)| {
                let encoded: Vec<u8> = value
                    .bytes()
                    .filter(|b| !matches!(b, b'\r' | b'\n'))
                    .collect();
                let bytes = BASE64.decode(encoded).map_err(|_| {
                    serde::de::Error::custom(format!(
                        "binaryData[{key:?}]: must contain padded standard base64"
                    ))
                })?;
                Ok((key, bytes))
            })
            .collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn config_map() -> ConfigMap {
        serde_json::from_value(json!({
            "apiVersion": "v1",
            "kind": "ConfigMap",
            "metadata": {"name": "app-config", "namespace": "demo"}
        }))
        .unwrap()
    }

    #[test]
    fn absent_null_and_empty_maps_normalize_in_json_and_yaml() {
        for suffix in [
            "",
            ",\"data\":null,\"binaryData\":null",
            ",\"data\":{},\"binaryData\":{}",
        ] {
            let json = format!(
                "{{\"apiVersion\":\"v1\",\"kind\":\"ConfigMap\",\"metadata\":{{\"name\":\"app\"}}{suffix}}}"
            );
            let cm: ConfigMap = serde_json::from_str(&json).unwrap();
            cm.validate().unwrap();
            assert!(cm.data.is_empty());
            assert!(cm.binary_data.is_empty());
            assert_eq!(cm.metadata.namespace, "default");
            assert_eq!(cm.immutable, None);
            let serialized = serde_json::to_value(&cm).unwrap();
            for omitted in ["data", "binaryData", "immutable", "spec"] {
                assert!(serialized.get(omitted).is_none());
            }
        }
        for suffix in [
            "",
            "data: null\nbinaryData: null\n",
            "data: {}\nbinaryData: {}\n",
        ] {
            let yaml = format!("apiVersion: v1\nkind: ConfigMap\nmetadata:\n  name: app\n{suffix}");
            let cm: ConfigMap = serde_yaml::from_str(&yaml).unwrap();
            cm.validate().unwrap();
            assert!(cm.data.is_empty() && cm.binary_data.is_empty());
        }
    }

    #[test]
    fn binary_data_roundtrips_as_base64_in_json_and_yaml() {
        let mut cm = config_map();
        cm.data.insert("unicode".into(), "配置\n\"\\".into());
        cm.data.insert("empty".into(), String::new());
        cm.binary_data.insert("blob".into(), vec![0, 255, 128]);
        cm.binary_data.insert("empty.bin".into(), vec![]);
        cm.immutable = Some(false);
        let json = serde_json::to_value(&cm).unwrap();
        assert_eq!(json["binaryData"]["blob"], "AP+A");
        assert_eq!(json["binaryData"]["empty.bin"], "");
        assert_eq!(json["immutable"], false);
        assert_eq!(serde_json::from_value::<ConfigMap>(json).unwrap(), cm);
        let yaml = serde_yaml::to_string(&cm).unwrap();
        assert!(yaml.contains("AP+A"));
        assert_eq!(serde_yaml::from_str::<ConfigMap>(&yaml).unwrap(), cm);
    }

    #[test]
    fn base64_decode_matches_go_padding_and_newline_behavior() {
        for (value, expected) in [("Zg==\r\n", b"f".as_slice()), ("AB==", b"\0".as_slice())] {
            let mut json = serde_json::to_value(config_map()).unwrap();
            json["binaryData"] = json!({"file": value});
            let cm: ConfigMap = serde_json::from_value(json).unwrap();
            assert_eq!(cm.binary_data["file"], expected);
        }
        for value in ["Zg", "Zg=", "Zg===", "_w==", "Z g==", "敏感配置"] {
            let mut json = serde_json::to_value(config_map()).unwrap();
            json["binaryData"] = json!({"file": value});
            let error = serde_json::from_value::<ConfigMap>(json)
                .unwrap_err()
                .to_string();
            assert!(error.contains("binaryData[\"file\"]"), "{error}");
            assert!(!error.contains(value), "{error}");
        }
    }

    #[test]
    fn rejects_unknown_fields_and_non_string_values() {
        for (field, value) in [
            ("spec", json!({})),
            ("binary_data", json!({})),
            ("data", json!({"key": 123})),
            ("data", json!({"key": true})),
            ("binaryData", json!({"key": [0, 255]})),
        ] {
            let mut json = serde_json::to_value(config_map()).unwrap();
            json[field] = value;
            assert!(
                serde_json::from_value::<ConfigMap>(json).is_err(),
                "{field}"
            );
        }
        let yaml = "apiVersion: v1\nkind: ConfigMap\nmetadata:\n  name: app\nbinaryData:\n  file: 'not base64!'\n";
        assert!(
            serde_yaml::from_str::<ConfigMap>(yaml)
                .unwrap_err()
                .to_string()
                .contains("binaryData[\"file\"]")
        );
    }

    #[test]
    fn yaml_values_are_strings_without_scalar_coercion() {
        for field in ["data", "binaryData"] {
            for value in ["123", "true", "null", "[0, 255]", "{nested: value}"] {
                let yaml = format!(
                    "apiVersion: v1\nkind: ConfigMap\nmetadata:\n  name: app\n{field}:\n  key: {value}\n"
                );
                let error = serde_yaml::from_str::<ConfigMap>(&yaml)
                    .unwrap_err()
                    .to_string();
                assert!(error.contains(&format!("{field}[\"key\"]")), "{error}");
            }
        }
        let yaml = "apiVersion: v1\nkind: ConfigMap\nmetadata:\n  name: app\ndata:\n  key: '123'\n  enabled: 'true'\n";
        let cm: ConfigMap = serde_yaml::from_str(yaml).unwrap();
        assert_eq!(cm.data["key"], "123");
        assert_eq!(cm.data["enabled"], "true");
    }

    #[test]
    fn validates_type_metadata() {
        let mut cm = config_map();
        cm.api_version = "v2".into();
        assert_eq!(cm.validate().unwrap_err().field, "apiVersion");
        cm.api_version = "v1".into();
        cm.kind = "Secret".into();
        assert_eq!(cm.validate().unwrap_err().field, "kind");
    }

    #[test]
    fn validates_name_and_namespace_boundaries() {
        let mut cm = config_map();
        for name in [
            "a".into(),
            "0".into(),
            "a-b.example".into(),
            "a".repeat(253),
        ] {
            cm.metadata.name = name;
            cm.validate().unwrap();
        }
        for name in [
            "", "A", "a_b", "a/b", ".a", "a.", "a..b", "-a", "a-", "配置",
        ] {
            cm.metadata.name = name.into();
            assert_eq!(cm.validate().unwrap_err().field, "metadata.name", "{name}");
        }
        cm.metadata.name = "a".repeat(254);
        assert_eq!(cm.validate().unwrap_err().field, "metadata.name");
        cm.metadata.name = "app".into();
        cm.metadata.namespace = "a".repeat(63);
        cm.validate().unwrap();
        for namespace in [
            "".into(),
            "a.b".into(),
            "UPPER".into(),
            "-a".into(),
            "a".repeat(64),
        ] {
            cm.metadata.namespace = namespace;
            assert_eq!(cm.validate().unwrap_err().field, "metadata.namespace");
        }
    }

    #[test]
    fn validates_keys_in_both_maps() {
        for key in [
            "".into(),
            ".".into(),
            "..".into(),
            "..data".into(),
            "a/b".into(),
            "a b".into(),
            "配置".into(),
            "a".repeat(254),
        ] {
            let mut cm = config_map();
            cm.data.insert(key.clone(), String::new());
            assert_eq!(cm.validate().unwrap_err().field, format!("data[{key:?}]"));
            cm.data.clear();
            cm.binary_data.insert(key.clone(), vec![]);
            assert_eq!(
                cm.validate().unwrap_err().field,
                format!("binaryData[{key:?}]")
            );
        }
        for key in [
            ".env".into(),
            "-".into(),
            "_".into(),
            "a..b".into(),
            "A_1.txt".into(),
            "a".repeat(253),
        ] {
            let mut cm = config_map();
            cm.data.insert(key.clone(), String::new());
            cm.validate().unwrap();
            cm.data.clear();
            cm.binary_data.insert(key, vec![]);
            cm.validate().unwrap();
        }
    }

    #[test]
    fn rejects_overlapping_keys_even_with_empty_values() {
        let mut cm = config_map();
        cm.data.insert("shared".into(), String::new());
        cm.binary_data.insert("shared".into(), vec![]);
        assert_eq!(cm.validate().unwrap_err().field, "data[\"shared\"]");
    }

    #[test]
    fn enforces_combined_size_at_the_exact_boundary() {
        for size in [
            CONFIG_MAP_MAX_SIZE - 1,
            CONFIG_MAP_MAX_SIZE,
            CONFIG_MAP_MAX_SIZE + 1,
        ] {
            let mut cm = config_map();
            cm.data.insert("text".into(), "a".repeat(size / 2));
            cm.binary_data
                .insert("blob".into(), vec![0xff; size - size / 2]);
            assert_eq!(cm.data_size(), size);
            assert_eq!(cm.validate().is_ok(), size <= CONFIG_MAP_MAX_SIZE);
        }
    }

    #[test]
    fn size_counts_utf8_and_decoded_bytes_not_wire_size_or_metadata() {
        let mut cm = config_map();
        cm.data
            .insert("text".into(), "中".repeat(CONFIG_MAP_MAX_SIZE / 3));
        cm.binary_data.insert("blob".into(), vec![0]);
        assert_eq!(cm.data_size(), CONFIG_MAP_MAX_SIZE);
        cm.validate().unwrap();
        cm.binary_data.get_mut("blob").unwrap().push(0);
        assert_eq!(cm.validate().unwrap_err().field, "data/binaryData");

        cm.data
            .insert("text".into(), "\0".repeat(CONFIG_MAP_MAX_SIZE));
        cm.binary_data.clear();
        cm.metadata
            .annotations
            .insert("note".into(), "not part of data quota".into());
        assert!(serde_json::to_vec(&cm).unwrap().len() > CONFIG_MAP_MAX_SIZE);
        cm.validate().unwrap();

        cm.data.clear();
        cm.binary_data
            .insert("blob".into(), vec![0xff; CONFIG_MAP_MAX_SIZE]);
        assert!(serde_json::to_vec(&cm).unwrap().len() > CONFIG_MAP_MAX_SIZE);
        cm.validate().unwrap();
    }

    #[test]
    fn immutable_blocks_data_changes_and_cannot_be_reverted() {
        let mut old = config_map();
        old.data.insert("text".into(), "original".into());
        old.binary_data.insert("blob".into(), vec![0]);
        old.immutable = Some(true);
        for immutable in [None, Some(false)] {
            let mut new = old.clone();
            new.immutable = immutable;
            assert_eq!(new.validate_update(&old).unwrap_err().field, "immutable");
        }
        let mut new = old.clone();
        new.data.insert("text".into(), "replacement".into());
        assert_eq!(new.validate_update(&old).unwrap_err().field, "data");
        new = old.clone();
        new.data.clear();
        assert_eq!(new.validate_update(&old).unwrap_err().field, "data");
        new = old.clone();
        new.binary_data.insert("blob".into(), vec![1]);
        assert_eq!(new.validate_update(&old).unwrap_err().field, "binaryData");
        new = old.clone();
        new.binary_data.clear();
        assert_eq!(new.validate_update(&old).unwrap_err().field, "binaryData");
    }

    #[test]
    fn immutable_allows_metadata_updates_and_mutable_maps_can_be_frozen() {
        let mut old = config_map();
        old.immutable = Some(true);
        let mut new = old.clone();
        new.metadata.labels.insert("app".into(), "demo".into());
        new.metadata.resource_version = Some("opaque-version".into());
        new.validate_update(&old).unwrap();
        for previous in [None, Some(false)] {
            old.immutable = previous;
            for next in [None, Some(false), Some(true)] {
                new = old.clone();
                new.data.insert("new".into(), "value".into());
                new.binary_data.insert("blob".into(), vec![1]);
                new.immutable = next;
                new.validate_update(&old).unwrap();
            }
        }
    }

    #[test]
    fn updates_preserve_identity_and_revalidate_input() {
        let old = config_map();
        let mut new = old.clone();
        new.metadata.name = "other".into();
        assert_eq!(
            new.validate_update(&old).unwrap_err().field,
            "metadata.name"
        );
        new = old.clone();
        new.metadata.namespace = "other".into();
        assert_eq!(
            new.validate_update(&old).unwrap_err().field,
            "metadata.namespace"
        );
        new = old.clone();
        new.metadata.uid = uuid::Uuid::new_v4();
        assert_eq!(new.validate_update(&old).unwrap_err().field, "metadata.uid");
        new = old.clone();
        new.data.insert("..bad".into(), String::new());
        assert_eq!(
            new.validate_update(&old).unwrap_err().field,
            "data[\"..bad\"]"
        );
    }

    #[test]
    fn list_roundtrip_keeps_snapshot_version_separate_from_item_versions() {
        let mut cm = config_map();
        cm.metadata.resource_version = Some("item-v1".into());
        let list = ConfigMapList {
            api_version: "v1".into(),
            kind: "ConfigMapList".into(),
            metadata: ListMeta {
                resource_version: Some("snapshot-v2".into()),
            },
            items: vec![cm],
        };
        let json = serde_json::to_value(&list).unwrap();
        assert_eq!(json["metadata"]["resourceVersion"], "snapshot-v2");
        assert_eq!(json["items"][0]["metadata"]["resourceVersion"], "item-v1");
        assert_eq!(serde_json::from_value::<ConfigMapList>(json).unwrap(), list);
        let yaml = serde_yaml::to_string(&list).unwrap();
        assert_eq!(serde_yaml::from_str::<ConfigMapList>(&yaml).unwrap(), list);
    }

    #[test]
    fn debug_and_validation_errors_do_not_expose_values() {
        let mut cm = config_map();
        cm.data.insert("bad/key".into(), "private-text".into());
        cm.binary_data
            .insert("blob".into(), b"private-bytes".to_vec());
        cm.metadata
            .annotations
            .insert("note".into(), "private-annotation".into());
        let output = format!("{cm:?} {}", cm.validate().unwrap_err());
        assert!(output.contains("app-config"));
        assert!(output.contains("data[\"bad/key\"]"));
        for value in ["private-text", "private-bytes", "private-annotation"] {
            assert!(!output.contains(value));
        }
        assert!(!output.contains(&format!("{:?}", cm.binary_data["blob"])));
    }
}
