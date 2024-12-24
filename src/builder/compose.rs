use crate::{
    DockerBuilder,
    config::{
        compose::{ComposeConfig, Service},
        health::HealthCheck,
        volume::VolumeType,
    },
    error::DockerError,
    parser::compose::ComposeParser,
};
use bollard::container::{Config, CreateContainerOptions, StartContainerOptions};
use bollard::image::BuildImageOptions;
use bollard::network::CreateNetworkOptions;
use bollard::service::{HealthConfig, HostConfig, Mount, MountTypeEnum, PortBinding};
use futures_util::StreamExt;
use std::collections::HashMap;
use std::path::Path;
use tokio::fs;
use uuid::Uuid;

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
        config: &mut ComposeConfig,
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

        // Collect all volumes from services
        config.collect_volumes();

        // Create volumes defined in the compose file
        for (volume_name, volume_type) in &config.volumes {
            if let VolumeType::Named(_) = volume_type {
                println!("Creating volume: {}", volume_name);
                self.client
                    .create_volume(bollard::volume::CreateVolumeOptions {
                        name: volume_name.to_string(),
                        ..Default::default()
                    })
                    .await
                    .map_err(DockerError::BollardError)?;
            }
        }

        // Collect environment variables
        let mut env_vars = std::env::vars().collect::<HashMap<String, String>>();
        for service in config.services.values() {
            if let Some(service_env) = &service.environment {
                env_vars.extend(service_env.clone());
            }
        }

        // Resolve environment variables
        config.resolve_env(&env_vars);

        let mut container_ids = HashMap::new();

        // Get service deployment order
        let service_order = config.resolve_service_order()?;

        // Deploy services in order
        for service_name in service_order {
            let service = config.services.get(&service_name).unwrap();
            println!("Deploying service: {}", service_name);
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
        if let Some(requirements) = &service.requirements {
            host_config = requirements.to_host_config();
            host_config.network_mode = Some(network_name.to_string());
        }

        // Configure mounts if volumes are specified
        if let Some(volumes) = &service.volumes {
            let mounts: Vec<Mount> = volumes.iter().map(|vol| vol.to_mount()).collect();
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
            test: Some(vec![
                "CMD-SHELL".to_string(),
                format!(
                    "curl -X {} {} -s -f -o /dev/null -w '%{{http_code}}' | grep -q {}",
                    health.method, health.endpoint, health.expected_status
                ),
            ]),
            interval: Some(health.interval.as_nanos() as i64),
            timeout: Some(health.timeout.as_nanos() as i64),
            retries: Some(health.retries as i64),
            start_period: None,
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

        // Pull the image if it doesn't exist
        if let Err(_) = self.client.inspect_image(&image).await {
            println!("Image {} not found locally, pulling...", image);
            let mut pull_stream = self.client.create_image(
                Some(bollard::image::CreateImageOptions {
                    from_image: image.as_str(),
                    ..Default::default()
                }),
                None,
                None,
            );

            while let Some(pull_result) = pull_stream.next().await {
                match pull_result {
                    Ok(_) => continue,
                    Err(e) => return Err(DockerError::BollardError(e)),
                }
            }
        }

        // Create container configuration
        let mut container_config = Config {
            image: Some(image),
            cmd: service.command.clone(),
            env: Self::prepare_environment_variables(service),
            labels: service.labels.clone(),
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

    pub fn prepare_environment_variables(service: &Service) -> Option<Vec<String>> {
        service
            .environment
            .as_ref()
            .map(|env| env.iter().map(|(k, v)| format!("{}={}", k, v)).collect())
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
