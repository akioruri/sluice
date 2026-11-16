//! Request logging policy.
//!
//! The gateway is a pass-through, so it sees every request a client makes —
//! including any credential that client chose to put in a URL query string. The
//! default `TraceLayer` span records the whole URI, query included, which would
//! copy such a credential into the logs.

use tower_http::trace::MakeSpan;

/// Builds the per-request span, recording only what is safe to log.
///
/// Deliberately records `path` and not `uri`: `Uri::to_string()` includes the
/// query string, and some APIs accept a key there (`?api-key=…`, `?key=…`,
/// `?access_token=…`). The README promises that logs never contain keys, and a
/// query string is one place a client could quietly break that promise, so it is
/// dropped rather than filtered.
///
/// Losing the query string costs a little debuggability — you can no longer see
/// `?stream=true` on a request line — which is the right trade for a component
/// whose whole job is holding credentials.
#[derive(Clone, Copy, Debug, Default)]
pub struct PathOnlySpan;

impl<B> MakeSpan<B> for PathOnlySpan {
    fn make_span(&mut self, request: &axum::http::Request<B>) -> tracing::Span {
        tracing::span!(
            tracing::Level::INFO,
            "request",
            method = %request.method(),
            path = %request.uri().path(),
            version = ?request.version(),
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::{Arc, Mutex};

    /// Collects the fields recorded on every span created while installed.
    #[derive(Clone, Default)]
    struct SpanFields(Arc<Mutex<Vec<(String, String)>>>);

    struct Collector(Vec<(String, String)>);

    /// Values arrive debug-formatted: `method` is `Display`-recorded by the
    /// span macro, which lands in `record_debug`.
    impl tracing::field::Visit for Collector {
        fn record_debug(&mut self, field: &tracing::field::Field, value: &dyn std::fmt::Debug) {
            self.0
                .push((field.name().to_string(), format!("{value:?}")));
        }
    }

    impl<S: tracing::Subscriber> tracing_subscriber::Layer<S> for SpanFields {
        fn on_new_span(
            &self,
            attrs: &tracing::span::Attributes<'_>,
            _id: &tracing::span::Id,
            _ctx: tracing_subscriber::layer::Context<'_, S>,
        ) {
            if attrs.metadata().name() != "request" {
                return;
            }
            let mut collector = Collector(Vec::new());
            attrs.record(&mut collector);
            *self.0.lock().unwrap() = collector.0;
        }
    }

    /// Records what the span would log for `uri`, with a subscriber installed
    /// just long enough to observe it.
    fn fields_for(uri: &str) -> Vec<(String, String)> {
        let recorder = SpanFields::default();
        let subscriber = tracing_subscriber::layer::SubscriberExt::with(
            tracing_subscriber::registry(),
            recorder.clone(),
        );
        let _guard = tracing::subscriber::set_default(subscriber);

        let request = axum::http::Request::builder()
            .method("POST")
            .uri(uri)
            .body(axum::body::Body::empty())
            .unwrap();

        let mut make_span = PathOnlySpan;
        let _span = make_span.make_span(&request);
        drop(_guard);

        recorder.0.lock().unwrap().clone()
    }

    #[test]
    fn the_query_string_is_not_recorded() {
        let fields = fields_for(
            "http://localhost/openai/v1/messages?api-key=SUPERSECRET&access_token=ALSOSECRET",
        );

        let rendered = fields
            .iter()
            .map(|(name, value)| format!("{name}={value}"))
            .collect::<Vec<_>>()
            .join(" ");

        assert!(
            !rendered.contains("SUPERSECRET") && !rendered.contains("ALSOSECRET"),
            "a query-string secret reached the span: {rendered}"
        );
    }

    #[test]
    fn the_path_is_still_recorded_so_requests_remain_traceable() {
        let fields = fields_for("http://localhost/anthropic/v1/messages?stream=true");

        // `method` and `path` carry the debugging signal; `version` is harmless.
        assert_eq!(
            fields,
            vec![
                ("method".to_string(), "POST".to_string()),
                ("path".to_string(), "/anthropic/v1/messages".to_string()),
                ("version".to_string(), "HTTP/1.1".to_string()),
            ]
        );
    }

    /// The field name changed from `uri` to `path`. Anything grepping logs for
    /// `uri=` would silently stop matching, which is worth catching here.
    #[test]
    fn no_uri_field_is_recorded() {
        let fields = fields_for("http://localhost/healthz?token=abc");
        assert!(
            !fields.iter().any(|(name, _)| name == "uri"),
            "the full uri must not be recorded: {fields:?}"
        );
    }
}
