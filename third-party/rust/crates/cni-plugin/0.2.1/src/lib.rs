#![warn(missing_docs)]

//! Library to write CNI plugins.
//!
//! - CNI information: on the [cni.dev](https://cni.dev) website.
//! - [Tooling overview][tools]
//! - [Tutorial][tuto]
//!
//! [tools]: https://github.com/passcod/cni-plugins/blob/main/docs/Standard-Tooling.md
//! [tuto]: https://github.com/passcod/cni-plugins/blob/main/docs/Plugin-Hello-World.md
//!
//! # Quick start
//!
//! ```no_run
//! use cni_plugin::{Cni, logger};
//! logger::install(env!("CARGO_PKG_NAME"));
//! match Cni::load() {
//!     Cni::Add { container_id, ifname, netns, path, config } => {}
//!     Cni::Del { container_id, ifname, netns, path, config } => {}
//!     Cni::Check { container_id, ifname, netns, path, config } => {}
//!     Cni::Version(_) => unreachable!()
//! }
//! ```
//!
//! or:
//!
//! ```no_run
//! use cni_plugin::{Cni, Inputs, logger};
//! logger::install(env!("CARGO_PKG_NAME"));
//!
//! let Inputs {
//!     command, container_id, ifname, netns, path, config
//! } = Cni::load().into_inputs().unwrap();
//! ```

pub use cni::Cni;
pub use command::Command;
pub use inputs::Inputs;

pub mod config;
#[cfg(any(feature = "with-smol", feature = "with-tokio"))]
pub mod delegation;
pub mod error;
pub mod ip_range;
pub mod logger;
pub mod macaddr;
pub mod reply;

mod cni;
mod command;
mod dns;
mod inputs;
mod path;
mod version;
