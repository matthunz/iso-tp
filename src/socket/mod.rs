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

    /// Create a reader for a new ISO-TP message.
    pub fn reader(&mut self) -> Reader<T, R> {
        Reader::new(self)
    }

    /// Create a writer for a new ISO-TP message.
    pub fn writer<E, D>(&mut self, delay: D) -> Writer<T, R, E, D> {
        Writer::new(self, delay)
    }
}
