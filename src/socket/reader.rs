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

enum State {
    Empty,
    Single {
        frame: Option<Frame>,
    },
    Consecutive {
        remaining_bytes: u16,
        remaining_frames: u8,
        index: u8,
        is_flushing: bool,
    },
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Error<T, R> {
    Transmit(T),
    Receive(R),
    InvalidFrame,
    UnknownFrameKind,
    UnexpectedEOF,
}

pub struct Reader<'a, T, R, E> {
    socket: &'a mut Socket<T, R, E>,
    state: State,
    block_len: u8,
    st: u8,
}

impl<'a, T, R, E> Reader<'a, T, R, E> {
    pub(crate) fn new(socket: &'a mut Socket<T, R, E>) -> Self {
        Self {
            socket,
            state: State::Empty,
            block_len: 10,
            st: 0,
        }
    }
}

impl<T, R, E> AsyncRead for Reader<'_, T, R, E>
where
    T: Sink<Frame> + Unpin,
    R: Stream<Item = Result<Frame, E>> + Unpin,
{
    type Error = Error<T::Error, E>;

    fn poll_read(
        mut self: Pin<&mut Self>,
        cx: &mut Context,
        buf: &mut [u8],
    ) -> Poll<Result<usize, Self::Error>> {
        let me = &mut *self;
        loop {
            match &mut me.state {
                State::Empty => {
                    let frame = ready!(me.socket.rx.poll_next_unpin(cx))
                        .ok_or(Error::UnexpectedEOF)?
                        .map_err(Error::Receive)?;

                    match frame.kind().ok_or(Error::UnknownFrameKind)? {
                        Kind::Single => me.state = State::Single { frame: Some(frame) },
                        Kind::First => {
                            let data = frame.first_data();
                            me.state = State::Consecutive {
                                remaining_bytes: frame.first_len() - data.len() as u16,
                                remaining_frames: 0,
                                index: 0,
                                is_flushing: false,
                            };
                            buf[..data.len()].copy_from_slice(data);
                            break Poll::Ready(Ok(data.len()));
                        }
                        Kind::Consecutive | Kind::Flow => {
                            break Poll::Ready(Err(Error::InvalidFrame))
                        }
                    }
                }
                State::Single { frame } => {
                    let used = if let Some(frame) = frame.take() {
                        let data = frame.single_data();
                        buf[..data.len()].copy_from_slice(data);

                        data.len()
                    } else {
                        0
                    };
                    break Poll::Ready(Ok(used));
                }
                State::Consecutive {
                    remaining_bytes,
                    remaining_frames,
                    index,
                    is_flushing,
                } => {
                    if *remaining_bytes == 0 {
                        break Poll::Ready(Ok(0));
                    }

                    while *remaining_frames == 0 {
                        if *is_flushing {
                            ready!(me.socket.tx.poll_flush_unpin(cx)).map_err(Error::Transmit)?;

                            *remaining_frames = me.block_len;
                            *is_flushing = false;
                        } else {
                            ready!(me.socket.tx.poll_ready_unpin(cx)).map_err(Error::Transmit)?;
                            let frame = Frame::flow(FlowKind::Continue, me.block_len, me.st);
                            me.socket
                                .tx
                                .start_send_unpin(frame)
                                .map_err(Error::Transmit)?;
                            *is_flushing = true;
                        }
                    }

                    let frame = ready!(me.socket.rx.poll_next_unpin(cx))
                        .ok_or(Error::UnexpectedEOF)?
                        .map_err(Error::Receive)?;
                    if frame.kind().ok_or(Error::UnknownFrameKind)? != Kind::Consecutive {
                        return Poll::Ready(Err(Error::InvalidFrame));
                    }

                    let data = frame.consecutive_data();
                    let used = core::cmp::min(data.len(), *remaining_bytes as _);
                    buf[..used].copy_from_slice(&data[..used]);

                    *index += 1;
                    *remaining_frames -= 1;

                    break Poll::Ready(Ok(used));
                }
            }
        }
    }
}
