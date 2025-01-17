#![allow(unknown_lints)]

pub use config::{
    compose::{BuildConfig, ComposeConfig, Service},
    volume::Volume,
};
pub use error::DockerError;

pub mod config;
pub mod error;
pub mod parser;

#[doc(hidden)]
pub mod tests;

#[cfg(feature = "docker")]
pub mod builder;
#[cfg(feature = "docker")]
pub use builder::DockerBuilder;
#[cfg(feature = "docker")]
pub mod container;
