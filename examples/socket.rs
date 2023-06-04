use async_hal::io::{AsyncRead, AsyncWrite};
use futures::channel::mpsc;
use iso_tp::Socket;

#[tokio::main]
async fn main() {
    let (a_tx, a_rx) = mpsc::unbounded();
    let (b_tx, b_rx) = mpsc::unbounded();

    let mut a = Socket::new(a_tx, b_rx);
    let mut b = Socket::new(b_tx, a_rx);

    let mut writer = a.writer();
    writer.write_all(b"hello").await.unwrap();

    let mut reader = b.reader(0, 0).await;
    let mut buf = [0; 5];
    reader.read(&mut buf).await.unwrap();

    assert_eq!(&buf, b"hello");
}
