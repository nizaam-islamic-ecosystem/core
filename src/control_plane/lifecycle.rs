use crate::runtime::lifecycle::LifecycleState;

/// Returns whether an engine runtime is eligible to receive new normal
/// Control Plane-routed work for the observed lifecycle state.
///
/// The Engine Runtime remains the authoritative owner of lifecycle state.
/// The Control Plane only consumes that state when determining routing
/// eligibility.
///
/// `Serving` is the only lifecycle state that permits normal new work.
/// `Ready` intentionally does not permit normal request admission.
/// `Draining` and `Stopped` are not eligible for new normal routing.
pub const fn is_routable(state: LifecycleState) -> bool {
    matches!(state, LifecycleState::Serving)
}

#[cfg(test)]
mod tests {
    use super::is_routable;
    use crate::runtime::lifecycle::LifecycleState;

    #[test]
    fn serving_is_routable() {
        assert!(is_routable(LifecycleState::Serving));
    }

    #[test]
    fn ready_is_not_routable() {
        assert!(!is_routable(LifecycleState::Ready));
    }

    #[test]
    fn draining_is_not_routable() {
        assert!(!is_routable(LifecycleState::Draining));
    }

    #[test]
    fn stopped_is_not_routable() {
        assert!(!is_routable(LifecycleState::Stopped));
    }

    #[test]
    fn startup_states_are_not_routable() {
        assert!(!is_routable(LifecycleState::Created));
        assert!(!is_routable(LifecycleState::Starting));
        assert!(!is_routable(LifecycleState::Configuring));
        assert!(!is_routable(LifecycleState::Dependencies));
        assert!(!is_routable(LifecycleState::Capabilities));
        assert!(!is_routable(LifecycleState::Registering));
    }

    #[test]
    fn only_serving_is_routable() {
        let states = [
            LifecycleState::Created,
            LifecycleState::Starting,
            LifecycleState::Configuring,
            LifecycleState::Dependencies,
            LifecycleState::Capabilities,
            LifecycleState::Registering,
            LifecycleState::Ready,
            LifecycleState::Serving,
            LifecycleState::Draining,
            LifecycleState::Stopped,
        ];

        for state in states {
            assert_eq!(
                is_routable(state),
                matches!(state, LifecycleState::Serving),
                "unexpected routing eligibility for {state:?}"
            );
        }
    }

    #[test]
    fn lifecycle_integration_does_not_mutate_runtime_state() {
        let state = LifecycleState::Serving;

        let first = is_routable(state);
        let second = is_routable(state);

        assert!(first);
        assert_eq!(first, second);
        assert_eq!(state, LifecycleState::Serving);
    }
}
