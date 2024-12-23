use crate::{config::compose::ComposeConfig, error::DockerError};

pub struct ComposeParser;

impl ComposeParser {
    pub fn parse(content: &str) -> Result<ComposeConfig, DockerError> {
        serde_yaml::from_str(content).map_err(DockerError::YamlError)
    }
}
