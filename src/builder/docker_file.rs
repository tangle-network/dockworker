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
    pub async fn from_dockerfile<P: AsRef<Path>>(
        &self,
        path: P,
    ) -> Result<DockerfileConfig, DockerError> {
        let content = fs::read_to_string(path).await?;
        DockerfileParser::parse(&content)
    }

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
