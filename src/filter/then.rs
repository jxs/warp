use std::future::Future;
use std::pin::Pin;
use std::task::{Context, Poll};

use futures::{ready, TryFuture};
use pin_project::{pin_project, project};

use super::{Filter, FilterBase, Func, Internal};
use crate::reject::CombineRejection;

#[derive(Clone, Copy, Debug)]
pub struct Then<T, F> {
    pub(super) filter: T,
    pub(super) callback: F,
}

impl<T, F> FilterBase for Then<T, F>
where
    T: Filter,
    F: Func<Result<T::Extract, T::Error>> + Clone + Send,
    F::Output: TryFuture + Send,
    <F::Output as TryFuture>::Error: CombineRejection<T::Error>,
{
    type Extract = (<F::Output as TryFuture>::Ok,);
    type Error = <<F::Output as TryFuture>::Error as CombineRejection<T::Error>>::One;
    type Future = ThenFuture<T, F>;
    #[inline]
    fn filter(&self, _: Internal) -> Self::Future {
        ThenFuture {
            state: State::First(self.filter.filter(Internal), self.callback.clone()),
        }
    }
}

#[allow(missing_debug_implementations)]
#[pin_project]
pub struct ThenFuture<T: Filter, F>
where
    T: Filter,
    F: Func<Result<T::Extract, T::Error>>,
    F::Output: TryFuture + Send,
    <F::Output as TryFuture>::Error: CombineRejection<T::Error>,
{
    #[pin]
    state: State<T, F>,
}

#[pin_project]
enum State<T, F>
where
    T: Filter,
    F: Func<Result<T::Extract, T::Error>>,
    F::Output: TryFuture + Send,
    <F::Output as TryFuture>::Error: CombineRejection<T::Error>,
{
    First(#[pin] T::Future, F),
    Second(#[pin] F::Output),
    Done,
}

impl<T, F> Future for ThenFuture<T, F>
where
    T: Filter,
    F: Func<Result<T::Extract, T::Error>>,
    F::Output: TryFuture + Send,
    <F::Output as TryFuture>::Error: CombineRejection<T::Error>,
{
    type Output = Result<
        (<F::Output as TryFuture>::Ok,),
        <<F::Output as TryFuture>::Error as CombineRejection<T::Error>>::One,
    >;

    #[project]
    fn poll(mut self: Pin<&mut Self>, cx: &mut Context) -> Poll<Self::Output> {
        loop {
            let pin = self.as_mut().project();
            #[project]
            let (ex1, second) = match pin.state.project() {
                State::First(first, second) => {
                    let ex1 = ready!(first.try_poll(cx));
                    (ex1, second)
                }
                State::Second(second) => {
                    let ex3 = match ready!(second.try_poll(cx)) {
                        Ok(item) => Ok((item,)),
                        Err(err) => Err(From::from(err)),
                    };
                    self.set(ThenFuture { state: State::Done });
                    return Poll::Ready(ex3);
                }
                State::Done => panic!("polled after complete"),
            };
            let fut2 = second.call(ex1);
            self.set(ThenFuture {
                state: State::Second(fut2),
            });
        }
    }
}
