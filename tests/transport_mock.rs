#![allow(clippy::expect_used, clippy::unwrap_used, clippy::panic, clippy::pedantic, clippy::nursery, clippy::cargo, clippy::indexing_slicing, clippy::integer_division, clippy::collapsible_if, clippy::byte_char_slices, clippy::redundant_pattern_matching)]

use futures::{SinkExt, StreamExt};
use mt::transport::mock::{MockTransport, Script};

#[tokio::test(flavor = "current_thread")]
async fn mock_delivers_scripted_frames_and_captures_writes() {
    let (transport, handle) =
        MockTransport::new(Script::from_frames(vec![vec![1, 2, 3], vec![9, 9, 9]]));
    let (mut sink, mut stream) = transport.split();

    assert_eq!(stream.next().await.expect("first").expect("ok"), vec![1, 2, 3]);
    assert_eq!(stream.next().await.expect("second").expect("ok"), vec![9, 9, 9]);

    sink.send(vec![0xAA]).await.expect("send");
    sink.send(vec![0xBB]).await.expect("send");

    assert_eq!(handle.captured(), vec![vec![0xAA], vec![0xBB]]);
}

#[tokio::test(flavor = "current_thread")]
async fn injected_frames_arrive_after_initial_script() {
    let (transport, handle) = MockTransport::new(Script::from_frames(vec![vec![1]]));
    let (_sink, mut stream) = transport.split();

    assert_eq!(stream.next().await.expect("scripted").expect("ok"), vec![1]);
    handle.inject(vec![42]);
    assert_eq!(stream.next().await.expect("injected").expect("ok"), vec![42]);

    handle.close();
    assert!(stream.next().await.is_none());
}
