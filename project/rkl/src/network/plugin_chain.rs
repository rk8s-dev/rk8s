use anyhow::{Context, Result, anyhow};
use cni_plugin::{Command, Inputs, config::NetworkConfig, reply::SuccessReply};
use json::JsonValue;
use libcni::rust_cni::config::ConfigFile;
use std::path::PathBuf;

const DEFAULT_CNI_CONF_DIR: &str = "/etc/cni/net.d";
const DEFAULT_IFNAME: &str = "vethcni0";

pub async fn setup_network_async(container_id: &str, netns_path: &str) -> Result<JsonValue> {
    let top_config = load_primary_plugin_config(DEFAULT_CNI_CONF_DIR)?;

    let reply = match top_config.plugin.as_str() {
        "libnetwork" => {
            let flannel_conf = libnetwork::cni_chain::load_flannel_net_conf(top_config)
                .map_err(|e| anyhow!("failed to load libnetwork config: {e}"))?;
            let delegate_config =
                libnetwork::cni_chain::build_delegate_add_config(flannel_conf, container_id)
                    .map_err(|e| anyhow!("failed to build delegate bridge config: {e}"))?;

            let inputs = build_inputs(
                Command::Add,
                container_id,
                netns_path,
                DEFAULT_IFNAME,
                delegate_config.clone(),
            );

            run_libbridge_add(delegate_config, inputs).await?
        }
        "libbridge" => {
            let inputs = build_inputs(
                Command::Add,
                container_id,
                netns_path,
                DEFAULT_IFNAME,
                top_config.clone(),
            );

            run_libbridge_add(top_config, inputs).await?
        }
        plugin => {
            return Err(anyhow!(
                "unsupported top-level CNI plugin '{plugin}' for direct library chaining"
            ));
        }
    };

    success_reply_to_json(&reply)
}

pub async fn remove_network_async(container_id: &str, netns_path: &str) -> Result<()> {
    let top_config = load_primary_plugin_config(DEFAULT_CNI_CONF_DIR)?;

    match top_config.plugin.as_str() {
        "libnetwork" => {
            let flannel_conf = libnetwork::cni_chain::load_flannel_net_conf(top_config)
                .map_err(|e| anyhow!("failed to load libnetwork config: {e}"))?;

            if let Some(delegate_config) =
                libnetwork::cni_chain::build_delegate_del_config(flannel_conf, container_id)
                    .map_err(|e| anyhow!("failed to load delegate config for del: {e}"))?
            {
                let inputs = build_inputs(
                    Command::Del,
                    container_id,
                    netns_path,
                    DEFAULT_IFNAME,
                    delegate_config.clone(),
                );

                run_libbridge_del(delegate_config, inputs).await?;
            }
        }
        "libbridge" => {
            let inputs = build_inputs(
                Command::Del,
                container_id,
                netns_path,
                DEFAULT_IFNAME,
                top_config.clone(),
            );

            run_libbridge_del(top_config, inputs).await?;
        }
        plugin => {
            return Err(anyhow!(
                "unsupported top-level CNI plugin '{plugin}' for direct library chaining"
            ));
        }
    }

    Ok(())
}

pub fn setup_network(container_id: &str, netns_path: &str) -> Result<JsonValue> {
    if tokio::runtime::Handle::try_current().is_ok() {
        return Err(anyhow!(
            "setup_network() called from async runtime; use setup_network_async()"
        ));
    }

    let rt = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .context("failed to create tokio runtime")?;
    rt.block_on(setup_network_async(container_id, netns_path))
}

pub fn remove_network(container_id: &str, netns_path: &str) -> Result<()> {
    if tokio::runtime::Handle::try_current().is_ok() {
        return Err(anyhow!(
            "remove_network() called from async runtime; use remove_network_async()"
        ));
    }

    let rt = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .context("failed to create tokio runtime")?;
    rt.block_on(remove_network_async(container_id, netns_path))
}

async fn run_libbridge_add(config: NetworkConfig, inputs: Inputs) -> Result<SuccessReply> {
    tokio::task::spawn_blocking(move || {
        let rt = tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()
            .map_err(|e| anyhow!("failed to create libbridge runtime: {e}"))?;
        rt.block_on(libnetwork::bridge::plugin::add_from_inputs(config, inputs))
            .map_err(|e| anyhow!("libbridge add failed: {e}"))
    })
    .await
    .map_err(|e| anyhow!("libbridge add join error: {e}"))?
}

async fn run_libbridge_del(config: NetworkConfig, inputs: Inputs) -> Result<SuccessReply> {
    tokio::task::spawn_blocking(move || {
        let rt = tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()
            .map_err(|e| anyhow!("failed to create libbridge runtime: {e}"))?;
        rt.block_on(libnetwork::bridge::plugin::del_from_inputs(config, inputs))
            .map_err(|e| anyhow!("libbridge del failed: {e}"))
    })
    .await
    .map_err(|e| anyhow!("libbridge del join error: {e}"))?
}

fn build_inputs(
    command: Command,
    container_id: &str,
    netns_path: &str,
    ifname: &str,
    config: NetworkConfig,
) -> Inputs {
    Inputs {
        command,
        container_id: container_id.to_string(),
        ifname: ifname.to_string(),
        netns: Some(PathBuf::from(netns_path)),
        path: Vec::new(),
        config,
    }
}

fn success_reply_to_json(reply: &SuccessReply) -> Result<JsonValue> {
    let payload = serde_json::to_string(reply).context("failed to serialize CNI success reply")?;
    let json = json::parse(&payload).map_err(|e| anyhow!("failed to parse CNI json reply: {e}"))?;
    Ok(json)
}

fn load_primary_plugin_config(conf_dir: &str) -> Result<NetworkConfig> {
    let mut files = ConfigFile::config_files(
        conf_dir.to_string(),
        vec![
            "conf".to_string(),
            "conflist".to_string(),
            "json".to_string(),
        ],
    )
    .map_err(|e| anyhow!("failed to list CNI config files: {e}"))?;

    if files.is_empty() {
        return Err(anyhow!("no CNI config files found in {conf_dir}"));
    }

    files.sort();
    let primary = files[0].clone();

    if primary.ends_with(".conflist") {
        let conf_list = ConfigFile::read_configlist_file(primary.clone())
            .ok_or_else(|| anyhow!("failed to parse CNI conflist: {primary}"))?;

        let plugin = conf_list
            .plugins
            .first()
            .ok_or_else(|| anyhow!("CNI conflist has no plugins: {primary}"))?;

        return decode_conflist_plugin_config(
            &plugin.bytes,
            &conf_list.cni_version,
            &conf_list.name,
            &primary,
        );
    }

    if primary.ends_with(".conf") || primary.ends_with(".json") {
        let conf = ConfigFile::read_config_file(primary.clone())
            .ok_or_else(|| anyhow!("failed to parse CNI config: {primary}"))?;

        return serde_json::from_slice(&conf.bytes)
            .map_err(|e| anyhow!("failed to decode CNI config from {primary}: {e}"));
    }

    Err(anyhow!("unsupported CNI config extension: {primary}"))
}

fn decode_conflist_plugin_config(
    plugin_bytes: &[u8],
    conflist_cni_version: &str,
    conflist_name: &str,
    source: &str,
) -> Result<NetworkConfig> {
    let mut plugin_value = serde_json::from_slice::<serde_json::Value>(plugin_bytes)
        .map_err(|e| anyhow!("failed to parse plugin json from {source}: {e}"))?;

    let plugin_object = plugin_value
        .as_object_mut()
        .ok_or_else(|| anyhow!("plugin entry in {source} must be a JSON object"))?;

    // CNI conflist plugins may omit top-level inherited fields.
    if !plugin_object.contains_key("cniVersion") {
        plugin_object.insert(
            "cniVersion".to_string(),
            serde_json::Value::String(conflist_cni_version.to_string()),
        );
    }
    if !plugin_object.contains_key("name") {
        plugin_object.insert(
            "name".to_string(),
            serde_json::Value::String(conflist_name.to_string()),
        );
    }

    serde_json::from_value(plugin_value)
        .map_err(|e| anyhow!("failed to decode plugin config from {source}: {e}"))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn decode_conflist_plugin_config_inherits_missing_fields() {
        let plugin = serde_json::json!({
            "type": "libbridge",
            "ipam": { "type": "libipam" }
        });
        let bytes = serde_json::to_vec(&plugin).expect("plugin json should serialize");

        let config = decode_conflist_plugin_config(&bytes, "0.4.0", "testnet", "test.conflist")
            .expect("plugin config should decode with inherited fields");

        assert_eq!(config.cni_version.to_string(), "0.4.0");
        assert_eq!(config.name, "testnet");
        assert_eq!(config.plugin, "libbridge");
    }

    #[test]
    fn decode_conflist_plugin_config_keeps_existing_fields() {
        let plugin = serde_json::json!({
            "cniVersion": "1.1.0",
            "name": "custom-net",
            "type": "libbridge"
        });
        let bytes = serde_json::to_vec(&plugin).expect("plugin json should serialize");

        let config = decode_conflist_plugin_config(&bytes, "0.4.0", "testnet", "test.conflist")
            .expect("plugin config should decode with explicit fields");

        assert_eq!(config.cni_version.to_string(), "1.1.0");
        assert_eq!(config.name, "custom-net");
        assert_eq!(config.plugin, "libbridge");
    }
}
