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

#[cfg(feature = "deploy")]
pub mod builder;
#[cfg(feature = "deploy")]
pub use builder::DockerBuilder;
#[cfg(feature = "deploy")]
pub mod container;
#[cfg(feature = "deploy")]
pub use bollard;
