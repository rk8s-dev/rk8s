//! OCI Image Configuration
//!
//! Definition following <https://github.com/opencontainers/image-spec/blob/v1.0/config.md>

use std::collections::{HashMap, HashSet};

use chrono::{DateTime, Utc};
use serde::{ser::SerializeMap, Deserialize, Deserializer, Serialize, Serializer};

/// The CPU architecture which the binaries in this image are
/// built to run on.
/// Validated values are listed in [Go Language document for GOARCH](https://golang.org/doc/install/source#environment)
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, Default)]
#[serde(rename_all = "lowercase")]
pub enum Architecture {
    /// Arm
    Arm,
    /// Arm 64bit
    Arm64,
    /// Amd64/x86-64
    #[default]
    Amd64,
    /// Intel i386
    #[serde(rename = "386")]
    I386,
    /// Wasm
    Wasm,
    /// Loong64
    Loong64,
    /// MIPS
    Mips,
    /// MIPSle
    Mipsle,
    /// MIPS64
    Mips64,
    /// MIPS64le
    Mips64le,
    /// Power PC64
    PPC64,
    /// Power PC64le
    PPC64le,
    /// RiscV 64
    Riscv64,
    /// IBM s390x
    S390x,
    /// With this field empty
    #[serde(rename = "")]
    None,
}

/// The name of the operating system which the image is
/// built to run on.
/// Validated values are listed in [Go Language document for GOARCH](https://golang.org/doc/install/source#environment)
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, Default)]
#[serde(rename_all = "lowercase")]
pub enum Os {
    /// IBM AIX
    Aix,
    /// Android
    Android,
    /// Apple Darwin
    Darwin,
    /// FreeBSD Dragonfly
    Dragonfly,
    /// FreeBSD
    Freebsd,
    /// Illumos
    Illumos,
    /// iOS
    Ios,
    /// Js
    Js,
    /// Linux
    #[default]
    Linux,
    /// NetBSD
    Netbsd,
    /// OpenBSD
    Openbsd,
    /// Plan9 from Bell Labs
    Plan9,
    /// Solaris
    Solaris,
    /// WASI Preview 1
    Wasip1,
    /// Microsoft Windows
    Windows,
    /// With this field empty
    #[serde(rename = "")]
    None,
}

/// An OCI Image is an ordered collection of root filesystem changes
/// and the corresponding execution parameters for use within a
/// container runtime.
///
/// Format defined [here](https://github.com/opencontainers/image-spec/blob/v1.0/config.md)
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, Default)]
pub struct ConfigFile {
    /// An combined date and time at which the image was created,
    /// formatted as defined by
    /// [RFC 3339, section 5.6](https://tools.ietf.org/html/rfc3339#section-5.6)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub created: Option<DateTime<Utc>>,

    /// Gives the name and/or email address of the person or entity
    /// which created and is responsible for maintaining the image.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub author: Option<String>,

    /// The CPU architecture which the binaries in this image are
    /// built to run on.
    pub architecture: Architecture,

    /// The name of the operating system which the image is built to run on.
    /// Validated values are listed in [Go Language document for GOOS](https://golang.org/doc/install/source#environment)
    pub os: Os,

    /// The execution parameters which SHOULD be used as a base when running a container using the image.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub config: Option<Config>,

    /// The rootfs key references the layer content addresses used by the image.
    pub rootfs: Rootfs,

    /// Describes the history of each layer.
    #[serde(skip_serializing_if = "is_option_vec_empty")]
    pub history: Option<Vec<History>>,
}

fn is_option_vec_empty<T>(opt_vec: &Option<Vec<T>>) -> bool {
    if let Some(vec) = opt_vec {
        vec.is_empty()
    } else {
        true
    }
}

/// Helper struct to be serialized into and deserialized from `{}`
#[derive(Deserialize, Serialize)]
struct Empty {}

/// Helper to deserialize a `map[string]struct{}` of golang
fn optional_hashset_from_str<'de, D: Deserializer<'de>>(
    d: D,
) -> Result<Option<HashSet<String>>, D::Error> {
    let res = <Option<HashMap<String, Empty>>>::deserialize(d)?.map(|h| h.into_keys().collect());
    Ok(res)
}

/// Helper to serialize an optional hashset
fn serialize_optional_hashset<T, S>(
    value: &Option<HashSet<T>>,
    serializer: S,
) -> Result<S::Ok, S::Error>
where
    T: Serialize,
    S: Serializer,
{
    match value {
        Some(set) => {
            let empty = Empty {};
            let mut map = serializer.serialize_map(Some(set.len()))?;
            for k in set {
                map.serialize_entry(k, &empty)?;
            }

            map.end()
        }
        None => serializer.serialize_none(),
    }
}

/// The execution parameters which SHOULD be used as a base when running a container using the image.
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, Default)]
#[serde(rename_all = "PascalCase")]
pub struct Config {
    /// The username or UID which is a platform-specific structure
    /// that allows specific control over which user the process run as. This acts as a default value to use when the value is
    /// not specified when creating a container. For Linux based
    /// systems, all of the following are valid: `user`, `uid`,
    /// `user:group`, `uid:gid`, `uid:group`, `user:gid`. If `group`/`gid` is
    /// not specified, the default group and supplementary groups
    /// of the given `user`/`uid` in `/etc/passwd` from the container are
    /// applied.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub user: Option<String>,

    /// A set of ports to expose from a container running this
    /// image. Its keys can be in the format of: `port/tcp`, `port/udp`,
    /// `port` with the default protocol being `tcp` if not specified.
    /// These values act as defaults and are merged with any
    /// specified when creating a container.
    #[serde(
        skip_serializing_if = "is_option_hashset_empty",
        deserialize_with = "optional_hashset_from_str",
        serialize_with = "serialize_optional_hashset",
        default
    )]
    pub exposed_ports: Option<HashSet<String>>,

    /// Entries are in the format of `VARNAME=VARVALUE`.
    #[serde(skip_serializing_if = "is_option_vec_empty")]
    pub env: Option<Vec<String>>,

    /// Default arguments to the entrypoint of the container.
    #[serde(skip_serializing_if = "is_option_vec_empty")]
    pub cmd: Option<Vec<String>>,

    /// A list of arguments to use as the command to execute when
    /// the container starts..
    #[serde(skip_serializing_if = "is_option_vec_empty")]
    pub entrypoint: Option<Vec<String>>,

    /// A set of directories describing where the process is likely write data specific to a container instance.
    #[serde(
        skip_serializing_if = "is_option_hashset_empty",
        deserialize_with = "optional_hashset_from_str",
        serialize_with = "serialize_optional_hashset",
        default
    )]
    pub volumes: Option<HashSet<String>>,

    /// Sets the current working directory of the entrypoint
    /// process in the container.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub working_dir: Option<String>,

    /// The field contains arbitrary metadata for the container.
    /// This property MUST use the [annotation rules](https://github.com/opencontainers/image-spec/blob/v1.0/annotations.md#rules).
    #[serde(skip_serializing_if = "is_option_hashmap_empty")]
    pub labels: Option<HashMap<String, String>>,

    /// The field contains the system call signal that will be sent
    /// to the container to exit. The signal can be a signal name
    /// in the format `SIGNAME`, for instance `SIGKILL` or `SIGRTMIN+3`.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub stop_signal: Option<String>,
}

fn is_option_hashset_empty<T>(opt_hash: &Option<HashSet<T>>) -> bool {
    if let Some(hash) = opt_hash {
        hash.is_empty()
    } else {
        true
    }
}

fn is_option_hashmap_empty<T, V>(opt_hash: &Option<HashMap<T, V>>) -> bool {
    if let Some(hash) = opt_hash {
        hash.is_empty()
    } else {
        true
    }
}

/// Default value of the type of a [`Rootfs`]
pub const ROOTFS_TYPE: &str = "layers";

/// The rootfs key references the layer content addresses used by the image.
/// This makes the image config hash depend on the filesystem hash.
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
pub struct Rootfs {
    /// MUST be set to `layers`.
    pub r#type: String,

    /// An array of layer content hashes (`DiffIDs`), in order from first to last.
    pub diff_ids: Vec<String>,
}

impl Default for Rootfs {
    fn default() -> Self {
        Self {
            r#type: String::from(ROOTFS_TYPE),
            diff_ids: Default::default(),
        }
    }
}

/// Describes the history of each layer. The array is ordered from first to last.
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, Default)]
pub struct History {
    /// A combined date and time at which the layer was created,
    /// formatted as defined by [RFC 3339, section 5.6](https://tools.ietf.org/html/rfc3339#section-5.6).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub created: Option<DateTime<Utc>>,

    /// The author of the build point.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub author: Option<String>,

    /// The command which created the layer.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub created_by: Option<String>,

    /// A custom message set when creating the layer.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub comment: Option<String>,

    /// This field is used to mark if the history item created a
    /// filesystem diff. It is set to true if this history item
    /// doesn't correspond to an actual layer in the rootfs section
    /// (for example, Dockerfile's `ENV` command results in no
    /// change to the filesystem).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub empty_layer: Option<bool>,
}

#[cfg(test)]
mod tests {
    use assert_json_diff::assert_json_eq;
    use chrono::DateTime;
    use rstest::*;
    use serde_json::Value;
    use std::collections::{HashMap, HashSet};

    use super::{Architecture, Config, ConfigFile, History, Os, Rootfs};

    const EXAMPLE_CONFIG: &str = r#"
    {
        "created": "2015-10-31T22:22:56.015925234Z",
        "author": "Alyssa P. Hacker <alyspdev@example.com>",
        "architecture": "amd64",
        "os": "linux",
        "config": {
            "User": "alice",
            "ExposedPorts": {
                "8080/tcp": {}
            },
            "Env": [
                "PATH=/usr/local/sbin:/usr/local/bin:/usr/sbin:/usr/bin:/sbin:/bin",
                "FOO=oci_is_a",
                "BAR=well_written_spec"
            ],
            "Entrypoint": [
                "/bin/my-app-binary"
            ],
            "Cmd": [
                "--foreground",
                "--config",
                "/etc/my-app.d/default.cfg"
            ],
            "Volumes": {
                "/var/job-result-data": {},
                "/var/log/my-app-logs": {}
            },
            "WorkingDir": "/home/alice",
            "Labels": {
                "com.example.project.git.url": "https://example.com/project.git",
                "com.example.project.git.commit": "45a939b2999782a3f005621a8d0f29aa387e1d6b"
            }
        },
        "rootfs": {
          "diff_ids": [
            "sha256:c6f988f4874bb0add23a778f753c65efe992244e148a1d2ec2a8b664fb66bbd1",
            "sha256:5f70bf18a086007016e948b04aed3b82103a36bea41755b6cddfaf10ace3c6ef"
          ],
          "type": "layers"
        },
        "history": [
          {
            "created": "2015-10-31T22:22:54.690851953Z",
            "created_by": "/bin/sh -c #(nop) ADD file:a3bc1e842b69636f9df5256c49c5374fb4eef1e281fe3f282c65fb853ee171c5 in /"
          },
          {
            "created": "2015-10-31T22:22:55.613815829Z",
            "created_by": "/bin/sh -c #(nop) CMD [\"sh\"]",
            "empty_layer": true
          }
        ]
    }"#;

    fn example_config() -> ConfigFile {
        let config = Config {
            user: Some("alice".into()),
            exposed_ports: Some(HashSet::from_iter(vec!["8080/tcp".into()])),
            env: Some(vec![
                "PATH=/usr/local/sbin:/usr/local/bin:/usr/sbin:/usr/bin:/sbin:/bin".into(),
                "FOO=oci_is_a".into(),
                "BAR=well_written_spec".into(),
            ]),
            cmd: Some(vec![
                "--foreground".into(),
                "--config".into(),
                "/etc/my-app.d/default.cfg".into(),
            ]),
            entrypoint: Some(vec!["/bin/my-app-binary".into()]),
            volumes: Some(HashSet::from_iter(vec![
                "/var/job-result-data".into(),
                "/var/log/my-app-logs".into(),
            ])),
            working_dir: Some("/home/alice".into()),
            labels: Some(HashMap::from_iter(vec![
                (
                    "com.example.project.git.url".into(),
                    "https://example.com/project.git".into(),
                ),
                (
                    "com.example.project.git.commit".into(),
                    "45a939b2999782a3f005621a8d0f29aa387e1d6b".into(),
                ),
            ])),
            stop_signal: None,
        };
        let rootfs = Rootfs {
            r#type: "layers".into(),
            diff_ids: vec![
                "sha256:c6f988f4874bb0add23a778f753c65efe992244e148a1d2ec2a8b664fb66bbd1".into(),
                "sha256:5f70bf18a086007016e948b04aed3b82103a36bea41755b6cddfaf10ace3c6ef".into(),
            ],
        };

        let history = Some(vec![History {
            created: Some(DateTime::parse_from_rfc3339("2015-10-31T22:22:54.690851953Z").expect("parse time failed").into()),
            author: None,
            created_by: Some("/bin/sh -c #(nop) ADD file:a3bc1e842b69636f9df5256c49c5374fb4eef1e281fe3f282c65fb853ee171c5 in /".into()),
            comment: None,
            empty_layer: None,
        },
        History {
            created: Some(DateTime::parse_from_rfc3339("2015-10-31T22:22:55.613815829Z").expect("parse time failed").into()),
            author: None,
            created_by: Some("/bin/sh -c #(nop) CMD [\"sh\"]".into()),
            comment: None,
            empty_layer: Some(true),
        }]);
        ConfigFile {
            created: Some(
                DateTime::parse_from_rfc3339("2015-10-31T22:22:56.015925234Z")
                    .expect("parse time failed")
                    .into(),
            ),
            author: Some("Alyssa P. Hacker <alyspdev@example.com>".into()),
            architecture: Architecture::Amd64,
            os: Os::Linux,
            config: Some(config),
            rootfs,
            history,
        }
    }

    const MINIMAL_CONFIG: &str = r#"
    {
        "architecture": "amd64",
        "os": "linux",
        "rootfs": {
          "diff_ids": [
            "sha256:c6f988f4874bb0add23a778f753c65efe992244e148a1d2ec2a8b664fb66bbd1",
            "sha256:5f70bf18a086007016e948b04aed3b82103a36bea41755b6cddfaf10ace3c6ef"
          ],
          "type": "layers"
        }
    }"#;

    fn minimal_config() -> ConfigFile {
        let rootfs = Rootfs {
            r#type: "layers".into(),
            diff_ids: vec![
                "sha256:c6f988f4874bb0add23a778f753c65efe992244e148a1d2ec2a8b664fb66bbd1".into(),
                "sha256:5f70bf18a086007016e948b04aed3b82103a36bea41755b6cddfaf10ace3c6ef".into(),
            ],
        };

        ConfigFile {
            architecture: Architecture::Amd64,
            os: Os::Linux,
            config: None,
            rootfs,
            history: None,
            created: None,
            author: None,
        }
    }

    const MINIMAL_CONFIG2: &str = r#"
    {
        "architecture":"arm64",
        "config":{
            "Env":["PATH=/usr/local/sbin:/usr/local/bin:/usr/sbin:/usr/bin:/sbin:/bin"],
            "WorkingDir":"/"
        },
        "created":"2023-04-21T11:53:28.176613804Z",
        "history":[{
            "created":"2023-04-21T11:53:28.176613804Z",
            "created_by":"COPY ./src/main.rs / # buildkit",
            "comment":"buildkit.dockerfile.v0"
        }],
        "os":"linux",
        "rootfs":{
            "type":"layers",
            "diff_ids":[
                "sha256:267fbf1f5a9377e40a2dc65b355000111e000a35ac77f7b19a59f587d4dd778e"
            ]
        }
    }"#;

    fn minimal_config2() -> ConfigFile {
        let config = Some(Config {
            env: Some(vec![
                "PATH=/usr/local/sbin:/usr/local/bin:/usr/sbin:/usr/bin:/sbin:/bin".into(),
            ]),
            working_dir: Some("/".into()),
            ..Config::default()
        });
        let history = Some(vec![History {
            created: Some(
                DateTime::parse_from_rfc3339("2023-04-21T11:53:28.176613804Z")
                    .expect("parse time failed")
                    .into(),
            ),
            author: None,
            created_by: Some("COPY ./src/main.rs / # buildkit".into()),
            comment: Some("buildkit.dockerfile.v0".into()),
            empty_layer: None,
        }]);

        let rootfs = Rootfs {
            r#type: "layers".into(),
            diff_ids: vec![
                "sha256:267fbf1f5a9377e40a2dc65b355000111e000a35ac77f7b19a59f587d4dd778e".into(),
            ],
        };

        ConfigFile {
            architecture: Architecture::Arm64,
            os: Os::Linux,
            config,
            rootfs,
            history,
            created: Some(
                DateTime::parse_from_rfc3339("2023-04-21T11:53:28.176613804Z")
                    .expect("parse time failed")
                    .into(),
            ),
            author: None,
        }
    }

    #[rstest]
    #[case(example_config(), EXAMPLE_CONFIG)]
    #[case(minimal_config(), MINIMAL_CONFIG)]
    #[case(minimal_config2(), MINIMAL_CONFIG2)]
    fn deserialize_test(#[case] config: ConfigFile, #[case] expected: &str) {
        let parsed: ConfigFile = serde_json::from_str(expected).expect("parsed failed");
        assert_eq!(config, parsed);
    }

    #[rstest]
    #[case(example_config(), EXAMPLE_CONFIG)]
    #[case(minimal_config(), MINIMAL_CONFIG)]
    #[case(minimal_config2(), MINIMAL_CONFIG2)]
    fn serialize_test(#[case] config: ConfigFile, #[case] expected: &str) {
        let serialized = serde_json::to_value(&config).expect("serialize failed");
        let parsed: Value = serde_json::from_str(expected).expect("parsed failed");
        assert_json_eq!(serialized, parsed);
    }
}
