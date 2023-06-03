#![cfg_attr(not(test), no_std)]

pub mod frame;
use async_hal::io::AsyncRead;
pub use frame::Frame;
use frame::{FlowKind, Kind};
use futures::{Sink, SinkExt, Stream, StreamExt};

pub struct Consecutive<'a, T, R> {
    first_frame: Frame,
    transport: &'a mut Transport<T, R>,
}

impl<T, R> Consecutive<'_, T, R> {
    pub async fn accept(self, block_len: u8, st: u8)
    where
        T: Sink<Frame> + Unpin,
    {
        let frame = Frame::flow(FlowKind::Continue, block_len, st);
        self.transport.tx.send(frame).await.ok().unwrap();
    }

    pub async fn wait(self, st: u8)
    where
        T: Sink<Frame> + Unpin,
    {
        let frame = Frame::flow(FlowKind::Wait, 0, st);
        self.transport.tx.send(frame).await.ok().unwrap();
    }

    pub async fn abort(self)
    where
        T: Sink<Frame> + Unpin,
    {
        let frame = Frame::flow(FlowKind::Abort, 0, 0);
        self.transport.tx.send(frame).await.ok().unwrap();
    }
}

pub enum Transaction<'a, T, R> {
    Single { frame: Frame },
    Consecutive(Consecutive<'a, T, R>),
}

impl< T, R> Transaction<'_, T, R> {
    pub fn single(self) -> Option<Frame> {
        if let Self::Single { frame } = self {
            Some(frame)
        } else {
            None
        }
    }
}

pub struct Transport<T, R> {
    tx: T,
    rx: R,
}

impl<T, R> Transport<T, R> {
    pub fn new(tx: T, rx: R) -> Self {
        Self { tx, rx }
    }

    pub async fn transaction(&mut self) -> Transaction<T, R>
    where
        R: Stream<Item = Frame> + Unpin,
    {
        let frame = self.rx.next().await.unwrap();
        match frame.kind().unwrap() {
            Kind::Single => Transaction::Single { frame },
            Kind::First => Transaction::Consecutive(Consecutive {
                first_frame: frame,
                transport: self,
            }),
            _ => todo!(),
        }
    }
}


#[cfg(test)]
mod tests {
    use futures::stream;
    use super::*;

    #[tokio::test]
    async fn it_works() {
        let tx: Vec<Frame> = vec![];
        let rx = stream::iter( vec![Frame::single(b"hello").unwrap()]);

        let mut tp = Transport::new(tx, rx);
        let transaction = tp.transaction().await;
        assert_eq!(transaction.single(), Some(Frame::single(b"hello").unwrap()));

    }
}
