//! Node output
//!
//! [`Output`] represents the output of the Node respectively.
//!
//! Users should consider the output results of the Node when defining the specific
//! behavior of the Node. The input results may be: normal output, no output, or Node
//! execution error message.
//! It should be noted that the content stored in [`Output`] must implement the [`Clone`] trait.
//!
//! # Example
//! In general, a Node may produce output or no output:
//! ```rust
//! use dagrs::Output;
//! let out=Output::new(10);
//! let non_out=Output::empty();
//! ```
//! In some special cases, when a predictable error occurs in the execution of a Node's
//! specific behavior, the user can choose to return the error message as the output of
//! the Node. Of course, this will cause subsequent Nodes to abandon execution.
//!
//! ```rust
//! use dagrs::Output;
//! use dagrs::Content;
//! let err_out = Output::Err("some error messages!".to_string());

use crate::connection::information_packet::Content;

/// [`Output`] represents the output of a node. Different from information packet (`Content`,
/// used to communicate with other Nodes), `Output` carries the information that `Node`
/// needs to pass to the `Graph`.
#[derive(Clone, Debug)]
pub enum Output {
    Out(Option<Content>),
    Err(String),
    ErrWithExitCode(Option<i32>, Option<Content>),
    /// ...
    ConditionResult(bool),
}

impl Output {
    /// Construct a new [`Output`].
    ///
    /// Since the return value may be transferred between threads,
    /// [`Send`], [`Sync`] is needed.
    pub fn new<H: Send + Sync + 'static>(val: H) -> Self {
        Self::Out(Some(Content::new(val)))
    }

    /// Construct an empty [`Output`].
    pub fn empty() -> Self {
        Self::Out(None)
    }

    /// Construct an [`Output`]` with an error message.
    pub fn error(msg: String) -> Self {
        Self::Err(msg)
    }

    /// Construct an [`Output`]` with an exit code and an optional error message.
    pub fn error_with_exit_code(code: Option<i32>, msg: Option<Content>) -> Self {
        Self::ErrWithExitCode(code, msg)
    }

    /// Determine whether [`Output`] stores error information.
    pub(crate) fn is_err(&self) -> bool {
        match self {
            Self::Err(_) | Self::ErrWithExitCode(_, _) => true,
            Self::Out(_) | Self::ConditionResult(_) => false,
        }
    }

    /// Get the contents of [`Output`].
    pub fn get_out(&self) -> Option<Content> {
        match self {
            Self::Out(ref out) => out.clone(),
            Self::Err(_) | Self::ErrWithExitCode(_, _) | Self::ConditionResult(_) => None,
        }
    }

    /// Get error information stored in [`Output`].
    pub fn get_err(&self) -> Option<String> {
        match self {
            Self::Out(_) | Self::ConditionResult(_) => None,
            Self::Err(err) => Some(err.to_string()),
            Self::ErrWithExitCode(code, _) => {
                let error_code = code.map_or("".to_string(), |v| v.to_string());
                Some(format!("code: {error_code}"))
            }
        }
    }

    /// Get the condition result stored in [`Output`].
    ///
    /// Returns `Some(bool)` if this is a `ConditionResult` variant,
    /// otherwise returns `None`.
    pub(crate) fn conditional_result(&self) -> Option<bool> {
        match self {
            Self::ConditionResult(b) => Some(*b),
            _ => None,
        }
    }
}
