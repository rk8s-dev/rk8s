use std::fs::{DirBuilder, File, OpenOptions};
use std::io::Write;
use std::net::IpAddr;
use std::os::unix::fs::{DirBuilderExt, OpenOptionsExt};
use std::path::PathBuf;

use anyhow::Context;
use nix::fcntl::{Flock, FlockArg};

const LINE_BREAK: &str = "\r\n";
const LAST_IPFILE_PREFIX: &str = "last_reserved_ip_";

pub struct Store {
    path: PathBuf,
}

impl Store {
    pub fn new(data_dir: Option<String>) -> anyhow::Result<Self> {
        let data_dir = data_dir.unwrap_or("/var/lib/cni/networks".into());
        DirBuilder::new()
            .recursive(true)
            .mode(0o755)
            .create(&data_dir)?;
        let path = PathBuf::from(data_dir);
        Ok(Store { path })
    }

    // 根据容器 ID 和接口名查找已分配的 IP
    pub fn get_by_id(&self, id: &str, ifname: &str) -> anyhow::Result<Vec<IpAddr>> {
        let text_match = format!("{}{}{}", id, LINE_BREAK, ifname);
        let mut result = Vec::new();

        // 遍历目录下的所有文件
        for entry in std::fs::read_dir(&self.path)? {
            let entry = entry?;
            let path = entry.path();

            // 跳过非文件和非 IP 文件
            if !entry.metadata()?.is_file() {
                continue;
            }
            if path
                .file_name()
                .and_then(|n| n.to_str())
                .map(|n| n.starts_with(LAST_IPFILE_PREFIX))
                .unwrap_or(false)
            {
                continue; // 跳过 last_reserved_ip_ 文件
            }

            // 读取文件内容并匹配
            let data = std::fs::read_to_string(&path)
                .with_context(|| format!("Failed to read {}", path.display()))?;
            if data.trim() == text_match {
                let ip_str = path
                    .file_name()
                    .and_then(|n| n.to_str())
                    .ok_or_else(|| anyhow::anyhow!("Invalid filename"))?;
                let ip = ip_str.parse::<IpAddr>()?;
                result.push(ip);
            }
        }
        Ok(result)
    }

    // 分配指定 IP
    pub fn reserve(
        &self,
        id: &str,
        ifname: &str,
        ip: IpAddr,
        range_id: &str,
    ) -> anyhow::Result<bool> {
        let _lock = self.new_lock()?; // 加锁确保原子性

        let file_path = self.path.join(ip.to_string());
        if file_path.exists() {
            return Ok(false); // IP 已被占用
        }

        // 创建 IP 文件并写入内容
        let mut f = OpenOptions::new()
            .write(true)
            .create_new(true)
            .mode(0o600) // 设置文件权限为 rw-------
            .open(&file_path)?;
        let content = format!("{}{}{}", id, LINE_BREAK, ifname);
        f.write_all(content.as_bytes())?;

        // 更新 last_reserved_ip 记录
        let last_ip_file = self
            .path
            .join(format!("{}{}", LAST_IPFILE_PREFIX, range_id));
        std::fs::write(last_ip_file, ip.to_string())?;

        Ok(true)
    }

    // 获取指定范围最后分配的 IP
    pub fn last_reserved_ip(&self, range_id: &str) -> Option<IpAddr> {
        let last_ip_file = self
            .path
            .join(format!("{}{}", LAST_IPFILE_PREFIX, range_id));
        std::fs::read_to_string(last_ip_file)
            .ok()
            .and_then(|s| s.parse().ok())
    }

    // 根据容器 ID 和接口名释放 IP
    pub fn release_by_id(&self, id: &str, ifname: &str) -> anyhow::Result<bool> {
        let _lock = self.new_lock()?; // 加锁确保原子性
        let text_match = format!("{}{}{}", id, LINE_BREAK, ifname);
        let mut found = false;

        for entry in std::fs::read_dir(&self.path)? {
            let entry = entry?;
            let path = entry.path();

            if !entry.metadata()?.is_file() {
                continue;
            }

            // 跳过 last_reserved_ip_ 文件
            if path
                .file_name()
                .and_then(|n| n.to_str())
                .map(|n| n.starts_with(LAST_IPFILE_PREFIX))
                .unwrap_or(false)
            {
                continue;
            }

            // 读取文件内容并匹配
            let data = std::fs::read_to_string(&path)
                .with_context(|| format!("Failed to read {}", path.display()))?;
            if data.trim() == text_match {
                std::fs::remove_file(&path)?;
                found = true;
            }
        }

        Ok(found)
    }
}

// 文件锁扩展 trait
pub trait FileLockExt {
    fn new_lock(&self) -> anyhow::Result<Flock<File>>;
}

impl FileLockExt for Store {
    fn new_lock(&self) -> anyhow::Result<Flock<File>> {
        let lock_file = self.path.join("ipam.lock"); // 使用专用锁文件

        // 打开（或创建）锁文件
        let file = OpenOptions::new()
            .read(true)
            .write(true)
            .create(true)
            .truncate(true)
            .mode(0o600)
            .open(lock_file)?;

        // 尝试获取非阻塞排他锁
        match Flock::lock(file, FlockArg::LockExclusiveNonblock) {
            Ok(lock) => Ok(lock),
            Err((_file, e)) => Err(anyhow::anyhow!("Lock failed: {}", e)),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::net::Ipv4Addr;

    #[test]
    fn test_exclusive_lock() {
        let temp_dir = tempfile::tempdir().unwrap();
        let store = Store::new(Some(temp_dir.path().to_str().unwrap().into())).unwrap();

        // 第一次加锁应成功
        let lock1 = store.new_lock().expect("First lock should succeed");

        // 第二次加锁应失败（非阻塞模式）
        let lock2 = store.new_lock();
        assert!(lock2.is_err(), "Second lock should fail");

        // 释放锁后可以重新获取
        drop(lock1);
        let lock3 = store.new_lock().expect("Third lock should succeed");
        drop(lock3);
    }

    #[test]
    fn test_reserve_and_release() -> anyhow::Result<()> {
        let temp_dir = tempfile::tempdir()?;
        let store = Store::new(Some(temp_dir.path().to_str().unwrap().into()))?;
        let ip = IpAddr::V4(Ipv4Addr::new(192, 168, 1, 10));

        // 分配 IP
        assert!(store.reserve("container1", "eth0", ip, "range1")?);
        assert!(!store.reserve("container2", "eth0", ip, "range1")?); // 已被占用

        // 验证分配
        let ips = store.get_by_id("container1", "eth0")?;
        assert_eq!(ips, vec![ip]);

        // 释放 IP
        assert!(store.release_by_id("container1", "eth0")?);
        assert!(!store.release_by_id("container1", "eth0")?); // 已释放

        Ok(())
    }
}
