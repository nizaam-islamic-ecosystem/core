//! Subscription and subscriber abstractions for internal Nizaam Core events.
//!
//! This module owns the declarative subscription boundary:
//!
//! ```text
//! EventSubscription
//! ├── event type
//! ├── scope
//! ├── subscriber handler
//! ├── cancellation
//! └── lifecycle
//! ```
//!
//! Publication remains owned by `publisher.rs`.
//! Queueing, worker execution, ordering, backpressure, and panic isolation
//! remain owned by `delivery.rs`.
//!
//! Subscription matching is intentionally limited to generic Core metadata.
//! Payload semantics are never interpreted here.
//!
//! Subscription authorization, when configured, reuses the existing Core
//! `Authorizer` and `SecurityContext` abstractions rather than introducing a
//! second Event-specific authorization framework.
//!
//! Subscription ownership reuses the existing Core `CancellationToken`
//! hierarchy. A subscription receives a child token from its owner's token,
//! so owner cancellation propagates to the subscription while subscription
//! cancellation cannot propagate back to the owner.

use std::fmt;
use std::sync::{
    Arc,
    atomic::{AtomicU8, Ordering},
};

use crate::{
    events::{Event, EventName, Scope},
    identity::CapabilityId,
    operation::CancellationToken,
    security::{AuthorizationDecision, AuthorizationRequest, Authorizer, SecurityContext},
};

/// Handler contract for an internal Core event subscriber.
///
/// Implementations must be safe to share between independent delivery paths.
/// Handler execution itself is performed by `delivery.rs`, not by the
/// subscription object.
pub trait EventSubscriber: Send + Sync + 'static {
    /// Handles one immutable event occurrence.
    ///
    /// Handler failures and panic isolation belong to the delivery layer.
    fn handle(&self, event: &Event);
}

impl<F> EventSubscriber for F
where
    F: Fn(&Event) + Send + Sync + 'static,
{
    fn handle(&self, event: &Event) {
        self(event);
    }
}

/// Lifecycle state of one event subscription.
///
/// This lifecycle belongs to an individual subscription. It is deliberately
/// separate from the Event subsystem lifecycle in `events::lifecycle`.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum SubscriptionLifecycleState {
    /// The subscription has been constructed but is not registered/active.
    Created,

    /// The subscription is registered and may receive matching events.
    Active,

    /// The subscription has been cancelled.
    Cancelled,

    /// The subscription has been explicitly closed.
    Closed,
}

impl SubscriptionLifecycleState {
    /// Returns `true` when this state is terminal.
    pub const fn is_terminal(self) -> bool {
        matches!(self, Self::Cancelled | Self::Closed)
    }

    /// Returns whether this state can transition to `next`.
    ///
    /// Repeating the same state is an idempotent no-op. Terminal states cannot
    /// transition back into an active state or into a different terminal state.
    pub const fn can_transition_to(self, next: Self) -> bool {
        matches!(
            (self, next),
            (Self::Created, Self::Created)
                | (Self::Active, Self::Active)
                | (Self::Cancelled, Self::Cancelled)
                | (Self::Closed, Self::Closed)
                | (Self::Created, Self::Active)
                | (Self::Created, Self::Cancelled)
                | (Self::Created, Self::Closed)
                | (Self::Active, Self::Cancelled)
                | (Self::Active, Self::Closed)
        )
    }
}

/// Error returned when an invalid subscription lifecycle transition is requested.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct SubscriptionLifecycleError {
    from: SubscriptionLifecycleState,
    to: SubscriptionLifecycleState,
}

impl SubscriptionLifecycleError {
    /// Creates an error describing an invalid lifecycle transition.
    pub const fn new(from: SubscriptionLifecycleState, to: SubscriptionLifecycleState) -> Self {
        Self { from, to }
    }

    /// Returns the current subscription state at the failed transition.
    pub const fn from(self) -> SubscriptionLifecycleState {
        self.from
    }

    /// Returns the requested destination state.
    pub const fn to(self) -> SubscriptionLifecycleState {
        self.to
    }
}

impl fmt::Display for SubscriptionLifecycleError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            formatter,
            "invalid event subscription lifecycle transition from {:?} to {:?}",
            self.from, self.to
        )
    }
}

impl std::error::Error for SubscriptionLifecycleError {}

/// Error returned when a subscription cannot be constructed.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum SubscriptionCreationError {
    /// An empty event type would otherwise become an implicit non-specific
    /// subscription.
    EmptyEventType,
}

impl fmt::Display for SubscriptionCreationError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::EmptyEventType => {
                formatter.write_str("event subscription type must not be empty")
            }
        }
    }
}

impl std::error::Error for SubscriptionCreationError {}

const CREATED: u8 = 0;
const ACTIVE: u8 = 1;
const CANCELLED: u8 = 2;
const CLOSED: u8 = 3;

/// One explicit subscription to a class of internal events.
///
/// A subscription is primarily a declaration of interest and ownership. It
/// does not contain queues, worker handles, delivery threads, retry state, or
/// backpressure state. Optional authorization configuration only references the
/// existing Core security abstractions; authorization execution remains
/// provider-neutral.
pub struct EventSubscription {
    event_name: EventName,
    event_type: Box<str>,
    scope: Scope,
    subscriber: Arc<dyn EventSubscriber>,
    cancellation: CancellationToken,
    lifecycle: AtomicU8,
    security_context: Option<SecurityContext>,
    authorizer: Option<Arc<dyn Authorizer>>,
    authorization_capability: Option<CapabilityId>,
}

impl fmt::Debug for EventSubscription {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("EventSubscription")
            .field("event_name", &self.event_name)
            .field("event_type", &self.event_type)
            .field("scope", &self.scope)
            .field("lifecycle", &self.state())
            .field(
                "authorization_configured",
                &(self.security_context.is_some()
                    || self.authorizer.is_some()
                    || self.authorization_capability.is_some()),
            )
            .finish_non_exhaustive()
    }
}

impl EventSubscription {
    /// Creates an unregistered subscription owned by the supplied cancellation
    /// scope.
    ///
    /// A child cancellation token is derived from `owner_cancellation`.
    /// Cancelling the owner therefore cancels the subscription, while
    /// cancelling the subscription does not cancel the owner.
    pub fn new(
        event_name: EventName,
        event_type: impl Into<String>,
        scope: Scope,
        subscriber: impl EventSubscriber,
        owner_cancellation: &CancellationToken,
    ) -> Result<Self, SubscriptionCreationError> {
        let event_type = event_type.into();

        if event_type.trim().is_empty() {
            return Err(SubscriptionCreationError::EmptyEventType);
        }

        Ok(Self {
            event_name,
            event_type: event_type.into_boxed_str(),
            scope,
            subscriber: Arc::new(subscriber),
            cancellation: owner_cancellation.child_token(),
            lifecycle: AtomicU8::new(CREATED),
            security_context: None,
            authorizer: None,
            authorization_capability: None,
        })
    }

    /// Returns the event name selected by this subscription.
    pub fn event_name(&self) -> &EventName {
        &self.event_name
    }

    /// Returns the event type selected by this subscription.
    pub fn event_type(&self) -> &str {
        &self.event_type
    }

    /// Returns the scope selected by this subscription.
    pub fn scope(&self) -> &Scope {
        &self.scope
    }

    /// Returns the subscriber handler.
    ///
    /// The returned `Arc` keeps the handler alive for delivery work without
    /// exposing mutable access to the subscription's internal state.
    pub fn subscriber(&self) -> Arc<dyn EventSubscriber> {
        Arc::clone(&self.subscriber)
    }

    /// Returns the cancellation token used by the subscription and its delivery
    /// path.
    pub fn cancellation_token(&self) -> CancellationToken {
        self.cancellation.clone()
    }

    /// Attaches the trusted security identity of the subscriber.
    ///
    /// This context represents the subscriber, not the event producer. It is
    /// consumed by the existing Core `Authorizer` when authorization is
    /// configured for the subscription.
    pub fn with_security_context(mut self, security_context: SecurityContext) -> Self {
        self.security_context = Some(security_context);
        self
    }

    /// Attaches the existing Core authorization authority used for delivery
    /// admission.
    pub fn with_authorizer(mut self, authorizer: Arc<dyn Authorizer>) -> Self {
        self.authorizer = Some(authorizer);
        self
    }

    /// Requires the supplied Core capability for this subscription to receive
    /// events.
    ///
    /// The capability identifies the protected receiving capability. It does
    /// not redefine `EventType` and does not make every Event a capability
    /// invocation.
    pub fn requiring_capability(mut self, capability: CapabilityId) -> Self {
        self.authorization_capability = Some(capability);
        self
    }

    /// Returns the subscriber security context, when configured.
    pub fn security_context(&self) -> Option<&SecurityContext> {
        self.security_context.as_ref()
    }

    /// Returns the configured authorization capability, when present.
    pub fn authorization_capability(&self) -> Option<&CapabilityId> {
        self.authorization_capability.as_ref()
    }

    /// Evaluates this subscription's existing Core authorization configuration.
    ///
    /// When no authorization components are configured, delivery remains
    /// available for ordinary internal events. If any authorization component
    /// is supplied without all required components, the result fails closed.
    pub fn authorization_decision(
        &self,
    ) -> Result<AuthorizationDecision, crate::security::AuthorizationError> {
        match (
            self.security_context.as_ref(),
            self.authorizer.as_ref(),
            self.authorization_capability.as_ref(),
        ) {
            (None, None, None) => Ok(AuthorizationDecision::Allow),
            (Some(context), Some(authorizer), Some(capability)) => {
                let request = AuthorizationRequest::new(
                    context.principal(),
                    context.calling_service(),
                    capability,
                );
                authorizer.authorize(&request)
            }
            _ => Ok(AuthorizationDecision::Deny),
        }
    }

    /// Returns the current subscription lifecycle state.
    ///
    /// Owner cancellation is reflected as `Cancelled` without requiring an
    /// independent cancellation watcher or Event-specific cancellation system.
    pub fn state(&self) -> SubscriptionLifecycleState {
        let state = self.atomic_state();

        if state == SubscriptionLifecycleState::Active && self.cancellation.is_cancelled() {
            SubscriptionLifecycleState::Cancelled
        } else {
            state
        }
    }

    /// Returns `true` when the subscription is currently active.
    pub fn is_active(&self) -> bool {
        self.state() == SubscriptionLifecycleState::Active
    }

    /// Returns `true` when the subscription has terminated.
    pub fn is_terminal(&self) -> bool {
        self.state().is_terminal()
    }

    /// Returns `true` when cancellation has terminated the subscription.
    pub fn is_cancelled(&self) -> bool {
        self.state() == SubscriptionLifecycleState::Cancelled
    }

    /// Returns `true` when the subscription has been explicitly closed.
    pub fn is_closed(&self) -> bool {
        self.state() == SubscriptionLifecycleState::Closed
    }

    /// Activates a newly constructed subscription.
    ///
    /// Registration with the publisher is responsible for deciding when this
    /// operation is appropriate.
    pub fn activate(&self) -> Result<(), SubscriptionLifecycleError> {
        if self.cancellation.is_cancelled() {
            return Err(SubscriptionLifecycleError::new(
                SubscriptionLifecycleState::Cancelled,
                SubscriptionLifecycleState::Active,
            ));
        }

        self.transition_to(SubscriptionLifecycleState::Active)
    }

    /// Cancels the subscription.
    ///
    /// Repeated cancellation is an idempotent no-op. A closed subscription
    /// cannot be changed into the cancelled state.
    pub fn cancel(&self) -> Result<(), SubscriptionLifecycleError> {
        self.cancellation.cancel();

        if self.state() == SubscriptionLifecycleState::Cancelled {
            return Ok(());
        }

        self.transition_to(SubscriptionLifecycleState::Cancelled)
    }

    /// Explicitly closes the subscription.
    ///
    /// Closing is distinct from cancellation. A cancelled subscription remains
    /// cancelled and cannot be reopened or converted into another terminal
    /// state.
    pub fn close(&self) -> Result<(), SubscriptionLifecycleError> {
        self.transition_to(SubscriptionLifecycleState::Closed)
    }

    /// Returns whether this subscription accepts the supplied event metadata.
    ///
    /// A subscription matches only when the event name, event type, and scope
    /// all match. Security authorization, payload interpretation, and delivery
    /// admission are handled by their respective Core subsystems.
    pub fn matches(&self, event_name: &EventName, event_type: &str, scope: &Scope) -> bool {
        self.is_active()
            && self.event_name == *event_name
            && self.event_type.as_ref() == event_type
            && &self.scope == scope
    }

    fn atomic_state(&self) -> SubscriptionLifecycleState {
        match self.lifecycle.load(Ordering::Acquire) {
            CREATED => SubscriptionLifecycleState::Created,
            ACTIVE => SubscriptionLifecycleState::Active,
            CANCELLED => SubscriptionLifecycleState::Cancelled,
            CLOSED => SubscriptionLifecycleState::Closed,
            _ => unreachable!("event subscription contains an invalid lifecycle state"),
        }
    }

    fn transition_to(
        &self,
        next: SubscriptionLifecycleState,
    ) -> Result<(), SubscriptionLifecycleError> {
        loop {
            let current = self.state();

            if current == next {
                return Ok(());
            }

            if !current.can_transition_to(next) {
                return Err(SubscriptionLifecycleError::new(current, next));
            }

            let current_raw = encode_state(current);
            let next_raw = encode_state(next);

            match self.lifecycle.compare_exchange(
                current_raw,
                next_raw,
                Ordering::AcqRel,
                Ordering::Acquire,
            ) {
                Ok(_) => return Ok(()),
                Err(_) => continue,
            }
        }
    }
}

fn encode_state(state: SubscriptionLifecycleState) -> u8 {
    match state {
        SubscriptionLifecycleState::Created => CREATED,
        SubscriptionLifecycleState::Active => ACTIVE,
        SubscriptionLifecycleState::Cancelled => CANCELLED,
        SubscriptionLifecycleState::Closed => CLOSED,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{identity::EventId, operation::CancellationToken};
    use std::sync::{
        Arc,
        atomic::{AtomicUsize, Ordering},
    };

    fn scope() -> Scope {
        Scope::new("engine:test").unwrap()
    }

    fn subscription() -> EventSubscription {
        let owner = CancellationToken::new();

        EventSubscription::new(
            EventName::new("operation.completed").unwrap(),
            "operation.completed",
            scope(),
            |_event: &Event| {},
            &owner,
        )
        .unwrap()
    }

    #[test]
    fn subscription_constructs_in_created_state() {
        let subscription = subscription();

        assert_eq!(subscription.state(), SubscriptionLifecycleState::Created);
        assert_eq!(subscription.event_name().as_str(), "operation.completed");
        assert_eq!(subscription.event_type(), "operation.completed");
        assert_eq!(subscription.scope().as_str(), "engine:test");
    }

    #[test]
    fn subscription_rejects_empty_event_type() {
        let owner = CancellationToken::new();

        assert_eq!(
            EventSubscription::new(
                EventName::new("operation.completed").unwrap(),
                "",
                scope(),
                |_event: &Event| {},
                &owner,
            )
            .unwrap_err(),
            SubscriptionCreationError::EmptyEventType
        );

        assert_eq!(
            EventSubscription::new(
                EventName::new("operation.completed").unwrap(),
                "   ",
                scope(),
                |_event: &Event| {},
                &owner,
            )
            .unwrap_err(),
            SubscriptionCreationError::EmptyEventType
        );
    }

    #[test]
    fn matching_requires_event_name_event_type_and_scope() {
        let subscription = subscription();

        subscription.activate().unwrap();

        let matching_scope = Scope::new("engine:test").unwrap();
        let different_scope = Scope::new("engine:other").unwrap();

        let matching_name = EventName::new("operation.completed").unwrap();
        let different_name = EventName::new("operation.failed").unwrap();

        assert!(subscription.matches(&matching_name, "operation.completed", &matching_scope,));
        assert!(!subscription.matches(&different_name, "operation.completed", &matching_scope,));
        assert!(!subscription.matches(&matching_name, "operation.failed", &matching_scope,));
        assert!(!subscription.matches(&matching_name, "operation.completed", &different_scope,));
    }

    #[test]
    fn inactive_subscription_does_not_match() {
        let subscription = subscription();
        let matching_scope = Scope::new("engine:test").unwrap();

        let event_name = EventName::new("operation.completed").unwrap();
        assert!(!subscription.matches(&event_name, "operation.completed", &matching_scope));
    }

    #[test]
    fn subscription_activates_only_from_created() {
        let subscription = subscription();

        subscription.activate().unwrap();

        assert_eq!(subscription.state(), SubscriptionLifecycleState::Active);
        assert!(subscription.is_active());

        assert!(subscription.activate().is_ok());

        assert_eq!(subscription.state(), SubscriptionLifecycleState::Active);
    }

    #[test]
    fn cancellation_is_terminal_and_idempotent() {
        let subscription = subscription();

        subscription.activate().unwrap();
        subscription.cancel().unwrap();
        subscription.cancel().unwrap();

        assert_eq!(subscription.state(), SubscriptionLifecycleState::Cancelled);
        assert!(!subscription.is_active());
        assert!(subscription.is_terminal());
        assert!(subscription.is_cancelled());
    }

    #[test]
    fn closing_is_terminal_and_idempotent() {
        let subscription = subscription();

        subscription.activate().unwrap();
        subscription.close().unwrap();
        subscription.close().unwrap();

        assert_eq!(subscription.state(), SubscriptionLifecycleState::Closed);
        assert!(!subscription.is_active());
        assert!(subscription.is_terminal());
        assert!(subscription.is_closed());
    }

    #[test]
    fn terminal_state_cannot_be_reactivated() {
        let cancelled = subscription();
        cancelled.cancel().unwrap();

        assert!(cancelled.activate().is_err());
        assert_eq!(cancelled.state(), SubscriptionLifecycleState::Cancelled);

        let closed = subscription();
        closed.close().unwrap();

        assert!(closed.activate().is_err());
        assert_eq!(closed.state(), SubscriptionLifecycleState::Closed);
    }

    #[test]
    fn cancelled_subscription_cannot_be_closed() {
        let subscription = subscription();

        subscription.cancel().unwrap();

        assert!(subscription.close().is_err());
        assert_eq!(subscription.state(), SubscriptionLifecycleState::Cancelled);
    }

    #[test]
    fn closed_subscription_cannot_be_cancelled() {
        let subscription = subscription();

        subscription.close().unwrap();

        assert!(subscription.cancel().is_err());
        assert_eq!(subscription.state(), SubscriptionLifecycleState::Closed);
    }

    #[test]
    fn owner_cancellation_propagates_to_subscription() {
        let owner = CancellationToken::new();

        let subscription = EventSubscription::new(
            EventName::new("operation.completed").unwrap(),
            "operation.completed",
            scope(),
            |_event: &Event| {},
            &owner,
        )
        .unwrap();

        subscription.activate().unwrap();
        owner.cancel();

        assert_eq!(subscription.state(), SubscriptionLifecycleState::Cancelled);
        assert!(subscription.is_cancelled());
        assert!(!subscription.is_active());
    }

    #[test]
    fn subscription_cancellation_does_not_cancel_owner() {
        let owner = CancellationToken::new();

        let subscription = EventSubscription::new(
            EventName::new("operation.completed").unwrap(),
            "operation.completed",
            scope(),
            |_event: &Event| {},
            &owner,
        )
        .unwrap();

        subscription.activate().unwrap();
        subscription.cancel().unwrap();

        assert!(!owner.is_cancelled());
        assert!(subscription.is_cancelled());
    }

    #[test]
    fn sibling_subscriptions_have_independent_cancellation() {
        let owner = CancellationToken::new();

        let first = EventSubscription::new(
            EventName::new("operation.completed").unwrap(),
            "operation.completed",
            scope(),
            |_event: &Event| {},
            &owner,
        )
        .unwrap();

        let second = EventSubscription::new(
            EventName::new("operation.completed").unwrap(),
            "operation.completed",
            scope(),
            |_event: &Event| {},
            &owner,
        )
        .unwrap();

        first.activate().unwrap();
        second.activate().unwrap();

        first.cancel().unwrap();

        assert!(first.is_cancelled());
        assert!(second.is_active());
        assert!(!owner.is_cancelled());
    }

    #[test]
    fn subscriber_handler_can_receive_immutable_event() {
        let received = Arc::new(AtomicUsize::new(0));
        let received_clone = Arc::clone(&received);

        let subscriber = move |_event: &Event| {
            received_clone.fetch_add(1, Ordering::SeqCst);
        };

        let event = Event::new(
            EventId::new("event-1").unwrap(),
            EventName::new("test.event").unwrap(),
            "test.event",
            scope(),
        )
        .unwrap();

        subscriber.handle(&event);

        assert_eq!(received.load(Ordering::SeqCst), 1);
    }

    #[test]
    fn subscriber_handler_is_shared_without_mutating_subscription() {
        let received = Arc::new(AtomicUsize::new(0));
        let received_clone = Arc::clone(&received);

        let subscription = EventSubscription::new(
            EventName::new("operation.completed").unwrap(),
            "operation.completed",
            scope(),
            move |_event: &Event| {
                received_clone.fetch_add(1, Ordering::SeqCst);
            },
            &CancellationToken::new(),
        )
        .unwrap();

        let handler = subscription.subscriber();
        let event = Event::new(
            EventId::new("event-2").unwrap(),
            EventName::new("test.event").unwrap(),
            "test.event",
            scope(),
        )
        .unwrap();

        handler.handle(&event);
        handler.handle(&event);

        assert_eq!(received.load(Ordering::SeqCst), 2);
        assert_eq!(subscription.state(), SubscriptionLifecycleState::Created);
    }

    #[test]
    fn lifecycle_transition_rules_are_terminal() {
        let terminal_states = [
            SubscriptionLifecycleState::Cancelled,
            SubscriptionLifecycleState::Closed,
        ];

        for state in terminal_states {
            assert!(!state.can_transition_to(SubscriptionLifecycleState::Active));
            assert!(state.can_transition_to(state));
            assert!(state.is_terminal());
        }
    }

    #[test]
    fn lifecycle_state_copy_and_equality_are_stable() {
        let state = SubscriptionLifecycleState::Active;
        let copied = state;

        assert_eq!(state, copied);
    }

    #[test]
    fn authorization_defaults_to_allow_when_not_configured() {
        let subscription = subscription();

        assert_eq!(
            subscription.authorization_decision().unwrap(),
            crate::security::AuthorizationDecision::Allow
        );
    }

    #[test]
    fn authorization_fails_closed_when_configuration_is_incomplete() {
        let subscription = subscription()
            .requiring_capability(crate::identity::CapabilityId::new("events.read").unwrap());

        assert_eq!(
            subscription.authorization_decision().unwrap(),
            crate::security::AuthorizationDecision::Deny
        );
    }

    #[test]
    fn existing_authorizer_receives_subscriber_security_context() {
        use crate::security::{
            AuthorizationDecision, AuthorizationRequest, Authorizer, SecurityContext,
            identity::{PrincipalId, PrincipalIdentity, PrincipalType},
        };

        struct RecordingAuthorizer;

        impl Authorizer for RecordingAuthorizer {
            fn authorize(
                &self,
                request: &AuthorizationRequest<'_>,
            ) -> Result<AuthorizationDecision, crate::security::AuthorizationError> {
                if request.principal().principal_id().as_str() == "subscriber" {
                    Ok(AuthorizationDecision::Allow)
                } else {
                    Ok(AuthorizationDecision::Deny)
                }
            }
        }

        let principal = PrincipalIdentity::new(
            PrincipalType::Engine,
            PrincipalId::new("subscriber").unwrap(),
        );
        let security_context = SecurityContext::new(principal, None);

        let subscription = subscription()
            .with_security_context(security_context)
            .with_authorizer(Arc::new(RecordingAuthorizer))
            .requiring_capability(CapabilityId::new("events.read").unwrap());

        assert_eq!(
            subscription.authorization_decision().unwrap(),
            AuthorizationDecision::Allow
        );
    }
}
