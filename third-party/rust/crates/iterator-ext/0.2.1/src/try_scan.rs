use crate::common::*;

#[derive(Debug)]
pub struct TryScan<I, St, F> {
    pub(super) iter: Option<I>,
    pub(super) state: St,
    pub(super) f: F,
}

impl<I, St, F, T, U, E> Iterator for TryScan<I, St, F>
where
    I: Iterator<Item = Result<T, E>>,
    F: FnMut(&mut St, T) -> Result<Option<U>, E>,
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
            None => {
                return None;
            }
        };

        let output_item = match (self.f)(&mut self.state, input_item) {
            Ok(Some(item)) => item,
            Ok(None) => return None,
            Err(err) => return Some(Err(err)),
        };

        self.iter = Some(iter);
        Some(Ok(output_item))
    }
}
