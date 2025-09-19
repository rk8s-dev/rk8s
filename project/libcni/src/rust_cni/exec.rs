// Copyright (c) 2024 https://github.com/divinerapier/cni-rs
use log::{debug, error, trace, warn};
use serde::{Deserialize, Serialize};

use crate::rust_cni::error::CNIError;
use crate::rust_cni::result::ResultCNI;
use std::path::Path;
use std::process::{Command, Stdio};
use std::{collections::HashMap, io::Write};

#[derive(Default, Serialize, Deserialize, Debug)]
pub struct ExecArgs {
    pub(crate) command: String,
    pub(crate) containerd_id: String,
    pub(crate) netns: String,
    pub(crate) plugin_args: Vec<[String; 2]>,
    pub(crate) plugin_args_str: String,
    pub(crate) ifname: String,
    pub(crate) path: String,
}

impl ExecArgs {
    pub fn to_env(&self) -> Vec<String> {
        debug!("Preparing environment for CNI execution , args :{self:?}");
        let mut result_env = Vec::default();

        // Set environment variables
        unsafe {
            std::env::set_var("CNI_COMMAND", self.command.clone());
            std::env::set_var("CNI_CONTAINERID", self.containerd_id.clone());
            std::env::set_var("CNI_NETNS", self.netns.clone());
            std::env::set_var("CNI_ARGS", self.plugin_args_str.clone());
            std::env::set_var("CNI_IFNAME", self.ifname.clone());
            std::env::set_var("CNI_PATH", self.path.clone());
        }

        // Collect all environment variables
        for (k, v) in std::env::vars() {
            result_env.push(format!("{k}={v}"));
        }

        trace!(
            "CNI environment prepared with {} variables",
            result_env.len()
        );
        result_env
    }
}

pub trait Exec {
    fn exec_plugins(
        &self,
        plugin_path: String,
        stdin_data: &[u8],
        environ: Vec<String>,
    ) -> ResultCNI<Vec<u8>>;

    fn find_in_path(&self, plugin: String, paths: Vec<String>) -> ResultCNI<String>;

    fn decode(&self, data: &[u8]) -> ResultCNI<()>;
}

#[derive(Default)]
pub struct RawExec {}

impl Exec for RawExec {
    fn exec_plugins(
        &self,
        plugin_path: String,
        stdin_data: &[u8],
        environ: Vec<String>,
    ) -> ResultCNI<Vec<u8>> {
        debug!("Executing CNI plugin: {plugin_path}");
        trace!("CNI stdin data: {}", String::from_utf8_lossy(stdin_data));

        // Parse environment variables
        let envs: HashMap<String, String> = environ
            .iter()
            .filter_map(|env_var| {
                let parts: Vec<&str> = env_var.splitn(2, '=').collect();
                if parts.len() == 2 {
                    Some((parts[0].to_string(), parts[1].to_string()))
                } else {
                    None
                }
            })
            .collect();
        // debug!("CNI environment variables: {:?}", envs);
        // Check if plugin exists
        if !Path::new(&plugin_path).exists() {
            let err_msg = format!("CNI plugin not found: {plugin_path}");
            return Err(Box::new(CNIError::ExecuteError(err_msg)));
        }

        // Start the plugin process
        let mut plugin_cmd = match Command::new(&plugin_path)
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .envs(envs)
            .spawn()
        {
            Ok(cmd) => cmd,
            Err(e) => {
                let err_msg = format!("Failed to start CNI plugin {plugin_path}: {e}");
                return Err(Box::new(CNIError::ExecuteError(err_msg)));
            }
        };
        debug!("cni stdin is: {:?}", String::from_utf8_lossy(stdin_data));
        // Write stdin data
        if let Some(mut stdin) = plugin_cmd.stdin.take() {
            if let Err(e) = stdin.write_all(stdin_data) {
                let err_msg = format!("Failed to write to plugin stdin: {e}");
                return Err(Box::new(CNIError::ExecuteError(err_msg)));
            }
            // Close stdin to signal end of input
            drop(stdin);
        }

        // Wait for command to complete and get output
        let output = match plugin_cmd.wait_with_output() {
            Ok(output) => output,
            Err(e) => {
                let err_msg = format!("Failed to get plugin output: {e}");
                return Err(Box::new(CNIError::ExecuteError(err_msg)));
            }
        };

        // Check for errors in stderr
        if !output.stderr.is_empty() {
            let stderr = String::from_utf8_lossy(&output.stderr);
            warn!("CNI plugin stderr: {stderr}");
        }

        // Check for error in stdout (CNI returns errors in JSON format)
        if let Ok(json_value) = serde_json::from_slice::<serde_json::Value>(&output.stdout) {
            if let Some(error_code) = json_value.get("code") {
                if error_code.as_u64().is_some() {
                    let msg = String::from_utf8_lossy(&output.stdout).to_string();
                    return Err(Box::new(CNIError::ExecuteError(msg)));
                }
            }
        }

        debug!("CNI plugin execution successful");
        Ok(output.stdout)
    }

    fn find_in_path(&self, plugin: String, paths: Vec<String>) -> ResultCNI<String> {
        trace!("Finding CNI plugin {plugin} in paths");

        if paths.is_empty() {
            let err_msg = format!("No plugin paths provided for {plugin}");
            error!("{err_msg}");
            return Err(Box::new(CNIError::Config(err_msg)));
        }

        for path in &paths {
            let full_path = format!("{path}/{plugin}");
            let plugin_path = Path::new(&full_path);

            if plugin_path.exists() {
                debug!("Found CNI plugin at: {full_path}");
                return Ok(full_path);
            }
        }

        let err_msg = format!("CNI plugin {plugin} not found in paths {paths:?}");
        error!("{err_msg}");
        Err(Box::new(CNIError::NotFound(plugin, paths.join(":"))))
    }

    fn decode(&self, data: &[u8]) -> ResultCNI<()> {
        trace!("Decoding CNI data: {} bytes", data.len());

        // Simple validation of JSON structure
        match serde_json::from_slice::<serde_json::Value>(data) {
            Ok(_) => {
                trace!("CNI data successfully decoded");
                Ok(())
            }
            Err(e) => {
                let err_msg = format!("Failed to decode CNI data: {e}");
                error!("{err_msg}");
                Err(Box::new(CNIError::VarDecode(
                    "Invalid JSON format".to_string(),
                )))
            }
        }
    }
}
