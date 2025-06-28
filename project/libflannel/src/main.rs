use std::collections::HashMap;
use std::fs::{self, File};
use std::io::Write;
use std::path::{Path, PathBuf};

use anyhow::{Context, Result};
use cni_plugin::config::IpamConfig;
use cni_plugin::{
    Cni, Command, Inputs,
    config::NetworkConfig,
    delegation::delegate,
    error::CniError,
    reply::{SuccessReply, reply},
};
use ipnetwork::{Ipv4Network, Ipv6Network};
use log::{debug, error, info};

use serde_json::{Map, Value, json};
use types::{FlannelNetConf, SubnetEnv};

mod types;
//const DEFAULT_SUBNET_FILE: &str = "/run/flannel/subnet.env";
const DEFAULT_SUBNET_FILE: &str = "/etc/cni/net.d/subnet.env";
const DEFAULT_DATA_DIR: &str = "/var/lib/cni/flannel";

/// Entry point of the CNI bridge plugin.
fn main() {
    cni_plugin::logger::install("libflannel.log");
    debug!(
        "{} (CNI flannel plugin) version {}",
        env!("CARGO_PKG_NAME"),
        env!("CARGO_PKG_VERSION")
    );

    let inputs: Inputs = Cni::load().into_inputs().unwrap();
    let cni_version = inputs.config.cni_version.clone();

    info!(
        "{} serving spec v{} for command={:?}",
        env!("CARGO_PKG_NAME"),
        cni_version,
        inputs.command
    );

    let flannel_conf = match load_flannel_net_conf(inputs.config.clone()) {
        Ok(conf) => conf,
        Err(err) => {
            error!("Failed to load flannel config: {}", err);
            return;
        }
    };

    info!(
        "(CNI flannel plugin) version flannel config: {:?}",
        flannel_conf
    );

    let rt = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .unwrap();

    let res: Result<SuccessReply, CniError> = rt.block_on(async move {
        match inputs.command {
            Command::Add => cmd_add(flannel_conf, inputs).await,
            Command::Del => cmd_del(flannel_conf, inputs).await,
            Command::Check => todo!(),
            Command::Version => unreachable!(),
        }
    });

    match res {
        Ok(res) => {
            debug!("success! {:#?}", res);
            reply(res)
        }
        Err(res) => {
            error!("error: {}", res);
            reply(res.into_reply(cni_version))
        }
    }
}

fn load_flannel_net_conf(config: NetworkConfig) -> Result<FlannelNetConf, CniError> {
    let mut json_value = serde_json::to_value(&config).map_err(CniError::from)?;

    if json_value.get("subnetFile").is_none() {
        json_value["subnetFile"] = serde_json::json!(DEFAULT_SUBNET_FILE);
    }
    if json_value.get("dataDir").is_none() {
        json_value["dataDir"] = serde_json::json!(DEFAULT_DATA_DIR);
    }

    let flannel_conf: FlannelNetConf =
        serde_json::from_value(json_value).map_err(CniError::from)?;

    Ok(flannel_conf)
}

fn load_flannel_subnet_env(path: &str) -> Result<SubnetEnv, CniError> {
    let content = std::fs::read_to_string(path).map_err(|e| CniError::Generic(e.to_string()))?;

    let mut subnet_env = SubnetEnv {
        networks: Vec::new(),
        subnet: None,
        ip6_networks: Vec::new(),
        ip6_subnet: None,
        mtu: None,
        ipmasq: None,
    };
    for line in content.lines() {
        let line = line.trim();
        if line.is_empty() || line.starts_with('#') {
            continue;
        }
        if let Some((k, v)) = line.split_once('=') {
            let k = k.trim();
            let v = v.trim();
            match k {
                "FLANNEL_NETWORK" => {
                    subnet_env.networks = v
                        .split(',')
                        .map(|s| s.parse::<Ipv4Network>())
                        .collect::<Result<_, _>>()
                        .map_err(|e| CniError::Generic(e.to_string()))?;
                }
                "FLANNEL_IPV6_NETWORK" => {
                    subnet_env.ip6_networks = v
                        .split(',')
                        .map(|s| s.parse::<Ipv6Network>())
                        .collect::<Result<_, _>>()
                        .map_err(|e| CniError::Generic(e.to_string()))?;
                }
                "FLANNEL_SUBNET" => {
                    subnet_env.subnet = Some(
                        v.parse::<Ipv4Network>()
                            .map_err(|e| CniError::Generic(e.to_string()))?,
                    );
                }
                "FLANNEL_IPV6_SUBNET" => {
                    subnet_env.ip6_subnet = Some(
                        v.parse::<Ipv6Network>()
                            .map_err(|e| CniError::Generic(e.to_string()))?,
                    );
                }
                "FLANNEL_MTU" => {
                    subnet_env.mtu = Some(
                        v.parse::<u32>()
                            .map_err(|e| CniError::Generic(e.to_string()))?,
                    );
                }
                "FLANNEL_IPMASQ" => {
                    subnet_env.ipmasq = Some(
                        v.parse::<bool>()
                            .map_err(|e| CniError::Generic(e.to_string()))?,
                    );
                }
                _ => {}
            }
        }
    }
    Ok(subnet_env)
}

fn get_delegate_ipam(
    flannel_conf: &mut FlannelNetConf,
    subnet_env: &SubnetEnv,
) -> Result<(), CniError> {
    if flannel_conf.net_conf.ipam.is_none() {
        flannel_conf.net_conf.ipam = Some(IpamConfig {
            plugin: "libipam".to_string(),
            specific: HashMap::new(),
        });
    }

    let ipam = flannel_conf.net_conf.ipam.as_mut().unwrap();
    if ipam.plugin.is_empty() {
        ipam.plugin = "libipam".to_string();
    }

    let mut ranges = Vec::new();
    if let Some(sn) = &subnet_env.subnet {
        let gateway_ip = sn.nth(1).ok_or_else(|| {
            CniError::Generic(format!("failed to compute gateway from subnet {sn}"))
        })?;
        ranges.push(Value::Array(vec![json!({
            "gateway": gateway_ip.to_string(),
            "subnet": sn.to_string()
        })]));
    }
    if let Some(ip6_sn) = &subnet_env.ip6_subnet {
        ranges.push(Value::Array(vec![json!({"subnet": ip6_sn.to_string()})]));
    }
    #[allow(clippy::collapsible_if)]
    if let Some(existing_ranges) = ipam.specific.get("ranges") {
        if let Some(arr) = existing_ranges.as_array() {
            ranges.extend(arr.clone());
        }
    }

    ipam.specific.insert("ranges".into(), Value::Array(ranges));

    let mut routes = Vec::new();
    let gateway_v4 = subnet_env
        .subnet
        .map(|subnet| subnet.nth(1).unwrap().to_string());
    routes.extend(subnet_env.networks.iter().map(|n| {
        let mut route = json!({"dst": n.to_string()});
        if let Some(ref gw) = gateway_v4 {
            route
                .as_object_mut()
                .unwrap()
                .insert("gw".to_string(), json!(gw));
        }
        route
    }));
    routes.extend(
        subnet_env
            .ip6_networks
            .iter()
            .map(|n| json!({"dst": n.to_string()})),
    );
    ipam.specific.insert("routes".into(), Value::Array(routes));

    info!("{}", serde_json::to_string(&ipam)?);

    Ok(())
}

async fn delegate_add(
    cid: &str,
    data_dir: &str,
    delegate_conf: &NetworkConfig,
) -> Result<SuccessReply, CniError> {
    let netconf_bytes = serde_json::to_vec(delegate_conf).map_err(CniError::Json)?;
    save_scratch_net_conf(cid, data_dir, &netconf_bytes)
        .map_err(|e| CniError::Generic(e.to_string()))?;

    let plugin_type = delegate_conf.plugin.as_str();
    let result: SuccessReply =
        match delegate(plugin_type, Command::Add, &delegate_conf.clone()).await {
            Ok(reply) => reply,
            Err(e) => return Err(e),
        };

    Ok(result)
}

pub fn save_scratch_net_conf(
    cid: &str,
    data_dir: &str,
    netconf_bytes: &[u8],
) -> anyhow::Result<()> {
    let cache_dir = build_cache_path(cid, data_dir)?;

    fs::create_dir_all(&cache_dir)
        .with_context(|| format!("Failed to create cache directory: {}", cache_dir.display()))?;

    let file_path = cache_dir.join(format!("{cid}.json"));

    let mut temp_file = File::create(&file_path)
        .with_context(|| format!("Failed to create temp file: {}", file_path.display()))?;

    temp_file
        .write_all(netconf_bytes)
        .with_context(|| format!("Failed to write to file: {}", file_path.display()))?;

    temp_file
        .sync_all()
        .with_context(|| format!("Failed to sync file: {}", file_path.display()))?;

    Ok(())
}

fn build_cache_path(cid: &str, data_dir: &str) -> anyhow::Result<PathBuf> {
    let base_path = Path::new(data_dir);

    if cid
        .chars()
        .any(|c| !c.is_ascii_alphanumeric() && c != '-' && c != '_')
    {
        anyhow::bail!("Invalid container ID format: {}", cid);
    }

    Ok(base_path.join("results").join(cid))
}

pub fn consume_scratch_net_conf(
    cid: &str,
    data_dir: &str,
) -> Result<(impl FnOnce(&anyhow::Error), Vec<u8>)> {
    let file_path = build_cache_path(cid, data_dir)?.join(format!("{cid}.json"));

    let contents = fs::read(&file_path)
        .with_context(|| format!("Failed to read file: {}", file_path.display()))?;

    let cleanup = move |err: &anyhow::Error| {
        if !err.is::<std::io::Error>() {
            let _ = fs::remove_file(&file_path);
        }
    };

    Ok((cleanup, contents))
}

async fn cmd_add(mut config: FlannelNetConf, inputs: Inputs) -> Result<SuccessReply, CniError> {
    let subnet_env = load_flannel_subnet_env(config.subnet_file.as_ref().unwrap())?;
    info!("subnet_env: {:?}", subnet_env);

    match &config.delegate {
        None => {
            config.delegate = Some(HashMap::new());
        }
        Some(delegate) => {
            if !delegate
                .get("type")
                .map(|it| it.is_string())
                .unwrap_or(false)
            {
                "'delegate' dictionary, if present, must have (string) 'type' field".to_string();
            }
            if delegate.get("name").is_some() {
                "'delegate' dictionary must not have 'name' field, it'll be set by flannel"
                    .to_string();
            }
            if delegate.get("ipam").is_some() {
                "'delegate' dictionary must not have 'ipam' field, it'll be set by flannel"
                    .to_string();
            }
        }
    }

    let delegate_mut = config.delegate.as_mut().unwrap();
    delegate_mut.insert("name".into(), Value::String(config.net_conf.name.clone()));
    delegate_mut
        .entry("type".into())
        .or_insert("libbridge".into());

    if !delegate_mut.contains_key("ipMasq") {
        delegate_mut.insert("ipMasq".into(), Value::Bool(!subnet_env.ipmasq.unwrap()));
    }

    delegate_mut
        .entry("mtu".into())
        .or_insert(Value::Number(subnet_env.mtu.unwrap().into()));

    if delegate_mut.get("type").unwrap().as_str() == Some("libbridge") {
        delegate_mut
            .entry("isGateway".into())
            .or_insert(Value::Bool(true));
    }
    delegate_mut.insert(
        "cniVersion".into(),
        Value::String(config.net_conf.cni_version.to_string()),
    );

    get_delegate_ipam(&mut config, &subnet_env)?;
    let delegate_mut = config.delegate.as_mut().unwrap();
    if let Some(ipam_config) = config.net_conf.ipam.clone() {
        delegate_mut.insert(
            "ipam".into(),
            serde_json::to_value(ipam_config).expect("Failed to convert IpamConfig to JSON value"),
        );
    }

    debug!("delegate_conf: {:?}", delegate_mut);
    let serde_map: Map<String, Value> = config
        .delegate
        .as_ref()
        .unwrap()
        .clone()
        .into_iter()
        .collect();

    let delegate_config: NetworkConfig = serde_json::from_value(Value::Object(serde_map))
        .map_err(|e| CniError::Generic(format!("Failed to parse delegate config: {e}")))?;

    let reply = delegate_add(
        &inputs.container_id,
        config.data_dir.as_ref().unwrap(),
        &delegate_config,
    )
    .await?;

    Ok(reply)
}

async fn cmd_del(config: FlannelNetConf, inputs: Inputs) -> Result<SuccessReply, CniError> {
    let result = SuccessReply {
        cni_version: config.net_conf.cni_version.clone(),
        interfaces: Default::default(),
        ips: Default::default(),
        routes: Default::default(),
        dns: Default::default(),
        specific: Default::default(),
    };

    let (cleanup, netconf_bytes) =
        match consume_scratch_net_conf(&inputs.container_id, config.data_dir.as_ref().unwrap()) {
            Ok((cleanup, bytes)) => (Some(cleanup), bytes),
            Err(err) => {
                if err
                    .downcast_ref::<std::io::Error>()
                    .map(|e| e.kind() == std::io::ErrorKind::NotFound)
                    .unwrap_or(false)
                {
                    return Ok(result);
                } else {
                    return Err(CniError::Generic(format!(
                        "Failed to cleanup config: {err}"
                    )));
                }
            }
        };

    struct CleanupGuard<F: FnOnce(&anyhow::Error)> {
        func: Option<F>,
        err: Option<anyhow::Error>,
    }

    impl<F: FnOnce(&anyhow::Error)> Drop for CleanupGuard<F> {
        fn drop(&mut self) {
            if let (Some(func), Some(err)) = (self.func.take(), self.err.as_ref()) {
                func(err);
            }
        }
    }

    let mut cleanup_guard = cleanup.map(|f| CleanupGuard {
        func: Some(f),
        err: None,
    });

    let nc: NetworkConfig = match serde_json::from_slice(&netconf_bytes) {
        Ok(v) => v,
        Err(e) => {
            let err = anyhow::anyhow!("failed to parse netconf: {}", e);
            let msg = err.to_string();
            if let Some(guard) = &mut cleanup_guard {
                guard.err = Some(err);
            }
            return Err(CniError::Generic(format!("Failed to cleanup_guard: {msg}")));
        }
    };

    let res: SuccessReply =
        match delegate::<SuccessReply>(&nc.plugin, Command::Del, &nc.clone()).await {
            Ok(reply) => reply,
            Err(e) => {
                if let Some(guard) = &mut cleanup_guard {
                    guard.err = Some(anyhow::anyhow!(e.to_string()));
                }
                return Err(CniError::Generic(e.to_string()));
            }
        };

    Ok(res)
}

#[cfg(test)]
mod tests {
    use super::*;
    use ipnetwork::Ipv4Network;
    use std::io::Write;
    use tempfile::{NamedTempFile, tempdir};

    fn create_temp_file(content: &str) -> NamedTempFile {
        let mut file = NamedTempFile::new().unwrap();
        write!(file, "{}", content).unwrap();
        file
    }

    #[test]
    fn test_load_flannel_subnet_env_normal() {
        let content = r#"
            FLANNEL_NETWORK=10.244.0.0/16,192.168.0.0/24
            FLANNEL_SUBNET=10.244.1.0/24
            FLANNEL_MTU=1450
            FLANNEL_IPMASQ=true
        "#;
        let file = create_temp_file(content);
        let result = load_flannel_subnet_env(file.path().to_str().unwrap()).unwrap();

        assert_eq!(
            result.networks,
            vec![
                "10.244.0.0/16".parse::<Ipv4Network>().unwrap(),
                "192.168.0.0/24".parse::<Ipv4Network>().unwrap()
            ]
        );
        assert_eq!(result.subnet, Some("10.244.1.0/24".parse().unwrap()));
        assert_eq!(result.mtu, Some(1450));
        assert_eq!(result.ipmasq, Some(true));
    }

    #[test]
    fn test_load_flannel_subnet_env_with_spaces_and_comments() {
        let content = r#"
            # Flannel configuration
            FLANNEL_NETWORK = 10.244.0.0/16
            FLANNEL_SUBNET = 10.244.1.0/24  
            # MTU setting
            FLANNEL_MTU = 1500
        "#;
        let file = create_temp_file(content);
        let result = load_flannel_subnet_env(file.path().to_str().unwrap()).unwrap();

        assert_eq!(result.networks, vec!["10.244.0.0/16".parse().unwrap()]);
        assert_eq!(result.subnet, Some("10.244.1.0/24".parse().unwrap()));
        assert_eq!(result.mtu, Some(1500));
    }

    #[test]
    fn test_save_and_consume_scratch_net_conf() {
        let tmp_dir = tempdir().expect("failed to create temp dir");
        let cid = "test-container-123";
        let data_dir = tmp_dir.path().to_str().unwrap();
        let content = br#"{"cniVersion":"0.4.0","name":"testnet"}"#;

        // Save
        save_scratch_net_conf(cid, data_dir, content).expect("failed to save netconf");

        // Consume
        let (cleanup, bytes) =
            consume_scratch_net_conf(cid, data_dir).expect("failed to consume netconf");

        assert_eq!(bytes, content);

        // Simulate no error so cleanup will run and delete file
        let fake_err = anyhow::anyhow!("no error");
        cleanup(&fake_err);

        // Check file was deleted
        let path = build_cache_path(cid, data_dir)
            .unwrap()
            .join(format!("{}.json", cid));
        assert!(!path.exists(), "file should be cleaned up");
    }

    #[test]
    fn test_build_cache_path_valid_and_invalid() {
        let data_dir = "/tmp/data";

        // Valid
        let valid = build_cache_path("abc_123-DEF", data_dir);
        assert!(valid.is_ok());
        assert_eq!(
            valid.unwrap(),
            Path::new(data_dir).join("results").join("abc_123-DEF")
        );

        // Invalid
        let invalid = build_cache_path("abc/123", data_dir);
        assert!(invalid.is_err());
    }

    #[test]
    fn test_cleanup_only_on_error() {
        let tmp_dir = tempdir().expect("failed to create temp dir");
        let cid = "test-cleanup";
        let data_dir = tmp_dir.path().to_str().unwrap();
        let content = b"test-cleanup-content";

        // Save once
        save_scratch_net_conf(cid, data_dir, content).expect("save failed");

        // 第一次测试：I/O 错误，不应该删除文件
        {
            let (cleanup, _) = consume_scratch_net_conf(cid, data_dir).expect("consume failed");
            let path = build_cache_path(cid, data_dir)
                .unwrap()
                .join(format!("{}.json", cid));
            assert!(path.exists());

            let io_err = anyhow::anyhow!(std::io::Error::new(std::io::ErrorKind::Other, "dummy"));
            cleanup(&io_err);
            assert!(path.exists(), "file should remain on I/O error");
        }

        {
            let (cleanup, _) =
                consume_scratch_net_conf(cid, data_dir).expect("consume failed again");
            let path = build_cache_path(cid, data_dir)
                .unwrap()
                .join(format!("{}.json", cid));
            assert!(path.exists());

            let logic_err = anyhow::anyhow!("some logical error");
            cleanup(&logic_err);
            assert!(!path.exists(), "file should be deleted on non-I/O error");
        }
    }
}
