use bollard::{API_DEFAULT_VERSION, Docker};
use std::ops::Deref;
use std::sync::Arc;

pub mod compose;
pub mod docker_file;
pub mod management;

pub struct DockerBuilder {
    client: Arc<Docker>,
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

        Ok(Self {
            client: Arc::new(client),
        })
    }

    pub async fn with_address(addr: &str) -> Result<Self, bollard::errors::Error> {
        let client = Docker::connect_with_local(addr, 20, API_DEFAULT_VERSION)?;
        if let Err(e) = client.ping().await {
            log::error!("Failed to ping docker server: {}", e);
            return Err(e);
        }

        Ok(Self {
            client: Arc::new(client),
        })
    }

    #[must_use]
    pub fn client(&self) -> Arc<Docker> {
        self.client.clone()
    }
}

impl Deref for DockerBuilder {
    type Target = Docker;

    fn deref(&self) -> &Self::Target {
        &self.client
    }
}
