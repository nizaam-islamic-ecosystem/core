use super::LogEvent;

/// Destination for validated log events.
pub trait LogSink: Send + Sync {
    fn publish(&self, event: &LogEvent);
}
