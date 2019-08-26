use std::pin::Pin;
use std::task::{Context, Poll};
use std::future::Future;

use futures::{ready, TryFuture};

use super::{Filter, FilterBase, Func};

#[derive(Clone, Copy, Debug)]
pub struct Map<T, F> {
    pub(super) filter: T,
    pub(super) callback: F,
}

impl<T, F> FilterBase for Map<T, F>
where
    T: Filter,
    T::Future: Unpin,
    F: Func<T::Extract> + Clone + Send,
{
    type Extract = (F::Output,);
    type Error = T::Error;
    type Future = MapFuture<T, F>;
    #[inline]
    fn filter(&self) -> Self::Future {
        MapFuture {
            extract: self.filter.filter(),
            callback: self.callback.clone(),
        }
    }
}

#[allow(missing_debug_implementations)]
pub struct MapFuture<T: Filter, F> {
    extract: T::Future,
    callback: F,
}

impl<T, F> Future for MapFuture<T, F>
where
    T: Filter,
    T::Future: Unpin,
    F: Func<T::Extract>,
{
    type Output = Result<(F::Output,), T::Error>;

    #[inline]
    fn poll(self: Pin<&mut Self>, cx: &mut Context) -> Poll<Self::Output> {
        match ready!(Pin::new(&mut self.extract).try_poll(cx)) {
            Ok(ex) => {
                let ex = (self.callback.call(ex),);
                Poll::Ready(Ok(ex))
            },
            Err(err) => return Poll::Ready(Err(err)),
        }
    }
}
