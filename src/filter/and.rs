use std::mem;
use std::pin::Pin;
use std::task::{Context, Poll};
use std::future::Future;

use futures::future::TryFuture;

use futures::ready;

use super::{Combine, Filter, FilterBase, HList, Tuple};
use crate::reject::CombineRejection;

#[derive(Clone, Copy, Debug)]
pub struct And<T, U> {
    pub(super) first: T,
    pub(super) second: U,
}

impl<T, U> FilterBase for And<T, U>
where
    T: Filter,
    T::Future: Unpin,
    U: Filter + Clone + Send,
    U::Future: Unpin,
    T::Extract: Send,
    <T::Extract as Tuple>::HList: Combine<<U::Extract as Tuple>::HList> + Send,
    <<<T::Extract as Tuple>::HList as Combine<<U::Extract as Tuple>::HList>>::Output as HList>::Tuple: Send,
    U::Error: CombineRejection<T::Error>,
{
    type Extract = <<<T::Extract as Tuple>::HList as Combine<<U::Extract as Tuple>::HList>>::Output as HList>::Tuple;
    type Error = <U::Error as CombineRejection<T::Error>>::Rejection;
    type Future = AndFuture<T, U>;

    fn filter(&self) -> Self::Future {
        AndFuture {
            state: State::First(self.first.filter(), self.second.clone()),
        }
    }
}

#[allow(missing_debug_implementations)]
pub struct AndFuture<T: Filter, U: Filter> {
    state: State<T, U>,
}

enum State<T: Filter, U: Filter> {
    First(T::Future, U),
    Second(Option<T::Extract>, U::Future),
    Done,
}

impl<T, U> Future for AndFuture<T, U>
where
    T: Filter,
    T::Future: Unpin,
    U: Filter,
    U::Future: Unpin,
    //T::Extract: Combine<U::Extract>,
    <T::Extract as Tuple>::HList: Combine<<U::Extract as Tuple>::HList> + Send,
    U::Error: CombineRejection<T::Error>,
{
    //type Item = <T::Extract as Combine<U::Extract>>::Output;
    type Output = Result<
            <<<T::Extract as Tuple>::HList as Combine<<U::Extract as Tuple>::HList>>::Output as HList>::Tuple,
        <U::Error as CombineRejection<T::Error>>::Rejection>;

    fn poll(self: Pin<&mut Self>, cx: &mut Context) -> Poll<Self::Output> {
        let ex1 = match self.state {
            State::First(ref mut first, _) => match ready!(Pin::new(first).try_poll(cx)) {
                Ok(first) => first,
                Err(err) => return Poll::Ready(Err(From::from(err)))

            }
            State::Second(ref mut ex1, ref mut second) => {
                let ex2 = match ready!(Pin::new(second).try_poll(cx)) {
                    Ok(second) => second,
                    Err(err) => return Poll::Ready(Err(From::from(err)))
                };
                let ex3 = ex1.take().unwrap().hlist().combine(ex2.hlist()).flatten();
                return Poll::Ready(Ok(ex3));
            }
            State::Done => panic!("polled after complete"),
        };

        let mut second = match mem::replace(&mut self.state, State::Done) {
            State::First(_, second) => second.filter(),
            _ => unreachable!(),
        };

        match Pin::new(&mut second).try_poll(cx)? {
            Poll::Ready(ex2) => Poll::Ready(Ok(ex1.hlist().combine(ex2.hlist()).flatten())),
            Poll::Pending => {
                self.state = State::Second(Some(ex1), second);
                Poll::Pending
            }
        }
    }
}
