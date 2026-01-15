use std::sync::{
    atomic::{AtomicBool, Ordering},
    Arc, Mutex,
};

use super::output::Output;
use crate::connection::information_packet::Content;

#[derive(Debug)]
pub(crate) struct ExecState {
    /// The execution succeed or not.
    success: AtomicBool,
    /// Output produced by a task.
    output: Arc<Mutex<Output>>,
    /*/// The semaphore is used to control the synchronous blocking of subsequent tasks to obtain the
    /// execution results of this task.
    /// When a task is successfully executed, the permits inside the semaphore will be increased to
    /// n (n represents the number of successor tasks of this task or can also be called the output
    /// of the node), which means that the output of the task is available, and then each successor
    /// The task will obtain a permits synchronously (the permit will not be returned), which means
    /// that the subsequent task has obtained the execution result of this task.
    semaphore: Semaphore,*/
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

    /*/// The semaphore is used to control the synchronous acquisition of task output results.
    /// Under normal circumstances, first use the semaphore to obtain a permit, and then call
    /// the `get_output` function to obtain the output. If the current task is not completed
    /// (no output is generated), the subsequent task will be blocked until the current task
    /// is completed and output is generated.
    pub(crate) fn semaphore(&self) -> &Semaphore {
        &self.semaphore
    }*/
}
