use std::future::Future;
use std::pin::Pin;
use std::task::{Context, Poll};

use futures_core::ready;

use super::{Filter, FilterBase, Func};

#[derive(Clone, Copy, Debug)]
pub struct Map<T, F> {
    pub(super) filter: T,
    pub(super) callback: F,
}

impl<T, F> FilterBase for Map<T, F>
where
    T: Filter,
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
    F: Func<T::Extract>,
{
    type Output = Result<(F::Output,), T::Error>;

    #[inline]
    fn poll(mut self: Pin<&mut Self>, cx: &mut Context) -> Poll<Self::Output> {
        let extract = &mut get_unchecked!(self).extract;
        match ready!(pin_unchecked!(extract).poll(cx)) {
            Ok(ex) => {
                let ex = (self.callback.call(ex),);
                Poll::Ready(Ok(ex))
            }
            Err(err) => return Poll::Ready(Err(err)),
        }
    }
}
