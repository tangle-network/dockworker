use crate::{
    config::docker_file::DockerfileConfig, error::DockerError,
    parser::docker_file::DockerfileParser,
};
use bollard::container::{Config, CreateContainerOptions, StartContainerOptions};
use bollard::image::BuildImageOptions;
use bollard::service::HostConfig;
use futures_util::StreamExt;
use std::path::Path;
use tokio::fs;

use super::DockerBuilder;

impl DockerBuilder {
    /// Creates a new Dockerfile configuration from a file
    ///
    /// This method reads a Dockerfile and parses it into a structured configuration.
    /// It handles basic Dockerfile syntax including:
    /// - Line continuations with backslash
    /// - Comments starting with #
    /// - Basic Dockerfile commands like FROM, COPY, etc.
    ///
    /// # Arguments
    ///
    /// * `path` - Path to the Dockerfile
    ///
    /// # Returns
    ///
    /// Returns a `Result` containing the parsed `DockerfileConfig` or a `DockerError`
    ///
    /// # Examples
    ///
    /// ```rust,no_run
    /// # use std::path::Path;
    /// # use dockworker::DockerBuilder;
    /// # async fn example() -> Result<(), Box<dyn std::error::Error>> {
    /// let builder = DockerBuilder::new()?;
    /// let config = builder.from_dockerfile(Path::new("Dockerfile")).await?;
    /// # Ok(())
    /// # }
    /// ```
    pub async fn from_dockerfile<P: AsRef<Path>>(
        &self,
        path: P,
    ) -> Result<DockerfileConfig, DockerError> {
        let content = fs::read_to_string(path).await?;
        DockerfileParser::parse(&content)
    }

    /// Deploys a Dockerfile configuration with optional settings
    ///
    /// This method builds a Docker image from a Dockerfile configuration and creates a container from it.
    /// It handles:
    /// - Creating a temporary build context
    /// - Building the Docker image
    /// - Creating and starting a container with the specified options
    ///
    /// # Arguments
    ///
    /// * `config` - The Dockerfile configuration to deploy
    /// * `tag` - Tag to apply to the built image
    /// * `command` - Optional command to override the default container command
    /// * `volumes` - Optional volume mounts for the container
    /// * `network` - Optional network to connect the container to
    /// * `env` - Optional environment variables for the container
    ///
    /// # Returns
    ///
    /// Returns a `Result` containing the ID of the created container, or a `DockerError` if deployment fails
    ///
    /// # Examples
    ///
    /// ```rust,no_run
    /// # use dockworker::{DockerBuilder, config::docker_file::{DockerCommand, DockerfileConfig}};
    /// # async fn example() -> Result<(), Box<dyn std::error::Error>> {
    /// let builder = DockerBuilder::new()?;
    /// let config = DockerfileConfig { // Your Dockerfile config
    ///     base_image: "ubuntu:latest".to_string(),
    ///     commands: vec![
    ///         DockerCommand::Run { command: "apt-get update".to_string() },
    ///         DockerCommand::Copy { source: "app".to_string(), dest: "/app".to_string(), chown: None },
    ///     ]
    /// };
    /// let container_id = builder.deploy_dockerfile(
    ///     &config,
    ///     "my-image:latest",
    ///     Some(vec!["echo".to_string(), "hello".to_string()]),
    ///     None,
    ///     None,
    ///     None
    /// ).await?;
    /// # Ok(())
    /// # }
    /// ```
    pub async fn deploy_dockerfile(
        &self,
        config: &DockerfileConfig,
        tag: &str,
        command: Option<Vec<String>>,
        volumes: Option<Vec<String>>,
        network: Option<String>,
        env: Option<Vec<String>>,
    ) -> Result<String, DockerError> {
        // Create a temporary directory for the build context
        let temp_dir = tempfile::tempdir().map_err(|e| DockerError::FileError(e.into()))?;
        let dockerfile_path = temp_dir.path().join("Dockerfile");

        // Write the Dockerfile content from our config
        tokio::fs::write(&dockerfile_path, config.to_dockerfile_content()).await?;

        // Create tar archive with the Dockerfile
        let tar_path = temp_dir.path().join("context.tar");
        let tar_file = std::fs::File::create(&tar_path)?;
        let mut tar_builder = tar::Builder::new(tar_file);
        tar_builder.append_path_with_name(&dockerfile_path, "Dockerfile")?;
        tar_builder.finish()?;

        // Read the tar file
        let context = tokio::fs::read(&tar_path).await?;

        // Build the image
        let build_opts = BuildImageOptions {
            dockerfile: "Dockerfile",
            t: tag,
            q: false,
            ..Default::default()
        };

        let mut build_stream = self
            .client
            .build_image(build_opts, None, Some(context.into()));

        while let Some(build_result) = build_stream.next().await {
            match build_result {
                Ok(_) => continue,
                Err(e) => return Err(DockerError::BollardError(e)),
            }
        }

        // Create and start container from our image
        let container_config = Config {
            image: Some(tag.to_string()),
            cmd: command.map(|v| v.iter().map(|s| s.to_string()).collect()),
            env: env.map(|v| v.iter().map(|s| s.to_string()).collect()),
            host_config: Some(HostConfig {
                binds: volumes.map(|v| v.iter().map(|s| s.to_string()).collect()),
                network_mode: network,
                ..Default::default()
            }),
            ..Default::default()
        };

        let container_info = self
            .client
            .create_container(None::<CreateContainerOptions<String>>, container_config)
            .await
            .map_err(DockerError::BollardError)?;

        self.client
            .start_container(&container_info.id, None::<StartContainerOptions<String>>)
            .await
            .map_err(DockerError::BollardError)?;

        Ok(container_info.id)
    }
}
