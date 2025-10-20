use itertools::Itertools;
use std::{fs, str::FromStr};

/// Represents a single UID or GID map entry.
///
/// Format: `host_id:to_id:len`
#[derive(Debug, Clone, Default, PartialEq, Eq, Hash)]
pub struct IdMapEntry {
    pub host: u32,
    pub to: u32,
    pub len: u32,
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Hash)]
pub struct IdMappings {
    pub uid_map: Vec<IdMapEntry>,
    pub gid_map: Vec<IdMapEntry>,

    /// Fallback UID used when no mapping is found.
    /// Typically read from `/proc/sys/kernel/overflowuid`.
    overflow_uid: u32,
    /// Fallback GID used when no mapping is found.
    /// Typically read from `/proc/sys/kernel/overflowgid`.
    overflow_gid: u32,
}

impl IdMappings {
    pub fn new(uid_map: Vec<IdMapEntry>, gid_map: Vec<IdMapEntry>) -> Self {
        let overflow_uid = fs::read_to_string("/proc/sys/kernel/overflowuid")
            .ok()
            .and_then(|s| s.trim().parse().ok())
            .unwrap_or_default();
        let overflow_gid = fs::read_to_string("/proc/sys/kernel/overflowgid")
            .ok()
            .and_then(|s| s.trim().parse().ok())
            .unwrap_or_default();
        IdMappings {
            uid_map,
            gid_map,
            overflow_uid,
            overflow_gid,
        }
    }

    /// Parses a colon-separated string in the format `host:to:len[:host2:to2:len2...]`
    /// into a vector of `IdMapEntry` structs.
    ///
    /// # Arguments
    ///
    /// * `mapping` - A string slice containing the mapping(s) in the format `host:to:len[:host2:to2:len2...]`.
    ///
    /// # Returns
    ///
    /// * `Ok(Vec<IdMapEntry>)` if the input string is valid and successfully parsed.
    /// * `Err(String)` if the format is invalid (e.g., the number of fields is not a multiple of 3, or parsing fails).
    fn read_mappings(mapping: &str) -> Result<Vec<IdMapEntry>, String> {
        let parts: Vec<&str> = mapping.split(':').collect();
        if !parts.len().is_multiple_of(3) {
            return Err(format!(
                "Invalid mapping specified: '{mapping}'. The number of fields must be a multiple of 3.",
            ));
        }
        parts
            .into_iter()
            .tuples()
            .map(|(host, to, len)| {
                let host: u32 = host
                    .parse()
                    .map_err(|e| format!("Invalid host id in mapping: {e}"))?;
                let to: u32 = to
                    .parse()
                    .map_err(|e| format!("Invalid to id in mapping: {e}"))?;
                let len: u32 = len
                    .parse()
                    .map_err(|e| format!("Invalid length in mapping: {e}"))?;
                if len == 0 {
                    return Err("Length in mapping cannot be zero".to_string());
                }
                Ok(IdMapEntry { host, to, len })
            })
            .collect::<Result<Vec<_>, _>>()
    }

    /// Finds the mapped ID based on the provided mappings.
    ///
    /// If no mapping is found, returns the original ID.
    ///
    /// - `direct` is `true`: Reverse mapping (Host -> Container).
    /// - `direct` is `false`: Forward mapping (Container -> Host).
    pub fn find_mapping(&self, id: u32, direct: bool, uid: bool) -> u32 {
        let map = if uid { &self.uid_map } else { &self.gid_map };
        if map.is_empty() {
            return id;
        }
        for entry in map {
            if direct {
                // Reverse mapping: check if id is in host range
                if id >= entry.host && id < entry.host + entry.len {
                    return entry.to + (id - entry.host);
                }
            } else {
                // Forward mapping: check if id is in container range
                if id >= entry.to && id < entry.to + entry.len {
                    return entry.host + (id - entry.to);
                }
            }
        }

        if uid {
            self.overflow_uid
        } else {
            self.overflow_gid
        }
    }

    /// Gets the host UID from a container UID (Forward mapping).
    pub fn get_uid(&self, container_id: u32) -> u32 {
        self.find_mapping(container_id, false, true)
    }

    /// Gets the host GID from a container GID (Forward mapping).
    pub fn get_gid(&self, container_id: u32) -> u32 {
        self.find_mapping(container_id, false, false)
    }
}

impl FromStr for IdMappings {
    type Err = String;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        let parts: Vec<&str> = s.split(',').collect();
        if parts.len() != 2 {
            return Err("Invalid mapping format. Expected 'uidmapping=,gidmapping='".to_string());
        }
        let uid_mappings_str = parts[0]
            .strip_prefix("uidmapping=")
            .ok_or_else(|| "Invalid uidmapping format: missing 'uidmapping=' prefix".to_string())?;

        let gid_mappings_str = parts[1]
            .strip_prefix("gidmapping=")
            .ok_or_else(|| "Invalid gidmapping format: missing 'gidmapping=' prefix".to_string())?;

        let uid_map = IdMappings::read_mappings(uid_mappings_str)?;
        let gid_map = IdMappings::read_mappings(gid_mappings_str)?;
        Ok(IdMappings::new(uid_map, gid_map))
    }
}

#[cfg(test)]
mod tests {
    use nix::unistd::{getgid, getuid};

    use crate::util::mapping::IdMappings;

    #[test]
    fn test_parse_mappings() {
        let cur_uid = getuid().as_raw();
        let cur_gid = getgid().as_raw();
        let mapping = format!("uidmapping={cur_uid}:1000:1,gidmapping={cur_gid}:1000:1");
        let id_mappings: IdMappings = mapping.parse().unwrap();
        assert_eq!(id_mappings.uid_map.len(), 1);
        assert_eq!(id_mappings.gid_map.len(), 1);
        assert_eq!(id_mappings.uid_map[0].host, cur_uid);
        assert_eq!(id_mappings.uid_map[0].to, 1000);
        assert_eq!(id_mappings.uid_map[0].len, 1);
        assert_eq!(id_mappings.gid_map[0].host, cur_gid);
        assert_eq!(id_mappings.gid_map[0].to, 1000);
        assert_eq!(id_mappings.gid_map[0].len, 1);

        let mapping =
            format!("uidmapping={cur_uid}:1000:1:0:65534:1,gidmapping={cur_gid}:1000:1:0:65534:1");
        let id_mappings: IdMappings = mapping.parse().unwrap();
        assert_eq!(id_mappings.uid_map.len(), 2);
        assert_eq!(id_mappings.gid_map.len(), 2);
        assert_eq!(id_mappings.uid_map[0].host, cur_uid);
        assert_eq!(id_mappings.uid_map[0].to, 1000);
        assert_eq!(id_mappings.uid_map[0].len, 1);
        assert_eq!(id_mappings.uid_map[1].host, 0);
        assert_eq!(id_mappings.uid_map[1].to, 65534);
        assert_eq!(id_mappings.uid_map[1].len, 1);
        assert_eq!(id_mappings.gid_map[0].host, cur_gid);
        assert_eq!(id_mappings.gid_map[0].to, 1000);
        assert_eq!(id_mappings.gid_map[0].len, 1);
        assert_eq!(id_mappings.gid_map[1].host, 0);
        assert_eq!(id_mappings.gid_map[1].to, 65534);
        assert_eq!(id_mappings.gid_map[1].len, 1);
    }
}
