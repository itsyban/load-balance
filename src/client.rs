pub mod pb {
    tonic::include_proto!("grpc.examples.echo");
}

use load_balance::inject;
use pb::{echo_client::EchoClient, EchoRequest};

use tonic::{
    transport::{Channel, Endpoint},
    Request, Status,
};

use futures::stream::Stream;
use std::time::Duration;
use tokio_stream::StreamExt;

use tracing::{info, info_span, Instrument};

fn echo_requests_iter() -> impl Stream<Item = EchoRequest> {
    let stream = tokio_stream::iter(1..usize::MAX).map(|i| EchoRequest {
        message: format!("msg {:02}", i),
    });

    stream

    // tokio_stream::iter(1..usize::MAX).map(|i| {
    //     load_balance::inject(tonic::Request::new(EchoRequest {
    //         message: "hello".into(),
    //     }))
    // })
}

async fn streaming_echo(client: &mut EchoClient<Channel>, num: usize) {
    let mut request = tonic::Request::new(EchoRequest {
        message: "hello".into(),
    });

    let stream = client
        .server_streaming_echo(request)
        .await
        .unwrap()
        .into_inner();

    // stream is infinite - take just 5 elements and then disconnect
    let mut stream = stream.take(num);
    while let Some(item) = stream.next().await {
        info!("\treceived: {}", item.unwrap().message);
    }
    // stream is droped here and the disconnect info is send to server
}

async fn bidirectional_streaming_echo(client: &mut EchoClient<Channel>, num: usize) {
    let in_stream = echo_requests_iter().take(num);

    let response = client
        .bidirectional_streaming_echo(in_stream)
        .await
        .unwrap();

    let mut resp_stream = response.into_inner();

    while let Some(received) = resp_stream.next().await {
        let received = received.unwrap();
        info!("\treceived message: `{}`", received.message);
    }
}

async fn bidirectional_streaming_echo_throttle(client: &mut EchoClient<Channel>, dur: Duration) {
    let in_stream = echo_requests_iter().throttle(dur);

    let response = client
        .bidirectional_streaming_echo(in_stream)
        .await
        .unwrap();

    let mut resp_stream = response.into_inner();

    while let Some(received) = resp_stream.next().await {
        let received = received.unwrap();
        info!("\treceived message: `{}`", received.message);
    }
}

#[tracing::instrument]
async fn send_echo() -> Result<(), Box<dyn std::error::Error + Send + Sync + 'static>> {
    // let endpoints = ["http://[::1]:50051", "http://[::1]:50052"]
    //     .iter()
    //     .map(|a| Channel::from_static(a));
    // let channel = Channel::balance_list(endpoints);

    // let mut client = EchoClient::new(channel);

    // for _ in 0..12usize {
    //     let mut request = tonic::Request::new(EchoRequest {
    //         message: "hello".into(),
    //     });

    //     request = load_balance::inject(request);

    //     let response = client
    //         .unary_echo(request)
    //         .instrument(info_span!("unary echo client request"))
    //         .await?;

    //     info!("RESPONSE={:?}", response);
    //     println!("RESPONSE={:?}", response);
    // }

    let channel = Endpoint::from_static("http://[::1]:50051")
        .connect()
        .await?;

    let mut client = EchoClient::with_interceptor(channel, intercept);
    info!("Streaming echo:");

    let stream = client
        .server_streaming_echo(tonic::Request::new(EchoRequest {
            message: "hello".into(),
        }))
        .instrument(info_span!("server_streaming_echo"))
        .await
        .unwrap()
        .into_inner();

    // stream is infinite - take just 5 elements and then disconnect
    let mut stream = stream.take(5);
    while let Some(item) = stream.next().await {
        info!("\treceived: {}", item.unwrap().message);
    }

    //streaming_echo(&mut client, 5).await;
    tokio::time::sleep(Duration::from_secs(1)).await; //do not mess server println functions

    // Echo stream that sends 17 requests then graceful end that connection
    info!("\r\nBidirectional stream echo:");
    {
        let in_stream = echo_requests_iter().take(5);

        let response = client
            .bidirectional_streaming_echo(in_stream)
            .instrument(info_span!("bidirectional_streaming_echo"))
            .await
            .unwrap();

        let mut resp_stream = response.into_inner();

        while let Some(received) = resp_stream.next().await {
            let received = received.unwrap();
            info!("\treceived message: `{}`", received.message);
        }
    }
    // bidirectional_streaming_echo(&mut client, 17)
    //     .instrument(info_span!("Bidirectional stream echo span"))
    //     .await;

    // Echo stream that sends up to `usize::MAX` requets. One request each 2s.
    // Exiting client with CTRL+C demonstrate how to distinguish broken pipe from
    //graceful client disconnection (above example) on the server side.
    info!("\r\nBidirectional stream echo (kill client with CTLR+C):");
    let in_stream = echo_requests_iter().throttle(Duration::from_secs(2));

    let response = client
        .bidirectional_streaming_echo(in_stream)
        .instrument(info_span!("immortal bidirectional_streaming_echo"))
        .await
        .unwrap();

    let mut resp_stream = response.into_inner();

    while let Some(received) = resp_stream.next().await {
        let received = received.unwrap();
        info!("\treceived message: `{}`", received.message);
    }
    // bidirectional_streaming_echo_throttle(&mut client, Duration::from_secs(2))
    //     .instrument(info_span!("Bidirectional stream echo span unlimeted"))
    //     .await;

    Ok(())
}

fn intercept(req: Request<()>) -> Result<Request<()>, Status> {
    info!("CLIENT Intercepting request: {:?}", req);
    Ok(inject(req))
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error + Send + Sync + 'static>> {
    let optl_tracer = load_balance::OpentelemetryTracer::new(Some("Client_service"), None);
    let _enter = optl_tracer.m_span.enter();

    send_echo().instrument(info_span!("Send Echo Span")).await?;

    Ok(())
}
