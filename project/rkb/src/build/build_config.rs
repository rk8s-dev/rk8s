#[derive(Debug, Clone, Default)]
pub struct BuildConfig {
    pub lib_fuse: bool,
    pub debug: bool,
}

impl BuildConfig {
    pub fn lib_fuse(mut self, lib_fuse: bool) -> Self {
        self.lib_fuse = lib_fuse;
        self
    }

    pub fn debug(mut self, debug: bool) -> Self {
        self.debug = debug;
        self
    }
}
