use std::collections::HashMap;
use std::sync::{Arc, RwLock};
use std::time::{Duration, Instant};

#[derive(Debug)]
pub struct GangEntry {
    pub size: u32,
    pub assumed: HashMap<String, String>,
    pub created_at: Instant,
}

#[derive(Default, Clone)]
pub struct GangStateStore {
    inner: Arc<RwLock<HashMap<String, GangEntry>>>,
}

impl GangStateStore {
    pub fn add_member(&self, gang_id: &str, size: u32, pod: &str, node: &str) -> bool {
        let mut g = self.inner.write().unwrap();
        let entry = g.entry(gang_id.to_string()).or_insert_with(|| GangEntry {
            size,
            assumed: HashMap::new(),
            created_at: Instant::now(),
        });
        entry.assumed.insert(pod.to_string(), node.to_string());
        entry.assumed.len() as u32 >= entry.size
    }

    pub fn take_and_clear(&self, gang_id: &str) -> Option<HashMap<String, String>> {
        let mut g = self.inner.write().unwrap();
        g.remove(gang_id).map(|e| e.assumed)
    }

    pub fn assumed_nodes(&self, gang_id: &str) -> Vec<String> {
        let g = self.inner.read().unwrap();
        g.get(gang_id)
            .map(|e| e.assumed.values().cloned().collect())
            .unwrap_or_default()
    }

    pub fn collect_timed_out(&self, timeout: Duration) -> Vec<String> {
        let g = self.inner.read().unwrap();
        let now = Instant::now();
        g.iter()
            .filter(|(_, e)| {
                (e.assumed.len() as u32) < e.size && now.duration_since(e.created_at) > timeout
            })
            .map(|(k, _)| k.clone())
            .collect()
    }

    #[cfg(test)]
    pub fn set_created_at_for_test(&self, gang_id: &str, ts: Instant) {
        let mut g = self.inner.write().unwrap();
        if let Some(e) = g.get_mut(gang_id) {
            e.created_at = ts;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn add_member_until_full_returns_true_on_last() {
        let store = GangStateStore::default();
        assert!(!store.add_member("g1", 3, "p1", "n1"));
        assert!(!store.add_member("g1", 3, "p2", "n1"));
        assert!(store.add_member("g1", 3, "p3", "n1"));
    }

    #[test]
    fn take_and_clear_returns_all_members() {
        let store = GangStateStore::default();
        store.add_member("g1", 2, "p1", "n1");
        store.add_member("g1", 2, "p2", "n2");
        let members = store.take_and_clear("g1");
        assert_eq!(members.unwrap().len(), 2);
        assert!(store.take_and_clear("g1").is_none());
    }

    #[test]
    fn collect_timed_out_returns_only_old_entries() {
        let store = GangStateStore::default();
        store.add_member("g_old", 4, "p1", "n1");
        store.set_created_at_for_test("g_old", Instant::now() - Duration::from_secs(1000));
        store.add_member("g_new", 4, "p1", "n1");
        let timed_out = store.collect_timed_out(Duration::from_secs(60));
        assert_eq!(timed_out, vec!["g_old".to_string()]);
    }

    #[test]
    fn assumed_nodes_returns_assumed_node_names() {
        let store = GangStateStore::default();
        store.add_member("g1", 4, "p1", "node-a");
        store.add_member("g1", 4, "p2", "node-b");
        let neighbors = store.assumed_nodes("g1");
        assert_eq!(neighbors.len(), 2);
        assert!(neighbors.iter().any(|n| n == "node-a"));
        assert!(neighbors.iter().any(|n| n == "node-b"));
    }
}
