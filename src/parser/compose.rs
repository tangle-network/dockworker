use super::env;
use crate::{config::compose::ComposeConfig, error::DockerError};
use std::collections::HashMap;
use std::fs::File;
use std::io::Read;
use std::path::{Path, PathBuf};

/// Parser for Docker Compose configuration files with environment variable support
///
/// This parser handles:
/// - YAML parsing of Docker Compose files
/// - Environment variable substitution with default values
/// - Loading and parsing of .env files
/// - Path normalization
///
/// # Examples
///
/// ```rust,no_run
/// use dockworker::parser::ComposeParser;
/// use std::path::Path;
///
/// # async fn example() -> Result<(), Box<dyn std::error::Error>> {
/// // Parse a compose file with environment variables from a .env file
/// let compose_path = "docker-compose.yml";
/// let env_path = ".env";
///
/// let config = ComposeParser::new()
///     .env_file(env_path)
///     .parse_from_path(compose_path)
///     .await?;
///
/// // Parse a compose file with explicit environment variables
/// let mut env_vars = std::collections::HashMap::new();
/// env_vars.insert("VERSION".to_string(), "1.0".to_string());
///
/// let config = ComposeParser::new()
///     .env_vars(env_vars)
///     .parse_from_path(compose_path)
///     .await?;
/// # Ok(()) }
/// ```
#[derive(Default, Clone)]
pub struct ComposeParser {
    env_file_path: Option<PathBuf>,
    env_vars: Option<HashMap<String, String>>,
}

impl ComposeParser {
    /// Parses a Docker Compose file from the given path
    ///
    /// # Arguments
    ///
    /// * `path` - Path to the Docker Compose file
    ///
    /// # Examples
    ///
    /// ```rust,no_run
    /// # use std::path::Path;
    /// # use dockworker::parser::ComposeParser;
    /// # async fn example() -> Result<(), Box<dyn std::error::Error>> {
    /// let config = ComposeParser::new()
    ///     .parse_from_path("docker-compose.yml")
    ///     .await?;
    /// # Ok(())
    /// # }
    /// ```
    pub fn parse_from_path<P: AsRef<Path>>(self, path: P) -> Result<ComposeConfig, DockerError> {
        let mut f = File::open(path)?;
        self.parse(&mut f)
    }

    pub fn parse<R>(self, reader: &mut R) -> Result<ComposeConfig, DockerError>
    where
        R: Read,
    {
        let mut env_vars = self.env_vars.unwrap_or_default();

        if let Some(env_file_path) = self.env_file_path {
            let env_content = std::fs::read_to_string(env_file_path).map_err(|e| {
                DockerError::ValidationError(format!("Failed to read env file: {}", e))
            })?;

            env_vars.extend(env::parse_env_file(&env_content)?);
        }

        let mut config_bytes = Vec::new();
        reader.read_to_end(&mut config_bytes)?;

        let compose = String::from_utf8(config_bytes).map_err(|e| {
            DockerError::ValidationError(format!("Failed to read compose file: {}", e))
        })?;
        let processed_content = env::substitute_env_vars(&compose, &env_vars)?;

        let config: ComposeConfig =
            serde_yaml::from_str(&processed_content).map_err(DockerError::YamlError)?;

        // Validate environment variables
        validate_required_env_vars(&config, &env_vars)?;

        Ok(config)
    }
}

impl ComposeParser {
    pub fn new() -> Self {
        Self {
            env_file_path: None,
            env_vars: None,
        }
    }

    /// Set an env file for environment variable substitution
    ///
    /// # Arguments
    ///
    /// * `path` - Path to the environment variables file
    ///
    /// # Examples
    ///
    /// ```
    /// use dockworker::parser::compose::ComposeParser;
    /// use std::fs::write;
    /// use std::path::Path;
    /// use tempfile::NamedTempFile;
    /// # use dockworker::error::DockerError;
    ///
    /// # fn main() -> Result<(), DockerError> {
    /// let compose_content = r#"version: "3"
    /// services:
    ///     app1:
    ///         image: "nginx:${VERSION:-latest}"
    ///         environment:
    ///             PORT: "${PORT}"
    ///             DEBUG: "true"
    ///     app2:
    ///         image: "nginx:${VERSION:-latest}"
    ///         environment:
    ///             - PORT=${PORT}
    ///             - DEBUG=true"#;
    ///
    /// let env_file = NamedTempFile::new()?;
    /// write(env_file.path(), b"VERSION=1.21\nPORT=8080")?;
    ///
    /// let config = ComposeParser::new()
    ///     .env_file(env_file.path())
    ///     .parse(&mut compose_content.as_bytes())?;
    ///
    /// // Test map syntax service
    /// let app1 = config.services.get("app1").unwrap();
    /// assert_eq!(app1.image.as_deref(), Some("nginx:1.21"));
    /// if let Some(env) = &app1.environment {
    ///     assert_eq!(env.get("PORT").map(String::as_str), Some("8080"));
    ///     assert_eq!(env.get("DEBUG").map(String::as_str), Some("true"));
    /// }
    ///
    /// // Test list syntax service
    /// let app2 = config.services.get("app2").unwrap();
    /// assert_eq!(app2.image.as_deref(), Some("nginx:1.21"));
    /// if let Some(env) = &app2.environment {
    ///     assert_eq!(env.get("PORT").map(String::as_str), Some("8080"));
    ///     assert_eq!(env.get("DEBUG").map(String::as_str), Some("true"));
    /// }
    /// # Ok(())
    /// # }
    /// ```
    pub fn env_file<P: AsRef<Path>>(mut self, path: P) -> Self {
        self.env_file_path = Some(path.as_ref().to_path_buf());
        self
    }

    /// Parses a Docker Compose file with environment variables from a HashMap
    ///
    /// This method is useful when you want to provide environment variables
    /// programmatically rather than from a file.
    ///
    /// # Arguments
    ///
    /// * `vars` - HashMap containing environment variable key-value pairs
    ///
    /// # Examples
    ///
    /// ```rust
    /// use dockworker::parser::compose::ComposeParser;
    /// use std::collections::HashMap;
    /// use std::fs::write;
    /// use std::path::Path;
    /// # use dockworker::error::DockerError;
    ///
    /// # fn main() -> Result<(), DockerError> {
    /// let compose_content = r#"version: "3"
    /// services:
    ///     app1:
    ///         image: "nginx:${VERSION:-latest}"
    ///         environment:
    ///             PORT: "${PORT}"
    ///             DEBUG: "true"
    ///     app2:
    ///         image: "nginx:${VERSION:-latest}"
    ///         environment:
    ///             - PORT=${PORT}
    ///             - DEBUG=true"#;
    /// let mut env_vars = HashMap::new();
    /// env_vars.insert(String::from("VERSION"), String::from("1.21"));
    /// env_vars.insert(String::from("PORT"), String::from("8080"));
    ///
    /// let config = ComposeParser::new()
    ///     .env_vars(env_vars)
    ///     .parse(&mut compose_content.as_bytes())?;
    ///
    /// // Test map syntax service
    /// let app1 = config.services.get("app1").unwrap();
    /// assert_eq!(app1.image.as_deref(), Some("nginx:1.21"));
    /// if let Some(env) = &app1.environment {
    ///     assert_eq!(env.get("PORT").map(String::as_str), Some("8080"));
    ///     assert_eq!(env.get("DEBUG").map(String::as_str), Some("true"));
    /// }
    ///
    /// // Test list syntax service
    /// let app2 = config.services.get("app2").unwrap();
    /// assert_eq!(app2.image.as_deref(), Some("nginx:1.21"));
    /// if let Some(env) = &app2.environment {
    ///     assert_eq!(env.get("PORT").map(String::as_str), Some("8080"));
    ///     assert_eq!(env.get("DEBUG").map(String::as_str), Some("true"));
    /// }
    /// # Ok(())
    /// # }
    /// ```
    pub fn env_vars(mut self, vars: HashMap<String, String>) -> Self {
        self.env_vars = Some(vars);
        self
    }
}

/// Validates that all required environment variables are present
fn validate_required_env_vars(
    config: &ComposeConfig,
    env_vars: &HashMap<String, String>,
) -> Result<(), DockerError> {
    let mut required_vars = std::collections::HashSet::new();

    // Collect all required environment variables from the compose file
    for service in config.services.values() {
        if let Some(env) = &service.environment {
            for (key, value) in env {
                if value.contains("${") && !value.contains(":-") {
                    required_vars.insert(key.clone());
                }
            }
        }
    }

    // Check if all required variables are present
    let env_keys: std::collections::HashSet<_> = env_vars.keys().cloned().collect();
    let missing_vars: Vec<_> = required_vars.difference(&env_keys).collect();

    if !missing_vars.is_empty() {
        return Err(DockerError::ValidationError(format!(
            "Missing required environment variables: {:?}",
            missing_vars
        )));
    }

    Ok(())
}

#[cfg(test)]
#[allow(clippy::literal_string_with_formatting_args)]
mod tests {
    use super::*;
    use std::fs;
    use tempfile::NamedTempFile;

    #[test]
    fn test_full_compose_parsing() {
        let compose_content = r#"
        version: "3.8"
        services:
          web:
            image: nginx:${NGINX_VERSION:-latest}
            ports:
              - "${PORT:-80}:80"
        "#;

        let env_content = "NGINX_VERSION=1.21\nPORT=8080";
        let temp_file = NamedTempFile::new().unwrap();
        fs::write(&temp_file, env_content).unwrap();

        let config = ComposeParser::new()
            .env_file(temp_file.path())
            .parse(&mut compose_content.as_bytes())
            .unwrap();

        if let Some(web_service) = config.services.get("web") {
            assert_eq!(web_service.image.as_deref().unwrap(), "nginx:1.21");
            assert_eq!(
                web_service.ports.as_ref().unwrap().first().unwrap(),
                "8080:80"
            );
        } else {
            panic!("Web service not found in parsed config");
        }
    }

    #[test]
    fn test_environment_variable_formats() {
        // Test both map and list formats
        let content = r#"version: "3"
services:
    app1:
        environment:
            KEY1: value1
            KEY2: value2
    app2:
        environment:
            - KEY3=value3
            - KEY4=value4"#;

        let config = ComposeParser::new().parse(&mut content.as_bytes()).unwrap();

        // Check map format
        let app1 = config.services.get("app1").unwrap();
        if let Some(env) = &app1.environment {
            assert_eq!(env.get("KEY1").map(String::as_str), Some("value1"));
            assert_eq!(env.get("KEY2").map(String::as_str), Some("value2"));
        } else {
            panic!("app1 environment should be Some");
        }

        // Check list format
        let app2 = config.services.get("app2").unwrap();
        if let Some(env) = &app2.environment {
            assert_eq!(env.get("KEY3").map(String::as_str), Some("value3"));
            assert_eq!(env.get("KEY4").map(String::as_str), Some("value4"));
        } else {
            panic!("app2 environment should be Some");
        }
    }

    #[test]
    fn test_environment_variable_edge_cases() {
        let content = r#"version: "3"
services:
    app1:
        environment:
            EMPTY: ""
            QUOTED: "quoted value"
            SPACES: "  value with spaces  "
    app2:
        environment:
            - EMPTY=
            - QUOTED="quoted value"
            - SPACES="  value with spaces  ""#;

        let config = ComposeParser::new().parse(&mut content.as_bytes()).unwrap();

        // Test both formats handle edge cases the same way
        for service_name in ["app1", "app2"] {
            let service = config.services.get(service_name).unwrap();
            if let Some(env) = &service.environment {
                assert_eq!(env.get("EMPTY").map(String::as_str), Some(""));
                assert_eq!(env.get("QUOTED").map(String::as_str), Some("quoted value"));
                assert_eq!(
                    env.get("SPACES").map(String::as_str),
                    Some("  value with spaces  ")
                );
            } else {
                panic!("{} environment should be Some", service_name);
            }
        }
    }

    #[test]
    fn test_environment_variable_substitution() {
        let content = r#"version: "3"
services:
    app1:
        image: nginx:${VERSION:-latest}
        environment:
            PORT: "${PORT:-8080}"
            DEBUG: "${DEBUG:-false}""#;

        let mut env_vars = HashMap::new();
        env_vars.insert("VERSION".to_string(), "1.21".to_string());
        env_vars.insert("DEBUG".to_string(), "true".to_string());

        let processed = env::substitute_env_vars(content, &env_vars).unwrap();
        let mut config = ComposeParser::new()
            .parse(&mut processed.as_bytes())
            .unwrap();
        config.resolve_env(&env_vars);

        let app1 = config.services.get("app1").unwrap();
        assert_eq!(app1.image.as_deref(), Some("nginx:1.21"));
        if let Some(env) = &app1.environment {
            assert_eq!(env.get("PORT").map(String::as_str), Some("8080")); // Uses default
            assert_eq!(env.get("DEBUG").map(String::as_str), Some("true")); // Uses env var
        } else {
            panic!("app1 environment should be Some");
        }
    }
}
