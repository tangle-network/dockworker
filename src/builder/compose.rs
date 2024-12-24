use bollard::container::{Config, CreateContainerOptions, StartContainerOptions};
use bollard::image::BuildImageOptions;
use bollard::network::CreateNetworkOptions;
use bollard::service::{HealthConfig, HostConfig, Mount, MountTypeEnum, PortBinding};
use futures_util::StreamExt;
use std::collections::HashMap;
use std::path::{Path, PathBuf};
use tokio::fs;
use uuid::Uuid;

use crate::DockerBuilder;
use crate::config::compose::{ComposeConfig, HealthCheck, Service};
use crate::error::DockerError;
use crate::parser::compose::ComposeParser;

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
        let network_name = format!("compose_network_{}", Uuid::new_v4());

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

        // Get service deployment order
        let service_order = config.resolve_service_order()?;

        // Deploy services in order
        for service_name in service_order {
            let service = config.services.get(&service_name).unwrap();
            let container_id = self
                .deploy_service(&service_name, service, &network_name)
                .await?;
            container_ids.insert(service_name, container_id);
        }

        Ok(container_ids)
    }

    fn create_host_config(
        &self,
        service: &Service,
        network_name: &str,
    ) -> Result<HostConfig, DockerError> {
        let mut host_config = HostConfig {
            network_mode: Some(network_name.to_string()),
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

        // Configure mounts if volumes are specified
        if let Some(volumes) = &service.volumes {
            let mounts: Vec<Mount> = volumes
                .iter()
                .map(|vol| match vol {
                    crate::VolumeType::Named(name) => {
                        let parts: Vec<&str> = name.split(':').collect();
                        Mount {
                            target: Some(parts[1].to_string()),
                            source: Some(parts[0].to_string()),
                            typ: Some(MountTypeEnum::VOLUME),
                            ..Default::default()
                        }
                    }
                    crate::VolumeType::Bind {
                        source,
                        target,
                        read_only,
                    } => Mount {
                        target: Some(target.clone()),
                        source: Some(source.to_string_lossy().to_string()),
                        typ: Some(MountTypeEnum::BIND),
                        read_only: Some(*read_only),
                        ..Default::default()
                    },
                })
                .collect();

            host_config.mounts = Some(mounts);
        }

        // Configure port bindings
        if let Some(ports) = &service.ports {
            let mut port_bindings = HashMap::new();
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
            host_config.port_bindings = Some(port_bindings);
        }

        Ok(host_config)
    }

    fn create_health_config(health: &HealthCheck) -> HealthConfig {
        HealthConfig {
            test: Some(health.test.clone()),
            interval: health.interval,
            timeout: health.timeout,
            start_period: health.start_period,
            retries: health.retries,
            ..Default::default()
        }
    }

    async fn deploy_service(
        &self,
        service_name: &str,
        service: &Service,
        network_name: &str,
    ) -> Result<String, DockerError> {
        let image = if let Some(build_config) = &service.build {
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
            service.image.clone().ok_or_else(|| {
                DockerError::DockerfileError("No image or build configuration provided".into())
            })?
        };

        let mut container_config = Config {
            image: Some(image),
            cmd: service.command.clone(),
            env: service
                .environment
                .as_ref()
                .map(|env| env.iter().map(|(k, v)| format!("{}={}", k, v)).collect()),
            ..Default::default()
        };

        // Configure host settings
        let host_config = self.create_host_config(service, network_name)?;
        container_config.host_config = Some(host_config);

        // Add health check if specified
        if let Some(health) = &service.healthcheck {
            container_config.healthcheck = Some(Self::create_health_config(health));
        }

        // Create and start container
        let container = self
            .client
            .create_container(
                Some(CreateContainerOptions {
                    name: service_name,
                    platform: None,
                }),
                container_config,
            )
            .await?;

        self.client
            .start_container(&container.id, None::<StartContainerOptions<String>>)
            .await?;

        Ok(container.id)
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

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;

    #[test]
    fn test_service_deployment() {
        let mut config = ComposeConfig::default();
        let service = Service {
            image: Some("nginx:latest".to_string()),
            volumes: Some(vec![crate::VolumeType::Bind {
                source: PathBuf::from("/host/data"),
                target: "/container/data".to_string(),
                read_only: false,
            }]),
            ..Default::default()
        };

        config.services.insert("web".to_string(), service);

        // Add test assertions here
        assert!(config.services.contains_key("web"));
    }

    #[test]
    fn test_memory_string_parsing() {
        assert_eq!(parse_memory_string("512M").unwrap(), 512 * 1024 * 1024);
        assert_eq!(parse_memory_string("1G").unwrap(), 1024 * 1024 * 1024);
        assert!(parse_memory_string("invalid").is_err());
    }
}
