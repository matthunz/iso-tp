use crate::{
    frame::{FlowKind, Kind},
    Frame,
};
use async_hal::io::AsyncRead;
use core::{
    marker::PhantomData,
    pin::Pin,
    task::{Context, Poll},
};
use futures::{ready, Sink, SinkExt, Stream, StreamExt};
use pin_project_lite::pin_project;

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

pin_project! {
    pub struct Reader<T, E> {
        #[pin]
        transport: T,

        state: State,
        block_len: u8,
        st: u8,
        _marker: PhantomData<E>
    }
}

impl<T, E> Reader<T, E> {
    /// Create a new reader from a socket.
    pub fn new(transport: T) -> Self {
        Self {
            transport,
            state: State::Empty,
            block_len: 10,
            st: 0,
            _marker: PhantomData,
        }
    }

    /// Abort the current read.
    pub async fn abort(mut self) -> Result<(), T::Error>
    where
        T: Sink<Frame> + Unpin,
    {
        let frame = Frame::flow(FlowKind::Abort, 0, 0);
        self.transport.send(frame).await
    }
}

impl<T, E> AsyncRead for Reader<T, E>
where
    T: Sink<Frame> + Stream<Item = Result<Frame, E>>,
{
    type Error = Error<T::Error, E>;

    fn poll_read(
        self: Pin<&mut Self>,
        cx: &mut Context,
        buf: &mut [u8],
    ) -> Poll<Result<usize, Self::Error>> {
        let mut me = self.project();
        loop {
            match me.state {
                State::Empty => {
                    let frame = ready!(me.transport.poll_next_unpin(cx))
                        .ok_or(Error::UnexpectedEOF)?
                        .map_err(Error::Receive)?;

                    match frame.kind().ok_or(Error::UnknownFrameKind)? {
                        Kind::Single => *me.state = State::Single { frame: Some(frame) },
                        Kind::First => {
                            let data = frame.first_data();
                            *me.state = State::Consecutive {
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
                            ready!(me.transport.poll_flush_unpin(cx)).map_err(Error::Transmit)?;

                            *remaining_frames = *me.block_len;
                            *is_flushing = false;
                        } else {
                            ready!(me.transport.poll_ready_unpin(cx)).map_err(Error::Transmit)?;
                            let frame = Frame::flow(FlowKind::Continue, *me.block_len, *me.st);
                            me.transport
                                .start_send_unpin(frame)
                                .map_err(Error::Transmit)?;
                            *is_flushing = true;
                        }
                    }

                    let frame = ready!(me.transport.poll_next_unpin(cx))
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
