//! Serialization and Deserialization implementation

use std::collections::{HashMap, HashSet};

use ::serde::{Deserialize, Deserializer, Serialize, Serializer};

use super::*;

impl<P: Prefix + Serialize + Eq + std::hash::Hash, T: Serialize> Serialize for PrefixMap<P, T> {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        let map: HashMap<&P, &T> = HashMap::from_iter(self);
        map.serialize(serializer)
    }
}

impl<P: Prefix + Serialize + Eq + std::hash::Hash> Serialize for PrefixSet<P> {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        let set: HashSet<&P> = HashSet::from_iter(self);
        set.serialize(serializer)
    }
}

impl<'de, P: Prefix + Deserialize<'de> + Eq + std::hash::Hash, T: Deserialize<'de>> Deserialize<'de>
    for PrefixMap<P, T>
{
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        let map: HashMap<P, T> = HashMap::deserialize(deserializer)?;
        Ok(Self::from_iter(map))
    }
}

impl<'de, P: Prefix + Deserialize<'de> + Eq + std::hash::Hash> Deserialize<'de> for PrefixSet<P> {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        let set: HashSet<P> = HashSet::deserialize(deserializer)?;
        Ok(Self::from_iter(set))
    }
}
