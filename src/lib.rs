#![no_std]

pub mod frame;
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

impl<T, R> Consecutive<'_, T, R> {}

pub enum Transaction<'a, T, R> {
    Single { frame: Frame },
    Consecutive(Consecutive<'a, T, R>),
}

pub struct Transport<T, R> {
    tx: T,
    rx: R,
}

impl<T, R> Transport<T, R> {
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

pub fn add(left: usize, right: usize) -> usize {
    left + right
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn it_works() {
        let result = add(2, 2);
        assert_eq!(result, 4);
    }
}
