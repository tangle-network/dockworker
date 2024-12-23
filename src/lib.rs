mod config;
mod error;
mod parser;

#[cfg(feature = "deploy")]
mod builder;

// Always export parsing-related items
pub use config::{
    compose::{BuildConfig, ComposeConfig, ServiceConfig},
    docker_file::{DockerCommand, DockerfileConfig},
};
pub use error::DockerError;
pub use parser::{compose::ComposeParser, docker_file::DockerfileParser};

// Only export deployment-related items when "deploy" feature is enabled
#[cfg(feature = "deploy")]
pub use builder::DockerBuilder;

#[cfg(test)]
pub mod tests;
