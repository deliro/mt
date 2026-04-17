#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use futures::StreamExt;
use mt::codec::frame::encode;
use mt::transport::tcp::connect;
use tokio::io::AsyncWriteExt;
use tokio::net::TcpListener;

#[tokio::test(flavor = "current_thread")]
async fn tcp_transport_receives_framed_payload() {
    let listener = TcpListener::bind("127.0.0.1:0").await.expect("bind");
    let addr = listener.local_addr().expect("addr");

    tokio::spawn(async move {
        let (mut sock, _) = listener.accept().await.expect("accept");
        let frame = encode(b"hi").expect("encode");
        sock.write_all(&frame).await.expect("write");
    });

    let transport = connect(&addr.ip().to_string(), addr.port()).await.expect("connect");
    let (_sink, mut stream) = transport.split();
    let first = stream.next().await.expect("frame").expect("ok");
    assert_eq!(first.as_slice(), b"hi");
}
