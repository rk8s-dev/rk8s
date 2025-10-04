//! The `libvault::api` module which contains code useful for interacting with a RustyVault server.

use serde::de::DeserializeOwned;
use serde_json::Value;

use crate::errors::RvError;

pub mod async_client;
pub mod auth;
pub mod auth_token;
pub mod client;
pub mod logical;
pub mod secret;
pub mod sys;
pub mod types;

pub use async_client::AsyncClient;
pub use client::Client;
pub use types::{ApiResponse, AuthResponse, GenericResponse};

#[derive(Debug, Clone, Default)]
pub struct HttpResponse {
    pub method: String,
    pub url: String,
    pub response_status: u16,
    pub response_data: Option<Value>,
}

impl HttpResponse {
    pub fn print_debug_info(&self) {
        println!("URL: {} {}", self.method, self.url);
        print!("Code: {}.", self.response_status);
        if self.response_status != 200 || self.response_status != 204 {
            println!(" Error:");
        }

        if let Some(response_data) = &self.response_data {
            println!("{response_data:?}");
        }
    }

    /// Attempts to deserialize the response payload into the provided type.
    pub fn parse<T>(&self) -> Result<ApiResponse<T>, RvError>
    where
        T: DeserializeOwned,
    {
        match &self.response_data {
            Some(value) => Ok(serde_json::from_value(value.clone())?),
            None => Err(RvError::ErrResponseDataInvalid),
        }
    }
}
