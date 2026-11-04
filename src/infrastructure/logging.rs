use std::fmt;
use std::path::Path;

use chrono::Utc;
use serde_json::{Map, Value, json};
use tracing::field::{Field, Visit};
use tracing::{Event, Level, Subscriber};
use tracing_appender::non_blocking::WorkerGuard;
use tracing_subscriber::EnvFilter;
use tracing_subscriber::fmt::format::Writer;
use tracing_subscriber::fmt::{FmtContext, FormatEvent, FormatFields};
use tracing_subscriber::registry::LookupSpan;
use uuid::Uuid;

use crate::i18n;

const DEFAULT_FILTER: &str = "info";
const SERVICE_NAME: &str = "lumi";

struct StructuredJsonFormatter {
    session_request_id: String,
}

#[derive(Default)]
struct JsonFieldVisitor {
    fields: Map<String, Value>,
    message_key: Option<String>,
}

impl Visit for JsonFieldVisitor {
    fn record_str(&mut self, field: &Field, value: &str) {
        if field.name() == "event" || field.name() == "message" {
            self.message_key = Some(value.to_owned());
        } else {
            self.fields
                .insert(field.name().to_owned(), Value::String(value.to_owned()));
        }
    }

    fn record_u64(&mut self, field: &Field, value: u64) {
        self.fields.insert(field.name().to_owned(), json!(value));
    }

    fn record_i64(&mut self, field: &Field, value: i64) {
        self.fields.insert(field.name().to_owned(), json!(value));
    }

    fn record_bool(&mut self, field: &Field, value: bool) {
        self.fields
            .insert(field.name().to_owned(), Value::Bool(value));
    }

    fn record_error(&mut self, field: &Field, value: &(dyn std::error::Error + 'static)) {
        let mut chain = vec![value.to_string()];
        let mut cursor = value.source();
        while let Some(cause) = cursor {
            chain.push(cause.to_string());
            cursor = cause.source();
        }
        self.fields
            .insert(field.name().to_owned(), Value::String(chain.join(" -> ")));
    }

    fn record_debug(&mut self, field: &Field, value: &dyn fmt::Debug) {
        let formatted = format!("{value:?}");
        if field.name() == "event" || field.name() == "message" {
            self.message_key = Some(formatted.trim_matches('"').to_owned());
        } else {
            self.fields
                .insert(field.name().to_owned(), Value::String(formatted));
        }
    }
}

impl<S, N> FormatEvent<S, N> for StructuredJsonFormatter
where
    S: Subscriber + for<'a> LookupSpan<'a>,
    N: for<'a> FormatFields<'a> + 'static,
{
    fn format_event(
        &self,
        _ctx: &FmtContext<'_, S, N>,
        mut writer: Writer<'_>,
        event: &Event<'_>,
    ) -> fmt::Result {
        let mut visitor = JsonFieldVisitor::default();
        event.record(&mut visitor);
        let payload = self.build_payload(event.metadata(), visitor);
        let serialized = serde_json::to_string(&Value::Object(payload)).map_err(|_| fmt::Error)?;
        writeln!(writer, "{serialized}")
    }
}

impl StructuredJsonFormatter {
    fn build_payload(
        &self,
        metadata: &tracing::Metadata<'_>,
        visitor: JsonFieldVisitor,
    ) -> Map<String, Value> {
        let key = visitor
            .message_key
            .unwrap_or_else(|| metadata.target().to_owned());
        let mut payload = Map::new();
        payload.insert("timestamp".into(), Value::String(Utc::now().to_rfc3339()));
        payload.insert(
            "level".into(),
            Value::String(metadata.level().as_str().to_owned()),
        );
        payload.insert("service".into(), Value::String(SERVICE_NAME.into()));
        payload.insert(
            "requestId".into(),
            Value::String(self.session_request_id.clone()),
        );
        payload.insert("messageKey".into(), Value::String(key.clone()));
        payload.insert("message".into(), Value::String(i18n::translate(&key)));
        if *metadata.level() == Level::ERROR {
            if let Some(file) = metadata.file() {
                payload.insert("file".into(), Value::String(file.into()));
            }
            if let Some(line) = metadata.line() {
                payload.insert("line".into(), json!(line));
            }
        }
        payload.extend(visitor.fields);
        payload
    }
}

/// Installs non-blocking structured JSON logging to a daily-rotated file.
pub fn init_logging(directory: &Path) -> WorkerGuard {
    let appender = tracing_appender::rolling::daily(directory.join("logs"), "lumi.log");
    let (writer, guard) = tracing_appender::non_blocking(appender);
    let filter =
        EnvFilter::try_from_env("LUMI_LOG").unwrap_or_else(|_| EnvFilter::new(DEFAULT_FILTER));
    let formatter = StructuredJsonFormatter {
        session_request_id: Uuid::new_v4().to_string(),
    };
    tracing_subscriber::fmt()
        .with_env_filter(filter)
        .with_writer(writer)
        .event_format(formatter)
        .init();
    guard
}
