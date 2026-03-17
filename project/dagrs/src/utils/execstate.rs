use std::sync::{
    Arc, Mutex,
    atomic::{AtomicBool, Ordering},
};

use super::output::Output;
use crate::connection::information_packet::Content;

#[derive(Debug)]
pub(crate) struct ExecState {
    /// The execution succeed or not.
    success: AtomicBool,
    /// Output produced by a task.
    output: Arc<Mutex<Output>>,
}

impl ExecState {
    /// Construct a new [`ExeState`].
    pub(crate) fn new() -> Self {
        // initialize the task to failure without output.
        Self {
            success: AtomicBool::new(false),
            output: Arc::new(Mutex::new(Output::empty())),
            //semaphore: Semaphore::new(0),
        }
    }

    /// After the task is successfully executed, set the execution result.
    pub(crate) fn set_output(&self, output: Output) {
        self.success.store(true, Ordering::Relaxed);
        *self.output.lock().unwrap() = output;
    }

    /// [`Output`] for fetching internal storage.
    /// This function is generally not called directly, but first uses the semaphore for synchronization control.
    pub(crate) fn get_output(&self) -> Option<Content> {
        self.output.lock().unwrap().get_out()
    }
    pub(crate) fn get_full_output(&self) -> Output {
        self.output.lock().unwrap().clone()
    }

    pub(crate) fn exe_success(&self) {
        self.success.store(true, Ordering::Relaxed)
    }

    pub(crate) fn exe_fail(&self) {
        self.success.store(false, Ordering::Relaxed)
    }
}
