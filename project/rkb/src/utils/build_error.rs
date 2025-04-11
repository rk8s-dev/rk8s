#[allow(unused)]
#[derive(Debug)]
pub enum BuildError {
    MountError(String),
    MutipleError(Vec<BuildError>),
    PathNotExists(String),
    ExecutionError(String),
    ChrootError(String),
    NotImplemented(String),
}

impl BuildError {
    pub fn new_mount_error(msg: String) -> Self {
        BuildError::MountError(msg)
    }

    pub fn new_multiple_error(errors: Vec<BuildError>) -> Self {
        BuildError::MutipleError(errors)
    }

    pub fn new_path_not_exists_error(path: String) -> Self {
        BuildError::PathNotExists(path)
    }

    pub fn new_execution_error(msg: String) -> Self {
        BuildError::ExecutionError(msg)
    }

    pub fn new_chroot_error(msg: String) -> Self {
        BuildError::ChrootError(msg)
    }

    pub fn new_not_implemented_error(msg: String) -> Self {
        BuildError::NotImplemented(msg)
    }
}
