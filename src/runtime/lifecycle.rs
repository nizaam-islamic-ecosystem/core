/// States shared by engine lifecycle implementations.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum LifecycleState {
    Created,
    Starting,
    Ready,
    Draining,
    Stopped,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct InvalidTransition {
    pub from: LifecycleState,
    pub to: LifecycleState,
}

impl LifecycleState {
    pub fn can_transition_to(self, next: Self) -> bool {
        matches!(
            (self, next),
            (Self::Created, Self::Starting)
                | (Self::Starting, Self::Ready)
                | (Self::Ready, Self::Draining)
                | (Self::Draining, Self::Stopped)
                | (Self::Created, Self::Stopped)
                | (Self::Starting, Self::Stopped)
                | (Self::Ready, Self::Stopped)
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
            return Err(InvalidTransition {
                from: self.state,
                to: next,
            });
        }
        self.state = next;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::{Lifecycle, LifecycleState};

    #[test]
    fn lifecycle_accepts_ordered_start_and_shutdown() {
        let mut lifecycle = Lifecycle::new();

        lifecycle.transition(LifecycleState::Starting).unwrap();
        lifecycle.transition(LifecycleState::Ready).unwrap();
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
}
