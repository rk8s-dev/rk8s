use crate::common::*;

pub fn try_unfold<T, E, St, F>(init: St, f: F) -> TryUnfold<T, E, St, F>
where
    F: FnMut(&mut St) -> Result<Option<T>, E>,
{
    TryUnfold {
        state: Some((init, f)),
        _phantom: PhantomData,
    }
}

pub struct TryUnfold<T, E, St, F>
where
    F: FnMut(&mut St) -> Result<Option<T>, E>,
{
    state: Option<(St, F)>,
    _phantom: PhantomData<(T, E)>,
}

impl<T, E, St, F> Iterator for TryUnfold<T, E, St, F>
where
    F: FnMut(&mut St) -> Result<Option<T>, E>,
{
    type Item = Result<T, E>;

    fn next(&mut self) -> Option<Self::Item> {
        let (mut state, mut f) = self.state.take()?;
        let result = f(&mut state).transpose()?;

        if result.is_ok() {
            self.state = Some((state, f));
        }

        Some(result)
    }
}
