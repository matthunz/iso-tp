use crate::Frame;
use async_hal::delay::DelayMs;
use futures::{Sink, Stream};

pub mod reader;
pub use reader::Reader;

pub mod writer;
pub use writer::Writer;

pub trait Transport<E>: Sink<Frame> + Stream<Item = Result<Frame, E>> + Sized {
    fn reader(self) -> Reader<Self, E> {
        Reader::new(self)
    }

    fn writer<D>(self, delay: D) -> Writer<Self, E, D>
    where
        D: DelayMs + Unpin,
        D::Delay: From<u8>,
    {
        Writer::new(self, delay)
    }
}

impl<T, E> Transport<E> for T where T: Sink<Frame> + Stream<Item = Result<Frame, E>> {}
