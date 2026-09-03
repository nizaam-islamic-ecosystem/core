use std::time::Duration;

use nizaam_core::prelude::*;

#[test]
fn consumer_can_propagate_engine_context_to_downstream_work() {
    let operation = Operation::new(
        OperationId::new("operation-5").unwrap(),
        CorrelationId::new("correlation-5").unwrap(),
    );
    let context = EngineContext::new(OperationContext::new(operation))
        .with_deadline(Deadline::from_now(Duration::from_secs(1)))
        .with_security(SecurityContext::new())
        .with_provenance(ProvenanceContext::new());
    let child = context.child_with_deadline(Deadline::from_now(Duration::from_secs(2)));

    assert_eq!(child.operation().operation.id.as_str(), "operation-5");
    assert_eq!(
        child.operation().operation.correlation_id.as_str(),
        "correlation-5"
    );
    assert_eq!(child.security(), context.security());
    assert_eq!(child.provenance(), context.provenance());
    assert!(!child.cancellation().is_cancelled());
    assert!(!child.is_expired());

    context.cancellation().cancel();

    assert!(child.cancellation().is_cancelled());
}
