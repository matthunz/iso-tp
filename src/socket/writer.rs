use crate::{
    frame::{FlowKind, Kind},
    Frame, Socket,
};
use async_hal::{delay::DelayMs, io::AsyncWrite};
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
        pos: Option<u8>,
        remaining: u8,
        is_delaying: bool,
        st: u8,
    },
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Error<T, R, D> {
    Transmit(T),
    Receive(R),
    Delay(D),
    InvalidFrame,
    Aborted,
    UnexpectedEOF,
}

/// Writer for an ISO-TP message.
pub struct Writer<'a, T, R, E, D> {
    socket: &'a mut Socket<T, R, E>,
    delay: D,
    state: State,
}

impl<'a, T, R, E, D> Writer<'a, T, R, E, D> {
    pub(crate) fn new(socket: &'a mut Socket<T, R, E>, delay: D) -> Self {
        Self {
            socket,
            delay,
            state: State::Empty,
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
            match &mut me.state {
                State::Empty => {
                    // Start a new transfer
                    me.state = if let Some(frame) = Frame::single(buf) {
                        State::Single { frame: Some(frame) }
                    } else {
                        State::Consecutive {
                            pos: None,
                            remaining: 0,
                            is_delaying: false,
                            st: 0,
                        }
                    };
                }
                State::Single { frame } => {
                    if frame.is_none() {
                        // This transfer is already finished
                        break Poll::Ready(Ok(0));
                    }

                    // Send a single frame
                    ready!(me.socket.tx.poll_ready_unpin(cx)).map_err(Error::Transmit)?;

                    // Take the current frame so it can only be written once
                    let frame = frame.take().unwrap();
                    me.socket
                        .tx
                        .start_send_unpin(frame)
                        .map_err(Error::Transmit)?;

                    // Reset the current state
                    me.state = State::Empty;

                    // Return the total len of `buf`
                    break Poll::Ready(Ok(buf.len()));
                }
                State::Consecutive {
                    pos,
                    remaining,
                    is_delaying,
                    st,
                } => {
                    // Poll the current delay if it's in progress
                    if *is_delaying {
                        ready!(me.delay.poll_delay_ms_unpin(cx)).map_err(Error::Delay)?;
                        *is_delaying = false;
                    }

                    if let Some(pos) = pos {
                        // Check if we have any remaining frames left
                        if *remaining == 0 {
                            // Wait for the next frame from `rx`
                            let frame = ready!(me.socket.rx.poll_next_unpin(cx))
                                .ok_or(Error::UnexpectedEOF)?
                                .map_err(Error::Receive)?;

                            // Make sure the frame is control flow
                            if frame.kind() != Some(Kind::Flow) {
                                return Poll::Ready(Err(Error::InvalidFrame));
                            }

                            // Handle control flow kinds
                            match frame.flow_kind() {
                                FlowKind::Continue => {}
                                FlowKind::Wait => {
                                    // Delay for the received wait time
                                    me.delay
                                        .start(frame.flow_st().into())
                                        .map_err(Error::Delay)?;
                                    *is_delaying = true;

                                    continue;
                                }
                                FlowKind::Abort => {
                                    // Abort this transfer
                                    me.state = State::Empty;
                                    return Poll::Ready(Err(Error::Aborted));
                                }
                            }

                            *remaining = frame.flow_len();
                            *st = frame.flow_st();
                        }

                        // Send a consecutive frame for the current transfer in progress
                        let (frame, used) = Frame::consecutive(*pos, buf);
                        ready!(poll_send(cx, &mut me.socket.tx, frame))?;

                        // Prepare the state for the next frame
                        *pos += 1;
                        *remaining -= 1;

                        // Delay for the received seperation time
                        me.delay.start(st.clone().into()).map_err(Error::Delay)?;
                        *is_delaying = true;

                        break Poll::Ready(Ok(used));
                    } else {
                        // Send the first frame of this sequence
                        let (frame, used) = Frame::first(buf);
                        ready!(poll_send(cx, &mut me.socket.tx, frame))?;

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

fn poll_send<S, R, D>(
    cx: &mut Context,
    tx: &mut S,
    frame: Frame,
) -> Poll<Result<(), Error<S::Error, R, D>>>
where
    S: Sink<Frame> + Unpin,
{
    ready!(tx.poll_ready_unpin(cx)).map_err(Error::Transmit)?;
    tx.start_send_unpin(frame).map_err(Error::Transmit)?;

    Poll::Ready(Ok(()))
}
