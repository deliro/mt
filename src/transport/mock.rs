use std::pin::Pin;
use std::sync::{Arc, Mutex};
use std::task::{Context, Poll};

use futures::{Sink, Stream};
use tokio::sync::mpsc;

use crate::transport::TransportError;

pub struct Script {
    frames: Vec<Vec<u8>>,
}

impl Script {
    pub fn from_frames(frames: Vec<Vec<u8>>) -> Self {
        Self { frames }
    }
}

pub struct MockTransport {
    incoming: mpsc::UnboundedReceiver<Result<Vec<u8>, TransportError>>,
    captured: Arc<Mutex<Vec<Vec<u8>>>>,
}

#[derive(Clone)]
pub struct MockHandle {
    captured: Arc<Mutex<Vec<Vec<u8>>>>,
    inject: mpsc::UnboundedSender<Result<Vec<u8>, TransportError>>,
}

impl MockHandle {
    pub fn captured(&self) -> Vec<Vec<u8>> {
        self.captured.lock().expect("mock capture lock").clone()
    }

    pub fn inject(&self, frame: Vec<u8>) {
        let _ = self.inject.send(Ok(frame));
    }

    pub fn close(self) {
        drop(self.inject);
    }
}

impl MockTransport {
    pub fn new(script: Script) -> (Self, MockHandle) {
        let (tx, rx) = mpsc::unbounded_channel();
        for frame in script.frames {
            let _ = tx.send(Ok(frame));
        }
        let captured = Arc::new(Mutex::new(Vec::new()));
        let handle = MockHandle { captured: captured.clone(), inject: tx };
        (Self { incoming: rx, captured }, handle)
    }
}

impl Stream for MockTransport {
    type Item = Result<Vec<u8>, TransportError>;

    fn poll_next(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Option<Self::Item>> {
        self.incoming.poll_recv(cx)
    }
}

impl Sink<Vec<u8>> for MockTransport {
    type Error = TransportError;

    fn poll_ready(self: Pin<&mut Self>, _: &mut Context<'_>) -> Poll<Result<(), Self::Error>> {
        Poll::Ready(Ok(()))
    }

    fn start_send(self: Pin<&mut Self>, item: Vec<u8>) -> Result<(), Self::Error> {
        self.captured.lock().expect("mock capture lock").push(item);
        Ok(())
    }

    fn poll_flush(self: Pin<&mut Self>, _: &mut Context<'_>) -> Poll<Result<(), Self::Error>> {
        Poll::Ready(Ok(()))
    }

    fn poll_close(self: Pin<&mut Self>, _: &mut Context<'_>) -> Poll<Result<(), Self::Error>> {
        Poll::Ready(Ok(()))
    }
}
