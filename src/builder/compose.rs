use bollard::container::{Config, CreateContainerOptions, StartContainerOptions};
use bollard::image::BuildImageOptions;
use bollard::network::CreateNetworkOptions;
use bollard::secret::{HealthConfig, HostConfig, PortBinding};
use futures_util::StreamExt;
use std::collections::HashMap;
use std::path::Path;
use tokio::fs;

use crate::ServiceConfig;
use crate::config::compose::HealthCheck;
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

    fn create_host_config(
        &self,
        service: &ServiceConfig,
        network_name: &str,
    ) -> Result<HostConfig, DockerError> {
        let mut host_config = HostConfig {
            network_mode: Some(network_name.to_string()),
            binds: service.volumes.clone(),
            ..Default::default()
        };

        // Add resource limits if specified
        if let Some(resources) = &service.resources {
            host_config.memory = resources
                .memory_limit
                .as_ref()
                .and_then(|m| parse_memory_string(m).ok());
            host_config.memory_swap = resources
                .memory_swap
                .as_ref()
                .and_then(|m| parse_memory_string(m).ok());
            host_config.memory_reservation = resources
                .memory_reservation
                .as_ref()
                .and_then(|m| parse_memory_string(m).ok());
            host_config.cpu_shares = resources.cpus_shares;
            host_config.cpuset_cpus = resources.cpuset_cpus.clone();
            host_config.nano_cpus = resources.cpu_limit.map(|c| (c * 1e9) as i64);
        }

        Ok(host_config)
    }

    fn create_health_config(health: &HealthCheck) -> HealthConfig {
        HealthConfig {
            test: Some(health.test.clone()),
            interval: health.interval.clone(),
            timeout: health.timeout.clone(),
            start_period: health.start_period.clone(),
            retries: health.retries,
            ..Default::default()
        }
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

        let host_config = self.create_host_config(service_config, network_name)?;
        let health_config = service_config
            .healthcheck
            .as_ref()
            .map(Self::create_health_config);

        let container_config = Config {
            image: Some(image),
            env: service_config
                .environment
                .as_ref()
                .map(|env| env.iter().map(|(k, v)| format!("{}={}", k, v)).collect()),
            host_config: Some(host_config),
            healthcheck: health_config,
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

// Helper function to parse memory strings like "1G", "512M" into bytes
pub fn parse_memory_string(memory: &str) -> Result<i64, DockerError> {
    let len = memory.len();
    let (num, unit) = memory.split_at(len - 1);
    let base = num.parse::<i64>().map_err(|_| {
        DockerError::InvalidResourceLimit(format!("Invalid memory value: {}", memory))
    })?;

    match unit.to_uppercase().as_str() {
        "K" => Ok(base * 1024),
        "M" => Ok(base * 1024 * 1024),
        "G" => Ok(base * 1024 * 1024 * 1024),
        _ => Err(DockerError::InvalidResourceLimit(format!(
            "Invalid memory unit: {}",
            unit
        ))),
    }
}
