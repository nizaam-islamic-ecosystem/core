//! Routing-facing operational observations for the Control Plane.
//!
//! This module stores the latest point-in-time observations for concrete engine
//! instances. Health semantics remain owned by the `health` subsystem; runtime
//! lifecycle remains owned by the runtime; membership remains owned by the
//! Control Plane membership subsystem. This module only preserves those
//! observations as a coherent, concrete-instance keyed view for later Control
//! Plane decisions.

use std::collections::BTreeMap;
use std::sync::RwLock;

use crate::health::HealthReport;
use crate::identity::{EngineId, EngineInstanceId};
use crate::observability::Diagnostic;

/// Errors produced while creating or mutating Control Plane observations.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum ObservationError {
    /// The health report belongs to a different logical engine than the
    /// observation's declared engine identity.
    EngineIdentityMismatch {
        engine_id: EngineId,
        report_engine_id: EngineId,
    },
    /// An update attempted to associate an existing concrete instance with
    /// another logical engine identity.
    StoredEngineIdentityMismatch {
        instance_id: EngineInstanceId,
        stored_engine_id: EngineId,
        observed_engine_id: EngineId,
    },

    /// No observation exists for the requested concrete engine instance.
    NotFound(EngineInstanceId),
}

impl std::fmt::Display for ObservationError {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::EngineIdentityMismatch {
                engine_id,
                report_engine_id,
            } => write!(
                formatter,
                "observation engine identity does not match health report: expected {}, got {}",
                engine_id.as_str(),
                report_engine_id.as_str(),
            ),
            Self::StoredEngineIdentityMismatch {
                instance_id,
                stored_engine_id,
                observed_engine_id,
            } => write!(
                formatter,
                "engine instance {} is associated with engine {}, but observation specifies engine {}",
                instance_id.as_str(),
                stored_engine_id.as_str(),
                observed_engine_id.as_str(),
            ),
            Self::NotFound(instance_id) => write!(
                formatter,
                "no observation exists for engine instance {}",
                instance_id.as_str(),
            ),
        }
    }
}

impl std::error::Error for ObservationError {}

/// Result type used by the Control Plane observation store.
pub type ObservationResult<T> = Result<T, ObservationError>;

/// One immutable routing-facing observation for a concrete engine instance.
///
/// The health report remains the authoritative health value. The instance
/// identity is added here because one logical engine may have multiple running
/// instances with independent operational state.
#[derive(Clone, Debug, PartialEq)]
pub struct EngineObservation {
    engine_id: EngineId,
    engine_instance_id: EngineInstanceId,
    health: HealthReport,
    diagnostics: Vec<Diagnostic>,
}

impl EngineObservation {
    /// Creates an observation for one concrete engine instance.
    ///
    /// The health report must describe the same logical engine identity. This
    /// prevents an observation from accidentally associating one engine's
    /// health state with another engine instance.
    pub fn new(
        engine_id: EngineId,
        engine_instance_id: EngineInstanceId,
        health: HealthReport,
    ) -> ObservationResult<Self> {
        if health.engine_id() != &engine_id {
            return Err(ObservationError::EngineIdentityMismatch {
                engine_id,
                report_engine_id: health.engine_id().clone(),
            });
        }

        Ok(Self {
            engine_id,
            engine_instance_id,
            health,
            diagnostics: Vec::new(),
        })
    }

    /// Adds one diagnostic associated with this observation.
    #[must_use]
    pub fn with_diagnostic(mut self, diagnostic: Diagnostic) -> Self {
        self.diagnostics.push(diagnostic);
        self
    }

    /// Adds multiple diagnostics associated with this observation.
    #[must_use]
    pub fn with_diagnostics<I>(mut self, diagnostics: I) -> Self
    where
        I: IntoIterator<Item = Diagnostic>,
    {
        self.diagnostics.extend(diagnostics);
        self
    }

    /// Returns the logical engine identity.
    #[must_use]
    pub fn engine_id(&self) -> &EngineId {
        &self.engine_id
    }

    /// Returns the concrete engine instance identity.
    #[must_use]
    pub fn engine_instance_id(&self) -> &EngineInstanceId {
        &self.engine_instance_id
    }

    /// Returns the authoritative health report carried by this observation.
    #[must_use]
    pub fn health(&self) -> &HealthReport {
        &self.health
    }

    /// Returns diagnostics associated with this observation.
    #[must_use]
    pub fn diagnostics(&self) -> &[Diagnostic] {
        &self.diagnostics
    }

    /// Returns the timestamp of the aggregate health observation.
    #[must_use]
    pub fn observed_at(&self) -> std::time::SystemTime {
        self.health.observed_at()
    }
}

/// Immutable point-in-time view of Control Plane observations.
///
/// A snapshot owns its observations. Later updates to the live store cannot
/// mutate an existing snapshot, so a Control Plane decision can remain tied to
/// one coherent observation revision.
#[derive(Clone, Debug, PartialEq)]
pub struct ObservationSnapshot {
    version: u64,
    observations: BTreeMap<EngineInstanceId, EngineObservation>,
}

impl ObservationSnapshot {
    /// Returns the local observation-store revision represented by this
    /// snapshot.
    #[must_use]
    pub const fn version(&self) -> u64 {
        self.version
    }

    /// Looks up one concrete engine instance in this snapshot.
    #[must_use]
    pub fn get(&self, instance_id: &EngineInstanceId) -> Option<&EngineObservation> {
        self.observations.get(instance_id)
    }

    /// Returns whether an observation exists for the concrete engine instance.
    #[must_use]
    pub fn contains(&self, instance_id: &EngineInstanceId) -> bool {
        self.observations.contains_key(instance_id)
    }

    /// Returns the number of observed engine instances in this snapshot.
    #[must_use]
    pub fn len(&self) -> usize {
        self.observations.len()
    }

    /// Returns whether this snapshot contains no observations.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.observations.is_empty()
    }

    /// Enumerates all observations in deterministic `EngineInstanceId` order.
    ///
    /// Ordering is only an iteration guarantee; observations do not assign
    /// routing priority to any engine instance.
    pub fn observations(&self) -> impl Iterator<Item = &EngineObservation> {
        self.observations.values()
    }

    /// Enumerates observations belonging to one logical engine in deterministic
    /// `EngineInstanceId` order.
    pub fn instances_for_engine(
        &self,
        engine_id: &EngineId,
    ) -> impl Iterator<Item = &EngineObservation> {
        self.observations
            .values()
            .filter(move |observation| observation.engine_id() == engine_id)
    }
}

/// Authoritative in-memory store of the latest Control Plane observations.
///
/// The store is keyed by `EngineInstanceId` because operational state belongs
/// to a concrete runtime instance, not merely to the logical engine type.
/// Updates are synchronous bounded state mutations and therefore do not require
/// an async execution context.
#[derive(Debug)]
pub struct Observations {
    state: RwLock<ObservationState>,
}

#[derive(Clone, Debug, PartialEq)]
struct ObservationState {
    version: u64,
    observations: BTreeMap<EngineInstanceId, EngineObservation>,
}

impl Default for Observations {
    fn default() -> Self {
        Self::new()
    }
}

impl Observations {
    /// Creates an empty observation store at revision zero.
    #[must_use]
    pub fn new() -> Self {
        Self {
            state: RwLock::new(ObservationState {
                version: 0,
                observations: BTreeMap::new(),
            }),
        }
    }

    /// Returns the current local observation-store revision.
    #[must_use]
    pub fn version(&self) -> u64 {
        self.state
            .read()
            .expect("observation store read lock must not be poisoned")
            .version
    }

    /// Inserts or replaces the latest observation for one concrete engine
    /// instance.
    ///
    /// Replacing an observation is intentional: this store represents the
    /// latest known point-in-time state rather than an event history.
    pub fn update(&self, observation: EngineObservation) -> ObservationResult<()> {
        let mut state = self
            .state
            .write()
            .expect("observation store write lock must not be poisoned");

        let instance_id = observation.engine_instance_id().clone();

        if let Some(existing) = state.observations.get(&instance_id)
            && existing.engine_id() != observation.engine_id()
        {
            return Err(ObservationError::StoredEngineIdentityMismatch {
                instance_id,
                stored_engine_id: existing.engine_id().clone(),
                observed_engine_id: observation.engine_id().clone(),
            });
        }

        state.observations.insert(instance_id, observation);
        state.version = state
            .version
            .checked_add(1)
            .expect("Control Plane observation version exhausted");

        Ok(())
    }

    /// Removes the observation for one concrete engine instance.
    pub fn remove(&self, instance_id: &EngineInstanceId) -> ObservationResult<()> {
        let mut state = self
            .state
            .write()
            .expect("observation store write lock must not be poisoned");

        if state.observations.remove(instance_id).is_none() {
            return Err(ObservationError::NotFound(instance_id.clone()));
        }

        state.version = state
            .version
            .checked_add(1)
            .expect("Control Plane observation version exhausted");
        Ok(())
    }

    /// Looks up the latest observation for one concrete engine instance.
    #[must_use]
    pub fn get(&self, instance_id: &EngineInstanceId) -> Option<EngineObservation> {
        self.state
            .read()
            .expect("observation store read lock must not be poisoned")
            .observations
            .get(instance_id)
            .cloned()
    }

    /// Returns the number of currently stored engine observations.
    #[must_use]
    pub fn len(&self) -> usize {
        self.state
            .read()
            .expect("observation store read lock must not be poisoned")
            .observations
            .len()
    }

    /// Returns whether the observation store contains no observations.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }

    /// Returns whether an observation exists for one concrete engine instance.
    #[must_use]
    pub fn contains(&self, instance_id: &EngineInstanceId) -> bool {
        self.state
            .read()
            .expect("observation store read lock must not be poisoned")
            .observations
            .contains_key(instance_id)
    }

    /// Returns an immutable, coherent point-in-time snapshot of all
    /// observations.
    #[must_use]
    pub fn snapshot(&self) -> ObservationSnapshot {
        let state = self
            .state
            .read()
            .expect("observation store read lock must not be poisoned");

        ObservationSnapshot {
            version: state.version,
            observations: state.observations.clone(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::health::{HealthStatus, LivenessReport, ReadinessReport};
    use crate::runtime::LifecycleState;

    fn engine_id(value: &str) -> EngineId {
        EngineId::new(value).unwrap()
    }

    fn instance_id(value: &str) -> EngineInstanceId {
        EngineInstanceId::new(value).unwrap()
    }

    fn health_report(engine: &EngineId, lifecycle: LifecycleState) -> HealthReport {
        HealthReport::new(
            engine.clone(),
            lifecycle,
            LivenessReport::healthy(),
            ReadinessReport::from_lifecycle(lifecycle),
            Vec::new(),
            Vec::new(),
        )
        .unwrap()
    }

    fn observation(
        engine: &EngineId,
        instance: &EngineInstanceId,
        lifecycle: LifecycleState,
    ) -> EngineObservation {
        EngineObservation::new(
            engine.clone(),
            instance.clone(),
            health_report(engine, lifecycle),
        )
        .unwrap()
    }

    #[test]
    fn observation_preserves_logical_and_concrete_identity() {
        let engine = engine_id("quran-engine");
        let instance = instance_id("quran-engine-01");
        let observation = observation(&engine, &instance, LifecycleState::Serving);

        assert_eq!(observation.engine_id(), &engine);
        assert_eq!(observation.engine_instance_id(), &instance);
        assert_eq!(observation.health().engine_id(), &engine);
    }

    #[test]
    fn observation_rejects_health_report_for_another_engine() {
        let engine = engine_id("quran-engine");
        let other_engine = engine_id("hadith-engine");
        let instance = instance_id("quran-engine-01");
        let health = health_report(&other_engine, LifecycleState::Serving);

        let result = EngineObservation::new(engine.clone(), instance, health);

        assert_eq!(
            result,
            Err(ObservationError::EngineIdentityMismatch {
                engine_id: engine,
                report_engine_id: other_engine,
            })
        );
    }

    #[test]
    fn store_starts_empty_at_revision_zero() {
        let observations = Observations::new();

        assert_eq!(observations.version(), 0);
        assert!(observations.is_empty());
    }

    #[test]
    fn update_inserts_and_increments_revision() {
        let observations = Observations::new();
        let engine = engine_id("quran-engine");
        let instance = instance_id("quran-engine-01");

        observations
            .update(observation(&engine, &instance, LifecycleState::Serving))
            .unwrap();

        assert_eq!(observations.version(), 1);
        assert!(observations.contains(&instance));
        assert_eq!(
            observations.get(&instance).unwrap().health().overall(),
            HealthStatus::Healthy
        );
    }

    #[test]
    fn update_replaces_only_the_same_concrete_instance() {
        let observations = Observations::new();
        let engine = engine_id("quran-engine");
        let first = instance_id("quran-engine-01");
        let second = instance_id("quran-engine-02");

        observations
            .update(observation(&engine, &first, LifecycleState::Serving))
            .unwrap();
        observations
            .update(observation(&engine, &second, LifecycleState::Ready))
            .unwrap();
        observations
            .update(observation(&engine, &first, LifecycleState::Ready))
            .unwrap();

        assert_eq!(observations.version(), 3);
        assert_eq!(observations.snapshot().len(), 2);
        assert_eq!(
            observations.get(&first).unwrap().health().lifecycle(),
            LifecycleState::Ready
        );
        assert_eq!(
            observations.get(&second).unwrap().health().lifecycle(),
            LifecycleState::Ready
        );
    }

    #[test]
    fn update_cannot_change_logical_engine_identity_for_an_existing_instance() {
        let observations = Observations::new();
        let quran = engine_id("quran-engine");
        let hadith = engine_id("hadith-engine");
        let instance = instance_id("shared-instance");

        observations
            .update(observation(&quran, &instance, LifecycleState::Serving))
            .unwrap();

        let result = observations.update(observation(&hadith, &instance, LifecycleState::Serving));

        assert_eq!(
            result,
            Err(ObservationError::StoredEngineIdentityMismatch {
                instance_id: instance.clone(),
                stored_engine_id: quran.clone(),
                observed_engine_id: hadith.clone(),
            })
        );

        assert_eq!(observations.get(&instance).unwrap().engine_id(), &quran);
        assert_eq!(observations.version(), 1);
    }

    #[test]
    fn snapshot_is_immutable_after_live_store_changes() {
        let observations = Observations::new();
        let engine = engine_id("quran-engine");
        let instance = instance_id("quran-engine-01");

        observations
            .update(observation(&engine, &instance, LifecycleState::Serving))
            .unwrap();
        let first_snapshot = observations.snapshot();

        observations
            .update(observation(&engine, &instance, LifecycleState::Ready))
            .unwrap();
        let second_snapshot = observations.snapshot();

        assert_eq!(first_snapshot.version(), 1);
        assert_eq!(second_snapshot.version(), 2);
        assert_eq!(
            first_snapshot.get(&instance).unwrap().health().lifecycle(),
            LifecycleState::Serving
        );
        assert_eq!(
            second_snapshot.get(&instance).unwrap().health().lifecycle(),
            LifecycleState::Ready
        );
    }

    #[test]
    fn multiple_instances_of_one_engine_remain_independent() {
        let observations = Observations::new();
        let engine = engine_id("quran-engine");
        let first = instance_id("quran-engine-01");
        let second = instance_id("quran-engine-02");

        observations
            .update(observation(&engine, &first, LifecycleState::Serving))
            .unwrap();
        observations
            .update(observation(&engine, &second, LifecycleState::Ready))
            .unwrap();

        let snapshot = observations.snapshot();
        let instances: Vec<_> = snapshot
            .instances_for_engine(&engine)
            .map(|item| item.engine_instance_id().clone())
            .collect();

        assert_eq!(instances, vec![first, second]);
    }

    #[test]
    fn remove_deletes_one_instance_and_increments_revision() {
        let observations = Observations::new();
        let engine = engine_id("quran-engine");
        let instance = instance_id("quran-engine-01");

        observations
            .update(observation(&engine, &instance, LifecycleState::Serving))
            .unwrap();
        observations.remove(&instance).unwrap();

        assert_eq!(observations.version(), 2);
        assert!(!observations.contains(&instance));
        assert_eq!(
            observations.remove(&instance),
            Err(ObservationError::NotFound(instance))
        );
        assert_eq!(observations.version(), 2);
    }

    #[test]
    fn diagnostics_are_associated_without_changing_health_semantics() {
        let engine = engine_id("quran-engine");
        let instance = instance_id("quran-engine-01");
        let diagnostic = Diagnostic::new(
            crate::observability::DiagnosticKind::Runtime,
            crate::observability::DiagnosticCondition::Degraded,
            "runtime condition requires attention",
        )
        .unwrap();

        let observation = observation(&engine, &instance, LifecycleState::Serving)
            .with_diagnostic(diagnostic.clone());

        assert_eq!(observation.diagnostics(), &[diagnostic]);
        assert_eq!(observation.health().overall(), HealthStatus::Healthy);
    }
}
