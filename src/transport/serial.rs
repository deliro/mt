use std::path::Path;

use futures::{SinkExt, TryStreamExt};
use tokio_serial::{DataBits, FlowControl, Parity, SerialPortBuilderExt, SerialStream, StopBits};
use tokio_util::codec::Framed;

use crate::codec::frame::FrameCodec;
use crate::error::ConnectError;
use crate::transport::{BoxedTransport, TransportError};

pub fn connect(path: &Path) -> Result<BoxedTransport, ConnectError> {
    let path_str = path.to_string_lossy();
    let stream: SerialStream = tokio_serial::new(path_str.as_ref(), 115_200)
        .data_bits(DataBits::Eight)
        .parity(Parity::None)
        .stop_bits(StopBits::One)
        .flow_control(FlowControl::None)
        .open_native_async()?;
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
