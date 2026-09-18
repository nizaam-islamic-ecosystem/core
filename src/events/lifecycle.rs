//! Lifecycle coordination for the internal Nizaam Core event subsystem.
//!
//! The Core runtime remains the canonical owner of application lifecycle.
//! This module only represents the lifecycle boundary of Event infrastructure:
//! whether event publication is currently admitted and whether Event-owned
//! infrastructure is draining or stopped.
//!
//! Event lifecycle state must not become a second Core runtime lifecycle.
//! Runtime shutdown drives this state, while Event delivery infrastructure
//! is responsible for terminating its own workers and resources.
//!
//! Publication admission is intentionally separate from delivery, security,
//! subscription matching, cancellation, retry, and domain semantics.

use std::fmt;

/// Lifecycle state of the internal Event subsystem.
///
/// This is an Event-specific lifecycle view, not a replacement for the
/// canonical Core runtime lifecycle.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum EventLifecycleState {
    /// Event publication and normal delivery are operational.
    Serving,

    /// New event publication is closed while accepted work is being drained.
    Draining,

    /// Event publication and Event-owned processing have stopped.
    Stopped,
}

impl EventLifecycleState {
    /// Returns whether new event publication may be admitted.
    pub const fn allows_publication(self) -> bool {
        if self.is_stopped() {
            return false;
        }

        !self.is_shutting_down()
    }

    /// Returns whether the Event subsystem has entered shutdown.
    pub const fn is_shutting_down(self) -> bool {
        matches!(self, Self::Draining | Self::Stopped)
    }

    /// Returns whether the Event subsystem has completely stopped.
    pub const fn is_stopped(self) -> bool {
        matches!(self, Self::Stopped)
    }
}

impl fmt::Display for EventLifecycleState {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        let state = match self {
            Self::Serving => "serving",
            Self::Draining => "draining",
            Self::Stopped => "stopped",
        };

        formatter.write_str(state)
    }
}

/// Coordinates lifecycle admission for Event infrastructure.
///
/// `EventLifecycle` does not own the Core runtime lifecycle. It provides the
/// Event subsystem with a small, synchronized state boundary that other Event
/// components can consult when deciding whether new work may be admitted.
///
/// Resource termination itself remains the responsibility of the component
/// that owns those resources, such as Event delivery workers.
#[derive(Debug)]
pub struct EventLifecycle {
    state: std::sync::atomic::AtomicU8,
}

impl EventLifecycle {
    const SERVING: u8 = 0;
    const DRAINING: u8 = 1;
    const STOPPED: u8 = 2;

    /// Creates a new Event lifecycle in the serving state.
    pub fn new() -> Self {
        Self {
            state: std::sync::atomic::AtomicU8::new(Self::SERVING),
        }
    }

    /// Returns the current Event lifecycle state.
    pub fn state(&self) -> EventLifecycleState {
        match self.state.load(std::sync::atomic::Ordering::Acquire) {
            Self::SERVING => EventLifecycleState::Serving,
            Self::DRAINING => EventLifecycleState::Draining,
            Self::STOPPED => EventLifecycleState::Stopped,
            _ => unreachable!("EventLifecycle contains an invalid state"),
        }
    }

    /// Returns whether a new event publication may be admitted.
    pub fn allows_publication(&self) -> bool {
        self.state().allows_publication()
    }

    /// Begins Event subsystem draining.
    ///
    /// The transition from `Serving` to `Draining` closes admission for new
    /// publications. Repeated calls are harmless once draining or stopped.
    pub fn begin_draining(&self) {
        let _ = self.state.compare_exchange(
            Self::SERVING,
            Self::DRAINING,
            std::sync::atomic::Ordering::AcqRel,
            std::sync::atomic::Ordering::Acquire,
        );
    }

    /// Marks Event infrastructure as stopped.
    ///
    /// Stopping is monotonic. Calling this more than once is harmless.
    pub fn stop(&self) {
        self.state
            .store(Self::STOPPED, std::sync::atomic::Ordering::Release);
    }
}

impl Default for EventLifecycle {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn lifecycle_starts_in_serving_state() {
        let lifecycle = EventLifecycle::new();

        assert_eq!(lifecycle.state(), EventLifecycleState::Serving);
        assert!(lifecycle.allows_publication());
    }

    #[test]
    fn serving_state_allows_publication() {
        assert!(EventLifecycleState::Serving.allows_publication());
        assert!(!EventLifecycleState::Serving.is_shutting_down());
        assert!(!EventLifecycleState::Serving.is_stopped());
    }

    #[test]
    fn draining_state_rejects_new_publication() {
        assert!(!EventLifecycleState::Draining.allows_publication());
        assert!(EventLifecycleState::Draining.is_shutting_down());
        assert!(!EventLifecycleState::Draining.is_stopped());
    }

    #[test]
    fn stopped_state_rejects_new_publication() {
        assert!(!EventLifecycleState::Stopped.allows_publication());
        assert!(EventLifecycleState::Stopped.is_shutting_down());
        assert!(EventLifecycleState::Stopped.is_stopped());
    }

    #[test]
    fn lifecycle_transitions_from_serving_to_draining() {
        let lifecycle = EventLifecycle::new();

        lifecycle.begin_draining();

        assert_eq!(lifecycle.state(), EventLifecycleState::Draining);
        assert!(!lifecycle.allows_publication());
    }

    #[test]
    fn lifecycle_transitions_from_draining_to_stopped() {
        let lifecycle = EventLifecycle::new();

        lifecycle.begin_draining();
        lifecycle.stop();

        assert_eq!(lifecycle.state(), EventLifecycleState::Stopped);
        assert!(!lifecycle.allows_publication());
    }

    #[test]
    fn stopping_serving_directly_closes_publication() {
        let lifecycle = EventLifecycle::new();

        lifecycle.stop();

        assert_eq!(lifecycle.state(), EventLifecycleState::Stopped);
        assert!(!lifecycle.allows_publication());
    }

    #[test]
    fn beginning_draining_is_idempotent() {
        let lifecycle = EventLifecycle::new();

        lifecycle.begin_draining();
        lifecycle.begin_draining();

        assert_eq!(lifecycle.state(), EventLifecycleState::Draining);
    }

    #[test]
    fn stopping_is_idempotent() {
        let lifecycle = EventLifecycle::new();

        lifecycle.stop();
        lifecycle.stop();

        assert_eq!(lifecycle.state(), EventLifecycleState::Stopped);
    }

    #[test]
    fn stopped_lifecycle_cannot_be_resurrected() {
        let lifecycle = EventLifecycle::new();

        lifecycle.stop();
        lifecycle.begin_draining();

        assert_eq!(lifecycle.state(), EventLifecycleState::Stopped);
        assert!(!lifecycle.allows_publication());
    }

    #[test]
    fn lifecycle_state_display_is_stable() {
        assert_eq!(EventLifecycleState::Serving.to_string(), "serving");
        assert_eq!(EventLifecycleState::Draining.to_string(), "draining");
        assert_eq!(EventLifecycleState::Stopped.to_string(), "stopped");
    }

    #[test]
    fn lifecycle_state_is_copy_and_eq() {
        let serving = EventLifecycleState::Serving;
        let copied = serving;

        assert_eq!(serving, copied);
    }

    #[test]
    fn concurrent_shutdown_is_safe() {
        use std::sync::Arc;
        use std::thread;

        let lifecycle = Arc::new(EventLifecycle::new());

        let mut handles = Vec::new();

        for _ in 0..16 {
            let lifecycle = Arc::clone(&lifecycle);

            handles.push(thread::spawn(move || {
                lifecycle.begin_draining();
                lifecycle.stop();
            }));
        }

        for handle in handles {
            handle.join().unwrap();
        }

        assert_eq!(lifecycle.state(), EventLifecycleState::Stopped);
        assert!(!lifecycle.allows_publication());
    }
}
