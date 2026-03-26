use crate::sandbox::protocol::GuestReadyEvent;
use anyhow::Result;
use async_trait::async_trait;
use serde::{Deserialize, Serialize};
use std::path::PathBuf;

pub mod firecracker;

pub use firecracker::{FirecrackerVmBackend, SandboxShimArgs, build_vm_spec, run_shim_command};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct VmInstanceSpec {
    pub sandbox_id: String,
    pub image: String,
    pub cpus: u32,
    pub memory_mib: u32,
    pub persistent: bool,
    pub kernel_path: Option<PathBuf>,
    pub initrd_path: Option<PathBuf>,
    pub guest_image_path: Option<PathBuf>,
    pub work_dir: PathBuf,
    pub ready_file: PathBuf,
    pub parent_pid: Option<u32>,
    pub boot_args: Option<String>,
    pub firecracker_api_socket: PathBuf,
    pub vsock_uds_path: PathBuf,
    pub guest_cid: u32,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct VmInstanceHandle {
    pub sandbox_id: String,
    pub vm_id: String,
    pub pid: Option<u32>,
    pub shim_pid: Option<u32>,
    pub control_socket: Option<PathBuf>,
    pub ready_file: PathBuf,
    pub work_dir: PathBuf,
    pub vmm_kind: VmmKind,
    pub vsock_uds_path: Option<PathBuf>,
}

#[async_trait]
pub trait VmBackend: Send + Sync {
    async fn boot(&self, spec: &VmInstanceSpec) -> Result<VmInstanceHandle>;
    async fn wait_ready(&self, handle: &VmInstanceHandle) -> Result<GuestReadyEvent>;
    async fn stop(&self, handle: &VmInstanceHandle) -> Result<()>;
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
pub enum VmmKind {
    Firecracker,
    // Libkrun,
}
