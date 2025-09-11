use crate::lock::MutexExt;
use std::{
    future::Future,
    pin::Pin,
    sync::{Arc, Mutex},
    task::ready,
    time::{Duration, Instant},
};
use tower::{load::Load, Service};

/// FailBackOff is the `Load` implementation for backing off when meet errors.
/// The general idea is to increase the "load" when a service returns a `Err` variant of some requests.
/// So normal nodes would have lower load hence easier to be chosen.
/// The load would be reset after a duration (i.e. after backing off).
pub struct BackOffWhenFail<S> {
    inner: S,
    handle: BackOffHandle,
}

impl<S> BackOffWhenFail<S> {
    pub fn new(inner: S, back_off: BackOffStatus) -> Self {
        Self {
            inner,
            handle: BackOffHandle {
                inner: Arc::new(Mutex::new(back_off)),
            },
        }
    }
}

#[derive(Clone)]
struct BackOffHandle {
    inner: Arc<Mutex<BackOffStatus>>,
}

#[derive(Clone)]
pub struct BackOffStatus {
    initial_backoff_dur: Duration,
    max_backoff_dur: Duration,

    pub(super) next_backoff_dur: Duration,
    pub(super) last_failure: Option<Instant>,
    pub(super) current_backoff_dur: Duration,
}

impl BackOffStatus {
    pub const fn new(initial: Duration, max: Duration) -> Self {
        Self {
            initial_backoff_dur: initial,
            max_backoff_dur: max,

            next_backoff_dur: initial,
            current_backoff_dur: initial,
            last_failure: None,
        }
    }
}

impl BackOffStatus {
    fn fail(&mut self) {
        // don't double the back off duration when already failed.
        // because when a service temporary totally unusable, there might be a flood of failure,
        // which may make the back off duration too long, even longer than the time it may take to recover.
        if !self.failed() {
            self.current_backoff_dur = self.next_backoff_dur;
            self.next_backoff_dur = Ord::min(self.next_backoff_dur * 2, self.max_backoff_dur);
        }
        self.last_failure = Some(Instant::now());
    }

    fn success(&mut self) {
        self.last_failure = None;
        self.next_backoff_dur = self.initial_backoff_dur;
        self.current_backoff_dur = self.initial_backoff_dur;
    }

    fn failed(&mut self) -> bool {
        if let Some(lf) = self.last_failure {
            if Instant::now().saturating_duration_since(lf) > self.current_backoff_dur {
                self.last_failure = None;
            }
        }
        self.last_failure.is_some()
    }
}

impl BackOffHandle {
    fn fail(&self) {
        let mut status = self.inner.lock_unpoisoned();
        status.fail()
    }

    fn failed(&self) -> bool {
        let mut status = self.inner.lock_unpoisoned();
        status.failed()
    }

    fn success(&self) {
        let mut status = self.inner.lock_unpoisoned();
        status.success()
    }
}

pub struct TraceFailFuture<F> {
    inner: F,
    handle: BackOffHandle,
}

impl<F, T, E> Future for TraceFailFuture<F>
where
    F: Future<Output = std::result::Result<T, E>>,
{
    type Output = <F as Future>::Output;

    fn poll(
        self: Pin<&mut Self>,
        cx: &mut std::task::Context<'_>,
    ) -> std::task::Poll<Self::Output> {
        // Safety: trivial projection.
        let (inner, handle) = unsafe {
            let this = self.get_unchecked_mut();
            (Pin::new_unchecked(&mut this.inner), &this.handle)
        };

        let result = ready!(Future::poll(inner, cx));
        match &result {
            Ok(_) => handle.success(),
            Err(_) => handle.fail(),
        }
        result.into()
    }
}

impl<Req, S> Service<Req> for BackOffWhenFail<S>
where
    S: Service<Req>,
{
    type Response = <S as Service<Req>>::Response;
    type Error = <S as Service<Req>>::Error;
    type Future = TraceFailFuture<<S as Service<Req>>::Future>;

    fn poll_ready(
        &mut self,
        cx: &mut std::task::Context<'_>,
    ) -> std::task::Poll<std::result::Result<(), Self::Error>> {
        self.inner.poll_ready(cx)
    }

    fn call(&mut self, req: Req) -> Self::Future {
        TraceFailFuture {
            inner: self.inner.call(req),
            handle: self.handle.clone(),
        }
    }
}

impl<S> Load for BackOffWhenFail<S> {
    type Metric = bool;

    fn load(&self) -> Self::Metric {
        self.handle.failed()
    }
}

#[cfg(test)]
mod test {
    use std::time::Duration;

    use super::BackOffStatus;

    #[test]
    fn test_back_off() {
        let mut state = BackOffStatus::new(Duration::from_secs(1), Duration::from_secs(4));
        let assert_state = |state: &mut BackOffStatus, failed, curr, next, desc| {
            assert_eq!(state.failed(), failed, "{desc}: success status not match");
            assert_eq!(
                state.current_backoff_dur,
                Duration::from_secs(curr),
                "{desc}: current_backoff_dur not match"
            );
            assert_eq!(
                state.next_backoff_dur,
                Duration::from_secs(next),
                "{desc}: next_backoff_dur not match"
            );
        };
        assert!(!state.failed());
        state.fail();
        assert_state(&mut state, true, 1, 2, "first failure");
        state.fail();
        assert_state(&mut state, true, 1, 2, "failed when fail");
        state.last_failure = None;
        state.fail();
        assert_state(&mut state, true, 2, 4, "failed after backoff");
        state.last_failure = None;
        state.fail();
        assert_state(&mut state, true, 4, 4, "failed and exceed backoff max");

        state.success();
        assert_state(&mut state, false, 1, 1, "success");
        state.fail();
        assert_state(&mut state, true, 1, 2, "failure again");
        state.success();
        assert_state(&mut state, false, 1, 1, "success after failure");
    }
}
