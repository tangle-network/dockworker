use super::volume::VolumeType;
use crate::error::DockerError;
use serde::{Deserialize, Serialize};
use std::collections::{HashMap, HashSet};
use std::path::PathBuf;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ResourceLimits {
    pub cpu_limit: Option<f64>,             // Number of CPUs
    pub memory_limit: Option<String>,       // e.g., "1G", "512M"
    pub memory_swap: Option<String>,        // Total memory including swap
    pub memory_reservation: Option<String>, // Soft limit
    pub cpus_shares: Option<i64>,           // CPU shares (relative weight)
    pub cpuset_cpus: Option<String>,        // CPUs in which to allow execution (0-3, 0,1)
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct HealthCheck {
    pub test: Vec<String>, // The test to perform. Possible values: [], ["NONE"], ["CMD", args...], ["CMD-SHELL", command]
    pub interval: Option<i64>, // Time between checks in nanoseconds. 0 or >= 1000000 (1ms). 0 means inherit.
    pub timeout: Option<i64>, // Time to wait before check is hung. 0 or >= 1000000 (1ms). 0 means inherit.
    pub retries: Option<i64>, // Number of consecutive failures before unhealthy. 0 means inherit.
    pub start_period: Option<i64>, // Container init period before retries countdown in ns. 0 or >= 1000000 (1ms). 0 means inherit.
    pub start_interval: Option<i64>, // Time between checks during start period in ns. 0 or >= 1000000 (1ms). 0 means inherit.
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BuildConfig {
    pub context: String,
    pub dockerfile: Option<String>,
}

/// Configuration for a Docker Compose deployment
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ComposeConfig {
    /// Docker Compose file version
    pub version: String,
    /// Map of service name to service configuration
    pub services: HashMap<String, Service>,
}

impl Default for ComposeConfig {
    fn default() -> Self {
        ComposeConfig {
            version: "3.8".to_string(),
            services: HashMap::new(),
        }
    }
}

/// Configuration for a single service in a Docker Compose file
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Service {
    pub image: Option<String>,
    pub build: Option<BuildConfig>,
    pub command: Option<Vec<String>>,
    pub environment: Option<HashMap<String, String>>,
    pub volumes: Option<Vec<VolumeType>>,
    pub depends_on: Option<Vec<String>>,
    pub ports: Option<Vec<String>>,
    pub networks: Option<Vec<String>>,
    pub resources: Option<ResourceLimits>,
    pub healthcheck: Option<HealthCheck>,
    pub restart: Option<String>,
}

impl Default for Service {
    fn default() -> Self {
        Service {
            image: None,
            build: None,
            command: None,
            environment: None,
            volumes: None,
            depends_on: None,
            ports: None,
            networks: None,
            resources: None,
            healthcheck: None,
            restart: None,
        }
    }
}

impl Service {
    /// Normalizes the image name by adding a :latest tag if none is specified
    fn normalize_image(&mut self) {
        if let Some(image) = &self.image {
            if !image.contains(':') {
                self.image = Some(format!("{}:latest", image));
            }
        }
    }

    /// Validates and fixes volume paths in the service configuration
    pub fn fix_volumes(&mut self, base_path: &PathBuf) -> Result<(), DockerError> {
        if let Some(volumes) = &mut self.volumes {
            let mut fixed = Vec::new();
            for volume in volumes.iter() {
                match volume {
                    VolumeType::Named(_) => fixed.push(volume.clone()),
                    VolumeType::Bind {
                        source,
                        target,
                        read_only,
                    } => {
                        let fixed_source = if source.is_relative() {
                            base_path.join(source)
                        } else {
                            source.clone()
                        };
                        fixed.push(VolumeType::Bind {
                            source: fixed_source,
                            target: target.clone(),
                            read_only: *read_only,
                        });
                    }
                }
            }
            *volumes = fixed;
        }
        Ok(())
    }
}

impl ComposeConfig {
    /// Normalizes the configuration by fixing image names and volume paths
    pub fn normalize(&mut self) {
        for service in self.services.values_mut() {
            service.normalize_image();
        }
    }

    pub fn validate_required_env_vars(&self, vars: &[&str]) -> Result<(), DockerError> {
        for (service_name, service) in &self.services {
            if let Some(env) = &service.environment {
                for var in vars {
                    if !env.contains_key(*var) {
                        return Err(DockerError::ValidationError(format!(
                            "Service '{}' is missing required environment variable: {}",
                            service_name, var
                        )));
                    }
                }
            } else if !vars.is_empty() {
                return Err(DockerError::ValidationError(format!(
                    "Service '{}' has no environment variables configured",
                    service_name
                )));
            }
        }
        Ok(())
    }

    pub fn validate_required_volumes(&self, required: &[&str]) -> Result<(), DockerError> {
        for (service_name, service) in &self.services {
            if let Some(volumes) = &service.volumes {
                for required_volume in required {
                    if !volumes.iter().any(|v| match v {
                        VolumeType::Named(name) => {
                            name.split(':').next().unwrap_or("") == *required_volume
                        }
                        VolumeType::Bind { target, .. } => target == required_volume,
                    }) {
                        return Err(DockerError::ValidationError(format!(
                            "Service '{}' is missing required volume: {}",
                            service_name, required_volume
                        )));
                    }
                }
            }
        }
        Ok(())
    }

    pub fn resolve_service_order(&self) -> Result<Vec<String>, DockerError> {
        let mut result = Vec::new();
        let mut visited = HashSet::new();
        let mut temp_visited = HashSet::new();

        // Build dependency graph
        let mut graph: HashMap<&str, Vec<&str>> = HashMap::new();
        for (service_name, service) in &self.services {
            let deps = service
                .depends_on
                .as_ref()
                .map(|deps| deps.iter().map(|s| s.as_str()).collect())
                .unwrap_or_default();
            graph.insert(service_name, deps);
        }

        // Helper function for topological sort
        fn visit(
            node: &str,
            graph: &HashMap<&str, Vec<&str>>,
            visited: &mut HashSet<String>,
            temp_visited: &mut HashSet<String>,
            result: &mut Vec<String>,
        ) -> Result<(), DockerError> {
            if temp_visited.contains(node) {
                return Err(DockerError::ValidationError(
                    "Circular dependency detected".to_string(),
                ));
            }
            if visited.contains(node) {
                return Ok(());
            }

            temp_visited.insert(node.to_string());

            if let Some(deps) = graph.get(node) {
                for &dep in deps {
                    visit(dep, graph, visited, temp_visited, result)?;
                }
            }

            temp_visited.remove(node);
            visited.insert(node.to_string());
            result.push(node.to_string());

            Ok(())
        }

        // Perform topological sort
        for service in self.services.keys().cloned() {
            if !visited.contains(&service) {
                visit(
                    service.as_str(),
                    &graph,
                    &mut visited,
                    &mut temp_visited,
                    &mut result,
                )?;
            }
        }

        Ok(result)
    }

    pub fn fix_relative_paths(&mut self, base_path: &PathBuf) {
        for service in self.services.values_mut() {
            if let Some(volumes) = &mut service.volumes {
                for volume in volumes.iter_mut() {
                    if let VolumeType::Bind {
                        source,
                        target: _,
                        read_only: _,
                    } = volume
                    {
                        if source.starts_with(".") {
                            *source = base_path.join(source.strip_prefix("./").unwrap_or(source));
                        }
                    }
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;

    #[test]
    fn test_compose_volume_parsing() {
        let yaml = r#"
            version: "3.8"
            services:
              test:
                volumes:
                  - type: volume
                    source: named_vol
                    target: /data
                  - type: bind
                    source: ./local
                    target: /container
                  - type: bind
                    source: /abs/path
                    target: /container/config
                    read_only: true
        "#;

        let config: ComposeConfig = serde_yaml::from_str(yaml).unwrap();
        let service = config.services.get("test").unwrap();
        let volumes = service.volumes.as_ref().unwrap();

        assert_eq!(volumes.len(), 3);

        // Check named volume
        assert!(matches!(&volumes[0], VolumeType::Named(name) if name == "named_vol:/data"));

        // Check relative path bind mount
        match &volumes[1] {
            VolumeType::Bind {
                source,
                target,
                read_only,
            } => {
                assert_eq!(source, &PathBuf::from("./local"));
                assert_eq!(target, "/container");
                assert!(!read_only);
            }
            _ => panic!("Expected bind mount"),
        }

        // Check absolute path bind mount with read-only
        match &volumes[2] {
            VolumeType::Bind {
                source,
                target,
                read_only,
            } => {
                assert_eq!(source, &PathBuf::from("/abs/path"));
                assert_eq!(target, "/container/config");
                assert!(*read_only);
            }
            _ => panic!("Expected bind mount"),
        }
    }

    #[test]
    fn test_service_volume_fixing() {
        let mut config = ComposeConfig::default();
        let mut service = Service::default();
        service.volumes = Some(vec![VolumeType::Bind {
            source: PathBuf::from("./data"),
            target: "/container/data".to_string(),
            read_only: false,
        }]);
        config.services.insert("test_service".to_string(), service);

        let base_path = PathBuf::from("/base/path");
        config.fix_relative_paths(&base_path);

        let volumes = config.services["test_service"].volumes.as_ref().unwrap();
        assert!(
            matches!(&volumes[0], VolumeType::Bind { source, target, read_only }
                if source == &base_path.join("data")
                    && target == "/container/data"
                    && !read_only
            )
        );
    }

    #[test]
    fn test_volume_serialization() {
        let volume = VolumeType::Bind {
            source: PathBuf::from("/host"),
            target: "/container".to_string(),
            read_only: true,
        };

        let service = Service {
            volumes: Some(vec![volume]),
            ..Default::default()
        };

        let serialized = serde_yaml::to_string(&service).unwrap();
        let deserialized: Service = serde_yaml::from_str(&serialized).unwrap();

        assert_eq!(service.volumes, deserialized.volumes);
    }

    #[test]
    fn test_volume_validation() {
        let mut config = ComposeConfig {
            version: "3.8".to_string(),
            services: HashMap::new(),
        };

        let service = Service {
            volumes: Some(vec![
                VolumeType::Named("data:/data".to_string()),
                VolumeType::Bind {
                    source: PathBuf::from("/host"),
                    target: "/container".to_string(),
                    read_only: false,
                },
            ]),
            ..Default::default()
        };

        config.services.insert("test".to_string(), service);

        // Test validation
        assert!(config.validate_required_volumes(&["data"]).is_ok());
        assert!(config.validate_required_volumes(&["/container"]).is_ok());
        assert!(config.validate_required_volumes(&["missing"]).is_err());
    }
}
