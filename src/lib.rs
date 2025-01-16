pub use config::{
    compose::{BuildConfig, ComposeConfig, Service},
    volume::VolumeType,
};
pub use error::DockerError;

pub mod config;
pub mod error;
pub mod parser;

#[cfg(test)]
pub mod tests;

#[cfg(feature = "docker")]
pub mod builder;
#[cfg(feature = "docker")]
pub use builder::DockerBuilder;
#[cfg(feature = "docker")]
pub mod container;