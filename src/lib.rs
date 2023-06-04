#![cfg_attr(not(test), no_std)]

pub mod frame;
pub use frame::Frame;

pub mod socket;
pub use socket::Socket;

#[cfg(test)]
mod tests {
    use super::*;
    use async_hal::io::{AsyncRead, AsyncWrite};
    use futures::{future::poll_fn, pin_mut, stream};

    #[tokio::test]
    async fn it_reads_single_frames() {
        let tx: Vec<Frame> = vec![];
        let rx = stream::iter(vec![Frame::single(b"hello").unwrap()]);

        let mut socket = Socket::new(tx, rx);
        let transaction = socket.read().await;
        assert_eq!(transaction.single(), Some(Frame::single(b"hello").unwrap()));
    }

    #[tokio::test]
    async fn it_reads_consecutive_frames() {
        let tx: Vec<Frame> = vec![];

        let bytes = b"Hello World!";
        let (first, used) = Frame::first(bytes);
        let (second, _) = Frame::consecutive(0, &bytes[used..]);

        let rx = stream::iter(vec![first, second]);

        let mut socket = Socket::new(tx, rx);
        let mut reader = socket.reader(10, 0).await;

        let mut buf = [0; 12];
        let used = reader.read(&mut buf).await.unwrap();
        reader.read(&mut buf[used..]).await.unwrap();

        assert_eq!(&buf, bytes);
    }

    #[tokio::test]
    async fn it_writess_single_frames() {
        let tx: Vec<Frame> = vec![];
        let rx = stream::iter(Vec::new());

        let mut socket = Socket::new(tx, rx);
        let writer = socket.writer();
        pin_mut!(writer);

        let buf = b"hello";
        poll_fn(|cx| writer.as_mut().poll_write(cx, buf))
            .await
            .unwrap();
        poll_fn(|cx| writer.as_mut().poll_write(cx, buf))
            .await
            .unwrap();

        assert_eq!(socket.tx[0], Frame::single(b"hello").unwrap());
    }

    #[tokio::test]
    async fn it_writess_consecutive_frames() {
        let tx: Vec<Frame> = vec![];
        let rx = stream::iter(vec![Frame::flow(frame::FlowKind::Continue, 10, 0)]);

        let mut socket = Socket::new(tx, rx);
        let writer = socket.writer();
        pin_mut!(writer);

        let buf = b"Hello World!";
        let used = poll_fn(|cx| writer.as_mut().poll_write(cx, buf))
            .await
            .unwrap();
        poll_fn(|cx| writer.as_mut().poll_write(cx, &buf[used..]))
            .await
            .unwrap();

        let (first, used) = Frame::first(buf);
        assert_eq!(socket.tx[0], first);

        let (second, _) = Frame::consecutive(0, &buf[used..]);
        assert_eq!(socket.tx[1], second);
    }
}
