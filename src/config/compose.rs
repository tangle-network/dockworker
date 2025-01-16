use super::volume::VolumeType;
use super::EnvironmentVars;
use crate::config::health::HealthCheck;
use crate::config::requirements::SystemRequirements;
use crate::error::DockerError;
use regex::Regex;
use serde::{Deserialize, Serialize};
use std::collections::{HashMap, HashSet};
use std::path::{Path, PathBuf};

/// Configuration for a single service in a Docker Compose file
#[derive(Default, Debug, Clone, Serialize, Deserialize)]
pub struct Service {
    pub image: Option<String>,
    pub build: Option<BuildConfig>,
    #[serde(default)]
    #[serde(deserialize_with = "deserialize_command")]
    pub command: Option<Vec<String>>,
    pub environment: Option<EnvironmentVars>,
    pub env_file: Option<Vec<String>>,
    pub volumes: Option<Vec<VolumeType>>,
    pub depends_on: Option<Vec<String>>,
    pub ports: Option<Vec<String>>,
    pub networks: Option<Vec<String>>,
    pub requirements: Option<SystemRequirements>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub healthcheck: Option<HealthCheck>,
    pub restart: Option<String>,
    pub user: Option<String>,
    #[serde(default)]
    pub labels: Option<HashMap<String, String>>,
    #[serde(default)]
    pub platform: Option<String>,
}

fn deserialize_command<'de, D>(deserializer: D) -> Result<Option<Vec<String>>, D::Error>
where
    D: serde::Deserializer<'de>,
{
    use serde::de::Error;
    let value = serde_yaml::Value::deserialize(deserializer)?;

    match value {
        serde_yaml::Value::String(s) => Ok(Some(vec![s])),
        serde_yaml::Value::Sequence(seq) => {
            let items: Result<Vec<String>, _> = seq
                .into_iter()
                .map(|v| {
                    v.as_str()
                        .map(String::from)
                        .ok_or_else(|| Error::custom("Invalid command item"))
                })
                .collect();
            Ok(Some(items?))
        }
        serde_yaml::Value::Null => Ok(None),
        _ => Err(Error::custom("Invalid command format")),
    }
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
    /// Map of volume name to volume configuration
    #[serde(default)]
    pub volumes: HashMap<String, VolumeType>,
}

impl Default for ComposeConfig {
    fn default() -> Self {
        ComposeConfig {
            version: "3".to_string(),
            services: HashMap::new(),
            volumes: HashMap::new(),
        }
    }
}

impl ComposeConfig {
    /// Validates that required environment variables are present
    pub fn validate_required_env_vars(&self, vars: &[&str]) -> Result<(), DockerError> {
        for (service_name, service) in &self.services {
            if let Some(env) = &service.environment {
                for var in vars {
                    if !env.contains_key(var) {
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

    /// Validates that required volumes are present
    pub fn validate_required_volumes(&self, required: &[&str]) -> Result<(), DockerError> {
        for (service_name, service) in &self.services {
            if let Some(volumes) = &service.volumes {
                for required_volume in required {
                    if !volumes.iter().any(|v| v.matches_name(required_volume)) {
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

    /// Resolves the order in which services should be deployed based on dependencies
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
        for service in self.services.keys() {
            if !visited.contains(service) {
                visit(
                    service,
                    &graph,
                    &mut visited,
                    &mut temp_visited,
                    &mut result,
                )?;
            }
        }

        Ok(result)
    }

    /// Collects all volumes used in services and adds them to the volumes section
    pub fn collect_volumes(&mut self) {
        let mut used_volumes = HashMap::new();

        // Collect all named volumes from services
        for service in self.services.values() {
            if let Some(volumes) = &service.volumes {
                for volume in volumes {
                    if let VolumeType::Named(name) = volume {
                        let volume_name = name.split(':').next().unwrap_or(name).to_string();
                        if !self.volumes.contains_key(&volume_name) {
                            used_volumes.insert(volume_name, volume.clone());
                        }
                    }
                }
            }
        }

        // Add missing volumes to the volumes section
        self.volumes.extend(used_volumes);
    }

    /// Resolves environment variables in the configuration
    pub fn resolve_env(&mut self, env_vars: &HashMap<String, String>) {
        // Helper function to resolve env vars in a volume
        fn resolve_volume(volume: &mut VolumeType, env_vars: &HashMap<String, String>) {
            match volume {
                VolumeType::Named(name) => {
                    *name = ComposeConfig::resolve_env_value(name, env_vars);
                }
                VolumeType::Bind { source, target, .. } => {
                    *source = ComposeConfig::resolve_env_value(source, env_vars);
                    *target = ComposeConfig::resolve_env_value(target, env_vars);
                }
                VolumeType::Config {
                    name,
                    driver,
                    driver_opts,
                } => {
                    *name = ComposeConfig::resolve_env_value(name, env_vars);
                    if let Some(d) = driver {
                        *d = ComposeConfig::resolve_env_value(d, env_vars);
                    }
                    if let Some(opts) = driver_opts {
                        for value in opts.values_mut() {
                            *value = ComposeConfig::resolve_env_value(value, env_vars);
                        }
                    }
                }
            }
        }

        // Resolve environment variables in services
        for service in self.services.values_mut() {
            // Resolve service environment
            if let Some(environment) = &mut service.environment {
                for value in environment.values_mut() {
                    *value = Self::resolve_env_value(value, env_vars);
                }
            }

            // Resolve service volumes
            if let Some(volumes) = &mut service.volumes {
                for volume in volumes.iter_mut() {
                    resolve_volume(volume, env_vars);
                }
            }
        }

        // Resolve environment variables in volume configurations
        for volume in self.volumes.values_mut() {
            resolve_volume(volume, env_vars);
        }
    }

    fn resolve_env_value(value: &str, env_vars: &HashMap<String, String>) -> String {
        let re = Regex::new(r"\$\{([^}]+)}|\$([a-zA-Z_][a-zA-Z0-9_]*)").unwrap();
        let mut result = value.to_string();

        for cap in re.captures_iter(value) {
            let full_match = cap.get(0).unwrap();
            let var_name = cap
                .get(1)
                .or_else(|| cap.get(2))
                .map(|m| m.as_str())
                .unwrap_or("");

            let (name, default) = if let Some((n, d)) = var_name.split_once(":-") {
                (n, Some(d))
            } else {
                (var_name, None)
            };

            let replacement = env_vars
                .get(name)
                .cloned()
                .or_else(|| std::env::var(name).ok())
                .or_else(|| default.map(|d| d.to_string()))
                .unwrap_or_default();

            result = result.replace(full_match.as_str(), &replacement);
        }
        result
    }

    /// Loads and processes all environment variables from:
    /// - System environment (lowest priority)
    /// - Environment files in order:
    ///   1. Common env files (e.g., envs/common/*.env)
    ///   2. Network-specific env files (e.g., envs/op-mainnet/*.env)
    ///   3. Service-specific env files (from env_file directive)
    /// - Service-specific environment variables (highest priority)
    pub fn process_environment(
        &mut self,
        base_dir: &Path,
    ) -> Result<HashMap<String, String>, DockerError> {
        // Start with system environment variables
        let mut env_vars = std::env::vars().collect::<HashMap<String, String>>();
        println!("Loaded system environment variables");

        // First pass: load common env files
        let common_env_dir = base_dir.join("envs").join("common");
        if common_env_dir.exists() {
            let entries = std::fs::read_dir(&common_env_dir).map_err(|e| {
                DockerError::ValidationError(format!("Failed to read common env directory: {}", e))
            })?;
            for entry in entries {
                let entry = entry.map_err(|e| {
                    DockerError::ValidationError(format!("Failed to read directory entry: {}", e))
                })?;
                if entry.path().extension().and_then(|s| s.to_str()) == Some("env") {
                    println!("Loading common env file: {}", entry.path().display());
                    match std::fs::read_to_string(entry.path()) {
                        Ok(content) => {
                            let file_vars =
                                crate::parser::compose::ComposeParser::parse_env_file(&content)?;
                            for (key, value) in file_vars {
                                env_vars.entry(key).or_insert(value);
                            }
                        }
                        Err(e) => {
                            println!(
                                "Warning: Failed to read env file {}: {}",
                                entry.path().display(),
                                e
                            );
                        }
                    }
                }
            }
        }

        // Second pass: resolve and load service env_files
        for service in self.services.values() {
            if let Some(env_files) = &service.env_file {
                for env_file in env_files {
                    // Resolve variables in the env file path
                    let resolved_path = Self::resolve_env_value(env_file, &env_vars);

                    // Make path absolute
                    let env_path = if Path::new(&resolved_path).is_absolute() {
                        PathBuf::from(resolved_path)
                    } else {
                        base_dir.join(resolved_path)
                    };

                    println!("Loading service env file: {}", env_path.display());

                    // Read and parse env file if it exists
                    match std::fs::read_to_string(&env_path) {
                        Ok(content) => {
                            let file_vars =
                                crate::parser::compose::ComposeParser::parse_env_file(&content)?;
                            env_vars.extend(file_vars);
                        }
                        Err(e) => {
                            println!(
                                "Warning: Failed to read env file {}: {}",
                                env_path.display(),
                                e
                            );
                            // Only fail if the file is required (no default value in the path)
                            if env_file.contains("${") && !env_file.contains(":-") {
                                return Err(DockerError::ValidationError(format!(
                                    "Required env file {} not found: {}",
                                    env_path.display(),
                                    e
                                )));
                            }
                        }
                    }
                }
            }
        }

        // Final pass: add service-specific environment variables
        for service in self.services.values() {
            if let Some(service_env) = &service.environment {
                // Service environment variables take highest precedence
                env_vars.extend(service_env.iter().map(|(k, v)| (k.clone(), v.clone())));
            }
        }

        // Validate that all required variables are present
        self.validate_environment(&env_vars)?;

        // Resolve all variables in the config using the complete env_vars
        self.resolve_env(&env_vars);

        Ok(env_vars)
    }

    /// Validates that all required environment variables are present
    fn validate_environment(&self, env_vars: &HashMap<String, String>) -> Result<(), DockerError> {
        for (service_name, service) in &self.services {
            // Check env_file paths can be resolved
            if let Some(env_files) = &service.env_file {
                for env_file in env_files {
                    if env_file.contains("${") && !env_file.contains(":-") {
                        let var_name = env_file
                            .split("${")
                            .nth(1)
                            .and_then(|s| s.split("}").next())
                            .ok_or_else(|| {
                                DockerError::ValidationError(format!(
                                    "Invalid environment variable syntax in env_file path: {}",
                                    env_file
                                ))
                            })?;
                        if !env_vars.contains_key(var_name) {
                            return Err(DockerError::ValidationError(format!(
                                "Service '{}' is missing required environment variable '{}' for env_file path",
                                service_name, var_name
                            )));
                        }
                    }
                }
            }

            // Check service environment variables
            if let Some(env) = &service.environment {
                for (key, value) in env.iter() {
                    if value.contains("${") && !value.contains(":-") && !env_vars.contains_key(key)
                    {
                        return Err(DockerError::ValidationError(format!(
                            "Service '{}' is missing required environment variable: {}",
                            service_name, key
                        )));
                    }
                }
            }
        }
        Ok(())
    }
}
