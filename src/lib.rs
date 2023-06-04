#![cfg_attr(not(test), no_std)]

use async_hal::io::AsyncRead;
use core::{
    pin::Pin,
    task::{Context, Poll},
};
use futures::{ready, Sink, SinkExt, Stream, StreamExt};

pub mod frame;
pub use frame::Frame;
use frame::{FlowKind, Kind};

pub struct ConsecutiveReader<'a, T, R> {
    tranaction: Consecutive<'a, T, R>,
    block_len: u8,
    st: u8,
}

impl<T, R> AsyncRead for ConsecutiveReader<'_, T, R>
where
    T: Sink<Frame> + Unpin,
    R: Stream<Item = Frame> + Unpin,
{
    type Error = ();

    fn poll_read(
        mut self: Pin<&mut Self>,
        cx: &mut Context,
        buf: &mut [u8],
    ) -> Poll<Result<usize, Self::Error>> {
        let me = &mut *self;
        loop {
            let used = ready!(Pin::new(&mut me.tranaction).poll_read(cx, buf))
                .ok()
                .unwrap();

            if used == 0 {
                if !ready!(Pin::new(&mut me.tranaction).poll_resume(cx, me.block_len, me.st)) {
                    return Poll::Ready(Ok(0));
                }
            } else {
                break Poll::Ready(Ok(used));
            }
        }
    }
}

pub struct Consecutive<'a, T, R> {
    remaining_frames: u8,
    remaining_bytes: u16,
    first_frame: Frame,
    transport: &'a mut Transport<T, R>,
    is_first_frame_read: bool,
    is_flushing: bool,
}

impl<'a, T, R> Consecutive<'a, T, R> {
    pub fn poll_resume(
        mut self: Pin<&mut Self>,
        cx: &mut Context,
        block_len: u8,
        st: u8,
    ) -> Poll<bool>
    where
        T: Sink<Frame> + Unpin,
    {
        if self.remaining_bytes == 0 {
            return Poll::Ready(false);
        }

        if !self.is_flushing {
            ready!(self.transport.tx.poll_ready_unpin(cx)).ok().unwrap();

            let frame = Frame::flow(FlowKind::Continue, block_len, st);
            self.transport.tx.start_send_unpin(frame).ok().unwrap();
            self.is_flushing = true;
        } else {
            ready!(self.transport.tx.poll_flush_unpin(cx)).ok().unwrap();
        }

        Poll::Ready(true)
    }

    pub async fn resume(&mut self, block_len: u8, st: u8) -> bool
    where
        T: Sink<Frame> + Unpin,
    {
        if self.remaining_bytes == 0 {
            return false;
        }

        let frame = Frame::flow(FlowKind::Continue, block_len, st);
        self.transport.tx.send(frame).await.ok().unwrap();

        true
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

    pub fn reader(self, block_len: u8, st: u8) -> ConsecutiveReader<'a, T, R> {
        ConsecutiveReader {
            tranaction: self,
            block_len,
            st,
        }
    }
}

impl<T, R> AsyncRead for Consecutive<'_, T, R>
where
    R: Stream<Item = Frame> + Unpin,
{
    type Error = ();

    fn poll_read(
        mut self: Pin<&mut Self>,
        cx: &mut Context,
        buf: &mut [u8],
    ) -> Poll<Result<usize, Self::Error>> {
        let used = if !self.is_first_frame_read {
            self.is_first_frame_read = true;

            let data = self.first_frame.first_data();
            buf[..data.len()].copy_from_slice(data);
            data.len()
        } else {
            if self.remaining_frames == 0 || self.remaining_bytes == 0 {
                return Poll::Ready(Ok(0));
            }

            let frame = ready!(self.transport.rx.poll_next_unpin(cx)).unwrap();
            if frame.kind() != Some(Kind::Consecutive) {
                todo!()
            }

            let data = frame.consecutive_data();
            let used = core::cmp::min(data.len(), self.remaining_bytes as _);

            buf[..used].copy_from_slice(&data[..used]);
            used
        };
        self.remaining_bytes -= used as u16;

        Poll::Ready(Ok(used))
    }
}

pub struct Reader<'a, T, R> {
    read: Read<'a, T, R>,
    block_len: u8,
    st: u8,
}

impl<T, R> AsyncRead for Reader<'_, T, R>
where
    T: Sink<Frame> + Unpin,
    R: Stream<Item = Frame> + Unpin,
{
    type Error = ();

    fn poll_read(
        mut self: Pin<&mut Self>,
        cx: &mut Context,
        buf: &mut [u8],
    ) -> Poll<Result<usize, Self::Error>> {
        let me = &mut *self;
        match me.read {
            Read::Single { ref frame } => {
                let data = frame.single_data();
                buf[..data.len()].copy_from_slice(data);

                Poll::Ready(Ok(data.len()))
            }
            Read::Consecutive(ref mut consecutive) => loop {
                let mut pinned = Pin::new(&mut *consecutive);
                let used = ready!(pinned.as_mut().poll_read(cx, buf)).ok().unwrap();

                if used == 0 {
                    if !ready!(pinned.poll_resume(cx, me.block_len, me.st)) {
                        return Poll::Ready(Ok(0));
                    }
                } else {
                    break Poll::Ready(Ok(used));
                }
            },
        }
    }
}

pub enum Read<'a, T, R> {
    Single { frame: Frame },
    Consecutive(Consecutive<'a, T, R>),
}

impl<'a, T, R> Read<'a, T, R> {
    pub fn single(self) -> Option<Frame> {
        if let Self::Single { frame } = self {
            Some(frame)
        } else {
            None
        }
    }

    pub fn consecutive(self) -> Option<Consecutive<'a, T, R>> {
        if let Self::Consecutive(consecutive) = self {
            Some(consecutive)
        } else {
            None
        }
    }

    pub fn reader(self, block_len: u8, st: u8) -> Reader<'a, T, R> {
        Reader {
            read: self,
            block_len,
            st,
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

    pub async fn read(&mut self) -> Read<T, R>
    where
        R: Stream<Item = Frame> + Unpin,
    {
        let frame = self.rx.next().await.unwrap();
        match frame.kind().unwrap() {
            Kind::Single => Read::Single { frame },
            Kind::First => Read::Consecutive(Consecutive {
                transport: self,
                remaining_frames: 0,
                is_flushing: false,
                is_first_frame_read: false,
                remaining_bytes: frame.first_len(),
                first_frame: frame,
            }),
            _ => todo!(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use futures::stream;

    #[tokio::test]
    async fn it_reads_single_frames() {
        let tx: Vec<Frame> = vec![];
        let rx = stream::iter(vec![Frame::single(b"hello").unwrap()]);

        let mut tp = Transport::new(tx, rx);
        let transaction = tp.read().await;
        assert_eq!(transaction.single(), Some(Frame::single(b"hello").unwrap()));
    }

    #[tokio::test]
    async fn it_reads_consecutive_frames() {
        let tx: Vec<Frame> = vec![];

        let bytes = b"Hello World!";
        let (first, used) = Frame::first(bytes);
        let (cons, _) = Frame::consecutive(0, &bytes[used..]);

        let rx = stream::iter(vec![first, cons]);

        let mut tp = Transport::new(tx, rx);
        let transaction = tp.read().await;
        let mut reader = transaction.reader(10, 0);

        let mut buf = [0; 12];
        let used = reader.read(&mut buf).await.unwrap();
        reader.read(&mut buf[used..]).await.unwrap();

        assert_eq!(&buf, bytes);
    }
}
