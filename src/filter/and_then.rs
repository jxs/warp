use std::mem;
use std::pin::Pin;
use std::task::{Context, Poll};
use std::future::Future;

use futures::{ready, TryFuture};

use super::{Filter, FilterBase, Func};
use crate::reject::CombineRejection;

#[derive(Clone, Copy, Debug)]
pub struct AndThen<T, F> {
    pub(super) filter: T,
    pub(super) callback: F,
}

impl<T, F> FilterBase for AndThen<T, F>
where
    T: Filter,
    T::Future: Unpin,
    F: Func<T::Extract> + Clone + Send,
    F::Output: TryFuture + Send + Unpin,
    <F::Output as TryFuture>::Error: CombineRejection<T::Error>,
{
    type Extract = (<F::Output as TryFuture>::Ok,);
    type Error = <<F::Output as TryFuture>::Error as CombineRejection<T::Error>>::Rejection;
    type Future = AndThenFuture<T, F>;
    #[inline]
    fn filter(&self) -> Self::Future {
        AndThenFuture {
            state: State::First(self.filter.filter(), self.callback.clone()),
        }
    }
}

#[allow(missing_debug_implementations)]
pub struct AndThenFuture<T: Filter, F>
where
    T: Filter,
    F: Func<T::Extract>,
    F::Output: TryFuture + Send,
    <F::Output as TryFuture>::Error: CombineRejection<T::Error>,
{
    state: State<T, F>,
}

enum State<T, F>
where
    T: Filter,
    F: Func<T::Extract>,
    F::Output: TryFuture + Send,
    <F::Output as TryFuture>::Error: CombineRejection<T::Error>,
{
    First(T::Future, F),
    Second(F::Output),
    Done,
}

impl<T, F> Future for AndThenFuture<T, F>
where
    T: Filter,
    T::Future: Unpin,
    F: Func<T::Extract>,
    F::Output: TryFuture + Send + Unpin,
    <F::Output as TryFuture>::Error: CombineRejection<T::Error>,
{
    type Output = Result<(<F::Output as TryFuture>::Ok,),
                         <<F::Output as TryFuture>::Error as CombineRejection<T::Error>>::Rejection>;

    fn poll(self: Pin<&mut Self>, cx: &mut Context) -> Poll<Self::Output> {
        let ex1 = match self.state {
            State::First(ref mut first, _) => match ready!(Pin::new(first).try_poll(cx)) {
                Ok(first) => first,
                Err(err) => return Poll::Ready(Err(From::from(err)))
            },
            State::Second(ref mut second) => {
                match ready!(Pin::new(second).try_poll(cx)) {
                    Ok(item) => return Poll::Ready(Ok((item,))),
                    Err(err) => return Poll::Ready(Err(From::from(err)))
                }

            }
            State::Done => panic!("polled after complete"),
        };

        let mut second = match mem::replace(&mut self.state, State::Done) {
            State::First(_, second) => second.call(ex1),
            _ => unreachable!(),
        };

        match Pin::new(&mut second).try_poll(cx)? {
            Poll::Ready(item) => Poll::Ready(Ok((item,))),
            Poll::Pending => {
                self.state = State::Second(second);
                Poll::Pending
            }
        }
    }
}
