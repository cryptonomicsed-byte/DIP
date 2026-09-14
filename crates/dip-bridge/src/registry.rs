use std::collections::HashMap;
use std::sync::RwLock;
use dip_types::{AdapterManifest, AdapterKind};

pub struct AdapterRegistry {
    adapters: RwLock<HashMap<String, AdapterManifest>>,
}

impl AdapterRegistry {
    pub fn new() -> Self {
        Self { adapters: RwLock::new(HashMap::new()) }
    }

    pub fn register(&self, manifest: AdapterManifest) {
        let mut adapters = self.adapters.write().unwrap();
        adapters.insert(manifest.adapter_id.clone(), manifest);
    }

    pub fn get(&self, adapter_id: &str) -> Option<AdapterManifest> {
        self.adapters.read().unwrap().get(adapter_id).cloned()
    }

    pub fn list(&self) -> Vec<AdapterManifest> {
        self.adapters.read().unwrap().values().cloned().collect()
    }

    pub fn find_by_kind(&self, kind: &AdapterKind) -> Vec<AdapterManifest> {
        self.adapters.read().unwrap()
            .values()
            .filter(|a| &a.kind == kind)
            .cloned()
            .collect()
    }
}

impl Default for AdapterRegistry {
    fn default() -> Self { Self::new() }
}
