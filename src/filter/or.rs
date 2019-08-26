use std::pin::Pin;
use std::task::{Context, Poll};
use std::future::Future;

use futures::{ready, TryFuture};

use super::{Filter, FilterBase};
use crate::generic::Either;
use crate::reject::CombineRejection;
use crate::route;

#[derive(Clone, Copy, Debug)]
pub struct Or<T, U> {
    pub(super) first: T,
    pub(super) second: U,
}

impl<T, U> FilterBase for Or<T, U>
where
    T: Filter,
    U: Filter + Clone + Send,
    U::Error: CombineRejection<T::Error>,
{
    type Extract = (Either<T::Extract, U::Extract>,);
    type Error = <U::Error as CombineRejection<T::Error>>::Rejection;
    type Future = EitherFuture<T, U>;

    fn filter(&self) -> Self::Future {
        let idx = route::with(|route| route.matched_path_index());
        EitherFuture {
            state: State::First(self.first.filter(), self.second.clone()),
            original_path_index: PathIndex(idx),
        }
    }
}

#[allow(missing_debug_implementations)]
pub struct EitherFuture<T: Filter, U: Filter> {
    state: State<T, U>,
    original_path_index: PathIndex,
}

enum State<T: Filter, U: Filter> {
    First(T::Future, U),
    Second(Option<T::Error>, U::Future),
    Done,
}

struct PathIndex(usize);

impl PathIndex {
    fn reset_path(&self) {
        route::with(|route| route.reset_matched_path_index(self.0));
    }
}

impl<T, U> Future for EitherFuture<T, U>
where
    T: Filter,
    U: Filter,
    U::Error: CombineRejection<T::Error>,
{
    type Output = Result<(Either<T::Extract, U::Extract>,), <U::Error as CombineRejection<T::Error>>::Rejection>;

    fn poll(mut self: Pin<&mut Self>, cx: &mut Context) -> Poll<Self::Output> {
        let pin = get_unchecked!(self);
        loop {
            let (err1, fut2) = match pin.state {
                State::First(ref mut first, ref mut second) => match ready!(pin_unchecked!(first).try_poll(cx)) {
                    Ok(ex1) => {
                        return Poll::Ready(Ok((Either::A(ex1),)));
                    }
                    Err(e) => {
                        pin.original_path_index.reset_path();
                        (e, second.filter())
                    }
                },
                State::Second(ref mut err1, ref mut second) => {
                    let ex2 = match ready!(pin_unchecked!(second).try_poll(cx)) {
                        Ok(ex2) => {
                            Ok((Either::B(ex2),))
                        },
                        Err(e) => {
                            pin.original_path_index.reset_path();
                            let err1 = err1.take().expect("polled after complete");
                            Err(e.combine(err1))
                        }
                    };
                    pin.state = State::Done;
                    return Poll::Ready(ex2)
                }
                State::Done => panic!("polled after complete"),
            };

            pin.state = State::Second(Some(err1), fut2);
        }
    }
}
