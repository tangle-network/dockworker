use crate::{
    config::{
        compose::{ComposeConfig, Service},
        health::HealthCheck,
        volume::VolumeType,
    },
    error::DockerError,
    DockerBuilder,
};
use bollard::container::{Config, CreateContainerOptions, StartContainerOptions};
use bollard::network::CreateNetworkOptions;
use bollard::service::{HealthConfig, HostConfig, Mount, PortBinding};
use futures_util::StreamExt;
use std::collections::HashMap;
use std::path::{Path, PathBuf};
use tar;
use tempfile;
use uuid::Uuid;
use walkdir;

impl DockerBuilder {
    /// Deploys a Docker Compose configuration with a custom base directory
    ///
    /// This method deploys services defined in a Docker Compose configuration, using the specified
    /// base directory for resolving relative paths. It handles:
    /// - Creating a dedicated network for the services
    /// - Creating required volumes
    /// - Deploying services in dependency order
    /// - Making bind mount paths absolute
    ///
    /// # Arguments
    ///
    /// * `config` - The Docker Compose configuration to deploy
    /// * `base_dir` - Base directory for resolving relative paths
    ///
    /// # Returns
    ///
    /// Returns a `Result` containing a HashMap mapping service names to their container IDs,
    /// or a `DockerError` if deployment fails
    ///
    /// # Examples
    ///
    /// ```rust,no_run
    /// # use std::path::Path;
    /// # use dockworker::DockerBuilder;
    /// # use dockworker::parser::ComposeParser;
    /// # async fn example() -> Result<(), Box<dyn std::error::Error>> {
    /// let compose_path = "docker-compose.yml";
    ///
    /// let builder = DockerBuilder::new().await?;
    /// let mut config = ComposeParser::new().parse_from_path(compose_path)?;
    /// let container_ids = builder.deploy_compose(&mut config).await?;
    /// # Ok(())
    /// # }
    /// ```
    ///
    /// # Errors
    ///
    /// Returns `DockerError` if:
    /// - Network creation fails
    /// - Volume creation fails
    /// - Container creation or startup fails
    /// - Path resolution fails
    pub async fn deploy_compose(
        &self,
        config: &mut ComposeConfig,
    ) -> Result<HashMap<String, String>, DockerError> {
        self.deploy_compose_with_base_dir(config, std::env::current_dir()?)
            .await
    }

    /// Deploys a Docker Compose configuration
    ///
    /// This method deploys services defined in a Docker Compose configuration using the current
    /// working directory for resolving relative paths. It handles:
    /// - Creating a dedicated network for the services
    /// - Creating required volumes
    /// - Deploying services in dependency order
    /// - Making bind mount paths absolute
    ///
    /// # Arguments
    ///
    /// * `config` - The Docker Compose configuration to deploy
    ///
    /// # Returns
    ///
    /// Returns a `Result` containing a HashMap mapping service names to their container IDs,
    /// or a `DockerError` if deployment fails
    ///
    /// # Examples
    ///
    /// ```rust,no_run
    /// # use dockworker::DockerBuilder;
    /// # use dockworker::parser::ComposeParser;
    /// # async fn example() -> Result<(), Box<dyn std::error::Error>> {
    /// let builder = DockerBuilder::new().await?;
    /// let mut config = ComposeParser::parse(
    ///     r#"
    ///   version: "3"
    ///   services:
    ///     web:
    ///       image: nginx
    /// "#,
    /// )?;
    /// let container_ids = builder.deploy_compose(&mut config).await?;
    /// # Ok(())
    /// # }
    /// ```
    ///
    /// # Errors
    ///
    /// Returns `DockerError` if:
    /// - Network creation fails
    /// - Volume creation fails
    /// - Container creation or startup fails
    /// - Path resolution fails
    pub async fn deploy_compose_with_base_dir(
        &self,
        config: &mut ComposeConfig,
        base_dir: PathBuf,
    ) -> Result<HashMap<String, String>, DockerError> {
        // Make all bind mount paths absolute relative to the base directory
        self.make_bind_paths_absolute(config, Some(base_dir.clone()))?;

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
                self.client
                    .create_volume(bollard::volume::CreateVolumeOptions {
                        name: volume_name.to_string(),
                        ..Default::default()
                    })
                    .await
                    .map_err(DockerError::BollardError)?;
            }
        }

        let mut container_ids = HashMap::new();

        // Get service deployment order
        let service_order = config.resolve_service_order()?;

        // Deploy services in order
        for service_name in service_order {
            let service = config.services.get(&service_name).unwrap();
            let container_id = self
                .deploy_service(&service_name, service, &network_name, &base_dir)
                .await?;
            container_ids.insert(service_name, container_id);
        }

        Ok(container_ids)
    }

    /// Creates a Docker HostConfig for a service
    ///
    /// This method generates the HostConfig needed to create a Docker container for a service.
    /// It handles:
    /// - Network configuration
    /// - Resource limits and requirements
    /// - Volume mounts
    /// - Port bindings
    ///
    /// # Arguments
    ///
    /// * `service` - The service configuration to create a host config for
    /// * `network_name` - Name of the Docker network to connect the container to
    ///
    /// # Returns
    ///
    /// Returns a `Result` containing the `HostConfig` or a `DockerError` if creation fails
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

    /// Creates a Docker HealthConfig from a HealthCheck configuration
    ///
    /// This method converts our internal HealthCheck configuration into the format
    /// expected by the Docker API. It sets up a health check that uses curl to
    /// make HTTP requests and verify the response status code.
    ///
    /// # Arguments
    ///
    /// * `health` - The HealthCheck configuration to convert
    ///
    /// # Returns
    ///
    /// Returns a `HealthConfig` struct configured according to the input parameters
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
            start_interval: None,
        }
    }

    /// Deploys a single service from a Docker Compose configuration
    ///
    /// This method deploys a single service defined in a Docker Compose configuration. It handles:
    /// - Building the image if a build configuration is provided
    /// - Pulling the image if it doesn't exist
    /// - Creating a container with the specified configuration
    /// - Starting the container
    ///
    /// # Arguments
    ///
    /// * `service_name` - Name of the service to deploy
    /// * `service` - The service configuration to deploy
    /// * `network_name` - Name of the Docker network to connect the container to
    /// * `base_dir` - Base directory for resolving relative paths
    ///
    /// # Returns
    ///
    /// Returns a `Result` containing the container ID of the deployed service or a `DockerError` if deployment fails
    async fn deploy_service(
        &self,
        service_name: &str,
        service: &Service,
        network_name: &str,
        base_dir: &Path,
    ) -> Result<String, DockerError> {
        let image = if let Some(build_config) = &service.build {
            // Build the image if build configuration is provided
            let tag = format!("compose_{}", service_name);

            // Make context path absolute and normalized
            let context_path = Self::normalize_path(base_dir, &build_config.context)?;

            // Get the dockerfile path relative to the context
            let dockerfile_path = if let Some(dockerfile) = &build_config.dockerfile {
                context_path.join(dockerfile)
            } else {
                context_path.join("Dockerfile")
            };

            if !dockerfile_path.exists() {
                return Err(DockerError::ValidationError(format!(
                    "Dockerfile not found at path: {}",
                    dockerfile_path.display()
                )));
            }

            // Create a temporary directory for the build context
            let temp_dir = tempfile::tempdir()?;
            let temp_dockerfile = temp_dir.path().join("Dockerfile");

            // Copy the Dockerfile to temp directory
            tokio::fs::copy(&dockerfile_path, &temp_dockerfile).await?;

            // Create tar archive with the Dockerfile and context
            let tar_path = temp_dir.path().join("context.tar");
            let tar_file = std::fs::File::create(&tar_path)?;
            let mut tar_builder = tar::Builder::new(tar_file);

            // Add Dockerfile to tar
            tar_builder.append_path_with_name(&temp_dockerfile, "Dockerfile")?;

            // Add context directory to tar
            for entry in walkdir::WalkDir::new(&context_path)
                .follow_links(true)
                .into_iter()
                .filter_map(|e| e.ok())
            {
                let path = entry.path();
                if path.is_file() {
                    let relative_path = path
                        .strip_prefix(&context_path)
                        .map_err(|e| DockerError::ValidationError(e.to_string()))?;
                    tar_builder.append_path_with_name(path, relative_path)?;
                }
            }
            tar_builder.finish()?;

            // Read the tar file
            let context = tokio::fs::read(&tar_path).await?;

            // Build the image using Bollard API
            let build_opts = bollard::image::BuildImageOptions {
                dockerfile: build_config.dockerfile.as_deref().unwrap_or("Dockerfile"),
                t: &tag,
                q: false,
                ..Default::default()
            };

            let mut build_stream = self
                .client
                .build_image(build_opts, None, Some(context.into()));

            while let Some(build_result) = build_stream.next().await {
                match build_result {
                    Ok(output) => {
                        if let Some(error) = output.error {
                            return Err(DockerError::ValidationError(format!(
                                "Docker build error: {}",
                                error
                            )));
                        }
                        if let Some(stream) = output.stream {
                            print!("{}", stream);
                        }
                    }
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
        if self.client.inspect_image(&image).await.is_err() {
            let mut pull_stream = self.client.create_image(
                Some(bollard::image::CreateImageOptions {
                    from_image: image.as_str(),
                    platform: service.platform.as_deref().unwrap_or("linux/amd64"),
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

    /// Prepares environment variables for a Docker container configuration
    ///
    /// Takes a service configuration and extracts environment variables into the format
    /// required by the Docker API (Vec<String> of "KEY=VALUE" pairs).
    ///
    /// # Arguments
    ///
    /// * `service` - Reference to a Service configuration containing environment variables
    ///
    /// # Returns
    ///
    /// Returns an Option containing a Vec of environment variable strings in "KEY=VALUE" format,
    /// or None if no environment variables are configured.
    ///
    /// # Examples
    ///
    /// ```rust
    /// # use dockworker::{DockerBuilder, config::compose::Service};
    /// # use std::collections::HashMap;
    /// # fn example() {
    /// let mut service = Service::default();
    /// let mut env = HashMap::new();
    /// env.insert("DEBUG".to_string(), "true".to_string());
    /// service.environment = Some(env.into());
    ///
    /// let env_vars = DockerBuilder::prepare_environment_variables(&service);
    /// assert_eq!(env_vars, Some(vec!["DEBUG=true".to_string()]));
    /// # }
    /// ```
    pub fn prepare_environment_variables(service: &Service) -> Option<Vec<String>> {
        service
            .environment
            .as_ref()
            .map(|env| env.iter().map(|(k, v)| format!("{}={}", k, v)).collect())
    }

    fn make_bind_paths_absolute(
        &self,
        config: &mut ComposeConfig,
        base_dir: Option<PathBuf>,
    ) -> Result<(), DockerError> {
        let base = if let Some(dir) = base_dir {
            dir
        } else {
            std::env::current_dir().map_err(|e| {
                DockerError::ValidationError(format!("Failed to get current directory: {}", e))
            })?
        };

        // Ensure base directory is absolute
        let base = if base.is_absolute() {
            base
        } else {
            std::env::current_dir()
                .map_err(|e| {
                    DockerError::ValidationError(format!("Failed to get current directory: {}", e))
                })?
                .join(base)
        };

        for service in config.services.values_mut() {
            if let Some(volumes) = &mut service.volumes {
                for volume in volumes.iter_mut() {
                    if let VolumeType::Bind { source, .. } = volume {
                        let absolute_path = Self::normalize_path(&base, source)?;
                        *source = absolute_path.to_string_lossy().into_owned();
                    }
                }
            }
        }
        Ok(())
    }

    fn normalize_path(base: &Path, path: &str) -> Result<PathBuf, DockerError> {
        let path = PathBuf::from(path);
        if path.is_absolute() {
            Ok(path)
        } else {
            // Remove any ./ or ../ from the path
            let normalized = path.components().fold(PathBuf::new(), |mut acc, comp| {
                match comp {
                    std::path::Component::Normal(x) => acc.push(x),
                    std::path::Component::ParentDir => {
                        acc.pop();
                    }
                    std::path::Component::CurDir => {}
                    _ => acc.push(comp.as_os_str()),
                }
                acc
            });

            // Join with base path and normalize
            Ok(base.join(normalized))
        }
    }
}
