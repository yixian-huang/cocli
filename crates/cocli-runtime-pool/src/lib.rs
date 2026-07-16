//! Runtime registry: name → Driver factory lookup with optional allowlist.

#![forbid(unsafe_code)]

mod catalog;
mod discovery;

use std::collections::HashMap;
use std::sync::Arc;

use cocli_driver_core::Driver;

pub use catalog::{
    initial_oss_runtime_specs, RuntimeCapabilities, RuntimeCatalog, RuntimeCatalogEntry,
    RuntimeModel, RuntimeSpec,
};
pub use discovery::{RuntimeProbe, SystemRuntimeProbe};

pub struct RuntimeRegistry {
    drivers: HashMap<String, Arc<dyn Driver>>,
    /// `None` = all registered runtimes allowed.
    /// `Some(vec![])` = nothing allowed.
    allowlist: Option<Vec<String>>,
}

impl RuntimeRegistry {
    pub fn new() -> Self {
        Self {
            drivers: HashMap::new(),
            allowlist: None,
        }
    }

    pub fn with_allowlist(mut self, allowed: Vec<String>) -> Self {
        self.allowlist = Some(allowed);
        self
    }

    pub fn register(&mut self, driver: Arc<dyn Driver>) {
        self.drivers.insert(driver.name().to_string(), driver);
    }

    pub fn get(&self, name: &str) -> Option<Arc<dyn Driver>> {
        if !self.is_allowed(name) {
            return None;
        }
        self.drivers.get(name).cloned()
    }

    pub fn is_registered(&self, name: &str) -> bool {
        self.drivers.contains_key(name)
    }

    pub fn is_allowed(&self, name: &str) -> bool {
        match &self.allowlist {
            None => true,
            Some(allow) => allow.iter().any(|entry| entry == name),
        }
    }

    pub fn names(&self) -> Vec<String> {
        let mut names: Vec<_> = self.drivers.keys().cloned().collect();
        names.sort();
        names
    }

    pub fn discover(&self, specs: &[RuntimeSpec], probe: &dyn RuntimeProbe) -> RuntimeCatalog {
        RuntimeCatalog::discover(self, specs, probe)
    }
}

impl Default for RuntimeRegistry {
    fn default() -> Self {
        Self::new()
    }
}
