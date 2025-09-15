//! A general version async Notify, like `tokio` Notify but can work with any async runtime.

use std::future::Future;
use std::ops::Deref;
use std::pin::{pin, Pin};
use std::sync::atomic::{AtomicBool, Ordering};
use std::task::{ready, Context, Poll};

use event_listener::{Event, EventListener};
use futures_core::Stream;
use pin_project_lite::pin_project;

/// Notify a single task to wake up.
///
/// `Notify` provides a basic mechanism to notify a single task of an event.
/// `Notify` itself does not carry any data. Instead, it is to be used to signal
/// another task to perform an operation.
///
/// If [`notify()`] is called **before** [`notified().await`], then the next call to
/// [`notified().await`] will complete immediately, consuming the permit. Any
/// subsequent calls to [`notified().await`] will wait for a new permit.
///
/// If [`notify()`] is called **multiple** times before [`notified().await`], only a
/// **single** permit is stored. The next call to [`notified().await`] will
/// complete immediately, but the one after will wait for a new permit.
///
/// [`notify()`]: Notify::notify
/// [`notified().await`]: Notify::notified()
///
/// # Examples
///
/// Basic usage.
///
/// ```
/// use std::sync::Arc;
/// use async_notify::Notify;
///
/// async_global_executor::block_on(async {
///    let notify = Arc::new(Notify::new());
///    let notify2 = notify.clone();
///
///    async_global_executor::spawn(async move {
///        notify2.notify();
///        println!("sent notification");
///    })
///    .detach();
///
///    println!("received notification");
///    notify.notified().await;
/// })
/// ```
#[derive(Debug)]
pub struct Notify {
    count: AtomicBool,
    event: Event,
}

/// Like tokio Notify, this is a runtime independent Notify.
impl Notify {
    pub fn new() -> Self {
        Self {
            count: Default::default(),
            event: Default::default(),
        }
    }

    /// Notifies a waiting task
    ///
    /// If a task is currently waiting, that task is notified. Otherwise, a
    /// permit is stored in this `Notify` value and the **next** call to
    /// [`notified().await`] will complete immediately consuming the permit made
    /// available by this call to `notify()`.
    ///
    /// At most one permit may be stored by `Notify`. Many sequential calls to
    /// `notify` will result in a single permit being stored. The next call to
    /// `notified().await` will complete immediately, but the one after that
    /// will wait.
    ///
    /// [`notified().await`]: Notify::notified()
    ///
    /// # Examples
    ///
    /// ```
    /// use std::sync::Arc;
    /// use async_notify::Notify;
    ///
    /// async_global_executor::block_on(async {
    ///    let notify = Arc::new(Notify::new());
    ///    let notify2 = notify.clone();
    ///
    ///    async_global_executor::spawn(async move {
    ///        notify2.notify();
    ///        println!("sent notification");
    ///    })
    ///    .detach();
    ///
    ///    println!("received notification");
    ///    notify.notified().await;
    /// })
    /// ```
    #[inline]
    pub fn notify(&self) {
        self.count.store(true, Ordering::Release);
        self.event.notify(1);
    }

    /// Wait for a notification.
    ///
    /// Each `Notify` value holds a single permit. If a permit is available from
    /// an earlier call to [`notify()`], then `notified().await` will complete
    /// immediately, consuming that permit. Otherwise, `notified().await` waits
    /// for a permit to be made available by the next call to `notify()`.
    ///
    /// This method is cancel safety.
    ///
    /// [`notify()`]: Notify::notify
    #[inline]
    pub async fn notified(&self) {
        loop {
            if self.fast_path() {
                return;
            }

            let listener = EventListener::new();
            let mut listener = pin!(listener);
            listener.as_mut().listen(&self.event);

            if self.fast_path() {
                return;
            }

            listener.await;
        }
    }

    fn fast_path(&self) -> bool {
        self.count
            .compare_exchange(true, false, Ordering::AcqRel, Ordering::Acquire)
            .is_ok()
    }
}

impl Default for Notify {
    fn default() -> Notify {
        Notify::new()
    }
}

pin_project! {
    /// A [`Stream`](Stream) [`Notify`] wrapper
    pub struct NotifyStream<T: Deref<Target=Notify>> {
        #[pin]
        notify: T,
        listener: Option<Pin<Box<EventListener>>>,
    }
}

impl<T: Deref<Target = Notify>> NotifyStream<T> {
    /// Create [`NotifyStream`] from `T`
    pub fn new(notify: T) -> Self {
        Self {
            notify,
            listener: None,
        }
    }

    /// acquire the inner [`T`]
    pub fn into_inner(self) -> T {
        self.notify
    }
}

impl<T: Deref<Target = Notify>> AsRef<Notify> for NotifyStream<T> {
    fn as_ref(&self) -> &Notify {
        self.notify.deref()
    }
}

impl<T: Deref<Target = Notify>> Stream for NotifyStream<T> {
    type Item = ();

    fn poll_next(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Option<Self::Item>> {
        let this = self.project();
        let notify = this.notify.deref();

        loop {
            if notify.fast_path() {
                *this.listener = None;

                return Poll::Ready(Some(()));
            }

            match this.listener.as_mut() {
                None => {
                    let listener = notify.event.listen();
                    *this.listener = Some(listener);
                }
                Some(listener) => {
                    ready!(listener.as_mut().poll(cx));
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use std::sync::Arc;

    use futures_util::{select, FutureExt, StreamExt};

    use super::*;

    #[test]
    fn test() {
        async_global_executor::block_on(async {
            let notify = Arc::new(Notify::new());
            let notify2 = notify.clone();

            async_global_executor::spawn(async move {
                notify2.notify();
                println!("sent notification");
            })
            .detach();

            println!("received notification");
            notify.notified().await;
        })
    }

    #[test]
    fn test_multi_notify() {
        async_global_executor::block_on(async {
            let notify = Arc::new(Notify::new());
            let notify2 = notify.clone();

            notify.notify();
            notify.notify();

            select! {
                _ = notify2.notified().fuse() => {}
                default => unreachable!("there should be notified")
            }

            select! {
                _ = notify2.notified().fuse() => unreachable!("there should not be notified"),
                default => {}
            }

            notify.notify();

            select! {
                _ = notify2.notified().fuse() => {}
                default => unreachable!("there should be notified")
            }
        })
    }

    #[test]
    fn stream() {
        async_global_executor::block_on(async {
            let notify = Arc::new(Notify::new());
            let mut notify_stream = NotifyStream::new(notify.clone());

            async_global_executor::spawn(async move {
                notify.notify();
                println!("sent notification");
            })
            .detach();

            notify_stream.next().await.unwrap();
        })
    }
}
