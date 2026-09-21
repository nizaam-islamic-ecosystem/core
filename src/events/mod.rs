//! Internal Event infrastructure for Nizaam Core.
//!
//! `events::mod` is the composition boundary for the Phase 14 Event subsystem.
//! It exposes the stable semantic Event API while keeping delivery machinery,
//! publisher coordination, and Event-subsystem lifecycle state internal to
//! Core.
//!
//! The child modules retain their own responsibilities:
//!
//! ```text
//! event.rs
//!     → immutable event occurrence
//!
//! scope.rs
//!     → exact generic scope value
//!
//! subscriber.rs
//!     → subscriber and subscription declaration
//!
//! publisher.rs
//!     → registration, matching, publication admission, publisher lifecycle
//!
//! delivery.rs
//!     → bounded delivery queues, workers, ordering, overload, isolation
//!
//! lifecycle.rs
//!     → Event-subsystem publication admission lifecycle
//! ```
//!
//! No EventBus, transport, persistence, replay, retry, idempotency, or
//! event-specific authorization framework is introduced here.

mod delivery;
mod event;
mod lifecycle;
mod publisher;
mod scope;
mod subscriber;

// -----------------------------------------------------------------------------
// Stable semantic Event API
// -----------------------------------------------------------------------------

pub use event::{Event, EventContext, EventCreationError, EventName, InvalidEventName};
pub use scope::{InvalidScope, Scope};
pub use subscriber::{
    EventSubscriber, EventSubscription, SubscriptionCreationError, SubscriptionLifecycleError,
    SubscriptionLifecycleState,
};

// -----------------------------------------------------------------------------
// Internal Event infrastructure shared by sibling Core modules
// -----------------------------------------------------------------------------

pub use delivery::{
    DeliveryConfig, DeliveryDispatcher, DeliveryError, DeliveryHandle, DeliveryOutcome,
};

pub use lifecycle::{EventLifecycle, EventLifecycleState};

pub use publisher::{
    EventPublisher, Publication, PublisherError, PublisherLifecycleError, PublisherLifecycleState,
};

#[cfg(test)]
mod tests {
    use super::{
        delivery::{DeliveryConfig, DeliveryDispatcher, DeliveryOutcome},
        event::{Event, EventContext, EventName},
        lifecycle::{EventLifecycle, EventLifecycleState},
        publisher::{EventPublisher, PublisherError, PublisherLifecycleState},
        scope::Scope,
        subscriber::{EventSubscription, SubscriptionLifecycleState},
    };
    use crate::identity::EventId;
    use crate::operation::CancellationToken;
    use std::sync::{Arc, mpsc};
    use std::time::Duration;

    fn scope() -> Scope {
        Scope::new("engine:test").unwrap()
    }

    fn event() -> Event {
        Event::new(
            EventId::new("event-1").unwrap(),
            EventName::new("test.event").unwrap(),
            "test.event",
            scope(),
        )
        .unwrap()
    }

    fn event_lifecycle() -> Arc<EventLifecycle> {
        Arc::new(EventLifecycle::new())
    }

    fn publisher() -> EventPublisher {
        EventPublisher::new(event_lifecycle(), &CancellationToken::new())
    }

    #[test]
    fn stable_semantic_types_are_reexported_from_events_root() {
        let event = event();
        let scope = scope();
        let owner = CancellationToken::new();

        let subscription = EventSubscription::new(
            EventName::new("test.event").unwrap(),
            "test.event",
            scope.clone(),
            |_event: &Event| {},
            &owner,
        )
        .unwrap();

        assert_eq!(event.event_id().as_str(), "event-1");
        assert_eq!(scope.as_str(), "engine:test");
        assert_eq!(subscription.event_type(), "test.event");
        assert_eq!(subscription.scope(), &scope);
    }

    #[test]
    fn event_lifecycle_and_publisher_compose_through_events_root() {
        let lifecycle = event_lifecycle();
        let publisher = EventPublisher::new(Arc::clone(&lifecycle), &CancellationToken::new());

        assert_eq!(lifecycle.state(), EventLifecycleState::Serving);
        assert_eq!(publisher.state(), PublisherLifecycleState::Created);

        publisher.activate().unwrap();

        assert!(publisher.is_active());
        assert!(lifecycle.allows_publication());
    }

    #[test]
    fn publisher_and_subscription_compose_through_events_root() {
        let publisher = publisher();
        let owner = CancellationToken::new();

        publisher.activate().unwrap();

        let subscription = EventSubscription::new(
            EventName::new("test.event").unwrap(),
            "test.event",
            scope(),
            |_event: &Event| {},
            &owner,
        )
        .unwrap();

        let registered = publisher.subscribe(subscription).unwrap();

        assert_eq!(publisher.subscription_count(), 1);
        assert_eq!(registered.state(), SubscriptionLifecycleState::Active);
        assert_eq!(registered.event_type(), "test.event");
    }

    #[test]
    fn publisher_and_delivery_compose_through_events_root() {
        let dispatcher = DeliveryDispatcher::new(
            DeliveryConfig::new(4, 1, 4).unwrap(),
            CancellationToken::new(),
        )
        .unwrap();

        let owner = CancellationToken::new();
        let (sender, receiver) = mpsc::channel();

        let subscription = Arc::new(
            EventSubscription::new(
                EventName::new("test.event").unwrap(),
                "test.event",
                scope(),
                move |event: &Event| {
                    sender.send(event.event_id().as_str().to_owned()).unwrap();
                },
                &owner,
            )
            .unwrap(),
        );
        subscription.activate().unwrap();

        let handle = dispatcher.register(Arc::clone(&subscription)).unwrap();

        assert_eq!(
            handle.enqueue(Arc::new(event())).unwrap(),
            DeliveryOutcome::Accepted
        );
        assert_eq!(
            receiver.recv_timeout(Duration::from_secs(2)).unwrap(),
            "event-1"
        );

        dispatcher.shutdown();
    }

    #[test]
    fn event_root_preserves_subsystem_lifecycle_boundary() {
        let lifecycle = event_lifecycle();
        let publisher = EventPublisher::new(Arc::clone(&lifecycle), &CancellationToken::new());

        publisher.activate().unwrap();
        lifecycle.begin_draining();

        assert!(!lifecycle.allows_publication());
        assert!(matches!(
            publisher.publish(event()),
            Err(PublisherError::EventSubsystemUnavailable)
        ));
    }

    #[test]
    fn event_context_is_reexported_and_composes_with_event() {
        use crate::{
            identity::{CorrelationId, OperationId},
            operation::{Operation, OperationContext},
            security::SecurityContext,
        };

        let operation_context = OperationContext::new(Operation::new(
            OperationId::new("operation-2").unwrap(),
            CorrelationId::new("correlation-2").unwrap(),
        ));

        let context = EventContext::empty().with_operation_context(operation_context.clone());
        let event = Event::new_with_context(
            EventId::new("event-context-2").unwrap(),
            EventName::new("test.event").unwrap(),
            "test.event",
            scope(),
            context,
        )
        .unwrap();

        assert_eq!(
            event.context().operation_context(),
            Some(&operation_context)
        );
        assert!(event.context().security_context().is_none());

        let _: Option<&SecurityContext> = event.context().security_context();
    }

    #[test]
    fn subscription_uses_existing_security_authorization_api() {
        use crate::security::{
            AuthorizationDecision, AuthorizationRequest, Authorizer, SecurityContext,
            identity::{PrincipalId, PrincipalIdentity, PrincipalType},
        };

        struct AllowAuthorizer;

        impl Authorizer for AllowAuthorizer {
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

        let lifecycle = event_lifecycle();
        let publisher = EventPublisher::new(Arc::clone(&lifecycle), &CancellationToken::new());
        publisher.activate().unwrap();

        let owner = CancellationToken::new();
        let subscriber_principal = PrincipalIdentity::new(
            PrincipalType::Engine,
            PrincipalId::new("subscriber").unwrap(),
        );
        let security_context = SecurityContext::new(subscriber_principal, None);

        let subscription = EventSubscription::new(
            EventName::new("test.event").unwrap(),
            "test.event",
            scope(),
            |_event: &Event| {},
            &owner,
        )
        .unwrap()
        .with_security_context(security_context)
        .with_authorizer(Arc::new(AllowAuthorizer))
        .requiring_capability(crate::identity::CapabilityId::new("events.read").unwrap());

        let registered = publisher.subscribe(subscription).unwrap();

        assert_eq!(
            registered.authorization_decision().unwrap(),
            AuthorizationDecision::Allow
        );
    }
}
