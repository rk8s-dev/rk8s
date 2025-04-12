use std::io::{Read, Write};
use std::os::unix::fs::OpenOptionsExt;
use std::path::Path;

pub const IPV4_FORWARD: &str = "net.ipv4.ip_forward";
pub const IPV6_FORWARD: &str = "net.ipv6.conf.all.forwarding";

pub fn sysctl_get<T: AsRef<str>>(name: T) -> anyhow::Result<String> {
    let name = name.as_ref();
    let base_path: &Path = "/proc/sys".as_ref();
    let full_name = base_path.join(normalize_sysctl_name(name));
    let mut file = std::fs::OpenOptions::new().read(true).open(full_name)?;
    let mut value = String::new();
    file.read_to_string(&mut value)?;
    Ok(value.trim().to_string())
}

pub fn sysctl_set<T: AsRef<str>>(name: T, value: T) -> anyhow::Result<()> {
    let name = name.as_ref();
    let value = value.as_ref();

    let base_path: &Path = "/proc/sys".as_ref();
    let full_name = base_path.join(normalize_sysctl_name(name));
    let mut file = std::fs::OpenOptions::new()
        .write(true)
        .create(true)
        .truncate(true)
        .mode(0o644)
        .open(full_name)?;
    file.write_all(value.as_bytes())?;
    file.flush()?;
    Ok(())
}

#[inline]
fn normalize_sysctl_name(name: &str) -> String {
    name.replace('.', "/")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_sysctl_set() {
        sysctl_set(IPV4_FORWARD, "1").unwrap();
        assert_eq!(sysctl_get(IPV4_FORWARD).unwrap(), "1");
        sysctl_set(IPV4_FORWARD, "0").unwrap();
        assert_eq!(sysctl_get(IPV4_FORWARD).unwrap(), "0");
    }
}
