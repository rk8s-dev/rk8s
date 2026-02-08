use std::sync::Arc;

use async_trait::async_trait;

use crate::{
    connection::{in_channel::InChannels, out_channel::OutChannels},
    utils::{env::EnvVar, output::Output},
};

/// Node specific behavior
///
/// [`Action`] stores the specific execution logic of a task.
///
/// # Example
/// An implementation of [`Action`]: `HelloAction`, having private
/// fields `statement` and `repeat`.
///
/// ```rust
/// use std::sync::Arc;
/// use dagrs::{Action, EnvVar, Output, InChannels, OutChannels};
/// use async_trait::async_trait;
///
/// struct HelloAction{
///    statement: String,
///    repeat: usize,
/// }
///
/// #[async_trait]
/// impl Action for HelloAction{
///     async fn run(&self, _: &mut InChannels, _: &mut OutChannels, _: Arc<EnvVar>) -> Output{
///         for i in 0..self.repeat {
///             println!("{}",self.statement);
///         }
///         Output::empty()
///     }
/// }
///
/// let hello=HelloAction {
///     statement: "hello world!".to_string(),
///     repeat: 10
/// };
///
/// ```
#[async_trait]
pub trait Action: Send + Sync {
    async fn run(
        &self,
        in_channels: &mut InChannels,
        out_channels: &mut OutChannels,
        env: Arc<EnvVar>,
    ) -> Output;
}

/// An empty implementaion of [`Action`].
///
/// Used as a placeholder when creating a `Node` without `Action`.
pub struct EmptyAction;
#[async_trait]
impl Action for EmptyAction {
    async fn run(&self, _: &mut InChannels, _: &mut OutChannels, _: Arc<EnvVar>) -> Output {
        Output::Out(None)
    }
}
