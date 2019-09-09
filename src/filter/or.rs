use std::mem;
use std::pin::Pin;
use std::task::{Context, Poll};
use std::future::Future;

use futures::TryFuture;

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
    T::Error: Unpin,
    U: Filter + Clone + Send + Unpin,
    U::Error: CombineRejection<T::Error> + Unpin,
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
    T::Error: Unpin,
    U: Filter + Unpin,
    U::Error: CombineRejection<T::Error> + Unpin,
{
    type Output = Result<(Either<T::Extract, U::Extract>,), <U::Error as CombineRejection<T::Error>>::Rejection>;

    fn poll(mut self: Pin<&mut Self>, cx: &mut Context) -> Poll<Self::Output> {
        let pin = self.get_mut();
        let err1 = match pin.state {
            State::First(ref mut first, _) => match Pin::new(first).try_poll(cx) {
                Poll::Ready(Ok(ex1)) => {
                    return Poll::Ready(Ok((Either::A(ex1),)));
                }
                Poll::Pending => return Poll::Pending,
                Poll::Ready(Err(e)) => e,
            },
            State::Second(ref mut err1, ref mut second) => {
                return match Pin::new(second).try_poll(cx) {
                    Poll::Ready(Ok(ex2)) => Poll::Ready(Ok((Either::B(ex2),))),
                    Poll::Pending => Poll::Pending,

                    Poll::Ready(Err(e)) => {
                        pin.original_path_index.reset_path();
                        let err1 = err1.take().expect("polled after complete");
                        Poll::Ready(Err(e.combine(err1)))
                    }
                };
            }
            State::Done => panic!("polled after complete"),
        };

        pin.original_path_index.reset_path();

        let mut second = match mem::replace(&mut pin.state, State::Done) {
            State::First(_, second) => second.filter(),
            _ => unreachable!(),
        };

        match Pin::new(&mut second).try_poll(cx) {
            Poll::Ready(Ok(ex2)) => Poll::Ready(Ok((Either::B(ex2),))),
            Poll::Pending => {
                pin.state = State::Second(Some(err1), second);
                Poll::Pending
            }
            Poll::Ready(Err(e)) => {
                pin.original_path_index.reset_path();
                return Poll::Ready(Err(e.combine(err1)));
            }
        }
    }
}
