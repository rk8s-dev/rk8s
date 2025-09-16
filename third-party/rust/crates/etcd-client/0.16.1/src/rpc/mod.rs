//! Etcd RPC interfaces.

#[cfg(feature = "pub-response-field")]
pub(crate) mod pb;

#[cfg(not(feature = "pub-response-field"))]
mod pb;

pub mod auth;
pub mod cluster;
pub mod election;
pub mod kv;
pub mod lease;
pub mod lock;
pub mod maintenance;
pub mod watch;

use crate::error::Result;
use pb::etcdserverpb::ResponseHeader as PbResponseHeader;
use pb::mvccpb::KeyValue as PbKeyValue;

/// General `etcd` response header.
#[cfg_attr(feature = "pub-response-field", visible::StructFields(pub))]
#[derive(Debug, Clone)]
#[repr(transparent)]
pub struct ResponseHeader(PbResponseHeader);

impl ResponseHeader {
    /// Create a response header from pb header.
    #[inline]
    pub(crate) const fn new(header: PbResponseHeader) -> Self {
        Self(header)
    }

    /// The ID of the cluster which sent the response.
    #[inline]
    pub const fn cluster_id(&self) -> u64 {
        self.0.cluster_id
    }

    /// The ID of the member which sent the response.
    #[inline]
    pub const fn member_id(&self) -> u64 {
        self.0.member_id
    }

    /// The key-value store revision when the request was applied.
    /// For watch progress responses, the header.revision() indicates progress. All future events
    /// received in this stream are guaranteed to have a higher revision number than the
    /// header.revision() number.
    #[inline]
    pub const fn revision(&self) -> i64 {
        self.0.revision
    }

    /// The raft term when the request was applied.
    #[inline]
    pub const fn raft_term(&self) -> u64 {
        self.0.raft_term
    }
}

impl From<&PbResponseHeader> for &ResponseHeader {
    #[inline]
    fn from(src: &PbResponseHeader) -> Self {
        unsafe { &*(src as *const _ as *const ResponseHeader) }
    }
}

/// Key-value pair.
#[cfg_attr(feature = "pub-response-field", visible::StructFields(pub))]
#[derive(Debug, Clone)]
#[repr(transparent)]
pub struct KeyValue(PbKeyValue);

impl KeyValue {
    /// Create a KeyValue from pb kv.
    #[inline]
    pub(crate) const fn new(kv: PbKeyValue) -> Self {
        Self(kv)
    }

    /// The key in bytes. An empty key is not allowed.
    #[inline]
    pub fn key(&self) -> &[u8] {
        &self.0.key
    }

    /// The key in string. An empty key is not allowed.
    #[inline]
    pub fn key_str(&self) -> Result<&str> {
        std::str::from_utf8(self.key()).map_err(From::from)
    }

    /// The key in string. An empty key is not allowed.
    ///
    /// # Safety
    /// This function is unsafe because it does not check that the bytes of the key are valid UTF-8.
    /// If this constraint is violated, undefined behavior results,
    /// as the rest of Rust assumes that [`&str`]s are valid UTF-8.
    #[inline]
    pub unsafe fn key_str_unchecked(&self) -> &str {
        std::str::from_utf8_unchecked(self.key())
    }

    /// The value held by the key, in bytes.
    #[inline]
    pub fn value(&self) -> &[u8] {
        &self.0.value
    }

    /// The value held by the key, in string.
    #[inline]
    pub fn value_str(&self) -> Result<&str> {
        std::str::from_utf8(self.value()).map_err(From::from)
    }

    /// The value held by the key, in bytes.
    ///
    /// # Safety
    /// This function is unsafe because it does not check that the bytes of the value are valid UTF-8.
    /// If this constraint is violated, undefined behavior results,
    /// as the rest of Rust assumes that [`&str`]s are valid UTF-8.
    #[inline]
    pub unsafe fn value_str_unchecked(&self) -> &str {
        std::str::from_utf8_unchecked(self.value())
    }

    /// Convert to key-value pair.
    pub fn into_key_value(self) -> (Vec<u8>, Vec<u8>) {
        (self.0.key, self.0.value)
    }

    /// The revision of last creation on this key.
    #[inline]
    pub const fn create_revision(&self) -> i64 {
        self.0.create_revision
    }

    /// The revision of last modification on this key.
    #[inline]
    pub const fn mod_revision(&self) -> i64 {
        self.0.mod_revision
    }

    /// The version of the key. A deletion resets
    /// the version to zero and any modification of the key
    /// increases its version.
    #[inline]
    pub const fn version(&self) -> i64 {
        self.0.version
    }

    /// The ID of the lease that attached to key.
    /// When the attached lease expires, the key will be deleted.
    /// If lease is 0, then no lease is attached to the key.
    #[inline]
    pub const fn lease(&self) -> i64 {
        self.0.lease
    }
}

impl From<&PbKeyValue> for &KeyValue {
    #[inline]
    fn from(src: &PbKeyValue) -> Self {
        unsafe { &*(src as *const _ as *const KeyValue) }
    }
}

impl From<&mut PbKeyValue> for &mut KeyValue {
    #[inline]
    fn from(src: &mut PbKeyValue) -> Self {
        unsafe { &mut *(src as *mut _ as *mut KeyValue) }
    }
}

/// Get prefix end key of `key`.
#[inline]
fn get_prefix(key: &[u8]) -> Vec<u8> {
    for (i, v) in key.iter().enumerate().rev() {
        if *v < 0xFF {
            let mut end = Vec::from(&key[..=i]);
            end[i] = *v + 1;
            return end;
        }
    }

    // next prefix does not exist (e.g., 0xffff);
    vec![0]
}

/// Key range builder.
#[derive(Debug, Default, Clone)]
struct KeyRange {
    key: Vec<u8>,
    range_end: Vec<u8>,
    with_prefix: bool,
    with_from_key: bool,
    with_all_keys: bool,
}

impl KeyRange {
    #[inline]
    pub const fn new() -> Self {
        KeyRange {
            key: Vec::new(),
            range_end: Vec::new(),
            with_prefix: false,
            with_from_key: false,
            with_all_keys: false,
        }
    }

    /// Sets key.
    #[inline]
    pub fn with_key(&mut self, key: impl Into<Vec<u8>>) {
        self.key = key.into();
    }

    /// Specifies the range end.
    /// `end_key` must be lexicographically greater than start key.
    #[inline]
    pub fn with_range(&mut self, end_key: impl Into<Vec<u8>>) {
        self.range_end = end_key.into();
        self.with_prefix = false;
        self.with_from_key = false;
        self.with_all_keys = false;
    }

    /// Sets all keys >= key.
    #[inline]
    pub fn with_from_key(&mut self) {
        self.with_from_key = true;
        self.with_prefix = false;
        self.with_all_keys = false;
    }

    /// Sets all keys prefixed with key.
    #[inline]
    pub fn with_prefix(&mut self) {
        self.with_prefix = true;
        self.with_from_key = false;
        self.with_all_keys = false;
    }

    /// Sets all keys.
    #[inline]
    pub fn with_all_keys(&mut self) {
        self.with_all_keys = true;
        self.with_prefix = false;
        self.with_from_key = false;
    }

    /// Build the key and range end.
    #[inline]
    pub fn build(mut self) -> (Vec<u8>, Vec<u8>) {
        if self.with_all_keys {
            self.key = vec![b'\0'];
            self.range_end = vec![b'\0'];
        } else if self.with_from_key {
            if self.key.is_empty() {
                self.key = vec![b'\0'];
            }
            self.range_end = vec![b'\0'];
        } else if self.with_prefix {
            if self.key.is_empty() {
                self.key = vec![b'\0'];
                self.range_end = vec![b'\0'];
            } else {
                self.range_end = get_prefix(&self.key);
            }
        }

        (self.key, self.range_end)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_get_prefix() {
        assert_eq!(get_prefix(b"foo1").as_slice(), b"foo2");
        assert_eq!(get_prefix(b"\xFF").as_slice(), b"\0");
        assert_eq!(get_prefix(b"foo\xFF").as_slice(), b"fop");
    }
}
