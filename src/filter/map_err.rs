use std::pin::Pin;
use std::task::{Context, Poll};
use std::future::Future;

use futures::TryFuture;

use super::{Filter, FilterBase};
use crate::reject::Reject;

#[derive(Clone, Copy, Debug)]
pub struct MapErr<T, F> {
    pub(super) filter: T,
    pub(super) callback: F,
}

impl<T, F, E> FilterBase for MapErr<T, F>
where
    T: Filter,
    T::Future: Unpin,
    F: Fn(T::Error) -> E + Clone + Send,
    E: Reject,
{
    type Extract = T::Extract;
    type Error = E;
    type Future = MapErrFuture<T, F>;
    #[inline]
    fn filter(&self) -> Self::Future {
        MapErrFuture {
            extract: self.filter.filter(),
            callback: self.callback.clone(),
        }
    }
}

#[allow(missing_debug_implementations)]
pub struct MapErrFuture<T: Filter, F> {
    extract: T::Future,
    callback: F,
}

impl<T, F, E> Future for MapErrFuture<T, F>
where
    T: Filter,
    T::Future: Unpin,
    F: Fn(T::Error) -> E,
{
    type Output = Result<T::Extract, E>;

    #[inline]
    fn poll(self: Pin<&mut Self>, cx: &mut Context) -> Poll<Self::Output> {
        Pin::new(&mut self.extract).try_poll(cx).map_err(|err| (self.callback)(err))
    }
}
