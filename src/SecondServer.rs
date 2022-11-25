pub mod pb {
    tonic::include_proto!("grpc.examples.echo");
}

use std::net::SocketAddr;
use tonic::{transport::Server, Request, Response, Status};

use pb::EchoRequest;
use pb::SecondEchoResponse;

type EchoResult<T> = Result<Response<T>, Status>;
use tracing::info;
use tracing_opentelemetry::OpenTelemetrySpanExt;

#[derive(Debug)]
pub struct SecondEchoServer {
    addr: SocketAddr,
}

#[tonic::async_trait]
impl pb::second_echo_server::SecondEcho for SecondEchoServer {
    #[tracing::instrument]
    async fn second_unary_echo(
        &self,
        mut request: Request<EchoRequest>,
    ) -> EchoResult<SecondEchoResponse> {
        request = load_balance::extract(request);

        let message = format!("{} (from {})", request.into_inner().message, self.addr);
        println!("HEAD EchoService message == {}", message);
        //println!("Current trace-id == {}", root.context().span().span_context().trace_id());
        info!("HEAD EchoService message == {}", message);

        Ok(Response::new(SecondEchoResponse { message }))
    }
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let optl_tracer = load_balance::OpentelemetryTracer::new(Some("Head_Server_service"), None); //Some("http://0.0.0.0:8080"));
    let _enter = optl_tracer.m_span.enter();
    {
        let addr = "[::1]:50053".parse()?;
        let server = SecondEchoServer { addr };

        Server::builder()
            .add_service(pb::second_echo_server::SecondEchoServer::new(server))
            .serve(addr)
            .await?;
    }

    Ok(())
}
