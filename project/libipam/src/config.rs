use std::net::IpAddr;

use crate::range_set::RangeSet;

use cni_plugin::{config::RuntimeConfig, reply::Route};
use serde::{Deserialize, Serialize};

#[derive(Serialize, Deserialize, Debug)]
pub struct IPAMConfig {
    #[serde(rename = "type")]
    pub type_field: String,

    pub name: Option<String>,

    #[serde(rename = "routes")]
    pub routes: Option<Vec<Route>>,

    #[serde(rename = "resolvConf")]
    pub resolv_conf: Option<String>,

    #[serde(rename = "dataDir")]
    pub data_dir: Option<String>,

    #[serde(rename = "ranges")]
    pub ranges: Vec<RangeSet>,

    // IPArgs are not serialized/deserialized as they are marked with `json:"-"`
    #[serde(skip)]
    pub ip_args: Vec<IpAddr>,
}

#[derive(Serialize, Deserialize, Debug)]
pub struct Net {
    #[serde(rename = "cniVersion")]
    pub cni_version: String,

    pub name: String,

    #[serde(rename = "ipam")]
    pub ipam: IPAMConfig,

    #[serde(rename = "runtimeConfig")]
    pub runtime_config: Option<RuntimeConfig>,
    // todo
    // #[serde(rename = "args")]
    // pub args: Option<IPAMArgs>,
}

#[cfg(test)]
mod tests {
    use crate::config::Net;

    #[test]
    fn test_parse() {
        let str = r#"{
  "cniVersion": "0.3.1",
  "name": "examplenet",
  "ipam": {
    "type": "host-local",
    "ranges": [
      [
        {
          "subnet": "10.10.0.0/16",
          "rangeStart": "10.10.1.20",
          "rangeEnd": "10.10.3.50",
          "gateway": "10.10.0.254"
        },
        {
          "subnet": "172.16.5.0/24"
        }
      ],
      [
        {
          "subnet": "3ffe:ffff:0:01ff::/64",
          "rangeStart": "3ffe:ffff:0:01ff::0010",
          "rangeEnd": "3ffe:ffff:0:01ff::0020"
        }
      ]
    ],
    "routes": [
      {
        "dst": "0.0.0.0/0"
      },
      {
        "dst": "192.168.0.0/16",
        "gw": "10.10.5.1"
      },
      {
        "dst": "3ffe:ffff:0:01ff::1/64"
      }
    ],
    "dataDir": "/run/my-orchestrator/container-ipam-state"
  }
}
        "#;
        let ipam_config: Net = serde_json::from_str(str).unwrap();
        println!("{:?}", ipam_config)
    }
}
