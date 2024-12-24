use thiserror::Error;

#[derive(Debug, Error)]
pub enum DockerError {
    #[error("Failed to read file: {0}")]
    FileError(#[from] std::io::Error),
    #[error("Failed to parse yaml: {0}")]
    YamlError(#[from] serde_yaml::Error),
    #[error("Failed to parse dockerfile: {0}")]
    DockerfileError(String),
    #[cfg(feature = "docker")]
    #[error("Docker API error: {0}")]
    BollardError(#[from] bollard::errors::Error),
    #[cfg(feature = "docker")]
    #[error("Invalid IPAM configuration")]
    InvalidIpamConfig,
    #[cfg(feature = "docker")]
    #[error("Container {0} is not running")]
    ContainerNotRunning(String),
    #[cfg(feature = "docker")]
    #[error("Network creation failed: {0}")]
    NetworkCreationError(String),
    #[error("Invalid resource limit: {0}")]
    InvalidResourceLimit(String),
    #[error("Validation error: {0}")]
    ValidationError(String),
}
