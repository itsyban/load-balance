pub mod pb {
    tonic::include_proto!("grpc.examples.echo");
}

use futures::Stream;
use load_balance::extract;
use std::{
    error::Error,
    io::ErrorKind,
    net::{SocketAddr, ToSocketAddrs},
    pin::Pin,
    time::Duration,
};
use tokio::sync::mpsc;
use tokio_stream::{wrappers::ReceiverStream, StreamExt};
use tonic::{transport::Server, Request, Response, Status, Streaming};

use pb::second_echo_client::SecondEchoClient;
use pb::{EchoRequest, EchoResponse};

type EchoResult<T> = Result<Response<T>, Status>;
type ResponseStream = Pin<Box<dyn Stream<Item = Result<EchoResponse, Status>> + Send>>;

//use opentelemetry::global;
use tracing::{info, info_span, Instrument};
//use tracing_opentelemetry::OpenTelemetrySpanExt;

#[derive(Debug)]
pub struct EchoServer {
    addr: SocketAddr,
}

fn match_for_io_error(err_status: &Status) -> Option<&std::io::Error> {
    let mut err: &(dyn Error + 'static) = err_status;

    loop {
        if let Some(io_err) = err.downcast_ref::<std::io::Error>() {
            return Some(io_err);
        }

        // h2::Error do not expose std::io::Error with `source()`
        // https://github.com/hyperium/h2/pull/462
        if let Some(h2_err) = err.downcast_ref::<h2::Error>() {
            if let Some(io_err) = h2_err.get_io() {
                return Some(io_err);
            }
        }

        err = match err.source() {
            Some(err) => err,
            None => return None,
        };
    }
}

#[tonic::async_trait]
impl pb::echo_server::Echo for EchoServer {
    #[tracing::instrument]
    async fn unary_echo(&self, mut request: Request<EchoRequest>) -> EchoResult<EchoResponse> {
        request = load_balance::extract(request);

        info!("Got a request: {:?}", request);

        request = load_balance::inject(request);

        let mut client = SecondEchoClient::connect("http://[::1]:50053")
            .instrument(info_span!("Middle server connect to Head", id = 2))
            .await
            .unwrap();
        let response = client
            .second_unary_echo(request)
            .instrument(tracing::Span::current())
            .await?;
        let parsed_response = response.into_inner();

        let message = format!("{} (from {})", parsed_response.message, self.addr);
        info!("EchoService message == {}", message);
        println!("EchoService message == {}", message);

        Ok(Response::new(EchoResponse { message }))
    }

    type ServerStreamingEchoStream = ResponseStream;
    #[tracing::instrument]
    async fn server_streaming_echo(
        &self,
        mut req: Request<EchoRequest>,
    ) -> EchoResult<Self::ServerStreamingEchoStream> {
        req = load_balance::extract(req);
        info!("EchoServer::server_streaming_echo");
        info!("\tclient connected from: {:?}", req.remote_addr());

        // creating infinite stream with requested message
        let repeat = std::iter::repeat(EchoResponse {
            message: req.into_inner().message,
        });
        let mut stream = Box::pin(tokio_stream::iter(repeat).throttle(Duration::from_millis(200)));

        // spawn and channel are required if you want handle "disconnect" functionality
        // the `out_stream` will not be polled after client disconnect
        let (tx, rx) = mpsc::channel(128);
        tokio::spawn(async move {
            while let Some(item) = stream.next().await {
                match tx.send(Result::<_, Status>::Ok(item)).await {
                    Ok(_) => {
                        // item (server response) was queued to be send to client
                    }
                    Err(_item) => {
                        // output_stream was build from rx and both are dropped
                        break;
                    }
                }
            }
            info!("\tclient disconnected");
        });

        let output_stream = ReceiverStream::new(rx);
        Ok(Response::new(
            Box::pin(output_stream) as Self::ServerStreamingEchoStream
        ))
    }
    #[tracing::instrument]
    async fn client_streaming_echo(
        &self,
        request: Request<Streaming<EchoRequest>>,
    ) -> EchoResult<EchoResponse> {
        load_balance::extract(request);
        info!("SERVER client_streaming_echo");

        Err(Status::unimplemented("not implemented"))
    }

    type BidirectionalStreamingEchoStream = ResponseStream;
    #[tracing::instrument]
    async fn bidirectional_streaming_echo(
        &self,
        req: Request<Streaming<EchoRequest>>,
    ) -> EchoResult<Self::BidirectionalStreamingEchoStream> {
        info!("EchoServer::bidirectional_streaming_echo");
        let req = load_balance::extract(req);
        let mut in_stream = req.into_inner();
        let (tx, rx) = mpsc::channel(128);

        // this spawn here is required if you want to handle connection error.
        // If we just map `in_stream` and write it back as `out_stream` the `out_stream`
        // will be drooped when connection error occurs and error will never be propagated
        // to mapped version of `in_stream`.
        tokio::spawn(async move {
            while let Some(result) = in_stream.next().await {
                match result {
                    Ok(v) => tx
                        .send(Ok(EchoResponse { message: v.message }))
                        .await
                        .expect("working rx"),
                    Err(err) => {
                        if let Some(io_err) = match_for_io_error(&err) {
                            if io_err.kind() == ErrorKind::BrokenPipe {
                                // here you can handle special case when client
                                // disconnected in unexpected way
                                eprintln!("\tclient disconnected: broken pipe");
                                break;
                            }
                        }

                        match tx.send(Err(err)).await {
                            Ok(_) => (),
                            Err(_err) => break, // response was droped
                        }
                    }
                }
            }
            println!("\tstream ended");
        });

        // echo just write the same data that was received
        let out_stream = ReceiverStream::new(rx);

        Ok(Response::new(
            Box::pin(out_stream) as Self::BidirectionalStreamingEchoStream
        ))
    }
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let optl_tracer = load_balance::OpentelemetryTracer::new(Some("Middle_Server_service"), None); //Some("http://0.0.0.0:8080"));
    let _enter = optl_tracer.m_span.enter();
    {
        // let addrs = ["[::1]:50051", "[::1]:50052"];

        // let (tx, mut rx) = mpsc::unbounded_channel();

        // for addr in &addrs {
        //     let addr = addr.parse()?;
        //     let tx = tx.clone();

        //     let server = EchoServer { addr };
        //     let serve = Server::builder()
        //         .add_service(pb::echo_server::EchoServer::new(server))
        //         .serve(addr);

        //     tokio::spawn(async move {
        //         if let Err(e) = serve.await {
        //             eprintln!("Error = {:?}", e);
        //         }

        //         tx.send(()).unwrap();
        //     });
        // }

        // rx.recv().await;
        let addr = "[::1]:50051".parse()?;
        let server = EchoServer { addr };
        Server::builder()
            .add_service(pb::echo_server::EchoServer::with_interceptor(
                server, intercept,
            ))
            .serve(addr)
            .await
            .unwrap();
    }

    Ok(())
}

fn intercept(mut req: Request<()>) -> Result<Request<()>, Status> {
    let req = extract(req);
    info!("SERVER Intercepting request: {:?}", req);

    // Set an extension that can be retrieved by `say_hello`

    Ok(req)
}
