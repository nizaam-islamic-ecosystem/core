/// Failure returned when a structured log event violates the Core contract.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum LogValidationError {
    EmptyComponent,
    EmptyMessage,
    GlobalEventHasEngineContext,
    LocalEventNeedsEngineContext,
    GlobalEventHasEngineSource,
    SourceContextMismatch,
    EmptyMetadataField,
}
