pub mod reader;
use core::marker::PhantomData;

pub use reader::Reader;

pub mod writer;
pub use writer::Writer;

pub struct Socket<T, R, E> {
    pub tx: T,
    pub rx: R,
    _marker: PhantomData<E>,
}

impl<T: Unpin, R: Unpin, E> Unpin for Socket<T, R, E> {}

impl<T, R, E> Socket<T, R, E> {
    pub fn new(tx: T, rx: R) -> Self {
        Self {
            tx,
            rx,
            _marker: PhantomData,
        }
    }

    /// Create a reader for a new ISO-TP message.
    pub fn reader(&mut self) -> Reader<T, R, E> {
        Reader::new(self)
    }

    /// Create a writer for a new ISO-TP message.
    pub fn writer<D>(&mut self, delay: D) -> Writer<T, R, E, D> {
        Writer::new(self, delay)
    }
}
