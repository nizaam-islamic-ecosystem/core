//! Capability registry.
//!
//! The `CapabilityRegistry` stores `CapabilityDefinition` and handler
//! associations. It provides thread-safe registration and lookup using
//! a `RwLock<BTreeMap>` to mirror the verified `ErrorCatalog` pattern.

use std::collections::BTreeMap;
use std::sync::{Arc, RwLock};

use crate::identity::CapabilityId;

use super::{CapabilityDefinition, CapabilityHandler};

/// A registered capability entry containing the definition and handler.
#[derive(Clone)]
pub struct CapabilityEntry {
    definition: CapabilityDefinition,
    handler: Arc<dyn CapabilityHandler>,
}

impl CapabilityEntry {
    /// Creates a new entry from a definition and handler.
    pub fn new(definition: CapabilityDefinition, handler: Arc<dyn CapabilityHandler>) -> Self {
        Self {
            definition,
            handler,
        }
    }

    /// Returns the capability definition.
    pub fn definition(&self) -> &CapabilityDefinition {
        &self.definition
    }

    /// Returns the capability handler.
    pub fn handler(&self) -> &dyn CapabilityHandler {
        &*self.handler
    }
}

impl std::fmt::Debug for CapabilityEntry {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter
            .debug_struct("CapabilityEntry")
            .field("definition", &self.definition)
            .finish()
    }
}

/// Error returned from registry operations.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum RegistryError {
    /// The capability is already registered.
    AlreadyRegistered(CapabilityId),
    /// The capability is not registered.
    NotFound(CapabilityId),
}

impl std::fmt::Display for RegistryError {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            RegistryError::AlreadyRegistered(id) => {
                write!(
                    formatter,
                    "capability {} is already registered",
                    id.as_str()
                )
            }
            RegistryError::NotFound(id) => {
                write!(formatter, "capability {} is not registered", id.as_str())
            }
        }
    }
}

impl std::error::Error for RegistryError {}

/// Thread-safe registry of capability definitions and handlers.
///
/// Uses `RwLock<BTreeMap>` for deterministic ordering and efficient
/// concurrent reads. Mirrors the verified `ErrorCatalog` pattern.
#[derive(Default)]
pub struct CapabilityRegistry {
    handlers: RwLock<BTreeMap<CapabilityId, CapabilityEntry>>,
}

impl CapabilityRegistry {
    /// Creates an empty registry.
    pub fn new() -> Self {
        Self {
            handlers: RwLock::new(BTreeMap::new()),
        }
    }

    /// Registers a capability handler.
    ///
    /// # Errors
    ///
    /// Returns `RegistryError::AlreadyRegistered` if the capability ID already exists.
    pub fn register(
        &self,
        definition: CapabilityDefinition,
        handler: Arc<dyn CapabilityHandler>,
    ) -> Result<(), RegistryError> {
        let mut handlers = self.handlers.write().expect("registry lock poisoned");
        if handlers.contains_key(definition.capability_id()) {
            return Err(RegistryError::AlreadyRegistered(
                definition.capability_id().clone(),
            ));
        }
        let entry = CapabilityEntry::new(definition, handler);
        handlers.insert(entry.definition().capability_id().clone(), entry);
        Ok(())
    }

    /// Unregisters a capability by its identifier.
    ///
    /// # Errors
    ///
    /// Returns `RegistryError::NotFound` if the capability ID does not exist.
    pub fn unregister(&self, capability_id: &CapabilityId) -> Result<(), RegistryError> {
        let mut handlers = self.handlers.write().expect("registry lock poisoned");
        if handlers.remove(capability_id).is_none() {
            return Err(RegistryError::NotFound(capability_id.clone()));
        }
        Ok(())
    }

    /// Returns the entry for the given capability identifier, if present.
    pub fn get(&self, capability_id: &CapabilityId) -> Option<CapabilityEntry> {
        let handlers = self.handlers.read().expect("registry lock poisoned");
        handlers.get(capability_id).cloned()
    }

    /// Returns true if the capability identifier is registered.
    pub fn contains(&self, capability_id: &CapabilityId) -> bool {
        let handlers = self.handlers.read().expect("registry lock poisoned");
        handlers.contains_key(capability_id)
    }

    /// Returns the number of registered capabilities.
    pub fn len(&self) -> usize {
        let handlers = self.handlers.read().expect("registry lock poisoned");
        handlers.len()
    }

    /// Returns true if the registry contains no capabilities.
    pub fn is_empty(&self) -> bool {
        let handlers = self.handlers.read().expect("registry lock poisoned");
        handlers.is_empty()
    }

    /// Returns an iterator over all registered entries.
    pub fn iter(&self) -> impl Iterator<Item = CapabilityEntry> + '_ {
        let handlers = self.handlers.read().expect("registry lock poisoned");
        handlers.values().cloned().collect::<Vec<_>>().into_iter()
    }
}

impl Clone for CapabilityRegistry {
    fn clone(&self) -> Self {
        let handlers = self.handlers.read().expect("registry lock poisoned");
        CapabilityRegistry {
            handlers: RwLock::new(handlers.clone()),
        }
    }
}

impl std::fmt::Debug for CapabilityRegistry {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let handlers = self.handlers.read().expect("registry lock poisoned");
        formatter
            .debug_struct("CapabilityRegistry")
            .field("handlers", &handlers)
            .finish()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::capability::{CapabilityInvocation, CapabilityOutcome, arc_handler};
    use crate::identity::EngineId;
    use crate::runtime::EngineContext;

    fn make_registry() -> CapabilityRegistry {
        CapabilityRegistry::new()
    }

    fn make_definition() -> CapabilityDefinition {
        CapabilityDefinition::new(
            CapabilityId::new("test.cap").unwrap(),
            EngineId::new("test.engine").unwrap(),
            "Test Capability",
        )
        .unwrap()
    }

    fn make_handler() -> Arc<dyn CapabilityHandler> {
        arc_handler(|_ctx: &EngineContext, _inv: &CapabilityInvocation| {
            Ok(CapabilityOutcome::new(b"ok".to_vec()))
        })
    }

    #[test]
    fn registry_register_and_get() {
        let registry = make_registry();
        let def = make_definition();
        registry.register(def.clone(), make_handler()).unwrap();

        assert!(registry.contains(def.capability_id()));
        let entry = registry.get(def.capability_id()).unwrap();
        assert_eq!(entry.definition().name(), "Test Capability");
    }

    #[test]
    fn registry_rejects_duplicate_registration() {
        let registry = make_registry();
        let def = make_definition();
        registry.register(def.clone(), make_handler()).unwrap();

        let result = registry.register(def, make_handler());
        assert!(matches!(result, Err(RegistryError::AlreadyRegistered(_))));
    }

    #[test]
    fn registry_unregister() {
        let registry = make_registry();
        let def = make_definition();
        registry.register(def.clone(), make_handler()).unwrap();

        registry.unregister(def.capability_id()).unwrap();
        assert!(!registry.contains(def.capability_id()));
    }

    #[test]
    fn registry_unregister_missing_returns_error() {
        let registry = make_registry();
        let result = registry.unregister(&CapabilityId::new("missing").unwrap());
        assert!(matches!(result, Err(RegistryError::NotFound(_))));
    }

    #[test]
    fn registry_get_missing_returns_none() {
        let registry = make_registry();
        let result = registry.get(&CapabilityId::new("missing").unwrap());
        assert!(result.is_none());
    }

    #[test]
    fn registry_len_and_is_empty() {
        let registry = make_registry();
        assert!(registry.is_empty());
        assert_eq!(registry.len(), 0);

        let def = make_definition();
        registry.register(def, make_handler()).unwrap();

        assert!(!registry.is_empty());
        assert_eq!(registry.len(), 1);
    }

    #[test]
    fn registry_iter_returns_all_entries() {
        let registry = make_registry();
        let def1 = CapabilityDefinition::new(
            CapabilityId::new("cap.a").unwrap(),
            EngineId::new("engine.1").unwrap(),
            "Capability A",
        )
        .unwrap();
        let def2 = CapabilityDefinition::new(
            CapabilityId::new("cap.b").unwrap(),
            EngineId::new("engine.2").unwrap(),
            "Capability B",
        )
        .unwrap();
        registry.register(def1.clone(), make_handler()).unwrap();
        registry.register(def2.clone(), make_handler()).unwrap();

        let entries: Vec<_> = registry.iter().collect();
        assert_eq!(entries.len(), 2);
        assert!(
            entries
                .iter()
                .any(|e| e.definition().capability_id() == def1.capability_id())
        );
        assert!(
            entries
                .iter()
                .any(|e| e.definition().capability_id() == def2.capability_id())
        );
    }
}
