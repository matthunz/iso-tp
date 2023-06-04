use crate::{frame::Kind, Frame};

use futures::{Stream, StreamExt};

mod read;
pub use read::{Consecutive, ConsecutiveReader, Read};

mod reader;
pub use reader::Reader;

pub mod writer;
pub use writer::Writer;

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

    pub async fn reader(&mut self, block_len: u8, st: u8) -> Reader<T, R>
    where
        R: Stream<Item = Frame> + Unpin,
    {
        self.read().await.reader(block_len, st)
    }

    pub fn writer(&mut self) -> Writer<T, R> {
        Writer::new(self)
    }
}
