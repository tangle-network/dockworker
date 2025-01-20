#![allow(unknown_lints)]

pub use config::{
    compose::{BuildConfig, ComposeConfig, Service},
    volume::Volume,
};
pub use error::DockerError;

pub mod config;
pub mod error;
pub mod parser;

#[cfg(test)]
mod test_fixtures;

#[cfg(feature = "docker")]
pub mod builder;
#[cfg(feature = "docker")]
pub use builder::DockerBuilder;
#[cfg(feature = "docker")]
pub mod container;
#[cfg(feature = "docker")]
pub use bollard;
