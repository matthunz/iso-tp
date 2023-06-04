#[cfg(feature = "socket")]
mod tests {
    use async_hal::io::AsyncRead;
    use futures::stream;
    use iso_tp::{Frame, Socket};

    #[tokio::test]
    async fn it_reads_single_frames() {
        let tx: Vec<Frame> = vec![];

        let bytes = b"example";
        let frame = Frame::single(bytes).unwrap();
        let rx = stream::iter(vec![Ok(frame)]);

        let mut socket = Socket::new(tx, rx);
        let mut reader = socket.reader();

        let mut buf = [0; 7];
        let used = reader.read(&mut buf).await.unwrap();
        reader.read(&mut buf[used..]).await.unwrap();

        assert_eq!(&buf, bytes);
    }

    #[tokio::test]
    async fn it_reads_consecutive_frames() {
        let tx: Vec<Frame> = vec![];

        let bytes = b"Hello World!";
        let (first, used) = Frame::first(bytes);
        let (second, _) = Frame::consecutive(0, &bytes[used..]);

        let rx = stream::iter(vec![Ok(first), Ok(second)]);

        let mut socket = Socket::new(tx, rx);
        let mut reader = socket.reader();

        let mut buf = [0; 12];
        let used = reader.read(&mut buf).await.unwrap();
        reader.read(&mut buf[used..]).await.unwrap();

        assert_eq!(&buf, bytes);
    }
}
