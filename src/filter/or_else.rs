use std::mem;
use std::pin::Pin;
use std::task::{Context, Poll};
use std::future::Future;

use futures::TryFuture;

use super::{Filter, FilterBase, Func};
use crate::route;

#[derive(Clone, Copy, Debug)]
pub struct OrElse<T, F> {
    pub(super) filter: T,
    pub(super) callback: F,
}

impl<T, F> FilterBase for OrElse<T, F>
where
    T: Filter,
    T::Future: Unpin,
    F: Func<T::Error> + Clone + Send,
    F::Output: TryFuture<Ok =  T::Extract, Error = T::Error> + Send + Unpin,
{
    type Extract = <F::Output as TryFuture>::Ok;
    type Error = <F::Output as TryFuture>::Error;
    type Future = OrElseFuture<T, F>;
    #[inline]
    fn filter(&self) -> Self::Future {
        let idx = route::with(|route| route.matched_path_index());
        OrElseFuture {
            state: State::First(self.filter.filter(), self.callback.clone()),
            original_path_index: PathIndex(idx),
        }
    }
}

#[allow(missing_debug_implementations)]
pub struct OrElseFuture<T: Filter, F>
where
    T: Filter,
    F: Func<T::Error>,
    F::Output: TryFuture<Ok =  T::Extract, Error = T::Error> + Send,
{
    state: State<T, F>,
    original_path_index: PathIndex,
}

enum State<T, F>
where
    T: Filter,
    F: Func<T::Error>,
    F::Output: TryFuture<Ok =  T::Extract, Error = T::Error> + Send,
{
    First(T::Future, F),
    Second(F::Output),
    Done,
}

struct PathIndex(usize);

impl PathIndex {
    fn reset_path(&self) {
        route::with(|route| route.reset_matched_path_index(self.0));
    }
}

impl<T, F> Future for OrElseFuture<T, F>
where
    T: Filter,
    T::Future: Unpin,
    F: Func<T::Error>,
    F::Output: TryFuture<Ok =  T::Extract, Error = T::Error> + Send + Unpin,
{
    type Output = Result<<F::Output as TryFuture>::Ok, <F::Output as TryFuture>::Error>;

    fn poll(self: Pin<&mut Self>, cx: &mut Context) -> Poll<Self::Output> {
        let err = match self.state {
            State::First(ref mut first, _) => match Pin::new(first).try_poll(cx) {
                Poll::Ready(Ok(ex)) => return Poll::Ready(Ok(ex)),
                Poll::Pending => return Poll::Pending,
                Poll::Ready(Err(err)) => err,
            },
            State::Second(ref mut second) => {
                return Pin::new(second).try_poll(cx);
            }
            State::Done => panic!("polled after complete"),
        };

        self.original_path_index.reset_path();

        let mut second = match mem::replace(&mut self.state, State::Done) {
            State::First(_, second) => second.call(err),
            _ => unreachable!(),
        };

        match Pin::new(&mut second).try_poll(cx) {
            Poll::Ready(item) => Poll::Ready(item),
            Poll::Pending => {
                self.state = State::Second(second);
                Poll::Pending
            }
        }
    }
}
