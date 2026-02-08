use std::sync::Arc;

use async_trait::async_trait;

use crate::{
    EnvVar, InChannels, OutChannels, Output,
    action::Action,
    connection::{in_channel::TypedInChannels, out_channel::TypedOutChannels},
};

/// A trait that provides typed channel operations
///
/// This trait allows actions to use typed input and output channels instead of raw channel types.
/// Through generic parameters `I` and `O`, specific types can be specified for input and output channels,
/// providing compile-time type checking.
///
/// # Type Parameters
///
/// * `I` - The type parameter for input channels, must satisfy `Send + Sync + 'static`
/// * `O` - The type parameter for output channels, must satisfy `Send + Sync + 'static`
/// When both `I` and `O` are set to `Content`, this trait degenerates to the base `Action` trait.
#[async_trait]
pub trait TypedAction: Send + Sync {
    /// The type parameter for input channels
    type I: Send + Sync + 'static;
    /// The type parameter for output channels
    type O: Send + Sync + 'static;

    /// Converts raw input channels to typed input channels
    ///
    /// # Arguments
    ///
    /// * `in_channels` - The raw input channels
    ///
    /// # Returns
    ///
    /// Returns a typed input channel with the type specified by the associated type `I`
    fn make_typed_in_channels(&self, in_channels: &InChannels) -> TypedInChannels<Self::I> {
        TypedInChannels(in_channels.0.clone(), Default::default())
    }

    /// Converts raw output channels to typed output channels
    ///
    /// # Arguments
    ///
    /// * `out_channels` - The raw output channels
    ///
    /// # Returns
    ///
    /// Returns a typed output channel with the type specified by the associated type `O`
    fn make_typed_out_channels(&self, out_channels: &OutChannels) -> TypedOutChannels<Self::O> {
        TypedOutChannels(out_channels.0.clone(), Default::default())
    }

    /// The method that users need to implement to define their action logic
    ///
    /// This is the main method of the trait, used to execute the specific action logic.
    /// It receives typed input and output channels, along with environment variables,
    /// and returns the execution result.
    ///
    /// # Arguments
    ///
    /// * `in_channels` - The typed input channels
    /// * `out_channels` - The typed output channels
    /// * `env` - The environment variables
    ///
    /// # Returns
    ///
    /// Returns the result of the action execution
    async fn run(
        &self,
        in_channels: TypedInChannels<Self::I>,
        out_channels: TypedOutChannels<Self::O>,
        env: Arc<EnvVar>,
    ) -> Output;
}

#[async_trait]
impl<T: TypedAction> Action for T {
    async fn run(
        &self,
        in_channels: &mut InChannels,
        out_channels: &mut OutChannels,
        env: Arc<EnvVar>,
    ) -> Output {
        let in_channels = self.make_typed_in_channels(in_channels);
        let out_channels = self.make_typed_out_channels(out_channels);
        self.run(in_channels, out_channels, env).await
    }
}
