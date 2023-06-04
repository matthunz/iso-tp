use super::Error;
use crate::{
    frame::{FlowKind, Kind},
    Frame, Socket,
};
use async_hal::{delay::DelayMs, io::AsyncWrite};
use core::{
    marker::PhantomData,
    pin::Pin,
    task::{Context, Poll},
};
use futures::{ready, Sink, SinkExt, Stream, StreamExt};

enum State {
    Empty,
    Single {
        frame: Frame,
    },
    Consecutive {
        pos: Option<u8>,
        remaining: u8,
        is_delaying: bool,
        st: u8,
    },
}

pub struct Writer<'a, T, R, E, D> {
    socket: &'a mut Socket<T, R>,
    delay: D,
    state: State,
    _marker: PhantomData<E>,
}

impl<T: Unpin, R: Unpin, E, D> Unpin for Writer<'_, T, R, E, D> {}

impl<'a, T, R, E, D> Writer<'a, T, R, E, D> {
    pub(crate) fn new(socket: &'a mut Socket<T, R>, delay: D) -> Self {
        Self {
            socket,
            delay,
            state: State::Empty,
            _marker: PhantomData,
        }
    }
}

impl<T, R, E, D> AsyncWrite for Writer<'_, T, R, E, D>
where
    T: Sink<Frame> + Unpin,
    R: Stream<Item = Result<Frame, E>> + Unpin,
    D: DelayMs + Unpin,
    D::Delay: From<u8>,
{
    type Error = Error<T::Error, E, D::Error>;

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
                            is_delaying: false,
                            st: 0,
                        }
                    };
                }
                State::Single { ref frame } => {
                    ready!(me.socket.tx.poll_ready_unpin(cx)).map_err(Error::Transmit)?;
                    me.socket
                        .tx
                        .start_send_unpin(frame.clone())
                        .map_err(Error::Transmit)?;
                    me.state = State::Empty;
                    break Poll::Ready(Ok(buf.len()));
                }
                State::Consecutive {
                    ref mut pos,
                    ref mut remaining,
                    ref mut is_delaying,
                    ref mut st,
                } => {
                    if *is_delaying {
                        ready!(me.delay.poll_delay_ms_unpin(cx)).map_err(Error::Delay)?;
                        *is_delaying = false;
                    }

                    if let Some(pos) = pos {
                        if *remaining == 0 {
                            let frame = ready!(me.socket.rx.poll_next_unpin(cx))
                                .ok_or(Error::UnexpectedEOF)?
                                .map_err(|e| Error::Receive(e))?;

                            if frame.kind() != Some(Kind::Flow) {
                                return Poll::Ready(Err(Error::InvalidFrame));
                            }

                            match frame.flow_kind() {
                                FlowKind::Continue => {}
                                FlowKind::Wait => todo!(),
                                FlowKind::Abort => {
                                    me.state = State::Empty;
                                    return Poll::Ready(Err(Error::Aborted));
                                }
                            }

                            *remaining = frame.flow_len();
                            *st = frame.flow_st();
                        }

                        let (frame, used) = Frame::consecutive(*pos, buf);
                        ready!(me.socket.tx.poll_ready_unpin(cx)).map_err(Error::Transmit)?;
                        me.socket
                            .tx
                            .start_send_unpin(frame)
                            .map_err(Error::Transmit)?;

                        *pos += 1;
                        *remaining -= 1;

                        me.delay.start(st.clone().into()).map_err(Error::Delay)?;
                        *is_delaying = true;

                        break Poll::Ready(Ok(used));
                    } else {
                        let (frame, used) = Frame::first(buf);
                        ready!(me.socket.tx.poll_ready_unpin(cx)).map_err(Error::Transmit)?;
                        me.socket
                            .tx
                            .start_send_unpin(frame)
                            .map_err(Error::Transmit)?;

                        *pos = Some(0);
                        break Poll::Ready(Ok(used));
                    }
                }
            }
        }
    }

    fn poll_flush(mut self: Pin<&mut Self>, cx: &mut Context) -> Poll<Result<(), Self::Error>> {
        self.socket.tx.poll_flush_unpin(cx).map_err(Error::Transmit)
    }
}
