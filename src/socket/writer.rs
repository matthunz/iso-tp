use crate::{
    frame::{FlowKind, Kind},
    Frame, Socket,
};
use async_hal::io::AsyncWrite;
use core::{
    pin::Pin,
    task::{Context, Poll},
};
use futures::{ready, Sink, SinkExt, Stream, StreamExt};

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Error<T> {
    Transmit(T),
    InvalidFrame,
    Aborted,
    UnexpectedEOF,
}

impl<T> From<T> for Error<T> {
    fn from(value: T) -> Self {
        Self::Transmit(value)
    }
}

enum State {
    Empty,
    Single { frame: Frame },
    Consecutive { pos: Option<u8>, remaining: u8 },
}

pub struct Writer<'a, T, R> {
    socket: &'a mut Socket<T, R>,
    state: State,
}

impl<'a, T, R> Writer<'a, T, R> {
    pub(crate) fn new(socket: &'a mut Socket<T, R>) -> Self {
        Self {
            socket,
            state: State::Empty,
        }
    }
}

impl<T, R> AsyncWrite for Writer<'_, T, R>
where
    T: Sink<Frame> + Unpin,
    R: Stream<Item = Frame> + Unpin,
{
    type Error = Error<T::Error>;

    fn poll_write(
        mut self: Pin<&mut Self>,
        cx: &mut Context,
        buf: &[u8],
    ) -> Poll<Result<usize, Self::Error>> {
        let me = &mut *self;
        loop {
            match me.state {
                State::Empty => {
                    me.state = if let Some(frame) = Frame::single(buf) {
                        State::Single { frame }
                    } else {
                        State::Consecutive {
                            pos: None,
                            remaining: 0,
                        }
                    };
                }
                State::Single { ref frame } => {
                    ready!(me.socket.tx.poll_ready_unpin(cx))?;
                    me.socket.tx.start_send_unpin(frame.clone())?;
                    me.state = State::Empty;
                    break Poll::Ready(Ok(buf.len()));
                }
                State::Consecutive {
                    ref mut pos,
                    ref mut remaining,
                } => {
                    if let Some(pos) = pos {
                        if *remaining == 0 {
                            let frame = ready!(me.socket.rx.poll_next_unpin(cx))
                                .ok_or(Error::UnexpectedEOF)?;

                            if frame.kind() != Some(Kind::Flow) {
                                return Poll::Ready(Err(Error::InvalidFrame));
                            }

                            match frame.flow_kind() {
                                FlowKind::Continue => {}
                                FlowKind::Wait => todo!(),
                                FlowKind::Abort => return Poll::Ready(Err(Error::Aborted)),
                            }

                            *remaining = frame.flow_len();
                        }

                        let (frame, used) = Frame::consecutive(*pos, buf);
                        ready!(me.socket.tx.poll_ready_unpin(cx))?;
                        me.socket.tx.start_send_unpin(frame)?;

                        *pos += 1;
                        *remaining -= 1;

                        break Poll::Ready(Ok(used));
                    } else {
                        let (frame, used) = Frame::first(buf);
                        ready!(me.socket.tx.poll_ready_unpin(cx))?;
                        me.socket.tx.start_send_unpin(frame)?;

                        *pos = Some(0);
                        break Poll::Ready(Ok(used));
                    }
                }
            }
        }
    }

    fn poll_flush(mut self: Pin<&mut Self>, cx: &mut Context) -> Poll<Result<(), Self::Error>> {
        self.socket.tx.poll_flush_unpin(cx).map_err(Into::into)
    }
}
