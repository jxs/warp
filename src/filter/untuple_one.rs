use std::future::Future;
use std::pin::Pin;
use std::task::{Context, Poll};

use futures_core::ready;

use super::{Filter, FilterBase, Tuple};

#[derive(Clone, Copy, Debug)]
pub struct UntupleOne<F> {
    pub(super) filter: F,
}

impl<F, T> FilterBase for UntupleOne<F>
where
    F: Filter<Extract = (T,)>,
    T: Tuple,
{
    type Extract = T;
    type Error = F::Error;
    type Future = UntupleOneFuture<F>;
    #[inline]
    fn filter(&self) -> Self::Future {
        UntupleOneFuture {
            extract: self.filter.filter(),
        }
    }
}

#[allow(missing_debug_implementations)]
pub struct UntupleOneFuture<F: Filter> {
    extract: F::Future,
}

impl<F, T> Future for UntupleOneFuture<F>
where
    F: Filter<Extract = (T,)>,
    T: Tuple,
{
    type Output = Result<T, F::Error>;

    #[inline]
    fn poll(mut self: Pin<&mut Self>, cx: &mut Context) -> Poll<Self::Output> {
        let extract = &mut get_unchecked!(self).extract;
        match ready!(pin_unchecked!(extract).poll(cx)) {
            Ok((t,)) => Poll::Ready(Ok(t)),
            Err(err) => Poll::Ready(Err(err)),
        }
    }
}
