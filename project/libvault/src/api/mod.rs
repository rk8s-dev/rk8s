//! The `rusty_vault::api` module which contains code useful for interacting with a RustyVault server.

use serde_json::Value;

pub mod auth;
pub mod auth_token;
pub mod client;
pub mod logical;
pub mod secret;
pub mod sys;

pub use client::Client;

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
}
