pub mod pb {
    tonic::include_proto!("grpc.examples.echo");
}

use opentelemetry::trace::TraceContextExt;
use pb::{echo_client::EchoClient, EchoRequest};
use tonic::transport::Channel;
use tracing_opentelemetry::OpenTelemetrySpanExt;
use tracing::{ trace };

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    
    let endpoints = ["http://[::1]:50051", "http://[::1]:50052"]
        .iter()
        .map(|a| Channel::from_static(a));

    let optl_tracer = load_balance::OpentelemetryTracer::new(Some("Client service"), None);
    {
        let _enter = optl_tracer.m_span.enter();

        let channel = Channel::balance_list(endpoints);

        let mut client = EchoClient::new(channel);

        for _ in 0..12usize {
            let request = tonic::Request::new(EchoRequest {
                message: "hello".into(),
                trace_id: format!("{}", optl_tracer.m_span.context().span().span_context().trace_id()),
            });

            let response = client.unary_echo(request).await?;

            trace!("RESPONSE={:?}", response);
        }
    }

    Ok(())
}
