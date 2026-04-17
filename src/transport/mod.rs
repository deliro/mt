pub mod error;
pub mod mock;
pub mod tcp;

use std::pin::Pin;

use futures::{Sink, Stream};

pub use error::TransportError;

pub type Frame = Vec<u8>;

pub trait Transport:
    Sink<Frame, Error = TransportError>
    + Stream<Item = Result<Frame, TransportError>>
    + Send
    + 'static
{
}

impl<T> Transport for T where
    T: Sink<Frame, Error = TransportError>
        + Stream<Item = Result<Frame, TransportError>>
        + Send
        + 'static
{
}

pub type BoxedTransport = Pin<Box<dyn Transport>>;
