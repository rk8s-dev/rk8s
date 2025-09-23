use crate::common::*;

#[derive(Debug)]
pub struct TryFlattenResults<I, J> {
    pub(super) iter: Option<I>,
    pub(super) sub_iter: Option<J>,
}

impl<I, J, T, U, E> Iterator for TryFlattenResults<I, J>
where
    I: Iterator<Item = Result<T, E>>,
    J: Iterator<Item = Result<U, E>>,
    T: IntoIterator<Item = Result<U, E>, IntoIter = J>,
{
    type Item = Result<U, E>;

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
                item.into_iter()
            }
        };

        loop {
            match sub_iter.next() {
                Some(Ok(item)) => {
                    self.iter = Some(iter);
                    self.sub_iter = Some(sub_iter);
                    return Some(Ok(item));
                }
                Some(Err(err)) => {
                    return Some(Err(err));
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
                    sub_iter = item.into_iter();
                }
            }
        }
    }
}
