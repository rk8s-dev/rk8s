//! Functionality corresponding to <https://github.com/opencontainers/image-spec/blob/main/descriptor.md#digests>.

use std::fmt::Display;
use std::str::FromStr;

/// A digest algorithm; at the current time only SHA-256
/// is widely used and supported in the ecosystem. Other
/// SHA variants are included as they are noted in the
/// standards. Other digest algorithms may be added
/// in the future, so this structure is marked as non-exhaustive.
#[non_exhaustive]
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub enum DigestAlgorithm {
    /// The SHA-256 algorithm.
    Sha256,
    /// The SHA-384 algorithm.
    Sha384,
    /// The SHA-512 algorithm.
    Sha512,
    /// Any other algorithm. Note that it is possible
    /// that other algorithms will be added as enum members.
    /// If you want to try to handle those, consider also
    /// comparing against [`Self::as_ref<str>`].
    Other(Box<str>),
}

impl AsRef<str> for DigestAlgorithm {
    fn as_ref(&self) -> &str {
        match self {
            DigestAlgorithm::Sha256 => "sha256",
            DigestAlgorithm::Sha384 => "sha384",
            DigestAlgorithm::Sha512 => "sha512",
            DigestAlgorithm::Other(o) => o,
        }
    }
}

impl Display for DigestAlgorithm {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(self.as_ref())
    }
}

impl DigestAlgorithm {
    /// Return the length of the digest in hexadecimal ASCII characters.
    pub const fn digest_hexlen(&self) -> Option<u32> {
        match self {
            DigestAlgorithm::Sha256 => Some(64),
            DigestAlgorithm::Sha384 => Some(96),
            DigestAlgorithm::Sha512 => Some(128),
            DigestAlgorithm::Other(_) => None,
        }
    }
}

impl From<&str> for DigestAlgorithm {
    fn from(value: &str) -> Self {
        match value {
            "sha256" => Self::Sha256,
            "sha384" => Self::Sha384,
            "sha512" => Self::Sha512,
            o => Self::Other(o.into()),
        }
    }
}

fn char_is_lowercase_ascii_hex(c: char) -> bool {
    matches!(c, '0'..='9' | 'a'..='f')
}

/// algorithm-component ::= [a-z0-9]+
fn char_is_algorithm_component(c: char) -> bool {
    matches!(c, 'a'..='z' | '0'..='9')
}

/// encoded ::= [a-zA-Z0-9=_-]+
fn char_is_encoded(c: char) -> bool {
    char_is_algorithm_component(c) || matches!(c, 'A'..='Z' | '=' | '_' | '-')
}

/// A parsed pair of algorithm:digest as defined
/// by <https://github.com/opencontainers/image-spec/blob/main/descriptor.md#digests>
///
/// ```
/// # fn main() -> Result<(), Box<dyn std::error::Error>> {
/// use std::str::FromStr;
/// use oci_spec::image::{Digest, DigestAlgorithm};
/// let d = Digest::from_str("sha256:6c3c624b58dbbcd3c0dd82b4c53f04194d1247c6eebdaab7c610cf7d66709b3b")?;
/// assert_eq!(d.algorithm(), &DigestAlgorithm::Sha256);
/// assert_eq!(d.digest(), "6c3c624b58dbbcd3c0dd82b4c53f04194d1247c6eebdaab7c610cf7d66709b3b");
/// let d = Digest::from_str("multihash+base58:QmRZxt2b1FVZPNqd8hsiykDL3TdBDeTSPX9Kv46HmX4Gx8")?;
/// assert_eq!(d.algorithm(), &DigestAlgorithm::from("multihash+base58"));
/// assert_eq!(d.digest(), "QmRZxt2b1FVZPNqd8hsiykDL3TdBDeTSPX9Kv46HmX4Gx8");
/// # Ok(())
/// # }
/// ```

#[derive(Clone, Debug, Eq, PartialEq, Hash)]
pub struct Digest {
    /// The algorithm; we need to hold a copy of this
    /// right now as we ended up returning a reference
    /// from the accessor. It probably would have been
    /// better to have both borrowed/owned DigestAlgorithm
    /// versions and our accessor just returns a borrowed version.
    algorithm: DigestAlgorithm,
    value: Box<str>,
    split: usize,
}

impl AsRef<str> for Digest {
    fn as_ref(&self) -> &str {
        &self.value
    }
}

impl Display for Digest {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(self.as_ref())
    }
}

impl<'de> serde::Deserialize<'de> for Digest {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        let s = String::deserialize(deserializer)?;
        Self::from_str(&s).map_err(serde::de::Error::custom)
    }
}

impl serde::ser::Serialize for Digest {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: serde::Serializer,
    {
        let v = self.to_string();
        serializer.serialize_str(&v)
    }
}

impl Digest {
    const ALGORITHM_SEPARATOR: &'static [char] = &['+', '.', '_', '-'];
    /// The algorithm name (e.g. sha256, sha512)
    pub fn algorithm(&self) -> &DigestAlgorithm {
        &self.algorithm
    }

    /// The algorithm digest component. When this is one of the
    /// SHA family (SHA-256, SHA-384, etc.) the digest value
    /// is guaranteed to be a valid length with only lowercase hexadecimal
    /// characters. For example with SHA-256, the length is 64.
    pub fn digest(&self) -> &str {
        &self.value[self.split + 1..]
    }
}

impl FromStr for Digest {
    type Err = crate::OciSpecError;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        Digest::try_from(s)
    }
}

impl TryFrom<String> for Digest {
    type Error = crate::OciSpecError;

    fn try_from(s: String) -> Result<Self, Self::Error> {
        let s = s.into_boxed_str();
        let Some(split) = s.find(':') else {
            return Err(crate::OciSpecError::Other("missing ':' in digest".into()));
        };
        let (algorithm, value) = s.split_at(split);
        let value = &value[1..];

        // algorithm ::= algorithm-component (algorithm-separator algorithm-component)*
        let algorithm_parts = algorithm.split(Self::ALGORITHM_SEPARATOR);
        for part in algorithm_parts {
            if part.is_empty() {
                return Err(crate::OciSpecError::Other(
                    "Empty algorithm component".into(),
                ));
            }
            if !part.chars().all(char_is_algorithm_component) {
                return Err(crate::OciSpecError::Other(format!(
                    "Invalid algorithm component: {part}"
                )));
            }
        }

        if value.is_empty() {
            return Err(crate::OciSpecError::Other("Empty algorithm value".into()));
        }
        if !value.chars().all(char_is_encoded) {
            return Err(crate::OciSpecError::Other(format!(
                "Invalid encoded value {value}"
            )));
        }

        let algorithm = DigestAlgorithm::from(algorithm);
        if let Some(expected) = algorithm.digest_hexlen() {
            let found = value.len();
            if expected as usize != found {
                return Err(crate::OciSpecError::Other(format!(
                    "Invalid digest length {found} expected {expected}"
                )));
            }
            let is_all_hex = value.chars().all(char_is_lowercase_ascii_hex);
            if !is_all_hex {
                return Err(crate::OciSpecError::Other(format!(
                    "Invalid non-hexadecimal character in digest: {value}"
                )));
            }
        }
        Ok(Self {
            algorithm,
            value: s,
            split,
        })
    }
}

impl TryFrom<&str> for Digest {
    type Error = crate::OciSpecError;

    fn try_from(string: &str) -> Result<Self, Self::Error> {
        TryFrom::try_from(string.to_owned())
    }
}

/// A SHA-256 digest, guaranteed to be 64 lowercase hexadecimal ASCII characters.
#[derive(Clone, Debug, Eq, PartialEq, Hash)]
pub struct Sha256Digest {
    digest: Box<str>,
}

impl From<Sha256Digest> for Digest {
    fn from(value: Sha256Digest) -> Self {
        Self {
            algorithm: DigestAlgorithm::Sha256,
            value: format!("sha256:{}", value.digest()).into(),
            split: 6,
        }
    }
}

impl AsRef<str> for Sha256Digest {
    fn as_ref(&self) -> &str {
        self.digest()
    }
}

impl Display for Sha256Digest {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(self.digest())
    }
}

impl FromStr for Sha256Digest {
    type Err = crate::OciSpecError;

    fn from_str(digest: &str) -> Result<Self, Self::Err> {
        let alg = DigestAlgorithm::Sha256;
        let v = format!("{alg}:{digest}");
        let d = Digest::from_str(&v)?;
        match d.algorithm {
            DigestAlgorithm::Sha256 => Ok(Self {
                digest: d.digest().into(),
            }),
            o => Err(crate::OciSpecError::Other(format!(
                "Expected algorithm sha256 but found {o}",
            ))),
        }
    }
}

impl Sha256Digest {
    /// The SHA-256 digest, guaranteed to be 64 lowercase hexadecimal characters.
    pub fn digest(&self) -> &str {
        &self.digest
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_digest_invalid() {
        let invalid = [
            "",
            "foo",
            ":",
            "blah+",
            "_digest:somevalue",
            ":blah",
            "blah:",
            "FooBar:123abc",
            "^:foo",
            "bar^baz:blah",
            "sha256:123456*78",
            "sha256:6c3c624b58dbbcd3c0dd82b4z53f04194d1247c6eebdaab7c610cf7d66709b3b", // has a z in the middle
            "sha384:x",
            "sha384:6c3c624b58dbbcd3c0dd82b4c53f04194d1247c6eebdaab7c610cf7d66709b3b",
            "sha512:6c3c624b58dbbcd3c0dd82b4c53f04194d1247c6eebdaab7c610cf7d66709b3b",
        ];
        for case in invalid {
            assert!(
                Digest::from_str(case).is_err(),
                "Should have failed to parse: {case}"
            )
        }
    }

    const VALID_DIGEST_SHA256: &str =
        "sha256:6c3c624b58dbbcd3c0dd82b4c53f04194d1247c6eebdaab7c610cf7d66709b3b";
    const VALID_DIGEST_SHA384: &str =
        "sha384:6c3c624b58dbbcd4d1247c6eebdaab7c610cf7d66709b3b3c0dd82b4c53f04194d1247c6eebdaab7c610cf7d66709b3b";
    const VALID_DIGEST_SHA512: &str =
        "sha512:6c3c624b58dbbcd3c0dd826c3c624b58dbbcd3c0dd82b4c53f04194d1247c6eebdaab7c610cf7d66709b3bb4c53f04194d1247c6eebdaab7c610cf7d66709b3b";

    #[test]
    fn test_digest_valid() {
        let cases = ["foo:bar", "xxhash:42"];
        for case in cases {
            Digest::from_str(case).unwrap();
        }

        let d = Digest::try_from("multihash+base58:QmRZxt2b1FVZPNqd8hsiykDL3TdBDeTSPX9Kv46HmX4Gx8")
            .unwrap();
        assert_eq!(d.algorithm(), &DigestAlgorithm::from("multihash+base58"));
        assert_eq!(d.digest(), "QmRZxt2b1FVZPNqd8hsiykDL3TdBDeTSPX9Kv46HmX4Gx8");
    }

    #[test]
    fn test_sha256_valid() {
        let expected_value = VALID_DIGEST_SHA256.split_once(':').unwrap().1;
        let d = Digest::from_str(VALID_DIGEST_SHA256).unwrap();
        assert_eq!(d.algorithm(), &DigestAlgorithm::Sha256);
        assert_eq!(d.digest(), expected_value);
        let base_digest = Digest::from(d.clone());
        assert_eq!(base_digest.digest(), expected_value);
    }

    #[test]
    fn test_sha384_valid() {
        let expected_value = VALID_DIGEST_SHA384.split_once(':').unwrap().1;
        let d = Digest::from_str(VALID_DIGEST_SHA384).unwrap();
        assert_eq!(d.algorithm(), &DigestAlgorithm::Sha384);
        assert_eq!(d.digest(), expected_value);
        // Verify we can cheaply coerce to a string
        assert_eq!(d.as_ref(), VALID_DIGEST_SHA384);
        let base_digest = Digest::from(d.clone());
        assert_eq!(base_digest.digest(), expected_value);
    }

    #[test]
    fn test_sha512_valid() {
        let expected_value = VALID_DIGEST_SHA512.split_once(':').unwrap().1;
        let d = Digest::from_str(VALID_DIGEST_SHA512).unwrap();
        assert_eq!(d.algorithm(), &DigestAlgorithm::Sha512);
        assert_eq!(d.digest(), expected_value);
        let base_digest = Digest::from(d.clone());
        assert_eq!(base_digest.digest(), expected_value);
    }

    #[test]
    fn test_sha256() {
        let digest = VALID_DIGEST_SHA256.split_once(':').unwrap().1;
        let v = Sha256Digest::from_str(digest).unwrap();
        assert_eq!(v.digest(), digest);
    }
}
