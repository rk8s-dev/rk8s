use serde::Deserialize;
use std::fmt::{Display, Formatter};
use std::str::FromStr;

#[derive(Debug, Clone)]
pub enum Visibility {
    Public,
    Private,
}

impl FromStr for Visibility {
    type Err = String;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s.trim().to_lowercase().as_str() {
            "public" => Ok(Visibility::Public),
            "private" => Ok(Visibility::Private),
            _ => Err("visibility must be `public` or `private`".to_string()),
        }
    }
}

impl Display for Visibility {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        match self {
            Visibility::Public => write!(f, "public"),
            Visibility::Private => write!(f, "private"),
        }
    }
}

#[derive(Deserialize)]
pub struct ListRepoResponse {
    pub data: Vec<RepoView>,
}

#[derive(Deserialize)]
pub struct RepoView {
    pub namespace: String,
    pub name: String,
    pub is_public: bool,
}
