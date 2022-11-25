use opentelemetry::trace::TraceContextExt;
use std::fs::File;

use tracing::{span, Level};
use tracing_subscriber::prelude::*;

use tracing_opentelemetry::{self, OpenTelemetrySpanExt};

use opentelemetry::global::shutdown_tracer_provider;
use opentelemetry::{
    propagation::{Extractor, Injector},
    sdk::{propagation::TraceContextPropagator, Resource},
    KeyValue,
};

use opentelemetry_otlp::{self, WithExportConfig};
use tracing_subscriber::{filter::EnvFilter, filter::LevelFilter, fmt};

pub struct MetadataMap<'a>(pub &'a mut tonic::metadata::MetadataMap);

impl<'a> Injector for MetadataMap<'a> {
    /// Set a key and value in the MetadataMap.  Does nothing if the key or value are not valid inputs
    fn set(&mut self, key: &str, value: String) {
        if let Ok(key) = tonic::metadata::MetadataKey::from_bytes(key.as_bytes()) {
            if let Ok(val) = tonic::metadata::MetadataValue::from_str(&value) {
                self.0.insert(key, val);
            }
        }
    }
}

impl<'a> Extractor for MetadataMap<'a> {
    /// Get a value for a key from the MetadataMap.  If the value can't be converted to &str, returns None
    fn get(&self, key: &str) -> Option<&str> {
        self.0.get(key).and_then(|metadata| metadata.to_str().ok())
    }

    /// Collect all the keys from the MetadataMap.
    fn keys(&self) -> Vec<&str> {
        self.0
            .keys()
            .map(|key| match key {
                tonic::metadata::KeyRef::Ascii(v) => v.as_str(),
                tonic::metadata::KeyRef::Binary(v) => v.as_str(),
            })
            .collect::<Vec<_>>()
    }
}

pub struct OpentelemetryTracer {
    pub m_span: tracing::Span,
}

pub fn inject<T>(mut request: tonic::Request<T>) -> tonic::Request<T> {
    opentelemetry::global::get_text_map_propagator(|propagator| {
        propagator.inject_context(
            &tracing::Span::current().context(),
            &mut MetadataMap(request.metadata_mut()),
        )
    });
    request
}

pub fn extract<T>(mut request: tonic::Request<T>) -> tonic::Request<T> {
    let parent_cx = opentelemetry::global::get_text_map_propagator(|prop| {
        prop.extract(&MetadataMap(request.metadata_mut()))
    });

    tracing::Span::current().set_parent(parent_cx);
    request
}

impl OpentelemetryTracer {
    pub fn new(service_name: Option<&'static str>, endpoint: Option<&str>) -> OpentelemetryTracer {
        opentelemetry::global::set_text_map_propagator(TraceContextPropagator::new());

        let m_endpoint = endpoint.unwrap_or("http://0.0.0.0:4317");
        let tracer_exporter = opentelemetry_otlp::new_exporter()
            .tonic()
            .with_endpoint(m_endpoint);

        let service_name_str = service_name.unwrap_or("default service").clone();
        let tracer_config =
            opentelemetry::sdk::trace::config().with_resource(Resource::new(vec![KeyValue::new(
                "service.name",
                service_name_str,
            )]));

        let tracer = opentelemetry_otlp::new_pipeline()
            .tracing()
            .with_exporter(tracer_exporter)
            .with_trace_config(tracer_config)
            .install_batch(opentelemetry::runtime::AsyncStd)
            .expect("failed to install");

        let tracer_layer = tracing_opentelemetry::layer().with_tracer(tracer);

        let filter = EnvFilter::builder()
            .with_default_directive(LevelFilter::INFO.into())
            .from_env_lossy();
        let log_file = File::create(format!("logs/{}.log", service_name_str)).unwrap();
        tracing_subscriber::fmt()
            .json()
            .with_writer(log_file)
            .with_env_filter(filter)
            .with_span_events(fmt::format::FmtSpan::NONE)
            .finish()
            .with(tracer_layer)
            .init();

        let root = span!(Level::INFO, "Service span");

        println!(
            "trace-id == {}",
            root.context().span().span_context().trace_id()
        );

        OpentelemetryTracer { m_span: root }
    }
    pub fn default() -> OpentelemetryTracer {
        OpentelemetryTracer::new(None, None)
    }
}

impl Drop for OpentelemetryTracer {
    fn drop(&mut self) {
        shutdown_tracer_provider();
    }
}
