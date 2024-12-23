use bollard::Docker;

use crate::error::DockerError;

mod compose;
mod dockerfile;

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
