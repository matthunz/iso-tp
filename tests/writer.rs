#[cfg(feature = "socket")]
mod tests {
    use async_hal::io::AsyncWrite;
    use futures::{pin_mut, stream};
    use iso_tp::{frame::FlowKind, Frame, Socket};

    #[tokio::test]
    async fn it_writes_single_frames() {
        let tx: Vec<Frame> = vec![];
        let rx = stream::iter(Vec::new());

        let mut socket = Socket::new(tx, rx);
        let writer = socket.writer();
        pin_mut!(writer);

        let buf = b"hello";
        writer.write_all(buf).await.unwrap();

        assert_eq!(socket.tx[0], Frame::single(b"hello").unwrap());
    }

    #[tokio::test]
    async fn it_writes_consecutive_frames() {
        let tx: Vec<Frame> = vec![];
        let rx = stream::iter(vec![Frame::flow(FlowKind::Continue, 10, 0)]);

        let mut socket = Socket::new(tx, rx);
        let writer = socket.writer();
        pin_mut!(writer);

        let buf = b"Hello World!";
        writer.write_all(buf).await.unwrap();

        let (first, used) = Frame::first(buf);
        assert_eq!(socket.tx[0], first);

        let (second, _) = Frame::consecutive(0, &buf[used..]);
        assert_eq!(socket.tx[1], second);
    }
}
