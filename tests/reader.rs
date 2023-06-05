use std::{
    convert::Infallible,
    pin::Pin,
    task::{Context, Poll},
    vec,
};

use futures::{stream, Sink, SinkExt, Stream, StreamExt};
use iso_tp::Frame;

struct Mock {
    tx: Vec<Frame>,
    rx: stream::Iter<vec::IntoIter<Result<Frame, ()>>>,
}

impl Mock {
    pub fn new(tx: Vec<Frame>, rx: Vec<Result<Frame, ()>>) -> Self {
        Self {
            tx,
            rx: stream::iter(rx),
        }
    }
}

impl Stream for Mock {
    type Item = Result<Frame, ()>;

    fn poll_next(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Option<Self::Item>> {
        self.rx.poll_next_unpin(cx)
    }
}

impl Sink<Frame> for Mock {
    type Error = Infallible;

    fn poll_ready(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Result<(), Self::Error>> {
        self.tx.poll_ready_unpin(cx)
    }

    fn start_send(mut self: Pin<&mut Self>, item: Frame) -> Result<(), Self::Error> {
        self.tx.start_send_unpin(item)
    }

    fn poll_flush(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Result<(), Self::Error>> {
        self.tx.poll_flush_unpin(cx)
    }

    fn poll_close(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Result<(), Self::Error>> {
        self.tx.poll_close_unpin(cx)
    }
}

#[cfg(feature = "socket")]
mod tests {
    use crate::Mock;
    use async_hal::io::AsyncRead;
    use iso_tp::{Frame, Transport};

    #[tokio::test]
    async fn it_reads_single_frames() {
        let bytes = b"example";
        let frame = Frame::single(bytes).unwrap();

        let mock = Mock::new(Vec::new(), vec![Ok(frame)]);
        let mut reader = mock.reader();

        let mut buf = [0; 7];
        let used = reader.read(&mut buf).await.ok().unwrap();
        reader.read(&mut buf[used..]).await.ok().unwrap();

        assert_eq!(&buf, bytes);
    }

    #[tokio::test]
    async fn it_reads_consecutive_frames() {
        let bytes = b"Hello World!";
        let (first, used) = Frame::first(bytes);
        let (second, _) = Frame::consecutive(0, &bytes[used..]);
        let mock = Mock::new(Vec::new(), vec![Ok(first), Ok(second)]);

        let mut reader = mock.reader();
        let mut buf = [0; 12];
        let used = reader.read(&mut buf).await.ok().unwrap();
        reader.read(&mut buf[used..]).await.ok().unwrap();

        assert_eq!(&buf, bytes);
    }
}
