use nizaam_core::prelude::*;

#[test]
fn a_consumer_can_construct_a_domain_agnostic_operation_context() {
    let operation = Operation::new(
        OperationId::new("operation-42").unwrap(),
        CorrelationId::new("correlation-42").unwrap(),
    )
    .with_plan(PlanId::new("plan-1").unwrap());

    let context = OperationContext::new(operation).for_attempt(
        NodeId::new("node-2").unwrap(),
        AttemptId::new("attempt-5").unwrap(),
    );

    assert_eq!(context.operation.plan_id.unwrap().as_str(), "plan-1");
    assert_eq!(context.node_id.unwrap().as_str(), "node-2");
}
