use super::Read;
use crate::Frame;
use async_hal::io::AsyncRead;
use core::{
    pin::Pin,
    task::{Context, Poll},
};
use futures::{ready, Sink, Stream};

pub struct Reader<'a, T, R> {
    read: Read<'a, T, R>,
    block_len: u8,
    st: u8,
}

impl<'a, T, R> Reader<'a, T, R> {
    pub(crate) fn new(read: Read<'a, T, R>, block_len: u8, st: u8) -> Self {
        Self {
            read,
            block_len,
            st,
        }
    }
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
