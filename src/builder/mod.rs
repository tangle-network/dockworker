use std::ops::Deref;
use bollard::Docker;

pub mod compose;
pub mod docker_file;
pub mod management;

pub struct DockerBuilder {
    client: Docker,
}

impl DockerBuilder {
    pub async fn new() -> Result<Self, bollard::errors::Error> {
        let client = Docker::connect_with_local_defaults()?;
        if let Err(e) = client.ping().await {
            log::error!("Failed to ping docker server: {}", e);
            return Err(e);
        }

        Ok(Self { client })
    }

    pub fn get_client(&self) -> &Docker {
        &self.client
    }
}

impl Deref for DockerBuilder {
    type Target = Docker;

    fn deref(&self) -> &Self::Target {
        &self.client
    }
}
