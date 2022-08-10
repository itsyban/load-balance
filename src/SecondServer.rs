pub mod pb {
    tonic::include_proto!("grpc.examples.echo");
}

use std::net::SocketAddr;
use tonic::{transport::Server, Request, Response, Status};

use pb::{EchoRequest };
use pb::{SecondEchoResponse};

type EchoResult<T> = Result<Response<T>, Status>;
use tracing::{ span, trace, Level };
use opentelemetry::trace::TraceContextExt;
use tracing_opentelemetry::OpenTelemetrySpanExt;

#[derive(Debug)]
pub struct SecondEchoServer {
    addr: SocketAddr,
}

#[tonic::async_trait]
impl pb::second_echo_server::SecondEcho for SecondEchoServer {
    async fn second_unary_echo(&self, request: Request<EchoRequest>) -> EchoResult<SecondEchoResponse> {
        let root = span!(Level::INFO, "SecondEchoServer ");
        let _enter = root.enter();
        let parsed_request = request.into_inner();
        let trace_id = parsed_request.trace_id;
        let message = format!("trace-id [{}]: {} (from {}) pre trace-id: {}", 
            root.context().span().span_context().trace_id(), parsed_request.message, self.addr, trace_id);
        trace!("Second EchoService message message == {}", message);

        Ok(Response::new(SecondEchoResponse { message, trace_id }))
    }
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let optl_tracer = load_balance::OpentelemetryTracer::new(Some("Head Server service"), None);
    {
        let enter = optl_tracer.m_span.enter();
        let addr = "[::1]:50053".parse()?;
        let server = SecondEchoServer{ addr };

        Server::builder()
            .add_service(pb::second_echo_server::SecondEchoServer::new(server))
            .serve(addr)
            .await?;
    }

    Ok(())
}
