use crate::error::InvalidTransition;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum LifecycleState {
    Created,
    Starting,
    Configuring,
    Dependencies,
    Capabilities,
    Registering,
    Ready,
    Serving,
    Draining,
    Stopped,
}

impl LifecycleState {
    pub fn can_transition_to(self, next: Self) -> bool {
        matches!(
            (self, next),
            (Self::Created, Self::Starting)
                | (Self::Starting, Self::Configuring)
                | (Self::Configuring, Self::Dependencies)
                | (Self::Dependencies, Self::Capabilities)
                | (Self::Capabilities, Self::Registering)
                | (Self::Registering, Self::Ready)
                | (Self::Ready, Self::Serving)
                | (Self::Serving, Self::Draining)
                | (Self::Draining, Self::Stopped)
        )
    }
}

#[derive(Debug)]
pub struct Lifecycle {
    state: LifecycleState,
}

impl Default for Lifecycle {
    fn default() -> Self {
        Self::new()
    }
}

impl Lifecycle {
    pub const fn new() -> Self {
        Self {
            state: LifecycleState::Created,
        }
    }

    pub const fn state(&self) -> LifecycleState {
        self.state
    }

    pub fn transition(&mut self, next: LifecycleState) -> Result<(), InvalidTransition> {
        if !self.state.can_transition_to(next) {
            return Err(InvalidTransition::new(
                format!("{:?}", self.state),
                format!("{:?}", next),
            ));
        }

        self.state = next;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::{Lifecycle, LifecycleState};

    #[test]
    fn lifecycle_accepts_full_ordered_start_and_shutdown() {
        let mut lifecycle = Lifecycle::new();

        lifecycle.transition(LifecycleState::Starting).unwrap();
        lifecycle.transition(LifecycleState::Configuring).unwrap();
        lifecycle.transition(LifecycleState::Dependencies).unwrap();
        lifecycle.transition(LifecycleState::Capabilities).unwrap();
        lifecycle.transition(LifecycleState::Registering).unwrap();
        lifecycle.transition(LifecycleState::Ready).unwrap();
        lifecycle.transition(LifecycleState::Serving).unwrap();
        lifecycle.transition(LifecycleState::Draining).unwrap();
        lifecycle.transition(LifecycleState::Stopped).unwrap();

        assert_eq!(lifecycle.state(), LifecycleState::Stopped);
    }

    #[test]
    fn lifecycle_rejects_invalid_transition() {
        let mut lifecycle = Lifecycle::new();

        assert!(lifecycle.transition(LifecycleState::Ready).is_err());
        assert_eq!(lifecycle.state(), LifecycleState::Created);
    }

    #[test]
    fn lifecycle_rejects_skip_configuring() {
        let mut lifecycle = Lifecycle::new();

        lifecycle.transition(LifecycleState::Starting).unwrap();

        assert!(lifecycle.transition(LifecycleState::Dependencies).is_err());

        assert_eq!(lifecycle.state(), LifecycleState::Starting);
    }

    #[test]
    fn lifecycle_rejects_draining_to_serving() {
        let mut lifecycle = Lifecycle::new();

        lifecycle.transition(LifecycleState::Starting).unwrap();
        lifecycle.transition(LifecycleState::Configuring).unwrap();
        lifecycle.transition(LifecycleState::Dependencies).unwrap();
        lifecycle.transition(LifecycleState::Capabilities).unwrap();
        lifecycle.transition(LifecycleState::Registering).unwrap();
        lifecycle.transition(LifecycleState::Ready).unwrap();
        lifecycle.transition(LifecycleState::Serving).unwrap();
        lifecycle.transition(LifecycleState::Draining).unwrap();

        assert!(lifecycle.transition(LifecycleState::Serving).is_err());
        assert_eq!(lifecycle.state(), LifecycleState::Draining);
    }

    #[test]
    fn lifecycle_rejects_stopped_transitions() {
        let mut lifecycle = Lifecycle::new();

        lifecycle.transition(LifecycleState::Starting).unwrap();
        lifecycle.transition(LifecycleState::Configuring).unwrap();
        lifecycle.transition(LifecycleState::Dependencies).unwrap();
        lifecycle.transition(LifecycleState::Capabilities).unwrap();
        lifecycle.transition(LifecycleState::Registering).unwrap();
        lifecycle.transition(LifecycleState::Ready).unwrap();
        lifecycle.transition(LifecycleState::Serving).unwrap();
        lifecycle.transition(LifecycleState::Draining).unwrap();
        lifecycle.transition(LifecycleState::Stopped).unwrap();

        assert!(lifecycle.transition(LifecycleState::Serving).is_err());
        assert!(lifecycle.transition(LifecycleState::Draining).is_err());
        assert!(lifecycle.transition(LifecycleState::Stopped).is_err());
        assert_eq!(lifecycle.state(), LifecycleState::Stopped);
    }

    #[test]
    fn lifecycle_rejects_direct_created_to_stopped() {
        let mut lifecycle = Lifecycle::new();

        assert!(lifecycle.transition(LifecycleState::Stopped).is_err());
        assert_eq!(lifecycle.state(), LifecycleState::Created);
    }

    #[test]
    fn lifecycle_rejects_direct_starting_to_stopped() {
        let mut lifecycle = Lifecycle::new();

        lifecycle.transition(LifecycleState::Starting).unwrap();

        assert!(lifecycle.transition(LifecycleState::Stopped).is_err());
        assert_eq!(lifecycle.state(), LifecycleState::Starting);
    }

    #[test]
    fn lifecycle_rejects_direct_ready_to_stopped() {
        let mut lifecycle = Lifecycle::new();

        lifecycle.transition(LifecycleState::Starting).unwrap();
        lifecycle.transition(LifecycleState::Configuring).unwrap();
        lifecycle.transition(LifecycleState::Dependencies).unwrap();
        lifecycle.transition(LifecycleState::Capabilities).unwrap();
        lifecycle.transition(LifecycleState::Registering).unwrap();
        lifecycle.transition(LifecycleState::Ready).unwrap();

        assert!(lifecycle.transition(LifecycleState::Stopped).is_err());
        assert_eq!(lifecycle.state(), LifecycleState::Ready);
    }
}
