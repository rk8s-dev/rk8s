use crate::common::*;

#[derive(Debug)]
pub struct AndThen<I, F> {
    pub(super) iter: Option<I>,
    pub(super) f: F,
}

impl<I, F, T, U, E> Iterator for AndThen<I, F>
where
    I: Iterator<Item = Result<T, E>>,
    F: FnMut(T) -> Result<U, E>,
{
    type Item = Result<U, E>;

    fn next(&mut self) -> Option<Self::Item> {
        let mut iter = match self.iter.take() {
            Some(iter) => iter,
            None => return None,
        };
        let input_item = match iter.next() {
            Some(Ok(item)) => item,
            Some(Err(err)) => {
                return Some(Err(err));
            }
            None => return None,
        };

        match (self.f)(input_item) {
            Ok(output_item) => {
                self.iter = Some(iter);
                Some(Ok(output_item))
            }
            Err(err) => Some(Err(err)),
        }
    }
}
