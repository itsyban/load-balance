pub mod pb {
    tonic::include_proto!("grpc.examples.echo");
}

use std::net::SocketAddr;
use tonic::{transport::Server, Request, Response, Status};

use pb::{EchoRequest };
use pb::{SecondEchoResponse};

type EchoResult<T> = Result<Response<T>, Status>;


#[derive(Debug)]
pub struct SecondEchoServer {
    addr: SocketAddr,
}

#[tonic::async_trait]
impl pb::second_echo_server::SecondEcho for SecondEchoServer {
    async fn second_unary_echo(&self, request: Request<EchoRequest>) -> EchoResult<SecondEchoResponse> {
        let message = format!("{} (from {})", request.into_inner().message, self.addr);
        println!("Second EchoService message message == {}", message);

        Ok(Response::new(SecondEchoResponse { message }))
    }
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let addr = "[::1]:50053".parse()?;
    let server = SecondEchoServer{ addr };

    Server::builder()
        .add_service(pb::second_echo_server::SecondEchoServer::new(server))
        .serve(addr)
        .await?;

    Ok(())
}
