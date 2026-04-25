use anyhow::{Context, Result, anyhow};
use chrono::{DateTime, Utc};
use std::fs;
use std::path::{Path, PathBuf};

#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct InstanceRecord {
    pub pid: u32,
    pub mount_point: String,
    pub socket_path: PathBuf,
    pub started_at: DateTime<Utc>,
}

impl InstanceRecord {
    pub fn new(
        pid: u32,
        mount_point: String,
        socket_path: PathBuf,
        started_at: DateTime<Utc>,
    ) -> Self {
        Self {
            pid,
            mount_point,
            socket_path,
            started_at,
        }
    }
}

#[derive(Debug, Clone)]
pub struct RuntimeRegistry {
    root: PathBuf,
}

impl RuntimeRegistry {
    pub fn new(root: PathBuf) -> Self {
        Self { root }
    }

    pub fn default_root() -> PathBuf {
        dirs::runtime_dir()
            .unwrap_or_else(std::env::temp_dir)
            .join("slayerfs")
    }

    pub fn socket_path(&self, pid: u32) -> PathBuf {
        self.root.join(format!("{pid}.sock"))
    }

    pub fn record_path(&self, pid: u32) -> PathBuf {
        self.root.join(format!("{pid}.json"))
    }

    pub async fn write_record(&self, record: &InstanceRecord) -> Result<()> {
        tokio::fs::create_dir_all(&self.root).await?;
        let data = serde_json::to_vec_pretty(record)?;
        tokio::fs::write(self.record_path(record.pid), data).await?;
        Ok(())
    }

    #[allow(dead_code)]
    pub async fn remove_record(&self, pid: u32) -> Result<()> {
        let record_path = self.record_path(pid);
        let socket_path = self.socket_path(pid);

        if record_path.exists() {
            tokio::fs::remove_file(&record_path).await?;
        }

        if socket_path.exists() {
            tokio::fs::remove_file(&socket_path).await?;
        }

        Ok(())
    }

    pub async fn select_instance(&self, mount_point: Option<&str>) -> Result<InstanceRecord> {
        tokio::fs::create_dir_all(&self.root).await?;

        let mut records = Vec::new();
        let mut dir = tokio::fs::read_dir(&self.root).await?;

        while let Some(entry) = dir.next_entry().await? {
            let path = entry.path();
            if path.extension().and_then(|e| e.to_str()) != Some("json") {
                continue;
            }

            match self.load_record(&path) {
                Ok(Some(record)) => records.push(record),
                Ok(None) => {}
                Err(_) => {}
            }
        }

        if let Some(mount_point) = mount_point {
            return records
                .into_iter()
                .find(|record| record.mount_point == mount_point)
                .ok_or_else(|| anyhow!("no slayerfs instance for mount point: {mount_point}"));
        }

        match records.len() {
            0 => Err(anyhow!("no slayerfs instances found")),
            1 => Ok(records.pop().expect("single record")),
            _ => Err(anyhow!(
                "multiple slayerfs instances found, please specify mount point"
            )),
        }
    }

    fn load_record(&self, path: &Path) -> Result<Option<InstanceRecord>> {
        let raw = fs::read(path).with_context(|| format!("read {}", path.display()))?;
        let record: InstanceRecord = serde_json::from_slice(&raw)?;

        if pid_is_alive(record.pid) {
            return Ok(Some(record));
        }

        let _ = fs::remove_file(path);
        let _ = fs::remove_file(&record.socket_path);
        Ok(None)
    }
}

fn pid_is_alive(pid: u32) -> bool {
    #[cfg(unix)]
    {
        let rc = unsafe { libc::kill(pid as i32, 0) };
        rc == 0 || std::io::Error::last_os_error().raw_os_error() == Some(libc::EPERM)
    }

    #[cfg(not(unix))]
    {
        let _ = pid;
        true
    }
}
