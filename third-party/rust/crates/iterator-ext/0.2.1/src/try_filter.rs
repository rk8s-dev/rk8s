use crate::common::*;

#[derive(Debug)]
pub struct TryFilter<I, F> {
    pub(super) iter: Option<I>,
    pub(super) f: F,
}

impl<I, T, E, F> Iterator for TryFilter<I, F>
where
    I: Iterator<Item = Result<T, E>>,
    F: FnMut(&T) -> Result<bool, E>,
{
    type Item = Result<T, E>;

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
            match (self.f)(&input_item) {
                Ok(true) => {
                    self.iter = Some(iter);
                    return Some(Ok(input_item));
                }
                Ok(false) => {}
                Err(err) => {
                    return Some(Err(err));
                }
            }
        }
    }
}
