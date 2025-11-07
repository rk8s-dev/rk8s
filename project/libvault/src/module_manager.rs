//! RustyVault is consisted of many modules. Modules are the real components that implement the
//! features that users need. All modules in RustyVault are managed by `libvault::module_manager`.
//!
//! In details, the module manager is able to organize, add, remove, setup, initialize, cleanup
//! other RustyVault modules.
//!
//! Do not mix up the RustyVault module with the concept of a Rust module. A RustyVault module is a
//! piece of code that implements some functionality. Although usually that piece of code is
//! organized in the form of a module of crate `libvault` in Rust language concept.

use std::{any::Any, sync::Arc};

use arc_swap::ArcSwap;

use crate::{
    core::Core,
    errors::RvError,
    modules::{Module, system::SystemModule},
};

pub struct ModuleManager {
    pub modules: ArcSwap<Vec<Arc<dyn Module>>>,
}

impl ModuleManager {
    pub fn new() -> Self {
        Self {
            modules: ArcSwap::from_pointee(Vec::new()),
        }
    }

    pub fn set_default_modules(&self, core: Arc<Core>) -> Result<(), RvError> {
        let modules: Vec<Arc<dyn Module>> = vec![Arc::new(SystemModule::new(core))];
        self.modules.store(Arc::new(modules));
        Ok(())
    }

    #[inline]
    pub fn get_module<T: Any + Send + Sync>(&self, name: &str) -> Option<Arc<T>> {
        let modules = self.modules.load();
        for m in modules.iter() {
            if m.name().as_str() == name {
                let any_arc = m.clone().as_any_arc();
                return Arc::downcast::<T>(any_arc).ok();
            }
        }

        None
    }

    #[inline]
    pub fn add_module(&self, module: Arc<dyn Module>) -> Result<(), RvError> {
        let modules = self.modules.load();
        for m in modules.iter() {
            if m.name().as_str() == module.name().as_str() {
                return Err(RvError::ErrModuleConflict);
            }
        }

        let old_modules = self.modules.load_full();
        let mut modules = (*old_modules).clone();
        modules.push(module);

        let modules = Arc::new(modules);

        if !Arc::ptr_eq(&self.modules.load().clone(), &old_modules) {
            return Err(RvError::ErrModuleConflict);
        }

        self.modules.store(modules);

        Ok(())
    }

    pub fn remove_module(&self, name: &str) -> Result<(), RvError> {
        let old_modules = self.modules.load_full();
        let mut modules = (*old_modules).clone();
        modules.retain(|m| m.name().as_str() != name);

        let modules = Arc::new(modules);

        if !Arc::ptr_eq(&self.modules.load().clone(), &old_modules) {
            return Err(RvError::ErrModuleConflict);
        }

        self.modules.store(modules);

        Ok(())
    }

    pub fn setup(&self, core: &Core) -> Result<(), RvError> {
        let modules = self.modules.load().clone();
        for module in modules.iter() {
            module.setup(core)?;
        }

        Ok(())
    }

    pub async fn init(&self, core: &Core) -> Result<(), RvError> {
        let modules = self.modules.load().clone();
        for module in modules.iter() {
            module.init(core).await?;
        }

        Ok(())
    }

    pub fn cleanup(&self, core: &Core) -> Result<(), RvError> {
        let modules = self.modules.load().clone();
        for module in modules.iter() {
            module.cleanup(core)?;
        }

        Ok(())
    }
}
