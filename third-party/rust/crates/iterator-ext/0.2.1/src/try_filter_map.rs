use crate::common::*;

#[derive(Debug)]
pub struct TryFilterMap<I, F> {
    pub(super) iter: Option<I>,
    pub(super) f: F,
}

impl<I, T, U, E, F> Iterator for TryFilterMap<I, F>
where
    I: Iterator<Item = Result<T, E>>,
    F: FnMut(T) -> Result<Option<U>, E>,
{
    type Item = Result<U, E>;

    fn next(&mut self) -> Option<Self::Item> {
        let mut iter = match self.iter.take() {
            Some(iter) => iter,
            None => return None,
        };

        loop {
            let input_item = match iter.next() {
                Some(Ok(item)) => item,
                Some(Err(err)) => {
                    return Some(Err(err));
                }
                None => {
                    return None;
                }
            };
            match (self.f)(input_item) {
                Ok(Some(output_item)) => {
                    self.iter = Some(iter);
                    return Some(Ok(output_item));
                }
                Ok(None) => {}
                Err(err) => {
                    return Some(Err(err));
                }
            }
        }
    }
}
