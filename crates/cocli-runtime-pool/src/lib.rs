//! Runtime registry: name → Driver factory lookup with optional allowlist.

#![forbid(unsafe_code)]

use std::collections::HashMap;
use std::sync::Arc;

use cocli_driver_core::Driver;

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
        if let Some(allow) = &self.allowlist {
            if !allow.iter().any(|a| a == name) {
                return None;
            }
        }
        self.drivers.get(name).cloned()
    }

    pub fn names(&self) -> Vec<String> {
        let mut names: Vec<_> = self.drivers.keys().cloned().collect();
        names.sort();
        names
    }
}

impl Default for RuntimeRegistry {
    fn default() -> Self {
        Self::new()
    }
}
