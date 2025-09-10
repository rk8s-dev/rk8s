// Copyright (c) 2024 Xu Shaohua <shaohua@biofan.org>. All rights reserved.
// Use of this source is governed by Apache-2.0 License that can be found
// in the LICENSE file.

#[cfg(target_os = "linux")]
mod imp {
    use std::sync::atomic::{AtomicU32, Ordering};
    use std::thread;
    use std::time::Duration;

    const NOTIFY_WAIT: u32 = 0;
    const NOTIFY_WAKE: u32 = 1;

    fn wake_one(count: &AtomicU32) {
        let ret = unsafe { nc::futex(count, nc::FUTEX_WAKE, NOTIFY_WAKE, None, None, 0) };
        assert!(ret.is_ok());
    }

    fn wait(count: &AtomicU32, expected: u32) {
        let ret = unsafe { nc::futex(count, nc::FUTEX_WAIT, expected, None, None, 0) };
        assert!(ret.is_ok());
    }

    pub(super) fn run_main() {
        let notify = AtomicU32::new(0);

        thread::scope(|s| {
            // Create the notify thread.
            s.spawn(|| {
                // Wake up some other threads after one second.
                println!("[notify] Sleep for 1s");
                thread::sleep(Duration::from_secs(1));
                println!("[notify] Wake up main thread");
                notify.store(NOTIFY_WAKE, Ordering::Relaxed);
                wake_one(&notify);
            });

            // Main thread will wait until the notify thread wakes it up.
            println!("[main] Waiting for notification..");
            while notify.load(Ordering::Relaxed) == NOTIFY_WAIT {
                wait(&notify, NOTIFY_WAIT);
            }
            println!("[main] Got wake up");
        });
    }
}

#[cfg(not(target_os = "linux"))]
mod imp {
    pub(super) fn run_main() {
        // do nothing
    }
}

fn main() {
    imp::run_main();
}
