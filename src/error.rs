use thiserror::Error;

#[derive(Debug, Error)]
pub enum DockerError {
    #[error("Failed to read file: {0}")]
    FileError(#[from] std::io::Error),
    #[error("Failed to parse yaml: {0}")]
    YamlError(#[from] serde_yaml::Error),
    #[error("Failed to parse dockerfile: {0}")]
    DockerfileError(String),
    #[error("Docker API error: {0}")]
    BollardError(#[from] bollard::errors::Error),
}
