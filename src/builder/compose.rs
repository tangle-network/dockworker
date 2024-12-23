use bollard::container::{Config, CreateContainerOptions, StartContainerOptions};
use bollard::image::BuildImageOptions;
use bollard::network::CreateNetworkOptions;
use bollard::secret::{HostConfig, PortBinding};
use futures_util::StreamExt;
use std::collections::HashMap;
use std::path::Path;
use tokio::fs;

use crate::ServiceConfig;
use crate::{config::compose::ComposeConfig, error::DockerError, parser::compose::ComposeParser};

use super::DockerBuilder;

impl DockerBuilder {
    pub async fn from_compose<P: AsRef<Path>>(
        &self,
        path: P,
    ) -> Result<ComposeConfig, DockerError> {
        let content = fs::read_to_string(path).await?;
        ComposeParser::parse(&content)
    }

    pub async fn deploy_compose(
        &self,
        config: &ComposeConfig,
    ) -> Result<HashMap<String, String>, DockerError> {
        let network_name = format!("compose_network_{}", uuid::Uuid::new_v4());

        // Create a network for the compose services
        self.client
            .create_network(CreateNetworkOptions {
                name: network_name.as_str(),
                driver: "bridge",
                ..Default::default()
            })
            .await
            .map_err(DockerError::BollardError)?;

        let mut container_ids = HashMap::new();

        // Deploy each service
        for (service_name, service_config) in &config.services {
            let container_id = self
                .deploy_service(service_name, service_config, &network_name)
                .await?;

            container_ids.insert(service_name.clone(), container_id);
        }

        Ok(container_ids)
    }

    async fn deploy_service(
        &self,
        service_name: &str,
        service_config: &ServiceConfig,
        network_name: &str,
    ) -> Result<String, DockerError> {
        let image = if let Some(build_config) = &service_config.build {
            // Build the image if build configuration is provided
            let tag = format!("compose_{}", service_name);
            let dockerfile_path = Path::new(&build_config.context)
                .join(build_config.dockerfile.as_deref().unwrap_or("Dockerfile"));

            let build_opts = BuildImageOptions {
                dockerfile: dockerfile_path.to_str().unwrap(),
                t: &tag,
                q: false,
                ..Default::default()
            };

            let mut build_stream = self.client.build_image(
                build_opts,
                None,
                Some(build_config.context.clone().into()),
            );

            while let Some(build_result) = build_stream.next().await {
                match build_result {
                    Ok(_) => continue,
                    Err(e) => return Err(DockerError::BollardError(e)),
                }
            }

            tag
        } else {
            service_config.image.clone().ok_or_else(|| {
                DockerError::DockerfileError("No image or build configuration provided".into())
            })?
        };

        // Prepare port bindings
        let mut port_bindings = HashMap::new();
        if let Some(ports) = &service_config.ports {
            for port_mapping in ports {
                let parts: Vec<&str> = port_mapping.split(':').collect();
                if parts.len() == 2 {
                    port_bindings.insert(
                        format!("{}/tcp", parts[1]),
                        Some(vec![PortBinding {
                            host_ip: Some("0.0.0.0".to_string()),
                            host_port: Some(parts[0].to_string()),
                        }]),
                    );
                }
            }
        }

        // Create container configuration
        let container_config = Config {
            image: Some(image),
            env: service_config
                .environment
                .as_ref()
                .map(|env| env.iter().map(|(k, v)| format!("{}={}", k, v)).collect()),
            host_config: Some(HostConfig {
                port_bindings: Some(port_bindings),
                network_mode: Some(network_name.to_string()),
                binds: service_config.volumes.clone(),
                ..Default::default()
            }),
            ..Default::default()
        };

        // Create and start the container
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
