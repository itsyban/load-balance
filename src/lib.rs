use opentelemetry::trace::{TraceContextExt};

use tracing::{ span, Level };
use tracing_subscriber::prelude::*;
use tracing_opentelemetry::{self, OpenTelemetrySpanExt};

use opentelemetry::{KeyValue, sdk::Resource, };
use opentelemetry::{ global::shutdown_tracer_provider};

use opentelemetry_otlp::{self, WithExportConfig};

pub struct OpentelemetryTracer {
    pub m_span: tracing::Span,
}

impl OpentelemetryTracer {
    pub fn new(service_name:  Option<&'static str>, endpoint: Option<&str>) -> OpentelemetryTracer {
        let tracer_exporter = opentelemetry_otlp::new_exporter()
            .tonic()
            .with_endpoint("http://0.0.0.0:4317");//(endpoint.unwrap_or("http://0.0.0.0:4317"));
    
        let service_name_str = service_name.unwrap_or("default service").clone();
        let tracer_config = opentelemetry::sdk::trace::config()
            .with_resource(Resource::new(vec![KeyValue::new(
                    "service.name", service_name_str,
                    )]));
    
        let tracer = opentelemetry_otlp::new_pipeline()
            .tracing()
            .with_exporter(tracer_exporter)
            .with_trace_config(tracer_config) 
            .install_batch(opentelemetry::runtime::AsyncStd)
            .expect("failed to install");

        let tracer_layer = tracing_opentelemetry::layer().with_tracer(tracer);

        tracing_subscriber::registry()
            .with(tracer_layer)
            .try_init();

        let root = span!(Level::INFO, "Service span");
        
        println!("trace-id == {}", root.context().span().span_context().trace_id());
       
        OpentelemetryTracer {m_span: root}
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

