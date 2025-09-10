use std::fmt;
use std::str::FromStr;
use std::{convert::TryFrom, sync::OnceLock};

use regex::{Regex, RegexBuilder};
use serde::{Deserialize, Serialize};
use thiserror::Error;

/// NAME_TOTAL_LENGTH_MAX is the maximum total number of characters in a repository name.
const NAME_TOTAL_LENGTH_MAX: usize = 255;

const DOCKER_HUB_DOMAIN_LEGACY: &str = "index.docker.io";
const DOCKER_HUB_DOMAIN: &str = "docker.io";
const DOCKER_HUB_OFFICIAL_REPO_NAME: &str = "library";
const DEFAULT_TAG: &str = "latest";
/// REFERENCE_REGEXP is the full supported format of a reference. The regexp
/// is anchored and has capturing groups for name, tag, and digest components.
const REFERENCE_REGEXP: &str = r"^((?:(?:[a-zA-Z0-9]|[a-zA-Z0-9][a-zA-Z0-9-]*[a-zA-Z0-9])(?:(?:\.(?:[a-zA-Z0-9]|[a-zA-Z0-9][a-zA-Z0-9-]*[a-zA-Z0-9]))+)?(?::[0-9]+)?/)?[a-z0-9]+(?:(?:(?:[._]|__|[-]*)[a-z0-9]+)+)?(?:(?:/[a-z0-9]+(?:(?:(?:[._]|__|[-]*)[a-z0-9]+)+)?)+)?)(?::([\w][\w.-]{0,127}))?(?:@([A-Za-z][A-Za-z0-9]*(?:[-_+.][A-Za-z][A-Za-z0-9]*)*[:][[:xdigit:]]{32,}))?$";

fn reference_regexp() -> &'static Regex {
    static RE: OnceLock<Regex> = OnceLock::new();
    RE.get_or_init(|| {
        RegexBuilder::new(REFERENCE_REGEXP)
            .size_limit(10 * (1 << 21))
            .build()
            .unwrap()
    })
}

/// Reasons that parsing a string as a Reference can fail.
#[derive(Debug, Error, PartialEq, Eq)]
pub enum ParseError {
    /// Will be returned if digest is ill-formed
    #[error("invalid checksum digest format")]
    DigestInvalidFormat,
    /// Will be returned if digest does not have a correct length
    #[error("invalid checksum digest length")]
    DigestInvalidLength,
    /// Will be returned for an unknown digest algorithm
    #[error("unsupported digest algorithm")]
    DigestUnsupported,
    /// Will be returned for an uppercase character in repository name
    #[error("repository name must be lowercase")]
    NameContainsUppercase,
    /// Will be returned if a name is empty
    #[error("repository name must have at least one component")]
    NameEmpty,
    /// Will be returned if a name is too long
    #[error("repository name must not be more than {NAME_TOTAL_LENGTH_MAX} characters")]
    NameTooLong,
    /// Will be returned if a reference is ill-formed
    #[error("invalid reference format")]
    ReferenceInvalidFormat,
    /// Will be returned if a tag is ill-formed
    #[error("invalid tag format")]
    TagInvalidFormat,
}

/// Reference provides a general type to represent any way of referencing images within an OCI registry.
///
/// # Examples
///
/// Parsing a tagged image reference:
///
/// ```
/// use oci_spec::distribution::Reference;
///
/// let reference: Reference = "docker.io/library/hello-world:latest".parse().unwrap();
///
/// assert_eq!("docker.io/library/hello-world:latest", reference.whole().as_str());
/// assert_eq!("docker.io", reference.registry());
/// assert_eq!("library/hello-world", reference.repository());
/// assert_eq!(Some("latest"), reference.tag());
/// assert_eq!(None, reference.digest());
/// ```
#[derive(Clone, Hash, PartialEq, Eq, Debug, Serialize, Deserialize)]
pub struct Reference {
    registry: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    mirror_registry: Option<String>,
    repository: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    tag: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    digest: Option<String>,
}

impl Reference {
    /// Create a Reference with a registry, repository and tag.
    pub fn with_tag(registry: String, repository: String, tag: String) -> Self {
        Self {
            registry,
            mirror_registry: None,
            repository,
            tag: Some(tag),
            digest: None,
        }
    }

    /// Create a Reference with a registry, repository and digest.
    pub fn with_digest(registry: String, repository: String, digest: String) -> Self {
        Self {
            registry,
            mirror_registry: None,
            repository,
            tag: None,
            digest: Some(digest),
        }
    }

    /// Clone the Reference for the same image with a new digest.
    pub fn clone_with_digest(&self, digest: String) -> Self {
        Self {
            registry: self.registry.clone(),
            mirror_registry: self.mirror_registry.clone(),
            repository: self.repository.clone(),
            tag: None,
            digest: Some(digest),
        }
    }

    /// Set a pull mirror registry for this reference.
    ///
    /// The mirror registry will be used to resolve the image, the original registry
    /// is available via the [`Reference::namespace`] function.
    ///
    /// The original registry will be sent with the `ns` query parameter to the mirror registry.
    /// The `ns` query parameter is currently not part of the stable OCI Distribution Spec yet,
    /// but is being discussed to be added and is already used by some other implementations
    /// (for example containerd). So be aware that this feature might not work with all registries.
    ///
    /// Since this is not part of the stable OCI Distribution Spec yet, this feature is exempt from
    /// semver backwards compatibility guarantees and might change in the future.
    #[doc(hidden)]
    pub fn set_mirror_registry(&mut self, registry: String) {
        self.mirror_registry = Some(registry);
    }

    /// Resolve the registry address of a given `Reference`.
    ///
    /// Some registries, such as docker.io, uses a different address for the actual
    /// registry. This function implements such redirection.
    ///
    /// If a mirror registry is set, it will be used instead of the original registry.
    pub fn resolve_registry(&self) -> &str {
        match (self.registry(), self.mirror_registry.as_deref()) {
            (_, Some(mirror_registry)) => mirror_registry,
            ("docker.io", None) => "index.docker.io",
            (registry, None) => registry,
        }
    }

    /// Returns the name of the registry.
    pub fn registry(&self) -> &str {
        &self.registry
    }

    /// Returns the name of the repository.
    pub fn repository(&self) -> &str {
        &self.repository
    }

    /// Returns the object's tag, if present.
    pub fn tag(&self) -> Option<&str> {
        self.tag.as_deref()
    }

    /// Returns the object's digest, if present.
    pub fn digest(&self) -> Option<&str> {
        self.digest.as_deref()
    }

    /// Returns the original registry when pulled via a mirror.
    ///
    /// Since this is not part of the stable OCI Distribution Spec yet, this feature is exempt from
    /// semver backwards compatibility guarantees and might change in the future.
    #[doc(hidden)]
    pub fn namespace(&self) -> Option<&str> {
        if self.mirror_registry.is_some() {
            Some(self.registry())
        } else {
            None
        }
    }

    /// Returns the full repository name and path.
    fn full_name(&self) -> String {
        if self.registry() == "" {
            self.repository().to_string()
        } else {
            format!("{}/{}", self.registry(), self.repository())
        }
    }

    /// Returns the whole reference.
    pub fn whole(&self) -> String {
        let mut s = self.full_name();
        if let Some(t) = self.tag() {
            if !s.is_empty() {
                s.push(':');
            }
            s.push_str(t);
        }
        if let Some(d) = self.digest() {
            if !s.is_empty() {
                s.push('@');
            }
            s.push_str(d);
        }
        s
    }
}

impl fmt::Display for Reference {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.whole())
    }
}

impl FromStr for Reference {
    type Err = ParseError;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        Reference::try_from(s)
    }
}

impl TryFrom<String> for Reference {
    type Error = ParseError;

    fn try_from(s: String) -> Result<Self, Self::Error> {
        if s.is_empty() {
            return Err(ParseError::NameEmpty);
        }
        let captures = match reference_regexp().captures(&s) {
            Some(caps) => caps,
            None => {
                return Err(ParseError::ReferenceInvalidFormat);
            }
        };
        let name = &captures[1];
        let mut tag = captures.get(2).map(|m| m.as_str().to_owned());
        let digest = captures.get(3).map(|m| m.as_str().to_owned());
        if tag.is_none() && digest.is_none() {
            tag = Some(DEFAULT_TAG.into());
        }
        let (registry, repository) = split_domain(name);
        let reference = Reference {
            registry,
            mirror_registry: None,
            repository,
            tag,
            digest,
        };
        if reference.repository().len() > NAME_TOTAL_LENGTH_MAX {
            return Err(ParseError::NameTooLong);
        }
        // Digests much always be hex-encoded, ensuring that their hex portion will always be
        // size*2
        if let Some(digest) = reference.digest() {
            match digest.split_once(':') {
                None => return Err(ParseError::DigestInvalidFormat),
                Some(("sha256", digest)) => {
                    if digest.len() != 64 {
                        return Err(ParseError::DigestInvalidLength);
                    }
                }
                Some(("sha384", digest)) => {
                    if digest.len() != 96 {
                        return Err(ParseError::DigestInvalidLength);
                    }
                }
                Some(("sha512", digest)) => {
                    if digest.len() != 128 {
                        return Err(ParseError::DigestInvalidLength);
                    }
                }
                Some((_, _)) => return Err(ParseError::DigestUnsupported),
            }
        }
        Ok(reference)
    }
}

impl TryFrom<&str> for Reference {
    type Error = ParseError;
    fn try_from(string: &str) -> Result<Self, Self::Error> {
        TryFrom::try_from(string.to_owned())
    }
}

impl From<Reference> for String {
    fn from(reference: Reference) -> Self {
        reference.whole()
    }
}

/// Splits a repository name to domain and remotename string.
/// If no valid domain is found, the default domain is used. Repository name
/// needs to be already validated before.
///
/// This function is a Rust rewrite of the official Go code used by Docker:
/// https://github.com/distribution/distribution/blob/41a0452eea12416aaf01bceb02a924871e964c67/reference/normalize.go#L87-L104
fn split_domain(name: &str) -> (String, String) {
    let mut domain: String;
    let mut remainder: String;

    match name.split_once('/') {
        None => {
            domain = DOCKER_HUB_DOMAIN.into();
            remainder = name.into();
        }
        Some((left, right)) => {
            if !(left.contains('.') || left.contains(':')) && left != "localhost" {
                domain = DOCKER_HUB_DOMAIN.into();
                remainder = name.into();
            } else {
                domain = left.into();
                remainder = right.into();
            }
        }
    }
    if domain == DOCKER_HUB_DOMAIN_LEGACY {
        domain = DOCKER_HUB_DOMAIN.into();
    }
    if domain == DOCKER_HUB_DOMAIN && !remainder.contains('/') {
        remainder = format!("{DOCKER_HUB_OFFICIAL_REPO_NAME}/{remainder}");
    }

    (domain, remainder)
}

#[cfg(test)]
mod test {
    use super::*;

    mod parse {
        use super::*;
        use rstest::rstest;

        #[rstest(input, registry, repository, tag, digest, whole,
            case("busybox", "docker.io", "library/busybox", Some("latest"), None, "docker.io/library/busybox:latest"),
            case("test.com:tag", "docker.io", "library/test.com", Some("tag"), None, "docker.io/library/test.com:tag"),
            case("test.com:5000", "docker.io", "library/test.com", Some("5000"), None, "docker.io/library/test.com:5000"),
            case("test.com/repo:tag", "test.com", "repo", Some("tag"), None, "test.com/repo:tag"),
            case("test:5000/repo", "test:5000", "repo", Some("latest"), None, "test:5000/repo:latest"),
            case("test:5000/repo:tag", "test:5000", "repo", Some("tag"), None, "test:5000/repo:tag"),
            case("test:5000/repo@sha256:ffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffff", "test:5000", "repo", None, Some("sha256:ffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffff"), "test:5000/repo@sha256:ffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffff"),
            case("test:5000/repo:tag@sha256:ffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffff", "test:5000", "repo", Some("tag"), Some("sha256:ffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffff"), "test:5000/repo:tag@sha256:ffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffff"),
            case("lowercase:Uppercase", "docker.io", "library/lowercase", Some("Uppercase"), None, "docker.io/library/lowercase:Uppercase"),
            case("sub-dom1.foo.com/bar/baz/quux", "sub-dom1.foo.com", "bar/baz/quux", Some("latest"), None, "sub-dom1.foo.com/bar/baz/quux:latest"),
            case("sub-dom1.foo.com/bar/baz/quux:some-long-tag", "sub-dom1.foo.com", "bar/baz/quux", Some("some-long-tag"), None, "sub-dom1.foo.com/bar/baz/quux:some-long-tag"),
            case("b.gcr.io/test.example.com/my-app:test.example.com", "b.gcr.io", "test.example.com/my-app", Some("test.example.com"), None, "b.gcr.io/test.example.com/my-app:test.example.com"),
            // ☃.com in punycode
            case("xn--n3h.com/myimage:xn--n3h.com", "xn--n3h.com", "myimage", Some("xn--n3h.com"), None, "xn--n3h.com/myimage:xn--n3h.com"),
            // 🐳.com in punycode
            case("xn--7o8h.com/myimage:xn--7o8h.com@sha512:ffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffff", "xn--7o8h.com", "myimage", Some("xn--7o8h.com"), Some("sha512:ffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffff"), "xn--7o8h.com/myimage:xn--7o8h.com@sha512:ffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffff"),
            case("foo_bar.com:8080", "docker.io", "library/foo_bar.com", Some("8080"), None, "docker.io/library/foo_bar.com:8080" ),
            case("foo/foo_bar.com:8080", "docker.io", "foo/foo_bar.com", Some("8080"), None, "docker.io/foo/foo_bar.com:8080"),
            case("opensuse/leap:15.3", "docker.io", "opensuse/leap", Some("15.3"), None, "docker.io/opensuse/leap:15.3"),
        )]
        fn parse_good_reference(
            input: &str,
            registry: &str,
            repository: &str,
            tag: Option<&str>,
            digest: Option<&str>,
            whole: &str,
        ) {
            println!("input: {}", input);
            let reference = Reference::try_from(input).expect("could not parse reference");
            println!("{} -> {:?}", input, reference);
            assert_eq!(registry, reference.registry());
            assert_eq!(repository, reference.repository());
            assert_eq!(tag, reference.tag());
            assert_eq!(digest, reference.digest());
            assert_eq!(whole, reference.whole());
        }

        #[rstest(input, err,
            case("", ParseError::NameEmpty),
            case(":justtag", ParseError::ReferenceInvalidFormat),
            case("@sha256:ffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffff", ParseError::ReferenceInvalidFormat),
            case("repo@sha256:ffffffffffffffffffffffffffffffffff", ParseError::DigestInvalidLength),
            case("validname@invaliddigest:ffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffff", ParseError::DigestUnsupported),
            // FIXME: should really pass a ParseError::NameContainsUppercase, but "invalid format" is good enough for now.
            case("Uppercase:tag", ParseError::ReferenceInvalidFormat),
            // FIXME: "Uppercase" is incorrectly handled as a domain-name here, and therefore passes.
            // https://github.com/docker/distribution/blob/master/reference/reference_test.go#L104-L109
            // case("Uppercase/lowercase:tag", ParseError::NameContainsUppercase),
            // FIXME: should really pass a ParseError::NameContainsUppercase, but "invalid format" is good enough for now.
            case("test:5000/Uppercase/lowercase:tag", ParseError::ReferenceInvalidFormat),
            case("aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa", ParseError::NameTooLong),
            case("aa/asdf$$^/aa", ParseError::ReferenceInvalidFormat)
        )]
        fn parse_bad_reference(input: &str, err: ParseError) {
            assert_eq!(Reference::try_from(input).unwrap_err(), err)
        }

        #[rstest(
            input,
            registry,
            resolved_registry,
            whole,
            case(
                "busybox",
                "docker.io",
                "index.docker.io",
                "docker.io/library/busybox:latest"
            ),
            case("test.com/repo:tag", "test.com", "test.com", "test.com/repo:tag"),
            case("test:5000/repo", "test:5000", "test:5000", "test:5000/repo:latest"),
            case(
                "sub-dom1.foo.com/bar/baz/quux",
                "sub-dom1.foo.com",
                "sub-dom1.foo.com",
                "sub-dom1.foo.com/bar/baz/quux:latest"
            ),
            case(
                "b.gcr.io/test.example.com/my-app:test.example.com",
                "b.gcr.io",
                "b.gcr.io",
                "b.gcr.io/test.example.com/my-app:test.example.com"
            )
        )]
        fn test_mirror_registry(input: &str, registry: &str, resolved_registry: &str, whole: &str) {
            let mut reference = Reference::try_from(input).expect("could not parse reference");
            assert_eq!(resolved_registry, reference.resolve_registry());
            assert_eq!(registry, reference.registry());
            assert_eq!(None, reference.namespace());
            assert_eq!(whole, reference.whole());

            reference.set_mirror_registry("docker.mirror.io".to_owned());
            assert_eq!("docker.mirror.io", reference.resolve_registry());
            assert_eq!(registry, reference.registry());
            assert_eq!(Some(registry), reference.namespace());
            assert_eq!(whole, reference.whole());
        }
    }
}
