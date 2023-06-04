use super::Reader;
use crate::{
    frame::{FlowKind, Kind},
    Frame, Socket,
};
use async_hal::io::AsyncRead;
use core::{
    pin::Pin,
    task::{Context, Poll},
};
use futures::{ready, Sink, SinkExt, Stream, StreamExt};

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
        Reader::new(self, block_len, st)
    }
}

pub struct Consecutive<'a, T, R> {
    remaining_frames: u8,
    remaining_bytes: u16,
    first_frame: Frame,
    socket: &'a mut Socket<T, R>,
    is_first_frame_read: bool,
    is_flushing: bool,
}

impl<'a, T, R> Consecutive<'a, T, R> {
    pub(crate) fn new(frame: Frame, socket: &'a mut Socket<T, R>) -> Self {
        Consecutive {
            socket,
            remaining_frames: 0,
            is_flushing: false,
            is_first_frame_read: false,
            remaining_bytes: frame.first_len(),
            first_frame: frame,
        }
    }
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
            ready!(self.socket.tx.poll_ready_unpin(cx)).ok().unwrap();

            let frame = Frame::flow(FlowKind::Continue, block_len, st);
            self.socket.tx.start_send_unpin(frame).ok().unwrap();
            self.is_flushing = true;
        } else {
            ready!(self.socket.tx.poll_flush_unpin(cx)).ok().unwrap();
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
        self.socket.tx.send(frame).await.ok().unwrap();

        true
    }

    pub async fn wait(self, st: u8)
    where
        T: Sink<Frame> + Unpin,
    {
        let frame = Frame::flow(FlowKind::Wait, 0, st);
        self.socket.tx.send(frame).await.ok().unwrap();
    }

    pub async fn abort(self)
    where
        T: Sink<Frame> + Unpin,
    {
        let frame = Frame::flow(FlowKind::Abort, 0, 0);
        self.socket.tx.send(frame).await.ok().unwrap();
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

            let frame = ready!(self.socket.rx.poll_next_unpin(cx)).unwrap();
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
