use std::pin::Pin;
use std::task::{Context, Poll};
use std::future::Future;

use futures::{ready, TryFuture};

use super::{Filter, FilterBase, Func};
use crate::generic::Either;
use crate::route;

#[derive(Clone, Copy, Debug)]
pub struct Recover<T, F> {
    pub(super) filter: T,
    pub(super) callback: F,
}

impl<T, F> FilterBase for Recover<T, F>
where
    T: Filter,
    F: Func<T::Error> + Clone + Send,
    F::Output: TryFuture<Error = T::Error> + Send,
{
    type Extract = (Either<T::Extract, (<F::Output as TryFuture>::Ok,)>,);
    type Error = <F::Output as TryFuture>::Error;
    type Future = RecoverFuture<T, F>;
    #[inline]
    fn filter(&self) -> Self::Future {
        let idx = route::with(|route| route.matched_path_index());
        RecoverFuture {
            state: State::First(self.filter.filter(), self.callback.clone()),
            original_path_index: PathIndex(idx),
        }
    }
}

#[allow(missing_debug_implementations)]
pub struct RecoverFuture<T: Filter, F>
where
    T: Filter,
    F: Func<T::Error>,
    F::Output: TryFuture<Error = T::Error> + Send,
{
    state: State<T, F>,
    original_path_index: PathIndex,
}

enum State<T, F>
where
    T: Filter,
    F: Func<T::Error>,
    F::Output: TryFuture<Error = T::Error> + Send,
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

impl<T, F> Future for RecoverFuture<T, F>
where
    T: Filter,
    F: Func<T::Error>,
    F::Output: TryFuture<Error = T::Error> + Send,
{
    // type Item = (Either<T::Extract, (<F::Output as IntoFuture>::Item,)>,);
    // type Error = <F::Output as IntoFuture>::Error;
    type Output = Result<(Either<T::Extract, (<F::Output as TryFuture>::Ok,)>,), <F::Output as TryFuture>::Error>;

    fn poll(mut self: Pin<&mut Self>, cx: &mut Context) -> Poll<Self::Output> {
        let pin = get_unchecked!(self);
        loop {
            let (err, second) = match pin.state {
                State::First(ref mut first, ref mut second) => match ready!(pin_unchecked!(first).try_poll(cx)) {
                    Ok(ex) => return Poll::Ready(Ok((Either::A(ex),))),
                    Err(err) => (err, second),
                },
                State::Second(ref mut second) => {
                    let ex2 = match ready!(pin_unchecked!(second).try_poll(cx)) {
                        Ok(ex2) => Ok((Either::B((ex2,)),)),
                        Err(e) => Err(e),
                    };
                    pin.state = State::Done;
                    return Poll::Ready(ex2);
                }
                State::Done => panic!("polled after complete"),
            };

            pin.original_path_index.reset_path();
            pin.state = State::Second(second.call(err));
        }
    }
}
