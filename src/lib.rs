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
    tranaction: ConsecutiveTransaction<'a, T, R>,
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

pub struct ConsecutiveTransaction<'a, T, R> {
    remaining_frames: u8,
    remaining_bytes: u16,
    consecutive: Consecutive<'a, T, R>,
    is_first_frame_read: bool,
    is_flushing: bool,
}

impl<'a, T, R> ConsecutiveTransaction<'a, T, R> {
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
            ready!(self.consecutive.transport.tx.poll_ready_unpin(cx))
                .ok()
                .unwrap();

            let frame = Frame::flow(FlowKind::Continue, block_len, st);
            self.consecutive
                .transport
                .tx
                .start_send_unpin(frame)
                .ok()
                .unwrap();
            self.is_flushing = true;
        } else {
            ready!(self.consecutive.transport.tx.poll_flush_unpin(cx))
                .ok()
                .unwrap();
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
        self.consecutive
            .transport
            .tx
            .send(frame)
            .await
            .ok()
            .unwrap();

        true
    }

    pub async fn wait(self, st: u8)
    where
        T: Sink<Frame> + Unpin,
    {
        self.consecutive.wait(st).await
    }

    pub async fn abort(self)
    where
        T: Sink<Frame> + Unpin,
    {
        self.consecutive.abort().await
    }

    pub fn reader(self, block_len: u8, st: u8) -> ConsecutiveReader<'a, T, R> {
        ConsecutiveReader {
            tranaction: self,
            block_len,
            st,
        }
    }
}

impl<T, R> AsyncRead for ConsecutiveTransaction<'_, T, R>
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

            let data = self.consecutive.first_frame.first_data();
            buf[..data.len()].copy_from_slice(data);
            data.len()
        } else {
            if self.remaining_frames == 0 || self.remaining_bytes == 0 {
                return Poll::Ready(Ok(0));
            }

            let frame = ready!(self.consecutive.transport.rx.poll_next_unpin(cx)).unwrap();
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

pub struct Consecutive<'a, T, R> {
    first_frame: Frame,
    transport: &'a mut Transport<T, R>,
}

impl<'a, T, R> Consecutive<'a, T, R> {
    pub async fn accept(self, block_len: u8, st: u8) -> ConsecutiveTransaction<'a, T, R>
    where
        T: Sink<Frame> + Unpin,
    {
        let frame = Frame::flow(FlowKind::Continue, block_len, st);
        self.transport.tx.send(frame).await.ok().unwrap();

        ConsecutiveTransaction {
            remaining_frames: block_len,
            remaining_bytes: self.first_frame.first_len(),
            consecutive: self,
            is_first_frame_read: false,
            is_flushing: false,
        }
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

pub struct Reader<'a, T, R> {
    read: Read<'a, T, R>,
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
        match self.read {
            Read::Single { ref frame } => {
                let data = frame.single_data();
                buf[..data.len()].copy_from_slice(data);

                Poll::Ready(Ok(data.len()))
            }
            Read::Consecutive(ref mut consecutive) => {
             
                todo!()
            }
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

    pub async fn read(self, buf: &mut [u8], block_len: u8, st: u8)
    where
        T: Sink<Frame> + Unpin,
        R: Stream<Item = Frame> + Unpin,
    {
        match self {
            Self::Single { frame } => {
                let data = frame.single_data();
                buf[..data.len()].copy_from_slice(data);
            }
            Self::Consecutive(consecutive) => {
                let mut reader = consecutive.accept(block_len, st).await;
                let mut pos = 0;
                loop {
                    let used = reader.read(&mut buf[pos..]).await.unwrap();
                    if used == 0 {
                        if !reader.resume(block_len, st).await {
                            return;
                        }
                    }
                    pos += used;
                }
            }
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
                first_frame: frame,
                transport: self,
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
        let transaction = tp.read().await.consecutive().unwrap();

        let mut reader = transaction.accept(10, 0).await;
        let mut buf = [0; 12];
        let used = reader.read(&mut buf).await.unwrap();
        reader.read(&mut buf[used..]).await.unwrap();

        assert_eq!(&buf, bytes);
    }

    #[tokio::test]
    async fn it_reads_consecutive_frames_high_level() {
        let tx: Vec<Frame> = vec![];

        let bytes = b"Hello World!";
        let (first, used) = Frame::first(bytes);
        let (cons, _) = Frame::consecutive(0, &bytes[used..]);

        let rx = stream::iter(vec![first, cons]);

        let mut tp = Transport::new(tx, rx);
        let transaction = tp.read().await;

        let mut buf = [0; 12];
        transaction.read(&mut buf, 10, 0).await;

        assert_eq!(&buf, bytes);
    }
}
