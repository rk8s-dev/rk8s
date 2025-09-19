#[derive(Clone)]
pub struct RepoIdentifier {
    pub namespace: String,
    pub name: String,
}

impl RepoIdentifier {
    pub fn new(namespace: impl Into<String>, name: impl Into<String>) -> RepoIdentifier {
        RepoIdentifier {
            namespace: namespace.into(),
            name: name.into(),
        }
    }

    pub fn full_name(&self) -> String {
        format!("{}/{}", self.namespace, self.name)
    }
}

pub fn identifier_from_full_name(full_name: impl AsRef<str>) -> RepoIdentifier {
    let full_name = full_name.as_ref();
    full_name
        .split_once("/")
        .map(|(namespace, name)| RepoIdentifier::new(namespace, name))
        .unwrap_or_else(|| RepoIdentifier::new(full_name, full_name))
}
