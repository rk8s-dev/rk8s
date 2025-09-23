//! Errors and functions for validating digests

use http::HeaderMap;
use sha2::Digest as _;

use crate::sha256_digest;

pub const DOCKER_DIGEST_HEADER: &str = "Docker-Content-Digest";

pub type Result<T> = std::result::Result<T, DigestError>;

/// Errors that can occur when validating digests
#[derive(Debug, thiserror::Error)]
pub enum DigestError {
    /// Invalid digest header
    #[error("Invalid digest header: {0}")]
    InvalidHeader(#[from] http::header::ToStrError),
    /// Invalid digest algorithm found
    #[error("Unsupported digest algorithm: {0}")]
    UnsupportedAlgorithm(String),
    /// Missing digest algorithm
    #[error("Missing digest algorithm")]
    MissingAlgorithm,
    /// Digest verification failed
    #[error("Invalid digest. Expected {expected}, got {actual}")]
    VerificationError {
        /// Expected digest
        expected: String,
        /// Actual digest
        actual: String,
    },
}

/// A convenience struct for parsing a digest value with an algorithm
#[derive(Debug, PartialEq, Eq, PartialOrd, Ord)]
pub struct Digest<'a> {
    pub algorithm: &'a str,
    pub digest: &'a str,
}

impl<'a> Digest<'a> {
    /// Create a new digest from a str. This isn't using `FromStr` because we can't use lifetimes
    /// properly when implementing the trait
    pub fn new(digest: &'a str) -> Result<Self> {
        let (algorithm, digest) = digest
            .split_once(':')
            .ok_or(DigestError::MissingAlgorithm)?;
        Ok(Self { algorithm, digest })
    }
}

/// Helper wrapper around various digest algorithms to make it easier to use them with our blob
/// utils. This has to be an enum because the digest algorithms aren't object safe so we can't box
/// dynner them
pub(crate) enum Digester {
    Sha256(sha2::Sha256),
    Sha384(sha2::Sha384),
    Sha512(sha2::Sha512),
}

impl Digester {
    pub fn new(digest: &str) -> Result<Self> {
        let parsed_digest = Digest::new(digest)?;

        match parsed_digest.algorithm {
            "sha256" => Ok(Digester::Sha256(sha2::Sha256::new())),
            "sha384" => Ok(Digester::Sha384(sha2::Sha384::new())),
            "sha512" => Ok(Digester::Sha512(sha2::Sha512::new())),
            // We already check this above when parsing, but just in case, we return the error as
            // well here
            _ => Err(DigestError::UnsupportedAlgorithm(
                parsed_digest.algorithm.to_string(),
            )),
        }
    }

    pub fn update(&mut self, data: impl AsRef<[u8]>) {
        match self {
            Self::Sha256(d) => d.update(data),
            Self::Sha384(d) => d.update(data),
            Self::Sha512(d) => d.update(data),
        }
    }

    pub fn finalize(&mut self) -> String {
        match self {
            Self::Sha256(d) => format!("sha256:{:x}", d.finalize_reset()),
            Self::Sha384(d) => format!("sha384:{:x}", d.finalize_reset()),
            Self::Sha512(d) => format!("sha512:{:x}", d.finalize_reset()),
        }
    }
}

/// Helper for extracting `Docker-Content-Digest` header from manifest GET or HEAD request.
pub fn digest_header_value(headers: HeaderMap) -> Result<Option<String>> {
    headers
        .get(DOCKER_DIGEST_HEADER)
        .map(|hv| hv.to_str().map(|s| s.to_string()))
        .transpose()
        .map_err(DigestError::from)
}

/// Given the optional digest header value and digest of the reference, returns the digest of the
/// content, validating that the digest of the content matches the proper digest. If neither a
/// header digest or a reference digest is provided, then the body is digested and returned as the
/// digest. If both digests are provided, but they use different algorithms, then the header digest
/// is returned after validation as according to the spec it is the "canonical" digest for the given
/// content.
pub fn validate_digest(
    body: &[u8],
    digest_header: Option<String>,
    reference_digest: Option<&str>,
) -> Result<String> {
    match (digest_header, reference_digest) {
        // If both digests are equal, then just calculate once
        (Some(digest), Some(reference)) if digest == reference => {
            calculate_and_validate(body, &digest)
        }
        (Some(digest), Some(reference)) => {
            calculate_and_validate(body, reference)?;
            calculate_and_validate(body, &digest)
        }
        (Some(digest), None) => calculate_and_validate(body, &digest),
        (None, Some(reference)) => calculate_and_validate(body, reference),
        // If we have neither, just digest the body
        (None, None) => Ok(sha256_digest(body)),
    }
}

/// Helper for calculating and validating the digest of the given content
fn calculate_and_validate(content: &[u8], digest: &str) -> Result<String> {
    let parsed_digest = Digest::new(digest)?;
    let digest_calculated = match parsed_digest.algorithm {
        "sha256" => format!("{:x}", sha2::Sha256::digest(content)),
        "sha384" => format!("{:x}", sha2::Sha384::digest(content)),
        "sha512" => format!("{:x}", sha2::Sha512::digest(content)),
        other => return Err(DigestError::UnsupportedAlgorithm(other.to_string())),
    };
    let hex = format!("{}:{digest_calculated}", parsed_digest.algorithm);
    tracing::debug!(%hex, "Computed digest of payload");
    if hex != digest {
        return Err(DigestError::VerificationError {
            expected: digest.to_owned(),
            actual: hex,
        });
    }
    Ok(hex)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_validate_digest() {
        let body = b"hello world";
        let digest_sha256 = format!("sha256:{:x}", sha2::Sha256::digest(body));
        let digest_sha384 = format!("sha384:{:x}", sha2::Sha384::digest(body));

        // Test case 1: Both digests are equal
        assert_eq!(
            validate_digest(body, Some(digest_sha256.clone()), Some(&digest_sha256))
                .expect("Failed to validate digest with matching header and reference"),
            digest_sha256
        );

        // Test case 2: Different digests
        assert_eq!(
            validate_digest(body, Some(digest_sha256.clone()), Some(&digest_sha384))
                .expect("Failed to validate digest with different header and reference"),
            digest_sha256
        );

        // Test case 3: Only digest_header
        assert_eq!(
            validate_digest(body, Some(digest_sha256.clone()), None)
                .expect("Failed to validate digest with only header"),
            digest_sha256
        );

        // Test case 4: Only reference_digest
        assert_eq!(
            validate_digest(body, None, Some(&digest_sha384))
                .expect("Failed to validate digest with only reference"),
            digest_sha384
        );

        // Test case 5: No digests provided
        assert_eq!(
            validate_digest(body, None, None)
                .expect("Failed to validate digest with no digests provided"),
            digest_sha256
        );

        // Test case 6: Invalid digest
        let invalid_digest = "sha256:invalid";
        validate_digest(body, Some(invalid_digest.to_string()), None)
            .expect_err("Expected error for invalid digest");

        // Test case 7: Valid header digest and invalid layer digest
        let invalid_layer_digest = "sha512:invalid";
        validate_digest(
            body,
            Some(digest_sha256.clone()),
            Some(invalid_layer_digest),
        )
        .expect_err("Expected error for invalid layer digest");

        // Test case 8: Unsupported algorithm
        let unsupported_digest = "md5:d41d8cd98f00b204e9800998ecf8427e";
        validate_digest(body, Some(unsupported_digest.to_string()), None)
            .expect_err("Expected error for unsupported algorithm");
    }
}
