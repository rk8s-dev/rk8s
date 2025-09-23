use crate::common::*;

#[derive(Debug)]
pub struct TryFlatMap<I, F, J> {
    pub(super) iter: Option<I>,
    pub(super) sub_iter: Option<J>,
    pub(super) f: F,
}

impl<I, F, J, T, U, V, E> Iterator for TryFlatMap<I, F, J>
where
    I: Iterator<Item = Result<T, E>>,
    J: Iterator<Item = V>,
    F: FnMut(T) -> Result<U, E>,
    U: IntoIterator<Item = V, IntoIter = J>,
{
    type Item = Result<V, E>;

    fn next(&mut self) -> Option<Self::Item> {
        let mut iter = match self.iter.take() {
            Some(iter) => iter,
            None => return None,
        };

        let mut sub_iter = match self.sub_iter.take() {
            Some(sub_iter) => sub_iter,
            None => {
                let item = match iter.next() {
                    Some(Ok(item)) => item,
                    Some(Err(err)) => {
                        return Some(Err(err));
                    }
                    None => {
                        return None;
                    }
                };
                let into_iter = match (self.f)(item) {
                    Ok(into_iter) => into_iter,
                    Err(err) => return Some(Err(err)),
                };
                into_iter.into_iter()
            }
        };

        loop {
            match sub_iter.next() {
                Some(item) => {
                    self.iter = Some(iter);
                    self.sub_iter = Some(sub_iter);
                    return Some(Ok(item));
                }
                None => {
                    let item = match iter.next() {
                        Some(Ok(item)) => item,
                        Some(Err(err)) => {
                            return Some(Err(err));
                        }
                        None => {
                            return None;
                        }
                    };
                    let into_iter = match (self.f)(item) {
                        Ok(into_iter) => into_iter,
                        Err(err) => return Some(Err(err)),
                    };
                    sub_iter = into_iter.into_iter();
                }
            }
        }
    }
}
