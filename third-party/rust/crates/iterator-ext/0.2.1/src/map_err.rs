use crate::common::*;

#[derive(Debug)]
pub struct MapErr<I, F> {
    pub(super) iter: I,
    pub(super) f: F,
}

impl<I, F, T, Ein, Eout> Iterator for MapErr<I, F>
where
    I: Iterator<Item = Result<T, Ein>>,
    F: FnMut(Ein) -> Eout,
{
    type Item = Result<T, Eout>;

    fn next(&mut self) -> Option<Self::Item> {
        match self.iter.next() {
            Some(Ok(item)) => Some(Ok(item)),
            Some(Err(err)) => Some(Err((self.f)(err))),
            None => None,
        }
    }
}
