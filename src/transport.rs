use async_hal::can::{CanReceive, CanTransmit, Frame as _};
use core::{
    marker::PhantomData,
    pin::Pin,
    task::{Context, Poll},
};
use embedded_hal::can::Id;
use futures::{Sink, SinkExt, Stream, StreamExt};
use pin_project_lite::pin_project;
use crate::Frame;

pin_project! {
    pub struct Transport<C, E, F> {
        id: Id,

        #[pin]
        can: C,
        _marker: PhantomData<(E, F)>,
    }

}

impl<C, E, F> Transport<C, E, F> {
    pub fn new(id: impl Into<Id>, can: C) -> Self {
        Self {
            id: id.into(),
            can,
            _marker: PhantomData,
        }
    }
}

impl<C, E, F> Stream for Transport<C, E, F>
where
    C: CanReceive<Error = E>,
{
    type Item = Result<Frame, E>;

    fn poll_next(self: Pin<&mut Self>, cx: &mut Context) -> Poll<Option<Self::Item>> {
        self.project()
            .can
            .poll_next_unpin(cx)
            .map_ok(|can_frame| Frame::from_bytes(can_frame.data()))
    }
}

impl<C, E, F> Sink<Frame> for Transport<C, E, F>
where
    C: CanTransmit<F>,
    F: async_hal::can::Frame,
{
    type Error = C::Error;

    fn poll_ready(self: Pin<&mut Self>, cx: &mut Context) -> Poll<Result<(), Self::Error>> {
        self.project().can.poll_ready_unpin(cx)
    }

    fn start_send(self: Pin<&mut Self>, item: Frame) -> Result<(), Self::Error> {
        let can_frame = F::new(self.id, item.as_ref()).unwrap();
        self.project().can.start_send_unpin(can_frame)
    }

    fn poll_flush(self: Pin<&mut Self>, cx: &mut Context) -> Poll<Result<(), Self::Error>> {
        self.project().can.poll_flush_unpin(cx)
    }

    fn poll_close(self: Pin<&mut Self>, cx: &mut Context) -> Poll<Result<(), Self::Error>> {
        self.project().can.poll_close_unpin(cx)
    }
}
