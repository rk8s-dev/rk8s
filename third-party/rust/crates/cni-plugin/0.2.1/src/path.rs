use std::{convert::Infallible, env::split_paths, path::PathBuf, str::FromStr};

#[derive(Clone, Debug, Default)]
pub(crate) struct CniPath(pub Vec<PathBuf>);

impl FromStr for CniPath {
	type Err = Infallible;

	fn from_str(s: &str) -> Result<Self, Self::Err> {
		Ok(Self(split_paths(s).map(PathBuf::from).collect()))
	}
}
