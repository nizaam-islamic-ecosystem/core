use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;
use std::fmt;
use std::time::{Duration, Instant, SystemTime};

pub const MAX_TRACE_ID_LENGTH: usize = 128;
pub const MAX_SPAN_ID_LENGTH: usize = 128;
pub const MAX_SPAN_NAME_LENGTH: usize = 128;
pub const MAX_SPAN_ATTRIBUTES: usize = 32;
pub const MAX_SPAN_ATTRIBUTE_LENGTH: usize = 256;
pub const MAX_SPAN_EVENTS: usize = 64;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum TraceError {
    InvalidTraceId,
    InvalidSpanId,
    InvalidSpanName,
    SpanIdMatchesParent,
    InvalidAttribute,
    AttributeLimitExceeded,
    EventLimitExceeded,
}

impl fmt::Display for TraceError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::InvalidTraceId => write!(f, "invalid trace id"),
            Self::InvalidSpanId => write!(f, "invalid span id"),
            Self::InvalidSpanName => write!(f, "invalid span name"),
            Self::SpanIdMatchesParent => write!(f, "span id matches parent span id"),
            Self::InvalidAttribute => write!(f, "invalid span attribute"),
            Self::AttributeLimitExceeded => write!(f, "span attribute limit exceeded"),
            Self::EventLimitExceeded => write!(f, "span event limit exceeded"),
        }
    }
}

impl std::error::Error for TraceError {}

#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, PartialOrd, Ord)]
pub struct TraceId(String);

impl<'de> Deserialize<'de> for TraceId {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        let value = String::deserialize(deserializer)?;
        TraceId::new(value).map_err(serde::de::Error::custom)
    }
}

impl TraceId {
    pub fn new(value: impl Into<String>) -> Result<Self, TraceError> {
        let value = value.into();
        if !validate_identifier(&value, MAX_TRACE_ID_LENGTH) {
            return Err(TraceError::InvalidTraceId);
        }
        Ok(Self(value))
    }

    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl fmt::Display for TraceId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.0)
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, PartialOrd, Ord)]
pub struct SpanId(String);

impl<'de> Deserialize<'de> for SpanId {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        let value = String::deserialize(deserializer)?;
        SpanId::new(value).map_err(serde::de::Error::custom)
    }
}

impl SpanId {
    pub fn new(value: impl Into<String>) -> Result<Self, TraceError> {
        let value = value.into();
        if !validate_identifier(&value, MAX_SPAN_ID_LENGTH) {
            return Err(TraceError::InvalidSpanId);
        }
        Ok(Self(value))
    }

    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl fmt::Display for SpanId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.0)
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct TraceContext {
    trace_id: TraceId,
    span_id: SpanId,
}

impl TraceContext {
    pub fn new(trace_id: TraceId, span_id: SpanId) -> Self {
        Self { trace_id, span_id }
    }

    pub fn trace_id(&self) -> &TraceId {
        &self.trace_id
    }

    pub fn span_id(&self) -> &SpanId {
        &self.span_id
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Default)]
pub struct SpanAttributes(BTreeMap<String, String>);

impl<'de> Deserialize<'de> for SpanAttributes {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        let values = BTreeMap::<String, String>::deserialize(deserializer)?;
        let mut attributes = Self::new();

        for (key, value) in values {
            attributes
                .insert(key, value)
                .map_err(serde::de::Error::custom)?;
        }

        Ok(attributes)
    }
}

impl SpanAttributes {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn insert(
        &mut self,
        key: impl Into<String>,
        value: impl Into<String>,
    ) -> Result<(), TraceError> {
        let key = key.into();
        let value = value.into();

        if key.trim().is_empty()
            || value.trim().is_empty()
            || key.len() > MAX_SPAN_ATTRIBUTE_LENGTH
            || value.len() > MAX_SPAN_ATTRIBUTE_LENGTH
        {
            return Err(TraceError::InvalidAttribute);
        }

        if !self.0.contains_key(&key) && self.0.len() >= MAX_SPAN_ATTRIBUTES {
            return Err(TraceError::AttributeLimitExceeded);
        }

        self.0.insert(key, value);
        Ok(())
    }

    pub fn get(&self, key: &str) -> Option<&str> {
        self.0.get(key).map(String::as_str)
    }

    pub fn iter(&self) -> impl Iterator<Item = (&str, &str)> {
        self.0
            .iter()
            .map(|(key, value)| (key.as_str(), value.as_str()))
    }

    pub fn is_empty(&self) -> bool {
        self.0.is_empty()
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct SpanEvent {
    name: String,
    timestamp: SystemTime,
    attributes: SpanAttributes,
}

#[derive(Deserialize)]
struct SpanEventWire {
    name: String,
    timestamp: SystemTime,
    attributes: SpanAttributes,
}

impl<'de> Deserialize<'de> for SpanEvent {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        let wire = SpanEventWire::deserialize(deserializer)?;
        let mut event =
            SpanEvent::new(wire.name, wire.timestamp).map_err(serde::de::Error::custom)?;

        for (key, value) in wire.attributes.iter() {
            event
                .set_attribute(key, value)
                .map_err(serde::de::Error::custom)?;
        }

        Ok(event)
    }
}

impl SpanEvent {
    pub fn new(name: impl Into<String>, timestamp: SystemTime) -> Result<Self, TraceError> {
        let name = name.into();
        if !validate_identifier(&name, MAX_SPAN_NAME_LENGTH) {
            return Err(TraceError::InvalidSpanName);
        }

        Ok(Self {
            name,
            timestamp,
            attributes: SpanAttributes::new(),
        })
    }

    pub fn name(&self) -> &str {
        &self.name
    }

    pub fn timestamp(&self) -> SystemTime {
        self.timestamp
    }

    pub fn attributes(&self) -> &SpanAttributes {
        &self.attributes
    }

    pub fn set_attribute(
        &mut self,
        key: impl Into<String>,
        value: impl Into<String>,
    ) -> Result<(), TraceError> {
        self.attributes.insert(key, value)
    }
}

#[derive(Debug)]
pub struct Span {
    trace_id: TraceId,
    span_id: SpanId,
    parent_span_id: Option<SpanId>,
    name: String,
    started_at: SystemTime,
    started_instant: Instant,
    attributes: SpanAttributes,
    events: Vec<SpanEvent>,
}

impl Span {
    pub fn root(
        trace_id: TraceId,
        span_id: SpanId,
        name: impl Into<String>,
    ) -> Result<Self, TraceError> {
        Self::create(trace_id, span_id, None, name)
    }

    pub fn child(
        parent: &Span,
        span_id: SpanId,
        name: impl Into<String>,
    ) -> Result<Self, TraceError> {
        Self::create(
            parent.trace_id.clone(),
            span_id,
            Some(parent.span_id.clone()),
            name,
        )
    }

    fn create(
        trace_id: TraceId,
        span_id: SpanId,
        parent_span_id: Option<SpanId>,
        name: impl Into<String>,
    ) -> Result<Self, TraceError> {
        let name = name.into();
        if !validate_identifier(&name, MAX_SPAN_NAME_LENGTH) {
            return Err(TraceError::InvalidSpanName);
        }

        if parent_span_id.as_ref() == Some(&span_id) {
            return Err(TraceError::SpanIdMatchesParent);
        }

        Ok(Self {
            trace_id,
            span_id,
            parent_span_id,
            name,
            started_at: SystemTime::now(),
            started_instant: Instant::now(),
            attributes: SpanAttributes::new(),
            events: Vec::new(),
        })
    }

    pub fn context(&self) -> TraceContext {
        TraceContext::new(self.trace_id.clone(), self.span_id.clone())
    }

    pub fn trace_id(&self) -> &TraceId {
        &self.trace_id
    }

    pub fn span_id(&self) -> &SpanId {
        &self.span_id
    }

    pub fn parent_span_id(&self) -> Option<&SpanId> {
        self.parent_span_id.as_ref()
    }

    pub fn name(&self) -> &str {
        &self.name
    }

    pub fn started_at(&self) -> SystemTime {
        self.started_at
    }

    pub fn attributes(&self) -> &SpanAttributes {
        &self.attributes
    }

    pub fn events(&self) -> &[SpanEvent] {
        &self.events
    }

    pub fn set_attribute(
        &mut self,
        key: impl Into<String>,
        value: impl Into<String>,
    ) -> Result<(), TraceError> {
        self.attributes.insert(key, value)
    }

    pub fn add_event(&mut self, event: SpanEvent) -> Result<(), TraceError> {
        if self.events.len() >= MAX_SPAN_EVENTS {
            return Err(TraceError::EventLimitExceeded);
        }
        self.events.push(event);
        Ok(())
    }

    pub fn duration(&self) -> Duration {
        self.started_instant.elapsed()
    }

    pub fn finish(self) -> CompletedSpan {
        let ended_at = SystemTime::now();
        let duration = self.started_instant.elapsed();

        CompletedSpan {
            trace_id: self.trace_id,
            span_id: self.span_id,
            parent_span_id: self.parent_span_id,
            name: self.name,
            started_at: self.started_at,
            ended_at,
            duration,
            attributes: self.attributes,
            events: self.events,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct CompletedSpan {
    trace_id: TraceId,
    span_id: SpanId,
    parent_span_id: Option<SpanId>,
    name: String,
    started_at: SystemTime,
    ended_at: SystemTime,
    duration: Duration,
    attributes: SpanAttributes,
    events: Vec<SpanEvent>,
}

#[derive(Deserialize)]
struct CompletedSpanWire {
    trace_id: TraceId,
    span_id: SpanId,
    parent_span_id: Option<SpanId>,
    name: String,
    started_at: SystemTime,
    ended_at: SystemTime,
    duration: Duration,
    attributes: SpanAttributes,
    events: Vec<SpanEvent>,
}

impl<'de> Deserialize<'de> for CompletedSpan {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        let wire = CompletedSpanWire::deserialize(deserializer)?;

        if !validate_identifier(&wire.name, MAX_SPAN_NAME_LENGTH) {
            return Err(serde::de::Error::custom(TraceError::InvalidSpanName));
        }

        if wire.events.len() > MAX_SPAN_EVENTS {
            return Err(serde::de::Error::custom(TraceError::EventLimitExceeded));
        }

        if wire.parent_span_id.as_ref() == Some(&wire.span_id) {
            return Err(serde::de::Error::custom(TraceError::SpanIdMatchesParent));
        }

        Ok(Self {
            trace_id: wire.trace_id,
            span_id: wire.span_id,
            parent_span_id: wire.parent_span_id,
            name: wire.name,
            started_at: wire.started_at,
            ended_at: wire.ended_at,
            duration: wire.duration,
            attributes: wire.attributes,
            events: wire.events,
        })
    }
}

impl CompletedSpan {
    pub fn trace_id(&self) -> &TraceId {
        &self.trace_id
    }

    pub fn span_id(&self) -> &SpanId {
        &self.span_id
    }

    pub fn parent_span_id(&self) -> Option<&SpanId> {
        self.parent_span_id.as_ref()
    }

    pub fn name(&self) -> &str {
        &self.name
    }

    pub fn started_at(&self) -> SystemTime {
        self.started_at
    }

    pub fn ended_at(&self) -> SystemTime {
        self.ended_at
    }

    pub fn duration(&self) -> Duration {
        self.duration
    }

    pub fn attributes(&self) -> &SpanAttributes {
        &self.attributes
    }

    pub fn events(&self) -> &[SpanEvent] {
        &self.events
    }
}

fn validate_identifier(value: &str, max_length: usize) -> bool {
    !value.trim().is_empty() && value.len() <= max_length
}

#[cfg(test)]
mod tests {
    use super::*;

    fn trace_id() -> TraceId {
        TraceId::new("trace-1").unwrap()
    }

    fn span_id(value: &str) -> SpanId {
        SpanId::new(value).unwrap()
    }

    #[test]
    fn trace_id_validates_and_round_trips() {
        let id = TraceId::new("trace-1").unwrap();
        assert_eq!(id.as_str(), "trace-1");
        assert_eq!(id.to_string(), "trace-1");

        let encoded = serde_json::to_string(&id).unwrap();
        let decoded: TraceId = serde_json::from_str(&encoded).unwrap();
        assert_eq!(decoded, id);
    }

    #[test]
    fn empty_or_oversized_trace_id_is_rejected() {
        assert_eq!(TraceId::new(" ").unwrap_err(), TraceError::InvalidTraceId);
        assert_eq!(
            TraceId::new("x".repeat(MAX_TRACE_ID_LENGTH + 1)).unwrap_err(),
            TraceError::InvalidTraceId
        );
    }

    #[test]
    fn span_id_validates_and_round_trips() {
        let id = SpanId::new("span-1").unwrap();
        let encoded = serde_json::to_string(&id).unwrap();
        let decoded: SpanId = serde_json::from_str(&encoded).unwrap();
        assert_eq!(decoded, id);
    }

    #[test]
    fn trace_context_preserves_trace_and_span_ids() {
        let context = TraceContext::new(trace_id(), span_id("span-1"));
        assert_eq!(context.trace_id().as_str(), "trace-1");
        assert_eq!(context.span_id().as_str(), "span-1");

        let encoded = serde_json::to_string(&context).unwrap();
        let decoded: TraceContext = serde_json::from_str(&encoded).unwrap();
        assert_eq!(decoded, context);
    }

    #[test]
    fn root_span_has_no_parent() {
        let span = Span::root(trace_id(), span_id("root"), "request").unwrap();
        assert_eq!(span.trace_id().as_str(), "trace-1");
        assert_eq!(span.span_id().as_str(), "root");
        assert!(span.parent_span_id().is_none());
        assert_eq!(span.name(), "request");
    }

    #[test]
    fn child_span_reuses_trace_and_references_parent() {
        let parent = Span::root(trace_id(), span_id("root"), "request").unwrap();
        let child = Span::child(&parent, span_id("child"), "database").unwrap();

        assert_eq!(child.trace_id(), parent.trace_id());
        assert_ne!(child.span_id(), parent.span_id());
        assert_eq!(child.parent_span_id(), Some(parent.span_id()));
    }

    #[test]
    fn child_span_rejects_self_parent() {
        let parent = Span::root(trace_id(), span_id("root"), "request").unwrap();

        assert_eq!(
            Span::child(&parent, span_id("root"), "database").unwrap_err(),
            TraceError::SpanIdMatchesParent
        );
    }

    #[test]
    fn sibling_spans_share_trace_and_parent() {
        let parent = Span::root(trace_id(), span_id("root"), "request").unwrap();
        let left = Span::child(&parent, span_id("left"), "left").unwrap();
        let right = Span::child(&parent, span_id("right"), "right").unwrap();

        assert_eq!(left.trace_id(), right.trace_id());
        assert_ne!(left.span_id(), right.span_id());
        assert_eq!(left.parent_span_id(), Some(parent.span_id()));
        assert_eq!(right.parent_span_id(), Some(parent.span_id()));
    }

    #[test]
    fn attributes_are_bounded_and_replace_existing_values() {
        let mut attributes = SpanAttributes::new();
        attributes.insert("component", "core").unwrap();
        attributes.insert("component", "engine").unwrap();

        assert_eq!(attributes.get("component"), Some("engine"));
        assert!(!attributes.is_empty());

        assert_eq!(
            attributes.insert(" ", "value"),
            Err(TraceError::InvalidAttribute)
        );
        assert_eq!(
            attributes.insert("key", ""),
            Err(TraceError::InvalidAttribute)
        );
        assert_eq!(
            attributes.insert("x".repeat(MAX_SPAN_ATTRIBUTE_LENGTH + 1), "value"),
            Err(TraceError::InvalidAttribute)
        );
    }

    #[test]
    fn attribute_limit_is_enforced() {
        let mut attributes = SpanAttributes::new();
        for index in 0..MAX_SPAN_ATTRIBUTES {
            attributes.insert(format!("key-{index}"), "value").unwrap();
        }

        assert_eq!(
            attributes.insert("overflow", "value"),
            Err(TraceError::AttributeLimitExceeded)
        );
    }

    #[test]
    fn event_records_name_timestamp_and_attributes() {
        let timestamp = SystemTime::UNIX_EPOCH;
        let mut event = SpanEvent::new("cache-hit", timestamp).unwrap();
        event.set_attribute("cache", "users").unwrap();

        assert_eq!(event.name(), "cache-hit");
        assert_eq!(event.timestamp(), timestamp);
        assert_eq!(event.attributes().get("cache"), Some("users"));
    }

    #[test]
    fn event_limit_is_enforced() {
        let mut span = Span::root(trace_id(), span_id("root"), "request").unwrap();
        let event = SpanEvent::new("event", SystemTime::UNIX_EPOCH).unwrap();

        for _ in 0..MAX_SPAN_EVENTS {
            span.add_event(event.clone()).unwrap();
        }

        assert_eq!(span.add_event(event), Err(TraceError::EventLimitExceeded));
    }

    #[test]
    fn span_context_and_duration_are_available() {
        let span = Span::root(trace_id(), span_id("root"), "request").unwrap();
        let context = span.context();
        let duration = span.duration();

        assert_eq!(context.trace_id(), span.trace_id());
        assert_eq!(context.span_id(), span.span_id());
        assert!(duration >= Duration::ZERO);
    }

    #[test]
    fn finished_span_contains_end_time_and_duration() {
        let span = Span::root(trace_id(), span_id("root"), "request").unwrap();
        let completed = span.finish();

        assert!(completed.ended_at() >= completed.started_at());
        assert!(completed.duration() >= Duration::ZERO);
        assert_eq!(completed.name(), "request");
        assert!(completed.parent_span_id().is_none());
    }

    #[test]
    fn deserialization_rejects_invalid_trace_and_span_ids() {
        assert!(serde_json::from_str::<TraceId>(r#"""#).is_err());
        assert!(serde_json::from_str::<SpanId>(r#"""#).is_err());

        let oversized = format!(r#""{}""#, "x".repeat(MAX_TRACE_ID_LENGTH + 1));
        assert!(serde_json::from_str::<TraceId>(&oversized).is_err());
    }

    #[test]
    fn deserialization_rejects_oversized_span_attributes() {
        let mut object = String::from("{");
        for index in 0..=MAX_SPAN_ATTRIBUTES {
            if index > 0 {
                object.push(',');
            }
            object.push_str(&format!(r#""key-{index}":"value""#));
        }
        object.push('}');

        assert!(serde_json::from_str::<SpanAttributes>(&object).is_err());
    }

    #[test]
    fn deserialization_rejects_invalid_span_event_name() {
        let value = r#"{"name":"","timestamp":"2026-01-01T00:00:00Z","attributes":{}}"#;
        assert!(serde_json::from_str::<SpanEvent>(value).is_err());
    }

    #[test]
    fn completed_span_deserialization_rejects_self_parent() {
        let span = Span::root(trace_id(), span_id("root"), "request").unwrap();
        let completed = span.finish();
        let span_id = completed.span_id().to_string();
        let mut value = serde_json::to_value(completed).unwrap();

        value["parent_span_id"] = serde_json::Value::String(span_id);

        let error = serde_json::from_value::<CompletedSpan>(value).unwrap_err();
        assert_eq!(
            error.to_string(),
            TraceError::SpanIdMatchesParent.to_string()
        );
    }

    #[test]
    fn completed_span_deserialization_rejects_invalid_name_and_event_count() {
        let span = Span::root(trace_id(), span_id("root"), "request").unwrap();
        let mut value = serde_json::to_value(span.finish()).unwrap();

        value["name"] = serde_json::Value::String(String::new());
        assert!(serde_json::from_value::<CompletedSpan>(value.clone()).is_err());

        let event =
            serde_json::to_value(SpanEvent::new("event", SystemTime::UNIX_EPOCH).unwrap()).unwrap();
        let events = vec![event; MAX_SPAN_EVENTS + 1];

        value["name"] = serde_json::Value::String("request".to_owned());
        value["events"] = serde_json::Value::Array(events);

        assert!(serde_json::from_value::<CompletedSpan>(value).is_err());
    }

    #[test]
    fn completed_span_is_serializable() {
        let span = Span::root(trace_id(), span_id("root"), "request").unwrap();
        let completed = span.finish();
        let encoded = serde_json::to_string(&completed).unwrap();
        let decoded: CompletedSpan = serde_json::from_str(&encoded).unwrap();

        assert_eq!(decoded.trace_id(), completed.trace_id());
        assert_eq!(decoded.span_id(), completed.span_id());
        assert_eq!(decoded.name(), completed.name());
        assert_eq!(decoded.duration(), completed.duration());
        assert_eq!(decoded.events(), completed.events());
    }

    #[test]
    fn invalid_span_name_is_rejected() {
        assert_eq!(
            Span::root(trace_id(), span_id("root"), " ").unwrap_err(),
            TraceError::InvalidSpanName
        );
        assert_eq!(
            SpanEvent::new("x".repeat(MAX_SPAN_NAME_LENGTH + 1), SystemTime::now()).unwrap_err(),
            TraceError::InvalidSpanName
        );
    }
}
