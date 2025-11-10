//! Miscellaneous public handy functions are collected here, such as cryptography tools,
//! uuid generator, etc.

use std::time::{Duration, SystemTime};

use blake3;
use chrono::prelude::*;
use humantime::{format_rfc3339, parse_duration, parse_rfc3339};
use openssl::hash::{Hasher, MessageDigest};
use rand::{Rng, thread_rng};
use serde::{Deserialize, Deserializer, Serialize, Serializer};
use std::collections::HashSet;

use crate::errors::RvError;

pub mod cert;
pub mod cidr;
pub mod crypto;
pub mod ip_sock_addr;
pub mod key;
pub mod kv_builder;
pub mod locks;
pub mod ocsp;
pub mod policy;
pub mod salt;
pub mod seal;
pub mod sock_addr;
pub mod string;
pub mod token_util;
pub mod unix_sock_addr;

/// A hash set that stores Blake3 hashes of arbitrary byte data.
///
/// BHashSet (Blake Hash Set) provides a space-efficient way to track whether
/// specific byte sequences have been "used" or seen before. Instead of storing
/// the actual data, it stores 32-byte Blake3 hashes, providing excellent
/// collision resistance while using constant space per item.
///
/// # Use Cases
/// - Tracking used unseal keys to prevent replay attacks
/// - Deduplication of data based on content
/// - Efficient membership testing for large byte sequences
/// - Preventing reuse of tokens, nonces, or other security-sensitive data
///
/// # Security Features
/// - Uses Blake3 cryptographic hash function for collision resistance
/// - Provides deterministic membership testing
/// - Space-efficient storage (32 bytes per unique item regardless of original size)
/// - Serializable for persistence across restarts
///
/// # Performance
/// - O(1) average case for membership testing and insertion
/// - Memory usage scales with number of unique items, not their size
/// - Blake3 hashing is extremely fast
///
/// # Example
/// ```
/// use libvault::utils::BHashSet;
///
/// let mut set = BHashSet::default();
///
/// // Insert some keys
/// set.insert(b"secret_key_1");
/// set.insert(b"secret_key_2");
///
/// // Check membership
/// assert!(set.contains(b"secret_key_1"));
/// assert!(!set.contains(b"unknown_key"));
///
/// // The set only stores hashes, not the original data
/// assert_eq!(set.len(), 2);
///
/// // Each stored hash is exactly 32 bytes regardless of input size
/// for hash in set.iter() {
///     assert_eq!(hash.len(), 32);
/// }
/// ```
#[derive(Serialize, Deserialize, Default, Debug, Clone)]
pub struct BHashSet(pub HashSet<[u8; 32]>);

impl BHashSet {
    /// Checks if the set contains a specific key.
    ///
    /// This method computes the Blake3 hash of the provided key and checks
    /// if that hash exists in the internal hash set. This provides efficient
    /// membership testing without storing the original key data.
    ///
    /// # Arguments
    /// - `key`: The byte slice to check for membership
    ///
    /// # Returns
    /// `true` if the key's hash exists in the set, `false` otherwise
    ///
    /// # Performance
    /// - Time complexity: O(1) average case
    /// - Space complexity: O(1) - no additional memory allocated
    ///
    /// # Example
    /// ```
    /// use libvault::utils::BHashSet;
    ///
    /// let mut set = BHashSet::default();
    /// set.insert(b"example_key");
    /// assert!(set.contains(b"example_key"));
    /// assert!(!set.contains(b"nonexistent_key"));
    /// ```
    pub fn contains(&self, key: &[u8]) -> bool {
        let hash: [u8; 32] = blake3::hash(key).into();
        self.0.contains(&hash)
    }

    /// Inserts a key into the set.
    ///
    /// This method computes the Blake3 hash of the provided key and stores
    /// the hash in the internal hash set. If the key was already present,
    /// this operation has no effect.
    ///
    /// # Arguments
    /// - `key`: The byte slice to insert into the set
    ///
    /// # Performance
    /// - Time complexity: O(1) average case
    /// - Space complexity: O(1) per unique key (32 bytes for the hash)
    ///
    /// # Security
    /// - Only stores cryptographic hash, not the original key data
    /// - Provides collision resistance through Blake3 hashing
    ///
    /// # Example
    /// ```
    /// use libvault::utils::BHashSet;
    ///
    /// let mut set = BHashSet::default();
    /// set.insert(b"secure_token");
    /// assert!(set.contains(b"secure_token"));
    /// ```
    pub fn insert(&mut self, key: &[u8]) {
        let hash: [u8; 32] = blake3::hash(key).into();
        self.0.insert(hash);
    }

    /// Removes a key from the set.
    ///
    /// This method computes the Blake3 hash of the provided key and removes
    /// that hash from the internal hash set. If the key was not present,
    /// this operation has no effect.
    ///
    /// # Arguments
    /// - `key`: The byte slice to remove from the set
    ///
    /// # Returns
    /// `true` if the key was present and removed, `false` if it wasn't present
    ///
    /// # Performance
    /// - Time complexity: O(1) average case
    /// - Space complexity: O(1) - may free 32 bytes if key was present
    ///
    /// # Example
    /// ```
    /// use libvault::utils::BHashSet;
    ///
    /// let mut set = BHashSet::default();
    /// set.insert(b"temporary_key");
    /// assert!(set.contains(b"temporary_key"));
    /// set.remove(b"temporary_key");
    /// assert!(!set.contains(b"temporary_key"));
    /// ```
    pub fn remove(&mut self, key: &[u8]) -> bool {
        let hash: [u8; 32] = blake3::hash(key).into();
        self.0.remove(&hash)
    }

    /// Clears all entries from the set.
    ///
    /// This method removes all stored hashes from the internal hash set,
    /// effectively resetting the set to an empty state. All previously
    /// inserted keys will no longer be considered as contained in the set.
    ///
    /// # Performance
    /// - Time complexity: O(n) where n is the number of stored hashes
    /// - Space complexity: Frees all allocated memory for stored hashes
    ///
    /// # Example
    /// ```
    /// use libvault::utils::BHashSet;
    ///
    /// let mut set = BHashSet::default();
    /// set.insert(b"key1");
    /// set.insert(b"key2");
    /// assert_eq!(set.len(), 2);
    ///
    /// set.clear();
    /// assert_eq!(set.len(), 0);
    /// assert!(set.is_empty());
    /// ```
    pub fn clear(&mut self) {
        self.0.clear();
    }

    /// Returns the number of unique keys stored in the set.
    ///
    /// This method returns the count of unique Blake3 hashes stored
    /// in the internal hash set, which corresponds to the number of
    /// unique keys that have been inserted.
    ///
    /// # Returns
    /// The number of unique keys in the set
    ///
    /// # Performance
    /// - Time complexity: O(1)
    /// - Space complexity: O(1)
    ///
    /// # Example
    /// ```
    /// use libvault::utils::BHashSet;
    ///
    /// let mut set = BHashSet::default();
    /// assert_eq!(set.len(), 0);
    ///
    /// set.insert(b"key1");
    /// set.insert(b"key2");
    /// set.insert(b"key1"); // Duplicate, won't increase count
    /// assert_eq!(set.len(), 2);
    /// ```
    pub fn len(&self) -> usize {
        self.0.len()
    }

    /// Checks if the set is empty.
    ///
    /// This method returns `true` if the set contains no keys,
    /// `false` otherwise. It's equivalent to checking if `len() == 0`
    /// but may be more efficient.
    ///
    /// # Returns
    /// `true` if the set is empty, `false` otherwise
    ///
    /// # Performance
    /// - Time complexity: O(1)
    /// - Space complexity: O(1)
    ///
    /// # Example
    /// ```
    /// use libvault::utils::BHashSet;
    ///
    /// let mut set = BHashSet::default();
    /// assert!(set.is_empty());
    ///
    /// set.insert(b"key");
    /// assert!(!set.is_empty());
    ///
    /// set.clear();
    /// assert!(set.is_empty());
    /// ```
    pub fn is_empty(&self) -> bool {
        self.0.is_empty()
    }

    /// Returns an iterator over the hashes stored in the set.
    ///
    /// This method provides an iterator that yields references to the
    /// Blake3 hashes stored in the set. Note that these are the computed
    /// hashes, not the original key data, as the original keys are not
    /// stored for security and space efficiency reasons.
    ///
    /// # Returns
    /// An iterator that yields `&[u8]` references to the 32-byte Blake3 hashes
    ///
    /// # Performance
    /// - Iterator creation: O(1)
    /// - Iteration: O(n) where n is the number of stored hashes
    ///
    /// # Security Note
    /// The returned hashes are cryptographically secure representations
    /// of the original keys, but the original key data cannot be recovered
    /// from these hashes.
    ///
    /// # Example
    /// ```
    /// use libvault::utils::BHashSet;
    ///
    /// let mut set = BHashSet::default();
    /// set.insert(b"key1");
    /// set.insert(b"key2");
    ///
    /// let hash_count = set.iter().count();
    /// assert_eq!(hash_count, 2);
    ///
    /// // Each hash is 32 bytes long (Blake3 output size)
    /// for hash in set.iter() {
    ///     assert_eq!(hash.len(), 32);
    /// }
    /// ```
    pub fn iter(&self) -> impl Iterator<Item = &[u8]> {
        self.0.iter().map(|hash| hash.as_slice())
    }
}

pub fn generate_uuid() -> String {
    let mut buf = [0u8; 16];
    thread_rng().fill(&mut buf);

    format!(
        "{:02x}{:02x}{:02x}{:02x}-{:02x}{:02x}-{:02x}{:02x}-{:02x}{:02x}-{:02x}{:02x}{:02x}{:02x}{:02x}{:02x}",
        buf[0],
        buf[1],
        buf[2],
        buf[3],
        buf[4],
        buf[5],
        buf[6],
        buf[7],
        buf[8],
        buf[9],
        buf[10],
        buf[11],
        buf[12],
        buf[13],
        buf[14],
        buf[15]
    )
}

pub fn is_str_subset<T: PartialEq>(sub: &Vec<T>, superset: &Vec<T>) -> bool {
    sub.iter().all(|item| superset.contains(item))
}

pub fn sha1(data: &[u8]) -> String {
    let mut hasher = Hasher::new(MessageDigest::sha1()).unwrap();
    hasher.update(data).unwrap();
    let result = hasher.finish().unwrap();
    hex::encode(result)
}

pub fn sha256(data: &[u8]) -> String {
    let mut hasher = Hasher::new(MessageDigest::sha256()).unwrap();
    hasher.update(data).unwrap();
    let result = hasher.finish().unwrap();
    hex::encode(result)
}

pub fn serialize_system_time<S>(time: &SystemTime, serializer: S) -> Result<S::Ok, S::Error>
where
    S: Serializer,
{
    let formatted = format_rfc3339(*time).to_string();
    serializer.serialize_str(&formatted)
}

pub fn deserialize_system_time<'de, D>(deserializer: D) -> Result<SystemTime, D::Error>
where
    D: Deserializer<'de>,
{
    let input: &str = Deserialize::deserialize(deserializer)?;
    let parsed_time = parse_rfc3339(input).map_err(serde::de::Error::custom)?;
    let system_time: SystemTime = parsed_time;
    Ok(system_time)
}

pub fn serialize_duration<S>(duration: &Duration, serializer: S) -> Result<S::Ok, S::Error>
where
    S: serde::Serializer,
{
    let timestamp = duration.as_secs();
    serializer.serialize_i64(timestamp as i64)
}

pub fn deserialize_duration<'de, D>(deserializer: D) -> Result<Duration, D::Error>
where
    D: serde::Deserializer<'de>,
{
    struct DurationVisitor;

    impl serde::de::Visitor<'_> for DurationVisitor {
        type Value = Duration;

        fn expecting(&self, formatter: &mut std::fmt::Formatter) -> std::fmt::Result {
            formatter.write_str("a number or a string with 's' suffix")
        }

        fn visit_u64<E>(self, value: u64) -> Result<Self::Value, E>
        where
            E: serde::de::Error,
        {
            Ok(Duration::from_secs(value))
        }

        fn visit_str<E>(self, value: &str) -> Result<Self::Value, E>
        where
            E: serde::de::Error,
        {
            parse_duration(value).map_err(serde::de::Error::custom)
        }
    }

    deserializer.deserialize_any(DurationVisitor)
}

pub fn asn1time_to_timestamp(time_str: &str) -> Result<i64, RvError> {
    // Parse the time string
    let dt = NaiveDateTime::parse_from_str(time_str, "%b %e %H:%M:%S %Y %Z")?;

    // Convert to a DateTime object with UTC timezone
    //let dt_utc = DateTime::<Utc>::from_utc(dt, Utc);
    let dt_utc = Utc.from_utc_datetime(&dt);

    // Get the timestamp
    let timestamp = dt_utc.timestamp();

    Ok(timestamp)
}

pub fn hex_encode_with_colon(bytes: &[u8]) -> String {
    let hex_str = hex::encode(bytes);
    let split_hex: Vec<String> = hex_str
        .as_bytes()
        .chunks(2)
        .map(|chunk| String::from_utf8(chunk.to_vec()).unwrap())
        .collect();

    split_hex.join(":")
}

pub fn is_protect_path(protected: &[&str], paths: &[&str]) -> bool {
    for p in protected.iter() {
        for path in paths.iter() {
            if path.starts_with(p) {
                return true;
            }
        }
    }

    false
}

pub fn default_system_time() -> SystemTime {
    SystemTime::UNIX_EPOCH
}
