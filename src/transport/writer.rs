use crate::{
    frame::{FlowKind, Kind},
    Frame,
};
use async_hal::{delay::DelayMs, io::AsyncWrite};
use core::{
    marker::PhantomData,
    pin::Pin,
    task::{Context, Poll},
};
use futures::{ready, Sink, SinkExt, Stream};
use pin_project_lite::pin_project;

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

pin_project! {
    /// Writer for an ISO-TP message.
    pub struct Writer<T, E, D> {
        #[pin]
        transport: T,
        #[pin]
        delay: D,
        state: State,
        _marker: PhantomData<E>
    }
}

impl<T, E, D> Writer<T, E, D> {
    pub fn new(transport: T, delay: D) -> Self {
        Self {
            transport,
            delay,
            state: State::Empty,
            _marker: PhantomData,
        }
    }
}

impl<T, E, D> AsyncWrite for Writer<T, E, D>
where
    T: Sink<Frame> + Stream<Item = Result<Frame, E>>,
    D: DelayMs + Unpin,
    D::Delay: From<u8>,
{
    type Error = Error<T::Error, E, D::Error>;

    fn poll_write(
        self: Pin<&mut Self>,
        cx: &mut Context,
        buf: &[u8],
    ) -> Poll<Result<usize, Self::Error>> {
        let mut me = self.project();
        loop {
            match me.state {
                State::Empty => {
                    // Start a new transfer
                    *me.state = if let Some(frame) = Frame::single(buf) {
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
                    ready!(me.transport.as_mut().poll_ready(cx)).map_err(Error::Transmit)?;

                    // Take the current frame so it can only be written once
                    let frame = frame.take().unwrap();
                    me.transport.start_send(frame).map_err(Error::Transmit)?;

                    // Reset the current state
                    *me.state = State::Empty;

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
                        ready!(me.delay.as_mut().poll_delay_ms(cx)).map_err(Error::Delay)?;
                        *is_delaying = false;
                    }

                    if let Some(pos) = pos {
                        // Check if we have any remaining frames left
                        if *remaining == 0 {
                            // Wait for the next frame from `rx`
                            let frame = ready!(me.transport.as_mut().poll_next(cx))
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
                                        .as_mut()
                                        .start(frame.flow_st().into())
                                        .map_err(Error::Delay)?;
                                    *is_delaying = true;

                                    continue;
                                }
                                FlowKind::Abort => {
                                    // Abort this transfer
                                    *me.state = State::Empty;
                                    return Poll::Ready(Err(Error::Aborted));
                                }
                            }

                            *remaining = frame.flow_len();
                            *st = frame.flow_st();
                        }

                        // Send a consecutive frame for the current transfer in progress
                        let (frame, used) = Frame::consecutive(*pos, buf);
                        ready!(poll_send(cx, me.transport.as_mut(), frame))?;

                        // Prepare the state for the next frame
                        *pos += 1;
                        *remaining -= 1;

                        // Delay for the received seperation time
                        me.delay
                            .as_mut()
                            .start(st.clone().into())
                            .map_err(Error::Delay)?;
                        *is_delaying = true;

                        break Poll::Ready(Ok(used));
                    } else {
                        // Send the first frame of this sequence
                        let (frame, used) = Frame::first(buf);
                        ready!(poll_send(cx, me.transport.as_mut(), frame))?;

                        *pos = Some(0);
                        break Poll::Ready(Ok(used));
                    }
                }
            }
        }
    }

    fn poll_flush(self: Pin<&mut Self>, cx: &mut Context) -> Poll<Result<(), Self::Error>> {
        self.project()
            .transport
            .poll_flush(cx)
            .map_err(Error::Transmit)
    }
}

fn poll_send<S, R, D>(
    cx: &mut Context,
    mut tx: S,
    frame: Frame,
) -> Poll<Result<(), Error<S::Error, R, D>>>
where
    S: Sink<Frame> + Unpin,
{
    ready!(tx.poll_ready_unpin(cx)).map_err(Error::Transmit)?;
    tx.start_send_unpin(frame).map_err(Error::Transmit)?;

    Poll::Ready(Ok(()))
}
