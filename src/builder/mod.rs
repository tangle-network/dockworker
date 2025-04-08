use bollard::Docker;
use std::ops::Deref;

pub mod compose;
pub mod docker_file;
pub mod management;

pub struct DockerBuilder {
    client: Docker,
}

impl DockerBuilder {
    /// Create a new `DockerBuilder`
    ///
    /// # Errors
    ///
    /// This will attempt to connect to and ping the docker server. If either fails, this will return
    /// an error.
    pub async fn new() -> Result<Self, bollard::errors::Error> {
        let client = Docker::connect_with_local_defaults()?;
        if let Err(e) = client.ping().await {
            log::error!("Failed to ping docker server: {}", e);
            return Err(e);
        }

        Ok(Self { client })
    }

    #[must_use]
    pub fn client(&self) -> &Docker {
        &self.client
    }
}

impl Deref for DockerBuilder {
    type Target = Docker;

    fn deref(&self) -> &Self::Target {
        &self.client
    }
}
