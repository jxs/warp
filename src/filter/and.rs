use std::pin::Pin;
use std::task::{Context, Poll};
use std::future::Future;

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
    T::Extract: Send,
    U: Filter + Clone + Send,
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
    U: Filter,
    //T::Extract: Combine<U::Extract>,
    <T::Extract as Tuple>::HList: Combine<<U::Extract as Tuple>::HList> + Send,
    U::Error: CombineRejection<T::Error>,
{
    type Output = Result<
            <<<T::Extract as Tuple>::HList as Combine<<U::Extract as Tuple>::HList>>::Output as HList>::Tuple,
        <U::Error as CombineRejection<T::Error>>::Rejection>;

    fn poll(mut self: Pin<&mut Self>, cx: &mut Context) -> Poll<Self::Output> {
        let pin = get_unchecked!(self);
        loop {
            let (ex1, fut2) = match pin.state {
                State::First(ref mut first, ref mut second) => match ready!(pin_unchecked!(first).poll(cx)) {
                    Ok(first) => (first, second.filter()),
                    Err(err) => return Poll::Ready(Err(From::from(err)))
                }
                State::Second(ref mut ex1, ref mut second) => {
                    let ex2 = match ready!(pin_unchecked!(second).poll(cx)) {
                        Ok(second) => second,
                        Err(err) => return Poll::Ready(Err(From::from(err)))
                    };
                    let ex3 = ex1.take().unwrap().hlist().combine(ex2.hlist()).flatten();
                    get_unchecked!(self).state = State::Done;
                    return Poll::Ready(Ok(ex3));
                }
                State::Done => panic!("polled after complete"),
            };

            pin.state = State::Second(Some(ex1), fut2);
        }
    }
}
