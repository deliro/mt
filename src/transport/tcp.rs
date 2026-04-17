use futures::{SinkExt, TryStreamExt};
use tokio::net::TcpStream;
use tokio_util::codec::Framed;

use crate::codec::frame::FrameCodec;
use crate::error::ConnectError;
use crate::transport::{BoxedTransport, TransportError};

pub async fn connect(host: &str, port: u16) -> Result<BoxedTransport, ConnectError> {
    let stream = TcpStream::connect((host, port)).await.map_err(ConnectError::Tcp)?;
    stream.set_nodelay(true).map_err(ConnectError::Tcp)?;
    let framed = Framed::new(stream, FrameCodec);
    Ok(Box::pin(adapt(framed)))
}

fn adapt<T>(
    inner: T,
) -> impl futures::Sink<Vec<u8>, Error = TransportError>
+ futures::Stream<Item = Result<Vec<u8>, TransportError>>
+ Send
+ 'static
where
    T: futures::Sink<Vec<u8>, Error = crate::codec::error::FrameError>
        + futures::Stream<Item = Result<Vec<u8>, crate::codec::error::FrameError>>
        + Send
        + 'static,
{
    inner.sink_map_err(TransportError::from).map_err(TransportError::from)
}
