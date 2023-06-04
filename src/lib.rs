#![cfg_attr(not(test), no_std)]

pub mod frame;
pub use frame::Frame;

pub mod socket;
pub use socket::Socket;

#[cfg(test)]
mod tests {
    use super::*;
    use async_hal::io::AsyncRead;
    use futures::stream;

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
        let (cons, _) = Frame::consecutive(0, &bytes[used..]);

        let rx = stream::iter(vec![first, cons]);

        let mut socket = Socket::new(tx, rx);
        let transaction = socket.read().await;
        let mut reader = transaction.reader(10, 0);

        let mut buf = [0; 12];
        let used = reader.read(&mut buf).await.unwrap();
        reader.read(&mut buf[used..]).await.unwrap();

        assert_eq!(&buf, bytes);
    }
}
