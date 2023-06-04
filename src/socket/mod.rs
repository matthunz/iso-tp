use crate::{frame::Kind, Frame};
use futures::{Stream, StreamExt};

mod read;
pub use read::{Consecutive, ConsecutiveReader, Read};

mod reader;
pub use reader::Reader;

pub mod writer;
pub use writer::Writer;

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Error<T, R, D> {
    Transmit(T),
    Receive(R),
    Delay(D),
    InvalidFrame,
    Aborted,
    UnexpectedEOF,
}

pub struct Socket<T, R> {
    pub tx: T,
    pub rx: R,
}

impl<T, R> Socket<T, R> {
    pub fn new(tx: T, rx: R) -> Self {
        Self { tx, rx }
    }

    pub async fn read(&mut self) -> Read<T, R>
    where
        R: Stream<Item = Frame> + Unpin,
    {
        let frame = self.rx.next().await.unwrap();
        match frame.kind().unwrap() {
            Kind::Single => Read::Single { frame },
            Kind::First => Read::Consecutive(Consecutive::new(frame, self)),
            _ => todo!(),
        }
    }

    /// Create a reader for a new ISO-TP message.
    pub async fn reader(&mut self, block_len: u8, st: u8) -> Reader<T, R>
    where
        R: Stream<Item = Frame> + Unpin,
    {
        self.read().await.reader(block_len, st)
    }

    /// Create a writer for a new ISO-TP message.
    pub fn writer<E, D>(&mut self, delay: D) -> Writer<T, R, E, D> {
        Writer::new(self, delay)
    }
}
