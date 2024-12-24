use crate::error::DockerError;
use bollard::Docker;

pub mod compose;
pub mod docker_file;
pub mod management;

pub struct DockerBuilder {
    client: Docker,
}

impl DockerBuilder {
    pub fn new() -> Result<Self, DockerError> {
        let client = Docker::connect_with_local_defaults().map_err(DockerError::BollardError)?;

        Ok(Self { client })
    }

    pub fn get_client(&self) -> &Docker {
        &self.client
    }
}
